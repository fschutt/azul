//! The file steps of the E2E commands: what a user, an editor or an attacker does to a device's
//! folder between passes (sync-chaos's file steps, by the same names), and the sandbox's own
//! files (the outside folder, a snapshot of it, the clean-up).

use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use super::{bool_arg, need, str_arg, with_harness, Answer, Args, Harness};

/// The engine's bookkeeping at a folder's top: never a user's file.
pub(crate) const BOOKKEEPING: &[&str] = &[".azdrive-trash", ".azlin", ".azsync"];

/// The outside folder: a fake home that no sync may touch.
pub(crate) fn make_outside(outside: &Path, secret: &str) -> std::io::Result<()> {
    fs::create_dir_all(outside.join(".ssh"))?;
    fs::create_dir_all(outside.join("Library").join("LaunchAgents"))?;
    fs::create_dir_all(outside.join("victim"))?;
    fs::write(
        outside.join(".ssh").join("authorized_keys"),
        "ssh-ed25519 AAAA-the-owners-own-key\n",
    )?;
    fs::write(outside.join("secret.txt"), format!("{secret}\n"))?;
    fs::write(
        outside.join("target.txt"),
        "the file outside, hard-linked into the root\n",
    )?;
    Ok(())
}

/// Every entry under `top` (not following links): its kind, its bytes' hash or its link
/// target, its mode. Two snapshots are equal when nothing under `top` changed.
pub(crate) fn snapshot(top: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    walk(top, top, &mut |rel, path, meta| {
        out.insert(rel.to_string(), describe(path, meta));
    });
    out
}

/// One entry as [`snapshot`] keeps it.
fn describe(path: &Path, meta: &fs::Metadata) -> String {
    let ft = meta.file_type();
    if ft.is_symlink() {
        format!(
            "link {}",
            fs::read_link(path).map_or_else(|_| String::new(), |t| t.to_string_lossy().into_owned())
        )
    } else if ft.is_file() {
        let hash = fs::read(path).map_or_else(|_| String::from("unreadable"), |b| hash(&b));
        format!("file {hash} {:o}", mode(meta))
    } else if ft.is_dir() {
        String::from("dir")
    } else {
        String::from("special")
    }
}

/// Calls `f` for every entry under `dir` (relative path with `/`, path, its own metadata).
fn walk(top: &Path, dir: &Path, f: &mut dyn FnMut(&str, &Path, &fs::Metadata)) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        let rel = path
            .strip_prefix(top)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        f(&rel, &path, &meta);
        if meta.file_type().is_dir() {
            walk(top, &path, f);
        }
    }
}

/// The BLAKE3 of `bytes`, as the sync names contents.
pub(crate) fn hash(bytes: &[u8]) -> String {
    azcloud_kit::sync::local::hash_bytes(bytes)
}

#[cfg(unix)]
fn mode(meta: &fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o7777
}

#[cfg(not(unix))]
fn mode(meta: &fs::Metadata) -> u32 {
    if meta.permissions().readonly() {
        0o444
    } else {
        0o644
    }
}

/// The files of a device's folder (regular files, bookkeeping left out): relative path, path.
pub(crate) fn user_files(root: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    walk(root, root, &mut |rel, path, meta| {
        let top = rel.split('/').next().unwrap_or_default();
        if meta.file_type().is_file() && !BOOKKEEPING.contains(&top) {
            out.push((rel.to_string(), path.to_path_buf()));
        }
    });
    out
}

/// Every file under `top`, bookkeeping included (a trash folder's).
pub(crate) fn all_files(top: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    walk(top, top, &mut |rel, path, meta| {
        if meta.file_type().is_file() {
            out.push((rel.to_string(), path.to_path_buf()));
        }
    });
    out
}

/// Removes a sandbox, files a scenario made read-only or unreadable too.
pub(crate) fn clean(top: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        walk(top, top, &mut |_, path, meta| {
            if !meta.file_type().is_symlink() {
                let want = if meta.is_dir() { 0o755 } else { 0o644 };
                let _ = fs::set_permissions(path, fs::Permissions::from_mode(want));
            }
        });
    }
    let _ = fs::remove_dir_all(top);
}

/// The bytes a step writes: `text`, `hex`, `fill` repeated `size` times, or `size` bytes from a
/// generator seeded with `seed`.
pub(crate) fn content_of(args: &Args) -> Result<Vec<u8>, String> {
    if let Some(text) = str_arg(args, "text") {
        return Ok(text.as_bytes().to_vec());
    }
    if let Some(hex) = str_arg(args, "hex") {
        return (0..hex.len())
            .step_by(2)
            .map(|i| {
                hex.get(i..i + 2)
                    .and_then(|h| u8::from_str_radix(h, 16).ok())
                    .ok_or_else(|| format!("'{hex}' is not hex"))
            })
            .collect();
    }
    let size = args.get("size").and_then(serde_json::Value::as_u64);
    if let Some(fill) = str_arg(args, "fill") {
        let n = usize::try_from(size.unwrap_or(1)).map_err(|e| e.to_string())?;
        return Ok(fill.as_bytes().repeat(n));
    }
    if let Some(size) = size {
        let seed = args.get("seed").and_then(serde_json::Value::as_u64).unwrap_or(1);
        return Ok(seeded(seed, size));
    }
    Ok(Vec::new())
}

/// `len` repeatable bytes for `seed` (splitmix64).
fn seeded(seed: u64, len: u64) -> Vec<u8> {
    let mut state = seed;
    let mut out = Vec::with_capacity(usize::try_from(len).unwrap_or(0));
    while (out.len() as u64) < len {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        for b in z.to_le_bytes() {
            if (out.len() as u64) < len {
                out.push(b);
            }
        }
    }
    out
}

fn io(e: std::io::Error, what: &Path) -> String {
    format!("{}: {e}", what.display())
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| io(e, parent))?;
    }
    fs::write(path, bytes).map_err(|e| io(e, path))
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|e| io(e, path))
}

#[cfg(not(unix))]
fn set_mode(path: &Path, mode: u32) -> Result<(), String> {
    let mut perms = fs::metadata(path).map_err(|e| io(e, path))?.permissions();
    perms.set_readonly(mode & 0o200 == 0);
    fs::set_permissions(path, perms).map_err(|e| io(e, path))
}

fn remember_text(h: &mut Harness, args: &Args) {
    if let Some(text) = str_arg(args, "text") {
        h.texts.insert(text.to_string());
    }
}

/// Runs the file step `name`.
pub(crate) fn file_op(name: &str, args: &Args) -> Answer {
    with_harness(|h| {
        match name {
            "drive_write" => {
                // `path_hex`: a name of raw bytes (one that is no UTF-8 cannot be a JSON string).
                let path = match str_arg(args, "path_hex") {
                    Some(hex) => super::b_files::raw_path(&h.device(args)?.root, hex)?,
                    None => h.path(args, "path")?,
                };
                write_file(&path, &content_of(args)?)?;
                if bool_arg(args, "exec") {
                    set_mode(&path, 0o755)?;
                }
                // A time before 1970 is a negative number of nanoseconds.
                if let Some(ns) = args.get("mtime").and_then(serde_json::Value::as_i64) {
                    super::b_files::set_mtime_signed(&path, ns).map_err(|e| io(e, &path))?;
                }
                remember_text(h, args);
            }
            "drive_overwrite_in_place" => {
                let path = h.path(args, "path")?;
                let mut f = fs::File::options()
                    .write(true)
                    .open(&path)
                    .map_err(|e| io(e, &path))?;
                let bytes = content_of(args)?;
                f.write_all(&bytes).map_err(|e| io(e, &path))?;
                f.set_len(bytes.len() as u64).map_err(|e| io(e, &path))?;
                remember_text(h, args);
            }
            "drive_append" => {
                let path = h.path(args, "path")?;
                let mut f = fs::File::options()
                    .append(true)
                    .create(true)
                    .open(&path)
                    .map_err(|e| io(e, &path))?;
                f.write_all(&content_of(args)?).map_err(|e| io(e, &path))?;
            }
            "drive_mkdir" => {
                let path = h.path(args, "path")?;
                fs::create_dir_all(&path).map_err(|e| io(e, &path))?;
            }
            "drive_rm" => {
                if let Some(pattern) = str_arg(args, "glob") {
                    let root = h.device(args)?.root.clone();
                    for (rel, path) in user_files(&root) {
                        if super::remote::glob(pattern, &rel) {
                            fs::remove_file(&path).map_err(|e| io(e, &path))?;
                        }
                    }
                } else {
                    let path = h.path(args, "path")?;
                    fs::remove_file(&path).map_err(|e| io(e, &path))?;
                }
            }
            "drive_rmtree" => {
                let path = h.path(args, "path")?;
                fs::remove_dir_all(&path).map_err(|e| io(e, &path))?;
            }
            "drive_rename" => {
                let from = h.path(args, "from")?;
                let to = h.path(args, "to")?;
                if let Some(parent) = to.parent() {
                    fs::create_dir_all(parent).map_err(|e| io(e, parent))?;
                }
                fs::rename(&from, &to).map_err(|e| io(e, &from))?;
            }
            "drive_symlink" => {
                let path = h.path(args, "path")?;
                let target = need(args, "target")?;
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(|e| io(e, parent))?;
                }
                symlink(Path::new(target), &path)?;
            }
            "drive_hardlink" => {
                let path = h.path(args, "path")?;
                let target = h.path(args, "target")?;
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(|e| io(e, parent))?;
                }
                fs::hard_link(&target, &path).map_err(|e| io(e, &path))?;
            }
            "drive_mkfifo" => {
                let path = h.path(args, "path")?;
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(|e| io(e, parent))?;
                }
                // The system's own tool: no new crate for one system call a test needs.
                let made = std::process::Command::new("mkfifo")
                    .arg(&path)
                    .status()
                    .map_err(|e| format!("mkfifo: {e}"))?;
                if !made.success() {
                    return Err(format!("mkfifo {} failed", path.display()));
                }
            }
            "drive_chmod" => {
                let path = h.path(args, "path")?;
                let mode = u32::from_str_radix(need(args, "mode")?, 8)
                    .map_err(|e| format!("mode: {e}"))?;
                set_mode(&path, mode)?;
            }
            "drive_utime" => {
                let path = h.path(args, "path")?;
                // Nanoseconds since 1970, negative before it (a photo from 1969).
                let ns = match str_arg(args, "like") {
                    Some(_) => {
                        let like = h.path(args, "like")?;
                        let t = fs::metadata(&like)
                            .and_then(|m| m.modified())
                            .map_err(|e| io(e, &like))?;
                        super::b_files::signed_ns(t)
                    }
                    None => args
                        .get("mtime")
                        .and_then(serde_json::Value::as_i64)
                        .ok_or("'mtime' or 'like' is missing")?,
                };
                super::b_files::set_mtime_signed(&path, ns).map_err(|e| io(e, &path))?;
            }
            "drive_many" => {
                let dir = h.path(args, "dir")?;
                let count = args
                    .get("count")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or("'count' is missing")?;
                let size = args.get("size").and_then(serde_json::Value::as_u64).unwrap_or(64);
                let seed = args.get("seed").and_then(serde_json::Value::as_u64).unwrap_or(1);
                let prefix = str_arg(args, "text_prefix");
                for i in 0..count {
                    let sub = dir.join(format!("d{:03}", i / 500));
                    let bytes = match prefix {
                        Some(p) => format!("{p} {i}").into_bytes(),
                        None => seeded(seed.wrapping_add(i), size),
                    };
                    write_file(&sub.join(format!("f{i:06}.txt")), &bytes)?;
                }
            }
            "drive_office_save" => {
                // Word's save of doc.docx: the new version into a temp file, the old one
                // renamed aside, the new one renamed in, the old one deleted; the lock file
                // ~$doc.docx stays while the document is open.
                let path = h.path(args, "path")?;
                let folder = path.parent().map(Path::to_path_buf).unwrap_or_default();
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let lock_name: String = if name.chars().count() > 2 {
                    name.chars().skip(2).collect()
                } else {
                    name.clone()
                };
                let lock = folder.join(format!("~${lock_name}"));
                write_file(&lock, b"owner")?;
                let new = folder.join("~WRD0001.tmp");
                let old = folder.join("~WRL0002.tmp");
                write_file(&new, need(args, "text")?.as_bytes())?;
                fs::rename(&path, &old).map_err(|e| io(e, &path))?;
                fs::rename(&new, &path).map_err(|e| io(e, &new))?;
                if !bool_arg(args, "keep_temp") {
                    fs::remove_file(&old).map_err(|e| io(e, &old))?;
                }
                if !bool_arg(args, "keep_lock") {
                    fs::remove_file(&lock).map_err(|e| io(e, &lock))?;
                }
                remember_text(h, args);
            }
            "drive_editor_save" => {
                // An editor's safe save: a sibling written, then renamed over the file (vim's
                // backupcopy=no); style "backup" leaves `file~` beside it.
                let path = h.path(args, "path")?;
                let folder = path.parent().map(Path::to_path_buf).unwrap_or_default();
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if str_arg(args, "style") == Some("backup") {
                    let backup = folder.join(format!("{name}~"));
                    fs::copy(&path, &backup).map_err(|e| io(e, &backup))?;
                }
                let swap = folder.join(format!(".{name}.swp"));
                write_file(&swap, b"vim swap")?;
                let tmp = folder.join(format!(".{name}.new-4913"));
                write_file(&tmp, need(args, "text")?.as_bytes())?;
                fs::rename(&tmp, &path).map_err(|e| io(e, &tmp))?;
                if !bool_arg(args, "keep_swap") {
                    fs::remove_file(&swap).map_err(|e| io(e, &swap))?;
                }
                remember_text(h, args);
            }
            "drive_move_root" => {
                let suffix = str_arg(args, "suffix").unwrap_or("-moved").to_string();
                let d = h.device_mut(args)?;
                let moved = PathBuf::from(format!("{}{suffix}", d.root.display()));
                fs::rename(&d.root, &moved).map_err(|e| io(e, &d.root))?;
                d.root = moved;
            }
            "drive_remove_root" => {
                let root = h.device(args)?.root.clone();
                clean(&root);
            }
            "drive_replace_root" => {
                let root = h.device(args)?.root.clone();
                clean(&root);
                fs::create_dir_all(&root).map_err(|e| io(e, &root))?;
            }
            "drive_wipe_state" => {
                let state = paired_state(h, args)?;
                for entry in fs::read_dir(&state).map_err(|e| io(e, &state))?.flatten() {
                    let path = entry.path();
                    let gone = if path.is_dir() {
                        fs::remove_dir_all(&path)
                    } else {
                        fs::remove_file(&path)
                    };
                    gone.map_err(|e| io(e, &path))?;
                }
            }
            "drive_corrupt_state" => {
                let state = paired_state(h, args)?;
                let base = state.join(azcloud_kit::sync::session::INDEX_FILE);
                write_file(&base, br#"{"format": "azcloud.sync-local", "generation": 3, "files": {"half": "#)?;
            }
            "drive_list" => {
                let root = h.device(args)?.root.clone();
                let mut files = serde_json::Map::new();
                walk(&root, &root, &mut |rel, path, meta| {
                    let top = rel.split('/').next().unwrap_or_default();
                    if BOOKKEEPING.contains(&top) || meta.is_dir() {
                        return;
                    }
                    files.insert(rel.to_string(), entry_json(path, meta));
                });
                return Ok(serde_json::json!({ "files": files }));
            }
            "drive_read" => {
                let path = h.path(args, "path")?;
                return Ok(match fs::symlink_metadata(&path) {
                    Ok(meta) => {
                        let mut out = entry_json(&path, &meta);
                        if let serde_json::Value::Object(map) = &mut out {
                            map.insert("exists".into(), true.into());
                        }
                        out
                    }
                    Err(_) => serde_json::json!({ "exists": false }),
                });
            }
            other => return Err(format!("'{other}' is not one of AzDrive's commands")),
        }
        Ok(serde_json::json!({}))
    })
}

/// One path as `drive_list` and `drive_read` show it.
fn entry_json(path: &Path, meta: &fs::Metadata) -> serde_json::Value {
    let ft = meta.file_type();
    if ft.is_symlink() {
        let target = fs::read_link(path).map(|t| t.to_string_lossy().into_owned()).ok();
        return serde_json::json!({ "kind": "link", "target": target });
    }
    if ft.is_dir() {
        return serde_json::json!({ "kind": "dir", "mode": format!("{:o}", mode(meta)) });
    }
    if !ft.is_file() {
        return serde_json::json!({ "kind": "special" });
    }
    let bytes = fs::read(path).unwrap_or_default();
    serde_json::json!({
        "kind": "file",
        "size": meta.len(),
        "hash": hash(&bytes),
        "text": String::from_utf8(bytes).ok(),
        "mode": format!("{:o}", mode(meta)),
        "mtime_ns": meta.modified().ok().map(super::b_files::signed_ns),
    })
}

fn paired_state(h: &Harness, args: &Args) -> Result<PathBuf, String> {
    h.device(args)?
        .state_dir
        .clone()
        .ok_or_else(|| String::from("the device is not paired"))
}

#[cfg(unix)]
fn symlink(target: &Path, path: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(target, path).map_err(|e| io(e, path))
}

#[cfg(windows)]
fn symlink(target: &Path, path: &Path) -> Result<(), String> {
    let made = if target.is_dir() {
        std::os::windows::fs::symlink_dir(target, path)
    } else {
        std::os::windows::fs::symlink_file(target, path)
    };
    made.map_err(|e| io(e, path))
}

/// The time now in nanoseconds (a check's tolerance is against it).
#[allow(dead_code)]
pub(crate) fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_content_is_repeatable_and_has_its_size() {
        assert_eq!(seeded(3, 100), seeded(3, 100));
        assert_ne!(seeded(3, 100), seeded(4, 100));
        assert_eq!(seeded(1, 13).len(), 13);
    }

    #[test]
    fn content_comes_from_text_hex_fill_or_a_seed() {
        let args = |v: serde_json::Value| v.as_object().cloned().unwrap_or_default();
        assert_eq!(content_of(&args(serde_json::json!({"text": "hi"}))).unwrap(), b"hi");
        assert_eq!(content_of(&args(serde_json::json!({"hex": "00ff"}))).unwrap(), vec![0, 255]);
        assert_eq!(content_of(&args(serde_json::json!({"fill": "ab", "size": 3}))).unwrap(), b"ababab");
        assert_eq!(content_of(&args(serde_json::json!({"size": 5, "seed": 9}))).unwrap().len(), 5);
        assert!(content_of(&args(serde_json::json!({"hex": "zz"}))).is_err());
    }
}
