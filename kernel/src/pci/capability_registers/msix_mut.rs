//! The [`MsixCapabilityMut`] type for a mutable view into a PCI device's MSI-X capability

use core::{fmt::Debug, marker::PhantomData};

use crate::pci::{PciMappedFunction, PcieMappedRegisters};

use super::{MsixControl, MsixTableEntry};

#[derive(Debug)]
/// A capability of a device to deliver interrupts using MSI-X.
/// This struct contains methods to change the values, such as enabling or disabling MSI-X
///
/// This struct represents a mutable view. If mutability is not needed, use [`MsixCapability`] instead.
///
/// [`MsixCapability`]: super::msix_const::MsixCapability
pub struct MsixCapabilityMut<'a> {
    /// A pointer to the control register
    control: *mut MsixControl,

    /// The BAR number where the table of interrupt vectors is
    bir: u8,
    /// The index into the BAR indicated by [`bir`][MsixCapabilityMut::bir] where the table of interrupts vectors is.
    table_offset: u32,
    /// The BAR number where the _Pending Bit Array_ is stored.
    /// This is a bit-array indicating which of a device's interrupts is currently pending response from the CPU.
    /// If the OS has allocated the same interrupt vector to multiple interrupts on a device, or across devices,
    /// this can be checked to see which interrupt was sent.
    pba_bir: u8,
    /// The index into the BAR indicated by [`pba_bir`][MsixCapabilityMut::pba_bir] where the _Pending Bit Array_ is.
    pba_offset: u32,

    /// Phantom data for borrow checking
    _p: PhantomData<&'a PcieMappedRegisters>,
}

impl<'a> MsixCapabilityMut<'a> {
    /// # Safety:
    /// * `offset` is the register (not byte) offset of an MSI capabilities structure within the configuration space of `function`
    pub(super) unsafe fn new(function: &PciMappedFunction, offset: u8) -> Self {
        // SAFETY: `registers + offset` points to a capabilities structure
        // The pointer is a `*mut u8` so that the `add` method adds 1 byte at a time
        let capability_start_ptr = unsafe {
            function
                .registers
                .as_mut_ptr::<u8>()
                .add(offset as usize * 4)
        };

        // SAFETY: It's unsound to create a reference in to a `PcieMappedRegisters`, so no references exist for this data
        let (control, table_offset, pending_bit_offset) = unsafe {
            (
                capability_start_ptr.add(2).cast(),
                capability_start_ptr.add(4).cast::<u32>(),
                capability_start_ptr.add(8).cast::<u32>(),
            )
        };

        // SAFETY: It's unsound to create a reference in to a `PcieMappedRegisters`, so no references exist for this data
        let (bir, table_offset, pending_bit_bir, pending_bit_offset) = unsafe {
            (
                (table_offset.read_volatile() & 0b111) as u8,
                table_offset.read_volatile() & !0b111,
                (pending_bit_offset.read_volatile() & 0b111) as u8,
                pending_bit_offset.read_volatile() & !0b111,
            )
        };

        Self {
            control,

            bir,
            table_offset,
            pba_bir: pending_bit_bir,
            pba_offset: pending_bit_offset,

            _p: PhantomData,
        }
    }

    /// Reads the capability structure's `control` register
    pub fn control(&self) -> MsixControl {
        // SAFETY: This pointer hasn't been changed since initialisation, so it's valid.
        unsafe { self.control.read_volatile() }
    }

    /// Writes to the capability structure's `control` register
    ///
    /// # Safety
    /// * The caller is responsible for making sure the device's behaviour is sound,
    ///     for instance that handlers are set up for any registered interrupt vectors when enabling MSI-X.
    pub unsafe fn write_control(&mut self, value: MsixControl) {
        // SAFETY: This pointer hasn't been changed since initialisation, so it's valid.
        unsafe { self.control.write_volatile(value) }
    }

    /// Gets the BAR and byte offset where the interrupt table is.
    ///
    /// The BAR may be shared with other data, for which another [`Bar`] struct may exist already. 
    /// It's unsound for two [`Bar`]s to exist for the same BAR at once, 
    /// so the BAR number (not register number) is returned rather than a [`Bar`].
    /// 
    /// [`Bar`]: crate::pci::bar::Bar
    pub fn interrupt_table(&self) -> (u8, u32) {
        (self.bir, self.table_offset)
    }

    /// Gets the BAR and byte offset where the interrupt table is.
    ///
    /// The BAR may be shared with other data, for which another [`Bar`] struct may exist already. 
    /// It's unsound for two [`Bar`]s to exist for the same BAR at once, 
    /// so the BAR number (not register number) is returned rather than a [`Bar`].
    /// 
    /// [`Bar`]: crate::pci::bar::Bar
    pub fn pending_bits(&self) -> (u8, u32) {
        (self.pba_bir, self.pba_offset)
    }
}

/// The MSI-X interrupt table of a PCI device.
///
/// Each entry in this table represents one type of interrupt the device can produce.
pub struct MsixInterruptArrayMut<'a> {
    /// A pointer to the first item in the array
    start: *mut MsixTableEntry,
    /// The index of the last item in the array
    last_index: usize,

    /// PhantomData for the lifetime of the array
    _p: PhantomData<&'a MsixTableEntry>,
}

impl<'a> MsixInterruptArrayMut<'a> {
    /// Constructs a new array
    ///
    /// # Safety
    /// * `start` must be a pointer to the interrupt table in the MMIO space of a PCI device.
    ///    The pointer must be valid for reads and writes for the lifetime `'a`
    /// * `last_index` must be the index of the last entry in the table, i.e. one less than the table's length.
    pub unsafe fn new(start: *mut MsixTableEntry, last_index: usize) -> Self {
        Self {
            start,
            last_index,
            _p: PhantomData,
        }
    }

    /// Reads the value at the given index into the array.
    pub fn read(&self, i: usize) -> Option<MsixTableEntry> {
        if i > self.last_index {
            None
        } else {
            // SAFETY: The index is less than the length of the table, so this read is valid
            unsafe { Some(self.start.add(i).read_volatile()) }
        }
    }

    /// Writes the value at the given index into the array.
    ///
    /// # Panics
    /// * If `i` is past the end of the array. This can be checked using [`len`].
    ///
    /// # Safety
    /// * The caller is responsible for the hardware's response to the write, 
    ///     including making sure there is a handler for the interrupt.
    /// 
    /// [`len`]: MsixInterruptArrayMut::len
    pub unsafe fn write(&mut self, i: usize, value: MsixTableEntry) {
        assert!(i <= self.last_index);

        // SAFETY: The index is less than the length of the table, so the write is valid in terms of borrowing.
        // The caller is responsible for hardware behaviour.
        unsafe {
            self.start.add(i).write_volatile(value);
        }
    }

    /// Gets an iterator over the values of the array.
    pub fn entries(&self) -> impl Iterator<Item = MsixTableEntry> + '_ {
        let mut i = 0;
        core::iter::from_fn(move || {
            let entry = self.read(i);
            i += 1;
            entry
        })
    }

    /// Gets the length of the array.
    pub fn len(&self) -> usize {
        // This will never overflow because it's being treated as a usize, but the underlying data comes from a 16 bit field
        self.last_index + 1
    }
}

impl<'a> Debug for MsixInterruptArrayMut<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut l = f.debug_list();

        l.entries(self.entries());

        l.finish()
    }
}
