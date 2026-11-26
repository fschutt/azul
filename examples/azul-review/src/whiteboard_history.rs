//! The history of a board: snapshots, the save points that say how the architecture changed and, above all, WHY. A board
//! is not one picture but a chain of them tied to the code's revisions; the reasoning is the first-class content.
//!
//! A [`Snapshot`] is immutable once it is written: it has an `id`, its `parent` (the one before it: a chain, like commits),
//! when it was taken and by whom (the git name as configured, nothing else personal), the [`Code`] it was taken at (the
//! repository, the full revision, the branch, whether the tree had changes), the board's state (every item, with the
//! references of its cutouts), and what explains it: the `why` (free text), `tags` (decision ids, issue and PR links the
//! person types), a [`Summary`] (what changed since the parent, worked out and never typed), optionally the [`Evidence`]
//! (a recording and the transcript of what was said then). Its text is written once (`value`) and the SHA-256 of its
//! canonical form (RFC 8785 JSON) is kept with it, so that a published history can be checked: a snapshot that does not hash to its hash
//! was changed after it was written ([`Snapshot::verified`]). History is append-only: nothing is rewritten, restoring an old
//! state is a new snapshot.
//!
//! This module is the model and the comparison of two states ([`diff`]); the text is `whiteboard_azboard`'s. Pure: no
//! window, no azul.

use std::{collections::BTreeMap, sync::Arc};

use crate::{
    whiteboard::{BoardData, CutoutImage, Item, ItemId},
    whiteboard_geom::Pt,
    whiteboard_git::{Host, RepoFacts},
    whiteboard_json::Json,
    whiteboard_relocate::RefStatus,
};

/// The most snapshots a board keeps.
pub const MAX_SNAPSHOTS: usize = 200;
/// The most items all the snapshots of a board hold together (a snapshot holds a copy of every item of the board).
pub const MAX_HISTORY_ITEMS: usize = 20_000;
/// The most text all the snapshots of a board are written as together, bytes: the board's file holds them in its manifest,
/// and a manifest is read up to [`crate::whiteboard_json::MAX_TEXT`] (16 MiB), the rest of the board taking what is left.
pub const MAX_HISTORY_BYTES: usize = 12 * 1024 * 1024;
/// The longest a reason is, characters.
pub const MAX_WHY_CHARS: usize = 20_000;
/// The most tags a snapshot has, and the longest one, characters.
pub const MAX_TAGS: usize = 24;
pub const MAX_TAG_CHARS: usize = 100;
/// The longest transcript that is kept as evidence, characters.
pub const MAX_EVIDENCE_CHARS: usize = 20_000;
/// A position that differs by less than this, px, is the same position.
const MOVE_EPS: f32 = 0.5;

/// Where the code was when a snapshot was taken.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Code {
    /// The full hash of HEAD.
    pub rev: Option<String>,
    pub branch: Option<String>,
    /// Whether the working tree had changes that were not committed (known when git was asked).
    pub dirty: Option<bool>,
    /// When the revision was committed, Unix milliseconds (known when git was asked).
    pub rev_time_ms: Option<u64>,
    pub remote: Option<String>,
    pub web_url: Option<String>,
    pub host: Host,
    /// Where the workspace's folder lies inside the repository.
    pub prefix: String,
}

impl Code {
    /// The code as the repository facts say it is.
    #[must_use]
    pub fn of(facts: &RepoFacts) -> Self {
        Self {
            rev: facts.rev.clone(),
            branch: facts.branch.clone(),
            dirty: facts.dirty,
            rev_time_ms: None,
            remote: facts.remote_url.clone(),
            web_url: facts.web_url.clone(),
            host: facts.host,
            prefix: facts.prefix.clone(),
        }
    }
}

/// What was said while the board was made, kept with the reason for it: the recording's id, when in it (milliseconds from
/// its start) and the words.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Evidence {
    pub session: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub transcript: String,
}

/// An item in a summary: its id, what it is and what to call it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Brief {
    pub id: ItemId,
    /// `cutout` or `arrow`.
    pub kind: &'static str,
    /// A cutout's label, else its file and lines; an arrow's label, else its two ends.
    pub name: String,
}

/// An item that moved.
#[derive(Debug, Clone, PartialEq)]
pub struct Moved {
    pub id: ItemId,
    pub from: Pt,
    pub to: Pt,
}

/// An item whose kind and place are as they were and something else is not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changed {
    pub id: ItemId,
    /// What differs: `label`, `size`, `reference` (the code a cutout points at: its file, lines or what they say), `ends`,
    /// `bend`.
    pub what: Vec<&'static str>,
}

/// What became of the code a cutout points at, when it was looked for again at a newer revision
/// ([`crate::whiteboard_relocate`]): only the references that are not as they were.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefChange {
    pub id: ItemId,
    pub status: RefStatus,
    /// The lines it pointed at, and the lines it points at now (the same for one that vanished).
    pub from: (u32, u32),
    pub to: (u32, u32),
}

/// What changed since the parent, worked out from the two states and nothing else: the same two states always give the
/// same summary. (`references` is what looking for the code again found: a move, a change or a loss is where the reason for
/// a snapshot is usually needed.)
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Summary {
    pub added: Vec<Brief>,
    pub removed: Vec<Brief>,
    pub moved: Vec<Moved>,
    pub changed: Vec<Changed>,
    pub references: Vec<RefChange>,
}

impl Summary {
    /// Whether nothing changed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.moved.is_empty() && self.changed.is_empty() && self.references.is_empty()
    }

    /// `(added, removed, moved, changed)`.
    #[must_use]
    pub fn counts(&self) -> (usize, usize, usize, usize) {
        (self.added.len(), self.removed.len(), self.moved.len(), self.changed.len())
    }
}

/// What to call a cutout: its label's first line, else its file and lines.
fn cutout_name(c: &crate::whiteboard::Cutout) -> String {
    if c.label.trim().is_empty() {
        format!("{}:{}-{}", c.source.file, c.source.first_line, c.source.last_line)
    } else {
        c.label.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default().to_string()
    }
}

/// What to call the end of an arrow: the cutout it is fastened to, or `...` for a loose end.
fn end_name(data: &BoardData, end: &crate::whiteboard::End) -> String {
    match end {
        crate::whiteboard::End::Free(_) => "...".to_string(),
        crate::whiteboard::End::Item { id, .. } => match data.items.iter().find(|i| i.id() == *id) {
            Some(Item::Cutout(c)) => cutout_name(c),
            _ => format!("#{id}"),
        },
    }
}

/// What to call an item: a cutout by its label or else its file and lines, an arrow by its label or else its two ends.
fn name_of(data: &BoardData, item: &Item) -> String {
    match item {
        Item::Cutout(c) => cutout_name(c),
        Item::Arrow(a) => {
            if a.label.trim().is_empty() {
                format!("{} -> {}", end_name(data, &a.from), end_name(data, &a.to))
            } else {
                a.label.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default().to_string()
            }
        }
    }
}

fn brief(data: &BoardData, item: &Item) -> Brief {
    Brief { id: item.id(), kind: if matches!(item, Item::Cutout(_)) { "cutout" } else { "arrow" }, name: name_of(data, item) }
}

fn differ(a: f32, b: f32) -> bool {
    (a - b).abs() >= MOVE_EPS
}

/// What changed from `before` (none: the first snapshot, where everything is new) to `after`, by item id, in id order.
#[must_use]
pub fn diff(before: Option<&BoardData>, after: &BoardData) -> Summary {
    let empty = BoardData::default();
    let before = before.unwrap_or(&empty);
    let old: BTreeMap<ItemId, &Item> = before.items.iter().map(|i| (i.id(), i)).collect();
    let new: BTreeMap<ItemId, &Item> = after.items.iter().map(|i| (i.id(), i)).collect();
    let mut s = Summary::default();
    for (id, item) in &new {
        let Some(was) = old.get(id) else {
            s.added.push(brief(after, item));
            continue;
        };
        match (was, item) {
            (Item::Cutout(a), Item::Cutout(b)) => {
                if differ(a.rect.x, b.rect.x) || differ(a.rect.y, b.rect.y) {
                    s.moved.push(Moved { id: *id, from: Pt::new(a.rect.x, a.rect.y), to: Pt::new(b.rect.x, b.rect.y) });
                }
                let mut what = Vec::new();
                if a.label != b.label {
                    what.push("label");
                }
                if differ(a.rect.w, b.rect.w) || differ(a.rect.h, b.rect.h) {
                    what.push("size");
                }
                // The code a cutout points at: its file, its lines and what they say. The revision it names is not the code: the
                // same lines at a newer revision are the same reference.
                let (x, y) = (&a.source, &b.source);
                if (x.file.as_str(), x.first_line, x.last_line, x.sha256.as_str()) != (y.file.as_str(), y.first_line, y.last_line, y.sha256.as_str()) {
                    what.push("reference");
                }
                if !what.is_empty() {
                    s.changed.push(Changed { id: *id, what });
                }
            }
            (Item::Arrow(a), Item::Arrow(b)) => {
                let mut what = Vec::new();
                if a.label != b.label {
                    what.push("label");
                }
                if a.from != b.from || a.to != b.to {
                    what.push("ends");
                }
                if (a.bend - b.bend).abs() >= 0.01 {
                    what.push("bend");
                }
                if !what.is_empty() {
                    s.changed.push(Changed { id: *id, what });
                }
            }
            // An id is never given twice on a board, so a kind that changed under one id is a document that was made by hand.
            _ => {
                s.removed.push(brief(before, was));
                s.added.push(brief(after, item));
            }
        }
    }
    for (id, item) in &old {
        if !new.contains_key(id) {
            s.removed.push(brief(before, item));
        }
    }
    s.removed.sort_by_key(|b| b.id);
    s.added.sort_by_key(|b| b.id);
    s
}

/// A save point of a board.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// `s1`, `s2`, ...: never given twice on a board.
    pub id: String,
    /// The snapshot this one follows (none for the first).
    pub parent: Option<String>,
    pub taken_ms: u64,
    pub author: Option<String>,
    pub code: Code,
    /// Why the architecture is shaped as it is now, or why it changed since the parent. The most important field.
    pub why: String,
    pub tags: Vec<String>,
    pub summary: Summary,
    pub evidence: Option<Evidence>,
    /// The board as it was.
    pub data: BoardData,
    /// The snapshot as it was written, without its hash: made once and written again as it is, never made again from the
    /// model (what is derived - a link, a language - stays what it was when the snapshot was taken).
    pub value: Json,
    /// The snapshot's object with its hash, written for a document as it lies in the list of snapshots (made once, so that a
    /// save of a board that has a long history copies text and does not write it again); what the history's size is counted by.
    pub text: String,
    /// `sha256:` and the hash of [`Self::value`] in RFC 8785's canonical form ([`crate::whiteboard_json::canonical`]); what
    /// was read for a snapshot read from a file.
    pub hash: String,
    /// The hash is the hash of the value. `false` for a snapshot that was changed after it was written, or has no hash.
    pub verified: bool,
    /// The pictures the text names, by the key it names them with.
    pub pictures: BTreeMap<String, Arc<CutoutImage>>,
}

impl Snapshot {
    /// The first line of the reason, for a list (empty for none).
    #[must_use]
    pub fn headline(&self) -> String {
        self.why.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default().to_string()
    }
}

/// The history of a board: its snapshots, oldest first.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct History {
    pub snapshots: Vec<Snapshot>,
}

impl History {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Snapshot> {
        self.snapshots.iter().find(|s| s.id == id)
    }

    /// The snapshot before `id` in the chain (its parent, when that one is here).
    #[must_use]
    pub fn parent_of(&self, id: &str) -> Option<&Snapshot> {
        self.get(id).and_then(|s| s.parent.as_deref()).and_then(|p| self.get(p))
    }

    /// The newest snapshot.
    #[must_use]
    pub fn latest(&self) -> Option<&Snapshot> {
        self.snapshots.last()
    }

    /// The id the next snapshot takes: past every one there is (`s7` after `s6`; ids that are not `s<number>` count as 0).
    #[must_use]
    pub fn next_id(&self) -> String {
        let n = self.snapshots.iter().filter_map(|s| s.id.strip_prefix('s').and_then(|n| n.parse::<u64>().ok())).max().unwrap_or(0);
        format!("s{}", n + 1)
    }

    /// How many items the snapshots hold together.
    #[must_use]
    pub fn items_held(&self) -> usize {
        self.snapshots.iter().map(|s| s.data.items.len()).sum()
    }

    /// How much text the snapshots are written as together, bytes.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.snapshots.iter().map(|s| s.text.len()).sum()
    }

    /// Whether another snapshot of a board of `items` items fits (by their number; its text is counted when it is written,
    /// [`Self::fits`]).
    #[must_use]
    pub fn has_room_for(&self, items: usize) -> bool {
        self.snapshots.len() < MAX_SNAPSHOTS && self.items_held() + items <= MAX_HISTORY_ITEMS
    }

    /// Whether `snapshots` more snapshots of `items` items in all and `bytes` bytes of text fit.
    #[must_use]
    pub fn fits(&self, snapshots: usize, items: usize, bytes: usize) -> bool {
        self.snapshots.len() + snapshots <= MAX_SNAPSHOTS && self.items_held() + items <= MAX_HISTORY_ITEMS && self.bytes() + bytes <= MAX_HISTORY_BYTES
    }
}

/// The tags a person typed ("ADR-7, #123 , https://x.org/pr/9"): split at commas and line ends, trimmed, empty ones and
/// repeats dropped, each cut to [`MAX_TAG_CHARS`] characters, at most [`MAX_TAGS`] of them.
#[must_use]
pub fn parse_tags(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in text.split(|c| c == ',' || c == '\n' || c == ';') {
        let tag: String = part.trim().chars().filter(|c| !c.is_control()).take(MAX_TAG_CHARS).collect();
        if !tag.is_empty() && !out.contains(&tag) {
            out.push(tag);
        }
        if out.len() >= MAX_TAGS {
            break;
        }
    }
    out
}

/// A reason cut to [`MAX_WHY_CHARS`] characters, without control characters other than line feeds and tabs, the ends
/// trimmed.
#[must_use]
pub fn clean_why(text: &str) -> String {
    text.chars().filter(|c| !c.is_control() || matches!(c, '\n' | '\t')).take(MAX_WHY_CHARS).collect::<String>().trim().to_string()
}

/// A time as `2026-11-26 05:12` (UTC: the time zone of the person's machine is not known here), from Unix milliseconds.
#[must_use]
pub fn date_utc(ms: u64) -> String {
    let secs = ms / 1000;
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // The civil date of a day count (days since 1970-01-01): Howard Hinnant's algorithm.
    let z = i64::try_from(days).unwrap_or(0) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}", rest / 3_600, rest % 3_600 / 60)
}

/// The first seven characters of a revision, for a list (empty for none).
#[must_use]
pub fn short_rev(rev: Option<&str>) -> String {
    rev.map(|r| r.chars().take(7).collect()).unwrap_or_default()
}

#[cfg(test)]
#[path = "whiteboard_history_test.rs"]
mod whiteboard_history_test;
