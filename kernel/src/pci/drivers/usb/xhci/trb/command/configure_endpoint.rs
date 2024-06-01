//! The [`ConfigureEndpointTrb`] type

use x86_64::PhysAddr;

use crate::pci::drivers::usb::xhci::trb::TrbType;

/// The input context pointer field of a [`ConfigureEndpointTrb`].
/// This tells the controller how to set up a device slot.
#[derive(Debug)]
pub enum InputContextPointer {
    /// The controller should deconfigure the device slot
    Deconfigure,
    /// The controller should configure the device slot using the physical address of the
    /// [`InputContext`] which should be associated with it.
    Configure(PhysAddr),
}

#[bitfield(u32)]
struct ConfigureEndpointTrbFlags {
    cycle: bool,

    #[bits(8)]
    _reserved: (),

    deconfigure: bool,

    #[bits(6, default = TrbType::ConfigureEndpointCommand)]
    trb_type: TrbType,

    #[bits(8)]
    _reserved: (),

    slot_id: u8,
}

/// A `Configure Endpoint TRB`, which instructs the controller to evaluates the bandwidth and resource
/// requirements of endpoints.
/// 
/// See the spec section [6.4.3.5] and [4.6.6] for more info.
/// 
/// [6.4.3.5]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A498%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C642%2C0%5D
/// [4.6.6]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A122%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C511%2C0%5D
#[derive(Debug)]
pub struct ConfigureEndpointTrb {
    /// The physical address of the [`InputContext`] to use, or an instruction to deconfigure the endpoint
    input_context_pointer: InputContextPointer,
    /// The slot id to configure
    slot_id: u8,
}

impl ConfigureEndpointTrb {
    /// Converts the TRB to the data written to a TRB ring
    pub fn to_parts(&self, cycle: bool) -> [u32; 4] {
        let (icp_low, icp_high, deconfigure) = match self.input_context_pointer {
            InputContextPointer::Configure(p) => {
                debug_assert!(p.is_aligned(1u64 << 4));

                #[allow(clippy::cast_possible_truncation)]
                (p.as_u64() as u32, (p.as_u64() >> 32) as u32, false)
            }
            _ => (0, 0, true),
        };

        let flags = ConfigureEndpointTrbFlags::new()
            .with_cycle(cycle)
            .with_deconfigure(deconfigure)
            .with_slot_id(self.slot_id);

        [icp_low, icp_high, 0, flags.into()]
    }
}
