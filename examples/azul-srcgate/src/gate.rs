//! A gate: where it reads, what it looks for, and where that may stand.
//!
//! - A [`Rule`] is data: what it finds in a file's tokens ([`crate::find`]), and its own cases -
//!   code it must find and code it must not. [`self_test`] runs every case through
//!   [`crate::code`] (in a function's body: found; in a test module or a comment: not found) and
//!   fails on a rule without cases, so a rule that matches nothing cannot pass for a clean tree.
//! - A [`Budget`] takes the hits: each allowance names a file (or a directory) and says why it
//!   may hold the rule, and how many times. A hit no allowance takes, an allowance whose count is
//!   not the code's and an allowance nothing uses any more all fail: the count only goes down,
//!   and a number that went down is written down.
//! - [`Gate::run`] does all of it in that order.

use std::path::{Path, PathBuf};

use crate::{
    find::{locate, Located},
    lex::code,
    read::{files, Code, Orphans, Root},
};

/// The workspace's root (azul-srcgate is `examples/azul-srcgate`).
///
/// # Panics
///
/// When the checkout the crate was built in is gone.
#[must_use]
pub fn workspace() -> PathBuf {
    let r = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    std::fs::canonicalize(&r).unwrap_or_else(|e| panic!("{}: {e}", r.display()))
}

/// What a rule finds in one file's tokens: the line (from 1) of each occurrence.
type Finder = Box<dyn Fn(&[Located]) -> Vec<usize>>;

/// A rule's mark in the code, with its own cases: code it must find and code it must not.
pub struct Rule {
    pub name: String,
    find: Finder,
    pub planted: Vec<String>,
    pub clean: Vec<String>,
}

impl Rule {
    /// A rule named `name` that `find` implements; `planted` are statements it must find (inside
    /// a function's body), `clean` statements it must not.
    pub fn new(
        name: &str,
        find: impl Fn(&[Located]) -> Vec<usize> + 'static,
        planted: &[&str],
        clean: &[&str],
    ) -> Rule {
        let owned = |v: &[&str]| v.iter().map(|s| (*s).to_string()).collect();
        Rule {
            name: name.to_string(),
            find: Box::new(find),
            planted: owned(planted),
            clean: owned(clean),
        }
    }

    /// The lines of `blanked` (a file's [`code`]) where it is, once per occurrence.
    #[must_use]
    pub fn find(&self, blanked: &str) -> Vec<usize> {
        (self.find)(&locate(blanked))
    }
}

/// Each rule's own cases, run through [`code`] as a file would be: every planted case is found
/// in a function's body and found neither in a test module nor in a comment; no clean case is
/// found. A rule without a planted or a clean case fails.
///
/// # Panics
///
/// With every rule that fails its cases.
pub fn self_test(rules: &[Rule]) {
    let mut wrong = Vec::new();
    assert!(!rules.is_empty(), "a gate without rules");
    for r in rules {
        if r.planted.is_empty() || r.clean.is_empty() {
            wrong.push(format!(
                "{}: a rule needs a planted and a clean case",
                r.name
            ));
        }
        for p in &r.planted {
            let body = format!("fn planted() {{\n{p}\n}}\n");
            if r.find(&code(&body)).is_empty() {
                wrong.push(format!("{}: misses its planted case {p:?}", r.name));
            }
            let tests = format!(
                "#[cfg(test)]\nmod tests {{\n    fn planted() {{\n{p}\n    }}\n}}\nfn after() {{}}\n"
            );
            if !r.find(&code(&tests)).is_empty() {
                wrong.push(format!("{}: finds {p:?} in a test module", r.name));
            }
            let commented: String = p.lines().map(|l| format!("// {l}\n")).collect();
            if !r.find(&code(&commented)).is_empty() {
                wrong.push(format!("{}: finds {p:?} in a comment", r.name));
            }
        }
        for c in &r.clean {
            let body = format!("fn clean() {{\n{c}\n}}\n");
            if !r.find(&code(&body)).is_empty() {
                wrong.push(format!("{}: finds its clean case {c:?}", r.name));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "the gate's rules fail their own cases:\n{}",
        wrong.join("\n")
    );
}

/// One occurrence of a rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub rel: String,
    pub line: usize,
    pub rule: String,
    /// The source's line, trimmed.
    pub text: String,
}

/// Every occurrence of every rule in `files`.
#[must_use]
pub fn hits(files: &[Code], rules: &[Rule]) -> Vec<Hit> {
    let mut out = Vec::new();
    for f in files {
        let tokens = f.tokens();
        for r in rules {
            for line in (r.find)(&tokens) {
                out.push(Hit {
                    rel: f.rel.clone(),
                    line,
                    rule: r.name.clone(),
                    text: f.source_line(line).to_string(),
                });
            }
        }
    }
    out
}

/// Where a rule may be broken, and why.
#[derive(Debug, Clone, Copy)]
pub struct Allow {
    /// A file from the base directory, or a directory ending in `/`.
    pub file: &'static str,
    /// One rule by its name; `None`: every rule.
    pub rule: Option<&'static str>,
    /// Exactly this many occurrences; `None`: any number, at least one.
    pub count: Option<usize>,
    pub why: &'static str,
}

impl Allow {
    /// Every rule in `file` (or under a directory ending in `/`), at least once.
    #[must_use]
    pub const fn file(file: &'static str, why: &'static str) -> Allow {
        Allow {
            file,
            rule: None,
            count: None,
            why,
        }
    }

    /// `rule` in `file`, at least once.
    #[must_use]
    pub const fn rule(file: &'static str, rule: &'static str, why: &'static str) -> Allow {
        Allow {
            file,
            rule: Some(rule),
            count: None,
            why,
        }
    }

    /// `rule` in `file` exactly `count` times: one more is a new site, one fewer a stale count.
    #[must_use]
    pub const fn exactly(
        file: &'static str,
        rule: &'static str,
        count: usize,
        why: &'static str,
    ) -> Allow {
        Allow {
            file,
            rule: Some(rule),
            count: Some(count),
            why,
        }
    }

    fn takes(&self, h: &Hit) -> bool {
        let in_file = match self.file.strip_suffix('/') {
            Some(dir) => h.rel.starts_with(&format!("{dir}/")),
            None => h.rel == self.file,
        };
        in_file && self.rule.is_none_or(|n| n == h.rule)
    }
}

/// The allowances of a gate: what may stand where, counted.
#[derive(Debug, Clone, Copy)]
pub struct Budget<'a>(pub &'a [Allow]);

impl Budget<'_> {
    /// What is wrong with `hits` under this budget: each hit no allowance takes, each allowance
    /// whose count is not the number of hits it takes, each allowance that takes none. Empty:
    /// the gate holds.
    #[must_use]
    pub fn judge(&self, hits: &[Hit]) -> Vec<String> {
        let mut wrong = Vec::new();
        let mut taken = vec![0usize; self.0.len()];
        for h in hits {
            match self.0.iter().position(|a| a.takes(h)) {
                Some(i) => taken[i] += 1,
                None => wrong.push(format!("{}:{}: {} ({})", h.rel, h.line, h.text, h.rule)),
            }
        }
        for (a, n) in self.0.iter().zip(&taken) {
            match a.count {
                Some(c) if c != *n => wrong.push(format!(
                    "{} holds {n} x {} where its allowance says {c} ({})",
                    a.file,
                    a.rule.unwrap_or("any rule"),
                    a.why
                )),
                None if *n == 0 => wrong.push(format!(
                    "{}: an allowance nothing uses any more ({}; {})",
                    a.file,
                    a.rule.unwrap_or("any rule"),
                    a.why
                )),
                _ => {}
            }
        }
        wrong
    }
}

/// A gate: where it reads, what it looks for, and where that may stand.
pub struct Gate<'a> {
    pub name: &'a str,
    pub roots: &'a [Root],
    pub rules: Vec<Rule>,
    pub budget: Budget<'a>,
    /// What to do with a `.rs` file under a root that no build compiles.
    pub orphans: Orphans,
}

impl Gate<'_> {
    /// The gate against the tree under `base`: the rules' own cases ([`self_test`]), then the
    /// files, their hits and the judgement. The hits when the gate holds, else every finding.
    ///
    /// # Errors
    ///
    /// The findings, one sentence each.
    ///
    /// # Panics
    ///
    /// When a rule fails its own cases.
    pub fn check(&self, base: &Path) -> Result<Vec<Hit>, Vec<String>> {
        self_test(&self.rules);
        let read = files(base, self.roots, self.orphans)?;
        let found = hits(&read, &self.rules);
        let wrong = self.budget.judge(&found);
        if wrong.is_empty() {
            Ok(found)
        } else {
            Err(wrong)
        }
    }

    /// [`Gate::check`] against this workspace; fails with every finding. The hits, for a gate's
    /// own further checks.
    ///
    /// # Panics
    ///
    /// With every finding.
    pub fn run(&self) -> Vec<Hit> {
        match self.check(&workspace()) {
            Ok(found) => found,
            Err(wrong) => panic!(
                "{}: {} finding(s):\n{}",
                self.name,
                wrong.len(),
                wrong.join("\n")
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::find::macro_calls;

    fn hit(rel: &str, rule: &str) -> Hit {
        Hit {
            rel: rel.into(),
            line: 1,
            rule: rule.into(),
            text: String::new(),
        }
    }

    fn todo_rule() -> Rule {
        Rule::new(
            "todo",
            |t| macro_calls(t, "todo"),
            &["todo!();"],
            &["let todo = 1;"],
        )
    }

    #[test]
    fn a_rule_finds_its_planted_case_and_not_its_clean_one_nor_in_tests_or_comments() {
        self_test(&[todo_rule()]);
        let src = "fn a() {\n    todo!(); todo!();\n}\n#[cfg(test)]\nmod t {\n    fn b() { todo!(); let _ = \"}\"; }\n}\n// todo!()\nfn c() {\n    todo!();\n}\n";
        assert_eq!(
            todo_rule().find(&code(src)),
            vec![2, 2, 10],
            "after the test module too, not in it"
        );
    }

    #[test]
    #[should_panic(expected = "a rule needs a planted and a clean case")]
    fn a_rule_without_its_cases_fails() {
        self_test(&[Rule::new("x", |t| macro_calls(t, "x"), &["x!()"], &[])]);
    }

    #[test]
    #[should_panic(expected = "misses its planted case")]
    fn a_rule_that_misses_its_planted_case_fails() {
        self_test(&[Rule::new("x", |t| macro_calls(t, "x"), &["y!()"], &["z"])]);
    }

    #[test]
    #[should_panic(expected = "finds its clean case")]
    fn a_rule_that_finds_its_clean_case_fails() {
        self_test(&[Rule::new(
            "x",
            |t| macro_calls(t, "x"),
            &["x!()"],
            &["x!(2)"],
        )]);
    }

    #[test]
    fn allowances_take_their_hits_count_them_and_go_stale() {
        let hits = vec![
            hit("a/durable.rs", "w"),
            hit("a/sink.rs", "w"),
            hit("a/sink.rs", "w"),
            hit("a/other.rs", "w"),
            hit("a/testing/x.rs", "v"),
        ];
        let allows = [
            Allow::file("a/durable.rs", "the helper"),
            Allow::exactly("a/sink.rs", "w", 2, "two sinks"),
            Allow::file("a/testing/", "the harness"),
        ];
        assert_eq!(
            Budget(&allows).judge(&hits),
            vec!["a/other.rs:1:  (w)".to_string()]
        );
        let fewer = [Allow::exactly("a/sink.rs", "w", 3, "three")];
        assert!(
            Budget(&fewer).judge(&hits[1..3])[0].contains("holds 2 x w where its allowance says 3")
        );
        let stale = [Allow::file("a/gone.rs", "gone")];
        assert!(Budget(&stale).judge(&[])[0].contains("nothing uses"));
    }
}
