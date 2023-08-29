//! Drivers for XHCI USB controllers. See the [XHCI spec] for more info.
//!
//! [XHCI spec]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf

// TODO: actually fix these warnings instead of ignoring them
#![allow(dead_code)]

use core::future::Pending;

use crate::{
    global_state::KERNEL_STATE,
    pci::{
        classcodes::ClassCode, devices::PciFunction,
        drivers::usb::xhci::operational_registers::OperationalRegisters, registers::HeaderType,
    },
    println,
};

use crate::pci::classcodes::{SerialBusControllerType, USBControllerType};

use capability_registers::CapabilityRegisters;
use spin::Mutex;

use self::runtime_registers::RuntimeRegisters;

pub mod capability_registers;
pub mod operational_registers;
pub mod runtime_registers;

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
}

impl XhciController {
    /// Initialises the given XHCI controller, following the process defined in the xHCI specification section [4.2]
    ///
    /// # Safety:
    /// This function may only be called once per xHCI controller
    ///
    /// [4.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A87%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C374%2C0%5D
    pub async unsafe fn init(function: PciFunction) {
        println!("{function}: Reading header");

        let header = function.get_header().unwrap().unwrap();
        let HeaderType::GeneralDevice(general_device_header) = header.header_type else {
            panic!()
        };

        assert_eq!(
            header.class_code,
            ClassCode::SerialBusController(SerialBusControllerType::UsbController(
                USBControllerType::Xhci
            ))
        );

        println!("{function}: Getting BAR");

        // SAFETY: XHCI controllers are guaranteed to have a BAR in BAR slot 0
        let bar = unsafe { general_device_header.bar(0) };

        bar.debug();

        println!("{function}: Allocating BAR");

        // SAFETY: as this function is only allowed to be called once, no other code can have seen this BAR yet
        let mmio = bar.get_frames();
        let mapped_mmio = KERNEL_STATE
            .physical_memory_accessor
            .lock()
            .map_frames(mmio);

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

        let mut controller = Self {
            function,
            capability_registers,
            operational_registers,
            runtime_registers,
        };

        println!("{function}: Sending Host Controller Reset");
        // SAFETY: The controller hasn't been set up yet so nothing is relying on the state being preserved
        unsafe {
            controller.reset_and_wait().await;
        }

        controller.enable_all_ports();

        println!(
            "{function}: Number of attached devices: {}",
            controller
                .operational_registers
                .ports()
                .filter(|port| port.read_status_and_control().device_connected())
                .count()
        );

        // let mut i = 0;

        // loop {
        //     futures::pending!();

        //     i += 1;

        //     if i % 20 == 0 {
        //         println!(
        //             "{function}: Number of attached devices: {}",
        //             controller
        //                 .operational_registers
        //                 .ports()
        //                 .filter(|port| port.read_status_and_control().device_connected())
        //                 .count()
        //         );
        //     }
        // }
    }

    /// Writes `true` to [`UsbCommand::reset`][operational_registers::UsbCommand::reset],
    /// and then waits for the controller to write `false` back, signalling the reset has complete.
    ///
    /// # Safety
    /// This function will completely reset the controller, so the caller needs to ensure no code
    /// is relying on the state of the controller being preserved.
    async unsafe fn reset_and_wait(&mut self) {
        let mut usb_command = self.operational_registers.read_usb_command();
        usb_command.set_reset(true);
        self.operational_registers.write_usb_command(usb_command);

        loop {
            futures::pending!();

            let usb_command = &self.operational_registers.read_usb_command();
            let usb_status = &self.operational_registers.read_usb_status();
            if !usb_command.reset() && !usb_status.controller_not_ready() {
                break;
            }
        }
    }

    /// Sets the value of [`max_device_slots_enabled`] to [`max_ports`].
    ///
    /// [`max_device_slots_enabled`]: operational_registers::ConfigureRegister::max_device_slots_enabled
    /// [`max_ports`]: capability_registers::StructuralParameters1::max_ports
    fn enable_all_ports(&mut self) {
        let structural_parameters_1 = self.capability_registers.structural_parameters_1();

        let mut configure_register = self.operational_registers.read_configure();
        // Set all ports to be usable
        configure_register.set_max_device_slots_enabled(structural_parameters_1.max_ports());
        self.operational_registers
            .write_configure(configure_register);
    }
}

/// Defines a safe, public getter method for a type which contains a pointer to another type,
/// using a volatile read.
/// The macro takes 5 arguments:
/// * `wrapper_struct`: The type of the wrapper struct. This type should have a field called `ptr` of type `*const field_struct` or `*mut field_struct`.
/// * `field_struct`: The type which contains the actual field being referenced.
/// * `field`: The field for which the getter and setter will be generated.
/// * `t`: The type of the field for which the getter and setter will be generated.
/// * `getter_name`: The name of the getter function.
///
/// Attributes can be inserted before `getter_name`, which will be copied to the respective functions.
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
        $getter_name: ident,

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
            pub fn $getter_name (&self) -> $t {
                assert!(($getter_check)(&self));

                // SAFETY: The pointer stored in `wrapper_struct` must always be valid or this macro is unsound
                unsafe {
                    // This reference to pointer cast also serves as a check that `$t` is actually the type of `$field`.
                    core::ptr::read_volatile(&(*self.ptr).$field as *const _)
                }
            }
    };

    (
        $wrapper_struct: ty,
        $field_struct: ty,
        $field: ident,
        $t: ty,

        $(#[$getter_attr: meta])*
        $getter_name: ident
    ) => {
        $crate::pci::drivers::usb::xhci::volatile_getter!($wrapper_struct, $field_struct, $field, $t, $(#[$getter_attr])* $getter_name, |_|true);
    }
}

/// Defines a safe, public setter method for a type which contains a pointer to another type,
/// using a volatile write.
/// The macro takes 5 arguments:
/// * `wrapper_struct`: The type of the wrapper struct. This type should have a field called `ptr` of type `*mut field_struct`.
/// * `field_struct`: The type which contains the actual field being referenced.
/// * `field`: The field for which the getter and setter will be generated.
/// * `t`: The type of the field for which the getter and setter will be generated.
/// * `setter_name`: The name of the getter function.
///
/// Attributes can be inserted before `setter_name`, which will be copied to the respective functions.
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
        $setter_name: ident,

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
            pub fn $setter_name (&mut self, value: $t) {
                assert!(($setter_check)(&self));

                // SAFETY: The pointer stored in `wrapper_struct` must always be valid or this macro is unsound
                unsafe {
                    core::ptr::write_volatile(&mut (*self.ptr).$field as *mut _, value)
                }
            }
    };

    (
        $wrapper_struct: ty,
        $field_struct: ty,
        $field: ident,
        $t: ty,

        $(#[$setter_attr: meta])*
        $setter_name: ident
    ) => {
        $crate::pci::drivers::usb::xhci::volatile_setter!($wrapper_struct, $field_struct, $field, $t, $(#[$setter_attr])* $setter_name, |_|true);
    }

}

/// Defines safe, public getter and setter methods for a type which contains a pointer to another type,
/// using volatile reads and writes.
/// The macro takes 6 arguments:
/// * `wrapper_struct`: The type of the wrapper struct. This type should have a field called `ptr` of type `*mut field_struct`.
/// * `field_struct`: The type which contains the actual field being referenced.
/// * `field`: The field for which the getter and setter will be generated.
/// * `t`: The type of the field for which the getter and setter will be generated.
/// * `getter_name`: The name of the getter function.
/// * `setter_name`: The name of the setter function.
///
/// Attributes can be inserted before `getter_name` and `setter_name`, which will be copied to the respective functions.
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
        $getter_name: ident,

        $(#[$setter_attr: meta])*
        $setter_name: ident
    ) => {
        $crate::pci::drivers::usb::xhci::volatile_getter!($wrapper_struct, $field_struct, $field, $t, $(#[$getter_attr])* $getter_name);
        $crate::pci::drivers::usb::xhci::volatile_setter!($wrapper_struct, $field_struct, $field, $t, $(#[$setter_attr])* $setter_name);
    };

    (
        $wrapper_struct: ty,
        $field_struct: ty,
        $field: ident,
        $t: ty,

        $(#[$getter_attr: meta])*
        $getter_name: ident,

        $(#[$setter_attr: meta])*
        $setter_name: ident,

        $getter_check: expr,
        $setter_check: expr
    ) => {
        $crate::pci::drivers::usb::xhci::volatile_getter!($wrapper_struct, $field_struct, $field, $t, $(#[$getter_attr])* $getter_name, $getter_check);
        $crate::pci::drivers::usb::xhci::volatile_setter!($wrapper_struct, $field_struct, $field, $t, $(#[$setter_attr])* $setter_name, $setter_check);
    };
}

use {volatile_accessors, volatile_getter, volatile_setter};
