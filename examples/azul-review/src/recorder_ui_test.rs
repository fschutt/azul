//! The recorder's window as a DOM: the devices are in it exactly when the recorder asks for them, the buttons are
//! the ones the state needs, and what the recorder says is in it. Needs azul, no window.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use azul::dom::NodeType;
use azul_appkit::phrase::Arg;

use super::*;
use crate::{
    recorder::Deps,
    recorder_clock::{Clock, MockClock},
    recorder_codec::{MockFactory, PixelFormat, RawFrame},
    recorder_export_job::{video_path, Failure, Made, Renderer},
    recorder_host::Facts,
    recorder_manifest::{AppInfo, HostInfo, Workspace},
    recorder_settings::RecorderSettings,
};

struct Tmp(PathBuf);

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn with(app: &mut RefAny, f: &mut dyn FnMut(&mut RecorderApp)) -> bool {
    match app.downcast_mut::<RecorderApp>() {
        Some(mut r) => {
            f(&mut r);
            true
        }
        None => false,
    }
}

fn rig(settings: RecorderSettings) -> (Host, Tmp) {
    let (app, tmp) = app_rig(settings);
    (Host { app: RefAny::new(app), with }, tmp)
}

fn app_rig(settings: RecorderSettings) -> (RecorderApp, Tmp) {
    let (app, tmp, _clock) = app_rig_with_clock(settings);
    (app, tmp)
}

fn app_rig_with_clock(settings: RecorderSettings) -> (RecorderApp, Tmp, MockClock) {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!("azreview-ui-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let clock = MockClock::at(0);
    let moved = clock.clone();
    let deps = Deps {
        unix_ms: Box::new(|| 1_764_108_000_000),
        make_clock: Box::new(move |_| Arc::new(clock.clone()) as Arc<dyn Clock>),
        free_bytes: Box::new(|_| Some(500_000_000_000)),
        factory: Box::new(MockFactory::default()),
    };
    let facts = Facts {
        app: AppInfo { name: String::from("AzReview"), version: String::from("0.1.0") },
        workspace: Workspace { root: String::from("/w"), name: String::from("w") },
        host: HostInfo { os: String::from("macos"), arch: String::from("aarch64") },
        utc_offset_minutes: 60,
        recordings_dir: dir.join("recordings"),
        who: Arg::from("Terminal"),
        decimal: '.',
        hotkey: String::from("Cmd+Shift+P"),
    };
    (RecorderApp::new(RecorderHost::new(settings, facts, deps)), Tmp(dir), moved)
}

fn quiet() -> RecorderSettings {
    RecorderSettings { idle_pause_us: None, ..RecorderSettings::default() }
}

const ALL: Accesses = Accesses { screen: Access::Allowed, camera: Access::Allowed, microphone: Access::Allowed };
const DISPLAY: DisplayFacts = DisplayFacts { id: 1, width: 2560, height: 1664 };

fn start(host: &Host) -> bool {
    host.run(|app| app.host.start(ALL, Devices::default(), DISPLAY, &[DISPLAY])).expect("an app")
}

fn nodes(dom: &Dom) -> Vec<&Dom> {
    let mut out = vec![dom];
    for child in dom.children.as_slice() {
        out.extend(nodes(child));
    }
    out
}

fn has_id(dom: &Dom, id: &AzString) -> bool {
    nodes(dom).into_iter().any(|n| n.root.has_id(id.clone()))
}

fn texts(dom: &Dom) -> Vec<String> {
    nodes(dom).into_iter().filter_map(|n| n.root.node_type.get_text().into_option()).map(|t| t.as_str().to_string()).filter(|t| !t.trim().is_empty()).collect()
}

fn buttons(dom: &Dom) -> usize {
    nodes(dom).into_iter().filter(|n| matches!(n.root.node_type, NodeType::Button)).count()
}

/// With nothing recording no device is in the window: the screen, the camera and the microphone are asked for
/// while a recording runs, and the window starts with the one button that starts one (and the one that shows the
/// camera).
///
/// Mutation: a device mounted while idle; a window without the start button.
#[test]
fn nothing_is_open_in_an_idle_window_and_it_says_so_and_offers_to_start() {
    crate::l10n::in_english();
    let (host, _tmp) = rig(quiet());
    let dom = hud_dom(&host, None);
    for id in [&ids::REC_SCREEN, &ids::REC_CAMERA, &ids::REC_MICROPHONE, &ids::REC_LEVEL] {
        assert!(!has_id(&dom, id), "{id:?}");
    }
    assert_eq!(buttons(&dom), 2, "start, and the camera");
    let said = texts(&dom);
    assert!(said.iter().any(|t| t == "azreview-rec-status-idle"), "{said:?}");
    assert!(said.iter().any(|t| t == "azreview-rec-will-display"), "what will be recorded is said before the first press: {said:?}");
}

/// A recording has its devices in the window, the level and the buttons that pause and stop it; a pause lets
/// the devices go (the window is drawn again without them).
///
/// Mutation: a device missing from a recording; a device left in a pause.
#[test]
fn a_recording_has_its_devices_in_the_window_and_a_pause_takes_them_out() {
    crate::l10n::in_english();
    let (host, _tmp) = rig(quiet());
    assert!(start(&host));
    let dom = hud_dom(&host, None);
    for id in [&ids::REC_SCREEN, &ids::REC_CAMERA, &ids::REC_MICROPHONE, &ids::REC_LEVEL] {
        assert!(has_id(&dom, id), "{id:?}");
    }
    assert_eq!(buttons(&dom), 3, "pause, stop, and the camera");
    assert!(texts(&dom).iter().any(|t| t == "azreview-rec-status-recording"));
    host.run(|app| app.host.toggle_pause());
    let dom = hud_dom(&host, None);
    for id in [&ids::REC_SCREEN, &ids::REC_CAMERA, &ids::REC_MICROPHONE] {
        assert!(!has_id(&dom, id), "paused: {id:?} is let go");
    }
    assert!(texts(&dom).iter().any(|t| t == "azreview-rec-status-paused-user"));
    assert_eq!(buttons(&dom), 3, "resume, stop, and the camera");
}

/// The camera's preview is the camera alone, and a camera the system refused is not in the window.
///
/// Mutation: a preview that brings the microphone or the screen; a refused camera shown.
#[test]
fn the_preview_brings_the_camera_alone_and_a_refused_camera_is_not_in_the_window() {
    crate::l10n::in_english();
    let (host, _tmp) = rig(quiet());
    host.run(|app| app.host.set_preview(true, Access::Allowed));
    let dom = hud_dom(&host, None);
    assert!(has_id(&dom, &ids::REC_CAMERA));
    assert!(!has_id(&dom, &ids::REC_SCREEN) && !has_id(&dom, &ids::REC_MICROPHONE));
    host.run(|app| app.host.set_preview(true, Access::Denied));
    assert!(!has_id(&hud_dom(&host, None), &ids::REC_CAMERA));
}

/// What the recorder says is in the window: a start that failed, a track that is off, the pause key that is not
/// available.
///
/// Mutation: an error that is not drawn; a note about the pause key that is dropped.
#[test]
fn what_the_recorder_says_is_in_the_window() {
    crate::l10n::in_english();
    let (host, _tmp) = rig(RecorderSettings { screen_source: crate::recorder_settings::ScreenKind::Window, ..quiet() });
    assert!(!start(&host));
    let dom = hud_dom(&host, Some(String::from("the pause key is gone")));
    let said = texts(&dom);
    assert!(said.iter().any(|t| t == "azreview-rec-error-window-source"), "{said:?}");
    assert!(said.iter().any(|t| t == "the pause key is gone"), "{said:?}");
    // The window's own flags: floats over the others and is out of the screen's recording.
    let options = window(layout_for_test);
    assert!(options.window_state.flags.is_top_level && options.window_state.flags.exclude_from_capture);
}

extern "C" fn layout_for_test(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    Dom::create_body()
}

/// The review tells the recording which file it is on: opening a file while a recording goes is an event of the
/// recording (what the export's chapters are made of), and nothing is written when nothing records. The window's
/// own call (`AppState::open_file`) reaches the recording through the hook the host gives the state.
///
/// Mutation: a state that is not given the host's hook; an event written when nothing records.
#[test]
fn opening_a_file_while_recording_is_an_event_of_the_recording() {
    let file = |name: &str| crate::code::SourceFile { path: PathBuf::from(name), display: name.to_string(), lines: vec![String::from("fn a() {}"), String::from("fn b() {}")] };
    let mut state = crate::AppState::create(PathBuf::new(), vec![file("a.rs"), file("src/b.rs")], RefAny::new(0u8), PathBuf::new());
    let (app, _tmp) = app_rig(quiet());
    state.recorder = Arc::new(app.host.hook());
    state.recording = Some(app);
    // Nothing records: the file opens and nothing is written anywhere.
    state.open_file(0);
    assert!(state.recording.as_ref().unwrap().host.current().is_none());
    // A recording goes: the next file is in its events.
    assert!(state.recording.as_mut().unwrap().host.start(ALL, Devices::default(), DISPLAY, &[DISPLAY]));
    state.open_file(1);
    let name = state.recording.as_ref().unwrap().host.current().expect("a recording is going");
    let dir = state.recording.as_ref().unwrap().host.path_of(&name);
    state.recording.as_mut().unwrap().host.stop();
    let dir = dir.unwrap_or_else(|| panic!("the folder of {name}"));
    let events = fs::read_to_string(dir.join("events.jsonl")).unwrap();
    let line = events.lines().find(|l| l.contains("\"nav.file\"")).unwrap_or_else(|| panic!("a nav.file event in {events}"));
    assert!(line.contains("\"file\":\"src/b.rs\"") && line.contains("\"lines\":2"), "{line}");
    assert_eq!(events.matches("\"nav.file\"").count(), 1, "the file opened before the recording is not in it");
}

/// A renderer that makes a video when the test lets it, and stops when it is told to.
struct Waits(Arc<AtomicBool>);

impl Renderer for Waits {
    fn render(&mut self, session: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(f64)) -> Result<Made, Failure> {
        progress(0.5);
        while !self.0.load(Ordering::SeqCst) {
            if cancel.load(Ordering::SeqCst) {
                return Err(Failure::Cancelled);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let file = video_path(session);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, vec![0_u8; 3_000_000]).unwrap();
        Ok(Made { file, bytes: 3_000_000, notes: Vec::new() })
    }
}

fn until_texts(host: &Host, what: &str, want: impl Fn(&[String]) -> bool) -> Dom {
    let from = Instant::now();
    loop {
        let dom = hud_dom(host, None);
        if want(&texts(&dom)) {
            return dom;
        }
        assert!(from.elapsed() < Duration::from_secs(5), "{what}: {:?}", texts(&dom));
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// A recording of the list has the buttons the panel decides, and its export is in the window in words: not offered
/// while the recording has no video, a cancel and how far it is while it is made, then the video named and offered.
///
/// Mutation: a row without its buttons; an export line that is not drawn; the delete button left while it is exported.
#[test]
fn a_row_of_the_list_has_its_buttons_and_says_how_its_export_stands() {
    crate::l10n::in_english();
    let (app, _tmp, clock) = app_rig_with_clock(quiet());
    let (host, release) = (Host { app: RefAny::new(app), with }, Arc::new(AtomicBool::new(false)));
    let r = Arc::clone(&release);
    host.run(|app| app.host.set_renderer(Box::new(move || Box::new(Waits(Arc::clone(&r))))));
    // A finished recording with a screen in it.
    host.run(|app| {
        assert!(app.host.start(ALL, Devices::default(), DISPLAY, &[DISPLAY]));
        for i in 0..20_u8 {
            clock.advance_ms(100);
            app.host.screen_frame(RawFrame { width: 1920, height: 1248, format: PixelFormat::Rgba8, bytes: vec![i; 1920 * 1248 * 4] }, 1.0);
            app.host.tick();
        }
        app.host.stop();
    })
    .expect("an app");
    let name = host.run(|app| app.host.rows()[0].name.clone()).unwrap();

    // Idle: start and the camera, and the row's export, show folder and delete.
    let dom = hud_dom(&host, None);
    assert_eq!(buttons(&dom), 5, "start, the camera, and the row's export, show folder, delete");
    assert!(!texts(&dom).iter().any(|t| t.contains("rec-export")), "nothing exported: nothing said of it: {:?}", texts(&dom));

    // Made: the cancel replaces the export and the delete goes, and it says how far it is.
    assert_eq!(host.run(|app| app.host.export(&name)), Some(true));
    let dom = until_texts(&host, "the export reports", |t| t.iter().any(|t| t == "azreview-rec-export-running"));
    assert_eq!(buttons(&dom), 4, "start, the camera, cancel the export, show folder");

    // Done: the video is named, and can be opened, and made again.
    release.store(true, Ordering::SeqCst);
    let dom = until_texts(&host, "the video is made", |t| t.iter().any(|t| t == "azreview-rec-export-done"));
    assert_eq!(buttons(&dom), 6, "start, the camera, export again, open the video, show folder, delete");
}

