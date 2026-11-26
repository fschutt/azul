//! What is kept of the review in the data root: the pages seen of each file and the files the reviewer said are not
//! reviewed, one small file per workspace (`review/progress-<hash of the folder>.json`), written when something changed and
//! read when the window opens.
//!
//! The format is versioned and tolerant like the session archive's: a key a later version adds is ignored, an entry that
//! cannot be read is left out and the rest kept, and what is read is bounded (a hundred thousand files, a few thousand
//! ranges of pages each, no page past what a file can have): a damaged or hostile file costs some seen pages, never a crash.
//! The ranges are written as `[first, last]` pairs: a file read through in order is one pair.
//!
//! The pure part is first; the part that reads and writes the file through the app kit's file jobs is below it.

use azul::{
    json::Json,
    vec::U8VecRef,
};
use azul_appkit::{
    data::app_key,
    files::{FileJob, FileOutcome},
    l10n::{t_args, Arg},
    ui as kit,
};
use azul::prelude::*;

use crate::{
    progress::{Back, Book, Entry, Seen, MAX_FILES},
    session::{count, items, push_string, text_of},
    AppState,
};

/// The format number of the file.
pub const FORMAT: &str = "azreview-progress/1";
/// How many files are read from a file.
pub const MAX_ENTRIES: usize = MAX_FILES;
/// How many ranges of pages are read of one file.
pub const MAX_RANGES: usize = 4_096;

/// The 16 hex digits of the folder's FNV-1a that tell one workspace from another in the data root.
#[must_use]
pub fn root_hash(root: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in root.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// The name of the file of the workspace at `root`: `progress-` and [`root_hash`] of the folder, so that each workspace has
/// its own.
#[must_use]
pub fn file_name(root: &str) -> String {
    format!("progress-{}.json", root_hash(root))
}

/// The text of the file for `book` (the workspace at `root`): the files something is known of.
#[must_use]
pub fn to_json(book: &Book, root: &str) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(96 + book.len() * 48);
    out.push_str("{\"format\":");
    push_string(&mut out, FORMAT);
    out.push_str(",\"root\":");
    push_string(&mut out, root);
    if let Some(back) = book.back() {
        out.push_str(",\"return\":{\"file\":");
        push_string(&mut out, &back.file);
        let _ = write!(out, ",\"page\":{},\"line\":{}}}", back.page, back.line);
    }
    out.push_str(",\"files\":[");
    let mut first = true;
    for (path, entry) in book.iter().filter(|(_, e)| e.seen.count() > 0 || e.held) {
        out.push_str(if first { "\n  " } else { ",\n  " });
        first = false;
        out.push_str("{\"path\":");
        push_string(&mut out, path);
        out.push_str(",\"seen\":[");
        for (i, (a, b)) in entry.seen.ranges().into_iter().take(MAX_RANGES).enumerate() {
            if i > 0 {
                out.push(',');
            }
            let _ = write!(out, "[{a},{b}]");
        }
        out.push(']');
        if entry.held {
            out.push_str(",\"held\":true");
        }
        out.push('}');
    }
    out.push_str("\n]}\n");
    out
}

/// A range of pages out of a JSON pair, if it is one.
fn range_of(item: &Json) -> Option<(usize, usize)> {
    let pair = item.to_array().into_option()?;
    let pair = pair.as_slice();
    if pair.len() != 2 {
        return None;
    }
    let number = |j: &Json| -> Option<usize> {
        let v = j
            .as_int()
            .into_option()
            .or_else(|| j.as_float().into_option().map(|f| f as i64))?;
        usize::try_from(v).ok()
    };
    Some((number(&pair[0])?, number(&pair[1])?))
}

/// The book a file's text holds, with at most `cap` files.
///
/// # Errors
/// A sentence for the log when the bytes are no progress file of this app, or of a format this build does not read.
pub fn from_json_capped(bytes: &[u8], cap: usize) -> Result<Book, String> {
    let json = Json::parse_bytes(U8VecRef::from(bytes))
        .into_result()
        .map_err(|_| "the progress file is not valid JSON".to_string())?;
    if let Some(format) = text_of(&json, "format").filter(|f| f != FORMAT) {
        return Err(format!("the progress file is of format {format}, which this version of AzReview does not read"));
    }
    let mut book = Book::with_cap(cap);
    if let Some(place) = json.get_key("return").into_option() {
        let page = count(&place, "page").and_then(|n| usize::try_from(n).ok());
        let line = count(&place, "line").and_then(|n| u32::try_from(n).ok());
        if let (Some(file), Some(page), Some(line)) = (text_of(&place, "file"), page, line) {
            let _ = book.jumped_from(Back { file, page, line });
            // what is read is not yet to be kept again
            let _ = book.take_dirty();
        }
    }
    for item in items(&json, "files") {
        let Some(path) = text_of(&item, "path").filter(|p| !p.is_empty()) else {
            continue;
        };
        let ranges: Vec<(usize, usize)> = items(&item, "seen").iter().filter_map(range_of).take(MAX_RANGES).collect();
        let held = item.get_key("held").into_option().and_then(|h| h.as_bool().into_option()).unwrap_or(false);
        let Some(entry) = book.entry(&path) else {
            break;
        };
        *entry = Entry { seen: Seen::from_ranges(&ranges), held };
    }
    Ok(book)
}

/// [`from_json_capped`] with the cap of a book.
///
/// # Errors
/// As [`from_json_capped`].
pub fn from_json(bytes: &[u8]) -> Result<Book, String> {
    from_json_capped(bytes, MAX_ENTRIES)
}

// ==== In the data root ====

/// The write-back tags of the writes and reads of the progress file.
const TAG_SAVE: u64 = 3;
const TAG_LOAD: u64 = 4;

fn key_of(root: &std::path::Path) -> String {
    app_key(crate::ABOUT.app_folder, &file_name(&root.to_string_lossy()))
}

/// Writes the book (when something changed and no write is under way), on a thread; one asked for meanwhile runs after.
pub(crate) fn save(data: &RefAny, info: &mut CallbackInfo) {
    let mut handle = data.clone();
    let job = {
        let Some(mut s) = handle.downcast_mut::<AppState>() else {
            return;
        };
        if !s.progress.is_dirty() || s.root.as_os_str().is_empty() {
            return;
        }
        if s.progress_saving {
            s.progress_save_pending = true;
            return;
        }
        s.progress_saving = true;
        s.progress_save_pending = false;
        s.progress.take_dirty();
        let text = to_json(&s.progress, &s.root.to_string_lossy());
        (s.data_root.clone(), FileJob::Put { key: key_of(&s.root), bytes: text.into_bytes() })
    };
    kit::spawn_file_jobs(info, &job.0, vec![job.1], data.clone(), TAG_SAVE, on_saved);
}

extern "C" fn on_saved(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(reply) = kit::take_reply(&mut msg) else {
        return Update::DoNothing;
    };
    let again = {
        let Some(mut s) = app.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        s.progress_saving = false;
        for outcome in &reply.outcomes {
            match (outcome.error(), outcome) {
                (None, FileOutcome::Put { key, .. }) => println!("AZREVIEW_SAVED {key}"),
                (None, _) => {}
                (Some(why), _) => {
                    println!("AZREVIEW_PROGRESS_SAVE_ERROR {why}");
                    s.progress.set_dirty();
                    s.problem = Some(t_args("azreview-save-failed", &[("why", Arg::from(why.to_string()))]));
                }
            }
        }
        s.progress_save_pending
    };
    if again {
        save(&app, &mut info);
    }
    Update::DoNothing
}

/// Reads the book of the workspace and takes it into the one the app has, on a thread.
pub(crate) fn load(data: &RefAny, info: &mut CallbackInfo) {
    let mut handle = data.clone();
    let job = {
        let Some(s) = handle.downcast_ref::<AppState>() else {
            return;
        };
        if s.root.as_os_str().is_empty() {
            return;
        }
        (s.data_root.clone(), FileJob::Get { key: key_of(&s.root) })
    };
    kit::spawn_file_jobs(info, &job.0, vec![job.1], data.clone(), TAG_LOAD, on_loaded);
}

extern "C" fn on_loaded(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(reply) = kit::take_reply(&mut msg) else {
        return Update::DoNothing;
    };
    let mut changed = false;
    for outcome in reply.outcomes {
        if let FileOutcome::Got { result: Ok(Some(bytes)), key } = outcome {
            match from_json(&bytes) {
                Ok(kept) => {
                    let n = kept.len();
                    if let Some(mut s) = app.downcast_mut::<AppState>() {
                        s.progress.merge(kept);
                        s.rollup_dirty = true;
                    }
                    println!("AZREVIEW_PROGRESS_LOADED {n}");
                    changed = true;
                }
                Err(why) => println!("AZREVIEW_PROGRESS_LOAD_ERROR {key}: {why}"),
            }
        }
    }
    if changed {
        info.trigger_all_virtual_view_rerender();
        return Update::RefreshDom;
    }
    Update::DoNothing
}

#[cfg(test)]
#[path = "progress_store_test.rs"]
mod progress_store_test;
