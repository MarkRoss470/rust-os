//! Manages the kernel heap

mod linked_list_allocator;
mod list_node;
#[cfg(test)]
mod tests;

pub use self::linked_list_allocator::{
    AllocationError, GlobalKernelHeapAllocator, LinkedListAllocator,
};

/// The start address of the kernel heap
const HEAP_START: usize = 0x4000_0000_0000;
/// The maximum size that the kernel heap can reach, in frames
/// TODO: check that these address ranges are free
const HEAP_MAX_SIZE: usize = 25 * 1024 * 1024; // 25 MiFrames = 100 GiB

/// The global allocator instance
#[global_allocator]
pub static ALLOCATOR: GlobalKernelHeapAllocator = GlobalKernelHeapAllocator::new();

/// Align the given address `addr` upwards to alignment `align`.
///
/// Requires that `align` is a power of two.
const fn align_up(addr: usize, align: usize) -> usize {
    // See https://os.phil-opp.com/allocator-designs/#address-alignment for why this works
    (addr + align - 1) & !(align - 1)
}

/// Initialises the [`LinkedListAllocator`] with parameters.
///
/// # Safety
/// This function must only be called once.
/// The virtual memory region of size `max_size` bytes starting at the address `heap_start` must be completely unused,
/// and must stay unused other than by the [`LinkedListAllocator`] for the whole lifetime of the program.
pub unsafe fn init_heap() -> Result<(), AllocationError> {
    // SAFETY:
    // HEAP_START is page-aligned
    ALLOCATOR.init(unsafe { LinkedListAllocator::init(HEAP_START, HEAP_MAX_SIZE)? });

    Ok(())
}
