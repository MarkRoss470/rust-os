//! Functionality to manage the Interrupt Descriptor Table, and the PICs which provide hardware interrupts

use acpica_bindings::types::{
    AcpiInterruptCallback, AcpiInterruptCallbackTag, AcpiInterruptHandledStatus,
};
use alloc::vec::Vec;
use log::{trace, warn};
use spin::Mutex;
use x86_64::{structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode}, VirtAddr};

use crate::{
    cpu::interrupt_controllers::end_interrupt,
    global_state::KERNEL_STATE,
    graphics::{flush, Colour, WRITER},
    println,
    scheduler::poll_tasks,
};
// use crate::cpu::ps2::PS2_CONTROLLER;

use super::{
    gdt::{DOUBLE_FAULT_STACK_INDEX, INTERRUPTS_STACK_INDEX},
    interrupt_controllers::PIC_1_OFFSET,
    ps2::PS2_CONTROLLER,
};

/// The Interrupt Descriptor Table
static mut IDT: Option<InterruptDescriptorTable> = None;

static ACPI_CALLBACKS: Mutex<[Vec<AcpiInterruptCallback>; 256]> = {
    const EMPTY_SET: Vec<AcpiInterruptCallback> = Vec::new();
    Mutex::new([EMPTY_SET; 256])
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackAddError {
    LockTaken,
}

pub fn register_interrupt_callback(
    interrupt_number: u8,
    callback: AcpiInterruptCallback,
) -> Result<(), CallbackAddError> {
    trace!(target: "register_interrupt_callback", "Registering callback: interrupt number: {interrupt_number:#x}");

    let mut callbacks = ACPI_CALLBACKS
        .try_lock()
        .ok_or(CallbackAddError::LockTaken)?;

    callbacks[interrupt_number as usize].push(callback);

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackRemoveError {
    LockTaken,
    NotFound,
}

/// Removes an interrupt callback which was previously registered with [`register_interrupt_callback`].
/// The callback is specified by a tag rather than by the callback itself.
pub fn remove_interrupt_callback(
    interrupt_number: u8,
    tag: AcpiInterruptCallbackTag,
) -> Result<(), CallbackRemoveError> {
    trace!(target: "remove_interrupt_callback", "Removing callback: interrupt number: {interrupt_number:#x}");

    let mut callbacks = ACPI_CALLBACKS
        .try_lock()
        .ok_or(CallbackRemoveError::LockTaken)?;

    let mut found = false;

    callbacks[interrupt_number as usize].retain(|callback| {
        if callback.is_tag(&tag) {
            found = true;
            false
        } else {
            true
        }
    });

    if !found {
        Err(CallbackRemoveError::NotFound)
    } else {
        Ok(())
    }
}

/// Registers all normal interrupt handlers (which don't take an error code) to [`unknown_interrupt`].
///
/// This is a macro rather than a normal loop because [`unknown_interrupt`] needs to take its vector number as a const generic.
/// This means each of the 200+ vectors needs its own line of code to set the interrupt.
macro_rules! register_interfaces {
    // Registers a single vector
    ($idt: expr, single, $i: expr) => {
        $idt[$i].set_handler_fn(unknown_interrupt::<{ $i }>);
    };

    // Registers ten consecutive vectors
    ($idt: expr, repeat_ten, $i: expr) => {
        register_interfaces!($idt, single, { 0 + $i });
        register_interfaces!($idt, single, { 1 + $i });
        register_interfaces!($idt, single, { 2 + $i });
        register_interfaces!($idt, single, { 3 + $i });
        register_interfaces!($idt, single, { 4 + $i });
        register_interfaces!($idt, single, { 5 + $i });
        register_interfaces!($idt, single, { 6 + $i });
        register_interfaces!($idt, single, { 7 + $i });
        register_interfaces!($idt, single, { 8 + $i });
        register_interfaces!($idt, single, { 9 + $i });
    };

    // Registers one hundred consecutive vectors
    ($idt: expr, repeat_hundred, $i: expr) => {
        register_interfaces!($idt, repeat_ten, { 00 + $i });
        register_interfaces!($idt, repeat_ten, { 10 + $i });
        register_interfaces!($idt, repeat_ten, { 20 + $i });
        register_interfaces!($idt, repeat_ten, { 30 + $i });
        register_interfaces!($idt, repeat_ten, { 40 + $i });
        register_interfaces!($idt, repeat_ten, { 50 + $i });
        register_interfaces!($idt, repeat_ten, { 60 + $i });
        register_interfaces!($idt, repeat_ten, { 70 + $i });
        register_interfaces!($idt, repeat_ten, { 80 + $i });
        register_interfaces!($idt, repeat_ten, { 90 + $i });
    };

    // Registers all non-error-code vectors
    ($idt: expr) => {
        register_interfaces!($idt, single, 0);
        register_interfaces!($idt, single, 1);
        register_interfaces!($idt, single, 2);
        register_interfaces!($idt, single, 3);
        register_interfaces!($idt, single, 4);
        register_interfaces!($idt, single, 5);
        register_interfaces!($idt, single, 6);
        register_interfaces!($idt, single, 7);

        register_interfaces!($idt, single, 9);
        register_interfaces!($idt, single, 16);
        register_interfaces!($idt, single, 19);
        register_interfaces!($idt, single, 20);

        register_interfaces!($idt, repeat_hundred, 32);
        register_interfaces!($idt, repeat_hundred, 132);

        register_interfaces!($idt, repeat_ten, 232);
        register_interfaces!($idt, repeat_ten, 242);

        register_interfaces!($idt, single, 252);
        register_interfaces!($idt, single, 253);
        register_interfaces!($idt, single, 254);
        register_interfaces!($idt, single, 255);
    };
}

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

    register_interfaces!(idt);

    // for i in 0..255 {
    //     match i {
    //         8 | 10..=15 | 17 | 18 | 21..=31 => continue,
    //         _ => idt[i].set_handler_fn(unknown_interrupt),
    //     };
    // }

    // SAFETY: This function is called after `init_gdt`, so the `INTERRUPTS_STACK` is registered.
    unsafe {
        idt.breakpoint
            .set_handler_fn(breakpoint_handler)
            .set_stack_index(INTERRUPTS_STACK_INDEX);

        idt.page_fault.set_handler_fn(page_fault_handler);
        // .set_stack_index(INTERRUPTS_STACK_INDEX);

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

/// Gets a list of all currently registered interrupt handler functions.
pub fn interrupt_handler_addresses() -> [VirtAddr; 256] {
    let mut addresses = [VirtAddr::new(0); 256];

    // SAFETY: TODO
    let idt = unsafe { IDT.as_ref().unwrap() };

    addresses[10] = idt.invalid_tss.handler_addr();
    addresses[11] = idt.segment_not_present.handler_addr();
    addresses[12] = idt.stack_segment_fault.handler_addr();
    addresses[13] = idt.general_protection_fault.handler_addr();
    addresses[17] = idt.alignment_check.handler_addr();
    addresses[29] = idt.vmm_communication_exception.handler_addr();
    addresses[30] = idt.security_exception.handler_addr();
    addresses[18] = idt.machine_check.handler_addr();

    addresses[3] = idt.breakpoint.handler_addr();
    addresses[14] = idt.page_fault.handler_addr();
    addresses[6] = idt.invalid_opcode.handler_addr();
    addresses[8] = idt.double_fault.handler_addr();

    for i in 0..=255 {
        match i {
            8 | 10..=15 | 17 | 18 | 21..=31 => continue,
            _ => addresses[i] = idt[i].handler_addr(),
        };
    }

    addresses
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

/// Interrupt handler for any interrupt there is not a dedicated handler for.
///
/// # Why a const generic interrupt number?
/// This handler covers all the unknown interrupts, but it needs to know which interrupt is currently executing
/// in order to call the right callbacks. This information is not provided by the CPU, and there is no reliable way
/// to get it from the interrupt controller. This also can't be captured using closures, because this function is
/// called by CPU internals and needs to be a function pointer rather than an [`Fn`] trait object.
///
/// The remaining solution is to make a different version of the function in memory for each interrupt served,
/// and const generics are just the tool for the job.
extern "x86-interrupt" fn unknown_interrupt<const N: u8>(_: InterruptStackFrame) {
    /// A non-generic inner function - this stops all this code being monomorphized, which would waste memory
    fn inner(interrupt: u8) {
        warn!(target: "unknown_interrupt", "Unknown interrupt - calling ACPICA callbacks {interrupt}");

        let callbacks = &mut ACPI_CALLBACKS.try_lock().unwrap()[interrupt as usize];
        callbacks.retain_mut(|callback| {
            // SAFETY: This is the correct interrupt handler
            let r = unsafe { callback.call() };
            r != AcpiInterruptHandledStatus::Handled
        });
    }

    inner(N);

    // SAFETY:
    // This function is a hardware interrupt handler, so it must tell the interrupt controller that the handler has completed before exiting.
    unsafe {
        end_interrupt(N);
    }
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
    let accessed_address = Cr2::read();
    println!("Accessed Address: {:?}", accessed_address);
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

    if KERNEL_STATE.ticks() % 2 == 0 {
        // Ignore result
        let _ = flush();
    }

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
