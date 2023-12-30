//! The [`MessageSignalledInterruptsCapability`] type for a read-only view into a PCI device's MSI capability

use core::marker::PhantomData;

use crate::pci::{PciMappedFunction, PcieMappedRegisters};

use super::MsiControl;

/// A read-only view into the MSI capability of a PCI device. If mutability is needed, use [`MessageSignalledInterruptsCapabilityMut`].
///
/// [`MessageSignalledInterruptsCapabilityMut`]: super::msi_mut::MessageSignalledInterruptsCapabilityMut
#[derive(Debug)]
pub struct MessageSignalledInterruptsCapability<'a> {
    /// The memory-mapped control register
    control: *const MsiControl,
    /// The memory-mapped least significant half of the message address register
    message_address_low: *const u32,
    /// The memory-mapped most significant half of the message address register
    message_address_high: Option<*const u32>,

    /// The memory-mapped data register
    data: *const u16,

    /// PhantomData for the lifetime of the memory-mapped registers
    _p: PhantomData<&'a PcieMappedRegisters>,
}

impl<'a> MessageSignalledInterruptsCapability<'a> {
    /// # Safety:
    /// * `offset` is the register (not byte) offset of an MSI capabilities structure within the configuration space of `function`
    pub(super) unsafe fn new(function: &PciMappedFunction, offset: u8) -> Self {
        let capability_start_ptr =
        // SAFETY: `offset` is the offset of an MSI capabilities structure
        unsafe { function.registers.as_ptr::<u32>().add(offset as _) };

        assert!(capability_start_ptr.is_aligned_to(4));
        assert!(!capability_start_ptr.is_null());

        // SAFETY: The control register is at offset 2 in the MSI capabilities structure
        let control_ptr = unsafe { capability_start_ptr.cast::<MsiControl>().add(1) };
        // SAFETY: The pointer is valid
        let control = unsafe { control_ptr.read_volatile() };

        let is_64_bit = control.is_64_bit();

        let message_address_high = if is_64_bit {
            // SAFETY: The message address high register is at offset 8 in the MSI capabilities structure
            unsafe { Some(capability_start_ptr.add(8).cast()) }
        } else {
            None
        };

        let offset_for_64_bit = if is_64_bit { 4 } else { 0 };

        // SAFETY: The message address low register is at offset 4 in the MSI capabilities structure
        let message_address_low = unsafe { capability_start_ptr.add(4).cast() };

        // SAFETY: The data register is at offset 8 in the MSI capabilities structure
        let data = unsafe { capability_start_ptr.add(12 + offset_for_64_bit).cast() };

        Self {
            control: control_ptr,
            message_address_low,
            message_address_high,

            data,
            _p: PhantomData,
        }
    }

    /// Reads the [`control`] register
    ///
    /// [`control`]: MessageSignalledInterruptsCapability::control
    pub fn control(&self) -> MsiControl {
        // SAFETY: It's unsound to create a reference in to a `PcieMappedRegisters`, so no references exist for this data
        unsafe { self.control.read_volatile() }
    }

    /// Reads the message address field.
    /// Note that this is _not_ just a physical address - it's a platform-specific format which could contain various flags
    pub fn message_address(&self) -> u64 {
        // SAFETY: It's unsound to create a reference in to a `PcieMappedRegisters`, so no references exist for this data
        let (high, low) = unsafe {
            (
                self.message_address_high
                    .map(|p| p.read_volatile())
                    .unwrap_or(0),
                self.message_address_low.read_volatile(),
            )
        };

        (high as u64) << 32 | (low as u64)
    }

    /// Reads the [`data`] register
    ///
    /// [`data`]: MessageSignalledInterruptsCapability::data
    pub fn data(&self) -> u16 {
        // SAFETY: It's unsound to create a reference in to a `PcieMappedRegisters`, so no references exist for this data
        unsafe { self.data.read_volatile() }
    }
}
