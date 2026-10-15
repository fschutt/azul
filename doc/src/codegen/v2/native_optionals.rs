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

#[test]
fn kotlin_takes_and_gives_nullables_and_types_the_resume_callback() {
    let path = "kotlin/Azul.kt";
    let text = generated(path);
    assert_contains(
        path,
        &text,
        &[
            // A static-only type has a class, with Kotlin types and `null` for None.
            "class FileDialog internal constructor(internal val ptr: Pointer) {",
            "@JvmStatic fun openDirectory(title: kotlin.String, default_path: kotlin.String?, \
             data: RefAny, on_result: ResumeCallback): RequestId {",
            "val __default_path_opt = AzulOptions.inOptionString(default_path)",
            "@JvmStatic fun <T : Any> openDirectory(title: kotlin.String, default_path: kotlin.String?, \
             data: T, on_result: AzulHostInvoker.ResumeCallbackWithData<T>): RequestId {",
            "@JvmStatic fun downcast(result: RefAny): FileOpenResult? {",
            // The engine's `result` is deep-copied into the downcast, never freed twice.
            "val __result_raw = result.__takeRaw()",
            "var path: FilePath?",
            "var light: kotlin.String?",
        ],
    );
}

/// Haskell: `Maybe` / `Either` at the surface, and `MaybeT` / `ExceptT`
/// guards that turn a failure into `(model, Update_DoNothing)`.
#[test]
fn haskell_takes_and_gives_maybe_and_either_and_guards_a_handler() {
    let api = "haskell/src/Azul/Internal/Api/Dialog.hs";
    let dialog = generated(api);
    assert_contains(
        api,
        &dialog,
        &[
            "fileDialogOpenDirectory :: (ResumeCallbackHandler h3) => String -> (Maybe String) -> RefAny -> h3 -> IO RequestId",
            "withMaybeArg T.OptionString_None T.OptionString_Some withAzStringArg a1 $ \\__p1 -> do",
            // The data bound to the model of the running callback.
            "fileDialogOpenDirectoryWithModel :: (ResumeCallbackHandler h3) => String -> (Maybe String) -> h3 -> IO RequestId",
            "fileOpenResultDowncast :: (ToRefAny d0) => d0 -> IO (Maybe FileOpenResult)",
            "fileOpenResultGetPath :: FileOpenResult -> IO (Maybe AzFilePath)",
            "fileOpenResultSetPath :: (Maybe AzFilePath) -> FileOpenResult -> IO ()",
        ],
    );
    let api = "haskell/src/Azul/Internal/Api/Url.hs";
    let url = generated(api);
    assert_contains(api, &url, &["urlParse :: String -> IO (Either UrlParseError Url)"]);
    let facade = "haskell/src/Azul.hs";
    let text = generated(facade);
    assert_contains(
        facade,
        &text,
        &[
            "orDoNothing :: model -> MaybeT IO (model, T.Update) -> IO (model, T.Update)",
            "orDoNothingOnError :: Show e => model -> ExceptT e IO (model, T.Update) -> IO (model, T.Update)",
        ],
    );
}

#[test]
fn zig_gives_optionals_and_error_unions() {
    let path = "azul.zig";
    let text = generated(path);
    assert_contains(
        path,
        &text,
        &[
            // Parameters already take `null`, `?T` or the value (`_asAzOption`).
            "pub fn openDirectory(title: anytype, default_path: anytype, data: anytype, on_result: anytype) RequestId {",
            "pub fn downcast(result: anytype) ?FileOpenResult {",
            "pub fn getPath(self: *const Self) ?FilePath {",
            "pub fn getLight(self: *const Self) ?String {",
            "pub fn parse(s: anytype) error{UrlParseError}!Url {",
            "pub inline fn _optPayload(o: anytype, some_tag: anytype) ?@TypeOf(o.Some.payload) {",
        ],
    );
}

#[test]
fn go_takes_nil_and_gives_ok_pairs_and_errors() {
    let path = "go/wrappers.go";
    let text = generated(path);
    assert_contains(
        path,
        &text,
        &[
            // nil is None; the resume callback is a Go function (`Bind3(onPick)`).
            "func FileDialogOpenDirectory(title *String, default_path *String, data any, on_result any) AzRequestId {",
            "azOptInOptionString(default_path)",
            "azGoResumeCallbackArg(on_result)",
            "func FileOpenResultDowncast(result any) (*FileOpenResult, bool) {",
            "func (self *FileOpenResult) Path() (*FilePath, bool) {",
            "func (self *FileOpenResult) SetPath(v *FilePath) {",
            "func UrlParse(s *String) (*Url, error) {",
            "type AzulError struct {",
        ],
    );
    let path = "go/callbacks.go";
    let text = generated(path);
    assert_contains(
        path,
        &text,
        &[
            "func Bind3[T any, Ctx any, A any, Ret any](cb func(*T, Ctx, A) Ret) func(any, Ctx, A) any {",
            "func azGoResumeCallbackArg(v any) AzResumeCallback {",
            // A resume callback's result reaches the callback as the *RefAny.
            "var refAny any = refAny_wrap",
        ],
    );
}

#[test]
fn node_takes_and_gives_null_and_passes_the_resume_result_to_downcast() {
    let path = "node/azul.js";
    let text = generated(path);
    assert_contains(
        path,
        &text,
        &[
            "_optIn(OptionString, 'AzOptionString', defaultPath, _azString)",
            "get path() { return _optTake(lib.AzOptionFilePath_clone(this._ptr.path), lib.AzOptionFilePath_delete,",
            "set light(v) { const nv = _optIn(OptionString, 'AzOptionString', v, _azString);",
            // The resume callback's result reaches `downcast` as a RefAny.
            "const ret = fn(_refanyFromPtr(arg0), arg1, _refanyArg(arg2));",
            "function _refanyArg(ptr) {",
        ],
    );
}

#[test]
fn ruby_takes_and_gives_nil_and_passes_the_resume_result_to_downcast() {
    let path = "azul.rb";
    let text = generated(path);
    assert_contains(
        path,
        &text,
        &[
            "Azul._opt_in(Native::AzOptionString, :az_option_string_none, :az_option_string_some, \
             default_path, ->(v) { Azul._az_string(v) })",
            "Native.az_option_string_clone(Azul._fields(self)[:light].to_ptr).to_opt",
            "def self._opt_in(opt_class, none_fn, some_fn, value, map)",
            // The resume callback's result reaches `downcast` as a RefAny.
            "unwrapped_args << (Azul::RefAny.unwrap(ptr_args[2]) || \
             Azul::RefAny.new(Native.az_ref_any_clone(ptr_args[2])))",
        ],
    );
}

#[test]
fn lua_takes_and_gives_nil() {
    let path = "azul.lua";
    let text = generated(path);
    assert_contains(
        path,
        &text,
        &[
            "default_path = azul._opt_in('AzOptionString', default_path, azul._az_string)",
            "function IconVariants_methods:get_light() return (C.AzOptionString_clone(self.light)):to_opt() end",
            "function azul._opt_in(ct, v, map)",
            // A static function returning an Option gives nil or the value.
            "        local _ret = C.AzFileOpenResult_downcast(result)\n        return _ret:to_opt()\n",
        ],
    );
}

#[test]
fn ocaml_takes_and_gives_options_and_results() {
    let path = "ocaml/azul_api_dialog.mli";
    let text = generated(path);
    assert_contains(
        path,
        &text,
        &[
            "val open_directory : string -> string option -> ref_any -> ",
            "val downcast : ref_any -> file_open_result option",
            "val get_path : t -> file_path option",
            "val set_path : t -> file_path option -> unit",
        ],
    );
    let path = "ocaml/azul_api_url.mli";
    let text = generated(path);
    assert_contains(path, &text, &["val parse : string -> (url, "]);
    let path = "ocaml/azul_enums_callbacks.ml";
    let text = generated(path);
    assert_contains(
        path,
        &text,
        &["let or_do_nothing (f : unit -> t option) : t = match f () with Some u -> u | None -> DoNothing"],
    );
}

#[test]
fn cpp_option_parameters_take_nullopt_an_optional_or_the_value() {
    for path in ["azul17.hpp", "azul20.hpp", "azul23.hpp"] {
        let text = generated(path);
        assert_contains(
            path,
            &text,
            &[
                "    OptionString(std::nullopt_t) noexcept : inner_(AzOptionString_none()) {}",
                "    OptionString(std::optional<String> v) : inner_(v ? AzOptionString_some(std::move(*v).release()) : AzOptionString_none()) {}",
                "    OptionString(U&& u) : inner_(AzOptionString_some(String(std::forward<U>(u)).release())) {}",
                // The way out was already there.
                "operator std::optional<String>() &&",
            ],
        );
    }
}

/// C has no optional type: it gets a macro that binds an Option's payload
/// or ends the block, on top of the generated `matchRef` helpers.
#[test]
fn c_binds_a_payload_or_ends_the_block() {
    let path = "azul.h";
    let text = generated(path);
    assert_contains(
        path,
        &text,
        &[
            "#define AZ_LET_SOME(OptT, opt_ptr, out, otherwise) if (!OptT##_matchRefSome((opt_ptr), &(out))) { otherwise; } else ((void)0)",
            "#define AZ_LET_OK(ResT, res_ptr, out, otherwise) if (!ResT##_matchRefOk((res_ptr), &(out))) { otherwise; } else ((void)0)",
        ],
    );
}

/// Python: `None` or the value where an Option is asked for, `None` or the
/// value back, a raised `ValueError` for an `Err`. A resume callback's result
/// reaches Python as an opaque handle that `downcast` takes back; before, it
/// arrived as `None` and every `downcast` failed.
#[test]
fn python_takes_and_gives_none_raises_errors_and_downcasts_the_resume_result() {
    let path = "python_api.rs";
    let text = generated(path);
    assert_contains(
        path,
        &text,
        &[
            "fn open_directory(py: Python<'_>, title: String, default_path: Option<String>, data: Py<PyAny>, on_result: Py<PyAny>) -> AzRequestId {",
            "fn downcast(py: Python<'_>, result: Py<PyAny>) -> Option<AzFileOpenResult> {",
            "fn __get_light(&self) -> Option<String> {",
            "fn __set_light(slf: &Bound<'_, Self>, value: Option<String>) -> PyResult<()> {",
            "fn __get_path(&self) -> Option<AzFilePath> {",
            "fn parse(s: String) -> PyResult<AzUrl> {",
            "pub struct PyOpaqueRefAny {",
            "None => Py::new(py, PyOpaqueRefAny { refany }).ok().map(|o| o.into_any()),",
            "extract::<pyo3::PyRef<'_, PyOpaqueRefAny>>()",
        ],
    );
}

/// Pascal has no optional type of its own (FPC's `TNullable` is not in
/// every compiler the unit supports): an Option or a Result is read with an
/// overloaded `AzTryGet*` that copies the payload out or answers False.
#[test]
fn pascal_reads_options_and_results_with_try_get() {
    let path = "azul.pas";
    let text = generated(path);
    assert_contains(
        path,
        &text,
        &[
            "function AzTryGetSome(const O: TAzOptionFilePath; out V: TAzFilePath): Boolean; overload; inline;",
            "function AzTryGetOk(const R: TAzResultUrlUrlParseError; out V: TAzUrl): Boolean; overload; inline;",
            "function AzTryGetErr(const R: TAzResultUrlUrlParseError; out E: TAzUrlParseError): Boolean; overload; inline;",
            "  Result := O.Tag = 1;\n  if Result then V := O.Payload_Some;\n",
        ],
    );
}

/// Fortran declares a tagged union as an opaque blob, so before this an
/// Option's payload could not be read at all. Optional values have no
/// Fortran type; a generic `azul_try_get_some` / `_ok` / `_err` copies the
/// payload out of the blob or answers `.false.`.
#[test]
fn fortran_reads_options_and_results_with_try_get() {
    let path = "fortran/azul_api.f90";
    let text = generated(path);
    assert_contains(
        path,
        &text,
        &[
            "interface azul_try_get_some",
            "interface azul_try_get_ok",
            "interface azul_try_get_err",
            "public :: azul_try_get_some, azul_try_get_ok, azul_try_get_err",
            "type(AzOptionFilePath), intent(in), target :: o",
            "type(AzFilePath), intent(out) :: v",
            "type(AzResultUrlUrlParseError), intent(in), target :: res",
            "type(AzUrlParseError), intent(out) :: e",
        ],
    );
}
