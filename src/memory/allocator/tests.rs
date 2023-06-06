use core::hint::black_box;

use super::list_node::ListNode;

/// Tests that heap allocation does not panic and that values are stored correctly
#[test_case]
fn test_heap_allocation() {
    use alloc::boxed::Box;
    use alloc::string::ToString;

    let a = Box::new(20);
    assert_eq!(*a, 20);

    let s = "Hello world test string".to_string();
    assert_eq!(s.chars().nth(black_box(17)), Some('s'));
}

/// Tests that large heap allocations do not panic, and that values are still stored correctly
#[test_case]
fn test_large_allocations() {
    use alloc::vec::Vec;

    let mut v = Vec::new();

    for i in 0..=1_000 {
        v.push(black_box(i));
    }

    assert_eq!(v.iter().sum::<u64>(), 1_000 * (1_000 + 1) / 2);
}

/// Tests that allocations are returned with the proper alignment
#[test_case]
fn test_allocation_alignment() {

    use alloc::boxed::Box;

    /// Defines a struct with the given alignment and size, allocates it in a [`Box`], and checks that it is correctly aligned
    macro_rules! test_alignment {
        ($align: expr, $size: expr) => {
            {
                // Define struct with given size and alignment
                #[repr(align($align))]
                struct AlignTest([u8; $size]);

                // Allocate it and check alignment
                let a = Box::new(AlignTest([0; $size]));
                let addr = a.as_ref() as *const AlignTest as usize;
                
                // Black box is needed for check not to be optimised away
                if addr % black_box($align) != 0 {
                    panic!("Misaligned reference: returned address 0x{:x} should have had alignment 0x{:x}, but it did not.", addr, $align);
                }
            }
        };
    }

    test_alignment!(4,    4);
    test_alignment!(16,   4);
    test_alignment!(256,  4);
    test_alignment!(4096, 4);

    test_alignment!(4,    256);
    test_alignment!(16,   256);
    test_alignment!(256,  256);
    test_alignment!(4096, 256);

    test_alignment!(4,    4096);
    test_alignment!(16,   4096);
    test_alignment!(256,  4096);
    test_alignment!(4096, 4096);

}

/// Tests that once an allocation is deallocated, the memory can be re-used for other allocations
#[test_case]
fn test_reallocation() {
    use alloc::boxed::Box;

    // If allocations can't be reused, this will run out of memory
    for i in 0..10_000 {
        let b = Box::new(i);
        black_box(b);
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
