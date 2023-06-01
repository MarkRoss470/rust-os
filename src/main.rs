#![no_std] // don't link the Rust standard library
#![no_main] // disable all Rust-level entry points

// Set up custom test harness
#![feature(custom_test_frameworks)]
#![test_runner(crate::tests::test_runner)]
#![reexport_test_harness_main = "test_main"]
// For interrupts
#![feature(abi_x86_interrupt)]

#[macro_use]
extern crate lazy_static;

#[macro_use]
mod vga;
mod memory;
#[cfg(test)]
mod tests;

/// This function is called on panic.
#[cfg(not(test))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("{info}");

    loop {
        x86_64::instructions::hlt();
    }
}

fn init() {
    // Load the GDT structure, which defines memory areas
    memory::init_gdt();
    // Load the IDT structure, which defines interrupt and exception handlers
    memory::init_idt();
    // Initialise the interrupt controller
    memory::init_pic();
    // Enable interrupts on the CPU
    x86_64::instructions::interrupts::enable();
}

#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn _start() -> ! {
    init();

    println!("Hello world");

    println!("Returned to original context");

    loop {
        x86_64::instructions::hlt();
    }
}
