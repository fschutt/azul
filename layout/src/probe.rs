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
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::time::Instant;

    thread_local! {
        static EVENTS: RefCell<Vec<super::Event>> = const { RefCell::new(Vec::new()) };
        /// Currently-open span count. Read when a span OPENS (its own
        /// depth) and decremented when it closes.
        static DEPTH: std::cell::Cell<u16> = const { std::cell::Cell::new(0) };
        /// Names of the currently-open spans, outermost first. Maintained
        /// UNCONDITIONALLY (even with recording off): this is what a crash
        /// report reads as "what scope was the app in" — a diagnostic that
        /// must not depend on AZ_PROFILE being set. Cost: one push/pop of a
        /// `&'static str` per span.
        static SPAN_NAMES: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
    }

    /// Whether spans/samples are RECORDED. The `probe` feature being compiled
    /// in used to mean "always record" — but the dll builds with `probe` on
    /// unconditionally, and the event buffer is only drained by the
    /// `AZ_PROFILE=cpu` report path. Every plain run therefore pushed ~40 B
    /// per span into a thread-local Vec that nothing ever emptied: unbounded
    /// growth, invisible to the `LayoutCache` memory walk (it's a thread-local).
    /// A 5 s resize drag alone is ~375 relayouts × hundreds of spans.
    ///
    /// 0 = uninitialized (resolve from `AZ_PROFILE` on first probe),
    /// 1 = recording, 2 = off.
    static RECORDING: AtomicU8 = AtomicU8::new(0);

    #[inline]
    fn recording() -> bool {
        match RECORDING.load(Ordering::Relaxed) {
            1 => true,
            2 => false,
            _ => {
                // First probe anywhere resolves the mode once. Any profile
                // mode that can consume events counts; the write is
                // idempotent so a racing thread resolving the same env is
                // harmless.
                let on = azul_core::profile::cpu_enabled()
                    || azul_core::profile::memory_enabled()
                    || azul_core::profile::heap_enabled();
                RECORDING.store(if on { 1 } else { 2 }, Ordering::Relaxed);
                on
            }
        }
    }

    pub(super) fn set_recording(on: bool) {
        RECORDING.store(if on { 1 } else { 2 }, Ordering::Relaxed);
    }

    /// RAII guard that records its name + elapsed nanos on drop.
    /// `start == None` means recording was off when the span opened: the
    /// guard is inert (no clock read on open, no TLS touch on drop).
    #[derive(Debug)]
    pub struct Span {
        pub(crate) name: &'static str,
        pub(crate) start: Option<Instant>,
        pub(crate) depth: u16,
    }

    impl Drop for Span {
        fn drop(&mut self) {
            let _ = SPAN_NAMES.try_with(|st| {
                st.borrow_mut().pop();
            });
            let Some(start) = self.start else { return };
            let dur_ns = start.elapsed().as_nanos() as u64;
            // try_with (not with): the lifted-to-wasm web backend has no real
            // TLS, so `with` hits panic_access_error. These probe accesses are
            // inlined into layout_dom_recursive/layout_document, so they can't
            // be stubbed at the symbol level — use the non-panicking access.
            let depth = self.depth;
            let _ = DEPTH.try_with(|d| d.set(d.get().saturating_sub(1)));
            let _ = EVENTS.try_with(|cell| {
                cell.borrow_mut().push(super::Event {
                    name: self.name,
                    kind: super::EventKind::Span { dur_ns },
                    depth,
                });
            });
        }
    }

    pub(super) fn open(name: &'static str) -> Span {
        let _ = SPAN_NAMES.try_with(|st| st.borrow_mut().push(name));
        if !recording() {
            return Span {
                name,
                start: None,
                depth: 0,
            };
        }
        let depth = DEPTH
            .try_with(|d| {
                let cur = d.get();
                d.set(cur.saturating_add(1));
                cur
            })
            .unwrap_or(0);
        Span {
            name,
            start: Some(Instant::now()),
            depth,
        }
    }

    pub(super) fn sample_rss(label: &'static str, bytes: u64) {
        if !recording() {
            return;
        }
        // try_with: see Span::drop — no real TLS in the lifted wasm backend.
        let depth = DEPTH.try_with(std::cell::Cell::get).unwrap_or(0);
        let _ = EVENTS.try_with(|cell| {
            cell.borrow_mut().push(super::Event {
                name: label,
                kind: super::EventKind::Rss { bytes },
                depth,
            });
        });
    }

    /// The path of currently-open spans on THIS thread, outermost first,
    /// joined with `" > "` — e.g. `dispatch.timer > layout > text_shape`.
    /// Empty when no span is open. Readable from a panic hook (same thread).
    pub(super) fn span_path() -> String {
        SPAN_NAMES
            .try_with(|st| st.borrow().join(" > "))
            .unwrap_or_default()
    }

    pub(super) fn drain() -> Vec<super::Event> {
        EVENTS
            .try_with(|cell| core::mem::take(&mut *cell.borrow_mut()))
            .unwrap_or_default()
    }

    /// `dladdr`-backed pointer→symbol resolution with a leak-once cache.
    /// Span names are `&'static str`, so each distinct callback leaks ONE
    /// small string for the process lifetime — bounded by the number of
    /// distinct callbacks an app has.
    /// Pointer→name cache shared with the BACKGROUND addr2line upgrade
    /// thread (`spawn_addr2line_upgrade` replaces an offset placeholder with
    /// the real symbol once the subprocess returns).
    fn fn_name_cache() -> &'static std::sync::Mutex<std::collections::HashMap<usize, &'static str>>
    {
        use std::collections::HashMap;
        use std::sync::{Mutex, OnceLock};
        static CACHE: OnceLock<Mutex<HashMap<usize, &'static str>>> = OnceLock::new();
        CACHE.get_or_init(|| Mutex::new(HashMap::new()))
    }

    /// Is this pointer resolved? Lets tests observe that a gated call did
    /// NOT resolve, robust against concurrent tests resolving THEIR pointers.
    #[cfg(test)]
    pub(super) fn fn_name_cache_contains(fn_ptr: usize) -> bool {
        fn_name_cache()
            .lock()
            .map(|m| m.contains_key(&fn_ptr))
            .unwrap_or(false)
    }

    pub(super) fn resolve_fn_name(fn_ptr: usize) -> &'static str {
        let cache = fn_name_cache();
        if let Ok(map) = cache.lock() {
            if let Some(name) = map.get(&fn_ptr) {
                return name;
            }
        }
        let resolved: &'static str = {
            #[cfg(unix)]
            {
                // dladdr resolves names from .dynsym only — a statically
                // linked, non--rdynamic binary yields NO symbol for its own
                // functions. Fallback ladder: (1) `addr2line` against the
                // module's DEBUG symbols (Linux, when the tool is installed —
                // this recovers the real name, e.g. `cb:demo_button_click`,
                // even on static binaries); (2) the MODULE-RELATIVE offset
                // (`cb:+0x<offset>`), stable across runs of the same binary
                // (ASLR shifts the base, not the offset), so distinct
                // callbacks stay distinguishable and comparable per version.
                let mut info: libc::Dl_info = unsafe { core::mem::zeroed() };
                let rc = unsafe { libc::dladdr(fn_ptr as *const libc::c_void, &raw mut info) };
                if rc != 0 && !info.dli_sname.is_null() {
                    let name = unsafe { core::ffi::CStr::from_ptr(info.dli_sname) };
                    match name.to_str() {
                        Ok(sym) if !sym.is_empty() => {
                            Box::leak(format!("cb:{sym}").into_boxed_str())
                        }
                        _ => Box::leak(format!("cb:0x{fn_ptr:x}").into_boxed_str()),
                    }
                } else if rc != 0 && !info.dli_fbase.is_null() {
                    let offset = fn_ptr.wrapping_sub(info.dli_fbase as usize);
                    let module = if info.dli_fname.is_null() {
                        None
                    } else {
                        unsafe { core::ffi::CStr::from_ptr(info.dli_fname) }
                            .to_str()
                            .ok()
                            .map(str::to_owned)
                    };
                    // The name lives in DEBUG symbols only - recovering it
                    // means shelling out to addr2line, which loads the
                    // module's DWARF: SECONDS on a debuginfo build of
                    // libazul.so, and this used to run synchronously on the
                    // MAIN THREAD inside the first span of every distinct
                    // callback (the azpaint first-stroke stall: each newly
                    // fired callback froze the event loop for seconds,
                    // 2026-08-29). Return the stable offset name NOW and let
                    // a detached thread upgrade the cache entry in place -
                    // early histogram samples file under `cb:+0x<offset>`,
                    // later ones under the real name, and nothing ever
                    // blocks on symbolization.
                    spawn_addr2line_upgrade(fn_ptr, module, offset);
                    Box::leak(format!("cb:+0x{offset:x}").into_boxed_str())
                } else {
                    // dladdr failed outright: the raw address still separates
                    // one callback from another within this run.
                    Box::leak(format!("cb:0x{fn_ptr:x}").into_boxed_str())
                }
            }
            #[cfg(not(unix))]
            {
                Box::leak(format!("cb:0x{fn_ptr:x}").into_boxed_str())
            }
        };
        if let Ok(mut map) = cache.lock() {
            map.insert(fn_ptr, resolved);
        }
        resolved
    }

    /// Replace `fn_ptr`'s cached offset placeholder with the real symbol
    /// name, resolved by `addr2line` on a DETACHED thread. At most one
    /// spawn per pointer (the caller inserts the placeholder into the cache
    /// in the same miss, so a second call is a cache hit and never gets
    /// here). Failure or a missing tool simply leaves the offset name.
    #[cfg(unix)]
    fn spawn_addr2line_upgrade(fn_ptr: usize, module: Option<String>, offset: usize) {
        if !cfg!(target_os = "linux") {
            return;
        }
        std::thread::Builder::new()
            .name("azul-addr2line".into())
            .spawn(move || {
                if let Some(sym) = addr2line_name(module.as_deref(), offset, fn_ptr) {
                    let pretty: &'static str =
                        Box::leak(format!("cb:{sym}").into_boxed_str());
                    if let Ok(mut map) = fn_name_cache().lock() {
                        map.insert(fn_ptr, pretty);
                    }
                }
            })
            .ok(); // spawn failure: the offset name stands
    }

    /// DEBUG-SYMBOL fallback: asks the system's `addr2line` for the function
    /// name at `offset` inside `module` (Linux; other unixes rarely ship
    /// it). Recovers real names on statically linked binaries whose own
    /// functions are absent from `.dynsym` — exactly the case `dladdr`
    /// cannot answer.
    ///
    /// Called from the detached `azul-addr2line` upgrade thread ONLY (never
    /// on a caller's thread): loading a big binary's DWARF costs anywhere
    /// from hundreds of ms to SECONDS, which is jank if anything waits on
    /// it. Runs at most once per distinct callback pointer.
    /// `AZ_PROBE_ADDR2LINE=0` disables it.
    ///
    /// PIE executables and shared objects map file offsets 1:1 to link-time
    /// addresses, so the module-relative offset is the right query; for a
    /// non-PIE main binary (fixed 0x400000 base) the RAW pointer is, so a
    /// failed first query retries with it.
    /// Probed ONCE per process (first resolution), then a flag test: systems
    /// without addr2line never spawn a second lookup attempt, and nothing
    /// here can fail loudly — "not available" just means the offset form.
    #[cfg(unix)]
    fn addr2line_available() -> bool {
        use std::sync::OnceLock;
        static AVAILABLE: OnceLock<bool> = OnceLock::new();
        *AVAILABLE.get_or_init(|| {
            std::process::Command::new("addr2line")
                .arg("--version")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        })
    }

    #[cfg(unix)]
    fn addr2line_name(module: Option<&str>, offset: usize, raw_ptr: usize) -> Option<String> {
        if !cfg!(target_os = "linux") {
            return None;
        }
        if std::env::var("AZ_PROBE_ADDR2LINE").is_ok_and(|v| v == "0") {
            return None;
        }
        if !addr2line_available() {
            return None;
        }
        let module: std::borrow::Cow<'_, str> = match module {
            Some(m) if !m.is_empty() => m.into(),
            _ => std::env::current_exe()
                .ok()?
                .to_string_lossy()
                .into_owned()
                .into(),
        };
        let ask = |addr: usize| -> Option<String> {
            let out = std::process::Command::new("addr2line")
                .arg("-f") // function names…
                .arg("-C") // …demangled
                .arg("-i") // …with the full INLINE stack: a tiny callback's
                //            first instruction often belongs to an inlined
                //            callee (black_box, a getter), and the innermost
                //            frame would name THAT. The callback is the
                //            OUTERMOST frame — the last name in the output.
                .arg("-e")
                .arg(module.as_ref())
                .arg(format!("0x{addr:x}"))
                .output()
                .ok()?;
            if !out.status.success() {
                return None;
            }
            let text = String::from_utf8_lossy(&out.stdout);
            // Lines alternate name/location, innermost first; keep the last
            // usable NAME line (the outermost frame).
            let name = text
                .lines()
                .step_by(2)
                .map(str::trim)
                .filter(|n| !n.is_empty() && *n != "??")
                .last()?;
            Some(name.to_owned())
        };
        ask(offset).or_else(|| ask(raw_ptr))
    }

    /// [`super::Probe::span_for_fn`]: the recording gate runs BEFORE name
    /// resolution. With recording off this is one atomic load and an inert
    /// guard - `resolve_fn_name` (dladdr; on a miss, a background addr2line
    /// spawn) does not run at all. The old order resolved unconditionally,
    /// which paid symbolization for every distinct callback even with the
    /// probe entirely off.
    pub(super) fn open_for_fn(fn_ptr: usize) -> Span {
        open_for_fn_gated(recording(), fn_ptr)
    }

    /// The gate itself, with the recording state as a PARAMETER so the test
    /// can pin it without touching the process-global flag (toggling that
    /// mid-suite kills other tests' spans - the flag is shared).
    pub(super) fn open_for_fn_gated(is_recording: bool, fn_ptr: usize) -> Span {
        if is_recording {
            open(resolve_fn_name(fn_ptr))
        } else {
            open("cb:off")
        }
    }

    pub(super) fn drop_events() {
        let _ = EVENTS.try_with(|cell| cell.borrow_mut().clear());
    }

    pub(super) fn peek_len() -> usize {
        EVENTS.try_with(|cell| cell.borrow().len()).unwrap_or(0)
    }

    pub(super) const fn enabled() -> bool {
        true
    }
}

#[cfg(any(not(feature = "probe"), target_family = "wasm", feature = "web_lift"))]
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
    pub(super) const fn span_path() -> String {
        String::new()
    }

    #[inline]
    pub(super) const fn resolve_fn_name(_fn_ptr: usize) -> &'static str {
        "cb:?"
    }

    #[inline]
    pub(super) const fn open_for_fn(_fn_ptr: usize) -> Span {
        Span
    }

    #[inline]
    pub(super) const fn set_recording(_on: bool) {}

    #[inline]
    pub(super) const fn sample_rss(_label: &'static str, _bytes: u64) {}

    #[inline]
    pub(super) const fn drain() -> Vec<super::Event> {
        Vec::new()
    }

    #[inline]
    pub(super) const fn drop_events() {}

    #[inline]
    pub(super) const fn peek_len() -> usize {
        0
    }

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
    /// Nesting depth at the time the span OPENED (0 = outermost).
    ///
    /// Spans are emitted post-order carrying only a duration, so a
    /// consumer could report a phase's CUMULATIVE time but never its own:
    /// an outer `layout_formatting_context` reports the whole subtree it
    /// contains, and the totals happily exceed wall-clock. With depth, a
    /// consumer walking the post-order stream can subtract each span's
    /// immediate children and get SELF time — which is what actually
    /// names a hot phase. `Rss` samples carry the current depth too.
    pub depth: u16,
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
    /// on drop into the thread-local event buffer — but ONLY while
    /// recording is on (any `AZ_PROFILE` mode, or [`Probe::set_recording`]).
    /// With recording off the guard is inert and the call is one relaxed
    /// atomic load: safe to leave in hot per-node paths.
    #[inline]
    // const only in the no-`probe` stub config; enabled `imp::` calls are non-const
    #[allow(clippy::missing_const_for_fn)]
    #[must_use]
    pub fn span(name: &'static str) -> Span {
        imp::open(name)
    }

    /// Force event recording on/off, overriding the lazy `AZ_PROFILE`
    /// resolution. Tests use this (they assert on drained events without
    /// setting env vars); a debug server could too. Flipping mid-span only
    /// perturbs the saturating depth counter, never memory safety.
    #[inline]
    // const only in the no-`probe` stub config; enabled `imp::` calls are non-const
    #[allow(clippy::missing_const_for_fn)]
    pub fn set_recording(on: bool) {
        imp::set_recording(on);
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
    #[must_use]
    pub fn drain() -> Vec<Event> {
        imp::drain()
    }

    /// The names of THIS thread's currently-open spans, outermost first,
    /// joined with `" > "` (empty when none). Maintained even with recording
    /// off — a crash report reads this as "what scope was the app in", and
    /// that diagnostic must not depend on `AZ_PROFILE`.
    #[inline]
    // const only in the no-`probe` stub config; enabled `imp::` calls are non-const
    #[allow(clippy::missing_const_for_fn)]
    #[must_use]
    pub fn span_path() -> String {
        imp::span_path()
    }

    /// A timed span NAMED AFTER the function the pointer points at,
    /// resolved through the dynamic linker (`dladdr`) and cached forever:
    /// an `extern "C"` app callback like `my_button_click` becomes span
    /// `cb:my_button_click`, so the per-phase histogram answers
    /// "`my_button_click` takes 0.2 ms on 1.5.0, took 0.1 ms on 1.4.3".
    ///
    /// With recording OFF this is one relaxed atomic load and an inert
    /// guard - NO resolution runs (it used to run unconditionally, which
    /// shelled out to `addr2line` on the caller's thread the first time
    /// each distinct callback fired: seconds per callback on a debuginfo
    /// build, with the probe entirely off - the azpaint first-stroke
    /// stall, 2026-08-29).
    ///
    /// While recording, resolution runs ONCE per distinct pointer (a
    /// leak-once cache bounded by the number of distinct callbacks); every
    /// later call is one map lookup. When the symbol is unresolvable
    /// (static non-`-rdynamic` binaries keep their own functions out of
    /// `.dynsym`), the span opens under the stable module-relative offset
    /// `cb:+0x<offset>` immediately while a DETACHED thread asks
    /// `addr2line` (Linux, when installed; `AZ_PROBE_ADDR2LINE=0`
    /// disables) and upgrades the cache to the real name for every span
    /// after it; `cb:0x<addr>` is the last resort when `dladdr` fails
    /// entirely.
    #[inline]
    // const only in the no-`probe` stub config; enabled `imp::` calls are non-const
    #[allow(clippy::missing_const_for_fn)]
    #[must_use]
    pub fn span_for_fn(fn_ptr: usize) -> Span {
        imp::open_for_fn(fn_ptr)
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
    #[must_use]
    pub fn peek_len() -> usize {
        imp::peek_len()
    }

    /// Whether the `probe` feature is compiled in.
    #[inline]
    // const only in the no-`probe` stub config; enabled `imp::` calls are non-const
    #[allow(clippy::missing_const_for_fn)]
    #[must_use]
    pub fn enabled() -> bool {
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
            eprintln!("[CPU] {label}: probe unavailable on this target (timings = ???)");
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
        // Self-measurement accounting: each sample reads /proc (or the mach
        // equivalent) — hundreds of µs each, ×10 checkpoints per pass. This
        // span makes the PROFILER'S OWN COST a line in its report instead of
        // silently inflating solver3_layout_document's self-time (~5 ms of
        // "unattributed" turned out to be largely this).
        let _p = Probe::span("probe_rss_sample_cost");
        let (current, _virt) = current_rss_bytes();
        let bytes = if current != 0 {
            current
        } else {
            peak_rss_bytes_self()
        };
        Probe::sample_rss(label, bytes);
    }
    #[cfg(any(not(feature = "probe"), feature = "web_lift"))]
    let _ = label;
}

#[cfg(feature = "probe")]
#[must_use]
pub fn peak_rss_bytes_pub() -> u64 {
    peak_rss_bytes_self()
}

#[cfg(feature = "probe")]
fn peak_rss_bytes_self() -> u64 {
    #[cfg(unix)]
    unsafe {
        let mut ru: libc::rusage = core::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &raw mut ru) != 0 {
            return 0;
        }
        let raw = ru.ru_maxrss as u64;
        if cfg!(target_os = "macos") {
            raw
        } else {
            raw.saturating_mul(1024)
        }
    }
    // Windows has no getrusage; `PeakWorkingSetSize` is the direct equivalent
    // of `ru_maxrss` and is already in bytes.
    #[cfg(all(target_os = "windows", not(miri)))]
    {
        windows_memory_counters().map_or(0, |c| c.peak_working_set)
    }
    #[cfg(not(any(unix, all(target_os = "windows", not(miri)))))]
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
            eprintln!(
                "[PURGE] mi_collect(true) called — current rss={:.2} MiB",
                rss as f64 / 1048576.0
            );
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
    #[cfg(all(
        target_os = "macos",
        not(miri),
        not(any(feature = "allocator_mimalloc", feature = "allocator_jemalloc"))
    ))]
    {
        extern "C" {
            fn malloc_zone_pressure_relief(zone: *mut core::ffi::c_void, goal: usize) -> usize;
        }
        unsafe {
            malloc_zone_pressure_relief(core::ptr::null_mut(), 0);
        }
    }
    // glibc's equivalent of malloc_zone_pressure_relief. Without this the
    // Linux default-allocator path was the "no-op" the doc comment describes,
    // so a purge-then-measure sequence could never show pages coming back.
    #[cfg(all(
        target_os = "linux",
        target_env = "gnu",
        not(miri),
        not(any(feature = "allocator_mimalloc", feature = "allocator_jemalloc"))
    ))]
    {
        // Declared here rather than via `libc::malloc_trim`: this function is
        // NOT gated on the `probe` feature (that is what pulls in libc), and
        // the macOS arm above declares `malloc_zone_pressure_relief` the same
        // way for the same reason.
        extern "C" {
            fn malloc_trim(pad: usize) -> core::ffi::c_int;
        }
        unsafe {
            malloc_trim(0);
        }
    }
}

/// Sample the process's "real" memory footprint (not peak).
/// Returns (`footprint_bytes`, `virtual_bytes`). On macOS this is
/// `phys_footprint` from `TASK_VM_INFO` — matches Activity Monitor
/// "Memory" and `vmmap`'s "Physical footprint" line, and excludes
/// shared library text pages that would otherwise inflate RSS
/// without costing the process anything uniquely. On Linux this
/// falls back to `/proc/self/statm` resident size (no direct
/// equivalent; the shared-lib inflation is much smaller there).
/// More useful than `getrusage.ru_maxrss` which only moves upward.
#[cfg(feature = "probe")]
#[must_use]
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
                target: u32,
                flavor: u32,
                info: *mut core::ffi::c_void,
                count: *mut u32,
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
    // The doc comment above has always promised a `/proc/self/statm` fallback
    // on Linux. Until 2026-07-29 this arm returned (0, 0) for every non-macOS
    // target, so `sample_peak_rss` silently fell back to ru_maxrss (peak-only,
    // never decreases) and every allocator-purge measurement on Linux read as
    // "no memory was returned".
    #[cfg(all(target_os = "linux", not(miri)))]
    {
        // statm fields are in pages: size resident shared text lib data dt.
        let Ok(statm) = std::fs::read_to_string("/proc/self/statm") else {
            return (0, 0);
        };
        let mut it = statm.split_ascii_whitespace();
        let size: u64 = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        let resident: u64 = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        let page = if page > 0 { page as u64 } else { 4096 };
        (resident.saturating_mul(page), size.saturating_mul(page))
    }
    // Windows: `K32GetProcessMemoryInfo` is the documented equivalent.
    // WorkingSetSize is the RSS analogue (what Task Manager calls "Memory
    // (active private working set)"'s superset), PrivateUsage is the commit
    // charge — the closest thing to the "virtual" slot the other arms fill.
    //
    // Until this arm existed every Windows build reported (0, 0), which made
    // "startup RSS after an update" — the one metric the telemetry rollout
    // gate is built on — silently meaningless on the majority desktop
    // platform (`core/src/profile.rs` documented this as a known hole).
    #[cfg(all(target_os = "windows", not(miri)))]
    {
        windows_memory_counters().map_or((0, 0), |c| (c.working_set, c.private_usage))
    }
    #[cfg(not(any(
        target_os = "macos",
        all(target_os = "linux", not(miri)),
        all(target_os = "windows", not(miri))
    )))]
    {
        (0, 0)
    }
}

/// Snapshot of `PROCESS_MEMORY_COUNTERS_EX`, in bytes.
#[cfg(all(feature = "probe", target_os = "windows", not(miri)))]
pub(crate) struct WindowsMemoryCounters {
    /// `WorkingSetSize` — resident bytes; the RSS analogue.
    pub working_set: u64,
    /// `PeakWorkingSetSize` — high-water mark of the above.
    pub peak_working_set: u64,
    /// `PrivateUsage` — commit charge (private bytes).
    pub private_usage: u64,
}

/// Reads `PROCESS_MEMORY_COUNTERS_EX` for the current process.
///
/// `K32GetProcessMemoryInfo` lives directly in `kernel32.dll` (Windows 7+),
/// so this needs no `psapi.dll` import library and no `windows-sys`
/// dependency — matching how the macOS arm above hand-declares `task_info`.
///
/// Returns `None` if the call fails.
#[cfg(all(feature = "probe", target_os = "windows", not(miri)))]
pub(crate) fn windows_memory_counters() -> Option<WindowsMemoryCounters> {
    // Layout per the Win32 header. On 64-bit the two leading DWORDs pack into
    // the first 8 bytes with no tail padding before the first SIZE_T, so the
    // struct maps 1:1; `cb` is validated by the callee against what we pass.
    #[repr(C)]
    struct ProcessMemoryCountersEx {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
        private_usage: usize,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> *mut core::ffi::c_void;
        fn K32GetProcessMemoryInfo(
            process: *mut core::ffi::c_void,
            counters: *mut ProcessMemoryCountersEx,
            cb: u32,
        ) -> i32;
    }

    unsafe {
        let mut counters: ProcessMemoryCountersEx = core::mem::zeroed();
        let cb = u32::try_from(core::mem::size_of::<ProcessMemoryCountersEx>()).ok()?;
        counters.cb = cb;
        if K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, cb) == 0 {
            return None;
        }
        Some(WindowsMemoryCounters {
            working_set: counters.working_set_size as u64,
            peak_working_set: counters.peak_working_set_size as u64,
            private_usage: counters.private_usage as u64,
        })
    }
}

/// Heap bytes currently held by the libc allocator (`mstats.bytes_used`).
///
/// Unlike RSS, this is what *Rust* allocations plus anything else going
/// through the default malloc zone is actually holding — mmap regions
/// for thread stacks, GL buffers, file-mapped fonts, etc. are NOT counted.
/// A leak that shows up here points to a genuine heap retention (an Arc
/// chain never dropped, a Vec never shrunk, a `Box<T>` forgotten).
///
/// - **macOS**: `mstats().bytes_used`.
/// - **Linux/glibc**: `mallinfo2().uordblks` — the same quantity, total
///   bytes currently handed out by malloc. Resolved with `dlsym` rather
///   than linked directly, because `mallinfo2` is glibc 2.33+ and a hard
///   link reference would break the build on older distros for the sake of
///   an opt-in diagnostic. Falls back to the `c_int`-based `mallinfo()`,
///   which is exact below 2 GiB of live heap.
/// - Everything else: 0.
///
/// CAVEAT (Linux): glibc accounts the **main arena only**. Allocations made
/// on other threads' arenas — and azul spawns font scout/builder threads —
/// are invisible here. A rising number is proof of a leak; a flat one is
/// not proof of its absence. Cross-check with [`current_rss_bytes`].
///
/// This returned 0 on every non-macOS target until 2026-07-29, which is the
/// only reason `dll/tests/leak_regression.rs` is `cfg(target_os = "macos")`:
/// the leak was never macOS-specific, the *instrument* was.
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
    #[cfg(all(target_os = "linux", target_env = "gnu", not(miri)))]
    {
        type Mallinfo2Fn = unsafe extern "C" fn() -> libc::mallinfo2;
        static MALLINFO2: std::sync::OnceLock<Option<Mallinfo2Fn>> = std::sync::OnceLock::new();
        let resolved = MALLINFO2.get_or_init(|| unsafe {
            // RTLD_DEFAULT is NULL on glibc; the libc crate doesn't define
            // the constant for linux-gnu, so spell it out.
            let sym = libc::dlsym(core::ptr::null_mut(), c"mallinfo2".as_ptr());
            if sym.is_null() {
                None
            } else {
                Some(core::mem::transmute::<*mut core::ffi::c_void, Mallinfo2Fn>(
                    sym,
                ))
            }
        });
        match resolved {
            Some(mallinfo2) => unsafe { mallinfo2().uordblks as u64 },
            // Pre-2.33 glibc. `uordblks` is a signed int that wraps past
            // 2 GiB; clamp rather than report a negative byte count.
            None => unsafe { libc::mallinfo().uordblks.max(0) as u64 },
        }
    }
    #[cfg(not(any(
        target_os = "macos",
        all(target_os = "linux", target_env = "gnu", not(miri))
    )))]
    {
        0
    }
}

/// Sample the Mach `phys_footprint` — the memory metric Activity
/// Monitor and `vmmap`'s "Physical footprint" line display. Unlike
/// `resident_size`, this excludes shared library text pages and
/// other kernel-mapped regions that inflate the traditional RSS
/// number without actually costing the process anything. For a
/// short-lived headless render this is a much more honest figure:
/// on a ~20 MiB `ru_maxrss` run, `phys_footprint` is typically ~8 MiB.
/// Returns 0 on non-macOS or if the Mach call fails.
///
/// There's no direct "peak `phys_footprint`" field; track the max
/// across calls in application code if you need it.
#[cfg(feature = "probe")]
// NOT const: the macOS branch calls mach task_info — const only held on
// targets where that branch compiles out (E0015 on aarch64-apple-darwin).
#[allow(clippy::missing_const_for_fn)]
#[must_use]
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
                target: u32,
                flavor: u32,
                info: *mut core::ffi::c_void,
                count: *mut u32,
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
            if kr == 0 {
                info.phys_footprint
            } else {
                0
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        0
    }
}

/// Background sampler for peak `phys_footprint`. Spawns a thread that
/// polls `phys_footprint_bytes()` every ~2 ms and updates a shared
/// atomic. The kernel does not expose a direct "peak `phys_footprint`"
/// — unlike `resident_size_peak` in `TASK_VM_INFO` — so polling is
/// the only way to catch mid-phase transients that are `MADV_FREE`'d
/// before the next explicit sample point.
///
/// Not started by default; call `start_peak_sampler()` once at
/// process init if you want peak tracking. Overhead is negligible
/// (~1-5 µs per poll on macOS, 500 Hz → <0.25% CPU of one core).
/// `peak_phys_footprint_seen()` reads the current high-water mark.
#[cfg(feature = "probe")]
// NOT const: the macOS branch spawns the sampler thread (E0015 there).
#[allow(clippy::missing_const_for_fn)]
pub fn start_peak_sampler() {
    #[cfg(target_os = "macos")]
    {
        use std::sync::atomic::Ordering;
        // Idempotent — only spawns once.
        static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
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
static PEAK_PHYS_FOOTPRINT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Read the peak `phys_footprint` seen by the background sampler.
/// Returns 0 if `start_peak_sampler` was never called.
#[cfg(feature = "probe")]
pub fn peak_phys_footprint_seen() -> u64 {
    PEAK_PHYS_FOOTPRINT.load(std::sync::atomic::Ordering::Relaxed)
}

/// Reset the global peak high-water mark to the current `phys_footprint`.
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
#[must_use]
pub const fn malloc_heap_bytes() -> u64 {
    0
}

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
    if !heap_jsonl_enabled() {
        return;
    }
    let Some(p) = azul_core::profile::out_path() else {
        return;
    };
    static CALL_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    // Auto-increment on every "start" label; "end" and intermediates reuse
    // the current id so all phases in one regenerate_layout invocation share
    // a call number.
    static CURRENT_CALL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
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
        drop(writeln!(
            f,
            r#"{{"ev":"phase","call":{call_id},"label":"{label}","heap":{heap}}}"#
        ));
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
    if !heap_jsonl_enabled() {
        return;
    }
    if !azul_core::profile::detail_enabled() {
        return;
    }
    let Some(p) = azul_core::profile::out_path() else {
        return;
    };
    let heap = malloc_heap_bytes();
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(p)
    {
        drop(writeln!(
            f,
            r#"{{"ev":"phase","call":0,"label":"{label}","heap":{heap},"extra":{extra}}}"#
        ));
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
#[must_use]
pub fn detail_enabled() -> bool {
    azul_core::profile::detail_enabled()
}

#[cfg(not(feature = "probe"))]
#[inline]
#[must_use]
pub const fn detail_enabled() -> bool {
    false
}

#[cfg(test)]
#[allow(let_underscore_drop, clippy::too_many_lines)]
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
    /// the same thread-local `EVENTS`). Also force recording ON: these tests
    /// assert on drained events, and without `AZ_PROFILE` in the environment
    /// the lazy gate would otherwise leave every span inert.
    fn reset() {
        Probe::set_recording(true);
        Probe::drop_events();
        assert_eq!(
            Probe::peek_len(),
            0,
            "drop_events must leave an empty buffer"
        );
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

    /// THE FIRST-STROKE STALL (azpaint, 2026-08-29): `span_for_fn` resolved
    /// the callback name BEFORE the recording gate, so the first span of
    /// every distinct callback shelled out to addr2line on the caller's
    /// thread - seconds per callback on a debuginfo build, with the probe
    /// entirely off. The gate now runs first: recording off = no resolution.
    #[test]
    #[cfg(all(
        feature = "probe",
        not(target_family = "wasm"),
        not(feature = "web_lift")
    ))]
    fn span_for_fn_resolves_only_while_recording() {
        extern "C" fn gated_probe_fixture_a() {}
        extern "C" fn gated_probe_fixture_b() {}
        let a = gated_probe_fixture_a as usize;
        let b = gated_probe_fixture_b as usize;

        // The gate is tested through its parameterized form: flipping the
        // PROCESS-GLOBAL recording flag here would race the other probe
        // tests (they record concurrently and count their events).
        drop(imp::open_for_fn_gated(false, a));
        assert!(
            !imp::fn_name_cache_contains(a),
            "recording OFF must not resolve the callback name (that is where \
             the synchronous addr2line jank lived)",
        );

        drop(imp::open_for_fn_gated(true, b));
        assert!(
            imp::fn_name_cache_contains(b),
            "recording ON must resolve (and cache) the callback name",
        );
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
            assert!(
                span_ns(&events[0]).is_some(),
                "span guard must emit EventKind::Span"
            );
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
            "{}{:?}{0}%s%n",             // format-string-looking payloads
            "🦀👨‍👩‍👧‍👦🇩🇪",                    // emoji + ZWJ sequence + flag
            "مرحبا بالعالم",             // RTL
            "e\u{0301}\u{0301}\u{0301}", // stacked combining marks
            leak("A".repeat(100_000)),   // huge
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
            assert_eq!(
                Probe::peek_len(),
                0,
                "buffers must not be shared across threads"
            );
            Probe::sample_rss("child_thread", 2);
            let drained = Probe::drain();
            for ev in &drained {
                assert_eq!(
                    ev.name, "child_thread",
                    "child must only see its own events"
                );
            }
            drained.len()
        })
        .join()
        .expect("probe calls must not panic on a spawned thread");

        let events = Probe::drain();
        if Probe::enabled() {
            assert_eq!(child_len, 1);
            assert_eq!(
                events.len(),
                1,
                "the child's drain must not touch our buffer"
            );
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
            Event {
                name: "a",
                kind: EventKind::Rss { bytes: 0 },
                depth: 0,
            },
            Event {
                name: "b",
                kind: EventKind::Rss { bytes: u64::MAX },
                depth: 0,
            },
            Event {
                name: "c",
                kind: EventKind::Rss { bytes: 1 },
                depth: 0,
            },
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
                    depth: 0,
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
            Event {
                name: "huge",
                kind: EventKind::Span { dur_ns: u64::MAX },
                depth: 0,
            },
            Event {
                name: "huge",
                kind: EventKind::Span { dur_ns: u64::MAX },
                depth: 0,
            },
            Event {
                name: "huge",
                kind: EventKind::Span { dur_ns: u64::MAX },
                depth: 0,
            },
            Event {
                name: "zero",
                kind: EventKind::Span { dur_ns: 0 },
                depth: 0,
            },
        ];
        print_drained_events("overflowing-total", &events);
    }

    #[test]
    fn print_drained_events_rss_delta_handles_full_u64_swing() {
        // The delta is computed in i128; a MAX -> 0 -> MAX swing is the worst
        // case for a naive i64/u64 subtraction.
        let events = [
            Event {
                name: "peak",
                kind: EventKind::Rss { bytes: u64::MAX },
                depth: 0,
            },
            Event {
                name: "trough",
                kind: EventKind::Rss { bytes: 0 },
                depth: 0,
            },
            Event {
                name: "peak_again",
                kind: EventKind::Rss { bytes: u64::MAX },
                depth: 0,
            },
        ];
        print_drained_events("delta-swing", &events);
    }

    #[test]
    fn print_drained_events_hostile_labels_and_names() {
        let big = leak("x".repeat(65_536));
        let events = [
            Event {
                name: "",
                kind: EventKind::Span { dur_ns: 1 },
                depth: 0,
            },
            Event {
                name: "{}{:?}",
                kind: EventKind::Span { dur_ns: 2 },
                depth: 0,
            },
            Event {
                name: big,
                kind: EventKind::Span { dur_ns: u64::MAX },
                depth: 0,
            },
            Event {
                name: "🦀\u{0301}\0",
                kind: EventKind::Rss { bytes: 1 },
                depth: 0,
            },
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
        assert!(
            before <= mid && mid <= after,
            "{before} <= {mid} <= {after}"
        );
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
            // `sample_peak_rss` deliberately wraps its /proc (or mach) read in
            // a `probe_rss_sample_cost` span so the profiler's own cost shows
            // up as a line in its own report. So the drain holds TWO events,
            // and the assertion has to be about the labelled Rss sample, not
            // about the buffer length — the old `events.len() == 1` could only
            // ever pass on the `!Probe::enabled()` path, i.e. never with the
            // `probe` feature actually on, which is the only configuration
            // where this test tests anything.
            let rss: Vec<&Event> = events
                .iter()
                .filter(|ev| ev.name == "autotest_peak_rss")
                .collect();
            assert_eq!(rss.len(), 1, "drained: {events:?}");
            assert!(
                rss_bytes(rss[0]).is_some(),
                "sample_peak_rss must emit an Rss-kind event"
            );
            assert!(
                events.iter().any(|ev| ev.name == "probe_rss_sample_cost"),
                "the self-measurement span must still be recorded: {events:?}"
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

    /// Platforms where `malloc_heap_bytes` is expected to return a real
    /// figure. This used to be macOS alone, which is exactly why the FFI leak
    /// regression could only ever be measured there.
    const HEAP_BYTES_IS_REAL: bool = cfg!(all(
        feature = "probe",
        any(
            target_os = "macos",
            all(target_os = "linux", target_env = "gnu")
        ),
        not(miri)
    ));

    #[test]
    fn malloc_heap_bytes_actually_tracks_live_heap() {
        if !HEAP_BYTES_IS_REAL {
            // Unsupported target (or the `probe` feature is off, where the
            // stub is a `const fn -> 0`). Say so by measurement, not by faith.
            assert_eq!(malloc_heap_bytes(), 0);
            assert_eq!(malloc_heap_bytes(), 0);
            return;
        }

        // A probe that returns a plausible constant is worse than one that
        // returns nothing, because it reads as evidence. Prove it MOVES, and
        // moves in the right direction by roughly the right amount.
        //
        // 8 MiB: far above allocator bookkeeping noise, and above glibc's
        // MMAP_THRESHOLD only if that has been tuned up — so ask for it as
        // many smaller blocks that are certain to come from the heap proper
        // rather than a fresh mmap that `uordblks` would not count.
        const BLOCK: usize = 64 * 1024;
        const BLOCKS: usize = 128;
        const TOTAL: u64 = (BLOCK * BLOCKS) as u64;

        // BEST OF SEVERAL, not one reading. The counter is PROCESS-GLOBAL and
        // this runs inside a test binary whose several thousand other tests
        // allocate and free megabytes on other threads throughout — so a
        // single before/during/after triple is signal plus whatever they did
        // in the same microseconds, and which dominates is luck. It failed
        // both ways within one afternoon: once with `during` BELOW `before`
        // (another thread freed more than this allocated), once with `after`
        // above `during - TOTAL/2`.
        //
        // Retrying does not weaken the assertion. A quiet window arrives
        // quickly, and a probe that genuinely does not track the heap - the
        // thing this test exists to catch - fails EVERY attempt.
        const ATTEMPTS: usize = 32;
        let mut last = (0_u64, 0_u64, 0_u64);
        for _ in 0..ATTEMPTS {
            let before = malloc_heap_bytes();
            assert!(before > 0, "a live process holds a non-zero heap");

            let mut ballast: Vec<Vec<u8>> = Vec::with_capacity(BLOCKS);
            for _ in 0..BLOCKS {
                // Touch it: a Vec that is never written may not be committed.
                ballast.push(vec![0xAB_u8; BLOCK]);
            }
            let during = malloc_heap_bytes();

            drop(ballast);
            let after = malloc_heap_bytes();

            let grew = during >= before + TOTAL / 2;
            let shrank = after < during - TOTAL / 2;
            if grew && shrank {
                return;
            }
            last = (before, during, after);
        }

        let (before, during, after) = last;
        panic!(
            "in {ATTEMPTS} attempts the probe never tracked a {TOTAL} B allocate-and-free \
             (last: before={before}, during={during}, after={after}) — it is not measuring the \
             heap"
        );
    }

    #[test]
    fn detail_enabled_is_deterministic() {
        let first = detail_enabled();
        for _ in 0..100 {
            assert_eq!(
                detail_enabled(),
                first,
                "flag reads are cached, must not flap"
            );
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
        assert_eq!(
            Probe::peek_len(),
            0,
            "JSONL emission must not touch the span buffer"
        );
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
        let span = Event {
            name: "n",
            kind: EventKind::Span { dur_ns: u64::MAX },
            depth: 0,
        };
        let rss = Event {
            name: "n",
            kind: EventKind::Rss { bytes: u64::MAX },
            depth: 0,
        };
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
        assert!(
            pubbed >= first,
            "peak RSS must never decrease: {first} -> {pubbed}"
        );
        assert!(
            second >= pubbed,
            "peak RSS must never decrease: {pubbed} -> {second}"
        );
        if cfg!(unix) && !cfg!(miri) {
            assert!(
                first > 0,
                "getrusage on a live unix process must report some RSS"
            );
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
            assert_eq!(
                seen, 0,
                "no phys_footprint source off macOS => peak stays 0"
            );
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
            assert_eq!(
                heap_jsonl_enabled(),
                first,
                "flags are cached, must not flap"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// RSS CENSUS — the mapping-level breakdown, in-process.
//
// This reproduces in-process what reading /proc/<pid>/smaps by hand shows:
// where the process's resident memory actually is,
// as opposed to what the engine's own object walk can see. The two answer
// different questions and the gap between them IS the finding — the engine
// walks its caches and reaches ~33% of RSS; the rest is framebuffers, fonts,
// binary, libraries, allocator retention, and any cache the APPLICATION owns.
//
// Deliberately NOT behind the `probe` feature: the memory report should work
// on a stock build, and reading one file per report is not a cost worth
// gating.
// ---------------------------------------------------------------------------

/// Resident memory grouped the way the RSS map groups it. All figures are
/// **KiB**, matching what `smaps` reports (it prints "kB" but means KiB —
/// conflating that with decimal MB is a 4.9% error and has produced at least
/// three wrong conclusions in this project's own analysis).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RssCensus {
    pub heap_kib: u64,
    /// Anonymous mappings. Large allocations go here rather than `[heap]`
    /// because glibc serves anything above `MMAP_THRESHOLD` with `mmap` — a
    /// full-window pixmap lands here and never appears in `[heap]`.
    pub anon_kib: u64,
    pub binary_kib: u64,
    pub shared_libs_kib: u64,
    pub font_files_kib: u64,
    /// `memfd:azul-fb` — the Wayland shared-memory framebuffer pool.
    pub framebuffer_kib: u64,
    pub stacks_kib: u64,
    pub other_kib: u64,
    pub total_kib: u64,
    pub shared_lib_mappings: usize,
    pub font_mappings: usize,
}

impl RssCensus {
    /// Sum of the categories. Equals `total_kib` unless a category was missed,
    /// so a caller can assert the census is exhaustive rather than trusting it.
    #[must_use]
    pub const fn categorised_kib(&self) -> u64 {
        self.heap_kib
            + self.anon_kib
            + self.binary_kib
            + self.shared_libs_kib
            + self.font_files_kib
            + self.framebuffer_kib
            + self.stacks_kib
            + self.other_kib
    }
}

/// Read `/proc/self/smaps` and group resident pages by what backs them.
///
/// Returns `None` off Linux or if `smaps` is unreadable. Costs one file read
/// and a linear scan; `smaps` is a few hundred KB on a process this size.
// Off Linux every `#[cfg]` arm below collapses to `None`, so clippy sees a
// function that could be `const` — and says so as an error under the extreme
// lint set, making `cargo clippy` red on every Mac while CI's ubuntu job is
// green. The Linux body reads a file; it cannot be const. Allow it here rather
// than let the gate mean two different things on two machines.
#[allow(clippy::missing_const_for_fn)]
#[must_use]
pub fn rss_census() -> Option<RssCensus> {
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/self/smaps").ok()?;
        let mut c = RssCensus::default();
        // The mapping name is the 6th whitespace field of a header line; the
        // `Rss:` line that follows belongs to it.
        let mut name = String::new();
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("Rss:") {
                let kib: u64 = rest
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                c.total_kib += kib;
                if name.is_empty() {
                    c.anon_kib += kib;
                } else if name.contains("memfd:azul-fb") {
                    c.framebuffer_kib += kib;
                } else if name == "[heap]" {
                    c.heap_kib += kib;
                } else if name.starts_with("[stack") || name == "[vdso]" || name == "[vvar]" {
                    c.stacks_kib += kib;
                } else if std::path::Path::new(name.as_str())
                    .extension()
                    .is_some_and(|e| {
                        ["ttf", "ttc", "otf", "pfb"]
                            .iter()
                            .any(|w| e.eq_ignore_ascii_case(w))
                    })
                {
                    c.font_files_kib += kib;
                    c.font_mappings += 1;
                } else if name.contains(".so") {
                    c.shared_libs_kib += kib;
                    c.shared_lib_mappings += 1;
                } else if std::env::current_exe()
                    .ok()
                    .and_then(|p| p.to_str().map(|s| name == s))
                    .unwrap_or(false)
                {
                    c.binary_kib += kib;
                } else {
                    c.other_kib += kib;
                }
            } else if let Some(first) = line.split_whitespace().next() {
                // Header lines start with an address range `hex-hex`.
                if first.len() > 8 && first.contains('-') && !line.ends_with(':') {
                    name = line.split_whitespace().nth(5).unwrap_or("").to_string();
                }
            }
        }
        Some(c)
    }
}

#[cfg(test)]
mod rss_census_tests {
    use super::*;

    /// The census must account for every resident page it counted. A category
    /// sum that falls short of the total means a mapping shape we do not
    /// recognise is being silently dropped — which is how a memory report
    /// starts under-reporting without anyone noticing.
    #[test]
    #[cfg(target_os = "linux")]
    fn census_is_exhaustive_and_sees_this_process() {
        let Some(c) = rss_census() else {
            // smaps unreadable (containers, hardened kernels). Not a failure.
            return;
        };
        assert!(
            c.total_kib > 0,
            "a running test process has resident memory; reading smaps returned none"
        );
        assert_eq!(
            c.categorised_kib(),
            c.total_kib,
            "every counted page must land in exactly one category — \
             {} KiB of {} KiB did not",
            c.total_kib - c.categorised_kib(),
            c.total_kib
        );
        // A Rust test binary always has a heap and shared libraries; if either
        // is zero the header/Rss pairing has broken.
        assert!(
            c.heap_kib > 0 || c.anon_kib > 0,
            "no heap and no anon mappings — parse is wrong"
        );
    }

    /// Off Linux the census is honest about being unavailable rather than
    /// returning a zeroed struct that reads as "no memory used".
    #[test]
    #[cfg(not(target_os = "linux"))]
    fn census_is_none_off_linux() {
        assert!(rss_census().is_none());
    }
}

// ---------------------------------------------------------------------------
// ALLOCATOR STATS — live vs freed-but-held.
//
// The RSS census says where the process's pages are. It CANNOT say how much
// of `[heap]` is live data and how much is memory the program freed but the
// allocator kept. That distinction is load-bearing for this codebase: the CSS
// clone churn, the ~32 MiB a window resize costs, and the "transient 5.2 MB"
// in the RSS map are ALL freed-but-unreturned, and until now the report could
// not tell them from live data — which is exactly the confusion that made a
// 2 MB peak look like 5 MB of live footprint.
// ---------------------------------------------------------------------------

/// glibc's `struct mallinfo2`. All fields `size_t`, in bytes.
///
/// `mallinfo` (the old one) uses `int` and silently WRAPS past 2 GB, which is
/// why only `mallinfo2` is used here.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct MallInfo2 {
    arena: usize,
    ordblks: usize,
    smblks: usize,
    hblks: usize,
    hblkhd: usize,
    usmblks: usize,
    fsmblks: usize,
    uordblks: usize,
    fordblks: usize,
    keepcost: usize,
}

/// What the allocator is holding, in bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AllocatorStats {
    /// Live: handed to the program and not yet freed. THIS is the number to
    /// compare against an object walk.
    pub live_bytes: u64,
    /// Freed by the program, still held by the allocator. Counts toward RSS
    /// and toward `[heap]`, but is not data — it is what a churn-heavy
    /// startup leaves behind.
    pub free_in_arena_bytes: u64,
    /// `hblkhd` — space in mmapped regions. Served by `mmap` rather than the
    /// arena (glibc does this above `MMAP_THRESHOLD`), so it lands in
    /// `[anon]` and NOT in `[heap]` — which is why a full-window pixmap
    /// never appears in the heap figure.
    ///
    /// Note for anyone testing this: an allocation whose result is never read
    /// is DELETED by LLVM at `-O`, and then none of these counters move. A
    /// first attempt here concluded the field was broken on that basis. With
    /// the allocation forced to exist, a 64 MiB `Vec` moves `hblks` by +1 and
    /// this field by +67 112 960 (64 MiB + a 4 KiB header), and drops back on
    /// free. `arena` and `live_bytes` correctly stay flat, because an mmapped
    /// block is not an arena block.
    pub mmapped_bytes: u64,
    /// Total arena size. `live + free_in_arena` should approximate it.
    pub arena_bytes: u64,
    /// Trailing space that `malloc_trim` could return to the OS.
    pub releasable_bytes: u64,
}

impl AllocatorStats {
    /// Freed-but-held as a share of the arena. High means churn, not data.
    #[must_use]
    pub fn fragmentation_pct(&self) -> f64 {
        if self.arena_bytes == 0 {
            0.0
        } else {
            100.0 * self.free_in_arena_bytes as f64 / self.arena_bytes as f64
        }
    }
}

/// Query the allocator, or `None` if it cannot be asked.
///
/// Looked up with `dlsym` rather than declared `extern`, deliberately.
/// `mallinfo2` only exists in glibc >= 2.33; an `extern` declaration would
/// make the BINARY FAIL TO LINK on musl, on older glibc, and on macOS. A
/// runtime lookup degrades to `None` instead, and the report says the
/// allocator could not be queried rather than printing zeros — a zero here
/// would read as "no memory held", which is the worst possible wrong answer.
/// Ask glibc to return free heap pages to the OS. Returns `Some(true)` if it
/// released anything, `Some(false)` if it had nothing to release, `None` if
/// `malloc_trim` is unavailable (musl, macOS, older glibc).
///
/// WHY THIS EXISTS. A window resize was measured at ~+62 MB of transient
/// PEAK and only ~+2.6 MB of
/// RETAINED memory — the RSS that stays behind is glibc's arena holding pages
/// it no longer needs, not objects anyone owns. Nothing that reduces retained
/// bytes can move it; returning the pages is the only lever that acts on it
/// directly.
///
/// `dlsym` rather than an `extern` declaration, for the same reason as
/// `allocator_stats`: an `extern` block would make the BINARY FAIL TO LINK
/// wherever the symbol is absent, turning a missing optimisation into a
/// missing build.
// Off Linux every `#[cfg]` arm below collapses to `None`, so clippy sees a
// function that could be `const` — and says so as an error under the extreme
// lint set, making `cargo clippy` red on every Mac while CI's ubuntu job is
// green. The Linux body reads a file; it cannot be const. Allow it here rather
// than let the gate mean two different things on two machines.
#[allow(clippy::missing_const_for_fn)]
#[must_use]
pub fn malloc_trim() -> Option<bool> {
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    {
        None
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        unsafe extern "C" {
            fn dlsym(handle: *mut core::ffi::c_void, symbol: *const u8) -> *mut core::ffi::c_void;
        }
        let sym = unsafe { dlsym(core::ptr::null_mut(), c"malloc_trim".as_ptr().cast()) };
        if sym.is_null() {
            return None;
        }
        type MallocTrimFn = unsafe extern "C" fn(usize) -> i32;
        let f: MallocTrimFn = unsafe { core::mem::transmute(sym) };
        // pad = 0: keep nothing back. A non-zero pad would leave a cushion for
        // the next spike, which is a tuning question this measurement has no
        // basis to answer yet.
        Some(unsafe { f(0) } != 0)
    }
}

// Off Linux every `#[cfg]` arm below collapses to `None`, so clippy sees a
// function that could be `const` — and says so as an error under the extreme
// lint set, making `cargo clippy` red on every Mac while CI's ubuntu job is
// green. The Linux body reads a file; it cannot be const. Allow it here rather
// than let the gate mean two different things on two machines.
#[allow(clippy::missing_const_for_fn)]
#[must_use]
pub fn allocator_stats() -> Option<AllocatorStats> {
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    {
        None
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // RTLD_DEFAULT is NULL on glibc: search the global symbol scope.
        unsafe extern "C" {
            fn dlsym(handle: *mut core::ffi::c_void, symbol: *const u8) -> *mut core::ffi::c_void;
        }
        let sym = unsafe { dlsym(core::ptr::null_mut(), c"mallinfo2".as_ptr().cast()) };
        if sym.is_null() {
            return None;
        }
        type MallInfo2Fn = unsafe extern "C" fn() -> MallInfo2;
        let f: MallInfo2Fn = unsafe { core::mem::transmute(sym) };
        let mi = unsafe { f() };
        Some(AllocatorStats {
            live_bytes: mi.uordblks as u64,
            free_in_arena_bytes: mi.fordblks as u64,
            mmapped_bytes: mi.hblkhd as u64,
            arena_bytes: mi.arena as u64,
            releasable_bytes: mi.keepcost as u64,
        })
    }
}

#[cfg(test)]
mod allocator_stats_tests {
    use super::*;

    /// On a glibc host the allocator must answer, and its numbers must be
    /// self-consistent. A test process has always allocated something, so a
    /// zero `live_bytes` means the struct layout is wrong — which is the
    /// failure mode a hand-written `#[repr(C)]` mirror invites.
    #[test]
    #[cfg(all(unix, not(target_os = "macos")))]
    fn allocator_stats_are_self_consistent_or_absent() {
        let Some(a) = allocator_stats() else {
            // musl, or glibc < 2.33. Absence is a valid answer.
            return;
        };
        assert!(
            a.live_bytes > 0,
            "a running test process has live allocations; 0 means the \
             mallinfo2 struct layout is wrong"
        );
        assert!(
            a.arena_bytes >= a.free_in_arena_bytes,
            "free-in-arena ({}) cannot exceed the arena ({})",
            a.free_in_arena_bytes,
            a.arena_bytes
        );
        let pct = a.fragmentation_pct();
        assert!(
            (0.0..=100.0).contains(&pct),
            "fragmentation {pct} out of range"
        );
    }

    /// Allocating must move the numbers, and must move the RIGHT one.
    ///
    /// This test was written asserting that a 4 MiB `Vec` raises
    /// `live_bytes`, and it FAILED — correctly. glibc serves anything above
    /// `MMAP_THRESHOLD` (128 KiB by default) with `mmap`, so a large
    /// allocation lands in `hblkhd`/`mmapped_bytes` and never touches
    /// `uordblks`/`live_bytes` at all. That split is the whole reason a
    /// full-window pixmap shows up in `[anon]` rather than `[heap]`, and it
    /// is worth pinning rather than discovering again.
    #[test]
    #[cfg(all(unix, not(target_os = "macos")))]
    fn arena_and_mmap_allocations_move_the_right_counters() {
        // ARENA allocations are tracked, and that is what this feature needs:
        // the live-vs-freed-but-held split for churn. Verified, pinned.
        // ARENA allocations move `live_bytes`. 32 MiB of 16 KiB blocks, so
        // the delta dominates whatever other tests are doing concurrently.
        let Some(before_small) = allocator_stats() else {
            return;
        };
        let small: Vec<Vec<u8>> = (0..2048).map(|_| vec![7u8; 16 * 1024]).collect();
        let small = core::hint::black_box(small);
        let Some(after_small) = allocator_stats() else {
            return;
        };
        let grew = after_small
            .live_bytes
            .saturating_sub(before_small.live_bytes);
        assert!(
            grew > 16 * 1024 * 1024,
            "32 MiB of arena allocations must raise live_bytes by well over \
             16 MiB; saw {grew} ({} -> {})",
            before_small.live_bytes,
            after_small.live_bytes
        );
        drop(small);

        // A LARGE allocation goes to mmap instead, moving `mmapped_bytes`
        // while leaving the arena counters alone.
        //
        // `black_box` is load-bearing. Without it LLVM deletes an allocation
        // whose result is never read, no counter moves, and the test "proves"
        // the field is broken — which is exactly the wrong conclusion an
        // earlier version of this test reached.
        let Some(before_big) = allocator_stats() else {
            return;
        };
        let mut big: Vec<u8> = vec![7u8; 64 * 1024 * 1024];
        big[12345] = 9;
        let big = core::hint::black_box(big);
        let Some(after_big) = allocator_stats() else {
            return;
        };
        assert!(
            after_big.mmapped_bytes > before_big.mmapped_bytes,
            "a 64 MiB allocation must raise mmapped_bytes ({} -> {})",
            before_big.mmapped_bytes,
            after_big.mmapped_bytes
        );
        // NOT asserted: that `live_bytes` is UNCHANGED across the mmap.
        // It should be — an mmapped block is not an arena block — but
        // `mallinfo2` is PROCESS-GLOBAL and the test harness runs tests in
        // parallel, so other threads' allocations move it between the two
        // samples. That assertion passed alone and failed in the full suite.
        // Only deltas large enough to dominate concurrent noise (64 MiB) are
        // safe to assert here.
        drop(big);
    }

    #[test]
    #[cfg(feature = "probe")] // without the probe the const stub returns "cb:?" by design
    fn fn_name_resolution_never_collapses_to_a_bare_question_mark() {
        // The static-link fallback law: two DIFFERENT functions must get
        // DIFFERENT span names even when dladdr cannot name them — "cb:?"
        // for everything made the per-callback panels a single useless bar.
        // #[inline(never)] + distinct bodies: release-mode ICF merges
        // identical functions into ONE address, which is not what this
        // test is about.
        #[inline(never)]
        fn f_one() -> u32 {
            std::hint::black_box(1)
        }
        #[inline(never)]
        fn f_two() -> u32 {
            std::hint::black_box(2)
        }
        let a = Probe::span_for_fn(f_one as usize);
        let b = Probe::span_for_fn(f_two as usize);
        drop(a);
        drop(b);
        let names = super::imp::resolve_fn_name(f_one as usize);
        let names2 = super::imp::resolve_fn_name(f_two as usize);
        assert_ne!(
            names, "cb:?",
            "unresolved symbol must fall back to an address form"
        );
        assert_ne!(
            names, names2,
            "distinct fns must resolve to distinct span names"
        );
        assert!(
            names.starts_with("cb:"),
            "span name keeps the cb: family prefix: {names}"
        );
        // With addr2line installed (Linux), the DEBUG-symbol fallback must
        // recover the REAL name — the test binary carries debuginfo. It runs
        // on a DETACHED thread now (the synchronous form stalled the caller
        // for seconds per callback on a big-debuginfo binary — the azpaint
        // first-stroke stall, 2026-08-29), so the first resolution returns
        // the offset placeholder and the cache upgrades in place: poll for
        // the upgrade instead of asserting on the first return.
        if cfg!(target_os = "linux")
            && std::process::Command::new("addr2line")
                .arg("--version")
                .output()
                .is_ok_and(|o| o.status.success())
        {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            let mut latest = names;
            while !latest.contains("f_one") && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(100));
                latest = super::imp::resolve_fn_name(f_one as usize);
            }
            assert!(
                latest.contains("f_one"),
                "the background addr2line upgrade must land, got: {latest}"
            );
        }
    }
}

/// The resolved name of a callback function pointer — the same ladder the
/// `cb:` spans use (`dladdr` → `addr2line` → module-relative offset →
/// address). Cached; the returned string lives for the process.
///
/// The action journal names handlers with this, so a problem report reads
/// `cb:on_save_clicked` rather than a bare pointer.
// const only in the no-`probe` stub config; the enabled resolver is not const
#[allow(clippy::missing_const_for_fn)]
#[must_use]
pub fn callback_name(fn_ptr: usize) -> &'static str {
    imp::resolve_fn_name(fn_ptr)
}
