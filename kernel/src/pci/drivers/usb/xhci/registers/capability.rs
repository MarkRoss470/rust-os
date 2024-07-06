//! Contains the [`CapabilityRegisters`] struct and the types it depends on

use core::{fmt::Debug, ptr::addr_of};

use alloc::string::String;
use x86_64::VirtAddr;

use super::super::{contexts::ContextSize, volatile_getter};

/// The `HCSPARAMS1` field of an [`CapabilityRegisters`] structure.
///
/// See the spec section [5.3.3] for more info
///
/// [5.3.3]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A389%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C560%2C0%5D
#[bitfield(u32, default = false)]
pub struct StructuralParameters1 {
    /// The maximum number of Device Context Structures and Doorbell Array entries this host controller can support.
    pub max_device_slots: u8,

    /// The number of Interrupters implemented on this host controller.
    #[bits(11)]
    pub max_interrupters: u16,

    #[bits(5)]
    __: (),

    /// The number of _Port Register_ entries in the [`OperationalRegisters`] table
    pub max_ports: u8,
}

/// The minimum length of time which is required to stay ahead of the controller when adding TRBs in order to
/// have the controller process them at the correct time.
#[derive(Debug)]
pub enum IsochronalSchedulingThreshold {
    /// Software can add a TRB no later than this number of Microframes before that TRB is scheduled to be executed.
    Frames(u8),
    /// software can add a TRB no later than this number of Frames before that TRB is scheduled to be executed.
    MicroFrames(u8),
}

impl IsochronalSchedulingThreshold {
    /// Constructs an [`IsochronalSchedulingThreshold`] from its bit representation
    const fn from_bits(bits: u32) -> Self {
        match bits & 0b1000 {
            0 => Self::MicroFrames((bits & 0b111) as u8),
            _ => Self::Frames((bits & 0b111) as u8),
        }
    }

    /// Converts an [`IsochronalSchedulingThreshold`] to its bit representation
    const fn into_bits(self) -> u32 {
        match self {
            Self::Frames(f) => (0b1000 & f) as u32,
            Self::MicroFrames(mf) => mf as u32,
        }
    }
}

/// The `HCSPARAMS2` field of an [`CapabilityRegisters`] structure.
///
/// See the spec section [5.3.4] for more info
///
/// [5.3.4]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A390%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C579%2C0%5D
#[bitfield(u32, debug = false, default = false)]
pub struct StructuralParameters2 {
    /// The minimum length of time which is required to stay ahead of the controller when adding TRBs in order to
    /// have the controller process them at the correct time.
    #[bits(4)]
    pub isochronal_scheduling_threshold: IsochronalSchedulingThreshold,

    /// The power of 2 of the maximum value supported by the _Event Ring Segment Table Base Size_ registers.
    /// For example, if this field has a value of 7 then the _Event Ring Segment Table(s)_ supports up to 128 entries.
    #[bits(4)]
    pub erst_max: u8,

    #[bits(13)]
    __: (),

    /// The low 5 bits of the number of scratchpad buffers
    #[bits(5)]
    max_scratchpad_buffers_low: u16,

    /// Whether the controller requires that scratchpad buffer space be maintained across power events
    pub scratchpad_restore: bool,

    /// The high 5 bits of the number of scratchpad buffers
    #[bits(5)]
    max_scratchpad_buffers_high: u16,
}

impl StructuralParameters2 {
    /// Gets the number of scratchpad buffers which the OS must provide for the controller.
    pub fn max_scratchpad_buffers(&self) -> u16 {
        self.max_scratchpad_buffers_high() << 5 & self.max_scratchpad_buffers_low()
    }
}

impl Debug for StructuralParameters2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StructuralParameters1")
            .field(
                "isochronal_scheduling_threshold",
                &self.isochronal_scheduling_threshold(),
            )
            .field("erst_max", &self.erst_max())
            .field("max_scratchpad_buffers", &self.max_scratchpad_buffers())
            .field("scratchpad_restore", &self.scratchpad_restore())
            .finish()
    }
}

/// The `HCSPARAMS3` field of an [`CapabilityRegisters`] structure.
///
/// See the spec section [5.3.5] for more info
///
/// [5.3.5]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A391%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C465%2C0%5D
#[bitfield(u32)]
pub struct StructuralParameters3 {
    /// The worst case latency in microseconds to transition a root hub _Port Link State_ (PLS) from U1 to U0.
    /// Applies to all root hub ports. Valid values are in the range 0x0000 to 0x000A.
    pub u1_device_exit_latency: u8,

    /// The worst case latency in microseconds to transition a root hub _Port Link State_ (PLS) from U2 to U0.
    /// Applies to all root hub ports. Valid values are in the range 0x0000 to 0x07FF.
    pub u2_device_exit_latency: u8,

    #[bits(16)]
    __: (),
}

impl ContextSize {
    /// Constructs a [`ContextSize`] from its bit representation in [`CapabilityParameters1`]
    const fn from_bits(bits: u32) -> Self {
        match bits {
            0 => Self::Small,
            1 => Self::Large,
            _ => unreachable!()
        }
    }

    /// Converts a [`ContextSize`] into its bit representation in [`CapabilityParameters1`]
    const fn into_bits(self) -> u32 {
        match self {
            Self::Small => 0,
            Self::Large => 1,
        }
    }
}

/// The `HCCPARAMS1` field of an [`CapabilityRegisters`] structure.
///
/// See the spec section [5.3.6] for more info
///
/// [5.3.6]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A392%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C506%2C0%5D
#[bitfield(u32, default = false)]
pub struct CapabilityParameters1 {
    /// Whether the controller supports 64 bit pointers.
    /// If this field is `false`, the controller can't be given 64 bit pointers and the top 32 bits of any pointers
    /// read from the controller must be ignored.
    pub is_64_bit: bool,

    /// Whether the controller supports bandwidth negotiation.
    ///
    /// See the spec section [4.16] for information about bandwidth negotiation.
    ///
    /// [4.16]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A290%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C555%2C0%5D
    pub supports_bandwidth_negotiation: bool,

    /// Whether the controller uses 32 or 64 byte context data structures
    #[bits(1)]
    pub context_size: ContextSize,

    /// Whether the controller supports port power control.
    ///
    /// See the spec section [5.4.8] for information about port power control.
    ///
    /// [5.4.8]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A412%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C386%2C0%5D
    pub supports_port_power_control: bool,

    /// Whether the root hub ports support port indicator control.
    ///
    /// See the spec section [5.4.8] for the definition of port indicator control.
    ///
    /// [5.4.8]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A412%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C386%2C0%5D
    pub supports_port_indicator_control: bool,

    /// Whether the controller supports a _Light Host Controller Reset_.
    /// This affects the functionality of the [USBCMD register].
    ///
    /// [USBCMD register]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A400%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C554%2C0%5D
    pub supports_lhcr: bool,

    /// Whether the controller supports _Latency Tolerance Messaging_ (LTM).
    ///
    /// See the spec section [4.13.1] for more information on LTM.
    ///
    /// [4.13.1]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A258%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C242%2C0%5D
    pub supports_ltm: bool,

    /// This bit is inverted - the [`supports_secondary_stream_ids`][Self::supports_secondary_stream_ids] method flips it so that it behaves like the rest of the bits in this struct.
    secondary_stream_ids_not_supported: bool,

    /// Whether the host controller implementation parses all Event Data TRBs while advancing to the next TD
    /// after a Short Packet, or it skips all but the first Event Data TRB.
    ///
    /// See the spec section [4.10.1.1] for more info.
    ///
    /// [4.10.1.1]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A198%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C501%2C0%5D
    pub parses_all_event_data_trbs: bool,

    /// Whether the controller can generate a _Stopped - Short Packet_ Completion Code.
    ///
    /// See the spec section [4.6.9] for more info.
    ///
    /// [4.6.9]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A140%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C694%2C0%5D
    pub can_produce_stop_short_packet: bool,

    /// Whether the controller's Stream Context supports a _Stopped EDTLA_ field.
    ///
    /// See the spec sections [4.6.9], [4.12], and [6.4.4.1] for more info.
    ///
    /// [4.6.9]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A140%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C694%2C0%5D
    /// [4.12]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A247%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C578%2C0%5D
    /// [6.4.4.1]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A510%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C198%2C0%5D
    pub supports_stopped_edtla: bool,

    /// Whether the controller is capable of matching the Frame ID of consecutive Isoch TDs.
    ///
    /// See the spec section [4.11.2.5] for more information.
    ///
    /// [4.11.2.5]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A226%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C464%2C0%5D
    pub can_match_consecutive_isoch_frame_id: bool,

    /// Identifies the maximum size _Primary Stream Array_ that the xHC supports.
    /// The _Primary Stream Array_ size = 2<sup>MaxPSASize+1</sup>.
    /// Valid values are 0 to 15, where ‘0’ indicates that Streams are not supported.
    #[bits(4)]
    max_primary_stream_array_size_exponent: u32,

    /// A pointer to the extended capabilities list, in 32 bit words, relative to the controller's MMIO region.
    pub extended_capabilities_pointer: u16,
}

impl CapabilityParameters1 {
    /// Whether the controller supports _Secondary Stream IDs_.
    ///
    /// See the spec sections [4.12.2] and [6.2.3] for more info.
    ///
    /// [4.12.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A253%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C694%2C0%5D
    /// [6.2.3]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A456%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C350%2C0%5D
    pub const fn supports_secondary_stream_ids(&self) -> bool {
        !self.secondary_stream_ids_not_supported()
    }

    /// Gets the maximum supported size of the _Primary Stream Array_, if supported.
    pub const fn max_primary_stream_array_size(&self) -> Option<u32> {
        let exponent = self.max_primary_stream_array_size_exponent();

        if exponent == 0 {
            None
        } else {
            Some(2u32.pow(exponent))
        }
    }

    /// Gets a space separated list of the capabilities, for terser printing
    fn capabilities_string(&self) -> String {
        let mut capabilities = String::new();

        if self.is_64_bit() {
            capabilities += "64_bit ";
        }
        if self.supports_bandwidth_negotiation() {
            capabilities += "bandwidth_negotiation ";
        }
        if self.context_size() == ContextSize::Large {
            capabilities += "64_byte_context ";
        }
        if self.supports_port_power_control() {
            capabilities += "port_power_control ";
        }
        if self.supports_port_indicator_control() {
            capabilities += "port_indicator_control ";
        }
        if self.supports_lhcr() {
            capabilities += "light_host_controller_reset ";
        }
        if self.supports_ltm() {
            capabilities += "latency_tolerance_messaging ";
        }
        if self.supports_secondary_stream_ids() {
            capabilities += "secondary_stream_ids ";
        }
        if self.parses_all_event_data_trbs() {
            capabilities += "parses_all_event_data_trbs ";
        }
        if self.can_produce_stop_short_packet() {
            capabilities += "stopped_short_packet ";
        }
        if self.supports_stopped_edtla() {
            capabilities += "stopped_edtla ";
        }
        if self.can_match_consecutive_isoch_frame_id() {
            capabilities += "match_consecutive_isoch_frame_ids ";
        }

        capabilities
    }
}

/// The `DBOFF` field of an [`CapabilityRegisters`] structure.
///
/// See the spec section [5.3.7] for more info
///
/// [5.3.7]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A394%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C218%2C0%5D
#[bitfield(u32)]
pub struct DoorbellOffsetRegister {
    #[bits(2)]
    __: (),

    /// The 32-byte offset into the controller's MMIO space of the Doorbell Array
    #[bits(30)]
    doorbell_array_offset: u32,
}

/// The `RTSOFF` field of an [`CapabilityRegisters`] structure.
///
/// See the spec section [5.3.8] for more info
///
/// [5.3.7]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A395%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C372%2C0%5D
#[bitfield(u32)]
pub struct RuntimeRegisterSpaceOffsetRegister {
    #[bits(5)]
    __: (),

    /// The 32-byte offset into the controller's MMIO space of the xHCI [`RuntimeRegisters`]
    #[bits(27)]
    pub runtime_register_space_offset: u64,
}

#[bitfield(u32)]
pub struct CapabilityParameters2 {
    /// Whether the root hub ports support the _Port Suspend Complete_ notification.
    ///
    /// See the spec section [4.15.1] for more info.
    ///
    /// [4.15.1]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A285%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C591%2C0%5D
    pub supports_u3_entry: bool,

    /// Whether a _Configure Endpoint Command_ is capable of generating a _Max Exit Latency Too Large Capability Error_.
    /// This capability is enabled by the CME flag in the USBCMD register.
    ///
    /// See the spec sections [4.23.5.2] and [5.4.1] for more info.
    ///
    /// [4.23.5.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A363%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C637%2C0%5D
    /// [5.4.1]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A400%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C554%2C0%5D
    pub can_generate_max_latency_too_large_error: bool,

    /// Whether the controller supports the _Force Save Context Capability_.
    ///
    /// See the spec sections [4.23.2] and [5.4.1] for more info.
    ///
    /// [4.23.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A348%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C219%2C0%5D
    /// [5.4.1]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A400%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C554%2C0%5D
    pub supports_force_save_context: bool,

    /// Whether the USB3 root hub supports the _Compliance Transition Enable_ (CTE) flag.
    ///
    /// See the spec section [4.19.1.2.4.1] for more info.
    ///
    /// [4.19.1.2.4.1]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A314%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C635%2C0%5D
    pub supports_compliance_transition: bool,

    /// Whether the controller supports ESIT payloads greater than 48KB.
    ///
    /// See the spec section [6.2.3.8] for more info.
    ///
    /// [6.2.3.8]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A464%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C601%2C0%5D
    pub supports_large_esit_payload: bool,

    /// Whether the controller supports extended Configuration Information.
    /// If `true`, the _Configuration Value_, _Interface Number_, and _Alternate Setting_ fields in the
    /// Input Control Context are supported.
    ///
    /// See the spec section [6.2.5.1] for more info.
    ///
    /// [6.2.5.1]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A468%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C694%2C0%5D
    pub supports_extended_configuration_information: bool,

    /// Whether the TBC field in an Isoch TRB supports the definition of _Burst Counts_ greater than 65535 bytes.
    ///
    /// See the spec section [4.11.2.3]
    ///
    /// [4.11.2.3]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A222%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C267%2C0%5D
    pub supports_extended_tbc: bool,

    /// Whether the  TBC/TRBSts field in an Isoch TRB indicates additional information regarding TRB in the TD.\
    /// If `true`, the Isoch TRB TD Size/TBC field presents TBC value and TBC/TRBSts field presents the TRBSts value.\
    /// If `false`,  the ETC/ETE values defines the TD Size/TBC field and TBC/RsvdZ field.
    ///
    /// See the spec section [4.11.2.3] for more info.
    ///
    /// [4.11.2.3]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A222%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C267%2C0%5D
    pub supports_extended_tbc_trb_status: bool,

    /// Whether the controller supports the _Get Extended Property_ and _Set Extended Property_ commands.
    ///
    /// See the spec sections [4.6.17] and [4.6.18] for the definitions of these commands.
    ///
    /// [4.6.17]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A162%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C666%2C0%5D
    /// [4.6.18]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A165%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C694%2C0%5D
    pub supports_extended_property_get_set: bool,

    /// Whether the controller supports the _Virtualisation based Trusted IO Capability_.
    /// This capability is enabled by the VTIOE flag in the USBCMD register.
    pub supports_virtualisation_based_trusted_io: bool,

    #[bits(22)]
    __: (),
}

impl CapabilityParameters2 {
    /// Gets a space separated list of the capabilities, for terser printing
    fn capabilities_string(&self) -> String {
        let mut capabilities = String::new();

        if self.supports_u3_entry() {
            capabilities += "u3_entry";
        }
        if self.can_generate_max_latency_too_large_error() {
            capabilities += "max_latency_too_large_error";
        }
        if self.supports_force_save_context() {
            capabilities += "force_save_context";
        }
        if self.supports_compliance_transition() {
            capabilities += "compliance_transition";
        }
        if self.supports_large_esit_payload() {
            capabilities += "large_esit_payload";
        }
        if self.supports_extended_configuration_information() {
            capabilities += "extended_configuration_information";
        }
        if self.supports_extended_tbc() {
            capabilities += "extended_tbc";
        }
        if self.supports_extended_tbc_trb_status() {
            capabilities += "extended_tbc_trb_status";
        }
        if self.supports_extended_property_get_set() {
            capabilities += "extended_property_get_set";
        }
        if self.supports_virtualisation_based_trusted_io() {
            capabilities += "virtualisation_based_trusted_io";
        }

        capabilities
    }
}

/// The capability registers of an XHCI controller.
///
/// See the spec section [5.3] for more info.
///
/// [5.3]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A387%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C176%2C0%5D
#[repr(C, align(4))]
#[derive(Clone, Copy)]
struct CapabilityRegistersFields {
    /// The length of the capability registers.
    /// The [`OperationalRegisters`] start this number of bytes after the capability registers.
    /// 
    /// [`OperationalRegisters`]: super::operational::OperationalRegisters
    capability_register_length: u8,

    #[doc(hidden)]
    _reserved0: u8,

    /// The interface version number. This is a binary coded decimal encoding of the three-part version number.
    /// For instance, a value of `0x0100` is `1.0.0`, `0x0110` is `1.1.0`, `0x0090` is `0.9.0`.
    version: u16,

    /// The first structural parameters register
    structural_parameters_1: StructuralParameters1,
    /// The second structural parameters register
    structural_parameters_2: StructuralParameters2,
    /// The third structural parameters register
    structural_parameters_3: StructuralParameters3,

    /// The first capability parameters register
    capability_parameters_1: CapabilityParameters1,
    /// The doorbell offset register
    doorbell_offset: DoorbellOffsetRegister,
    /// The runtime register space offset register
    runtime_register_space_offset: RuntimeRegisterSpaceOffsetRegister,
    /// The second capability parameters register
    capability_parameters_2: CapabilityParameters2,
}

#[rustfmt::skip]
impl Debug for CapabilityRegistersFields {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CapabilityRegisters")
            .field("capability_register_length", &self.capability_register_length)
            // format_args this one so that it always prints on one line even in pretty-print
            .field("version", &format_args!("{:?}", CapabilityRegisters::parse_version(self.version)))
            
            .field("max_device_slots", &self.structural_parameters_1.max_device_slots())
            .field("max_interrupters", &self.structural_parameters_1.max_interrupters())
            .field("max_ports", &self.structural_parameters_1.max_ports())

            // format_args this one so that it always prints on one line even in pretty-print
            .field("isochronal_scheduling_threshold", &format_args!("{:?}", self.structural_parameters_2.isochronal_scheduling_threshold()))
            .field("erst_max", &self.structural_parameters_2.erst_max())
            .field("max_scratchpad_buffers", &self.structural_parameters_2.max_scratchpad_buffers())
            .field("scratchpad_restore", &self.structural_parameters_2.scratchpad_restore())

            .field("u1_device_exit_latency", &self.structural_parameters_3.u1_device_exit_latency())
            .field("u2_device_exit_latency", &self.structural_parameters_3.u2_device_exit_latency())

            // format_args this one so that it always prints on one line even in pretty-print
            .field("max_primary_stream_array_size", &format_args!("{:?}", self.capability_parameters_1.max_primary_stream_array_size()))
            .field("capabilities", &(self.capability_parameters_1.capabilities_string() + &self.capability_parameters_2.capabilities_string()))

            .field("doorbell_array_offset", &self.doorbell_offset.doorbell_array_offset())
            
            .field("runtime_register_space_offset", &self.runtime_register_space_offset.runtime_register_space_offset())
            .finish()
    }
}

/// Wrapper struct around [`CapabilityRegistersFields`] to ensure accesses are volatile and read-only.
pub struct CapabilityRegisters {
    /// The pointer to where the capability registers struct is mapped in virtual memory.
    ptr: *const CapabilityRegistersFields,
}

impl CapabilityRegisters {
    /// Wraps the given pointer.
    ///
    /// # Safety
    /// The given pointer must point to the capability registers struct of an xHCI controller.
    /// This function may only be called once per controller.
    pub unsafe fn new(ptr: VirtAddr) -> Self {
        // SAFETY: `ptr` is valid
        let ptr = ptr.as_ptr();

        Self { ptr }
    }
}

#[rustfmt::skip]
impl CapabilityRegisters {
    volatile_getter!(
        CapabilityRegisters, CapabilityRegistersFields,
        capability_register_length, u8,
        (pub fn capability_register_length)
    );
    volatile_getter!(
        CapabilityRegisters, CapabilityRegistersFields,
        version, u16,
        (pub fn version)
    );
    volatile_getter!(
        CapabilityRegisters, CapabilityRegistersFields,
        structural_parameters_1, StructuralParameters1,
        (pub fn structural_parameters_1)
    );
    volatile_getter!(
        CapabilityRegisters, CapabilityRegistersFields,
        structural_parameters_2, StructuralParameters2,
        (pub fn structural_parameters_2)
    );
    volatile_getter!(
        CapabilityRegisters, CapabilityRegistersFields,
        structural_parameters_3, StructuralParameters3,
        (pub fn structural_parameters_3)
    );
    volatile_getter!(
        CapabilityRegisters, CapabilityRegistersFields,
        capability_parameters_1, CapabilityParameters1,
        (pub fn capability_parameters_1)
    );

    // Do this one separately so that the return type can be u64 rather than RuntimeRegisterSpaceOffsetRegister
    /// Performs a volatile read of the
    /// [`doorbell_offset`][CapabilityRegistersFields::doorbell_offset]
    /// field, returning the inner 
    /// [`runtime_register_space_offset`][RuntimeRegisterSpaceOffsetRegister::runtime_register_space_offset]
    /// field for convenience
    pub fn doorbell_offset(&self) -> u64 {
        // SAFETY: `self.ptr` is valid for reads as it points to the capabilities struct
        let r: DoorbellOffsetRegister = unsafe {
            addr_of!((*self.ptr).doorbell_offset).read_volatile()
        };

        (r.doorbell_array_offset() as u64) << 2
    }

    // Do this one separately so that the return type can be u64 rather than RuntimeRegisterSpaceOffsetRegister
    /// Performs a volatile read of the
    /// [`runtime_register_space_offset`][CapabilityRegistersFields::runtime_register_space_offset]
    /// field, returning the inner 
    /// [`runtime_register_space_offset`][RuntimeRegisterSpaceOffsetRegister::runtime_register_space_offset]
    /// field for convenience
    pub fn runtime_register_space_offset(&self) -> u64 {
        // SAFETY: `self.ptr` is valid for reads as it points to the capabilities struct
        let r: RuntimeRegisterSpaceOffsetRegister = unsafe {
            addr_of!((*self.ptr).runtime_register_space_offset).read_volatile()
        };

        r.runtime_register_space_offset() * 32
    }

    volatile_getter!(
        CapabilityRegisters, CapabilityRegistersFields,
        capability_parameters_2, CapabilityParameters2,
        (pub fn capability_parameters_2)
    );
}

impl CapabilityRegisters {
    /// Gets the version triple of the controller
    pub fn get_version(&self) -> (u8, u8, u8) {
        Self::parse_version(self.version())
    }

    /// Parses a version triple from a value stored in [`version`][CapabilityRegisters::version].
    /// To get the version of an instance, use [`get_version`][CapabilityRegisters::get_version].
    fn parse_version(version: u16) -> (u8, u8, u8) {
        (
            (version >> 8 & 0xf) as u8,
            (version >> 4 & 0xf) as u8,
            (version & 0xf) as u8,
        )
    }
}

impl Debug for CapabilityRegisters {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CapabilityRegisters")
            .field(
                "capability_register_length",
                &self.capability_register_length(),
            )
            .field("structural_parameters_1", &self.structural_parameters_1())
            .field("structural_parameters_2", &self.structural_parameters_2())
            .field("structural_parameters_3", &self.structural_parameters_3())
            .field("capability_parameters_1", &self.capability_parameters_1())
            .field(
                "doorbell_offset",
                &format_args!("{:#x}", self.doorbell_offset()),
            )
            .field(
                "runtime_register_space_offset",
                &format_args!("{:#x}", self.runtime_register_space_offset()),
            )
            .field("capability_parameters_2", &self.capability_parameters_2())
            .finish()
    }
}

/// Tests that the field offsets of [`CapabilityRegisters`] matches the xHCI spec,
/// so that values are read correctly.
#[rustfmt::skip]
#[test_case]
fn test_xhci_capability_field_offsets() {
    use core::mem::offset_of;

    assert_eq!(offset_of!(CapabilityRegistersFields, capability_register_length), 0x00);
    assert_eq!(offset_of!(CapabilityRegistersFields, version), 0x02);

    assert_eq!(offset_of!(CapabilityRegistersFields, structural_parameters_1), 0x04);
    assert_eq!(offset_of!(CapabilityRegistersFields, structural_parameters_2), 0x08);
    assert_eq!(offset_of!(CapabilityRegistersFields, structural_parameters_3), 0x0C);

    assert_eq!(offset_of!(CapabilityRegistersFields, capability_parameters_1), 0x10);
    assert_eq!(offset_of!(CapabilityRegistersFields, doorbell_offset), 0x14);
    assert_eq!(offset_of!(CapabilityRegistersFields, runtime_register_space_offset), 0x18);
    assert_eq!(offset_of!(CapabilityRegistersFields, capability_parameters_2), 0x1C);
}

#[test_case]
fn test_xhci_version_parsing() {
    assert_eq!(CapabilityRegisters::parse_version(0x0100), (1, 0, 0));
    assert_eq!(CapabilityRegisters::parse_version(0x0110), (1, 1, 0));
    assert_eq!(CapabilityRegisters::parse_version(0x0090), (0, 9, 0));
}
