//! Functionality to manage the global descriptor table.
//! The GDT is not used for as much on 64-bit CPUs as it was on 32-bit ones as segmentation is not supported on x86_64, but it still has some functions.
//! This includes managing the stacks which interrupt handlers use, and switching between different privilege levels.

use lazy_static::lazy_static;
use x86_64::instructions::tables::load_tss;
use x86_64::registers::segmentation::{Segment, CS};
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

/// The index into the IST of the stack the double fault handler will use
pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

/// The memory segments used by interrupt handlers
struct Selectors {
    /// The code segment selector - tells the CPU what privilege level to run at
    code_selector: SegmentSelector,
    /// Task State Segment selector - tells the CPU what TSS to use.
    /// This TSS contains the IST which tells the CPU what stacks to use for interrupts
    tss_selector: SegmentSelector,
}

lazy_static! {
    static ref TSS: TaskStateSegment = {
        let mut tss = TaskStateSegment::new();
        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            /// The stack size for the double fault handler's stack
            const STACK_SIZE: usize = 4096 * 5;
            /// The buffer which will be the double fault handler's stack
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

            // SAFETY:
            // This code is only run once on initialisation of TSS, and the STACK variable is never used again in rust code.
            // Therefore, it does not matter what the CPU does with the memory in STACK.
            let stack_start = VirtAddr::from_ptr(unsafe { &STACK });
            stack_start + STACK_SIZE
        };
        tss
    };
}

lazy_static! {
    static ref GDT: (GlobalDescriptorTable, Selectors) = {
        let mut gdt = GlobalDescriptorTable::new();

        // Add a kernel code segment so that the CPU stays in kernel mode
        let code_selector = gdt.add_entry(Descriptor::kernel_code_segment());
        // Add the TSS selector which contains the IST
        let tss_selector = gdt.add_entry(Descriptor::tss_segment(&TSS));
        (
            gdt,
            Selectors {
                code_selector,
                tss_selector,
            },
        )
    };
}

/// Initialises the GDT
///
/// # Safety
/// This function must only be called once.
/// All of physical memory must be mapped starting at the address given by `physical_memory_offset`.
pub unsafe fn init() {
    GDT.0.load();

    // SAFETY:
    // These SegmentSelectors are set up as valid implicitly when accessing GDT due to lazy_static
    unsafe {
        CS::set_reg(GDT.1.code_selector);
        load_tss(GDT.1.tss_selector);
    }
}
