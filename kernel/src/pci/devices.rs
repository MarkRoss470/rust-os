//! Structs representing different specificities of PCI hardware -
//! [device][PciDevice], [function][PciFunction], and [register][PciRegister]

use core::fmt::Display;
use x86_64::instructions::port::Port;

use super::{registers::PciHeader, classcodes::InvalidValueError};

/// The port number to write the address of a [`PciRegister`] to read or write its data
const CONFIG_ADDRESS: u16 = 0xCF8;
/// The port number to read or write data to get/set a [`PciRegister`]
const CONFIG_DATA: u16 = 0xCFC;

/// An error which can occur when constructing a PCI address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PciInvalidAddressError {
    /// The device number was too high
    Device(u8),
    /// The function number was too high
    Function(u8),
    /// The offset number was not 4 byte aligned
    Offset(u8),
}

/// Checks that the given device id is valid
fn check_device_id(device: u8) -> Result<(), PciInvalidAddressError> {
    if device & 0b11100000 != 0 {
        Err(PciInvalidAddressError::Device(device))
    } else {
        Ok(())
    }
}

/// Checks that the given function if is valid
fn check_function_id(function: u8) -> Result<(), PciInvalidAddressError> {
    if function & 0b11111000 != 0 {
        Err(PciInvalidAddressError::Function(function))
    } else {
        Ok(())
    }
}

/// Checks that the given register offset is valid for the standard PCI access scheme.
/// More registers may be present, but these can only be accessed using PCIe.
fn check_register_offset(offset: u8) -> Result<(), PciInvalidAddressError> {
    if offset & 0b00000011 != 0 {
        Err(PciInvalidAddressError::Offset(offset))
    } else {
        Ok(())
    }
}

impl Display for PciInvalidAddressError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Device(device) => write!(
                f,
                "Device ID {device} is too large to fit the format of a PCI address."
            ),
            Self::Function(function) => write!(
                f,
                "Function ID {function} is too large to fit the format of a PCI address."
            ),
            Self::Offset(offset) => write!(f, "Register offset {offset} is not 4-byte aligned."),
        }
    }
}

// TODO either when core::error::Error is stable or when the kernel gets access to std:
// Remove this comment
//impl Error for PciInvalidAddressError {}

/// The address of a specific 32-bit register of a PCI device
#[derive(Debug)]
pub struct PciRegister {
    /// The bus number
    bus: u8,
    /// The device number on the [bus][PciRegister::bus]
    device: u8,
    /// The function number on the [device][PciRegister::device]
    function: u8,
    /// The register offset. Stored in bytes, but must be 4-byte aligned.
    offset: u8,
}

impl PciRegister {
    /// Calculates a PCI address from the `bus`, `device`, `function`, and `offset`.  
    ///
    /// If any of the highest 3 bits of `device`, the 5 highest bits of `function`, or the 2 lowest bits of `offset` are set,
    /// the address is invalid and `None` will be returned.
    fn from_parts(
        bus: u8,
        device: u8,
        function: u8,
        offset: u8,
    ) -> Result<Self, PciInvalidAddressError> {
        // Check that `device`, `function`, and `offset` have valid values
        check_device_id(device)?;
        check_function_id(function)?;
        check_register_offset(offset)?;

        Ok(Self {
            bus,
            device,
            function,
            offset,
        })
    }

    /// Gets the address that needs to be written to the [`CONFIG_ADDRESS`] port to select this register.
    const fn get_address(&self) -> u32 {
        // Sanity check that the address is valid
        // Check that `device`, `function`, and `offset` have valid values before calculating the address
        if self.device & 0b11100000 != 0 {
            panic!("Invalid address");
        }
        if self.function & 0b11111000 != 0 {
            panic!("Invalid address");
        }
        if self.offset & 0b00000011 != 0 {
            panic!("Invalid address");
        }

        (1 << 31) // Set the `enable` bit
        | ((self.bus as u32) << 16)
        | ((self.device as u32) << 11)
        | ((self.function as u32) << 8)
        | (self.offset as u32)
    }

    /// Reads from the [`PciRegister`]
    /// # Safety
    /// This function is unsafe as the read may have side-effects depending on the PCI device in question
    unsafe fn read_u32(self) -> u32 {
        // SAFETY:
        // The safety of this operation is the caller's responsibility
        unsafe {
            Port::new(CONFIG_ADDRESS).write(self.get_address());
            Port::new(CONFIG_DATA).read()
        }
    }

    /// Writes the value to the [`PciRegister`]
    ///
    /// # Safety
    /// This function is unsafe as the write may have side-effects depending on the PCI device in question
    unsafe fn write_u32(self, value: u32) {
        // SAFETY:
        // The safety of this operation is the caller's responsibility
        unsafe {
            Port::new(CONFIG_ADDRESS).write(self.get_address());
            Port::new(CONFIG_DATA).write(value)
        }
    }
}

/// Represents a specific function of a [`PciDevice`]
#[derive(Debug)]
pub struct PciFunction {
    /// The bus number
    bus: u8,
    /// The device number on the [bus][PciFunction::bus]
    device: u8,
    /// The function number on the [device][PciFunction::device]
    function: u8,
}

impl Display for PciFunction {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{:02x}:{:02x}.{:01x}",
            self.bus, self.device, self.function
        )
    }
}

impl PciFunction {
    /// Constructs a new [`PciFunction`] from the `bus`, `device`, and `function` numbers.
    /// These values are checked before construction and construction will fail ()
    fn new(bus: u8, device: u8, function: u8) -> Result<Self, PciInvalidAddressError> {
        // Check that `device`, `function`, and `offset` have valid values
        check_device_id(device)?;
        check_function_id(function)?;

        Ok(Self {
            bus,
            device,
            function,
        })
    }

    /// Gets a [`PciRegister`] for the register on this device with the given byte offset
    pub fn register(&self, offset: u8) -> Result<PciRegister, PciInvalidAddressError> {
        PciRegister::from_parts(self.bus, self.device, self.function, offset)
    }

    /// Reads the PCI device's header
    pub fn get_header(&self) -> Result<Option<PciHeader>, InvalidValueError> {
        let mut registers = [0; 0x11];

        for (i, register) in registers.iter_mut().enumerate() {
            // SAFETY:
            // Reading from the header should not have side effects
            unsafe {
                *register = self.register(i as u8 * 4).unwrap().read_u32();
            }
        }

        PciHeader::from_registers(registers)

    }
}

/// The bus:device address of a PCI device
#[derive(Debug)]
pub struct PciDevice {
    /// The bus number
    bus: u8,
    /// The device number on the [bus][PciDevice::bus]
    device: u8,
}

impl PciDevice {
    /// Constructs a new [`PciDevice`] from the given `bus` and `device` numbers
    pub fn new(bus: u8, device: u8) -> Result<Self, PciInvalidAddressError> {
        // Check that `device`, has a valid value
        check_device_id(device)?;

        Ok(Self { bus, device })
    }

    /// Gets the [`PciFunction`] of this device with the given function number
    pub fn function(&self, function: u8) -> Result<PciFunction, PciInvalidAddressError> {
        PciFunction::new(self.bus, self.device, function)
    }
}
