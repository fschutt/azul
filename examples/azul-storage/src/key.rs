//! Keys: `/`-separated names, `mail/inbox/0001.eml`; a "folder" is a prefix
//! ending in `/`. The checks here keep a key that is a path on disk inside its
//! root, and a downloaded file inside its folder.
//!
//! What one name of a key may be is one table ([`NameProblem`]): each row says what a name must
//! not hold and where that matters - on every system (the name would leave its folder or be no
//! name at all), on Windows (it would name a stream, a drive, a device or another file there),
//! or for a synced name everywhere (no common disk takes it, or it hides in a list).
//! [`check_key`] keeps any drive's keys in shape; [`check_path_key`] adds the rows of the system
//! it runs on, for a key that becomes a path on this computer's disk, and
//! [`check_path_key_on`] asks the same for another system.

use serde::{Deserialize, Serialize};

use crate::DriveError;

fn invalid(key: &str, reason: &'static str) -> Result<(), DriveError> {
    Err(DriveError::InvalidKey {
        key: key.to_string(),
        reason,
    })
}

/// `C:/...`: a Windows drive letter and the root after it, an absolute path there. A bare
/// `C:` or `C:x` is relative to that drive's current folder; it is no absolute path, and the
/// `:` in it is refused where a key becomes a path on Windows ([`NameProblem`]).
fn has_drive_letter(key: &str) -> bool {
    let mut chars = key.chars();
    matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some(c), Some(':'), Some('/')) if c.is_ascii_alphabetic()
    )
}

/// The systems a name may have to live on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum System {
    Windows,
    Macos,
    Linux,
}

impl System {
    /// Every system.
    pub const ALL: [System; 3] = [System::Windows, System::Macos, System::Linux];

    /// The system this code runs on.
    #[must_use]
    pub fn here() -> System {
        if cfg!(windows) {
            System::Windows
        } else if cfg!(any(target_os = "macos", target_os = "ios")) {
            System::Macos
        } else {
            System::Linux
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            System::Windows => "Windows",
            System::Macos => "macOS",
            System::Linux => "Linux",
        }
    }
}

/// The longest name of one file or folder a synced folder takes, in bytes (what every common
/// file system holds).
pub const NAME_MAX_BYTES: usize = 255;

/// What one name of a path must not be: the rows of the one table every key is checked against,
/// in the order they are asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameProblem {
    /// Nothing between two slashes.
    Empty,
    /// `.`: the folder itself.
    Dot,
    /// `..`: the folder above, out of the folder.
    DotDot,
    /// A NUL: where the system ends the name.
    Nul,
    /// A backslash: a separator on Windows, so one name would be two there.
    Backslash,
    /// A `:`: on Windows `C:x` is relative to drive C's current folder (a path built from it
    /// leaves its root) and `a.txt:s` is an alternate data stream of `a.txt`.
    Colon,
    /// `<`, `>`, `"`, `|`, `?`, `*` or `/`: Windows refuses them in a name.
    WindowsCharacter,
    /// A control character below the space: Windows refuses it, and it hides in a list.
    ControlBelowSpace,
    /// DEL or a C1 control character: a disk takes it, but it hides in a list.
    OtherControl,
    /// A Unicode noncharacter: no text may hold one.
    Noncharacter,
    /// A dot or a space at the end: Windows drops it, so two names would be one file there.
    TrailingDotOrSpace,
    /// A device name (`CON`, `AUX`, `NUL`, `COM1`, ...), alone or before a dot: Windows opens
    /// the device instead.
    DeviceName,
    /// Longer than [`NAME_MAX_BYTES`].
    TooLong,
}

impl NameProblem {
    /// Every row, in the order a name is asked: the first that holds is the one said.
    pub const ALL: [NameProblem; 13] = [
        NameProblem::Empty,
        NameProblem::Dot,
        NameProblem::DotDot,
        NameProblem::Nul,
        NameProblem::Backslash,
        NameProblem::Colon,
        NameProblem::WindowsCharacter,
        NameProblem::ControlBelowSpace,
        NameProblem::OtherControl,
        NameProblem::Noncharacter,
        NameProblem::TrailingDotOrSpace,
        NameProblem::DeviceName,
        NameProblem::TooLong,
    ];

    /// Whether `name` (one name of a path, no slash) has this problem.
    #[must_use]
    pub fn holds(self, name: &str) -> bool {
        match self {
            NameProblem::Empty => name.is_empty(),
            NameProblem::Dot => name == ".",
            NameProblem::DotDot => name == "..",
            NameProblem::Nul => name.contains('\0'),
            NameProblem::Backslash => name.contains('\\'),
            NameProblem::Colon => name.contains(':'),
            NameProblem::WindowsCharacter => name
                .chars()
                .any(|c| matches!(c, '<' | '>' | '"' | '|' | '?' | '*' | '/')),
            NameProblem::ControlBelowSpace => name.chars().any(|c| c < ' '),
            NameProblem::OtherControl => name
                .chars()
                .any(|c| c == '\u{7f}' || ('\u{80}'..='\u{9f}').contains(&c)),
            NameProblem::Noncharacter => name.chars().any(|c| {
                let o = u32::from(c);
                (0xFDD0..=0xFDEF).contains(&o) || (o & 0xFFFE) == 0xFFFE
            }),
            NameProblem::TrailingDotOrSpace => name.ends_with('.') || name.ends_with(' '),
            NameProblem::DeviceName => {
                is_windows_device_name(name.split('.').next().unwrap_or(name))
            }
            NameProblem::TooLong => name.len() > NAME_MAX_BYTES,
        }
    }

    /// Whether a disk of `system` cannot take a name with this problem as it is meant: the path
    /// would leave its folder, or name a stream, a drive, a device or another file there.
    #[must_use]
    pub fn refused_on(self, system: System) -> bool {
        match self {
            NameProblem::Empty
            | NameProblem::Dot
            | NameProblem::DotDot
            | NameProblem::Nul
            | NameProblem::Backslash => true,
            NameProblem::Colon
            | NameProblem::WindowsCharacter
            | NameProblem::ControlBelowSpace
            | NameProblem::TrailingDotOrSpace
            | NameProblem::DeviceName => system == System::Windows,
            NameProblem::OtherControl | NameProblem::Noncharacter | NameProblem::TooLong => false,
        }
    }

    /// Whether a synced name may never have it, on any system. The other rows only Windows
    /// refuses: another system keeps such a name and says Windows cannot hold it.
    #[must_use]
    pub fn refused_for_sync(self) -> bool {
        !matches!(
            self,
            NameProblem::Colon
                | NameProblem::WindowsCharacter
                | NameProblem::TrailingDotOrSpace
                | NameProblem::DeviceName
        )
    }

    /// Why, as a drive's refusal says it.
    #[must_use]
    pub fn reason(self) -> &'static str {
        match self {
            NameProblem::Empty => "it has an empty path segment",
            NameProblem::Dot => "it has a \".\" segment",
            NameProblem::DotDot => "it climbs out of its folder (\"..\")",
            NameProblem::Nul => "it contains a NUL character",
            NameProblem::Backslash => "it contains a backslash",
            NameProblem::Colon => "a \":\" in a name means a drive or a stream on Windows",
            NameProblem::WindowsCharacter | NameProblem::ControlBelowSpace => {
                "it has a character Windows does not allow in a name"
            }
            NameProblem::OtherControl => "it has a control character",
            NameProblem::Noncharacter => "it has a Unicode noncharacter",
            NameProblem::TrailingDotOrSpace => {
                "Windows drops a dot or a space at the end of a name"
            }
            NameProblem::DeviceName => "it is a device name on Windows (CON, AUX, NUL, COM1, ...)",
            NameProblem::TooLong => "a name in it is longer than 255 bytes",
        }
    }
}

/// The first row of the table that `name` breaks among those `counts` keeps.
#[must_use]
pub fn name_problem(name: &str, counts: impl Fn(NameProblem) -> bool) -> Option<NameProblem> {
    NameProblem::ALL
        .into_iter()
        .find(|problem| counts(*problem) && problem.holds(name))
}

/// The shape of `key` - not absolute (`/x`, `\\server`, `C:`), no backslash or NUL, no trailing
/// `/`, no empty, `.` or `..` name - then every name of it against the rows `counts` keeps.
fn check_names(key: &str, counts: impl Fn(NameProblem) -> bool) -> Result<(), DriveError> {
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
    let shape =
        |p: NameProblem| matches!(p, NameProblem::Empty | NameProblem::Dot | NameProblem::DotDot);
    for name in key.split('/') {
        if let Some(problem) = name_problem(name, |p| shape(p) || counts(p)) {
            return invalid(key, problem.reason());
        }
    }
    Ok(())
}

/// Checks a key of any drive: its shape only (no `..` or `.` name, not absolute, no backslash
/// or NUL, no empty name, no trailing `/`). A drive's key becomes a path on a disk only through
/// [`check_path_key`].
pub fn check_key(key: &str) -> Result<(), DriveError> {
    check_names(key, |_| false)
}

/// Checks a key that becomes a path on this computer's disk: [`check_key`], and every name of
/// it one this system takes as it is meant ([`NameProblem::refused_on`]) - on Windows no `:`,
/// device name, trailing dot or space, or character Windows refuses.
pub fn check_path_key(key: &str) -> Result<(), DriveError> {
    check_path_key_on(key, System::here())
}

/// [`check_path_key`] for a disk of `system`.
pub fn check_path_key_on(key: &str, system: System) -> Result<(), DriveError> {
    check_names(key, |problem| problem.refused_on(system))
}

/// `check` of a listing prefix: empty (the root), a folder (`mail/`), or a folder and the start
/// of a name (`mail/in`).
fn check_prefix_with(
    prefix: &str,
    check: fn(&str) -> Result<(), DriveError>,
) -> Result<(), DriveError> {
    if prefix.is_empty() {
        return Ok(());
    }
    let trimmed = prefix.strip_suffix('/').unwrap_or(prefix);
    if trimmed.is_empty() {
        return invalid(prefix, "it is an absolute path");
    }
    check(trimmed).map_err(|e| match e {
        DriveError::InvalidKey { reason, .. } => DriveError::InvalidKey {
            key: prefix.to_string(),
            reason,
        },
        other => other,
    })
}

/// Checks a listing prefix of any drive as [`check_key`] checks a key.
pub fn check_prefix(prefix: &str) -> Result<(), DriveError> {
    check_prefix_with(prefix, check_key)
}

/// Checks a listing prefix of a drive whose keys are paths on this computer's disk, as
/// [`check_path_key`] checks a key.
pub fn check_path_prefix(prefix: &str) -> Result<(), DriveError> {
    check_prefix_with(prefix, check_path_key)
}

/// Why Windows cannot take `name` as one name of a path on disk, or `None`: the first row of the
/// table Windows refuses ([`NameProblem::refused_on`]).
#[must_use]
pub fn windows_name_problem(name: &str) -> Option<&'static str> {
    name_problem(name, |problem| problem.refused_on(System::Windows)).map(NameProblem::reason)
}

/// Whether `stem` (a name up to its first dot) is a name Windows keeps for a device, in any
/// case and with spaces after it: `CON`, `PRN`, `AUX`, `NUL`, `CONIN$`, `CONOUT$`, and `COM` or
/// `LPT` with one digit (0 to 9, or a superscript 1, 2 or 3).
#[must_use]
pub fn is_windows_device_name(stem: &str) -> bool {
    let upper = stem.trim_end_matches(' ').to_uppercase();
    matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ["COM", "LPT"].into_iter().any(|p| {
        upper.strip_prefix(p).is_some_and(|n| {
            let mut c = n.chars();
            matches!(
                (c.next(), c.next()),
                (Some('0'..='9' | '\u{b9}' | '\u{b2}' | '\u{b3}'), None)
            )
        })
    })
}

/// Checks the names of a key or a folder prefix that becomes a path on this computer's disk:
/// each one a name this system takes ([`NameProblem::refused_on`]). [`check_path_key`] and
/// [`check_path_prefix`] ask it already.
///
/// # Errors
///
/// [`DriveError::InvalidKey`] naming why.
pub fn check_local_names(key: &str) -> Result<(), DriveError> {
    let here = System::here();
    for name in key.split('/').filter(|n| !n.is_empty()) {
        if let Some(problem) = name_problem(name, |p| p.refused_on(here)) {
            return invalid(key, problem.reason());
        }
    }
    Ok(())
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
