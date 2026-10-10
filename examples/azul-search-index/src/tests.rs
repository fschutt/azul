//! The index end to end on temporary folders: the text of each kind, a build, a query (words,
//! a half typed word, a folder), an update that reads only what changed and drops what is gone,
//! a cancel, a second open, a folder of another format.

use std::{
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use azul_search::Filters;
use zip::write::SimpleFileOptions;

use crate::{extract, kind_of, DriveIndex, Extractors, Kind, UpdateProgress};

/// A fresh folder under the system's temporary folder, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        static MADE: AtomicUsize = AtomicUsize::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!(
            "azul-search-index-{tag}-{}-{nanos}-{}",
            std::process::id(),
            MADE.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&dir).expect("a temporary folder");
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, path: &str, bytes: &[u8]) {
        let file = self.0.join(path);
        fs::create_dir_all(file.parent().expect("a folder")).expect("the file's folder");
        fs::write(file, bytes).expect("the file");
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A zip of `parts` (entry name, its text): an office document.
fn zipped(parts: &[(&str, &str)]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, text) in parts {
        writer
            .start_file(*name, SimpleFileOptions::default())
            .expect("an entry");
        writer.write_all(text.as_bytes()).expect("its bytes");
    }
    writer.finish().expect("the zip").into_inner()
}

const DOCX: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:r><w:t>Quarterly</w:t></w:r><w:r><w:t xml:space="preserve"> rep</w:t></w:r><w:r><w:t>ort</w:t></w:r></w:p>
<w:p><w:r><w:t>Second paragraph</w:t></w:r></w:p></w:body></w:document>"#;

#[test]
fn a_files_kind_is_known_by_its_name() {
    assert_eq!(kind_of("notes.txt"), Some(Kind::Text));
    assert_eq!(kind_of("Makefile"), Some(Kind::Text));
    assert_eq!(kind_of("Report.DOCX"), Some(Kind::Office));
    assert_eq!(kind_of("book.epub"), Some(Kind::Office));
    assert_eq!(kind_of("0001.eml"), Some(Kind::Mail));
    assert_eq!(kind_of("scan.pdf"), Some(Kind::Pdf));
    assert_eq!(kind_of("photo.JPG"), None, "a picture holds no text");
    assert_eq!(kind_of("movie.mp4"), None);
}

#[test]
fn plain_text_is_read_and_binary_bytes_are_not() {
    let none = Extractors::default();
    assert_eq!(
        extract("a.txt", b"\xEF\xBB\xBFhello there", &none).as_deref(),
        Some("hello there"),
        "without the byte-order mark"
    );
    assert_eq!(extract("a.bin.txt", b"MZ\0\0\0", &none), None);
    let mut utf16 = vec![0xFF, 0xFE];
    for unit in "wide text".encode_utf16() {
        utf16.extend_from_slice(&unit.to_le_bytes());
    }
    assert_eq!(extract("w.txt", &utf16, &none).as_deref(), Some("wide text"));
    assert_eq!(extract("empty.txt", b"  \n", &none), None, "no text");
}

#[test]
fn an_office_documents_text_is_a_line_per_paragraph() {
    let none = Extractors::default();
    let docx = zipped(&[
        ("[Content_Types].xml", "<Types/>"),
        ("word/document.xml", DOCX),
        ("word/styles.xml", "<w:styles xmlns:w=\"x\"><w:t>Not text</w:t></w:styles>"),
    ]);
    let text = extract("Report.docx", &docx, &none).expect("its text");
    assert!(text.contains("Quarterly report"), "runs join: {text:?}");
    assert!(text.contains("\nSecond paragraph"), "a line per paragraph: {text:?}");
    assert!(!text.contains("Not text"), "the styles are no text");

    let xlsx = zipped(&[(
        "xl/sharedStrings.xml",
        "<sst><si><t>Revenue</t></si><si><t>Costs</t></si></sst>",
    )]);
    let text = extract("Budget.xlsx", &xlsx, &none).expect("its text");
    assert!(text.contains("Revenue") && text.contains("Costs"), "{text:?}");

    let pptx = zipped(&[
        ("ppt/slides/slide10.xml", "<p:sld xmlns:p=\"p\" xmlns:a=\"a\"><a:p><a:t>Ten</a:t></a:p></p:sld>"),
        ("ppt/slides/slide2.xml", "<p:sld xmlns:p=\"p\" xmlns:a=\"a\"><a:p><a:t>Two</a:t></a:p></p:sld>"),
    ]);
    let text = extract("Deck.pptx", &pptx, &none).expect("its text");
    assert!(text.find("Two") < text.find("Ten"), "slides in their order: {text:?}");

    let odt = zipped(&[(
        "content.xml",
        "<office:document-content xmlns:office=\"o\" xmlns:text=\"t\"><text:p>Open letter</text:p></office:document-content>",
    )]);
    assert!(extract("letter.odt", &odt, &none).is_some_and(|t| t.contains("Open letter")));
    assert_eq!(extract("broken.docx", b"not a zip", &none), None);
}

#[test]
fn a_mails_subject_people_and_body_are_its_text() {
    let none = Extractors::default();
    let eml = b"From: Ada Lovelace <ada@example.org>\r\nTo: Bob <bob@example.org>\r\n\
Subject: Engine notes\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nThe analytical engine weaves.\r\n";
    let text = extract("0001.eml", eml, &none).expect("its text");
    assert!(text.contains("Engine notes"), "{text:?}");
    assert!(text.contains("Ada Lovelace") && text.contains("bob@example.org"), "{text:?}");
    assert!(text.contains("analytical engine"), "{text:?}");
}

#[test]
fn a_pdf_is_read_by_the_apps_reader_or_not_at_all() {
    assert_eq!(extract("scan.pdf", b"%PDF-1.7", &Extractors::default()), None);
    let reader = Extractors {
        pdf: Some(Arc::new(|bytes: &[u8]| {
            bytes.starts_with(b"%PDF").then(|| String::from("Invoice 4711"))
        })),
    };
    assert_eq!(extract("scan.pdf", b"%PDF-1.7", &reader).as_deref(), Some("Invoice 4711"));
}

/// A drive with a text, an office document, a mail and a picture; its index in its own folder.
fn drive() -> (TempDir, TempDir) {
    let drive = TempDir::new("drive");
    drive.write("notes/plan.txt", b"the zebra-quartz plan\nsecond line\n");
    drive.write("docs/Report.docx", &zipped(&[("word/document.xml", DOCX)]));
    drive.write(
        "mail/0001.eml",
        b"Subject: Lunch\r\nFrom: ada@example.org\r\n\r\nPasta at noon?\r\n",
    );
    drive.write("photo.jpg", b"\xFF\xD8\xFFquarterly");
    (drive, TempDir::new("index"))
}

fn update(index: &DriveIndex, drive: &TempDir) -> crate::UpdateSummary {
    let cancel = AtomicBool::new(false);
    index
        .update(drive.path(), &Filters::default(), &Extractors::default(), &cancel, &mut |_| {})
        .expect("the update")
}

#[test]
fn an_index_finds_words_half_typed_words_and_phrases_below_a_folder() {
    let (drive, dir) = drive();
    let index = DriveIndex::open(dir.path()).expect("the index");
    let summary = update(&index, &drive);
    assert_eq!((summary.listed, summary.indexed, summary.without_text), (4, 3, 1));
    let status = index.status();
    assert_eq!((status.files, status.documents), (4, 3));
    assert!(status.updated.is_some());
    let find = |text: &str, under: &str| index.query(text, under, 100).expect("the query");
    assert_eq!(find("zebra", ""), vec!["notes/plan.txt"]);
    assert_eq!(find("QUARTZ", ""), vec!["notes/plan.txt"], "without case");
    assert_eq!(find("quarterly rep", ""), vec!["docs/Report.docx"], "a half typed last word");
    assert_eq!(find("pasta", ""), vec!["mail/0001.eml"]);
    assert!(find("quarterly", "notes/").is_empty(), "below a folder only");
    assert_eq!(find("zeb", "notes/"), vec!["notes/plan.txt"]);
    assert!(find("report quarterly", "").is_empty(), "words in a row");
    assert!(find("  --  ", "").is_empty(), "no words, nothing");
}

#[test]
fn an_update_reads_what_changed_and_drops_what_is_gone() {
    let (drive, dir) = drive();
    let index = DriveIndex::open(dir.path()).expect("the index");
    update(&index, &drive);
    let again = update(&index, &drive);
    assert_eq!((again.indexed, again.unchanged, again.removed), (0, 4, 0), "nothing read again");

    drive.write("notes/plan.txt", b"the giraffe plan, longer now\n");
    let later = UNIX_EPOCH + Duration::from_secs(2_000_000_000);
    fs::File::options()
        .write(true)
        .open(drive.path().join("notes/plan.txt"))
        .and_then(|f| f.set_modified(later))
        .expect("a new date");
    fs::remove_file(drive.path().join("mail/0001.eml")).expect("a mail gone");
    let changed = update(&index, &drive);
    assert_eq!((changed.indexed, changed.removed, changed.unchanged), (1, 1, 2));
    assert!(index.query("zebra", "", 10).expect("q").is_empty(), "the old words are gone");
    assert_eq!(index.query("giraffe", "", 10).expect("q"), vec!["notes/plan.txt"]);
    assert!(index.query("pasta", "", 10).expect("q").is_empty(), "the gone mail is gone");

    // Opened again (another job, the next start), it knows what it holds.
    drop(index);
    let reopened = DriveIndex::open(dir.path()).expect("the index again");
    assert_eq!(reopened.query("giraffe", "", 10).expect("q"), vec!["notes/plan.txt"]);
    assert_eq!(update(&reopened, &drive).indexed, 0);
}

#[test]
fn a_cancelled_update_keeps_what_it_committed_and_says_so() {
    let (drive, dir) = drive();
    let index = DriveIndex::open(dir.path()).expect("the index");
    let cancel = AtomicBool::new(true);
    let mut heard: Vec<UpdateProgress> = Vec::new();
    let summary = index
        .update(drive.path(), &Filters::default(), &Extractors::default(), &cancel, &mut |p| {
            heard.push(p)
        })
        .expect("the update");
    assert!(summary.cancelled);
    assert_eq!(summary.indexed, 0);
    assert_eq!(index.status().updated, None, "not a finished update");
    let finished = update(&index, &drive);
    assert_eq!(finished.indexed, 3, "the next update does the work");
}

#[test]
fn a_folder_of_another_format_is_made_again() {
    let (drive, dir) = drive();
    dir.write("azul-search-index.format", b"azul-search-index 0");
    dir.write("meta.json", b"{ not tantivy }");
    let index = DriveIndex::open(dir.path()).expect("the index, made again");
    assert_eq!(update(&index, &drive).indexed, 3);
    assert_eq!(index.query("pasta", "", 10).expect("q"), vec!["mail/0001.eml"]);
}

/// Every file the walk finds below the drive's folder.
fn listed(drive: &TempDir) -> Vec<azul_search::FileEntry> {
    let mut files = Vec::new();
    azul_search::list_files(drive.path(), &Filters::default(), &AtomicBool::new(false), &mut |f| {
        files.push(f)
    })
    .expect("listed");
    files
}

/// What the index has not read as it is now - a new file, one whose size or date changed - is
/// what a search still reads itself; the index answers for the rest.
#[test]
fn the_files_the_index_has_not_read_as_they_are_are_named() {
    let (drive, dir) = drive();
    let index = DriveIndex::open(dir.path()).expect("the index");
    assert_eq!(index.unread(&listed(&drive)).len(), 4, "nothing read yet");
    update(&index, &drive);
    assert!(index.unread(&listed(&drive)).is_empty(), "every file read as it is");
    drive.write("notes/new.txt", b"fresh\n");
    drive.write("notes/plan.txt", b"the zebra-quartz plan, and a longer line\n");
    let files = listed(&drive);
    let mut unread: Vec<&str> = index
        .unread(&files)
        .iter()
        .map(|f| f.path.as_str())
        .collect();
    unread.sort_unstable();
    assert_eq!(unread, vec!["notes/new.txt", "notes/plan.txt"]);
}

/// A file's text as the index reads it - an office document's paragraphs, a mail's text -, for
/// the line a result of a document shows; none for a picture or a file that is not there.
#[test]
fn a_files_text_is_read_as_the_index_reads_it() {
    let (drive, _dir) = drive();
    let none = Extractors::default();
    let text = crate::document_text(drive.path(), "docs/Report.docx", &none).expect("its text");
    assert!(text.contains("Quarterly report"), "{text:?}");
    let mail = crate::document_text(drive.path(), "mail/0001.eml", &none);
    assert!(mail.is_some_and(|t| t.contains("Pasta at noon")));
    assert_eq!(crate::document_text(drive.path(), "photo.jpg", &none), None);
    assert_eq!(crate::document_text(drive.path(), "gone.txt", &none), None);
}

/// A file's text read from wherever it is on this computer (a drive's local copy, named apart
/// from its key): its kind by its own name.
#[test]
fn a_files_text_is_read_from_any_file_on_this_computer() {
    let (drive, _dir) = drive();
    let none = Extractors::default();
    let text = crate::file_text(&drive.path().join("docs/Report.docx"), &none).expect("its text");
    assert!(text.contains("Quarterly report"), "{text:?}");
    let plan = crate::file_text(&drive.path().join("notes/plan.txt"), &none);
    assert!(plan.is_some_and(|t| t.contains("zebra-quartz")));
    assert_eq!(crate::file_text(&drive.path().join("photo.jpg"), &none), None);
    assert_eq!(crate::file_text(&drive.path().join("gone.txt"), &none), None);
}

/// An index reads the files a source names (a cloud drive's listing, an encrypted drive's index)
/// through the source's reader, asked for the bytes the file's kind needs: a file the reader
/// has nothing for is recorded as read without text, one it cannot read now (offline) is not -
/// the next update asks for it again and reads nothing else.
#[test]
fn an_index_reads_a_sources_files_through_its_reader_and_asks_again_for_what_failed() {
    use std::cell::{Cell, RefCell};

    let dir = TempDir::new("index-source");
    let index = DriveIndex::open(dir.path()).expect("the index");
    let file = |path: &str, size: u64| azul_search::FileEntry {
        path: path.to_string(),
        size,
        modified: Some(1_700_000_000),
    };
    let files = vec![
        file("notes/a.txt", 11),
        file("notes/b.txt", 9),
        file("big.txt", 1 << 40),
        file("Report.docx", 500),
    ];
    let offline = Cell::new(true);
    let asked: RefCell<Vec<(String, Kind, u64)>> = RefCell::new(Vec::new());
    let mut read = |f: &azul_search::FileEntry, kind: Kind, limit: u64| {
        asked.borrow_mut().push((f.path.clone(), kind, limit));
        match f.path.as_str() {
            "notes/a.txt" => Ok(Some(b"alpha words".to_vec())),
            "notes/b.txt" if offline.get() => Err(String::from("offline")),
            "notes/b.txt" => Ok(Some(b"beta word".to_vec())),
            "Report.docx" => Ok(Some(zipped(&[("word/document.xml", DOCX)]))),
            _ => Ok(None),
        }
    };
    let cancel = AtomicBool::new(false);
    let none = Extractors::default();
    let first = index
        .update_files(&files, &mut read, &none, &cancel, &mut |_| {})
        .expect("the first update");
    assert_eq!((first.indexed, first.without_text, first.failed), (2, 1, 1));
    assert_eq!(index.status().files, 3, "the file that failed is not recorded");
    assert_eq!(index.query("alpha", "", 10).expect("q"), vec!["notes/a.txt"]);
    assert_eq!(index.query("quarterly", "", 10).expect("q"), vec!["Report.docx"]);
    assert!(asked
        .borrow()
        .contains(&(String::from("notes/a.txt"), Kind::Text, crate::MAX_TEXT_BYTES as u64)));
    assert!(asked.borrow().contains(&(
        String::from("Report.docx"),
        Kind::Office,
        crate::MAX_DOCUMENT_BYTES
    )));

    offline.set(false);
    asked.borrow_mut().clear();
    let second = index
        .update_files(&files, &mut read, &none, &cancel, &mut |_| {})
        .expect("the second update");
    assert_eq!((second.indexed, second.failed, second.unchanged), (1, 0, 3));
    let again: Vec<String> = asked.borrow().iter().map(|(p, _, _)| p.clone()).collect();
    assert_eq!(again, vec!["notes/b.txt"], "only what failed is asked for again");
    assert_eq!(index.query("beta", "", 10).expect("q"), vec!["notes/b.txt"]);
}
