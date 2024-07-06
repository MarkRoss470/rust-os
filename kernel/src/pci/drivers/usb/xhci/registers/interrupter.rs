//! The [`Interrupter`] type

use x86_64::{PhysAddr, VirtAddr};

use super::super::trb::{EventTrb, EventTrbRing};
use super::super::volatile_accessors;

use core::fmt::Debug;
use core::ptr::{addr_of, addr_of_mut};

/// The _Interrupter Management Register_ of an [`Interrupter`].
///
/// Defined in the spec section [5.5.2.1]
///
/// [5.5.2.1]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A432%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C473%2C0%5D
#[bitfield(u32)]
pub struct InterrupterManagementRegister {
    /// This flag represents the current state of the [`Interrupter`]. If `true`, an interrupt is pending for this Interrupter.
    ///
    /// See the spec section [4.17.3] for the conditions that modify the state of this flag.
    ///
    /// [4.17.3]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A300%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C211%2C0%5D
    pub interrupt_pending: bool,

    /// whether the [`Interrupter`] is capable of generating an interrupt.
    /// When this field and [`interrupt_pending`] are both `true`, the Interrupter shall generate
    /// an interrupt when the Interrupter Moderation Counter reaches 0.
    /// If this field is `false`, then the [`Interrupter`] is prohibited from generating interrupts.
    ///
    /// [`interrupt_pending`]: InterrupterManagementRegister::interrupt_pending
    pub interrupt_enable: bool,

    #[bits(30)]
    #[doc(hidden)]
    reserved0: u32,
}

/// The _Interrupter Moderation Register_ of an [`Interrupter`].
///
/// Defined in the spec section [5.5.2.2]
///
/// [5.5.2.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A433%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C640%2C0%5D
#[bitfield(u32)]
pub struct InterrupterModerationRegister {
    /// The minimum inter-interrupt interval, in 250ns increments. A value of 0 disables interrupt throttling completely.
    pub interrupt_moderation_interval: u16,

    /// A countdown timer which is initialised to the value of [`interrupt_moderation_interval`]
    /// and counts down every 250ns, stopping at 0. When there is a TRB on this [`Interrupter`]'s event ring while the
    /// value is 0, an interrupt is triggered and the value is reset.
    pub interrupt_moderation_counter: u16,
}

/// The _Event Ring Table Size Register_ of an [`Interrupter`], which tells the controller the number of segments in the
/// [`EventTrbRing`]'s _Event Ring Segment Table_.
///
/// Defined in the spec section [5.5.2.3.1]
///
/// [5.5.2.3.1]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A434%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C563%2C0%5D
#[bitfield(u32)]
pub struct EventRingTableSizeRegister {
    pub event_ring_table_size: u16,

    #[bits(16)]
    #[doc(hidden)]
    reserved0: u16,
}

/// The _Event Ring Dequeue Pointer Register_ of an [`Interrupter`], which points to the
/// [`EventTrbRing`]'s _Event Ring Segment Table_.
///
/// Defined in the spec section [5.5.2.3.3]
///
/// [5.5.2.3.3.]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A435%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C412%2C0%5D
#[bitfield(u64)]
pub struct EventRingDequeuePointerRegister {
    #[bits(3)]
    pub dequeue_erst_segment_index: u8,
    pub event_handler_busy: bool,

    #[bits(60)]
    event_ring_dequeue_pointer_high: u64,
}

impl EventRingDequeuePointerRegister {
    /// The event ring dequeue pointer - the address of the last TRB read by software on this event ring
    pub fn event_ring_dequeue_pointer(&self) -> PhysAddr {
        PhysAddr::new(self.event_ring_dequeue_pointer_high() << 4)
    }
    /// The event ring dequeue pointer - the address of the last TRB read by software on this event ring
    pub fn set_event_ring_dequeue_pointer(&mut self, value: PhysAddr) {
        self.set_event_ring_dequeue_pointer_high(value.as_u64() >> 4);
    }
    /// The event ring dequeue pointer - the address of the last TRB read by software on this event ring
    pub fn with_event_ring_dequeue_pointer(&self, value: PhysAddr) -> Self {
        self.with_event_ring_dequeue_pointer_high(value.as_u64() >> 4)
    }
}

/// The fields of an [`InterrupterRegisterSet`].
/// This struct is used to guarantee all accesses are volatile.
///
/// This data structure is defined in the spec section [5.5.2]
///
/// [5.5.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A431%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C469%2C0%5D
#[repr(C)]
#[derive(Debug)]
struct InterrupterRegisterSetFields {
    /// Allows system software to enable, disable, and detect xHC interrupts
    interrupter_management: InterrupterManagementRegister,
    /// Controls the “interrupt moderation” feature of an [`Interrupter`], allowing system software to
    /// throttle the interrupt rate generated by the xHC.
    interrupter_moderation: InterrupterModerationRegister,
    /// The number of segments in the _Event Ring Segment Table_
    event_ring_table_size: EventRingTableSizeRegister,

    #[doc(hidden)]
    reserved0: u32,

    /// The physical address of the start of the _Event Ring Segment Table_
    event_ring_segment_table_base_address: u64,
    /// The dequeue pointer of the _Event Ring_. This is written by software to tell the controller
    /// when an [`EventTrb`] has been processed and it can write a new one in its place.
    event_ring_dequeue_pointer: EventRingDequeuePointerRegister,
}

/// The hardware registers associated with an [`Interrupter`].
///
/// This struct is defined in the spec section [5.5.2].
///
/// [5.5.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A431%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C469%2C0%5D
pub struct InterrupterRegisterSet {
    /// Where the registers are in memory.
    ptr: *mut InterrupterRegisterSetFields,
}

impl InterrupterRegisterSet {
    /// Constructs a new [`InterrupterRegisterSet`] around registers at the given virtual address.
    ///
    /// # Safety
    /// * `ptr` must be a pointer to a valid _Interrupter Register Set_
    /// * No other [`InterrupterRegisterSet`] may exist for the given `ptr`
    pub unsafe fn new(ptr: VirtAddr) -> Self {
        Self {
            ptr: ptr.as_mut_ptr(),
        }
    }

    /// Reads the base physical address of the _Event Ring Segment Table_.
    pub fn read_event_ring_segment_table_base_address(&self) -> PhysAddr {
        // SAFETY: `self.ptr` is valid
        let raw =
            unsafe { addr_of!((*self.ptr).event_ring_segment_table_base_address).read_volatile() };

        PhysAddr::new(raw & !0b11_1111)
    }

    /// Sets the base physical address of the _Event Ring Segment Table_.
    pub fn set_event_ring_segment_table_base_address(&mut self, value: PhysAddr) {
        assert_eq!(value.as_u64() & 0b11_1111, 0);

        // SAFETY: `self.ptr` is valid
        unsafe {
            addr_of_mut!((*self.ptr).event_ring_segment_table_base_address)
                .write_volatile(value.as_u64());
        }
    }
}

#[rustfmt::skip]
impl InterrupterRegisterSet {
    volatile_accessors!(
        InterrupterRegisterSet, InterrupterRegisterSetFields,
        interrupter_management, InterrupterManagementRegister,
        (pub fn read_interrupter_management), (pub unsafe fn set_interrupter_management)
    );

    volatile_accessors!(
        InterrupterRegisterSet, InterrupterRegisterSetFields,
        interrupter_moderation, InterrupterModerationRegister,
        (pub fn read_interrupter_moderation), (pub unsafe fn set_interrupter_moderation)
    );

    volatile_accessors!(
        InterrupterRegisterSet, InterrupterRegisterSetFields,
        event_ring_table_size, EventRingTableSizeRegister,
        (pub fn read_event_ring_table_size), (pub unsafe fn set_event_ring_table_size)
    );

    volatile_accessors!(
        InterrupterRegisterSet, InterrupterRegisterSetFields,
        event_ring_dequeue_pointer, EventRingDequeuePointerRegister,
        (pub fn read_event_ring_dequeue_pointer), (pub unsafe fn set_event_ring_dequeue_pointer)
    );
}

#[rustfmt::skip]
impl Debug for InterrupterRegisterSet {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let interrupter_management = self.read_interrupter_management();
        let interrupter_moderation = self.read_interrupter_moderation();
        let event_ring_table_size= self.read_event_ring_table_size();
        let event_ring_dequeue_pointer = self.read_event_ring_dequeue_pointer();
        let event_ring_segment_table_base_address = self.read_event_ring_segment_table_base_address();

        f.debug_struct("InterrupterRegisterSet")
            .field("interrupt_pending", &interrupter_management.interrupt_pending())
            .field("interrupt_enable", &interrupter_management.interrupt_enable())
            .field("interrupt_moderation_interval", &interrupter_moderation.interrupt_moderation_interval())
            .field("interrupt_moderation_counter", &interrupter_moderation.interrupt_moderation_counter())
            .field("event_ring_table_size", &event_ring_table_size.event_ring_table_size())
            .field("event_ring_segment_table_base_address", &event_ring_segment_table_base_address)
            .field("dequeue_erst_segment_index", &event_ring_dequeue_pointer.dequeue_erst_segment_index())
            .field("event_handler_busy", &event_ring_dequeue_pointer.event_handler_busy())
            .field("event_ring_dequeue_pointer", &event_ring_dequeue_pointer.event_ring_dequeue_pointer())
            .finish()
    }
}

/// An _Interrupter_
///
/// An Interrupter manages events and their notification to the host.
///
/// See the spec section [4.17] for more info.
///
/// [4.17]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A293%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C267%2C0%5D
#[derive(Debug)]
pub struct Interrupter {
    /// The interrupter's event ring
    event_ring: EventTrbRing,
    /// The interrupter's registers
    pub registers: InterrupterRegisterSet,
}

impl Interrupter {
    /// Constructs a new [`Interrupter`] around the given registers.
    ///
    /// # Safety
    /// * The passed `registers` must be valid for the whole lifetime of this struct.
    /// * Only one [`Interrupter`] may exist for a given [`InterrupterRegisterSet`] at once.
    pub unsafe fn new(mut registers: InterrupterRegisterSet) -> Self {
        // SAFETY: The pointer is valid
        let event_ring = unsafe { EventTrbRing::new() };

        // SAFETY: The event ring is set up
        unsafe {
            registers.set_event_ring_table_size(
                registers
                    .read_event_ring_table_size()
                    .with_event_ring_table_size(event_ring.segment_table_len()),
            );

            registers
                .set_event_ring_segment_table_base_address(event_ring.segment_table_start_addr());

            #[allow(clippy::cast_possible_truncation)]
            registers.set_event_ring_dequeue_pointer(
                EventRingDequeuePointerRegister::new()
                    .with_dequeue_erst_segment_index(event_ring.ring_start_segment() as u8)
                    .with_event_ring_dequeue_pointer(event_ring.ring_start_addr()),
            );
        }

        Self {
            event_ring,
            registers,
        }
    }

    /// Reads a TRB from this interrupter's [`EventTrbRing`], if one is present.
    pub fn dequeue(&mut self) -> Option<EventTrb> {
        // SAFETY: The dequeue pointer is about to be written
        let (trb, dequeue_addr) = unsafe { self.event_ring.dequeue()? };

        // SAFETY: This tells the controller that the TRB has been read, so it can write another one in the same place
        unsafe {
            self.registers.set_event_ring_dequeue_pointer(
                self.registers
                    .read_event_ring_dequeue_pointer()
                    .with_dequeue_erst_segment_index(0)
                    .with_event_ring_dequeue_pointer(dequeue_addr),
            );

            self.registers.set_interrupter_management(
                self.registers
                    .read_interrupter_management()
                    .with_interrupt_pending(false),
            );
        }

        Some(trb)
    }
}
