//! A tracked label centres once its last tracking is given back.
//!
//! `letter-spacing` adds its space after every character, the last one
//! included: Chrome does (a 20px "ABC" tracked 0.08em is 44.41px wide, the
//! glyphs' 39.61px and three trackings), and so does azul. A centred label in
//! capitals - a tab, a button, a group title - therefore sits half a tracking
//! left of the centre. Chrome has no property that drops the last tracking;
//! the way to centre the letters is `margin-right` of minus the tracking, in
//! the same `em`, which takes the trailing space out of the box that is
//! centred. The numbers are headless Chrome 155's for the same CSS and font.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// Calmet UI (the `azul-fonts-calmet` crate AzDrive links), registered the
/// way an app registers its fonts at startup: `AppConfig::add_font` per
/// face, then the window's font manager takes them in.
fn register_calmet(lw: &mut LayoutWindow) {
    let mut config = azul_core::resources::AppConfig::create();
    for face in azul_fonts_calmet::faces() {
        config.add_font(azul_fonts_calmet::FAMILY.into(), face.to_vec().into());
    }
    lw.font_manager.register_app_fonts(config.bundled_fonts.as_ref());
}

/// The label "ABC" in Calmet UI 20px tracked 0.08em, under `label`, in a
/// 300px wide box whose own CSS is `container`: the label's box (where it
/// has one of its own) and the pen x of its first capital.
fn label(container: &str, label: &str) -> (Option<LogicalRect>, f32) {
    // body (0) > div (1) > span (2) > text (3)
    let dom = Dom::create_body()
        .with_css("margin: 0px; font-family: \"Calmet UI\"; font-size: 20px;")
        .with_child(
            Dom::create_div()
                .with_css(&format!("width: 300px; height: 30px; {container}"))
                .with_child(
                    Dom::create_span()
                        .with_css(&format!("letter-spacing: 0.08em; {label}"))
                        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                            "ABC",
                        )),
                ),
        );
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    register_calmet(&mut lw);
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 200.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the label lays out");
    let rect = lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(2))),
    });
    let first_x = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list
        .items
        .iter()
        .find_map(|item| match item {
            DisplayListItem::Text { glyphs, .. } => glyphs.first().map(|g| g.point.x),
            _ => None,
        })
        .expect("the label paints");
    (rect, first_x)
}

fn assert_near(what: &str, got: f32, chrome: f32) {
    assert!((got - chrome).abs() < 0.05, "{what}: Chrome {chrome:.3}, azul {got:.3}");
}

const FLEX_CENTRE: &str = "display: flex; justify-content: center; align-items: center;";

#[test]
fn the_tracking_after_the_last_capital_is_part_of_the_label_as_in_chrome() {
    let (rect, first_x) = label(FLEX_CENTRE, "");
    let rect = rect.expect("a flex item has a box");
    assert_near("the label's width, three trackings in it", rect.size.width, 44.406);
    assert_near("half a tracking left of the centre", rect.origin.x, 127.797);
    assert_near("the first capital at the label's start", first_x, 127.797);
}

#[test]
fn a_negative_margin_of_one_tracking_centres_the_letters_in_a_flex_box() {
    let (rect, first_x) = label(FLEX_CENTRE, "margin-right: -0.08em;");
    let rect = rect.expect("a flex item has a box");
    assert_near("the label's width", rect.size.width, 44.406);
    assert_near("the letters centred", rect.origin.x, 128.594);
    assert_near("the first capital at the label's start", first_x, 128.594);
}

#[test]
fn a_negative_margin_of_one_tracking_centres_the_letters_in_a_line() {
    let (_, first_x) = label("text-align: center;", "margin-right: -0.08em;");
    assert_near("the letters centred", first_x, 128.594);
}
