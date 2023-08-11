//! Types and functions for managing memory and the CPU's state.
//! This includes managing the GDT and IDT, configuring the PICs, and loading interrupt handlers.

// pub mod allocator;
mod frame_allocator;
mod idt;

use core::arch::asm;

use bootloader_api::info::MemoryRegions;
pub use frame_allocator::BootInfoFrameAllocator;

use x86_64::structures::paging::OffsetPageTable;
use x86_64::structures::paging::PageTable;
use x86_64::PhysAddr;
use x86_64::VirtAddr;

use crate::global_state::KERNEL_STATE;
use crate::println;

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

#[derive(Debug)]
pub struct PhysicalMemoryAccessor {
    memory_offset: VirtAddr,
}

impl PhysicalMemoryAccessor {
    pub unsafe fn get_addr(&self, addr: PhysAddr) -> VirtAddr {
        self.memory_offset + addr.as_u64()
    }
}

/// This function:
/// * Loads the GDT and IDT structures
/// * Initialises the interrupt controller
/// * Turns on interrupts
/// * Constructs an [`OffsetPageTable`] and returns it
///
/// # Safety:
/// This function may only be called once, and must be called with kernel privileges.
/// All of physical memory must be mapped starting at address given by `physical_memory_offset`
pub unsafe fn init_cpu(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    enable_sse();

    // Load the IDT structure, which defines interrupt and exception handlers
    // SAFETY:
    // init_mem is only called once and this is the only call-site of idt::init
    unsafe { idt::init() }

    // Initialise the interrupt controller
    // SAFETY:
    // init_mem is only called once and this is the only call-site of idt::init_pic
    unsafe { idt::init_pic() }

    // Enable interrupts on the CPU
    x86_64::instructions::interrupts::enable();

    // SAFETY:
    // All of physical memory is mapped at the given address as a safety condition of init_mem
    let level_4_table = unsafe { active_level_4_table(physical_memory_offset) };
    
    KERNEL_STATE
        .physical_memory_accessor
        .init(PhysicalMemoryAccessor {
            memory_offset: physical_memory_offset,
        });

    // SAFETY:
    // The given level_4_table is correct as long as `physical_memory_offset` is correct,
    // as is `physical_memory_offset` itself.
    unsafe { OffsetPageTable::new(level_4_table, physical_memory_offset) }
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
pub unsafe fn init_frame_allocator(memory_map: &'static MemoryRegions) -> BootInfoFrameAllocator {
    // SAFETY:
    // `memory_map` is valid as a safety condition of this function
    unsafe { BootInfoFrameAllocator::init(memory_map) }
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
