//! A frame of the accordion's opening tween builds its display list once.
//!
//! The frame lays out the section's ancestors and builds the list in that same
//! pass; nothing after the pass may build it again. A second build doubled
//! the cost of every tween frame in AzWidgets (a rebuild through
//! `regenerate_display_list_for_dom` after the pass, for scroll geometry the
//! pass had already published).

use azul_layout::probe::Probe;

use crate::{
    a_one_box_slide_does_not_re_lay_out_the_page::{count, restore_ambient_recording, window},
    accordion_animation::{click, node_with_class},
    an_accordion_frame_does_not_re_lay_out_the_page::{accordion_page, step},
};

#[test]
fn an_accordion_tween_frame_builds_its_display_list_once() {
    let _serialised = crate::probe_lock();
    if !Probe::enabled() {
        eprintln!("[an_accordion_tween_frame] the probe is compiled out: nothing to count");
        return;
    }
    let mut lw = window(accordion_page(40));
    let header = node_with_class(&lw, "__azul-native-accordion-header");
    click(&mut lw, header);
    step(&mut lw);
    step(&mut lw);
    assert!(
        !lw.css_transitions.is_empty(),
        "harness: the section is still opening, the next frames are mid-tween"
    );

    for frame in 0..3 {
        Probe::set_recording(true);
        let _ = Probe::drain();
        step(&mut lw);
        let events = Probe::drain();
        Probe::set_recording(false);
        let passes = count(&events, "root_layout_pass");
        let builds = count(&events, "dl_generate");
        let full_rebuilds = count(&events, "dl_regenerate_full");
        assert!(
            passes >= 1,
            "harness: mid-tween frame {frame} ran a layout pass"
        );
        assert!(
            builds <= 1 && full_rebuilds == 0,
            "mid-tween frame {frame} built its display list {builds} times ({full_rebuilds} \
             full rebuilds after the pass): one build per frame"
        );
    }
    restore_ambient_recording();
}
