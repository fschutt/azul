use std::time::Duration as StdDuration;
use core::fmt;
use core::ffi::c_void;
use core::cell::RefCell;
use alloc::{
    rc::Rc,
};
use webrender::{
    render_api::{
        RenderApi as WrRenderApi,
    },
    api::{
        DocumentId as WrDocumentId,
        units::{
            LayoutSize as WrLayoutSize,
            DeviceIntRect as WrDeviceIntRect,
            DeviceIntPoint as WrDeviceIntPoint,
            DeviceIntSize as WrDeviceIntSize,
        },
        RenderNotifier as WrRenderNotifier,
    },
    Transaction as WrTransaction,
    PipelineInfo as WrPipelineInfo,
    RendererOptions as WrRendererOptions,
    Renderer as WrRenderer,
    ShaderPrecacheFlags as WrShaderPrecacheFlags,
    Shaders as WrShaders,
    RendererError as WrRendererError,
};
use glutin::{
    event_loop::{EventLoopProxy as GlutinEventLoopProxy, EventLoopWindowTarget, EventLoop},
    window::{
        Window as GlutinWindow,
        WindowBuilder as GlutinWindowBuilder,
        WindowId as GlutinWindowId,
    },
    CreationError as GlutinCreationError,
    ContextError as GlutinContextError,
    ContextBuilder, Context, WindowedContext,
    NotCurrent, PossiblyCurrent,
    Context as GlutinContext,
};
use gleam::gl::{self, Gl};
use clipboard2::{Clipboard as _, ClipboardError, SystemClipboard};
use crate::{
    compositor::Compositor,
    display_shader::DisplayShader,
};
use azul_core::{
    callbacks::{PipelineId, RefAny},
    task::ExternalSystemCallbacks,
    display_list::{CachedDisplayList, RenderCallbacks},
    app_resources::{ResourceUpdate, AppResources, LoadFontFn, IdNamespace, LoadImageFn},
    gl::{GlContextPtr, OptionGlContextPtr, GlShaderCreateError, Texture},
    window::LazyFcCache,
    window_state::{Events, NodesToCheck},
};
use azul_css::{LayoutPoint, AzString, OptionAzString, LayoutSize};
use glutin::monitor::MonitorHandle as WinitMonitorHandle;
pub use azul_core::window::*;

// TODO: Right now it's not very ergonomic to cache shaders between
// renderers - notify webrender about this.
const WR_SHADER_CACHE: Option<&Rc<RefCell<WrShaders>>> = None;

/// returns a unique identifier for the monitor, used for hashing
pub(crate) fn monitor_handle_get_id(handle: &WinitMonitorHandle) -> usize {
    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))] {
        use glutin::platform::unix::MonitorHandleExtUnix;
        handle.native_id() as usize
    }
    #[cfg(target_os = "windows")] {
        use glutin::platform::windows::MonitorHandleExtWindows;
        handle.hmonitor() as usize
    }
    #[cfg(target_os = "macos")] {
        use glutin::platform::macos::MonitorHandleExtMacOS;
        handle.native_id() as usize
    }
    #[cfg(target_arch = "wasm32")] {
        0 // there is only one screen
    }
}

pub(crate) fn monitor_new(handle: WinitMonitorHandle, is_primary_monitor: bool) -> Monitor {

    let name = handle.name();
    let size = handle.size();
    let position = handle.position();
    let scale_factor = handle.scale_factor();
    let video_modes = handle.video_modes().map(|v| {
        let v_size = v.size();
        VideoMode {
            size: LayoutSize { width: v_size.width as isize, height: v_size.height as isize },
            bit_depth: v.bit_depth(),
            refresh_rate: v.refresh_rate(),
        }
    }).collect::<Vec<_>>();

    Monitor {
        id: monitor_handle_get_id(&handle),
        name: name.map(|n| AzString::from(n)).into(),
        size: LayoutSize { width: size.width as isize, height: size.height as isize },
        position: LayoutPoint { x: position.x as isize, y: position.y as isize },
        scale_factor,
        video_modes: video_modes.into(),
        is_primary_monitor,
    }
}

/// returns the maximum framerate supported by this monitor
pub(crate) fn monitor_get_max_supported_framerate(mon: &Monitor) -> Option<StdDuration> {
    let max_refresh_rate = mon.video_modes.as_slice().iter().map(|m| m.refresh_rate).max()?;
    StdDuration::from_secs(1).checked_div(max_refresh_rate as u32)
}

#[derive(Copy, Clone)]
pub struct UserEvent {
    pub window_id: GlutinWindowId,
    pub composite_needed: bool,
}

struct Notifier {
    // ID of the window that this notifier is attached to
    window_id: GlutinWindowId,
    events_proxy: GlutinEventLoopProxy<UserEvent>,
}

impl Notifier {
    fn new(window_id: GlutinWindowId, events_proxy: GlutinEventLoopProxy<UserEvent>) -> Notifier {
        Notifier { events_proxy, window_id }
    }
}

impl WrRenderNotifier for Notifier {
    fn clone(&self) -> Box<dyn WrRenderNotifier> {
        Box::new(Notifier {
            events_proxy: self.events_proxy.clone(),
            window_id: self.window_id,
        })
    }

    fn wake_up(&self, composite_needed: bool) {
        #[cfg(not(target_os = "android"))]
        let _ = self.events_proxy.send_event(UserEvent {
            window_id: self.window_id,
            composite_needed
        });
    }

    fn new_frame_ready(&self,
                       _: WrDocumentId,
                       _scrolled: bool,
                       composite_needed: bool,
                       _render_time: Option<u64>) {
        self.wake_up(composite_needed);
    }
}

pub(crate) enum ContextState {
    MakeCurrentInProgress,
    Current(WindowedContext<PossiblyCurrent>),
    NotCurrent(WindowedContext<NotCurrent>),
}

/// Creates a wrapper with `.make_current()` and `.make_not_current()`
/// around `ContextState` and `HeadlessContextState`
impl ContextState {
    pub fn make_current(&mut self) {

        use std::mem;
        use self::ContextState::*;

        let mut new_state = match mem::replace(self, ContextState::MakeCurrentInProgress) {
            Current(c) => Current(c),
            NotCurrent(nc) => Current(unsafe { nc.make_current().unwrap() }),
            MakeCurrentInProgress => MakeCurrentInProgress,
        };

        mem::swap(self, &mut new_state);
    }

    /*
    pub fn make_not_current(&mut self) {

        use std::mem;
        use self::ContextState::*;

        let mut new_state = match mem::replace(self, ContextState::MakeCurrentInProgress) {
            Current(c) => NotCurrent(unsafe { c.make_not_current().unwrap() }),
            NotCurrent(nc) => NotCurrent(nc),
            MakeCurrentInProgress => MakeCurrentInProgress,
        };

        mem::swap(self, &mut new_state);
    }
    */

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

#[derive(Debug)]
pub enum WindowCreateError {
    Glutin(GlutinCreationError),
    WebRender(WrRendererError),
    NoHwAccelerationAvailable,
    FailedToInitializeWr,
    ContextError(GlutinContextError),
}

impl_from!(GlutinCreationError, WindowCreateError::Glutin);
impl_from!(WrRendererError, WindowCreateError::WebRender);
impl_from!(GlutinContextError, WindowCreateError::ContextError);

impl fmt::Display for WindowCreateError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            WindowCreateError::Glutin(g) => write!(f, "glutin: {}", g),
            WindowCreateError::WebRender(wr) => write!(f, "webrender: {:?}", wr),
            WindowCreateError::NoHwAccelerationAvailable => write!(f, "renderer: hardware acceleration was requested, but windowing system does not support hw acceleration"),
            WindowCreateError::FailedToInitializeWr => write!(f, "webrender: failed to initialize"),
            WindowCreateError::ContextError(c) => write!(f, "glutin: failed to make context current"),
        }
    }
}

/// Represents one graphical window to be rendered
pub struct Window {
    /// Stores things like scroll states, display list + epoch for the window
    pub(crate) internal: WindowInternal,
    /// The display, i.e. the actual window (+ the attached OpenGL context)
    pub(crate) display: ContextState,
    /// Main render API that can be used to register and un-register fonts and images
    pub(crate) render_api: WrRenderApi,
    // software_context: Option<Rc<swgl::Context>>
    hardware_gl: Rc<dyn Gl>,
    software_gl: Option<Rc<swgl::Context>>,
    /// Main renderer, responsible for rendering all windows
    ///
    /// This is `Some()` because of the `FakeDisplay` destructor: On shutdown,
    /// the `renderer` gets destroyed before the other fields do, that is why the
    /// renderer can be `None`
    pub(crate) renderer: Option<WrRenderer>,
}

impl Window {

    const CALLBACKS: RenderCallbacks = RenderCallbacks {
        insert_into_active_gl_textures: azul_core::gl::insert_into_active_gl_textures,
        layout_fn: azul_layout::do_the_layout,
        load_font_fn: LoadFontFn { cb: azulc_lib::font_loading::font_source_get_bytes },
        load_image_fn: LoadImageFn { cb: azulc_lib::image_loading::image_source_get_bytes },
        parse_font_fn: azul_layout::text_layout::parse_font_fn,
    };

    // copied from server/webrender/wrench
    fn upload_software_to_native(&self) {
        let swgl = match self.software_gl.as_ref() {
            Some(swgl) => swgl,
            None => return,
        };
        swgl.finish();
        let gl = &self.hardware_gl;
        let tex = gl.gen_textures(1)[0];
        gl.bind_texture(gl::TEXTURE_2D, tex);
        let (data_ptr, w, h, stride) = swgl.get_color_buffer(0, true);
        assert!(stride == w * 4);
        let buffer = unsafe { std::slice::from_raw_parts(data_ptr as *const u8, w as usize * h as usize * 4) };
        gl.tex_image_2d(gl::TEXTURE_2D, 0, gl::RGBA8 as gl::GLint, w, h, 0, gl::BGRA, gl::UNSIGNED_BYTE, Some(buffer));
        let fb = gl.gen_framebuffers(1)[0];
        gl.bind_framebuffer(gl::READ_FRAMEBUFFER, fb);
        gl.framebuffer_texture_2d(gl::READ_FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, tex, 0);
        gl.blit_framebuffer(0, 0, w, h, 0, 0, w, h, gl::COLOR_BUFFER_BIT, gl::NEAREST);
        gl.delete_framebuffers(&[fb]);
        gl.delete_textures(&[tex]);
        gl.finish();
    }

    fn get_gl_context(&self) -> Rc<dyn Gl> {
        match self.software_gl.as_ref() {
            Some(sw) => sw.clone(),
            None => self.hardware_gl.clone(),
        }
    }

    pub(crate) fn get_gl_context_ptr(&self) -> GlContextPtr {
        match self.software_gl.as_ref() {
            Some(sw) => GlContextPtr::new(RendererType::Software, sw.clone()),
            None => GlContextPtr::new(RendererType::Hardware, self.hardware_gl.clone()),
        }
    }

    /// Creates a new window
    pub(crate) fn new(
        data: &mut RefAny,
        mut options: WindowCreateOptions,
        events_loop: &EventLoopWindowTarget<UserEvent>,
        proxy: &GlutinEventLoopProxy<UserEvent>,
        app_resources: &mut AppResources,
        fc_cache: &mut LazyFcCache,
    ) -> Result<Self, WindowCreateError> {

        use crate::wr_translate::translate_document_id_wr;
        use webrender::ProgramCache as WrProgramCache;
        use webrender::api::ColorF as WrColorF;
        use crate::wr_translate::translate_id_namespace_wr;

        // NOTE: It would be OK to use &RenderApi here, but it's better
        // to make sure that the RenderApi is currently not in use by anything else.

        // NOTE: All windows MUST have a shared EventsLoop, creating a new EventLoop for the
        // new window causes a segfault.

        // always use a transparent background, reduces visible artifacts on resize
        let is_transparent_background = true;

        let window_builder = Self::create_window_builder(
            is_transparent_background,
            options.theme.into_option(),
            &options.state.platform_specific_options
        );

        // set the visibility of the window initially to false, only show the
        // window after the first frame has been drawn + swapped
        let window_builder = window_builder.with_visible(false);

        // Only create a context with VSync and SRGB if the context creation works
        let (glutin_window, window_renderer_info) = Self::create_glutin_window(window_builder, options.renderer.into_option().unwrap_or_default(), &events_loop)?;
        let window_id = glutin_window.window().id();
        let mut window_context = ContextState::NotCurrent(glutin_window);

        let (hidpi_factor, system_hidpi_factor) = get_hidpi_factor(&window_context.window(), &events_loop);
        options.state.size.hidpi_factor = hidpi_factor;
        options.state.size.system_hidpi_factor = system_hidpi_factor;

        let renderer_types = match options.renderer.into_option() {
            Some(s) => {
                // assert that the OS window supports hardware acceleration
                if window_renderer_info.hw_accel == HwAcceleration::Disabled && s.hw_accel == HwAcceleration::Enabled {
                    return Err(WindowCreateError::NoHwAccelerationAvailable);
                }
                vec![RendererType::Hardware]
            },
            None => vec![
                RendererType::Hardware,
                RendererType::Software,
            ]
        };

        // fetch the GlContextPtr
        window_context.make_current();

        // the hardware OpenGL context has to always be initialized -
        // TODO: change this - see minifb for a pure-software window!
        let hardware_gl = Self::initialize_hardware_gl_context(&window_context.context().unwrap())?;
        let mut renderer_sender = None;
        let mut software_gl = None;

        // Note: Notifier is fairly useless, since rendering is
        // completely single-threaded, see comments on RenderNotifier impl

        let gen_opts = || {
            // NOTE: If the clear_color is None, this may lead to "black screens"
            // (because black is the default color) - so instead, white should be the default
            // However, if the clear color is specified, then it's hard creating transparent windows
            // (because of bugs in webrender / handling multi-window background colors).
            // Therefore the background color has to be set before render() is invoked.
            WrRendererOptions {
                resource_override_path: None,
                precache_flags: WrShaderPrecacheFlags::EMPTY,
                device_pixel_ratio: hidpi_factor,
                enable_subpixel_aa: true,
                enable_aa: true,
                cached_programs: Some(WrProgramCache::new(None)),
                clear_color: Some(WrColorF { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }), // transparent
                enable_multithreading: true,
                .. WrRendererOptions::default()
            }
        };

        for rt in renderer_types.into_iter() {

            match rt {
                RendererType::Software => {
                    let s = Self::initialize_software_gl_context();
                    let notifier = Box::new(Notifier::new(window_id, proxy.clone()));
                    if let Ok(r) = WrRenderer::new(s.clone(), notifier, gen_opts(), WR_SHADER_CACHE) {
                        renderer_sender = Some(r);
                    }
                    software_gl = Some(s);
                    break;
                },
                RendererType::Hardware => {
                    let notifier = Box::new(Notifier::new(window_id, proxy.clone()));
                    let renderer = WrRenderer::new(hardware_gl.clone(), notifier, gen_opts(), WR_SHADER_CACHE);
                    match renderer {
                        Ok(r) => {
                            renderer_sender = Some(r);
                            break;
                        },
                        Err(e) => {
                            #[cfg(feature = "logging")] {
                                warn!("error initializing hardware webrender: {:?}", e);
                            }
                        }
                    }
                }
            }
        }

        let (mut renderer, sender) = match renderer_sender {
            Some(s) => s,
            None => { return Err(WindowCreateError::FailedToInitializeWr); },
        };

        renderer.set_external_image_handler(Box::new(Compositor::default()));

        let render_api = sender.create_api();

        // renderer created

        // Synchronize the state from the WindowCreateOptions with the window for the first time
        // (set maxmimization, etc.)
        initialize_os_window(&options.state, &window_context.window());

        let framebuffer_size = {
            let physical_size = options.state.size.dimensions.to_physical(hidpi_factor as f32);
            WrDeviceIntSize::new(physical_size.width as i32, physical_size.height as i32)
        };

        let document_id = translate_document_id_wr(render_api.add_document(framebuffer_size));

        // TODO: The PipelineId is what gets passed to the OutputImageHandler
        // (the code that coordinates displaying the rendered texture).
        //
        // Each window is a "pipeline", i.e a new web page in webrender terms,
        // however, there is only one global renderer, in order to save on memory,
        // The pipeline ID is important, in order to coordinate the rendered textures
        // back to their windows and window positions.
        let pipeline_id = PipelineId::new();

        app_resources.add_pipeline(pipeline_id);

        #[cfg(target_os = "windows")] {
            use crate::wr_translate::winit_translate::translate_winit_theme;
            use glutin::platform::windows::WindowExtWindows;
            options.state.theme = translate_winit_theme(window_context.window().theme());
        }

        let mut initial_resource_updates = Vec::new();
        let id_namespace = translate_id_namespace_wr(render_api.get_namespace_id());

        let gl_context_ptr = match software_gl.as_ref() {
            Some(s) => GlContextPtr::new(RendererType::Software, s.clone()),
            None => GlContextPtr::new(RendererType::Hardware, hardware_gl.clone()),
        };

        let internal = WindowInternal::new(
            WindowInternalInit {
                window_create_options: options,
                document_id,
                pipeline_id,
                id_namespace
            },
            data,
            app_resources,
            &OptionGlContextPtr::Some(gl_context_ptr.clone()),
            &mut initial_resource_updates,
            Window::CALLBACKS,
            fc_cache,
        );

        let mut window = Window {
            display: window_context,
            render_api,
            renderer: Some(renderer),
            software_gl,
            hardware_gl,
            internal,
        };

        let mut txn = WrTransaction::new();

        window.rebuild_display_list(&mut txn, &app_resources, initial_resource_updates);
        window.render_async(txn, /* display list was rebuilt */ true);

        Ok(window)
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
    fn create_window_context_builder<'a>(vsync: Vsync, srgb: Srgb, hardware_acceleration: HwAcceleration) -> ContextBuilder<'a, NotCurrent> {

        // See #33 - specifying a specific OpenGL version
        // makes winit crash on older Intel drivers, which is why we
        // don't specify a specific OpenGL version here
        //
        // TODO: The comment above might be old, see if it still happens and / or fallback to CPU

        let context_builder = ContextBuilder::new();

        #[cfg(debug_assertions)]
        let gl_debug_enabled = true;
        #[cfg(not(debug_assertions))]
        let gl_debug_enabled = false;

        context_builder
            .with_gl_debug_flag(gl_debug_enabled)
            .with_gl(glutin::GlRequest::GlThenGles {
                opengl_version: (3, 1),
                opengles_version: (3, 0),
            })
            .with_vsync(vsync.is_enabled())
            .with_srgb(srgb.is_enabled())
            .with_hardware_acceleration(Some(hardware_acceleration.is_enabled()))
    }

    fn create_glutin_window(window_builder: GlutinWindowBuilder, options: RendererOptions, event_loop: &EventLoopWindowTarget<UserEvent>)
    -> Result<(WindowedContext<NotCurrent>, RendererOptions), GlutinCreationError>
    {
        let opts = &[
            options,

            RendererOptions::new(Vsync::Enabled,  Srgb::Disabled, HwAcceleration::Enabled),
            RendererOptions::new(Vsync::Disabled, Srgb::Enabled,  HwAcceleration::Enabled),
            RendererOptions::new(Vsync::Disabled, Srgb::Disabled, HwAcceleration::Enabled),

            RendererOptions::new(Vsync::Enabled,  Srgb::Disabled, HwAcceleration::Disabled),
            RendererOptions::new(Vsync::Disabled, Srgb::Enabled,  HwAcceleration::Disabled),
            RendererOptions::new(Vsync::Disabled, Srgb::Disabled, HwAcceleration::Disabled),
        ];

        let mut last_err = None;

        for o in opts.iter() {
            match Self::create_window_context_builder(o.vsync, o.srgb, o.hw_accel).build_windowed(window_builder.clone(), event_loop) {
                Ok(s) => return Ok((s, *o)),
                Err(e) => { last_err = Some(e); },
            }
        }

        Err(last_err.unwrap_or(GlutinCreationError::NoAvailablePixelFormat))
    }

    fn initialize_hardware_gl_context(gl_context: &GlutinContext<PossiblyCurrent>) -> Result<Rc<dyn Gl>, GlutinCreationError> {
        use glutin::Api;
        match gl_context.get_api() {
            Api::OpenGl => Ok(unsafe { gl::GlFns::load_with(|symbol| gl_context.get_proc_address(symbol) as *const _) }),
            Api::OpenGlEs => Ok(unsafe { gl::GlesFns::load_with(|symbol| gl_context.get_proc_address(symbol) as *const _ ) }),
            Api::WebGl => Err(GlutinCreationError::NoBackendAvailable("WebGL".into())),
        }
    }

    fn initialize_software_gl_context() -> Rc<swgl::Context> {
        Rc::new(swgl::Context::create())
    }

    /// Calls the layout function again and updates the self.internal.gl_texture_cache field
    pub fn regenerate_styled_dom(
        &mut self,
        data: &mut RefAny,
        app_resources: &mut AppResources,
        resource_updates: &mut Vec<ResourceUpdate>,
        fc_cache: &mut LazyFcCache,
    ) {
        self.internal.regenerate_styled_dom(
            data,
            app_resources,
            &OptionGlContextPtr::Some(self.get_gl_context_ptr()),
            resource_updates,
            Window::CALLBACKS,
            fc_cache,
        );
    }

    /// Only re-build the display list and send it to webrender
    #[cfg(not(test))]
    pub fn rebuild_display_list(
        &mut self,
        txn: &mut WrTransaction,
        app_resources: &AppResources,
        resources: Vec<ResourceUpdate>
    ) {

        use crate::wr_translate::{
            wr_translate_pipeline_id,
            wr_translate_document_id,
            wr_translate_display_list,
            wr_translate_epoch,
            wr_translate_resource_update,
        };
        use webrender::render_api::Transaction as WrTransaction;

        // NOTE: Display list has to be rebuilt every frame, otherwise, the epochs get out of sync
        let cached_display_list = CachedDisplayList::new(
            self.internal.epoch,
            self.internal.pipeline_id,
            &self.internal.current_window_state,
            &self.internal.layout_results,
            &self.internal.gl_texture_cache,
            app_resources,
        );

        println!("display_list: {:#?}", cached_display_list.root);
        let display_list = wr_translate_display_list(cached_display_list, self.internal.pipeline_id);

        let logical_size = WrLayoutSize::new(self.internal.current_window_state.size.dimensions.width, self.internal.current_window_state.size.dimensions.height);
        txn.update_resources(resources.into_iter().map(wr_translate_resource_update).collect());
        txn.set_display_list(
            wr_translate_epoch(self.internal.epoch),
            None,
            logical_size.clone(),
            (wr_translate_pipeline_id(self.internal.pipeline_id), display_list),
            true,
        );
    }

    /// Synchronize the `self.internal.previous_window_state` with the `self.internal.current_window_state`
    ///  updating the OS-level window to reflect the new state
    pub fn synchronize_window_state_with_os(&mut self, new_state: WindowState, current_window_monitor: Monitor) -> bool {

        use crate::wr_translate::winit_translate::{translate_logical_position, translate_logical_size};
        use glutin::window::Fullscreen;

        let mut window_was_updated = false;

        // theme
        if self.internal.current_window_state.theme != new_state.theme {
            // self.display.window().set_theme(new_state.theme); // - doesn't work
            self.internal.current_window_state.theme = new_state.theme;
            window_was_updated = true;
        }


        // title
        if self.internal.current_window_state.title.as_str() != new_state.title.as_str() {
            self.display.window().set_title(new_state.title.as_str());
        }



        // size
        if self.internal.current_window_state.size.dimensions != new_state.size.dimensions {
            self.display.window().set_inner_size(translate_logical_size(new_state.size.dimensions));
            window_was_updated = true;
        }

        if self.internal.current_window_state.size.min_dimensions != new_state.size.min_dimensions {
            self.display.window().set_min_inner_size(new_state.size.min_dimensions.into_option().map(Into::into).map(translate_logical_size));
            window_was_updated = true;
        }

        if self.internal.current_window_state.size.max_dimensions != new_state.size.max_dimensions {
            self.display.window().set_max_inner_size(new_state.size.max_dimensions.into_option().map(Into::into).map(translate_logical_size));
            window_was_updated = true;
        }


        // position
        if self.internal.current_window_state.position != new_state.position.into() {
            if let WindowPosition::Initialized(new_position) = new_state.position {
                let new_position: PhysicalPosition<i32> = new_position.into();
                self.display.window().set_outer_position(translate_logical_position(new_position.to_logical(new_state.size.hidpi_factor)));
                window_was_updated = true;
            }
        }



        // flags:is_maximized, flags:is_minimized
        if self.internal.current_window_state.flags.is_maximized != new_state.flags.is_maximized {
            self.display.window().set_maximized(new_state.flags.is_maximized);
            window_was_updated = true;
        } else if self.internal.current_window_state.flags.is_minimized != new_state.flags.is_minimized {
            self.display.window().set_minimized(new_state.flags.is_maximized);
            window_was_updated = true;
        }

        // flags:is_fullscreen
        if self.internal.current_window_state.flags.is_fullscreen != new_state.flags.is_fullscreen {
            if new_state.flags.is_fullscreen {
                // TODO: implement exclusive fullscreen!
                self.display.window().set_fullscreen(Some(Fullscreen::Borderless(self.display.window().current_monitor())));
                window_was_updated = true;
            } else {
                self.display.window().set_fullscreen(None);
                window_was_updated = true;
            }
        }

        // flags:has_decorations
        if self.internal.current_window_state.flags.has_decorations != new_state.flags.has_decorations {
            self.display.window().set_decorations(new_state.flags.has_decorations);
        }

        // flags:is_visible
        if self.internal.current_window_state.flags.is_visible != new_state.flags.is_visible {
            self.display.window().set_visible(new_state.flags.is_visible);
        }

        // flags:is_always_on_top
        if self.internal.current_window_state.flags.is_always_on_top != new_state.flags.is_always_on_top {
            self.display.window().set_always_on_top(new_state.flags.is_always_on_top);
        }

        // flags:is_resizable
        if self.internal.current_window_state.flags.is_resizable != new_state.flags.is_resizable {
            self.display.window().set_resizable(new_state.flags.is_resizable);
            window_was_updated = true;
        }

        // flags:has_focus
        if self.internal.current_window_state.flags.has_focus != new_state.flags.has_focus {
            if new_state.flags.has_focus {
                use glutin::window::UserAttentionType;
                self.display.window().request_user_attention(Some(UserAttentionType::Informational));
            } else {
                self.display.window().request_user_attention(None);
            }
        }

        // TODO: flags:has_blur_behind_window

        if self.internal.current_window_state.ime_position != new_state.ime_position.into() {
            if let ImePosition::Initialized(new_ime_position) = new_state.ime_position {
                self.display.window().set_ime_position(translate_logical_position(new_ime_position.into()));
                self.internal.current_window_state.ime_position = new_state.ime_position;
            }
        }

        fn synchronize_mouse_state(old_mouse_state: &MouseState, new_mouse_state: &MouseState, window: &GlutinWindow) -> bool {
            use crate::wr_translate::winit_translate::translate_cursor_icon;

            let mut window_was_updated = false;

            match (old_mouse_state.mouse_cursor_type, new_mouse_state.mouse_cursor_type) {
                (OptionMouseCursorType::Some(_old_mouse_cursor), OptionMouseCursorType::None) => {
                    window.set_cursor_visible(false);
                },
                (OptionMouseCursorType::None, OptionMouseCursorType::Some(new_mouse_cursor)) => {
                    window.set_cursor_visible(true);
                    window.set_cursor_icon(translate_cursor_icon(new_mouse_cursor));
                },
                (OptionMouseCursorType::Some(old_mouse_cursor), OptionMouseCursorType::Some(new_mouse_cursor)) => {
                    if old_mouse_cursor != new_mouse_cursor {
                        window.set_cursor_icon(translate_cursor_icon(new_mouse_cursor));
                    }
                },
                (OptionMouseCursorType::None, OptionMouseCursorType::None) => { },
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
                    window_was_updated = true;
                }
            }

            window_was_updated
        }

        // TODO!
        // if synchronize_debug_state(...) { window_was_updated = true; }
        // if synchronize_keyboard_state(...) { window_was_updated = true; }
        // if synchronize_touch_state(...) { window_was_updated = true; }

        // mouse position, cursor type, etc.
        if synchronize_mouse_state(&self.internal.current_window_state.mouse_state, &new_state.mouse_state, &self.display.window()) {
            window_was_updated = true;
        }

        if synchronize_os_window_platform_extensions(&self.internal.current_window_state.platform_specific_options, &new_state.platform_specific_options, &self.display.window()) {
            window_was_updated = true;
        }

        if self.internal.current_window_state.layout_callback != new_state.layout_callback {
            window_was_updated = true;
        }

        if self.internal.current_window_state.close_callback != new_state.close_callback {
            window_was_updated = true;
        }

        let WindowState {
            theme,
            title,
            size,
            position,
            flags,
            debug_state,
            keyboard_state,
            mouse_state,
            touch_state,
            ime_position,
            platform_specific_options,
            background_color,
            layout_callback,
            close_callback,
            renderer_options: _,
            monitor: _,
        } = new_state;

        self.internal.current_window_state.theme = theme;
        self.internal.current_window_state.title = title;
        self.internal.current_window_state.size = size;
        self.internal.current_window_state.position = position;
        self.internal.current_window_state.flags = flags;
        self.internal.current_window_state.debug_state = debug_state;
        self.internal.current_window_state.keyboard_state = keyboard_state;
        self.internal.current_window_state.mouse_state = mouse_state;
        self.internal.current_window_state.touch_state = touch_state;
        self.internal.current_window_state.ime_position = ime_position;
        self.internal.current_window_state.platform_specific_options = platform_specific_options;
        self.internal.current_window_state.background_color = background_color;
        self.internal.current_window_state.layout_callback = layout_callback;
        self.internal.current_window_state.close_callback = close_callback;
        self.internal.current_window_state.monitor = current_window_monitor;

        window_was_updated
    }

    pub fn get_raw_window_handle(&self) -> RawWindowHandle {
        use raw_window_handle::HasRawWindowHandle;

        const fn translate_raw_window_handle(input: raw_window_handle::RawWindowHandle) -> RawWindowHandle {
            match input {
                #[cfg(target_os = "ios")]
                raw_window_handle::RawWindowHandle::IOS(h) => RawWindowHandle::IOS(IOSHandle {
                    ui_window: h.ui_window,
                    ui_view: h.ui_view,
                    ui_view_controller: h.ui_view_controller
                }),
                #[cfg(target_os = "macos")]
                raw_window_handle::RawWindowHandle::MacOS(h) => RawWindowHandle::MacOS(MacOSHandle {
                    ns_window: h.ns_window,
                    ns_view: h.ns_view,
                }),
                #[cfg(any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd"))]
                raw_window_handle::RawWindowHandle::Xlib(h) => RawWindowHandle::Xlib(XlibHandle {
                    window: h.window,
                    display: h.display,
                }),
                #[cfg(any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd"))]
                raw_window_handle::RawWindowHandle::Xcb(h) => RawWindowHandle::Xcb(XcbHandle {
                    window: h.window,
                    connection: h.connection,
                }),
                #[cfg(any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd"))]
                raw_window_handle::RawWindowHandle::Wayland(h) => RawWindowHandle::Wayland(WaylandHandle {
                    surface: h.surface,
                    display: h.display,
                }),
                #[cfg(target_os = "windows")]
                raw_window_handle::RawWindowHandle::Windows(h) => RawWindowHandle::Windows(WindowsHandle {
                    hwnd: h.hwnd,
                    hinstance: h.hinstance,
                }),
                #[cfg(target_arch = "wasm32")]
                raw_window_handle::RawWindowHandle::Web(h) => RawWindowHandle::Web(WebHandle {
                    id: h.id,
                }),
                #[cfg(target_os = "android")]
                raw_window_handle::RawWindowHandle::Android(h) => RawWindowHandle::Android(AndroidHandle {
                    a_native_window: h.a_native_window,
                }),
                _ => RawWindowHandle::Unsupported,
            }
        }

        translate_raw_window_handle(self.display.window().raw_window_handle())
    }

    /// Calls the callbacks and restyles / re-layouts the self.layout_results if necessary
    pub fn call_callbacks(&mut self, nodes_to_check: &NodesToCheck, events: &Events, gl_context: &GlContextPtr, app_resources: &mut AppResources, external_callbacks: &ExternalSystemCallbacks) -> CallCallbacksResult {
        use azul_core::window_state::CallbacksOfHitTest;

        let mut callbacks = CallbacksOfHitTest::new(&nodes_to_check, &events, &self.internal.layout_results);
        let raw_window_handle = self.get_raw_window_handle();
        let current_scroll_states = self.internal.get_current_scroll_states();

        // likely won't work because callbacks and &mut layout_results are borrowed
        callbacks.call(
            &self.internal.current_window_state,
            &raw_window_handle,
            &current_scroll_states,
            gl_context,
            &mut self.internal.layout_results,
            &mut self.internal.scroll_states,
            app_resources,
            external_callbacks,
        )
    }

    /// Returns what monitor the window is currently residing on (to query monitor size, etc.).
    pub(crate) fn get_current_monitor(&self) -> Option<Monitor> {
        Some(monitor_new(self.display.window().current_monitor()?, false))
    }

    fn create_window_builder(
        has_transparent_background: bool,
        theme: Option<WindowTheme>,
        platform_options: &PlatformSpecificOptions,
    ) -> GlutinWindowBuilder {


        #[cfg(target_arch = "wasm32")]
        fn create_window_builder_wasm(
            has_transparent_background: bool,
            _platform_options: &WasmWindowOptions,
        )  -> GlutinWindowBuilder {
            let mut window_builder = GlutinWindowBuilder::new()
                .with_transparent(has_transparent_background);
            window_builder
        }


        /// Create a window builder, depending on the platform options -
        /// set all options that *can only be set when the window is created*
        #[cfg(target_os = "windows")]
        fn create_window_builder_windows(
            has_transparent_background: bool,
            theme: Option<WindowTheme>,
            platform_options: &WindowsWindowOptions,
        ) -> GlutinWindowBuilder {

            use glutin::platform::windows::WindowBuilderExtWindows;
            use crate::wr_translate::winit_translate::{translate_taskbar_icon, translate_theme};

            let mut window_builder = GlutinWindowBuilder::new()
                .with_transparent(has_transparent_background)
                .with_theme(theme.map(translate_theme))
                .with_no_redirection_bitmap(platform_options.no_redirection_bitmap)
                .with_taskbar_icon(platform_options.taskbar_icon.clone().into_option().and_then(|ic| translate_taskbar_icon(ic).ok()));

            if let Some(parent_window) = platform_options.parent_window.into_option() {
                window_builder = window_builder.with_parent_window(parent_window as *mut _);
            }

            window_builder
        }



        #[cfg(target_os = "linux")]
        fn create_window_builder_linux(
            has_transparent_background: bool,
            platform_options: &LinuxWindowOptions,
        ) -> GlutinWindowBuilder {

            use glutin::platform::unix::WindowBuilderExtUnix;
            use crate::wr_translate::winit_translate::{translate_x_window_type, translate_logical_size};

            let mut window_builder = GlutinWindowBuilder::new()
                .with_transparent(has_transparent_background)
                .with_override_redirect(platform_options.x11_override_redirect);

            for AzStringPair { key, value } in platform_options.x11_wm_classes.iter() {
                window_builder = window_builder.with_class(
                    key.clone().into_library_owned_string(),
                    value.clone().into_library_owned_string()
                );
            }

            if !platform_options.x11_window_types.is_empty() {
                let window_types = platform_options.x11_window_types.iter().map(|e| translate_x_window_type(*e)).collect();
                window_builder = window_builder.with_x11_window_type(window_types);
            }

            if let OptionAzString::Some(theme_variant) = platform_options.x11_gtk_theme_variant.clone() {
                window_builder = window_builder.with_gtk_theme_variant(theme_variant.into_library_owned_string());
            }

            if let OptionLogicalSize::Some(resize_increments) = platform_options.x11_resize_increments {
                window_builder = window_builder.with_resize_increments(translate_logical_size(resize_increments));
            }

            if let OptionLogicalSize::Some(base_size) = platform_options.x11_base_size {
                window_builder = window_builder.with_base_size(translate_logical_size(base_size));
            }

            if let OptionAzString::Some(app_id) = platform_options.wayland_app_id.clone() {
                window_builder = window_builder.with_app_id(app_id.into_library_owned_string());
            }

            window_builder
        }


        #[cfg(target_os = "macos")]
        fn create_window_builder_macos(
            has_transparent_background: bool,
            platform_options: &MacWindowOptions,
        ) -> GlutinWindowBuilder {
            let mut window_builder = GlutinWindowBuilder::new()
                .with_transparent(has_transparent_background);

            window_builder
        }

        #[cfg(target_os = "linux")] { create_window_builder_linux(has_transparent_background, &platform_options.linux_options) }
        #[cfg(target_os = "windows")] { create_window_builder_windows(has_transparent_background, theme, &platform_options.windows_options) }
        #[cfg(target_os = "macos")] { create_window_builder_macos(has_transparent_background, &platform_options.mac_options) }
        #[cfg(target_arch = "wasm32")] { create_window_builder_wasm(has_transparent_background, &platform_options.wasm_options) }
    }

    // Function wrapper that is invoked on scrolling and normal rendering - only renders the
    // window contents and updates the screen, assumes that all transactions via the WrRenderApi
    // have been committed before this function is called.
    //
    // WebRender doesn't reset the active shader back to what it was, but rather sets it
    // to zero, which glutin doesn't know about, so on the next frame it tries to draw with shader 0.
    // This leads to problems when invoking GlCallbacks, because those don't expect
    // the OpenGL state to change between calls. Also see: https://github.com/servo/webrender/pull/2880
    //
    // NOTE: For some reason, webrender allows rendering to a framebuffer with a
    // negative width / height, although that doesn't make sense
    pub(crate) fn render_async(&mut self, mut txn: WrTransaction, display_list_was_rebuilt: bool) {

        /// Scroll all nodes in the ScrollStates to their correct position and insert
        /// the positions into the transaction
        ///
        /// NOTE: scroll_states has to be mutable, since every key has a "visited" field, to
        /// indicate whether it was used during the current frame or not.
        fn scroll_all_nodes(scroll_states: &mut ScrollStates, txn: &mut WrTransaction) {
            use webrender::api::ScrollClamping;
            use crate::wr_translate::{wr_translate_external_scroll_id, wr_translate_logical_position};
            for (key, value) in scroll_states.0.iter_mut() {
                txn.scroll_node_with_id(
                    wr_translate_logical_position(value.get()),
                    wr_translate_external_scroll_id(*key),
                    ScrollClamping::ToContentBounds
                );
            }
        }


        use azul_css::ColorF;
        use crate::wr_translate;

        let physical_size = self.internal.current_window_state.size.get_physical_size();
        let framebuffer_size = WrDeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);

        // Especially during minimization / maximization of a window, it can happen that the window
        // width or height is zero. In that case, no rendering is necessary (doing so would crash
        // the application, since glTexImage2D may never have a 0 as the width or height.
        if framebuffer_size.width == 0 || framebuffer_size.height == 0 {
            return;
        }

        self.internal.epoch.increment();

        txn.set_root_pipeline(wr_translate::wr_translate_pipeline_id(self.internal.pipeline_id));
        txn.set_document_view(WrDeviceIntRect::new(WrDeviceIntPoint::new(0, 0), framebuffer_size), self.internal.current_window_state.size.hidpi_factor);
        scroll_all_nodes(&mut self.internal.scroll_states, &mut txn);

        if !display_list_was_rebuilt {
            txn.skip_scene_builder(); // avoid rebuilding the scene if DL hasn't changed
        }

        txn.generate_frame(0);


        // Update WR texture cache
        self.render_api.send_transaction(wr_translate::wr_translate_document_id(self.internal.document_id), txn);
    }

    /// Does the actual rendering + swapping
    pub fn render_block_and_swap(&mut self) {

        fn clean_up_unused_opengl_textures(pipeline_info: WrPipelineInfo, pipeline_id: &PipelineId) {

            use azul_core::gl::gl_textures_remove_epochs_from_pipeline;
            use crate::wr_translate::translate_epoch_wr;

            // TODO: currently active epochs can be empty, why?
            //
            // I mean, while the renderer is rendering, there can never be "no epochs" active,
            // at least one epoch must always be active.
            if pipeline_info.epochs.is_empty() {
                return;
            }

            // TODO: pipeline_info.epochs does not contain all active epochs,
            // at best it contains the lowest in-use epoch. I.e. if `Epoch(43)`
            // is listed, you can remove all textures from Epochs **lower than 43**
            // BUT NOT EPOCHS HIGHER THAN 43.
            //
            // This means that "all active epochs" (in the documentation) is misleading
            // since it doesn't actually list all active epochs, otherwise it'd list Epoch(43),
            // Epoch(44), Epoch(45), which are currently active.
            let oldest_to_remove_epoch = pipeline_info.epochs.values().min().unwrap();

            gl_textures_remove_epochs_from_pipeline(pipeline_id, translate_epoch_wr(*oldest_to_remove_epoch));
        }

        let physical_size = self.internal.current_window_state.size.get_physical_size();
        let framebuffer_size = WrDeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);

        // NOTE: The `hidden_display` must share the OpenGL context with the `window`,
        // otherwise this will segfault! Use `ContextBuilder::with_shared_lists` to share the
        // OpenGL context across different windows.
        //
        // The context **must** be made current before calling `.bind_framebuffer()`,
        // otherwise EGL will panic with EGL_BAD_MATCH. The current context has to be the
        // hidden_display context, otherwise this will segfault on Windows.
        self.display.make_current();

        // let gl = self.get_gl_context();
        // let mut current_program = [0_i32];
        // unsafe { gl.get_integer_v(gl::CURRENT_PROGRAM, (&mut current_program[..]).into()); }

        if let Some(r) = self.renderer.as_mut() {
            r.update();
            let _ = r.render(framebuffer_size, 0);
            clean_up_unused_opengl_textures(r.flush_pipeline_info(), &self.internal.pipeline_id);
            // self.display.window().request_redraw();
        }

        self.display.windowed_context().unwrap().swap_buffers().unwrap();

        // self.upload_software_to_native(); // does nothing if hardware acceleration is on
        // gl.bind_framebuffer(gl::FRAMEBUFFER, 0);
        // gl.bind_texture(gl::TEXTURE_2D, 0);
        // gl.use_program(current_program[0] as u32);
        // self.display.make_not_current();
    }
}

impl Drop for Window {
    fn drop(&mut self) {

        self.display.make_current();

        // Important: destroy all OpenGL textures before the shared
        // OpenGL context is destroyed.
        azul_core::gl::gl_textures_remove_active_pipeline(&self.internal.pipeline_id);

        self.get_gl_context().disable(gl::FRAMEBUFFER_SRGB);
        self.get_gl_context().disable(gl::MULTISAMPLE);
        self.get_gl_context().disable(gl::POLYGON_SMOOTH);

        if let Some(renderer) = self.renderer.take() {
            renderer.deinit();
        }

        if let Some(sw) = self.software_gl.as_mut() {
            sw.destroy();
        }
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

fn synchronize_os_window_platform_extensions(
    old_state: &PlatformSpecificOptions,
    new_state: &PlatformSpecificOptions,
    window: &GlutinWindow,
) -> bool {
    let mut window_was_updated = false;
    // platform-specific extensions
    #[cfg(target_os = "windows")] {
        if synchronize_os_window_windows_extensions(&old_state.windows_options, &new_state.windows_options, window) { window_was_updated = true; }
    }
    #[cfg(target_os = "linux")] {
        if synchronize_os_window_linux_extensions( &old_state.linux_options, &new_state.linux_options, window) { window_was_updated = true; }
    }
    #[cfg(target_os = "macos")] {
        if synchronize_os_window_mac_extensions(&old_state.mac_options, &new_state.mac_options, window) { window_was_updated = true; }
    }
    window_was_updated
}

/// Do the inital synchronization of the window with the OS-level window
fn initialize_os_window(
    new_state: &WindowState,
    window: &GlutinWindow,
) {
    use crate::wr_translate::winit_translate::{translate_logical_size, translate_logical_position};
    use glutin::window::Fullscreen;

    window.set_title(new_state.title.as_str());
    window.set_maximized(new_state.flags.is_maximized);

    if new_state.flags.is_fullscreen {
        window.set_fullscreen(Some(Fullscreen::Borderless(window.current_monitor())));
    } else {
        window.set_fullscreen(None);
    }

    window.set_decorations(new_state.flags.has_decorations);
    window.set_inner_size(translate_logical_size(new_state.size.dimensions));
    window.set_min_inner_size(new_state.size.min_dimensions.into_option().map(translate_logical_size));
    window.set_min_inner_size(new_state.size.max_dimensions.into_option().map(translate_logical_size));

    if let WindowPosition::Initialized(new_position) = new_state.position {
        let new_position: PhysicalPosition<i32> = new_position.into();
        window.set_outer_position(translate_logical_position(new_position.to_logical(new_state.size.hidpi_factor)));
    }

    if let ImePosition::Initialized(new_ime_position) = new_state.ime_position {
        window.set_ime_position(translate_logical_position(new_ime_position));
    }

    window.set_always_on_top(new_state.flags.is_always_on_top);
    window.set_resizable(new_state.flags.is_resizable);

    // mouse position, cursor type, etc.
    initialize_mouse_state(&new_state.mouse_state, window);

    // platform-specific extensions
    initialize_os_window_platform_extensions(&new_state.platform_specific_options, &window);
}

fn initialize_os_window_platform_extensions(
    platform_options: &PlatformSpecificOptions,
    window: &GlutinWindow,
) {
    #[cfg(target_os = "windows")] { initialize_os_window_windows_extensions(&platform_options.windows_options, window); }
    #[cfg(target_os = "linux")] { initialize_os_window_linux_extensions(&platform_options.linux_options, window); }
    #[cfg(target_os = "macos")] { initialize_os_window_mac_extensions(&platform_options.mac_options, window); }
    #[cfg(target_arch = "wasm32")] { initialize_os_window_wasm_extensions(&platform_options.wasm_options, window); }
}


fn initialize_mouse_state(
    new_mouse_state: &MouseState,
    window: &GlutinWindow,
) {
    use crate::wr_translate::winit_translate::{translate_cursor_icon, translate_logical_position};

    match new_mouse_state.mouse_cursor_type {
        OptionMouseCursorType::None => { window.set_cursor_visible(false); },
        OptionMouseCursorType::Some(new_mouse_cursor) => {
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
) -> bool {
    use glutin::platform::windows::WindowExtWindows;
    use crate::wr_translate::winit_translate::{translate_window_icon, translate_taskbar_icon};

    let window_was_updated = false;

    if old_state.window_icon != new_state.window_icon {
        window.set_window_icon(new_state.window_icon.clone().into_option().and_then(|ic| translate_window_icon(ic).ok()));
    }

    if old_state.taskbar_icon != new_state.taskbar_icon {
        window.set_taskbar_icon(new_state.taskbar_icon.clone().into_option().and_then(|ic| translate_taskbar_icon(ic).ok()));
    }

    window_was_updated
}

// Linux-specific window options
#[cfg(target_os = "linux")]
fn synchronize_os_window_linux_extensions(
    old_state: &LinuxWindowOptions,
    new_state: &LinuxWindowOptions,
    window: &GlutinWindow,
) -> bool {
    use glutin::platform::unix::WindowExtUnix;
    use glutin::window::UserAttentionType as WinitUserAttentionType;
    use crate::wr_translate::winit_translate::{translate_window_icon, WaylandThemeWrapper};

    let window_was_updated = false;

    if old_state.request_user_attention != new_state.request_user_attention {
        window.request_user_attention(match new_state.request_user_attention {
            UserAttentionType::None => None,
            UserAttentionType::Critical => Some(WinitUserAttentionType::Critical),
            UserAttentionType::Informational => Some(WinitUserAttentionType::Informational),
        });
    }

    if old_state.wayland_theme != new_state.wayland_theme {
        if let Some(new_wayland_theme) = new_state.wayland_theme.as_ref() {
            window.set_wayland_theme(WaylandThemeWrapper(new_wayland_theme.clone()));
        }
    }

    if old_state.window_icon != new_state.window_icon {
        window.set_window_icon(new_state.window_icon.clone().into_option().and_then(|ic| translate_window_icon(ic).ok()));
    }

    window_was_updated
}

// Mac-specific window options
#[cfg(target_os = "macos")]
fn synchronize_os_window_mac_extensions(
    old_state: &MacWindowOptions,
    new_state: &MacWindowOptions,
    window: &GlutinWindow,
) -> bool {
    use glutin::platform::macos::WindowExtMacOS;

    let window_was_updated = false;

    window_was_updated
}

#[cfg(target_arch = "wasm32")]
fn initialize_os_window_windows_extensions(
    new_state: &WasmWindowOptions,
    window: &GlutinWindow,
) {
    // intentionally empty
}

// Windows-specific window options
#[cfg(target_os = "windows")]
fn initialize_os_window_windows_extensions(
    new_state: &WindowsWindowOptions,
    window: &GlutinWindow,
) {
    use glutin::platform::windows::WindowExtWindows;
    use crate::wr_translate::winit_translate::{translate_taskbar_icon, translate_window_icon};

    window.set_window_icon(new_state.window_icon.clone().into_option().and_then(|ic| translate_window_icon(ic).ok()));
    window.set_taskbar_icon(new_state.taskbar_icon.clone().into_option().and_then(|ic| translate_taskbar_icon(ic).ok()));
}

// Linux-specific window options
#[cfg(target_os = "linux")]
fn initialize_os_window_linux_extensions(
    new_state: &LinuxWindowOptions,
    window: &GlutinWindow,
) {
    use glutin::platform::unix::WindowExtUnix;
    use glutin::window::UserAttentionType as WinitUserAttentionType;
    use crate::wr_translate::winit_translate::{translate_window_icon, WaylandThemeWrapper};

    window.request_user_attention(match new_state.request_user_attention {
        UserAttentionType::None => None,
        UserAttentionType::Critical => Some(WinitUserAttentionType::Critical),
        UserAttentionType::Informational => Some(WinitUserAttentionType::Informational),
    });

    if let Some(new_wayland_theme) = new_state.wayland_theme.as_ref() {
        window.set_wayland_theme(WaylandThemeWrapper(new_wayland_theme.clone()));
    }

    window.set_window_icon(
        new_state.window_icon.clone()
        .into_option()
        .and_then(|ic| translate_window_icon(ic).ok())
    );
}

// Mac-specific window options
#[cfg(target_os = "macos")]
fn initialize_os_window_mac_extensions(
    new_state: &MacWindowOptions,
    window: &GlutinWindow,
) {
    use glutin::platform::macos::WindowExtMacOS;
    use glutin::platform::macos::RequestUserAttentionType;

    if new_state.request_user_attention {
        window.request_user_attention(RequestUserAttentionType::Informational);
    }
}

/// Returns the actual hidpi factor and the winit DPI factor for the current window
#[allow(unused_variables)]
pub(crate) fn get_hidpi_factor(window: &GlutinWindow, event_loop: &EventLoopWindowTarget<UserEvent>) -> (f32, f32) {

    let system_hidpi_factor = window.scale_factor() as f32;

    #[cfg(target_os = "linux")] {
        use crate::glutin::platform::unix::EventLoopWindowTargetExtUnix;

        let is_x11 = event_loop.is_x11();
        (linux_get_hidpi_factor(is_x11).unwrap_or(system_hidpi_factor), system_hidpi_factor)
    }

    #[cfg(not(target_os = "linux"))] {
        (system_hidpi_factor, system_hidpi_factor)
    }
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

    let system_hidpi_factor = env::var("system_hidpi_factor").ok().and_then(|hidpi_factor| hidpi_factor.parse::<f32>().ok());
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

    let options = [system_hidpi_factor, qt_font_dpi, gsettings_dpi_factor, xft_dpi];
    options.iter().filter_map(|x| *x).next()
}
