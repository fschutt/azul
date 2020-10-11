use std::{
    rc::Rc,
    collections::BTreeMap,
    marker::PhantomData,
};
use webrender::{
    api::{
        DocumentId as WrDocumentId,
        RenderApi as WrRenderApi,
        RenderNotifier as WrRenderNotifier,
        units::DeviceIntSize as WrDeviceIntSize,
    },
    Renderer as WrRenderer,
    RendererOptions as WrRendererOptions,
    RendererKind as WrRendererKind,
    ShaderPrecacheFlags as WrShaderPrecacheFlags,
    WrShaders as WrShaders,
    // renderer::RendererError; -- not currently public in WebRender
};
use glutin::{
    event_loop::{EventLoopWindowTarget, EventLoop},
    window::{Window as GlutinWindow, WindowBuilder as GlutinWindowBuilder},
    CreationError as GlutinCreationError, ContextBuilder, Context, WindowedContext,
    NotCurrent, PossiblyCurrent,
};
use gleam::gl::{self, Gl};
use clipboard2::{Clipboard as _, ClipboardError, SystemClipboard};
use azul_css::{ColorU, LayoutPoint, LayoutRect};
use crate::{
    resources::WrApi,
    compositor::Compositor
};
use azul_core::{
    FastHashMap,
    ui_state::UiState,
    callbacks::{PipelineId, ScrollPosition},
    dom::{NodeId, DomId},
    display_list::{CachedDisplayList, SolvedLayoutCache, GlTextureCache},
    ui_solver::{ScrolledNodes, ExternalScrollId, OverflowingScrollNode},
    window::WindowId,
    app_resources::{Epoch, AppResources},
};
pub use glutin::monitor::MonitorHandle;
pub use azul_core::window::*;

// TODO: Right now it's not very ergonomic to cache shaders between
// renderers - notify webrender about this.
const WR_SHADER_CACHE: Option<&mut WrShaders> = None;

struct Notifier { }

impl WrRenderNotifier for Notifier {
    fn clone(&self) -> Box<dyn WrRenderNotifier> {
        Box::new(Notifier { })
    }

    // NOTE: Rendering is single threaded (because that's the nature of OpenGL),
    // so when the Renderer::render() function is finished, then the rendering
    // is finished and done, the rendering is currently blocking (but only takes about 0..
    // There is no point in implementing RenderNotifier, it only leads to
    // synchronization problems (when handling Event::Awakened).

    fn wake_up(&self) { }
    fn new_frame_ready(&self, _id: WrDocumentId, _scrolled: bool, _composite_needed: bool, _render_time: Option<u64>) { }
}

/// Select on which monitor the window should pop up.
#[derive(Debug, Clone)]
pub enum Monitor {
    /// Window should appear on the primary monitor
    Primary,
    /// Use `Window::get_available_monitors()` to select the correct monitor
    Custom(MonitorHandle)
}

#[cfg(target_os = "linux")]
pub type NativeMonitorHandle = u32;
// HMONITOR, (*mut c_void), casted to a usize
#[cfg(target_os = "windows")]
pub type NativeMonitorHandle = usize;
#[cfg(target_os = "macos")]
pub type NativeMonitorHandle = u32;

impl Monitor {

    /// Returns an iterator over all given monitors
    pub fn get_available_monitors() -> impl Iterator<Item = MonitorHandle> {
        EventLoop::new().available_monitors()
    }

    pub fn get_native_id(&self) -> Option<NativeMonitorHandle> {

        use self::Monitor::*;

        #[cfg(target_os = "linux")]
        use glutin::platform::unix::MonitorHandleExtUnix;
        #[cfg(target_os = "windows")]
        use glutin::platform::windows::MonitorHandleExtWindows;
        #[cfg(target_os = "macos")]
        use glutin::platform::macos::MonitorHandleExtMacOS;

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

impl ::std::hash::Hash for Monitor {
    fn hash<H>(&self, state: &mut H) where H: ::std::hash::Hasher {
        use self::Monitor::*;
        state.write_usize(match self { Primary => 0, Custom(_) => 1, });
        state.write_usize(self.get_native_id().unwrap_or(0) as usize);
    }
}

impl PartialEq for Monitor {
    fn eq(&self, rhs: &Self) -> bool {
        self.get_native_id() == rhs.get_native_id()
    }
}

impl PartialOrd for Monitor {
    fn partial_cmp(&self, other: &Self) -> Option<::std::cmp::Ordering> {
        Some((self.get_native_id()).cmp(&(other.get_native_id())))
    }
}

impl Ord for Monitor {
    fn cmp(&self, other: &Self) -> ::std::cmp::Ordering {
        (self.get_native_id()).cmp(&(other.get_native_id()))
    }
}

impl Eq for Monitor { }

impl Default for Monitor {
    fn default() -> Self {
        Monitor::Primary
    }
}

/// Represents one graphical window to be rendered
pub struct Window<T> {
    /// System that can identify this window
    pub(crate) id: WindowId,
    /// Renderer type: Hardware-with-software-fallback, pure software or pure hardware renderer?
    pub(crate) renderer_type: RendererType,
    #[cfg(debug_assertions)]
    #[cfg(not(test))]
    /// An optional style hot-reloader for the current window, only
    /// available with debug_assertions enabled
    pub(crate) hot_reload_handler: Option<HotReloader>,
    /// Stores things like scroll states, display list + epoch for the window
    pub(crate) internal: WindowInternal,
    /// The display, i.e. the actual window (+ the attached OpenGL context)
    pub(crate) display: ContextState,
    // Marker, necessary to create a Window<T> out of the `WindowCreateOptions<T>`
    marker: PhantomData<T>,
}

pub(crate) enum ContextState {
    MakeCurrentInProgress,
    Current(WindowedContext<PossiblyCurrent>),
    NotCurrent(WindowedContext<NotCurrent>),
}

pub(crate) enum HeadlessContextState {
    MakeCurrentInProgress,
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

            let mut new_state = match mem::replace(self, $enum_name::MakeCurrentInProgress) {
                Current(c) => Current(c),
                NotCurrent(nc) => Current(unsafe { nc.make_current().unwrap() }),
                MakeCurrentInProgress => MakeCurrentInProgress,
            };

            mem::swap(self, &mut new_state);
        }

        pub fn make_not_current(&mut self) {

            use std::mem;
            use self::$enum_name::*;

            let mut new_state = match mem::replace(self, $enum_name::MakeCurrentInProgress) {
                Current(c) => NotCurrent(unsafe { c.make_not_current().unwrap() }),
                NotCurrent(nc) => NotCurrent(nc),
                MakeCurrentInProgress => MakeCurrentInProgress,
            };

            mem::swap(self, &mut new_state);
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
            MakeCurrentInProgress => {
                #[cfg(debug_assertions)] { unreachable!() }
                #[cfg(not(debug_assertions))] { use std::hint; unsafe{ hint::unreachable_unchecked() } }
            }
        }
    }

    pub fn context(&self) -> Option<&Context<PossiblyCurrent>> {
        use self::ContextState::*;
        match &self {
            Current(c) => Some(c.context()),
            NotCurrent(_) | MakeCurrentInProgress => None,
        }
    }

    pub fn windowed_context(&self) -> Option<&WindowedContext<PossiblyCurrent>> {
        use self::ContextState::*;
        match &self {
            Current(c) => Some(c),
            NotCurrent(_) | MakeCurrentInProgress => None,
        }
    }
}

impl HeadlessContextState {

    pub fn headless_context_not_current(&self) -> Option<&Context<NotCurrent>> {
        use self::HeadlessContextState::*;
        match &self {
            Current(_) | MakeCurrentInProgress => None,
            NotCurrent(nc) => Some(nc),
        }
    }

    pub fn headless_context(&self) -> Option<&Context<PossiblyCurrent>> {
        use self::HeadlessContextState::*;
        match &self {
            Current(c) => Some(c),
            NotCurrent(_) | MakeCurrentInProgress => None,
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
    /// A "document" in WebRender usually corresponds to one tab (i.e. in Azuls case, the whole window).
    pub(crate) document_id: WrDocumentId,
    /// One "document" (tab) can have multiple "pipelines" (important for hit-testing).
    ///
    /// A document can have multiple pipelines, for example in Firefox the tab / navigation bar,
    /// the actual browser window and the inspector are seperate pipelines, but contained in one document.
    /// In Azul, one pipeline = one document (this could be improved later on).
    pub(crate) pipeline_id: PipelineId,
    /// The "epoch" is a frame counter, to remove outdated images, fonts and OpenGL textures
    /// when they're not in use anymore.
    pub(crate) epoch: Epoch,
    /// Current display list active in this window (useful for debugging)
    pub(crate) cached_display_list: CachedDisplayList,
    /// Currently active, layouted rectangles
    pub(crate) layout_result: SolvedLayoutCache,
    /// Currently GL textures inside the active CachedDisplayList
    pub(crate) gl_texture_cache: GlTextureCache,
    /// Current scroll states of nodes (x and y position of where they are scrolled)
    pub(crate) scrolled_nodes: BTreeMap<DomId, ScrolledNodes>,
    /// States of scrolling animations, updated every frame
    pub(crate) scroll_states: ScrollStates,
}

impl WindowInternal {

    /// Returns a copy of the current scroll states + scroll positions
    pub(crate) fn get_current_scroll_states<T>(&self, ui_states: &BTreeMap<DomId, UiState<T>>)
    -> BTreeMap<DomId, BTreeMap<NodeId, ScrollPosition>>
    {
        self.scrolled_nodes.iter().filter_map(|(dom_id, scrolled_nodes)| {

            let layout_result = self.layout_result.solved_layouts.get(dom_id)?;
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
        render_api: &mut WrApi,
        shared_context: &Context<NotCurrent>,
        events_loop: &EventLoopWindowTarget<()>,
        mut options: WindowCreateOptions<T>,
        background_color: ColorU,
        app_resources: &mut AppResources,
    ) -> Result<Self, GlutinCreationError> {

        use crate::wr_translate::translate_logical_size_to_css_layout_size;

        // NOTE: It would be OK to use &RenderApi here, but it's better
        // to make sure that the RenderApi is currently not in use by anything else.

        // NOTE: All windows MUST have a shared EventsLoop, creating a new EventLoop for the
        // new window causes a segfault.

        let is_transparent_background = background_color.a != 0;

        let window_builder = create_window_builder(is_transparent_background, &options.state.platform_specific_options);

        // Only create a context with VSync and SRGB if the context creation works
        let gl_window = create_gl_window(window_builder, &events_loop, Some(shared_context))?;

        let (hidpi_factor, winit_hidpi_factor) = get_hidpi_factor(&gl_window.window(), &events_loop);
        options.state.size.hidpi_factor = hidpi_factor;
        options.state.size.winit_hidpi_factor = winit_hidpi_factor;

        // Synchronize the state from the WindowCreateOptions with the window for the first time
        // (set maxmimization, etc.)
        initialize_os_window(&options.state, &gl_window.window());

        // Hide the window until the first draw (prevents flash on startup)
        // gl_window.window().set_visible(false);

        let framebuffer_size = {
            let physical_size = options.state.size.dimensions.to_physical(hidpi_factor as f32);
            WrDeviceIntSize::new(physical_size.width as i32, physical_size.height as i32)
        };

        let document_id = render_api.api.add_document(framebuffer_size, 0);

        // TODO: The PipelineId is what gets passed to the OutputImageHandler
        // (the code that coordinates displaying the rendered texture).
        //
        // Each window is a "pipeline", i.e a new web page in webrender terms,
        // however, there is only one global renderer, in order to save on memory,
        // The pipeline ID is important, in order to coordinate the rendered textures
        // back to their windows and window positions.
        let pipeline_id = PipelineId::new();

        app_resources.add_pipeline(pipeline_id);

        options.state.css.sort_by_specificity();

        let display_list_dimensions = translate_logical_size_to_css_layout_size(options.state.size.dimensions);

        let window = Window {
            id: WindowId::new(),
            renderer_type: options.renderer_type,
            #[cfg(not(test))]
            #[cfg(debug_assertions)]
            hot_reload_handler: options.hot_reload_handler.map(|w| HotReloader::new(w)),
            display: ContextState::NotCurrent(gl_window),
            internal: WindowInternal {
                epoch: Epoch(0),
                pipeline_id,
                document_id,
                scrolled_nodes: BTreeMap::new(),
                scroll_states: ScrollStates::new(),
                layout_result: SolvedLayoutCache::default(),
                gl_texture_cache: GlTextureCache::default(),
                cached_display_list: CachedDisplayList::empty(display_list_dimensions),
            },
            marker: options.marker,
        };

        Ok(window)
    }

    /// Returns what monitor the window is currently residing on (to query monitor size, etc.).
    /// crashes and burns when there is no monitor available
    pub fn get_current_monitor(&self) -> MonitorHandle {

        self.display.window().current_monitor().unwrap()
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
    use crate::wr_translate::winit_translate::translate_taskbar_icon;

    let mut window_builder = GlutinWindowBuilder::new()
        .with_transparent(has_transparent_background)
        .with_no_redirection_bitmap(platform_options.no_redirection_bitmap)
        .with_taskbar_icon(platform_options.taskbar_icon.clone().and_then(|ic| translate_taskbar_icon(ic).ok()));

    if let Some(parent_window) = platform_options.parent_window {
        window_builder = window_builder.with_parent_window(parent_window as *mut _);
    }

    window_builder
}

#[cfg(target_os = "linux")]
fn create_window_builder(
    has_transparent_background: bool,
    platform_options: &LinuxWindowOptions,
) -> GlutinWindowBuilder {

    use glutin::platform::unix::WindowBuilderExtUnix;
    use crate::wr_translate::winit_translate::{translate_x_window_type, translate_logical_size};

    let mut window_builder = GlutinWindowBuilder::new()
        .with_transparent(has_transparent_background)
        .with_override_redirect(platform_options.x11_override_redirect);

    if let Some(classes) = platform_options.x11_wm_classes.clone() {
        for (k, v) in classes {
            window_builder = window_builder.with_class(k, v);
        }
    }

    if let Some(window_type) = platform_options.x11_window_type {
        window_builder = window_builder.with_x11_window_type(translate_x_window_type(window_type));
    }

    if let Some(theme_variant) = platform_options.x11_gtk_theme_variant.clone() {
        window_builder = window_builder.with_gtk_theme_variant(theme_variant);
    }

    if let Some(resize_increments) = platform_options.x11_resize_increments {
        window_builder = window_builder.with_resize_increments(translate_logical_size(resize_increments));
    }

    if let Some(base_size) = platform_options.x11_base_size {
        window_builder = window_builder.with_base_size(translate_logical_size(base_size));
    }

    if let Some(app_id) = platform_options.wayland_app_id.clone() {
        window_builder = window_builder.with_app_id(app_id);
    }

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
    use crate::wr_translate::winit_translate::{translate_logical_position, translate_logical_size};
    use glutin::window::Fullscreen;

    let current_window_state = old_state.clone();

    if old_state.title != new_state.title {
        window.set_title(&new_state.title);
    }

    if old_state.flags.is_maximized != new_state.flags.is_maximized {
        window.set_maximized(new_state.flags.is_maximized);
    }

    if old_state.flags.is_fullscreen != new_state.flags.is_fullscreen {
        if new_state.flags.is_fullscreen {
            // TODO: implement exclusive fullscreen!
            window.set_fullscreen(Some(Fullscreen::Borderless(window.current_monitor())));
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
    use crate::wr_translate::winit_translate::{translate_logical_size, translate_logical_position};
    use glutin::window::Fullscreen;

    window.set_title(&new_state.title);
    window.set_maximized(new_state.flags.is_maximized);

    if new_state.flags.is_fullscreen {
        window.set_fullscreen(Some(Fullscreen::Borderless(window.current_monitor())));
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
    use crate::wr_translate::winit_translate::{translate_cursor_icon, translate_logical_position};

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
    use crate::wr_translate::winit_translate::{translate_cursor_icon, translate_logical_position};

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
    use crate::wr_translate::winit_translate::{translate_window_icon, translate_taskbar_icon};

    if old_state.window_icon != new_state.window_icon {
        window.set_window_icon(new_state.window_icon.clone().and_then(|ic| translate_window_icon(ic).ok()));
    }

    if old_state.taskbar_icon != new_state.taskbar_icon {
        window.set_taskbar_icon(new_state.taskbar_icon.clone().and_then(|ic| translate_taskbar_icon(ic).ok()));
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
    use crate::wr_translate::winit_translate::{translate_window_icon, translate_wayland_theme};

    if old_state.request_user_attention != new_state.request_user_attention {
        window.set_urgent(new_state.request_user_attention);
    }

    if old_state.wayland_theme != new_state.wayland_theme {
        if let Some(new_wayland_theme) = new_state.wayland_theme {
            window.set_wayland_theme(translate_wayland_theme(new_wayland_theme));
        }
    }

    if old_state.window_icon != new_state.window_icon {
        window.set_window_icon(new_state.window_icon.clone().and_then(|ic| translate_window_icon(ic).ok()));
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
    use crate::wr_translate::winit_translate::{translate_taskbar_icon, translate_window_icon};

    window.set_window_icon(new_state.window_icon.clone().and_then(|ic| translate_window_icon(ic).ok()));
    window.set_taskbar_icon(new_state.taskbar_icon.clone().and_then(|ic| translate_taskbar_icon(ic).ok()));
}

// Linux-specific window options
#[cfg(target_os = "linux")]
fn initialize_os_window_linux_extensions(
    new_state: &LinuxWindowOptions,
    window: &GlutinWindow,
) {
    use glutin::platform::unix::WindowExtUnix;
    use crate::wr_translate::winit_translate::{translate_window_icon, translate_wayland_theme};

    window.set_urgent(new_state.request_user_attention);

    if let Some(new_wayland_theme) = new_state.wayland_theme {
        window.set_wayland_theme(translate_wayland_theme(new_wayland_theme));
    }

    window.set_window_icon(new_state.window_icon.clone().and_then(|ic| translate_window_icon(ic).ok()));
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
    full_window_state.platform_specific_options = window_state.platform_specific_options.clone();
}

/// Resets the mouse states `scroll_x` and `scroll_y` to 0
pub(crate) fn clear_scroll_state(window_state: &mut FullWindowState) {
    window_state.mouse_state.scroll_x = None;
    window_state.mouse_state.scroll_y = None;
}

/// Since the rendering is single-th9readed anyways, the renderer is shared across windows.
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
    pub(crate) render_api: WrApi,
    /// Main renderer, responsible for rendering all windows
    pub(crate) renderer: Option<WrRenderer>,
    /// Fake / invisible display, only used because OpenGL is tied to a display context
    /// (offscreen rendering is not supported out-of-the-box on many platforms)
    ///
    /// NOTE: The window and the associated context are split into separate fields.
    pub(crate) hidden_context: HeadlessContextState,
    /// Event loop that all windows share
    pub(crate) hidden_event_loop: EventLoop<()>,
    /// Stores the GL context (function pointers) that are shared across all windows
    pub(crate) gl_context: Rc<dyn Gl>,
}

impl FakeDisplay {

    /// Creates a new render + a new display, given a renderer type (software or hardware)
    pub(crate) fn new(renderer_type: RendererType) -> Result<Self, GlutinCreationError> {

        const DPI_FACTOR: f32 = 1.0;

        // The events loop is shared across all windows
        let event_loop = EventLoop::new();
        let mut gl_context = HeadlessContextState::NotCurrent(create_headless_context(&event_loop)?);

        gl_context.make_current();
        let gl_function_pointers = get_gl_context(gl_context.headless_context().unwrap())?;

        // Note: Notifier is fairly useless, since rendering is completely single-threaded, see comments on RenderNotifier impl
        let notifier = Box::new(Notifier { });
        let (mut renderer, render_api) = create_renderer(gl_function_pointers.clone(), notifier, renderer_type, DPI_FACTOR)?;

        renderer.set_external_image_handler(Box::new(Compositor::default()));

        gl_context.make_not_current();

        Ok(Self {
            render_api: WrApi { api: render_api },
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

fn get_gl_context(gl_window: &Context<PossiblyCurrent>) -> Result<Rc<dyn Gl>, GlutinCreationError> {
    use glutin::Api;
    match gl_window.get_api() {
        Api::OpenGl => Ok(unsafe { gl::GlFns::load_with(|symbol| gl_window.get_proc_address(symbol) as *const _) }),
        Api::OpenGlEs => Ok(unsafe { gl::GlesFns::load_with(|symbol| gl_window.get_proc_address(symbol) as *const _ ) }),
        Api::WebGl => Err(GlutinCreationError::NoBackendAvailable("WebGL".into())),
    }
}

/// Returns the actual hidpi factor and the winit DPI factor for the current window
#[allow(unused_variables)]
fn get_hidpi_factor(window: &GlutinWindow, event_loop: &EventLoopWindowTarget<()>) -> (f32, f32) {

    
    let winit_hidpi_factor = window.scale_factor() as f32;
    
    #[cfg(target_os = "linux")] {
        use crate::glutin::platform::unix::EventLoopWindowTargetExtUnix;
        let is_x11 = event_loop.is_x11();
        (linux_get_hidpi_factor(is_x11).unwrap_or(winit_hidpi_factor), winit_hidpi_factor)
    }

    #[cfg(not(target_os = "linux"))] {
        (winit_hidpi_factor, winit_hidpi_factor)
    }
}

fn create_gl_window<'a>(
    window_builder: GlutinWindowBuilder,
    event_loop: &EventLoopWindowTarget<()>,
    shared_context: Option<&'a Context<NotCurrent>>,
) -> Result<WindowedContext<NotCurrent>, GlutinCreationError> {
    create_window_context_builder(true, true, shared_context).build_windowed(window_builder.clone(), event_loop)
        .or_else(|_| create_window_context_builder(true, false, shared_context).build_windowed(window_builder.clone(), event_loop))
        .or_else(|_| create_window_context_builder(false, true, shared_context).build_windowed(window_builder.clone(), event_loop))
        .or_else(|_| create_window_context_builder(false, false,shared_context).build_windowed(window_builder, event_loop))
}

fn create_headless_context(
    event_loop: &EventLoop<()>,
) -> Result<Context<NotCurrent>, GlutinCreationError> {
    use glutin::dpi::PhysicalSize as GlutinPhysicalSize;
    create_window_context_builder(true, true, None).build_headless(event_loop, GlutinPhysicalSize::new(1, 1))
        .or_else(|_| create_window_context_builder(true, false, None).build_headless(event_loop, GlutinPhysicalSize::new(1, 1)))
        .or_else(|_| create_window_context_builder(false, true, None).build_headless(event_loop, GlutinPhysicalSize::new(1, 1)))
        .or_else(|_| create_window_context_builder(false, false,None).build_headless(event_loop, GlutinPhysicalSize::new(1, 1)))
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
fn get_renderer_opts(native: bool, device_pixel_ratio: f32) -> WrRendererOptions {

    use webrender::ProgramCache as WrProgramCache;

    // pre-caching shaders means to compile all shaders on startup
    // this can take significant time and should be only used for testing the shaders
    const PRECACHE_SHADER_FLAGS: WrShaderPrecacheFlags = WrShaderPrecacheFlags::EMPTY;

    // NOTE: If the clear_color is None, this may lead to "black screens"
    // (because black is the default color) - so instead, white should be the default
    // However, if the clear color is specified, then it's hard creating transparent windows
    // (because of bugs in webrender / handling multi-window background colors).
    // Therefore the background color has to be set before render() is invoked.

    WrRendererOptions {
        resource_override_path: None,
        precache_flags: PRECACHE_SHADER_FLAGS,
        device_pixel_ratio,
        enable_subpixel_aa: true,
        enable_aa: true,
        cached_programs: Some(WrProgramCache::new(None)),
        renderer_kind: if native {
            WrRendererKind::Native
        } else {
            WrRendererKind::OSMesa
        },
        .. WrRendererOptions::default()
    }
}

fn create_renderer(
    gl: Rc<dyn Gl>,
    notifier: Box<Notifier>,
    renderer_type: RendererType,
    device_pixel_ratio: f32,
) -> Result<(WrRenderer, WrRenderApi), GlutinCreationError> {

    use self::RendererType::*;

    let opts_native = get_renderer_opts(true, device_pixel_ratio);
    let opts_osmesa = get_renderer_opts(false, device_pixel_ratio);

    let init_size = webrender::api::units::DeviceIntSize::new(0,0);

    let (renderer, sender) = match renderer_type {
        Hardware => {
            // force hardware renderer
            
            WrRenderer::new(gl, notifier, opts_native, WR_SHADER_CACHE, init_size).unwrap()
        },
        Software => {
            // force software renderer
            WrRenderer::new(gl, notifier, opts_osmesa, WR_SHADER_CACHE, init_size).unwrap()
        },
        Default => {
            // try hardware first, fall back to software
            match WrRenderer::new(gl.clone(), notifier.clone(), opts_native, WR_SHADER_CACHE, init_size) {
                Ok(r) => r,
                Err(_) => {
                    WrRenderer::new(gl, notifier, opts_osmesa, WR_SHADER_CACHE, init_size).unwrap()
                }
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
fn linux_get_hidpi_factor(is_x11: bool) -> Option<f32> {

    use std::env;
    use std::process::Command;

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
    let xft_dpi = if is_x11 { get_xft_dpi() } else { None };

    let options = [winit_hidpi_factor, qt_font_dpi, gsettings_dpi_factor, xft_dpi];
    options.into_iter().filter_map(|x| *x).next()
}
