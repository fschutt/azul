//! azul-self-test — unattended platform-API smoke test.
//!
//! See `scripts/PLATFORM_DEBUG_PLAN.md` (Phase 3). The binary runs a fixed probe
//! sequence over the *public* `azul::` API, prints human-readable progress to
//! stdout, mirrors a structured trace to a log file (and, because it installs a
//! `log` logger, captures azul's own `plog_*!` platform-layer traces too — that
//! is the whole point: it makes the Phase 1 logging visible per-OS), and exits
//! with a non-zero code if a *required* probe hard-fails. A device merely being
//! **unavailable is NOT a failure** — that is the contract the desktop builds
//! must honour (item c): no panics when a feature is absent.
//!
//! Two classes of probe:
//!   * **standalone** (no event loop) — run first, in [`start`]: UDP loopback,
//!     AudioSink open. These are the deterministic ones used for the exit code.
//!   * **event-loop** (read via `CallbackInfo` inside the running App) — motion
//!     sensors, gamepad, geolocation, plus the MicrophoneWidget mic-level dots
//!     and a CameraWidget capture worker. Driven by a probe Timer that closes
//!     the window after `RUN_SECS`, ending `App::run`.
//!
//! Set `AZUL_SELFTEST_NO_WINDOW=1` to run only the standalone probes (for a
//! truly headless CI box with no display); set `AZUL_SELFTEST_LOG=<path>` to
//! override the log-file location.

use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use azul::dom::OnAudioFrameCallback;
use azul::misc::{AudioConfig, CameraConfig, SensorKind};
use azul::option::OptionRefAny;
use azul::prelude::*;
use azul::task::TerminateTimer;
use azul::vec::U8Vec;
use azul::widgets::{AudioFrame, CameraWidget, MicrophoneWidget};
use azul::window::{AudioSink, PlatformCapability};

/// How long the windowed (event-loop) probe phase runs before auto-closing.
const RUN_SECS: u64 = 4;

/// Process exit code, accumulated across probes. `0` = OK (incl. "unavailable"),
/// non-zero = a required probe hard-failed.
static EXIT_CODE: AtomicI32 = AtomicI32::new(0);
/// Count of mic frames seen (so the summary can report capture liveness).
static MIC_FRAMES: AtomicUsize = AtomicUsize::new(0);

fn fail(code: i32) {
    // First failure wins; keep it sticky.
    let _ = EXIT_CODE.compare_exchange(0, code, Ordering::SeqCst, Ordering::SeqCst);
}

/// The process exit code after [`start`] returns (desktop only; on mobile the
/// app keeps running and the log file is the artifact).
pub fn exit_code() -> i32 {
    EXIT_CODE.load(Ordering::SeqCst)
}

// ───────────────────────── logging ─────────────────────────

/// A tiny dual logger: every record goes to stderr *and* (best-effort) to the
/// log file. No external logging crate — keeps the dependency surface minimal.
struct DualLogger {
    file: Mutex<Option<std::fs::File>>,
}

impl log::Log for DualLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        let line = format!("[{:5}] {}", record.level(), record.args());
        eprintln!("{}", line);
        if let Ok(mut guard) = self.file.lock() {
            if let Some(f) = guard.as_mut() {
                use std::io::Write;
                let _ = writeln!(f, "{}", line);
                let _ = f.flush();
            }
        }
    }
    fn flush(&self) {}
}

fn log_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("AZUL_SELFTEST_LOG") {
        return std::path::PathBuf::from(p);
    }
    std::env::temp_dir().join("azul-self-test.log")
}

fn init_logger() {
    let path = log_path();
    let file = std::fs::File::create(&path).ok();
    let opened = file.is_some();
    let logger = Box::new(DualLogger {
        file: Mutex::new(file),
    });
    // If a logger is already installed (e.g. the host app set one), don't panic.
    if log::set_boxed_logger(logger).is_ok() {
        log::set_max_level(log::LevelFilter::Trace);
    }
    log::info!("════════ azul-self-test ════════");
    if opened {
        log::info!("log file: {}", path.display());
    } else {
        log::warn!("could not open log file {} — stderr only", path.display());
    }
    log::info!(
        "platform: os={} arch={}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
}

// ─────────────────────── capabilities probe ───────────────────────

/// Non-destructive capability dump via the `AzCapability_*` probe API — answers
/// "is each feature usable on this target, and which backend?" WITHOUT touching
/// hardware (so it never crashes and runs even headless). This is the upfront
/// capabilities table; the operational probes below then exercise the live path.
fn probe_capabilities() {
    log::info!("── capabilities (probe — non-destructive) ──");
    let rows = [
        ("camera", PlatformCapability::camera()),
        ("microphone", PlatformCapability::microphone()),
        ("audio-out", PlatformCapability::audio_output()),
        ("sensors", PlatformCapability::sensors()),
        ("gamepad", PlatformCapability::gamepad()),
        ("geolocation", PlatformCapability::geolocation()),
        ("keyring", PlatformCapability::keyring()),
        ("biometric", PlatformCapability::biometric()),
        ("video-codec", PlatformCapability::video_codec()),
    ];
    for (name, c) in rows {
        let status = if c.available { "AVAILABLE  " } else { "unavailable" };
        let reason = c.reason.as_str();
        if reason.is_empty() {
            log::info!("[cap] {:<12} {} via {}", name, status, c.backend.as_str());
        } else {
            log::info!(
                "[cap] {:<12} {} via {} ({})",
                name,
                status,
                c.backend.as_str(),
                reason
            );
        }
    }
}

// ─────────────────────── standalone probes ───────────────────────


/// AudioSink open probe — opening the default output device. Unavailable (no
/// device / CI box) is NOT a failure; we only report.
fn probe_audio_sink() {
    log::info!("── probe: AudioSink open ──");
    let sink = AudioSink::open(AudioConfig {
        sample_rate: 48_000,
        channels: 1,
    });
    if sink.is_open() {
        log::info!("[audio] sink opened (48kHz mono) — output device available");
    } else {
        log::info!("[audio] sink NOT open — no output device (unavailable, not a failure)");
    }
}


// ─────────────────────── event-loop probes ───────────────────────

/// Per-window state for the device-probe phase.
struct ProbeState {
    started: Instant,
    /// Whether we have seen at least one reading from each sensor kind.
    seen_accel: bool,
    seen_gyro: bool,
    seen_mag: bool,
    seen_gamepad: bool,
    seen_location: bool,
    ticks: usize,
    done: bool,
}

impl ProbeState {
    fn new() -> Self {
        ProbeState {
            started: Instant::now(),
            seen_accel: false,
            seen_gyro: false,
            seen_mag: false,
            seen_gamepad: false,
            seen_location: false,
            ticks: 0,
            done: false,
        }
    }
}

/// Mic `on_frame`: compute the RMS of the captured chunk and print it as a row
/// of dots/blocks to stdout so audio is *visible* without a UI (item d).
extern "C" fn on_mic_frame(_data: RefAny, _info: CallbackInfo, frame: AudioFrame) -> Update {
    let samples = frame.samples.as_ref();
    let rms = if samples.is_empty() {
        0.0f32
    } else {
        let sum: f32 = samples.iter().map(|s| s * s).sum();
        (sum / samples.len() as f32).sqrt()
    };
    // Map RMS (0.0..~1.0) to a 0..40 bar. Speech peaks well below 1.0, so scale.
    let level = ((rms * 8.0).min(1.0) * 40.0) as usize;
    let bar: String = "#".repeat(level) + &".".repeat(40 - level);
    let n = MIC_FRAMES.fetch_add(1, Ordering::SeqCst) + 1;
    println!("[mic] {:5} |{}| rms={:.4}", n, bar, rms);
    if n == 1 {
        log::info!("[mic] first frame: {} samples @ {}Hz x{}ch", samples.len(), frame.sample_rate, frame.channels);
    }
    Update::DoNothing
}

/// Probe Timer tick — poll the event-loop device APIs, then close the window
/// once `RUN_SECS` have elapsed.
extern "C" fn probe_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let mut terminate = TerminateTimer::Continue;

    if let Some(mut s) = data.downcast_mut::<ProbeState>() {
        s.ticks += 1;

        if !s.seen_accel
            && info
                .callback_info
                .get_sensor_reading(SensorKind::Accelerometer)
                .is_some()
        {
            s.seen_accel = true;
            log::info!("[sensors] accelerometer: reading received");
        }
        if !s.seen_gyro
            && info
                .callback_info
                .get_sensor_reading(SensorKind::Gyroscope)
                .is_some()
        {
            s.seen_gyro = true;
            log::info!("[sensors] gyroscope: reading received");
        }
        if !s.seen_mag
            && info
                .callback_info
                .get_sensor_reading(SensorKind::Magnetometer)
                .is_some()
        {
            s.seen_mag = true;
            log::info!("[sensors] magnetometer: reading received");
        }
        if !s.seen_gamepad && info.callback_info.get_primary_gamepad().is_some() {
            s.seen_gamepad = true;
            log::info!("[gamepad] primary gamepad connected");
        }
        if !s.seen_location && info.callback_info.get_location_fix().is_some() {
            s.seen_location = true;
            log::info!("[geolocation] location fix received");
        }

        if !s.done && s.started.elapsed().as_secs() >= RUN_SECS {
            s.done = true;
            print_summary(&s);
            terminate = TerminateTimer::Terminate;
            info.callback_info.close_window();
        }
    }

    TimerCallbackReturn {
        should_update: Update::DoNothing,
        should_terminate: terminate,
    }
}

fn print_summary(s: &ProbeState) {
    let avail = |b: bool| if b { "available" } else { "unavailable" };
    log::info!("──────── summary ────────");
    log::info!("ticks:           {}", s.ticks);
    log::info!("accelerometer:   {}", avail(s.seen_accel));
    log::info!("gyroscope:       {}", avail(s.seen_gyro));
    log::info!("magnetometer:    {}", avail(s.seen_mag));
    log::info!("gamepad:         {}", avail(s.seen_gamepad));
    log::info!("geolocation:     {}", avail(s.seen_location));
    log::info!("mic frames seen: {}", MIC_FRAMES.load(Ordering::SeqCst));
    log::info!("camera:          widget created (preview-only; frame inspection needs UI)");
    log::info!("exit code:       {}", exit_code());
    log::info!("─────────────────────────");
}

/// Window-create: install the probe Timer.
extern "C" fn startup(data: RefAny, mut info: CallbackInfo) -> Update {
    info.add_timer(
        TimerId::unique(),
        Timer::create(
            data.clone(),
            TimerCallback {
                cb: probe_tick,
                ctx: OptionRefAny::None,
            },
            info.get_system_time_fn(),
        ),
    );
    Update::DoNothing
}

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let elapsed = data
        .downcast_ref::<ProbeState>()
        .map(|s| s.started.elapsed().as_secs())
        .unwrap_or(0);
    let status = format!(
        "azul-self-test — probing platform APIs ({}s/{}s). Mic dots + traces on stdout/log.",
        elapsed, RUN_SECS
    );

    // The MicrophoneWidget starts a capture worker; its on_frame prints mic-level
    // dots. The CameraWidget starts the camera capture worker (preview).
    let mic = MicrophoneWidget::create(AudioConfig {
        sample_rate: 48_000,
        channels: 1,
    })
    .with_on_frame(
        data.clone(),
        OnAudioFrameCallback {
            cb: on_mic_frame,
            callable: OptionRefAny::None,
        },
    )
    .dom();

    let camera = CameraWidget::create(CameraConfig::default()).dom();

    Dom::create_body()
        .with_child(Dom::create_text("azul-self-test"))
        .with_child(Dom::create_text(status.as_str()))
        .with_child(mic)
        .with_child(camera)
}

// ───────────────────────── entry point ─────────────────────────

/// Print a self-describing intro to stdout (NOT the log) so a human — or a fresh
/// Claude instance asked to "run the self-test and debug the failures" — knows
/// what this is, how it behaves, and how to read the result, without prior
/// context. Lines are plain (no `[INFO]` prefix) so they stand out.
fn banner() {
    let no_window = std::env::var("AZUL_SELFTEST_NO_WINDOW").is_ok();
    let interactive = std::env::var("AZUL_SELFTEST_INTERACTIVE").is_ok();
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║  azul-self-test — platform device-API smoke test                   ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!("WHAT THIS IS: exercises every azul platform API (UDP, audio, camera,");
    println!("  microphone, motion sensors, gamepad, geolocation) on THIS machine,");
    println!("  printing each result and mirroring a trace — including azul's own");
    println!("  [camera]/[udp]/[gamepad]/... backend logs — to a file. Use it to");
    println!("  compare behaviour across OSes and pinpoint platform bugs.");
    println!();
    println!("HOW TO READ RESULTS:");
    println!("  PASS    — the API worked.");
    println!("  UNAVAIL — no device / not supported on this target. NOT a failure");
    println!("            (e.g. no camera on a server, no sensors on a desktop).");
    println!("  FAIL    — a required API is broken. This is a real bug; the process");
    println!("            exits non-zero. (Only UDP loopback is currently required.)");
    println!();
    println!("INTERACTION: runs UNATTENDED and exits on its own (~{}s for the device", RUN_SECS);
    println!("  phase). No clicks needed.");
    if !no_window {
        println!("  • A window opens for ~{}s for the live device probes. To see real", RUN_SECS);
        println!("    data: move a connected GAMEPAD, SPEAK into the mic (you'll see a");
        println!("    dot-bar rise), point the CAMERA at something. All optional — it");
        println!("    auto-closes and nothing blocks if no device is present.");
    } else {
        println!("  • AZUL_SELFTEST_NO_WINDOW set: only the headless probes run (UDP,");
        println!("    audio-sink). Device probes that need the event loop are skipped.");
    }
    if interactive {
        println!("  • AZUL_SELFTEST_INTERACTIVE set: keyring/biometric probes will run");
        println!("    and MAY PROMPT (fingerprint / PIN). Respond when asked.");
    } else {
        println!("  • Keyring/biometric probes are SKIPPED (they prompt). Set");
        println!("    AZUL_SELFTEST_INTERACTIVE=1 to include them.");
    }
    println!("LOG FILE: {}  (override with AZUL_SELFTEST_LOG=<path>)", log_path().display());
    println!("────────────────────────────────────────────────────────────────────");
    println!();
}

/// Run the self-test. Desktop: blocks through the windowed probe phase, then
/// returns (caller exits with [`exit_code`]). Android: `App::run` stashes the
/// window options and returns; the device probes run later under `android_main`.
pub fn start() {
    banner();
    init_logger();

    // 0) Capabilities table (non-destructive probe API).
    probe_capabilities();

    // 1) Standalone probes (no event loop) — the deterministic, exit-code ones.
    probe_audio_sink();

    // 2) Headless escape hatch: skip the window (no display on this box).
    if std::env::var("AZUL_SELFTEST_NO_WINDOW").is_ok() {
        log::info!("AZUL_SELFTEST_NO_WINDOW set — skipping windowed device probes");
        finish();
        return;
    }

    // 3) Windowed device probes via the App event loop.
    log::info!("── opening window for live device probes ({}s) ──", RUN_SECS);
    println!(">>> A window is opening for ~{}s — move a gamepad / speak / show the camera to see live data (optional). It closes automatically.", RUN_SECS);
    let data = RefAny::new(ProbeState::new());
    let config = AppConfig::create();
    let app = App::create(data, config);
    let mut window = WindowCreateOptions::create(layout);
    window.create_callback = Some(Callback::create(startup)).into();
    app.run(window);

    finish();
}

/// Final stdout verdict — written so an agent can act on it directly.
fn finish() {
    let code = exit_code();
    println!();
    println!("────────────────────────────────────────────────────────────────────");
    if code == 0 {
        println!("RESULT: PASS (exit 0). All required probes passed; UNAVAIL items are");
        println!("  expected on machines without that device.");
    } else {
        println!("RESULT: FAIL (exit {}). A required probe failed — see the FAIL line(s)", code);
        println!("  above; that is a real platform bug to debug.");
    }
    println!("Full trace (incl. azul backend logs): {}", log_path().display());
    println!("To compare against another OS: run this there and diff the two logs.");
    log::info!("self-test complete (exit code {})", code);
}

// Android has no `main()`: the OS loads this cdylib and calls libazul's
// `android_main`. `start()` must run BEFORE `ANativeActivity_onCreate`, i.e.
// from a load-time constructor. See guide/mobile.md.
#[cfg(target_os = "android")]
#[ctor::ctor]
fn azul_android_init() {
    start();
}
