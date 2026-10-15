//! Every shipped binding speaks its own optional and result type.
//!
//! The C ABI carries an optional value as a tagged union (`AzOptionString`:
//! a tag byte, then the payload) and a fallible one as `AzResult*`. An app
//! should never spell those out: a parameter takes the language's own
//! optional (`None`, `null`, `nil`, `Nothing`), a return value or a field
//! gives one, and a failure arrives the way the language reports failures
//! (an exception, `Either`, an error union). The wrapper code converts at the
//! boundary, so the C ABI and api.json stay as they are.
//!
//! These tests read what `azul-doc codegen all` wrote to `target/codegen`, one
//! test per binding, each pinned to the same three API items: the file
//! dialog's optional start folder (`FileDialog::open_directory`'s
//! `default_path`), the picked folder (`FileOpenResult::path`, reached through
//! the `OptionFileOpenResult` its `downcast` returns) and a parse that can
//! fail (`Url::parse`). The per-language smoke programs in
//! `tests/native_optionals/` compile and run the same items against libazul.

use super::bug_classes::{generated, ir};

/// The count of Option mirrors in the API, so a test can check that a
/// conversion reaches every one of them and not only the hello-world's.
fn option_count() -> usize {
    ir().enums.iter().filter(|e| ir().option_payload(&e.name).is_some()).count()
}

fn result_count() -> usize {
    ir().enums.iter().filter(|e| ir().result_payloads(&e.name).is_some()).count()
}

fn assert_contains(path: &str, text: &str, wanted: &[&str]) {
    let missing: Vec<&&str> = wanted.iter().filter(|w| !text.contains(**w)).collect();
    assert!(missing.is_empty(), "{path} lacks:\n{missing:#?}");
}

#[test]
fn rust_options_and_results_convert_to_std_in_both_directions() {
    let path = "dll_api_external.rs";
    let text = generated(path);
    assert_contains(
        path,
        &text,
        &[
            "impl From<AzOptionFilePath> for Option<AzFilePath> {",
            "impl From<Option<AzFilePath>> for AzOptionFilePath {",
            "impl From<AzFilePath> for AzOptionFilePath {",
            "impl From<AzResultUrlUrlParseError> for Result<AzUrl, AzUrlParseError> {",
            "impl From<Result<AzUrl, AzUrlParseError>> for AzResultUrlUrlParseError {",
        ],
    );
    let to_std = text.matches("> for Option<").count();
    assert_eq!(to_std, option_count(), "every Option mirror converts into a std Option");
    let to_std = text
        .lines()
        .filter(|l| l.starts_with("impl From<Az") && l.contains("> for Result<"))
        .count();
    assert_eq!(to_std, result_count(), "every Result mirror converts into a std Result");
}

#[test]
fn a_rust_callback_uses_question_mark_on_stable_and_nightly() {
    let path = "dll_api_external.rs";
    let text = generated(path);
    assert_contains(
        path,
        &text,
        &[
            // Stable: the callback body runs in a closure that answers Option / Result.
            "pub fn with<F: FnOnce() -> Option<AzUpdate>>(f: F) -> AzUpdate {",
            "f().unwrap_or(AzUpdate::DoNothing)",
            "pub fn with_result<E: core::fmt::Debug, F: FnOnce() -> Result<AzUpdate, E>>(f: F) -> AzUpdate {",
            // Nightly (`try-trait`): `?` directly in the callback and on the FFI types.
            "impl core::ops::FromResidual<Option<core::convert::Infallible>> for AzUpdate {",
            "impl<E: core::fmt::Debug> core::ops::FromResidual<Result<core::convert::Infallible, E>> for AzUpdate {",
            "impl core::ops::Try for AzOptionFileOpenResult {",
            "impl core::ops::Try for AzResultUrlUrlParseError {",
        ],
    );
    assert_eq!(
        text.matches("impl core::ops::Try for AzOption").count(),
        option_count(),
        "every Option mirror is `?`-able under try-trait"
    );
}
