//! The area the files of a share are COPIED into, and how a URI of the share provider is mapped back to one of them.
//!
//! A share never hands another app the app's own file: it hands a copy, staged in a folder of its own
//! (`<cache>/azul-share/<token>/<name>`), so that the receiver can read exactly the files of THIS share, under the name the app
//! gave them, and nothing else of the app's data. On Android the receiver reads them through the app's `ContentProvider`
//! (`com.azul.share.AzulShareProvider`: `content://<package>.azul.share/<token>/<name>`), which serves only a path of this
//! shape that stays inside the area; on iOS the sheet is given the staged files' URLs.
//!
//! The provider is Java and self-contained (it may be asked in a process that never ran the engine, and a provider is created
//! before the activity sets up the app's folders, so it must not load the engine's library): `resolve` and `describe` here are
//! the same rules in Rust, pinned by tests, and the contract test `android_share_open_contract` checks that the Java names the
//! same token length, the same sidecar and the same checks.

use alloc::{string::String, vec::Vec};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use azul_core::share::ShareStatus;

use super::share::{ShareLimits, SharePlan};
use crate::mime;

/// The length of a share's token: 128 bits as lower-case hexadecimal.
pub const TOKEN_LEN: usize = 32;

/// How long a staged share is kept: the receiver may read late (a mail draft that is sent an hour later), so not at once.
pub const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// The file of a staged share that records the MIME type of each file (`name<TAB>type` per line): the provider answers
/// `getType` from it, so the type the app (or the table) decided is the type the receiver is told. It starts with a dot, so
/// no shown name can be it.
pub const TYPES_FILE: &str = ".types";

/// The most files the area keeps at once across all shares before it removes the oldest, whatever their age: a loop that
/// shares every second must not fill the phone.
pub const MAX_SHARES: usize = 32;

/// One staged file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedFile {
    /// Where the copy is.
    pub path: PathBuf,
    /// The name the receiver sees.
    pub name: String,
    /// The type the receiver is told.
    pub mime: String,
    /// Bytes.
    pub size: u64,
}

/// A staged share: the folder of its token and its files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedShare {
    /// The token (32 hex characters).
    pub token: String,
    /// The folder `<area>/<token>`.
    pub dir: PathBuf,
    /// The copies, in the order of the plan.
    pub files: Vec<StagedFile>,
}

/// What a provider tells a receiver about one served file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareFileInfo {
    /// The display name (`OpenableColumns.DISPLAY_NAME`).
    pub name: String,
    /// Bytes (`OpenableColumns.SIZE`).
    pub size: u64,
    /// The MIME type (`getType`).
    pub mime: String,
}

/// The folder the copies of all shares of an app live in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareArea {
    root: PathBuf,
}

/// A fresh token: 128 bits from the standard library's randomly keyed hasher, as 32 lower-case hexadecimal characters. It
/// only has to be unique and unguessable to a process that was not given the URI (the provider is not exported: another app
/// reaches it only through a grant of the URI it was handed).
#[must_use]
pub fn new_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let mut token = String::with_capacity(TOKEN_LEN);
    for round in 0_u8..2 {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u128(now);
        hasher.write_u32(std::process::id());
        hasher.write_u8(round);
        token.push_str(&format!("{:016x}", hasher.finish()));
    }
    token
}

/// Whether `token` has the shape of [`new_token`].
#[must_use]
pub fn is_token(token: &str) -> bool {
    token.len() == TOKEN_LEN && token.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl ShareArea {
    /// The area at `root` (the folder is made at the first share).
    #[must_use]
    pub const fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// The folder of the area.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Copy the files of `plan` into a folder of their own and write the types file. A failure leaves nothing behind.
    ///
    /// # Errors
    ///
    /// `ShareStatus::Failed(reason)` when the area cannot be made, a file cannot be copied, or a file grew past
    /// `limits.max_file_bytes` between the check of the plan and the copy.
    pub fn stage(&self, plan: &SharePlan, limits: &ShareLimits) -> Result<StagedShare, ShareStatus> {
        let failed = |reason: String| ShareStatus::Failed(reason.into());
        fs::create_dir_all(&self.root)
            .map_err(|e| failed(format!("cannot make the folder of the shares: {e}")))?;
        // best effort: an old share that cannot be removed is not a reason to refuse a new one
        let _removed = self.cleanup(SystemTime::now(), MAX_AGE);
        self.keep_at_most(MAX_SHARES.saturating_sub(1));

        let (token, dir) = self.fresh_dir().map_err(|e| failed(format!("cannot make the folder of this share: {e}")))?;
        let mut files = Vec::new();
        let mut types = String::new();
        for (source, name, mime, _size) in plan.files() {
            let path = dir.join(name);
            let copied = fs::copy(source, &path).and_then(|bytes| {
                if bytes > limits.max_file_bytes {
                    Err(std::io::Error::other(format!("{bytes} bytes, more than the {} allowed", limits.max_file_bytes)))
                } else {
                    Ok(bytes)
                }
            });
            match copied {
                Ok(size) => {
                    types.push_str(name);
                    types.push('\t');
                    types.push_str(mime);
                    types.push('\n');
                    files.push(StagedFile { path, name: name.to_string(), mime: mime.to_string(), size });
                }
                Err(e) => {
                    drop(fs::remove_dir_all(&dir));
                    return Err(failed(format!("{name} could not be copied for sharing: {e}")));
                }
            }
        }
        if let Err(e) = fs::write(dir.join(TYPES_FILE), types) {
            drop(fs::remove_dir_all(&dir));
            return Err(failed(format!("cannot write the types of this share: {e}")));
        }
        Ok(StagedShare { token, dir, files })
    }

    /// A new, empty folder of a fresh token (a collision is retried).
    fn fresh_dir(&self) -> std::io::Result<(String, PathBuf)> {
        let mut last = None;
        for _ in 0..4 {
            let token = new_token();
            let dir = self.root.join(&token);
            match fs::create_dir(&dir) {
                Ok(()) => return Ok((token, dir)),
                Err(e) => last = Some(e),
            }
        }
        Err(last.unwrap_or_else(|| std::io::Error::other("no token")))
    }

    /// Remove the shares older than `max_age` at `now`; how many went.
    pub fn cleanup(&self, now: SystemTime, max_age: Duration) -> usize {
        let mut removed = 0;
        for (path, modified) in self.shares() {
            let age = now.duration_since(modified).unwrap_or(Duration::ZERO);
            if age > max_age && fs::remove_dir_all(&path).is_ok() {
                removed += 1;
            }
        }
        removed
    }

    /// Remove the oldest shares until at most `keep` are left.
    fn keep_at_most(&self, keep: usize) {
        let mut shares = self.shares();
        if shares.len() <= keep {
            return;
        }
        shares.sort_by_key(|(_, modified)| *modified);
        let excess = shares.len() - keep;
        for (path, _) in shares.into_iter().take(excess) {
            drop(fs::remove_dir_all(path));
        }
    }

    /// The folders of the area that are shares (named by a token) and when they were last written.
    fn shares(&self) -> Vec<(PathBuf, SystemTime)> {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return Vec::new();
        };
        entries
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_str().is_some_and(is_token))
            .filter_map(|entry| {
                let meta = entry.metadata().ok()?;
                meta.is_dir().then(|| (entry.path(), meta.modified().unwrap_or(SystemTime::UNIX_EPOCH)))
            })
            .collect()
    }

    /// The file a provider URI path (`/<token>/<name>`, as `Uri.getPath()` gives it: decoded) names, if it names one of a
    /// share of this area: a token of the right shape, a name that is one path segment (not `.`, `..`, hidden, with a
    /// separator or a NUL), a regular file, and a canonical path that stays inside the canonical area. Anything else is `None`.
    #[must_use]
    pub fn resolve(&self, uri_path: &str) -> Option<PathBuf> {
        let (token, name) = split_uri_path(uri_path)?;
        let candidate = self.root.join(token).join(name);
        let canonical = fs::canonicalize(&candidate).ok()?;
        let area = fs::canonicalize(&self.root).ok()?;
        if !canonical.starts_with(&area) || !canonical.is_file() {
            return None;
        }
        Some(canonical)
    }

    /// What a provider answers a receiver for `uri_path`: the name, the size and the type; `None` when
    /// [`ShareArea::resolve`] refuses the path.
    #[must_use]
    pub fn describe(&self, uri_path: &str) -> Option<ShareFileInfo> {
        let path = self.resolve(uri_path)?;
        let (token, name) = split_uri_path(uri_path)?;
        let size = fs::metadata(&path).ok()?.len();
        let types = fs::read_to_string(self.root.join(token).join(TYPES_FILE)).unwrap_or_default();
        let mime = types
            .lines()
            .filter_map(|line| line.split_once('\t'))
            .find(|(known, _)| *known == name)
            .map_or_else(|| mime::for_file_name(name).to_string(), |(_, mime)| mime.to_string());
        Some(ShareFileInfo { name: name.to_string(), size, mime })
    }
}

/// The URI a receiver is given for `name` of the share `token`: `content://<authority>/<token>/<name>`, the name percent-encoded
/// (every byte but the unreserved characters of RFC 3986: letters, digits, `-`, `.`, `_`, `~`), so that the segment
/// is one segment whatever the name holds and `Uri.getPath()` decodes it back to the name.
#[must_use]
pub fn content_uri(authority: &str, token: &str, name: &str) -> String {
    format!("content://{authority}/{token}/{}", percent_encode(name))
}

/// `text` with every byte that is not an unreserved URI character written as `%XX` (upper-case hexadecimal).
#[must_use]
pub fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The inverse of [`percent_encode`] (what `Uri.getPath()` does); `None` for a `%` that is not followed by two hexadecimal
/// digits or for a result that is not UTF-8.
#[must_use]
pub fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// `/<token>/<name>` as `(token, name)`: exactly two segments, the first a token, the second a plain, visible file name.
fn split_uri_path(uri_path: &str) -> Option<(&str, &str)> {
    let rest = uri_path.strip_prefix('/').unwrap_or(uri_path);
    let (token, name) = rest.split_once('/')?;
    if !is_token(token) {
        return None;
    }
    let plain = !name.is_empty()
        && name.len() <= 255
        && name != "."
        && name != ".."
        && !name.starts_with('.')
        && !name.contains(['/', '\\', '\0']);
    plain.then_some((token, name))
}

#[cfg(test)]
#[path = "share_area_test.rs"]
mod share_area_test;
