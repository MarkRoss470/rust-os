//! The [`MessageSignalledInterruptsCapabilityMut`] type for a mutable view into a PCI device's MSI capability

use core::marker::PhantomData;

use crate::pci::{PciMappedFunction, PcieMappedRegisters};

use super::{MsiControl, X64MsiAddress};

/// A mutable view into the MSI capability of a PCI device. If mutability is not needed, use [`MessageSignalledInterruptsCapability`].
/// 
/// [`MessageSignalledInterruptsCapability`]: super::msi_const::MessageSignalledInterruptsCapability
#[derive(Debug)]
pub struct MessageSignalledInterruptsCapabilityMut<'a> {
    /// The memory-mapped control register
    control: *mut MsiControl,
    /// The memory-mapped least significant half of the message address register
    message_address_low: *mut u32,
    /// The memory-mapped most significant half of the message address register
    message_address_high: Option<*mut u32>,

    /// The memory-mapped data register
    data: *mut u16,

    /// PhantomData for the lifetime of the memory-mapped registers
    _p: PhantomData<&'a mut PcieMappedRegisters>,
}

/// An error occurring when trying to write a 64-bit address to a device which doesn't support them
#[derive(Debug, Clone, Copy)]
pub struct Msi64BitWriteTo32BitDeviceError;

impl<'a> MessageSignalledInterruptsCapabilityMut<'a> {
    /// # Safety:
    /// * `offset` is the offset of an MSI capabilities structure within the configuration space of `function`
    pub(super) unsafe fn new(function: &mut PciMappedFunction, offset: u8) -> Self {
        let capability_start_ptr =
        // SAFETY: `offset` is the offset of an MSI capabilities structure
            unsafe { function.registers.as_mut_ptr::<u8>().add(offset as _) };

        // SAFETY: The control register is at offset 2 in the MSI capabilities structure
        let control_ptr = unsafe { capability_start_ptr.add(2).cast::<MsiControl>() };
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
        let message_address_low = unsafe {capability_start_ptr.add(4).cast()};

        // SAFETY: The data register is at offset 8 in the MSI capabilities structure
        let data = unsafe {capability_start_ptr.add(12 + offset_for_64_bit).cast()};
        
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
    /// [`control`]: MessageSignalledInterruptsCapabilityMut::control
    pub fn control(&self) -> MsiControl {
        // SAFETY: It's unsound to create a reference in to a `PcieMappedRegisters`, so no references exist for this data
        unsafe { self.control.read_volatile() }
    }

    /// Writes the [`control`] register
    /// 
    /// [`control`]: MessageSignalledInterruptsCapabilityMut::control
    pub fn write_control(&mut self, control: MsiControl) {
        // SAFETY: It's unsound to create a reference in to a `PcieMappedRegisters`, so no references exist for this data
        unsafe { self.control.write_volatile(control) }
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

    /// Writes to the message address register.
    /// Note that this is _not_ just a physical address - it's a platform-specific format which could contain various flags
    pub fn write_message_address(
        &mut self,
        value: u64,
    ) -> Result<(), Msi64BitWriteTo32BitDeviceError> {
        let high = (value >> 32) as u32;
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

    /// Reads the [`data`] register
    /// 
    /// [`data`]: MessageSignalledInterruptsCapabilityMut::data
    pub fn data(&self) -> u16 {
        // SAFETY: It's unsound to create a reference in to a `PcieMappedRegisters`, so no references exist for this data
        unsafe { self.data.read_volatile() }
    }

    /// Writes to the [`data`] register
    /// 
    /// [`data`]: MessageSignalledInterruptsCapabilityMut::data
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
