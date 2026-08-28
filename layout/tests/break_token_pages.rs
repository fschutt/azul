//! K30b part 2 + K30c skeleton: the NG-style page loop over break tokens.
//! Laws pinned here (design doc §6.2): progress (every page advances the
//! token), conservation (Σ per-page content = unfragmented content, for
//! avoid-free block stacks), nested resume (a wrapper container splits via
//! ResumeIn and CONTINUES, not restarts), monolith termination.

use azul_core::dom::{Dom, DomId};
use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_core::resources::RendererResources;
use azul_layout::font::loading::build_font_cache;
use azul_layout::font_traits::{FontManager, TextLayoutCache};
use azul_layout::solver3::break_token::{BreakToken, ChildBreakEntry};
use azul_layout::solver3::paged_layout::{layout_document_tokenized, TokenizedPage};
use azul_layout::text3::default::PathLoader;
use azul_layout::xml::DomXmlExt;
use azul_layout::Solver3LayoutCache;
use std::collections::HashMap;

fn run(html: &str, page_h: f32) -> Vec<TokenizedPage> {
    let styled_dom = Dom::from_xml_string(html);
    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("FontManager");
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
        size: LogicalSize::new(800.0, page_h),
    };
    let rr = RendererResources::default();
    let mut dbg = Some(Vec::new());
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
        page_h,
        16,
    )
    .expect("tokenized layout")
}

const FLAT: &str = r#"
<html><head><style>
    * { margin: 0; padding: 0; }
    .p { height: 150px; }
</style></head>
<body>
    <div class="p">one</div><div class="p">two</div><div class="p">three</div>
    <div class="p">four</div><div class="p">five</div><div class="p">six</div>
</body></html>"#;

#[test]
fn flat_stack_fills_pages_and_terminates() {
    // 6 × 150px on 400px pages: 2 blocks per page → 3 pages, then None.
    let pages = run(FLAT, 400.0);
    assert_eq!(pages.len(), 3, "{pages:?}");
    for (i, p) in pages.iter().enumerate() {
        assert!(
            (p.content_block_size - 300.0).abs() < 0.6,
            "page {i}: two 150px blocks fit per 400px page, got {}",
            p.content_block_size
        );
    }
    assert!(pages[0].outgoing.is_some());
    assert!(pages[1].outgoing.is_some());
    assert!(pages[2].outgoing.is_none(), "the document FINISHES");

    // Conservation: Σ fitted content = the unfragmented 900px.
    let total: f32 = pages.iter().map(|p| p.content_block_size).sum();
    assert!((total - 900.0).abs() < 1.0, "conservation: {total}");

    // Progress: consecutive tokens differ.
    assert_ne!(pages[0].outgoing, pages[1].outgoing);
}

const NESTED: &str = r#"
<html><head><style>
    * { margin: 0; padding: 0; }
    .p { height: 150px; }
    .wrap { display: block; }
</style></head>
<body>
    <div class="wrap">
        <div class="p">one</div><div class="p">two</div><div class="p">three</div>
        <div class="p">four</div><div class="p">five</div><div class="p">six</div>
    </div>
</body></html>"#;

#[test]
fn nested_container_splits_via_resume_in_and_continues() {
    // The SAME six blocks inside one wrapper div: the wrapper must SPLIT
    // (ResumeIn), not move whole — same page extents as the flat stack.
    let pages = run(NESTED, 400.0);
    assert_eq!(pages.len(), 3, "{pages:?}");
    let total: f32 = pages.iter().map(|p| p.content_block_size).sum();
    assert!(
        (total - 900.0).abs() < 1.0,
        "conservation through the nested split: {total} ({pages:?})"
    );
    // The outgoing token of page 0 resumes INSIDE the wrapper.
    let Some(BreakToken::Block(root_tok)) = &pages[0].outgoing else {
        panic!("page 0 must emit a block token: {:?}", pages[0].outgoing);
    };
    assert!(
        matches!(
            root_tok.children.first(),
            Some(ChildBreakEntry::ResumeIn { .. })
        ),
        "the wrapper CONTINUES via ResumeIn (not BreakBefore = restart): {root_tok:?}"
    );
}

#[test]
fn monolith_taller_than_every_page_terminates_with_overflow() {
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .big { height: 900px; }
    </style></head>
    <body><div class="big">tall</div></body></html>"#;
    let pages = run(html, 400.0);
    assert_eq!(
        pages.len(),
        1,
        "a monolith places ONCE (overflowing), never loops: {pages:?}"
    );
    assert!(pages[0].outgoing.is_none());
    assert!(
        pages[0].content_block_size >= 899.0,
        "monolith overflows the fragmentainer instead of tearing: {}",
        pages[0].content_block_size
    );
}

// ---------------------------------------------------------------------------
// K31: margin truncation at unforced breaks + forced-break propagation
// ---------------------------------------------------------------------------

#[test]
fn unforced_break_truncates_the_resumed_childs_top_margin() {
    // b1 140px; b2/b3 140px with margin-top 20. Continuous: b1 0..140,
    // b2 160..300, b3 320..460. Page 300: b1+b2 fit exactly (300); b3's
    // margin-advanced pen (320) overflows → unforced break before b3.
    // Page 2: css-break-3 §5.2 — b3 starts FLUSH (margin truncated): 140,
    // not 160.
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .a { height: 140px; }
        .b { height: 140px; margin-top: 20px; }
    </style></head>
    <body>
        <div class="a">one</div>
        <div class="b">two</div>
        <div class="b">three</div>
    </body></html>"#;
    let pages = run(html, 300.0);
    assert_eq!(pages.len(), 2, "{pages:?}");
    assert!(
        (pages[0].content_block_size - 300.0).abs() < 0.6,
        "page 0 holds b1 + margin + b2 exactly: {}",
        pages[0].content_block_size
    );
    assert!(
        (pages[1].content_block_size - 140.0).abs() < 0.6,
        "the resumed child's 20px top margin TRUNCATES at the unforced \
         break (got {} — 160 would mean the margin survived)",
        pages[1].content_block_size
    );
}

#[test]
fn forced_page_break_node_splits_and_preserves_the_next_margin() {
    // <pagebreak/> (UA break-before: page) after b1 forces a boundary even
    // though everything would fit one 400px page. FORCED breaks preserve
    // the adjoining margin (css-break-3 §5.2 truncates UNFORCED only):
    // page 2 = 30px margin + 100px block = 130.
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .p { height: 100px; }
        .m { height: 100px; margin-top: 30px; }
    </style></head>
    <body>
        <div class="p">one</div>
        <pagebreak/>
        <div class="m">two</div>
    </body></html>"#;
    let pages = run(html, 400.0);
    assert_eq!(
        pages.len(),
        2,
        "forced break splits a fitting page: {pages:?}"
    );
    assert!(
        (pages[0].content_block_size - 100.0).abs() < 0.6,
        "page 0 ends at the forced boundary: {}",
        pages[0].content_block_size
    );
    assert!(
        (pages[1].content_block_size - 130.0).abs() < 0.6,
        "a FORCED break preserves the next child's 30px margin (got {} —          100 would mean it was wrongly truncated)",
        pages[1].content_block_size
    );
}
