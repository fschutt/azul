//! Keys: `/`-separated names, `mail/inbox/0001.eml`; a "folder" is a prefix
//! ending in `/`. The checks here keep a key that is a path on disk inside its
//! root, and a downloaded file inside its folder.

use crate::DriveError;

fn invalid(key: &str, reason: &'static str) -> Result<(), DriveError> {
    Err(DriveError::InvalidKey {
        key: key.to_string(),
        reason,
    })
}

/// `C:...`: a Windows drive letter, an absolute path there.
fn has_drive_letter(key: &str) -> bool {
    let mut chars = key.chars();
    matches!((chars.next(), chars.next()), (Some(c), Some(':')) if c.is_ascii_alphabetic())
}

/// Checks a key of a drive whose keys are paths on disk: no `..` or `.`
/// segment, not absolute (`/x`, `\\server`, `C:`), no backslash or NUL, no
/// empty segment, no trailing `/`.
pub fn check_path_key(key: &str) -> Result<(), DriveError> {
    if key.is_empty() {
        return invalid(key, "it is empty");
    }
    if key.contains('\0') {
        return invalid(key, "it contains a NUL character");
    }
    if key.contains('\\') {
        return invalid(key, "it contains a backslash");
    }
    if key.starts_with('/') || has_drive_letter(key) {
        return invalid(key, "it is an absolute path");
    }
    if key.ends_with('/') {
        return invalid(key, "it names a folder, not a file");
    }
    for segment in key.split('/') {
        match segment {
            "" => return invalid(key, "it has an empty path segment"),
            "." => return invalid(key, "it has a \".\" segment"),
            ".." => return invalid(key, "it climbs out of its folder (\"..\")"),
            _ => {}
        }
    }
    Ok(())
}

/// Checks a listing prefix of such a drive: empty (the root), a folder
/// (`mail/`), or a folder and the start of a name (`mail/in`).
pub fn check_path_prefix(prefix: &str) -> Result<(), DriveError> {
    if prefix.is_empty() {
        return Ok(());
    }
    let trimmed = prefix.strip_suffix('/').unwrap_or(prefix);
    if trimmed.is_empty() {
        return invalid(prefix, "it is an absolute path");
    }
    check_path_key(trimmed).map_err(|e| match e {
        DriveError::InvalidKey { reason, .. } => DriveError::InvalidKey {
            key: prefix.to_string(),
            reason,
        },
        other => other,
    })
}

/// The folder part of a prefix: `mail/in` -> `mail/`, `mail/` -> `mail/`, `in` -> ``.
#[must_use]
pub fn folder_of(prefix: &str) -> &str {
    match prefix.rfind('/') {
        Some(i) => &prefix[..=i],
        None => "",
    }
}

/// The folder above a folder prefix: `mail/inbox/` -> `mail/`, `mail/` -> ``.
#[must_use]
pub fn parent_prefix(prefix: &str) -> String {
    let trimmed = prefix.strip_suffix('/').unwrap_or(prefix);
    folder_of(trimmed).to_string()
}

/// The last segment of a key or folder: `mail/inbox/0001.eml` -> `0001.eml`,
/// `mail/inbox/` -> `inbox`.
#[must_use]
pub fn last_segment(key: &str) -> &str {
    let trimmed = key.strip_suffix('/').unwrap_or(key);
    match trimmed.rfind('/') {
        Some(i) => &trimmed[i + 1..],
        None => trimmed,
    }
}

/// Every folder from the root down to `prefix`, as `(name, prefix)`:
/// `mail/inbox/` -> `[("mail", "mail/"), ("inbox", "mail/inbox/")]`.
#[must_use]
pub fn folder_trail(prefix: &str) -> Vec<(String, String)> {
    let mut trail = Vec::new();
    let Some(end) = prefix.rfind('/') else {
        return trail;
    };
    let mut so_far = String::new();
    for segment in prefix[..end].split('/') {
        if segment.is_empty() {
            continue;
        }
        so_far.push_str(segment);
        so_far.push('/');
        trail.push((segment.to_string(), so_far.clone()));
    }
    trail
}

/// The local file name for a download of `key`: its last segment with path
/// separators, control characters and characters Windows refuses replaced by
/// `_`. `None` when nothing usable is left (`..`, `.`, empty).
#[must_use]
pub fn safe_file_name(key: &str) -> Option<String> {
    let name = last_segment(key);
    if name.is_empty() || name == "." || name == ".." {
        return None;
    }
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    if cleaned.trim().is_empty() {
        None
    } else {
        Some(cleaned)
    }
}
