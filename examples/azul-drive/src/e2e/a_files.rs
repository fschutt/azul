//! What other programs do to a device's folder that the shared file steps do not: write into
//! the middle of a file (`drive_write_at`, `drive_patch`), set its size (`drive_truncate`), make
//! a sparse one (`drive_sparse`), copy one through the system (`drive_copy`), make a Finder alias
//! (`drive_alias_file`) or a Windows junction (`drive_junction`), swap a folder for a link and
//! back as fast as they can (`drive_racer`), and many files in one folder (`drive_many` with
//! `flat`).

use std::{
    fs,
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
};

use azul::prelude::*;

use super::{
    a_place::{job, job_waiting},
    a_state::with_a,
    files::content_of,
    need, pending, str_arg, with_harness, Answer, Args,
};
use crate::{sync_jobs, DriveState};

fn io(e: std::io::Error, what: &Path) -> String {
    format!("{}: {e}", what.display())
}

/// Writes `bytes` at `offset` of the file at `path` (made when missing), nothing cut.
fn write_at(path: &Path, offset: u64, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| io(e, parent))?;
    }
    let mut file = fs::File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|e| io(e, path))?;
    file.seek(SeekFrom::Start(offset))
        .and_then(|_| file.write_all(bytes))
        .map_err(|e| io(e, path))
}

/// `drive_write_at` / `drive_patch`: content (as `drive_write`) written at `offset`, nothing
/// cut - Explorer's out-of-order chunks, a database's page, a byte patched in place.
pub(super) fn write_at_step(args: &Args) -> Answer {
    let path = with_harness(|h| h.path(args, "path"))?;
    let offset = args
        .get("offset")
        .and_then(serde_json::Value::as_u64)
        .ok_or("'offset' is missing")?;
    let bytes = content_of(args)?;
    write_at(&path, offset, &bytes)?;
    Ok(serde_json::json!({ "written": bytes.len() }))
}

/// `drive_truncate`: the file's size set (made when missing): cut, or grown with a hole of
/// zeros, as `SetEndOfFile` / `ftruncate` does.
pub(super) fn truncate(args: &Args) -> Answer {
    let path = with_harness(|h| h.path(args, "path"))?;
    let size = args
        .get("size")
        .and_then(serde_json::Value::as_u64)
        .ok_or("'size' is missing")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| io(e, parent))?;
    }
    fs::File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .and_then(|f| f.set_len(size))
        .map_err(|e| io(e, &path))?;
    Ok(serde_json::json!({ "size": size }))
}

/// `drive_sparse`: a sparse file of `size` bytes whose only data is `text` (or content as
/// `drive_write`) at `data_at`; the disk keeps the rest as a hole.
pub(super) fn sparse(args: &Args) -> Answer {
    let path = with_harness(|h| h.path(args, "path"))?;
    let size = args
        .get("size")
        .and_then(serde_json::Value::as_u64)
        .ok_or("'size' is missing")?;
    let at = args.get("data_at").and_then(serde_json::Value::as_u64).unwrap_or(0);
    let bytes = content_of(args)?;
    if at + bytes.len() as u64 > size {
        return Err(format!("{} bytes at {at} do not fit in {size}", bytes.len()));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| io(e, parent))?;
    }
    fs::File::create(&path)
        .and_then(|f| f.set_len(size))
        .map_err(|e| io(e, &path))?;
    write_at(&path, at, &bytes)?;
    Ok(serde_json::json!({ "size": size }))
}

/// `drive_copy`: a file copied through the system, as Finder or Explorer copies it: `from` (of
/// the device's folder) to `to` (anywhere in the sandbox). A cloud-only file is brought down
/// first, as reading it through the system does - never copied as a stand-in.
pub(super) fn copy(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, args: &Args) -> Answer {
    let (drive_id, root) = with_harness(|h| {
        let d = h.device(args)?;
        Ok((d.drive_id.clone(), d.root.clone()))
    })?;
    let from = with_harness(|h| h.path(args, "from"))?;
    let to = with_harness(|h| h.path(args, "to"))?;
    let key = from
        .strip_prefix(&root)
        .map(|k| k.to_string_lossy().replace('\\', "/"))
        .ok();
    let cloud_only = key.as_ref().is_some_and(|k| {
        s.sync_view
            .store
            .states(&drive_id)
            .files
            .get(k)
            .is_some_and(|r| r.cloud_only)
    });
    if cloud_only || job_waiting(args)? {
        let keys: Vec<String> = key.into_iter().collect();
        if job(args, |id| sync_jobs::fetch(info, app, s, id, keys))?.is_none() {
            return Ok(pending());
        }
    }
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|e| io(e, parent))?;
    }
    let bytes = fs::copy(&from, &to).map_err(|e| io(e, &from))?;
    Ok(serde_json::json!({ "copied": bytes }))
}

/// `drive_move_in`: AzDrive's move of `from` (a file or folder outside the device's folder)
/// into it at `to`: copied now, each source removed only once a pass committed its copy (the
/// pass the move starts, or a later one). Answers `moved` (the key).
pub(super) fn move_in(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, args: &Args) -> Answer {
    let from = with_harness(|h| h.path(args, "from"))?;
    let key = need(args, "to")?.trim_matches('/').to_string();
    let started = job(args, |id| {
        sync_jobs::move_in(info, app, s, id, from.clone(), key.clone())
    })?;
    if started.is_none() {
        return Ok(pending());
    }
    Ok(serde_json::json!({ "moved": key }))
}

/// The Finder's flags of an alias file (`com.apple.FinderInfo`): type `alis`, creator `MACS`,
/// the "is alias" flag; 32 bytes.
fn alias_finder_info() -> String {
    let mut info = [0u8; 32];
    info[..4].copy_from_slice(b"alis");
    info[4..8].copy_from_slice(b"MACS");
    info[8] = 0x80;
    info.iter().map(|b| format!("{b:02x}")).collect()
}

/// `drive_alias_file` (macOS): a Finder alias at `path` to `target` - a small file of bookmark
/// data naming the target (never the target's bytes) with the Finder's alias flags. The sync
/// takes it as the small file it is.
pub(super) fn alias_file(args: &Args) -> Answer {
    if !cfg!(target_os = "macos") {
        return Err(String::from("a Finder alias is macOS's (only_on macos)"));
    }
    let path = with_harness(|h| h.path(args, "path"))?;
    let target = need(args, "target")?;
    let mut bookmark = b"book\0\0\0\0mark\0\0\0\0".to_vec();
    bookmark.extend_from_slice(&(target.len() as u32).to_le_bytes());
    bookmark.extend_from_slice(target.as_bytes());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| io(e, parent))?;
    }
    fs::write(&path, &bookmark).map_err(|e| io(e, &path))?;
    // The system's own tool sets the extended attribute (no new crate for one call).
    let set = std::process::Command::new("xattr")
        .arg("-wx")
        .arg("com.apple.FinderInfo")
        .arg(alias_finder_info())
        .arg(&path)
        .status()
        .map_err(|e| format!("xattr: {e}"))?;
    if !set.success() {
        return Err(format!("xattr could not mark {} as an alias", path.display()));
    }
    Ok(serde_json::json!({ "bytes": bookmark.len() }))
}

/// `drive_junction` (Windows): a directory junction at `path` to `target`.
pub(super) fn junction(args: &Args) -> Answer {
    if !cfg!(windows) {
        return Err(String::from("a junction is Windows' (only_on windows)"));
    }
    let path = with_harness(|h| h.path(args, "path"))?;
    let target = with_harness(|h| h.path(args, "target"))?;
    let made = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&path)
        .arg(&target)
        .status()
        .map_err(|e| format!("mklink: {e}"))?;
    if !made.success() {
        return Err(format!("mklink /J {} failed", path.display()));
    }
    Ok(serde_json::json!({}))
}

/// A racer running: its stop, its thread, the folder it swaps and where the real one waits.
pub(super) struct Racer {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<u64>>,
    path: PathBuf,
    aside: PathBuf,
}

impl Racer {
    /// Stops it; the real folder is back at its place. How many swaps it made.
    pub(super) fn stop(mut self) -> u64 {
        self.stop.store(true, Ordering::SeqCst);
        let swaps = self.thread.take().and_then(|t| t.join().ok()).unwrap_or(0);
        put_back(&self.path, &self.aside);
        swaps
    }
}

/// The real folder back at `path` (a link there removed first).
fn put_back(path: &Path, aside: &Path) {
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        let _ = fs::remove_file(path);
    }
    if aside.exists() && fs::symlink_metadata(path).is_err() {
        let _ = fs::rename(aside, path);
    }
}

#[cfg(unix)]
fn link(target: &Path, at: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, at)
}

#[cfg(windows)]
fn link(target: &Path, at: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(target, at)
}

/// `drive_racer`: a thread that swaps the folder at `path` for a link to `target` and back, as
/// fast as it can, until `drive_racer_stop` (a time-of-check attack on every step of a pass).
pub(super) fn racer(args: &Args) -> Answer {
    let (device, path, run) = with_harness(|h| {
        Ok((h.device(args)?.name.clone(), h.path(args, "path")?, h.run.clone()))
    })?;
    let target = with_harness(|h| h.path(args, "target"))?;
    if !path.is_dir() {
        return Err(format!("{} is no folder to swap", path.display()));
    }
    // The real folder waits outside the device's folder, as an attacker's swap leaves it.
    let aside = run.join(format!("racer-{device}-real"));
    let stop = Arc::new(AtomicBool::new(false));
    let thread = {
        let (stop, path, aside) = (stop.clone(), path.clone(), aside.clone());
        std::thread::spawn(move || {
            let mut swaps = 0u64;
            while !stop.load(Ordering::SeqCst) {
                if fs::rename(&path, &aside).is_ok() {
                    if link(&target, &path).is_ok() {
                        swaps += 1;
                        std::thread::yield_now();
                        let _ = fs::remove_file(&path);
                    }
                    let _ = fs::rename(&aside, &path);
                }
                std::thread::yield_now();
            }
            swaps
        })
    };
    let old = with_a(|a| {
        a.racers.insert(
            device,
            Racer {
                stop,
                thread: Some(thread),
                path,
                aside,
            },
        )
    })?;
    if let Some(old) = old {
        old.stop();
    }
    Ok(serde_json::json!({ "racing": true }))
}

/// `drive_racer_stop`: the racer stops; the real folder is back. Answers `swaps`.
pub(super) fn racer_stop(args: &Args) -> Answer {
    let device = with_harness(|h| Ok(h.device(args)?.name.clone()))?;
    let racer = with_a(|a| a.racers.remove(&device))?.ok_or("no racer runs for the device")?;
    Ok(serde_json::json!({ "swaps": racer.stop() }))
}

/// `drive_many` with `flat`: `count` files straight in `dir` (no subfolders of 500), named
/// `f000000.txt` on; content as `drive_many`'s.
pub(super) fn many_flat(args: &Args) -> Answer {
    let dir = with_harness(|h| h.path(args, "dir"))?;
    let count = args
        .get("count")
        .and_then(serde_json::Value::as_u64)
        .ok_or("'count' is missing")?;
    let size = args.get("size").and_then(serde_json::Value::as_u64).unwrap_or(64);
    let seed = args.get("seed").and_then(serde_json::Value::as_u64).unwrap_or(1);
    let prefix = str_arg(args, "text_prefix");
    fs::create_dir_all(&dir).map_err(|e| io(e, &dir))?;
    for i in 0..count {
        let bytes = match prefix {
            Some(p) => format!("{p} {i}").into_bytes(),
            None => content_of(&Args::from_iter([
                (String::from("size"), size.into()),
                (String::from("seed"), seed.wrapping_add(i).into()),
            ]))?,
        };
        let path = dir.join(format!("f{i:06}.txt"));
        fs::write(&path, bytes).map_err(|e| io(e, &path))?;
    }
    Ok(serde_json::json!({ "files": count }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_in_the_middle_cut_nothing_and_a_truncate_makes_a_hole() {
        let dir = azul_storage::testing::TempDir::new("azdrive-e2e-write-at");
        let path = dir.path().join("explorer.mp4");
        fs::File::create(&path).unwrap().set_len(8).unwrap();
        write_at(&path, 4, b"ab").unwrap();
        write_at(&path, 0, b"z").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"z\0\0\0ab\0\0");
    }

    #[test]
    fn an_alias_is_marked_as_the_finder_marks_one() {
        let info = alias_finder_info();
        assert_eq!(info.len(), 64);
        assert!(info.starts_with("616c69734d414353"), "alis MACS: {info}");
    }

    #[cfg(unix)]
    #[test]
    fn a_racer_swaps_a_folder_for_a_link_and_puts_it_back() {
        let dir = azul_storage::testing::TempDir::new("azdrive-e2e-racer");
        let path = dir.path().join("sub");
        fs::create_dir_all(path.join("d000")).unwrap();
        let aside = path.with_extension("racer-real");
        let stop = Arc::new(AtomicBool::new(true));
        let racer = Racer { stop, thread: None, path: path.clone(), aside: aside.clone() };
        fs::rename(&path, &aside).unwrap();
        link(dir.path(), &path).unwrap();
        racer.stop();
        assert!(path.join("d000").is_dir(), "the real folder is back");
        assert!(!aside.exists());
    }
}
