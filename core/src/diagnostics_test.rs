#[allow(unused_imports)]
pub use super::*;
#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    /// A recorder of this test's own that says nothing on stderr: no other test reads or writes it,
    /// so nothing here takes [`test_lock`].
    fn quiet() -> Diagnostics {
        let diagnostics = Diagnostics::new();
        diagnostics.set_sink(|_| {});
        diagnostics
    }

    #[test]
    fn a_recorded_diagnostic_is_findable_and_clearable() {
        let d = quiet();
        d.emit("[azul][test-lint] hello".to_string());
        assert!(d.any_contains("test-lint"));
        assert!(!d.any_contains("absent-lint"));
        d.clear();
        assert!(!d.any_contains("test-lint"), "clear() must empty the ring");
    }

    #[test]
    fn the_ring_is_bounded() {
        let d = quiet();
        for i in 0..(CAPACITY + 50) {
            d.record(format!("msg {i}"));
        }
        assert_eq!(
            d.recorded().len(),
            CAPACITY,
            "an every-frame lint must not grow without bound — the point is to catch churn, not \
             become it"
        );
        assert!(
            !d.any_contains("msg 0"),
            "the oldest entries are dropped first"
        );
        assert_eq!(
            d.recorded().last().map(String::as_str),
            Some(format!("msg {}", CAPACITY + 49).as_str()),
            "the newest is kept"
        );
    }

    /// Two recorders share nothing: what one says, the other has not heard, and clearing one
    /// leaves the other's ring and findings alone.
    #[test]
    fn a_recorder_of_its_own_shares_nothing_with_another() {
        let (a, b) = (quiet(), quiet());
        a.emit("[azul][test-lint] only for a".to_string());
        assert!(a.any_contains("only for a"));
        assert!(!b.any_contains("only for a"));

        assert!(a.emit_once(7, || "[azul][test-lint] once".to_string()));
        assert!(b.emit_once(7, || "[azul][test-lint] once".to_string()), "b has not said it");
        b.clear();
        assert!(!a.emit_once(7, || "[azul][test-lint] once".to_string()), "a still has");
        assert!(a.any_contains("only for a"), "b's clear did not touch a's ring");
    }

    /// A finding is said once per key; clearing forgets the keys with the ring.
    #[test]
    fn a_finding_is_said_once_until_the_recorder_is_cleared() {
        let d = quiet();
        let said = || {
            d.recorded()
                .iter()
                .filter(|line| line.contains("[azul][test-lint] finding"))
                .count()
        };
        assert!(d.emit_once(1, || "[azul][test-lint] finding".to_string()));
        assert!(!d.emit_once(1, || "[azul][test-lint] finding".to_string()));
        assert!(d.emit_once(2, || "[azul][test-lint] finding".to_string()), "another key");
        assert_eq!(said(), 2);

        d.clear();
        assert!(d.emit_once(1, || "[azul][test-lint] finding".to_string()), "forgotten");
        assert_eq!(said(), 1);
    }

    /// Past the cap no new finding is said: one notice, then nothing, until the recorder is
    /// cleared.
    #[test]
    fn the_findings_are_capped_with_one_notice() {
        let d = quiet();
        for key in 0..MAX_ONCE_KEYS as u64 {
            assert!(d.emit_once(key, || format!("[azul][test-lint] finding {key}")));
        }
        assert!(!d.emit_once(u64::MAX, || "[azul][test-lint] too many".to_string()));
        assert!(!d.emit_once(u64::MAX - 1, || "[azul][test-lint] too many".to_string()));
        let notices = d
            .recorded()
            .iter()
            .filter(|line| line.contains("distinct findings reported"))
            .count();
        assert_eq!(notices, 1, "the notice is said once");
        assert!(!d.any_contains("too many"));

        d.clear();
        assert!(d.emit_once(u64::MAX, || "[azul][test-lint] too many".to_string()));
    }

    /// The scenario that is running tags what is said, and what is said after it ends is not tagged.
    #[test]
    fn a_diagnostic_is_tagged_with_the_running_scenario() {
        let d = quiet();
        d.set_scope(Some("scroll_momentum".to_string()));
        assert_eq!(d.current_scope().as_deref(), Some("scroll_momentum"));
        d.emit("[azul][test-lint] inside".to_string());
        d.set_scope(None);
        d.emit("[azul][test-lint] outside".to_string());
        assert_eq!(
            d.recorded(),
            vec![
                "[test=scroll_momentum] [azul][test-lint] inside".to_string(),
                "[azul][test-lint] outside".to_string(),
            ]
        );
    }
}

/// A diagnostic must never take the process down because its destination
/// closed. `eprintln!` panics on a failed write, so piping a running app into
/// `head` killed it the moment `head` exited - see `write_diagnostic`'s own
/// doc for the captured backtrace.
#[test]
fn a_diagnostic_survives_a_destination_that_is_gone() {
    use std::io::{Error, ErrorKind, Write};

    struct ClosedPipe;
    impl Write for ClosedPipe {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(Error::new(ErrorKind::BrokenPipe, "broken pipe"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(Error::new(ErrorKind::BrokenPipe, "broken pipe"))
        }
    }

    // The assertion is that this RETURNS. With `writeln!(..).unwrap()` or
    // `eprintln!` behind it, this test panics instead.
    super::write_diagnostic(ClosedPipe, "a warning nobody can hear");
}
