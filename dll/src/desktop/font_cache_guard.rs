//! The font manifest, checked before it is believed.
//!
//! The registry of system fonts (`rust_fontconfig::registry::FcFontRegistry`) keeps what its scan found in one file,
//! the MANIFEST (`.../rfc/fonts/manifest.bin` in the platform's cache directory, the app's own cache directory on
//! Android), so that a second start loads a few hundred faces in 5 ms instead of parsing every font file of the
//! machine again (about 200 ms on macOS). rust-fontconfig 5.2.0 reads that file back on faith: it checks the
//! format number and inserts every entry. It never looks at the font files again, so
//!
//! - after an OS update that replaced a system font the OLD names, weights and coverage stay in the registry,
//! - a font that was removed stays, and every layout that picks it fails to load it,
//! - the file is rewritten on EVERY start, even when nothing changed,
//! - a file written by another build of the app, or cut by a crash, is trusted as far as its bytes happen to parse.
//!
//! This module is the check. Next to the manifest lives a KEY FILE (`manifest.key`, text) written by the same code
//! that writes the manifest, naming what the manifest was made under and what it describes:
//!
//! - the build of azul and the format of the manifest (a library that parses differently makes other entries),
//! - the OS build (macOS and iOS `kern.osversion`, Linux `/etc/os-release`; Windows and Android rely on the next
//!   item: an OS update replaces the files),
//! - a hash of the directories the registry scans (an app that scans more makes a manifest of its own),
//! - the length and a hash of the manifest itself (a manifest that is not the one this key file belongs to is a
//!   manifest nobody vouches for: a cut file, a damaged file, the file of another process),
//! - the size and the modification time of EVERY font file the manifest lists.
//!
//! [`check`] compares all of it with the machine: one read of each file of the pair, one `open` + `fstat` of each font
//! (on up to four threads: about a millisecond for 800 fonts). Anything that differs is no cache: the verdict says why
//! ([`Reason`]) and the caller scans as if there were none, then writes both files again ([`write_key_file`] after
//! the registry saved the manifest). Both writes are atomic (a temporary file of its own, then a rename), so a reader sees
//! the old file or the new one; two processes writing together leave a pair that does not match, which the next start
//! sees as [`Reason::ManifestChanged`] and replaces, never as a hit.
//!
//! A new font file needs no check: the scan of the directories finds it and parses it, as it does after a hit today.
//! Nothing here deletes a file of the user: a rejected cache is replaced by a rename.

use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::SystemTime,
};

use super::font_instance_store::checksum;

/// The format of the key file: bumped when its lines change (an older file is [`Reason::KeyFileDamaged`]), and WITH THE
/// rust-fontconfig PIN: a library that computes other values from the same font files (coverage, names, weights)
/// reads a manifest of the old one without error, and only another number here makes the cache rebuild.
pub const KEY_FILE_VERSION: u32 = 1;

/// The first word of a key file.
const MAGIC: &str = "azul-font-cache";

/// The most threads that check the font files (a manifest lists some hundreds of files).
const MAX_CHECK_THREADS: usize = 4;

/// Below this many fonts one thread checks them all: starting a thread costs more than the files.
const PARALLEL_FROM: usize = 64;

/// What a manifest was made under: the cache is the same only when this is the same.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    /// The version of azul that wrote the manifest (`CARGO_PKG_VERSION` of the library).
    pub azul_version: String,
    /// The format number of the manifest (`rust_fontconfig::disk_cache::FontManifest::CURRENT_VERSION`).
    pub manifest_version: u32,
    /// The build of the OS (see [`os_build`]); empty where the font files' own sizes and times say it.
    pub os_build: String,
    /// A hash of the directories the registry scans.
    pub scan_dirs: u64,
}

impl Key {
    /// The key of this process: this build of azul, `manifest_version`, this OS build, these scanned directories.
    #[must_use]
    pub fn current(manifest_version: u32, scan_dirs: &[PathBuf]) -> Self {
        Self::new(env!("CARGO_PKG_VERSION"), manifest_version, &os_build(), scan_dirs)
    }

    /// A key of the given parts (line breaks in a text are spaces: the key file is lines).
    #[must_use]
    pub fn new(azul_version: &str, manifest_version: u32, os_build: &str, scan_dirs: &[PathBuf]) -> Self {
        Self {
            azul_version: one_line(azul_version),
            manifest_version,
            os_build: one_line(os_build),
            scan_dirs: dirs_hash(scan_dirs),
        }
    }
}

/// `text` with every control character a space and the ends trimmed.
fn one_line(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

/// A hash of the directory list (their order and spelling: a registry configured otherwise is another registry).
#[must_use]
pub fn dirs_hash(dirs: &[PathBuf]) -> u64 {
    let mut bytes = Vec::new();
    for dir in dirs {
        bytes.extend_from_slice(dir.to_string_lossy().as_bytes());
        bytes.push(b'\n');
    }
    checksum(&bytes)
}

/// One font file as the manifest saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontStat {
    /// The path the registry loads the font from.
    pub path: String,
    /// Its size in bytes.
    pub size: u64,
    /// Its modification time in whole seconds since the epoch (what the manifest records).
    pub mtime_secs: u64,
}

/// The content of the key file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyFile {
    /// What the manifest was made under.
    pub key: Key,
    /// The length of the manifest it describes.
    pub manifest_len: u64,
    /// The hash ([`checksum`]) of the manifest it describes.
    pub manifest_hash: u64,
    /// Every font file the manifest lists.
    pub fonts: Vec<FontStat>,
}

impl KeyFile {
    /// The text of the key file: a line each for the format, the build, the manifest format, the OS build, the scanned
    /// directories, the manifest, the number of fonts, then `size mtime path` for each font, then `end` (a file that was
    /// cut before it is damaged).
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut text = String::with_capacity(160 + self.fonts.len() * 96);
        text.push_str(&format!("{MAGIC} {KEY_FILE_VERSION}\n"));
        text.push_str(&format!("azul {}\n", self.key.azul_version));
        text.push_str(&format!("manifest-version {}\n", self.key.manifest_version));
        text.push_str(&format!("os {}\n", self.key.os_build));
        text.push_str(&format!("dirs {:016x}\n", self.key.scan_dirs));
        text.push_str(&format!("manifest {} {:016x}\n", self.manifest_len, self.manifest_hash));
        // A path with a line break cannot be a line: its font is left to the scan (it is not checked, not trusted less).
        let fonts: Vec<&FontStat> = self.fonts.iter().filter(|f| !f.path.contains(['\n', '\r'])).collect();
        text.push_str(&format!("fonts {}\n", fonts.len()));
        for font in fonts {
            text.push_str(&format!("{} {} {}\n", font.size, font.mtime_secs, font.path));
        }
        text.push_str("end\n");
        text
    }

    /// Reads a key file back. Any line that is not where it should be, a number that is not one, a count that is not the
    /// number of fonts, or a missing `end` is an error that says which.
    ///
    /// # Errors
    ///
    /// The text is not a key file of this version.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut lines = text.lines();
        let mut next = |what: &str| lines.next().ok_or_else(|| format!("the file ends before the {what} line"));
        let header = next("first")?;
        if header != format!("{MAGIC} {KEY_FILE_VERSION}") {
            return Err(format!("not a key file of version {KEY_FILE_VERSION}: `{}`", clip(header)));
        }
        let azul_version = field(next("azul")?, "azul")?.to_string();
        let manifest_version = number(field(next("manifest-version")?, "manifest-version")?)?;
        let os_build = field(next("os")?, "os")?.to_string();
        let scan_dirs = hex(field(next("dirs")?, "dirs")?)?;
        let manifest = field(next("manifest")?, "manifest")?;
        let (manifest_len, manifest_hash) = manifest
            .split_once(' ')
            .ok_or_else(|| format!("the manifest line has no hash: `{}`", clip(manifest)))?;
        let manifest_len: u64 = number(manifest_len)?;
        let manifest_hash = hex(manifest_hash)?;
        let count: usize = number(field(next("fonts")?, "fonts")?)?;
        let mut fonts = Vec::with_capacity(count.min(1 << 16));
        for _ in 0..count {
            let line = next("font")?;
            let mut parts = line.splitn(3, ' ');
            let (Some(size), Some(mtime), Some(path)) = (parts.next(), parts.next(), parts.next()) else {
                return Err(format!("a font line is not `size mtime path`: `{}`", clip(line)));
            };
            fonts.push(FontStat {
                path: path.to_string(),
                size: number(size)?,
                mtime_secs: number(mtime)?,
            });
        }
        if next("end")? != "end" {
            return Err("more font lines than the count says".to_string());
        }
        Ok(Self {
            key: Key {
                azul_version,
                manifest_version,
                os_build,
                scan_dirs,
            },
            manifest_len,
            manifest_hash,
            fonts,
        })
    }
}

/// The text of `line` after the word `name` and a space (`os` with nothing after it is the empty text).
fn field<'a>(line: &'a str, name: &str) -> Result<&'a str, String> {
    let rest = line
        .strip_prefix(name)
        .ok_or_else(|| format!("expected the `{name}` line, found `{}`", clip(line)))?;
    match rest.strip_prefix(' ') {
        Some(value) => Ok(value),
        None if rest.is_empty() => Ok(""),
        None => Err(format!("expected the `{name}` line, found `{}`", clip(line))),
    }
}

fn number<T: std::str::FromStr>(text: &str) -> Result<T, String> {
    text.parse().map_err(|_| format!("`{}` is not a number", clip(text)))
}

fn hex(text: &str) -> Result<u64, String> {
    u64::from_str_radix(text, 16).map_err(|_| format!("`{}` is not a hexadecimal number", clip(text)))
}

/// At most 40 characters of `text`, for a message.
fn clip(text: &str) -> String {
    text.chars().take(40).collect()
}

/// Why a manifest was not believed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    /// There is a manifest and no key file (written by a build without this check).
    KeyFileMissing,
    /// The key file cannot be read as one (cut, damaged, of another version of the format): what is wrong with it.
    KeyFileDamaged(String),
    /// The manifest exists and cannot be read.
    ManifestUnreadable,
    /// The key file vouched for the manifest and the library refused to load it.
    ManifestRejected,
    /// The build of azul or the format of the manifest differs from the one that wrote it.
    VersionChanged {
        /// The one that wrote it.
        was: String,
        /// This process.
        now: String,
    },
    /// The OS build differs (an OS update).
    OsBuildChanged {
        /// The one that wrote it.
        was: String,
        /// This machine.
        now: String,
    },
    /// The registry scans other directories than the ones the manifest was made for.
    ScanDirsChanged,
    /// The manifest is not the one the key file describes: cut, damaged, or written by another process after the key file.
    ManifestChanged,
    /// Font files changed or went away since the manifest was written.
    FontsChanged {
        /// How many exist with another size or time.
        changed: usize,
        /// How many cannot be opened.
        gone: usize,
        /// The first of them in the manifest's order.
        first: String,
    },
}

impl std::fmt::Display for Reason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::KeyFileMissing => write!(f, "no key file"),
            Self::KeyFileDamaged(why) => write!(f, "key file damaged: {why}"),
            Self::ManifestUnreadable => write!(f, "manifest unreadable"),
            Self::ManifestRejected => write!(f, "manifest refused by the font library"),
            Self::VersionChanged { was, now } => write!(f, "version changed: {was} -> {now}"),
            Self::OsBuildChanged { was, now } => write!(f, "os build changed: {was} -> {now}"),
            Self::ScanDirsChanged => write!(f, "scanned directories changed"),
            Self::ManifestChanged => write!(f, "manifest is not the one the key file describes"),
            Self::FontsChanged { changed, gone, first } => {
                write!(f, "{changed} font files changed, {gone} gone, first: {first}")
            }
        }
    }
}

/// What [`check`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The manifest is the one its key file describes, under this key, and every font is as it was: load it, scan
    /// nothing again, write nothing (unless the scan finds a new font).
    Hit,
    /// There is no manifest: the first start, or a cache directory that was emptied.
    Miss,
    /// The platform has no cache directory for this process: nothing is read and nothing is kept.
    NoDirectory,
    /// There is a manifest that must not be used: scan, then replace it.
    Rebuilt(Reason),
}

impl Verdict {
    /// `hit`, `miss`, `miss (no cache directory)` or `rebuilt (<why>)`: the first part of the startup timeline's
    /// `fonts:cache` mark.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Hit => "hit".to_string(),
            Self::Miss => "miss".to_string(),
            Self::NoDirectory => "miss (no cache directory)".to_string(),
            Self::Rebuilt(why) => format!("rebuilt ({why})"),
        }
    }

    /// Whether the manifest is to be loaded.
    #[must_use]
    pub fn is_hit(&self) -> bool {
        matches!(self, Self::Hit)
    }
}

/// The key file that belongs to `manifest`: the same name with the extension `key`.
#[must_use]
pub fn key_file_path(manifest: &Path) -> PathBuf {
    manifest.with_extension("key")
}

/// Whether the manifest at `manifest` is to be believed under `expected`: reads the manifest and its key file and
/// compares them with each other, with `expected` and with the font files.
#[must_use]
pub fn check(manifest: &Path, expected: &Key) -> Verdict {
    let bytes = match std::fs::read(manifest) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Verdict::Miss,
        Err(_) => return Verdict::Rebuilt(Reason::ManifestUnreadable),
    };
    let text = match std::fs::read_to_string(key_file_path(manifest)) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Verdict::Rebuilt(Reason::KeyFileMissing),
        Err(e) => return Verdict::Rebuilt(Reason::KeyFileDamaged(e.to_string())),
    };
    match KeyFile::parse(&text) {
        Ok(file) => verify(&bytes, &file, expected),
        Err(why) => Verdict::Rebuilt(Reason::KeyFileDamaged(why)),
    }
}

/// [`check`] on what was read: the cheap comparisons first (the key, then the manifest's length and hash), the
/// font files last.
#[must_use]
pub fn verify(manifest: &[u8], file: &KeyFile, expected: &Key) -> Verdict {
    let (was, now) = (&file.key, expected);
    if was.azul_version != now.azul_version || was.manifest_version != now.manifest_version {
        return Verdict::Rebuilt(Reason::VersionChanged {
            was: format!("azul {} / manifest {}", was.azul_version, was.manifest_version),
            now: format!("azul {} / manifest {}", now.azul_version, now.manifest_version),
        });
    }
    if was.os_build != now.os_build {
        return Verdict::Rebuilt(Reason::OsBuildChanged {
            was: was.os_build.clone(),
            now: now.os_build.clone(),
        });
    }
    if was.scan_dirs != now.scan_dirs {
        return Verdict::Rebuilt(Reason::ScanDirsChanged);
    }
    if manifest.len() as u64 != file.manifest_len || checksum(manifest) != file.manifest_hash {
        return Verdict::Rebuilt(Reason::ManifestChanged);
    }
    let found = check_fonts(&file.fonts);
    if found.changed + found.gone > 0 {
        return Verdict::Rebuilt(Reason::FontsChanged {
            changed: found.changed,
            gone: found.gone,
            first: found.first.unwrap_or_default(),
        });
    }
    Verdict::Hit
}

/// What the font files look like now, against what the key file says.
#[derive(Debug, Default)]
struct FontsFound {
    changed: usize,
    gone: usize,
    first: Option<String>,
}

/// Size and modification time (whole seconds since the epoch) of the file at `path`, read the way the manifest
/// wrote them: the file is OPENED and the open file asked, so a font the sandbox lets the app read by its path
/// (iOS) is not "gone" for want of a `stat` by name.
#[must_use]
pub fn stat_of(path: &str) -> Option<(u64, u64)> {
    let meta = File::open(path).and_then(|file| file.metadata()).ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_secs());
    Some((meta.len(), mtime))
}

fn check_slice(fonts: &[FontStat]) -> FontsFound {
    let mut found = FontsFound::default();
    for font in fonts {
        match stat_of(&font.path) {
            Some((size, mtime)) if size == font.size && mtime == font.mtime_secs => {}
            Some(_) => {
                found.changed += 1;
                found.first.get_or_insert_with(|| font.path.clone());
            }
            None => {
                found.gone += 1;
                found.first.get_or_insert_with(|| font.path.clone());
            }
        }
    }
    found
}

fn check_fonts(fonts: &[FontStat]) -> FontsFound {
    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(MAX_CHECK_THREADS);
    if fonts.len() < PARALLEL_FROM || threads < 2 {
        return check_slice(fonts);
    }
    let chunk = fonts.len().div_ceil(threads);
    let parts: Vec<FontsFound> = std::thread::scope(|scope| {
        let handles: Vec<_> = fonts
            .chunks(chunk)
            .map(|part| scope.spawn(move || check_slice(part)))
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap_or_default())
            .collect()
    });
    // In the manifest's order: the first of the first part that has one.
    let mut total = FontsFound::default();
    for part in parts {
        total.changed += part.changed;
        total.gone += part.gone;
        if total.first.is_none() {
            total.first = part.first;
        }
    }
    total
}

/// The fonts of a registry as the key file wants them: every path once, with its size and time as they are now (a font
/// that cannot be opened is left out: the next start scans it).
#[must_use]
pub fn stats_of(paths: impl IntoIterator<Item = String>) -> Vec<FontStat> {
    let mut seen = std::collections::BTreeSet::new();
    let mut fonts = Vec::new();
    for path in paths {
        if !seen.insert(path.clone()) {
            continue;
        }
        if let Some((size, mtime_secs)) = stat_of(&path) {
            fonts.push(FontStat { path, size, mtime_secs });
        }
    }
    fonts
}

/// Writes the key file of `manifest` (after the manifest was written): the manifest as it is on disk NOW is what the
/// file describes, `fonts` is what the registry loaded them from.
///
/// # Errors
///
/// The manifest cannot be read, or the key file cannot be written.
pub fn write_key_file(manifest: &Path, key: &Key, fonts: Vec<FontStat>) -> std::io::Result<()> {
    let bytes = std::fs::read(manifest)?;
    let file = KeyFile {
        key: key.clone(),
        manifest_len: bytes.len() as u64,
        manifest_hash: checksum(&bytes),
        fonts,
    };
    write_atomic(&key_file_path(manifest), file.to_text().as_bytes())
}

/// What the end of a scan did to the files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Persisted {
    /// The start was a hit and the scan found nothing new: the manifest and the key file are exactly as they were (not
    /// rewritten, not touched).
    Unchanged,
    /// The scan found no font at all: an empty manifest would be a hit that knows none, so nothing is kept.
    NothingFound,
    /// The manifest and its key file were written.
    Written,
    /// A write failed: the next start scans again (the files are the old pair or none, never half of one).
    Failed,
}

/// The end of a scan: writes the manifest (`save_manifest`, the registry's own atomic save) and then the key file that
/// vouches for it (`font_paths`: every font file the registry loads from) - unless the start was a hit
/// (`loaded_faces` is what it loaded) and the scan ended with as many faces: then NOTHING is written, so a start
/// that finds the machine as it was leaves the cache alone (no rewrite, no new modification time, no disk traffic).
pub fn persist(
    manifest: &Path,
    key: &Key,
    loaded_faces: Option<usize>,
    faces_now: usize,
    save_manifest: impl FnOnce(&Path) -> bool,
    font_paths: impl FnOnce() -> Vec<String>,
) -> Persisted {
    if loaded_faces == Some(faces_now) {
        return Persisted::Unchanged;
    }
    if faces_now == 0 {
        return Persisted::NothingFound;
    }
    if !save_manifest(manifest) {
        return Persisted::Failed;
    }
    match write_key_file(manifest, key, stats_of(font_paths())) {
        Ok(()) => Persisted::Written,
        Err(_) => Persisted::Failed,
    }
}

/// Writes `bytes` to `path` so that a reader sees the old file or the new one: a temporary file of this call's own
/// name next to it (created exclusively, so no two writers share one), synced, then renamed over `path`.
///
/// # Errors
///
/// The directory cannot be made, or the file cannot be written or renamed (the temporary file is removed then).
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let thread: String = format!("{:?}", std::thread::current().id())
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    let mut name = path.file_name().map(std::ffi::OsStr::to_os_string).unwrap_or_default();
    name.push(format!(".tmp-{}-{nanos}-{thread}", std::process::id()));
    let temporary = path.with_file_name(name);
    let written = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()
    })();
    if let Err(e) = written {
        // Our own temporary file, which this call made a moment ago.
        let _ = std::fs::remove_file(&temporary);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(e);
    }
    Ok(())
}

/// The build of the OS the fonts belong to: the part of the key that an OS update changes without touching a font.
#[must_use]
pub fn os_build() -> String {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        apple_os_build().unwrap_or_default()
    }
    #[cfg(target_os = "linux")]
    {
        linux_os_build(&std::fs::read_to_string("/etc/os-release").unwrap_or_default())
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "linux")))]
    {
        // Windows and Android: the font files of an update differ in size or time, which the check reads.
        String::new()
    }
}

/// `kern.osversion` (macOS `24F74`, iOS `22C152`).
#[cfg(any(target_os = "macos", target_os = "ios"))]
fn apple_os_build() -> Option<String> {
    use core::ffi::{c_char, c_int, c_void, CStr};
    extern "C" {
        fn sysctlbyname(
            name: *const c_char,
            oldp: *mut c_void,
            oldlenp: *mut usize,
            newp: *mut c_void,
            newlen: usize,
        ) -> c_int;
    }
    let mut buffer = [0u8; 64];
    let mut len = buffer.len();
    // SAFETY: the name is NUL-terminated, the buffer has `len` bytes, no new value is set.
    let status = unsafe {
        sysctlbyname(
            c"kern.osversion".as_ptr(),
            buffer.as_mut_ptr().cast::<c_void>(),
            &mut len,
            core::ptr::null_mut(),
            0,
        )
    };
    if status != 0 {
        return None;
    }
    let text = CStr::from_bytes_until_nul(&buffer[..len.min(buffer.len())]).ok()?;
    Some(text.to_str().ok()?.to_string())
}

/// The distribution and its version out of the text of `/etc/os-release`: `ubuntu 22.04`, `arch rolling`.
#[cfg(any(target_os = "linux", test))]
#[must_use]
pub fn linux_os_build(os_release: &str) -> String {
    let value = |name: &str| {
        os_release
            .lines()
            .find_map(|line| line.strip_prefix(name)?.strip_prefix('='))
            .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_string())
            .unwrap_or_default()
    };
    [value("ID"), value("VERSION_ID"), value("BUILD_ID")]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
#[path = "font_cache_guard_test.rs"]
mod font_cache_guard_test;
