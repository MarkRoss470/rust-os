//! The [`TransferTrb`] type

use normal::NormalTrb;
use x86_64::PhysAddr;

use super::{link::LinkTrb, software_driven_rings::SoftwareDrivenTrbRing, RingFullError};

pub mod normal;


/// A TRB on a transfer TRB ring (TODO: link).
///
/// This tells the controller how to send or receive data.
///
/// See the spec section [6.4.1] for more info.
///
/// [6.4.1]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A472%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C548%2C0%5D
#[derive(Debug)]
#[allow(clippy::missing_docs_in_private_items)] // TODO: add docs with the corresponding structs
pub enum TransferTrb {
    /// A [`NormalTrb`]
    Normal(NormalTrb),
    SetupStage,
    DataStage,
    StatusStage,
    Isoch,
    /// A [`LinkTrb`]
    Link(LinkTrb),
    EventData,
    NoOp,
}

impl TransferTrb {
    /// Converts the TRB to the data written to a TRB ring
    pub fn to_parts(&self, cycle: bool) -> [u32; 4] {
        match self {
            TransferTrb::Normal(normal) => normal.to_parts(cycle),
            TransferTrb::SetupStage => todo!(),
            TransferTrb::DataStage => todo!(),
            TransferTrb::StatusStage => todo!(),
            TransferTrb::Isoch => todo!(),
            TransferTrb::Link(link) => link.to_parts(cycle),
            TransferTrb::EventData => todo!(),
            TransferTrb::NoOp => todo!(),
        }
    }

    /// Gets the chain bit for this TRB.
    /// This is used to set the chain bit of [`LinkTrb`]s correctly, as this needs to match the TRB before it.
    pub fn chain(&self) -> bool {
        match self {
            TransferTrb::Normal(normal) => normal.chain(),
            TransferTrb::SetupStage => todo!(),
            TransferTrb::DataStage => todo!(),
            TransferTrb::StatusStage => todo!(),
            TransferTrb::Isoch => todo!(),
            TransferTrb::Link(link) => link.chain(),
            TransferTrb::EventData => todo!(),
            TransferTrb::NoOp => todo!(),
        }
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
