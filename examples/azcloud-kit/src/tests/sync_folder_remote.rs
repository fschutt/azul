//! The harness's drive folder as a drive side: entries in its format both ways, its faults,
//! and whatever another party wrote never taken for a delete.

use std::{fs, path::Path};

use azul_storage::testing::TempDir;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::sync::{
    folder_remote::{self, FolderRemote},
    local::path_of,
    problems::Code,
    LocalRoot, RunHooks, SyncOptions, SyncReport,
};

fn clock() -> Box<dyn Fn() -> f64 + Sync> {
    Box::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64())
    })
}

struct Device {
    name: &'static str,
    folder: TempDir,
    state: TempDir,
}

impl Device {
    fn new(name: &'static str) -> Device {
        Device {
            name,
            folder: TempDir::new(&format!("folder-remote-root-{name}")),
            state: TempDir::new(&format!("folder-remote-state-{name}")),
        }
    }

    fn pass(&self, drive: &Path) -> SyncReport {
        let remote = FolderRemote::new(drive, self.name, clock()).unwrap();
        let root = LocalRoot::folder(self.folder.path());
        let opts = SyncOptions::new("", "harness", self.name).unwrap();
        folder_remote::sync(
            &remote,
            &root,
            &self.state.path().join("index.json"),
            &opts,
            &RunHooks::default(),
        )
        .unwrap()
    }

    fn write(&self, key: &str, bytes: &[u8]) {
        let path = path_of(self.folder.path(), key);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn read(&self, key: &str) -> Option<Vec<u8>> {
        fs::read(path_of(self.folder.path(), key)).ok()
    }
}

fn index(drive: &Path) -> Value {
    serde_json::from_slice(&fs::read(drive.join("index.json")).unwrap()).unwrap()
}

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Puts `key` as another party would (the harness's `tamper_put`), with `extra` fields.
fn tamper_put(drive: &Path, key: &str, data: &[u8], extra: Value) {
    let h = sha(data);
    let dir = drive.join("blobs").join(&h[..2]);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(&h), data).unwrap();
    let mut doc = index(drive);
    let gen = doc["generation"].as_u64().unwrap() + 1;
    let mut entry = json!({"hash": h, "size": data.len(), "blob": h, "kind": "file", "exec": false, "mtime": 0, "gen": gen, "device": "peer"});
    if let (Some(e), Some(x)) = (entry.as_object_mut(), extra.as_object()) {
        e.extend(x.clone());
    }
    doc["files"][key] = entry;
    doc["generation"] = json!(gen);
    fs::write(drive.join("index.json"), serde_json::to_vec_pretty(&doc).unwrap()).unwrap();
}

#[test]
fn a_file_goes_up_in_the_harness_format_and_comes_down_on_another_device() {
    let drive = TempDir::new("folder-remote-drive");
    let a = Device::new("a");
    let b = Device::new("b");
    a.write("docs/x.txt", b"hello");
    let up = a.pass(drive.path());
    assert!(up.index_written, "{}", up.summary());
    let doc = index(drive.path());
    let entry = &doc["files"]["docs/x.txt"];
    assert_eq!(entry["hash"], json!(sha(b"hello")), "the harness checks SHA-256");
    assert_eq!(entry["blob"], entry["hash"]);
    assert_eq!(entry["device"], "a");
    assert_eq!(doc["generation"], 1);
    assert!(drive.path().join("index-history/1.json").exists());
    b.pass(drive.path());
    assert_eq!(b.read("docs/x.txt").as_deref(), Some(&b"hello"[..]));
}

#[test]
fn a_file_another_party_put_comes_down_and_a_delete_leaves_a_tombstone() {
    let drive = TempDir::new("folder-remote-drive");
    let a = Device::new("a");
    a.write("keep.txt", b"keep");
    a.pass(drive.path());
    tamper_put(drive.path(), "peer.txt", b"from a peer", json!({}));
    a.pass(drive.path());
    assert_eq!(a.read("peer.txt").as_deref(), Some(&b"from a peer"[..]));
    fs::remove_file(path_of(a.folder.path(), "peer.txt")).unwrap();
    a.pass(drive.path());
    let doc = index(drive.path());
    assert!(doc["files"].get("peer.txt").is_none());
    assert_eq!(doc["deleted"]["peer.txt"]["hash"], json!(sha(b"from a peer")));
    let versions = fs::read_to_string(drive.path().join("versions.jsonl")).unwrap();
    assert!(versions.contains("peer.txt"));
}

#[test]
fn a_link_from_another_device_is_never_made_the_file_here_stays_and_the_entry_stays_on_the_drive() {
    let drive = TempDir::new("folder-remote-drive");
    let a = Device::new("a");
    a.write("note.txt", b"mine");
    a.write("other.txt", b"other");
    a.pass(drive.path());
    let mut doc = index(drive.path());
    doc["files"]["note.txt"] = json!({"kind": "link", "target": "/etc/passwd", "gen": 2, "device": "peer"});
    doc["generation"] = json!(2);
    fs::write(drive.path().join("index.json"), serde_json::to_vec(&doc).unwrap()).unwrap();
    let report = a.pass(drive.path());
    assert_eq!(a.read("note.txt").as_deref(), Some(&b"mine"[..]), "{}", report.summary());
    assert!(
        report.problems.iter().any(|p| p.code == Code::LinkRefused && p.key == "note.txt"),
        "{:?}",
        report.problems
    );
    // Another change commits without touching the link entry.
    a.write("new.txt", b"new");
    a.pass(drive.path());
    let after = index(drive.path());
    assert_eq!(after["files"]["note.txt"]["kind"], "link");
    assert!(after["files"].get("new.txt").is_some());
}

#[test]
fn a_key_that_is_no_utf8_is_listed_never_written_and_kept_byte_for_byte() {
    let drive = TempDir::new("folder-remote-drive");
    let a = Device::new("a");
    a.write("a.txt", b"a");
    a.pass(drive.path());
    tamper_put(drive.path(), "PLACEHOLDER", b"raw", json!({}));
    let raw = fs::read(drive.path().join("index.json")).unwrap();
    let text = String::from_utf8(raw).unwrap().replace("PLACEHOLDER", "bad\\u0001name");
    let mut bytes = text.into_bytes();
    let at = bytes.windows(9).position(|w| w == b"bad\\u0001").unwrap();
    bytes.splice(at..at + 9, [b'b', b'a', b'd', 0xC0, 0xAF]);
    fs::write(drive.path().join("index.json"), &bytes).unwrap();
    let report = a.pass(drive.path());
    assert!(report.problems.iter().any(|p| p.code == Code::BadRemoteName), "{:?}", report.problems);
    a.write("b.txt", b"b");
    a.pass(drive.path());
    let after = fs::read(drive.path().join("index.json")).unwrap();
    assert!(after.windows(5).any(|w| w == [b'b', b'a', b'd', 0xC0, 0xAF]), "the raw key survives a commit");
}

#[test]
fn one_file_the_server_keeps_failing_fails_alone() {
    let drive = TempDir::new("folder-remote-drive");
    let a = Device::new("a");
    for i in 0..5 {
        a.write(&format!("batch/{i}.txt"), format!("item {i}").as_bytes());
    }
    a.write("stuck.bin", b"stuck");
    FolderRemote::new(drive.path(), "a", clock()).unwrap();
    fs::write(drive.path().join("faults.json"), br#"{"fail_keys": ["stuck.bin"]}"#).unwrap();
    let report = a.pass(drive.path());
    let doc = index(drive.path());
    assert_eq!(doc["files"].as_object().unwrap().len(), 5, "{}", report.summary());
    assert!(report.problems.iter().any(|p| p.code == Code::Transient && p.key == "stuck.bin"));
    fs::write(drive.path().join("faults.json"), b"{}").unwrap();
    a.pass(drive.path());
    assert!(index(drive.path())["files"].get("stuck.bin").is_some());
}

#[test]
fn after_a_429_no_request_of_the_pass_comes_before_the_wait() {
    let drive = TempDir::new("folder-remote-drive");
    let a = Device::new("a");
    for i in 0..6 {
        a.write(&format!("batch/{i}.txt"), format!("item {i}").as_bytes());
    }
    FolderRemote::new(drive.path(), "a", clock()).unwrap();
    fs::write(drive.path().join("faults.json"), br#"{"rate_limit_every": 3, "retry_after": 0.3}"#).unwrap();
    a.pass(drive.path());
    let requests = fs::read_to_string(drive.path().join("requests.jsonl")).unwrap();
    assert!(requests.contains("\"status\":429"), "nothing was rate limited");
    assert!(!requests.contains("\"early\":true"), "{requests}");
    assert_eq!(index(drive.path())["files"].as_object().unwrap().len(), 6);
}

#[test]
fn a_corrupt_index_fails_the_pass_and_deletes_nothing_here() {
    let drive = TempDir::new("folder-remote-drive");
    let a = Device::new("a");
    a.write("a.txt", b"a");
    a.pass(drive.path());
    fs::write(drive.path().join("index.json"), br#"{"generation": 7, "files": {"a": [unterminated"#).unwrap();
    let remote = FolderRemote::new(drive.path(), "a", clock()).unwrap();
    let out = folder_remote::sync(
        &remote,
        &LocalRoot::folder(a.folder.path()),
        &a.state.path().join("index.json"),
        &SyncOptions::new("", "harness", "a").unwrap(),
        &RunHooks::default(),
    );
    let e = out.expect_err("an unreadable index is no empty one");
    assert!(e.to_string().contains("index cannot be read"), "{e}");
    assert_eq!(a.read("a.txt").as_deref(), Some(&b"a"[..]));
}
