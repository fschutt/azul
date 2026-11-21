//! Big uploads to a bucket: parts of 16 MiB, four at once; a file's upload outlives the app
//! (a state file per upload in the resume folder: the upload id, the parts' ETags, the file's
//! size, date and the BLAKE3 of its start) and resumes with the parts that are missing; an
//! upload that is stale or whose file changed is aborted, never finished with mixed bytes; a
//! conditional upload that loses is a conflict; of writers racing on `If-None-Match: *` exactly
//! one wins.

use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, Barrier, Mutex,
    },
    time::Duration,
};

use super::{
    fake_bucket::{FakeBucket, BUCKET},
    TempDir,
};
use crate::{
    meta::Certainty,
    multipart::STALE_AFTER_SECS,
    s3::{PARALLEL_PARTS, PART_SIZE},
    transport::CallLimits,
    Credentials, Drive, DriveError, HttpCall, HttpReply, Method, Precondition, S3Config, S3Drive,
    Transport,
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

/// X-DELTA-AZDRIVE-FILES-3. The engine looked at a name once, when a big file's upload began, and
/// the upload wrote over whatever was there when it ended. A file's upload now asks its condition
/// on the request that completes it (S3 checks `If-None-Match` / `If-Match` on
/// CompleteMultipartUpload): a name made while the parts went up is a Conflict, the parts are
/// aborted and the other device's object stays.
#[test]
fn x_delta_azdrive_files_3_a_name_made_while_the_parts_go_up_is_a_conflict_and_the_object_stays() {
    let s3 = FakeBucket::new();
    let dir = TempDir::new("dx3-parts");
    let (path, _) = file_with(&dir, "big.bin", 4, 5);
    let resume = dir.path().join("resume");
    let made = std::sync::atomic::AtomicBool::new(false);
    let ctl = |sent: u64| {
        // The first part is up: another device saves the name now.
        if sent > 0 && !made.swap(true, Ordering::SeqCst) {
            s3.write("taken.bin", b"another device's");
        }
    };
    let error = drive_on(&s3, Some(&resume), NOW)
        .put_file_if("taken.bin", &path, &Precondition::Absent, &ctl)
        .unwrap_err();
    assert_eq!(
        error,
        DriveError::Conflict {
            key: String::from("taken.bin")
        },
        "{:?}",
        s3.log()
    );
    assert_eq!(s3.object("taken.bin").unwrap(), b"another device's");
    assert!(s3.uploads().is_empty(), "aborted: {:?}", s3.log());
    assert!(
        state_files(&resume).is_empty(),
        "no state is kept for a lost race"
    );
    // The version the plan saw is the condition of a replace: a newer one is a conflict too.
    let etag = drive_on(&s3, None, NOW)
        .put_file_if("new.bin", &path, &Precondition::Absent, &no_progress)
        .unwrap()
        .expect("the tag of the new version");
    s3.write("new.bin", b"saved meanwhile");
    let error = drive_on(&s3, None, NOW)
        .put_file_if("new.bin", &path, &Precondition::Matches(etag), &no_progress)
        .unwrap_err();
    assert!(matches!(error, DriveError::Conflict { .. }), "{error:?}");
    assert_eq!(s3.object("new.bin").unwrap(), b"saved meanwhile");
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

#[test]
fn a_streamed_upload_whose_abort_was_refused_is_aborted_before_the_next_upload_starts() {
    // A streamed upload cannot resume, so a failed one is aborted. Credentials that ran out
    // refuse the abort too (and a killed app sends none): the record the upload keeps in the
    // resume folder while it runs lets the drive's next upload abort it.
    let tmp = TempDir::new("multipart-stream-sweep");
    let resume = tmp.path().join("resume");
    let s3 = FakeBucket::new();
    let drive = drive_on(&s3, Some(&resume), NOW);
    s3.take_parts(Some(1));
    s3.refuse_aborts(true);
    let body = vec![7u8; 6 * PART];
    assert!(drive.put_from("streamed.bin", &mut body.as_slice()).is_err());
    assert_eq!(s3.uploads().len(), 1, "the refused abort left the upload");
    s3.take_parts(None);
    s3.refuse_aborts(false);
    let (path, bytes) = file_with(&tmp, "next.bin", 3, 1);
    drive.put_file("next.bin", &path, &no_progress).unwrap();
    assert_eq!(s3.object("next.bin").unwrap(), bytes);
    assert!(
        s3.uploads().is_empty(),
        "the next upload aborted the dead one first: {:?}",
        s3.log()
    );
    assert!(state_files(&resume).is_empty());
}

#[test]
fn a_streamed_upload_under_way_is_never_aborted_by_another_upload() {
    let tmp = TempDir::new("multipart-stream-busy");
    let resume = tmp.path().join("resume");
    let s3 = FakeBucket::new();
    s3.slow_parts(Duration::from_millis(150));
    let streaming = {
        let drive = drive_on(&s3, Some(&resume), NOW);
        std::thread::spawn(move || {
            let body = vec![3u8; 8 * PART];
            drive.put_from("streamed.bin", &mut body.as_slice())
        })
    };
    // The other upload starts while the stream sends its parts.
    while s3.count("POST start streamed.bin") == 0 {
        std::thread::sleep(Duration::from_millis(5));
    }
    let (path, _) = file_with(&tmp, "next.bin", 3, 2);
    drive_on(&s3, Some(&resume), NOW)
        .put_file("next.bin", &path, &no_progress)
        .unwrap();
    streaming.join().unwrap().expect("the stream goes up whole");
    assert_eq!(s3.object("streamed.bin").unwrap(), vec![3u8; 8 * PART]);
    assert_eq!(s3.count("DELETE abort"), 0, "{:?}", s3.log());
    assert!(state_files(&resume).is_empty());
}

/// The answers that say the drive has no room for an upload: over its quota (a node's code, or
/// S3's 507), an object too large for it, a banned drive.
const NO_ROOM: [(u16, &str, Option<&str>); 4] = [
    (507, "InsufficientStorage", None),
    (403, "AccessDenied", Some("quota_exceeded")),
    (413, "EntityTooLarge", Some("object_too_large")),
    (403, "AccessDenied", Some("drive_banned")),
];

/// AZ-M14. The parts of an open upload count against the quota, and a resume would meet the
/// same refusal: an upload the service refused for want of room is aborted and its state
/// forgotten, where it was kept for six days.
#[test]
fn az_m14_an_upload_refused_for_want_of_room_is_aborted_and_forgotten() {
    for (kind, (status, code, azlin)) in NO_ROOM
        .into_iter()
        .flat_map(|answer| [("part", answer), ("complete", answer)])
    {
        let tmp = TempDir::new("multipart-m14-room");
        let resume = tmp.path().join("resume");
        let (path, _) = file_with(&tmp, "big.bin", 6, 11);
        let s3 = FakeBucket::new();
        // The first run is killed after two parts: their bytes sit in the bucket, counted.
        s3.take_parts(Some(2));
        assert!(drive_on(&s3, Some(&resume), NOW)
            .put_file("big.bin", &path, &no_progress)
            .is_err());
        assert_eq!(state_files(&resume).len(), 1);
        assert_eq!(s3.uploads().len(), 1);
        // The next run is refused for want of room.
        s3.take_parts(None);
        s3.refuse(kind, Some((status, code, azlin)));
        s3.clear_log();
        let error = drive_on(&s3, Some(&resume), NOW + 60)
            .put_file("big.bin", &path, &no_progress)
            .unwrap_err();
        assert!(matches!(error, DriveError::Service(_)), "{error:?}");
        assert_eq!(s3.count("DELETE abort"), 1, "{kind} {code}: {:?}", s3.log());
        assert!(
            s3.uploads().is_empty(),
            "{kind} {code}: the parts that counted are given back"
        );
        assert!(
            state_files(&resume).is_empty(),
            "{kind} {code}: nothing is left to resume"
        );
    }
}

/// AZ-M14. Credentials that ran out, a node that is busy, a drive that is unpaid: the same
/// upload goes on once it is over, so it stays.
#[test]
fn az_m14_an_upload_refused_for_another_reason_is_kept_for_the_resume() {
    for (kind, (status, code, azlin)) in [
        ("part", (403, "ExpiredToken", Some("session_expired"))),
        ("part", (503, "ServiceUnavailable", Some("unavailable"))),
        ("part", (403, "AccessDenied", Some("read_only_unpaid"))),
        ("complete", (403, "ExpiredToken", Some("session_expired"))),
        ("complete", (503, "ServiceUnavailable", Some("unavailable"))),
    ] {
        let tmp = TempDir::new("multipart-m14-kept");
        let resume = tmp.path().join("resume");
        let (path, bytes) = file_with(&tmp, "big.bin", 6, 12);
        let s3 = FakeBucket::new();
        s3.take_parts(Some(2));
        assert!(drive_on(&s3, Some(&resume), NOW)
            .put_file("big.bin", &path, &no_progress)
            .is_err());
        s3.take_parts(None);
        s3.refuse(kind, Some((status, code, azlin)));
        s3.clear_log();
        assert!(drive_on(&s3, Some(&resume), NOW + 60)
            .put_file("big.bin", &path, &no_progress)
            .is_err());
        assert_eq!(s3.count("DELETE abort"), 0, "{kind} {code}: {:?}", s3.log());
        assert_eq!(s3.uploads().len(), 1, "{kind} {code}: the upload stays");
        assert_eq!(
            state_files(&resume).len(),
            1,
            "{kind} {code}: and its state"
        );
        // The refusal ends: the same upload is finished, with the parts it holds.
        s3.refuse(kind, None);
        s3.clear_log();
        drive_on(&s3, Some(&resume), NOW + 120)
            .put_file("big.bin", &path, &no_progress)
            .unwrap();
        assert_eq!(s3.object("big.bin").unwrap(), bytes);
        assert_eq!(s3.count("POST start"), 0, "{kind} {code}: {:?}", s3.log());
        assert!(state_files(&resume).is_empty());
    }
}

/// AZ-M14. A sweep aborts an upload for a file that is gone or other; a file that exists and
/// cannot be looked at just now (a folder this user may not enter, a volume that does not
/// answer) says nothing, and its upload stays.
#[cfg(unix)]
#[test]
fn az_m14_a_sweep_does_not_abort_an_upload_whose_file_it_cannot_read_now() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = TempDir::new("multipart-m14-unreadable");
    let resume = tmp.path().join("resume");
    let shelf = tmp.path().join("shelf");
    std::fs::create_dir_all(&shelf).unwrap();
    let (path, _) = file_with(&tmp, "shelf/big.bin", 4, 14);
    let s3 = FakeBucket::new();
    s3.take_parts(Some(0));
    let drive = drive_on(&s3, Some(&resume), NOW);
    assert!(drive.put_file("big.bin", &path, &no_progress).is_err());
    assert_eq!(state_files(&resume).len(), 1);
    // The shelf is shut: the file is there, and this process cannot look at it.
    std::fs::set_permissions(&shelf, std::fs::Permissions::from_mode(0o000)).unwrap();
    let swept = if std::fs::metadata(&path).is_ok() {
        // A user who reads everything (root): nothing can be made unreadable - and that run fails
        // `x_delta_integrity3_19_the_suite_runs_as_a_user_a_mode_of_000_binds`, so this is no quiet pass.
        None
    } else {
        Some(drive.abort_stale_uploads())
    };
    std::fs::set_permissions(&shelf, std::fs::Permissions::from_mode(0o755)).unwrap();
    let Some(swept) = swept else {
        return;
    };
    assert_eq!(
        swept,
        0,
        "an unreadable file is no gone file: {:?}",
        s3.log()
    );
    assert_eq!(state_files(&resume).len(), 1, "the upload is remembered");
    assert_eq!(s3.uploads().len(), 1, "and not aborted");
    // Once the file is really gone, the upload goes with it.
    std::fs::remove_file(&path).unwrap();
    assert_eq!(drive.abort_stale_uploads(), 1);
    assert!(state_files(&resume).is_empty());
    assert!(s3.uploads().is_empty());
}

/// Whether this process is refused a file of `mode` (made in a folder of its own): `false` for root, and on a
/// mount that ignores permission bits - the two ways a mode of 000 binds nobody.
#[cfg(unix)]
fn a_mode_binds(mode: u32) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new("multipart-mode-binds");
    let file = dir.path().join("f");
    std::fs::write(&file, b"x").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).unwrap();
    let refused = std::fs::read(&file).is_err();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    refused
}

/// Fails the run, saying why, unless a file of `mode` is refused to this process.
#[cfg(unix)]
fn demand_that_a_mode_binds(mode: u32) {
    assert!(
        a_mode_binds(mode),
        "this process reads a file of mode {mode:o} (it runs as root, or on a mount that ignores \
         permission bits), so the test that shuts a folder this process cannot look into - \
         az_m14_a_sweep_does_not_abort_an_upload_whose_file_it_cannot_read_now - returns early and \
         asserts nothing: this run cannot say it passes. Run the suite as an ordinary user \
         (X-DELTA-INTEGRITY3-19)"
    );
}

/// X-DELTA-INTEGRITY3-19, the twin of the kit's test of the same name. The test of an unreadable upload
/// file returns early where the process reads a mode of 000 anyway - root, a CI container, a mount that
/// ignores modes - and reported green on the runner that cannot play it. This is where that run fails.
#[cfg(unix)]
#[test]
fn x_delta_integrity3_19_the_suite_runs_as_a_user_a_mode_of_000_binds() {
    demand_that_a_mode_binds(0o000);
}

/// The control of the test above, at the same size: a process that reads the file - root's case, made here
/// with a mode that lets the owner read it - fails the run with the words of the loud test.
#[cfg(unix)]
#[test]
#[should_panic(expected = "this process reads a file of mode 600")]
fn x_delta_integrity3_19_a_process_that_reads_through_the_mode_fails_the_run() {
    demand_that_a_mode_binds(0o600);
}

/// AZ-M14. The sweep of one drive never aborts the upload another thread of the process is
/// sending, however stale it looks to the drive's clock.
#[test]
fn az_m14_a_sweep_never_aborts_an_upload_this_process_is_sending() {
    let tmp = TempDir::new("multipart-m14-busy");
    let resume = tmp.path().join("resume");
    let (path, bytes) = file_with(&tmp, "big.bin", 8, 13);
    let s3 = FakeBucket::new();
    s3.slow_parts(Duration::from_millis(150));
    let sending = {
        let drive = drive_on(&s3, Some(&resume), NOW);
        let path = path.clone();
        std::thread::spawn(move || drive.put_file("big.bin", &path, &no_progress))
    };
    // The state file is there once the upload started; its parts take their time.
    while state_files(&resume).is_empty() {
        std::thread::sleep(Duration::from_millis(5));
    }
    let swept = drive_on(&s3, Some(&resume), NOW + STALE_AFTER_SECS + 1).abort_stale_uploads();
    assert_eq!(swept, 0, "{:?}", s3.log());
    sending.join().unwrap().expect("the upload goes up whole");
    assert_eq!(s3.object("big.bin").unwrap(), bytes);
    assert_eq!(s3.count("DELETE abort"), 0, "{:?}", s3.log());
    assert!(state_files(&resume).is_empty());
}

/// X-DELTA-SYNC2-9. A conditional upload that lost its race (the request that completes it answered
/// 412: a name another device made) aborted its parts with `let _ =` and forgot its record at once:
/// an abort the service refused (credentials that ran out, a network that dropped) left the parts
/// on the service, billed, until it expired them, and nothing on this computer knew them any more.
/// The record stays now, so that a sweep aborts them; and the user is told. RED on the old code: no
/// state file is left, the sweep finds nothing and the upload stays for good.
#[test]
fn x_delta_sync2_9_an_abort_the_service_refused_after_a_lost_race_is_not_forgotten() {
    let s3 = FakeBucket::new();
    let dir = TempDir::new("dx29-abort");
    let (path, _) = file_with(&dir, "big.bin", 4, 5);
    let resume = dir.path().join("resume");
    let made = std::sync::atomic::AtomicBool::new(false);
    let ctl = |sent: u64| {
        // The first part is up: another device saves the name now.
        if sent > 0 && !made.swap(true, Ordering::SeqCst) {
            s3.write("taken.bin", b"another device's");
        }
    };
    s3.refuse_aborts(true);
    let error = drive_on(&s3, Some(&resume), NOW)
        .put_file_if("taken.bin", &path, &Precondition::Absent, &ctl)
        .unwrap_err();
    assert!(matches!(error, DriveError::Conflict { .. }), "{error:?}");
    assert_eq!(s3.object("taken.bin").unwrap(), b"another device's");
    assert_eq!(
        s3.uploads().len(),
        1,
        "the refused abort left the parts on the service"
    );
    assert_eq!(
        state_files(&resume).len(),
        1,
        "and the record that lets a sweep abort them stays"
    );

    // The service answers again; once the upload is stale a sweep aborts it.
    s3.refuse_aborts(false);
    let later = drive_on(&s3, Some(&resume), NOW + STALE_AFTER_SECS + 1);
    assert_eq!(later.abort_stale_uploads(), 1, "{:?}", s3.log());
    assert!(s3.uploads().is_empty(), "{:?}", s3.log());
    assert!(state_files(&resume).is_empty());
}

// ==== X-EDGE-A-2 (E9b, T14): a part whose body stops without an error ====

/// A drive's transport in virtual time for a part that stalls: the first `answered` parts reach the bucket;
/// every part after them goes out and its body stops - no error, no answer - until the budget of its call is
/// spent, and then the client gives up as azul's `HttpClient` does ("timed out", the call's own
/// `send_body`). The wait is not slept: each stalled call's budget is recorded, so the test reads the time
/// the client was given and runs in milliseconds. (The enforcement of that budget is the HTTP client's,
/// `azul_transport.rs` `with_send_timeout`; what this crate decides is the budget a part's call carries and
/// what it does with the answer.)
struct Stalling {
    bucket: Arc<FakeBucket>,
    answered: usize,
    arrived: AtomicUsize,
    /// Each stalled call: the bytes of its body and the time its client waited for it.
    stalled: Mutex<Vec<(usize, Duration)>>,
}

impl Transport for Stalling {
    fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
        let is_part = call.method == Method::Put && call.url.contains("partNumber=");
        if is_part && self.arrived.fetch_add(1, Ordering::SeqCst) >= self.answered {
            let budget = call.budget();
            let waited = budget
                .send_body
                .map_or(budget.overall, |send| send.min(budget.overall));
            self.stalled.lock().unwrap().push((call.body.len(), waited));
            return Err(format!(
                "timed out after {} s: the body stopped going out",
                waited.as_secs()
            ));
        }
        self.bucket.answer(call)
    }
}

struct StallingHandle(Arc<Stalling>);

impl Transport for StallingHandle {
    fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
        self.0.send(call)
    }
}

/// X-EDGE-A-2 (T14). A part whose body stops going out without an error is given up when the budget of its
/// call is spent - an upload's: its base and the time its bytes take at the least rate, so a part is not
/// given the flat minute of a document - and the error is the transport's, which a pass retries inside its
/// budget; the upload stays, and the restart sends only the parts the service does not have. The first
/// four parts are answered, the next ones stall; every stalled call waited exactly
/// `CallLimits::upload().budget(<its body>).send_body`. RED when a part's call carries another limit (a
/// document's flat minute): the clients would wait 60 s for a part whose budget is 30 s and the bytes.
#[test]
fn a_part_stalled_without_error_is_abandoned_at_its_budget_and_resumed() {
    const ANSWERED: usize = 4;
    let tmp = TempDir::new("multipart-stall");
    let resume = tmp.path().join("resume");
    let (path, bytes) = file_with(&tmp, "big.bin", 10, 9);
    let s3 = FakeBucket::new();
    let stalling = Arc::new(Stalling {
        bucket: s3.clone(),
        answered: ANSWERED,
        arrived: AtomicUsize::new(0),
        stalled: Mutex::new(Vec::new()),
    });
    let stalled_drive = S3Drive::new(
        S3Config {
            endpoint: String::from("http://127.0.0.1:9000"),
            region: String::from("us-east-1"),
            bucket: BUCKET.to_string(),
            path_style: true,
        },
        Credentials::new("AKIDTEST", "test-secret"),
        Box::new(StallingHandle(stalling.clone())),
    )
    .unwrap()
    .with_clock(|| NOW)
    .with_part_size(PART)
    .with_resume_dir(&resume);
    let error = stalled_drive
        .put_file("big.bin", &path, &no_progress)
        .unwrap_err();
    // Given up at the budget of an upload with that body: no sooner, no later.
    let stalled = stalling.stalled.lock().unwrap().clone();
    assert!(!stalled.is_empty(), "no part stalled");
    for (len, waited) in &stalled {
        let allowed = CallLimits::upload()
            .budget(*len as u64)
            .send_body
            .expect("an upload's call has a time of its own for its body");
        assert_eq!(
            *waited, allowed,
            "a part of {len} bytes was given {waited:?}; the budget of an upload of that size is {allowed:?}"
        );
    }
    assert!(
        stalled.iter().any(|(len, _)| *len == PART),
        "a whole part stalled: {stalled:?}"
    );
    // A network fault: the pass retries it inside its budget. Whether the part landed is not known - never a
    // refusal, which a caller may take for proof that nothing was written.
    assert!(
        matches!(&error, DriveError::Transport(message) if message.contains("timed out")),
        "{error:?}"
    );
    assert_eq!(error.certainty(), Certainty::Unknown, "{error:?}");
    // What reached the service stays, and the upload is remembered, not aborted.
    assert_eq!(s3.count("PUT part"), ANSWERED, "{:?}", s3.log());
    assert_eq!(state_files(&resume).len(), 1, "the upload is remembered");
    assert_eq!(s3.uploads().len(), 1, "and not aborted: it resumes");
    // The restart, over a drive that answers: only the parts the service does not have go up.
    s3.clear_log();
    let sent = drive_on(&s3, Some(&resume), NOW + 60)
        .put_file("big.bin", &path, &no_progress)
        .unwrap();
    assert_eq!(sent, bytes.len() as u64);
    assert_eq!(s3.object("big.bin").unwrap(), bytes, "the bytes are whole");
    assert_eq!(s3.count("POST start"), 0, "the same upload: {:?}", s3.log());
    assert_eq!(
        s3.count("PUT part"),
        11 - ANSWERED,
        "only the missing parts: {:?}",
        s3.log()
    );
    assert!(
        state_files(&resume).is_empty(),
        "a finished upload leaves no state"
    );
}
