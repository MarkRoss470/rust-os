//! The [`LinkedListAllocator`] and [`GlobalKernelHeapAllocator`] types responsible for the global kernel heap.

use core::alloc::GlobalAlloc;
use core::ptr::null_mut;

use x86_64::structures::paging::{
    mapper::MapToError, FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB,
};
use x86_64::VirtAddr;

use crate::global_state::{GlobalState, GlobalStateLock};
use crate::KERNEL_STATE;

use super::align_up;
use super::list_node::ListNode;

/// A linked list allocator, which uses the [global frame allocator][crate::KernelState::frame_allocator]
/// and [global page table][crate::KernelState::page_table] to allocate frames when needed
#[derive(Debug)]
pub struct LinkedListAllocator {
    /// The address of the start of the heap
    heap_start: usize,
    /// The maximum size the heap can grow to, in frames
    max_size: usize,
}

/// An error that can occur when trying to allocate memory using a [`LinkedListAllocator`]
#[derive(Debug)]
pub enum AllocationError {
    /// The [allocator][LinkedListAllocator] reached its [maximum size][LinkedListAllocator::max_size].
    /// This error should not happen in practice, as the system will run out of physical memory long before the assigned address space is full.
    HeapFull,
    /// The [mapper][crate::KernelState::page_table] failed to map a page into virtual memory.
    /// This probably means the system is out of memory.
    MapToError(MapToError<Size4KiB>),
}

impl From<MapToError<Size4KiB>> for AllocationError {
    fn from(value: MapToError<Size4KiB>) -> Self {
        Self::MapToError(value)
    }
}

/// Maps the given number of frames, starting at the given offset (in frames) of virtual frames to unique physical frames.
/// Returns `Ok(())` if allocation succeeded, or `Err(())` if it failed.
///
/// # Fails
/// * If any of the requested frames lie outside of the bounds of the heap (if `frame_offset + num_frames > self.max_size`)
fn map_frames(
    heap_start: usize,
    max_size: usize,
    frame_offset: usize,
    num_frames: usize,
) -> Result<(), AllocationError> {
    // Check that the memory is in bounds
    if frame_offset + num_frames > max_size {
        println!(
            "HEAP FULL: {} + {} > {}",
            frame_offset, num_frames, max_size
        );
        return Err(AllocationError::HeapFull);
    }

    let page_range = {
        let range_start = VirtAddr::new(heap_start as u64);
        let range_start_page = Page::containing_address(range_start) + frame_offset as u64;

        let range_end_page = range_start_page + num_frames as u64;
        Page::range_inclusive(range_start_page, range_end_page)
    };

    let mut frame_allocator = KERNEL_STATE.frame_allocator.lock();
    let mut page_table = KERNEL_STATE.page_table.lock();

    for page in page_range {
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(MapToError::FrameAllocationFailed)?;
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

        // Don't re-map a frame if it's already mapped
        if page_table.translate_page(page).is_ok() {
            continue;
        }

        // SAFETY:
        // The physical page is not yet in use as it was just produced by the frame_allocator.
        // The virtual page is also unused as no heap allocation can occur before this method is called.
        unsafe {
            page_table
                .map_to(page, frame, flags, &mut *frame_allocator)?
                .flush()
        };
    }

    Ok(())
}

impl LinkedListAllocator {
    /// Initialises a new [`LinkedListAllocator`] in the given memory region.
    ///
    /// `heap_start` is the start of the heap.
    /// `max_size` is the maximum size **in frames** that the heap can reach.
    ///
    /// # Safety
    /// * `heap_start` must be page-aligned
    /// * The address range represented by `heap_start` and `max_size`
    pub unsafe fn init(heap_start: usize, max_size: usize) -> Result<Self, AllocationError> {
        map_frames(heap_start, max_size, 0, 1)?;

        let first_heap_node_ptr = heap_start as *mut ListNode;

        // SAFETY:
        // The pages for this write were just mapped by map_frames.
        // This means both that the write is valid and that nothing else is using this memory.
        unsafe {
            core::ptr::write(first_heap_node_ptr, ListNode::new(4, false, None));
        }

        Ok(Self {
            heap_start,
            max_size,
        })
    }

    /// Gets a shared reference to the first node in the linked list.
    fn get_head(&self) -> &'static ListNode {
        // SAFETY:
        // This `ListNode` was written in `init` and should never have been removed since.
        unsafe { &*(self.heap_start as *const ListNode) }
    }

    /// Gets a mutable reference to the first node in the linked list.
    fn get_head_mut(&mut self) -> &'static mut ListNode {
        // SAFETY:
        // This `ListNode` was written in `init` and should never have been removed since.
        unsafe { &mut *(self.heap_start as *mut ListNode) }
    }

    /// Prints all the [`ListNode`]s in the [`LinkedListAllocator`].
    /// This is useful for debugging the allocator itself.
    #[allow(dead_code)]
    fn print_nodes(&self) {
        let mut current_node = self.get_head();
        loop {
            println!(
                "ListNode at {:p}: size=0x{:x}, allocated={}",
                current_node,
                current_node.get_size(),
                current_node.allocated
            );
            match &current_node.next {
                None => return,
                Some(next_node) => current_node = next_node,
            }
        }
    }

    /// Either finds a [`ListNode`] with the required size and alignment and returns it,
    /// or constructs a new one at the end of the list. If neither of these is possible, an [`AllocationError`] is returned.
    ///
    /// # Safety
    /// `align` must be a power of 2
    unsafe fn allocate_region(
        &mut self,
        size: usize,
        align: usize,
    ) -> Result<*mut ListNode, AllocationError> {
        // All allocations should be aligned the same as or greater than ListNodes,
        // so that the ListNode directly before each allocation is also correctly aligned
        let align = align.max(ListNode::ALIGN);

        let mut current_node = self.get_head_mut();

        loop {
            let next_node = current_node.next.take();
            let Some(next_node) = next_node else {
                // Allocate a new allocation at the end of the list

                // The new ListNode needs to be at
                // The end of the allocation for the last node in the list aligned up to the align of ListNode,

                let current_allocation_end = current_node.get_allocation_end() as usize;

                let new_node_ptr = {
                    let new_allocation_start = align_up(current_allocation_end, align);
                    let new_node_ptr = new_allocation_start - ListNode::OFFSET;

                    if new_node_ptr < current_allocation_end {
                        (new_node_ptr + align.max(ListNode::OFFSET)) as *mut ListNode
                    } else {
                        new_node_ptr as *mut ListNode
                    }
                };

                let start_frame = ((new_node_ptr as usize) - self.heap_start) / 4096;
                let allocation_end = (new_node_ptr as usize + ListNode::OFFSET) + size;
                let end_frame = (allocation_end - self.heap_start) / 4096;
                
                map_frames(self.heap_start, self.max_size, start_frame, end_frame - start_frame + 1)?;

                // Double check that the new pointer is properly aligned and doesn't overlap with the previous allocation
                assert_eq!(new_node_ptr as usize, align_up(new_node_ptr as usize, ListNode::ALIGN));
                assert!(new_node_ptr as usize >= current_allocation_end);

                // Write a new `ListNode`
                // SAFETY:
                // The backing memory for this write was just allocated with map_frames.
                // Nothing else can be using the memory because it is after the previous allocation
                // (This, and proper alignment, are checked with the above asserts, which should never fail)
                unsafe { core::ptr::write(new_node_ptr, ListNode::new(size, true, None)); }

                // Convert `new_node_ptr` to a reference rather than a pointer
                // SAFETY:
                // The pointer points to a valid object because it was just written to
                let new_node = unsafe { &mut *new_node_ptr };

                // Update the previous last node to point to the new last node
                current_node.next = Some(new_node);
                
                // Return the new node
                return Ok(new_node_ptr);
            };

            if !next_node.allocated && next_node.get_size() >= size {
                // Get the start point of the allocation aligned to the given alignment
                let current_allocation_start = next_node.get_allocation_start() as usize;
                let aligned_start = align_up(current_allocation_start, align);
                let aligned_size = next_node.get_allocation_end() as usize - aligned_start;

                // If the allocation is still big enough after being aligned
                if aligned_size >= size {
                    let current_node_ptr = next_node as *const ListNode;
                    let next_node_ptr = (aligned_start - ListNode::OFFSET) as *mut ListNode;

                    current_node.next = None;

                    // Move `next_node` so that the region after it is properly aligned
                    // TODO: optimise this if the two pointers are the same?
                    // SAFETY:
                    // The read is sound because that memory is no longer referenced by `current_node`, so it's equivalent to a move.
                    // The write is sound because the node can only be moved forward into memory that was already owned by it.
                    // The new size is sound because it is calculated so that the end of the allocation is in the same place.
                    unsafe {
                        let mut next_node_info = core::ptr::read(current_node_ptr);
                        next_node_info.set_size(aligned_size);
                        core::ptr::write(next_node_ptr, next_node_info);
                    }

                    // TODO: add another node for the free space if possible
                    // SAFETY: The pointer points to valid data because it was just written to
                    let next_node = unsafe { &mut *next_node_ptr };

                    current_node.next = Some(next_node);

                    // SAFETY:
                    // The new size is valid as it is calculated to not overlap with the next node
                    unsafe {
                        current_node.set_size(
                            next_node_ptr as usize - current_node.get_allocation_start() as usize,
                        );
                    }

                    return Ok(next_node_ptr);
                }
            }

            current_node.next = Some(next_node);
            current_node = current_node.next.as_mut().unwrap();
        }
    }

    /// Deallocates the region of memory after the given [`ListNode`].
    ///
    /// # Safety:
    /// * `node` must be a valid [`ListNode`] belonging to this [`LinkedListAllocator`].
    unsafe fn deallocate_region(&mut self, node: &'static mut ListNode) {
        // TODO: proper deallocations
        node.allocated = false;
    }
}

/// A wrapper around
#[derive(Debug)]
pub struct GlobalKernelHeapAllocator(GlobalState<LinkedListAllocator>);

impl GlobalKernelHeapAllocator {
    /// Locks the contained [`GlobalState`].
    pub fn lock(&self) -> GlobalStateLock<LinkedListAllocator> {
        self.0.lock()
    }

    /// Wrapper around [`GlobalState::init`]
    pub fn init(&self, data: LinkedListAllocator) {
        self.0.init(data)
    }

    /// Wrapper around [`GlobalState::new`]
    pub const fn new() -> Self {
        Self(GlobalState::new())
    }

    /// Get a shared reference to the contained [`GlobalState`]
    pub const fn get(&self) -> &GlobalState<LinkedListAllocator> {
        &self.0
    }
}

// SAFETY: TODO
unsafe impl GlobalAlloc for GlobalKernelHeapAllocator {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        // SAFETY: See individual lines
        unsafe {
            self.0
                .lock()
                // SAFETY:
                // `align` is a power of 2 as `layout.align()` is also guaranteed to be one
                .allocate_region(layout.size(), layout.align())
                // use `offset(1)` here because we're returning the mapped memory region not the ListNode
                // SAFETY: The starting and ending pointers are part of the same allocation.
                // The offset does not wrap as it is a constant.
                .map(|node| node.offset(1) as *mut u8)
                // Check that the pointer has the correct alignment
                .unwrap_or(null_mut())
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: core::alloc::Layout) {
        // SAFETY: This function's safety requirements are the same as the called function
        unsafe {
            // Use `offset(-1)` because the given `ptr` points to the allocated memory, not to the node.
            // SAFETY: `ptr` is guaranteed to be a valid allocation on this heap, so it must be after a valid `ListNode`
            let node = (ptr as *mut ListNode).offset(-1);
            self.lock()
                // SAFETY: `ptr` is valid so `node` must be valid too
                .deallocate_region(&mut *node);
        }
    }
}
