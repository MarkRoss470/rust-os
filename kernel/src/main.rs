//! A 64-bit OS for x86_64 systems.

#![no_std] // don't link the Rust standard library
#![no_main] // disable all Rust-level entry points

// Set up custom test harness
#![feature(custom_test_frameworks)]
#![test_runner(crate::tests::test_runner)]
#![reexport_test_harness_main = "test_main"]
// For interrupts
#![feature(abi_x86_interrupt)]
// For checking offsets of struct fields
#![feature(offset_of)]
// Nice-to-have int methods, such as `div_ceil`
#![feature(int_roundings)]
#![feature(pointer_byte_offsets)]
// Set up warnings and lints
#![warn(
    //clippy::pedantic,
    //clippy::nursery,
    missing_docs,
    clippy::missing_docs_in_private_items,
    rustdoc::all,
    unsafe_op_in_unsafe_fn,
)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![allow(rustdoc::private_intra_doc_links)]

#[macro_use]
extern crate bitfield_struct;

// Use the std alloc crate for heap allocation
extern crate alloc;

use alloc::{string::String, vec::Vec};
use bootloader_api::{info::MemoryRegions, BootInfo, BootloaderConfig};
use x86_64::VirtAddr;

#[macro_use]
mod serial;

mod acpi;
mod allocator;
mod cpu;
mod global_state;
mod graphics;
pub mod input;
mod pci;
mod scheduler;
#[cfg(test)]
mod tests;
pub mod util;

use global_state::*;
use graphics::init_graphics;
use input::{init_keybuffer, pop_key};
use pci::lspci;

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

/// Prints out the regions of a [`MemoryRegions`] struct in a compact debug form.
fn debug_memory_regions(memory_regions: &MemoryRegions) {
    println!();

    let first = memory_regions.first().unwrap();

    // Keep track of the previous region to merge adjacent regions of the same kind
    let mut last_start = first.start;
    let mut last_end = first.end;
    let mut last_kind = first.kind;

    for region in memory_regions.iter().skip(1) {
        if region.start != last_end || region.kind != last_kind {
            println!("{:#016x} - {:#016x}: {:?}", last_start, last_end, last_kind);
            last_start = region.start;
            last_end = region.end;
            last_kind = region.kind;
        } else {
            last_end = region.end;
        }
    }

    println!();
}

/// Initialises the kernel and constructs a [`KernelState`] struct to represent it.
///
/// # Safety:
/// This function may only be called once, and must be called with kernel privileges.
/// The provided `boot_info` must be valid and correct.
unsafe fn init(boot_info: &'static mut BootInfo) {
    // SAFETY: This function is only called once. If the `physical_memory_offset` field of the BootInfo struct exists,
    // then the bootloader will have mapped all of physical memory at that address.
    let page_table = unsafe {
        cpu::init_cpu(VirtAddr::new(
            boot_info.physical_memory_offset.into_option().unwrap(),
        ))
    };

    KERNEL_STATE.page_table.init(page_table);

    println!("Initialised page table");

    init_graphics(boot_info.framebuffer.as_mut().unwrap());

    // SAFETY: This function is only called once.
    // The bootloader gets the rsdp pointer from the BIOS or UEFI so it is valid and accurate.
    unsafe {
        acpi::init(
            boot_info.rsdp_addr.into_option().unwrap(),
            boot_info.physical_memory_offset.into_option().unwrap(),
        );
    }

    println!("Initialised graphics");
    println!(
        "Physical memory offset: {:#x}",
        boot_info.physical_memory_offset.into_option().unwrap()
    );
    debug_memory_regions(&boot_info.memory_regions);

    // SAFETY: The provided `boot_info` is correct
    unsafe { cpu::init_frame_allocator(&boot_info.memory_regions) };

    println!("Initialised frame allocator");

    // SAFETY: This function is only called once. The provided `boot_info` is correct, so so are `offset_page_table` and `frame_allocator`
    unsafe { allocator::init_heap().expect("Initialising the heap should have succeeded") }

    println!("Initialised heap");

    // SAFETY: This function is only called once
    unsafe { pci::init() }

    init_keybuffer();

    println!("Finished initialising kernel");
}

/// The config struct to instruct the bootloader how to load the kernel
const BOOT_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(bootloader_api::config::Mapping::Dynamic);
    config
};

// Set kernel_main as the entrypoint, with type-checked arguments
#[cfg(not(test))]
bootloader_api::entry_point!(kernel_main, config = &BOOT_CONFIG);

/// The entry point for the kernel.
/// This function initialises memory maps and interrupts

// To stop clippy giving a warning
// For some reason #[cfg(not(test))] takes away inlay hints and smart autocomplete
#[cfg_attr(test, allow(dead_code))]
fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    // SAFETY:
    // This is the entry point for the program, so init() cannot have been run before.
    // This code runs with kernel privileges
    unsafe { init(boot_info) };

    //x86_64::instructions::interrupts::disable();
    x86_64::instructions::interrupts::int3();

    lspci();

    let mut input = String::new();

    loop {
        x86_64::instructions::hlt();

        if let Some(key) = pop_key() {
            match key {
                pc_keyboard::DecodedKey::Unicode(c) => {
                    print!("{c}");
                    if c == '\n' {
                        let commands: Vec<_> = input.split(' ').filter(|a| !a.is_empty()).collect();
                        if let Some(c) = commands.first() {
                            match *c {
                                "echo" => echo(&commands[1..]),
                                "lspci" => lspci(),
                                _ => println!("Unknown command {c}"),
                            }
                        }

                        input.clear();
                    } else {
                        input.push(c);
                    }
                }
                pc_keyboard::DecodedKey::RawKey(_) => {}
            }
        }
    }
}

/// The `echo` command - prints its arguments separated by a space
fn echo(args: &[&str]) {
    for arg in args {
        print!("{arg} ");
    }
    println!();
}
