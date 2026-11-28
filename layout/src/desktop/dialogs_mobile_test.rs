//! The answers of the phones' file dialogs, as the apps read them (`FileOpenResult`, `FileOpenMultiResult`,
//! `SaveTargetResult`): a failure carries its reason and is not a cancel; `save_bytes` says why it answers `false`.
//! The mappers are compiled on every target (they are pure), so these run on the host.

use azul_css::{corety::OptionString, AzString, StringVec};

use super::{
    export_unavailable, multi_result_from_status, open_result_from_status, save_result_from_status,
    FilePickerStatus, SaveTargetKind,
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
