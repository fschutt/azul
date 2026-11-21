//! D43: the sync index's format feature flags. A device that meets a feature it does not know
//! (or a newer version) leaves the drive as it is - read-only here: nothing up, nothing down,
//! no index written, no blob collected - and asks to be updated, instead of refusing it.

use super::{
    fake_s3::S3Bucket,
    sync::{index_of, Device, PREFIX},
};
use crate::{
    sync::{collect_garbage, remote::index_key},
    user_errors::{Behaviour, Class, Code, Lang, UserError},
};

#[test]
fn a_device_that_meets_a_feature_it_does_not_know_leaves_the_drive_as_it_is() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    a.write("a.md", b"a");
    a.sync(&store);
    // A newer device wrote the index with a feature this one does not know.
    let mut index = index_of(&store);
    index.features.push(String::from("chunked-files"));
    store.write(&index_key(PREFIX), index.to_bytes());
    a.write("a.md", b"a, edited");
    a.write("b.md", b"new");
    let report = a.sync(&store);
    assert_eq!(report.newer_format, vec![String::from("chunked-files")]);
    assert_eq!(
        (report.files_up, report.files_down, report.index_written),
        (0, 0, false),
        "{}",
        report.summary()
    );
    assert!(report.summary().contains("update"), "{}", report.summary());
    let after = index_of(&store);
    assert_eq!(
        after.features,
        vec![String::from("chunked-files")],
        "as the newer device wrote it"
    );
    assert_eq!(after.files.len(), 1);
    assert_eq!(
        a.read("a.md").as_deref(),
        Some(&b"a, edited"[..]),
        "nothing here changed"
    );
    // Nor does the garbage collection delete a blob the newer format may name.
    let err = collect_garbage(&store, PREFIX, -1, false)
        .unwrap_err()
        .to_string();
    assert!(err.contains("update"), "{err}");
}

#[test]
fn the_newer_format_is_said_as_a_read_only_drive_to_update_the_app_for() {
    let row = Code::NewerFormat.row();
    assert_eq!(
        (row.class, row.behaviour),
        (Class::ReadOnly, Behaviour::ReadsOnly)
    );
    assert_eq!(Code::parse("newer_format"), Some(Code::NewerFormat));
    let error = UserError {
        code: Code::NewerFormat,
        retry_after: None,
        request_id: None,
        detail: String::from("chunked-files"),
    };
    let text = error.message(Lang::En);
    assert!(text.contains("Update"), "{text}");
    assert!(!error.message(Lang::De).is_empty());
}
