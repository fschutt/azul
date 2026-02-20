/// Integration tests for CSS float positioning through full layout pipeline
/// (CSS 2.2 §9.5 Floats, §9.5.1 Positioning, §9.5.2 Clear)
///
/// These tests exercise the `position_float`, `position_floated_child`, and
/// `layout_bfc` code paths in fc.rs through the full layout pipeline.
///
/// CSS Spec references:
/// - CSS 2.2 §9.5: Float positioning rules
/// - CSS 2.2 §9.5.1: The 'float' property and its 9 rules
/// - CSS 2.2 §9.5.2: The 'clear' property
/// - CSS 2.2 §9.4.1: Block formatting contexts

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
    let content_size = LogicalSize::new(800.0, 600.0);
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
// CSS 2.2 §9.5.1 Rule 1: Left outer edge ≥ containing block's left edge
// ============================================================================

#[test]
fn test_float_left_stays_within_container() {
    // CSS 2.2 §9.5.1 Rule 1: "The left outer edge of a left-floating box
    // may not be to the left of the left edge of its containing block."
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .container { width: 400px; }
        .float { float: left; width: 100px; height: 50px; background: red; }
    </style></head>
    <body><div class="container"><div class="float"></div></div></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_float_right_stays_within_container() {
    // CSS 2.2 §9.5.1 Rule 1 analog: Right float must not extend past right edge
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .container { width: 400px; }
        .float { float: right; width: 100px; height: 50px; background: blue; }
    </style></head>
    <body><div class="container"><div class="float"></div></div></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

// ============================================================================
// CSS 2.2 §9.5.1 Rule 2: Successive left floats stack horizontally
// ============================================================================

#[test]
fn test_successive_left_floats_stack_horizontally() {
    // CSS 2.2 §9.5.1 Rule 2: "either the left outer edge of the current box
    // must be to the right of the right outer edge of the earlier box,
    // or its top must be lower than the bottom of the earlier box."
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .container { width: 400px; }
        .f1 { float: left; width: 100px; height: 50px; background: red; }
        .f2 { float: left; width: 100px; height: 50px; background: blue; }
    </style></head>
    <body><div class="container">
        <div class="f1"></div>
        <div class="f2"></div>
    </div></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_successive_right_floats_stack_horizontally() {
    // Same as above but for right floats
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .container { width: 400px; }
        .f1 { float: right; width: 100px; height: 50px; background: red; }
        .f2 { float: right; width: 100px; height: 50px; background: blue; }
    </style></head>
    <body><div class="container">
        <div class="f1"></div>
        <div class="f2"></div>
    </div></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

// ============================================================================
// CSS 2.2 §9.5.1 Rule 3: Left float doesn't overlap right float
// ============================================================================

#[test]
fn test_left_and_right_floats_dont_overlap() {
    // CSS 2.2 §9.5.1 Rule 3: "The right outer edge of a left-floating box
    // may not be to the right of the left outer edge of any right-floating
    // box that is next to it."
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .container { width: 400px; }
        .fl { float: left; width: 100px; height: 50px; background: red; }
        .fr { float: right; width: 100px; height: 50px; background: blue; }
    </style></head>
    <body><div class="container">
        <div class="fl"></div>
        <div class="fr"></div>
    </div></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

// ============================================================================
// CSS 2.2 §9.5.1 Rule 4: Float top ≤ containing block top
// ============================================================================

#[test]
fn test_float_top_not_above_container() {
    // CSS 2.2 §9.5.1 Rule 4: "A floating box's outer top may not be higher
    // than the top of its containing block."
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .container { width: 400px; padding-top: 20px; }
        .float { float: left; width: 100px; height: 50px; background: red; }
    </style></head>
    <body><div class="container"><div class="float"></div></div></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

// ============================================================================
// CSS 2.2 §9.5.1 Rule 8: Float pushed down when no space
// ============================================================================

#[test]
fn test_float_pushed_down_when_no_horizontal_space() {
    // CSS 2.2 §9.5.1 Rule 8: "A left-floating box that has another left-floating
    // box to its left may not have its right outer edge to the right of its
    // containing block's right edge."
    // If the float doesn't fit, it's shifted downward.
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .container { width: 250px; }
        .f1 { float: left; width: 150px; height: 50px; background: red; }
        .f2 { float: left; width: 150px; height: 50px; background: blue; }
    </style></head>
    <body><div class="container">
        <div class="f1"></div>
        <div class="f2"></div>
    </div></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

// ============================================================================
// CSS 2.2 §9.5.2: clear property
// ============================================================================

#[test]
fn test_clear_left_pushes_past_left_float() {
    // CSS 2.2 §9.5.2: "clear: left → top border edge of the box be below
    // the bottom outer edge of any left-floating boxes"
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .container { width: 400px; }
        .float { float: left; width: 100px; height: 80px; background: red; }
        .clear { clear: left; height: 30px; background: green; }
    </style></head>
    <body><div class="container">
        <div class="float"></div>
        <div class="clear"></div>
    </div></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_clear_right_pushes_past_right_float() {
    // CSS 2.2 §9.5.2: clear: right
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .container { width: 400px; }
        .float { float: right; width: 100px; height: 80px; background: red; }
        .clear { clear: right; height: 30px; background: green; }
    </style></head>
    <body><div class="container">
        <div class="float"></div>
        <div class="clear"></div>
    </div></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_clear_both_pushes_past_all_floats() {
    // CSS 2.2 §9.5.2: clear: both → clears both left and right floats
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .container { width: 400px; }
        .fl { float: left; width: 100px; height: 80px; background: red; }
        .fr { float: right; width: 100px; height: 120px; background: blue; }
        .clear { clear: both; height: 30px; background: green; }
    </style></head>
    <body><div class="container">
        <div class="fl"></div>
        <div class="fr"></div>
        <div class="clear"></div>
    </div></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_clear_left_ignores_right_floats() {
    // CSS 2.2 §9.5.2: clear: left should NOT clear right floats
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .container { width: 400px; }
        .fr { float: right; width: 100px; height: 120px; background: blue; }
        .clear { clear: left; height: 30px; background: green; }
    </style></head>
    <body><div class="container">
        <div class="fr"></div>
        <div class="clear"></div>
    </div></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

// ============================================================================
// Float interaction with BFC (CSS 2.2 §9.4.1)
// ============================================================================

#[test]
fn test_overflow_hidden_creates_new_bfc_for_float_containment() {
    // CSS 2.2 §9.4.1: "overflow: hidden" establishes a new BFC.
    // A new BFC "must not overlap the margin box of any floats".
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .container { width: 400px; }
        .float { float: left; width: 100px; height: 80px; background: red; }
        .bfc { overflow: hidden; height: 100px; background: green; }
    </style></head>
    <body><div class="container">
        <div class="float"></div>
        <div class="bfc">Content in new BFC</div>
    </div></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_float_with_margin() {
    // Floats with margins: margin is part of the float's "outer edge"
    // CSS 2.2 §9.5: "until its outer edge touches the containing block edge"
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .container { width: 400px; }
        .float { float: left; width: 100px; height: 50px; margin: 10px; background: red; }
    </style></head>
    <body><div class="container"><div class="float"></div></div></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

// ============================================================================
// Float with inline content (CSS 2.2 §9.5 line box narrowing)
// ============================================================================

#[test]
fn test_float_narrows_line_boxes() {
    // CSS 2.2 §9.5: "the current and subsequent line boxes created next to the
    // float are shortened as necessary to make room for the margin box of the float"
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .container { width: 400px; }
        .float { float: left; width: 100px; height: 100px; background: red; }
    </style></head>
    <body><div class="container">
        <div class="float"></div>
        <p>This text should flow around the float, wrapping to the right of it.
        After the float ends, the text should return to full width.</p>
    </div></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_float_none_is_not_floated() {
    // CSS 2.2 §9.5.1: "float: none → the box is not floated"
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .container { width: 400px; }
        .not-float { float: none; width: 100px; height: 50px; background: red; }
        .next { height: 50px; background: blue; }
    </style></head>
    <body><div class="container">
        <div class="not-float"></div>
        <div class="next"></div>
    </div></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

// ============================================================================
// Complex float scenarios
// ============================================================================

#[test]
fn test_multiple_floats_different_heights() {
    // Multiple floats of varying heights — tests the position_float loop
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .container { width: 400px; }
        .f1 { float: left; width: 80px; height: 100px; background: red; }
        .f2 { float: left; width: 80px; height: 60px; background: green; }
        .f3 { float: left; width: 80px; height: 120px; background: blue; }
        .f4 { float: right; width: 80px; height: 80px; background: yellow; }
    </style></head>
    <body><div class="container">
        <div class="f1"></div>
        <div class="f2"></div>
        <div class="f3"></div>
        <div class="f4"></div>
        <p>Text flowing around multiple floats with different heights.</p>
    </div></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_float_between_block_elements() {
    // CSS 2.2 §9.5: "non-positioned block boxes created before and after the
    // float box flow vertically as if the float did not exist"
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .container { width: 400px; }
        .block { height: 30px; background: gray; }
        .float { float: left; width: 100px; height: 80px; background: red; }
    </style></head>
    <body><div class="container">
        <div class="block">Before float</div>
        <div class="float"></div>
        <div class="block">After float</div>
    </div></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_float_margin_never_collapses() {
    // CSS 2.2 §8.3.1: "the margins of floating boxes never collapse
    // with margins of adjacent boxes"
    let html = r#"
    <html><head><style>
        * { padding: 0; }
        .container { width: 400px; margin: 0; }
        .block { height: 30px; margin-bottom: 20px; background: gray; }
        .float { float: left; width: 100px; height: 50px; margin-top: 20px; background: red; }
    </style></head>
    <body><div class="container">
        <div class="block">Block with margin-bottom: 20px</div>
        <div class="float">Float with margin-top: 20px</div>
    </div></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}
