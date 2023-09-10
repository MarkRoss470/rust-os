//! Functionality for drawing to a framebuffer

mod font_const;

use crate::global_state::{GlobalState, TryLockedIfInitError};
use bootloader_api::info::{FrameBuffer, FrameBufferInfo, PixelFormat};
use core::fmt;
use spin::Mutex;

use self::font_const::FONT_BITMAPS;

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

/// A wrapper around a framebuffer with software rendering utility functions
struct FrameBufferController {
    /// Info about the framebuffer
    info: FrameBufferInfo,
    /// The buffer itself
    buffer: &'static mut [u8],
}

impl FrameBufferController {
    /// Sets the pixel at position (`x`, `y`) from the top left of the framebuffer to the given colour.
    /// Returns `Ok(())` if the write succeeded, or `Err(())` if it failed
    /// (if the coordinate given is outside the buffer)
    #[inline]
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

    /// Clears the whole buffer with the given colour
    fn clear(&mut self, colour: Colour) {
        for y in 0..self.info.height {
            for x in 0..self.info.width {
                self.write_pixel(x, y, colour).unwrap();
            }
        }
    }

    /// Draws a rectangle with the top left corner at (`x`, `y`),
    /// with the given `width` and `height`, filled with the given colour
    #[allow(dead_code)]
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

    /// Scrolls the buffer vertically by `scroll_by` pixels,
    /// filling in the bottom rows with `fill`
    fn scroll(&mut self, scroll_by: usize, fill: Colour) {
        let byte_offset = scroll_by * self.info.stride * self.info.bytes_per_pixel;
        let copy_from = &self.buffer[byte_offset] as *const u8;
        let copy_to = &mut self.buffer[0] as *mut u8;
        // Bound is calculated this way to make sure
        let count = self.info.byte_len - byte_offset;

        // SAFETY:
        // This copy is all within `self.buffer`, so the memory is owned.
        unsafe { core::ptr::copy(copy_from, copy_to, count) }

        for y in self.info.height - scroll_by..self.info.height {
            for x in 0..self.info.width {
                self.write_pixel(x, y, fill).unwrap();
            }
        }
    }
}

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

            for (y, row) in bitmap.iter().enumerate() {
                for x in 0..8 {
                    // Extract one bit from the bitmap
                    let colour = if row & (1 << x) != 0 {
                        self.colour
                    } else {
                        Colour::BLACK
                    };

                    self.buffer
                        .write_pixel(x + start_x, y + start_y, colour)
                        .unwrap();
                }
            }
        }

        self.column += 1;

        if self.column == self.width {
            self.row += 1;
            self.column = 0;
        }

        if self.row >= self.height {
            self.buffer.scroll(CHAR_OFFSET, Colour::BLACK);
            self.row = self.height - 1;
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
    let mut buffer = FrameBufferController {
        info,
        buffer: framebuffer.buffer_mut(),
    };

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
            },
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
