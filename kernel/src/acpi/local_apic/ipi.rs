//! Data structures used to trigger an _Inter-Processor Interrupt_ (IPI)
//! 
//! The key here is the [`InterruptCommandRegister`] of the LAPIC - writing to this register triggers an IPI.

/// How an IPI is delivered.
///
/// For more info, see the [Intel 64 and IA-32 Architectures Software Developer’s Manual] volume 3 section 11.6
///
/// [Intel 64 and IA-32 Architectures Software Developer’s Manual]: https://cdrdv2.intel.com/v1/dl/getContent/671200
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum DeliveryMode {
    /// Deliver the signal to all the agents listed in the destination. The Trigger Mode for
    /// fixed delivery mode can be edge or level.
    Fixed,
    /// Deliver the signal to the agent that is executing at the lowest priority of all
    /// agents listed in the destination field. The trigger mode can be edge or level.
    LowestPriority,
    /// The delivery mode is edge only. For systems that rely on SMI semantics, the [`vector`]` field is ignored
    /// but must be programmed to all zeroes for future compatibility.
    ///
    /// [`vector`]: X64MsiAddress::vector
    SystemManagementInterrupt,
    /// Deliver the signal to all the agents listed in the destination field. [`vector`] is ignored.
    /// NMI is an edge triggered interrupt regardless of the Trigger Mode Setting.
    ///
    /// [`vector`]: X64MsiAddress::vector
    NonMaskableInterrupt,
    /// Deliver this signal to all the agents listed in the destination field. [`vector`] is ignored.
    /// INIT is an edge triggered interrupt regardless of the Trigger Mode Setting.
    ///
    /// [`vector`]: X64MsiAddress::vector
    Init,
    /// Sends a special “start-up” IPI (called a SIPI) to the target processor or processors.
    /// The vector typically points to a start-up routine that is part of the BIOS boot-strap code.
    ///
    /// For more info, see the [Intel 64 and IA-32 Architectures Software Developer’s Manual] volume 3 section 9.4
    ///
    /// [Intel 64 and IA-32 Architectures Software Developer’s Manual]: https://cdrdv2.intel.com/v1/dl/getContent/671200
    StartUp,
}

impl DeliveryMode {
    /// Parses the delivery mode from its bit representation
    const fn from_bits(bits: u32) -> Self {
        match bits {
            0 => Self::Fixed,
            1 => Self::LowestPriority,
            2 => Self::SystemManagementInterrupt,
            4 => Self::NonMaskableInterrupt,
            5 => Self::Init,
            6 => Self::StartUp,

            _ => panic!("Unknown or reserved delivery mode"),
        }
    }

    /// Converts the delivery mode into its bit representation
    const fn into_bits(self) -> u32 {
        match self {
            DeliveryMode::Fixed => 0,
            DeliveryMode::LowestPriority => 1,
            DeliveryMode::SystemManagementInterrupt => 2,
            DeliveryMode::NonMaskableInterrupt => 4,
            DeliveryMode::Init => 5,
            DeliveryMode::StartUp => 6,
        }
    }
}

/// How to interpret the value of the destination field
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestinationMode {
    /// The destination is the APIC ID of the core to send the interrupt to
    Physical,
    /// The destination is a message destination address which is used to select which core(s) to send the interrupt to
    Logical,
}

impl DestinationMode {
    /// Parses the destination mode from its bit representation
    const fn from_bits(bits: u32) -> Self {
        match bits {
            0 => Self::Physical,
            1 => Self::Logical,

            _ => unreachable!(),
        }
    }

    /// Converts the destination mode into its bit representation
    const fn into_bits(self) -> u32 {
        match self {
            Self::Physical => 0,
            Self::Logical => 1,
        }
    }
}

/// Whether the IPI is edge or level triggered (only for [`Init`] level de-assert)
///
/// [`Init`]: DeliveryMode::Init
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerMode {
    /// Edge triggered
    Edge,
    /// Level triggered
    Level,
}

impl TriggerMode {
    /// Parses the destination mode from its bit representation
    const fn from_bits(bits: u32) -> Self {
        match bits {
            0 => Self::Edge,
            1 => Self::Level,

            _ => unreachable!(),
        }
    }

    /// Converts the destination mode into its bit representation
    const fn into_bits(self) -> u32 {
        match self {
            Self::Edge => 0,
            Self::Level => 1,
        }
    }
}

/// Indicates whether a shorthand notation is used to specify the destination of the interrupt and,
/// if so, which shorthand is used. Destination shorthands are used in place of the destination field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestinationShorthand {
    /// No shorthand - the destination is read from the destination field
    NoShorthand,
    /// The destination field is ignored and the interrupt is delivered to this core
    ThisCore,
    /// The destination field is ignored and the interrupt is delivered to all cores, including this one
    All,
    /// The destination field is ignored and the interrupt is delivered to all cores except for this one.
    AllApartFromThisCore,
}

impl DestinationShorthand {
    /// Parses the destination mode from its bit representation
    const fn from_bits(bits: u32) -> Self {
        match bits {
            0 => Self::NoShorthand,
            1 => Self::ThisCore,
            2 => Self::All,
            3 => Self::AllApartFromThisCore,

            _ => unreachable!(),
        }
    }

    /// Converts the destination mode into its bit representation
    const fn into_bits(self) -> u32 {
        match self {
            Self::NoShorthand => 0,
            Self::ThisCore => 1,
            Self::All => 2,
            Self::AllApartFromThisCore => 3,
        }
    }
}

/// A value to write to the interrupt command register to trigger an IPI. If the [`destination_shorthand`] is [`NoShorthand`],
/// the destination must be written as well.
///
/// For more info, see the [Intel 64 and IA-32 Architectures Software Developer’s Manual] volume 3 section 11.6
///
/// [`destination_shorthand`]: InterruptCommandRegister::destination_shorthand
/// [`NoShorthand`]: DestinationShorthand::NoShorthand
/// [Intel 64 and IA-32 Architectures Software Developer’s Manual]: https://cdrdv2.intel.com/v1/dl/getContent/671200
#[bitfield(u32)]
pub struct InterruptCommandRegister {
    /// The vector of the interrupt being sent
    pub vector_number: u8,

    /// The type of IPI to send
    #[bits(3)]
    pub delivery_mode: DeliveryMode,

    /// How to interpret the destination field
    #[bits(1)]
    pub destination_mode: DestinationMode,

    /// Whether the interrupt has been delivered yet (read only)
    pub delivered: bool,

    #[bits(1)]
    _reserved: (),

    /// For the [`Init`] level de-assert delivery mode this flag must be set to 0; for all other delivery modes it must be set to 1
    ///
    /// [`Init`]: DeliveryMode::Init
    #[bits(default = true)]
    pub level: bool,

    /// The trigger mode when using the INIT level de-assert delivery mode.
    /// It is ignored for all other delivery modes.
    #[bits(1)]
    pub is_init_level_de_assert: TriggerMode,

    #[bits(2)]
    _reserved: (),

    /// Indicates whether a shorthand notation is used to specify the destination of the interrupt and, if so, which shorthand is used.
    #[bits(2)]
    pub destination_shorthand: DestinationShorthand,

    #[bits(12)]
    _reserved: (),
}