//! A device's copy of the drive index on disk, and the lazy copy (C6).

use std::{collections::BTreeMap, path::Path};

use super::TempDir;
use crate::meta::{
    keys, merge::keep_both, repo::RepoOptions, tree::walk, Bucket, Change, MemoryBucket,
    MetaError, MetaRepo, Mode, TestSealer,
};

type Repo = MetaRepo<MemoryBucket, TestSealer>;

const KEY: [u8; 32] = [31; 32];

fn sealer() -> TestSealer {
    TestSealer::new(KEY)
}

fn cached(dir: &TempDir) -> RepoOptions {
    RepoOptions {
        cache_dir: Some(dir.path().to_path_buf()),
        lazy: false,
    }
}

fn put(repo: &mut Repo, path: &str, content: &str) -> Change {
    Change::Put {
        path: path.to_string(),
        id: repo.write_blob(content.as_bytes()),
    }
}

fn commit(repo: &mut Repo, files: &[(&str, &str)]) {
    let changes: Vec<Change> = files.iter().map(|(p, c)| put(repo, p, c)).collect();
    repo.commit(&changes, "edit", &mut keep_both).unwrap();
}

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

/// Every file under `dir`, with its bytes.
fn files_on_disk(dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files_on_disk(&path, out);
        } else {
            out.push((path.display().to_string(), std::fs::read(&path).unwrap()));
        }
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn a_device_reopened_from_its_cache_polls_with_one_conditional_read_and_downloads_nothing() {
    let bucket = MemoryBucket::new();
    let dir = TempDir::new("meta-cache-reopen");
    let mut laptop =
        Repo::create_with(bucket.clone(), sealer(), "dev-laptop", "Laptop", &cached(&dir)).unwrap();
    commit(&mut laptop, &[("a.txt", "a1"), ("docs/b.txt", "b1")]);
    let before_files = files(&laptop);
    drop(laptop);

    let before = bucket.counts();
    let laptop =
        Repo::open_with(bucket.clone(), sealer(), "dev-laptop", "Laptop", &cached(&dir)).unwrap();
    let after = bucket.counts();
    assert_eq!(files(&laptop), before_files);
    assert_eq!(after.conditional_reads - before.conditional_reads, 1);
    assert_eq!(after.not_modified - before.not_modified, 1);
    assert_eq!(after.reads, before.reads);
    assert_eq!(after.range_reads, before.range_reads);
}

#[test]
fn a_device_reopened_from_its_cache_downloads_only_what_is_new() {
    let bucket = MemoryBucket::new();
    let dir = TempDir::new("meta-cache-new");
    let mut laptop =
        Repo::create_with(bucket.clone(), sealer(), "dev-laptop", "Laptop", &cached(&dir)).unwrap();
    commit(&mut laptop, &[("a.txt", "a1")]);
    drop(laptop);
    let mut phone = Repo::open(bucket.clone(), sealer(), "dev-phone", "Phone").unwrap();
    commit(&mut phone, &[("b.txt", "from the phone")]);

    let before = bucket.counts();
    let laptop =
        Repo::open_with(bucket.clone(), sealer(), "dev-laptop", "Laptop", &cached(&dir)).unwrap();
    let after = bucket.counts();
    assert_eq!(files(&laptop), files(&phone));
    assert_eq!(after.conditional_reads - before.conditional_reads, 1);
    // The phone's log entry, the index and the pack of its one pack.
    assert_eq!(after.reads - before.reads, 3);
}

#[test]
fn the_cache_on_disk_holds_no_name_and_no_content_in_plaintext() {
    let bucket = MemoryBucket::new();
    let dir = TempDir::new("meta-cache-plain");
    let mut laptop =
        Repo::create_with(bucket, sealer(), "dev-laptop", "Felix laptop", &cached(&dir)).unwrap();
    commit(&mut laptop, &[("Holiday photos/beach.jpg", "object data/ab/0123 size 4711")]);
    let mut on_disk = Vec::new();
    files_on_disk(dir.path(), &mut on_disk);
    assert!(on_disk.len() >= 3, "{on_disk:?}");
    let head = laptop.head().unwrap().to_hex();
    for (path, bytes) in &on_disk {
        assert!(!path.contains("Holiday"), "{path}");
        for secret in [&b"Holiday"[..], b"beach", b"Felix", b"size 4711", b"refs/heads", head.as_bytes()] {
            assert!(!contains(bytes, secret), "{path} holds {secret:?}");
        }
    }
}

#[test]
fn a_damaged_cache_is_ignored_and_the_copy_is_read_anew() {
    let bucket = MemoryBucket::new();
    let dir = TempDir::new("meta-cache-damaged");
    let mut laptop =
        Repo::create_with(bucket.clone(), sealer(), "dev-laptop", "Laptop", &cached(&dir)).unwrap();
    commit(&mut laptop, &[("a.txt", "a1")]);
    let expected = files(&laptop);
    drop(laptop);
    let mut on_disk = Vec::new();
    files_on_disk(dir.path(), &mut on_disk);
    for (path, bytes) in on_disk {
        let mut damaged = bytes.clone();
        for byte in damaged.iter_mut().skip(8).step_by(7) {
            *byte ^= 0x55;
        }
        std::fs::write(&path, damaged).unwrap();
    }
    let laptop =
        Repo::open_with(bucket.clone(), sealer(), "dev-laptop", "Laptop", &cached(&dir)).unwrap();
    assert_eq!(files(&laptop), expected);
    // A cache of another drive key is no cache either.
    let other = MetaRepo::open_with(
        bucket,
        TestSealer::new([32; 32]),
        "dev-laptop",
        "Laptop",
        &cached(&dir),
    );
    assert!(matches!(other, Err(MetaError::Sealed { .. })));
}

#[test]
fn an_older_manifest_is_refused_after_a_restart_too() {
    let bucket = MemoryBucket::new();
    let dir = TempDir::new("meta-cache-rollback");
    let mut laptop =
        Repo::create_with(bucket.clone(), sealer(), "dev-laptop", "Laptop", &cached(&dir)).unwrap();
    commit(&mut laptop, &[("a.txt", "a1")]);
    let old_manifest = bucket.read(keys::MANIFEST).unwrap().unwrap().0;
    commit(&mut laptop, &[("a.txt", "a2")]);
    drop(laptop);
    bucket.overwrite(keys::MANIFEST, &old_manifest);
    let reopened = Repo::open_with(bucket, sealer(), "dev-laptop", "Laptop", &cached(&dir));
    assert!(matches!(
        reopened,
        Err(MetaError::Rollback { seen: 3, served: 2 })
    ));
}

/// A drive of 2000 pointer-sized files in `photos/` (about seven pack chunks) and two in
/// `docs/`.
fn big_drive(bucket: &MemoryBucket) -> Repo {
    let mut laptop = Repo::create(bucket.clone(), sealer(), "dev-laptop", "Laptop").unwrap();
    let mut changes = Vec::new();
    for i in 0..2000 {
        let pointer = format!("object data/{i:04}/{}\nsize {i}\n", "0123456789abcdef".repeat(12));
        changes.push(put(&mut laptop, &format!("photos/{i:04}.jpg"), &pointer));
    }
    changes.push(put(&mut laptop, "docs/a.txt", "the a pointer"));
    changes.push(put(&mut laptop, "docs/b.txt", "the b pointer"));
    laptop.commit(&changes, "import", &mut keep_both).unwrap();
    laptop
}

#[test]
fn a_lazy_device_browses_a_folder_with_ranged_reads_and_never_the_whole_pack() {
    let bucket = MemoryBucket::new();
    let laptop = big_drive(&bucket);
    let pack = laptop.store().state().packs[0].clone();

    let before = bucket.counts();
    let lazy = RepoOptions {
        cache_dir: None,
        lazy: true,
    };
    let mut tablet = Repo::open_with(bucket.clone(), sealer(), "dev-tablet", "Tablet", &lazy).unwrap();
    let docs = tablet.ensure_folder("docs", true).unwrap().unwrap();
    let after = bucket.counts();
    // The manifest, the one log entry, the one index: no whole pack.
    assert_eq!(after.reads - before.reads, 3);
    assert!(after.range_reads > before.range_reads);
    assert!(tablet.objects().len() < laptop.objects().len());
    let folder = crate::meta::shard::read_folder(tablet.objects(), &docs).unwrap();
    assert_eq!(folder.entries().len(), 2);
    let a = folder.get("a.txt").unwrap();
    assert_eq!(tablet.objects().blob(&a.id).unwrap(), b"the a pointer");
    assert!(after.range_reads - before.range_reads < u64::from(pack.objects));
    assert!(tablet.ensure_folder("nothing/here", true).unwrap().is_none());

    // Everything, when it is needed whole.
    tablet.fetch_all().unwrap();
    assert_eq!(files(&tablet), files(&laptop));
}

#[test]
fn a_lazy_device_commits_and_the_others_see_it() {
    let bucket = MemoryBucket::new();
    let mut laptop = big_drive(&bucket);
    let lazy = RepoOptions {
        cache_dir: None,
        lazy: true,
    };
    let mut tablet = Repo::open_with(bucket.clone(), sealer(), "dev-tablet", "Tablet", &lazy).unwrap();
    commit(&mut tablet, &[("docs/c.txt", "from the tablet")]);
    laptop.pull().unwrap();
    assert_eq!(files(&laptop), files(&tablet));
    assert_eq!(files(&laptop).len(), 2003);
}

#[test]
fn a_lazy_device_reopened_from_its_cache_reads_no_chunk_twice() {
    let bucket = MemoryBucket::new();
    big_drive(&bucket);
    let dir = TempDir::new("meta-cache-lazy");
    let options = RepoOptions {
        cache_dir: Some(dir.path().to_path_buf()),
        lazy: true,
    };
    let mut tablet =
        Repo::open_with(bucket.clone(), sealer(), "dev-tablet", "Tablet", &options).unwrap();
    tablet.ensure_folder("docs", true).unwrap();
    drop(tablet);
    let before = bucket.counts();
    let mut tablet =
        Repo::open_with(bucket.clone(), sealer(), "dev-tablet", "Tablet", &options).unwrap();
    tablet.ensure_folder("docs", true).unwrap();
    let after = bucket.counts();
    assert_eq!(after.range_reads, before.range_reads);
    assert_eq!(after.reads, before.reads);
    assert_eq!(after.not_modified - before.not_modified, 1);
}
