//! D42's burst guard: a run whose changes look like a burst or like ransomware pauses uploads
//! until the user answers (`SyncOptions::allow_burst`, "these changes are mine").

use std::collections::BTreeMap;

use super::{
    fake_s3::S3Bucket,
    sync::{index_of, Device},
};
use crate::sync::{
    guard::{
        check_burst, entropy, Guard, PauseReason, BURST_CHANGES, BURST_WINDOW_SECS,
        ENTROPY_REWRITES, HIGH_ENTROPY, LOW_ENTROPY,
    },
    local::hash_bytes,
    Action, BaseEntry, LocalIndex,
};

/// `len` bytes that look encrypted (BLAKE3's output stream of `seed`).
fn random_bytes(seed: &str, len: usize) -> Vec<u8> {
    let mut out = vec![0u8; len];
    blake3::Hasher::new()
        .update(seed.as_bytes())
        .finalize_xof()
        .fill(&mut out);
    out
}

/// `len` bytes of prose.
fn prose(seed: &str, len: usize) -> Vec<u8> {
    let line = format!("{seed}: the quick brown fox jumps over the lazy dog.\n");
    line.repeat(len / line.len() + 1).into_bytes()[..len].to_vec()
}

#[test]
fn prose_is_low_entropy_and_encrypted_bytes_high() {
    assert!(entropy(&prose("a letter", 4096)) < LOW_ENTROPY);
    assert!(entropy(&random_bytes("a", 4096)) > HIGH_ENTROPY);
    assert!(entropy(&random_bytes("b", 1024)) > HIGH_ENTROPY);
}

#[test]
fn more_than_200_changes_in_five_minutes_pause_uploads_until_the_user_answers() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    let n = BURST_CHANGES + 1;
    for i in 0..n {
        a.write(&format!("f{i}.txt"), format!("{i}").as_bytes());
    }
    let first = a.sync(&store);
    assert!(
        first.paused.is_none(),
        "new files are no burst: {}",
        first.summary()
    );
    for i in 0..n {
        a.write(&format!("f{i}.txt"), format!("{i}, edited").as_bytes());
    }
    let report = a.sync(&store);
    let pause = report.paused.clone().expect("a burst pauses uploads");
    assert_eq!((pause.reason, pause.changes), (PauseReason::Burst, n));
    assert_eq!(report.files_up, 0, "{}", report.summary());
    assert!(report.summary().contains("paused"), "{}", report.summary());
    assert_eq!(
        index_of(&store).files["f0.txt"].hash,
        hash_bytes(b"0"),
        "the drive keeps what it had"
    );
    // Paused until the user answers, also on the next run; the pause is in the folder's index.
    assert_eq!(
        a.sync(&store).paused.map(|p| p.reason),
        Some(PauseReason::Burst)
    );
    let saved = LocalIndex::load(&a.index()).unwrap().expect("an index");
    assert!(saved.guard.paused.is_some());
    // "These changes are mine": one run with allow_burst sends them.
    let mut opts = a.opts();
    opts.allow_burst = true;
    let report = a.sync_with(&store, &opts).unwrap();
    assert!(report.paused.is_none(), "{}", report.summary());
    assert_eq!(report.files_up, n);
    let saved = LocalIndex::load(&a.index()).unwrap().expect("an index");
    assert!(saved.guard.paused.is_none());
    // The window starts afresh.
    a.write("f0.txt", b"0, again");
    let report = a.sync(&store);
    assert!(report.paused.is_none(), "{}", report.summary());
    assert_eq!(report.files_up, 1);
}

#[test]
fn the_changes_of_several_runs_within_five_minutes_add_up() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    for i in 0..250 {
        a.write(&format!("f{i}.txt"), format!("{i}").as_bytes());
    }
    a.sync(&store);
    for i in 0..150 {
        a.write(&format!("f{i}.txt"), format!("{i}, edited").as_bytes());
    }
    let report = a.sync(&store);
    assert!(report.paused.is_none(), "{}", report.summary());
    assert_eq!(report.files_up, 150);
    for i in 150..210 {
        a.write(&format!("f{i}.txt"), format!("{i}, edited").as_bytes());
    }
    let report = a.sync(&store);
    let pause = report
        .paused
        .expect("150 and 60 changes within five minutes");
    assert_eq!((pause.reason, pause.changes), (PauseReason::Burst, 210));
}

#[test]
fn changes_older_than_five_minutes_no_longer_count() {
    let now = 1_800_000_000;
    let base: BTreeMap<String, BaseEntry> = (0..60)
        .map(|i| {
            let entry = BaseEntry {
                hash: String::new(),
                size: 1,
                mtime_ns: 0,
                cloud_only: false,
            };
            (format!("f{i}.txt"), entry)
        })
        .collect();
    let actions: Vec<Action> = base
        .keys()
        .map(|key| Action::Upload { key: key.clone() })
        .collect();
    // No file is known to be low-entropy: nothing is read from the folder.
    let root = std::env::temp_dir();
    let mut old = Guard {
        recent: vec![(now - BURST_WINDOW_SECS - 1, 150, 0)],
        ..Guard::default()
    };
    assert_eq!(
        check_burst(&mut old, &actions, &base, &root, now, false),
        None
    );
    let mut fresh = Guard {
        recent: vec![(now - 60, 150, 0)],
        ..Guard::default()
    };
    let pause = check_burst(&mut fresh, &actions, &base, &root, now, false)
        .expect("150 and 60 changes within five minutes");
    assert_eq!((pause.reason, pause.changes), (PauseReason::Burst, 210));
    assert_eq!(fresh.paused, Some(pause));
}

#[test]
fn text_files_rewritten_into_random_bytes_pause_uploads_as_ransomware() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    let b = Device::new("dev-b");
    for i in 0..30 {
        a.write(
            &format!("docs/{i}.txt"),
            &prose(&format!("letter {i}"), 4096),
        );
    }
    a.sync(&store);
    b.sync(&store);
    for i in 0..ENTROPY_REWRITES {
        a.write(
            &format!("docs/{i}.txt"),
            &random_bytes(&format!("{i}"), 4096),
        );
    }
    b.write("docs/from-b.txt", b"made on b");
    b.sync(&store);
    let report = a.sync(&store);
    let pause = report
        .paused
        .clone()
        .expect("encrypted rewrites pause uploads");
    assert_eq!(
        (pause.reason, pause.changes),
        (PauseReason::Encryption, ENTROPY_REWRITES)
    );
    assert!(pause.files.iter().any(|f| f == "docs/0.txt"), "{pause:?}");
    assert_eq!(report.files_up, 0, "{}", report.summary());
    assert_eq!(
        index_of(&store).files["docs/0.txt"].hash,
        hash_bytes(&prose("letter 0", 4096)),
        "the drive keeps the letter"
    );
    // Only uploads pause: what the drive got from b still comes here.
    assert_eq!(report.files_down, 1, "{}", report.summary());
    assert_eq!(
        a.read("docs/from-b.txt").as_deref(),
        Some(&b"made on b"[..])
    );
}

#[test]
fn new_or_rewritten_photos_do_not_pause() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    for i in 0..40 {
        a.write(
            &format!("photos/{i}.jpg"),
            &random_bytes(&format!("photo {i}"), 4096),
        );
    }
    let report = a.sync(&store);
    assert!(report.paused.is_none(), "{}", report.summary());
    for i in 0..40 {
        a.write(
            &format!("photos/{i}.jpg"),
            &random_bytes(&format!("edited {i}"), 4096),
        );
    }
    let report = a.sync(&store);
    assert!(report.paused.is_none(), "{}", report.summary());
    assert_eq!(report.files_up, 40);
}
