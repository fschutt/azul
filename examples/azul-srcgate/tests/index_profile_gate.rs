//! Gate: an index provider is never built without saying where a restore of its drives is
//! announced (ARCH29 C6b, X-DELTA-AZDRIVE-SYNC-3; AZ-M4).
//!
//! `MetaIndexProvider::new` takes an `IndexProfile`, a struct with no `Default` whose `restore` and
//! `reclaim` fields every literal must write, so the type already keeps the app that leaves them out
//! from compiling: AzMail, the bridge and the command line built the provider with `new(name)` and no
//! `with_restore`, kept a persistent copy of the drive index, and refused a drive the operator
//! restored from its off-site copy, for good. What the type cannot see is the way out: a source
//! that is none on purpose. This gate keeps that way out visible:
//!
//! - `restore-none`: every call of `RestoreSource::none(` (and of `RestoreSource::default(`, the
//!   same thing without a reason) in shipped code carries, on its line or the line above,
//!   `// gate:restore-none allow: <why> [<register id>]` - one sentence and the row it belongs to.
//!   An annotation that sits by no such call fails too, so the reason cannot outlive the code.
//! - `in-memory-profile`: `IndexProfile::in_memory(` is a test's constructor (no source, nothing
//!   kept); a shipped build that calls it has made a provider for a test.
//!
//! Comments and test items are not read (a test's provider says `RestoreSource::none("test")`).

use std::path::Path;

use azul_srcgate::{files, hits, self_test, workspace, Code, Hit, Located, Orphans, Root, Rule};

/// The rule that wants an annotation.
const RESTORE_NONE: &str = "restore-none";

/// The rule that wants nothing.
const IN_MEMORY: &str = "in-memory-profile";

/// What an annotation starts with, after the comment's slashes.
const MARK: &str = "gate:restore-none allow:";

const ROOTS: &[Root] = &[
    Root {
        dir: "examples/azul-storage/src",
        min_files: 80,
    },
    Root {
        dir: "examples/azcloud-kit/src",
        min_files: 72,
    },
    Root {
        dir: "examples/azul-drive/src",
        min_files: 88,
    },
    Root {
        dir: "examples/azul-mail-core/src",
        min_files: 8,
    },
    Root {
        dir: "examples/azul-mail/src",
        min_files: 10,
    },
    Root {
        dir: "examples/azul-bridge/src",
        min_files: 15,
    },
    Root {
        dir: "examples/azul-appkit/src",
        min_files: 26,
    },
    Root {
        dir: "examples/azcloud-api/src",
        min_files: 1,
    },
];

/// Every call of `<type_name>::<function>(`.
fn path_calls(t: &[Located], type_name: &str, function: &str) -> Vec<usize> {
    let mut lines = Vec::new();
    for (i, x) in t.iter().enumerate() {
        if i >= 3
            && x.tok.is_ident(function)
            && t.get(i + 1).is_some_and(|next| next.tok.is_punct('('))
            && t[i - 1].tok.is_punct(':')
            && t[i - 2].tok.is_punct(':')
            && t[i - 3].tok.is_ident(type_name)
        {
            lines.push(x.line);
        }
    }
    lines
}

fn restore_none_calls(t: &[Located]) -> Vec<usize> {
    let mut lines = path_calls(t, "RestoreSource", "none");
    lines.extend(path_calls(t, "RestoreSource", "default"));
    lines
}

fn rules() -> Vec<Rule> {
    vec![
        Rule::new(
            RESTORE_NONE,
            restore_none_calls,
            &[
                "let restore = RestoreSource::none(\"a test\");",
                "let restore = azul_storage::meta::RestoreSource::none(\"why\");",
                "let restore = RestoreSource::default();",
            ],
            &[
                "let restore = RestoreSource::new(move || announcer.restore_notice());",
                "let none = restore.none_because();",
                "let none = RestoreSource::none;",
                "let restore_none = 1;",
            ],
        ),
        Rule::new(
            IN_MEMORY,
            |t| path_calls(t, "IndexProfile", "in_memory"),
            &[
                "let profile = IndexProfile::in_memory(\"AzDrive\");",
                "let profile = azul_storage::meta::IndexProfile::in_memory(name);",
            ],
            &[
                "let profile = IndexProfile { device_name, cache_root, lazy, packing, restore, reclaim };",
                "let in_memory = true;",
            ],
        ),
    ]
}

/// Whether `id` is a register id: `X-DELTA-AZDRIVE-SYNC-3`, `AZ-M4`, `NODE-INIT-16` - words with
/// dashes, the first upper-case, the last ending in a number.
fn is_register_id(id: &str) -> bool {
    let parts: Vec<&str> = id.split('-').collect();
    let [first, middle @ .., last] = parts.as_slice() else {
        return false;
    };
    first.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && first
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && middle
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric()))
        && last.chars().all(|c| c.is_ascii_alphanumeric())
        && last.chars().last().is_some_and(|c| c.is_ascii_digit())
}

/// What `line` says as an annotation: `None` when it holds none, else the reason or what is wrong
/// with it.
fn annotation(line: &str) -> Option<Result<(), String>> {
    let comment = line.find("//")?;
    let rest = line[comment + 2..].trim_start().strip_prefix(MARK)?.trim();
    let Some(open) = rest.rfind('[') else {
        return Some(Err(String::from("no `[<register id>]` at its end")));
    };
    let (why, id) = (rest[..open].trim(), rest[open + 1..].trim());
    let Some(id) = id.strip_suffix(']') else {
        return Some(Err(String::from("the `[<register id>]` is not closed")));
    };
    if why.is_empty() {
        return Some(Err(String::from("no reason before the `[<register id>]`")));
    }
    if !is_register_id(id.trim()) {
        return Some(Err(format!("`{id}` is no register id")));
    }
    Some(Ok(()))
}

fn line_of(code: &Code, n: usize) -> &str {
    code.source.lines().nth(n.wrapping_sub(1)).unwrap_or("")
}

/// What is wrong with `hits` in `read`: a `restore-none` call with no annotation (or a bad one) on
/// its line or the line above, an annotation that sits by no call, a call of the test constructor.
fn judge(read: &[Code], found: &[Hit]) -> Vec<String> {
    let mut wrong = Vec::new();
    for h in found {
        if h.rule == IN_MEMORY {
            wrong.push(format!(
                "{}:{}: {} (a test's profile in shipped code)",
                h.rel, h.line, h.text
            ));
            continue;
        }
        let Some(file) = read.iter().find(|f| f.rel == h.rel) else {
            continue;
        };
        let above = if h.line > 1 {
            line_of(file, h.line - 1)
        } else {
            ""
        };
        match annotation(line_of(file, h.line)).or_else(|| annotation(above)) {
            Some(Ok(())) => {}
            Some(Err(why)) => wrong.push(format!(
                "{}:{}: {} - its annotation is wrong: {why}",
                h.rel, h.line, h.text
            )),
            None => wrong.push(format!(
                "{}:{}: {} needs `// gate:restore-none allow: <why> [<register id>]` on its line \
                 or the line above",
                h.rel, h.line, h.text
            )),
        }
    }
    for file in read {
        for (i, line) in file.source.lines().enumerate() {
            let n = i + 1;
            if annotation(line).is_none() {
                continue;
            }
            let by_a_call = found.iter().any(|h| {
                h.rule == RESTORE_NONE && h.rel == file.rel && (h.line == n || h.line == n + 1)
            });
            if !by_a_call {
                wrong.push(format!(
                    "{}:{n}: an annotation of `restore-none` with no call on its line or below",
                    file.rel
                ));
            }
        }
    }
    wrong
}

/// The gate against the tree under `base` and `roots`: the rules' own cases, the files, the
/// judgement. The number of calls it let through.
fn check(base: &Path, roots: &[Root]) -> Result<usize, Vec<String>> {
    let rules = rules();
    self_test(&rules);
    let read = files(base, roots, Orphans::Ignore)?;
    let found = hits(&read, &rules);
    let wrong = judge(&read, &found);
    if wrong.is_empty() {
        Ok(found.len())
    } else {
        Err(wrong)
    }
}

#[test]
fn every_way_out_of_a_restore_source_in_shipped_code_says_why_and_a_test_profile_is_in_none() {
    if let Err(wrong) = check(&workspace(), ROOTS) {
        panic!(
            "an index provider without a restore source, or a test's profile, in shipped code \
             ({} finding(s)):\n{}",
            wrong.len(),
            wrong.join("\n")
        );
    }
}

/// A tree on disk below a fresh base: (path below the base, text).
fn base_with(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let base = std::env::temp_dir().join(format!(
        "azul-srcgate-index-profile-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&base);
    for (path, text) in files {
        let full = base.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }
    base
}

const SMALL: &[Root] = &[Root {
    dir: "examples/azul-mail-core/src",
    min_files: 1,
}];

/// X-DELTA-AZDRIVE-SYNC-3. A provider made with a source that is none, in shipped code, with no
/// reason, is the AzMail of before: a finding with its line. The annotation on the line or above,
/// with a reason and a register id, lets it through; a bad one, and one with no call, do not.
#[test]
fn a_planted_none_source_without_its_reason_is_caught_and_an_annotated_one_passes() {
    let planted = [
        "pub fn provider() -> MetaIndexProvider {",
        "    MetaIndexProvider::new(IndexProfile {",
        "        device_name: String::from(\"AzMail\"),",
        "        cache_root: Some(root()),",
        "        lazy: false,",
        "        packing: false,",
        "        restore: RestoreSource::none(\"later\"),",
        "        reclaim: ReclaimOptions::shadow(),",
        "    })",
        "}",
        "",
    ]
    .join("\n");
    let base = base_with(
        "planted",
        &[
            ("examples/azul-mail-core/src/lib.rs", "mod provider;\n"),
            ("examples/azul-mail-core/src/provider.rs", planted.as_str()),
        ],
    );
    let wrong = check(&base, SMALL).expect_err("a source that is none, unexplained");
    assert_eq!(wrong.len(), 1, "{wrong:?}");
    assert!(
        wrong[0].starts_with("examples/azul-mail-core/src/provider.rs:7"),
        "{wrong:?}"
    );

    let above = planted.replace(
        "        restore: RestoreSource::none(\"later\"),",
        "        // gate:restore-none allow: this provider opens no copy of an index [X-DELTA-AZDRIVE-SYNC-3]\n        restore: RestoreSource::none(\"later\"),",
    );
    std::fs::write(base.join("examples/azul-mail-core/src/provider.rs"), &above).unwrap();
    assert_eq!(
        check(&base, SMALL),
        Ok(1),
        "the reason is on the line above"
    );

    let same_line = planted.replace(
        "RestoreSource::none(\"later\"),",
        "RestoreSource::none(\"later\"), // gate:restore-none allow: a test double [AZ-M4]",
    );
    std::fs::write(
        base.join("examples/azul-mail-core/src/provider.rs"),
        &same_line,
    )
    .unwrap();
    assert_eq!(check(&base, SMALL), Ok(1), "the reason is on the line");

    for (bad, why) in [
        (
            "// gate:restore-none allow: [AZ-M4]",
            "no reason before the `[<register id>]`",
        ),
        (
            "// gate:restore-none allow: a reason with no id",
            "no `[<register id>]` at its end",
        ),
        (
            "// gate:restore-none allow: a reason [not an id]",
            "is no register id",
        ),
    ] {
        let text = planted.replace(
            "        restore: RestoreSource::none(\"later\"),",
            &format!("        {bad}\n        restore: RestoreSource::none(\"later\"),"),
        );
        std::fs::write(base.join("examples/azul-mail-core/src/provider.rs"), &text).unwrap();
        let wrong = check(&base, SMALL).expect_err(bad);
        assert!(
            wrong.iter().any(|w| w.contains(why)),
            "{bad}: wanted {why:?} in {wrong:?}"
        );
    }

    // The way out is gone from the code and the annotation stays: it fails, so the reason cannot
    // outlive the call.
    let gone = above.replace("RestoreSource::none(\"later\")", "announced_restore()");
    std::fs::write(base.join("examples/azul-mail-core/src/provider.rs"), &gone).unwrap();
    let wrong = check(&base, SMALL).expect_err("an annotation with no call");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("an annotation of `restore-none` with no call")),
        "{wrong:?}"
    );
    let _ = std::fs::remove_dir_all(base);
}

/// `RestoreSource::default()` is the same way out without a reason, and the test constructor of
/// the profile is none in a shipped build; a call inside a test item or a comment is read by no one.
#[test]
fn a_default_source_and_a_test_profile_in_shipped_code_are_caught_and_the_tests_are_not_read() {
    let planted = [
        "pub fn a() -> RestoreSource {",
        "    RestoreSource::default()",
        "}",
        "pub fn b() -> IndexProfile {",
        "    IndexProfile::in_memory(\"AzMail\")",
        "}",
        "// RestoreSource::none(\"a comment\")",
        "#[cfg(test)]",
        "mod tests {",
        "    fn t() {",
        "        let _ = RestoreSource::none(\"test\");",
        "        let _ = IndexProfile::in_memory(\"test\");",
        "    }",
        "}",
        "",
    ]
    .join("\n");
    let base = base_with(
        "default",
        &[
            ("examples/azul-mail-core/src/lib.rs", "mod provider;\n"),
            ("examples/azul-mail-core/src/provider.rs", planted.as_str()),
        ],
    );
    let wrong = check(&base, SMALL).expect_err("two findings");
    assert_eq!(wrong.len(), 2, "{wrong:?}");
    assert!(
        wrong.iter().any(|w| w.contains("provider.rs:2")),
        "{wrong:?}"
    );
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("provider.rs:5") && w.contains("a test's profile")),
        "{wrong:?}"
    );
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn a_register_id_is_upper_case_words_that_end_in_a_number() {
    for good in ["X-DELTA-AZDRIVE-SYNC-3", "AZ-M4x-1", "NODE-INIT-16", "AZ-1"] {
        assert!(is_register_id(good), "{good}");
    }
    for bad in [
        "",
        "SYNC",
        "sync-3",
        "SYNC-x",
        "SYNC-3-",
        "-3",
        "A--3",
        "not an id",
    ] {
        assert!(!is_register_id(bad), "{bad}");
    }
}
