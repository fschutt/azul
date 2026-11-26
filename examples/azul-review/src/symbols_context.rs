//! What the agent is told about the code the reviewer named, in a few lines the prompt can carry: the
//! DEFINITIONS found for certain (the doc comment and the lines of each, how the words found it), the
//! ones that did not fit the budget as a place (`file:line`), what changed under the index since
//! (a place that no longer holds the name is told as such, never shown as text), and, for what is
//! only hinted at, one short line each: roughly what to search for ([`crate::symbols_resolve::Hint`]).
//! An agent that gets this does not grep for `CssError`; one that gets a hint greps for `css.?error`.
//!
//! A snippet is read from the file as it is NOW ([`FileSource`]) and checked: the line the index
//! remembers must still hold the name, else the definition is `stale`. It is the in-memory index's only
//! cost of being rough. An excerpt names where it is from (`file:first-last`), the way azul-doc's
//! context bundles pin each excerpt to its source.
//!
//! [`SymbolsSection::render_prompt`] is the text for the agent-backends' prompt; [`context_around`]
//! is the page being reviewed, when a request does not carry it. Pure over a [`FileSource`], bounded
//! by [`ContextBudget`] (characters of text, lines of a snippet, number and length of hints).

use std::collections::HashSet;

use crate::{
    symbols::{DefInfo, Kind},
    symbols_resolve::{Certain, Hint, Resolution, Scope},
    symbols_source::{FileSource, MAX_FILE_BYTES},
    symbols_spoken::{VIA_DIGITS, VIA_HOMOPHONE, VIA_IDENTIFIER, VIA_JOINED, VIA_LETTERS},
};

/// The longest line of a snippet, characters.
const MAX_LINE_CHARS: usize = 200;

/// How much the section may be.
#[derive(Clone, Debug)]
pub struct ContextBudget {
    /// Characters of snippet text in all (the best definition always has some).
    pub chars: usize,
    /// Lines of one snippet's definition.
    pub max_lines: usize,
    /// Hints told.
    pub max_hints: usize,
    /// Characters of a hint's line.
    pub max_hint_chars: usize,
    /// Lines of comment or attribute above a definition that come with it.
    pub doc_lines: usize,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self { chars: 6_000, max_lines: 40, max_hints: 4, max_hint_chars: 220, doc_lines: 6 }
    }
}

/// A definition with its text.
#[derive(Clone, Debug, PartialEq)]
pub struct ContextSnippet {
    pub name: String,
    pub kind: Kind,
    pub file: String,
    /// The first line of the text: the doc comment's.
    pub line_start: usize,
    /// The last line of the definition.
    pub line_end: usize,
    pub text: String,
    /// What the reviewer said and how it was found.
    pub why: String,
}

/// A definition as a place only.
#[derive(Clone, Debug, PartialEq)]
pub struct Pointer {
    pub name: String,
    pub kind: Kind,
    pub file: String,
    pub line: usize,
}

/// One hint as a line of the prompt.
#[derive(Clone, Debug, PartialEq)]
pub struct HintLine {
    pub text: String,
}

/// The part of the agent's prompt about the symbols the reviewer named.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SymbolsSection {
    pub snippets: Vec<ContextSnippet>,
    /// Found, but the budget has no room for the text.
    pub pointers: Vec<Pointer>,
    /// Found once; the file has changed since (or cannot be read): look for it by name.
    pub stale: Vec<Pointer>,
    pub hints: Vec<HintLine>,
    /// What was heard and what it most likely was, when they differ.
    pub heard: Option<(String, String)>,
}

fn clip(line: &str) -> String {
    if line.chars().count() <= MAX_LINE_CHARS {
        line.to_string()
    } else {
        let mut cut: String = line.chars().take(MAX_LINE_CHARS).collect();
        cut.push('…');
        cut
    }
}

/// `text` in at most `max` characters, an ellipsis ending what was cut.
fn cut_to(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn is_comment_line(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("//") || t.starts_with("/*") || t.starts_with('*') || t.starts_with('#') || t.starts_with('@')
}

fn via_text(via: u8, repaired: bool) -> &'static str {
    if repaired {
        "one word the recogniser doubted was corrected"
    } else if via & VIA_LETTERS != 0 {
        "spelled-out letters"
    } else if via & VIA_HOMOPHONE != 0 {
        "a usual mishearing of a code word"
    } else if via & VIA_DIGITS != 0 {
        "number words"
    } else if via & VIA_IDENTIFIER != 0 {
        "written as an identifier"
    } else if via & VIA_JOINED != 0 {
        "written as one word"
    } else {
        "the words as heard"
    }
}

fn scope_text(scope: Scope) -> &'static str {
    match scope {
        Scope::File => "in this file",
        Scope::Crate => "the only one in this package",
        Scope::Recent => "named before in this conversation",
        Scope::Workspace => "the only one in the workspace",
    }
}

fn pointer_of(def: &DefInfo) -> Pointer {
    Pointer { name: def.name.clone(), kind: def.kind, file: def.file.clone(), line: def.line as usize }
}

fn read_lines(source: &dyn FileSource, file: &str) -> Option<Vec<String>> {
    let bytes = source.read(file, MAX_FILE_BYTES).ok()?;
    if bytes[..bytes.len().min(8192)].contains(&0) {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).lines().map(clip).collect())
}

/// The text of `certain`'s definition as the file has it now, or `None` when the line no longer holds the name.
fn snippet(certain: &Certain, heard: &str, lines: &[String], budget: &ContextBudget) -> Option<ContextSnippet> {
    let def = &certain.def;
    let at = (def.line as usize).checked_sub(1)?;
    let first = lines.get(at)?;
    if !first.contains(&def.name) {
        return None;
    }
    let mut start = at;
    while start > 0 && at - start < budget.doc_lines && is_comment_line(&lines[start - 1]) {
        start -= 1;
    }
    let extent = (def.lines as usize).max(1);
    let end = (at + extent).min(lines.len());
    let shown_end = end.min(at + budget.max_lines.max(1));
    let mut text = lines[start..shown_end].join("\n");
    if shown_end < end {
        text.push_str(&format!("\n… {} more lines", end - shown_end));
    }
    let why = format!("said \"{heard}\" ({}; {})", via_text(certain.via, certain.repaired), scope_text(certain.scope));
    Some(ContextSnippet { name: def.name.clone(), kind: def.kind, file: def.file.clone(), line_start: start + 1, line_end: end, text, why })
}

fn hint_line(h: &Hint, budget: &ContextBudget) -> HintLine {
    let mut line = format!("- heard \"{}\": search (case-insensitive) {}", h.said, h.patterns.join(" | "));
    if !h.kinds.is_empty() {
        let kinds: Vec<&str> = h.kinds.iter().map(|k| k.word()).collect();
        line.push_str(&format!("; kind: {}", kinds.join("/")));
    }
    if !h.candidates.is_empty() {
        let places: Vec<String> = h.candidates.iter().map(|d| format!("{} ({}) {}:{}", d.name, d.kind.word(), d.file, d.line)).collect();
        line.push_str(&format!("; named so in: {}", places.join(", ")));
    }
    if !h.similar.is_empty() {
        line.push_str(&format!("; similar names: {}", h.similar.join(", ")));
    }
    if !h.folders.is_empty() {
        line.push_str(&format!("; in: {}", h.folders.join(", ")));
    }
    HintLine { text: cut_to(&line, budget.max_hint_chars) }
}

/// The section for a resolution: the definitions found, read from `source`, within the budget.
#[must_use]
pub fn build_section(resolution: &Resolution, source: &dyn FileSource, budget: &ContextBudget) -> SymbolsSection {
    let mut section = SymbolsSection::default();
    let mut order: Vec<&Certain> = resolution.certain.iter().collect();
    order.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let mut seen: HashSet<(String, u32)> = HashSet::new();
    let mut used = 0usize;
    for c in order {
        if !seen.insert((c.def.file.clone(), c.def.line)) {
            continue;
        }
        let heard = resolution.corrections.iter().find(|x| x.words == c.words).map_or("", |x| x.from.as_str());
        let found = if c.def.kind == Kind::File {
            None
        } else {
            read_lines(source, &c.def.file).and_then(|lines| snippet(c, heard, &lines, budget))
        };
        match found {
            Some(mut sn) => {
                if section.snippets.is_empty() {
                    sn.text = cut_to(&sn.text, budget.chars);
                    used += sn.text.len();
                    section.snippets.push(sn);
                } else if used + sn.text.len() <= budget.chars {
                    used += sn.text.len();
                    section.snippets.push(sn);
                } else {
                    section.pointers.push(pointer_of(&c.def));
                }
            }
            None => section.stale.push(pointer_of(&c.def)),
        }
    }
    section.hints = resolution.hints.iter().take(budget.max_hints).map(|h| hint_line(h, budget)).collect();
    if resolution.corrected != resolution.raw {
        section.heard = Some((resolution.raw.clone(), resolution.corrected.clone()));
    }
    section
}

impl SymbolsSection {
    /// Nothing to tell.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.snippets.is_empty() && self.pointers.is_empty() && self.stale.is_empty() && self.hints.is_empty()
    }

    /// The characters the prompt part takes.
    #[must_use]
    pub fn chars(&self) -> usize {
        self.render_prompt().chars().count()
    }

    /// The text for the agent's prompt: the definitions found (do not search for them), where the rest
    /// is, what moved, roughly what to search for, and how the words were heard. Empty when there is nothing.
    #[must_use]
    pub fn render_prompt(&self) -> String {
        let mut out = String::new();
        if !self.snippets.is_empty() || !self.pointers.is_empty() {
            out.push_str("Definitions found in the workspace for what the reviewer said (matched by name, not guessed; do not search for them):\n");
            for sn in &self.snippets {
                out.push_str(&format!("- {} ({}) {}:{}-{}, {}\n```\n{}\n```\n", sn.name, sn.kind.word(), sn.file, sn.line_start, sn.line_end, sn.why, sn.text));
            }
            if !self.pointers.is_empty() {
                let places: Vec<String> = self.pointers.iter().map(|p| format!("{} ({}) {}:{}", p.name, p.kind.word(), p.file, p.line)).collect();
                out.push_str(&format!("Also found, by place only: {}\n", places.join("; ")));
            }
        }
        if !self.stale.is_empty() {
            let places: Vec<String> = self.stale.iter().map(|p| format!("{} ({}) {}:{}", p.name, p.kind.word(), p.file, p.line)).collect();
            out.push_str(&format!("Found a moment ago, but the file has changed since (look for them by name): {}\n", places.join("; ")));
        }
        if !self.hints.is_empty() {
            out.push_str("Not certain, roughly what the reviewer may mean (patterns to search for, not definitions found):\n");
            for h in &self.hints {
                out.push_str(&h.text);
                out.push('\n');
            }
        }
        if !out.is_empty() {
            if let Some((raw, corrected)) = &self.heard {
                out.push_str(&format!("The words were heard as \"{raw}\" and most likely were \"{corrected}\".\n"));
            }
        }
        out
    }
}

/// The lines around `line` (1-based) of `file`, `radius` before and after, each with its number: what the
/// reviewer is looking at, for a request that does not carry it. `None` when the file cannot be read as
/// text or has no such line.
#[must_use]
pub fn context_around(source: &dyn FileSource, file: &str, line: usize, radius: usize) -> Option<String> {
    let lines = read_lines(source, file)?;
    if line == 0 || line > lines.len() {
        return None;
    }
    let from = line.saturating_sub(radius).max(1);
    let to = (line + radius).min(lines.len());
    Some((from..=to).map(|n| format!("{n:>4}: {}", lines[n - 1])).collect::<Vec<_>>().join("\n"))
}

#[cfg(test)]
#[path = "symbols_context_test.rs"]
mod symbols_context_test;
