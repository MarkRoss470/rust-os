//! Functionality to read the capability registers of PCI devices
//!
//! The capability registers of a PCI device are a linked list of data structures in a device's configuration space.
//! Each item contains the type of capability, the pointer to the next item, and then other registers specific to the capability.
#![allow(dead_code)] // TODO: remove when the warnings are better

// TODO: deduplicate all these modules
pub mod capability;
pub mod msi;
pub mod msix;

use core::fmt::Debug;

pub use msi::*;
pub use capability::*;

use crate::util::generic_mutability::{Immutable, Mutable};

use super::PciMappedFunction;

#[bitfield(u16)]
pub struct MsiControl {
    /// Whether message signalled interrupts are enabled for this device
    enable: bool,

    /// Represents the number of multi-message interrupts the device supports.
    /// The number of vectors is 2 to the power of this value
    /// (e.g. a value of 0 means just 1 interrupt vector, a value of 4 means 16 vectors).
    /// Valid values are in the range `0..=5`
    #[bits(3)]
    multi_message_capable: u8,

    /// Represents the number of multi-message interrupts enabled on the device.
    /// The number of vectors is 2 to the power of this value
    /// (e.g. a value of 0 means just 1 interrupt vector, a value of 4 means 16 vectors).
    /// Valid values are in the range `0..=`[`multi_message_capable`][MsiControl::multi_message_capable]
    #[bits(3)]
    multi_message_enable: u8,

    /// Whether the device supports 64-bit
    is_64_bit: bool,

    /// Whether the device supports masking on a per-interrupt basis
    per_vector_masks: bool,

    #[bits(7)]
    #[doc(hidden)]
    reserved0: u8,
}

/// How an MSI interrupt is handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[allow(dead_code)]
pub enum X64MsiDeliveryMode {
    /// Deliver the signal to all the agents listed in the destination. The Trigger Mode for
    /// fixed delivery mode can be edge or level.
    Fixed = 0,
    /// Deliver the signal to the agent that is executing at the lowest priority of all
    /// agents listed in the destination field. The trigger mode can be edge or level.
    LowestPriority = 1,
    /// The delivery mode is edge only. For systems that rely on SMI semantics, the [`vector`] field is ignored
    /// but must be programmed to all zeroes for future compatibility.
    ///
    /// [`vector`]: X64MsiAddress::vector
    SystemManagementInterrupt = 2,
    /// Deliver the signal to all the agents listed in the destination field. [`vector`] is ignored.
    /// NMI is an edge triggered interrupt regardless of the Trigger Mode Setting.
    ///
    /// [`vector`]: X64MsiAddress::vector
    NonMaskableInterrupt = 4,
    /// Deliver this signal to all the agents listed in the destination field. [`vector`] is ignored.
    /// INIT is an edge triggered interrupt regardless of the Trigger Mode Setting.
    ///
    /// [`vector`]: X64MsiAddress::vector
    Init = 5,
    /// Deliver the signal to the INTR signal of all agents in the destination field (as an interrupt
    /// that originated from an 8259A compatible interrupt controller). The vector is supplied by the INTA cycle
    /// issued by the activation of the ExtINT. ExtINT is an edge triggered interrupt.
    ExtInit = 7,
}

/// Whether the interrupt is edge or level triggered, and if level triggered, whether the interrupt should be an assert or deassert message
///
/// Some values of this register only match with certain values of [`X64MsiDeliveryMode`] - see the docs for individual variants of that enum for info.
///
/// TODO: What do the different values of this enum actually change?
/// Hypotheses: `LevelAssert` starts the interrupt being triggered and `LevelDeassert` stops it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[allow(dead_code)]
pub enum X64MsiTriggerMode {
    /// The interrupt should be edge triggered
    Edge = 0b00,
    /// TODO: What's this?
    LevelDeassert = 0b10,
    /// TODO: What's this?
    LevelAssert = 0b11,
}

/// The structure of an address to give to an MSI device, for x64 platforms.
///
/// These registers are described in the [Intel 64 and IA-32 Architectures Software Developer’s Manual] Volume 3 chapter 11.11 for more info
///
/// [Intel 64 and IA-32 Architectures Software Developer’s Manual]: https://cdrdv2.intel.com/v1/dl/getContent/671200
#[derive(Debug, Clone, Copy)]
pub struct X64MsiAddress {
    /// The APIC ID of the core where the interrupt should run, or the message destination address in logical mode
    pub apic_id: u8,

    /// Whether the message should be directed to the processor with the lowest interrupt priority
    /// among processors that can receive the interrupt.
    pub redirection_hint: bool,

    /// Whether to use logical destination mode.
    ///
    /// In logical mode, [`apic_id`] is a message destination address which then gets mapped to an APIC ID.
    ///
    /// TODO: look into this - I think it's only needed for virtualisation or for systems with more than 255 cores.
    ///
    /// [`apic_id`]: X64MsiAddress::apic_id
    pub destination_is_logical: bool,

    /// The delivery mode
    pub delivery_mode: X64MsiDeliveryMode,

    /// The trigger mode
    pub trigger_mode: X64MsiTriggerMode,

    /// The interrupt vector to send to the core
    pub vector: u8,
}

impl X64MsiAddress {
    /// Converts the address into the values which need to be written to an MSI device's
    /// `message_address` and `message_data` registers, or an entry in an MSI-X table.
    pub fn to_address_and_data(self) -> (u32, u16) {
        let address = 0xFEE << 20
            | (self.apic_id as u32) << 12
            | (self.redirection_hint as u32) << 3
            | (self.delivery_mode as u32) << 2;

        let data = (self.delivery_mode as u16) << 8
            | (self.trigger_mode as u16) << 14
            | self.vector as u16;

        (address, data)
    }
}

#[bitfield(u16)]
pub struct MsixControl {
    /// The index of the last item in the table of interrupts.
    #[bits(11)]
    pub last_index: u16,

    #[bits(3)]
    #[doc(hidden)]
    reserved0: u16,

    /// Whether to mask interrupts from this function.
    /// If [`enable`][MsixControl::enable] is `true`,
    /// setting this field to `true` as well will prevent this function from triggering interrupts.
    pub function_mask: bool,
    /// Sets whether the device uses MSI-X to deliver interrupts.
    /// If this is set to `false`, the device will use pin-based interrupts instead.
    pub enable: bool,
}

#[bitfield(u32)]
pub struct MsixVectorControl {
    /// If `true`, this interrupt vector is masked and will not be triggered.
    pub masked: bool,

    #[bits(31)]
    #[doc(hidden)]
    reserved0: u32,
}

/// An entry in the MSI-X vector table, describing one interrupt source for the function
#[derive(Clone, Copy)]
#[repr(C)]
pub struct MsixTableEntry {
    /// The least significant 32 bits of the physical address the device will write to to signal an interrupt
    pub message_address_low: u32,
    /// The most significant 32 bits of the physical address the device will write to to signal an interrupt
    pub message_address_high: u32,
    /// The data the device will write to the address indicated by [`message_address_low`] and [`message_address_high`]
    ///
    /// [`message_address_low`]: MsixTableEntry::message_address_low
    /// [`message_address_high`]: MsixTableEntry::message_address_high
    pub message_data: u32,
    /// Flags about the vector
    pub vector_control: MsixVectorControl,
}

impl Debug for MsixTableEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let message_address =
            (self.message_address_high as u64) << 32 | self.message_address_low as u64;

        f.debug_struct("MsixTableEntry")
            .field("message_address", &format_args!("{message_address:#x}"))
            .field("message_data", &format_args!("{:#?}", self.message_data))
            .field("vector_control", &self.vector_control)
            .finish()
    }
}

/// The register offset of the PCI register containing the index of the first capability register,
/// if [`has_capabilities_list`][super::registers::StatusRegister::has_capabilities_list] is true
const CAPABILITIES_REGISTER_OFFSET: u8 = 0xD;

impl PciMappedFunction {
    /// Gets an iterator over the capability registers of the given `function`.
    /// Each item of the returned iterator is a tuple of ([`CapabilityEntry`], [`u8`]) where the `u8`
    /// is the register index of where the capability starts
    pub fn capabilities(&self) -> Option<impl Iterator<Item = (CapabilityEntry<Immutable>, u8)> + '_> {
        let header = self.read_header().unwrap().unwrap();
        if !header.status.has_capabilities_list() {
            return None;
        }

        // SAFETY: This register is the location of the capabilities pointer if `has_capabilities_list` is true
        // Reading from this register has no side effects
        let capabilities_pointer_register = unsafe { self.read_reg(CAPABILITIES_REGISTER_OFFSET) };
        let capabilities_pointer = (capabilities_pointer_register >> 2 & 0b11111100) as u8;

        let mut i = capabilities_pointer;

        Some(core::iter::from_fn(move || {
            // The last capability in the list has a null next pointer
            if i == 0 {
                None
            } else {
                // SAFETY: `i` was either read directly from the device's registers or returned from `CapabilityEntry::new`, so it is valid.
                let (capability_id, next_pointer) = unsafe { CapabilityEntry::new(self, i) };

                i = next_pointer;
                Some((capability_id, i))
            }
        }))
    }

    /// Gets an iterator over the capability registers of the given `function`, returning mutable [`CapabilityEntry`]s.
    /// Each item of the returned iterator is a tuple of ([`CapabilityEntry`], [`u8`]) where the `u8`
    /// is the register index of where the capability starts
    pub fn capabilities_mut(
        &mut self,
    ) -> Option<impl Iterator<Item = (CapabilityEntry<'_, Mutable>, u8)> + '_> {
        let header = self.read_header().unwrap().unwrap();
        if !header.status.has_capabilities_list() {
            return None;
        }

        // SAFETY: This register is the location of the capabilities pointer if `has_capabilities_list` is true
        // Reading from this register has no side effects
        let capabilities_pointer_register = unsafe { self.read_reg(CAPABILITIES_REGISTER_OFFSET) };
        let capabilities_pointer = (capabilities_pointer_register >> 2 & 0b11111100) as u8;

        let mut i = capabilities_pointer;

        Some(core::iter::from_fn(move || {
            // The last capability in the list has a null next pointer
            if i == 0 {
                None
            } else {
                // SAFETY: `i` was either read directly from the device's registers or returned from `CapabilityEntry::new`, so it is valid.
                let (capability_id, next_pointer) = unsafe { CapabilityEntry::new_mut(self, i) };

                i = next_pointer;

                Some((capability_id, i))
            }
        }))
    }
}
