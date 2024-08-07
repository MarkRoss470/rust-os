//! The [`DeviceContextBaseAddressArray`] struct for associating xHCI Device Slots
//! with their respective [`OwnedDeviceContext`] data structures.

use core::fmt::Debug;

use alloc::boxed::Box;
use x86_64::PhysAddr;

use crate::allocator::PageBox;

use super::super::{
    contexts::{device_context::OwnedDeviceContext, ContextSize},
    registers::{
        capability::CapabilityRegisters,
        operational::{OperationalRegisters, SupportedPageSize},
    },
};
use super::scratchpad::ScratchpadBufferArray;

/// The _Device Context Base Address Array_ (DCBAA) data structure is used to
/// associate an xHCI _Device Slot_ with its respective [`OwnedDeviceContext`] data structure.
/// The DCBAA entry associated with each allocated _Device Slot_
/// contains a 64-bit pointer to the base of the associated DeviceContext.
///
/// The DCBAA is 64-byte aligned and may not span page boundaries.
///
/// The Device Context Base Address Array data structure is also used to reference
/// the [`ScratchpadBufferArray`] data structure.
/// See the spec section [4.20] for more information on Scratchpad Buffer allocation.
///
/// [4.20]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A341%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C694%2C0%5D
pub struct DeviceContextBaseAddressArray {
    /// The page where the DCBAA is stored in memory
    page: PageBox,
    /// The length of the array
    len: usize,
    /// The scratchpad buffer array
    scratchpad_buffer_array: ScratchpadBufferArray,
    /// The device contexts pointed to by the DCBAA
    contexts: Box<[OwnedDeviceContext]>,
}

impl DeviceContextBaseAddressArray {
    /// Allocates a new DCBAA using the values in the given registers.
    ///
    /// # Safety
    /// * The given `capability` and `operational` registers must be valid registers from the same controller.
    pub unsafe fn from_registers(
        capability: &CapabilityRegisters,
        operational: &OperationalRegisters,
    ) -> DeviceContextBaseAddressArray {
        let len = capability
            .structural_parameters_1()
            .max_device_slots()
            .into();

        let page_size = operational.read_page_size();

        let context_size = capability.capability_parameters_1().context_size();

        let max_scratchpad_buffers = capability
            .structural_parameters_2()
            .max_scratchpad_buffers()
            .into();

        // SAFETY: `len`, `page_size`, `context_size`, and `max_scratchpad_buffers` are valid
        unsafe {
            DeviceContextBaseAddressArray::new(len, page_size, context_size, max_scratchpad_buffers)
        }
    }

    /// Allocates a new DCBAA with the given length
    ///
    /// # Safety
    /// * `page_size` must be the value of [the controller's `page_size` register]
    /// * `context_size` must be the value of the controller's [`context_size`] register
    /// * `max_scratchpad_buffers` must be the value of the controller's [`max_scratchpad_buffers`] register
    ///
    /// [the controller's `page_size` register]: super::operational::OperationalRegisters::read_page_size
    /// [`context_size`]: super::capability::CapabilityParameters1::context_size
    /// [`max_scratchpad_buffers`]: super::capability::StructuralParameters2::max_scratchpad_buffers
    pub unsafe fn new(
        len: usize,
        page_size: SupportedPageSize,
        context_size: ContextSize,
        max_scratchpad_buffers: usize,
    ) -> Self {
        assert!(len <= 256);

        // SAFETY: `page_size` is the controller's page size
        let scratchpad_buffer =
            unsafe { ScratchpadBufferArray::new(max_scratchpad_buffers, page_size) };

        let mut s = Self {
            page: PageBox::new(),
            len,
            scratchpad_buffer_array: scratchpad_buffer,
            contexts: core::iter::repeat(())
                .take(len)
                .map(|_| OwnedDeviceContext::new(page_size, context_size))
                .collect(),
        };

        // SAFETY: The passed `address` is the address of the scratchpad buffer array
        // `page_size` is valid
        unsafe {
            s.write_scratchpad_buffer_array(s.scratchpad_buffer_array.get_array_addr(), page_size);
        }

        for i in 0..s.contexts.len() {
            let addr = s.contexts[i].get_addr();

            // SAFETY: `addr` is the address of a device context
            unsafe {
                s.set_slot_addr(i, addr);
            }
        }

        s
    }

    /// Gets the address of the DCBAA
    pub fn array_addr(&self) -> PhysAddr {
        self.page.phys_frame().start_address()
    }

    /// Reads the physical address of the scratchpad buffer
    fn scratchpad_buffer_array(&self) -> PhysAddr {
        // SAFETY: The first entry in the array is the scratchpad array
        let v = unsafe { self.page.as_ptr::<u64>().read_volatile() };

        // The bottom 5 bits are reserved, so mask.
        PhysAddr::new(v & !0b11111)
    }

    /// Writes to the scratchpad register
    ///
    /// # Safety
    /// * `address` must be the physical address of a [`ScratchpadBufferArray`] data structure allocated for this controller.
    /// * `page_size` must be the value of [the controller's `page_size` register]
    ///
    /// # Panics
    /// * If `address` isn't `page_size` aligned
    ///
    /// [the controller's `page_size` register]: super::operational::OperationalRegisters::read_page_size
    pub unsafe fn write_scratchpad_buffer_array(
        &mut self,
        address: PhysAddr,
        page_size: SupportedPageSize,
    ) {
        assert!(
            address.is_aligned(page_size.page_size()),
            "Address must be page_size aligned"
        );

        // SAFETY: The first entry in the array is the scratchpad array.
        // The caller is responsible for ensuring that the address is valid.
        unsafe {
            self.page
                .as_mut_ptr::<u64>()
                .write_volatile(address.as_u64());
        }
    }

    /// Gets the address for the given slot.
    fn get_slot_addr(&self, i: usize) -> Option<PhysAddr> {
        if i >= self.len {
            return None;
        }

        // SAFETY: i < len, so i is within the bounds of the array.
        let v = unsafe {
            self.page
                .as_ptr::<u64>()
                .add(1 + i) // First element in array is scratchpad pointer, so add 1.
                .read_volatile()
        };

        Some(PhysAddr::new(v & !0b1111))
    }

    /// Sets the address for the given slot
    ///
    /// # Safety
    /// * `address` must be the address of a Device Context data structure
    ///
    /// # Panics
    /// * If the index is outside the range of the table, i.e. `i >= len`
    unsafe fn set_slot_addr(&mut self, i: usize, address: PhysAddr) {
        assert!(address.is_aligned(64u64), "Address must be 64-byte aligned");

        assert!(i < self.len, "Index outside of table");

        // SAFETY: The first entry in the array is the scratchpad array.
        // The caller is responsible for ensuring that the address is valid.
        unsafe {
            self.page
                .as_mut_ptr::<u64>()
                .add(1 + i)
                .write_volatile(address.as_u64());
        }
    }

    /// Gets the contained Device Contexts as a slice
    pub fn contexts(&self) -> &[OwnedDeviceContext] {
        &self.contexts
    }

    /// Gets the contained Device Contexts as a mutable slice
    pub fn contexts_mut(&mut self) -> &mut [OwnedDeviceContext] {
        &mut self.contexts
    }
}

impl Debug for DeviceContextBaseAddressArray {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DeviceContextBaseAddressArray")
            .field("scratchpad_buffer_array", &self.scratchpad_buffer_array())
            .field("addresses", &self.contexts)
            .finish()
    }
}
