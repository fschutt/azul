//! Selection RENDERING for the two things the display list resolved and then
//! dropped on the floor:
//!
//! - a multi-cursor (Ctrl+D) session, whose occurrences all live on the SAME
//!   node — `TextSelection::affected_nodes` mapped a node to ONE range, so
//!   every occurrence but the primary was unexpressible;
//! - the `::selection` text colour, which `get_selection_style` resolved and
//!   no painter ever applied, leaving selected glyphs dark-on-dark under an
//!   opaque highlight.

use azul_core::dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId};
use azul_core::geom::LogicalSize;
use azul_core::resources::RendererResources;
use azul_core::selection::{
    CursorAffinity, GraphemeClusterId, MultiCursorState, SelectionRange, TextCursor,
};
use azul_core::styled_dom::{NodeHierarchyItemId, StyledDom};
use azul_css::props::basic::ColorU;
use azul_layout::solver3::display_list::DisplayListItem;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// Node layout: body=0, div=1, text=2. The editing session keys on the div,
/// which is the IFC root that owns the inline layout.
const P: usize = 1;

const TEXT: &str = "first paragraph";

fn cursor(byte: u32) -> TextCursor {
    TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: byte,
        },
        affinity: CursorAffinity::Leading,
    }
}

fn range(start: u32, end: u32) -> SelectionRange {
    SelectionRange {
        start: cursor(start),
        end: cursor(end),
    }
}

fn layout_one_paragraph(paragraph_css: &str) -> LayoutWindow {
    let css_src = format!(
        "* {{ margin: 0; padding: 0; }} \
         body {{ font-size: 14px; width: 600px; }} \
         .p {{ display: block; {paragraph_css} }}"
    );
    let class: azul_core::dom::IdOrClassVec = vec![IdOrClass::Class("p".into())].into();
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(class)
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(TEXT)),
    );
    let (css, _) = azul_css::parser2::new_from_str(&css_src);
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut layout_window = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    layout_window.current_window_state = window_state.clone();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    layout_window
        .layout_and_generate_display_list(
            styled_dom,
            &window_state,
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();
    layout_window
}

/// Install a multi-cursor session holding exactly `ranges` and repaint.
fn select_ranges(lw: &mut LayoutWindow, ranges: &[SelectionRange]) {
    let node = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(P))),
    };
    let mut mc = MultiCursorState::new_with_cursor(cursor(0), node, 0);
    mc.selections.clear();
    for r in ranges {
        let _ = mc.add_selection(*r);
    }
    lw.text_edit_manager.multi_cursor = Some(mc);
    lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
}

/// The distinct horizontal bands the selection highlight covers. All ranges sit
/// on one line here, so a band per occurrence is a band per `x`.
fn selection_band_xs(lw: &LayoutWindow) -> Vec<f32> {
    let mut xs: Vec<f32> = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("layout result")
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::SelectionRect { bounds, .. } => Some(bounds.origin().x),
            _ => None,
        })
        .collect();
    xs.sort_by(f32::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < 2.0);
    xs
}

fn text_colors(lw: &LayoutWindow) -> Vec<ColorU> {
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("layout result")
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Text { color, .. } => Some(*color),
            _ => None,
        })
        .collect()
}

#[test]
fn every_range_of_a_multi_cursor_session_paints_its_own_highlight() {
    let mut lw = layout_one_paragraph("");

    select_ranges(&mut lw, &[range(0, 5)]);
    let one = selection_band_xs(&lw);
    assert_eq!(one.len(), 1, "one occurrence = one band, got {one:?}");

    // "first" and "paragrap": disjoint, same line, different x. The map used to
    // keep only the PRIMARY (last-added) range, so this stayed at one band.
    select_ranges(&mut lw, &[range(0, 5), range(6, 14)]);
    let two = selection_band_xs(&lw);
    assert_eq!(
        two.len(),
        2,
        "both occurrences must render, got bands at {two:?}"
    );
    assert!(two[0] < two[1], "document order: {two:?}");
}

#[test]
fn selected_glyphs_are_painted_in_the_selection_text_color() {
    let red = ColorU {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };
    let mut lw = layout_one_paragraph(
        "-azul-selection-color: #ff0000; -azul-selection-background-color: #0000ff;",
    );

    let before = text_colors(&lw);
    let normal = *before.first().expect("the paragraph paints a text run");
    assert!(
        !before.contains(&red),
        "nothing is selected yet: {before:?}"
    );

    select_ranges(&mut lw, &[range(0, 5)]);
    let after = text_colors(&lw);
    assert!(
        after.contains(&red),
        "the selected glyphs must be pushed in the ::selection colour: {after:?}"
    );
    assert!(
        after.contains(&normal),
        "the UNSELECTED half of the run keeps its own colour: {after:?}"
    );
}

#[test]
fn without_a_selection_color_the_glyph_run_is_not_split() {
    let mut lw = layout_one_paragraph("-azul-selection-background-color: #0000ff;");

    let before = text_colors(&lw);
    select_ranges(&mut lw, &[range(0, 5)]);

    assert!(
        !selection_band_xs(&lw).is_empty(),
        "the selection is active, only its text colour is unset"
    );
    assert_eq!(
        text_colors(&lw),
        before,
        "no ::selection colour = the same text items as before"
    );
}
