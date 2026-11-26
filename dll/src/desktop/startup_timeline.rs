//! The startup timeline: where the first milliseconds of an app go, from the moment the OS created
//! the process to the first frame on screen and the first input handled.
//!
//! Default OFF and free when off: `AZ_STARTUP=1` (on Android `adb shell setprop debug.az.startup 1`,
//! or `debug.az.env` with the pair) turns it on. The app owns ONE [`StartupTimeline`] (an
//! `Arc` made by `App::create` and kept in the `AppShared` every window and worker receives; no
//! static, no thread-local) and every layer marks named points on it:
//!
//! ```text
//! [startup] #   at_ms     +ms thread        mark
//! [startup] 0    0.000   0.000 -             process_start
//! [startup] 1   38.412  38.412 main          library_entered
//! [startup] 2   38.801   0.389 main          app_create:settings_and_rice
//! ```
//!
//! `at_ms` counts from the process creation the OS recorded (macOS and iOS `sysctl kern.proc.pid`,
//! Linux and Android `/proc/self/stat` against `/proc/uptime`, Windows `GetProcessTimes`), `+ms` is
//! the time since the previous mark OF THE SAME THREAD, so a worker thread's mark does not
//! shorten a main-thread step. When the first frame is presented the whole table is printed
//! through the engine's own log (stderr, the log file and, with the debug server, its queue: on
//! Android that is logcat, on iOS the console), later marks print as single lines, and
//! `GET /startup` of the debug server answers the same data as JSON ([`StartupTimeline::to_json`]).
//!
//! The parts that read the OS are small; what they parse is pure ([`proc_stat_start_ticks`],
//! [`proc_uptime_us`], [`age_from_proc_us`], [`kinfo_proc_start_unix_us`],
//! [`filetime_to_unix_us`]) and has its tests.

use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Mutex, PoisonError,
    },
    time::Instant,
};

/// The most marks one timeline keeps: a start that makes more is a loop that marks (a mark per
/// layout pass of a resize drag), and keeping them all would be a leak.
pub const MAX_MARKS: usize = 4096;

/// One named point of the start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    /// What happened (`window_created`, `app_create:system_style`).
    pub name: &'static str,
    /// More about it (`#2 reason=Resize`); empty when there is nothing to add.
    pub detail: String,
    /// Microseconds since the origin, the first line of the library (`App::create`).
    pub at_us: u64,
    /// The thread that made the mark: its name, or its id when it has none.
    pub thread: String,
}

/// How long before the origin the process was created, and where that number came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessAge {
    /// Microseconds from the process creation to the origin.
    pub micros: u64,
    /// The OS facility it came from (`sysctl`, `/proc`, `GetProcessTimes`).
    pub source: &'static str,
    /// The resolution of that facility in microseconds (1 on macOS and Windows, 10000 on Linux).
    pub resolution_us: u64,
}

/// What a timeline sends its lines to (the engine's own log, in the app; a vector in a test).
pub type Sink = fn(&str);

/// The named points of one app's start. Cheap when off: [`StartupTimeline::mark`] is a test of a
/// bool. Shared as `Arc<StartupTimeline>`.
pub struct StartupTimeline {
    enabled: bool,
    origin: Instant,
    before_origin: Option<ProcessAge>,
    marks: Mutex<Vec<Mark>>,
    /// How many layout passes began (the counter of [`Self::layout_begin`]).
    layouts: AtomicU32,
    /// The table was printed: from then on a mark prints as a line of its own.
    printed: AtomicBool,
    sink: Option<Sink>,
}

impl core::fmt::Debug for StartupTimeline {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StartupTimeline")
            .field("enabled", &self.enabled)
            .field("marks", &self.len())
            .finish()
    }
}

impl StartupTimeline {
    /// A timeline that records nothing: the one of an app (a test, a tool) that did not ask.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            origin: Instant::now(),
            before_origin: None,
            marks: Mutex::new(Vec::new()),
            layouts: AtomicU32::new(0),
            printed: AtomicBool::new(false),
            sink: None,
        }
    }

    /// A timeline whose clock starts at `origin` (the first line of the library), the process
    /// having been created `before_origin` earlier (`None` when the OS did not say).
    #[must_use]
    pub fn new(enabled: bool, origin: Instant, before_origin: Option<ProcessAge>) -> Self {
        if !enabled {
            return Self::disabled();
        }
        Self {
            enabled,
            origin,
            before_origin,
            marks: Mutex::new(Vec::new()),
            layouts: AtomicU32::new(0),
            printed: AtomicBool::new(false),
            sink: None,
        }
    }

    /// The timeline of an app: on when `enabled`, the origin being `origin` (an `Instant` read at
    /// `entered_wall`, the first line of `App::create`), the process age read from the OS now.
    #[must_use]
    pub fn for_process(enabled: bool, origin: Instant, entered_wall: std::time::SystemTime) -> Self {
        if !enabled {
            return Self::disabled();
        }
        let wall_us = entered_wall
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|d| u64::try_from(d.as_micros()).ok());
        let before = wall_us.and_then(process_age_at);
        let timeline = Self::new(true, origin, before);
        // The origin IS the library's first line: its mark is at 0, whatever time reading the
        // environment took since.
        timeline.lock().push(Mark {
            name: "library_entered",
            detail: String::new(),
            at_us: 0,
            thread: thread_label(),
        });
        timeline
    }

    /// The lines this timeline prints go to `sink`.
    #[must_use]
    pub const fn with_sink(mut self, sink: Sink) -> Self {
        self.sink = Some(sink);
        self
    }

    /// Whether this timeline records.
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// How many marks there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Whether nothing was marked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Mark>> {
        self.marks.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Records `name` now. Nothing when the timeline is off.
    pub fn mark(&self, name: &'static str) {
        if self.enabled {
            self.record(name, String::new(), false);
        }
    }

    /// Records `name` now with a detail that is only built when the timeline is on.
    pub fn mark_with(&self, name: &'static str, detail: impl FnOnce() -> String) {
        if self.enabled {
            self.record(name, detail(), false);
        }
    }

    /// Records `name` now unless a mark of that name exists: true when THIS call made it (the
    /// first present, the first event, the first a11y tree are marked by whoever gets there
    /// first).
    pub fn mark_once(&self, name: &'static str) -> bool {
        self.enabled && self.record(name, String::new(), true)
    }

    /// [`Self::mark_once`] with a detail built only when this call makes the mark.
    pub fn mark_once_with(&self, name: &'static str, detail: impl FnOnce() -> String) -> bool {
        if !self.enabled {
            return false;
        }
        // The detail is built outside the lock; a second thread that loses the race drops it.
        let text = if self.lock().iter().any(|m| m.name == name) {
            return false;
        } else {
            detail()
        };
        self.record(name, text, true)
    }

    /// Whether a mark of `name` exists.
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.lock().iter().any(|m| m.name == name)
    }

    /// The microseconds of the first mark of `name`, from the process start where the OS told it,
    /// else from the origin.
    #[must_use]
    pub fn at_us_of(&self, name: &str) -> Option<u64> {
        let from = self.before_origin.map_or(0, |a| a.micros);
        self.lock()
            .iter()
            .find(|m| m.name == name)
            .map(|m| from.saturating_add(m.at_us))
    }

    fn record(&self, name: &'static str, detail: String, once: bool) -> bool {
        let thread = thread_label();
        let line = {
            let mut marks = self.lock();
            if once && marks.iter().any(|m| m.name == name) {
                return false;
            }
            if marks.len() >= MAX_MARKS {
                return false;
            }
            // The clock is read under the lock so the order of the vector is the order of time.
            let at_us = u64::try_from(self.origin.elapsed().as_micros()).unwrap_or(u64::MAX);
            let before = self.before_origin.map_or(0, |a| a.micros);
            let since_previous = marks
                .iter()
                .rev()
                .find(|m| m.thread == thread)
                .map(|m| at_us.saturating_sub(m.at_us));
            let mark = Mark { name, detail, at_us, thread };
            let line = self
                .printed
                .load(Ordering::Acquire)
                .then(|| late_line(&mark, before, since_previous));
            marks.push(mark);
            line
        };
        if let (Some(line), Some(sink)) = (line, self.sink) {
            sink(&line);
        }
        true
    }

    /// A layout pass begins: the counter goes up and the pass is marked with its number and the
    /// reason it ran (`Initial`, `Resize`, ...). The number goes to [`Self::layout_end`].
    pub fn layout_begin(&self, reason: &str) -> u32 {
        if !self.enabled {
            return 0;
        }
        let n = self.layouts.fetch_add(1, Ordering::AcqRel).saturating_add(1);
        self.record("layout_begin", format!("#{n} reason={reason}"), false);
        n
    }

    /// Layout pass `n` ended after `passes` pass(es) of the lifecycle loop; `phases` is the
    /// per-phase line of the pass (empty when there is none).
    pub fn layout_end(&self, n: u32, passes: usize, phases: &str) {
        if !self.enabled {
            return;
        }
        let detail = if phases.is_empty() {
            format!("#{n} passes={passes}")
        } else {
            format!("#{n} passes={passes} {phases}")
        };
        self.record("layout_end", detail, false);
    }

    /// How many layout passes began.
    #[must_use]
    pub fn layout_count(&self) -> u32 {
        self.layouts.load(Ordering::Acquire)
    }

    /// A copy of the marks, in the order they were made.
    #[must_use]
    pub fn marks(&self) -> Vec<Mark> {
        self.lock().clone()
    }

    /// The time from the process start to the origin: what happened before the library ran.
    #[must_use]
    pub const fn before_origin(&self) -> Option<ProcessAge> {
        self.before_origin
    }

    /// Whether the table was printed.
    #[must_use]
    pub fn table_printed(&self) -> bool {
        self.printed.load(Ordering::Acquire)
    }

    /// Prints the table through the sink, once: true when this call printed it (the caller that
    /// marks the first present prints). The marks made after it print as single lines.
    pub fn print_table(&self) -> bool {
        if !self.enabled || self.printed.swap(true, Ordering::AcqRel) {
            return false;
        }
        if let Some(sink) = self.sink {
            for line in self.report_lines() {
                sink(&line);
            }
        }
        true
    }

    /// The table: one line per mark, then one line per layout pass and the headline numbers.
    #[must_use]
    pub fn report_lines(&self) -> Vec<String> {
        let marks = self.marks();
        let before = self.before_origin;
        render_table(&marks, before, self.layout_count())
    }

    /// The same data as JSON, for `GET /startup`. Marks are in the order they were made;
    /// `at_ms` counts from the process start where the OS told it (`process_start_known`),
    /// else from the origin.
    #[must_use]
    pub fn to_json(&self) -> String {
        render_json(self.enabled, &self.marks(), self.before_origin, self.layout_count())
    }
}

/// The name of the calling thread, or its id.
fn thread_label() -> String {
    let current = std::thread::current();
    current
        .name()
        .map_or_else(|| format!("{:?}", current.id()), str::to_owned)
}

fn ms(us: u64) -> f64 {
    us as f64 / 1000.0
}

/// One mark made after the table was printed, as the line it prints.
fn late_line(mark: &Mark, before_us: u64, since_previous: Option<u64>) -> String {
    let at = ms(before_us.saturating_add(mark.at_us));
    let step = since_previous.map_or_else(|| "-".to_owned(), |us| format!("{:.3}", ms(us)));
    let detail = if mark.detail.is_empty() { String::new() } else { format!(" {}", mark.detail) };
    format!("[startup] late: {} at {at:.3} ms (+{step} ms, {}){detail}", mark.name, mark.thread)
}

/// The table of `marks` as lines (every one begins `[startup] `).
#[must_use]
pub fn render_table(marks: &[Mark], before: Option<ProcessAge>, layouts: u32) -> Vec<String> {
    let before_us = before.map_or(0, |a| a.micros);
    let mut lines = Vec::with_capacity(marks.len() + 8);
    let origin_note = match before {
        Some(age) => format!(
            "process start from {} (resolution {} us), origin = App::create entry {:.3} ms after it",
            age.source,
            age.resolution_us,
            ms(age.micros)
        ),
        None => "process start unknown on this platform: times count from App::create entry".to_owned(),
    };
    lines.push(format!("[startup] timeline: {origin_note}"));
    lines.push(format!(
        "[startup] {:>3} {:>10} {:>9}  {:<22} mark",
        "#", "at_ms", "+ms", "thread"
    ));
    if before.is_some() {
        lines.push(format!(
            "[startup] {:>3} {:>10.3} {:>9}  {:<22} process_start",
            0, 0.0, "-", "-"
        ));
    }
    // The previous mark of each thread, for the step column.
    let mut last: Vec<(&str, u64)> = Vec::new();
    for (i, m) in marks.iter().enumerate() {
        let at = before_us.saturating_add(m.at_us);
        let step = match last.iter_mut().find(|(t, _)| *t == m.thread) {
            Some((_, prev)) => {
                let d = m.at_us.saturating_sub(*prev);
                *prev = m.at_us;
                format!("{:.3}", ms(d))
            }
            None => {
                last.push((m.thread.as_str(), m.at_us));
                // The first mark of a thread has no step of its own; the main thread's first is
                // the library entry, which is the time since the process start.
                if before.is_some() && i == 0 {
                    format!("{:.3}", ms(at))
                } else {
                    "-".to_owned()
                }
            }
        };
        let detail = if m.detail.is_empty() { String::new() } else { format!(" {}", m.detail) };
        lines.push(format!(
            "[startup] {:>3} {:>10.3} {:>9}  {:<22} {}{}",
            i + 1,
            ms(at),
            step,
            truncate_thread(&m.thread),
            m.name,
            detail
        ));
    }
    lines.push(format!("[startup] layout passes begun: {layouts}"));
    if let Some(at) = first_of(marks, "first_present") {
        lines.push(format!(
            "[startup] first_present at {:.3} ms ({:.3} ms after the library was entered)",
            ms(before_us.saturating_add(at)),
            ms(at)
        ));
    }
    lines
}

fn first_of(marks: &[Mark], name: &str) -> Option<u64> {
    marks.iter().find(|m| m.name == name).map(|m| m.at_us)
}

fn truncate_thread(t: &str) -> String {
    t.chars().take(22).collect()
}

/// `text` as the inside of a JSON string.
#[must_use]
pub fn json_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// The JSON of a timeline ([`StartupTimeline::to_json`]).
#[must_use]
pub fn render_json(enabled: bool, marks: &[Mark], before: Option<ProcessAge>, layouts: u32) -> String {
    let before_us = before.map_or(0, |a| a.micros);
    let mut out = String::with_capacity(256 + marks.len() * 96);
    out.push_str(&format!("{{\"enabled\":{enabled},\"process_start_known\":{}", before.is_some()));
    if let Some(age) = before {
        out.push_str(&format!(
            ",\"process_start_source\":\"{}\",\"process_start_resolution_us\":{},\"before_library_ms\":{:.3}",
            json_escape(age.source),
            age.resolution_us,
            ms(age.micros)
        ));
    }
    out.push_str(&format!(",\"layout_passes\":{layouts},\"marks\":["));
    for (i, m) in marks.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"i\":{},\"name\":\"{}\",\"at_ms\":{:.3},\"since_library_ms\":{:.3},\"thread\":\"{}\",\"detail\":\"{}\"}}",
            i + 1,
            json_escape(m.name),
            ms(before_us.saturating_add(m.at_us)),
            ms(m.at_us),
            json_escape(&m.thread),
            json_escape(&m.detail)
        ));
    }
    out.push(']');
    // The numbers a script wants without walking the marks.
    for (key, name) in [
        ("first_present_ms", "first_present"),
        ("first_event_ms", "first_event"),
        ("font_scan_complete_ms", "font_scan_complete"),
        ("a11y_tree_first_built_ms", "a11y_tree_first_built"),
    ] {
        if let Some(at) = first_of(marks, name) {
            out.push_str(&format!(",\"{key}\":{:.3}", ms(before_us.saturating_add(at))));
        }
    }
    out.push('}');
    out
}

// -- what the OS says of the process start (pure parts first) ------------------------------------

/// Field 22 of a `/proc/<pid>/stat` line: the process's start time in clock ticks since boot.
///
/// The command (field 2) sits in parentheses and may hold spaces and parentheses of its own, so
/// the fields are counted from the LAST `)`: `state` is field 3, `starttime` field 22, the 20th
/// after the parenthesis.
#[must_use]
pub fn proc_stat_start_ticks(stat: &str) -> Option<u64> {
    let rest = stat.get(stat.rfind(')')? + 1..)?;
    rest.split_ascii_whitespace().nth(19)?.parse().ok()
}

/// The first number of `/proc/uptime` (`"12345.67 98765.43"`), in microseconds.
#[must_use]
pub fn proc_uptime_us(uptime: &str) -> Option<u64> {
    let first = uptime.split_ascii_whitespace().next()?;
    let (secs, frac) = first.split_once('.').unwrap_or((first, ""));
    let secs: u64 = secs.parse().ok()?;
    let mut frac_us = 0u64;
    let mut scale = 100_000u64;
    for c in frac.chars().take(6) {
        frac_us += u64::from(c.to_digit(10)?) * scale;
        scale /= 10;
    }
    secs.checked_mul(1_000_000)?.checked_add(frac_us)
}

/// How long the process has run, from its start ticks, the tick rate and the uptime read after
/// it: the clock ticks are 1/100 s, so the answer is as coarse (the process cannot have started
/// after the uptime was read: a smaller uptime means 0).
#[must_use]
pub fn age_from_proc_us(start_ticks: u64, ticks_per_second: u64, uptime_us: u64) -> Option<u64> {
    if ticks_per_second == 0 {
        return None;
    }
    let start_us = start_ticks.checked_mul(1_000_000)? / ticks_per_second;
    Some(uptime_us.saturating_sub(start_us))
}

/// The process start of a `kinfo_proc` (what `sysctl kern.proc.pid.<pid>` returns on macOS and
/// iOS) as microseconds since the Unix epoch.
///
/// `struct kinfo_proc` begins with `struct extern_proc`, whose first member is a union that holds
/// `__p_starttime`, a `struct timeval`: `tv_sec` (8 bytes) then `tv_usec` (4 bytes, 4 of padding).
#[must_use]
pub fn kinfo_proc_start_unix_us(bytes: &[u8]) -> Option<u64> {
    let sec = i64::from_ne_bytes(bytes.get(0..8)?.try_into().ok()?);
    let usec = i32::from_ne_bytes(bytes.get(8..12)?.try_into().ok()?);
    let sec = u64::try_from(sec).ok()?;
    let usec = u64::try_from(usec).ok()?;
    if usec >= 1_000_000 || sec == 0 {
        return None;
    }
    sec.checked_mul(1_000_000)?.checked_add(usec)
}

/// A Windows `FILETIME` (100 ns ticks since 1601) as microseconds since the Unix epoch.
#[must_use]
pub fn filetime_to_unix_us(low: u32, high: u32) -> Option<u64> {
    const UNIX_EPOCH_IN_FILETIME: u64 = 116_444_736_000_000_000;
    let ticks = (u64::from(high) << 32) | u64::from(low);
    Some(ticks.checked_sub(UNIX_EPOCH_IN_FILETIME)? / 10)
}

/// How long the process had run at `now_unix_us`, as the OS records its start. `None` where the
/// platform does not say (or the read failed): the table then counts from the library entry.
#[must_use]
pub fn process_age_at(now_unix_us: u64) -> Option<ProcessAge> {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        let start = macos_process_start_unix_us()?;
        return Some(ProcessAge {
            micros: now_unix_us.saturating_sub(start),
            source: "sysctl kern.proc.pid",
            resolution_us: 1,
        });
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let _ = now_unix_us;
        let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
        let ticks = proc_stat_start_ticks(&stat)?;
        // The uptime is read AFTER the stat, so the age is never negative.
        let uptime = proc_uptime_us(&std::fs::read_to_string("/proc/uptime").ok()?)?;
        let hz = clock_ticks_per_second();
        return Some(ProcessAge {
            micros: age_from_proc_us(ticks, hz, uptime)?,
            source: "/proc/self/stat",
            resolution_us: 1_000_000 / hz.max(1),
        });
    }
    #[cfg(target_os = "windows")]
    {
        let start = windows_process_start_unix_us()?;
        return Some(ProcessAge {
            micros: now_unix_us.saturating_sub(start),
            source: "GetProcessTimes",
            resolution_us: 1,
        });
    }
    #[allow(unreachable_code)]
    {
        let _ = now_unix_us;
        None
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn clock_ticks_per_second() -> u64 {
    extern "C" {
        fn sysconf(name: i32) -> i64;
    }
    // _SC_CLK_TCK is 2 on Linux (glibc, musl) and on Android (bionic).
    const SC_CLK_TCK: i32 = 2;
    // SAFETY: sysconf takes an int and has no memory effects.
    let hz = unsafe { sysconf(SC_CLK_TCK) };
    u64::try_from(hz).ok().filter(|hz| *hz > 0).unwrap_or(100)
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn macos_process_start_unix_us() -> Option<u64> {
    use core::ffi::{c_int, c_void};
    extern "C" {
        fn sysctl(
            name: *mut c_int,
            namelen: u32,
            oldp: *mut c_void,
            oldlenp: *mut usize,
            newp: *mut c_void,
            newlen: usize,
        ) -> c_int;
    }
    const CTL_KERN: c_int = 1;
    const KERN_PROC: c_int = 14;
    const KERN_PROC_PID: c_int = 1;
    let mut mib = [CTL_KERN, KERN_PROC, KERN_PROC_PID, c_int::try_from(std::process::id()).ok()?];
    // sizeof(struct kinfo_proc) is 648 on macOS and iOS; the room is larger so a different size
    // is read whole instead of failing with ENOMEM.
    let mut buffer = [0u8; 1024];
    let mut len = buffer.len();
    // SAFETY: `mib` has 4 entries, `buffer` has `len` bytes, no new value is set.
    let status = unsafe {
        sysctl(
            mib.as_mut_ptr(),
            4,
            buffer.as_mut_ptr().cast::<c_void>(),
            &mut len,
            core::ptr::null_mut(),
            0,
        )
    };
    if status != 0 || len < 16 {
        return None;
    }
    kinfo_proc_start_unix_us(&buffer[..len])
}

#[cfg(target_os = "windows")]
fn windows_process_start_unix_us() -> Option<u64> {
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn GetProcessTimes(
            process: isize,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
    }
    let (mut creation, mut exit, mut kernel, mut user) = (FileTime::default(), FileTime::default(), FileTime::default(), FileTime::default());
    // SAFETY: the pseudo handle of the current process and four valid out pointers.
    let ok = unsafe { GetProcessTimes(GetCurrentProcess(), &mut creation, &mut exit, &mut kernel, &mut user) };
    if ok == 0 {
        return None;
    }
    filetime_to_unix_us(creation.low, creation.high)
}

#[cfg(test)]
#[path = "startup_timeline_test.rs"]
mod startup_timeline_test;
