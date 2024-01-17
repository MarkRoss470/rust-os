//! Drivers for XHCI USB controllers. See the [XHCI spec] for more info.
//!
//! [XHCI spec]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf

// TODO: actually fix these warnings instead of ignoring them
#![allow(dead_code)]

use crate::{
    global_state::KERNEL_STATE,
    pci::{
        bar::Bar,
        classcodes::ClassCode,
        devices::PciFunction,
        drivers::usb::xhci::dcbaa::DeviceContextBaseAddressArray,
        registers::{HeaderType, PciGeneralDeviceHeader},
        PciMappedFunction,
    },
    println,
};

use crate::pci::classcodes::{SerialBusControllerType, USBControllerType};

use alloc::boxed::Box;
use capability_registers::CapabilityRegisters;
use log::debug;
use x86_64::VirtAddr;

use self::runtime_registers::RuntimeRegisters;

pub mod capability_registers;
mod dcbaa;
mod device_context;
pub mod operational_registers;
pub mod runtime_registers;
mod scratchpad;
mod trb;

/// A specific xHCI USB controller connected to the system by PCI.
#[derive(Debug)]
pub struct XhciController {
    /// The PCI function where the controller is connected
    function: PciFunction,
    /// The controller's capability registers
    capability_registers: CapabilityRegisters,
    /// The controller's operational registers
    operational_registers: OperationalRegisters,
    /// The controller's runtime registers
    runtime_registers: RuntimeRegisters,

    /// The _Device Context Base Address Array_, which contains [`DeviceContext`]s for the controller's slots.
    ///
    /// [`DeviceContext`]: device_context::DeviceContext
    dcbaa: DeviceContextBaseAddressArray,
    /// The controller's [`Interrupter`]s, which are used to report events to software
    interrupters: Box<[Interrupter]>,
    /// The doorbell registers, which software uses to tell the controller there are TRBs to be processed.
    doorbell_registers: DoorbellRegisters,
}

impl XhciController {
    /// Initialises the given XHCI controller, following the process defined in the xHCI specification section [4.2]
    ///
    /// # Safety:
    /// This function may only be called once per xHCI controller
    ///
    /// [4.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A87%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C374%2C0%5D
    pub async unsafe fn init(mut function: PciMappedFunction) {
        debug!("Initialising XHCI controller at {}", function.function);

        debug!("{}: Parsing header", function.function);
        let general_device_header = parse_header(&function);

        debug!("{}: Mapping MMIO", function.function);

        // SAFETY: This function is only called once per controller.
        // No `Bar`s exist at this point in the function.
        let mapped_mmio = unsafe { map_mmio(general_device_header, &function) };

        debug!("{}: Finding registers", function.function);

        // SAFETY: mapped_mmio is the mapped MMIO.
        // This function is only called once per controller.
        let (
            capability_registers,
            mut operational_registers,
            mut runtime_registers,
            doorbell_registers,
        ) = unsafe { Self::find_registers(mapped_mmio) };

        debug!("{}: Sending Host Controller Reset", function.function);

        // SAFETY: The controller hasn't been set up yet so nothing is relying on the state being preserved
        unsafe {
            Self::reset_and_wait(&mut operational_registers).await;
        }

        debug!("{}: Enabling ports", function.function);

        Self::enable_all_ports(&capability_registers, &mut operational_registers);

        debug!("{}: Allocating DCBAA", function.function);

        let dcbaa = {
            let len = capability_registers
                .structural_parameters_1()
                .max_device_slots()
                .into();
            let page_size = operational_registers.read_page_size();
            let uses_64_byte_context_structs = capability_registers
                .capability_parameters_1()
                .uses_64_byte_context_structs();
            let max_scratchpad_buffers = capability_registers
                .structural_parameters_2()
                .max_scratchpad_buffers()
                .into();

            // SAFETY: `len`, `page_size`, `uses_64_byte_context_struct`, and `max_scratchpad_buffers` are valid
            unsafe {
                DeviceContextBaseAddressArray::new(
                    len,
                    page_size,
                    uses_64_byte_context_structs,
                    max_scratchpad_buffers,
                )
            }
        };


        // SAFETY: This function is only called once per controller
        let interrupters =
            unsafe { Self::init_interrupters(&capability_registers, &mut runtime_registers) };

        // TODO: interrupts

        // SAFETY: This function is only called once per controller.
        // No `Bar`s exist at this point in the function.
        unsafe {
            init_msi(&mut function);
        }

        let mut controller = Self {
            function: function.function,
            capability_registers,
            operational_registers,
            runtime_registers,
            dcbaa,
            interrupters,
            doorbell_registers,
        };

        debug!("{}: Enabling controller", function.function);

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

        loop {
            for _ in 0..200 {
                futures::pending!();
            }


        loop {
            futures::pending!();

            if let Some(trb) = controller.read_event_trb(0) {
                debug!("{trb:?}");
            }
        }
    }

    /// Locates the different register types in the given MMIO region.
    ///
    /// # Safety
    /// * `mapped_mmio` must be the MMIO mapping for the first bar of the XHCI controller at `function`
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

        debug!(
            "Operational registers offset: {:#x}",
            capability_registers.capability_register_length()
        );
        debug!(
            "Runtime registers offset: {:#x}",
            capability_registers.runtime_register_space_offset()
        );
        debug!(
            "Doorbell registers offset: {:#x}",
            capability_registers.doorbell_offset()
        );

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

    /// Writes `true` to [`UsbCommand::reset`][operational_registers::UsbCommand::reset],
    /// and then waits for the controller to write `false` back, signalling the reset has complete.
    ///
    /// # Safety
    /// This function will completely reset the controller, so the caller needs to ensure no code
    /// is relying on the state of the controller being preserved.
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
    /// [`max_device_slots_enabled`]: operational_registers::ConfigureRegister::max_device_slots_enabled
    /// [`max_ports`]: capability_registers::StructuralParameters1::max_ports
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
                let interrupter =
                    unsafe { Interrupter::new(runtime_registers.interrupter(i as _)) };

                // SAFETY: This enables interrupts for this interrupter
                // unsafe {
                //     interrupter.registers.set_interrupter_management(
                //         interrupter
                //             .registers
                //             .read_interrupter_management()
                //             .with_interrupt_enable(true),
                //     );
                // }

                #[allow(clippy::let_and_return)] // TODO: remove when above code is un-commented
                interrupter
            })
            .collect()
    }


    /// Reads an event from the event ring from the `i`th interrupter.
    /// Certain event types will be intercepted and acted on before being returned, such as calling
    /// [`update_dequeue`] for [`CommandCompletion`] TRBs.
    ///
    /// [`update_dequeue`]: CommandTrbRing::update_dequeue
    /// [`CommandCompletion`]: EventTrb::CommandCompletion
    fn read_event_trb(&mut self, i: usize) -> Option<EventTrb> {
        let trb = self.interrupters[i].dequeue()?;

        if let EventTrb::CommandCompletion(command_completion_trb) = trb {
            match command_completion_trb.completion_code {
                CompletionCode::Success => (),
                error => error!("Error occurred processing TRB: {error:?}"),
            }

            // SAFETY: The address was read from a command completion TRB
            unsafe {
                self.command_ring
                    .update_dequeue(command_completion_trb.command_trb_pointer);
            }
        }

        Some(trb)
    }
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

    bar.debug();

    let mmio = bar.get_frames();

    // SAFETY: The physical address is not used by other code as this function is only called once per controller
    let mapped_mmio = unsafe {
        KERNEL_STATE
            .physical_memory_accessor
            .lock()
            .map_frames(mmio)
            .start
            .start_address()
    };

    mapped_mmio
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

/// Reads the header of the controller.
///
/// This function also sanity checks that the device is actually an XHCI controller.
fn parse_header(function: &PciMappedFunction) -> PciGeneralDeviceHeader {
    debug!(
        "PCIe registers are at physical address {:?}",
        function.registers.phys_frame
    );
    debug!("{}: Reading header", function.function);

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

/// Defines a getter method for a type which contains a pointer to another type,
/// using a volatile read.
/// The macro takes 5 arguments:
/// * `wrapper_struct`: The type of the wrapper struct. This type should have a field called `ptr` of type `*const field_struct` or `*mut field_struct`.
/// * `field_struct`: The type which contains the actual field being referenced.
/// * `field`: The field for which the getter and setter will be generated.
/// * `t`: The type of the field for which the getter and setter will be generated.
/// * `getter_signature`: The function signature of the getter function, in brackets (e.g. `(pub fn read_field)`).
///
/// Attributes can be inserted before `getter_signature`, which will be copied to the respective functions.
/// This can be used to add additional doc comments on top of the one generated by the macro, or to add other function decorations
/// such as `#[inline]` or `#[deprecated]`.
///
/// Note that the implementations of the function uses an `unsafe` block to dereference the pointer and call
/// [`read_volatile`]. This means that the pointer to `field_struct` stored in `wrapper_struct`
/// must always be valid for reads, or the generated functions will be unsound.
///
/// In order for the generated functions to compile, the macro must be invoked from a module with access to:
/// * Add inherent impls to `wrapper_struct`
/// * Access the `field` field of `field_struct`
///
/// [`read_volatile`]: core::ptr::read_volatile
macro_rules! volatile_getter {
    (
        $wrapper_struct: ty,
        $field_struct: ty,
        $field: ident,
        $t: ty,

        $(#[$getter_attr: meta])*
        ($($getter_signature: tt)+),

        $getter_check: expr
    ) => {
            #[inline]
            #[doc = concat!(
                "Performs a volatile read of the [`",
                stringify!($field),
                "`][",
                stringify!($field_struct),
                "::",
                stringify!($field),
                "] field",
            )]
            $(#[$getter_attr])*
            #[allow(clippy::redundant_closure_call)]
            $($getter_signature)+ (&self) -> $t {
                assert!(($getter_check)(&self));

                // SAFETY: The pointer stored in `wrapper_struct` must always be valid or this macro is unsound
                unsafe {
                    // This reference to pointer cast also serves as a check that `$t` is actually the type of `$field`.
                    core::ptr::read_volatile(core::ptr::addr_of!((*self.ptr).$field))
                }
            }
    };

    (
        $wrapper_struct: ty,
        $field_struct: ty,
        $field: ident,
        $t: ty,

        $(#[$getter_attr: meta])*
        ($($getter_signature: tt)+)
    ) => {
        $crate::pci::drivers::usb::xhci::volatile_getter!($wrapper_struct, $field_struct, $field, $t, $(#[$getter_attr])* ($($getter_signature)+), |_|true);
    }
}

/// Defines a setter method for a type which contains a pointer to another type,
/// using a volatile write.
/// The macro takes 5 arguments:
/// * `wrapper_struct`: The type of the wrapper struct. This type should have a field called `ptr` of type `*mut field_struct`.
/// * `field_struct`: The type which contains the actual field being referenced.
/// * `field`: The field for which the getter and setter will be generated.
/// * `t`: The type of the field for which the getter and setter will be generated.
/// * `setter_signature`: The function signature of the getter function, in brackets (e.g. `(pub unsafe fn read_field)`).
///
/// Attributes can be inserted before `setter_signature`, which will be copied to the respective functions.
/// This can be used to add additional doc comments on top of the one generated by the macro, or to add other function decorations
/// such as `#[inline]` or `#[deprecated]`.
///
/// Note that the implementations of the function uses an `unsafe` block to dereference the pointer and call
/// [`write_volatile`]. This means that the pointer to `field_struct` stored in `wrapper_struct`
/// must always be valid for writes, or the generated functions will be unsound.
///
/// In order for the generated functions to compile, the macro must be invoked from a module with access to:
/// * Add inherent impls to `wrapper_struct`
/// * Access the `field` field of `field_struct`
///
/// [`write_volatile`]: core::ptr::write_volatile
macro_rules! volatile_setter {
    (
        $wrapper_struct: ty,
        $field_struct: ty,
        $field: ident,
        $t: ty,

        $(#[$setter_attr: meta])*
        ($($setter_signature: tt)+),

        $setter_check: expr
    ) => {
            #[inline]
            #[doc = concat!(
                "Performs a volatile write of the [`",
                stringify!($field),
                "`][",
                stringify!($field_struct),
                "::",
                stringify!($field),
                "] field",
            )]
            $(#[$setter_attr])*
            #[allow(clippy::redundant_closure_call)]
            $($setter_signature)+ (&mut self, value: $t) {
                assert!(($setter_check)(&self));

                // SAFETY: The pointer stored in `wrapper_struct` must always be valid or this macro is unsound
                unsafe {
                    core::ptr::write_volatile(core::ptr::addr_of_mut!((*self.ptr).$field), value)
                }
            }
    };

    (
        $wrapper_struct: ty,
        $field_struct: ty,
        $field: ident,
        $t: ty,

        $(#[$setter_attr: meta])*
        ($($setter_signature: tt)+)
    ) => {
        $crate::pci::drivers::usb::xhci::volatile_setter!($wrapper_struct, $field_struct, $field, $t, $(#[$setter_attr])* ($($setter_signature)+), |_|true);
    }
}

/// Defines safe, public getter and setter methods for a type which contains a pointer to another type,
/// using volatile reads and writes.
/// The macro takes 6 arguments:
/// * `wrapper_struct`: The type of the wrapper struct. This type should have a field called `ptr` of type `*mut field_struct`.
/// * `field_struct`: The type which contains the actual field being referenced.
/// * `field`: The field for which the getter and setter will be generated.
/// * `t`: The type of the field for which the getter and setter will be generated.
/// * `getter_signature`: The function signature of the getter function, in brackets (e.g. `(pub fn read_field)`).
/// * `setter_signature`: The function signature of the setter function, in brackets (e.g. `(pub unsafe fn set_field)`).
///
/// Attributes can be inserted before `getter_signature` and `setter_signature`, which will be copied to the respective functions.
/// This can be used to add additional doc comments on top of the ones generated by the macro, or to add other function decorations
/// such as `#[inline]` or `#[deprecated]`.
///
/// Note that the implementations of the functions use an `unsafe` block to dereference the pointer and call either
/// [`read_volatile`] or [`write_volatile`]. This means that the pointer to `field_struct` stored in `wrapper_struct`
/// must always be valid for both reads and writes, or the generated functions will be unsound.
///
/// In order for the generated functions to compile, the macro must be invoked from a module with access to:
/// * Add inherent impls to `wrapper_struct`
/// * Access the `field` field of `field_struct`
///
/// [`read_volatile`]: core::ptr::read_volatile
/// [`write_volatile`]: core::ptr::write_volatile
macro_rules! volatile_accessors {
    (
        $wrapper_struct: ty,
        $field_struct: ty,
        $field: ident,
        $t: ty,

        $(#[$getter_attr: meta])*
        ($($getter_signature: tt)*),

        $(#[$setter_attr: meta])*
        ($($setter_signature: tt)+)
    ) => {
        $crate::pci::drivers::usb::xhci::volatile_getter!($wrapper_struct, $field_struct, $field, $t, $(#[$getter_attr])* ($($getter_signature)+));
        $crate::pci::drivers::usb::xhci::volatile_setter!($wrapper_struct, $field_struct, $field, $t, $(#[$setter_attr])* ($($setter_signature)+));
    };

    (
        $wrapper_struct: ty,
        $field_struct: ty,
        $field: ident,
        $t: ty,

        $(#[$getter_attr: meta])*
        ($($getter_signature: tt)*),

        $(#[$setter_attr: meta])*
        ($($setter_signature: tt)*),

        $getter_check: expr,
        $setter_check: expr
    ) => {
        $crate::pci::drivers::usb::xhci::volatile_getter!($wrapper_struct, $field_struct, $field, $t, $(#[$getter_attr])* ($($getter_signature)+), $getter_check);
        $crate::pci::drivers::usb::xhci::volatile_setter!($wrapper_struct, $field_struct, $field, $t, $(#[$setter_attr])* ($($setter_signature)+), $setter_check);
    };
}

use {volatile_accessors, volatile_getter, volatile_setter};
