//! A 64-bit OS for x86_64 systems.

#![no_std] // don't link the Rust standard library
#![no_main] // disable all Rust-level entry points

// Set up custom test harness
#![feature(custom_test_frameworks)]
#![test_runner(crate::tests::test_runner)]
#![reexport_test_harness_main = "test_main"]
// For interrupts
#![feature(abi_x86_interrupt)]
// Set up warnings and lints
#![warn(
    //clippy::pedantic,
    //clippy::nursery,
    rustdoc::all,
    clippy::missing_docs_in_private_items,
    unsafe_op_in_unsafe_fn,
)]
#![deny(clippy::undocumented_unsafe_blocks)]

// Use the std alloc crate for heap allocation
extern crate alloc;

use bootloader::BootInfo;
use x86_64::VirtAddr;

#[macro_use]
mod vga;
mod global_state;
mod memory;
#[cfg(test)]
mod tests;

use global_state::*;

/// This function is called on panic.
#[cfg(not(test))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("{info}");

    x86_64::instructions::interrupts::disable();

    loop {
        x86_64::instructions::hlt();
    }
}

/// Initialises the kernel and constructs a [`KernelState`] struct to represent it.
///
/// # Safety:
/// This function may only be called once, and must be called with kernel privileges.
/// The provided `boot_info` must be valid and correct.
unsafe fn init(boot_info: &'static BootInfo) {
    // SAFETY: This function is only called once. If the `physical_memory_offset` field of the BootInfo struct exists,
    // then the bootloader will have mapped all of physical memory at that address.
    let page_table = unsafe { memory::init_cpu(VirtAddr::new(boot_info.physical_memory_offset)) };
    KERNEL_STATE.page_table.init(page_table);

    // SAFETY: The provided `boot_info` is correct
    let frame_allocator = unsafe { memory::init_frame_allocator(&boot_info.memory_map) };
    KERNEL_STATE.frame_allocator.init(frame_allocator);

    // SAFETY: This function is only called once. The provided `boot_info` is correct, so so are `offset_page_table` and `frame_allocator`
    unsafe { memory::allocator::init_heap().expect("Initialising the heap should have succeeded") }

    println!("Finished initialising kernel");
}

// Set kernel_main as the entrypoint, with type-checked arguments
#[cfg(not(test))]
bootloader::entry_point!(kernel_main);

/// The entry point for the kernel.
/// This function initialises memory maps and interrupts

// To stop clippy giving a warning
// For some reason #[cfg(not(test))] takes away inlay hints and smart autocomplete
#[cfg_attr(test, allow(dead_code))]
fn kernel_main(boot_info: &'static BootInfo) -> ! {
    // SAFETY:
    // This is the entry point for the program, so init() cannot have been run before.
    // This code runs with kernel privileges
    unsafe { init(boot_info) };

    loop {
        x86_64::instructions::hlt();
    }
}
