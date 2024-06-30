//! The [`setup_msi`] method on [`PciMappedFunction`]
//!
//! [`setup_msi`]: PciMappedFunction::setup_msi

use log::debug;

use crate::{
    cpu::interrupt_controllers::current_apic_id,
    global_state::KERNEL_STATE,
    pci::{
        bar::BarValue,
        capability_registers::{
            msix::MsixInterruptArray, CapabilityEntry, MsixTableEntry, MsixVectorControl, X64MsiAddress, X64MsiDeliveryMode, X64MsiTriggerMode
        },
    }, util::generic_mutability::Mutable,
};

use super::{
    bar::Bar,
    capability_registers::{self, msix::MsixCapability},
    PciMappedFunction,
};

/// An error which can occur when initialising MSI for a PCI device
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsiInitError {
    /// There was an error reading the device's PCI header
    HeaderReadError,
    /// The device does not support MSI - there was no MSI or MSI-X capability found
    NoMsiSupport,
}

impl PciMappedFunction {
    /// Sets up MSI or MSI-X interrupts for the device, if supported.
    ///
    /// # Arguments
    /// `f` is a closure which returns the [`Bar`] at the given bar number.
    /// This function is only called when using MSI-X, not MSI -  if the caller knows that the device uses only MSI, this closure can panic.
    /// This is used rather than this method constructing the [`Bar`] itself because it is unsound for two [`Bar`] objects to exist
    /// pointing to the same bar at the same time. It is possible that the BAR used by MSI-X is already being used by the caller for other
    /// features of the device. In this case, the same [`Bar`] object must be returned.
    ///
    /// # Safety
    /// * `f` must return the [`Bar`] for the BAR number (not register index) passed to it, on this device
    pub unsafe fn setup_msi<'a, F>(&'a mut self, f: F) -> Result<(), MsiInitError>
    where
        F: FnOnce(u8) -> &'a Bar<'a>,
    {
        let Ok(Some(header)) = self.read_header() else {
            return Err(MsiInitError::HeaderReadError);
        };

        assert!(
            header.status.has_capabilities_list(),
            "No MSI support on XHCI controller"
        );

        // The interrupt vector which the controller will send interrupts to
        let vector = 0xAA; // TODO: proper MSI vector allocation

        'found_msi: {
            for (c, i) in self.capabilities_mut().unwrap() {
                debug!("{c:?}, {i:?}");

                match c {
                    CapabilityEntry::MessageSignalledInterrupts(msi) => {
                        // SAFETY: TODO once vector allocation is done properly
                        unsafe {
                            setup_msi_standard(msi, vector)?;
                        }
                        break 'found_msi;
                    }
                    CapabilityEntry::MsiX(msix) => {
                        // SAFETY: TODO once vector allocation is done properly
                        unsafe {
                            setup_msix(msix, f, vector)?;
                        }
                        break 'found_msi;
                    }
                    _ => (),
                }
            }

            return Err(MsiInitError::NoMsiSupport);
        }

        // SAFETY: This sets the 'bus master' bit of the command register, which allows the device to make memory accesses
        unsafe {
            let status_and_command = self.read_reg(1);
            debug!("status and command: {status_and_command:#x}");
            self.write_reg(1, status_and_command | (1 << 2) | (1 << 10));

            assert!(self
                .read_header()
                .unwrap()
                .unwrap()
                .command
                .can_be_bus_master());
            assert!(self
                .read_header()
                .unwrap()
                .unwrap()
                .command
                .interrupts_disabled());
        }

        Ok(())
    }
}

/// Sets up MSI-X for a device.
///
/// # Safety
/// * This function will overwrite whatever MSI configuration is already present
/// * See [`setup_msi`][PciMappedFunction::setup_msi] for safety conditions related to `f`.
/// * The caller must make sure that the interrupt handler for `vector` is set up for this device.
unsafe fn setup_msix<'a, F>(
    mut msix: MsixCapability<'_, Mutable>,
    f: F,
    vector: u8,
) -> Result<(), MsiInitError>
where
    F: FnOnce(u8) -> &'a Bar<'a>,
{
    debug!("{:?}, {:?}", msix.interrupt_table(), msix.pending_bits());

    let (bir, offset) = msix.interrupt_table();
    let bar = f(bir);
    let bar_size = bar.get_size();

    let BarValue::MemorySpace { base_address, .. } = bar.read_value() else {
        panic!("MSI-X BARs must be memory space")
    };

    let last_index = msix.control().last_index().into();

    let remaining_bar_space = bar_size - offset as u64;
    let needed_space = (last_index as u64 + 1) * 16;

    debug!(
        "Physical address of BAR is {:#x}",
        base_address.as_address()
    );

    debug!(
        "16 bytes * {:#x} entries = {:#x} bytes, remaining BAR length is {:#x}",
        last_index + 1,
        needed_space,
        remaining_bar_space
    );

    assert!(remaining_bar_space >= needed_space);

    // SAFETY: The base address and length were read from a BAR, so MMIO exists for this address range.
    // `address` is the physical address of the MSI-X vector table. The caller guarantees that setting up the table is valid.
    // The `ptr` is only used in the closure.
    unsafe {
        KERNEL_STATE.physical_memory_accessor.lock().with_mapping(
            base_address.as_address() + offset as u64,
            needed_space.try_into().unwrap(),
            |ptr| {
                debug!("Array mapped at {ptr:p}");

                // SAFETY: `ptr` points to the interrupt table.
                // `last_index` is the last index.
                let mut array = MsixInterruptArray::new(ptr.cast(), last_index);

                for i in 0..=last_index {
                    let (address, data) = X64MsiAddress {
                        apic_id: current_apic_id().unwrap().try_into().unwrap(),
                        redirection_hint: false,
                        destination_is_logical: true,
                        delivery_mode: X64MsiDeliveryMode::Fixed,
                        trigger_mode: X64MsiTriggerMode::Edge,
                        vector,
                    }
                    .to_address_and_data();

                    // SAFETY: The interrupt handler for `vector` is set up, so this interrupt will be received properly
                    array.write(
                        i,
                        MsixTableEntry {
                            message_address_low: address,
                            message_address_high: 0,
                            message_data: data.into(),
                            vector_control: MsixVectorControl::new().with_masked(false),
                        },
                    );
                }
            },
        );
    }

    let control = msix.control();

    // SAFETY: All interrupts are enabled and set up to point to `vector`
    unsafe {
        msix.write_control(control.with_enable(true).with_function_mask(false));
    }

    Ok(())
}

/// Sets up MSI for a device.
///
/// # Safety
/// * This function will overwrite whatever MSI configuration is already present
/// * The caller must make sure that the interrupt handler for `vector` is set up for this device.
unsafe fn setup_msi_standard(
    mut msi: capability_registers::MessageSignalledInterruptsCapability<'_, Mutable>,
    vector: u8,
) -> Result<(), MsiInitError> {
    msi.write_address_x64(X64MsiAddress {
        apic_id: current_apic_id().unwrap().try_into().unwrap(),
        redirection_hint: false,
        destination_is_logical: false,
        delivery_mode: X64MsiDeliveryMode::Fixed,
        trigger_mode: X64MsiTriggerMode::Edge,
        vector,
    });

    debug!("{msi:?}");
    Ok(())
}
