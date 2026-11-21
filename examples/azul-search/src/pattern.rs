//! What a search looks for: a [`Pattern`] - its text, how the text is read ([`PatternKind`]),
//! whether case matters ([`Case`]) - compiled into a [`NameMatcher`] for names or a
//! [`ContentMatcher`] for the lines of files.

use std::fmt;

use globset::{GlobBuilder, GlobMatcher};
use grep_matcher::Matcher;
use grep_regex::{RegexMatcher, RegexMatcherBuilder};

/// How the text of a [`Pattern`] is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PatternKind {
    /// As it is: a part of a name (`report` finds `Q3 Report.pdf`), a literal in a line.
    #[default]
    Literal,
    /// A shell glob over the whole name (`*.rs`, `report-??.txt`, `{draft,final}*`); with a `/`
    /// in it, over the path below the searched folder (`src/**/*.rs`). Names only.
    Glob,
    /// A regular expression (the `regex` crate's syntax).
    Regex,
}

/// Whether upper and lower case differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Case {
    /// Never (a file manager's search box).
    #[default]
    Insensitive,
    /// Always.
    Sensitive,
    /// When the pattern holds an upper-case letter (ripgrep's `--smart-case`).
    Smart,
}

/// What a search looks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    pub text: String,
    pub kind: PatternKind,
    pub case: Case,
    /// The match stands as a whole word: no letter, digit or `_` right before or after it.
    pub whole_word: bool,
}

impl Pattern {
    fn of(text: impl Into<String>, kind: PatternKind) -> Pattern {
        Pattern {
            text: text.into(),
            kind,
            case: Case::default(),
            whole_word: false,
        }
    }

    /// `text` as it is, without case.
    #[must_use]
    pub fn literal(text: impl Into<String>) -> Pattern {
        Pattern::of(text, PatternKind::Literal)
    }

    /// A shell glob, without case.
    #[must_use]
    pub fn glob(text: impl Into<String>) -> Pattern {
        Pattern::of(text, PatternKind::Glob)
    }

    /// A regular expression, without case.
    #[must_use]
    pub fn regex(text: impl Into<String>) -> Pattern {
        Pattern::of(text, PatternKind::Regex)
    }

    /// What a search box's text most likely means: a glob when it holds `*`, `?` or `[`
    /// (Explorer's wildcards: `*.pdf`), else the text as it is.
    #[must_use]
    pub fn guess(text: impl Into<String>) -> Pattern {
        let text = text.into();
        let kind = if text.contains(|c: char| matches!(c, '*' | '?' | '[')) {
            PatternKind::Glob
        } else {
            PatternKind::Literal
        };
        Pattern::of(text, kind)
    }

    #[must_use]
    pub fn with_case(mut self, case: Case) -> Pattern {
        self.case = case;
        self
    }

    #[must_use]
    pub fn with_whole_word(mut self, yes: bool) -> Pattern {
        self.whole_word = yes;
        self
    }
}

/// Why a pattern or a filter glob does not compile - a sentence for the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternError {
    pub message: String,
}

impl PatternError {
    pub(crate) fn new(message: impl Into<String>) -> PatternError {
        PatternError {
            message: message.into(),
        }
    }
}

impl fmt::Display for PatternError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PatternError {}

/// The empty pattern is refused: it would match every name and every line.
fn check_not_empty(pattern: &Pattern) -> Result<(), PatternError> {
    if pattern.text.is_empty() {
        Err(PatternError::new("there is nothing to look for"))
    } else {
        Ok(())
    }
}

/// `grep-regex`'s matcher for a literal or a regular expression; `line_terminator` set for the
/// lines of files (a pattern with a line break in it is then refused).
fn regex_matcher(
    pattern: &Pattern,
    line_terminator: Option<u8>,
) -> Result<RegexMatcher, PatternError> {
    let mut builder = RegexMatcherBuilder::new();
    builder
        .fixed_strings(pattern.kind == PatternKind::Literal)
        .word(pattern.whole_word)
        .line_terminator(line_terminator);
    match pattern.case {
        Case::Insensitive => {
            builder.case_insensitive(true);
        }
        Case::Sensitive => {}
        Case::Smart => {
            builder.case_smart(true);
        }
    }
    builder.build(&pattern.text).map_err(|e| {
        PatternError::new(match pattern.kind {
            PatternKind::Regex => format!("\"{}\" is not a regular expression: {e}", pattern.text),
            _ => format!("\"{}\" cannot be searched for: {e}", pattern.text),
        })
    })
}

/// A name pattern, compiled.
#[derive(Debug, Clone)]
pub struct NameMatcher {
    how: NameHow,
}

#[derive(Debug, Clone)]
enum NameHow {
    /// A literal or a regular expression, against the name.
    Text(RegexMatcher),
    /// A glob against the name - or, with a `/` in it, against the path.
    Glob { glob: GlobMatcher, path: bool },
}

impl NameMatcher {
    /// # Errors
    ///
    /// An empty pattern, a glob or a regular expression that does not compile.
    pub fn new(pattern: &Pattern) -> Result<NameMatcher, PatternError> {
        check_not_empty(pattern)?;
        if pattern.kind != PatternKind::Glob {
            return Ok(NameMatcher {
                how: NameHow::Text(regex_matcher(pattern, None)?),
            });
        }
        let insensitive = match pattern.case {
            Case::Insensitive => true,
            Case::Sensitive => false,
            Case::Smart => !pattern.text.chars().any(char::is_uppercase),
        };
        let glob = GlobBuilder::new(&pattern.text)
            .case_insensitive(insensitive)
            .literal_separator(true)
            .backslash_escape(true)
            .build()
            .map_err(|e| PatternError::new(format!("\"{}\" is not a glob: {e}", pattern.text)))?
            .compile_matcher();
        Ok(NameMatcher {
            how: NameHow::Glob {
                glob,
                path: pattern.text.contains('/'),
            },
        })
    }

    /// Where the pattern matches `name`, the name of the item at `path` (its `/`-separated path
    /// below the searched folder, without a folder's trailing `/`): the bytes of `name` it
    /// covers - all of them for a glob.
    #[must_use]
    pub fn find(&self, name: &str, path: &str) -> Option<(usize, usize)> {
        match &self.how {
            NameHow::Text(matcher) => matcher
                .find(name.as_bytes())
                .ok()
                .flatten()
                .map(|m| (m.start(), m.end())),
            NameHow::Glob { glob, path: true } => glob.is_match(path).then_some((0, name.len())),
            NameHow::Glob { glob, path: false } => {
                glob.is_match(name).then_some((0, name.len()))
            }
        }
    }
}

/// A content pattern, compiled for lines (`\n`-terminated; a `\r` before it is part of the
/// line, so a CRLF file's lines match as an LF file's do, but for `$`).
#[derive(Debug, Clone)]
pub struct ContentMatcher {
    pub(crate) matcher: RegexMatcher,
}

impl ContentMatcher {
    /// # Errors
    ///
    /// An empty pattern, a glob (globs match names), a regular expression that does not
    /// compile or that holds a line break.
    pub fn new(pattern: &Pattern) -> Result<ContentMatcher, PatternError> {
        check_not_empty(pattern)?;
        if pattern.kind == PatternKind::Glob {
            return Err(PatternError::new(
                "a glob matches names; file contents take a literal or a regular expression",
            ));
        }
        Ok(ContentMatcher {
            matcher: regex_matcher(pattern, Some(b'\n'))?,
        })
    }

    /// The matches in `line` (a line without its line break), as byte ranges.
    #[must_use]
    pub fn find_all(&self, line: &str) -> Vec<(usize, usize)> {
        let mut ranges = Vec::new();
        let _ = self.matcher.find_iter(line.as_bytes(), |m| {
            ranges.push((m.start(), m.end()));
            true
        });
        ranges
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_search_box_text_with_a_wildcard_is_a_glob_and_any_other_text_is_literal() {
        assert_eq!(Pattern::guess("*.pdf").kind, PatternKind::Glob);
        assert_eq!(Pattern::guess("report-??.txt").kind, PatternKind::Glob);
        assert_eq!(Pattern::guess("[ab]*").kind, PatternKind::Glob);
        assert_eq!(Pattern::guess("Q3 report").kind, PatternKind::Literal);
        assert_eq!(Pattern::guess("a.b+c").kind, PatternKind::Literal);
        assert_eq!(Pattern::guess("x").case, Case::Insensitive, "without case by default");
    }

    #[test]
    fn a_literal_name_matches_a_part_of_the_name_without_case() {
        let m = NameMatcher::new(&Pattern::literal("report")).expect("compiles");
        assert_eq!(m.find("Q3 Report.pdf", "docs/Q3 Report.pdf"), Some((3, 9)));
        assert_eq!(m.find("notes.txt", "notes.txt"), None);
        let dotted = NameMatcher::new(&Pattern::literal("a.b")).expect("compiles");
        assert!(dotted.find("a.b.txt", "a.b.txt").is_some());
        assert!(dotted.find("axb.txt", "axb.txt").is_none(), "a literal dot is a dot");
    }

    #[test]
    fn smart_case_minds_an_upper_case_letter_and_sensitive_case_always_does() {
        let smart = |text: &str| NameMatcher::new(&Pattern::literal(text).with_case(Case::Smart));
        assert!(smart("report").expect("compiles").find("REPORT.md", "REPORT.md").is_some());
        assert!(smart("Report").expect("compiles").find("report.md", "report.md").is_none());
        assert!(smart("Report").expect("compiles").find("Report.md", "Report.md").is_some());
        let sensitive = NameMatcher::new(&Pattern::literal("report").with_case(Case::Sensitive))
            .expect("compiles");
        assert!(sensitive.find("Report.md", "Report.md").is_none());
        let glob = NameMatcher::new(&Pattern::glob("*.MD").with_case(Case::Smart)).expect("ok");
        assert!(glob.find("a.md", "a.md").is_none(), "an upper-case glob minds case");
    }

    #[test]
    fn a_glob_matches_the_whole_name_and_a_glob_with_a_slash_matches_the_path() {
        let md = NameMatcher::new(&Pattern::glob("*.md")).expect("compiles");
        assert_eq!(md.find("Report.MD", "docs/Report.MD"), Some((0, 9)));
        assert!(md.find("report.md.bak", "report.md.bak").is_none(), "the whole name");
        let deep = NameMatcher::new(&Pattern::glob("docs/**/*.txt")).expect("compiles");
        assert!(deep.find("b.txt", "docs/a/b.txt").is_some());
        assert!(deep.find("b.txt", "other/a/b.txt").is_none());
        let direct = NameMatcher::new(&Pattern::glob("docs/*.txt")).expect("compiles");
        assert!(direct.find("b.txt", "docs/a/b.txt").is_none(), "* stops at a /");
    }

    #[test]
    fn a_regex_name_matches_where_the_expression_does() {
        let m = NameMatcher::new(&Pattern::regex(r"^report-\d+\.txt$")).expect("compiles");
        assert!(m.find("report-12.txt", "a/report-12.txt").is_some());
        assert!(m.find("report-x.txt", "report-x.txt").is_none());
    }

    #[test]
    fn a_pattern_that_cannot_work_says_why() {
        assert!(NameMatcher::new(&Pattern::regex("(")).is_err());
        assert!(NameMatcher::new(&Pattern::glob("{a,b")).is_err());
        assert!(NameMatcher::new(&Pattern::literal("")).is_err());
        assert!(ContentMatcher::new(&Pattern::glob("*.rs")).is_err(), "globs match names");
        assert!(ContentMatcher::new(&Pattern::regex("a\nb")).is_err(), "one line at a time");
        let e = NameMatcher::new(&Pattern::regex("(")).expect_err("does not compile");
        assert!(e.to_string().contains("is not a regular expression"), "{e}");
    }

    #[test]
    fn a_content_pattern_finds_every_match_in_a_line_and_whole_words_on_request() {
        let any = ContentMatcher::new(&Pattern::literal("rent")).expect("compiles");
        assert_eq!(any.find_all("rental Rent"), vec![(0, 4), (7, 11)]);
        let word = ContentMatcher::new(&Pattern::literal("rent").with_whole_word(true))
            .expect("compiles");
        assert_eq!(word.find_all("rental rent"), vec![(7, 11)]);
        let regex = ContentMatcher::new(&Pattern::regex(r"fn \w+")).expect("compiles");
        assert_eq!(regex.find_all("pub fn picked() {}"), vec![(4, 13)]);
    }
}
