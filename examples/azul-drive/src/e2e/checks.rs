//! `drive_check`: sync-chaos's checks, by the same names and fields, over the devices' folders,
//! their problem lists and the remotes as they really are (past every fault). A check answers
//! `ok` and `detail`; `success` is `ok`, so a failed check fails its step with the detail.

use std::{collections::BTreeSet, fs, path::Path};

use azcloud_kit::sync::remote::{blob_key, index_key, RemoteIndex};

use super::{
    files::{all_files, hash, snapshot, user_files},
    named_files, need, problems_in, remote::glob, str_arg, with_harness, Answer, Args, Harness,
};
use crate::DriveState;

/// A check's verdict: `Ok(())` holds, `Err(detail)` does not.
type Verdict = Result<(), String>;

pub(crate) fn check(s: &mut DriveState, args: &Args) -> Answer {
    let name = need(args, "check")?.to_string();
    // The remote's files are read first (an encrypted remote through a device's drive, which
    // needs the window's state), then the checks run on what was read.
    let remote_view = match name.as_str() {
        "remote_file" | "remote_count" | "remote_consistent" | "kept_live" | "kept" => {
            Some(read_remote(s, args)?)
        }
        _ => None,
    };
    let problems_now = match name.as_str() {
        "problem" => {
            let state_dir = with_harness(|h| Ok(h.device(args)?.state_dir.clone()))?;
            Some(problems_in(state_dir.as_deref()))
        }
        _ => None,
    };
    let last_quiet = match name.as_str() {
        "last_pass_quiet" => Some(with_harness(|h| Ok(h.device(args)?.last.clone()))?),
        _ => None,
    };
    let verdict = with_harness(|h| {
        Ok(match name.as_str() {
            "file" => file(h, args),
            "link" => link(h, args),
            "files_count" => files_count(h, args),
            "conflict_copies" => conflict_copies(h, args),
            "trash_has" => trash_has(h, args),
            "trash_empty" => trash_empty(h, args),
            "no_temp_files" => no_temp_files(h, args),
            "converged" => converged(h, args),
            "kept_live" => kept(h, args, remote_view.as_ref(), false),
            "kept" => kept(h, args, remote_view.as_ref(), true),
            "outside_untouched" => outside_untouched(h),
            "remote_never_has" => remote_never_has(h, args),
            "remote_file" => remote_file(h, args, remote_view.as_ref()),
            "remote_count" => remote_count(args, remote_view.as_ref()),
            "remote_consistent" => remote_consistent(remote_view.as_ref()),
            "problem" => problem(h, args, problems_now.as_deref()),
            "remote_puts" => remote_puts(h, args),
            "retry_after_respected" => retry_after_respected(h, args),
            "state_private" => state_private(h, args),
            "mtime" => mtime(h, args),
            "last_pass_quiet" => last_pass_quiet(last_quiet.flatten()),
            "max_ops" => max_ops(h, args),
            "free_at_least" | "kept_soak" | "run_ok" => Err(format!(
                "check '{name}' needs what AzDrive's E2E commands do not have (the real \
                 disk's free space, a soak, a shell): it is a sync-chaos check only"
            )),
            other => Err(format!("no check '{other}'")),
        })
    })?;
    Ok(match verdict {
        Ok(()) => serde_json::json!({ "ok": true, "detail": "" }),
        Err(detail) => serde_json::json!({
            "ok": false, "success": false, "detail": detail, "error": format!("{name}: {detail}"),
        }),
    })
}

/// A remote as the checks read it: every file by key, with its bytes (`None`: its object is
/// missing) and, on a plain remote, the hash the index names.
pub(crate) struct RemoteView {
    pub files: Vec<RemoteEntry>,
}

pub(crate) struct RemoteEntry {
    pub key: String,
    pub hash: Option<String>,
    pub bytes: Option<Vec<u8>>,
}

fn read_remote(s: &mut DriveState, args: &Args) -> Result<RemoteView, String> {
    let remote = with_harness(|h| h.remote_of(args))?;
    if remote.encrypted {
        // Only a device's key reads an encrypted remote: the first device of it.
        let drive = super::drive_reading(s, &remote, None)?;
        let mut files = Vec::new();
        for key in named_files(drive.as_ref(), &remote.prefix)?.keys() {
            let bytes = drive.get(&format!("{}{key}", remote.prefix)).ok();
            files.push(RemoteEntry {
                key: key.clone(),
                hash: None,
                bytes,
            });
        }
        return Ok(RemoteView { files });
    }
    let files = match remote.read(&index_key(&remote.prefix)) {
        None => Vec::new(),
        Some(bytes) => {
            let index = RemoteIndex::parse(&bytes).map_err(|e| format!("the remote's index: {e}"))?;
            index
                .files
                .iter()
                .map(|(key, f)| RemoteEntry {
                    key: key.clone(),
                    hash: Some(f.hash.clone()),
                    bytes: remote.read(&blob_key(&remote.prefix, &f.hash)),
                })
                .collect()
        }
    };
    Ok(RemoteView { files })
}

fn read(path: &Path) -> Option<Vec<u8>> {
    fs::read(path).ok()
}

fn texts(args: &Args, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(serde_json::Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

fn absent(args: &Args) -> bool {
    super::bool_arg(args, "absent")
}

fn shown(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(80)]).into_owned();
    format!("{text:?}")
}

fn file(h: &Harness, args: &Args) -> Verdict {
    let path = h.path(args, "path")?;
    let what = need(args, "path")?;
    if absent(args) {
        return match fs::symlink_metadata(&path) {
            Ok(_) => Err(format!("{what} exists")),
            Err(_) => Ok(()),
        };
    }
    let meta = fs::symlink_metadata(&path).map_err(|_| format!("{what} does not exist"))?;
    if !meta.file_type().is_file() {
        return Err(format!("{what} is not a file"));
    }
    let data = read(&path).ok_or_else(|| format!("{what} cannot be read"))?;
    if let Some(text) = str_arg(args, "text") {
        if data != text.as_bytes() {
            return Err(format!("{what} holds {}, expected {:?}", shown(&data), text));
        }
    }
    let one_of = texts(args, "one_of");
    if !one_of.is_empty() && !one_of.iter().any(|t| t.as_bytes() == data.as_slice()) {
        return Err(format!("{what} holds {}: none of the expected versions", shown(&data)));
    }
    if let Some(size) = args.get("size").and_then(serde_json::Value::as_u64) {
        if data.len() as u64 != size {
            return Err(format!("{what} has {} bytes, expected {size}", data.len()));
        }
    }
    if str_arg(args, "same_as").is_some() {
        let other = read(&h.path(args, "same_as")?).unwrap_or_default();
        if hash(&other) != hash(&data) {
            return Err(format!("{what} differs from {}", need(args, "same_as")?));
        }
    }
    let mode = mode_of(&meta);
    if let Some(exec) = args.get("exec").and_then(serde_json::Value::as_bool) {
        if (mode & 0o100 != 0) != exec {
            return Err(format!("{what} executable={}, expected {exec}", mode & 0o100 != 0));
        }
    }
    if let Some(max) = str_arg(args, "mode_max") {
        let max = u32::from_str_radix(max, 8).map_err(|e| e.to_string())?;
        if mode & !max != 0 {
            return Err(format!("{what} has mode {mode:o}, more than {max:o}"));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn mode_of(meta: &fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o7777
}

#[cfg(not(unix))]
fn mode_of(meta: &fs::Metadata) -> u32 {
    if meta.permissions().readonly() {
        0o444
    } else {
        0o644
    }
}

fn link(h: &Harness, args: &Args) -> Verdict {
    let path = h.path(args, "path")?;
    let is_link = fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink());
    match (absent(args), is_link) {
        (true, true) => Err(format!("{} is a link", need(args, "path")?)),
        (false, false) => Err(format!("{} is no link", need(args, "path")?)),
        _ => Ok(()),
    }
}

fn count_arg(args: &Args) -> Result<usize, String> {
    args.get("count")
        .and_then(serde_json::Value::as_u64)
        .map(|n| n as usize)
        .ok_or_else(|| String::from("'count' is missing"))
}

fn files_count(h: &Harness, args: &Args) -> Verdict {
    let d = h.device(args)?;
    let pattern = str_arg(args, "glob").unwrap_or("*");
    let n = user_files(&d.root).iter().filter(|(rel, _)| glob(pattern, rel)).count();
    let want = count_arg(args)?;
    if n == want {
        Ok(())
    } else {
        Err(format!("{} has {n} files matching {pattern}, expected {want}", d.name))
    }
}

fn conflict_copies(h: &Harness, args: &Args) -> Verdict {
    let d = h.device(args)?;
    let rel = need(args, "path")?;
    let (folder, name) = rel.rsplit_once('/').map_or(("", rel), |(f, n)| (f, n));
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    let pattern = format!("{stem} (conflict *){ext}");
    let dir = d.root.join(folder);
    let found: Vec<String> = fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| glob(&pattern, n))
                .collect()
        })
        .unwrap_or_default();
    let want = count_arg(args)?;
    if found.len() == want {
        Ok(())
    } else {
        Err(format!(
            "{} has {} conflict copies of {rel} ({found:?}), expected {want}",
            d.name,
            found.len()
        ))
    }
}

/// The device's trash folders, wherever the engine keeps them.
fn trash_dirs(h: &Harness, args: &Args) -> Result<Vec<std::path::PathBuf>, String> {
    let root = h.device(args)?.root.clone();
    Ok(vec![
        root.join(".azdrive-trash"),
        root.join(".azsync").join("trash"),
        root.join(".azlin").join("trash"),
        // The sync engine's own trash (azcloud-kit's root::TRASH_DIR).
        root.join(azcloud_kit::sync::root::TRASH_DIR),
    ])
}

fn trash_has(h: &Harness, args: &Args) -> Verdict {
    let text = need(args, "text")?;
    for dir in trash_dirs(h, args)? {
        if all_files(&dir)
            .iter()
            .any(|(_, p)| read(p).as_deref() == Some(text.as_bytes()))
        {
            return Ok(());
        }
    }
    Err(format!("{}'s trash has no file holding {text:?}", h.device(args)?.name))
}

fn trash_empty(h: &Harness, args: &Args) -> Verdict {
    for dir in trash_dirs(h, args)? {
        let left = all_files(&dir);
        if !left.is_empty() {
            let names: Vec<&String> = left.iter().map(|(r, _)| r).take(5).collect();
            return Err(format!("{}'s trash is not empty: {names:?}", h.device(args)?.name));
        }
    }
    Ok(())
}

/// A temporary file of the engine's: a download not yet renamed in.
fn is_temp(name: &str) -> bool {
    name.starts_with(".azsync-tmp-")
        || (name.starts_with('.') && name.contains(".azul-storage-") && name.ends_with(".tmp"))
}

fn no_temp_files(h: &Harness, args: &Args) -> Verdict {
    let d = h.device(args)?;
    let left: Vec<String> = all_files(&d.root)
        .into_iter()
        .map(|(rel, _)| rel)
        .filter(|rel| is_temp(rel.rsplit('/').next().unwrap_or(rel)))
        .take(5)
        .collect();
    if left.is_empty() {
        Ok(())
    } else {
        Err(format!("{} has temp files left: {left:?}", d.name))
    }
}

fn tree(root: &Path) -> std::collections::BTreeMap<String, String> {
    user_files(root)
        .into_iter()
        .map(|(rel, p)| (rel, hash(&read(&p).unwrap_or_default())))
        .collect()
}

fn converged(h: &Harness, args: &Args) -> Verdict {
    let names = texts(args, "devices");
    let devices: Vec<&super::Device> = if names.is_empty() {
        h.devices.iter().collect()
    } else {
        names
            .iter()
            .map(|n| {
                h.devices
                    .iter()
                    .find(|d| &d.name == n)
                    .ok_or_else(|| format!("no device '{n}'"))
            })
            .collect::<Result<_, _>>()?
    };
    let Some((first, rest)) = devices.split_first() else {
        return Ok(());
    };
    let base = tree(&first.root);
    for d in rest {
        let other = tree(&d.root);
        if other != base {
            let a: BTreeSet<_> = base.iter().collect();
            let b: BTreeSet<_> = other.iter().collect();
            let diff: Vec<_> = a.symmetric_difference(&b).take(6).collect();
            return Err(format!("{} and {} differ: {diff:?}", first.name, d.name));
        }
    }
    Ok(())
}

/// The texts in some folder or on the remote (with `trash`: in a trash too).
fn kept(h: &Harness, args: &Args, remote: Option<&RemoteView>, trash: bool) -> Verdict {
    let mut live: BTreeSet<Vec<u8>> = BTreeSet::new();
    for d in &h.devices {
        for (_, p) in user_files(&d.root) {
            if let Some(b) = read(&p) {
                live.insert(b);
            }
        }
        if trash {
            for dir in [
                d.root.join(".azdrive-trash"),
                d.root.join(".azsync").join("trash"),
                d.root.join(".azlin").join("trash"),
                d.root.join(azcloud_kit::sync::root::TRASH_DIR),
            ] {
                for (_, p) in all_files(&dir) {
                    if let Some(b) = read(&p) {
                        live.insert(b);
                    }
                }
            }
        }
    }
    for e in remote.map(|r| r.files.as_slice()).unwrap_or_default() {
        if let Some(b) = &e.bytes {
            live.insert(b.clone());
        }
    }
    if trash {
        // Every stored version: a plain remote's blobs are its contents.
        if let Ok(r) = h.remote_of(args) {
            if !r.encrypted {
                for (_, b) in r.all_bytes() {
                    live.insert(b);
                }
            }
        }
    }
    // An encrypted drive keeps every version in its history: by the content's hash.
    let versions = if trash {
        h.remote_of(args)
            .ok()
            .filter(|r| r.encrypted)
            .and_then(|r| super::b_tamper::encrypted::every_version_hash(&r).ok())
            .unwrap_or_default()
    } else {
        BTreeSet::new()
    };
    let missing: Vec<String> = texts(args, "texts")
        .into_iter()
        .filter(|t| {
            !live.contains(t.as_bytes())
                && !versions.contains(&azcloud_kit::sync::local::hash_bytes(t.as_bytes()))
        })
        .collect();
    if missing.is_empty() {
        Ok(())
    } else if trash {
        Err(format!("lost everywhere (folders, trash, drive versions): {missing:?}"))
    } else {
        Err(format!("lost (not in any folder nor on the drive): {missing:?}"))
    }
}

fn outside_untouched(h: &Harness) -> Verdict {
    let after = snapshot(&h.outside);
    if after == h.outside_before {
        return Ok(());
    }
    let a: BTreeSet<_> = after.iter().collect();
    let b: BTreeSet<_> = h.outside_before.iter().collect();
    let changed: Vec<_> = a.symmetric_difference(&b).take(5).collect();
    Err(format!("files outside every root changed: {changed:?}"))
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

fn remote_never_has(h: &Harness, args: &Args) -> Verdict {
    let marker = str_arg(args, "marker").map_or_else(|| h.secret.clone(), str::to_string);
    for remote in h.remotes.values() {
        for (key, bytes) in remote.all_bytes() {
            if contains(&bytes, marker.as_bytes()) {
                return Err(format!(
                    "the object {key} of remote {} holds {:?}",
                    remote.name,
                    &marker[..marker.len().min(40)]
                ));
            }
        }
    }
    Ok(())
}

fn remote_file(h: &Harness, args: &Args, remote: Option<&RemoteView>) -> Verdict {
    let key = need(args, "key")?;
    let view = remote.ok_or("the remote was not read")?;
    let entry = view.files.iter().find(|e| e.key == key);
    if absent(args) {
        return match entry {
            Some(_) => Err(format!("the drive has {key:?}")),
            None => Ok(()),
        };
    }
    let Some(entry) = entry else {
        let some: Vec<&String> = view.files.iter().map(|e| &e.key).take(8).collect();
        return Err(format!("the drive has no {key:?} (it has {some:?})"));
    };
    let data = entry
        .bytes
        .as_ref()
        .ok_or_else(|| format!("the drive names {key} but has no object for it"))?;
    if let Some(text) = str_arg(args, "text") {
        if data.as_slice() != text.as_bytes() {
            return Err(format!("the drive's {key} holds {}, expected {text:?}", shown(data)));
        }
    }
    let one_of = texts(args, "one_of");
    if !one_of.is_empty() && !one_of.iter().any(|t| t.as_bytes() == data.as_slice()) {
        return Err(format!(
            "the drive's {key} holds {}: none of the expected versions (a torn read?)",
            shown(data)
        ));
    }
    if super::bool_arg(args, "uniform") && !data.is_empty() && data.iter().any(|b| *b != data[0]) {
        return Err(format!("the drive's {key} mixes two versions (a torn read)"));
    }
    if str_arg(args, "same_as").is_some() {
        let other = read(&h.path(args, "same_as")?).unwrap_or_default();
        if &other != data {
            return Err(format!("the drive's {key} differs from {}", need(args, "same_as")?));
        }
    }
    if args.get("exec").is_some() {
        return Err(String::from(
            "the drive's index keeps no executable bit yet: check exec on a device's file",
        ));
    }
    Ok(())
}

fn remote_count(args: &Args, remote: Option<&RemoteView>) -> Verdict {
    let view = remote.ok_or("the remote was not read")?;
    let pattern = str_arg(args, "glob").unwrap_or("*");
    let n = view.files.iter().filter(|e| glob(pattern, &e.key)).count();
    let want = count_arg(args)?;
    if n == want {
        Ok(())
    } else {
        Err(format!("the drive has {n} keys matching {pattern}, expected {want}"))
    }
}

fn remote_consistent(remote: Option<&RemoteView>) -> Verdict {
    let view = remote.ok_or("the remote was not read")?;
    for e in &view.files {
        let Some(bytes) = &e.bytes else {
            return Err(format!("the index names {:?} but its object is not on the drive", e.key));
        };
        if let Some(h) = &e.hash {
            if &hash(bytes) != h {
                return Err(format!("the object of {:?} does not match its hash", e.key));
            }
        }
    }
    Ok(())
}

fn problem(h: &Harness, args: &Args, now: Option<&[serde_json::Value]>) -> Verdict {
    let d = h.device(args)?;
    let seen = h.seen_problems.get(&d.name).map(Vec::as_slice).unwrap_or_default();
    let every: Vec<serde_json::Value>;
    let list = if str_arg(args, "when") == Some("last") {
        d.last
            .as_ref()
            .and_then(|l| l.get("problems"))
            .and_then(serde_json::Value::as_array)
            .map(Vec::as_slice)
            .or(now)
            .unwrap_or_default()
    } else {
        // Every pass's list, and what the device lists now (a check outside a pass - "Free up
        // space" finding a damaged copy - adds to it between passes).
        every = seen.iter().chain(now.unwrap_or_default()).cloned().collect();
        every.as_slice()
    };
    let mut codes = texts(args, "code_any");
    if codes.is_empty() {
        codes.push(need(args, "code")?.to_string());
    }
    let key = str_arg(args, "key");
    let hits: Vec<&serde_json::Value> = list
        .iter()
        .filter(|p| {
            let code = p.get("code").and_then(serde_json::Value::as_str).unwrap_or("");
            let pkey = p.get("key").and_then(serde_json::Value::as_str).unwrap_or("");
            codes.iter().any(|c| c == code) && key.is_none_or(|k| k == pkey || glob(k, pkey))
        })
        .collect();
    let listed: Vec<(String, String)> = list
        .iter()
        .take(8)
        .map(|p| {
            (
                p.get("code").and_then(serde_json::Value::as_str).unwrap_or("").to_string(),
                p.get("key").and_then(serde_json::Value::as_str).unwrap_or("").to_string(),
            )
        })
        .collect();
    match (absent(args), hits.is_empty()) {
        (true, false) => Err(format!("{} lists {codes:?}: {hits:?}", d.name)),
        (false, true) => Err(format!(
            "{} does not list {}{} (it lists {listed:?})",
            d.name,
            codes.join(" or "),
            key.map(|k| format!(" for {k}")).unwrap_or_default()
        )),
        _ => Ok(()),
    }
}

/// At most `max` destructive changes (of `key`, of kind `op`) in the device's operations log.
fn max_ops(h: &Harness, args: &Args) -> Verdict {
    let d = h.device(args)?;
    let state = d.state_dir.as_ref().ok_or("the device is not paired")?;
    let key = str_arg(args, "key");
    // `change`, not sync-chaos's `op`: a step's own `op` names the command, so a field of that
    // name never reached the check.
    let kind = str_arg(args, "change");
    let n = azcloud_kit::sync::oplog::OpLog::new(state)
        .read()
        .iter()
        .filter(|o| key.is_none_or(|k| o.key == k))
        .filter(|o| {
            kind.is_none_or(|want| {
                serde_json::to_value(o.op).ok().and_then(|v| v.as_str().map(str::to_string))
                    == Some(want.to_string())
            })
        })
        .count();
    let max = args
        .get("max")
        .and_then(serde_json::Value::as_u64)
        .ok_or("'max' is missing")?;
    if n as u64 <= max {
        Ok(())
    } else {
        Err(format!(
            "{} made {n} destructive changes to {}, at most {max} allowed",
            d.name,
            key.unwrap_or("files")
        ))
    }
}

fn remote_puts(h: &Harness, args: &Args) -> Verdict {
    let d = h.device(args)?;
    let remote = h.remote_of(args)?;
    // Content only, as sync-chaos counts blobs: a plain drive's blobs, an encrypted drive's
    // objects, a named drive's files - never the index or the drive's other bookkeeping.
    let content = |key: &str| {
        let rest = key.strip_prefix(remote.prefix.as_str()).unwrap_or(key);
        !rest.starts_with(".azlin/") || rest.starts_with(".azlin/blobs/")
    };
    let puts: u64 = remote
        .counters(Some(&d.name))
        .puts
        .iter()
        .filter(|(key, _)| content(key))
        .map(|(_, n)| n)
        .sum();
    let max = args
        .get("max")
        .and_then(serde_json::Value::as_u64)
        .ok_or("'max' is missing")?;
    if puts <= max {
        Ok(())
    } else {
        Err(format!("{} sent {puts} objects, at most {max} expected", d.name))
    }
}

fn retry_after_respected(h: &Harness, args: &Args) -> Verdict {
    let d = h.device(args)?;
    let c = h.remote_of(args)?.counters(Some(&d.name));
    if c.rate_limited == 0 {
        return Err(String::from(
            "no request was rate limited: the scenario did not test anything",
        ));
    }
    if c.early == 0 {
        Ok(())
    } else {
        Err(format!("{} request(s) came before the server's Retry-After", c.early))
    }
}

fn state_private(h: &Harness, args: &Args) -> Verdict {
    let d = h.device(args)?;
    let Some(state) = &d.state_dir else {
        return Err(String::from("the device is not paired"));
    };
    let mut loose = Vec::new();
    let mut look = |p: &Path| {
        if fs::metadata(p).is_ok_and(|m| mode_of(&m) & 0o077 != 0) {
            loose.push(p.strip_prefix(state).unwrap_or(p).display().to_string());
        }
    };
    look(state);
    for (_, p) in all_files(state) {
        look(&p);
    }
    if loose.is_empty() {
        Ok(())
    } else {
        Err(format!("{}'s state is readable by others: {:?}", d.name, &loose[..loose.len().min(5)]))
    }
}

fn mtime(h: &Harness, args: &Args) -> Verdict {
    let path = h.path(args, "path")?;
    let ns = i128::from(super::b_files::signed_ns(
        fs::metadata(&path)
            .and_then(|m| m.modified())
            .map_err(|e| e.to_string())?,
    ));
    let want = args.get("ns").and_then(serde_json::Value::as_i64).ok_or("'ns' is missing")?;
    let tolerance = args
        .get("tolerance_ns")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    if (ns - i128::from(want)).abs() <= i128::from(tolerance) {
        Ok(())
    } else {
        Err(format!("{} has mtime {ns}, expected {want}", need(args, "path")?))
    }
}

fn last_pass_quiet(last: Option<serde_json::Value>) -> Verdict {
    let Some(last) = last else {
        return Err(String::from("the device has not synced yet"));
    };
    let busy: Vec<String> = ["up", "down", "deleted", "conflicts"]
        .iter()
        .filter(|k| last.get(**k).and_then(serde_json::Value::as_u64).unwrap_or(0) > 0)
        .map(|k| format!("{k}={}", last[*k]))
        .collect();
    if busy.is_empty() {
        Ok(())
    } else {
        Err(format!("the last pass still changed things: {}", busy.join(" ")))
    }
}
