//! The [`EnableSlotTrb`] and [`DisableSlotTrb`] types

use super::super::TrbType;

/// The _Enable Slot Command_, which causes the controller to select an available Device Slot and return the ID of the selected
/// slot to the host in a [`CommandCompletionTrb`].
///
/// [`CommandCompletionTrb`]: super::super::event::command_completion::CommandCompletionTrb
#[bitfield(u32)]
pub struct EnableSlotTrb {
    pub cycle: bool,

    #[bits(9)]
    _reserved: (),

    #[bits(6, default = TrbType::EnableSlotCommand)]
    pub trb_type: TrbType,

    /// The type of protocol to use for the slot.
    /// This value can be found in the controller's [`extended_capability_registers`].
    /// 
    /// [`extended_capability_registers`]: super::super::super::XhciController::extended_capability_registers
    #[bits(5)]
    pub slot_type: u8,

    #[bits(11)]
    _reserved: (),
}

impl EnableSlotTrb {
    /// Converts the TRB to the data written to a TRB ring
    pub fn to_parts(self, cycle: bool) -> [u32; 4] {
        // The first 3 qwords are all rsvdz, so just return 0s for them.
        [0, 0, 0, self.with_cycle(cycle).into()]
    }
}

/// The Disable Slot Command_, which causes the controller to release any bandwidth assigned to the slot with the ID in [`slot_id`],
/// and sets the [`slot_state`] field of the associated [`SlotContext`] to `Disabled`.
///
/// [`slot_id`]: DisableSlotTrb::slot_id
/// [`SlotContext`]: super::super::super::contexts::slot_context::SlotContext
/// [`slot_state`]: super::super::super::contexts::slot_context::SlotContext::slot_state
#[bitfield(u32)]
pub struct DisableSlotTrb {
    pub cycle: bool,

    #[bits(9)]
    _reserved: (),

    #[bits(6, default = TrbType::EnableSlotCommand)]
    pub trb_type: TrbType,

    #[bits(8)]
    _reserved: (),

    pub slot_id: u8,
}

impl DisableSlotTrb {
    /// Converts the TRB to the data written to a TRB ring
    pub fn to_parts(self, cycle: bool) -> [u32; 4] {
        // The first 3 dwords are all rsvdz, so just return 0s for them.
        [0, 0, 0, self.with_cycle(cycle).into()]
    }
}
