//! A frame of an accordion section opening costs what the section's
//! ancestors cost, however long the page around it is.
//!
//! The user reported the accordion's show / hide animation as extremely
//! laggy. In AzWidgets (3473 nodes) no frame of the tween rebuilds the DOM or
//! cascades it again: the click writes the body's height, and every frame
//! after it is an incremental relayout of the retained tree. These tests keep
//! it that way. A mid-tween frame re-flows no text outside the section, lays
//! out no more on a page twice as long, never reconciles the unchanged DOM,
//! and fits a time budget in a release build.

use std::time::Duration;

use azul_core::dom::Dom;
use azul_layout::{probe::Probe, window::LayoutWindow};

use crate::{
    a_one_box_slide_does_not_re_lay_out_the_page::{
        card, count, relayout, restore_ambient_recording, window, FrameCost,
    },
    accordion_animation::{click, node_with_class},
};
use azul_layout::widgets::accordion::{Accordion, AccordionSection, AccordionSectionVec};

/// The budget of one mid-tween frame on the long page, in the CPU time of
/// the thread that runs it (`thread_cpu_clock`: the suite's other tests and
/// the machine's other work do not count), release builds only. The frame
/// lays out the section's ancestors and patches the display list; a frame
/// that lays out the page again takes several times this.
const FRAME_BUDGET: Duration = Duration::from_millis(8);

/// AzWidgets' shape: a scrolling column holding a closed one-section
/// accordion (80 px of content) above `cards` settings cards.
pub(crate) fn accordion_page(cards: usize) -> Dom {
    let section = AccordionSection::new("Details", Dom::create_div().with_css("height: 80px;"));
    let mut page = Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(Accordion::new(AccordionSectionVec::from_vec(vec![section])).dom());
    for i in 0..cards {
        page = page.with_child(card(i, false));
    }
    Dom::create_html().with_child(
        Dom::create_body()
            .with_css("display: flex; flex-direction: column; height: 100%;")
            .with_child(
                Dom::create_div()
                    .with_css(
                        "display: flex; flex-direction: column; flex-grow: 1; \
                         overflow-y: auto; padding: 24px;",
                    )
                    .with_child(page),
            ),
    )
}

/// One frame of the frame driver: tick the tweens by 16 ms, relayout when a
/// layout property moved.
pub(crate) fn step(lw: &mut LayoutWindow) {
    lw.tick_animations(0.016);
    if lw.take_transition_relayout() {
        relayout(lw);
    }
}

/// What a frame in the middle of the opening tween cost on a page of
/// `cards` cards: the probe's counts, the full reconciles it ran, and its
/// CPU time.
fn mid_tween_frame_cost(cards: usize) -> (FrameCost, usize, Duration) {
    let mut lw = window(accordion_page(cards));
    let header = node_with_class(&lw, "__azul-native-accordion-header");
    click(&mut lw, header);
    // The click's own frame and the first tween frame restyle the clicked
    // nodes; the frames the tween is made of come after them.
    step(&mut lw);
    step(&mut lw);
    assert!(
        !lw.css_transitions.is_empty(),
        "harness: the section is still opening, the next frame is mid-tween"
    );

    Probe::set_recording(true);
    let _ = Probe::drain();
    let started = crate::thread_cpu_clock::CpuInstant::now();
    step(&mut lw);
    let took = started.elapsed();
    let events = Probe::drain();
    Probe::set_recording(false);

    let cost = FrameCost {
        layout_passes: count(&events, "root_layout_pass"),
        text_flows: count(&events, "text_layout_flow"),
        flex_items_laid_out: count(&events, "taffy_cache_get_miss")
            + count(&events, "taffy_final_layout_stale"),
        formatting_contexts: count(&events, "fc_block")
            + count(&events, "fc_inline")
            + count(&events, "fc_inline_block")
            + count(&events, "fc_flex_grid"),
    };
    (cost, count(&events, "reconcile_and_invalidate"), took)
}

/// The section opening on a page of 40 cards and on one of 80: a frame in the
/// middle of the tween re-flows no text outside the section and lays out no
/// more on the long page than on the short one.
#[test]
fn an_accordion_frame_costs_the_same_on_a_page_twice_as_long() {
    let _serialised = crate::probe_lock();
    if !Probe::enabled() {
        eprintln!("[an_accordion_frame] the probe is compiled out: nothing to count");
        return;
    }
    let (short, _, _) = mid_tween_frame_cost(40);
    let (long, _, _) = mid_tween_frame_cost(80);
    restore_ambient_recording();
    eprintln!("[an_accordion_frame] 40 cards: {short:?}");
    eprintln!("[an_accordion_frame] 80 cards: {long:?}");

    assert!(
        short.layout_passes >= 1 && long.layout_passes >= 1,
        "harness: each frame ran a layout pass, {short:?} / {long:?}"
    );
    assert!(
        long.text_flows <= short.text_flows && long.text_flows <= 4,
        "a frame of the opening section re-flowed {} text runs on 80 cards and {} on 40: the \
         frame lays out the page, not the section's ancestors",
        long.text_flows,
        short.text_flows
    );
    assert!(
        long.flex_items_laid_out <= short.flex_items_laid_out,
        "a frame of the opening section laid out {} flex items on 80 cards and {} on 40",
        long.flex_items_laid_out,
        short.flex_items_laid_out
    );
    assert!(
        long.formatting_contexts <= short.formatting_contexts,
        "a frame of the opening section laid out {} formatting contexts on 80 cards and {} on 40",
        long.formatting_contexts,
        short.formatting_contexts
    );
}

/// A tween frame changes no DOM node, only the override the tween walks, so
/// it takes the retained layout tree as it is: the unchanged DOM is not
/// fingerprinted and reconciled again.
#[test]
fn an_accordion_frame_does_not_reconcile_the_unchanged_dom() {
    let _serialised = crate::probe_lock();
    if !Probe::enabled() {
        eprintln!("[an_accordion_frame] the probe is compiled out: nothing to count");
        return;
    }
    let (_, reconciles, _) = mid_tween_frame_cost(80);
    restore_ambient_recording();
    assert_eq!(
        reconciles, 0,
        "a mid-tween frame reconciled the whole DOM again"
    );
}

/// The frame fits its budget in a release build, on a page of 80 cards.
#[test]
fn an_accordion_frame_fits_its_time_budget() {
    if cfg!(debug_assertions) {
        eprintln!("[an_accordion_frame] debug build: the time budget is for release builds");
        return;
    }
    let _serialised = crate::probe_lock();
    // The best of three, in CPU time on a performance core: a frame is
    // measured, not the machine's other work or the core it was given.
    let took = crate::thread_cpu_clock::on_a_performance_core(|| {
        (0..3)
            .map(|_| mid_tween_frame_cost(80).2)
            .min()
            .expect("three runs")
    });
    restore_ambient_recording();
    eprintln!("[an_accordion_frame] mid-tween frame on 80 cards: {took:?}");
    // A budget in time needs the machine to itself: beside the whole suite
    // the cores clock down and share their caches, and even the thread's own
    // CPU time grows by half. Checked where the test runs alone:
    // `AZ_TIMING=1 cargo test --release -p azul-layout --test all --
    // --test-threads=1 an_accordion_frame`.
    if std::env::var_os("AZ_TIMING").is_none() {
        eprintln!("[an_accordion_frame] AZ_TIMING unset: the budget is checked in the serial run");
        return;
    }
    assert!(
        took <= FRAME_BUDGET,
        "a mid-tween accordion frame took {took:?} on 80 cards, over the {FRAME_BUDGET:?} budget"
    );
}
