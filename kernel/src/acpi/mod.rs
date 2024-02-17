//! Code for interacting with the [`acpica_bindings`] crate for ACPI management

pub mod io_apic;
pub mod local_apic;

use core::{convert::Infallible, sync::atomic::Ordering::Relaxed};

use acpica_bindings::{
    handler::AcpiHandler, register_interface, status::AcpiError, types::AcpiPhysicalAddress,
};
use log::{debug, info, trace};
use x86_64::{
    instructions::{hlt, port::Port},
    structures::paging::{frame::PhysFrameRange, page::PageRange, Page, PhysFrame},
    PhysAddr, VirtAddr,
};

use crate::{
    cpu::{register_interrupt_callback, remove_interrupt_callback, CallbackRemoveError},
    global_state::{KernelState, KERNEL_STATE},
    graphics::flush,
    pci, print, println,
};

/// Whether an interrupt is active high or low.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InterruptActiveState {
    /// The interrupt is sent when the signal is active
    ActiveHigh,
    /// The interrupt is sent when the signal is not active
    ActiveLow,
}

impl InterruptActiveState {
    /// Constructs an [`InterruptActiveState`] from its bit representation
    const fn from_bits_u64(bits: u64) -> Self {
        match bits {
            0 => Self::ActiveHigh,
            1 => Self::ActiveLow,
            _ => unreachable!(),
        }
    }

    /// Converts an [`InterruptActiveState`] into its bit representation
    const fn into_bits_u64(self) -> u64 {
        match self {
            Self::ActiveHigh => 0,
            Self::ActiveLow => 1,
        }
    }

    /// Constructs an [`InterruptActiveState`] from its bit representation
    const fn from_bits_u32(bits: u32) -> Self {
        match bits {
            0 => Self::ActiveHigh,
            1 => Self::ActiveLow,
            _ => unreachable!(),
        }
    }

    /// Converts an [`InterruptActiveState`] into its bit representation
    const fn into_bits_u32(self) -> u32 {
        match self {
            Self::ActiveHigh => 0,
            Self::ActiveLow => 1,
        }
    }
}

/// Whether an interrupt is edge- or level-triggered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InterruptTriggerMode {
    /// The interrupt is sent once when the signal changes
    EdgeTriggered,
    /// The interrupt is sent repeatedly until the signal changes back
    LevelTriggered,
}

impl InterruptTriggerMode {
    /// Constructs an [`InterruptTriggerMode`] from its bit representation
    const fn from_bits_u64(bits: u64) -> Self {
        match bits {
            0 => Self::EdgeTriggered,
            1 => Self::LevelTriggered,
            _ => unreachable!(),
        }
    }

    /// Converts an [`InterruptTriggerMode`] into its bit representation
    const fn into_bits_u64(self) -> u64 {
        match self {
            Self::EdgeTriggered => 0,
            Self::LevelTriggered => 1,
        }
    }

    /// Constructs an [`InterruptTriggerMode`] from its bit representation
    const fn from_bits_u32(bits: u32) -> Self {
        match bits {
            0 => Self::EdgeTriggered,
            1 => Self::LevelTriggered,
            _ => unreachable!(),
        }
    }

    /// Converts an [`InterruptTriggerMode`] into its bit representation
    const fn into_bits_u32(self) -> u32 {
        match self {
            Self::EdgeTriggered => 0,
            Self::LevelTriggered => 1,
        }
    }
}

/// An error which can occur when powering the system off
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum PowerOffError {
    /// ACPI returned an error
    Acpi(AcpiError),
    /// ACPI returned no error, but the system is still on
    DidntTurnOff,
}

impl From<AcpiError> for PowerOffError {
    fn from(value: AcpiError) -> Self {
        Self::Acpi(value)
    }
}

/// Powers the machine off by switching to sleep state 5
///
/// # Safety
/// This function should not return (unless an error occurs).
/// All running programs will be stopped and anything in RAM will be lost.
/// This function should be the last call after all other OS systems have been shut down.
pub unsafe fn power_off() -> Result<Infallible, PowerOffError> {
    let mut acpica = KERNEL_STATE.acpica.lock();

    // SAFETY: TODO
    unsafe {
        acpica.enter_sleep_state_prep(5)?;

        x86_64::instructions::interrupts::disable();

        acpica.enter_sleep_state(5)?;
    }

    // This code shouldn't be run - if execution gets here, return an error
    Err(PowerOffError::DidntTurnOff)
}

/// Initialises the [`acpica_bindings`] crate.
///
/// # Safety
/// * This function may only be called once.
/// * `rsdp_addr` must be the virtual address of the RSDP.
pub unsafe fn init(rsdp_addr: u64) {
    trace!(target: "acpi_init", "Initialising ACPICA");
    flush().unwrap();

    let acpica_initialization = register_interface(AcpiInterface { rsdp_addr }).unwrap();

    trace!(target: "acpi_init", "Initializing tables");
    flush().unwrap();

    let acpica_initialization = acpica_initialization.initialize_tables().unwrap();

    debug_tables(&acpica_initialization);

    // SAFETY: This function is only called once. The passed mcfg is provided by the BIOS / UEFI, so it is accurate.
    unsafe { pci::init(acpica_initialization.mcfg().unwrap()) }

    trace!(target: "acpi_init", "Loading tables");
    flush().unwrap();

    let acpica_initialization = acpica_initialization.load_tables().unwrap();

    trace!(target: "acpi_init", "Enabling subsystem");
    flush().unwrap();

    let acpica_initialization = acpica_initialization.enable_subsystem().unwrap();

    trace!(target: "acpi_init", "Initializing objects");
    flush().unwrap();

    let acpica_initialization = acpica_initialization.initialize_objects().unwrap();
    KERNEL_STATE.acpica.init(acpica_initialization);

    trace!(target: "acpi_init", "Done initialising ACPICA");
    flush().unwrap();
}

/// Prints out debug information about the parsed ACPI tables
fn debug_tables(
    acpica_initialization: &acpica_bindings::AcpicaOperation<true, false, false, false>,
) {
    println!();

    // for t in acpica_initialization.tables() {
    //     debug!("{t:?}");
    // }

    // println!();
    // debug!("MADT: {:?}", acpica_initialization.madt());

    // for r in acpica_initialization.madt().records() {
    //     debug!("{r:?}");
    // }

    // println!();

    for r in acpica_initialization.mcfg().unwrap().records() {
        debug!("{r:?}");
    }

    println!();
}

/// Performs a volatile, unaligned read of a value of the given size in bytes from the given address.
/// The return type can be inferred but must have the correct size.
macro_rules! read_physical {
    ($size: expr, $address: expr) => {{
        let address = PhysAddr::new($address.0.try_into().unwrap());

        // SAFETY: This read was instructed by ACPICA, so it should be sound
        let v = unsafe {
            KERNEL_STATE
                .physical_memory_accessor
                .lock()
                .with_mapping(address, $size, |ptr| {
                    let array: [u8; $size] = core::ptr::read_volatile(ptr.cast());
                    core::mem::transmute(array)
                })
        };

        v
    }};
}

/// Performs a volatile, unaligned write of a value of the given size in bytes to the given address.
macro_rules! write_physical {
    ($size: expr, $address: expr, $value: expr) => {{
        let address = PhysAddr::new($address.0.try_into().unwrap());

        // SAFETY: This write was instructed by ACPICA, so it should be sound
        unsafe {
            KERNEL_STATE
                .physical_memory_accessor
                .lock()
                .with_mapping(address, $size, |ptr| {
                    let array: [u8; $size] = $value.to_ne_bytes();
                    core::ptr::write_volatile(ptr.cast(), array);
                })
        };
    }};
}

/// The type which implements [`AcpiHandler`] in order to interact with the [`acpica_bindings`] crate
#[derive(Debug)]
struct AcpiInterface {
    /// The physical address of the RSDP
    rsdp_addr: u64,
}

// SAFETY: TODO
unsafe impl AcpiHandler for AcpiInterface {
    unsafe fn predefined_override(
        &mut self,
        predefined_object: &acpica_bindings::types::AcpiPredefinedNames,
    ) -> Result<Option<alloc::string::String>, acpica_bindings::status::AcpiError> {
        debug!(target: "predefined_override", "Name: {:?}", predefined_object.name());
        debug!(target: "predefined_override", "Object: {:?}", predefined_object.object());

        Ok(None)
    }

    fn get_root_pointer(&mut self) -> acpica_bindings::types::AcpiPhysicalAddress {
        debug!("Root pointer is {:#x}", self.rsdp_addr);
        AcpiPhysicalAddress(self.rsdp_addr as _)
    }

    unsafe fn map_memory(
        &mut self,
        physical_address: acpica_bindings::types::AcpiPhysicalAddress,
        length: usize,
    ) -> Result<*mut u8, acpica_bindings::types::AcpiMappingError> {
        // trace!(target: "map_memory", "Mapping {length} bytes from {physical_address:?}");

        let start_frame = PhysFrame::containing_address(PhysAddr::new(physical_address.0 as _));
        let num_frames = length as u64 / 4096 + 2;

        // SAFETY: The address is valid for reads
        let mapping = unsafe {
            KERNEL_STATE
                .physical_memory_accessor
                .lock()
                .map_frames(PhysFrameRange {
                    start: start_frame,
                    end: start_frame + num_frames,
                })
        };

        // SAFETY: Parameter to `add` is less than 4096 so can't overflow
        unsafe {
            Ok(mapping
                .start
                .start_address()
                .as_ptr::<u8>()
                .add(physical_address.0 % 4096)
                .cast_mut())
        }
    }

    unsafe fn unmap_memory(&mut self, address: *mut u8, length: usize) {
        // trace!(target: "unmap_memory", "Unmapping {length} bytes from {address:?}");

        let start_page = Page::containing_address(VirtAddr::new(address as _));
        let num_pages = length as u64 / 4096 + 2;

        // SAFETY: This memory was previously mapped by ACPICA and is no longer in use
        unsafe {
            KERNEL_STATE
                .physical_memory_accessor
                .lock()
                .unmap_frames(PageRange {
                    start: start_page,
                    end: start_page + num_pages,
                });
        }
    }

    fn get_physical_address(
        &mut self,
        logical_address: *mut u8,
    ) -> Result<
        Option<acpica_bindings::types::AcpiPhysicalAddress>,
        acpica_bindings::status::AcpiError,
    > {
        todo!()
    }

    unsafe fn install_interrupt_handler(
        &mut self,
        interrupt_number: u32,
        callback: acpica_bindings::types::AcpiInterruptCallback,
    ) -> Result<(), acpica_bindings::status::AcpiError> {
        register_interrupt_callback(
            interrupt_number
                .try_into()
                .expect("Interrupt number should have been <= 255"),
            callback,
        )
        .map_err(|_| AcpiError::Error)
    }

    unsafe fn remove_interrupt_handler(
        &mut self,
        interrupt_number: u32,
        callback: acpica_bindings::types::AcpiInterruptCallbackTag,
    ) -> Result<(), acpica_bindings::status::AcpiError> {
        match remove_interrupt_callback(interrupt_number.try_into().unwrap(), callback) {
            Ok(()) => Ok(()),
            Err(CallbackRemoveError::LockTaken) => panic!(),
            Err(CallbackRemoveError::NotFound) => Err(AcpiError::NotExist),
        }
    }

    fn get_thread_id(&mut self) -> u64 {
        1
    }

    fn printf(&mut self, message: core::fmt::Arguments) {
        if KERNEL_STATE.print_acpica_debug.load(Relaxed) {
            print!("{message}");
        }
    }

    unsafe fn execute(
        &mut self,
        // callback_type: AcpiExecuteType,
        callback: acpica_bindings::types::AcpiThreadCallback,
    ) -> Result<(), acpica_bindings::status::AcpiError> {
        todo!()
    }

    unsafe fn wait_for_events(&mut self) {
        todo!()
    }

    unsafe fn sleep(&mut self, millis: usize) {
        let target_kernel_ticks = KERNEL_STATE.ticks() + millis / 10;
        while KERNEL_STATE.ticks() < target_kernel_ticks {
            hlt();
        }
    }

    unsafe fn stall(&mut self, micros: usize) {
        todo!()
    }

    unsafe fn read_port_u8(
        &mut self,
        address: acpica_bindings::types::AcpiIoAddress,
    ) -> Result<u8, acpica_bindings::status::AcpiError> {
        let mut port = Port::new(address.0.try_into().unwrap());
        // SAFETY: ACPICA instructed this read so it is probably sound
        unsafe { Ok(port.read()) }
    }

    unsafe fn read_port_u16(
        &mut self,
        address: acpica_bindings::types::AcpiIoAddress,
    ) -> Result<u16, acpica_bindings::status::AcpiError> {
        let mut port = Port::new(address.0.try_into().unwrap());
        // SAFETY: ACPICA instructed this read so it is probably sound
        unsafe { Ok(port.read()) }
    }

    unsafe fn read_port_u32(
        &mut self,
        address: acpica_bindings::types::AcpiIoAddress,
    ) -> Result<u32, acpica_bindings::status::AcpiError> {
        let mut port = Port::new(address.0.try_into().unwrap());
        // SAFETY: ACPICA instructed this read so it is probably sound
        unsafe { Ok(port.read()) }
    }

    unsafe fn write_port_u8(
        &mut self,
        address: acpica_bindings::types::AcpiIoAddress,
        value: u8,
    ) -> Result<(), acpica_bindings::status::AcpiError> {
        let mut port = Port::new(address.0.try_into().unwrap());
        // SAFETY: ACPICA instructed this write so it is probably sound
        unsafe { port.write(value) };
        Ok(())
    }

    unsafe fn write_port_u16(
        &mut self,
        address: acpica_bindings::types::AcpiIoAddress,
        value: u16,
    ) -> Result<(), acpica_bindings::status::AcpiError> {
        let mut port = Port::new(address.0.try_into().unwrap());
        // SAFETY: ACPICA instructed this write so it is probably sound
        unsafe { port.write(value) };
        Ok(())
    }

    unsafe fn write_port_u32(
        &mut self,
        address: acpica_bindings::types::AcpiIoAddress,
        value: u32,
    ) -> Result<(), acpica_bindings::status::AcpiError> {
        let mut port = Port::new(address.0.try_into().unwrap());
        // SAFETY: ACPICA instructed this write so it is probably sound
        unsafe { port.write(value) };
        Ok(())
    }

    unsafe fn get_timer(&mut self) -> u64 {
        // TODO: actually implement a timer
        let timer = KERNEL_STATE.ticks() as u64 * 100_000;
        // trace!("Getting timer: {timer}");
        timer
    }

    unsafe fn read_physical_u8(&mut self, address: AcpiPhysicalAddress) -> Result<u8, AcpiError> {
        Ok(read_physical!(1, address))
    }

    unsafe fn read_physical_u16(&mut self, address: AcpiPhysicalAddress) -> Result<u16, AcpiError> {
        Ok(read_physical!(2, address))
    }

    unsafe fn read_physical_u32(&mut self, address: AcpiPhysicalAddress) -> Result<u32, AcpiError> {
        Ok(read_physical!(4, address))
    }

    unsafe fn read_physical_u64(&mut self, address: AcpiPhysicalAddress) -> Result<u64, AcpiError> {
        Ok(read_physical!(8, address))
    }

    unsafe fn write_physical_u8(
        &mut self,
        address: AcpiPhysicalAddress,
        value: u8,
    ) -> Result<(), AcpiError> {
        Ok(write_physical!(1, address, value))
    }

    unsafe fn write_physical_u16(
        &mut self,
        address: AcpiPhysicalAddress,
        value: u16,
    ) -> Result<(), AcpiError> {
        Ok(write_physical!(2, address, value))
    }

    unsafe fn write_physical_u32(
        &mut self,
        address: AcpiPhysicalAddress,
        value: u32,
    ) -> Result<(), AcpiError> {
        Ok(write_physical!(4, address, value))
    }

    unsafe fn write_physical_u64(
        &mut self,
        address: AcpiPhysicalAddress,
        value: u64,
    ) -> Result<(), AcpiError> {
        Ok(write_physical!(8, address, value))
    }

    unsafe fn readable(&mut self, pointer: *mut core::ffi::c_void, length: usize) -> bool {
        todo!()
    }

    unsafe fn writable(&mut self, pointer: *mut core::ffi::c_void, length: usize) -> bool {
        todo!()
    }

    unsafe fn read_pci_config_u8(
        &mut self,
        id: acpica_bindings::types::AcpiPciId,
        register: usize,
    ) -> Result<u8, AcpiError> {
        // SAFETY: The read is volatile
        let v = unsafe {
            crate::pci::get_pci_ptr::<u8>(
                id.segment,
                id.bus.try_into().unwrap(),
                id.device.try_into().unwrap(),
                id.function.try_into().unwrap(),
                register.try_into().unwrap(),
            )
            .read_volatile()
        };

        Ok(v)
    }

    unsafe fn read_pci_config_u16(
        &mut self,
        id: acpica_bindings::types::AcpiPciId,
        register: usize,
    ) -> Result<u16, AcpiError> {
        // SAFETY: The read is volatile
        let v = unsafe {
            crate::pci::get_pci_ptr::<u16>(
                id.segment,
                id.bus.try_into().unwrap(),
                id.device.try_into().unwrap(),
                id.function.try_into().unwrap(),
                register.try_into().unwrap(),
            )
            .read_volatile()
        };

        Ok(v)
    }

    unsafe fn read_pci_config_u32(
        &mut self,
        id: acpica_bindings::types::AcpiPciId,
        register: usize,
    ) -> Result<u32, AcpiError> {
        // SAFETY: The read is volatile
        let v = unsafe {
            crate::pci::get_pci_ptr::<u32>(
                id.segment,
                id.bus.try_into().unwrap(),
                id.device.try_into().unwrap(),
                id.function.try_into().unwrap(),
                register.try_into().unwrap(),
            )
            .read_volatile()
        };

        Ok(v)
    }

    unsafe fn read_pci_config_u64(
        &mut self,
        id: acpica_bindings::types::AcpiPciId,
        register: usize,
    ) -> Result<u64, AcpiError> {
        // SAFETY: The read is volatile
        let v = unsafe {
            crate::pci::get_pci_ptr::<u64>(
                id.segment,
                id.bus.try_into().unwrap(),
                id.device.try_into().unwrap(),
                id.function.try_into().unwrap(),
                register.try_into().unwrap(),
            )
            .read_volatile()
        };

        Ok(v)
    }

    unsafe fn write_pci_config_u8(
        &mut self,
        id: acpica_bindings::types::AcpiPciId,
        register: usize,
        value: u8,
    ) -> Result<(), AcpiError> {
        // SAFETY: The write is volatile.
        // The write is instructed by ACPI, so it should be sound
        unsafe {
            crate::pci::get_pci_ptr::<u8>(
                id.segment,
                id.bus.try_into().unwrap(),
                id.device.try_into().unwrap(),
                id.function.try_into().unwrap(),
                register.try_into().unwrap(),
            )
            .write_volatile(value)
        };

        Ok(())
    }

    unsafe fn write_pci_config_u16(
        &mut self,
        id: acpica_bindings::types::AcpiPciId,
        register: usize,
        value: u16,
    ) -> Result<(), AcpiError> {
        // SAFETY: The write is volatile.
        // The write is instructed by ACPI, so it should be sound
        unsafe {
            crate::pci::get_pci_ptr::<u16>(
                id.segment,
                id.bus.try_into().unwrap(),
                id.device.try_into().unwrap(),
                id.function.try_into().unwrap(),
                register.try_into().unwrap(),
            )
            .write_volatile(value)
        };

        Ok(())
    }

    unsafe fn write_pci_config_u32(
        &mut self,
        id: acpica_bindings::types::AcpiPciId,
        register: usize,
        value: u32,
    ) -> Result<(), AcpiError> {
        // SAFETY: The write is volatile.
        // The write is instructed by ACPI, so it should be sound
        unsafe {
            crate::pci::get_pci_ptr::<u32>(
                id.segment,
                id.bus.try_into().unwrap(),
                id.device.try_into().unwrap(),
                id.function.try_into().unwrap(),
                register.try_into().unwrap(),
            )
            .write_volatile(value)
        };

        Ok(())
    }

    unsafe fn write_pci_config_u64(
        &mut self,
        id: acpica_bindings::types::AcpiPciId,
        register: usize,
        value: u64,
    ) -> Result<(), AcpiError> {
        // SAFETY: The write is volatile.
        // The write is instructed by ACPI, so it should be sound
        unsafe {
            crate::pci::get_pci_ptr::<u64>(
                id.segment,
                id.bus.try_into().unwrap(),
                id.device.try_into().unwrap(),
                id.function.try_into().unwrap(),
                register.try_into().unwrap(),
            )
            .write_volatile(value)
        };

        Ok(())
    }

    unsafe fn signal_fatal(
        &mut self,
        _fatal_type: u32,
        _code: u32,
        _argument: u32,
    ) -> Result<(), AcpiError> {
        todo!("Fatal signal")
    }

    unsafe fn signal_breakpoint(&mut self, message: &str) -> Result<(), AcpiError> {
        info!(target: "signal_breakpoint", "Breakpoint hit: {message}");
        Ok(())
    }
}
