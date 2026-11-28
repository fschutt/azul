//! The files an app built OUTSIDE this checkout is built from (`azul-doc`'s embedded mobile assets, `doc/src/mobile/assets.rs`) are the
//! files the build scripts use: a Java class `AzulActivity` imports that is missing from the bundle fails `javac` for every such app, and
//! a script a build calls that is missing fails the build. Ledger WP-F: the share provider, the intent hand-over and the generator of
//! the intent filters were each added to the checkout and not to the bundle.
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/mobile_assets_contract.rs -o /tmp/t && /tmp/t

use std::{collections::BTreeSet, fs, path::Path};

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The paths the bundle embeds: every `"scripts/..."` followed by its `include_str!`.
fn bundled() -> BTreeSet<String> {
    let src = read("doc/src/mobile/assets.rs");
    let mut out = BTreeSet::new();
    let mut rest = src.as_str();
    while let Some(at) = rest.find("\"scripts/") {
        let tail = &rest[at + 1..];
        let end = tail.find('"').expect("an unterminated path");
        let path = &tail[..end];
        let after = &tail[end..];
        if after.trim_start_matches(['"', ',', ' ', '\n']).starts_with("include_str!") {
            out.insert(path.to_string());
        }
        rest = &tail[end..];
    }
    out
}

/// Every Java class of `scripts/android/` is in the bundle: `build-android.sh` compiles them all.
///
/// Mutation: `AzulOpenIntent.java` (or the share provider) out of the bundle.
#[test]
fn every_java_class_the_android_build_compiles_is_in_the_bundle() {
    let bundle = bundled();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("scripts").join("android");
    let mut missing = Vec::new();
    for entry in fs::read_dir(&dir).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().into_owned();
        if name.ends_with(".java") && !bundle.contains(&format!("scripts/android/{name}")) {
            missing.push(name);
        }
    }
    assert!(missing.is_empty(), "missing from doc/src/mobile/assets.rs (an app built outside the checkout fails javac): {missing:?}");
}

/// The scripts the builds call are in the bundle too.
#[test]
fn the_scripts_the_builds_call_are_in_the_bundle() {
    let bundle = bundled();
    for script in ["scripts/documents_manifest.py", "scripts/build-android.sh", "scripts/build-ios.sh", "scripts/cargo_bin_path.py"] {
        assert!(bundle.contains(script), "{script} is called by a build and is not in the bundle");
    }
}
