//! The [`EndpointContext`] type

use core::fmt::Debug;

use x86_64::PhysAddr;

use super::super::update_methods;

/// The current operational state of the endpoint
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointState {
    /// The endpoint is not operational
    Disabled,
    /// The endpoint is operational, either waiting for a doorbell ring or processing TDs.
    Running,
    /// The endpoint is halted due to a Halt condition detected on the USB. SW shall issue
    /// a [Reset Endpoint Command] to recover from the Halt condition and transition to the Stopped
    /// state. SW may manipulate the Transfer Ring while in this state.
    ///
    /// [Reset Endpoint Command]: super::super::trb::command::CommandTrb::ResetEndpoint
    Halted,
    /// The endpoint is not running due to a Stop Endpoint Command or recovering
    /// from a Halt condition. SW may manipulate the Transfer Ring while in this state.
    Stopped,
    /// The endpoint is not running due to a TRB Error. SW may manipulate the Transfer Ring while in this state.
    Error,
    /// Reserved
    Reserved(u8),
}

impl EndpointState {
    /// Constructs an [`EndpointState`] from its bit representation
    const fn from_bits(bits: u32) -> Self {
        #[allow(clippy::cast_possible_truncation)]
        match bits {
            0 => Self::Disabled,
            1 => Self::Running,
            2 => Self::Halted,
            3 => Self::Stopped,
            4 => Self::Error,
            5..=7 => Self::Reserved(bits as _),
            _ => unreachable!(),
        }
    }

    /// Converts an [`EndpointState`] into its bit representation
    const fn into_bits(self) -> u32 {
        match self {
            EndpointState::Disabled => 0,
            EndpointState::Running => 1,
            EndpointState::Halted => 2,
            EndpointState::Stopped => 3,
            EndpointState::Error => 4,
            EndpointState::Reserved(bits) => bits as _,
        }
    }
}

#[bitfield(u32)]
struct EndpointContextDword0 {
    /// The current operational state of the endpoint
    #[bits(3)]
    endpoint_state: EndpointState,

    #[bits(5)]
    _reserved: (),

    /// If [`supports_large_esit_payload`] is `true`, this field is reserved.
    ///
    /// Otherwise, this field is the maximum number of bursts within an Interval that this endpoint supports.
    /// Mult is a “zero-based” value, where 0 to 3 represents 1 to 4 bursts, respectively.
    /// The valid range of values is ‘0’ to ‘2’. This field shall be ‘0’ for all endpoint types except for SS Isochronous.
    ///
    /// [`supports_large_esit_payload`]: super::super::capability_registers::CapabilityParameters2::supports_large_esit_payload
    #[bits(2)]
    mult: u8,

    /// the maximum number of _Primary Stream IDs_ this endpoint supports.
    ///
    /// If the value of this field is `None`, then the [`tr_dequeue_pointer`] field shall point to a Transfer Ring.
    /// If this field is > 0 then the TR Dequeue Pointer field shall point to a Primary Stream Context Array.
    ///
    /// See the spec section [4.12] for more information.
    ///
    /// A value of ‘1’ to ‘15’ indicates that the Primary Stream ID Width is `max_primary_streams + 1` and the
    /// Primary Stream Array contains `2 ^ (MaxPStreams + 1)` entries
    ///
    /// [4.12]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A247%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C578%2C0%5D
    /// [`tr_dequeue_pointer`]: EndpointContext::tr_dequeue_pointer
    #[bits(5)]
    max_primary_streams: u8,

    /// how a Stream ID shall be interpreted.
    ///
    /// Setting this bit to  `true` shall disable _Secondary Stream Arrays_ and a _Stream ID_ shall be
    /// interpreted as a linear index into the _Primary Stream Array_, where valid values for [`max_primary_streams`]
    /// are ‘1’ to ‘15’.
    ///
    /// A value of `false` shall enable _Secondary Stream Arrays_, where the low order ([`max_primary_streams`] + 1) bits
    /// of a Stream ID shall be interpreted as a linear index into the Primary Stream Array, where valid
    /// values for [`max_primary_streams`] are ‘1’ to ‘7’. And the high order bits of a Stream ID shall be interpreted
    /// as a linear index into the Secondary Stream Array
    ///
    /// [`max_primary_streams`]: EndpointContext::max_primary_streams
    linear_stream_array: bool,

    /// The period between consecutive requests to a USB endpoint to send or receive data.
    /// Expressed in 125 μs. increments. The period is calculated as `125 μs. * (2 ^ interval)`, e.g., an `interval`
    /// value of 0 means a period of 125 μs. (2 ^ 0 = 1 * 125 μs.), a value of 1 means a period of 250 μs. (2 ^ 1
    /// = 2 * 125 μs.), a value of 4 means a period of 2 ms. (2 ^ 4 = 16 * 125 μs.).
    ///
    /// See the spec section [6.2.3.6] for more info.
    ///
    /// [6.2.3.6]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A463%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C658%2C0%5D
    interval: u8,

    /// See [`EndpointContext::max_esit_payload`]
    max_endpoint_service_time_interval_payload_high: u8,
}

#[bitfield(u32)]
struct EndpointContextDword1 {
    #[bits(1)]
    _reserved: (),

    /// The number of errors while executing a TD before the controller gives up and issues a [USB Transaction Error Event].
    /// E.g. a value of 1 means that all errors cause a [USB Transaction Error Event].
    ///
    /// A value of 0 means that the controller will not track error numbers and there is no limit on the number of retries.
    ///
    /// [USB Transaction Error Event]: super::super::trb::event::command_completion::CompletionError::UsbTransaction
    #[bits(2)]
    error_count: u8,

    /// Whether the context is valid, and if so, what kind of endpoint it defines.
    #[bits(3)]
    endpoint_type: EndpointType,

    #[bits(1)]
    _reserved: (),

    /// This field affects Stream enabled endpoints, allowing the _Host Initiated Stream_ selection feature to be disabled for the endpoint.
    /// Setting this bit to `true` shall disable the _Host Initiated Stream_ selection feature. A value of `false` will enable normal
    /// Stream operation.
    ///
    /// See the spec section [4.12.1.1] for more info.
    ///
    /// [4.12.1.1]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A252%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C394%2C0%5D
    host_initiate_disable: bool,

    /// The maximum number of consecutive USB transactions that should be executed per scheduling opportunity.
    ///
    /// See the spec section [6.2.3.4] for more info.
    ///
    /// [6.2.3.4]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A462%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C465%2C0%5D
    max_burst_size: u8,

    /// the maximum packet size in bytes that this endpoint is capable of sending or receiving when configured.
    ///
    /// See the spec section [6.2.3.5] for more info.
    ///
    /// [6.2.3.5]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A462%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C214%2C0%5D
    max_packet_size: u16,
}

#[bitfield(u32)]
struct EndpointContextDword2 {
    /// The value of the xHC Consumer Cycle State (CCS) flag for the TRB referenced by the TR Dequeue Pointer.
    /// This field shall be `false` if [`max_primary_streams`] > 0.
    ///
    /// See the spec section [4.9.2] for more info.
    ///
    /// [`max_primary_streams`]: EndpointContext::max_primary_streams
    /// [4.9.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A176%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C226%2C0%5D
    dequeue_cycle_state: bool,

    #[bits(3)]
    _reserved: (),

    /// See [`EndpointContext::tr_dequeue_pointer`]
    #[bits(28)]
    tr_dequeue_pointer_low: u32,
}

#[bitfield(u32)]
struct EndpointContextDword4 {
    /// The average Length of the TRBs executed by this endpoint. The value of this field shall be greater than 0.
    ///
    /// See the spec section [4.14.1.1] for more info.
    ///
    /// [4.14.1.1]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A264%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C527%2C0%5D
    average_trb_length: u16,

    /// See [`EndpointContext::max_esit_payload`]
    max_endpoint_service_time_interval_payload_low: u16,
}

/// A type of USB endpoint
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::missing_docs_in_private_items)]
pub enum EndpointType {
    NotValid,
    IsochOut,
    BulkOut,
    InterruptOut,
    Control,
    IsochIn,
    BulkIn,
    InterruptIn,
}

impl EndpointType {
    /// Constructs the [`EndpointType`] from its bit representation
    const fn from_bits(bits: u32) -> Self {
        match bits {
            0 => Self::NotValid,
            1 => Self::IsochOut,
            2 => Self::BulkOut,
            3 => Self::InterruptOut,
            4 => Self::Control,
            5 => Self::IsochIn,
            6 => Self::BulkIn,
            7 => Self::InterruptIn,

            _ => unreachable!(),
        }
    }

    /// Converts the [`EndpointType`] into its bit representation
    const fn into_bits(self) -> u32 {
        match self {
            Self::NotValid => 0,
            Self::IsochOut => 1,
            Self::BulkOut => 2,
            Self::InterruptOut => 3,
            Self::Control => 4,
            Self::IsochIn => 5,
            Self::BulkIn => 6,
            Self::InterruptIn => 7,
        }
    }
}

/// The _Endpoint Context_ data structure, which defines information that applies to a specific endpoint.
///
/// See the spec section [6.2.3] for more information.
///
/// [6.2.3]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A456%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C350%2C0%5D
#[repr(C)]
#[derive(Clone, Copy)]
pub struct EndpointContext {
    /// The first DWORD
    dword_0: EndpointContextDword0,
    /// The second DWORD
    dword_1: EndpointContextDword1,
    /// The third DWORD
    dword_2: EndpointContextDword2,
    /// The top 32 bits of the TR dequeue pointer
    tr_dequeue_pointer_high: u32,
    /// The fifth DWORD
    dword_4: EndpointContextDword4,
}

#[rustfmt::skip]
impl EndpointContext {
    update_methods!(
        dword_0, EndpointContextDword0,
        endpoint_state, EndpointState,
        endpoint_state, set_endpoint_state, with_endpoint_state
    );
    update_methods!(
        dword_0, EndpointContextDword0,
        linear_stream_array, bool,
        linear_stream_array, set_linear_stream_array, with_linear_stream_array
    );
    update_methods!(
        dword_0, EndpointContextDword0,
        interval, u8,
        interval, set_interval, with_interval
    );
    update_methods!(
        dword_1, EndpointContextDword1,
        error_count, u8,
        error_count, _, _
    );
    update_methods!(
        dword_1, EndpointContextDword1,
        endpoint_type, EndpointType,
        endpoint_type, set_endpoint_type, with_endpoint_type
    );
    update_methods!(
        dword_1, EndpointContextDword1,
        host_initiate_disable, bool,
        host_initiate_disable, set_host_initiate_disable, with_host_initiate_disable
    );
    update_methods!(
        dword_1, EndpointContextDword1,
        max_burst_size, u8,
        max_burst_size, set_max_burst_size, with_max_burst_size
    );
    update_methods!(
        dword_1, EndpointContextDword1,
        max_packet_size, u16,
        max_packet_size, set_max_packet_size, with_max_packet_size
    );
    update_methods!(
        dword_2, EndpointContextDword2,
        dequeue_cycle_state, bool,
        dequeue_cycle_state, set_dequeue_cycle_state, with_dequeue_cycle_state
    );
    update_methods!(
        dword_4, EndpointContextDword4,
        average_trb_length, u16,
        average_trb_length, set_average_trb_length, with_average_trb_length
    );
}

impl EndpointContext {
    /// Constructs a new [`EndpointContext`] with all fields set to 0
    pub fn new() -> Self {
        Self {
            dword_0: EndpointContextDword0::new(),
            dword_1: EndpointContextDword1::new(),
            dword_2: EndpointContextDword2::new(),
            tr_dequeue_pointer_high: 0,
            dword_4: EndpointContextDword4::new(),
        }
    }

    /// Gets the [`mult`] field. `large_esit_payload` is the value of the controller's [`supports_large_esit_payload`] field.
    ///
    /// [`mult`]: EndpointContextDword0::mult
    /// [`supports_large_esit_payload`]: super::super::capability_registers::CapabilityParameters2::supports_large_esit_payload
    pub fn mult(&self, large_esit_payload: bool) -> u32 {
        if large_esit_payload {
            self.max_esit_payload()
                .div_ceil(self.max_packet_size().into())
                .div_ceil(self.max_burst_size() as u32 + 1)
                - 1
        } else {
            self.dword_0.mult() as u32 + 1
        }
    }

    /// Gets the [`max_primary_streams`] field
    ///
    /// [`max_primary_streams`]: EndpointContextDword0::max_primary_streams
    pub fn max_primary_streams(&self) -> Option<u8> {
        let v = self.dword_0.max_primary_streams();

        match v {
            0 => None,
            _ => Some(v),
        }
    }

    /// If [`max_primary_streams`] == `0`, this field shall be used by the xHC to store the value of the
    /// _Dequeue Pointer_ when the endpoint enters the [`Halted`] or [`Stopped`] states, and the value of the
    /// this field shall be undefined when the endpoint is not in the [`Halted`] or [`Stopped`] states. if
    /// [`max_primary_streams`] > ‘0’ then this field shall point to a _Stream Context Array_.
    ///
    /// [`max_primary_streams`]: EndpointContext::max_primary_streams
    /// [`Halted`]: EndpointState::Halted
    /// [`Stopped`]: EndpointState::Stopped
    pub fn tr_dequeue_pointer(&self) -> PhysAddr {
        let phys_addr = (self.tr_dequeue_pointer_high as u64) << 32
            | (self.dword_2.tr_dequeue_pointer_low() as u64) << 4;
        PhysAddr::new(phys_addr)
    }

    /// The total number of bytes this endpoint will transfer during an ESIT.
    ///
    /// With the introduction of USB Gen 2 speed data rates (SSP), the Max ESIT Payload values exceeded 64K.
    /// The [`supports_large_esit_payload`] flag in the controller's capability registers indicates if the controller
    /// is capable of supporting Max ESIT Payload values greater than 48K bytes.
    ///
    /// If [`supports_large_esit_payload`] is `false`, then the largest value the xHC supports for the Max ESIT Payload is
    /// 48K bytes. Note that only devices attached to SSP or faster USB3 Root Hub ports may support Max ESIT Payload values greater than 48KB.
    /// If [`supports_large_esit_payload`] is `true`, then the largest value the xHC supports for the Max ESIT Payload is `16MB - 1` bytes.
    ///
    /// Refer to the spec section [4.14.2] for the definition of an “ESIT” and more information related to setting the value of Max ESIT Payload.
    /// For periodic endpoints, the Max ESIT Payload is used by the xHC to reserve the bus transfer time for the endpoint in its Pipe Schedule.
    ///
    /// [`supports_large_esit_payload`]: super::super::capability_registers::CapabilityParameters2::supports_large_esit_payload
    /// [4.14.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A265%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C354%2C0%5D
    pub fn max_esit_payload(&self) -> u32 {
        (self
            .dword_0
            .max_endpoint_service_time_interval_payload_high() as u32)
            << 16
            | self
                .dword_4
                .max_endpoint_service_time_interval_payload_low() as u32
    }

    /// Sets the [`mult`] field. `large_esit_payload` is the value of the controller's [`supports_large_esit_payload`] field.
    ///
    /// [`mult`]: EndpointContextDword0::mult
    /// [`supports_large_esit_payload`]: super::super::capability_registers::CapabilityParameters2::supports_large_esit_payload
    pub fn set_mult(&mut self, large_esit_payload: bool, mult: u32) {
        if large_esit_payload {
            todo!("Setting mult with large ESIT payload");
        }
        
        let mult: u8 = mult.try_into().unwrap();
        assert!(mult <= 2);

        self.dword_0.set_mult(mult);
    }

    /// Updates the [`mult`] field. `large_esit_payload` is the value of the controller's [`supports_large_esit_payload`] field.
    ///
    /// [`mult`]: EndpointContextDword0::mult
    /// [`supports_large_esit_payload`]: super::super::capability_registers::CapabilityParameters2::supports_large_esit_payload
    pub fn with_mult(mut self, large_esit_payload: bool, mult: u32) -> Self {
        self.set_mult(large_esit_payload, mult);
        self
    }

    /// Sets the [`max_primary_streams`] field
    ///
    /// [`max_primary_streams`]: EndpointContextDword0::max_primary_streams
    pub fn set_max_primary_streams(&mut self, max_primary_streams: Option<u8>) {
        if let Some(v) = max_primary_streams {
            assert_ne!(v, 0);
            assert!(v < 16);
        }

        self.dword_0
            .set_max_primary_streams(max_primary_streams.unwrap_or(0));
    }

    /// Updates the [`max_primary_streams`] field
    ///
    /// [`max_primary_streams`]: EndpointContextDword0::max_primary_streams
    pub fn with_max_primary_streams(mut self, max_primary_streams: Option<u8>) -> Self {
        self.set_max_primary_streams(max_primary_streams);
        self
    }

    /// Sets the [`error_count`] field
    ///
    /// [`error_count`]: EndpointContextDword1::error_count
    pub fn set_error_count(&mut self, error_count: u8) {
        assert!(error_count < 4);

        self.dword_1.set_error_count(error_count);
    }

    /// Updates the [`error_count`] field
    ///
    /// [`error_count`]: EndpointContextDword1::error_count
    pub fn with_error_count(mut self, error_count: u8) -> Self {
        self.set_error_count(error_count);
        self
    }

    /// Sets the [`tr_dequeue_pointer`] field
    ///
    /// [`tr_dequeue_pointer`]: EndpointContext::tr_dequeue_pointer
    pub fn set_tr_dequeue_pointer(&mut self, tr_dequeue_pointer: PhysAddr) {
        let tr_dequeue_pointer = tr_dequeue_pointer.as_u64();
        let high = (tr_dequeue_pointer >> 32) as u32;
        #[allow(clippy::cast_possible_truncation)]
        let low = tr_dequeue_pointer as u32;

        self.tr_dequeue_pointer_high = high;
        self.dword_2.set_tr_dequeue_pointer_low(low >> 4);
    }

    /// Updates the [`tr_dequeue_pointer`] field
    ///
    /// [`tr_dequeue_pointer`]: EndpointContext::tr_dequeue_pointer
    pub fn with_tr_dequeue_pointer(mut self, tr_dequeue_pointer: PhysAddr) -> Self {
        self.set_tr_dequeue_pointer(tr_dequeue_pointer);
        self
    }

    /// Sets the [`max_esit_payload`] field
    ///
    /// [`max_esit_payload`]: EndpointContext::max_esit_payload
    pub fn set_max_esit_payload(&mut self, max_esit_payload: u32) {
        assert_eq!(max_esit_payload >> 24, 0);

        #[allow(clippy::cast_possible_truncation)]
        let high = (max_esit_payload >> 16) as u8;
        #[allow(clippy::cast_possible_truncation)]
        let low = max_esit_payload as u16;

        self.dword_0
            .set_max_endpoint_service_time_interval_payload_high(high);
        self.dword_4
            .set_max_endpoint_service_time_interval_payload_low(low);
    }

    /// Updates the [`max_esit_payload`] field
    ///
    /// [`max_esit_payload`]: EndpointContext::max_esit_payload
    pub fn with_max_esit_payload(mut self, max_esit_payload: u32) -> Self {
        self.set_max_esit_payload(max_esit_payload);
        self
    }
}

impl Debug for EndpointContext {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("EndpointContext")
            .field("endpoint_state", &self.endpoint_state())
            .field("max_primary_streams", &self.max_primary_streams())
            .field("linear_stream_array", &self.linear_stream_array())
            .field("interval", &self.interval())
            .field("error_count", &self.error_count())
            .field("endpoint_type", &self.endpoint_type())
            .field("host_initiate_disable", &self.host_initiate_disable())
            .field("max_burst_size", &self.max_burst_size())
            .field("max_packet_size", &self.max_packet_size())
            .field("dequeue_cycle_state", &self.dequeue_cycle_state())
            .field("tr_dequeue_pointer", &self.tr_dequeue_pointer())
            .field("average_trb_length", &self.average_trb_length())
            .field(
                "max_endpoint_service_time_interval_payload",
                &self.max_esit_payload(),
            )
            .finish()
    }
}
