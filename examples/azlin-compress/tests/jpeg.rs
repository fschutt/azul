//! The JPEG transform: Lepton on JPEG files and on the JPEG images inside other files, always
//! bit-exact, with the fallbacks when it cannot run.
//!
//! The fixtures are synthetic pictures (gradients, shapes, noise) made for these tests.

mod common;

use azlin_compress::{decode, encode, transform, Budget, Kind, Options, TransformId};

const BASELINE: &[u8] = include_bytes!("fixtures/baseline.jpg");
const PROGRESSIVE: &[u8] = include_bytes!("fixtures/progressive.jpg");
const SMALL: &[u8] = include_bytes!("fixtures/small.jpg");
const EMBEDDED: &[u8] = include_bytes!("fixtures/embedded.jpg");

fn single_threaded() -> Options {
    Options {
        threads: 1,
        ..Options::idle()
    }
}

fn round_trip(input: &[u8], options: &Options) -> azlin_compress::Report {
    let encoded = encode(input, options).expect("encode");
    let decoded = decode(&encoded.bytes, options).expect("decode");
    assert_eq!(&decoded[..], input, "bit-exact");
    encoded.report
}

#[test]
fn a_jpeg_goes_through_lepton_bit_exact_and_smaller() {
    for (name, jpeg) in [("baseline", BASELINE), ("progressive", PROGRESSIVE)] {
        let report = round_trip(jpeg, &single_threaded());
        assert_eq!(report.kind, Kind::Jpeg, "{name}");
        assert_eq!(report.transform, TransformId::Jpeg, "{name}: {report:?}");
        assert_eq!(report.jpeg.recompressed, 1, "{name}");
        assert!(
            report.encoded_len * 100 <= report.original_len * 95,
            "{name}: {report:?}"
        );
        // Lepton's output is dense: the codec stage does not try to compress it again.
        assert_eq!(report.codec.compressed, 0, "{name}");
    }
}

#[test]
fn a_tiny_jpeg_comes_back_whether_or_not_lepton_pays() {
    // 3.5 KB: Lepton's own header may eat the saving; either way the bytes come back.
    let report = round_trip(SMALL, &single_threaded());
    assert_eq!(report.kind, Kind::Jpeg);
    assert!(report.encoded_len <= report.original_len + 9 + 14 + 49, "{report:?}");
}

#[test]
fn threads_change_nothing_but_the_speed() {
    let one = encode(BASELINE, &single_threaded()).unwrap();
    let four = encode(
        BASELINE,
        &Options {
            threads: 4,
            ..Options::idle()
        },
    )
    .unwrap();
    // Each decodes with any thread count.
    for threads in [1, 3, 8] {
        let options = Options {
            threads,
            ..Options::idle()
        };
        assert_eq!(&decode(&one.bytes, &options).unwrap()[..], BASELINE);
        assert_eq!(&decode(&four.bytes, &options).unwrap()[..], BASELINE);
    }
}

#[test]
fn a_second_picture_after_the_first_is_recompressed_too() {
    // Phones append a gain map or a depth map after the main picture's EOI.
    let mut input = BASELINE.to_vec();
    input.extend_from_slice(PROGRESSIVE);
    let report = round_trip(&input, &single_threaded());
    assert_eq!(report.jpeg.found, 2, "{report:?}");
    assert_eq!(report.jpeg.recompressed, 2, "{report:?}");
}

#[test]
fn trailing_bytes_after_the_picture_come_back_unchanged() {
    let mut input = BASELINE.to_vec();
    input.extend_from_slice(b"trailing metadata that is no JPEG \x00\x01\x02");
    let report = round_trip(&input, &single_threaded());
    assert_eq!(report.transform, TransformId::Jpeg);
}

#[test]
fn a_pdf_with_a_photo_gets_the_photo_recompressed() {
    let input = common::pdf(
        &[("/Type /XObject /Subtype /Image /Width 560 /Height 420 /Filter /DCTDecode", EMBEDDED)],
        400,
    );
    let report = round_trip(&input, &single_threaded());
    assert_eq!(report.kind, Kind::Pdf);
    // A PDF goes through the containers transform, which takes its pictures to Lepton.
    assert_eq!(report.transform, TransformId::Containers, "{report:?}");
    assert_eq!(report.jpeg.recompressed, 1);
    // The text around it is still compressed per segment.
    assert!(report.codec.compressed > 0, "{report:?}");
    let plain = encode(
        &input,
        &Options {
            transforms: false,
            ..single_threaded()
        },
    )
    .unwrap();
    assert!(report.encoded_len < plain.report.encoded_len, "{report:?} {:?}", plain.report);
}

#[test]
fn an_office_file_with_a_stored_photo_gets_the_photo_recompressed() {
    let input = common::stored_zip(&[
        ("[Content_Types].xml", b"<?xml version=\"1.0\"?><Types/>"),
        ("word/document.xml", b"<w:document><w:body>Holiday</w:body></w:document>"),
        ("word/media/image1.jpeg", EMBEDDED),
    ]);
    let report = round_trip(&input, &single_threaded());
    assert_eq!(report.kind, Kind::Office);
    assert_eq!(report.jpeg.recompressed, 1, "{report:?}");
}

#[test]
fn small_pictures_inside_other_files_are_left_alone() {
    // 13 KB is under the 16 KiB a picture inside another file must have.
    let input = common::pdf(&[("/Filter /DCTDecode", BASELINE)], 10);
    let report = round_trip(&input, &single_threaded());
    assert_eq!(report.transform, TransformId::None);
    assert_eq!(report.jpeg.found, 0);
    assert!(report.transform_skipped.is_some());
}

#[test]
fn a_broken_jpeg_is_compressed_per_segment_and_still_comes_back() {
    let cut = &BASELINE[..BASELINE.len() / 2];
    let report = round_trip(cut, &single_threaded());
    assert_eq!(report.kind, Kind::Jpeg);
    assert_eq!(report.transform, TransformId::None);
    assert!(report.transform_skipped.is_some(), "{report:?}");

    let mut garbled = BASELINE.to_vec();
    for i in (600..garbled.len() - 2).step_by(97) {
        garbled[i] ^= 0x5A;
    }
    round_trip(&garbled, &single_threaded());
}

#[test]
fn no_time_for_the_transform_means_no_transform() {
    let mut options = single_threaded();
    options.limits.transform_time = Budget::ZERO;
    let report = round_trip(BASELINE, &options);
    assert_eq!(report.transform, TransformId::None);
    assert_eq!(report.jpeg.over_budget, 0, "the stats belong to a transform that did not run");
    assert!(report.transform_skipped.is_some());
}

#[test]
fn the_pixel_cap_and_the_memory_cap_keep_lepton_off_big_pictures() {
    let mut options = single_threaded();
    options.limits.max_jpeg_pixels = 320 * 240 - 1;
    let report = round_trip(BASELINE, &options);
    assert_eq!(report.transform, TransformId::None);

    let mut options = single_threaded();
    options.limits.max_memory = 1 << 20;
    let report = round_trip(BASELINE, &options);
    assert_eq!(report.transform, TransformId::None);

    let mut options = single_threaded();
    options.limits.max_transform_input = 1024;
    let report = round_trip(BASELINE, &options);
    assert_eq!(report.transform, TransformId::None);
    assert_eq!(report.transform_skipped, None, "no transform was wanted at this size");
}

#[test]
fn transforms_can_be_turned_off() {
    let options = Options {
        transforms: false,
        ..single_threaded()
    };
    let report = round_trip(BASELINE, &options);
    assert_eq!(report.transform, TransformId::None);
    assert_eq!(report.encoded_len, report.original_len + 9 + 14 + 49);
}

#[test]
fn the_forward_transform_alone_is_verified_and_inverts() {
    let options = single_threaded();
    let transform::Forward::Applied(t) = transform::forward(BASELINE, Kind::Jpeg, &options) else {
        panic!("the transform applies");
    };
    assert_eq!(t.id, TransformId::Jpeg);
    let mut out = Vec::new();
    let written = transform::inverse(
        TransformId::Jpeg,
        &mut std::io::Cursor::new(&t.bytes[..]),
        &mut out,
        &options.limits,
        1,
    )
    .unwrap();
    assert_eq!(written, BASELINE.len() as u64);
    assert_eq!(out, BASELINE);
}

#[test]
fn a_damaged_lepton_record_is_an_error_never_other_bytes() {
    let options = single_threaded();
    let encoded = encode(BASELINE, &options).unwrap();
    let bytes = &encoded.bytes;
    // Flip bytes across the stored Lepton data (header 14, frame 9, record header 13).
    for at in (14 + 9 + 13..bytes.len() - 49).step_by(37) {
        let mut damaged = bytes.to_vec();
        damaged[at] ^= 0x01;
        match decode(&damaged, &options) {
            Ok(out) => assert_eq!(&out[..], BASELINE, "flip at {at}"),
            Err(_) => {}
        }
    }
}
