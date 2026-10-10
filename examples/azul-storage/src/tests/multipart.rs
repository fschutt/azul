//! Big uploads to a bucket: parts of 16 MiB, four at once; a file's upload outlives the app
//! (a state file per upload in the resume folder: the upload id, the parts' ETags, the file's
//! size, date and the BLAKE3 of its start) and resumes with the parts that are missing; an
//! upload that is stale or whose file changed is aborted, never finished with mixed bytes; a
//! conditional upload that loses is a conflict; of writers racing on `If-None-Match: *` exactly
//! one wins.

use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Barrier,
    },
    time::Duration,
};

use super::{
    fake_bucket::{FakeBucket, BUCKET},
    TempDir,
};
use crate::{
    multipart::STALE_AFTER_SECS,
    s3::{PARALLEL_PARTS, PART_SIZE},
    Credentials, Drive, DriveError, Precondition, S3Config, S3Drive,
};

/// 2026-10-10T00:00:00Z.
const NOW: u64 = 1_791_590_400;
/// The tests' part size (a real bucket wants 5 MiB and more).
const PART: usize = 1024;

fn drive_on(s3: &Arc<FakeBucket>, resume: Option<&Path>, now: u64) -> S3Drive {
    let drive = S3Drive::new(
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
    .with_clock(move || now)
    .with_part_size(PART);
    match resume {
        Some(dir) => drive.with_resume_dir(dir),
        None => drive,
    }
}

/// `parts` parts and a bit, a different byte pattern for every `seed`.
fn file_with(dir: &TempDir, name: &str, parts: usize, seed: u8) -> (PathBuf, Vec<u8>) {
    let bytes: Vec<u8> = (0..parts * PART + 7)
        .map(|i| (i % 251) as u8 ^ seed)
        .collect();
    let path = dir.path().join(name);
    std::fs::write(&path, &bytes).unwrap();
    (path, bytes)
}

fn state_files(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "json"))
                .collect()
        })
        .unwrap_or_default()
}

fn no_progress(_: u64) {}

#[test]
fn parts_are_16_mib_and_four_go_up_at_once() {
    assert_eq!(PART_SIZE, 16 * 1024 * 1024);
    assert_eq!(PARALLEL_PARTS, 4);
    let tmp = TempDir::new("multipart-parallel");
    let (path, bytes) = file_with(&tmp, "big.bin", 10, 1);
    let s3 = FakeBucket::new();
    s3.slow_parts(Duration::from_millis(40));
    let sent = drive_on(&s3, None, NOW)
        .put_file("big.bin", &path, &no_progress)
        .unwrap();
    assert_eq!(sent, bytes.len() as u64);
    assert_eq!(s3.object("big.bin").unwrap(), bytes);
    assert_eq!(s3.count("POST start"), 1);
    assert_eq!(s3.count("PUT part"), 11, "{:?}", s3.log());
    assert_eq!(s3.count("POST complete"), 1);
    let most = s3.most_parts_at_once();
    assert!(
        most > 1 && most <= PARALLEL_PARTS,
        "{most} parts were in flight at once"
    );
}

#[test]
fn a_streamed_body_goes_up_several_parts_at_once_and_reads_no_more_than_it_sends() {
    let s3 = FakeBucket::new();
    s3.slow_parts(Duration::from_millis(40));
    let body: Vec<u8> = (0..9 * PART + 3).map(|i| (i % 13) as u8).collect();
    let sent = drive_on(&s3, None, NOW)
        .put_from("streamed.bin", &mut &body[..])
        .unwrap();
    assert_eq!(sent, body.len() as u64);
    assert_eq!(s3.object("streamed.bin").unwrap(), body);
    assert_eq!(s3.count("PUT part"), 10);
    let most = s3.most_parts_at_once();
    assert!(most > 1 && most <= PARALLEL_PARTS, "{most} at once");
}

#[test]
fn progress_hears_every_byte_of_a_file_once() {
    let tmp = TempDir::new("multipart-progress");
    let (path, bytes) = file_with(&tmp, "big.bin", 5, 2);
    let s3 = FakeBucket::new();
    let heard = AtomicU64::new(0);
    drive_on(&s3, None, NOW)
        .put_file("big.bin", &path, &|n| {
            heard.fetch_max(n, Ordering::SeqCst);
        })
        .unwrap();
    assert_eq!(heard.load(Ordering::SeqCst), bytes.len() as u64);
}

#[test]
fn a_file_of_one_part_or_less_goes_up_in_one_put_without_a_state_file() {
    let tmp = TempDir::new("multipart-small");
    let resume = tmp.path().join("resume");
    let path = tmp.path().join("small.txt");
    std::fs::write(&path, b"hello").unwrap();
    let s3 = FakeBucket::new();
    assert_eq!(
        drive_on(&s3, Some(&resume), NOW)
            .put_file("small.txt", &path, &no_progress)
            .unwrap(),
        5
    );
    assert_eq!(s3.log(), vec![String::from("PUT object small.txt")]);
    assert!(state_files(&resume).is_empty());
}

#[test]
fn a_killed_upload_resumes_from_its_state_file_and_sends_only_the_missing_parts() {
    let tmp = TempDir::new("multipart-resume");
    let resume = tmp.path().join("resume");
    let (path, bytes) = file_with(&tmp, "big.bin", 10, 3);
    let s3 = FakeBucket::new();
    s3.take_parts(Some(4));
    let error = drive_on(&s3, Some(&resume), NOW)
        .put_file("big.bin", &path, &no_progress)
        .unwrap_err();
    assert!(matches!(error, DriveError::Transport(_)), "{error:?}");
    assert_eq!(state_files(&resume).len(), 1, "the upload is remembered");
    assert_eq!(s3.uploads().len(), 1, "and not aborted: it resumes");
    assert!(s3.object("big.bin").is_none());

    // The app starts again: a new drive, the same resume folder.
    s3.take_parts(None);
    s3.clear_log();
    let sent = drive_on(&s3, Some(&resume), NOW + 60)
        .put_file("big.bin", &path, &no_progress)
        .unwrap();
    assert_eq!(sent, bytes.len() as u64);
    assert_eq!(s3.object("big.bin").unwrap(), bytes, "the bytes are whole");
    assert_eq!(s3.count("POST start"), 0, "the same upload: {:?}", s3.log());
    assert_eq!(
        s3.count("GET list-parts"),
        1,
        "the service says which parts it has"
    );
    assert_eq!(s3.count("PUT part"), 7, "the 7 parts that were missing");
    assert_eq!(s3.count("POST complete"), 1);
    assert!(
        state_files(&resume).is_empty(),
        "a finished upload leaves no state"
    );
    assert!(s3.uploads().is_empty());
}

#[test]
fn an_upload_whose_file_changed_is_aborted_and_started_again() {
    let tmp = TempDir::new("multipart-changed");
    let resume = tmp.path().join("resume");
    let (path, _) = file_with(&tmp, "big.bin", 10, 4);
    let s3 = FakeBucket::new();
    s3.take_parts(Some(4));
    assert!(drive_on(&s3, Some(&resume), NOW)
        .put_file("big.bin", &path, &no_progress)
        .is_err());
    // Edited in place: the same size, other bytes.
    let (_, edited) = file_with(&tmp, "big.bin", 10, 5);
    s3.take_parts(None);
    s3.clear_log();
    drive_on(&s3, Some(&resume), NOW + 60)
        .put_file("big.bin", &path, &no_progress)
        .unwrap();
    assert_eq!(s3.object("big.bin").unwrap(), edited, "never the old parts");
    assert_eq!(
        s3.count("DELETE abort"),
        1,
        "the old upload: {:?}",
        s3.log()
    );
    assert_eq!(s3.count("POST start"), 1);
    assert_eq!(s3.count("PUT part"), 11);
    assert!(s3.uploads().is_empty());
    assert!(state_files(&resume).is_empty());
}

#[test]
fn a_stale_upload_is_aborted_and_not_resumed() {
    let tmp = TempDir::new("multipart-stale");
    let resume = tmp.path().join("resume");
    let (path, bytes) = file_with(&tmp, "big.bin", 6, 6);
    let s3 = FakeBucket::new();
    s3.take_parts(Some(2));
    assert!(drive_on(&s3, Some(&resume), NOW)
        .put_file("big.bin", &path, &no_progress)
        .is_err());
    s3.take_parts(None);
    s3.clear_log();
    drive_on(&s3, Some(&resume), NOW + STALE_AFTER_SECS + 1)
        .put_file("big.bin", &path, &no_progress)
        .unwrap();
    assert_eq!(s3.object("big.bin").unwrap(), bytes);
    assert_eq!(s3.count("DELETE abort"), 1, "{:?}", s3.log());
    assert_eq!(s3.count("POST start"), 1);
    assert_eq!(s3.count("PUT part"), 7);
}

#[test]
fn an_upload_the_service_forgot_starts_again() {
    let tmp = TempDir::new("multipart-forgot");
    let resume = tmp.path().join("resume");
    let (path, bytes) = file_with(&tmp, "big.bin", 6, 7);
    let s3 = FakeBucket::new();
    s3.take_parts(Some(2));
    assert!(drive_on(&s3, Some(&resume), NOW)
        .put_file("big.bin", &path, &no_progress)
        .is_err());
    s3.forget_uploads();
    s3.take_parts(None);
    s3.clear_log();
    drive_on(&s3, Some(&resume), NOW + 60)
        .put_file("big.bin", &path, &no_progress)
        .unwrap();
    assert_eq!(s3.object("big.bin").unwrap(), bytes);
    assert_eq!(s3.count("GET list-parts"), 1);
    assert_eq!(s3.count("POST start"), 1, "{:?}", s3.log());
    assert_eq!(s3.count("PUT part"), 7);
}

#[test]
fn an_upload_without_a_resume_folder_is_aborted_when_a_part_fails() {
    let tmp = TempDir::new("multipart-no-resume");
    let (path, _) = file_with(&tmp, "big.bin", 6, 8);
    let s3 = FakeBucket::new();
    s3.take_parts(Some(2));
    assert!(drive_on(&s3, None, NOW)
        .put_file("big.bin", &path, &no_progress)
        .is_err());
    assert_eq!(s3.count("DELETE abort"), 1, "{:?}", s3.log());
    assert!(s3.uploads().is_empty(), "no parts are left behind");
}

#[test]
fn a_conditional_streamed_upload_that_loses_is_a_conflict_and_leaves_nothing_behind() {
    let s3 = FakeBucket::new();
    s3.write("taken.bin", b"another device's");
    let body = vec![9u8; 3 * PART + 1];
    let error = drive_on(&s3, None, NOW)
        .put_from_if("taken.bin", &mut &body[..], &Precondition::Absent)
        .unwrap_err();
    assert_eq!(
        error,
        DriveError::Conflict {
            key: String::from("taken.bin")
        }
    );
    assert_eq!(s3.object("taken.bin").unwrap(), b"another device's");
    assert!(
        s3.uploads().is_empty(),
        "the parts are aborted: {:?}",
        s3.log()
    );
}

#[test]
fn a_conditional_streamed_upload_of_a_new_key_wins_with_its_etag() {
    let s3 = FakeBucket::new();
    let body = vec![3u8; 2 * PART + 5];
    let etag = drive_on(&s3, None, NOW)
        .put_from_if("new.bin", &mut &body[..], &Precondition::Absent)
        .unwrap();
    assert!(etag.is_some_and(|e| !e.is_empty() && !e.contains('"')));
    assert_eq!(s3.object("new.bin").unwrap(), body);
    let small = drive_on(&s3, None, NOW)
        .put_from_if("new.bin", &mut &b"x"[..], &Precondition::Absent)
        .unwrap_err();
    assert!(matches!(small, DriveError::Conflict { .. }), "{small:?}");
}

/// `writers` threads, each with a drive of its own, write `key` with `If-None-Match: *` at the
/// same moment; what each got.
fn race(
    s3: &Arc<FakeBucket>,
    key: &'static str,
    writers: usize,
    body_of: impl Fn(usize) -> Vec<u8> + Sync,
    streamed: bool,
) -> Vec<Result<Option<String>, DriveError>> {
    let start = Barrier::new(writers);
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..writers)
            .map(|i| {
                let (start, body_of) = (&start, &body_of);
                let drive = drive_on(s3, None, NOW);
                scope.spawn(move || {
                    let body = body_of(i);
                    start.wait();
                    if streamed {
                        drive.put_from_if(key, &mut &body[..], &Precondition::Absent)
                    } else {
                        drive.put_if(key, &body, &Precondition::Absent)
                    }
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    })
}

#[test]
fn of_two_devices_racing_on_if_none_match_exactly_one_wins() {
    for streamed in [false, true] {
        let s3 = FakeBucket::new();
        let key = if streamed {
            "race/big.bin"
        } else {
            "race/index.json"
        };
        let size = if streamed { 3 * PART + 1 } else { 10 };
        let results = race(&s3, key, 8, |i| vec![i as u8; size], streamed);
        let winners: Vec<usize> = results
            .iter()
            .enumerate()
            .filter(|(_, r)| r.is_ok())
            .map(|(i, _)| i)
            .collect();
        assert_eq!(winners.len(), 1, "streamed {streamed}: {results:?}");
        for (i, result) in results.iter().enumerate() {
            if i != winners[0] {
                assert!(
                    matches!(result, Err(DriveError::Conflict { .. })),
                    "streamed {streamed}: writer {i}: {result:?}"
                );
            }
        }
        assert_eq!(s3.object(key).unwrap(), vec![winners[0] as u8; size]);
        assert!(s3.uploads().is_empty(), "the losers' parts are aborted");
    }
}

#[test]
fn a_sweep_aborts_the_unfinished_uploads_that_are_stale_or_whose_file_is_gone() {
    let tmp = TempDir::new("multipart-sweep");
    let resume = tmp.path().join("resume");
    let (gone, _) = file_with(&tmp, "gone.bin", 4, 9);
    let (kept, _) = file_with(&tmp, "kept.bin", 4, 10);
    let s3 = FakeBucket::new();
    s3.take_parts(Some(0));
    let drive = drive_on(&s3, Some(&resume), NOW);
    assert!(drive.put_file("gone.bin", &gone, &no_progress).is_err());
    assert!(drive.put_file("kept.bin", &kept, &no_progress).is_err());
    assert_eq!(state_files(&resume).len(), 2, "both are remembered");
    std::fs::remove_file(&gone).unwrap();
    assert_eq!(drive.abort_stale_uploads(), 1, "the one whose file is gone");
    assert_eq!(state_files(&resume).len(), 1);
    assert_eq!(s3.uploads().len(), 1, "the other one can still resume");
    let later = drive_on(&s3, Some(&resume), NOW + STALE_AFTER_SECS + 1);
    assert_eq!(
        later.abort_stale_uploads(),
        1,
        "and once it is stale, that one too"
    );
    assert!(state_files(&resume).is_empty());
    assert!(s3.uploads().is_empty());
}
