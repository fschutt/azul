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

// WASM gate: `Instant::now()` panics on browser WASM (no monotonic clock)
// and `libc::getrusage` isn't available, so on `target_family = "wasm"`
// we drop to the no-op stubs even when the `probe` feature is on.
// `AZ_PROFILE=cpu` then prints "(probe unavailable on this target)"
// rather than crashing.

#[cfg(all(feature = "probe", not(target_family = "wasm")))]
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

    pub(super) fn drop_events() {
        EVENTS.with(|cell| cell.borrow_mut().clear());
    }

    pub(super) fn peek_len() -> usize {
        EVENTS.with(|cell| cell.borrow().len())
    }

    pub(super) fn enabled() -> bool {
        true
    }
}

#[cfg(any(not(feature = "probe"), target_family = "wasm"))]
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
    pub(super) fn drop_events() {}

    #[inline(always)]
    pub(super) fn peek_len() -> usize { 0 }

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

    /// Discard the per-thread event buffer without allocating a `Vec` to
    /// hand back. Used by long-running harnesses (e.g. `AZ_E2E_TEST`) that
    /// want to prevent the thread-local buffer from inflating RSS during
    /// thousands of layout passes without actually needing the events.
    #[inline(always)]
    pub fn drop_events() {
        imp::drop_events();
    }

    /// Current number of events in the per-thread buffer. Cheap to call.
    #[inline(always)]
    pub fn peek_len() -> usize {
        imp::peek_len()
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
pub fn print_drained_events(label: &str, events: &[Event]) {
    use std::collections::BTreeMap;

    if events.is_empty() {
        if !Probe::enabled() {
            // Feature absent or target-family disabled (WASM): show "???"
            // instead of a misleading "compile with feature=probe" hint.
            eprintln!(
                "[CPU] {label}: probe unavailable on this target (timings = ???)"
            );
        } else {
            eprintln!("[CPU] {label}: no events recorded this pass");
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
            let total: u128 = ns.iter().map(|&x| x as u128).sum();
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
                    let diff = *bytes as i128 - p as i128;
                    if diff >= 0 {
                        format!("  (Δ +{:.2} MiB)", diff as f64 / 1048576.0)
                    } else {
                        format!("  (Δ -{:.2} MiB)", -diff as f64 / 1048576.0)
                    }
                })
                .unwrap_or_default();
            eprintln!(
                "[CPU]   {:<28}  {:.2} MiB{}",
                lbl,
                *bytes as f64 / 1048576.0,
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

#[cfg(not(feature = "probe"))]
#[inline(always)]
pub fn malloc_heap_bytes() -> u64 { 0 }

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
#[inline(always)]
pub fn emit_phase_heap(_label: &str) {}

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
#[inline(always)]
pub fn emit_phase_heap_extra(_label: &str, _extra: u64) {}

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
#[inline(always)]
pub fn detail_enabled() -> bool { false }
