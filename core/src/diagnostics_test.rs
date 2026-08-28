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
