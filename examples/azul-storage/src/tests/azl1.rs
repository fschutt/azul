//! The AZL1 object format: round trips at the segment boundaries, compression, ranged and
//! streaming reads, and every way a changed object is caught.

use std::{
    io::{Cursor, Read, Seek, SeekFrom, Write},
    sync::Mutex,
};

use crate::crypto::{
    azl1::{
        decrypt, encrypt, encrypt_stream, segment_count, Azl1Writer, Finished, ObjectSource,
        ObjectSummary, OpenObject, PublicHeader, WriteOptions, DEFAULT_SEGMENT_SIZE, HEADER_LEN,
        MAX_SEGMENT_SIZE,
    },
    codec::{Compression, Recoding},
    CryptoError, DriveKey, FileKey, ObjectId,
};

const MIB: usize = 1 << 20;
/// A small segment size: many segments without megabytes of test data.
const SMALL: u32 = 4096;
/// A full small segment as stored: the codec byte, the data and the tag.
const SMALL_SEALED: u64 = 1 + SMALL as u64 + 16;

fn drive_key() -> DriveKey {
    DriveKey::generate().unwrap()
}

fn new_id() -> ObjectId {
    ObjectId::generate().unwrap()
}

/// Bytes from the OS random source: zstd cannot make them smaller.
fn noise(len: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; len];
    crate::crypto::random_bytes(&mut bytes).unwrap();
    bytes
}

/// Text that compresses well.
fn text(len: usize) -> Vec<u8> {
    b"Names, paths and folders never reach the bucket; the index keeps them. "
        .iter()
        .copied()
        .cycle()
        .take(len)
        .collect()
}

/// Segments of `segment_size`, none compressed.
fn stored(segment_size: u32) -> WriteOptions {
    WriteOptions {
        segment_size,
        compression: Compression::Never,
    }
}

/// The length of the object of `n` stored bytes: the header, every segment with its codec
/// byte and tag, the trailer (4 bytes a segment and a tag) and its length.
fn stored_len(n: u64, segment_size: u32) -> u64 {
    let count = segment_count(n, segment_size);
    HEADER_LEN as u64 + n + count * 17 + (count * 4 + 16) + 8
}

fn file_key_of(drive: &DriveKey, summary: &ObjectSummary) -> FileKey {
    drive
        .unwrap_file_key(&summary.wrapped_key, &summary.object_id)
        .unwrap()
}

/// `plain` encrypted, checked, and decrypted both with the file key (the index's way) and
/// with the drive key through the header's own wrap.
fn round_trip(plain: &[u8], options: &WriteOptions) -> (Vec<u8>, ObjectSummary) {
    let drive = drive_key();
    let object_id = new_id();
    let (object, summary) = encrypt(plain, object_id, &drive, options).unwrap();
    assert_eq!(summary.object_id, object_id);
    assert_eq!(summary.object_len, object.len() as u64);
    assert_eq!(summary.plaintext_size, plain.len() as u64);
    assert_eq!(summary.blake3, *blake3::hash(plain).as_bytes());
    assert_eq!(
        summary.segment_count,
        segment_count(plain.len() as u64, options.segment_size)
    );
    assert_eq!(&object[..4], b"AZL1");
    let key = file_key_of(&drive, &summary);
    assert_eq!(decrypt(&object, &object_id, &key).unwrap(), plain);
    let opened = OpenObject::open_with_drive_key(&object[..], &object_id, &drive).unwrap();
    assert_eq!(opened.plaintext_size(), plain.len() as u64);
    assert_eq!(opened.read_all(&object[..]).unwrap(), plain);
    (object, summary)
}

/// An object source that records every read.
struct Counted {
    bytes: Vec<u8>,
    reads: Mutex<Vec<(u64, u64)>>,
}

impl Counted {
    fn new(bytes: Vec<u8>) -> Counted {
        Counted {
            bytes,
            reads: Mutex::new(Vec::new()),
        }
    }

    fn reads(&self) -> Vec<(u64, u64)> {
        self.reads.lock().unwrap().clone()
    }
}

impl ObjectSource for Counted {
    type Error = CryptoError;

    fn read_at(&self, offset: u64, len: u64) -> Result<Vec<u8>, CryptoError> {
        self.reads.lock().unwrap().push((offset, len));
        self.bytes.read_at(offset, len)
    }
}

#[test]
fn an_empty_file_is_one_empty_last_segment_and_round_trips() {
    let (object, summary) = round_trip(&[], &stored(DEFAULT_SEGMENT_SIZE));
    assert_eq!(summary.segment_count, 1);
    assert_eq!(object.len() as u64, stored_len(0, DEFAULT_SEGMENT_SIZE));
    assert_eq!(object.len(), HEADER_LEN + 17 + 20 + 8);
    // The same with compression on: nothing to compress.
    let (_, summary) = round_trip(&[], &WriteOptions::default());
    assert!(!summary.compressed);
}

#[test]
fn a_one_byte_file_round_trips() {
    let (object, summary) = round_trip(b"x", &WriteOptions::default());
    assert_eq!(summary.segment_count, 1);
    assert_eq!(object.len() as u64, stored_len(1, DEFAULT_SEGMENT_SIZE));
}

#[test]
fn a_file_of_exactly_one_segment_stays_in_one_segment() {
    let plain = noise(MIB);
    let (object, summary) = round_trip(&plain, &WriteOptions::default());
    assert_eq!(summary.segment_count, 1);
    assert!(!summary.compressed, "noise does not compress");
    assert_eq!(object.len() as u64, stored_len(MIB as u64, DEFAULT_SEGMENT_SIZE));
}

#[test]
fn one_byte_more_than_a_segment_makes_a_second_segment() {
    let plain = noise(MIB + 1);
    let (object, summary) = round_trip(&plain, &WriteOptions::default());
    assert_eq!(summary.segment_count, 2);
    assert_eq!(
        object.len() as u64,
        stored_len(MIB as u64 + 1, DEFAULT_SEGMENT_SIZE)
    );
    let opened = OpenObject::open(&object[..], &summary.object_id, &FileKey::from_bytes([0; 32]));
    assert_eq!(opened.unwrap_err(), CryptoError::KeyMismatch);
}

#[test]
fn a_multi_segment_file_round_trips_when_written_in_odd_sized_pieces() {
    let drive = drive_key();
    let object_id = new_id();
    let plain = noise(10 * SMALL as usize + 123);
    let mut writer =
        Azl1Writer::new(vec![0u8; HEADER_LEN], object_id, &drive, &stored(SMALL)).unwrap();
    for piece in plain.chunks(777) {
        writer.write_all(piece).unwrap();
    }
    let Finished {
        mut sink,
        header,
        summary,
    } = writer.finish().unwrap();
    sink[..HEADER_LEN].copy_from_slice(&header[..]);
    assert_eq!(summary.segment_count, 11);
    assert_eq!(sink.len() as u64, stored_len(plain.len() as u64, SMALL));
    assert_eq!(
        decrypt(&sink, &object_id, &file_key_of(&drive, &summary)).unwrap(),
        plain
    );

    // A file of exactly three segments has three, not an empty fourth.
    let (_, summary) = round_trip(&noise(3 * SMALL as usize), &stored(SMALL));
    assert_eq!(summary.segment_count, 3);
}

#[test]
fn compressible_data_is_stored_smaller_and_round_trips() {
    let plain = text(3 * MIB + 5);
    let (object, summary) = round_trip(&plain, &WriteOptions::default());
    assert!(summary.compressed);
    assert_eq!(summary.segment_count, 4);
    assert!(
        object.len() < plain.len() / 10,
        "{} bytes for {}",
        object.len(),
        plain.len()
    );
    let with_a_wrong_key = OpenObject::open(
        &object[..],
        &summary.object_id,
        &FileKey::from_bytes([1; 32]),
    );
    assert_eq!(with_a_wrong_key.unwrap_err(), CryptoError::KeyMismatch);
}

#[test]
fn a_range_across_compressed_segments_reads_back() {
    let drive = drive_key();
    let object_id = new_id();
    let plain = text(2 * MIB + 77);
    let (object, summary) = encrypt(&plain, object_id, &drive, &WriteOptions::default()).unwrap();
    assert!(summary.compressed);
    let opened = OpenObject::open(&object[..], &object_id, &file_key_of(&drive, &summary)).unwrap();
    assert!(opened.compressed());
    let range = opened
        .read_range(&object[..], MIB as u64 - 5, MIB as u64 + 5)
        .unwrap();
    assert_eq!(range, &plain[MIB - 5..=MIB + 5]);
}

#[test]
fn a_recompressed_object_reads_back_whole_and_in_ranges() {
    for recoding in [Recoding::Brotli, Recoding::ZstdMax] {
        let options = WriteOptions {
            segment_size: SMALL,
            compression: Compression::Recode(recoding),
        };
        // Text with a segment of noise in the middle: that one stays stored.
        let mut plain = text(3 * SMALL as usize);
        plain.extend(noise(SMALL as usize));
        plain.extend(text(2 * SMALL as usize + 9));
        let (object, summary) = round_trip(&plain, &options);
        assert!(summary.compressed, "{recoding:?}");
        assert!(object.len() < plain.len(), "{recoding:?}");
        let with_a_wrong_key =
            OpenObject::open(&object[..], &summary.object_id, &FileKey::from_bytes([1; 32]));
        assert_eq!(with_a_wrong_key.unwrap_err(), CryptoError::KeyMismatch);
        let range_start = 3 * SMALL as u64 - 7;
        let range_end = 4 * SMALL as u64 + 7;
        let drive = drive_key();
        let id = new_id();
        let (object, summary) = encrypt(&plain, id, &drive, &options).unwrap();
        let opened = OpenObject::open(&object[..], &id, &file_key_of(&drive, &summary)).unwrap();
        assert_eq!(
            opened.read_range(&object[..], range_start, range_end).unwrap(),
            &plain[range_start as usize..=range_end as usize],
            "{recoding:?}"
        );
    }
}

#[test]
fn incompressible_data_is_stored_as_it_is() {
    let plain = noise(2 * MIB);
    let (object, summary) = round_trip(&plain, &WriteOptions::default());
    assert!(!summary.compressed);
    assert_eq!(
        object.len() as u64,
        stored_len(2 * MIB as u64, DEFAULT_SEGMENT_SIZE)
    );
}

#[test]
fn a_file_that_is_compressed_already_is_not_tried() {
    let mut png = b"\x89PNG\r\n\x1A\n".to_vec();
    png.extend(text(64 * 1024));
    let (object, summary) = round_trip(&png, &WriteOptions::default());
    assert!(!summary.compressed, "the magic number says compressed");
    assert_eq!(
        object.len() as u64,
        stored_len(png.len() as u64, DEFAULT_SEGMENT_SIZE)
    );
}

#[test]
fn a_ranged_read_reads_the_header_the_tail_and_only_the_segments_it_covers() {
    let drive = drive_key();
    let object_id = new_id();
    let plain = noise(5 * SMALL as usize - 100);
    let (object, summary) = encrypt(&plain, object_id, &drive, &stored(SMALL)).unwrap();
    let source = Counted::new(object);
    let opened = OpenObject::open(&source, &object_id, &file_key_of(&drive, &summary)).unwrap();
    let reads = source.reads();
    assert_eq!(reads.len(), 2, "the header and the tail: {reads:?}");
    assert_eq!(reads[0], (0, HEADER_LEN as u64));
    let tail = 5 * 4 + 16 + 8;
    assert_eq!(reads[1], (opened.object_len() - tail, tail));

    // Bytes 4106..=8211 lie in segments 1 and 2: one read of exactly those two.
    let small = u64::from(SMALL);
    let got = opened
        .read_range(&source, small + 10, 2 * small + 19)
        .unwrap();
    assert_eq!(got, &plain[SMALL as usize + 10..=2 * SMALL as usize + 19]);
    let reads = source.reads();
    assert_eq!(reads.len(), 3, "{reads:?}");
    let (first_offset, first_len) = opened.segment_span(1);
    let (second_offset, second_len) = opened.segment_span(2);
    assert_eq!(first_offset, HEADER_LEN as u64 + SMALL_SEALED);
    assert_eq!(second_offset, first_offset + first_len);
    assert_eq!(reads[2], (first_offset, first_len + second_len));

    // One byte of the last segment: that segment only.
    let last = opened
        .read_range(&source, 4 * small + 1, 4 * small + 1)
        .unwrap();
    assert_eq!(last, vec![plain[4 * SMALL as usize + 1]]);
    assert_eq!(source.reads()[3], opened.segment_span(4));
    assert_eq!(source.reads().len(), 4);
}

#[test]
fn a_range_past_the_end_reads_nothing_and_an_end_past_it_is_clamped() {
    let drive = drive_key();
    let object_id = new_id();
    let plain = noise(3000);
    let (object, summary) = encrypt(&plain, object_id, &drive, &stored(SMALL)).unwrap();
    let opened = OpenObject::open(&object[..], &object_id, &file_key_of(&drive, &summary)).unwrap();
    assert!(opened.read_range(&object[..], 3000, 3005).unwrap().is_empty());
    assert_eq!(
        opened.read_range(&object[..], 2997, u64::MAX).unwrap(),
        &plain[2997..]
    );
}

#[test]
fn the_streaming_reader_hands_out_the_plaintext_a_segment_at_a_time() {
    let drive = drive_key();
    let object_id = new_id();
    let plain = noise(7 * SMALL as usize + 9);
    let (object, summary) = encrypt(&plain, object_id, &drive, &stored(SMALL)).unwrap();
    let key = file_key_of(&drive, &summary);

    let opened = OpenObject::open(&object[..], &object_id, &key).unwrap();
    let mut reader = opened.into_reader(&object[..]);
    assert_eq!(reader.plaintext_size(), plain.len() as u64);
    let mut out = Vec::new();
    let mut buf = [0u8; 1000];
    loop {
        let n = reader.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
    }
    assert_eq!(out, plain);

    let source = Counted::new(object);
    let opened = OpenObject::open(&source, &object_id, &key).unwrap();
    let mut reader = opened.into_reader(&source);
    let mut first = [0u8; 10];
    reader.read_exact(&mut first).unwrap();
    assert_eq!(&first[..], &plain[..10]);
    assert_eq!(
        source.reads().len(),
        3,
        "the header, the tail and the first segment only"
    );
}

#[test]
fn encrypt_stream_writes_the_header_last_into_a_seekable_sink() {
    let drive = drive_key();
    let object_id = new_id();
    let plain = noise(5 * SMALL as usize + 1);
    let mut sink = Cursor::new(b"0123456789".to_vec());
    sink.seek(SeekFrom::End(0)).unwrap();
    let summary =
        encrypt_stream(&mut &plain[..], &mut sink, object_id, &drive, &stored(SMALL)).unwrap();
    assert_eq!(sink.position(), 10 + summary.object_len);
    let bytes = sink.into_inner();
    assert_eq!(&bytes[..10], b"0123456789", "what was before the object stays");
    let object = &bytes[10..];
    assert_eq!(object.len() as u64, summary.object_len);
    assert_eq!(
        decrypt(object, &object_id, &file_key_of(&drive, &summary)).unwrap(),
        plain
    );
}

#[test]
fn a_flipped_byte_in_one_segment_fails_that_segment_only() {
    let drive = drive_key();
    let object_id = new_id();
    let plain = noise(3 * SMALL as usize);
    let (mut object, summary) = encrypt(&plain, object_id, &drive, &stored(SMALL)).unwrap();
    let key = file_key_of(&drive, &summary);
    let middle = HEADER_LEN + SMALL_SEALED as usize + 100;
    object[middle] ^= 0x01;

    let opened = OpenObject::open(&object[..], &object_id, &key).unwrap();
    let small = u64::from(SMALL);
    assert_eq!(
        opened.read_range(&object[..], 0, 10).unwrap(),
        &plain[..=10],
        "segment 0 is intact"
    );
    assert_eq!(
        opened
            .read_range(&object[..], 2 * small, 2 * small + 10)
            .unwrap(),
        &plain[2 * SMALL as usize..=2 * SMALL as usize + 10],
        "segment 2 is intact"
    );
    assert!(matches!(
        opened.read_range(&object[..], small, small),
        Err(CryptoError::Damaged(why)) if why.contains("segment 1")
    ));
    assert!(matches!(
        decrypt(&object, &object_id, &key),
        Err(CryptoError::Damaged(_))
    ));

    let opened = OpenObject::open(&object[..], &object_id, &key).unwrap();
    let mut reader = opened.into_reader(&object[..]);
    let mut out = Vec::new();
    let error = reader.read_to_end(&mut out).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(matches!(CryptoError::from(error), CryptoError::Damaged(_)));
    assert_eq!(out, &plain[..SMALL as usize], "only the segment before it");
}

#[test]
fn a_cut_object_is_refused() {
    let drive = drive_key();
    let object_id = new_id();
    let plain = noise(3 * SMALL as usize + 5);
    let (object, summary) = encrypt(&plain, object_id, &drive, &stored(SMALL)).unwrap();
    let key = file_key_of(&drive, &summary);

    let one_short = &object[..object.len() - 1];
    assert!(matches!(
        decrypt(one_short, &object_id, &key),
        Err(CryptoError::Damaged(_))
    ));
    assert!(matches!(
        OpenObject::open(one_short, &object_id, &key),
        Err(CryptoError::Damaged(_))
    ));

    // The last segment cut out, the tail kept: the trailer opens, the lengths do not add up.
    let opened = OpenObject::open(&object[..], &object_id, &key).unwrap();
    let (last_offset, last_len) = opened.segment_span(3);
    let mut spliced = object[..last_offset as usize].to_vec();
    spliced.extend_from_slice(&object[(last_offset + last_len) as usize..]);
    assert!(matches!(
        decrypt(&spliced, &object_id, &key),
        Err(CryptoError::Damaged(_))
    ));

    // Only the header: no tail to read.
    assert!(matches!(
        decrypt(&object[..HEADER_LEN], &object_id, &key),
        Err(CryptoError::Damaged(_))
    ));
    assert!(matches!(
        decrypt(&object[..100], &object_id, &key),
        Err(CryptoError::Damaged(_))
    ));

    // Something after the object's end is not ignored either.
    let mut longer = object.clone();
    longer.push(0);
    assert!(matches!(
        decrypt(&longer, &object_id, &key),
        Err(CryptoError::Damaged(_))
    ));
}

#[test]
fn swapped_segments_are_refused() {
    let drive = drive_key();
    let object_id = new_id();
    let plain = noise(4 * SMALL as usize);
    let (object, summary) = encrypt(&plain, object_id, &drive, &stored(SMALL)).unwrap();
    let key = file_key_of(&drive, &summary);
    let opened = OpenObject::open(&object[..], &object_id, &key).unwrap();
    let (one, len) = opened.segment_span(1);
    let (two, len_two) = opened.segment_span(2);
    assert_eq!(len, len_two, "two full segments, the same length");
    let (one, two, len) = (one as usize, two as usize, len as usize);
    let mut swapped = object.clone();
    swapped[one..one + len].copy_from_slice(&object[two..two + len]);
    swapped[two..two + len].copy_from_slice(&object[one..one + len]);

    let opened = OpenObject::open(&swapped[..], &object_id, &key).unwrap();
    let small = u64::from(SMALL);
    assert!(matches!(
        opened.read_range(&swapped[..], small, small),
        Err(CryptoError::Damaged(_))
    ));
    assert!(matches!(
        opened.read_range(&swapped[..], 2 * small, 2 * small),
        Err(CryptoError::Damaged(_))
    ));
    assert!(matches!(
        decrypt(&swapped, &object_id, &key),
        Err(CryptoError::Damaged(_))
    ));
}

#[test]
fn a_wrong_drive_key_does_not_open_the_object() {
    let drive = drive_key();
    let object_id = new_id();
    let (object, _) = encrypt(b"secret", object_id, &drive, &WriteOptions::default()).unwrap();
    let other = drive_key();
    assert_eq!(
        OpenObject::open_with_drive_key(&object[..], &object_id, &other).unwrap_err(),
        CryptoError::WrongKey
    );
    assert_eq!(
        decrypt(&object, &object_id, &FileKey::generate().unwrap()).unwrap_err(),
        CryptoError::KeyMismatch
    );
}

#[test]
fn the_file_key_of_another_object_fails_the_key_commitment() {
    let drive = drive_key();
    let (a_id, b_id) = (new_id(), new_id());
    let (a, _) = encrypt(b"file a", a_id, &drive, &WriteOptions::default()).unwrap();
    let (_, b_summary) = encrypt(b"file b", b_id, &drive, &WriteOptions::default()).unwrap();
    let b_key = file_key_of(&drive, &b_summary);
    assert_eq!(
        OpenObject::open(&a[..], &a_id, &b_key).unwrap_err(),
        CryptoError::KeyMismatch
    );
    // Nor does B's wrapped key open for A (the wrap names its object).
    assert!(matches!(
        drive.unwrap_file_key(&b_summary.wrapped_key, &a_id),
        Err(CryptoError::Damaged(_))
    ));
}

#[test]
fn a_changed_commitment_is_caught_before_anything_is_decrypted() {
    let drive = drive_key();
    let object_id = new_id();
    let (mut object, summary) =
        encrypt(b"committed", object_id, &drive, &WriteOptions::default()).unwrap();
    let key = file_key_of(&drive, &summary);
    object[56] ^= 0x80;
    assert_eq!(
        decrypt(&object, &object_id, &key).unwrap_err(),
        CryptoError::KeyMismatch
    );
}

#[test]
fn a_changed_header_field_or_another_object_id_is_refused() {
    let drive = drive_key();
    let object_id = new_id();
    let plain = noise(2 * SMALL as usize);
    let (object, summary) = encrypt(&plain, object_id, &drive, &stored(SMALL)).unwrap();
    let key = file_key_of(&drive, &summary);

    // Another segment size (still a valid one): the info does not authenticate.
    let mut resized = object.clone();
    resized[12..16].copy_from_slice(&(2 * SMALL).to_le_bytes());
    assert!(matches!(
        decrypt(&resized, &object_id, &key),
        Err(CryptoError::Damaged(why)) if why.contains("header")
    ));
    // Read as another object.
    assert!(matches!(
        decrypt(&object, &new_id(), &key),
        Err(CryptoError::Damaged(why)) if why.contains("another object")
    ));
    // Bytes the format keeps zero.
    let mut padded = object.clone();
    padded[4000] = 1;
    assert!(matches!(
        decrypt(&padded, &object_id, &key),
        Err(CryptoError::Damaged(_))
    ));
    // Not AZL1, or a version from the future.
    let mut renamed = object.clone();
    renamed[3] = b'2';
    assert!(matches!(
        decrypt(&renamed, &object_id, &key),
        Err(CryptoError::Damaged(why)) if why.contains("AZL1")
    ));
    let mut newer = object.clone();
    newer[4] = 2;
    assert!(matches!(
        decrypt(&newer, &object_id, &key),
        Err(CryptoError::Unsupported(_))
    ));
    // A sealed info changed: the header does not authenticate.
    let mut sealed = object.clone();
    sealed[210] ^= 1;
    assert!(matches!(
        decrypt(&sealed, &object_id, &key),
        Err(CryptoError::Damaged(_))
    ));
    // The trailer changed.
    let mut trailer = object.clone();
    let at = trailer.len() - 8 - 3;
    trailer[at] ^= 1;
    assert!(matches!(
        decrypt(&trailer, &object_id, &key),
        Err(CryptoError::Damaged(why)) if why.contains("trailer")
    ));
}

#[test]
fn the_public_header_shows_neither_the_plaintext_size_nor_its_hash() {
    let drive = drive_key();
    let object_id = new_id();
    let plain = noise(123_457);
    let (object, summary) =
        encrypt(&plain, object_id, &drive, &WriteOptions::default()).unwrap();
    let header = &object[..HEADER_LEN];
    let size = (plain.len() as u64).to_le_bytes();
    assert!(!header.windows(size.len()).any(|w| w == size));
    assert!(!header.windows(32).any(|w| w == summary.blake3));
    let public = PublicHeader::parse(&object).unwrap();
    assert_eq!(public.segment_size, DEFAULT_SEGMENT_SIZE);
    assert_eq!(public.object_id, object_id);
    assert_eq!(public.wrapped_key, summary.wrapped_key);
    assert_eq!(public.wrapped_key.key_id, drive.id());
}

#[test]
fn every_object_gets_its_own_keys_and_nonces() {
    let drive = drive_key();
    let object_id = new_id();
    let plain = text(10_000);
    let (a, a_summary) = encrypt(&plain, object_id, &drive, &WriteOptions::default()).unwrap();
    let (b, b_summary) = encrypt(&plain, object_id, &drive, &WriteOptions::default()).unwrap();
    assert_eq!(a.len(), b.len());
    assert_ne!(&a[HEADER_LEN..], &b[HEADER_LEN..], "a fresh file key per object");
    assert_ne!(&a[32..51], &b[32..51], "a fresh nonce prefix");
    assert_ne!(&a[56..88], &b[56..88], "another key, another commitment");
    assert_ne!(a_summary.wrapped_key, b_summary.wrapped_key);
    assert_ne!(
        file_key_of(&drive, &a_summary),
        file_key_of(&drive, &b_summary)
    );
    assert_eq!(a_summary.blake3, b_summary.blake3, "the same plaintext");
}

#[test]
fn a_writer_refuses_segment_sizes_out_of_bounds() {
    let drive = drive_key();
    for segment_size in [100, MAX_SEGMENT_SIZE + 1] {
        assert!(matches!(
            encrypt(b"x", new_id(), &drive, &stored(segment_size)),
            Err(CryptoError::Unsupported(_))
        ));
    }
}
