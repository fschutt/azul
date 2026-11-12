//! Gate: the cargo feature `memory-keyring` of azul-storage is named in `[dev-dependencies]` tables
//! only (X-TESTFIX-AZUL2-4).
//!
//! The feature makes `azul_keyring::azlin_keyring()` - the ONE place the Azlin apps' keyring is built -
//! answer with a keyring in the process's memory: no system call, no file, one map per process. In a
//! TEST build that is the point (the libazul the apps' tests link ignores `AZ_KEYRING=memory`, and a
//! test that reached the system keyring wrote the owner's real login keychain: two AzDrive tests left
//! their drives' keys there and a rebuilt test binary then blocked on the keychain's access prompt). In
//! a build that SHIPS it would keep every drive key in memory only - an encrypted drive locked at the
//! next start for good. A `[dependencies]` entry turns a feature on in the build that ships, and so does
//! a feature list (`e2e = ["azul-storage/memory-keyring"]`, `default = [..]`) that a release build
//! enables: so the name stands in `[dev-dependencies]` tables (a test build's), and in its own
//! definition - the key `memory-keyring = []` of azul-storage's `[features]` - and nowhere else.
//!
//! What it reads: every `Cargo.toml` of the workspace's own crates (not `target`, not `third_party`),
//! as text. A line belongs to the table whose `[header]` was the last above it; a comment is not
//! read; a name in a key, a string or an array element counts alike. A table is a dev table when one
//! of the names of its header is `dev-dependencies` (`[dev-dependencies]`,
//! `[target.'cfg(unix)'.dev-dependencies]`, `[dev-dependencies.azul-storage]`).

use std::path::{Path, PathBuf};

use azul_srcgate::workspace;

/// The feature's name.
const FEATURE: &str = "memory-keyring";

/// The manifest that defines the feature, and the one line it does it in (spaces ignored).
const DEFINED_IN: &str = "examples/azul-storage/Cargo.toml";
const DEFINITION: &str = "memory-keyring=[]";

/// The crates whose tests reach `azlin_keyring()` - AzDrive, AzMail and, with its `os-keyring`
/// feature, the bridge - and so must name the feature in their `[dev-dependencies]`: the tests of
/// each of them assert the keyring they reach is a map, and this keeps the gate from passing over a
/// tree where nobody names it any more.
const NAMING: &[&str] = &[
    "examples/azul-drive/Cargo.toml",
    "examples/azul-mail/Cargo.toml",
    "examples/azul-bridge/Cargo.toml",
];

/// Folders whose manifests are not the workspace's own crates, or that hold no crate.
const NOT_READ: &[&str] = &[
    "target",
    "third_party",
    "node_modules",
    "webrender",
    "supply-chain",
    "_build",
    "assets",
    "packaging",
    "docker",
];

/// How deep below the workspace's root a manifest is looked for (`examples/azul-storage/fuzz/`).
const DEPTH: usize = 4;

/// One place a manifest names the feature.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Named {
    /// The manifest, from the workspace's root.
    rel: String,
    /// The line (from 1).
    line: usize,
    /// The header of the table the line is in (`dependencies`, `target.'cfg(unix)'.dev-dependencies`),
    /// empty above the first table.
    table: String,
    /// The line without its comment, trimmed.
    text: String,
}

/// `line` without its comment: the text before a `#` that is not inside a string.
fn without_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    for (at, c) in line.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '#') => return &line[..at],
            (None, _) => {}
        }
    }
    line
}

/// The names of a table header, in order: `[target.'cfg(unix)'.dev-dependencies]` is `target`,
/// `cfg(unix)`, `dev-dependencies`. `None` for a line that is no header: a line of a multi-line array
/// that starts with a bracket (`["a", "b"],`) has an `=`, a `,` or a bracket outside a string.
fn header_names(line: &str) -> Option<Vec<String>> {
    let inner = line.strip_prefix('[')?;
    let inner = inner.strip_prefix('[').unwrap_or(inner);
    let inner = inner.strip_suffix(']')?;
    let inner = inner.strip_suffix(']').unwrap_or(inner);
    let mut names = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for c in inner.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => current.push(c),
            (None, '"' | '\'') => quote = Some(c),
            (None, '.') => names.push(std::mem::take(&mut current)),
            (None, c) if c.is_whitespace() => {}
            (None, '=' | ',' | '[' | ']') => return None,
            (None, c) => current.push(c),
        }
    }
    if quote.is_some() {
        return None;
    }
    names.push(current);
    names.iter().all(|name| !name.is_empty()).then_some(names)
}

/// Every line of `manifest` that names the feature, with its table.
fn named_in(rel: &str, manifest: &str) -> Vec<Named> {
    let mut table = String::new();
    let mut found = Vec::new();
    for (at, raw) in manifest.lines().enumerate() {
        let text = without_comment(raw).trim();
        if text.starts_with('[') {
            if let Some(names) = header_names(text) {
                table = names.join(".");
                continue;
            }
        }
        if text.contains(FEATURE) {
            found.push(Named {
                rel: rel.to_string(),
                line: at + 1,
                table: table.clone(),
                text: text.to_string(),
            });
        }
    }
    found
}

/// Whether the table is a `[dev-dependencies]` one: one of its names is that.
fn is_dev_table(table: &str) -> bool {
    table.split('.').any(|name| name == "dev-dependencies")
}

/// Whether the line is the feature's own definition: its key in the `[features]` table of the
/// manifest that defines it, and nothing in its list.
fn is_definition(named: &Named) -> bool {
    named.rel == DEFINED_IN
        && named.table == "features"
        && named.text.replace(' ', "") == DEFINITION
}

/// The places that break the rule: a name outside a dev table that is not the definition.
fn findings(named: &[Named]) -> Vec<String> {
    named
        .iter()
        .filter(|n| !is_dev_table(&n.table) && !is_definition(n))
        .map(|n| {
            format!(
                "{}:{} names `{FEATURE}` in [{}]: `{}` (a dev-dependency of a crate whose tests reach \
                 the keyring, and nothing else, may)",
                n.rel, n.line, n.table, n.text
            )
        })
        .collect()
}

/// The manifests below `dir`, from `base` (relative paths with `/`).
fn manifests(base: &Path, dir: &Path, depth: usize, out: &mut Vec<String>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = read.flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_file() && name == "Cargo.toml" {
            if let Ok(rel) = path.strip_prefix(base) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        } else if kind.is_dir()
            && depth > 0
            && !name.starts_with('.')
            && !NOT_READ.contains(&name.as_str())
        {
            manifests(base, &path, depth - 1, out);
        }
    }
}

/// Every place of the tree below `base` that names the feature, and how many manifests were read.
fn read_tree(base: &Path) -> (Vec<Named>, usize) {
    let mut paths = Vec::new();
    manifests(base, base, DEPTH, &mut paths);
    let mut named = Vec::new();
    for rel in &paths {
        let text = std::fs::read_to_string(base.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
        named.extend(named_in(rel, &text));
    }
    (named, paths.len())
}

#[test]
fn the_feature_is_named_in_dev_dependencies_only() {
    let (named, manifests) = read_tree(&workspace());
    assert!(
        manifests >= 40,
        "the gate read {manifests} manifests: the tree it walks is not the workspace"
    );
    let wrong = findings(&named);
    assert!(wrong.is_empty(), "\n{}\n", wrong.join("\n"));
}

#[test]
fn the_feature_is_defined_once_as_an_empty_list_and_the_crates_that_reach_the_keyring_name_it() {
    let (named, _) = read_tree(&workspace());
    let definitions: Vec<&Named> = named.iter().filter(|n| is_definition(n)).collect();
    assert_eq!(
        definitions.len(),
        1,
        "`{DEFINITION}` in the [features] of {DEFINED_IN}, once: {named:#?}"
    );
    for rel in NAMING {
        assert!(
            named
                .iter()
                .any(|n| n.rel == *rel && is_dev_table(&n.table)),
            "{rel} does not name `{FEATURE}` in its [dev-dependencies]: its tests would reach the \
             system keychain"
        );
    }
}

/// A tree on disk below a fresh base: (path below the base, text).
fn base_with(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "azul-srcgate-memory-keyring-{name}-{}",
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

/// The places a release build can turn the feature on, planted: a `[dependencies]` entry (plain, in a
/// target table and as its own table), a `[build-dependencies]` one, another crate's feature list,
/// a `default` list and a workspace table are findings with their lines; the places a test build
/// names it, a comment and the definition are none.
#[test]
fn a_planted_name_outside_a_dev_dependencies_table_is_caught_and_the_dev_ones_are_not() {
    let base = base_with(
        "planted",
        &[
            (
                "examples/shipped/Cargo.toml",
                concat!(
                    "[package]\nname = \"shipped\"\n",
                    "[dependencies]\n",
                    "azul-storage = { path = \"../azul-storage\", features = [\"azul\", \"memory-keyring\"] }\n",
                    "[target.'cfg(unix)'.dependencies]\n",
                    "azul-storage = { path = \"../azul-storage\", features = [\"memory-keyring\"] }\n",
                    "[build-dependencies]\n",
                    "azul-storage = { path = \"../azul-storage\", features = [\"memory-keyring\"] }\n",
                    "[features]\n",
                    "e2e = [\"azul-storage/memory-keyring\"]\n",
                ),
            ),
            (
                "examples/own-table/Cargo.toml",
                concat!(
                    "[dependencies.azul-storage]\n",
                    "path = \"../azul-storage\"\n",
                    "features = [\n",
                    "    \"azul\",\n",
                    "    \"memory-keyring\",\n",
                    "]\n",
                ),
            ),
            (
                "examples/azul-storage/Cargo.toml",
                concat!(
                    "[features]\n",
                    "default = [\"memory-keyring\"]\n",
                    "# memory-keyring in a comment is not read\n",
                    "memory-keyring = []\n",
                ),
            ),
            (
                "Cargo.toml",
                "[workspace.dependencies]\nazul-storage = { features = [\"memory-keyring\"] }\n",
            ),
            (
                "examples/tests-only/Cargo.toml",
                concat!(
                    "[dev-dependencies]\n",
                    "azul-storage = { path = \"../azul-storage\", features = [\"testing\", \"memory-keyring\"] }\n",
                    "[target.'cfg(unix)'.dev-dependencies]\n",
                    "azul-storage = { path = \"../azul-storage\", features = [\"memory-keyring\"] } # a comment\n",
                    "[dev-dependencies.azul-storage]\n",
                    "path = \"../azul-storage\"\n",
                    "features = [\"memory-keyring\"]\n",
                ),
            ),
        ],
    );
    let (named, read) = read_tree(&base);
    assert_eq!(read, 5, "{named:#?}");
    let wrong = findings(&named);
    let at = |rel: &str, line: usize| {
        wrong
            .iter()
            .any(|w| w.starts_with(&format!("{rel}:{line} ")))
    };
    assert!(
        at("examples/shipped/Cargo.toml", 4),
        "[dependencies]: {wrong:#?}"
    );
    assert!(
        at("examples/shipped/Cargo.toml", 6),
        "a target table: {wrong:#?}"
    );
    assert!(
        at("examples/shipped/Cargo.toml", 8),
        "[build-dependencies]: {wrong:#?}"
    );
    assert!(
        at("examples/shipped/Cargo.toml", 10),
        "a feature list: {wrong:#?}"
    );
    assert!(
        at("examples/own-table/Cargo.toml", 5),
        "[dependencies.x]: {wrong:#?}"
    );
    assert!(
        at("examples/azul-storage/Cargo.toml", 2),
        "a default list: {wrong:#?}"
    );
    assert!(at("Cargo.toml", 2), "[workspace.dependencies]: {wrong:#?}");
    // The definition (line 4 of azul-storage's manifest), the three dev tables, the comment.
    assert_eq!(wrong.len(), 7, "{wrong:#?}");
    assert!(
        named
            .iter()
            .any(|n| n.rel == DEFINED_IN && n.line == 4 && is_definition(n)),
        "the definition is found and taken: {named:#?}"
    );
    assert!(
        named
            .iter()
            .filter(|n| n.rel == "examples/tests-only/Cargo.toml")
            .all(|n| is_dev_table(&n.table)),
        "{named:#?}"
    );
    assert!(
        named.iter().all(|n| !(n.rel == DEFINED_IN && n.line == 3)),
        "a name in a comment is not read: {named:#?}"
    );
    let _ = std::fs::remove_dir_all(base);
}

/// The definition is held to its text: a feature that lists another feature (one that pulls a
/// dependency or a second switch in with it), or a second definition elsewhere, is a finding.
#[test]
fn a_definition_that_lists_something_or_stands_in_another_manifest_is_a_finding() {
    let listing = base_with(
        "listing",
        &[(
            "examples/azul-storage/Cargo.toml",
            "[features]\nmemory-keyring = [\"azul\"]\n",
        )],
    );
    let (named, _) = read_tree(&listing);
    assert_eq!(findings(&named).len(), 1, "{named:#?}");
    let _ = std::fs::remove_dir_all(listing);

    let elsewhere = base_with(
        "elsewhere",
        &[(
            "examples/other/Cargo.toml",
            "[features]\nmemory-keyring = []\n",
        )],
    );
    let (named, _) = read_tree(&elsewhere);
    assert_eq!(findings(&named).len(), 1, "{named:#?}");
    let _ = std::fs::remove_dir_all(elsewhere);

    let right = base_with(
        "right",
        &[(
            "examples/azul-storage/Cargo.toml",
            "[features]\nmemory-keyring = [] # tests only\n",
        )],
    );
    let (named, _) = read_tree(&right);
    assert_eq!(findings(&named), Vec::<String>::new(), "{named:#?}");
    let _ = std::fs::remove_dir_all(right);
}

/// A header is a header and an array element that starts with a bracket is not one.
#[test]
fn a_header_is_told_from_a_line_of_an_array() {
    assert_eq!(
        header_names("[target.'cfg(any(unix, windows))'.dev-dependencies]"),
        Some(vec![
            String::from("target"),
            String::from("cfg(any(unix, windows))"),
            String::from("dev-dependencies"),
        ])
    );
    assert_eq!(header_names("[[bin]]"), Some(vec![String::from("bin")]));
    assert_eq!(header_names("[\"a\", \"b\"],"), None);
    assert_eq!(header_names("[\"a\", \"b\"]"), None);
    assert_eq!(header_names("[]"), None);
    assert_eq!(
        without_comment("a = \"# not a comment\" # a comment"),
        "a = \"# not a comment\" "
    );
}
