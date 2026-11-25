//! The recorder end to end on stand-in encoders, a clock the test moves, a disk whose free space
//! the test sets, and a real folder: what comes in at the top is in the files at the bottom, and
//! the recorder looks after the disk, the idle time and the length of the session by itself.

use std::{
    fs::{self, File},
    sync::{
        atomic::{AtomicU32, AtomicU64, Ordering},
        Arc,
    },
};

use super::*;
use crate::{
    recorder_clock::MockClock,
    recorder_codec::{MockFactory, PixelFormat, VideoScript},
    recorder_manifest::{SessionState, Word},
    recorder_mp4::scan,
    recorder_screen::{Damage, Rect},
    recorder_settings::GB,
};

// ---- the rig ----

struct Tmp(PathBuf);

impl Tmp {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!("azreview-recorder-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
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

struct Rig {
    rec: Recorder,
    clock: MockClock,
    free: Arc<AtomicU64>,
    tmp: Tmp,
}

fn quiet() -> RecorderSettings {
    // No idle pause and a long session unless a test is about them.
    RecorderSettings { idle_pause_us: None, ..RecorderSettings::default() }
}

fn rig_with(settings: RecorderSettings, factory: MockFactory) -> Rig {
    let clock = MockClock::at(0);
    let free = Arc::new(AtomicU64::new(500 * GB));
    let (c, f) = (clock.clone(), Arc::clone(&free));
    let deps = Deps {
        unix_ms: Box::new(|| 1_764_108_000_000),
        make_clock: Box::new(move |_| Arc::new(c.clone()) as Arc<dyn Clock>),
        free_bytes: Box::new(move |_| Some(f.load(Ordering::SeqCst))),
        factory: Box::new(factory),
    };
    Rig { rec: Recorder::new(settings, deps), clock, free, tmp: Tmp::new() }
}

fn rig() -> Rig {
    rig_with(quiet(), MockFactory::default())
}

fn request(dir: &Path) -> StartRequest {
    StartRequest {
        recordings_dir: dir.to_path_buf(),
        app: AppInfo { name: String::from("AzReview"), version: String::from("0.1.0") },
        workspace: Workspace { root: String::from("/w"), name: String::from("w") },
        host: HostInfo { os: String::from("macos"), arch: String::from("aarch64") },
        utc_offset_minutes: 60,
        camera: Perm::Granted,
        microphone: Perm::Granted,
        camera_device: Some(String::from("FaceTime HD Camera")),
        microphone_device: Some(String::from("MacBook Air Microphone")),
    }
}

impl Rig {
    fn start(&mut self) -> Started {
        let req = request(&self.tmp.0);
        self.rec.start(req).unwrap()
    }

    fn advance(&mut self, ms: u64) {
        self.clock.advance_ms(ms);
    }

    /// Moves the clock `ms` on in steps of `step` ms, ticking at each.
    fn run(&mut self, ms: u64, step: u64) -> Vec<Notice> {
        let mut notices = Vec::new();
        for _ in 0..ms / step {
            self.clock.advance_ms(step);
            notices.extend(self.rec.tick());
        }
        notices
    }

    fn manifest(&self, started: &Started) -> Manifest {
        Manifest::parse(&fs::read_to_string(started.dir.join("manifest.json")).unwrap()).unwrap()
    }
}

/// A window picture of `w` x `h`, every byte `fill`.
fn window(w: u32, h: u32, fill: u8) -> Vec<u8> {
    vec![fill; w as usize * h as usize * 4]
}

fn tap(rec: &mut Recorder, w: u32, h: u32, fill: u8, damage: Damage<'_>) -> Option<Tapped> {
    let px = window(w, h, fill);
    rec.tap(&TapFrame { width: w, height: h, stride: w as usize * 4, scale: 2.0, damage, pixels: &px })
}

fn voice(rec: &mut Recorder, samples: usize, level: f32) {
    rec.audio_chunk(&vec![level; samples], 48_000, 1);
}

fn camera(rec: &mut Recorder) {
    rec.camera_frame(RawFrame { width: 64, height: 48, format: PixelFormat::Nv12, bytes: vec![0; 64 * 48 * 3 / 2] });
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

fn scan_file(path: &Path) -> crate::recorder_mp4::Scan {
    scan(&mut File::open(path).unwrap()).unwrap()
}

// ---- starting and stopping ----

#[test]
fn a_session_starts_in_a_folder_of_its_own_and_stops_complete() {
    let mut r = rig();
    let started = r.start();
    assert_eq!(started.name, "2025-11-25_22-00-00");
    assert!(started.dir.join("manifest.json").exists() || r.rec.writer_status().is_some());
    assert_eq!(r.rec.state(), State::Recording);
    assert!(r.rec.is_active());
    assert_eq!(r.rec.session_dir(), Some(started.dir.as_path()));
    r.advance(1_500);
    let done = r.rec.stop("user").unwrap();
    assert!(done.clean);
    assert_eq!((done.name.as_str(), done.recorded_us), ("2025-11-25_22-00-00", 1_500_000));
    assert_eq!(r.rec.state(), State::Idle);
    assert!(r.rec.stop("again").is_none());

    let m = r.manifest(&started);
    assert_eq!(m.state, SessionState::Complete);
    assert_eq!(m.clock.origin_unix_ms, 1_764_108_000_000);
    assert_eq!(m.clock.origin_iso8601, "2025-11-25T22:00:00Z");
    assert_eq!(m.clock.utc_offset_minutes, 60);
    assert_eq!(m.workspace.root, "/w");
    assert_eq!(m.runs.len(), 1);
    assert_eq!((m.runs[0].start_us, m.runs[0].end_us, m.runs[0].end_reason.as_deref()), (0, Some(1_500_000), Some("stop")));
    assert!(m.problems().is_empty(), "{:?}", m.problems());
    let events = fs::read_to_string(started.dir.join("events.jsonl")).unwrap();
    let kinds: Vec<&str> = events.lines().map(|l| l.split("\"k\":\"").nth(1).unwrap().split('"').next().unwrap()).collect();
    assert_eq!(kinds, vec!["session.start", "session.stop"]);
}

#[test]
fn a_start_is_refused_in_words_when_it_cannot_be() {
    let mut r = rig();
    r.start();
    assert_eq!(r.rec.start(request(&r.tmp.0)).unwrap_err(), StartError::Busy);
    r.rec.stop("user");

    r.free.store(9 * GB, Ordering::SeqCst);
    let err = r.rec.start(request(&r.tmp.0)).unwrap_err();
    assert_eq!(err, StartError::DiskFull { free: 9 * GB, min_free: 10 * GB });
    assert!(err.to_string().contains("under the 10000000000 the recorder keeps free"));
    r.free.store(500 * GB, Ordering::SeqCst);

    let off = RecorderSettings { screen: false, camera: false, microphone: false, ..quiet() };
    let mut none = rig_with(off, MockFactory::default());
    assert_eq!(none.rec.start(request(&none.tmp.0)).unwrap_err(), StartError::NothingToRecord);

    // The recordings folder is a file: the folder of the session cannot be made.
    let mut blocked = rig();
    let file = blocked.tmp.0.join("not-a-folder");
    fs::write(&file, "x").unwrap();
    let err = blocked.rec.start(request(&file)).unwrap_err();
    assert!(matches!(err, StartError::Io(_)), "{err}");
}

// ---- the screen ----

#[test]
fn a_changed_window_is_encoded_with_its_settle_repeats_and_an_idle_one_costs_nothing() {
    let mut r = rig();
    let started = r.start();
    assert!(tap(&mut r.rec, 320, 200, 7, Damage::Full).is_some());
    r.run(2_000, 20); // the change at once, then the three repeats at 120, 400 and 1200 ms
    wait_for("the screen segment", || r.rec.writer_status().unwrap().screen_bytes > 400);
    let after_settles = r.rec.screen_stats().unwrap();
    assert_eq!((after_settles.changes_sent, after_settles.settles_sent), (1, 3));
    // A minute of nothing: no frames, no bytes.
    let before = r.rec.writer_status().unwrap().screen_bytes;
    r.run(60_000, 100);
    assert_eq!(r.rec.screen_stats().unwrap(), after_settles);
    assert_eq!(r.rec.writer_status().unwrap().screen_bytes, before);
    r.rec.stop("user").unwrap();

    let m = r.manifest(&started);
    let screen = m.tracks.screen.as_ref().unwrap();
    assert_eq!((screen.width, screen.height, screen.codec.as_str(), screen.vfr, screen.hardware), (320, 200, "h264", true, true));
    assert!((screen.scale - 2.0).abs() < f32::EPSILON);
    assert_eq!(screen.status, TrackStatus::Stopped);
    assert_eq!(screen.segments.len(), 1);
    let seg = &screen.segments[0];
    assert!(seg.complete && seg.samples == 4);
    let s = scan_file(&started.dir.join(&seg.file));
    assert!(s.finished && !s.truncated);
    assert_eq!((s.width, s.height, s.samples), (320, 200, 4));
    assert!(m.problems().is_empty(), "{:?}", m.problems());
}

#[test]
fn a_window_captured_by_the_system_is_recorded_like_the_taps_frames_and_says_where_it_came_from() {
    let mut r = rig();
    let started = r.start();
    let frame = |fill: u8| RawFrame { width: 320, height: 200, format: PixelFormat::Bgra8, bytes: vec![fill; 320 * 200 * 4] };
    assert!(r.rec.screen_frame(frame(1), 2.0).is_some());
    r.run(1_500, 100);
    assert!(r.rec.screen_frame(RawFrame { width: 321, height: 200, format: PixelFormat::Bgra8, bytes: vec![0; 321 * 200 * 4] }, 2.0).is_some(), "an odd one is reported ignored, not a crash");
    r.rec.stop("user").unwrap();
    let m = r.manifest(&started);
    let screen = m.tracks.screen.as_ref().unwrap();
    assert_eq!((screen.source.as_str(), screen.width, screen.height, screen.vfr), ("screencapturekit", 320, 200, true));
    assert!((screen.scale - 2.0).abs() < f32::EPSILON);
    assert_eq!(screen.segments[0].samples, 4, "the change and its three repeats");
    assert!(m.problems().is_empty(), "{:?}", m.problems());
}

#[test]
fn only_the_changed_rows_of_the_window_are_copied_and_a_pen_stroke_is_one_frame_in_the_file() {
    let mut r = rig();
    r.start();
    tap(&mut r.rec, 320, 200, 1, Damage::Full);
    r.run(2_000, 100);
    let rows = tap(&mut r.rec, 320, 200, 2, Damage::Rects(&[Rect { x: 10, y: 10, w: 40, h: 6 }]));
    assert_eq!(rows, Some(Tapped::Copied { rows: 6 }));
    r.run(2_000, 100);
    let s = r.rec.screen_stats().unwrap();
    assert_eq!(s.rows_copied, 200 + 6);
    assert_eq!((s.changes_sent, s.settles_sent), (2, 6), "two changes, each followed by its three repeats");
    r.rec.stop("user");
}

#[test]
fn a_resized_window_starts_a_new_segment_of_the_new_size() {
    let mut r = rig();
    let started = r.start();
    tap(&mut r.rec, 320, 200, 1, Damage::Full);
    r.run(1_500, 100);
    assert_eq!(tap(&mut r.rec, 400, 300, 2, Damage::Full), Some(Tapped::Resized));
    r.run(1_500, 100);
    r.rec.stop("user").unwrap();
    let m = r.manifest(&started);
    let segs = &m.tracks.screen.as_ref().unwrap().segments;
    assert_eq!(segs.len(), 2, "{segs:?}");
    assert_eq!((segs[0].width, segs[0].height), (0, 0), "the track's own size is left out");
    assert_eq!((segs[1].width, segs[1].height), (400, 300));
    assert_eq!(scan_file(&started.dir.join(&segs[1].file)).width, 400);
    assert!(m.problems().is_empty(), "{:?}", m.problems());
}

#[test]
fn an_encoder_that_is_behind_costs_dropped_frames_and_a_refused_change_is_sent_again() {
    // Every third picture is refused. Three separate changes of the window, each followed by its
    // repeats: the third change is refused once and goes in the next time.
    let factory = MockFactory { script: VideoScript { refuse_every: 3, ..VideoScript::default() }, ..MockFactory::default() };
    let mut r = rig_with(quiet(), factory);
    let started = r.start();
    for fill in 1..=3 {
        tap(&mut r.rec, 320, 200, fill, Damage::Full);
        r.run(1_500, 100);
    }
    let s = r.rec.screen_stats().unwrap();
    assert_eq!(s.changes_sent, 4, "three changes and the one that was refused and sent again: {s:?}");
    assert!(r.rec.indicator().dropped_frames >= 3, "{:?}", r.rec.indicator());
    r.rec.stop("user").unwrap();
    let m = r.manifest(&started);
    assert!(m.tracks.screen.as_ref().unwrap().segments[0].samples >= 8, "the pictures got through all the same");
}

#[test]
fn a_machine_with_no_h264_encoder_loses_the_screen_in_words_and_keeps_the_voice() {
    let mut r = rig_with(quiet(), MockFactory { no_video: true, ..MockFactory::default() });
    let started = r.start();
    tap(&mut r.rec, 320, 200, 1, Damage::Full);
    let notices = r.run(300, 100);
    assert!(
        notices.iter().any(|n| matches!(n, Notice::TrackOff { track: "screen", why } if why == "no H.264 encoder on this machine")),
        "{notices:?}"
    );
    for i in 0..50_u64 {
        r.advance(20);
        voice(&mut r.rec, 960, 0.2);
        let _ = i;
    }
    r.rec.stop("user").unwrap();
    let m = r.manifest(&started);
    let screen = m.tracks.screen.as_ref().unwrap();
    assert_eq!(screen.status, TrackStatus::Failed);
    assert_eq!(screen.note.as_deref(), Some("no H.264 encoder on this machine"));
    assert!(screen.segments.is_empty());
    assert_eq!(m.tracks.audio.as_ref().unwrap().segments.len(), 1, "the voice was recorded");
    assert!(m.problems().is_empty(), "{:?}", m.problems());
}

// ---- the voice and the camera ----

#[test]
fn the_voice_is_placed_where_it_was_said_and_lasts_what_was_said() {
    let mut r = rig();
    let started = r.start();
    r.advance(500); // nothing said for half a second
    for _ in 0..100 {
        r.advance(20);
        voice(&mut r.rec, 960, 0.3);
    }
    r.rec.stop("user").unwrap();
    let m = r.manifest(&started);
    let audio = m.tracks.audio.as_ref().unwrap();
    assert_eq!((audio.codec.as_str(), audio.sample_rate, audio.channels, audio.bitrate_kbps), ("opus", 48_000, 1, 32));
    assert_eq!(audio.device.as_deref(), Some("MacBook Air Microphone"));
    let seg = &audio.segments[0];
    assert_eq!(seg.start_us, 500_000, "the first chunk ended at 520 ms and was 20 ms long");
    assert_eq!(seg.end_us, Some(2_500_000));
    let s = scan_file(&started.dir.join(&seg.file));
    assert_eq!((s.samples, s.duration_us()), (100, 2_000_000), "100 packets of 20 ms, sample-exact");
    assert!(m.problems().is_empty(), "{:?}", m.problems());
}

#[test]
fn a_voice_input_that_lost_time_starts_again_at_the_new_time() {
    let mut r = rig();
    let started = r.start();
    for _ in 0..50 {
        r.advance(20);
        voice(&mut r.rec, 960, 0.3);
    }
    r.advance(2_000); // the microphone stalled for two seconds
    for _ in 0..50 {
        r.advance(20);
        voice(&mut r.rec, 960, 0.3);
    }
    r.rec.stop("user").unwrap();
    let m = r.manifest(&started);
    let seg = &m.tracks.audio.as_ref().unwrap().segments[0];
    let s = scan_file(&started.dir.join(&seg.file));
    assert_eq!(s.samples, 100);
    // The second half of the voice is where it was said, not two seconds early: the file is as long as
    // the session, with the stall in it.
    assert!(seg.end_us.unwrap() >= 4_000_000);
}

#[test]
fn the_camera_is_encoded_with_a_keyframe_every_two_seconds() {
    let mut r = rig();
    let started = r.start();
    for _ in 0..90 {
        r.advance(66);
        camera(&mut r.rec);
        r.rec.tick();
    }
    r.rec.stop("user").unwrap();
    let m = r.manifest(&started);
    let cam = m.tracks.camera.as_ref().unwrap();
    assert_eq!((cam.width, cam.height, cam.source.as_str(), cam.vfr), (64, 48, "camera", false));
    assert_eq!(cam.fps, 15);
    let s = scan_file(&started.dir.join(&cam.segments[0].file));
    assert_eq!(s.samples, 90);
    assert!(s.fragments >= 2, "keyframes at 0, 2 and 4 s cut the fragments: {}", s.fragments);
    assert!(m.problems().is_empty(), "{:?}", m.problems());
}

#[test]
fn a_camera_picture_of_an_odd_size_turns_the_camera_off_in_words() {
    let mut r = rig();
    r.start();
    r.rec.camera_frame(RawFrame { width: 63, height: 48, format: PixelFormat::Nv12, bytes: vec![0; 10] });
    let ind = r.rec.indicator();
    assert!(ind.problems.iter().any(|(t, w)| t == "camera" && w.contains("odd size")), "{:?}", ind.problems);
    r.rec.stop("user");
}

#[test]
fn a_refused_microphone_or_camera_is_said_in_the_manifest_and_the_others_record() {
    let mut r = rig();
    let mut req = request(&r.tmp.0);
    req.camera = Perm::Denied(String::from("the camera was refused: allow it in System Settings > Privacy & Security > Camera"));
    req.microphone = Perm::Denied(String::from("the microphone was refused"));
    let started = r.rec.start(req).unwrap();
    tap(&mut r.rec, 320, 200, 1, Damage::Full);
    r.run(500, 100);
    camera(&mut r.rec);
    voice(&mut r.rec, 960, 0.5);
    let ind = r.rec.indicator();
    assert_eq!(ind.problems.len(), 2, "{:?}", ind.problems);
    r.rec.stop("user").unwrap();
    let m = r.manifest(&started);
    let cam = m.tracks.camera.as_ref().unwrap();
    assert_eq!(cam.status, TrackStatus::Denied);
    assert!(cam.note.as_deref().unwrap().contains("System Settings"));
    assert!(cam.segments.is_empty());
    assert_eq!(m.tracks.audio.as_ref().unwrap().status, TrackStatus::Denied);
    assert_eq!(m.tracks.screen.as_ref().unwrap().segments.len(), 1, "the screen needs no permission");
    assert!(m.problems().is_empty(), "{:?}", m.problems());
}

// ---- pause, idle, quota, length ----

#[test]
fn a_pause_ends_the_run_and_ignores_what_comes_in_and_a_resume_begins_another() {
    let mut r = rig();
    let started = r.start();
    tap(&mut r.rec, 320, 200, 1, Damage::Full);
    r.run(1_500, 100);
    assert_eq!(r.rec.toggle_pause(), vec![Notice::Paused(PauseReason::User)]);
    assert_eq!(r.rec.state(), State::Paused(PauseReason::User));
    assert!(tap(&mut r.rec, 320, 200, 2, Damage::Full).is_none(), "nothing is taken while paused");
    r.advance(60_000); // a minute passes unrecorded
    assert_eq!(r.rec.toggle_pause(), vec![Notice::Resumed(PauseReason::User)]);
    tap(&mut r.rec, 320, 200, 3, Damage::Full);
    r.run(1_500, 100);
    let done = r.rec.stop("user").unwrap();
    assert_eq!(done.recorded_us, 3_000_000, "the minute is not in the recording");

    let m = r.manifest(&started);
    assert_eq!(m.runs.len(), 2);
    assert_eq!((m.runs[0].end_us, m.runs[0].end_reason.as_deref()), (Some(1_500_000), Some("user")));
    assert_eq!(m.runs[1].start_us, 61_500_000);
    let segs = &m.tracks.screen.as_ref().unwrap().segments;
    assert_eq!(segs.len(), 2, "a file holds only recorded time");
    assert!(segs[0].end_us.unwrap() <= 1_500_000 && segs[1].start_us >= 61_500_000);
    assert!(m.problems().is_empty(), "{:?}", m.problems());
    let events = fs::read_to_string(started.dir.join("events.jsonl")).unwrap();
    assert!(events.contains("session.pause") && events.contains("session.resume"));
}

#[test]
fn a_recording_pauses_itself_when_nothing_happens_and_goes_on_at_the_next_input() {
    let settings = RecorderSettings { idle_pause_us: Some(5 * 60_000_000), ..RecorderSettings::default() };
    let mut r = rig_with(settings, MockFactory::default());
    let started = r.start();
    tap(&mut r.rec, 320, 200, 1, Damage::Full);
    r.run(4 * 60_000, 1_000);
    assert_eq!(r.rec.state(), State::Recording, "four minutes: not yet");
    let notices = r.run(2 * 60_000, 1_000);
    assert!(notices.contains(&Notice::Paused(PauseReason::Idle)), "{notices:?}");
    assert_eq!(r.rec.state(), State::Paused(PauseReason::Idle));
    assert_eq!(r.rec.activity(), vec![Notice::Resumed(PauseReason::Idle)]);
    assert_eq!(r.rec.state(), State::Recording);
    // A change of the window counts as activity: the recording is not paused again at once.
    r.run(60_000, 1_000);
    tap(&mut r.rec, 320, 200, 9, Damage::Full);
    r.run(4 * 60_000, 1_000);
    assert_eq!(r.rec.state(), State::Recording);
    r.rec.stop("user").unwrap();
    let m = r.manifest(&started);
    assert_eq!(m.runs[0].end_reason.as_deref(), Some("idle"));
    assert!(fs::read_to_string(started.dir.join("events.jsonl")).unwrap().contains("session.idle_pause"));
}

#[test]
fn the_recorder_pauses_for_the_disk_warns_before_and_goes_on_only_with_room() {
    let mut r = rig();
    let started = r.start();
    // Getting low: under half as much again as is kept, plus a gigabyte.
    r.free.store(14 * GB, Ordering::SeqCst);
    let n = r.run(5_000, 1_000);
    assert!(n.contains(&Notice::DiskLow { free: 14 * GB, min_free: 10 * GB }), "{n:?}");
    assert_eq!(r.rec.state(), State::Recording);
    assert!(r.run(5_000, 1_000).iter().all(|n| !matches!(n, Notice::DiskLow { .. })), "said once");
    // Under what is kept: paused.
    r.free.store(9 * GB, Ordering::SeqCst);
    let n = r.run(5_000, 1_000);
    assert!(n.contains(&Notice::Paused(PauseReason::Quota)), "{n:?}");
    assert_eq!(r.rec.state(), State::Paused(PauseReason::Quota));
    // A little room is not enough; with the margin it goes on.
    r.free.store(10 * GB + GB / 2, Ordering::SeqCst);
    r.run(5_000, 1_000);
    assert_eq!(r.rec.state(), State::Paused(PauseReason::Quota));
    r.free.store(11 * GB, Ordering::SeqCst);
    let n = r.run(5_000, 1_000);
    assert!(n.contains(&Notice::Resumed(PauseReason::Quota)), "{n:?}");
    assert_eq!(r.rec.state(), State::Recording);
    r.rec.stop("user").unwrap();
    let m = r.manifest(&started);
    assert_eq!(m.runs.len(), 2);
    assert_eq!(m.runs[0].end_reason.as_deref(), Some("quota"));
    assert!(fs::read_to_string(started.dir.join("events.jsonl")).unwrap().contains("session.quota_pause"));
}

#[test]
fn a_disk_that_cannot_be_asked_is_said_once_and_does_not_stop_the_recording() {
    let clock = MockClock::at(0);
    let c = clock.clone();
    let deps = Deps {
        unix_ms: Box::new(|| 1_764_108_000_000),
        make_clock: Box::new(move |_| Arc::new(c.clone()) as Arc<dyn Clock>),
        free_bytes: Box::new(|_| None),
        factory: Box::new(MockFactory::default()),
    };
    let mut rec = Recorder::new(quiet(), deps);
    let tmp = Tmp::new();
    rec.start(request(&tmp.0)).unwrap();
    let mut told = 0;
    for _ in 0..20 {
        clock.advance_ms(5_000);
        told += rec.tick().iter().filter(|n| **n == Notice::DiskUnknown).count();
    }
    assert_eq!(told, 1);
    assert_eq!(rec.state(), State::Recording);
    assert_eq!(rec.indicator().disk, DiskState::Unknown);
    rec.stop("user");
}

#[test]
fn a_session_ends_at_its_longest_length_after_a_warning() {
    let settings = RecorderSettings { max_session_us: 3_600_000_000, ..quiet() };
    let mut r = rig_with(settings, MockFactory::default());
    let started = r.start();
    let n = r.run(50 * 60_000, 10_000);
    assert!(n.iter().any(|n| matches!(n, Notice::Ending { left_us: 600_000_000 })), "{n:?}");
    assert_eq!(r.rec.state(), State::Recording);
    let n = r.run(10 * 60_000, 10_000);
    assert!(n.contains(&Notice::LengthReached), "{n:?}");
    assert_eq!(r.rec.state(), State::Idle, "stopped by itself");
    assert_eq!(r.manifest(&started).state, SessionState::Complete);
}

// ---- events, transcript, indicator ----

#[test]
fn events_and_the_transcript_are_stamped_on_the_session_clock_and_go_to_their_files() {
    let mut r = rig();
    assert_eq!(r.rec.event("marker", "{}"), Ok(()), "nothing is going: nothing happens");
    let started = r.start();
    r.advance(2_500);
    r.rec.event("nav.file", r#"{"file":"src/lib.rs","lines":120}"#).unwrap();
    r.rec.transcript(TranscriptLine {
        t0_us: 1_000_000,
        t1_us: 2_000_000,
        text: String::from("this leaks"),
        lang: String::from("en"),
        engine: String::from("mock"),
        conv: Some(1),
        words: vec![Word { w: String::from("this"), t0_us: 1_000_000, t1_us: 1_300_000, p: Some(0.9) }],
    });
    let sink = r.rec.event_sink().unwrap();
    let thread = std::thread::spawn(move || sink.event("marker", r#"{"label":"from another thread"}"#));
    assert_eq!(thread.join().unwrap(), Ok(()));
    r.rec.stop("user").unwrap();
    let events = fs::read_to_string(started.dir.join("events.jsonl")).unwrap();
    assert!(events.contains(r#"{"t":2500000,"k":"nav.file","file":"src/lib.rs","lines":120}"#), "{events}");
    assert!(events.contains("from another thread"));
    let m = r.manifest(&started);
    assert_eq!(m.transcript.as_ref().unwrap().count, 1);
    assert_eq!(m.events.as_ref().unwrap().count as usize, events.lines().count());
    let (lines, problems) = crate::recorder_manifest::parse_transcript(&fs::read_to_string(started.dir.join("transcript.jsonl")).unwrap());
    assert_eq!((lines.len(), problems.len()), (1, 0));
}

#[test]
fn the_indicator_shows_the_time_recorded_the_voice_the_bytes_and_the_hours_left() {
    let mut r = rig();
    assert_eq!(r.rec.indicator().state, State::Idle);
    r.start();
    tap(&mut r.rec, 320, 200, 1, Damage::Full);
    for _ in 0..40 {
        r.advance(50);
        voice(&mut r.rec, 960, 0.8);
        r.rec.tick();
    }
    wait_for("some bytes", || r.rec.indicator().media_bytes > 400);
    let ind = r.rec.indicator();
    assert_eq!(ind.state, State::Recording);
    assert_eq!(ind.recorded_us, 2_000_000);
    assert!(ind.level > 0.5, "the meter shows the loud voice: {}", ind.level);
    assert_eq!(ind.free_bytes, Some(500 * GB), "asked at the start");
    r.run(6_000, 1_000);
    let ind = r.rec.indicator();
    assert_eq!(ind.free_bytes, Some(500 * GB));
    assert_eq!(ind.disk, DiskState::Fine);
    assert!(ind.hours_left.is_some_and(|h| h > 100.0), "{:?}", ind.hours_left);
    assert!(ind.level < 0.5, "the meter falls when nothing comes in: {}", ind.level);
    assert!(ind.problems.is_empty());
    r.rec.toggle_pause();
    r.advance(30_000);
    assert_eq!(r.rec.indicator().recorded_us, 8_000_000, "paused time is not counted: 2 + 6 s");
    r.rec.stop("user");
}

#[test]
fn a_recorder_that_is_dropped_without_a_stop_leaves_files_that_play() {
    let mut r = rig();
    let started = r.start();
    tap(&mut r.rec, 320, 200, 1, Damage::Full);
    r.run(1_500, 100);
    wait_for("the segment", || r.rec.writer_status().unwrap().screen_bytes > 400);
    drop(r.rec); // the app was torn down
    let m = Manifest::parse(&fs::read_to_string(started.dir.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(m.state, SessionState::Complete);
    let seg = &m.tracks.screen.as_ref().unwrap().segments[0];
    assert!(scan_file(&started.dir.join(&seg.file)).finished);
}

#[test]
fn the_voice_meter_falls_while_the_recording_is_paused_too() {
    let mut r = rig();
    r.start();
    r.advance(100);
    voice(&mut r.rec, 960, 0.9);
    r.rec.toggle_pause();
    voice(&mut r.rec, 960, 0.9); // the meter still shows the voice
    let loud = r.rec.indicator().level;
    assert!(loud > 0.8, "{loud}");
    r.run(4_000, 100);
    assert!(r.rec.indicator().level < 0.01, "it fell: {}", r.rec.indicator().level);
    r.rec.stop("user");
}

#[test]
fn an_input_that_runs_slow_or_fast_against_the_session_clock_is_kept_in_step_with_the_picture() {
    // 20 ms of voice arrives every 20.02 ms (slow) or 19.98 ms (fast): 1000 ppm, ten times what a sound
    // card does, for five minutes. The voice's timeline must stay within a tenth of a second of the
    // session clock, or the voice and the picture of the exported video part.
    for (interval_us, what) in [(20_020_u64, "slow"), (19_980, "fast")] {
        let mut r = rig();
        let started = r.start();
        let mut t = 0;
        for _ in 0..15_100 {
            t += interval_us;
            r.clock.set_us(t);
            voice(&mut r.rec, 960, 0.1);
        }
        let (last, max) = r.rec.audio_drift().unwrap();
        assert!(last.unsigned_abs() < 100_000 && max < 100_000, "{what}: drift {last} us now, {max} us at most");
        r.rec.stop("user").unwrap();
        let m = r.manifest(&started);
        let segs = &m.tracks.audio.as_ref().unwrap().segments;
        assert_eq!(segs.len(), 2, "{what}: five minutes make a second segment");
        // The voice's timeline is in step with the clock, so the second segment begins where the
        // voice's 300 s are: its packets are in order and the segments do not overlap.
        assert!(segs[1].start_us >= segs[0].end_us.unwrap().saturating_sub(100_000), "{what}: the segments are in order: {segs:?}");
        assert!(segs[1].start_us > 299_000_000 && segs[1].start_us < 302_000_000, "{what}: {}", segs[1].start_us);
    }
}

// ---- what it costs: a measurement, not a check ----

/// The CPU seconds this process has used (user and system) and the most memory it has held, in bytes:
/// the system's own account of it, to price a stand-in session in the units of RECORDER.md section 9.
#[cfg(target_os = "macos")]
fn process_cost() -> (f64, u64) {
    #[repr(C)]
    struct TimeVal {
        sec: i64,
        usec: i32,
    }
    #[repr(C)]
    struct RUsage {
        utime: TimeVal,
        stime: TimeVal,
        maxrss: i64,
        others: [i64; 13],
    }
    extern "C" {
        fn getrusage(who: i32, usage: *mut RUsage) -> i32;
    }
    let mut u = RUsage { utime: TimeVal { sec: 0, usec: 0 }, stime: TimeVal { sec: 0, usec: 0 }, maxrss: 0, others: [0; 13] };
    // SAFETY: `RUsage` is the layout of macOS's `struct rusage` (two timevals and fourteen longs), and
    // `getrusage(RUSAGE_SELF = 0)` only writes into it.
    let status = unsafe { getrusage(0, &mut u) };
    assert_eq!(status, 0, "getrusage");
    let secs = |t: &TimeVal| t.sec as f64 + f64::from(t.usec) / 1e6;
    (secs(&u.utime) + secs(&u.stime), u.maxrss.max(0) as u64)
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "a measurement: cargo test --release -p AzReview --lib -- --ignored --nocapture a_simulated_session"]
fn a_simulated_session_of_ten_minutes_is_priced_in_cpu_seconds_per_hour_and_memory() {
    // A 2880 x 1800 window, a 720p camera at 15 pictures a second and the voice in 20 ms chunks, ten
    // minutes at a clock the loop moves as fast as it can, with stand-in encoders that keep no picture:
    // the cost of the recorder's own code (the shadow frame, the pacing, the muxer, the writer thread).
    const SECONDS: u64 = 600;
    let (w, h) = (2880_u32, 1800_u32);
    let pixels = window(w, h, 7);
    let stroke = [Rect { x: 1000, y: 600, w: 400, h: 300 }];
    for scenario in ["idle", "typical", "busy"] {
        let factory = MockFactory { script: VideoScript { keep: 0, ..VideoScript::default() }, ..MockFactory::default() };
        let mut r = rig_with(quiet(), factory);
        let (cpu0, _) = process_cost();
        r.start();
        let mut next_camera = 0_u64;
        for tick in 0..SECONDS * 10 {
            let t = tick * 100;
            r.clock.set_us(t * 1_000);
            let damage = match scenario {
                "busy" => Some(Damage::Full),
                "typical" if tick % 10 < 2 => Some(Damage::Rects(&stroke)),
                _ if tick == 0 => Some(Damage::Full),
                _ => None,
            };
            if let Some(damage) = damage {
                r.rec.tap(&TapFrame { width: w, height: h, stride: w as usize * 4, scale: 2.0, damage, pixels: &pixels });
            }
            while next_camera <= t + 100 {
                r.clock.set_us(next_camera.max(t) * 1_000);
                r.rec.camera_frame(RawFrame { width: 1280, height: 720, format: PixelFormat::Nv12, bytes: vec![0; 1280 * 720 * 3 / 2] });
                next_camera += 66;
            }
            for k in 0..5 {
                r.clock.set_us((t + k * 20 + 20) * 1_000);
                voice(&mut r.rec, 960, 0.1);
            }
            r.rec.tick();
        }
        let done = r.rec.stop("measured").unwrap();
        let (cpu1, peak) = process_cost();
        let cpu = cpu1 - cpu0;
        assert!(done.clean, "{scenario}: {done:?}");
        eprintln!(
            "{scenario:8} {SECONDS} simulated s: CPU {cpu:.2} s = {:.1} CPU-s per hour = {:.2}% of one core; peak memory so far {} MB; media {} bytes",
            cpu / SECONDS as f64 * 3600.0,
            cpu / SECONDS as f64 * 100.0,
            peak / 1_000_000,
            done.media_bytes
        );
    }
}
