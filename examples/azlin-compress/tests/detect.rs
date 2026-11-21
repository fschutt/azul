//! Detection: the kind of a file from its first bytes, and which kinds skip the trial.

use azlin_compress::{detect, Kind};

fn zip_with_first_member(name: &[u8], data: &[u8]) -> Vec<u8> {
    let mut zip = Vec::new();
    zip.extend_from_slice(b"PK\x03\x04");
    zip.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // version, flags, method 0, time, date
    zip.extend_from_slice(&[0; 4]); // crc
    zip.extend_from_slice(&(data.len() as u32).to_le_bytes());
    zip.extend_from_slice(&(data.len() as u32).to_le_bytes());
    zip.extend_from_slice(&(name.len() as u16).to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(name);
    zip.extend_from_slice(data);
    zip
}

#[test]
fn pdf_zip_and_office_files_are_not_skipped_but_tried() {
    let pdf = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n1 0 obj\n<< /Type /Catalog >>\nendobj\n";
    assert_eq!(detect(pdf, None), Kind::Pdf);
    assert!(!Kind::Pdf.is_dense());

    let docx = zip_with_first_member(b"[Content_Types].xml", b"<?xml version=\"1.0\"?><Types/>");
    assert_eq!(detect(&docx, None), Kind::Office);
    assert!(!Kind::Office.is_dense());

    let odt = zip_with_first_member(b"mimetype", b"application/vnd.oasis.opendocument.text");
    assert_eq!(detect(&odt, None), Kind::Office);

    let epub = zip_with_first_member(b"mimetype", b"application/epub+zip");
    assert_eq!(detect(&epub, None), Kind::Epub);
    assert!(!Kind::Epub.is_dense());

    let zip = zip_with_first_member(b"notes.txt", b"hello");
    assert_eq!(detect(&zip, None), Kind::Zip);
    assert!(!Kind::Zip.is_dense());
}

#[test]
fn a_pdf_with_junk_before_its_header_is_still_a_pdf() {
    let mut pdf = b"\r\n\x00junk".to_vec();
    pdf.extend_from_slice(b"%PDF-1.4\n");
    assert_eq!(detect(&pdf, None), Kind::Pdf);
}

#[test]
fn video_audio_modern_pictures_and_archives_are_skipped_early() {
    let cases: &[(&[u8], Kind)] = &[
        (b"\x00\x00\x00\x18ftypisom\x00\x00\x02\x00isomiso2", Kind::Video),
        (b"\x00\x00\x00\x14ftypqt  \x00\x00\x00\x00qt  ", Kind::Video),
        (b"\x00\x00\x00\x18ftypheic\x00\x00\x00\x00mif1heic", Kind::Heif),
        (b"\x00\x00\x00\x1CftypM4A \x00\x00\x00\x00M4A mp42", Kind::Audio),
        (b"\x00\x00\x00\x1Cftypavif\x00\x00\x00\x00avifmif1", Kind::Heif),
        (b"\x1A\x45\xDF\xA3\x9F\x42\x86\x81\x01", Kind::Video),
        (b"RIFF\x24\x00\x00\x00AVI LIST", Kind::Video),
        (b"RIFF\x24\x00\x00\x00WEBPVP8 ", Kind::WebP),
        (b"ID3\x04\x00\x00\x00\x00\x00\x00", Kind::Audio),
        (b"\xFF\xFB\x90\x64\x00\x00", Kind::Audio),
        (b"\xFF\xF1\x50\x80\x00\x1F", Kind::Audio),
        (b"OggS\x00\x02\x00\x00", Kind::Audio),
        (b"fLaC\x00\x00\x00\x22", Kind::Audio),
        (b"\x28\xB5\x2F\xFD\x24\x00", Kind::Archive),
        (b"\xFD7zXZ\x00\x00\x04", Kind::Archive),
        (b"7z\xBC\xAF\x27\x1C\x00\x04", Kind::Archive),
        (b"Rar!\x1A\x07\x01\x00", Kind::Archive),
        (b"BZh91AY&SY", Kind::Archive),
        (b"GIF89a\x01\x00\x01\x00", Kind::Gif),
        (b"\x89PNG\r\n\x1A\n\x00\x00\x00\x0DIHDR", Kind::Png),
        (b"\x1F\x8B\x08\x00\x00\x00\x00\x00", Kind::Gzip),
        (b"wOF2\x00\x01\x00\x00", Kind::Font),
    ];
    for (start, kind) in cases {
        assert_eq!(detect(start, None), *kind, "{start:?}");
        assert!(kind.is_dense(), "{kind:?} is skipped without a trial");
    }
}

#[test]
fn a_jpeg_is_a_lepton_candidate_and_so_are_the_containers_that_hold_jpegs() {
    assert_eq!(detect(b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00", None), Kind::Jpeg);
    for kind in [Kind::Jpeg, Kind::Pdf, Kind::Zip, Kind::Office, Kind::Epub, Kind::Tiff] {
        assert!(kind.may_hold_jpegs(), "{kind:?}");
    }
    for kind in [Kind::Video, Kind::Heif, Kind::Json, Kind::Binary, Kind::Png] {
        assert!(!kind.may_hold_jpegs(), "{kind:?}");
    }
    assert_eq!(detect(b"II*\x00\x08\x00\x00\x00", None), Kind::Tiff);
    assert_eq!(detect(b"\x00\x00\x00\x18ftypcrx \x00\x00\x00\x01crx isom", None), Kind::Tiff);
}

#[test]
fn text_is_told_apart_by_its_content() {
    assert_eq!(detect(b"  {\"name\": \"value\", \"list\": [1, 2]}\n", None), Kind::Json);
    assert_eq!(detect(b"[{\"a\":1},{\"a\":2}]", None), Kind::Json);
    assert_eq!(detect(b"<?xml version=\"1.0\"?>\n<root/>", None), Kind::Markup);
    assert_eq!(detect(b"<!DOCTYPE html>\n<html><body></body></html>", None), Kind::Markup);
    assert_eq!(
        detect(b"date,amount,currency\n2026-01-01,12.50,EUR\n2026-01-02,7.00,EUR\n2026-01-03,1.00,USD\n", None),
        Kind::Csv
    );
    assert_eq!(
        detect(b"name;city\nAnna;Berlin\nBen;Wien\nCem;Zurich\n", None),
        Kind::Csv
    );
    assert_eq!(
        detect(b"use std::io;\n\nfn main() {\n    let x = 1;\n    return;\n}\npub fn other() {}\n", None),
        Kind::Code
    );
    assert_eq!(detect(b"#!/bin/sh\necho hi\n", None), Kind::Code);
    assert_eq!(
        detect(b"It was a bright cold day in April, and the clocks were striking thirteen.\n", None),
        Kind::Text
    );
    assert_eq!(detect("Grüße aus Köln, schöne Straße.\n".as_bytes(), None), Kind::Text);
    assert_eq!(detect(b"\xFF\xFEH\x00i\x00", None), Kind::Text);
    assert!(Kind::Json.is_text() && Kind::Csv.is_text() && Kind::Code.is_text());
}

#[test]
fn a_text_cut_in_the_middle_of_a_character_is_still_text() {
    let mut text = "Grüße ".repeat(10).into_bytes();
    text.push(0xC3); // the first byte of a two-byte character, its second cut off
    assert_eq!(detect(&text, None), Kind::Text);
}

#[test]
fn the_extension_only_breaks_ties() {
    // The content wins over the name.
    assert_eq!(detect(b"\xFF\xD8\xFF\xE1\x00\x10Exif", Some("txt")), Kind::Jpeg);
    assert_eq!(detect(b"%PDF-1.5\n", Some("docx")), Kind::Pdf);
    // Plain text: the name says which text.
    assert_eq!(detect(b"some words without structure\n", Some("rs")), Kind::Code);
    assert_eq!(detect(b"some words without structure\n", Some("JSON")), Kind::Json);
    // Binary content is not text whatever its name.
    assert_eq!(detect(b"\x00\x01\x02\x03\xFE\xFD", Some("json")), Kind::Binary);
    // A dense extension on unrecognised binary saves the trial (a raw AAC stream, say).
    assert_eq!(detect(b"\x00\x01\x02\x03\xFE\xFD", Some("m4a")), Kind::Audio);
}

#[test]
fn nothing_recognised_is_binary_and_gets_the_trial() {
    assert_eq!(detect(b"\x00\x00\x00\x00\x00\x00\x00\x00", None), Kind::Binary);
    assert_eq!(detect(b"", None), Kind::Binary);
    assert_eq!(detect(b"SQLite format 3\x00\x10\x00", None), Kind::Sqlite);
    assert!(!Kind::Binary.is_dense() && !Kind::Sqlite.is_dense() && !Kind::Wav.is_dense());
}
