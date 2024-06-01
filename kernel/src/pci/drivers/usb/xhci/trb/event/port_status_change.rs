//! The [`PortStatusChangeTrb`] type

use crate::pci::drivers::usb::xhci::trb::GenericTrbFlags;

#[bitfield(u32)]
struct Dword1 {
    #[bits(24)]
    _reserved: (),

    port_id: u8,
}

#[bitfield(u32)]
struct Dword3 {
    #[bits(24)]
    _reserved: (),

    completion_code: u8,
}

#[allow(unused)] // For docs
use super::super::super::operational_registers::port_registers::{
    PortRegisterFields, StatusAndControl,
};
use super::command_completion::CompletionCode;

/// A _Port Status Change_ TRB. This is generated whenever one of the following bits is set on 
/// the [`StatusAndControl`] field or a [`PortRegisterFields`] struct:
///
/// * [`connect_status_change`]
/// * [`port_enabled_change`]
/// * [`warm_port_reset_change`]
/// * [`over_current_change`]
/// * [`port_reset_change`]
/// * [`port_link_state_change`]
/// * [`port_config_error_change`]
///
/// [`connect_status_change`]: StatusAndControl::connect_status_change
/// [`port_enabled_change`]: StatusAndControl::port_enabled_change
/// [`warm_port_reset_change`]: StatusAndControl::warm_port_reset_change
/// [`over_current_change`]: StatusAndControl::over_current_change
/// [`port_reset_change`]: StatusAndControl::port_reset_change
/// [`port_link_state_change`]: StatusAndControl::port_link_state_change
/// [`port_config_error_change`]: StatusAndControl::port_config_error_change
#[derive(Debug, Clone, Copy)]
pub struct PortStatusChangeTrb {
    /// The port ID which has changed
    pub port_id: u8,
    /// The completion code of the TRB
    pub completion_code: CompletionCode,
    /// The TRB's flags
    pub flags: GenericTrbFlags,
}

impl PortStatusChangeTrb {
    /// Constructs a [`PortStatusChangeTrb`] from the data read from the event ring
    pub fn new(data: [u32; 4]) -> Self {
        let port_id = (data[0] >> 24) as u8;
        let completion_code = CompletionCode::new((data[2] >> 24) as u8);
        let flags = GenericTrbFlags::from(data[3]);

        Self {
            port_id,
            completion_code,
            flags,
        }
    }
}
