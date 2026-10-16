//! A material background is drawn when something it is drawn from changes -
//! its size in device pixels, its params (the tint a theme sets), the light,
//! the wallpaper - and at no other time: not frame after frame, and not when
//! the cursor moves over it. The cursor's sheen is a layer of its own over
//! the material, never a new frame of it.

use azul_core::{
    dom::{Dom, DomId},
    geom::{LogicalPosition, LogicalSize},
    resources::{ImageRef, RendererResources},
    styled_dom::StyledDom,
    window::CursorPosition,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    texture::materials::renders_on_this_thread, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// A pillowed leather box, as the ribbon's leather area is.
const LEATHER: &str = "width: 200px; height: 60px; background: builtin(leather); \
                       --azul-pillow: 76px .16;";

fn lay_out(lw: &mut LayoutWindow, css: &str, window: LogicalSize) {
    let dom = Dom::create_body()
        .with_css("margin: 0px;")
        .with_child(Dom::create_div().with_css(css));
    let mut ws = FullWindowState::default();
    ws.size.dimensions = window;
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the box lays out");
}

/// A 400 x 300 window holding one box with `css`, its first frame prepared.
fn window(css: &str) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lay_out(&mut lw, css, LogicalSize::new(400.0, 300.0));
    lw.prepare_frame_content();
    lw
}

/// The frame the display list draws for the material, and its width.
fn material_frame(lw: &LayoutWindow) -> (ImageRef, f32) {
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .display_list
        .items
        .iter()
        .find_map(|item| match item {
            DisplayListItem::Image { bounds, image, .. } => {
                Some((image.clone(), bounds.inner().size.width))
            }
            _ => None,
        })
        .expect("the material is drawn")
}

#[test]
fn a_material_at_rest_is_not_drawn_again_frame_after_frame() {
    let mut lw = window(LEATHER);
    let (drawn, _) = material_frame(&lw);
    assert!(!drawn.is_callback(), "harness: the first frame drew the leather");
    let before = renders_on_this_thread();
    for _ in 0..10 {
        lw.prepare_frame_content();
    }
    assert_eq!(
        renders_on_this_thread() - before,
        0,
        "ten frames at rest drew the leather again"
    );
    assert_eq!(material_frame(&lw).0.get_hash(), drawn.get_hash(), "the same frame");
}

#[test]
fn the_cursor_moving_over_a_material_does_not_draw_it_again() {
    let mut lw = window(LEATHER);
    let (drawn, _) = material_frame(&lw);
    let before = renders_on_this_thread();
    // The cursor crosses the leather in 7 px steps, a frame each: the sheen
    // eases in and follows it.
    for i in 0..24 {
        #[allow(clippy::cast_precision_loss)] // a small step count
        let x = 10.0 + 7.0 * i as f32;
        lw.current_window_state.mouse_state.cursor_position =
            CursorPosition::InWindow(LogicalPosition::new(x, 30.0));
        lw.step_material_sheen(0.016);
        lw.prepare_frame_content();
    }
    assert_eq!(
        renders_on_this_thread() - before,
        0,
        "the cursor crossed the leather in 24 frames, and each new cursor drew the whole surface \
         again"
    );
    assert_eq!(
        material_frame(&lw).0.get_hash(),
        drawn.get_hash(),
        "the display list draws the frame it drew at rest"
    );
}

#[test]
fn a_resized_material_is_drawn_once_for_its_new_size() {
    let half = "width: 50%; height: 60px; background: builtin(leather);";
    let mut lw = window(half);
    assert_eq!(material_frame(&lw).1, 200.0, "harness: half of 400");
    let before = renders_on_this_thread();
    // The window grows: the box with it, so the leather is drawn again for
    // its new size - once, however many frames follow.
    lay_out(&mut lw, half, LogicalSize::new(480.0, 300.0));
    for _ in 0..3 {
        lw.prepare_frame_content();
    }
    assert_eq!(renders_on_this_thread() - before, 1, "one resize, one new frame");
    let (frame, width) = material_frame(&lw);
    assert_eq!(width, 240.0);
    assert_eq!(
        frame.get_size(),
        LogicalSize::new(240.0, 60.0),
        "the frame is drawn for the new box"
    );
}

/// A window a desktop shell draws on the CPU does not wait for a large
/// surface: the shader runs off the UI thread, the layer shows its base
/// colour until the frame is ready, and the window keeps asking for frames
/// until then - then shows it, and asks for none.
#[test]
fn a_large_material_in_a_cpu_window_is_drawn_off_the_ui_thread() {
    let css = "width: 400px; height: 240px; background: builtin(leather);";
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.renderer_type = Some(azul_core::window::RendererType::Software);
    lay_out(&mut lw, css, LogicalSize::new(400.0, 300.0));
    let before = renders_on_this_thread();
    lw.prepare_frame_content();
    let (stand_in, _) = material_frame(&lw);
    assert!(
        stand_in.get_size().width < 400.0,
        "the first frame shows the base colour while the leather is drawn elsewhere, not a \
         frame drawn on the UI thread: {:?}",
        stand_in.get_size()
    );
    assert!(lw.needs_animation_frame(), "the window waits for the frame");

    let mut ready = None;
    for _ in 0..1000 {
        lw.prepare_frame_content();
        let (frame, _) = material_frame(&lw);
        if frame.get_size() == LogicalSize::new(400.0, 240.0) {
            ready = Some(frame);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(ready.is_some(), "the leather arrives");
    assert!(!lw.needs_animation_frame(), "and the window goes idle");
    assert_eq!(renders_on_this_thread() - before, 1, "drawn once");
}
