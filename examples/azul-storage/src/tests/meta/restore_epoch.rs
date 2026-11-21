//! A restore of the drive, as a device's copy of the drive index meets it (ARCH28 RF45; AZ-M4,
//! X-AR5a-24): the token server announces the restore, the device takes it once and reads the
//! restored drive anew; a bucket that serves an older state with no announcement is refused; and
//! what the device believes published is what the live packs hold.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use super::TempDir;
use crate::{
    meta::{
        keys, merge::keep_both, tree::walk, Bucket, Change, Manifest, MemoryBucket, MetaError,
        MetaRepo, Mode, RepoOptions, RestoreSource, Sealer, TestSealer,
    },
    restore::RestoreNotice,
};

type Repo = MetaRepo<MemoryBucket, TestSealer>;

fn sealer() -> TestSealer {
    TestSealer::new([61; 32])
}

/// What the token server last announced for the drive; the test changes it.
#[derive(Clone, Default)]
struct Announced(Arc<Mutex<Option<RestoreNotice>>>);

impl Announced {
    /// The token server announces a restore of the whole drive, epoch `epoch`.
    fn set(&self, epoch: u64) {
        *self.0.lock().unwrap() = Some(RestoreNotice {
            epoch,
            ..RestoreNotice::default()
        });
    }

    fn source(&self) -> RestoreSource {
        let cell = Arc::clone(&self.0);
        RestoreSource::new(move || cell.lock().unwrap().clone())
    }
}

fn options(announced: &Announced) -> RepoOptions {
    RepoOptions {
        restore: announced.source(),
        ..RepoOptions::default()
    }
}

fn put(repo: &mut Repo, path: &str, content: &str) -> Change {
    Change::Put {
        path: path.to_string(),
        id: repo.write_blob(content.as_bytes()),
    }
}

/// Every file of the drive with its content, as the device's copy shows it.
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

/// The manifest as the bucket holds it now.
fn manifest_bytes(bucket: &MemoryBucket) -> Vec<u8> {
    bucket
        .objects()
        .into_iter()
        .find(|(key, _)| key == keys::MANIFEST)
        .map(|(_, bytes)| bytes)
        .unwrap()
}

/// A drive with `a.txt`, its manifest as it was then (what an off-site copy made at that moment
/// holds), and `b.txt` added after it.
fn drive_and_its_older_manifest(bucket: &MemoryBucket, options: &RepoOptions) -> (Repo, Vec<u8>) {
    let mut laptop =
        Repo::create_with(bucket.clone(), sealer(), "dev-laptop", "Laptop", options).unwrap();
    let change = vec![put(&mut laptop, "a.txt", "a1")];
    laptop.commit(&change, "one", &mut keep_both).unwrap();
    let older = manifest_bytes(bucket);
    let change = vec![put(&mut laptop, "b.txt", "b1")];
    laptop.commit(&change, "two", &mut keep_both).unwrap();
    (laptop, older)
}

#[test]
fn az_m4_a_restore_the_token_server_announced_is_taken_once_and_a_rollback_nobody_announced_is_refused(
) {
    let bucket = MemoryBucket::new();
    let announced = Announced::default();
    let (mut laptop, older) = drive_and_its_older_manifest(&bucket, &options(&announced));
    assert_eq!(files(&laptop), expect(&[("a.txt", "a1"), ("b.txt", "b1")]));
    // The operator restores the bucket from the off-site copy that lags: the older manifest.
    bucket.overwrite(keys::MANIFEST, &older);

    // Nothing was announced: refused, for good - and the copy is as it was.
    let refused = laptop.pull().unwrap_err();
    assert!(matches!(refused, MetaError::Rollback { .. }), "{refused:?}");
    assert_eq!(files(&laptop), expect(&[("a.txt", "a1"), ("b.txt", "b1")]));
    assert_eq!(laptop.restore_epoch(), 0);

    // The token server announces the restore: the device takes it, once, and reads the restored
    // drive anew - b.txt was after the off-site copy and is gone with it.
    announced.set(1);
    assert!(laptop.pull().unwrap(), "the restore is a change");
    assert_eq!(laptop.restore_epoch(), 1);
    assert_eq!(files(&laptop), expect(&[("a.txt", "a1")]));
    assert!(
        !laptop.pull().unwrap(),
        "the same announcement is taken once"
    );
    assert_eq!(laptop.restore_epoch(), 1);

    // A commit on the restored drive, and then a replay of the older state with no NEW
    // announcement (the old one excuses nothing): a rollback again.
    let change = vec![put(&mut laptop, "c.txt", "c1")];
    laptop.commit(&change, "three", &mut keep_both).unwrap();
    bucket.overwrite(keys::MANIFEST, &older);
    let replayed = laptop.pull().unwrap_err();
    assert!(
        matches!(replayed, MetaError::Rollback { .. }),
        "{replayed:?}"
    );
}

#[test]
fn az_m4_another_device_that_committed_on_the_restored_drive_first_does_not_hide_the_restore() {
    let bucket = MemoryBucket::new();
    let announced = Announced::default();
    let (mut laptop, older) = drive_and_its_older_manifest(&bucket, &options(&announced));
    bucket.overwrite(keys::MANIFEST, &older);
    announced.set(1);

    // The phone is the first to meet the restored drive and commits on top of it, twice: the
    // bucket now serves a revision ABOVE what the laptop saw, so no rollback shows - the epoch
    // is what says the laptop's copy is of a drive that no longer is.
    let mut phone = Repo::open_with(
        bucket.clone(),
        sealer(),
        "dev-phone",
        "Phone",
        &options(&announced),
    )
    .unwrap();
    for (path, text) in [("p1.txt", "p1"), ("p2.txt", "p2")] {
        let change = vec![put(&mut phone, path, text)];
        phone.commit(&change, "phone", &mut keep_both).unwrap();
    }
    assert_eq!(
        files(&phone),
        expect(&[("a.txt", "a1"), ("p1.txt", "p1"), ("p2.txt", "p2")])
    );

    assert!(laptop.pull().unwrap());
    assert_eq!(laptop.restore_epoch(), 1);
    assert_eq!(
        files(&laptop),
        files(&phone),
        "the laptop's lost b.txt is not in its copy"
    );
}

#[test]
fn az_m4_the_epoch_a_device_took_survives_a_restart_and_a_fresh_device_starts_at_the_current_one() {
    let bucket = MemoryBucket::new();
    let announced = Announced::default();
    let dir = TempDir::new("restore-epoch");
    let cached = RepoOptions {
        cache_dir: Some(dir.path().to_path_buf()),
        ..options(&announced)
    };
    let (mut laptop, older) = drive_and_its_older_manifest(&bucket, &cached);
    bucket.overwrite(keys::MANIFEST, &older);
    announced.set(1);
    assert!(laptop.pull().unwrap());
    drop(laptop);

    // The app starts again with the announcement still the token server's newest: no second drop
    // (the pack files the first read cached are still there), the epoch is remembered.
    let mut again =
        Repo::open_with(bucket.clone(), sealer(), "dev-laptop", "Laptop", &cached).unwrap();
    assert_eq!(again.restore_epoch(), 1);
    assert!(!again.pull().unwrap());
    assert_eq!(files(&again), expect(&[("a.txt", "a1")]));

    // A device new to the drive, born after the restore, starts at the epoch the drive has: the
    // announcement that was made before it existed never excuses a rollback after it.
    let mut newcomer = Repo::open_with(
        bucket.clone(),
        sealer(),
        "dev-tablet",
        "Tablet",
        &options(&announced),
    )
    .unwrap();
    assert_eq!(newcomer.restore_epoch(), 1);
    let change = vec![put(&mut newcomer, "t.txt", "t1")];
    newcomer.commit(&change, "tablet", &mut keep_both).unwrap();
    let after = manifest_bytes(&bucket);
    let change = vec![put(&mut newcomer, "t2.txt", "t2")];
    newcomer
        .commit(&change, "tablet again", &mut keep_both)
        .unwrap();
    bucket.overwrite(keys::MANIFEST, &after);
    let refused = newcomer.pull().unwrap_err();
    assert!(matches!(refused, MetaError::Rollback { .. }), "{refused:?}");
}

/// The manifest the bucket serves with its `which`-th pack dropped and the revision one above:
/// the pack's objects are lost to every device that reads the manifest from now on (an operator
/// restored the bucket without its newest data, and the devices went on).
fn lose_pack(bucket: &MemoryBucket, which: usize) {
    let sealer = sealer();
    let (bytes, version) = bucket.read(keys::MANIFEST).unwrap().unwrap();
    let plain = sealer.open(keys::MANIFEST.as_bytes(), &bytes).unwrap();
    let mut manifest: Manifest = serde_json::from_slice(&plain).unwrap();
    let lost = manifest.packs.remove(which);
    manifest.revision += 1;
    let sealed = sealer
        .seal(
            keys::MANIFEST.as_bytes(),
            &serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
    bucket.replace(keys::MANIFEST, &sealed, &version).unwrap();
    bucket.remove(&keys::pack(&lost.name)).unwrap();
    bucket.remove(&keys::idx(&lost.name)).unwrap();
}

#[test]
fn x_ar5a_24_what_a_device_believes_published_is_what_the_live_packs_hold() {
    let bucket = MemoryBucket::new();
    let announced = Announced::default();
    let (mut laptop, _) = drive_and_its_older_manifest(&bucket, &options(&announced));
    let a_blob = laptop.write_blob(b"a1");
    let b_blob = laptop.write_blob(b"b1");
    assert!(laptop.holds_published(&a_blob) && laptop.holds_published(&b_blob));

    // The second pack (b.txt's) leaves the bucket and the manifest.
    lose_pack(&bucket, 1);
    laptop.pull().unwrap();
    assert!(
        laptop.holds_published(&a_blob),
        "the first pack is live still"
    );
    assert!(
        !laptop.holds_published(&b_blob),
        "no live pack holds b.txt's blob any more: the device no longer believes it is published"
    );

    // The next commit packs b.txt's objects again, so a device that reads the drive afresh finds
    // every object the head reaches (it used to name a blob no pack held).
    let change = vec![put(&mut laptop, "d.txt", "d1")];
    laptop
        .commit(&change, "after the loss", &mut keep_both)
        .unwrap();
    let phone = Repo::open(bucket.clone(), sealer(), "dev-phone", "Phone").unwrap();
    assert_eq!(
        files(&phone),
        expect(&[("a.txt", "a1"), ("b.txt", "b1"), ("d.txt", "d1")])
    );
}
