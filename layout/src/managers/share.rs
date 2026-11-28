//! The share sheet: the platform-independent half of `CallbackInfo::share`.
//!
//! The data model is `azul_core::share`. This module is what is pure and shared by every platform:
//!
//! * the LIMITS of a request (how many items, how large, how long a name) and the check of a request against them
//!   ([`plan`]), which answers `ShareStatus::Refused` with the reason for a request that is not one and gives the
//!   backends a [`SharePlan`]: the request with its names made safe and unique, its types decided and its sizes read;
//! * the slot the platform's backend is installed in ([`ShareChannel`], a field of `AppServices`), which the shells fill
//!   at the start the way they install the system clipboard's reader - or, for a headless / scripted run, the RECORDER,
//!   which keeps every request instead of showing a system sheet (`assert_shared` of the e2e scenarios reads it).
//!
//! The copies a share hands over are staged by [`super::share_area`]; the OS plumbing is `azul-dll`'s
//! (`desktop/extra/share`).

use alloc::{string::String, sync::Arc, vec::Vec};
use std::{
    fmt,
    path::PathBuf,
    sync::{Mutex, PoisonError},
};

use azul_core::{
    geom::LogicalRect,
    share::{ShareItemKind, ShareRequest, ShareStatus},
};

use crate::mime;

/// What one share may carry. Every limit is a CAP with a diagnostic, never a crash: a request over it is refused with
/// the number and the limit in the reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShareLimits {
    /// Items in one request.
    pub max_items: usize,
    /// One file, in bytes: a share is a COPY, and the receiver reads it through a provider.
    pub max_file_bytes: u64,
    /// All files of a request together, in bytes.
    pub max_total_bytes: u64,
    /// A shared text, in bytes: the intent that carries it crosses the system's 1 MB transaction buffer.
    pub max_text_bytes: usize,
    /// A file name the receiver shows, in bytes of UTF-8.
    pub max_name_bytes: usize,
    /// The title of the sheet, in bytes.
    pub max_title_bytes: usize,
}

impl Default for ShareLimits {
    fn default() -> Self {
        Self {
            max_items: ShareRequest::MAX_ITEMS,
            max_file_bytes: 256 * 1024 * 1024,
            max_total_bytes: 512 * 1024 * 1024,
            max_text_bytes: 256 * 1024,
            max_name_bytes: 128,
            max_title_bytes: 256,
        }
    }
}

/// A file name another app may be shown and a folder may hold: no path separator, no control or direction-changing
/// character, none of `: * ? " < > |`, no leading dot (a hidden file), no surrounding space or trailing dot, at most
/// `max_bytes` bytes of UTF-8 (cut at a character, the extension kept), and never empty (`file`).
#[must_use]
pub fn sanitize_display_name(name: &str, max_bytes: usize) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = base
        .chars()
        .map(|c| if is_unsafe_in_a_name(c) { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').trim_end_matches(['.', ' ']);
    if cleaned.is_empty() || cleaned.chars().all(|c| c == '_' || c == '.') {
        return String::from("file");
    }
    cut_keeping_extension(cleaned, max_bytes.max(8))
}

/// A character that is replaced in a shown file name: controls, the reserved punctuation of the strictest file systems
/// and the invisible characters that change how a name reads (the right-to-left override of "photo\u{202e}gpj.exe").
fn is_unsafe_in_a_name(c: char) -> bool {
    c.is_control()
        || matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|')
        || matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}')
}

/// `name` cut to at most `max_bytes` bytes at a character boundary, the extension (up to 16 bytes) kept.
fn cut_keeping_extension(name: &str, max_bytes: usize) -> String {
    if name.len() <= max_bytes {
        return name.to_string();
    }
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() && extension.len() <= 16 => (stem, format!(".{extension}")),
        _ => (name, String::new()),
    };
    let room = max_bytes.saturating_sub(extension.len()).max(1);
    let mut end = room.min(stem.len());
    while !stem.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{extension}", &stem[..end])
}

/// `names` made unique, in order: the second `a.pdf` becomes `a (2).pdf`, the third `a (3).pdf`; a name that already
/// looks like a numbered one is not confused with it. Comparison ignores case (a receiver may be case-insensitive).
#[must_use]
pub fn unique_names(names: Vec<String>) -> Vec<String> {
    let mut taken: Vec<String> = Vec::with_capacity(names.len());
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        let mut candidate = name.clone();
        let mut n = 1_u32;
        while taken.iter().any(|t| t.eq_ignore_ascii_case(&candidate)) {
            n += 1;
            let (stem, extension) = match name.rsplit_once('.') {
                Some((stem, extension)) if !stem.is_empty() => (stem, format!(".{extension}")),
                _ => (name.as_str(), String::new()),
            };
            candidate = format!("{stem} ({n}){extension}");
        }
        taken.push(candidate.clone());
        out.push(candidate);
    }
    out
}

/// One thing of a [`SharePlan`], ready for a backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlannedItem {
    /// A file: where it is now, the name and type the receiver is told, and its size.
    File {
        /// The app's file.
        source: PathBuf,
        /// The name the receiver shows: safe, unique among the files of the request.
        name: String,
        /// The type the receiver is told.
        mime: String,
        /// Bytes.
        size: u64,
    },
    /// A text.
    Text(String),
    /// A link.
    Url(String),
}

/// A [`ShareRequest`] that passed the limits, as the backends take it.
#[derive(Debug, Clone, PartialEq)]
pub struct SharePlan {
    /// The items, in the order the app gave them.
    pub items: Vec<PlannedItem>,
    /// The title of the sheet / the subject of a mail; may be empty.
    pub title: String,
    /// Where a tablet's popover is anchored (window coordinates).
    pub anchor: Option<LogicalRect>,
    /// "Open with...": exactly one file, offered to the apps that can open it.
    pub open_with: bool,
    /// The bytes of all files together.
    pub total_bytes: u64,
    /// The limits the plan was checked against (the staging checks a copy against the same ones).
    pub limits: ShareLimits,
}

impl SharePlan {
    /// The files of the plan: `(source, name, mime, size)`.
    pub fn files(&self) -> impl Iterator<Item = (&PathBuf, &str, &str, u64)> {
        self.items.iter().filter_map(|item| match item {
            PlannedItem::File { source, name, mime, size } => Some((source, name.as_str(), mime.as_str(), *size)),
            _ => None,
        })
    }

    /// The texts and links of the plan, in order (what a `text/plain` share carries).
    #[must_use]
    pub fn texts(&self) -> Vec<&str> {
        self.items
            .iter()
            .filter_map(|item| match item {
                PlannedItem::Text(t) | PlannedItem::Url(t) => Some(t.as_str()),
                PlannedItem::File { .. } => None,
            })
            .collect()
    }

    /// The one MIME type of the whole share (what an intent of it is typed with): `text/plain` for texts only, the common
    /// type of the files otherwise.
    #[must_use]
    pub fn mime(&self) -> String {
        let types: Vec<&str> = self.files().map(|(_, _, mime, _)| mime).collect();
        if types.is_empty() {
            "text/plain".to_string()
        } else {
            mime::common_type(&types)
        }
    }
}

/// The check of a request against `limits`: the plan, or the status to answer (`Refused` for a request that is not one,
/// `Failed` for a file that cannot be read). Reads the size of each file (one `stat`), nothing else.
///
/// # Errors
///
/// `ShareStatus::Refused(reason)` when the request has no item, more than `limits.max_items`, a text or a title over its
/// limit, an empty text or link, a link without a scheme, a folder, a file over `limits.max_file_bytes` or files over
/// `limits.max_total_bytes`, or "open with" without exactly one file; `ShareStatus::Failed(reason)` when a file cannot be
/// read.
pub fn plan(request: &ShareRequest, limits: &ShareLimits) -> Result<SharePlan, ShareStatus> {
    let refused = |reason: String| ShareStatus::Refused(reason.into());
    let items = request.items.as_ref();
    if items.is_empty() {
        return Err(refused("nothing to share: the request has no item".to_string()));
    }
    if items.len() > limits.max_items {
        return Err(refused(format!(
            "too many items to share ({}; at most {})",
            items.len(),
            limits.max_items
        )));
    }
    let title = request.title.as_str();
    if title.len() > limits.max_title_bytes {
        return Err(refused(format!(
            "the title of the share is too long ({} bytes; at most {})",
            title.len(),
            limits.max_title_bytes
        )));
    }
    let files = items.iter().filter(|i| i.kind == ShareItemKind::File).count();
    if request.open_with && (files != 1 || items.len() != 1) {
        return Err(refused(format!(
            "'open with' needs exactly one file and nothing else (the request has {files} file(s) of {} item(s))",
            items.len()
        )));
    }

    let wanted_names: Vec<String> = items
        .iter()
        .filter(|i| i.kind == ShareItemKind::File)
        .map(|i| {
            let given = i.name.as_str();
            let from = if given.is_empty() { i.value.as_str() } else { given };
            sanitize_display_name(from, limits.max_name_bytes)
        })
        .collect();
    let mut names = unique_names(wanted_names).into_iter();

    let mut planned = Vec::with_capacity(items.len());
    let mut total: u64 = 0;
    for item in items {
        let value = item.value.as_str();
        match item.kind {
            ShareItemKind::Text => {
                if value.trim().is_empty() {
                    return Err(refused("the text to share is empty".to_string()));
                }
                if value.len() > limits.max_text_bytes {
                    return Err(refused(format!(
                        "the text to share is too long ({} bytes; at most {})",
                        value.len(),
                        limits.max_text_bytes
                    )));
                }
                planned.push(PlannedItem::Text(value.to_string()));
            }
            ShareItemKind::Url => {
                if !has_scheme(value) {
                    return Err(refused(format!("{value:?} is not a link (no scheme such as https:)")));
                }
                if value.len() > limits.max_text_bytes {
                    return Err(refused(format!("the link to share is too long ({} bytes)", value.len())));
                }
                planned.push(PlannedItem::Url(value.to_string()));
            }
            ShareItemKind::File => {
                let name = names.next().unwrap_or_else(|| "file".to_string());
                let source = PathBuf::from(value);
                let meta = std::fs::metadata(&source).map_err(|e| {
                    ShareStatus::Failed(format!("{name} cannot be read: {e}").into())
                })?;
                if !meta.is_file() {
                    return Err(refused(format!("{name} is a folder: only files can be shared")));
                }
                let size = meta.len();
                if size > limits.max_file_bytes {
                    return Err(refused(format!(
                        "{name} is too large to share ({} MiB; the limit is {} MiB)",
                        size.div_ceil(1024 * 1024),
                        limits.max_file_bytes / (1024 * 1024)
                    )));
                }
                total = total.saturating_add(size);
                if total > limits.max_total_bytes {
                    return Err(refused(format!(
                        "the files to share are too large together ({} MiB; the limit is {} MiB)",
                        total.div_ceil(1024 * 1024),
                        limits.max_total_bytes / (1024 * 1024)
                    )));
                }
                let given = item.mime.as_str();
                let mime = if mime::is_valid(&mime::base_type(given)) && !given.is_empty() {
                    mime::base_type(given)
                } else {
                    mime::for_file_name(&name).to_string()
                };
                planned.push(PlannedItem::File { source, name, mime, size });
            }
        }
    }
    Ok(SharePlan {
        items: planned,
        title: title.to_string(),
        anchor: request.anchor.as_ref().copied(),
        open_with: request.open_with,
        total_bytes: total,
        limits: *limits,
    })
}

/// Whether `text` starts with a URL scheme (`https:`, `geo:`, `mailto:`).
fn has_scheme(text: &str) -> bool {
    let Some((scheme, rest)) = text.split_once(':') else {
        return false;
    };
    let mut chars = scheme.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        && !rest.is_empty()
}

/// The OS plumbing of one platform, as the shell installs it.
#[derive(Clone, Copy)]
pub struct ShareBackend {
    /// Show the system's sheet for a checked plan; the status says whether it is up.
    pub share: fn(&SharePlan) -> ShareStatus,
    /// `Some(reason)` when this platform has no share sheet (a desktop), `None` when it has one.
    pub unavailable: fn() -> Option<String>,
}

impl fmt::Debug for ShareBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ShareBackend")
    }
}

/// A share the recorder kept (a headless / scripted run: no system sheet is shown).
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedShare {
    /// What the platform would have been given.
    pub plan: SharePlan,
}

#[derive(Debug, Default)]
struct Inner {
    backend: Option<ShareBackend>,
    recording: bool,
    recorded: Vec<RecordedShare>,
    limits: Option<ShareLimits>,
}

/// The app's share slot: the installed backend, the limits, and the recorder of a scripted run. The windows of one app
/// share one (`AppServices::share`); a clone is another handle on the SAME slot.
#[derive(Debug, Clone, Default)]
pub struct ShareChannel {
    inner: Arc<Mutex<Inner>>,
}

impl ShareChannel {
    /// A new slot of its own, with no backend, shared with nobody.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Install the platform's backend (the shell, at the start); `None` removes it.
    pub fn install(&self, backend: Option<ShareBackend>) {
        self.lock().backend = backend;
    }

    /// Change the limits of the requests this app may make (the defaults are [`ShareLimits::default`]).
    pub fn set_limits(&self, limits: ShareLimits) {
        self.lock().limits = Some(limits);
    }

    /// Record every request instead of showing a sheet (a headless run, an e2e scenario): the request is still CHECKED
    /// against the limits, so a scenario catches a request the platform would refuse.
    pub fn record_instead(&self) {
        self.lock().recording = true;
    }

    /// Whether the requests are only recorded (the recorder was asked for, or the run opens nothing: a scripted run).
    #[must_use]
    pub fn is_recording(&self) -> bool {
        Self::recording_in(&self.lock())
    }

    // `test_hooks` is a `const fn` only in a build without the `test-hooks` feature (the QA builds have it): clippy sees the first
    #[allow(clippy::missing_const_for_fn)]
    fn recording_in(inner: &Inner) -> bool {
        // a run that opens nothing (`Url::open` answers "opened" and starts no browser) shows no system sheet either
        inner.recording || azul_core::run_mode::test_hooks().open_nothing
    }

    /// The requests the recorder kept, oldest first.
    #[must_use]
    pub fn recorded(&self) -> Vec<RecordedShare> {
        self.lock().recorded.clone()
    }

    /// Whether this app can show a share sheet now (what a Share button asks before it shows itself).
    #[must_use]
    pub fn can_share(&self) -> bool {
        let inner = self.lock();
        Self::recording_in(&inner)
            || inner
                .backend
                .is_some_and(|backend| (backend.unavailable)().is_none())
    }

    /// Why there is no share sheet, when there is none.
    #[must_use]
    pub fn unavailable_reason(&self) -> Option<String> {
        let inner = self.lock();
        if Self::recording_in(&inner) {
            return None;
        }
        match inner.backend {
            None => Some(NO_BACKEND.to_string()),
            Some(backend) => (backend.unavailable)(),
        }
    }

    /// Show the system's share sheet for `request`, or say why not. The answer is the real one: `Shown` means the sheet is
    /// up (on the thread the callback runs on, where the platform allows it).
    #[must_use]
    pub fn share(&self, request: &ShareRequest) -> ShareStatus {
        // copied out: the backend runs OUTSIDE the lock (it may take as long as a copy of 500 MB)
        let (backend, recording, limits) = {
            let inner = self.lock();
            (inner.backend, Self::recording_in(&inner), inner.limits.unwrap_or_default())
        };
        let unavailable = if recording {
            None
        } else {
            match backend {
                None => Some(NO_BACKEND.to_string()),
                Some(b) => (b.unavailable)(),
            }
        };
        if let Some(reason) = unavailable {
            return ShareStatus::Unsupported(reason.into());
        }
        let plan = match plan(request, &limits) {
            Ok(plan) => plan,
            Err(status) => return status,
        };
        match backend {
            Some(b) if !recording => (b.share)(&plan),
            _ => {
                let mut inner = self.lock();
                if inner.recorded.len() >= MAX_RECORDED {
                    inner.recorded.remove(0);
                }
                inner.recorded.push(RecordedShare { plan });
                ShareStatus::Shown
            }
        }
    }
}

/// The recorder keeps the newest this many shares: a headless app that shares in a loop must not grow without bound.
pub const MAX_RECORDED: usize = 256;

/// Why a runtime without an installed backend has no share sheet.
const NO_BACKEND: &str = "this runtime has no share sheet (no platform backend was installed)";

#[cfg(test)]
#[path = "share_test.rs"]
mod share_test;
