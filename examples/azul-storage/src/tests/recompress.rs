//! The recompression pass: which files, smaller objects in place of bigger ones, the user's
//! edits winning, a pass that stops and goes on, and a later pass that leaves done files alone.

use std::{cell::Cell, sync::Arc};

use super::mem_bucket::MemBucket;
use crate::{
    crypto::{
        azl1::{WriteOptions, HEADER_LEN},
        codec::Compression,
        random_bytes, DriveKey, ObjectId,
    },
    encrypted::{EncryptedDrive, Expect, IndexChange, MemoryIndex},
    recompress::{
        recompress_one, recompress_with, run_pass, worth_trying, RecompressOutcome,
        RecompressPolicy, RecompressState, STATE_FORMAT,
    },
    Drive, DriveError,
};

const SMALL: u32 = 4096;

fn noise(len: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; len];
    random_bytes(&mut bytes).unwrap();
    bytes
}

/// Text that the upload pass would compress too, but that is written here as it is.
fn text(len: usize) -> Vec<u8> {
    b"Minutes of the meeting: the budget, the timeline, the open questions; nothing secret. "
        .iter()
        .copied()
        .cycle()
        .take(len)
        .collect()
}

/// An encrypted drive whose uploads store every segment as it is (the recompression pass has
/// everything to gain), in small segments.
fn uploaded_uncompressed() -> EncryptedDrive<Arc<MemBucket>> {
    EncryptedDrive::new(
        Arc::new(MemBucket::new()),
        DriveKey::generate().unwrap(),
        Arc::new(MemoryIndex::new()),
    )
    .with_options(WriteOptions {
        segment_size: SMALL,
        compression: Compression::Never,
    })
}

fn object_of(drive: &EncryptedDrive<Arc<MemBucket>>, path: &str) -> ObjectId {
    drive.entry(path).unwrap().object_id().unwrap()
}

/// Sets `path`'s date in the index (the tests' clock).
fn set_modified(drive: &EncryptedDrive<Arc<MemBucket>>, path: &str, modified: u64) {
    let mut entry = drive.entry(path).unwrap();
    let id = entry.object_id().unwrap();
    entry.modified = Some(modified);
    drive
        .index()
        .apply(vec![IndexChange::Put {
            path: path.to_string(),
            entry,
            expect: Expect::Object(id),
        }])
        .unwrap();
}

const BASELINE_JPEG: &[u8] = include_bytes!("../../../azlin-compress/tests/fixtures/baseline.jpg");
const EMBEDDED_JPEG: &[u8] = include_bytes!("../../../azlin-compress/tests/fixtures/embedded.jpg");

/// Only dense kinds without a whole-file transform are left out: JPEG, PNG, PDF and Office
/// files (stored as they were by builds with a skip list) get their transform now.
#[test]
fn every_kind_but_dense_ones_without_a_transform_is_worth_trying() {
    let len = 200_000;
    assert!(worth_trying(&text(4096), len, Some("txt")));
    assert!(worth_trying(b"<html><body>Hallo</body></html>", len, None));
    assert!(worth_trying(b"SQLite format 3\0\x10\x00", len, None));
    assert!(worth_trying(&BASELINE_JPEG[..4096], len, Some("jpg")));
    assert!(worth_trying(b"\x89PNG\r\n\x1A\n\x00\x00\x00\x0DIHDR", len, None));
    assert!(worth_trying(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n", len, None));
    assert!(worth_trying(b"PK\x03\x04\x14\x00\x00\x00\x08\x00", len, Some("docx")));
    assert!(!worth_trying(b"\x00\x00\x00\x18ftypmp42\x00\x00\x00\x00", len, Some("mp4")));
    assert!(!worth_trying(b"7z\xBC\xAF\x27\x1C\x00\x04", len, Some("7z")));
    // A JPEG larger than the transforms take is left: its bytes are dense.
    assert!(!worth_trying(&BASELINE_JPEG[..4096], 65 << 20, Some("jpg")));
}

/// Files an older build stored as they were (JPEG, PDF) get their transform from the pass:
/// smaller, bit-exact, version 2 objects.
#[test]
fn a_jpeg_and_a_pdf_stored_as_they_were_are_replaced_by_their_transforms() {
    let drive = uploaded_uncompressed();
    let content = super::compress_inputs::far_repeats(&mut super::compress_inputs::Rng::new(46), 40 * 1024, 4);
    let flate = super::compress_inputs::zlib(&content, 6);
    let pdf = super::compress_inputs::pdf(
        &[
            ("/Filter /FlateDecode", &flate),
            ("/Type /XObject /Subtype /Image /Width 560 /Height 420 /Filter /DCTDecode", EMBEDDED_JPEG),
        ],
        50,
    );
    // The fixture JPEG is small (13 KB): what is checked here is that the transform replaces
    // the file at all, not how much the default policy asks of it.
    let policy = RecompressPolicy {
        min_size: 1,
        min_saving_percent: 1,
    };
    for (path, plain) in [("photos/IMG_0001.jpg", BASELINE_JPEG), ("invoices/2026-10.pdf", &pdf[..])] {
        drive.put(path, plain).unwrap();
        let old = object_of(&drive, path);
        let outcome = recompress_one(&drive, path, &policy).unwrap();
        let RecompressOutcome::Replaced { before, after } = outcome else {
            panic!("{path}: {outcome:?}");
        };
        assert!(after < before, "{path}: {after} of {before}");
        assert_ne!(object_of(&drive, path), old, "{path}");
        assert_eq!(drive.get(path).unwrap(), plain, "{path}");
        let object = drive.inner().get(&object_of(&drive, path).bucket_key()).unwrap();
        assert_eq!(u16::from_le_bytes([object[4], object[5]]), 2, "{path}: a version 2 object");
        let entry = drive.entry(path).unwrap();
        assert_eq!(entry.size, plain.len() as u64, "{path}: the size stays the file's");
        assert!(entry.object.unwrap().compressed, "{path}");
    }
}

#[test]
fn a_video_is_still_not_tried() {
    let drive = uploaded_uncompressed();
    let mut mp4 = b"\x00\x00\x00\x18ftypmp42\x00\x00\x00\x00mp42isom".to_vec();
    mp4.extend(text(200_000));
    drive.put("films/holiday.mp4", &mp4).unwrap();
    let puts = drive.inner().puts();
    assert_eq!(
        recompress_one(&drive, "films/holiday.mp4", &RecompressPolicy::default()).unwrap(),
        RecompressOutcome::Ineligible
    );
    assert_eq!(drive.inner().puts(), puts);
}

#[test]
fn a_file_written_fast_is_rewritten_smaller_and_reads_the_same() {
    let drive = uploaded_uncompressed();
    let plain = text(200_000);
    drive.put("notes/minutes.txt", &plain).unwrap();
    let before_entry = drive.entry("notes/minutes.txt").unwrap();
    let old = before_entry.object_id().unwrap();

    let outcome = recompress_one(&drive, "notes/minutes.txt", &RecompressPolicy::default()).unwrap();
    let RecompressOutcome::Replaced { before, after } = outcome else {
        panic!("{outcome:?}");
    };
    assert!(after < before / 10, "{after} of {before}");
    assert_eq!(drive.get("notes/minutes.txt").unwrap(), plain);

    let after_entry = drive.entry("notes/minutes.txt").unwrap();
    assert_ne!(after_entry.object_id(), Some(old), "a new object with a new file key");
    assert_eq!(after_entry.size, before_entry.size, "quotas count the plaintext");
    assert_eq!(after_entry.modified, before_entry.modified, "recompression is no edit");
    assert_eq!(
        drive.inner().keys(),
        vec![after_entry.object_id().unwrap().bucket_key()],
        "the old object left the bucket"
    );
}

#[test]
fn binary_data_is_rewritten_with_zstd_and_reads_back_in_ranges() {
    let drive = uploaded_uncompressed();
    let mut plain = b"SQLite format 3\0".to_vec();
    for i in 0..30_000u32 {
        plain.extend_from_slice(&(i % 251).to_le_bytes());
    }
    drive.put("db/app.sqlite", &plain).unwrap();
    let outcome = recompress_one(&drive, "db/app.sqlite", &RecompressPolicy::default()).unwrap();
    assert!(matches!(outcome, RecompressOutcome::Replaced { .. }), "{outcome:?}");
    let range = crate::ByteRange {
        start: 5000,
        end: Some(9000),
    };
    assert_eq!(drive.get_range("db/app.sqlite", range).unwrap(), &plain[5000..=9000]);
}

#[test]
fn a_file_that_saves_too_little_is_left_alone_and_nothing_is_uploaded() {
    let drive = uploaded_uncompressed();
    drive.put("notes/minutes.txt", &text(200_000)).unwrap();
    let old = object_of(&drive, "notes/minutes.txt");
    let puts = drive.inner().puts();
    let greedy = RecompressPolicy {
        min_saving_percent: 100,
        ..RecompressPolicy::default()
    };
    assert_eq!(
        recompress_one(&drive, "notes/minutes.txt", &greedy).unwrap(),
        RecompressOutcome::NotWorthIt
    );
    assert_eq!(object_of(&drive, "notes/minutes.txt"), old);
    assert_eq!(drive.inner().puts(), puts, "nothing uploaded");
}

#[test]
fn small_files_folders_and_compressed_formats_are_not_tried() {
    let drive = uploaded_uncompressed();
    let policy = RecompressPolicy::default();
    drive.put("small.txt", &text(1000)).unwrap();
    drive.create_folder("photos/").unwrap();
    let mut archive = b"7z\xBC\xAF\x27\x1C\x00\x04".to_vec();
    archive.extend(noise(100_000));
    drive.put("backup/old.7z", &archive).unwrap();
    let puts = drive.inner().puts();
    for path in ["small.txt", "photos/", "backup/old.7z"] {
        assert_eq!(
            recompress_one(&drive, path, &policy).unwrap(),
            RecompressOutcome::Ineligible,
            "{path}"
        );
    }
    // A PNG is tried (its transform), but a damaged one gains nothing: nothing is uploaded.
    let mut png = b"\x89PNG\r\n\x1A\n".to_vec();
    png.extend(noise(100_000));
    drive.put("photos/a.png", &png).unwrap();
    let puts_with_png = drive.inner().puts();
    assert_eq!(
        recompress_one(&drive, "photos/a.png", &policy).unwrap(),
        RecompressOutcome::NotWorthIt
    );
    assert_eq!(drive.inner().puts(), puts_with_png);
    assert_eq!(
        recompress_one(&drive, "gone.txt", &policy).unwrap(),
        RecompressOutcome::Skipped
    );
    assert_eq!(drive.inner().puts(), puts);
}

#[test]
fn a_file_edited_meanwhile_keeps_the_users_version() {
    let drive = uploaded_uncompressed();
    drive.put("notes/minutes.txt", &text(200_000)).unwrap();
    let edit = || drive.put("notes/minutes.txt", b"the user's new version").unwrap();
    assert_eq!(
        recompress_with(&drive, "notes/minutes.txt", &RecompressPolicy::default(), &edit)
            .unwrap(),
        RecompressOutcome::Skipped
    );
    assert_eq!(drive.get("notes/minutes.txt").unwrap(), b"the user's new version");
    assert_eq!(
        drive.inner().keys(),
        vec![object_of(&drive, "notes/minutes.txt").bucket_key()],
        "the recompressed object went again, the replaced original too"
    );
}

#[test]
fn a_stopped_pass_goes_on_where_it_stopped_and_a_later_pass_looks_only_at_newer_files() {
    let drive = uploaded_uncompressed();
    for path in ["a.txt", "b.txt", "c.txt"] {
        drive.put(path, &text(100_000)).unwrap();
        set_modified(&drive, path, 1000);
    }
    let policy = RecompressPolicy::default();
    let mut state = RecompressState::default();
    let mut saved = Vec::new();

    // Stopped after the first file.
    let asked = Cell::new(0);
    let stop_after_one = || {
        asked.set(asked.get() + 1);
        asked.get() > 1
    };
    let finished = run_pass(
        &drive,
        &mut state,
        &policy,
        2000,
        &mut |s| {
            saved.push(s.clone());
            Ok(())
        },
        &stop_after_one,
    )
    .unwrap();
    assert!(!finished);
    assert_eq!(state.cursor.as_deref(), Some("a.txt"));
    assert_eq!(state.started, Some(2000));
    assert_eq!(state.replaced, 1);
    let b_before = object_of(&drive, "b.txt");

    // Resumed from the saved state: b and c, not a again.
    let mut state = RecompressState::from_json(&saved.last().unwrap().to_json()).unwrap();
    let a_done = object_of(&drive, "a.txt");
    assert!(run_pass(&drive, &mut state, &policy, 2500, &mut |_| Ok(()), &|| false).unwrap());
    assert_eq!(state.replaced, 3);
    assert_eq!(state.since, 2000, "the next pass looks at files modified since this one began");
    assert_eq!((state.started, state.cursor.clone()), (None, None));
    assert_eq!(object_of(&drive, "a.txt"), a_done);
    assert_ne!(object_of(&drive, "b.txt"), b_before);

    // A later pass: only the file that came since.
    drive.put("d.txt", &text(100_000)).unwrap();
    set_modified(&drive, "d.txt", 3000);
    let done: Vec<ObjectId> = ["a.txt", "b.txt", "c.txt"]
        .iter()
        .map(|path| object_of(&drive, path))
        .collect();
    let d_before = object_of(&drive, "d.txt");
    assert!(run_pass(&drive, &mut state, &policy, 4000, &mut |_| Ok(()), &|| false).unwrap());
    assert_eq!(state.replaced, 4);
    assert_eq!(state.since, 4000);
    let after: Vec<ObjectId> = ["a.txt", "b.txt", "c.txt"]
        .iter()
        .map(|path| object_of(&drive, path))
        .collect();
    assert_eq!(after, done, "recompressed files keep their date and are not done again");
    assert_ne!(object_of(&drive, "d.txt"), d_before);
}

#[test]
fn a_damaged_file_is_counted_and_left_where_it_is() {
    let drive = uploaded_uncompressed();
    drive.put("broken.txt", &text(100_000)).unwrap();
    drive.put("fine.txt", &text(100_000)).unwrap();
    let key = object_of(&drive, "broken.txt").bucket_key();
    let mut bytes = drive.inner().object(&key).unwrap();
    bytes[HEADER_LEN + 10] ^= 1;
    drive.inner().set(&key, bytes);
    let mut state = RecompressState::default();
    assert!(run_pass(
        &drive,
        &mut state,
        &RecompressPolicy::default(),
        1,
        &mut |_| Ok(()),
        &|| false
    )
    .unwrap());
    assert_eq!((state.failed, state.replaced), (1, 1));
    assert_eq!(object_of(&drive, "broken.txt").bucket_key(), key);
}

#[test]
fn the_state_file_round_trips_and_other_files_are_refused() {
    let state = RecompressState {
        since: 7,
        started: Some(9),
        cursor: Some(String::from("x/y.txt")),
        replaced: 2,
        saved: 1234,
        failed: 1,
        ..RecompressState::default()
    };
    assert_eq!(RecompressState::from_json(&state.to_json()).unwrap(), state);
    assert!(matches!(
        RecompressState::from_json(r#"{"format":"azul-storage.migration","moved":0,"bytes":0}"#),
        Err(DriveError::InvalidConfig(_))
    ));
    // The state of the pass before the transforms: refused, so the next pass starts over and
    // every file gets the transforms once.
    assert_ne!(STATE_FORMAT, "azul-storage.recompression");
    assert!(matches!(
        RecompressState::from_json(r#"{"format":"azul-storage.recompression","since":1760000000}"#),
        Err(DriveError::InvalidConfig(_))
    ));
    assert_eq!(RecompressState::default().since, 0);
    assert!(RecompressPolicy::default().worth_it(100, 95));
    assert!(!RecompressPolicy::default().worth_it(100, 96));
}
