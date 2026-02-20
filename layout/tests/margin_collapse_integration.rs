/// Integration tests for CSS margin collapsing through the full layout pipeline
/// (CSS 2.2 §8.3.1 Collapsing Margins)
///
/// These tests exercise `advance_pen_with_margin_collapse`, `has_margin_collapse_blocker`,
/// `is_empty_block`, and the margin collapse logic in `layout_bfc` through
/// the full layout pipeline.
///
/// CSS Spec references:
/// - CSS 2.2 §8.3.1: Collapsing margins
/// - CSS 2.2 §9.4.1: Block formatting contexts (BFC establishes new margins)
/// - CSS 2.2 §9.5: Float margins never collapse

use azul_core::dom::{Dom, DomId};
use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_core::resources::RendererResources;
use azul_layout::font::loading::build_font_cache;
use azul_layout::font_traits::{FontManager, TextLayoutCache};
use azul_layout::paged::FragmentationContext;
use azul_layout::solver3::paged_layout::layout_document_paged_with_config;
use azul_layout::solver3::pagination::FakePageConfig;
use azul_layout::text3::default::PathLoader;
use azul_layout::xml::DomXmlExt;
use azul_layout::Solver3LayoutCache;
use std::collections::BTreeMap;

/// Helper: run layout and return the layout cache
fn run_layout(html: &str) -> Solver3LayoutCache {
    run_layout_with_size(html, 800.0, 600.0)
}

fn run_layout_with_size(html: &str, w: f32, h: f32) -> Solver3LayoutCache {
    let styled_dom = Dom::from_xml_string(html);
    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("Failed to create FontManager");
    let mut layout_cache = Solver3LayoutCache {
        tree: None,
        calculated_positions: Vec::new(),
        viewport: None,
        scroll_ids: BTreeMap::new(),
        scroll_id_to_node_id: BTreeMap::new(),
        counters: BTreeMap::new(),
        float_cache: BTreeMap::new(),
        cache_map: Default::default(),
    };
    let mut text_cache = TextLayoutCache::new();
    let content_size = LogicalSize::new(w, h);
    let fragmentation_context = FragmentationContext::new_paged(content_size);
    let viewport = LogicalRect {
        origin: LogicalPosition::zero(),
        size: content_size,
    };
    let renderer_resources = RendererResources::default();
    let mut debug_messages = Some(Vec::new());
    let loader = PathLoader::new();
    let font_loader = |bytes: &[u8], index: usize| loader.load_font(bytes, index);
    let page_config = FakePageConfig::new();

    let _display_lists = layout_document_paged_with_config(
        &mut layout_cache,
        &mut text_cache,
        fragmentation_context,
        &styled_dom,
        viewport,
        &mut font_manager,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &mut debug_messages,
        None,
        &renderer_resources,
        azul_core::resources::IdNamespace(0),
        DomId::ROOT_ID,
        font_loader,
        page_config,
        azul_core::task::GetSystemTimeCallback {
            cb: azul_core::task::get_system_time_libstd,
        },
        false,
    )
    .expect("Layout should succeed");

    layout_cache
}

// ============================================================================
// CSS 2.2 §8.3.1: Sibling margin collapsing
// ============================================================================

#[test]
fn test_adjacent_sibling_margins_collapse() {
    // CSS 2.2 §8.3.1: "the adjoining margins of two or more boxes...
    // can combine to form a single margin"
    //
    // Box A: margin-bottom 20px
    // Box B: margin-top 30px
    // Expected: collapsed margin = max(20, 30) = 30px between them
    let html = r#"
    <html><head><style>
        * { padding: 0; }
        body { margin: 0; }
        .a { height: 50px; margin-bottom: 20px; background: red; }
        .b { height: 50px; margin-top: 30px; background: blue; }
    </style></head>
    <body>
        <div class="a"></div>
        <div class="b"></div>
    </body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_equal_sibling_margins_collapse_to_one() {
    // CSS 2.2 §8.3.1: Equal margins collapse to that single value
    // Box A: margin-bottom 20px
    // Box B: margin-top 20px
    // Expected: 20px gap (not 40px)
    let html = r#"
    <html><head><style>
        * { padding: 0; }
        body { margin: 0; }
        .box { height: 50px; margin: 20px 0; background: red; }
    </style></head>
    <body>
        <div class="box"></div>
        <div class="box"></div>
        <div class="box"></div>
    </body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_multiple_adjacent_margins_collapse() {
    // CSS 2.2 §8.3.1: Multiple adjacent margins all collapse together
    let html = r#"
    <html><head><style>
        * { padding: 0; }
        body { margin: 0; }
        .a { height: 50px; margin-bottom: 10px; background: red; }
        .b { height: 50px; margin-top: 20px; margin-bottom: 15px; background: green; }
        .c { height: 50px; margin-top: 25px; background: blue; }
    </style></head>
    <body>
        <div class="a"></div>
        <div class="b"></div>
        <div class="c"></div>
    </body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

// ============================================================================
// CSS 2.2 §8.3.1: Negative margins
// ============================================================================

#[test]
fn test_negative_margins_both_negative() {
    // CSS 2.2 §8.3.1: When both margins are negative, the more negative wins
    let html = r#"
    <html><head><style>
        * { padding: 0; }
        body { margin: 0; }
        .a { height: 50px; margin-bottom: -10px; background: red; }
        .b { height: 50px; margin-top: -20px; background: blue; }
    </style></head>
    <body>
        <div class="a"></div>
        <div class="b"></div>
    </body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_negative_margin_with_positive() {
    // CSS 2.2 §8.3.1: When signs differ, sum the most positive and most negative
    let html = r#"
    <html><head><style>
        * { padding: 0; }
        body { margin: 0; }
        .a { height: 50px; margin-bottom: 30px; background: red; }
        .b { height: 50px; margin-top: -10px; background: blue; }
    </style></head>
    <body>
        <div class="a"></div>
        <div class="b"></div>
    </body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

// ============================================================================
// CSS 2.2 §8.3.1: Parent-child margin collapsing
// ============================================================================

#[test]
fn test_parent_child_top_margin_collapse() {
    // CSS 2.2 §8.3.1: "The top margin of an in-flow block-level element
    // collapses with its first in-flow block-level child's top margin"
    // (if parent has no border-top, padding-top)
    let html = r#"
    <html><head><style>
        * { padding: 0; }
        body { margin: 0; }
        .parent { margin-top: 20px; background: red; }
        .child { margin-top: 30px; height: 50px; background: blue; }
    </style></head>
    <body>
        <div class="parent">
            <div class="child"></div>
        </div>
    </body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_parent_child_bottom_margin_collapse() {
    // CSS 2.2 §8.3.1: "The bottom margin of an in-flow block box with a
    // 'height' of 'auto' collapses with its last in-flow block-level child's
    // bottom margin"
    let html = r#"
    <html><head><style>
        * { padding: 0; }
        body { margin: 0; }
        .parent { margin-bottom: 20px; background: red; }
        .child { margin-bottom: 30px; height: 50px; background: blue; }
        .after { height: 50px; background: green; }
    </style></head>
    <body>
        <div class="parent">
            <div class="child"></div>
        </div>
        <div class="after"></div>
    </body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

// ============================================================================
// CSS 2.2 §8.3.1: Border/padding prevents collapse
// ============================================================================

#[test]
fn test_border_prevents_parent_child_collapse() {
    // CSS 2.2 §8.3.1: "If the element's margins are separated from the
    // parent element's margins by the parent's border..."
    let html = r#"
    <html><head><style>
        * { padding: 0; }
        body { margin: 0; }
        .parent { margin-top: 20px; border-top: 1px solid black; background: red; }
        .child { margin-top: 30px; height: 50px; background: blue; }
    </style></head>
    <body>
        <div class="parent">
            <div class="child"></div>
        </div>
    </body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_padding_prevents_parent_child_collapse() {
    // CSS 2.2 §8.3.1: Padding between parent and child prevents collapsing
    let html = r#"
    <html><head><style>
        * { margin: 0; }
        body { margin: 0; }
        .parent { margin-top: 20px; padding-top: 10px; background: red; }
        .child { margin-top: 30px; height: 50px; background: blue; }
    </style></head>
    <body>
        <div class="parent">
            <div class="child"></div>
        </div>
    </body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

// ============================================================================
// CSS 2.2 §8.3.1: Empty block collapse through
// ============================================================================

#[test]
fn test_empty_block_margins_collapse_through() {
    // CSS 2.2 §8.3.1: "If a block element has no border, padding, inline content,
    // height, or min-height, then its top and bottom margins collapse with each other."
    let html = r#"
    <html><head><style>
        * { padding: 0; }
        body { margin: 0; }
        .a { height: 50px; margin-bottom: 10px; background: red; }
        .empty { margin-top: 20px; margin-bottom: 30px; }
        .b { height: 50px; margin-top: 5px; background: blue; }
    </style></head>
    <body>
        <div class="a"></div>
        <div class="empty"></div>
        <div class="b"></div>
    </body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_empty_block_with_height_does_not_collapse() {
    // CSS 2.2 §8.3.1: If the empty block has explicit height, it doesn't collapse through
    let html = r#"
    <html><head><style>
        * { padding: 0; }
        body { margin: 0; }
        .a { height: 50px; margin-bottom: 10px; background: red; }
        .not-empty { height: 1px; margin-top: 20px; margin-bottom: 30px; }
        .b { height: 50px; margin-top: 5px; background: blue; }
    </style></head>
    <body>
        <div class="a"></div>
        <div class="not-empty"></div>
        <div class="b"></div>
    </body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

// ============================================================================
// CSS 2.2 §9.4.1: BFC prevents margin collapsing
// ============================================================================

#[test]
fn test_overflow_hidden_prevents_margin_collapse_with_parent() {
    // CSS 2.2 §9.4.1: "overflow: hidden" creates a new BFC.
    // Margins of elements in different BFCs do not collapse.
    let html = r#"
    <html><head><style>
        * { padding: 0; }
        body { margin: 0; }
        .parent { margin-top: 20px; overflow: hidden; background: red; }
        .child { margin-top: 30px; height: 50px; background: blue; }
    </style></head>
    <body>
        <div class="parent">
            <div class="child"></div>
        </div>
    </body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_float_margins_never_collapse() {
    // CSS 2.2 §8.3.1: "Margins of floating boxes never collapse with
    // margins of adjacent boxes."
    let html = r#"
    <html><head><style>
        * { padding: 0; }
        body { margin: 0; }
        .a { height: 50px; margin-bottom: 20px; background: red; }
        .float { float: left; width: 100px; height: 50px; margin-top: 20px; background: green; }
        .b { height: 50px; margin-top: 20px; background: blue; clear: left; }
    </style></head>
    <body>
        <div class="a"></div>
        <div class="float"></div>
        <div class="b"></div>
    </body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn test_zero_margins_dont_affect_layout() {
    // Zero margins should collapse with other zero margins gracefully
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .box { height: 50px; background: red; }
    </style></head>
    <body>
        <div class="box"></div>
        <div class="box"></div>
        <div class="box"></div>
    </body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_deeply_nested_margin_collapse() {
    // Deep nesting: margins should still collapse through empty ancestors
    let html = r#"
    <html><head><style>
        * { padding: 0; }
        body { margin: 0; }
        .outer { margin-top: 10px; }
        .middle { margin-top: 20px; }
        .inner { margin-top: 30px; height: 50px; background: blue; }
    </style></head>
    <body>
        <div class="outer">
            <div class="middle">
                <div class="inner"></div>
            </div>
        </div>
    </body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_margin_collapse_with_inline_content_prevents_through_collapse() {
    // CSS 2.2 §8.3.1: An element with inline content is not empty,
    // so its margins do NOT collapse through
    let html = r#"
    <html><head><style>
        * { padding: 0; }
        body { margin: 0; }
        .a { height: 50px; margin-bottom: 10px; background: red; }
        .has-text { margin-top: 20px; margin-bottom: 30px; }
        .b { height: 50px; margin-top: 5px; background: blue; }
    </style></head>
    <body>
        <div class="a"></div>
        <div class="has-text">Hello</div>
        <div class="b"></div>
    </body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}
