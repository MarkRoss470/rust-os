// TODO: remove
#![allow(clippy::missing_docs_in_private_items, dead_code)]

pub mod generic_address_structure;

use core::fmt::Debug;

use x86_64::PhysAddr;

use self::generic_address_structure::GenericAddressStructure;

use super::{ChecksumError, SdtHeader};

/// A power management profile given to the OS by firmware
#[derive(Debug)]
pub enum PowerManagementProfile {
    Unspecified,
    Desktop,
    Mobile,
    Workstation,
    EnterpriseServer,
    SohoServer,
    AppliancePC,
    PerformanceServer,
    Tablet,

    Reserved,
}

impl PowerManagementProfile {
    fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Unspecified,
            1 => Self::Desktop,
            2 => Self::Mobile,
            3 => Self::Workstation,
            4 => Self::EnterpriseServer,
            5 => Self::SohoServer,
            6 => Self::AppliancePC,
            7 => Self::PerformanceServer,
            8 => Self::Tablet,

            _ => Self::Reserved,
        }
    }
}

#[bitfield(u8)]
pub struct BootArchitectureFlagsFirstByte {
    pub legacy_devices: bool,
    pub has_8042_controller: bool,
    pub vga_not_present: bool,
    pub msi_not_supported: bool,
    pub pcie_aspm_controls: bool,
    pub cmos_rtc_not_present: bool,

    #[bits(2)]
    _reserved: (),
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct BootArchitectureFlags {
    flags: BootArchitectureFlagsFirstByte,
    reserved: u8,
}

#[rustfmt::skip]
impl BootArchitectureFlags {
    /// Gets the [`legacy_devices`][BootArchitectureFlagsFirstByte::legacy_devices] field
    pub fn legacy_devices(&self) -> bool { self.flags.legacy_devices() }
    /// Gets the [`has_8042_controller`][BootArchitectureFlagsFirstByte::has_8042_controller] field
    pub fn has_8042_controller(&self) -> bool { self.flags.has_8042_controller() }
    /// Gets the [`vga_not_present`][BootArchitectureFlagsFirstByte::vga_not_present] field
    pub fn vga_not_present(&self) -> bool { self.flags.vga_not_present() }
    /// Gets the [`msi_not_supported`][BootArchitectureFlagsFirstByte::msi_not_supported] field
    pub fn msi_not_supported(&self) -> bool { self.flags.msi_not_supported() }
    /// Gets the [`pcie_aspm_controls`][BootArchitectureFlagsFirstByte::pcie_aspm_controls] field
    pub fn pcie_aspm_controls(&self) -> bool { self.flags.pcie_aspm_controls() }
    /// Gets the [`cmos_rtc_not_present`][BootArchitectureFlagsFirstByte::cmos_rtc_not_present] field
    pub fn cmos_rtc_not_present(&self) -> bool { self.flags.cmos_rtc_not_present() }
}

impl Debug for BootArchitectureFlags {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BootArchitectureFlags")
            .field("legacy_devices", &self.flags.legacy_devices())
            .field("has_8042_controller", &self.flags.has_8042_controller())
            .field("vga_not_present", &self.flags.vga_not_present())
            .field("msi_not_supported", &self.flags.msi_not_supported())
            .field("pcie_aspm_controls", &self.flags.pcie_aspm_controls())
            .field("cmos_rtc_not_present", &self.flags.cmos_rtc_not_present())
            .finish()
    }
}

#[repr(C)]
pub struct FadtMainFields {
    facs_addr: u32,
    dsdt_addr: u32,

    reserved0: u8,

    /// What kind of device the OS should power manage for.
    /// Converted to [`PowerManagementProfile`] in getter.
    preferred_power_management_profile: u8,
    sci_interrupt: u16,
    smi_command_port: u32,
    acpi_enable: u8,
    acpi_disable: u8,
    s4bios_req: u8,
    pstate_control: u8,
    pm1a_event_block: u32,
    pm1b_event_block: u32,
    pm1a_control_block: u32,
    pm1b_control_block: u32,
    pm2_control_block: u32,
    pm_timer_block: u32,
    gpe0_block: u32,
    gpe1_block: u32,
    pm1_event_length: u8,
    pm1_control_length: u8,
    pm2_control_length: u8,
    pm_timer_length: u8,
    gpe0_length: u8,
    gpe1_length: u8,
    gpe1_base: u8,
    c_state_control: u8,
    worst_c2_latency: u16,
    worst_c3_latency: u16,
    flush_size: u16,
    flush_stride: u16,
    duty_offset: u8,
    duty_width: u8,
    day_alarm: u8,
    month_alarm: u8,
    century: u8,

    /// Reserved in version 1.0
    boot_architecture_flags: BootArchitectureFlags,
    reserved1: u8,
    flags: u32,
}

pub struct Fadt {
    header: SdtHeader,
    main_fields: FadtMainFields,
    version_2_fields: Option<FadtVersion2Fields>,
}

impl Fadt {
    pub unsafe fn read(ptr: *const Self) -> Result<Self, ChecksumError> {
        // SAFETY: This read is within the table
        let header = unsafe { SdtHeader::read(ptr as *const _)? };
        let main_fields: FadtMainFields =
            // SAFETY: This read is within the table
             unsafe { core::ptr::read_unaligned(ptr.byte_offset(SdtHeader::TABLE_START) as *const _) };

        let version_2_fields = if header.revision == 0 {
            None
        } else {
            // SAFETY: This read is within the table
            Some(unsafe {
                core::ptr::read_unaligned(ptr.byte_offset(SdtHeader::TABLE_START + 109) as *const _)
            })
        };

        Ok(Self {
            header,
            main_fields,
            version_2_fields,
        })
    }

    pub fn dsdt_addr(&self) -> PhysAddr {
        if let Some(version_2_fields) = &self.version_2_fields {
            if version_2_fields.x_dsdt_addr != 0 {
                return PhysAddr::new(version_2_fields.x_dsdt_addr);
            }
        }

        PhysAddr::new(self.main_fields.dsdt_addr as u64)
    }

    pub fn facs_addr(&self) -> Option<PhysAddr> {
        if let Some(version_2_fields) = &self.version_2_fields {
            if version_2_fields.x_facs_addr != 0 {
                return Some(PhysAddr::new(version_2_fields.x_facs_addr));
            }
        }

        if self.main_fields.facs_addr != 0 {
            Some(PhysAddr::new(self.main_fields.facs_addr as u64))
        } else {
            None
        }
    }
}

#[repr(C)]
pub struct FadtVersion2Fields {
    reset_reg: GenericAddressStructure,
    reset_value: u8,
    reserved2: [u8; 3],

    x_facs_addr: u64,
    x_dsdt_addr: u64,

    x_pm1a_event_block: GenericAddressStructure,
    x_pm1b_event_block: GenericAddressStructure,
    x_pm1a_control_block: GenericAddressStructure,
    x_pm1b_control_block: GenericAddressStructure,
    x_pm2_control_block: GenericAddressStructure,
    x_pm_timer_block: GenericAddressStructure,
    x_gpe0_block: GenericAddressStructure,
    x_gpe1_block: GenericAddressStructure,
}

/// Fields which were added to the FADT in revision 5.0 of the ACPI spec
/// TODO: Find the exact version ID and add these to Fadt
pub struct FadtVersion5Fields {
    sleep_control_reg: GenericAddressStructure,
    sleep_status_reg: GenericAddressStructure,
}

/// Fields which were added to the FADT in revision 6.0 of the ACPI spec
/// TODO: Find the exact version ID and add these to Fadt
pub struct FadtVersion6Fields {
    hypervisor_vendor_identity: u64,
}

#[rustfmt::skip]
impl Fadt {
    /// Gets the [`preferred_power_management_profile`][Self::preferred_power_management_profile] field
    pub fn preferred_power_management_profile(&self) -> PowerManagementProfile {
        PowerManagementProfile::from_u8(self.main_fields.preferred_power_management_profile)
    }

    /// Gets the [`sci_interrupt`][Self::sci_interrupt] field
    pub fn sci_interrupt(&self) -> u16 { self.main_fields.sci_interrupt }
    /// Gets the [`smi_command_port`][Self::smi_command_port] field
    pub fn smi_command_port(&self) -> u32 { self.main_fields.smi_command_port }
    /// Gets the [`acpi_enable`][Self::acpi_enable] field
    pub fn acpi_enable(&self) -> u8 { self.main_fields.acpi_enable }
    /// Gets the [`acpi_disable`][Self::acpi_disable] field
    pub fn acpi_disable(&self) -> u8 { self.main_fields.acpi_disable }
    /// Gets the [`s4bios_req`][Self::s4bios_req] field
    pub fn s4bios_req(&self) -> u8 { self.main_fields.s4bios_req }
    /// Gets the [`pstate_control`][Self::pstate_control] field
    pub fn pstate_control(&self) -> u8 { self.main_fields.pstate_control }
    /// Gets the [`pm1a_event_block`][Self::pm1a_event_block] field
    pub fn pm1a_event_block(&self) -> u32 { self.main_fields.pm1a_event_block }
    /// Gets the [`pm1b_event_block`][Self::pm1b_event_block] field
    pub fn pm1b_event_block(&self) -> u32 { self.main_fields.pm1b_event_block }
    /// Gets the [`pm1a_control_block`][Self::pm1a_control_block] field
    pub fn pm1a_control_block(&self) -> u32 { self.main_fields.pm1a_control_block }
    /// Gets the [`pm1b_control_block`][Self::pm1b_control_block] field
    pub fn pm1b_control_block(&self) -> u32 { self.main_fields.pm1b_control_block }
    /// Gets the [`pm2_control_block`][Self::pm2_control_block] field
    pub fn pm2_control_block(&self) -> u32 { self.main_fields.pm2_control_block }
    /// Gets the [`pm_timer_block`][Self::pm_timer_block] field
    pub fn pm_timer_block(&self) -> u32 { self.main_fields.pm_timer_block }
    /// Gets the [`gpe0_block`][Self::gpe0_block] field
    pub fn gpe0_block(&self) -> u32 { self.main_fields.gpe0_block }
    /// Gets the [`gpe1_block`][Self::gpe1_block] field
    pub fn gpe1_block(&self) -> u32 { self.main_fields.gpe1_block }
    /// Gets the [`pm1_event_length`][Self::pm1_event_length] field
    pub fn pm1_event_length(&self) -> u8 { self.main_fields.pm1_event_length }
    /// Gets the [`pm1_control_length`][Self::pm1_control_length] field
    pub fn pm1_control_length(&self) -> u8 { self.main_fields.pm1_control_length }
    /// Gets the [`pm2_control_length`][Self::pm2_control_length] field
    pub fn pm2_control_length(&self) -> u8 { self.main_fields.pm2_control_length }
    /// Gets the [`pm_timer_length`][Self::pm_timer_length] field
    pub fn pm_timer_length(&self) -> u8 { self.main_fields.pm_timer_length }
    /// Gets the [`gpe0_length`][Self::gpe0_length] field
    pub fn gpe0_length(&self) -> u8 { self.main_fields.gpe0_length }
    /// Gets the [`gpe1_length`][Self::gpe1_length] field
    pub fn gpe1_length(&self) -> u8 { self.main_fields.gpe1_length }
    /// Gets the [`gpe1_base`][Self::gpe1_base] field
    pub fn gpe1_base(&self) -> u8 { self.main_fields.gpe1_base }
    /// Gets the [`c_state_control`][Self::c_state_control] field
    pub fn c_state_control(&self) -> u8 { self.main_fields.c_state_control }
    /// Gets the [`worst_c2_latency`][Self::worst_c2_latency] field
    pub fn worst_c2_latency(&self) -> u16 { self.main_fields.worst_c2_latency }
    /// Gets the [`worst_c3_latency`][Self::worst_c3_latency] field
    pub fn worst_c3_latency(&self) -> u16 { self.main_fields.worst_c3_latency }
    /// Gets the [`flush_size`][Self::flush_size] field
    pub fn flush_size(&self) -> u16 { self.main_fields.flush_size }
    /// Gets the [`flush_stride`][Self::flush_stride] field
    pub fn flush_stride(&self) -> u16 { self.main_fields.flush_stride }
    /// Gets the [`duty_offset`][Self::duty_offset] field
    pub fn duty_offset(&self) -> u8 { self.main_fields.duty_offset }
    /// Gets the [`duty_width`][Self::duty_width] field
    pub fn duty_width(&self) -> u8 { self.main_fields.duty_width }
    /// Gets the [`day_alarm`][Self::day_alarm] field
    pub fn day_alarm(&self) -> u8 { self.main_fields.day_alarm }
    /// Gets the [`month_alarm`][Self::month_alarm] field
    pub fn month_alarm(&self) -> u8 { self.main_fields.month_alarm }
    /// Gets the [`century`][Self::century] field
    pub fn century(&self) -> u8 { self.main_fields.century }
    /// Gets the [`boot_architecture_flags`][Self::boot_architecture_flags] field
    pub fn boot_architecture_flags(&self) -> BootArchitectureFlags { self.main_fields.boot_architecture_flags }
    /// Gets the [`flags`][Self::flags] field
    pub fn flags(&self) -> u32 { self.main_fields.flags }
}

impl Debug for Fadt {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut s = f.debug_struct("FadtMainFields");
        s.field("facs_addr", &self.facs_addr())
            .field("dsdt_addr", &self.dsdt_addr())
            .field(
                "preferred_power_management_profile",
                &self.preferred_power_management_profile(),
            )
            .field("sci_interrupt", &self.sci_interrupt())
            .field("smi_command_port", &self.smi_command_port())
            .field("acpi_enable", &self.acpi_enable())
            .field("acpi_disable", &self.acpi_disable())
            .field("s4bios_req", &self.s4bios_req())
            .field("pstate_control", &self.pstate_control())
            .field("pm1a_event_block", &self.pm1a_event_block())
            .field("pm1b_event_block", &self.pm1b_event_block())
            .field("pm1a_control_block", &self.pm1a_control_block())
            .field("pm1b_control_block", &self.pm1b_control_block())
            .field("pm2_control_block", &self.pm2_control_block())
            .field("pm_timer_block", &self.pm_timer_block())
            .field("gpe0_block", &self.gpe0_block())
            .field("gpe1_block", &self.gpe1_block())
            .field("pm1_event_length", &self.pm1_event_length())
            .field("pm1_control_length", &self.pm1_control_length())
            .field("pm2_control_length", &self.pm2_control_length())
            .field("pm_timer_length", &self.pm_timer_length())
            .field("gpe0_length", &self.gpe0_length())
            .field("gpe1_length", &self.gpe1_length())
            .field("gpe1_base", &self.gpe1_base())
            .field("c_state_control", &self.c_state_control())
            .field("worst_c2_latency", &self.worst_c2_latency())
            .field("worst_c3_latency", &self.worst_c3_latency())
            .field("flush_size", &self.flush_size())
            .field("flush_stride", &self.flush_stride())
            .field("duty_offset", &self.duty_offset())
            .field("duty_width", &self.duty_width())
            .field("day_alarm", &self.day_alarm())
            .field("month_alarm", &self.month_alarm())
            .field("century", &self.century())
            .field("boot_architecture_flags", &self.boot_architecture_flags())
            .field("flags", &self.flags());

        if let Some(v2_fields) = &self.version_2_fields {
            s   .field("reset_reg", &v2_fields.reset_reg)
            .field("reset_value", &v2_fields.reset_value)
            .field("reserved2", &v2_fields.reserved2)
            .field("x_facs_addr", &format_args!("{:#x}", v2_fields.x_facs_addr))
            .field("x_dsdt_addr", &format_args!("{:x}", v2_fields.x_dsdt_addr))
            .field("x_pm1a_event_block", &v2_fields.x_pm1a_event_block)
            .field("x_pm1b_event_block", &v2_fields.x_pm1b_event_block)
            .field("x_pm1a_control_block", &v2_fields.x_pm1a_control_block)
            .field("x_pm1b_control_block", &v2_fields.x_pm1b_control_block)
            .field("x_pm2_control_block", &v2_fields.x_pm2_control_block)
            .field("x_pm_timer_block", &v2_fields.x_pm_timer_block)
            .field("x_gpe0_block", &v2_fields.x_gpe0_block)
            .field("x_gpe1_block", &v2_fields.x_gpe1_block);
        }

        s.finish()
    }
}

#[rustfmt::skip]
impl FadtVersion2Fields {
    /// Gets the [`reset_reg`][Self::reset_reg] field
    pub fn reset_reg(&self) -> &GenericAddressStructure { &self.reset_reg }
    /// Gets the [`reset_value`][Self::reset_value] field
    pub fn reset_value(&self) -> &u8 { &self.reset_value }
    /// Gets the [`x_firmware_control`][Self::x_firmware_control] field
    pub fn x_facs_addr(&self) -> &u64 { &self.x_facs_addr }
    /// Gets the [`x_dsdt`][Self::x_dsdt] field
    pub fn x_dsdt_addr(&self) -> &u64 { &self.x_dsdt_addr }
    /// Gets the [`x_pm1a_event_block`][Self::x_pm1a_event_block] field
    pub fn x_pm1a_event_block(&self) -> &GenericAddressStructure { &self.x_pm1a_event_block }
    /// Gets the [`x_pm1b_event_block`][Self::x_pm1b_event_block] field
    pub fn x_pm1b_event_block(&self) -> &GenericAddressStructure { &self.x_pm1b_event_block }
    /// Gets the [`x_pm1a_control_block`][Self::x_pm1a_control_block] field
    pub fn x_pm1a_control_block(&self) -> &GenericAddressStructure { &self.x_pm1a_control_block }
    /// Gets the [`x_pm1b_control_block`][Self::x_pm1b_control_block] field
    pub fn x_pm1b_control_block(&self) -> &GenericAddressStructure { &self.x_pm1b_control_block }
    /// Gets the [`x_pm2_control_block`][Self::x_pm2_control_block] field
    pub fn x_pm2_control_block(&self) -> &GenericAddressStructure { &self.x_pm2_control_block }
    /// Gets the [`x_pm_timer_block`][Self::x_pm_timer_block] field
    pub fn x_pm_timer_block(&self) -> &GenericAddressStructure { &self.x_pm_timer_block }
    /// Gets the [`x_gpe0_block`][Self::x_gpe0_block] field
    pub fn x_gpe0_block(&self) -> &GenericAddressStructure { &self.x_gpe0_block }
    /// Gets the [`x_gpe1_block`][Self::x_gpe1_block] field
    pub fn x_gpe1_block(&self) -> &GenericAddressStructure { &self.x_gpe1_block }
}

impl Debug for FadtVersion2Fields {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FadtVersion2Fields")

            .finish()
    }
}
