//! The restore protocol (RF45): `judge` over its whole truth table, `baseline`, and the pure
//! `rebase` that makes a device's base untrusted where a restored drive disagrees with it.

use std::collections::BTreeMap;

use crate::restore::{
    baseline, judge, rebase, went_back, Clock, Remote, RestoreKind, RestoreNotice, Seen, Standing,
};

const PREFIX: &str = "azlin/data/";

fn notice(epoch: u64, prefix: &str) -> RestoreNotice {
    RestoreNotice {
        epoch,
        prefix: prefix.to_string(),
        ..RestoreNotice::default()
    }
}

/// The device last saw epoch 5 and counter 10.
fn seen() -> Seen {
    Seen {
        epoch: 5,
        clock: 10,
    }
}

/// What the drive serves relative to the memory's counter 10.
#[derive(Clone, Copy, Debug)]
enum Served {
    Below,
    Same,
    Above,
}

impl Served {
    fn counter(self) -> u64 {
        match self {
            Served::Below => 7,
            Served::Same => 10,
            Served::Above => 12,
        }
    }
}

/// The announcement the token server holds, relative to the memory's epoch 5.
#[derive(Clone, Copy, Debug)]
enum Said {
    Nothing,
    /// An epoch the device accepted already (equal or lower): no excuse for anything.
    Old,
    /// A newer epoch for the folder's own prefix.
    NewCovering,
    /// A newer epoch for a prefix that does not touch this folder.
    NewElsewhere,
}

impl Said {
    fn notice(self) -> Option<RestoreNotice> {
        match self {
            Said::Nothing => None,
            Said::Old => Some(notice(5, "")),
            Said::NewCovering => Some(notice(6, "")),
            Said::NewElsewhere => Some(notice(6, "mail/")),
        }
    }
}

/// The whole table of `judge`, written out by hand (not computed from the code under test):
/// {served below, same, above} x {nothing, old, new covering, new elsewhere} x {Shared, Own}.
#[test]
fn rf45_judge_truth_table_is_exhaustive() {
    use Said::{NewCovering, NewElsewhere, Nothing, Old};
    use Served::{Above, Below, Same};
    let rollback = Standing::Rollback {
        seen: 10,
        served: 7,
    };
    let restored = Standing::Restored(notice(6, ""));
    #[rustfmt::skip]
    let shared: [(Served, Said, Standing); 12] = [
        (Below, Nothing,      rollback.clone()),
        (Below, Old,          rollback.clone()),
        (Below, NewCovering,  restored.clone()),
        (Below, NewElsewhere, rollback),
        (Same,  Nothing,      Standing::InSync),
        (Same,  Old,          Standing::InSync),
        (Same,  NewCovering,  restored.clone()),
        (Same,  NewElsewhere, Standing::InSync),
        (Above, Nothing,      Standing::Behind),
        (Above, Old,          Standing::Behind),
        (Above, NewCovering,  restored.clone()),
        (Above, NewElsewhere, Standing::Behind),
    ];
    for (served, said, expected) in shared {
        let got = judge(
            &seen(),
            served.counter(),
            Clock::Shared,
            said.notice().as_ref(),
            PREFIX,
        );
        assert_eq!(
            got, expected,
            "Shared, served {served:?}, announced {said:?}"
        );
    }
    // A counter that counts this device's own commits says nothing about the drive: only the
    // epoch decides, whatever the counter is.
    #[rustfmt::skip]
    let own: [(Said, Standing); 4] = [
        (Nothing,      Standing::InSync),
        (Old,          Standing::InSync),
        (NewCovering,  restored),
        (NewElsewhere, Standing::InSync),
    ];
    for served in [Below, Same, Above] {
        for (said, expected) in &own {
            let got = judge(
                &seen(),
                served.counter(),
                Clock::Own,
                said.notice().as_ref(),
                PREFIX,
            );
            assert_eq!(&got, expected, "Own, served {served:?}, announced {said:?}");
        }
    }
}

/// The counter "went back" only for a shared counter that is older than the memory's: the
/// message to the user about a restore, never a rule.
#[test]
fn went_back_is_a_shared_counter_older_than_the_memory() {
    assert!(went_back(&seen(), 7, Clock::Shared));
    assert!(!went_back(&seen(), 10, Clock::Shared));
    assert!(!went_back(&seen(), 12, Clock::Shared));
    assert!(
        !went_back(&seen(), 0, Clock::Own),
        "an own counter says nothing of the drive"
    );
}

/// S3: the announcement is looked at whatever the counter says. Another device that synced first
/// after the restore and committed on top of it makes the counters equal again; the old rule
/// (look only when the counter went back) read this as "in sync" and never healed.
#[test]
fn az_s3_an_announcement_is_judged_whatever_the_counter_says() {
    let said = notice(6, "");
    for served in [7, 10, 12] {
        assert_eq!(
            judge(&seen(), served, Clock::Shared, Some(&said), PREFIX),
            Standing::Restored(said.clone()),
            "served {served}"
        );
    }
}

/// A restore of a prefix the folder is not in, or lies in, is judged by `covers`: the whole
/// drive, the folder inside the restored prefix, the restored prefix inside the folder.
#[test]
fn a_restore_covers_the_folders_it_touched_and_no_others() {
    let n = |prefix: &str| notice(6, prefix);
    assert!(n("").covers("azlin/data/"), "the whole drive");
    assert!(
        n("azlin/").covers("azlin/data/"),
        "the folder lies in the restored prefix"
    );
    assert!(
        n("azlin/data/docs/").covers("azlin/data/"),
        "the restored prefix lies in the folder"
    );
    assert!(!n("mail/").covers("azlin/data/"), "another folder");
    assert!(
        matches!(
            judge(&seen(), 10, Clock::Shared, Some(&n("mail/")), "azlin/data/"),
            Standing::InSync
        ),
        "an announcement elsewhere excuses nothing and changes nothing"
    );
}

/// S3/baseline: a new or reset pairing starts at the current epoch, not 0; an announcement made
/// before the pairing existed is then not new to it, and a LATER unannounced rollback is refused
/// instead of excused by the stale notice.
#[test]
fn az_s3_a_new_pairing_starts_at_the_current_epoch() {
    assert_eq!(baseline(None), 0);
    assert_eq!(baseline(Some(&notice(9, "docs/"))), 9);
    let said = notice(9, "");
    let born = Seen::fresh(Some(&said));
    assert_eq!(born, Seen { epoch: 9, clock: 0 });
    // The stale notice no longer restores anything for this pairing ...
    assert_eq!(
        judge(
            &Seen { clock: 20, ..born },
            20,
            Clock::Shared,
            Some(&said),
            PREFIX
        ),
        Standing::InSync
    );
    // ... and a rollback after it is a rollback.
    assert_eq!(
        judge(
            &Seen { clock: 20, ..born },
            15,
            Clock::Shared,
            Some(&said),
            PREFIX
        ),
        Standing::Rollback {
            seen: 20,
            served: 15
        }
    );
    // A pairing born without any announcement starts at 0: the first one restores.
    let first = notice(1, "");
    assert_eq!(
        judge(&Seen::fresh(None), 0, Clock::Shared, Some(&first), PREFIX),
        Standing::Restored(first)
    );
}

/// X-AR5a-22: the heal is a pure function of the base and what the restored drive says of each
/// entry. The base given is not changed; what the drive agrees with or recorded the delete of
/// stays, what it lost or holds another version of is dropped and named.
#[test]
fn x_ar5a_22_rebase_is_pure_and_names_what_it_drops() {
    let base: BTreeMap<String, u32> = [
        ("same.md", 1),
        ("gone.md", 2),
        ("edited.md", 3),
        ("deleted.md", 4),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();
    let before = base.clone();
    let relation = |key: &str, _: &u32| match key {
        "gone.md" => Remote::Lost,
        "edited.md" => Remote::Other,
        "deleted.md" => Remote::Deleted,
        _ => Remote::Same,
    };
    let rebased = rebase(&base, &relation);
    assert_eq!(base, before, "the base itself is not touched");
    assert_eq!(
        rebased
            .trusted
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["deleted.md", "same.md"],
        "what the drive agrees with, and what it recorded the delete of, stays"
    );
    assert_eq!(rebased.lost, ["gone.md"]);
    assert_eq!(rebased.edited, ["edited.md"]);
    // The same input gives the same answer.
    assert_eq!(rebase(&base, &relation), rebased);
}

/// The announcement carries `kind` and `as_of` (the contract sends both); a notice an older
/// build stored without them still reads, as the lagging copy.
#[test]
fn the_notice_keeps_its_kind_and_moment_and_an_older_one_still_reads() {
    let said = RestoreNotice {
        epoch: 4,
        prefix: String::from("docs/"),
        kind: RestoreKind::PointInTime,
        as_of: Some(1_760_000_000),
    };
    let text = serde_json::to_string(&said).unwrap();
    assert_eq!(serde_json::from_str::<RestoreNotice>(&text).unwrap(), said);
    let older: RestoreNotice = serde_json::from_str(r#"{"epoch":3,"prefix":"docs/"}"#).unwrap();
    assert_eq!(older.kind, RestoreKind::Offsite);
    assert_eq!(older.as_of, None);
    assert_eq!(
        RestoreKind::from_wire("point_in_time"),
        RestoreKind::PointInTime
    );
    assert_eq!(RestoreKind::from_wire("offsite"), RestoreKind::Offsite);
    assert_eq!(
        RestoreKind::from_wire("something new"),
        RestoreKind::Offsite
    );
    assert_eq!(RestoreKind::PointInTime.wire(), "point_in_time");
}
