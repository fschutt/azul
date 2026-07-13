//! MOCK FONT metrics: text layout as arithmetic.
//!
//! `Azul Mock Mono` advances exactly 0.5 em per glyph and has an 0.8/0.2 em
//! ascent/descent (see `layout/src/text3/mock_fonts.rs`,
//! `scripts/gen_mock_fonts.py`). At `font-size: 20px` that is exactly 10 px
//! per character, so `"HELLO"` is exactly 50 px wide — no tolerance, no
//! "roughly". Everything downstream (caret offsets, selection rectangles,
//! line breaks, bidi run widths) becomes assertable arithmetic.
//!
//! These tests also guard the two regressions found while fixing the
//! "8 font-families collapse onto 2 FontIds" bug:
//!  1. a family registered in memory must be found by the FAST resolver
//!     (which previously only looked at fonts that exist as files on disk);
//!  2. N distinct families must produce N distinct `FontId`s.

use std::collections::{BTreeMap, HashMap};

use azul_core::{
    dom::{Dom, DomId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
};
use azul_layout::{
    font::loading::build_font_cache,
    font_traits::{FontManager, TextLayoutCache},
    paged::FragmentationContext,
    solver3::{pagination::FakePageConfig, paged_layout::layout_document_paged_with_config},
    text3::default::PathLoader,
    xml::DomXmlExt,
    Solver3LayoutCache,
};

fn empty_cache() -> Solver3LayoutCache {
    Solver3LayoutCache {
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
    }
}

/// Lay out `html` and return the used size of the node at `node_index`.
fn layout_and_measure(
    html: &str,
    node_index: usize,
    with_registry: bool,
) -> LogicalSize {
    let styled_dom = Dom::from_xml_string(html);

    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("font manager");
    if with_registry {
        // The PRODUCTION configuration: a live registry means chain
        // resolution takes `resolve_font_chains_fast`.
        let registry = azul_layout::FcFontRegistry::new();
        registry.spawn_scout_and_builders();
        font_manager = font_manager.with_registry(registry);
    }

    let mut layout_cache = empty_cache();
    let mut text_cache = TextLayoutCache::new();
    let content_size = LogicalSize::new(400.0, 300.0);
    let viewport = LogicalRect {
        origin: LogicalPosition::zero(),
        size: content_size,
    };
    let loader = PathLoader::new();
    let font_loader =
        |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
            loader.load_font_shared(bytes, index)
        };

    layout_document_paged_with_config(
        &mut layout_cache,
        &mut text_cache,
        FragmentationContext::new_paged(content_size),
        &styled_dom,
        viewport,
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
    .expect("layout should succeed");

    let tree = layout_cache.tree.as_ref().expect("layout tree");
    let mut i = 0;
    while let Some(n) = tree.get(i) {
        println!("  node {i}: used_size={:?}", n.used_size);
        i += 1;
    }
    tree.get(node_index)
        .expect("node exists")
        .used_size
        .expect("used_size")
}

const MOCK_HTML: &str = r#"
<html>
  <head><style>
    .row { display: flex; flex-direction: row; }
    #t { font-family: "Azul Mock Mono"; font-size: 20px; }
    #w { font-family: "Azul Mock Wide"; font-size: 20px; }
  </style></head>
  <body>
    <div class="row"><div id="t">HELLO</div></div>
  </body>
</html>
"#;

/// Layout tree node 3 is the TEXT node (0 body, 1 div.row, 2 div#t, 3 text).
/// Its used_size is the shaped text box: this is what must be exact.
const TEXT_NODE: usize = 3;

/// 5 glyphs x 0.5 em x 20 px = EXACTLY 50 px wide, and the line box is
/// exactly ascent+descent = 0.8+0.2 em = 20 px tall. No tolerance.
#[test]
fn mock_mono_five_chars_is_exactly_50px() {
    let size = layout_and_measure(MOCK_HTML, TEXT_NODE, false);
    println!("legacy-resolver used_size = {size:?}");
    assert!(
        (size.width - 50.0).abs() < 0.01,
        "'HELLO' in Azul Mock Mono @20px must be exactly 50px, got {}",
        size.width
    );
    assert!(
        (size.height - 20.0).abs() < 0.01,
        "line box must be exactly ascent+descent = 20px, got {}",
        size.height
    );
}

/// Same, but with a live `FcFontRegistry` — the production path. Before the
/// fix, `resolve_font_chains_fast` never saw in-memory fonts, so this
/// silently rendered in a system fallback and the width was NOT 50px.
#[test]
fn mock_mono_exact_width_on_fast_resolver_path() {
    let size = layout_and_measure(MOCK_HTML, TEXT_NODE, true);
    println!("fast-resolver used_size = {size:?}");
    assert!(
        (size.width - 50.0).abs() < 0.01,
        "fast resolver must find the in-memory family: expected 50px, got {}",
        size.width
    );
}

/// The wide mock advances 1 em: the same 5 characters are exactly 100 px.
/// Two families, two different widths — proof the family actually selects
/// the font instead of collapsing onto a shared fallback.
#[test]
fn mock_wide_five_chars_is_exactly_100px() {
    let html = MOCK_HTML.replace(r#"id="t""#, r#"id="w""#);
    let size = layout_and_measure(&html, TEXT_NODE, true);
    println!("wide used_size = {size:?}");
    assert!(
        (size.width - 100.0).abs() < 0.01,
        "'HELLO' in Azul Mock Wide @20px must be exactly 100px, got {}",
        size.width
    );
}

/// LINE BREAKING as arithmetic: this is the exact DOM of
/// `e2e/mock-font-exact-metrics.json`, so the numbers asserted there are
/// verified here too.
///
/// `"HELLO HELLO"` = 2 words x 50 px + a 10 px space = 110 px. In a 60 px
/// box it must wrap into EXACTLY two lines => 40 px tall. That is only true
/// if every glyph advances exactly 10 px.
#[test]
fn mock_mono_line_breaking_is_exact() {
    let html = r#"
<html>
  <head><style>
    #one  { width: 60px; font-family: "Azul Mock Mono"; font-size: 20px; }
    #two  { width: 60px; font-family: "Azul Mock Mono"; font-size: 20px; }
    #wide { width: 60px; font-family: "Azul Mock Wide"; font-size: 20px; }
  </style></head>
  <body>
    <div id="one">HELLO</div>
    <div id="two">HELLO HELLO</div>
    <div id="wide">HELLO</div>
  </body>
</html>
"#;
    // 0 root, 1 body, 2 #one, 3 text, 4 #two, 5 text, 6 #wide, 7 text
    let one = layout_and_measure(html, 2, true);
    let two = layout_and_measure(html, 4, true);
    let wide = layout_and_measure(html, 6, true);
    println!("one={one:?} two={two:?} wide={wide:?}");
    assert!(
        (one.height - 20.0).abs() < 0.01,
        "one line of Azul Mock Mono @20px must be exactly 20px tall, got {}",
        one.height
    );
    assert!(
        (two.height - 40.0).abs() < 0.01,
        "'HELLO HELLO' (110px) in a 60px box must wrap to exactly 2 lines = 40px, got {}",
        two.height
    );
    assert!(
        (wide.height - 20.0).abs() < 0.01,
        "one line of Azul Mock Wide @20px must be exactly 20px tall, got {}",
        wide.height
    );
}

/// N distinct font-families must produce N distinct `FontId`s.
///
/// This is the regression test for the reported bug ("8 families collapse
/// onto 2 FontIds"). It uses mock families precisely BECAUSE a family that
/// the machine does not have installed legitimately falls back — the test
/// registers eight families that definitely exist, so any collapse is a
/// real engine bug rather than a missing system font.
#[test]
fn eight_families_produce_eight_distinct_font_ids() {
    use azul_layout::text3::mock_fonts::{mock_font_ranges, MOCK_MONO_TTF};

    let fc_cache = build_font_cache();
    let mut fm = FontManager::<azul_css::props::basic::FontRef>::new(fc_cache)
        .expect("font manager");

    let mut ids = std::collections::BTreeSet::new();
    for i in 0..8 {
        let family = format!("Azul Test Family {i}");
        let id = fm.register_named_font(&family, MOCK_MONO_TTF, mock_font_ranges());
        assert!(ids.insert(id), "family {family} reused FontId {id:?}");
    }
    assert_eq!(ids.len(), 8, "8 distinct families must yield 8 distinct FontIds");

    // Registering the same family twice must be idempotent (same id), not
    // mint a second id for the same bytes.
    let again = fm.register_named_font("Azul Test Family 0", MOCK_MONO_TTF, mock_font_ranges());
    assert!(ids.contains(&again), "re-registration must reuse the FontId");
}
