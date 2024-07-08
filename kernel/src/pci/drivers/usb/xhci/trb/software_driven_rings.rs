//! The [`CommandTrbRing`] and [`TransferTrbRing`] types

use core::cmp::Ordering;

use x86_64::PhysAddr;

use crate::{
    allocator::PageBox,
    pci::drivers::usb::xhci::trb::{command::CommandTrb, link::LinkTrb},
};

use super::{transfer::TransferTrb, RingFullError};

/// A type which is used for the implementation of [`CommandTrbRing`] and [`TransferTrbRing`]
#[derive(Debug)]
struct SoftwareDrivenTrbRing {
    /// The page where the ring is in memory
    page: PageBox,

    /// The index into the ring to write new TRBs
    enqueue: usize,

    /// The value of `cycle` to write for new TRBs
    cycle_state: bool,

    /// The index where the controller is reading TRBs.
    ///
    /// When the controller reads a TRB, it writes a [Command Completion] [`EventTrb`] to an
    /// [`EventTrbRing`], indicating that the TRB has been received. The address of the TRB is passed to [`update_dequeue`],
    /// which updates this field.
    ///
    /// [Command Completion]: super::event::command_completion::CommandCompletionTrb
    /// [`EventTrb`]: super::event::EventTrb
    /// [`EventTrbRing`]: super::event_ring::EventTrbRing
    /// [`update_dequeue`]: SoftwareDrivenTrbRing::update_dequeue
    dequeue: usize,
}

impl SoftwareDrivenTrbRing {
    /// The total length of the command ring including the link TRB
    const TOTAL_LENGTH: usize = 0x1000 / 16;
    /// The number of usable TRBs in the ring
    const USABLE_LENGTH: usize = Self::TOTAL_LENGTH - 1;

    /// Allocates a new [`SoftwareDrivenTrbRing`]
    fn new() -> Self {
        Self {
            page: PageBox::new_zeroed(),
            enqueue: 0,

            cycle_state: true,
            dequeue: 0,
        }
    }

    /// Gets the physical address of the start of the first segment of the ring
    fn ring_start_addr(&self) -> PhysAddr {
        self.page.phys_frame().start_address()
    }

    /// Writes the given data to the TRB slot at `i`
    ///
    /// # Safety
    /// * `value` either represents a valid TRB or is all zeroes
    /// * The TRB at `i` is currently owned by the OS
    /// * The caller is responsible for the behaviour of the controller in response to this TRB
    unsafe fn write(&mut self, i: usize, value: [u32; 4]) {
        assert!(i < Self::TOTAL_LENGTH);
        assert!(value[3] & 1 == self.cycle_state as u32);

        // SAFETY: This TRB is owned by the OS and is valid.
        // The caller is responsible for the behaviour of the controller.
        unsafe {
            self.page
                .as_mut_ptr::<[u32; 4]>()
                .add(i)
                .write_volatile(value);
        }
    }

    /// Writes to the link TRB at the end of the array.
    ///
    /// `chain` is whether to set the chain bit of the TRB. This should be set if a TD spans across the link TRB.
    ///
    /// # Safety
    /// * `chain` is set correctly
    unsafe fn write_link_trb(&mut self) {
        // Check that the link TRB isn't currently owned by the controller.
        // The link TRB is owned by the controller if the controller's TRBs wraps around the end of the array,
        // i.e. if the enqueue index is less than the dequeue index.
        assert!(self.enqueue >= self.dequeue);

        assert!(self.enqueue == Self::TOTAL_LENGTH - 1);

        // SAFETY: The TRB is valid.
        // The link TRB is owned by the OS.
        // The chain bit of link TRBs is ignored for the command ring
        unsafe {
            self.write(
                Self::TOTAL_LENGTH - 1,
                CommandTrb::Link(LinkTrb::new(
                    self.ring_start_addr(),
                    self.cycle_state,
                    true,
                    false,
                ))
                .to_parts(self.cycle_state),
            );
        }
    }

    /// Returns the number of TRBs currently in the buffer to be processed by the controller.
    ///
    /// This value is only accurate if [`dequeue`] is up-to-date.
    ///
    /// [`dequeue`]: SoftwareDrivenTrbRing::dequeue
    fn trbs_in_buffer(&self) -> usize {
        match self.enqueue.cmp(&self.dequeue) {
            Ordering::Equal => 0, // If enqueue == dequeue, len == 0
            Ordering::Greater => self.enqueue - self.dequeue, // If enqueue > dequeue
            Ordering::Less => Self::USABLE_LENGTH - self.dequeue + self.enqueue, // If enqueue < dequeue
        }
    }

    /// Returns the number of TRB slots which are currently owned by the OS.
    ///
    /// This value is only accurate if [`dequeue`] is up-to-date.
    ///
    /// [`dequeue`]: SoftwareDrivenTrbRing::dequeue
    fn free_space(&self) -> usize {
        Self::USABLE_LENGTH - self.trbs_in_buffer()
    }

    /// Writes a TRB to the buffer.
    ///
    /// This function does not ring the host controller doorbell, so the caller must do so to inform the controller to process the TRB.
    ///
    /// Returns the physical address of the queued TRB, to identify this TRB in future event TRBs.
    ///
    /// # Safety
    /// * The caller is responsible for the behaviour of the controller in response to this TRB
    unsafe fn enqueue(
        &mut self,
        trb: impl FnOnce(bool) -> [u32; 4],
    ) -> Result<PhysAddr, RingFullError> {
        if self.free_space() == 0 {
            return Err(RingFullError);
        }

        let trb_addr = self.ring_start_addr() + self.enqueue * 16;

        // SAFETY: The TRB is valid
        // The TRB at the enqueue pointer is owned by the OS as the ring contains free space
        // The caller is responsible for the behaviour of the controller in response to this TRB
        unsafe {
            self.write(self.enqueue, trb(self.cycle_state));
        }

        self.enqueue += 1;

        // If the ring has reached the end, add a link TRB
        if self.enqueue == Self::USABLE_LENGTH {
            // SAFETY: The chain bit is set properly
            unsafe {
                self.write_link_trb();
            }

            self.enqueue = 0;
            self.cycle_state = !self.cycle_state;
        }

        Ok(trb_addr)
    }

    /// Updates the ring's dequeue pointer
    ///
    /// # Safety
    /// * The passed address must have been read from the [`command_trb_pointer`] field of a [`CommandCompletion`] TRB.
    ///
    /// [`command_trb_pointer`]: super::event::command_completion::CommandCompletionTrb
    /// [`CommandCompletion`]: super::EventTrb::CommandCompletion
    unsafe fn update_dequeue(&mut self, dequeue: PhysAddr) {
        assert!(
            dequeue >= self.ring_start_addr(),
            "New dequeue pointer was outside the ring: address was too small. Dequeue: {dequeue:p}, Ring start: {:p}",
            self.ring_start_addr()
        );

        let acknowledged = ((dequeue - self.ring_start_addr()) / 16) as usize;

        assert!(
            acknowledged < Self::USABLE_LENGTH,
            "New dequeue pointer was outside the ring: address was too large. Dequeue: {dequeue:p}, Ring start: {:p}",
            self.ring_start_addr()
        );

        // The dequeue pointer is one TRB on from the acknowledged TRB, but needs to wrap around the end of the ring.
        self.dequeue = (acknowledged + 1) % Self::TOTAL_LENGTH;
    }
}

/// The _Command TRB Ring_
///
/// This ring contains [`CommandTrb`]s for the controller to execute.
#[derive(Debug)]
pub struct CommandTrbRing(SoftwareDrivenTrbRing);

impl CommandTrbRing {
    /// The total length of the command ring including the link TRB
    pub const TOTAL_LENGTH: usize = SoftwareDrivenTrbRing::TOTAL_LENGTH;

    /// Allocates a new [`CommandTrbRing`]
    pub fn new() -> Self {
        Self(SoftwareDrivenTrbRing::new())
    }

    /// Gets the physical address of the start of the first segment of the ring
    pub fn ring_start_addr(&self) -> PhysAddr {
        self.0.ring_start_addr()
    }

    /// Writes a TRB to the buffer.
    ///
    /// # Warning
    /// This function does not ring the host controller doorbell, so the caller must do so to inform the controller to process the TRB.
    /// To write a TRB and ring the doorbell, use [`XhciController::write_command_trb`].
    ///
    /// Returns the physical address of the queued TRB, to identify this TRB in future event TRBs.
    ///
    /// # Safety
    /// * The caller is responsible for the behaviour of the controller in response to this TRB.
    ///
    /// [`XhciController::write_command_trb`]: super::super::XhciController::write_command_trb
    pub unsafe fn enqueue(&mut self, trb: CommandTrb) -> Result<PhysAddr, RingFullError> {
        // SAFETY: This is just a wrapper function, so the safety requirements are the same.
        unsafe { self.0.enqueue(|cycle| trb.to_parts(cycle)) }
    }

    /// Updates the ring's dequeue pointer
    ///
    /// # Safety
    /// * The passed address must have been read from the [`command_trb_pointer`] field of a [`CommandCompletion`] TRB.
    ///
    /// [`command_trb_pointer`]: super::event::command_completion::CommandCompletionTrb
    /// [`CommandCompletion`]: super::EventTrb::CommandCompletion
    pub unsafe fn update_dequeue(&mut self, dequeue: PhysAddr) {
        // SAFETY: This is just a wrapper function, so the safety requirements are the same.
        unsafe { self.0.update_dequeue(dequeue) }
    }
}

/// The _Transfer TRB Ring_
///
/// This ring contains [`TransferTrb`]s for the controller to execute.
#[derive(Debug)]
pub struct TransferTrbRing(SoftwareDrivenTrbRing);

impl TransferTrbRing {
    /// The total length of the command ring including the link TRB
    pub const TOTAL_LENGTH: usize = SoftwareDrivenTrbRing::TOTAL_LENGTH;

    /// Allocates a new [`CommandTrbRing`]
    pub fn new() -> Self {
        Self(SoftwareDrivenTrbRing::new())
    }

    /// Gets the physical address of the start of the first segment of the ring
    pub fn ring_start_addr(&self) -> PhysAddr {
        self.0.ring_start_addr()
    }

    /// Writes a TRB to the buffer.
    ///
    /// This function does not ring the host controller doorbell, so the caller must do so to inform the controller to process the TRB.
    ///
    /// Returns the physical address of the queued TRB, to identify this TRB in future event TRBs.
    ///
    /// # Safety
    /// * The caller is responsible for the behaviour of the controller in response to this TRB
    pub unsafe fn enqueue(&mut self, trb: TransferTrb) -> Result<PhysAddr, RingFullError> {
        // SAFETY: This is just a wrapper function, so the safety requirements are the same.
        unsafe { self.0.enqueue(|cycle| trb.to_parts(cycle)) }
    }

    /// Updates the ring's dequeue pointer
    ///
    /// # Safety
    /// * The passed address must have been read from the [`command_trb_pointer`] field of a [`CommandCompletion`] TRB.
    ///
    /// [`command_trb_pointer`]: super::event::command_completion::CommandCompletionTrb
    /// [`CommandCompletion`]: super::EventTrb::CommandCompletion
    pub unsafe fn update_dequeue(&mut self, dequeue: PhysAddr) {
        // SAFETY: This is just a wrapper function, so the safety requirements are the same.
        unsafe { self.0.update_dequeue(dequeue) }
    }
}
