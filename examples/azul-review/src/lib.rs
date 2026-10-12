//! AzReview - ink-first code review: the files of a folder as paper pages,
//! annotated with a pen (or the mouse), with voice clips.
//!
//! The window has no title bar: its top row moves it and holds the files'
//! switch, the five meanings and the nib; the page rail (the page numbers,
//! record, save, settings) and the sheets are under it, and the files fly out
//! over the sheets. The app theme and the dark mode come from
//! `ShellThemeScope` (the pages stay paper). azul-appkit gives the switches (`AzReview [FOLDER]`,
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
    l10n::{t_args, Arg},
    shortcuts::Shortcut,
    ui as kit,
};

pub mod code;
pub mod ids;
pub mod ink;
#[cfg(test)]
mod ink_tests;
mod l10n;
#[cfg(test)]
mod l10n_tests;
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
    Shortcut::new("azreview-shortcuts-review", "Mod+S", "azreview-shortcut-save"),
    Shortcut::new("azreview-shortcuts-review", "1 to 9", "azreview-shortcut-meaning"),
    Shortcut::new("azreview-shortcuts-review", "Right click", "azreview-shortcut-previous-nib"),
];

/// The write-back tag of a session write.
const TAG_SAVE: u64 = 1;

pub struct AppState {
    pub files: Vec<code::SourceFile>,
    pub current: Option<usize>,
    pub strokes: Vec<Stroke>,
    pub live: Option<Stroke>,
    /// The revisions of each page's kept strokes: an ink layer draws its
    /// frame anew when its page's changes (`ink::InkRevisions`).
    pub ink: ink::InkRevisions,
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
    /// The files fly out over the pages (the title row's switch; a picked
    /// file and Escape fold them away).
    pub sidebar_open: bool,
    /// What went wrong and is told over the pages until dismissed: a session
    /// not written, an archive folder that would not open.
    pub problem: Option<String>,
    pub last_pad_keys: u32,
    /// azul-appkit's kit and the data root the session archives go to.
    pub kit: RefAny,
    pub data_root: PathBuf,
    /// A session write is in flight; another one waits for it (one writer).
    pub saving: bool,
    pub save_pending: bool,
}

impl AppState {
    /// AzReview on the `files` of `root`, the first open and the files out
    /// (they are what one picks from first); `kit` and `data_root` are
    /// azul-appkit's.
    #[must_use]
    pub fn create(root: PathBuf, files: Vec<code::SourceFile>, kit: RefAny, data_root: PathBuf) -> Self {
        Self {
            current: if files.is_empty() { None } else { Some(0) },
            files,
            strokes: Vec::new(),
            live: None,
            ink: ink::InkRevisions::default(),
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
            sidebar_open: true,
            problem: None,
            last_pad_keys: 0,
            kit,
            data_root,
            saving: false,
            save_pending: false,
        }
    }

    /// Opens file `index` (its pages start without ink) and folds the files
    /// away, out of the pages' way.
    pub fn open_file(&mut self, index: usize) {
        if index >= self.files.len() {
            return;
        }
        self.current = Some(index);
        self.strokes.clear();
        self.live = None;
        self.ink.open();
        self.visible_page = 0;
        self.sidebar_open = false;
        self.rederive();
    }

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
            .map(|c| t_args("azreview-voice-note", &[("count", Arg::from(c.samples.len()))]));
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
    // The words first: everything from here is said in the language chosen (Settings,
    // `--language`; the system's otherwise).
    let mut config = kit::app_config(&kit_ref);
    crate::l10n::register(&mut config);
    let mut kit_now = kit_ref.clone();
    let language = kit_now.downcast_ref::<kit::Kit>().map(|k| k.language());
    if let Some(language) = language.filter(|l| *l != azul_appkit::args::LanguagePref::System) {
        azul_appkit::l10n::set_locale(language.tag());
    }
    let data_root = {
        let mut k = kit_ref.clone();
        k.downcast_ref::<kit::Kit>()
            .map(|k| k.data_root.clone())
            .unwrap_or_default()
    };
    let files = code::load_tree(&root, 400);
    if let Some(first) = files.first() {
        println!("AZREVIEW_FILE {}", first.display);
    }
    let state = AppState::create(root, files, kit_ref.clone(), data_root);
    let app = App::create(RefAny::new(state), config);
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
                }
                (None, _) => {}
                (Some(why), _) => {
                    println!("AZREVIEW_SAVE_ERROR {why}");
                    s.problem =
                        Some(t_args("azreview-save-failed", &[("why", Arg::from(why.to_string()))]));
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
    if matches!(key, VirtualKeyCode::Escape) {
        let folded = data.downcast_mut::<AppState>().is_some_and(|mut s| {
            let was = s.sidebar_open;
            s.sidebar_open = false;
            was
        });
        return if folded { Update::RefreshDom } else { Update::DoNothing };
    }
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
    }
    Update::RefreshDom
}

/// The rail's Save button.
pub extern "C" fn on_save_button(data: RefAny, mut info: CallbackInfo) -> Update {
    save_session(&data, &mut info);
    Update::RefreshDom
}

/// The title row's switch: the files fly out over the pages, or fold away.
pub extern "C" fn on_toggle_sidebar(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.sidebar_open = !s.sidebar_open;
    Update::RefreshDom
}

/// Dismiss on the bar that told what went wrong.
pub extern "C" fn on_dismiss_problem(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.problem = None;
    Update::RefreshDom
}

/// The rail's gear: azul-appkit's settings page.
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
            let gone = s.strokes.remove(i);
            s.ink.touch(page);
            s.rederive();
            drop(s);
            ui::redraw_ink(&mut info, ink::Layer::of(gone.semantic));
            return Update::DoNothing;
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
    drop(s);
    // The first dab shows at once; the stroke grows in the frame of its layer.
    ui::redraw_ink(&mut info, ink::Layer::of(semantic));
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
    Update::RefreshDom
}

/// A pointer move while a stroke is drawn: its new point, and a new frame of
/// its layer - no new DOM (a DOM per move made drawing unusably slow).
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
    let Some(live) = s.live.as_mut() else {
        return Update::DoNothing;
    };
    let far = live
        .points
        .last()
        .is_none_or(|l| (l.x - p.x).abs() + (l.y - p.y).abs() > 0.35);
    if !far {
        return Update::DoNothing;
    }
    live.points.push(p);
    let layer = ink::Layer::of(live.semantic);
    drop(s);
    ui::redraw_ink(&mut info, layer);
    Update::DoNothing
}

/// The pen lifts: a stroke is kept (its page's frame drawn anew with it, no
/// new DOM - nothing else in the window shows the strokes), a click cycles
/// the nib.
pub extern "C" fn on_ink_up(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let (is_click, timer_id, layer) = {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        let Some(done) = s.live.take() else {
            return Update::DoNothing;
        };
        // Kept or dropped (a click cycles the nib), the page's frame no
        // longer shows a stroke being drawn.
        s.ink.touch(done.page);
        let layer = ink::Layer::of(done.semantic);
        if done.points.len() < CLICK_POINT_LIMIT {
            (true, s.idle_timer, layer)
        } else {
            s.strokes.push(done);
            s.rederive();
            println!("AZREVIEW_STROKES {}", s.strokes.len());
            (false, s.idle_timer, layer)
        }
    };
    ui::redraw_ink(&mut info, layer);
    if is_click {
        return on_cycle_tool(data, info);
    }
    save_session(&data, &mut info);
    arm_idle_timer(&mut info, data, timer_id);
    Update::DoNothing
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
        return Update::RefreshDom;
    }
    Update::DoNothing
}

/// The strip scrolled sideways (a wheel, a trackpad): the page that came
/// into view lights its number, and the numbers' row keeps it in view.
pub extern "C" fn on_strip_scroll(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(x) = ui::strip_scroll_x(&info) else {
        return Update::DoNothing;
    };
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let Some(pages) = s.file().map(code::SourceFile::page_count) else {
        return Update::DoNothing;
    };
    let page = ui::page_at(x, pages);
    if page == s.visible_page {
        return Update::DoNothing;
    }
    s.visible_page = page;
    drop(s);
    ui::reveal_pill(&mut info, page);
    Update::RefreshDom
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
    // apps' one opener; what went wrong (no folder yet) is told over the pages.
    let dir = s.data_root.join(ABOUT.app_folder);
    match azul_appkit::files::open_external(&dir.to_string_lossy()) {
        Ok(()) => Update::DoNothing,
        Err(e) => {
            s.problem = Some(e);
            Update::RefreshDom
        }
    }
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
    let Some(open) = data.downcast_ref::<AppState>().map(|s| s.current) else {
        return Update::DoNothing;
    };
    if open != Some(index) {
        // The file being left is kept first (its archive is built now).
        save_session(&data, &mut info);
    }
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    if open == Some(index) {
        s.sidebar_open = false;
        return Update::RefreshDom;
    }
    let Some(file) = s.files.get(index) else {
        return Update::DoNothing;
    };
    println!("AZREVIEW_FILE {}", file.display);
    s.open_file(index);
    Update::RefreshDom
}

pub extern "C" fn on_toggle_record(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let stopped = {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        match s.recording.take() {
            Some(clip) => {
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
