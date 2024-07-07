//! Drivers for XHCI USB controllers. See the [XHCI spec] for more info.
//!
//! [XHCI spec]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf

// TODO: actually fix these warnings instead of ignoring them
#![allow(dead_code)]

use core::cell::RefCell;

use crate::{pci::devices::PciFunction, KERNEL_STATE};

use alloc::boxed::Box;
use log::error;
use registers::capability::extended::{Capability, ExtendedCapabilityRegisters};
use tasks::TaskQueue;
use x86_64::PhysAddr;

use self::{
    registers::{
        capability::CapabilityRegisters, dcbaa::DeviceContextBaseAddressArray,
        doorbell::DoorbellRegisters, interrupter::Interrupter, operational::OperationalRegisters,
        runtime::RuntimeRegisters,
    },
    trb::{
        event::command_completion::CompletionCode, CommandTrb, CommandTrbRing, EventTrb,
        RingFullError,
    },
};

mod contexts;
mod init;
mod registers;
mod tasks;
mod trb;

/// A specific xHCI USB controller connected to the system by PCI.
pub struct XhciController {
    /// The PCI function where the controller is connected
    function: PciFunction,
    /// The controller's capability registers
    capability_registers: CapabilityRegisters,
    /// The controller's extended capability registers, if supported
    extended_capability_registers: Option<ExtendedCapabilityRegisters>,
    /// The controller's operational registers
    operational_registers: OperationalRegisters,
    /// The controller's runtime registers
    runtime_registers: RuntimeRegisters,

    /// The _Device Context Base Address Array_, which contains [`OwnedDeviceContext`]s for the controller's slots.
    ///
    /// [`OwnedDeviceContext`]: contexts::device_context::OwnedDeviceContext
    dcbaa: DeviceContextBaseAddressArray,
    /// The command TRB ring, which software uses to give instructions to the controller
    command_ring: CommandTrbRing,
    /// The controller's [`Interrupter`]s, which are used to report events to software
    interrupters: Box<[Interrupter]>,
    /// The doorbell registers, which software uses to tell the controller there are TRBs to be processed.
    doorbell_registers: DoorbellRegisters,
}

impl XhciController {
    /// Enters the main loop of the controller. This is called by [`init`] when the controller is set up.
    /// This function sets up a [`TaskQueue`] and continually polls it.
    ///
    /// [`init`]: XhciController::init
    async fn main_loop(self) -> ! {
        let s = RefCell::new(self);
        let mut tasks = TaskQueue::new(&s);
        let mut prev_ticks = KERNEL_STATE.ticks();

        loop {
            futures::pending!();

            let ticks = KERNEL_STATE.ticks();
            let tick_diff = ticks - prev_ticks;
            prev_ticks = ticks;
            let ns_since_last = tick_diff * (1_000_000_000 / 100);

            let trb = s.borrow_mut().read_event_trb(0);
            tasks.poll(ns_since_last, trb).await;
        }
    }

    /// Writes a TRB to the command ring and rings the host controller doorbell to notify the controller to process it.
    ///
    /// # Safety
    /// The caller is responsible for the behaviour of the controller in response to this TRB
    unsafe fn write_command_trb(&mut self, trb: CommandTrb) -> Result<PhysAddr, RingFullError> {
        // SAFETY: The caller is responsible for the behaviour of the controller in response to this TRB
        let trb_addr = unsafe { self.command_ring.enqueue(trb)? };

        self.doorbell_registers.host_controller_doorbell().ring();

        Ok(trb_addr)
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
                error => {
                    error!("Error occurred processing TRB: {error:?}");
                }
            }

            assert!(
                !command_completion_trb.command_trb_pointer.is_null(),
                "Command TRB pointer should not have been null"
            );

            // SAFETY: The address was read from a command completion TRB
            unsafe {
                self.command_ring
                    .update_dequeue(command_completion_trb.command_trb_pointer);
            }
        }

        Some(trb)
    }

    /// Gets an iterator over the controller's extended capabilities, if supported.
    /// 
    // TODO: should this just return an empty iterator if extended capabilities are not supported?
    fn extended_capabilities(&self) -> Option<impl Iterator<Item = Capability> + '_> {
        Some(self.extended_capability_registers.as_ref()?.capabilities())
    }
}

/// Defines a getter method for a type which contains a pointer to another type,
/// using a volatile read.
/// The macro takes 5 arguments:
/// * `wrapper_struct`: The type of the wrapper struct. This type should have a field called `ptr` which implements [`Pointer`].
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
/// [`Pointer`]: crate::util::generic_mutability::Pointer
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
                use $crate::util::generic_mutability::Pointer;

                assert!(($getter_check)(&self));
                let ptr: *const $field_struct = self.ptr.as_const_ptr();

                // SAFETY: The pointer stored in `wrapper_struct` must always be valid or this macro is unsound
                unsafe {
                    // This reference to pointer cast also serves as a check that `$t` is actually the type of `$field`.
                    core::ptr::read_volatile(core::ptr::addr_of!((*ptr).$field))
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
                let ptr: *mut $field_struct = self.ptr;

                // SAFETY: The pointer stored in `wrapper_struct` must always be valid or this macro is unsound
                unsafe {
                    core::ptr::write_volatile(core::ptr::addr_of_mut!((*ptr).$field), value)
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

/// Generates get, set, and update methods for a struct which is composed of bitfields
///
/// # Parameters
/// * `field`: The name of the field on the struct which has the methods which will be used
/// * `field_type`: The type of `field`
/// * `val`: The name of the value which is being get/set
/// * `val_type`: The type of `val`
/// * `get`, `set`, `with`: The names of the methods to get, set, and update `val`.
///     These are used both as the names of the methods on `field`, and as the names of the generated methods.
///     To omit generation of one of these methods, replace the method name with `_`.
macro_rules! update_methods {
    ($field: ident, $field_type: ty, $val: ident, $val_type: ty, $get: tt, $set: tt, $with: tt) => {
        update_methods!(#get: $field, $field_type, $val, $val_type, $get);
        update_methods!(#set: $field, $field_type, $val, $val_type, $set);
        update_methods!(#with: $field, $field_type, $val, $val_type, $with);

    };

    (#get: $field: ident, $field_type: ty, $val: ident, $val_type: ty, _) => {};
    (#get: $field: ident, $field_type: ty, $val: ident, $val_type: ty, $get: tt) => {
        #[doc = concat!("Reads the ", update_methods!(#gen_doc: $val, $field_type))]
        pub fn $get(&self) -> $val_type {
            self.$field.$get()
        }
    };

    (#set: $field: ident, $field_type: ty, $val: ident, $val_type: ty, _) => {};
    (#set: $field: ident, $field_type: ty, $val: ident, $val_type: ty, $set: tt) => {
        #[doc = concat!("Sets the ", update_methods!(#gen_doc: $val, $field_type))]
        pub fn $set(&mut self, value: $val_type) {
            self.$field.$set(value)
        }
    };

    (#with: $field: ident, $field_type: ty, $val: ident, $val_type: ty, _) => {};
    (#with: $field: ident, $field_type: ty, $val: ident, $val_type: ty, $with: tt) => {
        #[doc = concat!("Updates the ", update_methods!(#gen_doc: $val, $field_type))]
        pub fn $with(self, value: $val_type) -> Self {
            Self {
                $field: self.$field.$with(value),
                ..self
            }
        }
    };

    (#gen_doc: $val: ident, $field_type: ty) => {
        concat!(
            "[`",
            stringify!($val),
            "`] field\n\n",
            "[`",
            stringify!($val),
            "`]: ",
            stringify!($field_type),
            "::",
            stringify!($val)
        )
    }
}

use {update_methods, volatile_accessors, volatile_getter, volatile_setter};
