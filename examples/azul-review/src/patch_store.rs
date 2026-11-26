//! The patch files of the resolved conversations, in the data root: `review/patches-<hash of the workspace>/<time>-c<id>.patch`.
//!
//! A conversation that reaches its end (the agent answered in words, the last suggestion was taken or refused, or the
//! reviewer closed it) is written once as ONE file: the diff of what the reviewer took against the file as it is on the disk,
//! under a header that says which conversation it is and how it ended ([`crate::patch`]); a conversation that took nothing
//! is the header alone. One resolved a second time (opened again, decided again) writes a new file and removes the old one.
//!
//! The pure part is first ([`made`]: a note and the bytes of the file in, the text of the patch out); the part that hands the
//! files to the app kit's file jobs (a thread, never the callback) is below it.

use azul::prelude::*;
use azul_appkit::{
    data::app_key,
    files::{FileJob, FileOutcome},
    l10n::{t_args, Arg},
    ui as kit,
};

use crate::{
    model::Note,
    patch::{self, Header, Outcome},
    progress_store::root_hash,
    AppState,
};

/// The write-back tag of a patch write.
const TAG_PATCH: u64 = 5;

/// The longest words a header keeps of one conversation: the header cuts each line at its own bound too.
const MAX_WORDS_BYTES: usize = 8 * 1024;

/// A conversation as the patch file says it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Made {
    pub outcome: Outcome,
    /// The file's name in the workspace's folder ([`patch::file_name`]).
    pub name: String,
    /// The whole file: the header and, where something was taken, the diff.
    pub text: String,
    /// The file has a diff.
    pub changed: bool,
    /// How many suggestions the reviewer took that could not be put in the diff.
    pub left_out: usize,
}

/// The lines of the bytes of a file as the file has them: cut at each `\n` (a `\r` before it stays), the last line with no
/// end of its own is a line, the empty text has none. Bytes that are no UTF-8 are read as the page reads them (the bad
/// ones replaced).
#[must_use]
pub fn raw_lines(bytes: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    if lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
}

/// `text` as at most `max` bytes, cut at a letter.
fn bounded(text: String, max: usize) -> String {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

/// What the reviewer said: the dictation (or the note's own words when nothing was dictated), then each follow-up.
fn said_of(note: &Note) -> String {
    let dictated: Vec<&str> = note.talk.said.iter().map(|s| s.text.trim()).filter(|t| !t.is_empty()).collect();
    let mut words = if dictated.is_empty() { note.text.trim().to_string() } else { dictated.join(" ") };
    for turn in note.talk.turns.iter().filter(|t| t.who == crate::conversation::Who::Reviewer) {
        if !words.is_empty() {
            words.push_str(" / ");
        }
        words.push_str(turn.text.trim());
    }
    bounded(words, MAX_WORDS_BYTES)
}

/// What the agent said, answer by answer.
fn rationale_of(note: &Note) -> String {
    let answers: Vec<&str> = note.talk.turns.iter().filter(|t| t.who == crate::conversation::Who::Agent).map(|t| t.text.trim()).collect();
    bounded(answers.join(" / "), MAX_WORDS_BYTES)
}

/// The patch of a resolved conversation: its header, and the diff of what was taken against the file `file` (a path of the
/// workspace) whose bytes are `bytes` (or why they could not be read), at `now_unix_ms`. `base` is the workspace's revision.
#[must_use]
pub fn made(note: &Note, file: &str, bytes: Result<&[u8], &str>, base: Option<&str>, now_unix_ms: u64) -> Made {
    let outcome = note.talk.outcome();
    let path = file.replace('\\', "/");
    let mut files: Vec<(String, String)> = Vec::new();
    let mut left_out: Vec<String> = Vec::new();
    if outcome == Outcome::Accepted {
        match bytes {
            Ok(bytes) => {
                let accepted: Vec<&crate::conversation::Suggestion> = note.talk.accepted().collect();
                match patch::diff_for(&path, &raw_lines(bytes), &accepted) {
                    Ok(diff) => {
                        left_out = diff.left_out;
                        if diff.changed {
                            files.push((path.clone(), diff.text));
                        }
                    }
                    Err(why) => left_out.push(why),
                }
            }
            Err(why) => left_out.push(format!("the file could not be read: {why}")),
        }
    }
    let said = said_of(note);
    let rationale = rationale_of(note);
    let understood = note.talk.understood_text();
    let header = Header {
        conversation: note.id,
        outcome,
        base,
        time_unix_ms: now_unix_ms,
        said: &said,
        understood: understood.as_deref(),
        rationale: &rationale,
        left_out: &left_out,
    };
    Made { outcome, name: patch::file_name(note.id, now_unix_ms), text: patch::render(&header, &files), changed: !files.is_empty(), left_out: left_out.len() }
}

/// The drive key of the patch `name` of the workspace at `root`.
#[must_use]
pub fn key_of(root: &str, name: &str) -> String {
    app_key(crate::ABOUT.app_folder, &format!("patches-{}/{name}", root_hash(root)))
}

/// The patch a new one replaces: the name a conversation's last resolution wrote, when it is a patch's name and not the
/// name the new one has (which is written over).
#[must_use]
pub fn superseded(old: Option<&str>, new: &str) -> Option<String> {
    old.filter(|o| patch::is_name(o) && *o != new).map(str::to_string)
}

// ==== The write ====

/// Writes the patch of every conversation that was resolved since the last time (the file jobs run on a thread; what a
/// resolution wrote is marked at once, so that it is written once). A conversation whose file cannot be read or has
/// changed still has its header.
pub(crate) fn record_due(data: &RefAny, info: &mut CallbackInfo) {
    let mut handle = data.clone();
    let (data_root, jobs) = {
        let Some(mut s) = handle.downcast_mut::<AppState>() else {
            return;
        };
        if !s.notes.iter().any(|n| n.talk.due()) {
            return;
        }
        let Some(file) = s.file().map(|f| f.display.clone()) else {
            return;
        };
        let bytes = s.source.read(&file, crate::code::MAX_OPEN_BYTES).map_err(|why| format!("{why:?}"));
        let root = s.root.to_string_lossy().into_owned();
        let now = crate::agent_window::unix_now_ms();
        let mut jobs = Vec::new();
        for note in s.notes.iter_mut().filter(|n| n.talk.due()) {
            // the base is the workspace's git head: `None` until the git-head reader (`.git` files, no process) is on master
            let patch = made(note, &file, bytes.as_deref().map_err(String::as_str), None, now);
            if let Some(gone) = superseded(note.talk.patch.as_deref(), &patch.name) {
                jobs.push(FileJob::Delete { key: key_of(&root, &gone) });
            }
            note.talk.mark_logged(Some(patch.name.clone()));
            println!(
                "AZREVIEW_PATCH made c{} outcome={} changed={} left_out={} name={}",
                note.id,
                patch.outcome.name(),
                patch.changed,
                patch.left_out,
                patch.name
            );
            jobs.push(FileJob::Put { key: key_of(&root, &patch.name), bytes: patch.text.into_bytes() });
        }
        (s.data_root.clone(), jobs)
    };
    kit::spawn_file_jobs(info, &data_root, jobs, data.clone(), TAG_PATCH, on_written);
}

extern "C" fn on_written(mut app: RefAny, mut msg: RefAny, _info: CallbackInfo) -> Update {
    let Some(reply) = kit::take_reply(&mut msg) else {
        return Update::DoNothing;
    };
    let mut told = false;
    for outcome in &reply.outcomes {
        match (outcome.error(), outcome) {
            (None, FileOutcome::Put { key, .. }) => println!("AZREVIEW_PATCH written {key}"),
            (None, FileOutcome::Deleted { key, .. }) => println!("AZREVIEW_PATCH removed {key}"),
            (None, _) => {}
            (Some(why), _) => {
                println!("AZREVIEW_PATCH_ERROR {why}");
                if let Some(mut s) = app.downcast_mut::<AppState>() {
                    s.problem = Some(t_args("azreview-save-failed", &[("why", Arg::from(why.to_string()))]));
                }
                told = true;
            }
        }
    }
    if told {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

#[cfg(test)]
#[path = "patch_store_test.rs"]
mod patch_store_test;
