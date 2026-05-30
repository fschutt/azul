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

impl App {
    pub fn create(initial_data: RefAny, mut app_config: AppConfig) -> Self {
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

        // Use shell2 for the actual run loop
        let err = crate::desktop::shell2::run(data, config, fc_cache, font_registry, root_window);

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
    /// Initially empty — populated from the registry at first layout time
    pub fc_cache: Box<Arc<FcFontCache>>,
    /// Async font registry: background threads race to discover and parse fonts.
    /// At layout time, `request_fonts()` blocks until the needed fonts are ready,
    /// then snapshots into `fc_cache`. This eliminates the ~700ms startup block.
    pub font_registry: Option<Arc<FcFontRegistry>>,
}

impl AppInternal {
    /// Creates a new, empty application.
    ///
    /// Does not open any windows — call `App::run` to enter the event loop.
    pub fn create(initial_data: RefAny, app_config: AppConfig) -> Self {

        debug_server::log(
            debug_server::LogLevel::Info,
            debug_server::LogCategory::General,
            "Starting App creation",
            None,
        );

        #[cfg(not(miri))]
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
        #[cfg(miri)]
        let (fc_cache, font_registry) = (Arc::new(FcFontCache::default()), None);

        #[cfg(all(
            feature = "logging",
            feature = "use_fern_logger",
            not(feature = "use_pyo3_logger")
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
            fc_cache: Box::new(fc_cache),
            font_registry,
        }
    }
}

#[cfg(all(feature = "use_fern_logger", not(feature = "use_pyo3_logger")))]
use azul_core::resources::AppLogLevel;

#[cfg(all(feature = "use_fern_logger", not(feature = "use_pyo3_logger")))]
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
