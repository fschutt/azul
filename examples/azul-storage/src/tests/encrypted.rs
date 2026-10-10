//! The encrypted drive: files of every size through the Drive calls, what the bucket sees,
//! names from the index, ranged reads, rename / copy / delete in the index, conditional
//! writes, streaming, tampering, other keys, shares.

use std::{io::Read, sync::Arc};

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
        NameIndex, SharedFile,
    },
    keyring::MemoryKeyring,
    ops::list_all,
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

    // A device without a key is refused, not handed an empty drive.
    assert!(matches!(
        open_encrypted(bucket.clone(), &MemoryKeyring::new(), "d_1", &provider),
        Err(DriveError::Denied { .. })
    ));

    // A second device, enrolled by an invite, reads what the first wrote.
    let invite = seal_invite(bucket.as_ref(), "d_1", &drive_key).unwrap();
    let second = MemoryKeyring::new();
    adopt_invite(bucket.as_ref(), &second, "d_1", &invite).unwrap();
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

#[test]
fn an_auto_encrypted_drive_uses_a_plain_bucket_as_it_is() {
    let mem = Arc::new(MemBucket::new());
    mem.set("a.txt", b"plain".to_vec());
    let drive = AutoEncrypted::new(mem.clone(), "d_1", Arc::new(MemoryKeyring::new()), provider());
    assert_eq!(drive.is_encrypted(), None, "nothing decided before the first call");
    assert_eq!(drive.get("a.txt").unwrap(), b"plain");
    assert_eq!(drive.is_encrypted(), Some(false));
    drive.put("b.txt", b"also plain").unwrap();
    assert_eq!(mem.object("b.txt").unwrap(), b"also plain");
}

#[test]
fn an_auto_encrypted_drive_opens_an_encrypted_bucket_through_the_index() {
    let mem = Arc::new(MemBucket::new());
    let keyring = Arc::new(MemoryKeyring::new());
    let kdf = RecoveryKdf::with_cost(64, 1, 1).unwrap();
    setup_new_drive(mem.as_ref(), keyring.as_ref(), "d_1", kdf).unwrap();
    let drive = AutoEncrypted::new(mem.clone(), "d_1", keyring.clone(), provider());
    drive.put("letters/a.txt", b"sealed").unwrap();
    assert_eq!(drive.is_encrypted(), Some(true));
    assert_eq!(drive.get("letters/a.txt").unwrap(), b"sealed");
    assert!(mem.object("letters/a.txt").is_none(), "no plaintext name in the bucket");

    // A build without an index refuses rather than showing the raw bucket.
    let no_index = AutoEncrypted::new(mem.clone(), "d_1", keyring, None);
    assert!(matches!(
        no_index.list(&ListRequest::folder("")),
        Err(DriveError::Unsupported(_))
    ));
    assert_eq!(no_index.is_encrypted(), None, "tried again on the next call");
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
    assert_eq!(drive.is_encrypted(), Some(true), "this device keeps the key");
    assert!(mem.object("diary.txt").is_none());
}

#[test]
fn reopening_takes_a_drive_encrypted_since() {
    let mem = Arc::new(MemBucket::new());
    let keyring = Arc::new(MemoryKeyring::new());
    let drive = AutoEncrypted::new(mem.clone(), "d_1", keyring.clone(), provider());
    drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(drive.is_encrypted(), Some(false));
    let kdf = RecoveryKdf::with_cost(64, 1, 1).unwrap();
    setup_new_drive(drive.bucket().as_ref(), keyring.as_ref(), "d_1", kdf).unwrap();
    assert_eq!(drive.is_encrypted(), Some(false), "until it is reopened");
    drive.reopen();
    drive.put("new.txt", b"sealed").unwrap();
    assert_eq!(drive.is_encrypted(), Some(true));
    assert!(mem.object("new.txt").is_none());
}
