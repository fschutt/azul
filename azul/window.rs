use std::{
    rc::Rc,
    marker::PhantomData,
    io::Error as IoError,
    collections::BTreeMap,
};
use webrender::{
    api::{
        Epoch, DocumentId, RenderApi, RenderNotifier, DeviceIntSize,
    },
    Renderer, RendererOptions, RendererKind, ShaderPrecacheFlags, WrShaders,
    // renderer::RendererError; -- not currently public in WebRender
};
use glium::{
    IncompatibleOpenGl, Display, SwapBuffersError,
    debug::DebugCallbackBehavior,
    glutin::{
        EventsLoop, ContextTrait, CombinedContext, CreationError,
        MonitorId, ContextError, ContextBuilder, Window as GliumWindow,
        WindowBuilder as GliumWindowBuilder, Context,
    },
    backend::glutin::DisplayCreationError,
};
use gleam::gl::{self, Gl};
use clipboard2::{Clipboard as _, ClipboardError, SystemClipboard};
use azul_css::{Css, ColorU, LayoutPoint, LayoutRect};
#[cfg(debug_assertions)]
use azul_css::HotReloadHandler;
use {
    FastHashMap,
    compositor::Compositor,
    app::FrameEventInfo,
    callbacks::{PipelineId, ScrollPosition},
    dom::{NodeId, DomId},
};
use azul_core::{
    ui_state::UiState,
    display_list::CachedDisplayList,
    ui_solver::{ScrolledNodes, ExternalScrollId, LayoutResult, OverflowingScrollNode},
    window::WindowId,
};
pub use webrender::api::HitTestItem;
pub use glium::glutin::AvailableMonitorsIter;
pub use azul_core::window::*;
pub use window_state::*;

// TODO: Right now it's not very ergonomic to cache shaders between
// renderers - notify webrender about this.
const WR_SHADER_CACHE: Option<&mut WrShaders> = None;

/// Options on how to initially create the window
#[derive(Debug, Clone, PartialEq)]
pub struct WindowCreateOptions {
    /// State of the window, set the initial title / width / height here.
    pub state: WindowState,
    /// Which monitor should the window be created on?
    pub monitor: WindowMonitorTarget,
    /// Renderer type: Hardware-with-software-fallback, pure software or pure hardware renderer?
    pub renderer_type: RendererType,
    /// Sets the window icon (Windows and Linux only). Usually 16x16 px or 32x32px
    pub window_icon: Option<WindowIcon>,
    /// Windows only: Sets the 256x256 taskbar icon during startup
    pub taskbar_icon: Option<TaskBarIcon>,
}

impl Default for WindowCreateOptions {
    fn default() -> Self {
        Self {
            state: WindowState::default(),
            monitor: WindowMonitorTarget::default(),
            renderer_type: RendererType::default(),
            window_icon: None,
            taskbar_icon: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WindowIcon {
    /// 16x16x3 bytes icon
    Small(Vec<u8>),
    /// 32x32 bytes icon
    Large(Vec<u8>),
}

/// 256x256x3 window icon
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskBarIcon(pub Vec<u8>);

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
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

impl_display! {WindowCreateError, {
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

struct Notifier { }

impl RenderNotifier for Notifier {
    fn clone(&self) -> Box<RenderNotifier> {
        Box::new(Notifier { })
    }

    // NOTE: Rendering is single threaded (because that's the nature of OpenGL),
    // so when the Renderer::render() function is finished, then the rendering
    // is finished and done, the rendering is currently blocking (but only takes about 0..
    // There is no point in implementing RenderNotifier, it only leads to
    // synchronization problems (when handling Event::Awakened).

    fn wake_up(&self) { }
    fn new_frame_ready(&self, _id: DocumentId, _scrolled: bool, _composite_needed: bool, _render_time: Option<u64>) { }
}

/// Select on which monitor the window should pop up.
#[derive(Debug, Clone)]
pub enum WindowMonitorTarget {
    /// Window should appear on the primary monitor
    Primary,
    /// Use `Window::get_available_monitors()` to select the correct monitor
    Custom(MonitorId)
}

#[cfg(target_os = "linux")]
type NativeMonitorId = u32;
// HMONITOR, (*mut c_void), casted to a usize
#[cfg(target_os = "windows")]
type NativeMonitorId = usize;
#[cfg(target_os = "macos")]
type NativeMonitorId = u32;

impl WindowMonitorTarget {
    fn get_native_id(&self) -> Option<NativeMonitorId> {

        use self::WindowMonitorTarget::*;

        #[cfg(target_os = "linux")]
        use glium::glutin::os::unix::MonitorIdExt;
        #[cfg(target_os = "windows")]
        use glium::glutin::os::windows::MonitorIdExt;
        #[cfg(target_os = "macos")]
        use glium::glutin::os::macos::MonitorIdExt;

        match self {
            Primary => None,
            Custom(m) => Some({
                #[cfg(target_os = "windows")] { m.hmonitor() as usize }
                #[cfg(target_os = "linux")] { m.native_id() }
                #[cfg(target_os = "macos")] { m.native_id() }
            }),
        }
    }
}

impl ::std::hash::Hash for WindowMonitorTarget {
    fn hash<H>(&self, state: &mut H) where H: ::std::hash::Hasher {
        use self::WindowMonitorTarget::*;
        state.write_usize(match self { Primary => 0, Custom(_) => 1, });
        state.write_usize(self.get_native_id().unwrap_or(0) as usize);
    }
}

impl PartialEq for WindowMonitorTarget {
    fn eq(&self, rhs: &Self) -> bool {
        self.get_native_id() == rhs.get_native_id()
    }
}

impl PartialOrd for WindowMonitorTarget {
    fn partial_cmp(&self, other: &Self) -> Option<::std::cmp::Ordering> {
        Some((self.get_native_id()).cmp(&(other.get_native_id())))
    }
}

impl Ord for WindowMonitorTarget {
    fn cmp(&self, other: &Self) -> ::std::cmp::Ordering {
        (self.get_native_id()).cmp(&(other.get_native_id()))
    }
}

impl Eq for WindowMonitorTarget { }

impl Default for WindowMonitorTarget {
    fn default() -> Self {
        WindowMonitorTarget::Primary
    }
}

/// Represents one graphical window to be rendered
pub struct Window<T> {
    /// System that can identify this window
    pub(crate) id: WindowId,
    /// Stores the create_options: necessary because by default, the window is hidden
    /// and only gets shown after the first redraw.
    pub(crate) create_options: WindowCreateOptions,
    /// Current state of the window, stores the keyboard / mouse state,
    /// visibility of the window, etc. of the LAST frame. The user never sets this
    /// field directly, but rather sets the WindowState he wants to have for the NEXT frame,
    /// then azul compares the changes (i.e. if we are currently in fullscreen mode and
    /// the user wants the next screen to be in fullscreen mode, too, simply do nothing), then it
    /// updates this field to reflect the changes.
    ///
    /// This field is initialized from the `WindowCreateOptions`.
    pub(crate) state: WindowState,
    /// The display, i.e. the window
    pub(crate) display: Rc<Display>,
    /// The `WindowInternal` allows us to solve some borrowing issues
    pub(crate) internal: WindowInternal,
    // The background thread that is running for this window.
    // pub(crate) background_thread: Option<JoinHandle<()>>,
    /// The style applied to the current window
    pub(crate) css: Css,
    /// An optional style hot-reloader for the current window, only available with debug_assertions
    /// enabled
    #[cfg(debug_assertions)]
    pub(crate) css_loader: Option<Box<dyn HotReloadHandler>>,
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

#[derive(Debug, Default)]
pub(crate) struct ScrollStates(pub(crate) FastHashMap<ExternalScrollId, ScrollState>);

impl ScrollStates {

    pub(crate) fn new() -> ScrollStates {
        ScrollStates::default()
    }

    #[must_use]
    pub(crate) fn get_scroll_position(&self, scroll_id: &ExternalScrollId) -> Option<LayoutPoint> {
        self.0.get(&scroll_id).map(|entry| entry.get())
    }

    /// Set the scroll amount - does not update the `entry.used_this_frame`,
    /// since that is only relevant when we are actually querying the renderer.
    pub(crate) fn set_scroll_position(&mut self, node: &OverflowingScrollNode, scroll_position: LayoutPoint) {
        self.0.entry(node.parent_external_scroll_id)
        .or_insert_with(|| ScrollState::default())
        .set(scroll_position.x, scroll_position.y, &node.child_rect);
    }

    /// NOTE: This has to be a getter, because we need to update
    #[must_use]
    pub(crate) fn get_scroll_position_and_mark_as_used(&mut self, scroll_id: &ExternalScrollId) -> Option<LayoutPoint> {
        let entry = self.0.get_mut(&scroll_id)?;
        Some(entry.get_and_mark_as_used())
    }

    /// Updating (add to) the existing scroll amount does not update the `entry.used_this_frame`,
    /// since that is only relevant when we are actually querying the renderer.
    pub(crate) fn scroll_node(&mut self, node: &OverflowingScrollNode, scroll_by_x: f32, scroll_by_y: f32) {
        self.0.entry(node.parent_external_scroll_id)
        .or_insert_with(|| ScrollState::default())
        .add(scroll_by_x, scroll_by_y, &node.child_rect);
    }

    /// Removes all scroll states that weren't used in the last frame
    pub(crate) fn remove_unused_scroll_states(&mut self) {
        self.0.retain(|_, state| state.used_this_frame);
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub(crate) struct ScrollState {
    /// Amount in pixel that the current node is scrolled
    scroll_position: LayoutPoint,
    /// Was the scroll amount used in this frame?
    used_this_frame: bool,
}

impl ScrollState {

    /// Return the current position of the scroll state
    pub(crate) fn get(&self) -> LayoutPoint {
        self.scroll_position
    }

    /// Add a scroll X / Y onto the existing scroll state
    pub(crate) fn add(&mut self, x: f32, y: f32, child_rect: &LayoutRect) {
        self.scroll_position.x = (self.scroll_position.x + x).max(0.0).min(child_rect.size.width);
        self.scroll_position.y = (self.scroll_position.y + y).max(0.0).min(child_rect.size.height);
    }

    /// Set the scroll state to a new position
    pub(crate) fn set(&mut self, x: f32, y: f32, child_rect: &LayoutRect) {
        self.scroll_position.x = x.max(0.0).min(child_rect.size.width);
        self.scroll_position.y = y.max(0.0).min(child_rect.size.height);
    }

    /// Returns the scroll position and also set the "used_this_frame" flag
    pub(crate) fn get_and_mark_as_used(&mut self) -> LayoutPoint {
        self.used_this_frame = true;
        self.scroll_position
    }
}

impl Default for ScrollState {
    fn default() -> Self {
        ScrollState {
            scroll_position: LayoutPoint::zero(),
            used_this_frame: true,
        }
    }
}

pub(crate) struct WindowInternal {
    /// Current display list active in this window (useful for debugging)
    pub(crate) cached_display_list: CachedDisplayList,
    /// Currently active, layouted rectangles
    pub(crate) layout_result: BTreeMap<DomId, LayoutResult>,
    /// Current scroll states of nodes (x and y position of where they are scrolled)
    pub(crate) scrolled_nodes: BTreeMap<DomId, ScrolledNodes>,
    /// States of scrolling animations, updated every frame
    pub(crate) scroll_states: ScrollStates,
    pub(crate) epoch: Epoch,
    pub(crate) pipeline_id: PipelineId,
    pub(crate) document_id: DocumentId,
}

impl WindowInternal {

    /// Returns a copy of the current scroll states + scroll positions
    pub(crate) fn get_current_scroll_states<T>(&self, ui_states: &BTreeMap<DomId, UiState<T>>)
    -> BTreeMap<DomId, BTreeMap<NodeId, ScrollPosition>>
    {
        self.scrolled_nodes.iter().filter_map(|(dom_id, scrolled_nodes)| {

            let layout_result = self.layout_result.get(dom_id)?;
            let ui_state = &ui_states.get(dom_id)?;

            let scroll_positions = scrolled_nodes.overflowing_nodes.iter().filter_map(|(node_id, overflowing_node)| {
                let scroll_location = self.scroll_states.get_scroll_position(&overflowing_node.parent_external_scroll_id)?;
                let parent_node = ui_state.dom.arena.node_layout[*node_id].parent.unwrap_or(NodeId::ZERO);
                let scroll_position = ScrollPosition {
                    scroll_frame_rect: overflowing_node.child_rect,
                    parent_rect: layout_result.rects[parent_node].to_layouted_rectangle(),
                    scroll_location,
                };
                Some((*node_id, scroll_position))
            }).collect();

            Some((dom_id.clone(), scroll_positions))
        }).collect()
    }
}

impl<T> Window<T> {

    /// Creates a new window
    pub(crate) fn new(
        render_api: &mut RenderApi,
        shared_context: &Context,
        events_loop: &EventsLoop,
        options: WindowCreateOptions,
        mut css: Css,
        background_color: ColorU,
    ) -> Result<Self, WindowCreateError> {

        use wr_translate::wr_translate_logical_size;

        // NOTE: It would be OK to use &RenderApi here, but it's better
        // to make sure that the RenderApi is currently not in use by anything else.

        // NOTE: Creating a new EventsLoop for the new window causes a segfault.
        // Report this to the winit developers.
        // let events_loop = EventsLoop::new();

        let is_transparent_background = background_color.a != 0;

        let mut window = GliumWindowBuilder::new()
            .with_title(options.state.title.clone())
            .with_maximized(options.state.is_maximized)
            .with_decorations(options.state.has_decorations)
            .with_visibility(false)
            .with_transparency(is_transparent_background)
            .with_multitouch();

        // TODO: Update winit to have:
        //      .with_always_on_top(options.state.is_always_on_top)
        //
        // winit 0.13 -> winit 0.15

        // TODO: Add all the extensions for X11 / Mac / Windows,
        // like setting the taskbar icon, setting the titlebar icon, etc.

        // if let Some(icon) = options.window_icon.clone() {
        //     window = window.with_window_icon(Some(icon));
        // }

        // // TODO: Platform-specific options!
        // #[cfg(target_os = "windows")] {
        //     if let Some(icon) = options.taskbar_icon.clone() {
        //         use glium::glutin::os::windows::WindowBuilderExt;
        //         window = window.with_taskbar_icon(Some(icon));
        //     }
        //
        //     // if options.no_redirection_bitmap {
        //     //     use glium::glutin::os::windows::WindowBuilderExt;
        //     //     window = window.with_no_redirection_bitmap(true);
        //     // }
        // }

        if let Some(min_dim) = options.state.size.min_dimensions {
            // TODO: reverse logical size!
            window = window.with_min_dimensions(winit_translate::translate_logical_size(min_dim));
        }

        if let Some(max_dim) = options.state.size.max_dimensions {
            // TODO: reverse logical size!
            window = window.with_max_dimensions(winit_translate::translate_logical_size(max_dim));
        }

        // Only create a context with VSync and SRGB if the context creation works
        let gl_window = create_gl_window(window, &events_loop, Some(shared_context))?;

        // Hide the window until the first draw (prevents flash on startup)
        gl_window.hide();

        let (hidpi_factor, winit_hidpi_factor) = get_hidpi_factor(&gl_window.window(), &events_loop);

        let mut state = options.state.clone();
        state.size.hidpi_factor = hidpi_factor;
        state.size.winit_hidpi_factor = winit_hidpi_factor;

        if options.state.is_fullscreen {
            gl_window.window().set_fullscreen(Some(gl_window.window().get_current_monitor()));
        }

        if let Some(pos) = options.state.position {
            // TODO: reverse logical size!
            gl_window.window().set_position(winit_translate::translate_logical_position(pos));
        }

        if options.state.is_maximized && !options.state.is_fullscreen {
            gl_window.window().set_maximized(true);
        } else if !options.state.is_fullscreen {
            gl_window.window().set_inner_size(winit_translate::translate_logical_size(state.size.get_reverse_logical_size()));
        }

        // #[cfg(debug_assertions)]
        // let display = Display::with_debug(gl_window, DebugCallbackBehavior::DebugMessageOnError)?;
        // #[cfg(not(debug_assertions))]
        let display = Display::with_debug(gl_window, DebugCallbackBehavior::Ignore)?;

        let framebuffer_size = {
            let inner_logical_size = display.gl_window().get_inner_size().unwrap();
            let (width, height): (u32, u32) = inner_logical_size.to_physical(hidpi_factor as f64).into();
            DeviceIntSize::new(width as i32, height as i32)
        };

        let document_id = render_api.add_document(framebuffer_size, 0);
        let epoch = Epoch(0);

        // TODO: The PipelineId is what gets passed to the OutputImageHandler
        // (the code that coordinates displaying the rendered texture).
        //
        // Each window is a "pipeline", i.e a new web page in webrender terms,
        // however, there is only one global renderer, in order to save on memory,
        // The pipeline ID is important, in order to coordinate the rendered textures
        // back to their windows and window positions.
        let pipeline_id = PipelineId::new();

        // let (sender, receiver) = channel();
        // let thread = Builder::new().name(options.title.clone()).spawn(move || Self::handle_event(receiver))?;

        css.sort_by_specificity();

        let display_list_dimensions = wr_translate_logical_size(state.size.dimensions);

        let window = Window {
            id: WindowId::new(),
            create_options: options,
            state,
            display: Rc::new(display),
            css,
            #[cfg(debug_assertions)]
            css_loader: None,
            internal: WindowInternal {
                epoch,
                pipeline_id,
                document_id,
                scrolled_nodes: BTreeMap::new(),
                scroll_states: ScrollStates::new(),
                layout_result: BTreeMap::new(),
                cached_display_list: CachedDisplayList::empty(display_list_dimensions),
            },
            marker: PhantomData,
        };

        Ok(window)
    }

    /// Creates a new window that will automatically load a new style from a given HotReloadHandler.
    /// Only available with debug_assertions enabled.
    #[cfg(debug_assertions)]
    pub(crate) fn new_hot_reload(
        render_api: &mut RenderApi,
        shared_context: &Context,
        events_loop: &EventsLoop,
        options: WindowCreateOptions,
        css_loader: Box<dyn HotReloadHandler>,
        background_color: ColorU,
    ) -> Result<Self, WindowCreateError>  {
        let mut window = Window::new(render_api, shared_context, events_loop, options, Css::default(), background_color)?;
        window.css_loader = Some(css_loader);
        Ok(window)
    }

    /// Returns an iterator over all given monitors
    pub fn get_available_monitors() -> AvailableMonitorsIter {
        EventsLoop::new().get_available_monitors()
    }

    /// Returns what monitor the window is currently residing on (to query monitor size, etc.).
    pub fn get_current_monitor(&self) -> MonitorId {
        self.display.gl_window().window().get_current_monitor()
    }
}

/// Clipboard is an empty class with only static methods,
/// which is why it doesn't have any #[derive] markers.
#[allow(missing_copy_implementations)]
pub struct Clipboard;

impl Clipboard {

    /// Returns the contents of the system clipboard
    pub fn get_clipboard_string() -> Result<String, ClipboardError> {
        let clipboard = SystemClipboard::new()?;
        clipboard.get_string_contents()
    }

    /// Sets the contents of the system clipboard
    pub fn set_clipboard_string(contents: String) -> Result<(), ClipboardError> {
        let clipboard = SystemClipboard::new()?;
        clipboard.set_string_contents(contents)
    }
}

/// Synchronize the FullWindowState with the WindowState,
/// updating the OS-level window to reflect the new state
pub(crate) fn synchronize_window_state_with_os_window(
    old_state: &mut FullWindowState,
    new_state: &WindowState,
    window: &GliumWindow,
) {
    let current_window_state = old_state.clone();

    synchronize_mouse_state(&mut old_state.mouse_state, &new_state.mouse_state, window);

    if old_state.title != new_state.title {
        window.set_title(&new_state.title);
    }

    if old_state.is_maximized != new_state.is_maximized {
        window.set_maximized(new_state.is_maximized);
    }

    if old_state.is_fullscreen != new_state.is_fullscreen {
        if new_state.is_fullscreen {
            window.set_fullscreen(Some(window.get_current_monitor()));
        } else {
            window.set_fullscreen(None);
        }
    }

    if old_state.has_decorations != new_state.has_decorations {
        window.set_decorations(new_state.has_decorations);
    }

    if old_state.is_visible != new_state.is_visible {
        if new_state.is_visible {
            window.show();
        } else {
            window.hide();
        }
    }

    if old_state.size.min_dimensions != new_state.size.min_dimensions {
        window.set_min_dimensions(new_state.size.min_dimensions.map(|min| winit_translate::translate_logical_size(min).into()));
    }

    if old_state.size.max_dimensions != new_state.size.max_dimensions {
        window.set_max_dimensions(new_state.size.max_dimensions.map(|max| winit_translate::translate_logical_size(max).into()));
    }

    // Overwrite all fields of the old state with the new window state
    update_full_window_state(old_state, new_state);
    old_state.previous_window_state = Some(Box::new(current_window_state));
}

/// Reverse function of `full_window_state_to_window_state` - overwrites all
/// fields of the `FullWindowState` with the fields of the `WindowState`
fn update_full_window_state(
    full_window_state: &mut FullWindowState,
    window_state: &WindowState
) {
    full_window_state.title = window_state.title.clone();
    full_window_state.size = window_state.size;
    full_window_state.position = window_state.position;
    full_window_state.is_maximized = window_state.is_maximized;
    full_window_state.is_fullscreen = window_state.is_fullscreen;
    full_window_state.has_decorations = window_state.has_decorations;
    full_window_state.is_visible = window_state.is_visible;
    full_window_state.is_always_on_top = window_state.is_always_on_top;
    full_window_state.is_resizable = window_state.is_resizable;
    full_window_state.debug_state = window_state.debug_state;
    full_window_state.keyboard_state = window_state.keyboard_state.clone();
    full_window_state.mouse_state = window_state.mouse_state;
    full_window_state.ime_position = window_state.ime_position;
    full_window_state.request_user_attention = window_state.request_user_attention;
    full_window_state.wayland_theme = window_state.wayland_theme;
}

fn synchronize_mouse_state(
    old_mouse_state: &mut MouseState,
    new_mouse_state: &MouseState,
    window: &GliumWindow,
) {
    use wr_translate::winit_translate_cursor;
    match (old_mouse_state.mouse_cursor_type, new_mouse_state.mouse_cursor_type) {
        (Some(_old_mouse_cursor), None) => {
            window.hide_cursor(true);
        },
        (None, Some(new_mouse_cursor)) => {
            window.hide_cursor(false);
            window.set_cursor(winit_translate_cursor(new_mouse_cursor));
        },
        (Some(old_mouse_cursor), Some(new_mouse_cursor)) => {
            if old_mouse_cursor != new_mouse_cursor {
                window.set_cursor(winit_translate_cursor(new_mouse_cursor));
            }
        },
        (None, None) => { },
    }

    if old_mouse_state.is_cursor_locked != new_mouse_state.is_cursor_locked {
        window.grab_cursor(new_mouse_state.is_cursor_locked)
        .map_err(|e| { #[cfg(feature = "logging")] { warn!("{}", e); } })
        .unwrap_or(());
    }

    // TODO: Synchronize mouse cursor position!
}

pub(crate) fn full_window_state_to_window_state(full_window_state: &FullWindowState) -> WindowState {
    WindowState {
        title: full_window_state.title.clone(),
        size: full_window_state.size,
        position: full_window_state.position,
        is_maximized: full_window_state.is_maximized,
        is_fullscreen: full_window_state.is_fullscreen,
        has_decorations: full_window_state.has_decorations,
        is_visible: full_window_state.is_visible,
        is_always_on_top: full_window_state.is_always_on_top,
        is_resizable: full_window_state.is_resizable,
        debug_state: full_window_state.debug_state,
        keyboard_state: full_window_state.keyboard_state.clone(),
        mouse_state: full_window_state.mouse_state,
        ime_position: full_window_state.ime_position,
        request_user_attention: full_window_state.request_user_attention,
        wayland_theme: full_window_state.wayland_theme,
    }
}

#[allow(unused_variables)]
pub(crate) fn update_from_external_window_state(
    window_state: &mut FullWindowState,
    frame_event_info: &FrameEventInfo,
    events_loop: &EventsLoop,
    window: &GliumWindow,
) {
    #[cfg(target_os = "linux")] {
        if frame_event_info.new_window_size.is_some() || frame_event_info.new_dpi_factor.is_some() {
            window_state.size.hidpi_factor = linux_get_hidpi_factor(
                &window.get_current_monitor(),
                events_loop
            );
        }
    }

    if let Some(new_size) = frame_event_info.new_window_size {
        window_state.size.dimensions = new_size;
    }

    if let Some(dpi) = frame_event_info.new_dpi_factor {
        window_state.size.winit_hidpi_factor = dpi;
    }
}

/// Resets the mouse states `scroll_x` and `scroll_y` to 0
pub(crate) fn clear_scroll_state(window_state: &mut FullWindowState) {
    window_state.mouse_state.scroll_x = 0.0;
    window_state.mouse_state.scroll_y = 0.0;
}

/// Since the rendering is single-threaded anyways, the renderer is shared across windows.
/// Second, in order to use the font-related functions on the `RenderApi`, we need to
/// store the RenderApi somewhere in the AppResources. However, the `RenderApi` is bound
/// to a window (because OpenGLs function pointer is bound to a window).
///
/// This means that on startup (when calling App::new()), azul creates a fake, hidden display
/// that handles all the rendering, outputs the rendered frames onto a texture, so that the
/// other windows can use said texture. This is also important for animations and multi-window
/// apps later on, but for now the only reason is so that `AppResources::add_font()` has
/// the proper access to the `RenderApi`
pub(crate) struct FakeDisplay {
    /// Main render API that can be used to register and un-register fonts and images
    pub(crate) render_api: RenderApi,
    /// Main renderer, responsible for rendering all windows
    pub(crate) renderer: Option<Renderer>,
    /// Fake / invisible display, only used because OpenGL is tied to a display context
    /// (offscreen rendering is not supported out-of-the-box on many platforms)
    pub(crate) hidden_display: Display,
    /// TODO: Not sure if we even need this, the events loop isn't important
    /// for a window that is never shown
    pub(crate) hidden_events_loop: EventsLoop,
    /// Stores the GL context that is shared across all windows
    pub(crate) gl_context: Rc<Gl>,
}

impl FakeDisplay {

    /// Creates a new render + a new display, given a renderer type (software or hardware)
    pub(crate) fn new(renderer_type: RendererType)
    -> Result<Self, WindowCreateError>
    {
        let events_loop = EventsLoop::new();
        let window = GliumWindowBuilder::new()
            .with_dimensions(winit_translate::translate_logical_size(LogicalSize::new(10.0, 10.0)))
            .with_visibility(false);

        let gl_window = create_gl_window(window, &events_loop, None)?;
        let (dpi_factor, _) = get_hidpi_factor(&gl_window.window(), &events_loop);
        gl_window.hide();

        unsafe { gl_window.make_current().unwrap() };
        let gl = get_gl_context(&gl_window)?;
        let display = Display::with_debug(gl_window, DebugCallbackBehavior::Ignore)?;

        // Note: Notifier is fairly useless, since rendering is completely single-threaded, see comments on RenderNotifier impl
        let notifier = Box::new(Notifier { });
        let (mut renderer, render_api) = create_renderer(gl.clone(), notifier, renderer_type, dpi_factor)?;

        renderer.set_external_image_handler(Box::new(Compositor::default()));

        fn get_gl_context(gl_window: &CombinedContext) -> Result<Rc<dyn Gl>, WindowCreateError> {
            use glium::glutin::Api;
            match gl_window.get_api() {
                Api::OpenGl => Ok(unsafe { gl::GlFns::load_with(|symbol| gl_window.get_proc_address(symbol) as *const _) }),
                Api::OpenGlEs => Ok(unsafe { gl::GlesFns::load_with(|symbol| gl_window.get_proc_address(symbol) as *const _ ) }),
                Api::WebGl => Err(WindowCreateError::WebGlNotSupported),
            }
        }

        Ok(Self {
            render_api,
            renderer: Some(renderer),
            hidden_display: display,
            hidden_events_loop: events_loop,
            gl_context: gl,
        })
    }

    pub fn get_gl_context(&self) -> Rc<Gl> {
        self.gl_context.clone()
    }
}

impl Drop for FakeDisplay {
    fn drop(&mut self) {

        // NOTE: For some reason this is necessary, otherwise the renderer crashes on shutdown
        //
        // TODO: This still crashes on Linux because the makeCurrent call doesn't succeed
        // (likely because the underlying surface has been destroyed). In those cases,
        // we don't de-initialize the rendered (since this is an application shutdown it
        // doesn't matter, the resources are going to get cleaned up by the OS).
        match unsafe { self.hidden_display.gl_window().make_current() } {
            Ok(_) => { },
            Err(e) => {
                error!("Shutdown error: {}", e);
                return;
            },
        }

        self.gl_context.disable(gl::FRAMEBUFFER_SRGB);
        self.gl_context.disable(gl::MULTISAMPLE);
        self.gl_context.disable(gl::POLYGON_SMOOTH);

        if let Some(renderer) = self.renderer.take() {
            renderer.deinit();
        }
    }
}

/// Returns the actual hidpi factor and the winit DPI factor for the current window
#[allow(unused_variables)]
fn get_hidpi_factor(window: &GliumWindow, events_loop: &EventsLoop) -> (f32, f32) {
    let monitor = window.get_current_monitor();
    let winit_hidpi_factor = monitor.get_hidpi_factor();

    #[cfg(target_os = "linux")] {
        (linux_get_hidpi_factor(&monitor, &events_loop), winit_hidpi_factor as f32)
    }
    #[cfg(not(target_os = "linux"))] {
        (winit_hidpi_factor as f32, winit_hidpi_factor as f32)
    }
}


fn create_gl_window(window: GliumWindowBuilder, events_loop: &EventsLoop, shared_context: Option<&Context>)
-> Result<CombinedContext, WindowCreateError>
{
    // The shared_context is reversed: If the shared_context is None, then this window is the root window,
    // so the window should be created with new_shared (so the context can be shared to all other windows).
    //
    // If the shared_context is Some() then the window is not a root window, so it should share the existing
    // context, but not re-share it (so, create it normally via ::new() instead of ::new_shared()).

    CombinedContext::new(window.clone(), create_context_builder(true, true, shared_context),  &events_loop).or_else(|_|
    CombinedContext::new(window.clone(), create_context_builder(true, false, shared_context), &events_loop)).or_else(|_|
    CombinedContext::new(window.clone(), create_context_builder(false, true, shared_context), &events_loop)).or_else(|_|
    CombinedContext::new(window.clone(), create_context_builder(false, false,shared_context), &events_loop))
    .map_err(|e| WindowCreateError::CreateError(e))
}

/// ContextBuilder is sadly not clone-able, which is why it has to be re-created
/// every time you want to create a new context. The goals is to not crash on
/// platforms that don't have VSync or SRGB (which are OpenGL extensions) installed.
///
/// Secondly, in order to support multi-window apps, all windows need to share
/// the same OpenGL context - i.e. `builder.with_shared_lists(some_gl_window.context());`
///
/// `allow_sharing_context` should only be true for the root window - so that
/// we can be sure the shared context can't be re-shared by the created window. Only
/// the root window (via `FakeDisplay`) is allowed to manage the OpenGL context.
fn create_context_builder<'a>(
    vsync: bool,
    srgb: bool,
    shared_context: Option<&'a Context>,
) -> ContextBuilder<'a> {

    // See #33 - specifying a specific OpenGL version
    // makes winit crash on older Intel drivers, which is why we
    // don't specify a specific OpenGL version here
    let mut builder = ContextBuilder::new();

    if let Some(shared_context) = shared_context {
        builder = builder.with_shared_lists(shared_context);
    }

    // #[cfg(debug_assertions)] {
    //     builder = builder.with_gl_debug_flag(true);
    // }

    // #[cfg(not(debug_assertions))] {
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

// This exists because RendererOptions isn't Clone-able
fn get_renderer_opts(native: bool, device_pixel_ratio: f32) -> RendererOptions {

    use webrender::ProgramCache;

    // pre-caching shaders means to compile all shaders on startup
    // this can take significant time and should be only used for testing the shaders
    const PRECACHE_SHADER_FLAGS: ShaderPrecacheFlags = ShaderPrecacheFlags::EMPTY;

    // NOTE: If the clear_color is None, this may lead to "black screens"
    // (because black is the default color) - so instead, white should be the default
    // However, if the clear color is specified, then it's hard creating transparent windows
    // (because of bugs in webrender / handling multi-window background colors).
    // Therefore the background color has to be set before render() is invoked.

    RendererOptions {
        resource_override_path: None,
        precache_flags: PRECACHE_SHADER_FLAGS,
        device_pixel_ratio,
        enable_subpixel_aa: true,
        enable_aa: true,
        cached_programs: Some(ProgramCache::new(None)),
        renderer_kind: if native {
            RendererKind::Native
        } else {
            RendererKind::OSMesa
        },
        .. RendererOptions::default()
    }
}

fn create_renderer(
    gl: Rc<Gl>,
    notifier: Box<Notifier>,
    renderer_type: RendererType,
    device_pixel_ratio: f32,
) -> Result<(Renderer, RenderApi), WindowCreateError> {

    use self::RendererType::*;

    let opts_native = get_renderer_opts(true, device_pixel_ratio);
    let opts_osmesa = get_renderer_opts(false, device_pixel_ratio);

    let (renderer, sender) = match renderer_type {
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
            match Renderer::new(gl.clone(), notifier.clone(), opts_native, WR_SHADER_CACHE) {
                Ok(r) => r,
                Err(_) => Renderer::new(gl, notifier, opts_osmesa, WR_SHADER_CACHE).unwrap()
            }
        }
    };

    let api = sender.create_api();

    Ok((renderer, api))
}

#[cfg(target_os = "linux")]
fn get_xft_dpi() -> Option<f32>{
    // TODO!
    /*
    #include <X11/Xlib.h>
    #include <X11/Xatom.h>
    #include <X11/Xresource.h>

    double _glfwPlatformGetMonitorDPI(_GLFWmonitor* monitor)
    {
        char *resourceString = XResourceManagerString(_glfw.x11.display);
        XrmDatabase db;
        XrmValue value;
        char *type = NULL;
        double dpi = 0.0;

        XrmInitialize(); /* Need to initialize the DB before calling Xrm* functions */

        db = XrmGetStringDatabase(resourceString);

        if (resourceString) {
            printf("Entire DB:\n%s\n", resourceString);
            if (XrmGetResource(db, "Xft.dpi", "String", &type, &value) == True) {
                if (value.addr) {
                    dpi = atof(value.addr);
                }
            }
        }

        printf("DPI: %f\n", dpi);
        return dpi;
    }
    */
    None
}

/// Return the DPI on X11 systems
#[cfg(target_os = "linux")]
fn linux_get_hidpi_factor(monitor: &MonitorId, events_loop: &EventsLoop) -> f32 {

    use std::env;
    use std::process::Command;
    use glium::glutin::os::unix::EventsLoopExt;

    let winit_dpi = monitor.get_hidpi_factor() as f32;
    let winit_hidpi_factor = env::var("WINIT_HIDPI_FACTOR").ok().and_then(|hidpi_factor| hidpi_factor.parse::<f32>().ok());
    let qt_font_dpi = env::var("QT_FONT_DPI").ok().and_then(|font_dpi| font_dpi.parse::<f32>().ok());

    // Execute "gsettings get org.gnome.desktop.interface text-scaling-factor" and parse the output
    let gsettings_dpi_factor =
        Command::new("gsettings")
            .arg("get")
            .arg("org.gnome.desktop.interface")
            .arg("text-scaling-factor")
            .output().ok()
            .map(|output| output.stdout)
            .and_then(|stdout_bytes| String::from_utf8(stdout_bytes).ok())
            .map(|stdout_string| stdout_string.lines().collect::<String>())
            .and_then(|gsettings_output| gsettings_output.parse::<f32>().ok());

    // Wayland: Ignore Xft.dpi
    let xft_dpi = if events_loop.is_x11() { get_xft_dpi() } else { None };

    let options = [winit_hidpi_factor, qt_font_dpi, gsettings_dpi_factor, xft_dpi];
    options.into_iter().filter_map(|x| *x).next().unwrap_or(winit_dpi)
}
