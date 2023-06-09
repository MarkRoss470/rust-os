//! Functionality to manage the Interrupt Descriptor Table, and the PICs which provide hardware interrupts

use alloc::string::String;
use lazy_static::lazy_static;
use pic8259::ChainedPics;
use spin::Mutex;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

use crate::{graphics::{WRITER, Colour}, println, print};

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

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt.page_fault.set_handler_fn(page_fault_handler);

        // SAFETY:
        // This stack is set up in init_gdt(), which is called before init_idt()
        unsafe {
            idt.double_fault.set_handler_fn(double_fault_handler)
                .set_stack_index(super::gdt::DOUBLE_FAULT_IST_INDEX);
        }

        // Timer interrupt
        idt[InterruptIndex::Timer.as_usize()]
            .set_handler_fn(timer_interrupt_handler);
        // Keyboard interrupt
        idt[InterruptIndex::Keyboard.as_usize()]
            .set_handler_fn(keyboard_handler);

        idt
    };
}

/// Loads the IDT structure
/// # Safety
/// This function may only be called once
pub unsafe fn init() {
    IDT.load();
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
extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    WRITER.lock().set_colour(Colour::BLUE);
    println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

/// The interrupt handler which is called when a page fault occurs,
/// when the CPU tries to access a page of virtual memory which is not mapped, or is mapped with the wrong permissions
extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;

    WRITER.lock().set_colour(Colour::RED);

    println!("EXCEPTION: PAGE FAULT");
    println!("Accessed Address: {:?}", Cr2::read());
    println!("Error Code: {:?}", error_code);
    println!("{:#?}", stack_frame);
    loop {
        x86_64::instructions::hlt();
    }
}

/// A temporary global [`String`] to test the kernel heap allocator
static INPUT_STRING: Mutex<String> = Mutex::new(String::new());

/// The interrupt handler which is called when a key is pressed on the (virtual) PS/2 port
extern "x86-interrupt" fn keyboard_handler(_stack_frame: InterruptStackFrame) {
    use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};
    use x86_64::instructions::port::Port;

    /// The port number to read scancodes from
    const KEYBOARD_INPUT_PORT: u16 = 0x60;

    // Make a global KEYBOARD variable to store the state of the keyboard
    // e.g. whether shift is held
    lazy_static! {
        static ref KEYBOARD: Mutex<Keyboard<layouts::Us104Key, ScancodeSet1>> = Mutex::new(
            Keyboard::new(layouts::Us104Key, ScancodeSet1, HandleControl::Ignore)
        );
    }

    let mut keyboard = KEYBOARD.lock();
    let mut port = Port::new(KEYBOARD_INPUT_PORT);

    // Read the scancode from the port
    // SAFETY:
    // On x86, port 0x60 reads scancodes from the (virtual) PS/2 keyboard controller
    let scancode: u8 = unsafe { port.read() };

    // Parse the scancode using the pc-keyboard crate
    if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
        if let Some(key) = keyboard.process_keyevent(key_event) {
            match key {
                DecodedKey::Unicode(character) => {
                    print!("{}", character);

                    if character == '\n' {
                        println!("You typed: {}", *INPUT_STRING.lock());
                        *INPUT_STRING.lock() = String::new();
                    }

                    INPUT_STRING.lock().push(character);
                }
                DecodedKey::RawKey(key) => print!("{:?}", key),
            }
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
    _error_code: u64,
) -> ! {
    WRITER.lock().set_colour(Colour::RED);

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

/// Tests that invoking an `int3` instruction does not panic
#[test_case]
fn test_breakpoint_no_panic() {
    x86_64::instructions::interrupts::int3();
}
