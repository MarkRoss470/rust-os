//! Types for reading a controller's extended capability registers

use core::{marker::PhantomData, ptr};

use super::supported_protocol::{ProtocolSpeedId, SupportedProtocolCapability};

/// A capability in the controller's [`ExtendedCapabilityRegisters`]
#[derive(Debug, Clone, Copy)]
pub enum Capability<'a> {
    /// Provides the XHCI Pre-OS to OS Handoff Synchronization support capability.
    UsbLegacySupport,
    /// Enumerates the versions of USB supported by the controller and which ports use which versions
    SupportedProtocol(SupportedProtocolCapability<'a>),
    /// Defines power management for non-PCI XHCI implementations
    ExtendedPowerManagement,
    /// Hardware virtualisation support
    IoVirtualisation,
    /// Defines interrupt support for non-PCI XHCI implementations
    MessageInterrupt,
    /// Allows specifying RAM for the [`UsbDebug`] capability before system memory is initialised
    ///
    /// [`UsbDebug`]: Capability::UsbDebug
    LocalMemory,
    /// Defines support for debugging other devices over USB
    UsbDebug,
    /// Defines interrupt support for non-PCI XHCI implementations
    ExtendedMessageInterrupt,

    /// Reserved for future use
    Reserved(u8),
    /// Vendor defined codes
    VendorDefined(u8),
}

/// The controller's list of extended capabilities, which describes aspects of the controller's operation
/// such as the supported USB protocol versions and support for debugging
#[derive(Debug)]
pub struct ExtendedCapabilityRegisters {
    /// The pointer to the start of the capability list
    ptr: *const RawCapability,
}

impl ExtendedCapabilityRegisters {
    /// Constructs a new [`ExtendedCapabilityRegisters`] from the given pointer.
    ///
    /// # Safety
    /// * `ptr` must point to the start of an XHCI controller's _Extended Capability Registers_.
    ///     This pointer can be calculated using the [`extended_capabilities_pointer`] field.
    /// * `ptr` must be valid for reads for the lifetime of the constructed object.
    ///
    /// [`extended_capabilities_pointer`]: super::super::capability::CapabilityParameters1::extended_capabilities_pointer
    pub unsafe fn new(ptr: *const u32) -> Self {
        assert!(ptr.is_aligned());

        Self { ptr: ptr.cast() }
    }

    /// Gets an iterator of the controller's capabilities
    pub fn capabilities(&self) -> Capabilities {
        // SAFETY: `self.ptr` points to the Extended Capability Registers.
        // This is a linked list of Capability structures, so the pointer points to a Capability.
        // This also means the pointer is valid to read the entire list as this will be completely
        // within the capability registers.
        unsafe { Capabilities::new(self.ptr) }
    }

    /// Gets the [`ProtocolSpeedId`] for the given combination of port ID and [`port_speed`]
    ///
    /// [`ProtocolSpeedId`]: super::supported_protocol::ProtocolSpeedId
    /// [`port_speed`]: super::super::operational::port_registers::StatusAndControl::port_speed
    pub fn get_protocol_speed(&self, port_id: u8, speed_id: u8) -> Option<ProtocolSpeedId> {
        self.get_protocol_for_port(port_id)?
            .speed_ids()
            .iter()
            .find(|id| id.speed_id_value() == speed_id)
            .cloned()
    }

    /// Gets the value to write to the [`slot_type`] field of [`EnableSlotTrb`]s for the given port
    ///
    /// [`slot_type`]: super::super::super::trb::command::slot::EnableSlotTrb::slot_type
    /// [`EnableSlotTrb`]: super::super::super::trb::command::slot::EnableSlotTrb
    pub fn slot_type(&self, port_id: u8) -> Option<u8> {
        self.get_protocol_for_port(port_id)
            .map(|s| s.protocol_slot_type())
    }

    /// Gets the [`SupportedProtocolCapability`] for the given port number
    pub fn get_protocol_for_port(&self, port_id: u8) -> Option<SupportedProtocolCapability> {
        self.capabilities()
            .filter_map(|c| match c {
                Capability::SupportedProtocol(s) => Some(s),
                _ => None,
            })
            .find(|s| {
                (s.compatible_port_offset()
                    ..=s.compatible_port_offset() + s.compatible_port_count())
                    .contains(&port_id)
            })
    }
}

impl<'a> IntoIterator for &'a ExtendedCapabilityRegisters {
    type Item = Capability<'a>;
    type IntoIter = Capabilities<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.capabilities()
    }
}

/// An iterator over the extended capabilities of a controller
pub struct Capabilities<'a> {
    /// The next capability in the series. If all capabilities have been produced, this will be null.
    /// This pointer is valid for the lifetime `'a`.
    ptr: *const RawCapability,
    /// Tracks the lifetime of the list this struct is iterating over
    p: PhantomData<&'a ExtendedCapabilityRegisters>,
}

impl<'a> Capabilities<'a> {
    /// Constructs a new [`Capabilities`] iterator starting at the capability at the given pointer.
    ///
    /// # Safety
    /// * `ptr` must point to a [`Capability`] structure in the MMIO space of an XHCI controller.
    /// * `ptr` and all the subsequent calculated pointers for capabilities later in the list
    ///     must be valid for reads for the lifetime `'a`.
    unsafe fn new(ptr: *const RawCapability) -> Self {
        Self {
            ptr,
            p: PhantomData,
        }
    }
}

impl<'a> Iterator for Capabilities<'a> {
    type Item = Capability<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.ptr.is_null() {
            return None;
        }

        // SAFETY: This pointer is valid as it was either set using the extended capability registers offset
        // or the offset from the previous capability
        let raw_capability = unsafe { self.ptr.read_volatile() };

        let capability = match raw_capability.capability_id() {
            1 => Capability::UsbLegacySupport,
            // SAFETY: The pointer points to a capability and is valid for the lifetime `'a`.
            2 => Capability::SupportedProtocol(unsafe {
                SupportedProtocolCapability::new(self.ptr.cast())
            }),
            3 => Capability::ExtendedPowerManagement,
            4 => Capability::IoVirtualisation,
            5 => Capability::MessageInterrupt,
            6 => Capability::LocalMemory,
            10 => Capability::UsbDebug,
            17 => Capability::ExtendedMessageInterrupt,

            c @ (0 | 7..=9 | 11..=16 | 18..=191) => Capability::Reserved(c),
            c @ (192..=255) => Capability::VendorDefined(c),
        };

        self.ptr = match raw_capability.next_pointer() {
            0 => ptr::null(),
            // SAFETY: This offset is small so can't wrap around, and is still within the controller's registers.
            // Offset is in 32-bit units, so multiply by 4 to get byte offset.
            offset => unsafe { self.ptr.byte_add(offset as usize * 4) },
        };

        Some(capability)
    }
}

#[bitfield(u32)]
struct RawCapability {
    capability_id: u8,
    next_pointer: u8,
    data: u16,
}
