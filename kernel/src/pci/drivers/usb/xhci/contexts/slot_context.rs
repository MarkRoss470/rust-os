//! The [`SlotContext`] type

use core::fmt::Debug;

use crate::pci::drivers::usb::RouteString;

#[bitfield(u32)]
struct SlotContextDword0 {
    #[bits(20)]
    route_string: RouteString,

    #[bits(5)]
    _reserved: (),

    multi_tt: bool,

    is_hub: bool,

    #[bits(5)]
    context_entries: u8,
}

#[bitfield(u32)]
struct SlotContextDword1 {
    max_exit_latency: u16,

    root_hub_port_number: u8,

    num_ports: u8,
}

#[bitfield(u32)]
struct SlotContextDword2 {
    parent_hub_slot_id: u8,
    parent_port_number: u8,
    #[bits(2)]
    tt_think_time: u8, // TODO: enum-ify

    #[bits(4)]
    _reserved: (),

    #[bits(10)]
    interrupter_target: u16,
}

#[bitfield(u32)]
struct SlotContextDword3 {
    usb_device_address: u8,
    #[bits(19)]
    _reserved: (),
    #[bits(5)]
    slot_state: SlotState,
}

/// The current state of a Device Slot.
///
/// See the spec section [4.5.3] for the definition of this type.
///
/// The `Disabled` state is not part of this enum because it has the same binary representation as [`Enabled`].
/// This enum only represents the state of an enabled _Device Slot_.
///
/// [4.5.3]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A104%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C219%2C0%5D
/// [`Enabled`]: SlotState::Enabled
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotState {
    /// The _Device Slot_ has been allocated to software by the [`EnableSlot`] Command,
    /// however the Doorbell register for the slot is not enabled and the pointer to the slot’s
    /// Output [`OwnedDeviceContext`] in the [`DeviceContextBaseAddressArray`] is invalid.
    ///
    /// The only commands that software is allowed to issue for a slot in this state are [`AddressDevice`] and [`DisableSlot`].
    ///
    /// See the spec section [4.5.3.3] for more info.
    ///
    /// [`EnableSlot`]: super::super::trb::command::CommandTrb::EnableSlot
    /// [`OwnedDeviceContext`]: super::device_context::OwnedDeviceContext
    /// [`DeviceContextBaseAddressArray`]: super::super::dcbaa::DeviceContextBaseAddressArray
    ///
    /// [`AddressDevice`]: super::super::trb::command::CommandTrb::AddressDevice
    /// [`DisableSlot`]: super::super::trb::command::CommandTrb::DisableSlot
    ///
    /// [4.5.3.3]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A106%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C219%2C0%5D
    Enabled,

    /// The USB device is in the `Default` state, the pointer to the _Device Slot’s_
    /// Output [`OwnedDeviceContext`] in the [`DeviceContextBaseAddressArray`] is valid,
    /// the [`SlotContext`] and [`EndpointContext`] 0 in the Output Device Context have
    /// been initialized by the xHC, and the Doorbell register for the slot is enabled only
    /// for `DB Target = Control EP 0 Enqueue Pointer Update`.
    ///
    /// The only commands that software is allowed to issue for the slot in this state are the [`AddressDevice`] (BSR = 0),
    /// [`ResetEndpoint`], [`StopEndpoint`], [`EvaluateContext`], [`SetTRDequeuePointer`],
    /// and [`DisableSlot`].
    ///
    /// See the spec section [4.5.3.4] for more info.
    ///
    /// [`OwnedDeviceContext`]: super::device_context::OwnedDeviceContext
    /// [`DeviceContextBaseAddressArray`]: super::super::dcbaa::DeviceContextBaseAddressArray
    /// [`EndpointContext`]: super::endpoint_context::EndpointContext
    ///
    /// [`AddressDevice`]: super::super::trb::command::CommandTrb::AddressDevice
    /// [`ResetEndpoint`]: super::super::trb::command::CommandTrb::ResetEndpoint
    /// [`StopEndpoint`]: super::super::trb::command::CommandTrb::StopEndpoint
    /// [`EvaluateContext`]: super::super::trb::command::CommandTrb::EvaluateContext
    /// [`SetTRDequeuePointer`]: super::super::trb::command::CommandTrb::SetTRDequeuePointer
    /// [`DisableSlot`]: super::super::trb::command::CommandTrb::DisableSlot
    ///
    /// [4.5.3.4]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A107%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C377%2C0%5D
    Default,

    /// The USB device is in the `Address` state, the pointer to the _Device Slot’s_
    /// Output [`OwnedDeviceContext`] in the [`DeviceContextBaseAddressArray`] is valid,
    /// the [`SlotContext`] and [`EndpointContext`] 0 in the Output Device Context have
    /// been initialized by the xHC, and the Doorbell register for the slot is enabled only
    /// for `DB Target = Control EP 0 Enqueue Pointer Update`.
    ///
    /// The only commands that software is allowed to issue for the slot in this state are the [`EvaluateContext`],
    /// [`ConfigureEndpoint`], [`ResetEndpoint`], [`StopEndpoint`], [`NegotiateBandwidth`],
    /// [`SetTRDequeuePointer`], [`ResetDevice`], and [`DisableSlot`].
    ///
    /// See the spec section [4.5.3.5] for more info.
    ///
    /// [`OwnedDeviceContext`]: super::device_context::OwnedDeviceContext
    /// [`DeviceContextBaseAddressArray`]: super::super::dcbaa::DeviceContextBaseAddressArray
    /// [`EndpointContext`]: super::endpoint_context::EndpointContext
    ///
    /// [`EvaluateContext`]: super::super::trb::command::CommandTrb::EvaluateContext
    /// [`ConfigureEndpoint`]: super::super::trb::command::CommandTrb::ConfigureEndpoint
    /// [`ResetEndpoint`]: super::super::trb::command::CommandTrb::ResetEndpoint
    /// [`StopEndpoint`]: super::super::trb::command::CommandTrb::StopEndpoint
    /// [`NegotiateBandwidth`]: super::super::trb::command::CommandTrb::NegotiateBandwidth
    /// [`SetTRDequeuePointer`]: super::super::trb::command::CommandTrb::SetTRDequeuePointer
    /// [`ResetDevice`]: super::super::trb::command::CommandTrb::ResetDevice
    /// [`DisableSlot`]: super::super::trb::command::CommandTrb::DisableSlot
    ///
    /// [4.5.3.5]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A108%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C632%2C0%5D
    Addressed,

    /// the USB device is in the `Configured` state, the pointer to the
    /// _Device Slot’s_ Output [`OwnedDeviceContext`] in the [`DeviceContextBaseAddressArray`] is
    /// valid, the [`SlotContext`], [`EndpointContext`] 0, and enabled IN and OUT [`EndpointContext`]s
    /// between 1 and 15 in the Output Device Context have been initialized
    /// by the xHC, and the _Device Context doorbell_ for the slot is enabled for
    /// `DB Target = Control EP 0 Enqueue Pointer Update` and any enabled endpoint.
    ///
    /// The only commands that software is allowed to issue for the slot in this state are the
    /// [`ConfigureEndpoint`] (DC = ‘0’ or ‘1’), [`ResetEndpoint`], [`StopEndpoint`],
    /// [`SetTRDequeuePointer`], [`EvaluateContext`], [`ResetDevice`], [`NegotiateBandwidth`], and
    /// [`DisableSlot`].
    ///
    /// See the spec section [4.5.3.6] for more info.
    ///
    /// [`OwnedDeviceContext`]: super::device_context::OwnedDeviceContext
    /// [`DeviceContextBaseAddressArray`]: super::super::dcbaa::DeviceContextBaseAddressArray
    /// [`EndpointContext`]: super::endpoint_context::EndpointContext
    ///
    /// [`ResetEndpoint`]: super::super::trb::command::CommandTrb::ResetEndpoint
    /// [`StopEndpoint`]: super::super::trb::command::CommandTrb::StopEndpoint
    /// [`ConfigureEndpoint`]: super::super::trb::command::CommandTrb::ConfigureEndpoint
    /// [`SetTRDequeuePointer`]: super::super::trb::command::CommandTrb::SetTRDequeuePointer
    /// [`EvaluateContext`]: super::super::trb::command::CommandTrb::EvaluateContext
    /// [`ResetDevice`]: super::super::trb::command::CommandTrb::ResetDevice
    /// [`NegotiateBandwidth`]: super::super::trb::command::CommandTrb::NegotiateBandwidth
    /// [`DisableSlot`]: super::super::trb::command::CommandTrb::DisableSlot
    ///
    /// [4.5.3.6]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A108%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C184%2C0%5D
    Configured,

    /// Reserved
    Reserved(u8),
}

impl SlotState {
    /// Constructs a [`SlotState`] from its bit representation
    const fn from_bits(bits: u32) -> Self {
        #[allow(clippy::cast_possible_truncation)]
        let bits = bits as u8;

        match bits {
            0 => Self::Enabled,
            1 => Self::Default,
            2 => Self::Addressed,
            3 => Self::Configured,
            _ => Self::Reserved(bits),
        }
    }

    /// Converts a [`SlotState`] into its bit representation
    const fn into_bits(self) -> u32 {
        match self {
            Self::Enabled => 0,
            Self::Default => 1,
            Self::Addressed => 2,
            Self::Configured => 3,
            Self::Reserved(bits) => bits as u32,
        }
    }
}

/// Defines information relating to a device as a whole.
#[repr(C)]
pub struct SlotContext {
    /// The first DWORD
    dword_0: SlotContextDword0,
    /// The second DWORD
    dword_1: SlotContextDword1,
    /// The third DWORD
    dword_2: SlotContextDword2,
    /// The fourth DWORD
    dword_3: SlotContextDword3,
}

impl SlotContext {
    /// This field is used by hubs to route packets to the correct downstream port.
    pub fn route_string(&self) -> RouteString {
        self.dword_0.route_string()
    }

    /// Whether the _Multiple TT_ interface is enabled for this device or any of its parent hubs.
    pub fn multi_tt(&self) -> bool {
        self.dword_0.multi_tt()
    }

    /// Whether the device is a hub
    pub fn is_hub(&self) -> bool {
        self.dword_0.is_hub()
    }

    /// The index into the Device Context of the last valid [`EndpointContext`].
    /// Valid values are in the range `1..=32`.
    ///
    /// [`EndpointContext`]: super::endpoint_context::EndpointContext
    pub fn context_entries(&self) -> u8 {
        self.dword_0.context_entries()
    }

    /// The worst case time in microseconds it could take to wake up all the links in the path to the device,
    /// given the current USB link level power management settings.
    ///
    /// See the spec section [4.23.5.2] for more info.
    ///
    /// [4.23.5.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A363%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C637%2C0%5D
    pub fn max_exit_latency(&self) -> u16 {
        self.dword_1.max_exit_latency()
    }

    /// The root hub port used to access this device.
    ///
    /// See the spec section [4.19.7] for more info on port numbering.
    ///
    /// [4.19.7]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A336%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C370%2C0%5D
    pub fn root_hub_port_number(&self) -> u8 {
        self.dword_1.root_hub_port_number()
    }

    /// If this device [is a hub], then this field is set by software to identify the number of downstream facing ports supported by the hub.
    /// If this device is not a hub, then this field shall be 0.
    ///
    /// See the `bNbrPorts` field in the [USB3 spec] section 10.15.2.1 for more info.
    ///
    /// [is a hub]: SlotContext::is_hub
    /// [USB3 spec]: https://www.usb.org/document-library/usb-32-revision-11-june-2022
    pub fn num_ports(&self) -> u8 {
        self.dword_1.num_ports()
    }

    /// If this device is Low-/Full-speed and connected through a High-speed hub,
    /// then this field shall contain the Slot ID of the parent High-speed hub.
    ///
    /// For SS and SSP bus instance, if this device is connected through a higher rank hub then this
    /// field shall contain the Slot ID of the parent hub. For example, a Gen1 x1 connected behind a
    /// Gen1 x2 hub, or Gen1 x2 device connected behind Gen2 x2 hub.
    ///
    /// This field shall be 0 if any of the following are true:
    /// * Device is attached to a Root Hub port
    /// * Device is a High-Speed device
    /// * Device is the highest rank SS/SSP device supported by xHCI
    pub fn parent_hub_slot_id(&self) -> u8 {
        self.dword_2.parent_hub_slot_id()
    }

    /// The number of the downstream facing port of the parent hub. See [`parent_hub_slot_id`] for when this field is valid and when it is 0.
    ///
    /// [`parent_hub_slot_id`]: SlotContext::parent_hub_slot_id
    pub fn parent_port_number(&self) -> u8 {
        self.dword_2.parent_port_number()
    }

    /// If this is a High-speed hub ([`is_hub`] = ‘1’ and Speed = High-Speed), then this field shall be set by software
    /// to identify the time the TT of the hub requires to proceed to the next full-/low-speed transaction.
    /// Otherwise, the field should be 0.
    ///
    /// TODO: The speed field is deprecated but still referred to here: should it be used?
    ///
    /// # Valid values
    ///
    /// | Value     | Think Time                                                                                       |
    /// |:----------|:-------------------------------------------------------------------------------------------------|
    /// | 0         | TT requires at most 8 FS bit times of inter-transaction gap on a full-/low-speed downstream bus. |
    /// | 1         | TT requires at most 16 FS bit times.                                                             |
    /// | 2         | TT requires at most 24 FS bit times.                                                             |
    /// | 3         | TT requires at most 32 FS bit times.                                                             |
    ///
    /// See the [USB2 spec] section 11.23.2.1 for the definition of this field (as a sub-field of the wHubCharacteristics field)
    ///
    /// [`is_hub`]: SlotContext::is_hub
    /// [USB2 spec]: https://www.usb.org/document-library/usb-20-specification
    pub fn tt_think_time(&self) -> u8 {
        self.dword_2.tt_think_time()
    }

    /// This field defines the index of the Interrupter that will receive [`BandwidthRequest`] and [`DeviceNotification`]
    /// events generated by this slot, or when a [`RingUnderrun`] or [`RingOverrun`] condition is reported
    /// Valid values are between 0 and [`max_interrupters`] - 1.
    ///
    /// See the spec section [4.10.3.1] for more info on [`RingOverrun`] and [`RingUnderrun`] conditions.
    ///
    /// [`BandwidthRequest`]: super::super::trb::event::EventTrb::BandwidthRequest
    /// [`DeviceNotification`]: super::super::trb::event::EventTrb::DeviceNotification
    /// [`RingUnderrun`]: super::super::trb::event::command_completion::CompletionError::RingUnderrun
    /// [`RingOverrun`]: super::super::trb::event::command_completion::CompletionError::RingOverrun
    /// [`max_interrupters`]: super::super::capability_registers::StructuralParameters1::max_interrupters
    /// [4.10.3.1]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A211%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C389%2C0%5D
    pub fn interrupter_target(&self) -> u16 {
        self.dword_2.interrupter_target()
    }

    /// The address assigned to the USB device by the xHC.
    /// This field is set upon the successful completion of a Set Address command.
    /// (TODO: link? This is a USB command not XHCI so there might not be anything to link to)
    ///
    /// This field is invalid if the slot is disabled or [`slot_state`] is [`Default`].
    ///
    /// See the [USB2 spec] section 9.4.6 for more info on Set Address commands.
    ///
    /// [USB2 spec]: https://www.usb.org/document-library/usb-20-specification
    /// [`slot_state`]: SlotContext::slot_state
    /// [`Default`]: SlotState::Default
    pub fn usb_device_address(&self) -> u8 {
        self.dword_3.usb_device_address()
    }

    /// This field is updated by the xHC when a Device Slot transitions from one state to another.
    ///
    /// Note: This field is invalid before the slot has been assigned with an [`EnableSlot`] command
    ///
    /// [`EnableSlot`]: super::super::trb::command::CommandTrb::EnableSlot
    pub fn slot_state(&self) -> SlotState {
        self.dword_3.slot_state()
    }
}

impl Debug for SlotContext {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SlotContext")
            .field("route_string", &self.route_string())
            .field("multi_tt", &self.multi_tt())
            .field("is_hub", &self.is_hub())
            .field("context_entries", &self.context_entries())
            .field("max_exit_latency", &self.max_exit_latency())
            .field("root_hub_port_number", &self.root_hub_port_number())
            .field("num_ports", &self.num_ports())
            .field("parent_hub_slot_id", &self.parent_hub_slot_id())
            .field("parent_port_number", &self.parent_port_number())
            .field("tt_think_time", &self.tt_think_time())
            .field("interrupter_target", &self.interrupter_target())
            .field("usb_device_address", &self.usb_device_address())
            .field("slot_state", &self.slot_state())
            .finish()
    }
}
