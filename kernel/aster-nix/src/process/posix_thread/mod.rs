// SPDX-License-Identifier: MPL-2.0

#![allow(dead_code)]

use aster_rights::{ReadOp, WriteOp};
use futex::futex_wake;
use robust_list::wake_robust_futex;

use super::{
    do_exit_group,
    kill::SignalSenderIds,
    signal::{
        sig_mask::{SigMask, SigSet},
        sig_num::SigNum,
        sig_queues::SigQueues,
        signals::Signal,
        SigEvents, SigEventsFilter, SigStack,
    },
    Credentials, Process, TermStatus,
};
use crate::{
    events::Observer,
    prelude::*,
    process::signal::constants::SIGCONT,
    thread::{thread_table, Tid},
    time::{clocks::ProfClock, Timer, TimerManager},
    util::write_val_to_user,
};

mod builder;
pub mod futex;
mod name;
mod posix_thread_ext;
mod robust_list;

pub use builder::PosixThreadBuilder;
pub use name::{ThreadName, MAX_THREAD_NAME_LEN};
pub use posix_thread_ext::PosixThreadExt;
pub use robust_list::RobustListHead;

pub struct PosixThread {
    // Immutable part
    process: Weak<Process>,
    is_main_thread: bool,

    // Mutable part
    name: Mutex<Option<ThreadName>>,

    // Linux specific attributes.
    // https://man7.org/linux/man-pages/man2/set_tid_address.2.html
    set_child_tid: Mutex<Vaddr>,
    clear_child_tid: Mutex<Vaddr>,

    robust_list: Mutex<Option<RobustListHead>>,

    /// Process credentials. At the kernel level, credentials are a per-thread attribute.
    credentials: Credentials,

    // Signal
    /// Blocked signals
    sig_mask: Mutex<SigMask>,
    /// Thread-directed sigqueue
    sig_queues: SigQueues,
    /// Signal handler ucontext address
    /// FIXME: This field may be removed. For glibc applications with RESTORER flag set, the sig_context is always equals with rsp.
    sig_context: Mutex<Option<Vaddr>>,
    sig_stack: Mutex<Option<SigStack>>,

    /// A profiling clock measures the user CPU time and kernel CPU time in the thread.
    prof_clock: Arc<ProfClock>,

    /// A manager that manages timers based on the user CPU time of the current thread.
    virtual_timer_manager: Arc<TimerManager>,

    /// A manager that manages timers based on the profiling clock of the current thread.
    prof_timer_manager: Arc<TimerManager>,
}

impl PosixThread {
    pub fn process(&self) -> Arc<Process> {
        self.process.upgrade().unwrap()
    }

    pub fn thread_name(&self) -> &Mutex<Option<ThreadName>> {
        &self.name
    }

    pub fn set_child_tid(&self) -> &Mutex<Vaddr> {
        &self.set_child_tid
    }

    pub fn clear_child_tid(&self) -> &Mutex<Vaddr> {
        &self.clear_child_tid
    }

    pub fn sig_mask(&self) -> &Mutex<SigMask> {
        &self.sig_mask
    }

    pub fn sig_pending(&self) -> SigSet {
        self.sig_queues.sig_pending()
    }

    /// Returns whether the thread has some pending signals
    /// that are not blocked.
    pub fn has_pending(&self) -> bool {
        let blocked = *self.sig_mask().lock();
        self.sig_queues.has_pending(blocked)
    }

    /// Returns whether the signal is blocked by the thread.
    pub(in crate::process) fn has_signal_blocked(&self, signal: &dyn Signal) -> bool {
        let mask = self.sig_mask.lock();
        mask.contains(signal.num())
    }

    /// Checks whether the signal can be delivered to the thread.
    ///
    /// For a signal can be delivered to the thread, the sending thread must either
    /// be privileged, or the real or effective user ID of the sending thread must equal
    /// the real or saved set-user-ID of the target thread.
    ///
    /// For SIGCONT, the sending and receiving processes should belong to the same session.
    pub(in crate::process) fn check_signal_perm(
        &self,
        signum: Option<&SigNum>,
        sender: &SignalSenderIds,
    ) -> Result<()> {
        if sender.euid().is_root() {
            return Ok(());
        }

        if let Some(signum) = signum
            && *signum == SIGCONT
        {
            let receiver_sid = self.process().session().unwrap().sid();
            if receiver_sid == sender.sid() {
                return Ok(());
            }

            return_errno_with_message!(
                Errno::EPERM,
                "sigcont requires that sender and receiver belongs to the same session"
            );
        }

        let (receiver_ruid, receiver_suid) = {
            let credentials = self.credentials();
            (credentials.ruid(), credentials.suid())
        };

        // FIXME: further check the below code to ensure the behavior is same as Linux. According
        // to man(2) kill, the real or effective user ID of the sending process must equal the
        // real or saved set-user-ID of the target process.
        if sender.ruid() == receiver_ruid
            || sender.ruid() == receiver_suid
            || sender.euid() == receiver_ruid
            || sender.euid() == receiver_suid
        {
            return Ok(());
        }

        return_errno_with_message!(Errno::EPERM, "sending signal to the thread is not allowed.");
    }

    /// Enqueues a thread-directed signal. This method should only be used for enqueue kernel
    /// signal and fault signal.
    pub fn enqueue_signal(&self, signal: Box<dyn Signal>) {
        self.sig_queues.enqueue(signal);
    }

    /// Returns a reference to the profiling clock of the current thread.
    pub fn prof_clock(&self) -> &Arc<ProfClock> {
        &self.prof_clock
    }

    /// Creates a timer based on the profiling CPU clock of the current thread.
    pub fn create_prof_timer<F>(&self, func: F) -> Arc<Timer>
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.prof_timer_manager.create_timer(func)
    }

    /// Creates a timer based on the user CPU clock of the current thread.
    pub fn create_virtual_timer<F>(&self, func: F) -> Arc<Timer>
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.virtual_timer_manager.create_timer(func)
    }

    /// Checks the `TimerCallback`s that are managed by the `prof_timer_manager`.
    /// If any have timed out, call the corresponding callback functions.
    pub fn process_expired_timers(&self) {
        self.prof_timer_manager.process_expired_timers();
    }

    pub fn dequeue_signal(&self, mask: &SigMask) -> Option<Box<dyn Signal>> {
        self.sig_queues.dequeue(mask)
    }

    pub fn register_sigqueue_observer(
        &self,
        observer: Weak<dyn Observer<SigEvents>>,
        filter: SigEventsFilter,
    ) {
        self.sig_queues.register_observer(observer, filter);
    }

    pub fn unregiser_sigqueue_observer(&self, observer: &Weak<dyn Observer<SigEvents>>) {
        self.sig_queues.unregister_observer(observer);
    }

    pub fn sig_context(&self) -> &Mutex<Option<Vaddr>> {
        &self.sig_context
    }

    pub fn sig_stack(&self) -> &Mutex<Option<SigStack>> {
        &self.sig_stack
    }

    pub fn robust_list(&self) -> &Mutex<Option<RobustListHead>> {
        &self.robust_list
    }

    /// Whether the thread is main thread. For Posix thread, If a thread's tid is equal to pid, it's main thread.
    pub fn is_main_thread(&self) -> bool {
        self.is_main_thread
    }

    /// whether the thread is the last running thread in process
    pub fn is_last_thread(&self) -> bool {
        let process = self.process.upgrade().unwrap();
        let threads = process.threads().lock();
        threads
            .iter()
            .filter(|thread| !thread.status().is_exited())
            .count()
            == 0
    }

    /// Walks the robust futex list, marking futex dead and wake waiters.
    /// It corresponds to Linux's exit_robust_list(), errors are silently ignored.
    pub fn wake_robust_list(&self, tid: Tid) {
        let mut robust_list = self.robust_list.lock();
        let list_head = match *robust_list {
            None => {
                return;
            }
            Some(robust_list_head) => robust_list_head,
        };
        debug!("wake the rubust_list: {:?}", list_head);
        for futex_addr in list_head.futexes() {
            // debug!("futex addr = 0x{:x}", futex_addr);
            wake_robust_futex(futex_addr, tid).unwrap();
        }
        debug!("wake robust futex success");
        *robust_list = None;
    }

    /// Posix thread does not contains tid info. So we require tid as a parameter.
    pub fn exit(&self, tid: Tid, term_status: TermStatus) -> Result<()> {
        let mut clear_ctid = self.clear_child_tid().lock();
        // If clear_ctid !=0 ,do a futex wake and write zero to the clear_ctid addr.
        debug!("wake up ctid");
        if *clear_ctid != 0 {
            debug!("futex wake");
            futex_wake(*clear_ctid, 1)?;
            debug!("write ctid");
            // FIXME: the correct write length?
            debug!("ctid = 0x{:x}", *clear_ctid);
            write_val_to_user(*clear_ctid, &0u32).unwrap();
            debug!("clear ctid");
            *clear_ctid = 0;
        }
        debug!("wake up ctid succeeds");
        // exit the robust list: walk the robust list; mark futex words as dead and do futex wake
        self.wake_robust_list(tid);

        if tid != self.process().pid() {
            // If the thread is not main thread. We don't remove main thread.
            // Main thread are removed when the whole process is reaped.
            thread_table::remove_thread(tid);
        }

        if self.is_main_thread() || self.is_last_thread() {
            // exit current process.
            debug!("self is main thread or last thread");
            debug!("main thread: {}", self.is_main_thread());
            debug!("last thread: {}", self.is_last_thread());
            do_exit_group(term_status);
        }
        debug!("perform futex wake");
        futex_wake(Arc::as_ptr(&self.process()) as Vaddr, 1)?;
        Ok(())
    }

    /// Gets the read-only credentials of the thread.
    pub fn credentials(&self) -> Credentials<ReadOp> {
        self.credentials.dup().restrict()
    }

    /// Gets the write-only credentials of the thread.
    pub(in crate::process) fn credentials_mut(&self) -> Credentials<WriteOp> {
        self.credentials.dup().restrict()
    }
}
