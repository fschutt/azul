//! GPU frames without a window, for tests.
//!
//! A CGL context with no drawable renders the real GPU frame pipeline - the
//! layout's display list, `generate_frame`, WebRender - into an offscreen
//! framebuffer, and the pixels are read back. The CPU renderer draws the same
//! display list, so a test can hold the two backends to the same picture. A
//! machine where no OpenGL context can be made (a CI runner without a GPU)
//! gets `None`, and the tests that need one pass without checking anything.

use std::{
    ffi::c_void,
    rc::Rc,
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    gl::OptionGlContextPtr,
    glconst,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{render, AzulPixmap, RenderOptions},
    glyph_cache::GlyphCache,
    solver3::display_list::DisplayList,
    window::LayoutWindow,
    window_state::{FullWindowState, WindowCreateOptions},
};
use gl_context_loader::GenericGlContext;
use rust_fontconfig::FcFontCache;
use webrender::{api::units::DeviceIntSize, render_api::Transaction};

use super::gl::GlFunctions;
use crate::desktop::wr_translate2::{
    default_renderer_options, generate_frame, translate_document_id_wr,
    translate_id_namespace_wr, wr_translate_document_id, Notifier,
};

#[link(name = "OpenGL", kind = "framework")]
extern "C" {
    fn CGLChoosePixelFormat(attribs: *const i32, pix: *mut *mut c_void, npix: *mut i32) -> i32;
    fn CGLDestroyPixelFormat(pix: *mut c_void) -> i32;
    fn CGLCreateContext(pix: *mut c_void, share: *mut c_void, ctx: *mut *mut c_void) -> i32;
    fn CGLDestroyContext(ctx: *mut c_void) -> i32;
    fn CGLSetCurrentContext(ctx: *mut c_void) -> i32;
}

/// A CGL context made current on this thread, destroyed when dropped.
struct OffscreenContext(*mut c_void);

impl OffscreenContext {
    /// A 3.2 core context like the window's (`create_opengl_pixel_format`),
    /// on the GPU if there is one, else on any renderer that offers it.
    fn new() -> Option<Self> {
        const PROFILE: i32 = 99;
        const CORE_3_2: i32 = 0x3200;
        const COLOR_SIZE: i32 = 8;
        const ALPHA_SIZE: i32 = 11;
        const DEPTH_SIZE: i32 = 12;
        const ACCELERATED: i32 = 73;
        const ALLOW_OFFLINE_RENDERERS: i32 = 96;
        let base = [PROFILE, CORE_3_2, COLOR_SIZE, 24, ALPHA_SIZE, 8, DEPTH_SIZE, 24];
        for accelerated in [true, false] {
            let mut attribs = base.to_vec();
            if accelerated {
                attribs.push(ACCELERATED);
            }
            attribs.extend([ALLOW_OFFLINE_RENDERERS, 0]);
            let mut pixel_format = std::ptr::null_mut();
            let mut count = 0;
            unsafe {
                if CGLChoosePixelFormat(attribs.as_ptr(), &mut pixel_format, &mut count) != 0
                    || pixel_format.is_null()
                {
                    continue;
                }
                let mut context = std::ptr::null_mut();
                let made = CGLCreateContext(pixel_format, std::ptr::null_mut(), &mut context);
                CGLDestroyPixelFormat(pixel_format);
                if made != 0 || context.is_null() {
                    continue;
                }
                if CGLSetCurrentContext(context) != 0 {
                    CGLDestroyContext(context);
                    continue;
                }
                return Some(Self(context));
            }
        }
        None
    }
}

impl Drop for OffscreenContext {
    fn drop(&mut self) {
        unsafe {
            CGLSetCurrentContext(std::ptr::null_mut());
            CGLDestroyContext(self.0);
        }
    }
}

/// A colour and a depth buffer WebRender draws into: it takes the
/// framebuffer bound when a frame begins as its own.
struct Framebuffer {
    gl: Rc<GenericGlContext>,
    fbo: u32,
    color: u32,
    depth: u32,
}

impl Framebuffer {
    fn new(gl: Rc<GenericGlContext>, width: i32, height: i32) -> Option<Self> {
        let fbo = *gl.gen_framebuffers(1).first()?;
        let buffers = gl.gen_renderbuffers(2);
        let (color, depth) = (*buffers.first()?, *buffers.get(1)?);
        gl.bind_framebuffer(glconst::FRAMEBUFFER, fbo);
        gl.bind_renderbuffer(glconst::RENDERBUFFER, color);
        gl.renderbuffer_storage(glconst::RENDERBUFFER, glconst::RGBA8, width, height);
        gl.framebuffer_renderbuffer(glconst::FRAMEBUFFER, glconst::COLOR_ATTACHMENT0, glconst::RENDERBUFFER, color);
        gl.bind_renderbuffer(glconst::RENDERBUFFER, depth);
        gl.renderbuffer_storage(glconst::RENDERBUFFER, glconst::DEPTH_COMPONENT24, width, height);
        gl.framebuffer_renderbuffer(glconst::FRAMEBUFFER, glconst::DEPTH_ATTACHMENT, glconst::RENDERBUFFER, depth);
        let complete = gl.check_frame_buffer_status(glconst::FRAMEBUFFER) == glconst::FRAMEBUFFER_COMPLETE;
        let me = Self {
            gl,
            fbo,
            color,
            depth,
        };
        complete.then_some(me)
    }

    fn bind(&self) {
        self.gl.bind_framebuffer(glconst::FRAMEBUFFER, self.fbo);
    }

    /// The framebuffer's pixels, top row first, as RGBA.
    fn read(&self, width: u32, height: u32) -> Vec<u8> {
        self.bind();
        self.gl.finish();
        let row = width as usize * 4;
        // `GenericGlContext::read_pixels` sizes its buffer at one byte per
        // pixel for any format, and glReadPixels would write past it.
        let mut bottom_up = vec![0; row * height as usize];
        self.gl.read_pixels_into_buffer(
            0,
            0,
            width as i32,
            height as i32,
            glconst::RGBA,
            glconst::UNSIGNED_BYTE,
            &mut bottom_up,
        );
        bottom_up
            .chunks_exact(row)
            .rev()
            .flatten()
            .copied()
            .collect()
    }
}

impl Drop for Framebuffer {
    fn drop(&mut self) {
        self.gl.bind_framebuffer(glconst::FRAMEBUFFER, 0);
        self.gl.delete_renderbuffers(&[self.color, self.depth]);
        self.gl.delete_framebuffers(&[self.fbo]);
    }
}

/// One frame of a DOM, drawn by both backends on a white canvas.
pub(crate) struct Frames {
    /// WebRender's frame, read back from the GPU.
    pub(crate) gpu: AzulPixmap,
    /// The CPU renderer's frame of the same display list.
    pub(crate) cpu: AzulPixmap,
    /// The root DOM's display list both were drawn from.
    pub(crate) display_list: Arc<DisplayList>,
}

/// `dom` laid out in a `width` x `height` window at `dpi` device pixels per
/// CSS px and drawn by both backends, or `None` without an OpenGL context.
pub(crate) fn draw_both(dom: Dom, width: f32, height: f32, dpi: f32) -> Option<Frames> {
    let context = OffscreenContext::new()?;
    let gl_functions = GlFunctions::initialize().ok()?;
    let gl = gl_functions.get_context();
    let (device_width, device_height) = ((width * dpi) as u32, (height * dpi) as u32);
    let device_size = DeviceIntSize::new(device_width as i32, device_height as i32);
    let framebuffer = Framebuffer::new(gl.clone(), device_size.width, device_size.height)?;

    let new_frame_ready = Arc::new((Mutex::new(false), Condvar::new()));
    let notifier = Notifier {
        new_frame_ready: new_frame_ready.clone(),
        wake: None,
    };
    let mut options = default_renderer_options(&WindowCreateOptions::default(), None, None);
    options.clear_color = webrender::api::ColorF::WHITE;
    let (mut renderer, sender) =
        webrender::create_webrender_instance(gl.clone(), Box::new(notifier), options, None).ok()?;
    let mut render_api = sender.create_api();

    let mut window = LayoutWindow::new(FcFontCache::build()).ok()?;
    window.document_id = translate_document_id_wr(render_api.add_document(device_size));
    window.id_namespace = translate_id_namespace_wr(render_api.get_namespace_id());
    let mut state = FullWindowState::default();
    state.size.dimensions = LogicalSize::new(width, height);
    state.size.dpi = (96.0 * dpi) as u32;
    window.current_window_state = state.clone();
    let mut messages = None;
    window
        .layout_and_generate_display_list(
            StyledDom::create_from_dom(dom),
            &state,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut messages,
        )
        .ok()?;

    let mut txn = Transaction::new();
    generate_frame(&mut txn, &mut window, &mut render_api, true, &OptionGlContextPtr::None);
    render_api.send_transaction(wr_translate_document_id(window.document_id), txn);
    render_api.flush_scene_builder();
    {
        let (ready, signal) = &*new_frame_ready;
        let mut ready = ready.lock().ok()?;
        while !*ready {
            let (next, timeout) = signal.wait_timeout(ready, Duration::from_secs(20)).ok()?;
            ready = next;
            if timeout.timed_out() {
                return None;
            }
        }
    }
    framebuffer.bind();
    renderer.update();
    renderer.render(device_size, 0).ok()?;
    let mut gpu = AzulPixmap::new(device_width, device_height)?;
    gpu.data_mut()
        .copy_from_slice(&framebuffer.read(device_width, device_height));
    drop(framebuffer);
    renderer.deinit();
    drop(context);

    let display_list = window
        .get_layout_result(&DomId::ROOT_ID)?
        .display_list
        .clone();
    let cpu = render(
        &display_list,
        &window.renderer_resources,
        &window.font_manager,
        RenderOptions {
            width,
            height,
            dpi_factor: dpi,
        },
        &mut GlyphCache::new(),
    )
    .ok()?;
    Some(Frames {
        gpu,
        cpu,
        display_list,
    })
}

/// `dom()` drawn with its MATERIALS on the GPU, as a GPU window draws them:
/// the window is marked hardware (its background image callbacks run only in
/// the GL frame path, with this context), so the material shader draws every
/// `builtin(<material>)` background into a GL texture WebRender shows. The
/// CPU frame comes from a second window laid out the same way without GL,
/// whose materials the CPU shader draws. Also answers whether the GPU
/// window's display list holds GL textures (the material shader ran). `None`
/// without an OpenGL context.
pub(crate) fn draw_gpu_materials(
    dom: impl Fn() -> Dom,
    width: f32,
    height: f32,
    dpi: f32,
) -> Option<(Frames, bool)> {
    use azul_core::{gl::GlContextPtr, window::RendererType};
    use azul_layout::solver3::display_list::DisplayListItem;

    let context = OffscreenContext::new()?;
    let gl_functions = GlFunctions::initialize().ok()?;
    let gl = gl_functions.get_context();
    let gl_ptr = GlContextPtr::new(RendererType::Hardware, gl.clone());
    if !gl_ptr.is_gl_usable() {
        return None;
    }
    let (device_width, device_height) = ((width * dpi) as u32, (height * dpi) as u32);
    let device_size = DeviceIntSize::new(device_width as i32, device_height as i32);
    let framebuffer = Framebuffer::new(gl.clone(), device_size.width, device_size.height)?;

    let new_frame_ready = Arc::new((Mutex::new(false), Condvar::new()));
    let notifier = Notifier {
        new_frame_ready: new_frame_ready.clone(),
        wake: None,
    };
    let mut options = default_renderer_options(&WindowCreateOptions::default(), None, None);
    options.clear_color = webrender::api::ColorF::WHITE;
    let (mut renderer, sender) =
        webrender::create_webrender_instance(gl.clone(), Box::new(notifier), options, None).ok()?;
    renderer.set_external_image_handler(Box::new(crate::desktop::wr_translate2::Compositor::default()));
    let mut render_api = sender.create_api();

    let mut state = FullWindowState::default();
    state.size.dimensions = LogicalSize::new(width, height);
    state.size.dpi = (96.0 * dpi) as u32;

    let mut window = LayoutWindow::new(FcFontCache::build()).ok()?;
    window.document_id = translate_document_id_wr(render_api.add_document(device_size));
    window.id_namespace = translate_id_namespace_wr(render_api.get_namespace_id());
    window.renderer_type = Some(RendererType::Hardware);
    window.current_window_state = state.clone();
    let mut messages = None;
    window
        .layout_and_generate_display_list(
            StyledDom::create_from_dom(dom()),
            &state,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut messages,
        )
        .ok()?;

    let mut txn = Transaction::new();
    generate_frame(
        &mut txn,
        &mut window,
        &mut render_api,
        true,
        &OptionGlContextPtr::Some(gl_ptr.clone()),
    );
    render_api.send_transaction(wr_translate_document_id(window.document_id), txn);
    render_api.flush_scene_builder();
    {
        let (ready, signal) = &*new_frame_ready;
        let mut ready = ready.lock().ok()?;
        while !*ready {
            let (next, timeout) = signal.wait_timeout(ready, Duration::from_secs(20)).ok()?;
            ready = next;
            if timeout.timed_out() {
                return None;
            }
        }
    }
    framebuffer.bind();
    renderer.update();
    renderer.render(device_size, 0).ok()?;
    let mut gpu = AzulPixmap::new(device_width, device_height)?;
    gpu.data_mut()
        .copy_from_slice(&framebuffer.read(device_width, device_height));
    let display_list = window
        .get_layout_result(&DomId::ROOT_ID)?
        .display_list
        .clone();
    let on_gpu = display_list.items.iter().any(|item| {
        matches!(item, DisplayListItem::Image { image, .. } if image.is_gl_texture())
    });
    // The GL textures go while their context is current.
    drop(display_list);
    drop(window);
    drop(framebuffer);
    renderer.deinit();
    drop(gl_ptr);
    drop(context);

    let mut cpu_window = LayoutWindow::new(FcFontCache::build()).ok()?;
    cpu_window.current_window_state = state.clone();
    cpu_window
        .layout_and_generate_display_list(
            StyledDom::create_from_dom(dom()),
            &state,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut messages,
        )
        .ok()?;
    cpu_window.prepare_frame_content();
    let display_list = cpu_window
        .get_layout_result(&DomId::ROOT_ID)?
        .display_list
        .clone();
    let cpu = render(
        &display_list,
        &cpu_window.renderer_resources,
        &cpu_window.font_manager,
        RenderOptions {
            width,
            height,
            dpi_factor: dpi,
        },
        &mut GlyphCache::new(),
    )
    .ok()?;
    Some((
        Frames {
            gpu,
            cpu,
            display_list,
        },
        on_gpu,
    ))
}

/// The GL state a renderer sets up and expects to find again after the
/// frame's content step drew the materials: the enables a draw touches, the
/// active texture unit and the texture on each unit the material shader
/// samples, the framebuffer and the program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GlStateProbe {
    /// Whether blending is on.
    pub blend: bool,
    /// Whether the scissor test is on.
    pub scissor: bool,
    /// Whether the depth test is on.
    pub depth: bool,
    /// The active texture unit.
    pub active_texture: i32,
    /// The 2D texture bound to each of the units 0..3.
    pub textures: [i32; 4],
    /// The framebuffer bound.
    pub framebuffer: i32,
    /// The program in use.
    pub program: i32,
}

impl GlStateProbe {
    fn read(gl: &GenericGlContext) -> Self {
        let one = |name: u32| {
            let mut value = [0_i32];
            // SAFETY: each query writes one int.
            unsafe { gl.get_integer_v(name, &mut value) };
            value[0]
        };
        let active_texture = one(glconst::ACTIVE_TEXTURE);
        let mut textures = [0_i32; 4];
        for (unit, bound) in textures.iter_mut().enumerate() {
            gl.active_texture(glconst::TEXTURE0 + unit as u32);
            *bound = one(glconst::TEXTURE_BINDING_2D);
        }
        gl.active_texture(active_texture as u32);
        Self {
            blend: gl.is_enabled(glconst::BLEND) != 0,
            scissor: gl.is_enabled(glconst::SCISSOR_TEST) != 0,
            depth: gl.is_enabled(glconst::DEPTH_TEST) != 0,
            active_texture,
            textures,
            framebuffer: one(glconst::FRAMEBUFFER_BINDING),
            program: one(glconst::CURRENT_PROGRAM),
        }
    }
}

/// `dom()`'s materials drawn through the frame's content step
/// (`LayoutWindow::prepare_frame_gl`) in a context whose state a renderer set
/// up - blending and the scissor on, a texture on every unit the material
/// shader samples, unit 2 active: the state before and after, and whether the
/// material shader runs here. `None` without OpenGL.
#[cfg(feature = "materials")]
pub(crate) fn gl_state_around_materials(
    dom: impl Fn() -> Dom,
) -> Option<(GlStateProbe, GlStateProbe, bool)> {
    use azul_core::{gl::GlContextPtr, window::RendererType};

    let context = OffscreenContext::new()?;
    let gl_functions = GlFunctions::initialize().ok()?;
    let gl = gl_functions.get_context();
    let gl_ptr = GlContextPtr::new(RendererType::Hardware, gl.clone());
    if !gl_ptr.is_gl_usable() {
        return None;
    }
    let on_gpu = azul_layout::texture::materials::material_shader_compiles(&gl_ptr);
    let names = gl.gen_textures(4);
    for (unit, &name) in names.iter().enumerate() {
        gl.active_texture(glconst::TEXTURE0 + unit as u32);
        gl.bind_texture(glconst::TEXTURE_2D, name);
    }
    gl.active_texture(glconst::TEXTURE0 + 2);
    gl.enable(glconst::BLEND);
    gl.enable(glconst::SCISSOR_TEST);
    gl.disable(glconst::DEPTH_TEST);
    let before = GlStateProbe::read(&gl);

    let mut state = FullWindowState::default();
    state.size.dimensions = LogicalSize::new(260.0, 120.0);
    let mut window = LayoutWindow::new(FcFontCache::build()).ok()?;
    window.renderer_type = Some(RendererType::Hardware);
    window.current_window_state = state.clone();
    window
        .layout_and_generate_display_list(
            StyledDom::create_from_dom(dom()),
            &state,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut None,
        )
        .ok()?;
    let _ = window.prepare_frame_gl(&OptionGlContextPtr::Some(gl_ptr.clone()));
    let after = GlStateProbe::read(&gl);

    // The GL textures go while their context is current.
    drop(window);
    gl.delete_textures(&names);
    drop(gl_ptr);
    drop(context);
    Some((before, after, on_gpu))
}

/// The largest difference of any channel between the two frames' pixels in
/// the device-pixel box `(x, y, w, h)`, and where it is.
pub(crate) fn largest_difference(
    a: &AzulPixmap,
    b: &AzulPixmap,
    (x, y, w, h): (u32, u32, u32, u32),
) -> (u8, (u32, u32)) {
    let mut worst = (0u8, (x, y));
    for py in y..(y + h).min(a.height).min(b.height) {
        for px in x..(x + w).min(a.width).min(b.width) {
            let i = ((py * a.width + px) * 4) as usize;
            let j = ((py * b.width + px) * 4) as usize;
            for c in 0..4 {
                let d = a.data()[i + c].abs_diff(b.data()[j + c]);
                if d > worst.0 {
                    worst = (d, (px, py));
                }
            }
        }
    }
    worst
}

/// How many pixels of the device-pixel box `(x, y, w, h)` differ between the
/// two frames by more than `by` in some channel, and the largest difference
/// with where it is. A pixel of `a` that matches one of `b` at most one device
/// pixel away is not counted: two rasterisers may put a long curved edge
/// half a pixel apart all along it, while a missing bar or block still
/// differs inside. A count tells those from the antialiasing of an edge.
pub(crate) fn pixels_differing(
    a: &AzulPixmap,
    b: &AzulPixmap,
    (x, y, w, h): (u32, u32, u32, u32),
    by: u8,
) -> (u32, (u8, (u32, u32))) {
    let (x_end, y_end) = (
        (x + w).min(a.width).min(b.width),
        (y + h).min(a.height).min(b.height),
    );
    let matches_near = |px: u32, py: u32| {
        let i = ((py * a.width + px) * 4) as usize;
        (py.saturating_sub(1)..=(py + 1).min(b.height - 1)).any(|qy| {
            (px.saturating_sub(1)..=(px + 1).min(b.width - 1)).any(|qx| {
                let j = ((qy * b.width + qx) * 4) as usize;
                (0..4).all(|c| a.data()[i + c].abs_diff(b.data()[j + c]) <= by)
            })
        })
    };
    let mut differing = 0;
    for py in y..y_end {
        for px in x..x_end {
            if !matches_near(px, py) {
                differing += 1;
            }
        }
    }
    (differing, largest_difference(a, b, (x, y, w, h)))
}

/// Writes both frames as PNGs into `$AZ_GPU_PIXELS_DIR`, when it is set, to
/// look at what a failing test saw.
pub(crate) fn keep(name: &str, frames: &Frames) {
    let Some(dir) = std::env::var_os("AZ_GPU_PIXELS_DIR") else {
        return;
    };
    for (backend, pixmap) in [("gpu", &frames.gpu), ("cpu", &frames.cpu)] {
        if let Ok(png) = pixmap.encode_png() {
            let _ = std::fs::write(
                std::path::Path::new(&dir).join(format!("{name}-{backend}.png")),
                png,
            );
        }
    }
}

#[cfg(test)]
mod document_shown_tests {
    use super::*;
    use crate::desktop::wr_translate2::renderer_shows_document;

    /// WebRender builds a frame after the pass that sent its display list,
    /// so a render shows a window's document only once a later update took
    /// that frame in; before, the window shows its bare background, which
    /// is what a new window must not be shown with.
    #[test]
    fn the_renderer_shows_a_document_only_once_its_frame_was_taken_in() {
        let Some(context) = OffscreenContext::new() else {
            return; // no OpenGL here
        };
        let Ok(gl_functions) = GlFunctions::initialize() else {
            return;
        };
        let gl = gl_functions.get_context();
        let device_size = DeviceIntSize::new(64, 64);
        let Some(framebuffer) = Framebuffer::new(gl.clone(), 64, 64) else {
            return;
        };
        let new_frame_ready = Arc::new((Mutex::new(false), Condvar::new()));
        let notifier = Notifier {
            new_frame_ready: new_frame_ready.clone(),
            wake: None,
        };
        let options = default_renderer_options(&WindowCreateOptions::default(), None, None);
        let (mut renderer, sender) =
            webrender::create_webrender_instance(gl.clone(), Box::new(notifier), options, None)
                .expect("a renderer");
        let mut render_api = sender.create_api();
        let mut window = LayoutWindow::new(FcFontCache::build()).expect("a window");
        window.document_id = translate_document_id_wr(render_api.add_document(device_size));
        window.id_namespace = translate_id_namespace_wr(render_api.get_namespace_id());
        let mut state = FullWindowState::default();
        state.size.dimensions = LogicalSize::new(64.0, 64.0);
        window.current_window_state = state.clone();
        window
            .layout_and_generate_display_list(
                StyledDom::create_from_dom(Dom::create_body()),
                &state,
                &RendererResources::default(),
                &ExternalSystemCallbacks::rust_internal(),
                &mut None,
            )
            .expect("laid out");
        framebuffer.bind();

        renderer.update();
        assert!(
            !renderer_shows_document(&renderer, window.document_id),
            "nothing was sent yet"
        );

        let mut txn = Transaction::new();
        generate_frame(&mut txn, &mut window, &mut render_api, true, &OptionGlContextPtr::None);
        render_api.send_transaction(wr_translate_document_id(window.document_id), txn);
        {
            let (ready, signal) = &*new_frame_ready;
            let mut ready = ready.lock().expect("the signal");
            while !*ready {
                let (next, timeout) = signal
                    .wait_timeout(ready, Duration::from_secs(20))
                    .expect("the signal");
                ready = next;
                assert!(!timeout.timed_out(), "WebRender built no frame");
            }
        }
        assert!(
            !renderer_shows_document(&renderer, window.document_id),
            "built, but not taken in yet"
        );
        renderer.update();
        assert!(
            renderer_shows_document(&renderer, window.document_id),
            "the frame taken in holds the document"
        );
        renderer.render(device_size, 0).expect("rendered");
        drop(framebuffer);
        renderer.deinit();
        drop(context);
    }
}

/// A window the GPU draws, frame after frame, without a window: the layout window
/// (`window`) and WebRender, with every frame read back from the offscreen
/// framebuffer. A test lays the window out, changes it the way the shells'
/// events do, and asks for the next frame the way a shell does
/// ([`Self::present`]) - which is how a retained display list, or a value that
/// changed without one, is seen where it counts: in the pixels.
pub(crate) struct GpuSession {
    /// The layout window the frames are made from.
    pub(crate) window: LayoutWindow,
    context: Option<OffscreenContext>,
    framebuffer: Option<Framebuffer>,
    renderer: Option<webrender::Renderer>,
    render_api: webrender::render_api::RenderApi,
    new_frame_ready: Arc<(Mutex<bool>, Condvar)>,
    device_size: DeviceIntSize,
}

impl GpuSession {
    /// A session of `width` x `height` CSS px at `dpi` device px per CSS px, or
    /// `None` without an OpenGL context.
    pub(crate) fn new(width: f32, height: f32, dpi: f32) -> Option<Self> {
        let context = OffscreenContext::new()?;
        let gl_functions = GlFunctions::initialize().ok()?;
        let gl = gl_functions.get_context();
        let (device_width, device_height) = ((width * dpi) as u32, (height * dpi) as u32);
        let device_size = DeviceIntSize::new(device_width as i32, device_height as i32);
        let framebuffer = Framebuffer::new(gl.clone(), device_size.width, device_size.height)?;
        let new_frame_ready = Arc::new((Mutex::new(false), Condvar::new()));
        let notifier = Notifier {
            new_frame_ready: new_frame_ready.clone(),
            wake: None,
        };
        let mut options = default_renderer_options(&WindowCreateOptions::default(), None, None);
        options.clear_color = webrender::api::ColorF::WHITE;
        let (renderer, sender) =
            webrender::create_webrender_instance(gl, Box::new(notifier), options, None).ok()?;
        let mut render_api = sender.create_api();
        let mut window = LayoutWindow::new(FcFontCache::build()).ok()?;
        window.document_id = translate_document_id_wr(render_api.add_document(device_size));
        window.id_namespace = translate_id_namespace_wr(render_api.get_namespace_id());
        let mut state = FullWindowState::default();
        state.size.dimensions = LogicalSize::new(width, height);
        state.size.dpi = (96.0 * dpi) as u32;
        window.current_window_state = state;
        Some(Self {
            window,
            context: Some(context),
            framebuffer: Some(framebuffer),
            renderer: Some(renderer),
            render_api,
            new_frame_ready,
            device_size,
        })
    }

    /// Lay `dom` out as the window's new page.
    pub(crate) fn lay_out(&mut self, dom: Dom) -> Option<()> {
        let state = self.window.current_window_state.clone();
        self.window
            .layout_and_generate_display_list(
                StyledDom::create_from_dom(dom),
                &state,
                &RendererResources::default(),
                &ExternalSystemCallbacks::rust_internal(),
                &mut None,
            )
            .ok()
    }

    /// The next frame, the way a shell makes it: `display_list_was_rebuilt` is
    /// `generate_frame`'s flag - the display lists are translated and sent (the
    /// ones WebRender does not hold) - and without it the frame is "nothing was
    /// rebuilt": only the property values, scroll offsets and images go across.
    /// Returns what the GPU drew, top row first.
    pub(crate) fn present(&mut self, display_list_was_rebuilt: bool) -> Option<AzulPixmap> {
        // A notification still on its way from the last frame must not
        // be taken for this one's.
        std::thread::sleep(Duration::from_millis(30));
        *self.new_frame_ready.0.lock().ok()? = false;
        let mut txn = Transaction::new();
        generate_frame(
            &mut txn,
            &mut self.window,
            &mut self.render_api,
            display_list_was_rebuilt,
            &OptionGlContextPtr::None,
        );
        self.render_api
            .send_transaction(wr_translate_document_id(self.window.document_id), txn);
        self.render_api.flush_scene_builder();
        {
            let (ready, signal) = &*self.new_frame_ready;
            let mut ready = ready.lock().ok()?;
            while !*ready {
                let (next, timeout) = signal.wait_timeout(ready, Duration::from_secs(20)).ok()?;
                ready = next;
                if timeout.timed_out() {
                    return None;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(30));
        let framebuffer = self.framebuffer.as_ref()?;
        framebuffer.bind();
        let renderer = self.renderer.as_mut()?;
        renderer.update();
        renderer.render(self.device_size, 0).ok()?;
        let (w, h) = (self.device_size.width as u32, self.device_size.height as u32);
        let mut pixels = AzulPixmap::new(w, h)?;
        pixels.data_mut().copy_from_slice(&framebuffer.read(w, h));
        Some(pixels)
    }
}

impl Drop for GpuSession {
    fn drop(&mut self) {
        drop(self.framebuffer.take());
        if let Some(renderer) = self.renderer.take() {
            renderer.deinit();
        }
        drop(self.context.take());
    }
}

/// The device-pixel box `(x0, y0, x1, y1)` of the pixels of `frame` that are
/// "red" (a red channel above 200 and the others below 70), or `None`.
pub(crate) fn red_box(frame: &AzulPixmap) -> Option<(u32, u32, u32, u32)> {
    let mut found: Option<(u32, u32, u32, u32)> = None;
    for y in 0..frame.height {
        for x in 0..frame.width {
            let i = ((y * frame.width + x) * 4) as usize;
            let px = &frame.data()[i..i + 4];
            if px[0] > 200 && px[1] < 70 && px[2] < 70 {
                found = Some(match found {
                    Some((a, b, c, d)) => (a.min(x), b.min(y), c.max(x), d.max(y)),
                    None => (x, y, x, y),
                });
            }
        }
    }
    found
}

#[cfg(test)]
mod retained_list_tests {
    //! What WebRender draws when a frame does not send a display list again. A
    //! `VirtualView` is an iframe of its child DOM's own pipeline and WebRender
    //! keeps a pipeline's list until another replaces it, so a restyle of the
    //! host (a hover, the caret) sends the host's list alone and the child's
    //! rows must still be drawn; and a `transform` written to a node the list
    //! binds is a property value that reaches the frame with no list at all.
    //! Checked in the pixels of the real GPU pipeline, offscreen: a machine
    //! without an OpenGL context passes without checking anything.

    use azul_core::{
        callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
        dom::{DomId, NodeId},
        geom::{LogicalPosition, LogicalRect},
        refany::RefAny,
    };
    use azul_css::props::{
        basic::{PixelValue},
        property::CssProperty,
        style::{StyleTransform, StyleTransformTranslate2D},
    };
    use azul_layout::overlay::{ContentChange, ContentDirtyTier};

    use super::*;

    /// Forty rows of red and blue, 5 px each.
    extern "C" fn rows(_data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
        let mut dom = Dom::create_div();
        for i in 0..40 {
            dom = dom.with_child(Dom::create_div().with_css(if i % 2 == 0 {
                "height: 5px; background: #ff0000;"
            } else {
                "height: 5px; background: #0000ff;"
            }));
        }
        let rect = LogicalRect::new(LogicalPosition::zero(), info.bounds.logical_size);
        VirtualViewReturn::with_dom(dom, rect, rect)
    }

    /// A grey bar on top, the view (300 x 160) under it.
    fn page() -> Dom {
        Dom::create_body()
            .with_css("margin: 0px;")
            .with_child(
                Dom::create_div()
                    .with_class("bar".into())
                    .with_css("height: 40px; width: 300px; background: #888888;"),
            )
            .with_child(
                Dom::create_virtual_view(RefAny::new(()), VirtualViewCallback::create(rows))
                    .with_css("width: 300px; height: 160px;"),
            )
    }

    fn pixel(frame: &AzulPixmap, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * frame.width + x) * 4) as usize;
        [frame.data()[i], frame.data()[i + 1], frame.data()[i + 2], frame.data()[i + 3]]
    }

    fn node_with_class(window: &LayoutWindow, class: &str) -> NodeId {
        let sd = &window.layout_results[&DomId::ROOT_ID].styled_dom;
        let nodes = sd.node_data.as_container();
        (0..nodes.len())
            .map(NodeId::new)
            .find(|n| nodes[*n].has_class(class))
            .expect("the node")
    }

    #[test]
    fn a_view_the_frame_does_not_send_again_is_still_drawn() {
        let Some(mut gpu) = GpuSession::new(300.0, 200.0, 1.0) else {
            return;
        };
        gpu.lay_out(page()).expect("the page lays out");
        let first = gpu.present(true).expect("the first frame");
        // The view's rows are drawn: red at its top row, blue under it.
        assert_eq!(pixel(&first, 100, 42)[..3], [255, 0, 0], "the view's first row");
        assert_eq!(pixel(&first, 100, 47)[..3], [0, 0, 255], "its second row");
        assert_eq!(pixel(&first, 100, 10)[..3], [0x88, 0x88, 0x88], "the bar over it");

        // The host restyles (the bar turns dark): the host's list is rebuilt, the
        // view's is the one WebRender holds.
        let bar = node_with_class(&gpu.window, "bar");
        let tier = gpu
            .window
            .apply_content_change(ContentChange::NodeCss {
                dom_id: DomId::ROOT_ID,
                node_id: bar,
                props: vec![CssProperty::const_background_content(
                    azul_css::props::style::StyleBackgroundContentVec::from_vec(vec![
                        azul_css::props::style::StyleBackgroundContent::Color(
                            azul_css::props::basic::ColorU::new(0x22, 0x22, 0x22, 255),
                        ),
                    ]),
                )],
                override_only: false,
            })
            .tier;
        assert_eq!(tier, ContentDirtyTier::RebuildDisplayList, "a colour is a rebuild");
        let child = *gpu
            .window
            .layout_results
            .keys()
            .find(|dom| **dom != DomId::ROOT_ID)
            .expect("the view's DOM");
        let env = u64::from(gpu.window.current_window_state.size.get_hidpi_factor().inner.get().to_bits());
        let held = |gpu: &GpuSession, dom: DomId| {
            gpu.window.gpu_state_manager.display_list_is_held(
                dom,
                &gpu.window.layout_results[&dom].display_list,
                env,
            )
        };
        assert!(!held(&gpu, DomId::ROOT_ID), "the host's list is a new one");
        assert!(held(&gpu, child), "the view's is the one WebRender holds");

        let second = gpu.present(true).expect("the second frame");
        assert_eq!(pixel(&second, 100, 10)[..3], [0x22, 0x22, 0x22], "the host's new list is drawn");
        assert_eq!(
            pixels_differing(&first, &second, (0, 40, 300, 160), 0).0,
            0,
            "the view's rows were not sent again and are drawn exactly as before"
        );
    }

    #[test]
    fn a_transform_written_to_a_bound_node_moves_it_in_the_next_frame_with_no_list() {
        let Some(mut gpu) = GpuSession::new(300.0, 200.0, 1.0) else {
            return;
        };
        gpu.lay_out(
            Dom::create_body().with_css("margin: 0px;").with_child(
                Dom::create_div()
                    .with_class("mover".into())
                    .with_css("width: 40px; height: 40px; background: #ff0000; transform: translate(10px, 0px);"),
            ),
        )
        .expect("the page lays out");
        let before = gpu.present(true).expect("the first frame");
        let (x0, y0, x1, y1) = red_box(&before).expect("the box is drawn");
        assert!(
            (10..=11).contains(&x0) && (49..=50).contains(&x1) && y0 <= 1 && (39..=40).contains(&y1),
            "harness: the box at translate(10px): {:?}",
            (x0, y0, x1, y1)
        );

        gpu.window.frame_report.reset_counters();
        let mover = node_with_class(&gpu.window, "mover");
        let tier = gpu
            .window
            .apply_content_change(ContentChange::NodeCss {
                dom_id: DomId::ROOT_ID,
                node_id: mover,
                props: vec![CssProperty::transform(
                    vec![StyleTransform::Translate(StyleTransformTranslate2D {
                        x: PixelValue::px(60.0),
                        y: PixelValue::px(0.0),
                    })]
                    .into(),
                )],
                override_only: false,
            })
            .tier;
        assert_eq!(tier, ContentDirtyTier::Paint, "a value, not a rebuild");
        assert_eq!(gpu.window.frame_report.dl_rebuilds, 0, "no display-list regeneration");
        assert!(
            !gpu.window
                .gpu_state_manager
                .display_lists_unsubmitted(&gpu.window.layout_results),
            "the list is the one WebRender holds"
        );

        // The frame a shell makes for it: nothing rebuilt, no list sent.
        let after = gpu.present(false).expect("the next frame");
        let (a0, b0, a1, b1) = red_box(&after).expect("the box is still drawn");
        assert_eq!(
            (a0, a1, b0, b1),
            (x0 + 50, x1 + 50, y0, y1),
            "the next frame draws the box 50 px to the right, from the value alone"
        );
    }
}
