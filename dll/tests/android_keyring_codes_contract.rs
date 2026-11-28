//! The numbers the Android keyring and biometric helpers (Java) and their Rust backends agree on, pinned in source.
//!
//! The Rust mappings (`keyring/android.rs`: `result_of`, `need_of`, `support_of`; `biometric/android.rs`: `map_result`, `map_kind`)
//! are compiled for Android only, so their own tests (`android_test.rs`) do not run on a host; a phone cannot run where the tests run.
//! A number that drifts between the two files turns a locked-out Keystore into a stored secret without a compiler noticing, so each
//! table is written out here once, the Java file must declare it as written, and the Rust function must map it as written.
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/android_keyring_codes_contract.rs -o /tmp/t && /tmp/t

use std::{fs, path::Path};

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Java source without its line comments.
fn java_code(rel: &str) -> String {
    read(rel)
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Rust source without its line comments and without whitespace (rustfmt breaks the arms over lines).
fn rust_code(rel: &str) -> String {
    read(rel)
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<String>()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// The body of a method or function of `src` starting at `signature` (up to its matching brace).
fn body<'a>(src: &'a str, signature: &str) -> &'a str {
    let start = src
        .find(signature)
        .unwrap_or_else(|| panic!("no `{signature}` in the source"));
    let open = start + src[start..].find('{').unwrap();
    let mut depth = 0;
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &src[open..open + i + 1];
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced braces after `{signature}`");
}

/// The runs of consecutive `private static final int NAME = N;` lines of a Java file, each as `(NAME, N)` in order. A blank line or a
/// comment ends a run, so each table of the file is one run.
fn int_runs(java: &str) -> Vec<Vec<(String, i64)>> {
    let mut runs: Vec<Vec<(String, i64)>> = Vec::new();
    let mut open = false;
    for line in java.lines() {
        let line = line.trim();
        let declared = line
            .strip_prefix("private static final int ")
            .and_then(|rest| rest.strip_suffix(';'))
            .and_then(|rest| rest.split_once(" = "))
            .and_then(|(name, value)| value.trim().parse::<i64>().ok().map(|v| (name.trim().to_string(), v)));
        match declared {
            Some(entry) => {
                if !open {
                    runs.push(Vec::new());
                    open = true;
                }
                runs.last_mut().unwrap().push(entry);
            }
            None => open = false,
        }
    }
    runs
}

/// The run of `java` that starts with the constant `first`.
fn run_starting_with(java: &str, first: &str) -> Vec<(String, i64)> {
    int_runs(java)
        .into_iter()
        .find(|run| run.first().is_some_and(|(name, _)| name == first))
        .unwrap_or_else(|| panic!("no table of constants starts with {first}"))
}

/// One table: the Java constant of each row, its number, the Rust variant it maps to (`None`: the function's default arm).
type Table<'a> = &'a [(&'a str, i64, &'a str)];

/// The Java run must be exactly `table` (names, numbers, order); each row must be mapped by the Rust `function` of `rust` as
/// `N=>Enum::Variant`, and a row whose variant is the `fallback` is the function's default arm.
fn pin(java: &str, first: &str, table: Table, rust: &str, function: &str, enum_name: &str, fallback: &str) {
    let run = run_starting_with(java, first);
    let wanted: Vec<(String, i64)> = table.iter().map(|(name, n, _)| ((*name).to_string(), *n)).collect();
    assert_eq!(run, wanted, "the Java table that starts with {first} is not the one this test pins");
    let mapping = body(rust, function);
    for (name, number, variant) in table {
        if *variant == fallback {
            assert!(
                mapping.contains(&format!("_=>{enum_name}::{fallback}")),
                "{function}: the default arm is {enum_name}::{fallback} ({name} = {number})"
            );
            assert!(
                !mapping.contains(&format!("{number}=>")),
                "{function}: {name} = {number} is the default arm, it has no arm of its own"
            );
        } else {
            // a Retrieved carries its secret: its arm is a match on the secret the helper sent along
            let arm = if *variant == "Retrieved" {
                format!("{number}=>matchsecret{{Some(secret)=>{enum_name}::{variant}(")
            } else {
                format!("{number}=>{enum_name}::{variant}")
            };
            assert!(mapping.contains(&arm), "{function}: {arm} ({name})");
        }
    }
}

/// `AzulKeyring.java` answers a store, a read, a delete and an unlock with one of these numbers (`nativeOnKeyringResult`, and the
/// first element of `quiet`'s array); `keyring/android.rs::result_of` makes the model's `KeyringResult` of it. The model has more
/// results than the old backend (`LockedOut`, `NotEnrolled`, `Invalidated`, `WouldPrompt`): each has its number on both sides.
///
/// Mutation: two numbers swapped on the Rust side (a lockout answered as a denial).
#[test]
fn the_keyring_results_are_the_same_numbers_in_java_and_rust() {
    let java = java_code("scripts/android/AzulKeyring.java");
    let rust = rust_code("dll/src/desktop/extra/keyring/android.rs");
    pin(
        &java,
        "STORED",
        &[
            ("STORED", 0, "Stored"),
            ("DELETED", 1, "Deleted"),
            ("RETRIEVED", 2, "Retrieved"),
            ("NOT_FOUND", 3, "NotFound"),
            ("DENIED", 4, "Denied"),
            ("UNAVAILABLE", 5, "Unavailable"),
            ("ERROR", 6, "Error"),
            ("CANCELLED", 7, "Cancelled"),
            ("FAILED", 8, "Failed"),
            ("LOCKED_OUT", 9, "LockedOut"),
            ("NOT_ENROLLED", 10, "NotEnrolled"),
            ("INVALIDATED", 11, "Invalidated"),
            ("WOULD_PROMPT", 12, "WouldPrompt"),
        ],
        &rust,
        "pub(super)fnresult_of(code:i32,secret:Option<String>)->KeyringResult{",
        "KeyringResult",
        "Error",
    );
    // a Retrieved carries its secret, and one without it is an answer nobody can use
    let result_of = body(&rust, "pub(super)fnresult_of(code:i32,secret:Option<String>)->KeyringResult{");
    assert!(result_of.contains("2=>matchsecret{Some(secret)=>KeyringResult::Retrieved(secret.into()),None=>KeyringResult::Error,}"));
}

/// `needsPrompt` says what a read of an item would need; `need_of` makes the model's `KeyringNeed` of it.
///
/// Mutation: `UserPresence` and `Biometric` swapped on one side (a person asked for a finger where a passcode would do).
#[test]
fn the_keyring_needs_are_the_same_numbers_in_java_and_rust() {
    let java = java_code("scripts/android/AzulKeyring.java");
    let rust = rust_code("dll/src/desktop/extra/keyring/android.rs");
    pin(
        &java,
        "NEED_NOT_FOUND",
        &[
            ("NEED_NOT_FOUND", 0, "NotFound"),
            ("NEED_NOTHING", 1, "Nothing"),
            ("NEED_UNLOCK", 2, "Unlock"),
            ("NEED_USER_PRESENCE", 3, "UserPresence"),
            ("NEED_BIOMETRIC", 4, "Biometric"),
            ("NEED_INVALIDATED", 5, "Invalidated"),
            ("NEED_UNAVAILABLE", 6, "Unavailable"),
            ("NEED_ERROR", 7, "Error"),
        ],
        &rust,
        "pub(super)fnneed_of(code:i32)->KeyringNeed{",
        "KeyringNeed",
        "Error",
    );
}

/// `support` says whether the secure hardware guards the keys (`Hardware`), the library does (`Library`), the device cannot meet the
/// level for want of an enrolled biometric (`NotEnrolled`), or nothing can (`Unavailable`).
#[test]
fn the_keyring_support_levels_are_the_same_numbers_in_java_and_rust() {
    let java = java_code("scripts/android/AzulKeyring.java");
    let rust = rust_code("dll/src/desktop/extra/keyring/android.rs");
    pin(
        &java,
        "SUPPORT_HARDWARE",
        &[
            ("SUPPORT_HARDWARE", 0, "Hardware"),
            ("SUPPORT_LIBRARY", 1, "Library"),
            ("SUPPORT_NOT_ENROLLED", 2, "NotEnrolled"),
            ("SUPPORT_UNAVAILABLE", 3, "Unavailable"),
        ],
        &rust,
        "pub(super)fnsupport_of(code:i32)->KeyringSupport{",
        "KeyringSupport",
        "Unavailable",
    );
}

/// The protection level goes to Java as `KeyringProtection::code()` (0, 1, 2, the number the model keeps in an item's record too);
/// the Java helper names the same three.
#[test]
fn the_protection_levels_are_the_same_numbers_in_java_and_the_model() {
    let java = java_code("scripts/android/AzulKeyring.java");
    let run = run_starting_with(&java, "LEVEL_DEVICE_UNLOCKED");
    assert_eq!(
        run,
        vec![
            ("LEVEL_DEVICE_UNLOCKED".to_string(), 0),
            ("LEVEL_USER_PRESENCE".to_string(), 1),
            ("LEVEL_BIOMETRIC".to_string(), 2),
        ]
    );
    let core = rust_code("core/src/keyring.rs");
    let code = body(&core, "pubconstfncode(self)->u8{");
    assert!(code.contains("Self::DeviceUnlocked=>0,Self::UserPresence=>1,Self::Biometric=>2,"), "the model's numbers: {code}");
    let rust = rust_code("dll/src/desktop/extra/keyring/android.rs");
    assert!(
        body(&rust, "pub(super)constfnlevel_of(protection:KeyringProtection)->i32{").contains("protection.code()asi32"),
        "the level sent to Java is the model's number"
    );
}

/// `AzulBiometric` answers a prompt with one of these numbers (`nativeOnBiometricResult`); `biometric/android.rs::map_result` makes the
/// model's `BiometricResult` of it. `LockedOut` and `NotEnrolled` are results of their own since the model; a lockout is no failed match.
///
/// Mutation: `LockedOut` mapped to `Failed` (the person keeps trying a sensor that is locked for 30 seconds).
#[test]
fn the_biometric_results_are_the_same_numbers_in_java_and_rust() {
    let java = java_code("scripts/android/AzulBiometric.java");
    let rust = rust_code("dll/src/desktop/extra/biometric/android.rs");
    pin(
        &java,
        "RESULT_AUTHENTICATED",
        &[
            ("RESULT_AUTHENTICATED", 0, "Authenticated"),
            ("RESULT_FAILED", 1, "Failed"),
            ("RESULT_CANCELLED", 2, "Cancelled"),
            ("RESULT_FALLBACK", 3, "FellBackToPasscode"),
            ("RESULT_UNAVAILABLE", 4, "Unavailable"),
            ("RESULT_ERROR", 5, "Error"),
            ("RESULT_LOCKED_OUT", 6, "LockedOut"),
            ("RESULT_NOT_ENROLLED", 7, "NotEnrolled"),
        ],
        &rust,
        "fnmap_result(code:i32)->BiometricResult{",
        "BiometricResult",
        "Error",
    );
}

/// `canAuthenticate` says which sensor the phone has (0 none, 1 fingerprint, 2 face, 3 iris); `map_kind` makes the model's
/// `BiometricKind` of it.
#[test]
fn the_biometric_kinds_are_the_same_numbers_in_java_and_rust() {
    let java = java_code("scripts/android/AzulBiometric.java");
    let can = body(&java, "public static int canAuthenticate(Activity activity)");
    for number in 0..=3 {
        assert!(can.contains(&format!("return {number};")), "canAuthenticate can answer {number}");
    }
    let rust = rust_code("dll/src/desktop/extra/biometric/android.rs");
    let kind = body(&rust, "fnmap_kind(code:i32)->BiometricKind{");
    for (number, variant) in [(1, "Fingerprint"), (2, "Face"), (3, "Iris")] {
        assert!(kind.contains(&format!("{number}=>BiometricKind::{variant}")), "{number} is {variant}");
    }
    assert!(kind.contains("_=>BiometricKind::NotAvailable"), "0 and anything else: no sensor");
}
