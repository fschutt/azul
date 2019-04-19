use std::sync::{Arc, Mutex};
use std::collections::BTreeMap;
use {
    callbacks::WindowId,
    window::FakeWindow,
    app_resources::AppResources,
    async::{Timer, Task, TimerId},
    FastHashMap,
};

/// Wrapper for your application data, stores the data, windows and resources, as
/// well as running timers and asynchronous tasks.
///
/// In order to be layout-able, your data model needs to satisfy the `Layout` trait,
/// which maps the state of your application to a DOM (how the application data should be laid out)
pub struct AppState<T> {
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
    pub windows: BTreeMap<WindowId, FakeWindow<T>>,
    /// Fonts, images and cached text that is currently loaded inside the app (window-independent).
    ///
    /// Accessing this field is often required to load new fonts or images, so instead of
    /// requiring the `FontHashMap`, a lot of functions just require the whole `AppResources` field.
    pub resources: AppResources,
    /// Currently running timers (polling functions, run on the main thread)
    pub(crate) timers: FastHashMap<TimerId, Timer<T>>,
    /// Currently running tasks (asynchronous functions running each on a different thread)
    pub(crate) tasks: Vec<Task<T>>,
}

/// Same as the [AppState](./struct.AppState.html) but without the
/// `self.data` field - used for default callbacks, so that callbacks can
/// load and unload fonts or images + access the system clipboard
///
/// Default callbacks don't have access to the `AppState.data` field,
/// since they use a `StackCheckedPointer` instead.
pub struct AppStateNoData<'a, T> {
    /// See [`AppState.windows`](./struct.AppState.html#structfield.windows)
    pub windows: &'a BTreeMap<WindowId, FakeWindow<T>>,
    /// See [`AppState.resources`](./struct.AppState.html#structfield.resources)
    pub resources : &'a mut AppResources,
    /// Currently running timers (polling functions, run on the main thread)
    pub(crate) timers: FastHashMap<TimerId, Timer<T>>,
    /// Currently running tasks (asynchronous functions running each on a different thread)
    pub(crate) tasks: Vec<Task<T>>,
}