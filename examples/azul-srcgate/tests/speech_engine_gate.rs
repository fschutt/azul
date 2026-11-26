//! Gate: the vendor's static speech engine is in no shipped build.
//!
//! The cargo feature `speech-engine-static` of azul-dll links the Cactus engine's prebuilt static library
//! (`libcactus.a`) into the program. The engine is binary only, the LICENSE of its release names no copyright
//! holder and the licence of the weights' training data is not stated, so a public artifact (a release, the
//! website, the demos, a crate, an app's build on the release page) must never carry it: only a build of one's
//! own turns the feature on, by hand.
//!
//! "Never carries it" is a property of everything that could switch the feature on, so this gate reads all of
//! it: no workflow, script, recipe, Dockerfile or other manifest names the feature, no other feature of
//! azul-dll lists it (not `default`, not `build-dll`, not `link-static`, not `dist`), and the places that do
//! name it are the ones that belong to it. Documentation (`*.md`) may say how to turn it on.

use std::path::{Path, PathBuf};

use azul_srcgate::workspace;

/// How the feature is written wherever a build could name it.
const FEATURE: &str = "speech-engine-static";

/// The files that belong to the feature, relative to the workspace: its definition, the build script that
/// links the archive, the speech module that is compiled differently with it, the real-engine test, the
/// script that fetches the archive (in comments) and this gate.
const HOME: &[&str] = &[
    "dll/Cargo.toml",
    "dll/build.rs",
    "dll/tests/speech_builtin_engine.rs",
    "scripts/fetch_whistle.sh",
    "examples/azul-srcgate/tests/speech_engine_gate.rs",
];

/// Folders that belong to it as a whole.
const HOME_DIRS: &[&str] = &["dll/src/desktop/extra/speech/"];

/// The largest file read: a recipe or a manifest is far smaller.
const MAX_TEXT_BYTES: u64 = 16 * 1024 * 1024;

/// Folders that hold no sources of ours.
const SKIPPED: &[&str] = &["target", ".git", "node_modules", "third_party"];

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if !SKIPPED.contains(&name.as_str()) {
                walk(&path, out);
            }
        } else {
            out.push(path);
        }
    }
}

/// Why `text` of the file `relative` (a path with `/`, from the workspace) may not name the feature, one
/// sentence for each line that does. Empty when the file is in order.
fn violations(relative: &str, text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let home = HOME.contains(&relative) || HOME_DIRS.iter().any(|dir| relative.starts_with(dir));
    let documentation = Path::new(relative).extension().is_some_and(|e| e.eq_ignore_ascii_case("md"));
    for (index, line) in text.lines().enumerate() {
        if !line.contains(FEATURE) {
            continue;
        }
        let at = index + 1;
        let trimmed = line.trim_start();
        if documentation {
            continue;
        }
        if !home {
            found.push(format!("{relative}:{at}: names `{FEATURE}`, which no build recipe may: {}", line.trim()));
            continue;
        }
        match relative {
            // The definition and comments, nothing else: a list that holds it would switch it on.
            "dll/Cargo.toml" => {
                let definition = trimmed.strip_prefix(FEATURE).map(str::trim_start);
                let is_definition = definition.is_some_and(|rest| rest.replace(' ', "") == "=[]");
                if !trimmed.starts_with('#') && !is_definition {
                    found.push(format!(
                        "{relative}:{at}: `{FEATURE}` is in a list of features (only its own `{FEATURE} = []` may name it): {}",
                        line.trim()
                    ));
                }
            }
            // A script names it only to document how a build of one's own turns it on.
            "scripts/fetch_whistle.sh" => {
                if !trimmed.starts_with('#') {
                    found.push(format!("{relative}:{at}: a script runs something with `{FEATURE}`: {}", line.trim()));
                }
            }
            _ => {}
        }
    }
    found
}

#[test]
fn nothing_that_ships_or_builds_for_others_names_the_static_speech_engine() {
    let root = workspace();
    let mut files = Vec::new();
    walk(&root, &mut files);
    let mut found = Vec::new();
    let mut seen_definition = false;
    for path in files {
        if std::fs::metadata(&path).map_or(true, |m| m.len() > MAX_TEXT_BYTES) {
            continue; // a picture, a font, a database: no recipe is that large
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue; // not text: nothing that could name a feature
        };
        if !text.contains(FEATURE) {
            continue;
        }
        let relative = path.strip_prefix(&root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        if relative == "dll/Cargo.toml" {
            seen_definition = text.lines().any(|line| line.replace(' ', "") == format!("{FEATURE}=[]"));
        }
        found.extend(violations(&relative, &text));
    }
    assert!(seen_definition, "dll/Cargo.toml no longer defines `{FEATURE} = []`: the premise of this gate");
    assert!(found.is_empty(), "the static speech engine would reach a build of others:\n{}", found.join("\n"));
}

// ---- the rule itself, on planted and clean files ----

#[test]
fn a_workflow_or_a_script_or_another_manifest_that_names_it_fails() {
    let step = "      - run: cargo build -p azul-dll --features build-dll,speech-engine-static\n";
    assert_eq!(violations(".github/workflows/rust.yml", step).len(), 1);
    assert_eq!(violations("scripts/build_dist.sh", "cargo build --features speech-engine-static\n").len(), 1);
    assert_eq!(violations("examples/azul-review/Cargo.toml", "azul = { features = [\"speech-engine-static\"] }\n").len(), 1);
    assert_eq!(violations("Dockerfile", "RUN cargo build --features speech-engine-static\n").len(), 1);
}

#[test]
fn a_feature_list_of_azul_dll_that_holds_it_fails_and_its_definition_and_comments_do_not() {
    let own = "[features]\n# the engine: speech-engine-static links it\nspeech-engine-static = []\n";
    assert!(violations("dll/Cargo.toml", own).is_empty());
    let spaced = "speech-engine-static   =   [  ]\n";
    assert!(violations("dll/Cargo.toml", spaced).is_empty());
    let listed = "build-dll = [\"dist\", \"speech-engine-static\"]\n";
    assert_eq!(violations("dll/Cargo.toml", listed).len(), 1);
    let multi = "link-static = [\n    \"desktop\",\n    \"speech-engine-static\",\n]\n";
    assert_eq!(violations("dll/Cargo.toml", multi).len(), 1);
    let not_empty = "speech-engine-static = [\"dep:cc\"]\n";
    assert_eq!(violations("dll/Cargo.toml", not_empty).len(), 1, "the feature turns on nothing else");
}

#[test]
fn the_code_it_gates_documentation_and_the_fetch_script_comments_may_name_it() {
    let cfg = "#[cfg(feature = \"speech-engine-static\")]\n";
    assert!(violations("dll/src/desktop/extra/speech/builtin/engine.rs", cfg).is_empty());
    assert!(violations("dll/build.rs", "env::var(\"CARGO_FEATURE_SPEECH_ENGINE_STATIC\")\n// speech-engine-static\n").is_empty());
    assert!(violations("docs/speech.md", "cargo build --features speech-engine-static\n").is_empty());
    assert!(violations("scripts/fetch_whistle.sh", "#   the feature `speech-engine-static` links it\n").is_empty());
    let runs = "cargo build -p azul-dll --features speech-engine-static\n";
    assert_eq!(violations("scripts/fetch_whistle.sh", runs).len(), 1, "a script may document it, not run it");
}
