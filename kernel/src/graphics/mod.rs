#![allow(clippy::missing_docs_in_private_items)] // TODO: remove

use core::fmt;

use bootloader_api::info::{FrameBuffer, FrameBufferInfo, PixelFormat};

use crate::{global_state::GlobalState, serial_println};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colour {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Colour {
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self {
            red: r,
            green: g,
            blue: b,
        }
    }

    pub const BLACK: Self = Self::from_rgb(0, 0, 0);
    pub const WHITE: Self = Self::from_rgb(255, 255, 255);

    pub const RED: Self = Self::from_rgb(255, 0, 0);
    pub const GREEN: Self = Self::from_rgb(0, 255, 0);
    pub const BLUE: Self = Self::from_rgb(0, 0, 255);
}

struct FrameBufferController {
    info: FrameBufferInfo,

    buffer: &'static mut [u8],
}

impl FrameBufferController {
    fn write_pixel(&mut self, x: usize, y: usize, colour: Colour) -> Result<(), ()> {
        if x > self.info.width || y > self.info.height {
            return Err(());
        }

        assert_eq!(
            self.info.pixel_format,
            PixelFormat::Bgr,
            "TODO: non-bgr formats"
        );

        let pixel_start = (y * self.info.stride + x) * self.info.bytes_per_pixel;
        self.buffer[pixel_start] = colour.blue;
        self.buffer[pixel_start + 1] = colour.green;
        self.buffer[pixel_start + 2] = colour.red;

        Ok(())
    }

    fn clear(&mut self, colour: Colour) {
        for y in 0..self.info.height {
            for x in 0..self.info.width {
                self.write_pixel(x, y, colour).unwrap();
            }
        }
    }

    fn draw_rect(
        &mut self,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        colour: Colour,
    ) -> Result<(), ()> {
        for y in y..y + width {
            for x in x..x + height {
                self.write_pixel(x, y, colour)?;
            }
        }

        Ok(())
    }
}

static FRAME_BUFFER: GlobalState<FrameBufferController> = GlobalState::new();

pub struct Writer {
    row: usize,
    column: usize,

    width: usize,
    height: usize,

    colour: Colour,
}

impl Writer {
    // TODO: load a font and display characters properly
    // For now, rely on the serial output being redirected by qemu
    fn write_char(&mut self, c: char) {
        if c == '\n' {
            self.row += 1;
            self.column = 0;
            return;
        }

        if c != ' ' {
            FRAME_BUFFER
                .lock()
                .draw_rect(self.column * 10, self.row * 10, 10, 10, self.colour)
                .unwrap();
        }

        self.column += 1;

        if self.column == self.width {
            self.row += 1;
            self.column = 0;
        }
    }

    pub fn set_colour(&mut self, colour: Colour) {
        self.colour = colour;
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            self.write_char(c);
        }
        Ok(())
    }
}

pub static WRITER: GlobalState<Writer> = GlobalState::new();

pub fn init_graphics(framebuffer: &'static mut FrameBuffer) {
    let info = framebuffer.info();

    let mut buffer = FrameBufferController { info, buffer: framebuffer.buffer_mut() };

    buffer.clear(Colour::BLACK);

    FRAME_BUFFER.init(buffer);

    WRITER.init(Writer {
        row: 0,
        column: 0,
        width: 100,
        height: 50,
        colour: Colour::WHITE,
    })
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    // Disable interrupts while locking mutex to prevent deadlock
    interrupts::without_interrupts(|| {
        WRITER.lock().write_fmt(args).unwrap();
    });
}

/// Prints formatted arguments into the global [`struct@WRITER`]
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ({
        $crate::graphics::_print(format_args!($($arg)*));
        $crate::serial::_print(format_args!($($arg)*));
    });
}

/// Prints formatted arguments into the global [`struct@WRITER`], and then a newline.
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ({
        $crate::print!($($arg)*);
        $crate::print!("\n");
    });
}
