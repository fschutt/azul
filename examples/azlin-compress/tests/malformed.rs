//! Damaged and hostile containers: every one is an error, never a panic, never other bytes,
//! never a large allocation; and what a newer version wrote is refused by name.

mod common;

use azlin_compress::{container, decode, encode, Error, Limits, Options};
use common::Rng;

fn options() -> Options {
    Options {
        segment_size: 1024,
        threads: 1,
        ..Options::upload()
    }
}

fn sample() -> (Vec<u8>, Vec<u8>) {
    let mut rng = Rng::new(20);
    let mut input = common::text(&mut rng, 5000);
    input.extend(rng.bytes(1500));
    let encoded = encode(&input, &options()).unwrap();
    (input, encoded.bytes.to_vec())
}

/// Decoding `bytes` either fails or gives exactly `original`.
fn never_other_bytes(bytes: &[u8], original: &[u8]) {
    if let Ok(out) = decode(bytes, &options()) {
        assert_eq!(&out[..], original);
    }
}

#[test]
fn every_cut_of_a_container_is_an_error() {
    let (_, container) = sample();
    for len in 0..container.len() {
        assert!(decode(&container[..len], &options()).is_err(), "cut at {len}");
    }
}

#[test]
fn every_flipped_byte_is_an_error_or_the_same_file() {
    let (input, container) = sample();
    for at in 0..container.len() {
        for bit in [0x01u8, 0x80] {
            let mut damaged = container.clone();
            damaged[at] ^= bit;
            never_other_bytes(&damaged, &input);
        }
    }
}

#[test]
fn bytes_after_the_end_are_an_error() {
    let (_, mut container) = sample();
    container.push(0);
    assert!(matches!(decode(&container, &options()), Err(Error::Damaged(_))));
}

#[test]
fn random_bytes_behind_a_valid_header_are_an_error() {
    let mut rng = Rng::new(21);
    for _ in 0..2000 {
        let mut bytes = b"AZC1\x01\x00\x00\x00\x00\x04\x00\x00\x00\x00".to_vec();
        let len = rng.below(200) as usize;
        bytes.extend(rng.bytes(len));
        let _ = decode(&bytes, &options());
    }
    for _ in 0..2000 {
        let len = rng.below(300) as usize;
        let bytes = rng.bytes(len);
        assert!(decode(&bytes, &options()).is_err());
    }
}

fn header(version: u8, flags: u8, transform: u8, transform_version: u8, segment: u32) -> Vec<u8> {
    let mut bytes = b"AZC1".to_vec();
    bytes.extend_from_slice(&[version, flags, transform, transform_version]);
    bytes.extend_from_slice(&segment.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes
}

fn end(transformed: u64, original: u64, hash: &[u8; 32]) -> Vec<u8> {
    let mut bytes = vec![0xFF];
    bytes.extend_from_slice(&transformed.to_le_bytes());
    bytes.extend_from_slice(&original.to_le_bytes());
    bytes.extend_from_slice(hash);
    bytes
}

#[test]
fn what_a_newer_version_wrote_is_refused_by_name() {
    let empty_end = end(0, 0, blake3::hash(b"").as_bytes());
    let ok = [header(1, 0, 0, 0, 1 << 20), empty_end.clone()].concat();
    assert_eq!(&decode(&ok, &options()).unwrap()[..], b"");
    for bytes in [
        header(2, 0, 0, 0, 1 << 20),
        header(1, 1, 0, 0, 1 << 20),
        header(1, 0, 2, 2, 1 << 20), // containers transform version 2
        header(1, 0, 1, 2, 1 << 20), // JPEG transform version 2
        header(1, 0, 9, 1, 1 << 20),
    ] {
        let bytes = [bytes, empty_end.clone()].concat();
        assert!(
            matches!(decode(&bytes, &options()), Err(Error::Unsupported(_))),
            "{bytes:?}"
        );
    }
    // Transform parameters are a later version's.
    let mut with_params = header(1, 0, 0, 0, 1 << 20);
    with_params[12] = 1;
    with_params.push(0);
    assert!(matches!(
        decode(&[with_params, empty_end.clone()].concat(), &options()),
        Err(Error::Unsupported(_))
    ));
    // Codec 3 (JPEG XL) and unknown codecs.
    for codec in [3u8, 5, 0x80] {
        let mut bytes = header(1, 0, 0, 0, 1 << 20);
        bytes.push(codec);
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(b"ab");
        bytes.extend_from_slice(&end(4, 4, blake3::hash(b"abcd").as_bytes()));
        assert!(matches!(decode(&bytes, &options()), Err(Error::Unsupported(_))), "codec {codec}");
    }
    // A dictionary this version does not know.
    let mut bytes = header(1, 0, 0, 0, 1 << 20);
    bytes.push(4);
    bytes.extend_from_slice(&100u32.to_le_bytes());
    bytes.extend_from_slice(&10u32.to_le_bytes());
    bytes.extend_from_slice(&0xBEEFu16.to_le_bytes());
    bytes.extend_from_slice(&[0; 8]);
    bytes.extend_from_slice(&end(100, 100, &[0; 32]));
    assert!(matches!(decode(&bytes, &options()), Err(Error::Unsupported(_))));
}

#[test]
fn lengths_out_of_range_are_refused_before_anything_is_allocated() {
    // Segment sizes outside 1 KiB ..= 16 MiB.
    for segment in [0u32, 1023, (1 << 24) + 1, u32::MAX] {
        let bytes = [header(1, 0, 0, 0, segment), end(0, 0, &[0; 32])].concat();
        assert!(decode(&bytes, &options()).is_err(), "segment size {segment}");
    }
    // A segment longer than the segment size, a zero-length one, a stored one whose data
    // length differs, a compressed one that is not smaller.
    for (codec, plain, data) in [(0u8, 2048u32, 2048u32), (0, 0, 0), (0, 10, 11), (1, 10, 10), (1, 10, u32::MAX)] {
        let mut bytes = header(1, 0, 0, 0, 1024);
        bytes.push(codec);
        bytes.extend_from_slice(&plain.to_le_bytes());
        bytes.extend_from_slice(&data.to_le_bytes());
        bytes.extend_from_slice(&[0; 16]);
        assert!(
            matches!(decode(&bytes, &options()), Err(Error::Damaged(_))),
            "codec {codec} plain {plain} data {data}"
        );
    }
    // A segment after a short one.
    let mut bytes = header(1, 0, 0, 0, 1024);
    for _ in 0..2 {
        bytes.push(0);
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(b"abc");
    }
    bytes.extend_from_slice(&end(6, 6, blake3::hash(b"abcabc").as_bytes()));
    assert!(matches!(decode(&bytes, &options()), Err(Error::Damaged(_))));
}

#[test]
fn a_decompression_bomb_stops_at_the_cap() {
    let zeros = vec![0u8; 3 << 20];
    let encoded = encode(&zeros, &Options::upload()).unwrap();
    assert!(encoded.bytes.len() < 4096);
    let capped = Options {
        limits: Limits {
            max_decoded_len: 1 << 20,
            ..Limits::default()
        },
        ..Options::upload()
    };
    assert!(matches!(decode(&encoded.bytes, &capped), Err(Error::Limit(_))));
    assert_eq!(decode(&encoded.bytes, &Options::upload()).unwrap().len(), 3 << 20);
}

#[test]
fn a_wrong_hash_or_length_in_the_end_record_is_an_error() {
    let (_, container) = sample();
    let n = container.len();
    for at in [n - 49 + 1, n - 49 + 9, n - 1] {
        let mut damaged = container.clone();
        damaged[at] ^= 0x10;
        assert!(matches!(decode(&damaged, &options()), Err(Error::Damaged(_))), "byte {at}");
    }
}

#[test]
fn the_header_reader_reads_only_the_header() {
    let (_, container) = sample();
    let mut reader = std::io::Cursor::new(&container[..]);
    let header = container::read_header(&mut reader).unwrap();
    assert_eq!(header.segment_size, 1024);
    assert_eq!(reader.position(), 14);
}
