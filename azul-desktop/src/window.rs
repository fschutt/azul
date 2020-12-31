use std::{
    fmt,
    collections::BTreeMap,
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
use gleam::gl;
use clipboard2::{Clipboard as _, ClipboardError, SystemClipboard};
use azul_css::ColorU;
use crate::{
    resources::WrApi,
    compositor::Compositor
};
use azul_core::{
    callbacks::PipelineId,
    display_list::{CachedDisplayList, SolvedLayout, GlTextureCache},
    app_resources::{Epoch, AppResources},
    gl::GlContextPtr,
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
pub struct Window {
    /// Stores things like scroll states, display list + epoch for the window
    pub(crate) internal: WindowInternal,
    /// The display, i.e. the actual window (+ the attached OpenGL context)
    pub(crate) display: ContextState,
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

impl Window {

    const CALLBACKS: RenderCallbacks = RenderCallbacks {
        insert_into_active_gl_textures: azul_core::gl::insert_into_active_gl_textures,
        layout_fn: azul_layout::do_the_layout,
        load_font_fn: azul_core::app_resources::load_font,
        load_image_fn: azul_core::app_resources::load_image,
        parse_font_fn: azul_text_layout::parse_font,
    };

    /// Creates a new window
    pub(crate) fn new(
        mut options: WindowCreateOptions,
        shared_context: &Context<NotCurrent>,
        events_loop: &EventLoopWindowTarget<()>,
        app_resources: &mut AppResources,
        render_api: &mut WrApi,
    ) -> Result<Self, GlutinCreationError> {

        use crate::wr_translate::{
            translate_logical_size_to_css_layout_size,
            translate_document_id_wr
        };
        use azul_css::LayoutPoint;

        // NOTE: It would be OK to use &RenderApi here, but it's better
        // to make sure that the RenderApi is currently not in use by anything else.

        // NOTE: All windows MUST have a shared EventsLoop, creating a new EventLoop for the
        // new window causes a segfault.

        let is_transparent_background = options.background_color.a != 0;

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

        let document_id = translate_document_id_wr(render_api.api.add_document(framebuffer_size, 0));

        // TODO: The PipelineId is what gets passed to the OutputImageHandler
        // (the code that coordinates displaying the rendered texture).
        //
        // Each window is a "pipeline", i.e a new web page in webrender terms,
        // however, there is only one global renderer, in order to save on memory,
        // The pipeline ID is important, in order to coordinate the rendered textures
        // back to their windows and window positions.
        let pipeline_id = PipelineId::new();

        app_resources.add_pipeline(pipeline_id);

        let context_state = ContextState::NotCurrent(gl_window);
        let init = WindowInternalInit { window_create_options, document_id, pipeline_id };
        let internal = WindowInternal::new(init, app_resources, render_api, Window::CALLBACKS);
        let mut window = Window { display: context_state, internal };
        window.rebuild_display_list();
        Ok(window)
    }

    /// Calls the layout function again and updates the self.internal.gl_texture_cache field
    pub fn relayout_everything(&mut self, gl_context: &GlContextPtr, app_resources: &mut AppResources, render_api: &mut WrApi) {
        self.internal.relayout(gl_context, app_resources, render_api, Window::CALLBACKS);
        self.rebuild_display_list();
    }

    /// Only re-build the display list and send it to webrender
    #[cfg(not(test))]
    pub fn rebuild_display_list(&mut self, app_resources: &AppResources, render_api: &mut WrApi) {

        use crate::wr_translate::{
            wr_translate_pipeline_id,
            wr_translate_document_id,
            wr_translate_display_list,
            wr_translate_epoch,
        };

        // NOTE: Display list has to be rebuilt every frame, otherwise, the epochs get out of sync
        let cached_display_list = CachedDisplayList::new(self.internal.epoch, self.internal.pipeline_id, &self.internal.current_window_state, &self.internal.layout_results, &self.internal.gl_texture_cache, app_resources);
        let display_list = wr_translate_display_list(cached_display_list, self.internal.pipeline_id);

        let logical_size = WrLayoutSize::new(elf.internal.current_window_state.size.dimensions.width, elf.internal.current_window_state.size.dimensions.height);

        let mut txn = WrTransaction::new();
        txn.set_display_list(
            wr_translate_epoch(self.internal.epoch),
            None,
            logical_size.clone(),
            (wr_translate_pipeline_id(self.internal.pipeline_id), logical_size, display_list),
            true,
        );

        render_api.api.send_transaction(wr_translate_document_id(window.internal.document_id), txn);
    }

    // Function wrapper that is invoked on scrolling and normal rendering - only renders the
    // window contents and updates the screen, assumes that all transactions via the WrApi
    // have been committed before this function is called.
    //
    // WebRender doesn't reset the active shader back to what it was, but rather sets it
    // to zero, which glutin doesn't know about, so on the next frame it tries to draw with shader 0.
    // This leads to problems when invoking GlCallbacks, because those don't expect
    // the OpenGL state to change between calls. Also see: https://github.com/servo/webrender/pull/2880
    //
    // NOTE: For some reason, webrender allows rendering to a framebuffer with a
    // negative width / height, although that doesn't make sense
    #[cfg(not(test))]
    pub fn render_display_list_to_texture(&mut self, headless_shared_context: &mut HeadlessContextState, render_api: &mut WrApi, renderer: &mut WrRenderer, gl_context: &GlContextPtr) -> Option<Texture> {

        /// Scroll all nodes in the ScrollStates to their correct position and insert
        /// the positions into the transaction
        ///
        /// NOTE: scroll_states has to be mutable, since every key has a "visited" field, to
        /// indicate whether it was used during the current frame or not.
        fn scroll_all_nodes(scroll_states: &mut ScrollStates, txn: &mut WrTransaction) {
            use webrender::api::ScrollClamping;
            use crate::wr_translate::{wr_translate_external_scroll_id, wr_translate_layout_point};
            for (key, value) in scroll_states.0.iter_mut() {
                txn.scroll_node_with_id(
                    wr_translate_layout_point(value.get()),
                    wr_translate_external_scroll_id(*key),
                    ScrollClamping::ToContentBounds
                );
            }
        }

        use azul_css::ColorF;
        use crate::wr_translate;

        let mut txn = WrTransaction::new();

        let physical_size = internal.current_window_state.size.get_physical_size();
        let framebuffer_size = WrDeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);
        let background_color_f: ColorF = internal.current_window_state.background_color.into();
        let is_opaque = internal.current_window_state.background_color.a == 0;

        // Especially during minimization / maximization of a window, it can happen that the window
        // width or height is zero. In that case, no rendering is necessary (doing so would crash
        // the application, since glTexImage2D may never have a 0 as the width or height.
        if framebuffer_size.width == 0 || framebuffer_size.height == 0 {
            return None;
        }

        internal.epoch.increment();

        txn.set_root_pipeline(wr_translate::wr_translate_pipeline_id(window.internal.pipeline_id));
        scroll_all_nodes(&mut internal.scroll_states, &mut txn);
        txn.generate_frame();

        render_api.api.send_transaction(wr_translate::wr_translate_document_id(window.internal.document_id), txn);

        // Update WR texture cache
        renderer.update();

        // NOTE: The `hidden_display` must share the OpenGL context with the `window`,
        // otherwise this will segfault! Use `ContextBuilder::with_shared_lists` to share the
        // OpenGL context across different windows.
        //
        // The context **must** be made current before calling `.bind_framebuffer()`,
        // otherwise EGL will panic with EGL_BAD_MATCH. The current context has to be the
        // hidden_display context, otherwise this will segfault on Windows.
        headless_shared_context.make_current();

        let mut current_program = [0_i32];
        gl_context.get_integer_v(gl::CURRENT_PROGRAM, (&mut current_program[..]).into());
        let mut current_textures = [0_i32];
        gl_context.get_integer_v(gl::CURRENT_TEXTURE, (&mut current_textures[..]).into());
        let mut current_framebuffers = [0_i32];
        gl_context.get_integer_v(gl::CURRENT_FRAMEBUFFER, (&mut current_framebuffers[..]).into());

        // Generate a framebuffer (that will contain the final, rendered screen output).
        let framebuffers = gl_context.gen_framebuffers(1);
        gl_context.bind_framebuffer(gl::FRAMEBUFFER, framebuffers.get(0).copied().unwrap());

        // Create the texture to render to
        let textures = gl_context.gen_textures(1);

        gl_context.bind_texture(gl::TEXTURE_2D, textures.get(0).copied().unwrap());
        gl_context.tex_image_2d(gl::TEXTURE_2D, 0, gl::RGB as i32, framebuffer_size.width, framebuffer_size.height, 0, gl::RGB, gl::UNSIGNED_BYTE, None.into());

        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);

        let depthbuffers = gl_context.gen_renderbuffers(1);
        gl_context.bind_renderbuffer(gl::RENDERBUFFER, depthbuffers.get(0).copied().unwrap());
        gl_context.renderbuffer_storage(gl::RENDERBUFFER, gl::DEPTH_COMPONENT, framebuffer_size.width, framebuffer_size.height);
        gl_context.framebuffer_renderbuffer(gl::FRAMEBUFFER, gl::DEPTH_ATTACHMENT, gl::RENDERBUFFER, depthbuffers.get(0).copied().unwrap());

        // Set "textures[0]" as the color attachement #0
        gl_context.framebuffer_texture_2d(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, textures.get(0).copied().unwrap(), 0);

        gl_context.draw_buffers([gl::COLOR_ATTACHMENT0][..].into());

        // Check that the framebuffer is complete
        debug_assert!(gl_context.check_frame_buffer_status(gl::FRAMEBUFFER) == gl::FRAMEBUFFER_COMPLETE);

        // Disable SRGB and multisample, otherwise, WebRender will crash
        gl_context.disable(gl::FRAMEBUFFER_SRGB);
        gl_context.disable(gl::MULTISAMPLE);
        gl_context.disable(gl::POLYGON_SMOOTH);

        // Invoke WebRender to render the frame - renders to the currently bound FB
        gl_context.clear_color(background_color_f.r, background_color_f.g, background_color_f.b, background_color_f.a);
        gl_context.clear(gl::COLOR_BUFFER_BIT);
        gl_context.clear_depth(0.0);
        gl_context.clear(gl::DEPTH_BUFFER_BIT);
        renderer.render(framebuffer_size).unwrap();

        // FBOs can't be shared between windows, but textures can.
        // In order to draw on the windows backbuffer, first make the window current, then draw to FB 0

        let texture = Texture {
            texture_id: textures[0],
            flags: TextureFlags {
                is_opaque,
                is_video_texture: false,
            },
            size: PhysicalSizeU32 { width: framebuffer_size.width, height: framebuffer_size.height },
            gl_context: gl_context.clone(),
        };

        // Do not delete the texture here...
        gl_context.delete_framebuffers(framebuffers.as_ref().into());
        gl_context.delete_renderbuffers(depthbuffers.as_ref().into());

        // reset the state to what it was before
        gl_context.bind_framebuffer(gl::FRAMEBUFFER, current_framebuffer[0]);
        gl_context.bind_texture(gl::TEXTURE_2D, current_texture[0]);
        gl_context.use_program(current_program[0] as u32);

        headless_shared_context.make_not_current();

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

        // After rendering + swapping, remove the unused OpenGL textures
        clean_up_unused_opengl_textures(renderer.flush_pipeline_info(), &window.internal.pipeline_id);

        Some(texture)
    }

    /// Synchronize the `self.internal.previous_window_state` with the `self.internal.current_window_state`
    ///  updating the OS-level window to reflect the new state
    pub fn synchronize_window_state_with_os(&mut self, new_state: &WindowState) -> bool {

        use crate::wr_translate::winit_translate::{translate_logical_position, translate_logical_size};
        use glutin::window::Fullscreen;

        let mut window_was_updated = false;

        if self.current_window_state.title.as_str() != new_state.title.as_str() {
            window.set_title(new_state.title.as_str());
        }

        if self.current_window_state.flags.is_maximized != new_state.flags.is_maximized {
            window.set_maximized(new_state.flags.is_maximized);
            window_was_updated = true;
        }

        if self.current_window_state.flags.is_fullscreen != new_state.flags.is_fullscreen {
            if new_state.flags.is_fullscreen {
                // TODO: implement exclusive fullscreen!
                window.set_fullscreen(Some(Fullscreen::Borderless(window.current_monitor())));
                window_was_updated = true;
            } else {
                window.set_fullscreen(None);
                window_was_updated = true;
            }
        }

        if self.current_window_state.flags.has_decorations != new_state.flags.has_decorations {
            window.set_decorations(new_state.flags.has_decorations);
            window_was_updated = true;
        }

        if self.current_window_state.flags.is_visible != new_state.flags.is_visible {
            window.set_visible(new_state.flags.is_visible);
            window_was_updated = true;
        }

        if self.current_window_state.size.dimensions != new_state.size.dimensions {
            window.set_inner_size(translate_logical_size(new_state.size.dimensions));
            window_was_updated = true;
        }

        if self.current_window_state.size.min_dimensions != new_state.size.min_dimensions {
            window.set_min_inner_size(new_state.size.min_dimensions.into_option().map(Into::into).map(translate_logical_size));
            window_was_updated = true;
        }

        if self.current_window_state.size.max_dimensions != new_state.size.max_dimensions {
            window.set_max_inner_size(new_state.size.max_dimensions.into_option().map(Into::into).map(translate_logical_size));
            window_was_updated = true;
        }

        if self.current_window_state.position != new_state.position.into() {
            if let OptionPhysicalPositionI32::Some(new_position) = new_state.position {
                let new_position: PhysicalPosition<i32> = new_position.into();
                window.set_outer_position(translate_logical_position(new_position.to_logical(new_state.size.hidpi_factor)));
                window_was_updated = true;
            }
        }

        if self.current_window_state.ime_position != new_state.ime_position.into() {
            if let OptionLogicalPosition::Some(new_ime_position) = new_state.ime_position {
                window.set_ime_position(translate_logical_position(new_ime_position.into()));
                window_was_updated = true;
            }
        }

        if self.current_window_state.flags.is_always_on_top != new_state.flags.is_always_on_top {
            window.set_always_on_top(new_state.flags.is_always_on_top);
            window_was_updated = true;
        }

        if self.current_window_state.flags.is_resizable != new_state.flags.is_resizable {
            window.set_resizable(new_state.flags.is_resizable);
            window_was_updated = true;
        }

        // mouse position, cursor type, etc.
        if synchronize_mouse_state(&self.current_window_state.mouse_state, &new_state.mouse_state, &window) {
            window_was_updated = true;
        }

        if synchronize_os_window_platform_extensions(&self.current_window_state.platform_specific_options, &new_state.platform_specific_options, &window) {
            window_was_updated = true;
        }

        window_was_updated
    }

    /// Calls the callbacks and restyles / re-layouts the self.layout_results if necessary
    pub fn call_callbacks(&mut self, nodes_to_check: &NodesToCheck, events: &Events, gl_context: &GlContextPtr, app_resources: &mut AppResources) -> CallCallbacksResult {
        let callbacks = CallbacksOfHitTest::new(&nodes_to_check, &events, &self.internal.layout_results);
        let current_scroll_states = self.internal.scroll_states.get_copy();
        // likely won't work because callbacks and &mut layout_results are borrowed
        callbacks.call(
            self.internal.pipeline_id,
            &self.internal.current_window_state,
            &current_scroll_states,
            gl_context,
            &mut self.internal.layout_results,
            &mut self.internal.scroll_states,
            app_resources,
            azul_layout::do_the_relayout,
        )
    }

    /// Returns what monitor the window is currently residing on (to query monitor size, etc.).
    pub fn get_current_monitor(&self) -> Option<MonitorHandle> {
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

fn create_window_builder(
    has_transparent_background: bool,
    platform_options: &PlatformSpecificOptions,
) -> GlutinWindowBuilder {
    #[cfg(target_os = "linux")] { create_window_builder_linux(has_transparent_background, &platform_options.linux_options) }
    #[cfg(target_os = "windows")] { create_window_builder_windows(has_transparent_background, &platform_options.windows_options) }
    #[cfg(target_os = "macos")] { create_window_builder_macos(has_transparent_background, &platform_options.mac_options) }
    #[cfg(target_arch = "wasm32")] { create_window_builder_wasm(has_transparent_background, &platform_options.wasm_options) }
}

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
    platform_options: &WindowsWindowOptions,
) -> GlutinWindowBuilder {

    use glutin::platform::windows::WindowBuilderExtWindows;
    use crate::wr_translate::winit_translate::translate_taskbar_icon;

    let mut window_builder = GlutinWindowBuilder::new()
        .with_transparent(has_transparent_background)
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
        window_builder = window_builder.with_class(key.clone().into(), value.clone().into());
    }

    if !platform_options.x11_window_types.is_empty() {
        let window_types = platform_options.x11_window_types.iter().map(|e| translate_x_window_type(*e)).collect();
        window_builder = window_builder.with_x11_window_type(window_types);
    }

    if let OptionAzString::Some(theme_variant) = platform_options.x11_gtk_theme_variant.clone() {
        window_builder = window_builder.with_gtk_theme_variant(theme_variant.into());
    }

    if let OptionLogicalSize::Some(resize_increments) = platform_options.x11_resize_increments {
        window_builder = window_builder.with_resize_increments(translate_logical_size(resize_increments));
    }

    if let OptionLogicalSize::Some(base_size) = platform_options.x11_base_size {
        window_builder = window_builder.with_base_size(translate_logical_size(base_size));
    }

    if let OptionAzString::Some(app_id) = platform_options.wayland_app_id.clone() {
        window_builder = window_builder.with_app_id(app_id.into());
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

fn synchronize_os_window_platform_extensions(
    old_state: &PlatformSpecificOptions,
    new_state: &PlatformSpecificOptions,
    window: &GlutinWindow,
) {
    // platform-specific extensions
    #[cfg(target_os = "windows")] {
        synchronize_os_window_windows_extensions(&old_state.windows_options, &new_state.windows_options, window);
    }
    #[cfg(target_os = "linux")] {
        synchronize_os_window_linux_extensions( &old_state.linux_options, &new_state.linux_options, window);
    }
    #[cfg(target_os = "macos")] {
        synchronize_os_window_mac_extensions(&old_state.mac_options, &new_state.mac_options, window);
    }
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
    window.set_visible(new_state.flags.is_visible);
    window.set_inner_size(translate_logical_size(new_state.size.dimensions));
    window.set_min_inner_size(new_state.size.min_dimensions.into_option().map(translate_logical_size));
    window.set_min_inner_size(new_state.size.max_dimensions.into_option().map(translate_logical_size));

    if let OptionPhysicalPositionI32::Some(new_position) = new_state.position {
        let new_position: PhysicalPosition<i32> = new_position.into();
        window.set_outer_position(translate_logical_position(new_position.to_logical(new_state.size.hidpi_factor)));
    }

    if let OptionLogicalPosition::Some(new_ime_position) = new_state.ime_position {
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

fn synchronize_mouse_state(
    old_mouse_state: &MouseState,
    new_mouse_state: &MouseState,
    window: &GlutinWindow,
) {
    use crate::wr_translate::winit_translate::{translate_cursor_icon, translate_logical_position};

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
        }
    }
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
) {
    use glutin::platform::windows::WindowExtWindows;
    use crate::wr_translate::winit_translate::{translate_window_icon, translate_taskbar_icon};

    if old_state.window_icon != new_state.window_icon {
        window.set_window_icon(new_state.window_icon.clone().into_option().and_then(|ic| translate_window_icon(ic).ok()));
    }

    if old_state.taskbar_icon != new_state.taskbar_icon {
        window.set_taskbar_icon(new_state.taskbar_icon.clone().into_option().and_then(|ic| translate_taskbar_icon(ic).ok()));
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
        if let OptionWaylandTheme::Some(new_wayland_theme) = new_state.wayland_theme {
            window.set_wayland_theme(translate_wayland_theme(new_wayland_theme));
        }
    }

    if old_state.window_icon != new_state.window_icon {
        window.set_window_icon(new_state.window_icon.clone().into_option().and_then(|ic| translate_window_icon(ic).ok()));
    }
}

// Mac-specific window options
#[cfg(target_os = "macos")]
fn synchronize_os_window_mac_extensions(
    old_state: &MacWindowOptions,
    new_state: &MacWindowOptions,
    window: &GlutinWindow,
) {
    use glutin::platform::macos::WindowExtMacOS;
    use glutin::platform::macos::RequestUserAttentionType;

    if old_state.request_user_attention != new_state.request_user_attention && new_state.request_user_attention {
        window.request_user_attention(RequestUserAttentionType::Informational);
    }
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
    use crate::wr_translate::winit_translate::{translate_window_icon, translate_wayland_theme};

    window.set_urgent(new_state.request_user_attention);

    if let OptionWaylandTheme::Some(new_wayland_theme) = new_state.wayland_theme {
        window.set_wayland_theme(translate_wayland_theme(new_wayland_theme));
    }

    window.set_window_icon(new_state.window_icon.clone().into_option().and_then(|ic| translate_window_icon(ic).ok()));
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
    pub(crate) gl_context: GlContextPtr,
}

impl fmt::Debug for FakeDisplay {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "FakeDisplay {{ .. }}")
    }
}

impl FakeDisplay {

    /// Creates a new render + a new display, given a renderer type (software or hardware)
    pub(crate) fn new(renderer_type: RendererType) -> Result<Self, RendererCreationError> {

        const DPI_FACTOR: f32 = 1.0;

        let initial_size = WrDeviceIntSize::new(600, 800); // fake size for the renderer

        // The events loop is shared across all windows
        let event_loop = EventLoop::new();
        let (renderer, render_api, headless_context, gl_context) = create_renderer(&event_loop, renderer_type, DPI_FACTOR, initial_size)?;

        Ok(Self {
            render_api: WrApi { api: render_api },
            renderer: Some(renderer),
            hidden_context: headless_context,
            hidden_event_loop: event_loop,
            gl_context,
        })
    }

    pub fn get_gl_context(&self) -> GlContextPtr {
        self.gl_context.clone()
    }
}

fn get_gl_context(gl_window: &Context<PossiblyCurrent>) -> Result<GlContextPtr, GlutinCreationError> {
    use glutin::Api;
    match gl_window.get_api() {
        Api::OpenGl => Ok(GlContextPtr::new(unsafe { gl::GlFns::load_with(|symbol| gl_window.get_proc_address(symbol) as *const _) })),
        Api::OpenGlEs => Ok(GlContextPtr::new(unsafe { gl::GlesFns::load_with(|symbol| gl_window.get_proc_address(symbol) as *const _ ) })),
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
    create_window_context_builder(Vsync::Enabled, Srgb::Enabled, HwAcceleration::Enabled, shared_context).build_windowed(window_builder.clone(), event_loop)
        .or_else(|_| create_window_context_builder(Vsync::Enabled,  Srgb::Disabled, HwAcceleration::Enabled, shared_context).build_windowed(window_builder.clone(), event_loop))
        .or_else(|_| create_window_context_builder(Vsync::Disabled, Srgb::Enabled,  HwAcceleration::Enabled, shared_context).build_windowed(window_builder.clone(), event_loop))
        .or_else(|_| create_window_context_builder(Vsync::Disabled, Srgb::Disabled, HwAcceleration::Enabled, shared_context).build_windowed(window_builder.clone(), event_loop))
        // try building using no hardware acceleration
        .or_else(|_| create_window_context_builder(Vsync::Enabled,  Srgb::Disabled, HwAcceleration::Disabled, shared_context).build_windowed(window_builder.clone(), event_loop))
        .or_else(|_| create_window_context_builder(Vsync::Disabled, Srgb::Enabled,  HwAcceleration::Disabled, shared_context).build_windowed(window_builder.clone(), event_loop))
        .or_else(|_| create_window_context_builder(Vsync::Disabled, Srgb::Disabled, HwAcceleration::Disabled, shared_context).build_windowed(window_builder.clone(), event_loop))
}

fn create_headless_context(
    event_loop: &EventLoop<()>,
    force_hardware: HwAcceleration,
) -> Result<Context<NotCurrent>, GlutinCreationError> {
    use glutin::dpi::PhysicalSize as GlutinPhysicalSize;
    let default_size = GlutinPhysicalSize::new(1, 1);

                 create_window_context_builder(Vsync::Enabled,  Srgb::Enabled,  force_hardware, None).build_headless(event_loop, default_size)
    .or_else(|_| create_window_context_builder(Vsync::Enabled,  Srgb::Disabled, force_hardware, None).build_headless(event_loop, default_size))
    .or_else(|_| create_window_context_builder(Vsync::Disabled, Srgb::Enabled,  force_hardware, None).build_headless(event_loop, default_size))
    .or_else(|_| create_window_context_builder(Vsync::Disabled, Srgb::Disabled, force_hardware, None).build_headless(event_loop, default_size))
}

#[derive(PartialEq, Copy, Clone, Debug)]
enum Vsync { Enabled, Disabled }
impl Vsync { fn is_enabled(&self) -> bool { *self == Vsync::Enabled }}

#[derive(PartialEq, Copy, Clone, Debug)]
enum Srgb { Enabled, Disabled }
impl Srgb { fn is_enabled(&self) -> bool { *self == Srgb::Enabled }}

#[derive(PartialEq, Copy, Clone, Debug)]
enum HwAcceleration { Enabled, Disabled }
impl HwAcceleration { fn is_enabled(&self) -> bool { *self == HwAcceleration::Enabled }}

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
    vsync: Vsync,
    srgb: Srgb,
    hardware_acceleration: HwAcceleration,
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
        .with_vsync(vsync.is_enabled())
        .with_srgb(srgb.is_enabled())
        .with_hardware_acceleration(Some(hardware_acceleration.is_enabled()))
}

// This exists because WrRendererOptions isn't Clone-able
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

#[derive(Debug)]
pub enum RendererCreationError {
    Glutin(GlutinCreationError),
    ShaderCompileError(GlShaderCompileError),
    Wr,
}

impl From<GlutinCreationError> for RendererCreationError {
    fn from(e: GlutinCreationError) -> Self { RendererCreationError::Glutin(e) }
}

// Startup function of the renderer
fn create_renderer(
    event_loop: &EventLoop<()>,
    renderer_type: RendererType,
    device_pixel_ratio: f32,
    device_size: WrDeviceIntSize,
) -> Result<(WrRenderer, WrRenderApi, HeadlessContextState, GlContextPtr), RendererCreationError> {

    use self::RendererType::*;

    // Note: Notifier is fairly useless, since rendering is
    // completely single-threaded, see comments on RenderNotifier impl
    let notifier = Box::new(Notifier { });

    let opts_native = get_renderer_opts(true, device_pixel_ratio);
    let opts_osmesa = get_renderer_opts(false, device_pixel_ratio);

    let (mut renderer, sender, mut headless_context, gl_context) = match renderer_type {
        ForceHardware => {
            let gl_context = create_headless_context(&event_loop, HwAcceleration::Enabled)?;
            let mut headless_context = HeadlessContextState::NotCurrent(gl_context);
            headless_context.make_current();
            let gl_context = get_gl_context(headless_context.headless_context().unwrap()).unwrap();
            let (renderer, sender) = WrRenderer::new(gl_context.get().clone(), notifier, opts_native, WR_SHADER_CACHE, device_size)
                .map_err(|_| RendererCreationError::Wr)?;
            (renderer, sender, headless_context, gl_context)
        },
        ForceSoftware => {
            let gl_context = create_headless_context(&event_loop, HwAcceleration::Disabled)?;
            let mut headless_context = HeadlessContextState::NotCurrent(gl_context);
            headless_context.make_current();
            let gl_context = get_gl_context(headless_context.headless_context().unwrap()).unwrap();
            let (renderer, sender) = WrRenderer::new(gl_context.get().clone(), notifier, opts_osmesa, WR_SHADER_CACHE, device_size)
                .map_err(|_| RendererCreationError::Wr)?;
            (renderer, sender, headless_context, gl_context)
        },
        Default | Custom(_) /* TODO: can't use gl_context here! */ => {
            let ctx = create_headless_context(&event_loop, HwAcceleration::Enabled).map_err(|e| RendererCreationError::Glutin(e));
            match ctx {
                Ok(gl_context) => {
                    let mut headless_context = HeadlessContextState::NotCurrent(gl_context);
                    headless_context.make_current();
                    let gl_context = get_gl_context(headless_context.headless_context().unwrap()).unwrap();
                    let (renderer, sender) = WrRenderer::new(gl_context.get().clone(), notifier, opts_native, WR_SHADER_CACHE, device_size)
                        .map_err(|_| RendererCreationError::Wr)?;
                    (renderer, sender, headless_context, gl_context)
                },
                Err(_) => {
                    let gl_context = create_headless_context(&event_loop, HwAcceleration::Disabled)?;
                    let mut headless_context = HeadlessContextState::NotCurrent(gl_context);
                    headless_context.make_current();
                    let gl_context = get_gl_context(headless_context.headless_context().unwrap()).unwrap();
                    let (renderer, sender) = WrRenderer::new(gl_context.get().clone(), notifier, opts_native, WR_SHADER_CACHE, device_size)
                        .map_err(|_| RendererCreationError::Wr)?;
                    (renderer, sender, headless_context, gl_context)
                },
            }
        }
    };

    let api = sender.create_api();

    renderer.set_external_image_handler(Box::new(Compositor::default()));
    headless_context.make_not_current();

    Ok((renderer, api, headless_context, gl_context))
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
    options.iter().filter_map(|x| *x).next()
}
