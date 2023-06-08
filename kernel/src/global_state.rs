//! Types for managing the kernel's global state

use spin::{Mutex, MutexGuard};
use x86_64::structures::paging::OffsetPageTable;

use crate::memory::{
    allocator::{LinkedListAllocator, ALLOCATOR},
    BootInfoFrameAllocator,
};

/// A piece of global state.
#[derive(Debug)]
pub struct GlobalState<T>(Mutex<Option<T>>);

impl<T> GlobalState<T> {
    /// Create a new [`GlobalState`], with a value of [`None`].
    pub const fn new() -> Self {
        Self(Mutex::new(None))
    }

    /// Initialise the [`GlobalState`] with a value.
    ///
    /// # Panics
    /// If the [`GlobalState`] has already been initialised contains a [`Some`] variant
    pub fn init(&self, data: T) {
        let mut s = self.0.lock();
        if s.is_some() {
            panic!("GlobalState was already initialised")
        }
        *s = Some(data);
    }

    /// Lock the contained [`Mutex`], wrapped in a [`GlobalStateLock`]
    pub fn lock(&self) -> GlobalStateLock<T> {
        GlobalStateLock(self.0.lock())
    }
}

/// A lock over the [`Mutex`] of a [`GlobalState`] object.
/// This lock assumes that the [`GlobalState`] has been initialised with the [`init`][GlobalState::init] method,
/// and will panic on deref if this is not the case.
pub struct GlobalStateLock<'a, T>(MutexGuard<'a, Option<T>>);

impl<'a, T> core::ops::Deref for GlobalStateLock<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0.deref().as_ref().unwrap()
    }
}

impl<'a, T> core::ops::DerefMut for GlobalStateLock<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.deref_mut().as_mut().unwrap()
    }
}

/// The state of the kernel, and resources needed to manage memory and hardware
#[derive(Debug)]
pub struct KernelState {
    /// Struct which manages page tables to map virtual pages to physical memory
    pub page_table: GlobalState<KernelPageTable>,
    /// Struct which manages allocating physical frames
    pub frame_allocator: GlobalState<KernelFrameAllocator>,
    /// Struct which allocates the kernel heap
    pub heap_allocator: &'static GlobalState<KernelHeapAllocator>,
}

/// The global kernel state
pub static KERNEL_STATE: KernelState = KernelState {
    page_table: GlobalState::new(),
    frame_allocator: GlobalState::new(),
    heap_allocator: ALLOCATOR.get(),
};

/// A type alias for the kernel's page table. This makes it easier to change the exact type in future.
pub type KernelPageTable = OffsetPageTable<'static>;
/// A type alias for the kernel's frame allocator. This makes it easier to change the exact type in future.
pub type KernelFrameAllocator = BootInfoFrameAllocator;
/// A type alias for the kernel's heap allocator. This makes it easier to change the exact type in future.
pub type KernelHeapAllocator = LinkedListAllocator;
