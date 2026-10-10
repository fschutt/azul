//! AzReview - ink-first code review: the files of a folder as paper pages,
//! annotated with a pen (or the mouse), with voice clips.
//!
//! The window is the document shell: the files in the navigation pane, the
//! toolbar, the page rail and the sheets in the document, the status bar
//! under it; the app theme and the dark mode from `ShellThemeScope` (the
//! pages stay paper). azul-appkit gives the switches (`AzReview [FOLDER]`,
//! `--theme`, `--mode`, `--size`, `--shot`, `--data-dir`), the settings page
//! (Mod+,) and About. Every session is an archive in the data tree,
//! `review/<file>.azreview.zip`, written on a Thread (one writer).
//!
//! stdout, for scripts (`scripts/azreview_e2e.py`): `AZREVIEW_FILE <path>`
//! when a file is opened, `AZREVIEW_STROKES <n>` when a stroke is kept,
//! `AZREVIEW_SAVED <key>` / `AZREVIEW_SAVE_ERROR <why>` per write.

use std::path::PathBuf;

use azul::{dom::VirtualKeyCode, prelude::*, task::TerminateTimer, time::SystemTimeDiff};
use azul_appkit::{
    about::AboutInfo,
    args::{AppArgs, AppSpec},
    data::app_key,
    files::{FileJob, FileOutcome},
    shortcuts::Shortcut,
    ui as kit,
};

pub mod code;
pub mod ids;
pub mod ink;
pub mod model;
pub mod session;
pub mod ui;

use model::{Finding, Semantic, Stroke, Tool, VoiceClip};

/// What azul-appkit's switches know about AzReview.
pub const SPEC: AppSpec = AppSpec {
    name: "AzReview",
    binary: "AzReview",
    summary: "ink-first code review",
    screens: &["review"],
    files_help: "the folder to review (default: the current folder)",
};

/// The About facts.
pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzReview",
    version: env!("CARGO_PKG_VERSION"),
    summary: "Ink-first code review: the files of a folder as pages, marked with a pen, with \
              voice clips. Sessions are kept in your data folder.",
    license: "MIT",
    app_folder: "review",
};

/// The keys AzReview answers (the kit adds Mod+, / F1 / Escape).
pub const SHORTCUTS: [Shortcut; 3] = [
    Shortcut::new("Review", "Mod+S", "Save the session now"),
    Shortcut::new("Review", "1 to 9", "Pick the ink's meaning"),
    Shortcut::new("Review", "Right click", "The previous nib"),
];

/// The write-back tag of a session write.
const TAG_SAVE: u64 = 1;

pub struct AppState {
    pub files: Vec<code::SourceFile>,
    pub current: Option<usize>,
    pub strokes: Vec<Stroke>,
    pub live: Option<Stroke>,
    pub next_stroke_id: u64,
    pub active: Semantic,
    pub tool: Tool,
    pub findings: Vec<Finding>,
    pub recording: Option<VoiceClip>,
    pub clips: Vec<VoiceClip>,
    pub level_samples: usize,
    pub visible_page: usize,
    pub epoch: u64,
    pub idle_timer: TimerId,
    pub root: PathBuf,
    pub status: String,
    pub last_pad_keys: u32,
    /// azul-appkit's kit and the data root the session archives go to.
    pub kit: RefAny,
    pub data_root: PathBuf,
    /// A session write is in flight; another one waits for it (one writer).
    pub saving: bool,
    pub save_pending: bool,
}

impl AppState {
    pub(crate) fn file(&self) -> Option<&code::SourceFile> {
        self.current.and_then(|i| self.files.get(i))
    }

    fn rederive(&mut self) {
        let Some(file) = self.file().cloned() else {
            self.findings.clear();
            return;
        };
        self.findings = derive_findings(&self.strokes, &file, &self.recording);
    }
}

fn derive_findings(
    strokes: &[Stroke],
    file: &code::SourceFile,
    recording: &Option<VoiceClip>,
) -> Vec<Finding> {
    let mut out: Vec<Finding> = Vec::new();
    for s in strokes {
        let (_, y0, _, y1) = s.bounds();
        let (first_line, _) = file.page(s.page);
        let l0 = first_line + (y0 / ui::LINE_H).floor().max(0.0) as usize;
        let l1 = first_line + (y1 / ui::LINE_H).floor().max(0.0) as usize;

        let merged = out.iter_mut().find(|f| {
            f.epoch == s.epoch
                && f.semantic == s.semantic
                && l0 <= f.last_line.saturating_add(2)
                && l1.saturating_add(2) >= f.first_line
        });
        if let Some(f) = merged {
            f.first_line = f.first_line.min(l0);
            f.last_line = f.last_line.max(l1);
            f.stroke_count += 1;
            continue;
        }
        let voice = recording
            .as_ref()
            .filter(|c| c.stroke_ids.contains(&s.id))
            .map(|c| format!("{} samples of spoken rationale", c.samples.len()));
        out.push(Finding {
            semantic: s.semantic,
            file: file.display.clone(),
            first_line: l0,
            last_line: l1,
            voice_note: voice,
            stroke_count: 1,
            epoch: s.epoch,
        });
    }
    out.sort_by_key(|f| f.first_line);
    out
}

pub fn run() {
    let args = match AppArgs::from_env(&SPEC) {
        Ok(a) => a,
        Err(why) => {
            eprintln!("{why}");
            std::process::exit(2);
        }
    };
    let root = args
        .files
        .first()
        .cloned()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let kit_ref = kit::create_kit(SPEC, ABOUT, &SHORTCUTS, &[], args);
    let data_root = {
        let mut k = kit_ref.clone();
        k.downcast_ref::<kit::Kit>()
            .map(|k| k.data_root.clone())
            .unwrap_or_default()
    };
    let files = code::load_tree(&root, 400);
    let status = if files.is_empty() {
        format!("no reviewable files under {}", root.display())
    } else {
        format!("{} files \u{2014} pick one to start", files.len())
    };
    if let Some(first) = files.first() {
        println!("AZREVIEW_FILE {}", first.display);
    }
    let state = AppState {
        current: if files.is_empty() { None } else { Some(0) },
        files,
        strokes: Vec::new(),
        live: None,
        next_stroke_id: 1,
        active: Semantic::Scope,
        tool: Tool::Marker,
        findings: Vec::new(),
        recording: None,
        clips: Vec::new(),
        level_samples: 0,
        visible_page: 0,
        epoch: 0,
        idle_timer: TimerId::unique(),
        root,
        status,
        last_pad_keys: 0,
        kit: kit_ref.clone(),
        data_root,
        saving: false,
        save_pending: false,
    };
    let app = App::create(RefAny::new(state), kit::app_config(&kit_ref));
    let window =
        kit::window_options(&kit_ref, ui::layout, (1280.0, 820.0), (800.0, 520.0), on_window_created);
    app.run(window);
}

/// The kit's handle, out of the app's state.
fn kit_of(data: &mut RefAny) -> Option<RefAny> {
    data.downcast_ref::<AppState>().map(|s| s.kit.clone())
}

/// The window exists: azul-appkit's `--shot` timer.
extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    if let Some(kit_ref) = kit_of(&mut data) {
        kit::on_window_created(&kit_ref, &mut info);
    }
    Update::DoNothing
}

// ==== The session archive: written into the data tree on a Thread ====

/// Writes the session as it is now (`review/<file>.azreview.zip`), on a
/// Thread; a write asked for while one is in flight runs after it.
pub(crate) fn save_session(data: &RefAny, info: &mut CallbackInfo) {
    let mut handle = data.clone();
    let job = {
        let Some(mut s) = handle.downcast_mut::<AppState>() else {
            return;
        };
        if s.saving {
            s.save_pending = true;
            return;
        }
        s.saving = true;
        s.save_pending = false;
        let (name, bytes) = session::archive(&s);
        (
            s.data_root.clone(),
            FileJob::Put {
                key: app_key(ABOUT.app_folder, &name),
                bytes,
            },
        )
    };
    kit::spawn_file_jobs(info, &job.0, vec![job.1], data.clone(), TAG_SAVE, on_saved);
}

extern "C" fn on_saved(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(reply) = kit::take_reply(&mut msg) else {
        return Update::DoNothing;
    };
    let again = {
        let Some(mut s) = app.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        s.saving = false;
        for outcome in &reply.outcomes {
            match (outcome.error(), outcome) {
                (None, FileOutcome::Put { key, .. }) => {
                    println!("AZREVIEW_SAVED {key}");
                    s.status = format!("saved to {key}");
                }
                (None, _) => {}
                (Some(why), _) => {
                    println!("AZREVIEW_SAVE_ERROR {why}");
                    s.status = format!("save FAILED: {why}");
                }
            }
        }
        s.save_pending
    };
    if again {
        save_session(&app, &mut info);
    }
    Update::RefreshDom
}

/// The keys 1 to 9, in the order of [`Semantic::ALL`].
const DIGITS: [VirtualKeyCode; 9] = [
    VirtualKeyCode::Key1,
    VirtualKeyCode::Key2,
    VirtualKeyCode::Key3,
    VirtualKeyCode::Key4,
    VirtualKeyCode::Key5,
    VirtualKeyCode::Key6,
    VirtualKeyCode::Key7,
    VirtualKeyCode::Key8,
    VirtualKeyCode::Key9,
];

/// The kit's keys first (Mod+, settings, F1 shortcuts, Escape closes them);
/// then Mod+S saves and 1 to 9 pick the ink's meaning.
pub extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(kit_ref) = kit_of(&mut data) else {
        return Update::DoNothing;
    };
    if let Some(update) = kit::handle_key(&kit_ref, &mut info) {
        return update;
    }
    if kit::settings_open(&kit_ref) {
        return Update::DoNothing;
    }
    let Some(key) = info.get_current_keyboard_state().current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    if matches!(key, VirtualKeyCode::S) && info.get_key_modifiers().primary_down() {
        info.prevent_default();
        save_session(&data, &mut info);
        return Update::RefreshDom;
    }
    let Some(index) = DIGITS.iter().position(|d| *d == key) else {
        return Update::DoNothing;
    };
    let Some(&sem) = Semantic::ALL.get(index) else {
        return Update::DoNothing;
    };
    if let Some(mut s) = data.downcast_mut::<AppState>() {
        s.active = sem;
        s.status = format!("{} - {}", s.tool.label(), sem.label());
    }
    Update::RefreshDom
}

/// The toolbar's Save button.
pub extern "C" fn on_save_button(data: RefAny, mut info: CallbackInfo) -> Update {
    save_session(&data, &mut info);
    Update::RefreshDom
}

/// The toolbar's gear: azul-appkit's settings page.
pub extern "C" fn on_settings_open(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(kit_ref) = kit_of(&mut data) {
        kit::open_settings(&kit_ref, None);
    }
    Update::RefreshDom
}

pub extern "C" fn on_ink_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let ms = info.get_current_mouse_state();
    if ms.right_down || ms.middle_down {
        return Update::DoNothing;
    }
    let Some(page) = ui::page_of(&mut info) else {
        return Update::DoNothing;
    };
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let Some((p, is_eraser)) = ui::sample(&mut info) else {
        return Update::DoNothing;
    };
    if is_eraser {
        let hit = s.strokes.iter().rposition(|st| {
            let (x0, y0, x1, y1) = st.bounds();
            st.page == page
                && p.x >= x0 - 6.0
                && p.x <= x1 + 6.0
                && p.y >= y0 - 6.0
                && p.y <= y1 + 6.0
        });
        if let Some(i) = hit {
            s.strokes.remove(i);
            s.rederive();
            return Update::RefreshDom;
        }
        return Update::DoNothing;
    }
    let id = s.next_stroke_id;
    s.next_stroke_id += 1;
    let semantic = s.tool.semantic_for(s.active);
    let epoch = s.epoch;
    s.live = Some(Stroke {
        page,
        semantic,
        points: vec![p],
        id,
        epoch,
    });

    if s.tool.records_audio() && s.recording.is_none() {
        s.recording = Some(VoiceClip {
            sample_rate: 48_000,
            ..VoiceClip::default()
        });
    }
    if let Some(clip) = s.recording.as_mut() {
        clip.stroke_ids.push(id);
    }
    Update::DoNothing
}

pub extern "C" fn on_cycle_tool(mut data: RefAny, _: CallbackInfo) -> Update {
    cycle_tool(&mut data, false)
}

pub extern "C" fn on_cycle_tool_back(mut data: RefAny, _: CallbackInfo) -> Update {
    if std::env::var("AZ_REVIEW_DEBUG").is_ok() {
        eprintln!("[review] on_cycle_tool_back FIRED");
    }
    cycle_tool(&mut data, true)
}

fn cycle_tool(data: &mut RefAny, backward: bool) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.tool = if backward {
        s.tool.prev()
    } else {
        s.tool.next()
    };
    if !s.tool.records_audio() {
        if let Some(clip) = s.recording.take() {
            s.clips.push(clip);
        }
    }
    s.status = format!("{} - {}", s.tool.label(), s.active.label());
    Update::RefreshDom
}

pub extern "C" fn on_ink_move(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    if s.live.is_none() {
        return Update::DoNothing;
    }
    let Some((p, _)) = ui::sample(&mut info) else {
        return Update::DoNothing;
    };
    if let Some(live) = s.live.as_mut() {
        let far = live
            .points
            .last()
            .is_none_or(|l| (l.x - p.x).abs() + (l.y - p.y).abs() > 0.35);
        if far {
            live.points.push(p);
        }
    }
    Update::RefreshDom
}

pub extern "C" fn on_ink_up(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let (is_click, timer_id) = {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        let Some(done) = s.live.take() else {
            return Update::DoNothing;
        };
        if done.points.len() < CLICK_POINT_LIMIT {
            (true, s.idle_timer)
        } else {
            s.strokes.push(done);
            s.rederive();
            println!("AZREVIEW_STROKES {}", s.strokes.len());
            (false, s.idle_timer)
        }
    };
    if is_click {
        return on_cycle_tool(data, info);
    }
    save_session(&data, &mut info);
    arm_idle_timer(&mut info, data, timer_id);
    Update::RefreshDom
}

const ANNOTATION_IDLE_MS: u64 = 1_800;

fn arm_idle_timer(info: &mut CallbackInfo, data: RefAny, id: TimerId) {
    info.remove_timer(id);
    let timer = Timer::create(
        data,
        on_annotation_idle,
        info.get_system_time_fn(),
    )
    .with_delay(Duration::System(SystemTimeDiff::from_millis(
        ANNOTATION_IDLE_MS,
    )));
    info.add_timer(id, timer);
}

pub extern "C" fn on_annotation_idle(
    mut data: RefAny,
    mut info: TimerCallbackInfo,
) -> TimerCallbackReturn {
    {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return TimerCallbackReturn {
                should_update: Update::DoNothing,
                should_terminate: TerminateTimer::Terminate,
            };
        };
        s.epoch += 1;
        if let Some(clip) = s.recording.take() {
            s.clips.push(clip);
            s.level_samples = 0;
        }
        s.status = format!("annotation {} sealed", s.epoch);
    }
    save_session(&data, &mut info.callback_info);
    TimerCallbackReturn {
        should_update: Update::RefreshDom,
        should_terminate: TerminateTimer::Terminate,
    }
}

const CLICK_POINT_LIMIT: usize = 3;

pub extern "C" fn on_pad(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(pad) = info.get_tablet_pad().into_option() else {
        return Update::DoNothing;
    };
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let pressed = pad.express_keys & !s.last_pad_keys;
    s.last_pad_keys = pad.express_keys;
    if pressed == 0 {
        return Update::DoNothing;
    }
    let index = pressed.trailing_zeros() as usize;
    if let Some(&sem) = Semantic::ALL.get(index) {
        s.active = sem;
        s.status = format!("pad key {index} -> {}", sem.label());
        return Update::RefreshDom;
    }
    Update::DoNothing
}

pub extern "C" fn on_jump_to_page(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(page) = ui::index_of(&mut info) else {
        return Update::DoNothing;
    };
    ui::scroll_to_page(&mut info, page);
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.visible_page = page;
    Update::RefreshDom
}

pub extern "C" fn on_menu_semantic(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(tag) = data.downcast_ref::<ui::IndexTag>().map(|t| t.index) else {
        return Update::DoNothing;
    };
    let Some(&sem) = Semantic::ALL.get(tag) else {
        return Update::DoNothing;
    };
    let Some(mut app) = info.get_dataset(info.get_hit_node()).into_option() else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.active = sem;
    Update::RefreshDom
}

pub extern "C" fn on_menu_save(data: RefAny, mut info: CallbackInfo) -> Update {
    save_session(&data, &mut info);
    Update::RefreshDom
}

pub extern "C" fn on_menu_reveal(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    // The sessions' folder in the data tree (it exists after the first save), opened by the
    // apps' one opener; what went wrong (no folder yet) is said in the status line.
    let dir = s.data_root.join(ABOUT.app_folder);
    s.status = match azul_appkit::files::open_external(&dir.to_string_lossy()) {
        Ok(()) => format!("archives in {}", dir.display()),
        Err(e) => e,
    };
    Update::RefreshDom
}

pub extern "C" fn on_pick_semantic(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(index) = ui::index_of(&mut info) else {
        return Update::DoNothing;
    };
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    if let Some(&sem) = Semantic::ALL.get(index) {
        s.active = sem;
    }
    Update::RefreshDom
}

pub extern "C" fn on_pick_file(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(index) = ui::index_of(&mut info) else {
        return Update::DoNothing;
    };
    let switch = data
        .downcast_ref::<AppState>()
        .is_some_and(|s| index < s.files.len() && s.current != Some(index));
    if !switch {
        return Update::DoNothing;
    }
    // The file being left is kept first (its archive is built now).
    save_session(&data, &mut info);
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    println!("AZREVIEW_FILE {}", s.files[index].display);
    s.current = Some(index);
    s.strokes.clear();
    s.live = None;
    s.rederive();
    s.status = s.files[index].display.clone();
    Update::RefreshDom
}

pub extern "C" fn on_toggle_record(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let stopped = {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        match s.recording.take() {
            Some(clip) => {
                s.status = format!("recording stopped - {} samples kept", clip.samples.len());
                s.clips.push(clip);
                s.level_samples = 0;
                true
            }
            None => false,
        }
    };
    if stopped {
        save_session(&data, &mut info);
        return Update::RefreshDom;
    }
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.recording = Some(VoiceClip {
        sample_rate: 48_000,
        ..VoiceClip::default()
    });
    s.status = "recording - strokes drawn now carry this audio".to_string();
    Update::RefreshDom
}

pub extern "C" fn on_audio_frame(
    mut data: RefAny,
    _: CallbackInfo,
    frame: azul::audio::AudioFrame,
) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let Some(clip) = s.recording.as_mut() else {
        return Update::DoNothing;
    };
    clip.samples.extend(frame.samples.as_ref().iter().copied());
    let total = clip.samples.len();

    let step = ui::METER_PACKET_SAMPLES;
    if total / step > s.level_samples / step {
        s.level_samples = total;
        return Update::RefreshDom;
    }
    s.level_samples = total;
    Update::DoNothing
}
