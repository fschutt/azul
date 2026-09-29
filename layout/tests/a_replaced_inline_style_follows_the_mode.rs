//! `CallbackInfo::set_node_style`: a node's inline stylesheet replaced at
//! run time resolves like a node BUILT with it - now, and after a light /
//! dark switch of the retained DOM.
//!
//! The live restyles of segmented / stepper / pagination / date_picker used
//! `set_css_property`, which writes a USER OVERRIDE: it outranks every
//! declaration, so the colour they baked for the mode of the moment stayed
//! after a restyle-only mode switch (W4 section 6.2), and outranked the
//! node's `:hover` / `:focus` rules (W3b). A replaced inline style pins
//! nothing: its dark twins are declarations like any other.
//!
//! Driven through the content chokepoint the dll host and the e2e runner
//! both delegate `CallbackChange::SetNodeStyle` to.

use std::sync::Arc;

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
    window::{OptionWindowTheme, WindowTheme},
};
use azul_css::{
    css::Css,
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::color::ColorU,
        layout::{LayoutHeight, LayoutWidth},
        property::CssProperty,
        style::{StyleBackgroundContent, StyleBackgroundContentVec},
    },
    system::defaults,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    overlay::{ContentChange, ContentDirtyTier},
    solver3::display_list::DisplayListItem,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

pub(crate) const RED: ColorU = ColorU::rgb(200, 0, 0);
const BLUE: ColorU = ColorU::rgb(0, 0, 200);
const GREEN: ColorU = ColorU::rgb(0, 160, 0);
const YELLOW: ColorU = ColorU::rgb(220, 200, 0);

/// The box under test: body > div, the div is node 1.
pub(crate) const BOX: NodeId = NodeId::new(1);

pub(crate) fn env_pinned() -> bool {
    azul_css::dynamic_selector::mode_pinned_by_env().is_some()
}

pub(crate) fn fill(c: ColorU) -> CssProperty {
    CssProperty::const_background_content(StyleBackgroundContentVec::from_vec(vec![
        StyleBackgroundContent::Color(c),
    ]))
}

/// A `width` x 20 box painted `light` by day and `dark` by night.
fn box_style(width: isize, light: ColorU, dark: ColorU) -> Css {
    CssPropertyWithConditionsVec::from_vec(vec![
        CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(width))),
        CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(20))),
        CssPropertyWithConditions::simple(fill(light)),
        CssPropertyWithConditions::dark_theme(fill(dark)),
    ])
    .into()
}

fn window_state(theme: WindowTheme) -> FullWindowState {
    let mut ws = FullWindowState::default();
    ws.theme = theme;
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    ws
}

fn lay_out(lw: &mut LayoutWindow, styled: StyledDom, ws: &FullWindowState) {
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("layout");
}

/// A light desktop, the app following it, the red / blue box laid out.
pub(crate) fn window() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.set_system_style(Arc::new(defaults::macos_modern_light()));
    lw.mode = OptionWindowTheme::None;
    let dom = Dom::create_body()
        .with_css("margin: 0;")
        .with_child(Dom::create_div().with_style(box_style(40, RED, BLUE)));
    lay_out(&mut lw, StyledDom::create_from_dom(dom), &window_state(WindowTheme::LightMode));
    lw
}

/// The app switches its colour scheme; the RETAINED DOM is re-styled (no
/// new DOM - the restyle path of `set_mode`).
pub(crate) fn switch_scheme(lw: &mut LayoutWindow, scheme: OptionWindowTheme) {
    lw.mode = scheme;
    let ws = window_state(lw.window_mode_for(WindowTheme::LightMode));
    let retained = lw
        .layout_results
        .remove(&DomId::ROOT_ID)
        .expect("laid out")
        .styled_dom;
    lay_out(lw, retained, &ws);
}

/// The colour the display list paints a `width` x 20 box in.
pub(crate) fn box_fill(lw: &LayoutWindow, width: f32) -> Option<ColorU> {
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list
        .items
        .iter()
        .find_map(|item| match item {
            DisplayListItem::Rect { bounds, color, .. }
                if (bounds.0.size.width - width).abs() < 0.01
                    && (bounds.0.size.height - 20.0).abs() < 0.01 =>
            {
                Some(*color)
            }
            _ => None,
        })
}

pub(crate) fn replace(lw: &mut LayoutWindow, style: Css) -> ContentDirtyTier {
    lw.apply_content_change(ContentChange::NodeStyle {
        dom_id: DomId::ROOT_ID,
        node_id: BOX,
        style,
    })
    .tier
}

#[test]
fn a_replaced_inline_style_paints_its_light_face_at_once() {
    if env_pinned() {
        return;
    }
    let mut lw = window();
    assert_eq!(box_fill(&lw, 40.0), Some(RED), "premise: built red by day");
    let tier = replace(&mut lw, box_style(40, GREEN, YELLOW));
    assert_eq!(box_fill(&lw, 40.0), Some(GREEN), "the new style paints at once");
    assert_eq!(
        tier,
        ContentDirtyTier::RebuildDisplayList,
        "only colours changed: a repaint, not a relayout"
    );
}

/// THE property the widgets need: after a restyle-only switch to dark, the
/// node shows the dark twin of its NEW style - not the old style's, and not
/// a pinned light colour.
#[test]
fn a_replaced_inline_style_takes_its_own_dark_twin_after_a_scheme_switch() {
    if env_pinned() {
        return;
    }
    let mut lw = window();
    replace(&mut lw, box_style(40, GREEN, YELLOW));
    switch_scheme(&mut lw, OptionWindowTheme::Some(WindowTheme::DarkMode));
    assert_eq!(
        box_fill(&lw, 40.0),
        Some(YELLOW),
        "dark mode shows the replaced style's dark twin"
    );
    switch_scheme(&mut lw, OptionWindowTheme::None);
    assert_eq!(box_fill(&lw, 40.0), Some(GREEN), "and light mode its light face again");
}

/// A replacement that moves geometry lays the page out again - the node is
/// where its new style puts it.
#[test]
fn a_replaced_inline_style_that_changes_geometry_relayouts() {
    if env_pinned() {
        return;
    }
    let mut lw = window();
    let tier = replace(&mut lw, box_style(80, GREEN, YELLOW));
    assert_eq!(tier, ContentDirtyTier::Relayout);
    assert_eq!(box_fill(&lw, 80.0), Some(GREEN), "the box is 80 px wide now");
    assert_eq!(box_fill(&lw, 40.0), None, "and no longer 40");
}

/// Replacing a style with the same declarations changes nothing.
#[test]
fn replacing_a_style_with_itself_is_unchanged() {
    let mut lw = window();
    assert_eq!(replace(&mut lw, box_style(40, RED, BLUE)), ContentDirtyTier::Unchanged);
}
