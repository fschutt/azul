//! The note's document: a FLAT list of blocks, each a list of styled runs.
//!
//! Flat on purpose. A list item is a block with an indent level, a quote is
//! a block of its own: the editor renders ONE child of the editing host per
//! block, so a block index IS the host's child index - the vocabulary the
//! engine's edit paths (`get_node_child_index_path`, a split's resume path)
//! speak. A run renders as ONE child of its block, so a run index is a
//! block's child index too.
//!
//! Nothing here knows azul: the model is plain data with its edits, and the
//! unit tests run without a window.

use core::ops::Range;

/// One stretch of text in one format.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// Inline code (monospace).
    pub code: bool,
    /// The run is (part of) a link to this URL.
    pub link: Option<String>,
}

/// An inline format a run can carry (the toolbar's toggles).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Bold,
    Italic,
    Underline,
    Strike,
    Code,
}

impl Format {
    pub const ALL: [Format; 5] = [
        Format::Bold,
        Format::Italic,
        Format::Underline,
        Format::Strike,
        Format::Code,
    ];
}

/// The formats text typed at a caret takes (the editor's "typing style").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FormatSet {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub code: bool,
}

impl FormatSet {
    #[must_use]
    pub fn of(run: &Run) -> Self {
        FormatSet {
            bold: run.bold,
            italic: run.italic,
            underline: run.underline,
            strike: run.strike,
            code: run.code,
        }
    }

    #[must_use]
    pub fn has(&self, f: Format) -> bool {
        match f {
            Format::Bold => self.bold,
            Format::Italic => self.italic,
            Format::Underline => self.underline,
            Format::Strike => self.strike,
            Format::Code => self.code,
        }
    }

    pub fn set(&mut self, f: Format, on: bool) {
        match f {
            Format::Bold => self.bold = on,
            Format::Italic => self.italic = on,
            Format::Underline => self.underline = on,
            Format::Strike => self.strike = on,
            Format::Code => self.code = on,
        }
    }

    /// `run` with these formats (its text and link kept).
    pub fn apply_to(&self, run: &mut Run) {
        run.bold = self.bold;
        run.italic = self.italic;
        run.underline = self.underline;
        run.strike = self.strike;
        run.code = self.code;
    }
}

impl Run {
    #[must_use]
    pub fn plain(text: impl Into<String>) -> Run {
        Run {
            text: text.into(),
            ..Run::default()
        }
    }

    /// The run with `f` set (for building documents in code).
    #[must_use]
    pub fn with(mut self, f: Format) -> Run {
        self.set(f, true);
        self
    }

    /// The run as a link to `url`.
    #[must_use]
    pub fn linked(mut self, url: impl Into<String>) -> Run {
        self.link = Some(url.into());
        self
    }

    #[must_use]
    pub fn has(&self, f: Format) -> bool {
        FormatSet::of(self).has(f)
    }

    pub fn set(&mut self, f: Format, on: bool) {
        match f {
            Format::Bold => self.bold = on,
            Format::Italic => self.italic = on,
            Format::Underline => self.underline = on,
            Format::Strike => self.strike = on,
            Format::Code => self.code = on,
        }
    }

    /// Same formats and link (the text may differ).
    #[must_use]
    pub fn same_format(&self, other: &Run) -> bool {
        FormatSet::of(self) == FormatSet::of(other) && self.link == other.link
    }

    /// No format and no link: rendered as a bare text node.
    #[must_use]
    pub fn is_plain(&self) -> bool {
        FormatSet::of(self) == FormatSet::default() && self.link.is_none()
    }
}

/// What a block is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    /// `#` .. `######`.
    Heading(u8),
    /// A bulleted list item at an indent level (0 = top).
    Bullet(u8),
    /// A numbered list item at an indent level; its number is its place
    /// among the numbered items of its level ([`Doc::number_of`]).
    Numbered(u8),
    /// A checklist item.
    Check { indent: u8, checked: bool },
    Quote,
    /// A fenced code block; its one run holds the code, lines split by `\n`.
    Code { lang: String },
    /// A horizontal rule (no text).
    Rule,
    /// An image (no text): `src` is the path relative to the note's folder
    /// (`<uuid>/assets/<file>`) or a URL.
    Image { src: String, alt: String },
}

impl BlockKind {
    /// A list item (bullet, numbered, check).
    #[must_use]
    pub fn is_list(&self) -> bool {
        matches!(
            self,
            BlockKind::Bullet(_) | BlockKind::Numbered(_) | BlockKind::Check { .. }
        )
    }

    /// The indent level of a list item; 0 for every other block.
    #[must_use]
    pub fn indent(&self) -> u8 {
        match self {
            BlockKind::Bullet(i) | BlockKind::Numbered(i) => *i,
            BlockKind::Check { indent, .. } => *indent,
            _ => 0,
        }
    }

    /// The same kind at another indent level (no-op for a non-list block).
    #[must_use]
    pub fn with_indent(&self, indent: u8) -> BlockKind {
        match self {
            BlockKind::Bullet(_) => BlockKind::Bullet(indent),
            BlockKind::Numbered(_) => BlockKind::Numbered(indent),
            BlockKind::Check { checked, .. } => BlockKind::Check {
                indent,
                checked: *checked,
            },
            other => other.clone(),
        }
    }

    /// The block holds text (everything but a rule and an image).
    #[must_use]
    pub fn has_text(&self) -> bool {
        !matches!(self, BlockKind::Rule | BlockKind::Image { .. })
    }

    /// Two kinds of the same family for a toolbar toggle (indent, check
    /// state, heading level and code language aside... except the heading
    /// level, which is part of the button).
    #[must_use]
    pub fn same_family(&self, other: &BlockKind) -> bool {
        match (self, other) {
            (BlockKind::Heading(a), BlockKind::Heading(b)) => a == b,
            (BlockKind::Bullet(_), BlockKind::Bullet(_))
            | (BlockKind::Numbered(_), BlockKind::Numbered(_))
            | (BlockKind::Check { .. }, BlockKind::Check { .. })
            | (BlockKind::Code { .. }, BlockKind::Code { .. })
            | (BlockKind::Image { .. }, BlockKind::Image { .. }) => true,
            (a, b) => a == b,
        }
    }

    /// The kind the next block takes when Enter splits a block of this kind
    /// at its END (a heading continues as a paragraph, a list as a list).
    #[must_use]
    pub fn continuation(&self) -> BlockKind {
        match self {
            BlockKind::Heading(_) | BlockKind::Rule | BlockKind::Image { .. } => {
                BlockKind::Paragraph
            }
            BlockKind::Check { indent, .. } => BlockKind::Check {
                indent: *indent,
                checked: false,
            },
            other => other.clone(),
        }
    }
}

/// One block: its kind and its runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    pub runs: Vec<Run>,
}

impl Block {
    #[must_use]
    pub fn new(kind: BlockKind, runs: Vec<Run>) -> Block {
        Block { kind, runs }
    }

    /// A paragraph of plain `text`.
    #[must_use]
    pub fn paragraph(text: &str) -> Block {
        Block::text(BlockKind::Paragraph, text)
    }

    /// A block of `kind` holding plain `text` (no run when empty).
    #[must_use]
    pub fn text(kind: BlockKind, text: &str) -> Block {
        let runs = if text.is_empty() {
            Vec::new()
        } else {
            vec![Run::plain(text)]
        };
        Block { kind, runs }
    }

    /// The block's text, every run in order.
    #[must_use]
    pub fn flat(&self) -> String {
        flatten(&self.runs)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.runs.iter().all(|r| r.text.is_empty())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.runs.iter().map(|r| r.text.len()).sum()
    }
}

/// The note's body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Doc {
    pub blocks: Vec<Block>,
}

impl Default for Doc {
    fn default() -> Self {
        Doc::new()
    }
}

/// The concatenated text of `runs`.
#[must_use]
pub fn flatten(runs: &[Run]) -> String {
    runs.iter().map(|r| r.text.as_str()).collect()
}

/// Appends `run`, merging it into the last run when the formats agree; an
/// empty run is dropped.
pub fn push_run(runs: &mut Vec<Run>, run: Run) {
    if run.text.is_empty() {
        return;
    }
    if let Some(last) = runs.last_mut() {
        if last.same_format(&run) {
            last.text.push_str(&run.text);
            return;
        }
    }
    runs.push(run);
}

/// Merges neighbours of the same format and drops empty runs.
pub fn normalize_runs(runs: &mut Vec<Run>) {
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

/// Splits `runs` so a run boundary falls at byte `at` of their text.
fn split_at_byte(runs: &mut Vec<Run>, at: usize) {
    let mut acc = 0usize;
    for i in 0..runs.len() {
        let len = runs[i].text.len();
        if at > acc && at < acc + len {
            let cut = floor_char_boundary(&runs[i].text, at - acc);
            if cut > 0 && cut < len {
                let mut tail = runs[i].clone();
                tail.text = runs[i].text[cut..].to_string();
                runs[i].text.truncate(cut);
                runs.insert(i + 1, tail);
            }
            return;
        }
        acc += len;
    }
}

/// Splits `runs` at `start` and `end` and returns the index range of the
/// runs that hold exactly the bytes `start..end`.
fn isolate(runs: &mut Vec<Run>, start: usize, end: usize) -> Range<usize> {
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
        acc += run.text.len();
    }
    first..last.max(first)
}

/// Cuts `runs` at byte `at`: `runs` keeps the head, the tail is returned.
pub fn split_runs(runs: &mut Vec<Run>, at: usize) -> Vec<Run> {
    split_at_byte(runs, at);
    let mut acc = 0usize;
    let mut cut = runs.len();
    for (i, run) in runs.iter().enumerate() {
        if acc >= at {
            cut = i;
            break;
        }
        acc += run.text.len();
    }
    runs.split_off(cut)
}

/// The runs of `runs` that cover bytes `start..end`, cut to them.
#[must_use]
pub fn slice_runs(runs: &[Run], start: usize, end: usize) -> Vec<Run> {
    let mut copy = runs.to_vec();
    let range = isolate(&mut copy, start, end);
    copy[range].to_vec()
}

/// What a Markdown shortcut typed at the start of a block turns it into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcut {
    pub kind: BlockKind,
    /// The bytes of the trigger to remove from the block's start.
    pub strip: usize,
}

/// The shortcut whose trigger `text` starts with, for a block of `kind`:
/// `# ` / `## ` / `### `, `- ` / `* ` / `+ `, `1. ` (any number), `[ ] ` /
/// `[x] ` (also in a bullet, so `- [ ] ` typed in a row ends a check item),
/// `> `, and a block that is exactly ` ``` `.
#[must_use]
pub fn shortcut_in(kind: &BlockKind, text: &str) -> Option<Shortcut> {
    let shortcut = |kind: BlockKind, strip: usize| Some(Shortcut { kind, strip });
    match kind {
        BlockKind::Paragraph => {}
        BlockKind::Bullet(indent) => {
            return if text.starts_with("[ ] ") {
                shortcut(
                    BlockKind::Check {
                        indent: *indent,
                        checked: false,
                    },
                    4,
                )
            } else if text.starts_with("[x] ") || text.starts_with("[X] ") {
                shortcut(
                    BlockKind::Check {
                        indent: *indent,
                        checked: true,
                    },
                    4,
                )
            } else {
                None
            };
        }
        _ => return None,
    }
    for level in (1..=3u8).rev() {
        let trigger = format!("{} ", "#".repeat(level as usize));
        if text.starts_with(&trigger) {
            return shortcut(BlockKind::Heading(level), trigger.len());
        }
    }
    if text.starts_with("- ") || text.starts_with("* ") || text.starts_with("+ ") {
        return shortcut(BlockKind::Bullet(0), 2);
    }
    if text.starts_with("[ ] ") || text.starts_with("[] ") {
        let strip = if text.starts_with("[] ") { 3 } else { 4 };
        return shortcut(
            BlockKind::Check {
                indent: 0,
                checked: false,
            },
            strip,
        );
    }
    if text.starts_with("[x] ") || text.starts_with("[X] ") {
        return shortcut(
            BlockKind::Check {
                indent: 0,
                checked: true,
            },
            4,
        );
    }
    if text.starts_with("> ") {
        return shortcut(BlockKind::Quote, 2);
    }
    if text == "```" {
        return shortcut(
            BlockKind::Code {
                lang: String::new(),
            },
            3,
        );
    }
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    if (1..=9).contains(&digits) && text[digits..].starts_with(". ") {
        return shortcut(BlockKind::Numbered(0), digits + 2);
    }
    None
}

/// The shortcut an EDIT typed: `new` matches a trigger that `old` did not.
/// Text that already started with a trigger (a paragraph `# not a heading`
/// loaded from a file, `\#` in Markdown) is never converted by an edit
/// further on.
#[must_use]
pub fn typed_shortcut(kind: &BlockKind, old: &str, new: &str) -> Option<Shortcut> {
    let now = shortcut_in(kind, new)?;
    match shortcut_in(kind, old) {
        Some(before) if before == now => None,
        _ => Some(now),
    }
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

impl Doc {
    /// One empty paragraph: the caret's anchor in a new note.
    #[must_use]
    pub fn new() -> Doc {
        Doc {
            blocks: vec![Block::paragraph("")],
        }
    }

    /// `blocks`, normalized.
    #[must_use]
    pub fn from_blocks(blocks: Vec<Block>) -> Doc {
        let mut doc = Doc { blocks };
        doc.normalize();
        doc
    }

    /// The canonical form: at least one block; runs merged; a text-less
    /// block without runs; a code block one plain run; headings 1..=6; a
    /// list item at most one level deeper than the list item before it.
    pub fn normalize(&mut self) {
        if self.blocks.is_empty() {
            self.blocks.push(Block::paragraph(""));
        }
        let mut prev_indent: Option<u8> = None;
        for block in &mut self.blocks {
            match &mut block.kind {
                BlockKind::Heading(level) => *level = (*level).clamp(1, 6),
                BlockKind::Code { .. } => {
                    let text = flatten(&block.runs);
                    block.runs = if text.is_empty() {
                        Vec::new()
                    } else {
                        vec![Run::plain(text)]
                    };
                }
                BlockKind::Rule | BlockKind::Image { .. } => block.runs.clear(),
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
            normalize_runs(&mut block.runs);
        }
    }

    /// Every block's text, one line per block (search, word count).
    #[must_use]
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        for (i, block) in self.blocks.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            match &block.kind {
                BlockKind::Image { alt, .. } => out.push_str(alt),
                _ => out.push_str(&block.flat()),
            }
        }
        out
    }

    #[must_use]
    pub fn word_count(&self) -> usize {
        self.blocks
            .iter()
            .map(|b| b.flat().split_whitespace().count())
            .sum()
    }

    /// The first line of text that is not the title (the list's preview).
    #[must_use]
    pub fn preview(&self, title: &str) -> String {
        for block in &self.blocks {
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
        for block in &self.blocks {
            if let BlockKind::Check { checked, .. } = block.kind {
                total += 1;
                if checked {
                    done += 1;
                }
            }
        }
        (done, total)
    }

    /// The `src` of every image block.
    #[must_use]
    pub fn image_srcs(&self) -> Vec<String> {
        self.blocks
            .iter()
            .filter_map(|b| match &b.kind {
                BlockKind::Image { src, .. } => Some(src.clone()),
                _ => None,
            })
            .collect()
    }

    /// The number a numbered item shows: 1 + the numbered items at the same
    /// indent right before it (deeper items between them do not break the
    /// count; anything else does).
    #[must_use]
    pub fn number_of(&self, index: usize) -> usize {
        let Some(block) = self.blocks.get(index) else {
            return 1;
        };
        let BlockKind::Numbered(indent) = block.kind else {
            return 1;
        };
        let mut n = 1;
        for prev in self.blocks[..index].iter().rev() {
            match prev.kind {
                BlockKind::Numbered(i) if i == indent => n += 1,
                ref k if k.is_list() && k.indent() > indent => {}
                _ => break,
            }
        }
        n
    }

    /// The edited text of block `index` folded into its runs: the unchanged
    /// prefix and suffix keep their formats, the new middle takes the format
    /// of the run it was typed into (or `typing`, the typing style, when
    /// the edit inserted text at the caret the style was set at). Returns
    /// whether anything changed.
    pub fn sync_block_text(&mut self, index: usize, new_text: &str, typing: Option<FormatSet>) -> bool {
        let Some(block) = self.blocks.get_mut(index) else {
            return false;
        };
        let old = block.flat();
        if old == new_text {
            return false;
        }
        if matches!(block.kind, BlockKind::Code { .. }) {
            block.runs = if new_text.is_empty() {
                Vec::new()
            } else {
                vec![Run::plain(new_text)]
            };
            return true;
        }
        let (prefix, suffix) = text_diff(&old, new_text);
        let middle = &new_text[prefix..new_text.len() - suffix];
        let covered = isolate(&mut block.runs, prefix, old.len() - suffix);
        // The format typed text takes: the run it replaced, else the run
        // before the caret (the browser rule: typing continues the format
        // to the left), else the run after it.
        let mut template = block
            .runs
            .get(covered.start)
            .filter(|_| !covered.is_empty())
            .or_else(|| covered.start.checked_sub(1).and_then(|i| block.runs.get(i)))
            .or_else(|| block.runs.get(covered.start))
            .cloned()
            .unwrap_or_default();
        if let Some(style) = typing {
            style.apply_to(&mut template);
        }
        template.text = middle.to_string();
        let replacement: Vec<Run> = if template.text.is_empty() {
            Vec::new()
        } else {
            vec![template]
        };
        block.runs.splice(covered, replacement).for_each(drop);
        normalize_runs(&mut block.runs);
        true
    }

    /// Splits block `index` at byte `at` of its text (Enter): the head stays,
    /// the tail becomes a new block right after it. A heading split at its
    /// end continues as a paragraph, a list as a list item (a check item
    /// unchecked). The runs are cut, not merged, so a run index stays the
    /// child index the engine resumes at. Returns the new block's index.
    pub fn split_block(&mut self, index: usize, at: usize) -> Option<usize> {
        let block = self.blocks.get_mut(index)?;
        if !block.kind.has_text() {
            let next = Block::paragraph("");
            self.blocks.insert(index + 1, next);
            return Some(index + 1);
        }
        let len = block.len();
        let at = floor_char_boundary(&block.flat(), at.min(len));
        let tail = split_runs(&mut block.runs, at);
        let kind = if at >= len {
            block.kind.continuation()
        } else {
            match &block.kind {
                BlockKind::Check { indent, .. } => BlockKind::Check {
                    indent: *indent,
                    checked: false,
                },
                other => other.clone(),
            }
        };
        self.blocks.insert(index + 1, Block::new(kind, tail));
        Some(index + 1)
    }

    /// Merges block `index` into the block before it (Backspace at a block's
    /// start). A block without text before it (a rule, an image) is removed
    /// instead. `join_in_text` says the engine resumes inside the first
    /// block's last text (the seam runs merge) or before the second's first
    /// child (they stay apart). Returns the surviving block's index.
    pub fn merge_into_previous(&mut self, index: usize, join_in_text: bool) -> Option<usize> {
        if index == 0 || index >= self.blocks.len() {
            return None;
        }
        if !self.blocks[index - 1].kind.has_text() {
            self.blocks.remove(index - 1);
            return Some(index - 1);
        }
        if !self.blocks[index].kind.has_text() {
            // An image merged up: nothing to join; it stays after the text.
            return Some(index - 1);
        }
        let second = self.blocks.remove(index);
        let first = &mut self.blocks[index - 1];
        if join_in_text {
            for run in second.runs {
                push_run(&mut first.runs, run);
            }
        } else {
            first.runs.extend(second.runs.into_iter().filter(|r| !r.text.is_empty()));
        }
        Some(index - 1)
    }

    /// Replaces blocks `start..end` with ONE block whose text is `joined`
    /// (a delete or a type-over across blocks): the kind and the formats of
    /// the kept head come from block `start`, the kept tail's from block
    /// `end - 1`, typed text in between takes the head's format.
    pub fn replace_blocks(&mut self, start: usize, end: usize, joined: &str) -> bool {
        if start >= end || end > self.blocks.len() {
            return false;
        }
        let first = self.blocks[start].clone();
        let last = self.blocks[end - 1].clone();
        let first_text = first.flat();
        let last_text = last.flat();
        let (head, _) = text_diff(&first_text, joined);
        let rest = &joined[head..];
        let (_, tail) = text_diff(&last_text, rest);
        let tail = tail.min(last_text.len());
        let mut runs = slice_runs(&first.runs, 0, head);
        let middle = &rest[..rest.len() - tail];
        if !middle.is_empty() {
            let mut run = first
                .runs
                .last()
                .cloned()
                .unwrap_or_default();
            run.text = middle.to_string();
            push_run(&mut runs, run);
        }
        for run in slice_runs(&last.runs, last_text.len() - tail, last_text.len()) {
            push_run(&mut runs, run);
        }
        let kind = if first.kind.has_text() {
            first.kind
        } else {
            BlockKind::Paragraph
        };
        self.blocks.splice(start..end, [Block::new(kind, runs)]).for_each(drop);
        true
    }

    /// Sets block `index`'s kind (its text stays; a code block's runs lose
    /// their formats). Returns whether it changed.
    pub fn set_kind(&mut self, index: usize, kind: BlockKind) -> bool {
        let Some(block) = self.blocks.get_mut(index) else {
            return false;
        };
        if block.kind == kind {
            return false;
        }
        if !kind.has_text() {
            return false;
        }
        block.kind = kind;
        if let BlockKind::Code { .. } = block.kind {
            let text = block.flat();
            block.runs = if text.is_empty() {
                Vec::new()
            } else {
                vec![Run::plain(text)]
            };
        }
        true
    }

    /// The toolbar's block buttons: block `index` becomes `kind`, or a
    /// paragraph again when it already is of that family. A list item keeps
    /// its indent when it changes list kind.
    pub fn toggle_kind(&mut self, index: usize, kind: BlockKind) -> bool {
        let Some(block) = self.blocks.get(index) else {
            return false;
        };
        let next = if block.kind.same_family(&kind) {
            BlockKind::Paragraph
        } else if block.kind.is_list() && kind.is_list() {
            kind.with_indent(block.kind.indent())
        } else {
            kind
        };
        self.set_kind(index, next)
    }

    /// Applies a Markdown shortcut typed at the start of block `index`:
    /// the trigger goes, the kind changes. Returns whether it applied.
    pub fn apply_shortcut(&mut self, index: usize, shortcut: &Shortcut) -> bool {
        let Some(block) = self.blocks.get_mut(index) else {
            return false;
        };
        // `split_runs` leaves the trigger in `block.runs` and returns the
        // rest, which the block keeps.
        block.runs = split_runs(&mut block.runs, shortcut.strip);
        block.kind = shortcut.kind.clone();
        if let BlockKind::Code { .. } = block.kind {
            let text = block.flat();
            block.runs = if text.is_empty() {
                Vec::new()
            } else {
                vec![Run::plain(text)]
            };
        }
        normalize_runs(&mut block.runs);
        true
    }

    /// Toggles `format` over bytes `start..end` of block `index`: set on
    /// every run when one of them lacks it, else cleared. Returns whether
    /// anything changed.
    pub fn toggle_format(&mut self, index: usize, start: usize, end: usize, format: Format) -> bool {
        let Some(block) = self.blocks.get_mut(index) else {
            return false;
        };
        if start >= end || matches!(block.kind, BlockKind::Code { .. }) || !block.kind.has_text() {
            return false;
        }
        let flat = block.flat();
        let start = floor_char_boundary(&flat, start);
        let end = floor_char_boundary(&flat, end).max(start);
        if start == end {
            return false;
        }
        let covered = isolate(&mut block.runs, start, end);
        if covered.is_empty() {
            return false;
        }
        let on = !block.runs[covered.clone()].iter().all(|r| r.has(format));
        for run in &mut block.runs[covered] {
            run.set(format, on);
        }
        normalize_runs(&mut block.runs);
        true
    }

    /// Whether every run over `start..end` of block `index` carries
    /// `format` (a collapsed range asks the run before it).
    #[must_use]
    pub fn has_format(&self, index: usize, start: usize, end: usize, format: Format) -> bool {
        let Some(block) = self.blocks.get(index) else {
            return false;
        };
        if start >= end {
            let mut acc = 0usize;
            let mut before: Option<&Run> = None;
            for run in &block.runs {
                if acc >= start && acc > 0 {
                    break;
                }
                before = Some(run);
                acc += run.text.len();
            }
            return before.is_some_and(|r| r.has(format));
        }
        let runs = slice_runs(&block.runs, start, end);
        !runs.is_empty() && runs.iter().all(|r| r.has(format))
    }

    /// Links (or, `None`, unlinks) bytes `start..end` of block `index`.
    pub fn set_link(&mut self, index: usize, start: usize, end: usize, url: Option<String>) -> bool {
        let Some(block) = self.blocks.get_mut(index) else {
            return false;
        };
        if start >= end || !block.kind.has_text() {
            return false;
        }
        let covered = isolate(&mut block.runs, start, end);
        if covered.is_empty() {
            return false;
        }
        for run in &mut block.runs[covered] {
            run.link = url.clone();
        }
        normalize_runs(&mut block.runs);
        true
    }

    /// Indents (`delta > 0`) or outdents a list item; an outdent at level
    /// 0 turns it into a paragraph. Returns whether it changed.
    pub fn indent(&mut self, index: usize, delta: i8) -> bool {
        let Some(block) = self.blocks.get(index) else {
            return false;
        };
        if !block.kind.is_list() {
            return false;
        }
        let indent = block.kind.indent();
        if delta < 0 && indent == 0 {
            return self.set_kind(index, BlockKind::Paragraph);
        }
        let max = match index
            .checked_sub(1)
            .and_then(|i| self.blocks.get(i))
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
        let kind = self.blocks[index].kind.with_indent(next);
        self.blocks[index].kind = kind;
        true
    }

    /// Ticks or unticks check item `index`.
    pub fn toggle_check(&mut self, index: usize) -> bool {
        match self.blocks.get_mut(index).map(|b| &mut b.kind) {
            Some(BlockKind::Check { checked, .. }) => {
                *checked = !*checked;
                true
            }
            _ => false,
        }
    }

    /// Inserts `block` after block `index` (at the end when out of range);
    /// returns its index.
    pub fn insert_after(&mut self, index: usize, block: Block) -> usize {
        let at = (index + 1).min(self.blocks.len());
        self.blocks.insert(at, block);
        at
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn para(runs: Vec<Run>) -> Block {
        Block::new(BlockKind::Paragraph, runs)
    }

    #[test]
    fn typing_inside_a_bold_run_stays_bold_and_elsewhere_keeps_the_formats() {
        let mut doc = Doc::from_blocks(vec![para(vec![
            Run::plain("one "),
            Run::plain("two").with(Format::Bold),
            Run::plain(" three"),
        ])]);
        assert!(doc.sync_block_text(0, "one twXo three", None));
        assert_eq!(
            doc.blocks[0].runs,
            vec![
                Run::plain("one "),
                Run::plain("twXo").with(Format::Bold),
                Run::plain(" three")
            ]
        );
        assert!(doc.sync_block_text(0, "one twXo thrYee", None));
        assert_eq!(doc.blocks[0].runs[2], Run::plain(" thrYee"));
        assert!(!doc.sync_block_text(0, "one twXo thrYee", None), "no change, no edit");
    }

    #[test]
    fn typing_at_the_end_of_a_bold_run_continues_it_and_a_typing_style_wins() {
        let mut doc = Doc::from_blocks(vec![para(vec![
            Run::plain("a"),
            Run::plain("b").with(Format::Bold),
        ])]);
        doc.sync_block_text(0, "abc", None);
        assert_eq!(doc.blocks[0].runs[1], Run::plain("bc").with(Format::Bold));
        let italic = FormatSet {
            italic: true,
            ..FormatSet::default()
        };
        doc.sync_block_text(0, "abcd", Some(italic));
        assert_eq!(
            doc.blocks[0].runs,
            vec![
                Run::plain("a"),
                Run::plain("bc").with(Format::Bold),
                Run::plain("d").with(Format::Italic)
            ]
        );
    }

    #[test]
    fn deleting_a_whole_run_and_typing_into_an_empty_block() {
        let mut doc = Doc::from_blocks(vec![para(vec![
            Run::plain("x"),
            Run::plain("bold").with(Format::Bold),
        ])]);
        doc.sync_block_text(0, "x", None);
        assert_eq!(doc.blocks[0].runs, vec![Run::plain("x")]);
        doc.sync_block_text(0, "", None);
        assert!(doc.blocks[0].runs.is_empty());
        doc.sync_block_text(0, "hi", None);
        assert_eq!(doc.blocks[0].runs, vec![Run::plain("hi")]);
    }

    #[test]
    fn multibyte_edits_never_split_a_character() {
        let mut doc = Doc::from_blocks(vec![Block::paragraph("gr\u{fc}\u{df}e")]);
        assert!(doc.sync_block_text(0, "gr\u{fc}\u{df}\u{e9}e", None));
        assert_eq!(doc.blocks[0].flat(), "gr\u{fc}\u{df}\u{e9}e");
        assert!(doc.toggle_format(0, 0, 3, Format::Bold));
        assert_eq!(doc.blocks[0].runs[0], Run::plain("gr").with(Format::Bold));
    }

    #[test]
    fn enter_splits_a_block_and_a_heading_continues_as_a_paragraph() {
        let mut doc = Doc::from_blocks(vec![
            Block::text(BlockKind::Heading(1), "Title"),
            para(vec![Run::plain("ab"), Run::plain("cd").with(Format::Bold)]),
        ]);
        assert_eq!(doc.split_block(0, 5), Some(1));
        assert_eq!(doc.blocks[1], Block::paragraph(""));
        assert_eq!(doc.split_block(2, 3), Some(3));
        assert_eq!(
            doc.blocks[2].runs,
            vec![Run::plain("ab"), Run::plain("c").with(Format::Bold)]
        );
        assert_eq!(doc.blocks[3].runs, vec![Run::plain("d").with(Format::Bold)]);
        assert_eq!(doc.split_block(0, 2), Some(1));
        assert_eq!(doc.blocks[1].kind, BlockKind::Heading(1), "mid-heading: both halves headings");
    }

    #[test]
    fn enter_in_a_check_item_starts_an_unchecked_one() {
        let mut doc = Doc::from_blocks(vec![Block::text(
            BlockKind::Check {
                indent: 0,
                checked: true,
            },
            "done",
        )]);
        doc.split_block(0, 4);
        assert_eq!(
            doc.blocks[1].kind,
            BlockKind::Check {
                indent: 0,
                checked: false
            }
        );
    }

    #[test]
    fn backspace_merges_into_the_previous_block_or_removes_a_rule() {
        let mut doc = Doc::from_blocks(vec![
            Block::paragraph("one"),
            Block::paragraph("two"),
            Block::new(BlockKind::Rule, vec![]),
            Block::paragraph("three"),
        ]);
        assert_eq!(doc.merge_into_previous(1, true), Some(0));
        assert_eq!(doc.blocks[0].runs, vec![Run::plain("onetwo")]);
        assert_eq!(doc.merge_into_previous(2, false), Some(1), "the rule goes");
        assert_eq!(doc.blocks.len(), 2);
        assert_eq!(doc.blocks[1].flat(), "three");
        assert_eq!(doc.merge_into_previous(0, true), None);
    }

    #[test]
    fn a_merge_at_a_child_boundary_keeps_the_seam_runs_apart() {
        let mut doc = Doc::from_blocks(vec![Block::paragraph("a"), Block::paragraph("b")]);
        doc.merge_into_previous(1, false);
        assert_eq!(doc.blocks[0].runs, vec![Run::plain("a"), Run::plain("b")]);
        doc.normalize();
        assert_eq!(doc.blocks[0].runs, vec![Run::plain("ab")]);
    }

    #[test]
    fn a_delete_across_blocks_keeps_both_ends_formats() {
        let mut doc = Doc::from_blocks(vec![
            Block::new(
                BlockKind::Heading(2),
                vec![Run::plain("ab"), Run::plain("cd").with(Format::Italic)],
            ),
            Block::paragraph("middle"),
            para(vec![Run::plain("ef").with(Format::Bold), Run::plain("gh")]),
        ]);
        // select "d" .. "e" and type "X"
        assert!(doc.replace_blocks(0, 3, "abcXfgh"));
        assert_eq!(doc.blocks.len(), 1);
        assert_eq!(doc.blocks[0].kind, BlockKind::Heading(2));
        assert_eq!(
            doc.blocks[0].runs,
            vec![
                Run::plain("ab"),
                Run::plain("cX").with(Format::Italic),
                Run::plain("f").with(Format::Bold),
                Run::plain("gh")
            ]
        );
    }

    #[test]
    fn toggling_bold_over_a_range_splits_and_merges_back() {
        let mut doc = Doc::from_blocks(vec![Block::paragraph("hello world")]);
        assert!(doc.toggle_format(0, 6, 11, Format::Bold));
        assert_eq!(
            doc.blocks[0].runs,
            vec![Run::plain("hello "), Run::plain("world").with(Format::Bold)]
        );
        assert!(doc.has_format(0, 6, 11, Format::Bold));
        assert!(!doc.has_format(0, 0, 11, Format::Bold));
        assert!(doc.has_format(0, 11, 11, Format::Bold), "a caret after bold text");
        assert!(doc.toggle_format(0, 6, 11, Format::Bold));
        assert_eq!(doc.blocks[0].runs, vec![Run::plain("hello world")]);
    }

    #[test]
    fn markdown_shortcuts_fire_once_when_typed() {
        let p = BlockKind::Paragraph;
        assert_eq!(
            typed_shortcut(&p, "#", "# ").map(|s| s.kind),
            Some(BlockKind::Heading(1))
        );
        assert_eq!(
            typed_shortcut(&p, "##", "## ").map(|s| (s.kind, s.strip)),
            Some((BlockKind::Heading(2), 3))
        );
        assert_eq!(typed_shortcut(&p, "-", "- ").map(|s| s.kind), Some(BlockKind::Bullet(0)));
        assert_eq!(
            typed_shortcut(&p, "12.", "12. ").map(|s| (s.kind, s.strip)),
            Some((BlockKind::Numbered(0), 4))
        );
        assert_eq!(
            typed_shortcut(&p, "[ ]", "[ ] ").map(|s| s.kind),
            Some(BlockKind::Check {
                indent: 0,
                checked: false
            })
        );
        assert_eq!(typed_shortcut(&p, ">", "> ").map(|s| s.kind), Some(BlockKind::Quote));
        assert_eq!(
            typed_shortcut(&p, "``", "```").map(|s| s.kind),
            Some(BlockKind::Code {
                lang: String::new()
            })
        );
        assert_eq!(
            typed_shortcut(&BlockKind::Bullet(1), "[ ]", "[ ] ").map(|s| s.kind),
            Some(BlockKind::Check {
                indent: 1,
                checked: false
            }),
            "- [ ] typed in a row ends a check item"
        );
        assert_eq!(typed_shortcut(&p, "# x", "# xy"), None, "an old trigger is text now");
        assert_eq!(typed_shortcut(&p, "#tag", "#tags"), None);
        assert_eq!(typed_shortcut(&BlockKind::Heading(1), "", "# "), None);
    }

    #[test]
    fn applying_a_shortcut_strips_the_trigger_and_keeps_the_rest() {
        let mut doc = Doc::from_blocks(vec![para(vec![
            Run::plain("- buy "),
            Run::plain("milk").with(Format::Bold),
        ])]);
        let shortcut = shortcut_in(&BlockKind::Paragraph, &doc.blocks[0].flat()).expect("a bullet");
        assert!(doc.apply_shortcut(0, &shortcut));
        assert_eq!(doc.blocks[0].kind, BlockKind::Bullet(0));
        assert_eq!(
            doc.blocks[0].runs,
            vec![Run::plain("buy "), Run::plain("milk").with(Format::Bold)]
        );
    }

    #[test]
    fn numbered_items_count_per_level_and_restart_after_a_paragraph() {
        let doc = Doc::from_blocks(vec![
            Block::text(BlockKind::Numbered(0), "a"),
            Block::text(BlockKind::Numbered(1), "a.1"),
            Block::text(BlockKind::Numbered(1), "a.2"),
            Block::text(BlockKind::Numbered(0), "b"),
            Block::paragraph("break"),
            Block::text(BlockKind::Numbered(0), "c"),
        ]);
        let numbers: Vec<usize> = (0..doc.blocks.len()).map(|i| doc.number_of(i)).collect();
        assert_eq!(numbers, vec![1, 1, 2, 2, 1, 1]);
    }

    #[test]
    fn indents_are_bounded_by_the_item_before_and_outdent_ends_the_list() {
        let mut doc = Doc::from_blocks(vec![
            Block::text(BlockKind::Bullet(0), "a"),
            Block::text(BlockKind::Bullet(0), "b"),
        ]);
        assert!(!doc.indent(0, 1), "the first item cannot indent");
        assert!(doc.indent(1, 1));
        assert!(!doc.indent(1, 1), "at most one deeper than the item before");
        assert_eq!(doc.blocks[1].kind, BlockKind::Bullet(1));
        assert!(doc.indent(1, -1));
        assert!(doc.indent(1, -1));
        assert_eq!(doc.blocks[1].kind, BlockKind::Paragraph);
    }

    #[test]
    fn normalize_clamps_orphan_indents_and_keeps_one_block() {
        let mut doc = Doc { blocks: vec![] };
        doc.normalize();
        assert_eq!(doc, Doc::new());
        let doc = Doc::from_blocks(vec![
            Block::paragraph("p"),
            Block::text(BlockKind::Bullet(3), "deep"),
            Block::text(BlockKind::Bullet(4), "deeper"),
        ]);
        assert_eq!(doc.blocks[1].kind, BlockKind::Bullet(0));
        assert_eq!(doc.blocks[2].kind, BlockKind::Bullet(1));
    }

    #[test]
    fn toggling_a_block_kind_twice_returns_to_a_paragraph() {
        let mut doc = Doc::from_blocks(vec![
            Block::text(BlockKind::Bullet(0), "parent"),
            Block::text(BlockKind::Bullet(1), "x"),
        ]);
        assert!(doc.toggle_kind(1, BlockKind::Numbered(0)));
        assert_eq!(doc.blocks[1].kind, BlockKind::Numbered(1), "keeps its indent");
        assert!(doc.toggle_kind(1, BlockKind::Numbered(0)));
        assert_eq!(doc.blocks[1].kind, BlockKind::Paragraph);
        assert!(doc.toggle_kind(1, BlockKind::Heading(2)));
        assert!(!doc.toggle_kind(1, BlockKind::Rule), "a text block never becomes a rule");
    }

    #[test]
    fn preview_skips_the_title_and_blank_lines() {
        let doc = Doc::from_blocks(vec![
            Block::text(BlockKind::Heading(1), "Offsite agenda"),
            Block::paragraph(""),
            Block::paragraph("Bring laptops"),
        ]);
        assert_eq!(doc.preview("Offsite agenda"), "Bring laptops");
        assert_eq!(doc.word_count(), 4);
    }
}
