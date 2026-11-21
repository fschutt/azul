//! A layout pass rebuilds the CPU hit tester once, and a rebuild that
//! changed nothing does not.
//!
//! The pointer events of a window resolve their node through the window's shared
//! hit tester (`CommonWindowState::cpu_hit_tester`), which every layout pass
//! rebuilds. The headless backend rebuilt a second one of its own beside it that
//! nothing read: a walk over every laid-out node, twice per pass.

use azul_layout::probe::Probe;

use super::*;

fn hit_tester_rebuilds(run: impl FnOnce()) -> usize {
    Probe::set_recording(true);
    let _ = Probe::drain();
    run();
    let events = Probe::drain();
    Probe::set_recording(false);
    events
        .iter()
        .filter(|e| e.name == "cpu_hit_tester_rebuild")
        .count()
}

#[test]
fn a_layout_pass_rebuilds_the_hit_tester_once() {
    use crate::desktop::shell2::common::event::IncrementalRelayout;

    if !Probe::enabled() {
        eprintln!("[one_hit_tester] the probe is compiled out: nothing to count");
        return;
    }
    let state = Arc::new(RefCell::new(RefAny::new(UiState {
        label: "hello".to_string(),
    })));
    let mut window = make_harness_window(&state);
    window.regenerate_layout().expect("initial layout");

    // A rebuild that leaves the page as it was leaves its layout, and the
    // hit tester built from it, as they were.
    let unchanged = hit_tester_rebuilds(|| {
        window.regenerate_layout().expect("a rebuild");
    });
    assert_eq!(
        unchanged, 0,
        "an unchanged rebuild rebuilt the hit tester {unchanged} times"
    );

    set_label(&state, "a longer label than before");
    let rebuild = hit_tester_rebuilds(|| {
        window.regenerate_layout().expect("a rebuild");
    });
    assert_eq!(
        rebuild, 1,
        "a rebuild rebuilt the hit tester {rebuild} times"
    );

    let relayout = hit_tester_rebuilds(|| {
        window
            .relayout_existing_dom(IncrementalRelayout::Restyle)
            .expect("a relayout");
    });
    assert_eq!(
        relayout, 1,
        "a relayout rebuilt the hit tester {relayout} times"
    );

    assert!(
        window
            .common
            .cpu_hit_tester
            .as_ref()
            .is_some_and(|ht| ht.node_rects_total() > 0),
        "the shared hit tester knows the laid-out nodes"
    );
}
