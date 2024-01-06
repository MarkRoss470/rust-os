//! Manages the kernel heap

mod linked_list_allocator;
mod list_node;
#[cfg(test)]
mod tests;

use x86_64::structures::paging::{
    frame::PhysFrameRange, page::PageRange, FrameAllocator, Page, PhysFrame,
};

use crate::global_state::KERNEL_STATE;

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

/// A dynamically allocated, owned page of memory
#[derive(Debug)]
pub struct PageBox {
    /// The physical frame
    phys_frame: PhysFrame,

    /// The virtual page mapped to [`phys_frame`]
    ///
    /// [`phys_frame`]: PageBox::phys_frame
    virt_page: Page,
}

impl PageBox {
    /// Allocates a new page of memory. The contents of the page are uninitialised.
    pub fn new() -> Self {
        // The max size of a DCBAA is 2K bytes, so this allocation doesn't need to factor in `len`.
        // It's easier to just always allocate a page of memory than to try to satisfy all the requirements dynamically.
        let phys_frame = KERNEL_STATE
            .frame_allocator
            .lock()
            .allocate_frame()
            .unwrap();

        let frames = PhysFrameRange {
            start: phys_frame,
            end: phys_frame + 1, // Exclusive range so add 1
        };

        // SAFETY: `phys_frame` was just allocated, so it is not being used.
        let virt_pages = unsafe {
            KERNEL_STATE
                .physical_memory_accessor
                .lock()
                .map_frames(frames)
        };

        Self {
            phys_frame,
            virt_page: virt_pages.start,
        }
    }

    /// Allocates a new page, initialised to all zeroes.
    pub fn new_zeroed() -> Self {
        let mut page = Self::new();

        // SAFETY: This initialises the page to all zeroes
        unsafe {
            page.as_mut_ptr::<[u8; 0x1000]>()
                .write_volatile([0; 0x1000]);
        }

        page
    }

    /// Gets a pointer to the start of the page
    pub fn as_ptr<T>(&self) -> *const T {
        self.virt_page.start_address().as_ptr()
    }

    /// Gets a mutable pointer to the start of the page
    pub fn as_mut_ptr<T>(&mut self) -> *mut T {
        self.virt_page.start_address().as_mut_ptr()
    }

    /// Gets the [`PhysFrame`] allocated for this [`PageBox`]
    pub fn phys_frame(&self) -> PhysFrame {
        self.phys_frame
    }

    /// Gets the virtual [`Page`] allocated for this [`PageBox`]
    pub fn virt_page(&self) -> Page {
        self.virt_page
    }
}

impl Drop for PageBox {
    fn drop(&mut self) {
        // SAFETY: `virt_page` was allocated using `map_frames` in `new`, and is now no longer in use
        unsafe {
            let range = PageRange {
                start: self.virt_page,
                end: self.virt_page + 1,
            };

            KERNEL_STATE
                .physical_memory_accessor
                .lock()
                .unmap_frames(range);
        }

        // SAFETY: `phys_frame` was allocated using `allocate_frame` in `new`, and is now no longer in use.
        unsafe {
            let range = PhysFrameRange {
                start: self.phys_frame,
                end: self.phys_frame + 1,
            };

            KERNEL_STATE.frame_allocator.lock().free(range);
        }
    }
}
