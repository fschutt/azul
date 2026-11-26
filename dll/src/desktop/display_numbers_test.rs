//! The system's own numbers of the displays, put on the monitors: pure, no display.

use azul_core::window::{Monitor, MonitorId};

use super::number_monitors;

fn monitors(n: usize) -> Vec<Monitor> {
    (0..n).map(|i| Monitor { monitor_id: MonitorId { index: i, hash: i as u64 + 1 }, ..Monitor::default() }).collect()
}

/// The n-th number goes to the n-th monitor; the monitors keep everything else.
///
/// Mutation: the numbers in the wrong order; a monitor that loses its id.
#[test]
fn the_nth_number_goes_to_the_nth_monitor() {
    let numbered = number_monitors(monitors(3), &[69_733_382, 1_234, 99]);
    assert_eq!(numbered.iter().map(|m| m.native_id).collect::<Vec<_>>(), vec![69_733_382, 1_234, 99]);
    assert_eq!(numbered.iter().map(|m| m.monitor_id.index).collect::<Vec<_>>(), vec![0, 1, 2]);
}

/// A monitor the system gave no number keeps 0, whether the list is empty (a system without numbers) or shorter
/// than the monitors, and a list longer than the monitors is not an error.
///
/// Mutation: a missing number that is taken from another monitor.
#[test]
fn a_monitor_without_a_number_keeps_zero() {
    assert_eq!(number_monitors(monitors(2), &[]).iter().map(|m| m.native_id).collect::<Vec<_>>(), vec![0, 0]);
    assert_eq!(number_monitors(monitors(3), &[7]).iter().map(|m| m.native_id).collect::<Vec<_>>(), vec![7, 0, 0]);
    assert_eq!(number_monitors(monitors(1), &[7, 8, 9]).iter().map(|m| m.native_id).collect::<Vec<_>>(), vec![7]);
    assert!(number_monitors(Vec::new(), &[7]).is_empty());
}

/// A monitor made by hand has no number.
#[test]
fn a_monitor_starts_without_a_number() {
    assert_eq!(Monitor::default().native_id, 0);
}
