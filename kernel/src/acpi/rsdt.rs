//! Types to parse the RSDT and XSDT data structures

use core::fmt::Debug;

use crate::{
    println,
    util::{byte_align_ints::{ByteAlignU32, ByteAlignU64}, iterator_list_debug::IteratorListDebug},
};

use super::{fadt::Fadt, ChecksumError, SdtHeader, madt::Madt};

/// https://uefi.org/specs/ACPI/6.5/05_ACPI_Software_Programming_Model.html#root-system-description-table-rsdt
#[derive(Debug)]
pub struct Rsdt {
    /// The header of the table
    header: SdtHeader,
    /// The pointers to other tables.
    /// This is stored as 4 bytes rather than a u32 because the entries may not be 4 byte aligned.
    entries: &'static [ByteAlignU32],
    /// The virtual address where all of physical memory is mapped, used for converting physical addresses to virtual ones
    physical_memory_offset: u64,
}

/// Getters for struct fields
#[rustfmt::skip]
impl Rsdt {
    /// Gets the [`header`][Self::header] field
    pub fn header(&self) -> &SdtHeader { &self.header }
    /// Gets the [`entries`][Self::entries] field
    pub fn entries(&self) -> &'static [ByteAlignU32] { self.entries }
    /// Gets the [`physical_memory_offset`][Self::physical_memory_offset] field
    pub fn physical_memory_offset(&self) -> u64 { self.physical_memory_offset }
}

impl Rsdt {
    /// Reads the RSDT from the given virtual pointer.
    ///
    /// # Safety
    /// The pointer must point to a valid RSDT structure
    pub unsafe fn read(
        ptr: *const Self,
        physical_memory_offset: u64,
    ) -> Result<Self, ChecksumError> {
        // SAFETY: The pointer is valid
        let header = unsafe { SdtHeader::read(ptr as *const _)? };
        let entries_length = (header
            .length
            .checked_sub(SdtHeader::TABLE_START as u32)
            .unwrap())
            / 4;

        let entries =
            // SAFETY: All these bytes are part of the table because they are less than `length` bytes from the start
            unsafe { core::slice::from_raw_parts(ptr.byte_offset(SdtHeader::TABLE_START) as *const _, entries_length as _) };

        Ok(Self {
            header,
            entries,
            physical_memory_offset,
        })
    }

    /// Gets a pointer to the ACPI table with the given signature
    pub fn get_table(&self, signature: &[u8; 4]) -> Result<*const (), TableLookupError> {
        for table_addr in self.entries {
            let table_virtual_addr = self.physical_memory_offset + table_addr.to_u32() as u64;
            let signature_addr = table_virtual_addr + SdtHeader::SIGNATURE_OFFSET as u64;

            // SAFETY: the read is from within a table
            let table_signature: [u8; 4] = unsafe { core::ptr::read_unaligned(signature_addr as *const _) };
            
            // If the signatures don't match, move on
            if *signature != table_signature {
                continue;
            }

            // Check the checksum of the table
            // SAFETY: The pointer is valid
            unsafe { SdtHeader::read(table_virtual_addr as *const _)? };

            return Ok(table_virtual_addr as *const ());
        }

        Err(TableLookupError::NotFound)
    }

    /// Prints out all the table signatures in a list
    pub fn list_tables(&self) {
        println!("Table length: {}", self.entries.len());

        let signatures = self
            .entries
            .iter()
            .map(|table_addr| {
                self.physical_memory_offset + table_addr.to_u32() as u64
            })
            .map(|table_virtual_addr| table_virtual_addr + SdtHeader::SIGNATURE_OFFSET as u64)
            // SAFETY: This pointer points to the signature of a ACPI table
            .map(|signature_addr| unsafe { &*(signature_addr as *const [u8; 4]) })
            .map(|signature| core::str::from_utf8(signature).unwrap());

        println!("Tables: {:?}", IteratorListDebug::new(signatures));
    }
}

/// https://uefi.org/specs/ACPI/6.5/05_ACPI_Software_Programming_Model.html#root-system-description-table-rsdt
#[derive(Debug)]
pub struct Xsdt {
    /// The header of the table
    header: SdtHeader,
    /// The pointers to other tables.
    /// This is stored as 8 bytes rather than a u32 because the entries may not be 8 byte aligned.
    entries: &'static [ByteAlignU64],
    /// The virtual address where all of physical memory is mapped, used for converting physical addresses to virtual ones
    physical_memory_offset: u64,
}

/// Getters for struct fields
#[rustfmt::skip]
impl Xsdt {
    /// Gets the [`header`][Self::header] field
    pub fn header(&self) -> &SdtHeader { &self.header }
}

impl Xsdt {
    /// Reads the XSDT from the given virtual pointer.
    ///
    /// # Safety
    /// The pointer must point to a valid XSDT structure
    pub unsafe fn read(
        ptr: *const Self,
        physical_memory_offset: u64,
    ) -> Result<Self, ChecksumError> {
        // SAFETY: The pointer is valid
        let header = unsafe { SdtHeader::read(ptr as *const _)? };
        let entries_length = (header.length - SdtHeader::TABLE_START as u32) / 8;

        let entries =
            // SAFETY: All these bytes are part of the table because they are less than `length` bytes from the start
            unsafe { core::slice::from_raw_parts(ptr.byte_offset(SdtHeader::TABLE_START) as *const _, entries_length as _) };

        Ok(Self {
            header,
            entries,
            physical_memory_offset,
        })
    }

    /// Gets a pointer to the ACPI table with the given signature
    pub fn get_table(&self, signature: &[u8; 4]) -> Result<*const (), TableLookupError> {
        for table_addr in self.entries {
            let table_virtual_addr = self.physical_memory_offset + table_addr.to_u64();
            let signature_addr = table_virtual_addr + SdtHeader::SIGNATURE_OFFSET as u64;

            // SAFETY: the read is from within a table
            let table_signature: [u8; 4] = unsafe { core::ptr::read_unaligned(signature_addr as *const _) };
            
            // If the signatures don't match, move on
            if *signature != table_signature {
                continue;
            }

            // Check the checksum of the table
            // SAFETY: The pointer is valid
            unsafe { SdtHeader::read(table_virtual_addr as *const _)? };

            return Ok(table_virtual_addr as *const ());
        }

        Err(TableLookupError::NotFound)
    }

    /// Prints out all the table signatures in a list
    pub fn list_tables(&self) {
        let signatures = self
            .entries
            .iter()
            .map(|table_addr| {
                self.physical_memory_offset + table_addr.to_u64()
            })
            .map(|table_virtual_addr| table_virtual_addr + SdtHeader::SIGNATURE_OFFSET as u64)
            // SAFETY: This pointer points to the signature of a ACPI table
            .map(|signature_addr| unsafe { &*(signature_addr as *const [u8; 4]) })
            .map(|signature| core::str::from_utf8(signature).unwrap());

        println!("{:?}", IteratorListDebug::new(signatures));
    }
}

/// An ACPI table which contains pointers to other tables.
/// Contains either an [`Rsdt`] or an [`Xsdt`].
#[derive(Debug)]
pub enum SystemDescriptionTableVariant {
    /// An RSDT
    Rsdt(Rsdt),
    /// An XSDT
    Xsdt(Xsdt),
}

#[derive(Debug)]
pub struct SystemDescriptionTable {
    table: SystemDescriptionTableVariant,
}

/// An error which occurs when looking up a table from a [`SystemDescriptionTable`]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableLookupError {
    /// The table was not found
    NotFound,
    /// The table was found but had an invalid checksum
    ChecksumError(ChecksumError),
}

impl From<ChecksumError> for TableLookupError {
    fn from(value: ChecksumError) -> Self {
        Self::ChecksumError(value)
    }
}

impl SystemDescriptionTable {
    /// Constructs a wrapper around the given RSDT
    ///
    /// # Safety
    /// The RSDT must be valid. The crucial requirement is that all tables have the correct signatures.
    /// For instance, a table with signature "FACP" must be an [`Fadt`].
    pub unsafe fn from_rsdt(rsdt: Rsdt) -> Self {
        Self {
            table: SystemDescriptionTableVariant::Rsdt(rsdt),
        }
    }

    /// Constructs a wrapper around the given XSDT
    ///
    /// # Safety
    /// The XSDT must be valid. The crucial requirement is that all tables have the correct signatures.
    /// For instance, a table with signature "FACP" must be an [`Fadt`].
    pub unsafe fn from_xsdt(xsdt: Xsdt) -> Self {
        Self {
            table: SystemDescriptionTableVariant::Xsdt(xsdt),
        }
    }

    /// Gets a pointer to the ACPI table with the given signature
    pub fn get_table(&self, signature: &[u8; 4]) -> Result<*const (), TableLookupError> {
        match &self.table {
            SystemDescriptionTableVariant::Rsdt(rsdt) => rsdt.get_table(signature),
            SystemDescriptionTableVariant::Xsdt(xsdt) => xsdt.get_table(signature),
        }
    }

    /// Prints out all the table signatures in a list
    pub fn list_tables(&self) {
        match &self.table {
            SystemDescriptionTableVariant::Rsdt(rsdt) => rsdt.list_tables(),
            SystemDescriptionTableVariant::Xsdt(xsdt) => xsdt.list_tables(),
        }
    }

    /// Gets the [`Fadt`] table
    pub fn fadt(&self) -> Result<Fadt, TableLookupError> {
        let fadt_ptr = self.get_table(b"FACP")?;
        // SAFETY: A table with header "FACP" is guaranteed to be an FADT
        let table = unsafe { Fadt::read(fadt_ptr as *const _)? };

        Ok(table)
    }

    /// Gets the [`Madt`] table
    pub fn madt(&self) -> Result<Madt, TableLookupError> {
        let madt_ptr = self.get_table(b"APIC")?;
        // SAFETY: A table with header "APIC" is guaranteed to be an MADT
        let table = unsafe { Madt::read(madt_ptr as *const _)? };

        Ok(table)
    }
}
