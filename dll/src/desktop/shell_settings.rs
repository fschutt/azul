//! What the environment asks of the shells and the app start, and the one place of the dll that
//! names those variables (the groups D1 of the env pass).
//!
//! Nothing here reads the process: [`ShellSettings::from_lookup`] takes a `&dyn EnvSource`. The
//! app start builds it once from [`crate::env::ProcessEnv`], inside [`super::app_settings::AppSettings`],
//! and every window reads a plain field of it through its `AppShared`
//! (`window.common.app.settings().shell`). [`ShellSettings::default`] is the behaviour of an
//! EMPTY environment, which is what a test, a tool or an embedding host that never asked for
//! anything gets.

use azul_css::env::EnvSource;

/// `AZ_BACKEND`: the windowing or render backend, as typed.
const BACKEND: &str = "AZ_BACKEND";
/// `AZ_WINDOW`: the windowing system (`x11`, `wayland`, `auto`), as typed.
const WINDOW: &str = "AZ_WINDOW";
/// `AZ_E2E`: the script (a file or a directory) of an end-to-end run.
const E2E: &str = "AZ_E2E";
/// `AZ_LOG_STDERR`: `0`, `off`, `false`, `no` or `none` switch the stderr copy of the log off.
const LOG_STDERR: &str = "AZ_LOG_STDERR";
/// `AZ_LOG_FILE`: the file the log is written to.
const LOG_FILE: &str = "AZ_LOG_FILE";
/// `NO_COLOR` (https://no-color.org/): no colour in the built-in stderr logger.
const NO_COLOR: &str = "NO_COLOR";
/// `AZ_E2E_RUNTIME`: the JavaScript runtime of the embedded web end-to-end runner.
const E2E_RUNTIME: &str = "AZ_E2E_RUNTIME";
/// `AZ_E2E_TEST`: the scenario file of a scripted resize / tick test run.
const E2E_TEST: &str = "AZ_E2E_TEST";
/// `AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER`: exit 0 after the first rendered frame.
const EXIT_AFTER_FIRST_FRAME: &str = "AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER";
/// `AZ_HEADLESS_SNAPSHOT_PATH`: write the first rendered frame of a headless run as a PNG.
const HEADLESS_SNAPSHOT_PATH: &str = "AZ_HEADLESS_SNAPSHOT_PATH";
/// `AZ_HIT_DEBUG`: trace the hit tests and the focus-out button clears.
const HIT_DEBUG: &str = "AZ_HIT_DEBUG";
/// `AZ_FOCUS_TRACE`: one line per focus or popup decision.
const FOCUS_TRACE: &str = "AZ_FOCUS_TRACE";
/// `AZ_RECONCILE_DEBUG`: name the nodes a reconcile could not match.
const RECONCILE_DEBUG: &str = "AZ_RECONCILE_DEBUG";
/// `AZ_LAYOUT_TRACE`: with the debug server, a line per node of a layout pass.
const LAYOUT_TRACE: &str = "AZ_LAYOUT_TRACE";
/// `AZ_VALIDATE`: the window-state validation gate (always on in debug builds).
const VALIDATE: &str = "AZ_VALIDATE";
/// `AZ_NATIVE_BACKBUFFER`: `0` selects the legacy pixmap-and-copy present.
const NATIVE_BACKBUFFER: &str = "AZ_NATIVE_BACKBUFFER";
/// `AZ_DUMP_PRESENT`: report whether the Android present is partial or full.
const DUMP_PRESENT: &str = "AZ_DUMP_PRESENT";
/// `AZ_PRESENT`: the variable `adb shell setprop debug.az.present 1` is on Android (the property
/// is older than the variable's name): set to anything that is not empty, `0` or `false`, it asks
/// for [`DUMP_PRESENT`] too.
const PRESENT: &str = "AZ_PRESENT";
/// `AZ_PACE_TRACE`: the frame-pacing timeline of the macOS shell.
const PACE_TRACE: &str = "AZ_PACE_TRACE";
/// `AZ_CPU_PRESENT`: how the macOS CPU view presents (`bitmap`, `twice`, `layer`).
const CPU_PRESENT: &str = "AZ_CPU_PRESENT";
/// `AZ_KEEP_CACHES_MINIMIZED`: a window nobody sees keeps its caches.
const KEEP_CACHES_MINIMIZED: &str = "AZ_KEEP_CACHES_MINIMIZED";
/// `AZ_HALO_RELEASE`: `lists` keeps the halo pictures the display lists paint.
const HALO_RELEASE: &str = "AZ_HALO_RELEASE";
/// `AZ_A11Y_EAGER`: the macOS accessibility tree is built after every layout.
const A11Y_EAGER: &str = "AZ_A11Y_EAGER";
/// `AZ_CURSOR_SET_ALWAYS`: the macOS shell sets the cursor on every call.
const CURSOR_SET_ALWAYS: &str = "AZ_CURSOR_SET_ALWAYS";
/// `AZ_TELEMETRY_CHANNEL`: the release channel label of the telemetry meta.
const TELEMETRY_CHANNEL: &str = "AZ_TELEMETRY_CHANNEL";
/// `AZ_DUMP_SYSTEM_STYLE`: print what the desktop detection found.
const DUMP_SYSTEM_STYLE: &str = "AZ_DUMP_SYSTEM_STYLE";

/// The channel the telemetry reports when the environment names none.
const DEFAULT_TELEMETRY_CHANNEL: &str = "default";

/// A knob that does nothing in a build without the cargo feature that implements it: the names
/// of the ones that are set are kept ([`ShellSettings::inert_knobs`]) so the start can say so.
struct GatedKnob {
    /// The variable.
    name: &'static str,
    /// Whether this build has the feature that implements it.
    compiled: bool,
    /// The feature.
    feature: &'static str,
    /// What to build to get it back.
    how: &'static str,
}

/// The knobs that are FULLY inert without their feature (a knob that still does part of its job
/// gets a targeted message at its own read site). `cfg!`, not a `#[cfg]` on the list, so every
/// row is evaluated in every configuration and the list cannot rot behind the gate it describes.
fn gated_knobs() -> [GatedKnob; 6] {
    let scripting = cfg!(any(feature = "debug-server", feature = "e2e-scripting"));
    [
        GatedKnob {
            name: E2E,
            compiled: scripting,
            feature: "debug-server",
            how: "cargo build -p azul-dll --features build-dll,debug-server",
        },
        GatedKnob {
            name: "AZ_DEBUG",
            compiled: scripting,
            feature: "debug-server",
            how: "cargo build -p azul-dll --features build-dll,debug-server",
        },
        // The deterministic resize/tick scenario runner (common/e2e_test.rs). Without `e2e-test`
        // BOTH run.rs hooks are compiled out and the whole module does not exist: the app just
        // opens its normal window.
        GatedKnob {
            name: E2E_TEST,
            compiled: cfg!(feature = "e2e-test"),
            feature: "e2e-test",
            how: "cargo build -p azul-dll --features build-dll,e2e-test",
        },
        // First-frame PNG snapshot (headless/mod.rs). Read only under `cpurender`; without it
        // the run looks normal and no file appears.
        GatedKnob {
            name: HEADLESS_SNAPSHOT_PATH,
            compiled: cfg!(feature = "cpurender"),
            feature: "cpurender",
            how: "cargo build -p azul-dll --features build-dll,cpurender",
        },
        // Per-frame PNG dump out of CpuBackend::render_frame: the same gate.
        GatedKnob {
            name: "AZ_DUMP_FRAME_DIR",
            compiled: cfg!(feature = "cpurender"),
            feature: "cpurender",
            how: "cargo build -p azul-dll --features build-dll,cpurender",
        },
        // Layout profiling (azul-layout's `probe` feature, normally forced on by the DLL's
        // _internal_deps). Without it `AZ_PROFILE=heap,...` silently reports heap=0 / writes no
        // JSONL at all (layout/src/probe.rs no-op stubs); only the `cpu` mode prints its own
        // "probe unavailable" line. `Probe::enabled()` is the compile-time truth for whether the
        // real imp was built (probe on, not wasm, not web_lift): the dll has no `probe` feature
        // of its own, so `cfg!` can't express this row.
        GatedKnob {
            name: "AZ_PROFILE",
            compiled: azul_layout::probe::Probe::enabled(),
            feature: "probe",
            how: "cargo build -p azul-dll --features build-dll,azul-layout/probe",
        },
    ]
}

/// A knob set in the environment that this build cannot honour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InertKnob {
    /// The variable.
    pub name: &'static str,
    /// The cargo feature that implements it.
    pub feature: &'static str,
    /// What to build to get it back.
    pub how: &'static str,
}

/// Does this `AZ_VALIDATE` value turn the validation gate on?
///
/// Unset is off. An empty or explicitly-negative value is off, so `AZ_VALIDATE=` and
/// `AZ_VALIDATE=0` both mean what they look like. Anything else is on, because a user who typed
/// the variable at all meant to enable it.
#[must_use]
pub fn validation_from_value(value: Option<&str>) -> bool {
    match value {
        None => false,
        Some(v) => {
            let v = v.trim().to_ascii_lowercase();
            !matches!(v.as_str(), "" | "0" | "off" | "false" | "no" | "none")
        }
    }
}

/// The settings of the app start and of the platform shells.
// Each flag is its own switch of a debugging session: a set of enums would only rename them.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellSettings {
    /// `AZ_BACKEND` as typed: the render backend (`cpu`, `gpu`, `auto`, `headless`, `web://..`)
    /// and, for the legacy spelling, the windowing system (`x11`, `wayland`). `None` when it is
    /// unset or not UTF-8.
    pub backend: Option<String>,
    /// `AZ_WINDOW` as typed: the windowing system (`x11`, `wayland`, `auto`). `None` when it is
    /// unset or not UTF-8.
    pub window: Option<String>,
    /// `AZ_E2E`: the script (a file, or a directory of them) of an end-to-end run; `None` when it
    /// is unset, empty or not UTF-8.
    pub e2e_script: Option<String>,
    /// `AZ_E2E` is set at all (empty too): a script drives this run, so nothing may wait on a
    /// dialog a person would have to dismiss (the fatal-panic modal).
    pub e2e_run: bool,
    /// `AZ_E2E_TEST`: the scenario file of a scripted resize / tick test run; `None` when it is
    /// unset, empty or not UTF-8.
    pub e2e_scenario: Option<String>,
    /// `AZ_E2E_RUNTIME`: the JavaScript runtime (a binary) the embedded web end-to-end runner
    /// runs its harness under; `None` when it is unset, empty or not UTF-8 (the first of node,
    /// bun and deno that answers `--version` is used).
    pub e2e_runtime: Option<String>,
    /// `AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER` is set (to a UTF-8 value): the shell exits with 0
    /// after the first rendered frame.
    pub exit_after_first_frame: bool,
    /// `AZ_HEADLESS_SNAPSHOT_PATH`: the PNG the first rendered frame of a headless run is written
    /// to; `None` when it is unset or not UTF-8.
    pub headless_snapshot_path: Option<String>,
    /// `NO_COLOR` is set (to anything): the built-in stderr logger does not colour its lines.
    pub no_color: bool,
    /// `AZ_LOG_STDERR` says `0`, `off`, `false`, `no` or `none` (any case, blanks around): the
    /// log gate prints nothing to stderr itself and a debug server being on does not bring the
    /// copy back.
    pub log_stderr_off: bool,
    /// `AZ_LOG_FILE`: the file the log gate writes every record to; `None` when it is unset or
    /// not UTF-8.
    pub log_file: Option<String>,
    /// `AZ_HIT_DEBUG` is set (to a UTF-8 value): trace every hit test.
    pub hit_debug: bool,
    /// `AZ_FOCUS_TRACE` is set (to a UTF-8 value): one line per focus or popup decision.
    pub focus_trace: bool,
    /// `AZ_RECONCILE_DEBUG` is set (to anything): name the nodes a reconcile could not match.
    pub reconcile_debug: bool,
    /// `AZ_LAYOUT_TRACE` is set, not empty and not `0`: with the debug server, the layout pass
    /// collects a line per node.
    pub layout_trace: bool,
    /// `AZ_VALIDATE` turns the window-state validation gate on ([`validation_from_value`]); a
    /// debug build validates whatever it says ([`ShellSettings::validation_enabled`]).
    pub validate: bool,
    /// The CPU present goes through the native back buffer. On unless `AZ_NATIVE_BACKBUFFER=0`.
    pub native_backbuffer: bool,
    /// `AZ_DUMP_PRESENT` is set (to anything), or `AZ_PRESENT` (the Android property
    /// `debug.az.present`) is neither empty, `0` nor `false`: the Android shell reports whether a
    /// present is partial or full.
    pub dump_present: bool,
    /// `AZ_PACE_TRACE` is set (to anything): the macOS shell prints the timeline of its
    /// frame-pacing pipeline (display-link fire, dirty-marking, `drawRect:`, redraw requests).
    pub pace_trace: bool,
    /// `AZ_CPU_PRESENT` as typed (`bitmap,twice,layer`, parsed by the macOS shell): how the CPU
    /// view presents. `None` when it is unset or not UTF-8.
    pub cpu_present: Option<String>,
    /// `AZ_KEEP_CACHES_MINIMIZED` is set and not `0`: a window nobody sees (the Dock, Cmd-H,
    /// memory pressure) keeps its caches.
    pub keep_caches_minimized: bool,
    /// A window nobody sees gives back the text halo pictures its display lists paint too. On
    /// unless `AZ_HALO_RELEASE=lists`.
    pub halo_release_full: bool,
    /// `AZ_A11Y_EAGER` as typed: when it is set and not `0` the macOS accessibility tree is built
    /// after every layout. `None` when it is unset or not UTF-8.
    pub a11y_eager: Option<String>,
    /// `AZ_CURSOR_SET_ALWAYS` is set and not `0`: the macOS shell sends `-[NSCursor set]` on
    /// every call, as it did before audit A6-12.
    pub cursor_set_always: bool,
    /// `AZ_TELEMETRY_CHANNEL`: the release channel label of the telemetry meta; `default` when
    /// the variable is unset.
    pub telemetry_channel: String,
    /// `AZ_DUMP_SYSTEM_STYLE=1`: print what the desktop detection found.
    pub dump_system_style: bool,
    /// The names of the knobs of [`gated_knobs`] that are set to something that is not empty.
    gated_knobs_set: Vec<&'static str>,
}

impl ShellSettings {
    /// What the environment `env` asks for; an empty one asks for nothing.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        let not_empty = |name: &str| env.var(name).filter(|value| !value.is_empty());
        Self {
            backend: env.var(BACKEND),
            window: env.var(WINDOW),
            e2e_script: not_empty(E2E),
            e2e_run: env.is_set(E2E),
            e2e_scenario: not_empty(E2E_TEST),
            e2e_runtime: not_empty(E2E_RUNTIME),
            exit_after_first_frame: env.var(EXIT_AFTER_FIRST_FRAME).is_some(),
            headless_snapshot_path: env.var(HEADLESS_SNAPSHOT_PATH),
            no_color: env.is_set(NO_COLOR),
            log_stderr_off: env.var(LOG_STDERR).is_some_and(|v| {
                matches!(
                    v.trim().to_ascii_lowercase().as_str(),
                    "0" | "off" | "false" | "no" | "none"
                )
            }),
            log_file: env.var(LOG_FILE),
            hit_debug: env.var(HIT_DEBUG).is_some(),
            focus_trace: env.var(FOCUS_TRACE).is_some(),
            reconcile_debug: env.is_set(RECONCILE_DEBUG),
            layout_trace: env
                .var_os(LAYOUT_TRACE)
                .is_some_and(|v| !v.is_empty() && v != "0"),
            validate: validation_from_value(env.var(VALIDATE).as_deref()),
            native_backbuffer: !env.var(NATIVE_BACKBUFFER).is_some_and(|v| v == "0"),
            dump_present: env.is_set(DUMP_PRESENT)
                || env
                    .var(PRESENT)
                    .is_some_and(|v| !v.is_empty() && v != "0" && v != "false"),
            pace_trace: env.is_set(PACE_TRACE),
            cpu_present: env.var(CPU_PRESENT),
            keep_caches_minimized: env.var_os(KEEP_CACHES_MINIMIZED).is_some_and(|v| v != "0"),
            halo_release_full: !env.var(HALO_RELEASE).is_some_and(|v| v == "lists"),
            a11y_eager: env.var(A11Y_EAGER),
            cursor_set_always: env.var_os(CURSOR_SET_ALWAYS).is_some_and(|v| v != "0"),
            telemetry_channel: env
                .var(TELEMETRY_CHANNEL)
                .unwrap_or_else(|| DEFAULT_TELEMETRY_CHANNEL.to_owned()),
            dump_system_style: env.var(DUMP_SYSTEM_STYLE).as_deref() == Some("1"),
            gated_knobs_set: gated_knobs()
                .iter()
                .filter(|knob| not_empty(knob.name).is_some())
                .map(|knob| knob.name)
                .collect(),
        }
    }

    /// Whether the window-state validation gate is on: always in a debug build, in a release
    /// build only when `AZ_VALIDATE` asked for it.
    #[must_use]
    pub const fn validation_enabled(&self) -> bool {
        cfg!(debug_assertions) || self.validate
    }

    /// Nobody is at the screen to answer a dialog: a script drives the run (`AZ_E2E`), or the
    /// backend is the headless one (`AZ_BACKEND=headless`: no window exists, a harness talks to the
    /// app through the debug server). Nothing may wait on a dialog then - the fatal-panic modal
    /// among them, which a headless run showed (osascript) and waited on for good, so a panic in
    /// the layout pass froze the app instead of ending it with its message.
    #[must_use]
    pub fn unattended(&self) -> bool {
        self.e2e_run
    }

    /// The knobs that are set in the environment but that this build cannot honour (their cargo
    /// feature is compiled out): the start says so, because a knob that does nothing in silence
    /// looks exactly like a feature that ran and found nothing to do.
    #[must_use]
    pub fn inert_knobs(&self) -> Vec<InertKnob> {
        gated_knobs()
            .into_iter()
            .filter(|knob| !knob.compiled && self.gated_knobs_set.contains(&knob.name))
            .map(|knob| InertKnob {
                name: knob.name,
                feature: knob.feature,
                how: knob.how,
            })
            .collect()
    }
}

impl Default for ShellSettings {
    /// The empty environment's.
    fn default() -> Self {
        Self::from_lookup(&azul_css::env::NoEnv)
    }
}

#[cfg(test)]
#[path = "shell_settings_test.rs"]
mod tests;
