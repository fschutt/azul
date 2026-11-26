//! The shipped demos must be named consistently, and the release page must
//! link the names that are actually produced.
//!
//! This is a guard for a class that has now bitten twice in one release:
//!
//! 1. `examples/azul-writer` declared `[[bin]] name = "azwriter"` — the only demo whose binary name
//!    differed from its package name. CI staged `target/release/${package}`, a path that never
//!    existed, and a `[reuse]` branch swallowed the miss. The 0.2.0 release shipped ZERO
//!    azul-writer assets on all three desktop OSes while the job stayed green, and the release page
//!    advertised three downloads that 404.
//! 2. The resolver written to fix that did not resolve it either: cargo omits
//!    the package name after `#` when the directory is already named after the
//!    package (`path+file:///…/examples/azul-writer#0.1.0`), a spelling the
//!    matcher did not know. That turned a silent miss into a hard build failure.
//!
//! Both bugs are the same shape: a name is derived in one place and consumed in
//! another, and nothing checks that the two agree. So this asserts the
//! agreement directly, from the manifests, without needing a build.
//!
//! WHICH apps: every `[[app]]` of `scripts/demo_apps.toml`, the ONE list of the demo apps (rust.yml
//! builds from it, the release page is generated from it). This file keeps no list of its own, and
//! it does not read the workflow: `scripts/demo_apps.py check` is what holds rust.yml to the
//! manifest (it fails on a hard-coded list of apps there).
//!
//! The manifest is read line by line, as the `[[bin]]` tables of the apps are: this crate has no
//! toml dependency, and a test of the reader (below) pins the little of the format it relies on
//! (`package` and `dir` of an `[[app]]`, and whether the app has a `known_failing` table).

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("dll/ has a parent")
        .to_path_buf()
}

/// One `[[app]]` of `scripts/demo_apps.toml`, as far as this test needs it.
#[derive(Debug, PartialEq, Eq)]
struct DemoApp {
    /// The cargo package: package, `[[bin]]` and the stem of every asset are this one string.
    package: String,
    /// The crate's directory, relative to the repository root.
    dir: String,
    /// The manifest lists a build of the app as a known failure (`known_failing`): the app is not
    /// held to a binary it does not have yet.
    pending: bool,
}

/// The value of `key = "value"` on a line (a string value, with or without a trailing comment).
fn string_value(line: &str, key: &str) -> Option<String> {
    let rest = line.strip_prefix(key)?.trim_start().strip_prefix('=')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    Some(rest[..rest.find('"')?].to_string())
}

/// The apps of a `scripts/demo_apps.toml` text: `package` and `dir` of each `[[app]]` table, and
/// whether it has a `known_failing` table (or key). Sub-tables (`[app.android]`, `[app.ios.usage]`)
/// belong to the app above them; any other table ends it.
fn parse_apps(text: &str) -> Vec<DemoApp> {
    let mut apps: Vec<(Option<String>, Option<String>, bool)> = Vec::new();
    // true while the lines are the keys of an [[app]] table itself (not of a table below or beside it)
    let mut in_app_table = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            let header = line.split('#').next().unwrap_or(line).trim();
            in_app_table = header == "[[app]]";
            if in_app_table {
                apps.push((None, None, false));
            } else if header == "[app.known_failing]" {
                apps.last_mut().expect("[app.known_failing] follows an [[app]]").2 = true;
            }
            continue;
        }
        if !in_app_table {
            continue;
        }
        let app = apps.last_mut().expect("in an app table");
        if let Some(v) = string_value(line, "package") {
            app.0 = Some(v);
        } else if let Some(v) = string_value(line, "dir") {
            app.1 = Some(v);
        } else if line.starts_with("known_failing") {
            // the inline form: `known_failing = { android = "why" }`
            app.2 = true;
        }
    }
    apps.into_iter()
        .map(|(package, dir, pending)| DemoApp {
            package: package.expect("every [[app]] has a package"),
            dir: dir.expect("every [[app]] has a dir"),
            pending,
        })
        .collect()
}

fn demo_apps(root: &Path) -> Vec<DemoApp> {
    let text = std::fs::read_to_string(root.join("scripts/demo_apps.toml"))
        .expect("scripts/demo_apps.toml is readable");
    let apps = parse_apps(&text);
    // the reader must not quietly miss an app: as many apps as [[app]] tables
    let tables = text.lines().filter(|l| l.trim() == "[[app]]").count();
    assert_eq!(apps.len(), tables, "the line reader of the manifest lost an [[app]] table");
    assert!(!apps.is_empty(), "scripts/demo_apps.toml lists no app");
    apps
}

/// The Cargo.toml of an app, as (the `[package]` name, the `name` of every `[[bin]]`).
fn cargo_names(root: &Path, app: &DemoApp) -> (Option<String>, Vec<String>) {
    let path = root.join(&app.dir).join("Cargo.toml");
    let manifest = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} has no readable Cargo.toml ({}): {e}", app.package, path.display()));
    let mut package = None;
    let mut bins = Vec::new();
    let mut table = String::new();
    for line in manifest.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            table = t.split('#').next().unwrap_or(t).trim().to_string();
            continue;
        }
        if table == "[package]" && package.is_none() {
            package = string_value(t, "name");
        } else if table == "[[bin]]" {
            if let Some(name) = string_value(t, "name") {
                bins.push(name);
            }
        }
    }
    (package, bins)
}

/// The line reader of the manifest reads what the format gives an `[[app]]`: its package and dir, and
/// the sub-tables, arrays and comments around them do not confuse it.
#[test]
fn the_reader_of_the_manifest_takes_the_package_and_the_dir_of_each_app() {
    let text = r#"
schema = 1
[lib.desktop]
features = "dist"
package = "NotAnApp"           # a key of a table that is no app

# the apps
[[app]]
package = "AzOne"              # first
dir = "examples/azul-one"
description = "dir = \"not this\""
platforms = ["linux"]
[app.android]
permissions = [
    "android.permission.CAMERA",
]
[app.ios.strings.de]
package = "NotThisEither"

[[app]]
dir = "examples/azul-two"
package = "AzTwo"
known_failing = { android = "why" }

[[app]]
package = "AzThree"
dir = "examples/azul-three"
[app.known_failing]
ios = "no bin"

[plan]
dir = "elsewhere"
"#;
    assert_eq!(
        parse_apps(text),
        [
            DemoApp { package: "AzOne".into(), dir: "examples/azul-one".into(), pending: false },
            DemoApp { package: "AzTwo".into(), dir: "examples/azul-two".into(), pending: true },
            DemoApp { package: "AzThree".into(), dir: "examples/azul-three".into(), pending: true },
        ]
    );
    assert_eq!(string_value("name = \"x\" # y", "name"), Some("x".to_string()));
    assert_eq!(string_value("names = \"x\"", "name"), None);
    assert_eq!(string_value("name = 3", "name"), None);
}

/// Every demo the manifest lists must exist as a package, in the directory the manifest names.
#[test]
fn every_demo_of_the_manifest_exists() {
    let root = repo_root();
    let apps = demo_apps(&root);
    for app in &apps {
        assert!(
            root.join(&app.dir).join("Cargo.toml").is_file(),
            "scripts/demo_apps.toml lists `{}` but {}/Cargo.toml does not exist",
            app.package,
            app.dir
        );
        let (package, _) = cargo_names(&root, app);
        assert_eq!(
            package.as_deref(),
            Some(app.package.as_str()),
            "{}/Cargo.toml declares another package than the manifest's `{}`",
            app.dir,
            app.package
        );
    }
    let mut packages: Vec<&str> = apps.iter().map(|a| a.package.as_str()).collect();
    packages.sort_unstable();
    let before = packages.len();
    packages.dedup();
    assert_eq!(packages.len(), before, "an app is listed twice in scripts/demo_apps.toml");
}

/// The asset name CI produces is `<package>-<os>`; the binary it copies is
/// whatever cargo built. Those must be the same name, or the copy misses.
///
/// This is the azul-writer bug, asserted from the manifests: a demo may declare
/// `[[bin]]` names, but one of them must equal the package name, because that is what
/// the staging step and the release page both assume. (An app whose build is listed in
/// `known_failing` says in the manifest why it does not have that binary yet, and is not held to it.)
#[test]
fn a_demos_binary_is_named_after_its_package() {
    let root = repo_root();
    let mut wrong = Vec::new();
    for app in demo_apps(&root) {
        let (_, bins) = cargo_names(&root, &app);
        if !bins.is_empty() && !bins.contains(&app.package) && !app.pending {
            wrong.push(format!("{} declares [[bin]] name = {:?}", app.package, bins));
        }
    }
    assert!(
        wrong.is_empty(),
        "these demos build no binary named after their package: {}\n\nCI stages \
         `<package>` out of cargo's resolved executable and the release page links \
         `<package>-<os>.tar.gz`, so a differing bin name means the asset is never produced and the \
         download 404s — silently, because the staging step treats the miss as a deliberate reuse. \
         Either rename the bin to match the package, or (while the app does not build yet) list \
         the build in `known_failing` of scripts/demo_apps.toml with the reason.",
        wrong.join("; ")
    );
}

/// Package, binary and release asset are all one string, so it is the name users see in a download
/// URL: it follows the `AzXxx` convention.
#[test]
fn every_demo_follows_the_azxxx_convention() {
    for app in demo_apps(&repo_root()) {
        let d = &app.package;
        assert!(
            d.starts_with("Az")
                && d.len() > 2
                && d.as_bytes()[2].is_ascii_uppercase()
                && d.chars().all(|c| c.is_ascii_alphanumeric()),
            "demo package `{d}` does not follow the AzXxx convention — package, binary and \
             release asset are all this one string, so it is the name users see in a download URL"
        );
    }
}
