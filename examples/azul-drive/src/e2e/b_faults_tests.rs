//! The hostile server's and network's faults of the stand-in remote (`b_faults`), each on the
//! requests a sync sends: what it refuses, with which answer, and what it lets through.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use azcloud_kit::sync::remote::{blob_key, index_key};
use azul_storage::{testing::TempDir, AzlinError, BatchPut, Drive, DriveError, ListRequest};

use super::{
    b_faults::{arm, clear_remote, FileFaults, ServerFaults},
    files::hash,
    remote::{FaultDrive, Remote},
};

fn remote(dir: &TempDir, name: &str) -> Arc<Remote> {
    Arc::new(Remote::create(name, dir.path().join(name), false, "").expect("a remote"))
}

fn drive(remote: &Arc<Remote>, device: &str) -> FaultDrive {
    FaultDrive {
        remote: remote.clone(),
        device: device.to_string(),
    }
}

fn status_of(result: Result<impl std::fmt::Debug, DriveError>) -> (u16, Option<AzlinError>) {
    match result {
        Err(DriveError::Service(e)) => (e.status, e.azlin_error),
        other => panic!("a service error was expected, got {other:?}"),
    }
}

#[test]
fn a_read_only_drive_refuses_new_content_but_takes_the_index_commit_and_deletes() {
    let dir = TempDir::new("azdrive-b-read-only");
    let r = remote(&dir, "r");
    let d = drive(&r, "a");
    d.put("old.txt", b"kept").expect("before the fault");
    arm(&r, "", ServerFaults { read_only: Some(AzlinError::ReadOnlyUnpaid), ..ServerFaults::default() });
    let blob = blob_key("", &hash(b"new"));
    assert_eq!(status_of(d.put(&blob, b"new")), (403, Some(AzlinError::ReadOnlyUnpaid)));
    assert_eq!(status_of(d.put("plain.txt", b"new")).0, 403);
    d.put(&index_key(""), b"{\"files\": {}}").expect("the index commit of a delete lands");
    d.delete("old.txt").expect("the owner's delete still works");
    assert!(d.get(&index_key("")).is_ok(), "reads still work");
    assert!(r.counters(Some("a")).refused >= 2);
    clear_remote(&r);
}

#[test]
fn a_moved_server_answers_every_request_with_a_permanent_redirect() {
    let dir = TempDir::new("azdrive-b-moved");
    let r = remote(&dir, "r");
    let d = drive(&r, "a");
    arm(&r, "a", ServerFaults { moved: Some(String::from("https://new.e2e.invalid")), ..ServerFaults::default() });
    assert_eq!(status_of(d.get("x")).0, 301);
    assert_eq!(status_of(d.list(&ListRequest::folder(""))).0, 301);
    // Another device is not moved.
    drive(&r, "b").put("x", b"b reaches it").expect("b's write");
    clear_remote(&r);
}

#[test]
fn an_untrusted_certificate_is_a_transport_error_naming_the_certificate() {
    let dir = TempDir::new("azdrive-b-tls");
    let r = remote(&dir, "r");
    arm(&r, "a", ServerFaults { tls_untrusted: true, ..ServerFaults::default() });
    match drive(&r, "a").get("x") {
        Err(DriveError::Transport(why)) => assert!(why.contains("certificate"), "{why}"),
        other => panic!("a transport error was expected, got {other:?}"),
    }
    clear_remote(&r);
}

#[test]
fn a_hung_connection_times_out_after_its_wait_and_a_cleared_one_gives_up_at_once() {
    let dir = TempDir::new("azdrive-b-hang");
    let r = remote(&dir, "r");
    arm(&r, "a", ServerFaults { hang: Some(Duration::from_millis(150)), ..ServerFaults::default() });
    let started = Instant::now();
    match drive(&r, "a").get("x") {
        Err(DriveError::Transport(why)) => assert!(why.contains("timed out"), "{why}"),
        other => panic!("a timeout was expected, got {other:?}"),
    }
    assert!(started.elapsed() >= Duration::from_millis(150), "it waited for the timeout");
    arm(&r, "a", ServerFaults { hang: Some(Duration::from_secs(600)), ..ServerFaults::default() });
    let r2 = r.clone();
    let waiting = std::thread::spawn(move || drive(&r2, "a").get("x"));
    std::thread::sleep(Duration::from_millis(50));
    let cleared = Instant::now();
    clear_remote(&r);
    assert!(waiting.join().expect("the request ends").is_err());
    assert!(cleared.elapsed() < Duration::from_secs(30), "the cleared hang let go at once");
}

#[test]
fn a_short_body_is_an_error_or_a_short_success_as_report_ok_says() {
    let dir = TempDir::new("azdrive-b-short");
    let r = remote(&dir, "r");
    let d = drive(&r, "a");
    d.put("big", &[7u8; 100]).expect("the object");
    arm(&r, "a", ServerFaults { short_body_after: Some(40), report_ok: true, ..ServerFaults::default() });
    assert_eq!(d.get("big").expect("a short success").len(), 40);
    arm(&r, "a", ServerFaults { short_body_after: Some(40), report_ok: false, ..ServerFaults::default() });
    let before = r.counters(Some("a")).bytes_down;
    assert!(matches!(d.get("big"), Err(DriveError::Transport(_))));
    assert_eq!(r.counters(Some("a")).bytes_down - before, 40, "the bytes that came are counted");
    d.put("small", b"tiny").expect("a small object");
    assert_eq!(d.get("small").expect("a body under the cut is whole"), b"tiny");
    clear_remote(&r);
}

#[test]
fn the_object_cap_refuses_big_content_with_413_and_the_file_cap_refuses_a_full_commit() {
    let dir = TempDir::new("azdrive-b-caps");
    let r = remote(&dir, "r");
    let d = drive(&r, "a");
    arm(&r, "", ServerFaults { max_object_bytes: Some(10), max_files: Some(2), ..ServerFaults::default() });
    let big = blob_key("", &hash(&[1u8; 11]));
    assert_eq!(status_of(d.put(&big, &[1u8; 11])), (413, Some(AzlinError::ObjectTooLarge)));
    d.put(&blob_key("", &hash(b"small")), b"small").expect("under the cap");
    let two = br#"{"files": {"a": {}, "b": {}}}"#;
    let three = br#"{"files": {"a": {}, "b": {}, "c": {}}}"#;
    d.put(&index_key(""), two).expect("two files fit");
    assert_eq!(status_of(d.put(&index_key(""), three)), (507, Some(AzlinError::QuotaExceeded)));
    clear_remote(&r);
}

#[test]
fn another_remote_answers_a_redirected_device() {
    let dir = TempDir::new("azdrive-b-answer-from");
    let r1 = remote(&dir, "r1");
    let r2 = remote(&dir, "r2");
    drive(&r2, "x").put("k", b"the other account's").expect("r2's object");
    drive(&r1, "x").put("k", b"mine").expect("r1's object");
    arm(&r1, "a", ServerFaults { answer_from: Some(r2.clone()), ..ServerFaults::default() });
    assert_eq!(drive(&r1, "a").get("k").expect("an answer"), b"the other account's");
    assert_eq!(drive(&r1, "b").get("k").expect("b's answer"), b"mine");
    clear_remote(&r1);
}

#[test]
fn a_failing_file_on_a_plain_remote_fails_its_blob_found_by_its_local_hash() {
    let dir = TempDir::new("azdrive-b-fail-files");
    let r = remote(&dir, "r");
    let root = dir.path().join("a-root");
    std::fs::create_dir_all(root.join("big")).unwrap();
    std::fs::write(root.join("big/stuck.bin"), "the one that keeps failing").unwrap();
    std::fs::write(root.join("ok.txt"), "fine").unwrap();
    arm(&r, "a", ServerFaults {
        fail_files: vec![String::from("big/stuck.bin")],
        roots: vec![root],
        ..ServerFaults::default()
    });
    let d = drive(&r, "a");
    let stuck = blob_key("", &hash(b"the one that keeps failing"));
    assert_eq!(status_of(d.put(&stuck, b"the one that keeps failing")).0, 500);
    d.put(&blob_key("", &hash(b"fine")), b"fine").expect("the other file goes up");
    clear_remote(&r);
}

#[test]
fn a_failing_file_above_the_encryption_fails_its_calls_and_only_its_items_of_a_batch() {
    let dir = TempDir::new("azdrive-b-file-faults");
    let r = remote(&dir, "r");
    let files = FileFaults {
        inner: Arc::new(drive(&r, "a")),
        remote: r.clone(),
        device: String::from("a"),
    };
    arm(&r, "a", ServerFaults { fail_files: vec![String::from("big/*")], ..ServerFaults::default() });
    assert_eq!(status_of(files.put("big/stuck.bin", b"x")).0, 500);
    let answers = files.put_many(&[
        BatchPut::new("a.txt", b"a"),
        BatchPut::new("big/stuck.bin", b"x"),
        BatchPut::new("b.txt", b"b"),
    ]);
    assert!(answers[0].is_ok() && answers[2].is_ok(), "{answers:?}");
    assert!(answers[1].is_err(), "{answers:?}");
    assert_eq!(r.read("a.txt").as_deref(), Some(&b"a"[..]));
    assert!(r.read("big/stuck.bin").is_none(), "nothing of the failing file went up");
    clear_remote(&r);
}
