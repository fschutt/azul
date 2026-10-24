//! The containers transform: the deflate streams inside gzip files, PNG images, ZIP and Office
//! files and PDFs are stored as their plain text plus the bytes that rebuild them exactly, and
//! the JPEG images inside go through Lepton. Always bit-exact, with the fallbacks when it cannot
//! run or does not pay.

mod common;

use azlin_compress::{decode, encode, plan, transform, Budget, CodecPath, Error, Kind, Options, TransformId};
use common::Rng;

const EMBEDDED: &[u8] = include_bytes!("fixtures/embedded.jpg");

fn options() -> Options {
    Options {
        threads: 1,
        ..Options::idle()
    }
}

fn round_trip(input: &[u8], options: &Options) -> azlin_compress::Report {
    let encoded = encode(input, options).expect("encode");
    let decoded = decode(&encoded.bytes, options).expect("decode");
    assert_eq!(&decoded[..], input, "bit-exact");
    assert_eq!(encoded.report.encoded_len, encoded.bytes.len() as u64);
    encoded.report
}

/// The container without any transform, for comparison.
fn without_transform(input: &[u8], options: &Options) -> u64 {
    let off = Options {
        transforms: false,
        ..options.clone()
    };
    encode(input, &off).unwrap().report.encoded_len
}

#[test]
fn a_gzip_file_is_stored_as_its_plain_text_and_comes_back_bit_exact() {
    let mut rng = Rng::new(40);
    let text = common::far_repeats(&mut rng, 48 * 1024, 6);
    let input = common::gzip(&text, 6);
    let report = round_trip(&input, &options());
    assert_eq!(report.kind, Kind::Gzip);
    assert_eq!(report.transform, TransformId::Containers, "{report:?}");
    assert_eq!(report.deflate.recompressed, 1, "{report:?}");
    assert_eq!(report.deflate.plain_bytes, text.len() as u64);
    // The plain text is compressed per segment (gzip itself is a dense kind).
    assert!(report.codec.compressed > 0, "{report:?}");
    assert!(report.encoded_len * 2 < report.original_len, "{report:?}");
    assert!(report.encoded_len * 2 < without_transform(&input, &options()));
}

#[test]
fn every_deflate_level_comes_back_bit_exact() {
    let mut rng = Rng::new(41);
    let text = common::far_repeats(&mut rng, 40 * 1024, 4);
    for level in 0..=9 {
        let input = common::gzip(&text, level);
        let report = round_trip(&input, &options());
        assert_eq!(report.transform, TransformId::Containers, "level {level}: {report:?}");
    }
}

#[test]
fn a_gzip_file_of_two_members_gets_both_streams_rebuilt() {
    let mut rng = Rng::new(42);
    let mut input = common::gzip(&common::far_repeats(&mut rng, 36 * 1024, 3), 9);
    input.extend(common::gzip(&common::far_repeats(&mut rng, 36 * 1024, 3), 1));
    let report = round_trip(&input, &options());
    assert_eq!(report.deflate.recompressed, 2, "{report:?}");
}

#[test]
fn a_png_has_its_idat_chunks_rebuilt_with_their_crcs() {
    let mut rng = Rng::new(43);
    // 256 x 128 RGB, a 48-row tile (37 KiB) repeated: deflate cannot reach the repeats.
    let input = common::png(&mut rng, 256, 128, 48, 8192);
    let report = round_trip(&input, &options());
    assert_eq!(report.kind, Kind::Png);
    assert_eq!(report.transform, TransformId::Containers, "{report:?}");
    assert_eq!(report.deflate.recompressed, 1, "{report:?}");
    assert!(report.encoded_len * 2 < report.original_len, "{report:?}");
}

#[test]
fn a_png_in_one_idat_chunk_and_in_many_small_ones_both_come_back() {
    let mut rng = Rng::new(44);
    // A 90-row tile of 128 RGB pixels is 34 KiB: its repeat lies beyond deflate's window.
    for chunk in [1 << 20, 1000, 7] {
        let input = common::png(&mut rng, 128, 192, 90, chunk);
        let report = round_trip(&input, &options());
        assert_eq!(report.transform, TransformId::Containers, "chunk {chunk}: {report:?}");
    }
}

#[test]
fn an_office_file_gets_its_members_rebuilt_and_its_deflated_photo_through_lepton() {
    let mut rng = Rng::new(45);
    let body = String::from_utf8(common::far_repeats(&mut rng, 40 * 1024, 4)).unwrap();
    let document = format!("<?xml version=\"1.0\"?><w:document><w:body>{body}</w:body></w:document>");
    let input = common::deflated_zip(
        &[
            ("[Content_Types].xml", b"<?xml version=\"1.0\"?><Types/>"),
            ("word/document.xml", document.as_bytes()),
            ("word/media/image1.jpeg", EMBEDDED),
        ],
        6,
    );
    let report = round_trip(&input, &options());
    assert_eq!(report.kind, Kind::Office);
    assert_eq!(report.transform, TransformId::Containers, "{report:?}");
    assert!(report.deflate.recompressed >= 2, "{report:?}");
    assert_eq!(report.jpeg.recompressed, 1, "the picture inside the deflated member: {report:?}");
    assert!(report.encoded_len * 10 < without_transform(&input, &options()) * 7, "{report:?}");
}

#[test]
fn a_pdf_gets_its_flate_streams_rebuilt_and_its_photo_through_lepton() {
    let mut rng = Rng::new(46);
    let content = common::far_repeats(&mut rng, 40 * 1024, 4);
    let flate = common::zlib(&content, 6);
    let input = common::pdf(
        &[
            ("/Filter /FlateDecode", &flate),
            ("/Type /XObject /Subtype /Image /Width 560 /Height 420 /Filter /DCTDecode", EMBEDDED),
        ],
        50,
    );
    let report = round_trip(&input, &options());
    assert_eq!(report.kind, Kind::Pdf);
    assert_eq!(report.transform, TransformId::Containers, "{report:?}");
    assert_eq!(report.deflate.recompressed, 1, "{report:?}");
    assert_eq!(report.jpeg.recompressed, 1, "{report:?}");
}

#[test]
fn a_pdf_with_only_a_photo_still_gets_the_photo_recompressed() {
    let input = common::pdf(
        &[("/Type /XObject /Subtype /Image /Width 560 /Height 420 /Filter /DCTDecode", EMBEDDED)],
        400,
    );
    let report = round_trip(&input, &options());
    assert_eq!(report.transform, TransformId::Containers, "{report:?}");
    assert_eq!((report.deflate.recompressed, report.jpeg.recompressed), (0, 1));
}

#[test]
fn a_member_that_claims_deflate_but_is_not_stays_as_it_is() {
    let mut rng = Rng::new(47);
    let good = common::far_repeats(&mut rng, 40 * 1024, 4);
    let mut input = common::deflated_zip(&[("a.txt", &good), ("b.bin", &rng.bytes(5000))], 6);
    // b.bin's first deflate block now has the reserved block type 3 (its header still says
    // method 8).
    let at = input.windows(5).position(|w| w == b"b.bin").unwrap() + 5;
    input[at] = 0x07;
    let report = round_trip(&input, &options());
    assert_eq!(report.deflate.recompressed, 1, "{report:?}");
    assert!(report.deflate.refused >= 1, "{report:?}");
}

#[test]
fn a_cut_gzip_file_comes_back_unchanged() {
    let mut rng = Rng::new(48);
    let full = common::gzip(&common::far_repeats(&mut rng, 40 * 1024, 4), 6);
    for cut in [10, 100, full.len() / 2, full.len() - 9] {
        round_trip(&full[..cut], &options());
    }
}

#[test]
fn no_time_no_memory_or_transforms_off_mean_no_rebuilt_stream() {
    let mut rng = Rng::new(49);
    let input = common::gzip(&common::far_repeats(&mut rng, 40 * 1024, 4), 6);
    let mut no_time = options();
    no_time.limits.transform_time = Budget::ZERO;
    let mut no_memory = options();
    no_memory.limits.max_memory = 1 << 20;
    let off = Options {
        transforms: false,
        ..options()
    };
    for options in [no_time, no_memory, off] {
        let report = round_trip(&input, &options);
        assert_eq!(report.transform, TransformId::None, "{report:?}");
        assert_eq!(report.deflate.recompressed, 0);
    }
}

#[test]
fn the_plan_sends_png_gzip_zip_and_documents_to_the_containers_transform() {
    let options = Options::upload();
    for (start, kind) in [
        (&b"\x1F\x8B\x08\x00\x00\x00\x00\x00"[..], Kind::Gzip),
        (b"\x89PNG\r\n\x1A\n", Kind::Png),
        (b"%PDF-1.7\n", Kind::Pdf),
    ] {
        let p = plan(start, 1 << 20, &options);
        assert_eq!(p.kind, kind);
        assert_eq!(p.transform, Some(TransformId::Containers), "{kind:?}");
        // The plain text the transform yields is compressed, though gzip and PNG are dense.
        assert_ne!(p.codecs, CodecPath::Skip, "{kind:?}");
    }
    let jpeg = plan(b"\xFF\xD8\xFF\xE0", 1 << 20, &options);
    assert_eq!((jpeg.transform, jpeg.codecs), (Some(TransformId::Jpeg), CodecPath::Skip));
}

#[test]
fn the_transform_ids_and_versions_are_fixed() {
    assert_eq!(TransformId::None.bytes(), (0, 0));
    assert_eq!(TransformId::Jpeg.bytes(), (1, 1));
    assert_eq!(TransformId::Containers.bytes(), (2, 1));
    assert_eq!(TransformId::from_bytes(2, 1), Ok(TransformId::Containers));
    assert!(matches!(TransformId::from_bytes(2, 2), Err(Error::Unsupported(_))));
    assert!(matches!(TransformId::from_bytes(3, 1), Err(Error::Unsupported(_))));
}

#[test]
fn the_forward_transform_alone_is_verified_and_inverts() {
    let mut rng = Rng::new(50);
    let input = common::gzip(&common::far_repeats(&mut rng, 40 * 1024, 4), 6);
    let options = options();
    let transform::Forward::Applied(t) = transform::forward(&input, Kind::Gzip, &options) else {
        panic!("the transform applies");
    };
    assert_eq!(t.id, TransformId::Containers);
    let mut out = Vec::new();
    let written = transform::inverse(
        TransformId::Containers,
        &mut std::io::Cursor::new(&t.bytes[..]),
        &mut out,
        &options.limits,
        1,
    )
    .unwrap();
    assert_eq!(written, input.len() as u64);
    assert_eq!(out, input);
}

#[test]
fn a_damaged_deflate_record_is_an_error_never_other_bytes() {
    let mut rng = Rng::new(51);
    let input = common::gzip(&common::far_repeats(&mut rng, 36 * 1024, 2), 6);
    let options = options();
    let encoded = encode(&input, &options).unwrap();
    assert_eq!(encoded.report.transform, TransformId::Containers);
    let bytes = &encoded.bytes;
    for at in (14..bytes.len() - 49).step_by(23) {
        for bit in [0x01u8, 0x40] {
            let mut damaged = bytes.to_vec();
            damaged[at] ^= bit;
            match decode(&damaged, &options) {
                Ok(out) => assert_eq!(&out[..], &input[..], "flip at {at}"),
                Err(e) => assert!(!e.to_string().contains("panicked"), "flip at {at}: {e}"),
            }
        }
    }
}

/// A random body: text, random bytes, a run, or a mix (so streams compress well, badly or not
/// at all, and land in stored, fixed and dynamic deflate blocks).
fn random_body(rng: &mut Rng) -> Vec<u8> {
    let len = rng.below(60_000) as usize;
    match rng.below(4) {
        0 => common::text(rng, len),
        1 => rng.bytes(len),
        2 => vec![rng.next() as u8; len],
        _ => {
            let mut out = common::text(rng, len / 2);
            out.extend(rng.bytes(len / 2));
            out
        }
    }
}

#[test]
fn random_deflate_containers_round_trip() {
    let mut rng = Rng::new(52);
    for case in 0..120 {
        let body = random_body(&mut rng);
        let level = rng.below(10) as u32;
        let input = match case % 4 {
            0 => common::gzip(&body, level),
            1 => {
                let (width, height) = (16 + rng.below(64) as u32, 1 + rng.below(64) as u32);
                let (tile, chunk) = (1 + rng.below(80) as u32, 1 + rng.below(9000) as usize);
                common::png(&mut rng, width, height, tile, chunk)
            }
            2 => {
                let other = random_body(&mut rng);
                common::deflated_zip(&[("a.xml", &body), ("b.bin", &other)], level)
            }
            _ => common::pdf(&[("/Filter /FlateDecode", &common::zlib(&body, level))], rng.below(30) as usize),
        };
        let options = Options {
            segment_size: [1024, 4096, 1 << 20][rng.below(3) as usize],
            threads: 1,
            ..Options::idle()
        };
        round_trip(&input, &options);
    }
}
