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

#[test]
fn swift_takes_and_gives_optionals_throws_errors_and_lets_a_callback_throw() {
    let path = "azul.swift";
    let text = generated(path);
    assert_contains(
        path,
        &text,
        &[
            // `nil` in, `T?` out, an Err is a thrown error.
            "public static func openDirectory<T: AnyObject>(_ title: String, defaultPath: String?, \
             data: T, onResult: @escaping (T, CallbackInfo, RefAny) throws -> Update) -> RequestId {",
            "public static func downcast(_ result: AnyObject) -> FileOpenResult? {",
            "public var path: FilePath? {",
            "public static func parse(_ s: String) throws -> Url {",
            // A callback may `try`: the error is logged and the callback does nothing.
            "        let __ur = try onResult(__u0, __u1, __u2)",
            "AzCallbackInfo_log(&__log, AzAppLogLevel(rawValue: 1), _Native.azString(\"azul: a callback threw \\(error)\"))",
        ],
    );
}

#[test]
fn d_takes_and_gives_nullables_throws_errors_and_downcasts_the_engines_refany() {
    let path = "azul.d";
    let text = generated(path);
    assert_contains(
        path,
        &text,
        &[
            "static RequestId openDirectory(T)(string title, Nullable!(string) defaultPath, T data, \
             Update function(T, CallbackInfo, RefAny) onResult) if (is(T == class))",
            "Nullable!(FilePath) path() const",
            "void path(Nullable!(FilePath) v)",
            "Throws: `ResultException!(UrlParseError)` when the Rust `Result` is `Err`.",
            // The resume callback's `RefAny result` goes straight into `downcast`.
            "static Nullable!(FileOpenResult) downcast(Object result)",
            "alias _asObject this;",
            "if (auto shared_ = cast(_AzulSharedRefAny) object)",
        ],
    );
}

#[test]
fn crystal_takes_and_gives_nil_and_raises_errors() {
    let path = "azul.cr";
    let text = generated(path);
    assert_contains(
        path,
        &text,
        &[
            "def self.open_directory(title : ::String, default_path : (::String | ::Nil), data : T, \
             &on_result : T, Azul::CallbackInfo, Azul::RefAny -> Azul::Update) : Azul::RequestId forall T",
            "def self.downcast(result : ::Reference) : (Azul::FileOpenResult | ::Nil)",
            "def path : (Azul::FilePath | ::Nil)",
            "def path=(value : (Azul::FilePath | ::Nil)) : ::Nil",
            "raise Azul::ResultError(Azul::UrlParseError).new(Azul::UrlParseError.__own(__e))",
        ],
    );
}

#[test]
fn csharp_takes_and_gives_nullables_and_registers_a_typed_resume_callback() {
    let path = "Azul.cs";
    let text = generated(path);
    assert_contains(
        path,
        &text,
        &[
            // `null` in, a nullable out, both on parameters and fields.
            "public static AzRequestId OpenDirectory(string title, string? default_path, RefAny data, \
             ResumeCallback on_result)",
            "var __default_path_opt = __AzOpt.InOptionString(default_path);",
            "public global::Azul.FilePath? Path",
            "public string? Light",
            "public static FileOpenResult? Downcast(RefAny result)",
            // The engine's `result` is deep-copied into the downcast, never freed twice.
            "var __ret = NativeMethods.AzFileOpenResult_downcast(result.__Take());",
            // The resume callback is a typed delegate like every `On<Event>`.
            "public static AzRequestId OpenDirectory<T>(string title, string? default_path, T data, \
             HostInvoker.ResumeCallbackWithData<T> on_result) where T : class",
            "public static Url Parse(string s)",
        ],
    );
}

/// Java (and Scala, which compiles against the same classes).
#[test]
fn java_takes_the_value_or_null_and_gives_optionals_and_types_the_resume_callback() {
    let file = |name: &str| (format!("java/{name}.java"), generated(&format!("java/{name}.java")));
    let (path, dialog) = file("FileDialog");
    assert_contains(
        &path,
        &dialog,
        &[
            // A static-only type has a class for its functions.
            "public static RequestId openDirectory(java.lang.String title, java.lang.String default_path, \
             RefAny data, ResumeCallback on_result) {",
            "AzOptionString.ByValue __default_path_opt = AzulOptions.inOptionString(default_path);",
            "public static <T> RequestId openDirectory(java.lang.String title, java.lang.String default_path, \
             T data, AzulHostInvoker.ResumeCallbackWithData<T> on_result) {",
        ],
    );
    let (path, picked) = file("FileOpenResult");
    assert_contains(
        &path,
        &picked,
        &[
            "public static java.util.Optional<FileOpenResult> downcast(RefAny result) {",
            // The engine's `result` is deep-copied into the downcast, never freed twice.
            "AzRefAny.ByValue __result_raw = result.__takeRaw();",
            "public java.util.Optional<FilePath> getPath() {",
            "public void setPath(FilePath v) {",
        ],
    );
    let (path, icons) = file("IconVariants");
    assert_contains(
        &path,
        &icons,
        &[
            "public java.util.Optional<java.lang.String> getLight() {",
            "public void setLight(java.lang.String v) {",
        ],
    );
}
