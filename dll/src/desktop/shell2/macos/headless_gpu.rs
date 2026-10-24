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
