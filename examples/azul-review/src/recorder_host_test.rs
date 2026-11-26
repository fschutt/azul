//! The recorder as the app holds it: what the window asks the system for at each moment, how a recording
//! starts and says why it did not, what the panel keeps of the recorder's notices, and the list of recordings.
//! On stand-in encoders and a clock the test moves; no window, no azul.

use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
};

use azul_appkit::phrase::{Arg, Phrase, Text};

use super::*;
use crate::{
    recorder::{DesktopSample, FrontApp},
    recorder_clock::{Clock, MockClock},
    recorder_codec::{MockFactory, PixelFormat, RawFrame},
    recorder_manifest::{AppInfo, HostInfo, Workspace},
    recorder_panel::{Access, Do, Dot, Severity},
    recorder_settings::{RecorderSettings, ScreenKind},
};

struct Tmp(PathBuf);

impl Tmp {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!("azreview-host-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
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
    host: RecorderHost,
    clock: MockClock,
    free: Arc<std::sync::atomic::AtomicU64>,
    tmp: Tmp,
}

fn facts(dir: &std::path::Path) -> Facts {
    Facts {
        app: AppInfo { name: String::from("AzReview"), version: String::from("0.1.0") },
        workspace: Workspace { root: String::from("/w"), name: String::from("w") },
        host: HostInfo { os: String::from("macos"), arch: String::from("aarch64") },
        utc_offset_minutes: 60,
        recordings_dir: dir.join("recordings"),
        who: Arg::from("Terminal"),
        decimal: '.',
        hotkey: String::from("Cmd+Shift+P"),
    }
}

fn rig_with(settings: RecorderSettings) -> Rig {
    let clock = MockClock::at(0);
    let free = Arc::new(std::sync::atomic::AtomicU64::new(500 * 1_000_000_000));
    let (c, f) = (clock.clone(), Arc::clone(&free));
    let deps = Deps {
        unix_ms: Box::new(|| 1_764_108_000_000),
        make_clock: Box::new(move |_| Arc::new(c.clone()) as Arc<dyn Clock>),
        free_bytes: Box::new(move |_| Some(f.load(Ordering::SeqCst))),
        factory: Box::new(MockFactory::default()),
    };
    let tmp = Tmp::new();
    Rig { host: RecorderHost::new(settings, facts(&tmp.0), deps), clock, free, tmp }
}

fn quiet() -> RecorderSettings {
    RecorderSettings { idle_pause_us: None, ..RecorderSettings::default() }
}

fn rig() -> Rig {
    rig_with(quiet())
}

const ALL_ALLOWED: Accesses = Accesses { screen: Access::Allowed, camera: Access::Allowed, microphone: Access::Allowed };
const DISPLAY: DisplayFacts = DisplayFacts { id: 1, width: 2560, height: 1664 };

fn devices() -> Devices {
    Devices { screen: Some(String::from("Built-in Display")), camera: Some(String::from("FaceTime HD Camera")), microphone: Some(String::from("MacBook Microphone")) }
}

impl Rig {
    fn start(&mut self) -> bool {
        self.host.start(ALL_ALLOWED, devices(), DISPLAY, &[DISPLAY])
    }
}

// ---- what the window asks the system for ----

/// A picture is no bigger than the display and no wider than the setting; its aspect is the display's, its sides
/// are even (an encoder takes no odd side).
///
/// Mutation: an enlarged picture; a distorted aspect; an odd side.
#[test]
fn the_picture_asked_for_keeps_the_displays_aspect_never_grows_and_has_even_sides() {
    assert_eq!(consumer_size(2560, 1664, 1920), (1920, 1248));
    assert_eq!(consumer_size(1440, 900, 1920), (1440, 900), "a display narrower than the setting is not enlarged");
    assert_eq!(consumer_size(1920, 1080, 1920), (1920, 1080));
    assert_eq!(consumer_size(3840, 2160, 1920), (1920, 1080));
    assert_eq!(consumer_size(1366, 768, 1366), (1366, 768));
    assert_eq!(consumer_size(1365, 767, 2000), (1364, 766), "odd sides are cut down to even ones: neither the display nor the setting is exceeded");
    let (w, h) = consumer_size(2561, 1601, 999);
    assert!(w % 2 == 0 && h % 2 == 0 && w <= 999, "{w} x {h}");
    assert_eq!(consumer_size(1, 1, 1920), (0, 0), "a side that is cut to nothing is no picture");
    assert_eq!(consumer_size(0, 0, 1920), (0, 0), "no display, no picture");
    assert_eq!(consumer_size(2560, 1664, 0), (0, 0), "a width of nothing is no picture");
}

/// Nothing is open while nothing records: no screen, no camera, no microphone. A recording asks for the
/// screen (the display, in the setting's size), the camera and the microphone; a pause lets them all go.
///
/// Mutation: devices left open while idle; devices left open in a pause; the screen when it is off in the settings.
#[test]
fn nothing_is_open_until_a_recording_runs_and_a_pause_lets_everything_go() {
    let mut r = rig();
    assert_eq!(r.host.wants(), Wants::default(), "idle: nothing");
    assert!(r.start(), "{:?}", r.host.error());
    let wants = r.host.wants();
    assert_eq!(wants.screen, Some(ScreenWant { display_id: 1, width: 1920, height: 1248, exclude: vec![String::from("com.apple.notificationcenterui")] }));
    assert!(wants.camera && wants.microphone && !wants.tap);
    r.host.toggle_pause();
    assert_eq!(r.host.wants(), Wants::default(), "paused: the devices are let go");
    r.host.toggle_pause();
    assert!(r.host.wants().camera, "and open again when it goes on");
    r.host.stop();
    assert_eq!(r.host.wants(), Wants::default(), "stopped: nothing");
}

/// The camera's preview can be on before and between recordings: the camera alone, and never the screen or the
/// microphone; a camera the system refused is not asked for.
///
/// Mutation: a preview that opens the microphone; a preview of a refused camera.
#[test]
fn the_camera_preview_opens_the_camera_alone_and_only_where_it_is_allowed() {
    let mut r = rig();
    r.host.set_preview(true, Access::Allowed);
    assert_eq!(r.host.wants(), Wants { camera: true, ..Wants::default() });
    r.host.set_preview(true, Access::Denied);
    assert_eq!(r.host.wants(), Wants::default(), "a refused camera is not opened");
    r.host.set_preview(false, Access::Allowed);
    assert!(!r.host.preview());
    assert_eq!(r.host.wants(), Wants::default());
}

/// What the settings and the system say takes devices out of a recording: a screen that is off, a camera that is
/// off or refused, the review's own frames (the window taps them, the capture is not asked).
///
/// Mutation: a refused camera asked for anyway; the own frames' source that also captures the display.
#[test]
fn the_settings_and_the_systems_answers_take_devices_out_of_what_is_asked_for() {
    let mut r = rig_with(RecorderSettings { camera: false, microphone: false, ..quiet() });
    assert!(r.start());
    let wants = r.host.wants();
    assert!(wants.screen.is_some() && !wants.camera && !wants.microphone);

    let mut r = rig();
    assert!(r.host.start(Accesses { camera: Access::Denied, microphone: Access::Restricted, ..ALL_ALLOWED }, devices(), DISPLAY, &[DISPLAY]));
    let wants = r.host.wants();
    assert!(wants.screen.is_some() && !wants.camera && !wants.microphone, "{wants:?}");

    let mut r = rig_with(RecorderSettings { screen_source: ScreenKind::OwnFrames, ..quiet() });
    assert!(r.start());
    let wants = r.host.wants();
    assert_eq!((wants.screen.is_some(), wants.tap), (false, true), "the window's own frames come from its tap");

    let mut r = rig_with(RecorderSettings { screen: false, ..quiet() });
    assert!(r.start());
    assert_eq!((r.host.wants().screen, r.host.wants().tap), (None, false));

    let mut r = rig();
    assert!(r.host.start(Accesses { screen: Access::Denied, ..ALL_ALLOWED }, devices(), DISPLAY, &[DISPLAY]));
    assert_eq!(r.host.wants().screen, None, "a refused screen is not asked for again every second");
}

/// The exclusion list of the settings goes to the capture as it is.
#[test]
fn the_apps_to_leave_out_are_the_settings() {
    let mut r = rig_with(RecorderSettings { screen_exclude: vec![String::from("com.example.One"), String::from("com.example.Two")], ..quiet() });
    assert!(r.start());
    assert_eq!(r.host.wants().screen.unwrap().exclude, vec!["com.example.One", "com.example.Two"]);
}

/// The display of a recording is the one the window is on, unless the settings name another by the system's
/// number; a number of a display that is not there is said, and the window's display is recorded (never none).
///
/// Mutation: a named display that is ignored; an unknown number that records nothing or says nothing.
#[test]
fn the_display_is_the_windows_unless_the_settings_name_another_and_a_missing_one_is_said() {
    let (a, b) = (DisplayFacts { id: 11, width: 2560, height: 1664 }, DisplayFacts { id: 22, width: 1920, height: 1080 });
    assert_eq!(pick_display(0, a, &[a, b]), (a, None), "no display named: the window's");
    assert_eq!(pick_display(22, a, &[a, b]), (b, None), "another, by the system's number");
    assert_eq!(pick_display(11, a, &[a, b]), (a, None));
    let (picked, said) = pick_display(99, a, &[a, b]);
    assert_eq!(picked, a, "the window's, since display 99 is not there");
    let said = said.expect("it is said");
    assert_eq!(said.phrase("azreview-rec-notice-display-missing").unwrap().get("id"), Some(&Arg::Int(99)));
    assert_eq!(pick_display(22, a, &[]), (a, Some(Text::from(Phrase::new("azreview-rec-notice-display-missing").arg("id", 22_u32)))), "no list, no choice");
}

/// A recording of the display the settings name is of that display, at its own size, and one of a display that is
/// not there says so in the panel and records the window's.
///
/// Mutation: the named display's size not used; the missing display not said in the panel.
#[test]
fn a_named_display_is_the_one_recorded_at_its_own_size_and_a_missing_one_is_said_in_the_panel() {
    let (a, b) = (DisplayFacts { id: 11, width: 2560, height: 1664 }, DisplayFacts { id: 22, width: 1920, height: 1080 });
    let mut r = rig_with(RecorderSettings { screen_display: 22, ..quiet() });
    assert!(r.host.start(ALL_ALLOWED, devices(), a, &[a, b]), "{:?}", r.host.error());
    let screen = r.host.wants().screen.expect("the screen");
    assert_eq!((screen.display_id, screen.width, screen.height), (22, 1920, 1080));
    let mut r = rig_with(RecorderSettings { screen_display: 99, ..quiet() });
    assert!(r.host.start(ALL_ALLOWED, devices(), a, &[a, b]));
    let screen = r.host.wants().screen.expect("the screen");
    assert_eq!((screen.display_id, screen.width, screen.height), (11, 1920, 1248), "the window's display");
    assert!(r.host.recent().any(|n| n.severity == Severity::Warning && n.text.phrase("azreview-rec-notice-display-missing").is_some()), "{:?}", r.host.recent().collect::<Vec<_>>());
}

// ---- starting ----

/// A recording starts with what the system allows, and a refusal of one device does not stop the others: the panel
/// names the one that is off.
///
/// Mutation: a refused camera that stops the recording; a refusal that is not said.
#[test]
fn a_recording_starts_and_a_refused_device_is_named_while_the_rest_records() {
    let mut r = rig();
    assert!(r.host.start(Accesses { camera: Access::Denied, ..ALL_ALLOWED }, devices(), DISPLAY, &[DISPLAY]), "{:?}", r.host.error());
    assert_eq!(r.host.hud().dot, Dot::Recording);
    assert!(r.host.error().is_none());
    r.clock.advance_ms(200);
    r.host.tick();
    let hud = r.host.hud();
    assert!(
        hud.notes.iter().any(|n| n.severity == Severity::Warning && n.text.phrase("azreview-rec-problem").is_some_and(|p| p.get("track") == Some(&Arg::word("azreview-rec-track-camera", "camera")))),
        "the camera is named: {:?}",
        hud.notes
    );
}

/// A recording that cannot start says why in words: a window source azul cannot capture yet (never a quiet fall
/// back to the whole display), a recording going already, a full disk, nothing to record.
///
/// Mutation: a window source that records the display; an error that is not kept.
#[test]
fn a_recording_that_cannot_start_says_why_and_nothing_is_recorded() {
    let mut r = rig_with(RecorderSettings { screen_source: ScreenKind::Window, ..quiet() });
    assert!(!r.start());
    assert_eq!(r.host.error(), Some(&Text::key("azreview-rec-error-window-source")));
    assert_eq!(r.host.hud().dot, Dot::Idle);
    assert_eq!(r.host.wants(), Wants::default());

    let mut r = rig();
    assert!(r.start());
    assert!(!r.start(), "a second start");
    assert_eq!(r.host.error(), Some(&Text::key("azreview-rec-error-busy")));
    assert_eq!(r.host.hud().dot, Dot::Recording, "the first goes on");

    let mut r = rig();
    r.free.store(4_500_000_000, Ordering::SeqCst);
    assert!(!r.start());
    let error = r.host.error().expect("words");
    let p = error.phrase("azreview-rec-error-disk-full").expect("the disk");
    assert_eq!(p.get("free"), Some(&Arg::from("4.5 GB")));

    let mut r = rig_with(RecorderSettings { screen: false, camera: false, microphone: false, ..quiet() });
    assert!(!r.start());
    assert_eq!(r.host.error(), Some(&Text::key("azreview-rec-error-nothing")));

    // A start that works forgets the last error.
    let mut r = rig();
    r.free.store(4_500_000_000, Ordering::SeqCst);
    assert!(!r.start());
    r.free.store(500_000_000_000, Ordering::SeqCst);
    assert!(r.start());
    assert!(r.host.error().is_none());
}

// ---- what the panel keeps ----

/// The recorder's notices are kept for the panel, the newest last and at most twenty; a pause and a
/// resume show up in them.
///
/// Mutation: a notice that is dropped; more than twenty kept.
#[test]
fn the_recorders_notices_are_kept_for_the_panel_the_newest_last_and_at_most_twenty() {
    let mut r = rig();
    assert!(r.start());
    r.host.toggle_pause();
    r.host.toggle_pause();
    let kept: Vec<Text> = r.host.recent().map(|n| n.text.clone()).collect();
    assert_eq!(kept, vec![Text::key("azreview-rec-status-paused-user"), Text::key("azreview-rec-notice-resumed")]);
    for _ in 0..40 {
        r.host.toggle_pause();
    }
    assert_eq!(r.host.recent().count(), 20);
    assert_eq!(r.host.recent().last().map(|n| n.text.clone()), Some(Text::key("azreview-rec-notice-resumed")), "the newest is kept");
    // A new recording starts with a clean list: what the last one said is not this one's.
    r.host.stop();
    assert!(r.host.recent().count() > 0, "the last one's saved notice");
    assert!(r.start());
    assert_eq!(r.host.recent().count(), 0, "{:?}", r.host.recent().collect::<Vec<_>>());
}

/// The pause key pauses a recording and resumes it, and does nothing when nothing records.
///
/// Mutation: a pause of an idle recorder.
#[test]
fn the_pause_key_pauses_and_resumes_and_does_nothing_when_idle() {
    let mut r = rig();
    r.host.toggle_pause();
    assert_eq!((r.host.hud().dot, r.host.recent().count()), (Dot::Idle, 0));
    assert!(r.start());
    r.host.toggle_pause();
    let hud = r.host.hud();
    assert_eq!(hud.dot, Dot::Paused);
    assert_eq!(hud.buttons.iter().map(|b| b.what).collect::<Vec<_>>(), vec![Do::Resume, Do::Stop]);
    assert_eq!(hud.buttons[0].described, Phrase::new("azreview-rec-resume-described").arg("hotkey", "Cmd+Shift+P"), "the key as the facts name it");
    r.host.toggle_pause();
    assert_eq!(r.host.hud().dot, Dot::Recording);
}

/// What comes in goes to the files: the picture of the screen, the camera's, the voice, and what the desktop was
/// doing; stopping keeps the recording in the list with its length and says where it was saved.
///
/// Mutation: frames that are not passed on; a stop that does not refresh the list.
#[test]
fn what_comes_in_goes_to_the_files_and_a_stopped_recording_is_in_the_list() {
    let mut r = rig();
    assert!(r.start());
    let (w, h) = (1920, 1248);
    for i in 0..30_u8 {
        r.clock.advance_ms(100);
        r.host.screen_frame(RawFrame { width: w, height: h, format: PixelFormat::Rgba8, bytes: vec![i; w as usize * h as usize * 4] }, 1.0);
        r.host.camera_frame(RawFrame { width: 64, height: 48, format: PixelFormat::Nv12, bytes: vec![0; 64 * 48 * 3 / 2] });
        r.host.audio_chunk(&vec![0.1; 4_800], 48_000, 1);
        r.host.tick();
    }
    r.host.desktop_sample(&DesktopSample { front: Some(FrontApp { bundle: String::from("com.apple.Safari"), name: String::from("Safari") }), ..DesktopSample::default() });
    assert!(r.host.rows().is_empty(), "the list is read when it is asked to be");
    r.host.stop();
    let rows = r.host.rows();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].tracks, vec!["screen", "camera", "audio"]);
    assert_eq!(rows[0].state, Phrase::new("azreview-rec-state-complete"));
    assert!(rows[0].length.starts_with("0:0") || rows[0].length.starts_with("0:1"), "{}", rows[0].length);
    let saved = r.host.recent().last().expect("said").text.phrase("azreview-rec-notice-saved").cloned().expect("saved");
    assert_eq!(saved.get("name"), Some(&Arg::from(rows[0].name.as_str())));
    assert_eq!(r.host.hud().dot, Dot::Idle);
    assert!(r.host.path_of(&rows[0].name).is_some_and(|p| p.join("manifest.json").is_file()));
    assert!(r.host.event_sink().is_none(), "no recording, no sink");
}

/// A recording can be deleted from the list, and nothing else can: a name that is not a recording's is refused
/// in words.
///
/// Mutation: a delete that reaches outside the recordings folder; a refusal that is not said.
#[test]
fn a_recording_can_be_deleted_and_a_name_that_is_not_one_is_refused() {
    let mut r = rig();
    assert!(r.start());
    r.clock.advance_ms(500);
    r.host.stop();
    let name = r.host.rows()[0].name.clone();
    let outside = r.tmp.0.join("keep.txt");
    fs::write(&outside, b"mine").unwrap();
    assert!(!r.host.delete("../keep.txt"));
    assert!(!r.host.delete("missing"));
    assert!(outside.is_file(), "nothing outside the folder is touched");
    let error = r.host.recent().last().expect("said");
    assert_eq!(error.severity, Severity::Warning);
    assert!(error.text.phrase("azreview-rec-error-delete").is_some(), "{:?}", error.text);
    assert!(r.host.delete(&name));
    assert!(r.host.rows().is_empty());
    assert_eq!(r.host.path_of(&name), None, "a name that is gone is no path");
    for not_a_name in ["../keep.txt", "..", ".", "recordings/..", "a/b", "/", ""] {
        assert_eq!(r.host.path_of(not_a_name), None, "{not_a_name:?}: a path is not a name");
    }
}

/// A recording that is being written cannot be deleted from under its writer.
///
/// Mutation: a delete of the recording that is going.
#[test]
fn a_recording_that_is_going_is_not_deleted() {
    let mut r = rig();
    assert!(r.start());
    let name = r.host.current().expect("a recording is going");
    // (It is a recording to the folder's eyes once the writer has written its manifest, a moment after the start.)
    let folder = r.tmp.0.join("recordings").join(&name);
    for _ in 0..500 {
        if folder.join("manifest.json").is_file() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(folder.join("manifest.json").is_file());
    assert!(!r.host.delete(&name));
    assert!(folder.join("manifest.json").is_file(), "the folder is still there, whole");
    let said = r.host.recent().last().expect("said");
    assert_eq!(said.severity, Severity::Warning);
    assert!(said.text.phrase("azreview-rec-error-delete").is_some_and(|p| p.get("name") == Some(&Arg::from(name.as_str()))), "{:?}", said.text);
    assert!(r.host.path_of(&name).is_some(), "it is still there");
    assert_eq!(r.host.current(), Some(name), "and still going");
}

/// A recording that was cut off (the app ended while it ran) is put right when the app starts again, and the
/// panel says how many. (The folder of a recording that is going is copied, as a killed process leaves it, and the
/// copy is what the next start finds.)
///
/// Mutation: no recovery at the start; no word about it.
#[test]
fn a_recording_cut_off_is_put_right_at_the_next_start_and_the_panel_says_so() {
    fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap().flatten() {
            let (src, dst) = (entry.path(), to.join(entry.file_name()));
            if src.is_dir() {
                copy_dir(&src, &dst);
            } else {
                let _ = fs::copy(&src, &dst);
            }
        }
    }
    let mut r = rig();
    assert!(r.start());
    r.clock.advance_ms(1_000);
    r.host.tick();
    // (The writer thread writes the first manifest a moment after the start.)
    let name = r.host.current().expect("going");
    let manifest = r.tmp.0.join("recordings").join(&name).join("manifest.json");
    for _ in 0..500 {
        if manifest.is_file() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(manifest.is_file(), "the manifest of a recording that began");
    let elsewhere = Tmp::new();
    copy_dir(&r.tmp.0.join("recordings").join(&name), &elsewhere.0.join("recordings").join(&name));
    r.host.stop();

    let clock = MockClock::at(0);
    let deps = Deps {
        unix_ms: Box::new(|| 1_764_108_000_000),
        make_clock: Box::new(move |_| Arc::new(clock.clone()) as Arc<dyn Clock>),
        free_bytes: Box::new(|_| Some(500_000_000_000)),
        factory: Box::new(MockFactory::default()),
    };
    let mut again = RecorderHost::new(quiet(), facts(&elsewhere.0), deps);
    assert_eq!(again.recover(), 1);
    let said = again.recent().last().expect("said").text.phrase("azreview-rec-notice-recovered").cloned().expect("words");
    assert_eq!(said.get("count"), Some(&Arg::Int(1)));
    assert_eq!(again.rows().iter().map(|row| row.state.clone()).collect::<Vec<_>>(), vec![Phrase::new("azreview-rec-state-recovered")]);
    assert_eq!(again.recover(), 0, "a recording that is right is not recovered again");
    assert_eq!(again.recent().count(), 1, "and nothing is said again");
}

/// A locked screen pauses the recording (the setting says so) and the unlock goes on, only after a pause the lock
/// made.
///
/// Mutation: a lock that is not passed on.
#[test]
fn the_lock_of_the_screen_pauses_and_the_unlock_resumes() {
    let mut r = rig();
    assert!(r.start());
    r.host.session_locked(true);
    assert_eq!(r.host.hud().dot, Dot::Paused);
    assert_eq!(r.host.hud().status, Phrase::new("azreview-rec-status-paused-locked"));
    r.host.session_locked(false);
    assert_eq!(r.host.hud().dot, Dot::Recording);
}
