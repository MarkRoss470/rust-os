use core::fmt::Debug;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressSpace {
    SystemMemory,
    SystemIO,
    PciConfigurationSpace,
    EmbeddedController,
    Smb,
    SystemCmos,
    PciBar,
    Ipmi,
    Gpio,
    GenericSerialBus,
    PlatformCommunicationChannel,

    Reserved,
    OemDefined,
}

impl AddressSpace {
    fn from_u8(value: u8) -> Self {
        match value {
            0x00 => Self::SystemMemory,
            0x01 => Self::SystemIO,
            0x02 => Self::PciConfigurationSpace,
            0x03 => Self::EmbeddedController,
            0x04 => Self::Smb,
            0x05 => Self::SystemCmos,
            0x06 => Self::PciBar,
            0x07 => Self::Ipmi,
            0x08 => Self::Gpio,
            0x09 => Self::GenericSerialBus,
            0x0A => Self::PlatformCommunicationChannel,

            0x0B..=0x7F => Self::Reserved,
            0x80..=0xFF => Self::OemDefined,
        }
    }
}

#[derive(Debug)]
pub enum AccessSize {
    Undefined,
    B8,
    B16,
    B32,
    B64,
}

impl AccessSize {
    fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Undefined,
            1 => Self::B8,
            2 => Self::B16,
            3 => Self::B32,
            4 => Self::B64,

            _ => Self::Undefined,
        }
    }
}

#[repr(C)]
pub struct GenericAddressStructure {
    /// What address space to access. Converted to [`AddressSpace`] enum in getter.
    address_space: u8,
    /// Only used if the address space is a bitfield - defines the number of bits to read
    bit_width: u8,
    /// Only used if the address space is a bitfield - defines the offset of the first bit
    bit_offset: u8,
    /// The number of bytes which can be accessed at once. Converted to [`AccessSize`] enum in getter
    access_size: u8,
    /// The address
    address: u64,
}

impl Debug for GenericAddressStructure {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GenericAddressStructure")
            .field("address_space", &self.address_space())
            .field("bit_width", &self.bit_width)
            .field("bit_offset", &self.bit_offset)
            .field("access_size", &self.access_size())
            .field("address", &format_args!("{:#x}", self.address))
            .finish()
    }
}

/// Getters for struct fields
#[rustfmt::skip]
impl GenericAddressStructure {
    /// Gets the [`address_space`][Self::address_space] field
    pub fn address_space(&self) -> AddressSpace {
        AddressSpace::from_u8(self.address_space)
    }

    /// Gets the [`bit_width`][Self::bit_width] field
    pub fn bit_width(&self) -> u8 { self.bit_width }
    /// Gets the [`bit_offset`][Self::bit_offset] field
    pub fn bit_offset(&self) -> u8 { self.bit_offset }

    /// Gets the [`access_size`][Self::access_size] field
    pub fn access_size(&self) -> AccessSize {
        AccessSize::from_u8(self.access_size)
    }

    /// Gets the [`address`][Self::address] field
    pub fn address(&self) -> u64 { self.address }
}
