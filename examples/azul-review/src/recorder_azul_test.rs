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

/// Where a macOS machine keeps the tool that asks AVFoundation about a file.
const AVMEDIAINFO: &str = "/usr/bin/avmediainfo";

#[test]
fn what_the_real_encoders_make_goes_through_the_track_writer_into_files_the_system_reads() {
    use std::{fs::File, process::Command};

    use crate::{
        recorder_fs::DiskStore,
        recorder_mp4::{scan, TrackKind},
        recorder_track::{Kind, TrackWriter},
    };

    let (Ok(mut video), Ok(mut voice)) = (AzulFactory.video(640, 360, 2_000), AzulFactory.audio(48_000, 1, 32)) else {
        return; // no hardware encoders here: the two tests above checked the words
    };
    // Two seconds of a moving picture and of a tone, through the real encoders.
    for i in 0..60_u32 {
        let mut bytes = vec![0_u8; 640 * 360 * 4];
        for (n, px) in bytes.chunks_exact_mut(4).enumerate() {
            let (x, y) = ((n % 640) as u32, (n / 640) as u32);
            px.copy_from_slice(&[((x + i * 8) % 256) as u8, ((y + i * 4) % 256) as u8, (i * 4) as u8, 255]);
        }
        assert!(video.submit(RawFrame { width: 640, height: 360, format: PixelFormat::Rgba8, bytes }, i % 30 == 0));
    }
    video.flush();
    let mut aus = Vec::new();
    while let Some(au) = video.poll() {
        aus.push(au);
    }
    assert!(aus.len() >= 30, "{} access units for 60 pictures", aus.len());
    let tone: Vec<f32> = (0..96_000).map(|i| (i as f32 * 0.05).sin() * 0.3).collect();
    voice.submit(&tone);
    let mut packets = Vec::new();
    while let Some(p) = voice.poll() {
        packets.push(p);
    }

    let dir = std::env::temp_dir().join(format!("azreview-azul-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut store = DiskStore::new(dir.clone());
    let mut screen = TrackWriter::new("screen", Kind::Video, 300_000_000, 1, 0);
    for (i, au) in aus.iter().enumerate() {
        screen.push_video(&mut store, i as u64 * 33_333, au).unwrap();
    }
    screen.finish(aus.len() as u64 * 33_333).unwrap();
    let mut audio = TrackWriter::new("audio", Kind::Opus, 300_000_000, 1, 0);
    for (i, p) in packets.iter().enumerate() {
        audio.push_audio(&mut store, i as u64 * 20_000, p.clone()).unwrap();
    }
    audio.finish(packets.len() as u64 * 20_000).unwrap();

    let v = scan(&mut File::open(dir.join("screen/0001.mp4")).unwrap()).unwrap();
    assert!(v.finished && !v.truncated, "{v:?}");
    assert_eq!((v.kind, v.codec, v.width, v.height), (Some(TrackKind::Video), Some(*b"avc1"), 640, 360));
    assert_eq!(v.samples as usize, aus.len());
    let a = scan(&mut File::open(dir.join("audio/0001.mp4")).unwrap()).unwrap();
    assert!(a.finished && a.samples as usize == packets.len() && a.codec == Some(*b"Opus"), "{a:?}");

    // AVFoundation, which QuickTime Player is made of, reads both.
    if std::path::Path::new(AVMEDIAINFO).exists() {
        for file in ["screen/0001.mp4", "audio/0001.mp4"] {
            let out = Command::new(AVMEDIAINFO).arg(dir.join(file)).output().unwrap();
            let text = String::from_utf8_lossy(&out.stdout);
            assert!(text.contains("Movie analyzed with 0 error"), "{file}: {text}");
            assert!(text.contains("System support for decoding this track: Yes"), "{file}: {text}");
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
