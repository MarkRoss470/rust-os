//! The [`MessageSignalledInterruptsCapability`] type for a read-only view into a PCI device's MSI capability

use core::marker::PhantomData;

use crate::{
    pci::{PciMappedFunction, PcieMappedRegisters},
    util::generic_mutability::{Mutability, Mutable, Pointer, Reference},
};

use super::{MsiControl, X64MsiAddress};

/// A view into the MSI capability of a PCI device
#[derive(Debug)]
pub struct MessageSignalledInterruptsCapability<'a, M: Mutability> {
    /// The memory-mapped control register
    control: M::Ptr<MsiControl>,
    /// The memory-mapped least significant half of the message address register
    message_address_low: M::Ptr<u32>,
    /// The memory-mapped most significant half of the message address register
    message_address_high: Option<M::Ptr<u32>>,

    /// The memory-mapped data register
    data: M::Ptr<u16>,

    /// PhantomData for the lifetime of the memory-mapped registers
    _p: PhantomData<M::Ref<'a, PcieMappedRegisters>>,
}

impl<'a, M: Mutability> MessageSignalledInterruptsCapability<'a, M> {
    /// # Safety:
    /// * `offset` is the register (not byte) offset of an MSI capabilities structure within the configuration space of `function`
    pub(super) unsafe fn new(function: M::Ref<'_, PciMappedFunction>, offset: u8) -> Self {
        // SAFETY: `offset` is the offset of an MSI capabilities structure
        let capability_start_ptr = unsafe {
            function
                .as_const_ref()
                .registers
                .as_generic_ptr::<u32, M>()
                .add(offset as _)
        };

        assert!(capability_start_ptr.as_const_ptr().is_aligned_to(4));
        assert!(!capability_start_ptr.as_const_ptr().is_null());

        // SAFETY: The control register is at offset 2 in the MSI capabilities structure
        let control_ptr = unsafe { capability_start_ptr.cast::<MsiControl>().add(1) };
        // SAFETY: The pointer is valid
        let control = unsafe { control_ptr.as_const_ptr().read_volatile() };

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
        unsafe { self.control.as_const_ptr().read_volatile() }
    }

    /// Reads the message address field.
    /// Note that this is _not_ just a physical address - it's a platform-specific format which could contain various flags
    pub fn message_address(&self) -> u64 {
        // SAFETY: It's unsound to create a reference in to a `PcieMappedRegisters`, so no references exist for this data
        let (high, low) = unsafe {
            (
                self.message_address_high
                    .map_or(0, |p| p.as_const_ptr().read_volatile()),
                self.message_address_low.as_const_ptr().read_volatile(),
            )
        };

        (high as u64) << 32 | (low as u64)
    }

    /// Reads the [`data`] register
    ///
    /// [`data`]: MessageSignalledInterruptsCapability::data
    pub fn data(&self) -> u16 {
        // SAFETY: It's unsound to create a reference in to a `PcieMappedRegisters`, so no references exist for this data
        unsafe { self.data.as_const_ptr().read_volatile() }
    }
}

/// An error occurring when trying to write a 64-bit address to a device which doesn't support them
#[derive(Debug, Clone, Copy)]
pub struct Msi64BitWriteTo32BitDeviceError;

impl<'a> MessageSignalledInterruptsCapability<'a, Mutable> {
    /// Writes the [`control`] register
    ///
    /// [`control`]: MessageSignalledInterruptsCapability::control
    pub fn write_control(&mut self, control: MsiControl) {
        // SAFETY: It's unsound to create a reference in to a `PcieMappedRegisters`, so no references exist for this data
        unsafe { self.control.write_volatile(control) }
    }

    /// Writes to the message address register.
    /// Note that this is _not_ just a physical address - it's a platform-specific format which could contain various flags
    pub fn write_message_address(
        &mut self,
        value: u64,
    ) -> Result<(), Msi64BitWriteTo32BitDeviceError> {
        let high = (value >> 32) as u32;
        #[allow(clippy::cast_possible_truncation)] // deliberate truncation
        let low = value as u32;

        // SAFETY: It's unsound to create a reference in to a `PcieMappedRegisters`, so no references exist for this data
        unsafe {
            match self.message_address_high {
                Some(ptr) => ptr.write_volatile(high),
                None => {
                    if high != 0 {
                        return Err(Msi64BitWriteTo32BitDeviceError);
                    }
                }
            }

            self.message_address_low.write_volatile(low);
        }

        Ok(())
    }

    /// Writes to the [`data`] register
    ///
    /// [`data`]: MessageSignalledInterruptsCapability::data
    pub fn write_data(&mut self, data: u16) {
        // SAFETY: It's unsound to create a reference in to a `PcieMappedRegisters`, so no references exist for this data
        unsafe { self.data.write_volatile(data) }
    }

    /// Writes the `message_address` and `data` registers to match the given address for an x64 platform.
    pub fn write_address_x64(&mut self, address: X64MsiAddress) {
        let (address, data) = address.to_address_and_data();
        self.write_message_address(address as _).unwrap();
        self.write_data(data);
    }
}
