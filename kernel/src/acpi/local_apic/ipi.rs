//! Data structures used to trigger an _Inter-Processor Interrupt_ (IPI)
//!
//! The key here is the [`InterruptCommandRegister`] of the LAPIC - writing to this register triggers an IPI.

use crate::util::bitfield_enum::bitfield_enum;

bitfield_enum!(
    #[bitfield_enum(u32)]
    /// How an IPI is delivered.
    ///
    /// For more info, see the [Intel 64 and IA-32 Architectures Software Developer’s Manual] volume 3 section 11.6
    ///
    /// [Intel 64 and IA-32 Architectures Software Developer’s Manual]: https://cdrdv2.intel.com/v1/dl/getContent/671200
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[allow(dead_code)]
    pub enum DeliveryMode {
        #[value(0)]
        /// Deliver the signal to all the agents listed in the destination. The Trigger Mode for
        /// fixed delivery mode can be edge or level.
        Fixed,
        #[value(1)]
        /// Deliver the signal to the agent that is executing at the lowest priority of all
        /// agents listed in the destination field. The trigger mode can be edge or level.
        LowestPriority,
        #[value(2)]
        /// The delivery mode is edge only. For systems that rely on SMI semantics, the vector field is ignored
        /// but must be programmed to all zeroes for future compatibility.
        SystemManagementInterrupt,
        #[value(4)]
        /// Deliver the signal to all the agents listed in the destination field. The vector field is ignored.
        /// NMI is an edge triggered interrupt regardless of the Trigger Mode Setting.
        NonMaskableInterrupt,
        #[value(5)]
        /// Deliver this signal to all the agents listed in the destination field. The vector field is ignored.
        /// INIT is an edge triggered interrupt regardless of the Trigger Mode Setting.
        Init,
        #[value(6)]
        /// Sends a special “start-up” IPI (called a SIPI) to the target processor or processors.
        /// The vector typically points to a start-up routine that is part of the BIOS boot-strap code.
        ///
        /// For more info, see the [Intel 64 and IA-32 Architectures Software Developer’s Manual] volume 3 section 9.4
        ///
        /// [Intel 64 and IA-32 Architectures Software Developer’s Manual]: https://cdrdv2.intel.com/v1/dl/getContent/671200
        StartUp,
    }
);

bitfield_enum!(
    #[bitfield_enum(u32)]
    /// How to interpret the value of the destination field
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum DestinationMode {
        #[value(0)]
        /// The destination is the APIC ID of the core to send the interrupt to
        Physical,
        #[value(1)]
        /// The destination is a message destination address which is used to select which core(s) to send the interrupt to
        Logical,
    }
);

bitfield_enum!(
    #[bitfield_enum(u32)]
    /// Whether the IPI is edge or level triggered (only for [`Init`] level de-assert)
    ///
    /// [`Init`]: DeliveryMode::Init
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TriggerMode {
        #[value(0)]
        /// Edge triggered
        Edge,
        #[value(1)]
        /// Level triggered
        Level,
    }
);

bitfield_enum!(
    #[bitfield_enum(u32)]
    /// Indicates whether a shorthand notation is used to specify the destination of the interrupt and,
    /// if so, which shorthand is used. Destination shorthands are used in place of the destination field.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum DestinationShorthand {
        #[value(0)]
        /// No shorthand - the destination is read from the destination field
        NoShorthand,
        #[value(1)]
        /// The destination field is ignored and the interrupt is delivered to this core
        ThisCore,
        #[value(2)]
        /// The destination field is ignored and the interrupt is delivered to all cores, including this one
        All,
        #[value(3)]
        /// The destination field is ignored and the interrupt is delivered to all cores except for this one.
        AllApartFromThisCore,
    }
);

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
