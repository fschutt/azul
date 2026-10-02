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

// ==== The document's edits ====

impl RichTextDoc {
    /// One empty paragraph: the caret's anchor in a new document.
    #[must_use]
    pub fn create() -> Self {
        Self {
            blocks: RichBlockVec::from_vec(vec![RichBlock::paragraph("")]),
        }
    }

    /// `blocks`, normalized ([`Self::normalize`]).
    #[must_use]
    pub fn create_from_blocks(blocks: RichBlockVec) -> Self {
        let mut doc = Self { blocks };
        doc.normalize();
        doc
    }

    /// `blocks`, normalized (Rust convenience).
    #[must_use]
    pub fn from_blocks(blocks: Vec<RichBlock>) -> Self {
        Self::create_from_blocks(RichBlockVec::from_vec(blocks))
    }

    /// The blocks, as a slice.
    #[must_use]
    pub fn blocks(&self) -> &[RichBlock] {
        self.blocks.as_ref()
    }

    /// How many blocks the document has.
    #[must_use]
    pub fn block_count(&self) -> usize {
        self.blocks.as_ref().len()
    }

    /// Block `index`, if there is one.
    #[must_use]
    pub fn block(&self, index: usize) -> Option<&RichBlock> {
        self.blocks.as_ref().get(index)
    }

    /// Block `index`, mutable.
    pub fn block_mut(&mut self, index: usize) -> Option<&mut RichBlock> {
        self.blocks.as_mut().get_mut(index)
    }

    /// The blocks as a plain vector (the document is left empty: put them
    /// back with [`Self::put_blocks`]).
    pub fn take_blocks(&mut self) -> Vec<RichBlock> {
        core::mem::take(&mut self.blocks).into_library_owned_vec()
    }

    /// Puts `blocks` in as the document's blocks.
    pub fn put_blocks(&mut self, blocks: Vec<RichBlock>) {
        self.blocks = RichBlockVec::from_vec(blocks);
    }

    /// The canonical form: at least one block; runs merged; a block without
    /// text without runs; a code block one plain run; headings 1..=6; a list
    /// item at most one level deeper than the list item before it.
    pub fn normalize(&mut self) {
        let mut blocks = self.take_blocks();
        if blocks.is_empty() {
            blocks.push(RichBlock::paragraph(""));
        }
        let mut prev_indent: Option<u8> = None;
        for block in &mut blocks {
            let mut runs = block.runs_vec();
            match &mut block.kind {
                RichBlockKind::Heading(level) => *level = (*level).clamp(1, 6),
                RichBlockKind::Code(_) => {
                    let text = flatten(&runs);
                    runs = if text.is_empty() {
                        Vec::new()
                    } else {
                        vec![RichRun::plain(&text)]
                    };
                }
                RichBlockKind::Rule
                | RichBlockKind::Image(_)
                | RichBlockKind::PageBreak
                | RichBlockKind::Table(_) => runs.clear(),
                _ => {}
            }
            if block.kind.is_list() {
                let max = prev_indent.map_or(0, |p| p.saturating_add(1));
                let indent = block.kind.indent().min(max);
                block.kind = block.kind.with_indent(indent);
                prev_indent = Some(indent);
            } else {
                prev_indent = None;
            }
            normalize_runs(&mut runs);
            block.set_runs(runs);
        }
        self.put_blocks(blocks);
    }

    /// Every block's text, one line per block (search, word count, a
    /// preview): an image as its text alternative, a table's cells split by
    /// tabs and its rows by line breaks.
    #[must_use]
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        for (i, block) in self.blocks().iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            match &block.kind {
                RichBlockKind::Image(image) => out.push_str(image.alt.as_str()),
                RichBlockKind::Table(table) => {
                    for (r, row) in table.rows.as_ref().iter().enumerate() {
                        if r > 0 {
                            out.push('\n');
                        }
                        let cells: Vec<&str> =
                            row.cells.as_ref().iter().map(AzString::as_str).collect();
                        out.push_str(&cells.join("\t"));
                    }
                }
                _ => out.push_str(&block.flat()),
            }
        }
        out
    }

    /// How many words the document holds.
    #[must_use]
    pub fn word_count(&self) -> usize {
        self.plain_text().split_whitespace().count()
    }

    /// No text at all (only whitespace, rules, page breaks).
    #[must_use]
    pub fn is_blank(&self) -> bool {
        self.plain_text().trim().is_empty() && self.image_srcs().as_ref().is_empty()
    }

    /// The first line of text that is not `title` (a list's preview line),
    /// at most 120 characters.
    #[must_use]
    pub fn preview(&self, title: &str) -> String {
        for block in self.blocks() {
            if !block.kind.has_text() {
                continue;
            }
            let text = block.flat();
            let line = text.lines().map(str::trim).find(|l| !l.is_empty());
            let Some(line) = line else { continue };
            if line == title.trim() {
                continue;
            }
            return truncate_chars(line, 120);
        }
        String::new()
    }

    /// `(done, total)` of the checklist items.
    #[must_use]
    pub fn checklist(&self) -> (usize, usize) {
        let mut done = 0;
        let mut total = 0;
        for block in self.blocks() {
            if let RichBlockKind::Check(check) = &block.kind {
                total += 1;
                if check.checked {
                    done += 1;
                }
            }
        }
        (done, total)
    }

    /// The ticked checklist items.
    #[must_use]
    pub fn checklist_done(&self) -> usize {
        self.checklist().0
    }

    /// All checklist items.
    #[must_use]
    pub fn checklist_total(&self) -> usize {
        self.checklist().1
    }

    /// The `src` of every image block, in order.
    #[must_use]
    pub fn image_srcs(&self) -> StringVec {
        let srcs: Vec<AzString> = self
            .blocks()
            .iter()
            .filter_map(|b| match &b.kind {
                RichBlockKind::Image(image) => Some(image.src.clone()),
                _ => None,
            })
            .collect();
        StringVec::from_vec(srcs)
    }

    /// The number a numbered item shows: 1 + the numbered items at the same
    /// indent (and quote depth) right before it (deeper items between them
    /// do not break the count; anything else does).
    #[must_use]
    pub fn number_of(&self, index: usize) -> usize {
        let blocks = self.blocks();
        let Some(block) = blocks.get(index) else {
            return 1;
        };
        let RichBlockKind::Numbered(indent) = block.kind else {
            return 1;
        };
        let mut n = 1;
        for prev in blocks[..index].iter().rev() {
            if prev.quote_depth != block.quote_depth {
                break;
            }
            match prev.kind {
                RichBlockKind::Numbered(i) if i == indent => n += 1,
                ref k if k.is_list() && k.indent() > indent => {}
                _ => break,
            }
        }
        n
    }

    /// The edited text of block `index` folded into its runs: the unchanged
    /// prefix and suffix keep their formats, the new middle takes the format
    /// of the run it was typed into - the run BEFORE the caret at a run
    /// boundary (the browser rule: typing continues the format on the left)
    /// - or `typing`, the typing style, when the edit inserted text at the
    /// caret the style was set at. Returns whether anything changed.
    pub fn sync_block_text(
        &mut self,
        index: usize,
        new_text: &str,
        typing: Option<RichFormats>,
    ) -> bool {
        let Some(block) = self.block_mut(index) else {
            return false;
        };
        let old = block.flat();
        if old == new_text {
            return false;
        }
        if block.kind.is_code() {
            let runs = if new_text.is_empty() {
                Vec::new()
            } else {
                vec![RichRun::plain(new_text)]
            };
            block.set_runs(runs);
            return true;
        }
        if !block.kind.has_text() {
            return false;
        }
        let mut runs = block.runs_vec();
        let (prefix, suffix) = text_diff(&old, new_text);
        let middle = &new_text[prefix..new_text.len() - suffix];
        let covered = isolate(&mut runs, prefix, old.len() - suffix);
        // The format typed text takes: the run it replaced, else the run
        // before the caret (typing continues the format on the left), else
        // the run after it.
        let mut template = runs
            .get(covered.start)
            .filter(|_| !covered.is_empty())
            .or_else(|| covered.start.checked_sub(1).and_then(|i| runs.get(i)))
            .or_else(|| runs.get(covered.start))
            .cloned()
            .unwrap_or_default();
        if let Some(style) = typing {
            template.formats = style;
        }
        let replacement: Vec<RichRun> = if middle.is_empty() {
            Vec::new()
        } else {
            vec![template.with_text(middle)]
        };
        runs.splice(covered, replacement).for_each(drop);
        normalize_runs(&mut runs);
        block.set_runs(runs);
        true
    }

    /// Sets the text of cell `column` of row `row` of table block `index`
    /// (typing into a cell). Returns whether anything changed.
    pub fn set_table_cell(&mut self, index: usize, row: usize, column: usize, text: &str) -> bool {
        let Some(block) = self.block_mut(index) else {
            return false;
        };
        let RichBlockKind::Table(table) = &mut block.kind else {
            return false;
        };
        let Some(r) = table.rows.as_mut().get_mut(row) else {
            return false;
        };
        let mut cells = r.cells.as_ref().to_vec();
        if column >= cells.len() {
            cells.resize(column + 1, AzString::from_const_str(""));
        }
        if cells[column].as_str() == text {
            return false;
        }
        cells[column] = AzString::from(text);
        r.cells = StringVec::from_vec(cells);
        true
    }

    /// Splits block `index` at byte `at` of its text (Enter): the head stays,
    /// the tail becomes a new block right after it, at the same quote depth
    /// and alignment. A heading split at its end continues as a paragraph, a
    /// list as a list item (a check item unchecked). The runs are cut, not
    /// merged, so a run index stays the child index the engine resumes at.
    /// A block without text gets an empty paragraph after it. Returns the
    /// new block's index.
    pub fn split_block(&mut self, index: usize, at: usize) -> Option<usize> {
        if index >= self.block_count() {
            return None;
        }
        let mut blocks = self.take_blocks();
        let block = &mut blocks[index];
        let (quote_depth, align) = (block.quote_depth, block.align);
        if !block.kind.has_text() {
            let next = RichBlock::paragraph("").with_quote_depth(quote_depth);
            blocks.insert(index + 1, next);
            self.put_blocks(blocks);
            return Some(index + 1);
        }
        let len = block.len();
        let flat = block.flat();
        let at = floor_char_boundary(&flat, at.min(len));
        let mut head = block.runs_vec();
        let tail = split_runs(&mut head, at);
        block.set_runs(head);
        let kind = if at >= len {
            block.kind.continuation()
        } else {
            match &block.kind {
                RichBlockKind::Check(c) => RichBlockKind::Check(RichCheck {
                    indent: c.indent,
                    checked: false,
                }),
                other => other.clone(),
            }
        };
        let next = RichBlock::new(kind, tail)
            .with_quote_depth(quote_depth)
            .with_align(align);
        blocks.insert(index + 1, next);
        self.put_blocks(blocks);
        Some(index + 1)
    }

    /// Merges block `index` into the block before it (Backspace at a block's
    /// start). A block without text before it (a rule, an image, a page
    /// break, a table) is removed instead. `join_in_text` says the engine
    /// resumes inside the first block's last text (the seam runs merge) or
    /// before the second's first child (they stay apart). Returns the
    /// surviving block's index.
    pub fn merge_into_previous(&mut self, index: usize, join_in_text: bool) -> Option<usize> {
        if index == 0 || index >= self.block_count() {
            return None;
        }
        let mut blocks = self.take_blocks();
        if !blocks[index - 1].kind.has_text() {
            blocks.remove(index - 1);
            self.put_blocks(blocks);
            return Some(index - 1);
        }
        if !blocks[index].kind.has_text() {
            // An image merged up: nothing to join; it stays after the text.
            self.put_blocks(blocks);
            return Some(index - 1);
        }
        let second = blocks.remove(index);
        let first = &mut blocks[index - 1];
        let mut runs = first.runs_vec();
        if join_in_text {
            for run in second.runs_vec() {
                push_run(&mut runs, run);
            }
        } else {
            runs.extend(
                second
                    .runs_vec()
                    .into_iter()
                    .filter(|r| !r.text.as_str().is_empty()),
            );
        }
        first.set_runs(runs);
        self.put_blocks(blocks);
        Some(index - 1)
    }

    /// Replaces blocks `start..end` with ONE block whose text is `joined`
    /// (a delete or a type-over across blocks): the kind, quote depth and
    /// alignment and the formats of the kept head come from block `start`,
    /// the kept tail's formats from block `end - 1`; typed text in between
    /// takes the head's format.
    pub fn replace_blocks(&mut self, start: usize, end: usize, joined: &str) -> bool {
        if start >= end || end > self.block_count() {
            return false;
        }
        let first = self.blocks()[start].clone();
        let last = self.blocks()[end - 1].clone();
        let first_text = first.flat();
        let last_text = last.flat();
        let first_runs = first.runs_vec();
        let last_runs = last.runs_vec();
        let (head, _) = text_diff(&first_text, joined);
        let rest = &joined[head..];
        let (_, tail) = text_diff(&last_text, rest);
        let tail = tail.min(last_text.len());
        let mut runs = slice_runs(&first_runs, 0, head);
        let middle = &rest[..rest.len() - tail];
        if !middle.is_empty() {
            let template = first_runs.last().cloned().unwrap_or_default();
            push_run(&mut runs, template.with_text(middle));
        }
        for run in slice_runs(&last_runs, last_text.len() - tail, last_text.len()) {
            push_run(&mut runs, run);
        }
        let kind = if first.kind.has_text() {
            first.kind.clone()
        } else {
            RichBlockKind::Paragraph
        };
        let merged = RichBlock::new(kind, runs)
            .with_quote_depth(first.quote_depth)
            .with_align(first.align);
        let mut all = self.take_blocks();
        all.splice(start..end, [merged]).for_each(drop);
        self.put_blocks(all);
        true
    }

    /// Replaces blocks `start..end` with `parts` (a paste of several blocks
    /// over a selection): the first part keeps block `start`'s kind and the
    /// formats of what it kept of it, the last part the formats of what it
    /// kept of block `end - 1`; the parts between keep their own kind (a
    /// paragraph when they name none) and runs. Every part is quoted at
    /// least as deep as block `start`.
    pub fn replace_with(&mut self, start: usize, end: usize, parts: Vec<PastedBlock>) -> bool {
        if start >= end || end > self.block_count() || parts.is_empty() {
            return false;
        }
        let first = self.blocks()[start].clone();
        let last = self.blocks()[end - 1].clone();
        let first_runs = first.runs_vec();
        let last_runs = last.runs_vec();
        let count = parts.len();
        let mut out = Vec::with_capacity(count);
        for (i, part) in parts.into_iter().enumerate() {
            let PastedBlock {
                kind,
                quote_depth,
                runs,
            } = part;
            let mut runs = runs;
            if i == 0 {
                let text = flatten(&runs);
                let (head, _) = text_diff(&first.flat(), &text);
                let mut merged = slice_runs(&first_runs, 0, head);
                for run in slice_runs(&runs, head, text.len()) {
                    push_run(&mut merged, run);
                }
                runs = merged;
            }
            if i + 1 == count {
                let text = flatten(&runs);
                let last_text = last.flat();
                let (_, tail) = text_diff(&last_text, &text);
                let mut merged = slice_runs(&runs, 0, text.len() - tail);
                for run in slice_runs(&last_runs, last_text.len() - tail, last_text.len()) {
                    push_run(&mut merged, run);
                }
                runs = merged;
            }
            let kind = if i == 0 && first.kind.has_text() {
                first.kind.clone()
            } else {
                kind.filter(RichBlockKind::has_text)
                    .unwrap_or(RichBlockKind::Paragraph)
            };
            normalize_runs(&mut runs);
            let depth = if i == 0 {
                first.quote_depth
            } else {
                first.quote_depth.saturating_add(quote_depth)
            };
            out.push(
                RichBlock::new(kind, runs)
                    .with_quote_depth(depth)
                    .with_align(first.align),
            );
        }
        let mut all = self.take_blocks();
        all.splice(start..end, out).for_each(drop);
        self.put_blocks(all);
        true
    }

    /// Sets block `index`'s kind (its text stays; a code block's runs lose
    /// their formats). A text block never becomes a block without text.
    /// Returns whether it changed.
    pub fn set_kind(&mut self, index: usize, kind: RichBlockKind) -> bool {
        let Some(block) = self.block_mut(index) else {
            return false;
        };
        if block.kind == kind || !kind.has_text() || !block.kind.has_text() {
            return false;
        }
        block.kind = kind;
        if block.kind.is_code() {
            let text = block.flat();
            let runs = if text.is_empty() {
                Vec::new()
            } else {
                vec![RichRun::plain(&text)]
            };
            block.set_runs(runs);
        }
        true
    }

    /// The toolbar's block buttons: block `index` becomes `kind`, or a
    /// paragraph again when it already is of that family. A list item keeps
    /// its indent when it changes list kind.
    pub fn toggle_kind(&mut self, index: usize, kind: RichBlockKind) -> bool {
        let Some(block) = self.block(index) else {
            return false;
        };
        let next = if block.kind.same_family(&kind) {
            RichBlockKind::Paragraph
        } else if block.kind.is_list() && kind.is_list() {
            kind.with_indent(block.kind.indent())
        } else {
            kind
        };
        self.set_kind(index, next)
    }

    /// Applies a Markdown shortcut typed at the start of block `index`: the
    /// trigger goes, the kind changes (or, `> `, the quote deepens). Returns
    /// whether it applied.
    pub fn apply_shortcut(&mut self, index: usize, shortcut: &RichShortcut) -> bool {
        let Some(block) = self.block_mut(index) else {
            return false;
        };
        let mut runs = block.runs_vec();
        // `split_runs` leaves the trigger in `runs` and returns the rest,
        // which the block keeps.
        let mut rest = split_runs(&mut runs, shortcut.strip);
        match &shortcut.kind {
            Some(kind) => block.kind = kind.clone(),
            None => block.quote_depth = block.quote_depth.saturating_add(1),
        }
        if block.kind.is_code() {
            let text = flatten(&rest);
            rest = if text.is_empty() {
                Vec::new()
            } else {
                vec![RichRun::plain(&text)]
            };
        }
        normalize_runs(&mut rest);
        block.set_runs(rest);
        true
    }

    /// Sets (`on`) or clears `format` over bytes `start..end` of block
    /// `index`. Returns whether anything changed.
    pub fn set_format(
        &mut self,
        index: usize,
        start: usize,
        end: usize,
        format: RichFormat,
        on: bool,
    ) -> bool {
        let Some(block) = self.block_mut(index) else {
            return false;
        };
        if start >= end || block.kind.is_code() || !block.kind.has_text() {
            return false;
        }
        let flat = block.flat();
        let start = floor_char_boundary(&flat, start);
        let end = floor_char_boundary(&flat, end).max(start);
        if start == end {
            return false;
        }
        let mut runs = block.runs_vec();
        let covered = isolate(&mut runs, start, end);
        if covered.is_empty() {
            return false;
        }
        if runs[covered.clone()].iter().all(|r| r.has(format) == on) {
            return false;
        }
        for run in &mut runs[covered] {
            run.set(format, on);
        }
        normalize_runs(&mut runs);
        block.set_runs(runs);
        true
    }

    /// Toggles `format` over bytes `start..end` of block `index`: set on
    /// every run when one of them lacks it, else cleared (Bold pressed twice
    /// is plain again). Returns whether anything changed.
    pub fn toggle_format(&mut self, index: usize, start: usize, end: usize, format: RichFormat) -> bool {
        let on = !self.has_format(index, start, end, format);
        self.set_format(index, start, end, format, on)
    }

    /// Whether every run over `start..end` of block `index` carries
    /// `format` (a collapsed range asks the run before it).
    #[must_use]
    pub fn has_format(&self, index: usize, start: usize, end: usize, format: RichFormat) -> bool {
        let Some(block) = self.block(index) else {
            return false;
        };
        if start >= end {
            return self.formats_at(index, start).has(format);
        }
        let runs = slice_runs(block.runs.as_ref(), start, end);
        !runs.is_empty() && runs.iter().all(|r| r.has(format))
    }

    /// The formats text typed at byte `at` of block `index` takes without a
    /// typing style: the run before the caret's (the run after it at a
    /// block's start).
    #[must_use]
    pub fn formats_at(&self, index: usize, at: usize) -> RichFormats {
        self.run_at(index, at)
            .map_or(RichFormats::create(), |r| r.formats)
    }

    /// The link under byte `at` of block `index` (the run before the caret).
    #[must_use]
    pub fn link_at(&self, index: usize, at: usize) -> Option<String> {
        self.run_at(index, at)
            .and_then(|r| r.link_str().map(ToString::to_string))
    }

    /// The run the caret at byte `at` of block `index` continues.
    fn run_at(&self, index: usize, at: usize) -> Option<&RichRun> {
        let block = self.block(index)?;
        let mut acc = 0usize;
        let mut before: Option<&RichRun> = None;
        for run in block.runs.as_ref() {
            if acc >= at && acc > 0 {
                break;
            }
            before = Some(run);
            acc += run.text.as_str().len();
        }
        before
    }

    /// Links (or, `None`, unlinks) bytes `start..end` of block `index`.
    pub fn set_link(&mut self, index: usize, start: usize, end: usize, url: Option<&str>) -> bool {
        let Some(block) = self.block_mut(index) else {
            return false;
        };
        if start >= end || !block.kind.has_text() || block.kind.is_code() {
            return false;
        }
        let mut runs = block.runs_vec();
        let covered = isolate(&mut runs, start, end);
        if covered.is_empty() {
            return false;
        }
        let link: OptionString = url.map(AzString::from).into();
        for run in &mut runs[covered] {
            run.link = link.clone();
        }
        normalize_runs(&mut runs);
        block.set_runs(runs);
        true
    }

    /// Inserts `run` at byte `at` of block `index` (a link inserted at a
    /// caret). Returns whether it was inserted.
    pub fn insert_run(&mut self, index: usize, at: usize, run: RichRun) -> bool {
        let Some(block) = self.block_mut(index) else {
            return false;
        };
        if !block.kind.has_text() || run.as_str().is_empty() {
            return false;
        }
        let flat = block.flat();
        let at = floor_char_boundary(&flat, at.min(flat.len()));
        let mut runs = block.runs_vec();
        let tail = split_runs(&mut runs, at);
        push_run(&mut runs, run);
        for rest in tail {
            push_run(&mut runs, rest);
        }
        block.set_runs(runs);
        true
    }

    /// The byte range of the link the caret at byte `at` of block `index`
    /// is on: the neighbouring runs to the same address, together.
    #[must_use]
    pub fn link_range_at(&self, index: usize, at: usize) -> Option<(usize, usize)> {
        let block = self.block(index)?;
        let url = self.link_at(index, at)?;
        let mut spans: Vec<(usize, usize, bool)> = Vec::new();
        let mut acc = 0usize;
        for run in block.runs.as_ref() {
            let len = run.as_str().len();
            spans.push((acc, acc + len, run.link_str() == Some(url.as_str())));
            acc += len;
        }
        let hit = spans
            .iter()
            .position(|(s, e, linked)| *linked && *s <= at && at <= *e)?;
        let mut first = hit;
        while first > 0 && spans[first - 1].2 {
            first -= 1;
        }
        let mut last = hit;
        while last + 1 < spans.len() && spans[last + 1].2 {
            last += 1;
        }
        Some((spans[first].0, spans[last].1))
    }

    /// Indents (`delta > 0`) or outdents a list item; an outdent at level
    /// 0 turns it into a paragraph. Returns whether it changed.
    pub fn indent(&mut self, index: usize, delta: i8) -> bool {
        let Some(block) = self.block(index) else {
            return false;
        };
        if !block.kind.is_list() {
            return false;
        }
        let indent = block.kind.indent();
        if delta < 0 && indent == 0 {
            return self.set_kind(index, RichBlockKind::Paragraph);
        }
        let max = match index
            .checked_sub(1)
            .and_then(|i| self.block(i))
            .filter(|b| b.kind.is_list())
        {
            Some(prev) => prev.kind.indent().saturating_add(1),
            None => 0,
        };
        let next = if delta > 0 {
            indent.saturating_add(1).min(max)
        } else {
            indent - 1
        };
        if next == indent {
            return false;
        }
        let kind = block.kind.with_indent(next);
        if let Some(block) = self.block_mut(index) {
            block.kind = kind;
        }
        true
    }

    /// Ticks or unticks check item `index`.
    pub fn toggle_check(&mut self, index: usize) -> bool {
        match self.block_mut(index).map(|b| &mut b.kind) {
            Some(RichBlockKind::Check(check)) => {
                check.checked = !check.checked;
                true
            }
            _ => false,
        }
    }

    /// Sets block `index`'s quote depth. Returns whether it changed.
    pub fn set_quote_depth(&mut self, index: usize, depth: u8) -> bool {
        match self.block_mut(index) {
            Some(block) if block.quote_depth != depth => {
                block.quote_depth = depth;
                true
            }
            _ => false,
        }
    }

    /// Sets block `index`'s alignment. Returns whether it changed.
    pub fn set_align(&mut self, index: usize, align: RichAlign) -> bool {
        match self.block_mut(index) {
            Some(block) if block.align != align => {
                block.align = align;
                true
            }
            _ => false,
        }
    }

    /// Inserts `block` after block `index` (at the end when out of range);
    /// returns its index.
    pub fn insert_after(&mut self, index: usize, block: RichBlock) -> usize {
        let mut blocks = self.take_blocks();
        let at = (index + 1).min(blocks.len());
        blocks.insert(at, block);
        self.put_blocks(blocks);
        at
    }

    /// Removes block `index` (the document keeps one empty paragraph at
    /// least). Returns whether there was one.
    pub fn remove_block(&mut self, index: usize) -> bool {
        let mut blocks = self.take_blocks();
        let removed = index < blocks.len();
        if removed {
            blocks.remove(index);
        }
        if blocks.is_empty() {
            blocks.push(RichBlock::paragraph(""));
        }
        self.put_blocks(blocks);
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(text: &str) -> RichRun {
        RichRun::plain(text)
    }

    fn bold(text: &str) -> RichRun {
        RichRun::plain(text).with_format(RichFormat::Bold)
    }

    fn para(runs: Vec<RichRun>) -> RichBlock {
        RichBlock::new(RichBlockKind::Paragraph, runs)
    }

    fn runs(doc: &RichTextDoc, index: usize) -> Vec<RichRun> {
        doc.blocks()[index].runs_vec()
    }

    fn kinds(doc: &RichTextDoc) -> Vec<RichBlockKind> {
        doc.blocks().iter().map(|b| b.kind.clone()).collect()
    }

    fn check(indent: u8, checked: bool) -> RichBlockKind {
        RichBlockKind::Check(RichCheck { indent, checked })
    }

    #[test]
    fn typing_inside_a_bold_run_stays_bold_and_elsewhere_keeps_the_formats() {
        let mut doc =
            RichTextDoc::from_blocks(vec![para(vec![plain("one "), bold("two"), plain(" three")])]);
        assert!(doc.sync_block_text(0, "one twXo three", None));
        assert_eq!(
            runs(&doc, 0),
            vec![plain("one "), bold("twXo"), plain(" three")]
        );
        assert!(doc.sync_block_text(0, "one twXo thrYee", None));
        assert_eq!(runs(&doc, 0)[2], plain(" thrYee"));
        assert!(
            !doc.sync_block_text(0, "one twXo thrYee", None),
            "no change, no edit"
        );
    }

    #[test]
    fn typing_into_a_formatted_paragraph_keeps_its_bold_and_its_link() {
        // AzMail's sync flattened `<p>ab<b>c</b><a>d</a></p>` into one text
        // node on the first keystroke (DEDUP_EDITORS A3.1).
        let link = plain("d").with_link(AzString::from("https://example.org"));
        let mut doc = RichTextDoc::from_blocks(vec![para(vec![plain("ab"), bold("c"), link.clone()])]);
        assert!(doc.sync_block_text(0, "abXcd", None));
        assert_eq!(runs(&doc, 0), vec![plain("abX"), bold("c"), link.clone()]);
        assert!(doc.sync_block_text(0, "abXcdY", None));
        let linked = plain("dY").with_link(AzString::from("https://example.org"));
        assert_eq!(
            runs(&doc, 0),
            vec![plain("abX"), bold("c"), linked],
            "typing at a link's end continues the link"
        );
    }

    #[test]
    fn typing_at_the_end_of_a_bold_run_continues_it_and_a_typing_style_wins() {
        let mut doc = RichTextDoc::from_blocks(vec![para(vec![plain("a"), bold("b")])]);
        doc.sync_block_text(0, "abc", None);
        assert_eq!(runs(&doc, 0)[1], bold("bc"));
        let italic = RichFormats::create().with(RichFormat::Italic);
        doc.sync_block_text(0, "abcd", Some(italic));
        assert_eq!(
            runs(&doc, 0),
            vec![
                plain("a"),
                bold("bc"),
                plain("d").with_format(RichFormat::Italic)
            ]
        );
    }

    #[test]
    fn deleting_a_whole_run_and_typing_into_an_empty_block() {
        let mut doc = RichTextDoc::from_blocks(vec![para(vec![plain("x"), bold("bold")])]);
        doc.sync_block_text(0, "x", None);
        assert_eq!(runs(&doc, 0), vec![plain("x")]);
        doc.sync_block_text(0, "", None);
        assert!(runs(&doc, 0).is_empty());
        doc.sync_block_text(0, "hi", None);
        assert_eq!(runs(&doc, 0), vec![plain("hi")]);
    }

    #[test]
    fn multibyte_edits_never_split_a_character() {
        let mut doc = RichTextDoc::from_blocks(vec![RichBlock::paragraph("gr\u{fc}\u{df}e")]);
        assert!(doc.sync_block_text(0, "gr\u{fc}\u{df}\u{e9}e", None));
        assert_eq!(doc.blocks()[0].flat(), "gr\u{fc}\u{df}\u{e9}e");
        assert!(doc.toggle_format(0, 0, 3, RichFormat::Bold));
        assert_eq!(runs(&doc, 0)[0], bold("gr"));
    }

    #[test]
    fn enter_splits_a_block_and_a_heading_continues_as_a_paragraph() {
        let mut doc = RichTextDoc::from_blocks(vec![
            RichBlock::text(RichBlockKind::Heading(1), "Title"),
            para(vec![plain("ab"), bold("cd")]),
        ]);
        assert_eq!(doc.split_block(0, 5), Some(1));
        assert_eq!(doc.blocks()[1], RichBlock::paragraph(""));
        assert_eq!(doc.split_block(2, 3), Some(3));
        assert_eq!(runs(&doc, 2), vec![plain("ab"), bold("c")]);
        assert_eq!(runs(&doc, 3), vec![bold("d")]);
        assert_eq!(doc.split_block(0, 2), Some(1));
        assert_eq!(
            doc.blocks()[1].kind,
            RichBlockKind::Heading(1),
            "mid-heading: both halves headings"
        );
    }

    #[test]
    fn a_split_keeps_the_quote_depth_and_the_alignment() {
        let mut doc = RichTextDoc::from_blocks(vec![RichBlock::paragraph("quoted words")
            .with_quote_depth(2)
            .with_align(RichAlign::Center)]);
        assert_eq!(doc.split_block(0, 6), Some(1));
        assert_eq!(doc.blocks()[1].quote_depth, 2);
        assert_eq!(doc.blocks()[1].align, RichAlign::Center);
        assert_eq!(doc.blocks()[1].flat(), " words");
    }

    #[test]
    fn enter_in_a_check_item_starts_an_unchecked_one() {
        let mut doc = RichTextDoc::from_blocks(vec![RichBlock::text(check(0, true), "done")]);
        doc.split_block(0, 4);
        assert_eq!(doc.blocks()[1].kind, check(0, false));
    }

    #[test]
    fn backspace_merges_into_the_previous_block_or_removes_a_rule() {
        let mut doc = RichTextDoc::from_blocks(vec![
            RichBlock::paragraph("one"),
            RichBlock::paragraph("two"),
            RichBlock::new(RichBlockKind::Rule, vec![]),
            RichBlock::paragraph("three"),
        ]);
        assert_eq!(doc.merge_into_previous(1, true), Some(0));
        assert_eq!(runs(&doc, 0), vec![plain("onetwo")]);
        assert_eq!(doc.merge_into_previous(2, false), Some(1), "the rule goes");
        assert_eq!(doc.block_count(), 2);
        assert_eq!(doc.blocks()[1].flat(), "three");
        assert_eq!(doc.merge_into_previous(0, true), None);
    }

    #[test]
    fn a_merge_at_a_child_boundary_keeps_the_seam_runs_apart() {
        let mut doc =
            RichTextDoc::from_blocks(vec![RichBlock::paragraph("a"), RichBlock::paragraph("b")]);
        doc.merge_into_previous(1, false);
        assert_eq!(runs(&doc, 0), vec![plain("a"), plain("b")]);
        doc.normalize();
        assert_eq!(runs(&doc, 0), vec![plain("ab")]);
    }

    #[test]
    fn a_delete_across_blocks_keeps_both_ends_formats() {
        let italic = |t: &str| plain(t).with_format(RichFormat::Italic);
        let mut doc = RichTextDoc::from_blocks(vec![
            RichBlock::new(RichBlockKind::Heading(2), vec![plain("ab"), italic("cd")]),
            RichBlock::paragraph("middle"),
            para(vec![bold("ef"), plain("gh")]),
        ]);
        // select "d" .. "e" and type "X"
        assert!(doc.replace_blocks(0, 3, "abcXfgh"));
        assert_eq!(doc.block_count(), 1);
        assert_eq!(doc.blocks()[0].kind, RichBlockKind::Heading(2));
        assert_eq!(
            runs(&doc, 0),
            vec![plain("ab"), italic("cX"), bold("f"), plain("gh")]
        );
    }

    #[test]
    fn a_paste_of_blocks_over_a_selection_keeps_both_ends() {
        let mut doc = RichTextDoc::from_blocks(vec![
            para(vec![plain("ab"), bold("cd")]),
            RichBlock::text(RichBlockKind::Bullet(0), "efgh"),
        ]);
        // select "d" .. "e", paste "X" / heading "Y" / "Z"
        let part = |kind: Option<RichBlockKind>, runs: Vec<RichRun>| PastedBlock {
            kind,
            quote_depth: 0,
            runs,
        };
        let parts = vec![
            part(Some(RichBlockKind::Paragraph), vec![plain("abcX")]),
            part(Some(RichBlockKind::Heading(2)), vec![plain("Y")]),
            part(
                None,
                vec![plain("Z").with_format(RichFormat::Italic), plain("fgh")],
            ),
        ];
        assert!(doc.replace_with(0, 2, parts));
        assert_eq!(doc.block_count(), 3);
        assert_eq!(runs(&doc, 0), vec![plain("ab"), bold("c"), plain("X")]);
        assert_eq!(doc.blocks()[1].kind, RichBlockKind::Heading(2));
        assert_eq!(doc.blocks()[2].kind, RichBlockKind::Paragraph);
        assert_eq!(
            runs(&doc, 2),
            vec![plain("Z").with_format(RichFormat::Italic), plain("fgh")]
        );
    }

    #[test]
    fn toggling_bold_over_a_range_splits_and_merges_back() {
        let mut doc = RichTextDoc::from_blocks(vec![RichBlock::paragraph("hello world")]);
        assert!(doc.toggle_format(0, 6, 11, RichFormat::Bold));
        assert_eq!(runs(&doc, 0), vec![plain("hello "), bold("world")]);
        assert!(doc.has_format(0, 6, 11, RichFormat::Bold));
        assert!(!doc.has_format(0, 0, 11, RichFormat::Bold));
        assert!(
            doc.has_format(0, 11, 11, RichFormat::Bold),
            "a caret after bold text"
        );
        assert!(doc.toggle_format(0, 6, 11, RichFormat::Bold));
        assert_eq!(runs(&doc, 0), vec![plain("hello world")]);
    }

    #[test]
    fn bold_pressed_twice_over_a_selection_is_plain_again() {
        // AzMail wrapped another <b> on every press (DEDUP_EDITORS A3.4).
        let mut doc = RichTextDoc::from_blocks(vec![para(vec![plain("a "), bold("b"), plain(" c")])]);
        assert!(doc.toggle_format(0, 0, 5, RichFormat::Bold), "partly bold: all bold");
        assert_eq!(runs(&doc, 0), vec![bold("a b c")]);
        assert!(doc.toggle_format(0, 0, 5, RichFormat::Bold), "all bold: plain");
        assert_eq!(runs(&doc, 0), vec![plain("a b c")]);
        assert!(
            !doc.set_format(0, 0, 5, RichFormat::Bold, false),
            "clearing what is not there changes nothing"
        );
    }

    #[test]
    fn links_are_set_and_removed_over_a_range() {
        let mut doc = RichTextDoc::from_blocks(vec![RichBlock::paragraph("see the plan")]);
        assert!(doc.set_link(0, 4, 12, Some("https://example.org/plan")));
        assert_eq!(
            doc.link_at(0, 6).as_deref(),
            Some("https://example.org/plan")
        );
        assert_eq!(doc.link_at(0, 2), None);
        assert!(doc.set_link(0, 4, 12, None));
        assert_eq!(runs(&doc, 0), vec![plain("see the plan")]);
    }

    #[test]
    fn markdown_shortcuts_fire_once_when_typed() {
        let p = RichBlockKind::Paragraph;
        let kind = |s: Option<RichShortcut>| s.and_then(|s| s.kind);
        assert_eq!(
            kind(typed_shortcut(&p, "#", "# ")),
            Some(RichBlockKind::Heading(1))
        );
        assert_eq!(
            typed_shortcut(&p, "##", "## ").map(|s| (s.kind, s.strip)),
            Some((Some(RichBlockKind::Heading(2)), 3))
        );
        assert_eq!(
            kind(typed_shortcut(&p, "-", "- ")),
            Some(RichBlockKind::Bullet(0))
        );
        assert_eq!(
            typed_shortcut(&p, "12.", "12. ").map(|s| (s.kind, s.strip)),
            Some((Some(RichBlockKind::Numbered(0)), 4))
        );
        assert_eq!(kind(typed_shortcut(&p, "[ ]", "[ ] ")), Some(check(0, false)));
        assert_eq!(
            typed_shortcut(&p, ">", "> "),
            Some(RichShortcut {
                kind: None,
                strip: 2
            }),
            "a quote deepens the quote"
        );
        assert_eq!(
            kind(typed_shortcut(&p, "``", "```")),
            Some(RichBlockKind::Code(AzString::from_const_str("")))
        );
        assert_eq!(
            kind(typed_shortcut(&RichBlockKind::Bullet(1), "[ ]", "[ ] ")),
            Some(check(1, false)),
            "- [ ] typed in a row ends a check item"
        );
        assert_eq!(
            typed_shortcut(&p, "# x", "# xy"),
            None,
            "an old trigger is text now"
        );
        assert_eq!(typed_shortcut(&p, "#tag", "#tags"), None);
        assert_eq!(typed_shortcut(&RichBlockKind::Heading(1), "", "# "), None);
    }

    #[test]
    fn applying_a_shortcut_strips_the_trigger_and_keeps_the_rest() {
        let mut doc = RichTextDoc::from_blocks(vec![para(vec![plain("- buy "), bold("milk")])]);
        let flat = doc.blocks()[0].flat();
        let shortcut = shortcut_in(&RichBlockKind::Paragraph, &flat).expect("a bullet");
        assert!(doc.apply_shortcut(0, &shortcut));
        assert_eq!(doc.blocks()[0].kind, RichBlockKind::Bullet(0));
        assert_eq!(runs(&doc, 0), vec![plain("buy "), bold("milk")]);
    }

    #[test]
    fn the_quote_shortcut_deepens_the_quote_and_keeps_the_kind() {
        let mut doc = RichTextDoc::from_blocks(vec![RichBlock::paragraph("> said")]);
        let shortcut = shortcut_in(&RichBlockKind::Paragraph, "> said").expect("a quote");
        assert!(doc.apply_shortcut(0, &shortcut));
        assert_eq!(doc.blocks()[0].kind, RichBlockKind::Paragraph);
        assert_eq!(doc.blocks()[0].quote_depth, 1);
        assert_eq!(doc.blocks()[0].flat(), "said");
    }

    #[test]
    fn numbered_items_count_per_level_and_restart_after_a_paragraph() {
        let doc = RichTextDoc::from_blocks(vec![
            RichBlock::text(RichBlockKind::Numbered(0), "a"),
            RichBlock::text(RichBlockKind::Numbered(1), "a.1"),
            RichBlock::text(RichBlockKind::Numbered(1), "a.2"),
            RichBlock::text(RichBlockKind::Numbered(0), "b"),
            RichBlock::paragraph("break"),
            RichBlock::text(RichBlockKind::Numbered(0), "c"),
            RichBlock::text(RichBlockKind::Numbered(0), "quoted").with_quote_depth(1),
        ]);
        let numbers: Vec<usize> = (0..doc.block_count()).map(|i| doc.number_of(i)).collect();
        assert_eq!(numbers, vec![1, 1, 2, 2, 1, 1, 1]);
    }

    #[test]
    fn indents_are_bounded_by_the_item_before_and_outdent_ends_the_list() {
        let mut doc = RichTextDoc::from_blocks(vec![
            RichBlock::text(RichBlockKind::Bullet(0), "a"),
            RichBlock::text(RichBlockKind::Bullet(0), "b"),
        ]);
        assert!(!doc.indent(0, 1), "the first item cannot indent");
        assert!(doc.indent(1, 1));
        assert!(!doc.indent(1, 1), "at most one deeper than the item before");
        assert_eq!(doc.blocks()[1].kind, RichBlockKind::Bullet(1));
        assert!(doc.indent(1, -1));
        assert!(doc.indent(1, -1));
        assert_eq!(doc.blocks()[1].kind, RichBlockKind::Paragraph);
    }

    #[test]
    fn normalize_clamps_orphan_indents_and_keeps_one_block() {
        let mut doc = RichTextDoc {
            blocks: RichBlockVec::from_vec(Vec::new()),
        };
        doc.normalize();
        assert_eq!(doc, RichTextDoc::create());
        let doc = RichTextDoc::from_blocks(vec![
            RichBlock::paragraph("p"),
            RichBlock::text(RichBlockKind::Bullet(3), "deep"),
            RichBlock::text(RichBlockKind::Bullet(4), "deeper"),
        ]);
        assert_eq!(doc.blocks()[1].kind, RichBlockKind::Bullet(0));
        assert_eq!(doc.blocks()[2].kind, RichBlockKind::Bullet(1));
    }

    #[test]
    fn toggling_a_block_kind_twice_returns_to_a_paragraph() {
        let mut doc = RichTextDoc::from_blocks(vec![
            RichBlock::text(RichBlockKind::Bullet(0), "parent"),
            RichBlock::text(RichBlockKind::Bullet(1), "x"),
        ]);
        assert!(doc.toggle_kind(1, RichBlockKind::Numbered(0)));
        assert_eq!(
            doc.blocks()[1].kind,
            RichBlockKind::Numbered(1),
            "keeps its indent"
        );
        assert!(doc.toggle_kind(1, RichBlockKind::Numbered(0)));
        assert_eq!(doc.blocks()[1].kind, RichBlockKind::Paragraph);
        assert!(doc.toggle_kind(1, RichBlockKind::Heading(2)));
        assert!(
            !doc.toggle_kind(1, RichBlockKind::Rule),
            "a text block never becomes a rule"
        );
    }

    #[test]
    fn typing_into_a_table_cell_changes_that_cell_only() {
        let table = RichTable::empty(2, 2);
        let mut doc = RichTextDoc::from_blocks(vec![
            RichBlock::new(RichBlockKind::Table(table), vec![]),
            RichBlock::paragraph("after"),
        ]);
        assert!(doc.set_table_cell(0, 1, 0, "Q4"));
        assert!(!doc.set_table_cell(0, 1, 0, "Q4"), "no change, no edit");
        let table = doc.blocks()[0].table().expect("a table");
        assert_eq!(table.rows.as_ref()[1].cell(0), "Q4");
        assert_eq!(table.rows.as_ref()[0].cell(0), "");
        assert!(!doc.sync_block_text(0, "text", None), "a table has no runs");
        assert_eq!(doc.plain_text(), "\t\nQ4\t\nafter");
    }

    #[test]
    fn enter_after_a_block_without_text_starts_a_paragraph_and_backspace_removes_it() {
        let mut doc = RichTextDoc::from_blocks(vec![
            RichBlock::paragraph("before"),
            RichBlock::new(RichBlockKind::PageBreak, vec![RichRun::plain("ignored")]),
        ]);
        assert!(doc.blocks()[1].is_empty(), "a page break holds no runs");
        assert_eq!(doc.split_block(1, 0), Some(2));
        assert_eq!(doc.blocks()[2], RichBlock::paragraph(""));
        assert_eq!(doc.merge_into_previous(2, false), Some(1), "the page break goes");
        assert_eq!(kinds(&doc), vec![RichBlockKind::Paragraph, RichBlockKind::Paragraph]);
    }

    #[test]
    fn preview_skips_the_title_and_blank_lines_and_counts_words() {
        let doc = RichTextDoc::from_blocks(vec![
            RichBlock::text(RichBlockKind::Heading(1), "Offsite agenda"),
            RichBlock::paragraph(""),
            RichBlock::paragraph("Bring laptops"),
            RichBlock::new(
                RichBlockKind::Image(RichImage {
                    src: AzString::from("a.png"),
                    alt: AzString::from("diagram"),
                }),
                vec![],
            ),
        ]);
        assert_eq!(doc.preview("Offsite agenda"), "Bring laptops");
        assert_eq!(doc.word_count(), 5);
        assert_eq!(doc.image_srcs().as_ref(), &[AzString::from("a.png")]);
        assert!(!doc.is_blank());
        assert!(RichTextDoc::create().is_blank());
    }

    #[test]
    fn the_checklist_counts_ticked_and_all_items() {
        let doc = RichTextDoc::from_blocks(vec![
            RichBlock::text(check(0, true), "a"),
            RichBlock::text(check(0, false), "b"),
            RichBlock::paragraph("c"),
        ]);
        assert_eq!(doc.checklist(), (1, 2));
        assert_eq!((doc.checklist_done(), doc.checklist_total()), (1, 2));
    }
}
