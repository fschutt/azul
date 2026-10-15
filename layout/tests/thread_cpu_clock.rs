//! The CPU time the calling thread has run: the clock of the release-only
//! timing budgets.
//!
//! A budget measured on the wall clock measures the machine. The suite runs
//! its tests on every core at once, beside whatever else the machine does,
//! and a thread waiting for a core is billed for the wait: a frame that takes
//! 5 ms alone read 20 ms in a loaded run. The thread's own CPU clock advances
//! only while the thread runs, so a budget on it holds under load and stays
//! as tight as it was written. Where the platform has no such clock (other
//! than macOS, iOS, Linux and Android) it is the wall clock.

use std::time::Duration;

/// A point on the calling thread's CPU clock.
#[derive(Debug, Clone, Copy)]
pub struct CpuInstant(Duration);

impl CpuInstant {
    /// Now, on the calling thread's CPU clock.
    pub fn now() -> Self {
        Self(thread_cpu_time())
    }

    /// The CPU time the calling thread has run since `self`. Read it on the
    /// thread that took `self`: another thread's clock counts another
    /// thread's work.
    pub fn elapsed(&self) -> Duration {
        thread_cpu_time().saturating_sub(self.0)
    }
}

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "linux",
    target_os = "android"
))]
fn thread_cpu_time() -> Duration {
    /// `struct timespec` of the 64-bit targets the suite runs on.
    #[repr(C)]
    struct Timespec {
        tv_sec: i64,
        tv_nsec: i64,
    }
    extern "C" {
        fn clock_gettime(clock: i32, out: *mut Timespec) -> i32;
    }
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    const CLOCK_THREAD_CPUTIME_ID: i32 = 16;
    #[cfg(any(target_os = "linux", target_os = "android"))]
    const CLOCK_THREAD_CPUTIME_ID: i32 = 3;

    let mut now = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `clock_gettime` writes one `timespec` through a pointer to a
    // live, writable one.
    let rc = unsafe { clock_gettime(CLOCK_THREAD_CPUTIME_ID, &mut now) };
    assert_eq!(rc, 0, "the thread's CPU clock cannot be read");
    Duration::new(
        u64::try_from(now.tv_sec).unwrap_or(0),
        u32::try_from(now.tv_nsec).unwrap_or(0),
    )
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "linux",
    target_os = "android"
)))]
fn thread_cpu_time() -> Duration {
    use std::{sync::OnceLock, time::Instant};
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed()
}

#[test]
fn a_threads_cpu_clock_counts_its_work_and_not_its_sleep() {
    let start = CpuInstant::now();
    std::thread::sleep(Duration::from_millis(30));
    let asleep = start.elapsed();
    let start = CpuInstant::now();
    let mut x = 1u64;
    let wall = std::time::Instant::now();
    while wall.elapsed() < Duration::from_millis(30) {
        x = core::hint::black_box(x.wrapping_mul(6364136223846793005).wrapping_add(1));
    }
    let busy = start.elapsed();
    if cfg!(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "linux",
        target_os = "android"
    )) {
        assert!(
            asleep < Duration::from_millis(10),
            "30 ms asleep read {asleep:?} of CPU time"
        );
    }
    assert!(
        busy >= Duration::from_millis(5),
        "30 ms of work read {busy:?} of CPU time"
    );
}

/// A budget is measured on a performance core where the machine has two
/// kinds: on an Apple M2 (four performance, four efficiency cores) the
/// suite's other tests push a timing test onto an efficiency core half the
/// time, where the same frame takes three to four times the CPU time (7 ms
/// alone, 22-26 ms in the loaded suite). The measuring thread asks for the
/// user-interactive class, which macOS runs on the performance cores first,
/// and gets its own class back afterwards.
#[test]
fn a_budget_is_measured_on_a_performance_core() {
    let before = qos_class_of_this_thread();
    let inside = on_a_performance_core(qos_class_of_this_thread);
    if cfg!(any(target_os = "macos", target_os = "ios")) {
        assert_eq!(inside, Some(QOS_CLASS_USER_INTERACTIVE), "the measuring thread's class");
    }
    assert_eq!(
        qos_class_of_this_thread(),
        before,
        "the thread has its own class again after the measurement"
    );
}
