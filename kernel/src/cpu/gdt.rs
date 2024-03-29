//! Code to register a new GDT

use x86_64::{
    instructions::tables::load_tss,
    registers::segmentation::{Segment, CS, DS, ES, SS},
    structures::{
        gdt::{Descriptor, GlobalDescriptorTable},
        tss::TaskStateSegment,
    },
    VirtAddr,
};

/// The size of each stack in bytes
const STACK_SIZE: usize = 50 * 4096;
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

    tss.interrupt_stack_table[INTERRUPTS_STACK_INDEX as usize] =
        // SAFETY: Rust code never reads or writes to `INTERRUPTS_STACK`, so the CPU can use it as a stack.
        VirtAddr::from_ptr(unsafe { &INTERRUPTS_STACK }) + STACK_SIZE;
    tss.interrupt_stack_table[DOUBLE_FAULT_STACK_INDEX as usize] =
        // SAFETY: Rust code never reads or writes to `DOUBLE_FAULT_STACK`, so the CPU can use it as a stack.
        VirtAddr::from_ptr(unsafe { &DOUBLE_FAULT_STACK }) + STACK_SIZE;

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

/// One of the stacks used by the kernel
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stack {
    /// The stack for normal interrupt handlers
    InterruptHandler,
    /// The stack for the double fault interrupt handler
    DoubleFaultHandler,
    /// Another stack
    Other,
}

/// Gets which stack the given address falls into
pub fn get_stack(address: usize) -> Stack {
    // SAFETY: This is just getting an address, not reading or writing
    let interrupts_stack_pointer = unsafe { INTERRUPTS_STACK.as_ptr() as usize };

    // SAFETY: This is just getting an address, not reading or writing
    let double_fault_stack_pointer = unsafe { DOUBLE_FAULT_STACK.as_ptr() as usize };

    if (interrupts_stack_pointer..interrupts_stack_pointer + STACK_SIZE).contains(&address) {
        Stack::InterruptHandler
    } else if (double_fault_stack_pointer..double_fault_stack_pointer + STACK_SIZE)
        .contains(&address)
    {
        Stack::DoubleFaultHandler
    } else {
        Stack::Other
    }
}
