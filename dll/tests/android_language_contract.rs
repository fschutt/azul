//! The Android shell hands the phone's language to the engine, at the start and when it changes.
//!
//! The style `App::create` makes of the platform defaults says `en-US` for every phone, and nothing read the
//! device's language: a German phone showed English (the 2026-11-26 emulator pictures, apps that register
//! bundles for `en` and `de`). The shell reads the activity's configuration (the NDK's language and country),
//! makes the BCP 47 tag of it with the engine's `SystemLanguage::from_platform_tag` (host-tested in
//! `css/src/system_language_test.rs`: the tag matrix of en-US, en_US, de-DE, pt-BR, an empty tag, `und`, the
//! old Java codes) and puts it into the style the window is made with, and into the style it adopts when the
//! configuration changes (the manifest lists `locale`: the activity is not recreated, the engine translates
//! the laid-out text again in place or builds `layout()` again when it read the locale).
//!
//! What only the device can run is pinned by reading the source, like the other android_*.rs files:
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/android_language_contract.rs -o /tmp/t && /tmp/t

use std::{fs, path::Path};

/// The code of a Rust file: comments and doc comments dropped.
fn code_of(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    let src = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    src.lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

fn shell() -> String {
    code_of("dll/src/desktop/shell2/android/mod.rs")
}

/// The text of `fn <name>(` up to the closing brace at column 0.
fn function<'a>(code: &'a str, name: &str) -> &'a str {
    let start = code
        .find(&format!("fn {name}("))
        .unwrap_or_else(|| panic!("the shell has `fn {name}(`"));
    let rest = &code[start..];
    let end = rest.find("\n}\n").unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn the_language_is_made_of_the_configurations_language_and_country_by_the_engines_function() {
    let code = shell();
    let language_of = function(&code, "language_of");
    assert!(language_of.contains(".language()"), "the NDK's AConfiguration_getLanguage");
    assert!(language_of.contains(".country()"), "and its country");
    assert!(
        language_of.contains("SystemLanguage::from_platform_tag"),
        "the tag is made by the engine's host-tested function, not by a copy of it in the shell"
    );
}

#[test]
fn the_window_is_made_with_the_phones_language_not_the_platform_defaults() {
    let code = shell();
    let main = function(&code, "android_main");
    let language = main
        .find("config.system_style.language = language")
        .expect("android_main puts the phone's language into the style");
    let window = main
        .find("AndroidWindow::new(")
        .expect("android_main makes the window");
    assert!(
        language < window,
        "the style is cloned into the window when it is made: the language has to be in it by then"
    );
    assert!(main.contains("mut config"), "the config is changed before the window takes it");
    assert!(
        main.contains("language_of(&app, config.localization.known_languages.as_slice())"),
        "the app's known languages give the right-to-left-ness"
    );
}

#[test]
fn a_changed_language_is_adopted_by_the_engine_and_not_by_the_shell() {
    let code = shell();
    let event: String = {
        let start = code.find("MainEvent::ConfigChanged").expect("the shell handles ConfigChanged");
        code[start..].chars().take(1200).collect()
    };
    assert!(
        event.contains("adopt_language(app, window)"),
        "a configuration change re-reads the language (the manifest lists `locale`: no restart)"
    );
    let adopt = function(&code, "adopt_language");
    assert!(adopt.contains("language_of("));
    assert!(
        adopt.contains("snapshot_window_state_baseline("),
        "snapshot -> mutate -> pass, like every other OS-sourced change"
    );
    assert!(
        adopt.contains("adopt_system_style("),
        "the engine relocalizes in place or rebuilds layout(): the shell does not translate anything"
    );
    assert!(
        adopt.contains("system_style.language == language"),
        "no pass for a configuration change that is not a language change (density, font scale)"
    );
}
