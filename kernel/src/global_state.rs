//! Types for managing the kernel's global state

use spin::{Mutex, MutexGuard};
use x86_64::structures::paging::OffsetPageTable;

use crate::acpi::AcpiCache;
use crate::allocator::{LinkedListAllocator, ALLOCATOR};
use crate::cpu::{BootInfoFrameAllocator, PhysicalMemoryAccessor};

/// A piece of global state.
#[derive(Debug)]
pub struct GlobalState<T>(Mutex<Option<T>>);

/// An error which can occur when trying to get the data of a [`GlobalState`] object
/// using the [`try_locked_if_init`][GlobalState::try_locked_if_init] method.
pub enum TryLockedIfInitError {
    /// The [`GlobalState`] object was locked
    Locked,
    /// The [`GlobalState`] object is not yet initialised
    NotInitialised,
}

impl<T> GlobalState<T> {
    /// Create a new [`GlobalState`], with a value of [`None`].
    pub const fn new() -> Self {
        Self(Mutex::new(None))
    }

    /// Initialise the [`GlobalState`] with a value.
    ///
    /// # Panics
    /// If the [`GlobalState`] has already been initialised
    pub fn init(&self, data: T) {
        let mut s = self.0.lock();
        if s.is_some() {
            panic!("GlobalState was already initialised")
        }
        *s = Some(data);
    }

    /// Tries to gets whether the [`GlobalState`] object has been initialised or not
    pub fn try_is_init(&self) -> Option<bool> {
        self.0.try_lock().map(|lock| lock.is_some())
    }

    /// Lock the contained [`Mutex`], wrapped in a [`GlobalStateLock`]
    ///
    /// # Panics
    /// If the [`GlobalState`] is already locked
    pub fn lock(&self) -> GlobalStateLock<T> {
        GlobalStateLock(self.0.lock())
    }

    /// Tries to lock the contained [`Mutex`]
    pub fn try_lock(&self) -> Option<GlobalStateLock<T>> {
        self.0.try_lock().map(|lock| GlobalStateLock(lock))
    }

    /// Tries to lock the contained [`Mutex`] and then only return a lock if the data has been initialised.
    pub fn try_locked_if_init(&self) -> Result<GlobalStateLock<T>, TryLockedIfInitError> {
        let Some(l) = self.0.try_lock() else {
            return Err(TryLockedIfInitError::Locked);
        };

        if l.is_some() {
            Ok(GlobalStateLock(l))
        } else {
            Err(TryLockedIfInitError::NotInitialised)
        }
    }
}

/// A lock over the [`Mutex`] of a [`GlobalState`] object.
/// This lock assumes that the [`GlobalState`] has been initialised with the [`init`][GlobalState::init] method,
/// and will panic on deref if this is not the case.
pub struct GlobalStateLock<'a, T>(MutexGuard<'a, Option<T>>);

impl<'a, T> core::ops::Deref for GlobalStateLock<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0.deref().as_ref().expect("GlobalState should have been initialised")
    }
}

impl<'a, T> core::ops::DerefMut for GlobalStateLock<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.deref_mut().as_mut().expect("GlobalState should have been initialised")
    }
}

/// The state of the kernel, and resources needed to manage memory and hardware
#[derive(Debug)]
pub struct KernelState {
    /// Struct which manages page tables to map virtual pages to physical memory
    pub page_table: GlobalState<KernelPageTable>,
    /// Struct which manages allocating physical frames of memory
    pub frame_allocator: GlobalState<KernelFrameAllocator>,
    /// Struct which allocates the kernel heap
    pub heap_allocator: &'static GlobalState<KernelHeapAllocator>,
    /// Helper struct to access physical memory locations
    pub physical_memory_accessor: GlobalState<PhysicalMemoryAccessor>,
    /// Cache of ACPI tables
    pub acpi_cache: GlobalState<AcpiCache>,
}

/// The global kernel state
pub static KERNEL_STATE: KernelState = KernelState {
    page_table: GlobalState::new(),
    frame_allocator: GlobalState::new(),
    heap_allocator: ALLOCATOR.get(),
    physical_memory_accessor: GlobalState::new(),
    acpi_cache: GlobalState::new(),
};

/// A type alias for the kernel's page table. This makes it easier to change the exact type in future.
pub type KernelPageTable = OffsetPageTable<'static>;
/// A type alias for the kernel's frame allocator. This makes it easier to change the exact type in future.
pub type KernelFrameAllocator = BootInfoFrameAllocator;
/// A type alias for the kernel's heap allocator. This makes it easier to change the exact type in future.
pub type KernelHeapAllocator = LinkedListAllocator;
