//! Handles writing to the VGA buffer for text output

use spin::Mutex;
use volatile::Volatile;

pub mod colour;

#[cfg(test)]
mod tests;

use colour::{Colour, ColourCode};

/// One character in the VGA screen buffer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct ScreenChar {
    /// The character to be displayed
    ascii_character: u8,
    /// The [`ColourCode`] to display it with
    colour_code: ColourCode,
}

/// The number of lines in the VGA buffer
const BUFFER_HEIGHT: usize = 25;
/// The width of each line in the VGA buffer
const BUFFER_WIDTH: usize = 80;

/// The VGA buffer itself - 
/// a rectangular buffer of [`ScreenChar`]s which is mapped in memory over the hardware VGA buffer.
/// Because of this mapping, writing to the buffer will cause the written [`ScreenChar`] to appear on screen.
/// To prevent the writes from being optimised away, the [`Volatile`] wrapper type is used.
#[repr(transparent)]
struct Buffer {
    /// The buffer
    chars: [[Volatile<ScreenChar>; BUFFER_WIDTH]; BUFFER_HEIGHT],
}

/// A virtual write-head into a VGA [`Buffer`].
pub struct Writer {
    /// The current row
    row: usize,
    /// The current column
    column: usize,
    /// The current [`ColourCode`]
    colour_code: ColourCode,
    /// The [`Buffer`] to write into
    buffer: &'static mut Buffer,
}

impl Writer {
    /// Writes a byte into the [`Buffer`] at the current position, with the current colour.
    /// If the byte is a newline or the writer has reached the end of the row, [`new_line`][Self::new_line] will be called.
    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            byte => {
                if self.column >= BUFFER_WIDTH {
                    self.new_line();
                }

                let colour_code = self.colour_code;
                self.buffer.chars[self.row][self.column].write(ScreenChar {
                    ascii_character: byte,
                    colour_code,
                });
                self.column += 1;
            }
        }
    }

    /// Loops through the bytes of a [`str`] and calls [`write_byte`][Self::write_byte] for each.
    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                // printable ASCII byte or newline
                0x20..=0x7e | b'\n' => self.write_byte(byte),
                // not part of printable ASCII range
                _ => self.write_byte(0xfe),
            }
        }
    }

    /// Moves the write-head down one row and resets to the left hand side of the buffer.
    /// If the write-head is already on the bottom line, the contents of the whole buffer will be moved up by one line instead.
    fn new_line(&mut self) {
        // Move back to the left of the screen
        self.column = 0;

        // If not at the bottom of the screen, just move down a line
        if self.row < BUFFER_HEIGHT - 1 {
            self.row += 1;
            return;
        }

        // If at the bottom of the screen, move all the content up a line
        for row in 1..BUFFER_HEIGHT {
            for col in 0..BUFFER_WIDTH {
                let character = self.buffer.chars[row][col].read();
                self.buffer.chars[row - 1][col].write(character);
            }
        }

        // Clear the bottom row
        self.clear_row(BUFFER_HEIGHT - 1);
    }

    /// Clears the bottom row of the buffer with the space character.
    fn clear_row(&mut self, row: usize) {
        let blank = ScreenChar {
            ascii_character: b' ',
            colour_code: self.colour_code,
        };
        for col in 0..BUFFER_WIDTH {
            self.buffer.chars[row][col].write(blank);
        }
    }
}

use core::fmt;

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

use lazy_static::lazy_static;

lazy_static! {
    /// The global [`Writer`] instance, used for the [`print!`][crate::print!] and [`println!`][crate::println!] macros
    pub static ref WRITER: Mutex<Writer> = Mutex::new(Writer {
        row: 0,
        column: 0,
        colour_code: ColourCode::new(Colour::White, Colour::Black),
        // SAFETY:
        // The page containing the VGA buffer was identity mapped by the bootloader, so it is present at 0xb8000.
        // This code is only run once, so no duplicate references are created.
        buffer: unsafe { &mut *(0xb8000 as *mut Buffer) },
    });
}

/// Sets the [`colour_code`][Writer::colour_code] of the global [`struct@WRITER`]
pub fn set_colours(colours: ColourCode) {
    WRITER.lock().colour_code = colours;
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
    ($($arg:tt)*) => ($crate::vga::_print(format_args!($($arg)*)));
}

/// Prints formatted arguments into the global [`struct@WRITER`], and then a newline.
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}