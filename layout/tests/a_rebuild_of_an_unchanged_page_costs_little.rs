//! A rebuild of an UNCHANGED page costs little: the reconciliation's
//! restyle diff compared every one of the ~200 CSS property types of every
//! matched node, old against new, through the full cascade lookup - for a
//! PDF page of a few thousand words that was the bulk of every rebuild
//! (AzPdf, AzMaps' label tiles: "performance is horrible").

use std::time::Instant;

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// A page of `lines` x `words` absolutely placed words, as AzPdf builds one.
fn page(lines: usize, words: usize) -> StyledDom {
    let mut page = Dom::create_div().with_css("position: relative; width: 800px; height: 1100px;");
    for l in 0..lines {
        for w in 0..words {
            page = page.with_child(
                Dom::create_span_with_text(format!("word{l}x{w}").as_str()).with_css(&format!(
                    "position: absolute; left: {}px; top: {}px; font-size: 11px;",
                    w * 60,
                    l * 14
                )),
            );
        }
    }
    StyledDom::create_from_dom(Dom::create_body().with_css("margin: 0;").with_child(page))
}

#[test]
#[ignore = "a measurement: cargo test --release -- --ignored --nocapture a_rebuild_of_an_unchanged"]
fn a_rebuild_of_an_unchanged_page_costs_little() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(900.0, 1200.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        page(60, 12),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    for round in 0..3 {
        let mut next = page(60, 12);
        let t = Instant::now();
        let pending =
            lw.begin_reconciliation(DomId::ROOT_ID, &mut next, azul_core::task::Instant::now());
        let reconcile = t.elapsed();
        let t = Instant::now();
        lw.layout_new_generation(
            next,
            &ws,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut None,
        )
        .expect("the next page lays out");
        lw.finish_reconciliation(DomId::ROOT_ID, &pending);
        let layout = t.elapsed();
        println!(
            "round {round}: {} matched, {} restyled - reconciliation {:.1} ms, layout {:.1} ms",
            pending.node_moves.len(),
            pending.restyled.len(),
            reconcile.as_secs_f64() * 1000.0,
            layout.as_secs_f64() * 1000.0
        );
    }
}
