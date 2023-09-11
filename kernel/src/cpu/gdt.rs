//! Code to register a new GDT

use x86_64::{
    registers::segmentation::{CS, Segment, SS, ES, DS},
    structures::{
        gdt::{Descriptor, GlobalDescriptorTable},
        tss::TaskStateSegment,
    },
    VirtAddr, instructions::tables::load_tss,
};

/// The size of each stack in bytes
const STACK_SIZE: usize = 5 * 4096;
/// The index into the TSS of the stack used for the [`INTERRUPTS_STACK`]
pub const INTERRUPTS_STACK_INDEX: u16 = 0;
/// The index into the TSS of the stack used for the [`DOUBLE_FAULT_STACK`]
pub const DOUBLE_FAULT_STACK_INDEX: u16 = 1;

/// An array of bytes which will be used as the stack for most interrupts and exceptions.
/// The regular kernel stack is not used so that if it becomes invalid (e.g. if it overflows)
/// then the kernel can still handle exceptions.
static mut INTERRUPTS_STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];
/// An array of bytes which will be used as the stack for the double fault exception handler.
/// This stack is separate from the regular kernel stack for the same reasons as [`INTERRUPTS_STACK`],
/// and is also separate from [`INTERRUPTS_STACK`] as additional protection against triple faults.
static mut DOUBLE_FAULT_STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

/// The _Task State Segment_ which will be loaded by the kernel.
static mut TSS: TaskStateSegment = TaskStateSegment::new();
/// The _Global Descriptor Table_ which will be loaded by the kernel.
static mut GDT: GlobalDescriptorTable = GlobalDescriptorTable::new();

/// Initialises a GDT which puts the kernel's interrupt and double fault handlers on separate stacks.
/// This means that if the kernel's main stack overflows, a triple fault does not occur.
pub unsafe fn init_gdt() {
    // SAFETY: This function is only run once and it is the only function to touch `TSS` and `GDT`
    // so no other code can be reading or modifying `TSS` and `GDT` while these references exists
    let (tss, gdt) = unsafe { (&mut TSS, &mut GDT) };

    // SAFETY: Rust code never reads or writes to `INTERRUPTS_STACK`, so the CPU can use it as a stack.
    tss.interrupt_stack_table[INTERRUPTS_STACK_INDEX as usize] = VirtAddr::from_ptr(unsafe { &INTERRUPTS_STACK }) + STACK_SIZE;
    // SAFETY: Rust code never reads or writes to `DOUBLE_FAULT_STACK`, so the CPU can use it as a stack.
    tss.interrupt_stack_table[DOUBLE_FAULT_STACK_INDEX as usize] = VirtAddr::from_ptr(unsafe { &DOUBLE_FAULT_STACK }) + STACK_SIZE;

    let code_segment = gdt.add_entry(Descriptor::kernel_code_segment());
    let data_segment = gdt.add_entry(Descriptor::kernel_data_segment());
    let tss_segment = gdt.add_entry(Descriptor::tss_segment(tss));
    gdt.load();
    
    // SAFETY: The constructed TSS is valid
    unsafe {  
        load_tss(tss_segment);
    }

    // Set the segment registers to point to the new GDT.
    // These registers are mostly unused and sometimes ignored if they are 0 but some fields still have to be valid.
    // Setting them all reduces the chances of weird memory-related bugs.
    // SAFETY: The registers point to valid entries in the GDT.
    unsafe {
        CS::set_reg(code_segment);
        DS::set_reg(data_segment);
        ES::set_reg(data_segment);
        SS::set_reg(data_segment);
    }

}
