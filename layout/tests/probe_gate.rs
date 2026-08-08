//! The probe RECORDING gate is a shipping-memory contract, not a nicety.
//!
//! The dll compiles azul-layout with `probe` unconditionally, and the event
//! buffer is only drained by the `AZ_PROFILE=cpu` report. Before the gate,
//! every span in a PLAIN run pushed ~40 B into a thread-local Vec nothing
//! ever emptied — unbounded RSS growth proportional to frames rendered
//! (a 5 s resize drag ≈ 375 relayouts × hundreds of spans), and invisible
//! to the LayoutCache memory walk because a thread-local owns it.
//!
//! This lives in its own integration binary ON PURPOSE: `set_recording` is
//! a process-global toggle, and flipping it OFF inside the shared unit-test
//! binary would race any concurrently running test that asserts events
//! appear. A separate binary is a separate process — the only writer wins.

use azul_layout::probe::Probe;

#[test]
fn spans_are_inert_until_recording_is_switched_on() {
    // Fresh process, no AZ_PROFILE in the battery environment: the lazy
    // gate must resolve to OFF, so this span buffers nothing. (If the
    // environment ever exports AZ_PROFILE, force the same starting state
    // instead of failing spuriously.)
    if std::env::var("AZ_PROFILE").is_ok() {
        Probe::set_recording(false);
    }
    {
        let _g = Probe::span("gate_off_span");
        Probe::sample_rss("gate_off_rss", 123);
    }
    assert_eq!(
        Probe::peek_len(),
        0,
        "recording is off — a span/sample must not buffer events"
    );

    // Flip ON: the exact same calls must now record — proving the zero
    // above came from the gate, not from a probe that cannot see at all
    // ("a zero is not a measurement" rule).
    Probe::set_recording(true);
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

    // Flip OFF again: the gate must close, not latch.
    Probe::set_recording(false);
    {
        let _g = Probe::span("gate_off_again");
    }
    assert_eq!(Probe::peek_len(), 0, "recording off again — buffer stays empty");
}
