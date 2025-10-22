use alloc::sync::Arc;
use std::{fmt, sync::Mutex, thread::JoinHandle};

use azul_core::{
    callbacks::{Dummy, Update},
    refany::RefAny,
    resources::{AppConfig, ImageCache, ImageRef},
    task::TimerId,
    window::MonitorVec,
};
use azul_css::{impl_option, impl_option_inner, AzString};
use azul_layout::{timer::Timer, window_state::WindowCreateOptions};
use clipboard2::{Clipboard as _, ClipboardError, SystemClipboard};
use rust_fontconfig::FcFontCache;

#[derive(Debug, Clone)]
#[repr(C)]
pub struct AzAppPtr {
    pub ptr: Box<Arc<Mutex<App>>>,
    pub run_destructor: bool,
}

impl Drop for AzAppPtr {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

impl AzAppPtr {
    pub fn new(initial_data: RefAny, app_config: AppConfig) -> Self {
        Self {
            ptr: Box::new(Arc::new(Mutex::new(App::new(initial_data, app_config)))),
            run_destructor: true,
        }
    }

    pub fn add_window(&mut self, create_options: WindowCreateOptions) {
        if let Ok(mut l) = (&*self.ptr).try_lock() {
            l.add_window(create_options);
        }
    }

    pub fn get_monitors(&self) -> MonitorVec {
        self.ptr
            .lock()
            .map(|m| m.get_monitors())
            .unwrap_or(MonitorVec::from_const_slice(&[]))
    }

    pub fn run(&self, root_window: WindowCreateOptions) {
        if let Ok(mut l) = self.ptr.try_lock() {
            let mut app = App::new(RefAny::new(Dummy { _dummy: 0 }), l.config.clone());
            core::mem::swap(&mut *l, &mut app);
            app.run(root_window)
        }
    }
}

/// Graphical application that maintains some kind of application state
#[derive(Debug)]
pub struct App {
    /// Your data (the global struct which all callbacks will have access to)
    pub data: RefAny,
    /// Application configuration, whether to enable logging, etc.
    pub config: AppConfig,
    /// The window create options (only set at startup), get moved into the `.run_inner()` method
    /// No window is actually shown until the `.run_inner()` method is called.
    pub windows: Vec<WindowCreateOptions>,
    /// Font configuration cache (shared across all windows)
    pub fc_cache: std::sync::Arc<FcFontCache>,
}

impl App {
    #[allow(unused_variables)]
    /// Creates a new, empty application using a specified callback.
    ///
    /// This does not open any windows, but it starts the event loop
    /// to the display server
    pub fn new(initial_data: RefAny, app_config: AppConfig) -> Self {
        #[cfg(not(miri))]
        let fc_cache = std::sync::Arc::new(FcFontCache::build());
        #[cfg(miri)]
        let fc_cache = std::sync::Arc::new(FcFontCache::default());

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

        Self {
            windows: Vec::new(),
            data: initial_data,
            config: app_config,
            fc_cache,
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
        // TODO: Implement in shell2
        // For now return empty list until shell2 is complete
        MonitorVec::from_const_slice(&[])

        // #[cfg(target_os = "windows")]
        // {
        //     crate::desktop::shell::win32::get_monitors(self)
        // }

        // #[cfg(target_os = "linux")]
        // {
        //     crate::desktop::shell::x11::get_monitors(self)
        // }

        // #[cfg(target_os = "macos")]
        // {
        //     crate::desktop::shell::appkit::get_monitors(self)
        // }
    }

    /// Start the rendering loop for the currently added windows. The run() function
    /// takes one `WindowCreateOptions` as an argument, which is the "root" window, i.e.
    /// the main application window.
    #[cfg(feature = "std")]
    pub fn run(mut self, root_window: WindowCreateOptions) {
        // Use shell2 for new implementation
        let err = crate::desktop::shell2::run(self.config.clone(), self.fc_cache.clone(), root_window);

        if let Err(e) = err {
            crate::desktop::dialogs::msg_box(&format!("Error: {:?}", e));
            eprintln!("Application error: {:?}", e);
        }
    }
}

#[cfg(all(feature = "use_fern_logger", not(feature = "use_pyo3_logger")))]
const fn translate_log_level(log_level: azul_core::resources::AppLogLevel) -> log::LevelFilter {
    match log_level {
        azul_core::resources::AppLogLevel::Off => log::LevelFilter::Off,
        azul_core::resources::AppLogLevel::Error => log::LevelFilter::Error,
        azul_core::resources::AppLogLevel::Warn => log::LevelFilter::Warn,
        azul_core::resources::AppLogLevel::Info => log::LevelFilter::Info,
        azul_core::resources::AppLogLevel::Debug => log::LevelFilter::Debug,
        azul_core::resources::AppLogLevel::Trace => log::LevelFilter::Trace,
    }
}

/// Clipboard is an empty class with only static methods,
/// which is why it doesn't have any #[derive] markers.
#[repr(C)]
#[derive(Clone)]
pub struct Clipboard {
    pub _native: Box<Arc<Mutex<SystemClipboard>>>,
    pub run_destructor: bool,
}

impl fmt::Debug for Clipboard {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Clipboard {{ ... }}")
    }
}

impl_option!(Clipboard, OptionClipboard, copy = false, [Clone, Debug]);

impl Clipboard {
    pub fn new() -> Option<Self> {
        let clipboard = SystemClipboard::new().ok()?;
        Some(Self {
            _native: Box::new(Arc::new(Mutex::new(clipboard))),
            run_destructor: true,
        })
    }

    /// Returns the contents of the system clipboard
    pub fn get_clipboard_string(&self) -> Option<AzString> {
        self._native
            .lock()
            .ok()?
            .get_string_contents()
            .map(|o| o.into())
            .ok()
    }

    /// Sets the contents of the system clipboard
    pub fn set_clipboard_string(&mut self, contents: AzString) -> Option<()> {
        Arc::get_mut(&mut *self._native)?
            .get_mut()
            .ok()?
            .set_string_contents(contents.into_library_owned_string())
            .ok()?;
        Some(())
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}
