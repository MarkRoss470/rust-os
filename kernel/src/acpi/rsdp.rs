//! Struct to parse the RSDP data structure

use core::fmt::Debug;

use crate::acpi::rsdt::{Rsdt, Xsdt};

use super::{rsdt::SystemDescriptionTable, ChecksumError};

/// The extra fields only present on ACPI 2.0+ systems
pub struct XsdpFields {
    /// The length of the table in bytes
    length: u32,
    /// The physical address of the XSDT (TODO: link) table.
    xsdt_address: u64,
    /// The checksum for the extended fields
    extended_checksum: u8,
    /// Reserved bytes
    reserved: [u8; 3],
}

/// Getters for struct fields
#[rustfmt::skip]
impl XsdpFields {
    /// Gets the [`length`][Self::length] field
    pub fn length(&self) -> &u32 { &self.length }
    /// Gets the [`xsdt_address`][Self::xsdt_address] field
    pub fn xsdt_address(&self) -> &u64 { &self.xsdt_address }
    /// Gets the [`extended_checksum`][Self::extended_checksum] field
    pub fn extended_checksum(&self) -> &u8 { &self.extended_checksum }
    /// Gets the [`reserved`][Self::reserved] field
    pub fn reserved(&self) -> &[u8; 3] { &self.reserved }
}

/// The RSDP data structure given to the OS by the BIOS or UEFI.
/// This contains a pointer to the RSDT or XSDT (TODO: links) tables.
pub struct Rsdp {
    /// The ASCII byte sequence "RSD PTR ", which identifies this table.
    signature: [u8; 8],
    /// A checksum byte which the OS can use to check that the table is valid,
    /// as all bytes in the structure add to 0 mod 0x100
    checksum: u8,
    /// An ASCII string identifying the OEM
    oem_id: [u8; 6],
    /// The revision of ACPI the tables use.
    /// 0 = 1.0, meaning XSDP is not present.
    /// Any other value means the XSDP is present.
    revision: u8,
    /// The 32 bit physical address of the RSDT (TODO: link) data structure
    rsdt_address: u32,

    /// Fields which are only present if [`revision`][Self::revision] > 0
    xsdp_fields: Option<XsdpFields>,
}

/// Getters for struct fields
#[rustfmt::skip]
impl Rsdp {
    /// Gets the [`signature`][Self::signature] field
    pub fn signature(&self) -> &[u8; 8] { &self.signature }
    /// Gets the [`checksum`][Self::checksum] field
    pub fn checksum(&self) -> u8 { self.checksum }
    /// Gets the [`oem_id`][Self::oem_id] field
    pub fn oem_id(&self) -> &[u8; 6] { &self.oem_id }
    /// Gets the [`revision`][Self::revision] field
    pub fn revision(&self) -> u8 { self.revision }
    /// Gets the [`rsdt_address`][Self::rsdt_address] field
    pub fn rsdt_address(&self) -> u32 { self.rsdt_address }
    /// Gets the [`xsdp_fields`][Self::xsdp_fields] field
    pub fn xsdp_fields(&self) -> Option<&XsdpFields> { self.xsdp_fields.as_ref() }
}

impl Rsdp {
    /// The offset of the [`signature`][Self::signature] field from the start of the struct
    const SIGNATURE_OFFSET: isize = 0;
    /// The offset of the [`checksum`][Self::checksum] field from the start of the struct
    const CHECKSUM_OFFSET: isize = Self::SIGNATURE_OFFSET + 8;
    /// The offset of the [`oem_id`][Self::oem_id] field from the start of the struct
    const OEM_ID_OFFSET: isize = Self::CHECKSUM_OFFSET + 1;
    /// The offset of the [`revision`][Self::revision] field from the start of the struct
    const REVISION_OFFSET: isize = Self::OEM_ID_OFFSET + 6;
    /// The offset of the [`rsdt_address`][Self::rsdt_address] field from the start of the struct
    const RSDT_ADDRESS_OFFSET: isize = Self::REVISION_OFFSET + 1;

    /// The offset of the [`length`][Self::length] field from the start of the struct
    const LENGTH_OFFSET: isize = Self::RSDT_ADDRESS_OFFSET + 4;
    /// The offset of the [`xsdt_address`][Self::xsdt_address] field from the start of the struct
    const XSDT_ADDRESS_OFFSET: isize = Self::LENGTH_OFFSET + 4;
    /// The offset of the [`extended_checksum`][Self::extended_checksum] field from the start of the struct
    const EXTENDED_CHECKSUM_OFFSET: isize = Self::XSDT_ADDRESS_OFFSET + 8;
    /// The offset of the [`reserved`][Self::reserved] field from the start of the struct
    const RESERVED_OFFSET: isize = Self::EXTENDED_CHECKSUM_OFFSET + 1;

    /// Reads an [`Rsdp`] from the given pointer.
    ///
    /// # Safety
    /// The pointer must be valid for reading the RSDP data.
    /// This includes all the other tables pointed to by the RSDP table being valid.
    #[rustfmt::skip] // Formatting breaks safety comments
    pub unsafe fn read(ptr: *const Self) -> Result<Self, ChecksumError> {
        // SAFETY: The pointer points to an RSDP struct and so it is valid for reads
        unsafe { Self::check(ptr)? };

        // SAFETY: The pointer is valid for reads, and this read does not exceed the length of the RSDP structure
        let signature: [u8; 8] = unsafe { core::ptr::read_unaligned(ptr.byte_offset(Self::SIGNATURE_OFFSET) as *const _) };
        // SAFETY: Same as above
        let checksum: u8 = unsafe { core::ptr::read_unaligned(ptr.byte_offset(Self::CHECKSUM_OFFSET) as *const _) };
        // SAFETY: Same as above
        let oem_id: [u8; 6] = unsafe { core::ptr::read_unaligned(ptr.byte_offset(Self::OEM_ID_OFFSET) as *const _) };
        // SAFETY: Same as above
        let revision: u8 = unsafe { core::ptr::read_unaligned(ptr.byte_offset(Self::REVISION_OFFSET) as *const _) };
        // SAFETY: Same as above
        let rsdt_address: u32 = unsafe { core::ptr::read_unaligned(ptr.byte_offset(Self::RSDT_ADDRESS_OFFSET) as *const _) };

        let xsdp = if revision == 0 { None } else {
            // SAFETY: Same as above
            let length: u32 = unsafe { core::ptr::read_unaligned(ptr.byte_offset(Self::LENGTH_OFFSET) as *const _) };

            // Check that the table is long enough to hold all the fields
            assert!(length as isize >= Self::RESERVED_OFFSET + 3);

            // SAFETY: Same as above
            let xsdt_address: u64 = unsafe { core::ptr::read_unaligned(ptr.byte_offset(Self::XSDT_ADDRESS_OFFSET) as *const _) };
            // SAFETY: Same as above
            let extended_checksum: u8 = unsafe { core::ptr::read_unaligned(ptr.byte_offset(Self::EXTENDED_CHECKSUM_OFFSET) as *const _) };
            // SAFETY: Same as above
            let reserved: [u8; 3] = unsafe { core::ptr::read_unaligned(ptr.byte_offset(Self::RESERVED_OFFSET) as *const _) };

            Some(XsdpFields {
                length,
                xsdt_address,
                extended_checksum,
                reserved,
            })
        };

        Ok(Self {
            signature,
            checksum,
            oem_id,
            revision,
            rsdt_address,
            xsdp_fields: xsdp,
        })
    }

    /// Checks that the checksum is valid
    ///
    /// # Safety
    /// The given pointer must be valid for reads up to
    /// [`length`][XsdpFields::length] bytes or 20 bytes for version 1.0 tables
    pub unsafe fn check(ptr: *const Self) -> Result<(), ChecksumError> {
        let mut sum: u8 = 0;

        for i in 0..Self::LENGTH_OFFSET {
            // SAFETY: The pointer is valid for reads for 20 bytes
            let byte = unsafe { core::ptr::read_unaligned(ptr.byte_offset(i) as *const u8) };
            sum = sum.wrapping_add(byte);
        }

        if sum != 0 {
            return Err(ChecksumError(sum));
        }

        let revision: u8 =
        // SAFETY: Same as above
            unsafe { core::ptr::read_unaligned(ptr.byte_offset(Self::REVISION_OFFSET) as *const _) };

        if revision != 0 {
            let length: u32 =
            // SAFETY: Same as above
                unsafe { core::ptr::read_unaligned(ptr.byte_offset(Self::LENGTH_OFFSET) as *const _) };

            let mut sum: u8 = 0;

            for i in 0..length {
                let byte =
                // SAFETY: The pointer is valid for reads for 20 bytes
                    unsafe { core::ptr::read_unaligned(ptr.byte_offset(i as _) as *const u8) };
                sum = sum.wrapping_add(byte);
            }

            if sum != 0 {
                return Err(ChecksumError(sum));
            }
        }

        Ok(())
    }

    /// Gets the [`SystemDescriptionTable`] this table points to.
    /// If [`revision`][Self::revision] is 0, gets the [`Rsdt`], else gets the [`Xsdt`].
    ///
    /// # Safety
    /// All of physical memory must be mapped at the virtual address `physical_memory_offset`
    pub unsafe fn get_system_description_table(
        &self,
        physical_memory_offset: u64,
    ) -> SystemDescriptionTable {
        match self.xsdp_fields {
            None => {
                let rsdt_virtual_addr = physical_memory_offset + self.rsdt_address as u64;

                // SAFETY: The pointer was obtained from an RSDP table so is valid
                let rsdt = unsafe {
                    Rsdt::read(rsdt_virtual_addr as *const _, physical_memory_offset).unwrap()
                };

                // SAFETY: The RSDT was
                unsafe { SystemDescriptionTable::from_rsdt(rsdt) }
            }
            Some(XsdpFields { xsdt_address, .. }) => {
                let xsdt_virtual_addr = physical_memory_offset + xsdt_address;

                // SAFETY: The pointer was obtained from an RSDP table so is valid
                let xsdt = unsafe {
                    Xsdt::read(xsdt_virtual_addr as *const _, physical_memory_offset).unwrap()
                };

                SystemDescriptionTable::from_xsdt(xsdt)
            }
        }
    }
}

impl Debug for XsdpFields {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("XsdpFields")
            .field("length", &format_args!("{:#x}", &self.length))
            .field("xsdt_address", &format_args!("{:#x}", &self.xsdt_address))
            .field(
                "extended_checksum",
                &format_args!("{:#02x}", &self.extended_checksum),
            )
            .field("reserved", &self.reserved)
            .finish()
    }
}

impl Debug for Rsdp {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Rsdp")
            .field("signature", &core::str::from_utf8(&self.signature).unwrap())
            .field("checksum", &format_args!("{:#02x}", &self.checksum))
            .field("oem_id", &core::str::from_utf8(&self.oem_id).unwrap())
            .field("revision", &self.revision)
            .field("rsdt_address", &format_args!("{:#x}", &self.rsdt_address))
            .field("xsdp_fields", &self.xsdp_fields)
            .finish()
    }
}
