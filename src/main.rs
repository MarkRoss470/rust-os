#![no_std] // don't link the Rust standard library
#![no_main] // disable all Rust-level entry points

mod vga;

use core::panic::PanicInfo;

use vga::print_something;

/// This function is called on panic.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{info}");
    
    loop {}
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    print_something();

    println!("TEST");

    loop {}
}