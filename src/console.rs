use bootloader_api::info::FrameBufferInfo;
use core::fmt;
use lazy_static::lazy_static;
use spin::Mutex;

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
    if let Some(writer) = WRITER.lock().as_mut() {
        writer.write_fmt(args).unwrap();
    }
}

pub fn backspace() {
    if let Some(writer) = WRITER.lock().as_mut() {
        writer.backspace();
    }
}

pub fn clear_screen() {
    if let Some(writer) = WRITER.lock().as_mut() {
        writer.clear();
    }
}

pub fn set_color(r: u8, g: u8, b: u8) {
    if let Some(writer) = WRITER.lock().as_mut() {
        writer.fg_color = [r, g, b];
    }
}

pub fn reset_color() {
    if let Some(writer) = WRITER.lock().as_mut() {
        writer.fg_color = COLOR_MIKU;
    }
}

pub fn move_cursor_left() {
    if let Some(writer) = WRITER.lock().as_mut() {
        if writer.x_pos > BORDER_PADDING {
            writer.x_pos -= CHAR_WIDTH;
        }
    }
}

pub fn move_cursor_right() {
    if let Some(writer) = WRITER.lock().as_mut() {
        writer.x_pos += CHAR_WIDTH;
    }
}

pub fn get_x() -> usize {
    if let Some(writer) = WRITER.lock().as_ref() {
        writer.x_pos
    } else {
        0
    }
}

pub fn set_x(x: usize) {
    if let Some(writer) = WRITER.lock().as_mut() {
        writer.x_pos = x;
    }
}

pub fn draw_cursor(x: usize) {
    if let Some(writer) = WRITER.lock().as_mut() {
        for y in 1..CHAR_HEIGHT - 1 {
            writer.write_pixel(x, writer.y_pos + y, 200, 220, 220);
            writer.write_pixel(x + 1, writer.y_pos + y, 200, 220, 220);
        }
    }
}

pub fn erase_cursor(x: usize) {
    if let Some(writer) = WRITER.lock().as_mut() {
        for y in 1..CHAR_HEIGHT - 1 {
            writer.write_pixel(x, writer.y_pos + y, 0, 0, 0);
            writer.write_pixel(x + 1, writer.y_pos + y, 0, 0, 0);
        }
    }
}

pub fn clear_from_x(start_x: usize, count: usize) {
    if let Some(writer) = WRITER.lock().as_mut() {
        for i in 0..count {
            let cx = start_x + i * CHAR_WIDTH;
            for y in 0..CHAR_HEIGHT {
                for x in 0..CHAR_WIDTH {
                    writer.write_pixel(cx + x, writer.y_pos + y, 0, 0, 0);
                }
            }
        }
    }
}

pub fn hide_cursor() {
    if let Some(_writer) = WRITER.lock().as_mut() {}
}

pub fn show_cursor() {
    if let Some(_writer) = WRITER.lock().as_mut() {}
}

pub fn clear_char() {
    if let Some(writer) = WRITER.lock().as_mut() {
        writer.clear_char_at_cursor();
    }
}

pub const COLOR_MIKU: [u8; 3] = [57, 197, 187];
pub const COLOR_MIKU_DARK: [u8; 3] = [0, 150, 136];
pub const COLOR_MIKU_LIGHT: [u8; 3] = [128, 222, 217];
pub const COLOR_PINK: [u8; 3] = [255, 105, 140];
pub const COLOR_WHITE: [u8; 3] = [230, 240, 240];
pub const COLOR_GRAY: [u8; 3] = [120, 140, 140];
pub const COLOR_GREEN: [u8; 3] = [100, 220, 150];
pub const COLOR_YELLOW: [u8; 3] = [220, 220, 100];
pub const COLOR_CYAN: [u8; 3] = [0, 220, 220];

pub const BORDER_PADDING: usize = 10;
pub const CHAR_WIDTH: usize = 9;

pub struct Console {
    framebuffer: &'static mut [u8],
    info: FrameBufferInfo,
    pub x_pos: usize,
    pub y_pos: usize,
    pub fg_color: [u8; 3],
}

const LINE_SPACING: usize = 2;
const CHAR_HEIGHT: usize = 16;

impl Console {
    pub fn new(framebuffer: &'static mut [u8], info: FrameBufferInfo) -> Self {
        framebuffer.fill(0);
        Self {
            framebuffer,
            info,
            x_pos: BORDER_PADDING,
            y_pos: BORDER_PADDING,
            fg_color: COLOR_MIKU,
        }
    }

    pub fn clear(&mut self) {
        self.framebuffer.fill(0);
        self.x_pos = BORDER_PADDING;
        self.y_pos = BORDER_PADDING;
    }

    fn new_line(&mut self) {
        self.y_pos += CHAR_HEIGHT + LINE_SPACING;
        self.x_pos = BORDER_PADDING;
        if self.y_pos + CHAR_HEIGHT >= self.info.height {
            self.scroll_up();
        }
    }

    fn scroll_up(&mut self) {
        let line_bytes =
            (CHAR_HEIGHT + LINE_SPACING) * self.info.stride * self.info.bytes_per_pixel;
        let total = self.info.height * self.info.stride * self.info.bytes_per_pixel;
        if line_bytes >= total {
            return;
        }
        self.framebuffer.copy_within(line_bytes..total, 0);
        let clear_start = total - line_bytes;
        self.framebuffer[clear_start..total].fill(0);
        self.y_pos -= CHAR_HEIGHT + LINE_SPACING;
    }

    pub fn backspace(&mut self) {
        if self.x_pos > BORDER_PADDING {
            self.x_pos -= CHAR_WIDTH;
            self.clear_char_at_cursor();
        }
    }

    pub fn clear_char_at_cursor(&mut self) {
        for y in 0..CHAR_HEIGHT {
            for x in 0..CHAR_WIDTH {
                self.write_pixel(self.x_pos + x, self.y_pos + y, 0, 0, 0);
            }
        }
    }

    fn write_char(&mut self, c: char) {
        match c {
            '\n' => self.new_line(),
            '\x08' => self.backspace(),
            ch => {
                if let Some((glyph, _width)) = crate::font::get_glyph(ch) {
                    if self.x_pos + CHAR_WIDTH >= self.info.width {
                        self.new_line();
                    }
                    self.clear_char_at_cursor();
                    self.write_glyph(glyph);
                } else if let Some(raster) = noto_sans_mono_bitmap::get_raster(
                    ch,
                    noto_sans_mono_bitmap::FontWeight::Regular,
                    noto_sans_mono_bitmap::RasterHeight::Size16,
                ) {
                    if self.x_pos + raster.width() >= self.info.width {
                        self.new_line();
                    }
                    self.clear_char_at_cursor();
                    self.write_rendered_char(raster);
                }
            }
        }
    }

    fn write_glyph(&mut self, glyph: &[u8; CHAR_WIDTH * CHAR_HEIGHT]) {
        for y in 0..CHAR_HEIGHT {
            for x in 0..CHAR_WIDTH {
                let intensity = glyph[y * CHAR_WIDTH + x] as u16;
                if intensity > 0 {
                    let r = ((self.fg_color[0] as u16 * intensity) / 255) as u8;
                    let g = ((self.fg_color[1] as u16 * intensity) / 255) as u8;
                    let b = ((self.fg_color[2] as u16 * intensity) / 255) as u8;
                    self.write_pixel(self.x_pos + x, self.y_pos + y, r, g, b);
                }
            }
        }
        self.x_pos += CHAR_WIDTH;
    }

    fn write_rendered_char(&mut self, rendered_char: noto_sans_mono_bitmap::RasterizedChar) {
        for (y, row) in rendered_char.raster().iter().enumerate() {
            for (x, byte) in row.iter().enumerate() {
                if *byte > 0 {
                    let intensity = *byte as u16;
                    let r = ((self.fg_color[0] as u16 * intensity) / 255) as u8;
                    let g = ((self.fg_color[1] as u16 * intensity) / 255) as u8;
                    let b = ((self.fg_color[2] as u16 * intensity) / 255) as u8;
                    self.write_pixel(self.x_pos + x, self.y_pos + y, r, g, b);
                }
            }
        }
        self.x_pos += CHAR_WIDTH;
    }

    fn write_pixel(&mut self, x: usize, y: usize, r: u8, g: u8, b: u8) {
        if x >= self.info.width || y >= self.info.height {
            return;
        }
        let byte_offset = (self.info.stride * y + x) * self.info.bytes_per_pixel;
        let buffer = &mut self.framebuffer[byte_offset..];
        buffer[0] = b;
        buffer[1] = g;
        buffer[2] = r;
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
