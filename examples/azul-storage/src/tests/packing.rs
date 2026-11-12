//! Small files packed together on an encrypted drive: many small files share one AZL1 object (a
//! pack), each read back with one ranged GET; a crash between a pack's upload and its index
//! commit leaves nothing the index names without its bytes; repacking keeps every file readable
//! and retires the old packs for the garbage collection's grace period.

use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Barrier, Mutex,
    },
    thread,
};

use super::{mem_bucket::MemBucket, TempDir};
use crate::{
    crypto::{random_bytes, DriveKey, ObjectId, ShareKey},
    encrypted::{
        read_shared, EncryptedDrive, IndexChange, IndexEntry, IndexPage, MemoryIndex, NameIndex,
    },
    meta::{pointer, Released},
    packing::{
        data_shards_of, pack_object_len, plan_layout, segment_span, PackOptions, JOURNAL_STALE_SECS,
        PACK_FILL, PACK_MAX, PACK_THRESHOLD,
    },
    time::now_unix,
    BatchPut, ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo, Precondition,
};

fn noise(len: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; len];
    random_bytes(&mut bytes).unwrap();
    bytes
}

fn text(len: usize, seed: usize) -> Vec<u8> {
    format!("Note {seed}: the meeting moved to Thursday, bring the slides and the numbers. ")
        .into_bytes()
        .into_iter()
        .cycle()
        .take(len)
        .collect()
}

/// A drive that packs small files, over a bucket and an index in memory, the journal in `dir`.
fn packing_drive(dir: Option<&TempDir>) -> EncryptedDrive<Arc<MemBucket>> {
    packing_drive_over(Arc::new(MemBucket::new()), Arc::new(MemoryIndex::new()), dir)
}

fn packing_drive_over(
    bucket: Arc<MemBucket>,
    index: Arc<dyn NameIndex>,
    dir: Option<&TempDir>,
) -> EncryptedDrive<Arc<MemBucket>> {
    let mut options = PackOptions::default();
    if let Some(dir) = dir {
        options = options.with_journal(dir.path().join("packs"));
    }
    EncryptedDrive::new(bucket, DriveKey::generate().unwrap(), index).with_packing(options)
}

/// The data objects in the bucket (packs and files with objects of their own).
fn data_objects(bucket: &MemBucket) -> Vec<String> {
    bucket
        .keys()
        .into_iter()
        .filter(|key| ObjectId::from_bucket_key(key).is_some())
        .collect()
}

/// Writes `files` in one batch; every write must succeed.
fn put_all<D: Drive>(drive: &EncryptedDrive<D>, files: &[(String, Vec<u8>)]) {
    let items: Vec<BatchPut<'_>> = files
        .iter()
        .map(|(path, bytes)| BatchPut::new(path, bytes))
        .collect();
    for (result, (path, _)) in drive.put_many(&items).into_iter().zip(files) {
        result.unwrap_or_else(|e| panic!("{path}: {e}"));
    }
}

/// Every path the index names whose bytes are not in the bucket: what must never exist.
fn dangling(drive: &EncryptedDrive<Arc<MemBucket>>) -> Vec<String> {
    let mut out = Vec::new();
    for (path, entry) in drive.all_entries("").unwrap() {
        let Some(object) = entry.object else {
            continue;
        };
        if drive.inner().object(&object.bucket_object().bucket_key()).is_none() {
            out.push(path);
        }
    }
    out
}

fn small_files(count: usize, size: usize) -> Vec<(String, Vec<u8>)> {
    (0..count)
        .map(|i| (format!("notes/{i:04}.txt"), text(size + i % 7, i)))
        .collect()
}

/// Files that do not compress: their stored bytes are their size.
fn noise_files(count: usize, size: usize) -> Vec<(String, Vec<u8>)> {
    (0..count)
        .map(|i| (format!("photos/{i:04}.jpg"), noise(size)))
        .collect()
}

#[test]
fn small_files_written_together_share_one_pack_and_each_reads_back() {
    let drive = packing_drive(None);
    let files = small_files(50, 3_000);
    put_all(&drive, &files);
    assert_eq!(data_objects(drive.inner()).len(), 1, "fifty small files, one pack");
    for (path, bytes) in &files {
        assert_eq!(&drive.get(path).unwrap(), bytes, "{path}");
        let entry = drive.entry(path).unwrap();
        let object = entry.object.unwrap();
        let packed = object.packed.as_ref().expect("a small file is packed");
        assert_eq!(object.blake3, *blake3::hash(bytes).as_bytes(), "its own BLAKE3");
        assert_eq!(drive.head(path).unwrap().size, bytes.len() as u64);
        assert!(packed.pack.object_len <= PACK_MAX);
    }
}

#[test]
fn a_ten_kilobyte_packed_file_is_read_with_one_range_request() {
    let drive = packing_drive(None);
    let files: Vec<(String, Vec<u8>)> = (0..20)
        .map(|i| (format!("photos/{i}.jpg"), noise(10_000)))
        .collect();
    put_all(&drive, &files);
    let packs = data_objects(drive.inner());
    assert_eq!(packs.len(), 1);
    let (gets, ranges) = (drive.inner().gets(), drive.inner().ranges().len());
    assert_eq!(drive.get("photos/7.jpg").unwrap(), files[7].1);
    assert_eq!(drive.inner().gets(), gets, "no GET of the whole pack");
    let asked = drive.inner().ranges();
    assert_eq!(asked.len(), ranges + 1, "one ranged GET");
    let (key, range) = &asked[ranges];
    assert_eq!(key, &packs[0]);
    let len = range.end.unwrap() - range.start + 1;
    // The member's own segments: 10,001 bytes (codec byte and data) cover at most four.
    assert!(len <= 4 * 4113, "{len} bytes asked for a 10 KB file");
    // A part of a packed file is the same one request.
    let part = drive.get_range("photos/3.jpg", ByteRange::new(100, Some(199))).unwrap();
    assert_eq!(part, files[3].1[100..200]);
    assert_eq!(drive.inner().ranges().len(), ranges + 2);
}

#[test]
fn a_file_at_the_threshold_keeps_an_object_of_its_own() {
    let drive = packing_drive(None);
    let big = noise(PACK_THRESHOLD as usize);
    let small = noise(PACK_THRESHOLD as usize - 1);
    put_all(
        &drive,
        &[
            (String::from("big.bin"), big.clone()),
            (String::from("small.bin"), small.clone()),
        ],
    );
    let big_object = drive.entry("big.bin").unwrap().object.unwrap();
    assert!(big_object.packed.is_none(), "512 KiB and more: its own object");
    assert!(drive.entry("small.bin").unwrap().object.unwrap().packed.is_some());
    assert_eq!(drive.get("big.bin").unwrap(), big);
    assert_eq!(drive.get("small.bin").unwrap(), small);
    // A single write packs too.
    drive.put("one.txt", b"one small file").unwrap();
    assert!(drive.entry("one.txt").unwrap().object.unwrap().packed.is_some());
    assert_eq!(drive.get("one.txt").unwrap(), b"one small file");
}

#[test]
fn a_plain_bucket_keeps_one_object_per_file() {
    let bucket = MemBucket::new();
    let files = small_files(5, 100);
    let items: Vec<BatchPut<'_>> = files
        .iter()
        .map(|(path, bytes)| BatchPut::new(path, bytes))
        .collect();
    for result in bucket.put_many(&items) {
        result.unwrap();
    }
    assert_eq!(bucket.keys().len(), 5, "no packs outside an encrypted drive");
    assert_eq!(bucket.puts(), 5);
}

#[test]
fn packed_files_have_entity_tags_of_their_own() {
    let drive = packing_drive(None);
    put_all(&drive, &small_files(2, 500));
    let a = drive.head("notes/0000.txt").unwrap().etag.unwrap();
    let b = drive.head("notes/0001.txt").unwrap().etag.unwrap();
    assert_ne!(a, b, "two files in one pack are two versions");
    let refused = drive.put_if("notes/0001.txt", b"new", &Precondition::Matches(a.clone()));
    assert!(matches!(refused, Err(DriveError::Conflict { .. })), "{refused:?}");
    let written = drive.put_if("notes/0000.txt", b"new", &Precondition::Matches(a));
    assert!(written.is_ok(), "{written:?}");
    assert_eq!(drive.get("notes/0000.txt").unwrap(), b"new");
}

#[test]
fn a_failed_condition_in_a_batch_refuses_that_file_only() {
    let drive = packing_drive(None);
    drive.put("taken.txt", b"first").unwrap();
    let absent = Precondition::Absent;
    let items = [
        BatchPut::new("taken.txt", b"second").when(&absent),
        BatchPut::new("free.txt", b"free").when(&absent),
        BatchPut::new("any.txt", b"any"),
    ];
    let results = drive.put_many(&items);
    assert!(matches!(results[0], Err(DriveError::Conflict { .. })), "{:?}", results[0]);
    assert!(results[1].is_ok() && results[2].is_ok(), "{results:?}");
    assert_eq!(drive.get("taken.txt").unwrap(), b"first");
    assert_eq!(drive.get("free.txt").unwrap(), b"free");
    assert_eq!(drive.get("any.txt").unwrap(), b"any");
}

#[test]
fn an_update_goes_into_a_new_pack_and_never_rewrites_the_old_one() {
    let drive = packing_drive(None);
    put_all(&drive, &small_files(10, 1_000));
    let packs = data_objects(drive.inner());
    assert_eq!(packs.len(), 1);
    let before = drive.inner().object(&packs[0]).unwrap();
    drive.put("notes/0003.txt", b"edited").unwrap();
    drive.delete("notes/0004.txt").unwrap();
    assert_eq!(drive.inner().object(&packs[0]).unwrap(), before, "the pack is never rewritten");
    assert_eq!(data_objects(drive.inner()).len(), 2, "the new version in a new pack");
    let edited = drive.entry("notes/0003.txt").unwrap().object.unwrap();
    assert_ne!(edited.bucket_object().bucket_key(), packs[0]);
    assert_eq!(drive.get("notes/0003.txt").unwrap(), b"edited");
    assert_eq!(drive.get("notes/0005.txt").unwrap(), small_files(10, 1_000)[5].1);
}

/// A bucket whose first data object (a pack's upload) waits until `ready` says so.
#[derive(Default)]
struct FirstPackWaits {
    inner: MemBucket,
    ready: std::sync::OnceLock<Box<dyn Fn() -> bool + Send + Sync>>,
    waited: std::sync::atomic::AtomicBool,
}

impl Drive for FirstPackWaits {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.inner.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.inner.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.inner.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        if ObjectId::from_bucket_key(key).is_some() && !self.waited.swap(true, Ordering::SeqCst) {
            if let Some(ready) = self.ready.get() {
                while !ready() {
                    thread::yield_now();
                }
            }
        }
        self.inner.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.inner.delete(key)
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.inner.head(key)
    }
}

#[test]
fn writers_at_the_same_time_share_packs() {
    let threads = 8;
    let rounds = 10;
    let drive = Arc::new(
        EncryptedDrive::new(
            FirstPackWaits::default(),
            DriveKey::generate().unwrap(),
            Arc::new(MemoryIndex::new()),
        )
        .with_packing(PackOptions::default()),
    );
    // A writer alone seals at once, and a bucket in memory is so fast that writers released
    // together may still each be alone. So the first pack's upload waits until the round's
    // other writers queued their files: they overlap for certain, and share the next pack.
    let weak = Arc::downgrade(&drive);
    let _ = drive.inner().ready.set(Box::new(move || {
        weak.upgrade()
            .is_some_and(|drive| drive.files_waiting_for_a_pack() >= threads - 1)
    }));
    let barrier = Arc::new(Barrier::new(threads));
    let workers: Vec<_> = (0..threads)
        .map(|t| {
            let drive = Arc::clone(&drive);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                for round in 0..rounds {
                    barrier.wait();
                    let path = format!("t{t}/r{round}.txt");
                    drive.put(&path, &text(2_000, t * 100 + round)).unwrap();
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    let objects = data_objects(&drive.inner().inner).len();
    assert!(objects < threads * rounds, "{objects} objects for {} files", threads * rounds);
    for t in 0..threads {
        for round in 0..rounds {
            let path = format!("t{t}/r{round}.txt");
            assert_eq!(drive.get(&path).unwrap(), text(2_000, t * 100 + round), "{path}");
        }
    }
}

#[test]
fn a_packed_pointer_round_trips_through_its_text() {
    let drive = packing_drive(None);
    put_all(&drive, &small_files(3, 700));
    let entry = drive.entry("notes/0001.txt").unwrap();
    let blob = pointer::encode(&entry);
    assert!(blob.starts_with(b"azlin-pointer 2\n"), "{}", String::from_utf8_lossy(&blob));
    assert_eq!(pointer::decode(&blob).unwrap(), entry);
    // A file with an object of its own keeps the first format.
    drive.put("big.bin", &noise(PACK_THRESHOLD as usize + 1)).unwrap();
    let own = drive.entry("big.bin").unwrap();
    let blob = pointer::encode(&own);
    assert!(blob.starts_with(b"azlin-pointer 1\n"));
    assert_eq!(pointer::decode(&blob).unwrap(), own);
}

#[test]
fn no_member_of_a_full_pack_crosses_a_data_shard() {
    let mut seed = 0x2545_f491_u64;
    let mut next = |below: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % below
    };
    for largest in [30_000, 200_000] {
        let mut lengths = Vec::new();
        let mut total = 0;
        loop {
            let len = 1 + next(largest);
            if total + len > PACK_FILL {
                break;
            }
            total += len;
            lengths.push(len);
        }
        let layout = plan_layout(&lengths);
        assert!(layout.aligned, "a full pack of members up to {largest} bytes is aligned");
        let object_len = pack_object_len(layout.body_len);
        assert!(object_len <= PACK_MAX);
        assert!(layout.body_len - total <= total / 16, "padding stays small");
        let mut spans: Vec<(u64, u64)> = Vec::new();
        for (len, offset) in lengths.iter().zip(&layout.offsets) {
            assert!(offset + len <= layout.body_len);
            spans.push((*offset, offset + len));
            let (start, sealed) = segment_span(layout.body_len, *offset, *len);
            let shards = data_shards_of(object_len, start, sealed);
            assert_eq!(shards.len(), 1, "a member of {len} bytes at {offset} lies in one shard");
        }
        spans.sort_unstable();
        assert!(spans.windows(2).all(|w| w[0].1 <= w[1].0), "members never overlap");
    }
}

/// An index that fails its next commit: before anything changes (the crash came before the
/// commit), or after the change landed (the crash came before the journal was cleared).
struct FailingIndex {
    inner: MemoryIndex,
    /// 0: works; 1: the next apply fails and changes nothing; 2: the next apply changes the
    /// index, then fails.
    mode: AtomicU32,
}

impl NameIndex for FailingIndex {
    fn get(&self, path: &str) -> Result<Option<IndexEntry>, DriveError> {
        self.inner.get(path)
    }
    fn list(&self, request: &ListRequest) -> Result<IndexPage, DriveError> {
        self.inner.list(request)
    }
    fn apply(&self, changes: Vec<IndexChange>) -> Result<Released, DriveError> {
        match self.mode.swap(0, Ordering::SeqCst) {
            1 => Err(DriveError::Transport(String::from("the computer went off"))),
            2 => {
                self.inner.apply(changes)?;
                Err(DriveError::Transport(String::from("the computer went off")))
            }
            _ => self.inner.apply(changes),
        }
    }
}

#[test]
fn a_crash_between_pack_upload_and_index_commit_leaves_no_dangling_reference() {
    let dir = TempDir::new("pack-journal");
    let bucket = Arc::new(MemBucket::new());
    let index = Arc::new(FailingIndex {
        inner: MemoryIndex::new(),
        mode: AtomicU32::new(0),
    });
    let key = DriveKey::generate().unwrap();
    let options = PackOptions::default().with_journal(dir.path().join("packs"));
    let drive = EncryptedDrive::new(Arc::clone(&bucket), key.clone(), index.clone())
        .with_packing(options.clone());
    put_all(&drive, &small_files(5, 400));
    assert_eq!(data_objects(&*bucket).len(), 1);

    // The upload lands, the commit never does.
    index.mode.store(1, Ordering::SeqCst);
    let lost = small_files(30, 900)[10..].to_vec();
    let items: Vec<BatchPut<'_>> = lost.iter().map(|(p, b)| BatchPut::new(p, b)).collect();
    assert!(drive.put_many(&items).iter().all(Result::is_err));
    assert_eq!(data_objects(&*bucket).len(), 2, "the pack went up");
    for (path, _) in &lost {
        assert!(matches!(drive.get(path), Err(DriveError::NotFound { .. })), "{path}");
    }
    assert!(dangling(&drive).is_empty());

    // The commit lands, the journal is not cleared.
    index.mode.store(2, Ordering::SeqCst);
    let landed = vec![(String::from("landed.txt"), text(800, 1))];
    let items: Vec<BatchPut<'_>> = landed.iter().map(|(p, b)| BatchPut::new(p, b)).collect();
    assert!(drive.put_many(&items)[0].is_err());
    assert_eq!(data_objects(&*bucket).len(), 3);

    // The next start: a pack in flight for a few minutes is left alone (another writer may
    // still commit it); a stale one is cleared up.
    let restarted =
        EncryptedDrive::new(Arc::clone(&bucket), key, index.clone()).with_packing(options);
    assert_eq!(restarted.recover_packs(now_unix()).unwrap(), 0);
    assert_eq!(data_objects(&*bucket).len(), 3);
    let later = now_unix() + JOURNAL_STALE_SECS + 1;
    assert_eq!(restarted.recover_packs(later).unwrap(), 2, "two packs were in flight");
    assert_eq!(data_objects(&*bucket).len(), 2, "the pack nothing names left the bucket");
    assert_eq!(restarted.get("landed.txt").unwrap(), text(800, 1), "the committed one stays");
    for (path, bytes) in small_files(5, 400) {
        assert_eq!(restarted.get(&path).unwrap(), bytes);
    }
    assert!(dangling(&restarted).is_empty());
    assert_eq!(restarted.recover_packs(later).unwrap(), 0, "nothing left in flight");
}

#[test]
fn repacking_keeps_every_file_readable_and_retires_the_old_packs() {
    let mut options = PackOptions::default();
    options.retired_grace_secs = 100;
    let drive = EncryptedDrive::new(
        Arc::new(MemBucket::new()),
        DriveKey::generate().unwrap(),
        Arc::new(MemoryIndex::new()),
    )
    .with_packing(options);
    let files = noise_files(400, 12_000);
    put_all(&drive, &files);
    let old_packs = data_objects(drive.inner());
    assert!(old_packs.len() >= 2, "{} packs", old_packs.len());
    // Three of every four files go: every pack falls below half full.
    for (i, (path, _)) in files.iter().enumerate() {
        if i % 4 != 0 {
            drive.delete(path).unwrap();
        }
    }
    let now = now_unix();
    let report = drive.repack(now).unwrap();
    assert_eq!(report.files_moved, 100);
    assert_eq!(report.packs_read, old_packs.len());
    assert!(report.packs_written >= 1 && report.packs_written < old_packs.len());
    assert!(report.bytes_after < report.bytes_before);
    for (i, (path, bytes)) in files.iter().enumerate() {
        if i % 4 == 0 {
            assert_eq!(&drive.get(path).unwrap(), bytes, "{path}");
            let object = drive.entry(path).unwrap().object.unwrap();
            assert!(!old_packs.contains(&object.bucket_object().bucket_key()));
        } else {
            assert!(matches!(drive.get(path), Err(DriveError::NotFound { .. })));
        }
    }
    assert!(dangling(&drive).is_empty());
    // The old packs wait for the grace period, then go.
    let retired: Vec<String> = drive
        .retired_packs()
        .into_iter()
        .map(|(id, _)| id.bucket_key())
        .collect();
    for pack in &old_packs {
        assert!(retired.contains(pack), "{pack} is retired");
        assert!(drive.inner().object(pack).is_some(), "{pack} stays for the grace period");
    }
    assert_eq!(drive.collect_retired(now + 50).unwrap(), 0);
    assert_eq!(drive.collect_retired(now + 101).unwrap(), old_packs.len());
    for pack in &old_packs {
        assert!(drive.inner().object(pack).is_none(), "{pack} is gone");
    }
    assert!(drive.retired_packs().is_empty());
    for (path, bytes) in files.iter().step_by(4) {
        assert_eq!(&drive.get(path).unwrap(), bytes);
    }
    // Nothing more to do: a second round reads nothing.
    let again = drive.repack(now + 200).unwrap();
    assert_eq!((again.packs_read, again.files_moved), (0, 0));
}

#[test]
fn a_copy_of_a_packed_file_moves_with_it() {
    let drive = packing_drive(None);
    let files = small_files(40, 5_000);
    put_all(&drive, &files);
    drive.copy("notes/0000.txt", "copies/first.txt").unwrap();
    for (path, _) in files.iter().skip(1) {
        drive.delete(path).unwrap();
    }
    let report = drive.repack(now_unix()).unwrap();
    assert_eq!(report.files_moved, 1, "one version, named twice");
    let a = drive.entry("notes/0000.txt").unwrap().object.unwrap();
    let b = drive.entry("copies/first.txt").unwrap().object.unwrap();
    assert_eq!(a, b, "both names follow the move");
    assert_eq!(drive.get("copies/first.txt").unwrap(), files[0].1);
}

#[test]
fn the_garbage_collection_spares_a_retired_pack_the_index_names_again() {
    let mut options = PackOptions::default();
    options.retired_grace_secs = 0;
    let index = Arc::new(MemoryIndex::new());
    let drive = EncryptedDrive::new(
        Arc::new(MemBucket::new()),
        DriveKey::generate().unwrap(),
        index.clone(),
    )
    .with_packing(options);
    put_all(&drive, &small_files(10, 2_000));
    let old_entry = drive.entry("notes/0000.txt").unwrap();
    for i in 1..10 {
        drive.delete(&format!("notes/{i:04}.txt")).unwrap();
    }
    let now = now_unix();
    drive.repack(now).unwrap();
    let old_pack = old_entry.object.as_ref().unwrap().bucket_object();
    // A restore from the history names the old version again.
    index
        .apply(vec![IndexChange::Put {
            path: String::from("restored.txt"),
            entry: old_entry,
            expect: crate::encrypted::Expect::Absent,
        }])
        .unwrap();
    assert_eq!(drive.collect_retired(now + 1).unwrap(), 0);
    assert!(drive.inner().object(&old_pack.bucket_key()).is_some());
    assert_eq!(drive.get("restored.txt").unwrap(), small_files(1, 2_000)[0].1);
    assert!(drive.retired_packs().is_empty(), "named again: no longer retired");
}

#[test]
fn a_pack_read_often_is_fetched_whole_once() {
    let drive = packing_drive(None);
    let files = small_files(30, 4_000);
    put_all(&drive, &files);
    let (gets, ranges) = (drive.inner().gets(), drive.inner().ranges().len());
    for (path, bytes) in &files {
        assert_eq!(&drive.get(path).unwrap(), bytes);
    }
    let asked = (
        drive.inner().gets() - gets,
        drive.inner().ranges().len() - ranges,
    );
    assert_eq!(asked, (1, 2), "two ranged reads, then the hot pack once whole");
}

/// A bucket whose ranged GETs answer only the first half of what was asked, `cuts` times.
struct CutShort {
    inner: MemBucket,
    cuts: AtomicU32,
}

impl Drive for CutShort {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.inner.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.inner.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        let mut bytes = self.inner.get_range(key, range)?;
        let cut = self
            .cuts
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            .is_ok();
        if cut {
            bytes.truncate(bytes.len() / 2);
        }
        Ok(bytes)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.inner.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.inner.delete(key)
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.inner.head(key)
    }
}

#[test]
fn a_range_read_cut_short_resumes_where_it_stopped() {
    let bucket = CutShort {
        inner: MemBucket::new(),
        cuts: AtomicU32::new(0),
    };
    let drive = EncryptedDrive::new(bucket, DriveKey::generate().unwrap(), Arc::new(MemoryIndex::new()))
        .with_packing(PackOptions::default());
    let files: Vec<(String, Vec<u8>)> =
        (0..4).map(|i| (format!("{i}.bin"), noise(20_000))).collect();
    put_all(&drive, &files);
    drive.inner().cuts.store(1, Ordering::SeqCst);
    let before = drive.inner().inner.ranges().len();
    assert_eq!(drive.get("2.bin").unwrap(), files[2].1);
    let asked = drive.inner().inner.ranges();
    assert_eq!(asked.len(), before + 2, "the rest asked for once more");
    let (first, rest) = (asked[before].1, asked[before + 1].1);
    assert!(rest.start > first.start && rest.end == first.end, "{first:?} then {rest:?}");
}

#[test]
fn the_totals_count_each_pack_once() {
    let drive = packing_drive(None);
    let files = small_files(20, 3_000);
    put_all(&drive, &files);
    let pack = data_objects(drive.inner()).remove(0);
    let stored = drive.inner().object(&pack).unwrap().len() as u64;
    let totals = drive.totals().unwrap();
    assert_eq!(totals.files, 20);
    assert_eq!(
        totals.original_bytes,
        files.iter().map(|(_, b)| b.len() as u64).sum::<u64>()
    );
    assert_eq!(totals.stored_bytes, stored, "the pack's bytes, once");
}

#[test]
fn a_shared_packed_file_moves_into_an_object_of_its_own_first() {
    let drive = packing_drive(None);
    let files = small_files(5, 2_000);
    put_all(&drive, &files);
    let share = ShareKey::generate().unwrap();
    let shared = drive.share_file("notes/0002.txt", &share).unwrap();
    let entry = drive.entry("notes/0002.txt").unwrap().object.unwrap();
    assert!(entry.packed.is_none(), "a share never carries a pack's key");
    assert_eq!(shared.object, entry.id);
    assert_eq!(read_shared(drive.inner(), &shared, &share).unwrap(), files[2].1);
    assert_eq!(drive.get("notes/0002.txt").unwrap(), files[2].1);
    for (path, bytes) in &files {
        assert_eq!(&drive.get(path).unwrap(), bytes);
    }
    assert!(drive.entry("notes/0001.txt").unwrap().object.unwrap().packed.is_some());
}

#[test]
fn a_key_rotation_rewraps_packed_files_and_the_reencryption_repacks_them() {
    let bucket = Arc::new(MemBucket::new());
    let index: Arc<dyn NameIndex> = Arc::new(MemoryIndex::new());
    let old = DriveKey::generate().unwrap();
    let new = DriveKey::generate().unwrap();
    let before = EncryptedDrive::new(Arc::clone(&bucket), old.clone(), Arc::clone(&index))
        .with_packing(PackOptions::default());
    let files = small_files(12, 1_500);
    put_all(&before, &files);
    let old_pack = data_objects(&*bucket).remove(0);
    crate::rotation::rewrap_index(index.as_ref(), &old, &new).unwrap();
    let after = EncryptedDrive::new(Arc::clone(&bucket), new.clone(), Arc::clone(&index))
        .with_packing(PackOptions::default());
    for (path, bytes) in &files {
        assert_eq!(&after.get(path).unwrap(), bytes, "{path} opens with the new key");
    }
    let mut state = crate::rotation::ReencryptState::new(now_unix() + 10);
    let done =
        super::asked::reencrypt_pass_of_parts(&new, &after, &mut state, &mut |_| Ok(()), &|| false)
            .unwrap();
    assert!(done);
    for (path, bytes) in &files {
        let object = after.entry(path).unwrap().object.unwrap();
        assert_eq!(object.wrapped_key.key_id, new.id());
        assert_ne!(object.bucket_object().bucket_key(), old_pack, "{path} left the old pack");
        assert_eq!(&after.get(path).unwrap(), bytes);
    }
}

#[test]
fn the_drive_index_provider_packs_when_asked_and_keeps_its_journal_in_the_drive_cache() {
    use crate::{
        encrypted::open_encrypted,
        keyring::MemoryKeyring,
        meta::{IndexProfile, MetaIndexProvider},
    };
    let dir = TempDir::new("pack-provider");
    let bucket: Arc<dyn Drive> = Arc::new(crate::meta::MemoryBucket::new());
    let keyring = MemoryKeyring::new();
    crate::crypto::device::setup_new_drive(
        bucket.as_ref(),
        &keyring,
        "d_pack",
        crate::crypto::keys::RecoveryKdf::with_cost(64, 1, 1).unwrap(),
    )
    .unwrap();
    let provider = MetaIndexProvider::new(IndexProfile {
        cache_root: Some(dir.path().to_path_buf()),
        packing: true,
        ..IndexProfile::in_memory("Laptop")
    });
    let drive = open_encrypted(Arc::clone(&bucket), &keyring, "d_pack", &provider).unwrap();
    let journal = provider.drive_cache_dir("d_pack").unwrap().join("packs");
    assert_eq!(drive.packing().unwrap().journal.as_deref(), Some(journal.as_path()));
    let files = small_files(30, 1_000);
    put_all(&drive, &files);
    let packs: Vec<String> = crate::ops::list_all(bucket.as_ref(), "data/")
        .unwrap()
        .into_iter()
        .map(|o| o.key)
        .collect();
    assert_eq!(packs.len(), 1, "{packs:?}");
    for (path, bytes) in &files {
        assert_eq!(&drive.get(path).unwrap(), bytes);
    }
    // Without the switch, a drive writes one object per file as before.
    let plain = MetaIndexProvider::new(IndexProfile::in_memory("Laptop"));
    let unpacked = open_encrypted(Arc::clone(&bucket), &keyring, "d_pack", &plain).unwrap();
    assert!(unpacked.packing().is_none());
    assert_eq!(&unpacked.get("notes/0003.txt").unwrap(), &files[3].1, "packed files read anyway");
}

/// An index that keeps the history (as the drive's metadata repository does): nothing it drops
/// is released, so a pack whose every file went stays in the bucket until the packs' upkeep.
struct KeepsHistory(MemoryIndex);

impl NameIndex for KeepsHistory {
    fn get(&self, path: &str) -> Result<Option<IndexEntry>, DriveError> {
        self.0.get(path)
    }
    fn list(&self, request: &ListRequest) -> Result<IndexPage, DriveError> {
        self.0.list(request)
    }
    fn apply(&self, changes: Vec<IndexChange>) -> Result<Released, DriveError> {
        // What it releases is held by its history: nothing for the drive to delete.
        self.0
            .apply(changes)
            .map(|released| Released::held(released.delete_now))
    }
}

#[test]
fn a_pack_whose_every_file_went_is_retired_and_collected_after_a_restart_too() {
    let dir = TempDir::new("pack-emptied");
    let bucket = Arc::new(MemBucket::new());
    let index: Arc<dyn NameIndex> = Arc::new(KeepsHistory(MemoryIndex::new()));
    let key = DriveKey::generate().unwrap();
    let mut options = PackOptions::default().with_journal(dir.path().join("packs"));
    options.retired_grace_secs = 10;
    let drive = EncryptedDrive::new(Arc::clone(&bucket), key.clone(), Arc::clone(&index))
        .with_packing(options.clone());
    let gone = small_files(10, 800);
    let kept: Vec<(String, Vec<u8>)> = noise_files(10, 800);
    put_all(&drive, &gone);
    put_all(&drive, &kept);
    let emptied = drive
        .entry(&gone[0].0)
        .unwrap()
        .object
        .unwrap()
        .bucket_object()
        .bucket_key();
    for (path, _) in &gone {
        drive.delete(path).unwrap();
    }
    assert!(bucket.object(&emptied).is_some(), "the history keeps it for now");
    drop(drive);

    // Another start of the app: the journal remembers the packs this device wrote.
    let drive = EncryptedDrive::new(Arc::clone(&bucket), key, index).with_packing(options);
    let now = now_unix();
    let report = drive.repack(now).unwrap();
    assert_eq!(report.packs_emptied, 1, "{report:?}");
    assert_eq!(report.files_moved, 0, "{report:?}");
    assert_eq!(drive.collect_retired(now + 5).unwrap(), 0);
    assert_eq!(drive.collect_retired(now + 11).unwrap(), 1);
    assert!(bucket.object(&emptied).is_none());
    for (path, bytes) in &kept {
        assert_eq!(&drive.get(path).unwrap(), bytes);
    }
}

#[test]
fn a_packed_ten_kilobyte_file_is_one_read_of_one_data_shard() {
    let drive = packing_drive(None);
    // Enough small files for a pack of a few MiB: its files are placed on the data shards.
    let files = noise_files(300, 10_000);
    put_all(&drive, &files);
    let mut seen = 0;
    for (path, _) in &files {
        let object = drive.entry(path).unwrap().object.unwrap();
        let packed = object.packed.unwrap();
        if packed.pack.object_len < crate::packing::ALIGN_FROM {
            continue;
        }
        seen += 1;
        let (start, len) = packed.span();
        let reads = crate::shards::shard_reads(packed.pack.object_len, start, len);
        assert_eq!(reads.len(), 1, "{path}: {reads:?}");
        assert_eq!(reads[0].len, len);
    }
    assert!(seen >= 250, "{seen} files in packs of a MiB and more");
}

#[test]
fn the_pack_summary_counts_packed_files_packs_and_files_with_objects_of_their_own() {
    let drive = packing_drive(None);
    let files = small_files(20, 1_500);
    put_all(&drive, &files);
    drive.put("big.bin", &noise(PACK_THRESHOLD as usize)).unwrap();
    drive.copy("notes/0001.txt", "copies/one.txt").unwrap();
    let summary = drive.pack_summary().unwrap();
    assert_eq!(summary.packed_files, 21, "a copy names its version twice: {summary:?}");
    assert_eq!(summary.packs, 1);
    assert_eq!(summary.own_objects, 1);
    let pack = data_objects(drive.inner())
        .into_iter()
        .find(|key| drive.inner().object(key).unwrap().len() < PACK_THRESHOLD as usize)
        .unwrap();
    assert_eq!(summary.pack_bytes, drive.inner().object(&pack).unwrap().len() as u64);
    assert!(summary.live_bytes > 0 && summary.live_bytes <= summary.pack_bytes, "{summary:?}");
}

// ==== Members compressed on their own (azlin-compress's member records) ====

const BASELINE_JPEG: &[u8] = include_bytes!("../../../azlin-compress/tests/fixtures/baseline.jpg");

/// The record of the packed file `path`: its codec byte, then its data, from its pack.
fn record_of(drive: &EncryptedDrive<Arc<MemBucket>>, path: &str) -> Vec<u8> {
    let object = drive.entry(path).unwrap().object.unwrap();
    let packed = object.packed.clone().expect("packed");
    let pack = packed.pack.id;
    let key = drive.drive_key.unwrap_file_key(&object.wrapped_key, &pack).unwrap();
    let bytes = drive.inner().get(&pack.bucket_key()).unwrap();
    let plain = crate::crypto::azl1::decrypt(&bytes, &pack, &key).unwrap();
    plain[packed.offset as usize..(packed.offset + packed.length) as usize].to_vec()
}

#[test]
fn a_small_json_file_is_packed_with_a_shipped_dictionary_and_a_small_jpeg_through_lepton() {
    let drive = packing_drive(None);
    // A settings file: the shape the JSON dictionary is for (a long array of records goes to
    // brotli, which beats the dictionary there).
    let json = super::compress_inputs::settings_json();
    assert!((json.len() as u64) < PACK_THRESHOLD);
    drive.put("settings/app.json", &json).unwrap();
    drive.put("photos/thumb.jpg", BASELINE_JPEG).unwrap();
    for (path, plain, codec) in [("settings/app.json", &json[..], 4u8), ("photos/thumb.jpg", BASELINE_JPEG, 5)] {
        let record = record_of(&drive, path);
        assert_eq!(record[0], codec, "{path}");
        assert!(record.len() < plain.len(), "{path}: {} of {}", record.len(), plain.len());
        let entry = drive.entry(path).unwrap();
        assert!(entry.object.unwrap().compressed, "{path}");
        assert_eq!(drive.get(path).unwrap(), plain, "{path}");
    }
}

#[test]
fn a_record_of_a_codec_this_build_does_not_write_is_refused_by_name() {
    let hash = *blake3::hash(b"hello").as_bytes();
    for codec in [3u8, 6, 200] {
        let record = [codec, b'h', b'e', b'l', b'l', b'o'];
        assert!(
            matches!(
                crate::packing::decode_record(&record, 5, &hash, "a.txt"),
                Err(DriveError::Unsupported(_))
            ),
            "codec {codec}"
        );
    }
    // A stored record decodes; one that would decode past its entry's size does not.
    assert_eq!(
        crate::packing::decode_record(b"\x00hello", 5, &hash, "a.txt").unwrap(),
        b"hello"
    );
    assert!(matches!(
        crate::packing::decode_record(b"\x00hello!", 5, &hash, "a.txt"),
        Err(DriveError::Corrupt { .. })
    ));
    assert!(matches!(
        crate::packing::decode_record(b"\x05not a container", 5, &hash, "a.txt"),
        Err(DriveError::Corrupt { .. })
    ));
}

/// A bucket with room for `room` bytes of data objects: a bigger one is refused as a full
/// drive refuses it (507), and nothing of it is stored.
struct Room {
    inner: MemBucket,
    room: u64,
}

impl Room {
    fn data_bytes(&self) -> u64 {
        data_objects(&self.inner)
            .iter()
            .filter_map(|key| self.inner.object(key))
            .map(|bytes| bytes.len() as u64)
            .sum()
    }
}

impl Drive for Room {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.inner.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.inner.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.inner.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        if ObjectId::from_bucket_key(key).is_some()
            && self.data_bytes() + bytes.len() as u64 > self.room
        {
            return Err(DriveError::Service(crate::ServiceError {
                status: 507,
                code: String::from("QuotaExceeded"),
                ..crate::ServiceError::default()
            }));
        }
        self.inner.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.inner.delete(key)
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.inner.head(key)
    }
}

#[test]
fn a_full_drive_refuses_only_the_packed_files_that_do_not_fit_and_the_others_go_up() {
    // Room for two of the three files, not for one pack of all three.
    let one = 100_000;
    let bucket = Arc::new(Room {
        inner: MemBucket::new(),
        room: 250_000,
    });
    let drive = EncryptedDrive::new(
        bucket.clone(),
        DriveKey::generate().unwrap(),
        Arc::new(MemoryIndex::new()),
    )
    .with_packing(PackOptions::default());
    let files = noise_files(3, one);
    let items: Vec<BatchPut<'_>> = files
        .iter()
        .map(|(path, bytes)| BatchPut::new(path, bytes))
        .collect();
    let results = drive.put_many(&items);
    let sent: Vec<bool> = results.iter().map(Result::is_ok).collect();
    assert_eq!(sent, [true, true, false], "{results:?}");
    assert!(
        matches!(&results[2], Err(DriveError::Service(s)) if s.status == 507),
        "the file that does not fit says the drive is full: {:?}",
        results[2]
    );
    for (path, bytes) in &files[..2] {
        assert_eq!(&drive.get(path).unwrap(), bytes);
    }
    assert!(drive.get(&files[2].0).is_err());
    assert!(bucket.data_bytes() <= 250_000);
}

/// X-AR4-37 (the AZ-C3/C6 family): a range an index entry names is an index entry's word, and a
/// hostile member writes the index. `read_resumed` used to reserve all of it up front - a
/// petabyte for an entry that says so, and the process aborted. The first reservation is capped
/// and the reads decide: a range past the pack's end is an error.
#[test]
fn x_ar4_37_a_range_an_index_entry_names_does_not_reserve_its_whole_length() {
    use crate::packing::{read_resumed, ReadCounter};

    let bucket = MemBucket::new();
    bucket.set("pack", vec![7u8; 100]);
    let counted = ReadCounter::default();
    let answer = read_resumed(&bucket, "pack", 0, 1 << 50, &counted);
    assert!(answer.is_err(), "{answer:?}");
    // A range the pack has is read whole, however the reservation was capped.
    assert_eq!(
        read_resumed(&bucket, "pack", 10, 50, &counted).unwrap(),
        vec![7u8; 50]
    );
}

// ==== X-DELTA-AZDRIVE-KEYS-14: numbers an index entry carries size nothing ====

/// A zstd frame that declares `size` bytes of content in its header (single segment, an 8-byte
/// content size) and holds one empty raw block.
fn zstd_frame_declaring(size: u64) -> Vec<u8> {
    let mut frame = vec![0x28, 0xB5, 0x2F, 0xFD, 0xE0];
    frame.extend_from_slice(&size.to_le_bytes());
    frame.extend_from_slice(&[0x01, 0x00, 0x00]);
    frame
}

/// X-DELTA-AZDRIVE-KEYS-14: `decode_record(.., entry.size, ..)` handed the entry's size - a number
/// a member of the drive writes - to `zstd::bulk::decompress` and `Vec::with_capacity`: a record of
/// 16 bytes (a frame declaring 2^60) and a pointer with `size = 2^60` aborted every reader of the
/// file (`panic = "abort"`), and the sync again after the restart. The size is held against what a
/// pack can be before a byte is reserved.
#[test]
fn x_delta_azdrive_keys_14_a_record_written_for_a_huge_size_is_refused_before_it_is_reserved() {
    let hash = [0u8; 32];
    for codec in [
        azlin_compress::segment::Codec::Zstd,
        azlin_compress::segment::Codec::Brotli,
        azlin_compress::segment::Codec::ZstdDict,
    ] {
        let mut record = vec![codec.byte()];
        record.extend(zstd_frame_declaring(1 << 60));
        for size in [1u64 << 60, PACK_MAX + 1, u64::MAX] {
            let refused = crate::packing::decode_record(&record, size, &hash, "a.txt");
            assert!(
                matches!(&refused, Err(DriveError::Corrupt { reason, .. }) if reason.contains("file")),
                "{codec:?}, {size}: {refused:?}"
            );
        }
    }
    // The member container (codec byte 5) is held to the same bound.
    let refused = crate::packing::decode_record(&[5, 1, 2, 3], 1 << 60, &hash, "a.txt");
    assert!(
        matches!(refused, Err(DriveError::Corrupt { .. })),
        "{refused:?}"
    );
}

/// X-DELTA-AZDRIVE-KEYS-14: the numbers of a packed file's entry and of its pack's reference are
/// each held against a pack's size: the file, its record, the pack's plaintext, the pack's object.
#[test]
fn x_delta_azdrive_keys_14_every_number_of_a_packed_entry_is_held_against_a_packs_size() {
    use crate::packing::{check_packed, PackRef, PackedAt};
    let at = |length: u64, plaintext: u64, object: u64| PackedAt {
        pack: PackRef {
            id: ObjectId([7; 16]),
            plaintext_size: plaintext,
            object_len: object,
            segment_size: 4096,
            prefix: [0; crate::crypto::azl1::PREFIX_LEN],
        },
        offset: 0,
        length,
    };
    assert!(check_packed("a.txt", 10_000, &at(10_001, PACK_MAX, PACK_MAX)).is_ok());
    for (size, packed) in [
        (PACK_MAX + 1, at(10, 4096, 4096)),
        (10, at(PACK_MAX + 1, 4096, 4096)),
        (10, at(10, PACK_MAX + 1, 4096)),
        (10, at(10, 4096, PACK_MAX + 1)),
        (1 << 60, at(1 << 60, 1 << 60, 1 << 60)),
    ] {
        let refused = check_packed("a.txt", size, &packed);
        assert!(
            matches!(refused, Err(DriveError::Corrupt { .. })),
            "{size} {packed:?}: {refused:?}"
        );
    }
}

/// An index whose `hijack`-th apply LANDS and then answers with a conflict at its first path: what
/// a change sent again after it landed answers (a 409 for a swap still under way, then the rebuilt
/// change meeting its own commit). The applies after it go straight to the index, so a change that
/// expects its path absent conflicts with the landed commit's paths as well. After the landing the
/// index can be made unreadable (`unreadable_after`): the read that would say whether it landed.
struct OwnCommitConflicts {
    inner: MemoryIndex,
    /// The apply, counted from 1, that lands and answers with a conflict; 0: none.
    hijack: AtomicU32,
    calls: AtomicU32,
    unreadable_after: AtomicBool,
    unreadable: AtomicBool,
}

impl OwnCommitConflicts {
    fn new(hijack: u32, unreadable_after: bool) -> Arc<OwnCommitConflicts> {
        Arc::new(OwnCommitConflicts {
            inner: MemoryIndex::new(),
            hijack: AtomicU32::new(hijack),
            calls: AtomicU32::new(0),
            unreadable_after: AtomicBool::new(unreadable_after),
            unreadable: AtomicBool::new(false),
        })
    }

    fn reading(&self) -> Result<(), DriveError> {
        if self.unreadable.load(Ordering::SeqCst) {
            return Err(DriveError::Transport(String::from(
                "connection reset by peer",
            )));
        }
        Ok(())
    }
}

impl NameIndex for OwnCommitConflicts {
    fn get(&self, path: &str) -> Result<Option<IndexEntry>, DriveError> {
        self.reading()?;
        self.inner.get(path)
    }
    fn list(&self, request: &ListRequest) -> Result<IndexPage, DriveError> {
        self.reading()?;
        self.inner.list(request)
    }
    fn apply(&self, changes: Vec<IndexChange>) -> Result<Released, DriveError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if call != self.hijack.load(Ordering::SeqCst) {
            return self.inner.apply(changes);
        }
        let first = match changes.first() {
            Some(IndexChange::Put { path, .. } | IndexChange::Remove { path, .. }) => path.clone(),
            None => String::new(),
        };
        self.inner.apply(changes)?;
        if self.unreadable_after.load(Ordering::SeqCst) {
            self.unreadable.store(true, Ordering::SeqCst);
        }
        Err(DriveError::Conflict { key: first })
    }
}

/// A drive that packs small files over `index`, its journal in `dir`.
fn packing_over(
    bucket: &Arc<MemBucket>,
    index: &Arc<OwnCommitConflicts>,
    key: &DriveKey,
    dir: &TempDir,
) -> EncryptedDrive<Arc<MemBucket>> {
    let options = PackOptions::default().with_journal(dir.path().join("packs"));
    let index: Arc<dyn NameIndex> = index.clone();
    EncryptedDrive::new(Arc::clone(bucket), key.clone(), index).with_packing(options)
}

/// Create-only writes of `files`: every file must be a new name.
fn put_new(
    drive: &EncryptedDrive<Arc<MemBucket>>,
    files: &[(String, Vec<u8>)],
) -> Vec<Result<Option<String>, DriveError>> {
    let absent = Precondition::Absent;
    let items: Vec<BatchPut<'_>> = files
        .iter()
        .map(|(path, bytes)| BatchPut::new(path, bytes).when(&absent))
        .collect();
    drive.put_many(&items)
}

/// An index whose next `apply` first runs something else (once): another thread of the process, here
/// the upkeep, while a commit is under way.
struct DuringApply {
    inner: MemoryIndex,
    then: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl NameIndex for DuringApply {
    fn get(&self, path: &str) -> Result<Option<IndexEntry>, DriveError> {
        self.inner.get(path)
    }
    fn list(&self, request: &ListRequest) -> Result<IndexPage, DriveError> {
        self.inner.list(request)
    }
    fn apply(&self, changes: Vec<IndexChange>) -> Result<Released, DriveError> {
        let then = self.then.lock().unwrap().take();
        if let Some(then) = then {
            then();
        }
        self.inner.apply(changes)
    }
}

/// X-DELTA-SYNC4-9, the pack in flight. `recover_packs` took a pack for abandoned when its journal
/// stamp - taken BEFORE the upload - was older than JOURNAL_STALE_SECS, though the commit that names it
/// was still running in this very process (a slow upload and a commit that waits and retries can outlast
/// a quarter of an hour): the upkeep deleted the pack under the commit, and the index then named a pack
/// no bucket held. The process knows what it is writing now, and the stamp is renewed when the upload
/// is over. RED on the old code: the upkeep clears the pack up (1), and the files read "its object is
/// missing".
#[test]
fn x_delta_sync4_9_a_pack_this_process_is_still_committing_is_not_cleared_up_as_abandoned() {
    let dir = TempDir::new("pack-writing");
    let bucket = Arc::new(MemBucket::new());
    let index = Arc::new(DuringApply {
        inner: MemoryIndex::new(),
        then: Mutex::new(None),
    });
    let named: Arc<dyn NameIndex> = index.clone();
    let options = PackOptions::default().with_journal(dir.path().join("packs"));
    let drive = Arc::new(
        EncryptedDrive::new(Arc::clone(&bucket), DriveKey::generate().unwrap(), named)
            .with_packing(options),
    );
    // The commit has been running for longer than the journal's stale time when the upkeep comes by.
    let later = now_unix() + JOURNAL_STALE_SECS + 60;
    let cleared = Arc::new(AtomicU32::new(u32::MAX));
    {
        let (drive, cleared) = (Arc::clone(&drive), Arc::clone(&cleared));
        *index.then.lock().unwrap() = Some(Box::new(move || {
            let n = drive.recover_packs(later).unwrap();
            cleared.store(u32::try_from(n).unwrap(), Ordering::SeqCst);
        }));
    }
    let files = small_files(5, 700);
    let results = put_new(&drive, &files);
    assert_eq!(
        cleared.load(Ordering::SeqCst),
        0,
        "the upkeep left the pack that is being committed alone"
    );
    assert!(
        results.iter().all(Result::is_ok),
        "{:?}",
        results.iter().filter_map(|r| r.as_ref().err()).next()
    );
    for (path, bytes) in &files {
        assert_eq!(&drive.get(path).unwrap(), bytes, "{path}");
    }
    assert!(dangling(&drive).is_empty());
}

/// X-DELTA-SYNC4-2, row "the change landed, its retry met its own commit". The one index change
/// that names a pack of 20 new files landed and answered with a conflict; the rebuilt changes expect
/// their paths absent and conflict with the commit's own paths, one after the other, until every
/// target is dead. `commit_pack` took that for "refused for good" and deleted the pack the index
/// names: 20 entries read "its object is missing" and every caller was told Conflict. RED on the old
/// code: every result is a conflict and the bucket holds no pack.
#[test]
fn x_delta_sync4_2_a_pack_whose_commit_landed_and_whose_retry_conflicted_is_kept_and_its_files_are_written(
) {
    let dir = TempDir::new("pack-own-commit");
    let bucket = Arc::new(MemBucket::new());
    let index = OwnCommitConflicts::new(1, false);
    let drive = packing_over(&bucket, &index, &DriveKey::generate().unwrap(), &dir);
    let files = small_files(20, 700);
    let results = put_new(&drive, &files);
    assert!(
        results.iter().all(Result::is_ok),
        "the files are in the index: {:?}",
        results.iter().filter_map(|r| r.as_ref().err()).next()
    );
    assert_eq!(
        data_objects(&bucket).len(),
        1,
        "the pack the index names is in the bucket"
    );
    for (path, bytes) in &files {
        assert_eq!(&drive.get(path).unwrap(), bytes, "{path}");
    }
    assert!(dangling(&drive).is_empty());
}

/// X-DELTA-SYNC4-2, the mixed row: a name that another writer took (a real conflict) and the files
/// of the commit that landed. The files the index names with the pack are written, the one that is
/// someone else's is a conflict, and the pack stays for the files it holds. RED on the old code:
/// the pack is deleted, so the files the index names have no object.
#[test]
fn x_delta_sync4_2_a_member_that_lost_to_another_writer_is_a_conflict_and_the_landed_ones_are_written(
) {
    let dir = TempDir::new("pack-own-commit-mixed");
    let bucket = Arc::new(MemBucket::new());
    // The first write of the test is the apply 1; of the second write the first apply is the real
    // conflict of the taken name (2) and the second one lands and conflicts (3).
    let index = OwnCommitConflicts::new(3, false);
    let drive = packing_over(&bucket, &index, &DriveKey::generate().unwrap(), &dir);
    drive.put("notes/0001.txt", b"someone else's file").unwrap();
    let packs_before = data_objects(&bucket).len();
    let files = small_files(5, 600);
    let results = put_new(&drive, &files);
    assert!(
        matches!(results[1], Err(DriveError::Conflict { .. })),
        "the taken name stays a conflict: {:?}",
        results[1]
    );
    for i in [0usize, 2, 3, 4] {
        assert!(results[i].is_ok(), "{}: {:?}", files[i].0, results[i]);
        assert_eq!(
            drive.get(&files[i].0).unwrap(),
            files[i].1,
            "{}",
            files[i].0
        );
    }
    assert_eq!(drive.get("notes/0001.txt").unwrap(), b"someone else's file");
    assert_eq!(
        data_objects(&bucket).len(),
        packs_before + 1,
        "the new pack is kept"
    );
    assert!(dangling(&drive).is_empty());
}

/// X-DELTA-SYNC4-2, row "every path really conflicts": nothing names the pack, the index is asked
/// and says so, and the pack leaves the bucket again (the compensation still works: it asks, then
/// deletes). Green on the old code too.
#[test]
fn x_delta_sync4_2_a_pack_no_path_of_which_the_index_names_goes_again_after_real_conflicts() {
    let dir = TempDir::new("pack-real-conflicts");
    let bucket = Arc::new(MemBucket::new());
    let index = OwnCommitConflicts::new(0, false);
    let drive = packing_over(&bucket, &index, &DriveKey::generate().unwrap(), &dir);
    let files = small_files(4, 500);
    put_all(&drive, &files);
    let before = data_objects(&bucket);
    assert_eq!(before.len(), 1);
    let others: Vec<(String, Vec<u8>)> = files
        .iter()
        .map(|(path, _)| (path.clone(), text(450, 99)))
        .collect();
    let results = put_new(&drive, &others);
    assert!(
        results
            .iter()
            .all(|r| matches!(r, Err(DriveError::Conflict { .. }))),
        "{results:?}"
    );
    assert_eq!(
        data_objects(&bucket),
        before,
        "the pack nothing names left the bucket again"
    );
    for (path, bytes) in &files {
        assert_eq!(&drive.get(path).unwrap(), bytes);
    }
}

/// X-DELTA-SYNC4-2, row "the index cannot be asked": the commit landed, its retry conflicted, and
/// the read that would say so fails. Nothing is deleted, the callers are told it is not known (not
/// "conflict"), and the upkeep that runs later finds the pack named and keeps it. RED on the old
/// code: the pack was deleted on the conflict's word.
#[test]
fn x_delta_sync4_2_a_conflict_whose_read_back_fails_keeps_the_pack_and_is_not_a_conflict() {
    let dir = TempDir::new("pack-own-commit-unread");
    let bucket = Arc::new(MemBucket::new());
    let index = OwnCommitConflicts::new(1, true);
    let key = DriveKey::generate().unwrap();
    let drive = packing_over(&bucket, &index, &key, &dir);
    let files = small_files(6, 650);
    let results = put_new(&drive, &files);
    assert_eq!(
        data_objects(&bucket).len(),
        1,
        "the pack stays: the index may name it"
    );
    assert!(
        results
            .iter()
            .all(|r| matches!(r, Err(DriveError::Transport(_)))),
        "not known, not a conflict: {results:?}"
    );

    // The index can be read again: the files are there, and the upkeep keeps the pack.
    index.unreadable.store(false, Ordering::SeqCst);
    for (path, bytes) in &files {
        assert_eq!(&drive.get(path).unwrap(), bytes, "{path}");
    }
    let later = now_unix() + JOURNAL_STALE_SECS + 1;
    assert_eq!(
        drive.recover_packs(later).unwrap(),
        1,
        "the pack was in flight"
    );
    assert_eq!(
        data_objects(&bucket).len(),
        1,
        "a pack the index names is no garbage"
    );
    for (path, bytes) in &files {
        assert_eq!(&drive.get(path).unwrap(), bytes, "{path}");
    }
    assert!(dangling(&drive).is_empty());
    assert_eq!(
        drive.recover_packs(later).unwrap(),
        0,
        "nothing is left in flight"
    );
}
