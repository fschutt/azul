//! Many files at once: a folder deleted or moved, many files deleted or moved together. A plain
//! bucket does it with the service's batch calls (DeleteObjects for up to 1,000 keys a request,
//! CopyObject on the service, never a download); an encrypted drive does it in ONE commit of its
//! index, and a move or a rename writes no object at all.

use std::io::{Cursor, Read};

use azul_storage::{BatchDelete, BatchMove, BatchStream, Drive, DriveError, Precondition};

use super::{fake_s3::FakeS3, sync_named::s3_drive};

/// `count` objects of one byte under `prefix`, written straight into the service.
fn fill(s3: &FakeS3, prefix: &str, count: usize) -> Vec<String> {
    (0..count)
        .map(|i| {
            let key = format!("{prefix}{i:05}.txt");
            s3.write(&key, vec![b'a' + (i % 26) as u8]);
            key
        })
        .collect()
}

#[test]
fn deleting_a_folder_of_a_plain_bucket_takes_one_delete_objects_request_per_thousand_files() {
    let s3 = FakeS3::new();
    fill(&s3, "big/", 2_500);
    s3.write("keep.txt", b"kept".to_vec());
    let drive = s3_drive(&s3);
    s3.clear_log();
    drive.delete_folder("big/").unwrap();
    assert_eq!(s3.count("DELETEOBJECTS"), 3, "1,000 + 1,000 + 500 (and the marker)");
    assert_eq!(s3.count("DELETE"), 0, "no request per file");
    assert_eq!(s3.keys(), vec![String::from("keep.txt")]);
}

#[test]
fn many_deletes_of_a_plain_bucket_go_in_one_request_and_an_edit_beats_a_delete() {
    let s3 = FakeS3::new();
    let keys: Vec<String> = (0..10).map(|i| format!("notes/{i}.txt")).collect();
    let tags: Vec<String> = keys
        .iter()
        .map(|key| s3.write(key, b"first".to_vec()).trim_matches('"').to_string())
        .collect();
    // Another device edits one of them after this one read the listing.
    s3.write(&keys[3], b"edited elsewhere".to_vec());
    let drive = s3_drive(&s3);
    s3.clear_log();
    let items: Vec<BatchDelete<'_>> = keys
        .iter()
        .zip(&tags)
        .map(|(key, tag)| BatchDelete::new(key).of_version(tag))
        .collect();
    let results = drive.delete_many(&items);
    assert!(matches!(results[3], Err(DriveError::Conflict { .. })), "{:?}", results[3]);
    assert!(results.iter().enumerate().all(|(i, r)| i == 3 || r.is_ok()), "{results:?}");
    assert_eq!(s3.count("DELETEOBJECTS"), 1);
    assert_eq!(s3.count("DELETE"), 0);
    assert_eq!(s3.keys(), vec![keys[3].clone()], "the edit stays");
    // Deleting what is gone is done.
    let again = drive.delete_many(&[BatchDelete::new(&keys[0])]);
    assert!(again[0].is_ok());
}

#[test]
fn renaming_a_folder_of_a_plain_bucket_copies_on_the_service_and_deletes_in_one_batch() {
    let s3 = FakeS3::new();
    let keys = fill(&s3, "a/", 50);
    let drive = s3_drive(&s3);
    s3.clear_log();
    drive.rename("a/", "b/").unwrap();
    assert_eq!(s3.count("COPY"), 50, "every file copied on the service");
    assert_eq!(s3.count("GET"), 0, "nothing downloaded");
    assert_eq!(s3.count("PUT"), 0, "nothing uploaded");
    assert_eq!(s3.count("DELETEOBJECTS"), 1);
    assert_eq!(s3.count("DELETE"), 0);
    for (i, key) in keys.iter().enumerate() {
        assert!(s3.read(key).is_none(), "{key} moved");
        let moved = key.replacen("a/", "b/", 1);
        assert_eq!(s3.read(&moved), Some(vec![b'a' + (i % 26) as u8]), "{moved}");
    }
}

#[test]
fn moving_many_files_of_a_plain_bucket_never_overwrites_and_says_what_was_missing() {
    let s3 = FakeS3::new();
    fill(&s3, "in/", 5);
    s3.write("out/00002.txt", b"there already".to_vec());
    let drive = s3_drive(&s3);
    s3.clear_log();
    let from: Vec<String> = (0..6).map(|i| format!("in/{i:05}.txt")).collect();
    let to: Vec<String> = (0..6).map(|i| format!("out/{i:05}.txt")).collect();
    let items: Vec<BatchMove<'_>> = from
        .iter()
        .zip(&to)
        .map(|(from, to)| BatchMove::new(from, to))
        .collect();
    let results = drive.move_many(&items);
    assert!(results[2].is_err(), "the target is taken: {:?}", results[2]);
    assert!(
        matches!(results[5], Err(DriveError::NotFound { .. })),
        "no such file: {:?}",
        results[5]
    );
    for i in [0, 1, 3, 4] {
        assert!(results[i].is_ok(), "{i}: {:?}", results[i]);
        assert!(s3.read(&from[i]).is_none() && s3.read(&to[i]).is_some(), "{i} moved");
    }
    assert_eq!(s3.read(&to[2]), Some(b"there already".to_vec()));
    assert!(s3.read(&from[2]).is_some(), "a move that could not land leaves its file");
    assert_eq!(s3.count("DELETEOBJECTS"), 1);
}

#[test]
fn a_batch_of_large_files_to_a_plain_bucket_lands_with_one_result_each() {
    let s3 = FakeS3::new();
    s3.write("taken.bin", b"there already".to_vec());
    let drive = s3_drive(&s3);
    let bodies: Vec<Vec<u8>> = (0..6).map(|i| vec![b'0' + i as u8; 700 * 1024]).collect();
    let keys: Vec<String> = (0..6).map(|i| format!("video/{i}.bin")).collect();
    let mut readers: Vec<Cursor<&[u8]>> = bodies.iter().map(|b| Cursor::new(&b[..])).collect();
    let absent = Precondition::Absent;
    let mut taken = Cursor::new(&b"mine"[..]);
    let mut items: Vec<BatchStream<'_>> = keys
        .iter()
        .zip(readers.iter_mut())
        .map(|(key, body)| BatchStream::new(key, body as &mut (dyn Read + Send)))
        .collect();
    items.push(BatchStream::new("taken.bin", &mut taken).when(&absent));
    let results = drive.put_from_many(&mut items);
    assert!(results[..6].iter().all(Result::is_ok), "{results:?}");
    assert!(matches!(results[6], Err(DriveError::Conflict { .. })), "{:?}", results[6]);
    for (key, body) in keys.iter().zip(&bodies) {
        assert_eq!(s3.read(key).as_ref(), Some(body), "{key}");
    }
    assert_eq!(s3.read("taken.bin"), Some(b"there already".to_vec()));
}

// ==== Bursts: a checkout, an unzip, a build ====

mod bursts {
    use std::{
        fs,
        path::PathBuf,
        thread,
        time::{Duration, Instant},
    };

    use azul_storage::{testing::TempDir, Drive};

    use super::super::{fake_s3::FakeS3, sync_named::s3_drive};
    use crate::sync::{
        guard::{PauseReason, BURST_CHANGES},
        sync_to, LocalRoot, RunHooks, SyncOptions, SyncReport, Target,
    };

    fn run(folder: &TempDir, state: &TempDir, drive: &dyn Drive, settle: Duration) -> SyncReport {
        let mut opts = SyncOptions::new("Docs/", "d-bulk", "laptop").unwrap();
        opts.settle = settle;
        let root = LocalRoot::folder(folder.path());
        let index = state.path().join("index.json");
        sync_to(Target::Named(drive), &root, &index, &opts, &RunHooks::default()).unwrap()
    }

    /// Writes `count` files into `dir`, `every` apart, on a thread of its own.
    fn burst(
        dir: PathBuf,
        count: usize,
        every: Duration,
        content: fn(usize) -> Vec<u8>,
    ) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            fs::create_dir_all(&dir).unwrap();
            for i in 0..count {
                fs::write(dir.join(format!("{i:04}.txt")), content(i)).unwrap();
                thread::sleep(every);
            }
        })
    }

    fn note(i: usize) -> Vec<u8> {
        format!("note {i}: the minutes of the meeting, the plan for the week.\n")
            .repeat(30)
            .into_bytes()
    }

    fn edited(i: usize) -> Vec<u8> {
        format!("note {i}, edited: the minutes of the meeting, the plan for the week.\n")
            .repeat(30)
            .into_bytes()
    }

    #[test]
    fn a_burst_still_being_written_when_a_run_starts_is_synced_as_a_whole() {
        let s3 = FakeS3::new();
        let drive = s3_drive(&s3);
        let (folder, state) = (TempDir::new("bulk-burst"), TempDir::new("bulk-burst-state"));
        let writer = burst(folder.path().join("unzipped"), 300, Duration::from_millis(3), note);
        thread::sleep(Duration::from_millis(30));
        let started = Instant::now();
        let report = run(&folder, &state, &*drive, Duration::from_millis(500));
        writer.join().unwrap();
        assert_eq!(report.files_up, 300, "{}", report.summary());
        assert!(started.elapsed() >= Duration::from_millis(500), "the run waited for quiet");
    }

    #[test]
    fn the_burst_guard_sees_a_whole_burst_written_while_a_run_starts() {
        let s3 = FakeS3::new();
        let drive = s3_drive(&s3);
        let (folder, state) = (TempDir::new("bulk-guard"), TempDir::new("bulk-guard-state"));
        let count = BURST_CHANGES + 50;
        burst(folder.path().join("notes"), count, Duration::ZERO, note).join().unwrap();
        let first = run(&folder, &state, &*drive, Duration::ZERO);
        assert_eq!(first.files_up, count, "new files are no burst: {}", first.summary());
        // Every file rewritten over about a second; the run starts at its beginning.
        let writer = burst(folder.path().join("notes"), count, Duration::from_millis(4), edited);
        thread::sleep(Duration::from_millis(30));
        let report = run(&folder, &state, &*drive, Duration::from_millis(500));
        writer.join().unwrap();
        let pause = report.paused.clone().expect("the whole burst pauses uploads");
        assert_eq!((pause.reason, pause.changes), (PauseReason::Burst, count));
        assert_eq!(report.files_up, 0, "nothing of the burst went up: {}", report.summary());
    }

    #[test]
    fn a_quiet_folder_is_synced_without_waiting() {
        let s3 = FakeS3::new();
        let drive = s3_drive(&s3);
        let (folder, state) = (TempDir::new("bulk-quiet"), TempDir::new("bulk-quiet-state"));
        burst(folder.path().join("a"), 5, Duration::ZERO, note).join().unwrap();
        thread::sleep(Duration::from_millis(250));
        let started = Instant::now();
        let report = run(&folder, &state, &*drive, Duration::from_millis(200));
        assert_eq!(report.files_up, 5);
        assert!(started.elapsed() < Duration::from_secs(5), "{:?}", started.elapsed());
    }
}

#[cfg(feature = "encryption")]
mod encrypted {
    use std::{
        io::{Cursor, Read},
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        thread,
        time::Duration,
    };

    use azul_storage::{
        crypto::{DriveKey, ObjectId},
        encrypted::{EncryptedDrive, IndexChange, IndexEntry, IndexPage, MemoryIndex, NameIndex},
        meta::{open_encrypted_drive, MemoryBucket},
        ops::list_all,
        packing::PackOptions,
        BatchDelete, BatchMove, BatchPut, BatchStream, ByteRange, Drive, DriveError, ListPage,
        ListRequest, ObjectInfo, Precondition, PARALLEL_FILES,
    };

    use super::super::{fake_s3::FakeS3, sync_named::s3_drive};

    /// An index in memory that counts its commits and fails the next one when asked.
    #[derive(Default)]
    struct Counting {
        inner: MemoryIndex,
        applies: AtomicUsize,
        fail_next: AtomicUsize,
    }

    impl NameIndex for Counting {
        fn get(&self, path: &str) -> Result<Option<IndexEntry>, DriveError> {
            self.inner.get(path)
        }
        fn list(&self, request: &ListRequest) -> Result<IndexPage, DriveError> {
            self.inner.list(request)
        }
        fn apply(&self, changes: Vec<IndexChange>) -> Result<Vec<ObjectId>, DriveError> {
            if self.fail_next.swap(0, Ordering::SeqCst) == 1 {
                return Err(DriveError::Transport(String::from("the computer went off")));
            }
            // A commit is an apply that went through: one refused for a taken target (the
            // batch then drops that item and applies the rest) is no commit.
            let applied = self.inner.apply(changes);
            if applied.is_ok() {
                self.applies.fetch_add(1, Ordering::SeqCst);
            }
            applied
        }
    }

    fn files(prefix: &str, count: usize, size: usize) -> Vec<(String, Vec<u8>)> {
        (0..count)
            .map(|i| {
                let bytes = format!("file {i} of {prefix}: ")
                    .into_bytes()
                    .into_iter()
                    .cycle()
                    .take(size)
                    .collect();
                (format!("{prefix}{i:05}.txt"), bytes)
            })
            .collect()
    }

    fn put_all(drive: &dyn Drive, files: &[(String, Vec<u8>)]) {
        let items: Vec<BatchPut<'_>> = files.iter().map(|(k, b)| BatchPut::new(k, b)).collect();
        for (result, (key, _)) in drive.put_many(&items).into_iter().zip(files) {
            result.unwrap_or_else(|e| panic!("{key}: {e}"));
        }
    }

    /// The drive index's commits so far: one log entry each.
    fn commits(bucket: &dyn Drive) -> usize {
        list_all(bucket, ".azlin/meta/log/").unwrap().len()
    }

    #[test]
    fn deleting_a_ten_thousand_file_folder_of_an_encrypted_drive_is_one_index_commit() {
        let bucket = Arc::new(MemoryBucket::new());
        let drive = open_encrypted_drive(
            Arc::clone(&bucket),
            DriveKey::generate().unwrap(),
            "laptop",
            "Laptop",
            None,
            false,
        )
        .unwrap()
        .with_packing(PackOptions::default());
        put_all(&drive, &files("big/", 10_000, 40));
        let kept = files("kept/", 3, 40);
        put_all(&drive, &kept);
        let before = commits(&*bucket);
        let data_before = list_all(&*bucket, "data/").unwrap().len();
        drive.delete_folder("big/").unwrap();
        assert_eq!(commits(&*bucket), before + 1, "one commit for 10,000 files");
        assert!(list_all(&drive, "big/").unwrap().is_empty());
        for (key, bytes) in &kept {
            assert_eq!(&drive.get(key).unwrap(), bytes);
        }
        assert_eq!(
            list_all(&*bucket, "data/").unwrap().len(),
            data_before,
            "the packs stay for the history, the repack and the garbage collection"
        );
    }

    #[test]
    fn deleting_many_files_of_an_encrypted_drive_is_one_commit_and_their_objects_go_in_batches() {
        let s3 = FakeS3::new();
        let index = Arc::new(Counting::default());
        let drive = EncryptedDrive::new(
            s3_drive(&s3),
            DriveKey::generate().unwrap(),
            Arc::clone(&index) as Arc<dyn NameIndex>,
        );
        // No packing: every file an object of its own.
        let all = files("notes/", 1_200, 8);
        for (key, bytes) in &all {
            drive.put(key, bytes).unwrap();
        }
        assert_eq!(s3.count_under("PUT", "data/"), 1_200);
        s3.clear_log();
        let applies = index.applies.load(Ordering::SeqCst);
        let items: Vec<BatchDelete<'_>> = all.iter().map(|(k, _)| BatchDelete::new(k)).collect();
        assert!(drive.delete_many(&items).iter().all(Result::is_ok));
        assert_eq!(index.applies.load(Ordering::SeqCst), applies + 1, "one index commit");
        assert_eq!(s3.count("DELETEOBJECTS"), 2, "1,000 + 200 objects");
        assert_eq!(s3.count("DELETE"), 0);
        assert!(s3.keys().iter().all(|k| !k.starts_with("data/")), "{:?}", s3.keys());
    }

    #[test]
    fn deleting_packed_files_only_takes_them_out_of_the_index() {
        let s3 = FakeS3::new();
        let drive = EncryptedDrive::new(
            s3_drive(&s3),
            DriveKey::generate().unwrap(),
            Arc::new(MemoryIndex::new()),
        )
        .with_packing(PackOptions::default());
        let all = files("small/", 400, 300);
        put_all(&drive, &all);
        s3.clear_log();
        let half: Vec<BatchDelete<'_>> =
            all.iter().step_by(2).map(|(k, _)| BatchDelete::new(k)).collect();
        assert!(drive.delete_many(&half).iter().all(Result::is_ok));
        assert_eq!(
            (s3.count("DELETEOBJECTS"), s3.count("DELETE")),
            (0, 0),
            "the pack still holds live files: the repack and the garbage collection take it"
        );
        for (key, bytes) in all.iter().skip(1).step_by(2) {
            assert_eq!(&drive.get(key).unwrap(), bytes);
        }
    }

    #[test]
    fn an_edit_on_another_device_beats_a_delete_in_a_batch() {
        let index = Arc::new(Counting::default());
        let drive = EncryptedDrive::new(
            Arc::new(MemoryBucket::new()),
            DriveKey::generate().unwrap(),
            Arc::clone(&index) as Arc<dyn NameIndex>,
        );
        let all = files("docs/", 5, 20);
        let tags: Vec<String> = all
            .iter()
            .map(|(k, b)| drive.put_if(k, b, &azul_storage::Precondition::Absent).unwrap().unwrap())
            .collect();
        drive.put(&all[1].0, b"edited").unwrap();
        let items: Vec<BatchDelete<'_>> = all
            .iter()
            .zip(&tags)
            .map(|((k, _), t)| BatchDelete::new(k).of_version(t))
            .collect();
        let results = drive.delete_many(&items);
        assert!(matches!(results[1], Err(DriveError::Conflict { .. })), "{results:?}");
        assert!(results.iter().enumerate().all(|(i, r)| i == 1 || r.is_ok()), "{results:?}");
        assert_eq!(drive.get(&all[1].0).unwrap(), b"edited");
        assert_eq!(list_all(&drive, "docs/").unwrap().len(), 1);
    }

    #[test]
    fn renaming_a_five_thousand_file_folder_of_an_encrypted_drive_writes_no_object() {
        let s3 = FakeS3::new();
        let plain = Arc::new(s3_drive(&s3));
        let drive = open_encrypted_drive(
            Arc::clone(&plain),
            DriveKey::generate().unwrap(),
            "laptop",
            "Laptop",
            None,
            false,
        )
        .unwrap()
        .with_packing(PackOptions::default());
        let small = files("a/", 5_000, 60);
        put_all(&drive, &small);
        // Packed or not: three files with objects of their own move the same way.
        let big = files("a/big/", 3, 600 * 1024);
        put_all(&drive, &big);
        let before = commits(&**plain);
        s3.clear_log();
        drive.rename("a/", "b/").unwrap();
        for method in ["PUT", "COPY", "GET", "DELETE"] {
            assert_eq!(s3.count_under(method, "data/"), 0, "no {method} of an object");
        }
        assert_eq!(s3.count("DELETEOBJECTS"), 0);
        assert_eq!(commits(&**plain), before + 1, "one commit");
        for (key, bytes) in small.iter().step_by(250).chain(&big) {
            let moved = key.replacen("a/", "b/", 1);
            assert_eq!(&drive.get(&moved).unwrap(), bytes, "{moved}");
            assert!(matches!(drive.get(key), Err(DriveError::NotFound { .. })));
        }
    }

    #[test]
    fn moving_many_files_of_an_encrypted_drive_is_one_commit_and_never_overwrites() {
        let index = Arc::new(Counting::default());
        let bucket = Arc::new(MemoryBucket::new());
        let drive = EncryptedDrive::new(
            Arc::clone(&bucket),
            DriveKey::generate().unwrap(),
            Arc::clone(&index) as Arc<dyn NameIndex>,
        )
        .with_packing(PackOptions::default());
        let all = files("in/", 300, 100);
        put_all(&drive, &all);
        drive.put("out/00007.txt", b"there already").unwrap();
        let writes = bucket.counts().writes;
        let applies = index.applies.load(Ordering::SeqCst);
        let to: Vec<String> = all.iter().map(|(k, _)| k.replacen("in/", "out/", 1)).collect();
        let items: Vec<BatchMove<'_>> = all
            .iter()
            .zip(&to)
            .map(|((from, _), to)| BatchMove::new(from, to))
            .collect();
        let results = drive.move_many(&items);
        assert!(results[7].is_err(), "the target is taken: {:?}", results[7]);
        assert!(results.iter().enumerate().all(|(i, r)| i == 7 || r.is_ok()));
        assert_eq!(index.applies.load(Ordering::SeqCst), applies + 1, "one commit");
        assert_eq!(bucket.counts().writes, writes, "no object written");
        assert_eq!(drive.get(&to[7]).unwrap(), b"there already");
        assert_eq!(drive.get(&all[7].0).unwrap(), all[7].1, "its file stays");
        assert_eq!(drive.get(&to[42]).unwrap(), all[42].1);
    }

    #[test]
    fn a_crash_in_the_middle_of_a_batch_leaves_the_index_and_the_bucket_as_they_were() {
        let s3 = FakeS3::new();
        let index = Arc::new(Counting::default());
        let drive = EncryptedDrive::new(
            s3_drive(&s3),
            DriveKey::generate().unwrap(),
            Arc::clone(&index) as Arc<dyn NameIndex>,
        );
        let all = files("keep/", 50, 10);
        for (key, bytes) in &all {
            drive.put(key, bytes).unwrap();
        }
        s3.clear_log();
        index.fail_next.store(1, Ordering::SeqCst);
        let items: Vec<BatchDelete<'_>> = all.iter().map(|(k, _)| BatchDelete::new(k)).collect();
        assert!(drive.delete_many(&items).iter().all(Result::is_err));
        assert_eq!((s3.count("DELETEOBJECTS"), s3.count("DELETE")), (0, 0));
        index.fail_next.store(1, Ordering::SeqCst);
        let to: Vec<String> = all.iter().map(|(k, _)| k.replacen("keep/", "moved/", 1)).collect();
        let moves: Vec<BatchMove<'_>> = all
            .iter()
            .zip(&to)
            .map(|((from, _), to)| BatchMove::new(from, to))
            .collect();
        assert!(drive.move_many(&moves).iter().all(Result::is_err));
        for (key, bytes) in &all {
            assert_eq!(&drive.get(key).unwrap(), bytes, "{key} as it was");
        }
        assert!(list_all(&drive, "moved/").unwrap().is_empty());
    }

    /// A bucket in memory whose writes take a while and that counts how many run at once.
    #[derive(Default)]
    struct Slow {
        inner: MemoryBucket,
        now: AtomicUsize,
        most: AtomicUsize,
    }

    impl Slow {
        fn track<T>(&self, write: impl FnOnce() -> T) -> T {
            let now = self.now.fetch_add(1, Ordering::SeqCst) + 1;
            self.most.fetch_max(now, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(30));
            let result = write();
            self.now.fetch_sub(1, Ordering::SeqCst);
            result
        }
    }

    impl Drive for Slow {
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
            self.track(|| self.inner.put(key, bytes))
        }
        fn put_if(
            &self,
            key: &str,
            bytes: &[u8],
            condition: &Precondition,
        ) -> Result<Option<String>, DriveError> {
            self.track(|| self.inner.put_if(key, bytes, condition))
        }
        fn delete(&self, key: &str) -> Result<(), DriveError> {
            self.inner.delete(key)
        }
        fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
            self.inner.head(key)
        }
    }

    fn streams<'a>(
        keys: &'a [String],
        readers: &'a mut [Cursor<Vec<u8>>],
    ) -> Vec<BatchStream<'a>> {
        keys.iter()
            .zip(readers.iter_mut())
            .map(|(key, body)| BatchStream::new(key, body as &mut (dyn Read + Send)))
            .collect()
    }

    #[test]
    fn a_batch_of_large_files_uploads_several_at_once_and_commits_the_index_once() {
        let bucket = Arc::new(Slow::default());
        let index = Arc::new(Counting::default());
        let drive = EncryptedDrive::new(
            Arc::clone(&bucket),
            DriveKey::generate().unwrap(),
            Arc::clone(&index) as Arc<dyn NameIndex>,
        )
        .with_packing(PackOptions::default());
        let big = files("films/", 10, 600 * 1024);
        let small = files("notes/", 40, 200);
        let mut keys: Vec<String> = big.iter().map(|(k, _)| k.clone()).collect();
        keys.extend(small.iter().map(|(k, _)| k.clone()));
        let mut readers: Vec<Cursor<Vec<u8>>> = big
            .iter()
            .chain(&small)
            .map(|(_, b)| Cursor::new(b.clone()))
            .collect();
        let applies = index.applies.load(Ordering::SeqCst);
        let mut items = streams(&keys, &mut readers);
        let results = drive.put_from_many(&mut items);
        assert!(results.iter().all(Result::is_ok), "{results:?}");
        assert_eq!(
            index.applies.load(Ordering::SeqCst),
            applies + 2,
            "one commit for the pack of small files, one for the large ones"
        );
        let most = bucket.most.load(Ordering::SeqCst);
        assert!((2..=PARALLEL_FILES).contains(&most), "{most} uploads at once");
        for (key, bytes) in big.iter().chain(&small) {
            assert_eq!(&drive.get(key).unwrap(), bytes, "{key}");
        }
        assert!(drive.entry(&big[0].0).unwrap().object.unwrap().packed.is_none());
        assert!(drive.entry(&small[0].0).unwrap().object.unwrap().packed.is_some());
    }

    #[test]
    fn a_crash_before_the_commit_of_a_batch_of_large_files_names_none_of_them() {
        let bucket = Arc::new(MemoryBucket::new());
        let index = Arc::new(Counting::default());
        let drive = EncryptedDrive::new(
            Arc::clone(&bucket),
            DriveKey::generate().unwrap(),
            Arc::clone(&index) as Arc<dyn NameIndex>,
        );
        let big = files("films/", 5, 600 * 1024);
        let keys: Vec<String> = big.iter().map(|(k, _)| k.clone()).collect();
        let mut readers: Vec<Cursor<Vec<u8>>> =
            big.iter().map(|(_, b)| Cursor::new(b.clone())).collect();
        index.fail_next.store(1, Ordering::SeqCst);
        let mut items = streams(&keys, &mut readers);
        assert!(drive.put_from_many(&mut items).iter().all(Result::is_err));
        assert!(list_all(&drive, "films/").unwrap().is_empty(), "the index names none");
        // What went up is garbage nothing names, never a name without its bytes.
        for (key, _) in &big {
            assert!(matches!(drive.get(key), Err(DriveError::NotFound { .. })));
        }
    }

    #[test]
    fn a_batch_of_large_files_drops_only_the_one_whose_condition_failed() {
        let bucket = Arc::new(MemoryBucket::new());
        let drive = EncryptedDrive::new(
            Arc::clone(&bucket),
            DriveKey::generate().unwrap(),
            Arc::new(MemoryIndex::new()),
        );
        drive.put("films/00001.txt", b"there already").unwrap();
        let objects_before = list_all(&*bucket, "data/").unwrap().len();
        let big = files("films/", 3, 600 * 1024);
        let keys: Vec<String> = big.iter().map(|(k, _)| k.clone()).collect();
        let mut readers: Vec<Cursor<Vec<u8>>> =
            big.iter().map(|(_, b)| Cursor::new(b.clone())).collect();
        let absent = Precondition::Absent;
        let mut items: Vec<BatchStream<'_>> = streams(&keys, &mut readers)
            .into_iter()
            .map(|item| item.when(&absent))
            .collect();
        let results = drive.put_from_many(&mut items);
        assert!(matches!(results[1], Err(DriveError::Conflict { .. })), "{results:?}");
        assert!(results[0].is_ok() && results[2].is_ok());
        assert_eq!(drive.get("films/00001.txt").unwrap(), b"there already");
        assert_eq!(
            list_all(&*bucket, "data/").unwrap().len(),
            objects_before + 2,
            "the refused file's object left the bucket again"
        );
    }

    // ==== The sync: a folder deleted, added and renamed on this device ====

    use std::fs;

    use azul_storage::testing::TempDir;

    use crate::sync::{local::path_of, sync_to, LocalRoot, RunHooks, SyncOptions, SyncReport, Target};

    /// A folder on this device and its sync state.
    struct Folder {
        dir: TempDir,
        state: TempDir,
    }

    impl Folder {
        fn new(name: &str) -> Folder {
            Folder {
                dir: TempDir::new(&format!("azcloud-bulk-{name}")),
                state: TempDir::new(&format!("azcloud-bulk-state-{name}")),
            }
        }

        fn sync(&self, drive: &dyn Drive) -> Result<SyncReport, crate::error::CloudError> {
            let mut opts = SyncOptions::new("Docs/", "d-bulk", "laptop").unwrap();
            opts.allow_mass_delete = true;
            opts.allow_burst = true;
            let root = LocalRoot::folder(self.dir.path());
            let index = self.state.path().join("index.json");
            sync_to(Target::Named(drive), &root, &index, &opts, &RunHooks::default())
        }

        fn write_all(&self, files: &[(String, Vec<u8>)]) {
            for (key, bytes) in files {
                let path = path_of(self.dir.path(), key);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, bytes).unwrap();
            }
        }

        fn path(&self, folder: &str) -> std::path::PathBuf {
            self.dir.path().join(folder)
        }
    }

    /// The files (not the folders: a drive index keeps an emptied folder) under `prefix`.
    fn on_drive(drive: &dyn Drive, prefix: &str) -> usize {
        list_all(drive, &format!("Docs/{prefix}"))
            .unwrap()
            .iter()
            .filter(|object| !object.key.ends_with('/'))
            .count()
    }

    #[test]
    fn a_sync_that_deletes_a_ten_thousand_file_folder_commits_the_drive_index_once() {
        let bucket = Arc::new(MemoryBucket::new());
        let drive = open_encrypted_drive(
            Arc::clone(&bucket),
            DriveKey::generate().unwrap(),
            "laptop",
            "Laptop",
            None,
            false,
        )
        .unwrap()
        .with_packing(PackOptions::default());
        let here = Folder::new("delete");
        here.write_all(&files("big/", 10_000, 30));
        here.write_all(&files("kept/", 3, 30));
        let sent = here.sync(&drive).unwrap();
        assert_eq!(sent.files_up, 10_003, "{}", sent.summary());
        fs::remove_dir_all(here.path("big")).unwrap();
        let before = commits(&*bucket);
        let gone = here.sync(&drive).unwrap();
        assert_eq!(gone.deleted_there, 10_000, "{}", gone.summary());
        assert_eq!(commits(&*bucket), before + 1, "one commit for 10,000 deletes");
        assert_eq!(on_drive(&drive, "big/"), 0);
        assert_eq!(on_drive(&drive, "kept/"), 3);
    }

    #[test]
    fn a_sync_that_adds_a_folder_of_five_thousand_files_takes_a_few_commits() {
        let bucket = Arc::new(MemoryBucket::new());
        let drive = open_encrypted_drive(
            Arc::clone(&bucket),
            DriveKey::generate().unwrap(),
            "laptop",
            "Laptop",
            None,
            false,
        )
        .unwrap()
        .with_packing(PackOptions::default());
        let here = Folder::new("add");
        let small = files("project/src/", 5_000, 900);
        let large = files("project/assets/", 6, 700 * 1024);
        here.write_all(&small);
        here.write_all(&large);
        let before = commits(&*bucket);
        let sent = here.sync(&drive).unwrap();
        assert_eq!(sent.files_up, 5_006, "{}", sent.summary());
        let made = commits(&*bucket) - before;
        assert!(made <= 4, "{made} commits for a folder of 5,006 files");
        for (key, bytes) in small.iter().step_by(500).chain(&large) {
            assert_eq!(&drive.get(&format!("Docs/{key}")).unwrap(), bytes, "{key}");
        }
    }

    #[test]
    fn a_folder_renamed_here_moves_on_the_drive_without_any_object_written() {
        let s3 = FakeS3::new();
        let index = Arc::new(Counting::default());
        let drive = EncryptedDrive::new(
            s3_drive(&s3),
            DriveKey::generate().unwrap(),
            Arc::clone(&index) as Arc<dyn NameIndex>,
        )
        .with_packing(PackOptions::default());
        let here = Folder::new("rename");
        let small = files("draft/", 5_000, 120);
        let large = files("draft/scans/", 2, 600 * 1024);
        here.write_all(&small);
        here.write_all(&large);
        here.sync(&drive).unwrap();
        fs::rename(here.path("draft"), here.path("final")).unwrap();
        s3.clear_log();
        let applies = index.applies.load(Ordering::SeqCst);
        let moved = here.sync(&drive).unwrap();
        for method in ["PUT", "COPY", "GET"] {
            assert_eq!(s3.count_under(method, "data/"), 0, "no {method} of an object");
        }
        assert_eq!(
            index.applies.load(Ordering::SeqCst),
            applies + 1,
            "one commit moves the folder: {}",
            moved.summary()
        );
        assert_eq!(on_drive(&drive, "draft/"), 0);
        assert_eq!(on_drive(&drive, "final/"), 5_002);
        for (key, bytes) in small.iter().step_by(700).chain(&large) {
            let renamed = key.replacen("draft/", "final/", 1);
            assert_eq!(&drive.get(&format!("Docs/{renamed}")).unwrap(), bytes, "{renamed}");
        }
    }

    #[test]
    fn a_crash_in_the_middle_of_a_sync_batch_leaves_the_drive_as_it_was_and_the_next_run_finishes() {
        let index = Arc::new(Counting::default());
        let drive = EncryptedDrive::new(
            Arc::new(MemoryBucket::new()),
            DriveKey::generate().unwrap(),
            Arc::clone(&index) as Arc<dyn NameIndex>,
        )
        .with_packing(PackOptions::default());
        let here = Folder::new("crash");
        here.write_all(&files("old/", 200, 50));
        here.sync(&drive).unwrap();
        fs::remove_dir_all(here.path("old")).unwrap();
        here.write_all(&files("new/", 200, 50));
        // The first commit of the run fails: the pack of new files is named by nothing.
        index.fail_next.store(1, Ordering::SeqCst);
        assert!(here.sync(&drive).is_err());
        assert_eq!(on_drive(&drive, "new/"), 0, "no name without its bytes");
        assert_eq!(on_drive(&drive, "old/"), 200, "nothing deleted");
        let again = here.sync(&drive).unwrap();
        assert_eq!((again.files_up, again.deleted_there), (200, 200), "{}", again.summary());
        assert_eq!((on_drive(&drive, "new/"), on_drive(&drive, "old/")), (200, 0));
    }
}
