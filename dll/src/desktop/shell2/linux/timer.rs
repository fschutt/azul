//! Shared Linux timer implementation using timerfd.
//!
//! Both X11 and Wayland use identical timerfd-based timer logic.
//! This module provides the shared implementation.

use std::collections::BTreeMap;

use crate::desktop::shell2::common::debug_server::LogCategory;
use crate::{log_debug, log_error};

/// Create a timerfd and insert it into the timer_fds map.
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{start_timerfd, stop_timerfd};

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
}
