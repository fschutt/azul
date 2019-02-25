use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
#[cfg(feature = "image_loading")]
use app_resources::ImageSource;
use glium::glutin::WindowId as GliumWindowId;
use {
    FastHashMap,
    text_cache::TextId,
    app::AppConfig,
    window::{FakeWindow, WindowCreateError},
    task::Task,
    dom::{UpdateScreen, Redraw, DontRedraw},
    traits::Layout,
    app_resources::{
        AppResources, ImageId, FontSource, FontId, CssImageId,
        FontReloadError, ImageReloadError, RawImage,
    },
    error::ClipboardError,
    daemon::{Daemon, DaemonId, TerminateDaemon},
};

/// Wrapper for your application data, stores the data, windows and resources, as
/// well as running daemons and asynchronous tasks.
///
/// In order to be layout-able, your data model needs to satisfy the `Layout` trait,
/// which maps the state of your application to a DOM (how the application data should be laid out)
pub struct AppState<T: Layout> {
    /// Your data (the global struct which all callbacks will have access to)
    pub data: Arc<Mutex<T>>,
    /// This field represents the state of the windows, public to the user. You can
    /// mess around with the state as you like, however, the actual window won't update
    /// until the next frame. This is done to "decouple" the frameworks internal
    /// state updating logic from the user code (and to make the API future-proof
    /// in case extra functions are introduced).
    ///
    /// Another reason this is needed is to (later) introduce testing for the window
    /// state - if the API would directly modify the window itself, these changes
    /// wouldn't be recorded anywhere, so there wouldn't be a way to unit-test certain APIs.
    ///
    /// The state of these `FakeWindow`s gets deleted and recreated on each frame, especially
    /// the app's style. This should force a user to design his code in a functional way,
    /// without relying on state-based conditions. Example:
    ///
    /// ```no_run,ignore
    /// let window_state = &mut app_state.windows[event.window];
    /// // Update the title
    /// window_state.state.title = "Hello";
    /// ```
    pub windows: BTreeMap<GliumWindowId, FakeWindow<T>>,
    /// Fonts, images and cached text that is currently loaded inside the app (window-independent).
    ///
    /// Accessing this field is often required to load new fonts or images, so instead of
    /// requiring the `FontHashMap`, a lot of functions just require the whole `AppResources` field.
    pub resources: AppResources,
    /// Currently running daemons (polling functions, run on the main thread)
    pub(crate) daemons: FastHashMap<DaemonId, Daemon<T>>,
    /// Currently running tasks (asynchronous functions running each on a different thread)
    pub(crate) tasks: Vec<Task<T>>,
}

/// Same as the [AppState](./struct.AppState.html) but without the
/// `self.data` field - used for default callbacks, so that callbacks can
/// load and unload fonts or images + access the system clipboard
///
/// Default callbacks don't have access to the `AppState.data` field,
/// since they use a `StackCheckedPointer` instead.
pub struct AppStateNoData<'a, T: 'a + Layout> {
    /// See [`AppState.windows`](./struct.AppState.html#structfield.windows)
    pub windows: &'a BTreeMap<GliumWindowId, FakeWindow<T>>,
    /// See [`AppState.resources`](./struct.AppState.html#structfield.resources)
    pub resources : &'a mut AppResources,
    /// Currently running daemons (polling functions, run on the main thread)
    pub(crate) daemons: FastHashMap<DaemonId, Daemon<T>>,
    /// Currently running tasks (asynchronous functions running each on a different thread)
    pub(crate) tasks: Vec<Task<T>>,
}

macro_rules! impl_deamon_api {() => (

    /// Insert a daemon into the list of active daemons.
    /// Replaces the existing daemon if called with the same DaemonId.
    pub fn add_daemon(&mut self, id: DaemonId, daemon: Daemon<T>) {
        self.daemons.insert(id, daemon);
    }

    pub fn has_daemon(&self, daemon_id: &DaemonId) -> bool {
        self.get_daemon(daemon_id).is_some()
    }

    pub fn get_daemon(&self, daemon_id: &DaemonId) -> Option<Daemon<T>> {
        self.daemons.get(&daemon_id).cloned()
    }

    pub fn delete_daemon(&mut self, daemon_id: &DaemonId) -> Option<Daemon<T>> {
        self.daemons.remove(daemon_id)
    }

    /// Custom tasks can be used when the `AppState` isn't `Send`. For example
    /// `SvgCache` isn't thread-safe, since it has to interact with OpenGL, so
    /// it can't be sent to other threads safely.
    ///
    /// What you can do instead, is take a part of your application data, wrap
    /// that in an `Arc<Mutex<>>` and push a task that takes it onto the queue.
    /// This way you can modify a part of the application state on a different
    /// thread, while not requiring that everything is thread-safe.
    ///
    /// While you can't modify the `SvgCache` from a different thread, you can
    /// modify other things in the `AppState` and leave the SVG cache alone.
    pub fn add_task(&mut self, task: Task<T>) {
        self.tasks.push(task);
    }
)}

impl<'a, T: 'a + Layout> AppStateNoData<'a, T> {
    impl_deamon_api!();
}

impl<T: Layout> AppState<T> {

    /// Creates a new `AppState`
    pub fn new(initial_data: T, config: &AppConfig) -> Result<Self, WindowCreateError> {
        Ok(Self {
            data: Arc::new(Mutex::new(initial_data)),
            windows: BTreeMap::new(),
            resources: AppResources::new(config)?,
            daemons: FastHashMap::default(),
            tasks: Vec::new(),
        })
    }

    impl_deamon_api!();

    /// Run all currently registered daemons
    #[must_use]
    pub(crate) fn run_all_daemons(&mut self)
    -> UpdateScreen
    {
        let mut should_update_screen = DontRedraw;
        let mut lock = self.data.lock().unwrap();
        let mut daemons_to_terminate = Vec::new();

        for (key, daemon) in self.daemons.iter_mut() {
            let (should_update, should_terminate) = daemon.invoke_callback_with_data(&mut lock, &mut self.resources);

            if should_update == Redraw &&
               should_update_screen == DontRedraw {
                should_update_screen = Redraw;
            }

            if should_terminate == TerminateDaemon::Terminate {
                daemons_to_terminate.push(key.clone());
            }
        }

        for key in daemons_to_terminate {
            self.daemons.remove(&key);
        }

        should_update_screen
    }

    /// Remove all tasks that have finished executing
    #[must_use]
    pub(crate) fn clean_up_finished_tasks(&mut self)
    -> UpdateScreen
    {
        let old_count = self.tasks.len();
        let mut daemons_to_add = Vec::new();
        self.tasks.retain(|task| {
            if !task.is_finished() {
                true
            } else {
                daemons_to_add.extend(task.after_completion_daemons.iter().cloned());
                false
            }
        });

        let daemons_is_empty = daemons_to_add.is_empty();
        let new_count = self.tasks.len();

        // Start all the daemons that should run after the completion of the task
        for (daemon_id, daemon) in daemons_to_add {
            self.add_daemon(daemon_id, daemon);
        }

        if old_count == new_count && daemons_is_empty {
            DontRedraw
        } else {
            Redraw
        }
    }
}

image_api!(AppState::resources);
font_api!(AppState::resources);
text_api!(AppState::resources);
clipboard_api!(AppState::resources);