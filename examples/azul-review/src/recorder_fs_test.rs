//! The session folders on disk: files that are never overwritten, a manifest that is always whole,
//! what a crash leaves put right, the list and the delete. Real files in a temporary folder.

use std::{
    io::Cursor,
    sync::atomic::{AtomicU32, Ordering},
};

use super::*;
use crate::{
    recorder_manifest::{AppInfo, ClockInfo, HostInfo, Track, TrackStatus, Workspace},
    recorder_mp4::{FragmentWriter, Limits, OpusConfig, TrackConfig},
    recorder_track::Kind,
    recorder_writer::{Writer, WriterConfig},
};

/// A temporary folder that goes away with the test.
struct Tmp(PathBuf);

impl Tmp {
    fn new(what: &str) -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!("azreview-rec-{what}-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
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

const SEC: u64 = 1_000_000;

fn track(video: bool) -> Track {
    Track {
        source: String::from("mock"),
        device: None,
        codec: String::from(if video { "h264" } else { "opus" }),
        encoder: String::from("mock"),
        hardware: false,
        width: if video { 1280 } else { 0 },
        height: if video { 720 } else { 0 },
        scale: 1.0,
        fps: 10,
        vfr: video,
        bitrate_kbps: 100,
        sample_rate: if video { 0 } else { 48_000 },
        channels: if video { 0 } else { 1 },
        timescale: if video { 90_000 } else { 48_000 },
        status: TrackStatus::Recording,
        note: None,
        segments: Vec::new(),
    }
}

/// A session on disk with a screen and an audio track and `seconds` of media written through the
/// real writer and files; the writer is returned so the test decides whether it ever finishes.
fn session(recordings: &Path, seconds: u64) -> (PathBuf, Writer) {
    let (name, dir) = new_session_dir(recordings, "2025-11-25_22-00-00").unwrap();
    let m = Manifest::begin(
        AppInfo { name: String::from("AzReview"), version: String::from("0.1.0") },
        &name,
        Workspace { root: String::from("/w"), name: String::from("w") },
        ClockInfo { unit: String::from("us"), origin_unix_ms: 1_764_108_000_000, origin_iso8601: String::from("2025-11-25T22:00:00Z"), utc_offset_minutes: 0 },
        HostInfo { os: String::from("macos"), arch: String::from("aarch64") },
    );
    let transcript = open_lines(&dir.join(TRANSCRIPT_FILE)).unwrap();
    let mut w = Writer::new(
        WriterConfig { segment_us: 300 * SEC, created_unix_s: 1_764_108_000 },
        m,
        Box::new(DiskStore::new(dir.clone())),
        Some(Box::new(transcript)),
    );
    w.add_track("screen", Kind::Video, track(true)).unwrap();
    w.add_track("audio", Kind::Opus, track(false)).unwrap();
    for i in 0..seconds * 10 {
        let t = i * SEC / 10;
        w.video("screen", t, &if i % 20 == 0 { key(i as u8) } else { delta(i as u8) });
        w.audio(t, vec![0xf8, i as u8, 1, 2, 3]);
    }
    ManifestFile::new(&dir).write(&w.manifest().to_json()).unwrap();
    (dir, w)
}

fn scan_file(path: &Path) -> crate::recorder_mp4::Scan {
    scan(&mut File::open(path).unwrap()).unwrap()
}

// ---- writing ----

#[test]
fn the_store_makes_new_files_with_their_folders_and_never_overwrites() {
    let tmp = Tmp::new("store");
    let mut store = DiskStore::new(tmp.0.clone());
    let mut f = store.create("screen/0001.mp4").unwrap();
    f.append(b"hello").unwrap();
    f.append(b" world").unwrap();
    f.sync().unwrap();
    assert_eq!(f.len(), 11);
    assert_eq!(fs::read(tmp.0.join("screen/0001.mp4")).unwrap(), b"hello world");
    let err = store.create("screen/0001.mp4").err().unwrap();
    assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read(tmp.0.join("screen/0001.mp4")).unwrap(), b"hello world", "untouched");
    for bad in ["../x.mp4", "/abs.mp4", "a/../../b.mp4", ""] {
        assert_eq!(store.create(bad).err().unwrap().kind(), io::ErrorKind::InvalidInput, "{bad:?}");
    }
}

#[test]
fn the_manifest_is_replaced_whole_and_leaves_no_temporary_file() {
    let tmp = Tmp::new("atomic");
    let path = tmp.0.join(MANIFEST_FILE);
    write_atomic(&path, b"first").unwrap();
    write_atomic(&path, b"second, longer").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"second, longer");
    let names: Vec<String> = fs::read_dir(&tmp.0).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, vec!["manifest.json"]);
    let mut sink = ManifestFile::new(&tmp.0);
    sink.write("{}").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"{}");
}

#[test]
fn two_sessions_of_the_same_second_get_two_folders() {
    let tmp = Tmp::new("names");
    let a = new_session_dir(&tmp.0, "2025-11-25_22-00-00").unwrap();
    let b = new_session_dir(&tmp.0, "2025-11-25_22-00-00").unwrap();
    let c = new_session_dir(&tmp.0, "2025-11-25_22-00-00").unwrap();
    assert_eq!((a.0.as_str(), b.0.as_str(), c.0.as_str()), ("2025-11-25_22-00-00", "2025-11-25_22-00-00-2", "2025-11-25_22-00-00-3"));
    assert!(a.1.is_dir() && b.1.is_dir() && c.1.is_dir());
}

// ---- recovery ----

#[test]
fn a_killed_session_is_cut_back_to_its_last_whole_fragment_and_says_so() {
    let tmp = Tmp::new("crash");
    let (dir, writer) = session(&tmp.0, 5);
    std::mem::forget(writer); // the app was killed: nothing is finished, nothing more is written
    let screen = dir.join("screen/0001.mp4");
    let whole = fs::metadata(&screen).unwrap().len();
    // The kill tore the last fragment on its way to the disk.
    let f = OpenOptions::new().write(true).open(&screen).unwrap();
    f.set_len(whole - 7).unwrap();
    drop(f);

    let r = recover_session(&dir).unwrap();
    assert_eq!((r.cut, r.removed), (1, 0));
    assert!(r.lost_bytes > 0);

    let m = Manifest::parse(&fs::read_to_string(dir.join(MANIFEST_FILE)).unwrap()).unwrap();
    assert_eq!(m.state, SessionState::Recovered);
    let seg = &m.tracks.screen.as_ref().unwrap().segments[0];
    assert!(seg.complete && seg.recovered);
    let s = scan_file(&screen);
    assert!(!s.truncated, "the file is whole now");
    assert_eq!(seg.bytes, s.whole_end);
    assert_eq!(seg.samples, s.samples);
    assert_eq!(seg.end_us, Some(s.duration_us()), "from its start at 0");
    assert_eq!(s.fragments, 1, "the torn fragment went, the one before stayed: 0 to 2 s");
    assert_eq!(s.duration_us(), 2 * SEC);
    // The audio file had only whole fragments: nothing was cut, but it counts as recovered.
    let a = &m.tracks.audio.as_ref().unwrap().segments[0];
    assert!(a.complete && a.recovered && a.samples > 0);
    // The run ended at the last time that is certain; the tracks are stopped; the diagnostics say what happened.
    assert_eq!(m.runs[0].end_reason.as_deref(), Some("crash"));
    assert!(m.runs[0].end_us.unwrap() >= 2 * SEC);
    assert_eq!(m.tracks.screen.as_ref().unwrap().status, TrackStatus::Stopped);
    assert!(m.diagnostics.iter().any(|d| d.text.contains("the app ended without stopping this recording")));
    assert!(m.problems().is_empty(), "{:?}", m.problems());
}

#[test]
fn recovering_twice_changes_nothing_the_second_time() {
    let tmp = Tmp::new("twice");
    let (dir, writer) = session(&tmp.0, 5);
    std::mem::forget(writer);
    recover_session(&dir).unwrap();
    let first = fs::read(dir.join(MANIFEST_FILE)).unwrap();
    let again = recover_session(&dir).unwrap();
    assert_eq!((again.cut, again.removed, again.whole), (0, 0, 0));
    assert_eq!(fs::read(dir.join(MANIFEST_FILE)).unwrap(), first);
}

#[test]
fn a_session_that_was_stopped_properly_is_left_alone() {
    let tmp = Tmp::new("complete");
    let (dir, mut writer) = session(&tmp.0, 3);
    writer.stop(3 * SEC, Some(1));
    ManifestFile::new(&dir).write(&writer.manifest().to_json()).unwrap();
    let before = fs::read(dir.join(MANIFEST_FILE)).unwrap();
    let r = recover_session(&dir).unwrap();
    assert_eq!(r, Recovery { session: String::from("2025-11-25_22-00-00"), ..Recovery::default() });
    assert_eq!(fs::read(dir.join(MANIFEST_FILE)).unwrap(), before);
}

#[test]
fn a_segment_opened_after_the_last_manifest_write_and_one_that_is_gone_are_dealt_with() {
    let tmp = Tmp::new("unlisted");
    let (dir, writer) = session(&tmp.0, 3);
    std::mem::forget(writer);
    // A second screen segment the manifest never heard of: just its head.
    let (_w, head) = FragmentWriter::new(TrackConfig::Opus(OpusConfig::mono_48k()), 0, Limits::default());
    fs::write(dir.join("screen/0002.mp4"), head).unwrap();
    // The audio segment is gone.
    fs::remove_file(dir.join("audio/0001.mp4")).unwrap();
    let r = recover_session(&dir).unwrap();
    assert_eq!(r.removed, 2);
    assert!(!dir.join("screen/0002.mp4").exists(), "the unlisted file was removed");
    let m = Manifest::parse(&fs::read_to_string(dir.join(MANIFEST_FILE)).unwrap()).unwrap();
    assert!(m.tracks.audio.as_ref().unwrap().segments.is_empty());
    assert!(m.diagnostics.iter().any(|d| d.text.contains("audio/0001.mp4 was missing at recovery")));
    assert_eq!(m.tracks.screen.as_ref().unwrap().segments.len(), 1);
}

#[test]
fn the_line_files_lose_a_torn_last_line_and_the_manifest_counts_what_is_left() {
    let tmp = Tmp::new("lines");
    let (dir, writer) = session(&tmp.0, 2);
    std::mem::forget(writer);
    fs::write(dir.join(EVENTS_FILE), "{\"t\":1,\"k\":\"marker\"}\n{\"t\":2,\"k\":\"marker\"}\n{\"t\":3,\"k\":\"mar").unwrap();
    fs::write(dir.join(TRANSCRIPT_FILE), "{\"a\":1}\n{\"a\":2}\n").unwrap();
    let r = recover_session(&dir).unwrap();
    assert!(r.lines_cut);
    assert_eq!(fs::read_to_string(dir.join(EVENTS_FILE)).unwrap(), "{\"t\":1,\"k\":\"marker\"}\n{\"t\":2,\"k\":\"marker\"}\n");
    let m = Manifest::parse(&fs::read_to_string(dir.join(MANIFEST_FILE)).unwrap()).unwrap();
    let e = m.events.unwrap();
    assert_eq!((e.count, e.bytes), (2, 42));
    assert_eq!(m.transcript.unwrap().count, 2);
}

#[test]
fn a_manifest_that_cannot_be_read_is_reported_not_guessed() {
    let tmp = Tmp::new("badmanifest");
    fs::create_dir_all(tmp.0.join("2025-01-01_00-00-00")).unwrap();
    fs::write(tmp.0.join("2025-01-01_00-00-00").join(MANIFEST_FILE), "this is not json").unwrap();
    let (done, failed) = recover_all(&tmp.0);
    assert!(done.is_empty());
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].0, "2025-01-01_00-00-00");
    assert!(failed[0].1.contains("the manifest is not valid"), "{}", failed[0].1);
}

#[test]
fn recover_all_goes_through_every_session_and_names_the_ones_it_touched() {
    let tmp = Tmp::new("all");
    let (_a, wa) = session(&tmp.0, 4);
    std::mem::forget(wa);
    let (b_dir, mut wb) = session(&tmp.0, 2);
    wb.stop(2 * SEC, Some(1));
    ManifestFile::new(&b_dir).write(&wb.manifest().to_json()).unwrap();
    fs::create_dir_all(tmp.0.join("not-a-recording")).unwrap();
    let (done, failed) = recover_all(&tmp.0);
    assert!(failed.is_empty(), "{failed:?}");
    assert_eq!(done.len(), 1, "only the session that was left recording: {done:?}");
    assert_eq!(done[0].session, "2025-11-25_22-00-00");
}

// ---- the list and the delete ----

#[test]
fn the_list_has_the_newest_first_with_what_each_holds() {
    let tmp = Tmp::new("list");
    let (_a, mut wa) = session(&tmp.0, 3);
    wa.stop(3 * SEC, Some(1));
    ManifestFile::new(&tmp.0.join("2025-11-25_22-00-00")).write(&wa.manifest().to_json()).unwrap();
    let (b, wb) = session(&tmp.0, 2); // 2025-11-25_22-00-00-2, still recording
    std::mem::forget(wb);
    fs::create_dir_all(tmp.0.join("empty-folder")).unwrap();
    fs::create_dir_all(tmp.0.join("broken")).unwrap();
    fs::write(tmp.0.join("broken").join(MANIFEST_FILE), "{").unwrap();

    let list = list_recordings(&tmp.0);
    let names: Vec<&str> = list.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, vec!["broken", "2025-11-25_22-00-00-2", "2025-11-25_22-00-00"], "newest first by name; a folder with no manifest is no recording");
    let complete = &list[2];
    assert_eq!(complete.state, Some(SessionState::Complete));
    assert_eq!(complete.tracks, vec!["screen", "audio"]);
    assert!(complete.bytes > 1_000);
    assert!(complete.problem.is_none());
    assert_eq!(list[1].state, Some(SessionState::Recording));
    assert_eq!(list[1].name, b.file_name().unwrap().to_string_lossy());
    assert!(list[0].problem.as_deref().is_some_and(|p| p.contains("not valid")), "{:?}", list[0].problem);
}

#[test]
fn a_recording_is_deleted_with_its_size_and_nothing_else_is() {
    let tmp = Tmp::new("delete");
    let (dir, mut w) = session(&tmp.0, 3);
    w.stop(3 * SEC, Some(1));
    ManifestFile::new(&dir).write(&w.manifest().to_json()).unwrap();
    let before = dir_bytes(&dir, 4);
    assert!(before > 1_000);
    fs::create_dir_all(tmp.0.join("my-other-folder")).unwrap();
    fs::write(tmp.0.join("my-other-folder/precious.txt"), "keep").unwrap();

    for bad in ["../outside", "a/b", "", "my-other-folder"] {
        assert!(delete_recording(&tmp.0, bad).is_err(), "{bad:?} must be refused");
    }
    assert!(tmp.0.join("my-other-folder/precious.txt").exists());

    let name = dir.file_name().unwrap().to_string_lossy().into_owned();
    assert_eq!(delete_recording(&tmp.0, &name).unwrap(), before);
    assert!(!dir.exists());
}

#[cfg(unix)]
#[test]
fn a_link_is_never_followed_by_a_delete() {
    let tmp = Tmp::new("link");
    let outside = Tmp::new("outside");
    fs::write(outside.0.join(MANIFEST_FILE), "{}").unwrap();
    fs::write(outside.0.join("precious.txt"), "keep").unwrap();
    std::os::unix::fs::symlink(&outside.0, tmp.0.join("link")).unwrap();
    assert!(delete_recording(&tmp.0, "link").is_err());
    assert!(outside.0.join("precious.txt").exists());
}

#[test]
fn a_scan_of_what_the_writer_wrote_to_disk_is_the_same_as_in_memory() {
    let tmp = Tmp::new("scan");
    let (dir, mut w) = session(&tmp.0, 5);
    w.stop(5 * SEC, Some(1));
    let s = scan(&mut Cursor::new(fs::read(dir.join("screen/0001.mp4")).unwrap())).unwrap();
    assert!(s.finished && !s.truncated);
    assert_eq!(s.samples, 50);
    assert_eq!(track_folders(), ["screen", "camera", "audio"]);
}
