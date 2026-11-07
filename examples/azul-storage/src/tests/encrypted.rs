//! The encrypted drive: files of every size through the Drive calls, what the bucket sees,
//! names from the index, ranged reads, rename / copy / delete in the index, conditional
//! writes, streaming, tampering, other keys, shares.

use std::{
    io::Read,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Arc, Mutex,
    },
    time::Duration,
};

use super::{mem_bucket::MemBucket, TempDir};
use crate::{
    crypto::{
        azl1::{WriteOptions, HEADER_LEN},
        codec::Compression,
        device::{adopt_invite, seal_invite, setup_new_drive},
        keys::RecoveryKdf,
        random_bytes, DriveKey, ObjectId, ShareKey,
    },
    encrypted::{
        open_encrypted, read_shared, AutoEncrypted, EncryptedDrive, IndexProvider, MemoryIndex,
        NameIndex, Recorded, SharedFile,
    },
    keyring::{KeyringError, KeyringStore, MemoryKeyring},
    ops::list_all,
    trust::{EncState, Via, Why},
    ByteRange, Drive, DriveError, ListRequest, LocalDrive, Precondition,
};

const MIB: usize = 1 << 20;
/// A small segment size: many segments without megabytes of test data.
const SMALL: u32 = 4096;
/// A full small segment as stored: the codec byte, the data and the tag.
const SMALL_SEALED: u64 = 1 + SMALL as u64 + 16;

fn noise(len: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; len];
    random_bytes(&mut bytes).unwrap();
    bytes
}

fn text(len: usize) -> Vec<u8> {
    b"Dear diary, the quarterly numbers look fine and the password is not in here. "
        .iter()
        .copied()
        .cycle()
        .take(len)
        .collect()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle)
}

/// An encrypted drive over a new bucket and index in memory.
fn drive_with(options: WriteOptions) -> EncryptedDrive<Arc<MemBucket>> {
    EncryptedDrive::new(
        Arc::new(MemBucket::new()),
        DriveKey::generate().unwrap(),
        Arc::new(MemoryIndex::new()),
    )
    .with_options(options)
}

fn new_drive() -> EncryptedDrive<Arc<MemBucket>> {
    drive_with(WriteOptions::default())
}

/// Small segments, none compressed: exact offsets.
fn small_segments() -> WriteOptions {
    WriteOptions {
        segment_size: SMALL,
        compression: Compression::Never,
    }
}

#[test]
fn files_of_every_size_round_trip_through_the_drive() {
    let drive = new_drive();
    for (i, size) in [0, 1, MIB, MIB + 1, 3 * MIB + 17].into_iter().enumerate() {
        let path = format!("files/{i}.bin");
        let plain = noise(size);
        drive.put(&path, &plain).unwrap();
        assert_eq!(drive.get(&path).unwrap(), plain, "{size} bytes");
        let info = drive.head(&path).unwrap();
        assert_eq!(info.key, path);
        assert_eq!(info.size, size as u64);
        assert!(info.modified.is_some());
    }
    let long = text(2 * MIB + 3);
    drive.put("notes/long.txt", &long).unwrap();
    assert_eq!(drive.get("notes/long.txt").unwrap(), long);
    let object = drive.entry("notes/long.txt").unwrap().object.unwrap();
    assert!(object.compressed);
    assert!(object.stored_size < long.len() as u64 / 10);
    let shown = drive.metadata("notes/long.txt").unwrap();
    assert!(shown.iter().any(|(name, _)| name == "Encryption"), "{shown:?}");
    assert!(shown.contains(&(String::from("Compressed"), String::from("Yes"))));
}

/// C3: a file is read at the size the index names. An object whose header names another size - a
/// writer who holds the file key can say any - used to be decrypted whole (its plaintext reserved
/// at the header's size) before anything compared it with the index; one that happened to carry
/// the index's hash was returned as the file.
#[test]
fn a_file_whose_object_names_another_size_than_the_index_is_refused_not_returned() {
    let drive = new_drive();
    drive.put("a.txt", b"ten bytes!").unwrap();
    drive.put("b.txt", &noise(5000)).unwrap();
    // The index of a.txt now names b's object (its key, its hash, its length) - and a size of 10.
    let mut entry = drive.entry("b.txt").unwrap();
    entry.size = 10;
    drive
        .index()
        .apply(vec![crate::encrypted::IndexChange::Put {
            path: String::from("a.txt"),
            entry,
            expect: crate::encrypted::Expect::Any,
        }])
        .unwrap();
    match drive.get("a.txt") {
        Err(DriveError::Corrupt { .. }) => {}
        Ok(bytes) => panic!(
            "a file the index calls 10 bytes came back with {} bytes",
            bytes.len()
        ),
        Err(other) => panic!("{other:?}"),
    }
    // The segment reader refuses the length the index does not name just as well.
    let object = drive.entry("a.txt").unwrap().object.unwrap();
    let mut wrong = object.clone();
    wrong.stored_size += 1;
    let mut entry = drive.entry("a.txt").unwrap();
    entry.object = Some(wrong);
    drive
        .index()
        .apply(vec![crate::encrypted::IndexChange::Put {
            path: String::from("c.txt"),
            entry,
            expect: crate::encrypted::Expect::Any,
        }])
        .unwrap();
    assert!(drive.open_reader("c.txt").is_err());
    assert_eq!(
        drive.get("b.txt").unwrap().len(),
        5000,
        "the honest file reads as it did"
    );
}

/// A file's details name the BLAKE3 of its plaintext ([`crate::CONTENT_HASH_METADATA`]), as the
/// index keeps it: a sync compares it with a file on this device without downloading.
#[test]
fn the_metadata_names_the_blake3_of_the_plaintext() {
    let drive = new_drive();
    drive.put("notes/a.txt", b"hello").unwrap();
    let shown = drive.metadata("notes/a.txt").unwrap();
    let want = blake3::hash(b"hello").to_hex().to_string();
    assert!(
        shown.contains(&(crate::CONTENT_HASH_METADATA.to_string(), want)),
        "{shown:?}"
    );
}

#[test]
fn the_bucket_holds_no_plaintext_name_or_content() {
    let drive = new_drive();
    let secret = b"The account password is hunter2-correct-horse".to_vec();
    let diary = [secret.as_slice(), text(100_000).as_slice()].concat();
    let photo = noise(50_000);
    drive.put("Steuererklaerung 2025.pdf", &text(3000)).unwrap();
    drive.put("private/diary.txt", &diary).unwrap();
    drive.put("photos/holiday/IMG_4711.jpg", &photo).unwrap();
    drive.create_folder("empty folder/").unwrap();
    drive.copy("private/diary.txt", "private/diary copy.txt").unwrap();
    drive.rename("private/diary copy.txt", "backup/diary.txt").unwrap();

    let objects = drive.inner().objects();
    assert_eq!(objects.len(), 3, "one object per file version, none for folders or names");
    // Five bytes and more: a shorter needle could turn up in ciphertext by chance.
    let names = [
        "Steuererklaerung",
        "private",
        "diary",
        "photos",
        "holiday",
        "IMG_4711",
        "empty folder",
        "backup",
        "diary.txt",
    ];
    for (key, bytes) in &objects {
        assert!(ObjectId::from_bucket_key(key).is_some(), "a random key: {key}");
        for name in names {
            assert!(!key.contains(name), "{key} names {name}");
            assert!(!contains(bytes, name.as_bytes()), "{key} holds the name {name}");
        }
        assert!(!contains(bytes, &secret), "{key} holds the diary's plaintext");
        assert!(!contains(bytes, &text(64)), "{key} holds the text");
        assert!(!contains(bytes, &photo[1000..1064]), "{key} holds the photo");
    }
}

#[test]
fn a_ranged_read_fetches_the_header_the_tail_and_only_the_covering_segments() {
    let drive = drive_with(small_segments());
    let plain = noise(6 * SMALL as usize + 50);
    drive.put("big.bin", &plain).unwrap();
    let object = drive.entry("big.bin").unwrap().object.unwrap();
    let key = object.id.bucket_key();
    let small = u64::from(SMALL);
    let header = HEADER_LEN as u64;

    let got = drive
        .get_range("big.bin", ByteRange::new(2 * small + 7, Some(3 * small + 9)))
        .unwrap();
    assert_eq!(got, &plain[2 * SMALL as usize + 7..=3 * SMALL as usize + 9]);
    let ranges = drive.inner().ranges();
    assert_eq!(ranges.len(), 3, "{ranges:?}");
    assert!(ranges.iter().all(|(k, _)| *k == key));
    assert_eq!(ranges[0].1, ByteRange::new(0, Some(header - 1)));
    let tail = 7 * 4 + 16 + 8;
    assert_eq!(
        ranges[1].1,
        ByteRange::new(object.stored_size - tail, Some(object.stored_size - 1))
    );
    // Segments 2 and 3, nothing else.
    assert_eq!(
        ranges[2].1,
        ByteRange::new(header + 2 * SMALL_SEALED, Some(header + 4 * SMALL_SEALED - 1))
    );
    assert_eq!(drive.inner().gets(), 0, "no GET of the whole object");

    // The same file again: its header and tail come from the cache.
    let again = drive
        .get_range("big.bin", ByteRange::new(5 * small, Some(5 * small + 3)))
        .unwrap();
    assert_eq!(again, &plain[5 * SMALL as usize..=5 * SMALL as usize + 3]);
    let ranges = drive.inner().ranges();
    assert_eq!(ranges.len(), 4, "{ranges:?}");
    assert_eq!(
        ranges[3].1,
        ByteRange::new(header + 5 * SMALL_SEALED, Some(header + 6 * SMALL_SEALED - 1))
    );

    let to_end = drive
        .get_range("big.bin", ByteRange::new(plain.len() as u64 - 10, None))
        .unwrap();
    assert_eq!(to_end, &plain[plain.len() - 10..]);
    assert!(matches!(
        drive.get_range("big.bin", ByteRange::new(plain.len() as u64, None)),
        Err(DriveError::InvalidRange { .. })
    ));
}

#[test]
fn listing_and_stat_come_from_the_index_with_the_plaintext_sizes() {
    let drive = drive_with(small_segments());
    drive.put("mail/inbox/1.eml", b"one").unwrap();
    drive.put("mail/inbox/2.eml", &noise(10_000)).unwrap();
    drive.put("mail/sent/3.eml", b"three").unwrap();
    drive.put("readme.txt", b"hello").unwrap();
    let asked = (drive.inner().gets(), drive.inner().ranges().len());

    let root = drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(root.folders, vec![String::from("mail/")]);
    let keys: Vec<&str> = root.objects.iter().map(|o| o.key.as_str()).collect();
    assert_eq!(keys, vec!["readme.txt"]);
    assert_eq!(root.objects[0].size, 5);

    let inbox = drive.list(&ListRequest::folder("mail/inbox/")).unwrap();
    let keys: Vec<&str> = inbox.objects.iter().map(|o| o.key.as_str()).collect();
    assert_eq!(keys, vec!["mail/inbox/1.eml", "mail/inbox/2.eml"]);
    assert_eq!(inbox.objects[1].size, 10_000, "the plaintext size");
    assert!(inbox.objects[1].etag.is_some());

    let first = drive
        .list(&ListRequest::recursive("").with_max_keys(2))
        .unwrap();
    assert_eq!(first.objects.len(), 2);
    assert!(first.next.is_some());
    assert_eq!(list_all(&drive, "").unwrap().len(), 4);
    assert_eq!(drive.head("mail/sent/3.eml").unwrap().size, 5);
    assert_eq!(
        (drive.inner().gets(), drive.inner().ranges().len()),
        asked,
        "the bucket was not asked"
    );
    assert!(matches!(
        drive.head("nope.txt"),
        Err(DriveError::NotFound { .. })
    ));
    assert!(matches!(
        drive.get("../escape.txt"),
        Err(DriveError::InvalidKey { .. })
    ));
}

#[test]
fn rename_and_copy_change_the_index_and_leave_the_ciphertext() {
    let drive = new_drive();
    drive.put("a.txt", b"alpha").unwrap();
    let objects = drive.inner().keys();
    let puts = drive.inner().puts();

    drive.rename("a.txt", "docs/b.txt").unwrap();
    assert_eq!(drive.inner().keys(), objects, "the object stayed where it was");
    assert_eq!(drive.inner().puts(), puts);
    assert!(matches!(drive.get("a.txt"), Err(DriveError::NotFound { .. })));
    assert_eq!(drive.get("docs/b.txt").unwrap(), b"alpha");

    drive.put("c.txt", b"gamma").unwrap();
    assert!(matches!(
        drive.rename("c.txt", "docs/b.txt"),
        Err(DriveError::InvalidKey { .. })
    ));
    assert_eq!(drive.get("docs/b.txt").unwrap(), b"alpha", "never over something");

    // A copy names the same object a second time.
    drive.copy("docs/b.txt", "docs/b copy.txt").unwrap();
    assert_eq!(drive.inner().keys().len(), 2, "alpha's object and gamma's");
    assert_eq!(
        drive.head("docs/b.txt").unwrap().etag,
        drive.head("docs/b copy.txt").unwrap().etag
    );
    assert_eq!(drive.get("docs/b copy.txt").unwrap(), b"alpha");

    // The object stays while a name is left, and goes with the last one.
    drive.delete("docs/b.txt").unwrap();
    assert_eq!(drive.inner().keys().len(), 2);
    assert_eq!(drive.get("docs/b copy.txt").unwrap(), b"alpha");
    drive.delete("docs/b copy.txt").unwrap();
    assert_eq!(drive.inner().keys().len(), 1, "only gamma's object");
    drive.delete("docs/b copy.txt").unwrap();
}

#[test]
fn overwriting_a_file_makes_a_new_object_and_deletes_the_old_one() {
    let drive = new_drive();
    drive.put("a.txt", b"one").unwrap();
    let first = drive.inner().keys();
    drive.put("a.txt", b"two").unwrap();
    let second = drive.inner().keys();
    assert_eq!(second.len(), 1);
    assert_ne!(first, second, "a new object under a new random key");
    assert_eq!(drive.inner().deletes(), 1, "the old object went");
    assert_eq!(drive.get("a.txt").unwrap(), b"two");

    // A copy keeps the old version alive while it names it.
    drive.copy("a.txt", "b.txt").unwrap();
    drive.put("a.txt", b"three").unwrap();
    assert_eq!(drive.inner().keys().len(), 2);
    assert_eq!(drive.get("a.txt").unwrap(), b"three");
    assert_eq!(drive.get("b.txt").unwrap(), b"two");
}

#[test]
fn renaming_a_folder_moves_every_entry_under_it_in_the_index() {
    let drive = new_drive();
    drive.put("docs/a.txt", b"a").unwrap();
    drive.put("docs/sub/b.txt", b"b").unwrap();
    drive.create_folder("docs/empty/").unwrap();
    drive.put("other.txt", b"o").unwrap();
    let objects = drive.inner().keys();

    drive.rename("docs/", "archive/2025/").unwrap();
    assert_eq!(drive.inner().keys(), objects);
    assert_eq!(drive.get("archive/2025/a.txt").unwrap(), b"a");
    assert_eq!(drive.get("archive/2025/sub/b.txt").unwrap(), b"b");
    assert!(drive.head("archive/2025/empty/").is_ok());
    assert!(list_all(&drive, "docs/").unwrap().is_empty());

    assert!(matches!(
        drive.rename("archive/", "archive/inner/"),
        Err(DriveError::InvalidKey { .. })
    ));
    assert!(matches!(
        drive.rename("nothing/", "else/"),
        Err(DriveError::NotFound { .. })
    ));
    drive.put("taken/x.txt", b"x").unwrap();
    assert!(matches!(
        drive.rename("archive/", "taken/"),
        Err(DriveError::InvalidKey { .. })
    ));

    drive.delete_folder("archive/").unwrap();
    assert!(list_all(&drive, "archive/").unwrap().is_empty());
    assert_eq!(
        drive.inner().keys().len(),
        2,
        "the objects of other.txt and taken/x.txt"
    );
}

#[test]
fn a_conditional_write_holds_only_while_the_file_is_unchanged() {
    let drive = new_drive();
    let first = drive
        .put_if("a.txt", b"v1", &Precondition::Absent)
        .unwrap()
        .expect("the object id as the tag");
    assert!(matches!(
        drive.put_if("a.txt", b"v1 again", &Precondition::Absent),
        Err(DriveError::Conflict { .. })
    ));
    let second = drive
        .put_if("a.txt", b"v2", &Precondition::Matches(first.clone()))
        .unwrap()
        .unwrap();
    assert_ne!(second, first);
    assert!(matches!(
        drive.put_if("a.txt", b"stale", &Precondition::Matches(first)),
        Err(DriveError::Conflict { .. })
    ));
    assert!(matches!(
        drive.put_if(
            "a.txt",
            b"x",
            &Precondition::Matches(String::from("not an id"))
        ),
        Err(DriveError::Conflict { .. })
    ));
    assert_eq!(drive.get("a.txt").unwrap(), b"v2");
    assert_eq!(drive.head("a.txt").unwrap().etag, Some(second));
    assert_eq!(
        drive.inner().keys().len(),
        1,
        "the objects of the writes that lost were taken back"
    );
}

#[test]
fn the_object_put_is_conditional_when_the_inner_drive_can_be() {
    let bucket = Arc::new(MemBucket::with_conditional_writes());
    let conditional = EncryptedDrive::new(
        Arc::clone(&bucket),
        DriveKey::generate().unwrap(),
        Arc::new(MemoryIndex::new()),
    );
    conditional.put("a.txt", b"x").unwrap();
    let puts = bucket.conditional_puts();
    assert_eq!(puts.len(), 1);
    assert_eq!(puts[0].1, Precondition::Absent, "If-None-Match: *");
    assert!(ObjectId::from_bucket_key(&puts[0].0).is_some());
    assert_eq!(bucket.puts(), 0, "no plain PUT");
    assert_eq!(conditional.get("a.txt").unwrap(), b"x");

    // A bucket without conditional writes gets a plain PUT.
    let plain = new_drive();
    plain.put("a.txt", b"x").unwrap();
    assert_eq!(plain.inner().puts(), 1);
    assert!(plain.inner().conditional_puts().is_empty());
}

#[test]
fn an_encrypted_drive_over_a_folder_on_disk_keeps_only_random_names_there() {
    let tmp = TempDir::new("encrypted-local");
    let drive = EncryptedDrive::new(
        LocalDrive::without_manifest(tmp.path()),
        DriveKey::generate().unwrap(),
        Arc::new(MemoryIndex::new()),
    );
    drive.put("Briefe/an Oma.txt", b"Liebe Oma").unwrap();
    assert_eq!(drive.get("Briefe/an Oma.txt").unwrap(), b"Liebe Oma");
    let on_disk = list_all(drive.inner(), "").unwrap();
    assert_eq!(on_disk.len(), 1);
    assert!(
        ObjectId::from_bucket_key(&on_disk[0].key).is_some(),
        "{}",
        on_disk[0].key
    );
    assert!(!tmp.path().join("Briefe").exists());
}

#[test]
fn put_from_streams_a_reader_into_an_object() {
    let drive = drive_with(small_segments());
    let plain = noise(20 * SMALL as usize + 3);
    let written = drive.put_from("stream.bin", &mut &plain[..]).unwrap();
    assert_eq!(written, plain.len() as u64);
    assert_eq!(drive.get("stream.bin").unwrap(), plain);

    // Above the spool's 8 MiB in memory: through a temporary file, into a folder on disk
    // through its own streaming put.
    let tmp = TempDir::new("encrypted-stream");
    let on_disk = EncryptedDrive::new(
        LocalDrive::without_manifest(tmp.path()),
        DriveKey::generate().unwrap(),
        Arc::new(MemoryIndex::new()),
    );
    let big = noise(9 * MIB + 5);
    assert_eq!(
        on_disk.put_from("big.bin", &mut &big[..]).unwrap(),
        big.len() as u64
    );
    assert_eq!(on_disk.get("big.bin").unwrap(), big);

    // And back a segment at a time.
    let mut reader = on_disk.open_reader("big.bin").unwrap();
    assert_eq!(reader.plaintext_size(), big.len() as u64);
    let mut out = Vec::new();
    reader.read_to_end(&mut out).unwrap();
    assert_eq!(out, big);
}

#[test]
fn a_tampered_swapped_or_missing_object_reads_as_corrupt() {
    let drive = drive_with(small_segments());
    let plain = noise(3 * SMALL as usize);
    drive.put("a.bin", &plain).unwrap();
    let a_key = drive.entry("a.bin").unwrap().object.unwrap().id.bucket_key();

    // One flipped byte in segment 1.
    let mut bytes = drive.inner().object(&a_key).unwrap();
    bytes[HEADER_LEN + SMALL_SEALED as usize + 100] ^= 1;
    drive.inner().set(&a_key, bytes);
    assert!(matches!(
        drive.get("a.bin"),
        Err(DriveError::Corrupt { .. })
    ));
    assert_eq!(
        drive
            .get_range("a.bin", ByteRange::new(0, Some(99)))
            .unwrap(),
        &plain[..100],
        "segment 0 is intact"
    );
    let small = u64::from(SMALL);
    assert!(matches!(
        drive.get_range("a.bin", ByteRange::new(small, Some(small))),
        Err(DriveError::Corrupt { .. })
    ));

    // Another file's object in its place.
    drive.put("b.bin", &noise(100)).unwrap();
    let b_key = drive.entry("b.bin").unwrap().object.unwrap().id.bucket_key();
    drive.inner().set(&a_key, drive.inner().object(&b_key).unwrap());
    drive.forget_opened();
    assert!(matches!(
        drive.get("a.bin"),
        Err(DriveError::Corrupt { .. })
    ));
    assert!(matches!(
        drive.get_range("a.bin", ByteRange::new(0, Some(10))),
        Err(DriveError::Corrupt { .. })
    ));

    // An object gone from the bucket.
    drive.inner().delete(&b_key).unwrap();
    assert!(matches!(
        drive.get("b.bin"),
        Err(DriveError::Corrupt { reason, .. }) if reason.contains("missing")
    ));
}

#[test]
fn another_drive_key_reads_nothing_and_the_same_key_reads_everything() {
    let bucket = Arc::new(MemBucket::new());
    let index: Arc<dyn NameIndex> = Arc::new(MemoryIndex::new());
    let key = DriveKey::generate().unwrap();
    let ours = EncryptedDrive::new(Arc::clone(&bucket), key.clone(), Arc::clone(&index));
    ours.put("a.txt", b"secret").unwrap();

    let theirs = EncryptedDrive::new(
        Arc::clone(&bucket),
        DriveKey::generate().unwrap(),
        Arc::clone(&index),
    );
    assert!(matches!(theirs.get("a.txt"), Err(DriveError::Denied { .. })));
    assert!(matches!(
        theirs.get_range("a.txt", ByteRange::new(0, None)),
        Err(DriveError::Denied { .. })
    ));

    // Another device with the drive key (a member's unwrapped copy).
    let device = EncryptedDrive::new(Arc::clone(&bucket), key, Arc::clone(&index));
    assert_eq!(device.get("a.txt").unwrap(), b"secret");
    assert_eq!(device.drive_key_id(), ours.drive_key_id());
}

#[test]
fn a_shared_file_opens_with_its_share_key_and_no_other_file_does() {
    let drive = new_drive();
    drive
        .put("shared/report.pdf", b"%PDF-1.7 quarterly numbers")
        .unwrap();
    drive.put("private/diary.txt", b"dear diary").unwrap();
    let share = ShareKey::generate().unwrap();
    let grant = drive.share_file("shared/report.pdf", &share).unwrap();
    assert_eq!(
        read_shared(drive.inner(), &grant, &share).unwrap(),
        b"%PDF-1.7 quarterly numbers"
    );

    let diary = drive.entry("private/diary.txt").unwrap().object.unwrap();
    // The grant pointed at the diary's object: it names the report.
    let mut moved = grant.clone();
    moved.object = diary.id;
    assert!(matches!(
        read_shared(drive.inner(), &moved, &share),
        Err(DriveError::Corrupt { .. })
    ));
    // The diary's own wrap belongs to the drive key, not to the share.
    let direct = SharedFile {
        object: diary.id,
        stored_size: diary.stored_size,
        wrapped_key: diary.wrapped_key.clone(),
    };
    assert!(matches!(
        read_shared(drive.inner(), &direct, &share),
        Err(DriveError::Denied { .. })
    ));
    // Another share key opens nothing.
    assert!(matches!(
        read_shared(drive.inner(), &grant, &ShareKey::generate().unwrap()),
        Err(DriveError::Denied { .. })
    ));
}

#[test]
fn folder_markers_live_in_the_index_only() {
    let drive = new_drive();
    drive.create_folder("photos/2026/").unwrap();
    assert!(drive.inner().keys().is_empty());
    let root = drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(root.folders, vec![String::from("photos/")]);
    assert_eq!(drive.head("photos/2026/").unwrap().size, 0);
    assert!(matches!(
        drive.get("photos/2026/"),
        Err(DriveError::NotFound { .. })
    ));
    // What generic code does to make a folder: an empty object named like it.
    drive.put("music/", &[]).unwrap();
    assert!(drive.head("music/").is_ok());
    assert!(drive.inner().keys().is_empty());
}

/// The tests' index provider: one index in memory that every device shares (the metadata
/// repository is the apps' provider).
struct SharedIndex(Arc<MemoryIndex>);

impl IndexProvider for SharedIndex {
    fn open_index(
        &self,
        _drive: &str,
        _bucket: Arc<dyn Drive>,
        _drive_key: &DriveKey,
    ) -> Result<Arc<dyn NameIndex>, DriveError> {
        Ok(self.0.clone())
    }
}

#[test]
fn a_device_opens_an_encrypted_drive_with_its_own_key_and_the_providers_index() {
    let mem = Arc::new(MemBucket::new());
    let bucket: Arc<dyn Drive> = mem.clone();
    let first = MemoryKeyring::new();
    let kdf = RecoveryKdf::with_cost(64, 1, 1).unwrap();
    let (drive_key, _) = setup_new_drive(bucket.as_ref(), &first, "d_1", kdf).unwrap();
    let provider = SharedIndex(Arc::new(MemoryIndex::new()));

    let drive = open_encrypted(bucket.clone(), &first, "d_1", &provider).unwrap();
    assert_eq!(drive.drive_key_id(), drive_key.id());
    drive.put("letters/a.txt", b"hello").unwrap();

    // A device without a key is refused as a drive locked here, not handed an empty drive: it
    // unlocks with the recovery code or joins from a device that has the key.
    assert!(matches!(
        open_encrypted(bucket.clone(), &MemoryKeyring::new(), "d_1", &provider),
        Err(DriveError::Locked { drive }) if drive == "d_1"
    ));

    // A second device, enrolled by an invite, reads what the first wrote.
    let invite = seal_invite(bucket.as_ref(), "d_1", &drive_key).unwrap();
    let second = MemoryKeyring::new();
    adopt_invite(bucket.as_ref(), &second, "d_1", &invite, None).unwrap();
    let other = open_encrypted(bucket, &second, "d_1", &provider).unwrap();
    assert_eq!(other.get("letters/a.txt").unwrap(), b"hello");
    assert!(
        !mem.keys().iter().any(|key| key.contains("letters")),
        "{:?}",
        mem.keys()
    );
}

fn provider() -> Option<Arc<dyn IndexProvider>> {
    Some(Arc::new(SharedIndex(Arc::new(MemoryIndex::new()))))
}

/// A drive's state read as the two-valued answer the older assertions ask: `Some(true)` for an
/// encrypted drive, `Some(false)` for one that is not, `None` while nothing is decided.
fn kind(drive: &AutoEncrypted) -> Option<bool> {
    match drive.state() {
        EncState::Undecided => None,
        EncState::Encrypted { .. } => Some(true),
        EncState::SetupPending | EncState::Unencrypted(_) => Some(false),
    }
}

/// A bucket without key files that this device knows nothing about is read as it lists (it may be
/// a drive made before encryption), and takes no plaintext file: an Azlin drive is encrypted.
#[test]
fn an_auto_encrypted_drive_reads_a_plain_bucket_and_takes_no_file_into_it() {
    let mem = Arc::new(MemBucket::new());
    mem.set("a.txt", b"plain".to_vec());
    let drive = AutoEncrypted::new(mem.clone(), "d_1", Arc::new(MemoryKeyring::new()), provider());
    assert_eq!(kind(&drive), None, "nothing decided before the first call");
    assert_eq!(drive.get("a.txt").unwrap(), b"plain");
    assert_eq!(kind(&drive), Some(false));
    assert!(matches!(
        drive.put("b.txt", b"not plain"),
        Err(DriveError::Denied { .. })
    ));
    assert!(
        mem.object("b.txt").is_none(),
        "no plaintext reached the bucket"
    );
}

/// "We always encrypt": a new Azlin drive whose encryption was not set up (it failed, the app
/// closed first) is a plain bucket without files. It takes no plaintext file: every write is
/// refused until the drive is encrypted, and then the write is sealed.
#[test]
fn a_new_plain_drive_takes_no_file_until_it_is_encrypted() {
    let mem = Arc::new(MemBucket::new());
    // Only the drive's bookkeeping: still a new drive.
    mem.set(".azlin/policy.toml", b"# policy".to_vec());
    let keyring = Arc::new(MemoryKeyring::new());
    let drive = AutoEncrypted::new(mem.clone(), "d_new", keyring.clone(), provider());
    assert!(drive.list(&ListRequest::folder("")).is_ok(), "it reads as a drive");
    assert_eq!(kind(&drive), Some(false));
    assert!(drive.needs_encryption());
    let refused = |result: Result<(), DriveError>, what: &str| {
        assert!(matches!(result, Err(DriveError::Denied { .. })), "{what}: {result:?}");
    };
    refused(drive.put("a.txt", b"plaintext"), "put");
    refused(drive.put_from("b.txt", &mut &b"plaintext"[..]).map(|_| ()), "put_from");
    refused(
        drive.put_if("c.txt", b"plaintext", &Precondition::Absent).map(|_| ()),
        "put_if",
    );
    refused(drive.create_folder("photos/"), "create_folder");
    let batch = drive.put_many(&[crate::BatchPut::new("d.txt", b"plaintext")]);
    assert!(matches!(batch[0], Err(DriveError::Denied { .. })), "put_many");
    assert_eq!(mem.keys(), vec![String::from(".azlin/policy.toml")], "nothing reached the bucket");

    let kdf = RecoveryKdf::with_cost(64, 1, 1).unwrap();
    setup_new_drive(drive.bucket().as_ref(), keyring.as_ref(), "d_new", kdf).unwrap();
    drive.reopen();
    drive.put("a.txt", b"sealed now").unwrap();
    assert_eq!(kind(&drive), Some(true));
    assert!(!drive.needs_encryption());
    assert!(mem.object("a.txt").is_none(), "no plaintext name in the bucket");
    assert_eq!(drive.get("a.txt").unwrap(), b"sealed now");
}

/// A drive made before encryption (it holds files) keeps opening and reading, takes no file in the
/// clear - the bucket's listing cannot tell it from a hostile one that hides its key files -,
/// and is not set up by itself: its menu offers the encryption, which moves its files in.
#[test]
fn an_older_plain_drive_with_files_reads_and_takes_no_file_until_it_is_encrypted() {
    let mem = Arc::new(MemBucket::new());
    mem.set("photos/2019/a.jpg", b"jpeg bytes".to_vec());
    let keyring = Arc::new(MemoryKeyring::new());
    let drive = AutoEncrypted::new(mem.clone(), "d_old", keyring.clone(), provider());
    assert_eq!(drive.get("photos/2019/a.jpg").unwrap(), b"jpeg bytes");
    assert_eq!(kind(&drive), Some(false));
    assert!(
        !drive.needs_encryption(),
        "the user's word starts its encryption, not the app's"
    );
    assert!(matches!(
        drive.put("notes.txt", b"not plain"),
        Err(DriveError::Denied { .. })
    ));
    assert!(mem.object("notes.txt").is_none());

    // Encrypted (the app's "Encrypt this drive"), it takes files, sealed.
    let kdf = RecoveryKdf::with_cost(64, 1, 1).unwrap();
    setup_new_drive(drive.bucket().as_ref(), keyring.as_ref(), "d_old", kdf).unwrap();
    drive.reopen();
    drive.put("notes.txt", b"sealed").unwrap();
    assert_eq!(kind(&drive), Some(true));
    assert!(mem.object("notes.txt").is_none());
}

#[test]
fn an_auto_encrypted_drive_opens_an_encrypted_bucket_through_the_index() {
    let mem = Arc::new(MemBucket::new());
    let keyring = Arc::new(MemoryKeyring::new());
    let kdf = RecoveryKdf::with_cost(64, 1, 1).unwrap();
    setup_new_drive(mem.as_ref(), keyring.as_ref(), "d_1", kdf).unwrap();
    let drive = AutoEncrypted::new(mem.clone(), "d_1", keyring.clone(), provider());
    drive.put("letters/a.txt", b"sealed").unwrap();
    assert_eq!(kind(&drive), Some(true));
    assert_eq!(drive.get("letters/a.txt").unwrap(), b"sealed");
    assert!(mem.object("letters/a.txt").is_none(), "no plaintext name in the bucket");

    // A build without an index refuses rather than showing the raw bucket.
    let no_index = AutoEncrypted::new(mem.clone(), "d_1", keyring, None);
    assert!(matches!(
        no_index.list(&ListRequest::folder("")),
        Err(DriveError::Unsupported(_))
    ));
    assert_eq!(kind(&no_index), None, "tried again on the next call");
}

/// A bucket that hides its key files from listings (a server trying to turn devices back to
/// plaintext).
struct HidesKeyFiles(Arc<MemBucket>);

impl Drive for HidesKeyFiles {
    fn list(&self, request: &ListRequest) -> Result<crate::ListPage, DriveError> {
        let mut page = self.0.list(request)?;
        page.objects.retain(|o| !o.key.starts_with(".azlin/"));
        page.folders.retain(|f| !f.starts_with(".azlin/"));
        Ok(page)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.0.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.0.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.0.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.0.delete(key)
    }
    fn head(&self, key: &str) -> Result<crate::ObjectInfo, DriveError> {
        self.0.head(key)
    }
}

#[test]
fn a_bucket_hiding_its_key_files_cannot_turn_a_device_with_the_key_back_to_plaintext() {
    let mem = Arc::new(MemBucket::new());
    let keyring = Arc::new(MemoryKeyring::new());
    let kdf = RecoveryKdf::with_cost(64, 1, 1).unwrap();
    setup_new_drive(mem.as_ref(), keyring.as_ref(), "d_1", kdf).unwrap();
    let drive = AutoEncrypted::new(
        Arc::new(HidesKeyFiles(mem.clone())),
        "d_1",
        keyring,
        provider(),
    );
    drive.put("diary.txt", b"dear diary").unwrap();
    assert_eq!(kind(&drive), Some(true), "this device keeps the key");
    assert!(mem.object("diary.txt").is_none());
}

/// The record of the drives file: a device that found the drive encrypted once is never taken to a
/// plain drive afterwards - its key gone from the keyring (a reinstall, a wiped keychain), a
/// bucket that hides its key files and shows an object: the drive is locked, every call says
/// so, and nothing is written.
#[test]
fn a_drive_recorded_as_encrypted_is_locked_without_its_key_whatever_the_bucket_lists() {
    let mem = Arc::new(MemBucket::new());
    let first = MemoryKeyring::new();
    let kdf = RecoveryKdf::with_cost(64, 1, 1).unwrap();
    let (key, _code) = setup_new_drive(mem.as_ref(), &first, "d_1", kdf).unwrap();
    mem.set("decoy.txt", b"plain".to_vec());

    let drive = AutoEncrypted::new(
        Arc::new(HidesKeyFiles(mem.clone())),
        "d_1",
        Arc::new(MemoryKeyring::new()),
        provider(),
    )
    .with_record(Recorded {
        encrypted: true,
        key_id: Some(key.id()),
        ..Recorded::default()
    });
    let locked = |what: &str, error: DriveError| {
        assert!(
            matches!(&error, DriveError::Locked { drive } if drive == "d_1"),
            "{what}: {error:?}"
        );
    };
    locked("list", drive.list(&ListRequest::folder("")).unwrap_err());
    locked("get", drive.get("decoy.txt").unwrap_err());
    locked("head", drive.head("decoy.txt").unwrap_err());
    locked("put", drive.put("diary.txt", b"dear diary").unwrap_err());
    locked("delete", drive.delete("decoy.txt").unwrap_err());
    assert_eq!(kind(&drive), None, "locked: nothing is decided");
    assert!(!drive.needs_encryption());
    assert!(mem.object("diary.txt").is_none());
    assert!(
        mem.object("decoy.txt").is_some(),
        "nothing was deleted either"
    );

    // With the key back (unlocked with the recovery code), the same drive opens encrypted.
    let keyring = Arc::new(MemoryKeyring::new());
    let device = AutoEncrypted::new(
        Arc::new(HidesKeyFiles(mem.clone())),
        "d_1",
        keyring.clone(),
        provider(),
    )
    .with_record(Recorded {
        encrypted: true,
        key_id: None,
        ..Recorded::default()
    });
    assert!(matches!(
        device.put("a.txt", b"x"),
        Err(DriveError::Locked { .. })
    ));
    crate::crypto::device::enroll(mem.as_ref(), keyring.as_ref(), "d_1", &key).unwrap();
    device.reopen();
    device.put("a.txt", b"sealed").unwrap();
    assert_eq!(kind(&device), Some(true));
    assert!(mem.object("a.txt").is_none());
}

/// A drive the device never recorded is decided from the device's key, and the app is told the
/// key's id when it opens encrypted - to write it down; a drive that takes no file is told of
/// nothing.
#[test]
fn the_app_is_told_the_drive_keys_id_when_the_drive_opens_encrypted() {
    let told: Arc<Mutex<Vec<crate::crypto::KeyId>>> = Arc::default();
    let tell = {
        let told = Arc::clone(&told);
        Arc::new(move |id: &crate::crypto::KeyId| told.lock().unwrap().push(*id))
    };
    let mem = Arc::new(MemBucket::new());
    let keyring = Arc::new(MemoryKeyring::new());
    let drive = AutoEncrypted::new(mem.clone(), "d_1", keyring.clone(), provider())
        .on_encrypted(tell.clone());
    drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(kind(&drive), Some(false));
    assert!(
        told.lock().unwrap().is_empty(),
        "a drive without a key is told of nothing"
    );
    assert_eq!(drive.recorded(), Recorded::default());

    let kdf = RecoveryKdf::with_cost(64, 1, 1).unwrap();
    let (key, _code) =
        setup_new_drive(drive.bucket().as_ref(), keyring.as_ref(), "d_1", kdf).unwrap();
    drive.reopen();
    drive.put("a.txt", b"sealed").unwrap();
    assert_eq!(*told.lock().unwrap(), vec![key.id()]);
    assert_eq!(
        drive.recorded(),
        Recorded {
            encrypted: true,
            key_id: Some(key.id()),
            ..Recorded::default()
        }
    );
    drive.put("b.txt", b"sealed").unwrap();
    assert_eq!(told.lock().unwrap().len(), 1, "told once per opening");
}

/// K3 with the app's view: a setup this device started and did not finish (it kept the drive's key,
/// and the recovery wrap of that try is in the bucket) is a drive waiting for its setup, not a
/// locked one nobody can open: it takes no file, the app is told to set it up, and the same
/// setup call finishes it.
#[test]
fn a_setup_that_stopped_halfway_is_a_drive_waiting_for_its_setup_not_a_locked_one() {
    let mem = Arc::new(MemBucket::new());
    let keyring = Arc::new(MemoryKeyring::new());
    // The state a stop after the recovery wrap leaves: the key planned in the keyring, the
    // recovery wrap of this very key in the bucket, no wrap for this device.
    let key = DriveKey::generate().unwrap();
    crate::crypto::device::plan_drive_key(keyring.as_ref(), "d_1", &key).unwrap();
    let kdf = RecoveryKdf::with_cost(64, 1, 1).unwrap();
    let code = crate::crypto::keys::RecoveryCode::generate().unwrap();
    let wrap = crate::crypto::keys::RecoveryWrap::seal(&key, "d_1", &code, kdf.clone()).unwrap();
    crate::crypto::keys::store_recovery_wrap(mem.as_ref(), &wrap).unwrap();
    assert!(crate::crypto::device::is_encrypted(mem.as_ref()).unwrap());

    let drive = AutoEncrypted::new(mem.clone(), "d_1", keyring.clone(), provider());
    drive.list(&ListRequest::folder("")).unwrap();
    assert!(
        drive.needs_encryption(),
        "waiting for its setup, not Locked"
    );
    assert!(matches!(
        drive.put("a.txt", b"x"),
        Err(DriveError::Denied { .. })
    ));
    assert!(mem.object("a.txt").is_none());

    setup_new_drive(drive.bucket().as_ref(), keyring.as_ref(), "d_1", kdf).unwrap();
    drive.reopen();
    drive.put("a.txt", b"sealed").unwrap();
    assert_eq!(kind(&drive), Some(true));
    assert!(!drive.needs_encryption());
    assert_eq!(drive.get("a.txt").unwrap(), b"sealed");
}

/// A keyring nothing can be read from: the user said "No" to the system's prompt, or the
/// system has no keyring, or it failed.
struct UnreadableKeyring(KeyringError);

impl KeyringStore for UnreadableKeyring {
    fn get(&self, _key: &str) -> Result<Option<String>, KeyringError> {
        Err(self.0.clone())
    }
    fn set(&self, _key: &str, _secret: &str) -> Result<(), KeyringError> {
        Err(self.0.clone())
    }
    fn delete(&self, _key: &str) -> Result<(), KeyringError> {
        Err(self.0.clone())
    }
}

/// C1/K8: a keyring that cannot be read (a declined prompt, none on the system, a failure) is no
/// answer about the key, and the bucket's word decides nothing. A bucket that looks plain (it
/// shows an object and no key files) used to turn the drive into a plain one, and every file
/// written went out in the clear.
#[test]
fn a_keyring_that_cannot_be_read_never_turns_a_drive_plain() {
    for failure in [
        KeyringError::Denied,
        KeyringError::Unavailable,
        KeyringError::Failed(String::from("the keychain is locked")),
    ] {
        let mem = Arc::new(MemBucket::new());
        mem.set("decoy.txt", b"plain".to_vec());
        let drive = AutoEncrypted::new(
            mem.clone(),
            "d_1",
            Arc::new(UnreadableKeyring(failure.clone())),
            provider(),
        );
        assert!(
            drive.put("diary.txt", b"dear diary").is_err(),
            "{failure:?}: put"
        );
        assert!(
            drive.list(&ListRequest::folder("")).is_err(),
            "{failure:?}: list"
        );
        assert!(drive.get("decoy.txt").is_err(), "{failure:?}: get");
        assert_eq!(kind(&drive), None, "{failure:?}: nothing was decided");
        assert!(
            mem.object("diary.txt").is_none(),
            "{failure:?}: nothing reached the bucket"
        );
    }
}

/// The same, then the keyring answers: the next call decides.
#[test]
fn a_keyring_that_answers_later_decides_then() {
    struct Flaky {
        inner: MemoryKeyring,
        down: std::sync::atomic::AtomicBool,
    }
    impl KeyringStore for Flaky {
        fn get(&self, key: &str) -> Result<Option<String>, KeyringError> {
            if self.down.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(KeyringError::Denied);
            }
            self.inner.get(key)
        }
        fn set(&self, key: &str, secret: &str) -> Result<(), KeyringError> {
            self.inner.set(key, secret)
        }
        fn delete(&self, key: &str) -> Result<(), KeyringError> {
            self.inner.delete(key)
        }
    }
    let mem = Arc::new(MemBucket::new());
    let keyring = Arc::new(Flaky {
        inner: MemoryKeyring::new(),
        down: std::sync::atomic::AtomicBool::new(false),
    });
    let kdf = RecoveryKdf::with_cost(64, 1, 1).unwrap();
    setup_new_drive(mem.as_ref(), keyring.as_ref(), "d_1", kdf).unwrap();
    keyring
        .down
        .store(true, std::sync::atomic::Ordering::SeqCst);
    // Even though the bucket shows no key files, as a hostile one would.
    let drive = AutoEncrypted::new(
        Arc::new(HidesKeyFiles(mem.clone())),
        "d_1",
        keyring.clone(),
        provider(),
    );
    assert!(matches!(
        drive.put("a.txt", b"x"),
        Err(DriveError::Denied { .. })
    ));
    assert_eq!(kind(&drive), None);
    keyring
        .down
        .store(false, std::sync::atomic::Ordering::SeqCst);
    drive.put("a.txt", b"sealed").unwrap();
    assert_eq!(kind(&drive), Some(true));
    assert!(
        mem.object("a.txt").is_none(),
        "no plaintext name in the bucket"
    );
}

/// C1: a device that has no key for the drive (a new computer, a reinstall) meets a bucket that
/// hides its key files and shows an object: no file it writes goes out in the clear.
#[test]
fn a_device_without_the_key_writes_no_plaintext_into_a_bucket_that_hides_its_key_files() {
    let mem = Arc::new(MemBucket::new());
    let first = MemoryKeyring::new();
    let kdf = RecoveryKdf::with_cost(64, 1, 1).unwrap();
    setup_new_drive(mem.as_ref(), &first, "d_1", kdf).unwrap();
    mem.set("decoy.txt", b"plain".to_vec());

    let drive = AutoEncrypted::new(
        Arc::new(HidesKeyFiles(mem.clone())),
        "d_1",
        Arc::new(MemoryKeyring::new()),
        provider(),
    );
    let refused = |result: Result<(), DriveError>, what: &str| {
        assert!(result.is_err(), "{what} went through: {result:?}");
    };
    refused(drive.put("diary.txt", b"dear diary"), "put");
    refused(
        drive.put_from("b.txt", &mut &b"dear diary"[..]).map(|_| ()),
        "put_from",
    );
    refused(
        drive
            .put_if("c.txt", b"dear diary", &Precondition::Absent)
            .map(|_| ()),
        "put_if",
    );
    refused(drive.create_folder("photos/"), "create_folder");
    let batch = drive.put_many(&[crate::BatchPut::new("d.txt", b"dear diary")]);
    assert!(batch[0].is_err(), "put_many");
    for name in ["diary.txt", "b.txt", "c.txt", "d.txt", "photos/"] {
        assert!(mem.object(name).is_none(), "{name} reached the bucket");
    }
    assert_ne!(kind(&drive), Some(true));
}

/// A bucket whose first read waits until the test lets it go: a drive's first call held up by
/// the network, as a slow node holds it.
struct HeldBucket {
    inner: Arc<MemBucket>,
    entered: Mutex<Option<mpsc::Sender<()>>>,
    release: Mutex<mpsc::Receiver<()>>,
}

impl HeldBucket {
    fn hold_the_first_read(&self) {
        let Some(entered) = self.entered.lock().unwrap().take() else {
            return;
        };
        let _ = entered.send(());
        let _ = self.release.lock().unwrap().recv();
    }
}

impl Drive for HeldBucket {
    fn list(&self, request: &ListRequest) -> Result<crate::ListPage, DriveError> {
        self.hold_the_first_read();
        self.inner.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.hold_the_first_read();
        self.inner.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.hold_the_first_read();
        self.inner.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.inner.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.inner.delete(key)
    }
    fn head(&self, key: &str) -> Result<crate::ObjectInfo, DriveError> {
        self.hold_the_first_read();
        self.inner.head(key)
    }
}

/// The app's UI thread asks whether a drive is encrypted (to draw it, to plan its upkeep) while
/// a worker's first call to the drive still decides it - and that call waits for an answer
/// which, in the apps, only the UI thread delivers. The question must not wait for the
/// decision: waiting is a deadlock.
#[test]
fn whether_a_drive_is_encrypted_is_answered_while_its_first_call_decides_it() {
    let mem = Arc::new(MemBucket::new());
    mem.set("a.txt", b"plain".to_vec());
    let (entered_tx, entered) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    let bucket = Arc::new(HeldBucket {
        inner: mem,
        entered: Mutex::new(Some(entered_tx)),
        release: Mutex::new(release_rx),
    });
    let drive = Arc::new(AutoEncrypted::new(
        bucket,
        "d_1",
        Arc::new(MemoryKeyring::new()),
        provider(),
    ));
    let first = {
        let drive = Arc::clone(&drive);
        std::thread::spawn(move || drive.list(&ListRequest::folder("")).map(|_| ()))
    };
    entered
        .recv_timeout(Duration::from_secs(10))
        .expect("the first call reached the bucket");
    let (answer_tx, answer) = mpsc::channel();
    {
        let drive = Arc::clone(&drive);
        std::thread::spawn(move || {
            let _ = answer_tx.send(kind(&drive));
        });
    }
    let asked = answer.recv_timeout(Duration::from_secs(5));
    release.send(()).unwrap();
    first.join().unwrap().unwrap();
    assert_eq!(asked, Ok(None), "not decided yet - and said at once");
    assert_eq!(kind(&drive), Some(false), "decided by the first call");
}

/// The same question asked on the app's UI thread, before any call opened the drive, while the
/// first call still opens it, and after. The drive's lock is held across network requests, so
/// the UI thread never takes it (a debug build asserts that at the lock, contended or not): a
/// question that took it would fail here every time, not only when it happens to race.
#[test]
fn the_ui_thread_asks_whether_a_drive_is_encrypted_without_taking_its_lock() {
    let mem = Arc::new(MemBucket::new());
    mem.set("a.txt", b"plain".to_vec());
    let (entered_tx, entered) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    let bucket = Arc::new(HeldBucket {
        inner: mem,
        entered: Mutex::new(Some(entered_tx)),
        release: Mutex::new(release_rx),
    });
    let drive = Arc::new(AutoEncrypted::new(
        bucket,
        "d_1",
        Arc::new(MemoryKeyring::new()),
        provider(),
    ));
    let (ask_tx, ask) = mpsc::channel::<()>();
    let (answer_tx, answer) = mpsc::channel();
    let ui = {
        let drive = Arc::clone(&drive);
        std::thread::spawn(move || {
            crate::ui_thread::mark_ui_thread();
            while ask.recv().is_ok() {
                let asked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| kind(&drive)))
                    .map_err(|_| "the UI thread took the drive's lock");
                let _ = answer_tx.send(asked);
            }
        })
    };
    let ask_ui = || {
        ask_tx.send(()).unwrap();
        answer.recv_timeout(Duration::from_secs(5))
    };
    assert_eq!(ask_ui(), Ok(Ok(None)), "before any call");
    let first = {
        let drive = Arc::clone(&drive);
        std::thread::spawn(move || drive.list(&ListRequest::folder("")).map(|_| ()))
    };
    entered
        .recv_timeout(Duration::from_secs(10))
        .expect("the first call reached the bucket");
    let meanwhile = ask_ui();
    release.send(()).unwrap();
    first.join().unwrap().unwrap();
    assert_eq!(meanwhile, Ok(Ok(None)), "while the first call opens the drive");
    assert_eq!(ask_ui(), Ok(Ok(Some(false))), "after it decided");
    drop(ask_tx);
    ui.join().unwrap();
}

#[test]
fn reopening_takes_a_drive_encrypted_since() {
    let mem = Arc::new(MemBucket::new());
    let keyring = Arc::new(MemoryKeyring::new());
    let drive = AutoEncrypted::new(mem.clone(), "d_1", keyring.clone(), provider());
    drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(kind(&drive), Some(false));
    let kdf = RecoveryKdf::with_cost(64, 1, 1).unwrap();
    setup_new_drive(drive.bucket().as_ref(), keyring.as_ref(), "d_1", kdf).unwrap();
    assert_eq!(kind(&drive), Some(false), "until it is reopened");
    drive.reopen();
    drive.put("new.txt", b"sealed").unwrap();
    assert_eq!(kind(&drive), Some(true));
    assert!(mem.object("new.txt").is_none());
}

// ==== RF40: the state is one value, the gate lets a call through, the record is compared ====

/// A keyring that counts its reads and answers every one with `answer`.
struct Counting {
    gets: AtomicUsize,
    answer: KeyringError,
}

impl Counting {
    fn new(answer: KeyringError) -> Arc<Counting> {
        Arc::new(Counting {
            gets: AtomicUsize::new(0),
            answer,
        })
    }

    fn gets(&self) -> usize {
        self.gets.load(Ordering::SeqCst)
    }
}

impl KeyringStore for Counting {
    fn get(&self, _key: &str) -> Result<Option<String>, KeyringError> {
        self.gets.fetch_add(1, Ordering::SeqCst);
        Err(self.answer.clone())
    }
    fn set(&self, _key: &str, _secret: &str) -> Result<(), KeyringError> {
        Err(self.answer.clone())
    }
    fn delete(&self, _key: &str) -> Result<(), KeyringError> {
        Err(self.answer.clone())
    }
}

/// X-AR6-26802cb54-3: a keychain prompt the user declined is not shown again with every listing
/// refresh, sync attempt and thumbnail - it is remembered for the back-off, and the user's own act
/// (an unlock, a reopen) asks at once.
#[test]
fn x_ar6_26802cb54_3_a_declined_prompt_is_asked_once_in_the_back_off_and_again_after_a_reopen() {
    let mem = Arc::new(MemBucket::new());
    mem.set("decoy.txt", b"plain".to_vec());
    let keyring = Counting::new(KeyringError::Denied);
    let drive = AutoEncrypted::new(mem.clone(), "d_1", keyring.clone(), provider())
        .with_denied_backoff(Duration::from_secs(30));
    for call in 0..5 {
        let answer = drive.list(&ListRequest::folder(""));
        assert!(
            matches!(answer, Err(DriveError::Denied { .. })),
            "call {call}: {answer:?}"
        );
    }
    assert_eq!(
        keyring.gets(),
        1,
        "the user was asked once, not once per call"
    );
    assert_eq!(
        drive.state(),
        EncState::Undecided,
        "and nothing was decided"
    );
    assert!(mem.object("decoy.txt").is_some());

    drive.reopen();
    assert!(drive.list(&ListRequest::folder("")).is_err());
    assert_eq!(keyring.gets(), 2, "the user's own act asks again at once");
}

/// X-AR6-26802cb54-3: only a DECLINE is remembered. A keyring that is not there or failed answers
/// afresh with every call (it may be back), and a back-off of nothing asks every time.
#[test]
fn x_ar6_26802cb54_3_only_a_declined_prompt_is_remembered() {
    for (answer, backoff) in [
        (KeyringError::Unavailable, Duration::from_secs(30)),
        (
            KeyringError::Failed(String::from("locked")),
            Duration::from_secs(30),
        ),
        (KeyringError::Denied, Duration::ZERO),
    ] {
        let keyring = Counting::new(answer.clone());
        let drive = AutoEncrypted::new(
            Arc::new(MemBucket::new()),
            "d_1",
            keyring.clone(),
            provider(),
        )
        .with_denied_backoff(backoff);
        for _ in 0..3 {
            assert!(drive.list(&ListRequest::folder("")).is_err());
        }
        assert_eq!(
            keyring.gets(),
            3,
            "{answer:?} after {backoff:?}: asked with every call"
        );
    }
}

/// X-AR6-26802cb54-2 (a): a drive that is not encrypted takes no delete either - a record-less
/// device cannot tell an older plain drive from a bucket that hides its key files, and a delete
/// is a write. It still reads as its bucket lists.
#[test]
fn x_ar6_26802cb54_2_a_drive_that_is_not_encrypted_takes_no_delete() {
    let mem = Arc::new(MemBucket::new());
    mem.set("photos/a.jpg", b"jpeg".to_vec());
    mem.set("notes.txt", b"plain".to_vec());
    let drive = AutoEncrypted::new(
        mem.clone(),
        "d_old",
        Arc::new(MemoryKeyring::new()),
        provider(),
    );
    assert_eq!(drive.get("notes.txt").unwrap(), b"plain", "it reads");
    let denied = |result: Result<(), DriveError>, what: &str| {
        assert!(
            matches!(result, Err(DriveError::Denied { .. })),
            "{what}: {result:?}"
        );
    };
    denied(drive.delete("notes.txt"), "delete");
    denied(drive.delete_folder("photos/"), "delete_folder");
    let batch = drive.delete_many(&[crate::BatchDelete::new("notes.txt")]);
    denied(batch.into_iter().next().unwrap(), "delete_many");
    assert!(
        mem.object("notes.txt").is_some() && mem.object("photos/a.jpg").is_some(),
        "nothing was deleted"
    );
}

/// X-AR6-26802cb54-2 (b): the key id the drives file recorded is compared with the key the drive
/// opens with. Another line of keys is refused and said; a key that replaced the recorded one (a
/// rotation: the key chain holds it) is the drive's own; the user's own reopening takes the
/// key it brings and records it.
#[test]
fn x_ar6_26802cb54_2_a_recorded_key_is_the_drives_own_or_one_that_replaced_it() {
    let mem = Arc::new(MemBucket::new());
    let first = DriveKey::generate().unwrap();
    let second = DriveKey::generate().unwrap();
    let stranger = DriveKey::generate().unwrap();
    let mut chain = crate::crypto::keys::KeyChain::new();
    chain.add("d_1", &first, &second).unwrap();
    crate::crypto::keys::store_key_chain(mem.as_ref(), &chain).unwrap();
    let keyring = Arc::new(MemoryKeyring::new());
    crate::crypto::device::store_drive_key(keyring.as_ref(), "d_1", &second).unwrap();

    // The recorded key was replaced by the one in the keyring (this device or another rotated).
    let rotated =
        AutoEncrypted::new(mem.clone(), "d_1", keyring.clone(), provider()).with_record(Recorded {
            encrypted: true,
            key_id: Some(first.id()),
            ..Recorded::default()
        });
    rotated.list(&ListRequest::folder("")).unwrap();
    assert_eq!(
        rotated.recorded().key_id,
        Some(second.id()),
        "the new key is recorded"
    );

    // The recorded key is of another line: refused, nothing decided, nothing read.
    let swapped =
        AutoEncrypted::new(mem.clone(), "d_1", keyring.clone(), provider()).with_record(Recorded {
            encrypted: true,
            key_id: Some(stranger.id()),
            ..Recorded::default()
        });
    let refused = swapped.list(&ListRequest::folder(""));
    assert!(
        matches!(&refused, Err(DriveError::KeyRefused { drive, .. }) if drive == "d_1"),
        "{refused:?}"
    );
    assert_eq!(swapped.state(), EncState::Undecided);
    assert!(swapped.put("a.txt", b"x").is_err());

    // The user opens it again on purpose (an unlock, a join): the key it brings is recorded.
    swapped.reopen();
    swapped.list(&ListRequest::folder("")).unwrap();
    assert_eq!(swapped.recorded().key_id, Some(second.id()));
}

/// X-AR5b-1 (the bridge and AzMail): an app that opens an encrypted drive only meets a bucket that
/// hides its key files and shows an object, with no key on the device: it reads nothing and
/// writes nothing - what the bucket lists is the bucket's word alone.
#[test]
fn x_ar5b_1_an_encrypted_only_app_reads_and_writes_nothing_of_a_bucket_that_hides_its_keys() {
    let mem = Arc::new(MemBucket::new());
    let first = MemoryKeyring::new();
    let kdf = RecoveryKdf::with_cost(64, 1, 1).unwrap();
    setup_new_drive(mem.as_ref(), &first, "d_1", kdf).unwrap();
    mem.set("decoy.txt", b"plain".to_vec());

    let drive = AutoEncrypted::new(
        Arc::new(HidesKeyFiles(mem.clone())),
        "d_1",
        Arc::new(MemoryKeyring::new()),
        provider(),
    )
    .encrypted_only();
    let answers = [
        ("list", drive.list(&ListRequest::folder("")).map(|_| ())),
        ("get", drive.get("decoy.txt").map(|_| ())),
        ("head", drive.head("decoy.txt").map(|_| ())),
        ("put", drive.put("diary.txt", b"dear diary")),
        ("delete", drive.delete("decoy.txt")),
    ];
    for (what, answer) in &answers {
        assert!(answer.is_err(), "{what} went through: {answer:?}");
    }
    assert!(mem.object("diary.txt").is_none(), "nothing was written");
    assert!(mem.object("decoy.txt").is_some(), "nothing was deleted");

    // The same app with the key opens the drive as usual.
    let keyring = Arc::new(MemoryKeyring::new());
    let kdf = RecoveryKdf::with_cost(64, 1, 1).unwrap();
    let other = Arc::new(MemBucket::new());
    setup_new_drive(other.as_ref(), keyring.as_ref(), "d_2", kdf).unwrap();
    let keyed = AutoEncrypted::new(other, "d_2", keyring, provider()).encrypted_only();
    keyed.put("a.txt", b"sealed").unwrap();
    assert_eq!(keyed.get("a.txt").unwrap(), b"sealed");
}

/// X-AR5b-3: `decide` answers what a dummy call used to swallow - the state, or why there is none
/// - and every state has its own reading.
#[test]
fn x_ar5b_3_decide_says_what_it_decided_or_why_it_could_not() {
    let kdf = || RecoveryKdf::with_cost(64, 1, 1).unwrap();
    let list = ListRequest::folder("");

    // A keyring that cannot be read: the error, and nothing decided.
    let mem = Arc::new(MemBucket::new());
    let unreadable = AutoEncrypted::new(
        mem.clone(),
        "d_1",
        Arc::new(UnreadableKeyring(KeyringError::Denied)),
        provider(),
    );
    assert!(matches!(
        unreadable.decide(),
        Err(DriveError::Denied { .. })
    ));
    assert_eq!(unreadable.state(), EncState::Undecided);

    // Key files listed, no key and no record here: locked, not plain.
    let mem = Arc::new(MemBucket::new());
    let owner = MemoryKeyring::new();
    let (key, _code) = setup_new_drive(mem.as_ref(), &owner, "d_1", kdf()).unwrap();
    let locked = AutoEncrypted::new(
        mem.clone(),
        "d_1",
        Arc::new(MemoryKeyring::new()),
        provider(),
    );
    assert!(matches!(locked.decide(), Err(DriveError::Locked { .. })));
    assert_eq!(locked.state(), EncState::Undecided);

    // The key in the keyring.
    let kept = AutoEncrypted::new(mem.clone(), "d_1", Arc::new(owner), provider());
    assert_eq!(
        kept.decide().unwrap(),
        EncState::Encrypted {
            key_id: Some(key.id()),
            via: Via::KeyInKeyring
        }
    );

    // Only this device's member key and its wrap in the bucket: the bucket's key files made the
    // drive encrypted, the device's own wrap opens it.
    let owner = MemoryKeyring::new();
    let mem = Arc::new(MemBucket::new());
    let (key, _code) = setup_new_drive(mem.as_ref(), &owner, "d_1", kdf()).unwrap();
    let member_only = |keyring: &MemoryKeyring| {
        let entry = crate::crypto::device::member_key_entry("d_1");
        keyring
            .set(&entry, &owner.get(&entry).unwrap().unwrap())
            .unwrap();
    };
    let listed = Arc::new(MemoryKeyring::new());
    member_only(&listed);
    let by_wrap = AutoEncrypted::new(mem.clone(), "d_1", listed, provider());
    assert_eq!(
        by_wrap.decide().unwrap(),
        EncState::Encrypted {
            key_id: Some(key.id()),
            via: Via::KeyFilesListed
        }
    );
    let recorded = Arc::new(MemoryKeyring::new());
    member_only(&recorded);
    let by_record =
        AutoEncrypted::new(mem.clone(), "d_1", recorded, provider()).with_record(Recorded {
            encrypted: true,
            key_id: Some(key.id()),
            ..Recorded::default()
        });
    assert_eq!(
        by_record.decide().unwrap(),
        EncState::Encrypted {
            key_id: Some(key.id()),
            via: Via::Record
        }
    );

    // An empty bucket, and a bucket with files, and no key anywhere.
    let empty = AutoEncrypted::new(
        Arc::new(MemBucket::new()),
        "d_2",
        Arc::new(MemoryKeyring::new()),
        provider(),
    );
    assert_eq!(empty.decide().unwrap(), EncState::Unencrypted(Why::New));
    assert!(empty.needs_encryption());
    let files = Arc::new(MemBucket::new());
    files.set("a.txt", b"plain".to_vec());
    let hidden = AutoEncrypted::new(files, "d_3", Arc::new(MemoryKeyring::new()), provider());
    assert_eq!(
        hidden.decide().unwrap(),
        EncState::Unencrypted(Why::HiddenOrAbsent)
    );
    assert!(!hidden.needs_encryption());
    assert!(
        hidden.list(&list).is_ok(),
        "it still reads, as its bucket lists"
    );
}

/// X-AR5b-2: a drive that could not be decided restores nothing - the policy for "could not
/// tell" is to open the drive first, never the bucket's restore.
#[test]
fn x_ar5b_2_a_drive_the_keyring_would_not_open_is_no_drive_to_restore_by_its_bucket() {
    let mem = Arc::new(MemBucket::new());
    mem.set("decoy.txt", b"plain".to_vec());
    let drive = AutoEncrypted::new(
        mem,
        "d_1",
        Arc::new(UnreadableKeyring(KeyringError::Denied)),
        provider(),
    );
    assert!(drive.decide().is_err());
    assert_eq!(
        drive.state().restore_kind(),
        crate::trust::RestoreKind::OpenFirst
    );
}

/// X-AR5b-1 (the command line): a program that opens an encrypted drive only. A bucket that hides
/// its key files and shows an object, on a device with no key and no record, is refused as a drive
/// that is not encrypted - nothing is read from it, nothing can be written - where the command
/// line used to take it for a plaintext drive and put the file in the clear.
#[test]
fn x_ar5b_1_open_encrypted_only_refuses_what_is_not_encrypted_and_has_no_plain_path() {
    use crate::encrypted::{judge, open_encrypted_only, OpenRefusal};

    let kdf = || RecoveryKdf::with_cost(64, 1, 1).unwrap();
    let provider = SharedIndex(Arc::new(MemoryIndex::new()));
    let mem = Arc::new(MemBucket::new());
    let owner = MemoryKeyring::new();
    let (key, _code) = setup_new_drive(mem.as_ref(), &owner, "d_1", kdf()).unwrap();
    mem.set("decoy.txt", b"plain".to_vec());
    let hidden: Arc<dyn Drive> = Arc::new(HidesKeyFiles(mem.clone()));
    let keyless = MemoryKeyring::new();

    // The hostile bucket: not encrypted as far as this device can tell, and refused.
    let refused = open_encrypted_only(
        Arc::clone(&hidden),
        &keyless,
        "d_1",
        &provider,
        Recorded::default(),
    );
    assert!(
        matches!(refused, Err(OpenRefusal::NotEncrypted(Why::HiddenOrAbsent))),
        "{refused:?}"
    );
    assert_eq!(
        judge(hidden.as_ref(), &keyless, "d_1", Recorded::default()).unwrap(),
        EncState::Unencrypted(Why::HiddenOrAbsent)
    );
    assert!(
        mem.object("decoy.txt").is_some() && mem.keys().len() > 1,
        "and nothing was touched"
    );

    // The device recorded the drive as encrypted: locked, not plain, whatever the bucket lists.
    let locked = open_encrypted_only(
        Arc::clone(&hidden),
        &keyless,
        "d_1",
        &provider,
        Recorded {
            encrypted: true,
            key_id: Some(key.id()),
            ..Recorded::default()
        },
    );
    assert!(
        matches!(&locked, Err(OpenRefusal::Locked { drive, recorded: true }) if drive == "d_1"),
        "{locked:?}"
    );

    // Key files in the listing and no key here: locked for want of a key.
    let listed: Arc<dyn Drive> = mem.clone();
    let locked = open_encrypted_only(listed, &keyless, "d_1", &provider, Recorded::default());
    assert!(
        matches!(
            &locked,
            Err(OpenRefusal::Locked {
                recorded: false,
                ..
            })
        ),
        "{locked:?}"
    );

    // A keyring that cannot be read decides nothing.
    let unreadable = open_encrypted_only(
        Arc::clone(&hidden),
        &UnreadableKeyring(KeyringError::Denied),
        "d_1",
        &provider,
        Recorded::default(),
    );
    assert!(matches!(
        unreadable,
        Err(OpenRefusal::Keyring(DriveError::Denied { .. }))
    ));

    // The key in the keyring opens it, whatever the bucket hides; a record of another key does not.
    let opened = open_encrypted_only(
        Arc::clone(&hidden),
        &owner,
        "d_1",
        &provider,
        Recorded::default(),
    )
    .unwrap();
    assert_eq!(opened.drive_key_id(), key.id());
    let swapped = open_encrypted_only(
        hidden,
        &owner,
        "d_1",
        &provider,
        Recorded {
            encrypted: true,
            key_id: Some(DriveKey::generate().unwrap().id()),
            ..Recorded::default()
        },
    );
    assert!(matches!(
        swapped,
        Err(OpenRefusal::Refused(DriveError::KeyRefused { .. }))
    ));

    // A new drive and a stopped setup are not encrypted either.
    let empty: Arc<dyn Drive> = Arc::new(MemBucket::new());
    let new = open_encrypted_only(empty, &keyless, "d_2", &provider, Recorded::default());
    assert!(
        matches!(new, Err(OpenRefusal::NotEncrypted(Why::New))),
        "{new:?}"
    );
}

/// The quota counts stored bytes: the totals say what the files take of it (compressed, a copy
/// once) and how big they are before compression.
#[test]
fn the_totals_count_stored_bytes_once_and_the_files_original_size() {
    let drive = new_drive();
    let notes = text(300_000);
    let photo = noise(50_000);
    drive.put("notes/minutes.txt", &notes).unwrap();
    drive.put("photos/a.jpg", &photo).unwrap();
    drive.copy("notes/minutes.txt", "backup/minutes.txt").unwrap();
    drive.create_folder("empty/").unwrap();
    let totals = drive.totals().unwrap();
    assert_eq!(totals.files, 3, "a folder marker is no file");
    assert_eq!(
        totals.original_bytes,
        2 * notes.len() as u64 + photo.len() as u64
    );
    let stored: u64 = drive.inner().objects().iter().map(|(_, b)| b.len() as u64).sum();
    assert_eq!(totals.stored_bytes, stored, "the copy's object counted once");
    assert!(totals.stored_bytes < totals.original_bytes / 2, "{totals:?}");
}

// ==== Compression on by default: what the drive stores and what the quota counts ====

const BASELINE_JPEG: &[u8] = include_bytes!("../../../azlin-compress/tests/fixtures/baseline.jpg");

/// A new drive compresses without being told: a JPEG goes through Lepton, a PDF's text is
/// compressed (the skip list stored both as they were), and the totals count the stored bytes,
/// which are what the quota counts.
#[test]
fn a_new_drive_stores_a_jpeg_and_a_pdf_smaller_and_counts_the_stored_bytes() {
    let drive = new_drive();
    let mut pdf = b"%PDF-1.7\n".to_vec();
    pdf.extend(text(300_000));
    drive.put("photos/IMG_0001.jpg", BASELINE_JPEG).unwrap();
    drive.put("letters/offer.pdf", &pdf).unwrap();
    let mut stored = 0;
    for (path, plain) in [("photos/IMG_0001.jpg", BASELINE_JPEG), ("letters/offer.pdf", &pdf[..])] {
        let entry = drive.entry(path).unwrap();
        assert_eq!(entry.size, plain.len() as u64, "{path}");
        let object = entry.object.unwrap();
        assert!(object.compressed, "{path}");
        // Smaller than the object it would be stored as: a 13 KB JPEG cannot also win back the
        // object's 4 KiB header against its plain length.
        let as_it_was = crate::crypto::azl1::stored_object_len(plain.len() as u64, crate::crypto::azl1::DEFAULT_SEGMENT_SIZE);
        assert!(object.stored_size < as_it_was, "{path}: {} of {as_it_was}", object.stored_size);
        let in_bucket = drive.inner().get(&object.id.bucket_key()).unwrap();
        assert_eq!(object.stored_size, in_bucket.len() as u64, "{path}");
        stored += object.stored_size;
        assert_eq!(drive.get(path).unwrap(), plain, "{path}");
        // A ranged read of the transformed JPEG reads back too.
        let range = crate::ByteRange { start: 100, end: Some(4000) };
        assert_eq!(drive.get_range(path, range).unwrap(), &plain[100..=4000], "{path}");
    }
    let totals = drive.totals().unwrap();
    assert_eq!(totals.files, 2);
    assert_eq!(totals.original_bytes, (BASELINE_JPEG.len() + pdf.len()) as u64);
    assert_eq!(totals.stored_bytes, stored);
    assert!(totals.stored_bytes < totals.original_bytes);
}

/// A streamed write (a sync's upload) transforms a JPEG as a write from memory does.
#[test]
fn a_streamed_jpeg_is_stored_as_a_transformed_object() {
    let drive = new_drive();
    let mut body = BASELINE_JPEG;
    drive
        .write_from("photos/streamed.jpg", &mut body, crate::encrypted::Expect::Any)
        .unwrap();
    let object = drive.entry("photos/streamed.jpg").unwrap().object.unwrap();
    let in_bucket = drive.inner().get(&object.id.bucket_key()).unwrap();
    assert_eq!(u16::from_le_bytes([in_bucket[4], in_bucket[5]]), 2, "AZL1 version 2");
    let mut reader = drive.open_reader("photos/streamed.jpg").unwrap();
    let mut back = Vec::new();
    std::io::Read::read_to_end(&mut reader, &mut back).unwrap();
    assert_eq!(back, BASELINE_JPEG);
}

/// X-AR5b-16: the length an object's sealed header names is checked against the length the bucket
/// really has (the index's `stored_size`) before the segments are looked for - the header is the
/// writer's word, and a size it declares reserves nothing ahead of the bytes (`open_expecting`).
/// An object that is longer or shorter than the index names is refused whole and by range.
#[test]
fn x_ar5b_16_an_object_of_another_length_than_the_index_names_is_refused_before_it_is_read() {
    let drive = new_drive();
    drive.put("notes/a.txt", &text(5000)).unwrap();
    let (key, bytes) = drive.inner().objects().into_iter().next().unwrap();
    assert_eq!(
        drive.get("notes/a.txt").unwrap(),
        text(5000),
        "the object as written reads"
    );

    let mut longer = bytes.clone();
    longer.push(0);
    drive.inner().set(&key, longer);
    let whole = drive.get("notes/a.txt");
    assert!(
        matches!(whole, Err(DriveError::Corrupt { .. })),
        "{whole:?}"
    );

    drive.inner().set(&key, bytes[..bytes.len() - 1].to_vec());
    assert!(drive.get("notes/a.txt").is_err(), "a shorter object");
    let ranged = drive.get_range("notes/a.txt", ByteRange::new(0, Some(10)));
    assert!(ranged.is_err(), "and a range of it: {ranged:?}");
}

/// X-DELTA-AZDRIVE-KEYS-18: a call is judged by the state of the decision whose drive it was
/// handed. The state used to be read in a second step, after the drive: a reopening on another
/// thread that decided the drive encrypted in between made a write meet the bare bucket of the OLD
/// decision ("not encrypted") with the policy of the new one ("takes everything"). The hook is that
/// other thread, at the place: with the state read after it, "a.txt" lands in the bucket in the
/// clear (the first assertion that fails on the old code is the refusal).
#[test]
fn x_delta_azdrive_keys_18_a_write_is_judged_by_the_state_of_the_drive_it_was_handed() {
    let mem = Arc::new(MemBucket::new());
    let keyring = Arc::new(MemoryKeyring::new());
    let me: Arc<Mutex<Option<std::sync::Weak<AutoEncrypted>>>> = Arc::new(Mutex::new(None));
    let fired = Arc::new(AtomicUsize::new(0));
    let hook: Arc<dyn Fn() + Send + Sync> = {
        let (me, mem, keyring, fired) = (
            Arc::clone(&me),
            Arc::clone(&mem),
            Arc::clone(&keyring),
            Arc::clone(&fired),
        );
        Arc::new(move || {
            if fired.fetch_add(1, Ordering::SeqCst) > 0 {
                return;
            }
            let drive = me
                .lock()
                .unwrap()
                .as_ref()
                .and_then(std::sync::Weak::upgrade)
                .expect("the drive is alive");
            // Another thread's whole story, between this call's drive and its judgement.
            let kdf = RecoveryKdf::with_cost(64, 1, 1).unwrap();
            setup_new_drive(mem.as_ref(), keyring.as_ref(), "d_1", kdf).unwrap();
            drive.reopen();
            assert!(
                matches!(drive.decide(), Ok(EncState::Encrypted { .. })),
                "the other thread decided the drive encrypted"
            );
        })
    };
    let drive = Arc::new(
        AutoEncrypted::new(mem.clone(), "d_1", keyring.clone(), provider())
            .with_after_resolved(hook),
    );
    *me.lock().unwrap() = Some(Arc::downgrade(&drive));

    let first = drive.put("a.txt", b"in the clear");
    assert!(
        matches!(first, Err(DriveError::Denied { .. })),
        "judged by the state the drive was decided with (not encrypted): {first:?}"
    );
    assert!(
        mem.object("a.txt").is_none(),
        "no plaintext reached the bucket: {:?}",
        mem.keys()
    );
    assert_eq!(
        fired.load(Ordering::SeqCst),
        1,
        "the hook ran in the window"
    );

    // The next call meets the drive as it is now: encrypted, and sealed.
    drive.put("b.txt", b"sealed").unwrap();
    assert!(mem.object("b.txt").is_none(), "the name is in the index");
    assert_eq!(drive.get("b.txt").unwrap(), b"sealed");
}

/// X-DELTA-AZDRIVE-KEYS-18 (the other half): a restore changes files and no key, so the drive it
/// reopens keeps the key id this device recorded - a keychain put back from an older backup is
/// still refused after it (`reopen` forgot the id, and the next open took whatever the keyring
/// held and wrote it down as the truth). The user's own act that brings a key is `reopen`.
#[test]
fn x_delta_azdrive_keys_18_a_reload_keeps_the_recorded_key_and_a_reopening_takes_the_new_one() {
    let mem = Arc::new(MemBucket::new());
    let first = DriveKey::generate().unwrap();
    let second = DriveKey::generate().unwrap();
    let stranger = DriveKey::generate().unwrap();
    let mut chain = crate::crypto::keys::KeyChain::new();
    chain.add("d_1", &first, &second).unwrap();
    crate::crypto::keys::store_key_chain(mem.as_ref(), &chain).unwrap();
    let keyring = Arc::new(MemoryKeyring::new());
    crate::crypto::device::store_drive_key(keyring.as_ref(), "d_1", &second).unwrap();
    let drive =
        AutoEncrypted::new(mem.clone(), "d_1", keyring.clone(), provider()).with_record(Recorded {
            encrypted: true,
            key_id: Some(first.id()),
            ..Recorded::default()
        });
    drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(drive.recorded().key_id, Some(second.id()));

    // A keychain put back from an older backup: a key of another line.
    crate::crypto::device::store_drive_key(keyring.as_ref(), "d_1", &stranger).unwrap();
    drive.reload();
    assert_eq!(
        drive.state(),
        EncState::Undecided,
        "the next call opens the drive again"
    );
    assert_eq!(
        drive.recorded().key_id,
        Some(second.id()),
        "a restore brought no key: the recorded id stays"
    );
    let refused = drive.list(&ListRequest::folder(""));
    assert!(
        matches!(&refused, Err(DriveError::KeyRefused { drive, .. }) if drive == "d_1"),
        "{refused:?}"
    );

    // The user brings a key on purpose (an unlock, a join, a setup, a rotation).
    drive.reopen();
    drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(drive.recorded().key_id, Some(stranger.id()));
}

/// X-DELTA-AZDRIVE-KEYS-22: the question "does this bucket hold a file of the drive" reads root
/// pages for a bounded number of pages. A storage that answers "there is more" for ever with
/// nothing but its own bookkeeping used to keep the first call - and the lock every later call
/// waits on - for ever; it is a drive that holds files now (`HiddenOrAbsent`): nothing is set up,
/// nothing is written.
#[test]
fn x_delta_azdrive_keys_22_a_root_listing_that_never_ends_is_a_drive_that_takes_no_file() {
    let endless = Arc::new(super::ops::Endless::new(
        "",
        |n| format!(".azlin/pad-{n}"),
        false,
    ));
    let drive = AutoEncrypted::new(
        endless.clone(),
        "d_1",
        Arc::new(MemoryKeyring::new()),
        provider(),
    );
    assert_eq!(
        drive.decide().unwrap(),
        EncState::Unencrypted(Why::HiddenOrAbsent)
    );
    assert_eq!(
        endless.asked() as usize,
        crate::limits::named::ROOT_SCAN_PAGES,
        "the pages read, no more"
    );
    assert!(matches!(
        drive.put("a.txt", b"plaintext"),
        Err(DriveError::Denied { .. })
    ));

    // A listing that names its own page as the next is no proof of an empty bucket either.
    let repeating = Arc::new(super::ops::Endless::new(
        "",
        |n| format!(".azlin/pad-{n}"),
        true,
    ));
    let drive = AutoEncrypted::new(
        repeating.clone(),
        "d_1",
        Arc::new(MemoryKeyring::new()),
        provider(),
    );
    assert_eq!(
        drive.decide().unwrap(),
        EncState::Unencrypted(Why::HiddenOrAbsent)
    );
    assert_eq!(repeating.asked(), 2);
}

/// X-DELTA-AZDRIVE-KEYS-21: a key id in the record that is no key id is a damaged record, not "no
/// id": `and_then(KeyId::from_hex)` turned it into "nothing to compare", the drive opened with
/// whatever key the keyring held and the id was written down as the truth. Now the drive is found
/// encrypted, does not open (`KeyRefused`, the record named) and stays shut until the user opens it
/// on purpose; that brings the key and records it.
#[test]
fn x_delta_azdrive_keys_21_an_unreadable_key_id_in_the_record_is_refused_and_not_skipped() {
    // The text is read in one place.
    assert_eq!(Recorded::from_entry(false, None), Recorded::default());
    let key = DriveKey::generate().unwrap();
    assert_eq!(
        Recorded::from_entry(true, Some(&key.id().to_hex())),
        Recorded {
            encrypted: true,
            key_id: Some(key.id()),
            damaged: false,
        }
    );
    for text in [
        "",
        "not a key id",
        "0011",
        "zz112233445566778899aabbccddeeff",
    ] {
        let record = Recorded::from_entry(false, Some(text));
        assert!(
            record.damaged && record.encrypted && record.key_id.is_none(),
            "{text:?}"
        );
    }

    // Through the drive: this device holds the key of an encrypted drive.
    let mem = Arc::new(MemBucket::new());
    let keyring = Arc::new(MemoryKeyring::new());
    let kdf = RecoveryKdf::with_cost(64, 1, 1).unwrap();
    setup_new_drive(mem.as_ref(), keyring.as_ref(), "d_1", kdf).unwrap();
    let drive = AutoEncrypted::new(mem.clone(), "d_1", keyring.clone(), provider())
        .with_record(Recorded::from_entry(true, Some("not a key id")));
    let refused = drive.list(&ListRequest::folder(""));
    assert!(
        matches!(&refused, Err(DriveError::KeyRefused { drive, reason })
            if drive == "d_1" && reason.contains("not readable")),
        "{refused:?}"
    );
    assert!(drive.put("a.txt", b"x").is_err(), "and nothing is written");
    assert!(mem.object("a.txt").is_none());
    assert_eq!(drive.state(), EncState::Undecided, "nothing was decided");

    // The apps that open an encrypted drive only refuse it as well.
    let provider = SharedIndex(Arc::new(MemoryIndex::new()));
    let refused = crate::encrypted::open_encrypted_only(
        mem.clone(),
        keyring.as_ref(),
        "d_1",
        &provider,
        Recorded::from_entry(true, Some("not a key id")),
    );
    assert!(
        matches!(
            &refused,
            Err(crate::encrypted::OpenRefusal::Refused(
                DriveError::KeyRefused { .. }
            ))
        ),
        "{:?}",
        refused.as_ref().err()
    );

    // The user's own act that brings a key records it.
    drive.reopen();
    drive.list(&ListRequest::folder("")).unwrap();
    let held = crate::crypto::device::load_drive_key(keyring.as_ref(), "d_1")
        .unwrap()
        .unwrap();
    assert_eq!(
        drive.recorded(),
        Recorded {
            encrypted: true,
            key_id: Some(held.id()),
            damaged: false,
        }
    );
}
