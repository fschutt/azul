use super::*;

fn good() -> Measured {
    Measured {
        decoder: String::from("MediaCodec (c2.android.avc.decoder, software)"),
        encoder: String::from("encodes with MediaCodec (c2.android.avc.encoder, software)"),
        hardware: false,
        frames_in: FRAMES,
        frames_out: FRAMES,
        wrong_size: 0,
        packets: FRAMES,
        bytes: 90_000,
        key_packets: 2,
        key_packets_without_sets: 0,
        first_packet_is_key_with_sets: true,
        forced_key_is_key_with_sets: true,
        psnr_y_mean: 41.2,
        psnr_y_min: 36.5,
        psnr_rgb_mean: 33.0,
        psnr_rgb_min: 30.1,
        psnr_y_each: vec![41.0; FRAMES as usize],
        packet_sizes: vec![3000; FRAMES as usize],
        centre_rgb: Some(([230, 220, 40], [230, 221, 41])),
        encode_first_ms: 31.0,
        encode_ms_per_frame: 7.5,
        decode_first_ms: 22.0,
        decode_ms_per_frame: 4.0,
    }
}

#[test]
fn the_clip_is_the_same_every_time_and_the_box_moves() {
    let a = box_frame_rgba(0);
    assert_eq!(a.len(), (WIDTH * HEIGHT * 4) as usize);
    assert_eq!(a, box_frame_rgba(0), "deterministic");
    assert_ne!(a, box_frame_rgba(1), "the box moved");
    // The box is yellow inside, dark at its border; the background is a gradient with full alpha.
    let at = |x: u32, y: u32| {
        let i = ((y * WIDTH + x) * 4) as usize;
        [a[i], a[i + 1], a[i + 2], a[i + 3]]
    };
    assert_eq!(at(24, 120), [230, 220, 40, 255]);
    assert_eq!(at(0, 96), [20, 20, 20, 255]);
    assert_eq!(at(WIDTH - 1, 0)[3], 255);
    assert_ne!(at(200, 10), at(10, 200));
}

#[test]
fn the_box_stays_inside_the_picture_over_the_whole_clip() {
    for index in 0..FRAMES {
        assert_eq!(box_frame_rgba(index).len(), (WIDTH * HEIGHT * 4) as usize, "frame {index}");
    }
}

#[test]
fn luma_is_the_bt601_video_range_of_each_pixel() {
    assert_eq!(luma(&[255, 255, 255, 255, 0, 0, 0, 255]), vec![235, 16]);
    assert_eq!(luma(&[255, 0, 0, 255]), vec![82]);
    assert_eq!(luma(&[0, 0, 255, 255]), vec![41]);
    assert!(luma(&[]).is_empty());
}

#[test]
fn psnr_of_equal_runs_is_the_cap_and_of_close_ones_is_high() {
    let a = vec![100u8; 1000];
    assert_eq!(psnr(&a, &a), 99.0);
    let mut b = a.clone();
    for x in b.iter_mut() {
        *x += 1;
    }
    // Every sample off by one: 10 log10(255^2) = 48.13 dB.
    assert!((psnr(&a, &b) - 48.13).abs() < 0.01, "{}", psnr(&a, &b));
    // Every sample off by 25: 20.2 dB.
    let c: Vec<u8> = a.iter().map(|x| x + 25).collect();
    assert!((psnr(&a, &c) - 20.17).abs() < 0.05, "{}", psnr(&a, &c));
    // Unequal or empty runs compare as nothing at all.
    assert_eq!(psnr(&a, &a[..10]), 0.0);
    assert_eq!(psnr(&[], &[]), 0.0);
}

#[test]
fn the_rgb_psnr_ignores_the_alpha_byte() {
    let a = [10u8, 20, 30, 255, 40, 50, 60, 255];
    let b = [10u8, 20, 30, 0, 40, 50, 60, 7];
    assert_eq!(psnr_rgb(&a, &b), 99.0);
    let c = [11u8, 20, 30, 255, 40, 50, 60, 255];
    assert!(psnr_rgb(&a, &c) < 99.0);
    assert_eq!(psnr_rgb(&a, &b[..4]), 0.0);
}

#[test]
fn the_nal_units_of_a_packet_are_read_from_its_start_codes() {
    let key = [0, 0, 0, 1, 0x67, 1, 2, 0, 0, 0, 1, 0x68, 3, 0, 0, 1, 0x65, 0x88, 9];
    assert_eq!(nal_types(&key), vec![7, 8, 5]);
    assert!(is_key(&key));
    assert!(begins_with_parameter_sets(&key));
    let delta = [0, 0, 0, 1, 0x41, 0x9a, 1, 2, 3];
    assert_eq!(nal_types(&delta), vec![1]);
    assert!(!is_key(&delta));
    assert!(!begins_with_parameter_sets(&delta));
    // A key frame with no parameter sets in front, or with only the SPS.
    let bare = [0, 0, 0, 1, 0x65, 0x88, 9];
    assert!(is_key(&bare) && !begins_with_parameter_sets(&bare));
    let only_sps = [0, 0, 0, 1, 0x67, 1, 0, 0, 0, 1, 0x65, 0x88];
    assert!(is_key(&only_sps) && !begins_with_parameter_sets(&only_sps));
    assert!(nal_types(&[]).is_empty());
    assert!(!is_key(&[1, 2, 3]));
}

#[test]
fn a_round_trip_that_gave_back_everything_with_a_good_picture_passes_with_its_numbers() {
    let numbers = judge(&good()).expect("a pass");
    for part in [
        "320x240",
        "30/30 frames",
        "psnr_y mean 41.2 min 36.5 dB",
        "psnr_rgb mean 33.0",
        "encoder hardware=false",
        "c2.android.avc.decoder",
        "c2.android.avc.encoder",
        "ms/frame",
    ] {
        assert!(numbers.contains(part), "{part} missing in {numbers}");
    }
}

#[test]
fn each_way_a_round_trip_can_go_wrong_is_a_failure_that_says_which() {
    let case = |change: fn(&mut Measured), expect: &str| {
        let mut m = good();
        change(&mut m);
        let why = judge(&m).expect_err("a failure");
        assert!(why.contains(expect), "{expect} not in {why}");
        assert!(why.contains("psnr_y"), "the numbers come with the failure: {why}");
    };
    case(|m| m.frames_out = FRAMES - 3, "27 of 30 frames");
    case(|m| m.wrong_size = 2, "2 decoded frames are not 320x240");
    case(|m| m.first_packet_is_key_with_sets = false, "first packet is not a key frame");
    case(|m| m.forced_key_is_key_with_sets = false, "frame 15 was asked for as a key frame");
    case(|m| m.key_packets_without_sets = 1, "1 key frames have no SPS and PPS");
    case(|m| m.psnr_y_mean = 30.0, "luma is too far");
    case(|m| m.psnr_y_min = 12.0, "luma is too far");
    case(|m| m.psnr_rgb_mean = 15.0, "the luma is right and the colour is not");
}

#[test]
fn the_thresholds_are_as_the_module_documents_them() {
    let mut m = good();
    m.psnr_y_mean = MIN_PSNR_Y_MEAN;
    m.psnr_y_min = MIN_PSNR_Y_EACH;
    m.psnr_rgb_mean = MIN_PSNR_RGB_MEAN;
    assert!(judge(&m).is_ok(), "exactly at the limits passes");
    m.psnr_rgb_mean -= 0.1;
    assert!(judge(&m).is_err());
}

#[test]
fn a_failure_shows_the_figures_of_every_frame_and_the_colour_of_the_box() {
    let mut m = good();
    m.frames_out = 29;
    m.psnr_y_each = vec![33.0, 34.0, 35.0];
    m.packet_sizes = vec![111, 222];
    m.centre_rgb = Some(([40, 40, 230], [230, 220, 40]));
    let why = judge(&m).expect_err("a failure");
    assert!(why.contains("psnr_y per frame: 33 34 35"), "{why}");
    assert!(why.contains("packet bytes: 111 222"), "{why}");
    assert!(why.contains("box centre decoded [40, 40, 230] encoded [230, 220, 40]"), "{why}");
    m.centre_rgb = None;
    assert!(judge(&m).expect_err("a failure").contains("no decoded frame to compare"));
}
