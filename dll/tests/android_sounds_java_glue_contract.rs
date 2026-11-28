//! The Android sound player is a Rust call into a Java helper, and what can go wrong between the two
//! only shows on a phone, pinned here in source (the JVM cannot run where the tests run):
//!
//! * a JNI signature that does not match the Java method is a `NoSuchMethodError` at the first
//!   sound, on the device only: every `public static` method of `AzulSounds.java` has the name and
//!   the descriptor `dll/src/desktop/shell2/android/sounds.rs` calls it by;
//! * the effect is played with the ONE-argument `AudioManager.playSoundEffect(int)`, which applies
//!   the user's "Touch sounds" switch; the overload with a volume is for apps with a panel of their
//!   own and does not look at the switch (AOSP `AudioService.playSoundEffectVolume`), so using it
//!   would play a click for a user who turned clicks off;
//! * an exception that escapes the helper ends the app (it runs on the native thread), so every
//!   public method catches `Throwable` and answers with a line of text;
//! * the notification tone is made from the application's context: a static `Ringtone` keeps what it
//!   was made with alive, and an activity must not be what it keeps.
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/android_sounds_java_glue_contract.rs -o /tmp/t && /tmp/t

use std::{fs, path::Path};

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Java source without its `//` comments and its javadoc.
fn java_code(rel: &str) -> String {
    let source = read(rel);
    let mut out = String::new();
    let mut in_block = false;
    for line in source.lines() {
        let line = line.trim();
        if in_block {
            if line.contains("*/") {
                in_block = false;
            }
            continue;
        }
        if line.starts_with("/*") {
            in_block = !line.contains("*/");
            continue;
        }
        out.push_str(line.split("//").next().unwrap_or(""));
        out.push('\n');
    }
    out
}

const JAVA: &str = "scripts/android/AzulSounds.java";
const RUST: &str = "dll/src/desktop/shell2/android/sounds.rs";

/// The JNI descriptor of a Java type of the helper's signatures.
fn descriptor(java_type: &str) -> &'static str {
    match java_type {
        "Activity" => "Landroid/app/Activity;",
        "String" => "Ljava/lang/String;",
        "boolean" => "Z",
        "int" => "I",
        "void" => "V",
        other => panic!("AzulSounds.java uses a type this test has no descriptor for: {other}"),
    }
}

/// `(name, descriptor)` of every `public static` method of the helper.
fn public_static_methods(code: &str) -> Vec<(String, String)> {
    let mut methods = Vec::new();
    for line in code.lines() {
        let Some(rest) = line.trim().strip_prefix("public static ") else {
            continue;
        };
        let Some(open) = rest.find('(') else { continue };
        let Some(close) = rest.find(')') else {
            continue;
        };
        let mut head = rest[..open].split_whitespace();
        let returns = head.next().expect("a return type");
        let name = head.next().expect("a method name");
        let params: Vec<&str> = rest[open + 1..close]
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(|p| p.split_whitespace().next().expect("a parameter type"))
            .collect();
        let mut sig = String::from("(");
        for p in params {
            sig.push_str(descriptor(p));
        }
        sig.push(')');
        sig.push_str(descriptor(returns));
        methods.push((name.to_string(), sig));
    }
    methods
}

#[test]
fn every_static_method_of_the_helper_is_called_by_the_name_and_descriptor_it_has() {
    let methods = public_static_methods(&java_code(JAVA));
    let names: Vec<&str> = methods.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        ["touchSoundsEnabled", "playEffect", "playTone", "playCamera"],
        "the public surface of AzulSounds.java changed: say here what Rust calls"
    );
    let rust = read(RUST);
    for (name, sig) in &methods {
        assert!(
            rust.contains(&format!("\"{name}\"")),
            "sounds.rs never calls {name}"
        );
        assert!(
            rust.contains(&format!("\"{sig}\"")),
            "sounds.rs does not call {name} with the descriptor {sig} that AzulSounds.java declares"
        );
    }
}

#[test]
fn the_effect_is_played_with_the_one_argument_overload_that_obeys_touch_sounds() {
    let code = java_code(JAVA);
    let calls: Vec<&str> = code
        .lines()
        .filter(|line| line.contains(".playSoundEffect("))
        .collect();
    assert_eq!(calls.len(), 1, "one call of playSoundEffect: {calls:?}");
    assert!(
        calls[0].contains("playSoundEffect(effect)"),
        "the overload with a volume skips the user's switch: {}",
        calls[0]
    );
    assert!(
        code.contains("Settings.System.SOUND_EFFECTS_ENABLED"),
        "the switch is read, so that 'off' is an answer and not a silence"
    );
}

#[test]
fn a_missing_switch_reads_as_off_as_the_audio_service_reads_it() {
    let code = java_code(JAVA);
    assert!(
        code.contains("Settings.System.SOUND_EFFECTS_ENABLED, 0) != 0"),
        "AudioService asks for the row with the default 0; a different default would say 'played' \
         for a click the service drops"
    );
}

#[test]
fn no_public_method_of_the_helper_lets_an_exception_out() {
    let code = java_code(JAVA);
    for name in ["touchSoundsEnabled", "playEffect", "playTone", "playCamera"] {
        let start = code
            .find(&format!(" {name}("))
            .unwrap_or_else(|| panic!("no {name} in the helper"));
        let rest = &code[start..];
        let end = rest[1..]
            .find("\n    public static ")
            .map_or(rest.len(), |e| e + 1);
        assert!(
            rest[..end].contains("catch (Throwable"),
            "{name} must catch Throwable: an exception that escapes it ends the app"
        );
    }
    assert!(
        !code.contains(" native "),
        "the helper has no native methods: it is called, and calls nothing back"
    );
}

#[test]
fn the_notification_tone_is_made_from_the_application_context() {
    let code = java_code(JAVA);
    assert!(
        code.contains("RingtoneManager.getRingtone(activity.getApplicationContext()"),
        "a static Ringtone keeps its context alive: it must not be an activity"
    );
    assert!(
        code.contains("AudioAttributes.USAGE_NOTIFICATION"),
        "the tone plays as a notification, so the ringer mode and Do Not Disturb apply"
    );
}
