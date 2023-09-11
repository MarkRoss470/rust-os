//! Functionality to manage the Interrupt Descriptor Table, and the PICs which provide hardware interrupts

use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

use crate::{
    cpu::{interrupt_controllers::end_interrupt, ps2::PS2_CONTROLLER},
    global_state::KERNEL_STATE,
    graphics::{Colour, WRITER}, println,
    scheduler::poll_tasks,
};

use super::{
    gdt::{DOUBLE_FAULT_STACK_INDEX, INTERRUPTS_STACK_INDEX},
    interrupt_controllers::PIC_1_OFFSET,
};

/// The Interrupt Descriptor Table
static mut IDT: Option<InterruptDescriptorTable> = None;

/// Loads the IDT structure
///
/// # Safety
/// This function may only be called once.
/// This function must be called after [`init_gdt`][super::gdt::init_gdt].
pub unsafe fn init() {
    let mut idt = InterruptDescriptorTable::new();

    // SAFETY: This function is called after `init_gdt`, so the `INTERRUPTS_STACK` is registered.
    unsafe {
        idt.invalid_tss
            .set_handler_fn(unknown_interrupt_with_error_code::<0>)
            .set_stack_index(INTERRUPTS_STACK_INDEX);
        idt.segment_not_present
            .set_handler_fn(unknown_interrupt_with_error_code::<1>)
            .set_stack_index(INTERRUPTS_STACK_INDEX);
        idt.stack_segment_fault
            .set_handler_fn(unknown_interrupt_with_error_code::<2>)
            .set_stack_index(INTERRUPTS_STACK_INDEX);
        idt.invalid_tss
            .set_handler_fn(unknown_interrupt_with_error_code::<3>)
            .set_stack_index(INTERRUPTS_STACK_INDEX);
        idt.general_protection_fault
            .set_handler_fn(general_protection_fault_handler)
            .set_stack_index(INTERRUPTS_STACK_INDEX);
        idt.alignment_check
            .set_handler_fn(unknown_interrupt_with_error_code::<5>)
            .set_stack_index(INTERRUPTS_STACK_INDEX);
        idt.vmm_communication_exception
            .set_handler_fn(unknown_interrupt_with_error_code::<6>)
            .set_stack_index(INTERRUPTS_STACK_INDEX);
        idt.security_exception
            .set_handler_fn(unknown_interrupt_with_error_code::<7>)
            .set_stack_index(INTERRUPTS_STACK_INDEX);

        idt.machine_check
            .set_handler_fn(unknown_interrupt_diverging)
            .set_stack_index(INTERRUPTS_STACK_INDEX);
    }

    for i in 0..255 {
        match i {
            8 | 10..=15 | 17 | 18 | 21..=31 => continue,
            _ => idt[i].set_handler_fn(unknown_interrupt),
        };
    }

    // SAFETY: This function is called after `init_gdt`, so the `INTERRUPTS_STACK` is registered.
    unsafe {
        idt.breakpoint
            .set_handler_fn(breakpoint_handler)
            .set_stack_index(INTERRUPTS_STACK_INDEX);
        idt.page_fault
            .set_handler_fn(page_fault_handler)
            .set_stack_index(INTERRUPTS_STACK_INDEX);
        idt.invalid_opcode
            .set_handler_fn(invalid_opcode)
            .set_stack_index(INTERRUPTS_STACK_INDEX);

        idt.double_fault
            .set_handler_fn(double_fault_handler)
            .set_stack_index(DOUBLE_FAULT_STACK_INDEX);

        // Timer interrupt
        idt[InterruptIndex::Timer.as_usize()]
            .set_handler_fn(timer_interrupt_handler)
            .set_stack_index(INTERRUPTS_STACK_INDEX);
        // Keyboard interrupt
        idt[InterruptIndex::Ps2PrimaryPort.as_usize()]
            .set_handler_fn(ps2_primary_port_handler)
            .set_stack_index(INTERRUPTS_STACK_INDEX);
        // Mouse interrupt
        idt[InterruptIndex::Ps2SecondaryPort.as_usize()]
            .set_handler_fn(ps2_secondary_port_handler)
            .set_stack_index(INTERRUPTS_STACK_INDEX);
    }

    // SAFETY: this is the only place this static is accessed, and it may only be accessed once.
    unsafe {
        IDT = Some(idt);
        IDT.as_ref().unwrap().load();
    }
}

/// The index in the IDT where different types of hardware interrupt handlers will be registered
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,
    Ps2PrimaryPort = PIC_1_OFFSET + 1,
    Ps2SecondaryPort = PIC_1_OFFSET + 2,
}

impl InterruptIndex {
    /// Get the value for a specific type of interrupt as a [`u8`]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// Get the value for a specific type of interrupt as a [`usize`]
    pub fn as_usize(self) -> usize {
        usize::from(self.as_u8())
    }
}

/// Interrupt handler for any interrupt there is not a dedicated handler for
extern "x86-interrupt" fn unknown_interrupt(stack_frame: InterruptStackFrame) {
    panic!("Unknown interrupt\nstack_frame: {stack_frame:?}");
}

/// Interrupt handler for any interrupt there is not a dedicated handler for, for interrupts with an error code
extern "x86-interrupt" fn unknown_interrupt_with_error_code<const N: usize>(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    panic!("Unknown interrupt {N} with error code\nstack_frame: {stack_frame:?}\nerror_code: {error_code:?}");
}

/// Interrupt handler for any interrupt there is not a dedicated handler for, for interrupts which do not return
extern "x86-interrupt" fn unknown_interrupt_diverging(stack_frame: InterruptStackFrame) -> ! {
    panic!("Unknown diverging interrupt\nstack_frame: {stack_frame:?}");
}

/// The interrupt handler which is called by a cpu `int3` breakpoint instruction
extern "x86-interrupt" fn breakpoint_handler(_stack_frame: InterruptStackFrame) {
    if let Some(mut lock) = WRITER.try_lock() {
        lock.set_colour(Colour::BLUE);
    }
    println!("BREAKPOINT");
    if let Some(mut lock) = WRITER.try_lock() {
        lock.set_colour(Colour::WHITE);
    }
}

/// The interrupt handler which is called when a page fault occurs,
/// when the CPU tries to access a page of virtual memory which is not mapped, or is mapped with the wrong permissions
extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;

    if let Some(mut lock) = WRITER.try_lock() {
        lock.set_colour(Colour::RED);
    }

    println!("EXCEPTION: PAGE FAULT");
    println!("Accessed Address: {:?}", Cr2::read());
    println!("Error Code: {:?}", error_code);
    println!("{:#?}", stack_frame);
    panic!("Page fault");
}

/// Interrupt handler for general protection faults
extern "x86-interrupt" fn general_protection_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    panic!("General Protection fault at {stack_frame:#?} with error code {error_code}");
}

/// The interrupt handler which is called when data is ready from the primary PS/2 port
extern "x86-interrupt" fn ps2_primary_port_handler(_stack_frame: InterruptStackFrame) {
    if let Ok(mut controller) = PS2_CONTROLLER.try_locked_if_init() {
        // SAFETY: This interrupt handler means that there is data in the primary port
        unsafe { controller.poll(super::ps2::Ps2Port::Primary) }
    }

    // SAFETY:
    // This function is a hardware interrupt handler, so it must tell the interrupt controller that the handler has completed before exiting.
    unsafe {
        end_interrupt(InterruptIndex::Ps2PrimaryPort.as_u8());
    }
}

/// The interrupt handler which is called when data is ready from the secondary PS/2 port
extern "x86-interrupt" fn ps2_secondary_port_handler(_stack_frame: InterruptStackFrame) {
    if let Ok(mut controller) = PS2_CONTROLLER.try_locked_if_init() {
        // SAFETY: This interrupt handler means that there is data in the primary port
        unsafe { controller.poll(super::ps2::Ps2Port::Secondary) }
    }

    // SAFETY:
    // This function is a hardware interrupt handler, so it must tell the interrupt controller that the handler has completed before exiting.
    unsafe { end_interrupt(InterruptIndex::Ps2SecondaryPort.as_u8()) }
}

/// The interrupt handler which is called when a double fault occurs, when a CPU exception occurs during an interrupt handler,
/// or when an interrupt is raised which does not have an associated handler.
/// If an exception happens inside the double fault handler, the CPU resets.
extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) -> ! {
    if let Ok(mut lock) = WRITER.try_locked_if_init() {
        lock.set_colour(Colour::RED);
    }

    println!("Error code: {}", error_code);
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}

/// The interrupt handler which is called for the PIC timer interrupt
extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    KERNEL_STATE.increment_ticks();

    poll_tasks();

    // SAFETY:
    // This function is a hardware interrupt handler, so it must tell the interrupt controller that the handler has completed before exiting.
    unsafe {
        end_interrupt(InterruptIndex::Timer.as_u8());
    }
}

/// Exception handler for when an invalid instruction is encountered
extern "x86-interrupt" fn invalid_opcode(stack_frame: InterruptStackFrame) {
    panic!("EXCEPTION: INVALID OPCODE\n{:#?}", stack_frame);
}

/// Tests that invoking an `int3` instruction does not panic
#[test_case]
fn test_breakpoint_no_panic() {
    x86_64::instructions::interrupts::int3();
}
