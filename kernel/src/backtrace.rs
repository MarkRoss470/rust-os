#![allow(clippy::missing_docs_in_private_items)]

use core::{
    arch::asm,
    convert::Infallible,
    num::TryFromIntError,
    sync::atomic::{AtomicBool, Ordering},
};

use alloc::boxed::Box;
use gimli::{
    BaseAddresses, DW_AT_high_pc, DW_AT_linkage_name, DW_AT_low_pc, DW_AT_name,
    DW_TAG_subprogram, EndianSlice, LineRow, NativeEndian, Register, UnwindContext,
};

use object::{elf::FileHeader64, Object, ObjectSection};
use x86_64::VirtAddr;

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
    /// An unsupported type of data or instruction was encountered
    Unimplemented(&'static str),
    /// Arithmetic overflow etc.
    MathsError,
    /// A register was undefined which shouldn't have been
    UndefinedRegister,

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
    fn from(_: Infallible) -> Self {
        unreachable!()
    }
}

impl From<TryFromIntError> for BacktracePrintError {
    fn from(_: TryFromIntError) -> Self {
        Self::MathsError
    }
}

type ElfFile<'a> = object::read::elf::ElfFile<'a, FileHeader64<object::NativeEndian>>;
type ElfSection<'a> =
    object::read::elf::ElfSection<'a, 'a, FileHeader64<object::NativeEndian>, &'a [u8]>;
type EhFrameHeader<'a> = gimli::ParsedEhFrameHdr<gimli::EndianSlice<'a, gimli::LittleEndian>>;
type EhFrame<'a> = gimli::EhFrame<gimli::EndianSlice<'a, gimli::LittleEndian>>;
type Dwarf<'a> = gimli::Dwarf<gimli::EndianSlice<'a, gimli::LittleEndian>>;
type UnitHeader<'a> = gimli::UnitHeader<gimli::EndianSlice<'a, gimli::NativeEndian>, usize>;
type UnwindTableRow<'a> = gimli::UnwindTableRow<EndianSlice<'a, gimli::NativeEndian>>;
type LineProgramHeader<'a> = gimli::LineProgramHeader<EndianSlice<'a, gimli::NativeEndian>>;

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

    let interrupt_handlers = crate::cpu::interrupt_handler_addresses();

    let stack_pointer: u64;
    let instruction_pointer: u64;
    let mut rbp: u64;

    // SAFETY: This reads the RSP, RIP, and RBP registers and doesn't affect any other registers.
    unsafe {
        asm!(
            "push rsp",
            "pop {stack_pointer}",
            "lea {instruction_pointer}, [rip]",
            "mov {rbp}, rbp",
            stack_pointer = out(reg) stack_pointer,
            instruction_pointer = out(reg) instruction_pointer,
            rbp = out(reg) rbp
        );
    }

    let mut frame_pointer = stack_pointer;
    let mut address_to_look_up = instruction_pointer;
    let mut ctx = Box::new(UnwindContext::new());

    println!();

    // Don't print the first few stack frames because they're just the panicking and backtracing code
    let mut frames_checked = 0;
    const FRAMES_TO_SKIP: usize = 4;

    loop {
        frames_checked += 1;

        let function_start = if frames_checked > FRAMES_TO_SKIP {
            print_location(&dwarf, address_to_look_up, frames_checked - FRAMES_TO_SKIP)
                .unwrap_or_else(|e| {
                    println!("{address_to_look_up:#x} @ ?? - Error getting frame info: {e:?}");
                    None
                })
        } else {
            None
        };

        let unwinding_info = table.unwind_info_for_address(
            &eh_frame,
            &base_addresses,
            &mut ctx,
            // Look up one before the current address to correctly find frames when at the very end of a function
            address_to_look_up - 1,
            gimli::UnwindSection::cie_from_offset,
        )?;

        frame_pointer = match unwinding_info.cfa() {
            gimli::CfaRule::RegisterAndOffset { register, offset } => {
                let register_value = match register.0 {
                    6 => rbp,
                    7 => frame_pointer,
                    _ => {
                        return Err(BacktracePrintError::Unimplemented(
                            "Untracked register needed for calculation",
                        ))
                    }
                };

                register_value
                    .checked_add_signed(*offset)
                    .ok_or(BacktracePrintError::MathsError)?
            }

            gimli::CfaRule::Expression(_) => {
                return Err(BacktracePrintError::Unimplemented(
                    "Expression rule for getting return pointer",
                ))
            }
        };

        rbp = eval_register(unwinding_info, Register(6), frame_pointer, rbp)?
            .ok_or(BacktracePrintError::UndefinedRegister)?;

        // If this frame is an interrupt handler, the next stack frame will be invalid so stop the trace
        if let Some(function_start) = function_start {
            if interrupt_handlers.contains(&VirtAddr::new(function_start)) {
                return Ok(())
            }
        }

        // SAFETY: TODO
        address_to_look_up = match eval_register(unwinding_info, Register(16), frame_pointer, rbp)?
        {
            // A null pointer or undefined RIP means that this is the last call frame
            Some(0) | None => return Ok(()),
            Some(addr) => addr,
        };
    }
}

fn eval_register(
    unwinding_info: &UnwindTableRow,
    register: Register,
    frame_pointer: u64,
    rbp: u64,
) -> Result<Option<u64>, BacktracePrintError> {
    match unwinding_info.register(register) {
        gimli::RegisterRule::Offset(off) => {
            // SAFETY: TODO
            let value = unsafe {
                (frame_pointer as *const u64)
                    .byte_offset(off.try_into()?)
                    .read()
            };

            Ok(Some(value))
        }

        gimli::RegisterRule::Undefined => {
            if register.0 == 7 {
                Ok(Some(frame_pointer))
            } else if register.0 == 6 {
                Ok(Some(rbp))
            } else {
                Ok(None)
            }
        }

        rule => todo!("{rule:?}"),
        // _ => return Err(BacktracePrintError::Unimplemented("")),
    }
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

fn get_line_row<'a>(
    dwarf: &'a Dwarf<'a>,
    address_to_look_up: u64,
) -> Result<Option<(UnitHeader, LineRow, LineProgramHeader<'a>)>, BacktracePrintError> {
    // Get the offset into `.debug_info` of the compilation unit for this address
    let cu_offset = get_cu_offset(dwarf, address_to_look_up)?
        .ok_or(BacktracePrintError::AddressNotFoundInARanges)?;
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
        if row.address() == address_to_look_up {
            found = Some(*row);
            break;
        }

        if let Some(previous) = previous {
            if (previous.address()..row.address()).contains(&address_to_look_up) {
                found = Some(previous);
                break;
            }
        };

        previous = Some(*row);
    }

    Ok(found.map(|found| (cu, found, rows.header().clone())))
}

/// TODO: docs
/// Prints out a line of a stack trace for the given address.
/// Also computes and returns the starting address of the function the address is in.
fn print_location(
    dwarf: &Dwarf,
    address: u64,
    frame_number: usize,
) -> Result<Option<u64>, BacktracePrintError> {
    print!("#{frame_number:03} 0x{address:016x} ");

    // Look up one before the current address to correctly find frames when at the very end of a function
    let address_to_look_up = address - 1;

    let Some((cu, row, header)) = get_line_row(dwarf, address_to_look_up)? else {
        return Ok(None);
    };

    let abbreviations = cu.abbreviations(&dwarf.debug_abbrev)?;
    let mut entries = cu.entries(&abbreviations);
    let mut found = false;
    let mut function_start = None;

    while let Some((_, entry)) = entries.next_dfs()? {
        if entry.tag() != DW_TAG_subprogram {
            continue;
        }

        let name = entry
            .attr(DW_AT_name)?
            .and_then(|attr| attr.string_value(&dwarf.debug_str))
            .and_then(|attr| attr.to_string().ok());

        let Some(name) = name else {
            continue;
        };

        let Ok(Some(gimli::AttributeValue::Addr(start))) = entry.attr_value(DW_AT_low_pc) else {
            continue;
        };

        let Ok(Some(gimli::AttributeValue::Udata(len))) = entry.attr_value(DW_AT_high_pc) else {
            continue;
        };

        if (start..start + len).contains(&address_to_look_up) {
            function_start = Some(start);
            println!("@ {name}");

            let symbol_name = entry
                .attr(DW_AT_linkage_name)?
                .and_then(|attr| attr.string_value(&dwarf.debug_str))
                .and_then(|attr| attr.to_string().ok());

            if let Some(symbol_name) = symbol_name {
                println!("       {}", rustc_demangle::demangle(symbol_name));
            }

            found = true;
            break;
        }
    }

    // If the function was not found, print a newline to keep consistent formatting
    if !found {
        println!();
    }

    // Print out debug info for found line
    let file = &row.file(&header);

    let Some(file) = file else {
        println!();
        return Ok(function_start);
    };

    let unit = gimli::Unit::new(dwarf, cu)?;

    let dir = file.directory(&header);
    let dir = dir.and_then(|dir| dwarf.attr_string(&unit, dir).ok());
    let dir = dir.as_ref().and_then(|dir| core::str::from_utf8(dir).ok());

    let file = file.path_name();
    let file = dwarf.attr_string(&unit, file)?;
    let file = core::str::from_utf8(&file).unwrap_or("??");

    print!("       in {}/{file}", dir.unwrap_or("??"));

    if let Some(line) = row.line() {
        print!(" - {line}");

        match row.column() {
            gimli::ColumnType::LeftEdge => (),
            gimli::ColumnType::Column(column) => print!(":{column}"),
        }
    }

    println!();

    Ok(function_start)
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
