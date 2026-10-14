//! A clamped block ends its last line with an ellipsis.
//!
//! AzDrive's icon names wrap to two lines and end in an ellipsis where the
//! name goes on (`-webkit-line-clamp: 2`, the design's CSS). Chrome keeps the
//! clamped lines as they were broken and ends the last one in an ellipsis:
//! the line keeps what fits in front of the ellipsis, its trailing space left
//! out, and gives up characters from its end until the ellipsis fits too -
//! "alpha beta gamma / delta epsilon zet..." in a 170px box of Calmet UI 24px
//! (headless Chrome 155). Unclamped, the same text takes three lines.
//! azul dropped the lines past the clamp without any ellipsis, and did not
//! know the prefixed spelling at all.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, font::parsed::ParsedFont,
    solver3::display_list::DisplayListItem, window::LayoutWindow,
    window_state::FullWindowState,
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

const TEXT: &str = "alpha beta gamma delta epsilon zeta eta theta";
const WIDTH: f32 = 170.0;
const LINE: f32 = 30.0;

/// The upright Calmet UI, parsed.
fn calmet() -> ParsedFont {
    ParsedFont::from_bytes(&azul_fonts_calmet::faces()[0], 0, &mut Vec::new())
        .expect("Calmet UI parses")
}

/// What the clamped block shows: its height and every glyph painted
/// (index, pen x, baseline y).
struct Shown {
    height: f32,
    glyphs: Vec<(u32, f32, f32)>,
}

fn shown(clamp: &str) -> Shown {
    // body (0) > div (1) > text (2)
    let dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_div()
            .with_css(&format!(
                "width: {WIDTH}px; font-family: \"Calmet UI\"; font-size: 24px; \
                 line-height: {LINE}px; overflow: hidden; {clamp}"
            ))
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(TEXT)),
    );
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    register_calmet(&mut lw);
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
    .expect("the block lays out");
    let height = lw
        .get_node_layout_rect(DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1))),
        })
        .expect("the block is laid out")
        .size
        .height;
    let glyphs = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list
        .items
        .iter()
        .flat_map(|item| match item {
            DisplayListItem::Text { glyphs, .. } => glyphs
                .iter()
                .map(|g| (g.index, g.point.x, g.point.y))
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect();
    Shown { height, glyphs }
}

fn assert_two_lines_ending_in_an_ellipsis(clamp: &str) {
    let font = calmet();
    let ellipsis = u32::from(font.lookup_glyph_index(0x2026).expect("Calmet has the ellipsis"));
    let space = u32::from(font.lookup_glyph_index(0x20).expect("Calmet has the space"));
    let ellipsis_advance = f32::from(font.get_horizontal_advance(ellipsis as u16)) / 1000.0 * 24.0;

    let s = shown(clamp);
    assert!((s.height - 2.0 * LINE).abs() < 0.5, "{clamp}: two lines, {}px", s.height);
    let marks: Vec<_> = s.glyphs.iter().filter(|g| g.0 == ellipsis).collect();
    assert_eq!(marks.len(), 1, "{clamp}: one ellipsis: {:?}", s.glyphs);
    let (_, x, baseline) = *marks[0];
    let lines: Vec<f32> = {
        let mut ys: Vec<f32> = s.glyphs.iter().map(|g| g.2).collect();
        ys.sort_by(f32::total_cmp);
        ys.dedup_by(|a, b| (*a - *b).abs() < 0.5);
        ys
    };
    assert_eq!(lines.len(), 2, "{clamp}: glyphs on two baselines: {lines:?}");
    assert!((baseline - lines[1]).abs() < 0.5, "{clamp}: the ellipsis ends the second line");
    assert!(
        x + ellipsis_advance <= WIDTH + 0.5,
        "{clamp}: the ellipsis inside the box: {x} + {ellipsis_advance} > {WIDTH}"
    );
    let line_two: Vec<_> = s
        .glyphs
        .iter()
        .filter(|g| (g.2 - lines[1]).abs() < 0.5 && g.0 != ellipsis)
        .collect();
    assert!(
        line_two.iter().all(|g| g.1 < x),
        "{clamp}: nothing painted after the ellipsis: {line_two:?}"
    );
    let letters = line_two.iter().filter(|g| g.0 != space).count();
    assert_eq!(letters, "deltaepsilonzet".len(), "{clamp}: \"delta epsilon zet\" kept, as in Chrome");
}

#[test]
fn unclamped_the_text_takes_three_lines() {
    let s = shown("");
    assert!((s.height - 3.0 * LINE).abs() < 0.5, "three lines: {}px", s.height);
}

#[test]
fn webkit_line_clamp_two_ends_the_second_line_with_an_ellipsis() {
    assert_two_lines_ending_in_an_ellipsis("-webkit-line-clamp: 2;");
}

#[test]
fn line_clamp_two_ends_the_second_line_with_an_ellipsis() {
    assert_two_lines_ending_in_an_ellipsis("line-clamp: 2;");
}
