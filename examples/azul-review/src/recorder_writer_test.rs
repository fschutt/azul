//! The writer of a session: its rules on files in memory, then its thread. No window, no azul, no
//! disk.

use std::{
    collections::BTreeMap,
    io::{self, Cursor},
    sync::{Arc, Mutex},
};

use super::*;
use crate::{
    recorder_events::{event_channel, Caps},
    recorder_clock::MockClock,
    recorder_manifest::{AppInfo, ClockInfo, HostInfo, Segment, Workspace},
    recorder_mp4::scan,
    recorder_track::SegmentFile,
};

// ---- the store, the transcript file and the manifest file, in memory ----

#[derive(Default)]
struct Shared {
    files: BTreeMap<String, Vec<u8>>,
    /// Files fail once they hold more than this: per path prefix.
    fail_prefix: Option<(String, usize)>,
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
        let fail = s.fail_prefix.clone();
        let file = s.files.get_mut(&self.name).unwrap();
        if let Some((prefix, limit)) = fail {
            if self.name.starts_with(&prefix) && file.len() + bytes.len() > limit {
                return Err(io::Error::other("No space left on device"));
            }
        }
        file.extend_from_slice(bytes);
        Ok(())
    }
    fn sync(&mut self) -> io::Result<()> {
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

/// A transcript file whose contents the test can read.
#[derive(Clone, Default)]
struct MemLines(Arc<Mutex<Vec<u8>>>);

impl Write for MemLines {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A manifest file: every text written, newest last.
#[derive(Clone, Default)]
struct MemManifest {
    writes: Arc<Mutex<Vec<String>>>,
    fail: Arc<Mutex<bool>>,
}

impl ManifestSink for MemManifest {
    fn write(&mut self, text: &str) -> io::Result<()> {
        if *self.fail.lock().unwrap() {
            return Err(io::Error::other("the disk is read-only"));
        }
        self.writes.lock().unwrap().push(text.to_string());
        Ok(())
    }
}

// ---- the fixtures ----

fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

fn annexb(nals: &[&[u8]]) -> Vec<u8> {
    nals.iter().flat_map(|n| [&[0, 0, 0, 1][..], n].concat()).collect()
}

fn key(tag: u8) -> Vec<u8> {
    annexb(&[&hex("2764001fac56805005b9"), &hex("28ee3cb0"), &[0x65, tag, 1, 2, 3, 4]])
}

fn delta(tag: u8) -> Vec<u8> {
    annexb(&[&[0x41, tag, 9, 9, 9, 9]])
}

fn opus(tag: u8) -> Vec<u8> {
    vec![0xf8, tag, 1, 2, 3, 4]
}

const SEC: u64 = 1_000_000;

fn manifest() -> Manifest {
    Manifest::begin(
        AppInfo { name: String::from("AzReview"), version: String::from("0.1.0") },
        "2025-11-25_22-00-00",
        Workspace { root: String::from("/w"), name: String::from("w") },
        ClockInfo {
            unit: String::from("us"),
            origin_unix_ms: 1_764_108_000_000,
            origin_iso8601: String::from("2025-11-25T22:00:00Z"),
            utc_offset_minutes: 0,
        },
        HostInfo { os: String::from("macos"), arch: String::from("aarch64") },
    )
}

fn video_track(w: u32, h: u32) -> Track {
    Track {
        source: String::from("frame-tap"),
        device: None,
        codec: String::from("h264"),
        encoder: String::from("VideoToolbox"),
        hardware: true,
        width: w,
        height: h,
        scale: 2.0,
        fps: 10,
        vfr: true,
        bitrate_kbps: 3_000,
        sample_rate: 0,
        channels: 0,
        timescale: 90_000,
        status: TrackStatus::Recording,
        note: None,
        segments: Vec::new(),
    }
}

fn audio_track() -> Track {
    Track {
        source: String::from("microphone"),
        device: None,
        codec: String::from("opus"),
        encoder: String::from("AudioToolbox"),
        hardware: false,
        width: 0,
        height: 0,
        scale: 1.0,
        fps: 0,
        vfr: false,
        bitrate_kbps: 32,
        sample_rate: 48_000,
        channels: 1,
        timescale: 48_000,
        status: TrackStatus::Recording,
        note: None,
        segments: Vec::new(),
    }
}

fn writer(store: &MemStore, transcript: Option<MemLines>) -> Writer {
    Writer::new(
        WriterConfig { segment_us: 300 * SEC, created_unix_s: 1_764_108_000, camera_chunk_us: 5 * SEC, voice_in_camera: false },
        manifest(),
        Box::new(store.clone()),
        transcript.map(|t| Box::new(t) as Box<dyn Write + Send>),
    )
}

fn with_tracks(store: &MemStore) -> Writer {
    let mut w = writer(store, None);
    w.add_track("screen", Kind::Video, video_track(1280, 720)).unwrap();
    w.add_track("audio", Kind::Opus, audio_track()).unwrap();
    w
}

// ---- the tracks ----

#[test]
fn only_the_three_track_folders_are_tracks_and_each_once() {
    let store = MemStore::default();
    let mut w = writer(&store, None);
    assert!(w.add_track("screen", Kind::Video, video_track(1280, 720)).is_ok());
    assert!(w.add_track("screen", Kind::Video, video_track(1280, 720)).unwrap_err().contains("there already"));
    assert!(w.add_track("../x", Kind::Video, video_track(1, 1)).unwrap_err().contains("none of the track folders"));
    assert!(w.add_track("video", Kind::Video, video_track(1, 1)).is_err());
    assert!(w.manifest().tracks.screen.is_some());
    assert!(w.manifest().tracks.camera.is_none());
}

#[test]
fn the_manifest_is_dirty_when_it_changed_and_clean_after_it_was_taken() {
    let store = MemStore::default();
    let mut w = with_tracks(&store);
    assert!(w.take_dirty(), "a new writer has a manifest nobody has written");
    assert!(!w.take_dirty());
    w.diagnostic(5, "info", "hello");
    assert!(w.take_dirty());
}

#[test]
fn a_keyframe_opens_a_segment_that_the_manifest_lists_with_its_size_left_out_when_the_tracks() {
    let store = MemStore::default();
    let mut w = with_tracks(&store);
    let notes = w.video("screen", 2 * SEC, &key(1));
    assert_eq!(notes, vec![Note::SegmentOpened { track: "screen", file: "screen/0001.mp4".into(), start_us: 2 * SEC }]);
    let segs = w.manifest().tracks.screen.as_ref().unwrap().segments.clone();
    assert_eq!(segs.len(), 1);
    assert_eq!((segs[0].start_us, segs[0].complete, segs[0].end_us), (2 * SEC, false, None));
    assert_eq!((segs[0].width, segs[0].height), (0, 0), "1280x720 is the track's size: left out");
    assert!(w.manifest().problems().is_empty(), "{:?}", w.manifest().problems());
    assert!(w.take_dirty());
}

#[test]
fn a_segment_of_another_size_keeps_its_own() {
    let store = MemStore::default();
    let mut w = writer(&store, None);
    w.add_track("screen", Kind::Video, video_track(2880, 1800)).unwrap();
    w.video("screen", 0, &key(1)); // a 1280x720 keyframe in a track said to be 2880x1800
    let seg = &w.manifest().tracks.screen.as_ref().unwrap().segments[0];
    assert_eq!((seg.width, seg.height), (1280, 720));
}

#[test]
fn samples_for_a_track_that_is_not_there_are_ignored() {
    let store = MemStore::default();
    let mut w = writer(&store, None);
    assert!(w.video("camera", 0, &key(1)).is_empty());
    assert!(w.audio(0, opus(1)).is_empty());
    assert!(store.names().is_empty());
}

// ---- pause and stop ----

#[test]
fn a_pause_closes_the_segments_and_the_run_and_a_resume_begins_a_new_run() {
    let store = MemStore::default();
    let mut w = with_tracks(&store);
    for i in 0..30_u64 {
        w.video("screen", i * 100_000, &if i % 20 == 0 { key(i as u8) } else { delta(i as u8) });
        w.audio(i * 100_000, opus(i as u8));
    }
    let notes = w.run_end(3 * SEC, "pause");
    assert_eq!(notes.len(), 2, "both tracks closed a segment: {notes:?}");
    assert!(notes.iter().all(|n| matches!(n, Note::SegmentClosed { end_us: 3_000_000, .. })));
    let m = w.manifest().clone();
    assert_eq!(m.runs.len(), 1);
    assert_eq!((m.runs[0].end_us, m.runs[0].end_reason.as_deref()), (Some(3 * SEC), Some("pause")));
    for name in ["screen/0001.mp4", "audio/0001.mp4"] {
        assert!(scan(&mut Cursor::new(store.file(name))).unwrap().finished, "{name} is finished at the pause");
    }

    w.run_start(60 * SEC);
    w.run_start(61 * SEC); // a second resume while running changes nothing
    w.video("screen", 60 * SEC, &key(1));
    w.audio(60 * SEC, opus(1));
    let m = w.manifest().clone();
    assert_eq!(m.runs.len(), 2);
    assert_eq!((m.runs[1].start_us, m.runs[1].end_us), (60 * SEC, None));
    assert_eq!(m.tracks.screen.as_ref().unwrap().segments.len(), 2, "a new segment after the pause");
    assert_eq!(m.tracks.screen.as_ref().unwrap().segments[1].start_us, 60 * SEC);
    assert!(m.problems().is_empty(), "{:?}", m.problems());
}

/// One track's segment ends while the run goes on (a gap of the screen): its file is finished, the next
/// keyframe opens another file, the other tracks do not notice, and the manifest stays true. A track that
/// is not there, or has no segment open, has nothing to end.
///
/// Mutation: an end that finishes every track; one that does not finish the file (the next keyframe goes
/// on in the same segment).
#[test]
fn one_tracks_segment_ends_while_the_others_go_on_and_the_next_keyframe_opens_another() {
    let store = MemStore::default();
    let mut w = with_tracks(&store);
    for i in 0..10_u64 {
        w.video("screen", i * 100_000, &if i == 0 { key(0) } else { delta(i as u8) });
        w.audio(i * 100_000, opus(i as u8));
    }
    assert!(w.end_segment("camera", SEC).is_empty(), "no such track");
    let notes = w.end_segment("screen", SEC);
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(matches!(notes[0], Note::SegmentClosed { end_us: 1_000_000, .. }));
    assert!(scan(&mut Cursor::new(store.file("screen/0001.mp4"))).unwrap().finished);
    assert!(w.end_segment("screen", 2 * SEC).is_empty(), "nothing is open: nothing to end");
    // A delta picture opens nothing; the next keyframe does.
    w.video("screen", 5 * SEC, &delta(1));
    w.video("screen", 6 * SEC, &key(2));
    w.audio(6 * SEC, opus(9));
    let m = w.manifest().clone();
    let screen = m.tracks.screen.as_ref().unwrap();
    assert_eq!(screen.segments.len(), 2);
    assert_eq!((screen.segments[0].end_us, screen.segments[1].start_us), (Some(SEC), 6 * SEC));
    assert_eq!(m.tracks.audio.as_ref().unwrap().segments.len(), 1, "the voice went on in its one segment");
    assert!(m.problems().is_empty(), "{:?}", m.problems());
}

#[test]
fn stopping_finishes_every_file_and_the_manifest() {
    let store = MemStore::default();
    let mut w = with_tracks(&store);
    for i in 0..10_u64 {
        w.video("screen", i * 100_000, &if i == 0 { key(0) } else { delta(i as u8) });
        w.audio(i * 100_000, opus(i as u8));
    }
    w.track_status("audio", TrackStatus::Recording, None);
    let notes = w.stop(SEC, Some(1_764_108_001_000));
    assert_eq!(notes.len(), 2);
    let m = w.manifest().clone();
    assert_eq!(m.state, SessionState::Complete);
    assert_eq!(m.ended_unix_ms, Some(1_764_108_001_000));
    assert_eq!((m.runs[0].end_us, m.runs[0].end_reason.as_deref()), (Some(SEC), Some("stop")));
    assert_eq!(m.tracks.screen.as_ref().unwrap().status, TrackStatus::Stopped);
    assert_eq!(m.tracks.audio.as_ref().unwrap().status, TrackStatus::Stopped);
    assert!(m.tracks.screen.as_ref().unwrap().segments.iter().all(|s| s.complete));
    assert!(m.problems().is_empty(), "{:?}", m.problems());
    for name in ["screen/0001.mp4", "audio/0001.mp4"] {
        assert!(scan(&mut Cursor::new(store.file(name))).unwrap().finished, "{name}");
    }
    // The manifest survives its own text.
    assert_eq!(Manifest::parse(&m.to_json()), Ok(m));
}

#[test]
fn a_track_that_was_denied_keeps_that_through_the_stop() {
    let store = MemStore::default();
    let mut w = with_tracks(&store);
    w.track_status("audio", TrackStatus::Denied, Some(String::from("the microphone was refused")));
    w.stop(SEC, Some(1));
    let a = w.manifest().tracks.audio.clone().unwrap();
    assert_eq!(a.status, TrackStatus::Denied);
    assert_eq!(a.note.as_deref(), Some("the microphone was refused"));
}

// ---- failure ----

#[test]
fn a_file_that_fails_marks_its_track_failed_in_words_once_and_the_others_go_on() {
    let store = MemStore::default();
    store.0.lock().unwrap().fail_prefix = Some((String::from("screen/"), 1_500));
    let mut w = with_tracks(&store);
    let mut failures = Vec::new();
    for i in 0..300_u64 {
        let t = i * 100_000;
        for n in w.video("screen", t, &if i % 20 == 0 { key(i as u8) } else { delta(i as u8) }) {
            if let Note::TrackFailed { track, why } = n {
                failures.push((track, why));
            }
        }
        w.audio(t, opus(i as u8));
    }
    assert_eq!(failures.len(), 1, "said once: {failures:?}");
    assert_eq!(failures[0].0, "screen");
    assert!(failures[0].1.contains("No space left on device"));
    let m = w.manifest().clone();
    let screen = m.tracks.screen.as_ref().unwrap();
    assert_eq!(screen.status, TrackStatus::Failed);
    assert!(screen.note.as_deref().unwrap().contains("No space left on device"));
    assert_eq!(m.tracks.audio.as_ref().unwrap().status, TrackStatus::Recording, "the sound goes on");
    assert!(w.track_bytes("audio") > 1_500);
}

// ---- the transcript ----

#[test]
fn transcript_lines_are_appended_and_counted_in_the_manifest() {
    let store = MemStore::default();
    let lines = MemLines::default();
    let mut w = writer(&store, Some(lines.clone()));
    let line = TranscriptLine {
        t0_us: 1_000_000,
        t1_us: 2_000_000,
        text: String::from("this leaks"),
        lang: String::from("en"),
        engine: String::from("mock"),
        conv: None,
        words: Vec::new(),
    };
    w.transcript(&line);
    w.transcript(&line);
    let text = String::from_utf8(lines.0.lock().unwrap().clone()).unwrap();
    assert_eq!(text.lines().count(), 2);
    let info = w.manifest().transcript.clone().unwrap();
    assert_eq!((info.file.as_str(), info.count, info.bytes), ("transcript.jsonl", 2, text.len() as u64));
    let (parsed, problems) = crate::recorder_manifest::parse_transcript(&text);
    assert_eq!((parsed.len(), problems.len()), (2, 0));
}

// ---- the thread ----

fn thread_rig() -> (MemStore, MemManifest, Handle, crate::recorder_events::EventSink, MockClock) {
    let store = MemStore::default();
    let sink = MemManifest::default();
    let clock = MockClock::at(0);
    let (events, rx) = event_channel(Arc::new(clock.clone()), Caps::default());
    let log = crate::recorder_events::EventLog::new(Vec::<u8>::new(), rx, 0);
    let mut w = writer(&store, None);
    w.add_track("screen", Kind::Video, video_track(1280, 720)).unwrap();
    let handle = spawn(w, Box::new(sink.clone()), Some(log));
    (store, sink, handle, events, clock)
}

fn wait_for(what: &str, mut cond: impl FnMut() -> bool) {
    for _ in 0..500 {
        if cond() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("gave up waiting for {what}");
}

#[test]
fn the_thread_writes_what_it_is_sent_and_finishes_on_a_stop() {
    let (store, sink, mut handle, events, clock) = thread_rig();
    assert!(handle.send(Cmd::AddTrack { dir: "audio", kind: Kind::Opus, track: Box::new(audio_track()) }));
    for i in 0..30_u64 {
        handle.send(Cmd::Video { dir: "screen", pts_us: i * 100_000, annexb: if i % 20 == 0 { key(i as u8) } else { delta(i as u8) } });
        handle.send(Cmd::Audio { pts_us: i * 100_000, packet: opus(i as u8) });
    }
    clock.advance_ms(1_500);
    events.event("nav.file", r#"{"file":"a.rs"}"#).unwrap();
    wait_for("the segments", || handle.status().screen_bytes > 400 && handle.status().audio_bytes > 400);
    wait_for("the event line", || handle.status().event_lines >= 1);
    handle.send(Cmd::Stop { t_us: 3 * SEC, ended_unix_ms: 42 });
    assert!(handle.join(std::time::Duration::from_secs(5)), "the thread finished");
    let st = handle.status();
    assert!(st.done);
    assert!(st.failures.is_empty() && st.manifest_error.is_none(), "{st:?}");
    assert_eq!(st.media_bytes, st.screen_bytes + st.audio_bytes);
    let last = sink.writes.lock().unwrap().last().cloned().unwrap();
    let m = Manifest::parse(&last).expect("the last manifest is a sound manifest");
    assert_eq!(m.state, SessionState::Complete);
    assert_eq!(m.ended_unix_ms, Some(42));
    assert_eq!(m.events.as_ref().map(|e| e.count), Some(1));
    assert!(scan(&mut Cursor::new(store.file("screen/0001.mp4"))).unwrap().finished);
}

#[test]
fn a_manifest_that_cannot_be_written_is_said_and_the_media_goes_on() {
    let (store, sink, mut handle, _events, _clock) = thread_rig();
    *sink.fail.lock().unwrap() = true;
    handle.send(Cmd::Video { dir: "screen", pts_us: 0, annexb: key(1) });
    wait_for("the error", || handle.status().manifest_error.as_deref().is_some_and(|e| e.contains("read-only")));
    handle.send(Cmd::Video { dir: "screen", pts_us: 100_000, annexb: delta(1) });
    handle.send(Cmd::Stop { t_us: SEC, ended_unix_ms: 1 });
    assert!(handle.join(std::time::Duration::from_secs(5)));
    assert!(scan(&mut Cursor::new(store.file("screen/0001.mp4"))).unwrap().finished, "the file was finished all the same");
}

#[test]
fn a_dropped_handle_finishes_the_files_where_they_were() {
    let (store, sink, handle, _events, _clock) = thread_rig();
    for i in 0..30_u64 {
        handle.send(Cmd::Video { dir: "screen", pts_us: i * 100_000, annexb: if i % 20 == 0 { key(i as u8) } else { delta(i as u8) } });
    }
    wait_for("the segment", || handle.status().screen_bytes > 400);
    drop(handle); // no stop: the app was torn down
    let s = scan(&mut Cursor::new(store.file("screen/0001.mp4"))).unwrap();
    assert!(s.finished && !s.truncated, "{s:?}");
    let last = sink.writes.lock().unwrap().last().cloned().unwrap();
    let m = Manifest::parse(&last).unwrap();
    assert_eq!(m.state, SessionState::Complete);
    assert!(m.diagnostics.iter().any(|d| d.text.contains("dropped without a stop")), "{:?}", m.diagnostics);
}

#[test]
fn a_segment_in_a_listing_is_what_the_writer_has() {
    // The manifest's segments are the writer's own: nothing is kept twice to disagree.
    let store = MemStore::default();
    let mut w = with_tracks(&store);
    w.video("screen", 0, &key(1));
    w.video("screen", 100_000, &delta(2));
    let listed: Vec<Segment> = w.manifest().tracks.screen.as_ref().unwrap().segments.clone();
    assert_eq!(listed[0].samples, 2);
    assert_eq!(listed[0].bytes, store.file("screen/0001.mp4").len() as u64);
}

// ---- the camera's files that carry the voice ----

fn with_camera_and_voice(store: &MemStore, voice_in_camera: bool) -> Writer {
    let mut w = Writer::new(
        WriterConfig { segment_us: 300 * SEC, created_unix_s: 1_764_108_000, camera_chunk_us: 5 * SEC, voice_in_camera },
        manifest(),
        Box::new(store.clone()),
        None,
    );
    w.add_track("camera", Kind::Video, video_track(1280, 720)).unwrap();
    w.add_track("audio", Kind::Opus, audio_track()).unwrap();
    w
}

/// How many tracks the `moov` of a file describes.
fn tracks_in(file: &[u8]) -> usize {
    let boxes = |data: &[u8]| -> Vec<([u8; 4], Vec<u8>)> {
        let mut out = Vec::new();
        let mut at = 0;
        while at + 8 <= data.len() {
            let size = u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]) as usize;
            if size < 8 || at + size > data.len() {
                break;
            }
            out.push(([data[at + 4], data[at + 5], data[at + 6], data[at + 7]], data[at + 8..at + size].to_vec()));
            at += size;
        }
        out
    };
    let moov = boxes(file).into_iter().find(|(k, _)| k == b"moov").map(|(_, b)| b).unwrap_or_default();
    boxes(&moov).iter().filter(|(k, _)| k == b"trak").count()
}

/// How many samples the voice's (track 2) runs of a file hold in all.
fn voice_samples_in(file: &[u8]) -> u32 {
    let boxes = |data: &[u8]| -> Vec<([u8; 4], Vec<u8>)> {
        let mut out = Vec::new();
        let mut at = 0;
        while at + 8 <= data.len() {
            let size = u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]) as usize;
            if size < 8 || at + size > data.len() {
                break;
            }
            out.push(([data[at + 4], data[at + 5], data[at + 6], data[at + 7]], data[at + 8..at + size].to_vec()));
            at += size;
        }
        out
    };
    let mut total = 0;
    for (kind, moof) in boxes(file) {
        if &kind != b"moof" {
            continue;
        }
        for (k, traf) in boxes(&moof) {
            if &k != b"traf" {
                continue;
            }
            let parts = boxes(&traf);
            let tfhd = parts.iter().find(|(k, _)| k == b"tfhd").map(|(_, b)| b.clone()).unwrap_or_default();
            let trun = parts.iter().find(|(k, _)| k == b"trun").map(|(_, b)| b.clone()).unwrap_or_default();
            if tfhd.len() >= 8 && u32::from_be_bytes([tfhd[4], tfhd[5], tfhd[6], tfhd[7]]) == 2 && trun.len() >= 8 {
                total += u32::from_be_bytes([trun[4], trun[5], trun[6], trun[7]]);
            }
        }
    }
    total
}

/// The voice goes into the camera's files as well as into its own track's, and the camera's picture is what the
/// manifest and the scanner see of the camera track: the voice's own files and the manifest's count of
/// them are as they were.
///
/// Mutation: a voice that goes into the camera's files only; one that does not reach them.
#[test]
fn the_voice_goes_into_the_cameras_files_and_into_its_own() {
    let store = MemStore::default();
    let mut w = with_camera_and_voice(&store, true);
    w.video("camera", SEC, &key(1));
    for i in 0..100_u64 {
        w.audio(SEC + i * 20_000, opus(i as u8));
    }
    for i in 1..30_u64 {
        w.video("camera", SEC + i * 33_333, &delta(i as u8));
    }
    w.stop(4 * SEC, None);
    let camera = store.file("camera/0001.mp4");
    let voice = store.file("audio/0001.mp4");
    assert_eq!(tracks_in(&camera), 2, "the camera's file carries a voice track");
    assert_eq!(voice_samples_in(&camera), 100, "and every packet of the voice is in it");
    assert_eq!(tracks_in(&voice), 1, "and the voice has its own, whole");
    let s = scan(&mut Cursor::new(&voice)).unwrap();
    assert_eq!(s.samples, 100, "every packet is in the voice's own file");
    let c = scan(&mut Cursor::new(&camera)).unwrap();
    assert_eq!(c.samples, 30, "the pictures are what the camera track counts");
    let m = w.manifest();
    assert_eq!(m.tracks.camera.as_ref().unwrap().segments[0].samples, 30);
    assert_eq!(m.tracks.audio.as_ref().unwrap().segments[0].samples, 100);
}

/// A writer told not to carry the voice in the camera's files writes files of the picture alone, and the voice's
/// own track is the same.
///
/// Mutation: the setting ignored.
#[test]
fn a_writer_that_is_not_to_carry_the_voice_writes_the_cameras_files_without_it() {
    let store = MemStore::default();
    let mut w = with_camera_and_voice(&store, false);
    w.video("camera", SEC, &key(1));
    for i in 0..50_u64 {
        w.audio(SEC + i * 20_000, opus(i as u8));
    }
    w.stop(3 * SEC, None);
    assert_eq!(tracks_in(&store.file("camera/0001.mp4")), 1);
    assert_eq!(scan(&mut Cursor::new(&store.file("audio/0001.mp4"))).unwrap().samples, 50);
}

/// The voice before the camera's first picture has no file of the camera's to be in and is in the voice's own,
/// and a camera that is not there takes nothing and breaks nothing.
///
/// Mutation: a voice that fails the audio track when there is no camera.
#[test]
fn the_voice_before_the_camera_began_is_in_its_own_track_only_and_no_camera_is_no_problem() {
    let store = MemStore::default();
    let mut w = with_camera_and_voice(&store, true);
    for i in 0..10_u64 {
        w.audio(i * 20_000, opus(i as u8));
    }
    w.video("camera", SEC, &key(1));
    w.stop(2 * SEC, None);
    assert_eq!(scan(&mut Cursor::new(&store.file("audio/0001.mp4"))).unwrap().samples, 10);
    assert!(w.manifest().problems().is_empty(), "{:?}", w.manifest().problems());

    // No camera track at all: the voice goes to its own and nothing is written for a camera.
    let store = MemStore::default();
    let mut w = Writer::new(
        WriterConfig { segment_us: 300 * SEC, created_unix_s: 1_764_108_000, camera_chunk_us: 5 * SEC, voice_in_camera: true },
        manifest(),
        Box::new(store.clone()),
        None,
    );
    w.add_track("audio", Kind::Opus, audio_track()).unwrap();
    for i in 0..10_u64 {
        assert!(w.audio(i * 20_000, opus(i as u8)).iter().all(|n| !matches!(n, Note::TrackFailed { .. })));
    }
    w.stop(SEC, None);
    assert_eq!(store.names(), vec!["audio/0001.mp4"]);
}
