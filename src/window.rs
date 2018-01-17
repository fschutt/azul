use webrender::api::*;
use webrender::{Renderer, ExternalImageHandler, OutputImageHandler, RendererOptions};
use glium::glutin::{self, EventsLoop, AvailableMonitorsIter,
						  MonitorId, EventsLoopProxy, ContextError, ContextBuilder, WindowBuilder};
use gleam::gl;
use glium::backend::glutin::DisplayCreationError;
use euclid::TypedScale;

use std::time::Duration;

const TITLE: &str = "WebRender Sample App";
const PRECACHE_SHADERS: bool = false;
const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;

pub struct WindowId(u32);

/// Options on how to initially create the window
pub struct CreateWindowOptions {
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

/// How should the window be decorated
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

/// Where the window should be positioned
pub struct WindowPlacement {
	pub x: u32,
	pub y: u32,
	pub width: u32,
	pub height: u32,
}

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

pub enum UpdateBehaviour {
	/// Redraw the window only if the mouse cursor is
	/// on top of the window
	UpdateOnHover,
	/// Always update the screen, regardless of the
	/// position of the mouse cursor
	UpdateAlways,
}

/// In which intervals should the screen be updated
pub enum UpdateMode {
	/// Retained = the screen is only updated when necessary.
	/// Underlying GlImages will be ignored and only updated when the UI changes
	Retained,
	/// Fixed update every X duration.
	FixedUpdate(Duration),
	/// Draw the screen as fast as possible.
	AsFastAsPossible,
}

/// Configuration of the mouse
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

pub enum WindowCreateError {
	WebGlNotSupported,
	DisplayCreateError(DisplayCreationError),
	Context(ContextError),
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
        self.events_loop_proxy.wakeup().unwrap_or_else(|e| {
        	error!("{}", e);
        });
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

/// Select, on which monitor the window should pop up.
pub enum WindowMonitorTarget {
	/// Window should appear on the primary monitor
	Primary,
	/// Window should appear on the current monitor
	Current,
	/// Use `Window::get_available_monitors()` to select the correct monitor
	Custom(MonitorId)
}

/// Represents one graphical window to be rendered.
pub struct Window {
	events_loop: EventsLoop,
}

impl Window {

	/// Creates a new window
	pub fn new(options: CreateWindowOptions) -> Result<Self, WindowCreateError>  {
		use glium::Display;

		let mut events_loop = EventsLoop::new();
		let window = WindowBuilder::new()
		    .with_dimensions(WIDTH, HEIGHT)
		    .with_title(TITLE);
		let context = ContextBuilder::new()
			.with_gl(glutin::GlRequest::GlThenGles {
			    opengl_version: (3, 2),
			    opengles_version: (3, 0),
			});
		let display = Display::new(window, context, &events_loop)?;

		use glium::glutin::GlContext;
		unsafe {
		    display.gl_window().make_current()?;
		}

		let gl = match display.gl_window().get_api() {
		    glutin::Api::OpenGl => unsafe {
		        gl::GlFns::load_with(|symbol| display.gl_window().get_proc_address(symbol) as *const _)
		    },
		    glutin::Api::OpenGlEs => unsafe {
		        gl::GlesFns::load_with(|symbol| display.gl_window().get_proc_address(symbol) as *const _)
		    },
		    glutin::Api::WebGl => unimplemented!(),
		};

		let device_pixel_ratio = display.gl_window().hidpi_factor();

		println!("OpenGL version {}", gl.get_string(gl::VERSION));
		println!("Device pixel ratio: {}", device_pixel_ratio);
		println!("Loading shaders...");

		let opts = RendererOptions {
		    resource_override_path: None,
		    debug: true,
		    precache_shaders: PRECACHE_SHADERS,
		    device_pixel_ratio,
		    clear_color: Some(ColorF::new(0.3, 0.0, 0.0, 1.0)),
		    //scatter_gpu_cache_updates: false,
		    .. RendererOptions::default()
		};

		let framebuffer_size = {
			#[allow(deprecated)]
		    let (width, height) = display.gl_window().get_inner_size_pixels().unwrap();
		    DeviceUintSize::new(width, height)
		};
		let notifier = Box::new(Notifier::new(events_loop.create_proxy()));
		let (mut renderer, sender) = Renderer::new(gl.clone(), notifier, opts).unwrap();
		let api = sender.create_api();
		let document_id = api.add_document(framebuffer_size, 0);

		let (external, output) = Self::get_image_handlers(&*gl);

		if let Some(output_image_handler) = output {
		    renderer.set_output_image_handler(output_image_handler);
		}

		if let Some(external_image_handler) = external {
		    renderer.set_external_image_handler(external_image_handler);
		}

		let epoch = Epoch(0);
		let pipeline_id = PipelineId(0, 0);
		let layout_size = framebuffer_size.to_f32() / TypedScale::new(device_pixel_ratio);
		let mut builder = DisplayListBuilder::new(pipeline_id, layout_size);
		let mut resources = ResourceUpdates::new();

		let mut window = Window {
			events_loop: events_loop,
		};
		window.render(
		    &api,
		    &mut builder,
		    &mut resources,
		    framebuffer_size,
		    pipeline_id,
		    document_id,
		);
		api.set_display_list(
		    document_id,
		    epoch,
		    None,
		    layout_size,
		    builder.finalize(),
		    true,
		    resources,
		);
		api.set_root_pipeline(document_id, pipeline_id);
		api.generate_frame(document_id, None);
		Ok(window)
	}

	pub fn get_available_monitors() -> MonitorIter {
		MonitorIter {
			inner: EventsLoop::new().get_available_monitors(),
		}
	}

	fn render(
	    &mut self,
	    api: &RenderApi,
	    builder: &mut DisplayListBuilder,
	    resources: &mut ResourceUpdates,
	    framebuffer_size: DeviceUintSize,
	    pipeline_id: PipelineId,
	    document_id: DocumentId,
	) {

	}

	fn on_event(&mut self, event: glutin::Event, render_api: &RenderApi, document_id: DocumentId)
	-> bool
	{
	    false
	}

	fn get_image_handlers(
	    _gl: &gl::Gl,
	) -> (Option<Box<ExternalImageHandler>>,
	      Option<Box<OutputImageHandler>>)
	{
	    (None, None)
	}

	fn draw_custom(&self, _gl: &gl::Gl)
	{

	}
}