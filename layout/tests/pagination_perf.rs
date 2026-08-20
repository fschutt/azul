//! Where does pagination time actually go?
//!
//! Run: `cargo test -p azul-layout --features probe --test all --
//! pagination_perf:: --nocapture`
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
        // Newer cache fields — defaults are the empty/cold state.
        ..Default::default()
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
    paginate_with(html, font_manager, text_cache, &mut fresh_cache())
}

fn paginate_with(
    html: &str,
    font_manager: &mut FontManager<azul_css::props::basic::FontRef>,
    text_cache: &mut TextLayoutCache,
    cache: &mut Solver3LayoutCache,
) -> usize {
    let styled_dom = {
        let _p = azul_layout::probe::Probe::span("parse_and_cascade");
        Dom::from_xml_string(html)
    };
    let content_size = LogicalSize::new(602.0, 931.0);
    let loader = PathLoader::new();
    let pagination = compute_document_pagination(
        cache,
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

/// A debug build is 10-13x slower than release here, and the ratio is not
/// uniform across phases — an unoptimised `String`/`Vec` helper dominates a
/// debug profile while barely registering in release. Reading a debug
/// profile as if it were the shipped cost sends you optimising the
/// allocator, not the algorithm; that happened once already (a 38 ms
/// "sizing pass" that is 2 ms in release). Say so in the output.
fn build_profile_banner() {
    if cfg!(debug_assertions) {
        eprintln!(
            "[perf] *** DEBUG BUILD - these numbers are NOT the shipped cost. \
             Re-run with --release before drawing any conclusion. ***"
        );
    }
}

#[test]
fn pagination_phase_breakdown() {
    // `Probe`'s recording flag is a process-global atomic (the buffer it gates
    // is thread-local). This file shares a binary with `probe_gate`, which
    // deliberately flips that flag on and off; without this lock the phase
    // breakdown below would attribute a truncated or a phantom profile
    // depending on the interleaving. See `crate::PROBE_LOCK`.
    let _serialised = crate::probe_lock();

    build_profile_banner();
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

    // EXPERIMENT: does REUSING the layout cache across paginations of the
    // same document make the per-node caches hit?
    let mut shared = fresh_cache();
    {
        paginate_with(&html, &mut fm, &mut text_cache, &mut shared);
        let _ = azul_layout::probe::Probe::drain();
        let t = std::time::Instant::now();
        for _ in 0..N {
            paginate_with(&html, &mut fm, &mut text_cache, &mut shared);
        }
        eprintln!("[perf] warm (SHARED layout cache) = {:?}", t.elapsed() / N);
    }

    // Attribute the warm time to phases.
    let events = azul_layout::probe::Probe::drain();
    if events.is_empty() {
        eprintln!(
            "[perf] no probe events - rerun with `--features probe` to get the \
             phase breakdown"
        );
        return;
    }
    // SELF time. Spans arrive post-order carrying (duration, depth), so a
    // span's immediate children are the not-yet-consumed spans at
    // depth+1 that precede it. Subtracting them turns "this subtree cost
    // X" (which double-counts and can exceed wall-clock) into "this phase
    // itself cost X" — the only number that names a hot spot.
    let mut totals: BTreeMap<&'static str, (u64, u64, u32)> = BTreeMap::new();
    // Stack of (depth, child_total_ns) for spans still awaiting a parent.
    let mut pending: Vec<(u16, u64)> = Vec::new();
    for e in &events {
        let azul_layout::probe::EventKind::Span { dur_ns } = e.kind else {
            continue;
        };
        // Everything deeper than this span, sitting on top of the stack,
        // is its direct-or-indirect child; the ones at exactly depth+1
        // are its immediate children.
        let mut children_ns = 0u64;
        while let Some(&(d, ns)) = pending.last() {
            if d > e.depth {
                if d == e.depth + 1 {
                    children_ns += ns;
                }
                pending.pop();
            } else {
                break;
            }
        }
        let self_ns = dur_ns.saturating_sub(children_ns);
        let slot = totals.entry(e.name).or_insert((0, 0, 0));
        slot.0 += self_ns;
        slot.1 += dur_ns;
        slot.2 += 1;
        pending.push((e.depth, dur_ns));
    }
    let mut rows: Vec<_> = totals.into_iter().collect();
    rows.sort_by_key(|(_, (self_ns, _, _))| std::cmp::Reverse(*self_ns));
    eprintln!(
        "[perf] phases by SELF time across {N} paginations (self / cumulative):"
    );
    for (name, (self_ns, cum_ns, count)) in rows.iter().take(40) {
        eprintln!(
            "[perf]   {:<32} {:>8.2} ms self  {:>8.2} ms cum  ({count} calls)",
            name,
            *self_ns as f64 / 1_000_000.0,
            *cum_ns as f64 / 1_000_000.0
        );
    }
}

/// The font-resolver skip must be a CACHE, not a gate.
///
/// Pagination now skips font resolution when the DOM's font-stack signature
/// matches what the `FontManager` last resolved. If that check is too
/// coarse, a document asking for a DIFFERENT family silently renders in the
/// previous document's font — the exact failure `window.rs` documents for
/// wholesale DOM swaps. This paginates two documents with different
/// families through ONE manager and requires the second family to resolve.
#[test]
fn changing_the_font_family_still_resolves_after_the_skip() {
    fn doc(family: &str) -> String {
        format!(
            r#"<html><head><style>
                body {{ font-family: '{family}'; font-size: 15px; }}
            </style></head><body>
            <p>Some text that needs a font to measure at all.</p>
            </body></html>"#
        )
    }

    let fc = build_font_cache();
    let mut fm = FontManager::new(fc).expect("font manager");
    let mut text_cache = TextLayoutCache::new();

    paginate_once(&doc("Liberation Sans"), &mut fm, &mut text_cache);
    let sans_keys: Vec<String> = fm
        .font_chain_cache
        .keys()
        .filter_map(|k| k.font_families.first().cloned())
        .collect();
    assert!(
        sans_keys.iter().any(|f| f == "Liberation Sans"),
        "first document's family must resolve, got {sans_keys:?}"
    );

    // Same manager, DIFFERENT family: the signature must change and the
    // resolver must run again.
    paginate_once(&doc("Liberation Mono"), &mut fm, &mut text_cache);
    let mono_keys: Vec<String> = fm
        .font_chain_cache
        .keys()
        .filter_map(|k| k.font_families.first().cloned())
        .collect();
    assert!(
        mono_keys.iter().any(|f| f == "Liberation Mono"),
        "a NEW font family must still resolve after the skip was armed — \
         otherwise the skip is a gate that starves changed fonts. Got \
         {mono_keys:?}"
    );

    // Negative control: paginating the SAME document again must NOT change
    // the cache (i.e. the skip really is taking effect, so this test would
    // notice if it silently stopped skipping).
    let before = fm.last_resolved_font_stacks_sig;
    paginate_once(&doc("Liberation Mono"), &mut fm, &mut text_cache);
    assert_eq!(
        fm.last_resolved_font_stacks_sig, before,
        "an unchanged document must reuse the recorded signature"
    );
    assert!(
        before.is_some(),
        "the signature must be RECORDED (the plain setter clears it, which \
         is what defeated the skip before)"
    );
}
