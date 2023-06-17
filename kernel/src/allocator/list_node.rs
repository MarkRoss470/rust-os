//! Contains the [`ListNode`] type

use super::align_up;

/// A node in the linked list of a [`LinkedListAllocator`][super::LinkedListAllocator]
#[derive(Debug)]
pub struct ListNode {
    /// The size of the [`ListNode`]'s allocation
    size: usize,

    /// Whether the memory is still allocated.
    /// If `true`, the memory cannot be re-allocated to another allocation.
    /// If `false`, the previous allocation has expired and the memory can be re-allocated.
    pub allocated: bool,

    /// The next node in the list, if this is not the last node.
    /// [`ListNode`]s are linked sequentially in memory. `next` may only point to the [`ListNode`] directly after this one in memory.
    pub next: Option<&'static mut ListNode>,
}

impl ListNode {
    /// The alignment of a [`ListNode`]. Used for making sure [`ListNode`]s are always allocated aligned
    pub const ALIGN: usize = core::mem::align_of::<ListNode>();
    /// The offset of a [`ListNode`] in an array. Used for converting between [`ListNode`]s and their allocations.
    pub const OFFSET: usize = align_up(
        core::mem::size_of::<ListNode>(),
        core::mem::align_of::<ListNode>(),
    );

    /// Construct a new [`ListNode`] with the given state.
    pub fn new(size: usize, allocated: bool, next: Option<&'static mut ListNode>) -> Self {
        debug_assert_ne!(size, 0);
        debug_assert!(size < 100_000);

        Self {
            size,
            allocated,
            next,
        }
    }

    /// Set the [size][ListNode::size] of a [`ListNode`].
    ///
    /// # Panics:
    /// When trying to reduce the size of a node with [`allocated`][ListNode::allocated] set to `true`
    ///
    /// # Safety:
    /// `size` bytes after the [`ListNode`] must be mapped and unused.
    pub unsafe fn set_size(&mut self, size: usize) {
        debug_assert_ne!(size, 0);
        debug_assert!(size < 100_000);

        if size == 0 {
            panic!("Tried to set size to 0");
        }
        self.size = size;
    }

    /// Get the [`size`][ListNode::size] of the [`ListNode`]
    pub fn get_size(&self) -> usize {
        self.size
    }

    /// Get a pointer to the start of the [`ListNode`]'s allocation
    pub fn get_allocation_start(&self) -> *const u8 {
        // SAFETY:
        // The starting pointer (self) and the resulting pointer are in the same allocation as the resulting pointer is the start of the 'real' allocation
        // Computed offset cannot overflow as it is a constant
        unsafe { (self as *const ListNode).offset(1) as *const u8 }
    }

    /// Returns a pointer 1 byte after the end of the allocation, i.e. the first byte which is not owned by this [`ListNode`]
    pub fn get_allocation_end(&self) -> *const u8 {
        // Cast `self.size` to an `isize`
        let offset = isize::try_from(self.size).expect("self.size should have fit in an isize");

        // SAFETY:
        // The starting pointer (self.get_allocation_start()) and the resulting pointer are in the same allocation
        // as they are the start and end pointers of the same allocation
        // Computed offset cannot overflow due to the check above, and that the size of `u8` is 1 byte
        unsafe { self.get_allocation_start().offset(offset) }
    }

    /// Checks that this node and all nodes after it have a reasonable value of `size`
    /// and the correct relationship to their next node 
    #[allow(dead_code)]
    pub fn check(&self) {
        // SAFETY: no references exist to list nodes
        let mut current_node = self;
        loop {
            // Check that `size` is non-zero and less than 100kB
            // This check might give false positives, but allocations that large need a better allocator anyway
            assert_ne!(current_node.size, 0);
            assert!(current_node.size < 100_000);

            match &current_node.next {
                None => return,
                Some(next_node) => {
                    // Check that this node's allocation ends before the next node
                    assert!((current_node.get_allocation_end() as usize) <= *next_node as *const ListNode as usize);
                    current_node = next_node
                },
            }
        }
    }
}
