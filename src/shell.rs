use crate::commands;
use crate::{console, cprint, cprintln, print, serial_println};
use core::sync::atomic::{compiler_fence, AtomicBool, Ordering};
use lazy_static::lazy_static;
use pc_keyboard::DecodedKey;
use spin::Mutex;

const MAX_PATH: usize = 64;
const MAX_CMD: usize = 64;
const MAX_HISTORY: usize = 16;

pub struct Session {
    pub cwd: usize,
    pub path: [u8; MAX_PATH],
    pub plen: usize,
}

pub struct HistoryEntry {
    pub buf: [u8; MAX_CMD],
    pub len: usize,
}

impl HistoryEntry {
    const fn empty() -> Self {
        Self {
            buf: [0; MAX_CMD],
            len: 0,
        }
    }
}

pub struct Shell {
    pub buf: [u8; MAX_CMD],
    pub len: usize,
    pub cursor: usize,
    pub prompt_end_x: usize,
    pub history: [HistoryEntry; MAX_HISTORY],
    pub history_count: usize,
    pub history_pos: usize,
    pub browsing: bool,
    pub saved_buf: [u8; MAX_CMD],
    pub saved_len: usize,
}

static CMD_READY: AtomicBool = AtomicBool::new(false);
static mut PENDING_BUF: [u8; MAX_CMD] = [0; MAX_CMD];
static mut PENDING_LEN: usize = 0;

lazy_static! {
    pub static ref SESSION: Mutex<Session> = Mutex::new(Session {
        cwd: 0,
        path: {
            let mut p = [0; MAX_PATH];
            p[0] = b'/';
            p
        },
        plen: 1,
    });
    pub static ref SHELL: Mutex<Shell> = Mutex::new(Shell {
        buf: [0; MAX_CMD],
        len: 0,
        cursor: 0,
        prompt_end_x: 0,
        history: [const { HistoryEntry::empty() }; MAX_HISTORY],
        history_count: 0,
        history_pos: 0,
        browsing: false,
        saved_buf: [0; MAX_CMD],
        saved_len: 0,
    });
}

pub fn init() {
    serial_println!("[shell] init");
    cprintln!(57, 197, 187, "MikuOS v0.0.8");
    prompt();
}

pub fn process_pending() {
    if !CMD_READY.load(Ordering::SeqCst) {
        return;
    }

    let mut cmd_buf = [0u8; MAX_CMD];
    let cmd_len;

    unsafe {
        compiler_fence(Ordering::Acquire);
        cmd_len = PENDING_LEN;
        cmd_buf[..cmd_len].copy_from_slice(&PENDING_BUF[..cmd_len]);
        PENDING_LEN = 0;
    }

    CMD_READY.store(false, Ordering::SeqCst);

    let s = unsafe { core::str::from_utf8_unchecked(&cmd_buf[..cmd_len]) };
    serial_println!("[shell] exec: '{}'", s);

    commands::execute(s);

    serial_println!("[shell] exec done");
    prompt();
}

fn prompt() {
    let s = SESSION.lock();
    let p = unsafe { core::str::from_utf8_unchecked(&s.path[..s.plen]) };
    print!("\n");
    cprint!(100, 160, 255, "miku");
    cprint!(150, 160, 170, "@");
    cprint!(255, 255, 255, "os");
    cprint!(150, 160, 170, ":");
    cprint!(57, 197, 187, "{}", p);
    cprint!(255, 255, 255, " $ ");
    drop(s);
    let mut sh = SHELL.lock();
    sh.prompt_end_x = console::get_x();
    drop(sh);
    draw_shell_cursor();
}

fn cursor_x_pos(sh: &Shell) -> usize {
    sh.prompt_end_x + sh.cursor * console::CHAR_WIDTH
}

fn draw_shell_cursor() {
    let sh = SHELL.lock();
    let x = cursor_x_pos(&sh);
    drop(sh);
    console::draw_cursor(x);
}

fn redraw_input(sh: &Shell) {
    let start_x = sh.prompt_end_x;
    console::clear_from_x(start_x, sh.len + 2);
    console::set_x(start_x);
    for i in 0..sh.len {
        let c = sh.buf[i] as char;
        print!("{}", c);
    }
    console::set_x(start_x + sh.len * console::CHAR_WIDTH);
}

pub fn handle_keypress(key: DecodedKey) {
    let mut sh = SHELL.lock();
    match key {
        DecodedKey::Unicode(c) => match c {
            '\n' => {
                erase_cursor_inner(&sh);
                let cl = sh.len;

                if cl > 0 {
                    let mut tmp = [0u8; MAX_CMD];
                    tmp[..cl].copy_from_slice(&sh.buf[..cl]);

                    let idx = sh.history_count % MAX_HISTORY;
                    sh.history[idx].buf[..cl].copy_from_slice(&tmp[..cl]);
                    sh.history[idx].len = cl;
                    sh.history_count += 1;

                    unsafe {
                        PENDING_BUF[..cl].copy_from_slice(&tmp[..cl]);
                        PENDING_LEN = cl;
                        compiler_fence(Ordering::Release);
                    }
                }

                sh.len = 0;
                sh.cursor = 0;
                sh.browsing = false;
                drop(sh);

                print!("\n");

                if cl > 0 {
                    CMD_READY.store(true, Ordering::SeqCst);
                } else {
                    prompt();
                }
            }
            '\u{8}' => {
                if sh.cursor > 0 {
                    erase_cursor_inner(&sh);
                    let pos = sh.cursor - 1;
                    for i in pos..sh.len - 1 {
                        sh.buf[i] = sh.buf[i + 1];
                    }
                    sh.len -= 1;
                    sh.cursor -= 1;
                    redraw_input(&sh);
                    draw_cursor_inner(&sh);
                }
            }
            '\x03' => {
                crate::net::CTRL_C.store(true, core::sync::atomic::Ordering::SeqCst);
                crate::println!("^C");
            }
            _ => {
                if sh.len < MAX_CMD {
                    let b = c as u8;
                    if b >= 0x20 && b <= 0x7E {
                        sh.browsing = false;
                        erase_cursor_inner(&sh);

                        let cur = sh.cursor;
                        if cur < sh.len {
                            let mut i = sh.len;
                            while i > cur {
                                sh.buf[i] = sh.buf[i - 1];
                                i -= 1;
                            }
                        }

                        sh.buf[cur] = b;
                        sh.len += 1;
                        sh.cursor += 1;

                        redraw_input(&sh);
                        draw_cursor_inner(&sh);
                    }
                }
            }
        },
        DecodedKey::RawKey(key) => {
            use pc_keyboard::KeyCode;
            match key {
                KeyCode::ArrowLeft => {
                    if sh.cursor > 0 {
                        erase_cursor_inner(&sh);
                        sh.cursor -= 1;
                        draw_cursor_inner(&sh);
                    }
                }
                KeyCode::ArrowRight => {
                    if sh.cursor < sh.len {
                        erase_cursor_inner(&sh);
                        sh.cursor += 1;
                        draw_cursor_inner(&sh);
                    }
                }
                KeyCode::Home => {
                    erase_cursor_inner(&sh);
                    sh.cursor = 0;
                    draw_cursor_inner(&sh);
                }
                KeyCode::End => {
                    erase_cursor_inner(&sh);
                    sh.cursor = sh.len;
                    draw_cursor_inner(&sh);
                }
                KeyCode::Delete => {
                    if sh.cursor < sh.len {
                        erase_cursor_inner(&sh);
                        let pos = sh.cursor;
                        for i in pos..sh.len - 1 {
                            sh.buf[i] = sh.buf[i + 1];
                        }
                        sh.len -= 1;
                        redraw_input(&sh);
                        draw_cursor_inner(&sh);
                    }
                }
                KeyCode::ArrowUp => {
                    if sh.history_count == 0 {
                        return;
                    }

                    if !sh.browsing {
                        let len = sh.len;
                        let mut tmp = [0u8; MAX_CMD];
                        tmp[..len].copy_from_slice(&sh.buf[..len]);
                        sh.saved_buf[..len].copy_from_slice(&tmp[..len]);
                        sh.saved_len = len;
                        sh.browsing = true;
                        sh.history_pos = sh.history_count;
                    }

                    if sh.history_pos > 0 {
                        let start = if sh.history_count > MAX_HISTORY {
                            sh.history_count - MAX_HISTORY
                        } else {
                            0
                        };
                        if sh.history_pos > start {
                            erase_cursor_inner(&sh);
                            sh.history_pos -= 1;
                            let idx = sh.history_pos % MAX_HISTORY;
                            let hlen = sh.history[idx].len;
                            let mut tmp = [0u8; MAX_CMD];
                            tmp[..hlen].copy_from_slice(&sh.history[idx].buf[..hlen]);
                            sh.buf[..hlen].copy_from_slice(&tmp[..hlen]);
                            sh.len = hlen;
                            sh.cursor = hlen;
                            redraw_input(&sh);
                            draw_cursor_inner(&sh);
                        }
                    }
                }
                KeyCode::ArrowDown => {
                    if !sh.browsing {
                        return;
                    }

                    erase_cursor_inner(&sh);

                    if sh.history_pos < sh.history_count - 1 {
                        sh.history_pos += 1;
                        let idx = sh.history_pos % MAX_HISTORY;
                        let hlen = sh.history[idx].len;
                        let mut tmp = [0u8; MAX_CMD];
                        tmp[..hlen].copy_from_slice(&sh.history[idx].buf[..hlen]);
                        sh.buf[..hlen].copy_from_slice(&tmp[..hlen]);
                        sh.len = hlen;
                        sh.cursor = hlen;
                    } else if sh.history_pos == sh.history_count - 1 {
                        sh.history_pos = sh.history_count;
                        let slen = sh.saved_len;
                        let mut tmp = [0u8; MAX_CMD];
                        tmp[..slen].copy_from_slice(&sh.saved_buf[..slen]);
                        sh.buf[..slen].copy_from_slice(&tmp[..slen]);
                        sh.len = slen;
                        sh.cursor = slen;
                        sh.browsing = false;
                    }

                    redraw_input(&sh);
                    draw_cursor_inner(&sh);
                }
                _ => {}
            }
        }
    }
}

fn draw_cursor_inner(sh: &Shell) {
    let x = cursor_x_pos(sh);
    console::draw_cursor(x);
}

fn erase_cursor_inner(sh: &Shell) {
    let x = cursor_x_pos(sh);
    console::erase_cursor(x);
    let start_x = sh.prompt_end_x;
    console::set_x(start_x + sh.cursor * console::CHAR_WIDTH);
    if sh.cursor < sh.len {
        let c = sh.buf[sh.cursor] as char;
        print!("{}", c);
    }
    console::set_x(start_x + sh.cursor * console::CHAR_WIDTH);
}

pub fn update_path(s: &mut Session, arg: &str) {
    if arg.is_empty() {
        return;
    }

    if arg.starts_with('/') {
        s.path[0] = b'/';
        s.plen = 1;
        for component in arg.split('/') {
            if component.is_empty() || component == "." {
                continue;
            }
            if component == ".." {
                if s.plen > 1 {
                    let mut nl = s.plen - 1;
                    while nl > 0 && s.path[nl] != b'/' {
                        nl -= 1;
                    }
                    s.plen = if nl == 0 { 1 } else { nl };
                }
                continue;
            }
            append_component(s, component);
        }
        return;
    }

    for component in arg.split('/') {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." {
            if s.plen > 1 {
                let mut nl = s.plen - 1;
                while nl > 0 && s.path[nl] != b'/' {
                    nl -= 1;
                }
                s.plen = if nl == 0 { 1 } else { nl };
            }
            continue;
        }
        append_component(s, component);
    }
}

fn append_component(s: &mut Session, name: &str) {
    if s.plen == 0 {
        return;
    }
    if s.plen > 1 && s.plen < MAX_PATH {
        s.path[s.plen] = b'/';
        s.plen += 1;
    }
    for &b in name.as_bytes() {
        if s.plen < MAX_PATH {
            s.path[s.plen] = b;
            s.plen += 1;
        }
    }
}
