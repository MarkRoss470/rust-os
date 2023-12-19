//! The [`LvtRegisters`] struct for parsing the _Local Vector Table_ registers

use crate::acpi::{InterruptActiveState, InterruptTriggerMode};

/// The delivery mode for an interrupt in the local vector table
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LvtDeliveryMode {
    /// The interrupt is delivered as normal to the specified field
    Fixed,
    /// The interrupt is delivered as a system management interrupt
    SystemManagement,
    /// The interrupt is delivered as a non-maskable interrupt.
    /// When using this delivery mode, the vector field should be set to 00H for future compatibility
    NonMaskable,
    /// Causes the processor to respond to the interrupt as if the interrupt originated
    /// in an externally connected (8259A-compatible) interrupt controller.
    /// A special INTA bus cycle corresponding to ExtINT, is routed to the external
    /// controller. The external controller is expected to supply the vector information.
    /// The APIC architecture supports only one ExtINT source in a system,
    /// usually contained in the compatibility bridge. Only one processor in the
    /// system should have an LVT entry configured to use the ExtINT delivery
    /// mode. Not supported for the LVT CMCI register, the LVT thermal monitor
    /// register, or the LVT performance counter register.
    External,
    /// Delivers an INIT request to the processor core, which causes the processor
    /// to perform an INIT. When using this delivery mode, the vector field should
    /// be set to 00H for future compatibility. Not supported for the LVT CMCI register,
    /// the LVT thermal monitor register, or the LVT performance counter register.
    Init,
}

impl LvtDeliveryMode {
    /// Parses the delivery mode from its bit representation
    const fn from_bits(bits: u32) -> Self {
        match bits {
            0 => Self::Fixed,
            2 => Self::SystemManagement,
            4 => Self::NonMaskable,
            7 => Self::External,
            5 => Self::Init,

            _ => panic!("Unknown or reserved LVT delivery mode"),
        }
    }

    /// Converts the delivery mode into its bit representation
    const fn into_bits(self) -> u32 {
        match self {
            Self::Fixed => 0,
            Self::SystemManagement => 2,
            Self::NonMaskable => 4,
            Self::External => 7,
            Self::Init => 5,
        }
    }
}

/// The mode of the local APIC timer
#[derive(Debug)]
pub enum TimerMode {
    /// The timer counts down once and then stops
    OneShot,
    /// The timer counts down and then starts again
    Periodic,
    /// Only available if CPUID.01H:ECX.TSC_Deadline[bit 24] = 1
    Deadline,
}

impl TimerMode {
    /// Constructs a [`TimerMode`] from its bit representation
    const fn from_bits(bits: u32) -> Self {
        match bits {
            0 => Self::OneShot,
            1 => Self::Periodic,
            2 => Self::Deadline,
            _ => panic!("Unknown timer mode"),
        }
    }

    /// Converts a [`TimerMode`] into its bit representation
    const fn into_bits(self) -> u32 {
        match self {
            TimerMode::OneShot => 0,
            TimerMode::Periodic => 1,
            TimerMode::Deadline => 2,
        }
    }
}

/// The format of the register controlling an interrupt in the _Local Vector Table_ (LVT).
/// These are interrupts which occur within a core.
///
/// This struct is the format for the following registers:
///
/// * LVT Corrected Machine Check Interrupt (CMCI) Register
/// * LVT Timer Register
/// * LVT Thermal Sensor Register 2
/// * LVT Performance Monitoring Counters Register 3
/// * LVT LINT0 Register
/// * LVT LINT1 Register
/// * LVT Error Register
#[bitfield(u32)]
pub struct LvtRegisters {
    /// The interrupt vector this interrupt is assigned to
    pub vector_number: u8,

    /// The delivery mode of the interrupt (all except timer and error registers)
    #[bits(3)]
    pub delivery_mode: LvtDeliveryMode,

    #[bits(1)]
    _reserved: (),

    /// Whether the interrupt is currently pending
    pub is_pending: bool,

    /// Whether the interrupt is active-high or active-low (LINT0 and LINT1 registers only)
    #[bits(1, from = InterruptActiveState::from_bits_u32, into = InterruptActiveState::into_bits_u32)]
    pub input_pin_polarity: InterruptActiveState,

    /// Whether the interrupt is currently being processed (read only, LINT0 and LINT1 registers only)
    /// 
    /// For fixed mode, level-triggered interrupts; this flag is set when the local APIC accepts the
    /// interrupt for servicing and is reset when an EOI command is received from the processor. The
    /// meaning of this flag is undefined for edge-triggered interrupts and other delivery modes.
    pub remote_irr: bool,

    /// Whether the interrupt is edge-triggered or level-triggered (LINT0 and LINT1 registers only)
    #[bits(1, from = InterruptTriggerMode::from_bits_u32, into = InterruptTriggerMode::into_bits_u32)]
    pub is_level_triggered: InterruptTriggerMode,

    /// Whether the interrupt is masked - if this bit is set, the core will not receive this interrupt.
    pub masked: bool,

    /// The timer mode (timer register only)
    #[bits(2)]
    pub timer_mode: TimerMode,

    #[bits(13)]
    _reserved: (),
}
