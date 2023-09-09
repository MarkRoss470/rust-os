//! The [`Madt`] and related types

use core::fmt::Debug;

use x86_64::PhysAddr;

use crate::{println, util::iterator_list_debug::IteratorListDebug};

use super::{ChecksumError, SdtHeader};

/// Flags for the [`ProcessorLocalApic`][MadtRecord::ProcessorLocalApic] record type
#[bitfield(u32)]
pub struct ApicFlags {
    /// Whether the processor is ready for use
    enabled: bool,
    /// Whether the processor can be turned on by the OS, if it is not already on.
    online_capable: bool,

    #[bits(30)]
    _reserved: (),
}

/// Under what condition the interrupt is triggered
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptPolarity {
    /// Conforms to the specification of the bus
    ConformsToBus,
    /// Active high - the interrupt is triggered while the line is on
    ActiveHigh,
    /// Reserved
    Reserved,
    /// Active low - the interrupt is triggered while the line is off
    ActiveLow,
}

impl InterruptPolarity {
    /// Constructs an [`InterruptPolarity`] from its bit representation
    const fn from_bits(bits: u16) -> Self {
        match bits {
            0 => Self::ConformsToBus,
            1 => Self::ActiveHigh,
            2 => Self::Reserved,
            3 => Self::ActiveLow,
            _ => unreachable!(),
        }
    }

    /// Converts an [`InterruptPolarity`] into its bit representation
    const fn into_bits(self) -> u16 {
        match self {
            Self::ConformsToBus => 0,
            Self::ActiveHigh => 1,
            Self::Reserved => 2,
            Self::ActiveLow => 3,
        }
    }
}

/// How often the interrupt is triggered while the condition is met
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptTriggerMode {
    /// Conforms to the specification of the bus
    ConformsToBus,
    /// The interrupt is triggered once when the condition becomes true
    EdgeTriggered,
    /// Reserved
    Reserved,
    /// The interrupt is triggered continuously while the condition is true
    LevelTriggered,
}

impl InterruptTriggerMode {
    /// Constructs an [`InterruptTriggerMode`] from its bit representation
    const fn from_bits(bits: u16) -> Self {
        match bits {
            0 => Self::ConformsToBus,
            1 => Self::EdgeTriggered,
            2 => Self::Reserved,
            3 => Self::LevelTriggered,
            _ => unreachable!(),
        }
    }

    /// Converts an [`InterruptTriggerMode`] into its bit representation
    const fn into_bits(self) -> u16 {
        match self {
            Self::ConformsToBus => 0,
            Self::EdgeTriggered => 1,
            Self::Reserved => 2,
            Self::LevelTriggered => 3,
        }
    }
}

/// TODO: This is called MPS INTI flags in the spec, what does that stand for, rename this struct?
#[bitfield(u16)]
pub struct InterruptVectorFlags {
    #[bits(2)]
    polarity: InterruptPolarity,
    #[bits(2)]
    trigger_mode: InterruptTriggerMode,

    #[bits(12)]
    _reserved: (),
}

/// A record in the [`Madt`]
#[derive(Debug)]
pub enum MadtRecord {
    /// Record declaring the presence of a processor and associated APIC
    ProcessorLocalApic {
        /// An ID which the OS uses to match this record to an object in the DSDT (TODO: link)
        processor_id: u8,
        /// The processor's local APIC ID
        apic_id: u8,
        /// Flags about the processor and APIC
        flags: ApicFlags,
    },
    /// Record declaring the presence of an I/O APIC, which is accessible by all processors and handles I/O interrupts
    /// such as mouse and keyboard events
    IoApic {
        /// The ID of the I/O APIC
        id: u8,
        #[doc(hidden)]
        reserved0: u8,
        /// The physical address of the I/O APIC's registers
        address: u32,
        /// The global system interrupt number where this I/O APIC’s interrupt inputs start.
        /// The number of interrupt inputs is determined by the I/O APIC’s Max Redir Entry register. (TODO: link)
        global_system_interrupt_base: u32,
    },
    /// Record describing a variation from the default in the I/O APIC's IRQ mappings.
    IoApicInterruptSourceOverride {
        /// Should always be 0
        bus_source: u8,
        /// The source IRQ e.g. 0 for the timer interrupt
        irq_source: u8,
        /// What IRQ the interrupt will trigger on the I/O APIC
        global_system_interrupt: u32,
        /// Under what conditions the interrupt is triggered
        flags: InterruptVectorFlags,
    },
    /// Record specifying which I/O interrupt inputs should be enabled as non-maskable.
    /// Non-maskable interrupts are not available for use by devices.
    IoApicNonMaskableInterruptSource {
        /// Under what conditions the interrupt is triggered
        flags: InterruptVectorFlags,
        /// The global system interrupt which this NMI will signal
        global_system_interrupt: u32,
    },
    /// Record describing how non-maskable interrupts are connected to local APICs.
    LocalApicNonMaskableInterrupts {
        /// An ID which the OS uses to match this record to an object in the DSDT (TODO: link)
        processor_id: u8,
        /// Under what conditions the interrupt is triggered
        flags: InterruptVectorFlags,
        /// Local APIC interrupt input (LINTn) to which the NMI is connected
        lint: u8,
    },
    /// An override of the 32-bit [`local_apic_address`][Madt::local_apic_address] field
    /// to extend the address to 64 bits.
    LocalApicAddressOverride {
        #[doc(hidden)]
        reserved0: u16,
        /// The new address
        address: u64,
    },
    /// Similar to [`IoApic`][MadtRecord::IoApic] but for an I/O SAPIC.
    /// This data should be used instead of the data in [`IoApic`][MadtRecord::IoApic] if both records are
    /// present with the same `id`.
    IoSapic {
        /// The ID of the I/O SAPIC
        id: u8,
        #[doc(hidden)]
        reserved0: u8,
        /// The global system interrupt number where this I/O APIC’s interrupt inputs start.
        /// The number of interrupt inputs is determined by the I/O APIC’s Max Redir Entry register. (TODO: link)
        global_system_interrupt_base: u32,
        /// The physical address of the I/O SAPIC's registers
        address: u64,
    },
    /// Similar to [`ProcessorLocalApic`][MadtRecord::ProcessorLocalApic] but for an I/O SAPIC.
    /// This data should be used instead of the data in [`ProcessorLocalApic`][MadtRecord::ProcessorLocalApic] if both records are
    /// present with the same `id`.
    LocalSapic {
        /// The ID of the I/O SAPIC
        id: u8,
        /// The processor's local SAPIC ID
        local_sapic_id: u8,
        /// The processor's local SAPIC EID
        local_sapic_eid: u8,
        #[doc(hidden)]
        reserved0: [u8; 3],
        /// Local SAPIC flags
        flags: ApicFlags,
        /// A value used to match this record to an object in the DSDT (TODO: link)
        acpi_processor_uid_value: u32,
        /// A string used to match this record to an object in the DSDT (TODO: link)
        acpi_processor_uid_string: &'static str,
    },
    /// Record which communicates which I/O SAPIC interrupt inputs are connected to the platform interrupt sources.
    PlatformInterruptSources,
    /// Similar to [`ProcessorLocalApic`][MadtRecord::ProcessorLocalApic] but for an X2APIC.
    ProcessorLocalX2Apic {
        #[doc(hidden)]
        reserved: u16,
        /// The ID of the local X2APIC
        id: u32,
        /// Flags for the X2APIC
        flags: ApicFlags,
        /// A value used to match this record to an object in the DSDT (TODO: link)
        acpi_id: u32,
    },
    /// TODO
    LocalX2ApicNonMaskableInterrupt,
    /// TODO
    GicCpuInterface,
    /// TODO
    GicDistributor,
    /// TODO
    GicMsiFrame,
    /// TODO
    GicRedistributor,
    /// TODO
    GicInterruptTranslationService,
    /// TODO
    MultiprocessorWakeup,
    /// TODO
    CorePic,
    /// TODO
    LegacyIoPic,
    /// TODO
    HyperTransportPic,
    /// TODO
    ExtendIoPic,
    /// TODO
    MsiPic,
    /// TODO
    BridgeIoPic,
    /// TODO
    LowPinCountPic,

    /// Reserved, OEM-specified, or unknown record type
    Reserved,
}

impl MadtRecord {
    /// Reads the record from the given pointer and returns it along with the pointer to the next record.
    ///
    /// # Safety
    /// The pointer must point to a record in an MADT.
    unsafe fn read(ptr: *const Self) -> (Self, *const Self) {
        let mut read_ptr = ptr as *const ();

        /// Convenience function to read from a type-erased pointer and increment it
        fn read_from<T>(ptr: &mut *const ()) -> T {
            // SAFETY: This function is only called within `read` and `read_ptr` always stays within one record
            let value = unsafe { core::ptr::read_unaligned(*ptr as *const _) };
            // SAFETY: The resulting pointer will be in bounds
            *ptr = unsafe { ptr.byte_add(core::mem::size_of::<T>()) };
            value
        }

        // The first byte of a record is always a number indicating the type of record
        let variant: u8 = read_from(&mut read_ptr);
        // The second byte is always the length of the record in bytes
        let length: u8 = read_from(&mut read_ptr);

        let record = match variant {
            0x00 => Self::ProcessorLocalApic {
                processor_id: read_from(&mut read_ptr),
                apic_id: read_from(&mut read_ptr),
                flags: read_from(&mut read_ptr),
            },
            0x01 => Self::IoApic {
                id: read_from(&mut read_ptr),
                reserved0: read_from(&mut read_ptr),
                address: read_from(&mut read_ptr),
                global_system_interrupt_base: read_from(&mut read_ptr),
            },
            0x02 => Self::IoApicInterruptSourceOverride {
                bus_source: read_from(&mut read_ptr),
                irq_source: read_from(&mut read_ptr),
                global_system_interrupt: read_from(&mut read_ptr),
                flags: read_from(&mut read_ptr),
            },
            0x03 => Self::IoApicNonMaskableInterruptSource {
                flags: read_from(&mut read_ptr),
                global_system_interrupt: read_from(&mut read_ptr),
            },
            0x04 => Self::LocalApicNonMaskableInterrupts {
                processor_id: read_from(&mut read_ptr),
                flags: read_from(&mut read_ptr),
                lint: read_from(&mut read_ptr),
            },
            0x05 => Self::LocalApicAddressOverride {
                reserved0: read_from(&mut read_ptr),
                address: read_from(&mut read_ptr),
            },
            0x06 => Self::IoSapic {
                id: read_from(&mut read_ptr),
                reserved0: read_from(&mut read_ptr),
                global_system_interrupt_base: read_from(&mut read_ptr),
                address: read_from(&mut read_ptr),
            },
            0x07 => Self::LocalSapic {
                id: read_from(&mut read_ptr),
                local_sapic_id: read_from(&mut read_ptr),
                local_sapic_eid: read_from(&mut read_ptr),
                reserved0: read_from(&mut read_ptr),
                flags: read_from(&mut read_ptr),
                acpi_processor_uid_value: read_from(&mut read_ptr),
                acpi_processor_uid_string: {
                    // TODO: find a system with SAPICs to test this code path
                    let start_ptr = read_ptr as *const u8;
                    let bytes_until_null =
                        core::iter::from_fn(|| Some(read_from::<u8>(&mut read_ptr)))
                            .position(|b| b == 0)
                            .unwrap();

                    // SAFETY: The string is guaranteed to be null-terminated, so this read will be in-bounds
                    let slice = unsafe { core::slice::from_raw_parts(start_ptr, bytes_until_null) };
                    core::str::from_utf8(slice).unwrap()
                },
            },
            0x08 => Self::PlatformInterruptSources,
            0x09 => Self::ProcessorLocalX2Apic {
                reserved: read_from(&mut read_ptr),
                id: read_from(&mut read_ptr),
                flags: read_from(&mut read_ptr),
                acpi_id: read_from(&mut read_ptr),
            },
            0x0A => Self::LocalX2ApicNonMaskableInterrupt,
            0x0B => Self::GicCpuInterface,
            0x0C => Self::GicDistributor,
            0x0D => Self::GicMsiFrame,
            0x0E => Self::GicRedistributor,
            0x0F => Self::GicInterruptTranslationService,
            0x10 => Self::MultiprocessorWakeup,
            0x11 => Self::CorePic,
            0x12 => Self::LegacyIoPic,
            0x13 => Self::HyperTransportPic,
            0x14 => Self::ExtendIoPic,
            0x15 => Self::MsiPic,
            0x16 => Self::BridgeIoPic,
            0x17 => Self::LowPinCountPic,

            _ => Self::Reserved,
        };

        // SAFETY: There is guaranteed to be another record at this location
        let next_record = unsafe { ptr.byte_offset(length as _) };

        (record, next_record)
    }
}

#[bitfield(u32)]
pub struct MadtFlags {
    /// Whether also has a PC-AT-compatible dual-8259 setup.
    /// The 8259 vectors must be disabled (that is, masked) when enabling the ACPI APIC operation.
    pcat_compatible: bool,

    #[bits(31)]
    _reserved: (),
}

/// The MADT data structure.
///
/// For more info see the spec section [5.2.12]
///
/// [5.2.12]: https://uefi.org/specs/ACPI/6.5/05_ACPI_Software_Programming_Model.html#multiple-apic-description-table-madt
pub struct Madt {
    /// The table header
    header: SdtHeader,

    /// The physical address of the local APIC. This value may be overridden by a
    /// [`LocalApicAddressOverride`][MadtRecord::LocalApicAddressOverride] record.
    local_apic_address: u32,
    /// flags related to the MADT
    flags: MadtFlags,

    /// A pointer to the first [`MadtRecord`] in the table
    records_start: *const MadtRecord,
    /// Points to one byte after the end of the table to indicate when
    /// [`records`][Self::records] should stop looping.
    records_end: *const MadtRecord,
}

// SAFETY: Currently there is no multithreading
// TODO: when implementing multiprocessing, review this code
unsafe impl Send for Madt {}

impl Madt {
    /// Reads the MADT from the given pointer
    pub unsafe fn read(ptr: *const Self) -> Result<Self, ChecksumError> {
        println!("Reading from ptr {ptr:p}");

        // SAFETY: This only reads within the table
        let header = unsafe { SdtHeader::read(ptr as *const _)? };

        let local_apic_address =
            // SAFETY: This read is within the table
            unsafe { core::ptr::read(ptr.byte_offset(SdtHeader::TABLE_START) as *const _) };

        let flags =
            // SAFETY: This read is within the table
            unsafe { core::ptr::read(ptr.byte_offset(SdtHeader::TABLE_START + 4) as *const _) };

        // SAFETY: This pointer points to the first record in the table
        let records_start = unsafe { ptr.byte_offset(SdtHeader::TABLE_START + 8) as *const _ };
        // SAFETY: This pointer is never dereferenced and points to the byte after the end of the table
        let records_end = unsafe { ptr.byte_offset(header.length as _) as *const _ };
        Ok(Self {
            header,
            local_apic_address,
            flags,
            records_start,
            records_end,
        })
    }

    /// Gets an iterator over the records in the table
    pub fn records(&self) -> impl Iterator<Item = MadtRecord> {
        // This struct is unsound to construct except from a valid MADT, so there has to be a record here.
        let mut current_record_ptr = self.records_start;
        let records_end = self.records_end;

        core::iter::from_fn(move || {
            if current_record_ptr < records_end {
                // SAFETY: The pointer was calculated from the previous record so is valid
                let (record, next_record_ptr) = unsafe { MadtRecord::read(current_record_ptr) };
                current_record_ptr = next_record_ptr;
                Some(record)
            } else {
                None
            }
        })
    }

    /// Gets the physical address of the local APIC register space
    pub fn local_apic_address(&self) -> PhysAddr {
        let address = self
            .records()
            .find_map(|record| {
                if let MadtRecord::LocalApicAddressOverride { address, .. } = record {
                    Some(address)
                } else {
                    None
                }
            })
            .unwrap_or(self.local_apic_address as u64);

        PhysAddr::new(address)
    }

    /// Gets the physical address of the I/O APIC
    /// 
    /// # Panics
    /// If the system has multiple APICs
    /// TODO: implement this
    pub fn io_apic_address(&self) -> PhysAddr {
        let mut records = self.records();

        let address = records
            .find_map(|record| {
                if let MadtRecord::IoApic { address, .. } = record {
                    Some(address)
                } else {
                    None
                }
            })
            .unwrap();

        // If there are more IO APICs after the first one
        if records
            .any(|record| matches!(&record, MadtRecord::IoApic { .. }))
        {
            todo!("Multiple I/O APICs");
        }

        PhysAddr::new(address as u64)
    }

    /// Gets the [`header`][Self::header] field
    pub fn header(&self) -> &SdtHeader {
        &self.header
    }
    /// Gets the [`flags`][Self::flags] field
    pub fn flags(&self) -> MadtFlags {
        self.flags
    }
}

impl Debug for Madt {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Madt")
            .field("header", &self.header)
            .field(
                "local_apic_address",
                &format_args!("{:#x}", self.local_apic_address),
            )
            .field("flags", &self.flags)
            .field(
                "records",
                &IteratorListDebug::new_with_default_formatting(self.records()),
            )
            .finish()
    }
}
