//! A simple task-based scheduler for running code asynchronously.

use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};

use alloc::{boxed::Box, vec::Vec};
use spin::Mutex;
use x86_64::instructions::interrupts::without_interrupts;

use crate::println;

/// An async task which is polled on each timer interrupt
pub struct Task(Pin<Box<dyn Future<Output = ()>>>);

// SAFETY: Currently the kernel doesn't have threads.
// TODO: When threads are added, this code will need to be updated to ensure soundness.
unsafe impl Send for Task {}

impl Task {
    /// Registers a new task
    pub fn register<T>(t: T)
    where
        T: Future<Output = ()> + 'static,
    {
        // The `TASKS` vector is used in the timer interrupt handler,
        // so disable interrupts while modifying it to avoid deadlock
        without_interrupts(|| {
            TASKS.lock().push(Self(Box::pin(t)));
        });
    }
}

/// A global list of tasks
static TASKS: Mutex<Vec<Task>> = Mutex::new(Vec::new());

/// Constructs a [`RawWaker`] which panics if [`wake`][Waker::wake] is called
fn dummy_raw_waker() -> RawWaker {
    /// Constructs a new [`RawWaker`]
    fn clone(_: *const ()) -> RawWaker {
        dummy_raw_waker()
    }
    /// Panics
    /// TODO: an actual waker implementation
    fn waker_panic(_: *const ()) {
        panic!("Dummy waker should not be used");
    }
    /// Does nothing
    fn no_op(_: *const ()) {}

    let vtable = &RawWakerVTable::new(clone, waker_panic, waker_panic, no_op);

    RawWaker::new(core::ptr::null(), vtable)
}

/// Constructs a [`Waker`] which panics if [`wake`][Waker::wake] is called
fn dummy_waker() -> Waker {
    let raw_waker = dummy_raw_waker();

    // SAFETY: This waker always panics if called
    unsafe { Waker::from_raw(raw_waker) }
}

/// Polls all registered devices
pub fn poll_tasks() {
    let devices = &mut *TASKS.lock();
    devices.retain_mut(|device| {
        match device
            .0
            .as_mut()
            .poll(&mut Context::from_waker(&dummy_waker()))
        {
            Poll::Pending => true,
            Poll::Ready(()) => false,
        }
    });
}

/// Gets the number of tasks in [`TASKS`]
pub fn num_tasks() -> usize {
    TASKS.lock().len()
}
