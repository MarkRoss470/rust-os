//! Code to initialise the kernel and hardware

use crate::{acpi, allocator, cpu, log, println};

use bootloader_api::BootInfo;
use x86_64::VirtAddr;

use crate::global_state::*;
use crate::graphics::flush;
use crate::graphics::init_graphics;
use crate::input::init_keybuffer;

/// Initialises the kernel and constructs a [`KernelState`] struct to represent it.
///
/// # Safety:
/// This function may only be called once, and must be called with kernel privileges.
/// The provided `boot_info` must be valid and correct.
pub unsafe fn init(boot_info: &'static mut BootInfo) {
    // SAFETY: This function is only called once. If the `physical_memory_offset` field of the BootInfo struct exists,
    // then the bootloader will have mapped all of physical memory at that address.
    let page_table = unsafe {
        cpu::init_cpu(VirtAddr::new(
            boot_info.physical_memory_offset.into_option().unwrap(),
        ))
    };

    log::init_log();

    KERNEL_STATE.page_table.init(page_table);
    // println!("Initialised page table");

    // println!(
    //     "Physical memory offset: {:#x}",
    //     boot_info.physical_memory_offset.into_option().unwrap()
    // );

    // Get initrd and store in KERNEL_STATE
    // SAFETY: The bootloader has loaded the initrd here, so it is sound to construct this slice
    let init_rd = unsafe {
        core::slice::from_raw_parts(
            boot_info.ramdisk_addr.into_option().unwrap() as _,
            boot_info.ramdisk_len.try_into().unwrap(),
        )
    };

    *KERNEL_STATE.initrd.write() = Some(init_rd);

    // SAFETY: The provided `boot_info` is correct
    unsafe { cpu::init_frame_allocator(&boot_info.memory_regions) };

    // SAFETY: This function is only called once.
    unsafe { cpu::init_kernel_stack() }

    // println!("Initialised frame allocator");

    // SAFETY: This function is only called once. The provided `boot_info` is correct, so so are `offset_page_table` and `frame_allocator`
    unsafe { allocator::init_heap().expect("Initialising the heap should have succeeded") }

    // println!("Initialised heap");

    init_graphics(boot_info.framebuffer.as_mut().unwrap());
    // println!("Initialised graphics");

    let _ = flush();

    // SAFETY: This function is only called once
    unsafe {
        cpu::init_interrupts();
    }

    // SAFETY: This function is only called once.
    // The bootloader gets the rsdp pointer from the BIOS or UEFI so it is valid and accurate.
    unsafe { acpi::init(boot_info.rsdp_addr.into_option().unwrap()) };

    init_keybuffer();

    // println!("Initialising APIC");
    let _ = flush();

    // SAFETY: This function is only called once.
    // TODO: This doesn't need unwrapping if the PIC is working
    unsafe { cpu::interrupt_controllers::init_local_apic().unwrap() };

    // SAFETY: This function is only called once.
    // The core is set up to receive interrupts as `init_interrupts` has been called above.
    unsafe { cpu::interrupt_controllers::init_io_apic().unwrap() };
    let _ = flush();

    // SAFETY: This function is only called once.
    unsafe { cpu::init_ps2() };

    // SAFETY: This function is only called once.
    // unsafe { devices::init() };

    // println!("Finished initialising kernel");
    let _ = flush();
}


// /// Prints out the regions of a [`MemoryRegions`] struct in a compact debug form.
// fn debug_memory_regions(memory_regions: &MemoryRegions) {
//     println!();

//     let first = memory_regions.first().unwrap();

//     // Keep track of the previous region to merge adjacent regions of the same kind
//     let mut last_start = first.start;
//     let mut last_end = first.end;
//     let mut last_kind = first.kind;

//     for region in memory_regions.iter().skip(1) {
//         if region.start != last_end || region.kind != last_kind {
//             println!("{:#016x} - {:#016x}: {:?}", last_start, last_end, last_kind);
//             last_start = region.start;
//             last_end = region.end;
//             last_kind = region.kind;
//         } else {
//             last_end = region.end;
//         }
//     }

//     println!();
// }