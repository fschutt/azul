//! Gate: the modules that may record the user's answer to "erase the old versions too"
//! (X-DELTA-KEYS4-5; ARCH30-1).
//!
//! A `PurgeConfirmation` is the licence to erase every old version of every file of a drive for good
//! (`Why::Confirmed`, `purge_options`). The constructor from a number is private; the one door left is
//! the record the app makes of the user's answer, `ReencryptState::confirm_purge(at)`, which the
//! re-encryption's state keeps (and a saved state reads back). Nothing in the types can say that the
//! user really answered - a number is all that call takes - so, as for `Chose`, a short text rule names
//! the modules that may call it: the commands and dialogs of a user's own request, one row each. A
//! listing, a scan, a retry or an upkeep records no answer.
//!
//! The identifier is read, not only the call: a function value or an alias of it is a way round a rule
//! that reads one spelling. Comments, strings and test items are not read (a test records the answer it
//! needs). A row is added with the job that records an answer, in the commit that gives it its first one.

use std::path::{Path, PathBuf};

use azul_srcgate::{idents, Allow, Budget, Gate, Orphans, Root, Rule};

const ROOTS: &[Root] = &[Root {
    dir: "examples",
    min_files: 500,
}];

/// The rule's name.
const CONFIRM_PURGE: &str = "confirm-purge-named";

fn rules() -> Vec<Rule> {
    vec![Rule::new(
        CONFIRM_PURGE,
        |t| idents(t, "confirm_purge"),
        &[
            "state.confirm_purge(now);",
            "ReencryptState::confirm_purge(&mut state, now);",
            "let record = ReencryptState::confirm_purge;",
            "use azul_storage::rotation::ReencryptState as State; State::confirm_purge(&mut s, 1);",
            "pub fn confirm_purge(&mut self, at: u64) {",
        ],
        &[
            "let confirmation = state.purge_confirmation();",
            "let x = confirm_purge_at;",
            "let confirm_purged = 1;",
            "state.confirmed_purge(1);",
            "let name = chose.confirm_purge_later;",
        ],
    )]
}

/// The modules that may name `confirm_purge`, and why. TO GROW ONLY WITH A JOB: each row names the job.
const ALLOWED: &[Allow] = &[
    Allow::file(
        "examples/azul-storage/src/rotation.rs",
        "the definition of `ReencryptState::confirm_purge`, the record of the user's answer",
    ),
    Allow::file(
        "examples/azcloud-api/src/bin/azcloud.rs",
        "`azcloud reencrypt --erase-old-versions --yes`: the user's own command records the answer",
    ),
];

fn gate(roots: &'static [Root], allowed: &'static [Allow]) -> Gate<'static> {
    Gate {
        name: "only the commands and dialogs of a user's own request record the answer to a purge",
        roots,
        rules: rules(),
        budget: Budget(allowed),
        orphans: Orphans::Ignore,
    }
}

#[test]
fn only_the_listed_modules_record_a_purge_confirmation() {
    gate(ROOTS, ALLOWED).run();
}

/// A tree on disk below a fresh base: (path below the base, text).
fn base_with(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "azul-srcgate-confirm-purge-{name}-{}",
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
    dir: "examples/jobs/src",
    min_files: 1,
}];

const LISTED: &[Allow] = &[Allow::file(
    "examples/jobs/src/reencrypt.rs",
    "the planted command that may record the answer",
)];

fn check(base: &Path, allowed: &'static [Allow]) -> Result<usize, Vec<String>> {
    gate(SMALL, allowed).check(base).map(|hits| hits.len())
}

/// A planted tree: a module that is not listed records the answer in each of the ways round a rule that
/// reads one spelling - the method call, the path to it and a function value - and each is a finding
/// with its line; the listed command's own call, a comment, a string, a test item and the getter
/// (`purge_confirmation`) are none.
#[test]
fn a_planted_confirmation_in_a_module_that_is_not_listed_is_caught_and_the_listed_command_is_not() {
    let base = base_with(
        "planted",
        &[
            (
                "examples/jobs/src/lib.rs",
                "mod reencrypt;\nmod quiet;\nmod upkeep;\n",
            ),
            (
                "examples/jobs/src/reencrypt.rs",
                "fn reencrypt(state: &mut ReencryptState) {\n    state.confirm_purge(1);\n    let c = state.purge_confirmation();\n}\n",
            ),
            (
                "examples/jobs/src/upkeep.rs",
                concat!(
                    "fn upkeep(state: &mut ReencryptState) {\n",
                    "    state.confirm_purge(2);\n",
                    "    ReencryptState::confirm_purge(state, 3);\n",
                    "    let record = ReencryptState::confirm_purge;\n",
                    "}\n",
                ),
            ),
            (
                "examples/jobs/src/quiet.rs",
                concat!(
                    "// state.confirm_purge(1)\n",
                    "fn quiet(state: &ReencryptState) {\n",
                    "    let said = \"confirm_purge(\";\n",
                    "    let c = state.purge_confirmation();\n",
                    "}\n",
                    "#[cfg(test)]\n",
                    "mod tests {\n",
                    "    fn t(state: &mut ReencryptState) { state.confirm_purge(4); }\n",
                    "}\n",
                ),
            ),
        ],
    );
    let wrong = check(&base, LISTED).unwrap_err();
    let at = |file: &str, line: usize| {
        wrong
            .iter()
            .any(|w| w.starts_with(&format!("{file}:{line}:")))
    };
    assert!(
        at("examples/jobs/src/upkeep.rs", 2),
        "a method call: {wrong:#?}"
    );
    assert!(
        at("examples/jobs/src/upkeep.rs", 3),
        "a path to it: {wrong:#?}"
    );
    assert!(
        at("examples/jobs/src/upkeep.rs", 4),
        "a function value: {wrong:#?}"
    );
    assert_eq!(
        wrong.len(),
        3,
        "the listed command, the comment, the string, the test and the getter are none: {wrong:#?}"
    );
    let _ = std::fs::remove_dir_all(base);
}

/// An allowance nothing uses any more is a finding too: a row whose command went away is taken out, so
/// the list is the commands that exist.
#[test]
fn a_row_of_the_list_that_no_module_uses_is_a_finding() {
    let base = base_with(
        "stale",
        &[
            ("examples/jobs/src/lib.rs", "mod reencrypt;\n"),
            (
                "examples/jobs/src/reencrypt.rs",
                "fn reencrypt(state: &ReencryptState) {\n    let c = state.purge_confirmation();\n}\n",
            ),
        ],
    );
    let wrong = check(&base, LISTED).unwrap_err();
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("an allowance nothing uses any more")),
        "{wrong:#?}"
    );
    let _ = std::fs::remove_dir_all(base);
}
