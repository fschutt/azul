//! A hostile server's changes to stored objects (`b_tamper`): each damages exactly what it
//! says, and a reader with the key finds the damage.

use std::sync::Arc;

use azcloud_kit::sync::remote::{blob_key, index_key};
use azul_storage::{testing::TempDir, Drive};

use super::{announcement, content_objects, plain, restore, Direct};
use crate::e2e::{files::hash, remote::Remote};

fn args(v: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    v.as_object().cloned().unwrap_or_default()
}

/// A plain remote whose index names `files` (key, text), their blobs stored.
fn plain_remote(dir: &TempDir, files: &[(&str, &str)]) -> Arc<Remote> {
    let r = Arc::new(Remote::create("p", dir.path().join("p"), false, "").expect("a remote"));
    let mut entries = serde_json::Map::new();
    for (key, text) in files {
        let h = hash(text.as_bytes());
        r.write(&blob_key("", &h), text.as_bytes()).unwrap();
        entries.insert(
            key.to_string(),
            serde_json::json!({ "hash": h, "size": text.len(), "mtime": 0, "gen": 1, "device": "x" }),
        );
    }
    let index = serde_json::json!({
        "format": "azcloud.sync-index", "version": 1, "generation": 1, "updated_at": 0,
        "updated_by": "x", "files": entries, "deleted": {},
    });
    r.write(&index_key(""), &serde_json::to_vec(&index).unwrap()).unwrap();
    r
}

#[test]
fn a_dropped_blob_is_gone_while_the_index_still_names_it() {
    let dir = TempDir::new("azdrive-b-tamper-drop");
    let r = plain_remote(&dir, &[("a.txt", "aaa"), ("b.txt", "bbb")]);
    plain(&r, "drop_blob", &args(serde_json::json!({ "key": "a.txt" }))).expect("dropped");
    assert!(r.read(&blob_key("", &hash(b"aaa"))).is_none());
    assert!(r.read(&blob_key("", &hash(b"bbb"))).is_some());
    let index: serde_json::Value = serde_json::from_slice(&r.read(&index_key("")).unwrap()).unwrap();
    assert!(index["files"]["a.txt"].is_object(), "the index still names it");
}

#[test]
fn a_purge_removes_the_entry_and_its_blob_but_keeps_a_blob_another_file_shares() {
    let dir = TempDir::new("azdrive-b-tamper-purge");
    let r = plain_remote(&dir, &[("a.txt", "same"), ("b.txt", "same"), ("c.txt", "own")]);
    plain(&r, "purge", &args(serde_json::json!({ "key": "a.txt" }))).expect("purged");
    assert!(r.read(&blob_key("", &hash(b"same"))).is_some(), "b.txt still names it");
    plain(&r, "purge", &args(serde_json::json!({ "key": "c.txt" }))).expect("purged");
    assert!(r.read(&blob_key("", &hash(b"own"))).is_none());
    let index: serde_json::Value = serde_json::from_slice(&r.read(&index_key("")).unwrap()).unwrap();
    assert!(index["files"].get("a.txt").is_none() && index["files"].get("c.txt").is_none());
}

#[test]
fn a_corrupted_object_differs_in_exactly_one_byte() {
    let dir = TempDir::new("azdrive-b-tamper-corrupt");
    let r = plain_remote(&dir, &[("x.txt", "x v2, a longer text")]);
    plain(&r, "corrupt_object", &args(serde_json::json!({ "key": "x.txt", "byte": 3 }))).expect("flipped");
    let now = r.read(&blob_key("", &hash(b"x v2, a longer text"))).unwrap();
    let differ: Vec<usize> = now
        .iter()
        .zip(b"x v2, a longer text")
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(differ, vec![3]);
}

#[test]
fn an_announced_restore_writes_the_next_epoch_before_the_older_index() {
    let dir = TempDir::new("azdrive-b-tamper-restore");
    let r = plain_remote(&dir, &[("docs/a.txt", "a")]);
    let t0 = r.read(&index_key("")).unwrap();
    r.snapshots.lock().unwrap().insert(String::from("t0"), t0.clone());
    r.write(&index_key(""), br#"{"generation": 7, "files": {}, "deleted": {}}"#).unwrap();
    assert!(announcement(&r).is_none(), "nothing announced before a restore");
    let answer = restore(&r, &args(serde_json::json!({ "to": "t0" }))).expect("restored");
    assert_eq!(answer["epoch"], 1);
    assert_eq!(r.read(&index_key("")).unwrap(), t0);
    let announced = announcement(&r).expect("an announcement");
    assert_eq!(announced["epoch"], 1);
    assert_eq!(announced["kind"], "point_in_time");
    assert_eq!(restore(&r, &args(serde_json::json!({ "to": "t0" }))).expect("again")["epoch"], 2);
}

#[test]
fn the_direct_reader_sees_the_bucket_past_every_fault_and_writes_nothing() {
    let dir = TempDir::new("azdrive-b-tamper-direct");
    let r = plain_remote(&dir, &[("a.txt", "a")]);
    r.set_faults("", false, &args(serde_json::json!({ "partition": true })));
    let direct = Direct { remote: r.clone() };
    assert!(direct.get(&index_key("")).is_ok(), "past the partition");
    assert!(direct.head(&index_key("")).unwrap().etag.is_some());
    assert!(direct.put("x", b"x").is_err());
    let listed = direct.list(&azul_storage::ListRequest::recursive("")).unwrap();
    assert_eq!(listed.objects.len(), 2, "the index and one blob");
    assert_eq!(content_objects(&r).len(), 1, "the blob is the only content object");
}

#[cfg(feature = "encryption")]
mod encrypted {
    use std::sync::Arc;

    use azul_storage::{
        crypto::{device, keys},
        encrypted::{open_encrypted, EncryptedDrive},
        keyring::KeyringStore,
        meta::MetaIndexProvider,
        testing::TempDir,
        Drive,
    };

    use super::args;
    use crate::e2e::{
        b_tamper::{content_objects, encrypted, Direct},
        remote::{FaultDrive, MemKeyring, Remote},
    };

    /// A new encrypted remote, its keys made as AzDrive makes a new drive's (a cheap recovery
    /// derivation: a test).
    fn encrypted_remote(dir: &TempDir) -> Arc<Remote> {
        let r = Arc::new(Remote::create("enc", dir.path().join("enc"), true, "").expect("a remote"));
        let bucket = FaultDrive {
            remote: r.clone(),
            device: String::from("(setup)"),
        };
        let kdf = keys::RecoveryKdf::with_cost(8, 1, 1).unwrap();
        device::setup_new_drive(&bucket, r.keyring.as_ref(), &r.drive_name, kdf).expect("the keys");
        r
    }

    /// A device's encrypted drive on `r` (no packing: one object per file).
    fn device(r: &Arc<Remote>, name: &str) -> EncryptedDrive<Arc<dyn Drive>> {
        let bucket: Arc<dyn Drive> = Arc::new(FaultDrive {
            remote: r.clone(),
            device: name.to_string(),
        });
        open_encrypted(bucket, r.keyring.as_ref(), &r.drive_name, &MetaIndexProvider::new(name))
            .expect("the encrypted drive")
    }

    fn bytes(len: usize, seed: u8) -> Vec<u8> {
        (0..len).map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed)).collect()
    }

    #[test]
    fn a_dropped_or_reordered_segment_fails_to_open_and_the_other_file_reads() {
        let dir = TempDir::new("azdrive-b-tamper-chunks");
        let r = encrypted_remote(&dir);
        let w = device(&r, "w");
        w.put("big.bin", &bytes(3_000_000, 1)).expect("a file of three segments");
        w.put("other.bin", &bytes(3_000_000, 2)).expect("another");
        assert_eq!(device(&r, "x").get("big.bin").expect("it reads"), bytes(3_000_000, 1));
        encrypted::tamper(&r, "drop_chunk", &args(serde_json::json!({ "key": "big.bin", "chunk": 1 })))
            .expect("a segment dropped");
        assert!(device(&r, "y").get("big.bin").is_err(), "a dropped segment never opens");
        encrypted::tamper(&r, "reorder_chunks", &args(serde_json::json!({ "key": "other.bin" })))
            .expect("segments reversed");
        assert!(device(&r, "z").get("other.bin").is_err(), "a reordered object never opens");
    }

    #[test]
    fn swapped_names_and_a_spliced_segment_never_open_as_the_other_file() {
        let dir = TempDir::new("azdrive-b-tamper-swap");
        let r = encrypted_remote(&dir);
        let w = device(&r, "w");
        w.put("a.bin", &bytes(600_000, 3)).expect("a");
        w.put("b.bin", &bytes(600_000, 4)).expect("b");
        w.put("c.bin", &bytes(2_100_000, 5)).expect("c");
        encrypted::tamper(&r, "swap_names", &args(serde_json::json!({ "key": "a.bin", "with": "b.bin" })))
            .expect("swapped");
        let reader = device(&r, "x");
        assert!(reader.get("a.bin").is_err() && reader.get("b.bin").is_err());
        encrypted::tamper(&r, "splice", &args(serde_json::json!({ "key": "c.bin", "from_key": "a.bin" })))
            .expect("spliced");
        assert!(device(&r, "y").get("c.bin").is_err());
    }

    #[test]
    fn a_purge_removes_every_version_of_the_file_and_nothing_else() {
        let dir = TempDir::new("azdrive-b-tamper-purge-enc");
        let r = encrypted_remote(&dir);
        let w = device(&r, "w");
        let only_other = {
            w.put("keep.bin", &bytes(700_000, 6)).expect("another file");
            content_objects(&r)
        };
        w.put("gone.bin", &bytes(700_000, 7)).expect("v1");
        w.put("gone.bin", &bytes(700_000, 8)).expect("v2");
        w.delete("gone.bin").expect("deleted");
        assert!(content_objects(&r).len() > only_other.len(), "the history keeps the versions");
        let answer = encrypted::tamper(&r, "purge", &args(serde_json::json!({ "key": "gone.bin" })))
            .expect("purged from the history");
        assert_eq!(answer["removed"].as_array().map(Vec::len), Some(2), "{answer}");
        let left: Vec<String> = content_objects(&r).into_keys().collect();
        let before: Vec<String> = only_other.into_keys().collect();
        assert_eq!(left, before, "only the other file's object stays");
    }

    #[test]
    fn a_replaced_drive_key_seals_another_key_to_the_same_member() {
        let dir = TempDir::new("azdrive-b-tamper-key");
        let r = encrypted_remote(&dir);
        let real = encrypted::drive_key(&r).unwrap();
        let answer = encrypted::replace_drive_key(&r).expect("replaced");
        let file = answer["replaced"][0].as_str().unwrap().to_string();
        let member = file
            .trim_start_matches(".azlin/keys/")
            .trim_end_matches(".key")
            .to_string();
        let wrap = keys::load_member_wrap(&Direct { remote: r.clone() }, &member).unwrap();
        assert_ne!(wrap.drive_key_id, real.id(), "the wrap holds another drive key");
        // A device that unlocks from its member wrap (no drive key kept) never takes it.
        let fresh = MemKeyring::default();
        let entry = device::member_key_entry(&r.drive_name);
        fresh.set(&entry, &r.keyring.get(&entry).unwrap().unwrap()).unwrap();
        let bucket = Direct { remote: r.clone() };
        let unlocked = device::unlock(&bucket, &fresh, &r.drive_name);
        assert!(
            !matches!(unlocked, Ok(Some(ref key)) if key.id() != real.id()),
            "the substituted key is refused"
        );
    }
}
