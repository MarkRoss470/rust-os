//! The [`handle_port_status_change`] task and [`PortStatusChangeTask`] type alias

use core::cell::RefCell;

use futures::Future;
use log::debug;

use crate::pci::drivers::usb::xhci::{
    tasks::{TimeoutReachedError, TIMEOUT_1_SECOND},
    trb::event::{command_completion::CompletionCode, port_status_change::PortStatusChangeTrb},
    XhciController,
};

use super::TaskWaker;

/// The type of the future produced by [`handle_port_status_change_inner`], and stored in [`PortStatusChange`] tasks
///
/// [`PortStatusChange`]: super::TaskType::PortStatusChange
pub type PortStatusChangeTask<'a> = impl Future<Output = Result<(), Error>> + 'a;

/// An error occurring during the execution of [`handle_port_status_change`]
#[derive(Debug, Clone, Copy)]
pub struct Error {
    /// The port id of the port which was being handled
    port_id: u8,
    /// The type of error which occurred
    kind: ErrorKind,
}

/// A type of [`Error`]
#[derive(Debug, Clone, Copy)]
enum ErrorKind {
    /// The initial TRB had a non-success completion code
    InitialError(CompletionCode),
    /// The port failed to reset
    Reset(CompletionCode),
    /// A timeout expired
    Timeout,
}

/// Handles a [`PortStatusChangeTrb`] following the process defined in the spec section [4.3]
///
/// [`PortStatusChangeTrb`]: super::super::trb::event::port_status_change::PortStatusChangeTrb
/// [4.3]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A90%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C658%2C0%5D
async fn handle_port_status_change_inner<'a>(
    controller: &RefCell<XhciController>,
    t: &TaskWaker,
    trb: PortStatusChangeTrb,
) -> Result<(), Error> {
    // Check that the TRB which triggered this task was successful
    if trb.completion_code != CompletionCode::Success {
        return Err(Error {
            port_id: trb.port_id,
            kind: ErrorKind::InitialError(trb.completion_code),
        });
    }

    // Read the status and control register
    let status_and_control = {
        let controller_borrow = controller.borrow_mut();
        let port = controller_borrow
            .operational_registers
            .port(trb.port_id.into())
            .unwrap();
        port.read_status_and_control()
    };

    // Check whether the status change was an attach or detach
    if status_and_control.connect_status_change() {
        // USB2 ports require a reset to advance the port to the enabled state
        if !status_and_control.port_enabled() {
            reset_usb2_port(controller, trb.port_id, t).await?;
        }

        debug!("Device attach on port {:?}", trb.port_id);
    } else {
        debug!("Device detach on port {:?}", trb.port_id);
    }

    Ok(())
}

/// Resets a USB2 port and waits for a response
async fn reset_usb2_port(
    controller: &RefCell<XhciController>,
    port_id: u8,
    t: &TaskWaker,
) -> Result<(), Error> {
    debug!("Resetting USB2 port");

    // Write the reset flag to reset the port
    {
        let mut controller_borrow = controller.borrow_mut();
        let mut port = controller_borrow
            .operational_registers
            .port_mut(port_id.into())
            .unwrap();

        let new_status_and_control = port.read_status_and_control().normalised().with_reset(true);

        port.write_status_and_control(new_status_and_control);
    }

    // Wait for a PortStatusChange TRB indicating that the port has been reset
    match t.wait_for_port_status_change(port_id, TIMEOUT_1_SECOND).await {
        Ok(trb) => {
            if trb.completion_code != CompletionCode::Success {
                return Err(Error {
                    port_id,
                    kind: ErrorKind::Reset(trb.completion_code),
                });
            }
        }
        Err(TimeoutReachedError) => {
            return Err(Error {
                port_id,
                kind: ErrorKind::Timeout,
            });
        }
    };

    Ok(())
}

/// Wrapper around [`handle_port_status_change_inner`] which also acts as the defining use of the [`PortStatusChangeTask`] type alias
pub fn handle_port_status_change<'a>(
    s: &'a RefCell<XhciController>,
    t: &'a TaskWaker,
    trb: PortStatusChangeTrb,
) -> PortStatusChangeTask<'a> {
    handle_port_status_change_inner(s, t, trb)
}
