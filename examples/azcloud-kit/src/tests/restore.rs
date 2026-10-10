//! "Restore the drive as of <time>" (D42) over an encrypted drive's metadata repository: the
//! whole drive back as it was, as one more commit, its policy and keys left as they are.

use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use azul_storage::meta::{
    merge::keep_both, tree::walk, Change, MemoryBucket, MetaRepo, Mode, TestSealer,
};

use crate::restore::{restore_drive_as_of, state_at};

type Repo = MetaRepo<MemoryBucket, TestSealer>;

/// When the drive was made.
const T0: u64 = 1_791_000_000;

fn sealer() -> TestSealer {
    TestSealer::new([33; 32])
}

/// A device's copy of the drive in `bucket` (`create`: the first), on the shared `clock`.
fn device(bucket: &MemoryBucket, clock: &Arc<AtomicU64>, name: &str, create: bool) -> Repo {
    let repo = if create {
        Repo::create(bucket.clone(), sealer(), name, name)
    } else {
        Repo::open(bucket.clone(), sealer(), name, name)
    };
    let clock = Arc::clone(clock);
    repo.unwrap()
        .with_clock(move || clock.load(Ordering::SeqCst))
}

fn put(repo: &mut Repo, path: &str, content: &str) -> Change {
    Change::Put {
        path: path.to_string(),
        id: repo.write_blob(content.as_bytes()),
    }
}

/// Every file of the drive with its content.
fn files(repo: &Repo) -> BTreeMap<String, String> {
    let Some(root) = repo.root().unwrap() else {
        return BTreeMap::new();
    };
    walk(repo.objects(), &root)
        .unwrap()
        .into_iter()
        .filter(|(_, (mode, _))| *mode == Mode::File)
        .map(|(path, (_, id))| {
            let text = String::from_utf8(repo.objects().blob(&id).unwrap().to_vec()).unwrap();
            (path, text)
        })
        .collect()
}

fn expect(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(path, text)| ((*path).to_string(), (*text).to_string()))
        .collect()
}

/// A drive made at T0, then at T0 + 100 rewritten (a ransomware run): one file encrypted, one
/// deleted, one note left, the policy changed (a member removed).
fn drive_and_attack(bucket: &MemoryBucket, clock: &Arc<AtomicU64>) -> Repo {
    clock.store(T0, Ordering::SeqCst);
    let mut laptop = device(bucket, clock, "laptop", true);
    let changes = vec![
        put(&mut laptop, "a.txt", "a1"),
        put(&mut laptop, "docs/b.txt", "b1"),
        put(&mut laptop, ".azlin/policy.toml", "members = 2"),
    ];
    laptop.commit(&changes, "init", &mut keep_both).unwrap();
    clock.store(T0 + 100, Ordering::SeqCst);
    let changes = vec![
        put(&mut laptop, "a.txt", "encrypted"),
        Change::Delete {
            path: String::from("docs/b.txt"),
        },
        put(&mut laptop, "READ-ME-TO-DECRYPT.txt", "pay"),
        put(&mut laptop, ".azlin/policy.toml", "members = 1"),
    ];
    laptop.commit(&changes, "attack", &mut keep_both).unwrap();
    laptop
}

#[test]
fn the_whole_drive_comes_back_as_it_was_as_one_more_commit() {
    let bucket = MemoryBucket::new();
    let clock = Arc::new(AtomicU64::new(T0));
    let mut laptop = drive_and_attack(&bucket, &clock);
    clock.store(T0 + 200, Ordering::SeqCst);
    let as_of = (T0 + 50) as i64;
    let restore = restore_drive_as_of(&mut laptop, as_of, &mut keep_both)
        .unwrap()
        .expect("the drive had a state then");
    assert_eq!((restore.restored, restore.removed), (2, 1));
    assert!(restore.outcome.is_some(), "a commit");
    assert_eq!(
        files(&laptop),
        expect(&[
            (".azlin/policy.toml", "members = 1"),
            ("a.txt", "a1"),
            ("docs/b.txt", "b1"),
        ]),
        "the files as they were; the policy and keys as they are"
    );
    let history = laptop.history().unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(
        history[0].1.message,
        format!("Restore the drive as of {}", crate::rfc3339(as_of))
    );
    // Another device sees the restored drive.
    let mut phone = device(&bucket, &clock, "phone", false);
    phone.pull().unwrap();
    assert_eq!(files(&phone), files(&laptop));
    // Nothing is lost: the drive as it was before the restore comes back the same way.
    clock.store(T0 + 300, Ordering::SeqCst);
    restore_drive_as_of(&mut phone, (T0 + 150) as i64, &mut keep_both)
        .unwrap()
        .expect("a state then");
    assert_eq!(files(&phone)["a.txt"], "encrypted");
    assert_eq!(files(&phone)["READ-ME-TO-DECRYPT.txt"], "pay");
}

#[test]
fn a_drive_already_as_it_was_gets_no_commit_and_a_time_before_it_was_made_restores_nothing() {
    let bucket = MemoryBucket::new();
    let clock = Arc::new(AtomicU64::new(T0));
    let mut laptop = drive_and_attack(&bucket, &clock);
    let before = laptop.history().unwrap().len();
    let restore = restore_drive_as_of(&mut laptop, (T0 + 150) as i64, &mut keep_both)
        .unwrap()
        .expect("a state then");
    assert_eq!((restore.restored, restore.removed), (0, 0));
    assert!(restore.outcome.is_none());
    assert!(
        restore_drive_as_of(&mut laptop, (T0 - 1) as i64, &mut keep_both)
            .unwrap()
            .is_none()
    );
    assert_eq!(laptop.history().unwrap().len(), before);
}

#[test]
fn the_state_at_a_time_is_the_newest_commit_made_then_or_before() {
    let bucket = MemoryBucket::new();
    let clock = Arc::new(AtomicU64::new(T0));
    let laptop = drive_and_attack(&bucket, &clock);
    let history = laptop.history().unwrap();
    let (attack, init) = (history[0].0, history[1].0);
    assert_eq!(state_at(&history, T0 as i64), Some(init));
    assert_eq!(state_at(&history, (T0 + 99) as i64), Some(init));
    assert_eq!(state_at(&history, (T0 + 100) as i64), Some(attack));
    assert_eq!(state_at(&history, (T0 - 1) as i64), None);
}
