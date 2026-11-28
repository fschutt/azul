//! What an export on a phone may say, pinned in source (the Android and iOS code cannot run where the tests run; the emulator proof
//! is `scripts/ci/android_files_proof.py`). Ledger WP-F, step 3a, FP-5.
//!
//! `FileDialog::export_bytes` answers a NAME only after the bytes were written where the person chose:
//!
//! * the engine's mobile arm answers from the picker's status and writes nothing itself (the old `save_bytes` wrote the app's private
//!   folder and said `true`);
//! * Android: the system's "save as" creates the (empty) document, the bytes are written through it by a WORKER, the answer is the
//!   name after the write, and a failed write deletes the empty document again;
//! * iOS: the export picker copies a file of the bytes (`asCopy:YES`) and the answer is the name it was saved under; the file of the
//!   bytes is removed whatever the person did.
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/file_picker_export_contract.rs -o /tmp/t && /tmp/t

use std::{fs, path::Path};

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

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

/// The engine's phone arm answers from the picker and writes nothing.
///
/// Mutation: the old arm of `save_bytes` (`file_write(...)` into the app's folder) in the phone arm of `export_bytes`.
#[test]
fn the_engines_phone_arm_answers_from_the_picker_and_writes_nothing() {
    let src = code("layout/src/desktop/dialogs.rs");
    let export = body(&src, "pub fn export_bytes(");
    let at = export.find("#[cfg(any(target_os = \"android\", target_os = \"ios\"))]").expect("a phone arm");
    let phone = &export[at..];
    assert!(phone.contains("(backend.export_bytes)(suggested_name, mime, bytes)"), "the backend gets the bytes:\n{phone}");
    assert!(phone.contains("export_result_from_status(handle.poll())"), "the answer is the picker's:\n{phone}");
    assert!(
        !phone.contains("file_write") && !phone.contains("get_document_dir") && !phone.contains("get_data_dir") && !phone.contains("fs::write"),
        "the phone arm writes no file of its own:\n{phone}"
    );
    // a status of the picker that is not a pick is never a save
    let map = body(&src, "pub(crate) fn export_result_from_status(");
    assert!(map.contains("FilePickerStatus::Error(reason) => ExportResult::failed(reason)"));
    assert!(map.contains("FilePickerStatus::Cancelled => ExportResult::new(OptionString::None)"));
    assert!(map.contains("FilePickerStatus::Selected(name) => ExportResult::new(OptionString::Some(name))"));
}

/// Android: a worker writes, the name comes after the write, a failed write leaves no empty document.
///
/// Mutation: `Selected(entry.name)` answered before the write (a name for a file that is empty), or no `delete` after a failed write.
#[test]
fn android_writes_through_the_created_document_on_a_worker_and_answers_after_the_write() {
    let src = code("dll/src/desktop/extra/file_picker/android.rs");
    let intent = body(&src, "fn export_intent<'a>(");
    assert!(intent.contains("ACTION_CREATE_DOCUMENT") && intent.contains("EXTRA_TITLE") && intent.contains("addCategory"));
    let finish = body(&src, "fn finish_export(");
    assert!(finish.contains("std::thread::Builder::new()") && finish.contains(".spawn("), "the write is on a worker:\n{finish}");
    let write = finish.find("documents.write(").expect("the write goes through the documents backend");
    let named = finish.find("FilePickerStatus::Selected(entry.name)").expect("the answer is the name");
    assert!(write < named, "the name is answered after the write:\n{finish}");
    let failed = finish.find("Err(why) =>").expect("a failed write");
    let delete = finish.find("documents.delete(").expect("the empty document is deleted again");
    assert!(failed < delete && delete < finish.find("the file could not be saved").unwrap(), "delete, then answer:\n{finish}");
    // the native hands an export to `finish_export` and answers nothing before it
    let native = body(&src, "pub unsafe extern \"system\" fn Java_com_azul_picker_AzulFilePicker_nativeOnActivityResult(");
    assert!(native.contains("PickKind::Export") && native.contains("finish_export("));
}

/// iOS: the export picker copies a file of the bytes; the file is removed whatever the person did.
///
/// Mutation: the scratch folder not removed in the cancel path (a copy of the user's data stays in the app's tmp).
#[test]
fn ios_exports_through_the_picker_with_a_copy_and_cleans_its_file() {
    let src = code("dll/src/desktop/extra/file_picker/ios.rs");
    let export = body(&src, "pub fn dispatch_export(handle: FilePickerHandle, suggested_name: AzString, _mime: AzString, bytes: azul_css::U8Vec) {\n    use");
    assert!(export.contains("initForExportingURLs") && export.contains("asCopy: true"), "{export}");
    assert!(export.contains("sanitize_display_name("), "the name of the file is made safe first");
    // every way the picker ends removes the file: a pick (did_pick), a cancel (pop_handle cleans), a failed presentation
    let pick = body(&src, "extern \"C\" fn document_picker_did_pick(");
    assert!(pick.contains("clean_scratch(pending.scratch.as_deref())"), "a pick removes the file:\n{pick}");
    let pop = body(&src, "fn pop_handle(");
    assert!(pop.contains("clean_scratch("), "a cancel (and a failed presentation) removes the file:\n{pop}");
}

/// The backend is registered with the export entry, and a build with no backend answers an error, never a save.
#[test]
fn the_export_is_registered_and_a_build_without_it_says_so() {
    let module = code("dll/src/desktop/extra/file_picker/mod.rs");
    assert!(module.contains("export_bytes: apply_export_bytes"));
    let apply = body(&module, "pub fn apply_export_bytes(");
    assert!(apply.contains("android::dispatch_export(") && apply.contains("ios::dispatch_export("));
    let dialogs = code("layout/src/desktop/dialogs.rs");
    let export = body(&dialogs, "pub fn export_bytes(");
    assert!(export.contains("ExportResult::failed("), "no backend is an error, not a save:\n{export}");
}
