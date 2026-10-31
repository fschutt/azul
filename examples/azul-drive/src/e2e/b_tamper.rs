//! Group B's `drive_tamper` actions: a hostile or broken server damaging the stored objects of
//! a remote, plain or encrypted, and the server's announced restore.
//!
//! A server holds no drive key. What it can do to an encrypted remote is what these actions do:
//! put, remove, cut, reorder, splice and swap whole stored objects and their bytes. It cannot
//! seal a forged entry into the drive's index, nor edit one entry of it (the index lives in
//! sealed packs of the drive's metadata repository). To aim at the object of one file the
//! harness reads the index with the remote's key, past every device's faults; every change it
//! then makes is one a keyless server could make.
//!
//! Actions (each with `remote`; `key` is a file's key under the remote's prefix):
//! - `raw_put` (`key`, content): an object written into the bucket past the drive's key (a web
//!   or WebDAV upload into an encrypted folder); the drive's index does not name it.
//! - `drop_blob` (`key`): the file's stored object removed while the index still names it.
//! - `purge` (`key`): a permanent delete on the server: every stored object any version of the
//!   file had is removed (on a plain remote also its entry and tombstone). A pack shared with
//!   other files stays.
//! - `graft_blob` (`key`, `from_remote`, `from_key`): the file's object replaced by the stored
//!   object of `from_key` on another remote (another account's).
//! - `corrupt_object` (`key`, `byte`): one byte of the file's stored object flipped (`byte`:
//!   its offset in the object; default the middle of the file's first segment).
//! - `truncate_object` (`key`, `keep_chunks`), `drop_chunk` (`key`, `chunk`), `swap_chunks`
//!   (`key`, `first`, `second`), `reorder_chunks` (`key`: the segments in reverse),
//!   `replay_chunk` (`key`, `chunk`: an earlier segment in its place), `splice` (`key`,
//!   `from_key`: the other file's first segment in place of this one's): an encrypted object's
//!   AZL1 segments, cut and moved as a server can.
//! - `swap_names` (`key`, `with`): the two files' stored objects exchanged.
//! - `replace_drive_key`: every member's key file in the bucket replaced by one that seals
//!   another drive key to the same member.
//! - `restore` (`to`: a snapshot, or `generations`): an announced restore of a plain remote: the
//!   stand-in server's restore announcement (the next epoch) is written first, then the older
//!   index; devices take it as that restore (`server-restored`), not as a rollback.

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use azcloud_kit::sync::{
    remote::{blob_key, index_key},
    RestoreNotice,
};
use azul_storage::{ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo};

use super::{
    files::{content_of, hash},
    need, remote::Remote, str_arg, with_harness, Answer, Args,
};
use crate::DriveState;

/// The actions of this module (the others are `tamper.rs`'s).
const ACTIONS: &[&str] = &[
    "raw_put",
    "drop_blob",
    "purge",
    "graft_blob",
    "corrupt_object",
    "truncate_object",
    "drop_chunk",
    "swap_chunks",
    "reorder_chunks",
    "replay_chunk",
    "splice",
    "swap_names",
    "replace_drive_key",
    "corrupt_entry",
    "drop_folder",
    "restore",
];

/// Whether this module answers the `drive_tamper` step `args`.
pub(crate) fn owns(args: &Args) -> bool {
    str_arg(args, "action").is_some_and(|a| ACTIONS.contains(&a))
}

pub(crate) fn tamper(_s: &mut DriveState, args: &Args) -> Answer {
    let remote = with_harness(|h| h.remote_of(args))?;
    let action = need(args, "action")?;
    match action {
        "raw_put" => {
            let key = need(args, "key")?;
            let data = content_of(args)?;
            remote
                .write(&format!("{}{key}", remote.prefix), &data)
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "written": format!("{}{key}", remote.prefix) }))
        }
        "restore" => restore(&remote, args),
        "corrupt_entry" | "drop_folder" if remote.encrypted => Err(format!(
            "{action}: an encrypted remote's entries live in sealed packs of its metadata \
             repository, a pack per change and sealed per chunk: a server without the key can \
             damage a chunk of a pack, never one entry; the stand-in cannot aim at the chunk that \
             holds one entry until azul-storage's meta module tells which pack and chunk hold a \
             path"
        )),
        "corrupt_entry" | "drop_folder" => Err(format!(
            "{action} is an encrypted remote's action: on a plain remote use raw_entry or delete"
        )),
        "replace_drive_key" => replace_drive_key(&remote),
        _ if remote.encrypted => encrypted::tamper(&remote, action, args),
        _ => plain(&remote, action, args),
    }
}

// ==== A plain remote ====

/// The blob a plain remote's index names for `key`.
fn plain_blob(remote: &Remote, key: &str) -> Result<(String, String), String> {
    let index = plain_index(remote)?;
    let hash = index["files"][key]["hash"]
        .as_str()
        .ok_or_else(|| format!("the index has no {key:?}"))?
        .to_string();
    Ok((blob_key(&remote.prefix, &hash), hash))
}

fn plain_index(remote: &Remote) -> Result<serde_json::Value, String> {
    let bytes = remote
        .read(&index_key(&remote.prefix))
        .ok_or("the remote has no index yet")?;
    serde_json::from_slice(&bytes).map_err(|e| format!("the index is no JSON: {e}"))
}

fn plain(remote: &Arc<Remote>, action: &str, args: &Args) -> Answer {
    match action {
        "drop_blob" => {
            let (blob, _) = plain_blob(remote, need(args, "key")?)?;
            remote.remove(&blob).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "removed": [blob] }))
        }
        "purge" => {
            let key = need(args, "key")?;
            let mut index = plain_index(remote)?;
            let mut hashes = Vec::new();
            for part in ["files", "deleted"] {
                if let Some(h) = index[part][key]["hash"].as_str() {
                    hashes.push(h.to_string());
                }
                if let Some(map) = index[part].as_object_mut() {
                    map.remove(key);
                }
            }
            // A blob another entry still names stays: blobs are shared by content.
            let named = |h: &str| {
                ["files", "deleted"].iter().any(|part| {
                    index[*part]
                        .as_object()
                        .is_some_and(|m| m.values().any(|e| e["hash"].as_str() == Some(h)))
                })
            };
            let mut removed = Vec::new();
            for h in hashes.iter().filter(|h| !named(h)) {
                let blob = blob_key(&remote.prefix, h);
                remote.remove(&blob).map_err(|e| e.to_string())?;
                removed.push(blob);
            }
            let mut bytes = serde_json::to_vec_pretty(&index).map_err(|e| e.to_string())?;
            bytes.push(b'\n');
            remote
                .write(&index_key(&remote.prefix), &bytes)
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "removed": removed }))
        }
        "graft_blob" => {
            let (blob, _) = plain_blob(remote, need(args, "key")?)?;
            let bytes = other_object(args)?;
            remote.write(&blob, &bytes).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "grafted": blob }))
        }
        "corrupt_object" => {
            let (blob, _) = plain_blob(remote, need(args, "key")?)?;
            let mut bytes = remote.read(&blob).ok_or("the blob is not on the remote")?;
            let at = byte_arg(args).unwrap_or(bytes.len() / 2);
            flip(&mut bytes, at)?;
            remote.write(&blob, &bytes).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "flipped": at }))
        }
        "swap_names" => Err(String::from(
            "a plain index binds no name to its content: a server swaps two files with raw_entry; \
             swap_names is an encrypted remote's action",
        )),
        other => Err(format!(
            "tamper '{other}' cuts an encrypted object's AZL1 segments: make the remote with \
             \"encrypted\": true (a plain blob is cut with truncate_blob)"
        )),
    }
}

/// The stored object of `from_key` on the remote `from_remote` (a plain one's blob, an
/// encrypted one's AZL1 object).
fn other_object(args: &Args) -> Result<Vec<u8>, String> {
    let from = need(args, "from_remote")?.to_string();
    let other = with_harness(|h| {
        h.remotes
            .get(&from)
            .cloned()
            .ok_or_else(|| format!("no remote '{from}'"))
    })?;
    let from_key = need(args, "from_key")?;
    let object = if other.encrypted {
        encrypted::object_of(&other, from_key)?.bucket_key
    } else {
        plain_blob(&other, from_key)?.0
    };
    other
        .read(&object)
        .ok_or_else(|| format!("{from_key}'s object is not on {from}"))
}

fn byte_arg(args: &Args) -> Option<usize> {
    args.get("byte")
        .and_then(serde_json::Value::as_u64)
        .and_then(|b| usize::try_from(b).ok())
}

fn flip(bytes: &mut [u8], at: usize) -> Result<(), String> {
    let len = bytes.len();
    let b = bytes
        .get_mut(at)
        .ok_or_else(|| format!("byte {at} is past the object's {len} bytes"))?;
    *b ^= 0x5A;
    Ok(())
}

// ==== The stored objects, past every fault ====

/// The remote's bucket as the server holds it: every read past the devices' faults. Writes are
/// refused, so opening the drive's index to aim never changes the remote.
pub(crate) struct Direct {
    pub remote: Arc<Remote>,
}

impl Direct {
    fn refused(key: &str) -> DriveError {
        DriveError::Unsupported(format!("the tamper's reader writes nothing ({key})"))
    }
}

impl Drive for Direct {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        let mut page = ListPage {
            folders: Vec::new(),
            objects: Vec::new(),
            next: None,
        };
        for key in self.remote.keys() {
            let Some(rest) = key.strip_prefix(&request.prefix) else {
                continue;
            };
            let folder = request
                .delimiter
                .as_deref()
                .and_then(|d| rest.find(d).map(|i| format!("{}{}", request.prefix, &rest[..i + d.len()])));
            match folder {
                Some(folder) => {
                    if page.folders.last() != Some(&folder) {
                        page.folders.push(folder);
                    }
                }
                None => page.objects.push(self.head(&key)?),
            }
        }
        Ok(page)
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.remote.read(key).ok_or_else(|| DriveError::NotFound {
            key: key.to_string(),
        })
    }

    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        let bytes = self.get(key)?;
        let start = usize::try_from(range.start).unwrap_or(usize::MAX).min(bytes.len());
        let end = range
            .end
            .map_or(bytes.len(), |e| usize::try_from(e).unwrap_or(usize::MAX).saturating_add(1))
            .min(bytes.len());
        Ok(bytes[start..end.max(start)].to_vec())
    }

    fn put(&self, key: &str, _: &[u8]) -> Result<(), DriveError> {
        Err(Self::refused(key))
    }

    fn delete(&self, key: &str) -> Result<(), DriveError> {
        Err(Self::refused(key))
    }

    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        let bytes = self.get(key)?;
        Ok(ObjectInfo {
            key: key.to_string(),
            size: bytes.len() as u64,
            modified: None,
            // The stand-in remote's entity tag (remote.rs): the content's BLAKE3, cut.
            etag: Some(hash(&bytes)[..32].to_string()),
        })
    }
}

// ==== An encrypted remote ====

#[cfg(feature = "encryption")]
pub(crate) mod encrypted {
    use std::{collections::BTreeSet, sync::Arc};

    use azul_storage::{
        crypto::{azl1::OpenObject, device, keys, DriveKey, ObjectId},
        encrypted::{open_encrypted, EncryptedDrive},
        meta::{pointer, DriveBucket, MetaIndexProvider, MetaRepo},
        Drive,
    };

    use super::{byte_arg, flip, need, other_object, Answer, Args, Direct, Remote};

    /// Where a file's bytes are stored.
    pub(crate) struct Located {
        /// The bucket key of the object that holds them (a pack for a small file).
        pub bucket_key: String,
        /// Where the file's record starts in a pack's plaintext (`None`: an object of its own).
        pub packed_at: Option<u64>,
    }

    /// The BLAKE3 of every content the drive's history holds: each version of every file in
    /// every commit (the drive keeps them; a pointer names its plaintext's hash, so nothing is
    /// decrypted).
    pub(crate) fn every_version_hash(remote: &Arc<Remote>) -> Result<BTreeSet<String>, String> {
        let bucket: Arc<dyn Drive> = Arc::new(Direct {
            remote: remote.clone(),
        });
        let mut repo = MetaRepo::open(DriveBucket::new(bucket), drive_key(remote)?, "e2e-check", "e2e-check")
            .map_err(|e| format!("the drive's history: {e}"))?;
        repo.fetch_all().map_err(|e| e.to_string())?;
        let mut hashes = BTreeSet::new();
        let mut seen = BTreeSet::new();
        for (_, commit) in repo.history().map_err(|e| e.to_string())? {
            let files = azul_storage::meta::tree::walk(repo.objects(), &commit.tree)
                .map_err(|e| e.to_string())?;
            for (mode, id) in files.into_values() {
                if mode != azul_storage::meta::Mode::File || !seen.insert(id) {
                    continue;
                }
                let bytes = repo.objects().blob(&id).map_err(|e| e.to_string())?;
                if let Some(object) = pointer::decode(bytes).ok().and_then(|entry| entry.object) {
                    hashes.insert(object.blake3.iter().map(|b| format!("{b:02x}")).collect());
                }
            }
        }
        Ok(hashes)
    }

    /// The remote's drive key, from the keyring its devices share.
    pub(crate) fn drive_key(remote: &Remote) -> Result<DriveKey, String> {
        device::load_drive_key(remote.keyring.as_ref(), &remote.drive_name)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| String::from("the remote's keyring holds no drive key"))
    }

    /// [`reader`] as a drive: the remote as the server holds it, for the checks.
    pub(crate) fn reader_drive(remote: &Arc<Remote>) -> Result<Arc<dyn Drive>, String> {
        Ok(Arc::new(reader(remote)?))
    }

    /// The remote's encrypted drive read past every fault (no packing: it writes nothing).
    fn reader(remote: &Arc<Remote>) -> Result<EncryptedDrive<Arc<dyn Drive>>, String> {
        let bucket: Arc<dyn Drive> = Arc::new(Direct {
            remote: remote.clone(),
        });
        let provider = MetaIndexProvider::new("e2e-tamper").with_cache_root(None);
        open_encrypted(bucket, remote.keyring.as_ref(), &remote.drive_name, &provider)
            .map_err(|e| format!("the remote's index cannot be read to aim: {e}"))
    }

    /// The stored object of the file `key`.
    pub(crate) fn object_of(remote: &Arc<Remote>, key: &str) -> Result<Located, String> {
        let entry = reader(remote)?
            .entry(&format!("{}{key}", remote.prefix))
            .map_err(|e| format!("{key}: {e}"))?;
        let object = entry
            .object
            .ok_or_else(|| format!("{key} is a folder, it has no object"))?;
        Ok(Located {
            bucket_key: object.bucket_object().bucket_key(),
            packed_at: object.packed.as_ref().map(|p| p.offset),
        })
    }

    /// An AZL1 object's bytes and where each of its segments lies (offset, length).
    pub(crate) struct Segmented {
        pub bytes: Vec<u8>,
        pub spans: Vec<(usize, usize)>,
        pub segment_size: u64,
    }

    impl Segmented {
        fn read(remote: &Remote, bucket_key: &str) -> Result<Segmented, String> {
            let bytes = remote
                .read(bucket_key)
                .ok_or_else(|| format!("{bucket_key} is not in the bucket"))?;
            let id = ObjectId::from_bucket_key(bucket_key)
                .ok_or_else(|| format!("{bucket_key} names no AZL1 object"))?;
            let open = OpenObject::open_with_drive_key(&bytes, &id, &drive_key(remote)?)
                .map_err(|e| format!("{bucket_key} does not open: {e}"))?;
            let spans = (0..open.segment_count())
                .map(|i| {
                    let (at, len) = open.segment_span(i);
                    (at as usize, len as usize)
                })
                .collect();
            let segment_size = azul_storage::crypto::azl1::PublicHeader::parse(&bytes)
                .map(|h| u64::from(h.segment_size))
                .map_err(|e| e.to_string())?;
            Ok(Segmented {
                bytes,
                spans,
                segment_size,
            })
        }

        fn segment(&self, i: usize) -> Result<&[u8], String> {
            let (at, len) = *self
                .spans
                .get(i)
                .ok_or_else(|| format!("the object has {} segments, no segment {i}", self.spans.len()))?;
            Ok(&self.bytes[at..at + len])
        }

        fn head(&self) -> &[u8] {
            &self.bytes[..self.spans.first().map_or(self.bytes.len(), |s| s.0)]
        }

        fn tail(&self) -> &[u8] {
            let end = self.spans.last().map_or(self.bytes.len(), |(at, len)| at + len);
            &self.bytes[end..]
        }

        /// The object with its segments in `order` (indices of the segments, any may repeat
        /// or be left out), between its header and its trailer.
        fn with_order(&self, order: &[usize]) -> Result<Vec<u8>, String> {
            let mut out = self.head().to_vec();
            for i in order {
                out.extend_from_slice(self.segment(*i)?);
            }
            out.extend_from_slice(self.tail());
            Ok(out)
        }

        /// The segment that holds the file: its first, or a packed record's.
        fn file_segment(&self, packed_at: Option<u64>) -> usize {
            let i = packed_at.map_or(0, |at| usize::try_from(at / self.segment_size.max(1)).unwrap_or(0));
            i.min(self.spans.len().saturating_sub(1))
        }
    }

    fn index_arg(args: &Args, key: &str, default: usize) -> usize {
        args.get(key)
            .and_then(serde_json::Value::as_u64)
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(default)
    }

    pub(crate) fn tamper(remote: &Arc<Remote>, action: &str, args: &Args) -> Answer {
        let key = need(args, "key")?;
        // A purge aims at the history: the file is often deleted already.
        if action == "purge" {
            return purge(remote, key);
        }
        let located = object_of(remote, key)?;
        let target = located.bucket_key.clone();
        let write = |bytes: &[u8]| remote.write(&target, bytes).map_err(|e| e.to_string());
        let answer = match action {
            "drop_blob" => {
                remote.remove(&target).map_err(|e| e.to_string())?;
                serde_json::json!({ "removed": [target] })
            }
            "graft_blob" => {
                write(&other_object(args)?)?;
                serde_json::json!({ "grafted": target })
            }
            "corrupt_object" => {
                let mut object = Segmented::read(remote, &target)?;
                let at = match byte_arg(args) {
                    Some(at) => at,
                    None => {
                        let (start, len) = object.spans[object.file_segment(located.packed_at)];
                        start + len / 2
                    }
                };
                flip(&mut object.bytes, at)?;
                write(&object.bytes)?;
                serde_json::json!({ "object": target, "flipped": at })
            }
            "truncate_object" => {
                let object = Segmented::read(remote, &target)?;
                let keep = index_arg(args, "keep_chunks", 1);
                let cut = object
                    .spans
                    .get(keep)
                    .map(|(at, _)| *at)
                    .ok_or_else(|| format!("the object has {} segments: keeping {keep} cuts nothing", object.spans.len()))?;
                write(&object.bytes[..cut])?;
                serde_json::json!({ "object": target, "kept_bytes": cut })
            }
            "drop_chunk" | "swap_chunks" | "reorder_chunks" | "replay_chunk" => {
                let object = Segmented::read(remote, &target)?;
                let n = object.spans.len();
                let mut order: Vec<usize> = (0..n).collect();
                match action {
                    "drop_chunk" => {
                        let c = index_arg(args, "chunk", 0);
                        if c >= n {
                            return Err(format!("the object has {n} segments, no segment {c}"));
                        }
                        order.remove(c);
                    }
                    "swap_chunks" => {
                        let (a, b) = (index_arg(args, "first", 0), index_arg(args, "second", 1));
                        if a >= n || b >= n || a == b {
                            return Err(format!("segments {a} and {b} of {n} cannot be swapped"));
                        }
                        order.swap(a, b);
                    }
                    "reorder_chunks" => {
                        if n < 2 {
                            return Err(format!("an object of {n} segment cannot be reordered"));
                        }
                        order.reverse();
                    }
                    _ => {
                        if n < 2 {
                            return Err(format!("an object of {n} segment has none to replay"));
                        }
                        let c = index_arg(args, "chunk", 1).min(n - 1);
                        order[c] = if c == 0 { 1 } else { c - 1 };
                    }
                }
                write(&object.with_order(&order)?)?;
                serde_json::json!({ "object": target, "order": order })
            }
            "splice" => {
                let object = Segmented::read(remote, &target)?;
                let from = object_of(remote, need(args, "from_key")?)?;
                let source = Segmented::read(remote, &from.bucket_key)?;
                let i = object.file_segment(located.packed_at);
                let (at, len) = object.spans[i];
                let mut out = object.bytes[..at].to_vec();
                out.extend_from_slice(source.segment(source.file_segment(from.packed_at))?);
                out.extend_from_slice(&object.bytes[at + len..]);
                write(&out)?;
                serde_json::json!({ "object": target, "segment": i, "from": from.bucket_key })
            }
            "swap_names" => {
                let with = str_with(args)?;
                let other = object_of(remote, with)?;
                if other.bucket_key == target {
                    return Err(format!(
                        "{key} and {with} are packed into one object: a server without the key \
                         can only swap whole objects (write files over 512 KiB)"
                    ));
                }
                let mine = remote.read(&target).ok_or("the object is not in the bucket")?;
                let theirs = remote.read(&other.bucket_key).ok_or("the other object is not in the bucket")?;
                write(&theirs)?;
                remote
                    .write(&other.bucket_key, &mine)
                    .map_err(|e| e.to_string())?;
                serde_json::json!({ "swapped": [target, other.bucket_key] })
            }
            other => return Err(format!("no tamper action '{other}' for an encrypted remote")),
        };
        Ok(answer)
    }

    /// `swap_names`'s other file: `with` (`other` is the older spelling).
    fn str_with(args: &Args) -> Result<&str, String> {
        super::str_arg(args, "with")
            .or_else(|| super::str_arg(args, "other"))
            .ok_or_else(|| String::from("'with' is missing"))
    }

    /// A permanent delete of `key` on the server: every object any version of it had, found in
    /// the drive's history. A pack that holds other files' records stays.
    fn purge(remote: &Arc<Remote>, key: &str) -> Answer {
        let path = format!("{}{key}", remote.prefix);
        let bucket: Arc<dyn Drive> = Arc::new(Direct {
            remote: remote.clone(),
        });
        let mut repo = MetaRepo::open(DriveBucket::new(bucket), drive_key(remote)?, "e2e-tamper", "e2e-tamper")
            .map_err(|e| format!("the drive's history: {e}"))?;
        repo.fetch_all().map_err(|e| e.to_string())?;
        let mut objects = BTreeSet::new();
        let mut packs = BTreeSet::new();
        for (commit, _) in repo.history().map_err(|e| e.to_string())? {
            let Some(blob) = repo.file_at(&commit, &path).map_err(|e| e.to_string())? else {
                continue;
            };
            let bytes = repo.objects().blob(&blob).map_err(|e| e.to_string())?;
            let entry = pointer::decode(bytes)?;
            if let Some(object) = entry.object {
                let bucket_key = object.bucket_object().bucket_key();
                if object.packed.is_some() {
                    packs.insert(bucket_key);
                } else {
                    objects.insert(bucket_key);
                }
            }
        }
        for object in &objects {
            remote.remove(object).map_err(|e| e.to_string())?;
        }
        Ok(serde_json::json!({ "removed": objects, "packs_kept": packs }))
    }

    /// Every member's key file sealed again, by the server, around a drive key of its own
    /// making: a member's public key is the only thing it needs (the key file's own member id
    /// names whose it is; the stand-in learns the public key from the member secret its devices
    /// share).
    pub(crate) fn replace_drive_key(remote: &Arc<Remote>) -> Answer {
        let secret = device::load_member_secret(remote.keyring.as_ref(), &remote.drive_name)
            .map_err(|e| e.to_string())?
            .ok_or("the remote's keyring holds no member key")?;
        let public = secret.public();
        let member = public.id();
        let forged = DriveKey::generate().map_err(|e| e.to_string())?;
        let wrap = keys::MemberWrap::seal(&forged, &remote.drive_name, &member, &public)
            .map_err(|e| e.to_string())?;
        let file = keys::member_key_file(&member).map_err(|e| e.to_string())?;
        remote.write(&file, &wrap.to_bytes()).map_err(|e| e.to_string())?;
        Ok(serde_json::json!({ "replaced": [file] }))
    }
}

#[cfg(not(feature = "encryption"))]
pub(crate) mod encrypted {
    use std::sync::Arc;

    use super::{Answer, Args, Remote};

    pub(crate) struct Located {
        pub bucket_key: String,
    }

    pub(crate) fn every_version_hash(
        _remote: &Arc<Remote>,
    ) -> Result<std::collections::BTreeSet<String>, String> {
        Err(none())
    }

    pub(crate) fn reader_drive(
        _remote: &Arc<Remote>,
    ) -> Result<Arc<dyn azul_storage::Drive>, String> {
        Err(none())
    }

    fn none() -> String {
        String::from("this AzDrive was built without encryption: no encrypted remote")
    }

    pub(crate) fn object_of(_remote: &Arc<Remote>, _key: &str) -> Result<Located, String> {
        Err(none())
    }

    pub(crate) fn tamper(_remote: &Arc<Remote>, _action: &str, _args: &Args) -> Answer {
        Err(none())
    }

    pub(crate) fn replace_drive_key(_remote: &Arc<Remote>) -> Answer {
        Err(none())
    }
}

fn replace_drive_key(remote: &Arc<Remote>) -> Answer {
    if !remote.encrypted {
        return Err(String::from("a plain remote has no drive key to replace"));
    }
    encrypted::replace_drive_key(remote)
}

// ==== The announced restore ====

/// Where the stand-in server keeps its newest restore announcement for a remote: what the
/// token server hands out with every credentials refresh and `GET /v1/drives/{id}` (the field
/// `restore`). The stand-in's is not signed: the devices take it from the harness, never from
/// the bucket.
fn announcement_path(remote: &Remote) -> PathBuf {
    remote.dir.join("announcements").join("restore.json")
}

/// The remote's newest restore announcement, if it made one.
pub(crate) fn announcement(remote: &Remote) -> Option<serde_json::Value> {
    let bytes = std::fs::read(announcement_path(remote)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// The restore an E2E device's passes take as announced (`None`: none, or not an E2E device).
pub(crate) fn restore_of(drive_id: &str) -> Option<RestoreNotice> {
    let remote = super::harness().as_ref().and_then(|h| {
        let name = &h.devices.iter().find(|d| d.drive_id == drive_id)?.remote;
        h.remotes.get(name).cloned()
    })?;
    let announced = announcement(&remote)?;
    Some(RestoreNotice {
        epoch: announced.get("epoch")?.as_u64()?,
        prefix: announced
            .get("prefix")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string(),
    })
}

/// An announced restore of a plain remote to the snapshot `to` (or `generations` back): the
/// announcement with the next epoch first (the token server announces before it restores),
/// then the older index.
fn restore(remote: &Remote, args: &Args) -> Answer {
    if remote.encrypted {
        return Err(String::from(
            "an announced restore puts an older index back; an encrypted remote's index is its \
             metadata repository, not one object: make the remote with \"encrypted\": false",
        ));
    }
    let bytes = match str_arg(args, "to") {
        Some(name) => lock(&remote.snapshots)
            .get(name)
            .cloned()
            .ok_or_else(|| format!("no snapshot '{name}' (drive_snapshot keeps one)"))?,
        None => {
            let back = args
                .get("generations")
                .and_then(serde_json::Value::as_u64)
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(1);
            let history = lock(&remote.index_history);
            if history.len() <= back {
                return Err(format!(
                    "the remote has {} index versions: none {back} back",
                    history.len()
                ));
            }
            history[history.len() - 1 - back].clone()
        }
    };
    let restored: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
    let generation = restored
        .get("generation")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let epoch = announcement(remote)
        .and_then(|a| a.get("epoch").and_then(serde_json::Value::as_u64))
        .unwrap_or(0)
        + 1;
    let now = azcloud_kit::now();
    let announced = serde_json::json!({
        "epoch": epoch,
        "kind": "point_in_time",
        "as_of": restored.get("updated_at").cloned().unwrap_or(serde_json::Value::Null),
        "prefix": "",
        "announced_at": now,
        "generation": generation,
        "sig": serde_json::Value::Null,
    });
    let path = announcement_path(remote);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, serde_json::to_vec_pretty(&announced).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    remote
        .write(&index_key(&remote.prefix), &bytes)
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "epoch": epoch, "generation": generation }))
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

// ==== Marks of the stored objects ====

/// The content objects of every remote recorded by `drive_mark_objects`: by (remote folder,
/// name), each object's key and the BLAKE3 of its bytes.
type Marks = BTreeMap<(PathBuf, String), BTreeMap<String, String>>;

static MARKS: Mutex<Marks> = Mutex::new(BTreeMap::new());

/// Whether `key` holds a file's content (an encrypted remote's AZL1 object or pack, a plain
/// remote's blob), not the drive's bookkeeping.
pub(crate) fn is_content_object(key: &str) -> bool {
    key.starts_with("data/") || key.contains(".azlin/blobs/")
}

/// Every content object of `remote` now: key -> BLAKE3 of its bytes.
pub(crate) fn content_objects(remote: &Remote) -> BTreeMap<String, String> {
    remote
        .keys()
        .into_iter()
        .filter(|k| is_content_object(k))
        .filter_map(|k| remote.read(&k).map(|b| (k, hash(&b))))
        .collect()
}

/// `drive_mark_objects` (`remote`, `name`): records the remote's content objects now.
pub(crate) fn mark_objects(args: &Args) -> Answer {
    let remote = with_harness(|h| h.remote_of(args))?;
    let name = need(args, "name")?.to_string();
    let objects = content_objects(&remote);
    let count = objects.len();
    lock(&MARKS).insert((remote.dir.clone(), name), objects);
    Ok(serde_json::json!({ "objects": count }))
}

/// The content objects recorded under `name` for `remote`.
pub(crate) fn marked(remote: &Remote, name: &str) -> Option<BTreeMap<String, String>> {
    lock(&MARKS).get(&(remote.dir.clone(), name.to_string())).cloned()
}

#[cfg(test)]
#[path = "b_tamper_tests.rs"]
mod tests;
