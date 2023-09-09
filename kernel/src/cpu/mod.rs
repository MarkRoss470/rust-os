//! Types and functions for managing memory and the CPU's state.
//! This includes managing the GDT and IDT, configuring the PICs, and loading interrupt handlers.

// pub mod allocator;
mod frame_allocator;
mod idt;
pub mod interrupt_controllers;
pub mod ps2;


use core::arch::asm;

use bootloader_api::info::MemoryRegions;
pub use frame_allocator::BootInfoFrameAllocator;

use x86_64::structures::paging::frame::PhysFrameRange;
use x86_64::structures::paging::Mapper;
use x86_64::structures::paging::OffsetPageTable;
use x86_64::structures::paging::Page;
use x86_64::structures::paging::PageTable;
use x86_64::structures::paging::PageTableFlags;
use x86_64::VirtAddr;

use crate::global_state::KERNEL_STATE;
use crate::println;

use self::ps2::Ps2Controller8042;
use self::ps2::PS2_CONTROLLER;

/// Returns a mutable reference to the active level 4 table.
///
/// # Safety:
/// The caller must guarantee that the complete physical memory is mapped to virtual memory at the passed `physical_memory_offset`.
/// Also, this function must be only called once to avoid aliasing `&mut` references.
unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    use x86_64::registers::control::Cr3;

    let (level_4_table_frame, _) = Cr3::read();

    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();

    // SAFETY:
    // This function is unsafe and may only be called once, so a mutable static reference can be created
    unsafe { &mut *page_table_ptr }
}

/// The start address of the virtual memory region set aside for mapping MMIO regions
const PHYSICAL_MEMORY_ACCESS_START: u64 = 0x5000_0000_0000;
/// The max size in frames of the virtual memory region set aside for mapping MMIO regions
/// TODO: check that these address ranges are free
const PHYSICAL_MEMORY_ACCESS_MAX_SIZE: u64 = 25 * 1024 * 1024; // 25 MiFrames = 100 GiB

/// Helper struct for accessing physical addresses
#[derive(Debug)]
pub struct PhysicalMemoryAccessor {
    /// The index of the next virtual frame to be allocated from [`PHYSICAL_MEMORY_ACCESS_START`]
    next_frame: u64,
}

impl PhysicalMemoryAccessor {
    /// Maps the given page range into virtual memory and returns the address where they were mapped
    pub fn map_frames(&mut self, frames: PhysFrameRange) -> VirtAddr {
        let flags: PageTableFlags =
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_CACHE;

        let num_frames = frames.end - frames.start;
        let mut page_table = KERNEL_STATE.page_table.lock();
        let mut frame_allocator = KERNEL_STATE.frame_allocator.lock();

        let start_virtual_page =
            Page::containing_address(VirtAddr::new(PHYSICAL_MEMORY_ACCESS_START)) + self.next_frame;

        self.next_frame += frames.end - frames.start;

        if self.next_frame >= PHYSICAL_MEMORY_ACCESS_MAX_SIZE {
            panic!("Used up MMIO mapping space");
        }

        let start_physical_page = frames.start;

        for i in 0..num_frames {
            let page = start_virtual_page + i;

            // SAFETY: This virtual frame has not been used yet.
            // It is the caller's responsibility to make sure the physical frame is valid.
            unsafe {
                page_table
                    .map_to(page, start_physical_page + i, flags, &mut *frame_allocator)
                    .unwrap()
                    .flush();
            }
        }

        for i in 0..num_frames {
            let page = start_virtual_page + i;
            let physical_page = page_table.translate_page(page).unwrap();

            debug_assert_eq!(physical_page, start_physical_page + i);
        }

        start_virtual_page.start_address()
    }
}

/// This function:
/// * Initialises the interrupt controller
/// * Constructs an [`OffsetPageTable`] and returns it
///
/// # Safety
/// This function may only be called once.
/// All of physical memory must be mapped starting at address given by `physical_memory_offset`
pub unsafe fn init_cpu(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    enable_sse();

    // Load the IDT structure, which defines interrupt and exception handlers
    // SAFETY:
    // init_cpu is only called once and this is the only call-site of idt::init
    unsafe { idt::init() }

    // Initialise the interrupt controller
    // SAFETY:
    // init_cpu is only called once and this is the only call-site of idt::init_pic
    unsafe { interrupt_controllers::init_pic() }

    // Enable interrupts on the CPU
    x86_64::instructions::interrupts::enable();

    // SAFETY:
    // All of physical memory is mapped at the given address as a safety condition of init_mem
    let level_4_table = unsafe { active_level_4_table(physical_memory_offset) };

    KERNEL_STATE
        .physical_memory_accessor
        .init(PhysicalMemoryAccessor { next_frame: 0 });

    // SAFETY:
    // The given level_4_table is correct as long as `physical_memory_offset` is correct,
    // as is `physical_memory_offset` itself.
    unsafe { OffsetPageTable::new(level_4_table, physical_memory_offset) }
}

/// This function:
/// * Loads the GDT and IDT structures
/// * Turns on interrupts
///
/// # Safety
/// This function may only be called once.
/// This function must be called after [`init_cpu`]
pub unsafe fn init_interrupts() {
    // Load the IDT structure, which defines interrupt and exception handlers
    // SAFETY:
    // This function is only called once and this is the only call-site of idt::init
    // `init_gdt` must have been called as it is called in `init_cpu`, which has been called before this function
    unsafe { idt::init() }

    // Initialise the interrupt controller
    // SAFETY:
    // This function is only called once and this is the only call-site of idt::init_pic
    unsafe { interrupt_controllers::init_pic() }

    // Enable interrupts on the CPU
    x86_64::instructions::interrupts::enable();
}

/// Initialises the 8042 PS/2 controller if it is present
/// 
/// # Safety
/// This function may only be called once.
pub unsafe fn init_ps2() {
    // SAFETY: This function is only called once
    unsafe {
        if let Some(controller) = Ps2Controller8042::new() {
            println!("{controller:?}");
            PS2_CONTROLLER.init(controller.unwrap())
        } else {
            println!("No PS/2 Controller");
        }
    }
}

/// Enables SSE (SIMD float processing) by writing values into the `cr0` and `cr4` registers
fn enable_sse() {
    let mut cr0: u64;
    let mut cr4: u64;

    // SAFETY: this only reads the value of the cr0 and cr4 registers, which has no side-effects.
    unsafe {
        asm!(
            "mov {cr0}, cr0",
            "mov {cr4}, cr4",
            cr0 = out(reg) cr0,
            cr4 = out(reg) cr4,
        );
    }

    // Unset cr0 bit 2 to remove emulated FPU
    cr0 &= !(1 << 2);
    // Set cr0 bit 1 to have correct interaction with co-processor
    cr0 |= 1 << 1;
    // Set cr4 bit 9 to enable SSE instructions
    cr4 |= 1 << 9;
    // Set cr4 bit 10 to enable unmasked SSE exceptions
    cr4 |= 1 << 10;

    // SAFETY: this only sets certain bits in control registers which are needed for SSE,
    // And does not change anything else
    unsafe {
        asm!(
            "mov cr0, {cr0}",
            "mov cr4, {cr4}",
            cr0 = in(reg) cr0,
            cr4 = in(reg) cr4,
        );
    }

    println!("Enabled SSE");
}

/// Initialises the [global frame allocator][crate::global_state::KernelState::frame_allocator].
///
/// # Safety
/// This function must only be called once. The provided [`MemoryRegions`] must be valid and correct.
pub unsafe fn init_frame_allocator(memory_map: &'static MemoryRegions) {
    // SAFETY:
    // `memory_map` is valid as a safety condition of this function
    let frame_allocator = unsafe { BootInfoFrameAllocator::new(memory_map) };
    KERNEL_STATE.frame_allocator.init(frame_allocator);
}

/// Tests that floating point numbers are usable and work correctly
#[test_case]
fn test_floats() {
    let mut a = 0.0f32;

    for i in 0..200 {
        a += i as f32;
    }

    assert_eq!(a, 19900.0);

    let mut a = 0.0;

    for i in 0..200 {
        a += (i as f64) / 10.0;
    }

    // Due to floating point inaccuracies, this will come out not quite equal
    let error = libm::fabs(a - 1990.0);
    assert!(error < libm::pow(10.0, -10.0));
}
