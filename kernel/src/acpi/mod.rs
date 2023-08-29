//! Structs to parse the ACPI tables, based on [the ACPI spec].
//! 
//! [the ACPI spec]: https://uefi.org/specs/ACPI/6.5/index.html

use core::fmt::Debug;

use crate::{acpi::rsdp::Rsdp, println};

mod fadt;
mod madt;
mod rsdp;
mod rsdt;

/// Initialises ACPI, using the given RSDP table.
///
/// # Safety
/// This function may only be called once.
/// The given [`PhysAddr`] must point to a valid RSDP structure which describes the system.
/// All of physical memory must be mapped at the virtual address `physical_memory_offset`
pub unsafe fn init(rsdp_addr: u64, physical_memory_offset: u64) {
    println!("Initialising ACPI from RSDP at {rsdp_addr:#x}");

    let rsdp_virtual_addr = physical_memory_offset + rsdp_addr;
    // SAFETY: The pointer pointing to an RSDP table is the caller's responsibility.
    let rsdp = unsafe { Rsdp::read(rsdp_virtual_addr as *const _).unwrap() };
    // SAFETY: All of physical memory is mapped at `physical_memory_offset`.
    let system_table = unsafe { rsdp.get_system_description_table(physical_memory_offset) };

    system_table.list_tables();

    let fadt = system_table.fadt().unwrap();
    let madt = system_table.madt().unwrap();

    for record in madt.records() {
        println!("{record:?}");
    }
}

/// An error occurring while calculating the checksum of an ACPI table.
/// The stored [`u8`] indicates the sum value of the table's bytes, which should have been 0 but wasn't.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChecksumError(u8);

/// The common header fields of various ACPI tables
///
/// https://uefi.org/specs/ACPI/6.5/05_ACPI_Software_Programming_Model.html#system-description-table-header
#[repr(C)]
pub struct SdtHeader {
    /// The table's signature as a 4 byte ASCII string
    signature: [u8; 4],
    /// The length of the table in bytes
    length: u32,
    /// The ACPI version the table is using
    revision: u8,
    /// A checksum byte to make all bytes in the table add to 0 mod 0x100
    checksum: u8,
    /// A 6 byte ASCII string identifying the OEM
    oem_id: [u8; 6],
    /// An 8 byte ASCII string identifying the table provided by the OEM
    /// TODO: check these out more
    oem_table_id: [u8; 8],
    /// The revision of the table supplied by the OEM.
    oem_revision: u32,
    /// Vendor ID of the utility which created the table.
    /// For tables containing Definition Blocks, this is the ID for the ASL Compiler.
    creator_id: u32,
    /// Revision of the utility that created the table.
    /// For tables containing Definition Blocks, this is the revision for the ASL Compiler.
    creator_revision: u32,
}

impl SdtHeader {
    /// The offset of the [`signature`][Self::signature] field from the beginning of the header
    const SIGNATURE_OFFSET: isize = 0;
    /// The offset of the [`length`][Self::length] field from the beginning of the header
    const LENGTH_OFFSET: isize = Self::SIGNATURE_OFFSET + 4;

    /// The offset of the start of the table's data from the start of the header
    const TABLE_START: isize = 36;

    /// Reads the header from the given pointer, and checks the checksum of the whole table.
    ///
    /// # Safety
    /// The pointer must point to the header of an SDT structure.
    #[rustfmt::skip] // Formatting breaks safety comments
    unsafe fn read(ptr: *const Self) -> Result<Self, ChecksumError> {
        // SAFETY: The pointer points to an SDT structure
        unsafe { Self::check(ptr)?; }

        // SAFETY: The pointer is valid for reads, and this read does not exceed the length of SDT header structure
        Ok(unsafe { core::ptr::read(ptr) })
    }

    /// Checks the checksum of the entire table
    ///
    /// # Safety
    /// The pointer must point to the header of an SDT data structure
    unsafe fn check(ptr: *const Self) -> Result<(), ChecksumError> {
        let mut sum: u8 = 0;

        let length: u32 =
        // SAFETY: The pointer is valid for reads, and this read does not exceed the length of SDT header structure
            unsafe { core::ptr::read_unaligned(ptr.byte_offset(Self::LENGTH_OFFSET) as *const _) };

        for i in 0..length as isize {
            // SAFETY: The pointer is valid for reads and this read is less than `length` bytes from the start of the header
            let byte = unsafe { core::ptr::read_unaligned(ptr.byte_offset(i) as *const _) };
            sum = sum.wrapping_add(byte);
        }

        if sum != 0 {
            Err(ChecksumError(sum))
        } else {
            Ok(())
        }
    }
}

impl Debug for SdtHeader {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SdtHeader")
            .field("signature", &core::str::from_utf8(&self.signature).unwrap())
            .field("length", &self.length)
            .field("revision", &self.revision)
            .field("checksum", &self.checksum)
            .field("oem_id", &core::str::from_utf8(&self.oem_id).unwrap())
            .field(
                "oem_table_id",
                &core::str::from_utf8(&self.oem_table_id).unwrap(),
            )
            .field("oem_revision", &self.oem_revision)
            .field("creator_id", &self.creator_id)
            .field("creator_revision", &self.creator_revision)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use core::mem::offset_of;

    use crate::acpi::SdtHeader;

    #[test_case]
    /// Tests that the offsets of [`SdtHeader`] match the spec
    fn test_sdt_header_field_offsets() {
        assert_eq!(offset_of!(SdtHeader, signature), 0);
        assert_eq!(offset_of!(SdtHeader, length), 4);
        assert_eq!(offset_of!(SdtHeader, revision), 8);
        assert_eq!(offset_of!(SdtHeader, checksum), 9);
        assert_eq!(offset_of!(SdtHeader, oem_id), 10);
        assert_eq!(offset_of!(SdtHeader, oem_table_id), 16);
        assert_eq!(offset_of!(SdtHeader, oem_revision), 24);
        assert_eq!(offset_of!(SdtHeader, creator_id), 28);
        assert_eq!(offset_of!(SdtHeader, creator_revision), 32);
    }
}
