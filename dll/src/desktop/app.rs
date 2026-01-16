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

use crate::desktop::shell2::common::debug_server::{self, DebugServerHandle, LogCategory};
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
        
        Self {
            ptr: Box::new(AppInternal::create(initial_data, app_config)),
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

        // Use shell2 for the actual run loop
        let err = crate::desktop::shell2::run(data, config, fc_cache, root_window);

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
    pub fc_cache: Box<Arc<FcFontCache>>,
    /// Debug server handle (if AZUL_DEBUG is set)
    #[allow(dead_code)]
    pub debug_server: Option<Arc<DebugServerHandle>>,
}

impl AppInternal {
    #[allow(unused_variables)]
    /// Creates a new, empty application using a specified callback.
    ///
    /// This does not open any windows, but it starts the event loop
    /// to the display server
    pub fn create(initial_data: RefAny, app_config: AppConfig) -> Self {
        // Start debug server first if AZUL_DEBUG is set (blocks until ready)
        let debug_server = if let Some(port) = debug_server::get_debug_port() {
            // Debug server will be started - logging will be available after this
            let handle = debug_server::start_debug_server(port);
            Some(Arc::new(handle))
        } else {
            None
        };

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
                "initial_data._internal_ptr: {:?}",
                initial_data._internal_ptr
            ),
            None,
        );

        #[cfg(not(miri))]
        let fc_cache = {
            debug_server::log(
                debug_server::LogLevel::Info,
                debug_server::LogCategory::Resources,
                "Building FcFontCache...",
                None,
            );
            let cache = Arc::new(FcFontCache::build());
            debug_server::log(
                debug_server::LogLevel::Info,
                debug_server::LogCategory::Resources,
                "FcFontCache built successfully",
                None,
            );
            cache
        };
        #[cfg(miri)]
        let fc_cache = Arc::new(FcFontCache::default());

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
            debug_server,
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
        #[cfg(target_os = "linux")]
        {
            crate::desktop::shell2::linux::get_monitors()
        }

        #[cfg(not(target_os = "linux"))]
        {
            // TODO: Implement for Windows and macOS
            MonitorVec::from_const_slice(&[])
        }
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
