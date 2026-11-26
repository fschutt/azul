//! What has been looked at, and what follows from it.
//!
//! A file is REVIEWED when every page of it has been SEEN and every conversation about it is resolved; no key is pressed
//! for it. A page is seen when at least half of its height has been in view for a second and a half, all told (the time
//! of one visit and the next add up), or when it carries ink. The pages seen are kept per file as a set of pages (a bit
//! each: 2 KB for a file of 15,000 pages) and in the data root as ranges; a folder's totals are the sums over the files
//! under it (`rollup`), in lines, with the lines that were seen as the lighter number and the files whose lines are
//! not counted yet told apart.
//!
//! Pure: the time that passes is told to the model (`watch`), the tree and the files come in as arguments. No window, no
//! clock, no disk (`progress_test.rs`; the store is `progress_store.rs`).

use crate::{
    chips::Counts,
    code::LINES_PER_PAGE,
    tree::{Kind, NodeId, Tree},
};

/// How long a page must have been in view, ms, all told, to be seen.
pub const SEEN_DWELL_MS: u32 = 1_500;
/// How much of its height must be in view for the time to count.
pub const SEEN_FRACTION: f32 = 0.5;
/// The longest step of time that is counted as reading between two looks, ms: a window that was asleep or a machine that
/// stalled for an hour was not reading.
pub const MAX_STEP_MS: u32 = 400;
/// How many pages are kept partway (looked at, not yet for long enough): the oldest is forgotten.
pub const MAX_PENDING: usize = 32;
/// How many pages of a file are kept (about twelve million lines).
pub const MAX_PAGES: usize = 262_144;

/// The pages of one file that have been seen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Seen {
    /// A bit a page.
    bits: Vec<u64>,
    count: usize,
    /// Pages partway to seen: the page and the ms it has been in view.
    pending: Vec<(u32, u32)>,
}

impl Seen {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn is_seen(&self, page: usize) -> bool {
        page < MAX_PAGES && self.bits.get(page / 64).is_some_and(|w| w & (1 << (page % 64)) != 0)
    }

    /// How many pages are seen.
    #[must_use]
    pub const fn count(&self) -> usize {
        self.count
    }

    /// How many pages are partway.
    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Marks a page seen (it carries ink, or was loaded as seen); whether it was not before.
    pub fn mark(&mut self, page: usize) -> bool {
        if page >= MAX_PAGES || self.is_seen(page) {
            return false;
        }
        let word = page / 64;
        if self.bits.len() <= word {
            self.bits.resize(word + 1, 0);
        }
        self.bits[word] |= 1 << (page % 64);
        self.count += 1;
        self.pending.retain(|(p, _)| *p as usize != page);
        true
    }

    /// Marks the pages `first` to `last` seen (either way round; no page past what a file can have).
    pub fn mark_range(&mut self, first: usize, last: usize) {
        let (lo, hi) = (first.min(last), first.max(last).min(MAX_PAGES - 1));
        if lo > hi {
            return;
        }
        let mut page = lo;
        while page <= hi {
            if page % 64 == 0 && page + 63 <= hi {
                // a whole word at once
                let word = page / 64;
                if self.bits.len() <= word {
                    self.bits.resize(word + 1, 0);
                }
                self.count += (!self.bits[word]).count_ones() as usize;
                self.bits[word] = u64::MAX;
                page += 64;
            } else {
                self.mark(page);
                page += 1;
            }
        }
        self.pending.retain(|(p, _)| !(lo..=hi).contains(&(*p as usize)));
    }

    /// Marks the first `pages` pages seen.
    pub fn mark_all(&mut self, pages: usize) {
        if pages > 0 {
            self.mark_range(0, pages - 1);
        }
    }

    /// Forgets every page.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// `elapsed_ms` passed with these pages in view (`(page, the fraction of its height that is in view)`): the pages that
    /// are at least half in view add the time to what they had; those that have had [`SEEN_DWELL_MS`] are seen. Whether a
    /// page became seen.
    pub fn watch(&mut self, in_view: &[(usize, f32)], elapsed_ms: u32) -> bool {
        let step = elapsed_ms.min(MAX_STEP_MS);
        let mut changed = false;
        for &(page, fraction) in in_view {
            if !(fraction >= SEEN_FRACTION) || page >= MAX_PAGES || self.is_seen(page) {
                continue;
            }
            let at = u32::try_from(page).unwrap_or(u32::MAX);
            let total = if let Some(entry) = self.pending.iter_mut().find(|(p, _)| *p == at) {
                entry.1 = entry.1.saturating_add(step);
                entry.1
            } else {
                if self.pending.len() >= MAX_PENDING {
                    self.pending.remove(0);
                }
                self.pending.push((at, step));
                step
            };
            if total >= SEEN_DWELL_MS {
                changed |= self.mark(page);
            }
        }
        changed
    }

    /// The pages seen as ranges (first, last), in order.
    #[must_use]
    pub fn ranges(&self) -> Vec<(usize, usize)> {
        let mut out: Vec<(usize, usize)> = Vec::new();
        for (w, &word) in self.bits.iter().enumerate() {
            if word == 0 {
                continue;
            }
            let mut bit = 0;
            while bit < 64 {
                if word >> bit & 1 == 1 {
                    let start = w * 64 + bit;
                    let mut end = start;
                    bit += 1;
                    while bit < 64 && word >> bit & 1 == 1 {
                        end += 1;
                        bit += 1;
                    }
                    match out.last_mut() {
                        Some(last) if last.1 + 1 == start => last.1 = end,
                        _ => out.push((start, end)),
                    }
                } else {
                    bit += 1;
                }
            }
        }
        out
    }

    /// The pages of these ranges, as an archive says them: a range that runs backwards is the other way round, ranges that
    /// overlap are one, no page past [`MAX_PAGES`] is kept.
    #[must_use]
    pub fn from_ranges(ranges: &[(usize, usize)]) -> Self {
        let mut seen = Self::new();
        for &(first, last) in ranges {
            seen.mark_range(first, last);
        }
        seen
    }

    /// How many lines the pages seen hold, in a file of `total` lines (the last page holds what is left).
    #[must_use]
    pub fn lines_seen(&self, total: usize) -> usize {
        let mut lines = 0;
        for (first, last) in self.ranges() {
            let start = first * LINES_PER_PAGE;
            if start >= total {
                continue;
            }
            let last_page = last.min((total - 1) / LINES_PER_PAGE);
            let pages = last_page - first + 1;
            let end = (last_page + 1) * LINES_PER_PAGE;
            lines += pages * LINES_PER_PAGE - end.saturating_sub(total);
        }
        lines
    }
}

/// Whether a file is reviewed: `seen` of its `pages` pages are seen, its conversations are as `conversations` says, and the
/// reviewer has not said it is not (`not_reviewed`).
#[must_use]
pub fn is_reviewed(seen: usize, pages: usize, conversations: Counts, not_reviewed: bool) -> bool {
    !not_reviewed && pages > 0 && seen >= pages && conversations.working == 0 && conversations.suggested == 0
}

/// The pages in view of a strip of pages scrolled `scroll_x` px sideways in a viewport `view_w` px wide, the pages being
/// `page_w` wide and `stride` apart (the pages are laid out in a row, from the viewport's top, the strip filling its
/// height): each page that is in view with the part of its width that is. A window shorter than a page shows its top.
#[must_use]
pub fn pages_in_view(scroll_x: f32, view_w: f32, page_w: f32, stride: f32, total: usize) -> Vec<(usize, f32)> {
    if total == 0 || !(page_w > 0.0 && stride > 0.0 && page_w.is_finite() && stride.is_finite()) {
        return Vec::new();
    }
    let scroll_x = if scroll_x.is_finite() && scroll_x > 0.0 { scroll_x } else { 0.0 };
    let view_w = if view_w.is_finite() && view_w > 0.0 { view_w } else { 0.0 };
    let right = scroll_x + view_w;
    let first = (scroll_x / stride).floor();
    if first >= total as f32 {
        return Vec::new();
    }
    let first = first as usize;
    let last = ((right / stride).floor() as usize).min(total - 1);
    let mut out = Vec::new();
    for page in first..=last {
        let left_edge = page as f32 * stride;
        let overlap = (right.min(left_edge + page_w) - scroll_x.max(left_edge)).max(0.0);
        if overlap > 0.0 {
            out.push((page, (overlap / page_w).min(1.0)));
        }
    }
    out
}

/// How many files a book keeps (a workspace of a hundred thousand files).
pub const MAX_FILES: usize = 100_000;

/// What is known of one file's review.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    pub seen: Seen,
    /// The reviewer said this file is not reviewed: whatever the pages say, it is not until they say it is again.
    pub held: bool,
}

/// What is known of the review of every file of the workspace, by path: what is kept in the data root.
#[derive(Debug, Clone, Default)]
pub struct Book {
    files: std::collections::BTreeMap<String, Entry>,
    cap: usize,
    dirty: bool,
    /// Where the reviewer comes back to after the conversations they were taken to.
    back: Option<Back>,
}

/// The longest path of a place to come back to, bytes.
pub const MAX_BACK_PATH: usize = 4_096;

/// A place of the review: a page of a file, and the first line the page has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Back {
    pub file: String,
    /// The page, the first being 0.
    pub page: usize,
    /// The line of the file in view, the first being 1.
    pub line: u32,
}

impl Back {
    /// The place in a few words, for a key: the file, the page counted from one, the line (`src/lib.rs p3 l93`).
    #[must_use]
    pub fn words(&self) -> String {
        format!("{} p{} l{}", self.file, self.page + 1, self.line)
    }
}

impl Book {
    #[must_use]
    pub fn with_cap(cap: usize) -> Self {
        Self { files: std::collections::BTreeMap::new(), cap, dirty: false, back: None }
    }

    fn cap(&self) -> usize {
        if self.cap == 0 {
            MAX_FILES
        } else {
            self.cap
        }
    }

    #[must_use]
    pub fn get(&self, path: &str) -> Option<&Entry> {
        self.files.get(path)
    }

    /// The entry of `path`, made if it is new; `None` when the book is full.
    pub fn entry(&mut self, path: &str) -> Option<&mut Entry> {
        if !self.files.contains_key(path) {
            if self.files.len() >= self.cap() {
                return None;
            }
            self.files.insert(path.to_string(), Entry::default());
        }
        self.files.get_mut(path)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Every file and what is known of it, by path.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Entry)> {
        self.files.iter()
    }

    /// Takes what `other` knows (what was kept) into this book (what has been seen since): the pages of both, a file held back
    /// in either held back.
    pub fn merge(&mut self, other: Self) {
        for (path, entry) in other.files {
            let Some(mine) = self.entry(&path) else {
                continue;
            };
            for (first, last) in entry.seen.ranges() {
                mine.seen.mark_range(first, last);
            }
            mine.held |= entry.held;
        }
        // a place set since the file was read is the place; else the one that was kept
        if self.back.is_none() {
            self.back = other.back;
        }
    }

    /// Where the reviewer comes back to, when they were taken to a conversation.
    #[must_use]
    pub const fn back(&self) -> Option<&Back> {
        self.back.as_ref()
    }

    /// The reviewer jumped to a conversation from `here`: the first jump is the place they come back to, and a jump from
    /// one conversation to another keeps it. `true` when it was set (a place with no file, or a file name longer than a
    /// path can be, is none).
    pub fn jumped_from(&mut self, here: Back) -> bool {
        if self.back.is_some() || here.file.is_empty() || here.file.len() > MAX_BACK_PATH {
            return false;
        }
        self.back = Some(here);
        self.dirty = true;
        true
    }

    /// The reviewer comes back: the place, and none any more.
    pub fn come_back(&mut self) -> Option<Back> {
        let back = self.back.take();
        if back.is_some() {
            self.dirty = true;
        }
        back
    }

    /// Something changed that is not kept yet.
    pub fn set_dirty(&mut self) {
        self.dirty = true;
    }

    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Whether something changed since the last time it was asked; the answer is given once.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }
}

/// What a file adds to the totals above it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileInfo {
    /// How many lines it has (`None`: not counted yet).
    pub lines: Option<u32>,
    /// How many of them are on the pages seen.
    pub lines_seen: u32,
    pub reviewed: bool,
}

/// The totals of a file or of a folder: the sums over the files under it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Totals {
    pub files: u32,
    pub files_reviewed: u32,
    /// The lines of the files counted.
    pub lines: u64,
    /// The lines of the files that are reviewed.
    pub lines_reviewed: u64,
    /// The lines on the pages that were seen, in any file.
    pub lines_seen: u64,
    /// The files whose lines are not counted yet (they are in `files`, not in `lines`).
    pub unknown_files: u32,
}

impl Totals {
    /// How much of the lines are reviewed, in whole percent (0 where there are no lines).
    #[must_use]
    pub const fn percent_reviewed(&self) -> u64 {
        if self.lines == 0 {
            0
        } else {
            self.lines_reviewed * 100 / self.lines
        }
    }

    /// Some files have not been counted: the numbers are of the part that has.
    #[must_use]
    pub const fn is_partial(&self) -> bool {
        self.unknown_files > 0
    }

    fn add(&mut self, other: &Self) {
        self.files += other.files;
        self.files_reviewed += other.files_reviewed;
        self.lines += other.lines;
        self.lines_reviewed += other.lines_reviewed;
        self.lines_seen += other.lines_seen;
        self.unknown_files += other.unknown_files;
    }
}

/// The totals of every node of `tree`, indexed by node: a file's are its own, a folder's the sum over what is under it,
/// the root's those of the workspace. One pass: a node's children come after it.
#[must_use]
pub fn rollup(tree: &Tree, info: &dyn Fn(NodeId) -> FileInfo) -> Vec<Totals> {
    let mut totals = vec![Totals::default(); tree.node_count()];
    for id in (0..totals.len()).rev() {
        let Some(node) = tree.node(id) else {
            continue;
        };
        if node.kind == Kind::File {
            let f = info(id);
            totals[id] = Totals {
                files: 1,
                files_reviewed: u32::from(f.reviewed),
                lines: f.lines.map_or(0, u64::from),
                lines_reviewed: if f.reviewed { f.lines.map_or(0, u64::from) } else { 0 },
                lines_seen: u64::from(f.lines_seen),
                unknown_files: u32::from(f.lines.is_none()),
            };
        }
        if let Some(parent) = node.parent {
            let mine = totals[id];
            totals[parent].add(&mine);
        }
    }
    totals
}

#[cfg(test)]
#[path = "progress_test.rs"]
mod progress_test;
