//! Structs which handle the

mod port_status_change;

use core::{
    cell::{Cell, RefCell},
    fmt::Debug,
    mem::MaybeUninit,
    pin::Pin,
};

use alloc::{boxed::Box, vec::Vec};
use futures::Future;
use log::{error, warn};
use port_status_change::{handle_port_status_change, PortStatusChangeTask};

use super::{
    trb::{event::port_status_change::PortStatusChangeTrb, EventTrb},
    XhciController,
};

/// A value for a timeout field equivalent to a 1 second timeout
const TIMEOUT_1_SECOND: usize = 1_000_000_000;

/// A task which is executing in relation to a controller.
#[derive(Debug)]
struct Task<'a> {
    /// Specifies whether the task should be polled, or whether it is waiting for an external event
    waker: TaskWaker,
    /// The task which is being run
    task: TaskType<'a>,
}

impl<'a> Task<'a> {
    /// Constructs a new [`struct@Task`]. The task is constructed in-place, pinned on the heap to allow the [`task`] to contain a reference to the [`waker`].
    /// Takes as an argument a function which constructs a [`TaskType`] given a reference to the task's waker.
    ///
    /// [`task`]: Task::task
    /// [`waker`]: Task::waker
    fn new_inner<F: FnOnce(&'a TaskWaker) -> TaskType<'a>>(f: F) -> Pin<Box<Self>> {
        let mut b = Box::<Self>::new_uninit();
        let b_ptr = b.as_mut_ptr();

        // SAFETY: This is a write to uninitialised memory in the Box, which is sound.
        // The pointer was just written to so the reference is sound.
        let w = unsafe {
            let p = core::ptr::addr_of_mut!((*b_ptr).waker);
            p.write(TaskWaker::new());
            &*p
        };

        let task = f(w);

        // SAFETY: This is a write to uninitialised memory in the Box, which is sound.
        unsafe {
            core::ptr::addr_of_mut!((*b_ptr).task).write(task);
        }

        // SAFETY: Both fields of the Task have been written to, so the whole struct is initialised
        unsafe { Box::into_pin(Box::<MaybeUninit<_>>::assume_init(b)) }
    }

    /// Constructs a new [`PortStatusChange`] [`Task`] responding to
    ///
    /// [`PortStatusChange`]: TaskType::PortStatusChange
    fn port_status_change(
        c: &'a RefCell<XhciController>,
        trb: PortStatusChangeTrb,
    ) -> Pin<Box<Self>> {
        Self::new_inner(|w| TaskType::PortStatusChange(handle_port_status_change(c, w, trb)))
    }

    /// Checks the type of the passed TRB and constructs a new [`Task`] if needed to handle it.
    fn new(c: &'a RefCell<XhciController>, trb: EventTrb) -> Option<Pin<Box<Self>>> {
        match trb {
            EventTrb::MFINDEXWrap => None,
            EventTrb::PortStatusChange(trb) => Some(Self::port_status_change(c, trb)),

            _ => {
                warn!("Unhandled TRB: {trb:?}");
                None
            }
        }
    }
}

impl<'a> Future for Task<'a> {
    type Output = Result<(), TaskError>;

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        // SAFETY: The reference is not moved out of
        let task = unsafe { self.get_unchecked_mut() };

        // SAFETY: As `self` is pinned, the contained future is also pinned
        unsafe {
            match task.task {
                TaskType::PortStatusChange(ref mut p) => {
                    Future::poll(Pin::new_unchecked(p), cx).map_err(TaskError::PortStatusChange)
                }
            }
        }
    }
}

/// An error which can occur while executing a task
#[derive(Debug, Clone, Copy)]
enum TaskError {
    /// An error from a [`PortStatusChangeTask`]
    PortStatusChange(port_status_change::Error),
}

impl From<port_status_change::Error> for TaskError {
    fn from(v: port_status_change::Error) -> Self {
        Self::PortStatusChange(v)
    }
}

/// A type of [`Task`]
enum TaskType<'a> {
    /// A task responding to a [`PortStatusChangeTrb`]
    ///
    /// [`PortStatusChangeTrb`]: super::trb::event::port_status_change::PortStatusChangeTrb
    PortStatusChange(PortStatusChangeTask<'a>),
}

impl<'a> Debug for TaskType<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::PortStatusChange(_) => write!(f, "PortStatusChange"),
        }
    }
}

/// An error occurring when a timeout expires which should not have
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TimeoutReachedError;

/// Stores what a [`Task`] is waiting for. This will be checked by [`poll_tasks`] to decide whether
/// or not to poll a given task. If the task is waiting for some data (e.g. a TRB), the data may also
/// be written to the task's [`TaskWaker`]. The task's future will be passed a reference to the waker,
/// and can use methods such as [`wait_for_timeout`] to wait for certain conditions to be met.
///
/// [`wait_for_timeout`]: TaskWaker::wait_for_timeout
#[derive(Debug)]
struct TaskWaker(Cell<Waiting>);

impl TaskWaker {
    /// Constructs a new [`TaskWaker`] which is not waiting for anything
    fn new() -> Self {
        Self(Cell::new(Waiting::None))
    }

    /// Waits for a timeout given in nanoseconds to elapse
    async fn wait_for_timeout(&self, timeout_ns: usize) {
        self.0.set(Waiting::TimeoutNS(timeout_ns));

        loop {
            futures::pending!();

            match self.0.get() {
                Waiting::TimeoutReached => return,
                Waiting::TimeoutNS(_) => (),
                _ => panic!("Waiting state changed unexpectedly"),
            }
        }
    }

    /// Waits for a [`PortStatusChangeTrb`] on the given `port_id`. If the TRB is not received within the given
    /// timeout in nanoseconds, An error is returned.
    async fn wait_for_port_status_change(
        &self,
        port_id: u8,
        timeout_ns: usize,
    ) -> Result<PortStatusChangeTrb, TimeoutReachedError> {
        self.0.set(Waiting::PortStatusChange {
            port: port_id,
            timeout: timeout_ns,
        });

        let r = loop {
            futures::pending!();

            match self.0.get() {
                Waiting::TimeoutReached => break Err(TimeoutReachedError),
                Waiting::PortStatusChangeReceived(trb) => break Ok(trb),
                Waiting::PortStatusChange { .. } => (),
                _ => panic!("Waiting state changed unexpectedly"),
            }
        };

        self.0.set(Waiting::None);

        r
    }
}

/// What a [`Task`] is waiting for. This is used by the [`TaskWaker`] to communicate with [`poll_tasks`]
#[derive(Debug, Clone, Copy)]
enum Waiting {
    /// The task is not waiting for anything and should be polled immediately
    None,
    /// A specified timeout elapsed
    TimeoutReached,
    /// The task is waiting for a timeout given in nanoseconds to expire
    TimeoutNS(usize),
    /// The task is waiting for a [`PortStatusChangeTrb`] on the given port.
    /// If received, it will be written into the given [`Cell`]. If the timeout
    /// reaches zero before the TRB is received, the task will be polled anyway
    PortStatusChange {
        /// The [`port_id`] of the TRB
        ///
        /// [`port_id`]: PortStatusChangeTrb::port_id
        port: u8,
        /// The remaining timeout in nanoseconds
        timeout: usize,
    },
    /// The result of the [`PortStatusChange`] variant
    ///
    /// [`PortStatusChange`]: Waiting::PortStatusChange
    PortStatusChangeReceived(PortStatusChangeTrb),
}

impl Waiting {
    /// Whether a task with this [`Waiting`] state should be polled
    fn active(&self) -> bool {
        match self {
            Waiting::None => true,
            Waiting::TimeoutReached => true,
            Waiting::PortStatusChangeReceived(_) => true,

            Waiting::TimeoutNS(_) => false,
            Waiting::PortStatusChange { .. } => false,
        }
    }
}

/// Future type for [`TaskQueue::poll`]
struct PollTasks<'a: 'b, 'b> {
    /// The tasks to poll
    tasks: &'b mut Vec<Pin<Box<Task<'a>>>>,
    /// The time in nanoseconds since the last poll. This is used to update timeout values.
    ns_since_last: usize,
    /// A trb which may have been received since the last poll
    trb: Option<EventTrb>,
}

impl<'a: 'b, 'b> PollTasks<'a, 'b> {
    /// Implementation of [`TaskQueue::poll`]
    fn poll(&mut self, cx: &mut core::task::Context<'_>) -> core::task::Poll<Option<EventTrb>> {
        self.tasks.retain_mut(|i| {
            let new_state = match i.waker.0.get() {
                Waiting::TimeoutNS(ns) => match ns.checked_sub(self.ns_since_last) {
                    Some(ns) => Waiting::TimeoutNS(ns),
                    None => Waiting::TimeoutReached,
                },
                Waiting::PortStatusChange { port, timeout } => match self.trb {
                    Some(EventTrb::PortStatusChange(trb)) if trb.port_id == port => {
                        self.trb = None;
                        Waiting::PortStatusChangeReceived(trb)
                    }
                    _ => match timeout.checked_sub(self.ns_since_last) {
                        Some(timeout) => Waiting::PortStatusChange { port, timeout },
                        None => Waiting::TimeoutReached,
                    },
                },
                s => s,
            };

            i.waker.0.set(new_state);

            if new_state.active() {
                // Only keep tasks which have not completed
                match i.as_mut().poll(cx) {
                    core::task::Poll::Pending => true,
                    core::task::Poll::Ready(Ok(())) => false,
                    core::task::Poll::Ready(Err(e)) => {
                        error!("{e:?}");
                        false
                    }
                }
            } else {
                // Always keep tasks which are inactive
                true
            }
        });

        core::task::Poll::Ready(self.trb)
    }
}

impl<'a: 'b, 'b> Future for PollTasks<'a, 'b> {
    type Output = Option<EventTrb>;

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        let s = self.get_mut();
        s.poll(cx)
    }
}

/// A queue of [`Task`]s. These tasks can be polled using [`poll`], and will be removed when they complete.
///
/// [`poll`]: TaskQueue::poll
pub struct TaskQueue<'a>(Vec<Pin<Box<Task<'a>>>>, &'a RefCell<XhciController>);

impl<'a> TaskQueue<'a> {
    /// Constructs a new queue with no tasks
    pub fn new(c: &'a RefCell<XhciController>) -> Self {
        Self(Vec::new(), c)
    }

    /// Checks the type of the passed TRB and potentially starts a new task to handle it.
    /// If no new task is needed (e.g. for [`MFINDEXWrap`] TRBs), none are added.
    ///
    /// [`MFINDEXWrap`]: EventTrb::MFINDEXWrap
    fn push(&mut self, trb: EventTrb) {
        if let Some(task) = Task::new(self.1, trb) {
            self.0.push(task);
        }
    }

    /// Iterates through the queue, polling each active [`Task`] and removing any which complete.
    ///
    /// # Parameters
    /// * `ns_since_last`: The time in nanoseconds since the last call to [`poll`]. This is used to update timeouts.
    /// * `trb`: A TRB which may have been received since the last call to [`poll`]. This will be passed to any task whose
    ///     [`Waiting`] state is waiting for a TRB which matches the one passed. If no task is waiting for the TRB, a new
    ///     task may be started to handle it.
    ///
    /// [`poll`]: TaskQueue::poll
    pub async fn poll(&mut self, ns_since_last: usize, trb: Option<EventTrb>) {
        let leftover_trb = PollTasks {
            tasks: &mut self.0,
            ns_since_last,
            trb,
        }
        .await;

        if let Some(trb) = leftover_trb {
            self.push(trb);
        }
    }
}
