//! AZL1 version 2: a JPEG, an Office file, a PNG or a PDF goes through its whole-file transform
//! before it is sealed (compression is on by default), reads back bit-exact whole, in ranges
//! and through the streaming reader, and is refused when its transform block changes.

use std::io::{Cursor, Read};

use super::compress_inputs::{self as inputs, Rng};
use crate::crypto::{
    azl1::{
        decrypt, encrypt, encrypt_as, encrypt_stream, encrypt_stream_as, FileHint, ObjectSummary,
        OpenObject, TransformId, WriteOptions, HEADER_LEN, VERSION, VERSION_TRANSFORMED,
    },
    codec::Compression,
    CryptoError, DriveKey, FileKey, ObjectId,
};

const BASELINE_JPEG: &[u8] = include_bytes!("../../../azlin-compress/tests/fixtures/baseline.jpg");
const PROGRESSIVE_JPEG: &[u8] =
    include_bytes!("../../../azlin-compress/tests/fixtures/progressive.jpg");
const EMBEDDED_JPEG: &[u8] = include_bytes!("../../../azlin-compress/tests/fixtures/embedded.jpg");

fn never() -> WriteOptions {
    WriteOptions {
        compression: Compression::Never,
        ..WriteOptions::default()
    }
}

fn file_key_of(drive: &DriveKey, summary: &ObjectSummary) -> FileKey {
    drive
        .unwrap_file_key(&summary.wrapped_key, &summary.object_id)
        .unwrap()
}

fn version_of(object: &[u8]) -> u16 {
    u16::from_le_bytes([object[4], object[5]])
}

/// A DOCX whose document repeats beyond deflate's window, with a photo inside: what Word
/// writes, small.
fn docx() -> Vec<u8> {
    let mut rng = Rng::new(45);
    let body = String::from_utf8(inputs::far_repeats(&mut rng, 40 * 1024, 4)).unwrap();
    let document =
        format!("<?xml version=\"1.0\"?><w:document><w:body>{body}</w:body></w:document>");
    inputs::deflated_zip(
        &[
            ("[Content_Types].xml", b"<?xml version=\"1.0\"?><Types/>"),
            ("word/document.xml", document.as_bytes()),
            ("word/media/image1.jpeg", EMBEDDED_JPEG),
        ],
        6,
    )
}

/// A PNG whose rows repeat a tile larger than deflate's window.
fn png() -> Vec<u8> {
    inputs::png(&mut Rng::new(43), 256, 128, 48, 8192)
}

/// A PDF with a deflated content stream and a photo.
fn pdf() -> Vec<u8> {
    let content = inputs::far_repeats(&mut Rng::new(46), 40 * 1024, 4);
    let flate = inputs::zlib(&content, 6);
    inputs::pdf(
        &[
            ("/Filter /FlateDecode", &flate),
            (
                "/Type /XObject /Subtype /Image /Width 560 /Height 420 /Filter /DCTDecode",
                EMBEDDED_JPEG,
            ),
        ],
        50,
    )
}

/// `plain` written with compression on (as `name`), checked as a version 2 object of
/// `transform`, read back every way.
fn transformed_round_trip(name: &str, plain: &[u8], transform: TransformId) -> Vec<u8> {
    let drive = DriveKey::generate().unwrap();
    let id = ObjectId::generate().unwrap();
    let hint = FileHint::for_path(name, None);
    let (object, summary) = encrypt_as(plain, id, &drive, &WriteOptions::default(), &hint).unwrap();
    assert_eq!(summary.transform, Some(transform), "{name}");
    assert_eq!(version_of(&object), VERSION_TRANSFORMED, "{name}");
    assert!(summary.compressed, "{name}");
    // The index, the quota and the final check see the original file.
    assert_eq!(summary.plaintext_size, plain.len() as u64, "{name}");
    assert_eq!(summary.blake3, *blake3::hash(plain).as_bytes(), "{name}");
    assert_eq!(summary.object_len, object.len() as u64, "{name}");
    let (stored, _) = encrypt(plain, ObjectId::generate().unwrap(), &drive, &never()).unwrap();
    assert!(
        object.len() < stored.len(),
        "{name}: {} bytes transformed, {} stored",
        object.len(),
        stored.len()
    );

    let key = file_key_of(&drive, &summary);
    assert_eq!(decrypt(&object, &id, &key).unwrap(), plain, "{name}");
    let opened = OpenObject::open(&object[..], &id, &key).unwrap();
    assert_eq!(opened.transform(), Some(transform), "{name}");
    assert_eq!(opened.plaintext_size(), plain.len() as u64);
    assert_eq!(opened.read_all(&object[..]).unwrap(), plain, "{name}");
    let len = plain.len() as u64;
    for (start, end) in [(0, 99), (len / 2 - 7, len / 2 + 7), (len - 100, len + 50), (len - 1, len - 1)] {
        let got = opened.read_range(&object[..], start, end).unwrap();
        let end = end.min(len - 1);
        assert_eq!(got, &plain[start as usize..=end as usize], "{name}: {start}..={end}");
    }
    assert!(opened.read_range(&object[..], len, len + 10).unwrap().is_empty());
    for piece in [1usize, 777, 1 << 20] {
        let opened = OpenObject::open_with_drive_key(&object[..], &id, &drive).unwrap();
        let mut reader = opened.into_reader(&object[..]);
        let mut back = Vec::new();
        let mut buffer = vec![0u8; piece];
        loop {
            let n = reader.read(&mut buffer).unwrap();
            if n == 0 {
                break;
            }
            back.extend_from_slice(&buffer[..n]);
        }
        assert_eq!(back, plain, "{name}: reads of {piece}");
    }
    object
}

#[test]
fn a_jpeg_is_stored_through_lepton_and_reads_back_bit_exact_every_way() {
    transformed_round_trip("photos/IMG_0001.jpg", BASELINE_JPEG, TransformId::Jpeg);
    transformed_round_trip("photos/progressive.JPEG", PROGRESSIVE_JPEG, TransformId::Jpeg);
}

#[test]
fn a_docx_a_png_and_a_pdf_are_stored_as_their_rebuilt_deflate_streams() {
    transformed_round_trip("letters/offer.docx", &docx(), TransformId::Containers);
    transformed_round_trip("scans/page.png", &png(), TransformId::Containers);
    transformed_round_trip("invoices/2026-10.pdf", &pdf(), TransformId::Containers);
}

#[test]
fn a_streamed_jpeg_of_unknown_length_is_transformed_too() {
    let drive = DriveKey::generate().unwrap();
    let id = ObjectId::generate().unwrap();
    let mut sink = Cursor::new(Vec::new());
    let mut source = BASELINE_JPEG;
    let summary = encrypt_stream(&mut source, &mut sink, id, &drive, &WriteOptions::default())
        .unwrap();
    assert_eq!(summary.transform, Some(TransformId::Jpeg));
    let object = sink.into_inner();
    assert_eq!(object.len() as u64, summary.object_len);
    assert_eq!(
        decrypt(&object, &id, &file_key_of(&drive, &summary)).unwrap(),
        BASELINE_JPEG
    );
}

#[test]
fn a_stream_said_to_be_larger_than_the_transforms_take_streams_as_version_1() {
    let drive = DriveKey::generate().unwrap();
    let id = ObjectId::generate().unwrap();
    let mut sink = Cursor::new(Vec::new());
    let mut source = BASELINE_JPEG;
    let hint = FileHint {
        extension: Some(String::from("jpg")),
        len: Some(65 << 20),
    };
    let summary = encrypt_stream_as(
        &mut source,
        &mut sink,
        id,
        &drive,
        &WriteOptions::default(),
        &hint,
    )
    .unwrap();
    assert_eq!(summary.transform, None);
    let object = sink.into_inner();
    assert_eq!(version_of(&object), VERSION);
    assert_eq!(
        decrypt(&object, &id, &file_key_of(&drive, &summary)).unwrap(),
        BASELINE_JPEG
    );
}

#[test]
fn without_compression_a_jpeg_stays_version_1_and_stored() {
    let drive = DriveKey::generate().unwrap();
    let id = ObjectId::generate().unwrap();
    let (object, summary) = encrypt(BASELINE_JPEG, id, &drive, &never()).unwrap();
    assert_eq!(summary.transform, None);
    assert!(!summary.compressed);
    assert_eq!(version_of(&object), VERSION);
    assert_eq!(decrypt(&object, &id, &file_key_of(&drive, &summary)).unwrap(), BASELINE_JPEG);
}

#[test]
fn a_changed_transform_block_or_version_is_refused() {
    let drive = DriveKey::generate().unwrap();
    let id = ObjectId::generate().unwrap();
    let (object, summary) = encrypt(BASELINE_JPEG, id, &drive, &WriteOptions::default()).unwrap();
    assert_eq!(summary.transform, Some(TransformId::Jpeg));
    let key = file_key_of(&drive, &summary);

    // The sealed block (bytes 324..388) or its nonce (300..324) changed.
    for at in [301, 330, 387] {
        let mut changed = object.clone();
        changed[at] ^= 1;
        assert!(
            matches!(
                decrypt(&changed, &id, &key),
                Err(CryptoError::Damaged(why)) if why.contains("transform block")
            ),
            "byte {at}"
        );
    }
    // A version 2 object read as version 1: its block is not zero, and the context differs.
    let mut older = object.clone();
    older[4..6].copy_from_slice(&VERSION.to_le_bytes());
    assert!(matches!(decrypt(&older, &id, &key), Err(CryptoError::Damaged(_))));
    // A version 1 object read as version 2: the info does not authenticate under it.
    let (plain_object, plain_summary) = encrypt(b"plain text", ObjectId::generate().unwrap(), &drive, &WriteOptions::default()).unwrap();
    assert_eq!(plain_summary.transform, None);
    let mut newer = plain_object.clone();
    newer[4..6].copy_from_slice(&VERSION_TRANSFORMED.to_le_bytes());
    assert!(matches!(
        decrypt(&newer, &plain_summary.object_id, &file_key_of(&drive, &plain_summary)),
        Err(CryptoError::Damaged(_))
    ));
    // A version from the future is named, not guessed.
    let mut future = object.clone();
    future[4..6].copy_from_slice(&3u16.to_le_bytes());
    assert!(matches!(decrypt(&future, &id, &key), Err(CryptoError::Unsupported(_))));
    // Bytes past the transform block stay zero.
    let mut padded = object.clone();
    padded[HEADER_LEN - 1] = 1;
    assert!(matches!(decrypt(&padded, &id, &key), Err(CryptoError::Damaged(_))));
}

#[test]
fn a_flipped_segment_of_a_transformed_file_is_caught() {
    let drive = DriveKey::generate().unwrap();
    let id = ObjectId::generate().unwrap();
    let (object, summary) = encrypt(&docx(), id, &drive, &WriteOptions::default()).unwrap();
    assert_eq!(summary.transform, Some(TransformId::Containers));
    let mut changed = object.clone();
    changed[HEADER_LEN + 40] ^= 1;
    assert!(matches!(
        decrypt(&changed, &id, &file_key_of(&drive, &summary)),
        Err(CryptoError::Damaged(_))
    ));
}

#[test]
fn compression_is_on_by_default_and_text_is_stored_smaller() {
    assert_eq!(WriteOptions::default().compression, Compression::Auto);
    let drive = DriveKey::generate().unwrap();
    let text: Vec<u8> = b"{\"name\":\"invoice\",\"total\":12.50,\"items\":[1,2,3]}\n"
        .iter()
        .copied()
        .cycle()
        .take(200_000)
        .collect();
    let (object, summary) =
        encrypt(&text, ObjectId::generate().unwrap(), &drive, &WriteOptions::default()).unwrap();
    assert!(summary.compressed);
    assert_eq!(summary.transform, None, "text has no whole-file transform");
    assert!(object.len() < text.len() / 10, "{} bytes for {}", object.len(), text.len());
}
