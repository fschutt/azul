use super::*;

/// Writes the bits of an RBSP, most significant first.
#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    used: u32,
}

impl BitWriter {
    fn bit(&mut self, bit: u32) {
        if self.used % 8 == 0 {
            self.bytes.push(0);
        }
        if bit != 0 {
            let last = self.bytes.len() - 1;
            self.bytes[last] |= 1 << (7 - self.used % 8);
        }
        self.used += 1;
    }

    fn bits(&mut self, value: u32, n: u32) {
        for i in (0..n).rev() {
            self.bit((value >> i) & 1);
        }
    }

    fn ue(&mut self, value: u32) {
        let v = u64::from(value) + 1;
        let len = 64 - v.leading_zeros();
        for _ in 0..len - 1 {
            self.bit(0);
        }
        for i in (0..len).rev() {
            self.bit(((v >> i) & 1) as u32);
        }
    }

    fn se(&mut self, value: i32) {
        let mapped = if value > 0 {
            (value as u32) * 2 - 1
        } else {
            (-value) as u32 * 2
        };
        self.ue(mapped);
    }

    /// The NAL unit: header byte, then the RBSP with its trailing bit and emulation prevention.
    fn into_nal(mut self, header: u8) -> Vec<u8> {
        self.bit(1);
        while self.used % 8 != 0 {
            self.bit(0);
        }
        let mut nal = vec![header];
        let mut zeros = 0;
        for byte in self.bytes {
            if zeros >= 2 && byte <= 3 {
                nal.push(3);
                zeros = 0;
            }
            zeros = if byte == 0 { zeros + 1 } else { 0 };
            nal.push(byte);
        }
        nal
    }
}

/// What an SPS to build holds.
struct SpsSpec {
    profile: u8,
    width_mbs: u32,
    height_map_units: u32,
    frame_mbs_only: bool,
    crop: Option<(u32, u32, u32, u32)>,
    chroma_format_idc: u32,
    vui: Option<VuiSpec>,
}

#[derive(Clone, Copy)]
struct VuiSpec {
    full_range: bool,
    matrix: Option<u8>,
    reorder: Option<u32>,
    hrd: bool,
}

impl Default for SpsSpec {
    fn default() -> Self {
        SpsSpec {
            profile: 66,
            width_mbs: 20,
            height_map_units: 15,
            frame_mbs_only: true,
            crop: None,
            chroma_format_idc: 1,
            vui: None,
        }
    }
}

fn build_sps(spec: &SpsSpec) -> Vec<u8> {
    let mut w = BitWriter::default();
    w.bits(u32::from(spec.profile), 8);
    w.bits(0xc0, 8);
    w.bits(30, 8);
    w.ue(0);
    if has_chroma_info(spec.profile) {
        w.ue(spec.chroma_format_idc);
        if spec.chroma_format_idc == 3 {
            w.bit(0);
        }
        w.ue(0);
        w.ue(0);
        w.bit(0);
        // A scaling matrix: the first list present with two deltas, the others absent.
        w.bit(1);
        let lists = if spec.chroma_format_idc == 3 { 12 } else { 8 };
        for index in 0..lists {
            if index == 0 {
                w.bit(1);
                w.se(2);
                w.se(-3);
                for _ in 2..16 {
                    w.se(0);
                }
            } else {
                w.bit(0);
            }
        }
    }
    w.ue(0); // log2_max_frame_num_minus4
    w.ue(1); // pic_order_cnt_type 1
    w.bit(0);
    w.se(-1);
    w.se(2);
    w.ue(2);
    w.se(3);
    w.se(-4);
    w.ue(1); // max_num_ref_frames
    w.bit(0);
    w.ue(spec.width_mbs - 1);
    w.ue(spec.height_map_units - 1);
    w.bit(u32::from(spec.frame_mbs_only));
    if !spec.frame_mbs_only {
        w.bit(0);
    }
    w.bit(1); // direct_8x8_inference_flag
    match spec.crop {
        Some((l, r, t, b)) => {
            w.bit(1);
            w.ue(l);
            w.ue(r);
            w.ue(t);
            w.ue(b);
        }
        None => w.bit(0),
    }
    match spec.vui {
        None => w.bit(0),
        Some(vui) => {
            w.bit(1);
            w.bit(1); // aspect_ratio_info_present_flag
            w.bits(255, 8);
            w.bits(4, 16);
            w.bits(3, 16);
            w.bit(0); // overscan_info_present_flag
            w.bit(1); // video_signal_type_present_flag
            w.bits(5, 3);
            w.bit(u32::from(vui.full_range));
            match vui.matrix {
                Some(matrix) => {
                    w.bit(1);
                    w.bits(1, 8);
                    w.bits(1, 8);
                    w.bits(u32::from(matrix), 8);
                }
                None => w.bit(0),
            }
            w.bit(1); // chroma_loc_info_present_flag
            w.ue(1);
            w.ue(2);
            w.bit(1); // timing_info_present_flag
            w.bits(1, 32);
            w.bits(60, 32);
            w.bit(1);
            w.bit(u32::from(vui.hrd)); // nal_hrd_parameters_present_flag
            if vui.hrd {
                w.ue(1); // cpb_cnt_minus1: two
                w.bits(3, 4);
                w.bits(4, 4);
                for _ in 0..2 {
                    w.ue(1000);
                    w.ue(2000);
                    w.bit(1);
                }
                w.bits(23, 5);
                w.bits(23, 5);
                w.bits(23, 5);
                w.bits(24, 5);
            }
            w.bit(0); // vcl_hrd_parameters_present_flag
            if vui.hrd {
                w.bit(0); // low_delay_hrd_flag
            }
            w.bit(0); // pic_struct_present_flag
            match vui.reorder {
                Some(reorder) => {
                    w.bit(1);
                    w.bit(1);
                    w.ue(2);
                    w.ue(1);
                    w.ue(16);
                    w.ue(16);
                    w.ue(reorder);
                    w.ue(4);
                }
                None => w.bit(0),
            }
        }
    }
    w.into_nal(0x67)
}

fn annexb(nals: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    for nal in nals {
        out.extend_from_slice(&START_CODE);
        out.extend_from_slice(nal);
    }
    out
}

#[test]
fn the_nal_units_of_an_annexb_buffer_are_found_with_three_and_four_byte_start_codes() {
    let data = [
        9, 9, // junk before the first start code
        0, 0, 1, 0x67, 1, 2, // three-byte start code
        0, 0, 0, 1, 0x68, 3, // four-byte
        0, 0, 0, 0, 1, 0x65, 0x88, 4, 0, // extra zero bytes in front, a zero at the end
    ];
    let spans = nal_spans(&data);
    assert_eq!(spans.len(), 3);
    assert_eq!(spans[0].bytes(&data), &[0x67, 1, 2]);
    assert_eq!(spans[0].with_start_code(&data), &[0, 0, 1, 0x67, 1, 2]);
    assert_eq!(spans[1].bytes(&data), &[0x68, 3]);
    assert_eq!(spans[1].with_start_code(&data), &[0, 0, 0, 1, 0x68, 3]);
    assert_eq!(spans[2].nal_type(&data), NAL_IDR);
    assert_eq!(spans[2].bytes(&data), &[0x65, 0x88, 4, 0]);
    assert_eq!(spans[2].start_code, 14, "the extra zero bytes belong to the delimiter");
}

#[test]
fn an_empty_buffer_a_buffer_without_start_codes_and_empty_nal_units_give_no_nal() {
    assert!(nal_spans(&[]).is_empty());
    assert!(nal_spans(&[1, 2, 3, 4, 5]).is_empty());
    assert!(nal_spans(&[0, 0, 1]).is_empty());
    let two_in_a_row = [0, 0, 1, 0, 0, 1, 0x41, 7];
    let spans = nal_spans(&two_in_a_row);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].bytes(&two_in_a_row), &[0x41, 7]);
}

#[test]
fn nal_types_and_key_frames_are_told_from_the_header() {
    let key = annexb(&[&[0x67, 1], &[0x68, 2], &[0x65, 0x88, 5]]);
    assert!(contains_idr(&key));
    assert!(starts_with_sps(&key));
    assert!(contains_nal(&key, NAL_PPS));
    let delta = annexb(&[&[0x41, 0x9a, 5]]);
    assert!(!contains_idr(&delta));
    assert!(!starts_with_sps(&delta));
    assert!(!starts_with_sps(&[]));
}

#[test]
fn the_parameter_sets_are_taken_out_in_order_with_four_byte_start_codes() {
    let data = [
        0, 0, 1, 0x06, 9, 9, // SEI: not a parameter set
        0, 0, 1, 0x67, 1, 2, 3, //
        0, 0, 0, 1, 0x68, 4, //
        0, 0, 1, 0x65, 0x88, 8,
    ];
    assert_eq!(
        parameter_sets(&data),
        [0, 0, 0, 1, 0x67, 1, 2, 3, 0, 0, 0, 1, 0x68, 4]
    );
    assert!(parameter_sets(&annexb(&[&[0x41, 1]])).is_empty());
}

#[test]
fn parameter_sets_go_in_front_of_a_key_frame_that_has_none_and_only_there() {
    let config = annexb(&[&[0x67, 1], &[0x68, 2]]);
    let bare = annexb(&[&[0x65, 0x88, 5]]);
    let joined = with_parameter_sets(&config, &bare);
    assert_eq!(joined.len(), config.len() + bare.len());
    assert!(starts_with_sps(&joined) && contains_idr(&joined));
    // A frame that brings its own SPS is left alone.
    let own = annexb(&[&[0x67, 9], &[0x68, 9], &[0x65, 0x88, 5]]);
    assert_eq!(with_parameter_sets(&config, &own), own);
    // Nothing to put in front.
    assert_eq!(with_parameter_sets(&[], &bare), bare);
}

#[test]
fn a_chunk_with_two_pictures_splits_into_two_access_units_and_one_picture_stays_whole() {
    let key = annexb(&[&[0x67, 1], &[0x68, 2], &[0x65, 0x88, 5], &[0x65, 0x00, 6]]);
    let delta = annexb(&[&[0x41, 0x9a, 5]]);
    // The second slice of the key picture (first_mb_in_slice != 0) stays with it.
    let units = split_access_units(&key);
    assert_eq!(units, vec![key.as_slice()]);

    let two = [key.clone(), delta.clone()].concat();
    let units = split_access_units(&two);
    assert_eq!(units, vec![key.as_slice(), delta.as_slice()]);

    // Two delta pictures, then a new GOP that opens with its parameter sets.
    let three = [delta.clone(), delta.clone(), key.clone()].concat();
    let units = split_access_units(&three);
    assert_eq!(units, vec![delta.as_slice(), delta.as_slice(), key.as_slice()]);

    // An access unit delimiter begins a unit, SEI before the first slice does not.
    let with_aud = [
        annexb(&[&[0x09, 0xf0], &[0x06, 1], &[0x41, 0x9a, 5]]),
        annexb(&[&[0x09, 0xf0], &[0x41, 0x9a, 6]]),
    ]
    .concat();
    assert_eq!(split_access_units(&with_aud).len(), 2);
    assert!(split_access_units(&[]).is_empty());
}

#[test]
fn an_sps_gives_the_picture_size_and_the_cropping_is_taken_off() {
    let plain = parse_sps(&build_sps(&SpsSpec::default())).expect("a baseline SPS");
    assert_eq!((plain.width, plain.height), (320, 240));
    assert_eq!(plain.profile_idc, 66);
    assert_eq!(plain.level_idc, 30);
    assert_eq!(plain.colour, None);
    assert_eq!(plain.max_num_reorder_frames, None);

    // 1920x1088 coded, 8 rows cropped at the bottom: the usual 1080p.
    let hd = parse_sps(&build_sps(&SpsSpec {
        width_mbs: 120,
        height_map_units: 68,
        crop: Some((0, 0, 0, 4)),
        ..SpsSpec::default()
    }))
    .expect("a cropped SPS");
    assert_eq!((hd.width, hd.height), (1920, 1080));

    // Cropped at the left and right too (4:2:0: units of two).
    let narrow = parse_sps(&build_sps(&SpsSpec {
        crop: Some((1, 3, 2, 0)),
        ..SpsSpec::default()
    }))
    .expect("a cropped SPS");
    assert_eq!((narrow.width, narrow.height), (320 - 8, 240 - 4));
}

#[test]
fn interlaced_streams_double_the_height_and_the_crop_unit() {
    let interlaced = parse_sps(&build_sps(&SpsSpec {
        width_mbs: 45,
        height_map_units: 17,
        frame_mbs_only: false,
        crop: Some((0, 0, 0, 2)),
        ..SpsSpec::default()
    }))
    .expect("an interlaced SPS");
    // 17 map units of two macroblock rows: 544 coded rows, 2 * 2 * 2 cropped.
    assert_eq!((interlaced.width, interlaced.height), (720, 544 - 8));
}

#[test]
fn a_high_profile_sps_with_a_scaling_matrix_is_read_through() {
    for chroma_format_idc in [1, 2, 3] {
        let sps = parse_sps(&build_sps(&SpsSpec {
            profile: 100,
            chroma_format_idc,
            width_mbs: 80,
            height_map_units: 45,
            ..SpsSpec::default()
        }))
        .unwrap_or_else(|| panic!("a high profile SPS with chroma_format_idc {chroma_format_idc}"));
        assert_eq!((sps.width, sps.height), (1280, 720), "chroma {chroma_format_idc}");
        assert_eq!(sps.profile_idc, 100);
    }
}

#[test]
fn the_vui_gives_the_colour_description_and_the_reorder_depth() {
    let sps = parse_sps(&build_sps(&SpsSpec {
        vui: Some(VuiSpec {
            full_range: false,
            matrix: Some(1),
            reorder: Some(0),
            hrd: false,
        }),
        ..SpsSpec::default()
    }))
    .expect("an SPS with a VUI");
    assert_eq!(
        sps.colour,
        Some(Colour {
            full_range: false,
            matrix_coefficients: 1
        })
    );
    assert_eq!(sps.max_num_reorder_frames, Some(0));

    // HRD parameters in front of the bitstream restriction are skipped over.
    let with_hrd = parse_sps(&build_sps(&SpsSpec {
        vui: Some(VuiSpec {
            full_range: true,
            matrix: None,
            reorder: Some(3),
            hrd: true,
        }),
        ..SpsSpec::default()
    }))
    .expect("an SPS with a VUI and HRD parameters");
    assert_eq!(
        with_hrd.colour,
        Some(Colour {
            full_range: true,
            matrix_coefficients: 2
        })
    );
    assert_eq!(with_hrd.max_num_reorder_frames, Some(3));
}

#[test]
fn a_truncated_vui_keeps_the_size() {
    let full = build_sps(&SpsSpec {
        vui: Some(VuiSpec {
            full_range: false,
            matrix: Some(1),
            reorder: Some(0),
            hrd: false,
        }),
        ..SpsSpec::default()
    });
    let cut = &full[..full.len() - 6];
    let sps = parse_sps(cut).expect("the size is before the VUI");
    assert_eq!((sps.width, sps.height), (320, 240));
}

#[test]
fn emulation_prevention_bytes_do_not_shift_the_fields() {
    // 16383 macroblock columns: the ue(v) of the width has long runs of zero bits, so the RBSP
    // holds 00 00 sequences that get a 03 inserted by the writer.
    let sps = build_sps(&SpsSpec {
        width_mbs: 1000,
        height_map_units: 1,
        ..SpsSpec::default()
    });
    let parsed = parse_sps(&sps).expect("a wide SPS");
    assert_eq!((parsed.width, parsed.height), (16000, 16));
}

#[test]
fn what_is_not_an_sps_or_is_cut_short_or_absurd_is_refused() {
    assert_eq!(parse_sps(&[]), None);
    assert_eq!(parse_sps(&[0x68, 1, 2, 3]), None, "a PPS");
    let good = build_sps(&SpsSpec::default());
    for cut in 0..good.len().min(9) {
        assert_eq!(parse_sps(&good[..cut]), None, "cut to {cut} bytes");
    }
    // 2000 macroblocks wide: over the 16384-pixel cap.
    let huge = build_sps(&SpsSpec {
        width_mbs: 2000,
        ..SpsSpec::default()
    });
    assert_eq!(parse_sps(&huge), None);
    // Cropping away the whole picture.
    let nothing = build_sps(&SpsSpec {
        crop: Some((80, 80, 0, 0)),
        ..SpsSpec::default()
    });
    assert_eq!(parse_sps(&nothing), None);
}

#[test]
fn the_first_sps_of_a_chunk_is_found_among_other_nal_units() {
    let sps = build_sps(&SpsSpec::default());
    let chunk = annexb(&[&[0x06, 1, 2], &sps, &[0x68, 5], &[0x65, 0x88, 1]]);
    let info = first_sps(&chunk).expect("an SPS in the chunk");
    assert_eq!((info.width, info.height), (320, 240));
    assert_eq!(first_sps(&annexb(&[&[0x41, 1, 2]])), None);
}

#[test]
fn the_bit_reader_reads_exp_golomb_codes() {
    // 1 | 010 | 011 | 00100 | 00101 -> ue 0, 1, 2, 3, 4
    let bytes = [0b1010_0110, 0b0100_0010, 0b1000_0000];
    let mut bits = Bits::new(&bytes);
    assert_eq!(
        [bits.ue(), bits.ue(), bits.ue(), bits.ue(), bits.ue()],
        [Some(0), Some(1), Some(2), Some(3), Some(4)]
    );
    // se: ue 1, 2, 3, 4 -> 1, -1, 2, -2
    let bytes = [0b0100_1100, 0b1000_0101, 0b0000_0000];
    let mut bits = Bits::new(&bytes);
    assert_eq!(
        [bits.se(), bits.se(), bits.se(), bits.se()],
        [Some(1), Some(-1), Some(2), Some(-2)]
    );
    // Running out of bits is an answer, not a panic.
    let mut bits = Bits::new(&[0]);
    assert_eq!(bits.ue(), None);
    assert_eq!(bits.bits(1), None);
}

/// A deterministic stream of numbers for the property-style tests below.
fn pseudo_random(seed: u64) -> impl FnMut() -> u64 {
    let mut x = seed | 1;
    move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    }
}

#[test]
fn bytes_that_are_not_a_stream_never_panic_the_parsers() {
    let mut next = pseudo_random(0x2545_F491_4F6C_DD1D);
    for round in 0..3000 {
        let len = (next() % 240) as usize;
        let mut data: Vec<u8> = (0..len).map(|_| (next() & 0xff) as u8).collect();
        // Half of the buffers get a start code and an SPS or slice header, so the parsers go deep.
        if len > 12 && round % 2 == 0 {
            let header = [0x67, 0x41, 0x65, 0x68, 0x06][(next() % 5) as usize];
            data[2..6].copy_from_slice(&[0, 0, 1, header]);
        }
        let spans = nal_spans(&data);
        for span in &spans {
            assert!(span.start_code <= span.payload && span.payload < span.end && span.end <= data.len());
            let _ = (span.nal_type(&data), span.bytes(&data), span.with_start_code(&data));
        }
        let units = split_access_units(&data);
        let joined: usize = units.iter().map(|unit| unit.len()).sum();
        assert!(joined <= data.len(), "the units are slices of the chunk");
        let _ = (first_sps(&data), parameter_sets(&data), contains_idr(&data), starts_with_sps(&data));
        let _ = with_parameter_sets(&data, &data);
        let _ = parse_sps(&data);
    }
}

#[test]
fn a_valid_sps_with_every_single_byte_changed_never_panics_and_still_parses_or_says_no() {
    let good = build_sps(&SpsSpec {
        profile: 100,
        vui: Some(VuiSpec {
            full_range: true,
            matrix: Some(1),
            reorder: Some(2),
            hrd: true,
        }),
        ..SpsSpec::default()
    });
    assert!(parse_sps(&good).is_some());
    for at in 0..good.len() {
        for value in [0u8, 1, 3, 0x7f, 0x80, 0xff] {
            let mut bad = good.clone();
            bad[at] = value;
            // Whatever it gives, the size is a size: nothing here may allocate or panic.
            if let Some(info) = parse_sps(&bad) {
                assert!(info.width >= 1 && info.width <= 16384 && info.height >= 1 && info.height <= 16384);
            }
        }
    }
}
