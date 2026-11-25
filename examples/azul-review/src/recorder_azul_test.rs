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

/// The test below needs the machine's real encoders. Where they do not open it says so and goes (a test
/// that passed without testing anything is no test), and a run that sets `AZ_REQUIRE_ENCODERS=1` (the
/// one on a Mac that checks the adapters does) fails instead of going.
fn skipped(why: &str) {
    assert!(std::env::var("AZ_REQUIRE_ENCODERS").as_deref() != Ok("1"), "AZ_REQUIRE_ENCODERS=1, and {why}");
    eprintln!("SKIPPED the real encoders through the track writer: {why}");
}

/// Where ffprobe is, when it is anywhere.
fn ffprobe() -> Option<std::path::PathBuf> {
    ["/opt/homebrew/bin/ffprobe", "/usr/local/bin/ffprobe", "/usr/bin/ffprobe"].iter().map(std::path::PathBuf::from).find(|p| p.is_file())
}

#[test]
fn what_the_real_encoders_make_goes_through_the_track_writer_into_files_the_system_reads() {
    use std::{fs::File, process::Command};

    use crate::{
        recorder_fs::DiskStore,
        recorder_mp4::{scan, TrackKind},
        recorder_track::{Kind, TrackWriter},
    };

    let (video, voice) = (AzulFactory.video(640, 360, 2_000), AzulFactory.audio(48_000, 1, 32));
    let (mut video, mut voice) = match (video, voice) {
        (Ok(video), Ok(voice)) => (video, voice),
        (v, a) => {
            skipped(&format!("no encoders open here (video: {}; audio: {})", v.err().unwrap_or_else(|| String::from("opens")), a.err().unwrap_or_else(|| String::from("opens"))));
            return;
        }
    };
    eprintln!("encoders: {} (hardware: {}), {}", video.name(), video.hardware(), voice.name());
    // Two seconds of a moving picture and of a tone, made up, through the real encoders: nothing here
    // comes from a camera, a microphone or a screen.
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

    // What they cost in bytes: size and bitrate of what the encoders made, for the seconds they were given.
    let (video_secs, audio_secs) = (aus.len() as f64 * 0.033_333, packets.len() as f64 * 0.02);
    for (file, secs, what) in [("screen/0001.mp4", video_secs, "H.264"), ("audio/0001.mp4", audio_secs, "Opus")] {
        let bytes = std::fs::metadata(dir.join(file)).unwrap().len();
        eprintln!("{file}: {what}, {bytes} bytes for {secs:.2} s = {:.0} kbit/s", bytes as f64 * 8.0 / secs / 1000.0);
    }

    // AVFoundation, which QuickTime Player is made of, reads both.
    if std::path::Path::new(AVMEDIAINFO).exists() {
        for file in ["screen/0001.mp4", "audio/0001.mp4"] {
            let out = Command::new(AVMEDIAINFO).arg(dir.join(file)).output().unwrap();
            let text = String::from_utf8_lossy(&out.stdout);
            eprintln!("avmediainfo {file}:\n{}", text.lines().take(40).collect::<Vec<_>>().join("\n"));
            assert!(text.contains("Movie analyzed with 0 error"), "{file}: {text}");
            assert!(text.contains("System support for decoding this track: Yes"), "{file}: {text}");
        }
    }
    if let Some(ffprobe) = ffprobe() {
        for file in ["screen/0001.mp4", "audio/0001.mp4"] {
            let out = Command::new(&ffprobe)
                .args(["-v", "error", "-show_entries", "stream=codec_name,codec_type,width,height,r_frame_rate,sample_rate,channels,bit_rate:format=duration,size,bit_rate", "-of", "default=nw=1"])
                .arg(dir.join(file))
                .output()
                .unwrap();
            assert!(out.status.success(), "ffprobe {file}: {}", String::from_utf8_lossy(&out.stderr));
            eprintln!("ffprobe {file}:\n{}", String::from_utf8_lossy(&out.stdout));
        }
    }
    // One small clip may be kept to look at, where the caller names a folder for it.
    if let Ok(keep) = std::env::var("AZREVIEW_KEEP_CLIP") {
        std::fs::create_dir_all(&keep).unwrap();
        for (file, name) in [("screen/0001.mp4", "synthetic-picture.mp4"), ("audio/0001.mp4", "synthetic-tone.mp4")] {
            let to = std::path::Path::new(&keep).join(name);
            std::fs::copy(dir.join(file), &to).unwrap();
            eprintln!("kept: {}", to.display());
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
