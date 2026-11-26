//! The lifecycle executor of the GPU path (`gles_gpu`) against real EGL and real WebRender on a host: Mesa's surfaceless
//! EGL with a pbuffer where the phone's window surface would be, a real `CommonWindowState` (a headless window's), the
//! events the Android shell sends and the pictures that come out, held against the CPU renderer's.
//!
//! What this shows that the reducer's own tests (a fake platform) cannot: that the actions, performed on a real
//! EGL, leave the context, the surface and the renderer where the reducer believes them, that the renderer survives a
//! lost window surface and is rebuilt after a lost context, that a failure at each stage ends in the CPU renderer with
//! nothing left behind, and what `gpu` and `auto` do with the software renderer a host (and the emulator) has.
//!
//! No OpenGL ES on the machine is a skip with a line on stderr, a failure when `AZ_REQUIRE_GLES` is set.

use std::{cell::RefCell, sync::Arc};

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    glconst,
    icon::{IconProviderHandle, SharedIconProvider},
    refany::RefAny,
    resources::{AppConfig, RendererResources},
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{render, AzulPixmap, RenderOptions},
    glyph_cache::GlyphCache,
    window_state::{FullWindowState, WindowCreateOptions},
};
use rust_fontconfig::FcFontCache;

use super::{
    event::{CommonWindowState, SharedUndoManager},
    gles_gpu::{GlesGpu, GlesGpuConfig, GpuCtx, Outcome},
    gles_host_test::{assert_same_picture, page, REQUIRE},
    gpu_lifecycle::{Phase, Surface, TrimLevel},
    gpu_renderer,
    gpu_select::{FailPlan, GpuRequest, GpuStage, Ran},
};
use crate::desktop::shell2::headless::HeadlessWindow;

/// The window's size in CSS pixels and the density of the screen.
const WIDTH: f32 = 400.0;
const HEIGHT: f32 = 200.0;
const DPI: f32 = 2.0;

/// A window of the engine with no renderer: the layout window and the common state a GL window keeps its renderer in.
fn headless() -> HeadlessWindow {
    HeadlessWindow::new(
        WindowCreateOptions::default(),
        Arc::new(RefCell::new(RefAny::new(()))),
        SharedUndoManager::new(),
        AppConfig::default(),
        SharedIconProvider::from_handle(IconProviderHandle::default()),
        Arc::new(FcFontCache::build()),
        None,
    )
    .expect("a headless window")
}

fn options() -> WindowCreateOptions {
    let mut options = WindowCreateOptions::default();
    options.window_state.background_color =
        azul_css::props::basic::OptionColorU::Some(azul_css::props::basic::ColorU::WHITE);
    options
}

fn gpu(request: GpuRequest, plan: &str) -> GlesGpu {
    GlesGpu::new(
        GlesGpuConfig::host_offscreen(),
        request,
        FailPlan::parse(Some(plan)),
        options(),
        None,
        None,
    )
}

/// The surface of the window in device pixels.
fn surface_at(scale: f32) -> Surface {
    Surface::new((WIDTH * scale) as u32, (HEIGHT * scale) as u32, scale)
}

fn lay_out(common: &mut CommonWindowState, dom: Dom, scale: f32) {
    let window = common.layout_window.as_mut().expect("a layout window");
    let mut state = FullWindowState::default();
    state.size.dimensions = LogicalSize::new(WIDTH, HEIGHT);
    state.size.dpi = (96.0 * scale) as u32;
    window.current_window_state = state.clone();
    window
        .layout_and_generate_display_list(
            StyledDom::create_from_dom(dom),
            &state,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut None,
        )
        .expect("laid out");
}

/// The CPU renderer's picture of the window as it is laid out.
fn cpu_picture(common: &CommonWindowState, scale: f32) -> AzulPixmap {
    let window = common.layout_window.as_ref().expect("a layout window");
    let display_list = window
        .get_layout_result(&DomId::ROOT_ID)
        .expect("a layout result")
        .display_list
        .clone();
    render(
        &display_list,
        &window.renderer_resources,
        &window.font_manager,
        RenderOptions {
            width: WIDTH,
            height: HEIGHT,
            dpi_factor: scale,
        },
        &mut GlyphCache::new(),
    )
    .expect("the CPU frame")
}

/// What the GPU drew into the window's surface, top row first.
fn read_back(gpu: &GlesGpu, surface: Surface) -> AzulPixmap {
    let gl = gpu.gl().expect("a GL context");
    gl.bind_framebuffer(glconst::FRAMEBUFFER, 0);
    gl.finish();
    let row = surface.width as usize * 4;
    let mut bottom_up = vec![0; row * surface.height as usize];
    gl.read_pixels_into_buffer(
        0,
        0,
        surface.width as i32,
        surface.height as i32,
        glconst::RGBA,
        glconst::UNSIGNED_BYTE,
        &mut bottom_up,
    );
    let flipped: Vec<u8> = bottom_up.chunks_exact(row).rev().flatten().copied().collect();
    let mut pixmap = AzulPixmap::new(surface.width, surface.height).expect("a pixmap");
    pixmap.data_mut().copy_from_slice(&flipped);
    pixmap
}

/// Whether the outcome says this machine has no OpenGL ES (the library or the context could not be made): a skip, with
/// the reason on stderr, or a failure with `AZ_REQUIRE_GLES`.
fn no_gles_here(test: &str, gpu: &GlesGpu, outcome: &Outcome) -> bool {
    let Outcome::FellBack(fallback) = outcome else {
        return false;
    };
    if !matches!(fallback.stage, GpuStage::Library | GpuStage::Context | GpuStage::Functions) {
        return false;
    }
    assert!(
        std::env::var_os(REQUIRE).is_none(),
        "{test}: {REQUIRE} is set and the GPU path fell back at {}: {} ({:?})",
        fallback.stage.name(),
        fallback.cause,
        gpu.decision()
    );
    eprintln!(
        "SKIPPED {test}: no OpenGL ES 3 context on this machine (stage {}: {}); set {REQUIRE}=1 to make this a failure",
        fallback.stage.name(),
        fallback.cause
    );
    true
}

/// Brings the GPU path up on a surface; `None` when the machine has no OpenGL ES (skipped).
fn up(
    test: &str,
    window: &mut HeadlessWindow,
    request: GpuRequest,
    plan: &str,
) -> Option<(GlesGpu, Surface)> {
    let mut gpu = gpu(request, plan);
    let surface = surface_at(DPI);
    let mut ctx = GpuCtx {
        common: &mut window.common,
        native_window: None,
    };
    let outcome = gpu.surface_created(&mut ctx, surface);
    if no_gles_here(test, &gpu, &outcome) {
        return None;
    }
    assert_eq!(outcome, Outcome::Gpu, "{test}: {:?}", gpu.decision());
    Some((gpu, surface))
}

/// Draws the laid-out window on the GPU: the frame to WebRender, then the frame out. The picture, top row first.
fn draw(gpu: &mut GlesGpu, window: &mut HeadlessWindow, surface: Surface) -> AzulPixmap {
    // A frame the renderer built before this transaction does not count: its flag is cleared first.
    let _ = gpu.take_frame_ready();
    gpu_renderer::submit_full_frame(&mut window.common);
    // WebRender builds the frame on its own thread and says so through its notifier: the loop presents then.
    assert!(
        gpu.wait_frame_ready(std::time::Duration::from_secs(60)),
        "WebRender built no frame in 60 s"
    );
    let mut ctx = GpuCtx {
        common: &mut window.common,
        native_window: None,
    };
    assert_eq!(gpu.present(&mut ctx), Outcome::Gpu);
    read_back(gpu, surface)
}

/// The first surface brings the context, the surface and the renderer up in the reducer's order, the first frame is the
/// page the CPU renderer draws, and the line the QA harness reads says `gpu`.
#[test]
fn the_first_surface_brings_the_path_up_and_draws_the_cpu_page() {
    let test = "the_first_surface_brings_the_path_up_and_draws_the_cpu_page";
    let mut window = headless();
    let Some((mut gpu, surface)) = up(test, &mut window, GpuRequest::Gpu, "") else {
        return;
    };
    assert_eq!(gpu.phase(), Phase::Ready);
    assert!(gpu.has_context() && gpu.is_running());
    assert_eq!(gpu.renderers_made(), 1);
    let decision = gpu.decision().expect("a decision");
    assert_eq!(decision.ran, Ran::Gpu);
    let line = decision.render_line();
    assert!(line.starts_with("[render] backend=gpu requested=gpu device="), "{line}");
    assert!(decision.fallback_line().is_none());
    // A renderer that was just made holds no frame: a present now would swap an empty buffer, so the shell waits for the first one.
    assert!(!gpu.has_frame(), "nothing was built yet");

    lay_out(&mut window.common, page(), DPI);
    let picture = draw(&mut gpu, &mut window, surface);
    assert!(gpu.has_frame(), "WebRender announced the frame that was drawn");
    assert_same_picture("executor-page", &picture, &cpu_picture(&window.common, DPI));

    let mut ctx = GpuCtx { common: &mut window.common, native_window: None };
    assert_eq!(gpu.shutdown(&mut ctx), Outcome::Gpu);
}

/// A rotation: the surface goes and a surface of another size comes. The context and the renderer stay (the glyph
/// atlas and the compiled programs with them), and the frame at the new size is the page the CPU draws.
#[test]
fn a_new_surface_keeps_the_renderer_and_draws_at_the_new_size() {
    let test = "a_new_surface_keeps_the_renderer_and_draws_at_the_new_size";
    let mut window = headless();
    let Some((mut gpu, first)) = up(test, &mut window, GpuRequest::Gpu, "") else {
        return;
    };
    lay_out(&mut window.common, page(), DPI);
    let _ = draw(&mut gpu, &mut window, first);

    let mut ctx = GpuCtx { common: &mut window.common, native_window: None };
    assert_eq!(gpu.surface_destroyed(&mut ctx), Outcome::Gpu);
    assert!(gpu.has_context(), "the context outlives the surface");
    assert!(ctx.common.renderer.is_some(), "and so does the renderer");
    assert_eq!(gpu.phase(), Phase::Incomplete);

    // The same window at a lower density: another size in device pixels.
    let second = surface_at(1.0);
    assert_eq!(gpu.surface_created(&mut ctx, second), Outcome::Gpu);
    assert_eq!(gpu.renderers_made(), 1, "no second renderer was made");
    assert_eq!(gpu.phase(), Phase::Ready);

    lay_out(&mut window.common, page(), 1.0);
    let picture = draw(&mut gpu, &mut window, second);
    assert_eq!((picture.width, picture.height), (second.width, second.height));
    assert_same_picture("executor-rotated", &picture, &cpu_picture(&window.common, 1.0));
}

/// A lost context is rebuilt under the same window and the frame after it is right; a second loss with no frame between
/// ends the GPU path with `lost`.
#[test]
fn a_lost_context_is_rebuilt_and_a_second_one_without_a_frame_falls_back() {
    let test = "a_lost_context_is_rebuilt_and_a_second_one_without_a_frame_falls_back";
    let mut window = headless();
    let Some((mut gpu, surface)) = up(test, &mut window, GpuRequest::Gpu, "") else {
        return;
    };
    lay_out(&mut window.common, page(), DPI);
    let _ = draw(&mut gpu, &mut window, surface);

    let mut ctx = GpuCtx { common: &mut window.common, native_window: None };
    assert_eq!(gpu.context_lost(&mut ctx), Outcome::Gpu);
    assert_eq!(gpu.renderers_made(), 2, "a new renderer on a new context");
    assert!(ctx.common.renderer.is_some());
    assert!(!gpu.has_frame(), "the new renderer has built nothing yet");

    // The window's keys named the old renderer: its registries were cleared and the next frame sends everything.
    let picture = draw(&mut gpu, &mut window, surface);
    assert_same_picture("executor-rebuilt", &picture, &cpu_picture(&window.common, DPI));

    // A good frame forgave the first loss: one more is rebuilt again, two with no frame between are the end.
    let mut ctx = GpuCtx { common: &mut window.common, native_window: None };
    assert_eq!(gpu.context_lost(&mut ctx), Outcome::Gpu);
    match gpu.context_lost(&mut ctx) {
        Outcome::FellBack(fallback) => assert_eq!(fallback.stage, GpuStage::Lost),
        Outcome::Gpu => panic!("two losses without a frame between must end the GPU path"),
    }
    assert!(!gpu.is_running());
    assert!(ctx.common.renderer.is_none(), "nothing of the GPU is left in the window");
    assert!(!gpu.has_context());
    let line = gpu.decision().and_then(|d| d.fallback_line()).expect("the diagnostic");
    assert!(line.contains("lost"), "{line}");
}

/// A failure made on purpose at each stage ends in the CPU renderer with that stage named and nothing left behind: no
/// renderer in the window, no context, one diagnostic line and the contract line saying `cpu`.
#[test]
fn a_failure_at_each_stage_falls_back_naming_it_and_leaves_nothing() {
    let test = "a_failure_at_each_stage_falls_back_naming_it_and_leaves_nothing";
    for stage in [
        GpuStage::Library,
        GpuStage::Context,
        GpuStage::Surface,
        GpuStage::MakeCurrent,
        GpuStage::Functions,
        GpuStage::Probe,
        GpuStage::Renderer,
        GpuStage::Document,
        GpuStage::Shaders,
    ] {
        let mut window = headless();
        let mut gpu = gpu(GpuRequest::Gpu, stage.name());
        let mut ctx = GpuCtx { common: &mut window.common, native_window: None };
        let outcome = gpu.surface_created(&mut ctx, surface_at(DPI));
        let Outcome::FellBack(fallback) = outcome else {
            panic!("{test}: {} did not fall back", stage.name());
        };
        assert_eq!(fallback.stage, stage, "{test}");
        assert!(fallback.cause.contains("AZ_GPU_FAIL"), "{test}: {fallback:?}");
        assert!(!gpu.is_running(), "{test}: {}", stage.name());
        assert!(!gpu.has_context(), "{test}: a context is left after {}", stage.name());
        assert!(ctx.common.renderer.is_none(), "{test}: a renderer is left after {}", stage.name());
        assert!(ctx.common.render_api.is_none() && ctx.common.document_id.is_none());
        let decision = gpu.decision().expect("a decision");
        assert_eq!(decision.ran, Ran::Cpu);
        let diagnostic = decision.fallback_line().expect("one diagnostic line");
        assert!(diagnostic.contains(stage.name()), "{diagnostic}");
        assert!(
            decision.render_line().starts_with("[render] backend=cpu requested=gpu"),
            "{}",
            decision.render_line()
        );
        // The path is over: nothing more is asked of the machine.
        assert_eq!(gpu.present(&mut ctx), Outcome::Gpu);
        assert!(ctx.common.renderer.is_none());
    }
}

/// A heavy trim with no surface gives the renderer and the context back; the next surface makes both again and the
/// frame is right.
#[test]
fn a_heavy_trim_without_a_surface_gives_the_gpu_memory_back_and_the_next_surface_rebuilds() {
    let test = "a_heavy_trim_without_a_surface_gives_the_gpu_memory_back";
    let mut window = headless();
    let Some((mut gpu, surface)) = up(test, &mut window, GpuRequest::Gpu, "") else {
        return;
    };
    lay_out(&mut window.common, page(), DPI);
    let _ = draw(&mut gpu, &mut window, surface);

    let mut ctx = GpuCtx { common: &mut window.common, native_window: None };
    // With a surface a heavy trim is a light one: everything stays.
    assert_eq!(gpu.trim(&mut ctx, TrimLevel::Heavy), Outcome::Gpu);
    assert!(gpu.has_context() && ctx.common.renderer.is_some());

    assert_eq!(gpu.surface_destroyed(&mut ctx), Outcome::Gpu);
    assert_eq!(gpu.trim(&mut ctx, TrimLevel::Heavy), Outcome::Gpu);
    assert!(!gpu.has_context(), "the context was given back");
    assert!(ctx.common.renderer.is_none(), "and the renderer");

    assert_eq!(gpu.surface_created(&mut ctx, surface), Outcome::Gpu);
    assert_eq!(gpu.renderers_made(), 2);
    let picture = draw(&mut gpu, &mut window, surface);
    assert_same_picture("executor-after-trim", &picture, &cpu_picture(&window.common, DPI));
}

/// Closing the window leaves nothing: the surface, then the renderer, then the context.
#[test]
fn shutdown_leaves_nothing_of_the_gpu_path() {
    let test = "shutdown_leaves_nothing_of_the_gpu_path";
    let mut window = headless();
    let Some((mut gpu, surface)) = up(test, &mut window, GpuRequest::Gpu, "") else {
        return;
    };
    lay_out(&mut window.common, page(), DPI);
    let _ = draw(&mut gpu, &mut window, surface);
    let mut ctx = GpuCtx { common: &mut window.common, native_window: None };
    assert_eq!(gpu.shutdown(&mut ctx), Outcome::Gpu);
    assert_eq!(gpu.phase(), Phase::Closed);
    assert!(!gpu.has_context());
    assert!(ctx.common.renderer.is_none() && ctx.common.render_api.is_none());
    // Nothing more happens to a closed path.
    assert_eq!(gpu.present(&mut ctx), Outcome::Gpu);
}

/// `gpu` forces OpenGL ES on a software renderer (that is how the emulator's and the Simulator's GL is tested), `auto`
/// refuses it and says so: the host's llvmpipe is a software renderer.
#[test]
fn gpu_forces_the_software_renderer_and_auto_refuses_it_with_the_diagnostic() {
    let test = "gpu_forces_the_software_renderer_and_auto_refuses_it";
    let mut window = headless();
    let Some((gpu_forced, _)) = up(test, &mut window, GpuRequest::Gpu, "") else {
        return;
    };
    let software = gpu_forced
        .device()
        .expect("a driver")
        .info
        .renderer
        .to_lowercase()
        .contains("llvmpipe");
    drop(gpu_forced);
    if !software {
        eprintln!("SKIPPED {test}: the renderer here is not a software one");
        return;
    }
    let mut window = headless();
    let mut auto = gpu(GpuRequest::Auto, "");
    let mut ctx = GpuCtx { common: &mut window.common, native_window: None };
    let Outcome::FellBack(fallback) = auto.surface_created(&mut ctx, surface_at(DPI)) else {
        panic!("{test}: auto took a software renderer");
    };
    assert_eq!(fallback.stage, GpuStage::Probe);
    assert!(fallback.cause.contains("software"), "{fallback:?}");
    assert!(ctx.common.renderer.is_none() && !auto.has_context());
    let decision = auto.decision().expect("a decision");
    assert!(decision.render_line().starts_with("[render] backend=cpu requested=auto"));
    assert!(decision.fallback_line().expect("the diagnostic").contains("auto"));
}
