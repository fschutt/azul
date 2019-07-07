use std::{
    rc::Rc,
    marker::PhantomData,
    collections::BTreeMap,
    time::Duration,
};
use webrender::{
    api::{
        Epoch, DocumentId, RenderApi, RenderNotifier, DeviceIntSize,
    },
    Renderer, RendererOptions, RendererKind, ShaderPrecacheFlags, WrShaders,
    // renderer::RendererError; -- not currently public in WebRender
};
use glutin::{
    event_loop::EventLoop,
    window::{Window as GlutinWindow, WindowBuilder as GlutinWindowBuilder},
    CreationError, ContextBuilder, Context, WindowedContext,
    NotCurrent, PossiblyCurrent,
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
    window::{AzulUpdateEvent, WindowId},
};
pub use webrender::api::HitTestItem;
pub use glutin::monitor::MonitorHandle;
pub use azul_core::window::*;
pub use window_state::*;

// TODO: Right now it's not very ergonomic to cache shaders between
// renderers - notify webrender about this.
const WR_SHADER_CACHE: Option<&mut WrShaders> = None;

/// Options on how to initially create the window
pub struct WindowCreateOptions<T> {
    /// State of the window, set the initial title / width / height here.
    pub state: WindowState,
    /// Which monitor should the window be created on?
    pub monitor: WindowMonitorTarget,
    /// Renderer type: Hardware-with-software-fallback, pure software or pure hardware renderer?
    pub renderer_type: RendererType,
    /// Windows only: Sets the 256x256 taskbar icon during startup
    pub taskbar_icon: Option<TaskBarIcon>,
    /// The style of this window
    pub css: Css,
    #[cfg(debug_assertions)]
    #[cfg(not(test))]
    /// An optional style hot-reloader for the current window, only available with debug_assertions
    /// enabled
    pub hot_reload: Option<Box<dyn HotReloadHandler>>,
    // Marker, necessary to create a Window<T> out of the create options
    pub marker: PhantomData<T>,
}

impl<T> Default for WindowCreateOptions<T> {
    fn default() -> Self {
        Self {
            state: WindowState::default(),
            monitor: WindowMonitorTarget::default(),
            renderer_type: RendererType::default(),
            taskbar_icon: None,
            css: Css::default(),
            #[cfg(debug_assertions)]
            #[cfg(not(test))]
            hot_reload: None,
            marker: PhantomData,
        }
    }
}

impl<T> WindowCreateOptions<T> {

    pub(crate) fn get_reload_interval(&self) -> Option<Duration> {
        let hot_reloader = self.hot_reload?;
        Some(hot_reloader.get_reload_interval())
    }

    /// Reloads the CSS (if possible).
    ///
    /// Returns:
    ///
    /// - Ok(true) if the CSS has been successfully reloaded
    /// - Ok(false) if there is no CSS hot-reloader
    /// - Err(why) if the CSS failed to hot-reload.
    pub(crate) fn reload_style(&mut self) -> Result<bool, String> {

        #[cfg(debug_assertions)] {
            let hot_reloader = match self.hot_reload.as_mut() {
                None => return Ok(false),
                Some(s) => s,
            };

            match hot_reloader.reload_style() {
                Ok(mut new_css) => {
                    new_css.sort_by_specificity();
                    self.css = new_css;
                    return Ok(true);
                },
                Err(why) => {
                    return Err(format!("{}", why));
                },
            };
        }

        #[cfg(not(debug_assertions))] {
            return Ok(false);
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

struct Notifier { }

impl RenderNotifier for Notifier {
    fn clone(&self) -> Box<dyn RenderNotifier> {
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
    Custom(MonitorHandle)
}

#[cfg(target_os = "linux")]
type NativeMonitorHandle = u32;
// HMONITOR, (*mut c_void), casted to a usize
#[cfg(target_os = "windows")]
type NativeMonitorHandle = usize;
#[cfg(target_os = "macos")]
type NativeMonitorHandle = u32;

impl WindowMonitorTarget {
    fn get_native_id(&self) -> Option<NativeMonitorHandle> {

        use self::WindowMonitorTarget::*;

        #[cfg(target_os = "linux")]
        use glutin::platform::unix::MonitorHandleExtUnix;
        #[cfg(target_os = "windows")]
        use glutin::platform::windows::MonitorHandleExtWindows;
        #[cfg(target_os = "macos")]
        use glutin::platform::macos::MonitorHandleExtMac;

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
    pub(crate) create_options: WindowCreateOptions<T>,
    /// Current state of the window, stores the keyboard / mouse state,
    /// visibility of the window, etc. of the LAST frame. The user never sets this
    /// field directly, but rather sets the WindowState he wants to have for the NEXT frame,
    /// then azul compares the changes (i.e. if we are currently in fullscreen mode and
    /// the user wants the next screen to be in fullscreen mode, too, simply do nothing), then it
    /// updates this field to reflect the changes.
    ///
    /// This field is initialized from the `WindowCreateOptions`.
    pub(crate) state: WindowState,
    /// Stores things like scroll states, display list + epoch for the window
    pub(crate) internal: WindowInternal,
    /// The display, i.e. the actual window (+ the attached OpenGL context)
    pub(crate) display: ContextState,
}

pub(crate) enum ContextState {
    Current(WindowedContext<PossiblyCurrent>),
    NotCurrent(WindowedContext<NotCurrent>),
}

pub(crate) enum HeadlessContextState {
    Current(Context<PossiblyCurrent>),
    NotCurrent(Context<NotCurrent>),
}

/// Creates a wrapper with `.make_current()` and `.make_not_current()`
/// around `ContextState` and `HeadlessContextState`
macro_rules! impl_context_wrapper {($enum_name:ident) => {
    impl $enum_name {
        pub fn make_current(&mut self) {

            use std::mem;
            use self::$enum_name::*;

            let mut self_mem = unsafe { mem::zeroed::<Self>() };
            mem::swap(&mut self_mem, self); // self -> self_mem

            let mut new_state = match self_mem {
                Current(c) => Current(c),
                NotCurrent(nc) => Current(unsafe { nc.make_current().unwrap() }),
            };

            mem::swap(&mut new_state, self); // self_mem -> self
            mem::forget(new_state); // can't call the destructor on zeroed memory
        }

        pub fn make_not_current(&mut self) {

            use std::mem;
            use self::$enum_name::*;

            let mut self_mem = unsafe { mem::zeroed::<Self>() };
            mem::swap(&mut self_mem, self); // self -> self_mem

            let mut new_state = match self_mem {
                Current(c) => NotCurrent(unsafe { c.make_not_current().unwrap() }),
                NotCurrent(nc) => NotCurrent(nc),
            };

            mem::swap(&mut new_state, self); // self_mem -> self
            mem::forget(new_state); // can't call the destructor on zeroed memory
        }
    }
}}

impl_context_wrapper!(ContextState);
impl_context_wrapper!(HeadlessContextState);

impl ContextState {
    pub fn window(&self) -> &GlutinWindow {
        use self::ContextState::*;
        match &self {
            Current(c) => c.window(),
            NotCurrent(nc) => nc.window(),
        }
    }

    pub fn context(&self) -> Option<&Context<PossiblyCurrent>> {
        use self::ContextState::*;
        match &self {
            Current(c) => Some(c.context()),
            NotCurrent(nc) => None,
        }
    }
}

impl HeadlessContextState {

    pub fn headless_context_not_current(&self) -> Option<&Context<NotCurrent>> {
        use self::HeadlessContextState::*;
        match &self {
            Current(_) => None,
            NotCurrent(nc) => Some(nc),
        }
    }

    pub fn headless_context(&self) -> Option<&Context<PossiblyCurrent>> {
        use self::HeadlessContextState::*;
        match &self {
            Current(c) => Some(c),
            NotCurrent(_) => None,
        }
    }
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
        shared_context: &Context<NotCurrent>,
        events_loop: &EventLoop<AzulUpdateEvent>,
        mut options: WindowCreateOptions<T>,
        background_color: ColorU,
    ) -> Result<Self, CreationError> {

        use wr_translate::translate_logical_size_to_css_layout_size;

        // NOTE: It would be OK to use &RenderApi here, but it's better
        // to make sure that the RenderApi is currently not in use by anything else.

        // NOTE: All windows MUST have a shared EventsLoop, creating a new EventLoop for the
        // new window causes a segfault.

        let is_transparent_background = background_color.a != 0;

        let mut window = create_window_builder(is_transparent_background, &options.state.platform_specific_options);

        // Only create a context with VSync and SRGB if the context creation works
        let gl_window = create_gl_window(window, &events_loop, Some(shared_context))?;

        let (hidpi_factor, winit_hidpi_factor) = get_hidpi_factor(&gl_window.window(), &events_loop);
        options.state.size.hidpi_factor = hidpi_factor;
        options.state.size.winit_hidpi_factor = winit_hidpi_factor;

        // Synchronize the state from the WindowCreateOptions with the window for the first time
        // (set maxmimization, etc.)
        initialize_os_window(&options.state, &gl_window.window());

        // Hide the window until the first draw (prevents flash on startup)
        gl_window.window().set_visible(false);

        let framebuffer_size = {
            let physical_size = options.state.size.dimensions.to_physical(hidpi_factor as f32);
            DeviceIntSize::new(physical_size.width as i32, physical_size.height as i32)
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

        options.css.sort_by_specificity();

        let display_list_dimensions = translate_logical_size_to_css_layout_size(options.state.size.dimensions);

        let window = Window {
            id: WindowId::new(),
            create_options: options,
            state: options.state,
            display: ContextState::NotCurrent(gl_window),
            internal: WindowInternal {
                epoch,
                pipeline_id,
                document_id,
                scrolled_nodes: BTreeMap::new(),
                scroll_states: ScrollStates::new(),
                layout_result: BTreeMap::new(),
                cached_display_list: CachedDisplayList::empty(display_list_dimensions),
            },
        };

        Ok(window)
    }

    /// Returns an iterator over all given monitors
    pub fn get_available_monitors() -> impl Iterator<Item = MonitorHandle> {
        EventLoop::new().available_monitors()
    }

    /// Returns what monitor the window is currently residing on (to query monitor size, etc.).
    pub fn get_current_monitor(&self) -> MonitorHandle {
        self.display.window().current_monitor()
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

/// Create a window builder, depending on the platform options -
/// set all options that *can only be set when the window is created*
#[cfg(target_os = "windows")]
fn create_window_builder(
    has_transparent_background: bool,
    platform_options: &WindowsWindowOptions,
) -> GlutinWindowBuilder {
    use glutin::platform::windows::WindowBuilderExtWindows;

    let mut window_builder = GlutinWindowBuilder::new()
        .with_transparent(has_transparent_background)
        .with_no_redirection_bitmap(platform_options.no_redirection_bitmap);

    window_builder
}

#[cfg(target_os = "linux")]
fn create_window_builder(
    has_transparent_background: bool,
    platform_options: &LinuxWindowOptions,
) -> GlutinWindowBuilder {
    use glutin::platform::windows::WindowBuilderExtUnix;

/*
    fn with_override_redirect(self, override_redirect: bool) -> WindowBuilder
    fn with_x11_window_type(self, x11_window_type: WindowType) -> WindowBuilder
    fn with_gtk_theme_variant(self, variant: String) -> WindowBuilder
    fn with_resize_increments(self, increments: LogicalSize) -> WindowBuilder
    fn with_base_size(self, base_size: LogicalSize) -> WindowBuilder
    fn with_app_id(self, app_id: String) -> WindowBuilder
*/

    let mut window_builder = GlutinWindowBuilder::new()
        .with_transparent(has_transparent_background);
/*
    if let Some(classes) = platform_options.x11_wm_classes {
        for class in classes {
            window_builder = window_builder.with_class(class);
        }
    }

    if let Some(override_redirect) = platform_options.x11_override_redirect {
        window_builder = window_builder.with_override_redirect(override_redirect);
    }

    if let Some(override_redirect) = platform_options.x11_override_redirect {
        window_builder = window_builder.with_override_redirect(override_redirect);
    }

    if let Some(override_redirect) = platform_options.x11_override_redirect {
        window_builder = window_builder.with_override_redirect(override_redirect);
    }

    if let Some(override_redirect) = platform_options.x11_override_redirect {
        window_builder = window_builder.with_override_redirect(override_redirect);
    }
*/
    window_builder
}

#[cfg(target_os = "macos")]
fn create_window_builder(
    has_transparent_background: bool,
    platform_options: &MacWindowOptions,
) -> GlutinWindowBuilder {
    use glutin::platform::windows::WindowBuilderExtMac;

    let mut window_builder = GlutinWindowBuilder::new()
        .with_transparent(has_transparent_background);

    window_builder
}

/// Synchronize the FullWindowState with the WindowState,
/// updating the OS-level window to reflect the new state
pub(crate) fn synchronize_window_state_with_os_window(
    old_state: &mut FullWindowState,
    new_state: &WindowState,
    window: &GlutinWindow,
) {
    use wr_translate::winit_translate::{translate_logical_position, translate_logical_size};

    let current_window_state = old_state.clone();

    if old_state.title != new_state.title {
        window.set_title(&new_state.title);
    }

    if old_state.flags.is_maximized != new_state.flags.is_maximized {
        window.set_maximized(new_state.flags.is_maximized);
    }

    if old_state.flags.is_fullscreen != new_state.flags.is_fullscreen {
        if new_state.flags.is_fullscreen {
            window.set_fullscreen(Some(window.current_monitor()));
        } else {
            window.set_fullscreen(None);
        }
    }

    if old_state.flags.has_decorations != new_state.flags.has_decorations {
        window.set_decorations(new_state.flags.has_decorations);
    }

    if old_state.flags.is_visible != new_state.flags.is_visible {
        window.set_visible(new_state.flags.is_visible);
    }

    if old_state.size.dimensions != new_state.size.dimensions {
        window.set_inner_size(translate_logical_size(new_state.size.dimensions));
    }

    if old_state.size.min_dimensions != new_state.size.min_dimensions {
        window.set_min_inner_size(new_state.size.min_dimensions.map(translate_logical_size));
    }

    if old_state.size.max_dimensions != new_state.size.max_dimensions {
        window.set_max_inner_size(new_state.size.max_dimensions.map(translate_logical_size));
    }

    if old_state.position != new_state.position {
        if let Some(new_position) = new_state.position {
            window.set_outer_position(translate_logical_position(new_position));
        }
    }

    if old_state.ime_position != new_state.ime_position {
        if let Some(new_ime_position) = new_state.ime_position {
            window.set_ime_position(translate_logical_position(new_ime_position));
        }
    }

    if old_state.flags.is_always_on_top != new_state.flags.is_always_on_top {
        window.set_always_on_top(new_state.flags.is_always_on_top);
    }

    if old_state.flags.is_resizable != new_state.flags.is_resizable {
        window.set_resizable(new_state.flags.is_resizable);
    }

    // mouse position, cursor type, etc.
    synchronize_mouse_state(&mut old_state.mouse_state, &new_state.mouse_state, window);

    // platform-specific extensions
    #[cfg(target_os = "windows")] {
        synchronize_os_window_windows_extensions(
            &old_state.platform_specific_options,
            &new_state.platform_specific_options,
            &window
        );
    }
    #[cfg(target_os = "linux")] {
        synchronize_os_window_linux_extensions(
            &old_state.platform_specific_options,
            &new_state.platform_specific_options,
            &window
        );
    }
    #[cfg(target_os = "macos")] {
        synchronize_os_window_mac_extensions(
            &old_state.platform_specific_options,
            &new_state.platform_specific_options,
            &window
        );
    }

    // Overwrite all fields of the old state with the new window state
    update_full_window_state(old_state, new_state);
    old_state.previous_window_state = Some(Box::new(current_window_state));
}

/// Do the inital synchronization of the window with the OS-level window
fn initialize_os_window(
    new_state: &WindowState,
    window: &GlutinWindow,
) {
    use wr_translate::winit_translate::{translate_logical_size, translate_logical_position};

    window.set_title(&new_state.title);
    window.set_maximized(new_state.flags.is_maximized);

    if new_state.flags.is_fullscreen {
        window.set_fullscreen(Some(window.current_monitor()));
    } else {
        window.set_fullscreen(None);
    }

    window.set_decorations(new_state.flags.has_decorations);
    window.set_visible(new_state.flags.is_visible);
    window.set_inner_size(translate_logical_size(new_state.size.dimensions));
    window.set_min_inner_size(new_state.size.min_dimensions.map(translate_logical_size));
    window.set_min_inner_size(new_state.size.max_dimensions.map(translate_logical_size));

    if let Some(new_position) = new_state.position {
        window.set_outer_position(translate_logical_position(new_position));
    }

    if let Some(new_ime_position) = new_state.ime_position {
        window.set_ime_position(translate_logical_position(new_ime_position));
    }

    window.set_always_on_top(new_state.flags.is_always_on_top);
    window.set_resizable(new_state.flags.is_resizable);

    // mouse position, cursor type, etc.
    initialize_mouse_state(&new_state.mouse_state, window);

    // platform-specific extensions
    #[cfg(target_os = "windows")] {
        initialize_os_window_windows_extensions(
            &new_state.platform_specific_options,
            &window
        );
    }
    #[cfg(target_os = "linux")] {
        initialize_os_window_linux_extensions(
            &new_state.platform_specific_options,
            &window
        );
    }
    #[cfg(target_os = "macos")] {
        initialize_os_window_mac_extensions(
            &new_state.platform_specific_options,
            &window
        );
    }
}

fn synchronize_mouse_state(
    old_mouse_state: &mut MouseState,
    new_mouse_state: &MouseState,
    window: &GlutinWindow,
) {
    use wr_translate::winit_translate::{translate_cursor_icon, translate_logical_position};

    match (old_mouse_state.mouse_cursor_type, new_mouse_state.mouse_cursor_type) {
        (Some(_old_mouse_cursor), None) => {
            window.set_cursor_visible(false);
        },
        (None, Some(new_mouse_cursor)) => {
            window.set_cursor_visible(true);
            window.set_cursor_icon(translate_cursor_icon(new_mouse_cursor));
        },
        (Some(old_mouse_cursor), Some(new_mouse_cursor)) => {
            if old_mouse_cursor != new_mouse_cursor {
                window.set_cursor_icon(translate_cursor_icon(new_mouse_cursor));
            }
        },
        (None, None) => { },
    }

    if old_mouse_state.is_cursor_locked != new_mouse_state.is_cursor_locked {
        window.set_cursor_grab(new_mouse_state.is_cursor_locked)
        .map_err(|e| { #[cfg(feature = "logging")] { warn!("{}", e); } })
        .unwrap_or(());
    }

    if old_mouse_state.cursor_position != new_mouse_state.cursor_position {
        if let Some(new_cursor_position) = new_mouse_state.cursor_position.get_position() {
            window.set_cursor_position(translate_logical_position(new_cursor_position))
            .map_err(|e| { #[cfg(feature = "logging")] { warn!("{}", e); } })
            .unwrap_or(());
        }
    }
}

fn initialize_mouse_state(
    new_mouse_state: &MouseState,
    window: &GlutinWindow,
) {
    use wr_translate::winit_translate::{translate_cursor_icon, translate_logical_position};

    match new_mouse_state.mouse_cursor_type {
        None => { window.set_cursor_visible(false); },
        Some(new_mouse_cursor) => {
            window.set_cursor_visible(true);
            window.set_cursor_icon(translate_cursor_icon(new_mouse_cursor));
        },
    }

    window.set_cursor_grab(new_mouse_state.is_cursor_locked)
    .map_err(|e| { #[cfg(feature = "logging")] { warn!("{}", e); } })
    .unwrap_or(());

    if let Some(new_cursor_position) = new_mouse_state.cursor_position.get_position() {
        window.set_cursor_position(translate_logical_position(new_cursor_position))
        .map_err(|e| { #[cfg(feature = "logging")] { warn!("{}", e); } })
        .unwrap_or(());
    }
}

// Windows-specific window options
#[cfg(target_os = "windows")]
fn synchronize_os_window_windows_extensions(
    old_state: &WindowsWindowOptions,
    new_state: &WindowsWindowOptions,
    window: &GlutinWindow,
) {
    use glutin::platform::windows::WindowExtWindows;
    use wr_translate::winit_translate::{translate_window_icon, translate_taskbar_icon};

    if old_state.window_icon != new_state.window_icon {
        window.set_window_icon(new_state.window_icon.and_then(|ic| translate_window_icon(ic).ok()));
    }

    if old_state.taskbar_icon != new_state.taskbar_icon {
        window.set_taskbar_icon(new_state.taskbar_icon.and_then(|ic| translate_taskbar_icon(ic).ok()));
    }
}

// Linux-specific window options
#[cfg(target_os = "linux")]
fn synchronize_os_window_linux_extensions(
    old_state: &LinuxWindowOptions,
    new_state: &LinuxWindowOptions,
    window: &GlutinWindow,
) {
    use glutin::platform::unix::WindowExtUnix;
    use wr_translate::winit_translate::{translate_window_icon, translate_wayland_theme};

    if old_state.request_user_attention != new_state.request_user_attention {
        window.set_urgent(new_state.request_user_attention);
    }

    if old_state.wayland_theme != new_state.wayland_theme {
        if let Some(new_wayland_theme) = new_state.wayland_theme {
            window.set_wayland_theme(translate_wayland_theme(new_wayland_theme));
        }
    }

    if old_state.window_icon != new_state.window_icon {
        window.set_window_icon(new_state.window_icon.and_then(|ic| translate_window_icon(ic)));
    }
}

// Mac-specific window options
#[cfg(target_os = "macos")]
fn synchronize_os_window_mac_extensions(
    old_state: &MacWindowOptions,
    new_state: &MacWindowOptions,
    window: &GlutinWindow,
) {
    use glutin::platform::macos::WindowExtMacOs;

    if old_state.request_user_attention != new_state.request_user_attention {
        window.set_urgent(new_state.request_user_attention);
    }
}

// Windows-specific window options
#[cfg(target_os = "windows")]
fn initialize_os_window_windows_extensions(
    new_state: &WindowsWindowOptions,
    window: &GlutinWindow,
) {
    use glutin::platform::windows::WindowExtWindows;
    use wr_translate::winit_translate::{translate_taskbar_icon, translate_window_icon};

    window.set_window_icon(new_state.window_icon.and_then(|ic| translate_window_icon(ic).ok()));
    window.set_taskbar_icon(new_state.taskbar_icon.and_then(|ic| translate_taskbar_icon(ic).ok()));
}

// Linux-specific window options
#[cfg(target_os = "linux")]
fn initialize_os_window_linux_extensions(
    new_state: &LinuxWindowOptions,
    window: &GlutinWindow,
) {
    use glutin::platform::unix::WindowExtUnix;
    use wr_translate::winit_translate::{translate_window_icon, translate_wayland_theme};

    window.set_urgent(new_state.request_user_attention);

    if let Some(new_wayland_theme) = new_state.wayland_theme {
        window.set_wayland_theme(translate_wayland_theme(new_wayland_theme));
    }

    window.set_window_icon(new_state.window_icon.and_then(|ic| translate_window_icon(ic)));
}

// Mac-specific window options
#[cfg(target_os = "macos")]
fn initialize_os_window_mac_extensions(
    new_state: &MacWindowOptions,
    window: &GlutinWindow,
) {
    use glutin::platform::macos::WindowExtMacOs;

    window.set_urgent(new_state.request_user_attention);
}

/// Overwrites all fields of the `FullWindowState` with the fields of the `WindowState`,
/// but leaves the extra fields such as `.hover_nodes` untouched
fn update_full_window_state(
    full_window_state: &mut FullWindowState,
    window_state: &WindowState
) {
    full_window_state.title = window_state.title.clone();
    full_window_state.size = window_state.size;
    full_window_state.position = window_state.position;
    full_window_state.flags = window_state.flags;
    full_window_state.debug_state = window_state.debug_state;
    full_window_state.keyboard_state = window_state.keyboard_state.clone();
    full_window_state.mouse_state = window_state.mouse_state;
    full_window_state.ime_position = window_state.ime_position;
    full_window_state.platform_specific_options = window_state.platform_specific_options;
}

#[allow(unused_variables)]
pub(crate) fn update_from_external_window_state(
    window_state: &mut FullWindowState,
    frame_event_info: &FrameEventInfo,
    events_loop: &EventLoop<AzulUpdateEvent>,
    window: &GlutinWindow,
) {
    #[cfg(target_os = "linux")] {
        if frame_event_info.new_window_size.is_some() || frame_event_info.new_dpi_factor.is_some() {
            window_state.size.hidpi_factor = linux_get_hidpi_factor(
                &window.current_monitor(),
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
    ///
    /// NOTE: The window and the associated context are split into separate fields.
    pub(crate) hidden_context: HeadlessContextState,
    /// Event loop that all windows share
    pub(crate) hidden_event_loop: EventLoop<AzulUpdateEvent>,
    /// Stores the GL context (function pointers) that are shared across all windows
    pub(crate) gl_context: Rc<Gl>,
}

impl FakeDisplay {

    /// Creates a new render + a new display, given a renderer type (software or hardware)
    pub(crate) fn new(renderer_type: RendererType) -> Result<Self, CreationError> {

        // The events loop is shared across all windows
        let event_loop = EventLoop::new_user_event();
        let gl_context = HeadlessContextState::NotCurrent(create_headless_context(&event_loop)?);

        gl_context.make_current();
        let gl_function_pointers = get_gl_context(gl_context.headless_context().unwrap())?;
        gl_context.make_not_current();

        const DPI_FACTOR: f32 = 1.0;

        // Note: Notifier is fairly useless, since rendering is completely single-threaded, see comments on RenderNotifier impl
        let notifier = Box::new(Notifier { });
        let (mut renderer, render_api) = create_renderer(gl_function_pointers.clone(), notifier, renderer_type, DPI_FACTOR)?;

        renderer.set_external_image_handler(Box::new(Compositor::default()));

        Ok(Self {
            render_api,
            renderer: Some(renderer),
            hidden_context: gl_context,
            hidden_event_loop: event_loop,
            gl_context: gl_function_pointers,
        })
    }

    pub fn get_gl_context(&self) -> Rc<dyn Gl> {
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
        self.hidden_context.make_current();

        self.gl_context.disable(gl::FRAMEBUFFER_SRGB);
        self.gl_context.disable(gl::MULTISAMPLE);
        self.gl_context.disable(gl::POLYGON_SMOOTH);

        if let Some(renderer) = self.renderer.take() {
            renderer.deinit();
        }
    }
}

fn get_gl_context(gl_window: &Context<PossiblyCurrent>) -> Result<Rc<dyn Gl>, CreationError> {
    use glutin::Api;
    match gl_window.get_api() {
        Api::OpenGl => Ok(unsafe { gl::GlFns::load_with(|symbol| gl_window.get_proc_address(symbol) as *const _) }),
        Api::OpenGlEs => Ok(unsafe { gl::GlesFns::load_with(|symbol| gl_window.get_proc_address(symbol) as *const _ ) }),
        Api::WebGl => Err(CreationError::NoBackendAvailable("WebGL".into())),
    }
}

/// Returns the actual hidpi factor and the winit DPI factor for the current window
#[allow(unused_variables)]
fn get_hidpi_factor(window: &GlutinWindow, events_loop: &EventLoop<AzulUpdateEvent>) -> (f32, f32) {
    let monitor = window.current_monitor();
    let winit_hidpi_factor = monitor.hidpi_factor();

    #[cfg(target_os = "linux")] {
        (linux_get_hidpi_factor(&monitor, &events_loop), winit_hidpi_factor as f32)
    }
    #[cfg(not(target_os = "linux"))] {
        (winit_hidpi_factor as f32, winit_hidpi_factor as f32)
    }
}

fn create_gl_window<'a>(
    window_builder: GlutinWindowBuilder,
    event_loop: &EventLoop<AzulUpdateEvent>,
    shared_context: Option<&'a Context<NotCurrent>>,
) -> Result<WindowedContext<NotCurrent>, CreationError> {
    create_window_context_builder(true, true, shared_context).build_windowed(window_builder.clone(), event_loop)
        .or_else(|_| create_window_context_builder(true, false, shared_context).build_windowed(window_builder.clone(), event_loop))
        .or_else(|_| create_window_context_builder(false, true, shared_context).build_windowed(window_builder.clone(), event_loop))
        .or_else(|_| create_window_context_builder(false, false,shared_context).build_windowed(window_builder, event_loop))
}

fn create_headless_context(
    event_loop: &EventLoop<AzulUpdateEvent>,
) -> Result<Context<NotCurrent>, CreationError> {
    use glutin::dpi::PhysicalSize as GlutinPhysicalSize;
    create_window_context_builder(true, true, None).build_headless(event_loop, GlutinPhysicalSize::new(1.0, 1.0))
        .or_else(|_| create_window_context_builder(true, false, None).build_headless(event_loop, GlutinPhysicalSize::new(1.0, 1.0)))
        .or_else(|_| create_window_context_builder(false, true, None).build_headless(event_loop, GlutinPhysicalSize::new(1.0, 1.0)))
        .or_else(|_| create_window_context_builder(false, false,None).build_headless(event_loop, GlutinPhysicalSize::new(1.0, 1.0)))
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
fn create_window_context_builder<'a>(
    vsync: bool,
    srgb: bool,
    shared_context: Option<&'a Context<NotCurrent>>
) -> ContextBuilder<'a, NotCurrent> {

    // See #33 - specifying a specific OpenGL version
    // makes winit crash on older Intel drivers, which is why we
    // don't specify a specific OpenGL version here
    //
    // TODO: The comment above might be old, see if it still happens and / or fallback to CPU

    let context_builder = match shared_context {
        Some(s) => ContextBuilder::new().with_shared_lists(s),
        None => ContextBuilder::new(),
    };

    #[cfg(debug_assertions)]
    let gl_debug_enabled = true;
    #[cfg(not(debug_assertions))]
    let gl_debug_enabled = false;
    context_builder
        .with_gl_debug_flag(gl_debug_enabled)
        .with_vsync(vsync)
        .with_srgb(srgb)
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
    gl: Rc<dyn Gl>,
    notifier: Box<Notifier>,
    renderer_type: RendererType,
    device_pixel_ratio: f32,
) -> Result<(Renderer, RenderApi), CreationError> {

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
fn linux_get_hidpi_factor(monitor: &MonitorHandle, events_loop: &EventLoop<AzulUpdateEvent>) -> f32 {

    use std::env;
    use std::process::Command;
    use glutin::platform::unix::EventLoopExtUnix;

    let winit_dpi = monitor.hidpi_factor() as f32;
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
