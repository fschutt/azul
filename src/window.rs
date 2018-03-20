use webrender::api::*;
use webrender::{Renderer, RendererOptions};
use glium::{IncompatibleOpenGl, Display};
use glium::debug::DebugCallbackBehavior;
use glium::glutin::{self, EventsLoop, AvailableMonitorsIter, GlProfile, GlContext, GlWindow, CreationError,
                          MonitorId, EventsLoopProxy, ContextError, ContextBuilder, WindowBuilder};
use gleam::gl;
use glium::backend::glutin::DisplayCreationError;
use euclid::TypedScale;
use cassowary::{Variable, Solver};
use cassowary::strength::*;

use display_list::SolvedLayout;
use traits::LayoutScreen;
use css::Css;
use cache::{EditVariableCache, DomTreeCache};
use id_tree::NodeId;

use std::time::Duration;
use std::fmt;

const DEFAULT_TITLE: &str = "Azul App";
const DEFAULT_WIDTH: u32 = 800;
const DEFAULT_HEIGHT: u32 = 600;

/// azul-internal ID for a window
#[derive(Debug, Copy, Clone, PartialOrd, Ord, PartialEq, Eq)]
pub struct WindowId {
    pub id: usize,
}

impl WindowId {
    pub fn new(id: usize) -> Self { Self { id: id } }
}

/// Options on how to initially create the window
#[derive(Debug, Clone)]
pub struct WindowCreateOptions {
    /// Title of the window
    pub title: String,
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
    /// How should the window be decorated?
    pub decorations: WindowDecorations,
    /// Size and position of the window
    pub size: WindowPlacement,
    /// What type of window (full screen, popup, normal)
    pub class: WindowClass,
}

impl Default for WindowCreateOptions {
    fn default() -> Self {
        Self {
            title: self::DEFAULT_TITLE.into(),
            background: ColorF::new(1.0, 1.0, 1.0, 1.0),
            clear_stencil: None,
            clear_depth: None,
            update_mode: UpdateMode::default(),
            monitor: WindowMonitorTarget::default(),
            mouse_mode: MouseMode::default(),
            update_behaviour: UpdateBehaviour::default(),
            decorations: WindowDecorations::default(),
            size: WindowPlacement::default(),
            class: WindowClass::default(),
        }
    }
}

/// How should the window be decorated
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum WindowDecorations {
    /// Regular window decorations
    Normal,
    /// Maximize button disabled
    MaximizeDisabled,
    /// Minimize button disabled
    MinimizeDisabled,
    /// Both maximize and minimize button disabled
    MaximizeMinimizeDisabled,
    /// No decorations (borderless window)
    ///
    /// Combine this with `WindowClass::FullScreen`
    /// to get borderless fullscreen mode
    /// (useful for correct Alt+Tab behaviour)
    NoDecorations,
}

impl Default for WindowDecorations {
    fn default() -> Self {
        WindowDecorations::Normal
    }
}

/// Where the window should be positioned, 
/// from the top left corner of the screen
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct WindowPlacement {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Default for WindowPlacement {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            width: self::DEFAULT_WIDTH,
            height: self::DEFAULT_HEIGHT,
        }
    }
}

/// What class the window should have (important for window managers). 
/// Currently not in use.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum WindowClass {
    /// Regular desktop window
    Normal,
    /// Popup window (some window managers handle this differently)
    Popup,
    /// Will open the window in full-screen mode
    /// and set it as the top-level window on the given monitor.
    /// Window size is ignored
    FullScreen,
    /// Start the window maximized
    Maximized,
    /// Start the window minimized
    Minimized,
    /// Window is hidden at startup.
    ///
    /// This is useful for background rendering. Many windowing systems
    /// do not properly support off-screen rendering (via OSMesa or similar).
    /// As a workaround, you can just create a hidden window
    Hidden,
}

impl Default for WindowClass {
    fn default() -> Self {
        WindowClass::Normal
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
    /// WebGl is not supported by webrender
    WebGlNotSupported,
    /// Couldn't create the display from the window and the EventsLoop
    DisplayCreateError(DisplayCreationError),
    /// OpenGL version is either too old or invalid
    Gl(IncompatibleOpenGl),
    /// Could not create an OpenGL context
    Context(ContextError),
    /// Could not create a window
    CreateError(CreationError),
    SwapBuffers(::glium::SwapBuffersError),
    /// IO error
    Io(::std::io::Error),
}

impl From<::glium::SwapBuffersError> for WindowCreateError {
    fn from(e: ::glium::SwapBuffersError) -> Self {
        WindowCreateError::SwapBuffers(e)
    }
}

impl From<CreationError> for WindowCreateError {
    fn from(e: CreationError) -> Self {
        WindowCreateError::CreateError(e)
    }
}

impl From<::std::io::Error> for WindowCreateError {
    fn from(e: ::std::io::Error) -> Self {
        WindowCreateError::Io(e)
    }
}

impl From<IncompatibleOpenGl> for WindowCreateError {
    fn from(e: IncompatibleOpenGl) -> Self {
        WindowCreateError::Gl(e)
    }
}

impl From<DisplayCreationError> for WindowCreateError {
    fn from(e: DisplayCreationError) -> Self {
        WindowCreateError::DisplayCreateError(e)
    }
}

impl From<ContextError> for WindowCreateError {
    fn from(e: ContextError) -> Self {
        WindowCreateError::Context(e)
    }
}

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
        self.events_loop_proxy.wakeup().unwrap_or_else(|_| { });
    }

    fn new_document_ready(&self, _: DocumentId, _scrolled: bool, _composite_needed: bool) {
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
pub struct Window<T: LayoutScreen> {
    // TODO: technically, having one EventsLoop for all windows is sufficient
    pub(crate) events_loop: EventsLoop,
    pub(crate) options: WindowCreateOptions,
    pub(crate) renderer: Option<Renderer>,
    pub(crate) display: Display,
    pub(crate) internal: WindowInternal,
    /// The solver for the UI, for caching the results of the computations
    pub(crate) solver: UiSolver<T>,
    // The background thread that is running for this window.
    // pub(crate) background_thread: Option<JoinHandle<()>>,
    /// The css (how the current window is styled)
    pub css: Css,
}

/// Used in the solver, for the root constraint
#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) struct WindowDimensions {
    pub(crate) layout_size: LayoutSize,
    pub(crate) width_var: Variable,
    pub(crate) height_var: Variable,
}

impl WindowDimensions {
    pub fn new_from_layout_size(layout_size: LayoutSize) -> Self {
        Self { 
            layout_size: layout_size,
            width_var: Variable::new(),
            height_var: Variable::new(),
        }
    }

    pub fn width(&self) -> f32 {
        self.layout_size.width_typed().get()
    }
    pub fn height(&self) -> f32 {
        self.layout_size.height_typed().get()
    }
}

/// Solver for solving the UI of the current window
pub(crate) struct UiSolver<T: LayoutScreen> {
    /// The actual solver
    pub(crate) solver: Solver,
    /// Dimensions of the root window
    pub(crate) window_dimensions: WindowDimensions,
    /// Solved layout from the previous frame (empty by default)
    /// This is necessary for caching the constraints of the given layout
    pub(crate) solved_layout: SolvedLayout<T>,
    /// The list of variables that has been added to the solver
    pub(crate) edit_variable_cache: EditVariableCache,
    /// The cache of the previous frames DOM tree
    pub(crate) dom_tree_cache: DomTreeCache,
}

impl<T: LayoutScreen> UiSolver<T> {
    pub(crate) fn query_bounds_of_rect(&self, rect_id: NodeId) {
        // TODO: After solving the UI, use this function to get the actual coordinates of an item in the UI.
        // This function should cache values accordingly
    }
}

pub(crate) struct WindowInternal {
    pub(crate) last_display_list_builder: BuiltDisplayList,
    pub(crate) layout_size: LayoutSize,
    pub(crate) api: RenderApi,
    pub(crate) epoch: Epoch,
    pub(crate) framebuffer_size: DeviceUintSize,
    pub(crate) pipeline_id: PipelineId,
    pub(crate) document_id: DocumentId,
    pub(crate) hidpi_factor: f32,
}

impl<T: LayoutScreen> Window<T> {

    /// Creates a new window
    pub fn new(options: WindowCreateOptions, css: Css) -> Result<Self, WindowCreateError>  {

        let events_loop = EventsLoop::new();

        let mut window = WindowBuilder::new()
            .with_dimensions(options.size.width, options.size.height)
            .with_title(options.title.clone())
            .with_decorations(options.decorations != WindowDecorations::NoDecorations)
            .with_maximized(options.class == WindowClass::Maximized);

        if options.class == WindowClass::FullScreen {
            let monitor = match options.monitor {
                WindowMonitorTarget::Primary => events_loop.get_primary_monitor(),
                WindowMonitorTarget::Custom(ref id) => id.clone(),
            };

            window = window.with_fullscreen(Some(monitor));
        }

        fn create_context_builder<'a>(vsync: bool, srgb: bool) -> ContextBuilder<'a> {
            let mut builder = ContextBuilder::new()
                .with_gl(glutin::GlRequest::GlThenGles {
                    opengl_version: (3, 2),
                    opengles_version: (3, 0),
                })
                .with_gl_profile(GlProfile::Core)
                .with_gl_debug_flag(false);
            if vsync {
                builder = builder.with_vsync(true);
            }
            if srgb {
                builder = builder.with_srgb(true);
            }
            builder
        }

        // For some reason, there is GL_INVALID_OPERATION stuff going on,
        // but the display works fine. TODO: report this to glium

        // Only create a context with VSync and SRGB if the context creation works
        let gl_window = GlWindow::new(window.clone(), create_context_builder(true, true), &events_loop)
            .or_else(|_| GlWindow::new(window.clone(), create_context_builder(true, false), &events_loop))
            .or_else(|_| GlWindow::new(window, create_context_builder(false, false), &events_loop))?;

        let hidpi_factor = gl_window.hidpi_factor();
        let display = Display::with_debug(gl_window, DebugCallbackBehavior::Ignore)?;

        unsafe {
            display.gl_window().make_current()?;
        }

        // draw the first frame in the background color
        use glium::Surface;
        let mut frame = display.draw();
        if let Some(depth) = options.clear_depth {
            if let Some(stencil) = options.clear_stencil {
                frame.clear_all_srgb((options.background.r, options.background.g, options.background.b, options.background.a), depth, stencil);
            }
            frame.clear_color_srgb_and_depth((options.background.r, options.background.g, options.background.b, options.background.a), depth);
        } else if let Some(stencil) = options.clear_stencil {
            frame.clear_color_srgb_and_stencil((options.background.r, options.background.g, options.background.b, options.background.a), stencil);
        }
        frame.clear_color_srgb(options.background.r, options.background.g, options.background.b, options.background.a);
        frame.finish()?;

        let gl = match display.gl_window().get_api() {
            glutin::Api::OpenGl => unsafe {
                gl::GlFns::load_with(|symbol|
                    display.gl_window().get_proc_address(symbol) as *const _)
            },
            glutin::Api::OpenGlEs => unsafe {
                gl::GlesFns::load_with(|symbol|
                    display.gl_window().get_proc_address(symbol) as *const _)
            },
            glutin::Api::WebGl => return Err(WindowCreateError::WebGlNotSupported),
        };

        let device_pixel_ratio = display.gl_window().hidpi_factor();

        let opts = RendererOptions {
            resource_override_path: None,
            debug: false,
            // pre-caching shaders means to compile all shaders on startup
            // this can take significant time and should be only used for testing the shaders
            precache_shaders: false,
            device_pixel_ratio,
            enable_subpixel_aa: true,
            enable_aa: true,
            clear_color: Some(options.background),
            enable_render_on_scroll: false,
            // TODO: Fallback to OSMesa if needed!
            // renderer_kind: RendererKind::Native,
            .. RendererOptions::default()
        };

        let framebuffer_size = {
            #[allow(deprecated)]
            let (width, height) = display.gl_window().get_inner_size_pixels().unwrap();
            DeviceUintSize::new(width, height)
        };
        let notifier = Box::new(Notifier::new(events_loop.create_proxy()));
        let (renderer, sender) = Renderer::new(gl.clone(), notifier, opts).unwrap();

        let api = sender.create_api();
        let document_id = api.add_document(framebuffer_size, 0);
        let epoch = Epoch(0);
        let pipeline_id = PipelineId(0, 0);
        let layout_size = framebuffer_size.to_f32() / TypedScale::new(device_pixel_ratio);
/*
        let (sender, receiver) = channel();
        let thread = Builder::new().name(options.title.clone()).spawn(move || Self::handle_event(receiver))?;
*/
        let mut solver = Solver::new();

        let window_dim = WindowDimensions::new_from_layout_size(layout_size);

        solver.add_edit_variable(window_dim.width_var, STRONG).unwrap();
        solver.add_edit_variable(window_dim.height_var, STRONG).unwrap();
        solver.suggest_value(window_dim.width_var, window_dim.width() as f64).unwrap();
        solver.suggest_value(window_dim.height_var, window_dim.height() as f64).unwrap();

        let window = Window {
            events_loop: events_loop,
            options: options,
            renderer: Some(renderer),
            display: display,
            css: css,
            internal: WindowInternal {
                layout_size: layout_size,
                api: api,
                epoch: epoch,
                framebuffer_size: framebuffer_size,
                pipeline_id: pipeline_id,
                document_id: document_id,
                hidpi_factor: hidpi_factor,
                last_display_list_builder: BuiltDisplayList::default(),
            },
            solver: UiSolver {
                solver: solver,
                window_dimensions: window_dim,
                solved_layout: SolvedLayout::empty(),
                edit_variable_cache: EditVariableCache::empty(),
                dom_tree_cache: DomTreeCache::empty(),
            }
        };

        Ok(window)
    }

    pub fn get_available_monitors() -> MonitorIter {
        MonitorIter {
            inner: EventsLoop::new().get_available_monitors(),
        }
    }
}

impl<T: LayoutScreen> Drop for Window<T> {
    fn drop(&mut self) {
        // self.background_thread.take().unwrap().join();
        let renderer = self.renderer.take().unwrap();
        renderer.deinit();
    }
}