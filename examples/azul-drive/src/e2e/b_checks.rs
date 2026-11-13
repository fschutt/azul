//! Group B's `drive_check` checks: what a remote stores (fresh nonces, no object shared between
//! accounts or across a permanent delete) and what AzDrive leaves readable on this computer (no
//! file name, no secret outside its private files).

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use super::{
    b_tamper::{content_objects, marked},
    files::user_files,
    need, str_arg, with_harness, Answer, Args, Harness,
};
use crate::DriveState;

/// The checks of this module (the others are `checks.rs`'s).
const CHECKS: &[&str] = &[
    "nonces_unique",
    "remote_objects_disjoint",
    "objects_fresh",
    "logs_never_have",
    "logs_clean",
];

/// Whether this module answers the `drive_check` step `args`.
pub(crate) fn owns(args: &Args) -> bool {
    str_arg(args, "check").is_some_and(|c| CHECKS.contains(&c))
}

type Verdict = Result<(), String>;

pub(crate) fn check(_s: &mut DriveState, args: &Args) -> Answer {
    let name = need(args, "check")?.to_string();
    let verdict = with_harness(|h| {
        Ok(match name.as_str() {
            "nonces_unique" => nonces_unique(h, args),
            "remote_objects_disjoint" => objects_disjoint(h, args),
            "objects_fresh" => objects_fresh(h, args),
            "logs_never_have" => {
                let marker = need(args, "marker")?.to_string();
                never_readable(h, &[marker])
            }
            "logs_clean" => {
                let names = file_names(h, args)?;
                if names.is_empty() {
                    Err(String::from("the device holds no file: the scenario tests nothing"))
                } else {
                    never_readable(h, &names)
                }
            }
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

// ==== What a remote stores ====

/// The nonces in an AZL1 object's public header: the segments' STREAM prefix (32..51), the
/// info's (176..200) and the trailer's (276..300).
fn nonces_of(bytes: &[u8]) -> Option<[&[u8]; 3]> {
    if bytes.len() < 300 || &bytes[..4] != b"AZL1" {
        return None;
    }
    Some([&bytes[32..51], &bytes[176..200], &bytes[276..300]])
}

/// No nonce of one AZL1 object (segments, info, trailer) is used again, in it or in another.
fn nonces_unique(h: &Harness, args: &Args) -> Verdict {
    let remote = h.remote_of(args)?;
    let mut seen: BTreeMap<Vec<u8>, String> = BTreeMap::new();
    let mut objects = 0usize;
    for key in remote.keys().into_iter().filter(|k| k.starts_with("data/")) {
        let Some(bytes) = remote.read(&key) else {
            continue;
        };
        let Some(nonces) = nonces_of(&bytes) else {
            continue;
        };
        objects += 1;
        for (what, nonce) in ["segments", "info", "trailer"].iter().zip(nonces) {
            if let Some(before) = seen.insert(nonce.to_vec(), format!("{key} ({what})")) {
                return Err(format!("{key} ({what}) uses the nonce of {before} again"));
            }
        }
    }
    if objects == 0 {
        return Err(String::from(
            "the remote holds no AZL1 object: the scenario tests nothing (an encrypted remote?)",
        ));
    }
    Ok(())
}

/// No content object of one remote has the bytes of one of another (no deduplication across
/// accounts).
fn objects_disjoint(h: &Harness, args: &Args) -> Verdict {
    let names: Vec<String> = args
        .get("remotes")
        .and_then(serde_json::Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    if names.len() < 2 {
        return Err(String::from("'remotes' names fewer than two remotes"));
    }
    let mut owner: BTreeMap<String, (String, String)> = BTreeMap::new();
    for name in &names {
        let remote = h
            .remotes
            .get(name)
            .ok_or_else(|| format!("no remote '{name}'"))?;
        for (key, hash) in content_objects(remote) {
            if let Some((other, other_key)) = owner.get(&hash) {
                if other != name {
                    return Err(format!(
                        "{name}'s {key} holds the same bytes as {other}'s {other_key}"
                    ));
                }
            }
            owner.insert(hash, (name.clone(), key));
        }
    }
    Ok(())
}

/// No content object stored now is one recorded by `drive_mark_objects` under `since`: not
/// the same key, not the same bytes.
fn objects_fresh(h: &Harness, args: &Args) -> Verdict {
    let remote = h.remote_of(args)?;
    let since = need(args, "since")?;
    let then = marked(&remote, since)
        .ok_or_else(|| format!("no objects were marked as '{since}' (drive_mark_objects)"))?;
    let old_hashes: BTreeSet<&String> = then.values().collect();
    for (key, hash) in content_objects(&remote) {
        if then.contains_key(&key) {
            return Err(format!("{key} was stored at '{since}' and still is"));
        }
        if old_hashes.contains(&hash) {
            return Err(format!("{key} holds the bytes of an object stored at '{since}'"));
        }
    }
    Ok(())
}

// ==== What AzDrive leaves readable here ====

/// The names of the files in the device's folder (five bytes or more: a shorter one would
/// match by chance).
fn file_names(h: &Harness, args: &Args) -> Result<Vec<String>, String> {
    let root = h.device(args)?.root.clone();
    Ok(user_files(&root)
        .into_iter()
        .filter_map(|(rel, _)| rel.rsplit('/').next().map(str::to_string))
        .filter(|name| name.len() >= 5)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect())
}

/// AzDrive's own folders in this run: its data, its cache, its drives file, its home, and the
/// run's cache folder (where the pairings' state lies).
fn app_folders(h: &Harness) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = ["AZLIN_DATA", "AZDRIVE_CACHE", "AZDRIVE_HOME", "AZUL_DRIVES"]
        .iter()
        .filter_map(|var| std::env::var_os(var).map(PathBuf::from))
        .collect();
    out.push(h.run.join("cache"));
    out
}

/// What is never searched: the files the user put there (the devices' folders), the remotes
/// (the stand-in cloud), the outside folder and the pairings' private state folders.
fn skipped(h: &Harness) -> Vec<PathBuf> {
    let mut out = vec![h.run.join("remote"), h.run.join("devices"), h.outside.clone()];
    out.extend(h.devices.iter().filter_map(|d| d.state_dir.clone()));
    out.extend(h.devices.iter().map(|d| d.root.clone()));
    out
}

/// No file of AzDrive's folders that others can read holds one of `needles`, in its name or in
/// its bytes. A private file (no permission for the group or others) may: that is where the
/// sync keeps what names files.
fn never_readable(h: &Harness, needles: &[String]) -> Verdict {
    let skip = skipped(h);
    let mut found = Vec::new();
    for top in app_folders(h) {
        visit(&top, &skip, &mut |path| {
            if is_private(path) {
                return;
            }
            let name = path.to_string_lossy();
            let bytes = fs::read(path).unwrap_or_default();
            for needle in needles {
                if name.contains(needle.as_str()) || contains(&bytes, needle.as_bytes()) {
                    found.push(format!("{} holds {needle:?}", path.display()));
                }
            }
        });
    }
    if found.is_empty() {
        Ok(())
    } else {
        Err(found.into_iter().take(5).collect::<Vec<_>>().join("; "))
    }
}

/// Calls `f` for every file under `top` (or `top` itself when it is a file), not under `skip`
/// and not following links.
fn visit(top: &Path, skip: &[PathBuf], f: &mut dyn FnMut(&Path)) {
    if skip.iter().any(|s| top.starts_with(s)) {
        return;
    }
    let Ok(meta) = fs::symlink_metadata(top) else {
        return;
    };
    if meta.is_file() {
        f(top);
        return;
    }
    if !meta.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(top) else {
        return;
    };
    for entry in entries.flatten() {
        visit(&entry.path(), skip, f);
    }
}

#[cfg(unix)]
fn is_private(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o077 == 0)
}

/// Windows keeps a user's files private by their folder's ACL: every file counts.
#[cfg(not(unix))]
fn is_private(_path: &Path) -> bool {
    false
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_azl1_header_shows_its_three_nonces_and_anything_else_none() {
        let mut header = vec![0u8; 4096];
        header[..4].copy_from_slice(b"AZL1");
        header[32] = 1;
        header[176] = 2;
        header[276] = 3;
        let nonces = nonces_of(&header).expect("an AZL1 header");
        assert_eq!(nonces[0][0], 1);
        assert_eq!(nonces[1][0], 2);
        assert_eq!(nonces[2][0], 3);
        assert_eq!(nonces[0].len(), 19);
        assert_eq!(nonces[1].len(), 24);
        assert!(nonces_of(b"plain text").is_none());
    }

    #[cfg(unix)]
    #[test]
    fn a_readable_file_that_names_a_file_is_found_and_a_private_one_is_not() {
        use std::os::unix::fs::PermissionsExt;
        let dir = azul_storage::testing::TempDir::new("azdrive-b-logs");
        let open = dir.path().join("app.log");
        let private = dir.path().join("state.json");
        fs::write(&open, "synced canary-diagnosis-7f3a.pdf").unwrap();
        fs::write(&private, "canary-diagnosis-7f3a.pdf").unwrap();
        fs::set_permissions(&open, fs::Permissions::from_mode(0o644)).unwrap();
        fs::set_permissions(&private, fs::Permissions::from_mode(0o600)).unwrap();
        let mut hits = Vec::new();
        visit(dir.path(), &[], &mut |p| {
            if !is_private(p) && contains(&fs::read(p).unwrap(), b"canary-diagnosis-7f3a") {
                hits.push(p.to_path_buf());
            }
        });
        assert_eq!(hits, vec![open]);
    }
}
