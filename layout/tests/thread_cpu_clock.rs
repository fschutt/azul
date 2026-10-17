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
//!
//! On a machine with performance and efficiency cores (Apple silicon) the CPU
//! time of the same work depends on the core it ran on, so a budget is
//! measured inside [`on_a_performance_core`].

use std::time::Duration;

/// macOS's `QOS_CLASS_USER_INTERACTIVE`: the class macOS runs on the
/// performance cores first.
pub const QOS_CLASS_USER_INTERACTIVE: u32 = 0x21;

#[cfg(any(target_os = "macos", target_os = "ios"))]
extern "C" {
    fn pthread_self() -> *mut core::ffi::c_void;
    fn pthread_get_qos_class_np(
        thread: *mut core::ffi::c_void,
        qos_class: *mut u32,
        relative_priority: *mut i32,
    ) -> i32;
    fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: i32) -> i32;
}

/// The calling thread's QoS class (`None` where there are no classes).
pub fn qos_class_of_this_thread() -> Option<u32> {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        let (mut class, mut priority) = (0u32, 0i32);
        // SAFETY: both out-pointers point to live locals; `pthread_self` is
        // the calling thread.
        let rc = unsafe { pthread_get_qos_class_np(pthread_self(), &mut class, &mut priority) };
        (rc == 0).then_some(class)
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    {
        None
    }
}

/// Runs `measure` with the calling thread in the user-interactive class, so
/// it runs on a performance core while the suite's other tests run at the
/// default class, then gives the thread its own class back. Elsewhere it
/// just runs `measure`.
pub fn on_a_performance_core<R>(measure: impl FnOnce() -> R) -> R {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        let before = qos_class_of_this_thread();
        // SAFETY: sets the calling thread's own class; a failure leaves it as
        // it was, which only makes the measurement as noisy as before.
        unsafe { pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, 0) };
        let result = measure();
        // A thread that had no class of its own (0, unspecified) cannot be
        // given that back; it gets the default class, which it ran at.
        let back = match before {
            Some(class) if class != 0 => class,
            _ => 0x15,
        };
        // SAFETY: as above.
        unsafe { pthread_set_qos_class_self_np(back, 0) };
        result
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    {
        measure()
    }
}

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
