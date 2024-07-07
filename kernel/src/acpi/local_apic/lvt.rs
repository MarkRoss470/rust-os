//! The [`LvtRegisters`] struct for parsing the _Local Vector Table_ registers

use crate::{
    acpi::{InterruptActiveState, InterruptTriggerMode},
    util::bitfield_enum::bitfield_enum,
};

bitfield_enum!(
    #[bitfield_enum(u32)]
    /// The delivery mode for an interrupt in the local vector table
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum LvtDeliveryMode {
        #[value(0)]
        /// The interrupt is delivered as normal to the specified field
        Fixed,
        #[value(2)]
        /// The interrupt is delivered as a system management interrupt
        SystemManagement,
        #[value(4)]
        /// The interrupt is delivered as a non-maskable interrupt.
        /// When using this delivery mode, the vector field should be set to 00H for future compatibility
        NonMaskable,
        #[value(7)]
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
        #[value(5)]
        /// Delivers an INIT request to the processor core, which causes the processor
        /// to perform an INIT. When using this delivery mode, the vector field should
        /// be set to 00H for future compatibility. Not supported for the LVT CMCI register,
        /// the LVT thermal monitor register, or the LVT performance counter register.
        Init,
    }
);

bitfield_enum!(
    #[bitfield_enum(u32)]
    /// The mode of the local APIC timer
    #[derive(Debug)]
    pub enum TimerMode {
        #[value(0)]
        /// The timer counts down once and then stops
        OneShot,
        #[value(1)]
        /// The timer counts down and then starts again
        Periodic,
        #[value(2)]
        /// Only available if CPUID.01H:ECX.TSC_Deadline[bit 24] = 1
        Deadline,
    }
);

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
