//! The MIME table: the extension of a name, the type of an extension and back, the common type of a list, what a platform
//! accepts as a type.

use super::*;

#[test]
fn the_extension_of_a_name_is_its_last_dot_in_lower_case() {
    assert_eq!(extension_of("report.PDF").as_deref(), Some("pdf"));
    assert_eq!(extension_of("archive.tar.gz").as_deref(), Some("gz"));
    assert_eq!(extension_of("/a.b/c/photo.JPeG").as_deref(), Some("jpeg"));
    assert_eq!(extension_of("C:\\dir.x\\notes.md").as_deref(), Some("md"));
}

#[test]
fn a_name_without_an_extension_has_none() {
    for name in ["Makefile", ".profile", "trailing.", "", "dir.d/noext", "a.b c", "x.çsv"] {
        assert_eq!(extension_of(name), None, "{name:?}");
    }
    // an absurdly long "extension" is part of the name, not a type
    assert_eq!(extension_of(&format!("a.{}", "x".repeat(40))), None);
}

#[test]
fn an_extension_has_a_type_with_or_without_its_dot_in_any_case() {
    assert_eq!(for_extension("md"), Some("text/markdown"));
    assert_eq!(for_extension(".MD"), Some("text/markdown"));
    assert_eq!(for_extension("*.pdf"), Some("application/pdf"));
    assert_eq!(for_extension("vcf"), Some("text/vcard"));
    assert_eq!(for_extension("ics"), Some("text/calendar"));
    assert_eq!(for_extension("gpx"), Some("application/gpx+xml"));
    assert_eq!(for_extension("epub"), Some("application/epub+zip"));
    assert_eq!(for_extension("nonsense"), None);
}

#[test]
fn a_file_name_is_typed_by_its_extension_else_it_is_a_stream_of_bytes() {
    assert_eq!(for_file_name("a/b/Notes.MD"), "text/markdown");
    assert_eq!(for_file_name("photo.jpeg"), "image/jpeg");
    assert_eq!(for_file_name("Makefile"), OCTET_STREAM);
    assert_eq!(for_file_name("data.unknownext"), OCTET_STREAM);
}

#[test]
fn a_type_has_the_extension_the_table_lists_first() {
    assert_eq!(extension_for("text/plain"), Some("txt"));
    assert_eq!(extension_for("image/jpeg"), Some("jpg"));
    assert_eq!(extension_for("TEXT/Markdown; charset=utf-8"), Some("md"));
    assert_eq!(extension_for("application/x-nothing"), None);
}

#[test]
fn the_table_round_trips_every_type_through_its_first_extension() {
    for (extension, mime) in TABLE {
        let back = extension_for(mime).unwrap_or_else(|| panic!("{mime} has no extension"));
        assert_eq!(for_extension(back), Some(*mime), "{extension} -> {mime} -> {back}");
    }
}

#[test]
fn the_table_has_no_extension_twice_and_every_entry_is_a_valid_type() {
    let mut seen = std::collections::BTreeSet::new();
    for (extension, mime) in TABLE {
        assert!(seen.insert(*extension), "{extension} twice");
        assert!(is_valid(mime), "{mime}");
        assert_eq!(*extension, extension.to_ascii_lowercase());
        assert!(!extension.starts_with('.') && !extension.is_empty());
    }
}

#[test]
fn a_type_is_valid_when_a_platform_takes_it_in_an_intent() {
    for good in ["text/plain", "image/*", "*/*", "application/vnd.openxmlformats-officedocument.wordprocessingml.document", "video/x-matroska", "application/gpx+xml"] {
        assert!(is_valid(good), "{good}");
    }
    for bad in ["", "text", "/plain", "text/", "text/plain/extra", "te xt/plain", "text/pla\0in", "*/plain", "text/plain; charset=utf-8", "image/\u{e9}"] {
        assert!(!is_valid(bad), "{bad:?}");
    }
    assert!(!is_valid(&format!("a/{}", "b".repeat(200))));
}

#[test]
fn the_common_type_of_several_files_is_what_an_intent_of_them_needs() {
    assert_eq!(common_type(&["image/png"]), "image/png");
    assert_eq!(common_type(&["image/png", "image/png"]), "image/png");
    assert_eq!(common_type(&["image/png", "image/jpeg"]), "image/*");
    assert_eq!(common_type(&["image/png", "image/jpeg", "image/gif"]), "image/*");
    assert_eq!(common_type(&["image/png", "application/pdf"]), "*/*");
    assert_eq!(common_type(&["IMAGE/PNG; q=1", "image/png"]), "image/png");
    assert_eq!(common_type(&[]), "*/*");
    // a nonsense entry is a stream of bytes, which only matches itself and the wildcard
    assert_eq!(common_type(&["not a type"]), OCTET_STREAM);
    assert_eq!(common_type(&["image/png", "not a type"]), "*/*");
    // once it is the wildcard it stays one
    assert_eq!(common_type(&["image/png", "video/mp4", "image/png"]), "*/*");
}
