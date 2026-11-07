//! Gate: the CI runs the tests of the code that decides what gets installed (AZ-Y13). The gate that
//! the source gates themselves run in the CI is `ci_gate.rs`.
//!
//! The updater's unit tests - every signature-chain test, 75 of them - are compiled only with the
//! cargo feature `updater` of azul-layout, which no default switches on. A test that no job runs
//! protects nothing: the chain that decides which program is installed could break and the CI stay
//! green. This reads `.github/workflows/rust.yml` and fails when the step that runs azul-layout's
//! tests does not pass `azul-layout/updater`, or when the module is no longer behind that feature
//! (the premise of the check).

use azul_srcgate::workspace;

fn read(relative: &str) -> String {
    let path = workspace().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The shell command that starts at the first line containing `start`: its lines joined while they
/// end in a backslash.
fn command_from(text: &str, start: &str) -> Option<String> {
    let mut lines = text.lines().skip_while(|line| !line.contains(start));
    let mut command = String::new();
    for line in lines.by_ref() {
        let trimmed = line.trim();
        command.push_str(trimmed.trim_end_matches('\\'));
        command.push(' ');
        if !trimmed.ends_with('\\') {
            break;
        }
    }
    (!command.is_empty()).then_some(command)
}

#[test]
fn the_ci_runs_the_layout_tests_with_the_updater_feature() {
    let workflow = read(".github/workflows/rust.yml");
    let command = command_from(
        &workflow,
        "cargo test -p azul-css -p azul-core -p azul-layout",
    )
    .expect("the CI has a step that runs azul-layout's tests");
    assert!(
        command.contains("-p azul-layout"),
        "the step does not run azul-layout's tests: {command}"
    );
    let features: Vec<&str> = command
        .split("--features")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .map(|list| list.split(',').collect())
        .unwrap_or_default();
    assert!(
        features.contains(&"azul-layout/updater"),
        "the updater's tests (the signature chain) are not run by the CI: --features {features:?}"
    );
}

/// The premise: the updater module and its tests exist behind that feature.
#[test]
fn the_updater_and_its_tests_are_behind_the_feature_the_ci_passes() {
    let lib = read("layout/src/lib.rs");
    let gated = lib.lines().zip(lib.lines().skip(1)).any(|(attr, item)| {
        attr.trim() == "#[cfg(feature = \"updater\")]" && item.trim() == "pub mod updater;"
    });
    assert!(
        gated,
        "layout/src/lib.rs no longer gates `pub mod updater;` by the feature `updater`"
    );
    let updater = read("layout/src/updater.rs");
    let tests = updater.matches("#[test]").count();
    assert!(tests >= 30, "the updater lost its tests: {tests}");
    let manifest = read("layout/Cargo.toml");
    assert!(
        manifest.lines().any(|line| line.starts_with("updater = [")),
        "layout/Cargo.toml has no `updater` feature"
    );
}

/// The command finder on its own cases: a continued line is one command, a command that is not
/// continued ends at its line.
#[test]
fn the_command_finder_joins_continued_lines() {
    let text = "      - name: x\n        run: |\n          cargo test -p a \\\n            --features one,two \\\n            --no-fail-fast 2>&1 | tee out.txt\n          ./next.sh\n";
    let command = command_from(text, "cargo test -p a").expect("a command");
    assert!(command.contains("--features one,two"), "{command}");
    assert!(command.contains("--no-fail-fast"), "{command}");
    assert!(!command.contains("next.sh"), "{command}");
    assert!(command_from(text, "cargo bench").is_none());
}
