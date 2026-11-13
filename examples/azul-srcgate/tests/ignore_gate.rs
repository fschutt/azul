//! Gate: every `#[ignore]` of azul's apps says where it is tracked (X-DELTA-INTEGRITY3-17).
//!
//! azul-apps reads the `#[ignore]` attributes of its own tree (`g10_every_ignored_test_has_a_row`,
//! iso/crates/azctl/src/ctl/ci_gates_tests.rs: a row of iso/scripts/ignored-tests.tsv for each, and a
//! row of a test that is not ignored any more goes): an ignore is a decision, and a decision is a row -
//! "never ignore a test to turn a run green". Nothing read this repository's. A later lane that ignored a
//! failing test of the drive crates with no row was told by no gate; the two ignores added in one day
//! (`SYNC-E7`, `X-TESTEDGE-A-1`) were checked by a reviewer's eye.
//!
//! The rule, as g10 has it for its `open` and `planned` rows: every `#[ignore]` under `examples/` has a
//! reason, and the reason BEGINS with the id of the row that tracks it, then a colon: a register row (a
//! known red the ignore parks, a run nobody has made: `SYNC-E7`, `ENT27-M16`) or a row the run step
//! owns (`RUN-R9`). `#[ignore = "SYNC-E7: 20,000 files ..."]` passes; `#[ignore]`, `#[ignore = "flaky"]`
//! and `#[ignore = "see the notes"]` fail.
//!
//! WHAT IT DOES NOT DO: the register is azul-apps' file (notes/audits/closeout-2026-11/ISSUES.tsv); this
//! repository keeps no copy of it, and a test that read a file of the other checkout would pass or fail
//! by where it runs. So the gate checks the id's SHAPE - an upper-case first letter, letters and digits
//! joined by hyphens, at least one digit (`X-DELTA-INTEGRITY3-17`, `SYNC-E7`, `RUN-R9`) - and not that
//! the row exists or is open. That the id is a row, and that the ignore goes when its row closes, is
//! `issues_status.py`'s and the coordinator's. There is no count and no list of files that are let
//! off: every `#[ignore]` of `examples/`, in a test module, a test file or a helper, is read.
//!
//! What it reads is the raw source of every `.rs` file below `examples/` (a test file is where an
//! ignore lives, so the kit's shipped-code finders, which blank test items, are not used), through the
//! kit's lexer: a comment is never an attribute and a string that says `#[ignore]` is none.

use std::path::{Path, PathBuf};

use azul_srcgate::{locate, workspace, Located};

/// The tree it reads.
const ROOT: &str = "examples";

/// One `#[ignore]` as written, and where.
#[derive(Debug, PartialEq, Eq)]
struct Ignored {
    /// The function it is on (`?` when none follows).
    test: String,
    /// What it says (`None`: a bare `#[ignore]`).
    reason: Option<String>,
    /// The line of its `#` (from 1).
    line: usize,
}

/// The `#[ignore]` and `#[cfg_attr(.., ignore ..)]` attributes of `src`, with the function each is on.
fn ignored_in(src: &str) -> Vec<Ignored> {
    let toks = locate(src);
    let mut found = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        if !(toks[i].tok.is_punct('#') && toks.get(i + 1).is_some_and(|t| t.tok.is_punct('['))) {
            i += 1;
            continue;
        }
        let (end, inner) = attribute(&toks, i + 2);
        let first = inner.first().and_then(|t| t.tok.ident());
        let at = match first {
            Some("ignore") => Some(0),
            // `#[cfg_attr(feature = "x", ignore = "...")]` is an ignore too.
            Some("cfg_attr") => inner.iter().position(|t| t.tok.is_ident("ignore")),
            _ => None,
        };
        if let Some(at) = at {
            let reason = match (inner.get(at + 1), inner.get(at + 2)) {
                (
                    Some(eq),
                    Some(Located {
                        tok: azul_srcgate::Tok::Str(why),
                        ..
                    }),
                ) if eq.tok.is_punct('=') => Some(why.clone()),
                _ => None,
            };
            found.push(Ignored {
                test: function_after(&toks, end),
                reason,
                line: toks[i].line,
            });
        }
        i = end;
    }
    found
}

/// The tokens of the attribute whose body starts at `from` (just after `#[`), and the index after its
/// closing `]` (brackets nest).
fn attribute(toks: &[Located], from: usize) -> (usize, &[Located]) {
    let mut depth = 1;
    let mut at = from;
    while at < toks.len() && depth > 0 {
        if toks[at].tok.is_punct('[') {
            depth += 1;
        } else if toks[at].tok.is_punct(']') {
            depth -= 1;
        }
        at += 1;
    }
    (at, &toks[from..at.saturating_sub(1).max(from)])
}

/// The name of the function an attribute list ending at `from` is on: past further attributes and
/// the qualifiers, the identifier after `fn`.
fn function_after(toks: &[Located], from: usize) -> String {
    let mut at = from;
    while at < toks.len() && at < from + 400 {
        if toks[at].tok.is_punct('#') && toks.get(at + 1).is_some_and(|t| t.tok.is_punct('[')) {
            at = attribute(toks, at + 2).0;
        } else if toks[at].tok.is_ident("fn") {
            return toks
                .get(at + 1)
                .and_then(|t| t.tok.ident())
                .unwrap_or("?")
                .to_string();
        } else if toks[at].tok.ident().is_some()
            || toks[at].tok.is_punct('(')
            || toks[at].tok.is_punct(')')
        {
            at += 1;
        } else {
            break;
        }
    }
    String::from("?")
}

/// Whether `s` has the shape of the id of a row: an upper-case first letter, letters and digits
/// joined by single hyphens, at least one hyphen and one digit.
fn has_the_shape_of_an_id(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() >= 2
        && s.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric()))
        && s.chars().any(|c| c.is_ascii_digit())
}

/// What is wrong with one ignore, if anything.
fn wrong_with(ignored: &Ignored) -> Option<String> {
    let Some(reason) = ignored
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty())
    else {
        return Some(String::from("it has no reason"));
    };
    let Some((head, _why)) = reason.split_once(':') else {
        return Some(format!(
            "its reason does not begin with the id of the row that tracks it and a colon: {reason:?}"
        ));
    };
    (!has_the_shape_of_an_id(head.trim())).then(|| {
        format!(
            "{:?} is not the id of a row (`SYNC-E7`, `X-DELTA-FOO-9`, `RUN-R9`): {reason:?}",
            head.trim()
        )
    })
}

/// Every `.rs` file below `dir`, `target` and hidden folders left out; what cannot be read is a problem
/// of the gate, never a folder that is not there.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>, problems: &mut Vec<String>) {
    let read = match std::fs::read_dir(dir) {
        Ok(read) => read,
        Err(e) => {
            problems.push(format!("{}: the folder cannot be read: {e}", dir.display()));
            return;
        }
    };
    for entry in read {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                problems.push(format!("{}: an entry cannot be read: {e}", dir.display()));
                continue;
            }
        };
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        // The entry's own type: a link is followed by nothing here (a link to a folder is no loop).
        let Ok(kind) = entry.file_type() else {
            problems.push(format!("{}: its type cannot be read", path.display()));
            continue;
        };
        if kind.is_dir() {
            if name != "target" && !name.starts_with('.') {
                rust_files(&path, out, problems);
            }
        } else if kind.is_file() && name.ends_with(".rs") {
            out.push(path);
        }
    }
}

/// The ignores below `base/examples` that are wrong, as `file:line test: what` (what the gate could
/// not read among them), how many files were read, and every ignore as `file:line test`.
fn judge(base: &Path) -> (Vec<String>, usize, Vec<String>) {
    let mut files = Vec::new();
    let mut wrong = Vec::new();
    rust_files(&base.join(ROOT), &mut files, &mut wrong);
    files.sort();
    let mut seen = Vec::new();
    for file in &files {
        let src = match std::fs::read_to_string(file) {
            Ok(src) => src,
            Err(e) => {
                wrong.push(format!("{}: the file cannot be read: {e}", file.display()));
                continue;
            }
        };
        let rel = file
            .strip_prefix(base)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        for ignored in ignored_in(&src) {
            seen.push(format!("{rel}:{} {}", ignored.line, ignored.test));
            if let Some(why) = wrong_with(&ignored) {
                wrong.push(format!("{rel}:{} {}: {why}", ignored.line, ignored.test));
            }
        }
    }
    (wrong, files.len(), seen)
}

#[test]
fn every_ignored_test_of_the_apps_names_the_row_that_tracks_it() {
    let (wrong, files, seen) = judge(&workspace());
    assert!(files > 300, "the scan read {files} files: the wrong place");
    // The scan reaches test modules and test files: the ignore of the 20,000-file run is in `src/tests`.
    assert!(
        seen.iter()
            .any(|s| s.contains("azcloud-kit/src/tests/bulk.rs") && s.contains("twenty_thousand")),
        "the ignore of the 20,000-file test is not found: {} ignores read",
        seen.len()
    );
    assert!(
        wrong.is_empty(),
        "an `#[ignore]` says no row that tracks it (begin its reason with the id of its row and a colon, \
         `#[ignore = \"SYNC-E7: why\"]`; never ignore a test to turn a run green: that is an open row):\n{}",
        wrong.join("\n")
    );
}

/// The scanner on planted source: every spelling of an ignore is found, with its reason and its
/// function, and what only looks like one is not.
#[test]
fn the_scanner_reads_every_spelling_of_an_ignore_and_nothing_that_only_looks_like_one() {
    let src = "\
#[test]\n\
#[ignore = \"needs sqld\"]\n\
fn a() {}\n\
// #[ignore] fn in_a_comment() {}\n\
/// the doc says `#[ignore]` too\n\
const S: &str = \"#[ignore] fn in_a_string()\";\n\
#[cfg(test)]\n\
mod t {\n\
    #[tokio::test]\n\
    #[ignore]\n\
    async fn b() {}\n\
    #[test]\n\
    fn not_ignored() {}\n\
    #[test]\n\
    #[cfg_attr(unix, ignore = \"SYNC-E7: the run\")]\n\
    #[should_panic]\n\
    pub(crate) fn c() {}\n\
    #[ignore = \"\"]\n\
    #[test]\n\
    fn d() {}\n\
}\n";
    let found: Vec<(String, Option<String>)> = ignored_in(src)
        .into_iter()
        .map(|i| (i.test, i.reason))
        .collect();
    assert_eq!(
        found,
        vec![
            (String::from("a"), Some(String::from("needs sqld"))),
            (String::from("b"), None),
            (String::from("c"), Some(String::from("SYNC-E7: the run"))),
            (String::from("d"), Some(String::new())),
        ]
    );
}

/// The rule on planted ignores: no reason, a reason with no id and an id that is none fail; an ignore that
/// names its row passes. The same cases through the whole gate, on a planted tree.
#[test]
fn an_ignore_with_no_reason_or_no_id_fails_and_one_that_names_its_row_passes() {
    let tree = std::env::temp_dir().join(format!("azul-srcgate-ignore-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tree);
    let file = tree.join("examples/demo/src/tests.rs");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let write = |src: &str| std::fs::write(&file, src).unwrap();
    let judged = |src: &str| {
        write(src);
        judge(&tree).0
    };
    // A clean case.
    assert!(judged(
        "#[test]\n#[ignore = \"SYNC-E7: 20,000 files, run by the RUN step\"]\nfn ok() {}\n"
    )
    .is_empty());
    assert!(
        judged("#[test]\n#[ignore = \"X-DELTA-INTEGRITY3-17: parked\"]\nfn ok() {}\n").is_empty()
    );
    // No reason.
    let bare = judged("#[test]\n#[ignore]\nfn bare() {}\n");
    assert_eq!(bare.len(), 1, "{bare:?}");
    assert!(
        bare[0].contains("bare") && bare[0].contains("it has no reason"),
        "{bare:?}"
    );
    // A reason that names no row, in each way it can fail to.
    for why in [
        "flaky",
        "see the notes: later",
        "no-id: because",
        "TODO-LATER: soon",
        "e8: the owner's machine",
        "SYNC-E7 20,000 files",
        ": nothing before the colon",
    ] {
        let wrong = judged(&format!(
            "#[test]\n#[ignore = \"{why}\"]\nfn names_no_row() {{}}\n"
        ));
        assert_eq!(wrong.len(), 1, "{why:?}: {wrong:?}");
        assert!(wrong[0].contains("names_no_row"), "{wrong:?}");
    }
    // The ids the register has, in their shapes.
    for id in [
        "X-DELTA-INTEGRITY3-17",
        "SYNC-E7",
        "ENT27-M16",
        "X-EDGE-C-1",
        "RUN-R9",
        "TR29-gc-grace",
        "A1-1",
    ] {
        assert!(has_the_shape_of_an_id(id), "{id}");
    }
    for not in [
        "",
        "E8",
        "SYNC",
        "sync-e7",
        "SYNC--E7",
        "SYNC-E7 ",
        "X-DELTA-FOO",
        "-E7",
    ] {
        assert!(!has_the_shape_of_an_id(not), "{not:?}");
    }
    let _ = std::fs::remove_dir_all(&tree);
}
