use webrender::api::*;
use webrender::{Renderer, RendererOptions};
use glium::{IncompatibleOpenGl, Display};
use glium::debug::DebugCallbackBehavior;
use glium::glutin::{self, EventsLoop, AvailableMonitorsIter, GlProfile, GlContext, GlWindow, CreationError,
                          MonitorId, EventsLoopProxy, ContextError, ContextBuilder, WindowBuilder};
use gleam::gl;
use glium::backend::glutin::DisplayCreationError;
use euclid::TypedScale;
use cassowary::Solver;

use std::time::Duration;

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
#[derive(Clone)]
pub struct WindowCreateOptions {
    /// Title of the window
    pub title: String,
    /// OpenGL clear color
    pub background: ColorF,
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
#[derive(Copy, Clone, PartialEq)]
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
#[derive(Copy, Clone, PartialEq)]
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
#[derive(Copy, Clone, PartialEq)]
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

#[derive(Copy, Clone)]
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
#[derive(Copy, Clone)]
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
#[derive(Copy, Clone)]
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
    /// 
    Context(ContextError),
    CreateError(CreationError),
    Io(::std::io::Error),
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

impl Default for WindowMonitorTarget {
    fn default() -> Self {
        WindowMonitorTarget::Primary
    }
}

/// Represents one graphical window to be rendered
pub struct Window {
    // TODO: technically, having one EventsLoop for all windows is sufficient
    pub(crate) events_loop: EventsLoop,
    pub(crate) options: WindowCreateOptions,
    pub(crate) renderer: Option<Renderer>,
    pub(crate) display: Display,
    pub(crate) internal: WindowInternal,
    /// The solver for the UI, for caching the results of the computations
    pub(crate) solver: UiSolver,
    // The background thread that is running for this window.
    // pub(crate) background_thread: Option<JoinHandle<()>>,
}

/// Solver for solving the UI of the current window
pub(crate) struct UiSolver {
    pub(crate) solver: Solver,
}

pub(crate) struct WindowInternal {
    pub(crate) layout_size: LayoutSize,
    pub(crate) api: RenderApi,
    pub(crate) epoch: Epoch,
    pub(crate) framebuffer_size: DeviceUintSize,
    pub(crate) pipeline_id: PipelineId,
    pub(crate) document_id: DocumentId,
    pub(crate) hidpi_factor: f32,
}

impl Window {

    /// Creates a new window
    pub fn new(options: WindowCreateOptions) -> Result<Self, WindowCreateError>  {

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

        let context = ContextBuilder::new()
            .with_gl(glutin::GlRequest::GlThenGles {
                opengl_version: (3, 2),
                opengles_version: (3, 0),
            })
            .with_gl_profile(GlProfile::Core)
            .with_vsync(true)
            /*.with_multisampling(4)*/
            .with_srgb(true)
            .with_gl_debug_flag(false);

        // For some reason, there is GL_INVALID_OPERATION stuff going on,
        // but the display works fine. TODO: report this to glium
        let gl_window = GlWindow::new(window, context, &events_loop)?;
        let hidpi_factor = gl_window.hidpi_factor();
        let display = Display::with_debug(gl_window, DebugCallbackBehavior::Ignore)?;

        unsafe {
            display.gl_window().make_current()?;
        }

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
        let window = Window {
            events_loop: events_loop,
            options: options,
            renderer: Some(renderer),
            display: display,
            internal: WindowInternal {
                layout_size: layout_size,
                api: api,
                epoch: epoch,
                framebuffer_size: framebuffer_size,
                pipeline_id: pipeline_id,
                document_id: document_id,
                hidpi_factor: hidpi_factor,
            },
            solver: UiSolver {
                solver: Solver::new(),
            }
        };

        Ok(window)
    }
/*
    pub fn send_event(&self, event: InputEvent) -> Result<(), SendError<InputEvent>> {
        self.sender.send(event)
    }

    fn handle_event(rx: Receiver<InputEvent>) {
        while let Ok(event) = rx.recv() {
            println!("handling event: {:?}", event);
        }
    }
*/
    pub fn get_available_monitors() -> MonitorIter {
        MonitorIter {
            inner: EventsLoop::new().get_available_monitors(),
        }
    }

    fn render(&mut self) {
        /*
            /// layout
            window.render(
                &window.internal.api,
                &mut window.internal.builder,
                &mut window.internal.resources,
                window.internal.framebuffer_size,
                window.internal.pipeline_id,
                window.internal.document_id,
            );

            // set display list
            window.internal.api.set_display_list(
                window.internal.document_id,
                window.internal.epoch,
                None,
                layout_size,
                window.internal.builder.finalize(),
                true,
                window.internal.resources,
            );

            window.internal.api.set_root_pipeline(document_id, pipeline_id);
            window.internal.api.generate_frame(document_id, None);

            /*
            'outer: for event in window.display.wait_events() {
                let mut events = Vec::new();
                events.push(event);
                events.extend(window.poll_events());

                for event in events {
                    match event {
                        glutin::Event::Closed |
                        glutin::Event::KeyboardInput(_, _, Some(glutin::VirtualKeyCode::Escape)) => break 'outer,

                        glutin::Event::KeyboardInput(
                            glutin::ElementState::Pressed,
                            _,
                            Some(glutin::VirtualKeyCode::P),
                        ) => {
                            renderer.toggle_debug_flags(webrender::DebugFlags::PROFILER_DBG);
                        }
                        glutin::Event::KeyboardInput(
                            glutin::ElementState::Pressed,
                            _,
                            Some(glutin::VirtualKeyCode::O),
                        ) => {
                            renderer.toggle_debug_flags(webrender::DebugFlags::RENDER_TARGET_DBG);
                        }
                        glutin::Event::KeyboardInput(
                            glutin::ElementState::Pressed,
                            _,
                            Some(glutin::VirtualKeyCode::I),
                        ) => {
                            renderer.toggle_debug_flags(webrender::DebugFlags::TEXTURE_CACHE_DBG);
                        }
                        glutin::Event::KeyboardInput(
                            glutin::ElementState::Pressed,
                            _,
                            Some(glutin::VirtualKeyCode::B),
                        ) => {
                            renderer.toggle_debug_flags(webrender::DebugFlags::ALPHA_PRIM_DBG);
                        }
                        glutin::Event::KeyboardInput(
                            glutin::ElementState::Pressed,
                            _,
                            Some(glutin::VirtualKeyCode::S),
                        ) => {
                            renderer.toggle_debug_flags(webrender::DebugFlags::COMPACT_PROFILER);
                        }
                        glutin::Event::KeyboardInput(
                            glutin::ElementState::Pressed,
                            _,
                            Some(glutin::VirtualKeyCode::Q),
                        ) => {
                            renderer.toggle_debug_flags(webrender::DebugFlags::GPU_TIME_QUERIES
                                | webrender::DebugFlags::GPU_SAMPLE_QUERIES);
                        }
                        glutin::Event::KeyboardInput(
                            glutin::ElementState::Pressed,
                            _,
                            Some(glutin::VirtualKeyCode::Key1),
                        ) => {
                            api.set_window_parameters(
                                document_id,
                                framebuffer_size,
                                DeviceUintRect::new(DeviceUintPoint::zero(), framebuffer_size),
                                1.0
                            );
                        }
                        glutin::Event::KeyboardInput(
                            glutin::ElementState::Pressed,
                            _,
                            Some(glutin::VirtualKeyCode::Key2),
                        ) => {
                            api.set_window_parameters(
                                document_id,
                                framebuffer_size,
                                DeviceUintRect::new(DeviceUintPoint::zero(), framebuffer_size),
                                2.0
                            );
                        }
                        glutin::Event::KeyboardInput(
                            glutin::ElementState::Pressed,
                            _,
                            Some(glutin::VirtualKeyCode::M),
                        ) => {
                            api.notify_memory_pressure();
                        }
                        #[cfg(feature = "capture")]
                        glutin::Event::KeyboardInput(
                            glutin::ElementState::Pressed,
                            _,
                            Some(glutin::VirtualKeyCode::C),
                        ) => {
                            let path: PathBuf = "../captures/example".into();
                            //TODO: switch between SCENE/FRAME capture types
                            // based on "shift" modifier, when `glutin` is updated.
                            let bits = CaptureBits::all();
                            api.save_capture(path, bits);
                        }
                        _ => if example.on_event(event, &api, document_id) {
                            let mut builder = DisplayListBuilder::new(pipeline_id, layout_size);
                            let mut resources = ResourceUpdates::new();

                            // re-layout

                            example.render(
                                &api,
                                &mut builder,
                                &mut resources,
                                framebuffer_size,
                                pipeline_id,
                                document_id,
                            );

                            // reset display list
                            api.set_display_list(
                                document_id,
                                epoch,
                                None,
                                layout_size,
                                builder.finalize(),
                                true,
                                resources,
                            );
                            api.generate_frame(document_id, None);
                        }
                    }
                }
            */

            window.renderer.update();
            window.renderer.render(window.internal.framebuffer_size).unwrap();
            window.display.swap_buffers().ok();
        */
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        // self.background_thread.take().unwrap().join();
        let renderer = self.renderer.take().unwrap();
        renderer.deinit();
    }
}