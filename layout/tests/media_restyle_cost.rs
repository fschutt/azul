#![cfg(feature = "text_layout")]
//! #15 workbench: what does a breakpoint crossing PAY today, and where?
//!
//! On a crossing the DOM is identical, `precascade_skip` fires, and the
//! @media context change routes through
//! `StyledDom::set_dynamic_selector_context` → `restyle_retained()`
//! (FULL author re-cascade) + inheritance/compact/tag rebuilds. The
//! condition-delta design replaces the full re-cascade with a re-match
//! of only the FLIPPED rule blocks; this file measures the baseline it
//! must beat, at document scale (~2k nodes), and pins the correctness
//! law any delta implementation must keep: post-delta property state
//! equals a from-scratch cascade under the new context.

use azul_core::{
    dom::Dom,
    styled_dom::StyledDom,
};
use azul_css::dynamic_selector::DynamicSelectorContext;

/// ~2k-node document: 320 paragraphs in 16 sections, with a stylesheet
/// carrying BOTH unconditional rules and @media-gated blocks (the
/// mobile-ribbon shape).
fn document_scale_dom() -> (Dom, &'static str) {
    let mut body = Dom::create_body();
    for s in 0..16 {
        let mut section = Dom::create_div().with_ids_and_classes(
            vec![azul_core::dom::IdOrClass::Class("section".into())].into(),
        );
        for p in 0..20 {
            let para = Dom::create_div()
                .with_ids_and_classes(
                    vec![azul_core::dom::IdOrClass::Class("para".into())].into(),
                )
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(azul_css::AzString::from(
                    format!("Section {s} paragraph {p} with some running text"),
                )));
            section = section.with_child(para);
        }
        body = body.with_child(section);
    }
    const CSS: &str = "
        .section { display: block; padding: 8px; }
        .para { display: block; margin-bottom: 4px; font-size: 14px; }
        @media (max-width: 800px) {
            .section { padding: 2px; }
            .para { font-size: 12px; margin-bottom: 2px; }
        }
        @media (min-width: 801px) {
            .para { color: #222222; }
        }
    ";
    (body, CSS)
}

fn ctx_with_width(w: f32) -> DynamicSelectorContext {
    DynamicSelectorContext {
        viewport_width: w,
        ..Default::default()
    }
}

/// Baseline measurement: initial cascade, then the crossing restyle in
/// both directions. Informational (prints); the delta impl must beat
/// the crossing numbers.
#[test]
fn probe_crossing_restyle_cost_at_document_scale() {
    let (mut dom, css_str) = document_scale_dom();
    let (css, _) = azul_css::parser2::new_from_str(css_str);

    let t0 = std::time::Instant::now();
    let mut styled = StyledDom::create(&mut dom, css);
    let create_dt = t0.elapsed();
    let n = styled.node_data.as_ref().len();

    // Prime the context wide, then cross narrow, then back.
    styled.set_dynamic_selector_context(ctx_with_width(1280.0));
    let t1 = std::time::Instant::now();
    styled.set_dynamic_selector_context(ctx_with_width(700.0));
    let cross_narrow = t1.elapsed();
    let t2 = std::time::Instant::now();
    styled.set_dynamic_selector_context(ctx_with_width(1280.0));
    let cross_wide = t2.elapsed();
    // No-op context set (same width): must be ~free (the early return).
    //
    // BEST-OF-N, not one sample. This file used to be alone in its process; it
    // now shares a multi-threaded harness with ~115 other test files, so any
    // single measurement can absorb a scheduler preemption worth milliseconds
    // and blow a 100 us budget for reasons that have nothing to do with the
    // code under test. The MINIMUM over a handful of runs is the honest floor:
    // it still goes red if the early return stops early-returning (a real
    // re-cascade is orders of magnitude over budget, on every sample), and it
    // cannot go red from contention alone.
    let noop = (0..9)
        .map(|_| {
            let t3 = std::time::Instant::now();
            styled.set_dynamic_selector_context(ctx_with_width(1280.0));
            t3.elapsed()
        })
        .min()
        .expect("9 samples");

    eprintln!(
        "[RESTYLE-PROBE] {n} nodes: create={create_dt:?} cross_narrow={cross_narrow:?} \
         cross_wide={cross_wide:?} noop={noop:?}"
    );
    // The NaN-sentinel PartialEq fix makes this strict: a same-context
    // set is an early return, orders of magnitude under a restyle.
    assert!(
        noop.as_micros() < 100,
        "the same-context set must early-return, not re-cascade (took {noop:?})"
    );
}

/// THE CORRECTNESS LAW any condition-delta restyle must keep: after a
/// context change, the retained DOM's cascaded state must equal a
/// FROM-SCRATCH cascade under the new context. Pinned via the crossing
/// path today (full restyle trivially satisfies it); when the delta
/// implementation lands, this is the gate that keeps it honest.
#[test]
fn crossing_restyle_matches_from_scratch_cascade() {
    let (mut dom_a, css_str) = document_scale_dom();
    let (css_a, _) = azul_css::parser2::new_from_str(css_str);
    let mut crossed = StyledDom::create(&mut dom_a, css_a);
    crossed.set_dynamic_selector_context(ctx_with_width(1280.0));
    crossed.set_dynamic_selector_context(ctx_with_width(700.0));

    let (mut dom_b, _) = document_scale_dom();
    let (css_b, _) = azul_css::parser2::new_from_str(css_str);
    let mut fresh = StyledDom::create(&mut dom_b, css_b);
    fresh.set_dynamic_selector_context(ctx_with_width(700.0));

    let n = crossed.node_data.as_ref().len();
    assert_eq!(n, fresh.node_data.as_ref().len());
    let cache_crossed = crossed.get_css_property_cache();
    let cache_fresh = fresh.get_css_property_cache();
    for i in 0..n {
        let a = cache_crossed.cascaded_props.get_slice(i);
        let b = cache_fresh.cascaded_props.get_slice(i);
        assert_eq!(
            a, b,
            "node {i}: crossed-restyle cascade diverges from from-scratch under the same context"
        );
    }
}
