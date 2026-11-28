use super::*;

#[test]
fn the_codecs_google_ships_are_software_and_the_vendors_are_hardware() {
    for software in [
        "c2.android.avc.decoder",
        "c2.android.avc.encoder",
        "OMX.google.h264.decoder",
        "OMX.google.h264.encoder",
        "c2.google.something",
        "OMX.SEC.avc.sw.dec",
        "c2.qti.avc.swdec",
        "ffmpeg.h264",
        "OMX.rk.video_decoder.avc.sw",
    ] {
        assert!(is_software_codec(software), "{software}");
    }
    for hardware in [
        "c2.qti.avc.decoder",
        "c2.exynos.h264.encoder",
        "OMX.qcom.video.decoder.avc",
        "OMX.MTK.VIDEO.DECODER.AVC",
        "c2.mtk.avc.decoder",
        "OMX.Exynos.AVC.Decoder",
        "arc.h264.decoder",
    ] {
        assert!(!is_software_codec(hardware), "{hardware}");
    }
}

#[test]
fn an_engine_names_its_codec_and_where_it_runs() {
    assert_eq!(
        engine_label(Some("c2.android.avc.encoder")),
        "MediaCodec (c2.android.avc.encoder, software)"
    );
    assert_eq!(
        engine_label(Some("c2.qti.avc.decoder")),
        "MediaCodec (c2.qti.avc.decoder, hardware)"
    );
    assert!(engine_label(None).contains("Android 9"));
    assert_eq!(is_hardware(Some("c2.qti.avc.decoder")), Some(true));
    assert_eq!(is_hardware(Some("c2.android.avc.decoder")), Some(false));
    assert_eq!(is_hardware(None), None, "a name the platform could not give says nothing");
}

#[test]
fn the_api_level_is_read_from_the_property_text() {
    assert_eq!(parse_api_level("34"), Some(34));
    assert_eq!(parse_api_level(" 24\n"), Some(24));
    assert_eq!(parse_api_level(""), None);
    assert_eq!(parse_api_level("0"), None);
    assert_eq!(parse_api_level("abc"), None);
    assert_eq!(parse_api_level("99999"), None);
}

#[test]
fn a_status_says_in_words_what_the_ndk_meant() {
    assert_eq!(describe_status(0), "ok");
    assert!(describe_status(-10002).contains("unsupported"));
    assert!(describe_status(-10005).contains("wrong state"));
    assert!(describe_status(1101).contains("reclaimed"));
    assert!(describe_status(1100).contains("another app"));
    assert!(describe_status(7).contains("does not know"));
    let error = MediaCodecError::Status {
        call: "AMediaCodec_configure",
        code: -10002,
    };
    assert_eq!(
        error.to_string(),
        "AMediaCodec_configure answered -10002 (unsupported)"
    );
    assert!(MediaCodecError::NoCodec { encoder: true }.to_string().contains("encoder"));
    assert!(MediaCodecError::NoCodec { encoder: false }.to_string().contains("decoder"));
    assert!(MediaCodecError::Timeout("an input buffer").to_string().contains("input buffer"));
}

#[test]
fn only_a_status_or_a_missing_codec_means_the_codec_has_to_be_made_again() {
    assert!(MediaCodecError::Status { call: "x", code: 1101 }.codec_is_lost());
    assert!(MediaCodecError::NoCodec { encoder: false }.codec_is_lost());
    assert!(!MediaCodecError::Unsupported(String::from("odd size")).codec_is_lost());
    assert!(!MediaCodecError::Timeout("an output").codec_is_lost());
    assert!(!MediaCodecError::Library(String::from("no lib")).codec_is_lost());
}

#[test]
fn an_encoder_is_configured_within_the_limits_and_refused_outside_the_sizes() {
    let config = EncoderConfig::realtime(640, 480, 800).expect("a normal size");
    assert_eq!(
        (config.width, config.height, config.bitrate_kbps, config.fps),
        (640, 480, 800, DEFAULT_FPS)
    );
    assert_eq!(config.bitrate_bps(), 800_000);
    assert_eq!(config.i_frame_interval_s, DEFAULT_I_FRAME_INTERVAL_S);
    // The bitrate is moved into the limits, never refused.
    assert_eq!(EncoderConfig::realtime(640, 480, 1).expect("low").bitrate_kbps, MIN_KBPS);
    assert_eq!(EncoderConfig::realtime(640, 480, 0).expect("zero").bitrate_kbps, MIN_KBPS);
    assert_eq!(
        EncoderConfig::realtime(640, 480, u32::MAX).expect("high").bitrate_kbps,
        MAX_KBPS
    );
    assert_eq!(clamp_bitrate(5000), (5000, false));
    assert_eq!(clamp_bitrate(63), (MIN_KBPS, true));
    assert_eq!(clamp_bitrate(50_001), (MAX_KBPS, true));
    // A size no encoder takes is a typed error with the numbers in it.
    for (w, h) in [(641, 480), (640, 481), (8, 480), (640, 8), (4098, 480), (640, 4098), (0, 0)] {
        let error = EncoderConfig::realtime(w, h, 800).expect_err("an unsupported size");
        assert!(error.to_string().contains(&format!("{w}x{h}")), "{error}");
        assert!(!error.codec_is_lost());
    }
    assert!(EncoderConfig::realtime(MIN_SIDE, MIN_SIDE, 800).is_ok());
    assert!(EncoderConfig::realtime(MAX_SIDE, MAX_SIDE, 800).is_ok());
}

#[test]
fn the_bitrate_in_bits_does_not_overflow() {
    let config = EncoderConfig {
        width: 640,
        height: 480,
        bitrate_kbps: u32::MAX,
        fps: 30,
        i_frame_interval_s: 1,
    };
    assert_eq!(config.bitrate_bps(), i32::MAX);
}

#[test]
fn a_codec_that_keeps_failing_is_given_up_on_after_a_few_tries() {
    for consecutive in 1..=MAX_CONSECUTIVE_FAILURES {
        assert_eq!(recovery_after(consecutive), Recovery::Remake, "failure {consecutive}");
    }
    assert_eq!(recovery_after(MAX_CONSECUTIVE_FAILURES + 1), Recovery::GiveUp);
    assert_eq!(recovery_after(u32::MAX), Recovery::GiveUp);
}

#[test]
fn a_line_is_said_once_and_only_so_many_different_ones() {
    let mut said = Said::default();
    assert!(said.is_new("a"));
    assert!(!said.is_new("a"));
    assert!(said.is_new("b"));
    for n in 0..100 {
        said.is_new(&format!("line {n}"));
    }
    assert!(!said.is_new("one more nobody hears"));
    assert!(!said.is_new("a"));
}

#[test]
fn the_waits_are_bounded_and_ordered() {
    // One wait for an input is short against the stall limit, so a stall is many tries.
    assert!(INPUT_WAIT_US > 0 && INPUT_WAIT_US as u64 / 1000 * 5 < INPUT_STALL_MS);
    // A drain's idle limit is under its total, and both are under the codec thread's flush
    // timeout (5 s), so a flush never runs into it for one drain.
    assert!(DRAIN_IDLE_MS < DRAIN_TOTAL_MS && DRAIN_TOTAL_MS < 5_000);
    assert!(MAX_DRAIN_STEPS >= 16);
    // The encoder formats start with NV12.
    assert_eq!(ENCODER_INPUT_FORMATS[0], 21);
}

#[test]
fn a_stream_size_outside_what_a_decoder_is_configured_for_is_refused_with_the_numbers() {
    assert!(check_decode_size(320, 240).is_ok());
    assert!(check_decode_size(MIN_SIDE, MIN_SIDE).is_ok());
    assert!(check_decode_size(MAX_DECODE_SIDE, MAX_DECODE_SIDE).is_ok());
    // Odd sides are legal in a stream (cropping): only the range is checked.
    assert!(check_decode_size(321, 241).is_ok());
    for (w, h) in [(0, 0), (8, 240), (320, 8), (8193, 240), (320, 8193)] {
        let error = check_decode_size(w, h).expect_err("outside the range");
        assert!(error.to_string().contains(&format!("{w}x{h}")), "{error}");
        assert!(!error.codec_is_lost());
    }
}

#[test]
fn a_decoder_asks_for_nv12_then_i420_then_whatever_the_codec_has() {
    assert_eq!(DECODER_OUTPUT_HINTS, [Some(21), Some(19), None]);
}
