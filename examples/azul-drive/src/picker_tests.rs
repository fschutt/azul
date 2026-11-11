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

/// X-AR5a-6: the registration other apps find the picker by is put in place whole, by rename: a
/// reader that has the old one open keeps the bytes it had (`fs::write` truncated that very file
/// and wrote into it), the new one parses, and nothing else is left in the folder.
#[cfg(unix)]
#[test]
fn x_ar5a_6_the_registration_replaces_the_file_it_does_not_write_into_it() {
    use std::io::Read;
    let dir = azul_storage::testing::TempDir::new("picker-registration");
    let drives = dir.path().join("azul-storage").join("drives.json");
    let path = registration_path(&drives);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, br#"{"format": "an older registration"}"#).unwrap();
    let mut reading = std::fs::File::open(&path).unwrap();
    register(&drives);
    let mut old = String::new();
    reading.read_to_string(&mut old).unwrap();
    assert!(
        old.contains("an older registration"),
        "the open file kept its bytes: {old}"
    );
    let text = std::fs::read_to_string(&path).unwrap();
    let value: serde_json::Value = serde_json::from_str(&text).expect("whole JSON");
    assert_eq!(value["format"], "azul-storage.file-picker");
    assert_eq!(value["provider"], "AzDrive");
    let names: Vec<String> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["file-picker.json".to_string()], "{names:?}");
}

/// A first start: the folder of the drives file is made, and the registration is there.
#[test]
fn x_ar5a_6_a_first_start_makes_the_folder_and_the_registration() {
    let dir = azul_storage::testing::TempDir::new("picker-registration-new");
    let drives = dir
        .path()
        .join("config")
        .join("azul-storage")
        .join("drives.json");
    register(&drives);
    let text = std::fs::read_to_string(registration_path(&drives)).expect("written");
    assert!(text.ends_with('\n'), "{text:?}");
    assert!(
        serde_json::from_str::<serde_json::Value>(&text).is_ok(),
        "{text}"
    );
}

/// X-DELTA-AZDRIVE-FILES-29: the copies a picker fetches wait in scratch - the drive's kept folder,
/// swept when unused for a day - with a cache folder too; `<cache>/picked/<drive>` was swept by
/// nothing and the plaintext of an encrypted drive piled up in it.
#[test]
fn x_delta_azdrive_files_29_the_pickers_copies_wait_in_scratch_with_or_without_a_cache_folder() {
    let root = copy_root("d_pick29").unwrap();
    assert!(root.starts_with(picked_scratch().dir()), "{root:?}");
    assert_eq!(root, picked_scratch().kept("d_pick29"));
    assert!(root.is_dir());
    // Another drive, another folder.
    assert_ne!(root, copy_root("d_pick29b").unwrap());
    std::fs::remove_dir_all(&root).unwrap();
}

/// `path` (a file, or a folder) made `days` old.
#[cfg(unix)]
fn aged_by(path: &Path, days: u32) {
    let when =
        std::time::SystemTime::now() - std::time::Duration::from_secs(u64::from(days) * 86_400);
    std::fs::File::open(path)
        .unwrap()
        .set_modified(when)
        .unwrap();
}

/// `path` and everything below it made `days` old: a kept folder is as old as the newest thing in it.
#[cfg(unix)]
fn aged_tree(path: &Path, days: u32) {
    if path.is_dir() && !path.is_symlink() {
        for entry in std::fs::read_dir(path).unwrap() {
            aged_tree(&entry.unwrap().path(), days);
        }
    }
    if !path.is_symlink() {
        aged_by(path, days);
    }
}

/// A copy of a cloud object as the picker's fetch leaves it: the key's file, in a staging folder.
#[cfg(unix)]
fn fetched(staging: &Path, name: &str) -> PathBuf {
    std::fs::create_dir_all(staging).unwrap();
    let path = staging.join(name);
    std::fs::write(&path, b"the drive's bytes").unwrap();
    path
}

/// The handle of the picked object `key` of the drive `d_pick24`.
#[cfg(unix)]
fn cloud_ref(key: &str) -> CloudRef {
    CloudRef {
        drive_id: String::from("d_pick24"),
        drive_name: String::from("Docs"),
        key: key.to_string(),
        size: Some(17),
        etag: Some(String::from("\"e1\"")),
        modified: Some(5),
        link: None,
    }
}

/// X-DELTA-SYNC2-4. The picker's copies were one kept folder per drive of scratch with no record and
/// no read-only mark: a host program that opened a picked copy and saved into it lost the edit at the
/// next start - the sweep takes a folder a day after its newest entry, and a folder with no record
/// "holds nobody's work". A picked copy is an open copy now ([`hand_out`]): a folder of its own, read
/// only, noted as made - an edit saved over it survives the sweep four days on, and an untouched copy
/// of that age is still swept (the negative control: the record is what keeps the edit). RED on the
/// old code: its folder holds no record, so the sweep removes the edit with the folder.
#[cfg(unix)]
#[test]
fn x_delta_sync2_4_a_picked_copy_is_an_open_copy_and_an_edit_saved_over_it_survives_the_sweep() {
    use azul_storage::scratch::{MadeState, OPEN_COPY_KEPT};
    use std::os::unix::fs::PermissionsExt;

    let base = azul_storage::testing::TempDir::new("picker-sync2-4");
    let copies = Scratch::under(base.path(), "open");
    let staging = base.path().join("staging");
    let a_staged = fetched(&staging, "a.docx");
    let b_staged = fetched(&staging, "b.docx");

    let a = hand_out(&copies, &cloud_ref("docs/a.docx"), &a_staged).unwrap();
    let b = hand_out(&copies, &cloud_ref("docs/b.docx"), &b_staged).unwrap();
    assert!(a.starts_with(copies.dir()), "{a:?}");
    assert_eq!(std::fs::read(&a).unwrap(), b"the drive's bytes");
    assert_eq!(
        std::fs::metadata(&a).unwrap().permissions().mode() & 0o222,
        0,
        "read-only: a save in place fails loudly"
    );
    assert!(!a_staged.exists(), "the fetched file moved into its copy");
    assert_ne!(
        a.parent(),
        b.parent(),
        "a folder per file: one copy never holds another's"
    );
    assert_eq!(
        Scratch::made_state(a.parent().unwrap(), "a.docx"),
        MadeState::Untouched
    );

    // The host program saves its edit over the copy: an editor renames a new file over it.
    let saving = a.parent().unwrap().join(".a.docx.saving");
    std::fs::write(&saving, b"the user's edit").unwrap();
    std::fs::rename(&saving, &a).unwrap();

    // Four days go by. The untouched copy b is the file that was written, four days on (noted after
    // it was aged, its record and folder aged last); a's folder holds the edit.
    aged_tree(copies.dir(), 4);
    let b_folder = b.parent().unwrap();
    Scratch::note_made(b_folder, "b.docx").unwrap();
    aged_by(&b_folder.join(".made.json"), 4);
    aged_by(b_folder, 4);
    assert_eq!(
        Scratch::made_state(b_folder, "b.docx"),
        MadeState::Untouched
    );

    assert_eq!(
        copies.sweep(OPEN_COPY_KEPT),
        1,
        "one folder went: the untouched copy's"
    );
    assert_eq!(
        std::fs::read(&a).unwrap(),
        b"the user's edit",
        "the edit saved over a picked copy is there"
    );
    assert!(!b.exists(), "an untouched copy that old is swept");
}

/// X-DELTA-SYNC2-4: the same version picked again is the same copy and needs no second fetch; another
/// version is another folder and the first stays as it was.
#[cfg(unix)]
#[test]
fn x_delta_sync2_4_the_same_version_picked_again_is_one_copy_and_another_version_another() {
    let base = azul_storage::testing::TempDir::new("picker-sync2-4-version");
    let copies = Scratch::under(base.path(), "open");
    let staging = base.path().join("staging");
    let first = hand_out(
        &copies,
        &cloud_ref("docs/a.docx"),
        &fetched(&staging, "a.docx"),
    )
    .unwrap();
    // Picked again: the copy is there and is what was written - handed out as it is.
    let again_staged = fetched(&staging, "a.docx");
    let again = hand_out(&copies, &cloud_ref("docs/a.docx"), &again_staged).unwrap();
    assert_eq!(first, again);
    assert!(
        again_staged.exists(),
        "nothing was moved: the copy needed no fetch"
    );
    // The drive's file changed (another tag): another copy, and the first is untouched.
    let mut newer = cloud_ref("docs/a.docx");
    newer.etag = Some(String::from("\"e2\""));
    let second = hand_out(&copies, &newer, &fetched(&staging, "a.docx")).unwrap();
    assert_ne!(first.parent(), second.parent());
    assert!(first.exists() && second.exists());
}

/// X-DELTA-SYNC2-4: a key with no usable file name is refused, and the fetched file stays where it is.
#[cfg(unix)]
#[test]
fn x_delta_sync2_4_an_object_with_no_file_name_is_not_handed_out() {
    let base = azul_storage::testing::TempDir::new("picker-sync2-4-name");
    let copies = Scratch::under(base.path(), "open");
    let staged = fetched(&base.path().join("staging"), "x");
    let refused = hand_out(&copies, &cloud_ref("docs/.."), &staged).unwrap_err();
    assert!(
        matches!(refused, DriveError::InvalidKey { .. }),
        "{refused:?}"
    );
    assert!(staged.exists());
}
