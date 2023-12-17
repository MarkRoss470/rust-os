//! Functionality for reading and writing Base Address Registers (BARs)

use core::fmt::Debug;

use x86_64::{
    structures::paging::{frame::PhysFrameRange, PhysFrame},
    PhysAddr,
};

use super::{devices::PciRegister, PciMappedFunction, PcieController};
use crate::println;

/// The address of a region in memory used by the PCI device
#[derive(Clone, Copy)]
pub enum MemorySpaceBarBaseAddress {
    /// A 32-bit base address
    Small(u32),
    /// A 64-bit base address
    Large(u64),
}

impl Debug for MemorySpaceBarBaseAddress {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Small(arg0) => f
                .debug_tuple("Small")
                .field(&format_args!("{arg0:#x}"))
                .finish(),
            Self::Large(arg0) => f
                .debug_tuple("Large")
                .field(&format_args!("{arg0:#x}"))
                .finish(),
        }
    }
}

/// A Base Address Representation (BAR) - a pointer to a memory location used by a PCI device
#[derive(Debug, Clone, Copy)]
pub enum BarValue {
    /// The BAR is the physical address af a memory region used by the PCI device.
    /// This means the BAR has to live in physical memory.
    MemorySpace {
        /// The address of the memory region
        base_address: MemorySpaceBarBaseAddress,
        /// Whether the memory region is prefetchable. If a BAR is prefetchable,
        /// the CPU is allowed to combine reads and writes to the memory region.
        prefetchable: bool,
    },

    /// The BAR is a port number
    /// TODO: figure out what's up with these
    #[allow(dead_code)]
    IOSpace {
        /// The port number (I think) of the device
        base_address: u32,
    },
}

/// A specific base address register of a PCI device
pub struct Bar<'a> {
    function: &'a PciMappedFunction,
    /// The register which this BAR is in
    register: u8,
}

impl<'a> Bar<'a> {
    /// # Safety
    /// * The passed `register` must be part of a BAR. If the BAR is 64-bit,
    ///     it must be the register with the lower offset.
    /// * Only one [`Bar`] struct may exist for each BAR at one time. 
    ///     No other code may access the BAR while this struct exists.
    pub unsafe fn new(function: &'a PciMappedFunction, register: u8) -> Self {
        Self { function, register }
    }

    /// Reads the value of the BAR
    pub fn read_value(&self) -> BarValue {
        // SAFETY: This struct is unsafe to construct from a PciRegister which is not a BAR
        let lower_32 = unsafe { self.function.read_reg(self.register) };
        let prefetchable = lower_32 & (1 << 3) != 0;
        let bar_type = (lower_32 >> 1) & 0b11;

        match bar_type {
            // 32-bit BAR
            0x00 => BarValue::MemorySpace {
                base_address: MemorySpaceBarBaseAddress::Small(lower_32 & (!0b1111)),
                prefetchable,
            },

            0x01 => unimplemented!("16 bit BARs"),

            // 64-bit BAR, spread over two PCI registers
            0x02 => {
                // SAFETY: This struct is unsafe to construct from a PciRegister which is not a BAR,
                // And any BAR with a type of 0x02 is guaranteed for the next register to be part of the same BAR.
                let upper_32 = unsafe { self.function.read_reg(self.register + 1) };

                // println!("Second BAR: 0b{upper_32:b}");

                BarValue::MemorySpace {
                    base_address: MemorySpaceBarBaseAddress::Large(
                        (upper_32 as u64) << 32 | lower_32 as u64 & (!0b1111),
                    ),
                    prefetchable,
                }
            }

            t => panic!("Invalid BAR type {t}"),
        }
    }

    /// Gets the size of the BAR
    pub fn get_size(&self) -> u64 {
        const STATUS_AND_COMMAND_REGISTER: u8 = 1;

        // Disable both IO space and memory space accesses while performing all 1s write
        // to prevent it from being misinterpreted

        // SAFETY: Reads from PCI configuration registers shouldn't have side effects
        let previous_command = unsafe {
            // Take only the bottom 2 bytes because the top 2 bytes are the status register
            self.function.read_reg(STATUS_AND_COMMAND_REGISTER) & 0xffff
        };

        // SAFETY: This write sets the Memory Space and I/O Space bits of the command register to 0.
        // This disables memory and IO space accesses.
        // This operation is sound because the bits are reset at the end of the method.
        unsafe {
            self.function
                .write_reg(STATUS_AND_COMMAND_REGISTER, previous_command & !0b11);
        }

        // SAFETY: memory and IO space accesses were disabled above, so this write can't have side effects.
        let value_after_write = unsafe { self.function.write_and_reset(self.register, u32::MAX) };

        // Mask out the BAR's flag bits
        let masked_address = value_after_write & !0b1111;

        // SAFETY: This only restores the value that was previously in the command register.
        // This write also writes all 0s to the status register,
        // but all the bits in that register are either read only or RW1C (writing 0 has no effect).
        unsafe {
            self.function
                .write_reg(STATUS_AND_COMMAND_REGISTER, previous_command)
        }

        // Only the writes to the top bits will have succeeded, so doing a bitwise not will make this only the lower bits.
        // Then adding one will give back the power of 2 size of the BAR
        (!masked_address + 1).try_into().unwrap()
    }

    /// Writes a 32 bit value to the base address of this BAR.
    ///
    /// # Safety
    /// The caller must ensure that writing this value will not violate safety,
    /// and that no other code is relying on the value of this BAR.
    pub unsafe fn write_u32(&self, value: u32) {
        let BarValue::MemorySpace { base_address, .. } = self.read_value() else {
            unimplemented!("Writing to IO space BARs");
        };

        match base_address {
            MemorySpaceBarBaseAddress::Small(_) => {
                // SAFETY: The safety of this operation is the caller's responsibility.
                unsafe {
                    self.function.write_reg(self.register, value);
                }

                debug_assert!(
                    matches!(self.read_value(), BarValue::MemorySpace { base_address: MemorySpaceBarBaseAddress::Small(a), .. } if a == value)
                );
            }
            MemorySpaceBarBaseAddress::Large(_) => {
                // SAFETY: The safety of this operation is the caller's responsibility.
                unsafe {
                    self.function.write_reg(self.register, value);
                    // Clear the top 32 bits
                    self.function.write_reg(self.register + 1, 0);
                }

                debug_assert!(
                    matches!(self.read_value(), BarValue::MemorySpace { base_address: MemorySpaceBarBaseAddress::Large(a), .. } if a == value as u64)
                );
            }
        }
    }

    /// Writes a 64 bit value to the base address of this BAR.
    ///
    /// # Safety
    /// The caller must ensure that writing this value will not violate safety,
    /// and that no other code is relying on the value of this BAR.
    pub unsafe fn write_u64(&self, value: u64) {
        let BarValue::MemorySpace { base_address, .. } = self.read_value() else {
            unimplemented!("Writing to IO space BARs");
        };

        assert_eq!(
            value & (self.get_size() - 1),
            0,
            "Value must be aligned to the size of the BAR"
        );

        match base_address {
            MemorySpaceBarBaseAddress::Small(_) => {
                // TODO: make this fallible
                panic!("Can't write a 64-bit value to a 32-bit BAR");
            }
            MemorySpaceBarBaseAddress::Large(_) => {
                // SAFETY: The safety of this operation is the caller's responsibility.
                unsafe {
                    // Write the upper 32 bits
                    self.function
                        .write_reg(self.register + 1, (value >> 32) as u32);

                    // Write the lower 32 bits
                    self.function.write_reg(self.register, value as u32);

                    let r0 = self.function.read_reg(self.register) & !0b1111;
                    let r1 = self.function.read_reg(self.register + 1);

                    println!("r0: 0x{r0:x}, r1: 0x{r1:x}");
                    println!("base address: 0x{:x}", ((r1 as u64) << 32) | (r0 as u64));

                    debug_assert_eq!(r0, value as u32);
                    debug_assert_eq!(r1, (value >> 32) as u32);
                    debug_assert_eq!((r1 as u64) << 32 | r0 as u64, value);
                }

                let BarValue::MemorySpace {
                    base_address: MemorySpaceBarBaseAddress::Large(a),
                    ..
                } = self.read_value()
                else {
                    panic!("The BAR changed type");
                };

                debug_assert_eq!(a, value);
            }
        }
    }

    /// Gets the physical frames this BAR is mapped to.
    ///
    /// # Panics
    /// * If the bar is IO space
    /// * If the value of the BAR is 0
    pub fn get_frames(&self) -> PhysFrameRange {
        let BarValue::MemorySpace { base_address, .. } = self.read_value() else {
            unimplemented!("IO space BARs")
        };

        let base_address = match base_address {
            MemorySpaceBarBaseAddress::Small(a) => a as u64,
            MemorySpaceBarBaseAddress::Large(a) => a,
        };

        if base_address == 0 {
            panic!("BAR was not allocated by the BIOS")
        }

        let size = self.get_size();

        let start_page = PhysFrame::containing_address(PhysAddr::new(base_address));

        PhysFrameRange {
            start: start_page,
            end: start_page + size / 0x1000,
        }
    }

    /// Prints out the BAR's info in a debug format
    #[allow(dead_code)]
    pub fn debug(&self) {
        let BarValue::MemorySpace {
            base_address,
            prefetchable,
        } = self.read_value()
        else {
            panic!()
        };

        println!(
            "Address: {:?}, Size: 0x{:x}, Prefetchable: {}",
            base_address,
            self.get_size(),
            prefetchable
        );
    }
}
