//! Types for representing VGA colours

/// A VGA colour, either foreground or background
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Colour {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

/// A combination of a foreground and background [`Colour`]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
#[allow(clippy::module_name_repetitions)]
pub struct ColourCode(u8);

impl ColourCode {
    /// Constructs a new [`ColourCode`] from a foreground and background [`Colour`]
    pub const fn new(foreground: Colour, background: Colour) -> Self {
        Self((background as u8) << 4 | (foreground as u8))
    }
}
