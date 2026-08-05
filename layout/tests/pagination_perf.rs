//! Where does pagination time actually go?
//!
//! Run: `cargo test -p azul-layout --features probe --test pagination_perf
//! -- --nocapture`
//!
//! A 34-block document paginating in hundreds of milliseconds is a defect,
//! not a cost — every phase here should be cheap on a warm cache. This
//! harness paginates the same document repeatedly and drains azul's own
//! probe spans so the hot phase names itself instead of being guessed at.

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
    solver3::{pagination::FakePageConfig, paged_layout::compute_document_pagination},
    text3::default::PathLoader,
    xml::DomXmlExt,
    Solver3LayoutCache,
};

/// The miniword sample shape: headings, paragraphs, a list, a quote.
fn sample_html(paragraphs: usize) -> String {
    let mut s = String::from(
        r#"<html><head><style>
        body { font-family: 'Liberation Sans', sans-serif; font-size: 15px;
               color: #1a1a1a; line-height: 1.35; }
        p  { margin-bottom: 11px; }
        h1 { font-size: 28px; color: #2e74b5; margin-bottom: 12px; }
        h2 { font-size: 21px; color: #2e74b5; margin-bottom: 10px; }
        ul { margin-bottom: 11px; margin-left: 36px; }
    </style></head><body>
    <h1>Project Report</h1>
    <h2>Background</h2>
    <ul><li>alpha item</li><li>beta item</li><li>gamma item</li></ul>
"#,
    );
    for i in 0..paragraphs {
        s.push_str(&format!(
            "<p>Paragraph number {i}: lorem ipsum dolor sit amet, consectetur \
             adipiscing elit, sed do eiusmod tempor incididunt ut labore et \
             dolore magna aliqua nostrud exercitation.</p>\n"
        ));
    }
    s.push_str("</body></html>");
    s
}

fn fresh_cache() -> Solver3LayoutCache {
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

/// One pagination of `html` at the A4 content box. `text_cache` and
/// `font_manager` are the CALLER's, so repeated calls exercise exactly the
/// caching an application would get.
fn paginate_once(
    html: &str,
    font_manager: &mut FontManager<azul_css::props::basic::FontRef>,
    text_cache: &mut TextLayoutCache,
) -> usize {
    let styled_dom = {
        let _p = azul_layout::probe::Probe::span("parse_and_cascade");
        Dom::from_xml_string(html)
    };
    let mut cache = fresh_cache();
    let content_size = LogicalSize::new(602.0, 931.0);
    let loader = PathLoader::new();
    let pagination = compute_document_pagination(
        &mut cache,
        text_cache,
        FragmentationContext::new_paged(content_size),
        &styled_dom,
        LogicalRect {
            origin: LogicalPosition::zero(),
            size: content_size,
        },
        font_manager,
        &BTreeMap::new(),
        &mut None,
        None,
        &RendererResources::default(),
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
    .expect("pagination");
    pagination.page_count
}

#[test]
fn pagination_phase_breakdown() {
    let html = sample_html(30);
    let fc = build_font_cache();
    let mut fm = FontManager::new(fc).expect("font manager");
    let mut text_cache = TextLayoutCache::new();

    // Cold: fonts get loaded here.
    let t = std::time::Instant::now();
    let pages = paginate_once(&html, &mut fm, &mut text_cache);
    eprintln!("[perf] cold  = {:?}  ({pages} pages)", t.elapsed());
    let _ = azul_layout::probe::Probe::drain();

    // Warm: same document, same caches — an application's steady state.
    const N: u32 = 5;
    let t = std::time::Instant::now();
    for _ in 0..N {
        paginate_once(&html, &mut fm, &mut text_cache);
    }
    let warm = t.elapsed() / N;
    eprintln!("[perf] warm  = {warm:?} per pagination");

    // Attribute the warm time to phases.
    let events = azul_layout::probe::Probe::drain();
    if events.is_empty() {
        eprintln!(
            "[perf] no probe events - rerun with `--features probe` to get the \
             phase breakdown"
        );
        return;
    }
    let mut totals: BTreeMap<&'static str, (u64, u32)> = BTreeMap::new();
    for e in &events {
        if let azul_layout::probe::EventKind::Span { dur_ns } = e.kind {
            let slot = totals.entry(e.name).or_insert((0, 0));
            slot.0 += dur_ns;
            slot.1 += 1;
        }
    }
    let mut rows: Vec<_> = totals.into_iter().collect();
    rows.sort_by_key(|(_, (nanos, _))| std::cmp::Reverse(*nanos));
    eprintln!("[perf] top phases across {N} paginations:");
    for (name, (nanos, count)) in rows.iter().take(14) {
        eprintln!(
            "[perf]   {:<34} {:>9.2} ms  ({count} calls)",
            name,
            *nanos as f64 / 1_000_000.0
        );
    }
}
