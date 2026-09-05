#[allow(unused_imports)]
pub use super::*;
#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    // Uses the workspace-wide lock: see `diagnostics::test_lock`.

    #[test]
    fn a_recorded_diagnostic_is_findable_and_clearable() {
        let _g = test_lock().lock();
        clear();
        emit("[azul][test-lint] hello".to_string());
        assert!(any_contains("test-lint"));
        assert!(!any_contains("absent-lint"));
        clear();
        assert!(!any_contains("test-lint"), "clear() must empty the ring");
    }

    #[test]
    fn the_ring_is_bounded() {
        let _g = test_lock().lock();
        clear();
        for i in 0..(CAPACITY + 50) {
            record(format!("msg {i}"));
        }
        assert_eq!(
            recorded().len(),
            CAPACITY,
            "an every-frame lint must not grow without bound — the point is to \
             catch churn, not become it"
        );
        assert!(
            !any_contains("msg 0"),
            "the oldest entries are dropped first"
        );
        clear();
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
