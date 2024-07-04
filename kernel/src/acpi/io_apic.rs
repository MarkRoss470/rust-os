//! Code to interact with the IO APIC for receiving hardware interrupts

use log::debug;
use x86_64::{
    structures::paging::{frame::PhysFrameRange, page::PageRange, Page, PhysFrame},
    PhysAddr, VirtAddr,
};

use crate::global_state::KERNEL_STATE;

use super::{InterruptActiveState, InterruptTriggerMode};

#[bitfield(u32)]
struct IoApicId {
    #[bits(24)]
    _reserved: (),

    #[bits(4)]
    id: u8,

    #[bits(4)]
    _reserved: (),
}

#[bitfield(u32)]
struct IoApicVersion {
    version: u8,

    #[bits(8)]
    _reserved: (),

    maximum_redirection_entry: u8,

    #[bits(8)]
    _reserved: u8,
}

#[bitfield(u32)]
struct IoApicArbitration {
    #[bits(24)]
    _reserved: (),

    #[bits(4)]
    arbitration_id: u8,

    #[bits(4)]
    reserved: u8,
}

/// A priority of delivering an interrupt to a local APIC.
///
/// For more information see the I/O APIC datasheet section [3.2.4].
///
/// [3.2.4]: https://web.archive.org/web/20161130153145if_/http://download.intel.com:80/design/chipsets/datashts/29056601.pdf#%5B%7B%22num%22%3A36%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C-12%2C797%2C0%5D
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InterruptDeliveryMode {
    /// Send the interrupt to all processor cores listed in the destination.
    Fixed,
    /// Send the interrupt to the core running at the lowest priority
    LowestPriority,
    /// System Management Interrupt.
    /// If this is selected, the [`vector`][RedirectionEntry::vector] is ignoredbut must be set to all 0s.
    /// The interrupt must be [`EdgeTriggered`][InterruptTriggerMode::EdgeTriggered].
    Smi,
    /// Non maskable interrupt - send the interrupt to the NMI signal of all processor cores listed in the destination.
    /// The interrupt is always treated as [`EdgeTriggered`][InterruptTriggerMode::EdgeTriggered].
    Nmi,
    /// Send the interrupt to all processor cores listed in the destination by asserting the `INIT` signal.
    /// The interrupt is always treated as [`EdgeTriggered`][InterruptTriggerMode::EdgeTriggered].
    Init,
    /// Send the interrupt to all processor cores listed in the destination,
    /// through an externally connected interrupt controller.
    ExtInt,
}

impl InterruptDeliveryMode {
    /// Constructs an [`InterruptDeliveryMode`] from its bit representation
    const fn from_bits(bits: u64) -> Self {
        match bits {
            0 => Self::Fixed,
            1 => Self::LowestPriority,
            2 => Self::Smi,
            4 => Self::Nmi,
            5 => Self::Init,
            7 => Self::ExtInt,
            _ => panic!("Invalid InterruptDeliveryMode"),
        }
    }

    /// Converts an [`InterruptDeliveryMode`] into its bit representation.
    const fn into_bits(self) -> u64 {
        match self {
            Self::Fixed => 0,
            Self::LowestPriority => 1,
            Self::Smi => 2,
            Self::Nmi => 4,
            Self::Init => 5,
            Self::ExtInt => 7,
        }
    }
}

/// Whether the local APIC is addressed physically by ID or logically
/// by checking the Destination Format Register and Logical Destination Register in each Local APIC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InterruptDestinationMode {
    /// The interrupt is sent to the local APIC with the ID stored in the lower 4 bits of
    /// [`destination`][RedirectionEntry::destination]
    Physical,
    /// The interrupt is sent to the local APIC(s) whose _Destination Format Register_ and
    /// _Logical Destination Register_ match [`destination`][RedirectionEntry::destination]
    Logical,
}

impl InterruptDestinationMode {
    /// Constructs an [`InterruptDestinationMode`] from its bit representation
    const fn from_bits(bits: u64) -> Self {
        match bits {
            0 => Self::Physical,
            1 => Self::Logical,
            _ => unreachable!(),
        }
    }

    /// Converts an [`InterruptDestinationMode`] into its bit representation
    const fn into_bits(self) -> u64 {
        match self {
            Self::Physical => 0,
            Self::Logical => 1,
        }
    }
}

/// The state of a local APIC's response to a level-trigged interrupt
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LevelTriggeredInterruptState {
    /// The local APIC(s) have sent an EOI
    EoiSent,
    /// The local APIC(s) have accepted the interrupt
    Accepted,
}

impl LevelTriggeredInterruptState {
    /// Constructs an [`LevelTriggeredInterruptState`] from its bit representation
    const fn from_bits(bits: u64) -> Self {
        match bits {
            0 => Self::EoiSent,
            1 => Self::Accepted,
            _ => unreachable!(),
        }
    }

    /// Converts an [`LevelTriggeredInterruptState`] into its bit representation
    const fn into_bits(self) -> u64 {
        match self {
            Self::EoiSent => 0,
            Self::Accepted => 1,
        }
    }
}

#[bitfield(u64)]
struct RedirectionEntry {
    /// What interrupt vector to send to the local APICs
    vector: u8,

    /// How to send the interrupt
    #[bits(3)]
    delivery_mode: InterruptDeliveryMode,

    /// How to interpret [`destination`][RedirectionEntry::destination]
    #[bits(1)]
    destination_mode: InterruptDestinationMode,

    /// The interrupt is pending to be sent
    will_be_sent: bool,

    /// Whether the interrupt is active high or low
    #[bits(1, from = InterruptActiveState::from_bits_u64, into = InterruptActiveState::into_bits_u64)]
    active_state: InterruptActiveState,

    /// This field is only valid when [`trigger_mode`][RedirectionEntry::trigger_mode]
    /// is [`LevelTriggered`][InterruptTriggerMode::LevelTriggered]
    #[bits(1)]
    level_triggered_state: LevelTriggeredInterruptState,

    /// Whether the interrupt is edge- or level-triggered.
    #[bits(1, from = InterruptTriggerMode::from_bits_u64, into = InterruptTriggerMode::into_bits_u64)]
    trigger_mode: InterruptTriggerMode,

    /// Whether the interrupt is masked. If this field is `true` then the interrupt won't be sent.
    masked: bool,

    #[bits(39)]
    _reserved: (),

    /// Which local APICs to send the interrupt to. How these bits are interpreted depends on
    /// [`destination_mode`][RedirectionEntry::destination_mode].
    destination: u8,
}

/// The registers of the I/O APIC, which is responsible for routing interrupts
/// from hardware to a local APIC
#[derive(Debug)]
pub struct IoApicRegisters(*mut u32);

impl IoApicRegisters {
    /// Constructs a new [`IoApicRegisters`] struct for registers at the given physical address.
    ///
    /// # Safety
    /// `ptr` must point to a valid system I/O APIC.
    /// This function may only be called once per APIC.
    pub unsafe fn new(ptr: PhysAddr) -> Self {
        let start = PhysFrame::containing_address(ptr);
        let frames = PhysFrameRange {
            start,
            end: start + 2,
        };

        // SAFETY: This function can only be called once per APIC, so this MMIO is not being used by other code
        let virt_addr = unsafe {
            KERNEL_STATE
                .physical_memory_accessor
                .lock()
                .map_frames(frames)
        };

        Self(virt_addr.start.start_address().as_mut_ptr())
    }
}

impl Drop for IoApicRegisters {
    fn drop(&mut self) {
        let start = Page::containing_address(VirtAddr::from_ptr(self.0));

        let pages = PageRange {
            start,
            end: start + 2,
        };

        // SAFETY: These are the pages which were mapped in `new`.
        // This struct is about to be destroyed, so the pointer will not be used again.
        unsafe {
            KERNEL_STATE
                .physical_memory_accessor
                .lock()
                .unmap_frames(pages);
        }
    }
}

impl IoApicRegisters {
    /// The offset into the I/O APIC physical registers of the address register
    const ADDRESS_REGISTER_OFFSET: usize = 0x00;
    /// The offset into the I/O APIC physical registers of the data register
    const DATA_REGISTER_OFFSET: usize = 0x10;

    /// Reads from a register. This method requires an `&mut self` parameter because
    /// reading a logical register requires writing to the physical address register, which is not thread safe.
    fn read_reg(&mut self, register: u32) -> u32 {
        assert!(register <= 0x3F);

        // SAFETY: These are the physical registers of the I/O APIC.
        // Any side effects are the caller's responsibility.
        unsafe {
            core::ptr::write_volatile(self.0.byte_add(Self::ADDRESS_REGISTER_OFFSET), register);
            core::ptr::read_volatile(self.0.byte_add(Self::DATA_REGISTER_OFFSET))
        }
    }

    /// Reads from a register.
    ///
    /// # Safety
    /// The write may have side effects.
    unsafe fn write_reg(&mut self, register: u32, value: u32) {
        assert!(register <= 0x3F);

        // SAFETY: These are the physical registers of the I/O APIC.
        // Any side effects are the caller's responsibility.
        unsafe {
            core::ptr::write_volatile(self.0.byte_add(Self::ADDRESS_REGISTER_OFFSET), register);
            core::ptr::write_volatile(self.0.byte_add(Self::DATA_REGISTER_OFFSET), value);
        }
    }

    /// Gets the APIC's [`IoApicId`]
    fn get_identification(&mut self) -> IoApicId {
        self.read_reg(0).into()
    }

    /// Gets the APIC's [`IoApicVersion`]
    fn get_version(&mut self) -> IoApicVersion {
        self.read_reg(1).into()
    }

    /// Gets the APIC's [`IoApicArbitration`]
    fn get_arbitration(&mut self) -> IoApicArbitration {
        self.read_reg(2).into()
    }

    /// Writes the redirection entry to the given interrupt vector.
    ///
    /// # Safety
    /// The `entry` must be valid and the core it points to must be set up to receive the interrupts.
    ///
    /// TODO: Take [`IoApicInterruptSourceOverride`]s
    /// into account when making this mapping
    ///
    /// [`IoApicInterruptSourceOverride`]: acpica_bindings::types::tables::madt::MadtRecord::IoApicInterruptSourceOverride
    unsafe fn write_redirection_entry(
        &mut self,
        vector: u8,
        entry: RedirectionEntry,
    ) -> Result<(), ()> {
        assert!(vector <= self.get_version().maximum_redirection_entry());

        let vector = vector as u32;
        let entry: u64 = entry.into();

        let higher = (entry >> 32) as u32;
        let lower = entry as u32;

        // SAFETY: This will write the entry to the vector specified by the caller.
        // Whether this operation in itself is sound is the caller's responsibility
        unsafe {
            self.write_reg(0x10 + vector * 2, lower);
            self.write_reg(0x10 + vector * 2 + 1, higher);
        }

        debug_assert_eq!(self.read_reg(0x10 + vector * 2), lower);
        debug_assert_eq!(self.read_reg(0x10 + vector * 2 + 1), higher);

        Ok(())
    }

    /// Sets the interrupt for the primary port of an
    /// [8042 PS/2 controller] (IRQ 1) to go to interrupt number `vector`.
    ///
    /// # Safety
    /// The `local_apic_id` must refer to a local APIC, and its associated core must be
    /// set up to receive interrupts from this source.
    ///
    /// [8042 PS/2 controller]: crate::cpu::ps2::Ps2Controller8042
    pub unsafe fn set_ps2_primary_port_interrupt(
        &mut self,
        local_apic_id: u8,
        vector: u8,
    ) -> Result<(), ()> {
        let entry = RedirectionEntry::new()
            .with_vector(vector)
            .with_delivery_mode(InterruptDeliveryMode::Fixed)
            .with_destination_mode(InterruptDestinationMode::Physical)
            .with_active_state(InterruptActiveState::ActiveHigh)
            .with_trigger_mode(InterruptTriggerMode::EdgeTriggered)
            .with_masked(false)
            .with_destination(local_apic_id);

        // SAFETY: The entry is valid as it was just constructed.
        // The core being ready is the caller's responsibility.
        unsafe { self.write_redirection_entry(1, entry) }
    }

    /// Sets the interrupt for the secondary port of an
    /// [8042 PS/2 controller] (IRQ 1) to go to interrupt number `vector`.
    ///
    /// # Safety
    /// The `local_apic_id` must refer to a local APIC, and its associated core must be
    /// set up to receive interrupts from this source.
    ///
    /// [8042 PS/2 controller]: crate::cpu::ps2::Ps2Controller8042
    pub unsafe fn set_ps2_secondary_port_interrupt(
        &mut self,
        local_apic_id: u8,
        vector: u8,
    ) -> Result<(), ()> {
        let entry = RedirectionEntry::new()
            .with_vector(vector)
            .with_delivery_mode(InterruptDeliveryMode::Fixed)
            .with_destination_mode(InterruptDestinationMode::Physical)
            .with_active_state(InterruptActiveState::ActiveHigh)
            .with_trigger_mode(InterruptTriggerMode::EdgeTriggered)
            .with_masked(false)
            .with_destination(local_apic_id);

        // SAFETY: The entry is valid as it was just constructed.
        // The core being ready is the caller's responsibility.
        unsafe { self.write_redirection_entry(12, entry) }
    }
}
