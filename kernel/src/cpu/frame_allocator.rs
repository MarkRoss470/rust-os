//! Contains the [`BootInfoFrameAllocator`] type which allocates frames of physical memory
// TODO: rewrite this to be able to deallocate frames

use bootloader_api::info::{MemoryRegion, MemoryRegionKind, MemoryRegions};
use x86_64::structures::paging::frame::PhysFrameRange;
use x86_64::structures::paging::{FrameAllocator, PhysFrame, Size4KiB};
use x86_64::PhysAddr;

/// A [`FrameAllocator`] that returns usable frames from the bootloader's memory map.
#[derive(Debug)]
pub struct BootInfoFrameAllocator {
    /// The [`MemoryRegion`] of what sections of physical memory are free
    memory_map: &'static [MemoryRegion],
    /// The region of the [`memory_map`][Self::memory_map] from which frames are currently being allocated
    current_region: usize,
    /// The next frame in the [`current_region`][Self::current_region] to be allocated
    current_frame: u64,
}

impl BootInfoFrameAllocator {
    /// Create a [`FrameAllocator`] from the passed memory map.
    ///
    /// # Safety
    /// The passed [`MemoryRegion`] must be valid. The main requirement is that all frames that are marked
    /// as `USABLE` in it are really unused.
    /// The returned [`FrameAllocator`] must be the only frame allocator globally, or frames will be allocated twice, causing undefined behaviour.
    pub unsafe fn new(memory_map: &'static MemoryRegions) -> Self {
        Self {
            memory_map,
            current_region: 0,
            current_frame: 0,
        }
    }

    /// Allocates consecutive physical frames.
    ///
    /// # Parameters:
    /// * `frames`: The number of frames to allocate
    /// * `align`: The byte alignment that the starting address of the frame needs to have
    pub fn allocate_consecutive(&mut self, frames: u64, align: u64) -> Option<PhysFrameRange> {
        // TODO: this skips lots of frames which can then never be allocated
        'regions: loop {
            let start_frame = self.allocate_frame()?;

            if !start_frame.start_address().is_aligned(align) {
                continue;
            }

            for i in 1..=frames {
                if self.allocate_frame()? - start_frame != i {
                    continue 'regions;
                }
            }

            return Some(PhysFrameRange {
                start: start_frame,
                end: start_frame + frames,
            });
        }
    }

    /// Frees pages which were previously allocated using [`allocate_frame`] or [`allocate_consecutive`]
    ///
    /// # Safety
    /// * `range` must be a page range previously allocated using [`allocate_frame`] or [`allocate_consecutive`]
    /// * The pages must be no longer in use - any pointers mapped into this memory will become invalid
    ///
    /// [`allocate_frame`]: BootInfoFrameAllocator::allocate_frame
    /// [`allocate_consecutive`]: BootInfoFrameAllocator::allocate_frame
    pub unsafe fn free(&mut self, range: PhysFrameRange) {
        let _ = range;
        // TODO: deallocations
    }
}

// SAFETY:
// The MemoryMap passed to init is guaranteed to be accurate, so this will only produce unused frames
unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        loop {
            let region = self.memory_map.get(self.current_region)?;

            // If the region is not usable, skip it
            if region.kind != MemoryRegionKind::Usable {
                self.current_region += 1;
                self.current_frame = 0;
                continue;
            }

            let frame = region.start + 0x1000 * self.current_frame;

            // If the end of the current region has been reached, move on to the next
            if frame >= region.end {
                self.current_region += 1;
                self.current_frame = 0;
                continue;
            }

            self.current_frame += 1;

            return Some(PhysFrame::containing_address(PhysAddr::new(frame)));
        }
    }
}
