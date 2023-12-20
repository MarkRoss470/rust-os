#![allow(missing_docs, clippy::missing_docs_in_private_items)] // TODO: Docs

use self::normal::NormalTrb;

pub mod normal;

/// A type of TRB.
/// 
/// Taken from [this table]
/// 
/// [this table]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A518%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C89%2C513%2C0%5D
#[derive(Debug)]
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
    const fn from_bits(bits: u32) -> Self {
        use TrbType::*;

        let bits = bits as u8;

        match bits {
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

#[repr(C, align(16))]
struct GenericTrbFields {
    data_1: u32,
    data_2: u32,
    data_3: u32,
    flags: GenericTrbFlags,
}

#[derive(Debug)]
pub enum Trb {
    NormalTrb(NormalTrb),
}

impl Trb {
    pub fn new(data: u64, config: u32, flags: u32) -> Self {
        let generic_flags = GenericTrbFlags::from(flags);
        match generic_flags.trb_type() {
            TrbType::Normal => Self::NormalTrb(NormalTrb::new(data, config, flags)),
            _ => todo!(),
        }
    }
}