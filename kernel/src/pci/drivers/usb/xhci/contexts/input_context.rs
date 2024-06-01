//! The [`InputContext`] type

use core::marker::PhantomData;

use crate::{
    allocator::PageBox, pci::drivers::usb::xhci::operational_registers::SupportedPageSize,
    util::generic_mutability::Immutable,
};

use super::{super::volatile_getter, device_context::DeviceContextRef, ContextSize};

/// The _Input Context_. This data structure "specifies the endpoints and the operations to
/// be performed on those endpoints by the Address Device, Configure Endpoint,
/// and Evaluate Context Commands".
///
/// For more info, see the spec section [6.2.5]
///
/// [6.2.5]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A466%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C473%2C0%5D
pub struct InputContext {
    /// The page where the data structure is in memory
    page: PageBox,
    /// The size of a context structure
    context_size: ContextSize,
}

impl InputContext {
    /// Allocates a new input context data structure.
    ///
    /// # Parameters
    /// * `page_size` is the page size supported by the controller, from the controller's operational registers.
    ///    This value can be obtained using the [`read_page_size`] method on the controller's [`OperationalRegisters`].
    /// * `context_size` the size of context structures.
    ///    This can be obtained using the [`context_size`] method on the controller's [`CapabilityParameters1`]
    ///
    /// [`read_page_size`]: super::super::operational_registers::OperationalRegisters::read_page_size
    /// [`OperationalRegisters`]: super::super::operational_registers::OperationalRegisters
    /// [`context_size`]: super::super::capability_registers::CapabilityParameters1::context_size
    /// [`CapabilityParameters1`]: super::super::capability_registers::CapabilityParameters1
    pub fn new(page_size: SupportedPageSize, context_size: ContextSize) -> Self {
        if page_size.page_size() != 0x1000 {
            todo!("Non-4k pages");
        }

        Self {
            page: PageBox::new(),
            context_size,
        }
    }

    /// Gets the [`InputControlContext`] for this input context
    fn input_control_context(&self) -> InputControlContext {
        InputControlContext {
            // The input control context is at the start of the input context so pointer is the same
            ptr: self.page.as_ptr(),
            p: PhantomData,
        }
    }

    /// Gets a reference to the input context's contained device context
    fn device_context(&self) -> DeviceContextRef<Immutable> {
        // Slots 2 to the end of the table have the same layout as a device context data structure

        // SAFETY: Stride can only be 32 or 64, so the memory is still part of the
        let ptr = unsafe { self.page.as_ptr::<()>().byte_add(self.context_size.bytes()) };
        // SAFETY: `ptr` is the start of a device context. `context_size` is valid.
        unsafe { DeviceContextRef::new(ptr, self.context_size) }
    }
}

/// The fields of the [`InputControlContext`] type
#[repr(C)]
struct InputControlContextFields {
    /// Bitflags for which device contexts need to be disabled by software
    drop_context_flags: u32,
    /// Bitflags for which device contexts need to be evaluated by software
    add_context_flags: u32,

    #[doc(hidden)]
    _reserved0: [u32; 5],

    /// If the controller supports [extended Configuration Information] and 
    /// [extended input context control fields are enabled], and this input context is associated with a [`ConfigureEndpointTrb`],
    /// then this field is the `bConfigurationValue` field of the Configuration Descriptor (TODO: links) associated with the TRB.
    /// 
    /// [`ConfigureEndpointTrb`]: super::super::trb::command::configure_endpoint::ConfigureEndpointTrb
    /// [extended Configuration Information]: super::super::capability_registers::CapabilityParameters2::supports_extended_configuration_information
    /// [extended input context control fields are enabled]: super::super::operational_registers::ConfigureRegister::config_info_enable
    configuration_value: u8,
    /// If the controller supports [extended Configuration Information] and 
    /// [extended input context control fields are enabled], and this input context is associated with a 
    /// [`ConfigureEndpointTrb`] which was issued due to a `SET_INTERFACE` request,
    /// then this field is the `bInterfaceNumber` field of the Configuration Descriptor (TODO: links) associated with the TRB.
    /// 
    /// [`ConfigureEndpointTrb`]: super::super::trb::command::configure_endpoint::ConfigureEndpointTrb
    /// [extended Configuration Information]: super::super::capability_registers::CapabilityParameters2::supports_extended_configuration_information
    /// [extended input context control fields are enabled]: super::super::operational_registers::ConfigureRegister::config_info_enable
    interface_number: u8,
    /// If the controller supports [extended Configuration Information] and 
    /// [extended input context control fields are enabled], and this input context is associated with a 
    /// [`ConfigureEndpointTrb`] which was issued due to a `SET_INTERFACE` request,
    /// then this field is the `bAlternateSetting` field of the Configuration Descriptor (TODO: links) associated with the TRB.
    /// 
    /// [`ConfigureEndpointTrb`]: super::super::trb::command::configure_endpoint::ConfigureEndpointTrb
    /// [extended Configuration Information]: super::super::capability_registers::CapabilityParameters2::supports_extended_configuration_information
    /// [extended input context control fields are enabled]: super::super::operational_registers::ConfigureRegister::config_info_enable
    alternate_setting: u8,

    #[doc(hidden)]
    _reserved1: u8,
}

/// The _Input Control Context_. This data structure defines which Device Context data
/// structures are affected by a command and the operations to be performed on
/// those contexts.
///
/// See the spec section [6.2.5.1] for more info.
///
/// [6.2.5.1]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A468%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C694%2C0%5D
pub struct InputControlContext<'a> {
    /// The pointer to where the input control context struct is mapped in virtual memory
    ptr: *const InputControlContextFields,

    /// Captures the lifetime `'a`. This means that an [`InputControlContext`] can't outlive the
    /// [`InputContext`] it is contained in.
    p: PhantomData<&'a InputControlContextFields>,
}

impl<'a> InputControlContext<'a> {
    volatile_getter!(
        InputControlContext, InputControlContextFields,
        drop_context_flags, u32,
        (fn drop_context_flags)
    );
    volatile_getter!(
        InputControlContext, InputControlContextFields,
        add_context_flags, u32,
        (fn add_context_flags)
    );

    volatile_getter!(
        InputControlContext, InputControlContextFields,
        configuration_value, u8,
        (pub fn configuration_value)
    );
    volatile_getter!(
        InputControlContext, InputControlContextFields,
        interface_number, u8,
        (pub fn interface_number)
    );
    volatile_getter!(
        InputControlContext, InputControlContextFields,
        alternate_setting, u8,
        (pub fn alternate_setting)
    );
}

impl<'a> InputControlContext<'a> {
    /// Gets the drop context flag for the given device context
    pub fn drop_context_flag(&self, n: u8) -> bool {
        // Bits 0 and 1 are reserved
        assert!((2..32).contains(&n));

        (self.drop_context_flags() & (1 << n)) != 0
    }

    /// Gets the add context flag for the given device context
    pub fn add_context_flag(&self, n: u8) -> bool {
        assert!((0..32).contains(&n));

        (self.add_context_flags() & (1 << n)) != 0
    }
}
