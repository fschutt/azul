use alloc::sync::Arc;
use std::{fmt, sync::Mutex, thread::JoinHandle};

use azul_core::{
    app_resources::{AppConfig, ImageCache, ImageRef},
    callbacks::{Dummy, RefAny, Update},
    display_list::RenderCallbacks,
    task::{Timer, TimerId},
    window::{MonitorVec, WindowCreateOptions},
};
use azul_css::{impl_option, impl_option_inner, AzString};
use clipboard2::{Clipboard as _, ClipboardError, SystemClipboard};
use rust_fontconfig::FcFontCache;

pub(crate) const CALLBACKS: RenderCallbacks = RenderCallbacks {
    insert_into_active_gl_textures_fn: azul_core::gl::insert_into_active_gl_textures,
    layout_fn: azul_layout::solver2::do_the_layout,
    load_font_fn: azul_layout::font::loading::font_source_get_bytes,
    parse_font_fn: azul_layout::parse_font_fn,
};

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

    pub fn add_image(&mut self, css_id: AzString, image: ImageRef) {
        if let Ok(mut l) = (&*self.ptr).try_lock() {
            l.add_image(css_id, image);
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
    /// Initial cache of images that are loaded before the first frame is rendered
    pub image_cache: ImageCache,
    /// Font configuration cache - already start building the font cache
    /// while the app is starting
    pub fc_cache: LazyFcCache,
}

impl App {
    #[allow(unused_variables)]
    /// Creates a new, empty application using a specified callback.
    ///
    /// This does not open any windows, but it starts the event loop
    /// to the display server
    pub fn new(initial_data: RefAny, app_config: AppConfig) -> Self {
        use std::thread;

        #[cfg(not(miri))]
        let fc_cache = LazyFcCache::InProgress(Some(thread::spawn(move || FcFontCache::build())));
        #[cfg(miri)]
        let fc_cache = LazyFcCache::Resolved(FcFontCache::default());

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
            image_cache: ImageCache::new(),
            fc_cache,
        }
    }

    /// Registers an image with a CSS Id so that it can be used in the `background-content` property
    pub fn add_image(&mut self, css_id: AzString, image: ImageRef) {
        self.image_cache.add_css_image_id(css_id, image);
    }

    /// Spawn a new window on the screen. Note that this should only be used to
    /// create extra windows, the default window will be the window submitted to
    /// the `.run` method.
    pub fn add_window(&mut self, create_options: WindowCreateOptions) {
        self.windows.push(create_options);
    }

    /// Returns a list of monitors available on the system
    pub fn get_monitors(&self) -> MonitorVec {
        #[cfg(target_os = "windows")]
        {
            crate::desktop::shell::win32::get_monitors(self)
        }

        #[cfg(target_os = "linux")]
        {
            crate::desktop::shell::x11::get_monitors(self)
        }

        #[cfg(target_os = "macos")]
        {
            crate::desktop::shell::appkit::get_monitors(self)
        }
    }

    /// Start the rendering loop for the currently added windows. The run() function
    /// takes one `WindowCreateOptions` as an argument, which is the "root" window, i.e.
    /// the main application window.
    #[cfg(feature = "std")]
    pub fn run(mut self, root_window: WindowCreateOptions) {
        #[cfg(target_os = "windows")]
        let err = crate::desktop::shell::win32::run(self, root_window);

        #[cfg(target_os = "linux")]
        let err = crate::desktop::shell::x11::run(self, root_window);

        #[cfg(target_os = "macos")]
        let err = crate::desktop::shell::appkit::run(self, root_window);

        if let Err(e) = err {
            crate::desktop::dialogs::msg_box(&format!("{:?}", e));
            println!("{:?}", e);
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

#[derive(Debug)]
pub enum LazyFcCache {
    Resolved(FcFontCache),
    InProgress(Option<JoinHandle<FcFontCache>>),
}

impl LazyFcCache {
    pub fn apply_closure<T, F: FnOnce(&mut FcFontCache) -> T>(&mut self, closure: F) -> T {
        let mut replace = None;

        let result = match self {
            LazyFcCache::Resolved(c) => closure(c),
            LazyFcCache::InProgress(j) => {
                let mut font_cache = j
                    .take()
                    .and_then(|j| Some(j.join().ok()))
                    .unwrap_or_default()
                    .unwrap_or_default();
                let r = closure(&mut font_cache);
                replace = Some(font_cache);
                r
            }
        };

        if let Some(replace) = replace {
            *self = LazyFcCache::Resolved(replace);
        }

        result
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
