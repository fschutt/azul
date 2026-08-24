//! Application lifecycle entry point.
//!
//! This module defines [`App`] and [`AppInternal`], which together manage
//! the top-level application state, font discovery, and the transition into
//! the platform event loop (`shell2::run`).

use alloc::sync::Arc;

use azul_core::{
    refany::RefAny,
    resources::AppConfig,
    window::MonitorVec,
};
use azul_layout::window_state::{WindowCreateOptions, WindowCreateOptionsVec};
use rust_fontconfig::FcFontCache;
use rust_fontconfig::registry::FcFontRegistry;

use crate::desktop::shell2::common::debug_server;

/// Wait (off the calling thread) for the font scan to finish, then write the
/// on-disk font manifest.
///
/// Persisting must not happen on the layout thread  -  the serialize + write is
/// real I/O  -  and it must not happen before the scan is complete: a manifest
/// written mid-scan would describe a PARTIAL font set, and the next launch
/// would load it, see `cache_loaded == true`, and lay out against a font
/// universe missing most of the system's families. A partial cache is strictly
/// worse than no cache, so an incomplete scan writes nothing at all.
#[cfg(all(not(miri), not(feature = "web")))]
fn spawn_font_cache_persist(registry: Arc<FcFontRegistry>) {
    use core::time::Duration;

    let spawned = std::thread::Builder::new()
        .name("azul-font-cache-persist".to_string())
        .spawn(move || {
            // Bounded poll rather than `wait_for_scout()`: that helper caps at
            // 5s and prints a warning on timeout, and we neither want the
            // warning nor a 5s ceiling on a cold machine.
            let deadline = std::time::Instant::now() + Duration::from_secs(60);
            while !registry.is_build_complete() {
                if std::time::Instant::now() >= deadline {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            let saved = registry.save_to_disk_cache().is_some();
            debug_server::log(
                debug_server::LogLevel::Info,
                debug_server::LogCategory::Resources,
                if saved {
                    "Persisted font metadata to disk cache"
                } else {
                    "Failed to persist font metadata to disk cache"
                },
                None,
            );
        });

    // A machine that cannot spawn one more thread is not a reason to fail
    // startup; it just means this launch does not leave a cache behind.
    let _ = spawned;
}

/// Primary public handle for creating and running an Azul application.
///
/// Wraps [`AppInternal`] in a `Box` and is the type used by all Rust examples.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct App {
    pub ptr: Box<AppInternal>,
    pub run_destructor: bool,
}

/// `run_destructor` and this `Drop` impl exist for FFI compatibility:
/// C callers may need to prevent the automatic drop of the inner `Box`
/// (same pattern as `RefAny` and other `#[repr(C)]` handles).
impl Drop for App {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

impl Default for App {
    fn default() -> Self {
        Self::create(RefAny::new(()), AppConfig::default())
    }
}

/// Set `SIGPIPE` to `SIG_IGN` exactly once per process.
///
/// A C/C++/Python host that `dlopen`s libazul never runs Rust's runtime, which
/// is what normally installs this. Without it the default `SIGPIPE` disposition
/// (`SIG_DFL` = terminate) kills the whole process on the first write to a
/// closed socket/pipe  -  e.g. the D-Bus theme probe in `discover_system_style`,
/// or a dropped Wayland/X11/debug-server connection. Idempotent, and harmless
/// for Rust hosts (whose runtime already ignores SIGPIPE).
#[cfg(unix)]
fn ignore_sigpipe_once() {
    use core::sync::atomic::{AtomicBool, Ordering};
    static DONE: AtomicBool = AtomicBool::new(false);
    if DONE.swap(true, Ordering::SeqCst) {
        return;
    }
    // SIGPIPE = 13 and SIG_IGN = 1 are ABI-stable across Linux/macOS/*BSD.
    // Declared inline (like `getuid` in system_style.rs) so this works whether
    // or not the optional `libc` feature is enabled.
    const SIGPIPE: i32 = 13;
    const SIG_IGN: usize = 1;
    extern "C" {
        fn signal(signum: i32, handler: usize) -> usize;
    }
    // SAFETY: installing SIG_IGN for SIGPIPE is async-signal-safe and has no
    // memory effects; the handler value 1 (SIG_IGN) is the documented constant.
    unsafe {
        signal(SIGPIPE, SIG_IGN);
    }
}

#[cfg(not(unix))]
fn ignore_sigpipe_once() {}

impl App {
    pub fn create(initial_data: RefAny, mut app_config: AppConfig) -> Self {
        // C/C++/Python hosts never run Rust's runtime, so install our own
        // SIGPIPE -> SIG_IGN before any socket/pipe I/O happens (see fn docs).
        ignore_sigpipe_once();

        // Initialize AZ_RECORD file logging before anything else
        debug_server::init_recording();

        // Install azul's built-in stderr logger (default ON; AZ_LOG=off to
        // silence). The lean `build-dll` library otherwise installs NO logger,
        // so every platform-layer trace is discarded and a failed startup looks
        // like a silent quit. Doing it here means it is live before any window,
        // event-loop or font work runs. No-op if the host already set a logger.
        #[cfg(feature = "logging")]
        crate::desktop::logging::init_default_logger();

        // Discover the real system style (replaces the hard-coded default from AppConfig::create)
        app_config.system_style = discover_system_style();

        // Apply the AppConfig system-animation overrides ON TOP of discovery:
        // the user's scroll physics (momentum / overscroll / wheel-vs-trackpad
        // curves) must survive the discovery overwrite above, and the tween
        // configuration becomes the app-global default LayoutWindows read.
        if let azul_css::props::style::scrollbar::OptionScrollPhysics::Some(p) =
            app_config.system_animations.scroll_physics
        {
            app_config.system_style.scroll_physics = p;
        }
        azul_layout::window::set_global_system_animations(
            app_config.system_animations.clone(),
        );

        // Set the icon resolver from the layout crate (the default resolver in core is a no-op)
        app_config.icon_provider.set_resolver(azul_layout::icon::default_icon_resolver);

        // Register embedded Material Icons if the feature is enabled. The
        // font bytes are embedded in the dll (downstream of codegen), not
        // in azul-layout, so we pass them in.
        if let Some(font_bytes) = crate::desktop::material_icons::get_material_icons_font_bytes() {
            azul_layout::icon::register_embedded_material_icons(
                &mut app_config.icon_provider,
                font_bytes,
            );
        }

        let app_internal = AppInternal::create(initial_data, app_config);
        let boxed = Box::new(app_internal);
        
        Self {
            ptr: boxed,
            run_destructor: true,
        }
    }

    pub fn add_window(&mut self, create_options: WindowCreateOptions) {
        self.ptr.windows.push(create_options);
    }

    pub fn get_monitors(&self) -> MonitorVec {
        crate::desktop::display::get_monitors()
    }

    /// Ask for a system-tray icon. Takes effect when `run()` starts.
    ///
    /// Deferred rather than immediate because a tray cannot exist this early:
    /// macOS needs `NSApplication`, and every backend needs the icon registry
    /// published so a `TrayIconSource::Named` spec can resolve.
    ///
    /// This is best-effort by design. On a desktop with no tray  -  a vanilla
    /// GNOME has no `StatusNotifierWatcher` at all  -  nothing appears and the
    /// app still runs; the failure is logged. Use
    /// [`crate::desktop::tray::TrayIcon::is_available`] beforehand if the app
    /// needs to know.
    pub fn set_tray(&mut self, tray: azul_core::tray::TrayIconData) {
        self.ptr.tray = Some(tray);
    }

    pub fn run(&self, root_window: WindowCreateOptions) {
        debug_server::log(
            debug_server::LogLevel::Info,
            debug_server::LogCategory::EventLoop,
            "Starting App::run",
            None,
        );
        crate::plog_info!("[azul] App::run starting (AZ_BACKEND={:?})", std::env::var("AZ_BACKEND").ok());
        let data = self.ptr.data.clone();
        let config = self.ptr.config.clone();
        let fc_cache = (*self.ptr.fc_cache).clone();
        let font_registry = self.ptr.font_registry.clone();
        let undo_manager = self.ptr.undo_manager.clone();

        // Publish the AppConfig snapshot the engine services read outside
        // callbacks: the updater (manifest URL, version, mode) and the
        // system dialogs (support mailbox, changelog URL).
        azul_layout::appenv::set_app_env(azul_layout::appenv::AppEnv::from_config(&config));

        // Arm the ACTION JOURNAL for apps that declared a problem-report
        // mailbox: the breadcrumb trail has to be recorded BEFORE the
        // problem, and an app that never collects reports pays nothing.
        if !matches!(
            config.report_problem,
            azul_core::resources::OptionEmailAddress::None
        ) {
            azul_layout::journal::set_enabled(true);
        }

        // Hand the CPU dialogs the driver-provisioning entry points. They
        // live in the dll (`video_codec::provision`) and the dialogs live
        // BELOW it in azul-layout, so the dll publishes fn pointers rather
        // than the type. `check` is inspection only; `remediate` is the
        // consent-gated, pkexec-elevated repair.
        azul_layout::appenv::set_gpu_provision_hooks(azul_layout::appenv::GpuProvisionHooks {
            check: || {
                let c = crate::unified::video_codec::provision::VideoStartupCheck::run();
                azul_layout::appenv::GpuProvisionReport {
                    hw_decode_ready: c.hw_decode_ready,
                    boot_safe: c.boot_safe,
                    can_remediate: c.can_remediate,
                    needs_reboot: c.needs_reboot,
                    summary: c.summary.as_str().to_string(),
                    detail: c.detail.as_str().to_string(),
                }
            },
            remediate: |on_step| {
                let o = crate::unified::video_codec::provision::VideoStartupCheck::
                    remediate_with_progress(on_step);
                azul_layout::appenv::GpuProvisionOutcome {
                    ok: o.ok,
                    reboot_required: o.reboot_required,
                    message: o.message.as_str().to_string(),
                }
            },
        });

        // ENGINE TELEMETRY (dll feature "telemetry"): initialised for every
        // app, but the TIER stays OFF unless AZ_TELEMETRY / the config files
        // opted in — with nothing opted in, none of this collects or sends.
        // This is what lets any azul app (azwriter under an e2e run, say) be
        // driven headlessly and report real frame durations + solver/raster
        // spans, so a stutter can be drilled down to the phase that caused
        // it.
        #[cfg(feature = "telemetry")]
        {
            let channel = std::env::var("AZ_TELEMETRY_CHANNEL")
                .unwrap_or_else(|_| "default".to_owned());
            let _telemetry_config = azul_layout::telemetry::init(
                config.updates.app_name.as_str(),
                azul_layout::telemetry::AppMeta::new(
                    config.updates.current_version.as_str(),
                    channel,
                ),
            );
            azul_layout::telemetry::install_panic_hook();
            azul_layout::telemetry::record_session_start();
            let _ = azul_layout::telemetry::spawn_uploader();
        }

        // CRASH-REPORTER TAKEOVER: a crashed sibling process (telemetry off,
        // crash contact configured) re-spawned this executable with
        // AZ_CRASH_DUMP=<dump.json>. This invocation IS the crash reporter:
        // show the dump in a CPU-rendered dialog instead of running the app.
        #[cfg(feature = "telemetry")]
        if let Some(dump) = azul_layout::telemetry::crash_dump_from_env() {
            crate::plog_info!(
                "[azul] AZ_CRASH_DUMP set — running the crash-reporter dialog for {:?}",
                dump.path
            );
            let dialog = azul_layout::dialogs::crash_reporter::window(dump);
            let err = crate::desktop::shell2::run(
                data,
                undo_manager,
                config,
                fc_cache,
                font_registry,
                dialog,
                // The crash reporter is a standalone dialog, not the app: it
                // must not inherit the app's tray.
                None,
            );
            if let Err(e) = err {
                eprintln!("[azul] crash-reporter dialog failed: {e:?}");
            }
            return;
        }

        // Use shell2 for the actual run loop
        let err = crate::desktop::shell2::run(data, undo_manager, config, fc_cache, font_registry, root_window, self.ptr.tray.clone());

        // Telemetry: persist + upload whatever the interval uploader has not
        // sent yet. Without this a SHORT run (an e2e drive, a screenshot
        // harness) exits before the first flush tick and reports nothing.
        #[cfg(feature = "telemetry")]
        {
            let _ = azul_layout::telemetry::drain_probe_events();
            let _outcome = azul_layout::telemetry::flush();
        }

        if let Err(e) = err {
            // ALWAYS surface the error — to the log facade AND raw stderr — on
            // EVERY platform. Previously a desktop error only went to msg_box,
            // which silently no-ops on Linux/Wayland without zenity/kdialog, so
            // a failed startup looked like the app "just exiting with no error".
            crate::plog_error!("[azul] application exited with error: {:?}", e);
            eprintln!("[azul] application error: {:?}", e);
            // Best-effort GUI dialog on desktop (only shows if a dialog backend
            // like zenity/kdialog is present; the stderr line above is the
            // guaranteed channel).
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            crate::desktop::dialogs::msg_box(&format!("Error: {:?}", e));
            debug_server::log(
                debug_server::LogLevel::Error,
                debug_server::LogCategory::EventLoop,
                format!("Application error: {:?}", e),
                None,
            );
        } else {
            crate::plog_info!("[azul] App::run returned cleanly (event loop ended)");
        }
    }
}

/// Graphical application that maintains some kind of application state
#[derive(Debug, Clone)]
#[repr(C)]
pub struct AppInternal {
    /// Your data (the global struct which all callbacks will have access to)
    pub data: RefAny,
    /// Application configuration, whether to enable logging, etc.
    pub config: AppConfig,
    /// The window create options (only set at startup), get moved into the `.run_inner()` method
    /// No window is actually shown until the `.run_inner()` method is called.
    pub windows: WindowCreateOptionsVec,
    /// Font configuration cache (shared across all windows)
    /// Initially empty  -  populated from the registry at first layout time
    pub fc_cache: Box<Arc<FcFontCache>>,
    /// Async font registry: background threads race to discover and parse fonts.
    /// At layout time, `request_fonts()` blocks until the needed fonts are ready,
    /// then snapshots into `fc_cache`. This eliminates the ~700ms startup block.
    pub font_registry: Option<Arc<FcFontRegistry>>,
    /// App-global undo/redo manager. Owned by the App; a shared clone is threaded
    /// to every window so a callback's `undo_app_state` / `redo_app_state` /
    /// `commit_undo_snapshot` operates on one shared history.
    pub undo_manager: crate::desktop::shell2::common::event::SharedUndoManager,
    /// Tray requested via [`App::set_tray`], applied when `run()` starts.
    ///
    /// Owned by the App rather than a process global: it is per-App state, and
    /// a global would silently pick the wrong one if a process ever ran two.
    pub tray: Option<azul_core::tray::TrayIconData>,
}

impl AppInternal {
    /// Creates a new, empty application.
    ///
    /// Does not open any windows  -  call `App::run` to enter the event loop.
    pub fn create(initial_data: RefAny, app_config: AppConfig) -> Self {

        debug_server::log(
            debug_server::LogLevel::Info,
            debug_server::LogCategory::General,
            "Starting App creation",
            None,
        );

        // [az-web-lift 2026-06-05] The web server's layout font comes from the injected
        // `with_memory_fonts` (eventloop.rs), NOT the system-font registry. Spawning
        // FcFontRegistry's multi-threaded scout+builder scan races on the no-atomic `StLock`
        // (StLock is single-thread-only — required so the lifted wasm doesn't spin on real LSE
        // atomics), causing a FLAKY native startup crash (rfc-font-builder translation faults /
        // insert_builder_font BTreeMap abort, post-"Classified"). Skip the registry on web (like
        // miri): no scan threads → no race. The lifted single-threaded font path is unaffected.
        #[cfg(all(not(miri), not(feature = "web")))]
        let (fc_cache, font_registry) = {
            // Create the async font registry (returns immediately)
            let registry = FcFontRegistry::new();

            // Try to load on-disk font cache (~10-20ms if cache exists, 0ms otherwise)
            let had_cache = registry.load_from_disk_cache();
            if had_cache.is_some() {
                debug_server::log(
                    debug_server::LogLevel::Info,
                    debug_server::LogCategory::Resources,
                    "Loaded font metadata from disk cache",
                    None,
                );
            }

            // Spawn Scout + Builder threads (returns immediately)
            registry.spawn_scout_and_builders();

            // Persist the scan so the NEXT launch can take the ~10-20ms
            // `load_from_disk_cache()` path above instead of re-scanning every
            // font on the system (~190ms on macOS, ~370 files).
            //
            // Without this, `load_from_disk_cache()` missed on every single
            // launch: `rust_fontconfig::FcFontRegistry::save_to_disk_cache`
            // had no caller anywhere — not in azul, and not inside
            // rust-fontconfig itself — so `dirs::cache_dir()/rfc/fonts/
            // manifest.bin` was never created and the branch above was dead
            // code that could only ever take its `else`.
            //
            // rust-fontconfig >= 4.4.12 persists from its own builder thread;
            // this call is what makes the fix work against the 4.4.x we
            // currently pin, and is harmless once the crate does it too (the
            // second write is a byte-identical atomic replace).
            if had_cache.is_none() {
                spawn_font_cache_persist(Arc::clone(&registry));
            }

            // Start with an empty FcFontCache; it will be populated at first layout
            // from the registry via request_fonts() + into_fc_font_cache()
            let cache = if had_cache.is_some() {
                // If we had a disk cache, snapshot the registry now so the fc_cache
                // is immediately usable (contains cached fonts from last run)
                Arc::new(registry.shared_cache())
            } else {
                Arc::new(FcFontCache::default())
            };

            (cache, Some(registry))
        };
        #[cfg(any(miri, feature = "web"))]
        let (fc_cache, font_registry) = (Arc::new(FcFontCache::default()), None);

        #[cfg(all(
            feature = "logging",
            feature = "fern_logger",
            not(feature = "pyo3_logger")
        ))]
        {
            crate::desktop::logging::set_up_logging(translate_log_level(app_config.log_level));
        }

        #[cfg(feature = "logging")]
        {
            if app_config.enable_logging_on_panic {
                crate::desktop::logging::set_up_panic_hooks();
            }

            if app_config.enable_visual_panic_hook {
                use std::sync::atomic::Ordering;
                crate::desktop::logging::SHOULD_ENABLE_PANIC_HOOK.store(true, Ordering::SeqCst);
            }
        }

        debug_server::log(
            debug_server::LogLevel::Info,
            debug_server::LogCategory::General,
            "App created successfully",
            None,
        );

        Self {
            windows: WindowCreateOptionsVec::from_const_slice(&[]),
            data: initial_data,
            config: app_config,
            tray: None,
            fc_cache: Box::new(fc_cache),
            font_registry,
            undo_manager: crate::desktop::shell2::common::event::SharedUndoManager::new(),
        }
    }
}

#[cfg(all(feature = "fern_logger", not(feature = "pyo3_logger")))]
use azul_core::resources::AppLogLevel;

#[cfg(all(feature = "fern_logger", not(feature = "pyo3_logger")))]
const fn translate_log_level(log_level: AppLogLevel) -> log::LevelFilter {
    match log_level {
        AppLogLevel::Off => log::LevelFilter::Off,
        AppLogLevel::Error => log::LevelFilter::Error,
        AppLogLevel::Warn => log::LevelFilter::Warn,
        AppLogLevel::Info => log::LevelFilter::Info,
        AppLogLevel::Debug => log::LevelFilter::Debug,
        AppLogLevel::Trace => log::LevelFilter::Trace,
    }
}

/// Discover the system style using platform-specific native APIs.
///
/// Dispatches to the appropriate platform's discovery module:
/// - macOS: `shell2/macos/system_style.rs` (dlopen + AppKit)
/// - Windows: `shell2/windows/system_style.rs` (LoadLibrary + User32/Dwmapi)
/// - Linux: `shell2/linux/system_style.rs` (D-Bus + gsettings)
pub(crate) fn discover_system_style() -> azul_css::system::SystemStyle {
    // Under Miri the platform `discover()` paths spawn external tools
    // (gsettings / dlopen AppKit / LoadLibrary), which Miri cannot emulate
    // ("can't call foreign function ..."). Fall back to the pure-Rust default
    // so `App::create` — and every test that builds an App — works under Miri.
    #[cfg(miri)]
    { azul_css::system::SystemStyle::detect() }
    #[cfg(all(not(miri), target_os = "macos"))]
    { crate::desktop::shell2::macos::system_style::discover() }
    #[cfg(all(not(miri), target_os = "windows"))]
    { crate::desktop::shell2::windows::system_style::discover() }
    #[cfg(all(not(miri), target_os = "linux"))]
    { crate::desktop::shell2::linux::system_style::discover() }
    #[cfg(all(not(miri), not(any(target_os = "macos", target_os = "windows", target_os = "linux"))))]
    { azul_css::system::SystemStyle::detect() }
}
