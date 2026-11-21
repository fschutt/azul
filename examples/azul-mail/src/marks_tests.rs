//! X-DELTA-APPS3-6: the marks of a folder (`flags.json`) are read as "none" only when there is no
//! file; a file that cannot be read is an error, a save of one mark merges into the file, and a save
//! on a file that cannot be read writes nothing. The twin of `azlin_sync::read_marks`
//! (X-DELTA-SYNC4-21), which swept two files and left this one.

use crate::{
    listing::{self, LocalFlags},
    read_flags, save_marks,
    store::MailStore,
    testutil::{MailFolder, TempDir},
};

/// RED on the code before: `read_flags` read a failed `flags.json` as "no marks", the window showed
/// everything unread, and the next "Done" on one mail wrote that one mark over the file - the marks
/// made earlier and not yet pushed to the drive gone.
#[test]
fn x_delta_apps3_6_an_unreadable_flags_file_is_no_empty_set_and_a_save_never_replaces_it() {
    let dir = TempDir::new("marks-unreadable");
    let store = MailStore::new(dir.folder());
    // No file yet: the empty set, the only case that is.
    assert_eq!(read_flags(&store, "inbox"), Ok(LocalFlags::create()));
    let mut made = LocalFlags::create();
    made.read.insert(1, true);
    made.flagged.insert(2, true);
    save_marks(&store, "inbox", &made).unwrap();
    let file = dir.0.join(listing::flags_key("inbox"));
    let before = std::fs::read(&file).unwrap();
    crate::testutil::make_unreadable(&file);
    // The window opens the folder: an error, not "no marks".
    assert!(read_flags(&store, "inbox").is_err());
    // The user presses "Done" on one mail: the window's set holds that one mark only. Nothing is
    // written; the file is byte for byte as it was.
    let mut one = LocalFlags::create();
    one.read.insert(9, true);
    let refused = save_marks(&store, "inbox", &one);
    crate::testutil::make_readable(&file);
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(std::fs::read(&file).unwrap(), before);
    // Readable again: the save merges - the marks made earlier stay.
    save_marks(&store, "inbox", &one).unwrap();
    let merged = read_flags(&store, "inbox").unwrap();
    assert_eq!(
        (
            merged.read.keys().copied().collect::<Vec<_>>(),
            merged.flagged.len()
        ),
        (vec![1, 9], 1)
    );
    // A file that is no marks file this AzMail reads is an error too, and is left as it is.
    std::fs::write(&file, b"{ not json").unwrap();
    assert!(read_flags(&store, "inbox").is_err());
    assert!(save_marks(&store, "inbox", &one).is_err());
    assert_eq!(std::fs::read(&file).unwrap(), b"{ not json");
}
