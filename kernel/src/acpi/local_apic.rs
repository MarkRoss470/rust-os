//! Contains the [`LocalApicRegisters`] struct

use x86_64::{
    structures::paging::{frame::PhysFrameRange, PhysFrame},
    PhysAddr,
};

use crate::{global_state::KERNEL_STATE, println};

#[bitfield(u32)]
struct InterruptCommandRegister {
    vector_number: u8,
    #[bits(3)]
    destination_mode: u8,
    is_logical_destination: bool,
    delivery_status: bool,

    #[bits(1)]
    _reserved: (),

    is_not_init_level_de_assert: bool,
    is_init_level_de_assert: bool,

    #[bits(2)]
    _reserved: (),

    #[bits(2)]
    destination_type: u8,

    #[bits(12)]
    _reserved: (),
}

#[bitfield(u32)]
struct LvtRegisters {
    vector_number: u8,
    /// OsDev wiki says: _100b if NMI_
    /// TODO: What does that mean?
    #[bits(3)]
    is_nmi: u8,

    #[bits(1)]
    _reserved: (),
    /// Whether the interrupt is pending
    is_pending: bool,
    /// Whether the interrupt is active-high or active-low
    /// TODO: enum-ify
    is_active_low: bool,
    remote_irr: bool,
    /// Whether the interrupt is edge-triggered or level-triggered
    /// TODO: enum-ify
    is_level_triggered: bool,
    masked: bool,

    #[bits(15)]
    _reserved: (),
}

/// The mode of the local APIC timer
#[derive(Debug)]
enum TimerMode {
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

#[bitfield(u32)]
struct LvtTimerRegisters {
    vector_number: u8,
    #[bits(4)]
    _reserved: (),
    /// Whether the interrupt is pending
    is_pending: bool,

    #[bits(3)]
    _reserved: (),
    masked: bool,

    #[bits(2)]
    timer_mode: TimerMode,

    #[bits(13)]
    _reserved: (),
}

#[bitfield(u32)]
struct TaskPriorityRegister {
    #[bits(4)]
    sub_class: u8,
    #[bits(4)]
    class: u8,

    #[bits(24)]
    _reserved: (),
}

/// The registers of a local programmable interrupt controller
#[derive(Debug)]
pub struct LocalApicRegisters(*mut u32);

impl LocalApicRegisters {
    /// The offset of the lapic_id field
    const LAPIC_ID_OFFSET: usize = 0x020;
    /// The offset of the lapic_version field
    const LAPIC_VERSION_OFFSET: usize = 0x030;
    /// The offset of the task_priority_reg field
    const TASK_PRIORITY_OFFSET: usize = 0x080;
    /// The offset of the arbitration_priority field
    const ARBITRATION_PRIORITY_OFFSET: usize = 0x090;
    /// The offset of the processor_priority field
    const PROCESSOR_PRIORITY_OFFSET: usize = 0x0A0;
    /// The offset of the EOI field. Writing a 0 to this field signals the end of an interrupt handler.
    const EOI_OFFSET: usize = 0x0B0;
    /// The offset of the remote_read field
    const REMOTE_READ_OFFSET: usize = 0x0C0;
    /// The offset of the logical_destination field
    const LOGICAL_DESTINATION_OFFSET: usize = 0x0D0;
    /// The offset of the destination_format field
    const DESTINATION_FORMAT_OFFSET: usize = 0x0E0;
    /// The offset of the spurious_interrupt_vector field
    const SPURIOUS_INTERRUPT_VECTOR_OFFSET: usize = 0x0F0;
    /// The offset of the in_service field
    const IN_SERVICE_OFFSET: usize = 0x100;
    /// The offset of the trigger_mode field
    const TRIGGER_MODE_OFFSET: usize = 0x180;
    /// The offset of the interrupt_request field
    const INTERRUPT_REQUEST_OFFSET: usize = 0x200;
    /// The offset of the error_status field
    const ERROR_STATUS_OFFSET: usize = 0x280;
    /// The offset of the lvt_corrected_machine_check_interrupt_cmci field
    const LVT_CORRECTED_MACHINE_CHECK_INTERRUPT_CMCI_OFFSET: usize = 0x2F0;
    /// The offset of the interrupt_command field
    const INTERRUPT_COMMAND_OFFSET: usize = 0x300;
    /// The offset of the lvt_timer field
    const LVT_TIMER_OFFSET: usize = 0x320;
    /// The offset of the lvt_thermal_sensor field
    const LVT_THERMAL_SENSOR_OFFSET: usize = 0x330;
    /// The offset of the lvt_performance_monitoring_counters field
    const LVT_PERFORMANCE_MONITORING_COUNTERS_OFFSET: usize = 0x340;
    /// The offset of the lvt_lint0 field
    const LVT_LINT0_OFFSET: usize = 0x350;
    /// The offset of the lvt_lint1 field
    const LVT_LINT1_OFFSET: usize = 0x360;
    /// The offset of the lvt_error field
    const LVT_ERROR_OFFSET: usize = 0x370;
    /// The offset of the initial_count field
    const INITIAL_COUNT_OFFSET: usize = 0x380;
    /// The offset of the current_count field
    const CURRENT_COUNT_OFFSET: usize = 0x390;
    /// The offset of the divide_configuration field
    const DIVIDE_CONFIGURATION_OFFSET: usize = 0x3E0;

    /// Constructs a new [`LocalApicRegisters`] from the given register block.
    ///
    /// # Safety
    /// The pointer must point to the registers of a local APIC.
    /// No other code is allowed to write to read or write to these registers,
    /// so no other instances of [`LocalApicRegisters`] are allowed to exist on this core.
    /// Each core has a different APIC but they are all mapped to the same physical address,
    /// so it is okay to have multiple instances with the same physical address as long as
    /// they always stay on different cores.
    pub unsafe fn new(addr: PhysAddr) -> Self {
        let start = PhysFrame::containing_address(addr);
        let frames = PhysFrameRange {
            start,
            end: start + 2, // Add 2 in case the registers are mapped across a page boundary
        };

        let virt_addr = KERNEL_STATE
            .physical_memory_accessor
            .lock()
            .map_frames(frames);

        let virt_addr = virt_addr.start.start_address().as_u64() + (addr.as_u64() & 4096);
        Self(virt_addr as _)
    }

    /// Reads the register at the given byte offset
    ///
    /// # Safety
    /// The read may have side effects (TODO: do any of them? Maybe this method is safe).
    /// It is the caller's responsibility to ensure these do not cause undefined behaviour.
    pub unsafe fn read_reg(&self, offset: usize) -> u32 {
        // Check that the offset is 16-byte aligned
        assert_eq!(offset % 16, 0);
        // Check that the offset is in-bounds
        assert!(offset <= 0x3f0);

        // SAFETY: self.0 is guaranteed to point to local APIC registers
        // and offset is less than the length of the registers
        unsafe { core::ptr::read_volatile(self.0.byte_offset(offset as _)) }
    }

    /// Writes a value to the register at the given byte offset
    ///
    /// # Safety
    /// The write may have side effects.
    /// It is the caller's responsibility to ensure these do not cause undefined behaviour.
    pub unsafe fn write_reg(&mut self, offset: usize, value: u32) {
        // Check that the offset is 16-byte aligned
        assert_eq!(offset % 16, 0);
        // Check that the offset is in-bounds
        assert!(offset <= 0x3f0);

        // SAFETY: self.0 is guaranteed to point to local APIC registers
        // and offset is less than the length of the registers
        unsafe { core::ptr::write_volatile(self.0.byte_offset(offset as _), value) }
    }

    /// Sends an EOI to the APIC
    ///
    /// # Safety
    /// This method may only be called from an interrupt handler which this APIC signalled,
    /// and may only be called once per interrupt.
    pub unsafe fn notify_end_of_interrupt(&mut self) {
        // SAFETY: This register is the EOI register
        // TODO: un-magic-number this offset
        unsafe { self.write_reg(Self::EOI_OFFSET, 0) }
    }

    /// Maps a division value for the local timer to the value to be written to the
    /// `divide_configuration` register.
    const fn create_divide_value(division: u8) -> u32 {
        match division {
            1 => 0b1011,
            2 => 0b0000,
            4 => 0b0001,
            8 => 0b0010,
            16 => 0b0011,
            32 => 0b1000,
            64 => 0b1001,
            128 => 0b1010,
            _ => panic!("Invalid division base"),
        }
    }

    /// Maps a value from the `divide_configuration` register to the divisor it represents.
    const fn parse_divide_value(register_value: u32) -> u8 {
        match register_value {
            0b1011 => 1,
            0b0000 => 2,
            0b0001 => 4,
            0b0010 => 8,
            0b0011 => 16,
            0b1000 => 32,
            0b1001 => 64,
            0b1010 => 128,
            _ => panic!("Invalid division base"),
        }
    }

    /// Enables the local interrupt timer.
    /// The interrupts will target the given interrupt vector.
    ///
    /// # Safety
    /// The CPU must be set up to receive timer interrupts at the given vector.
    ///
    /// TODO: allow specifying frequency
    pub unsafe fn enable_timer(&mut self, vector: u8) {
        // Set up the timer interrupt to target the given vector
        // and occur periodically rather than just once.

        // SAFETY: This has no side effects until the initial_count register is set.
        unsafe {
            self.write_reg(
                Self::LVT_TIMER_OFFSET,
                LvtTimerRegisters::new()
                    .with_masked(false)
                    .with_vector_number(vector)
                    .with_timer_mode(TimerMode::Periodic)
                    .into(),
            );
        }

        // Set the divisor the timer uses

        // SAFETY: This has no side effects until the initial_count register is set.
        unsafe {
            self.write_reg(
                Self::DIVIDE_CONFIGURATION_OFFSET,
                Self::create_divide_value(128),
            );
        }

        // SAFETY: This will start the timer.
        // It is the caller's responsibility that the interrupts are received properly.
        unsafe {
            // Number chosen to get about 100 interrupts per second in qemu
            // TODO: calculate this number properly
            self.write_reg(Self::INITIAL_COUNT_OFFSET, 100000);
        }
    }

    /// Starts the APIC, while setting the spurious interrupt vector to the given value.
    ///
    /// # Safety
    /// The CPU must be set up to receive any interrupts generated by the APIC.
    pub unsafe fn enable(&mut self, spurious_interrupt_vector: u8) {
        // SAFETY: This will start the APIC. The safety of this the caller's responsibility
        unsafe {
            self.write_reg(
                Self::SPURIOUS_INTERRUPT_VECTOR_OFFSET,
                0x100 | spurious_interrupt_vector as u32,
            )
        }
    }

    /// Manually sends an interrupt to the core this function is called on.
    ///
    /// # Safety
    /// This function is for debugging purposes only and should not be relied upon for soundness
    /// or any message passing effects.
    pub unsafe fn send_debug_self_interrupt(&mut self, vector_number: u8) {
        // SAFETY: For debugging only, not guaranteed to be sound
        unsafe { self.send_debug_self_interrupt_delayed(vector_number)() }
    }

    /// Returns a function which, when called, manually sends an interrupt to the core on which it is called.
    /// This allows for avoiding deadlocks if the [`LocalApicRegisters`] struct is stored behind a mutex.
    ///
    /// # Safety
    /// The returned closure should be called immediately after it is created.
    /// This function is for debugging purposes only and should not be relied upon for soundness
    /// or any message passing effects.
    pub unsafe fn send_debug_self_interrupt_delayed(&mut self, vector_number: u8) -> impl Fn() {
        let value = InterruptCommandRegister::new()
            .with_vector_number(vector_number)
            .with_destination_mode(0)
            .with_is_logical_destination(true)
            .with_delivery_status(true)
            .with_is_not_init_level_de_assert(false) // Switch these?
            .with_is_init_level_de_assert(true)
            .with_destination_type(1); // Send to self

        // SAFETY: For debugging only, not guaranteed to be sound
        let ptr = unsafe { self.0.byte_add(Self::INTERRUPT_COMMAND_OFFSET) };

        // SAFETY: For debugging only, not guaranteed to be sound
        move || unsafe { core::ptr::write_volatile(ptr, value.into()) }
    }

    /// Prints out the APIC's registers
    #[rustfmt::skip]
    pub fn debug_re(&self) {
        // SAFETY: This register doesn't have read side effects
        unsafe { println!("APIC {{"); }
        // SAFETY: Reading from these registers has no side effects
        unsafe {
            println!("    LAPIC_ID: {}", self.read_reg(Self::LAPIC_ID_OFFSET));
            println!("    LAPIC_VERSION: {}", self.read_reg(Self::LAPIC_VERSION_OFFSET));
            println!("    TASK_PRIORITY: {:?}", TaskPriorityRegister::from(self.read_reg(Self::TASK_PRIORITY_OFFSET)));
            println!("    ARBITRATION_PRIORITY: {}", self.read_reg(Self::ARBITRATION_PRIORITY_OFFSET));
            println!("    PROCESSOR_PRIORITY: {:?}", TaskPriorityRegister::from(self.read_reg(Self::PROCESSOR_PRIORITY_OFFSET)));
            println!("    REMOTE_READ: {}", self.read_reg(Self::REMOTE_READ_OFFSET));
            println!("    LOGICAL_DESTINATION: {}", self.read_reg(Self::LOGICAL_DESTINATION_OFFSET));
            println!("    DESTINATION_FORMAT: {}", self.read_reg(Self::DESTINATION_FORMAT_OFFSET));
            println!("    SPURIOUS_INTERRUPT_VECTOR: {}", self.read_reg(Self::SPURIOUS_INTERRUPT_VECTOR_OFFSET));
            println!("    IN_SERVICE: {}", self.read_reg(Self::IN_SERVICE_OFFSET));
            println!("    TRIGGER_MODE: {}", self.read_reg(Self::TRIGGER_MODE_OFFSET));
            println!("    INTERRUPT_REQUEST: {}", self.read_reg(Self::INTERRUPT_REQUEST_OFFSET));
            println!("    ERROR_STATUS: {}", self.read_reg(Self::ERROR_STATUS_OFFSET));
            println!("    LVT_CORRECTED_MACHINE_CHECK_INTERRUPT_CMCI: {:?}", LvtRegisters::from(self.read_reg(Self::LVT_CORRECTED_MACHINE_CHECK_INTERRUPT_CMCI_OFFSET)));
            println!("    INTERRUPT_COMMAND: {:?}", InterruptCommandRegister::from(self.read_reg(Self::INTERRUPT_COMMAND_OFFSET)));
            println!("    LVT_TIMER: {:?}", LvtTimerRegisters::from(self.read_reg(Self::LVT_TIMER_OFFSET)));
            println!("    LVT_THERMAL_SENSOR: {:?}", LvtRegisters::from(self.read_reg(Self::LVT_THERMAL_SENSOR_OFFSET)));
            println!("    LVT_PERFORMANCE_MONITORING_COUNTERS: {:?}", LvtRegisters::from(self.read_reg(Self::LVT_PERFORMANCE_MONITORING_COUNTERS_OFFSET)));
            println!("    LVT_LINT0: {:?}", LvtRegisters::from(self.read_reg(Self::LVT_LINT0_OFFSET)));
            println!("    LVT_LINT1: {:?}", LvtRegisters::from(self.read_reg(Self::LVT_LINT1_OFFSET)));
            println!("    LVT_ERROR: {:?}", LvtRegisters::from(self.read_reg(Self::LVT_ERROR_OFFSET)));
            println!("    INITIAL_COUNT: {}", self.read_reg(Self::INITIAL_COUNT_OFFSET));
            println!("    CURRENT_COUNT: {}", self.read_reg(Self::CURRENT_COUNT_OFFSET));
            println!("    DIVIDE_CONFIGURATION: {}", self.read_reg(Self::DIVIDE_CONFIGURATION_OFFSET));
        }
        println!("}}");
    }
}

impl LocalApicRegisters {
    /// Reads the LAPIC_ID_OFFSET register
    pub fn lapic_id(&self) -> u32 {
        // SAFETY: This register doesn't have read side effects
        unsafe { self.read_reg(Self::LAPIC_ID_OFFSET) }
    }
    /// Reads the LAPIC_VERSION_OFFSET register
    pub fn lapic_version(&self) -> u32 {
        // SAFETY: This register doesn't have read side effects
        unsafe { self.read_reg(Self::LAPIC_VERSION_OFFSET) }
    }
    /// Reads the TASK_PRIORITY_OFFSET register
    pub fn task_priority(&self) -> u32 {
        // SAFETY: This register doesn't have read side effects
        unsafe { self.read_reg(Self::TASK_PRIORITY_OFFSET) }
    }
    /// Reads the ARBITRATION_PRIORITY_OFFSET register
    pub fn arbitration_priority(&self) -> u32 {
        // SAFETY: This register doesn't have read side effects
        unsafe { self.read_reg(Self::ARBITRATION_PRIORITY_OFFSET) }
    }
    /// Reads the PROCESSOR_PRIORITY_OFFSET register
    pub fn processor_priority(&self) -> u32 {
        // SAFETY: This register doesn't have read side effects
        unsafe { self.read_reg(Self::PROCESSOR_PRIORITY_OFFSET) }
    }
    /// Reads the REMOTE_READ_OFFSET register
    pub fn remote_read(&self) -> u32 {
        // SAFETY: This register doesn't have read side effects
        unsafe { self.read_reg(Self::REMOTE_READ_OFFSET) }
    }
    /// Reads the LOGICAL_DESTINATION_OFFSET register
    pub fn logical_destination(&self) -> u32 {
        // SAFETY: This register doesn't have read side effects
        unsafe { self.read_reg(Self::LOGICAL_DESTINATION_OFFSET) }
    }
    /// Reads the DESTINATION_FORMAT_OFFSET register
    pub fn destination_format(&self) -> u32 {
        // SAFETY: This register doesn't have read side effects
        unsafe { self.read_reg(Self::DESTINATION_FORMAT_OFFSET) }
    }
    /// Reads the SPURIOUS_INTERRUPT_VECTOR_OFFSET register
    pub fn spurious_interrupt_vector(&self) -> u32 {
        // SAFETY: This register doesn't have read side effects
        unsafe { self.read_reg(Self::SPURIOUS_INTERRUPT_VECTOR_OFFSET) }
    }
    /// Reads the IN_SERVICE_OFFSET register
    pub fn in_service(&self) -> u32 {
        // SAFETY: This register doesn't have read side effects
        unsafe { self.read_reg(Self::IN_SERVICE_OFFSET) }
    }
    /// Reads the TRIGGER_MODE_OFFSET register
    pub fn trigger_mode(&self) -> u32 {
        // SAFETY: This register doesn't have read side effects
        unsafe { self.read_reg(Self::TRIGGER_MODE_OFFSET) }
    }
    /// Reads the INTERRUPT_REQUEST_OFFSET register
    pub fn interrupt_request(&self) -> u32 {
        // SAFETY: This register doesn't have read side effects
        unsafe { self.read_reg(Self::INTERRUPT_REQUEST_OFFSET) }
    }
    /// Reads the ERROR_STATUS_OFFSET register
    pub fn error_status(&self) -> u32 {
        // SAFETY: This register doesn't have read side effects
        unsafe { self.read_reg(Self::ERROR_STATUS_OFFSET) }
    }
    /// Reads the LVT_CORRECTED_MACHINE_CHECK_INTERRUPT_CMCI_OFFSET register
    pub fn lvt_corrected_machine_check_interrupt_cmci(&self) -> u32 {
        // SAFETY: This register doesn't have read side effects
        unsafe { self.read_reg(Self::LVT_CORRECTED_MACHINE_CHECK_INTERRUPT_CMCI_OFFSET) }
    }
    /// Reads the INTERRUPT_COMMAND_OFFSET register
    pub fn interrupt_command(&self) -> u32 {
        // SAFETY: This register doesn't have read side effects
        unsafe { self.read_reg(Self::INTERRUPT_COMMAND_OFFSET) }
    }
    /// Reads the LVT_TIMER_OFFSET register
    pub fn lvt_timer(&self) -> u32 {
        // SAFETY: This register doesn't have read side effects
        unsafe { self.read_reg(Self::LVT_TIMER_OFFSET) }
    }
    /// Reads the LVT_THERMAL_SENSOR_OFFSET register
    pub fn lvt_thermal_sensor(&self) -> u32 {
        // SAFETY: This register doesn't have read side effects
        unsafe { self.read_reg(Self::LVT_THERMAL_SENSOR_OFFSET) }
    }
    /// Reads the LVT_PERFORMANCE_MONITORING_COUNTERS_OFFSET register
    pub fn lvt_performance_monitoring_counters(&self) -> u32 {
        // SAFETY: This register doesn't have read side effects
        unsafe { self.read_reg(Self::LVT_PERFORMANCE_MONITORING_COUNTERS_OFFSET) }
    }
    /// Reads the LVT_LINT0_OFFSET register
    pub fn lvt_lint0(&self) -> u32 {
        // SAFETY: This register doesn't have read side effects
        unsafe { self.read_reg(Self::LVT_LINT0_OFFSET) }
    }
    /// Reads the LVT_LINT1_OFFSET register
    pub fn lvt_lint1(&self) -> u32 {
        // SAFETY: This register doesn't have read side effects
        unsafe { self.read_reg(Self::LVT_LINT1_OFFSET) }
    }
    /// Reads the LVT_ERROR_OFFSET register
    pub fn lvt_error(&self) -> u32 {
        // SAFETY: This register doesn't have read side effects
        unsafe { self.read_reg(Self::LVT_ERROR_OFFSET) }
    }
    /// Reads the INITIAL_COUNT_OFFSET register
    pub fn initial_count(&self) -> u32 {
        // SAFETY: This register doesn't have read side effects
        unsafe { self.read_reg(Self::INITIAL_COUNT_OFFSET) }
    }
    /// Reads the CURRENT_COUNT_OFFSET register
    pub fn current_count(&self) -> u32 {
        // SAFETY: This register doesn't have read side effects
        unsafe { self.read_reg(Self::CURRENT_COUNT_OFFSET) }
    }
    /// Reads the DIVIDE_CONFIGURATION_OFFSET register
    pub fn divide_configuration(&self) -> u32 {
        // SAFETY: This register doesn't have read side effects
        unsafe { self.read_reg(Self::DIVIDE_CONFIGURATION_OFFSET) }
    }
}
