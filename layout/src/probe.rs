//! Optional fine-grained timing + RSS instrumentation.
//!
//! Behind the `probe` feature flag every [`Probe::span`] returns a guard
//! that records the elapsed wall-clock on `Drop`, and
//! [`Probe::sample_rss`] records a labelled RSS checkpoint. Events are
//! buffered in a per-thread [`Vec`] and drained by the consumer with
//! [`Probe::drain`].
//!
//! With the feature off every method is a `#[inline]` no-op so
//! release builds without the feature pay zero cost.
//!
//! Consumer (e.g. servo-shot) groups drained events by name to produce
//! the per-phase averages / p99s in its trace report.

use core::marker::PhantomData;

// WASM gate: `Instant::now()` panics on browser WASM (no monotonic clock)
// and `libc::getrusage` isn't available, so on `target_family = "wasm"`
// we drop to the no-op stubs even when the `probe` feature is on.
// `AZ_PROFILE=cpu` then prints "(probe unavailable on this target)"
// rather than crashing.

// [WEB-LIFT 2026-06-11] `web_lift` also forces the no-op imp: the real
// module is Instant::now (mach-time syscall, out-of-image when lifted) +
// thread-local pushes + first-access dtor registration (`_tlv_atexit`).
// With the TLV emulation in place TLS "works", which flips these from
// harmlessly-failing (`try_with` Err) to actually-running — and the
// mach/atexit extern calls inside are unliftable. Profiling is
// meaningless in lifted wasm; the dylib built with `web-transpiler*`
// (which enables `web_lift`) is the web-server build, so desktop
// release builds keep real probes.
#[cfg(all(
    feature = "probe",
    not(target_family = "wasm"),
    not(feature = "web_lift")
))]
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
            // try_with (not with): the lifted-to-wasm web backend has no real
            // TLS, so `with` hits panic_access_error. These probe accesses are
            // inlined into layout_dom_recursive/layout_document, so they can't
            // be stubbed at the symbol level — use the non-panicking access.
            let _ = EVENTS.try_with(|cell| {
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
        // try_with: see Span::drop — no real TLS in the lifted wasm backend.
        let _ = EVENTS.try_with(|cell| {
            cell.borrow_mut().push(super::Event {
                name: label,
                kind: super::EventKind::Rss { bytes },
            });
        });
    }

    pub(super) fn drain() -> Vec<super::Event> {
        EVENTS
            .try_with(|cell| core::mem::take(&mut *cell.borrow_mut()))
            .unwrap_or_default()
    }

    pub(super) fn drop_events() {
        let _ = EVENTS.try_with(|cell| cell.borrow_mut().clear());
    }

    pub(super) fn peek_len() -> usize {
        EVENTS.try_with(|cell| cell.borrow().len()).unwrap_or(0)
    }

    pub(super) fn enabled() -> bool {
        true
    }
}

#[cfg(any(
    not(feature = "probe"),
    target_family = "wasm",
    feature = "web_lift"
))]
mod imp {
    #[derive(Debug)]
    pub struct Span;

    impl Drop for Span {
        #[inline]
        fn drop(&mut self) {}
    }

    #[inline]
    pub(super) const fn open(_name: &'static str) -> Span {
        Span
    }

    #[inline]
    pub(super) const fn sample_rss(_label: &'static str, _bytes: u64) {}

    #[inline]
    pub(super) const fn drain() -> Vec<super::Event> {
        Vec::new()
    }

    #[inline]
    pub(super) const fn drop_events() {}

    #[inline]
    pub(super) const fn peek_len() -> usize { 0 }

    #[inline]
    pub(super) const fn enabled() -> bool {
        false
    }
}

/// Drained probe event. `Vec<Event>` is what consumers walk to render
/// trace summaries; the order is the order events fired in.
#[derive(Copy, Debug, Clone)]
pub struct Event {
    pub name: &'static str,
    pub kind: EventKind,
}

#[derive(Copy, Debug, Clone)]
pub enum EventKind {
    /// A timed scope's wall-clock duration.
    Span { dur_ns: u64 },
    /// A labelled RSS checkpoint.
    Rss { bytes: u64 },
}

/// Re-exported guard. Held by the caller of [`Probe::span`].
pub use imp::Span;

/// Probe API. All methods are no-ops without the `probe` feature.
#[derive(Copy, Clone, Debug)]
pub struct Probe {
    _no_construct: PhantomData<()>,
}

impl Probe {
    /// Open a timed span. The returned guard records its name + nanos
    /// on drop into the thread-local event buffer.
    #[inline]
    // const only in the no-`probe` stub config; enabled `imp::` calls are non-const
    #[allow(clippy::missing_const_for_fn)]
    #[must_use] pub fn span(name: &'static str) -> Span {
        imp::open(name)
    }

    /// Record an RSS checkpoint with the given label + byte count. The
    /// caller supplies the bytes (this module does not depend on
    /// platform RSS readers) so consumers can use whatever measurement
    /// helper they own.
    #[inline]
    // const only in the no-`probe` stub config; enabled `imp::` calls are non-const
    #[allow(clippy::missing_const_for_fn)]
    pub fn sample_rss(label: &'static str, bytes: u64) {
        imp::sample_rss(label, bytes);
    }

    /// Drain the per-thread event buffer.
    #[inline]
    // const only in the no-`probe` stub config; enabled `imp::` calls are non-const
    #[allow(clippy::missing_const_for_fn)]
    #[must_use] pub fn drain() -> Vec<Event> {
        imp::drain()
    }

    /// Discard the per-thread event buffer without allocating a `Vec` to
    /// hand back. Used by long-running harnesses (e.g. `AZ_E2E_TEST`) that
    /// want to prevent the thread-local buffer from inflating RSS during
    /// thousands of layout passes without actually needing the events.
    #[inline]
    // const only in the no-`probe` stub config; enabled `imp::` calls are non-const
    #[allow(clippy::missing_const_for_fn)]
    pub fn drop_events() {
        imp::drop_events();
    }

    /// Current number of events in the per-thread buffer. Cheap to call.
    #[inline]
    // const only in the no-`probe` stub config; enabled `imp::` calls are non-const
    #[allow(clippy::missing_const_for_fn)]
    #[must_use] pub fn peek_len() -> usize {
        imp::peek_len()
    }

    /// Whether the `probe` feature is compiled in.
    #[inline]
    // const only in the no-`probe` stub config; enabled `imp::` calls are non-const
    #[allow(clippy::missing_const_for_fn)]
    #[must_use] pub fn enabled() -> bool {
        imp::enabled()
    }
}

/// Same monotonic clock used by `font::parsed::monotonic_now_nanos` for
/// LRU stamping. Re-exported here so any caller that wants raw nanos
/// without going through a span guard has one source of truth.
#[inline]
#[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/font/fixed-point/debug-marker cast
pub fn monotonic_now_nanos() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static LAUNCH: OnceLock<Instant> = OnceLock::new();
    let start = LAUNCH.get_or_init(Instant::now);
    start.elapsed().as_nanos() as u64
}

/// Format drained probe events as a per-phase timing table to stderr.
///
/// Groups `EventKind::Span` by name and prints count / total / avg / p99 /
/// max in µs. `EventKind::Rss` checkpoints print in wall-clock order with
/// deltas so allocator purges are visible.
///
/// Sorted by total-ns descending so the slowest phase is on top — ideal
/// for spotting which phase spiked during a stuttering frame.
///
/// Called by `AZ_PROFILE=cpu` dumps (both initial layout and relayout),
/// and also by external consumers like `servo-shot --azul-trace`.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)] // bounded graphics/coord/font/fixed-point/debug-marker cast
/// # Panics
///
/// Panics if the collected timing-sample list is empty.
pub fn print_drained_events(label: &str, events: &[Event]) {
    use std::collections::BTreeMap;

    if events.is_empty() {
        if Probe::enabled() {
            eprintln!("[CPU] {label}: no events recorded this pass");
        } else {
            // Feature absent or target-family disabled (WASM): show "???"
            // instead of a misleading "compile with feature=probe" hint.
            eprintln!(
                "[CPU] {label}: probe unavailable on this target (timings = ???)"
            );
        }
        return;
    }

    let mut spans: BTreeMap<&'static str, Vec<u64>> = BTreeMap::new();
    let mut rss_marks: Vec<(&'static str, u64)> = Vec::new();
    for ev in events {
        match ev.kind {
            EventKind::Span { dur_ns } => spans.entry(ev.name).or_default().push(dur_ns),
            EventKind::Rss { bytes } => rss_marks.push((ev.name, bytes)),
        }
    }

    let mut rows: Vec<(&'static str, usize, u64, u64, u64, u64)> = spans
        .into_iter()
        .map(|(name, mut ns)| {
            ns.sort_unstable();
            let n = ns.len();
            let total: u128 = ns.iter().map(|&x| u128::from(x)).sum();
            let avg = (total / n.max(1) as u128) as u64;
            let p99 = ns[(n.saturating_sub(1) * 99) / 100];
            let max = *ns.last().unwrap();
            (name, n, total as u64, avg, p99, max)
        })
        .collect();
    rows.sort_by(|a, b| b.2.cmp(&a.2));

    eprintln!("[CPU] === {label} ({} phases) ===", rows.len());
    eprintln!(
        "[CPU] {:<28}  {:>5}  {:>10}  {:>9}  {:>9}  {:>9}",
        "phase", "n", "total(µs)", "avg(µs)", "p99(µs)", "max(µs)"
    );
    for (name, n, total, avg, p99, max) in &rows {
        eprintln!(
            "[CPU] {:<28}  {:>5}  {:>10.1}  {:>9.2}  {:>9.2}  {:>9.2}",
            name,
            n,
            (*total as f64) / 1_000.0,
            (*avg as f64) / 1_000.0,
            (*p99 as f64) / 1_000.0,
            (*max as f64) / 1_000.0,
        );
    }
    if !rss_marks.is_empty() {
        eprintln!("[CPU]   -- RSS checkpoints (wall-clock order) --");
        let mut prev: Option<u64> = None;
        for (lbl, bytes) in &rss_marks {
            let delta = prev
                .map(|p| {
                    let diff = i128::from(*bytes) - i128::from(p);
                    if diff >= 0 {
                        format!("  (Δ +{:.2} MiB)", diff as f64 / 1_048_576.0)
                    } else {
                        format!("  (Δ -{:.2} MiB)", -diff as f64 / 1_048_576.0)
                    }
                })
                .unwrap_or_default();
            eprintln!(
                "[CPU]   {:<28}  {:.2} MiB{}",
                lbl,
                *bytes as f64 / 1_048_576.0,
                delta
            );
            prev = Some(*bytes);
        }
    }
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
// const only without the `probe` feature; enabled path calls non-const RSS readers
#[allow(clippy::missing_const_for_fn)]
pub fn sample_peak_rss(label: &'static str) {
    // [WEB-LIFT 2026-06-11] also no-op under web_lift: current_rss_bytes/
    // peak_rss_bytes_self are mach syscalls (task_info/getrusage) —
    // out-of-image and unliftable. See the `imp` cfg note above.
    #[cfg(all(feature = "probe", not(feature = "web_lift")))]
    {
        let (current, _virt) = current_rss_bytes();
        let bytes = if current != 0 { current } else { peak_rss_bytes_self() };
        Probe::sample_rss(label, bytes);
    }
    #[cfg(any(not(feature = "probe"), feature = "web_lift"))]
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
// const only on the default-allocator no-op path (e.g. Linux); the mimalloc /
// jemalloc / macOS `malloc_zone_pressure_relief` bodies call non-const fns
#[allow(clippy::missing_const_for_fn)]
pub fn hint_purge_allocator() {
    #[cfg(feature = "allocator_mimalloc")]
    {
        // Aggressive purge — returns arenas to the OS when possible.
        unsafe {
            libmimalloc_sys::mi_collect(true);
        }
        static PURGE_TRACE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if *PURGE_TRACE.get_or_init(azul_core::profile::memory_enabled) {
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
    #[cfg(all(target_os = "macos", not(miri), not(any(feature = "allocator_mimalloc", feature = "allocator_jemalloc"))))]
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
    // Miri cannot call the mach `task_info` foreign function; memory profiling
    // is meaningless under Miri anyway, so report zero.
    #[cfg(miri)]
    return (0, 0);
    #[cfg(all(target_os = "macos", not(miri)))]
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

/// Heap bytes currently held by the libc allocator (`mstats.bytes_used`).
///
/// Unlike RSS, this is what *Rust* allocations plus anything else going
/// through the default malloc zone is actually holding — mmap regions
/// for thread stacks, GL buffers, file-mapped fonts, etc. are NOT counted.
/// A leak that shows up here points to a genuine heap retention (an Arc
/// chain never dropped, a Vec never shrunk, a `Box<T>` forgotten).
/// Returns 0 on non-macOS.
#[cfg(feature = "probe")]
pub fn malloc_heap_bytes() -> u64 {
    #[cfg(target_os = "macos")]
    {
        #[repr(C)]
        struct Mstats {
            bytes_total: usize,
            chunks_used: usize,
            bytes_used: usize,
            chunks_free: usize,
            bytes_free: usize,
        }
        extern "C" {
            fn mstats() -> Mstats;
        }
        unsafe { mstats().bytes_used as u64 }
    }
    #[cfg(not(target_os = "macos"))]
    { 0 }
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
    // Miri cannot call the mach `task_info` foreign function.
    #[cfg(miri)]
    return 0;
    #[cfg(all(target_os = "macos", not(miri)))]
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
#[inline]
pub const fn reset_peak() {}

#[cfg(not(feature = "probe"))]
#[inline]
pub const fn sample_phase_peak(_label: &'static str) {}

#[cfg(not(feature = "probe"))]
#[inline]
#[must_use] pub const fn malloc_heap_bytes() -> u64 { 0 }

/// Emit one `{"ev":"phase","label":L,"heap":N,"call":C}` line to the
/// JSONL file named by `AZ_PROFILE_OUT=<path>`. Only fires when
/// `AZ_PROFILE=heap,jsonl` is set *and* the path is given.
///
/// Each call auto-increments a monotonic `call` id so downstream
/// analyzers can group phases belonging to a single `regenerate_layout`
/// invocation.
///
/// `label` convention: `start` at function entry; `<step>` after each
/// phase completes; `end` at function exit. Heap Δ between adjacent
/// labels within the same call-id is the bytes retained by that phase.
///
/// Zero overhead when flags aren't set (two atomic loads). Zero overhead
/// when the `probe` feature is off (no-op stub).
#[cfg(feature = "probe")]
pub fn emit_phase_heap(label: &str) {
    use std::io::Write;
    if !heap_jsonl_enabled() { return; }
    let Some(p) = azul_core::profile::out_path() else { return };
    static CALL_ID: std::sync::atomic::AtomicU64 =
        std::sync::atomic::AtomicU64::new(0);
    // Auto-increment on every "start" label; "end" and intermediates reuse
    // the current id so all phases in one regenerate_layout invocation share
    // a call number.
    static CURRENT_CALL: std::sync::atomic::AtomicU64 =
        std::sync::atomic::AtomicU64::new(0);
    let call_id = if label == "start" {
        let next = CALL_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        CURRENT_CALL.store(next, std::sync::atomic::Ordering::Relaxed);
        next
    } else {
        CURRENT_CALL.load(std::sync::atomic::Ordering::Relaxed)
    };
    let heap = malloc_heap_bytes();
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(p)
    {
        let _ = writeln!(
            f,
            r#"{{"ev":"phase","call":{},"label":"{}","heap":{}}}"#,
            call_id, label, heap
        );
    }
}

#[cfg(not(feature = "probe"))]
#[inline]
pub const fn emit_phase_heap(_label: &str) {}

/// Like [`emit_phase_heap`] but attaches a numeric payload (e.g., a cache
/// size) to the JSONL record under the `"extra"` field.
///
/// Gated behind `AZ_PROFILE=heap,jsonl,detail` — the `detail` token opts
/// in to fine-grained probes that produce extra per-step records (one
/// per intermediate step inside a phase). Without `detail`, only the
/// coarser phase probes from [`emit_phase_heap`] fire.
#[cfg(feature = "probe")]
pub fn emit_phase_heap_extra(label: &str, extra: u64) {
    use std::io::Write;
    if !heap_jsonl_enabled() { return; }
    if !azul_core::profile::detail_enabled() { return; }
    let Some(p) = azul_core::profile::out_path() else { return };
    let heap = malloc_heap_bytes();
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(p)
    {
        let _ = writeln!(
            f,
            r#"{{"ev":"phase","call":0,"label":"{}","heap":{},"extra":{}}}"#,
            label, heap, extra
        );
    }
}

#[cfg(not(feature = "probe"))]
#[inline]
pub const fn emit_phase_heap_extra(_label: &str, _extra: u64) {}

/// Both `heap` and `jsonl` tokens active in `AZ_PROFILE` — the combination
/// that enables JSONL heap-probe emission. Either alone is a no-op.
#[cfg(feature = "probe")]
#[inline]
fn heap_jsonl_enabled() -> bool {
    let f = azul_core::profile::flags();
    f.heap && f.jsonl
}

/// Returns true iff `AZ_PROFILE=detail` is active. Kept as a public
/// re-export so downstream crates can write `azul_layout::probe::detail_enabled()`
/// without pulling in `azul_core::profile` directly.
#[cfg(feature = "probe")]
#[inline]
pub fn detail_enabled() -> bool {
    azul_core::profile::detail_enabled()
}

#[cfg(not(feature = "probe"))]
#[inline]
#[must_use] pub const fn detail_enabled() -> bool { false }

#[cfg(test)]
#[allow(clippy::let_underscore_drop, clippy::too_many_lines)]
mod autotest_generated {
    use super::*;

    /// Build a `&'static str` with arbitrary (possibly hostile) contents.
    /// Leaks — fine for a test binary, and the only way to feed adversarial
    /// text into the `&'static str` APIs (`Probe::span`, `sample_rss`, ...).
    fn leak(s: String) -> &'static str {
        Box::leak(s.into_boxed_str())
    }

    /// Clear this thread's event buffer so a test's assertions hold even when
    /// the suite runs with `--test-threads=1` (all tests on one thread share
    /// the same thread-local `EVENTS`).
    fn reset() {
        Probe::drop_events();
        assert_eq!(Probe::peek_len(), 0, "drop_events must leave an empty buffer");
    }

    fn span_ns(ev: &Event) -> Option<u64> {
        match ev.kind {
            EventKind::Span { dur_ns } => Some(dur_ns),
            EventKind::Rss { .. } => None,
        }
    }

    fn rss_bytes(ev: &Event) -> Option<u64> {
        match ev.kind {
            EventKind::Rss { bytes } => Some(bytes),
            EventKind::Span { .. } => None,
        }
    }

    // ---------------------------------------------------------------
    // enabled() / cfg invariants
    // ---------------------------------------------------------------

    #[test]
    fn enabled_matches_the_compiled_imp() {
        // `Probe::enabled()` is the single runtime source of truth for
        // "events actually get buffered"; it must track the cfg that selects
        // the real `imp` (probe on, not wasm, not web_lift).
        let expected = cfg!(all(
            feature = "probe",
            not(target_family = "wasm"),
            not(feature = "web_lift")
        ));
        assert_eq!(Probe::enabled(), expected);
        assert_eq!(imp::enabled(), expected);
    }

    #[test]
    fn enabled_is_pure_and_idempotent() {
        let first = Probe::enabled();
        for _ in 0..1000 {
            assert_eq!(Probe::enabled(), first);
        }
    }

    // ---------------------------------------------------------------
    // span / drain round-trips
    // ---------------------------------------------------------------

    #[test]
    fn span_round_trips_name_through_drain() {
        reset();
        {
            let _g = Probe::span("autotest_span_round_trip");
        }
        let events = Probe::drain();
        if Probe::enabled() {
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].name, "autotest_span_round_trip");
            assert!(span_ns(&events[0]).is_some(), "span guard must emit EventKind::Span");
        } else {
            assert!(events.is_empty(), "no-op imp must never buffer events");
        }
        assert_eq!(Probe::peek_len(), 0, "drain must empty the buffer");
    }

    #[test]
    fn nested_spans_drop_inner_first_and_outer_duration_is_the_larger() {
        reset();
        {
            let _outer = Probe::span("outer");
            {
                let _inner = Probe::span("inner");
            }
        }
        let events = Probe::drain();
        if !Probe::enabled() {
            assert!(events.is_empty());
            return;
        }
        assert_eq!(events.len(), 2);
        // Drop order is inner-then-outer, so the buffer order is the same.
        assert_eq!(events[0].name, "inner");
        assert_eq!(events[1].name, "outer");
        let inner = span_ns(&events[0]).expect("inner is a span");
        let outer = span_ns(&events[1]).expect("outer is a span");
        // The outer span strictly encloses the inner one in wall-clock time.
        assert!(
            outer >= inner,
            "outer span ({outer} ns) must cover the inner one ({inner} ns)"
        );
    }

    #[test]
    fn forgotten_span_guard_records_nothing() {
        reset();
        core::mem::forget(Probe::span("forgotten"));
        let events = Probe::drain();
        assert!(
            events.is_empty(),
            "a leaked guard never runs Drop, so it must not emit an event"
        );
    }

    #[test]
    fn many_spans_do_not_lose_or_reorder_events() {
        reset();
        const N: usize = 10_000;
        let names: Vec<&'static str> = (0..N).map(|i| leak(format!("phase_{i}"))).collect();
        for &name in &names {
            drop(Probe::span(name));
        }
        if Probe::enabled() {
            assert_eq!(Probe::peek_len(), N);
        } else {
            assert_eq!(Probe::peek_len(), 0);
        }
        let events = Probe::drain();
        if Probe::enabled() {
            assert_eq!(events.len(), N);
            for (i, ev) in events.iter().enumerate() {
                assert_eq!(ev.name, names[i], "event order must be emission order");
            }
        } else {
            assert!(events.is_empty());
        }
        assert_eq!(Probe::peek_len(), 0);
    }

    #[test]
    fn span_survives_hostile_unicode_and_huge_names() {
        reset();
        let hostile: Vec<&'static str> = vec![
            "",
            "\0embedded\0nul\0",
            "\n\r\t",
            "{}{:?}{0}%s%n",           // format-string-looking payloads
            "🦀👨‍👩‍👧‍👦🇩🇪",         // emoji + ZWJ sequence + flag
            "مرحبا بالعالم",           // RTL
            "e\u{0301}\u{0301}\u{0301}", // stacked combining marks
            leak("A".repeat(100_000)), // huge
            leak("\u{1F4A9}".repeat(10_000)),
        ];
        for &name in &hostile {
            drop(Probe::span(name));
        }
        let events = Probe::drain();
        if Probe::enabled() {
            assert_eq!(events.len(), hostile.len());
            for (ev, name) in events.iter().zip(hostile.iter()) {
                assert_eq!(ev.name, *name, "name must round-trip byte-for-byte");
            }
            // Formatting the hostile names must not panic either.
            print_drained_events("hostile-names", &events);
        } else {
            assert!(events.is_empty());
        }
    }

    #[test]
    fn drain_is_empty_the_second_time() {
        reset();
        drop(Probe::span("once"));
        let first = Probe::drain();
        let second = Probe::drain();
        if Probe::enabled() {
            assert_eq!(first.len(), 1);
        }
        assert!(second.is_empty(), "a drained buffer must stay drained");
    }

    // ---------------------------------------------------------------
    // sample_rss: numeric boundaries + exact round-trip
    // ---------------------------------------------------------------

    #[test]
    fn sample_rss_round_trips_every_numeric_boundary() {
        reset();
        let boundaries: [u64; 8] = [
            0,
            1,
            u64::from(u32::MAX),
            u64::from(u32::MAX) + 1,
            1 << 63,
            u64::MAX - 1,
            u64::MAX,
            0xDEAD_BEEF_DEAD_BEEF,
        ];
        for b in boundaries {
            Probe::sample_rss("bytes", b);
        }
        let events = Probe::drain();
        if !Probe::enabled() {
            assert!(events.is_empty());
            return;
        }
        assert_eq!(events.len(), boundaries.len());
        for (ev, expected) in events.iter().zip(boundaries.iter()) {
            assert_eq!(
                rss_bytes(ev),
                Some(*expected),
                "RSS byte counts must survive the buffer unchanged (no saturation)"
            );
        }
    }

    #[test]
    fn sample_rss_zero_is_recorded_not_skipped() {
        reset();
        Probe::sample_rss("zero", 0);
        let events = Probe::drain();
        if Probe::enabled() {
            assert_eq!(events.len(), 1, "a 0-byte checkpoint is still a checkpoint");
            assert_eq!(rss_bytes(&events[0]), Some(0));
            assert_eq!(events[0].name, "zero");
        } else {
            assert!(events.is_empty());
        }
    }

    // ---------------------------------------------------------------
    // peek_len / drop_events
    // ---------------------------------------------------------------

    #[test]
    fn peek_len_tracks_pushes_and_drop_events_clears() {
        reset();
        assert_eq!(Probe::peek_len(), 0);
        for i in 0..64u64 {
            Probe::sample_rss("tick", i);
        }
        if Probe::enabled() {
            assert_eq!(Probe::peek_len(), 64);
        } else {
            assert_eq!(Probe::peek_len(), 0);
        }
        Probe::drop_events();
        assert_eq!(Probe::peek_len(), 0, "drop_events must clear the buffer");
        assert!(
            Probe::drain().is_empty(),
            "drop_events must discard, not stash, the events"
        );
    }

    #[test]
    fn drop_events_on_an_empty_buffer_is_a_no_op() {
        reset();
        for _ in 0..100 {
            Probe::drop_events();
            assert_eq!(Probe::peek_len(), 0);
        }
    }

    #[test]
    fn peek_len_is_side_effect_free() {
        reset();
        Probe::sample_rss("keep", 7);
        let expected = if Probe::enabled() { 1 } else { 0 };
        for _ in 0..100 {
            assert_eq!(Probe::peek_len(), expected, "peek must not consume events");
        }
        let events = Probe::drain();
        assert_eq!(events.len(), expected);
    }

    // ---------------------------------------------------------------
    // thread-locality
    // ---------------------------------------------------------------

    #[test]
    fn event_buffer_is_per_thread() {
        reset();
        Probe::sample_rss("main_thread", 1);

        let child_len = std::thread::spawn(|| {
            // A fresh thread starts with an empty buffer, even though the
            // parent just pushed an event.
            assert_eq!(Probe::peek_len(), 0, "buffers must not be shared across threads");
            Probe::sample_rss("child_thread", 2);
            let drained = Probe::drain();
            for ev in &drained {
                assert_eq!(ev.name, "child_thread", "child must only see its own events");
            }
            drained.len()
        })
        .join()
        .expect("probe calls must not panic on a spawned thread");

        let events = Probe::drain();
        if Probe::enabled() {
            assert_eq!(child_len, 1);
            assert_eq!(events.len(), 1, "the child's drain must not touch our buffer");
            assert_eq!(events[0].name, "main_thread");
        } else {
            assert_eq!(child_len, 0);
            assert!(events.is_empty());
        }
    }

    // ---------------------------------------------------------------
    // imp:: (private) parity with the public facade
    // ---------------------------------------------------------------

    #[test]
    fn imp_facade_parity() {
        reset();
        {
            let _g = imp::open("imp_open");
        }
        imp::sample_rss("imp_rss", u64::MAX);
        let len = imp::peek_len();
        assert_eq!(len, Probe::peek_len());
        let events = imp::drain();
        assert_eq!(events.len(), len);
        assert_eq!(imp::peek_len(), 0);
        if Probe::enabled() {
            assert_eq!(events[0].name, "imp_open");
            assert_eq!(rss_bytes(&events[1]), Some(u64::MAX));
        } else {
            assert!(events.is_empty());
        }
        imp::drop_events();
        assert_eq!(imp::peek_len(), 0);
    }

    // ---------------------------------------------------------------
    // print_drained_events: the formatter is the panic-prone one
    // ---------------------------------------------------------------

    #[test]
    fn print_drained_events_empty_slice_does_not_panic() {
        // The doc comment claims it "Panics if the collected timing-sample
        // list is empty" — the implementation early-returns instead. Pin the
        // safe behaviour.
        print_drained_events("empty", &[]);
        print_drained_events("", &[]);
    }

    #[test]
    fn print_drained_events_rss_only_has_no_span_rows() {
        // With zero spans the row list is empty; the `ns.last().unwrap()` in
        // the row builder must never be reached.
        let events = [
            Event { name: "a", kind: EventKind::Rss { bytes: 0 } },
            Event { name: "b", kind: EventKind::Rss { bytes: u64::MAX } },
            Event { name: "c", kind: EventKind::Rss { bytes: 1 } },
        ];
        print_drained_events("rss-only", &events);
    }

    #[test]
    fn print_drained_events_p99_index_is_in_bounds_for_every_sample_count() {
        // p99 is `ns[(n - 1) * 99 / 100]` — an off-by-one here is an
        // out-of-bounds index. Walk the counts where it would bite.
        for n in [1usize, 2, 3, 99, 100, 101, 199, 200, 201, 1000] {
            let events: Vec<Event> = (0..n)
                .map(|i| Event {
                    name: "phase",
                    kind: EventKind::Span { dur_ns: i as u64 },
                })
                .collect();
            print_drained_events("p99", &events);
        }
    }

    #[test]
    fn print_drained_events_saturating_totals_do_not_panic() {
        // Summing u64::MAX durations overflows u64; the impl accumulates in
        // u128 and truncates for display, so this must not panic in a debug
        // build (overflow checks are on for `cargo test`).
        let events = [
            Event { name: "huge", kind: EventKind::Span { dur_ns: u64::MAX } },
            Event { name: "huge", kind: EventKind::Span { dur_ns: u64::MAX } },
            Event { name: "huge", kind: EventKind::Span { dur_ns: u64::MAX } },
            Event { name: "zero", kind: EventKind::Span { dur_ns: 0 } },
        ];
        print_drained_events("overflowing-total", &events);
    }

    #[test]
    fn print_drained_events_rss_delta_handles_full_u64_swing() {
        // The delta is computed in i128; a MAX -> 0 -> MAX swing is the worst
        // case for a naive i64/u64 subtraction.
        let events = [
            Event { name: "peak", kind: EventKind::Rss { bytes: u64::MAX } },
            Event { name: "trough", kind: EventKind::Rss { bytes: 0 } },
            Event { name: "peak_again", kind: EventKind::Rss { bytes: u64::MAX } },
        ];
        print_drained_events("delta-swing", &events);
    }

    #[test]
    fn print_drained_events_hostile_labels_and_names() {
        let big = leak("x".repeat(65_536));
        let events = [
            Event { name: "", kind: EventKind::Span { dur_ns: 1 } },
            Event { name: "{}{:?}", kind: EventKind::Span { dur_ns: 2 } },
            Event { name: big, kind: EventKind::Span { dur_ns: u64::MAX } },
            Event { name: "🦀\u{0301}\0", kind: EventKind::Rss { bytes: 1 } },
        ];
        print_drained_events(big, &events);
        print_drained_events("\0\n{}", &events);
    }

    #[test]
    fn print_drained_events_accepts_a_real_drain() {
        reset();
        {
            let _a = Probe::span("layout");
            let _b = Probe::span("layout");
        }
        Probe::sample_rss("after", 4096);
        let events = Probe::drain();
        print_drained_events("real-drain", &events);
    }

    // ---------------------------------------------------------------
    // monotonic_now_nanos
    // ---------------------------------------------------------------

    #[test]
    fn monotonic_now_nanos_never_goes_backwards() {
        let mut prev = monotonic_now_nanos();
        for _ in 0..10_000 {
            let now = monotonic_now_nanos();
            assert!(now >= prev, "clock went backwards: {prev} -> {now}");
            prev = now;
        }
    }

    #[test]
    fn monotonic_now_nanos_is_monotonic_across_threads() {
        // The `OnceLock<Instant>` launch stamp is process-global, so a value
        // read on another thread is comparable with one read here.
        let before = monotonic_now_nanos();
        let mid = std::thread::spawn(monotonic_now_nanos)
            .join()
            .expect("monotonic_now_nanos must not panic off the main thread");
        let after = monotonic_now_nanos();
        assert!(before <= mid && mid <= after, "{before} <= {mid} <= {after}");
    }

    // ---------------------------------------------------------------
    // sample_peak_rss / sample_phase_peak / reset_peak
    // ---------------------------------------------------------------

    #[test]
    fn sample_peak_rss_emits_exactly_one_labelled_event() {
        reset();
        sample_peak_rss("autotest_peak_rss");
        let events = Probe::drain();
        if Probe::enabled() {
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].name, "autotest_peak_rss");
            assert!(
                rss_bytes(&events[0]).is_some(),
                "sample_peak_rss must emit an Rss-kind event"
            );
        } else {
            assert!(events.is_empty());
        }
    }

    #[test]
    fn sample_phase_peak_emits_exactly_one_labelled_event() {
        reset();
        sample_phase_peak("autotest_phase_peak");
        let events = Probe::drain();
        if Probe::enabled() {
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].name, "autotest_phase_peak");
            assert!(rss_bytes(&events[0]).is_some());
        } else {
            assert!(events.is_empty());
        }
    }

    #[test]
    fn reset_peak_is_repeatable_and_side_effect_free_on_the_event_buffer() {
        reset();
        for _ in 0..100 {
            reset_peak();
        }
        assert_eq!(
            Probe::peek_len(),
            0,
            "reset_peak touches an atomic, it must not push events"
        );
    }

    #[test]
    fn hint_purge_allocator_is_repeatable_and_emits_nothing() {
        reset();
        for _ in 0..50 {
            hint_purge_allocator();
        }
        assert_eq!(Probe::peek_len(), 0, "purging must not push probe events");
    }

    // ---------------------------------------------------------------
    // malloc_heap_bytes / detail_enabled (both cfg worlds)
    // ---------------------------------------------------------------

    #[test]
    fn malloc_heap_bytes_is_zero_off_macos_and_deterministic() {
        let a = malloc_heap_bytes();
        let b = malloc_heap_bytes();
        if cfg!(all(feature = "probe", target_os = "macos")) {
            // Only the macOS `mstats()` path returns a real figure; both reads
            // are live samples so they need not be equal.
            let _ = (a, b);
        } else {
            // Documented: "Returns 0 on non-macOS" (and the no-probe stub is
            // `const fn -> 0`).
            assert_eq!(a, 0);
            assert_eq!(b, 0);
        }
    }

    #[test]
    fn detail_enabled_is_deterministic() {
        let first = detail_enabled();
        for _ in 0..100 {
            assert_eq!(detail_enabled(), first, "flag reads are cached, must not flap");
        }
        if !cfg!(feature = "probe") {
            assert!(!first, "the no-probe stub is a const `false`");
        }
    }

    // ---------------------------------------------------------------
    // emit_phase_heap / emit_phase_heap_extra (no-op unless
    // AZ_PROFILE=heap,jsonl + AZ_PROFILE_OUT; must never panic regardless)
    // ---------------------------------------------------------------

    #[test]
    fn emit_phase_heap_survives_hostile_labels() {
        reset();
        let huge = "L".repeat(65_536);
        let labels: Vec<&str> = vec![
            "",
            "start",
            "start", // repeated: exercises the call-id auto-increment
            "end",
            "\"quote\"", // would corrupt the emitted JSON if flags were on
            "back\\slash",
            "new\nline",
            "\0nul",
            "🦀 unicode",
            &huge,
        ];
        for l in &labels {
            emit_phase_heap(l);
        }
        assert_eq!(Probe::peek_len(), 0, "JSONL emission must not touch the span buffer");
    }

    #[test]
    fn emit_phase_heap_extra_survives_numeric_boundaries() {
        reset();
        for extra in [0u64, 1, u64::MAX / 2, u64::MAX - 1, u64::MAX] {
            emit_phase_heap_extra("autotest_extra", extra);
            emit_phase_heap_extra("", extra);
        }
        assert_eq!(Probe::peek_len(), 0);
    }

    // ---------------------------------------------------------------
    // Event / EventKind value type
    // ---------------------------------------------------------------

    #[test]
    fn event_is_copy_and_clone_preserving_payload() {
        let span = Event { name: "n", kind: EventKind::Span { dur_ns: u64::MAX } };
        let rss = Event { name: "n", kind: EventKind::Rss { bytes: u64::MAX } };
        let span_copy = span; // Copy
        #[allow(clippy::clone_on_copy)]
        let rss_clone = rss.clone();
        assert_eq!(span_ns(&span_copy), Some(u64::MAX));
        assert_eq!(rss_bytes(&rss_clone), Some(u64::MAX));
        // Span and Rss must not be confusable even with identical payloads.
        assert!(span_ns(&rss_clone).is_none());
        assert!(rss_bytes(&span_copy).is_none());
        // Debug must not panic on the extremes.
        let _ = format!("{span:?}{rss:?}");
    }

    // ---------------------------------------------------------------
    // probe-only platform readers
    // ---------------------------------------------------------------

    #[cfg(feature = "probe")]
    #[test]
    fn peak_rss_bytes_is_monotonic_and_agrees_with_the_pub_wrapper() {
        // ru_maxrss is a high-water mark, so it can only move up.
        let first = peak_rss_bytes_self();
        let pubbed = peak_rss_bytes_pub();
        let second = peak_rss_bytes_self();
        assert!(pubbed >= first, "peak RSS must never decrease: {first} -> {pubbed}");
        assert!(second >= pubbed, "peak RSS must never decrease: {pubbed} -> {second}");
        if cfg!(unix) && !cfg!(miri) {
            assert!(first > 0, "getrusage on a live unix process must report some RSS");
        }
    }

    #[cfg(feature = "probe")]
    #[test]
    fn current_rss_bytes_does_not_panic_and_is_self_consistent() {
        let (footprint, virt) = current_rss_bytes();
        if cfg!(all(target_os = "macos", not(miri))) {
            assert!(footprint > 0, "macOS must report a non-zero footprint");
            assert!(virt >= footprint || virt == 0);
        }
        // Repeated sampling must stay panic-free (foreign-fn call each time).
        for _ in 0..100 {
            let _ = current_rss_bytes();
        }
    }

    #[cfg(feature = "probe")]
    #[test]
    fn phys_footprint_bytes_is_zero_off_macos() {
        let v = phys_footprint_bytes();
        if cfg!(all(target_os = "macos", not(miri))) {
            assert!(v > 0);
        } else {
            assert_eq!(v, 0, "documented: returns 0 on non-macOS / under miri");
        }
    }

    #[cfg(feature = "probe")]
    #[test]
    fn start_peak_sampler_is_idempotent() {
        // Documented as "Idempotent — only spawns once"; calling it in a loop
        // must not spawn 200 threads or panic.
        for _ in 0..200 {
            start_peak_sampler();
        }
        let _ = peak_phys_footprint_seen();
    }

    #[cfg(feature = "probe")]
    #[test]
    fn peak_phys_footprint_seen_is_readable_without_a_sampler() {
        // Documented: "Returns 0 if start_peak_sampler was never called."
        // Other tests in this binary may have started it / reset it, so only
        // the non-macOS path (where phys_footprint is always 0) is assertable.
        let seen = peak_phys_footprint_seen();
        if !cfg!(target_os = "macos") {
            assert_eq!(seen, 0, "no phys_footprint source off macOS => peak stays 0");
        }
    }

    #[cfg(feature = "probe")]
    #[test]
    fn heap_jsonl_enabled_matches_the_profile_flags() {
        let f = azul_core::profile::flags();
        assert_eq!(
            heap_jsonl_enabled(),
            f.heap && f.jsonl,
            "either token alone must be a no-op"
        );
        let first = heap_jsonl_enabled();
        for _ in 0..100 {
            assert_eq!(heap_jsonl_enabled(), first, "flags are cached, must not flap");
        }
    }
}
