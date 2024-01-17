//! Contains the [`RuntimeRegisters`] struct and the types it depends on

use core::fmt::Debug;

use x86_64::VirtAddr;

use super::interrupter::InterrupterRegisterSet;

/// The runtime registers of an XHCI controller
pub struct RuntimeRegisters(*mut ());

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

    /// Gets the value of the _Microframe Index Register_.
    /// This register is updated every microframe (125 microseconds), while [`enabled`] is `true`.
    ///
    /// [`enabled`]: super::operational_registers::UsbCommand::enabled
    #[allow(clippy::cast_possible_truncation)]
    pub fn microframe_index(&self) -> u16 {
        // SAFETY: The first 32 bit register in runtime registers is the MFINDEX register
        let reg = unsafe { self.0.cast::<u32>().read_volatile() };

        // Lowest 14 bits are microframe register, other bits are reserved
        reg as u16 & 0b11_1111_1111_1111
    }

    /// Gets the [`InterrupterRegisterSet`] at the given index
    ///
    /// # Safety
    /// * Only one [`InterrupterRegisterSet`] may exist at once per interrupter,
    ///     so this method may not be called if an existing instance exists for the given `i`.
    pub unsafe fn interrupter(&mut self, i: usize) -> InterrupterRegisterSet {
        // SAFETY: Interrupter registers start at offset 0x20 and are 32 bytes each, so this calculates the address of the `i`th interrupter.
        // No other `InterrupterRegisterSet` exists for this `i`.
        unsafe { InterrupterRegisterSet::new(VirtAddr::from_ptr(self.0) + 0x20usize + 32 * i) }
    }
}

impl Debug for RuntimeRegisters {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RuntimeRegisters").finish()
    }
}
