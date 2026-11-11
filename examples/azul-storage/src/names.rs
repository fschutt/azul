//! Names: what a name from a drive, another device, a user or a listing may be before it becomes
//! a path on this computer (ARCH28 RF42 / AR5a-3).
//!
//! The name policy of a synced folder: which names from the drive may become files here, which
//! are valid here but not on another system, and which would mislead a person reading them.
//!
//! A name on the drive comes from another device or from someone the folder is shared with, so
//! it is untrusted input. A name that would leave the folder, or that this system or this disk
//! cannot hold, never becomes a file: the run lists it with its reason and leaves the drive's
//! file as it is. Nothing is renamed silently, because a renamed file is a different file to
//! every other device. The policy is the same on every system; only [`Volume`] (what the disk
//! under the folder refuses or folds, probed when the folder is paired) differs. What one name
//! may be is the storage crate's one table ([`NameProblem`]): the sync refuses its rows that no
//! system should hold, and Windows (or a disk that refuses what Windows refuses) the rows
//! Windows misreads - the same rows every path check on Windows refuses.

use std::{fmt, ops::Deref};

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

pub use crate::key::System;
use crate::{
    beneath::{BeneathError, Dir, Root},
    key::{self as storage_key, NameProblem},
};

/// The longest name of one file or folder, in bytes (every common file system).
pub const COMPONENT_MAX_BYTES: usize = storage_key::NAME_MAX_BYTES;
/// The longest path, in bytes (Linux's PATH_MAX).
pub const PATH_MAX_BYTES: usize = 4096;
/// Windows' MAX_PATH without the long-path opt-in, in UTF-16 units, less what a typical folder
/// (`C:\Users\<name>\AzDrive\`) takes before the key starts.
const WINDOWS_KEY_MAX_UNITS: usize = 259 - 40;
/// The engine's own bookkeeping at the root of a synced folder (the root marker, the trash):
/// no name from the drive may point into it.
pub const BOOKKEEPING: &str = ".azlin";
/// The version-control folders, at any depth: a repository syncs through git, never file by
/// file, and what a name from the drive writes into `.git/hooks` runs as the user at the next
/// `git commit`.
pub const VCS_FOLDERS: [&str; 3] = [".git", ".hg", ".svn"];
/// The bookkeeping folder of a synced folder, from its root: the root marker, the trash and the
/// spool live in it.
pub const SYNC_DIR: &str = ".azlin/sync";
/// Where a delete here moves the file, in it.
pub const TRASH_DIR: &str = ".azlin/sync/trash";
/// Where big downloads are spooled on the folder's own disk, in it.
pub const SPOOL_DIR: &str = ".azlin/sync/tmp";
/// The sync's record of a folder synced under a drive's prefix, in the drive: a file of the drive
/// (sealed, kept in its history) that no listing shows.
pub const RECORD_FILE: &str = ".azlin/sync/record.json";

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
    /// The disk keeps no permission bits (FAT, exFAT): every file shows the same mode, so a
    /// file's executable bit here says nothing and its synced version's bit stands.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_exec_bits: bool,
}

fn is_control(c: char) -> bool {
    let o = u32::from(c);
    o < 0x20 || o == 0x7F || (0x80..=0x9F).contains(&o)
}

/// Why one name of a path can be no file name on any system, or `None`: the table's rows a
/// synced name may never break.
#[must_use]
pub fn segment_problem(segment: &str) -> Option<String> {
    let problem = storage_key::name_problem(segment, NameProblem::refused_for_sync)?;
    Some(match problem {
        NameProblem::Empty => String::from("an empty name in its path"),
        NameProblem::Dot | NameProblem::DotDot => {
            format!("a \"{segment}\" in its path climbs out of its folder")
        }
        NameProblem::Nul => String::from("a NUL character"),
        NameProblem::Backslash => String::from("a backslash (a path separator on Windows)"),
        NameProblem::ControlBelowSpace | NameProblem::OtherControl => {
            String::from("a control character")
        }
        NameProblem::Noncharacter => String::from("a Unicode noncharacter"),
        NameProblem::TooLong => format!("a name longer than {COMPONENT_MAX_BYTES} bytes"),
        other => String::from(other.reason()),
    })
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
    if let Some(why) = protected_problem(key) {
        return Some(why);
    }
    key.split('/').find_map(segment_problem)
}

/// Why a name of the drive must not point into one of the folder's protected places, or `None`:
/// the engine's bookkeeping at the root (`.azlin`) and a version-control name at any depth.
/// Compared by [`Protected`]: without regard to case, Unicode form, the code points a disk
/// ignores, trailing dots and spaces, a stream suffix or an NTFS short name (`GIT~1`), on EVERY
/// system and disk (AZ-W1, AZ-W7): APFS and NTFS read `.GIT` as `.git`, so the answer cannot wait
/// for a probe of this disk - and a drive is shared between systems, so a name that is harmless
/// here is not on the next device. The kind of the entry does not matter (X-DELTA-AZDRIVE-FILES-1,
/// -9): a folder entry, a file and a link are refused alike, and so is a LAST name - a folder the
/// drive renames to `.git` is a repository here, and so is a file `.git` (git follows its
/// `gitdir:` line) or a link.
fn protected_problem(key: &str) -> Option<String> {
    let (_, kind, name) = Protected::in_key(key)?;
    match kind {
        ProtectedKind::Bookkeeping => Some(String::from(
            "the folder's own bookkeeping (.azlin at its root)",
        )),
        ProtectedKind::Vcs => Some(format!(
            "a version-control name ({name}): a repository syncs through git, and a disk may \
             read this name as its own - whether it is a folder, a file or a link"
        )),
        // Warned of, not refused (the owner's call: hooks and configuration are what matters).
        ProtectedKind::VcsMetadata | ProtectedKind::OsPrivate => None,
    }
}

/// A note for a key with a repository's own configuration in its name (`.gitmodules`,
/// `.gitattributes`, in any spelling): warned of, not refused - git reads such a file where it
/// lies, and a drive's other participants write it (X-DELTA-AZDRIVE-FILES-32).
#[must_use]
pub fn vcs_metadata_note(key: &str) -> Option<String> {
    key.split('/')
        .find(|name| Protected::kind(name) == Some(ProtectedKind::VcsMetadata))
        .map(|name| {
            format!(
                "{name} is a repository's own configuration: git reads it where it lies, and the \
                 drive's other participants write it"
            )
        })
}

/// Why a folder entry of the drive (its key without a slash at the end) can never become a
/// folder here, on any system, or `None`. The rows are [`key_problem`]'s, and the protected names
/// among them hold for every name of the key, the last included: the name policy does not depend
/// on the kind of the entry, so a folder made or renamed here passes the same table a file does
/// (X-DELTA-AZDRIVE-FILES-1).
#[must_use]
pub fn folder_problem(key: &str) -> Option<String> {
    key_problem(key)
}

/// The rows of the name table only Windows refuses: the ones another system keeps and warns of.
fn windows_only(problem: NameProblem) -> bool {
    problem.refused_on(System::Windows) && !problem.refused_for_sync()
}

/// Why Windows (or a FAT or exFAT disk) cannot hold this otherwise valid key, or `None`. A `:`
/// is refused outright: on NTFS `a.txt:x` writes an alternate data stream of `a.txt`, and
/// `dir/C:x` makes a path relative to drive C's current folder.
#[must_use]
pub fn windows_problem(key: &str) -> Option<String> {
    for segment in key.split('/') {
        let Some(problem) = storage_key::name_problem(segment, windows_only) else {
            continue;
        };
        return Some(match problem {
            NameProblem::Colon | NameProblem::WindowsCharacter => {
                // Every character of the name Windows refuses, said once each.
                let refused = |c: &char| {
                    let mut buf = [0u8; 4];
                    let one: &str = c.encode_utf8(&mut buf);
                    NameProblem::Colon.holds(one) || NameProblem::WindowsCharacter.holds(one)
                };
                let mut bad: Vec<char> = segment.chars().filter(refused).collect();
                bad.sort_unstable();
                bad.dedup();
                let bad: String = bad.into_iter().collect();
                format!("\"{bad}\" is not allowed in a Windows name")
            }
            NameProblem::TrailingDotOrSpace => String::from("a name ending in a dot or a space"),
            NameProblem::DeviceName => {
                let stem = segment.split('.').next().unwrap_or(segment);
                format!(
                    "{} is a reserved device name on Windows",
                    stem.trim_end_matches(' ').to_uppercase()
                )
            }
            other => String::from(other.reason()),
        });
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
    protected_problem(key)
        .or_else(|| system_problem(key, system))
        .or_else(|| {
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

// ==== Protected names ====

/// What kind of protected name a name is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProtectedKind {
    /// `.azlin`: the engine's own folder at the root of a synced folder or data tree.
    Bookkeeping,
    /// `.git`, `.hg`, `.svn`: a repository syncs through git, and what a name from a drive
    /// writes into `.git/hooks` runs as the user at the next `git commit`.
    Vcs,
    /// `.gitmodules`, `.gitattributes`: what a repository reads for its configuration. Warned of,
    /// not refused (the owner's call: start with warnings, refuse hooks and configuration paths).
    VcsMetadata,
    /// `$RECYCLE.BIN`, `System Volume Information`: a system's own folders (the sync's rules
    /// leave them out).
    OsPrivate,
}

/// The code points a disk ignores when it compares names (HFS+ and APFS skip them; git's own
/// `is_hfs_dotgit` lists the same): `.g\u{200c}it` is `.git` there.
fn is_ignorable(c: char) -> bool {
    matches!(
        u32::from(c),
        0x200c..=0x200f | 0x202a..=0x202e | 0x206a..=0x206f | 0xfeff
    )
}

/// A name as it is compared for "is this a protected name", the same on every system and disk (a
/// drive is shared between them): the part before a stream suffix (`.git::$INDEX_ALLOCATION`),
/// in NFC, without the code points a disk ignores, case folded (upper then lower: `ſ` and the
/// Kelvin sign fold as a disk folds them), without the dots and spaces NTFS drops at its end.
fn protected_fold(name: &str) -> String {
    let base = name.split(':').next().unwrap_or(name);
    let mut out = String::with_capacity(base.len());
    for c in base.nfc().filter(|c| !is_ignorable(*c)) {
        for upper in c.to_uppercase() {
            out.extend(upper.to_lowercase());
        }
    }
    out.trim_end_matches(['.', ' ']).to_string()
}

/// Whether the folded name `folded` is the protected `word` (`.git`) or its NTFS short name:
/// the first six letters of the name without its dot, `~` and a number (`GIT~1`, `AZLIN~1`).
fn is_word(folded: &str, word: &str) -> bool {
    if folded == word {
        return true;
    }
    let short: String = word.trim_start_matches('.').chars().take(6).collect();
    folded.split_once('~').is_some_and(|(stem, number)| {
        stem == short && !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit())
    })
}

/// The protected names: the folder's bookkeeping and the version-control folders, in any
/// spelling a disk reads as theirs (ARCH28 RF42; ARCH28 AR2-8's `Protected`).
pub struct Protected;

impl Protected {
    /// What `component` (one name of a path, never a path) is, whatever its case, Unicode form,
    /// ignorable code points, trailing dots and spaces, stream suffix or NTFS short name.
    #[must_use]
    pub fn kind(component: &str) -> Option<ProtectedKind> {
        let folded = protected_fold(component);
        if is_word(&folded, BOOKKEEPING) {
            Some(ProtectedKind::Bookkeeping)
        } else if VCS_FOLDERS.iter().any(|v| is_word(&folded, v)) {
            Some(ProtectedKind::Vcs)
        } else if [".gitmodules", ".gitattributes"]
            .iter()
            .any(|w| is_word(&folded, w))
        {
            Some(ProtectedKind::VcsMetadata)
        } else if ["$recycle.bin", "system volume information"]
            .iter()
            .any(|w| is_word(&folded, w))
        {
            Some(ProtectedKind::OsPrivate)
        } else {
            None
        }
    }

    /// Whether `component` is the bookkeeping folder's name (the caller knows it is the first name
    /// of the path: a user's folder `x/.azlin` is an ordinary folder).
    #[must_use]
    pub fn is_bookkeeping(component: &str) -> bool {
        Protected::kind(component) == Some(ProtectedKind::Bookkeeping)
    }

    /// Whether `component` is a version-control folder's name.
    #[must_use]
    pub fn is_vcs(component: &str) -> bool {
        Protected::kind(component) == Some(ProtectedKind::Vcs)
    }

    /// Whether `component` is a protected name spelled differently from the canonical one: it is
    /// no exact `.git` or `.azlin`, only a name a disk reads as one (`.GIT`, `GIT~1`, `.git.`).
    /// A copy of a repository keeps its `.git`; what a case-folding disk would merge into an
    /// existing `.git` is such a spelling.
    #[must_use]
    pub fn is_disguised(component: &str) -> bool {
        matches!(
            Protected::kind(component),
            Some(ProtectedKind::Bookkeeping | ProtectedKind::Vcs)
        ) && component != BOOKKEEPING
            && !VCS_FOLDERS.contains(&component)
    }

    /// The first name of `key` a sync refuses: the bookkeeping folder as the FIRST name, a
    /// version-control name at any depth and in any position - the last name too, whether the key
    /// names a folder, a file or a link (a file or link `.git` redirects git to a repository the
    /// drive's other participants write; X-DELTA-AZDRIVE-FILES-9). The position, the kind and the
    /// name as written.
    #[must_use]
    pub fn in_key(key: &str) -> Option<(usize, ProtectedKind, &str)> {
        key.split('/')
            .enumerate()
            .find_map(|(at, name)| match Protected::kind(name)? {
                ProtectedKind::Bookkeeping if at == 0 => {
                    Some((at, ProtectedKind::Bookkeeping, name))
                }
                ProtectedKind::Vcs => Some((at, ProtectedKind::Vcs, name)),
                _ => None,
            })
    }
}

// ==== The policy and the key that passed it ====

/// Why a name is admitted. The rows that refuse change with it: a name from a synced drive is
/// held to all of them, a name a person chose for their own files is not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    /// A key of a synced drive that becomes a file in the synced folder: every row, the protected
    /// names in any spelling included.
    Sync,
    /// A file of a drive downloaded to a folder the user chose. No production caller says it: the
    /// one place a drive's names reach a folder of this computer is the transfer plan
    /// ([`crate::xfer::plan`]), which cannot tell a Download from a Paste and admits every such name
    /// as [`Purpose::Copy`] - the same rows (X-DELTA-AZDRIVE-FILES-32). The two stay until the
    /// purposes are merged with [`Purpose::Local`]'s deletion (ARCH29 C6c).
    Download,
    /// The rescue copy of a drive.
    Rescue,
    /// A cloud file opened from a cache.
    Open,
    /// A name the user typed (a new folder, a rename): the shape and this system's rows.
    UserTyped,
    /// A copy between drives or into a folder.
    Copy,
    /// A key already on this disk or in this device's own records (a base entry, a trash
    /// operation): only what makes it unsafe as a path argument. It is the one purpose with no
    /// protected-name row, which is how a name from a DRIVE could be walked with it and made
    /// (X-DELTA-AZDRIVE-FILES-1); [`OnDisk`] is its key now - it can only be walked to what is
    /// there - and the callers move to it one file at a time (ARCH29 C6c). Deleted with the last.
    Local,
}

/// Why a key was not admitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// The key as it was given.
    pub key: String,
    /// Why, as a sentence.
    pub why: String,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\"{}\" cannot be a name here: {}", self.key, self.why)
    }
}

impl std::error::Error for Refusal {}

impl From<Refusal> for crate::beneath::BeneathError {
    fn from(r: Refusal) -> Self {
        crate::beneath::BeneathError::BadName(r.key)
    }
}

impl From<Refusal> for crate::DriveError {
    fn from(r: Refusal) -> Self {
        crate::DriveError::InvalidKey {
            key: r.key,
            reason: "it is not a name this computer can hold as it is meant",
        }
    }
}

/// A key that passed [`Policy::admit`]: the only argument [`crate::beneath::Root::parent_of`]
/// takes. It cannot be made any other way, copied from a file or defaulted.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LocalKey(String);

impl Deref for LocalKey {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LocalKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl LocalKey {
    /// The key as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The file under `root`: every name of the key pushed on it, none of which can leave it.
    #[must_use]
    pub fn under(&self, root: &std::path::Path) -> std::path::PathBuf {
        let mut path = root.to_path_buf();
        for name in self.0.split('/') {
            path.push(name);
        }
        path
    }
}

/// A key of a name that is on this disk already, or in a record this device wrote about its own
/// folder: an entry a scan or a listing of the trash read, a base entry, a line of the oplog or the
/// journal (ARCH29 C6c). It passed the shape of a path and this system's rows - what
/// [`Purpose::Local`] checked - and never the protected names' spelling rows: the disk has the file
/// already. What it can do is walk to a file that is there: [`Root::parent_of_existing`] never makes
/// a folder. A name that comes from a DRIVE, a device or a person and is to be MADE here is an
/// [`Admitted`] key, which carries the rows of its purpose; the two types do not convert into each
/// other, so a create cannot be given a key of the first kind (X-DELTA-AZDRIVE-FILES-1: a folder the
/// drive renamed to `.git` was made on the victim's disk by a walk that took any `&str` and a flag).
///
/// Made only by [`OnDisk::of_entry`] and [`OnDisk::of_record`]: the name says where the key came
/// from, and `names_gate` counts the callers of both per file.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct OnDisk(String);

impl Deref for OnDisk {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OnDisk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl OnDisk {
    /// The key of a directory entry this device read: what a scan found, what a listing of the
    /// trash's folder named.
    ///
    /// # Errors
    ///
    /// [`Refusal`] for a key that is no path of plain names (it leaves its folder, has a separator
    /// or a character this system refuses).
    pub fn of_entry(key: &str) -> Result<OnDisk, Refusal> {
        OnDisk::checked(key)
    }

    /// The key of a record this device wrote about its own folder: a base entry, a line of the
    /// oplog or of the journal.
    ///
    /// # Errors
    ///
    /// As [`OnDisk::of_entry`].
    pub fn of_record(key: &str) -> Result<OnDisk, Refusal> {
        OnDisk::checked(key)
    }

    fn checked(key: &str) -> Result<OnDisk, Refusal> {
        match Policy::here(Volume::default()).path_problem(key) {
            Some(why) => Err(Refusal {
                key: key.to_string(),
                why,
            }),
            None => Ok(OnDisk(key.to_string())),
        }
    }

    /// The key as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The path of the file under `root` (every name of the key pushed on it, none of which can
    /// leave it): a path to SAY - for a message to the user - nothing is opened or made by it.
    #[must_use]
    pub fn under(&self, root: &std::path::Path) -> std::path::PathBuf {
        let mut path = root.to_path_buf();
        for name in self.0.split('/') {
            path.push(name);
        }
        path
    }
}

/// A key of a name from a drive, a device, a user or a listing that passed [`Policy::admitted`]: the
/// rows of the purpose it is for, the protected names in every spelling among them. The only key
/// that makes a name here: [`Root::parent_of_new`] takes nothing else (ARCH29 C6c), and
/// [`Policy::admitted`] mints none for [`Purpose::Local`], so an [`OnDisk`] key - what is on the
/// disk already - cannot be turned into one. It cannot be made any other way, copied from a file or
/// defaulted. (The type the key of [`Policy::admit`] becomes when [`Purpose::Local`] is gone: the
/// renamed [`LocalKey`].)
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Admitted(LocalKey);

impl Deref for Admitted {
    type Target = str;

    fn deref(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for Admitted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.as_str())
    }
}

impl Admitted {
    /// The key as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// The file under `root`: every name of the key pushed on it, none of which can leave it.
    #[must_use]
    pub fn under(&self, root: &std::path::Path) -> std::path::PathBuf {
        self.0.under(root)
    }
}

/// What a name may be on this system and this disk: the one gate a name from a drive, a device,
/// a user or a listing passes before it is a path.
#[derive(Clone, Copy, Debug)]
pub struct Policy {
    pub system: System,
    pub volume: Volume,
}

impl Policy {
    /// This system, a disk that does `volume`.
    #[must_use]
    pub fn here(volume: Volume) -> Policy {
        Policy {
            system: System::here(),
            volume,
        }
    }

    /// The key as a [`LocalKey`], or why not. `key` is a file's key: `a/b/c.txt`.
    ///
    /// # Errors
    ///
    /// [`Refusal`], the first row the key breaks for `purpose`.
    pub fn admit(&self, key: &str, purpose: Purpose) -> Result<LocalKey, Refusal> {
        let refuse = |why: String| Refusal {
            key: key.to_string(),
            why,
        };
        let problem = match purpose {
            Purpose::Sync => key_problem(key).or_else(|| here_problem(key, self.system, &self.volume)),
            Purpose::Download | Purpose::Rescue | Purpose::Open | Purpose::Copy => {
                self.path_problem(key).or_else(|| {
                    key.split('/')
                        .find(|name| Protected::is_disguised(name))
                        .map(|name| {
                            format!(
                                "\"{name}\" is a spelling of a protected name (the bookkeeping or a \
                                 version-control folder) that a disk may read as the real one"
                            )
                        })
                })
            }
            Purpose::UserTyped | Purpose::Local => self.path_problem(key),
        };
        match problem {
            Some(why) => Err(refuse(why)),
            None => Ok(LocalKey(key.to_string())),
        }
    }

    /// The key as an [`Admitted`] one for `purpose` (a name to be made on this disk), or why not:
    /// [`Policy::admit`]'s rows for the six purposes that are about a name from outside, and never
    /// for [`Purpose::Local`] - what is on the disk already makes no new name.
    ///
    /// # Errors
    ///
    /// [`Refusal`], the first row the key breaks for `purpose`; for [`Purpose::Local`] always.
    pub fn admitted(&self, key: &str, purpose: Purpose) -> Result<Admitted, Refusal> {
        if purpose == Purpose::Local {
            return Err(Refusal {
                key: key.to_string(),
                why: String::from(
                    "a key of what is on this disk already makes no new name (it is an OnDisk key)",
                ),
            });
        }
        self.admit(key, purpose).map(Admitted)
    }

    /// The folder of `key` under `root` and the key's last name, reached without a link: the key
    /// is admitted for `purpose` ([`Policy::admit`]) and walked ([`Root::parent_of`]) in one step.
    /// The name is owned, since the admitted key is not kept.
    ///
    /// # Errors
    ///
    /// [`BeneathError::BadName`] for a key the policy refuses, else as [`Root::parent_of`].
    pub fn walk(
        &self,
        root: &Root,
        key: &str,
        purpose: Purpose,
        create: bool,
    ) -> Result<(Dir, String), BeneathError> {
        let admitted = self.admit(key, purpose)?;
        let (dir, name) = root.parent_of(&admitted, create)?;
        Ok((dir, name.to_string()))
    }

    /// The shape of the key and this system's rows: what makes a string unsafe as a path.
    fn path_problem(&self, key: &str) -> Option<String> {
        storage_key::check_path_key_on(key, self.system)
            .err()
            .map(|e| match e {
                crate::DriveError::InvalidKey { reason, .. } => reason.to_string(),
                other => other.to_string(),
            })
    }

    /// The name as this disk compares names ([`fold`]).
    #[must_use]
    pub fn fold(&self, name: &str) -> String {
        fold(name, &self.volume)
    }

    /// What a protected name `name` is, in any spelling, on every volume ([`Protected::kind`]).
    #[must_use]
    pub fn protected(&self, name: &str) -> Option<ProtectedKind> {
        Protected::kind(name)
    }

    /// A file or folder name made from `text` a person wrote (a drive's name for the folder
    /// "Copy everything" fills): see [`leaf_for_text`].
    #[must_use]
    pub fn leaf_for_text(&self, text: &str) -> String {
        leaf_for_text(text)
    }
}

/// A key already on this disk or in this device's own records, as a [`LocalKey`]: it passes the
/// shape and this system's rows ([`Purpose::Local`]), never a protected name's spelling check (the
/// disk has it already).
///
/// # Errors
///
/// [`Refusal`] for a key that is no path of plain names.
pub fn local_key(key: &str) -> Result<LocalKey, Refusal> {
    Policy::here(Volume::default()).admit(key, Purpose::Local)
}

/// [`Policy::walk`] for a key of the engine's own records or of what the disk shows
/// ([`Purpose::Local`]): its shape and this system's rows, never a protected name's spelling (the
/// disk has the file already). A name that came from the DRIVE is walked with
/// [`Purpose::Sync`], by a [`Policy`] that knows the folder's [`Volume`].
///
/// # Errors
///
/// As [`Policy::walk`].
pub fn walk_local(root: &Root, key: &str, create: bool) -> Result<(Dir, String), BeneathError> {
    Policy::here(Volume::default()).walk(root, key, Purpose::Local, create)
}

/// A folder or file name made from `text` a person wrote: the characters a name cannot hold
/// (separators, controls, and those Windows refuses) are `-`, the dots and spaces at its ends go,
/// a Windows device name (`CON`, `NUL.txt`) and a protected name get a `_` before them, and it is
/// cut to 120 bytes at a character. Nothing remains: the empty string, and the caller says a
/// word of its own.
#[must_use]
pub fn leaf_for_text(text: &str) -> String {
    let cleaned: String = text
        .trim()
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    let mut leaf = cleaned.trim_matches(['.', ' ']).to_string();
    if leaf.len() > 120 {
        let mut end = 120;
        while !leaf.is_char_boundary(end) {
            end -= 1;
        }
        leaf.truncate(end);
        leaf = leaf.trim_matches(['.', ' ']).to_string();
    }
    if leaf.is_empty() {
        return leaf;
    }
    if storage_key::is_windows_device_name(leaf.split('.').next().unwrap_or(&leaf))
        || Protected::kind(&leaf).is_some()
    {
        leaf.insert(0, '_');
    }
    leaf
}
