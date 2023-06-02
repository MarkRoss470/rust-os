//! Contains the [`BootInfoFrameAllocator`] type which allocates frames of physical memory

use bootloader::bootinfo::{MemoryMap, MemoryRegionType};
use x86_64::PhysAddr;
use x86_64::structures::paging::{FrameAllocator, PhysFrame, Size4KiB};

/// A [`FrameAllocator`] that returns usable frames from the bootloader's memory map.
#[derive(Debug)]
pub struct BootInfoFrameAllocator {
    /// The [`MemoryMap`] of what sections of physical memory are free
    memory_map: &'static MemoryMap,
    /// The index into [`self.usable_frames`][Self::usable_frames]
    next: usize,
}

impl BootInfoFrameAllocator {
    /// Create a [`FrameAllocator`] from the passed memory map.
    ///
    /// # Safety
    /// The caller must guarantee that the passed
    /// memory map is valid. The main requirement is that all frames that are marked
    /// as `USABLE` in it are really unused.
    pub unsafe fn init(memory_map: &'static MemoryMap) -> Self {
        Self {
            memory_map,
            next: 0,
        }
    }

    /// Returns an iterator over the usable frames specified in the memory map given to [`init`][BootInfoFrameAllocator::init].
    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> {
        // get usable regions from memory map
        let regions = self.memory_map.iter();
        let usable_regions = regions
            .filter(|r| r.region_type == MemoryRegionType::Usable);
        // map each region to its address range
        let addr_ranges = usable_regions
            .map(|r| r.range.start_addr()..r.range.end_addr());
        // transform to an iterator of frame start addresses
        let frame_addresses = addr_ranges.flat_map(|r| r.step_by(4096));
        // create `PhysFrame` types from the start addresses
        frame_addresses.map(|addr| PhysFrame::containing_address(PhysAddr::new(addr)))
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