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
//! The behavioural half of the gate (with the test variables set and no `test-hooks`, the
//! keyring, `Url::open` and the sign-in take their production paths) belongs to the lane that
//! moves the other consumers behind `test-hooks`.

use std::collections::{BTreeMap, BTreeSet};

/// The features no shipped build may have.
const DOORS: [&str; 4] = ["test-hooks", "debug-server", "e2e-scripting", "e2e-test"];

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
