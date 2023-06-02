//! Manages the kernel heap

use linked_list_allocator::LockedHeap;
use x86_64::VirtAddr;
use x86_64::structures::paging::{Mapper, Size4KiB, FrameAllocator, mapper::MapToError, Page, PageTableFlags};

/// The start address of the kernel heap
const HEAP_START: usize = 0x4000_0000_0000;
/// The size of the kernel heap
const HEAP_SIZE: usize = 100 * 1024; // 100 KiB

/// The global allocator instance
#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

/// Maps pages for the [kernel heap][ALLOCATOR] and sets up the backing [`LockedHeap`]
pub unsafe fn init_heap(
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) -> Result<(), MapToError<Size4KiB>> {
    let page_range = {
        let heap_start = VirtAddr::new(HEAP_START as u64);
        let heap_end = heap_start + HEAP_SIZE - 1u64;
        let heap_start_page = Page::containing_address(heap_start);
        let heap_end_page = Page::containing_address(heap_end);
        Page::range_inclusive(heap_start_page, heap_end_page)
    };

    for page in page_range {
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(MapToError::FrameAllocationFailed)?;
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        
        // SAFETY:
        // The physical page is not yet in use as it was just produced by the frame_allocator.
        // The virtual page is also unused as no heap allocation can occur before this method is called.
        unsafe {
            mapper.map_to(page, frame, flags, frame_allocator)?.flush()
        };
    }

    // SAFETY:
    // We have just allocated the backing pages for this heap
    unsafe {
        ALLOCATOR.lock().init(HEAP_START as *mut u8, HEAP_SIZE);
    }

    Ok(())
}


/// Tests that heap allocation does not panic and that values are stored correctly
#[test_case]
fn test_heap_allocation() {
    use alloc::boxed::Box;
    use alloc::string::ToString;

    let a = Box::new(20);
    assert_eq!(*a, 20);

    let s = "Hello world test string".to_string();
    assert_eq!(s.chars().nth(17), Some('s'));
}

/// Tests that large heap allocations do not panic, and that values are still stored correctly
#[test_case]
fn test_large_allocations() {
    use alloc::vec::Vec;

    let mut v = Vec::new();

    for i in 0..=1_000 {
        v.push(i);
    }

    assert_eq!(v.iter().sum::<u64>(), 1_000 * (1_000 + 1) / 2);
}

/// Tests that once an allocation is deallocated, the memory can be re-used for other allocations
#[test_case]
fn test_reallocation() {
    use alloc::boxed::Box;

    // If allocations can't be reused, this will run out of memory
    for i in 0..HEAP_SIZE {
        let b = Box::new(i);
        core::hint::black_box(b);
    }
}