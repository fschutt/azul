//! The loop detector: a file that keeps flipping between the same states is frozen.

use azul_storage::testing::TempDir;

use crate::sync::loops::{LoopDetector, REPEATS, WINDOW_SECS};

fn state(s: &str) -> Option<String> {
    Some(s.to_string())
}

#[test]
fn a_file_deleted_and_put_back_again_and_again_is_frozen_on_its_third_return() {
    let mut d = LoopDetector::default();
    let mut t = 1000.0;
    let mut applied = 0;
    // put, delete, put, delete, put: the fifth step is the third return.
    for next in [state("A"), None, state("A"), None, state("A"), None] {
        t += 10.0;
        if d.frozen("flap.txt", &next, t) {
            break;
        }
        d.applied("flap.txt", next, t);
        applied += 1;
    }
    assert_eq!(applied, 4, "two puts and two deletes go through, the third put is held");
    assert!(d.frozen("flap.txt", &None, t + 1.0), "it stays frozen");
    assert_eq!(REPEATS, 3);
}

#[test]
fn a_growing_file_never_repeats_a_state_and_never_trips() {
    let mut d = LoopDetector::default();
    for i in 0..100 {
        let next = state(&format!("log-{i}"));
        assert!(!d.frozen("app.log", &next, f64::from(i)));
        d.applied("app.log", next, f64::from(i));
    }
}

#[test]
fn states_older_than_the_window_are_forgotten() {
    let mut d = LoopDetector::default();
    for (i, next) in [state("A"), None, state("A"), None].into_iter().enumerate() {
        d.applied("f", next, i as f64);
    }
    assert!(d.frozen("f", &state("A"), 5.0));
    assert!(!d.frozen("f", &state("A"), 5.0 + WINDOW_SECS));
    d.forget_old(10.0 + WINDOW_SECS);
    assert!(d.history.is_empty());
}

#[test]
fn the_user_releasing_a_file_starts_its_history_afresh_and_it_survives_the_process() {
    let dir = TempDir::new("loops");
    let mut d = LoopDetector::default();
    for (i, next) in [state("A"), None, state("A"), None].into_iter().enumerate() {
        d.applied("f", next, i as f64);
    }
    d.save(dir.path()).unwrap();
    let mut again = LoopDetector::load(dir.path());
    assert!(again.frozen("f", &state("A"), 5.0));
    again.release("f");
    assert!(!again.frozen("f", &state("A"), 5.0));
}
