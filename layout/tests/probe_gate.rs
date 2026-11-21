//! The probe RECORDING gate is a shipping-memory contract, not a nicety.
//!
//! The dll compiles azul-layout with `probe` unconditionally, and the event
//! buffer is only drained by the `AZ_PROFILE=cpu` report. Before the gate,
//! every span in a PLAIN run pushed ~40 B into a thread-local Vec nothing
//! ever emptied — unbounded RSS growth proportional to frames rendered
//! (a 5 s resize drag ≈ 375 relayouts × hundreds of spans), and invisible
//! to the LayoutCache memory walk because a thread-local owns it.
//!
//! ## Isolation
//!
//! `Probe::set_recording` writes a PROCESS-wide switch; what the gate is
//! about is what a thread does with its own spans, and a thread can choose
//! that for itself (`Probe::record_on_this_thread`), until its guard drops.
//! This file chooses for its own test thread only, so it shares the binary
//! with every other test that records spans (`frame_perf`, the layout-cost
//! tests) without a lock around the profiler and without leaving the process
//! switch in the state it found.
//!
//! The initial `drain` below covers the other half: with `--test-threads=1`
//! libtest runs every test on the SAME thread, so the thread-local buffer is
//! shared and a neighbour could leave events in it.

use azul_layout::probe::Probe;

#[test]
fn spans_are_inert_until_recording_is_switched_on() {
    // Fresh buffer, recording off. (Under --test-threads=1 the thread-local
    // is shared with every other test in this binary, so drain rather than
    // assume; and if the environment exports AZ_PROFILE, force the same
    // starting state instead of failing spuriously.)
    let off = Probe::record_on_this_thread(false);
    let _ = Probe::drain();

    {
        let _g = Probe::span("gate_off_span");
        Probe::sample_rss("gate_off_rss", 123);
    }
    assert_eq!(
        Probe::peek_len(),
        0,
        "recording is off — a span/sample must not buffer events"
    );
    drop(off);

    // Flip ON: the exact same calls must now record — proving the zero
    // above came from the gate, not from a probe that cannot see at all
    // ("a zero is not a measurement" rule).
    let on = Probe::record_on_this_thread(true);
    {
        let _g = Probe::span("gate_on_span");
        Probe::sample_rss("gate_on_rss", 456);
    }
    let events = Probe::drain();
    if Probe::enabled() {
        assert_eq!(events.len(), 2, "recording on — both events must buffer");
        assert_eq!(events[0].name, "gate_on_rss"); // sample fires inside the span
        assert_eq!(events[1].name, "gate_on_span");
    } else {
        // Stub imp (feature off): nothing records regardless of the toggle.
        assert!(events.is_empty(), "stub imp must never buffer");
    }
    drop(on);

    // Flip OFF again: the gate must close, not latch.
    let off_again = Probe::record_on_this_thread(false);
    {
        let _g = Probe::span("gate_off_again");
    }
    assert_eq!(
        Probe::peek_len(),
        0,
        "recording off again — buffer stays empty"
    );
    drop(off_again);
    let _ = Probe::drain();
}
