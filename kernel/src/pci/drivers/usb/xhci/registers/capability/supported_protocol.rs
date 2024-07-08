//! The [`SupportedProtocolCapability`] and related types

use core::{fmt::Debug, str::Utf8Error};

use super::super::super::update_methods;
use crate::util::bitfield_enum::bitfield_enum;

/// A _Supported Protocol Capability_. This describes the USB protocol version supported by a range of ports,
/// and the speeds which they can operate at.
///
/// This data structure is defined in the spec section [7.2]
///
/// [7.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A528%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C306%2C0%5D
#[derive(Clone, Copy)]
#[allow(clippy::missing_docs_in_private_items)]
pub struct SupportedProtocolCapability<'a> {
    dword_0: Dword0,
    /// A 4-byte ASCII string identifying the protocol being implemented.
    /// The only valid value currently is 'USB '.
    name_string: [u8; 4],
    dword_2: Dword2,
    dword_3: Dword3,
    /// A list of mappings between the 4-bit [`speed_id_value`] and the port's speed.
    ///
    /// [`speed_id_value`]: ProtocolSpeedId::speed_id_value
    speed_ids: &'a [ProtocolSpeedId],
}

impl<'a> SupportedProtocolCapability<'a> {
    /// Reads a [`SupportedProtocolCapability`] from the given pointer
    ///
    /// # Safety
    /// * The passed `ptr` must point to a _Supported Protocol Capability_ in an XHCI controller's MMIO space.
    ///     Crucially, the [`protocol_speed_id_count`] field must be valid to ensure no out-of-bounds reads take place.
    /// * The `ptr` must be valid for reads for the lifetime `'a`.
    ///
    /// [`protocol_speed_id_count`]: Dword2::protocol_speed_id_count
    pub unsafe fn new(ptr: *const u32) -> Self {
        // SAFETY: `ptr` points to a Supported Protocol Capability so at least the first 4 dwords are valid to read
        let registers = unsafe { core::slice::from_raw_parts(ptr, 4) };

        let dword_0 = Dword0::from(registers[0]);
        let name_string = registers[1].to_le_bytes();
        let dword_2 = Dword2::from(registers[2]);
        let dword_3 = Dword3::from(registers[3]);

        // SAFETY: The `protocol_speed_id_count` field is valid so this slice is valid to construct.
        // `ptr` is valid for reads for the lifetime `'a`.
        let speed_ids = unsafe {
            core::slice::from_raw_parts(ptr.add(4).cast(), dword_2.protocol_speed_id_count())
        };

        Self {
            dword_0,
            name_string,
            dword_2,
            dword_3,

            speed_ids,
        }
    }
}

#[rustfmt::skip]
impl<'a> SupportedProtocolCapability<'a> {
    update_methods!(
        dword_0, Dword0,
        revision_major, u8,
        revision_major, _, _
    );
    update_methods!(
        dword_0, Dword0,
        revision_minor, u8,
        revision_minor, _, _
    );
    update_methods!(
        dword_2, Dword2,
        compatible_port_offset, u8,
        compatible_port_offset, _, _
    );
    update_methods!(
        dword_2, Dword2,
        compatible_port_count, u8,
        compatible_port_count, _, _
    );
    update_methods!(
        dword_2, Dword2 ,
        protocol_defined, u16,
        protocol_defined, _, _
    );
    update_methods!(
        dword_3, Dword3,
        protocol_slot_type, u8,
        protocol_slot_type, _, _
    );    
}

impl<'a> SupportedProtocolCapability<'a> {
    /// Gets the [`name_string`] field
    ///
    /// [`name_string`]: SupportedProtocolCapability::name_string
    pub fn name_string(&self) -> Result<&str, Utf8Error> {
        core::str::from_utf8(&self.name_string)
    }

    /// Gets the list of [`ProtocolSpeedId`]s associated with this capability
    pub fn speed_ids(&self) -> &[ProtocolSpeedId] {
        self.speed_ids
    }
}

#[rustfmt::skip]
impl<'a> Debug for SupportedProtocolCapability<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SupportedProtocolCapability")
            // The revision number is in binary coded decimal, which can be formatted by printing the digits as hex
            .field("revision", &format_args!(
                "{:x}.{}.{}", 
                self.revision_major(), 
                self.revision_minor() >> 4,
                self.revision_minor() & 0xF
            ))
            .field("name_string", &self.name_string().unwrap_or("Error: Invalid ASCII"))
            .field("compatible_port_offset", &self.compatible_port_offset())            
            .field("compatible_port_count", &self.compatible_port_count())
            .field("protocol_defined", &self.protocol_defined())
            .field("protocol_slot_type", &self.protocol_slot_type())
            .field("speed_ids", &self.speed_ids)
            .finish()
    }
}

#[bitfield(u32)]
struct Dword0 {
    /// Identifies the capability as a Supported Protocol Capability
    capability_id: u8,
    /// Offset to the next capability
    next_pointer: u8,
    /// The major revision of the USB spec implemented by the port range covered by this capability
    revision_minor: u8,
    /// The minor revision of the USB spec implemented by the port range covered by this capability
    revision_major: u8,
}

#[bitfield(u32)]
struct Dword2 {
    /// The first port covered by this capability
    compatible_port_offset: u8,
    /// The number of ports covered by this capability
    compatible_port_count: u8,
    /// Data specific to the revision.
    #[bits(12)]
    protocol_defined: u16,
    /// The number of [`ProtocolSpeedId`]s in the capability
    #[bits(4)]
    protocol_speed_id_count: usize,
}

#[bitfield(u32)]
struct Dword3 {
    /// The value to place in the [`slot_type`] field of an [`EnableSlotTrb`] using this protocol
    ///
    /// [`EnableSlotTrb`]: super::super::super::trb::command::slot::EnableSlotTrb
    /// [`slot_type`]: super::super::super::trb::command::slot::EnableSlotTrb::slot_type
    protocol_slot_type: u8,
    #[bits(24)]
    reserved0: u32,
}

#[bitfield(u32)]
pub struct ProtocolSpeedId {
    /// The value of [`port_speed`] which indicates that this speed ID is being used
    ///
    /// [`port_speed`]: super::super::super::registers::operational::port_registers::StatusAndControl::port_speed
    #[bits(4)]
    pub speed_id_value: u8,
    /// The unit of [`speed_id_mantissa`]
    ///
    /// [`speed_id_mantissa`]: ProtocolSpeedId::speed_id_mantissa
    #[bits(2)]
    pub speed_id_exponent: ProtocolSpeedIdExponent,
    /// Whether this speed is the receive speed, transmit speed, or both
    #[bits(2)]
    pub psi_type: ProtocolSpeedIdType,
    /// If this field is `true`, the link is full-duplex (dual-simplex),
    /// and if `false` the link is half-duplex (simplex).
    ///
    /// TODO: What does this mean? What does it affect?
    pub psi_full_duplex: bool,
    #[bits(5)]
    reserved0: u8,
    /// For USB3 ports ([`revision_major`] = 3), this field indicates the link-level protocol used by the port.
    /// For USB2 ports, this field is reserved and the link protocol is determined by the reported link speed.
    ///
    /// [`revision_major`]: Dword0::revision_major
    #[bits(2)]
    pub link_protocol: LinkProtocol,
    /// The maximum bit rate for this speed. The unit for this field is indicated by [`speed_id_exponent`]
    ///
    /// [`speed_id_exponent`]: ProtocolSpeedId::speed_id_exponent
    #[bits(16)]
    pub speed_id_mantissa: u16,
}

bitfield_enum!(
    #[bitfield_enum(u32)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ProtocolSpeedIdExponent {
        #[value(0)]
        Bits,
        #[value(1)]
        Kilobits,
        #[value(2)]
        Megabits,
        #[value(3)]
        Gigabits,
    }
);

bitfield_enum!(
    #[bitfield_enum(u32)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ProtocolSpeedIdType {
        #[value(0)]
        Symmetric,
        #[value(1)]
        Reserved,
        #[value(2)]
        AsymmetricReceive,
        #[value(3)]
        AsymmetricTransmit,
    }
);

bitfield_enum!(
    #[bitfield_enum(u32)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum LinkProtocol {
        #[value(0)]
        SuperSpeed,
        #[value(1)]
        SuperSpeedPlus,
        #[rest]
        Reserved(u8),
    }
);
