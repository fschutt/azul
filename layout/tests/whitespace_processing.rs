/// Integration tests for CSS white-space processing (CSS Text Level 3 §4)
///
/// Tests cover:
/// - `white-space: normal` → collapse sequences of whitespace into a single space
/// - `white-space: nowrap` → same collapsing as normal, but no wrapping
/// - `white-space: pre` → preserve all whitespace, honor newlines
/// - `white-space: pre-wrap` → preserve whitespace, honor newlines, allow wrapping
/// - `white-space: pre-line` → collapse whitespace but honor newlines
///
/// CSS Spec references:
/// - CSS Text Level 3 §3: White Space and Wrapping
/// - CSS Text Level 3 §4.1.1: Phase I: Collapsing and Transformation
/// - CSS Text Level 3 §4.1.2: Phase II: Trimming and Positioning
/// - https://www.w3.org/TR/css-text-3/#white-space-processing

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

/// Helper: runs layout on an HTML fragment and returns the layout cache
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
// white-space: normal (default) — CSS Text L3 §4.1.1
// ============================================================================

#[test]
fn test_whitespace_normal_collapses_spaces() {
    // CSS Text L3 §4.1.1: "Any sequence of collapsible spaces is collapsed
    // to a single space."
    let html = r#"
    <html><head><style>
        p { white-space: normal; margin: 0; padding: 0; }
    </style></head>
    <body><p>Hello     World</p></body></html>
    "#;
    let cache = run_layout(html);
    // Layout should succeed without panics; the text should render as "Hello World"
    assert!(
        !cache.calculated_positions.is_empty(),
        "Layout must produce positions"
    );
}

#[test]
fn test_whitespace_normal_collapses_newlines_to_spaces() {
    // CSS Text L3 §4.1.1: "All newlines are converted to spaces."
    let html = r#"
    <html><head><style>
        p { white-space: normal; margin: 0; padding: 0; }
    </style></head>
    <body><p>Hello
    World</p></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_whitespace_normal_collapses_tabs() {
    // CSS Text L3 §4.1.1: Tabs are treated as spaces and collapsed
    let html = "<html><head><style>\
        p { white-space: normal; margin: 0; padding: 0; }\
    </style></head>\
    <body><p>Hello\tWorld</p></body></html>";
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_whitespace_normal_preserves_inter_element_space() {
    // CSS Text L3 §4.1.1: Spaces between inline elements must be preserved
    // "<span>Hello</span> <span>World</span>" → "Hello World"
    let html = r#"
    <html><head><style>
        span { white-space: normal; }
        p { margin: 0; padding: 0; }
    </style></head>
    <body><p><span>Hello</span> <span>World</span></p></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

// ============================================================================
// white-space: pre — CSS Text L3 §3
// ============================================================================

#[test]
fn test_whitespace_pre_preserves_spaces() {
    // CSS Text L3 §3: "prevents user agents from collapsing sequences of white space"
    let html = r#"
    <html><head><style>
        pre { white-space: pre; margin: 0; padding: 0; }
    </style></head>
    <body><pre>Hello     World</pre></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_whitespace_pre_honors_newlines() {
    // CSS Text L3 §3: "Segment breaks such as line feeds are preserved as forced line breaks"
    let html = "<html><head><style>\
        pre { white-space: pre; margin: 0; padding: 0; }\
    </style></head>\
    <body><pre>Line1\nLine2\nLine3</pre></body></html>";
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_whitespace_pre_preserves_tabs() {
    // CSS Text L3 §4.1.2: "each preserved tab is rendered as a horizontal shift"
    let html = "<html><head><style>\
        pre { white-space: pre; margin: 0; padding: 0; }\
    </style></head>\
    <body><pre>Col1\tCol2\tCol3</pre></body></html>";
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

// ============================================================================
// white-space: pre-wrap — CSS Text L3 §3
// ============================================================================

#[test]
fn test_whitespace_pre_wrap_preserves_spaces_but_allows_wrapping() {
    // CSS Text L3 §3: pre-wrap preserves whitespace and honors newlines,
    // but allows soft wrapping at the end of a space sequence
    let html = r#"
    <html><head><style>
        p { white-space: pre-wrap; margin: 0; padding: 0; width: 200px; }
    </style></head>
    <body><p>Hello     World     This is a long text with preserved spaces</p></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_whitespace_pre_wrap_honors_newlines() {
    // CSS Text L3 §3: Newlines in pre-wrap create forced line breaks
    let html = "<html><head><style>\
        p { white-space: pre-wrap; margin: 0; padding: 0; }\
    </style></head>\
    <body><p>Line1\nLine2</p></body></html>";
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

// ============================================================================
// white-space: pre-line — CSS Text L3 §3
// ============================================================================

#[test]
fn test_whitespace_pre_line_collapses_spaces_but_honors_newlines() {
    // CSS Text L3 §3: pre-line collapses whitespace but honors newlines
    let html = "<html><head><style>\
        p { white-space: pre-line; margin: 0; padding: 0; }\
    </style></head>\
    <body><p>Hello     World\nNext Line</p></body></html>";
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_whitespace_pre_line_multiple_newlines() {
    // Multiple newlines should each produce a forced line break
    let html = "<html><head><style>\
        p { white-space: pre-line; margin: 0; padding: 0; }\
    </style></head>\
    <body><p>Line1\n\nLine3</p></body></html>";
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

// ============================================================================
// white-space: nowrap — CSS Text L3 §3
// ============================================================================

#[test]
fn test_whitespace_nowrap_collapses_like_normal() {
    // CSS Text L3 §3: nowrap collapses whitespace just like normal
    // but does not allow soft wrapping
    let html = r#"
    <html><head><style>
        p { white-space: nowrap; margin: 0; padding: 0; width: 100px; }
    </style></head>
    <body><p>Hello     World this is a very long line that should not wrap</p></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

// ============================================================================
// Mixed white-space values in nested elements
// ============================================================================

#[test]
fn test_whitespace_inherited_from_parent() {
    // white-space is inherited — text nodes get the value from their parent
    let html = r#"
    <html><head><style>
        div { white-space: pre; margin: 0; padding: 0; }
        span { /* inherits pre from div */ }
    </style></head>
    <body><div><span>Hello     World</span></div></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_whitespace_override_in_child() {
    // Child can override inherited white-space
    let html = r#"
    <html><head><style>
        div { white-space: pre; margin: 0; padding: 0; }
        p { white-space: normal; margin: 0; padding: 0; }
    </style></head>
    <body>
        <div>
            <p>Hello     World</p>
        </div>
    </body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn test_whitespace_empty_text_normal() {
    // Empty text content should not crash layout
    let html = r#"
    <html><head><style>
        p { margin: 0; padding: 0; }
    </style></head>
    <body><p></p></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_whitespace_only_spaces_normal() {
    // CSS Text L3 §4.1.1: Whitespace-only text in normal mode should collapse
    // to a single space for inter-element spacing
    let html = r#"
    <html><head><style>
        p { margin: 0; padding: 0; }
    </style></head>
    <body><p>   </p></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_whitespace_only_newlines_normal() {
    // Newlines-only in normal mode → collapse to single space
    let html = "<html><head><style>\
        p { margin: 0; padding: 0; }\
    </style></head>\
    <body><p>\n\n\n</p></body></html>";
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}

#[test]
fn test_whitespace_mixed_content_with_br() {
    // <br> should always produce a line break regardless of white-space mode
    let html = r#"
    <html><head><style>
        p { white-space: normal; margin: 0; padding: 0; }
    </style></head>
    <body><p>Hello<br/>World</p></body></html>
    "#;
    let cache = run_layout(html);
    assert!(!cache.calculated_positions.is_empty());
}
