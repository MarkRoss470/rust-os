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
    /// The index into [`self.usable_frames`][Self::usable_frames]
    next: usize,
}

impl BootInfoFrameAllocator {
    /// Create a [`FrameAllocator`] from the passed memory map.
    ///
    /// # Safety
    /// The passed [`MemoryRegion`] must be valid. The main requirement is that all frames that are marked
    /// as `USABLE` in it are really unused.
    /// The returned [`FrameAllocator`] must be the only frame allocator globally, or frames will be allocated twice, causing undefined behaviour.
    pub unsafe fn init(memory_map: &'static MemoryRegions) -> Self {
        Self {
            memory_map,
            next: 0,
        }
    }

    /// Returns an iterator over the usable frames specified in the memory map given to [`init`][BootInfoFrameAllocator::init].
    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> {
        // get usable regions from memory map
        let regions = self.memory_map.iter();
        let usable_regions = regions.filter(|r| r.kind == MemoryRegionKind::Usable);
        // map each region to its address range
        let addr_ranges = usable_regions.map(|r| r.start..r.end);
        // transform to an iterator of frame start addresses
        let frame_addresses = addr_ranges.flat_map(|r| r.step_by(4096));
        // create `PhysFrame` types from the start addresses
        frame_addresses.map(|addr| PhysFrame::containing_address(PhysAddr::new(addr)))
    }

    /// Allocates consecutive physical frames.
    pub fn allocate_consecutive(&mut self, frames: u64, align: u64) -> Option<PhysFrameRange> {
        let mut frame_iter = self.usable_frames().take(self.next - 1);

        'regions: loop {
            let start_frame =
                frame_iter.find(|frame| frame.start_address().as_u64() & (align - 1) == 0)?;

            for i in 1..=frames {
                if frame_iter.next()? - start_frame != i {
                    continue 'regions;
                }
            }

            return Some(PhysFrameRange {
                start: start_frame,
                end: start_frame + frames - 1,
            });
        }
    }
}

// SAFETY:
// The MemoryMap passed to init is guaranteed to be accurate, so this will only produce unused frames
unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        frame
    }
}
