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
use azul_layout::{timer::Timer, window_state::{WindowCreateOptions, WindowCreateOptionsVec}};
use rust_fontconfig::FcFontCache;

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

impl App {
    pub fn new(initial_data: RefAny, app_config: AppConfig) -> Self {
        Self {
            ptr: Box::new(AppInternal::new(initial_data, app_config)),
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
        eprintln!("[App::run] Starting...");
        eprintln!("[App::run] Cloning data...");
        let data = self.ptr.data.clone();
        eprintln!("[App::run] Data cloned successfully");
        eprintln!("[App::run] Cloning config...");
        let config = self.ptr.config.clone();
        eprintln!("[App::run] Config cloned successfully");
        eprintln!("[App::run] Cloning fc_cache...");
        let fc_cache = (*self.ptr.fc_cache).clone();
        eprintln!("[App::run] fc_cache cloned successfully");
        eprintln!("[App::run] Calling shell2::run...");
        
        // Use shell2 for the actual run loop
        let err = crate::desktop::shell2::run(
            data,
            config,
            fc_cache,
            root_window,
        );

        if let Err(e) = err {
            crate::desktop::dialogs::msg_box(&format!("Error: {:?}", e));
            eprintln!("Application error: {:?}", e);
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
}

impl AppInternal {
    #[allow(unused_variables)]
    /// Creates a new, empty application using a specified callback.
    ///
    /// This does not open any windows, but it starts the event loop
    /// to the display server
    pub fn new(initial_data: RefAny, app_config: AppConfig) -> Self {
        eprintln!("[AppInternal::new] Starting App creation");
        eprintln!("[AppInternal::new] initial_data._internal_ptr: {:?}", initial_data._internal_ptr);
        eprintln!("[AppInternal::new] initial_data.sharing_info.ptr: {:?}", initial_data.sharing_info.ptr);

        #[cfg(not(miri))]
        let fc_cache = {
            eprintln!("[App::new] Building FcFontCache...");
            let cache = Arc::new(FcFontCache::build());
            eprintln!("[App::new] FcFontCache built successfully");
            cache
        };
        #[cfg(miri)]
        let fc_cache = Arc::new(FcFontCache::default());

        eprintln!("[App::new] Setting up logging...");

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

        eprintln!("[App::new] App created successfully");

        Self {
            windows: WindowCreateOptionsVec::from_const_slice(&[]),
            data: initial_data,
            config: app_config,
            fc_cache: Box::new(fc_cache),
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
            eprintln!("Application error: {:?}", e);
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
