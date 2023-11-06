//! Code to manage different interrupt handlers

use core::{fmt::Debug, sync::atomic::AtomicU64};

use pic8259::ChainedPics;
use spin::Mutex;
use x86_64::{
    instructions::interrupts::without_interrupts,
    structures::paging::{frame::PhysFrameRange, PhysFrame},
};

use crate::{
    acpi::{
        local_apic::{self, LocalApicRegisters}, io_apic::IoApicRegisters,
    },
    cpu::idt::InterruptIndex,
    global_state::KERNEL_STATE,
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
        .acpi_cache
        .lock()
        .madt
        .as_ref()
        .ok_or(())?
        .local_apic_address();

    let local_apic_start_frame = PhysFrame::containing_address(local_apic_addr);

    let local_apic_mapping_addr =
        KERNEL_STATE
            .physical_memory_accessor
            .lock()
            .map_frames(PhysFrameRange {
                start: local_apic_start_frame,
                end: local_apic_start_frame + 1,
            });

    let local_apic_addr = local_apic_mapping_addr + (local_apic_addr.as_u64() & 4096);
    let local_apic_addr = local_apic_addr.as_mut_ptr();

    // Disable interrupts while changing controller
    // to prevent race conditions where EOI is sent to the wrong controller
    without_interrupts(|| {
        let mut local_apic =
        // SAFETY: This function is only called once per core.
        // The pointer was taken from the MADT so the APIC is definitely there.
            unsafe { LocalApicRegisters::new(local_apic_addr) };

        // local_apic.debug_registers();

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

        // SAFETY: This interrupt vector is set up to receive timer interrupts
        unsafe { local_apic.enable_timer(InterruptIndex::Timer.as_u8() as _) };

        // local_apic.debug_registers();

        *controller = InterruptController::LocalApic(local_apic);
    });

    Ok(())
}

/// Initialises the I/O APIC, and sets interrupts from PS/2 devices to be sent to this core.
///
/// # Safety
/// * This function may only be called once on the whole system,
/// unlike [`init_local_apic`] which may be called once per core.
/// * The core which this function is called on must be set up to receive interrupts from PS/2 devices
/// on their respective [`InterruptIndex`]es.
///
/// # Panics
/// If this core's local APIC is not set up, i.e. if [`init_local_apic`] hasn't been called
pub unsafe fn init_io_apic() -> Result<(), ()> {
    let acpi_cache = KERNEL_STATE.acpi_cache.lock();
    let madt = acpi_cache.madt.as_ref().ok_or(())?;

    let io_apic_addr = madt.io_apic_address();

    // SAFETY: The pointer was fetched from ACPI tables so it must be valid.
    // This function is only called once so `IoApicRegisters::new` will only be called once.
    let mut io_apic = unsafe { IoApicRegisters::new(io_apic_addr) };

    let id = match *CURRENT_CONTROLLER.lock() {
        InterruptController::None | InterruptController::Pic(_) => panic!("Local APIC not set up"),
        InterruptController::LocalApic(ref mut apic) => apic.get_id(),
    };

    // SAFETY: This core's local APIC is set up to receive interrupts.
    unsafe {
        io_apic
            .set_ps2_primary_port_interrupt(id, InterruptIndex::Ps2PrimaryPort.as_u8())
            .unwrap();
        io_apic
            .set_ps2_secondary_port_interrupt(id, InterruptIndex::Ps2SecondaryPort.as_u8())
            .unwrap();
    }

    Ok(())
}

/// Sends an interrupt to the core this function is called from with the given vector
///
/// # Safety
/// This function is for debugging purposes only and is not guaranteed to be sound.
#[allow(dead_code)]
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
