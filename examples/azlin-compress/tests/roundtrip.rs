//! decode(encode(x)) == x, for arbitrary bytes and every shape of input, and what the codec
//! stage chooses on the way.

mod common;

use std::io::Write;

use azlin_compress::{
    decode, encode,
    segment::{Choice, Codec},
    Budget, Kind, Options, StreamEncoder, TransformId,
};
use common::Rng;

fn small_segments() -> Options {
    Options {
        segment_size: 1024,
        ..Options::upload()
    }
}

fn round_trip(input: &[u8], options: &Options) -> azlin_compress::Report {
    let encoded = encode(input, options).expect("encode");
    let decoded = decode(&encoded.bytes, options).expect("decode");
    assert_eq!(&decoded[..], input, "decode(encode(x)) == x ({} bytes)", input.len());
    assert_eq!(encoded.report.encoded_len, encoded.bytes.len() as u64);
    encoded.report
}

#[test]
fn arbitrary_bytes_round_trip() {
    let mut rng = Rng::new(1);
    let options = small_segments();
    for case in 0..300 {
        let len = match case % 4 {
            0 => rng.below(64) as usize,
            1 => rng.below(4096) as usize,
            2 => 1024 * (1 + rng.below(5) as usize) + rng.below(3) as usize,
            _ => rng.below(20_000) as usize,
        };
        // Mix random runs with repeats, so both codecs and stored segments occur.
        let mut input = Vec::with_capacity(len);
        while input.len() < len {
            if rng.below(2) == 0 {
                let run = 1 + rng.below(300) as usize;
                input.extend(rng.bytes(run));
            } else {
                let byte = rng.next() as u8;
                input.extend(std::iter::repeat_n(byte, 1 + rng.below(500) as usize));
            }
        }
        input.truncate(len);
        round_trip(&input, &options);
    }
}

#[test]
fn every_shape_round_trips_with_every_budget_and_segment_size() {
    let mut rng = Rng::new(2);
    for segment_size in [1024u32, 4096, 1 << 20] {
        for budget in [Budget::ZERO, Budget::UNLIMITED] {
            let options = Options {
                segment_size,
                limits: azlin_compress::Limits {
                    codec_time: budget,
                    ..Default::default()
                },
                ..Options::upload()
            };
            for input in common::shapes(&mut rng, segment_size as usize) {
                round_trip(&input, &options);
            }
        }
    }
}

#[test]
fn the_stream_encoder_writes_what_decode_reads_whatever_the_write_sizes() {
    let mut rng = Rng::new(3);
    let options = small_segments();
    for input in common::shapes(&mut rng, 1024) {
        let mut encoder = StreamEncoder::new(Vec::new(), &options, Some(input.len() as u64)).unwrap();
        let mut rest = &input[..];
        while !rest.is_empty() {
            let take = (1 + rng.below(3000) as usize).min(rest.len());
            encoder.write_all(&rest[..take]).unwrap();
            rest = &rest[take..];
        }
        let (container, report) = encoder.finish().unwrap();
        assert_eq!(report.encoded_len, container.len() as u64);
        assert_eq!(report.transform, TransformId::None);
        let decoded = decode(&container, &options).unwrap();
        assert_eq!(&decoded[..], &input[..]);
    }
}

#[test]
fn compressible_text_is_compressed() {
    let mut rng = Rng::new(4);
    let input = common::text(&mut rng, 3 << 20);
    let report = round_trip(&input, &Options::upload());
    assert_eq!(report.kind, Kind::Text);
    assert!(report.encoded_len * 3 < report.original_len, "{report:?}");
    assert_eq!(report.codec.stored, 0);
    assert_eq!(report.codec.verify_failed, 0);
}

#[test]
fn random_data_is_stored_and_the_trial_gives_up_early() {
    let mut rng = Rng::new(5);
    let input = rng.bytes(5 << 20);
    let report = round_trip(&input, &Options::upload());
    assert_eq!(report.codec.compressed, 0);
    assert_eq!(report.codec.first_choice, None);
    // The frames and the end record only.
    assert!(report.encoded_len <= report.original_len + 5 * 9 + 64, "{report:?}");
}

#[test]
fn a_compressible_rest_after_an_incompressible_start_is_still_compressed() {
    // The first segment fails the trial; a later retry (every 8th segment) finds the text.
    let mut rng = Rng::new(6);
    let mut input = rng.bytes(1024);
    input.extend(common::text(&mut rng, 1024 * 40));
    let report = round_trip(&input, &small_segments());
    assert!(report.codec.compressed >= 30, "{report:?}");
    assert!(report.encoded_len * 10 < report.original_len * 7, "{report:?}");
}

#[test]
fn dense_kinds_are_stored_without_a_trial() {
    // An MP4 header over bytes that would compress: the kind says video, so nothing is tried.
    let mut input = b"\x00\x00\x00\x18ftypisom\x00\x00\x02\x00isomiso2".to_vec();
    input.extend(vec![0u8; 3 << 20]);
    let report = round_trip(&input, &Options::upload());
    assert_eq!(report.kind, Kind::Video);
    assert_eq!(report.codec.compressed, 0);
    assert_eq!(report.codec.first_choice, None);
}

#[test]
fn a_pdf_is_compressed_now_that_it_is_off_the_skip_list() {
    let input = common::pdf(&[], 20_000);
    let report = round_trip(&input, &Options::upload());
    assert_eq!(report.kind, Kind::Pdf);
    assert!(report.codec.compressed > 0, "{report:?}");
    assert!(report.encoded_len * 2 < report.original_len, "{report:?}");
}

#[test]
fn an_office_file_is_compressed_now_that_it_is_off_the_skip_list() {
    let mut rng = Rng::new(7);
    let xml = format!(
        "<?xml version=\"1.0\"?><w:document><w:body>{}</w:body></w:document>",
        String::from_utf8(common::text(&mut rng, 400_000)).unwrap()
    );
    let input = common::stored_zip(&[
        ("[Content_Types].xml", b"<?xml version=\"1.0\"?><Types/>"),
        ("word/document.xml", xml.as_bytes()),
    ]);
    let report = round_trip(&input, &Options::upload());
    assert_eq!(report.kind, Kind::Office);
    assert!(report.encoded_len * 2 < report.original_len, "{report:?}");
}

#[test]
fn no_time_means_the_fastest_level_and_unlimited_time_the_highest() {
    let mut rng = Rng::new(8);
    let input = common::json(&mut rng, 40_000);
    let mut options = Options::upload();
    options.dictionaries = false;
    options.limits.codec_time = Budget::ZERO;
    let fast = round_trip(&input, &options);
    assert_eq!(fast.codec.first_choice, Some(Choice::Zstd(1)), "{fast:?}");
    options.limits.codec_time = Budget::UNLIMITED;
    options.max_brotli_quality = 0;
    let best = round_trip(&input, &options);
    assert_eq!(best.codec.first_choice, Some(Choice::Zstd(19)), "{best:?}");
    assert!(best.encoded_len < fast.encoded_len, "{best:?} {fast:?}");
}

#[test]
fn text_takes_brotli_when_brotli_is_smaller_and_zstd_when_zstd_is() {
    let mut rng = Rng::new(9);
    let input = common::text(&mut rng, 600_000);
    let mut options = Options::upload();
    options.limits.codec_time = Budget::UNLIMITED;
    let report = round_trip(&input, &options);
    // Whatever won must be no larger than the other on the first segment.
    let first = &input[..input.len().min(1 << 20)];
    let zstd19 = zstd::bulk::compress(first, 19).unwrap().len();
    let brotli11 = {
        let mut out = Vec::new();
        let mut writer = brotli::CompressorWriter::new(&mut out, 64 * 1024, 11, 20);
        writer.write_all(first).unwrap();
        let _ = writer.into_inner();
        out.len()
    };
    match report.codec.first_choice {
        Some(Choice::Brotli(11)) => assert!(brotli11 <= zstd19, "{brotli11} {zstd19}"),
        Some(Choice::Zstd(19)) => assert!(zstd19 <= brotli11, "{brotli11} {zstd19}"),
        other => panic!("unexpected choice {other:?}"),
    }
}

#[test]
fn binary_never_tries_brotli() {
    let mut rng = Rng::new(10);
    let mut input = vec![0u8; 8];
    for _ in 0..50_000 {
        input.extend_from_slice(&(rng.below(500) as u32).to_le_bytes());
    }
    let mut options = Options::upload();
    options.limits.codec_time = Budget::UNLIMITED;
    let report = round_trip(&input, &options);
    assert_eq!(report.kind, Kind::Binary);
    assert!(matches!(report.codec.first_choice, Some(Choice::Zstd(_))), "{report:?}");
}

#[test]
fn the_codec_bytes_are_the_ones_azl1_uses() {
    assert_eq!(Codec::Stored.byte(), 0);
    assert_eq!(Codec::Zstd.byte(), 1);
    assert_eq!(Codec::Brotli.byte(), 2);
    assert_eq!(Codec::ZstdDict.byte(), 4);
    assert!(matches!(
        Codec::from_byte(3),
        Err(azlin_compress::Error::Unsupported(_))
    ));
    for byte in 0..=255u8 {
        match Codec::from_byte(byte) {
            Ok(codec) => assert_eq!(codec.byte(), byte),
            Err(e) => assert!(matches!(e, azlin_compress::Error::Unsupported(_))),
        }
    }
}

#[test]
fn the_plan_says_what_encode_will_do() {
    let options = Options::upload();
    let jpeg = azlin_compress::plan(b"\xFF\xD8\xFF\xE0", 3 << 20, &options);
    assert_eq!(jpeg.kind, Kind::Jpeg);
    assert_eq!(jpeg.transform, Some(TransformId::Jpeg));
    assert_eq!(jpeg.codecs, azlin_compress::CodecPath::Skip);
    let huge_jpeg = azlin_compress::plan(b"\xFF\xD8\xFF\xE0", 1 << 34, &options);
    assert_eq!(huge_jpeg.transform, None);
    let video = azlin_compress::plan(b"\x00\x00\x00\x18ftypisom", 1 << 30, &options);
    assert_eq!((video.transform, video.codecs), (None, azlin_compress::CodecPath::Skip));
    let text = azlin_compress::plan(b"just some words\n", 1 << 20, &options);
    assert_eq!(text.codecs, azlin_compress::CodecPath::Text);
    let binary = azlin_compress::plan(b"\x00\x01\x02", 1 << 20, &options);
    assert_eq!((binary.transform, binary.codecs), (None, azlin_compress::CodecPath::Trial));
    let off = Options {
        transforms: false,
        ..Options::upload()
    };
    assert_eq!(azlin_compress::plan(b"%PDF-1.7", 1 << 20, &off).transform, None);
}

#[test]
fn the_estimate_is_cheap_and_close() {
    let mut rng = Rng::new(11);
    let options = Options::upload();
    let input = common::text(&mut rng, 2 << 20);
    let estimate = azlin_compress::estimate(&input[..256 * 1024], input.len() as u64, &options);
    let encoded = encode(&input, &options).unwrap();
    assert!(estimate.measured);
    assert!(estimate.low_len <= estimate.expected_len && estimate.expected_len <= estimate.high_len);
    let actual = encoded.bytes.len() as f64;
    let expected = estimate.expected_len as f64;
    assert!((actual - expected).abs() < expected * 0.5, "{estimate:?} vs {actual}");

    let random = rng.bytes(100_000);
    let none = azlin_compress::estimate(&random, 1 << 30, &options);
    assert_eq!(none.expected_len, 1 << 30);

    let jpeg = azlin_compress::estimate(b"\xFF\xD8\xFF\xE0\x00\x10JFIF", 10_000_000, &options);
    assert_eq!((jpeg.kind, jpeg.expected_len, jpeg.measured), (Kind::Jpeg, 7_900_000, false));
    let video = azlin_compress::estimate(b"\x00\x00\x00\x18ftypisom", 10_000_000, &options);
    assert_eq!(video.expected_len, 10_000_000);
}

#[test]
fn encode_and_decode_are_send_and_options_are_sync() {
    fn send<T: Send>() {}
    fn sync<T: Sync>() {}
    send::<StreamEncoder<Vec<u8>>>();
    send::<azlin_compress::segment::SegmentEncoder>();
    send::<azlin_compress::Encoded>();
    sync::<Options>();
    let handle = std::thread::spawn(|| {
        let options = Options::upload();
        let encoded = encode(b"from another thread", &options).unwrap();
        decode(&encoded.bytes, &options).unwrap().to_vec()
    });
    assert_eq!(handle.join().unwrap(), b"from another thread");
}

#[test]
fn small_files_are_packed_as_one_record_each() {
    use azlin_compress::{decode_member, encode_member, MEMBER_CONTAINER};
    let mut rng = Rng::new(12);
    let options = Options::upload();
    let jpeg: &[u8] = include_bytes!("fixtures/baseline.jpg");
    let json = common::json(&mut rng, 50);
    let random = rng.bytes(3000);
    let cases: [(&[u8], &[u8]); 4] = [
        (&json, &[1, 2, 4]),
        (&random, &[0]),
        (jpeg, &[MEMBER_CONTAINER]),
        (b"", &[0]),
    ];
    for (file, codecs) in cases {
        let member = encode_member(file, &options).unwrap();
        assert!(codecs.contains(&member.codec), "codec {} for {} bytes", member.codec, file.len());
        let back = decode_member(member.codec, &member.data, file.len(), &options).unwrap();
        assert_eq!(&back[..], file);
        // The entry's size is the cap: a member never decodes to more.
        if !file.is_empty() {
            assert!(decode_member(member.codec, &member.data, file.len() - 1, &options).is_err());
        }
    }
}
