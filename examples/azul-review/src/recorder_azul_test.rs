//! azul's encoders behind the recorder's traits. What these can do depends on the machine: on a Mac
//! VideoToolbox and AudioToolbox open and the tests encode for real; elsewhere the factory refuses in
//! words and the tests check the words; on a virtual machine the encoder may open and then refuse every
//! picture (no hardware encoder behind it). Each half says what it does here, a half that cannot run says
//! `SKIPPED` and why, and a run that sets `AZ_REQUIRE_ENCODERS=1` (the one on a Mac that checks the
//! adapters) fails instead of skipping: a test that passed without testing anything is no test.

use super::*;
use crate::recorder_mp4::{annexb_to_avcc, opus_packet_samples, VideoConfig};

/// The test needs the machine's real encoder and this machine does not give it: say so and go, or, under
/// `AZ_REQUIRE_ENCODERS=1`, fail.
fn skipped(what: &str, why: &str) {
    assert!(std::env::var("AZ_REQUIRE_ENCODERS").as_deref() != Ok("1"), "AZ_REQUIRE_ENCODERS=1, and {what}: {why}");
    eprintln!("SKIPPED {what}: {why}");
}

/// A picture of `w` x `h` that moves with `i`.
fn picture(w: u32, h: u32, i: u32) -> RawFrame {
    let mut bytes = vec![0_u8; w as usize * h as usize * 4];
    for (n, px) in bytes.chunks_exact_mut(4).enumerate() {
        let (x, y) = ((n % w as usize) as u32, (n / w as usize) as u32);
        px.copy_from_slice(&[((x + i * 8) % 256) as u8, ((y + i * 4) % 256) as u8, (i * 4) as u8, 255]);
    }
    RawFrame { width: w, height: h, format: PixelFormat::Rgba8, bytes }
}

/// The access units `pictures` made-up pictures of `w` x `h` come out of a real encoder as, or why it
/// cannot be done here (said by [`skipped`]).
fn real_video(w: u32, h: u32, pictures: u32) -> Option<Vec<Vec<u8>>> {
    let mut codec = match AzulFactory.video(w, h, 2_000) {
        Ok(codec) => codec,
        Err(why) => {
            assert!(why.contains("no H.264 encoder"), "{why}");
            skipped("the real video encoder", &why);
            return None;
        }
    };
    eprintln!("video encoder: {} (hardware: {})", codec.name(), codec.hardware());
    for i in 0..pictures {
        if !codec.submit(picture(w, h, i), i % 30 == 0) {
            skipped("the real video encoder", &format!("it opened ({}, hardware: {}) and refused picture {i}: a machine without an H.264 encoder behind it", codec.name(), codec.hardware()));
            return None;
        }
    }
    codec.flush();
    let mut aus = Vec::new();
    while let Some(au) = codec.poll() {
        aus.push(au);
    }
    if aus.is_empty() {
        skipped("the real video encoder", &format!("it took {pictures} pictures ({}, hardware: {}) and gave no packet", codec.name(), codec.hardware()));
        return None;
    }
    Some(aus)
}

/// The Opus packets `seconds` of a made-up tone come out of a real encoder as, or why not.
fn real_voice(seconds: usize) -> Option<Vec<Vec<u8>>> {
    let mut codec = match AzulFactory.audio(48_000, 1, 32) {
        Ok(codec) => codec,
        Err(why) => {
            assert!(why.contains("no Opus encoder"), "{why}");
            skipped("the real Opus encoder", &why);
            return None;
        }
    };
    eprintln!("audio encoder: {}", codec.name());
    let tone: Vec<f32> = (0..48_000 * seconds).map(|i| (i as f32 * 0.05).sin() * 0.3).collect();
    if !codec.submit(&tone) {
        skipped("the real Opus encoder", &format!("it opened ({}) and refused {seconds} s of sound", codec.name()));
        return None;
    }
    let mut packets = Vec::new();
    while let Some(p) = codec.poll() {
        packets.push(p);
    }
    if packets.is_empty() {
        skipped("the real Opus encoder", &format!("it took {seconds} s of sound ({}) and gave no packet", codec.name()));
        return None;
    }
    Some(packets)
}

#[test]
fn the_video_encoder_opens_and_makes_a_keyframe_with_its_size_or_says_why_not() {
    // A real encoder: a grey frame comes out as a keyframe whose parameter sets say 320 x 240.
    let Some(mut codec) = AzulFactory.video(320, 240, 800).map_err(|why| {
        assert!(why.contains("no H.264 encoder"), "{why}");
        skipped("the real video encoder", &why);
    }).ok() else { return };
    eprintln!("video encoder: {} (hardware: {})", codec.name(), codec.hardware());
    let frame = RawFrame { width: 320, height: 240, format: PixelFormat::Rgba8, bytes: vec![128; 320 * 240 * 4] };
    if !codec.submit(frame, true) {
        skipped("the real video encoder", &format!("it opened ({}, hardware: {}) and refused the first picture: a machine without an H.264 encoder behind it", codec.name(), codec.hardware()));
        return;
    }
    codec.flush();
    let Some(au) = codec.poll() else {
        skipped("the real video encoder", "it took the picture and gave no packet after the flush");
        return;
    };
    let sample = annexb_to_avcc(&au);
    assert!(sample.keyframe);
    let config = VideoConfig::from_keyframe(&au).expect("the keyframe carries its parameter sets");
    assert_eq!((config.info.width, config.info.height), (320, 240));
    assert!(!codec.name().is_empty());
}

#[test]
fn the_audio_encoder_opens_and_makes_opus_packets_of_20_ms_or_says_why_not() {
    let Some(packets) = real_voice(1) else { return };
    for p in &packets {
        assert_eq!(opus_packet_samples(p), Some(960), "a packet of 20 ms");
    }
    assert!(packets.len() >= 40, "a second of sound is about 50 packets, got {}", packets.len());
}

/// Where a macOS machine keeps the tool that asks AVFoundation about a file.
const AVMEDIAINFO: &str = "/usr/bin/avmediainfo";

/// Where ffprobe is, when it is anywhere.
fn ffprobe() -> Option<std::path::PathBuf> {
    ["/opt/homebrew/bin/ffprobe", "/usr/local/bin/ffprobe", "/usr/bin/ffprobe"].iter().map(std::path::PathBuf::from).find(|p| p.is_file())
}

/// What the real encoders make goes through the track writer into files the system reads: a made-up
/// moving picture and tone (nothing from any device) through VideoToolbox and AudioToolbox, the files
/// scanned, AVFoundation's `avmediainfo` and ffprobe asked. Each half runs where its encoder works: a
/// virtual machine that gives no H.264 encoder says so and still checks the sound.
#[test]
fn what_the_real_encoders_make_goes_through_the_track_writer_into_files_the_system_reads() {
    use std::{fs::File, process::Command};

    use crate::{
        recorder_fs::DiskStore,
        recorder_mp4::{scan, TrackKind},
        recorder_track::{Kind, TrackWriter},
    };

    // Two seconds of a moving picture and of a tone, through the real encoders where they work.
    let aus = real_video(640, 360, 60);
    let packets = real_voice(2);
    if aus.is_none() && packets.is_none() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("azreview-azul-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut store = DiskStore::new(dir.clone());
    let mut files: Vec<(&str, f64, &str)> = Vec::new();
    if let Some(aus) = &aus {
        assert!(aus.len() >= 30, "{} access units for 60 pictures", aus.len());
        let mut screen = TrackWriter::new("screen", Kind::Video, 300_000_000, 1, 0);
        for (i, au) in aus.iter().enumerate() {
            screen.push_video(&mut store, i as u64 * 33_333, au).unwrap();
        }
        screen.finish(aus.len() as u64 * 33_333).unwrap();
        let v = scan(&mut File::open(dir.join("screen/0001.mp4")).unwrap()).unwrap();
        assert!(v.finished && !v.truncated, "{v:?}");
        assert_eq!((v.kind, v.codec, v.width, v.height), (Some(TrackKind::Video), Some(*b"avc1"), 640, 360));
        assert_eq!(v.samples as usize, aus.len());
        files.push(("screen/0001.mp4", aus.len() as f64 * 0.033_333, "H.264"));
    }
    if let Some(packets) = &packets {
        let mut audio = TrackWriter::new("audio", Kind::Opus, 300_000_000, 1, 0);
        for (i, p) in packets.iter().enumerate() {
            audio.push_audio(&mut store, i as u64 * 20_000, p.clone()).unwrap();
        }
        audio.finish(packets.len() as u64 * 20_000).unwrap();
        let a = scan(&mut File::open(dir.join("audio/0001.mp4")).unwrap()).unwrap();
        assert!(a.finished && a.samples as usize == packets.len() && a.codec == Some(*b"Opus"), "{a:?}");
        files.push(("audio/0001.mp4", packets.len() as f64 * 0.02, "Opus"));
    }

    // What they cost in bytes: size and bitrate of what the encoders made, for the seconds they were given.
    for (file, secs, what) in &files {
        let bytes = std::fs::metadata(dir.join(file)).unwrap().len();
        eprintln!("{file}: {what}, {bytes} bytes for {secs:.2} s = {:.0} kbit/s", bytes as f64 * 8.0 / secs / 1000.0);
    }

    // AVFoundation, which QuickTime Player is made of, reads them.
    if std::path::Path::new(AVMEDIAINFO).exists() {
        for (file, _, _) in &files {
            let out = Command::new(AVMEDIAINFO).arg(dir.join(file)).output().unwrap();
            let text = String::from_utf8_lossy(&out.stdout);
            eprintln!("avmediainfo {file}:\n{}", text.lines().take(40).collect::<Vec<_>>().join("\n"));
            assert!(text.contains("Movie analyzed with 0 error"), "{file}: {text}");
            assert!(text.contains("System support for decoding this track: Yes"), "{file}: {text}");
        }
    }
    if let Some(ffprobe) = ffprobe() {
        for (file, _, _) in &files {
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
            if dir.join(file).exists() {
                let to = std::path::Path::new(&keep).join(name);
                std::fs::copy(dir.join(file), &to).unwrap();
                eprintln!("kept: {}", to.display());
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
