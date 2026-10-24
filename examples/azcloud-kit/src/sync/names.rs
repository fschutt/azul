//! The name policy of a synced folder: which names from the drive may become files here, which
//! are valid here but not on another system, and which would mislead a person reading them.
//!
//! A name on the drive comes from another device or from someone the folder is shared with, so
//! it is untrusted input. A name that would leave the folder, or that this system or this disk
//! cannot hold, never becomes a file: the run lists it with its reason and leaves the drive's
//! file as it is. Nothing is renamed silently, because a renamed file is a different file to
//! every other device. The policy is the same on every system; only [`Volume`] (what the disk
//! under the folder refuses or folds, probed when the folder is paired) differs.

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

/// The longest name of one file or folder, in bytes (every common file system).
pub const COMPONENT_MAX_BYTES: usize = 255;
/// The longest path, in bytes (Linux's PATH_MAX).
pub const PATH_MAX_BYTES: usize = 4096;
/// Windows' MAX_PATH without the long-path opt-in, in UTF-16 units, less what a typical folder
/// (`C:\Users\<name>\AzDrive\`) takes before the key starts.
const WINDOWS_KEY_MAX_UNITS: usize = 259 - 40;
/// The engine's own bookkeeping at the root of a synced folder (the root marker, the trash):
/// no name from the drive may point into it.
pub const BOOKKEEPING: &str = ".azlin";

/// The systems a synced folder lives on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum System {
    Windows,
    Macos,
    Linux,
}

impl System {
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

/// What the disk under a synced folder does with names, probed once when it is paired: a disk
/// that ignores case (APFS, NTFS) or Unicode normalization (APFS, HFS+) takes two names of the
/// drive for one file, and FAT and exFAT refuse the characters Windows refuses, on every system.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Volume {
    /// `Report.txt` and `report.txt` are one file here.
    pub case_insensitive: bool,
    /// `café` precomposed (NFC) and decomposed (NFD) are one file here.
    pub norm_insensitive: bool,
    /// The disk refuses (or quietly changes) the names Windows refuses: `:`, `?`, a trailing dot.
    pub windows_names: bool,
    /// The largest file the disk holds, when it is small enough to matter (FAT32's 4 GiB - 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_file: Option<u64>,
}

fn is_control(c: char) -> bool {
    let o = u32::from(c);
    o < 0x20 || o == 0x7F || (0x80..=0x9F).contains(&o)
}

fn is_noncharacter(c: char) -> bool {
    let o = u32::from(c);
    (0xFDD0..=0xFDEF).contains(&o) || (o & 0xFFFE) == 0xFFFE
}

/// Why one name of a path can be no file name on any system, or `None`.
#[must_use]
pub fn segment_problem(segment: &str) -> Option<String> {
    if segment.is_empty() {
        return Some(String::from("an empty name in its path"));
    }
    if segment == "." || segment == ".." {
        return Some(format!(
            "a \"{segment}\" in its path climbs out of its folder"
        ));
    }
    if segment.contains('\0') {
        return Some(String::from("a NUL character"));
    }
    if segment.contains('\\') {
        return Some(String::from("a backslash (a path separator on Windows)"));
    }
    if segment.chars().any(is_control) {
        return Some(String::from("a control character"));
    }
    if segment.chars().any(is_noncharacter) {
        return Some(String::from("a Unicode noncharacter"));
    }
    if segment.len() > COMPONENT_MAX_BYTES {
        return Some(format!("a name longer than {COMPONENT_MAX_BYTES} bytes"));
    }
    None
}

/// Why a key of the drive can never become a file under the folder, on any system, or `None`.
/// The whole key is checked first (absolute, a drive letter, UNC), since one name of it cannot
/// show those, then every name of it.
#[must_use]
pub fn key_problem(key: &str) -> Option<String> {
    if key.is_empty() {
        return Some(String::from("an empty name"));
    }
    if key.starts_with('/') {
        return Some(String::from("an absolute path"));
    }
    if key.starts_with("\\\\") || key.starts_with("//") {
        return Some(String::from("a network (UNC) path"));
    }
    let mut chars = key.chars();
    if matches!((chars.next(), chars.next()), (Some(c), Some(':')) if c.is_ascii_alphabetic()) {
        return Some(String::from("a Windows drive path"));
    }
    if key.ends_with('/') {
        return Some(String::from("a folder name, not a file"));
    }
    if key.len() > PATH_MAX_BYTES {
        return Some(format!("a path longer than {PATH_MAX_BYTES} bytes"));
    }
    if key.split('/').next() == Some(BOOKKEEPING) {
        return Some(String::from(
            "the folder's own bookkeeping (.azlin at its root)",
        ));
    }
    key.split('/').find_map(segment_problem)
}

/// The names Windows reserves for devices, as a whole name and as the part before the first
/// dot (`nul.tar.gz`), in any case.
fn is_windows_device(stem: &str) -> bool {
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

/// Why Windows (or a FAT or exFAT disk) cannot hold this otherwise valid key, or `None`. A `:`
/// is refused outright: on NTFS `a.txt:x` writes an alternate data stream of `a.txt`, and
/// `dir/C:x` makes a path relative to drive C's current folder.
#[must_use]
pub fn windows_problem(key: &str) -> Option<String> {
    const REFUSED: &[char] = &['<', '>', ':', '"', '|', '?', '*', '\\'];
    for segment in key.split('/') {
        let mut bad: Vec<char> = segment.chars().filter(|c| REFUSED.contains(c)).collect();
        if !bad.is_empty() {
            bad.sort_unstable();
            bad.dedup();
            let bad: String = bad.into_iter().collect();
            return Some(format!("\"{bad}\" is not allowed in a Windows name"));
        }
        if segment.ends_with('.') || segment.ends_with(' ') {
            return Some(String::from("a name ending in a dot or a space"));
        }
        let stem = segment.split('.').next().unwrap_or(segment);
        if is_windows_device(stem) {
            return Some(format!(
                "{} is a reserved device name on Windows",
                stem.trim_end_matches(' ').to_uppercase()
            ));
        }
        if segment.encode_utf16().count() > COMPONENT_MAX_BYTES {
            return Some(String::from("a name longer than 255 UTF-16 units"));
        }
    }
    if key.encode_utf16().count() > WINDOWS_KEY_MAX_UNITS {
        return Some(String::from(
            "a path too long for Windows without long-path support",
        ));
    }
    None
}

/// Why `system` cannot hold the key, or `None`.
#[must_use]
pub fn system_problem(key: &str, system: System) -> Option<String> {
    match system {
        System::Windows => windows_problem(key),
        System::Macos | System::Linux => None,
    }
}

/// Why this system and this disk cannot hold the key, or `None`: Windows' rules on Windows, and
/// on a disk that refuses what Windows refuses (FAT, exFAT) on any system.
#[must_use]
pub fn here_problem(key: &str, system: System, volume: &Volume) -> Option<String> {
    system_problem(key, system).or_else(|| {
        if volume.windows_names {
            windows_problem(key).map(|why| format!("{why} (this disk refuses it too)"))
        } else {
            None
        }
    })
}

/// The other systems that cannot hold a key this system can: a warning, so the user sees early
/// that a Windows device will not get the file.
#[must_use]
pub fn portability(key: &str, here: System) -> Vec<(System, String)> {
    [System::Windows, System::Macos, System::Linux]
        .into_iter()
        .filter(|system| *system != here)
        .filter_map(|system| system_problem(key, system).map(|why| (system, why)))
        .collect()
}

fn is_bidi(c: char) -> bool {
    matches!(u32::from(c), 0x061C | 0x200E | 0x200F | 0x202A..=0x202E | 0x2066..=0x2069)
}

fn is_invisible(c: char) -> bool {
    matches!(
        u32::from(c),
        0x00AD | 0x034F | 0x180E | 0x200B | 0x200C | 0x200D | 0x2060 | 0xFEFF
    )
}

/// Characters that look like a dot or a slash: `invoice\u{2024}pdf` reads as `invoice.pdf`.
fn is_lookalike(c: char) -> bool {
    matches!(
        u32::from(c),
        0x2024 | 0x2215 | 0x2044 | 0xFF0F | 0x29F8 | 0xFF0E | 0x3002
    )
}

/// The script of a letter, for the scripts look-alike letters come from.
fn script_of(c: char) -> Option<&'static str> {
    let o = u32::from(c);
    if !c.is_alphabetic() {
        return None;
    }
    match o {
        0x41..=0x5A | 0x61..=0x7A | 0xC0..=0x24F | 0x1E00..=0x1EFF => Some("latin"),
        0x370..=0x3FF | 0x1F00..=0x1FFF => Some("greek"),
        0x400..=0x52F => Some("cyrillic"),
        0x530..=0x58F => Some("armenian"),
        0x13A0..=0x13FF => Some("cherokee"),
        _ => None,
    }
}

/// Ways a name can mislead a person: a direction override that reverses the shown extension,
/// invisible characters, look-alike dots and slashes, letters of two scripts in one word,
/// leading or trailing spaces.
#[must_use]
pub fn display_risks(key: &str) -> Vec<String> {
    let mut risks = Vec::new();
    if key.chars().any(is_bidi) {
        risks.push(String::from(
            "a text-direction control character (the shown extension may be false)",
        ));
    }
    if key.chars().any(is_invisible) {
        risks.push(String::from("an invisible character"));
    }
    if key.chars().any(is_lookalike) {
        risks.push(String::from("a character that looks like a dot or a slash"));
    }
    for segment in key.split('/') {
        // Per word: `Москва.txt` is a Russian name with a Latin extension, `pаypal` (a Cyrillic
        // a) is a look-alike.
        let mixed = segment
            .split(|c: char| !c.is_alphabetic())
            .find_map(|word| {
                let mut scripts: Vec<&str> = word.chars().filter_map(script_of).collect();
                scripts.sort_unstable();
                scripts.dedup();
                (scripts.len() > 1).then(|| scripts.join(" and "))
            });
        if let Some(scripts) = mixed {
            risks.push(format!("letters of {scripts} in one word"));
            break;
        }
        if segment != segment.trim() {
            risks.push(String::from("leading or trailing spaces"));
            break;
        }
    }
    risks
}

/// The name as a person should see it in the problem list: every risky character written as its
/// code point, so a reversed extension cannot hide (`invoice<U+202E>fdp.exe`).
#[must_use]
pub fn escaped(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    for c in key.chars() {
        if is_bidi(c) || is_invisible(c) || is_lookalike(c) || is_control(c) {
            out.push_str(&format!("<U+{:04X}>", u32::from(c)));
        } else {
            out.push(c);
        }
    }
    out
}

/// The key in NFC.
#[must_use]
pub fn nfc(key: &str) -> String {
    key.nfc().collect()
}

/// The key as the disk compares names: NFC on a disk that ignores normalization, folded to
/// lower case on one that ignores case. Two keys with the same fold are one file here.
#[must_use]
pub fn fold(key: &str, volume: &Volume) -> String {
    let mut out = if volume.norm_insensitive {
        nfc(key)
    } else {
        key.to_string()
    };
    if volume.case_insensitive {
        out = out.to_lowercase();
        if volume.norm_insensitive {
            out = nfc(&out);
        }
    }
    out
}
