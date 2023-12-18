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
use cpu::interrupt_controllers::send_debug_self_interrupt;
use log::Log;
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
mod devices;

use global_state::*;
use graphics::init_graphics;
use input::{init_keybuffer, pop_key};
use pci::lspci;

use crate::{
    acpi::power_off,
    graphics::{clear, flush, Colour, WRITER},
    scheduler::num_tasks,
};

/// This function is called on panic.
#[cfg(not(test))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    x86_64::instructions::interrupts::disable();

    println!("{info}");

    flush();

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

/// The kernel's implementation of the [`Log`] trait for printing logs
struct KernelLogger;

impl Log for KernelLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        let target = metadata.target();
        match metadata.level() {
            log::Level::Error => true,
            log::Level::Warn => true,
            log::Level::Trace | log::Level::Debug | log::Level::Info => {
                if target.starts_with("acpi") {
                    ![
                        "acpi_os_create_semaphore",
                        "acpi_os_delete_semaphore",
                        "acpi_os_signal_semaphore",
                        "acpi_os_wait_semaphore",
                        "acpi_os_allocate",
                        "acpi_os_free",
                    ]
                    .contains(&target)
                } else if target.starts_with("ps2") {
                    false
                } else {
                    true
                }
            }
        }
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        print!("[");

        let level_str = match record.level() {
            log::Level::Error => {
                if let Ok(mut w) = WRITER.try_locked_if_init() {
                    w.set_colour(Colour::RED)
                }
                "ERROR"
            }
            log::Level::Warn => {
                if let Ok(mut w) = WRITER.try_locked_if_init() {
                    w.set_colour(Colour::YELLOW)
                }
                "WARNING"
            }
            log::Level::Info => "INFO",
            log::Level::Debug => "DEBUG",
            log::Level::Trace => "TRACE",
        };

        print!("{level_str}");

        if let Ok(mut w) = WRITER.try_locked_if_init() {
            w.set_colour(Colour::WHITE)
        }

        match (record.module_path(), record.file()) {
            // If the record is an error, print the whole file path not just the module
            (_, Some(file)) if record.level() == log::Level::Error => {
                print!(" {file}");
                if let Some(line) = record.line() {
                    print!(":{line}");
                }
            }
            (Some(module), _) => {
                print!(" {module}");
                if let Some(line) = record.line() {
                    print!(":{line}");
                }
            }
            _ => (),
        }

        print!("] ");

        println!("{}", record.args());
    }

    fn flush(&self) {}
}

/// Sets up logging for the kernel
fn init_log() {
    log::set_logger(&KernelLogger).expect("Logging should have initialised");
    log::set_max_level(log::LevelFilter::Trace);
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

    init_log();

    KERNEL_STATE.page_table.init(page_table);
    println!("Initialised page table");

    println!(
        "Physical memory offset: {:#x}",
        boot_info.physical_memory_offset.into_option().unwrap()
    );
    debug_memory_regions(&boot_info.memory_regions);

    // SAFETY: The provided `boot_info` is correct
    unsafe { cpu::init_frame_allocator(&boot_info.memory_regions) };

    // SAFETY: This function is only called once.
    unsafe { cpu::init_kernel_stack() }

    println!("Initialised frame allocator");

    // SAFETY: This function is only called once. The provided `boot_info` is correct, so so are `offset_page_table` and `frame_allocator`
    unsafe { allocator::init_heap().expect("Initialising the heap should have succeeded") }

    println!("Initialised heap");

    init_graphics(boot_info.framebuffer.as_mut().unwrap());
    println!("Initialised graphics");
    flush();

    // SAFETY: This function is only called once
    unsafe {
        cpu::init_interrupts();
    }

    // SAFETY: This function is only called once.
    // The bootloader gets the rsdp pointer from the BIOS or UEFI so it is valid and accurate.
    unsafe { acpi::init(boot_info.rsdp_addr.into_option().unwrap()) };

    init_keybuffer();

    println!("Initialising APIC");
    flush();

    // SAFETY: This function is only called once.
    // TODO: This doesn't need unwrapping if the PIC is working
    unsafe { cpu::interrupt_controllers::init_local_apic().unwrap() };

    // SAFETY: This function is only called once.
    // The core is set up to receive interrupts as `init_interrupts` has been called above.
    unsafe { cpu::interrupt_controllers::init_io_apic().unwrap() };
    flush();

    // SAFETY: This function is only called once.
    unsafe { cpu::init_ps2() };

    // SAFETY: This function is only called once.
    unsafe { devices::init() };

    println!("Finished initialising kernel");
    flush();
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
                                "interrupt" => unsafe {debug_interrupt(&commands[1..])},
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
    match args.first().map(|n|n.parse()) {
        Some(Ok(vector)) => {
            // SAFETY: For debugging only, not sound
            unsafe { send_debug_self_interrupt(vector) }
        },
        _ => {
        println!("First argument must be interrupt vector");
        }
    };
}