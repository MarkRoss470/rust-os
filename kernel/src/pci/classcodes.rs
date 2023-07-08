//! Enums related to PCI class code values

/// An error resulting from an invalid value being parsed as a [`ClassCode`]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidValueError {
    /// The value which is invalid
    pub value: u8,
    /// The name of the field for which an invalid value was found
    pub field: &'static str,
}

impl core::fmt::Display for InvalidValueError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Invalid value 0x{:x} for field '{}'", self.value, self.field)
    }
}

/// A device which does not fall into the other categories
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnclassifiedDeviceType {
    /// The device does not support VGA
    NonVgaCompatible,
    /// The device supports VGA
    VgaCompatible,
}

impl UnclassifiedDeviceType {
    /// Constructs an [`UnclassifiedDeviceType`] from the subclass
    fn from_subclass(subclass: u8) -> Result<Self, InvalidValueError> {
        match subclass {
            0x00 => Ok(Self::NonVgaCompatible),
            0x01 => Ok(Self::VgaCompatible),
            _ => Err(InvalidValueError { value: subclass, field: "Unclassified device subclass" })
        }
    }
}

/// A type of mass storage controller
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MassStorageControllerType {
    /// An SCSI bus controller
    SCSIBusController,
    /// An IDE controller 
    IDEController,
    /// A floppy disk controller
    FloppyDiskController,
    /// An IPI bus controller
    IPIBusController,
    /// A RAID controller
    RAIDController,
    /// An ATA controller
    ATAController,
    /// A serial ATA (SATA) controller
    SerialATAController,
    /// A serial attached SCSI controller
    SerialAttachedSCSIController,
    /// A non volatile memory (including NVME) controller 
    NonVolatileMemoryController,
    /// A different type of storage controller
    Other,
}

impl MassStorageControllerType {
    /// Constructs a [`MassStorageControllerType`] from the `subclass` and `prog_if`
    fn from_subclass(subclass: u8, _prog_if: u8) -> Result<Self, InvalidValueError> {
        match subclass {
            0x00 => Ok(Self::SCSIBusController),
            0x01 => Ok(Self::IDEController),
            0x02 => Ok(Self::FloppyDiskController),
            0x03 => Ok(Self::IPIBusController),
            0x04 => Ok(Self::RAIDController),
            0x05 => Ok(Self::ATAController),
            0x06 => Ok(Self::SerialATAController),
            0x07 => Ok(Self::SerialAttachedSCSIController),
            0x08 => Ok(Self::NonVolatileMemoryController),
            0x80 => Ok(Self::Other),
            _ => Err(InvalidValueError { value: subclass, field: "Unclassified device subclass" })
        }
    }
}

/// A type of USB controller
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum USBControllerType {
    /// Universal host controller interface (UHCI) controller
    Uhci,
    /// Open host controller interface (OHCI) controller
    Ohci,
    /// Enhanced host controller interface (EHCI) controller
    Ehci,
    /// Extensible host controller interface (XHCI) controller
    Xhci,
    /// An unspecified controller type
    Unspecified,
    /// The PCI device is a USB device rather than a USB controller
    Device,
}

impl USBControllerType {
    /// Construct a [`USBControllerType`] from the associated programming interface
    fn from_prog_if(prog_if: u8) -> Result<Self, InvalidValueError> {
        match prog_if {
            0x00 => Ok(USBControllerType::Uhci),
            0x10 => Ok(USBControllerType::Ohci),
            0x20 => Ok(USBControllerType::Ehci),
            0x30 => Ok(USBControllerType::Xhci),
            0x80 => Ok(USBControllerType::Unspecified),
            0xFE => Ok(USBControllerType::Device),
            _ => Err(InvalidValueError { value: prog_if, field: "USB " })
        }
    }
}

/// A type of serial bus controller
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerialBusControllerType {
    /// A [FireWire](https://en.wikipedia.org/wiki/IEEE_1394) controller
    FireWireController,
    /// An [ACCESS.bus](https://en.wikipedia.org/wiki/ACCESS.bus) controller
    AccessBusController,
    /// An [SSA](https://en.wikipedia.org/wiki/Serial_Storage_Architecture) controller
    Ssa,
    /// A [Universal Serial Bus](https://en.wikipedia.org/wiki/USB) (USB) controller
    UsbController(USBControllerType),
    /// A [Fibre Channel](https://en.wikipedia.org/wiki/Fibre_Channel) controller
    FibreChannel,
    /// A [System Management Bus](https://en.wikipedia.org/wiki/System_Management_Bus) (SMBus) controller
    SMBusController,
    /// An [InfiniBand](https://en.wikipedia.org/wiki/InfiniBand) controller
    InfiniBandController,
    /// An [Intelligent Platform Management Interface](https://en.wikipedia.org/wiki/Intelligent_Platform_Management_Interface) controller
    IpmiInterface,
    /// A [Serial Real-time Communication System](https://en.wikipedia.org/wiki/SERCOS_interface) (SERCOS) controller
    SercosInterface,
    /// A [Controller Area Network Bus](https://en.wikipedia.org/wiki/CAN_bus) (CANBus) controller
    CanBusController,
    /// A controller for another type of serial bus
    Other,
}

impl SerialBusControllerType {
    /// Constructs a [`SerialBusControllerType`] from its `subclass` and `prog_if`
    fn from_subclass(subclass: u8, prog_if: u8) -> Result<Self, InvalidValueError> {
        match subclass {
            0x00 => Ok(Self::FireWireController),
            0x01 => Ok(Self::AccessBusController),
            0x02 => Ok(Self::Ssa),
            0x03 => Ok(Self::UsbController(USBControllerType::from_prog_if(prog_if)?)),
            0x04 => Ok(Self::FibreChannel),
            0x05 => Ok(Self::SMBusController),
            0x06 => Ok(Self::InfiniBandController),
            0x07 => Ok(Self::IpmiInterface),
            0x08 => Ok(Self::SercosInterface),
            0x09 => Ok(Self::CanBusController),
            0x80 => Ok(Self::Other),
            _ => Err(InvalidValueError { value: subclass, field: "Serial bus controller type" })
        }
    }
}



/// A general function performed by a PCI device
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassCode {
    /// 0x00
    Unclassified (UnclassifiedDeviceType),
    /// 0x01
    MassStorageController (MassStorageControllerType),
    /// 0x02
    NetworkController,
    /// 0x03
    DisplayController,
    /// 0x04
    MultimediaController,
    /// 0x05
    MemoryController,
    /// 0x06
    Bridge,
    /// 0x07
    SimpleCommunicationController,
    /// 0x08
    BaseSystemPeripheral,
    /// 0x09
    InputDeviceController,
    /// 0x0A
    DockingStation,
    /// 0x0B
    Processor,
    /// 0x0C
    SerialBusController(SerialBusControllerType),
    /// 0x0D
    WirelessController,
    /// 0x0E
    IntelligentController,
    /// 0x0F
    SatelliteCommunicationController,
    /// 0x10
    EncryptionController,
    /// 0x11
    SignalProcessingController,
    /// 0x12
    ProcessingAccelerator,
    /// 0x13
    NonEssentialInstrumentation,
    
    /// 0x40
    CoProcessor,

    /// 0xFF
    Unassigned,
}

impl ClassCode {
    /// Construct a [`ClassCode`] from its `class_code`, `subclass`, and `prog_if`
    pub fn new(class_code: u8, subclass: u8, prog_if: u8) -> Result<Self, InvalidValueError> {
        match class_code {
            0x00 => Ok(Self::Unclassified (UnclassifiedDeviceType::from_subclass(subclass)?)),
            0x01 => Ok(Self::MassStorageController(MassStorageControllerType::from_subclass(subclass, prog_if)?)),
            0x02 => Ok(Self::NetworkController),
            0x03 => Ok(Self::DisplayController),
            0x04 => Ok(Self::MultimediaController),
            0x05 => Ok(Self::MemoryController),
            0x06 => Ok(Self::Bridge),
            0x07 => Ok(Self::SimpleCommunicationController),
            0x08 => Ok(Self::BaseSystemPeripheral),
            0x09 => Ok(Self::InputDeviceController),
            0x0A => Ok(Self::DockingStation),
            0x0B => Ok(Self::Processor),
            0x0C => Ok(Self::SerialBusController(SerialBusControllerType::from_subclass(subclass, prog_if)?)),
            0x0D => Ok(Self::WirelessController),
            0x0E => Ok(Self::IntelligentController),
            0x0F => Ok(Self::SatelliteCommunicationController),
            0x10 => Ok(Self::EncryptionController),
            0x11 => Ok(Self::SignalProcessingController),
            0x12 => Ok(Self::ProcessingAccelerator),
            0x13 => Ok(Self::NonEssentialInstrumentation),
            
            0x40 => Ok(Self::CoProcessor),
            0xFF => Ok(Self::Unassigned),

            _ => Err(InvalidValueError { value: class_code, field: "Class code" })
        }
    }
}