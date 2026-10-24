//! Shared Linux timer implementation using timerfd.
//!
//! Both X11 and Wayland use identical timerfd-based timer logic.
//! This module provides the shared implementation.
//!
//! # Off Linux
//!
//! The X11 backend is also built for macOS (`x11-macos`, run against
//! XQuartz), which has neither timerfd nor eventfd. What the event loop needs
//! from both is a descriptor that `poll(2)` reports readable once something is
//! owed, and a kqueue is exactly that: one `EVFILT_TIMER` (a timer) or
//! `EVFILT_USER` (a cross-thread wake) per queue, with the queue's own fd in
//! the poll set. The one difference a caller sees is the acknowledgement: a
//! timerfd or eventfd is `read(2)`, while a kqueue answers `read` with ENXIO
//! and stays readable until `kevent(2)` retrieves its event - an idle loop
//! that read it would spin. Every acknowledgement therefore goes through
//! [`drain_fd`], and every descriptor is created here.

use std::collections::BTreeMap;

use crate::{desktop::shell2::common::debug_server::LogCategory, log_debug, log_error};

/// Create a timerfd and insert it into the timer_fds map.
#[cfg(target_os = "linux")]
pub fn start_timerfd(
    timer_fds: &mut BTreeMap<usize, i32>,
    timer_id: usize,
    interval_ms: u64,
    backend_name: &str,
) {
    unsafe {
        let fd = libc::timerfd_create(
            libc::CLOCK_MONOTONIC,
            libc::TFD_NONBLOCK | libc::TFD_CLOEXEC,
        );
        if fd >= 0 {
            // A timerfd with it_value == {0, 0} is DISARMED, not "fire
            // immediately and repeatedly". `Timer::tick_millis` legitimately
            // returns 0 for sub-millisecond / "as fast as possible" timers, and
            // those silently never fired on Linux while working everywhere else.
            // One nanosecond is the smallest armed value the kernel accepts; it
            // still coalesces to the actual timer resolution.
            let interval_ns = (interval_ms as u128 * 1_000_000).max(1);
            let secs = (interval_ns / 1_000_000_000) as libc::time_t;
            let nsecs = (interval_ns % 1_000_000_000) as libc::c_long;
            let spec = libc::itimerspec {
                it_interval: libc::timespec {
                    tv_sec: secs,
                    tv_nsec: nsecs,
                },
                it_value: libc::timespec {
                    tv_sec: secs,
                    tv_nsec: nsecs,
                },
            };
            if libc::timerfd_settime(fd, 0, &spec, std::ptr::null_mut()) == 0 {
                if let Some(old_fd) = timer_fds.insert(timer_id, fd) {
                    libc::close(old_fd);
                }
                log_debug!(
                    LogCategory::Timer,
                    "[{}] Created timerfd {} for timer {} (interval {}ms)",
                    backend_name,
                    fd,
                    timer_id,
                    interval_ms
                );
            } else {
                libc::close(fd);
                log_error!(
                    LogCategory::Timer,
                    "[{}] Failed to set timerfd interval",
                    backend_name
                );
            }
        } else {
            log_error!(
                LogCategory::Timer,
                "[{}] Failed to create timerfd: errno={}",
                backend_name,
                std::io::Error::last_os_error()
            );
        }
    }
}

/// Create a periodic kqueue timer and insert it into the timer_fds map: the
/// stand-in for a timerfd off Linux (see the module docs).
#[cfg(not(target_os = "linux"))]
pub fn start_timerfd(
    timer_fds: &mut BTreeMap<usize, i32>,
    timer_id: usize,
    interval_ms: u64,
    backend_name: &str,
) {
    match kqueue::timer(u128::from(interval_ms) * 1_000_000, false) {
        Ok(fd) => {
            if let Some(old_fd) = timer_fds.insert(timer_id, fd) {
                unsafe {
                    libc::close(old_fd);
                }
            }
            log_debug!(
                LogCategory::Timer,
                "[{}] Created kqueue timer {} for timer {} (interval {}ms)",
                backend_name,
                fd,
                timer_id,
                interval_ms
            );
        }
        Err(e) => {
            log_error!(
                LogCategory::Timer,
                "[{}] Failed to create kqueue timer: {}",
                backend_name,
                e
            );
        }
    }
}

/// Close and remove a timerfd from the timer_fds map.
pub fn stop_timerfd(timer_fds: &mut BTreeMap<usize, i32>, timer_id: usize, backend_name: &str) {
    if let Some(fd) = timer_fds.remove(&timer_id) {
        unsafe {
            libc::close(fd);
        }
        log_debug!(
            LogCategory::Timer,
            "[{}] Closed timerfd {} for timer {}",
            backend_name,
            fd,
            timer_id
        );
    }
}

/// Arm the one-shot frame-pace timer `*fd` to fire `after` from now, creating
/// it on first use.
///
/// `false` means no timer could be armed; the caller must then render now
/// rather than defer, because pacing must never lose a frame.
pub(crate) fn arm_oneshot_timer(fd: &mut i32, after: std::time::Duration) -> bool {
    #[cfg(target_os = "linux")]
    {
        unsafe {
            if *fd < 0 {
                *fd = libc::timerfd_create(
                    libc::CLOCK_MONOTONIC,
                    libc::TFD_NONBLOCK | libc::TFD_CLOEXEC,
                );
            }
            if *fd >= 0 {
                let ns = after.as_nanos().max(1);
                #[allow(clippy::cast_possible_truncation)]
                let spec = libc::itimerspec {
                    // one-shot: it_interval zero
                    it_interval: libc::timespec {
                        tv_sec: 0,
                        tv_nsec: 0,
                    },
                    it_value: libc::timespec {
                        tv_sec: (ns / 1_000_000_000) as libc::time_t,
                        tv_nsec: (ns % 1_000_000_000) as libc::c_long,
                    },
                };
                if libc::timerfd_settime(*fd, 0, &spec, std::ptr::null_mut()) == 0 {
                    return true;
                }
            }
        }
        false
    }
    #[cfg(not(target_os = "linux"))]
    {
        kqueue::arm_oneshot(fd, after.as_nanos())
    }
}

/// Create a wake descriptor: an fd for the poll set that another thread
/// raises with [`signal_wake_fd`]. `-1` when it cannot be created, in which
/// case the loop only wakes on its other descriptors.
///
/// Linux: an eventfd. Elsewhere: a kqueue carrying one `EVFILT_USER` event.
pub(crate) fn new_wake_fd() -> i32 {
    #[cfg(target_os = "linux")]
    {
        unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) }
    }
    #[cfg(not(target_os = "linux"))]
    {
        kqueue::user_event().unwrap_or(-1)
    }
}

/// Raise a [`new_wake_fd`] descriptor. Safe to call from any thread.
pub(crate) fn signal_wake_fd(fd: i32) {
    #[cfg(target_os = "linux")]
    {
        let one: u64 = 1;
        unsafe {
            libc::write(fd, std::ptr::addr_of!(one).cast(), 8);
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        kqueue::trigger(fd);
    }
}

/// Acknowledge a timer or wake descriptor that `poll(2)` reported readable,
/// so that it stops being reported.
///
/// Linux: read the 8-byte expiration / wake count (the count itself is not
/// needed - every caller re-checks its own state). Elsewhere: retrieve the
/// kqueue's pending event, which is the only thing that clears it.
pub(crate) fn drain_fd(fd: i32) {
    #[cfg(target_os = "linux")]
    {
        let mut count: u64 = 0;
        unsafe {
            libc::read(fd, std::ptr::addr_of_mut!(count).cast(), 8);
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        kqueue::drain(fd);
    }
}

/// The kqueue-backed timers and wakes used off Linux (see the module docs).
///
/// Each queue carries exactly ONE event, under `IDENT`, so a queue fd is a
/// drop-in for the timerfd / eventfd it replaces: it goes into the same poll
/// set and is closed with the same `close(2)`.
#[cfg(not(target_os = "linux"))]
mod kqueue {
    use std::io;

    /// The identifier of the single event each queue carries.
    const IDENT: libc::uintptr_t = 1;

    /// XNU fires a periodic `EVFILT_TIMER` whose period is under a
    /// microsecond ONCE and then never again. Measured on this machine: a
    /// 1 ns period gave one expiry in 200 ms, a 1 us period 200 000. The 0 ms
    /// "as fast as possible" timers (`Timer::tick_millis` returns 0 for them)
    /// are therefore armed at the smallest period that keeps firing - the
    /// counterpart of the 1 ns floor the Linux path needs for the same timers.
    const MIN_PERIOD_NS: u128 = 1_000;

    fn event(filter: i16, flags: u16, fflags: u32, data: libc::intptr_t) -> libc::kevent {
        libc::kevent {
            ident: IDENT,
            filter,
            flags,
            fflags,
            data,
            udata: std::ptr::null_mut(),
        }
    }

    fn apply(kq: i32, change: &libc::kevent) -> io::Result<()> {
        let rc = unsafe { libc::kevent(kq, change, 1, std::ptr::null_mut(), 0, std::ptr::null()) };
        if rc < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn new_queue_with(change: &libc::kevent) -> io::Result<i32> {
        let kq = unsafe { libc::kqueue() };
        if kq < 0 {
            return Err(io::Error::last_os_error());
        }
        unsafe {
            libc::fcntl(kq, libc::F_SETFD, libc::FD_CLOEXEC);
        }
        if let Err(e) = apply(kq, change) {
            unsafe {
                libc::close(kq);
            }
            return Err(e);
        }
        Ok(kq)
    }

    fn timer_event(period_ns: u128, oneshot: bool) -> libc::kevent {
        // `intptr_t` is `isize` on every unix; clamped into it, the cast is exact.
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let ns = period_ns.clamp(MIN_PERIOD_NS, isize::MAX as u128) as libc::intptr_t;
        // A periodic timer is EV_CLEAR: retrieving it resets it, the way
        // reading a timerfd does. A one-shot timer deletes itself once
        // retrieved.
        let mode = if oneshot {
            libc::EV_ONESHOT
        } else {
            libc::EV_CLEAR
        };
        event(
            libc::EVFILT_TIMER,
            libc::EV_ADD | libc::EV_ENABLE | mode,
            libc::NOTE_NSECONDS,
            ns,
        )
    }

    /// A queue carrying one timer, periodic or one-shot.
    pub(super) fn timer(period_ns: u128, oneshot: bool) -> io::Result<i32> {
        new_queue_with(&timer_event(period_ns, oneshot))
    }

    /// (Re-)arm the one-shot timer in `*kq`, creating the queue on first use.
    pub(super) fn arm_oneshot(kq: &mut i32, after_ns: u128) -> bool {
        let change = timer_event(after_ns, true);
        if *kq >= 0 {
            // EV_ADD on an existing (ident, filter) pair re-arms it in place.
            return apply(*kq, &change).is_ok();
        }
        match new_queue_with(&change) {
            Ok(fd) => {
                *kq = fd;
                true
            }
            Err(_) => false,
        }
    }

    /// A queue carrying one `EVFILT_USER` event, raised by [`trigger`].
    pub(super) fn user_event() -> io::Result<i32> {
        new_queue_with(&event(
            libc::EVFILT_USER,
            libc::EV_ADD | libc::EV_ENABLE | libc::EV_CLEAR,
            libc::NOTE_FFNOP,
            0,
        ))
    }

    /// Raise the `EVFILT_USER` event of a [`user_event`] queue.
    pub(super) fn trigger(kq: i32) {
        // Called from the notifier's thread, where there is nobody to report a
        // failure to; a lost wake costs the loop at most its next other wake.
        let _posted = apply(kq, &event(libc::EVFILT_USER, 0, libc::NOTE_TRIGGER, 0)).is_ok();
    }

    /// Room for everything a queue can have pending: its one event, with
    /// slack.
    const PENDING: usize = 4;

    /// Retrieve whatever the queue has pending - its one event.
    pub(super) fn drain(kq: i32) {
        let no_wait = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        let mut pending: [libc::kevent; PENDING] = unsafe { std::mem::zeroed() };
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let capacity = PENDING as libc::c_int;
        unsafe {
            libc::kevent(
                kq,
                std::ptr::null(),
                0,
                pending.as_mut_ptr(),
                capacity,
                &no_wait,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, time::Duration};

    use super::{
        arm_oneshot_timer, drain_fd, new_wake_fd, signal_wake_fd, start_timerfd, stop_timerfd,
    };

    /// Wait up to `budget_ms` for `fd` to become readable, i.e. for the timer
    /// to have fired at least once.
    fn fired_within(fd: i32, budget_ms: i32) -> bool {
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        unsafe { libc::poll(&mut pfd, 1, budget_ms) == 1 && (pfd.revents & libc::POLLIN) != 0 }
    }

    /// `Timer::tick_millis` legitimately returns 0 for a sub-millisecond / "as
    /// fast as possible" timer. A timerfd armed with `it_value == {0, 0}` is
    /// DISARMED, not "fire immediately and repeatedly", so those timers worked
    /// headlessly and silently never fired on Linux — animation and polling
    /// timers on both Linux backends.
    ///
    /// NEGATIVE CONTROL: revert the arming value to the raw milliseconds —
    /// `let secs = (interval_ms / 1000) as libc::time_t;` /
    /// `let nsecs = ((interval_ms % 1000) * 1_000_000) as libc::c_long;` — and
    /// the fd never becomes readable, so this fails.
    #[test]
    fn a_zero_millisecond_timer_is_armed_not_disarmed() {
        let mut fds = BTreeMap::new();
        start_timerfd(&mut fds, 1, 0, "test");

        let fd = *fds.get(&1).expect("timerfd_create/settime must succeed");
        assert!(
            fired_within(fd, 1000),
            "a 0 ms timer must tick as fast as the kernel allows, not never"
        );

        stop_timerfd(&mut fds, 1, "test");
        assert!(fds.is_empty());
    }

    /// An ordinary interval keeps working: the ns widening only reshapes the
    /// existing ms → (s, ns) split, so a 20 ms timer must still tick.
    #[test]
    fn an_ordinary_interval_still_fires() {
        let mut fds = BTreeMap::new();
        start_timerfd(&mut fds, 7, 20, "test");
        let fd = *fds.get(&7).expect("timerfd_create/settime must succeed");

        assert!(fired_within(fd, 2000), "a 20 ms timer must fire");

        stop_timerfd(&mut fds, 7, "test");
        assert!(fds.is_empty());
    }

    /// Re-arming a live timer must replace, not duplicate, its entry — every
    /// `set_timer` on the same id would otherwise leak a descriptor.
    #[test]
    fn restarting_a_timer_replaces_its_entry() {
        let mut fds = BTreeMap::new();
        start_timerfd(&mut fds, 3, 50, "test");
        let first = *fds.get(&3).unwrap();
        start_timerfd(&mut fds, 3, 50, "test");
        let second = *fds.get(&3).unwrap();

        assert_eq!(fds.len(), 1);
        assert_ne!(first, second, "the old descriptor must have been released");

        stop_timerfd(&mut fds, 3, "test");
    }

    /// Acknowledging a tick is what lets an idle loop park again. A kqueue
    /// answers `read(2)` with ENXIO and stays readable, so a loop that
    /// "acknowledged" it that way would spin at 100% CPU between ticks.
    ///
    /// NEGATIVE CONTROL: make `drain_fd` a no-op and the second assertion
    /// fails - the fd is still readable right after its tick was taken.
    #[test]
    fn a_drained_periodic_timer_is_quiet_until_its_next_tick() {
        let mut fds = BTreeMap::new();
        start_timerfd(&mut fds, 9, 500, "test");
        let fd = *fds.get(&9).expect("the timer must be created");

        assert!(fired_within(fd, 3000), "a 500 ms timer must fire");
        drain_fd(fd);
        assert!(
            !fired_within(fd, 50),
            "a drained timer must not report again before its next period"
        );

        stop_timerfd(&mut fds, 9, "test");
    }

    /// The frame-pace timer is ONE-SHOT: armed, it fires once; drained, it
    /// stays quiet until armed again - and re-arming reuses the descriptor
    /// rather than growing one per frame.
    #[test]
    fn a_oneshot_timer_fires_once_and_rearms_in_place() {
        let mut fd = -1;
        assert!(arm_oneshot_timer(&mut fd, Duration::from_millis(5)));
        assert!(fd >= 0);
        assert!(fired_within(fd, 2000), "an armed one-shot timer must fire");
        drain_fd(fd);
        assert!(
            !fired_within(fd, 50),
            "a one-shot timer must not fire a second time on its own"
        );

        let first = fd;
        assert!(arm_oneshot_timer(&mut fd, Duration::from_millis(5)));
        assert_eq!(fd, first, "re-arming must reuse the descriptor");
        assert!(fired_within(fd, 2000), "a re-armed timer must fire again");

        unsafe {
            libc::close(fd);
        }
    }

    /// The WebRender notifier's wake: raised from ANOTHER thread, it makes
    /// the loop's descriptor readable; drained, it is quiet again.
    #[test]
    fn a_wake_fd_wakes_the_poll_set_from_another_thread() {
        let fd = new_wake_fd();
        assert!(fd >= 0, "the wake descriptor must be created");
        assert!(!fired_within(fd, 0), "nothing was signalled yet");

        std::thread::spawn(move || signal_wake_fd(fd))
            .join()
            .expect("the signalling thread must not panic");
        assert!(fired_within(fd, 1000), "a signalled wake must be readable");

        drain_fd(fd);
        assert!(
            !fired_within(fd, 0),
            "a drained wake must stop reporting readable"
        );

        unsafe {
            libc::close(fd);
        }
    }
}
