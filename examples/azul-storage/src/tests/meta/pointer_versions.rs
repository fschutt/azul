//! A drive index written by a newer version of the app: its pointers of a newer format are said
//! as that - update the app - never as damage (an older AzDrive or azcloud meeting a drive a
//! newer one wrote, as the chaos E2E's older azcloud did).

use crate::meta::pointer;

#[test]
fn a_pointer_of_a_newer_format_asks_for_an_update_and_is_not_damage() {
    let newer = b"azlin-pointer 7\nsize 5\nsomething new 1\n";
    assert_eq!(pointer::newer_format(newer), Some(7));
    let why = pointer::decode(newer).unwrap_err();
    assert!(why.contains("newer version"), "{why}");
    assert!(why.contains("update"), "{why}");
    // The formats this version reads, and what is no pointer at all, are not newer.
    assert_eq!(pointer::newer_format(b"azlin-pointer 1\nsize 5\n"), None);
    assert_eq!(pointer::newer_format(b"azlin-pointer 2\nsize 5\n"), None);
    assert_eq!(pointer::newer_format(b"hello"), None);
    assert!(pointer::decode(b"hello").unwrap_err().contains("not a pointer"));
}
