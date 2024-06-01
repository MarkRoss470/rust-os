//! The [`EventTrb`] type

use self::{command_completion::CommandCompletionTrb, port_status_change::PortStatusChangeTrb};

use super::{GenericTrbFlags, TrbType};

pub mod command_completion;
mod port_status_change;

/// An event sent from the controller to the OS on an [`EventTrbRing`]
///
/// [`EventTrbRing`]: super::event_ring::EventTrbRing
#[derive(Debug, Clone, Copy)]
#[allow(clippy::missing_docs_in_private_items)] // TODO: add docs with structs
pub enum EventTrb {
    Transfer,
    /// A TRB sent to indicate the completion or failure of a [`CommandTrb`].
    ///
    /// [`CommandTrb`]: super::CommandTrb
    CommandCompletion(CommandCompletionTrb),
    PortStatusChange(PortStatusChangeTrb),
    BandwidthRequest,
    Doorbell,
    HostController,
    DeviceNotification,
    MFINDEXWrap,
}

impl EventTrb {
    /// Constructs a new [`EventTrb`] from the raw data read from the event TRB ring.
    pub fn new(data: [u32; 4]) -> Self {
        let generic_flags = GenericTrbFlags::from(data[3]);

        match generic_flags.trb_type() {
            TrbType::TransferEvent => Self::Transfer,
            TrbType::CommandCompletionEvent => {
                Self::CommandCompletion(CommandCompletionTrb::new(data))
            }
            TrbType::PortStatusChangeEvent => {
                Self::PortStatusChange(PortStatusChangeTrb::new(data))
            }
            TrbType::BandwidthRequestEvent => Self::BandwidthRequest,
            TrbType::DoorbellEvent => Self::Doorbell,
            TrbType::HostControllerEvent => Self::HostController,
            TrbType::DeviceNotificationEvent => Self::DeviceNotification,
            TrbType::MFINDEXWrapEvent => Self::MFINDEXWrap,

            t => panic!("{t:?} is not a valid event TRB type"),
        }
    }
}
