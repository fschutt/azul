//! The answers of the phones' file dialogs, as the apps read them (`FileOpenResult`, `FileOpenMultiResult`,
//! `SaveTargetResult`): a failure carries its reason and is not a cancel; `save_bytes` says why it answers `false`.
//! The mappers are compiled on every target (they are pure), so these run on the host.

use azul_css::{corety::OptionString, AzString, StringVec};

use super::{
    export_result_from_status, export_unavailable, folder_result_from_status, multi_result_from_status,
    open_result_from_status, save_result_from_status, FilePickerStatus, SaveTargetKind,
};

fn s(text: &str) -> AzString {
    AzString::from(text)
}

fn strings(list: &[&str]) -> StringVec {
    StringVec::from_vec(list.iter().map(|t| s(t)).collect())
}

#[test]
fn a_pending_picker_has_no_answer_yet() {
    assert!(open_result_from_status(FilePickerStatus::Pending).is_none());
    assert!(multi_result_from_status(FilePickerStatus::Pending).is_none());
    assert!(save_result_from_status(FilePickerStatus::Pending).is_none());
}

#[test]
fn a_cancel_has_no_path_and_no_error() {
    let open = open_result_from_status(FilePickerStatus::Cancelled).unwrap();
    assert!(open.path.as_ref().is_none());
    assert_eq!(open.error, OptionString::None);
    let multi = multi_result_from_status(FilePickerStatus::Cancelled).unwrap();
    assert!(multi.paths.as_ref().is_empty());
    assert_eq!(multi.error, OptionString::None);
    let save = save_result_from_status(FilePickerStatus::Cancelled).unwrap();
    assert!(save.target.as_ref().is_none());
    assert_eq!(save.error, OptionString::None);
}

#[test]
fn a_failure_has_no_path_and_says_why() {
    let reason = "choosing a folder is not available on Android yet";
    let open = open_result_from_status(FilePickerStatus::Error(s(reason))).unwrap();
    assert!(open.path.as_ref().is_none(), "no path after a failure");
    assert_eq!(open.error, OptionString::Some(s(reason)), "the reason reaches the app");
    let multi = multi_result_from_status(FilePickerStatus::Error(s(reason))).unwrap();
    assert!(multi.paths.as_ref().is_empty());
    assert_eq!(multi.error, OptionString::Some(s(reason)));
    let save = save_result_from_status(FilePickerStatus::Error(s(reason))).unwrap();
    assert!(save.target.as_ref().is_none());
    assert_eq!(save.error, OptionString::Some(s(reason)));
}

#[test]
fn a_pick_has_its_paths_and_no_error() {
    let open = open_result_from_status(FilePickerStatus::Selected(s("/files/a.md"))).unwrap();
    assert_eq!(open.path.as_ref().unwrap().as_str(), "/files/a.md");
    assert_eq!(open.error, OptionString::None);
    // a single-file request that was answered with several takes the first
    let first = open_result_from_status(FilePickerStatus::SelectedMultiple(strings(&["/a", "/b"]))).unwrap();
    assert_eq!(first.path.as_ref().unwrap().as_str(), "/a");

    let multi = multi_result_from_status(FilePickerStatus::SelectedMultiple(strings(&["/a", "/b"]))).unwrap();
    let paths: Vec<&str> = multi.paths.as_ref().iter().map(|p| p.as_str()).collect();
    assert_eq!(paths, ["/a", "/b"]);
    assert_eq!(multi.error, OptionString::None);
    let one = multi_result_from_status(FilePickerStatus::Selected(s("/only"))).unwrap();
    assert_eq!(one.paths.as_ref().len(), 1);

    let save = save_result_from_status(FilePickerStatus::Selected(s("/files/out.pdf"))).unwrap();
    let target = save.target.as_ref().unwrap();
    assert_eq!(target.kind, SaveTargetKind::Path);
    assert_eq!(target.path.as_ref().unwrap().as_str(), "/files/out.pdf");
    assert_eq!(save.error, OptionString::None);
}

#[test]
fn an_empty_multiple_answer_is_a_cancel_not_an_error() {
    let multi = multi_result_from_status(FilePickerStatus::SelectedMultiple(strings(&[]))).unwrap();
    assert!(multi.paths.as_ref().is_empty());
    assert_eq!(multi.error, OptionString::None);
}

#[test]
fn the_export_that_cannot_happen_names_the_file_and_says_nothing_was_saved() {
    let text = export_unavailable("report.pdf");
    assert!(text.contains("report.pdf"), "{text}");
    assert!(text.contains("NOT saved"), "{text}");
    // a nameless export is still said in words
    let anonymous = export_unavailable("");
    assert!(anonymous.contains("the file"), "{anonymous}");
    assert!(!anonymous.contains("\"\""), "{anonymous}");
}

// ---- a folder the person grants ----

const TREE: &str = "content://com.android.externalstorage.documents/tree/primary%3ADownload/document/primary%3ADownload";

#[test]
fn a_pending_folder_picker_has_no_answer_and_a_cancel_has_no_locator_and_no_error() {
    assert!(folder_result_from_status(FilePickerStatus::Pending).is_none());
    let cancelled = folder_result_from_status(FilePickerStatus::Cancelled).unwrap();
    assert_eq!(cancelled.locator, OptionString::None);
    assert_eq!(cancelled.error, OptionString::None);
}

#[test]
fn a_picked_folder_is_its_locator_untouched() {
    let picked = folder_result_from_status(FilePickerStatus::Selected(s(TREE))).unwrap();
    assert_eq!(picked.locator, OptionString::Some(s(TREE)));
    assert_eq!(picked.error, OptionString::None);
    // a path is a locator on a desktop and on iOS
    let path = folder_result_from_status(FilePickerStatus::Selected(s("/Users/me/Documents"))).unwrap();
    assert_eq!(path.locator, OptionString::Some(s("/Users/me/Documents")));
}

#[test]
fn several_answers_for_a_folder_request_are_the_first_and_a_failure_keeps_its_reason() {
    let first = folder_result_from_status(FilePickerStatus::SelectedMultiple(strings(&["/a", "/b"]))).unwrap();
    assert_eq!(first.locator, OptionString::Some(s("/a")));
    assert_eq!(folder_result_from_status(FilePickerStatus::SelectedMultiple(strings(&[]))).unwrap().locator, OptionString::None);
    let failed = folder_result_from_status(FilePickerStatus::Error(s("access to the folder could not be kept"))).unwrap();
    assert_eq!(failed.locator, OptionString::None, "a failure is not a pick");
    assert_eq!(failed.error, OptionString::Some(s("access to the folder could not be kept")), "and it is not a cancel");
}

// ---- an export: the name the file was saved under, after the write ----

#[test]
fn a_pending_export_has_no_answer_and_a_cancel_saved_nothing_and_says_no_error() {
    assert!(export_result_from_status(FilePickerStatus::Pending).is_none());
    let cancelled = export_result_from_status(FilePickerStatus::Cancelled).unwrap();
    assert_eq!(cancelled.saved_as, OptionString::None);
    assert_eq!(cancelled.error, OptionString::None);
}

#[test]
fn a_saved_export_is_the_name_it_was_saved_under() {
    let saved = export_result_from_status(FilePickerStatus::Selected(s("report (1).pdf"))).unwrap();
    assert_eq!(saved.saved_as, OptionString::Some(s("report (1).pdf")), "the person's name, not the app's suggestion");
    assert_eq!(saved.error, OptionString::None);
}

#[test]
fn an_export_that_failed_saved_nothing_and_is_not_a_cancel() {
    let failed = export_result_from_status(FilePickerStatus::Error(s("the file could not be saved: no space left"))).unwrap();
    assert_eq!(failed.saved_as, OptionString::None, "a failure claims no name");
    assert_eq!(failed.error, OptionString::Some(s("the file could not be saved: no space left")));
}
