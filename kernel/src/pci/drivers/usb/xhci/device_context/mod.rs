//! The [`DeviceContext`] type

pub mod endpoint_context;
pub mod slot_context;

use core::fmt::Debug;

use x86_64::PhysAddr;

use self::{endpoint_context::EndpointContext, slot_context::SlotContext};
use super::operational_registers::SupportedPageSize;
use crate::{allocator::PageBox, util::iterator_list_debug::IteratorListDebug};

/// The XHCI _Device Context_ data structure.
///
/// This data structure is defined in the spec section [6.2.1].
///
/// [6.2.1]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A449%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C361%2C0%5D
pub struct DeviceContext {
    /// The page where the data structure is in memory
    page: PageBox,

    /// The byte offset of each item in the array from the last.
    ///
    /// This is dependant on the [`uses_64_byte_context_structs`] field of the controller's capability registers.
    ///
    /// [`uses_64_byte_context_structs`]: super::capability_registers::CapabilityParameters1::uses_64_byte_context_structs
    stride: usize,
}

impl DeviceContext {
    /// Constructs a new [`DeviceContext`] data structure.
    ///
    /// # Parameters
    /// * `page_size` is the page size supported by the controller, from the controller's operational registers.
    ///    This value can be obtained using the [`read_page_size`] method on the controller's [`OperationalRegisters`].
    /// * `uses_64_byte_context_structures` is whether the controller uses 64 byte context structures as opposed to 32 byte ones.
    ///    This valid can be obtained using the [`uses_64_byte_context_structs`] method on the controller's [`CapabilityParameters1`]
    ///
    /// [`read_page_size`]: super::operational_registers::OperationalRegisters::read_page_size
    /// [`OperationalRegisters`]: super::operational_registers::OperationalRegisters
    /// [`uses_64_byte_context_structs`]: super::capability_registers::CapabilityParameters1::uses_64_byte_context_structs
    /// [`CapabilityParameters1`]: super::capability_registers::CapabilityParameters1
    pub fn new(page_size: SupportedPageSize, uses_64_byte_context_structs: bool) -> Self {
        if page_size.page_size() != 0x1000 {
            todo!("Non-4k pages");
        }

        Self {
            page: PageBox::new(),
            stride: if uses_64_byte_context_structs {
                0x40
            } else {
                0x20
            },
        }
    }

    /// Gets the physical address of the start of the page where the data structure is.
    pub fn get_addr(&self) -> PhysAddr {
        self.page.phys_frame().start_address()
    }

    /// Gets the [`DeviceContext`]'s [`SlotContext`]
    pub fn get_slot_context(&self) -> SlotContext {
        // SAFETY: The first item in the array is the slot context
        unsafe { self.page.as_ptr::<SlotContext>().read() }
    }

    /// The number of OUT [`EndpointContext`]s in the [`DeviceContext`]
    fn out_len(&self) -> usize {
        let context_entries: usize = self.get_slot_context().context_entries().into();

        context_entries / 2
    }

    /// The number of IN [`EndpointContext`]s in the [`DeviceContext`]
    fn in_len(&self) -> usize {
        let context_entries: usize = self.get_slot_context().context_entries().into();

        (context_entries - 1) / 2
    }

    /// Gets the bidirectional first [`EndpointContext`]
    pub fn get_ep_context_0(&self) -> EndpointContext {
        // SAFETY: The first two items in the table are the slot context and the bidirectional endpoint context 0,
        // so this endpoint context is at offset `stride`
        unsafe {
            self.page
                .as_ptr::<EndpointContext>()
                .byte_add(self.stride)
                .read_volatile()
        }
    }

    /// Sets the bidirectional first [`EndpointContext`]
    ///
    /// # Safety
    /// * The OS must be allowed to write to the endpoint context (TODO: when is this true?)
    /// * The new value must be valid. The caller is responsible for the behaviour of the controller in response to this [`EndpointContext`].
    pub unsafe fn set_ep_context_0(&mut self, context: EndpointContext) {
        // SAFETY: The first two items in the table are the slot context and the bidirectional endpoint context 0,
        // so this endpoint context is at offset `stride`.

        // The caller guarantees that the write is allowed and is responsible for the controller's response.
        unsafe {
            self.page
                .as_mut_ptr::<EndpointContext>()
                .byte_add(self.stride)
                .write_volatile(context);
        }
    }

    /// Gets the `i`th OUT [`EndpointContext`].
    pub fn get_ep_context_out(&self, i: usize) -> Option<EndpointContext> {
        assert_ne!(i, 0, "Slot 0 does not have an OUT EP context");

        if i >= self.out_len() {
            return None;
        }

        // SAFETY: The array is laid out alternating OUT and IN contexts
        // so the offset from the beginning is `stride * 2 * i`
        let ec = unsafe {
            self.page
                .as_ptr::<EndpointContext>()
                .byte_add(self.stride * 2 * i)
                .read_volatile()
        };

        Some(ec)
    }

    /// Gets the `i`th IN [`EndpointContext`].
    pub fn get_ep_context_in(&self, i: usize) -> Option<EndpointContext> {
        assert_ne!(i, 0, "Slot 0 does not have an IP EP context");

        if i >= self.in_len() {
            return None;
        }

        // SAFETY: The array is laid out alternating OUT and IN contexts
        // so the offset from the beginning is `stride * (2 * i + 1)`
        let ec = unsafe {
            self.page
                .as_ptr::<EndpointContext>()
                .byte_add(self.stride * (2 * i + 1))
                .read_volatile()
        };

        Some(ec)
    }

    /// Sets the `i`th IN [`EndpointContext`].
    ///
    /// # Safety
    /// * The OS must be allowed to write to the endpoint context (TODO: when is this true?)
    /// * The new value must be valid. The caller is responsible for the behaviour of the controller in response to this [`EndpointContext`].
    pub unsafe fn write_ep_context_in(&mut self, i: usize, context: EndpointContext) {
        assert_ne!(i, 0, "Slot 0 does not have an IP EP context");
        assert!(i < self.in_len(), "Index outside of array");

        // SAFETY: The array is laid out alternating OUT and IN contexts
        // so the offset from the beginning is `stride * (2 * i + 1)`

        // The caller guarantees that the write is allowed and is responsible for the controller's response.
        unsafe {
            self.page
                .as_mut_ptr::<EndpointContext>()
                .byte_add(self.stride * (2 * i + 1))
                .write_volatile(context);
        }
    }
}

impl Debug for DeviceContext {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DeviceContext")
            .field("slot_context", &self.get_slot_context())
            .field("ep_context_0", &self.get_ep_context_0())
            .field("contexts", &{
                let mut i = 1;

                if self.out_len() != self.in_len() {
                    todo!("Debugging DeviceContext with unequal in and out lengths");
                }

                IteratorListDebug::new(core::iter::from_fn(move || {
                    let context_out = self.get_ep_context_out(i)?;
                    let context_in = self.get_ep_context_in(i)?;

                    i += 1;

                    Some((context_out, context_in))
                }))
            })
            .finish()
    }
}
