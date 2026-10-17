//! A relatively positioned inline box moves its text.
//!
//! CSS 2.2 9.4.3: "Once a box has been laid out according to the normal flow
//! or floated, it may be shifted relative to this position" - an inline box
//! too, with its text, background and border, and without moving any other
//! box or changing its line. `<p>mentioned<span style="position: relative;
//! top: -0.45em">a</span> in Job</p>` (the book's footnote marks) drew the
//! "a" on the line's baseline: the relative pass
//! (`positioning::adjust_relative_positions`) moves layout BOXES, and the text
//! of an inline box is painted from its block container's line layout - the
//! glyph runs on screen, the `TextLayout` payload printpdf draws from - which
//! never learned of the offset (pdfocr engine issue 2). Chrome is the
//! reference: the "a" is drawn 0.45em higher.
//!
//! Font-independent: every position is compared with the same paragraph
//! without the offset (the "control"), relative to its own box.
//!
//! Not compiled by the author (house rule); RED before the fix.

use std::{collections::BTreeMap, sync::Arc};

use azul_core::{
    dom::{Dom, DomId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
};
use azul_layout::{
    font::loading::build_font_cache,
    font_traits::{FontManager, TextLayoutCache},
    paged::FragmentationContext,
    solver3::{
        display_list::{DisplayListItem, WindowLogicalRect},
        layout_tree::TextPayload,
        paged_layout::layout_document_paged_with_config,
        pagination::FakePageConfig,
    },
    text3::{
        cache::{PositionedItem, ShapedItem, UnifiedLayout},
        default::PathLoader,
    },
    window::LayoutWindow,
    xml::DomXmlExt,
    Solver3LayoutCache,
};

use crate::table_markup::{body, items, near, rect, rects_of_color};

const RED: (u8, u8, u8) = (255, 0, 0);
const BLUE: (u8, u8, u8) = (0, 0, 255);
const BLACK: (u8, u8, u8) = (0, 0, 0);

/// `mentioned<span ..>a</span> in Job` in a paragraph `id`, the "a" red on
/// blue with `shift` (CSS declarations) in its style.
fn paragraph(id: &str, shift: &str) -> String {
    format!(
        "<p id=\"{id}\" style=\"margin: 0; font-size: 20px; line-height: 30px\">mentioned\
         <span style=\"color: rgb(255, 0, 0); background-color: rgb(0, 0, 255); {shift}\">a</span> \
         in Job</p>"
    )
}

/// The pen position (left end of the baseline) of the first glyph of every
/// text run painted in `color`, in paint order.
fn runs_in(lw: &LayoutWindow, (r, g, b): (u8, u8, u8)) -> Vec<LogicalPosition> {
    items(lw)
        .into_iter()
        .filter_map(|item| match item {
            DisplayListItem::Text { glyphs, color, .. }
                if color.r == r && color.g == g && color.b == b && color.a > 0 =>
            {
                glyphs.first().map(|glyph| glyph.point)
            }
            _ => None,
        })
        .collect()
}

fn minus(a: LogicalPosition, b: LogicalPosition) -> LogicalPosition {
    LogicalPosition::new(a.x - b.x, a.y - b.y)
}

fn assert_moved_by(what: &str, control: LogicalPosition, moved: LogicalPosition, by: (f32, f32)) {
    assert!(
        near(moved.x - control.x, by.0, 0.5) && near(moved.y - control.y, by.1, 0.5),
        "{what} moved by ({}, {}), want {by:?} (control {control:?}, moved {moved:?})",
        moved.x - control.x,
        moved.y - control.y,
    );
}

#[test]
fn a_relatively_positioned_inline_box_paints_its_text_and_background_moved() {
    let lw = body(&format!(
        "{}{}",
        paragraph("control", ""),
        paragraph("moved", "position: relative; top: -10px; left: 4px"),
    ));
    let control = rect(&lw, "control").origin;
    let moved = rect(&lw, "moved").origin;

    let red = runs_in(&lw, RED);
    assert_eq!(red.len(), 2, "one red \"a\" per paragraph: {red:?}");
    assert_moved_by(
        "the \"a\"",
        minus(red[0], control),
        minus(red[1], moved),
        (4.0, -10.0),
    );

    let blue = rects_of_color(&lw, BLUE);
    assert_eq!(blue.len(), 2, "one blue background per span: {blue:?}");
    assert_moved_by(
        "the span's background",
        minus(blue[0].origin, control),
        minus(blue[1].origin, moved),
        (4.0, -10.0),
    );
    assert!(
        near(blue[1].size.width, blue[0].size.width, 0.5)
            && near(blue[1].size.height, blue[0].size.height, 0.5),
        "the background keeps its size: {blue:?}"
    );
}

#[test]
fn a_relatively_positioned_inline_box_moves_nothing_else() {
    let lw = body(&format!(
        "{}{}",
        paragraph("control", ""),
        paragraph("moved", "position: relative; top: -10px; left: 4px"),
    ));
    let control = rect(&lw, "control");
    let moved = rect(&lw, "moved");
    assert!(
        near(moved.size.height, control.size.height, 0.01)
            && near(moved.size.width, control.size.width, 0.01),
        "the line is laid out as if the box were not moved: {control:?} vs {moved:?}"
    );
    // "mentioned" and " in Job", the same runs in each paragraph.
    let black = runs_in(&lw, BLACK);
    let half = black.len() / 2;
    assert!(
        half > 0 && black.len() == 2 * half,
        "the same black runs in both paragraphs: {black:?}"
    );
    for i in 0..half {
        assert_moved_by(
            "the text around the span",
            minus(black[i], control.origin),
            minus(black[i + half], moved.origin),
            (0.0, 0.0),
        );
    }
}

#[test]
fn nested_relatively_positioned_inline_boxes_add_their_offsets() {
    // The inner box is shifted relative to where the outer one put it.
    let nested = |id: &str, outer: &str, inner: &str| {
        format!(
            "<p id=\"{id}\" style=\"margin: 0; font-size: 20px; line-height: 30px\">x\
             <span style=\"{outer}\">y<span style=\"color: rgb(255, 0, 0); {inner}\">a</span>\
             </span> z</p>"
        )
    };
    let lw = body(&format!(
        "{}{}",
        nested("control", "", ""),
        nested(
            "moved",
            "position: relative; top: -4px",
            "position: relative; top: -6px; left: 3px"
        ),
    ));
    let control = rect(&lw, "control").origin;
    let moved = rect(&lw, "moved").origin;
    let red = runs_in(&lw, RED);
    assert_eq!(red.len(), 2, "{red:?}");
    assert_moved_by(
        "the inner \"a\"",
        minus(red[0], control),
        minus(red[1], moved),
        (3.0, -10.0),
    );
}

#[test]
fn an_em_offset_resolves_against_the_inline_boxs_own_font_size() {
    // The book's `top: -0.45em` on a 20px span: 9px up.
    let lw = body(&format!(
        "{}{}",
        paragraph("control", ""),
        paragraph("moved", "position: relative; top: -0.45em"),
    ));
    let control = rect(&lw, "control").origin;
    let moved = rect(&lw, "moved").origin;
    let red = runs_in(&lw, RED);
    assert_eq!(red.len(), 2, "{red:?}");
    assert_moved_by(
        "the \"a\"",
        minus(red[0], control),
        minus(red[1], moved),
        (0.0, -9.0),
    );
}

// ---- the paged (PDF) path: printpdf draws the `TextLayout` payload ----

/// The items of a `TextLayout` payload: a bare layout, or the cached
/// payload's sparse half (expanded from its dense half when retired).
fn payload_items(payload: &Arc<dyn std::any::Any + Send + Sync>) -> Option<Vec<PositionedItem>> {
    if let Some(layout) = payload.downcast_ref::<UnifiedLayout>() {
        return Some(layout.items.clone());
    }
    let p = payload.downcast_ref::<TextPayload>()?;
    Some(if p.sparse.items.is_empty() {
        p.dense.to_unified_items()
    } else {
        p.sparse.items.clone()
    })
}

/// Where the cluster of text `text` sits in `items`.
fn cluster_at(items: &[PositionedItem], text: &str) -> LogicalPosition {
    items
        .iter()
        .find_map(|it| match &it.item {
            ShapedItem::Cluster(c) if c.text() == text => {
                Some(LogicalPosition::new(it.position.x, it.position.y))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no cluster {text:?} in the layout"))
}

#[test]
fn a_relatively_positioned_inline_box_moves_its_text_in_the_pdf_payload() {
    let html = format!(
        "<html><head></head><body style=\"margin: 0\">{}{}</body></html>",
        paragraph("control", ""),
        paragraph("moved", "position: relative; top: -10px; left: 4px"),
    );
    let styled_dom = Dom::from_xml_string(html);
    let mut font_manager =
        FontManager::new(build_font_cache()).expect("Failed to create font manager");
    let mut layout_cache = Solver3LayoutCache::default();
    let mut text_cache = TextLayoutCache::new();
    let content_size = LogicalSize::new(800.0, 600.0);
    let loader = PathLoader::new();
    let font_loader = |bytes: Arc<rust_fontconfig::FontBytes>, index: usize| {
        loader.load_font_shared(bytes, index)
    };
    let display_lists = layout_document_paged_with_config(
        &mut layout_cache,
        &mut text_cache,
        FragmentationContext::new_paged(content_size),
        &styled_dom,
        LogicalRect::new(LogicalPosition::zero(), content_size),
        &mut font_manager,
        &BTreeMap::new(),
        &mut Some(Vec::new()),
        None,
        &RendererResources::default(),
        azul_core::resources::IdNamespace(0),
        DomId::ROOT_ID,
        font_loader,
        FakePageConfig::new(),
        &azul_core::resources::ImageCache::default(),
        azul_core::task::GetSystemTimeCallback {
            cb: azul_core::task::get_system_time_libstd,
        },
        false,
    )
    .expect("the document lays out");

    // One payload per paragraph, in document order (each its own page-top
    // relative layout; only positions WITHIN one are compared).
    let mut layouts: Vec<(f32, Vec<PositionedItem>)> = display_lists
        .iter()
        .flat_map(|dl| dl.items.iter())
        .filter_map(|item| match item {
            DisplayListItem::TextLayout {
                layout,
                bounds: WindowLogicalRect(bounds),
                ..
            } => payload_items(layout).map(|items| (bounds.origin.y, items)),
            _ => None,
        })
        .filter(|(_, items)| {
            items
                .iter()
                .any(|it| matches!(&it.item, ShapedItem::Cluster(c) if c.text() == "a"))
        })
        .collect();
    layouts.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert_eq!(layouts.len(), 2, "one text layout per paragraph");

    let a_from_m = |items: &[PositionedItem]| minus(cluster_at(items, "a"), cluster_at(items, "m"));
    assert_moved_by(
        "the \"a\" in the PDF payload",
        a_from_m(&layouts[0].1),
        a_from_m(&layouts[1].1),
        (4.0, -10.0),
    );
    // The text around it stays.
    let j_from_m = |items: &[PositionedItem]| minus(cluster_at(items, "J"), cluster_at(items, "m"));
    assert_moved_by(
        "the \"J\" in the PDF payload",
        j_from_m(&layouts[0].1),
        j_from_m(&layouts[1].1),
        (0.0, 0.0),
    );
}
