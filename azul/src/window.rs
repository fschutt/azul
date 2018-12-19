//! Window creation module

use std::{
    time::Duration,
    fmt,
    rc::Rc,
    marker::PhantomData,
    io::Error as IoError,
};
use webrender::{
    api::{
        LayoutRect, PipelineId, Epoch, ColorF, BuiltDisplayList, DocumentId,
        RenderApi, ExternalScrollId, RenderNotifier, HitTestResult, DeviceIntSize,
    },
    Renderer, RendererOptions, RendererKind, ShaderPrecacheFlags,
    // renderer::RendererError; -- not currently public in WebRender
};
use glium::{
    IncompatibleOpenGl, Display, SwapBuffersError,
    debug::DebugCallbackBehavior,
    glutin::{
        self, EventsLoop, AvailableMonitorsIter, GlContext, GlWindow, CreationError,
        MonitorId, EventsLoopProxy, ContextError, ContextBuilder, WindowBuilder, Icon,
        dpi::{LogicalSize, PhysicalSize}
    },
    backend::{Context, Facade, glutin::DisplayCreationError},
};
use gleam::gl::{self, Gl};
use azul_css::Css;
#[cfg(debug_assertions)]
use azul_css::HotReloadHandler;
use {
    FastHashMap,
    dom::{Texture, Callback},
    daemon::{Daemon, DaemonId},
    window_state::{WindowState, MouseState, KeyboardState, DebugState},
    traits::Layout,
    compositor::Compositor,
    style::sort_by_specificity,
    app::FrameEventInfo,
    app_resources::AppResources,
    id_tree::NodeId,
    default_callbacks::{DefaultCallbackSystem, StackCheckedPointer, DefaultCallback, DefaultCallbackId},
    ui_state::UiState,
    display_list::ScrolledNodes,
};

/// azul-internal ID for a window
#[derive(Debug, Copy, Clone, PartialOrd, Ord, PartialEq, Eq)]
pub struct WindowId {
    pub(crate) id: usize,
}

impl WindowId {
    pub(crate) fn new(id: usize) -> Self { Self { id: id } }
}

/// User-modifiable fake window
#[derive(Clone)]
pub struct FakeWindow<T: Layout> {
    /// The window state for the next frame
    pub state: WindowState,
    /// The user can push default callbacks in this `DefaultCallbackSystem`,
    /// which get called later in the hit-testing logic
    pub(crate) default_callbacks: DefaultCallbackSystem<T>,
    /// An Rc to the original WindowContext - this is only so that
    /// the user can create textures and other OpenGL content in the window
    /// but not change any window properties from underneath - this would
    /// lead to mismatch between the
    pub(crate) read_only_window: Rc<Display>,
}

impl<T: Layout> FakeWindow<T> {

    /// Returns a read-only window which can be used to create / draw
    /// custom OpenGL texture during the `.layout()` phase
    pub fn read_only_window(&self) -> ReadOnlyWindow {
        ReadOnlyWindow {
            inner: self.read_only_window.clone()
        }
    }

    pub(crate) fn set_keyboard_state(&mut self, kb: &KeyboardState) {
        self.state.keyboard_state = kb.clone();
    }

    pub(crate) fn set_mouse_state(&mut self, mouse: &MouseState) {
        self.state.mouse_state = *mouse;
    }

    /// Returns the current keyboard keyboard state. We don't want the library
    /// user to be able to modify this state, only to read it.
    pub fn get_keyboard_state<'a>(&'a self) -> &'a KeyboardState {
        self.state.get_keyboard_state()
    }

    /// Returns the current windows mouse state. We don't want the library
    /// user to be able to modify this state, only to read it
    pub fn get_mouse_state<'a>(&'a self) -> &'a MouseState {
        self.state.get_mouse_state()
    }

    /// Adds a default callback to the window. The default callbacks are
    /// cleared after every frame, so two-way data binding widgets have to call this
    /// on every frame they want to insert a default callback.
    ///
    /// Returns an ID by which the callback can be uniquely identified (used for hit-testing)
    #[must_use]
    pub fn add_callback(
        &mut self,
        callback_ptr: StackCheckedPointer<T>,
        callback_fn: DefaultCallback<T>)
    -> DefaultCallbackId
    {
        use default_callbacks::get_new_unique_default_callback_id;

        let default_callback_id = get_new_unique_default_callback_id();
        self.default_callbacks.add_callback(default_callback_id, callback_ptr, callback_fn);
        default_callback_id
    }
}

/// Read-only window which can be used to create / draw
/// custom OpenGL texture during the `.layout()` phase
#[derive(Clone)]
pub struct ReadOnlyWindow {
    pub inner: Rc<Display>,
}

impl Facade for ReadOnlyWindow {
    fn get_context(&self) -> &Rc<Context> {
        self.inner.get_context()
    }
}

impl ReadOnlyWindow {

    pub fn get_physical_size(&self) -> (u32, u32) {
        let hidpi = self.get_hidpi_factor();
        self.inner.gl_window().get_inner_size().unwrap().to_physical(hidpi).into()
    }

    /// Returns the current HiDPI factor.
    pub fn get_hidpi_factor(&self) -> f64 {
        self.inner.gl_window().get_hidpi_factor()
    }

    // Since webrender is asynchronous, we can't let the user draw
    // directly onto the frame or the texture since that has to be timed
    // with webrender
    pub fn create_texture(&self, width: u32, height: u32) -> Texture {
        use glium::texture::texture2d::Texture2d;
        let tex = Texture2d::empty(&*self.inner, width, height).unwrap();
        Texture::new(tex)
    }

    /// Make the window active (OpenGL) - necessary before
    /// starting to draw on any window-owned texture
    pub fn make_current(&self) {
        use glium::glutin::GlContext;
        let gl_window = self.inner.gl_window();
        unsafe { gl_window.make_current().unwrap() };
    }

    /// Unbind the current framebuffer manually. Is also executed on `Drop`.
    ///
    /// TODO: Is it necessary to expose this or is it enough to just
    /// unbind the framebuffer on drop?
    pub fn unbind_framebuffer(&self) {
        let gl = get_gl_context(&*self.inner).unwrap();

        gl.bind_framebuffer(gl::FRAMEBUFFER, 0);
    }

    pub fn get_gl_context(&self) -> Rc<Gl> {
        // Can only fail when the API was initialized from WebGL,
        // which can't happen, since that would already crash on startup
        get_gl_context(&*self.inner).unwrap()
    }
}

impl Drop for ReadOnlyWindow {
    fn drop(&mut self) {
        self.unbind_framebuffer();
    }
}

pub struct WindowInfo<'a, 'b, T: 'b + Layout> {
    pub window: &'b mut FakeWindow<T>,
    pub resources: &'a AppResources,
}

impl<T: Layout> fmt::Debug for FakeWindow<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
            "FakeWindow {{\
                state: {:?}, \
                read_only_window: Rc<Display>, \
            }}", self.state)
    }
}

/// Window event that is passed to the user when a callback is invoked
#[derive(Debug)]
pub struct WindowEvent<'a, T: 'a + Layout> {
    /// The ID of the window that the event was clicked on (for indexing into
    /// `app_state.windows`). `app_state.windows[event.window]` should never panic.
    pub window: usize,
    /// The ID of the node that was hit. You can use this to query information about
    /// the node, but please don't hard-code any if / else statements based on the `NodeId`
    pub hit_dom_node: NodeId,
    /// UiState containing the necessary data for testing what
    pub(crate) ui_state: &'a UiState<T>,
    pub(crate) hit_test_result: &'a HitTestResult,
    /// The (x, y) position of the mouse cursor, **relative to top left of the element that was hit**.
    pub cursor_relative_to_item: (f32, f32),
    /// The (x, y) position of the mouse cursor, **relative to top left of the window**.
    pub cursor_in_viewport: (f32, f32),
}

impl<'a, T: 'a + Layout> Clone for WindowEvent<'a, T> {
    fn clone(&self) -> Self {
        Self {
            window: self.window,
            hit_dom_node: self.hit_dom_node,
            ui_state: self.ui_state,
            hit_test_result: self.hit_test_result,
            cursor_relative_to_item: self.cursor_relative_to_item,
            cursor_in_viewport: self.cursor_in_viewport,
        }
    }
}

impl<'a, T: 'a + Layout> Copy for WindowEvent<'a, T> { }


pub struct IndexPathIterator<'a, 'b: 'a, T: 'b + Layout> {
    current_item: NodeId,
    event: &'a WindowEvent<'b, T>,
}

impl<'a, 'b, T: 'a + Layout> IndexPathIterator<'a, 'b, T> {
    /// Returns what node ID the iterator is currently processing
    pub fn current_node(&self) -> NodeId {
        self.current_item
    }
}

impl<'a, 'b, T: 'a + Layout> Iterator for IndexPathIterator<'a, 'b, T> {
    type Item = usize;

    /// For each item in the current item path, returns the index of the item in the parent
    fn next(&mut self) -> Option<usize> {
        let (new_index, new_parent) = self.event.get_index_in_parent(self.current_item)?;
        self.current_item = new_parent;
        Some(new_index)
    }
}

impl<'a, T: 'a + Layout> WindowEvent<'a, T> {

    /// Creates an iterator that starts at the current DOM node and continouusly
    /// returns the index in the parent, until it gets to the root component.
    pub fn index_path_iter<'b>(&'b self) -> IndexPathIterator<'a, 'b, T> {
        IndexPathIterator {
            current_item: self.hit_dom_node,
            event: &self,
        }
    }

    /// For any node ID, returns what the position in its parent it is, plus the parent itself.
    /// Returns `None` on the root ID (because the root has no parent, therefore it's the 1st item)
    ///
    /// Note: Index is 0-based (first item has the index of 0)
    pub fn get_index_in_parent(&self, node_id: NodeId) -> Option<(usize, NodeId)> {
        let arena_borrow = self.ui_state.dom.arena.borrow();
        let node_layout = &arena_borrow.node_layout;

        if node_id.index() > node_layout.len() {
            return None; // node_id out of range
        }

        let parent = node_layout[node_id].parent?;
        Some((node_id.preceding_siblings(&node_layout).count() - 1, parent))
    }
}

/// Options on how to initially create the window
#[derive(Debug, Clone)]
pub struct WindowCreateOptions<T: Layout> {
    /// State of the window, set the initial title / width / height here.
    pub state: WindowState,
    /// OpenGL clear color
    pub background: ColorF,
    /// Clear the stencil buffer with the given value. If not set, stencil buffer is not cleared
    pub clear_stencil: Option<i32>,
    /// Clear the depth buffer with the given value. If not set, depth buffer is not cleared
    pub clear_depth: Option<f32>,
    /// How should the screen be updated - as fast as possible
    /// or retained & energy saving?
    pub update_mode: UpdateMode,
    /// Which monitor should the window be created on?
    pub monitor: WindowMonitorTarget,
    /// How precise should the mouse updates be?
    pub mouse_mode: MouseMode,
    /// Should the window update regardless if the mouse is hovering
    /// over the window? (useful for games vs. applications)
    pub update_behaviour: UpdateBehaviour,
    /// Renderer type: Hardware-with-software-fallback, pure software or pure hardware renderer?
    pub renderer_type: RendererType,
    /// Win32 menu callbacks
    pub menu_callbacks: FastHashMap<u16, Callback<T>>,
    /// Sets the window icon (Windows and Linux only). Usually 16x16 px or 32x32px
    pub window_icon: Option<Icon>,
    /// Windows only: Sets the 256x256 taskbar icon during startup
    pub taskbar_icon: Option<Icon>,
    /// Windows only: Sets `WS_EX_NOREDIRECTIONBITMAP` on the window
    pub no_redirection_bitmap: bool,
}

impl<T: Layout> Default for WindowCreateOptions<T> {
    fn default() -> Self {
        Self {
            state: WindowState::default(),
            background: ColorF::new(1.0, 1.0, 1.0, 1.0),
            clear_stencil: None,
            clear_depth: None,
            update_mode: UpdateMode::default(),
            monitor: WindowMonitorTarget::default(),
            mouse_mode: MouseMode::default(),
            update_behaviour: UpdateBehaviour::default(),
            renderer_type: RendererType::default(),
            menu_callbacks: FastHashMap::default(),
            window_icon: None,
            taskbar_icon: None,
            no_redirection_bitmap: false,
        }
    }
}

/// Force a specific renderer.
/// By default, Azul will try to use the hardware renderer and fall
/// back to the software renderer if it can't create an OpenGL 3.2 context.
/// However, in some cases a hardware renderer might create problems
/// or you want to force either a software or hardware renderer.
///
/// If the field `renderer_type` on the `WindowCreateOptions` is not
/// `RendererType::Default`, the `create_window` method will try to create
/// a window with the specific renderer type and **crash** if the renderer is
/// not available for whatever reason.
///
/// If you don't know what any of this means, leave it at `Default`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RendererType {
    Default,
    Hardware,
    Software,
}

impl Default for RendererType {
    fn default() -> Self {
        RendererType::Default
    }
}

/// Should the window be updated only if the mouse cursor is hovering over it?
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum UpdateBehaviour {
    /// Redraw the window only if the mouse cursor is
    /// on top of the window
    UpdateOnHover,
    /// Always update the screen, regardless of the
    /// position of the mouse cursor
    UpdateAlways,
}

impl Default for UpdateBehaviour {
    fn default() -> Self {
        UpdateBehaviour::UpdateOnHover
    }
}

/// In which intervals should the screen be updated
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum UpdateMode {
    /// Retained = the screen is only updated when necessary.
    /// Underlying GlImages will be ignored and only updated when the UI changes
    Retained,
    /// Fixed update every X duration.
    FixedUpdate(Duration),
    /// Draw the screen as fast as possible.
    AsFastAsPossible,
}

impl Default for UpdateMode {
    fn default() -> Self {
        UpdateMode::Retained
    }
}

/// Mouse configuration
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum MouseMode {
    /// A mouse event is only fired if the cursor has moved at least 1px.
    /// More energy saving, but less precision.
    Normal,
    /// High-precision mouse input (useful for games)
    ///
    /// This disables acceleration and uses the raw values
    /// provided by the mouse.
    DirectInput,
}

impl Default for MouseMode {
    fn default() -> Self {
        MouseMode::Normal
    }
}

/// Error that could happen during window creation
#[derive(Debug)]
pub enum WindowCreateError {
    /// WebGl is not supported by WebRender
    WebGlNotSupported,
    /// Couldn't create the display from the window and the EventsLoop
    DisplayCreateError(DisplayCreationError),
    /// OpenGL version is either too old or invalid
    Gl(IncompatibleOpenGl),
    /// Could not create an OpenGL context
    Context(ContextError),
    /// Could not create a window
    CreateError(CreationError),
    /// Could not swap the front & back buffers
    SwapBuffers(::glium::SwapBuffersError),
    /// IO error
    Io(::std::io::Error),
    /// WebRender creation error (probably OpenGL missing?)
    Renderer/*(RendererError)*/,
}

impl_display! {
    WindowCreateError,
    {
        DisplayCreateError(e) => format!("Could not create the display from the window and the EventsLoop: {}", e),
        Gl(e) => format!("{}", e),
        Context(e) => format!("{}", e),
        CreateError(e) => format!("{}", e),
        SwapBuffers(e) => format!("{}", e),
        Io(e) => format!("{}", e),
        WebGlNotSupported => "WebGl is not supported by WebRender",
        Renderer => "Webrender creation error (probably OpenGL missing?)",
    }
}

impl_from!(SwapBuffersError, WindowCreateError::SwapBuffers);
impl_from!(CreationError, WindowCreateError::CreateError);
impl_from!(IoError, WindowCreateError::Io);
impl_from!(IncompatibleOpenGl, WindowCreateError::Gl);
impl_from!(DisplayCreationError, WindowCreateError::DisplayCreateError);
impl_from!(ContextError, WindowCreateError::Context);

struct Notifier {
    events_loop_proxy: EventsLoopProxy,
}

impl Notifier {
    fn new(events_loop_proxy: EventsLoopProxy) -> Notifier {
        Notifier {
            events_loop_proxy
        }
    }
}

impl RenderNotifier for Notifier {
    fn clone(&self) -> Box<RenderNotifier> {
        Box::new(Notifier {
            events_loop_proxy: self.events_loop_proxy.clone(),
        })
    }

    fn wake_up(&self) {
        #[cfg(not(target_os = "android"))]
        self.events_loop_proxy.wakeup().unwrap_or_else(|_| {
            #[cfg(feature = "logging")] {
                error!("couldn't wakeup event loop");
            }
        });
    }

    fn new_frame_ready(&self, _id: DocumentId, _scrolled: bool, _composite_needed: bool, _render_time: Option<u64>) {
        self.wake_up();
    }
}

/// Iterator over connected monitors (for positioning, etc.)
pub struct MonitorIter {
    inner: AvailableMonitorsIter,
}

impl Iterator for MonitorIter {
    type Item = MonitorId;
    fn next(&mut self) -> Option<MonitorId> {
        self.inner.next()
    }
}

/// Select on which monitor the window should pop up.
#[derive(Clone)]
pub enum WindowMonitorTarget {
    /// Window should appear on the primary monitor
    Primary,
    /// Use `Window::get_available_monitors()` to select the correct monitor
    Custom(MonitorId)
}

impl fmt::Debug for WindowMonitorTarget {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::WindowMonitorTarget::*;
        match *self {
            Primary =>  write!(f, "WindowMonitorTarget::Primary"),
            Custom(_) =>  write!(f, "WindowMonitorTarget::Custom(_)"),
        }
    }
}

impl Default for WindowMonitorTarget {
    fn default() -> Self {
        WindowMonitorTarget::Primary
    }
}

/// Represents one graphical window to be rendered
pub struct Window<T: Layout> {
    // TODO: technically, having one EventsLoop for all windows is sufficient
    pub(crate) events_loop: EventsLoop,
    /// Current state of the window, stores the keyboard / mouse state,
    /// visibility of the window, etc. of the LAST frame. The user never sets this
    /// field directly, but rather sets the WindowState he wants to have for the NEXT frame,
    /// then azul compares the changes (i.e. if we are currently in fullscreen mode and
    /// the user wants the next screen to be in fullscreen mode, too, simply do nothing), then it
    /// updates this field to reflect the changes.
    ///
    /// This field is initialized from the `WindowCreateOptions`.
    pub(crate) state: WindowState,
    /// The WebRender renderer
    pub(crate) renderer: Option<Renderer>,
    /// The display, i.e. the window
    pub(crate) display: Rc<Display>,
    /// The `WindowInternal` allows us to solve some borrowing issues
    pub(crate) internal: WindowInternal,
    /// Currently running animations / transitions
    pub(crate) animations: FastHashMap<DaemonId, Daemon<AnimationState>>,
    /// States of scrolling animations, updated every frame
    pub(crate) scroll_states: ScrollStates,
    // The background thread that is running for this window.
    // pub(crate) background_thread: Option<JoinHandle<()>>,
    /// The style applied to the current window
    pub(crate) style: Css,
    /// An optional style hot-reloader for the current window, only available with debug_assertions
    /// enabled
    #[cfg(debug_assertions)]
    pub(crate) style_loader: Option<Box<dyn HotReloadHandler>>,
    /// Purely a marker, so that `app.run()` can infer the type of `T: Layout`
    /// of the `WindowCreateOptions`, so that we can write:
    ///
    /// ```rust,ignore
    /// app.run(Window::new(WindowCreateOptions::new(), Css::default()).unwrap());
    /// ```
    ///
    /// instead of having to annotate the type:
    ///
    /// ```rust,ignore
    /// app.run(Window::new(WindowCreateOptions::<MyAppData>::new(), Css::default()).unwrap());
    /// ```
    marker: PhantomData<T>,
}

#[derive(Debug, Copy, Clone)]
pub struct AnimationState { }

pub(crate) struct ScrollStates(pub(crate) FastHashMap<ExternalScrollId, ScrollState>);

impl ScrollStates {
    pub fn new() -> ScrollStates {
        ScrollStates(FastHashMap::default())
    }

    /// NOTE: This has to be a getter, because we need to update
    #[must_use]
    pub(crate) fn get_scroll_amount(&mut self, scroll_id: &ExternalScrollId) -> Option<(f32, f32)> {
        let entry = self.0.get_mut(&scroll_id)?;
        Some(entry.get())
    }

    /// Updating the scroll amount does not update the `entry.used_this_frame`,
    /// since that is only relevant when we are actually querying the renderer.
    pub(crate) fn scroll_node(&mut self, scroll_id: &ExternalScrollId, scroll_by_x: f32, scroll_by_y: f32) {
        if let Some(entry) = self.0.get_mut(scroll_id) {
            entry.add(scroll_by_x, scroll_by_y);
        }
    }

    pub(crate) fn ensure_initialized_scroll_state(&mut self, scroll_id: ExternalScrollId, overflow_x: f32, overflow_y: f32) {
        self.0.entry(scroll_id).or_insert_with(|| ScrollState::new(overflow_x, overflow_y));
    }

    /// Removes all scroll states that weren't used in the last frame
    pub(crate) fn remove_unused_scroll_states(&mut self) {
        self.0.retain(|_, state| state.used_this_frame);
    }
}

#[derive(Debug, Copy, Clone)]
pub struct ScrollState {
    /// Amount in pixel that the current node is scrolled
    scroll_amount_x: f32,
    scroll_amount_y: f32,
    overflow_x: f32,
    overflow_y: f32,
    /// Was the scroll amount used in this frame?
    used_this_frame: bool,
}

impl ScrollState {
    fn new(overflow_x: f32, overflow_y: f32) -> Self {
        ScrollState {
            scroll_amount_x: 0.0,
            scroll_amount_y: 0.0,
            overflow_x: overflow_x,
            overflow_y: overflow_y,
            used_this_frame: true,
        }
    }

    pub fn get(&mut self) -> (f32, f32) {
        self.used_this_frame = true;
        (self.scroll_amount_x, self.scroll_amount_y)
    }

    pub fn add(&mut self, x: f32, y: f32) {
        self.scroll_amount_x = self.overflow_x.min(self.scroll_amount_x + x).max(0.0);
        self.scroll_amount_y = self.overflow_y.min(self.scroll_amount_y + y).max(0.0);
    }
}

impl Default for ScrollState {
    fn default() -> Self {
        ScrollState {
            scroll_amount_x: 0.0,
            scroll_amount_y: 0.0,
            overflow_x: 0.0,
            overflow_y: 0.0,
            used_this_frame: true,
        }
    }
}

pub(crate) struct WindowInternal {
    pub(crate) last_display_list_builder: BuiltDisplayList,
    pub(crate) last_scrolled_nodes: ScrolledNodes,
    pub(crate) api: RenderApi,
    pub(crate) epoch: Epoch,
    pub(crate) pipeline_id: PipelineId,
    pub(crate) document_id: DocumentId,
}

impl<'a, T: Layout> Window<T> {

    /// Creates a new window
    pub fn new(mut options: WindowCreateOptions<T>, style: Css) -> Result<Self, WindowCreateError> {

        use self::RendererType::*;
        use webrender::WrShaders;

        let events_loop = EventsLoop::new();

        let monitor = match options.monitor {
            WindowMonitorTarget::Primary => events_loop.get_primary_monitor(),
            WindowMonitorTarget::Custom(ref id) => id.clone(),
        };

        let hidpi_factor = monitor.get_hidpi_factor();
        options.state.size.hidpi_factor = hidpi_factor;

        let mut window = WindowBuilder::new()
            .with_title(options.state.title.clone())
            .with_maximized(options.state.is_maximized)
            .with_decorations(options.state.has_decorations)
            .with_visibility(options.state.is_visible)
            .with_transparency(options.state.is_transparent)
            .with_multitouch();
/*
        events_loop.create_proxy().execute_in_thread(|_| {

        });
*/
        // TODO: Update winit to have:
        //      .with_always_on_top(options.state.is_always_on_top)
        //
        // winit 0.13 -> winit 0.15

        // TODO: Add all the extensions for X11 / Mac / Windows,
        // like setting the taskbar icon, setting the titlebar icon, etc.

        if let Some(icon) = options.window_icon {
            window = window.with_window_icon(Some(icon));
        }

        #[cfg(target_os = "windows")] {
            if let Some(icon) = options.taskbar_icon {
                use glium::glutin::os::windows::WindowBuilderExt;
                window = window.with_taskbar_icon(Some(icon));
            }
        }

        #[cfg(target_os = "windows")] {
            if options.no_redirection_bitmap {
                use glium::glutin::os::windows::WindowBuilderExt;
                window = window.with_no_redirection_bitmap(true);
            }
        }

        if options.state.is_fullscreen {
            window = window.with_fullscreen(Some(monitor));
        }

        if let Some(min_dim) = options.state.size.min_dimensions {
            window = window.with_min_dimensions(min_dim);
        }

        if let Some(max_dim) = options.state.size.max_dimensions {
            window = window.with_max_dimensions(max_dim);
        }

        fn create_context_builder<'a>(vsync: bool, srgb: bool) -> ContextBuilder<'a> {
            // See #33 - specifying a specific OpenGL version
            // makes winit crash on older Intel drivers, which is why we
            // don't specify a specific OpenGL version here
            let mut builder = ContextBuilder::new();

            /*#[cfg(debug_assertions)] {
                builder = builder.with_gl_debug_flag(true);
            }

            #[cfg(not(debug_assertions))] {*/
                builder = builder.with_gl_debug_flag(false);
            // }

            if vsync {
                builder = builder.with_vsync(true);
            }
            if srgb {
                builder = builder.with_srgb(true);
            }

            builder
        }

        // Only create a context with VSync and SRGB if the context creation works
        let gl_window = GlWindow::new(window.clone(), create_context_builder(true, true), &events_loop)
            .or_else(|_| GlWindow::new(window.clone(), create_context_builder(true, false), &events_loop))
            .or_else(|_| GlWindow::new(window.clone(), create_context_builder(false, true), &events_loop))
            .or_else(|_| GlWindow::new(window, create_context_builder(false, false), &events_loop))?;

        if let Some(pos) = options.state.position {
            gl_window.window().set_position(pos);
        }

        if options.state.is_maximized && !options.state.is_fullscreen {
            gl_window.window().set_maximized(true);
        } else if !options.state.is_fullscreen {
            gl_window.window().set_inner_size(options.state.size.dimensions);
        }

        /*#[cfg(debug_assertions)]
        let display = Display::with_debug(gl_window, DebugCallbackBehavior::DebugMessageOnError)?;
        #[cfg(not(debug_assertions))]*/
        let display = Display::with_debug(gl_window, DebugCallbackBehavior::Ignore)?;

        let device_pixel_ratio = display.gl_window().get_hidpi_factor();

        // pre-caching shaders means to compile all shaders on startup
        // this can take significant time and should be only used for testing the shaders
        const PRECACHE_SHADER_FLAGS: ShaderPrecacheFlags = ShaderPrecacheFlags::EMPTY;

        // this exists because RendererOptions isn't Clone-able
        fn get_renderer_opts(native: bool, device_pixel_ratio: f32, clear_color: Option<ColorF>) -> RendererOptions {
            use webrender::ProgramCache;
            RendererOptions {
                resource_override_path: None,
                precache_flags: PRECACHE_SHADER_FLAGS,
                device_pixel_ratio: device_pixel_ratio,
                enable_subpixel_aa: true,
                enable_aa: true,
                clear_color: clear_color,
                cached_programs: Some(ProgramCache::new(None)),
                renderer_kind: if native {
                    RendererKind::Native
                } else {
                    RendererKind::OSMesa
                },
                .. RendererOptions::default()
            }
        }

        let framebuffer_size = {
            let (width, height): (u32, u32) = display.gl_window().get_inner_size().unwrap().to_physical(hidpi_factor).into();
            DeviceIntSize::new(width as i32, height as i32)
        };

        let notifier = Box::new(Notifier::new(events_loop.create_proxy()));

        let gl = get_gl_context(&display)?;

        let opts_native = get_renderer_opts(true, device_pixel_ratio as f32, Some(options.background));
        let opts_osmesa = get_renderer_opts(false, device_pixel_ratio as f32, Some(options.background));

        // TODO: Right now it's not very ergonomic to cache shaders between
        // renderers - notify webrender about this.
        const WR_SHADER_CACHE: Option<&mut WrShaders> = None;

        let (mut renderer, sender) = match options.renderer_type {
            Hardware => {
                // force hardware renderer
                Renderer::new(gl, notifier, opts_native, WR_SHADER_CACHE).unwrap()
            },
            Software => {
                // force software renderer
                Renderer::new(gl, notifier, opts_osmesa, WR_SHADER_CACHE).unwrap()
            },
            Default => {
                // try hardware first, fall back to software
                if let Ok(r) = Renderer::new(gl.clone(), notifier.clone(), opts_native, WR_SHADER_CACHE) {
                    r
                } else {
                    Renderer::new(gl, notifier, opts_osmesa, WR_SHADER_CACHE).unwrap()
                }
            }
        };

        let api = sender.create_api();
        let document_id = api.add_document(framebuffer_size, 0);
        let epoch = Epoch(0);
        let pipeline_id = PipelineId(0, 0);

        /*
        let (sender, receiver) = channel();
        let thread = Builder::new().name(options.title.clone()).spawn(move || Self::handle_event(receiver))?;
        */

        renderer.set_external_image_handler(Box::new(Compositor::default()));

        set_webrender_debug_flags(&mut renderer, &DebugState::default(), &options.state.debug_state);

        let window = Window {
            events_loop: events_loop,
            state: options.state,
            renderer: Some(renderer),
            display: Rc::new(display),
            style: sort_by_specificity(style),
            #[cfg(debug_assertions)]
            style_loader: None,
            animations: FastHashMap::default(),
            scroll_states: ScrollStates::new(),
            internal: WindowInternal {
                api: api,
                epoch: epoch,
                pipeline_id: pipeline_id,
                document_id: document_id,
                last_display_list_builder: BuiltDisplayList::default(),
                last_scrolled_nodes: ScrolledNodes::default(),
            },
            marker: PhantomData,
        };

        Ok(window)
    }

    /// Creates a new window that will automatically load a new style from a given HotReloadHandler.
    /// Only available with debug_assertions enabled.
    #[cfg(debug_assertions)]
    pub fn new_hot_reload(options: WindowCreateOptions<T>, style_loader: Box<dyn HotReloadHandler>) -> Result<Self, WindowCreateError>  {
        let mut window = Window::new(options, Css::default())?;
        window.style_loader = Some(style_loader);
        Ok(window)
    }

    /// Returns an iterator over all given monitors
    pub fn get_available_monitors() -> MonitorIter {
        MonitorIter {
            inner: EventsLoop::new().get_available_monitors(),
        }
    }

    /// Updates the window state, diff the `self.state` with the `new_state`
    /// and updating the platform window to reflect the changes
    ///
    /// Note: Currently, setting `mouse_state.position`, `window.size` or
    /// `window.position` has no effect on the platform window, since they are very
    /// frequently modified by the user (other properties are always set by the
    /// application developer)
    pub(crate) fn update_from_user_window_state(&mut self, new_state: WindowState) {

        let gl_window = self.display.gl_window();
        let window = gl_window.window();
        let old_state = &mut self.state;

        // Compare the old and new state, field by field
        if let Some(r) = &mut self.renderer {
            set_webrender_debug_flags(r, &old_state.debug_state, &new_state.debug_state);
        }

        if old_state.title != new_state.title {
            window.set_title(&new_state.title);
            old_state.title = new_state.title;
        }

        if old_state.mouse_state.mouse_cursor_type != new_state.mouse_state.mouse_cursor_type {
            window.set_cursor(new_state.mouse_state.mouse_cursor_type);
            old_state.mouse_state.mouse_cursor_type = new_state.mouse_state.mouse_cursor_type;
        }

        if old_state.is_maximized != new_state.is_maximized {
            window.set_maximized(new_state.is_maximized);
            old_state.is_maximized = new_state.is_maximized;
        }

        if old_state.is_fullscreen != new_state.is_fullscreen {
            if new_state.is_fullscreen {
                window.set_fullscreen(Some(window.get_current_monitor()));
            } else {
                window.set_fullscreen(None);
            }
            old_state.is_fullscreen = new_state.is_fullscreen;
        }

        if old_state.has_decorations != new_state.has_decorations {
            window.set_decorations(new_state.has_decorations);
            old_state.has_decorations = new_state.has_decorations;
        }

        if old_state.is_visible != new_state.is_visible {
            if new_state.is_visible {
                window.show();
            } else {
                window.hide();
            }
            old_state.is_visible = new_state.is_visible;
        }

        if old_state.size.min_dimensions != new_state.size.min_dimensions {
            window.set_min_dimensions(new_state.size.min_dimensions.and_then(|dim| Some(dim.into())));
            old_state.size.min_dimensions = new_state.size.min_dimensions;
        }

        if old_state.size.max_dimensions != new_state.size.max_dimensions {
            window.set_max_dimensions(new_state.size.max_dimensions.and_then(|dim| Some(dim.into())));
            old_state.size.max_dimensions = new_state.size.max_dimensions;
        }
    }

    pub(crate) fn update_from_external_window_state(&mut self, frame_event_info: &mut FrameEventInfo) {

        if let Some(new_size) = frame_event_info.new_window_size {
            self.state.size.dimensions = new_size;
            frame_event_info.should_redraw_window = true;
        }

        if let Some(dpi) = frame_event_info.new_dpi_factor {
            self.state.size.hidpi_factor = dpi;
            frame_event_info.should_redraw_window = true;
        }
    }

    /// Resets the mouse states `scroll_x` and `scroll_y` to 0
    pub(crate) fn clear_scroll_state(&mut self) {
        self.state.mouse_state.scroll_x = 0.0;
        self.state.mouse_state.scroll_y = 0.0;
    }
}

pub(crate) fn get_gl_context(display: &Display) -> Result<Rc<Gl>, WindowCreateError> {
    match display.gl_window().get_api() {
        glutin::Api::OpenGl => Ok(unsafe {
            gl::GlFns::load_with(|symbol| display.gl_window().get_proc_address(symbol) as *const _)
        }),
        glutin::Api::OpenGlEs => Ok(unsafe {
            gl::GlesFns::load_with(|symbol| display.gl_window().get_proc_address(symbol) as *const _)
        }),
        glutin::Api::WebGl => Err(WindowCreateError::WebGlNotSupported),
    }
}

impl<T: Layout> Drop for Window<T> {
    fn drop(&mut self) {
        // self.background_thread.take().unwrap().join();
        let renderer = self.renderer.take().unwrap();
        renderer.deinit();
    }
}

// Only necessary for GlTextures and IFrames that need the
// width and height of their container to calculate their content
#[derive(Debug, Copy, Clone)]
pub struct HidpiAdjustedBounds {
    pub logical_size: LogicalSize,
    pub physical_size: PhysicalSize,
    pub hidpi_factor: f64,
}

impl HidpiAdjustedBounds {
    pub fn from_bounds<T: Layout>(fake_window: &FakeWindow<T>, bounds: LayoutRect) -> Self {
        let hidpi_factor = fake_window.read_only_window().get_hidpi_factor();
        let logical_size = LogicalSize::new(bounds.size.width as f64, bounds.size.height as f64);
        let physical_size = logical_size.to_physical(hidpi_factor);

        Self {
            logical_size,
            physical_size,
            hidpi_factor,
        }
    }
}


fn set_webrender_debug_flags(r: &mut Renderer, old_flags: &DebugState, new_flags: &DebugState) {

    use webrender::DebugFlags;

    if old_flags.profiler_dbg != new_flags.profiler_dbg {
        r.set_debug_flag(DebugFlags::PROFILER_DBG, new_flags.profiler_dbg);
    }
    if old_flags.render_target_dbg != new_flags.render_target_dbg {
        r.set_debug_flag(DebugFlags::RENDER_TARGET_DBG, new_flags.render_target_dbg);
    }
    if old_flags.texture_cache_dbg != new_flags.texture_cache_dbg {
        r.set_debug_flag(DebugFlags::TEXTURE_CACHE_DBG, new_flags.texture_cache_dbg);
    }
    if old_flags.gpu_time_queries != new_flags.gpu_time_queries {
        r.set_debug_flag(DebugFlags::GPU_TIME_QUERIES, new_flags.gpu_time_queries);
    }
    if old_flags.gpu_sample_queries != new_flags.gpu_sample_queries {
        r.set_debug_flag(DebugFlags::GPU_SAMPLE_QUERIES, new_flags.gpu_sample_queries);
    }
    if old_flags.disable_batching != new_flags.disable_batching {
        r.set_debug_flag(DebugFlags::DISABLE_BATCHING, new_flags.disable_batching);
    }
    if old_flags.epochs != new_flags.epochs {
        r.set_debug_flag(DebugFlags::EPOCHS, new_flags.epochs);
    }
    if old_flags.compact_profiler != new_flags.compact_profiler {
        r.set_debug_flag(DebugFlags::COMPACT_PROFILER, new_flags.compact_profiler);
    }
    if old_flags.echo_driver_messages != new_flags.echo_driver_messages {
        r.set_debug_flag(DebugFlags::ECHO_DRIVER_MESSAGES, new_flags.echo_driver_messages);
    }
    if old_flags.new_frame_indicator != new_flags.new_frame_indicator {
        r.set_debug_flag(DebugFlags::NEW_FRAME_INDICATOR, new_flags.new_frame_indicator);
    }
    if old_flags.new_scene_indicator != new_flags.new_scene_indicator {
        r.set_debug_flag(DebugFlags::NEW_SCENE_INDICATOR, new_flags.new_scene_indicator);
    }
    if old_flags.show_overdraw != new_flags.show_overdraw {
        r.set_debug_flag(DebugFlags::SHOW_OVERDRAW, new_flags.show_overdraw);
    }
    if old_flags.gpu_cache_dbg != new_flags.gpu_cache_dbg {
        r.set_debug_flag(DebugFlags::GPU_CACHE_DBG, new_flags.gpu_cache_dbg);
    }
}
