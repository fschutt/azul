//! The GPU renderer on OpenGL ES, on a host: WebRender over an OpenGL ES 3 context that Mesa makes on EGL's surfaceless
//! platform (llvmpipe, no window system, no GPU), and the CPU renderer's picture of the same display list.
//!
//! This is the whole GLES path of the phone shells except the window: the EGL binding (`egl_gles`), the function table
//! with its extension aliases, the probe, the bring-up (`gpu_renderer::init_webrender`), the GLES branches of the vendored
//! device (the depth clear), and every WebRender shader compiled as `#version 300 es`. What it cannot show is a real
//! phone driver: Adreno, Mali and PowerVR are what the emulator and a device run are for.
//!
//! The comparison uses the limits of the phone QA (`scripts/ci/mobile_qa.py`: a pixel differs when a channel is off by
//! more than 24 of 255; the pictures agree below 3 % differing pixels and a mean difference below 2): the same
//! words that judge the emulator's two pictures judge these. The reason they are not tighter is the one the QA states:
//! the two renderers rasterise text and curves with different coverage, and WebRender draws grey-scale text where the
//! GLES stack has no dual-source blending.
//!
//! With no OpenGL ES on the machine a test says so on stderr and returns; with `AZ_REQUIRE_GLES` set it fails instead.
//! The runner script sets it: a run that skipped is not a pass.

use std::{
    rc::Rc,
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    glconst,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{render, AzulPixmap, RenderOptions},
    glyph_cache::GlyphCache,
    window::LayoutWindow,
    window_state::{FullWindowState, WindowCreateOptions},
};
use gl_context_loader::GenericGlContext;
use rust_fontconfig::FcFontCache;
use webrender::render_api::Transaction;

use super::{
    egl_gles::{DisplayKind, EglApi, EglSession, GlesFunctions},
    egl_pure::EGL_PBUFFER_BIT,
    gpu_renderer::{init_webrender, GpuParts, InitParams},
    gpu_select::FailPlan,
};
use crate::desktop::wr_translate2::{generate_frame, wr_translate_document_id, Notifier};

/// A pixel differs when a channel is off by more than this (of 255): `PIXEL_TOLERANCE` of the phone QA.
const PIXEL_TOLERANCE: u8 = 24;
/// The pictures agree below this share of differing pixels: `MAX_DIFFERING_SHARE` of the phone QA.
const MAX_DIFFERING_SHARE: f64 = 0.03;
/// ... and below this mean difference per channel (of 255): `MAX_MEAN_DIFF` of the phone QA.
const MAX_MEAN_DIFF: f64 = 2.0;

/// The variable that turns "no OpenGL ES here" from a skip into a failure.
pub(super) const REQUIRE: &str = "AZ_REQUIRE_GLES";

/// An OpenGL ES 3 context on a pbuffer-less surfaceless display, current on this thread.
struct OffscreenGles {
    session: EglSession,
    functions: GlesFunctions,
}

impl OffscreenGles {
    fn open() -> Result<Self, String> {
        let api = EglApi::load(&["libEGL.so.1", "libEGL.so"]).map_err(|e| e.to_string())?;
        let mut session = EglSession::open(api, DisplayKind::SurfacelessMesa, &[EGL_PBUFFER_BIT])
            .map_err(|e| e.to_string())?;
        session.bind_without_window().map_err(|e| e.to_string())?;
        let functions = GlesFunctions::load(&session, &["libGLESv2.so.2", "libGLESv2.so"])?;
        Ok(Self { session, functions })
    }

    fn gl(&self) -> Rc<GenericGlContext> {
        self.functions.context.clone()
    }
}

/// The context of a test, or `None` with the reason on stderr: a machine with no OpenGL ES 3 does not run the test,
/// and does not pretend that it did. With `AZ_REQUIRE_GLES` set the missing context is a failure.
fn gles_or_skip(test: &str) -> Option<OffscreenGles> {
    match OffscreenGles::open() {
        Ok(gles) => Some(gles),
        Err(reason) => {
            assert!(
                std::env::var_os(REQUIRE).is_none(),
                "{test}: {REQUIRE} is set and there is no OpenGL ES 3 context here: {reason}"
            );
            eprintln!(
                "SKIPPED {test}: no OpenGL ES 3 context on this machine ({reason}); set {REQUIRE}=1 to make this a failure"
            );
            None
        }
    }
}

/// A colour and a depth buffer WebRender draws into: it takes the framebuffer bound when a frame begins as its own.
struct Framebuffer {
    gl: Rc<GenericGlContext>,
    fbo: u32,
    color: u32,
    depth: u32,
}

impl Framebuffer {
    fn new(gl: Rc<GenericGlContext>, width: i32, height: i32) -> Result<Self, String> {
        let fbo = *gl.gen_framebuffers(1).first().ok_or("no framebuffer")?;
        let buffers = gl.gen_renderbuffers(2);
        let (color, depth) = (
            *buffers.first().ok_or("no renderbuffer")?,
            *buffers.get(1).ok_or("no second renderbuffer")?,
        );
        gl.bind_framebuffer(glconst::FRAMEBUFFER, fbo);
        gl.bind_renderbuffer(glconst::RENDERBUFFER, color);
        gl.renderbuffer_storage(glconst::RENDERBUFFER, glconst::RGBA8, width, height);
        gl.framebuffer_renderbuffer(
            glconst::FRAMEBUFFER,
            glconst::COLOR_ATTACHMENT0,
            glconst::RENDERBUFFER,
            color,
        );
        gl.bind_renderbuffer(glconst::RENDERBUFFER, depth);
        gl.renderbuffer_storage(glconst::RENDERBUFFER, glconst::DEPTH_COMPONENT24, width, height);
        gl.framebuffer_renderbuffer(
            glconst::FRAMEBUFFER,
            glconst::DEPTH_ATTACHMENT,
            glconst::RENDERBUFFER,
            depth,
        );
        let status = gl.check_frame_buffer_status(glconst::FRAMEBUFFER);
        let me = Self { gl, fbo, color, depth };
        if status == glconst::FRAMEBUFFER_COMPLETE {
            Ok(me)
        } else {
            Err(format!("the framebuffer is incomplete (0x{status:x})"))
        }
    }

    fn bind(&self) {
        self.gl.bind_framebuffer(glconst::FRAMEBUFFER, self.fbo);
    }

    /// The framebuffer's pixels, top row first, as RGBA.
    fn read(&self, width: u32, height: u32) -> Vec<u8> {
        self.bind();
        self.gl.finish();
        let row = width as usize * 4;
        // `read_pixels_into_buffer` writes into the buffer it is given; `read_pixels` sizes its own at one byte a pixel.
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
        bottom_up.chunks_exact(row).rev().flatten().copied().collect()
    }
}

impl Drop for Framebuffer {
    fn drop(&mut self) {
        self.gl.bind_framebuffer(glconst::FRAMEBUFFER, 0);
        self.gl.delete_renderbuffers(&[self.color, self.depth]);
        self.gl.delete_framebuffers(&[self.fbo]);
    }
}

/// A window the GPU draws, frame after frame: the layout window and WebRender over the OpenGL ES context, every frame read
/// back from the offscreen framebuffer.
struct GpuSession<'a> {
    window: LayoutWindow,
    parts: Option<GpuParts>,
    framebuffer: Option<Framebuffer>,
    new_frame_ready: Arc<(Mutex<bool>, Condvar)>,
    size_px: (u32, u32),
    width: f32,
    height: f32,
    dpi: f32,
    gles: &'a OffscreenGles,
}

impl<'a> GpuSession<'a> {
    fn new(gles: &'a OffscreenGles, width: f32, height: f32, dpi: f32) -> Result<Self, String> {
        let size_px = ((width * dpi) as u32, (height * dpi) as u32);
        let gl = gles.gl();
        let framebuffer = Framebuffer::new(gl.clone(), size_px.0 as i32, size_px.1 as i32)?;
        let new_frame_ready = Arc::new((Mutex::new(false), Condvar::new()));
        let notifier = Notifier {
            new_frame_ready: new_frame_ready.clone(),
            wake: None,
        };
        let mut options = WindowCreateOptions::default();
        options.window_state.background_color =
            azul_css::props::basic::OptionColorU::Some(azul_css::props::basic::ColorU::WHITE);
        let params = InitParams {
            notifier,
            damage: None,
            framebuffer_px: size_px,
            shader_cache_dir: None,
        };
        let parts = init_webrender(&gl, &options, params, &mut FailPlan::default())
            .map_err(|e| format!("the bring-up failed at {}: {}", e.stage.name(), e.cause))?;
        let mut window = LayoutWindow::new(FcFontCache::build()).map_err(|e| format!("{e:?}"))?;
        window.document_id = parts.document_id;
        window.id_namespace = parts.id_namespace;
        let mut state = FullWindowState::default();
        state.size.dimensions = LogicalSize::new(width, height);
        state.size.dpi = (96.0 * dpi) as u32;
        window.current_window_state = state;
        Ok(Self {
            window,
            parts: Some(parts),
            framebuffer: Some(framebuffer),
            new_frame_ready,
            size_px,
            width,
            height,
            dpi,
            gles,
        })
    }

    fn lay_out(&mut self, dom: Dom) -> Result<(), String> {
        let state = self.window.current_window_state.clone();
        self.window
            .layout_and_generate_display_list(
                StyledDom::create_from_dom(dom),
                &state,
                &RendererResources::default(),
                &ExternalSystemCallbacks::rust_internal(),
                &mut None,
            )
            .map_err(|e| format!("layout failed: {e:?}"))
    }

    /// The next frame, the way a shell makes it after a layout: the display lists are sent, the scene is built, the frame
    /// is drawn and read back.
    fn present(&mut self) -> Result<AzulPixmap, String> {
        std::thread::sleep(Duration::from_millis(30));
        *self.new_frame_ready.0.lock().map_err(|e| e.to_string())? = false;
        let parts = self.parts.as_mut().ok_or("the renderer is gone")?;
        let mut txn = Transaction::new();
        generate_frame(
            &mut txn,
            &mut self.window,
            &mut parts.render_api,
            true,
            &parts.gl_context_ptr,
        );
        parts
            .render_api
            .send_transaction(wr_translate_document_id(self.window.document_id), txn);
        parts.render_api.flush_scene_builder();
        {
            let (ready, signal) = &*self.new_frame_ready;
            let mut ready = ready.lock().map_err(|e| e.to_string())?;
            while !*ready {
                let (next, timeout) = signal
                    .wait_timeout(ready, Duration::from_secs(60))
                    .map_err(|e| e.to_string())?;
                ready = next;
                if timeout.timed_out() {
                    return Err("WebRender built no frame in 60 s".to_owned());
                }
            }
        }
        std::thread::sleep(Duration::from_millis(30));
        let framebuffer = self.framebuffer.as_ref().ok_or("no framebuffer")?;
        framebuffer.bind();
        let renderer = &mut parts.renderer;
        renderer.update();
        let size = webrender::api::units::DeviceIntSize::new(self.size_px.0 as i32, self.size_px.1 as i32);
        renderer
            .render(size, 0)
            .map_err(|errors| format!("the frame did not render: {errors:?}"))?;
        let mut pixels =
            AzulPixmap::new(self.size_px.0, self.size_px.1).ok_or("a pixmap of that size")?;
        pixels
            .data_mut()
            .copy_from_slice(&framebuffer.read(self.size_px.0, self.size_px.1));
        Ok(pixels)
    }

    /// The CPU renderer's picture of the window as it is laid out now.
    fn cpu(&self) -> Result<AzulPixmap, String> {
        let display_list = self
            .window
            .get_layout_result(&DomId::ROOT_ID)
            .ok_or("no layout result")?
            .display_list
            .clone();
        render(
            &display_list,
            &self.window.renderer_resources,
            &self.window.font_manager,
            RenderOptions {
                width: self.width,
                height: self.height,
                dpi_factor: self.dpi,
            },
            &mut GlyphCache::new(),
        )
        .map_err(|e| format!("the CPU render failed: {e:?}"))
    }
}

impl Drop for GpuSession<'_> {
    fn drop(&mut self) {
        // The GL objects go while their context is current (it is: the test's `OffscreenGles` outlives this).
        drop(self.framebuffer.take());
        if let Some(parts) = self.parts.take() {
            let GpuParts {
                renderer,
                render_api,
                gl_context_ptr,
                ..
            } = parts;
            drop(render_api);
            drop(gl_context_ptr);
            renderer.deinit();
        }
        let _ = self.gles;
    }
}

/// What differs between two pictures, with the limits of the phone QA.
#[derive(Debug)]
struct Difference {
    /// The share of pixels with a channel off by more than [`PIXEL_TOLERANCE`].
    share: f64,
    /// The mean absolute difference per colour channel (of 255).
    mean_abs: f64,
    /// The largest difference of any channel.
    max_abs: u8,
    /// The box `(x0, y0, x1, y1)` of the differing pixels.
    bbox: Option<(u32, u32, u32, u32)>,
}

fn difference(a: &AzulPixmap, b: &AzulPixmap) -> Difference {
    assert_eq!((a.width, a.height), (b.width, b.height), "the pictures have one size");
    let (mut differing, mut total, mut max_abs) = (0_u64, 0_u64, 0_u8);
    let mut bbox: Option<(u32, u32, u32, u32)> = None;
    for y in 0..a.height {
        for x in 0..a.width {
            let i = ((y * a.width + x) * 4) as usize;
            let mut worst = 0_u8;
            for c in 0..3 {
                let d = a.data()[i + c].abs_diff(b.data()[i + c]);
                total += u64::from(d);
                worst = worst.max(d);
            }
            max_abs = max_abs.max(worst);
            if worst > PIXEL_TOLERANCE {
                differing += 1;
                bbox = Some(match bbox {
                    Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
                    None => (x, y, x, y),
                });
            }
        }
    }
    let pixels = f64::from(a.width) * f64::from(a.height);
    Difference {
        share: differing as f64 / pixels,
        mean_abs: total as f64 / (pixels * 3.0),
        max_abs,
        bbox,
    }
}

/// Writes both frames into `$AZ_GPU_PIXELS_DIR` when it is set, to look at what a failing test saw.
fn keep(name: &str, gpu: &AzulPixmap, cpu: &AzulPixmap) {
    let Some(dir) = std::env::var_os("AZ_GPU_PIXELS_DIR") else {
        return;
    };
    for (backend, pixmap) in [("gles", gpu), ("cpu", cpu)] {
        if let Ok(png) = pixmap.encode_png() {
            let _ = std::fs::write(
                std::path::Path::new(&dir).join(format!("{name}-{backend}.png")),
                png,
            );
        }
    }
}

/// Fails with the numbers when the two pictures are not the same page within the QA's limits.
pub(super) fn assert_same_picture(name: &str, gpu: &AzulPixmap, cpu: &AzulPixmap) {
    keep(name, gpu, cpu);
    let d = difference(gpu, cpu);
    eprintln!(
        "{name}: {:.3} % of the pixels differ by more than {PIXEL_TOLERANCE}/255, mean {:.3}/255, max {}, box {:?}",
        d.share * 100.0,
        d.mean_abs,
        d.max_abs,
        d.bbox
    );
    assert!(
        d.share < MAX_DIFFERING_SHARE && d.mean_abs < MAX_MEAN_DIFF,
        "{name}: the OpenGL ES picture and the CPU picture differ: {d:?} (limits {} % and {MAX_MEAN_DIFF})",
        MAX_DIFFERING_SHARE * 100.0
    );
}

/// Three squares, a bordered box with round corners, a gradient and some text on white.
pub(super) fn page() -> Dom {
    let swatch = |colour: &str| {
        Dom::create_div().with_css(format!("width: 60px; height: 60px; background: {colour};").as_str())
    };
    let row = Dom::create_div()
        .with_css("display: flex; gap: 10px; padding: 10px;")
        .with_child(swatch("#d32f2f"))
        .with_child(swatch("#388e3c"))
        .with_child(swatch("#1976d2"))
        .with_child(
            Dom::create_div().with_css(
                "width: 100px; height: 60px; border: 4px solid #6a1b9a; border-radius: 12px; background: #ffe082;",
            ),
        )
        .with_child(
            Dom::create_div().with_css(
                "width: 100px; height: 60px; background: linear-gradient(to right, #ff9800, #3f51b5);",
            ),
        );
    Dom::create_body()
        .with_css("margin: 0px; background: #ffffff;")
        .with_child(row)
        .with_child(
            Dom::create_div_with_text("OpenGL ES, drawn by WebRender")
                .with_css("font-size: 24px; color: #212121; padding: 10px;"),
        )
        .with_child(
            Dom::create_div_with_text("The same page, drawn on the CPU, must look like it.")
                .with_css("font-size: 14px; color: #424242; padding: 10px;"),
        )
}

/// The page WebRender draws over OpenGL ES 3 is the page the CPU renderer draws: every shader of the first frame compiled
/// as `#version 300 es`, the depth cleared through `glClearBufferfv`, the buffers and textures of the ES device layer.
#[test]
fn webrender_over_gles_draws_the_page_the_cpu_renderer_draws() {
    let Some(gles) = gles_or_skip("webrender_over_gles_draws_the_page_the_cpu_renderer_draws") else {
        return;
    };
    let mut session = GpuSession::new(&gles, 400.0, 200.0, 2.0).expect("a session");
    session.lay_out(page()).expect("laid out");
    let gpu = session.present().expect("a frame");
    let cpu = session.cpu().expect("the CPU frame");
    assert_same_picture("page", &gpu, &cpu);

    // The page is not blank, and it is not the white canvas under it: the squares are where they were laid out.
    let at = |x: u32, y: u32| {
        let i = ((y * gpu.width + x) * 4) as usize;
        [gpu.data()[i], gpu.data()[i + 1], gpu.data()[i + 2]]
    };
    let red = at(2 * (10 + 30), 2 * (10 + 30));
    assert!(red[0] > 180 && red[1] < 90 && red[2] < 90, "the first square is red, not {red:?}");
}

/// A second frame of another page leaves nothing of the first: the render targets are cleared (colour and depth) at the
/// start of every frame, and the opaque pass is not rejected by the depth of the last one.
#[test]
fn a_second_frame_shows_its_own_page_and_nothing_of_the_first() {
    let Some(gles) = gles_or_skip("a_second_frame_shows_its_own_page_and_nothing_of_the_first") else {
        return;
    };
    let mut session = GpuSession::new(&gles, 200.0, 100.0, 2.0).expect("a session");
    session
        .lay_out(
            Dom::create_body()
                .with_css("margin: 0px; background: #d32f2f;")
                .with_child(Dom::create_div().with_css("width: 200px; height: 100px; background: #d32f2f;")),
        )
        .expect("laid out");
    let first = session.present().expect("the first frame");
    let first_cpu = session.cpu().expect("the CPU frame");
    assert_same_picture("second-first", &first, &first_cpu);

    session
        .lay_out(
            Dom::create_body()
                .with_css("margin: 0px; background: #ffffff;")
                .with_child(Dom::create_div().with_css("width: 80px; height: 40px; background: #1976d2;")),
        )
        .expect("laid out");
    let second = session.present().expect("the second frame");
    let second_cpu = session.cpu().expect("the CPU frame");
    assert_same_picture("second-second", &second, &second_cpu);

    // The corner the blue square does not cover is white, not the red of the frame before.
    let i = (((second.height - 4) * second.width + second.width - 4) * 4) as usize;
    let corner = [second.data()[i], second.data()[i + 1], second.data()[i + 2]];
    assert!(corner.iter().all(|c| *c > 240), "the corner of the second frame is white, not {corner:?}");
}

/// A stage that is made to fail (`AZ_GPU_FAIL`) ends the bring-up with that stage and leaves nothing behind: a bring-up
/// made after each failure, on the same context, works.
#[test]
fn a_failure_at_each_stage_of_the_bring_up_names_it_and_leaves_the_context_usable() {
    let Some(gles) = gles_or_skip("a_failure_at_each_stage_of_the_bring_up_names_it") else {
        return;
    };
    let gl = gles.gl();
    let attempt = |fail_at: &str| {
        let notifier = Notifier {
            new_frame_ready: Arc::new((Mutex::new(false), Condvar::new())),
            wake: None,
        };
        let params = InitParams {
            notifier,
            damage: None,
            framebuffer_px: (64, 64),
            shader_cache_dir: None,
        };
        init_webrender(
            &gl,
            &WindowCreateOptions::default(),
            params,
            &mut FailPlan::parse(Some(fail_at)),
        )
    };
    for stage in ["probe", "renderer", "document", "shaders"] {
        let failure = attempt(stage).expect_err("the stage was made to fail");
        assert_eq!(failure.stage.name(), stage);
        assert!(failure.cause.contains("AZ_GPU_FAIL"), "{failure:?}");
    }
    // Nothing failed on purpose: the same context brings WebRender up after four failed attempts.
    let parts = attempt("").expect("the bring-up after the failures");
    let GpuParts { renderer, render_api, gl_context_ptr, .. } = parts;
    drop(render_api);
    drop(gl_context_ptr);
    renderer.deinit();
}

/// The probe reads what Mesa answers, and the bring-up keeps what the driver is: an OpenGL ES context of 3.0 or later, a
/// renderer string, a class.
#[test]
fn the_bring_up_reports_the_driver_it_ran_on() {
    let Some(gles) = gles_or_skip("the_bring_up_reports_the_driver_it_ran_on") else {
        return;
    };
    let gl = gles.gl();
    let notifier = Notifier {
        new_frame_ready: Arc::new((Mutex::new(false), Condvar::new())),
        wake: None,
    };
    let params = InitParams {
        notifier,
        damage: None,
        framebuffer_px: (64, 64),
        shader_cache_dir: None,
    };
    let parts = init_webrender(
        &gl,
        &WindowCreateOptions::default(),
        params,
        &mut FailPlan::default(),
    )
    .expect("the bring-up");
    eprintln!("driver: {:?}", parts.info);
    assert!(parts.gles.major >= 3, "{:?}", parts.gles);
    assert!(!parts.info.renderer.is_empty() && !parts.info.version.is_empty());
    // No directory was given: no cache was kept and the status says why.
    assert!(parts.shader_cache.dir.is_none() || parts.shader_cache.loaded == 0);
    let GpuParts { renderer, render_api, gl_context_ptr, .. } = parts;
    drop(render_api);
    drop(gl_context_ptr);
    renderer.deinit();
}
