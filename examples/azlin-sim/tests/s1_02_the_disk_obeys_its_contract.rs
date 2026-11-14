//! S1-02: the simulated disk obeys its contract (planning/architecture.md section 1.4 rule 2: torn
//! writes, a flush that lies, sector errors, flipped bits, vanishing disks; section 6.10 V1 "saved
//! means fsynced"). Every test enters through the public API.

use azlin_sim::config::Ppm;
use azlin_sim::disk::{DiskError, SimDisk, Synced};
use azlin_sim::draw::step_rng;

fn filled(n: u32) -> SimDisk<u32> {
    let mut disk = SimDisk::new();
    for i in 0..n {
        disk.append(i).expect("room");
    }
    disk
}

#[test]
fn s1_02_an_honest_disk_keeps_the_synced_records_and_a_prefix_of_the_rest() {
    let mut torn_seen = 0;
    let mut longer_seen = 0;
    for salt in 0..300u32 {
        let mut disk = filled(6);
        assert_eq!(disk.fsync(false), Ok(Synced::Durable));
        for i in 6..10 {
            disk.append(i).expect("room");
        }
        assert_eq!((disk.len(), disk.synced_len()), (10, 6));
        let mut rng = step_rng(0xD15C, salt);
        let report = disk.crash(&mut rng, Ppm::new(500_000).expect("a rate"));
        assert!(
            report.kept >= 6 && report.kept <= 10,
            "kept {}",
            report.kept
        );
        assert_eq!(report.kept + report.lost, 10);
        assert_eq!(
            report.lost_by_lie, 0,
            "an honest disk lost nothing it had reported durable"
        );
        if report.kept > 6 {
            longer_seen += 1;
        }
        let prefix = disk.valid_prefix();
        if report.torn {
            torn_seen += 1;
            assert!(report.kept > 6, "only an unsynced record can be torn");
            assert_eq!(
                prefix.cut,
                Some(DiskError::Corrupt {
                    index: report.kept - 1
                })
            );
            assert_eq!(
                prefix.records,
                (0..report.kept as u32 - 1).collect::<Vec<u32>>()
            );
            assert_eq!(
                disk.read_all(),
                Err(DiskError::Corrupt {
                    index: report.kept - 1
                })
            );
        } else {
            assert_eq!(prefix.cut, None);
            assert_eq!(
                prefix.records,
                (0..report.kept as u32).collect::<Vec<u32>>()
            );
        }
        // The synced records are always all there and intact.
        assert!(prefix.records.starts_with(&(0..6).collect::<Vec<u32>>()));
    }
    assert!(torn_seen > 20, "a torn write was seen {torn_seen} times");
    assert!(
        longer_seen > 50,
        "the operating system flushed some {longer_seen} times"
    );
}

#[test]
fn s1_02_a_lying_fsync_says_ok_and_a_crash_loses_what_it_covered() {
    let mut lost_everything = 0;
    for salt in 0..200u32 {
        let mut disk = filled(5);
        assert_eq!(disk.fsync(true), Ok(Synced::Lied));
        assert_eq!(disk.synced_len(), 0, "the cursor did not move");
        assert_eq!(disk.len(), 5);
        let mut rng = step_rng(0x11E, salt);
        let report = disk.crash(&mut rng, Ppm::ZERO);
        assert_eq!(report.kept + report.lost, 5);
        assert_eq!(
            report.lost_by_lie, report.lost,
            "everything lost was covered by the lie"
        );
        assert!(!report.torn);
        if report.kept == 0 {
            lost_everything += 1;
        }
    }
    assert!(
        lost_everything > 10,
        "the whole covered tail was lost {lost_everything} times"
    );
    // An honest fsync after a lie makes the lied records durable too.
    let mut disk = filled(3);
    assert_eq!(disk.fsync(true), Ok(Synced::Lied));
    disk.append(3).expect("room");
    assert_eq!(disk.fsync(false), Ok(Synced::Durable));
    assert_eq!(disk.synced_len(), 4);
    let report = disk.crash(&mut step_rng(1, 1), Ppm::ZERO);
    assert_eq!((report.kept, report.lost), (4, 0));
    // An armed lie lies a given number of times, then the disk is honest again.
    let mut disk = filled(2);
    disk.arm_lie(2);
    assert_eq!(disk.fsync(false), Ok(Synced::Lied));
    assert_eq!(disk.fsync(false), Ok(Synced::Lied));
    assert_eq!(disk.fsync(false), Ok(Synced::Durable));
    assert_eq!(disk.stats().lies, 2);
}

#[test]
fn s1_02_a_read_error_a_flip_and_a_vanish_are_typed() {
    let mut disk = filled(5);
    disk.fsync(false).expect("honest");
    disk.arm_read_error();
    assert_eq!(disk.read_all(), Err(DiskError::Read));
    assert_eq!(
        disk.read_all(),
        Ok((0..5).collect::<Vec<u32>>()),
        "one read fails, the next works"
    );
    let mut seen = [false; 5];
    for salt in 0..100u32 {
        let mut disk = filled(5);
        disk.flip(&mut step_rng(5, salt));
        match disk.read_all() {
            Err(DiskError::Corrupt { index }) => {
                assert!(index < 5);
                seen[index] = true;
                let prefix = disk.valid_prefix();
                assert_eq!(
                    prefix.records.len(),
                    index,
                    "the prefix ends before the corrupt record"
                );
                assert_eq!(prefix.cut, Some(DiskError::Corrupt { index }));
            }
            other => panic!("a flipped record must be Corrupt, got {other:?}"),
        }
    }
    assert!(
        seen.iter().all(|s| *s),
        "every record can be the one that flips"
    );
    let mut disk = filled(3);
    disk.vanish();
    assert!(disk.is_gone());
    assert_eq!(disk.read_all(), Err(DiskError::Gone));
    assert_eq!(disk.append(1), Err(DiskError::Gone));
    assert_eq!(disk.fsync(false), Err(DiskError::Gone));
    let prefix = disk.valid_prefix();
    assert!(prefix.records.is_empty() && prefix.cut == Some(DiskError::Gone));
    disk.reset();
    assert!(!disk.is_gone() && disk.is_empty());
    assert_eq!(disk.append(1), Ok(()));
    // A flip on an empty disk is nothing.
    let mut empty: SimDisk<u32> = SimDisk::new();
    empty.flip(&mut step_rng(1, 1));
    assert_eq!(empty.read_all(), Ok(Vec::new()));
}

#[test]
fn s1_02_an_armed_tear_tears_the_last_unsynced_record_of_the_next_crash() {
    let mut hits = 0;
    for salt in 0..100u32 {
        let mut disk = filled(4);
        disk.fsync(false).expect("honest");
        disk.append(4).expect("room");
        disk.append(5).expect("room");
        disk.arm_tear();
        let report = disk.crash(&mut step_rng(8, salt), Ppm::ZERO);
        if report.kept > 4 {
            assert!(
                report.torn,
                "an armed tear tears the last record that was kept"
            );
            hits += 1;
        } else {
            assert!(!report.torn, "nothing unsynced was kept, nothing to tear");
        }
        // The tear is spent.
        disk.append(9).expect("room");
        let again = disk.crash(&mut step_rng(8, salt + 1000), Ppm::ZERO);
        assert!(!again.torn);
    }
    assert!(hits > 30);
}

#[test]
fn s1_02_the_bound_is_a_typed_refusal_and_a_replace_is_atomic() {
    let mut disk: SimDisk<u32> = SimDisk::with_limit(3);
    for i in 0..3 {
        disk.append(i).expect("under the bound");
    }
    assert_eq!(disk.append(3), Err(DiskError::Full));
    assert_eq!(disk.len(), 3);
    // An honest replace swaps the whole content and makes it durable at once.
    assert_eq!(disk.replace(vec![7, 8], false), Ok(Synced::Durable));
    assert_eq!(disk.read_all(), Ok(vec![7, 8]));
    assert_eq!(disk.synced_len(), 2);
    // A lying replace leaves the old content: nothing happened.
    assert_eq!(disk.replace(vec![1], true), Ok(Synced::Lied));
    assert_eq!(disk.read_all(), Ok(vec![7, 8]));
    // Too many records for the disk is a refusal, not a truncation.
    assert_eq!(disk.replace(vec![1, 2, 3, 4], false), Err(DiskError::Full));
    assert_eq!(disk.read_all(), Ok(vec![7, 8]));
}

#[test]
fn s1_02_every_outcome_depends_only_on_the_salt() {
    let run = |salt: u32| {
        let mut disk = filled(8);
        disk.fsync(false).expect("honest");
        for i in 8..14 {
            disk.append(i).expect("room");
        }
        let report = disk.crash(&mut step_rng(77, salt), Ppm::new(300_000).expect("a rate"));
        (report, disk.valid_prefix().records)
    };
    for salt in 0..50 {
        assert_eq!(run(salt), run(salt), "salt {salt}");
    }
    assert!(
        (0..50).any(|salt| run(salt) != run(salt + 1)),
        "different salts give different crashes"
    );
}
