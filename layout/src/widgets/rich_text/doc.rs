//! The rich-text document: a FLAT list of blocks, each a list of styled runs.
//!
//! Flat on purpose (AzNotes' model, promoted). A list item is a block with an
//! indent level, a quoted paragraph is a block with a quote depth: the editor
//! renders ONE child of the editing host per block, so a block index IS the
//! host's child index - the vocabulary the engine's edit paths
//! (`get_node_child_index_path`, a split's resume point) speak. A run renders
//! as ONE child of its block, so a run index is a block's child index too.
//!
//! The union of the three editors this replaces (scripts/DEDUP_EDITORS A1/A2):
//! AzNotes' five formats, nine block kinds, nested lists and check items;
//! AzWriter's alignment, page breaks and tables; AzMail's quote depth.
//!
//! Every type here crosses the FFI (`repr(C)`, the vectors through
//! `impl_vec!`). The edits work on a block's runs as a plain `Vec<RichRun>`
//! (`RichBlock::runs_vec` / `set_runs`) and are plain data: the unit tests
//! run without a window.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use core::ops::Range;

use azul_css::{
    impl_option, impl_option_inner, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_eq,
    impl_vec_mut, impl_vec_partialeq, AzString, OptionString, StringVec,
};

// ==== Formats ====

/// An inline format a run can carry (a toolbar toggle, Ctrl/Cmd+B / I / U).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RichFormat {
    Bold,
    Italic,
    Underline,
    /// Strikethrough.
    Strike,
    /// Inline code (monospace).
    Code,
}

impl RichFormat {
    /// Every format, in the order a toolbar shows them.
    pub const ALL: [RichFormat; 5] = [
        RichFormat::Bold,
        RichFormat::Italic,
        RichFormat::Underline,
        RichFormat::Strike,
        RichFormat::Code,
    ];
}

/// The set of formats one run carries (or text typed at a caret takes).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct RichFormats {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub code: bool,
}

impl RichFormats {
    /// No format at all.
    #[must_use]
    pub const fn create() -> Self {
        Self {
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            code: false,
        }
    }

    /// Whether `format` is on.
    #[must_use]
    pub const fn has(&self, format: RichFormat) -> bool {
        match format {
            RichFormat::Bold => self.bold,
            RichFormat::Italic => self.italic,
            RichFormat::Underline => self.underline,
            RichFormat::Strike => self.strike,
            RichFormat::Code => self.code,
        }
    }

    /// Turns `format` on or off.
    pub fn set(&mut self, format: RichFormat, on: bool) {
        match format {
            RichFormat::Bold => self.bold = on,
            RichFormat::Italic => self.italic = on,
            RichFormat::Underline => self.underline = on,
            RichFormat::Strike => self.strike = on,
            RichFormat::Code => self.code = on,
        }
    }

    /// The set with `format` on (for building documents in code).
    #[must_use]
    pub fn with(mut self, format: RichFormat) -> Self {
        self.set(format, true);
        self
    }

    /// No format is on.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        !(self.bold || self.italic || self.underline || self.strike || self.code)
    }
}

impl_option!(
    RichFormats,
    OptionRichFormats,
    [Debug, Clone, PartialEq, Eq]
);

// ==== Runs ====

/// One stretch of text in one format (and, optionally, one link).
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RichRun {
    pub text: AzString,
    /// The run is (part of) a link to this URL.
    pub link: OptionString,
    pub formats: RichFormats,
}

impl RichRun {
    /// A plain run of `text`.
    #[must_use]
    pub fn create(text: AzString) -> Self {
        Self {
            text,
            link: OptionString::None,
            formats: RichFormats::create(),
        }
    }

    /// A plain run of `text` (Rust convenience).
    #[must_use]
    pub fn plain(text: &str) -> Self {
        Self::create(AzString::from(text))
    }

    /// The run with `format` on.
    #[must_use]
    pub fn with_format(mut self, format: RichFormat) -> Self {
        self.formats.set(format, true);
        self
    }

    /// The run with these formats.
    #[must_use]
    pub fn with_formats(mut self, formats: RichFormats) -> Self {
        self.formats = formats;
        self
    }

    /// The run as a link to `url`.
    #[must_use]
    pub fn with_link(mut self, url: AzString) -> Self {
        self.link = OptionString::Some(url);
        self
    }

    /// The run's text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.text.as_str()
    }

    /// The link's URL, if the run is a link.
    #[must_use]
    pub fn link_str(&self) -> Option<&str> {
        self.link.as_ref().map(AzString::as_str)
    }

    /// Whether `format` is on.
    #[must_use]
    pub const fn has(&self, format: RichFormat) -> bool {
        self.formats.has(format)
    }

    /// Turns `format` on or off.
    pub fn set(&mut self, format: RichFormat, on: bool) {
        self.formats.set(format, on);
    }

    /// Same formats and link (the text may differ).
    #[must_use]
    pub fn same_format(&self, other: &Self) -> bool {
        self.formats == other.formats && self.link_str() == other.link_str()
    }

    /// No format and no link: rendered as a bare text node.
    #[must_use]
    pub fn is_plain(&self) -> bool {
        self.formats.is_empty() && self.link.is_none()
    }

    /// Replaces the run's text.
    pub fn set_text(&mut self, text: String) {
        self.text = AzString::from(text);
    }

    /// A copy of the run (formats and link) holding `text`.
    #[must_use]
    pub fn with_text(&self, text: &str) -> Self {
        Self {
            text: AzString::from(text),
            link: self.link.clone(),
            formats: self.formats,
        }
    }
}

impl_option!(
    RichRun,
    OptionRichRun,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);
impl_vec!(
    RichRun,
    RichRunVec,
    RichRunVecDestructor,
    RichRunVecDestructorType,
    RichRunVecSlice,
    OptionRichRun
);
impl_vec_clone!(RichRun, RichRunVec, RichRunVecDestructor);
impl_vec_debug!(RichRun, RichRunVec);
impl_vec_partialeq!(RichRun, RichRunVec);
impl_vec_eq!(RichRun, RichRunVec);
impl_vec_mut!(RichRun, RichRunVec);

// ==== Block kinds ====

/// A checklist item: its indent level and whether it is ticked.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct RichCheck {
    pub indent: u8,
    pub checked: bool,
}

/// An image block: `src` (a path the app resolves, or a URL) and its text
/// alternative.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct RichImage {
    pub src: AzString,
    pub alt: AzString,
}

/// One row of a table block: its cells' plain text.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RichTableRow {
    pub cells: StringVec,
}

impl RichTableRow {
    /// A row of `cells`.
    #[must_use]
    pub fn create(cells: StringVec) -> Self {
        Self { cells }
    }

    /// A row of `count` empty cells.
    #[must_use]
    pub fn empty(count: usize) -> Self {
        let cells: Vec<AzString> = (0..count).map(|_| AzString::from_const_str("")).collect();
        Self {
            cells: StringVec::from_vec(cells),
        }
    }

    /// The text of cell `index` ("" past the end).
    #[must_use]
    pub fn cell(&self, index: usize) -> &str {
        self.cells.as_ref().get(index).map_or("", AzString::as_str)
    }
}

impl_option!(
    RichTableRow,
    OptionRichTableRow,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);
impl_vec!(
    RichTableRow,
    RichTableRowVec,
    RichTableRowVecDestructor,
    RichTableRowVecDestructorType,
    RichTableRowVecSlice,
    OptionRichTableRow
);
impl_vec_clone!(RichTableRow, RichTableRowVec, RichTableRowVecDestructor);
impl_vec_debug!(RichTableRow, RichTableRowVec);
impl_vec_partialeq!(RichTableRow, RichTableRowVec);
impl_vec_eq!(RichTableRow, RichTableRowVec);
impl_vec_mut!(RichTableRow, RichTableRowVec);

/// A table block: rows of plain-text cells (AzWriter's tables).
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RichTable {
    pub rows: RichTableRowVec,
    /// The first row is the header row.
    pub has_header: bool,
}

impl RichTable {
    /// A table of `rows`.
    #[must_use]
    pub fn create(rows: RichTableRowVec, has_header: bool) -> Self {
        Self { rows, has_header }
    }

    /// An empty `rows` x `columns` table.
    #[must_use]
    pub fn empty(rows: usize, columns: usize) -> Self {
        let rows: Vec<RichTableRow> = (0..rows).map(|_| RichTableRow::empty(columns)).collect();
        Self {
            rows: RichTableRowVec::from_vec(rows),
            has_header: false,
        }
    }

    /// The widest row's cell count.
    #[must_use]
    pub fn columns(&self) -> usize {
        self.rows
            .as_ref()
            .iter()
            .map(|r| r.cells.as_ref().len())
            .max()
            .unwrap_or(0)
    }
}

/// What a block is. A quoted block is any kind with a `quote_depth`
/// ([`RichBlock::quote_depth`]), not a kind of its own.
#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RichBlockKind {
    Paragraph,
    /// `#` .. `######` (1..=6).
    Heading(u8),
    /// A bulleted list item at an indent level (0 = top).
    Bullet(u8),
    /// A numbered list item at an indent level; its number is its place
    /// among the numbered items of its level ([`RichTextDoc::number_of`]).
    Numbered(u8),
    /// A checklist item.
    Check(RichCheck),
    /// A code block; its one run holds the code, lines split by `\n`. The
    /// payload is its language ("" for none).
    Code(AzString),
    /// A horizontal rule (no text).
    Rule,
    /// An image (no text).
    Image(RichImage),
    /// A page break (no text): the next block starts a new page.
    PageBreak,
    /// A table (its text lives in its cells, not in runs).
    Table(RichTable),
}

impl Default for RichBlockKind {
    fn default() -> Self {
        Self::Paragraph
    }
}

impl RichBlockKind {
    /// A list item (bullet, numbered, check).
    #[must_use]
    pub const fn is_list(&self) -> bool {
        matches!(
            self,
            Self::Bullet(_) | Self::Numbered(_) | Self::Check(_)
        )
    }

    /// The indent level of a list item; 0 for every other block.
    #[must_use]
    pub const fn indent(&self) -> u8 {
        match self {
            Self::Bullet(i) | Self::Numbered(i) => *i,
            Self::Check(c) => c.indent,
            _ => 0,
        }
    }

    /// The same kind at another indent level (no-op for a non-list block).
    #[must_use]
    pub fn with_indent(&self, indent: u8) -> Self {
        match self {
            Self::Bullet(_) => Self::Bullet(indent),
            Self::Numbered(_) => Self::Numbered(indent),
            Self::Check(c) => Self::Check(RichCheck {
                indent,
                checked: c.checked,
            }),
            other => other.clone(),
        }
    }

    /// The block holds runs of text (everything but a rule, an image, a
    /// page break and a table).
    #[must_use]
    pub const fn has_text(&self) -> bool {
        !matches!(
            self,
            Self::Rule | Self::Image(_) | Self::PageBreak | Self::Table(_)
        )
    }

    /// A code block.
    #[must_use]
    pub const fn is_code(&self) -> bool {
        matches!(self, Self::Code(_))
    }

    /// Two kinds of the same family for a toolbar toggle (indent, check
    /// state and code language aside; the heading level is part of the
    /// button).
    #[must_use]
    pub fn same_family(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Heading(a), Self::Heading(b)) => a == b,
            (Self::Bullet(_), Self::Bullet(_))
            | (Self::Numbered(_), Self::Numbered(_))
            | (Self::Check(_), Self::Check(_))
            | (Self::Code(_), Self::Code(_))
            | (Self::Image(_), Self::Image(_))
            | (Self::Table(_), Self::Table(_)) => true,
            (a, b) => a == b,
        }
    }

    /// The kind the next block takes when Enter splits a block of this kind
    /// at its END (a heading continues as a paragraph, a list as a list, a
    /// check item unchecked).
    #[must_use]
    pub fn continuation(&self) -> Self {
        match self {
            Self::Heading(_) | Self::Rule | Self::Image(_) | Self::PageBreak | Self::Table(_) => {
                Self::Paragraph
            }
            Self::Check(c) => Self::Check(RichCheck {
                indent: c.indent,
                checked: false,
            }),
            other => other.clone(),
        }
    }
}

/// How a block's lines are aligned (AzWriter's paragraph alignment).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RichAlign {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

impl RichAlign {
    /// The CSS `text-align` keyword.
    #[must_use]
    pub const fn css(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
            Self::Justify => "justify",
        }
    }
}

// ==== Blocks ====

/// One block: its kind, its runs, how deep it is quoted and its alignment.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RichBlock {
    pub kind: RichBlockKind,
    pub runs: RichRunVec,
    /// 0 = the writer's own text; 1 = quoted once (`> `), 2 = a quote in a
    /// quote (a reply to a reply), ...
    pub quote_depth: u8,
    pub align: RichAlign,
}

impl RichBlock {
    /// A block of `kind` holding `runs`.
    #[must_use]
    pub fn create(kind: RichBlockKind, runs: RichRunVec) -> Self {
        Self {
            kind,
            runs,
            quote_depth: 0,
            align: RichAlign::Left,
        }
    }

    /// A block of `kind` holding `runs` (Rust convenience).
    #[must_use]
    pub fn new(kind: RichBlockKind, runs: Vec<RichRun>) -> Self {
        Self::create(kind, RichRunVec::from_vec(runs))
    }

    /// A paragraph of plain `text`.
    #[must_use]
    pub fn paragraph(text: &str) -> Self {
        Self::text(RichBlockKind::Paragraph, text)
    }

    /// A block of `kind` holding plain `text` (no run when empty).
    #[must_use]
    pub fn text(kind: RichBlockKind, text: &str) -> Self {
        let runs = if text.is_empty() {
            Vec::new()
        } else {
            alloc::vec![RichRun::plain(text)]
        };
        Self::new(kind, runs)
    }

    /// The block quoted `depth` levels deep.
    #[must_use]
    pub fn with_quote_depth(mut self, depth: u8) -> Self {
        self.quote_depth = depth;
        self
    }

    /// The block aligned `align`.
    #[must_use]
    pub fn with_align(mut self, align: RichAlign) -> Self {
        self.align = align;
        self
    }

    /// The block's text, every run in order.
    #[must_use]
    pub fn flat(&self) -> String {
        flatten(self.runs.as_ref())
    }

    /// The block's runs as a plain vector (a copy).
    #[must_use]
    pub fn runs_vec(&self) -> Vec<RichRun> {
        self.runs.as_ref().to_vec()
    }

    /// Replaces the block's runs.
    pub fn set_runs(&mut self, runs: Vec<RichRun>) {
        self.runs = RichRunVec::from_vec(runs);
    }

    /// No text at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.runs.as_ref().iter().all(|r| r.text.as_str().is_empty())
    }

    /// The length of the block's text in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.runs.as_ref().iter().map(|r| r.text.as_str().len()).sum()
    }

    /// The table of a table block.
    #[must_use]
    pub const fn table(&self) -> Option<&RichTable> {
        match &self.kind {
            RichBlockKind::Table(t) => Some(t),
            _ => None,
        }
    }
}

impl_option!(
    RichBlock,
    OptionRichBlock,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);
impl_vec!(
    RichBlock,
    RichBlockVec,
    RichBlockVecDestructor,
    RichBlockVecDestructorType,
    RichBlockVecSlice,
    OptionRichBlock
);
impl_vec_clone!(RichBlock, RichBlockVec, RichBlockVecDestructor);
impl_vec_debug!(RichBlock, RichBlockVec);
impl_vec_partialeq!(RichBlock, RichBlockVec);
impl_vec_eq!(RichBlock, RichBlockVec);
impl_vec_mut!(RichBlock, RichBlockVec);

// ==== The document ====

/// A rich-text document: its blocks, in order. Never empty once normalized
/// (one empty paragraph is the caret's anchor in a new document).
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichTextDoc {
    pub blocks: RichBlockVec,
}

impl Default for RichTextDoc {
    fn default() -> Self {
        Self::create()
    }
}

impl_option!(
    RichTextDoc,
    OptionRichTextDoc,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);
impl_vec!(
    RichTextDoc,
    RichTextDocVec,
    RichTextDocVecDestructor,
    RichTextDocVecDestructorType,
    RichTextDocVecSlice,
    OptionRichTextDoc
);
impl_vec_clone!(RichTextDoc, RichTextDocVec, RichTextDocVecDestructor);
impl_vec_debug!(RichTextDoc, RichTextDocVec);
impl_vec_partialeq!(RichTextDoc, RichTextDocVec);
impl_vec_eq!(RichTextDoc, RichTextDocVec);
impl_vec_mut!(RichTextDoc, RichTextDocVec);

// ==== Run helpers (one copy: AzNotes' doc.rs and AzWriter's ir.rs had twins) ====

/// The concatenated text of `runs`.
#[must_use]
pub fn flatten(runs: &[RichRun]) -> String {
    runs.iter().map(|r| r.text.as_str()).collect()
}

/// Appends `run`, merging it into the last run when the formats agree; an
/// empty run is dropped.
pub fn push_run(runs: &mut Vec<RichRun>, run: RichRun) {
    if run.text.as_str().is_empty() {
        return;
    }
    if let Some(last) = runs.last_mut() {
        if last.same_format(&run) {
            let mut text = last.text.as_str().to_string();
            text.push_str(run.text.as_str());
            last.set_text(text);
            return;
        }
    }
    runs.push(run);
}

/// Merges neighbours of the same format and drops empty runs.
pub fn normalize_runs(runs: &mut Vec<RichRun>) {
    let old = core::mem::take(runs);
    for run in old {
        push_run(runs, run);
    }
}

/// The largest char boundary of `s` at or below `i`.
#[must_use]
pub fn floor_char_boundary(s: &str, i: usize) -> usize {
    let mut i = i.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Splits `runs` so a run boundary falls at byte `at` of their text (a cut
/// inside a character moves down to its start).
fn split_at_byte(runs: &mut Vec<RichRun>, at: usize) {
    let mut acc = 0usize;
    for i in 0..runs.len() {
        let len = runs[i].text.as_str().len();
        if at > acc && at < acc + len {
            let text = runs[i].text.as_str().to_string();
            let cut = floor_char_boundary(&text, at - acc);
            if cut > 0 && cut < len {
                let tail = runs[i].with_text(&text[cut..]);
                runs[i].set_text(text[..cut].to_string());
                runs.insert(i + 1, tail);
            }
            return;
        }
        acc += len;
    }
}

/// Splits `runs` at `start` and `end` and returns the index range of the
/// runs that hold exactly the bytes `start..end`.
pub fn isolate(runs: &mut Vec<RichRun>, start: usize, end: usize) -> Range<usize> {
    split_at_byte(runs, end);
    split_at_byte(runs, start);
    let mut acc = 0usize;
    let mut first = runs.len();
    let mut last = runs.len();
    for (i, run) in runs.iter().enumerate() {
        if acc >= start && first == runs.len() {
            first = i;
        }
        if acc >= end {
            last = i;
            break;
        }
        acc += run.text.as_str().len();
    }
    first..last.max(first)
}

/// Cuts `runs` at byte `at`: `runs` keeps the head, the tail is returned.
pub fn split_runs(runs: &mut Vec<RichRun>, at: usize) -> Vec<RichRun> {
    split_at_byte(runs, at);
    let mut acc = 0usize;
    let mut cut = runs.len();
    for (i, run) in runs.iter().enumerate() {
        if acc >= at {
            cut = i;
            break;
        }
        acc += run.text.as_str().len();
    }
    runs.split_off(cut)
}

/// The runs of `runs` that cover bytes `start..end`, cut to them.
#[must_use]
pub fn slice_runs(runs: &[RichRun], start: usize, end: usize) -> Vec<RichRun> {
    let mut copy = runs.to_vec();
    let range = isolate(&mut copy, start, end);
    copy[range].to_vec()
}

/// Where an edit of a block's text happened: `old[..prefix]` and
/// `old[old.len() - suffix..]` are unchanged, the middle was replaced by
/// `new[prefix..new.len() - suffix]`.
#[must_use]
pub fn text_diff(old: &str, new: &str) -> (usize, usize) {
    let mut prefix = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    prefix = floor_char_boundary(old, prefix.min(new.len()));
    while prefix > 0 && !new.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let mut suffix = old
        .bytes()
        .rev()
        .zip(new.bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    suffix = suffix.min(old.len() - prefix).min(new.len() - prefix);
    while suffix > 0
        && (!old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix))
    {
        suffix -= 1;
    }
    (prefix, suffix)
}

/// `text` cut to `max` characters, with an ellipsis when cut.
#[must_use]
pub fn truncate_chars(text: &str, max: usize) -> String {
    let mut out: String = text.chars().take(max).collect();
    if text.chars().count() > max {
        out.push('\u{2026}');
    }
    out
}

// ==== Markdown shortcuts typed at a block's start ====

/// What a Markdown shortcut typed at the start of a block turns it into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichShortcut {
    /// The new kind, or `None` for the quote shortcut (`> `), which
    /// deepens the block's quote instead.
    pub kind: Option<RichBlockKind>,
    /// The bytes of the trigger to remove from the block's start.
    pub strip: usize,
}

/// The shortcut whose trigger `text` starts with, for a block of `kind`:
/// `# ` / `## ` / `### `, `- ` / `* ` / `+ `, `1. ` (any number), `[ ] ` /
/// `[x] ` (also in a bullet, so `- [ ] ` typed in a row ends a check item),
/// `> `, and a block that is exactly ` ``` `.
#[must_use]
pub fn shortcut_in(kind: &RichBlockKind, text: &str) -> Option<RichShortcut> {
    let shortcut = |kind: RichBlockKind, strip: usize| {
        Some(RichShortcut {
            kind: Some(kind),
            strip,
        })
    };
    let check = |indent: u8, checked: bool| RichBlockKind::Check(RichCheck { indent, checked });
    match kind {
        RichBlockKind::Paragraph => {}
        RichBlockKind::Bullet(indent) => {
            return if text.starts_with("[ ] ") {
                shortcut(check(*indent, false), 4)
            } else if text.starts_with("[x] ") || text.starts_with("[X] ") {
                shortcut(check(*indent, true), 4)
            } else {
                None
            };
        }
        _ => return None,
    }
    for level in (1..=3u8).rev() {
        let mut trigger = "#".repeat(level as usize);
        trigger.push(' ');
        if text.starts_with(&trigger) {
            return shortcut(RichBlockKind::Heading(level), trigger.len());
        }
    }
    if text.starts_with("- ") || text.starts_with("* ") || text.starts_with("+ ") {
        return shortcut(RichBlockKind::Bullet(0), 2);
    }
    if text.starts_with("[ ] ") || text.starts_with("[] ") {
        let strip = if text.starts_with("[] ") { 3 } else { 4 };
        return shortcut(check(0, false), strip);
    }
    if text.starts_with("[x] ") || text.starts_with("[X] ") {
        return shortcut(check(0, true), 4);
    }
    if text.starts_with("> ") {
        return Some(RichShortcut {
            kind: None,
            strip: 2,
        });
    }
    if text == "```" {
        return shortcut(RichBlockKind::Code(AzString::from_const_str("")), 3);
    }
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    if (1..=9).contains(&digits) && text[digits..].starts_with(". ") {
        return shortcut(RichBlockKind::Numbered(0), digits + 2);
    }
    None
}

/// The shortcut an EDIT typed: `new` matches a trigger that `old` did not.
/// Text that already started with a trigger (a paragraph `# not a heading`
/// loaded from a file, `\#` in Markdown) is never converted by an edit
/// further on.
#[must_use]
pub fn typed_shortcut(kind: &RichBlockKind, old: &str, new: &str) -> Option<RichShortcut> {
    let now = shortcut_in(kind, new)?;
    match shortcut_in(kind, old) {
        Some(before) if before == now => None,
        _ => Some(now),
    }
}

/// A block of a paste (a multi-block replacement): its kind when it names
/// one, its quote depth and its runs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PastedBlock {
    pub kind: Option<RichBlockKind>,
    pub quote_depth: u8,
    pub runs: Vec<RichRun>,
}

// RTE-DOC-EDITS: the document's edits follow (next commit).
