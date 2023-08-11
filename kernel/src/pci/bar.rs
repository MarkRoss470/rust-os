//! Functionality for reading and writing Base Address Registers (BARs)

use x86_64::{
    structures::paging::{frame::PhysFrameRange, FrameAllocator, PhysFrame},
    PhysAddr,
};

use crate::{global_state::KERNEL_STATE, println};

use super::devices::PciRegister;

/// The address of a region in memory used by the PCI device
#[derive(Debug, Clone, Copy)]
pub enum MemorySpaceBarBaseAddress {
    /// A 32-bit base address
    Small(u32),
    /// A 64-but base address
    Large(u64),
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
    IOSpace {
        /// The port number (I think) of the device
        base_address: u32,
    },
}

/// A specific base address register of a PCI device
pub struct Bar {
    /// The register which this BAR is in
    register: PciRegister,
}

impl Bar {
    /// # Safety
    /// The passed `register` must be part of a BAR. If the BAR is 64-bit,
    /// it must be the register with the lower offset.
    pub unsafe fn new(register: PciRegister) -> Self {
        Self { register }
    }

    /// Reads the value of the BAR
    pub fn read_value(&self) -> BarValue {
        // SAFETY: This struct is unsafe to construct from a PciRegister which is not a BAR
        let lower_32 = unsafe { self.register.read_u32() };
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
                let upper_32 = unsafe { self.register.next().unwrap().read_u32() };

                println!("Second BAR: 0b{upper_32:b}");

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

    pub fn get_size(&self) -> usize {
        // SAFETY: This struct is unsafe to construct from a PciRegister which is not a BAR,
        let value_after_write = unsafe { self.register.write_and_reset(u32::MAX) };

        // Mask out the BAR's flag bits
        let masked_address = value_after_write & !0b1111;

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
                    self.register.write_u32(value);
                }

                debug_assert!(
                    matches!(self.read_value(), BarValue::MemorySpace { base_address: MemorySpaceBarBaseAddress::Small(a), .. } if a == value)
                );
            }
            MemorySpaceBarBaseAddress::Large(_) => {
                // SAFETY: The safety of this operation is the caller's responsibility.
                unsafe {
                    self.register.write_u32(value);
                    // Clear the top 32 bits
                    self.register.next().unwrap().write_u32(0);
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
            value & (self.get_size() as u64 - 1),
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
                    self.register
                        .next()
                        .unwrap()
                        .write_u32((value >> 32) as u32);
                    // Write the lower 32 bits
                    self.register.write_u32(value as u32);

                    let r0 = self.register.read_u32() & !0b1111;
                    let r1 = self.register.next().unwrap().read_u32();

                    println!("r0: 0x{r0:x}, r1: 0x{r1:x}");
                    println!("base address: 0x{:x}", ((r1 as u64) << 32) | (r0 as u64));

                    debug_assert_eq!(r0, value as u32);
                    debug_assert_eq!(r1, (value >> 32) as u32);
                    debug_assert_eq!((r1 as u64) << 32 | r0 as u64, value);
                }

                println!("{:?}", self.read_value());

                if let BarValue::MemorySpace {
                    base_address: MemorySpaceBarBaseAddress::Large(a),
                    ..
                } = self.read_value()
                {
                    // SAFETY: PCI config reads don't have side effects
                    let (r0, r1) = unsafe {
                        let r0 = self.register.read_u32() & !0b1111;
                        let r1 = self.register.next().unwrap().read_u32();
                        (r0, r1)
                    };

                    println!("0x{:x}  0x{:x}", r0, r1);
                    println!(
                        "0x{:x}  0x{:?}",
                        (r1 as u64) << 32 | r0 as u64,
                        self.read_value()
                    );
                    debug_assert_eq!(a, value);
                } else {
                    panic!("The BAR changed type");
                }
            }
        }
    }

    /// Allocates enough frames for this BAR and writes the address to the BAR, returning the allocated frames.
    /// If the BAR is already allocated (it contains a non-zero value) the current allocation will be returned instead.
    ///
    /// # Safety
    /// The caller must ensure that writing this value will not violate safety,
    /// and that no other code is relying on the value of this BAR.
    pub unsafe fn allocate(&self) -> PhysFrameRange {
        let BarValue::MemorySpace { base_address, .. } = &self.read_value() else {
            unimplemented!("Allocating IO space BARs");
        };

        let size = self.get_size();
        let frames = size.div_ceil(4096) as u64;
        let allocated_frames = KERNEL_STATE
            .frame_allocator
            .lock()
            .allocate_consecutive(frames, size as u64)
            .expect("Should have allocated frames");

        match base_address {
            // If the BAR is already allocated, don't reallocate
            MemorySpaceBarBaseAddress::Large(address) => {
                if *address != 0 {
                    let start = PhysFrame::containing_address(PhysAddr::new(*address));
                    return PhysFrameRange {
                        start,
                        end: start + frames,
                    };
                }
            }
            _ => {
                todo!("Allocating 32-bit BARs");
            }
        }

        let address = allocated_frames.start.start_address().as_u64();

        // SAFETY: The safety of this operation is the caller's responsibility
        unsafe {
            self.write_u64(address);
        }

        allocated_frames
    }
}
