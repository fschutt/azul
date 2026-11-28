//! What a storage provider's rows, flags and exceptions mean as `Documents` entries and errors.

use azul_core::documents::DocumentError;

use super::*;

const DIR: &str = "vnd.android.document/directory";
// four distinct bits: the test must not depend on the framework's values (the backend reads those at run time)
const BITS: FlagBits = FlagBits {
    write: 1 << 1,
    delete: 1 << 2,
    create: 1 << 3,
    rename: 1 << 6,
};

fn row(id: &str, name: Option<&str>, mime: Option<&str>) -> DocumentRow {
    DocumentRow {
        document_id: id.to_string(),
        display_name: name.map(str::to_string),
        mime: mime.map(str::to_string),
        ..DocumentRow::default()
    }
}

#[test]
fn a_file_row_becomes_an_entry_with_what_the_flags_allow() {
    let mut r = row("primary:Download/report.pdf", Some("report.pdf"), Some("application/pdf"));
    r.size = Some(1234);
    r.modified_ms = Some(1_700_000_000_000);
    r.flags = BITS.write | BITS.rename;
    let e = entry_from_row("content://x/tree/t/document/d".to_string(), &r, DIR, BITS);
    assert_eq!(e.uri.as_str(), "content://x/tree/t/document/d");
    assert_eq!(e.name.as_str(), "report.pdf");
    assert_eq!(e.mime.as_str(), "application/pdf");
    assert!(!e.is_dir);
    assert_eq!(e.size.as_option().copied(), Some(1234));
    assert_eq!(e.modified_ms.as_option().copied(), Some(1_700_000_000_000));
    assert!(e.can_write && e.can_rename && !e.can_delete);
}

#[test]
fn a_folder_row_is_a_folder_by_its_type_and_creates_where_a_file_writes() {
    let mut r = row("primary:Download", Some("Download"), Some(DIR));
    r.size = Some(4096);
    r.flags = BITS.create | BITS.delete;
    let e = entry_from_row("u".to_string(), &r, DIR, BITS);
    assert!(e.is_dir);
    assert_eq!(e.mime.as_str(), "inode/directory", "the engine's word for a folder, not the framework's");
    assert!(e.size.as_option().is_none(), "a folder has no size");
    assert!(e.can_write && e.can_delete && !e.can_rename);
    // the write bit of a FILE does not make a folder writable
    r.flags = BITS.write;
    assert!(!entry_from_row("u".to_string(), &r, DIR, BITS).can_write);
}

#[test]
fn what_a_provider_does_not_know_is_none_and_never_a_made_up_number() {
    let mut r = row("id", Some("a.txt"), Some("text/plain"));
    r.size = Some(-1);
    r.modified_ms = Some(0);
    let e = entry_from_row("u".to_string(), &r, DIR, BITS);
    assert!(e.size.as_option().is_none() && e.modified_ms.as_option().is_none());
    r.size = None;
    r.modified_ms = None;
    let e = entry_from_row("u".to_string(), &r, DIR, BITS);
    assert!(e.size.as_option().is_none() && e.modified_ms.as_option().is_none());
    r.size = Some(0);
    assert_eq!(entry_from_row("u".to_string(), &r, DIR, BITS).size.as_option().copied(), Some(0), "an empty file is 0 bytes");
}

#[test]
fn a_row_without_a_name_is_named_by_the_last_piece_of_its_id() {
    for (id, expected) in [
        ("primary:Download/sub/a.txt", "a.txt"),
        ("primary:Download", "Download"),
        ("1234", "1234"),
        ("raw:/storage/emulated/0/Music/", "Music"),
        ("", ""),
    ] {
        let e = entry_from_row("u".to_string(), &row(id, None, None), DIR, BITS);
        assert_eq!(e.name.as_str(), expected, "{id}");
        let blank = entry_from_row("u".to_string(), &row(id, Some(""), None), DIR, BITS);
        assert_eq!(blank.name.as_str(), expected, "a blank name is no name: {id}");
    }
}

#[test]
fn a_type_the_provider_does_not_give_is_taken_from_the_name() {
    for given in [None, Some(""), Some("application/octet-stream")] {
        let e = entry_from_row("u".to_string(), &row("id", Some("notes.md"), given), DIR, BITS);
        assert_eq!(e.mime.as_str(), "text/markdown", "{given:?}");
    }
    let e = entry_from_row("u".to_string(), &row("id", Some("mystery"), None), DIR, BITS);
    assert_eq!(e.mime.as_str(), "application/octet-stream");
    let e = entry_from_row("u".to_string(), &row("id", Some("x.png"), Some("image/x-special")), DIR, BITS);
    assert_eq!(e.mime.as_str(), "image/x-special", "a type the provider gives is kept");
}

fn kind(error: &DocumentError) -> &'static str {
    match error {
        DocumentError::NotFound(_) => "NotFound",
        DocumentError::Denied(_) => "Denied",
        DocumentError::TimedOut(_) => "TimedOut",
        DocumentError::Unsupported(_) => "Unsupported",
        DocumentError::Exists(_) => "Exists",
        DocumentError::NotADirectory(_) => "NotADirectory",
        DocumentError::Cancelled => "Cancelled",
        DocumentError::Failed(_) => "Failed",
    }
}

#[test]
fn a_providers_exceptions_say_what_the_app_does_next() {
    for (exception, expected) in [
        ("java.lang.SecurityException: Permission Denial: reading X uri content://y requires grantUriPermission()", "Denied"),
        ("java.lang.SecurityException", "Denied"),
        ("java.io.FileNotFoundException: No such file or directory", "NotFound"),
        ("java.io.FileNotFoundException: open failed: EACCES (Permission denied)", "Denied"),
        ("java.io.FileNotFoundException: open failed: ENOENT (No such file or directory)", "NotFound"),
        ("java.io.FileNotFoundException: open failed: ENOSPC (No space left on device)", "Failed"),
        ("java.io.FileNotFoundException: open failed: EROFS (Read-only file system)", "Denied"),
        ("java.lang.IllegalArgumentException: Failed to determine if X is child of Y: java.io.FileNotFoundException", "Denied"),
        ("java.lang.IllegalArgumentException: Document X is not a descendant of Y", "Denied"),
        ("java.lang.IllegalArgumentException: Unknown URI content://z", "NotFound"),
        ("java.lang.IllegalArgumentException: something else", "Failed"),
        ("java.lang.IllegalStateException: Already exists /storage/emulated/0/a.txt", "Exists"),
        ("java.lang.IllegalStateException: boom", "Failed"),
        ("java.lang.UnsupportedOperationException: Rename not supported", "Unsupported"),
        ("android.os.OperationCanceledException", "Cancelled"),
        ("android.os.DeadObjectException", "Failed"),
        ("android.os.RemoteException: provider died", "Failed"),
        ("java.io.IOException: write failed: ENOSPC (No space left on device)", "Failed"),
        ("java.io.IOException: open failed: EACCES (Permission denied)", "Denied"),
        ("java.io.IOException: open failed: ENOENT (No such file or directory)", "NotFound"),
        ("java.io.IOException: unexpected end of stream", "Failed"),
        ("android.system.ErrnoException: read failed: ENOENT (No such file or directory)", "NotFound"),
        ("java.lang.OutOfMemoryError: too big", "Failed"),
        ("", "Failed"),
    ] {
        let got = classify_failure(exception, "Downloads");
        assert_eq!(kind(&got), expected, "{exception}");
    }
}

#[test]
fn the_failure_names_the_place_and_keeps_the_providers_words_where_the_cause_is_not_clear() {
    match classify_failure("java.lang.IllegalStateException: the provider is confused", "Photos") {
        DocumentError::Failed(why) => assert!(why.as_str().contains("Photos") && why.as_str().contains("the provider is confused"), "{}", why.as_str()),
        other => panic!("{other:?}"),
    }
    match classify_failure("java.io.IOException: write failed: ENOSPC (No space left on device)", "Photos") {
        DocumentError::Failed(why) => assert!(why.as_str().contains("no space left"), "{}", why.as_str()),
        other => panic!("{other:?}"),
    }
    match classify_failure("java.lang.SecurityException: x", "Photos") {
        DocumentError::Denied(what) => assert_eq!(what.as_str(), "Photos"),
        other => panic!("{other:?}"),
    }
    match classify_failure("com.vendor.StrangeException: odd", "Photos") {
        DocumentError::Failed(why) => assert!(why.as_str().contains("com.vendor.StrangeException: odd"), "{}", why.as_str()),
        other => panic!("{other:?}"),
    }
}

// ---- the locators ----

const EXTERNAL: &str = "com.android.externalstorage.documents";

#[test]
fn the_locators_of_a_tree_a_document_in_it_and_a_picked_document_are_taken_apart() {
    assert_eq!(
        Locator::parse("content://com.android.externalstorage.documents/tree/primary%3ADownload"),
        Some(Locator::Tree { authority: EXTERNAL.to_string(), tree_id: "primary:Download".to_string() })
    );
    assert_eq!(
        Locator::parse("content://com.android.externalstorage.documents/tree/primary%3ADownload/document/primary%3ADownload%2Fsub%2Fa%20b.txt"),
        Some(Locator::TreeDocument {
            authority: EXTERNAL.to_string(),
            tree_id: "primary:Download".to_string(),
            doc_id: "primary:Download/sub/a b.txt".to_string(),
        })
    );
    assert_eq!(
        Locator::parse("content://com.android.providers.downloads.documents/document/msf%3A1234"),
        Some(Locator::Document { authority: "com.android.providers.downloads.documents".to_string(), doc_id: "msf:1234".to_string() })
    );
}

#[test]
fn what_is_not_a_document_uri_is_not_a_locator() {
    for text in [
        "",
        "/storage/emulated/0/Download",
        "file:///storage/emulated/0/a.txt",
        "content://",
        "content:///tree/x",
        "content://x/tree/",
        "content://x/tree/a/document",
        "content://x/tree/a/document/",
        "content://x/tree/a/doc/b",
        "content://x/document/a/b",
        "content://x/tree/a/document/b/c",
        "content://x/tree/a?x=1",
        "content://x/tree/a#frag",
        "content://x/tree/%ZZ",
        "content://x/tree/%FF%FE",
    ] {
        assert_eq!(Locator::parse(text), None, "{text:?}");
    }
}

#[test]
fn the_uri_of_a_document_is_the_one_the_framework_builds() {
    // a root becomes its root document inside the tree; an id with a space, a slash, a colon and a bracket is encoded as Uri.encode does
    let tree = Locator::parse("content://com.android.externalstorage.documents/tree/primary%3ADownload").unwrap();
    assert_eq!(
        tree.document_uri(),
        "content://com.android.externalstorage.documents/tree/primary%3ADownload/document/primary%3ADownload"
    );
    assert_eq!(tree.tree_uri().as_deref(), Some("content://com.android.externalstorage.documents/tree/primary%3ADownload"));
    assert_eq!(tree.document_id(), "primary:Download");
    let child = tree.child("primary:Download/photo (1).jpg").unwrap();
    assert_eq!(
        child,
        "content://com.android.externalstorage.documents/tree/primary%3ADownload/document/primary%3ADownload%2Fphoto%20(1).jpg"
    );
    // and the child's locator parses back to the same document
    let parsed = Locator::parse(&child).unwrap();
    assert_eq!(parsed.document_id(), "primary:Download/photo (1).jpg");
    assert_eq!(parsed.document_uri(), child, "canonical in, canonical out");
    assert_eq!(parsed.tree_uri(), tree.tree_uri());
    // a locator spelled with %28 for ( is the same document, written the framework's way
    let other = Locator::parse(
        "content://com.android.externalstorage.documents/tree/primary%3ADownload/document/primary%3ADownload%2Fphoto%20%281%29.jpg",
    )
    .unwrap();
    assert_eq!(other.document_uri(), child);
}

#[test]
fn a_picked_document_has_no_folder_around_it() {
    let one = Locator::parse("content://com.android.providers.downloads.documents/document/msf%3A1234").unwrap();
    assert_eq!(one.tree_uri(), None);
    assert_eq!(one.child("x"), None);
    assert_eq!(one.document_uri(), "content://com.android.providers.downloads.documents/document/msf%3A1234");
}

#[test]
fn the_encoding_is_androids_uri_encode() {
    for (text, expected) in [
        ("abc-XYZ_0.9~", "abc-XYZ_0.9~"),
        ("!'()*", "!'()*"),
        ("a b", "a%20b"),
        ("a/b:c", "a%2Fb%3Ac"),
        ("a+b", "a%2Bb"),
        ("100%", "100%25"),
        ("\u{e4}", "%C3%A4"),
        ("\u{1F600}", "%F0%9F%98%80"),
        ("", ""),
    ] {
        assert_eq!(android_encode(text), expected, "{text:?}");
    }
    // every encoded text decodes back
    for text in ["primary:Download/a b (1).txt", "x\u{e4}y/\u{1F600}", "%41"] {
        assert_eq!(percent_decode(&android_encode(text)).as_deref(), Some(text));
    }
}

#[test]
fn a_failure_names_the_document_as_a_person_knows_it() {
    assert_eq!(
        readable_name("content://com.android.externalstorage.documents/tree/primary%3ADownload/document/primary%3ADownload%2Fsub%2Fa%20b.txt"),
        "a b.txt"
    );
    assert_eq!(readable_name("content://com.android.externalstorage.documents/tree/primary%3ADownload"), "Download");
    assert_eq!(readable_name("/storage/emulated/0/x"), "/storage/emulated/0/x", "not a document URI: shown as it is");
}

// ---- where the picker opens ----

#[test]
fn a_folder_of_the_shared_storage_is_where_the_picker_opens() {
    for (path, expected) in [
        ("/storage/emulated/0/Documents/AzProof", "content://com.android.externalstorage.documents/document/primary%3ADocuments%2FAzProof"),
        ("/sdcard/Download/", "content://com.android.externalstorage.documents/document/primary%3ADownload"),
        ("/storage/self/primary/Pictures/My Photos", "content://com.android.externalstorage.documents/document/primary%3APictures%2FMy%20Photos"),
    ] {
        assert_eq!(initial_uri_for(path).as_deref(), Some(expected), "{path}");
    }
}

#[test]
fn a_document_uri_is_passed_on_in_the_frameworks_form_and_nothing_else_is_guessed() {
    let tree = "content://com.android.externalstorage.documents/tree/primary%3ADownload";
    assert_eq!(
        initial_uri_for(tree).as_deref(),
        Some("content://com.android.externalstorage.documents/tree/primary%3ADownload/document/primary%3ADownload")
    );
    for none in ["", "/data/user/0/com.x/files", "/storage/emulated/0/", "/sdcard/../etc", "/storage/emulated/0/a//b", "relative/path", "https://x/y"] {
        assert_eq!(initial_uri_for(none), None, "{none:?}");
    }
}

#[test]
fn any_other_content_uri_is_a_file_that_can_be_read_and_written_where_the_grant_allows() {
    for uri in [
        "content://media/external/images/media/12",
        "content://com.azul.sharelab.docs/edit.md",
        "content://com.example.mail.fileprovider/attachments/Report%20Q3.pdf",
    ] {
        let parsed = Locator::parse(uri).unwrap_or_else(|| panic!("{uri}"));
        assert_eq!(parsed, Locator::Content { uri: uri.to_string() });
        assert_eq!(parsed.document_uri(), uri, "the URI the provider made is the one used");
        assert_eq!(parsed.tree_uri(), None, "no folder around it");
        assert_eq!(parsed.child("x"), None);
    }
    assert_eq!(readable_name("content://com.example.mail.fileprovider/attachments/Report%20Q3.pdf"), "Report Q3.pdf");
    assert_eq!(readable_name("content://media/external/images/media/12"), "12");
    // damaged URIs of the document forms are not taken for such files
    for damaged in ["content://x/tree/", "content://x/document/a/b", "content://x/tree/a/doc/b", "content://x/tree"] {
        assert_eq!(Locator::parse(damaged), None, "{damaged}");
    }
}
