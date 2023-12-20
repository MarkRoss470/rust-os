//! Structs representing different specificities of PCI hardware -
//! [device][PciDevice], [function][PciFunction], and [register][PciRegister]

use core::fmt::Display;

use x86_64::PhysAddr;

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
#[derive(Debug, Clone, Copy)]
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

    /// Gets the next register - i.e. the same as this but with [`offset`][PciRegister::offset] 4 bytes greater
    ///
    /// Returns [`None`] if this register is the PCI device's last register
    pub fn next(&self) -> Option<Self> {
        Some(Self {
            offset: self.offset.checked_add(4)?,
            ..*self
        })
    }

    /// Gets the [`PciDevice`] which this register is a part of
    pub fn get_device(&self) -> PciDevice {
        PciDevice {
            bus: self.bus,
            device: self.device,
        }
    }

    /// Gets the [`PciFunction`] which this register is a part of
    pub fn get_function(&self) -> PciFunction {
        PciFunction {
            bus: self.bus,
            device: self.device,
            function: self.function,
        }
    }

    /// Gets the bus number of the device this register is a part of
    pub fn get_bus(&self) -> u8 {
        self.bus
    }
}

/// Represents a specific function of a [`PciDevice`]
#[derive(Debug, Clone, Copy)]
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

    /// Gets the [`PciDevice`] which this function is a part of
    pub fn get_device(&self) -> PciDevice {
        PciDevice {
            bus: self.bus,
            device: self.device,
        }
    }

    /// Gets the device number of the device this function is a part of
    pub fn get_device_number(&self) -> u8 {
        self.device
    }

    /// Gets the bus number of the device this function is a part of
    pub fn get_bus_number(&self) -> u8 {
        self.bus
    }

    /// Gets the function number
    pub fn get_function_number(&self) -> u8 {
        self.function
    }

    /// Gets the starting address of this function's PCIe registers, assuming the controller's registers start at `start`
    pub fn get_register_address(&self, min_bus: u8, start: PhysAddr) -> PhysAddr {
        let offset = (self.get_bus_number() as usize - min_bus as usize) << 20
            | (self.get_device_number() as usize) << 15
            | (self.get_function_number() as usize) << 12;

        start + offset
    }
}

/// The bus:device address of a PCI device
#[derive(Debug, Clone, Copy)]
pub struct PciDevice {
    /// The bus number
    bus: u8,
    /// The device number on the [bus][PciDevice::bus]
    device: u8,
}

impl PciDevice {
    /// Constructs a new [`PciDevice`] from the given `bus` and `device` numbers
    pub fn new(segment: u16, bus: u8, device: u8) -> Result<Self, PciInvalidAddressError> {
        // Check that `device`, has a valid value
        check_device_id(device)?;

        Ok(Self { bus, device })
    }

    /// Gets the [`PciFunction`] of this device with the given function number
    pub fn function(&self, function: u8) -> Result<PciFunction, PciInvalidAddressError> {
        PciFunction::new(self.bus, self.device, function)
    }

    /// Gets the bus number this device is on
    pub fn get_bus_number(&self) -> u8 {
        self.bus
    }

    /// Gets the device number of this device on its bus.
    pub fn get_device_number(&self) -> u8 {
        self.device
    }
}
