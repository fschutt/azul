//! A pass crashed at a named point of the fault seam (`sync::faults`), the way a harness plays
//! a process killed there: what the drive and the folder hold afterwards, and the next pass.

use std::panic::{catch_unwind, AssertUnwindSafe};

use super::{
    fake_s3::S3Bucket,
    sync::{index_of, Device, PREFIX},
};
use crate::sync::{faults, remote::index_key};

#[test]
fn a_pass_crashed_after_its_blobs_went_up_commits_no_index_and_the_next_pass_does() {
    let store = S3Bucket::new();
    let a = Device::new("crash-a");
    let b = Device::new("crash-b");
    a.write("f.txt", b"one");
    faults::arm_crash(a.folder(), "after-blob");
    let crashed = catch_unwind(AssertUnwindSafe(|| a.sync(&store)));
    let payload = crashed.expect_err("the pass crashes at after-blob");
    assert_eq!(faults::crashed_at(payload.as_ref()).as_deref(), Some("after-blob"));
    assert!(store.read(&index_key(PREFIX)).is_none(), "no index was committed");
    let again = a.sync(&store);
    assert!(again.index_written);
    assert_eq!(again.files_up, 1);
    b.sync(&store);
    assert_eq!(b.read("f.txt").as_deref(), Some(&b"one"[..]));
    faults::clear(a.folder());
}

#[test]
fn a_pass_crashed_after_its_commit_leaves_the_index_written_and_the_next_pass_sends_nothing_again() {
    let store = S3Bucket::new();
    let a = Device::new("commit-a");
    a.write("f.txt", b"two");
    faults::arm_crash(a.folder(), "after-commit");
    assert!(catch_unwind(AssertUnwindSafe(|| a.sync(&store))).is_err());
    assert!(index_of(&store).files.contains_key("f.txt"), "the commit landed");
    assert_eq!(faults::crashes(a.folder()), 1);
    // The base was never saved: the next pass finds the drive already holds this file.
    let again = a.sync(&store);
    assert_eq!(again.blobs_up, 0, "the blob is not sent twice");
    assert!(!again.index_written, "nothing new to commit");
    assert_eq!(a.read("f.txt").as_deref(), Some(&b"two"[..]));
    faults::clear(a.folder());
}
