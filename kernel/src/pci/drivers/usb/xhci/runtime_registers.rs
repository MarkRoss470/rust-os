//! Contains the [`RuntimeRegisters`] struct and the types it depends on

use core::fmt::Debug;

use x86_64::VirtAddr;

/// The runtime registers of an XHCI controller.
/// 
/// See the spec section [5.5] for more info.
/// 
/// [5.5]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A429%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C193%2C0%5D
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RuntimeRegistersFields {
    
}

/// Wrapper struct around [`RuntimeRegistersFields`] to ensure all reads and writes are volatile
pub struct RuntimeRegisters(*mut RuntimeRegisters);

impl RuntimeRegisters {
    /// Wraps the given pointer.
    ///
    /// # Safety
    /// The given pointer must point to the runtime registers struct of an xHCI controller
    pub unsafe fn new(ptr: VirtAddr) -> Self {
        // SAFETY: `ptr` is valid
        let ptr = ptr.as_mut_ptr();

        Self(ptr)
    }
}

impl Debug for RuntimeRegisters {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RuntimeRegisters").finish()
    }
}
