//! Group B's steps on a device's folder and disk: a folder the scan cannot list for a while
//! (`drive_local_fault`), extended attributes and BSD flags (`drive_xattr`, `drive_chflags`).

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, PoisonError},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use azcloud_kit::sync::{attrs, faults};

use super::{files::content_of, need, str_arg, with_harness, Answer, Args};

/// `drive_xattr`: sets the extended attribute `name` of `path` (a file or folder of the device)
/// to the step's content (`text`, `hex`, `size` + `seed`, as `drive_write` takes it).
pub(crate) fn xattr(args: &Args) -> Answer {
    with_harness(|h| {
        let path = h.path(args, "path")?;
        let name = need(args, "name")?;
        let value = content_of(args)?;
        attrs::set_attribute(&path, name, &value)
            .map_err(|e| format!("the attribute {name} of {}: {e}", path.display()))?;
        Ok(serde_json::json!({ "bytes": value.len() }))
    })
}

/// `drive_chflags`: the BSD flags of `path` as `chflags` sets them (`uchg`, `uappnd`,
/// `hidden`, `nodump`, ...; `no` before a name clears it; `""` clears them all). macOS only.
pub(crate) fn chflags(args: &Args) -> Answer {
    with_harness(|h| {
        let path = h.path(args, "path")?;
        let spec = str_arg(args, "flags").ok_or("'flags' is missing")?;
        let now = attrs::flags(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let next = attrs::apply_flags(now, spec)?;
        attrs::set_flags(&path, next).map_err(|e| format!("chflags {}: {e}", path.display()))?;
        Ok(serde_json::json!({ "flags": next }))
    })
}

/// Clears the BSD flags of everything under `top` (an immutable file cannot be removed), before
/// a sandbox is removed.
pub(crate) fn clear_flags(top: &Path) {
    fn walk(dir: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if attrs::flags(&path).is_ok_and(|f| f != 0) {
                let _ = attrs::set_flags(&path, 0);
            }
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                walk(&path);
            }
        }
    }
    walk(top);
}

/// Files each E2E device's scans hashed since it was opened or restarted, by drive id.
static HASHED: Mutex<BTreeMap<String, u64>> = Mutex::new(BTreeMap::new());

fn hashed() -> MutexGuard<'static, BTreeMap<String, u64>> {
    HASHED.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A pass of `drive_id` hashed `n` files.
pub(crate) fn note_hashed(drive_id: &str, n: u64) {
    *hashed().entry(drive_id.to_string()).or_default() += n;
}

/// `drive_id` was opened or restarted: it has hashed nothing yet.
pub(crate) fn reset_hashed(drive_id: &str) {
    hashed().remove(drive_id);
}

/// The files `drive_id`'s passes hashed since it was opened or restarted (`drive_counters`'
/// `hashed`).
pub(crate) fn hashed_of(drive_id: &str) -> u64 {
    hashed().get(drive_id).copied().unwrap_or(0)
}

/// The time `t` in nanoseconds since 1970, negative before it.
pub(crate) fn signed_ns(t: SystemTime) -> i64 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_nanos()).unwrap_or(i64::MAX),
        Err(before) => i64::try_from(before.duration().as_nanos()).map_or(i64::MIN, |ns| -ns),
    }
}

/// The time `ns` nanoseconds after 1970 (before it when negative).
pub(crate) fn time_of_ns(ns: i64) -> SystemTime {
    let span = Duration::from_nanos(ns.unsigned_abs());
    if ns < 0 {
        UNIX_EPOCH - span
    } else {
        UNIX_EPOCH + span
    }
}

/// Sets the modification time of `path` to `ns` nanoseconds after 1970 (negative: before).
pub(crate) fn set_mtime_signed(path: &Path, ns: i64) -> std::io::Result<()> {
    std::fs::File::options()
        .write(true)
        .open(path)
        .and_then(|f| f.set_modified(time_of_ns(ns)))
}

/// The path of a name of raw bytes (`hex`, a relative path with `/` between its folders) in
/// the folder `root`: a name that is no UTF-8 exists on Linux and the BSDs, never in a JSON
/// string.
pub(crate) fn raw_path(root: &Path, hex: &str) -> Result<PathBuf, String> {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| hex.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok()))
        .collect::<Option<_>>()
        .ok_or_else(|| format!("path_hex '{hex}' is not hex"))?;
    if bytes.is_empty() || bytes.contains(&0) || bytes.first() == Some(&b'/') {
        return Err(String::from("path_hex names no relative path (empty, a NUL, or absolute)"));
    }
    raw_name(root, &bytes)
}

#[cfg(unix)]
fn raw_name(root: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    use std::os::unix::ffi::OsStrExt;
    Ok(root.join(std::ffi::OsStr::from_bytes(bytes)))
}

#[cfg(not(unix))]
fn raw_name(root: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    String::from_utf8(bytes.to_vec())
        .map(|name| root.join(name))
        .map_err(|_| String::from("this system's names are Unicode: a name of raw bytes cannot be made"))
}

/// `drive_local_fault`: the scan of the device's folder meets `readdir_error` (`EIO`,
/// `ENOTDIR`, `EACCES` or an error number) when it lists `path` (a folder of the device, `""`
/// or absent: its top), for the next `passes` passes (default 1; 0 ends it).
pub(crate) fn local_fault(args: &Args) -> Answer {
    with_harness(|h| {
        let d = h.device(args)?;
        let folder = folder_key(&d.root, str_arg(args, "path").unwrap_or(""))?;
        let name = need(args, "readdir_error")?;
        let errno = faults::errno_of(name)
            .ok_or_else(|| format!("readdir_error '{name}' is none of EIO, ENOTDIR, EACCES or a number"))?;
        let passes = args
            .get("passes")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1);
        let passes = u32::try_from(passes).map_err(|_| String::from("'passes' is too big"))?;
        faults::set_readdir_error(&d.pair_folder, &folder, errno, passes);
        Ok(serde_json::json!({ "folder": folder, "errno": errno, "passes": passes }))
    })
}

/// The key of the folder `path` names in the folder `root` (relative, or absolute under it).
fn folder_key(root: &Path, path: &str) -> Result<String, String> {
    let rel = match Path::new(path).strip_prefix(root) {
        Ok(rel) => rel.to_string_lossy().into_owned(),
        Err(_) if Path::new(path).is_absolute() => {
            return Err(format!("{path} is not in the device's folder {}", root.display()))
        }
        Err(_) => path.to_string(),
    };
    Ok(rel.replace('\\', "/").trim_matches('/').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hashed_count_adds_up_per_device_and_starts_again_at_a_restart() {
        reset_hashed("e2e-hash-a");
        note_hashed("e2e-hash-a", 2000);
        note_hashed("e2e-hash-a", 1);
        note_hashed("e2e-hash-b", 5);
        assert_eq!(hashed_of("e2e-hash-a"), 2001);
        reset_hashed("e2e-hash-a");
        note_hashed("e2e-hash-a", 1);
        assert_eq!(hashed_of("e2e-hash-a"), 1);
        assert_eq!(hashed_of("e2e-hash-b"), 5);
    }

    #[test]
    fn a_time_before_1970_is_a_negative_number_of_nanoseconds_both_ways() {
        let ns = -31_536_000_000_000_000_i64;
        assert_eq!(signed_ns(time_of_ns(ns)), ns);
        assert_eq!(signed_ns(time_of_ns(5)), 5);
        assert_eq!(signed_ns(UNIX_EPOCH), 0);
    }

    #[test]
    fn a_file_written_with_a_time_before_1970_keeps_it() {
        let dir = azul_storage::testing::TempDir::new("azdrive-b-old-mtime");
        let path = dir.path().join("IMG_1969.JPG");
        std::fs::write(&path, b"old").unwrap();
        let ns = -31_536_000_000_000_000_i64;
        set_mtime_signed(&path, ns).expect("a time before 1970 can be set");
        let back = signed_ns(std::fs::metadata(&path).unwrap().modified().unwrap());
        assert!((back - ns).abs() < 2_000_000_000, "{back} is {ns}");
    }

    #[cfg(unix)]
    #[test]
    fn a_raw_name_keeps_its_bytes() {
        use std::os::unix::ffi::OsStrExt;
        let root = Path::new("/sandbox/root");
        let path = raw_path(root, "696e76616c6964ff2e747874").unwrap();
        assert_eq!(path.file_name().unwrap().as_bytes(), b"invalid\xff.txt");
        assert!(raw_path(root, "2f6574632f706173737764").is_err(), "absolute");
        assert!(raw_path(root, "zz").is_err());
    }

    #[test]
    fn chflags_names_set_and_clear_flags_as_the_command_does() {
        let uchg = attrs::apply_flags(0, "uchg").unwrap();
        assert_eq!(uchg, 0x2);
        assert_eq!(attrs::apply_flags(uchg, "nouchg").unwrap(), 0);
        assert_eq!(attrs::apply_flags(0, "hidden,nodump").unwrap(), 0x8001);
        assert_eq!(attrs::apply_flags(0x8003, "").unwrap(), 0);
        assert!(attrs::apply_flags(0, "sticky").is_err());
    }

    #[test]
    fn a_folder_is_named_by_its_key_whether_relative_or_absolute() {
        let root = Path::new("/sandbox/devices/a/root");
        assert_eq!(folder_key(root, "photos/").unwrap(), "photos");
        assert_eq!(folder_key(root, "/sandbox/devices/a/root/photos/2026").unwrap(), "photos/2026");
        assert_eq!(folder_key(root, "").unwrap(), "");
        assert!(folder_key(root, "/elsewhere/photos").is_err());
    }
}
