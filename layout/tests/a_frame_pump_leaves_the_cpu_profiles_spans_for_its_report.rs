//! A frame pump leaves the CPU profile's spans for its report.
//!
//! `AZ_PROFILE=cpu` prints the probe's spans once per layout pass, at the end
//! of each DOM's pass (`layout_dom_recursive_impl`). Everything a relayout
//! does AFTER its last DOM pass - the accessibility tree, the scroll
//! registration, the hit-tester rebuild, the shell's own span - was recorded
//! and then thrown away: the telemetry `FramePump` around every present (and
//! every `regenerate_layout`) drains the same buffer on drop, and with
//! telemetry off it discards what it drained. So the wave-8 knob-tick
//! profiles never showed `a11y_update_tree` (~3 ms of every tick) and
//! LAYOUTPERF8B saw it only after a DOM rebuild (A11YPATCH8, 2026-10-03).
//!
//! The rule: with telemetry not collecting, a frame pump leaves the spans to
//! the CPU report when there is one, and still drops them (the buffer stays
//! bounded) when there is none.
//!
//! Not compiled by the author (house rule). Expected RED before the fix: the
//! first test.

use azul_layout::{
    probe::{Event, Probe},
    telemetry,
};

const SPAN: &str = "a_span_after_the_last_layout_pass";

/// Record one span, let a frame pump drain with or without a CPU report
/// reading the buffer, and return what is left in it.
fn left_after_a_frame_pump(cpu_report: bool) -> Vec<Event> {
    Probe::set_recording(true);
    let _ = Probe::drain();
    {
        let _span = Probe::span(SPAN);
    }
    let _ = telemetry::drain_probe_events_for(cpu_report);
    let left = Probe::drain();
    // Put the recording flag back the way `AZ_PROFILE` asked for it.
    let ambient = azul_core::profile::cpu_enabled()
        || azul_core::profile::memory_enabled()
        || azul_core::profile::heap_enabled();
    Probe::set_recording(ambient);
    left
}

#[test]
fn a_frame_pump_leaves_the_spans_to_the_cpu_report() {
    let _serialised = crate::probe_lock();
    if !Probe::enabled() {
        eprintln!("[a_frame_pump] the probe is compiled out: nothing to record");
        return;
    }
    let left = left_after_a_frame_pump(true);
    assert!(
        left.iter().any(|e| e.name == SPAN),
        "the frame pump discarded a span the AZ_PROFILE=cpu report had not printed yet"
    );
}

#[test]
fn with_no_report_reading_them_a_frame_pump_still_drops_the_spans() {
    let _serialised = crate::probe_lock();
    if !Probe::enabled() {
        eprintln!("[a_frame_pump] the probe is compiled out: nothing to record");
        return;
    }
    let left = left_after_a_frame_pump(false);
    assert!(
        left.is_empty(),
        "with neither telemetry nor a CPU report reading the buffer, a frame pump must empty it \
         (it grew without bound otherwise); left: {}",
        left.len()
    );
}
