//! The [`CapabilityEntry`] type for a constant view into a PCI device's capability list

use crate::pci::PciMappedFunction;

use super::{msix_const::MsixCapability, MessageSignalledInterruptsCapability};

/// A type of capability entry on a PCI device.
///
/// This struct represents a read-only view. If mutability is needed, use [`CapabilityEntryMut`] instead.
///
/// [`CapabilityEntryMut`]: super::capability_mut::CapabilityEntryMut
#[derive(Debug)]
pub enum CapabilityEntry<'a> {
    /// A placeholder capability, containing no extra registers
    Null,
    /// PCI Power Management Interface.
    ///
    /// Documentation for this capability can be found in the _PCI Bus Power Management Interface Specification_. (TODO: link)
    PciPowerManagementInterface,
    /// Accelerated Graphics Port
    ///
    /// Documentation for this capability can be found in the _Accelerated Graphics Port Interface Specification_. (TODO: link)
    AcceleratedGraphicsPort,
    /// Vital Product Data
    ///
    /// Documentation for this capability can be found in the _PCI Local Bus Specification_. (TODO: link)
    VitalProductData,
    /// Slot Identification
    ///
    /// This Capability structure identifies a bridge that provides external expansion capabilities.
    /// Documentation for this capability can be found in the _PCI-to-PCI Bridge Architecture Specification_. (TODO: link)
    SlotIdentification,
    /// Message Signalled Interrupts
    MessageSignalledInterrupts(MessageSignalledInterruptsCapability<'a>),
    /// Compact PCI Hot Swap
    CompactPciHotSwap,
    /// PCI-X
    /// 
    /// Documentation for this capability can be found in the _PCI-X Protocol Addendum to the PCI Local Bus Specification_. (TODO: link)
    PciX,
    /// Hyper-Transport
    /// 
    /// Documentation for this capability can be found in the _HyperTransport I/O Link Specification_.
    HyperTransport,
    /// A vendor specific capability
    VendorSpecific,
    /// Debug Port
    DebugPort,
    /// Compact PCI Central Resource Control
    /// 
    /// Documentation for this capability can be found in the _PICMG 2.13 Specification_. (TODO: link)
    CompactPciCentralResourceControl,
    /// PCI Hot Plug
    PciHotPlug,
    /// PCI Bridge Subsystem Vendor Id
    PciBridgeSubsystemVendorId,
    /// APG 8x
    Apg8x,
    /// Secure Device
    SecureDevice,
    /// PCI Express
    PciExpress,
    /// MSI-X
    MsiX(MsixCapability<'a>),
    /// SATA Config
    SataConfig,
    /// Advanced Features
    /// 
    /// Documentation for this capability can be found in the _Advanced Capabilities for Conventional PCI ECN_. (TODO: link)
    AdvancedFeatures,
    /// Enhanced Allocation
    EnhancedAllocation,
    /// Flattening Portal Bridge
    FlatteningPortalBridge,
    /// A reserved ID
    Reserved(u8),
}

impl<'a> CapabilityEntry<'a> {
    /// # Safety
    /// * `offset` is the register (not byte) index of a capabilities structure in the configuration space of `function`
    ///
    /// # Returns
    /// The parsed entry, and the register index of the next capability in the list
    pub(super) unsafe fn new(function: &PciMappedFunction, offset: u8) -> (Self, u8) {
        // SAFETY: This index was read from another PCI register, so it is correct
        let reg = unsafe { function.read_reg(offset) };

        let id = reg as u8;

        // Shift by 10 instead of 8 because the bottom 2 bits are reserved
        // This also converts between the byte offset and the register offset
        let next_pointer = ((reg >> 10) as u8) & 0b111111;

        // These IDs can be found in section 2 of the PCI Code and ID Assignment Specification
        // https://pcisig.com/sites/default/files/files/PCI_Code-ID_r_1_12__v9_Jan_2020.pdf
        let entry = match id {
            0x00 => Self::Null,
            0x01 => Self::PciPowerManagementInterface,
            0x02 => Self::AcceleratedGraphicsPort,
            0x03 => Self::VitalProductData,
            0x04 => Self::SlotIdentification,
            // SAFETY: `offset` is a valid index,
            // and the id value is 5 so it is an MSI capability
            0x05 => unsafe {
                Self::MessageSignalledInterrupts(MessageSignalledInterruptsCapability::new(
                    function, offset,
                ))
            },
            0x06 => Self::CompactPciHotSwap,
            0x07 => Self::PciX,
            0x08 => Self::HyperTransport,
            0x09 => Self::VendorSpecific,
            0x0A => Self::DebugPort,
            0x0B => Self::CompactPciCentralResourceControl,
            0x0C => Self::PciHotPlug,
            0x0D => Self::PciBridgeSubsystemVendorId,
            0x0E => Self::Apg8x,
            0x0F => Self::SecureDevice,
            0x10 => Self::PciExpress,
            // SAFETY: `offset` is a valid index,
            // and the id value is 0x11 so it is an MSI capability
            0x11 => unsafe { Self::MsiX(MsixCapability::new(function, offset)) },
            0x12 => Self::SataConfig,
            0x13 => Self::AdvancedFeatures,
            0x14 => Self::EnhancedAllocation,
            0x15 => Self::FlatteningPortalBridge,
            _ => Self::Reserved(id as _),
        };

        (entry, next_pointer)
    }
}
