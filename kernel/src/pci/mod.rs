//! Functionality for reading and driving the PCI bus

mod bar;
mod classcodes;
mod devices;
mod drivers;
mod registers;

use alloc::{collections::VecDeque, vec::Vec};

use crate::scheduler::Task;
use crate::{global_state::GlobalState, println};
use devices::*;
use registers::HeaderType;
use registers::PciHeader;

use self::classcodes::SerialBusControllerType;
use self::drivers::usb::xhci::XhciController;

/// A cached header of a [`PciFunction`]
#[derive(Debug)]
struct PciFunctionCache {
    /// The function for which this is a cache
    function: PciFunction,
    /// The cached header
    header: PciHeader,
}

/// A cache of the [`functions`][PciFunction] of a [`PciDevice`]
#[derive(Debug)]
struct PciDeviceCache {
    /// The device for which this is a cache
    device: PciDevice,
    /// The cached functions
    functions: Vec<PciFunctionCache>,
}

/// A cache of the [`devices`][PciDevice] on a PCI bus
#[derive(Debug)]
struct PciBusCache {
    /// The bus number for which this is a cache
    bus: u8,
    /// The cached devices
    devices: Vec<PciDeviceCache>,
}

/// A cache of the system's PCI devices
#[derive(Debug)]
struct PciCache {
    /// The system's buses.
    /// This vector is unsorted, so bus `n` is not guaranteed to be at index `n`.
    /// To find a bus cache by its bus number, use the [`bus`][PciCache::get_bus] method.
    buses: Vec<PciBusCache>,
}

impl PciCache {
    /// Gets an iterator over the functions
    fn functions(&self) -> impl Iterator<Item = &PciFunctionCache> {
        self.buses
            .iter()
            .flat_map(|b| b.devices.iter().flat_map(|d| d.functions.iter()))
    }

    /// Gets the [`PciBusCache`] for the given bus number, if present.
    fn get_bus(&self, bus: u8) -> Option<&PciBusCache> {
        self.buses.iter().find(|bus_cache| bus_cache.bus == bus)
    }
}

/// Scans a specific [`PciFunction`]
fn scan_function(function: PciFunction) -> Option<PciFunctionCache> {
    let header = function.get_header().unwrap()?;

    Some(PciFunctionCache { function, header })
}

/// Enumerates the functions of a [`PciDevice`] and records their info.
/// Also returns the IDs of any PCI buses which this device is a bridge to
fn scan_device(device: PciDevice) -> Option<(PciDeviceCache, Vec<u8>)> {
    let mut functions = Vec::new();
    let mut buses = Vec::new();

    // Scan the device's functions
    for function in 0..8 {
        if let Some(header) = scan_function(device.function(function).unwrap()) {
            if let HeaderType::PciToPciBridge(h) = header.header.header_type {
                buses.push(h.secondary_bus_number)
            }

            let is_multifunction = header.header.is_multifunction;

            functions.push(header);

            // If the device is not multifunction, don't check the other functions
            if function == 0 && !is_multifunction {
                break;
            }
        } else {
            break;
        }
    }

    Some((PciDeviceCache { device, functions }, buses))
}

/// Checks all the devices on the PCI bus with the given bus id and prints their info
fn scan_bus(bus: u8) -> (PciBusCache, Vec<u8>) {
    let (devices, buses_iter): (_, Vec<Vec<u8>>) = (0..32)
        .filter_map(|device| scan_device(PciDevice::new(bus, device).unwrap()))
        .unzip();
    let buses = buses_iter.into_iter().flatten().collect();

    (PciBusCache { bus, devices }, buses)
}

/// Scans the whole PCI configuration space, constructing a [`PciCache`]
fn scan_pci() -> PciCache {
    let mut buses = Vec::new();
    let mut to_scan = VecDeque::from([0]);

    while let Some(bus) = to_scan.pop_front() {
        let (bus_cache, subordinates) = scan_bus(bus);
        buses.push(bus_cache);
        for subordinate in subordinates {
            to_scan.push_back(subordinate)
        }
    }

    PciCache { buses }
}

/// Enumerates the system's PCI devices and prints info about them
pub fn lspci() {
    PCI_CACHE.lock().functions().for_each(|function_cache| {
        println!(
            "{} {:?}   {:?}",
            function_cache.function,
            function_cache.header.device_code,
            function_cache.header.class_code
        );
    });
}

/// A cache of the system's PCI devices
static PCI_CACHE: GlobalState<PciCache> = GlobalState::new();

/// Initialises the PCI bus
///
/// # Safety:
/// This function may only be called once
pub unsafe fn init() {
    PCI_CACHE.init(scan_pci());

    for function in PCI_CACHE.lock().functions() {
        if let classcodes::ClassCode::SerialBusController(SerialBusControllerType::UsbController(
            classcodes::USBControllerType::Xhci,
        )) = function.header.class_code
        {
            // SAFETY: This function may only be called once, and `PCI_CACHE.lock().functions()`
            // produces each function only once, so `XhciController::new` will only be called once per function. 
            let task = unsafe { XhciController::init(function.function) };

            Task::register(task);
        }
    }

    // println!("{:#?}", *PCI_CACHE.lock());
}
