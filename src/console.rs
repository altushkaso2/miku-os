extern crate alloc;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::instructions::interrupts;

lazy_static! {
    pub static ref WRITER: Mutex<Option<Console>> = Mutex::new(None);
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::console::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    interrupts::without_interrupts(|| {
        if let Some(writer) = WRITER.lock().as_mut() {
            writer.write_fmt(args).unwrap();
        }
    });
}

pub fn print_colored(r: u8, g: u8, b: u8, args: fmt::Arguments) {
    use core::fmt::Write;
    interrupts::without_interrupts(|| {
        if let Some(writer) = WRITER.lock().as_mut() {
            let saved = writer.fg_color;
            writer.fg_color = [r, g, b];
            writer.write_fmt(args).unwrap();
            writer.fg_color = saved;
        }
    });
}

pub fn backspace() {
    interrupts::without_interrupts(|| {
        if let Some(writer) = WRITER.lock().as_mut() {
            writer.backspace();
        }
    });
}

pub fn clear_screen() {
    interrupts::without_interrupts(|| {
        if let Some(writer) = WRITER.lock().as_mut() {
            writer.clear();
        }
    });
}

pub fn set_color(r: u8, g: u8, b: u8) {
    interrupts::without_interrupts(|| {
        if let Some(writer) = WRITER.lock().as_mut() {
            writer.fg_color = [r, g, b];
        }
    });
}

pub fn reset_color() {
    interrupts::without_interrupts(|| {
        if let Some(writer) = WRITER.lock().as_mut() {
            writer.fg_color = COLOR_MIKU;
        }
    });
}

pub fn move_cursor_left() {
    interrupts::without_interrupts(|| {
        if let Some(writer) = WRITER.lock().as_mut() {
            if writer.x_pos > BORDER_PADDING {
                writer.x_pos -= CHAR_WIDTH;
            }
        }
    });
}

pub fn move_cursor_right() {
    interrupts::without_interrupts(|| {
        if let Some(writer) = WRITER.lock().as_mut() {
            writer.x_pos += CHAR_WIDTH;
        }
    });
}

pub fn get_x() -> usize {
    interrupts::without_interrupts(|| {
        if let Some(writer) = WRITER.lock().as_ref() {
            writer.x_pos
        } else {
            0
        }
    })
}

pub fn set_x(x: usize) {
    interrupts::without_interrupts(|| {
        if let Some(writer) = WRITER.lock().as_mut() {
            writer.x_pos = x;
            writer.cur_col = x.saturating_sub(BORDER_PADDING) / CHAR_WIDTH;
        }
    });
}

pub fn draw_cursor(x: usize) {
    interrupts::without_interrupts(|| {
        if let Some(writer) = WRITER.lock().as_mut() {
            for y in 1..CHAR_HEIGHT - 1 {
                writer.write_pixel_direct(x,     writer.y_pos + y, 200, 220, 220);
                writer.write_pixel_direct(x + 1, writer.y_pos + y, 200, 220, 220);
            }
        }
    });
}

pub fn erase_cursor(x: usize) {
    interrupts::without_interrupts(|| {
        if let Some(writer) = WRITER.lock().as_mut() {
            for y in 1..CHAR_HEIGHT - 1 {
                writer.write_pixel_direct(x,     writer.y_pos + y, 0, 0, 0);
                writer.write_pixel_direct(x + 1, writer.y_pos + y, 0, 0, 0);
            }
        }
    });
}

pub fn clear_from_x(start_x: usize, count: usize) {
    interrupts::without_interrupts(|| {
        if let Some(writer) = WRITER.lock().as_mut() {
            let start_col = start_x.saturating_sub(BORDER_PADDING) / CHAR_WIDTH;
            for i in 0..count {
                let cx = start_x + i * CHAR_WIDTH;
                for y in 0..CHAR_HEIGHT {
                    for x in 0..CHAR_WIDTH {
                        writer.write_pixel_direct(cx + x, writer.y_pos + y, 0, 0, 0);
                    }
                }
                let col = start_col + i;
                if col < writer.cols {
                    writer.cells[writer.cur_row * writer.cols + col] = Cell::blank();
                }
            }
        }
    });
}

pub fn hide_cursor() {}
pub fn show_cursor() {}

pub fn clear_char() {
    interrupts::without_interrupts(|| {
        if let Some(writer) = WRITER.lock().as_mut() {
            writer.clear_char_at_cursor();
        }
    });
}

pub const COLOR_MIKU: [u8; 3]       = [57,  197, 187];
pub const COLOR_MIKU_DARK: [u8; 3]  = [0,   150, 136];
pub const COLOR_MIKU_LIGHT: [u8; 3] = [128, 222, 217];
pub const COLOR_PINK: [u8; 3]       = [255, 105, 140];
pub const COLOR_WHITE: [u8; 3]      = [230, 240, 240];
pub const COLOR_GRAY: [u8; 3]       = [120, 140, 140];
pub const COLOR_GREEN: [u8; 3]      = [100, 220, 150];
pub const COLOR_YELLOW: [u8; 3]     = [220, 220, 100];
pub const COLOR_CYAN: [u8; 3]       = [0,   220, 220];

pub const BORDER_PADDING: usize = 10;
pub const CHAR_WIDTH: usize     = 9;
const LINE_SPACING: usize       = 2;
const CHAR_HEIGHT: usize        = 16;
const LINE_HEIGHT: usize        = CHAR_HEIGHT + LINE_SPACING;

const MAX_COLS: usize = 160;
const MAX_ROWS: usize = 60;

#[derive(Clone, Copy)]
struct Cell {
    ch: u8,
    r:  u8,
    g:  u8,
    b:  u8,
}

impl Cell {
    const fn blank() -> Self {
        Self { ch: b' ', r: 0, g: 0, b: 0 }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FrameBufferConfig {
    pub width:           usize,
    pub height:          usize,
    pub stride:          usize,
    pub bytes_per_pixel: usize,
    pub is_bgr:          bool,
}

pub struct Console {
    framebuffer:      &'static mut [u8],
    cells:            Vec<Cell>,
    cols:             usize,
    rows:             usize,
    cur_col:          usize,
    cur_row:          usize,
    width:            usize,
    height:           usize,
    stride:           usize,
    bytes_per_pixel:  usize,
    pub x_pos:        usize,
    pub y_pos:        usize,
    pub fg_color:     [u8; 3],
    pub is_bgr:       bool,
}

impl Console {
    pub fn new_limine(framebuffer: &'static mut [u8], config: FrameBufferConfig) -> Self {
        let cols = ((config.width.saturating_sub(BORDER_PADDING)) / CHAR_WIDTH).min(MAX_COLS);
        let rows = ((config.height.saturating_sub(BORDER_PADDING)) / LINE_HEIGHT).min(MAX_ROWS);
        let cells = vec![Cell::blank(); cols * rows];
        let fill_end = (config.height * config.stride * config.bytes_per_pixel)
            .min(framebuffer.len());
        framebuffer[..fill_end].fill(0);
        Self {
            framebuffer,
            cells,
            cols,
            rows,
            cur_col: 0,
            cur_row: 0,
            width:           config.width,
            height:          config.height,
            stride:          config.stride,
            bytes_per_pixel: config.bytes_per_pixel,
            x_pos:   BORDER_PADDING,
            y_pos:   BORDER_PADDING,
            fg_color: COLOR_MIKU,
            is_bgr:   config.is_bgr,
        }
    }

    pub fn clear(&mut self) {
        for c in self.cells.iter_mut() { *c = Cell::blank(); }
        let fill_end = (self.height * self.stride * self.bytes_per_pixel)
            .min(self.framebuffer.len());
        self.framebuffer[..fill_end].fill(0);
        self.x_pos   = BORDER_PADDING;
        self.y_pos   = BORDER_PADDING;
        self.cur_col = 0;
        self.cur_row = 0;
    }

    fn new_line(&mut self) {
        self.cur_col = 0;
        self.cur_row += 1;
        self.x_pos = BORDER_PADDING;
        self.y_pos += LINE_HEIGHT;
        if self.cur_row >= self.rows {
            self.scroll_up();
        }
    }

    fn scroll_up(&mut self) {
        let cols = self.cols;
        let rows = self.rows;

        for row in 0..rows - 1 {
            for col in 0..cols {
                self.cells[row * cols + col] = self.cells[(row + 1) * cols + col];
            }
        }
        for col in 0..cols {
            self.cells[(rows - 1) * cols + col] = Cell::blank();
        }

        self.cur_row = rows - 1;
        self.y_pos  = BORDER_PADDING + self.cur_row * LINE_HEIGHT;

        let fill_end = (self.height * self.stride * self.bytes_per_pixel)
            .min(self.framebuffer.len());
        self.framebuffer[..fill_end].fill(0);

        for row in 0..rows {
            for col in 0..cols {
                let cell = self.cells[row * cols + col];
                if cell.ch > b' ' {
                    let px = BORDER_PADDING + col * CHAR_WIDTH;
                    let py = BORDER_PADDING + row * LINE_HEIGHT;
                    self.render_char_at(cell.ch as char, px, py, cell.r, cell.g, cell.b);
                }
            }
        }
    }

    pub fn backspace(&mut self) {
        if self.x_pos > BORDER_PADDING {
            self.x_pos -= CHAR_WIDTH;
            if self.cur_col > 0 { self.cur_col -= 1; }
            self.clear_char_at_cursor();
            if self.cur_col < self.cols && self.cur_row < self.rows {
                self.cells[self.cur_row * self.cols + self.cur_col] = Cell::blank();
            }
        }
    }

    pub fn clear_char_at_cursor(&mut self) {
        for y in 0..CHAR_HEIGHT {
            for x in 0..CHAR_WIDTH {
                self.write_pixel_direct(self.x_pos + x, self.y_pos + y, 0, 0, 0);
            }
        }
    }

    fn write_char(&mut self, c: char) {
        match c {
            '\n' => self.new_line(),
            '\x08' => self.backspace(),
            ch => {
                if self.x_pos + CHAR_WIDTH >= self.width {
                    self.new_line();
                }
                self.clear_char_at_cursor();

                if self.cur_col < self.cols && self.cur_row < self.rows {
                    let [r, g, b] = self.fg_color;
                    self.cells[self.cur_row * self.cols + self.cur_col] =
                        Cell { ch: ch as u8, r, g, b };
                }

                if let Some((glyph, _)) = crate::font::get_glyph(ch) {
                    self.write_glyph(glyph);
                } else if let Some(raster) = noto_sans_mono_bitmap::get_raster(
                    ch,
                    noto_sans_mono_bitmap::FontWeight::Regular,
                    noto_sans_mono_bitmap::RasterHeight::Size16,
                ) {
                    self.write_rendered_char(raster);
                }

                self.cur_col += 1;
            }
        }
    }

    fn render_char_at(&mut self, c: char, px: usize, py: usize, r: u8, g: u8, b: u8) {
        if let Some((glyph, _)) = crate::font::get_glyph(c) {
            for y in 0..CHAR_HEIGHT {
                for x in 0..CHAR_WIDTH {
                    let intensity = glyph[y * CHAR_WIDTH + x] as u16;
                    if intensity > 0 {
                        let pr = ((r as u16 * intensity) / 255) as u8;
                        let pg = ((g as u16 * intensity) / 255) as u8;
                        let pb = ((b as u16 * intensity) / 255) as u8;
                        self.write_pixel_direct(px + x, py + y, pr, pg, pb);
                    }
                }
            }
        } else if let Some(raster) = noto_sans_mono_bitmap::get_raster(
            c,
            noto_sans_mono_bitmap::FontWeight::Regular,
            noto_sans_mono_bitmap::RasterHeight::Size16,
        ) {
            for (y, row) in raster.raster().iter().enumerate() {
                for (x, byte) in row.iter().enumerate() {
                    if *byte > 0 {
                        let intensity = *byte as u16;
                        let pr = ((r as u16 * intensity) / 255) as u8;
                        let pg = ((g as u16 * intensity) / 255) as u8;
                        let pb = ((b as u16 * intensity) / 255) as u8;
                        self.write_pixel_direct(px + x, py + y, pr, pg, pb);
                    }
                }
            }
        }
    }

    fn write_glyph(&mut self, glyph: &[u8; CHAR_WIDTH * CHAR_HEIGHT]) {
        let [r, g, b] = self.fg_color;
        for y in 0..CHAR_HEIGHT {
            for x in 0..CHAR_WIDTH {
                let intensity = glyph[y * CHAR_WIDTH + x] as u16;
                if intensity > 0 {
                    let pr = ((r as u16 * intensity) / 255) as u8;
                    let pg = ((g as u16 * intensity) / 255) as u8;
                    let pb = ((b as u16 * intensity) / 255) as u8;
                    self.write_pixel_direct(self.x_pos + x, self.y_pos + y, pr, pg, pb);
                }
            }
        }
        self.x_pos += CHAR_WIDTH;
    }

    fn write_rendered_char(&mut self, rendered_char: noto_sans_mono_bitmap::RasterizedChar) {
        let rw = rendered_char.width();
        if self.x_pos + rw >= self.width {
            self.new_line();
        }
        for x in 0..rw.max(CHAR_WIDTH) {
            for y in 0..CHAR_HEIGHT {
                self.write_pixel_direct(self.x_pos + x, self.y_pos + y, 0, 0, 0);
            }
        }
        let [r, g, b] = self.fg_color;
        for (y, row) in rendered_char.raster().iter().enumerate() {
            for (x, byte) in row.iter().enumerate() {
                if *byte > 0 {
                    let intensity = *byte as u16;
                    let pr = ((r as u16 * intensity) / 255) as u8;
                    let pg = ((g as u16 * intensity) / 255) as u8;
                    let pb = ((b as u16 * intensity) / 255) as u8;
                    self.write_pixel_direct(self.x_pos + x, self.y_pos + y, pr, pg, pb);
                }
            }
        }
        self.x_pos += CHAR_WIDTH;
    }

    pub fn write_pixel(&mut self, x: usize, y: usize, r: u8, g: u8, b: u8) {
        self.write_pixel_direct(x, y, r, g, b);
    }

    #[inline(always)]
    pub fn write_pixel_direct(&mut self, x: usize, y: usize, r: u8, g: u8, b: u8) {
        if x >= self.width || y >= self.height {
            return;
        }
        let byte_offset = (self.stride * y + x) * self.bytes_per_pixel;
        if byte_offset + self.bytes_per_pixel > self.framebuffer.len() {
            return;
        }
        let fb = &mut self.framebuffer[byte_offset..];
        if self.is_bgr {
            fb[0] = b; fb[1] = g; fb[2] = r;
        } else {
            fb[0] = r; fb[1] = g; fb[2] = b;
        }
        if self.bytes_per_pixel >= 4 {
            fb[3] = 0xFF;
        }
    }
}

impl fmt::Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            self.write_char(c);
        }
        Ok(())
    }
}
