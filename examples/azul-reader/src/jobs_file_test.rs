//! The file of a book, read back for the copy the user saves.

use azul_storage::{testing::TempDir, LocalDrive};

use super::*;

#[test]
fn a_books_file_is_read_back_exactly_as_it_was_imported() {
    let dir = TempDir::new("read-file");
    let drive = LocalDrive::new(&dir.0);
    let bytes = b"CHAPTER I\n\nHello, reader.".to_vec();
    let (entry, _) = import(&drive, "t1", "hello.txt", &bytes, 7).expect("imported");
    assert_eq!(read_file(&drive, "t1", entry.info.format), Ok(bytes));
}

#[test]
fn a_book_that_is_gone_says_so_instead_of_giving_an_empty_file() {
    let dir = TempDir::new("read-file-gone");
    let drive = LocalDrive::new(&dir.0);
    let answer = read_file(&drive, "nothing", Format::Epub);
    assert!(answer.is_err(), "{answer:?}");
}

#[test]
fn every_format_tells_the_save_dialog_its_mime_type() {
    assert_eq!(Format::Epub.mime(), "application/epub+zip");
    assert_eq!(Format::Text.mime(), "text/plain");
    assert_eq!(Format::Html.mime(), "text/html");
}
