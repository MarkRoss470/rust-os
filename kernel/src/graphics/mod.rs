//! Functionality for drawing to a framebuffer

mod font_const;
mod framebuffer;

use crate::global_state::{GlobalState, TryLockedIfInitError};
use alloc::vec;
use alloc::vec::Vec;
use bootloader_api::info::{FrameBuffer, FrameBufferInfo, PixelFormat};
use core::fmt;
use spin::Mutex;

use self::{font_const::FONT_BITMAPS, framebuffer::FrameBufferController};

/// A 24-bit colour
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colour {
    /// How much red in the colour
    pub red: u8,
    /// How much green in the colour
    pub green: u8,
    /// How much blue in the colour
    pub blue: u8,
}

#[allow(dead_code)]
impl Colour {
    /// Construct a colour from its constituent parts
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self {
            red: r,
            green: g,
            blue: b,
        }
    }

    /// Black
    pub const BLACK: Self = Self::from_rgb(0, 0, 0);
    /// White
    pub const WHITE: Self = Self::from_rgb(255, 255, 255);

    /// Red
    pub const RED: Self = Self::from_rgb(255, 0, 0);
    /// Green
    pub const GREEN: Self = Self::from_rgb(0, 255, 0);
    /// Blue
    pub const BLUE: Self = Self::from_rgb(0, 0, 255);

    /// Yellow
    pub const YELLOW: Self = Self::from_rgb(255, 255, 0);
}

/// The size in pixels of each character
const CHAR_OFFSET: usize = 10;

/// A text writer into a framebuffer
pub struct Writer {
    /// The current row the [`Writer`] is writing at
    row: usize,
    /// The current column the [`Writer`] is writing at
    column: usize,

    /// The maximum width in columns the [`Writer`] can reach before moving to the next row
    width: usize,
    /// The maximum height in rows the [`Writer`] can reach before scrolling the screen
    height: usize,

    /// The current [`Colour`] of the text the [`Writer`] is rendering
    colour: Colour,
    /// The framebuffer the [`Writer`] is rendering into
    buffer: FrameBufferController,
}

const SCROLL_LINES: usize = 10;

impl Writer {
    /// Writes a character to the screen
    fn write_char(&mut self, c: char) {
        if c == '\n' {
            self.row += 1;
            self.column = 0;
        } else if c.is_ascii() {
            let start_x = self.column * CHAR_OFFSET;
            let start_y = self.row * CHAR_OFFSET;

            let bitmap = FONT_BITMAPS[c as usize];

            self.buffer
                .draw_packed_bitmap(bitmap, start_x, start_y, self.colour, Colour::BLACK)
                .unwrap();
        }

        self.column += 1;

        if self.column == self.width {
            self.row += 1;
            self.column = 0;
        }

        if self.row >= self.height {
            self.buffer
                .scroll(CHAR_OFFSET * SCROLL_LINES, Colour::BLACK);
            self.row = self.height - SCROLL_LINES;
        }
    }

    /// Sets the [`colour`][Writer::colour] of the [`Writer`]
    pub fn set_colour(&mut self, colour: Colour) {
        self.colour = colour;
    }

    /// Clears the entire framebuffer with the given [`Colour`]
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.buffer.clear(Colour::BLACK);
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            self.write_char(c);
            serial_print!("{c}");
        }
        Ok(())
    }
}

/// The global [`Writer`] used by [`print!`][crate::print!] and [`println!`][crate::println!]
pub static WRITER: GlobalState<Writer> = GlobalState::new();

/// An error which can occur when trying to write to the screen
#[derive(Debug, Clone, Copy)]
enum WriteError {
    /// The formatter passed to [`print!`] called [`print!`], leading to the inner call being unable to succeed.
    Reentrancy,
}

/// An error which may have occurred while writing to the screen.
/// Errors are stored here to indicate that writing failed.
static WRITE_ERROR: Mutex<Option<WriteError>> = Mutex::new(None);

/// Initialises the framebuffer.
pub fn init_graphics(framebuffer: &'static mut FrameBuffer) {
    let info = framebuffer.info();

    assert_eq!(info.pixel_format, PixelFormat::Bgr, "TODO: non-bgr formats");

    let mut buffer = FrameBufferController::new(info, framebuffer);

    buffer.clear(Colour::BLACK);

    WRITER.init(Writer {
        row: 0,
        column: 0,
        width: info.width / CHAR_OFFSET - 1,
        height: info.height / CHAR_OFFSET - 1,
        colour: Colour::WHITE,
        buffer,
    });
}

pub fn flush() -> Result<(), ()> {
    let mut writer = WRITER.try_lock().ok_or(())?;

    writer.buffer.flush();

    Ok(())
}

/// Clears the display, resetting the cursor to the top
pub fn clear() {
    let mut writer = WRITER.lock();

    writer.buffer.clear(Colour::BLACK);
    writer.column = 1;
    writer.row = 1;
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    // Disable interrupts while locking mutex to prevent deadlock
    interrupts::without_interrupts(|| {
        // If the writer is not initialised, or is locked, return immediately without printing anything
        match WRITER.try_locked_if_init() {
            Ok(mut lock) => {
                lock.write_fmt(args).unwrap();
            }
            Err(TryLockedIfInitError::Locked) => {
                if let Some(mut lock) = WRITE_ERROR.try_lock() {
                    *lock = Some(WriteError::Reentrancy)
                };
            }
            Err(TryLockedIfInitError::NotInitialised) => {
                serial_print!("{args}");
            }
        }
    });
}

/// Prints formatted arguments into the global [`static@WRITER`]
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ({
        $crate::graphics::_print(format_args!($($arg)*));
    });
}

/// Prints formatted arguments into the global [`static@WRITER`], and then a newline.
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ({
        $crate::print!($($arg)*);
        $crate::print!("\n");
    });
}
