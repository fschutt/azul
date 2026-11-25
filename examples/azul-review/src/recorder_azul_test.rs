//! azul's encoders behind the recorder's traits. What these can do depends on the machine: on a Mac
//! VideoToolbox and AudioToolbox open and the tests encode for real; elsewhere the factory refuses in
//! words and the tests check the words. Either way nothing crashes.

use super::*;
use crate::recorder_mp4::{annexb_to_avcc, opus_packet_samples, VideoConfig};

#[test]
fn the_video_encoder_opens_and_makes_a_keyframe_with_its_size_or_says_why_not() {
    match AzulFactory.video(320, 240, 800) {
        Ok(mut codec) => {
            // A real encoder: a grey frame comes out as a keyframe whose parameter sets say 320 x 240.
            let frame = RawFrame { width: 320, height: 240, format: PixelFormat::Rgba8, bytes: vec![128; 320 * 240 * 4] };
            assert!(codec.submit(frame, true));
            codec.flush();
            let au = codec.poll().expect("a packet after the flush");
            let sample = annexb_to_avcc(&au);
            assert!(sample.keyframe);
            let config = VideoConfig::from_keyframe(&au).expect("the keyframe carries its parameter sets");
            assert_eq!((config.info.width, config.info.height), (320, 240));
            assert!(!codec.name().is_empty());
        }
        Err(why) => assert!(why.contains("no H.264 encoder"), "{why}"),
    }
}

#[test]
fn the_audio_encoder_opens_and_makes_opus_packets_of_20_ms_or_says_why_not() {
    match AzulFactory.audio(48_000, 1, 32) {
        Ok(mut codec) => {
            let tone: Vec<f32> = (0..48_000).map(|i| (i as f32 * 0.05).sin() * 0.3).collect();
            assert!(codec.submit(&tone));
            let mut packets = 0;
            while let Some(p) = codec.poll() {
                assert_eq!(opus_packet_samples(&p), Some(960), "a packet of 20 ms");
                packets += 1;
            }
            assert!(packets >= 40, "a second of sound is about 50 packets, got {packets}");
        }
        Err(why) => assert!(why.contains("no Opus encoder"), "{why}"),
    }
}
