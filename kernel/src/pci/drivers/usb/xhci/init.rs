//! Initialisation code for an [`XhciController`]

use super::XhciController;

use crate::{
    global_state::KERNEL_STATE,
    pci::{
        bar::Bar,
        classcodes::{ClassCode, SerialBusControllerType, USBControllerType},
        drivers::usb::xhci::registers::capability::extended::ExtendedCapabilityRegisters,
        registers::{HeaderType, PciGeneralDeviceHeader},
        PciMappedFunction,
    },
};

use alloc::boxed::Box;
use log::debug;
use x86_64::VirtAddr;

use super::{
    registers::{
        capability::CapabilityRegisters,
        dcbaa::DeviceContextBaseAddressArray,
        doorbell::DoorbellRegisters,
        interrupter::Interrupter,
        operational::{
            CommandRingControl, DeviceContextBaseAddressArrayPointer, OperationalRegisters,
        },
        runtime::RuntimeRegisters,
    },
    trb::{event::command_completion::CompletionCode, CommandTrb, CommandTrbRing, EventTrb},
};

impl XhciController {
    /// Initialises the given XHCI controller, following the process defined in the xHCI specification section [4.2]
    ///
    /// # Safety:
    /// This function may only be called once per xHCI controller
    ///
    /// [4.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A87%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C374%2C0%5D
    pub async unsafe fn init(mut function: PciMappedFunction) {
        // SAFETY: This function is only called once per controller
        let (
            capability_registers,
            mut operational_registers,
            mut runtime_registers,
            doorbell_registers,
            extended_capability_registers,
        ) = unsafe { init_mmio(&function) };

        // SAFETY: The controller hasn't been set up yet so nothing is relying on the state being preserved
        unsafe {
            reset_and_wait(&mut operational_registers).await;
        }

        enable_all_ports(&capability_registers, &mut operational_registers);

        // SAFETY: The registers are valid
        let dcbaa = unsafe {
            DeviceContextBaseAddressArray::from_registers(
                &capability_registers,
                &operational_registers,
            )
        };

        let command_ring = CommandTrbRing::new();

        operational_registers.write_device_context_base_address_array_pointer(
            DeviceContextBaseAddressArrayPointer::from_pointer(dcbaa.array_addr()),
        );

        // Check that the command ring isn't running before writing pointer
        assert!(!operational_registers
            .read_command_ring_control()
            .command_ring_running());

        operational_registers.write_command_ring_control(
            CommandRingControl::new()
                .with_ring_cycle_state(true)
                .with_command_ring_pointer(command_ring.ring_start_addr()),
        );

        // SAFETY: This function is only called once per controller
        let interrupters =
            unsafe { init_interrupters(&capability_registers, &mut runtime_registers) };

        // SAFETY: This function is only called once per controller.
        // No `Bar`s exist at this point in the function.
        unsafe {
            init_msi(&mut function);
        }

        let mut controller = Self {
            function: function.function,
            capability_registers,
            extended_capability_registers,
            operational_registers,
            runtime_registers,
            dcbaa,
            command_ring,
            interrupters,
            doorbell_registers,
        };

        // Make sure `host_controller_halted` is set before starting controller
        assert!(controller
            .operational_registers
            .read_usb_status()
            .host_controller_halted());

        controller.operational_registers.write_usb_command(
            controller
                .operational_registers
                .read_usb_command()
                .with_interrupts_enabled(true) // TODO: real interrupts
                .with_wrap_events_enabled(true)
                .with_enabled(true),
        );

        // Wait for `host_controller_halted` to be unset
        // TODO: timeout
        loop {
            if !controller
                .operational_registers
                .read_usb_status()
                .host_controller_halted()
            {
                break;
            }

            futures::pending!();
        }

        controller
            .doorbell_registers
            .host_controller_doorbell()
            .ring();

        controller.test_command_ring().await;

        for mut port in controller.operational_registers.ports_mut() {
            // SAFETY: This resets the port, which has no effect on memory safety
            unsafe {
                port.write_status_and_control(port.read_status_and_control().with_reset(true));
            }
        }

        controller.main_loop().await;
    }

    /// Adds [`NoOp`] TRBs to the control ring and then waits for a response.
    /// Repeats until the command ring has wrapped 4 times.
    ///
    /// # Panics
    /// Panics if an error occurs while writing a TRB or waiting for a response
    ///
    /// [`NoOp`]: CommandTrb::NoOp
    async fn test_command_ring(&mut self) {
        for _ in 0..CommandTrbRing::TOTAL_LENGTH * 4 {
            self.test_noop().await.unwrap();
        }
    }

    /// Puts a single [`NoOp`] TRB on the command ring and waits for a [`CommandCompletion`] event TRB in response.
    ///
    /// [`NoOp`]: CommandTrb::NoOp
    /// [`CommandCompletion`]: EventTrb::CommandCompletion
    async fn test_noop(&mut self) -> Result<(), &'static str> {
        if self
            .operational_registers
            .read_usb_status()
            .host_controller_halted()
        {
            return Err("XHCI controller is not running");
        }

        if !self.operational_registers.read_usb_command().enabled() {
            return Err("XHCI controller is not running");
        }

        // SAFETY: NoOp TRBs shouldn't cause the controller to do anything other than send a CommandCompletion event
        let trb_addr = unsafe { self.write_command_trb(CommandTrb::NoOp).unwrap() };

        // Wait for controller to process TRB
        // TODO: time based timeout rather than fixed iteration count
        for _ in 0..20 {
            let read_event_trb = self.read_event_trb(0);

            match read_event_trb {
                None => (),
                Some(trb) => match trb {
                    EventTrb::CommandCompletion(trb) => {
                        if trb.command_trb_pointer != trb_addr {
                            return Err("CommandCompletion TRB points to wrong command TRB");
                        }
                        if trb.completion_code != CompletionCode::Success {
                            return Err("TRB reported non-success completion code");
                        }

                        return Ok(());
                    }
                    other => debug!("Found other TRB {other:?}"),
                },
            }

            futures::pending!();
        }

        Err("No CommandCompletion TRB found")
    }
}

/// Initialises the MMIO associated with the controller at the given function.
///
/// # Safety
/// * This function may only be called once per controller
unsafe fn init_mmio(
    function: &PciMappedFunction,
) -> (
    CapabilityRegisters,
    OperationalRegisters,
    RuntimeRegisters,
    DoorbellRegisters,
    Option<ExtendedCapabilityRegisters>,
) {
    let general_device_header = parse_header(function);

    // SAFETY: This function is only called once per controller.
    // No `Bar`s exist at this point in the function.
    let mapped_mmio = unsafe { map_mmio(general_device_header, function) };

    // SAFETY: mapped_mmio is the mapped MMIO.
    // This function is only called once per controller.
    let (capability_registers, operational_registers, runtime_registers, doorbell_registers) =
        unsafe { find_registers(mapped_mmio) };

    let extended_capability_registers = match capability_registers
        .capability_parameters_1()
        .extended_capabilities_pointer()
    {
        0 => None,
        // SAFETY: This pointer is valid for the whole lifetime of the controller
        // The offset is in 32-bit units, so multiply it by 4 to get the proper byte offset.
        offset => unsafe {
            Some(ExtendedCapabilityRegisters::new(
                mapped_mmio.as_ptr::<u32>().byte_add(offset as usize * 4),
            ))
        },
    };

    (
        capability_registers,
        operational_registers,
        runtime_registers,
        doorbell_registers,
        extended_capability_registers,
    )
}

/// Reads the header of the controller.
///
/// This function also sanity checks that the device is actually an XHCI controller.
fn parse_header(function: &PciMappedFunction) -> PciGeneralDeviceHeader {
    let header = function.read_header().unwrap().unwrap();
    let HeaderType::GeneralDevice(general_device_header) = header.header_type else {
        panic!("Device is not an XHCI controller")
    };

    assert_eq!(
        header.class_code,
        ClassCode::SerialBusController(SerialBusControllerType::UsbController(
            USBControllerType::Xhci
        )),
        "Device is not an XHCI controller"
    );

    general_device_header
}

/// Maps the MMIO range in an XHCI controller's first BAR
///
/// # Safety
/// * This function may only be called once per controller
/// * No [`Bar`] struct may exist for the device's first BAR while this function is called
unsafe fn map_mmio(
    general_device_header: PciGeneralDeviceHeader,
    function: &PciMappedFunction,
) -> VirtAddr {
    // SAFETY: XHCI controllers are guaranteed to have a BAR in BAR slot 0
    // No other `Bar` exists.
    let bar = unsafe { general_device_header.bar(function, 0) };

    let mmio = bar.get_frames();

    // SAFETY: The physical address is not used by other code as this function is only called once per controller
    let mapped_mmio = unsafe {
        KERNEL_STATE
            .physical_memory_accessor
            .try_lock()
            .unwrap()
            .map_frames(mmio)
            .start
            .start_address()
    };

    mapped_mmio
}

/// Locates the different register types in the given MMIO region.
///
/// # Safety
/// * `mapped_mmio` must be the MMIO mapping for the first BAR of the XHCI controller at `function`
/// * This function may only be called once per controller
unsafe fn find_registers(
    mapped_mmio: VirtAddr,
) -> (
    CapabilityRegisters,
    OperationalRegisters,
    RuntimeRegisters,
    DoorbellRegisters,
) {
    // SAFETY: The XHCI capability registers struct is guaranteed to be at this location in memory.
    let capability_registers = unsafe { CapabilityRegisters::new(mapped_mmio) };

    // SAFETY: The XHCI operational registers struct is guaranteed to be at this location in memory.
    let operational_registers = unsafe {
        let ptr = mapped_mmio + capability_registers.capability_register_length() as u64;

        OperationalRegisters::new(ptr, &capability_registers)
    };

    // SAFETY: The XHCI runtime registers struct is guaranteed to be at this location in memory.
    let runtime_registers = unsafe {
        let ptr = mapped_mmio + capability_registers.runtime_register_space_offset();

        RuntimeRegisters::new(ptr)
    };

    // SAFETY: The XHCI doorbell registers are guaranteed to be at this location in memory.
    // No other `DoorbellRegisters` struct exists as this function is only called once
    // The passed `max_device_slots` is accurate
    let doorbell_registers = unsafe {
        let ptr = mapped_mmio + capability_registers.doorbell_offset();

        DoorbellRegisters::new(
            ptr,
            capability_registers
                .structural_parameters_1()
                .max_device_slots()
                .into(),
        )
    };

    (
        capability_registers,
        operational_registers,
        runtime_registers,
        doorbell_registers,
    )
}

/// Writes `true` to [`UsbCommand::reset`],
/// and then waits for the controller to write `false` back, signalling the reset has complete.
///
/// # Safety
/// This function will completely reset the controller, so the caller needs to ensure no code
/// is relying on the state of the controller being preserved.
///
/// [`UsbCommand::reset`]: super::registers::operational::UsbCommand::reset
async unsafe fn reset_and_wait(operational_registers: &mut OperationalRegisters) {
    let mut usb_command = operational_registers.read_usb_command();
    usb_command.set_reset(true);
    operational_registers.write_usb_command(usb_command);

    loop {
        futures::pending!();

        let usb_command = &operational_registers.read_usb_command();
        let usb_status = &operational_registers.read_usb_status();
        if !usb_command.reset() && !usb_status.controller_not_ready() {
            break;
        }
    }
}

/// Sets the value of [`max_device_slots_enabled`] to [`max_ports`].
///
/// [`max_device_slots_enabled`]: super::registers::operational::ConfigureRegister::max_device_slots_enabled
/// [`max_ports`]: super::registers::capability::StructuralParameters1::max_ports
fn enable_all_ports(
    capability_registers: &CapabilityRegisters,
    operational_registers: &mut OperationalRegisters,
) {
    let structural_parameters_1 = capability_registers.structural_parameters_1();

    let mut configure_register = operational_registers.read_configure();
    // Set all ports to be usable
    configure_register.set_max_device_slots_enabled(structural_parameters_1.max_ports());
    operational_registers.write_configure(configure_register);
}

/// Initialises the [`Interrupter`] array in the runtime registers for this controller.
///
/// # Safety
/// This function may only be called once per controller
unsafe fn init_interrupters(
    capability_registers: &CapabilityRegisters,
    runtime_registers: &mut RuntimeRegisters,
) -> Box<[Interrupter]> {
    let max_interrupters = capability_registers
        .structural_parameters_1()
        .max_interrupters();

    (0..max_interrupters)
        .map(|i| {
            // SAFETY: This function is only called once, so no other `InterrupterRegisterSet` exists
            let mut interrupter =
                unsafe { Interrupter::new(runtime_registers.interrupter(i as _)) };

            // SAFETY: This makes sure interrupts are off for this interrupter
            unsafe {
                interrupter.registers.set_interrupter_management(
                    interrupter
                        .registers
                        .read_interrupter_management()
                        .with_interrupt_enable(false), // TODO: set to true to enable MSI
                );
            }

            interrupter
        })
        .collect()
}

/// Initialises MSI or MSI-X for an XHCI controller
///
/// # Safety
/// * This function must only be called once per controller
/// * No [`Bar`] struct may exist for the device while this function is called
unsafe fn init_msi(function: &mut PciMappedFunction) {
    let registers = function.registers.clone();
    let mut b = None;

    // SAFETY: The passed closure returns the correct BAR.
    unsafe {
        function
            .setup_msi(|i| {
                b = Some(Bar::new_from_bar_number(&registers, i));
                b.as_mut().unwrap()
            })
            .unwrap();
    }
}
