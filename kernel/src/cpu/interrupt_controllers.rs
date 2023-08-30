use core::{fmt::Debug, sync::atomic::AtomicU64};

use pic8259::ChainedPics;
use spin::Mutex;
use x86_64::{
    instructions::interrupts::without_interrupts,
    structures::paging::{frame::PhysFrameRange, PhysFrame},
};

use crate::{
    acpi::local_apic::LocalApicRegisters, cpu::idt::InterruptIndex, global_state::KERNEL_STATE,
    println,
};

/// A type of interrupt controller that the CPU can recieve interrupts from
enum InterruptController {
    /// No interrupt controller is set up
    None,
    /// Traditional 8259 PIC chip
    Pic(ChainedPics),
    /// Local APIC
    LocalApic(LocalApicRegisters),
}

impl Debug for InterruptController {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::Pic(_) => write!(f, "Pic"),
            Self::LocalApic(arg0) => f.debug_tuple("LocalApic").field(arg0).finish(),
        }
    }
}

impl InterruptController {
    /// Destructs the Interrupt controller, disabling the controller and setting the value to [`None`][InterruptController::None]
    ///
    /// # Safety
    /// It is the caller's responsibility to ensure that another interrupt controller is enabled
    /// or else the system will receive no interrupts.
    unsafe fn disable(&mut self) {
        match self {
            InterruptController::None => (),
            // SAFETY: The caller will re-enable interrupts
            InterruptController::Pic(pics) => unsafe { pics.disable() },
            InterruptController::LocalApic(_) => todo!(),
        }

        *self = Self::None;
    }
}

// SAFETY: Currently there is no multithreading.
// TODO: make CURRENT_CONTROLLER a core-local
unsafe impl Send for InterruptController {}

/// The currently enabled interrupt controller
static CURRENT_CONTROLLER: Mutex<InterruptController> = Mutex::new(InterruptController::None);
/// The number of calls to [`end_interrupt`] while the PIC was the active controller
pub static PIC_EOI: AtomicU64 = AtomicU64::new(0);
/// The number of calls to [`end_interrupt`] while the APIC was the active controller
pub static APIC_EOI: AtomicU64 = AtomicU64::new(0);

/// Sends an EOI to the [`CURRENT_CONTROLLER`]
///
/// # Safety
/// This function must be called from an interrupt handler and should be the last call before the function returns.
pub unsafe fn end_interrupt(interrupt_id: u8) {
    let mut controller = CURRENT_CONTROLLER.lock();

    match *controller {
        InterruptController::None => {
            panic!("end_interrupt called with no interrupt controller registered")
        }
        InterruptController::Pic(ref mut pics) => {
            PIC_EOI.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            // SAFETY: This function is called from an interrupt handler
            unsafe { pics.notify_end_of_interrupt(interrupt_id) }
        }
        InterruptController::LocalApic(ref mut apic) => {
            APIC_EOI.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            // SAFETY: This function is called from an interrupt handler
            unsafe { apic.notify_end_of_interrupt() }
        }
    }
}

/// The start of the interrupt range taken up by the first PIC.
/// 32 is chosen because it is the first free interrupt slot after the 32 CPU exceptions.
pub const PIC_1_OFFSET: u8 = 32;
/// The start of the interrupt range taken up by the second PIC.
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

/// # Safety
/// No other code may be handling the PIC.
pub unsafe fn init_pic() {
    let mut current_controller = CURRENT_CONTROLLER.lock();
    // SAFETY: The PIC is about to be initialised which will provide interrupt handling
    unsafe { current_controller.disable() };

    // SAFETY: The given IRQ vectors are free
    let mut pics = unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) };
    // SAFETY: Same as above
    unsafe { pics.initialize() };

    *current_controller = InterruptController::Pic(pics);
}

/// Initialises the APIC, if it's present. This function should be called after the ACPI cache is initialised.
/// This function also disables the legacy PIC.
///
/// # Safety
/// This function must only be called once per core.
pub unsafe fn init_local_apic() -> Result<(), ()> {
    println!("Getting apic addr");

    let local_apic_addr = KERNEL_STATE
        .apci_cache
        .lock()
        .madt
        .as_ref()
        .ok_or(())?
        .local_apic_address();

    let start_frame = PhysFrame::containing_address(local_apic_addr);

    println!("Mapping memory");

    let local_apic_registers_virt_addr =
        KERNEL_STATE
            .physical_memory_accessor
            .lock()
            .map_frames(PhysFrameRange {
                start: start_frame,
                end: start_frame + 2,
            });

    // Disable interrupts while changing controller
    // to prevent race conditions where EOI is sent to the wrong controller
    without_interrupts(|| {
        let mut local_apic =
        // SAFETY: 
            unsafe { LocalApicRegisters::new(local_apic_registers_virt_addr.as_mut_ptr()) };

        local_apic.debug_registers();

        let mut controller = CURRENT_CONTROLLER.lock();
        // SAFETY: The local APIC is about to be enabled so interrupts will continue to occur
        unsafe {
            controller.disable();
        }

        // SAFETY: This MSR controls whether the local APIC is enabled.
        // Setting bit 11 enables the APIC
        unsafe {
            let mut apic_reg = x86_64::registers::model_specific::Msr::new(0x1B);
            apic_reg.write(apic_reg.read() | (1 << 11))
        };

        // SAFETY: The IDT is set up so the CPU can receive interrupts.
        unsafe { local_apic.enable(0xFF) };

        // SAFETY: 
        unsafe { local_apic.enable_timer(InterruptIndex::Timer.as_u8() as _) };

        local_apic.debug_registers();

        *controller = InterruptController::LocalApic(local_apic);

        match *controller {
            InterruptController::None | InterruptController::Pic(_) => unreachable!(),
            InterruptController::LocalApic(ref mut apic) => apic.send_debug_self_interrupt(0),
        };
    });

    Ok(())
}

/// Sends an interrupt to the core this function is called from with the given vector
///
/// # Safety
/// This function is for debugging purposes only and is not guaranteed to be sound.
pub unsafe fn send_debug_self_interrupt(vector: u8) {
    let callback = match *CURRENT_CONTROLLER.lock() {
        InterruptController::None | InterruptController::Pic(_) => {
            panic!("Can't send a self interrupt unless the current controller is an APIC")
        }
        // SAFETY: For debugging only, not guaranteed to be sound
        InterruptController::LocalApic(ref mut apic) => unsafe {
            apic.send_debug_self_interrupt_delayed(vector)
        },
    };

    callback()
}
