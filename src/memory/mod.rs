//! Types and functions for managing memory and the CPU's state.
//! This includes managing the GDT and IDT, configuring the PICs, and loading interrupt handlers.

mod gdt;
mod idt;
pub mod frame_allocator;

use x86_64::structures::paging::OffsetPageTable;
use x86_64::structures::paging::PageTable;
use x86_64::VirtAddr;

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

/// This function:
/// * Loads the GDT and IDT structures
/// * Initialises the interrupt controller
/// * Turns on interrupts
/// * Constructs an [`OffsetPageTable`] and returns it
///
/// # Safety:
/// This function may only be called once, and must be called with kernel privileges.
/// All of physical memory must be mapped starting at address given by `physical_memory_offset`
pub unsafe fn init_mem(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    // Load the GDT structure
    // SAFETY:
    // init_mem is only called once and this is the only call-site of init_gdt
    unsafe { gdt::init() }

    // Load the IDT structure, which defines interrupt and exception handlers
    // SAFETY:
    // init_mem is only called once and this is the only call-site of init_gdt
    unsafe { idt::init() }

    // Initialise the interrupt controller
    // SAFETY:
    // init_mem is only called once and this is the only call-site of init_gdt
    unsafe { idt::init_pic() }

    // Enable interrupts on the CPU
    x86_64::instructions::interrupts::enable();

    // SAFETY:
    // All of physical memory is mapped at the given address as a safety condition of init_mem
    let level_4_table = unsafe { active_level_4_table(physical_memory_offset) };

    // SAFETY:
    // The given level_4_table is correct as long as `physical_memory_offset` is correct,
    // as is `physical_memory_offset` itself.
    unsafe { OffsetPageTable::new(level_4_table, physical_memory_offset) }
}
