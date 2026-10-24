//! Small files packed together on an encrypted drive: many small files share one AZL1 object (a
//! pack), each read back with one ranged GET; a crash between a pack's upload and its index
//! commit leaves nothing the index names without its bytes; repacking keeps every file readable
//! and retires the old packs for the garbage collection's grace period.

use std::{
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Barrier,
    },
    thread,
};

use super::{mem_bucket::MemBucket, TempDir};
use crate::{
    crypto::{random_bytes, DriveKey, ObjectId, ShareKey},
    encrypted::{
        read_shared, EncryptedDrive, IndexChange, IndexEntry, IndexPage, MemoryIndex, NameIndex,
    },
    meta::pointer,
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
    fn apply(&self, changes: Vec<IndexChange>) -> Result<Vec<ObjectId>, DriveError> {
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
        crate::rotation::reencrypt_pass(&after, &mut state, &mut |_| Ok(()), &|| false).unwrap();
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
    use crate::{encrypted::open_encrypted, keyring::MemoryKeyring, meta::MetaIndexProvider};
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
    let provider = MetaIndexProvider::new("Laptop")
        .with_cache_root(Some(dir.path().to_path_buf()))
        .with_packing(true);
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
    let plain = MetaIndexProvider::new("Laptop");
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
    fn apply(&self, changes: Vec<IndexChange>) -> Result<Vec<ObjectId>, DriveError> {
        self.0.apply(changes).map(|_| Vec::new())
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
