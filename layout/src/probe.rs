//! Optional fine-grained timing + RSS instrumentation.
//!
//! Behind the `probe` feature flag every [`Probe::span`] returns a guard
//! that records the elapsed wall-clock on `Drop`, and
//! [`Probe::sample_rss`] records a labelled RSS checkpoint. Events are
//! buffered in a per-thread [`Vec`] and drained by the consumer with
//! [`Probe::drain`].
//!
//! With the feature off every method is a `#[inline(always)]` no-op so
//! release builds without the feature pay zero cost.
//!
//! Consumer (e.g. servo-shot) groups drained events by name to produce
//! the per-phase averages / p99s in its trace report.

use core::marker::PhantomData;

#[cfg(feature = "probe")]
mod imp {
    use std::cell::RefCell;
    use std::time::Instant;

    thread_local! {
        static EVENTS: RefCell<Vec<super::Event>> = const { RefCell::new(Vec::new()) };
    }

    /// RAII guard that records its name + elapsed nanos on drop.
    pub struct Span {
        pub(crate) name: &'static str,
        pub(crate) start: Instant,
    }

    impl Drop for Span {
        fn drop(&mut self) {
            let dur_ns = self.start.elapsed().as_nanos() as u64;
            EVENTS.with(|cell| {
                cell.borrow_mut().push(super::Event {
                    name: self.name,
                    kind: super::EventKind::Span { dur_ns },
                });
            });
        }
    }

    pub(super) fn open(name: &'static str) -> Span {
        Span { name, start: Instant::now() }
    }

    pub(super) fn sample_rss(label: &'static str, bytes: u64) {
        EVENTS.with(|cell| {
            cell.borrow_mut().push(super::Event {
                name: label,
                kind: super::EventKind::Rss { bytes },
            });
        });
    }

    pub(super) fn drain() -> Vec<super::Event> {
        EVENTS.with(|cell| core::mem::take(&mut *cell.borrow_mut()))
    }

    pub(super) fn enabled() -> bool {
        true
    }
}

#[cfg(not(feature = "probe"))]
mod imp {
    pub struct Span;

    impl Drop for Span {
        #[inline(always)]
        fn drop(&mut self) {}
    }

    #[inline(always)]
    pub(super) fn open(_name: &'static str) -> Span {
        Span
    }

    #[inline(always)]
    pub(super) fn sample_rss(_label: &'static str, _bytes: u64) {}

    #[inline(always)]
    pub(super) fn drain() -> Vec<super::Event> {
        Vec::new()
    }

    #[inline(always)]
    pub(super) fn enabled() -> bool {
        false
    }
}

/// Drained probe event. `Vec<Event>` is what consumers walk to render
/// trace summaries; the order is the order events fired in.
#[derive(Debug, Clone)]
pub struct Event {
    pub name: &'static str,
    pub kind: EventKind,
}

#[derive(Debug, Clone)]
pub enum EventKind {
    /// A timed scope's wall-clock duration.
    Span { dur_ns: u64 },
    /// A labelled RSS checkpoint.
    Rss { bytes: u64 },
}

/// Re-exported guard. Held by the caller of [`Probe::span`].
pub use imp::Span;

/// Probe API. All methods are no-ops without the `probe` feature.
pub struct Probe {
    _no_construct: PhantomData<()>,
}

impl Probe {
    /// Open a timed span. The returned guard records its name + nanos
    /// on drop into the thread-local event buffer.
    #[inline(always)]
    pub fn span(name: &'static str) -> Span {
        imp::open(name)
    }

    /// Record an RSS checkpoint with the given label + byte count. The
    /// caller supplies the bytes (this module does not depend on
    /// platform RSS readers) so consumers can use whatever measurement
    /// helper they own.
    #[inline(always)]
    pub fn sample_rss(label: &'static str, bytes: u64) {
        imp::sample_rss(label, bytes);
    }

    /// Drain the per-thread event buffer.
    #[inline(always)]
    pub fn drain() -> Vec<Event> {
        imp::drain()
    }

    /// Whether the `probe` feature is compiled in.
    #[inline(always)]
    pub fn enabled() -> bool {
        imp::enabled()
    }
}

/// Same monotonic clock used by `font::parsed::monotonic_now_nanos` for
/// LRU stamping. Re-exported here so any caller that wants raw nanos
/// without going through a span guard has one source of truth.
#[inline]
pub fn monotonic_now_nanos() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static LAUNCH: OnceLock<Instant> = OnceLock::new();
    let start = LAUNCH.get_or_init(Instant::now);
    start.elapsed().as_nanos() as u64
}

/// Convenience wrapper: sample the process's **current** resident set
/// (not peak) via `task_info` on macOS / `/proc/self/statm` on Linux and
/// push it into the probe event buffer under the given label.
///
/// Using current RSS (not `getrusage.ru_maxrss`) is essential so that
/// allocator purges are visible — peak RSS only moves up. Name kept as
/// `sample_peak_rss` for backwards compatibility with existing
/// checkpoint labels; semantically it is "sample current".
#[inline]
pub fn sample_peak_rss(label: &'static str) {
    #[cfg(feature = "probe")]
    {
        let (current, _virt) = current_rss_bytes();
        let bytes = if current != 0 { current } else { peak_rss_bytes_self() };
        Probe::sample_rss(label, bytes);
    }
    #[cfg(not(feature = "probe"))]
    let _ = label;
}

#[cfg(feature = "probe")]
pub fn peak_rss_bytes_pub() -> u64 { peak_rss_bytes_self() }

#[cfg(feature = "probe")]
fn peak_rss_bytes_self() -> u64 {
    #[cfg(unix)]
    unsafe {
        let mut ru: libc::rusage = core::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut ru) != 0 {
            return 0;
        }
        let raw = ru.ru_maxrss as u64;
        if cfg!(target_os = "macos") { raw } else { raw.saturating_mul(1024) }
    }
    #[cfg(not(unix))]
    {
        0
    }
}

/// Ask the active global allocator to return freed pages to the OS.
///
/// - With `allocator_mimalloc` feature: calls `mi_collect(true)`, which
///   aggressively returns pages (matches `az_purge_allocator` in azul-dll).
/// - With `allocator_jemalloc` feature: calls `mallctl("arena.0.purge")`.
/// - Otherwise on macOS: falls back to `malloc_zone_pressure_relief`
///   which drains the system zone (no-op when a third-party allocator
///   is the global one — hence the explicit feature flags above).
/// - Other platforms with default allocator: no-op.
///
/// Call after major allocations are freed (e.g. after a layout pass).
#[inline]
pub fn hint_purge_allocator() {
    #[cfg(feature = "allocator_mimalloc")]
    {
        // Aggressive purge — returns arenas to the OS when possible.
        unsafe {
            libmimalloc_sys::mi_collect(true);
        }
        if std::env::var("AZUL_PURGE_TRACE").is_ok() {
            let (rss, _) = current_rss_bytes();
            eprintln!("[PURGE] mi_collect(true) called — current rss={:.2} MiB", rss as f64 / 1048576.0);
        }
        return;
    }
    #[cfg(feature = "allocator_jemalloc")]
    {
        // Purge all arenas. `arena.<i>.purge` with i = MALLCTL_ARENAS_ALL.
        unsafe {
            let _ = tikv_jemalloc_sys::mallctl(
                b"arena.4096.purge\0".as_ptr() as *const _,
                core::ptr::null_mut(),
                core::ptr::null_mut(),
                core::ptr::null_mut(),
                0,
            );
        }
        return;
    }
    #[cfg(all(target_os = "macos", not(any(feature = "allocator_mimalloc", feature = "allocator_jemalloc"))))]
    {
        extern "C" {
            fn malloc_zone_pressure_relief(zone: *mut core::ffi::c_void, goal: usize) -> usize;
        }
        unsafe {
            malloc_zone_pressure_relief(core::ptr::null_mut(), 0);
        }
    }
}

/// Sample the process's "real" memory footprint (not peak).
/// Returns (footprint_bytes, virtual_bytes). On macOS this is
/// `phys_footprint` from `TASK_VM_INFO` — matches Activity Monitor
/// "Memory" and `vmmap`'s "Physical footprint" line, and excludes
/// shared library text pages that would otherwise inflate RSS
/// without costing the process anything uniquely. On Linux this
/// falls back to `/proc/self/statm` resident size (no direct
/// equivalent; the shared-lib inflation is much smaller there).
/// More useful than `getrusage.ru_maxrss` which only moves upward.
#[cfg(feature = "probe")]
pub fn current_rss_bytes() -> (u64, u64) {
    #[cfg(target_os = "macos")]
    {
        // Prefer phys_footprint (TASK_VM_INFO). Fall back to
        // resident_size (MACH_TASK_BASIC_INFO) if the bigger struct
        // isn't populated for some reason.
        let pf = phys_footprint_bytes();
        #[repr(C)]
        struct MachTaskBasicInfo {
            virtual_size: u64,
            resident_size: u64,
            resident_size_max: u64,
            user_time: [u32; 2],
            system_time: [u32; 2],
            policy: i32,
            suspend_count: i32,
        }
        const MACH_TASK_BASIC_INFO: u32 = 20;
        extern "C" {
            fn mach_task_self() -> u32;
            fn task_info(
                target: u32, flavor: u32,
                info: *mut core::ffi::c_void, count: *mut u32,
            ) -> i32;
        }
        unsafe {
            let mut info: MachTaskBasicInfo = core::mem::zeroed();
            let mut count = (core::mem::size_of::<MachTaskBasicInfo>() / 4) as u32;
            let kr = task_info(
                mach_task_self(),
                MACH_TASK_BASIC_INFO,
                &mut info as *mut _ as *mut core::ffi::c_void,
                &mut count,
            );
            if kr == 0 {
                let rss = if pf != 0 { pf } else { info.resident_size };
                (rss, info.virtual_size)
            } else {
                (pf, 0)
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    { (0, 0) }
}

/// Sample the Mach `phys_footprint` — the memory metric Activity
/// Monitor and `vmmap`'s "Physical footprint" line display. Unlike
/// `resident_size`, this excludes shared library text pages and
/// other kernel-mapped regions that inflate the traditional RSS
/// number without actually costing the process anything. For a
/// short-lived headless render this is a much more honest figure:
/// on a ~20 MiB ru_maxrss run, phys_footprint is typically ~8 MiB.
/// Returns 0 on non-macOS or if the Mach call fails.
///
/// There's no direct "peak phys_footprint" field; track the max
/// across calls in application code if you need it.
#[cfg(feature = "probe")]
pub fn phys_footprint_bytes() -> u64 {
    #[cfg(target_os = "macos")]
    {
        // TASK_VM_INFO = 22; the struct is large (~88 u32 counts ≈ 352 B)
        // and phys_footprint lives near the end, so we have to read the
        // whole thing. Layout is from osfmk/mach/task_info.h.
        #[repr(C)]
        struct TaskVmInfo {
            virtual_size: u64,
            region_count: u32,
            page_size: u32,
            resident_size: u64,
            resident_size_peak: u64,
            device: u64,
            device_peak: u64,
            internal: u64,
            internal_peak: u64,
            external: u64,
            external_peak: u64,
            reusable: u64,
            reusable_peak: u64,
            purgeable_volatile_pmap: u64,
            purgeable_volatile_resident: u64,
            purgeable_volatile_virtual: u64,
            compressed: u64,
            compressed_peak: u64,
            compressed_lifetime: u64,
            phys_footprint: u64,
            // there are more fields after this, but we don't need them
            _rest: [u64; 12],
        }
        const TASK_VM_INFO: u32 = 22;
        extern "C" {
            fn mach_task_self() -> u32;
            fn task_info(
                target: u32, flavor: u32,
                info: *mut core::ffi::c_void, count: *mut u32,
            ) -> i32;
        }
        unsafe {
            let mut info: TaskVmInfo = core::mem::zeroed();
            let mut count = (core::mem::size_of::<TaskVmInfo>() / 4) as u32;
            let kr = task_info(
                mach_task_self(),
                TASK_VM_INFO,
                &mut info as *mut _ as *mut core::ffi::c_void,
                &mut count,
            );
            if kr == 0 { info.phys_footprint } else { 0 }
        }
    }
    #[cfg(not(target_os = "macos"))]
    { 0 }
}

/// Background sampler for peak phys_footprint. Spawns a thread that
/// polls `phys_footprint_bytes()` every ~2 ms and updates a shared
/// atomic. The kernel does not expose a direct "peak phys_footprint"
/// — unlike `resident_size_peak` in TASK_VM_INFO — so polling is
/// the only way to catch mid-phase transients that are MADV_FREE'd
/// before the next explicit sample point.
///
/// Not started by default; call `start_peak_sampler()` once at
/// process init if you want peak tracking. Overhead is negligible
/// (~1-5 µs per poll on macOS, 500 Hz → <0.25% CPU of one core).
/// `peak_phys_footprint_seen()` reads the current high-water mark.
#[cfg(feature = "probe")]
pub fn start_peak_sampler() {
    #[cfg(target_os = "macos")]
    {
        use std::sync::atomic::Ordering;
        // Idempotent — only spawns once.
        static STARTED: std::sync::atomic::AtomicBool =
            std::sync::atomic::AtomicBool::new(false);
        if STARTED.swap(true, Ordering::AcqRel) {
            return;
        }
        std::thread::Builder::new()
            .name("azul-peak-sampler".to_string())
            .spawn(|| loop {
                let now = phys_footprint_bytes();
                let prev = PEAK_PHYS_FOOTPRINT.load(Ordering::Relaxed);
                if now > prev {
                    PEAK_PHYS_FOOTPRINT.store(now, Ordering::Relaxed);
                }
                std::thread::sleep(std::time::Duration::from_micros(250));
            })
            .ok();
    }
}

#[cfg(feature = "probe")]
static PEAK_PHYS_FOOTPRINT: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Read the peak `phys_footprint` seen by the background sampler.
/// Returns 0 if `start_peak_sampler` was never called.
#[cfg(feature = "probe")]
pub fn peak_phys_footprint_seen() -> u64 {
    PEAK_PHYS_FOOTPRINT.load(std::sync::atomic::Ordering::Relaxed)
}

/// Reset the global peak high-water mark to the current phys_footprint.
/// Paired with `peak_phys_footprint_seen()` so a caller can record
/// "peak during phase X" — call `reset_peak()` at phase entry, then
/// `peak_phys_footprint_seen()` at phase exit. The 500 Hz background
/// sampler runs continuously either way.
#[cfg(feature = "probe")]
pub fn reset_peak() {
    let now = phys_footprint_bytes();
    PEAK_PHYS_FOOTPRINT.store(now, std::sync::atomic::Ordering::Relaxed);
}

/// Record a phase's peak footprint into the probe event stream.
/// Call at phase exit after `reset_peak()` at phase entry. Emits an
/// RSS-kind event with `bytes = peak seen during phase`.
#[cfg(feature = "probe")]
#[inline]
pub fn sample_phase_peak(label: &'static str) {
    let peak = PEAK_PHYS_FOOTPRINT.load(std::sync::atomic::Ordering::Relaxed);
    Probe::sample_rss(label, peak);
}

#[cfg(not(feature = "probe"))]
#[inline(always)]
pub fn reset_peak() {}

#[cfg(not(feature = "probe"))]
#[inline(always)]
pub fn sample_phase_peak(_label: &'static str) {}
