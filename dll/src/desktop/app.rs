use alloc::sync::Arc;
use std::fmt;

use azul_core::{
    callbacks::Update,
    refany::RefAny,
    resources::{AppConfig, ImageCache, ImageRef},
    task::TimerId,
    window::MonitorVec,
};
use azul_css::{impl_option, impl_option_inner, AzString};
use azul_layout::{
    timer::Timer,
    window_state::{WindowCreateOptions, WindowCreateOptionsVec},
};
use rust_fontconfig::FcFontCache;
use rust_fontconfig::registry::FcFontRegistry;

use crate::desktop::shell2::common::debug_server::{self, LogCategory};
use crate::log_error;

#[derive(Debug, Clone)]
#[repr(C)]
pub struct App {
    pub ptr: Box<AppInternal>,
    pub run_destructor: bool,
}

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
        // Set the icon resolver from the layout crate (the default resolver in core is a no-op)
        app_config.icon_provider.set_resolver(azul_layout::icon::default_icon_resolver);
        
        // Register embedded Material Icons if the feature is enabled
        azul_layout::icon::register_embedded_material_icons(&mut app_config.icon_provider);
        
        let app_internal = AppInternal::create(initial_data, app_config);
        let boxed = Box::new(app_internal);
        
        Self {
            ptr: boxed,
            run_destructor: true,
        }
    }

    pub fn add_window(&mut self, create_options: WindowCreateOptions) {
        self.ptr.add_window(create_options);
    }

    pub fn get_monitors(&self) -> MonitorVec {
        self.ptr.get_monitors()
    }

    pub fn run(&self, root_window: WindowCreateOptions) {
        debug_server::log(
            debug_server::LogLevel::Info,
            debug_server::LogCategory::EventLoop,
            "Starting App::run",
            None,
        );
        let data = self.ptr.data.clone();
        let config = self.ptr.config.clone();
        let fc_cache = (*self.ptr.fc_cache).clone();
        let font_registry = self.ptr.font_registry.clone();

        // Use shell2 for the actual run loop
        let err = crate::desktop::shell2::run(data, config, fc_cache, font_registry, root_window);

        if let Err(e) = err {
            crate::desktop::dialogs::msg_box(&format!("Error: {:?}", e));
            debug_server::log(
                debug_server::LogLevel::Error,
                debug_server::LogCategory::EventLoop,
                format!("Application error: {:?}", e),
                None,
            );
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
    #[allow(unused_variables)]
    /// Creates a new, empty application using a specified callback.
    ///
    /// This does not open any windows, but it starts the event loop
    /// to the display server
    pub fn create(initial_data: RefAny, app_config: AppConfig) -> Self {

        debug_server::log(
            debug_server::LogLevel::Info,
            debug_server::LogCategory::General,
            "Starting App creation",
            None,
        );

        debug_server::log(
            debug_server::LogLevel::Debug,
            debug_server::LogCategory::General,
            format!(
                "initial_data sharing_info: {:?}",
                initial_data.sharing_info
            ),
            None,
        );

        #[cfg(not(miri))]
        let (fc_cache, font_registry) = {
            debug_server::log(
                debug_server::LogLevel::Info,
                debug_server::LogCategory::Resources,
                "Starting async font registry...",
                None,
            );

            // Create the async font registry (returns immediately)
            let registry = FcFontRegistry::new();

            // Try to load on-disk font cache (~10-20ms if cache exists, 0ms otherwise)
            let had_cache = registry.load_from_disk_cache();
            if had_cache {
                debug_server::log(
                    debug_server::LogLevel::Info,
                    debug_server::LogCategory::Resources,
                    "Loaded font metadata from disk cache",
                    None,
                );
            }

            // Spawn Scout + Builder threads (returns immediately)
            registry.spawn_scout_and_builders();

            debug_server::log(
                debug_server::LogLevel::Info,
                debug_server::LogCategory::Resources,
                "Font registry spawned (background threads scanning)",
                None,
            );

            // Start with an empty FcFontCache; it will be populated at first layout
            // from the registry via request_fonts() + into_fc_font_cache()
            let cache = if had_cache {
                // If we had a disk cache, snapshot the registry now so the fc_cache
                // is immediately usable (contains cached fonts from last run)
                Arc::new(registry.into_fc_font_cache())
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
            crate::logging::set_up_logging(translate_log_level(app_config.log_level));
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
            font_registry: font_registry,
        }
    }

    /// Spawn a new window on the screen. Note that this should only be used to
    /// create extra windows, the default window will be the window submitted to
    /// the `.run` method.
    pub fn add_window(&mut self, create_options: WindowCreateOptions) {
        self.windows.push(create_options);
    }

    /// Returns a list of monitors available on the system
    pub fn get_monitors(&self) -> MonitorVec {
        crate::desktop::display::get_monitors()
    }

    /// Start the rendering loop for the currently added windows. The run() function
    /// takes one `WindowCreateOptions` as an argument, which is the "root" window, i.e.
    /// the main application window.
    #[cfg(feature = "std")]
    pub fn run(mut self, root_window: WindowCreateOptions) {
        // Use shell2 for new implementation
        let err = crate::desktop::shell2::run(
            self.data,
            self.config.clone(),
            (*self.fc_cache).clone(),
            self.font_registry.clone(),
            root_window,
        );

        if let Err(e) = err {
            crate::desktop::dialogs::msg_box(&format!("Error: {:?}", e));
            crate::log_error!(LogCategory::General, "Application error: {:?}", e);
        }
    }
}

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
