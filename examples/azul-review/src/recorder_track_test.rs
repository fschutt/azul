//! A track's segment files on a map in memory: opening, fragments, rotation, a new picture size,
//! a file that fails. No window, no azul, no disk.

use std::{
    collections::BTreeMap,
    io::Cursor,
    sync::{Arc, Mutex},
};

use super::*;
use crate::recorder_mp4::{
    recorder_mp4_test::{first_voice_run, tracks_of, voice_samples},
    scan, TrackKind,
};

// ---- the store ----

#[derive(Default)]
struct Shared {
    files: BTreeMap<String, Vec<u8>>,
    syncs: usize,
    /// A file fails once it holds more than this many bytes.
    fail_over: Option<usize>,
}

#[derive(Clone, Default)]
struct MemStore(Arc<Mutex<Shared>>);

struct MemFile {
    name: String,
    shared: Arc<Mutex<Shared>>,
}

impl SegmentStore for MemStore {
    fn create(&mut self, rel_path: &str) -> io::Result<Box<dyn SegmentFile>> {
        let mut s = self.0.lock().unwrap();
        if s.files.contains_key(rel_path) {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, "the file exists"));
        }
        s.files.insert(rel_path.to_string(), Vec::new());
        Ok(Box::new(MemFile { name: rel_path.to_string(), shared: Arc::clone(&self.0) }))
    }
}

impl SegmentFile for MemFile {
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        let mut s = self.shared.lock().unwrap();
        let over = s.fail_over;
        let file = s.files.get_mut(&self.name).unwrap();
        if over.is_some_and(|limit| file.len() + bytes.len() > limit) {
            return Err(io::Error::other("No space left on device"));
        }
        file.extend_from_slice(bytes);
        Ok(())
    }
    fn sync(&mut self) -> io::Result<()> {
        self.shared.lock().unwrap().syncs += 1;
        Ok(())
    }
    fn len(&self) -> u64 {
        self.shared.lock().unwrap().files[&self.name].len() as u64
    }
}

impl MemStore {
    fn file(&self, name: &str) -> Vec<u8> {
        self.0.lock().unwrap().files[name].clone()
    }
    fn names(&self) -> Vec<String> {
        self.0.lock().unwrap().files.keys().cloned().collect()
    }
}

// ---- the fixtures ----

fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

fn annexb(nals: &[&[u8]]) -> Vec<u8> {
    nals.iter().flat_map(|n| [&[0, 0, 0, 1][..], n].concat()).collect()
}

fn key_720(tag: u8) -> Vec<u8> {
    annexb(&[&hex("2764001fac56805005b9"), &hex("28ee3cb0"), &[0x65, tag, 1, 2, 3, 4]])
}

fn key_2880(tag: u8) -> Vec<u8> {
    annexb(&[&hex("27640033ac56802d00e3f950"), &hex("28ee3cb0"), &[0x65, tag, 1, 2, 3, 4]])
}

fn delta(tag: u8) -> Vec<u8> {
    annexb(&[&[0x41, tag, 9, 9, 9, 9]])
}

fn opus(tag: u8) -> Vec<u8> {
    vec![0xf8, tag, 1, 2, 3, 4]
}

const SEC: u64 = 1_000_000;

fn video_writer(segment_us: u64) -> TrackWriter {
    TrackWriter::new("screen", Kind::Video, segment_us, 1, 1_764_108_000)
}

/// Feeds `seconds` of video at 10 fps with a keyframe every two seconds, starting at `from_us`.
fn feed(w: &mut TrackWriter, store: &mut MemStore, from_us: u64, seconds: u64) -> Vec<Change> {
    let mut changes = Vec::new();
    for i in 0..seconds * 10 {
        let t = from_us + i * SEC / 10;
        let au = if i % 20 == 0 { key_720(i as u8) } else { delta(i as u8) };
        changes.extend(w.push_video(store, t, &au).unwrap());
    }
    changes
}

// ---- opening ----

#[test]
fn the_first_keyframe_opens_the_first_segment_and_writes_the_head() {
    let mut store = MemStore::default();
    let mut w = video_writer(300 * SEC);
    let changes = w.push_video(&mut store, 2_500_000, &key_720(1)).unwrap();
    let [Change::Opened(seg)] = &changes[..] else { panic!("{changes:?}") };
    assert_eq!(seg.file, "screen/0001.mp4");
    assert_eq!(seg.start_us, 2_500_000, "the segment starts where its first picture is");
    assert_eq!((seg.width, seg.height), (1280, 720));
    assert!(!seg.complete && seg.end_us.is_none());
    assert!(seg.bytes > 100, "ftyp and moov are in the file already");
    assert_eq!(store.names(), vec!["screen/0001.mp4"]);
    assert!(store.0.lock().unwrap().syncs >= 1, "the head is synced");
    assert!(w.is_open());
    assert_eq!(w.open_since_us(), Some(2_500_000));
}

#[test]
fn pictures_before_the_first_keyframe_are_dropped_and_counted() {
    let mut store = MemStore::default();
    let mut w = video_writer(300 * SEC);
    for i in 0..5 {
        assert!(w.push_video(&mut store, i * 100_000, &delta(i as u8)).unwrap().is_empty());
    }
    assert_eq!(w.dropped_before_keyframe(), 5);
    assert!(store.names().is_empty(), "no file without a keyframe");
    assert!(!w.is_open());
    assert!(w.segments().is_empty());
}

#[test]
fn a_unit_with_only_parameter_sets_writes_nothing() {
    let mut store = MemStore::default();
    let mut w = video_writer(300 * SEC);
    let only_sets = annexb(&[&hex("2764001fac56805005b9"), &hex("28ee3cb0")]);
    assert!(w.push_video(&mut store, 0, &only_sets).unwrap().is_empty());
    assert!(store.names().is_empty());
}

#[test]
fn a_video_track_takes_no_sound_and_an_audio_track_no_pictures() {
    let mut store = MemStore::default();
    let mut v = video_writer(300 * SEC);
    assert!(matches!(v.push_audio(&mut store, 0, opus(0)), Err(TrackError::Config(_))));
    let mut a = TrackWriter::new("audio", Kind::Opus, 300 * SEC, 1, 0);
    assert!(matches!(a.push_video(&mut store, 0, &key_720(0)), Err(TrackError::Config(_))));
}

// ---- fragments and the manifest's view ----

#[test]
fn fragments_are_appended_and_synced_when_they_are_whole() {
    let mut store = MemStore::default();
    let mut w = video_writer(300 * SEC);
    feed(&mut w, &mut store, 0, 7);
    let bytes_now = store.file("screen/0001.mp4");
    let s = scan(&mut Cursor::new(&bytes_now)).unwrap();
    // Keyframes at 0, 2, 4 and 6 s: three fragments are whole, the fourth is still open.
    assert_eq!(s.fragments, 3);
    assert!(s.truncated || s.whole_end == bytes_now.len() as u64);
    assert_eq!(s.whole_end as usize, bytes_now.len(), "only whole fragments were written");
    assert!(store.0.lock().unwrap().syncs >= 4, "the head and each fragment were synced: {}", store.0.lock().unwrap().syncs);
}

#[test]
fn the_open_segment_is_listed_as_it_stands() {
    let mut store = MemStore::default();
    let mut w = video_writer(300 * SEC);
    feed(&mut w, &mut store, 1_000_000, 5);
    let segs = w.segments();
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].file, "screen/0001.mp4");
    assert_eq!((segs[0].start_us, segs[0].end_us, segs[0].complete), (1_000_000, None, false));
    assert_eq!(segs[0].samples, 50);
    assert_eq!(segs[0].bytes, store.file("screen/0001.mp4").len() as u64);
    assert_eq!(w.bytes(), segs[0].bytes);
}

// ---- rotation ----

#[test]
fn a_segment_closes_at_the_first_keyframe_after_its_length_and_the_next_one_opens() {
    let mut store = MemStore::default();
    let mut w = video_writer(10 * SEC);
    let changes = feed(&mut w, &mut store, 0, 25);
    // Opened at 0; the keyframe at 10 s closes it and opens the next; the one at 20 s again.
    let kinds: Vec<(&str, &str)> = changes
        .iter()
        .map(|c| match c {
            Change::Opened(s) => ("opened", s.file.as_str()),
            Change::Closed(s) => ("closed", s.file.as_str()),
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            ("opened", "screen/0001.mp4"),
            ("closed", "screen/0001.mp4"),
            ("opened", "screen/0002.mp4"),
            ("closed", "screen/0002.mp4"),
            ("opened", "screen/0003.mp4"),
        ]
    );
    let Change::Closed(first) = &changes[1] else { panic!() };
    assert_eq!((first.start_us, first.end_us, first.complete), (0, Some(10 * SEC), true));
    assert_eq!(first.samples, 100);
    let Change::Opened(second) = &changes[2] else { panic!() };
    assert_eq!(second.start_us, 10 * SEC, "the next begins where the last ended: no gap, no overlap");

    // The closed files are whole, finished and as long as the manifest says.
    for (name, samples) in [("screen/0001.mp4", 100), ("screen/0002.mp4", 100)] {
        let bytes = store.file(name);
        let s = scan(&mut Cursor::new(&bytes)).unwrap();
        assert!(s.finished && !s.truncated, "{name}");
        assert_eq!(s.samples, samples, "{name}");
        assert_eq!(s.duration_us(), 10 * SEC, "{name}");
    }
    let segs = w.segments();
    assert_eq!(segs.len(), 3);
    assert_eq!(segs.iter().filter(|s| s.complete).count(), 2);
    assert_eq!(segs[2].end_us, None);
}

#[test]
fn a_late_keyframe_is_waited_for_and_the_segment_runs_on() {
    let mut store = MemStore::default();
    let mut w = video_writer(5 * SEC);
    w.push_video(&mut store, 0, &key_720(0)).unwrap();
    // 12 s of deltas with no keyframe: the segment is over its length but cannot be cut.
    for i in 1..120 {
        assert!(w.push_video(&mut store, i * SEC / 10, &delta(i as u8)).unwrap().is_empty());
    }
    assert_eq!(w.segments().len(), 1);
    let changes = w.push_video(&mut store, 12 * SEC, &key_720(9)).unwrap();
    assert!(matches!(&changes[..], [Change::Closed(_), Change::Opened(_)]));
}

#[test]
fn a_new_picture_size_starts_a_new_segment_at_once() {
    let mut store = MemStore::default();
    let mut w = video_writer(300 * SEC);
    w.push_video(&mut store, 0, &key_720(0)).unwrap();
    w.push_video(&mut store, 100_000, &delta(1)).unwrap();
    // The window was resized: the encoder was reopened and its first keyframe has other parameter sets.
    let changes = w.push_video(&mut store, 5 * SEC, &key_2880(2)).unwrap();
    let [Change::Closed(old), Change::Opened(new)] = &changes[..] else { panic!("{changes:?}") };
    assert_eq!((old.file.as_str(), old.end_us), ("screen/0001.mp4", Some(5 * SEC)));
    assert_eq!((new.file.as_str(), new.width, new.height), ("screen/0002.mp4", 2880, 1800));
    // Each file says its own size.
    let a = scan(&mut Cursor::new(&store.file("screen/0001.mp4"))).unwrap();
    let b = scan(&mut Cursor::new(&store.file("screen/0002.mp4"))).unwrap();
    assert_eq!((a.width, a.height), (1280, 720));
    assert_eq!((b.width, b.height), (2880, 1800));
}

#[test]
fn the_same_parameter_sets_again_do_not_rotate_a_segment() {
    let mut store = MemStore::default();
    let mut w = video_writer(300 * SEC);
    for i in 0..5 {
        let changes = w.push_video(&mut store, i * SEC, &key_720(i as u8)).unwrap();
        assert_eq!(changes.len(), usize::from(i == 0), "only the first opens anything");
    }
    assert_eq!(store.names().len(), 1);
}

#[test]
fn numbering_goes_on_from_where_an_earlier_run_stopped() {
    let mut store = MemStore::default();
    let mut w = TrackWriter::new("screen", Kind::Video, 300 * SEC, 4, 0);
    let changes = w.push_video(&mut store, 0, &key_720(0)).unwrap();
    let [Change::Opened(seg)] = &changes[..] else { panic!() };
    assert_eq!(seg.file, "screen/0004.mp4");
}

#[test]
fn a_file_that_exists_is_never_overwritten() {
    let mut store = MemStore::default();
    store.create("screen/0001.mp4").unwrap();
    let mut w = video_writer(300 * SEC);
    let err = w.push_video(&mut store, 0, &key_720(0)).unwrap_err();
    assert!(matches!(err, TrackError::Io(_)), "{err}");
    assert!(w.failure().is_some());
}

// ---- finishing ----

#[test]
fn finishing_closes_the_segment_at_the_given_time() {
    let mut store = MemStore::default();
    let mut w = video_writer(300 * SEC);
    feed(&mut w, &mut store, 3 * SEC, 4);
    let changes = w.finish(8 * SEC).unwrap();
    let [Change::Closed(seg)] = &changes[..] else { panic!("{changes:?}") };
    assert_eq!((seg.start_us, seg.end_us, seg.complete), (3 * SEC, Some(8 * SEC), true));
    assert_eq!(seg.samples, 40);
    let s = scan(&mut Cursor::new(&store.file("screen/0001.mp4"))).unwrap();
    assert!(s.finished);
    assert_eq!(s.duration_us(), 5 * SEC);
    assert!(w.finish(9 * SEC).unwrap().is_empty(), "nothing is open any more");
    assert!(!w.is_open());
}

// ---- sound ----

#[test]
fn opus_packets_fill_segments_that_rotate_by_time() {
    let mut store = MemStore::default();
    let mut w = TrackWriter::new("audio", Kind::Opus, 2 * SEC, 1, 0);
    let mut opened = Vec::new();
    for i in 0..300_u64 {
        for c in w.push_audio(&mut store, 5 * SEC + i * 20_000, opus(i as u8)).unwrap() {
            if let Change::Opened(s) = c {
                opened.push((s.file, s.start_us));
            }
        }
    }
    w.finish(5 * SEC + 300 * 20_000).unwrap();
    // 6 s of sound in segments of 2 s: three files, each beginning at its first packet.
    assert_eq!(
        opened,
        vec![
            ("audio/0001.mp4".to_string(), 5 * SEC),
            ("audio/0002.mp4".to_string(), 7 * SEC),
            ("audio/0003.mp4".to_string(), 9 * SEC),
        ]
    );
    for name in ["audio/0001.mp4", "audio/0002.mp4", "audio/0003.mp4"] {
        let s = scan(&mut Cursor::new(&store.file(name))).unwrap();
        assert_eq!((s.kind, s.codec, s.finished), (Some(TrackKind::Audio), Some(*b"Opus"), true), "{name}");
        assert_eq!((s.samples, s.duration_us()), (100, 2 * SEC), "{name}");
        assert_eq!(s.first_us(), 0, "{name}: a segment's own time starts at zero");
    }
}

// ---- failure ----

#[test]
fn a_write_that_fails_breaks_the_track_in_words_and_the_rest_is_dropped_quietly() {
    let mut store = MemStore::default();
    store.0.lock().unwrap().fail_over = Some(2_000);
    let mut w = video_writer(300 * SEC);
    let mut failure = None;
    for i in 0..200 {
        let au = if i % 20 == 0 { key_720(i as u8) } else { delta(i as u8) };
        match w.push_video(&mut store, i * 100_000, &au) {
            Ok(_) => {}
            Err(e) => {
                failure = Some(e);
                break;
            }
        }
    }
    let err = failure.expect("the disk filled up");
    assert!(err.to_string().contains("No space left on device"), "{err}");
    assert!(w.failure().unwrap().contains("No space left on device"));
    // Nothing more is taken and nothing panics.
    assert!(w.push_video(&mut store, 99 * SEC, &key_720(1)).unwrap().is_empty());
    assert!(w.finish(100 * SEC).unwrap().is_empty());
    // What was written is listed, as not complete: a scan will find where it is whole.
    let segs = w.segments();
    assert_eq!(segs.len(), 1);
    assert!(!segs[0].complete);
}

// ---- the camera's files that carry the voice ----

/// A picture track that carries the voice makes its segments files of two tracks, and the voice's packets go
/// into the open segment at their times, after the segment's start.
///
/// Mutation: a voice that is not written; a packet before the segment's start written at its start.
#[test]
fn a_picture_track_that_carries_the_voice_writes_files_of_two_tracks() {
    let mut store = MemStore::default();
    let mut t = TrackWriter::new("camera", Kind::Video, 60_000_000, 1, 0).with_voice(true);
    // The voice before the first keyframe has no file to go in: it is dropped here (the voice's own track has it).
    assert!(t.push_voice(&mut store, 0, opus(0)).unwrap().is_empty());
    assert!(t.push_voice(&mut store, 100_000, opus(1)).unwrap().is_empty());
    t.push_video(&mut store, 1_000_000, &key_720(1)).unwrap();
    for i in 0..60_u64 {
        let at = 1_000_000 + i * 33_333;
        if i > 0 {
            t.push_video(&mut store, at, &delta(i as u8)).unwrap();
        }
        // The voice packets of 20 ms, with one before the segment's start among them.
        t.push_voice(&mut store, at.saturating_sub(40_000), opus(i as u8)).unwrap();
    }
    t.finish(3_000_000).unwrap();
    let file = store.file("camera/0001.mp4");
    assert_eq!(tracks_of(&file), 2, "a picture and a voice");
    // The packets at the segment's start or after it are in the file (the two before it are not): 58 of the 60.
    assert_eq!(voice_samples(&file), 58);
    let s = scan(&mut Cursor::new(&file)).unwrap();
    assert!(s.finished && !s.truncated);
    assert_eq!((s.kind, s.samples), (Some(TrackKind::Video), 60), "the picture is what the file is scanned by");
}

/// A picture track that does not carry the voice writes files of one track, and takes a voice packet for nothing.
///
/// Mutation: a voice track in every picture file.
#[test]
fn a_picture_track_without_the_voice_writes_files_of_one_track_and_ignores_it() {
    let mut store = MemStore::default();
    let mut t = TrackWriter::new("camera", Kind::Video, 60_000_000, 1, 0);
    t.push_video(&mut store, 0, &key_720(1)).unwrap();
    assert!(t.push_voice(&mut store, 10_000, opus(1)).unwrap().is_empty());
    t.finish(1_000_000).unwrap();
    assert_eq!(tracks_of(&store.file("camera/0001.mp4")), 1);
    // And a sound track takes no voice for a copy.
    let mut a = TrackWriter::new("audio", Kind::Opus, 60_000_000, 1, 0).with_voice(true);
    assert!(a.push_voice(&mut store, 0, opus(2)).unwrap().is_empty());
    assert!(!a.is_open());
}

/// The voice goes on into the next segment when a picture track rotates its file, and a file's voice begins at
/// that file's own start.
///
/// Mutation: a voice that is lost at a rotation; a voice's time that is the session's and not the file's.
#[test]
fn the_voice_goes_on_into_the_next_segment_and_is_timed_from_its_start() {
    let mut store = MemStore::default();
    // Segments of two seconds: the second begins at the keyframe at 2 s.
    let mut t = TrackWriter::new("camera", Kind::Video, 2_000_000, 1, 0).with_voice(true);
    for i in 0..9_u64 {
        let at = i * 500_000;
        let au = if i % 4 == 0 { key_720(i as u8) } else { delta(i as u8) };
        t.push_video(&mut store, at, &au).unwrap();
        for k in 0..25_u64 {
            t.push_voice(&mut store, at + k * 20_000, opus(k as u8)).unwrap();
        }
    }
    t.finish(4_500_000).unwrap();
    assert_eq!(store.names(), vec!["camera/0001.mp4", "camera/0002.mp4", "camera/0003.mp4"]);
    for name in ["camera/0001.mp4", "camera/0002.mp4"] {
        let file = store.file(name);
        assert_eq!(tracks_of(&file), 2, "{name}");
        let s = scan(&mut Cursor::new(&file)).unwrap();
        assert_eq!(s.first_us(), 0, "{name}: the picture begins at the file's own start");
    }
    // The voice of the second file begins at its start (0), not at the session's 2 s: read its first voice run's time.
    let second = store.file("camera/0002.mp4");
    let (voice_base, voice_samples) = first_voice_run(&second).expect("a voice run");
    assert!(voice_base < 960 * 50, "the voice's time in the file is under the file's first second: {voice_base}");
    assert!(voice_samples > 0);
}

/// The voice carried in a file is cut and recovered with it: a file cut mid-fragment is whole up to the fragment before,
/// and finishing a track that carries the voice reports the segment as any other.
///
/// Mutation: a voice run that makes the scanner count the voice's samples as the picture's.
#[test]
fn a_file_with_the_voice_in_it_is_a_whole_file_up_to_its_last_whole_fragment() {
    let mut store = MemStore::default();
    let mut t = TrackWriter::new("camera", Kind::Video, 60_000_000, 1, 0).with_voice(true).with_limits(Limits::chunked(1_000_000));
    t.push_video(&mut store, 0, &key_720(0)).unwrap();
    for i in 1..150_u64 {
        let at = i * 33_333;
        t.push_video(&mut store, at, &if i % 30 == 0 { key_720(i as u8) } else { delta(i as u8) }).unwrap();
        t.push_voice(&mut store, at, opus(i as u8)).unwrap();
    }
    // Not finished: the file as a kill leaves it.
    let file = store.file("camera/0001.mp4");
    let s = scan(&mut Cursor::new(&file)).unwrap();
    assert!(s.fragments >= 3 && !s.finished);
    assert_eq!(s.samples % 30, 0, "whole chunks of 30 pictures; the voice's samples are not in the count: {}", s.samples);
    let cut = scan(&mut Cursor::new(&file[..s.whole_end as usize])).unwrap();
    assert!(!cut.truncated && cut.fragments == s.fragments);
}
