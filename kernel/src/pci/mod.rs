//! Functionality for reading and driving the PCI bus

mod bar;
mod capability_registers;
mod classcodes;
mod devices;
mod drivers;
mod msi;
mod registers;

use acpica_bindings::types::tables::mcfg::Mcfg;
use alloc::sync::Arc;
use alloc::{collections::VecDeque, vec::Vec};
use core::mem::size_of;
use log::debug;
use x86_64::structures::paging::frame::PhysFrameRange;
use x86_64::structures::paging::page::PageRange;
use x86_64::structures::paging::{Page, PhysFrame};
use x86_64::PhysAddr;

use crate::global_state::KERNEL_STATE;
use crate::print;
use crate::scheduler::Task;
use crate::{global_state::GlobalState, println};
use devices::*;
use registers::HeaderType;
use registers::PciHeader;

use self::classcodes::{ClassCode, SerialBusControllerType};
use self::drivers::usb::xhci::XhciController;
use self::registers::PciDeviceId;

/// A mapping into the PCIe configuration space of a PCI device.
/// When this struct is dropped, the mapping is deleted.
#[derive(Debug)]
pub struct PcieMappedRegisters {
    /// The page where the function's config registers are mapped
    page: Page,
    /// The physical address of the registers
    phys_frame: PhysFrame,
}

impl PcieMappedRegisters {
    /// Constructs a new mapping from the given page
    ///
    /// # Safety
    /// * `phys_frame` must be the configuration registers of a PCIe device
    unsafe fn new(phys_frame: PhysFrame) -> Self {
        // SAFETY: No other code is using these registers
        let pages = unsafe {
            KERNEL_STATE
                .physical_memory_accessor
                .lock()
                .map_frames(PhysFrameRange {
                    start: phys_frame,
                    end: phys_frame + 1,
                })
        };

        Self {
            page: pages.start,
            phys_frame,
        }
    }

    /// Gets a pointer to the start of the configuration space
    unsafe fn as_ptr<T>(&self) -> *const T {
        self.page.start_address().as_ptr()
    }

    /// Gets a mutable pointer to the start of the configuration space
    unsafe fn as_mut_ptr<T>(&self) -> *mut T {
        self.page.start_address().as_mut_ptr()
    }

    /// Reads the register at the given offset into the configuration space.
    /// `register` is in registers, i.e. 4-byte multiples, **not** in bytes.
    ///
    /// # Safety
    /// * The caller is responsible for managing any side effects this read may have.
    unsafe fn read_reg(&self, register: u8) -> u32 {
        // SAFETY: Side-effects are the caller's responsibility
        unsafe { self.as_ptr::<u32>().add(register.into()).read_volatile() }
    }

    /// Reads the register at the given offset into the configuration space.
    /// `register` is in registers, i.e. 4-byte multiples, **not** in bytes.
    ///
    /// # Safety
    /// * The caller is responsible for managing any side effects this write may have.
    unsafe fn write_reg(&self, register: u8, value: u32) {
        // SAFETY: Side-effects are the caller's responsibility
        unsafe {
            self.as_mut_ptr::<u32>()
                .add(register.into())
                .write_volatile(value)
        }
    }

    /// Writes to a register and then reads the value back, before resetting the register to its initial value.
    /// The return value is the read value
    ///
    /// This is useful to calculate the size of a BAR, by seeing which bits are able to be set by software.
    ///
    /// # Safety
    /// This method will briefly change the value of the register.
    /// It is the caller's responsibility to ensure that any side-effects of this write are sound.  
    unsafe fn write_and_reset(&self, register: u8, value: u32) -> u32 {
        // SAFETY: Reading registers doesn't have side effects
        let initial_value = unsafe { self.read_reg(register) };
        // SAFETY: The caller guarantees this is sound
        unsafe { self.write_reg(register, value) }
        // SAFETY: Reading registers doesn't have side effects
        let new_value = unsafe { self.read_reg(register) };
        // SAFETY: The caller guarantees this is sound. This is the same value as the register held originally.
        unsafe { self.write_reg(register, initial_value) };

        new_value
    }
}

impl Drop for PcieMappedRegisters {
    fn drop(&mut self) {
        let pages = PageRange {
            start: self.page,
            end: self.page + 1,
        };

        // SAFETY: `addr` is the start address
        unsafe {
            KERNEL_STATE
                .physical_memory_accessor
                .lock()
                .unmap_frames(pages)
        };
    }
}

/// A cached header of a [`PciFunction`]
#[derive(Debug, Clone)]
pub struct PciMappedFunction {
    /// The PCI segment which this function is in
    segment: u16,
    /// The function for which this is a cache
    function: PciFunction,

    /// The ID of the device
    id: PciDeviceId,

    /// The type of device
    class_code: ClassCode,

    registers: Arc<PcieMappedRegisters>,
}

/// A cache of the [`functions`][PciFunction] of a [`PciDevice`]
#[derive(Debug)]
struct PciDeviceCache {
    /// The device for which this is a cache
    device: PciDevice,
    /// The cached functions
    functions: Vec<PciMappedFunction>,
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
struct PciSegmentCache {
    /// The PCIe controller for which this is a cache
    controller: PcieController,

    /// The segment's buses.
    /// This vector is unsorted, so bus `n` is not guaranteed to be at index `n`.
    /// To find a bus cache by its bus number, use the [`get_bus`] method.
    ///
    /// [`get_bus`]: PciSegmentCache::get_bus
    buses: Vec<PciBusCache>,
}

/// A cache of all the PCI devices discovered on the system
#[derive(Debug)]
struct PciCache {
    /// The system's segments
    segments: Vec<PciSegmentCache>,
}

impl PciMappedFunction {
    /// Reads the register at the given offset into the configuration space.
    /// `register` is in registers, i.e. 4-byte multiples, **not** in bytes.
    ///
    /// # Safety
    /// * The caller is responsible for managing any side effects this read may have.
    unsafe fn read_reg(&self, register: u8) -> u32 {
        // SAFETY: Side-effects are the caller's responsibility
        unsafe { self.registers.read_reg(register) }
    }

    /// Reads the register at the given offset into the configuration space.
    /// `register` is in registers, i.e. 4-byte multiples, **not** in bytes.
    ///
    /// # Safety
    /// * The caller is responsible for managing any side effects this write may have.
    unsafe fn write_reg(&self, register: u8, value: u32) {
        // SAFETY: Side-effects are the caller's responsibility
        unsafe { self.registers.write_reg(register, value) }
    }

    /// Reads the device's PCI header.
    fn read_header(&self) -> Result<Option<PciHeader>, classcodes::InvalidValueError> {
        let mut registers = [0; 17];

        for (i, register) in registers.iter_mut().enumerate() {
            // SAFETY: Reading from PCI header registers shouldn't have side-effects.
            *register = unsafe { self.read_reg(i as _) };
        }

        PciHeader::from_registers(registers, &self.function)
    }
}

impl PciDeviceCache {
    /// Gets the [`PciMappedFunction`] for the given function, if present.
    fn get_function(&self, function: u8) -> Option<&PciMappedFunction> {
        self.functions
            .iter()
            .find(|function_cache| function_cache.function.get_function_number() == function)
    }

    /// Gets the [`PciMappedFunction`] for the given function, if present.
    fn get_function_mut(&mut self, function: u8) -> Option<&mut PciMappedFunction> {
        self.functions
            .iter_mut()
            .find(|function_cache| function_cache.function.get_function_number() == function)
    }
}

impl PciBusCache {
    /// Gets the [`PciDeviceCache`] for the given device, if present.
    fn get_device(&self, device: u8) -> Option<&PciDeviceCache> {
        self.devices
            .iter()
            .find(|device_cache| device_cache.device.get_device_number() == device)
    }

    /// Gets the [`PciDeviceCache`] for the given device, if present.
    fn get_device_mut(&mut self, device: u8) -> Option<&mut PciDeviceCache> {
        self.devices
            .iter_mut()
            .find(|device_cache| device_cache.device.get_device_number() == device)
    }
}

impl PciSegmentCache {
    /// Gets an iterator over the functions
    fn functions(&self) -> impl Iterator<Item = &PciMappedFunction> {
        self.buses
            .iter()
            .flat_map(|b| b.devices.iter().flat_map(|d| d.functions.iter()))
    }

    /// Gets an iterator over the functions
    fn functions_mut(&mut self) -> impl Iterator<Item = &mut PciMappedFunction> {
        self.buses
            .iter_mut()
            .flat_map(|b| b.devices.iter_mut().flat_map(|d| d.functions.iter_mut()))
    }

    /// Gets the [`PciBusCache`] for the given bus number, if present.
    fn get_bus(&self, bus: u8) -> Option<&PciBusCache> {
        self.buses.iter().find(|bus_cache| bus_cache.bus == bus)
    }
}

impl PciCache {
    /// Gets an iterator over the functions in the cache
    fn functions(&self) -> impl Iterator<Item = &PciMappedFunction> {
        self.segments.iter().flat_map(|b| b.functions())
    }

    /// Gets an iterator over mutable references to the functions in the cache
    fn functions_mut(&mut self) -> impl Iterator<Item = &mut PciMappedFunction> {
        self.segments.iter_mut().flat_map(|b| b.functions_mut())
    }

    /// Gets the cache for a specific segment, if present.
    fn get_segment(&self, segment: u16) -> Option<&PciSegmentCache> {
        self.segments
            .iter()
            .find(|segment_cache| segment_cache.controller.segment == segment)
    }
}

/// Maps the configuration space of a device on a given PCIe controller into virtual memory.
///
/// # Safety
/// * `controller` must represent a real controller on the system
/// * No existing [`PcieMappedRegisters`] may exist for this function
/// * The returned physical page must be used soundly
///     (only volatile reads / writes are performed, side effects of any accesses are taken into account, etc)
unsafe fn map_pci_registers(
    controller: &PcieController,
    function: PciFunction,
) -> PcieMappedRegisters {
    let phys_addr = function.get_register_address(controller.min_bus, controller.address);
    let start = PhysFrame::containing_address(phys_addr);

    // SAFETY: This is the frame of a PCIe device's config registers
    unsafe { PcieMappedRegisters::new(start) }
}

/// Scans a function on a given PCIe controller and caches the results
///
/// # Safety
/// * `controller` must represent a real controller on the system
unsafe fn scan_function(
    controller: &PcieController,
    function: PciFunction,
) -> Option<PciMappedFunction> {
    // SAFETY: `controller` is a real controller, all reads below are volatile
    let registers = unsafe { map_pci_registers(controller, function) };

    let (vendor_id, device_id) =
    // SAFETY: Reading from PCI header registers shouldn't have side-effects.
        unsafe { split_to_u16(registers.as_ptr::<u32>().read_volatile()) };

    if vendor_id == 0xffff {
        None
    } else {
        // SAFETY: Reading from PCI header registers shouldn't have side-effects
        let value = unsafe { registers.as_ptr::<u32>().add(2).read_volatile() };

        // let (class_code, subclass, prog_if, _) = split_to_u8(value);
        let (_, prog_if, subclass, class_code) = split_to_u8(value);

        let class_code = ClassCode::new(class_code, subclass, prog_if)
            .expect("PCI device should have had a valid class code");

        Some(PciMappedFunction {
            segment: controller.segment,
            function,
            registers: Arc::new(registers),
            id: PciDeviceId {
                vendor: vendor_id,
                device: device_id,
            },
            class_code,
        })
    }
}

/// Enumerates the functions of a [`PciDevice`] and records their info.
/// Also returns the IDs of any PCI buses which this device is a bridge to.
///
/// # Safety
/// * `controller` must represent a real controller on the system
unsafe fn scan_device(
    controller: &PcieController,
    device: PciDevice,
) -> Option<(PciDeviceCache, Vec<u8>)> {
    let mut functions = Vec::new();
    let mut buses = Vec::new();

    // Scan the device's functions
    for function in 0..8 {
        // SAFETY: `controller` is a real controller
        let f = unsafe { scan_function(controller, device.function(function).unwrap()) };

        if let Some(cache) = f {
            let Ok(Some(header)) = cache.read_header() else {
                panic!("Invalid header on a PCI device")
            };

            if let HeaderType::PciToPciBridge(h) = header.header_type {
                buses.push(h.secondary_bus_number)
            }

            let is_multifunction = header.is_multifunction;

            functions.push(cache);

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
///
/// # Safety
/// * `controller` must represent a real controller on the system
unsafe fn scan_bus(controller: &PcieController, bus: u8) -> (PciBusCache, Vec<u8>) {
    let (devices, buses_iter): (_, Vec<Vec<u8>>) = (0..32)
        // SAFETY: `controller` represents a real controller
        .filter_map(|device| unsafe {
            scan_device(
                controller,
                PciDevice::new(controller.segment, bus, device).unwrap(),
            )
        })
        .unzip();

    let buses = buses_iter.into_iter().flatten().collect();

    (PciBusCache { bus, devices }, buses)
}

/// Enumerates the system's PCI devices and prints info about them
pub fn lspci(args: &[&str]) {
    let is_verbose = args.contains(&"-v");

    PCI_CACHE.lock().functions().for_each(|function_cache| {
        let header = function_cache.read_header().unwrap().unwrap();

        print!("{:04x}:", function_cache.segment);
        print!("{}  ", function_cache.function);
        print!("{}  ", header.device_code);
        print!("{:?}", header.class_code);
        println!();

        if is_verbose {
            println!(
                "  Mapped at {:#x}",
                function_cache.registers.page.start_address()
            );

            if let Some(capabilities) = function_cache.capabilities() {
                println!("  Capabilities:");
                for (c, _) in capabilities {
                    println!("    {c:?}");
                }
            }
        }
    });
}

/// A cache of the system's PCI devices
static PCI_CACHE: GlobalState<PciCache> = GlobalState::new();

/// A PCI root bus controller
#[derive(Debug)]
pub struct PcieController {
    /// The PCI segment which this controller manages
    segment: u16,
    /// The physical address of the controller's registers
    address: PhysAddr,
    /// The lowest bus number the controller manages
    min_bus: u8,
    /// The highest bus number the controller manages
    max_bus: u8,
}

/// Initialises the PCIe bus
///
/// # Safety:
/// * This function may only be called once
/// * `mcfg` must accurately describe the system
pub unsafe fn init(mcfg: Mcfg) {
    let segments: Vec<_> = mcfg
        .records()
        .map(|r| {
            let controller = PcieController {
                segment: r.segment,
                address: r.base_address.into(),
                min_bus: r.min_bus_number,
                max_bus: r.max_bus_number,
            };

            let mut buses = Vec::new();
            let mut to_scan = VecDeque::from([controller.min_bus]);

            while let Some(bus) = to_scan.pop_front() {
                // SAFETY: `controller` was described by a valid MCFG, so it is valid.
                let (bus_cache, subordinates) = unsafe { scan_bus(&controller, bus) };
                buses.push(bus_cache);
                for subordinate in subordinates {
                    to_scan.push_back(subordinate)
                }
            }

            PciSegmentCache { controller, buses }
        })
        .collect();

    PCI_CACHE.init(PciCache { segments });

    let mut lock = PCI_CACHE.lock();

    for function in lock.functions_mut() {
        let header = function.read_header().unwrap().unwrap();

        if let Some(capabilities) = function.capabilities() {
            for (c, i) in capabilities {
                debug!("{c:?}, {i}");
            }
        }

        if let classcodes::ClassCode::SerialBusController(SerialBusControllerType::UsbController(
            classcodes::USBControllerType::Xhci,
        )) = header.class_code
        {
            // SAFETY: This function may only be called once, and `PCI_CACHE.lock().functions()`
            // produces each function only once, so `XhciController::new` will only be called once per function.
            let task = unsafe { XhciController::init(function.clone()) };

            Task::register(task);
        }
    }
}

/// Gets a pointer into the PCIe configuration space of the given PCI device
///
/// # Panics
/// * If `offset + size_of::<T>() >= 4096` - i.e. if the pointer would extend beyond the device's PCI configuration space.
///
/// # Safety
/// * All accesses to the returned pointer should be volatile
/// * Writes to the pointer may have side effects for the hardware - these must be taken into account
pub unsafe fn get_pci_ptr<T>(
    segment: u16,
    bus: u8,
    device: u8,
    function: u8,
    offset: u16,
) -> *mut T {
    // Check that the type is completely within the configuration space of the device
    assert!(offset as usize + size_of::<T>() <= 4096);

    let cache = PCI_CACHE.lock();
    let function_cache = cache
        .get_segment(segment)
        .unwrap()
        .get_bus(bus)
        .unwrap()
        .get_device(device)
        .unwrap()
        .get_function(function)
        .unwrap();

    // SAFETY: `offset + size_of::<T>()` is less than 4096, so the pointer stays within the same object, and can't wrap
    unsafe {
        function_cache
            .registers
            .as_mut_ptr::<T>()
            .byte_add(offset.into())
    }
}

/// Splits a [`u32`] into two [`u16`]s.
/// The less significant [`u16`] is returned first.
fn split_to_u16(value: u32) -> (u16, u16) {
    (value as u16, (value >> 16) as u16)
}

/// Splits a [`u32`] into four [`u8`]s.
/// The least significant [`u8`]s are returned first.
fn split_to_u8(value: u32) -> (u8, u8, u8, u8) {
    (
        value as u8,
        (value >> 8) as u8,
        (value >> 16) as u8,
        (value >> 24) as u8,
    )
}
