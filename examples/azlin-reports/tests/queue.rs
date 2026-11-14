//! Check R15 of lane W1-S46a: the order of the Reports page and the due dates (planning/architecture.md section
//! 3.9: "Reports for CSAM, terrorism or a threat to life head the Reports page and the daily report; an ordinary
//! report is decided within 7 days est."). Every test enters through the public API of the crate.

mod common;

use azlin_reports::{
    due, order, overdue, Day, Days, Due, QueueItem, Reason, ReportId, ORDINARY_DAYS,
};

use common::*;

fn item(id: u8, reason: Reason, received: u32, decided: bool) -> QueueItem {
    QueueItem {
        id: rid(id),
        reason,
        received: day(received),
        decided,
    }
}

/// The order the page must have, written out from the sentence of the plan: urgent first, then oldest first,
/// then by id.
fn expected_order(items: &[QueueItem]) -> Vec<QueueItem> {
    let mut v = items.to_vec();
    v.sort_by_key(|i| {
        let urgent = matches!(i.reason, Reason::Csam | Reason::Terrorism);
        (
            !urgent,
            i.received.get(),
            *i.id.as_bytes(),
            i.reason.code(),
            i.decided,
        )
    });
    v
}

fn random_items(rng: &mut Rng, n: usize) -> Vec<QueueItem> {
    (0..n)
        .map(|_| QueueItem {
            // Few distinct days and few distinct ids, so that ties are common: the id and the rest decide.
            id: ReportId::from_bytes([rng.below(12) as u8; 16]),
            reason: rng.pick(&Reason::ALL),
            received: day(500 + rng.below(4) as u32),
            decided: rng.one_in(3),
        })
        .collect()
}

#[test]
fn r15_the_queue_order_is_total_and_stable() {
    // Pinned: urgent reasons first (Csam and Terrorism), oldest first, then by id.
    let mut items = vec![
        item(9, Reason::Malware, 10, false),
        item(5, Reason::Other, 10, false),
        item(7, Reason::Terrorism, 30, false),
        item(2, Reason::Phishing, 5, false),
        item(8, Reason::Csam, 30, false),
        item(1, Reason::Csam, 31, false),
        item(6, Reason::Copyright, 10, true),
    ];
    order(&mut items);
    let ids: Vec<u8> = items.iter().map(|i| i.id.as_bytes()[0]).collect();
    assert_eq!(
        ids,
        vec![7, 8, 1, 2, 5, 6, 9],
        "urgent (by day, then id), then oldest, then id"
    );

    // Total and stable: the sort of a shuffled list is identical for 2,000 shuffles.
    for seed in 0..2_000u64 {
        let mut rng = Rng::new(seed);
        let n = rng.range(1, 30) as usize;
        let mut items = random_items(&mut rng, n);
        let expected = expected_order(&items);
        rng.shuffle(&mut items);
        order(&mut items);
        assert_eq!(
            items, expected,
            "seed {seed}: the order depends on the order the rows came in"
        );
    }
    // Ordering twice changes nothing.
    let mut rng = Rng::new(77);
    let mut items = random_items(&mut rng, 40);
    order(&mut items);
    let once = items.clone();
    order(&mut items);
    assert_eq!(items, once);
}

#[test]
fn r15_due_is_immediately_for_the_urgent_ones_and_received_plus_seven_days_otherwise() {
    assert_eq!(ORDINARY_DAYS, Days::new(7));
    for reason in Reason::ALL {
        let it = item(1, reason, 1_000, false);
        match reason {
            Reason::Csam | Reason::Terrorism => {
                assert_eq!(due(&it), Due::Immediately, "{reason:?}")
            }
            _ => assert_eq!(due(&it), Due::By(day(1_007)), "{reason:?}"),
        }
        assert_eq!(reason.is_urgent(), due(&it) == Due::Immediately);
    }
    assert_eq!(Day::new(10).plus(Days::new(7)), Day::new(17));
}

#[test]
fn r15_overdue_lists_exactly_the_undecided_items_past_due() {
    let items = [
        item(1, Reason::Malware, 100, false),  // due by 107
        item(2, Reason::Phishing, 100, true),  // decided: never overdue
        item(3, Reason::Csam, 100, false),     // immediately
        item(4, Reason::Terrorism, 100, true), // decided
        item(5, Reason::Other, 101, false),    // due by 108
        item(6, Reason::Copyright, 99, false), // due by 106
    ];
    let ids = |today: u32| -> Vec<u8> {
        overdue(&items, day(today))
            .iter()
            .map(|i| i.as_bytes()[0])
            .collect()
    };
    // Before anything is received nothing is overdue.
    assert_eq!(ids(98), Vec::<u8>::new());
    // The urgent report is overdue from the day it was received, the ordinary ones are on time through their
    // due day and overdue after it.
    assert_eq!(ids(100), vec![3]);
    assert_eq!(
        ids(106),
        vec![3],
        "item 6 is still on time on its due day (99 + 7 = 106)"
    );
    assert_eq!(
        ids(107),
        vec![3, 6],
        "item 6 is overdue the day after its due day"
    );
    assert_eq!(
        ids(108),
        vec![3, 6, 1],
        "item 1 (100 + 7 = 107) is overdue on 108; the list is in page order"
    );
    assert_eq!(ids(109), vec![3, 6, 1, 5]);
    // A decided item is never listed, whatever the day.
    assert!(!ids(10_000).contains(&2) && !ids(10_000).contains(&4));
    // The list is in the order of the page: urgent first, oldest first.
    let mut page = items;
    order(&mut page);
    let expected: Vec<ReportId> = page.iter().filter(|i| !i.decided).map(|i| i.id).collect();
    assert_eq!(overdue(&items, day(10_000)), expected);
}

#[test]
fn r15_overdue_matches_a_model_on_random_lists() {
    for seed in 0..2_000u64 {
        let mut rng = Rng::new(seed);
        let n = rng.range(0, 25) as usize;
        let mut items = random_items(&mut rng, n);
        // Distinct ids, so that the list can be compared by id.
        for (i, it) in items.iter_mut().enumerate() {
            it.id = ReportId::from_bytes([i as u8; 16]);
        }
        let today = day(498 + rng.below(20) as u32);
        let mut page = items.clone();
        order(&mut page);
        let expected: Vec<ReportId> = page
            .iter()
            .filter(|i| !i.decided)
            .filter(|i| match i.reason {
                Reason::Csam | Reason::Terrorism => today.get() >= i.received.get(),
                _ => today.get() > i.received.get() + 7,
            })
            .map(|i| i.id)
            .collect();
        assert_eq!(overdue(&items, today), expected, "seed {seed}");
    }
}
