//! What the phones' file dialogs may and may not answer, pinned in source (the Android and iOS code cannot run where the
//! tests run). Ledger WP-F, rows FP-1 .. FP-6 of 2026-11-28.
//!
//! A call must never say `true` (or hand back a path) when the user's file is not where the user chose:
//!
//! * `FileDialog::save_bytes` on a phone wrote the app's PRIVATE folder and answered `true`: every Export button of every
//!   app said "saved" and the user could not find the file;
//! * Android's save request created an EMPTY document in the user's folder and handed the app a cache copy of it (the
//!   data never reached the document); its folder request opened a tree URI as a file and answered "cancelled" for every
//!   pick; iOS's save request answered "cancelled" to every request;
//! * an error and a cancel were the same answer (`path: None`).
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/file_picker_tells_the_truth_contract.rs -o /tmp/t && /tmp/t

use std::{fs, path::Path};

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Rust source without its `//` comments (a comment may name what the code must not do).
fn code(rel: &str) -> String {
    read(rel)
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The brace-balanced body after the first `signature`.
fn body<'a>(src: &'a str, signature: &str) -> &'a str {
    let start = src.find(signature).unwrap_or_else(|| panic!("no `{signature}`"));
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

/// The part of `body` that is compiled only on a phone: from the `#[cfg(any(target_os = "android", target_os = "ios"))]`
/// block to its end.
fn mobile_arm(fn_body: &str) -> &str {
    let marker = "#[cfg(any(target_os = \"android\", target_os = \"ios\"))]";
    let at = fn_body.find(marker).unwrap_or_else(|| panic!("no mobile arm in:\n{fn_body}"));
    &fn_body[at..]
}

/// `FileDialog::save_bytes` on a phone writes nothing and answers `false`.
///
/// Mutation: the old arm (`FilePath::get_document_dir()` + `file_write(...).is_ok()`) put back.
#[test]
fn save_bytes_on_a_phone_never_writes_a_folder_the_user_did_not_choose() {
    let src = code("layout/src/desktop/dialogs.rs");
    let save_bytes = body(&src, "pub fn save_bytes(");
    let mobile = mobile_arm(save_bytes);
    assert!(
        !mobile.contains("file_write") && !mobile.contains("get_document_dir") && !mobile.contains("get_data_dir"),
        "the mobile arm of save_bytes writes a file the user did not choose:\n{mobile}"
    );
    assert!(mobile.contains("false"), "the mobile arm answers false:\n{mobile}");
    assert!(
        !mobile.contains("true"),
        "the mobile arm must not answer true without a file the user chose:\n{mobile}"
    );
}

/// The two phone dispatchers that cannot be served answer with an ERROR that carries the reason, and show no dialog.
///
/// Mutation: `android::dispatch_save_file(...)` / `android::dispatch_open_directory(...)` / `ios::dispatch_save_file(...)`
/// back in `mod.rs` (the state of 96d83ae2e0).
#[test]
fn a_request_the_phone_cannot_serve_fails_with_its_reason_and_shows_no_dialog() {
    let src = code("dll/src/desktop/extra/file_picker/mod.rs");
    let save = body(&src, "pub fn apply_save_file(");
    assert!(
        save.contains("fail(&handle, ANDROID_SAVE_UNAVAILABLE)"),
        "Android's save answers an error with its reason:\n{save}"
    );
    assert!(
        save.contains("fail(&handle, IOS_SAVE_UNAVAILABLE)"),
        "iOS's save answers an error with its reason:\n{save}"
    );
    // and `fail` is an Error status (with the reason), not a cancel
    let fail = body(&src, "fn fail(");
    assert!(
        fail.contains("FilePickerStatus::Error(") && !fail.contains("Cancelled"),
        "fail answers an error:\n{fail}"
    );
    assert!(!save.contains("dispatch_save_file"), "no save dialog is dispatched:\n{save}");

    let dir = body(&src, "pub fn apply_open_directory(");
    assert!(
        dir.contains("fail(&handle, ANDROID_FOLDER_UNAVAILABLE)"),
        "Android's folder request answers an error with its reason:\n{dir}"
    );
    assert!(
        !dir.contains("android::dispatch_open_directory"),
        "Android's folder dialog is not shown while its answer is thrown away:\n{dir}"
    );
    // iOS's folder picker works (security-scoped, for the life of the process): it stays dispatched
    assert!(dir.contains("ios::dispatch_open_directory"), "iOS's folder picker stays:\n{dir}");

    let android = code("dll/src/desktop/extra/file_picker/android.rs");
    assert!(
        !android.contains("saveDocument") && !android.contains("pickDirectory"),
        "no Rust call of the Java save / folder pickers whose answer is a cache copy of an empty document / a tree URI that cannot be opened"
    );
    let ios = code("dll/src/desktop/extra/file_picker/ios.rs");
    assert!(
        !ios.contains("pub fn dispatch_save_file"),
        "iOS's save dispatcher answered Cancelled to every request"
    );
}

/// The engine's result of a failed dialog is not the result of a cancelled one.
///
/// Mutation: `error: OptionString` removed from the three result structs (an error was `path: None`, a cancel).
#[test]
fn a_failed_dialog_is_not_a_cancelled_one() {
    let src = code("layout/src/desktop/dialogs.rs");
    for (name, field) in [
        ("pub struct FileOpenResult", "pub path: OptionFilePath"),
        ("pub struct FileOpenMultiResult", "pub paths: FilePathVec"),
        ("pub struct SaveTargetResult", "pub target: OptionSaveTarget"),
    ] {
        let b = body(&src, name);
        assert!(b.contains(field), "{name}: {field}");
        assert!(b.contains("pub error: OptionString"), "{name} carries the reason of a failure:\n{b}");
    }
    // and every mobile status maps an Error to a failure, never to the cancelled answer
    for f in ["open_result_from_status", "multi_result_from_status", "save_result_from_status"] {
        let b = body(&src, &format!("fn {f}("));
        assert!(
            b.contains("FilePickerStatus::Error(reason)") && b.contains("failed(reason)"),
            "{f} keeps the reason of an error:\n{b}"
        );
        assert!(
            !b.contains("Cancelled | FilePickerStatus::Error"),
            "{f} merges an error into a cancel:\n{b}"
        );
    }
}
