use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use super::{
    fake_bucket::{FakeBucket, BUCKET},
    TempDir,
};
use crate::{
    transfer::{copy_object, download_path, download_to_file, upload_file},
    ByteRange, Credentials, Ctl, Drive, DriveError, ListPage, ListRequest, LocalDrive, ObjectInfo,
    S3Config, S3Drive,
};

/// A local drive that counts the reads and the streamed and file writes (it is no folder on
/// this computer to a copy: `local_path` is the trait's `None`).
struct Counting {
    inner: LocalDrive,
    gets: AtomicU32,
    ranges: AtomicU32,
    heads: AtomicU32,
    put_froms: AtomicU32,
    put_files: Mutex<Vec<(String, PathBuf)>>,
}

impl Counting {
    fn new(inner: LocalDrive) -> Self {
        Counting {
            inner,
            gets: AtomicU32::new(0),
            ranges: AtomicU32::new(0),
            heads: AtomicU32::new(0),
            put_froms: AtomicU32::new(0),
            put_files: Mutex::new(Vec::new()),
        }
    }
}

impl Drive for Counting {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.inner.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.gets.fetch_add(1, Ordering::SeqCst);
        self.inner.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.ranges.fetch_add(1, Ordering::SeqCst);
        self.inner.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.inner.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.inner.delete(key)
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.heads.fetch_add(1, Ordering::SeqCst);
        self.inner.head(key)
    }
    fn put_from(&self, key: &str, body: &mut dyn Read) -> Result<u64, DriveError> {
        self.put_froms.fetch_add(1, Ordering::SeqCst);
        self.inner.put_from(key, body)
    }
    fn put_file(&self, key: &str, path: &Path, ctl: &dyn Ctl) -> Result<u64, DriveError> {
        self.put_files
            .lock()
            .unwrap()
            .push((key.to_string(), path.to_path_buf()));
        self.inner.put_file(key, path, ctl)
    }
}

#[test]
fn a_small_object_downloads_in_one_get() {
    let tmp = TempDir::new("transfer-small");
    let drive = Counting::new(LocalDrive::new(tmp.path().join("drive")));
    drive.put("mail/a.eml", b"hello").unwrap();
    let dest = tmp.path().join("out").join("a.eml");
    let written = download_to_file(&drive, "mail/a.eml", Some(5), &dest, 1024).unwrap();
    assert_eq!(written, 5);
    assert_eq!(std::fs::read(&dest).unwrap(), b"hello");
    assert_eq!(drive.gets.load(Ordering::SeqCst), 1);
    assert_eq!(drive.ranges.load(Ordering::SeqCst), 0);
    assert_eq!(drive.heads.load(Ordering::SeqCst), 0);
}

#[test]
fn a_big_object_downloads_in_ranged_chunks() {
    let tmp = TempDir::new("transfer-big");
    let drive = Counting::new(LocalDrive::new(tmp.path().join("drive")));
    drive.put("big.bin", b"0123456789").unwrap();
    let dest = tmp.path().join("big.bin");
    let written = download_to_file(&drive, "big.bin", Some(10), &dest, 4).unwrap();
    assert_eq!(written, 10);
    assert_eq!(std::fs::read(&dest).unwrap(), b"0123456789");
    assert_eq!(drive.ranges.load(Ordering::SeqCst), 3);
    assert_eq!(drive.gets.load(Ordering::SeqCst), 0);
}

#[test]
fn an_unknown_size_is_asked_for_first() {
    let tmp = TempDir::new("transfer-head");
    let drive = Counting::new(LocalDrive::new(tmp.path().join("drive")));
    drive.put("a.txt", b"abc").unwrap();
    let dest = tmp.path().join("a.txt");
    download_to_file(&drive, "a.txt", None, &dest, 1024).unwrap();
    assert_eq!(drive.heads.load(Ordering::SeqCst), 1);
    assert_eq!(std::fs::read(&dest).unwrap(), b"abc");
}

#[test]
fn a_failed_download_leaves_no_partial_file() {
    let tmp = TempDir::new("transfer-fail");
    let drive = LocalDrive::new(tmp.path().join("drive"));
    let dest = tmp.path().join("missing.txt");
    assert!(download_to_file(&drive, "missing.txt", Some(3), &dest, 1024).is_err());
    assert!(!dest.exists());
    let leftovers: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains("missing"))
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn a_download_does_not_overwrite_an_existing_file() {
    let tmp = TempDir::new("transfer-name");
    assert_eq!(
        download_path(tmp.path(), "mail/inbox/0001.eml"),
        Some(tmp.path().join("0001.eml"))
    );
    std::fs::write(tmp.path().join("0001.eml"), b"old").unwrap();
    assert_eq!(
        download_path(tmp.path(), "mail/inbox/0001.eml"),
        Some(tmp.path().join("0001 (1).eml"))
    );
    std::fs::write(tmp.path().join("0001 (1).eml"), b"old").unwrap();
    assert_eq!(
        download_path(tmp.path(), "mail/inbox/0001.eml"),
        Some(tmp.path().join("0001 (2).eml"))
    );
    std::fs::write(tmp.path().join("README"), b"old").unwrap();
    assert_eq!(
        download_path(tmp.path(), "README"),
        Some(tmp.path().join("README (1)"))
    );
    assert_eq!(download_path(tmp.path(), "mail/.."), None);
}

#[test]
fn upload_file_puts_the_local_bytes_under_the_key() {
    let tmp = TempDir::new("transfer-upload");
    let drive = LocalDrive::new(tmp.path().join("drive"));
    let source = tmp.path().join("notes.txt");
    std::fs::write(&source, b"notes").unwrap();
    assert_eq!(upload_file(&drive, &source, "docs/notes.txt").unwrap(), 5);
    assert_eq!(drive.get("docs/notes.txt").unwrap(), b"notes");
}

/// A bucket on the fake service (its ETags tell a download whether it may resume).
fn bucket_on(s3: &Arc<FakeBucket>) -> S3Drive {
    S3Drive::new(
        S3Config {
            endpoint: String::from("http://127.0.0.1:9000"),
            region: String::from("us-east-1"),
            bucket: BUCKET.to_string(),
            path_style: true,
        },
        Credentials::new("AKIDTEST", "test-secret"),
        s3.transport(),
    )
    .unwrap()
}

/// The files of `dir` whose name contains `part`.
fn files_named(dir: &Path, part: &str) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(part))
        .collect()
}

/// 11 chunks of 1 KiB (the last one short).
fn eleven_chunks(seed: u8) -> Vec<u8> {
    (0..10 * 1024 + 5).map(|i| (i % 241) as u8 ^ seed).collect()
}

#[test]
fn a_big_download_fetches_several_ranges_at_once() {
    let tmp = TempDir::new("transfer-parallel");
    let s3 = FakeBucket::new();
    let bytes = eleven_chunks(1);
    s3.write("big.bin", &bytes);
    s3.slow_ranges(Duration::from_millis(40));
    let dest = tmp.path().join("big.bin");
    let written = download_to_file(
        &bucket_on(&s3),
        "big.bin",
        Some(bytes.len() as u64),
        &dest,
        1024,
    )
    .unwrap();
    assert_eq!(written, bytes.len() as u64);
    assert_eq!(std::fs::read(&dest).unwrap(), bytes);
    assert_eq!(s3.count("GET object"), 11, "{:?}", s3.log());
    let most = s3.most_ranges_at_once();
    assert!(most > 1 && most <= 4, "{most} ranges at once");
    assert_eq!(
        files_named(tmp.path(), "big.bin"),
        vec![String::from("big.bin")]
    );
}

#[test]
fn a_download_that_failed_half_way_resumes_with_the_ranges_it_still_lacks() {
    let tmp = TempDir::new("transfer-resume");
    let s3 = FakeBucket::new();
    let bytes = eleven_chunks(2);
    s3.write("big.bin", &bytes);
    s3.take_ranges(Some(4));
    let dest = tmp.path().join("big.bin");
    let size = Some(bytes.len() as u64);
    assert!(download_to_file(&bucket_on(&s3), "big.bin", size, &dest, 1024).is_err());
    assert!(!dest.exists(), "nothing half-done under the file's name");
    s3.take_ranges(None);
    s3.clear_log();
    download_to_file(&bucket_on(&s3), "big.bin", size, &dest, 1024).unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), bytes);
    assert_eq!(
        s3.count("GET object"),
        7,
        "only the 7 missing ranges: {:?}",
        s3.log()
    );
    assert_eq!(
        files_named(tmp.path(), "big.bin"),
        vec![String::from("big.bin")],
        "the kept part and its state are gone"
    );
}

#[test]
fn a_download_whose_object_changed_meanwhile_starts_over() {
    let tmp = TempDir::new("transfer-changed");
    let s3 = FakeBucket::new();
    s3.write("big.bin", &eleven_chunks(3));
    s3.take_ranges(Some(4));
    let dest = tmp.path().join("big.bin");
    let size = Some(eleven_chunks(3).len() as u64);
    assert!(download_to_file(&bucket_on(&s3), "big.bin", size, &dest, 1024).is_err());
    let newer = eleven_chunks(4);
    s3.write("big.bin", &newer);
    s3.take_ranges(None);
    s3.clear_log();
    download_to_file(&bucket_on(&s3), "big.bin", size, &dest, 1024).unwrap();
    assert_eq!(
        std::fs::read(&dest).unwrap(),
        newer,
        "never parts of both versions"
    );
    assert_eq!(s3.count("GET object"), 11);
}

#[test]
fn upload_file_sends_the_file_through_put_file() {
    let tmp = TempDir::new("transfer-put-file");
    let drive = Counting::new(LocalDrive::new(tmp.path().join("drive")));
    let source = tmp.path().join("notes.txt");
    std::fs::write(&source, b"notes").unwrap();
    assert_eq!(upload_file(&drive, &source, "docs/notes.txt").unwrap(), 5);
    assert_eq!(
        *drive.put_files.lock().unwrap(),
        vec![(String::from("docs/notes.txt"), source.clone())]
    );
    assert_eq!(drive.get("docs/notes.txt").unwrap(), b"notes");
}

#[test]
fn copying_a_local_file_into_another_drive_sends_it_through_put_file() {
    let tmp = TempDir::new("transfer-copy-up");
    let source = LocalDrive::new(tmp.path().join("here"));
    source.put("docs/a.txt", b"alpha").unwrap();
    let target = Counting::new(LocalDrive::new(tmp.path().join("there")));
    let mut seen = Vec::new();
    let copied = copy_object(
        &source,
        "docs/a.txt",
        Some(5),
        &target,
        "in/a.txt",
        &mut |n| seen.push(n),
    )
    .unwrap();
    assert_eq!(copied, 5);
    let sent = target.put_files.lock().unwrap().clone();
    assert_eq!(sent.len(), 1, "the file itself, not its bytes in memory");
    assert_eq!(sent[0].0, "in/a.txt");
    assert_eq!(target.get("in/a.txt").unwrap(), b"alpha");
    assert_eq!(seen.last(), Some(&5));
}

#[test]
fn copying_a_big_object_between_two_remote_drives_streams_it_in_ranges() {
    let tmp = TempDir::new("transfer-copy-across");
    let source = Counting::new(LocalDrive::new(tmp.path().join("a")));
    let big: Vec<u8> = (0..crate::transfer::CHUNK as usize + 10)
        .map(|i| (i % 253) as u8)
        .collect();
    source.inner.put("big.bin", &big).unwrap();
    let target = Counting::new(LocalDrive::new(tmp.path().join("b")));
    let mut seen = Vec::new();
    copy_object(
        &source,
        "big.bin",
        Some(big.len() as u64),
        &target,
        "copy.bin",
        &mut |n| seen.push(n),
    )
    .unwrap();
    assert_eq!(target.inner.get("copy.bin").unwrap(), big);
    assert_eq!(
        source.gets.load(Ordering::SeqCst),
        0,
        "never the whole object at once"
    );
    assert_eq!(source.ranges.load(Ordering::SeqCst), 2);
    assert_eq!(
        target.put_froms.load(Ordering::SeqCst),
        1,
        "one streamed write"
    );
    assert_eq!(seen.last().copied(), Some(big.len() as u64));
}

/// A file's permission bits (Unix; 0 elsewhere, where the test does not look).
#[cfg(unix)]
fn mode_of(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[cfg(unix)]
fn give_time(path: &Path, at: std::time::SystemTime) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(at)
        .unwrap();
}

#[cfg(unix)]
#[test]
fn az_u9_a_copy_on_disk_keeps_the_mode_and_the_time_of_the_file() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = TempDir::new("transfer-u9");
    let source = LocalDrive::without_manifest(tmp.path().join("here"));
    source.put("keys/id_key", b"private key").unwrap();
    source.put("tools/run.sh", b"#!/bin/sh\n").unwrap();
    let key = tmp.path().join("here/keys/id_key");
    let script = tmp.path().join("here/tools/run.sh");
    std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let long_ago = std::time::UNIX_EPOCH + Duration::from_secs(1_200_000_000);
    give_time(&key, long_ago);
    give_time(&script, long_ago);
    let target = LocalDrive::without_manifest(tmp.path().join("there"));
    // The UI's copy and move between two drives on this disk: copy_object.
    copy_object(&source, "keys/id_key", None, &target, "id_key", &mut |_| {}).unwrap();
    copy_object(
        &source,
        "tools/run.sh",
        None,
        &target,
        "run.sh",
        &mut |_| {},
    )
    .unwrap();
    let copied = tmp.path().join("there/id_key");
    assert_eq!(std::fs::read(&copied).unwrap(), b"private key");
    assert_eq!(mode_of(&copied), 0o600, "a private key stays private");
    assert_eq!(
        mode_of(&tmp.path().join("there/run.sh")),
        0o755,
        "a script stays executable"
    );
    assert_eq!(
        std::fs::metadata(&copied).unwrap().modified().unwrap(),
        long_ago,
        "the copy has the date of the file"
    );
    // Inside one drive: LocalDrive::copy, plain and contained.
    source.copy("keys/id_key", "keys/id_key.bak").unwrap();
    assert_eq!(mode_of(&tmp.path().join("here/keys/id_key.bak")), 0o600);
    let contained = LocalDrive::without_manifest(tmp.path().join("here")).contained();
    contained.copy("tools/run.sh", "tools/run-copy.sh").unwrap();
    assert_eq!(mode_of(&tmp.path().join("here/tools/run-copy.sh")), 0o755);
    assert_eq!(
        std::fs::metadata(tmp.path().join("here/tools/run-copy.sh"))
            .unwrap()
            .modified()
            .unwrap(),
        long_ago
    );
    // No temporary file is left beside any of them.
    for dir in ["there", "here/keys", "here/tools"] {
        assert!(
            files_named(&tmp.path().join(dir), ".azul-storage-").is_empty(),
            "{dir}: {:?}",
            files_named(&tmp.path().join(dir), "")
        );
    }
}

#[test]
fn az_u5_az_m7_a_ranged_download_and_a_copy_come_in_whole_and_leave_nothing_hidden() {
    let tmp = TempDir::new("transfer-u5");
    let s3 = FakeBucket::new();
    let bytes = eleven_chunks(9);
    s3.write("big.bin", &bytes);
    let dest = tmp.path().join("out").join("big.bin");
    download_to_file(
        &bucket_on(&s3),
        "big.bin",
        Some(bytes.len() as u64),
        &dest,
        1024,
    )
    .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), bytes);
    let copy = tmp.path().join("out").join("copy.bin");
    let local = LocalDrive::without_manifest(tmp.path().join("out"));
    let there = LocalDrive::without_manifest(tmp.path().join("there"));
    copy_object(&local, "big.bin", None, &there, "copy.bin", &mut |_| {}).unwrap();
    assert_eq!(
        std::fs::read(tmp.path().join("there/copy.bin")).unwrap(),
        bytes
    );
    assert!(!copy.exists());
    for dir in ["out", "there"] {
        let all = files_named(&tmp.path().join(dir), "");
        assert!(
            all.iter().all(|name| !name.starts_with('.')),
            "{dir} holds a hidden file: {all:?}"
        );
    }
}

#[test]
fn az_w19_a_name_of_240_bytes_is_written_copied_and_downloaded_like_any_other() {
    // The temporary and the partial name of such a name were longer than the 255 bytes a name
    // may have, so the sync took it and every other path failed.
    let tmp = TempDir::new("transfer-w19");
    let long = format!("{}.txt", "n".repeat(240));
    let longer = format!("{}.bak", "m".repeat(240));
    let drive = LocalDrive::without_manifest(tmp.path().join("drive"));
    drive.put(&long, b"put").unwrap();
    assert_eq!(drive.get(&long).unwrap(), b"put");
    drive.put_from(&longer, &mut &b"streamed"[..]).unwrap();
    assert_eq!(drive.get(&longer).unwrap(), b"streamed");
    let copied = format!("{}.cpy", "c".repeat(240));
    drive.copy(&long, &copied).unwrap();
    assert_eq!(drive.get(&copied).unwrap(), b"put");
    let contained = LocalDrive::without_manifest(tmp.path().join("drive")).contained();
    contained
        .put(&format!("{}.d", "d".repeat(240)), b"contained")
        .unwrap();
    // A download: one GET, and ranged (a partial file and its state beside the name).
    let dest = tmp.path().join("out").join(&long);
    download_to_file(&drive, &long, Some(3), &dest, 1024).unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), b"put");
    let bigger = format!("{}.bin", "b".repeat(240));
    let bytes = eleven_chunks(5);
    drive.put(&bigger, &bytes).unwrap();
    let dest = tmp.path().join("out").join(&bigger);
    download_to_file(&drive, &bigger, Some(bytes.len() as u64), &dest, 1024).unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), bytes);
    for name in files_named(&tmp.path().join("out"), "") {
        assert!(name.len() <= 255 && !name.starts_with('.'), "{name}");
    }
}

// The pid of a process that cannot exist is "running" where the system cannot be asked (Windows).
#[cfg(unix)]
#[test]
fn az_u15_the_first_copy_into_a_folder_sweeps_what_an_earlier_run_left_there() {
    let tmp = TempDir::new("transfer-u15");
    let out = tmp.path().join("Downloads");
    std::fs::create_dir_all(&out).unwrap();
    // A copy of a run that was killed, and a download nobody resumed for ten days.
    std::fs::write(
        out.join(".report.pdf.azul-storage-2000000000-3.tmp"),
        b"half",
    )
    .unwrap();
    std::fs::write(out.join(".big.zip.download.part"), b"half").unwrap();
    std::fs::write(out.join(".big.zip.download-state.part"), b"{}").unwrap();
    std::fs::write(out.join(".mine"), b"the user's own hidden file").unwrap();
    let ten_days_ago = std::time::SystemTime::now() - Duration::from_secs(10 * 24 * 3600);
    give_time(&out.join(".big.zip.download.part"), ten_days_ago);
    give_time(&out.join(".big.zip.download-state.part"), ten_days_ago);
    let drive = LocalDrive::without_manifest(tmp.path().join("drive"));
    drive.put("a.txt", b"alpha").unwrap();
    download_to_file(&drive, "a.txt", Some(5), &out.join("a.txt"), 1024).unwrap();
    assert_eq!(std::fs::read(out.join("a.txt")).unwrap(), b"alpha");
    let mut left = files_named(&out, "");
    left.sort();
    assert_eq!(left, vec![String::from(".mine"), String::from("a.txt")]);
}

#[cfg(unix)]
#[test]
fn a_streamed_write_over_a_link_to_a_folder_is_refused_as_a_put_is() {
    let tmp = TempDir::new("transfer-link-folder");
    let drive = LocalDrive::without_manifest(tmp.path().join("drive"));
    drive.put("real/inside.txt", b"in").unwrap();
    std::os::unix::fs::symlink(
        tmp.path().join("drive/real"),
        tmp.path().join("drive/alias"),
    )
    .unwrap();
    assert!(matches!(
        drive.put("alias", b"x"),
        Err(DriveError::InvalidKey { .. })
    ));
    assert!(matches!(
        drive.put_from("alias", &mut &b"x"[..]),
        Err(DriveError::InvalidKey { .. })
    ));
    assert_eq!(drive.get("real/inside.txt").unwrap(), b"in");
    assert!(std::fs::symlink_metadata(tmp.path().join("drive/alias"))
        .unwrap()
        .file_type()
        .is_symlink());
}

/// A look at the step that syncs a range, failing the asked-for one: the disk that could not sync.
struct FailSync {
    /// Syncs asked so far.
    asked: AtomicU32,
    /// The one that fails (1 = the first).
    fails: u32,
    /// Syncs that went through.
    synced: AtomicU32,
}

impl crate::durable::Probe for FailSync {
    fn before(&self, step: crate::durable::Step) -> std::io::Result<()> {
        if step != crate::durable::Step::SyncFile {
            return Ok(());
        }
        let n = self.asked.fetch_add(1, Ordering::SeqCst) + 1;
        if n == self.fails {
            return Err(std::io::Error::other("the disk could not sync the range"));
        }
        self.synced.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// X-DELTA-AZDRIVE-FILES-5. A resumed ranged download trusted a state file written (a rename, no
/// sync) before the range's data was on the disk (`flush` of a `File` does nothing): after a power
/// cut the state could be durable while a range was not, the retry skipped it as done and published
/// the hole of zeros, and a Move deleted the cloud original. A range is synced to the disk before
/// the state names it: the state lists exactly the ranges whose sync went through.
#[test]
fn x_delta_azdrive_files_5_a_range_is_on_the_disk_before_the_state_says_it_is_done() {
    use crate::{ctl::Quiet, durable::Expect, transfer::download_ranges_probed};
    let tmp = TempDir::new("dx5-ranges");
    let drive = Counting::new(LocalDrive::new(tmp.path().join("drive")));
    drive.put("big.bin", b"0123456789AB").unwrap();
    let info = drive.head("big.bin").unwrap();
    let dest = tmp.path().join("out").join("big.bin");
    // Four ranges of three bytes; the disk fails the sync of the second one asked.
    let probe = FailSync {
        asked: AtomicU32::new(0),
        fails: 2,
        synced: AtomicU32::new(0),
    };
    let result = download_ranges_probed(&drive, &info, &dest, 3, Expect::Any, &Quiet, &probe);
    assert!(
        result.is_err(),
        "a range that could not be synced is no finished download: {result:?}"
    );
    assert!(probe.asked.load(Ordering::SeqCst) >= 2);
    assert!(!dest.exists(), "nothing was published");
    let state_file = std::fs::read_dir(dest.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.to_string_lossy().ends_with("download-state.part"))
        .expect("the state a failed download keeps for the next one");
    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(state_file).unwrap()).unwrap();
    let done = state["done"].as_array().expect("the ranges done").len();
    assert_eq!(
        done as u32,
        probe.synced.load(Ordering::SeqCst),
        "the state names a range whose data was not synced: {state}"
    );
    assert!(done < 4, "{state}");
}

// ---- X-DELTA-AZDRIVE-FILES-36: a listed size is not the object's

/// A drive whose object grew since it was listed: the listing said 5 bytes, `head` says a chunk
/// and ten bytes, and a whole-object `get` would put all of it in memory (S3's `get` takes
/// `u64::MAX`). It records which reads were made.
struct Grown {
    limited: Mutex<Vec<u64>>,
    gets: AtomicU32,
    ranges: AtomicU32,
    /// What `head` says the size is now.
    now: u64,
}

impl Grown {
    fn new(now: u64) -> Grown {
        Grown {
            limited: Mutex::new(Vec::new()),
            gets: AtomicU32::new(0),
            ranges: AtomicU32::new(0),
            now,
        }
    }
}

impl Drive for Grown {
    fn list(&self, _: &ListRequest) -> Result<ListPage, DriveError> {
        Ok(ListPage::default())
    }
    fn get(&self, _: &str) -> Result<Vec<u8>, DriveError> {
        self.gets.fetch_add(1, Ordering::SeqCst);
        Err(DriveError::Io(String::from(
            "the whole object must not be read",
        )))
    }
    fn get_limited(&self, key: &str, limit: crate::limits::Limit) -> Result<Vec<u8>, DriveError> {
        self.limited.lock().unwrap().push(limit.max);
        if self.now > limit.max {
            return Err(limit.check_object(key, self.now).unwrap_err());
        }
        Ok(vec![7u8; usize::try_from(self.now).unwrap()])
    }
    fn get_range(&self, _: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.ranges.fetch_add(1, Ordering::SeqCst);
        let end = range.end.unwrap_or(self.now - 1).min(self.now - 1);
        Ok(vec![7u8; usize::try_from(end - range.start + 1).unwrap()])
    }
    fn put(&self, _: &str, _: &[u8]) -> Result<(), DriveError> {
        Err(DriveError::Io(String::from("read-only")))
    }
    fn delete(&self, _: &str) -> Result<(), DriveError> {
        Ok(())
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        Ok(ObjectInfo {
            key: key.to_string(),
            size: self.now,
            modified: Some(1),
            etag: Some(String::from("grown")),
        })
    }
}

/// The listing said 5 bytes and the object has a chunk and ten more: the whole-object read is
/// limited to the chunk, refused, and the ranges take the object by the size it has now.
#[test]
fn x_delta_azdrive_files_36_an_object_that_grew_since_it_was_listed_is_read_by_ranges() {
    let tmp = TempDir::new("dx36-download");
    let now = crate::transfer::CHUNK + 10;
    let drive = Grown::new(now);
    let dest = tmp.path().join("big.log");
    let len = crate::transfer::download_guarded(
        &drive,
        "big.log",
        Some(5),
        &dest,
        crate::transfer::CHUNK,
        crate::durable::Expect::Absent,
        &crate::ctl::Quiet,
    )
    .unwrap();
    assert_eq!(len, now);
    assert_eq!(std::fs::metadata(&dest).unwrap().len(), now);
    assert_eq!(
        drive.gets.load(Ordering::SeqCst),
        0,
        "never the whole object, unlimited"
    );
    assert_eq!(*drive.limited.lock().unwrap(), vec![crate::transfer::CHUNK]);
    assert!(drive.ranges.load(Ordering::SeqCst) >= 2, "ranges took it");
}

/// The same between two drives that are no folders: the listing's size picks the single GET, which
/// is limited, and an object that grew is copied by ranges.
#[test]
fn x_delta_azdrive_files_36_a_copy_of_an_object_that_grew_goes_by_ranges_too() {
    let source = Grown::new(crate::transfer::CHUNK + 10);
    let target_dir = TempDir::new("dx36-copy");
    let target = Counting::new(LocalDrive::without_manifest(target_dir.path()));
    let len = crate::transfer::copy_object_guarded(
        &source,
        "big.log",
        Some(5),
        &target,
        "big.log",
        crate::durable::Expect::Any,
        &crate::ctl::Quiet,
    )
    .unwrap();
    assert_eq!(len, crate::transfer::CHUNK + 10);
    assert_eq!(source.gets.load(Ordering::SeqCst), 0);
    assert_eq!(
        *source.limited.lock().unwrap(),
        vec![crate::transfer::CHUNK]
    );
    assert_eq!(
        std::fs::metadata(target_dir.path().join("big.log"))
            .unwrap()
            .len(),
        crate::transfer::CHUNK + 10
    );
}

/// A refusal that is no growth stays the refusal: the object is as small as the listing said and
/// the drive still refused it.
#[test]
fn x_delta_azdrive_files_36_a_refusal_of_an_object_that_did_not_grow_is_not_hidden() {
    struct Refuses(Grown);
    impl Drive for Refuses {
        fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
            self.0.list(request)
        }
        fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
            self.0.get(key)
        }
        fn get_limited(&self, _: &str, _: crate::limits::Limit) -> Result<Vec<u8>, DriveError> {
            Err(DriveError::Protocol(String::from(
                "the drive's own refusal",
            )))
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
        fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
            self.0.head(key)
        }
    }
    let tmp = TempDir::new("dx36-refused");
    let drive = Refuses(Grown::new(5));
    let refused = crate::transfer::download_guarded(
        &drive,
        "small.log",
        Some(5),
        &tmp.path().join("small.log"),
        crate::transfer::CHUNK,
        crate::durable::Expect::Absent,
        &crate::ctl::Quiet,
    )
    .unwrap_err();
    assert!(matches!(refused, DriveError::Protocol(_)), "{refused:?}");
    assert!(!tmp.path().join("small.log").exists());
}

/// A drive whose object is replaced by other bytes of the same length while its ranges are read, and
/// put back before the last one: the second range read meets the other version. It can pin a range to
/// the version a read began with (`Drive::get_range_if`), as S3 does with `If-Match`.
struct ReplacedAndPutBack {
    inner: Arc<super::mem_bucket::MemBucket>,
    original: Vec<u8>,
    other: Vec<u8>,
    reads: Mutex<u32>,
}

impl ReplacedAndPutBack {
    /// One range read, in the order the reads arrive (the lock is held across it: the ranges are read
    /// in parallel, and the second must meet what the first left): the first read is of the original
    /// and leaves the other version in its place, the second meets it and puts the original back.
    /// `pin`: the version a pinned read asks for.
    fn read(&self, key: &str, range: ByteRange, pin: Option<&str>) -> Result<Vec<u8>, DriveError> {
        let mut reads = self.reads.lock().unwrap();
        *reads += 1;
        let now = self.inner.head(key)?;
        let answer = match pin {
            Some(tag) if now.etag.as_deref() != Some(tag) => Err(DriveError::Conflict {
                key: key.to_string(),
            }),
            _ => self.inner.get_range(key, range),
        };
        match *reads {
            1 => self.inner.set(key, self.other.clone()),
            2 => self.inner.set(key, self.original.clone()),
            _ => {}
        }
        answer
    }
}

impl Drive for ReplacedAndPutBack {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.inner.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.inner.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        // Plain: whatever the object holds when the request arrives.
        self.read(key, range, None)
    }
    fn get_range_if(
        &self,
        key: &str,
        range: ByteRange,
        condition: &crate::Precondition,
    ) -> Result<Vec<u8>, DriveError> {
        let crate::Precondition::Matches(tag) = condition else {
            return Err(DriveError::Unsupported(String::from("only a version")));
        };
        self.read(key, range, Some(tag))
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
    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &crate::Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.inner.put_if(key, bytes, condition)
    }
}

/// X-DELTA-SYNC4-12. `download_ranges` read its ranges with plain `get_range` and compared the object's
/// tag, size and date only at the end: an object replaced and put back while the ranges were read (the
/// tag it began with comes back) was published under the final name as the SPLICE of the two versions.
/// Every range now asks for the version the download began with where the drive can. RED on the old code:
/// the file is the splice.
#[test]
fn x_delta_sync4_12_a_ranged_download_of_an_object_replaced_and_put_back_is_no_splice() {
    let dir = TempDir::new("transfer-s12");
    let dest = dir.path().join("big.bin");
    let original: Vec<u8> = (0..12u8).collect();
    let other: Vec<u8> = (0..12u8).map(|b| 100 + b).collect();
    let inner = Arc::new(super::mem_bucket::MemBucket::new());
    inner.set("big.bin", original.clone());
    let drive = ReplacedAndPutBack {
        inner,
        original: original.clone(),
        other,
        reads: Mutex::new(0),
    };
    // Three ranges of four bytes, one at a time.
    let result = download_to_file(&drive, "big.bin", Some(12), &dest, 4);
    match result {
        Ok(_) => assert_eq!(
            std::fs::read(&dest).unwrap(),
            original,
            "a published file is the version the download began with, not a splice"
        ),
        Err(e) => assert!(
            matches!(e, DriveError::Conflict { .. }),
            "the object changed under the download: {e:?}"
        ),
    }
    assert!(
        !dest.exists() || std::fs::read(&dest).unwrap() == original,
        "nothing but the whole original is ever under the final name"
    );
}
