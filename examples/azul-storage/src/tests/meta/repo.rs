use std::{collections::BTreeMap, sync::mpsc};

use crate::meta::{
    keys,
    merge::keep_both,
    tree::walk,
    wal::MAINTENANCE,
    Change, ConflictKind, MemoryBucket, MetaRepo, Mode, Resolution, TestSealer,
};

type Repo = MetaRepo<MemoryBucket, TestSealer>;

const KEY: [u8; 32] = [21; 32];

fn sealer() -> TestSealer {
    TestSealer::new(KEY)
}

fn put(repo: &mut Repo, path: &str, content: &str) -> Change {
    Change::Put {
        path: path.to_string(),
        id: repo.write_blob(content.as_bytes()),
    }
}

/// The drive as the device sees it: every file with its content.
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
        .map(|(p, c)| ((*p).to_string(), (*c).to_string()))
        .collect()
}

/// A drive with two files, made on the laptop, and the phone that opened it.
fn laptop_and_phone(bucket: &MemoryBucket) -> (Repo, Repo) {
    let mut laptop = Repo::create(bucket.clone(), sealer(), "dev-laptop", "Laptop").unwrap();
    let changes = vec![
        put(&mut laptop, "a.txt", "a1"),
        put(&mut laptop, "docs/b.txt", "b1"),
    ];
    laptop.commit(&changes, "init", &mut keep_both).unwrap();
    let phone = Repo::open(bucket.clone(), sealer(), "dev-phone", "Phone").unwrap();
    assert_eq!(files(&phone), files(&laptop));
    (laptop, phone)
}

#[test]
fn two_devices_converge_after_editing_different_files() {
    let bucket = MemoryBucket::new();
    let (mut laptop, mut phone) = laptop_and_phone(&bucket);
    let change = put(&mut laptop, "a.txt", "a2");
    laptop.commit(&[change], "laptop edit", &mut keep_both).unwrap();
    let change = put(&mut phone, "docs/c.txt", "c1");
    let outcome = phone.commit(&[change], "phone edit", &mut keep_both).unwrap();
    assert!(outcome.conflicts.is_empty());
    // The phone merged the laptop's commit in.
    assert_eq!(phone.history().unwrap()[0].1.parents.len(), 2);

    assert!(laptop.pull().unwrap());
    let both = expect(&[("a.txt", "a2"), ("docs/b.txt", "b1"), ("docs/c.txt", "c1")]);
    assert_eq!(files(&laptop), both);
    assert_eq!(files(&phone), both);
    assert_eq!(laptop.head(), phone.head());
    assert_eq!(bucket.counts().lists, 0);
}

#[test]
fn a_device_that_loses_the_swap_merges_and_lands_on_its_second_attempt() {
    let bucket = MemoryBucket::new();
    let (mut laptop, phone) = laptop_and_phone(&bucket);
    let (send_back, phone_back) = mpsc::channel();
    bucket.before_next_replace(keys::MANIFEST, move || {
        let mut phone = phone;
        let change = put(&mut phone, "docs/b.txt", "b-phone");
        phone.commit(&[change], "phone edit", &mut keep_both).unwrap();
        send_back.send(phone).unwrap();
    });
    let change = put(&mut laptop, "a.txt", "a-laptop");
    let outcome = laptop.commit(&[change], "laptop edit", &mut keep_both).unwrap();
    let mut phone = phone_back.recv().unwrap();

    assert_eq!(outcome.published.attempts, 2);
    assert!(outcome.conflicts.is_empty());
    phone.pull().unwrap();
    let both = expect(&[("a.txt", "a-laptop"), ("docs/b.txt", "b-phone")]);
    assert_eq!(files(&laptop), both);
    assert_eq!(files(&phone), both);
    assert_eq!(laptop.head(), phone.head());
}

#[test]
fn both_editing_one_file_keeps_both_versions_on_every_device() {
    let bucket = MemoryBucket::new();
    let (mut laptop, mut phone) = laptop_and_phone(&bucket);
    let change = put(&mut laptop, "docs/b.txt", "laptop's");
    laptop.commit(&[change], "laptop edit", &mut keep_both).unwrap();
    let change = put(&mut phone, "docs/b.txt", "phone's");
    let outcome = phone.commit(&[change], "phone edit", &mut keep_both).unwrap();

    assert_eq!(outcome.conflicts.len(), 1);
    let resolved = &outcome.conflicts[0];
    assert_eq!(resolved.conflict.kind, ConflictKind::BothChanged);
    assert_eq!(resolved.copy.as_deref(), Some("docs/b (conflict, Phone).txt"));
    laptop.pull().unwrap();
    let both = expect(&[
        ("a.txt", "a1"),
        ("docs/b (conflict, Phone).txt", "phone's"),
        ("docs/b.txt", "laptop's"),
    ]);
    assert_eq!(files(&laptop), both);
    assert_eq!(files(&phone), both);
}

#[test]
fn the_user_can_keep_their_own_version_instead() {
    let bucket = MemoryBucket::new();
    let (mut laptop, mut phone) = laptop_and_phone(&bucket);
    let change = put(&mut laptop, "a.txt", "laptop's");
    laptop.commit(&[change], "laptop edit", &mut keep_both).unwrap();
    let change = put(&mut phone, "a.txt", "phone's");
    let mut asked = Vec::new();
    phone
        .commit(&[change], "phone edit", &mut |conflict| {
            asked.push(conflict.path.clone());
            Resolution::KeepMine
        })
        .unwrap();
    assert_eq!(asked, ["a.txt"]);
    laptop.pull().unwrap();
    assert_eq!(
        files(&laptop),
        expect(&[("a.txt", "phone's"), ("docs/b.txt", "b1")])
    );
}

#[test]
fn a_file_that_moved_on_the_drive_since_its_base_is_noticed_before_the_upload() {
    let bucket = MemoryBucket::new();
    let (mut laptop, mut phone) = laptop_and_phone(&bucket);
    // The laptop's local copy of a.txt is based on this version.
    let base = laptop.head().unwrap();
    let change = put(&mut phone, "a.txt", "phone's");
    phone.commit(&[change], "phone edit", &mut keep_both).unwrap();

    assert!(!laptop.file_changed_since("a.txt", &base).unwrap());
    laptop.pull().unwrap();
    assert!(laptop.file_changed_since("a.txt", &base).unwrap());
    assert!(!laptop.file_changed_since("docs/b.txt", &base).unwrap());
    assert!(!laptop.file_changed_since("never.txt", &base).unwrap());
}

#[test]
fn an_older_version_of_a_file_can_be_restored() {
    let bucket = MemoryBucket::new();
    let (mut laptop, mut phone) = laptop_and_phone(&bucket);
    for version in ["a2", "a3"] {
        let change = put(&mut laptop, "a.txt", version);
        laptop.commit(&[change], version, &mut keep_both).unwrap();
    }
    let change = put(&mut laptop, "new.txt", "later");
    laptop.commit(&[change], "new", &mut keep_both).unwrap();
    let history = laptop.history().unwrap();
    assert_eq!(history.len(), 4);
    let first = history.last().unwrap().0;
    assert_eq!(history[0].1.message, "new");

    laptop
        .restore("a.txt", &first, &mut keep_both)
        .unwrap();
    laptop
        .restore("new.txt", &first, &mut keep_both)
        .unwrap();
    assert_eq!(files(&laptop), expect(&[("a.txt", "a1"), ("docs/b.txt", "b1")]));
    assert_eq!(laptop.history().unwrap().len(), 6);
    phone.pull().unwrap();
    assert_eq!(files(&phone), files(&laptop));
}

#[test]
fn a_commit_packs_only_the_objects_no_pack_holds_yet() {
    let bucket = MemoryBucket::new();
    let (mut laptop, _phone) = laptop_and_phone(&bucket);
    let change = put(&mut laptop, "docs/b.txt", "b2");
    let outcome = laptop.commit(&[change], "edit", &mut keep_both).unwrap();
    // The blob, the docs tree, the root tree, the commit.
    assert_eq!(outcome.published.pack.unwrap().objects, 4);
}

#[test]
fn a_new_device_reads_the_drive_from_a_checkpoint_and_a_compacted_pack_without_listing() {
    let bucket = MemoryBucket::new();
    let (mut laptop, mut phone) = laptop_and_phone(&bucket);
    for i in 0..5 {
        let change = put(&mut laptop, &format!("photos/{i}.jpg"), &format!("pointer {i}"));
        laptop.commit(&[change], "photo", &mut keep_both).unwrap();
        let change = put(&mut phone, &format!("notes/{i}.md"), &format!("note {i}"));
        phone.commit(&[change], "note", &mut keep_both).unwrap();
    }
    laptop.pull().unwrap();
    let store = laptop.store_mut();
    store.checkpoint().unwrap();
    let guard = store.acquire_lease(MAINTENANCE, 600).unwrap();
    assert!(store.compact(&guard).unwrap().is_some());
    store.collect_garbage(&guard, 0).unwrap();
    store.release_lease(guard).unwrap();

    let tablet = Repo::open(bucket.clone(), sealer(), "dev-tablet", "Tablet").unwrap();
    assert_eq!(files(&tablet).len(), 12);
    assert_eq!(files(&tablet), files(&laptop));
    assert_eq!(tablet.store().state().packs.len(), 1);
    phone.pull().unwrap();
    assert_eq!(files(&phone), files(&laptop));
    assert_eq!(bucket.counts().lists, 0);
}
