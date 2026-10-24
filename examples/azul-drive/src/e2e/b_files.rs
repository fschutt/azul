//! Group B's steps on a device's folder and disk: a folder the scan cannot list for a while
//! (`drive_local_fault`), extended attributes and BSD flags (`drive_xattr`, `drive_chflags`).

use std::path::Path;

use azcloud_kit::sync::faults;

use super::{need, str_arg, with_harness, Answer, Args};

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
    fn a_folder_is_named_by_its_key_whether_relative_or_absolute() {
        let root = Path::new("/sandbox/devices/a/root");
        assert_eq!(folder_key(root, "photos/").unwrap(), "photos");
        assert_eq!(folder_key(root, "/sandbox/devices/a/root/photos/2026").unwrap(), "photos/2026");
        assert_eq!(folder_key(root, "").unwrap(), "");
        assert!(folder_key(root, "/elsewhere/photos").is_err());
    }
}
