//! The libazul that SHIPS holds none of the doors a test run needs (ARCH28 RF38, REVIEW K15,
//! X-AR5b-22).
//!
//! `dist` is the feature set that ships: `build-dll` minus
//!
//! - `debug-server`: an HTTP port and a debugger inside a customer's process,
//! - `e2e-scripting`: `AZ_E2E=script.json` drives the UI from an environment variable,
//! - `test-hooks`: the keyring, sign-in and `Url::open` stand-ins of a test run
//!   (and `e2e-test`, the `AZ_E2E_TEST` scenario runner).
//!
//! This reads the `[features]` table of `dll/Cargo.toml` and follows the CLOSURE of every
//! feature, so a door that comes back through an implication (`dist` -> `x` -> `debug-server`,
//! or a dependency's `e2e-server-http` named in a list) is caught, not only one written in the
//! list itself. It has no dependency on purpose: the table is parsed by hand (a TOML or JSON
//! crate would change the lock file for a test), and the first test pins that the parse found
//! what it must, so a parser that silently finds nothing cannot pass.
//!
//! The other halves of RF38 live where the code is:
//!
//! - the SOURCE gate (the switches are read in `azul_core::run_mode` and nowhere else, and every
//!   consumer asks it) is `azul-srcgate`'s `tests/keyring_gate.rs`;
//! - the BEHAVIOURAL half (the variables really set in a process, no feature: the hooks are
//!   none; with the feature: read once) is `azul-core`'s `tests/run_mode_release.rs` and
//!   `tests/run_mode_hooks.rs`, and each consumer's own tests under the hooks `NONE` and
//!   `HEADLESS` (`Url::open`, the sign-in, the biometric prompt, the sensors, the speech, the
//!   keyring facts, the request mocks);
//! - what this file adds to its closure gate: the features FORWARD - the dll's `test-hooks`
//!   switches on core's (which reads the variables) and layout's, or a libazul built for the E2E
//!   runs would carry the keyring marker and no hook at all.

use std::collections::{BTreeMap, BTreeSet};

/// The features no shipped build may have. `web-e2e-runner` makes any azul binary the web test
/// runner (it spawns node, bun or deno).
const DOORS: [&str; 5] = [
    "test-hooks",
    "debug-server",
    "e2e-scripting",
    "e2e-test",
    "web-e2e-runner",
];

/// A dependency's feature that is the same door: named as `pkg/feature` or `pkg?/feature`.
const DEPENDENCY_DOORS: [&str; 3] = ["e2e-server", "e2e-server-http", "test-hooks"];

/// `line` without its `#` comment (a `#` inside a string stays).
fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    for (at, c) in line.char_indices() {
        match c {
            '"' => in_string = !in_string,
            '#' if !in_string => return &line[..at],
            _ => {}
        }
    }
    line
}

/// The strings of an array body, up to its closing bracket.
fn quoted(body: &str) -> Vec<String> {
    let body = body.split(']').next().unwrap_or_default();
    body.split('"')
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, s)| s.to_string())
        .collect()
}

/// `[features]` of a manifest: name -> entries, comments dropped, multi-line arrays joined.
fn parse_features(text: &str) -> BTreeMap<String, Vec<String>> {
    let mut table = BTreeMap::new();
    let mut in_features = false;
    let mut open: Option<(String, String)> = None;
    for raw in text.lines() {
        let line = strip_comment(raw).trim();
        if let Some((name, body)) = open.as_mut() {
            body.push_str(line);
            body.push(' ');
            if line.contains(']') {
                table.insert(name.clone(), quoted(body));
                open = None;
            }
            continue;
        }
        if line.starts_with('[') {
            in_features = line == "[features]";
            continue;
        }
        if !in_features || line.is_empty() {
            continue;
        }
        let Some((name, rest)) = line.split_once('=') else {
            continue;
        };
        let (name, rest) = (name.trim(), rest.trim());
        let Some(array) = rest.strip_prefix('[') else {
            continue;
        };
        if array.contains(']') {
            table.insert(name.to_string(), quoted(array));
        } else {
            open = Some((name.to_string(), format!("{array} ")));
        }
    }
    table
}

fn dll_features() -> BTreeMap<String, Vec<String>> {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest)
        .unwrap_or_else(|e| panic!("read {}: {e}", manifest.display()));
    parse_features(&text)
}

/// Every feature `root` switches on (itself included) and every raw entry along the way
/// (`dep:x`, `pkg/feature`, ...).
fn closure(
    table: &BTreeMap<String, Vec<String>>,
    root: &str,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut features = BTreeSet::new();
    let mut entries = BTreeSet::new();
    let mut todo = vec![root.to_string()];
    while let Some(name) = todo.pop() {
        if !features.insert(name.clone()) {
            continue;
        }
        for entry in table.get(&name).into_iter().flatten() {
            entries.insert(entry.clone());
            if table.contains_key(entry) {
                todo.push(entry.clone());
            }
        }
    }
    (features, entries)
}

/// The doors `root` switches on, in words.
fn doors_in(table: &BTreeMap<String, Vec<String>>, root: &str) -> Vec<String> {
    let (features, entries) = closure(table, root);
    let mut found: Vec<String> = DOORS
        .iter()
        .filter(|door| features.contains(**door))
        .map(|door| format!("feature `{door}`"))
        .collect();
    for entry in entries {
        let Some((_, feature)) = entry.split_once('/') else {
            continue;
        };
        if DEPENDENCY_DOORS.contains(&feature) {
            found.push(format!("`{entry}`"));
        }
    }
    found
}

#[test]
fn the_parse_finds_the_feature_table_it_is_about_to_judge() {
    let table = dll_features();
    assert!(
        table.len() > 40,
        "the dll has dozens of features: {}",
        table.len()
    );
    for must in [
        "dist",
        "build-dll",
        "debug-server",
        "e2e-scripting",
        "test-hooks",
    ] {
        assert!(table.contains_key(must), "no `{must}` in [features]");
    }
    let (dist, entries) = closure(&table, "dist");
    for member in [
        "cabi_export",
        "http",
        "telemetry",
        "db-sqlite",
        "iroh",
        "_internal_deps",
    ] {
        assert!(dist.contains(member), "`dist` lost `{member}`: {dist:?}");
    }
    assert!(
        entries.contains("azul-layout/probe"),
        "a dependency's feature is an entry"
    );

    // The parser on a table with comments, a multi-line array and a `#` inside a string.
    let sample = parse_features(
        "[package]\nname = \"x\"\n[features]\n# a comment\na = []\nb = [\"a\", \"c/d\"] # tail\nc = [\n  \"b\", # why\n  \"e\",\n]\n[dependencies]\nz = [\"nope\"]\n",
    );
    assert_eq!(sample["a"], Vec::<String>::new());
    assert_eq!(sample["b"], vec!["a", "c/d"]);
    assert_eq!(sample["c"], vec!["b", "e"]);
    assert!(!sample.contains_key("z"), "only [features] is read");
}

/// REVIEW K15, X-AR5b-22: nothing the shipped libazul switches on is a door.
#[test]
fn the_feature_closure_of_dist_holds_no_debug_server_no_e2e_scripting_and_no_test_hooks() {
    let table = dll_features();
    assert!(
        table.contains_key("dist"),
        "there is no `dist`: nothing ships without the doors"
    );
    let doors = doors_in(&table, "dist");
    assert!(doors.is_empty(), "`dist` switches on {doors:?}");
}

/// X-DELTA-AZDRIVE-KEYS-10: cargo features ADD UP. A `--features dist` that did not say
/// `--no-default-features` carries every default along, so a door in `default` is a door in the
/// libazul that ships: `debug-server` and `web-e2e-runner` were defaults, and the gate above looked
/// at `dist` alone. No default is a door, nor leads to one.
#[test]
fn the_feature_closure_of_the_defaults_holds_no_door() {
    let table = dll_features();
    let default = table.get("default").expect("the dll has a `default`");
    for door in DOORS {
        assert!(
            !default.iter().any(|entry| entry == door),
            "`default` names the door `{door}`: {default:?}"
        );
    }
    let doors = doors_in(&table, "default");
    assert!(doors.is_empty(), "`default` switches on {doors:?}");
    // What ships is the union of what it names and the defaults it did not switch off.
    let mut shipped = closure(&table, "dist").0;
    shipped.extend(closure(&table, "default").0);
    for door in DOORS {
        assert!(
            !shipped.contains(door),
            "`dist` + `default` switch on `{door}`"
        );
    }
}

/// The shipped extension modules are shipped: the wheel and the PHP extension are `dist` too (the
/// E2E builds add `debug-server` by name).
#[test]
fn the_python_and_the_php_extension_are_shipped_and_hold_no_door() {
    let table = dll_features();
    for shipped in ["python-extension", "php-extension"] {
        let doors = doors_in(&table, shipped);
        assert!(doors.is_empty(), "`{shipped}` switches on {doors:?}");
        let (features, _) = closure(&table, shipped);
        assert!(
            features.contains("dist"),
            "`{shipped}` is not built on `dist`"
        );
    }
}

/// `build-dll` is the developers' and the E2E builds' libazul: `dist` plus the two engines,
/// and never the test hooks (those are asked for by name, `build-dll,test-hooks`).
#[test]
fn build_dll_is_dist_plus_the_two_engines_and_nothing_of_the_test_hooks() {
    let table = dll_features();
    let (dist, _) = closure(&table, "dist");
    let (full, _) = closure(&table, "build-dll");
    assert!(
        dist.is_subset(&full),
        "build-dll lost part of dist: {:?}",
        dist.difference(&full)
    );
    let extra: BTreeSet<&str> = full.difference(&dist).map(String::as_str).collect();
    assert!(
        extra.contains("debug-server") && extra.contains("e2e-scripting"),
        "{extra:?}"
    );
    assert!(
        !full.contains("test-hooks"),
        "build-dll must not switch the test hooks on"
    );
    assert!(
        !dist.contains("build-dll"),
        "dist is the base, not a user of build-dll"
    );
}

/// The mutant of the gate: a door written into `dist` is found, directly or through an implication.
#[test]
fn the_gate_sees_a_door_named_in_the_list_or_reached_through_another_feature() {
    let direct =
        parse_features("[features]\ndist = [\"a\", \"debug-server\"]\ndebug-server = []\na = []\n");
    assert_eq!(doors_in(&direct, "dist"), vec!["feature `debug-server`"]);
    let indirect = parse_features(
        "[features]\ndist = [\"a\"]\na = [\"b\"]\nb = [\"test-hooks\"]\ntest-hooks = []\n",
    );
    assert_eq!(doors_in(&indirect, "dist"), vec!["feature `test-hooks`"]);
    let dependency =
        parse_features("[features]\ndist = [\"a\"]\na = [\"azul-layout?/e2e-server-http\"]\n");
    assert_eq!(
        doors_in(&dependency, "dist"),
        vec!["`azul-layout?/e2e-server-http`"]
    );
    let clean = parse_features("[features]\ndist = [\"a\", \"azul-layout/probe\"]\na = []\n");
    assert!(doors_in(&clean, "dist").is_empty());
}

/// The features forward: the dll's `test-hooks` is what the E2E libazul is built with, so it must
/// switch on core's (which reads the variables) and layout's (which forwards to core's, for a
/// layout built on its own). Without it the hooks stay `NONE` in the build that needs them.
#[test]
fn the_dlls_test_hooks_switch_on_the_hooks_of_core_and_layout() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the dll folder has a parent")
        .to_path_buf();
    let read = |folder: &str| {
        let manifest = root.join(folder).join("Cargo.toml");
        let text = std::fs::read_to_string(&manifest)
            .unwrap_or_else(|e| panic!("read {}: {e}", manifest.display()));
        parse_features(&text)
    };
    let dll = dll_features();
    let (_, dll_entries) = closure(&dll, "test-hooks");
    for must in ["azul-core?/test-hooks", "azul-layout?/test-hooks"] {
        assert!(
            dll_entries.contains(must),
            "the dll's `test-hooks` lost `{must}`: {dll_entries:?}"
        );
    }
    let (_, layout_entries) = closure(&read("layout"), "test-hooks");
    assert!(
        layout_entries.contains("azul-core/test-hooks"),
        "layout's `test-hooks` lost core's: {layout_entries:?}"
    );
    let (core_features, _) = closure(&read("core"), "test-hooks");
    assert!(
        core_features.contains("std"),
        "core's `test-hooks` needs `std` (the module is std-only): {core_features:?}"
    );
}

// ==== the recipes that SHIP build `dist` (X-DELTA-AZDRIVE-KEYS-10) ====

/// The shell commands of a file: lines joined at a trailing `\`, comment lines dropped.
fn commands(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if current.is_empty() && line.starts_with('#') {
            continue;
        }
        current.push_str(line.trim_end_matches('\\'));
        current.push(' ');
        if !line.ends_with('\\') {
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    out
}

/// The features a command asks for: `--features a,b`, `--features="a,b"` or `--features=a,b`.
fn features_asked(command: &str) -> Vec<String> {
    let mut out = Vec::new();
    let words: Vec<&str> = command.split_whitespace().collect();
    for (at, word) in words.iter().enumerate() {
        let value = if let Some(rest) = word.strip_prefix("--features=") {
            Some(rest)
        } else if *word == "--features" {
            words.get(at + 1).copied()
        } else {
            None
        };
        if let Some(value) = value {
            out.extend(
                value
                    .trim_matches(|c| c == '"' || c == '\'')
                    .split(',')
                    .map(|f| f.trim().to_string())
                    .filter(|f| !f.is_empty()),
            );
        }
    }
    out
}

/// What is wrong with a shipping command of azul-dll: it must switch `dist` (or an extension that
/// is built on it) on and no door.
fn shipping_fault(command: &str, table: &BTreeMap<String, Vec<String>>) -> Option<String> {
    let asked = features_asked(command);
    let mut on = BTreeSet::new();
    for feature in &asked {
        on.extend(closure(table, feature).0);
    }
    if !on.contains("dist") {
        return Some(format!("builds without `dist`: {command}"));
    }
    DOORS
        .iter()
        .find(|door| on.contains(**door))
        .map(|door| format!("switches on `{door}` ({asked:?}): {command}"))
}

fn repo_file(path: &str) -> String {
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the dll folder has a parent")
        .join(path);
    std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("read {}: {e}", file.display()))
}

/// Debug builds that are published as such, by what names them in the command.
const DEBUG_ARTIFACTS: [&str; 1] = ["target/azuldbg"];

/// Every release job of the workflow builds the libazul that ships with `dist`, and so does the
/// AUR package, the base image and the cache image: `build-dll` ships the HTTP debugger and the
/// `AZ_E2E=script.json` engine to the customer's machine.
#[test]
fn the_recipes_that_ship_libazul_build_dist_and_never_build_dll() {
    let table = dll_features();
    let mut faults = Vec::new();
    let mut seen = 0;
    for command in commands(&repo_file(".github/workflows/rust.yml")) {
        let shipping = command.contains("azul-dll")
            && command.contains("build")
            && (command.contains("prod-release") || command.contains("rust9x"));
        if !shipping
            || DEBUG_ARTIFACTS
                .iter()
                .any(|a| command.contains(a) && command.contains("debug-server"))
        {
            continue;
        }
        seen += 1;
        faults.extend(shipping_fault(&command, &table));
    }
    assert!(
        seen >= 5,
        "the scanner found {seen} shipping commands in the workflow"
    );
    for file in ["packaging/aur/PKGBUILD", "Dockerfile", "docker/Dockerfile"] {
        let mut here = 0;
        for command in commands(&repo_file(file)) {
            if command.contains("cargo build") && command.contains("azul-dll") {
                here += 1;
                faults.extend(shipping_fault(&command, &table).map(|f| format!("{file}: {f}")));
            }
        }
        assert!(here >= 1, "{file}: no `cargo build -p azul-dll` found");
    }
    assert!(
        faults.is_empty(),
        "shipping recipes that are not `dist`:\n{}",
        faults.join("\n")
    );
}

/// The mutants of the recipe scanner: a release command with `build-dll`, with `dist` and a door
/// by name, and with no `dist` at all are each found; `dist` and the extensions pass.
#[test]
fn the_recipe_scanner_sees_build_dll_a_door_by_name_and_a_missing_dist() {
    let table = dll_features();
    let ok = |c: &str| shipping_fault(c, &table).is_none();
    assert!(ok(
        r#"cargo build --profile prod-release -p azul-dll --features="dist""#
    ));
    assert!(ok(
        "cargo build --release -p azul-dll --lib --features python-extension"
    ));
    assert!(ok(
        "cargo build --release -p azul-dll --features dist,web-transpiler-static"
    ));
    assert!(!ok(
        r#"cargo build --profile prod-release -p azul-dll --features="build-dll""#
    ));
    assert!(!ok(
        "cargo build --release -p azul-dll --features dist,debug-server"
    ));
    assert!(!ok(
        "cargo build --release -p azul-dll --features dist,test-hooks"
    ));
    assert!(!ok("cargo build --release -p azul-dll"));
    // Continuation lines are one command; comments are none.
    let joined = commands("# cargo build -p azul-dll --features build-dll\ncargo build \\\n  -p azul-dll \\\n  --features build-dll\n");
    assert_eq!(joined.len(), 1, "{joined:?}");
    assert!(!ok(&joined[0]));
    assert_eq!(
        features_asked(r#"x --features "a,b" --features=c"#),
        vec!["a", "b", "c"]
    );
}
