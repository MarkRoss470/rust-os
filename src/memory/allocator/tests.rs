use super::list_node::ListNode;

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
    for i in 0..10_000 {
        let b = Box::new(i);
        core::hint::black_box(b);
    }
}

/// It is relied on in [`LinkedListAllocator`][super::LinkedListAllocator] that
/// `<pointer to ListNode>::offset(1) as usize` is equivalent to `<pointer to ListNode> as usize + ListNode::OFFSET`.
/// This function tests that assumption
#[test_case]
fn test_offset() {
    // Construct a fictitious pointer
    // This is not UB as pointers don't have to point to valid data like references do, and this pointer will never be dereferenced
    let ptr = 0x1000 as *const ListNode;

    // Test the equivalence
    assert_eq!(
        // SAFETY: offset is constant and can't wrap
        unsafe { ptr.offset(1) as usize },
        ptr as usize + ListNode::OFFSET
    );
    // Test the equivalence with subtraction
    assert_eq!(
        // SAFETY: offset is constant and can't wrap
        unsafe { ptr.offset(-1) as usize },
        ptr as usize - ListNode::OFFSET
    );
}
