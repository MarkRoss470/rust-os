//! The [`DeviceContextRef`] type and its owned version [`OwnedDeviceContext`]

use core::{fmt::Debug, marker::PhantomData};

use x86_64::PhysAddr;

use super::{
    super::operational_registers::SupportedPageSize, endpoint_context::EndpointContext,
    slot_context::SlotContext, ContextSize,
};
use crate::{
    allocator::PageBox,
    util::{
        generic_mutability::{Immutable, Mutability, Mutable, Pointer},
        iterator_list_debug::IteratorListDebug,
    },
};

/// A [device context] in the [`DeviceContextBaseAddressArray`]. This is an _Output Device Context_.
///
/// [device context]: DeviceContextRef
/// [`DeviceContextBaseAddressArray`]: super::super::dcbaa::DeviceContextBaseAddressArray
pub struct OwnedDeviceContext {
    /// The page where the data structure is in memory
    page: PageBox,
    /// The size of a context structure
    context_size: ContextSize,
}

impl Debug for OwnedDeviceContext {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.get().fmt(f)
    }
}

impl OwnedDeviceContext {
    /// Allocates a new device context data structure.
    ///
    /// # Parameters
    /// * `page_size` is the page size supported by the controller, from the controller's operational registers.
    ///    This value can be obtained using the [`read_page_size`] method on the controller's [`OperationalRegisters`].
    /// * `context_size` is the size of context structures.
    ///    This can be obtained using the [`context_size`] method on the controller's [`CapabilityParameters1`]
    ///
    /// [`read_page_size`]: super::super::operational_registers::OperationalRegisters::read_page_size
    /// [`OperationalRegisters`]: super::super::operational_registers::OperationalRegisters
    /// [`context_size`]: super::super::capability_registers::CapabilityParameters1::context_size
    /// [`CapabilityParameters1`]: super::super::capability_registers::CapabilityParameters1
    pub fn new(page_size: SupportedPageSize, context_size: ContextSize) -> Self {
        if page_size.page_size() != 0x1000 {
            todo!("Non-4k pages");
        }

        Self {
            page: PageBox::new(),
            context_size,
        }
    }

    /// Gets the physical address of the start of the page where the data structure is.
    pub fn get_addr(&self) -> PhysAddr {
        self.page.phys_frame().start_address()
    }

    /// Gets a read-only reference to the device context
    pub fn get(&self) -> DeviceContextRef<Immutable> {
        // SAFETY: The pointer is valid for this borrow. `stride` is accurate.
        unsafe { DeviceContextRef::new(self.page.as_ptr(), self.context_size) }
    }

    /// Gets a mutable reference to the device context
    pub fn get_mut(&mut self) -> DeviceContextRef<Mutable> {
        // SAFETY: The pointer is valid for this borrow. `stride` is accurate.
        unsafe { DeviceContextRef::new(self.page.as_mut_ptr(), self.context_size) }
    }
}

/// A reference to a _Device Context_ data structure. Device contexts can be either _Input Device Contexts_ if they are part of an [`InputContext`]
/// or [_Output Device Contexts_] if they are stored in the [`DeviceContextBaseAddressArray`] 
/// 
/// This data structure is defined in the spec section [6.2.1].
///
/// [`InputContext`]: super::input_context::InputContext
/// [_Output Device Contexts_]: OwnedDeviceContext
/// [`DeviceContextBaseAddressArray`]: super::super::dcbaa::DeviceContextBaseAddressArray
/// [6.2.1]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A449%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C361%2C0%5D
pub struct DeviceContextRef<'a, M: Mutability> {
    /// The pointer where the device context is
    ptr: M::Ptr<()>,

    /// See [`OwnedDeviceContext::context_size`]
    context_size: ContextSize,

    /// The lifetime that this reference is valid for
    p: PhantomData<&'a mut EndpointContext>,
}

impl<'a, M: Mutability> DeviceContextRef<'a, M> {
    /// # Safety:
    /// * `ptr` must be valid for reads for the size of a device context data structure
    ///     for the whole lifetime `'a` (if `M` is `Mutable`, it must also be valid for writes).
    /// * `context_size` must be accurate to the controller's [`context_size`] value.
    ///
    /// [`context_size`]: super::super::capability_registers::CapabilityParameters1::context_size
    pub unsafe fn new(ptr: M::Ptr<()>, context_size: ContextSize) -> Self {
        Self {
            ptr,
            context_size,
            p: PhantomData,
        }
    }

    /// Gets the DeviceContext's [`SlotContext`]
    pub fn get_slot_context(&self) -> SlotContext {
        // SAFETY: The first item in the array is the slot context
        unsafe { self.ptr.as_const_ptr().cast::<SlotContext>().read_volatile() }
    }

    /// The number of OUT [`EndpointContext`]s in the Device Context
    fn out_len(&self) -> usize {
        let context_entries: usize = self.get_slot_context().context_entries().into();

        context_entries / 2
    }

    /// The number of IN [`EndpointContext`]s in the Device Context
    fn in_len(&self) -> usize {
        let context_entries: usize = self.get_slot_context().context_entries().into();

        (context_entries.saturating_sub(1)) / 2
    }

    /// Gets the bidirectional first [`EndpointContext`]
    pub fn get_ep_context_0(&self) -> EndpointContext {
        // SAFETY: The first two items in the table are the slot context and the bidirectional endpoint context 0,
        // so this endpoint context is at offset `stride`
        unsafe {
            self.ptr
                .as_const_ptr()
                .cast::<EndpointContext>()
                .byte_add(self.context_size.bytes())
                .read_volatile()
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
            self.ptr
                .as_const_ptr()
                .cast::<EndpointContext>()
                .byte_add(self.context_size.bytes() * 2 * i)
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
            self.ptr
                .as_const_ptr()
                .cast::<EndpointContext>()
                .byte_add(self.context_size.bytes() * (2 * i + 1))
                .read_volatile()
        };

        Some(ec)
    }
}

impl<'a> DeviceContextRef<'a, Mutable> {
    /// Gets the DeviceContext's [`SlotContext`]
    /// 
    /// # Safety
    /// * The OS must be allowed to write to the slot context (TODO: when is this true?)
    /// * The new value must be valid. The caller is responsible for the behaviour of the controller in response to this [`EndpointContext`].
    pub unsafe fn set_slot_context(&mut self, context: SlotContext) {
        // SAFETY: The first item in the array is the slot context
        unsafe { self.ptr.cast::<SlotContext>().write_volatile(context) }
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
            self.ptr
                .cast::<EndpointContext>()
                .byte_add(self.context_size.bytes())
                .write_volatile(context);
        }
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
            self.ptr
                .cast::<EndpointContext>()
                .byte_add(self.context_size.bytes() * (2 * i + 1))
                .write_volatile(context);
        }
    }
}

impl<'a, M: Mutability> Debug for DeviceContextRef<'a, M> {
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
