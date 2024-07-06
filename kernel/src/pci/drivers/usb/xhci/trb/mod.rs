//! Types related to TRBs and TRB rings.
//!
//! There are three types of TRB ring:
//! * [`CommandTrbRing`]s, which contain [`CommandTrb`]s
//! * Transfer TRB rings (TODO: link), which contain [`TransferTrb`]s
//! * [`EventTrbRing`]s, which contain [`EventTrb`]s

use self::{link::LinkTrb, normal::NormalTrb};

pub mod command;
pub mod event;
mod event_ring;
mod link;
pub mod normal;
mod software_driven_rings;

pub use command::CommandTrb;
pub use event::EventTrb;
pub use event_ring::EventTrbRing;
pub use software_driven_rings::{CommandTrbRing, TransferTrbRing};

/// A type of TRB. Taken from [this table].
///
/// This enum only holds the type of TRB, not any data that they hold - see [`CommandTrb`], [`TransferTrb`], and [`EventTrb`].
///
/// [this table]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A518%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C89%2C513%2C0%5D
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::missing_docs_in_private_items)]
enum TrbType {
    Normal,
    SetupStage,
    DataStage,
    StatusStage,
    Isoch,
    Link,
    EventData,
    NoOp,

    EnableSlotCommand,
    DisableSlotCommand,
    AddressDeviceCommand,
    ConfigureEndpointCommand,
    EvaluateContextCommand,
    ResetEndpointCommand,
    StopEndpointCommand,
    SetTRDequeuePointerCommand,
    ResetDeviceCommand,
    ForceEventCommand,
    NegotiateBandwidthCommand,
    SetLatencyToleranceValueCommand,
    GetPortBandwidthCommand,
    ForceHeaderCommand,
    NoOpCommand,
    GetExtendedPropertyCommand,
    SetExtendedPropertyCommand,

    TransferEvent,
    CommandCompletionEvent,
    PortStatusChangeEvent,
    BandwidthRequestEvent,
    DoorbellEvent,
    HostControllerEvent,
    DeviceNotificationEvent,
    MFINDEXWrapEvent,

    Reserved(u8),
    VendorDefined(u8),
}

impl TrbType {
    /// Constructs a [`TrbType`] from its bit representation
    const fn from_bits(bits: u32) -> Self {
        use TrbType::*;

        #[allow(clippy::cast_possible_truncation)]
        let bits = bits as u8;

        match bits {
            0 => Reserved(0),

            1 => Normal,
            2 => SetupStage,
            3 => DataStage,
            4 => StatusStage,
            5 => Isoch,
            6 => Link,
            7 => EventData,
            8 => NoOp,

            9 => EnableSlotCommand,
            10 => DisableSlotCommand,
            11 => AddressDeviceCommand,
            12 => ConfigureEndpointCommand,
            13 => EvaluateContextCommand,
            14 => ResetEndpointCommand,
            15 => StopEndpointCommand,
            16 => SetTRDequeuePointerCommand,
            17 => ResetDeviceCommand,
            18 => ForceEventCommand,
            19 => NegotiateBandwidthCommand,
            20 => SetLatencyToleranceValueCommand,
            21 => GetPortBandwidthCommand,
            22 => ForceHeaderCommand,
            23 => NoOpCommand,
            24 => GetExtendedPropertyCommand,
            25 => SetExtendedPropertyCommand,

            26..=31 => Reserved(bits),

            32 => TransferEvent,
            33 => CommandCompletionEvent,
            34 => PortStatusChangeEvent,
            35 => BandwidthRequestEvent,
            36 => DoorbellEvent,
            37 => HostControllerEvent,
            38 => DeviceNotificationEvent,
            39 => MFINDEXWrapEvent,

            40..=47 => Reserved(bits),
            48..=63 => VendorDefined(bits),

            _ => unreachable!(),
        }
    }

    /// Converts a [`TrbType`] into its bit representation
    const fn into_bits(self) -> u32 {
        use TrbType::*;

        match self {
            Normal => 1,
            SetupStage => 2,
            DataStage => 3,
            StatusStage => 4,
            Isoch => 5,
            Link => 6,
            EventData => 7,
            NoOp => 8,

            EnableSlotCommand => 9,
            DisableSlotCommand => 10,
            AddressDeviceCommand => 11,
            ConfigureEndpointCommand => 12,
            EvaluateContextCommand => 13,
            ResetEndpointCommand => 14,
            StopEndpointCommand => 15,
            SetTRDequeuePointerCommand => 16,
            ResetDeviceCommand => 17,
            ForceEventCommand => 18,
            NegotiateBandwidthCommand => 19,
            SetLatencyToleranceValueCommand => 20,
            GetPortBandwidthCommand => 21,
            ForceHeaderCommand => 22,
            NoOpCommand => 23,
            GetExtendedPropertyCommand => 24,
            SetExtendedPropertyCommand => 25,

            TransferEvent => 32,
            CommandCompletionEvent => 33,
            PortStatusChangeEvent => 34,
            BandwidthRequestEvent => 35,
            DoorbellEvent => 36,
            HostControllerEvent => 37,
            DeviceNotificationEvent => 38,
            MFINDEXWrapEvent => 39,

            Reserved(t) => t as u32,
            VendorDefined(t) => t as u32,
        }
    }
}

#[bitfield(u32)]
struct GenericTrbFlags {
    cycle: bool,

    #[bits(9)]
    __: (),

    #[bits(6)]
    trb_type: TrbType,

    #[bits(16)]
    __: (),
}

/// An error indicating that a TRB ring is full
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RingFullError;

/// A TRB on a transfer TRB ring (TODO: link).
///
/// This tells the controller how to send or receive data.
///
/// See the spec section [6.4.1] for more info.
///
/// [6.4.1]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A472%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C548%2C0%5D
#[derive(Debug)]
#[allow(clippy::missing_docs_in_private_items)] // TODO: add docs with the corresponding structs
pub enum TransferTrb {
    /// A [`NormalTrb`]
    Normal(NormalTrb),
    SetupStage,
    DataStage,
    StatusStage,
    Isoch,
    /// A [`LinkTrb`]
    Link(LinkTrb),
    EventData,
    NoOp,
}

impl TransferTrb {
    /// Converts the TRB to the data written to a TRB ring
    pub fn to_parts(&self, cycle: bool) -> [u32; 4] {
        match self {
            TransferTrb::Normal(normal) => normal.to_parts(cycle),
            TransferTrb::SetupStage => todo!(),
            TransferTrb::DataStage => todo!(),
            TransferTrb::StatusStage => todo!(),
            TransferTrb::Isoch => todo!(),
            TransferTrb::Link(link) => link.to_parts(cycle),
            TransferTrb::EventData => todo!(),
            TransferTrb::NoOp => todo!(),
        }
    }

    /// Gets the chain bit for this TRB.
    /// This is used to set the chain bit of [`LinkTrb`]s correctly, as this needs to match the TRB before it.
    pub fn chain(&self) -> bool {
        match self {
            TransferTrb::Normal(normal) => normal.chain(),
            TransferTrb::SetupStage => todo!(),
            TransferTrb::DataStage => todo!(),
            TransferTrb::StatusStage => todo!(),
            TransferTrb::Isoch => todo!(),
            TransferTrb::Link(link) => link.chain(),
            TransferTrb::EventData => todo!(),
            TransferTrb::NoOp => todo!(),
        }
    }
}
