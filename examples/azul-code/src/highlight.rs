//! Syntax colours: syntect's TextMate grammars, INCREMENTAL BY LINE.
//!
//! syntect parses a line from the parse state the line before left (a
//! `ParseState` and the scope stack - both `Clone + Eq`), so highlighting
//! is a walk down the lines. The highlighter keeps:
//! - CHECKPOINTS: the state at the start of every 64th line it walked
//!   past, so a line far down is reached from the nearest one, not from the
//!   top;
//! - a CACHE of the lines it coloured lately (their spans and the state
//!   after them), so the lines in view cost nothing while nothing changes
//!   and the next line down costs one line;
//! - after an edit at line L: everything up to L stays (a line's state
//!   depends only on the lines above it); the checkpoints below the edit
//!   become STALE (shifted by the lines it added or removed) and are taken
//!   back as soon as a walk reaches one with an EQUAL state - an edit that
//!   closes no comment and opens no string re-parses a few lines, not the
//!   rest of the file.
//!
//! A line more than [`SYNC_LIMIT`] lines below anything known is NOT parsed
//! on the UI thread: [`Highlighter::line_spans`] answers `None` (the view
//! shows the line plain), the app runs a [`HighlightJob`] on an azul Thread
//! and hands its checkpoints back ([`Highlighter::adopt`]).
//!
//! The scopes map onto azul's `CodeTokenKind`s ([`TokenKind`] here, the
//! same list) - the colours are the app theme's, not a syntect theme's.

use std::{collections::BTreeMap, sync::Arc};

use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};

/// Lines between two checkpoints.
pub const EVERY: usize = 64;
/// The most lines parsed on the UI thread to reach a line.
pub const SYNC_LIMIT: usize = 1500;
/// The coloured lines kept.
pub const CACHE_LINES: usize = 4000;

/// What a run of a line is - azul's `CodeTokenKind`, the same list in the
/// same order (the UI maps one onto the other).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TokenKind {
    #[default]
    Plain,
    Keyword,
    Type,
    Function,
    StringLiteral,
    Number,
    Comment,
    Constant,
    Macro,
    Attribute,
    Operator,
    Punctuation,
    Variable,
    Tag,
    Heading,
    Link,
    Invalid,
}

/// A coloured run: bytes `start..end` of the line are `kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeSpan {
    pub start: u32,
    pub end: u32,
    pub kind: TokenKind,
}

/// The grammars and the scope rules: loaded ONCE by the app (its state owns them) and shared, by `Arc`, with
/// every file's highlighter and every walk on a worker thread. (They used to be two process-wide caches.)
#[derive(Debug)]
pub struct Grammar {
    set: SyntaxSet,
    rules: Rules,
}

impl Grammar {
    /// The grammars syntect ships and the scope rules, ready to share.
    #[must_use]
    pub fn load() -> Arc<Grammar> {
        Arc::new(Grammar {
            set: SyntaxSet::load_defaults_newlines(),
            rules: Rules::new(),
        })
    }

    /// The grammar for a file - its place in the set: by the file's extension, else by its first line (a `#!`
    /// line), else plain text.
    #[must_use]
    pub fn syntax_for(&self, file_name: &str, first_line: &str) -> usize {
        let extension = file_name.rsplit_once('.').map_or("", |(_, e)| e);
        let found = self
            .set
            .find_syntax_by_extension(extension)
            .or_else(|| self.set.find_syntax_by_first_line(first_line))
            .unwrap_or_else(|| self.set.find_syntax_plain_text());
        self.set.syntaxes().iter().position(|s| std::ptr::eq(s, found)).unwrap_or(0)
    }

    /// The grammar at `index` (the last one when the index is past the set).
    fn syntax(&self, index: usize) -> &SyntaxReference {
        let all = self.set.syntaxes();
        &all[index.min(all.len().saturating_sub(1))]
    }

    /// One line parsed from `state`: its runs and the state after it.
    fn parse_line(&self, state: &LineState, text: &str) -> (Vec<CodeSpan>, LineState) {
        let mut next = state.clone();
        let mut line = String::with_capacity(text.len() + 1);
        line.push_str(text);
        line.push('\n');
        let ops = next.parse.parse_line(&line, &self.set).unwrap_or_default();
        let len = text.len();
        let mut spans = Vec::new();
        let mut last = 0_usize;
        for (at, op) in ops {
            let at = at.min(len);
            if at > last {
                push_span(&mut spans, last, at, self.classify(next.scopes.as_slice()));
                last = at;
            }
            let _ = next.scopes.apply(&op);
        }
        if len > last {
            push_span(&mut spans, last, len, self.classify(next.scopes.as_slice()));
        }
        (spans, next)
    }

    /// The kind a run inside `scopes` is (outermost first): inside a comment or a string it is that; else the
    /// innermost scope that names a kind decides.
    #[must_use]
    pub fn classify(&self, scopes: &[Scope]) -> TokenKind {
        let r = &self.rules;
        let within = |prefix: Option<Scope>| prefix.map_or(false, |p| scopes.iter().any(|s| p.is_prefix_of(*s)));
        if within(r.comment) {
            return TokenKind::Comment;
        }
        if within(r.string) {
            return TokenKind::StringLiteral;
        }
        for s in scopes.iter().rev() {
            if let Some((_, kind)) = r.list.iter().find(|(p, _)| p.is_prefix_of(*s)) {
                return *kind;
            }
        }
        TokenKind::Plain
    }
}

/// The parse state at the start (or the end) of a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineState {
    parse: ParseState,
    scopes: ScopeStack,
}

impl LineState {
    /// The state before the first line of `syntax`.
    pub fn start(syntax: &SyntaxReference) -> LineState {
        LineState {
            parse: ParseState::new(syntax),
            scopes: ScopeStack::new(),
        }
    }
}

/// One file's highlighter. See the module documentation.
#[derive(Debug, Clone)]
pub struct Highlighter {
    /// The grammars, shared with the app and the walks.
    grammar: Arc<Grammar>,
    /// This file's grammar: its place in the set.
    syntax: usize,
    /// The state at the start of these lines, sorted; line 0 always.
    checkpoints: Vec<(usize, LineState)>,
    /// Checkpoints from before the last edits, on their new lines.
    stale: Vec<(usize, LineState)>,
    /// Coloured lines: their spans and the state after them.
    cache: BTreeMap<usize, (Vec<CodeSpan>, LineState)>,
    /// Bumped by every edit: a job started before it is stale.
    generation: u64,
    /// Lines parsed so far (what the tests count).
    pub lines_parsed: usize,
}

/// A walk down the lines for a background thread: from a known state to
/// the line the view wants, leaving checkpoints.
#[derive(Debug, Clone)]
pub struct HighlightJob {
    pub first: usize,
    pub state: LineState,
    pub lines: Vec<String>,
    pub generation: u64,
    /// The grammars the walk parses with (shared with the app).
    pub grammar: Arc<Grammar>,
}

/// What a job found: the checkpoints of its walk.
#[derive(Debug, Clone)]
pub struct JobResult {
    pub checkpoints: Vec<(usize, LineState)>,
    pub generation: u64,
}

impl Highlighter {
    /// A highlighter for the file `file_name` whose first line is `first_line`, knowing nothing yet.
    #[must_use]
    pub fn new(grammar: Arc<Grammar>, file_name: &str, first_line: &str) -> Highlighter {
        let syntax = grammar.syntax_for(file_name, first_line);
        Highlighter {
            checkpoints: vec![(0, LineState::start(grammar.syntax(syntax)))],
            grammar,
            syntax,
            stale: Vec::new(),
            cache: BTreeMap::new(),
            generation: 0,
            lines_parsed: 0,
        }
    }

    /// The grammar's name ("Rust", "Markdown", "Plain Text").
    pub fn language(&self) -> &str {
        &self.grammar.syntax(self.syntax).name
    }

    /// Whether line `line`'s colours are cached.
    pub fn is_cached(&self, line: usize) -> bool {
        self.cache.contains_key(&line)
    }

    /// Line `line`'s colours, `text_of(i)` being line i's text; `None` when
    /// the line is too far below anything known (run a [`HighlightJob`]).
    pub fn line_spans(&mut self, line: usize, text_of: &dyn Fn(usize) -> String) -> Option<Vec<CodeSpan>> {
        if let Some((spans, _)) = self.cache.get(&line) {
            return Some(spans.clone());
        }
        let (mut at, mut state) = self.start_for(line);
        if line - at > SYNC_LIMIT {
            return None;
        }
        loop {
            if self.converge(at, &state) {
                let (jump, jumped) = self.start_for(line);
                if jump > at {
                    at = jump;
                    state = jumped;
                }
            }
            self.checkpoint(at, &state);
            let (spans, next) = self.grammar.parse_line(&state, &text_of(at));
            self.lines_parsed += 1;
            self.cache.insert(at, (spans.clone(), next.clone()));
            if at >= line {
                self.trim_cache(line);
                return Some(spans);
            }
            state = next;
            at += 1;
        }
    }

    /// The text changed: line `first` and the `removed` lines after it
    /// became `first` and `added` lines after it.
    pub fn edited(&mut self, first: usize, removed: usize, added: usize) {
        let old_end = first + removed;
        let moved = |l: usize| (l + added).saturating_sub(removed);
        let mut kept = Vec::with_capacity(self.checkpoints.len());
        let mut stale = Vec::new();
        for (l, s) in self.checkpoints.drain(..) {
            if l <= first {
                kept.push((l, s));
            } else if l > old_end {
                stale.push((moved(l), s));
            }
        }
        for (l, s) in self.stale.drain(..) {
            if l <= first {
                stale.push((l, s));
            } else if l > old_end {
                stale.push((moved(l), s));
            }
        }
        stale.sort_by_key(|(l, _)| *l);
        self.checkpoints = kept;
        self.stale = stale;
        let _below = self.cache.split_off(&first);
        self.generation += 1;
    }

    /// The walk a thread should do to reach line `line`.
    pub fn job_for(&self, line: usize, text_of: &dyn Fn(usize) -> String) -> HighlightJob {
        let (first, state) = self.start_for(line);
        HighlightJob {
            first,
            state,
            lines: (first..=line).map(|i| text_of(i)).collect(),
            generation: self.generation,
            grammar: Arc::clone(&self.grammar),
        }
    }

    /// A job's checkpoints taken in; `false` when an edit came since.
    pub fn adopt(&mut self, result: JobResult) -> bool {
        if result.generation != self.generation {
            return false;
        }
        for (l, s) in result.checkpoints {
            if self.checkpoints.iter().any(|(x, _)| *x == l) {
                continue;
            }
            let i = self.checkpoints.partition_point(|(x, _)| *x < l);
            self.checkpoints.insert(i, (l, s));
        }
        true
    }

    /// Where a walk to `line` starts: the nearest checkpoint at or above
    /// it, or the line after the nearest coloured line above it.
    fn start_for(&self, line: usize) -> (usize, LineState) {
        let i = self.checkpoints.partition_point(|(l, _)| *l <= line).saturating_sub(1);
        let (mut at, mut state) = match self.checkpoints.get(i) {
            Some((l, s)) => (*l, s.clone()),
            None => (0, LineState::start(self.grammar.syntax(self.syntax))),
        };
        if let Some((&l, (_, end))) = self.cache.range(..line).next_back() {
            if l + 1 > at {
                at = l + 1;
                state = end.clone();
            }
        }
        (at, state)
    }

    /// A walk reached line `at` in `state`: an old checkpoint there with
    /// the same state means the rest of the old ones hold again - they are
    /// taken back (`true`).
    fn converge(&mut self, at: usize, state: &LineState) -> bool {
        self.stale.retain(|(l, _)| *l >= at);
        let here = matches!(self.stale.first(), Some((l, _)) if *l == at);
        if !here {
            return false;
        }
        if self.stale[0].1 != *state {
            self.stale.remove(0);
            return false;
        }
        let back: Vec<(usize, LineState)> = self.stale.drain(..).collect();
        self.checkpoints.retain(|(l, _)| *l < at);
        self.checkpoints.extend(back);
        true
    }

    /// A checkpoint at `at` when the last one is `EVERY` lines above.
    fn checkpoint(&mut self, at: usize, state: &LineState) {
        let last = self.checkpoints.last().map_or(0, |(l, _)| *l);
        if at >= last + EVERY {
            self.checkpoints.push((at, state.clone()));
        }
    }

    /// The cache kept to [`CACHE_LINES`] lines around `around`.
    fn trim_cache(&mut self, around: usize) {
        if self.cache.len() <= CACHE_LINES {
            return;
        }
        let low = around.saturating_sub(CACHE_LINES / 2);
        let high = around + CACHE_LINES / 2;
        self.cache.retain(|l, _| *l >= low && *l <= high);
    }
}

impl HighlightJob {
    /// Parses the job's lines (on any thread).
    pub fn run(self) -> JobResult {
        let mut state = self.state;
        let mut checkpoints = Vec::new();
        for (k, text) in self.lines.iter().enumerate() {
            let at = self.first + k;
            if k > 0 && at % EVERY == 0 {
                checkpoints.push((at, state.clone()));
            }
            state = self.grammar.parse_line(&state, text).1;
        }
        checkpoints.push((self.first + self.lines.len(), state));
        JobResult {
            checkpoints,
            generation: self.generation,
        }
    }
}

/// A run added (merged into the one before when it continues it; plain
/// runs are not kept).
fn push_span(spans: &mut Vec<CodeSpan>, start: usize, end: usize, kind: TokenKind) {
    if kind == TokenKind::Plain || start >= end {
        return;
    }
    let start = u32::try_from(start).unwrap_or(u32::MAX);
    let end = u32::try_from(end).unwrap_or(u32::MAX);
    if let Some(last) = spans.last_mut() {
        if last.kind == kind && last.end == start {
            last.end = end;
            return;
        }
    }
    spans.push(CodeSpan { start, end, kind });
}

/// The scope prefixes and what they are.
#[derive(Debug)]
struct Rules {
    comment: Option<Scope>,
    string: Option<Scope>,
    list: Vec<(Scope, TokenKind)>,
}

impl Rules {
    /// The scope prefixes of syntect's grammars and the kinds they mean.
    fn new() -> Rules {
        use TokenKind as K;
        let table: [(&str, TokenKind); 30] = [
            ("constant.numeric", K::Number),
            ("constant.character", K::StringLiteral),
            ("constant", K::Constant),
            ("keyword.operator", K::Operator),
            ("keyword", K::Keyword),
            ("storage", K::Keyword),
            ("entity.name.function", K::Function),
            ("support.function", K::Function),
            ("variable.function", K::Function),
            ("entity.name.type", K::Type),
            ("entity.name.struct", K::Type),
            ("entity.name.enum", K::Type),
            ("entity.name.class", K::Type),
            ("entity.name.trait", K::Type),
            ("support.type", K::Type),
            ("support.class", K::Type),
            ("entity.name.tag", K::Tag),
            ("entity.other.attribute-name", K::Attribute),
            ("meta.annotation", K::Attribute),
            ("meta.attribute", K::Attribute),
            ("support.macro", K::Macro),
            ("entity.name.macro", K::Macro),
            ("markup.heading", K::Heading),
            ("entity.name.section", K::Heading),
            ("markup.underline.link", K::Link),
            ("invalid", K::Invalid),
            ("variable.parameter", K::Variable),
            ("variable", K::Variable),
            ("punctuation", K::Punctuation),
            ("entity.name", K::Type),
        ];
        Rules {
            comment: Scope::new("comment").ok(),
            string: Scope::new("string").ok(),
            list: table
                .iter()
                .filter_map(|(name, kind)| Scope::new(name).ok().map(|s| (s, *kind)))
                .collect(),
        }
    }
}

#[cfg(test)]
#[path = "highlight_test.rs"]
mod tests;
