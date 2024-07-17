//! The [`CommandTrb`] type

use address_device::AddressDeviceTrb;
use x86_64::PhysAddr;

use crate::pci::drivers::usb::xhci::trb::GenericTrbFlags;

use self::{
    configure_endpoint::ConfigureEndpointTrb,
    slot::{DisableSlotTrb, EnableSlotTrb},
};

use super::{link::LinkTrb, software_driven_rings::SoftwareDrivenTrbRing, RingFullError, TrbType};

pub mod configure_endpoint;
pub mod slot;
pub mod address_device;

/// A TRB on the [`CommandTrbRing`].
///
/// This gives the controller a command to execute, used to manage slots, devices, and connections.
///
/// See the spec section [6.4.3] for more info.
///
/// [`CommandTrbRing`]: super::CommandTrbRing
/// [6.4.3]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A494%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C169%2C0%5D
#[derive(Debug)]
#[allow(clippy::missing_docs_in_private_items)] // TODO: add docs with the corresponding structs
pub enum CommandTrb {
    /// A [`LinkTrb`]
    Link(LinkTrb),

    EnableSlot(EnableSlotTrb),
    DisableSlot(DisableSlotTrb),
    AddressDevice(AddressDeviceTrb),
    ConfigureEndpoint(ConfigureEndpointTrb),
    EvaluateContext,
    ResetEndpoint,
    StopEndpoint,
    SetTRDequeuePointer,
    ResetDevice,
    ForceEvent,
    NegotiateBandwidth,
    SetLatencyToleranceValue,
    GetPortBandwidth,
    ForceHeader,
    /// A command which does nothing except cause the controller to send a [`CommandCompletion`] event.
    ///
    /// This is used to test that the command and event rings are set up properly
    ///
    /// [`CommandCompletion`]: super::event::EventTrb::CommandCompletion
    NoOp,
    GetExtendedProperty,
    SetExtendedProperty,
}

impl CommandTrb {
    /// Gets the type of the TRB
    fn trb_type(&self) -> TrbType {
        match self {
            CommandTrb::Link(_) => TrbType::Link,
            CommandTrb::EnableSlot(_) => TrbType::EnableSlotCommand,
            CommandTrb::DisableSlot(_) => TrbType::DisableSlotCommand,
            CommandTrb::AddressDevice(_) => TrbType::AddressDeviceCommand,
            CommandTrb::ConfigureEndpoint(_) => TrbType::ConfigureEndpointCommand,
            CommandTrb::EvaluateContext => TrbType::EvaluateContextCommand,
            CommandTrb::ResetEndpoint => TrbType::ResetEndpointCommand,
            CommandTrb::StopEndpoint => TrbType::StopEndpointCommand,
            CommandTrb::SetTRDequeuePointer => TrbType::SetTRDequeuePointerCommand,
            CommandTrb::ResetDevice => TrbType::ResetDeviceCommand,
            CommandTrb::ForceEvent => TrbType::ForceEventCommand,
            CommandTrb::NegotiateBandwidth => TrbType::NegotiateBandwidthCommand,
            CommandTrb::SetLatencyToleranceValue => TrbType::SetLatencyToleranceValueCommand,
            CommandTrb::GetPortBandwidth => TrbType::GetPortBandwidthCommand,
            CommandTrb::ForceHeader => TrbType::ForceHeaderCommand,
            CommandTrb::NoOp => TrbType::NoOpCommand,
            CommandTrb::GetExtendedProperty => TrbType::GetExtendedPropertyCommand,
            CommandTrb::SetExtendedProperty => TrbType::SetExtendedPropertyCommand,
        }
    }

    /// Converts the TRB to the data written to a TRB ring
    pub fn to_parts(&self, cycle: bool) -> [u32; 4] {
        let trb_type = self.trb_type();

        let parts = match self {
            CommandTrb::Link(link) => link.to_parts(cycle),
            CommandTrb::EnableSlot(enable_slot) => enable_slot.to_parts(cycle),
            CommandTrb::DisableSlot(disable_slot) => disable_slot.to_parts(cycle),
            CommandTrb::AddressDevice(address_device) => address_device.to_parts(cycle),
            CommandTrb::ConfigureEndpoint(configure_endpoint) => configure_endpoint.to_parts(cycle),
            CommandTrb::EvaluateContext => todo!(),
            CommandTrb::ResetEndpoint => todo!(),
            CommandTrb::StopEndpoint => todo!(),
            CommandTrb::SetTRDequeuePointer => todo!(),
            CommandTrb::ResetDevice => todo!(),
            CommandTrb::ForceEvent => todo!(),
            CommandTrb::NegotiateBandwidth => todo!(),
            CommandTrb::SetLatencyToleranceValue => todo!(),
            CommandTrb::GetPortBandwidth => todo!(),
            CommandTrb::ForceHeader => todo!(),
            CommandTrb::NoOp => [
                0,
                0,
                0,
                GenericTrbFlags::new()
                    .with_cycle(cycle)
                    .with_trb_type(TrbType::NoOpCommand)
                    .into(),
            ],
            CommandTrb::GetExtendedProperty => todo!(),
            CommandTrb::SetExtendedProperty => todo!(),
        };

        debug_assert_eq!(GenericTrbFlags::from(parts[3]).cycle(), cycle);
        debug_assert_eq!(GenericTrbFlags::from(parts[3]).trb_type(), trb_type);

        parts
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