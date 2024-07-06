//! The [`AddressDeviceTrb`] type

use x86_64::PhysAddr;

use crate::pci::drivers::usb::xhci::trb::TrbType;

#[bitfield(u32)]
pub struct AddressDeviceTrbFlags {
    cycle: bool,

    #[bits(8)]
    _reserved: (),

    block_set_address_request: bool,

    #[bits(6, default = TrbType::AddressDeviceCommand)]
    trb_type: TrbType,

    #[bits(8)]
    _reserved: (),

    slot_id: u8,
}

/// An `Address Device TRB`, which  transitions the selected [Device Context] from
/// the [`Default`] to the [`Addressed`] state and causes the controller to select an address for
/// the USB device in the `Default` state and issue a `SET_ADDRESS` request to the device.
/// See the spec sections [4.6.5] and [6.4.3.4] for more information.
///
/// [Device Context]: super::super::super::contexts::device_context::DeviceContextRef
/// [`Default`]: super::super::super::contexts::slot_context::SlotState::Default
/// [`Addressed`]: super::super::super::contexts::slot_context::SlotState::Addressed
/// [4.6.5]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A117%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C169%2C0%5D
/// [6.4.3.4]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A497%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C694%2C0%5D
#[derive(Debug)]
pub struct AddressDeviceTrb {
    /// The pointer to the [`InputContext`] to use
    ///
    /// [`InputContext`]: super::super::super::contexts::input_context::InputContext
    /// [Device Context]: super::super::super::contexts::device_context::DeviceContextRef
    pub input_context_pointer: PhysAddr,
    /// The index into the [`DeviceContextBaseAddressArray`] of the [device context] to use
    /// 
    /// [`DeviceContextBaseAddressArray`]: super::super::super::dcbaa::DeviceContextBaseAddressArray
    /// [device context]: super::super::super::contexts::device_context::DeviceContextRef
    pub slot_id: u8,
    /// Whether to inhibit the controller from sending a USB `SET_ADDRESS` request
    pub block_set_address_request: bool,
}

impl AddressDeviceTrb {
    /// Converts the TRB to the data written to a TRB ring
    pub fn to_parts(&self, cycle: bool) -> [u32; 4] {
        assert!(
            self.input_context_pointer.is_aligned(16u64),
            "Input contexts passed in an AddressDeviceTrb must be 16-byte aligned"
        );

        #[allow(clippy::cast_possible_truncation)]
        let icp_low = self.input_context_pointer.as_u64() as u32;
        let icp_high = (self.input_context_pointer.as_u64() >> 32) as u32;

        let flags = AddressDeviceTrbFlags::new()
            .with_cycle(cycle)
            .with_block_set_address_request(self.block_set_address_request)
            .with_slot_id(self.slot_id);

        [icp_low, icp_high, 0, flags.into()]
    }
}
