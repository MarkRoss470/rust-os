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
