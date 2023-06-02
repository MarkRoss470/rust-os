use crate::println;

/// Tests that the [`println!`] macro doesn't panic
#[test_case]
fn test_println_no_panic() {
    println!("Test data")
}

/// Tests that the [`println!`] macro doesn't panic, even if lots of lines are printed
#[test_case]
fn test_println_scrollback_no_panic() {
    for _ in 0..100 {
        println!("Test data")
    }
}
