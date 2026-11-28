//! AzSelfTest: every device capability the engine offers, one probe each, on the device it runs on.
//!
//! The probes are in [`probes`] (one file per group, one row each), the verdicts and the report in
//! [`report`] (std only, tested on every host). This file is the runner and the list:
//!
//! * a list with a verdict per row, 'Run all' and a 'Run' per row (a row that needs a person says what to do);
//! * a 100 ms timer runs one probe at a time, calling its step until it answers;
//! * the report is ONE line `AZ_SELFTEST_REPORT {json}` on stdout (logcat on Android, `os_log` / the console on
//!   iOS) and a file `az-selftest-report.json` in the app's data directory (`AZ_SELFTEST_REPORT_FILE` overrides),
//!   printed whenever the queue runs empty after a run. On Android a report longer than 3000 bytes leaves as numbered
//!   parts, `AZ_SELFTEST_REPORT_PART i/n <slice>` (the slices joined in order are the JSON): logcat drops what is past
//!   ~4000 bytes of a line without a word, and a full report of 30 rows is longer than that;
//! * unattended (`AZ_SELFTEST=1`, `--selftest`, on Android `adb shell setprop debug.az.selftest 1`): runs every row
//!   that needs no person (the others are listed with their instruction), writes the report, closes the window; the
//!   process exits with 1 when a capability that says it is available failed.
//!
//! `AZ_SELFTEST_ROWS=<id>,<id>` (Android: `setprop debug.az.selftest_rows <id>`) runs just those rows, with a person (a harness that taps the
//! dialogs) or unattended, prints the report; an attended run stays open for the harness.
//! A system permission dialog appears only when a person runs a row that needs one: an unattended run never asks, it
//! lists what to grant (`adb shell pm grant ...`). The same app runs on the desktop and says UNAVAILABLE for what
//! only a phone has, so it cannot rot. A QA artifact: never a published demo.

use std::{collections::VecDeque, time::Instant};

use azul::{
    prelude::*,
    time::{Duration, SystemTimeDiff},
};

pub mod probes;
pub mod render_lines;
pub mod report;

use probes::{Ctx, ProbeDef, Step, WidgetFn};
use report::{Context, RowResult, Verdict};

/// How often the runner looks at the running probe (a frame's worth: the renderer row animates and wants a frame per tick).
const TICK_MS: u64 = 16;
/// The longest a probe may run before the runner calls it a failure (a probe that never answers is a bug).
const HARD_LIMIT_MS: u64 = 120_000;

/// The probe that runs now.
struct Running {
    idx: usize,
    started: Instant,
    phase: u32,
    widget: bool,
    scratch: Vec<u64>,
}

/// The app's state: the rows, the queue and what runs.
pub struct SelfTest {
    rows: Vec<(&'static ProbeDef, Option<RowResult>)>,
    queue: VecDeque<usize>,
    running: Option<Running>,
    unattended: bool,
    /// A harness answers the `AZ_SELFTEST_AWAIT` lines (`AZ_SELFTEST_FED=1`): the rows of [`probes::FEEDABLE`] run.
    fed: bool,
    started: Instant,
    report_due: bool,
    exit_code: i32,
}

/// What the list draws of a row.
struct RowView {
    title: &'static str,
    status: String,
    color: &'static str,
    detail: String,
}

/// A click on a row's 'Run' button: the app's state and the row.
struct RowClick {
    app: RefAny,
    idx: usize,
}

impl SelfTest {
    fn new(unattended: bool) -> Self {
        let rows = probes::all().into_iter().map(|def| (def, None)).collect();
        let mut state = Self {
            rows,
            queue: VecDeque::new(),
            running: None,
            unattended,
            fed: unattended && std::env::var("AZ_SELFTEST_FED").is_ok_and(|v| !v.is_empty() && v != "0"),
            started: Instant::now(),
            report_due: false,
            exit_code: 0,
        };
        // Rows asked for by name (a harness that plays the person for one row: `AZ_SELFTEST_ROWS`, on Android the property
        // `debug.az.selftest_rows`) run on their own, attended or not; otherwise an unattended run does every row and an
        // attended one waits for a tap.
        let wanted = rows_requested();
        if !wanted.is_empty() {
            state.queue_rows(&wanted);
        } else if unattended {
            state.queue_all();
        }
        state
    }

    /// Queue the rows whose ids are in `ids`.
    fn queue_rows(&mut self, ids: &[String]) {
        for idx in 0..self.rows.len() {
            if ids.iter().any(|id| id == self.rows[idx].0.id) {
                self.queue_one(idx);
            }
        }
    }

    fn queue_all(&mut self) {
        for idx in 0..self.rows.len() {
            self.queue_one(idx);
        }
    }

    fn queue_one(&mut self, idx: usize) {
        let running_now = self.running.as_ref().map(|run| run.idx) == Some(idx);
        if idx >= self.rows.len() || running_now || self.queue.contains(&idx) {
            return;
        }
        self.rows[idx].1 = None;
        self.queue.push_back(idx);
        self.report_due = true;
    }

    fn idle(&self) -> bool {
        self.running.is_none() && self.queue.is_empty()
    }

    fn results(&self) -> Vec<RowResult> {
        self.rows.iter().filter_map(|(_, result)| result.clone()).collect()
    }

    fn finish(&mut self, idx: usize, verdict: Verdict, ms: u64) {
        let def = self.rows[idx].0;
        self.rows[idx].1 = Some(RowResult {
            id: def.id.to_string(),
            group: def.group.to_string(),
            title: def.title.to_string(),
            verdict,
            ms,
        });
    }

    /// The widget the running row wants in the DOM, once it asked for it, and how long the row has run.
    fn active_widget(&self) -> Option<(WidgetFn, u64)> {
        let run = self.running.as_ref()?;
        if !run.widget {
            return None;
        }
        let elapsed = u64::try_from(run.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.rows[run.idx].0.widget.map(|widget| (widget, elapsed))
    }

    /// The app's lifecycle handler heard `event` (`CallbackInfo::get_lifecycle_event`): the running row's scratch keeps the codes of
    /// `probes::lifecycle` in the order they came.
    pub fn note_lifecycle(&mut self, event: azul::dom::LifecycleEvent) {
        use azul::dom::LifecycleEvent;
        if let Some(run) = self.running.as_mut() {
            run.scratch.push(match event {
                LifecycleEvent::Paused => probes::lifecycle::PAUSED,
                LifecycleEvent::Resumed => probes::lifecycle::RESUMED,
                LifecycleEvent::LowMemory => 3,
                LifecycleEvent::WillTerminate => 4,
            });
        }
    }

    /// The app's open handler heard a request: the running row's scratch keeps what `probes::files` reads back from it (the number of
    /// requests so far, the items, the files with a working copy, the first file's size and digest).
    pub fn note_open(&mut self, summary: &probes::files::OpenSummary) {
        use probes::files::{SCRATCH_COPIES, SCRATCH_FIRST_FNV, SCRATCH_FIRST_LEN, SCRATCH_ITEMS, SCRATCH_LEN, SCRATCH_REQUESTS};
        if let Some(run) = self.running.as_mut() {
            if run.scratch.len() < SCRATCH_LEN {
                run.scratch.resize(SCRATCH_LEN, 0);
            }
            run.scratch[SCRATCH_REQUESTS] += 1;
            run.scratch[SCRATCH_ITEMS] = summary.items;
            run.scratch[SCRATCH_COPIES] = summary.copies;
            run.scratch[SCRATCH_FIRST_LEN] = summary.first_len;
            run.scratch[SCRATCH_FIRST_FNV] = summary.first_fnv;
        }
    }

    /// A frame of the running row's capture widget arrived (the widget's callback, on the loop thread, between two ticks): the
    /// row's scratch keeps `[frames, width, height, loudest sample in millionths]` and the row's next tick reads it.
    pub fn note_media_frame(&mut self, width: u32, height: u32, loudest_micro: u64) {
        if let Some(run) = self.running.as_mut() {
            if run.scratch.len() < 4 {
                run.scratch.resize(4, 0);
            }
            run.scratch[0] += 1;
            run.scratch[1] = u64::from(width);
            run.scratch[2] = u64::from(height);
            run.scratch[3] = run.scratch[3].max(loudest_micro);
        }
    }

    /// One tick: start the next row if none runs, step the running one. Whether the list changed.
    fn advance(&mut self, info: &mut CallbackInfo) -> bool {
        let mut changed = false;
        if self.running.is_none() {
            let Some(idx) = self.queue.pop_front() else {
                return false;
            };
            let def = self.rows[idx].0;
            // A row a harness feeds (`AZ_SELFTEST_FED=1`) runs: the probe says what it waits for on stdout.
            let fed = self.fed && probes::FEEDABLE.contains(&def.id);
            if let (true, Some(what)) = (self.unattended && !fed, def.human) {
                self.finish(idx, Verdict::NeedsHuman(what.to_string()), 0);
                return true;
            }
            self.running = Some(Running {
                idx,
                started: Instant::now(),
                phase: 0,
                widget: false,
                scratch: Vec::new(),
            });
            changed = true;
        }
        let Some(mut run) = self.running.take() else {
            return changed;
        };
        let def = self.rows[run.idx].0;
        let elapsed = u64::try_from(run.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let done = self.results();
        let (step, phase, widget, refresh) = {
            let mut ctx = Ctx {
                info,
                elapsed_ms: elapsed,
                phase: run.phase,
                unattended: self.unattended,
                want_widget: run.widget,
                want_refresh: false,
                scratch: &mut run.scratch,
                done: &done,
                note: def.note,
            };
            let step = (def.step)(&mut ctx);
            (step, ctx.phase, ctx.want_widget, ctx.want_refresh)
        };
        run.phase = phase;
        if widget != run.widget {
            run.widget = widget;
            changed = true;
        }
        // An animation asks for a refreshed DOM on every tick.
        changed |= refresh;
        match step {
            Step::Done(verdict) => {
                self.finish(run.idx, verdict, elapsed);
                changed = true;
            }
            Step::Wait if elapsed > HARD_LIMIT_MS => {
                self.finish(
                    run.idx,
                    Verdict::Fail(format!("the probe never finished in {HARD_LIMIT_MS} ms")),
                    elapsed,
                );
                changed = true;
            }
            Step::Wait => self.running = Some(run),
        }
        changed
    }

    fn views(&self) -> Vec<RowView> {
        self.rows
            .iter()
            .enumerate()
            .map(|(idx, (def, result))| {
                let running = self.running.as_ref().map(|run| run.idx) == Some(idx);
                let (status, color, detail) = match result {
                    Some(row) => (row.verdict.code().to_string(), color_of(&row.verdict), row.verdict.detail().to_string()),
                    None if running => ("RUNNING".to_string(), "#2563eb", String::new()),
                    None if self.queue.contains(&idx) => ("QUEUED".to_string(), "#6b7280", String::new()),
                    None => ("-".to_string(), "#6b7280", String::new()),
                };
                RowView {
                    title: def.title,
                    status,
                    color,
                    detail,
                }
            })
            .collect()
    }

    fn summary(&self) -> String {
        let c = report::counts(&self.results());
        format!(
            "{} pass, {} unavailable, {} denied, {} FAIL, {} need a person, {} pending{}",
            c.pass,
            c.unavailable,
            c.denied,
            c.fail,
            c.needs_human,
            c.pending,
            if self.unattended { " (unattended)" } else { "" }
        )
    }

    /// Prints the report line, writes the file; the exit code it asks for.
    fn emit_report(&self) -> i32 {
        let rows = self.results();
        let ctx = Context {
            app: "AzSelfTest".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            platform: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            unattended: self.unattended,
            total_ms: u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX),
        };
        // On Android one line when it fits, else numbered parts (logcat drops what is past ~4000 bytes of a line without a word);
        // elsewhere always the one line (the readers of iOS and the desktop grep that, and their consoles keep long lines).
        let limit = if cfg!(target_os = "android") { report::MAX_LINE_BYTES } else { usize::MAX };
        for line in report::report_lines(&ctx, &rows, limit) {
            println!("{line}");
        }
        let path = report_path();
        if let Err(e) = std::fs::write(&path, report::report_json(&ctx, &rows)) {
            eprintln!("AzSelfTest: cannot write the report file {}: {e}", path.display());
        }
        report::exit_code(&rows)
    }
}

fn color_of(verdict: &Verdict) -> &'static str {
    match verdict {
        Verdict::Pass(_) => "#16a34a",
        Verdict::Unavailable(_) | Verdict::Pending(_) => "#6b7280",
        Verdict::Denied(_) => "#d97706",
        Verdict::Fail(_) => "#dc2626",
        Verdict::NeedsHuman(_) => "#2563eb",
    }
}

/// Where the report file goes: the app's data directory on a phone (`HOME` is the app's folder there), the
/// temp directory on a desktop.
fn report_path() -> std::path::PathBuf {
    if let Some(path) = std::env::var_os("AZ_SELFTEST_REPORT_FILE") {
        return path.into();
    }
    let dir = if cfg!(any(target_os = "android", target_os = "ios")) {
        std::env::var_os("HOME").map(std::path::PathBuf::from).filter(|p| p.is_dir())
    } else {
        None
    };
    dir.unwrap_or_else(std::env::temp_dir).join("az-selftest-report.json")
}

/// Whether this run has no person in it: `AZ_SELFTEST` set (not `0`), the `--selftest` argument, or on Android the
/// property `debug.az.selftest` set to 1.
fn unattended_requested() -> bool {
    let env_set = std::env::var("AZ_SELFTEST").is_ok_and(|v| !v.is_empty() && v != "0");
    let arg_set = std::env::args().any(|arg| arg == "--selftest" || arg == "--unattended");
    env_set || arg_set || android_property_set()
}

/// The value of the Android system property `name` (`getprop`), empty when it is not set.
#[cfg(target_os = "android")]
fn android_property(name: &str) -> String {
    std::process::Command::new("getprop")
        .arg(name)
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_default()
}

#[cfg(target_os = "android")]
fn android_property_set() -> bool {
    android_property("debug.az.selftest") == "1"
}

/// The rows a harness asked for: `AZ_SELFTEST_ROWS`, on Android the property `debug.az.selftest_rows`.
fn rows_requested() -> Vec<String> {
    let from_env = std::env::var("AZ_SELFTEST_ROWS").unwrap_or_default();
    #[cfg(target_os = "android")]
    let from_property = android_property("debug.az.selftest_rows");
    #[cfg(not(target_os = "android"))]
    let from_property = String::new();
    report::parse_row_list(&format!("{from_env},{from_property}"))
}

#[cfg(not(target_os = "android"))]
const fn android_property_set() -> bool {
    false
}

extern "C" fn tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let Some(mut state) = data.downcast_mut::<SelfTest>() else {
        return TimerCallbackReturn::terminate_unchanged();
    };
    let changed = state.advance(&mut info.callback_info);
    let mut finished_unattended = false;
    if state.idle() && state.report_due {
        state.report_due = false;
        state.exit_code = state.emit_report();
        finished_unattended = state.unattended;
    }
    drop(state);
    if finished_unattended {
        info.callback_info.close_window();
        return TimerCallbackReturn::terminate_unchanged();
    }
    if changed {
        TimerCallbackReturn::continue_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_unchanged()
    }
}

/// The app's lifecycle handler: what it hears goes to the running row.
extern "C" fn on_lifecycle(mut data: RefAny, info: CallbackInfo) -> Update {
    if let Some(event) = info.get_lifecycle_event().into_option() {
        if let Some(mut state) = data.downcast_mut::<SelfTest>() {
            state.note_lifecycle(event);
        }
    }
    Update::DoNothing
}

/// The app's open handler: what the system asks this app to open (a file shared to it, "Open with", a link of a declared scheme) goes
/// to the log, one `AZ_SELFTEST_OPEN {json}` line per request, and to the running row.
extern "C" fn on_open(mut data: RefAny, info: CallbackInfo) -> Update {
    if let Some(request) = info.get_open_request().into_option() {
        let summary = probes::files::summarize(&request);
        println!("AZ_SELFTEST_OPEN {}", summary.json);
        probes::files::append_open_log(&summary.json);
        {
            use std::io::Write as _;
            let _ = std::io::stdout().flush();
        }
        if let Some(mut state) = data.downcast_mut::<SelfTest>() {
            state.note_open(&summary);
        }
    }
    Update::DoNothing
}

extern "C" fn on_created(data: RefAny, mut info: CallbackInfo) -> Update {
    info.set_lifecycle_handler(data.clone(), on_lifecycle);
    info.set_open_handler(data.clone(), on_open);
    let timer = Timer::create(data.clone(), tick, info.get_system_time_fn())
        .with_interval(Duration::System(SystemTimeDiff::from_millis(TICK_MS)));
    info.add_timer(TimerId::unique(), timer);
    Update::DoNothing
}

extern "C" fn on_run_all(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut state) = data.downcast_mut::<SelfTest>() else {
        return Update::DoNothing;
    };
    state.queue_all();
    Update::RefreshDom
}

extern "C" fn on_run_row(mut data: RefAny, _info: CallbackInfo) -> Update {
    let (mut app, idx) = {
        let Some(click) = data.downcast_ref::<RowClick>() else {
            return Update::DoNothing;
        };
        (click.app.clone(), click.idx)
    };
    let Some(mut state) = app.downcast_mut::<SelfTest>() else {
        return Update::DoNothing;
    };
    state.queue_one(idx);
    Update::RefreshDom
}

fn row_dom(data: &RefAny, idx: usize, view: &RowView) -> Dom {
    let text = Dom::create_div()
        .with_css("flex-grow: 1; min-width: 0px;")
        .with_child(Dom::create_p_with_text(view.title).with_css("margin: 0px; font-weight: bold;"))
        .with_child(
            Dom::create_p_with_text(view.detail.as_str())
                .with_css("margin: 0px; font-size: 12px; color: #6b7280;"),
        );
    let status = Dom::create_p_with_text(view.status.as_str())
        .with_css(format!("margin: 0px 10px; font-weight: bold; font-size: 12px; color: {};", view.color).as_str());
    let run = Button::create("Run")
        .with_on_click(
            RefAny::new(RowClick {
                app: data.clone(),
                idx,
            }),
            on_run_row,
        )
        .dom();
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; padding: 8px 0px;")
        .with_child(text)
        .with_child(status)
        .with_child(run)
}

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let (views, summary, widget) = {
        let Some(state) = data.downcast_ref::<SelfTest>() else {
            return Dom::create_body();
        };
        (state.views(), state.summary(), state.active_widget())
    };
    let header = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; padding: 10px 12px;")
        .with_child(
            Dom::create_div()
                .with_css("flex-grow: 1;")
                .with_child(Dom::create_p_with_text("AzSelfTest").with_css("margin: 0px; font-size: 20px; font-weight: bold;"))
                .with_child(
                    Dom::create_p_with_text(summary.as_str())
                        .with_css("margin: 0px; font-size: 12px; color: #6b7280;"),
                ),
        )
        .with_child(Button::create("Run all").with_on_click(data.clone(), on_run_all).dom());
    let mut list = Dom::create_div()
        .with_css("display: flex; flex-direction: column; overflow-y: auto; flex-grow: 1; min-height: 0px; padding: 0px 12px;");
    for (idx, view) in views.iter().enumerate() {
        list = list.with_child(row_dom(&data, idx, view));
    }
    let mut body = Dom::create_body()
        .with_css("margin: 0px; font-family: system:ui; color: system:text; background-color: system:background; display: flex; flex-direction: column; height: 100%;")
        .with_child(header)
        .with_child(list);
    if let Some((widget, elapsed_ms)) = widget {
        body = body.with_child(widget(&data, elapsed_ms));
    }
    body
}

/// Runs the self-test: opens the window and blocks until it closes (desktop and iOS), returning the exit code; on
/// Android the window options are stashed and the call returns at once (the activity runs the loop).
pub fn start() -> i32 {
    let unattended = unattended_requested();
    let mut data = RefAny::new(SelfTest::new(unattended));
    // Every Az app starts in flora.
    let app = App::create(data.clone(), AppConfig::create().with_theme("flora"));
    let mut window = WindowCreateOptions::create(layout);
    window.window_state.title = "AzSelfTest".into();
    window.create_callback = Some(Callback::create(on_created)).into();
    app.run(window);
    data.downcast_ref::<SelfTest>().map_or(0, |state| state.exit_code)
}

// Android has no `main()`: the OS loads this cdylib and calls libazul's `android_main`; `start()` has to run BEFORE
// `ANativeActivity_onCreate`, from a load-time constructor (guide: deploying/mobile).
#[cfg(target_os = "android")]
#[ctor::ctor]
fn android_ctor() {
    let _ = start();
}
