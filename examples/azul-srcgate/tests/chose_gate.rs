//! Gate: the modules that may make a `Chose`, the one reason to delete that the apps make
//! (ARCH30-1; ARCH29 section 9.4 A1).
//!
//! `Drive::delete_for` takes a reason (`azul_storage::erase::Why`): five witnesses, four of which only
//! the owner of the evidence can make (the licence of the one gate and the proof of the history, a
//! confirmed purge, the receipt of a publish, a refused change). The fifth, `Why::UserChose(&Chose)`,
//! is the job a user started, and its constructor, `Chose::job`, is public: the apps have to make it.
//! What keeps it from being the `pub(crate)` constructor that makes every witness a witness for the
//! crate is the list of the modules that may name it - the jobs, the handlers of a user's own click,
//! command or request: a listing, a scan, a retry or the mirror of another side's delete is no job
//! a user started. This is the ONE text rule of the design that stays (the others are signatures);
//! the list is short, each row says which job it is, and `rg 'Why::UserChose'` is the whole list of
//! "the user said so".
//!
//! The identifier `Chose` is read, not only the call `Chose::job(`: a `use .. Chose as Job;`, a
//! function value (`let make = Chose::job;`) and a type in a signature are the ways round a rule
//! that reads one spelling. Comments, strings and test items are not read (a test makes the reason it
//! needs), and a module that names a `Chose` is a row of `ALLOWED` with its reason, in the commit that
//! gives it its first one. The migration of the delete sites (ARCH30-4) adds a row per job.

use std::path::{Path, PathBuf};

use azul_srcgate::{idents, Allow, Budget, Gate, Orphans, Root, Rule};

const ROOTS: &[Root] = &[Root {
    dir: "examples",
    min_files: 500,
}];

/// The rule's name.
const CHOSE: &str = "chose-named";

fn rules() -> Vec<Rule> {
    vec![Rule::new(
        CHOSE,
        |t| idents(t, "Chose"),
        &[
            "let chose = Chose::job(\"delete\", Target::Key(key));",
            "let chose = azul_storage::erase::Chose::job(\"delete\", target);",
            "let make = Chose::job;",
            "use azul_storage::erase::Chose as Job;",
            "fn jobs(chose: &Chose) {}",
        ],
        &[
            "let chosen = Chosen::job(1);",
            "let x = ChoseNot::job(1);",
            "let why = Why::UserChose(&job);",
            "let name = chose.job;",
        ],
    )]
}

/// The modules that may name a `Chose`, and why. TO GROW ONLY WITH A JOB: each row names the job.
const ALLOWED: &[Allow] = &[Allow::file(
    "examples/azul-storage/src/erase.rs",
    "the definition of `Chose` and of `Why`, which carries it",
)];

fn gate(roots: &'static [Root], allowed: &'static [Allow]) -> Gate<'static> {
    Gate {
        name: "only the jobs of the apps name a `Chose`",
        roots,
        rules: rules(),
        budget: Budget(allowed),
        orphans: Orphans::Ignore,
    }
}

#[test]
fn only_the_listed_modules_name_a_chose() {
    gate(ROOTS, ALLOWED).run();
}

/// A tree on disk below a fresh base: (path below the base, text).
fn base_with(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let base =
        std::env::temp_dir().join(format!("azul-srcgate-chose-{name}-{}", std::process::id()));
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
    "examples/jobs/src/delete.rs",
    "the planted job that may make a reason",
)];

fn check(base: &Path, allowed: &'static [Allow]) -> Result<usize, Vec<String>> {
    gate(SMALL, allowed).check(base).map(|hits| hits.len())
}

/// A planted tree: a module that is not listed names a `Chose` in each of the ways round a rule that
/// reads one spelling - the call, the path to it, an alias, a function value and a type - and each is
/// a finding with its line; the listed job's own call, a comment, a string, a test item and the name
/// of the variant (`Why::UserChose`) are none.
#[test]
fn a_planted_chose_in_a_module_that_is_not_listed_is_caught_and_the_listed_job_is_not() {
    let base = base_with(
        "planted",
        &[
            (
                "examples/jobs/src/lib.rs",
                "mod delete;\nmod quiet;\nmod sync;\n",
            ),
            (
                "examples/jobs/src/delete.rs",
                "fn delete(key: String) {\n    let chose = Chose::job(\"delete\", Target::Key(key));\n    let why = Why::UserChose(&chose);\n}\n",
            ),
            (
                "examples/jobs/src/sync.rs",
                concat!(
                    "use azul_storage::erase::Chose as Job;\n",
                    "fn mirror(key: String) {\n",
                    "    let chose = azul_storage::erase::Chose::job(\"mirror\", Target::Key(key));\n",
                    "    let make = Chose::job;\n",
                    "}\n",
                    "fn takes(chose: &Chose) {}\n",
                ),
            ),
            (
                "examples/jobs/src/quiet.rs",
                concat!(
                    "// Chose::job(\"a comment\")\n",
                    "fn quiet() {\n",
                    "    let said = \"Chose::job(\";\n",
                    "    let why = Why::UserChose(&job);\n",
                    "}\n",
                    "#[cfg(test)]\n",
                    "mod tests {\n",
                    "    fn t() { let chose = Chose::job(\"a test\", Target::Key(String::new())); }\n",
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
    assert!(at("examples/jobs/src/sync.rs", 1), "an alias: {wrong:#?}");
    assert!(
        at("examples/jobs/src/sync.rs", 3),
        "a path to the call: {wrong:#?}"
    );
    assert!(
        at("examples/jobs/src/sync.rs", 4),
        "a function value: {wrong:#?}"
    );
    assert!(at("examples/jobs/src/sync.rs", 6), "a type: {wrong:#?}");
    assert_eq!(
        wrong.len(),
        4,
        "the listed job, the comment, the string, the test and the variant are none: {wrong:#?}"
    );
    let _ = std::fs::remove_dir_all(base);
}

/// An allowance nothing uses any more is a finding too: a row of `ALLOWED` whose job went away is
/// taken out, so the list is the jobs that exist.
#[test]
fn a_row_of_the_list_that_no_module_uses_is_a_finding() {
    let base = base_with(
        "stale",
        &[
            ("examples/jobs/src/lib.rs", "mod delete;\n"),
            (
                "examples/jobs/src/delete.rs",
                "fn delete() {\n    let why = Why::UserChose(&job);\n}\n",
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
