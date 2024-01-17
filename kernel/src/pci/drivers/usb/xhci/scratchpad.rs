//! The [`ScratchpadBufferArray`] type

use alloc::boxed::Box;
use x86_64::PhysAddr;

use crate::allocator::PageBox;

use super::operational_registers::SupportedPageSize;

/// The _Scratchpad Buffer Array_ data structure, defined in the spec section [6.6].
/// 
/// This is a data structure which gives the controller pointers to pages of memory which it can use for its own use.
/// 
/// [6.6]: https://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/extensible-host-controler-interface-usb-xhci.pdf#%5B%7B%22num%22%3A522%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C138%2C187%2C0%5D
pub struct ScratchpadBufferArray {
    /// The page containing the _Scratchpad Buffer Array_ - this contains pointers to pages in [`scratchpad_pages`]
    /// 
    /// [`scratchpad_pages`]: ScratchpadBufferArray::scratchpad_pages
    array_page: PageBox,
    /// The number of items in the array
    len: usize,
    /// The pages which are given to the controller. Note that these are for the controller's private use only.
    scratchpad_pages: Box<[PageBox]>,
}

impl ScratchpadBufferArray {
    /// Initialises a new scratchpad buffer array with the given length
    /// 
    /// # Safety
    /// * `page_size` must be the value of [the controller's `page_size` register]
    /// 
    /// [the controller's `page_size` register]: super::operational_registers::OperationalRegisters::read_page_size
    pub unsafe fn new(len: usize, page_size: SupportedPageSize) -> Self {
        if page_size.page_size() != 0x1000 {
            todo!("Non-4k pages");
        }

        assert!(
            len < 32,
            "Too many scratchpad buffers requested"
        );

        let array_page = PageBox::new();

        let scratchpad_pages: Box<[PageBox]> = core::iter::repeat(())
            .take(len)
            .map(|_| PageBox::new())
            .collect();

        let mut s = Self {
            array_page,
            len,
            scratchpad_pages,
        };

        for i in 0..len {
            // SAFETY: addr is the address of a scratchpad buffer
            unsafe {
                let addr = s.scratchpad_pages[i].phys_frame().start_address();
                s.set_slot_addr(i, addr);
            }
        }

        s
    }

    /// Gets the physical address of the _Scratchpad Buffer Array_ in memory
    pub fn get_array_addr(&self) -> PhysAddr {
        self.array_page.phys_frame().start_address()
    }

    /// Sets the address of the given entry in the scratchpad buffer array
    /// 
    /// # Safety
    /// * `addr` must be the physical address of a scratchpad buffer allocated for this controller
    unsafe fn set_slot_addr(&mut self, i: usize, addr: PhysAddr) {
        assert!(i < self.len);

        // SAFETY: i < len so this index is in range
        // `addr` is the address of a scratchpad buffer
        unsafe {
            self.array_page
                .as_mut_ptr::<u64>()
                .add(i)
                .write_volatile(addr.as_u64());
        }
    }
}
