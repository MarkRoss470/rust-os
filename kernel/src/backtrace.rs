#![allow(clippy::missing_docs_in_private_items)]

use core::{
    arch::asm,
    convert::Infallible,
    sync::atomic::{AtomicBool, Ordering},
};

use alloc::boxed::Box;
use gimli::{BaseAddresses, NativeEndian, UnwindContext};
use log::debug;
use object::{elf::FileHeader64, Object, ObjectSection};

use crate::{print, println, KERNEL_STATE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BacktracePrintError {
    InitRdLocked,
    InitRdUnset,
    ObjectReadError(object::read::Error),
    GimliError(gimli::Error),
    MissingSection(&'static str),
    NoEhTable,
    AddressNotFoundInARanges,
    BadDebugInfoEntry,
    /// The panic leading to this backtrace started in the backtracing code itself
    BacktraceOngoing,
}

impl From<gimli::Error> for BacktracePrintError {
    fn from(v: gimli::Error) -> Self {
        Self::GimliError(v)
    }
}

impl From<object::read::Error> for BacktracePrintError {
    fn from(value: object::read::Error) -> Self {
        Self::ObjectReadError(value)
    }
}

impl From<Infallible> for BacktracePrintError {
    fn from(value: Infallible) -> Self {
        unreachable!()
    }
}

type ElfFile<'a> = object::read::elf::ElfFile<'a, FileHeader64<object::NativeEndian>>;
type ElfSection<'a> =
    object::read::elf::ElfSection<'a, 'a, FileHeader64<object::NativeEndian>, &'a [u8]>;
type EhFrameHeader<'a> = gimli::ParsedEhFrameHdr<gimli::EndianSlice<'a, gimli::LittleEndian>>;
type EhFrame<'a> = gimli::EhFrame<gimli::EndianSlice<'a, gimli::LittleEndian>>;
type Dwarf<'a> = gimli::Dwarf<gimli::EndianSlice<'a, gimli::LittleEndian>>;
type UnitHeader<'a> = gimli::UnitHeader<gimli::EndianSlice<'a, gimli::NativeEndian>, usize>;

// TODO: make thread-local
/// Whether a backtrace is currently being printed.
/// This is used to make sure a backtrace isn't printed if the panic came from the backtracing code,
/// in order to prevent infinite loops of backtracing.
static BACKTRACE_ONGOING: AtomicBool = AtomicBool::new(false);

/// Prints a stack backtrace
#[allow(dead_code)] // TODO: backtraces in test mode
pub fn backtrace() -> Result<(), BacktracePrintError> {
    let backtracing = BACKTRACE_ONGOING.swap(true, Ordering::Relaxed);

    if backtracing {
        println!("Panic occurred in backtracing code - skipping second backtrace");
        return Err(BacktracePrintError::BacktraceOngoing);
    }

    let r = backtrace_impl();

    BACKTRACE_ONGOING.store(false, Ordering::Relaxed);

    r
}

/// The real implementation of printing a backtrace
fn backtrace_impl() -> Result<(), BacktracePrintError> {
    let rd = KERNEL_STATE
        .initrd
        .try_read()
        .ok_or(BacktracePrintError::InitRdLocked)?
        .ok_or(BacktracePrintError::InitRdUnset)?;

    let object_file: ElfFile = ElfFile::parse(rd)?;
    let base_addresses = get_base_addresses(&object_file)?;
    let eh_frame_header = get_eh_frame_header(&object_file, &base_addresses)?;
    let eh_frame = get_eh_frame(&object_file)?;
    let table = eh_frame_header
        .table()
        .ok_or(BacktracePrintError::NoEhTable)?;

    let dwarf = Dwarf::load::<_, Infallible>(|id| {
        let data = object_file
            .section_by_name(id.name())
            .and_then(|s| s.data().ok())
            .unwrap_or(&[]);

        Ok(gimli::EndianSlice::new(data, NativeEndian))
    })?;

    let stack_pointer: u64;
    let instruction_pointer: u64;
    // SAFETY: This reads the stack pointer register and doesn't affect any other registers
    unsafe {
        asm!(
            "push rsp",
            "pop {stack_pointer}",
            "lea {instruction_pointer}, [rip]",
            stack_pointer = out(reg) stack_pointer,
            instruction_pointer = out(reg) instruction_pointer
        );
    }

    let mut stack_pointer = stack_pointer - 8;
    let mut address_to_look_up = instruction_pointer;
    let mut ctx = Box::new(UnwindContext::new());

    println!();

    // Don't print the first few stack frames because they're just the panicking code + this function
    let mut frames_checked = 0;
    const FRAMES_TO_SKIP: usize = 3;

    loop {
        frames_checked += 1;
        if frames_checked > FRAMES_TO_SKIP {
            print_location(&dwarf, address_to_look_up, frames_checked - FRAMES_TO_SKIP)
                .unwrap_or_else(|_| {
                    println!("{address_to_look_up:#x} @ ?? - Error getting frame info")
                });
        }
        // debug!("Address of function is {address_to_look_up:#x}");

        let unwinding_info = table.unwind_info_for_address(
            &eh_frame,
            &base_addresses,
            &mut ctx,
            address_to_look_up,
            gimli::UnwindSection::cie_from_offset,
        );

        let unwinding_info = match unwinding_info {
            Ok(u) => u,
            Err(gimli::Error::NoUnwindInfoForAddress) => return Ok(()),
            Err(e) => Err(e)?,
        };

        // debug!("{fde:#?}");
        // debug!("{unwinding_info:#?}");

        let frame_offset = match unwinding_info.cfa() {
            gimli::CfaRule::RegisterAndOffset { register, offset } => {
                assert_eq!(register.0, 7);
                *offset
            }
            gimli::CfaRule::Expression(_) => todo!(),
        };

        stack_pointer = stack_pointer.checked_add_signed(frame_offset).unwrap();

        // debug!("Stack pointer of next address is {stack_pointer:#x}");

        // SAFETY: TODO
        address_to_look_up = unsafe { (stack_pointer as *const u64).read() };
    }

    Ok(())
}

fn get_cu_offset(
    dwarf: &Dwarf,
    address: u64,
) -> Result<Option<gimli::DebugInfoOffset>, BacktracePrintError> {
    let aranges = dwarf.debug_aranges;
    let mut headers = aranges.headers();

    while let Some(header) = headers.next()? {
        let mut entries = header.entries();
        while let Some(entry) = entries.next()? {
            let range = entry.range();
            if (range.begin..range.end).contains(&address) {
                return Ok(Some(header.debug_info_offset()));
            }
        }
    }

    Ok(None)
}

fn get_cu_at_offset<'a>(
    dwarf: &'a Dwarf<'a>,
    offset: gimli::DebugInfoOffset,
) -> Result<Option<UnitHeader<'a>>, BacktracePrintError> {
    let debug_info = dwarf.debug_info;
    let mut units = debug_info.units();

    while let Some(unit) = units.next()? {
        if unit.offset().as_debug_info_offset() == Some(offset) {
            return Ok(Some(unit));
        }
    }

    Ok(None)
}

/// TODO: docs
fn print_location(
    dwarf: &Dwarf,
    address: u64,
    frame_number: usize,
) -> Result<(), BacktracePrintError> {
    // Get the offset into `.debug_info` of the compilation unit for this address
    let cu_offset =
        get_cu_offset(dwarf, address)?.ok_or(BacktracePrintError::AddressNotFoundInARanges)?;
    // Get the compilation unit for this address
    let cu = get_cu_at_offset(dwarf, cu_offset)?.ok_or(BacktracePrintError::BadDebugInfoEntry)?;

    // Get the debug entries for the CU
    let abbreviations = cu.abbreviations(&dwarf.debug_abbrev)?;
    let mut entries = cu.entries(&abbreviations);

    // Get the root node
    let (_, root_entry) = entries
        .next_dfs()?
        .ok_or(BacktracePrintError::BadDebugInfoEntry)?;

    // Read the offset into the `.debug_line` section for the CU's line info
    let statement_list_offset = root_entry
        .attr_value(gimli::DW_AT_stmt_list)?
        .ok_or(BacktracePrintError::BadDebugInfoEntry)?;
    let gimli::AttributeValue::DebugLineRef(offset) = statement_list_offset else {
        return Err(BacktracePrintError::BadDebugInfoEntry);
    };

    // Get the line info for the CU
    let lines = dwarf.debug_line;
    let program = lines.program(offset, cu.address_size(), None, None)?;
    let mut rows = program.rows();

    // Find the info for the current address
    let mut previous: Option<gimli::LineRow> = None;
    let mut found = None;

    while let Some((_, row)) = rows.next_row()? {
        if row.address() == address {
            found = Some(*row);
            break;
        }

        if let Some(previous) = previous {
            if (previous.address()..row.address()).contains(&address) {
                found = Some(previous);
                break;
            }
        };

        previous = Some(*row);
    }

    print!("#{frame_number:03}  0x{address:016x} in ");

    // Print out debug info for found line
    if let Some(found) = found {
        let file = &found.file(rows.header());

        if let Some(file) = file {
            let unit = gimli::Unit::new(dwarf, cu)?;

            let dir = file.directory(rows.header());
            let dir = dir.and_then(|dir| dwarf.attr_string(&unit, dir).ok());
            let dir = dir.as_ref().and_then(|dir| core::str::from_utf8(dir).ok());

            let file = file.path_name();
            let file = dwarf.attr_string(&unit, file)?;
            let file = core::str::from_utf8(&file).unwrap();

            print!("{}/{file}", dir.unwrap_or("??"));

            if let Some(line) = found.line() {
                print!(" - {line}");

                match found.column() {
                    gimli::ColumnType::LeftEdge => (),
                    gimli::ColumnType::Column(column) => print!(":{column}"),
                }
            }

            println!();
        } else {
            println!("??");
        }
    }

    Ok(())
}

fn get_eh_frame<'a>(object_file: &'a ElfFile<'a>) -> Result<EhFrame<'a>, BacktracePrintError> {
    let eh_frame = get_section(object_file, ".eh_frame")?;
    let eh_frame = gimli::EhFrame::new(eh_frame.data()?, gimli::NativeEndian);
    Ok(eh_frame)
}

fn get_base_addresses(object_file: &ElfFile) -> Result<BaseAddresses, BacktracePrintError> {
    let base_addresses = BaseAddresses::default()
        .set_eh_frame(get_section(object_file, ".eh_frame")?.address())
        .set_eh_frame_hdr(get_section(object_file, ".eh_frame_hdr")?.address())
        .set_text(get_section(object_file, ".text")?.address())
        .set_got(get_section(object_file, ".got")?.address());

    Ok(base_addresses)
}

fn get_eh_frame_header<'a>(
    object_file: &'a ElfFile<'_>,
    base_addresses: &BaseAddresses,
) -> Result<EhFrameHeader<'a>, BacktracePrintError> {
    let eh_frame_header = get_section(object_file, ".eh_frame_hdr")?;
    let eh_frame_header = gimli::EhFrameHdr::new(eh_frame_header.data()?, gimli::NativeEndian);
    let eh_frame_header = eh_frame_header.parse(base_addresses, 64)?;
    Ok(eh_frame_header)
}

fn get_section<'a>(
    object_file: &'a ElfFile,
    section: &'static str,
) -> Result<ElfSection<'a>, BacktracePrintError> {
    object_file
        .section_by_name(section)
        .ok_or(BacktracePrintError::MissingSection(section))
}
