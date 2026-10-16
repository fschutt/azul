//! A window draws its materials, its light-following shadows and the cursor's
//! sheen from the ONE app light: the app sets it once (`AppConfig::with_light`)
//! or moves it (`CallbackInfo::set_app_light`), and every window adopts it at
//! its next frame. The cursor's sheen on a material eases in over 140 ms and
//! out over 320 ms, and asks for frames only while it moves.

use azul_core::{
    dom::{Dom, DomId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    id::NodeId,
    light::Light,
    resources::{AppConfig, ImageRef, RawImageData, RawImageFormat, RendererResources},
    styled_dom::StyledDom,
    window::CursorPosition,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    texture::MaterialLight, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// One div with `css` at the window's top-left in a 400 x 300 window, laid
/// out and with its first frame prepared.
fn lay_out(css: &str) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let dom = Dom::create_body()
        .with_css("margin: 0px;")
        .with_child(Dom::create_div().with_css(css));
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the div lays out");
    lw.prepare_frame_content();
    lw
}

fn items(lw: &LayoutWindow) -> Vec<DisplayListItem> {
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .display_list
        .items
        .clone()
}

/// The rendered frame of the window's one material background.
fn frame(lw: &LayoutWindow) -> (LogicalRect, ImageRef) {
    items(lw)
        .into_iter()
        .find_map(|item| match item {
            DisplayListItem::Image { bounds, image, .. } => Some((*bounds.inner(), image)),
            _ => None,
        })
        .expect("the material is drawn")
}

/// The mean straight luma of the frame's pixels in the box (`x`, `y`, `w`,
/// `h`), frame pixels.
fn mean_luma(image: &ImageRef, (x0, y0, w, h): (usize, usize, usize, usize)) -> f32 {
    let raw = image.get_rawimage().expect("a CPU frame");
    let RawImageData::U8(bytes) = &raw.pixels else {
        panic!("an 8-bit frame");
    };
    let b = bytes.as_ref();
    let mut sum = 0.0;
    for y in y0..y0 + h {
        for x in x0..x0 + w {
            let i = (y * raw.width + x) * 4;
            let (r, g, bl) = match raw.data_format {
                RawImageFormat::BGRA8 => (b[i + 2], b[i + 1], b[i]),
                _ => (b[i], b[i + 1], b[i + 2]),
            };
            sum += 0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(bl);
        }
    }
    sum / (w * h) as f32
}

#[test]
fn the_app_config_carries_the_light_every_window_starts_with() {
    assert_eq!(AppConfig::create().light, Light::DEFAULT, "the sun at the top right, low");
    let moved = Light::point(1200.0, 40.0, 700.0);
    assert_eq!(AppConfig::create().with_light(moved).light, moved);
}

#[test]
fn a_new_app_light_relights_the_materials_of_a_window() {
    let mut lw = lay_out("width: 200px; height: 60px; background: builtin(leather); --azul-bevel: 6px;");
    let (_, before) = frame(&lw);
    // Moving the app light: the window adopts it at its next frame and
    // renders its material again.
    let lit_from_below = Light::sun(270.0, 30.0);
    azul_core::light::set_app_light(lit_from_below);
    lw.prepare_frame_content();
    let adopted = lw.effective_light();
    let (_, after) = frame(&lw);
    azul_core::light::set_app_light(Light::DEFAULT);
    assert_eq!(adopted, lit_from_below, "the window follows the app light");
    assert_ne!(before.get_hash(), after.get_hash(), "and draws its material anew");
}

#[test]
fn a_window_s_own_material_light_outranks_the_app_light() {
    let mut lw = lay_out("width: 200px; height: 60px; background: builtin(leather);");
    assert!(lw.set_material_light(MaterialLight::from_direction(180.0, 40.0)));
    assert_eq!(lw.effective_light(), Light::sun(180.0, 40.0));
}

#[test]
fn a_box_shadow_that_follows_the_light_falls_away_from_it() {
    // The shadow is written as falling 16 px down (a light straight above).
    let mut lw = lay_out(
        "width: 100px; height: 40px; margin: 40px; box-shadow: 0px 16px 30px black; \
         --azul-follow-light: box-shadow;",
    );
    // A window light from the top right, 36 degrees high.
    let light = Light::sun(45.0, 36.0);
    lw.set_material_light(MaterialLight::from_direction(45.0, 36.0));
    lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
    let shadow = items(&lw)
        .into_iter()
        .find_map(|item| match item {
            DisplayListItem::BoxShadow { shadow, .. } => Some(shadow),
            _ => None,
        })
        .expect("the shadow is drawn");
    let expected = light.follow_offset([0.0, 16.0], [90.0, 60.0], true);
    let got = [
        shadow.offset_x.to_pixels_internal(16.0, 16.0),
        shadow.offset_y.to_pixels_internal(16.0, 16.0),
    ];
    assert!(got[0] < -4.0 && got[1] > 4.0, "it falls to the bottom left: {got:?}");
    assert!(
        (got[0] - expected[0]).abs() < 0.5 && (got[1] - expected[1]).abs() < 0.5,
        "{got:?} vs {expected:?}"
    );

    // Without the opt-in a shadow stays as written.
    let mut lw = lay_out("width: 100px; height: 40px; box-shadow: 0px 16px 30px black;");
    lw.set_material_light(MaterialLight::from_direction(45.0, 36.0));
    lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
    let shadow = items(&lw)
        .into_iter()
        .find_map(|item| match item {
            DisplayListItem::BoxShadow { shadow, .. } => Some(shadow),
            _ => None,
        })
        .expect("the shadow is drawn");
    assert_eq!(shadow.offset_x.to_pixels_internal(16.0, 16.0), 0.0);
}

#[test]
fn the_cursor_sheen_eases_in_quickly_and_out_slowly() {
    let mut lw = lay_out("width: 200px; height: 60px; background: builtin(leather);");
    let node = NodeId::new(1);
    assert_eq!(lw.material_sheen_of(DomId::ROOT_ID, node), Some(0.0), "at rest no sheen");
    assert!(!lw.needs_animation_frame(), "a material at rest asks for no frames");
    let (_, unlit) = frame(&lw);

    // The cursor comes over the surface: the sheen eases in, 1 - e^-1 of the
    // way after 140 ms.
    lw.current_window_state.mouse_state.cursor_position =
        CursorPosition::InWindow(LogicalPosition::new(100.0, 30.0));
    assert!(lw.needs_animation_frame(), "the sheen starts to move");
    assert!(lw.step_material_sheen(0.14));
    let amount = lw.material_sheen_of(DomId::ROOT_ID, node).expect("a surface");
    assert!((amount - 0.632).abs() < 0.03, "eased in: {amount}");
    lw.prepare_frame_content();
    let (_, lit) = frame(&lw);
    assert_eq!(
        unlit.get_hash(),
        lit.get_hash(),
        "the sheen is a layer over the material: the material is not drawn again"
    );

    // The cursor leaves: it fades out more slowly, e^-1 of the rest after
    // 320 ms.
    lw.current_window_state.mouse_state.cursor_position =
        CursorPosition::OutOfWindow(LogicalPosition::new(500.0, 30.0));
    assert!(lw.step_material_sheen(0.32));
    let fading = lw.material_sheen_of(DomId::ROOT_ID, node).expect("a surface");
    assert!((fading - amount * 0.368).abs() < 0.03, "eased out: {fading}");
    for _ in 0..200 {
        lw.step_material_sheen(0.016);
    }
    assert_eq!(lw.material_sheen_of(DomId::ROOT_ID, node), Some(0.0), "settled");
    assert!(!lw.needs_animation_frame(), "a settled sheen asks for no frames");
}
