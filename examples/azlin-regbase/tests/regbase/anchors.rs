//! S9A-30 `anchors_windows`: `identity_at`, `payments_at` and `operator_at` over open-ended and
//! overlapping windows at the boundary seconds; an empty set returns no key (the callers refuse:
//! an empty anchor set is never "no check").

use azlin_regbase::{
    AnchorSet, BoundError, KeyWindow, LeaderTime, MetaGroupId, PublicKey32, ANCHOR_WINDOWS_MAX,
};

fn at(secs: u64) -> LeaderTime {
    LeaderTime::from_secs(secs)
}

fn key(n: u8) -> PublicKey32 {
    PublicKey32::from_bytes([n; 32])
}

fn window(n: u8, from: u64, until: Option<u64>) -> KeyWindow {
    KeyWindow {
        public: key(n),
        not_before: at(from),
        not_after: until.map(at),
    }
}

fn keys(found: Vec<&KeyWindow>) -> Vec<u8> {
    found.iter().map(|w| w.public.as_bytes()[0]).collect()
}

#[test]
fn s9a_30_a_window_is_not_before_inclusive_and_not_after_exclusive() {
    let w = window(1, 100, Some(200));
    assert!(!w.contains(at(0)));
    assert!(!w.contains(at(99)));
    assert!(w.contains(at(100)), "the first second is inside");
    assert!(w.contains(at(150)));
    assert!(w.contains(at(199)), "the last second is inside");
    assert!(
        !w.contains(at(200)),
        "not_after is the first second outside"
    );
    assert!(!w.contains(at(201)));
    // open-ended: never ends
    let open = window(2, 100, None);
    assert!(!open.contains(at(99)));
    assert!(open.contains(at(100)));
    assert!(open.contains(at(u64::MAX)));
    // a window that holds no second contains nothing
    let empty = window(3, 100, Some(100));
    assert!(!empty.contains(at(99)) && !empty.contains(at(100)) && !empty.contains(at(101)));
}

#[test]
fn s9a_30_payments_and_operator_windows_overlap_during_a_rotation() {
    let set = AnchorSet {
        payments: vec![window(1, 0, Some(1_000)), window(2, 900, None)],
        operator: vec![window(5, 0, None), window(6, 500, Some(700))],
        identity: Vec::new(),
        map_epoch: 4,
    };
    assert_eq!(keys(set.payments_at(at(0))), [1]);
    assert_eq!(keys(set.payments_at(at(899))), [1]);
    assert_eq!(
        keys(set.payments_at(at(900))),
        [1, 2],
        "the overlap verifies both keys"
    );
    assert_eq!(keys(set.payments_at(at(999))), [1, 2]);
    assert_eq!(
        keys(set.payments_at(at(1_000))),
        [2],
        "the old key ends at its not_after"
    );
    assert_eq!(keys(set.payments_at(at(u64::MAX))), [2]);

    assert_eq!(keys(set.operator_at(at(499))), [5]);
    assert_eq!(keys(set.operator_at(at(500))), [5, 6]);
    assert_eq!(keys(set.operator_at(at(699))), [5, 6]);
    assert_eq!(keys(set.operator_at(at(700))), [5]);
    // the lists do not mix
    assert!(set.identity_at(MetaGroupId::FIRST, at(950)).is_empty());
}

#[test]
fn s9a_30_identity_windows_are_per_metadata_group() {
    let g0 = MetaGroupId::FIRST;
    let g1 = MetaGroupId::new(1);
    let g9 = MetaGroupId::new(9);
    let set = AnchorSet {
        payments: Vec::new(),
        operator: Vec::new(),
        identity: vec![
            (g0, window(10, 0, Some(100))),
            (g1, window(11, 0, None)),
            (g0, window(12, 100, None)),
            (g0, window(13, 50, Some(150))),
        ],
        map_epoch: 1,
    };
    assert_eq!(keys(set.identity_at(g0, at(49))), [10]);
    assert_eq!(keys(set.identity_at(g0, at(50))), [10, 13]);
    assert_eq!(
        keys(set.identity_at(g0, at(100))),
        [12, 13],
        "10 ends at 100, 12 starts at 100"
    );
    assert_eq!(keys(set.identity_at(g0, at(150))), [12]);
    assert_eq!(keys(set.identity_at(g1, at(1_000_000))), [11]);
    assert!(
        set.identity_at(g9, at(10)).is_empty(),
        "a group with no key has no key"
    );
    assert!(set.identity_at(g1, at(0)).len() == 1);
}

#[test]
fn s9a_30_an_empty_anchor_set_returns_no_key() {
    let set = AnchorSet::empty();
    assert!(set.is_empty());
    assert_eq!(set, AnchorSet::default());
    for t in [0, 1, 1_800_000_000, u64::MAX] {
        assert!(set.payments_at(at(t)).is_empty());
        assert!(set.operator_at(at(t)).is_empty());
        assert!(set.identity_at(MetaGroupId::FIRST, at(t)).is_empty());
    }
    // a set with only one list is not empty
    let some = AnchorSet {
        payments: vec![window(1, 0, None)],
        ..AnchorSet::empty()
    };
    assert!(!some.is_empty());
}

#[test]
fn s9a_30_the_windows_of_a_list_are_bounded() {
    let many = |n: usize| {
        (0..n)
            .map(|i| window(1, i as u64, None))
            .collect::<Vec<_>>()
    };
    let ok = AnchorSet {
        payments: many(ANCHOR_WINDOWS_MAX),
        ..AnchorSet::empty()
    };
    assert_eq!(ok.check(), Ok(()));
    let over = AnchorSet {
        operator: many(ANCHOR_WINDOWS_MAX + 1),
        ..AnchorSet::empty()
    };
    assert_eq!(
        over.check(),
        Err(BoundError::TooMany {
            what: "AnchorSet.operator",
            count: ANCHOR_WINDOWS_MAX + 1,
            max: ANCHOR_WINDOWS_MAX
        })
    );
    let identity: Vec<(MetaGroupId, KeyWindow)> = (0..=ANCHOR_WINDOWS_MAX)
        .map(|i| (MetaGroupId::new(i as u16), window(2, 0, None)))
        .collect();
    assert!(AnchorSet {
        identity,
        ..AnchorSet::empty()
    }
    .check()
    .is_err());
}
