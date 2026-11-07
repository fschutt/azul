//! Gate: the test-hook switches - `AZ_BACKEND` (as `headless`), `AZ_E2E_TEST`, `AZ_KEYRING`,
//! `AZ_KEYRING_FILE`, `AZ_AUTH_SESSION_REDIRECT` - are read only in the files listed here
//! (AZ-K5/P9, ARCH28 AR5b-1 / AR5a-10, RF38).
//!
//! The owner's decision (d): a shipped build never honours a test switch in the environment -
//! not the keyring stand-in, not the fake sign-in, not "open nothing", not "no biometric". The
//! predicate `AZ_BACKEND == "headless" || AZ_E2E_TEST is set` was written in seven places, each
//! deciding for itself what a test run is. It is now written ONCE: `azul_core::run_mode`
//! (`core/src/run_mode.rs`) reads the switches, and only in a build with the cargo feature
//! `test-hooks`; the dll's keyring, sign-in, biometric prompt, sensors and speech, `Url::open` and
//! the request mocks ask `test_hooks()` (the second test below pins that they do). What is left
//! on the list is that one place, three apps that cannot ask it (they link libazul as a library
//! and hold no azul-core) and read a switch of their own behind a `test-hooks` cargo feature of
//! their own, and the windowing choice: a new file that reads a name fails, and a file that
//! stops reading one has to leave the list - the counts only go down.
//!
//! What it finds is a string literal that IS a switch's name in shipped code: `env::var("..")`,
//! `var_os`, a `const NAME: &str = ".."`. Comments and test items are not read. `AZ_BACKEND` is
//! also the windowing/rendering choice (`cpu`, `gpu`, `x11`, ...): those files are listed too,
//! under their own heading, until the one `env` seam of AR2-7 reads it for all of them.

use std::path::{Path, PathBuf};

use azul_srcgate::{
    calls, code, files, locate, strings, workspace, Allow, Budget, Gate, Orphans, Root, Rule,
};

/// The names of the switches.
const SWITCHES: [&str; 5] = [
    "AZ_BACKEND",
    "AZ_E2E_TEST",
    "AZ_KEYRING",
    "AZ_KEYRING_FILE",
    "AZ_AUTH_SESSION_REDIRECT",
];

const ROOTS: &[Root] = &[
    Root {
        dir: "core/src",
        min_files: 20,
    },
    Root {
        dir: "css/src",
        min_files: 5,
    },
    Root {
        dir: "layout/src",
        min_files: 50,
    },
    Root {
        dir: "dll/src",
        min_files: 100,
    },
    Root {
        dir: "doc/src",
        min_files: 5,
    },
    Root {
        dir: "examples",
        min_files: 500,
    },
];

/// The rule's name.
const READ: &str = "test-switch-name";

/// Where the switches are read today. A is the one place (RF38): `azul_core::run_mode` names all
/// five variables, once each, as constants of the parse that `test_hooks()` runs behind the cargo
/// feature `test-hooks`. B is apps with a switch of their own, each behind a `test-hooks` cargo
/// feature of its own (they hold no azul-core to ask); each leaves this list when libazul can
/// answer it. C reads `AZ_BACKEND` as the windowing and rendering choice, or is an e2e runner
/// reading its own switch; those stay until the one `env` seam (AR2-7) reads them. The counts are
/// exact: a new read is a failure, and so is one that went away.
const ALLOWED: &[Allow] = &[
    // ---- A. the one place that knows which variable means what ----
    Allow::exactly(
        "core/src/run_mode.rs",
        READ,
        5,
        "TestHooks::from_env: the names of AZ_BACKEND (as a stand-in), AZ_E2E_TEST, AZ_KEYRING, \
         AZ_KEYRING_FILE and AZ_AUTH_SESSION_REDIRECT, read only behind the cargo feature test-hooks",
    ),
    // ---- B. an app's own switch, behind its own cargo feature test-hooks ----
    Allow::exactly(
        "examples/azul-builder/src/lib.rs",
        READ,
        2,
        "no debug-server page is opened in a headless or scripted run of a test build",
    ),
    Allow::exactly(
        "examples/azul-mail/src/lib.rs",
        READ,
        2,
        "the headless test password and test certificate are honoured under AZ_BACKEND=headless \
         in a test build only (LANE-AZ-3)",
    ),
    Allow::exactly(
        "examples/azul-drive/src/presence.rs",
        READ,
        2,
        "the stand-in of libazul's biometric prompt (AZ_BACKEND=headless, AZ_E2E_TEST) runs a \
         gated action at once in a test build only; a shipped build asks the drive's name",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/shared.rs",
        READ,
        1,
        "the keyring stand-in file's name, whose locks lie beside it: read in a test build only \
         (K5/P9 decision d)",
    ),
    Allow::exactly(
        "examples/azul-meet/src/call_audio.rs",
        READ,
        1,
        "no real microphone, speaker or camera is opened under AZ_BACKEND=headless (TO SHRINK: \
         read in every build; not yet behind a test-hooks feature)",
    ),
    // ---- C. AZ_BACKEND as the windowing / rendering choice; e2e runners' own reads ----
    Allow::exactly(
        "dll/src/desktop/app.rs",
        READ,
        1,
        "App::run logs the backend it starts with",
    ),
    Allow::exactly(
        "dll/src/desktop/shell2/common/compositor.rs",
        READ,
        2,
        "AzBackend::resolve: cpu, gpu, auto, headless, web",
    ),
    Allow::exactly(
        "dll/src/desktop/shell2/common/e2e_test.rs",
        READ,
        1,
        "the scenario runner reads its own AZ_E2E_TEST (cargo feature e2e-test)",
    ),
    Allow::exactly(
        "dll/src/desktop/shell2/common/x11_host.rs",
        READ,
        1,
        "the windowing choice on Linux (x11, wayland)",
    ),
    Allow::exactly(
        "dll/src/desktop/shell2/linux/mod.rs",
        READ,
        2,
        "the windowing choice on Linux (x11, wayland)",
    ),
    Allow::exactly(
        "dll/src/desktop/shell2/run.rs",
        READ,
        4,
        "logs AZ_BACKEND at the run entry of each platform, and the table of switches that need a \
         cargo feature (no read)",
    ),
    Allow::exactly(
        "dll/src/e2e_web_runner.rs",
        READ,
        1,
        "the embedded web-e2e executor: AZ_BACKEND=<http url> turns the binary into the runner",
    ),
];

fn rules() -> Vec<Rule> {
    vec![Rule::new(
        READ,
        |t| strings(t, |s| SWITCHES.contains(&s)),
        &[
            "let headless = std::env::var(\"AZ_E2E_TEST\").is_ok();",
            "let b = std::env::var_os(\"AZ_BACKEND\");",
            "let _ = var(\"AZ_KEYRING\");",
            "let f = std::env::var_os(\"AZ_KEYRING_FILE\");",
            "let r = std::env::var(\"AZ_AUTH_SESSION_REDIRECT\").ok();",
        ],
        &[
            "let v = std::env::var(\"AZ_BACKEND_URL\");",
            "println!(\"AZ_BACKEND is not headless\");",
            "let AZ_E2E_TEST = 1;",
        ],
    )]
}

fn gate(roots: &'static [Root], allowed: &'static [Allow]) -> Gate<'static> {
    Gate {
        name: "the test switches are read only where the list says",
        roots,
        rules: rules(),
        budget: Budget(allowed),
        orphans: Orphans::Ignore,
    }
}

#[test]
fn the_test_switches_are_read_only_in_the_listed_files() {
    gate(ROOTS, ALLOWED).run();
}

/// Prints every read of a switch: `cargo test -p azul-srcgate --test keyring_gate -- --ignored
/// --nocapture print_reads`, to write the list above.
#[test]
#[ignore = "prints the reads; the list above is written from it"]
fn print_reads() {
    let hits = azul_srcgate::hits(
        &azul_srcgate::files(&azul_srcgate::workspace(), ROOTS, Orphans::Ignore)
            .unwrap_or_else(|wrong| panic!("{wrong:#?}")),
        &rules(),
    );
    let mut per_file: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for h in &hits {
        *per_file.entry(h.rel.as_str()).or_default() += 1;
    }
    for (file, n) in per_file {
        println!("{n:>3} {file}");
    }
}

/// A tree on disk below a fresh base: (path below the base, text).
fn base_with(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "azul-srcgate-keyring-{name}-{}",
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
    dir: "core/src",
    min_files: 1,
}];

const LISTED: &[Allow] = &[Allow::exactly(
    "core/src/url.rs",
    "test-switch-name",
    2,
    "the planted copy",
)];

fn check(base: &Path, allowed: &'static [Allow]) -> Result<usize, Vec<String>> {
    gate(SMALL, allowed).check(base).map(|hits| hits.len())
}

#[test]
fn a_planted_read_of_a_switch_in_an_unlisted_file_is_caught_and_a_listed_one_is_counted() {
    let planted = "pub fn opens_nothing() -> bool {\n    std::env::var(\"AZ_BACKEND\").is_ok_and(|b| b == \"headless\")\n        || std::env::var(\"AZ_E2E_TEST\").is_ok()\n}\n";
    let base = base_with(
        "planted",
        &[
            ("core/src/lib.rs", "mod url;\n"),
            ("core/src/url.rs", planted),
        ],
    );
    // nobody may read a switch: both reads are findings, with their lines.
    let wrong = check(&base, &[]).expect_err("an unlisted read is a finding");
    assert_eq!(wrong.len(), 2, "{wrong:?}");
    assert!(
        wrong.iter().any(|w| w.starts_with("core/src/url.rs:2")),
        "{wrong:?}"
    );
    assert!(
        wrong.iter().any(|w| w.starts_with("core/src/url.rs:3")),
        "{wrong:?}"
    );
    // the list takes them, by count.
    assert_eq!(check(&base, LISTED), Ok(2));
    // a third read in the listed file is a new site: the count says so.
    std::fs::write(
        base.join("core/src/url.rs"),
        format!("{planted}pub fn more() -> bool {{\n    std::env::var_os(\"AZ_KEYRING\").is_some()\n}}\n"),
    )
    .unwrap();
    let wrong = check(&base, LISTED).expect_err("one more read than the list says");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("holds 3 x test-switch-name where its allowance says 2")),
        "{wrong:?}"
    );
    // a read that went away leaves a stale count: the list shrinks with the code.
    std::fs::write(base.join("core/src/url.rs"), "pub fn clean() {}\n").unwrap();
    let wrong = check(&base, LISTED).expect_err("fewer reads than the list says");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("holds 0 x test-switch-name where its allowance says 2")),
        "{wrong:?}"
    );
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn a_switch_named_in_a_comment_a_test_or_a_message_is_no_read() {
    let base = base_with(
        "quiet",
        &[(
            "core/src/lib.rs",
            "// AZ_BACKEND=headless\n/// `AZ_E2E_TEST` is a test switch\npub fn a() {\n    println!(\"AZ_BACKEND={}\", 1);\n}\n#[cfg(test)]\nmod tests {\n    fn t() {\n        std::env::set_var(\"AZ_KEYRING\", \"memory\");\n    }\n}\n",
        )],
    );
    assert_eq!(check(&base, &[]), Ok(0));
    let _ = std::fs::remove_dir_all(base);
}

/// Every consumer of a stand-in ASKS the hooks: a consumer that dropped the question (and kept
/// the stand-in always on, or always off) is found here, not only a file that reads a variable
/// itself. These are the places the seven copies of the predicate were (RF38): `Url::open`, the
/// request mocks, the dll's sign-in, biometric prompt, sensors, speech and keyring facts.
const CONSUMERS: [&str; 7] = [
    "core/src/url.rs",
    "layout/src/request.rs",
    "dll/src/desktop/extra/auth_session/mod.rs",
    "dll/src/desktop/extra/biometric/mod.rs",
    "dll/src/desktop/extra/sensors/mod.rs",
    "dll/src/desktop/extra/speech/mod.rs",
    "dll/src/desktop/extra/keyring/mod.rs",
];

#[test]
fn every_consumer_of_a_stand_in_asks_the_run_modes_hooks() {
    // The finder's own cases: a call in a function's body is found, one in a test module or a
    // comment is not.
    let found = |src: &str| calls(&locate(&code(src)), "run_mode::test_hooks").len();
    assert_eq!(found("fn f() { azul_core::run_mode::test_hooks(); }"), 1);
    assert_eq!(
        found("fn f() { crate::run_mode::test_hooks().open_nothing; }"),
        1
    );
    assert_eq!(
        found("#[cfg(test)]\nmod t { fn f() { run_mode::test_hooks(); } }"),
        0
    );
    assert_eq!(
        found("// run_mode::test_hooks()\nfn f() { test_hooks(); }"),
        0
    );

    let read = files(&workspace(), ROOTS, Orphans::Ignore).expect("the roots are readable");
    for rel in CONSUMERS {
        let file = read
            .iter()
            .find(|f| f.rel == rel)
            .unwrap_or_else(|| panic!("{rel} is not among the compiled files of the roots"));
        assert!(
            !calls(&file.tokens(), "run_mode::test_hooks").is_empty(),
            "{rel} does not ask azul_core::run_mode::test_hooks(): its stand-in is decided by \
             something else"
        );
    }
}
