//! The [`EventTrbRing`] type

use x86_64::PhysAddr;

use crate::allocator::PageBox;

use super::{EventTrb, GenericTrbFlags};

/// An _Event TRB Ring_
///
/// This ring contains [`EventTrb`]s for the OS to respond to.
#[derive(Debug)]
pub struct EventTrbRing {
    /// The page where the ring is in memory
    ring: PageBox,
    /// As the event ring is written by the controller, link TRBs can't be used to set the structure of the ring
    /// like for the command and transfer rings. Instead, a secondary table is used which stores the addresses
    /// and lengths of ring segments.
    ///
    /// See [`EventRingSegmentTableEntry`].
    segment_table: PageBox,

    /// The index where new TRBs will be dequeued
    dequeue: usize,
    /// The value of the cycle bit which will be considered a valid TRB
    cycle_state: bool,
}

impl EventTrbRing {
    /// The number of TRBs per page of memory
    const SEGMENT_SIZE: u16 = 0x1000 / 16;

    /// Constructs a new event ring. When the dequeue pointer changes, the new value will be written to the given register.
    ///
    /// # Safety
    /// * The given `dequeue_pointer_register` pointer must point to a valid register.
    ///    The pointer must be valid for the whole lifetime of this struct.
    pub unsafe fn new() -> Self {
        let ring = PageBox::new_zeroed();
        let mut segment_table = PageBox::new_zeroed();

        // SAFETY: This writes the first entry of the ERST
        unsafe {
            segment_table
                .as_mut_ptr::<EventRingSegmentTableEntry>()
                .write_volatile(EventRingSegmentTableEntry::new(
                    ring.phys_frame().start_address(),
                    Self::SEGMENT_SIZE,
                ));
        }

        Self {
            ring,
            segment_table,
            dequeue: 0,
            cycle_state: true,
        }
    }

    /// Reads a TRB from the ring, if one is present. Also returns the new dequeue address, 
    /// which must be written to the event ring's dequeue register.
    ///
    /// # Safety
    /// This method does _not_ update the controller's dequeue pointer.
    /// The caller must make sure the pointer is updated if this method returns `Some`,
    /// or else the controller will not be able to issue a new TRB in the location this one was read.
    pub unsafe fn dequeue(&mut self) -> Option<(EventTrb, PhysAddr)> {
        // SAFETY: This reads the TRB at `dequeue`.
        let raw = unsafe {
            self.ring
                .as_ptr::<[u32; 4]>()
                .add(self.dequeue)
                .read_volatile()
        };

        let current_dequeue = self.dequeue;
        
        // Check whether the TRB has the cycle bit set matching `cycle_state`
        if GenericTrbFlags::from(raw[3]).cycle() == self.cycle_state {
            self.dequeue += 1;
            if self.dequeue >= Self::SEGMENT_SIZE.into() {
                self.dequeue = 0;
                self.cycle_state = !self.cycle_state;
            }

            Some((
                EventTrb::new(raw),
                self.ring_start_addr() + (current_dequeue * 16),
            ))
        } else {
            None
        }
    }

    /// Gets the physical address of the start of the first segment of the ring
    pub fn ring_start_addr(&self) -> PhysAddr {
        self.ring.phys_frame().start_address()
    }

    /// Gets the index into the _Event Ring Segment Table_ of the segment of the start of the ring
    pub fn ring_start_segment(&self) -> u16 {
        0
    }

    /// Gets the length of the ring in TRBs
    pub fn ring_len(&self) -> u16 {
        Self::SEGMENT_SIZE
    }

    /// Gets the physical address of the segment table for this event ring
    pub fn segment_table_start_addr(&self) -> PhysAddr {
        self.segment_table.phys_frame().start_address()
    }

    /// Gets the number of items in the segment table for this event ring
    pub fn segment_table_len(&self) -> u16 {
        1
    }
}

/// An entry in the segment table for an event ring. This indicates the address and length of a segment of an [`EventTrbRing`].
#[repr(C)]
#[derive(Debug)]
struct EventRingSegmentTableEntry {
    /// The base address of the segment.
    ///
    /// Bits `0..=5` are reserved and should be masked.
    base_address: u64,
    /// The number of TRBs in the segment
    ring_segment_size: u16,

    #[doc(hidden)]
    _reserved0: u16,
    #[doc(hidden)]
    _reserved1: u32,
}

impl EventRingSegmentTableEntry {
    /// Constructs a new [`EventRingSegmentTableEntry`] pointing to `addr`, with the given `segment_size` in TRBs.
    fn new(addr: PhysAddr, segment_size: u16) -> Self {
        assert!(addr.is_aligned(64u64));
        assert!(segment_size >= 16);

        let base_address = addr.as_u64();

        Self {
            base_address,
            ring_segment_size: segment_size,
            _reserved0: 0,
            _reserved1: 0,
        }
    }
}
