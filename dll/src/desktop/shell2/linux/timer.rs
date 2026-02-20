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
            let secs = (interval_ms / 1000) as i64;
            let nsecs = ((interval_ms % 1000) * 1_000_000) as i64;
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
                timer_fds.insert(timer_id, fd);
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
                *libc::__errno_location()
            );
        }
    }
}

/// Close and remove a timerfd from the timer_fds map.
pub fn stop_timerfd(
    timer_fds: &mut BTreeMap<usize, i32>,
    timer_id: usize,
    backend_name: &str,
) {
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
