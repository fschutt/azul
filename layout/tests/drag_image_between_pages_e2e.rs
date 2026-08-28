//! H22/H23 — THE architecture validation (user directive): "is the
//! architecture good enough so that we won't hit issues when we drag
//! images around between pages?"
//!
//! The Word model: an anchored object belongs to its anchor's page and
//! contributes exclusion geometry ONLY to that page's layout (css-break-3
//! parallel flows; design doc §4.6/K32). A drag = the app moves the
//! object's anchor in its model → repaginate → BOTH affected pages re-wrap
//! with per-page exclusions and no memory of the old-page geometry.
//!
//! The "image" is a fixed-size floated block — pure wrap geometry, no
//! image-decoding entanglement (replaced-content sizing is tracked in
//! ENGINE-ISSUES separately).

use azul_core::dom::{Dom, DomId};
use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_core::resources::RendererResources;
use azul_layout::font::loading::build_font_cache;
use azul_layout::font_traits::{FontManager, TextLayoutCache};
use azul_layout::solver3::display_list::{DisplayList, DisplayListItem};
use azul_layout::solver3::paged_layout::layout_document_tokenized;
use azul_layout::text3::default::PathLoader;
use azul_layout::xml::DomXmlExt;
use azul_layout::Solver3LayoutCache;
use std::collections::HashMap;

const PAGE_H: f32 = 300.0;

/// The app model: four 140px paragraphs over 300px pages (2 per page), the
/// float anchored before paragraph `anchor`.
fn doc(anchor_paragraph: usize) -> String {
    let mut body = String::from(
        r#"<html><head><style>
        * { margin: 0; padding: 0; }
        body { font-size: 14px; width: 600px; }
        .p { height: 140px; }
        .img { float: left; width: 200px; height: 100px; }
    </style></head><body>"#,
    );
    for i in 0..4 {
        if i == anchor_paragraph {
            body.push_str(r#"<div class="img">IMG</div>"#);
        }
        body.push_str(&format!(r#"<div class="p">paragraph {i}</div>"#));
    }
    body.push_str("</body></html>");
    body
}

fn paginate(html: &str) -> Vec<DisplayList> {
    let styled_dom = Dom::from_xml_string(html);
    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("fm");
    let mut cache = Solver3LayoutCache {
        tree: None,
        calculated_positions: Vec::new(),
        viewport: None,
        scroll_ids: HashMap::new(),
        scroll_id_to_node_id: HashMap::new(),
        counters: HashMap::new(),
        float_cache: HashMap::new(),
        cache_map: Default::default(),
        previous_positions: Vec::new(),
        cached_display_list: None,
        prev_dom_ptr: 0,
        prev_viewport: LogicalRect {
            origin: LogicalPosition::zero(),
            size: LogicalSize::zero(),
        },
        ..Default::default()
    };
    let mut text_cache = TextLayoutCache::new();
    let viewport = LogicalRect {
        origin: LogicalPosition::zero(),
        size: LogicalSize::new(600.0, PAGE_H),
    };
    let rr = RendererResources::default();
    let mut dbg = None;
    let loader = PathLoader::new();
    layout_document_tokenized(
        &mut cache,
        &mut text_cache,
        &styled_dom,
        viewport,
        &mut font_manager,
        &mut dbg,
        &azul_core::resources::ImageCache::default(),
        azul_core::task::GetSystemTimeCallback {
            cb: azul_core::task::get_system_time_libstd,
        },
        |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
            loader.load_font_shared(bytes, index)
        },
        &rr,
        azul_core::resources::IdNamespace(0),
        DomId::ROOT_ID,
        PAGE_H,
        16,
    )
    .expect("tokenized")
    .into_iter()
    .map(|p| p.display_list)
    .collect()
}

/// The float's 200x100 footprint on a page, if present.
///
/// The fixture's float is an UNSTYLED div (pure wrap geometry), so the only
/// painted-model item carrying its box is its hit-test area — matched
/// alongside Rect/Border so a styled variant would count too. (This used to
/// match only Rect/Border and passed by accident: every node emitted a
/// phantom zero-width Border until the display list stopped emitting
/// borders that paint nothing.)
fn float_rect_on(page: &DisplayList) -> Option<LogicalRect> {
    page.items.iter().find_map(|i| match i {
        DisplayListItem::Rect { bounds, .. }
        | DisplayListItem::Border { bounds, .. }
        | DisplayListItem::HitTestArea { bounds, .. }
            if (bounds.0.size.width - 200.0).abs() < 0.6
                && (bounds.0.size.height - 100.0).abs() < 0.6 =>
        {
            Some(bounds.0)
        }
        _ => None,
    })
}

/// Text runs whose clip starts to the RIGHT of the float band (x >= 190):
/// evidence that line content was pushed aside by an exclusion.
fn wrapped_text_count(page: &DisplayList) -> usize {
    page.items
        .iter()
        .filter(|i| match i {
            DisplayListItem::Text { glyphs, .. } => {
                glyphs.first().is_some_and(|g| g.point.x >= 190.0)
            }
            _ => false,
        })
        .count()
}

#[test]
fn dragging_the_image_to_page_two_moves_its_exclusion_with_it() {
    // BEFORE: anchored at paragraph 0 (page 1).
    let before = paginate(&doc(0));
    assert!(before.len() >= 2, "{} pages", before.len());
    let img_before = float_rect_on(&before[0]).expect("float renders on page 1");
    assert!(
        img_before.origin.y < 1.0,
        "anchored at the top of page 1: {img_before:?}"
    );
    assert!(
        float_rect_on(&before[1]).is_none(),
        "the exclusion belongs to page 1 ONLY"
    );
    assert!(
        wrapped_text_count(&before[0]) >= 1,
        "page 1 text wraps beside the float (pushed right of x=190)"
    );
    assert_eq!(
        wrapped_text_count(&before[1]),
        0,
        "page 2 lines start at the left edge — no leaked exclusion"
    );

    // DRAG: the app moves the anchor to paragraph 2 (page 2) and
    // repaginates — the entire flow, exactly what a drop does.
    let after = paginate(&doc(2));
    assert!(after.len() >= 2);
    assert!(
        float_rect_on(&after[0]).is_none(),
        "page 1 no longer hosts the image after the drag"
    );
    let img_after = float_rect_on(&after[1]).expect("the image now renders on page 2");
    assert!(
        img_after.origin.y < 1.0,
        "anchored at its paragraph's page top: {img_after:?}"
    );
    assert_eq!(
        wrapped_text_count(&after[0]),
        0,
        "page 1 lines are FULL WIDTH again — the old page keeps no memory \
         of the exclusion (the Regions-killing leak the design forbids)"
    );
    assert!(
        wrapped_text_count(&after[1]) >= 1,
        "page 2 text wraps beside the image at its NEW anchor"
    );
}

#[test]
fn image_that_no_longer_fits_its_page_moves_whole_never_splits() {
    // Anchor before paragraph 1: pen sits at 140 when the 100px float
    // arrives — it fits page 1 (140+100=240 < 300). Anchor before
    // paragraph 2 (pen 280 on page 1 → 280+100 > 300): the ATOMIC float
    // moves whole to page 2 (the Word anchoring model — objects never
    // tear across pages).
    let pages = paginate(&doc(2));
    assert!(
        float_rect_on(&pages[0]).is_none() && float_rect_on(&pages[1]).is_some(),
        "an unfitting anchored object moves WHOLE to the next page"
    );
    // Negative control of the atomicity claim: no page shows a partial
    // (clipped-height) copy of the 200px-wide float.
    for (i, page) in pages.iter().enumerate() {
        let partial = page.items.iter().any(|it| match it {
            DisplayListItem::Rect { bounds, .. } | DisplayListItem::HitTestArea { bounds, .. } => {
                (bounds.0.size.width - 200.0).abs() < 0.6
                    && bounds.0.size.height > 1.0
                    && (bounds.0.size.height - 100.0).abs() > 0.6
            }
            _ => false,
        });
        assert!(!partial, "page {i} holds a TORN float fragment");
    }
}
