//! Moving the app light (`CallbackInfo::set_app_light`) relights the
//! materials of a window at its next frame.
//!
//! A process of its own: the app light belongs to the process, and a window
//! in another test of a shared binary that draws a material would adopt the
//! moved light at its next frame and draw that material anew.

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    light::Light,
    resources::{ImageRef, RendererResources},
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
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

/// The rendered frame of the window's one material background.
fn frame(lw: &LayoutWindow) -> ImageRef {
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .display_list
        .items
        .iter()
        .find_map(|item| match item {
            DisplayListItem::Image { image, .. } => Some(image.clone()),
            _ => None,
        })
        .expect("the material is drawn")
}

#[test]
fn a_new_app_light_relights_the_materials_of_a_window() {
    let mut lw = lay_out("width: 200px; height: 60px; background: builtin(leather); --azul-bevel: 6px;");
    let before = frame(&lw);
    // Moving the app light: the window adopts it at its next frame and
    // renders its material again.
    let lit_from_below = Light::sun(270.0, 30.0);
    azul_core::light::set_app_light(lit_from_below);
    lw.prepare_frame_content();
    let adopted = lw.effective_light();
    let after = frame(&lw);
    azul_core::light::set_app_light(Light::DEFAULT);
    assert_eq!(adopted, lit_from_below, "the window follows the app light");
    assert_ne!(before.get_hash(), after.get_hash(), "and draws its material anew");
}
