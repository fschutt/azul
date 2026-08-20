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
//! `Probe::set_recording` writes a PROCESS-GLOBAL `AtomicU8`; only the event
//! buffer it gates is thread-local. This file used to be its own integration
//! binary for exactly that reason — a separate process meant a single writer.
//! It is now a module of `tests/all.rs`, so the isolation is explicit instead:
//! every test that touches the flag takes [`crate::probe_lock`], which
//! serialises this file against `frame_perf` and `pagination_perf` (the two
//! readers that drain and attribute spans). Adding a fourth test that calls
//! `Probe::set_recording`, `drain` or `peek_len` means taking that lock too.
//!
//! The initial `drain` below covers the other half: with `--test-threads=1`
//! libtest runs every test on the SAME thread, so the thread-local buffer is
//! shared and a neighbour could leave events in it.

use azul_layout::probe::Probe;

#[test]
fn spans_are_inert_until_recording_is_switched_on() {
    let _serialised = crate::probe_lock();

    // Fresh buffer, recording off. (Under --test-threads=1 the thread-local
    // is shared with every other test in this binary, so drain rather than
    // assume; and if the environment exports AZ_PROFILE, force the same
    // starting state instead of failing spuriously.)
    Probe::set_recording(false);
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

    // Put the AMBIENT gate back before releasing the lock. In its own process
    // this did not matter — the process ended. Here, `frame_perf` and
    // `pagination_perf` share the binary and expect the state `AZ_PROFILE`
    // asked for; leaving the flag forced OFF would silently reduce them to
    // "no probe events". There is no public getter for the recording flag, so
    // re-derive it exactly the way `probe::imp`'s lazy gate does.
    let ambient = azul_core::profile::cpu_enabled()
        || azul_core::profile::memory_enabled()
        || azul_core::profile::heap_enabled();
    Probe::set_recording(ambient);
    let _ = Probe::drain();
}
