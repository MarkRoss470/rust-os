//! A 64-bit OS for x86_64 systems.

#![no_std] // don't link the Rust standard library
#![no_main] // disable all Rust-level entry points

// Set up custom test harness
#![feature(custom_test_frameworks)]
#![test_runner(crate::tests::test_runner)]
#![reexport_test_harness_main = "test_main"]
// For interrupts
#![feature(abi_x86_interrupt)]
// For checking pointer alignment
#![feature(pointer_is_aligned_to)]
// Nice-to-have int methods, such as `div_ceil`
#![feature(int_roundings)]
// Allows `impl Future` type aliases for task systems
#![feature(type_alias_impl_trait)]
// Allows to work with `Box<MaybeUninit<T>>` more easily
#![feature(new_uninit)]
// Set up warnings and lints
#![warn(
    // clippy::pedantic,
    // clippy::nursery,
    missing_docs,
    clippy::missing_docs_in_private_items,
    clippy::semicolon_if_nothing_returned,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::ptr_as_ptr,
    clippy::cast_ptr_alignment,
    clippy::manual_assert,
    clippy::map_unwrap_or,
    clippy::redundant_closure,
    clippy::redundant_closure_for_method_calls,
    clippy::must_use_candidate,
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
use bootloader_api::{BootInfo, BootloaderConfig};
use cpu::interrupt_controllers::send_debug_self_interrupt;

#[macro_use]
mod serial;

mod acpi;
mod allocator;
mod cpu;
mod devices;
mod global_state;
mod graphics;
mod init;
pub mod input;
mod log;
mod panic;
mod pci;
mod scheduler;
pub mod util;

#[cfg(test)]
mod tests;

use global_state::*;
use input::pop_key;
use pci::lspci;

use crate::{acpi::power_off, graphics::clear, scheduler::num_tasks};

/// The starting virtual address where the kernel will be mapped by the bootloader
const KERNEL_VIRT_ADDR: u64 = 0xFFFF800000000000;

/// The config struct to instruct the bootloader how to load the kernel
const BOOT_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(bootloader_api::config::Mapping::Dynamic);
    config.mappings.dynamic_range_start = Some(KERNEL_VIRT_ADDR);
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
    unsafe { init::init(boot_info) };

    //x86_64::instructions::interrupts::disable();
    x86_64::instructions::interrupts::int3();

    // SAFETY: Just for debugging
    // unsafe { power_off().unwrap() };

    shell_loop()
}

/// Loops while receiving commands from keyboard input
fn shell_loop() -> ! {
    let mut input = String::new();

    print!(">");

    loop {
        x86_64::instructions::hlt();

        if let Some(key) = pop_key() {
            match key {
                pc_keyboard::DecodedKey::Unicode(c) => {
                    print!("{c}");

                    #[allow(unreachable_code)]
                    // This is needed because of a bug in rustc to do with uninhabited types
                    if c == '\n' {
                        let commands: Vec<_> =
                            input.split_whitespace().filter(|a| !a.is_empty()).collect();
                        if let Some(c) = commands.first() {
                            match *c {
                                "echo" => echo(&commands[1..]),
                                "lspci" => lspci(&commands[1..]),
                                // SAFETY: This is just a debug console, so killing the OS is fine.
                                // TODO: shut down the kernel first
                                "poweroff" => unsafe {
                                    power_off().unwrap();
                                },
                                "clear" => clear(),
                                "kinfo" => kinfo(&commands[1..]),
                                // SAFETY: For debugging only, not sound
                                "interrupt" => unsafe { debug_interrupt(&commands[1..]) },
                                "panic" => panic!("User-instructed panic"),
                                _ => println!("Unknown command {c}"),
                            }
                        }

                        input.clear();
                        print!(">");
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

/// Prints info about the kernel's state
fn kinfo(args: &[&str]) {
    match args.first().copied() {
        Some("schedule") => {
            println!("Kernel ticks: {}", KERNEL_STATE.ticks());
            println!("Registered tasks: {}", num_tasks());
        }

        Some("acpi") => {
            let acpica = KERNEL_STATE.acpica.lock();

            println!("MADT: {:?}", acpica.madt());
            println!("FADT: {:?}", acpica.fadt());
            println!("DSDT: {:?}", acpica.dsdt());

            if let Some(mcfg) = acpica.mcfg() {
                println!("MCFG: {:?}", acpica.mcfg());
                for record in mcfg.records() {
                    println!("    Record: {record:?}");
                }
            }
        }

        Some(a) => {
            println!("Unknown argument '{a}'");
        }
        None => println!("Provide argument for what to give info about"),
    }
}

/// Sends an interrupt on the vector specified in the first argument
unsafe fn debug_interrupt(args: &[&str]) {
    match args.first().map(|n| n.parse()) {
        Some(Ok(vector)) => {
            // SAFETY: For debugging only, not sound
            unsafe { send_debug_self_interrupt(vector) }
        }
        _ => {
            println!("First argument must be interrupt vector");
        }
    };
}
