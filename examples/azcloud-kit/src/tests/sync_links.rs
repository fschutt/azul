//! Links as entries of their own: a relative link whose target stays inside the folder syncs as
//! a link - never followed, its target its content - and any other link is never made here,
//! never followed and never sent.

use std::{fs, os::unix::fs::symlink};

use serde_json::json;

use super::{
    fake_s3::S3Bucket,
    sync::{index_of, Device, PREFIX},
};
use crate::sync::{
    local::hash_bytes,
    remote::{blob_key, index_key},
};

/// The target of the link `key` of `device`, when it is a link.
fn link_target(device: &Device, key: &str) -> Option<String> {
    let path = device.path().join(key);
    let meta = fs::symlink_metadata(&path).ok()?;
    if !meta.file_type().is_symlink() {
        return None;
    }
    fs::read_link(path)
        .ok()
        .map(|target| target.to_string_lossy().into_owned())
}

#[test]
fn a_relative_link_inside_the_folder_syncs_as_a_link_and_is_never_followed() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a-link");
    let b = Device::new("dev-b-link");
    a.write("notes/2026/report.md", b"the report");
    symlink("notes/2026/report.md", a.path().join("latest.md")).unwrap();
    // A link to a folder inside: never walked into.
    symlink("2026", a.path().join("notes/current")).unwrap();
    a.sync(&store);
    let index = serde_json::to_value(index_of(&store)).unwrap();
    assert_eq!(index["files"]["latest.md"]["link"], "notes/2026/report.md");
    assert_eq!(index["files"]["notes/current"]["link"], "2026");
    assert!(index["files"].get("notes/current/report.md").is_none(), "never followed");
    b.sync(&store);
    assert_eq!(link_target(&b, "latest.md").as_deref(), Some("notes/2026/report.md"));
    assert_eq!(link_target(&b, "notes/current").as_deref(), Some("2026"));
    assert_eq!(b.read("notes/2026/report.md").as_deref(), Some(&b"the report"[..]));
    let again = b.sync(&store);
    assert_eq!((again.files_up, again.files_down), (0, 0), "{}", again.summary());
    // A link pointed elsewhere on one device is pointed there on the other.
    fs::remove_file(a.path().join("latest.md")).unwrap();
    symlink("notes/2026", a.path().join("latest.md")).unwrap();
    a.sync(&store);
    b.sync(&store);
    assert_eq!(link_target(&b, "latest.md").as_deref(), Some("notes/2026"));
}

/// A link from the drive (another device, a share participant, a hostile server) that would
/// point out of the folder, at an absolute path or into its bookkeeping is never made here, as
/// a link or as a file.
#[test]
fn a_link_from_the_drive_that_would_leave_the_folder_is_never_made() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a-badlink");
    let b = Device::new("dev-b-badlink");
    a.write("keep.txt", b"kept");
    a.sync(&store);
    let mut index = serde_json::to_value(index_of(&store)).unwrap();
    let bad = [
        ("escape", "../../outside"),
        ("abs", "/etc/passwd"),
        ("deep/up", "../../.."),
        ("into", ".azlin/sync/root-id"),
        ("drive", "C:/Windows"),
    ];
    for (key, target) in bad {
        let hash = hash_bytes(target.as_bytes());
        store.write(&blob_key(PREFIX, &hash), target.as_bytes().to_vec());
        index["files"][key] = json!({
            "hash": hash,
            "size": target.len(),
            "mtime": 1,
            "gen": 2,
            "device": "peer",
            "link": target
        });
    }
    index["generation"] = json!(2);
    store.write(&index_key(PREFIX), serde_json::to_vec(&index).unwrap());
    b.sync(&store);
    for (key, _) in bad {
        assert!(
            fs::symlink_metadata(b.path().join(key)).is_err(),
            "{key}: nothing is made here for it"
        );
    }
    assert_eq!(b.read("keep.txt").as_deref(), Some(&b"kept"[..]));
    // The entries stay on the drive as they are.
    let after = serde_json::to_value(index_of(&store)).unwrap();
    assert_eq!(after["files"]["escape"]["link"], "../../outside");
}
