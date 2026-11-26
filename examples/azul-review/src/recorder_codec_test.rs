//! The stand-in encoders: what they make is what a muxer accepts, and their scripted delays and
//! refusals do what they say. No window, no azul.

use super::*;
use crate::recorder_mp4::{annexb_to_avcc, opus_packet_samples, parse_sps, VideoConfig};

fn frame(n: u8) -> RawFrame {
    RawFrame { width: 4, height: 4, format: PixelFormat::Rgba8, bytes: vec![n; 64] }
}

#[test]
fn a_made_sps_gives_back_the_size_it_was_made_for() {
    for (w, h) in [(1280, 720), (2880, 1800), (2560, 1664), (642, 478), (16, 16), (1920, 1080)] {
        let sps = parse_sps(&synthetic_sps(w, h)).unwrap_or_else(|e| panic!("{w}x{h}: {e}"));
        assert_eq!((sps.width, sps.height), (w, h));
    }
}

#[test]
fn the_first_picture_is_a_keyframe_with_parameter_sets_and_the_rest_are_deltas() {
    let mut c = MockVideoCodec::new(1280, 720, VideoScript::default());
    assert!(c.submit(frame(0), false), "taken");
    assert!(c.submit(frame(1), false));
    assert!(c.submit(frame(2), true), "a keyframe asked for");
    let first = annexb_to_avcc(&c.poll().unwrap());
    assert!(first.keyframe && first.sps.is_some() && first.pps.is_some(), "the first is a keyframe whatever was asked");
    let second = annexb_to_avcc(&c.poll().unwrap());
    assert!(!second.keyframe && second.sps.is_none());
    let third = annexb_to_avcc(&c.poll().unwrap());
    assert!(third.keyframe);
    assert!(c.poll().is_none());
    // The keyframe can start a file: its description reads back as the size.
    let au = {
        let mut d = MockVideoCodec::new(2880, 1800, VideoScript::default());
        d.submit(frame(0), true);
        d.poll().unwrap()
    };
    let config = VideoConfig::from_keyframe(&au).unwrap();
    assert_eq!((config.info.width, config.info.height), (2880, 1800));
}

#[test]
fn a_delayed_encoder_gives_a_packet_only_after_its_latency_and_flushes_the_rest() {
    let mut c = MockVideoCodec::new(64, 64, VideoScript { latency: 2, ..VideoScript::default() });
    c.submit(frame(0), false);
    c.submit(frame(1), false);
    assert!(c.poll().is_none(), "two pictures in, none out yet");
    c.submit(frame(2), false);
    assert!(c.poll().is_some());
    assert!(c.poll().is_none());
    c.flush();
    assert!(c.poll().is_some());
    assert!(c.poll().is_some());
    assert!(c.poll().is_none());
}

#[test]
fn an_encoder_that_is_behind_refuses_every_nth_picture() {
    let mut c = MockVideoCodec::new(64, 64, VideoScript { refuse_every: 3, ..VideoScript::default() });
    let taken: Vec<bool> = (0..9).map(|i| c.submit(frame(i), false)).collect();
    assert_eq!(taken, vec![true, true, false, true, true, false, true, true, false]);
    assert_eq!(c.taken.len(), 6);
    let mut units = 0;
    while c.poll().is_some() {
        units += 1;
    }
    assert_eq!(units, 6, "a refused picture makes no packet");
}

#[test]
fn a_stand_in_keeps_only_as_many_pictures_as_it_is_told_to_and_numbers_them_all() {
    // A long soak must not hold every picture it ever took; the packets still follow the pictures.
    let mut c = MockVideoCodec::new(64, 64, VideoScript { keep: 2, ..VideoScript::default() });
    for i in 0..5 {
        assert!(c.submit(frame(i), false));
    }
    assert_eq!(c.taken.len(), 2, "the last two");
    assert_eq!((c.taken[0].bytes[0], c.taken[1].bytes[0]), (3, 4), "the newest ones, in order");
    let tags: Vec<u8> = std::iter::from_fn(|| c.poll()).map(|au| au[au.len() - 7]).collect();
    assert_eq!(tags, vec![0, 1, 2, 3, 4], "every picture made its packet, numbered from the first");
    let mut none = MockVideoCodec::new(64, 64, VideoScript { keep: 0, ..VideoScript::default() });
    assert!(none.submit(frame(0), false) && none.submit(frame(1), false));
    assert!(none.taken.is_empty(), "none kept");
    let first = annexb_to_avcc(&none.poll().unwrap());
    let second = annexb_to_avcc(&none.poll().unwrap());
    assert!(first.keyframe && !second.keyframe, "the first picture is still the keyframe when none are kept");
}

#[test]
fn the_stand_in_says_what_it_is() {
    let c = MockVideoCodec::new(1, 1, VideoScript { hardware: false, ..VideoScript::default() });
    assert_eq!(c.name(), "mock H.264");
    assert!(!c.hardware());
    assert!(MockVideoCodec::new(1, 1, VideoScript::default()).hardware());
}

#[test]
fn sound_makes_a_packet_for_every_20_milliseconds_whatever_the_size_of_the_chunks() {
    let mut a = MockAudioCodec::new(1);
    for chunk in [480_usize, 1_000, 5, 2_000, 395] {
        assert!(a.submit(&vec![0.1; chunk]));
    }
    // 3 880 samples = 4 packets of 960 and 40 left over.
    let mut packets = Vec::new();
    while let Some(p) = a.poll() {
        packets.push(p);
    }
    assert_eq!(packets.len(), 4);
    assert!(packets.iter().all(|p| opus_packet_samples(p) == Some(960)), "each is an Opus packet of 20 ms");
    // Stereo needs twice the samples for a packet.
    let mut s = MockAudioCodec::new(2);
    s.submit(&vec![0.0; 1_920]);
    assert!(s.poll().is_some());
    assert!(s.poll().is_none());
}

#[test]
fn the_factory_opens_stand_ins_and_can_be_told_to_fail_in_words() {
    let f = MockFactory::default();
    assert!(f.video(640, 480, 800).is_ok());
    assert!(f.audio(48_000, 1, 32).is_ok());
    let broken = MockFactory { no_video: true, no_audio: true, ..MockFactory::default() };
    assert_eq!(broken.video(1, 1, 1).err().unwrap(), "no H.264 encoder on this machine");
    assert_eq!(broken.audio(1, 1, 1).err().unwrap(), "no Opus encoder on this machine");
}

/// Work given to `in_background` is not waited for (the caller is back long before a slow job is done) and is done on a
/// thread of its own with the name it was given: the video encoder's warm-up, a few hundred milliseconds on a Mac, must not
/// be spent on the window's thread.
///
/// Mutation: work done on the caller's thread; a thread without the name; work that is never done.
#[test]
fn background_work_is_not_waited_for_and_runs_on_a_thread_of_its_own() {
    let (done, finished) = std::sync::mpsc::channel();
    let began = std::time::Instant::now();
    in_background("recorder-test-background", move || {
        std::thread::sleep(std::time::Duration::from_millis(400));
        let me = std::thread::current();
        let _ = done.send((me.name().map(str::to_string), me.id()));
    });
    assert!(began.elapsed() < std::time::Duration::from_millis(200), "the caller waited {:?} for work of 400 ms", began.elapsed());
    let (name, thread) = finished.recv_timeout(std::time::Duration::from_secs(10)).expect("the work was done");
    assert_eq!(name.as_deref(), Some("recorder-test-background"));
    assert_ne!(thread, std::thread::current().id(), "on a thread of its own");
}
