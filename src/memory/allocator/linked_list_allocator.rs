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

/// Combines adjacent unallocated nodes into one node, starting with the given node.
fn combine_unallocated(node: &mut ListNode) {
    if node.allocated {
        return;
    }

    // Loop until an allocated node is found
    while let Some(
        next_node @ ListNode {
            allocated: false, ..
        },
    ) = &node.next
    {
        // Get the end of the allocation before moving the node into a local variable, as otherwise the wrong address would be calculated.
        let next_node_allocation_end = next_node.get_allocation_end();

        // SAFETY:
        // No references can exist to this node because the reference to `node` is mutable and therefore unique.
        // This read is semantically a move out of `next_node`
        let next_node = unsafe { core::ptr::read(*next_node) };
        node.next = next_node.next;

        // SAFETY: The new size is calculated to align with the end of the old allocation, so all the memory is owned and mapped.
        unsafe {
            node.set_size(next_node_allocation_end as usize - node.get_allocation_start() as usize);
        }
    }
}

/// Moves the given [`ListNode`] forward so that its allocation is aligned to the given alignment.
/// If the node can't fit the given `size` and `align`, `Err` with the original node is returned instead.
/// If the node is the last one in the list, its allocation will be expanded to the fit `size`.
///
/// # Safety:
/// * `align` must be a power of 2.
/// * The caller is responsible for updating the previous node to correctly point to the new node.
/// * The caller is responsible for ensuring the memory belonging to the returned node is mapped before returning it further.
unsafe fn align_next(
    node: &'static mut ListNode,
    align: usize,
    size: usize,
) -> Result<&'static mut ListNode, &'static mut ListNode> {
    let current_allocation_start = node.get_allocation_start() as usize;
    let aligned_start = align_up(current_allocation_start, align);

    let aligned_size = match (node.get_allocation_end() as usize).checked_sub(aligned_start) {
        None => return Err(node),
        Some(aligned_size) => aligned_size,
    };

    if aligned_size < size {
        // If the node is the last one in the list, the allocation can be freely expanded
        if node.next.is_none() {
            // SAFETY: this node is the last one in the list, so the size can be expanded as much as needed.
            // The caller is responsible for ensuring the memory is mapped.
            unsafe {
                node.set_size(size + current_allocation_start - aligned_start);
            }
        } else {
            return Err(node);
        }
    }

    let node = if current_allocation_start != aligned_start {
        let current_node_ptr = node as *const ListNode;
        let next_node_ptr = (aligned_start - ListNode::OFFSET) as *mut ListNode;

        // Move `next_node` so that the region after it is properly aligned
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
        unsafe { &mut *next_node_ptr }
    } else {
        node
    };

    // If allocating `size` bytes into this allocation would leave a lot of unused space, split the node into two
    if aligned_size > size + ListNode::ALIGN + 8 {
        let new_node_ptr =
            align_up(node.get_allocation_start() as usize + size, ListNode::ALIGN) as *mut ListNode;

        // Save `node.next` to later set it on the new node
        // This preserves the ordering of the list
        let next_node = node.next.take();
        
        // SAFETY:
        // This size is always smaller than the previous size, so the memory is mapped.
        // It is unused as it only extends up to the new node, not past it.
        unsafe {
            node.set_size(new_node_ptr as usize - node.get_allocation_start() as usize);
        }

        // SAFETY:
        // This memory no longer belongs to the previous node as its length was just changed,
        // however as it used to be owned it is guaranteed to be mapped.
        unsafe {
            core::ptr::write(
                new_node_ptr,
                ListNode::new(
                    node.get_allocation_end() as usize - new_node_ptr as usize,
                    false,
                    next_node,
                ),
            )
        }

        // SAFETY: This points to a valid object because it was just written to.
        node.next = Some(unsafe { &mut *new_node_ptr });
    }

    Ok(node)
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
    ///
    /// # Safety:
    /// No references may exist to [`ListNode`]s on this heap.
    unsafe fn get_head(&self) -> &'static ListNode {
        // SAFETY:
        // This `ListNode` was written in `init` and should never have been removed since.
        unsafe { &*(self.heap_start as *const ListNode) }
    }

    /// Gets a mutable reference to the first node in the linked list.
    ///
    /// # Safety:
    /// No other active references may exist to [`ListNode`]s on this heap.
    unsafe fn get_head_mut(&mut self) -> &'static mut ListNode {
        // SAFETY:
        // This `ListNode` was written in `init` and should never have been removed since.
        unsafe { &mut *(self.heap_start as *mut ListNode) }
    }

    /// Prints all the [`ListNode`]s in the [`LinkedListAllocator`].
    /// This is useful for debugging the allocator itself.
    ///
    /// # Safety:
    /// No references may exist to [`ListNode`]s on this heap.
    /// This condition is probably okay to violate for debugging purposes, but this function should not be used otherwise without
    /// making sure no [`ListNode`] references exist.
    #[allow(dead_code)]
    pub unsafe fn print_nodes(&self) {
        // SAFETY: no references exist to list nodes
        let mut current_node = unsafe { self.get_head() };
        loop {
            println!(
                "ListNode at {:p}: alloc at {:p}, size=0x{:x}, allocated={}",
                current_node,
                current_node.get_allocation_start(),
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
    /// * `align` must be a power of 2
    /// * No references to [`ListNode`]s anywhere in this [`LinkedListAllocator`] may be held when calling this function.
    ///     This is because this function can mutate any node in the list while running.
    ///     However, [allocated][ListNode::allocated] nodes are guaranteed to still be valid after the function exits,
    ///     so these references may be converted to pointers before the function is called and back to references after the function exits.
    unsafe fn allocate_region(
        &mut self,
        size: usize,
        align: usize,
    ) -> Result<*mut ListNode, AllocationError> {
        //println!("Allocating with size={} and align={}", size, align);

        // All allocations should be aligned the same as or greater than ListNodes,
        // so that the ListNode directly before each allocation is also correctly aligned
        let align = align.max(ListNode::ALIGN);

        // SAFETY: no references exist to list nodes.
        let mut current_node = unsafe { self.get_head_mut() };

        loop {
            let next_node = current_node.next.take();
            let Some(mut next_node) = next_node else {
                // Allocate a new allocation at the end of the list

                let current_allocation_end = current_node.get_allocation_end() as usize;

                let new_node_ptr = {
                    let new_allocation_start = align_up(current_allocation_end, align);
                    let new_node_ptr = new_allocation_start - ListNode::OFFSET;

                    // If the node would be allocated over the allocation of the previous node, move it forward
                    if new_node_ptr < current_allocation_end {
                        (new_node_ptr + align_up(ListNode::OFFSET, align)) as *mut ListNode
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

            // Combine adjacent unallocated nodes
            combine_unallocated(next_node);

            // If `next_node` is unallocated, try to fit the required size and alignment into it
            if !next_node.allocated {
                // SAFETY: `align` is a power of 2.
                // If allocation succeeds, the memory region is mapped.
                match unsafe { align_next(next_node, align, size) } {
                    Ok(new_node) => {
                        let new_node_ptr = new_node as *mut ListNode;
                        new_node.allocated = true;

                        if new_node.next.is_none() {
                            let start_frame = ((new_node_ptr as usize) - self.heap_start) / 4096;
                            let end_frame =
                                (new_node.get_allocation_end() as usize - self.heap_start) / 4096;

                            map_frames(
                                self.heap_start,
                                self.max_size,
                                start_frame,
                                end_frame - start_frame + 1,
                            )?;
                        }
                        // SAFETY:
                        // The new size is valid as it is calculated to not overlap with the next node.
                        // The memory is guaranteed to be mapped as it was just mapped with `map_frames`.
                        unsafe {
                            current_node.set_size(
                                // SAFETY: offset is a constant and so can't wrap
                                new_node_ptr as usize
                                    - current_node.get_allocation_start() as usize,
                            );
                        }

                        current_node.next = Some(new_node);

                        return Ok(new_node_ptr);
                    }
                    Err(next_node_ref) => next_node = next_node_ref,
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
        node.allocated = false;
    }

    /// Reallocate a given [`ListNode`] to have a larger allocation.
    ///
    /// # Safety:
    /// * `node` must be an [allocated][ListNode::allocated] [`ListNode`] owned by this [`LinkedListAllocator`]
    /// * `align` must be the same alignment the [`ListNode`] was originally allocated with, and must be a power of 2.
    unsafe fn reallocate_region(
        &mut self,
        node: &'static mut ListNode,
        new_size: usize,
        align: usize,
    ) -> Result<*mut ListNode, AllocationError> {
        println!("Reallocating to {}", new_size);

        // If the node already has enough space, don't move any data
        if node.get_size() >= new_size {
            return Ok(node);
        }

        // If the node is the last node in the list, expand it
        if node.next.is_none() {
            let start_frame = (node.get_allocation_start() as usize - self.heap_start) / 4096;
            let end_frame = (node.get_allocation_end() as usize - self.heap_start) / 4096;

            map_frames(
                self.heap_start,
                self.max_size,
                start_frame,
                end_frame - start_frame + 1,
            )?;

            // SAFETY:
            // This node is the last one in the list, so the memory after it is unused.
            // The memory was just mapped with `map_frames`.
            unsafe {
                node.set_size(new_size);
            }
            return Ok(node);
        }

        let node_ptr = node as *mut ListNode;
        // Get rid of the reference to `node` while calling `allocate_region`
        #[allow(dropping_references)]
        drop(node);

        // No optimisations applied, so allocate a new node
        // SAFETY: `align` is a power of 2.
        let new_node = unsafe { self.allocate_region(new_size, align) }?;

        // SAFETY: This was converted from a reference to an allocated node before calling `allocate_region`,
        // so it is guaranteed to still be valid.
        let node = unsafe { &mut *node_ptr };

        // Copy the data from the old node to the new one
        // SAFETY: The allocations are distinct and both at least this size, so the copy is valid
        unsafe {
            core::ptr::copy_nonoverlapping(
                node.get_allocation_start(),
                new_node.offset(1) as *mut u8,
                node.get_size(),
            )
        }

        Ok(new_node)
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
                .map(|ptr| {debug_assert_eq!(ptr as usize, align_up(ptr as usize, layout.align())); ptr} )
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

    unsafe fn realloc(
        &self,
        ptr: *mut u8,
        layout: core::alloc::Layout,
        new_size: usize,
    ) -> *mut u8 {
        // SAFETY: see individual lines
        unsafe {
            // Use `offset(-1)` because the given `ptr` points to the allocated memory, not to the node.
            // SAFETY: `ptr` is guaranteed to be a valid allocation on this heap, so it must be after a valid `ListNode`
            let node = &mut *(ptr as *mut ListNode).offset(-1);
            self.0
                .lock()
                .reallocate_region(node, new_size, layout.align())
                // use `offset(1)` here because we're returning the mapped memory region not the ListNode
                // SAFETY: The starting and ending pointers are part of the same allocation.
                // The offset does not wrap as it is a constant.
                .map(|node| node.offset(1) as *mut u8)
                // Check that the new pointer has the correct alignment
                .map(|ptr| {debug_assert_eq!(ptr as usize, align_up(ptr as usize, layout.align())); ptr} )
                .unwrap_or(null_mut())
        }
    }
}
