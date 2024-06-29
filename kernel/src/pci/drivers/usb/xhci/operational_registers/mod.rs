//! Contains the [`OperationalRegisters`] struct and the types it depends on

pub mod port_registers;

use core::fmt::Debug;
use x86_64::{PhysAddr, VirtAddr};

use self::port_registers::PortRegister;

use super::{capability_registers::CapabilityRegisters, volatile_accessors, volatile_getter};
use crate::{print, println, util::generic_mutability::{Immutable, Mutable}};

/// The behaviour of when the controller is allowed to stop incrementing MFINDEX.
/// Regardless of this setting, the controller may always stop incrementing if all root hub ports are in the
/// Disconnected, Disabled, or Powered-Off states.
///
/// See the spec section [4.12.2] for more info.
///
/// [4.12.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A253%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C694%2C0%5D
#[derive(Debug)]
pub enum MfindexStopBehaviour {
    /// The controller may stop incrementing MFINDEX if all root hub ports are in the
    /// Training, Disconnected, Disabled, or Powered-Off states.
    Training,
    /// The controller may stop incrementing MFINDEX if all root hub ports are in the
    /// U3, Disconnected, Disabled, or Powered-Off states.
    U3,
}

impl MfindexStopBehaviour {
    /// Constructs an [`MfindexStopBehaviour`] from its bit representation
    const fn from_bits(value: u32) -> Self {
        match value {
            0 => Self::Training,
            _ => Self::U3,
        }
    }

    /// Converts an [`MfindexStopBehaviour`] to its bit representation
    const fn into_bits(self) -> u32 {
        match self {
            Self::Training => 0,
            Self::U3 => 1,
        }
    }
}

/// The `USBCMD` field of an [`OperationalRegisters`] structure.
///
/// See the spec section [5.4.1] for more info
///
/// [5.4.1]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A400%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C554%2C0%5D
#[bitfield(u32, debug = false, default = false)]
pub struct UsbCommand {
    /// Whether the controller is currently running.
    /// When `false` is written to this field, the controller will execute any queued commands and TDs, and then halts.
    /// The [`host_controller_halted`][UsbStatus::host_controller_halted] field indicates when the controller has halted.
    /// It is undefined behaviour to write `true` to this field unless the controller is in the halted state (HCHalted = `true`).
    #[bits(1)]
    pub enabled: bool,

    /// Writing `true` to this field resets the controller.
    /// When the reset is complete, the controller will write `false` to this field.
    /// All internal state is reset, but PCI configuration registers (e.g. BARs) are not.
    /// After resetting, the controller must be reinitialised.
    /// Use [`XhciController::reset_and_wait`][super::XhciController::reset_and_wait] to handle writing to this field.
    pub reset: bool,

    /// Whether the device will produce host system interrupts (i.e. CPU interrupts) on USB events.
    pub interrupts_enabled: bool,

    /// Whether the controller asserts out-of-band error signalling to the host. The signalling is acknowledged
    /// by software clearing the HSE bit.
    ///
    /// See the spec section [4.10.2.6] for more information.
    ///
    /// [4.10.2.6]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A207%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C252%2C0%5D
    pub host_system_error_enabled: bool,

    #[bits(3)]
    #[doc(hidden)]
    reserved0: u32,

    /// If [`CapabilityParameters1::supports_lhcr`] is `true`, writing `true` to this field triggers the device to reset
    /// without affecting the state of the ports.
    /// After the reset is complete, the controller will write `false` to this field.
    /// After resetting, the controller must be reinitialised.
    ///
    /// [`CapabilityParameters1::supports_lhcr`]: super::capability_registers::CapabilityParameters1::supports_lhcr
    pub light_reset: bool,

    /// If the `HCHalted` field in the `USBSTS` register (TODO: link) is 1, writing `true` to this field triggers the
    /// controller to save its state. The status of the save is indicated by the `SSS` field of the `USBSTS` register (TODO: link).
    /// It is undefined behaviour to trigger a save while the controller is loading its state (`USBSTS` field `RSS` = 1) (TODO: link)
    ///
    /// See the spec section [4.23.2] for more info on saving and loading state.
    ///
    /// [4.23.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A348%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C219%2C0%5D
    pub save_state: bool,

    /// If the `HCHalted` field in the `USBSTS` register (TODO: link) is 1, writing `true` to this field triggers the
    /// controller to restore its state. The status of the save is indicated by the RSS field of the `USBSTS register` (TODO: link).
    /// It is undefined behaviour to trigger a restore while the controller is saving its state (`USBSTS` field `SSS` = 1) (TODO: link)
    ///
    /// See the spec section [4.23.2] for more info on saving and loading state.
    ///
    /// [4.23.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A348%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C219%2C0%5D
    pub restore_state: bool,

    /// Whether the controller will generate a `MFINDEX` Wrap Event every time the `MFINDEX` register transitions
    /// from 0x03FFF to 0x00.
    ///
    /// See the spec section [4.12.2] for more info
    ///
    /// [4.12.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A253%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C694%2C0%5D
    pub wrap_events_enabled: bool,

    /// Under what conditions the controller is allowed to stop incrementing `MFINDEX`
    #[bits(1)]
    pub mfindex_stop_behaviour: MfindexStopBehaviour,

    #[bits(1)]
    #[doc(hidden)]
    reserved1: u32,

    /// Whether a _Max Exit Latency Too Large Capability Error_ may be returned by a _Configure Endpoint Command_.
    ///
    /// See the spec section [4.23.5.2.2] for more info.
    ///
    /// [4.23.5.2.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A363%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C637%2C0%5D
    pub cem_enabled: bool,

    /// Whether the controller supports Transfer Burst Count (TBC) values greater that 4 in isoch TDs.
    /// If `true`, the Isoch TRB TD Size/TBC field presents the TBC value, and the TBC/RsvdZ field is RsvdZ.
    /// If `false`, the TDSize/TCB field presents the TD Size value, and the TBC/RsvdZ field presents the TBC value.
    ///
    /// See the spec section [4.11.2.3] for more info.
    ///
    /// [4.11.2.3]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A222%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C267%2C0%5D
    pub extended_tbc_supported: bool,

    /// Whether the controller supports ETC_TSC capability. If `true`, the `TRBSts` field of a TRB is updated
    /// to indicate whether it is the last TRB of the TD.
    ///
    /// See the spec section [4.11.2.3] for more info.
    ///
    /// [4.11.2.3]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A222%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C267%2C0%5D
    pub extended_tbc_trb_status_supported: bool,

    /// Whether the controller should enable VTIO and use information provided by VTIO registers to determine its
    /// DMA-ID (direct memory access). Otherwise, the controller will use the primary DMA-ID for all accesses.
    pub vtio_enabled: bool,

    #[bits(15)]
    #[doc(hidden)]
    reserved2: u32,
}

#[rustfmt::skip]
impl Debug for UsbCommand {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // TODO: maybe add some info about ongoing commands e.g. reset?
        f.debug_struct("UsbCommand")
            .field("enables", &self.enabled())
            .field("interrupts_enabled", &self.interrupts_enabled())
            .field("host_system_error_enabled", &self.host_system_error_enabled())
            .field("wrap_events_enabled", &self.wrap_events_enabled())
            .field("mfindex_stop_behaviour", &self.mfindex_stop_behaviour())
            .field("cem_enabled", &self.cem_enabled())
            .field("extended_tbc_supported", &self.extended_tbc_supported())
            .field("extended_tbc_trb_status_supported", &self.extended_tbc_trb_status_supported())
            .field("vtio_enabled", &self.vtio_enabled())
            .finish()
    }
}

/// Indicates various information about the state of the controller, as well as pending interrupts.
#[bitfield(u32)]
pub struct UsbStatus {
    /// Whether the controller has stopped execution. This is set to `true` on an error or after `false` is written to
    /// the [`enabled`][UsbCommand::enabled] field.
    pub host_controller_halted: bool,

    #[bits(1)]
    __: (),

    /// Whether a serious error has been detected, either internal to the controller or during a host system access,
    /// e.g. a PCI parity error.
    ///
    /// See the spec section [4.10.2.6] for more info.
    ///
    /// [4.10.2.6]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A207%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C252%2C0%5D
    pub host_system_error: bool,

    /// Whether any interrupter has a pending interrupt.
    ///
    /// See the spec section [7.1.2] for more info.
    ///
    /// [7.1.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A527%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C556%2C0%5D
    pub event_interrupt: bool,

    /// Set to `true` whenever a port has a change bit (TODO: link) transition from a 0 to a 1.
    /// This indicates whether there has been root hub port activity.
    ///
    /// See the spec section [4.15.2.3] for more info.
    ///
    /// [4.15.2.3]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A288%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C444%2C0%5D
    pub port_change_detect: bool,

    #[bits(3)]
    __: (),

    /// Whether the controller is currently saving its state.
    ///
    /// See the spec section [4.23.2] for more info.
    ///
    /// [4.23.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A348%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C219%2C0%5D
    pub save_state_status: bool,

    /// Whether the controller is currently restoring its state.
    ///
    /// See the spec section [4.23.2] for more info.
    ///
    /// [4.23.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A348%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C219%2C0%5D
    pub restore_state_status: bool,

    /// Whether an error occurred during a save or restore operation.
    ///
    /// See the spec section [4.23.2] for more info.
    ///
    /// [4.23.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A348%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C219%2C0%5D
    pub save_restore_error: bool,

    /// Whether the controller is not ready. While this field is `true`, the OS should not write to any
    /// Doorbell (TODO: link) or [Operational][OperationalRegisters] registers, other than the [`UsbStatus`] register.
    pub controller_not_ready: bool,

    /// Whether an error has occurred which requires the controller to be reinitialised.
    ///
    /// See the spec section [4.24.1] for more info.
    ///
    /// [4.24.1]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A365%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C233%2C0%5D
    pub host_controller_error: bool,

    #[bits(19)]
    __: (),
}

/// Wrapper type to provide methods specific to the [`page_size`][OperationalRegistersFields::page_size] field
#[derive(Clone, Copy)]
pub struct SupportedPageSize(u32);

impl SupportedPageSize {
    /// Gets the page size supported by the device, e.g. a device supporting 4k pages will return 0x1000
    pub fn page_size(&self) -> u64 {
        let v = (self.0 as u64) << 12;

        if !v.is_power_of_two() {
            unimplemented!("Devices supporting multiple page sizes");
        }

        v
    }
}

impl Debug for SupportedPageSize {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("SupportedPageSize")
            .field(&format_args!("{:#x}", self.page_size()))
            .finish()
    }
}

/// Controls the operation of the command ring.
///
/// See the spec section [4.6] for more info.
///
/// [4.6]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A110%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C427%2C0%5D
#[bitfield(u64)]
pub struct CommandRingControl {
    /// Identifies the value of the _Consumer Cycle State_ flag (TODO: link) for the TRB referenced by the Command Ring Pointer.
    /// Writes to this field are ignored if [`command_ring_running`][CommandRingControl::command_ring_running] is `true`.
    ///
    /// See the spec section [4.9.3] for more information.
    ///
    /// [4.9.3]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A185%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C524%2C0%5D
    pub ring_cycle_state: bool,

    /// Writing `true` to this field stops the operation of the command ring after the completion of the currently executing command.
    /// and generate a _Command Completion Event_ with the _Completion Code_ set to _Command Ring Stopped_.
    ///
    /// See the spec section [4.6.1.1] for more info on stopping commands.
    ///
    /// [4.6.1.1]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A112%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C373%2C0%5D
    pub command_stop: bool,

    /// Writing `true` to this field aborts the current command and then stops the operation of the command ring,
    /// generating a _Command Completion Event_ with the _Completion Code_ set to _Command Ring Stopped_.
    /// Note that this will only abort the current command if it is blocked.
    ///
    /// See the spec section [4.6.1.2] for more info on aborting commands.
    ///
    /// [4.6.1.2]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A113%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C658%2C0%5D
    pub command_abort: bool,

    /// Whether the command ring is currently processing commands.
    pub command_ring_running: bool,

    #[bits(2)]
    #[doc(hidden)]
    reserved0: u64,

    /// The top 58 bits of the 64-bit _Command Ring Dequeue Pointer_.
    ///
    /// If the [`command_ring_control`][OperationalRegisters::command_ring_control] register is written to while
    /// [`command_ring_running`][CommandRingControl::command_ring_running] is `false`, the value of this field
    /// is used to fetch the first Command TRB the next time the Host Controller Doorbell register is
    /// written with the DB Reason field set to Host Controller Command, otherwise the internal xHC Command
    /// Ring Dequeue Pointer will be used.
    ///
    /// Reading this field always returns 0.
    #[bits(58)]
    command_ring_pointer_high: u64,
}

impl CommandRingControl {
    /// Returns the [`command_ring_pointer_high`] field.
    ///
    /// [`command_ring_pointer_high`]: CommandRingControl::command_ring_pointer_high
    pub fn command_ring_pointer(self) -> PhysAddr {
        PhysAddr::new(self.command_ring_pointer_high() << 6)
    }

    /// Returns the register with the [`command_ring_pointer_high`][CommandRingControl::command_ring_pointer_high]
    /// field updated with the pointer passed in.
    ///
    /// # Panics
    /// If `pointer` is not 64 byte aligned
    pub fn with_command_ring_pointer(self, pointer: PhysAddr) -> Self {
        assert_eq!(pointer.as_u64() & 0b111111, 0);

        self.with_command_ring_pointer_high(pointer.as_u64() >> 6)
    }
}

/// A pointer to an array of device context structures for the devices attached to the host
#[derive(Debug, Clone, Copy)]
pub struct DeviceContextBaseAddressArrayPointer(u64);

impl DeviceContextBaseAddressArrayPointer {
    /// Constructs a [`DeviceContextBaseAddressArrayPointer`] from the physical pointer.
    ///
    /// # Panics
    /// If the pointer is not 32 byte aligned, as the bottom 5 bits are reserved.
    pub fn from_pointer(pointer: PhysAddr) -> Self {
        assert_eq!(pointer.as_u64() & 0b11111, 0);
        Self(pointer.as_u64())
    }

    /// Gets the pointer contained within the [`DeviceContextBaseAddressArrayPointer`]
    pub fn get_pointer(&self) -> PhysAddr {
        PhysAddr::new(self.0 & !0b11111)
    }
}

#[bitfield(u32)]
pub struct ConfigureRegister {
    /// The maximum number of enabled device slots.
    /// Valid values are in the range 0 to [`max_device_slots`], inclusive.
    /// This field should not be modified while the controller is running ([`enabled`] is `true`).
    ///
    /// [`max_device_slots`]: super::capability_registers::StructuralParameters1::max_device_slots
    /// [`enabled`]: UsbCommand::enabled
    pub max_device_slots_enabled: u8,

    /// Whether the controller should assert the PLC flag when a root hub port transitions to the U3 state.
    ///
    /// See the spec section [4.15.1] for more info.
    ///
    /// [4.15.1]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A285%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C591%2C0%5D
    pub u3_entry_enable: bool,

    /// Whether the extended _Input Control Context_ fields are supported.
    ///
    /// See the spec section [6.2.5.1] for more info.
    ///
    /// [6.2.5.1]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A468%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C694%2C0%5D
    pub config_info_enable: bool,

    #[bits(22)]
    #[doc(hidden)]
    reserved0: u32,
}

/// The operational registers of an XHCI controller.
///
/// See the spec section [5.4] for more info.
///
/// [5.4]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A398%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C302%2C0%5D
#[repr(C)]
#[derive(Clone, Copy)]
struct OperationalRegistersFields {
    /// Writes to this register cause the controller to execute a command
    usb_command: UsbCommand,
    /// Information about the status of the controller
    usb_status: UsbStatus,
    /// What page sizes are supported
    page_size: SupportedPageSize,

    #[doc(hidden)]
    _reserved0: u32,
    #[doc(hidden)]
    _reserved1: u32,

    /// Sets which _Device Notification Transaction Packets_ generate a _Device Notification Event_.
    /// If bit n of this register is set (n <= 15), then _Device Notification Transaction Packets_ with a
    /// _Notification Type_ field of n will trigger a _Device Notification Event_.
    ///
    /// See the spec section [6.4.2.7] for more info.
    ///
    /// [6.4.2.7]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A492%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C348%2C0%5D
    device_notification_control: u32,

    /// Controls the operation of the command ring
    command_ring_control: CommandRingControl,

    #[doc(hidden)]
    _reserved2: u32,
    #[doc(hidden)]
    _reserved3: u32,
    #[doc(hidden)]
    _reserved4: u32,
    #[doc(hidden)]
    _reserved5: u32,

    /// A pointer to an array of device context structures for the devices attached to the host
    device_context_base_address_array_pointer: DeviceContextBaseAddressArrayPointer,
    /// Runtime xHC configuration parameters
    configure: ConfigureRegister,
}

#[rustfmt::skip]
impl Debug for OperationalRegistersFields {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("OperationalRegisters")
            .field("usb_command", &self.usb_command)
            .field("usb_status", &self.usb_status)
            .field("page_size", &self.page_size)
            .field("device_notification_control", &self.device_notification_control)
            .field("command_ring_control", &self.command_ring_control)
            .field("device_context_base_address_array_pointer", &self.device_context_base_address_array_pointer)
            .field("configure", &self.configure)
            .finish()
    }
}

/// A wrapper struct around [`OperationalRegistersFields`] to ensure all reads and writes are volatile
#[derive(Debug)]
pub struct OperationalRegisters {
    /// The address of the registers
    ptr: *mut OperationalRegistersFields,
    /// The number of [`PortRegister`] structures
    max_ports: u8,
}

impl OperationalRegisters {
    /// Wraps the given pointer.
    ///
    /// # Safety
    /// The given pointer must point to the operational registers struct of an xHCI controller.
    /// This function may only be called once per controller.
    /// The passed `capability_registers` must be for the same controller.
    pub unsafe fn new(ptr: VirtAddr, capability_registers: &CapabilityRegisters) -> Self {
        // SAFETY: `ptr` is valid
        let ptr = ptr.as_mut_ptr();

        Self {
            ptr,
            max_ports: capability_registers.structural_parameters_1().max_ports(),
        }
    }
}

// TODO: make these setters unsafe
#[rustfmt::skip]
impl OperationalRegisters {
    volatile_accessors!(
        OperationalRegisters, OperationalRegistersFields, 
        usb_command, UsbCommand,
        (pub fn read_usb_command), (pub fn write_usb_command)
    );
    volatile_accessors!(
        OperationalRegisters, OperationalRegistersFields,
        usb_status, UsbStatus, 
        (pub fn read_usb_status), (pub fn write_usb_status)
    );
    volatile_getter!(
        OperationalRegisters, OperationalRegistersFields, 
        page_size, SupportedPageSize, 
        (pub fn read_page_size)
    );
    volatile_accessors!(
        OperationalRegisters, OperationalRegistersFields, 
        device_notification_control, u32,
        (pub fn read_device_notification_control), (pub fn write_device_notification_control)
    );
    volatile_accessors!(
        OperationalRegisters, OperationalRegistersFields, 
        command_ring_control, CommandRingControl,
        (pub fn read_command_ring_control), (pub fn write_command_ring_control)
    );
    volatile_accessors!(
        OperationalRegisters, OperationalRegistersFields, 
        device_context_base_address_array_pointer, DeviceContextBaseAddressArrayPointer, 
        (pub fn read_device_context_base_address_array_pointer), (pub fn write_device_context_base_address_array_pointer)
    );
    volatile_accessors!(
        OperationalRegisters, OperationalRegistersFields, 
        configure, ConfigureRegister,
        (pub fn read_configure), (pub fn write_configure)
    );
}

impl OperationalRegisters {
    /// Gets the number of port registers the controller has.
    /// This is the largest value where [`port`][OperationalRegisters::port] will return [`Some`]
    pub fn max_ports(&self) -> usize {
        self.max_ports as usize
    }

    /// Gets the [`PortRegister`] at the _1 based_ port number given.
    pub fn port(&self, port_number: usize) -> Option<PortRegister<'_, Immutable>> {
        if port_number != 0 && port_number > self.max_ports as usize {
            None
        } else {
            // SAFETY: `port_number` is not greater than `max_ports`, so this pointer is valid
            unsafe {
                Some(PortRegister::new(
                    self.ptr.byte_add(0x400 + 0x10 * (port_number - 1)).cast(),
                ))
            }
        }
    }

    /// Gets the [`PortRegister`] at the _1 based_ port number given.
    pub fn port_mut(&mut self, port_number: usize) -> Option<PortRegister<'_, Mutable>> {
        if port_number != 0 && port_number > self.max_ports as usize {
            None
        } else {
            // SAFETY: `port_number` is not greater than `max_ports`, so this pointer is valid
            unsafe {
                Some(PortRegister::new_mut(
                    self.ptr.byte_add(0x400 + 0x10 * (port_number - 1)).cast(),
                    self,
                ))
            }
        }
    }

    /// Get an iterator over the ports
    pub fn ports(&self) -> impl Iterator<Item = PortRegister<'_, Immutable>> {
        // SAFETY: Each port is only produced once, so it is not possible to create two `PortRegister`
        // structs for the same port. `PortRegister` contains a phantom mutable reference to the
        (1..self.max_ports as usize).map(|port_number| unsafe {
            PortRegister::new(self.ptr.byte_add(0x400 + 0x10 * (port_number - 1)).cast())
        })
    }

    /// Get an iterator over mutable the ports
    pub fn ports_mut(&mut self) -> impl Iterator<Item = PortRegister<'_, Mutable>> {
        // SAFETY: Each port is only produced once, so it is not possible to create two `PortRegister`
        // structs for the same port. `PortRegister` contains a phantom mutable reference to the
        (1..self.max_ports as usize).map(|port_number| unsafe {
            PortRegister::new_mut(
                self.ptr.byte_add(0x400 + 0x10 * (port_number - 1)).cast(),
                self,
            )
        })
    }

    /// Reads the fields of the register and prints them in a debug format
    pub fn debug(&self) {
        let fields = OperationalRegistersFields {
            usb_command: self.read_usb_command(),
            usb_status: self.read_usb_status(),
            page_size: self.read_page_size(),
            _reserved0: 0,
            _reserved1: 0,
            device_notification_control: self.read_device_notification_control(),
            command_ring_control: self.read_command_ring_control(),
            _reserved2: 0,
            _reserved3: 0,
            _reserved4: 0,
            _reserved5: 0,
            device_context_base_address_array_pointer: self
                .read_device_context_base_address_array_pointer(),
            configure: self.read_configure(),
        };

        println!("{fields:#?}");

        for (i, port) in self.ports().enumerate() {
            print!("Port number {}: ", i + 1);
            if port.read_status_and_control().device_connected() {
                port.debug();
            } else {
                println!("no device connected");
            }
        }
    }
}

/// Tests that the field offsets of [`OperationalRegisters`] matches the xHCI spec,
/// so that values are read correctly.
#[rustfmt::skip]
#[test_case]
fn test_xhci_operational_field_offsets() {
    use core::mem::offset_of;

    assert_eq!(offset_of!(OperationalRegistersFields, usb_command), 0x00);
    assert_eq!(offset_of!(OperationalRegistersFields, usb_status), 0x04);
    assert_eq!(offset_of!(OperationalRegistersFields, page_size), 0x08);

    assert_eq!(offset_of!(OperationalRegistersFields, device_notification_control), 0x14);
    assert_eq!(offset_of!(OperationalRegistersFields, command_ring_control), 0x18);

    assert_eq!(offset_of!(OperationalRegistersFields, device_context_base_address_array_pointer), 0x30);
    assert_eq!(offset_of!(OperationalRegistersFields, configure), 0x38);
}
