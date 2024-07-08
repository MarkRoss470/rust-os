//! The [`TransferTrb`] type

use normal::NormalTrb;

use super::link::LinkTrb;

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
