use super::*;

#[test]
fn an_ops_description_is_json_that_names_it() {
    for name in [OP_OPEN_SAMPLE, OP_IMPORT_SAMPLE] {
        let v: serde_json::Value = serde_json::from_str(&op_def(name, "what it does")).expect("valid JSON");
        assert_eq!(v["name"], name);
        assert_eq!(v["summary"], "what it does");
        assert!(v["args"].is_array());
    }
}

#[test]
fn the_ops_are_the_apps_own_and_differ() {
    assert!(OP_OPEN_SAMPLE.starts_with("azreader_"));
    assert!(OP_IMPORT_SAMPLE.starts_with("azreader_"));
    assert_ne!(OP_OPEN_SAMPLE, OP_IMPORT_SAMPLE);
}

#[test]
fn the_sample_the_ops_add_is_a_book_the_library_takes() {
    let bytes = crate::sample::sample_epub();
    assert!(bytes.starts_with(b"PK"), "an EPUB is a zip");
    assert!(crate::library::Format::of_name(crate::sample::FILE_NAME).is_some());
}

#[test]
fn a_wait_step_names_what_it_waits_for() {
    assert_eq!(waited_for("{\"for\":\"library\"}"), "library");
    assert_eq!(waited_for("{}"), "");
    assert_eq!(waited_for("not json"), "");
}

#[test]
fn the_library_and_the_chapter_are_ready_only_when_they_are() {
    let mut st = crate::app::AppState::new(
        RefAny::new(0u8),
        std::path::PathBuf::from("."),
        crate::settings::ReadingSettings::default(),
        false,
    );
    assert_eq!(ready("library", &st), Some(false), "not read yet");
    assert_eq!(ready("chapter", &st), Some(false), "no book open");
    st.listed = true;
    assert_eq!(ready("library", &st), Some(false), "read, but empty");
    assert_eq!(ready("nonsense", &st), None);
}
