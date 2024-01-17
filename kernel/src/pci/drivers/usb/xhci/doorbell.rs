//! The [`DoorbellRegisters`] type

use core::marker::PhantomData;

use x86_64::VirtAddr;

/// Which endpoint within the slot this doorbell write is targeting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoorbellTarget {
    /// The doorbell write indicates an update of the control endpoint's dequeue pointer
    ControlEndpoint,

    /// The doorbell write indicates an update of an OUT endpoint's dequeue pointer.
    ///
    /// The stored `u8` is the EP number. As the [`ControlEndpoint`] is EP number 0, this value is 1-based
    /// (The first OUT EP is 1, second is 2, etc). The maximum EP number is 15.
    ///
    /// [`ControlEndpoint`]: DoorbellTarget::ControlEndpoint
    OutEndpoint(u8),
    /// The doorbell write indicates an update of an OUT endpoint's dequeue pointer.
    ///
    /// The stored `u8` is the EP number. As the [`ControlEndpoint`] is EP number 0, this value is 1-based
    /// (The first IN EP is 1, second is 2, etc). The maximum EP number is 15.
    ///
    /// [`ControlEndpoint`]: DoorbellTarget::ControlEndpoint
    InEndpoint(u8),

    /// Reserved
    Reserved(u8),
    /// Vendor defined
    VendorDefined(u8),
}

impl DoorbellTarget {
    /// Gets the [`DoorbellTarget`] as the byte value understood by the controller
    const fn to_byte(self) -> u8 {
        match self {
            Self::ControlEndpoint => 1,
            Self::OutEndpoint(ep) => {
                debug_assert!(ep != 1 && ep <= 15);
                ep * 2
            }
            Self::InEndpoint(ep) => {
                debug_assert!(ep != 1 && ep <= 15);
                ep * 2 + 1
            }
            Self::Reserved(v) => {
                debug_assert!(v == 0 || (v >= 32 && v <= 247));
                v
            }
            Self::VendorDefined(v) => {
                debug_assert!(v >= 248);
                v
            }
        }
    }

    /// Parses a [`DoorbellTarget`] from the byte value understood by the controller
    const fn from_byte(byte: u8) -> Self {
        match byte {
            1 => Self::ControlEndpoint,
            2..=30 if byte % 2 == 0 => Self::OutEndpoint(byte / 2),
            2..=31 => Self::OutEndpoint(byte / 2),
            0 | 32..=247 => Self::Reserved(byte),
            248..=255 => Self::VendorDefined(byte),
        }
    }

    /// Constructs a [`DoorbellTarget`] from its bit representation
    const fn from_bits(bits: u32) -> Self {
        #[allow(clippy::cast_possible_truncation)]
        Self::from_byte(bits as _)
    }

    /// Converts a [`DoorbellTarget`] into its bit representation
    const fn into_bits(self) -> u32 {
        self.to_byte() as _
    }
}

#[bitfield(u32)]
pub struct DoorbellArrayEntry {
    #[bits(8)]
    target: DoorbellTarget,

    #[bits(8)]
    _reserved: (),

    task_id: u16,
}

/// The doorbell registers of the XHCI controller.
/// This is an array of up to 256 32-bit registers which the OS writes to to indicate that there is something for the controller to do
/// (e.g. processing a command TRB).
#[derive(Debug)]
pub struct DoorbellRegisters {
    /// Pointer to the first doorbell
    ptr: *mut DoorbellArrayEntry,
    /// The number of doorbells
    len: usize,
}

impl DoorbellRegisters {
    /// Constructs a new [`DoorbellRegisters`] struct for registers at the given address
    ///
    /// # Safety
    /// * `ptr` must be the address of the first doorbell register
    /// * `max_device_slots` must be the value of the controller's [`max_device_slots`] field
    /// * There must not be an existing [`DoorbellRegisters`] struct for this controller
    ///
    /// [`max_device_slots`]: super::capability_registers::StructuralParameters1::max_device_slots
    pub unsafe fn new(ptr: VirtAddr, max_device_slots: usize) -> Self {
        assert!(max_device_slots > 1);

        Self {
            ptr: ptr.as_mut_ptr(),
            len: max_device_slots,
        }
    }

    /// Gets the host controller doorbell
    pub fn host_controller_doorbell(&mut self) -> HostControllerDoorbell {
        HostControllerDoorbell(self.ptr.cast(), PhantomData)
    }
}

/// The host controller doorbell. This is the first doorbell and a write to it indicates that
/// there is a TRB to be processed in the command ring.
#[derive(Debug)]
pub struct HostControllerDoorbell<'a>(*mut u32, PhantomData<&'a mut u32>);

impl<'a> HostControllerDoorbell<'a> {
    /// Rings the doorbell
    pub fn ring(&mut self) {
        // SAFETY: The stored pointer points to the host controller doorbell.
        // Writing 0 to this register rings the doorbell
        unsafe { self.0.write_volatile(0) }
    }
}
