//! The fragmented-MP4 muxer, the H.264 and Opus helpers and the scanner, on real parameter sets
//! (VideoToolbox's and x264's, made with ffmpeg on the machine this was written on) and on
//! streams made up here. No window, no azul, no files.

use std::io::Cursor;

use super::*;

// ---- fixtures ----

fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

/// VideoToolbox, 1280x720, High profile level 3.1, no VUI.
fn sps_vt_720() -> Vec<u8> {
    hex("2764001fac56805005b9")
}

/// VideoToolbox, 2880x1800: 1800 is not a multiple of 16, so the picture is cropped.
fn sps_vt_2880() -> Vec<u8> {
    hex("27640033ac56802d00e3f950")
}

/// x264, 320x240, High profile level 1.3, with a VUI (timing, no colour).
fn sps_x264_320() -> Vec<u8> {
    hex("6764000dacd94141fb0110000003001000000303c0f1429960")
}

fn pps() -> Vec<u8> {
    hex("28ee3cb0")
}

/// An Annex-B stream of these NAL units, each behind a four-byte start code.
fn annexb(nals: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    for n in nals {
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(n);
    }
    out
}

/// A keyframe as an encoder hands it out: SPS, PPS, an IDR slice of `size` bytes whose first byte
/// is `tag`.
fn keyframe(tag: u8, size: usize) -> Vec<u8> {
    let mut idr = vec![0x65, tag];
    idr.resize(size.max(2), 0x5a);
    annexb(&[&sps_vt_720(), &pps(), &idr])
}

/// A delta picture: one non-IDR slice.
fn delta(tag: u8, size: usize) -> Vec<u8> {
    let mut slice = vec![0x41, tag];
    slice.resize(size.max(2), 0x3c);
    annexb(&[&slice])
}

fn video_config() -> VideoConfig {
    VideoConfig::from_keyframe(&keyframe(0, 40)).unwrap()
}

/// A raw Opus packet of 20 ms (mono, CELT full band): the table-of-contents byte and a payload.
fn opus_packet(tag: u8) -> Vec<u8> {
    vec![0xf8, tag, 1, 2, 3, 4, 5, 6]
}

/// A bit writer for making SPS units the encoders here do not.
#[derive(Default)]
struct Bw(Vec<bool>);

impl Bw {
    fn u(&mut self, n: u32, v: u32) -> &mut Self {
        for i in (0..n).rev() {
            self.0.push((v >> i) & 1 == 1);
        }
        self
    }
    fn ue(&mut self, v: u32) -> &mut Self {
        let x = v + 1;
        let len = 32 - x.leading_zeros();
        for _ in 1..len {
            self.0.push(false);
        }
        self.u(len, x)
    }
    fn se(&mut self, v: i32) -> &mut Self {
        self.ue(if v > 0 { (2 * v - 1) as u32 } else { (-2 * v) as u32 })
    }
    /// The NAL unit: the header, the three bytes of profile, constraints and level, then the bits
    /// with the stop bit, the padding and the emulation-prevention bytes.
    fn nal(&self, profile: u8, level: u8) -> Vec<u8> {
        let mut bits = self.0.clone();
        bits.push(true);
        while bits.len() % 8 != 0 {
            bits.push(false);
        }
        let mut rbsp: Vec<u8> = bits.chunks(8).map(|c| c.iter().fold(0_u8, |a, b| (a << 1) | u8::from(*b))).collect();
        let mut out = vec![0x67, profile, 0, level];
        let mut zeros = 0;
        for b in rbsp.drain(..) {
            if zeros >= 2 && b <= 3 {
                out.push(3);
                zeros = 0;
            }
            zeros = if b == 0 { zeros + 1 } else { 0 };
            out.push(b);
        }
        out
    }
}

/// A 4:2:0 SPS of `width` x `height` (the pictures are padded to whole macroblocks and cropped),
/// with a colour description when `colour` is given.
fn made_sps(profile: u8, width: u32, height: u32, colour: Option<(u8, u8, u8, bool)>) -> Vec<u8> {
    let mut b = Bw::default();
    b.ue(0); // seq_parameter_set_id
    if profile >= 100 {
        b.ue(1).ue(0).ue(0).u(1, 0).u(1, 0); // 4:2:0, 8 bit, no bypass, no scaling matrix
    }
    b.ue(0); // log2_max_frame_num_minus4
    b.ue(2); // pic_order_cnt_type 2
    b.ue(1).u(1, 0); // max_num_ref_frames, gaps
    let (mbs_w, mbs_h) = (width.div_ceil(16), height.div_ceil(16));
    b.ue(mbs_w - 1).ue(mbs_h - 1).u(1, 1).u(1, 1); // frame_mbs_only, direct_8x8
    let (pad_w, pad_h) = (mbs_w * 16 - width, mbs_h * 16 - height);
    if pad_w > 0 || pad_h > 0 {
        b.u(1, 1).ue(0).ue(pad_w / 2).ue(0).ue(pad_h / 2);
    } else {
        b.u(1, 0);
    }
    match colour {
        Some((p, t, m, full)) => {
            b.u(1, 1); // vui_parameters_present_flag
            b.u(1, 1).u(8, 1); // aspect ratio: 1:1
            b.u(1, 0); // no overscan info
            b.u(1, 1).u(3, 5).u(1, u32::from(full)).u(1, 1).u(8, u32::from(p)).u(8, u32::from(t)).u(8, u32::from(m));
        }
        None => {
            b.u(1, 0);
        }
    }
    b.nal(profile, 40)
}

/// A file as a list of top-level boxes: `(name, offset, size)`.
fn top_boxes(bytes: &[u8]) -> Vec<([u8; 4], usize, usize)> {
    let mut out = Vec::new();
    let mut at = 0;
    while at + 8 <= bytes.len() {
        let size = u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as usize;
        out.push(([bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]], at, size));
        assert!(size >= 8 && at + size <= bytes.len(), "box at {at} of size {size} runs past the file ({})", bytes.len());
        at += size;
    }
    assert_eq!(at, bytes.len(), "the boxes end where the file ends");
    out
}

fn find<'a>(mut data: &'a [u8], path: &[&[u8; 4]]) -> Option<&'a [u8]> {
    for name in path {
        data = child(data, name)?;
    }
    Some(data)
}

/// Writes `frames` video samples (a keyframe every `gop`), `step_us` apart, and finishes the file.
fn video_file(frames: usize, gop: usize, step_us: u64) -> (Vec<u8>, MuxStats) {
    let (mut w, mut file) = FragmentWriter::new(TrackConfig::Video(video_config()), 1_764_108_000, Limits::default());
    for i in 0..frames {
        let key = i % gop == 0;
        let au = if key { keyframe(i as u8, 300) } else { delta(i as u8, 100) };
        let sample = annexb_to_avcc(&au);
        file.extend(w.push(i as u64 * step_us, sample.bytes, key).unwrap());
    }
    file.extend(w.finish(frames as u64 * step_us));
    (file, w.stats())
}

fn opus_file(packets: usize) -> (Vec<u8>, MuxStats) {
    let (mut w, mut file) = FragmentWriter::new(TrackConfig::Opus(OpusConfig::mono_48k()), 0, Limits::default());
    for i in 0..packets {
        file.extend(w.push_ticks(i as u64 * 960, opus_packet(i as u8), true).unwrap());
    }
    file.extend(w.finish(packets as u64 * 20_000));
    (file, w.stats())
}

// ---- Annex-B and AVCC ----

#[test]
fn the_nal_units_of_an_annexb_stream_come_out_without_start_codes_or_padding() {
    let stream = [
        vec![0, 0, 0, 1, 0x67, 1, 2, 3],
        vec![0, 0, 1, 0x68, 9, 8, 0, 0], // a three-byte start code; two zero bytes of padding after
        vec![0, 0, 0, 1, 0x65, 7],
    ]
    .concat();
    let nals = annexb_nals(&stream);
    assert_eq!(nals, vec![&[0x67, 1, 2, 3][..], &[0x68, 9, 8][..], &[0x65, 7][..]]);
    assert!(annexb_nals(&[]).is_empty());
    assert!(annexb_nals(&[1, 2, 3, 4]).is_empty(), "no start code, no NAL unit");
}

#[test]
fn a_keyframe_becomes_a_sample_with_its_parameter_sets_set_aside() {
    let sample = annexb_to_avcc(&keyframe(7, 20));
    assert!(sample.keyframe);
    assert_eq!(sample.sps.as_deref(), Some(&sps_vt_720()[..]));
    assert_eq!(sample.pps.as_deref(), Some(&pps()[..]));
    // One NAL left in the sample: the IDR slice behind its four-byte length.
    assert_eq!(&sample.bytes[..4], &20_u32.to_be_bytes());
    assert_eq!(sample.bytes.len(), 4 + 20);
    assert_eq!(sample.bytes[4], 0x65);
}

#[test]
fn a_delta_picture_is_no_keyframe_and_an_access_unit_delimiter_is_dropped() {
    let au = annexb(&[&[0x09, 0xf0], &[0x06, 5, 5], &[0x41, 1, 2, 3]]); // AUD, SEI, slice
    let sample = annexb_to_avcc(&au);
    assert!(!sample.keyframe);
    assert!(sample.sps.is_none() && sample.pps.is_none());
    assert_eq!(sample.bytes, vec![0, 0, 0, 3, 0x06, 5, 5, 0, 0, 0, 4, 0x41, 1, 2, 3]);
}

// ---- the sequence parameter set ----

#[test]
fn the_parameter_sets_of_videotoolbox_and_x264_give_their_size_profile_and_level() {
    let vt = parse_sps(&sps_vt_720()).unwrap();
    assert_eq!((vt.width, vt.height), (1280, 720));
    assert_eq!((vt.profile_idc, vt.level_idc), (100, 31));
    assert_eq!((vt.chroma_format_idc, vt.bit_depth_luma, vt.bit_depth_chroma), (1, 8, 8));
    assert_eq!(vt.colour, None);

    let big = parse_sps(&sps_vt_2880()).unwrap();
    assert_eq!((big.width, big.height), (2880, 1800), "1808 rows coded, cropped to 1800");
    assert_eq!(big.level_idc, 51);

    let x = parse_sps(&sps_x264_320()).unwrap();
    assert_eq!((x.width, x.height), (320, 240));
    assert_eq!((x.profile_idc, x.level_idc), (100, 13));
}

#[test]
fn a_made_sps_round_trips_its_size_across_profiles_and_crops() {
    for profile in [66, 77, 100] {
        for (w, h) in [(320, 240), (1280, 720), (1920, 1080), (2560, 1664), (2880, 1800), (642, 478)] {
            let sps = parse_sps(&made_sps(profile, w, h, None)).unwrap_or_else(|e| panic!("{profile} {w}x{h}: {e}"));
            assert_eq!((sps.width, sps.height), (w, h), "profile {profile}, asked {w}x{h}");
            assert_eq!(sps.profile_idc, profile);
        }
    }
}

#[test]
fn the_colour_description_of_a_vui_is_read() {
    let sps = parse_sps(&made_sps(100, 1280, 720, Some((1, 13, 1, false)))).unwrap();
    assert_eq!(sps.colour, Some(Colour { primaries: 1, transfer: 13, matrix: 1, full_range: false }));
    let full = parse_sps(&made_sps(100, 1280, 720, Some((9, 16, 9, true)))).unwrap();
    assert_eq!(full.colour, Some(Colour { primaries: 9, transfer: 16, matrix: 9, full_range: true }));
}

#[test]
fn a_parameter_set_that_is_none_or_cut_short_is_refused_in_words() {
    assert!(parse_sps(&[0x68, 1, 2, 3, 4]).unwrap_err().contains("not a sequence parameter set"));
    assert!(parse_sps(&[0x67, 100]).unwrap_err().contains("not a sequence parameter set"));
    let cut = &sps_vt_720()[..6];
    assert!(parse_sps(cut).is_err(), "cut off in the middle of the fields");
}

// ---- Opus ----

#[test]
fn an_opus_packet_says_how_many_samples_it_holds() {
    assert_eq!(opus_packet_samples(&[0xf8, 1, 2]), Some(960), "CELT full band, 20 ms, one frame");
    assert_eq!(opus_packet_samples(&[0xfc, 1, 2]), Some(960), "the stereo bit does not change it");
    assert_eq!(opus_packet_samples(&[0xe0]), Some(120), "CELT full band, 2.5 ms");
    assert_eq!(opus_packet_samples(&[0x78]), Some(960), "hybrid full band, 20 ms");
    assert_eq!(opus_packet_samples(&[0x08]), Some(960), "SILK narrow band, 20 ms (config 1)");
    assert_eq!(opus_packet_samples(&[0x18]), Some(2880), "SILK narrow band, 60 ms (config 3)");
    assert_eq!(opus_packet_samples(&[0xf9, 1, 2]), Some(1920), "two frames of 20 ms");
    assert_eq!(opus_packet_samples(&[0xfa, 1, 2]), Some(1920), "two frames of different sizes");
    assert_eq!(opus_packet_samples(&[0xfb, 3, 1]), Some(2880), "code 3: the next byte counts three frames");
}

#[test]
fn a_packet_that_cannot_be_opus_has_no_length() {
    assert_eq!(opus_packet_samples(&[]), None);
    assert_eq!(opus_packet_samples(&[0xfb]), None, "code 3 without its count byte");
    assert_eq!(opus_packet_samples(&[0xfb, 0, 1]), None, "zero frames");
    assert_eq!(opus_packet_samples(&[0xfb, 7, 1]), None, "7 x 20 ms is over the 120 ms an Opus packet may hold");
}

// ---- the head of a file ----

#[test]
fn a_video_file_starts_with_ftyp_and_a_moov_that_describes_the_track() {
    let (_w, head) = FragmentWriter::new(TrackConfig::Video(video_config()), 1_764_108_000, Limits::default());
    let boxes = top_boxes(&head);
    assert_eq!(boxes.iter().map(|b| b.0).collect::<Vec<_>>(), vec![*b"ftyp", *b"moov"]);
    assert_eq!(&head[8..12], b"iso5");
    let moov = &head[boxes[1].1 + 8..boxes[1].1 + boxes[1].2];
    for name in [b"mvhd", b"trak", b"mvex"] {
        assert!(child(moov, name).is_some(), "moov has {}", String::from_utf8_lossy(name));
    }
    let hdlr = find(moov, &[b"trak", b"mdia", b"hdlr"]).unwrap();
    assert_eq!(&hdlr[8..12], b"vide");
    let mdhd = find(moov, &[b"trak", b"mdia", b"mdhd"]).unwrap();
    assert_eq!(be32(mdhd, 12), Some(VIDEO_TIMESCALE));
    let stsd = find(moov, &[b"trak", b"mdia", b"minf", b"stbl", b"stsd"]).unwrap();
    let (kind, entry) = children(&stsd[8..]).into_iter().next().unwrap();
    assert_eq!(&kind, b"avc1");
    assert_eq!(u16::from_be_bytes([entry[24], entry[25]]), 1280);
    assert_eq!(u16::from_be_bytes([entry[26], entry[27]]), 720);
    let avcc = child(&entry[78..], b"avcC").unwrap();
    assert_eq!(&avcc[..4], &[1, 100, 0, 31], "version, profile, compatibility, level");
    assert_eq!(avcc[4], 0xff, "four-byte NAL lengths");
    assert_eq!(avcc[5], 0xe1, "one SPS");
    assert_eq!(&avcc[6..8], &(sps_vt_720().len() as u16).to_be_bytes());
    assert_eq!(&avcc[avcc.len() - 4..], &[0xfd, 0xf8, 0xf8, 0], "a High profile says its chroma format and bit depths");
}

#[test]
fn a_colour_description_in_the_sps_becomes_a_colr_box() {
    let au = annexb(&[&made_sps(100, 1280, 720, Some((1, 1, 1, false))), &pps(), &[0x65, 1, 2, 3]]);
    let config = VideoConfig::from_keyframe(&au).unwrap();
    let (_w, head) = FragmentWriter::new(TrackConfig::Video(config), 0, Limits::default());
    let boxes = top_boxes(&head);
    let moov = &head[boxes[1].1 + 8..boxes[1].1 + boxes[1].2];
    let stsd = find(moov, &[b"trak", b"mdia", b"minf", b"stbl", b"stsd"]).unwrap();
    let (_, entry) = children(&stsd[8..]).into_iter().next().unwrap();
    let colr = child(&entry[78..], b"colr").expect("the colour tags are in the file");
    assert_eq!(&colr[..4], b"nclx");
    assert_eq!(u16::from_be_bytes([colr[4], colr[5]]), 1);
    assert_eq!(u16::from_be_bytes([colr[8], colr[9]]), 1);
    assert_eq!(colr[10], 0, "limited range");
}

#[test]
fn a_default_colour_is_written_when_the_sps_has_none_and_the_sps_colour_wins() {
    let colr_of = |config: VideoConfig| {
        let (_w, head) = FragmentWriter::new(TrackConfig::Video(config), 0, Limits::default());
        let boxes = top_boxes(&head);
        let moov = &head[boxes[1].1 + 8..boxes[1].1 + boxes[1].2];
        let stsd = find(moov, &[b"trak", b"mdia", b"minf", b"stbl", b"stsd"]).unwrap();
        let (_, entry) = children(&stsd[8..]).into_iter().next().unwrap();
        child(&entry[78..], b"colr").map(|c| (u16::from_be_bytes([c[4], c[5]]), u16::from_be_bytes([c[8], c[9]]), c[10]))
    };
    // VideoToolbox's SPS has no VUI: no tags, unless the track is given a default.
    assert_eq!(colr_of(video_config()), None);
    assert_eq!(colr_of(video_config().with_default_colour(Colour::bt709())), Some((1, 1, 0)));
    assert_eq!(colr_of(video_config().with_default_colour(Colour::bt601())), Some((6, 6, 0)));
    // An SPS that says its colour keeps it.
    let au = annexb(&[&made_sps(100, 1280, 720, Some((9, 16, 9, true))), &pps(), &[0x65, 1, 2, 3]]);
    let own = VideoConfig::from_keyframe(&au).unwrap().with_default_colour(Colour::bt709());
    assert_eq!(colr_of(own), Some((9, 9, 0x80)));
}

#[test]
fn an_opus_file_describes_a_sound_track_at_48_khz() {
    let (_w, head) = FragmentWriter::new(TrackConfig::Opus(OpusConfig::mono_48k()), 0, Limits::default());
    let boxes = top_boxes(&head);
    let moov = &head[boxes[1].1 + 8..boxes[1].1 + boxes[1].2];
    assert_eq!(&find(moov, &[b"trak", b"mdia", b"hdlr"]).unwrap()[8..12], b"soun");
    assert_eq!(be32(find(moov, &[b"trak", b"mdia", b"mdhd"]).unwrap(), 12), Some(OPUS_TIMESCALE));
    assert!(find(moov, &[b"trak", b"mdia", b"minf", b"smhd"]).is_some());
    let stsd = find(moov, &[b"trak", b"mdia", b"minf", b"stbl", b"stsd"]).unwrap();
    let (kind, entry) = children(&stsd[8..]).into_iter().next().unwrap();
    assert_eq!(&kind, b"Opus");
    assert_eq!(u16::from_be_bytes([entry[16], entry[17]]), 1, "one channel");
    assert_eq!(u32::from_be_bytes([entry[24], entry[25], entry[26], entry[27]]), 48_000 << 16);
    let dops = child(&entry[28..], b"dOps").unwrap();
    assert_eq!(dops[0], 0, "version 0");
    assert_eq!(dops[1], 1, "one channel");
    assert_eq!(u16::from_be_bytes([dops[2], dops[3]]), 312, "pre-skip");
    assert_eq!(u32::from_be_bytes([dops[4], dops[5], dops[6], dops[7]]), 48_000);
    assert_eq!(dops[10], 0, "channel mapping family 0");
}

// ---- fragments ----

#[test]
fn five_seconds_of_video_are_cut_into_fragments_at_the_keyframes() {
    // 150 frames at 30 fps, a keyframe every two seconds (60 frames): 0, 60, 120.
    let (file, stats) = video_file(150, 60, 33_333);
    let boxes = top_boxes(&file);
    let kinds: Vec<[u8; 4]> = boxes.iter().map(|b| b.0).collect();
    assert_eq!(
        kinds,
        vec![*b"ftyp", *b"moov", *b"moof", *b"mdat", *b"moof", *b"mdat", *b"moof", *b"mdat", *b"mfra"],
        "three fragments, then the index"
    );
    assert_eq!(stats.fragments, 3);
    assert_eq!(stats.samples, 150);
    assert_eq!(stats.bytes, file.len() as u64, "the stats count every byte handed out");

    let scan = scan(&mut Cursor::new(&file)).unwrap();
    assert_eq!(scan.fragments, 3);
    assert_eq!(scan.samples, 150);
    assert!(scan.finished && !scan.truncated);
    assert_eq!(scan.whole_end, file.len() as u64);
    assert_eq!((scan.kind, scan.codec), (Some(TrackKind::Video), Some(*b"avc1")));
    assert_eq!((scan.width, scan.height), (1280, 720));
    assert_eq!(scan.timescale, VIDEO_TIMESCALE);
    // The last frame lasts until the end of the file's timeline: 150 x 33.333 ms, to within the
    // rounding of a tick (11 us).
    assert!(scan.duration_us().abs_diff(4_999_950) <= 12, "{}", scan.duration_us());
}

#[test]
fn the_data_offset_of_a_fragment_points_at_its_first_sample() {
    let (file, _) = video_file(70, 60, 33_333);
    let boxes = top_boxes(&file);
    for &(kind, at, size) in boxes.iter().filter(|b| &b.0 == b"moof") {
        let moof = &file[at + 8..at + size];
        let trun = find(moof, &[b"traf", b"trun"]).unwrap();
        let data_offset = be32(trun, 8).unwrap() as usize;
        assert_eq!(&kind, b"moof");
        // The media starts right after the moof's end and the mdat header.
        assert_eq!(at + data_offset, at + size + 8);
        assert_eq!(&file[at + size + 4..at + size + 8], b"mdat");
    }
}

#[test]
fn the_media_of_a_fragment_is_its_samples_in_order() {
    let (file, _) = video_file(3, 60, 40_000);
    // One fragment: the keyframe and two deltas. Its mdat holds them one after the other.
    let boxes = top_boxes(&file);
    let mdat = boxes.iter().find(|b| &b.0 == b"mdat").unwrap();
    let payload = &file[mdat.1 + 8..mdat.1 + mdat.2];
    let first = annexb_to_avcc(&keyframe(0, 300)).bytes;
    let second = annexb_to_avcc(&delta(1, 100)).bytes;
    let third = annexb_to_avcc(&delta(2, 100)).bytes;
    assert_eq!(payload, [first, second, third].concat());
    let moof = boxes.iter().find(|b| &b.0 == b"moof").unwrap();
    let trun = find(&file[moof.1 + 8..moof.1 + moof.2], &[b"traf", b"trun"]).unwrap();
    assert_eq!(be32(trun, 4), Some(3), "three samples");
}

#[test]
fn a_still_screen_is_one_long_sample() {
    // The screen changes at 0 s, then at 20 s, then 100 ms later: variable frame rate.
    let (mut w, mut file) = FragmentWriter::new(TrackConfig::Video(video_config()), 0, Limits::default());
    for (i, t) in [0_u64, 20_000_000, 20_100_000].into_iter().enumerate() {
        let sample = annexb_to_avcc(&keyframe(i as u8, 50));
        file.extend(w.push(t, sample.bytes, true).unwrap());
    }
    file.extend(w.finish(20_500_000));
    let s = scan(&mut Cursor::new(&file)).unwrap();
    assert_eq!(s.samples, 3);
    assert_eq!(s.duration_us(), 20_500_000);
    // The first sample lasts the whole 20 s (it ends where the second begins).
    let boxes = top_boxes(&file);
    let moof = boxes.iter().find(|b| &b.0 == b"moof").unwrap();
    let trun = find(&file[moof.1 + 8..moof.1 + moof.2], &[b"traf", b"trun"]).unwrap();
    assert_eq!(be32(trun, 12), Some(20 * VIDEO_TIMESCALE), "the first entry's duration, in ticks");
}

#[test]
fn a_time_that_does_not_rise_is_moved_up_one_tick_and_counted() {
    let (mut w, _) = FragmentWriter::new(TrackConfig::Video(video_config()), 0, Limits::default());
    let key = annexb_to_avcc(&keyframe(0, 50)).bytes;
    let d = annexb_to_avcc(&delta(1, 50)).bytes;
    w.push(1_000_000, key, true).unwrap();
    w.push(1_000_000, d.clone(), false).unwrap(); // the same time
    w.push(900_000, d, false).unwrap(); // and one before
    assert_eq!(w.stats().bumped, 2);
    let file_tail = w.finish(2_000_000);
    assert!(!file_tail.is_empty());
}

#[test]
fn audio_packets_are_fragmented_by_time_and_last_exactly_their_samples() {
    let (file, stats) = opus_file(100); // two seconds of 20 ms packets
    assert_eq!(stats.samples, 100);
    assert_eq!(stats.fragments, 2, "a fragment closes at the first packet after a second");
    let s = scan(&mut Cursor::new(&file)).unwrap();
    assert_eq!((s.kind, s.codec), (Some(TrackKind::Audio), Some(*b"Opus")));
    assert_eq!(s.samples, 100);
    assert_eq!(s.end_ticks, 100 * 960);
    assert_eq!(s.duration_us(), 2_000_000);
    // Every packet is a sync sample and lasts 960 ticks.
    let boxes = top_boxes(&file);
    let moof = boxes.iter().find(|b| &b.0 == b"moof").unwrap();
    let trun = find(&file[moof.1 + 8..moof.1 + moof.2], &[b"traf", b"trun"]).unwrap();
    assert_eq!(be32(trun, 12), Some(960));
    assert_eq!(be32(trun, 20), Some(0x0200_0000));
}

#[test]
fn a_fragment_that_never_meets_a_keyframe_is_closed_by_its_length() {
    let limits = Limits { max_fragment_us: 1_000_000, ..Limits::default() };
    let (mut w, mut file) = FragmentWriter::new(TrackConfig::Video(video_config()), 0, limits);
    file.extend(w.push(0, annexb_to_avcc(&keyframe(0, 50)).bytes, true).unwrap());
    for i in 1..90_u64 {
        file.extend(w.push(i * 33_333, annexb_to_avcc(&delta(i as u8, 50)).bytes, false).unwrap());
    }
    file.extend(w.finish(90 * 33_333));
    let s = scan(&mut Cursor::new(&file)).unwrap();
    assert!(s.fragments >= 3, "three seconds in one-second fragments, got {}", s.fragments);
    assert_eq!(s.samples, 90);
}

/// A chunked track: 15 pictures a second for 30 s with a keyframe every 5 s, the keyframes a few
/// milliseconds early as a clock that is read in steps makes them. With the limits of a chunk (`chunked`:
/// a fragment of the chunk's length, closing at its keyframe) every chunk is one fragment, six of them;
/// with the limits of the other tracks the 4-second cap cuts the chunks at pictures that begin nothing.
///
/// Mutation: a minimum fragment of the whole chunk (the keyframe stamped 40 ms early does not close its
/// fragment, and the fragments come out at pictures that begin nothing: not six); a maximum of the default
/// four seconds (the chunk is cut in two).
#[test]
fn a_chunked_track_has_one_fragment_for_each_chunk_closed_at_its_keyframe() {
    let write = |limits: Limits| {
        let (mut w, mut file) = FragmentWriter::new(TrackConfig::Video(video_config()), 0, limits);
        let mut keyframes_at = Vec::new();
        for i in 0..450_u64 {
            let key = i % 75 == 0;
            // 66.667 ms a picture; a keyframe is stamped 40 ms before its time.
            let pts = i * 66_667 - if key && i > 0 { 40_000 } else { 0 };
            if key {
                keyframes_at.push(pts);
            }
            let au = if key { keyframe(i as u8, 200) } else { delta(i as u8, 60) };
            file.extend(w.push(pts, annexb_to_avcc(&au).bytes, key).unwrap());
        }
        file.extend(w.finish(450 * 66_667));
        (w.stats().fragments, scan(&mut Cursor::new(&file)).unwrap().fragments, keyframes_at.len())
    };
    let (made, scanned, keyframes) = write(Limits::chunked(5_000_000));
    assert_eq!((made, scanned, keyframes), (6, 6, 6), "a fragment for each chunk");
    let (made, _, _) = write(Limits::default());
    assert!(made >= 8, "the 4-second cap cuts a 5-second chunk in two: {made} fragments");
    // The numbers of the limits themselves: three quarters of a chunk to close at a keyframe, one and a
    // half to close at any picture.
    let l = Limits::chunked(5_000_000);
    assert_eq!((l.min_fragment_us, l.max_fragment_us), (3_750_000, 7_500_000));
    assert_eq!((l.max_fragment_bytes, l.max_fragment_samples), (Limits::default().max_fragment_bytes, Limits::default().max_fragment_samples));
}

/// Every fragment of a file: the `(duration, size, flags)` of each of its samples and its media.
fn read_fragments(file: &[u8]) -> Vec<(Vec<(u32, u32, u32)>, Vec<u8>)> {
    let boxes = top_boxes(file);
    let moofs: Vec<_> = boxes.iter().filter(|b| &b.0 == b"moof").collect();
    let mdats: Vec<_> = boxes.iter().filter(|b| &b.0 == b"mdat").collect();
    moofs
        .iter()
        .zip(&mdats)
        .map(|(moof, mdat)| {
            let trun = find(&file[moof.1 + 8..moof.1 + moof.2], &[b"traf", b"trun"]).unwrap();
            let n = be32(trun, 4).unwrap() as usize;
            let entries = (0..n)
                .map(|i| {
                    let at = 12 + i * 12;
                    (be32(trun, at).unwrap(), be32(trun, at + 4).unwrap(), be32(trun, at + 8).unwrap())
                })
                .collect();
            (entries, file[mdat.1 + 8..mdat.1 + mdat.2].to_vec())
        })
        .collect()
}

#[test]
fn every_sample_comes_back_out_of_its_fragment_with_its_size_flags_and_duration() {
    let (file, _) = video_file(150, 60, 33_333);
    let mut samples = Vec::new();
    for (entries, media) in read_fragments(&file) {
        let mut at = 0_usize;
        for (duration, size, flags) in entries {
            samples.push((duration, media[at..at + size as usize].to_vec(), flags));
            at += size as usize;
        }
        assert_eq!(at, media.len(), "the sizes add up to the media of the fragment");
    }
    assert_eq!(samples.len(), 150);
    for (i, (duration, bytes, flags)) in samples.iter().enumerate() {
        let key = i % 60 == 0;
        let au = if key { keyframe(i as u8, 300) } else { delta(i as u8, 100) };
        assert_eq!(bytes, &annexb_to_avcc(&au).bytes, "sample {i}'s bytes");
        assert_eq!(*flags, if key { 0x0200_0000 } else { 0x0101_0000 }, "sample {i}'s flags");
        let end = if i + 1 < 150 { us_to_ticks((i as u64 + 1) * 33_333, 90_000) } else { us_to_ticks(150 * 33_333, 90_000) };
        assert_eq!(u64::from(*duration), end - us_to_ticks(i as u64 * 33_333, 90_000), "sample {i}'s duration");
    }
}

#[test]
fn the_index_at_the_end_points_at_the_fragments() {
    let (file, _) = video_file(150, 60, 33_333);
    let boxes = top_boxes(&file);
    let mfra = boxes.last().unwrap();
    assert_eq!(&mfra.0, b"mfra");
    let body = &file[mfra.1 + 8..mfra.1 + mfra.2];
    let tfra = child(body, b"tfra").unwrap();
    assert_eq!(tfra[0], 1, "version 1: 64-bit times and offsets");
    let count = be32(tfra, 12).unwrap() as usize;
    assert_eq!(count, 3);
    let moofs: Vec<usize> = boxes.iter().filter(|b| &b.0 == b"moof").map(|b| b.1).collect();
    for i in 0..count {
        let at = 16 + i * 19;
        let offset = be64(tfra, at + 8).unwrap() as usize;
        assert_eq!(offset, moofs[i], "entry {i} points at a moof");
        assert_eq!(&file[offset + 4..offset + 8], b"moof");
    }
    // mfro's size is the size of the whole mfra box.
    let mfro = child(body, b"mfro").unwrap();
    assert_eq!(be32(mfro, 4), Some(mfra.2 as u32));
}

// ---- limits ----

#[test]
fn a_sample_with_nothing_in_it_or_too_much_is_refused() {
    let limits = Limits { max_sample_bytes: 100, ..Limits::default() };
    let (mut w, _) = FragmentWriter::new(TrackConfig::Video(video_config()), 0, limits);
    assert_eq!(w.push(0, Vec::new(), true), Err(MuxError::EmptySample));
    assert_eq!(
        w.push(0, vec![1; 101], true),
        Err(MuxError::SampleTooLarge { bytes: 101, limit: 100 })
    );
    assert!(w.push(0, vec![1; 100], true).is_ok());
    assert_eq!(w.stats().samples, 1, "the refused ones were not counted");
}

#[test]
fn nothing_can_be_added_to_a_finished_file() {
    let (mut w, _) = FragmentWriter::new(TrackConfig::Opus(OpusConfig::mono_48k()), 0, Limits::default());
    w.push_ticks(0, opus_packet(1), true).unwrap();
    assert!(!w.finish(20_000).is_empty());
    assert_eq!(w.push_ticks(960, opus_packet(2), true), Err(MuxError::Finished));
    assert!(w.finish(40_000).is_empty(), "finishing twice writes nothing more");
}

#[test]
fn the_fragment_sample_cap_closes_a_fragment() {
    let limits = Limits { max_fragment_samples: 10, min_fragment_us: u64::MAX, max_fragment_us: u64::MAX, ..Limits::default() };
    let (mut w, mut file) = FragmentWriter::new(TrackConfig::Opus(OpusConfig::mono_48k()), 0, limits);
    for i in 0..25 {
        file.extend(w.push_ticks(i * 960, opus_packet(i as u8), true).unwrap());
    }
    file.extend(w.finish(25 * 20_000));
    assert_eq!(w.stats().fragments, 3, "10 + 10 + 5");
}

// ---- scanning what a crash left ----

#[test]
fn a_file_cut_in_the_middle_of_a_fragment_is_whole_up_to_the_fragment_before() {
    let (file, _) = video_file(150, 60, 33_333);
    let boxes = top_boxes(&file);
    let moofs: Vec<&([u8; 4], usize, usize)> = boxes.iter().filter(|b| &b.0 == b"moof").collect();
    let mdats: Vec<&([u8; 4], usize, usize)> = boxes.iter().filter(|b| &b.0 == b"mdat").collect();
    assert_eq!(moofs.len(), 3);

    // Cut inside the third fragment's media.
    let cut = mdats[2].1 + mdats[2].2 / 2;
    let s = scan(&mut Cursor::new(&file[..cut])).unwrap();
    assert_eq!(s.fragments, 2);
    assert_eq!(s.samples, 120);
    assert!(s.truncated && !s.finished);
    assert_eq!(s.whole_end as usize, mdats[1].1 + mdats[1].2, "the end of the second fragment");
    assert!(s.duration_us().abs_diff(120 * 33_333) <= 12, "{}", s.duration_us());

    // Cutting the file there leaves one that scans clean.
    let clean = scan(&mut Cursor::new(&file[..s.whole_end as usize])).unwrap();
    assert!(!clean.truncated);
    assert_eq!((clean.fragments, clean.samples), (2, 120));
    assert!(!clean.finished, "no index: it was not finished, only cut");
}

#[test]
fn a_file_cut_in_the_moof_or_before_any_fragment_is_whole_up_to_the_head() {
    let (file, _) = video_file(150, 60, 33_333);
    let boxes = top_boxes(&file);
    let first_moof = boxes.iter().find(|b| &b.0 == b"moof").unwrap();
    for cut in [first_moof.1 + 3, first_moof.1 + 12, first_moof.1 + first_moof.2 - 1, first_moof.1 + first_moof.2 + 4] {
        let s = scan(&mut Cursor::new(&file[..cut])).unwrap();
        assert!(s.has_moov, "cut at {cut}");
        assert!(s.is_empty(), "cut at {cut}: no whole fragment");
        assert_eq!(s.whole_end as usize, first_moof.1, "cut at {cut}: whole up to the end of the head");
        assert!(s.truncated);
    }
}

#[test]
fn a_file_cut_inside_the_head_has_nothing() {
    let (file, _) = video_file(10, 60, 33_333);
    let boxes = top_boxes(&file);
    let moov = boxes.iter().find(|b| &b.0 == b"moov").unwrap();
    let s = scan(&mut Cursor::new(&file[..moov.1 + 40])).unwrap();
    assert!(!s.has_moov);
    assert_eq!(s.whole_end, 0);
    assert!(s.is_empty());
}

#[test]
fn a_cut_at_every_byte_never_panics_and_never_overstates() {
    let (file, _) = opus_file(60);
    let full = scan(&mut Cursor::new(&file)).unwrap();
    let mut last_samples = 0;
    for cut in (0..file.len()).step_by(7) {
        let s = scan(&mut Cursor::new(&file[..cut])).unwrap_or_default();
        assert!(s.whole_end as usize <= cut, "cut {cut}: whole_end {} is past the file", s.whole_end);
        assert!(s.samples <= full.samples && s.samples >= last_samples, "cut {cut}: samples only grow with the file");
        last_samples = s.samples;
        if s.whole_end > 0 {
            let clean = scan(&mut Cursor::new(&file[..s.whole_end as usize])).unwrap();
            assert!(!clean.truncated, "cut {cut}: the file up to whole_end is whole");
            assert_eq!(clean.samples, s.samples);
        }
    }
}

#[test]
fn a_file_that_is_no_mp4_is_refused() {
    assert_eq!(scan(&mut Cursor::new(b"this is not an mp4 file at all".to_vec())), Err(ScanError::NotMp4));
    let empty = scan(&mut Cursor::new(Vec::new())).unwrap();
    assert!(empty.is_empty() && !empty.has_moov);
}

#[test]
fn a_scan_reads_headers_not_media() {
    // A fragment whose media is a megabyte: a counting reader sees the scan read a few hundred
    // bytes, because it seeks over the mdat.
    struct Counting<R> {
        inner: R,
        read: usize,
    }
    impl<R: Read> Read for Counting<R> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = self.inner.read(buf)?;
            self.read += n;
            Ok(n)
        }
    }
    impl<R: Seek> Seek for Counting<R> {
        fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
            self.inner.seek(pos)
        }
    }
    let (mut w, mut file) = FragmentWriter::new(TrackConfig::Video(video_config()), 0, Limits::default());
    file.extend(w.push(0, annexb_to_avcc(&keyframe(0, 1_000_000)).bytes, true).unwrap());
    file.extend(w.finish(1_000_000));
    let mut reader = Counting { inner: Cursor::new(&file), read: 0 };
    let s = scan(&mut reader).unwrap();
    assert_eq!(s.samples, 1);
    assert!(reader.read < 4_096, "the scan read {} bytes of a file of {}", reader.read, file.len());
}

// ---- the rounding of time ----

#[test]
fn microseconds_and_ticks_convert_to_the_nearest_whole_tick() {
    assert_eq!(us_to_ticks(1_000_000, 90_000), 90_000);
    assert_eq!(us_to_ticks(33_333, 90_000), 3_000, "2999.97 rounds to 3000");
    assert_eq!(us_to_ticks(20_000, 48_000), 960);
    assert_eq!(ticks_to_us(960, 48_000), 20_000);
    assert_eq!(ticks_to_us(3_000, 90_000), 33_333);
    assert_eq!(us_to_ticks(u64::MAX, u32::MAX), u64::MAX, "no overflow: it saturates");
}
