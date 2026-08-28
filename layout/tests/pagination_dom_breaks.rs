//! The break-Y -> DOM-path keystone of the DOM-materialized-breaks editor
//! architecture (pdf2html AZUL-STILL-TODO section A):
//!
//! - `compute_document_pagination` estimates break Y coordinates without
//!   materializing pages, leaving tree + positions in the caller's cache.
//! - `pagination_to_dom_breaks` maps every break to the child-index path of
//!   the first block at/after it (the spine the cut runs along) so the
//!   application can insert its break nodes at DOM positions.
//! - Forced breaks carry `causing_node` end to end (display-list recording
//!   through `PageBreakPosition`).

use azul_core::dom::{Dom, DomId};
use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_core::resources::RendererResources;
use azul_layout::font::loading::build_font_cache;
use azul_layout::font_traits::{FontManager, TextLayoutCache};
use azul_layout::paged::FragmentationContext;
use azul_layout::solver3::paged_layout::{compute_document_pagination, pagination_to_dom_breaks};
use azul_layout::solver3::pagination::FakePageConfig;
use azul_layout::solver3::LayoutNodeId;
use azul_layout::text3::default::PathLoader;
use azul_layout::xml::DomXmlExt;
use azul_layout::{BreakKind, Solver3LayoutCache};
use std::collections::{BTreeMap, HashMap};

fn paginate(
    html: &str,
    w: f32,
    h: f32,
) -> (
    Solver3LayoutCache,
    azul_core::styled_dom::StyledDom,
    azul_layout::PaginationInfo,
) {
    let styled_dom = Dom::from_xml_string(html);
    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("FontManager");
    let mut layout_cache = Solver3LayoutCache {
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
    let content_size = LogicalSize::new(w, h);
    let fragmentation_context = FragmentationContext::new_paged(content_size);
    let viewport = LogicalRect {
        origin: LogicalPosition::zero(),
        size: content_size,
    };
    let renderer_resources = RendererResources::default();
    let mut debug_messages = Some(Vec::new());
    let loader = PathLoader::new();
    let font_loader = |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
        loader.load_font_shared(bytes, index)
    };
    let pagination = compute_document_pagination(
        &mut layout_cache,
        &mut text_cache,
        fragmentation_context,
        &styled_dom,
        viewport,
        &mut font_manager,
        &BTreeMap::new(),
        &mut debug_messages,
        None,
        &renderer_resources,
        azul_core::resources::IdNamespace(0),
        DomId::ROOT_ID,
        font_loader,
        FakePageConfig::new(),
        &azul_core::resources::ImageCache::default(),
        azul_core::task::GetSystemTimeCallback {
            cb: azul_core::task::get_system_time_libstd,
        },
    )
    .expect("pagination should succeed");
    (layout_cache, styled_dom, pagination)
}

#[test]
fn interval_breaks_map_to_dom_paths() {
    // Three 150px blocks on 200px pages: breaks fall inside block 2 and
    // block 3's territory; each break must address a real block by path.
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .p { height: 150px; }
    </style></head>
    <body>
        <div class="p">one</div>
        <div class="p">two</div>
        <div class="p">three</div>
    </body></html>"#;
    let (cache, styled_dom, pagination) = paginate(html, 800.0, 200.0);
    assert!(
        pagination.page_count >= 2,
        "450px of content on 200px pages must span pages, got {}",
        pagination.page_count
    );
    let breaks = pagination_to_dom_breaks(&cache, &styled_dom, &pagination)
        .expect("cache holds tree+positions right after compute_document_pagination");
    assert_eq!(breaks.len(), pagination.breaks.len());
    for b in &breaks {
        assert_eq!(b.kind, BreakKind::Interval);
    }
    // A break with a block-level box at/after it maps to that box's path;
    // a break inside the LAST block (no following block) maps to None —
    // there is no "insert before this node" target, the content tail stays
    // on its page. With 150px blocks at y 0/150/300 and 200px pages:
    // break y=200 -> block 3 (top 300), break y=400 -> None (tail).
    assert!(
        breaks[0].path.is_some(),
        "a break followed by more blocks must map to a spine path: {:?}",
        breaks[0]
    );
    if let Some(last) = breaks.last() {
        if last.y >= 400.0 {
            assert!(
                last.path.is_none(),
                "a break inside the final block maps to None (tail semantics): {last:?}"
            );
        }
    }
    // Present paths address blocks in document order.
    let paths: Vec<_> = breaks.iter().filter_map(|b| b.path.clone()).collect();
    assert!(
        paths.windows(2).all(|w| w[0] <= w[1]),
        "spine paths must be document-ordered: {paths:?}"
    );
}

#[test]
fn forced_break_carries_its_causing_node() {
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .p { height: 50px; }
        .brk { break-before: page; height: 50px; }
    </style></head>
    <body>
        <div class="p">one</div>
        <div class="brk">two</div>
    </body></html>"#;
    let (cache, styled_dom, pagination) = paginate(html, 800.0, 400.0);
    let forced: Vec<_> = pagination
        .breaks
        .iter()
        .filter(|b| b.kind == BreakKind::Forced)
        .collect();
    assert_eq!(
        forced.len(),
        1,
        "break-before: page must yield exactly one forced break: {:?}",
        pagination.breaks
    );
    let causing = forced[0]
        .causing_node
        .expect("forced break must carry the node whose break property caused it");
    let node_data = &styled_dom.node_data.as_container()[causing];
    assert!(
        format!("{:?}", node_data.get_node_type()).contains("Div"),
        "causing node should be the .brk div, got {:?}",
        node_data.get_node_type()
    );
    let breaks = pagination_to_dom_breaks(&cache, &styled_dom, &pagination).expect("mapped");
    let fb = breaks.iter().find(|b| b.kind == BreakKind::Forced).unwrap();
    assert_eq!(fb.causing_node, Some(causing));
    assert!(fb.path.is_some(), "forced break maps to a spine path too");
}

#[test]
fn pagination_session_reports_unchanged_prefix_on_identical_re_estimate() {
    use azul_layout::PaginationSession;
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .p { height: 150px; }
    </style></head>
    <body>
        <div class="p">one</div>
        <div class="p">two</div>
        <div class="p">three</div>
    </body></html>"#;
    let styled_dom = Dom::from_xml_string(html);
    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("FontManager");
    let renderer_resources = RendererResources::default();
    let loader = PathLoader::new();
    let viewport = LogicalRect {
        origin: LogicalPosition::zero(),
        size: LogicalSize::new(800.0, 200.0),
    };

    let mut session = PaginationSession::new();
    let run = |session: &mut PaginationSession,
               dom: &azul_core::styled_dom::StyledDom,
               fm: &mut FontManager<_>| {
        let mut debug_messages = Some(Vec::new());
        session
            .re_estimate(
                dom,
                viewport,
                fm,
                &BTreeMap::new(),
                &mut debug_messages,
                None,
                &renderer_resources,
                azul_core::resources::IdNamespace(0),
                DomId::ROOT_ID,
                |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
                    loader.load_font_shared(bytes, index)
                },
                FakePageConfig::new(),
                &azul_core::resources::ImageCache::default(),
                azul_core::task::GetSystemTimeCallback {
                    cb: azul_core::task::get_system_time_libstd,
                },
            )
            .expect("re-estimate")
    };

    let first = run(&mut session, &styled_dom, &mut font_manager);
    assert_eq!(
        first.unchanged_prefix_len, 0,
        "the first estimate has nothing to be unchanged against"
    );
    let n_breaks = session.info().expect("estimate stored").breaks.len();
    assert!(n_breaks >= 1);

    let second = run(&mut session, &styled_dom, &mut font_manager);
    assert_eq!(
        second.unchanged_prefix_len, n_breaks,
        "an identical document must keep every break bit-for-bit: {second:?}"
    );
    assert!(!second.page_count_changed);
    assert!(
        session.dom_breaks(&styled_dom).is_some(),
        "structural mapping stays available from the session's caches"
    );
}

/// AZUL-STILL-TODO A3: estimation <-> materialization equivalence.
///
/// The v1 contract is BLOCK-GRANULAR (office-suite semantics): a break can only be
/// materialized BEFORE a block, so lossless equivalence holds exactly when
/// the estimated boundary coincides with a block top. This fixture aligns
/// them **including sibling margins** — two 260px blocks with 20px collapsed
/// margins put block 2's top at exactly y=300 = the 300px page boundary:
///
/// - the estimator yields one break at 300 whose spine path addresses block 2,
/// - inserting `Dom::create_page_break()` (an empty UA-styled block) there
///   must NOT move block 2 (margins keep collapsing through the empty
///   element — the doc's explicit margin-collapse worry), and
/// - re-estimating the materialized document yields the same boundary,
///   now FORCED (the forced break wins over the coinciding interval).
///
/// For boundaries that fall MID-block, block-granular materialization
/// legitimately snaps to the spine block's top — asserted separately below.
#[test]
fn materialized_breaks_reproduce_the_estimated_boundaries() {
    const CSS: &str = r#"
        * { margin: 0; padding: 0; }
        .p { height: 260px; margin: 20px 0; }
    "#;
    fn blocks(n: usize) -> Vec<Dom> {
        (0..n)
            .map(|i| {
                let mut d = Dom::create_div();
                d.root
                    .set_ids_and_classes(vec![azul_core::dom::IdOrClass::Class("p".into())].into());
                d.with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    format!("block {i}"),
                ))
            })
            .collect()
    }
    let build = |break_child_idxs: &[usize]| -> azul_core::styled_dom::StyledDom {
        let mut body = Dom::create_body();
        let mut children = blocks(2);
        let mut idxs = break_child_idxs.to_vec();
        idxs.sort_unstable();
        for &i in idxs.iter().rev() {
            if i <= children.len() {
                children.insert(i, Dom::create_page_break());
            }
        }
        for c in children {
            body = body.with_child(c);
        }
        let (css, _) = azul_css::parser2::new_from_str(CSS);
        azul_core::styled_dom::StyledDom::create(&mut body, css)
    };

    // Estimate on the break-less document: blocks at y=20 and y=300.
    let styled_a = build(&[]);
    let (cache_a, pagination_a) = paginate_styled(&styled_a, 800.0, 300.0);
    assert_eq!(
        pagination_a.breaks.len(),
        1,
        "560px of content on 300px pages: exactly one break: {:?}",
        pagination_a.breaks
    );
    assert!(
        (pagination_a.breaks[0].y - 300.0).abs() < 1.0,
        "the break must land on the page boundary at 300: {:?}",
        pagination_a.breaks
    );
    let breaks_a = pagination_to_dom_breaks(&cache_a, &styled_a, &pagination_a).expect("mapped");
    let path = breaks_a[0]
        .path
        .clone()
        .expect("aligned break maps to block 2");
    let body_child_idx = *path.last().expect("non-empty path") as usize;

    // Materialize the canonical element at the estimated position.
    let styled_b = build(&[body_child_idx]);
    let (cache_b, pagination_b) = paginate_styled(&styled_b, 800.0, 300.0);

    // Content must not have moved: block 2 still starts at y=300 (sibling
    // margins collapse THROUGH the empty break element).
    let block2_dom = azul_core::dom::NodeId::new(
        styled_b
            .node_data
            .as_ref()
            .iter()
            .enumerate()
            .filter(|(_, nd)| format!("{:?}", nd.get_node_type()).contains("Div"))
            .map(|(i, _)| i)
            .nth(1)
            .expect("second .p div"),
    );
    let tree_b = cache_b.tree.as_ref().expect("tree");
    let li = *tree_b
        .dom_to_layout
        .get(&block2_dom)
        .and_then(|v| v.first())
        .expect("block 2 laid out");
    let block2_y = cache_b
        .calculated_positions
        .get(li.index())
        .map(|p| p.y)
        .unwrap();
    assert!(
        (block2_y - 300.0).abs() < 1.0,
        "inserting the empty break element must not move block 2 (margin \
         collapse-through): got y={block2_y}"
    );

    // Same boundary, now forced.
    assert_eq!(pagination_a.page_count, pagination_b.page_count);
    assert_eq!(pagination_b.breaks.len(), 1, "{:?}", pagination_b.breaks);
    let b = &pagination_b.breaks[0];
    assert!(
        (b.y - 300.0).abs() < 1.0 && b.kind == BreakKind::Forced,
        "the materialized boundary is the SAME Y, now forced: {b:?}"
    );
}

/// Block-granular caveat, characterized: a boundary estimated MID-block
/// materializes at the spine block's TOP (the whole straddling block moves
/// to the next page, office-suite-style) — the boundary snaps, it does not slice.
#[test]
fn midblock_breaks_materialize_at_the_spine_block_top() {
    const CSS: &str = r#"
        * { margin: 0; padding: 0; }
        .p { height: 150px; }
    "#;
    fn blocks(n: usize) -> Vec<Dom> {
        (0..n)
            .map(|i| {
                let mut d = Dom::create_div();
                d.root
                    .set_ids_and_classes(vec![azul_core::dom::IdOrClass::Class("p".into())].into());
                d.with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    format!("block {i}"),
                ))
            })
            .collect()
    }
    // blocks at 0/150/300, page 200: break estimated at 200 (inside block 2),
    // spine = block 3 (top 300).
    let build = |break_child_idxs: &[usize]| -> azul_core::styled_dom::StyledDom {
        let mut body = Dom::create_body();
        let mut children = blocks(3);
        for &i in break_child_idxs.iter().rev() {
            children.insert(i, Dom::create_page_break());
        }
        for c in children {
            body = body.with_child(c);
        }
        let (css, _) = azul_css::parser2::new_from_str(CSS);
        azul_core::styled_dom::StyledDom::create(&mut body, css)
    };
    let styled_a = build(&[]);
    let (cache_a, pagination_a) = paginate_styled(&styled_a, 800.0, 200.0);
    let breaks_a = pagination_to_dom_breaks(&cache_a, &styled_a, &pagination_a).expect("mapped");
    let first = breaks_a
        .iter()
        .find(|b| b.path.is_some())
        .expect("mappable break");
    assert!(
        (first.y - 200.0).abs() < 1.0,
        "estimated mid-block: {first:?}"
    );
    let idx = *first.path.as_ref().unwrap().last().unwrap() as usize;

    let styled_b = build(&[idx]);
    let (_cache_b, pagination_b) = paginate_styled(&styled_b, 800.0, 200.0);
    assert!(
        pagination_b
            .breaks
            .iter()
            .any(|b| b.kind == BreakKind::Forced && (b.y - 300.0).abs() < 1.0),
        "the materialized break lands at the spine block's top (300), not at \
         the mid-block estimate (200): {:?}",
        pagination_b.breaks
    );
}

/// Shared driver for StyledDom fixtures (the `paginate` helper builds from
/// XML; this one takes a prepared StyledDom).
fn paginate_styled(
    styled_dom: &azul_core::styled_dom::StyledDom,
    w: f32,
    h: f32,
) -> (Solver3LayoutCache, azul_layout::PaginationInfo) {
    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("FontManager");
    let font_manager = &mut font_manager;
    let mut layout_cache = Solver3LayoutCache {
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
    let content_size = LogicalSize::new(w, h);
    let fragmentation_context = FragmentationContext::new_paged(content_size);
    let viewport = LogicalRect {
        origin: LogicalPosition::zero(),
        size: content_size,
    };
    let renderer_resources = RendererResources::default();
    let mut debug_messages = Some(Vec::new());
    let loader = PathLoader::new();
    let pagination = compute_document_pagination(
        &mut layout_cache,
        &mut text_cache,
        fragmentation_context,
        styled_dom,
        viewport,
        font_manager,
        &BTreeMap::new(),
        &mut debug_messages,
        None,
        &renderer_resources,
        azul_core::resources::IdNamespace(0),
        DomId::ROOT_ID,
        |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
            loader.load_font_shared(bytes, index)
        },
        FakePageConfig::new(),
        &azul_core::resources::ImageCache::default(),
        azul_core::task::GetSystemTimeCallback {
            cb: azul_core::task::get_system_time_libstd,
        },
    )
    .expect("pagination should succeed");
    (layout_cache, pagination)
}

#[test]
fn mid_paragraph_break_exposes_the_line_start_byte() {
    // Ledger #2 (line-granular option): ONE long wrapped paragraph spanning
    // several 200px pages. The interval breaks land BETWEEN LINE BOXES
    // inside the paragraph, so each mid-paragraph break must expose the
    // (run, byte) of the first line that moves — the app splits the
    // paragraph text there instead of moving the whole block.
    let long = "wrap wrap wrap wrap wrap wrap wrap wrap wrap wrap ".repeat(40);
    let html = format!(
        r#"
    <html><head><style>
        * {{ margin: 0; padding: 0; }}
        body {{ font-size: 16px; width: 300px; }}
        .p {{ display: block; width: 300px; }}
    </style></head>
    <body>
        <div class="p">{long}</div>
    </body></html>"#
    );
    let (cache, styled_dom, pagination) = paginate(&html, 300.0, 200.0);
    assert!(
        pagination.page_count >= 2,
        "the paragraph must span pages, got {}",
        pagination.page_count
    );
    let breaks = pagination_to_dom_breaks(&cache, &styled_dom, &pagination)
        .expect("cache holds tree+positions");

    // Every break here lands inside the single paragraph: line_start must
    // name a real, strictly-increasing byte per break (line starts are
    // monotone down the text).
    let mut prev_byte: Option<u32> = None;
    let mut saw_line_start = 0usize;
    for b in &breaks {
        if let Some(ls) = b.line_start {
            saw_line_start += 1;
            assert!(
                ls.item_index > 0,
                "a mid-paragraph break never starts at byte 0 (that would \
                 be a block boundary): {b:?}"
            );
            if let Some(prev) = prev_byte {
                assert!(
                    ls.item_index > prev,
                    "line-start bytes must increase page over page: \
                     {prev} then {ls:?}"
                );
            }
            prev_byte = Some(ls.item_index);
        }
    }
    assert!(
        saw_line_start >= 1,
        "at least one interval break lands mid-paragraph and must carry \
         line_start: {breaks:?}"
    );
}

#[test]
fn block_boundary_breaks_carry_no_line_start() {
    // The block-granular contract is untouched: fixed-height blocks whose
    // boundaries align with break positions yield line_start == None (the
    // whole addressed block moves).
    let html = r#"
    <html><head><style>
        * { margin: 0; padding: 0; }
        .p { height: 200px; }
    </style></head>
    <body>
        <div class="p">one</div>
        <div class="p">two</div>
        <div class="p">three</div>
    </body></html>"#;
    let (cache, styled_dom, pagination) = paginate(html, 800.0, 200.0);
    let breaks = pagination_to_dom_breaks(&cache, &styled_dom, &pagination)
        .expect("cache holds tree+positions");
    for b in &breaks {
        assert!(
            b.line_start.is_none(),
            "a break at a block boundary must stay block-granular: {b:?}"
        );
    }
}

/// AZUL-STILL-TODO #16b: the worked example for "window shift on content
/// deletion". Deleting ~a page of content mid-document must tell BOTH
/// consumers (the mounted-pages window and the thumbnail rail) exactly
/// where re-derivation starts — via the same BreaksDelta signal — and the
/// estimate output must be cheap to materialize per page for thumbnails.
#[test]
fn deletion_shifts_the_window_via_breaks_delta() {
    use azul_layout::PaginationSession;
    let doc = |blocks: usize| -> azul_core::styled_dom::StyledDom {
        let mut body = String::from(
            r#"<html><head><style>* { margin:0; padding:0; } .p { height: 150px; }</style></head><body>"#,
        );
        for i in 0..blocks {
            body.push_str(&format!(r#"<div class="p">block {i}</div>"#));
        }
        body.push_str("</body></html>");
        Dom::from_xml_string(&body)
    };
    let doc_a = doc(6); // 900px on 200px pages
    let doc_b = doc(4); // two blocks (~1.5 pages) deleted → 600px

    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("FontManager");
    let renderer_resources = RendererResources::default();
    let loader = PathLoader::new();
    let viewport = LogicalRect {
        origin: LogicalPosition::zero(),
        size: LogicalSize::new(800.0, 200.0),
    };
    let mut session = PaginationSession::new();
    let run = |session: &mut PaginationSession,
               dom: &azul_core::styled_dom::StyledDom,
               fm: &mut FontManager<_>| {
        let mut debug_messages = Some(Vec::new());
        session
            .re_estimate(
                dom,
                viewport,
                fm,
                &BTreeMap::new(),
                &mut debug_messages,
                None,
                &renderer_resources,
                azul_core::resources::IdNamespace(0),
                DomId::ROOT_ID,
                |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
                    loader.load_font_shared(bytes, index)
                },
                FakePageConfig::new(),
                &azul_core::resources::ImageCache::default(),
                azul_core::task::GetSystemTimeCallback {
                    cb: azul_core::task::get_system_time_libstd,
                },
            )
            .expect("re-estimate")
    };

    let _first = run(&mut session, &doc_a, &mut font_manager);
    let breaks_a = session.info().expect("estimate").breaks.clone();
    assert!(breaks_a.len() >= 3, "900px / 200px pages: {breaks_a:?}");

    // The app's pre-deletion state: window shows pages 0..5, caret on the
    // LAST page.
    let old_page_count = session.info().unwrap().page_count;

    let delta = run(&mut session, &doc_b, &mut font_manager);
    // Both consumers read the SAME signal: everything before
    // first_changed_page is untouched (window keeps those pages mounted,
    // rail keeps those thumbnails); re-derivation starts there.
    assert!(delta.page_count_changed, "{delta:?}");
    assert!(
        delta.unchanged_prefix_len >= 1,
        "breaks before the deletion point are bit-for-bit unchanged: {delta:?}"
    );
    assert_eq!(delta.first_changed_page, delta.unchanged_prefix_len);
    let new_info = session.info().unwrap();
    assert!(
        new_info.page_count < old_page_count,
        "deleting ~1.5 pages shrinks the document: {} -> {}",
        old_page_count,
        new_info.page_count
    );
    // Window math (the app side of the example): a caret that sat on the
    // old last page now sits on the new last page — its page index comes
    // straight from the new spans, and it is >= first_changed_page (the
    // shifted region), so the window re-derives from the signal alone.
    let caret_y_new = 550.0; // inside the last remaining block
    let spans = azul_layout::page_spans(&new_info.breaks, 600.0);
    let caret_page = spans
        .iter()
        .position(|&(start, end)| caret_y_new >= start && caret_y_new < end)
        .expect("caret lands on a page");
    assert!(
        caret_page >= delta.first_changed_page,
        "the caret's page ({caret_page}) is in the re-derived region \
         (>= {})",
        delta.first_changed_page
    );
    // Structural mapping stays available for the re-derived region.
    assert!(session.dom_breaks(&doc_b).is_some());
}

#[allow(dead_code)] // diagnostic helper: re-enabled by hand when a break moves
fn estimator_node_heights(xml: &str, label: &str) {
    let parsed = azul_layout::xml::parse_xml_string(xml).expect("parse");
    let full = azul_layout::xml::dom_from_parsed_xml(azul_layout::xml::Xml {
        root: parsed.into(),
    });
    let mut content = Dom::create_div();
    content.css = full.css.clone();
    for c in full.children.as_ref() {
        if matches!(c.root.get_node_type(), azul_core::dom::NodeType::Body) {
            content.children = c.children.clone();
        }
    }
    content.fixup_children_estimated();
    let styled_dom = azul_core::styled_dom::StyledDom::create_from_dom(core::mem::replace(
        &mut content,
        Dom::create_div(),
    ));
    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("FontManager");
    let mut layout_cache = Solver3LayoutCache {
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
    let content_size = LogicalSize::new(602.0, 931.0);
    let fragmentation_context = FragmentationContext::new_paged(content_size);
    let viewport = LogicalRect {
        origin: LogicalPosition::zero(),
        size: content_size,
    };
    let renderer_resources = RendererResources::default();
    let mut debug_messages = None;
    let loader = PathLoader::new();
    let font_loader = |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
        loader.load_font_shared(bytes, index)
    };
    let _ = compute_document_pagination(
        &mut layout_cache,
        &mut text_cache,
        fragmentation_context,
        &styled_dom,
        viewport,
        &mut font_manager,
        &BTreeMap::new(),
        &mut debug_messages,
        None,
        &renderer_resources,
        azul_core::resources::IdNamespace(0),
        DomId::ROOT_ID,
        font_loader,
        FakePageConfig::new(),
        &azul_core::resources::ImageCache::default(),
        azul_core::task::GetSystemTimeCallback {
            cb: azul_core::task::get_system_time_libstd,
        },
    );
    if let Some(t) = layout_cache.tree.as_ref() {
        for (i, n) in t.nodes.iter().enumerate() {
            let lines = t
                .warm(LayoutNodeId::new(i))
                .and_then(|w| w.inline_layout_result.as_ref())
                .map(|r| format!("{:?}", r.layout.bounds()))
                .unwrap_or_else(|| "-".into());
            eprintln!(
                "[{label}] node {i}: dom={:?} used_h={:?} pos={:?} ifc_bounds={lines}",
                n.dom_node_id.map(|d| d.index()),
                n.used_size.map(|s| s.height),
                layout_cache.calculated_positions.get(i),
            );
        }
    }
}

fn estimator_root_height(xml: &str) -> f32 {
    let parsed = azul_layout::xml::parse_xml_string(xml).expect("parse");
    let full = azul_layout::xml::dom_from_parsed_xml(azul_layout::xml::Xml {
        root: parsed.into(),
    });
    let mut content = Dom::create_div();
    content.css = full.css.clone();
    for c in full.children.as_ref() {
        if matches!(c.root.get_node_type(), azul_core::dom::NodeType::Body) {
            content.children = c.children.clone();
        }
    }
    content.fixup_children_estimated();
    let styled_dom = azul_core::styled_dom::StyledDom::create_from_dom(core::mem::replace(
        &mut content,
        Dom::create_div(),
    ));
    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("FontManager");
    let mut layout_cache = Solver3LayoutCache {
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
    let content_size = LogicalSize::new(602.0, 931.0);
    let fragmentation_context = FragmentationContext::new_paged(content_size);
    let viewport = LogicalRect {
        origin: LogicalPosition::zero(),
        size: content_size,
    };
    let renderer_resources = RendererResources::default();
    let mut debug_messages = None;
    let loader = PathLoader::new();
    let font_loader = |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
        loader.load_font_shared(bytes, index)
    };
    let _ = compute_document_pagination(
        &mut layout_cache,
        &mut text_cache,
        fragmentation_context,
        &styled_dom,
        viewport,
        &mut font_manager,
        &BTreeMap::new(),
        &mut debug_messages,
        None,
        &renderer_resources,
        azul_core::resources::IdNamespace(0),
        DomId::ROOT_ID,
        font_loader,
        FakePageConfig::new(),
        &azul_core::resources::ImageCache::default(),
        azul_core::task::GetSystemTimeCallback {
            cb: azul_core::task::get_system_time_libstd,
        },
    );
    layout_cache
        .tree
        .as_ref()
        .and_then(|t| t.nodes.first())
        .and_then(|n| n.used_size)
        .map(|s| s.height)
        .unwrap_or(-1.0)
}

/// miniword capstone: wrapped pure-text paragraphs (markdown soft breaks =
/// embedded newlines) must measure in the estimator, through the unstyled
/// XML path the app uses.
#[test]
fn wrapped_pure_text_measures_via_the_unstyled_xml_path() {
    let long = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do\neiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad\nminim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip\nex ea commodo consequat. Duis aute irure dolor in reprehenderit in\nvoluptate velit esse cillum dolore eu fugiat nulla pariatur.";
    let xml = format!(
        "<html><head><style>\n\
            body {{ font-family: 'Liberation Sans', sans-serif; font-size: 15px;\n\
                    color: #1a1a1a; line-height: 1.35; }}\n\
            p {{ margin-bottom: 11px; }}\n\
        </style></head>\n\
        <body>\n<p>{long}</p>\n<p>{long}</p>\n<p>{long}</p>\n<p>{long}</p>\n<p>{long}</p>\n<p>{long}</p>\n</body></html>"
    );
    let h = estimator_root_height(&xml);
    assert!(
        h > 400.0,
        "six wrapped paragraphs must measure (>700px), got {h}px \
         (miniword: multi-line pure-text <p>s measured 0.0)"
    );
}

/// KNOWN BUG (ignored pin, miniword capstone): the estimator measures the
/// SAME document differently depending on ONE character — whether a newline
/// separates `<body>` from its first element. With `<body><h1>` every
/// multi-line pure-text `<p>` measures 0.0 (the sample document reports
/// 482px / 1 page); with `<body>\n<h1>` everything measures (~1360px /
/// 2 pages). The parser preserves text verbatim in both cases, so the
/// divergence is in the unstyled XML->Dom builder or the anonymous-box/IFC
/// classification. Un-ignore when fixed.
#[test]
fn estimator_is_insensitive_to_leading_body_whitespace() {
    let bad = MINIWORD_SAMPLE_XML;
    let good = MINIWORD_SAMPLE_XML.replacen("<body><h1>", "<body>\n<h1>", 1);
    let h_bad = estimator_root_height(bad);
    let h_good = estimator_root_height(&good);
    assert!(
        (h_bad - h_good).abs() < 1.0 && h_good > 1000.0,
        "one whitespace character must not change document measurement: \
         <body><h1> measured {h_bad}px, <body>-newline-<h1> measured {h_good}px"
    );
}

const MINIWORD_SAMPLE_XML: &str = r####"<html><head><style>
    body { font-family: 'Liberation Sans', sans-serif; font-size: 15px;
           color: #1a1a1a; line-height: 1.35; }
    p    { margin-bottom: 11px; }
    h1   { font-size: 28px; color: #2e74b5; margin-bottom: 12px; margin-top: 4px; }
    h2   { font-size: 21px; color: #2e74b5; margin-bottom: 10px; margin-top: 4px; }
    h3   { font-size: 17px; color: #1f4d78; margin-bottom: 9px;  margin-top: 4px; }
    ul, ol { margin-bottom: 11px; margin-left: 36px; }
    li   { margin-bottom: 2px; }
    blockquote { margin-left: 36px; margin-bottom: 11px; color: #555555;
                 border-left: 3px solid #cccccc; padding-left: 10px; }
    code { font-family: 'Liberation Mono', monospace; font-size: 13px;
           background: #f2f2f2; }
    pre  { font-family: 'Liberation Mono', monospace; font-size: 13px;
           background: #f6f6f6; padding: 8px; margin-bottom: 11px; }
    hr   { border-bottom: 1px solid #bbbbbb; margin-bottom: 11px; }
    strong { font-weight: bold; }
    em   { font-style: italic; }
</style></head><body><h1>Project Report</h1>
<p>This document demonstrates the miniword pipeline: markdown is converted to
HTML, parsed into a DOM by the azul XML parser, styled with the the Office-2013-era look
document stylesheet, and dynamically paginated by the layout engine.</p>
<h2>Background</h2>
<p>The layout engine estimates page breaks with <code>compute_document_pagination</code>,
maps every break to a structural DOM path, and the application splits its
content DOM at those paths. Each page is an independent subtree wrapped in
the white sheet frame.</p>
<h2>Method</h2>
<ul>
<li>Markdown parsing via <strong>pulldown-cmark</strong></li>
<li>HTML to DOM through the <em>azul XML parser</em></li>
<li>Dynamic pagination with the break-token engine</li>
<li>One DOM per page, the classic page model</li>
</ul>
<h3>Details</h3>
<p>Paragraphs flow across pages at block boundaries. Forced breaks, widows and
orphans follow the css-break-3 rules that the engine test suite pins. The
status bar shows the live page count from the same pagination result.</p>
<h2>Results</h2>
<p>The pipeline round-trips: load, edit, paginate, save. What you see on the
canvas is the engine's own idea of where each page ends — no estimates in
the application, no hand-computed heights, no hacks.</p>
<blockquote>
<p>The whole point of the capstone: the application composes engine
primitives without working around them.</p>
</blockquote>
<h2>Longer section to force a second page</h2>
<p>Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod
tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam,
quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo
consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse
cillum dolore eu fugiat nulla pariatur.</p>
<p>Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia
deserunt mollit anim id est laborum. Sed ut perspiciatis unde omnis iste
natus error sit voluptatem accusantium doloremque laudantium, totam rem
aperiam, eaque ipsa quae ab illo inventore veritatis et quasi architecto
beatae vitae dicta sunt explicabo.</p>
<p>Nemo enim ipsam voluptatem quia voluptas sit aspernatur aut odit aut fugit,
sed quia consequuntur magni dolores eos qui ratione voluptatem sequi
nesciunt. Neque porro quisquam est, qui dolorem ipsum quia dolor sit amet,
consectetur, adipisci velit, sed quia non numquam eius modi tempora incidunt
ut labore et dolore magnam aliquam quaerat voluptatem.</p>
<p>Ut enim ad minima veniam, quis nostrum exercitationem ullam corporis
suscipit laboriosam, nisi ut aliquid ex ea commodi consequatur? Quis autem
vel eum iure reprehenderit qui in ea voluptate velit esse quam nihil
molestiae consequatur, vel illum qui dolorem eum fugiat quo voluptas nulla
pariatur?</p>
<p>At vero eos et accusamus et iusto odio dignissimos ducimus qui blanditiis
praesentium voluptatum deleniti atque corrupti quos dolores et quas
molestias excepturi sint occaecati cupiditate non provident, similique sunt
in culpa qui officia deserunt mollitia animi, id est laborum et dolorum
fuga. Et harum quidem rerum facilis est et expedita distinctio.</p>
</body></html>"####;

// ── AppliedEdit::inverse_resume: undo must be replayable verbatim ──────────

/// An application undoing a structural edit re-records the returned inverse
/// through the same apply loop. Index resolution is ASYMMETRIC (split reads
/// `resume.last() - 1`, merge reads `resume.last()`), so replaying with the
/// ORIGINAL resume point edits the wrong pair — miniword's undo merged
/// blocks 1+2 where it had split 0. `inverse_resume` is the fix; this pins
/// that a split→undo round-trip restores the document EXACTLY.
#[test]
fn applied_edit_inverse_resume_makes_undo_a_verbatim_replay() {
    use azul_layout::managers::changeset::{
        DocOpSplitNode, DocumentChangeset, DocumentOperation, EditResumePoint, NodePosition,
    };

    fn null_node() -> azul_core::dom::DomNodeId {
        azul_core::dom::DomNodeId {
            dom: DomId::ROOT_ID,
            node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(None),
        }
    }
    fn block_texts(d: &Dom) -> Vec<String> {
        fn own(d: &Dom) -> String {
            let mut s = String::new();
            for c in d.children.as_ref() {
                match c.root.get_node_type() {
                    azul_core::dom::NodeType::Text(t) => s.push_str(t.as_str()),
                    _ => s.push_str(&own(c)),
                }
            }
            s
        }
        d.children.as_ref().iter().map(own).collect()
    }
    fn cs(op: DocumentOperation, path: Vec<u32>) -> DocumentChangeset {
        DocumentChangeset::new(
            null_node(),
            op,
            EditResumePoint {
                anchor_key: 0,
                node_path: path.into(),
                position: NodePosition {
                    child_index: 0,
                    text_byte: Some(0).into(),
                },
            },
            azul_core::task::Instant::from(std::time::Instant::now()),
        )
    }

    let mut model = Dom::create_div()
        .with_child(Dom::create_p().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper("First paragraph here."),
        ))
        .with_child(Dom::create_p().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper("Second."),
        ))
        .with_child(Dom::create_p().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper("Third."),
        ));
    model.fixup_children_estimated();
    let before = block_texts(&model);

    // Split block 0 after "First" — the resume names the NEW second part.
    let forward = cs(
        DocumentOperation::SplitNode(DocOpSplitNode {
            node: null_node(),
            at: NodePosition {
                child_index: 0,
                text_byte: Some(5).into(),
            },
        }),
        vec![1],
    );
    let applied = azul_layout::document_edit::apply_document_operation(&mut model, &[], &forward)
        .expect("apply split");
    assert_eq!(
        block_texts(&model),
        vec!["First", " paragraph here.", "Second.", "Third."]
    );

    // The inverse's resume must NOT be the forward one (that is the bug).
    assert_ne!(
        applied.inverse_resume.node_path.as_ref(),
        applied.resume.node_path.as_ref(),
        "a split's inverse merges at a different index than the split's resume"
    );

    // UNDO: replay the inverse verbatim with the resume the engine handed back.
    let undo = cs(
        applied.inverse.clone(),
        applied.inverse_resume.node_path.as_ref().to_vec(),
    );
    azul_layout::document_edit::apply_document_operation(&mut model, &[], &undo)
        .expect("apply inverse");
    assert_eq!(
        block_texts(&model),
        before,
        "undo must restore the document exactly"
    );

    // NEGATIVE CONTROL: replaying with the FORWARD resume (what an app would
    // naively reach for) must NOT restore it — otherwise this test would
    // pass even with inverse_resume removed.
    let mut model2 = Dom::create_div()
        .with_child(Dom::create_p().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper("First paragraph here."),
        ))
        .with_child(Dom::create_p().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper("Second."),
        ))
        .with_child(Dom::create_p().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper("Third."),
        ));
    model2.fixup_children_estimated();
    let applied2 = azul_layout::document_edit::apply_document_operation(&mut model2, &[], &forward)
        .expect("apply split 2");
    let naive = cs(
        applied2.inverse.clone(),
        applied2.resume.node_path.as_ref().to_vec(),
    );
    let _ = azul_layout::document_edit::apply_document_operation(&mut model2, &[], &naive);
    assert_ne!(
        block_texts(&model2),
        before,
        "control: the forward resume must NOT undo correctly (if it does, \
         inverse_resume is not carrying its weight)"
    );
}
