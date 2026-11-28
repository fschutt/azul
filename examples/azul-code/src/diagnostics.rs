//! What AzCode says about its own work, for the bottom panel's two lists: the PROBLEMS (a file that
//! could not be read or saved, a folder that could not be listed or searched, a terminal that would
//! not start) and the OUTPUT (what AzCode did: a folder opened, a file saved, a search run, a task
//! typed into the terminal). There is no language server in the tree to find problems in the code:
//! these are the problems of the workspace's operations, and each names its file when it has one, so
//! a click opens it. Plain Rust; both lists are capped and a cap is told, never a crash.

use std::collections::VecDeque;

/// The most problems kept (the oldest go).
pub const MAX_PROBLEMS: usize = 200;
/// The most lines of output kept (the oldest go).
pub const MAX_OUTPUT: usize = 1000;
/// A line or a message is cut to this many characters.
pub const MAX_CHARS: usize = 400;

/// How bad a problem is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    /// The Material icon of its row.
    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }

    /// The key of the word that names it (`azcode-severity-error`), for a screen reader.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Severity::Error => "azcode-severity-error",
            Severity::Warning => "azcode-severity-warning",
            Severity::Info => "azcode-severity-info",
        }
    }
}

/// One problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub severity: Severity,
    pub message: String,
    /// The file it is about, as a workspace key (`src/main.rs`).
    pub file: Option<String>,
    /// The line in it (counted from 0), when the problem has one.
    pub line: Option<usize>,
}

/// The two lists, and how many entries each has let go of at its cap (the panel says so under the list).
#[derive(Debug, Clone, Default)]
pub struct Diagnostics {
    problems: Vec<Problem>,
    output: VecDeque<String>,
    dropped_problems: usize,
    dropped_output: usize,
}

/// `text` cut to [`MAX_CHARS`] characters with an ellipsis, a line break turned into a space.
fn tidy(text: &str) -> String {
    let flat: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let flat = flat.trim();
    if flat.chars().count() <= MAX_CHARS {
        return flat.to_string();
    }
    let mut cut: String = flat.chars().take(MAX_CHARS - 1).collect();
    cut.push('\u{2026}');
    cut
}

impl Diagnostics {
    /// A problem found: listed (the same problem again moves to the end instead of standing twice)
    /// and said in the output too.
    pub fn report(&mut self, severity: Severity, message: &str, file: Option<&str>, line: Option<usize>) {
        let problem = Problem {
            severity,
            message: tidy(message),
            file: file.map(str::to_string),
            line,
        };
        self.say(&problem.message);
        self.problems.retain(|p| *p != problem);
        self.problems.push(problem);
        if self.problems.len() > MAX_PROBLEMS {
            let extra = self.problems.len() - MAX_PROBLEMS;
            self.problems.drain(..extra);
            self.dropped_problems = self.dropped_problems.saturating_add(extra);
        }
    }

    /// An error about `file` (a key), or about nothing in particular.
    pub fn error(&mut self, message: &str, file: Option<&str>) {
        self.report(Severity::Error, message, file, None);
    }

    /// A line of output.
    pub fn say(&mut self, line: &str) {
        self.output.push_back(tidy(line));
        while self.output.len() > MAX_OUTPUT {
            self.output.pop_front();
            self.dropped_output = self.dropped_output.saturating_add(1);
        }
    }

    /// The problems, the oldest first.
    #[must_use]
    pub fn problems(&self) -> &[Problem] {
        &self.problems
    }

    /// The lines of output, the oldest first.
    pub fn output(&self) -> impl Iterator<Item = &str> {
        self.output.iter().map(String::as_str)
    }

    /// How many problems the cap has let go of since the list was last cleared.
    #[must_use]
    pub const fn dropped_problems(&self) -> usize {
        self.dropped_problems
    }

    /// How many lines of output the cap has let go of since the list was last cleared.
    #[must_use]
    pub const fn dropped_output(&self) -> usize {
        self.dropped_output
    }

    /// How many lines of output there are.
    #[must_use]
    pub fn output_len(&self) -> usize {
        self.output.len()
    }

    /// How many problems have `severity`.
    #[must_use]
    pub fn count(&self, severity: Severity) -> usize {
        self.problems.iter().filter(|p| p.severity == severity).count()
    }

    /// Every problem forgotten (the Problems list's Clear).
    pub fn clear_problems(&mut self) {
        self.problems.clear();
        self.dropped_problems = 0;
    }

    /// The output forgotten (the Output list's Clear).
    pub fn clear_output(&mut self) {
        self.output.clear();
        self.dropped_output = 0;
    }
}

#[cfg(test)]
#[path = "diagnostics_test.rs"]
mod tests;
