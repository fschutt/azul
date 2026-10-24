//! `drive_tamper` and `drive_snapshot`: a hostile or broken server, changing a remote past every
//! device's fault layer (sync-chaos's `tamper` step, by the same actions).
//!
//! A plain remote is changed in its own format: the sync index (as JSON, so fields this version
//! does not know survive) and the blobs. An encrypted remote is changed through a device's key
//! (`put`, `delete`, `delete_all`); its stored objects cannot be named by a file's key, so the
//! actions that damage one object are refused there.

use azcloud_kit::sync::remote::{blob_key, index_key, INDEX_FORMAT, INDEX_VERSION};

use super::{
    files::{content_of, hash},
    named_files, need, remote::Remote, str_arg, with_harness, Answer, Args,
};
use crate::DriveState;

fn now() -> i64 {
    azcloud_kit::now()
}

/// The remote's index as JSON (an empty one when there is none).
fn index_json(remote: &Remote) -> Result<serde_json::Value, String> {
    match remote.read(&index_key(&remote.prefix)) {
        Some(bytes) => serde_json::from_slice(&bytes).map_err(|e| format!("the index is no JSON: {e}")),
        None => Ok(serde_json::json!({
            "format": INDEX_FORMAT, "version": INDEX_VERSION, "generation": 0,
            "updated_at": 0, "updated_by": "", "files": {}, "deleted": {},
        })),
    }
}

/// Writes `index` with its generation one up, as a writer of the drive would.
fn commit(remote: &Remote, mut index: serde_json::Value, writer: &str) -> Result<u64, String> {
    let generation = index.get("generation").and_then(serde_json::Value::as_u64).unwrap_or(0) + 1;
    index["generation"] = generation.into();
    index["updated_at"] = now().into();
    index["updated_by"] = writer.into();
    let mut bytes = serde_json::to_vec_pretty(&index).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    remote
        .write(&index_key(&remote.prefix), &bytes)
        .map_err(|e| e.to_string())?;
    Ok(generation)
}

fn files_of(index: &mut serde_json::Value) -> Result<&mut serde_json::Map<String, serde_json::Value>, String> {
    if !index.get("files").is_some_and(serde_json::Value::is_object) {
        index["files"] = serde_json::json!({});
    }
    index["files"]
        .as_object_mut()
        .ok_or_else(|| String::from("the index has no files"))
}

/// A tombstone for `key` at `generation` (its content's hash kept).
fn tombstone(index: &mut serde_json::Value, key: &str, generation: u64, writer: &str) {
    let hash = index["files"][key]["hash"].clone();
    if let Some(files) = index["files"].as_object_mut() {
        files.remove(key);
    }
    if !index.get("deleted").is_some_and(serde_json::Value::is_object) {
        index["deleted"] = serde_json::json!({});
    }
    index["deleted"][key] = serde_json::json!({
        "hash": hash, "gen": generation, "at": now(), "device": writer,
    });
}

pub(crate) fn tamper(s: &mut DriveState, args: &Args) -> Answer {
    let remote = with_harness(|h| h.remote_of(args))?;
    let action = need(args, "action")?.to_string();
    let writer = str_arg(args, "writer").unwrap_or("tamper").to_string();
    if remote.encrypted {
        return tamper_encrypted(s, &remote, &action, args);
    }
    let mut index = index_json(&remote)?;
    if !index.is_object() {
        return Err(String::from("the remote's index is no JSON object (corrupt_index ran?)"));
    }
    let next = index.get("generation").and_then(serde_json::Value::as_u64).unwrap_or(0) + 1;
    let generation = match action.as_str() {
        "put" => {
            let key = need(args, "key")?;
            let data = content_of(args)?;
            let h = hash(&data);
            remote
                .write(&blob_key(&remote.prefix, &h), &data)
                .map_err(|e| e.to_string())?;
            let mut entry = serde_json::json!({
                "hash": h, "size": data.len(), "mtime": now(), "gen": next, "device": writer,
            });
            for field in ["kind", "target", "exec", "mode"] {
                if let Some(v) = args.get(field) {
                    entry[field] = v.clone();
                }
            }
            files_of(&mut index)?.insert(key.to_string(), entry);
            if let Some(deleted) = index.get_mut("deleted").and_then(serde_json::Value::as_object_mut) {
                deleted.remove(key);
            }
            if let Some(text) = str_arg(args, "text") {
                with_harness(|h| {
                    h.texts.insert(text.to_string());
                    Ok(())
                })?;
            }
            commit(&remote, index, &writer)?
        }
        "delete" => {
            let key = need(args, "key")?;
            tombstone(&mut index, key, next, &writer);
            commit(&remote, index, &writer)?
        }
        "delete_all" => {
            let keys: Vec<String> = files_of(&mut index)?.keys().cloned().collect();
            for key in keys {
                tombstone(&mut index, &key, next, &writer);
            }
            commit(&remote, index, &writer)?
        }
        "rollback" => rollback(&remote, args)?,
        "restore" => {
            return Err(String::from(
                "an announced restore needs the server's restore announcement, which the \
                 stand-in remote does not have yet: use rollback for an unannounced one",
            ))
        }
        "corrupt_index" => {
            remote
                .write(
                    &index_key(&remote.prefix),
                    br#"{"format": "azcloud.sync-index", "version": 1, "generation": "#,
                )
                .map_err(|e| e.to_string())?;
            return Ok(serde_json::json!({}));
        }
        "corrupt_blob" | "truncate_blob" => {
            let key = need(args, "key")?;
            let h = index["files"][key]["hash"]
                .as_str()
                .ok_or_else(|| format!("the index has no {key:?}"))?
                .to_string();
            let blob = blob_key(&remote.prefix, &h);
            let mut bytes = remote.read(&blob).ok_or("the blob is not on the remote")?;
            if action == "corrupt_blob" {
                if bytes.is_empty() {
                    bytes.push(0);
                } else {
                    for b in bytes.iter_mut().take(16) {
                        *b ^= 0xA5;
                    }
                }
            } else {
                let keep = args.get("keep").and_then(serde_json::Value::as_u64).unwrap_or(0);
                bytes.truncate(usize::try_from(keep).unwrap_or(usize::MAX));
            }
            remote.write(&blob, &bytes).map_err(|e| e.to_string())?;
            return Ok(serde_json::json!({}));
        }
        "raw_entry" => {
            let key = need(args, "key")?;
            let entry: serde_json::Value =
                serde_json::from_str(need(args, "entry_json")?).map_err(|e| e.to_string())?;
            files_of(&mut index)?.insert(key.to_string(), entry);
            commit(&remote, index, &writer)?
        }
        "raw_key" => {
            let hex = need(args, "key_hex")?;
            let raw: Vec<u8> = (0..hex.len())
                .step_by(2)
                .map(|i| hex.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok()))
                .collect::<Option<_>>()
                .ok_or("key_hex is not hex")?;
            let key = String::from_utf8(raw)
                .map_err(|_| String::from("a key that is not UTF-8 cannot be written in a JSON index"))?;
            let data = content_of(args)?;
            let h = hash(&data);
            remote
                .write(&blob_key(&remote.prefix, &h), &data)
                .map_err(|e| e.to_string())?;
            files_of(&mut index)?.insert(
                key,
                serde_json::json!({
                    "hash": h, "size": data.len(), "mtime": now(), "gen": next, "device": writer,
                }),
            );
            commit(&remote, index, &writer)?
        }
        "features" => {
            index["features"] = args.get("features").cloned().unwrap_or(serde_json::json!([]));
            commit(&remote, index, &writer)?
        }
        other => return Err(format!("no tamper action '{other}'")),
    };
    Ok(serde_json::json!({ "generation": generation }))
}

/// Puts back an older index with no announcement: a snapshot (`to`), or the version
/// `generations` writes back.
fn rollback(remote: &Remote, args: &Args) -> Result<u64, String> {
    let bytes = match str_arg(args, "to") {
        Some(name) => remote
            .snapshots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(name)
            .cloned()
            .ok_or_else(|| format!("no snapshot '{name}' (drive_snapshot keeps one)"))?,
        None => {
            let back = args.get("generations").and_then(serde_json::Value::as_u64).unwrap_or(1);
            let history = remote
                .index_history
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let back = usize::try_from(back).unwrap_or(usize::MAX);
            if history.len() <= back {
                return Err(format!(
                    "the remote has {} index versions: none {back} back",
                    history.len()
                ));
            }
            history[history.len() - 1 - back].clone()
        }
    };
    remote
        .write(&index_key(&remote.prefix), &bytes)
        .map_err(|e| e.to_string())?;
    let index: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
    Ok(index.get("generation").and_then(serde_json::Value::as_u64).unwrap_or(0))
}

fn tamper_encrypted(s: &mut DriveState, remote: &Remote, action: &str, args: &Args) -> Answer {
    let drive = super::drive_reading(s, remote, None)?;
    match action {
        "put" => {
            let key = need(args, "key")?;
            drive
                .put(&format!("{}{key}", remote.prefix), &content_of(args)?)
                .map_err(|e| e.to_string())?;
        }
        "delete" => {
            let key = need(args, "key")?;
            drive
                .delete(&format!("{}{key}", remote.prefix))
                .map_err(|e| e.to_string())?;
        }
        "delete_all" => {
            for key in named_files(drive.as_ref(), &remote.prefix)?.keys() {
                drive
                    .delete(&format!("{}{key}", remote.prefix))
                    .map_err(|e| e.to_string())?;
            }
        }
        other => {
            return Err(format!(
                "tamper '{other}' damages one stored object, which an encrypted remote does not \
                 name by the file's key: make the remote with \"encrypted\": false"
            ))
        }
    }
    Ok(serde_json::json!({}))
}

/// Keeps the remote's index as it is now under `name` (for `rollback` `to`).
pub(crate) fn snapshot(args: &Args) -> Answer {
    let remote = with_harness(|h| h.remote_of(args))?;
    let name = need(args, "name")?;
    if remote.encrypted {
        return Err(String::from(
            "an encrypted remote's index is not one object: snapshots need \"encrypted\": false",
        ));
    }
    let bytes = remote
        .read(&index_key(&remote.prefix))
        .ok_or("the remote has no index yet")?;
    remote
        .snapshots
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(name.to_string(), bytes);
    Ok(serde_json::json!({}))
}
