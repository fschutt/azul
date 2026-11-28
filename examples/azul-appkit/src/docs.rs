//! The OPEN DOCUMENTS of a document app (AzWriter, AzSheets, AzShow): what the sidebar lists, and
//! what each row says of its document.
//!
//! The wireframe's rule: in these apps the left sidebar is the document switcher (the open
//! documents are its "tabs", so one switches between `book1.xlsx` and `book2.xlsx` at once), and
//! every row carries a SYNC PIE and a COLLABORATION icon (off / on). The sync kit that would fill
//! them (presence, comments, merging edits; `briefs/sync_kit_todo_2026-11-28.md`) is a TODO, so
//! this is UI only, and the honest parts of it are real:
//!
//! - the pie is the document's LOCAL SAVE STATE ([`DocSync`]): saved on this device, being
//!   written, changed since the last write, or not written because the write failed. When the
//!   kit exists its states (offline, waiting for a peer, conflict, shared with N) join the enum;
//!   the app's rows do not change.
//! - the collaboration icon is a per-document switch the app keeps in its model
//!   ([`DocRow::collab`]); switching it on connects nothing yet, and the popover of the pie says so
//!   ([`COLLAB_NOTE_KEY`]).
//!
//! This module is plain Rust (no azul types, so its tests need no libazul): [`DocRow`] is what an
//! app's model says of one open document, [`doc_tag`] / [`doc_of_tag`] and the list rules
//! ([`insert_at`], [`active_after_close`]) are the vocabulary of the sidebar's report, and
//! `frame::doc_rows` (feature `azul`) turns the rows into the sidebar's. Tags: the open documents
//! are `DOC_TAG_BASE + index` (an app's other rows use other bases), at most [`MAX_OPEN_DOCS`]
//! documents are open at once.

/// The most documents open at once: a realistic ceiling for a person's working set, and the
/// bound of the tag range. Opening one more is refused with a notice, never a crash.
pub const MAX_OPEN_DOCS: usize = 32;

/// The sidebar tag of the first open document; the n-th is `DOC_TAG_BASE + n`.
pub const DOC_TAG_BASE: usize = 10_000;

/// The sidebar tag of the "New or open" row.
pub const NEW_TAG: usize = 9_000;

/// The sidebar tag of a row of the app's own (its outline, its sheets) starts here: nothing of
/// the open documents or of this module's rows lies at or above it.
pub const APP_TAG_BASE: usize = 20_000;

/// The characters of a document's name a row shows before it cuts the name.
pub const NAME_CHARS: usize = 48;

/// Where a document's save stands; the sync pie's meaning until the sync kit exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DocSync {
    /// The device has the document as it is on screen.
    #[default]
    Saved,
    /// A write is on its way.
    Saving,
    /// The document changed since the last write.
    Unsaved,
    /// The last write failed; the document stays open and unsaved.
    Failed,
}

/// What a pie's number means; azul's `PieTone` without the engine (`frame::pie_tone` maps it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Just how far it is.
    Accent,
    /// Working: the accent, breathing.
    Busy,
    /// Done: green.
    Good,
    /// Waiting: amber.
    Warning,
    /// Needs the user: red.
    Danger,
}

impl DocSync {
    /// The state a document is in, from what the app knows: a write on its way wins, then a
    /// failed write, then changes not written yet, else saved.
    #[must_use]
    pub const fn of(dirty: bool, saving: bool, failed: bool) -> Self {
        if saving {
            Self::Saving
        } else if failed {
            Self::Failed
        } else if dirty {
            Self::Unsaved
        } else {
            Self::Saved
        }
    }

    /// How far the pie is, 0 to 1: full when the state is settled, a wedge while something is
    /// still to be written.
    #[must_use]
    pub const fn fraction(self) -> f32 {
        match self {
            Self::Saved | Self::Failed => 1.0,
            Self::Saving => 0.5,
            Self::Unsaved => 0.25,
        }
    }

    /// What the pie's colour says.
    #[must_use]
    pub const fn tone(self) -> Tone {
        match self {
            Self::Saved => Tone::Good,
            Self::Saving => Tone::Busy,
            Self::Unsaved => Tone::Warning,
            Self::Failed => Tone::Danger,
        }
    }

    /// The short word ("Saved"): the pie's accessible name and the status slot's text.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Saved => "kit-doc-saved",
            Self::Saving => "kit-doc-saving",
            Self::Unsaved => "kit-doc-unsaved",
            Self::Failed => "kit-doc-failed",
        }
    }

    /// The sentence of the popover that says what the state means.
    #[must_use]
    pub const fn detail_key(self) -> &'static str {
        match self {
            Self::Saved => "kit-doc-saved-detail",
            Self::Saving => "kit-doc-saving-detail",
            Self::Unsaved => "kit-doc-unsaved-detail",
            Self::Failed => "kit-doc-failed-detail",
        }
    }
}

/// The note under the collaboration switch while the sync kit does not exist.
pub const COLLAB_NOTE_KEY: &str = "kit-doc-collab-note";

/// What an app's model says of one open document, in the order the sidebar lists them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocRow {
    /// The name shown ("report.docx", the title of a note).
    pub name: String,
    /// Where its save stands.
    pub sync: DocSync,
    /// Whether collaboration is switched on for it (UI only).
    pub collab: bool,
    /// Whether this is the document the main view shows.
    pub selected: bool,
}

impl DocRow {
    /// A row for `name`, saved, collaboration off, not selected.
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            sync: DocSync::Saved,
            collab: false,
            selected: false,
        }
    }

    /// The name as a row shows it: cut to [`NAME_CHARS`] characters with an ellipsis; a blank
    /// name is `fallback`.
    #[must_use]
    pub fn shown_name(&self, fallback: &str) -> String {
        shown_name(&self.name, fallback)
    }
}

/// `name` as a row shows it: its first line, trimmed, cut to [`NAME_CHARS`] characters with an
/// ellipsis; `fallback` when nothing is left.
#[must_use]
pub fn shown_name(name: &str, fallback: &str) -> String {
    let line = name.lines().next().unwrap_or("").trim();
    if line.is_empty() {
        return fallback.to_string();
    }
    if line.chars().count() > NAME_CHARS {
        let cut: String = line.chars().take(NAME_CHARS - 1).collect();
        format!("{cut}\u{2026}")
    } else {
        line.to_string()
    }
}

/// The sidebar tag of the open document at `index`.
#[must_use]
pub const fn doc_tag(index: usize) -> usize {
    DOC_TAG_BASE + index
}

/// The index of the open document a sidebar tag names; `None` for any other tag.
#[must_use]
pub const fn doc_of_tag(tag: usize) -> Option<usize> {
    match tag.checked_sub(DOC_TAG_BASE) {
        Some(index) if index < MAX_OPEN_DOCS => Some(index),
        _ => None,
    }
}

/// Where the open list goes after document `closed` was closed while `active` was shown, `len_before`
/// documents being open: the index of the document that takes the screen in the shortened list
/// (`None`: nothing is open any more). The one that slid into the closed one's place, or the one
/// before it when the last was closed; a closed document that was not shown leaves the shown one
/// as it is (its index moves down by one when the closed one stood before it). An index past the
/// list closes nothing: the shown one stays, held inside the list.
#[must_use]
pub fn active_after_close(len_before: usize, active: usize, closed: usize) -> Option<usize> {
    if len_before == 0 {
        return None;
    }
    if closed >= len_before {
        return Some(active.min(len_before - 1));
    }
    // the index of the last document of the shortened list; none when the only document was closed
    let last = len_before.checked_sub(2)?;
    if closed < active {
        Some((active - 1).min(last))
    } else {
        // the closed one was the shown one (the next slides into its place, the last falls back to
        // its predecessor) or stood after it (the shown one keeps its index)
        Some(active.min(last))
    }
}

/// The index at which a new document is opened: right after the shown one, so the list reads in
/// the order the person worked in; 0 for an empty list.
#[must_use]
pub const fn insert_at(len: usize, active: usize) -> usize {
    if len == 0 {
        0
    } else if active + 1 < len {
        active + 1
    } else {
        len
    }
}

#[cfg(test)]
#[path = "docs_test.rs"]
mod docs_test;
