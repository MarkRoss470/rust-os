//! Functionality to manage the Interrupt Descriptor Table, and the PICs which provide hardware interrupts

use conquer_once::spin::OnceCell;
use crossbeam_queue::ArrayQueue;
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};
use pic8259::ChainedPics;
use spin::Mutex;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

use crate::{
    global_state::GlobalState,
    graphics::{Colour, WRITER},
    print, println, input::push_key,
};

/// The start of the interrupt range taken up by the first PIC.
/// 32 is chosen because it is the first free interrupt slot after the 32 CPU exceptions.
const PIC_1_OFFSET: u8 = 32;
/// The start of the interrupt range taken up by the second PIC.
const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

/// Struct in charge of controlling the two PICs which give hardware interrupts.
static PICS: Mutex<ChainedPics> = Mutex::new(
    // SAFETY:
    // These interrupt offsets do not interfere with the CPU exception range
    unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) },
);

/// The Interrupt Descriptor Table
static mut IDT: Option<InterruptDescriptorTable> = None;

/// Loads the IDT structure
/// # Safety
/// This function may only be called once
pub unsafe fn init() {
    let mut idt = InterruptDescriptorTable::new();
    idt.breakpoint.set_handler_fn(breakpoint_handler);
    idt.page_fault.set_handler_fn(page_fault_handler);
    idt.invalid_opcode.set_handler_fn(invalid_opcode);

    idt.double_fault.set_handler_fn(double_fault_handler);

    // Timer interrupt
    idt[InterruptIndex::Timer.as_usize()].set_handler_fn(timer_interrupt_handler);
    // Keyboard interrupt
    idt[InterruptIndex::Keyboard.as_usize()].set_handler_fn(keyboard_handler);

    // SAFETY: this is the only place this static is accessed, and it may only be accessed once.
    unsafe {
        IDT = Some(idt);
        IDT.as_ref().unwrap().load();
    }

    KEYBOARD.init(Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::Ignore,
    ));
}

/// # Safety
/// This function may only be called once
pub unsafe fn init_pic() {
    // SAFETY:
    // This function is only called once
    unsafe { PICS.lock().initialize() };
}

/// The index in the IDT where different types of hardware interrupt handlers will be registered
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,
    Keyboard = PIC_1_OFFSET + 1,
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

/// Notifies the interrupt controller that a handler has ended
/// # Safety
/// This function may only be called by hardware interrupt handlers, once per call, directly before the handler returns
unsafe fn end_interrupt() {
    // SAFETY:
    // These safety requirements are enforced by the caller
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
    }
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
    loop {
        x86_64::instructions::hlt();
    }
}



/// Global variable to store the state of the keyboard
/// e.g. whether shift is held
static KEYBOARD: GlobalState<Keyboard<layouts::Us104Key, ScancodeSet1>> = GlobalState::new();

/// The interrupt handler which is called when a key is pressed on the (virtual) PS/2 port
extern "x86-interrupt" fn keyboard_handler(_stack_frame: InterruptStackFrame) {
    use x86_64::instructions::port::Port;

    /// The port number to read scancodes from
    const KEYBOARD_INPUT_PORT: u16 = 0x60;

    let mut keyboard = KEYBOARD.lock();
    let mut port = Port::new(KEYBOARD_INPUT_PORT);

    // Read the scancode from the port
    // SAFETY:
    // On x86, port 0x60 reads scancodes from the (virtual) PS/2 keyboard controller
    let scancode: u8 = unsafe { port.read() };

    // Parse the scancode using the pc-keyboard crate
    if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
        if let Some(key) = keyboard.process_keyevent(key_event) {
            push_key(key);
        }
    }

    // SAFETY:
    // This function is a hardware interrupt handler, so it must tell the interrupt controller that the handler has completed before exiting.
    unsafe {
        end_interrupt();
    }
}

/// The interrupt handler which is called when a double fault occurs, when a CPU exception occurs during an interrupt handler,
/// or when an interrupt is raised which does not have an associated handler.
/// If an exception happens inside the double fault handler, the CPU resets.
extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) -> ! {
    if let Some(mut lock) = WRITER.try_lock() {
        lock.set_colour(Colour::RED);
    }

    println!("Error code: {}", error_code);
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}

/// The interrupt handler which is called for the PIC timer interrupt
extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    // SAFETY:
    // This function is a hardware interrupt handler, so it must tell the interrupt controller that the handler has completed before exiting.
    unsafe {
        end_interrupt();
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
