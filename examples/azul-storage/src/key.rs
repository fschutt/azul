//! Keys: `/`-separated names, `mail/inbox/0001.eml`; a "folder" is a prefix
//! ending in `/`. The checks here keep a key that is a path on disk inside its
//! root, and a downloaded file inside its folder.

use crate::DriveError;

/// Checks a key of a drive whose keys are paths on disk: no `..` or `.`
/// segment, not absolute (`/x`, `\\server`, `C:`), no backslash or NUL, no
/// empty segment, no trailing `/`.
pub fn check_path_key(key: &str) -> Result<(), DriveError> {
    let _ = key;
    todo!("RED")
}

/// Checks a listing prefix of such a drive: empty (the root), a folder
/// (`mail/`), or a folder and the start of a name (`mail/in`).
pub fn check_path_prefix(prefix: &str) -> Result<(), DriveError> {
    let _ = prefix;
    todo!("RED")
}

/// The folder above a folder prefix: `mail/inbox/` -> `mail/`, `mail/` -> ``.
#[must_use]
pub fn parent_prefix(prefix: &str) -> String {
    let _ = prefix;
    todo!("RED")
}

/// The last segment of a key or folder: `mail/inbox/0001.eml` -> `0001.eml`,
/// `mail/inbox/` -> `inbox`.
#[must_use]
pub fn last_segment(key: &str) -> &str {
    let _ = key;
    todo!("RED")
}

/// Every folder from the root down to `prefix`, as `(name, prefix)`:
/// `mail/inbox/` -> `[("mail", "mail/"), ("inbox", "mail/inbox/")]`.
#[must_use]
pub fn folder_trail(prefix: &str) -> Vec<(String, String)> {
    let _ = prefix;
    todo!("RED")
}

/// The local file name for a download of `key`: its last segment with path
/// separators, control characters and characters Windows refuses replaced by
/// `_`. `None` when nothing usable is left (`..`, `.`, empty).
#[must_use]
pub fn safe_file_name(key: &str) -> Option<String> {
    let _ = key;
    todo!("RED")
}
