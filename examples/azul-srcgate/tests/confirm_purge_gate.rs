//! Gate: the modules that may record the user's answer to "erase the old versions too", and the modules
//! that may erase on it (X-DELTA-KEYS4-5, X-DELTA-SYNC7-6; ARCH30-1).
//!
//! A `PurgeConfirmation` is the licence to erase every old version of every file of a drive for good
//! (`Why::Confirmed`, `purge_options`). The constructor from a number is private; the one door left is
//! the record the app makes of the user's answer, `ReencryptState::confirm_purge(at)`, which the
//! re-encryption's state keeps (and a saved state reads back). Nothing in the types can say that the
//! user really answered - a number is all that call takes - so, as for `Chose`, a short text rule names
//! the modules that may call it: the commands and dialogs of a user's own request, one row each. A
//! listing, a scan, a retry or an upkeep records no answer.
//!
//! The same list reads the CONSUMER of the answer: `Account::erase_old_versions` and the provider's
//! `purge_confirmed` it calls. A job that resumes from a saved state (the state is `Deserialize`: its
//! answer comes back from a file) calls `erase_old_versions` and never `confirm_purge`, and a rule that read
//! only the recorder would not list it (X-DELTA-SYNC7-6). A module that can erase old versions has its row,
//! whether it recorded the answer or read it.
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

/// The rule that reads the recorder's name.
const CONFIRM_PURGE: &str = "confirm-purge-named";

/// The rule that reads the consumer's names.
const ERASE_NAMED: &str = "erase-old-versions-named";

fn rules() -> Vec<Rule> {
    vec![
        Rule::new(
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
        ),
        Rule::new(
            ERASE_NAMED,
            |t| {
                let mut lines = idents(t, "erase_old_versions");
                lines.extend(idents(t, "purge_confirmed"));
                lines.sort_unstable();
                lines
            },
            &[
                "account.erase_old_versions(&keys, &provider, &state)?;",
                "Account::erase_old_versions(&account, &keys, &provider, &state);",
                "let go = Account::erase_old_versions;",
                "provider.purge_confirmed(id, bucket, &key, &confirmation)?;",
                "pub fn erase_old_versions(&self, keyring: &dyn KeyringStore) {",
            ],
            &[
                "let asked = erase_old_versions_asked(args);",
                "state.purge_confirmed_at = None;",
                "let later = erase_old_versions_later;",
                "let name = chose.purge_confirmed_by;",
            ],
        ),
    ]
}

/// The modules that may name `confirm_purge`, and why. TO GROW ONLY WITH A JOB: each row names the job.
const ALLOWED: &[Allow] = &[
    Allow::rule(
        "examples/azul-storage/src/rotation.rs",
        CONFIRM_PURGE,
        "the definition of `ReencryptState::confirm_purge`, the record of the user's answer",
    ),
    Allow::rule(
        "examples/azcloud-api/src/bin/azcloud.rs",
        CONFIRM_PURGE,
        "`azcloud reencrypt --erase-old-versions --yes`: the user's own command records the answer",
    ),
    Allow::rule(
        "examples/azcloud-api/src/bin/azcloud.rs",
        ERASE_NAMED,
        "`azcloud reencrypt --erase-old-versions --yes`: the same command erases on it, after the pass",
    ),
    Allow::rule(
        "examples/azcloud-kit/src/encryption.rs",
        ERASE_NAMED,
        "`Account::erase_old_versions`: the one consumer of the licence (`ReencryptState::erase_licence`); it calls the provider",
    ),
    Allow::rule(
        "examples/azul-storage/src/encrypted.rs",
        ERASE_NAMED,
        "the definition of `IndexProvider::purge_confirmed`: the default refuses, nothing is erased",
    ),
    Allow::rule(
        "examples/azul-storage/src/meta/index.rs",
        ERASE_NAMED,
        "the provider that keeps the history runs the confirmed purge (`MetaIndexProvider::purge`)",
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

const LISTED: &[Allow] = &[
    Allow::rule(
        "examples/jobs/src/reencrypt.rs",
        CONFIRM_PURGE,
        "the planted command that may record the answer",
    ),
    Allow::rule(
        "examples/jobs/src/reencrypt.rs",
        ERASE_NAMED,
        "the planted command that may erase on it",
    ),
];

fn check(base: &Path, allowed: &'static [Allow]) -> Result<usize, Vec<String>> {
    gate(SMALL, allowed).check(base).map(|hits| hits.len())
}

/// A planted tree: a module that is not listed records the answer in each of the ways round a rule that
/// reads one spelling - the method call, the path to it and a function value - and each is a finding
/// with its line; a module that never records the answer and only RESUMES from a saved state - it calls
/// the consumer, `erase_old_versions` and the provider's `purge_confirmed`, and is found too (X-DELTA-SYNC7-6);
/// the listed command's own calls, a comment, a string, a test item and the getter (`purge_confirmation`)
/// are none.
#[test]
fn a_planted_confirmation_in_a_module_that_is_not_listed_is_caught_and_the_listed_command_is_not() {
    let base = base_with(
        "planted",
        &[
            (
                "examples/jobs/src/lib.rs",
                "mod reencrypt;\nmod quiet;\nmod resume;\nmod upkeep;\n",
            ),
            (
                "examples/jobs/src/reencrypt.rs",
                "fn reencrypt(state: &mut ReencryptState) {\n    state.confirm_purge(1);\n    let c = state.purge_confirmation();\n    account.erase_old_versions(k, p, state);\n}\n",
            ),
            (
                "examples/jobs/src/resume.rs",
                concat!(
                    "fn resume(account: &Account, saved: &str) {\n",
                    "    let state = ReencryptState::from_json(saved).unwrap();\n",
                    "    account.erase_old_versions(&keys, &provider, &state);\n",
                    "    provider.purge_confirmed(\"d\", bucket, &key, &confirmation);\n",
                    "}\n",
                ),
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
                    "// state.confirm_purge(1); account.erase_old_versions(k, p, s)\n",
                    "fn quiet(state: &ReencryptState) {\n",
                    "    let said = \"confirm_purge(\";\n",
                    "    let also = \"erase_old_versions(\";\n",
                    "    let c = state.purge_confirmation();\n",
                    "    let asked = erase_old_versions_asked(args);\n",
                    "}\n",
                    "#[cfg(test)]\n",
                    "mod tests {\n",
                    "    fn t(state: &mut ReencryptState) { state.confirm_purge(4); account.erase_old_versions(k, p, state); }\n",
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
    assert!(
        at("examples/jobs/src/resume.rs", 3),
        "a job that only reads the saved answer and erases on it: {wrong:#?}"
    );
    assert!(
        at("examples/jobs/src/resume.rs", 4),
        "the provider's confirmed purge: {wrong:#?}"
    );
    assert_eq!(
        wrong.len(),
        5,
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
