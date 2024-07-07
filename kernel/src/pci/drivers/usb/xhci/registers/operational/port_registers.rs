//! Structs for managing the port register data structures.
//!
//! See the spec section [5.4] for more info.
//!
//! [5.4]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A398%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C302%2C0%5D

use core::fmt::Debug;
use core::marker::PhantomData;

use crate::println;
use crate::util::bitfield_enum::bitfield_enum;
use crate::util::generic_mutability::{Immutable, Mutability, Mutable};

use super::super::super::{volatile_getter, volatile_setter};
use super::OperationalRegisters;

/// Power management and connection state of a USB port
#[derive(Debug, Clone, Copy)]
pub enum PortLinkState {
    /// The port is in the U0 state
    U0,
    /// The port is in the U1 state
    U1,
    /// The port is in the U2 state
    U2,
    /// The port is in the U3 state
    U3,
    /// The port is in the Disabled state
    Disabled,
    /// The port is in the RxDetect state
    RxDetect,
    /// The port is in the Inactive state
    Inactive,
    /// The port is in the Polling state
    Polling,
    /// The port is in the Recovery state
    Recovery,
    /// The port is in the HotReset state
    HotReset,
    /// The port is in the ComplianceMode state
    ComplianceMode,
    /// The port is in the TestMode state
    TestMode,
    /// The port is in the Resume state
    Resume,
}

impl PortLinkState {
    /// Parses a [`PortLinkState`] from its bit representation
    const fn from_bits(bits: u32) -> Self {
        match bits {
            0 => Self::U0,
            1 => Self::U1,
            2 => Self::U2,
            3 => Self::U3,
            4 => Self::Disabled,
            5 => Self::RxDetect,
            6 => Self::Inactive,
            7 => Self::Polling,
            8 => Self::Recovery,
            9 => Self::HotReset,
            10 => Self::ComplianceMode,
            11 => Self::TestMode,

            15 => Self::Resume,

            _ => panic!("Invalid port link state"),
        }
    }

    /// Converts a [`PortLinkState`] to its bit representation.
    /// Currently unimplemented because writing to the [`port_link_state`][StatusAndControl::port_link_state]
    /// register has significantly different semantics than reading from it.
    const fn into_bits(self) -> u32 {
        unimplemented!()
    }
}

bitfield_enum!(
    #[bitfield_enum(u32)]
    /// The status of a port indicator light
    #[derive(Debug, Clone, Copy)]
    pub enum PortIndicatorState {
        #[value(0)]
        /// The light is off
        Off,
        #[value(1)]
        /// The light is amber
        Amber,
        #[value(2)]
        /// The light is green
        Green,
        #[value(3)]
        /// The light is in an undefined state
        Undefined,
    }
);

/// Information about the power and connection status of a port.
///
/// See the spec section [5.4.8] for more info.
///
/// [5.4.8]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A412%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C386%2C0%5D
#[bitfield(u32)]
pub struct StatusAndControl {
    /// Whether a device is connected to the port.
    pub device_connected: bool,

    /// Whether the port is enabled.
    /// This field may only be set to `true` by the controller, but the OS can disable a port by writing `true`.
    /// It will be cleared to false if the port is disconnected.
    ///
    /// For USB 2 ports, a port may be re-enabled by writing `true` to [`reset`][StatusAndControl::reset].
    /// For USB 3 ports, a port may be re-enabled by writing RxDetect (TODO: link) to [`port_link_state`][StatusAndControl::port_link_state]
    pub port_enabled: bool,

    #[bits(1)]
    __: (),

    /// Whether the port currently has an over-current condition.
    pub over_current_active: bool,

    /// Writing `true` to this field initialises a reset of the port.
    /// The field will be set to `false` by the controller once the reset is complete.
    pub reset: bool,

    /// Connection state of the port.
    ///
    /// Writes to this field have no effect unless [`port_link_state_write_strobe`][StatusAndControl::port_link_state_write_strobe]
    /// is also written `true`.
    #[bits(4)]
    pub port_link_state: PortLinkState,

    /// Whether the port is powered.
    /// If this field is `false`, the port is powered off and will not respond to device connections.

    /// When writing to this field, the OS should read the field to ensure the change has been registered
    /// before writing to it again.
    ///
    ///
    /// See the spec section [4.19.4] for more info.
    ///
    /// [4.19.4]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A330%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C387%2C0%5D
    pub port_power: bool,

    /// The speed of the connected device, if one is connected.
    ///
    /// TODO: parse these properly using _Supported Protocol_ data structure (spec section 7.2.1)
    #[bits(4)]
    pub port_speed: u8,

    /// The status of port indicator lights.
    /// Has no effect if [`supports_port_indicator_control`][super::capability_registers::CapabilityParameters1::supports_port_indicator_control] is false
    #[bits(2)]
    pub port_indicator_control: PortIndicatorState,

    /// This field must be written `true` for writes to [`port_link_state`][StatusAndControl::port_link_state]
    /// to have any effect.
    pub port_link_state_write_strobe: bool,

    /// Whether a change has occurred in the [`device_connected`][StatusAndControl::device_connected] or
    /// [`cold_attach_status`][StatusAndControl::cold_attach_status] registers.
    /// Write `true` to reset to `false`.
    ///
    /// See the spec section [4.19.2] for more info on change bit usage.
    ///
    /// [4.19.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A326%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C243%2C0%5D
    pub connect_status_change: bool,

    /// Whether a change has occurred in the [`port_enabled`][StatusAndControl::port_enabled] register.
    /// Write `true` to reset to `false`.
    ///
    /// See the spec section [4.19.2] for more info on change bit usage.
    ///
    /// [4.19.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A326%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C243%2C0%5D
    pub port_enabled_change: bool,

    /// This field is set to `true` when _Warm Reset_ processing on this port completes.
    /// Write `true` to reset to `false`.
    ///
    /// See the spec section [4.19.2] for more info on change bit usage.\
    /// See the spec section [4.19.5.1] for more info on warm resets.
    ///
    /// [4.19.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A326%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C243%2C0%5D
    /// [4.19.5.1]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A335%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C485%2C0%5D
    pub warm_port_reset_change: bool,

    /// Set to `true` when [`over_current_active`][StatusAndControl::over_current_active] changes.
    /// Write `true` to reset to `false`.
    ///
    /// See the spec section [4.19.2] for more info on change bit usage.
    ///
    /// [4.19.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A326%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C243%2C0%5D
    pub over_current_change: bool,

    /// Set to `true` when [`reset`][StatusAndControl::reset] transitions from `true` to `false`,
    /// after the reset operation is complete.
    /// Write `true` to reset to `false`.
    ///
    /// See the spec section [4.19.2] for more info on change bit usage.\
    /// See the spec section [4.19.5] for more info on resets.
    ///
    /// [4.19.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A326%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C243%2C0%5D
    /// [4.19.5]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A334%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C668%2C0%5D
    pub port_reset_change: bool,

    /// Set to `true` when [`port_link_state`][StatusAndControl::port_link_state]
    /// undergoes any of the following transitions:
    ///
    /// * [`U3`] -> [`Resume`]
    /// * [`Resume`] -> [`Recovery`] -> [`U0`]
    /// * [`Resume`] -> [`U0`]
    /// * [`U3`] -> [`Recovery`] -> [`U0`]
    /// * [`U3`] -> [`U0`]
    /// * [`U2`] -> [`U0`]
    /// * [`U0`] -> [`U0`]
    /// * any state -> [`Inactive`]
    /// * any state -> [`U3`]
    ///
    /// Write `true` to reset to `false`.
    ///
    /// See the spec section [4.19.2] for more info on change bit usage.\
    /// See the spec section [4.19.1] for more info on port state transitions.
    ///
    /// [`U0`]: PortLinkState::U0
    /// [`U1`]: PortLinkState::U1
    /// [`U2`]: PortLinkState::U2
    /// [`U3`]: PortLinkState::U3
    /// [`Resume`]: PortLinkState::Resume
    /// [`Recovery`]: PortLinkState::Recovery
    /// [`Inactive`]: PortLinkState::Inactive
    ///
    /// [4.19.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A326%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C243%2C0%5D
    /// [4.19.1]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A305%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C195%2C0%5D
    pub port_link_state_change: bool,

    /// Set to `true` if the port failed to configure its link partner.
    /// Only valid for USB3 ports.
    /// Write `true` to reset to `false`.
    ///
    /// See the spec section [4.19.2] for more info on change bit usage.
    ///
    /// [4.19.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A326%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C243%2C0%5D
    pub port_config_error_change: bool,

    /// Set to `true` if a device is attached to the port but the controller is unable to establish a connection.
    ///
    /// See the spec section [4.19.8] for more info on cold attaches.
    ///
    /// [4.19.8]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A338%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C556%2C0%5D
    pub cold_attach_status: bool,

    /// Whether device connects should trigger system wake-up events.
    ///
    /// See the spec section [4.15] for more info on suspend and resume.
    ///
    /// [4.15]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A283%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C476%2C0%5D
    pub wake_on_connect: bool,

    /// Whether device disconnects should trigger system wake-up events.
    ///
    /// See the spec section [4.15] for more info on suspend and resume.
    ///
    /// [4.15]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A283%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C476%2C0%5D
    pub wake_on_disconnect: bool,

    /// Whether over current conditions should trigger system wake-up events.
    ///
    /// See the spec section [4.15] for more info on suspend and resume.
    ///
    /// [4.15]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A283%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C476%2C0%5D
    pub wake_on_over_current: bool,

    #[bits(2)]
    __: (),

    /// Whether the device in this port is non-removable.
    pub device_is_non_removable: bool,

    /// When `true` is written to this field, a warm reset is initiated on this port.
    ///
    /// See the spec section [4.19.5.1] for more info on warm resets.
    ///
    /// [4.19.5.1]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A335%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C485%2C0%5D
    pub warm_reset: bool,
}

impl StatusAndControl {
    /// Sets all `RW1C` and `RW1S` bits to 0. This means that writing this value back to the controller won't have any unintended side-effects.
    pub fn normalised(self) -> Self {
        self.with_port_enabled(false)
            .with_reset(false)
            .with_connect_status_change(false)
            .with_port_enabled_change(false)
            .with_warm_port_reset_change(false)
            .with_over_current_change(false)
            .with_port_reset_change(false)
            .with_port_link_state_change(false)
            .with_port_config_error_change(false)
            .with_warm_reset(false)
    }
}

/// The behaviour of a connection in the U0 state transitioning to the U1 state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum U1Timeout {
    /// The controller never starts or accepts transitions to U1
    Never,
    /// The controller will start a transition to U1 after this many microseconds of inactivity
    AfterTimeout(u8),
    /// The controller will not start a transition to U1 but will accept attempts
    /// by the connected device to transition to U1.
    AcceptOnly,
}

impl U1Timeout {
    /// Constructs a [`U2Timeout`] from its bit representation
    const fn from_bits(bits: u32) -> Self {
        #[allow(clippy::cast_possible_truncation)]
        let bits = bits as u8;

        match bits {
            0x00 => Self::Never,
            0x01..=0x7F => Self::AfterTimeout(bits),
            0xFF => Self::AcceptOnly,
            _ => panic!("Invalid U1 timeout value"),
        }
    }
    /// Converts a [`U2Timeout`] into its bit representation
    const fn into_bits(self) -> u32 {
        match self {
            Self::Never => 0,
            Self::AfterTimeout(ms) => ms as _,
            Self::AcceptOnly => 0xFF,
        }
    }
}

/// The behaviour of a connection in the U0 state transitioning to the U2 state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum U2Timeout {
    /// The controller never starts or accepts transitions to U2
    Never,
    /// The controller will start a transition to U2 after this many multiples of 256 microseconds of inactivity
    AfterTimeout(u8),
    /// The controller will not start a transition to U2 but will accept attempts
    /// by the connected device to transition to U2.
    AcceptOnly,
}

impl U2Timeout {
    /// Constructs a [`U1Timeout`] from its bit representation
    const fn from_bits(bits: u32) -> Self {
        #[allow(clippy::cast_possible_truncation)]
        let bits = bits as u8;

        match bits {
            0x00 => Self::Never,
            0x01..=0xFE => Self::AfterTimeout(bits),
            0xFF => Self::AcceptOnly,
        }
    }
    /// Converts a [`U1Timeout`] into its bit representation
    const fn into_bits(self) -> u32 {
        match self {
            Self::Never => 0,
            Self::AfterTimeout(ms) => ms as _,
            Self::AcceptOnly => 0xFF,
        }
    }
}

/// The _Port Power Management Status and Control Register_ of a port.
///
/// See the spec section [5.4.9] for more info.
///
/// [5.4.9]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A422%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C671%2C0%5D
#[bitfield(u32)]
pub struct PowerManagement {
    /// If the connection is in the U0 state, this field defines under what conditions the controller will
    /// initiate or accept transitions to the U1 state.
    #[bits(8)]
    pub u1_timeout: U1Timeout,

    /// If the connection is in the U0 state, this field defines under what conditions the controller will
    /// initiate or accept transitions to the U2 state.
    #[bits(8)]
    pub u2_timeout: U2Timeout,

    /// Writes to this field cause the controller to generate a _Set Link Function LMP_ with the
    /// Force_LinkPM_Accept field in the written state. This field should only be used for testing and should
    /// not be written to if there are pending packets at the link level.
    /// This field has no effect if [`port_power`][StatusAndControl::port_power] is `false`.
    pub force_link_pm_accept: bool,

    #[doc(hidden)]
    #[bits(15)]
    reserved0: u32,
}

/// The _Port Link Info Register_ of a USB 3 port.
///
/// See the spec section [5.4.10] for more info.
///
/// [5.4.10]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A425%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C279%2C0%5D
#[bitfield(u32)]
pub struct PortLinkInfo {
    /// The number of link errors detected by the port. This is reset to 0 if the controller or port is reset.
    /// In this case, an error is a transition from the U0 to Recovery state.
    error_count: u16,

    /// The number of receive lanes negotiated by the port, minus 1.
    /// This field is only valid if [`device_connected`][StatusAndControl::device_connected] is `true`.
    #[bits(4)]
    receive_lane_count: u8,

    /// The number of transmit lanes negotiated by the port, minus 1.
    /// This field is only valid if [`device_connected`][StatusAndControl::device_connected] is `true`.
    #[bits(4)]
    transmit_lane_count: u8,

    #[doc(hidden)]
    #[bits(8)]
    reserved0: u32,
}

/// The _Port Hardware LPM Control Register_ of a USB 3 port.
///
/// See the spec section [5.4.11] for more info.
///
/// [5.4.11]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A426%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C232%2C0%5D
#[bitfield(u32)]
pub struct PortHardwareLpmControl {
    /// The number of soft link errors detected by the port. This is reset to 0 if the controller or port is reset.
    /// In this case, an error is defined by section 7.3.3.2 of the USB 3.2 specification revision 1.1,
    /// which includes the following:
    ///
    /// * Single-bit error in the block header.
    /// * CRC-5 or CRC-16 or CRC-32 error.
    /// * Single symbol framing error.
    /// * Idle Symbol error.
    /// * Single SKP symbol error.
    /// * Optionally for error in the length field replica of DPH
    soft_error_count: u16,

    #[doc(hidden)]
    #[bits(16)]
    reserved0: u32,
}

/// The registers representing a port on the controller
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PortRegisterFields {
    /// Information about the power and connection status of a port.
    status_and_control: StatusAndControl,

    /// Controls the power management of the port
    power_management: PowerManagement,

    /// Information about the link state.
    /// This field is only valid if the port is USB 3, and is reserved otherwise.
    link_info: PortLinkInfo,

    /// Information about soft errors in the link.
    /// This field is only valid if the port is USB 3 and the controller supports
    /// link soft errors (LSECC = 1, TODO: link), and is reserved otherwise.
    hardware_lpm_control: PortHardwareLpmControl,
}

/// Trait to store the data needed for a [`PortRegister`] of the given mutability.
/// This is needed because a [`PortRegister`] with a mutable reference also needs a reference to the
/// [`OperationalRegisters`] in order to check that the controller is halted before writing, while the
/// immutable version only needs a [`PhantomData`] to track lifetimes.
pub trait PortRegisterMutability: Mutability {
    /// The data needed
    type OperationalRegisters<'a>;
}

impl PortRegisterMutability for Immutable {
    type OperationalRegisters<'a> = PhantomData<&'a OperationalRegisters>;
}

impl PortRegisterMutability for Mutable {
    type OperationalRegisters<'a> = &'a OperationalRegisters;
}

/// A wrapper around the [`PortRegisterFields`] which ensures that all reads are volatile.
/// Behaves like a shared reference.
pub struct PortRegister<'a, M: PortRegisterMutability> {
    /// The pointer
    ptr: M::Ptr<PortRegisterFields>,
    /// A phantom reference to the operational registers struct
    /// so that this struct can be properly borrow-checked
    operational_registers: M::OperationalRegisters<'a>,
}

impl<'a, M: PortRegisterMutability> PortRegister<'a, M> {
    /// Reads the fields of the register and prints them in a debug format
    pub fn debug(&self) {
        let fields = PortRegisterFields {
            status_and_control: self.read_status_and_control(),
            power_management: self.read_power_management(),
            link_info: self.read_link_info(),
            hardware_lpm_control: self.read_hardware_lpm_control(),
        };

        println!("{fields:#?}");
    }
}

impl<'a> PortRegister<'a, Immutable> {
    /// # Safety
    /// The pointer must be valid for reads for the duration of `'a`.
    pub unsafe fn new(ptr: *const PortRegisterFields) -> Self {
        Self {
            ptr,
            operational_registers: PhantomData,
        }
    }
}

impl<'a> PortRegister<'a, Mutable> {
    /// # Safety
    /// The pointer must be valid for reads for the duration of `'a`.
    pub unsafe fn new_mut(
        ptr: *mut PortRegisterFields,
        operational_registers: &'a OperationalRegisters,
    ) -> Self {
        Self {
            ptr,
            operational_registers,
        }
    }
}

#[rustfmt::skip]
impl<'a, M: PortRegisterMutability> PortRegister<'a, M> {
    volatile_getter!(
        PortRegister, PortRegisterFields,
        status_and_control, StatusAndControl,
        (pub fn read_status_and_control)
    );
    volatile_getter!(
        PortRegister, PortRegisterFields,
        power_management, PowerManagement,
        (pub fn read_power_management)
    );
    volatile_getter!(
        PortRegister, PortRegisterFields,
        link_info, PortLinkInfo,
        (pub fn read_link_info)
    );
    volatile_getter!(
        PortRegister, PortRegisterFields,
        hardware_lpm_control, PortHardwareLpmControl,
        (pub fn read_hardware_lpm_control)
    );
}

impl<'a> PortRegister<'a, Mutable> {
    volatile_setter!(
        PortRegister, PortRegisterFields,
        status_and_control, StatusAndControl,
        (pub unsafe fn write_status_and_control),
        |v: &PortRegister<'a, Mutable>|!v.operational_registers.read_usb_status().host_controller_halted()
    );
    volatile_setter!(
        PortRegister, PortRegisterFields,
        power_management, PowerManagement,
        (pub unsafe fn write_power_management),
        |v: &PortRegister<'a, Mutable>|!v.operational_registers.read_usb_status().host_controller_halted()
    );
    volatile_setter!(
        PortRegister, PortRegisterFields,
        link_info, PortLinkInfo,
        (pub unsafe fn write_link_info),
        |v: &PortRegister<'a, Mutable>|!v.operational_registers.read_usb_status().host_controller_halted()
    );
    volatile_setter!(
        PortRegister, PortRegisterFields,
        hardware_lpm_control, PortHardwareLpmControl,
        (pub unsafe fn write_hardware_lpm_control),
        |v: &PortRegister<'a, Mutable>|!v.operational_registers.read_usb_status().host_controller_halted()
    );

    /// Clears all `RW1C` bits in [`status_and_control`] by reading the field and writing it with the same value
    ///
    /// [`status_and_control`]: PortRegisterFields::status_and_control
    pub fn clear_status_and_control(&mut self) {
        // SAFETY: This only clears flags, which has no effect on memory safety
        unsafe { self.write_status_and_control(self.read_status_and_control()) };
    }
}

impl<'a, M: PortRegisterMutability> Debug for PortRegister<'a, M> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PortRegister")
            .field("status_and_control", &self.read_status_and_control())
            .field("power_management", &self.read_power_management())
            .field("link_info", &self.read_link_info())
            .field("hardware_lpm_control", &self.read_hardware_lpm_control())
            .finish()
    }
}
