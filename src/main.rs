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

use bootloader::BootInfo;
use memory::frame_allocator::BootInfoFrameAllocator;
use x86_64::structures::paging::OffsetPageTable;
use x86_64::VirtAddr;

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

/// The state of the kernel, and resources needed to manage memory and hardware
#[derive(Debug)]
struct KernelState {
    /// Struct which manages page tables to map virtual pages to physical memory
    offset_page_table: OffsetPageTable<'static>,
    /// Struct which allocates free frames of physical memory
    frame_allocator: BootInfoFrameAllocator,
}

/// Initialises the kernel and constructs a [`KernelState`] struct to represent it.
///
/// # Safety:
/// This function may only be called once, and must be called with kernel privileges.
/// The provided `boot_info` must be valid and correct.
unsafe fn init(boot_info: &'static BootInfo) -> KernelState {
    // SAFETY: This function is only called once. If the `physical_memory_offset` field of the BootInfo struct exists,
    // then the bootloader will have mapped all of physical memory at that address.
    let offset_page_table = unsafe {
        memory::init_mem(VirtAddr::new(boot_info.physical_memory_offset))
    };

    // SAFETY: The provided boot_info is correct
    let frame_allocator = unsafe {
        memory::frame_allocator::BootInfoFrameAllocator::init(&boot_info.memory_map)
    };

    KernelState { offset_page_table, frame_allocator }
}

// Set kernel_main as the entrypoint, with type-checked arguments
#[cfg(not(test))]
bootloader::entry_point!(kernel_main);

/// The entry point for the kernel.
/// This function initialises memory maps and interrupts
fn kernel_main(boot_info: &'static BootInfo) -> ! {
    // SAFETY:
    // This is the entry point for the program, so init() cannot have been run before.
    // This code runs with kernel privileges
    let mut _kernel_state = unsafe { init(boot_info) };

    loop {
        x86_64::instructions::hlt();
    }
}
