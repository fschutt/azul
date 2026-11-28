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
