//! The E2E commands' stand-in remote, driven by azcloud-kit's real folder sync: two devices
//! converge through it, its conditional writes are atomic, and a fault cuts off only the device
//! it is armed for.

use std::sync::{atomic::AtomicBool, Arc};

use azcloud_kit::sync::{
    drive_store::DriveStore,
    session::{SyncSession, SyncSetup},
};
use azul_storage::{testing::TempDir, Drive, DriveError, ListRequest, Precondition};

use super::remote::{FaultDrive, Remote};

fn remote(dir: &TempDir) -> Arc<Remote> {
    Arc::new(Remote::create("r", dir.path().join("remote"), false, "").expect("a remote"))
}

fn drive(remote: &Arc<Remote>, device: &str) -> Arc<FaultDrive> {
    Arc::new(FaultDrive {
        remote: remote.clone(),
        device: device.to_string(),
    })
}

fn session(dir: &TempDir, remote: &Arc<Remote>, device: &str) -> SyncSession {
    let folder = dir.path().join(device).join("root");
    std::fs::create_dir_all(&folder).expect("the folder");
    let store = Arc::new(DriveStore::new(drive(remote, device) as Arc<dyn Drive>));
    SyncSession::plain(
        SyncSetup::new(&format!("e2e-{device}"), "", &folder),
        dir.path().join(device).join("state"),
        device,
        store,
    )
}

fn args(v: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    v.as_object().cloned().unwrap_or_default()
}

#[test]
fn two_devices_converge_through_the_stand_in_remote() {
    let dir = TempDir::new("azdrive-e2e-remote-converge");
    let r = remote(&dir);
    let a = session(&dir, &r, "a");
    let b = session(&dir, &r, "b");
    std::fs::write(dir.path().join("a/root/x.txt"), "from a").unwrap();
    a.pass(&AtomicBool::new(false), &mut |_| {}).expect("a's pass");
    b.pass(&AtomicBool::new(false), &mut |_| {}).expect("b's pass");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("b/root/x.txt")).ok().as_deref(),
        Some("from a")
    );
}

#[test]
fn a_partition_cuts_off_only_the_device_it_is_armed_for_and_counts_the_refusals() {
    let dir = TempDir::new("azdrive-e2e-remote-partition");
    let r = remote(&dir);
    let a = session(&dir, &r, "a");
    let b = session(&dir, &r, "b");
    r.set_faults("b", false, &args(serde_json::json!({ "partition": true })));
    std::fs::write(dir.path().join("b/root/y.txt"), "from b").unwrap();
    assert!(b.pass(&AtomicBool::new(false), &mut |_| {}).is_err(), "b is cut off");
    assert!(r.counters(Some("b")).refused >= 1);
    a.pass(&AtomicBool::new(false), &mut |_| {}).expect("a still reaches the drive");
    r.set_faults("b", true, &serde_json::Map::new());
    b.pass(&AtomicBool::new(false), &mut |_| {}).expect("b's pass after the partition");
    a.pass(&AtomicBool::new(false), &mut |_| {}).expect("a's pass");
    assert!(dir.path().join("a/root/y.txt").exists(), "b's file reached a afterwards");
}

#[test]
fn a_conditional_write_holds_only_against_the_version_it_read() {
    let dir = TempDir::new("azdrive-e2e-remote-cas");
    let r = remote(&dir);
    let d = drive(&r, "a");
    let first = d
        .put_if("k", b"one", &Precondition::Absent)
        .expect("the first write")
        .expect("an entity tag");
    assert!(matches!(
        d.put_if("k", b"two", &Precondition::Absent),
        Err(DriveError::Conflict { .. })
    ));
    d.put_if("k", b"two", &Precondition::Matches(first.clone()))
        .expect("the write against the version read");
    assert!(matches!(
        d.put_if("k", b"three", &Precondition::Matches(first)),
        Err(DriveError::Conflict { .. })
    ));
    assert_eq!(d.get("k").expect("the object"), b"two");
}

#[test]
fn a_listing_pages_and_folds_folders_like_a_bucket() {
    let dir = TempDir::new("azdrive-e2e-remote-list");
    let r = remote(&dir);
    let d = drive(&r, "a");
    for key in ["a/1", "a/2", "b", "c/d/e", "C"] {
        d.put(key, b"x").expect("a write");
    }
    let page = d.list(&ListRequest::folder("")).expect("a listing");
    assert_eq!(page.folders, vec!["a/".to_string(), "c/".to_string()]);
    let names: Vec<&str> = page.objects.iter().map(|o| o.key.as_str()).collect();
    assert_eq!(names, vec!["C", "b"], "keys differing in case stay apart");
    let first = d
        .list(&ListRequest::recursive("").with_max_keys(2))
        .expect("the first page");
    assert_eq!(first.objects.len(), 2);
    let mut rest = ListRequest::recursive("");
    rest.continuation = first.next.clone();
    let second = d.list(&rest).expect("the second page");
    assert_eq!(second.objects.len(), 3);
}

#[test]
fn a_quota_refuses_the_write_that_would_pass_it() {
    let dir = TempDir::new("azdrive-e2e-remote-quota");
    let r = remote(&dir);
    let d = drive(&r, "a");
    r.set_faults("", false, &args(serde_json::json!({ "quota_bytes": 10 })));
    d.put("small", b"12345").expect("under the quota");
    match d.put("big", b"1234567890") {
        Err(DriveError::Service(e)) => assert_eq!(e.code, "QuotaExceeded"),
        other => panic!("the quota refuses, got {other:?}"),
    }
}

#[test]
fn writes_at_the_same_time_cannot_both_take_the_last_room() {
    let dir = TempDir::new("azdrive-e2e-remote-quota-race");
    let r = remote(&dir);
    r.set_faults("", false, &args(serde_json::json!({ "quota_bytes": 1000 })));
    let body = vec![7u8; 600];
    for round in 0..20 {
        let barrier = std::sync::Barrier::new(2);
        let sent: Vec<bool> = std::thread::scope(|scope| {
            let writers: Vec<_> = ["a", "b"]
                .into_iter()
                .map(|device| {
                    let (d, barrier, body) = (drive(&r, device), &barrier, &body);
                    scope.spawn(move || {
                        barrier.wait();
                        d.put(&format!("{round}-{device}"), body).is_ok()
                    })
                })
                .collect();
            writers.into_iter().map(|w| w.join().unwrap()).collect()
        });
        assert_eq!(sent.iter().filter(|s| **s).count(), 1, "round {round}: {sent:?}");
        for device in ["a", "b"] {
            let _ = r.remove(&format!("{round}-{device}"));
        }
    }
}

/// An encrypted remote's key files are what tells a device's drive it is encrypted (when its
/// keyring holds no key, after a lock): the stand-in lists them as a bucket does, also through
/// the retrying connection the devices open it with.
#[cfg(feature = "encryption")]
#[test]
fn the_key_files_of_an_encrypted_remote_are_listed_through_a_devices_connection() {
    let dir = TempDir::new("azdrive-e2e-remote-keys");
    let r = Arc::new(Remote::create("r", dir.path().join("remote"), true, "").expect("a remote"));
    r.write(".azlin/keys/recovery.key", b"wrap").unwrap();
    r.write("data/aa/aa00", b"object").unwrap();
    let plain = drive(&r, "a");
    assert!(azul_storage::crypto::device::is_encrypted(plain.as_ref()).unwrap());
    let connection = crate::connection(plain);
    assert!(azul_storage::crypto::device::is_encrypted(connection.as_ref()).unwrap());
}
