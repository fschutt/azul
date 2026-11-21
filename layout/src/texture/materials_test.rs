//! What a window's material renderer owns, and that nothing of it is shared:
//! the render count, the render cache, the grain tiles, the worker thread and
//! the wallpapers belong to one renderer, so a window - or a test - sees only
//! what it drew itself.

use azul_core::{
    dom::Dom, geom::LogicalSize, resources::RendererResources, styled_dom::StyledDom,
};
use rust_fontconfig::FcFontCache;

use super::*;
use crate::{callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState};

/// A window holding one leather box, its first frame prepared.
fn window_with_leather() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    let dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_div().with_css("width: 200px; height: 60px; background: builtin(leather);"),
    );
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the box lays out");
    lw.prepare_frame_content();
    lw
}

/// A wallpaper of one grey over the screen a window is taken to be on.
fn a_wallpaper() -> crate::texture::environment::Wallpaper {
    crate::texture::environment::Wallpaper::from_rgba8(1, 1, &[90, 90, 90, 255], [0.0, 0.0, 1920.0, 1080.0])
        .expect("a wallpaper")
}

#[test]
fn a_renderer_counts_the_surfaces_it_draws_and_no_other_renderer_does() {
    let (mine, yours) = (MaterialRenderer::new(), MaterialRenderer::new());
    let params = MaterialParams::of(Material::Leather);
    let scene = SurfaceScene::default();
    assert_eq!((mine.renders(), yours.renders()), (0, 0));

    let first = mine.render_material(Material::Leather, 64, 24, 1.0, &params, &scene);
    assert_eq!((mine.renders(), yours.renders()), (1, 0), "the renderer that drew it counts it");
    let again = mine.render_material(Material::Leather, 64, 24, 1.0, &params, &scene);
    assert_eq!(mine.renders(), 1, "the same surface again is found in the render cache, not drawn");
    assert_eq!(first.get_hash(), again.get_hash(), "and is the same frame");

    let other = yours.render_material(Material::Leather, 64, 24, 1.0, &params, &scene);
    assert_eq!(
        (mine.renders(), yours.renders()),
        (1, 1),
        "another renderer does not find what this one drew: it draws its own"
    );
    assert_ne!(first.get_hash(), other.get_hash(), "so the two frames are two");
}

#[test]
fn a_renderer_draws_the_pixels_a_render_of_its_own_draws() {
    let params = MaterialParams::of(Material::Aluminium);
    let scene = SurfaceScene::default();
    let renderer = MaterialRenderer::new();
    // The second render finds its grain in the renderer's tiles.
    let first = renderer.render_pixels(Material::Aluminium, 80, 40, 2.0, &params, &scene).expect("a render");
    let kept = renderer.render_pixels(Material::Aluminium, 80, 40, 2.0, &params, &scene).expect("a render");
    let alone = render_material_pixels(Material::Aluminium, 80, 40, 2.0, &params, &scene).expect("a render");
    assert!(first == alone, "keeping the grain is a saving, not a different picture");
    assert!(kept == alone, "and the grain found again is the grain computed");
}

#[test]
fn the_grain_tiles_are_kept_by_the_cache_that_computed_them() {
    let (mine, yours) = (shade::GrainCache::default(), shade::GrainCache::default());
    let spec = shade::SurfaceSpec {
        material: Material::Leather,
        source: MaterialSource::Procedural,
        tile: None,
        grain: 1.7,
        scale: 1.0,
        relief: 1.0,
        dpr: 1.0,
    };
    let first = shade::grain_tiles(&mine, &spec, (0, 2), (0, 2));
    let again = shade::grain_tiles(&mine, &spec, (0, 2), (0, 2));
    assert!(
        first.iter().zip(&again).all(|(a, b)| Arc::ptr_eq(a, b)),
        "the tiles a cache holds are found again, not computed again"
    );
    let other = shade::grain_tiles(&yours, &spec, (0, 2), (0, 2));
    assert!(
        first.iter().zip(&other).all(|(a, b)| !Arc::ptr_eq(a, b) && a == b),
        "another cache computes its own tiles, and they are the same grain"
    );
}

#[test]
fn a_metal_reflects_the_wallpapers_of_the_desktop_it_is_asked_of() {
    let desktop = DesktopWallpapers::new();
    desktop.set_wallpapers(vec![a_wallpaper()]);
    let extras = RenderImageCallbackExtras::new(MaterialLight::DEFAULT);
    let scene_of = |desktop: &DesktopWallpapers, material: Material| {
        let params = MaterialParams::of(material);
        scene_from_extras(desktop, material, Some(&extras), &params, [100.0, 40.0], 1.0)
    };
    assert!(
        matches!(scene_of(&desktop, Material::Aluminium).environment, Environment::Wallpapers(_)),
        "a metal reflects the wallpaper its desktop knows"
    );
    assert!(
        matches!(scene_of(&desktop, Material::Leather).environment, Environment::Highlight(_)),
        "what is no metal reflects the highlight colour only"
    );
    assert!(
        matches!(scene_of(&DesktopWallpapers::new(), Material::Aluminium).environment, Environment::Highlight(_)),
        "another desktop has no wallpaper of this one's"
    );
}

#[test]
fn the_worker_of_a_renderer_ends_with_its_last_handle() {
    let renderer = MaterialRenderer::new();
    let handle = renderer.clone();
    let shared = Arc::clone(&renderer.inner.shared);
    let params = MaterialParams::of(Material::Leather);
    let scene = SurfaceScene::default();

    let queued = shared.render_deferred(7, Material::Leather, [16, 16], 1.0, &params, &scene);
    assert!(queued.is_none(), "the surface is queued for the worker, not drawn here");
    renderer.wait_for_deferred_renders();
    assert!(
        shared.render_deferred(7, Material::Leather, [16, 16], 1.0, &params, &scene).is_some(),
        "the worker's frame is in the render cache"
    );
    assert_eq!(renderer.renders(), 1, "the surface was drawn once");
    let worker = shared
        .deferred
        .lock()
        .expect("the queue's lock")
        .worker
        .take()
        .expect("the first surface started the worker");

    drop(renderer);
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(!worker.is_finished(), "a handle is left: the worker waits for the next surface");

    drop(handle);
    let started = std::time::Instant::now();
    while !worker.is_finished() && started.elapsed() < std::time::Duration::from_secs(20) {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(worker.is_finished(), "the last handle is dropped: the worker ends");
    worker.join().expect("the worker ends without a panic");
}

#[test]
fn a_window_draws_and_counts_its_own_materials() {
    let (a, b) = (window_with_leather(), window_with_leather());
    assert_eq!(a.material_renderer.renders(), 1, "the first window drew its leather");
    assert_eq!(
        b.material_renderer.renders(),
        1,
        "the second window drew its own, it did not find the first's"
    );
}

#[test]
fn a_window_reflects_the_wallpapers_it_knows_and_no_other_window_does() {
    let (a, b) = (window_with_leather(), window_with_leather());
    assert_eq!(a.material_renderer.desktop_generation(), 0, "no wallpaper is known yet");
    a.material_renderer.desktop().set_wallpapers(vec![a_wallpaper()]);
    assert_eq!(a.material_renderer.desktop_generation(), 1);
    assert!(
        a.materials_follow_window_position(),
        "a window that knows a wallpaper relights its metals as it moves"
    );
    assert_eq!(b.material_renderer.desktop_generation(), 0, "the other window knows none");
}
