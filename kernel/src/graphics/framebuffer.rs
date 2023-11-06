use alloc::vec;
use alloc::vec::Vec;
use bootloader_api::info::{FrameBuffer, FrameBufferInfo};

use super::Colour;

/// A wrapper around a framebuffer with software rendering utility functions
pub struct FrameBufferController {
    /// Info about the framebuffer
    info: FrameBufferInfo,
    /// The back buffer, where rendering occurs
    back_buffer: Vec<u8>,
    /// The front buffer. Writing to this buffer will show pixels on the screen
    front_buffer: &'static mut [u8],

    changed_start: usize,
    changed_end: usize,
}

impl FrameBufferController {
    /// Constructs a new controller from the given info and framebuffer.
    pub fn new(info: FrameBufferInfo, framebuffer: &'static mut FrameBuffer) -> Self {
        Self {
            info,
            back_buffer: vec![0; info.byte_len],
            front_buffer: framebuffer.buffer_mut(),

            changed_start: 0,
            changed_end: info.byte_len,
        }
    }

    /// Flushes the back buffer to the front buffer.
    pub fn flush(&mut self) {
        if self.changed_end <= self.changed_start {
            return;
        }

        self.front_buffer[self.changed_start..self.changed_end]
            .copy_from_slice(&self.back_buffer[self.changed_start..self.changed_end]);

        // self.front_buffer[..].copy_from_slice(&self.back_buffer[..]);
        self.changed_start = self.info.byte_len;
        self.changed_end = 0;
    }

    /// Sets the pixel at position (`x`, `y`) from the top left of the framebuffer to the given colour.
    /// Returns `Ok(())` if the write succeeded, or `Err(())` if it failed
    /// (if the coordinate given is outside the buffer)
    #[inline]
    fn write_pixel(&mut self, x: usize, y: usize, colour: Colour) -> Result<(), ()> {
        if x > self.info.width || y > self.info.height {
            return Err(());
        }

        let pixel_start = (y * self.info.stride + x) * self.info.bytes_per_pixel;
        self.back_buffer[pixel_start] = colour.blue;
        self.back_buffer[pixel_start + 1] = colour.green;
        self.back_buffer[pixel_start + 2] = colour.red;

        Ok(())
    }

    /// Clears the whole buffer with the given colour
    pub fn clear(&mut self, colour: Colour) {
        for y in 0..self.info.height {
            for x in 0..self.info.width {
                self.write_pixel(x, y, colour).unwrap();
            }
        }

        self.changed_start = 0;
        self.changed_end = self.info.byte_len;
    }

    #[inline]
    pub fn draw_packed_bitmap(
        &mut self,
        bitmap: [u8; 8],
        start_x: usize,
        start_y: usize,
        front: Colour,
        back: Colour,
    ) -> Result<(), ()> {
        for (y, row) in bitmap.iter().enumerate() {
            for x in 0..8 {
                // Extract one bit from the bitmap
                let colour = if row & (1 << x) != 0 { front } else { back };

                self.write_pixel(x + start_x, y + start_y, colour)?;
            }
        }

        let write_start = (start_y * self.info.stride + start_x) * self.info.bytes_per_pixel;
        let write_end =
            ((start_y + 8) * self.info.stride + (start_x + 8)) * self.info.bytes_per_pixel;

        self.changed_start = self.changed_start.min(write_start);
        self.changed_end = self.changed_end.max(write_end);

        Ok(())
    }

    /// Draws a rectangle with the top left corner at (`x`, `y`),
    /// with the given `width` and `height`, filled with the given colour
    #[allow(dead_code)]
    pub fn draw_rect(
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

        let write_start = (y * self.info.stride + x) * self.info.bytes_per_pixel;
        let write_end = ((y + height) * self.info.stride + (x + width)) * self.info.bytes_per_pixel;
        
        self.changed_start = write_start.min(write_start);
        self.changed_end = write_end.max(write_end);

        Ok(())
    }

    /// Scrolls the buffer vertically by `scroll_by` pixels,
    /// filling in the bottom rows with `fill`
    pub fn scroll(&mut self, scroll_by: usize, fill: Colour) {
        let byte_offset = scroll_by * self.info.stride * self.info.bytes_per_pixel;
        let copy_from = &self.back_buffer[byte_offset] as *const u8;
        let copy_to = &mut self.back_buffer[0] as *mut u8;
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

        self.changed_start = 0;
        self.changed_end = self.info.byte_len;
    }
}
