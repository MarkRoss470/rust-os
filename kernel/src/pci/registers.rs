//! Types for reading values from PCI registers

use super::{
    bar::Bar,
    classcodes::{ClassCode, InvalidValueError},
    devices::PciFunction, split_to_u16, split_to_u8, PciMappedFunction,
};

/// The vendor:device code of a particular PCI device
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciDeviceId {
    /// The PCI vendor code of the device's manufacturer
    pub vendor: u16,
    /// The device code. Device codes are specific to certain manufacturers
    pub device: u16,
}

impl PciDeviceId {
    /// Gets whether the device code is valid.
    /// A code is invalid if the vendor is `0xffff`, which signals that there is no device connected to that slot.
    pub fn is_valid(&self) -> bool {
        self.vendor != 0xffff
    }
}

impl core::fmt::Display for PciDeviceId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "0x{:04x} 0x{:04x}  ", self.vendor, self.device)?;

        match (self.vendor, self.device) {
            (0x8086, 0x7113) => write!(f, "Intel 82371AB/EB/MB PIIX4 ACPI"),
            (0x8086, _) => write!(f, "Unknown Intel device"),
            (0x10DE, _) => write!(f, "Unknown NVIDIA device"),
            (0x1234, 0x1111) => write!(f, "QEMU virtual video controller"),
            (0x1b36, 0x000d) => write!(f, "QEMU virtual XHCI USB controller"),
            (_, _) => write!(f, "Unknown device"),
        }
    }
}

/// The timing values for the [`devsel_timing`][StatusRegister::devsel_timing] method
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DEVSELTiming {
    /// Device asserts `DEVSEL#` after 1 clock cycle
    Fast = 0,
    /// Device asserts `DEVSEL#` after 2 clock cycles
    Medium = 1,
    /// Device asserts `DEVSEL#` after 3 clock cycles
    Slow = 2,
}

impl DEVSELTiming {
    /// Converts the [`DEVSELTiming`] into bits. Needed for the inclusion of the enum in [`StatusRegister`]
    const fn into_bits(self) -> u16 {
        self as _
    }

    /// Constructs a [`DEVSELTiming`] from bits. Needed for the inclusion of the enum in [`StatusRegister`]
    const fn from_bits(value: u16) -> Self {
        match value {
            0 => Self::Fast,
            1 => Self::Medium,
            2 => Self::Slow,
            _ => panic!("Invalid DEVSEL timing value"),
        }
    }
}

/// The value of the status register of a PCI device.
/// This register just contains flags about the device's capabilities and what errors have occurred.
#[bitfield(u16)]
pub struct StatusRegister {
    #[bits(3)]
    _reserved_1: (),

    /// Whether the device has interrupts enabled
    pub interrupts_enabled: bool,
    /// Whether the device has a pointer to a linked list of capabilities in register at offset 0x34
    pub has_capabilities_list: bool,
    /// Whether the device is capable of operating and 66MHz
    pub is_66_mhz_capable: bool,

    #[bits(1)]
    _reserved_0: (),

    /// Whether the device can accept fast back-to-back transactions which are not from the same agent
    pub is_fast_back_to_back_capable: bool,
    /// Whether a master data parity error has occurred on this device. This occurs when
    pub has_master_data_parity_error: bool,

    #[bits(2)]
    pub devsel_timing: DEVSELTiming,

    /// Whether a transaction has been terminated by this device with a Target-Abort
    pub signalled_target_abort: bool,
    /// Whether a transaction has been terminated by a master device with a Target-Abort
    pub received_target_abort: bool,
    /// Whether a transaction has been terminated by a master device with a Master-Abort
    pub received_master_abort: bool,
    /// Whether the device has asserted a `SERR#` (connection failure)
    pub signalled_system_error: bool,
    /// Whether the device has detected a parity error
    pub detected_parity_error: bool,
}

#[bitfield(u16)]
pub struct CommandRegister {
    /// Whether the device can respond to IO space accesses
    pub supports_io_space_accesses: bool,
    /// Whether the device can respond to memory space accesses
    pub supports_memory_space_accesses: bool,
    /// Whether the device can behave as a bus master
    pub can_be_bus_master: bool,
    /// Whether the device can monitor Special Cycle operations
    pub can_monitor_special_cycles: bool,
    /// Whether the device can generate the Memory Write and Invalidate command; otherwise, the Memory Write command must be used.
    pub can_generate_memory_write_and_invalidate: bool,
    /// Whether the device does not respond to palette register writes and will snoop the data; otherwise, the device will treat palette write accesses like all other accesses.
    pub responds_to_palette_register_writes: bool,
    /// Whether parity errors are enabled for this device. If set to `true`, a `PERR#` will be asserted when a parity error is detected.
    pub parity_errors_enabled: bool,

    #[bits(1)]
    _reserved_0: (),

    /// Whether the `SERR#` driver is enabled
    pub system_error_enable: bool,
    /// Whether the device is allowed to generate fast back-to-back transactions
    pub fast_back_to_back_enable: bool,
    /// Whether interrupts are disabled for this device (whether the `INTx#` signals are disabled)
    pub interrupts_disabled: bool,

    #[bits(5)]
    _reserved_1: (),
}

/// A PCI interrupt pin. PCI has 4 interrupt pins, which can each be connected to any line on the IO/APIC.
#[derive(Debug, Clone, Copy)]
pub enum InterruptPin {
    /// The `INTA#` pin
    IntA,
    /// The `INTB#` pin
    IntB,
    /// The `INTC#` pin
    IntC,
    /// The `INTD#` pin
    IntD,
    /// The device does not use an interrupt pin
    None,
}

impl InterruptPin {
    /// Constructs an [`InterruptPin`] from the corresponding value of the interrupt pin header field.
    const fn from_pin_number(number: u8) -> Result<Self, InvalidValueError> {
        match number {
            0 => Ok(Self::None),
            1 => Ok(Self::IntA),
            2 => Ok(Self::IntB),
            3 => Ok(Self::IntC),
            4 => Ok(Self::IntD),
            _ => Err(InvalidValueError {
                value: number,
                field: "Interrupt pin number",
            }),
        }
    }
}

/// The additional headers of a general PCI device (header type 0x00)
#[derive(Debug, Clone, Copy)]
pub struct PciGeneralDeviceHeader {
    function: PciFunction,

    /// Points (TODO: how) to the Card Information Structure.
    pub cardbus_cis_pointer: u32,
    /// For PCI expansion cards, these fields are used to uniquely identify the board 
    /// (as opposed to [`device_code`], which identifies the PCI controller)
    /// 
    /// [`device_code`]: PciHeader::device_code
    pub subsystem_device_code: PciDeviceId,
    /// A field indicating the physical address of expansion ROM.
    /// See the [PCI spec 2.2] section 6.2.5.2 for more info.
    /// 
    /// [PCI spec 2.2]: https://ics.uci.edu/~harris/ics216/pci/PCI_22.pdf
    pub expansion_rom_base_address: u32,
    /// A pointer into this device's configuration space, pointing to a linked list of the device's capabilities
    pub capabilities_pointer: Option<u8>,

    /// How often the device needs to access the PCI bus in 0.25 microsecond units
    pub max_latency: u8,
    /// The burst period the device needs in 0.25 microsecond units, assuming a 33mhz clock rate
    pub min_grant: u8,
    /// Specifies which PCI interrupt pin the device uses.
    /// A value of 1 means `INTA#`, 2 means `INTB#`, 3 means `INTC#`, 4 means `INTD#`,
    /// and 0 means the device does not use any of the interrupt pins.
    pub interrupt_pin: InterruptPin,
    /// Specifies which input of the interrupt controller the device is connected to.
    /// On x86, this corresponds to the PIC IRQ numbers, but not the IO/APIC interrupt numbers.
    pub interrupt_line: u8,
}

impl PciGeneralDeviceHeader {
    /// Constructs a [`PciGeneralDeviceHeader`] from the PCI registers which make it up.
    /// Takes all registers including registers common to all devices,
    /// because the value of these registers can affect the parsing of the general device specific registers.
    fn from_registers(
        registers: [u32; 0x11],
        function: &PciFunction,
    ) -> Result<Self, InvalidValueError> {
        let cardbus_cis_pointer = registers[10];
        let (subsystem_vendor_id, subsystem_device_id) = split_to_u16(registers[11]);
        let expansion_rom_base_address = registers[12];
        // Capabilities pointer is only valid if bit 4 of the status register is set
        let capabilities_pointer = if registers[1] & (1 << 20) != 0 {
            Some(split_to_u8(registers[13]).0)
        } else {
            None
        };

        let (interrupt_line, interrupt_pin, min_grant, max_latency) = split_to_u8(registers[15]);

        Ok(Self {
            function: *function,
            cardbus_cis_pointer,
            subsystem_device_code: PciDeviceId {
                vendor: subsystem_vendor_id,
                device: subsystem_device_id,
            },
            expansion_rom_base_address,
            capabilities_pointer,
            max_latency,
            min_grant,
            interrupt_pin: InterruptPin::from_pin_number(interrupt_pin)?,
            interrupt_line,
        })
    }

    /// Gets the [`Bar`] with this number. `bar_number` is an offset into the devices BARs, not registers.
    /// For example, if `bar_number` is 0, this corresponds with the PCI register with byte offset 0x10.
    ///
    /// # Panics
    /// If `bar_number` is greater than 5, as this would be past the end of the devices list of BARs.
    ///
    /// # Safety
    /// `bar_number` must be the offset of a BAR which really exists,
    /// and must not point to the second half of a 64-bit BAR.
    /// This can be verified by checking [`class_code`][PciHeader::class_code].
    pub unsafe fn bar<'a>(&self, function: &'a PciMappedFunction, bar_number: u8) -> Bar<'a> {
        if bar_number > 5 {
            panic!("bar_number too high");
        }

        // SAFETY: the caller guarantees that this register really is a BAR
        unsafe { Bar::new(&function.registers, 4 + bar_number) }
    }
}

/// The additional headers of a PCI to PCI bridge (header type 0x01)
#[derive(Debug, Clone, Copy)]
pub struct PciToPciBridgeHeader {
    /// Which PCI function this header is for
    function: PciFunction,
    pub secondary_latency_timer: u8,
    /// The highest bus number of any bus which is downstream of this bridge.
    /// This controls how packets are routed on the PCI bus - any packets with a bus number between
    /// [`secondary_bus_number`][PciToPciBridgeHeader::secondary_bus_number] and [`subordinate_bus_number`][PciToPciBridgeHeader::subordinate_bus_number]
    /// will be routed across this bridge.
    pub subordinate_bus_number: u8,
    /// The bus number of the bus which this device is a bridge to
    pub secondary_bus_number: u8,
    /// The bus number of the bus which this device is a bridge from
    pub primary_bus_number: u8,

    /// The status register of the secondary bus
    pub secondary_status: StatusRegister,
    pub io_limit: u32,
    pub io_base: u32,

    pub memory_limit: u16,
    pub memory_base: u16,
    pub prefetchable_memory_limit: u64,
    pub prefetchable_memory_base: u64,

    pub capabilities_pointer: Option<u8>,
    pub expansion_rom_base_address: u32,
    pub bridge_control: u16,
    /// Specifies which PCI interrupt pin the device uses.
    /// A value of 1 means `INTA#`, 2 means `INTB#`, 3 means `INTC#`, 4 means `INTD#`,
    /// and 0 means the device does not use any of the interrupt pins.
    pub interrupt_pin: InterruptPin,
    /// Specifies which input of the interrupt controller the device is connected to.
    /// On x86, this corresponds to the PIC IRQ numbers, but not the IO/APIC interrupt numbers.
    pub interrupt_line: u8,
}

impl PciToPciBridgeHeader {
    /// Constructs a [`PciGeneralDeviceHeader`] from the PCI registers which make it up.
    /// Takes all registers including registers common to all devices,
    /// because the value of these registers can affect the parsing of the general device specific registers.
    fn from_registers(
        registers: [u32; 0x11],
        function: &PciFunction,
    ) -> Result<Self, InvalidValueError> {
        let (
            primary_bus_number,
            secondary_bus_number,
            subordinate_bus_number,
            secondary_latency_timer,
        ) = split_to_u8(registers[6]);

        let (_, secondary_status) = split_to_u16(registers[7]);
        let (io_base_lower, io_limit_lower, _, _) = split_to_u8(registers[7]);
        let (memory_base, memory_limit) = split_to_u16(registers[8]);

        let (prefetchable_memory_base_lower, prefetchable_memory_limit_lower) =
            split_to_u16(registers[9]);
        let prefetchable_memory_base_upper = registers[10];
        let prefetchable_memory_limit_upper = registers[11];

        let (io_base_upper, io_limit_upper) = split_to_u16(registers[12]);

        // Capabilities pointer is only valid if bit 4 of the status register is set
        let capabilities_pointer = if registers[1] & (1 << 20) != 0 {
            Some(split_to_u8(registers[13]).0)
        } else {
            None
        };

        let expansion_rom_base_address = registers[14];
        let (_, bridge_control) = split_to_u16(registers[15]);
        let (interrupt_line, interrupt_pin, _, _) = split_to_u8(registers[15]);

        let io_limit = (io_limit_upper as u32) << 8 & (io_limit_lower as u32);
        let io_base = (io_base_upper as u32) << 8 & (io_base_lower as u32);
        let prefetchable_memory_base =
            (prefetchable_memory_base_upper as u64) << 16 & (prefetchable_memory_base_lower as u64);
        let prefetchable_memory_limit = (prefetchable_memory_limit_upper as u64) << 16
            & (prefetchable_memory_limit_lower as u64);

        Ok(Self {
            function: *function,

            secondary_latency_timer,
            subordinate_bus_number,
            secondary_bus_number,
            primary_bus_number,
            secondary_status: StatusRegister::from(secondary_status),
            io_limit,
            io_base,
            memory_limit,
            memory_base,
            prefetchable_memory_limit,
            prefetchable_memory_base,
            capabilities_pointer,
            expansion_rom_base_address,
            bridge_control,
            interrupt_pin: InterruptPin::from_pin_number(interrupt_pin)?,
            interrupt_line,
        })
    }

    /// Gets the [`Bar`] with this number. `bar_number` is an offset into the devices BARs, not registers.
    /// For example, if `bar_number` is 0, this corresponds with the PCI register with byte offset 0x10.
    ///
    /// # Panics
    /// If `bar_number` is greater than 1, as this would be past the end of the devices list of BARs.
    ///
    /// # Safety
    /// `bar_number` must be the offset of a BAR which really exists,
    /// and must not point to the second half of a 64-bit BAR.
    /// This can be verified by checking [`class_code`][PciHeader::class_code].
    pub unsafe fn bar<'a>(&self, function: &'a mut PciMappedFunction, bar_number: u8) -> Bar<'a> {
        if bar_number > 1 {
            panic!("bar_number too high");
        }

        // SAFETY: the caller guarantees that this register really is a BAR
        unsafe { Bar::new(&function.registers, 4 + bar_number) }
    }
}

/// A parsed set of values which are not common to each device.
/// The parsing is based on the value of the header type field.
#[derive(Debug, Clone, Copy)]
pub enum HeaderType {
    /// A general PCI device
    GeneralDevice(PciGeneralDeviceHeader),
    /// A PCI-to-PCI bridge, which links two PCI buses together to make other buses accessible from the main bus
    PciToPciBridge(PciToPciBridgeHeader),
    /// A PCI-to-CardBus bridge
    PciToCardbusBridge(),
}

/// The registers which are common to every PCI device
#[derive(Debug, Clone, Copy)]
pub struct PciHeader {
    /// The [`PciDeviceId`] of the device, which uniquely identifies the type of PCI device.
    pub device_code: PciDeviceId,
    /// The status of events related to this device
    pub status: StatusRegister,
    /// Configuration data about the device's connection to the PCI bus
    pub command: CommandRegister,

    /// The type of function the device performs
    pub class_code: ClassCode,
    /// The device's revision ID, allocated by the vendor
    pub revision_id: u8,

    /// Represents status of build in self test
    pub bist: u8,

    /// The latency timer in units of PCI bus clock cycles
    pub latency_timer: u8,
    /// The size of the cache line in units of 4 bytes
    pub cache_line_size: u8,

    /// Whether the device is multifunction (has devices on functions > 0)
    pub is_multifunction: bool,
    /// The header type, containing more specific fields
    pub header_type: HeaderType,
}



impl PciHeader {
    /// Constructs a [`PciHeader`] from the PCI registers which make it up.
    /// Returns [`None`] if there is no device connected, checked for by whether the vendor field is `0xffff`
    pub fn from_registers(
        registers: [u32; 0x11],
        function: &PciFunction,
    ) -> Result<Option<Self>, InvalidValueError> {
        let (vendor, device) = split_to_u16(registers[0]);
        let (command, status) = split_to_u16(registers[1]);
        let (revision_id, prog_if, subclass, class_code) = split_to_u8(registers[2]);
        let (cache_line_size, latency_timer, header_type, bist) = split_to_u8(registers[3]);

        if vendor == 0xffff {
            return Ok(None);
        }

        Ok(Some(Self {
            device_code: PciDeviceId { vendor, device },

            status: status.into(),
            command: command.into(),

            class_code: ClassCode::new(class_code, subclass, prog_if)?,
            revision_id,

            bist,
            latency_timer,
            cache_line_size,

            is_multifunction: header_type & 0x80 != 0,
            header_type: match header_type & 0x4f {
                0 => HeaderType::GeneralDevice(PciGeneralDeviceHeader::from_registers(
                    registers, function,
                )?),
                1 => HeaderType::PciToPciBridge(PciToPciBridgeHeader::from_registers(
                    registers, function,
                )?),
                2 => HeaderType::PciToCardbusBridge(),
                t => panic!("Invalid header type 0x{t:x}"),
            },
        }))
    }
}
