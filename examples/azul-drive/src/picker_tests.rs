//! AzDrive as the file picker: the request line, the answer line, the type filter, what the
//! strip's Open / Save button does, and the registration other apps find it by.

use std::path::{Path, PathBuf};

use super::*;

fn files(keys: &[&str]) -> Vec<(String, bool)> {
    keys.iter().map(|k| (k.to_string(), k.ends_with('/'))).collect()
}

fn answer(keys: &[&str]) -> Decision {
    Decision::Answer(keys.iter().map(|k| k.to_string()).collect())
}

#[test]
fn a_request_line_names_what_to_pick() {
    let line = r#"{"v":1, "op":"open", "multiple":true, "title":"Insert picture",
        "default_location":"/Users/me/Pictures",
        "filters":[{"name":"Pictures","patterns":["*.png","*.jpg"]}],
        "accept":"local-only", "cloud":"handle", "want_link":false, "app":"AzWriter",
        "parent":{"x":120,"y":80,"width":1200,"height":760}}"#;
    let request = parse_request(&line.replace('\n', " ")).unwrap();
    assert_eq!(request.op, PickOp::Open);
    assert!(request.multiple);
    assert_eq!(request.title, "Insert picture");
    assert_eq!(request.default_location.as_deref(), Some("/Users/me/Pictures"));
    assert_eq!(request.filters[0].patterns, vec!["*.png", "*.jpg"]);
    assert_eq!(request.accept, Accept::LocalOnly);
    assert_eq!(request.cloud, CloudMode::Handle);
    assert_eq!(request.app, "AzWriter");
    // The defaults: one item, local and cloud, a cloud object fetched as a copy.
    let plain = parse_request(r#"{"v":1,"op":"save"}"#).unwrap();
    assert_eq!(plain.op, PickOp::Save);
    assert!(!plain.multiple);
    assert_eq!(plain.accept, Accept::LocalAndCloud);
    assert_eq!(plain.cloud, CloudMode::Copy);
    assert_eq!(
        parse_request(r#"{"v":1,"op":"open-directory"}"#).unwrap().op,
        PickOp::OpenDirectory
    );
}

#[test]
fn a_request_of_another_protocol_or_without_an_op_is_refused() {
    assert!(parse_request(r#"{"v":2,"op":"open"}"#).is_err());
    assert!(parse_request(r#"{"v":1}"#).is_err());
    assert!(parse_request(r#"{"v":1,"op":"delete"}"#).is_err());
    assert!(parse_request("not json").is_err());
}

#[test]
fn the_answer_is_one_line_after_its_prefix() {
    let line = PickAnswer::selected(vec![PickedItem::local("/home/me/a.png")]).line();
    assert!(line.starts_with("AZPICK {"), "{line}");
    assert!(!line.contains('\n'));
    let value: serde_json::Value = serde_json::from_str(&line["AZPICK ".len()..]).unwrap();
    assert_eq!(value["v"], 1);
    assert_eq!(value["status"], "selected");
    assert_eq!(value["items"][0]["path"], "/home/me/a.png");
    let cancelled: serde_json::Value =
        serde_json::from_str(&PickAnswer::cancelled().line()["AZPICK ".len()..]).unwrap();
    assert_eq!(cancelled["status"], "cancelled");
    let error: serde_json::Value =
        serde_json::from_str(&PickAnswer::error("no keys").line()["AZPICK ".len()..]).unwrap();
    assert_eq!(error["status"], "error");
    assert_eq!(error["message"], "no keys");
    let cloud = PickedItem {
        path: None,
        cloud: Some(CloudRef {
            drive_id: String::from("photos"),
            drive_name: String::from("Photos"),
            key: String::from("2026/beach.jpg"),
            size: Some(182_733),
            etag: Some(String::from("\"9b2c\"")),
            modified: Some(1_759_800_000),
            link: None,
        }),
    };
    let value: serde_json::Value =
        serde_json::from_str(&PickAnswer::selected(vec![cloud]).line()["AZPICK ".len()..]).unwrap();
    assert_eq!(value["items"][0]["cloud"]["key"], "2026/beach.jpg");
    assert_eq!(value["items"][0]["cloud"]["size"], 182_733);
    assert!(value["items"][0].get("path").is_none());
}

#[test]
fn a_filter_shows_folders_and_the_files_its_patterns_match() {
    let pictures = PickFilter {
        name: String::from("Pictures"),
        patterns: vec![String::from("*.png"), String::from("*.jpg")],
    };
    assert!(filter_shows(Some(&pictures), "beach.PNG", false), "case does not count");
    assert!(filter_shows(Some(&pictures), "a.jpg", false));
    assert!(!filter_shows(Some(&pictures), "notes.txt", false));
    assert!(filter_shows(Some(&pictures), "Holidays", true), "folders always show");
    assert!(filter_shows(None, "notes.txt", false), "no filter: everything");
    assert!(glob_matches("report-??.txt", "report-01.txt"));
    assert!(!glob_matches("report-??.txt", "report-1.txt"));
    assert!(glob_matches("*", "anything"));
    assert!(glob_matches("*.tar.gz", "a.tar.gz"));
}

#[test]
fn open_answers_files_and_goes_into_a_folder() {
    assert_eq!(
        decide(PickOp::Open, false, &files(&["Pictures/"]), "", ""),
        Decision::OpenFolder(String::from("Pictures/"))
    );
    assert_eq!(decide(PickOp::Open, false, &files(&["a.png"]), "", ""), answer(&["a.png"]));
    assert_eq!(
        decide(PickOp::Open, false, &files(&["a.png", "b.png"]), "", ""),
        answer(&["a.png"]),
        "one item asked for: the first"
    );
    assert_eq!(
        decide(PickOp::Open, true, &files(&["a.png", "sub/", "b.png"]), "", ""),
        answer(&["a.png", "b.png"]),
        "several: the files of the selection"
    );
    assert_eq!(
        decide(PickOp::Open, false, &[], "Docs/", " b.png "),
        answer(&["Docs/b.png"]),
        "a typed name in the open folder"
    );
    assert_eq!(decide(PickOp::Open, false, &[], "Docs/", ""), Decision::Nothing);
}

#[test]
fn open_directory_answers_the_selected_or_the_open_folder() {
    assert_eq!(
        decide(PickOp::OpenDirectory, false, &files(&["Docs/sub/"]), "Docs/", ""),
        answer(&["Docs/sub/"])
    );
    assert_eq!(decide(PickOp::OpenDirectory, false, &[], "Docs/", ""), answer(&["Docs/"]));
    assert_eq!(
        decide(PickOp::OpenDirectory, false, &files(&["Docs/a.txt"]), "Docs/", ""),
        answer(&["Docs/"]),
        "a file selected: still the open folder"
    );
}

#[test]
fn save_answers_the_typed_name_in_the_open_folder() {
    assert_eq!(
        decide(PickOp::Save, false, &[], "Docs/", "report.txt"),
        answer(&["Docs/report.txt"])
    );
    assert_eq!(
        decide(PickOp::Save, false, &files(&["Docs/old/"]), "Docs/", ""),
        Decision::OpenFolder(String::from("Docs/old/"))
    );
    assert_eq!(
        decide(PickOp::Save, false, &files(&["Docs/old.txt"]), "Docs/", ""),
        answer(&["Docs/old.txt"]),
        "a file picked to be replaced"
    );
    assert_eq!(decide(PickOp::Save, false, &[], "Docs/", "a/b"), Decision::NeedName);
    assert_eq!(decide(PickOp::Save, false, &[], "Docs/", "  "), Decision::NeedName);
    assert_eq!(save_name("  r.txt "), Some(String::from("r.txt")));
    assert_eq!(save_name(".."), None);
    assert_eq!(save_name("a\\b"), None);
}

#[test]
fn the_registration_names_this_program_and_its_protocol() {
    let value = registration("/Applications/AzDrive.app/Contents/MacOS/AzDrive");
    assert_eq!(value["format"], "azul-storage.file-picker");
    assert_eq!(value["version"], 1);
    assert_eq!(value["provider"], "AzDrive");
    assert_eq!(value["exec"], "/Applications/AzDrive.app/Contents/MacOS/AzDrive");
    assert_eq!(value["args"], serde_json::json!(["--pick"]));
    assert_eq!(value["protocol"], 1);
    assert_eq!(
        value["modes"],
        serde_json::json!(["open", "open-multiple", "open-directory", "save"])
    );
    assert_eq!(value["cloud"], true);
    assert_eq!(
        registration_path(Path::new("/c/azul-storage/drives.json")),
        PathBuf::from("/c/azul-storage/file-picker.json")
    );
}

#[test]
fn the_default_location_opens_in_the_drive_that_holds_it() {
    let roots = vec![
        (String::from("home"), PathBuf::from("/Users/me")),
        (String::from("data"), PathBuf::from("/Users/me/Azlin")),
    ];
    assert_eq!(
        place_of_path(Path::new("/Users/me/Pictures"), &roots),
        Some((String::from("home"), String::from("Pictures/")))
    );
    assert_eq!(
        place_of_path(Path::new("/Users/me/Azlin/notes"), &roots),
        Some((String::from("data"), String::from("notes/"))),
        "the drive whose root is deepest"
    );
    assert_eq!(
        place_of_path(Path::new("/Users/me"), &roots),
        Some((String::from("home"), String::new()))
    );
    assert_eq!(place_of_path(Path::new("/Users/meadow"), &roots), None, "whole folder names");
    assert_eq!(place_of_path(Path::new("/etc"), &roots), None);
}
