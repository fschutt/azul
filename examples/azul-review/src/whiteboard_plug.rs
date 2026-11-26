//! The whiteboard in the review window: the board of the workspace (the panel at the right of the window), kept in the data
//! folder and read back at the next start, its keys, and the jump from a cutout to the lines it was circled from.
//!
//! The panel is `whiteboard_ui`'s; this is the glue it was written to be given: what the panel reaches the app through
//! ([`host`]), how the board is read at the start and kept when it changes (Threads, never the window's own thread), what
//! the list of boards and the export and the import ask of the app, and where a double click on a cutout goes. What the
//! page does with a loop (arm it, lift it, drop it on the bar, send it with a key) is `whiteboard_page.rs`.
//!
//! The state is [`Glue`], held by the app's state in a `RefCell` (the layout builds the panel from a shared reference to the
//! app's state, and the panel's own builder wants the panel mutably); every access here goes through [`with_glue`], which
//! never panics when it is reached while it is already held: it does nothing then.

use std::{
    path::{Path, PathBuf},
    time::Instant,
};

use azul::{
    callbacks::{SegmentedOnChangeCallbackType, WriteBackCallbackType},
    dialog::{FileDialog, FileOpenResult, SaveTargetResult},
    dom::VirtualKeyCode,
    option::{OptionFileTypeList, OptionString},
    prelude::*,
    str::String as AzString,
    task::TerminateTimer,
    time::SystemTimeDiff,
    widgets::{Segmented, SegmentedState},
};
use azul_appkit::{
    l10n::{label, labels, t, t_args, Arg},
    ui as kit,
};

use crate::{
    column::nearest,
    ids,
    layout,
    ui,
    whiteboard::ItemId,
    whiteboard_app::Plan,
    whiteboard_archive,
    whiteboard_boards::BoardsError,
    whiteboard_boards_ui::BoardOutcome,
    whiteboard_drag::RegionId,
    whiteboard_export::{self, ExportRequest},
    whiteboard_git,
    whiteboard_glue::Flash,
    whiteboard_io,
    whiteboard_jump::{jump, JumpTarget},
    whiteboard_settings::{self, Settings},
    whiteboard_store::{self, StoreError},
    whiteboard_ui::{self, Host, Key, Panel},
    workspace_view::WorkspaceView,
    AppState,
};

/// How long an armed loop waits to be taken in the review window, ms, unless the setting says another time: longer than the
/// board's own default (8 s), because a person who has never done this needs the time to read the words by the loop and
/// reach for the grip or the key.
pub const ARM_MS_DEFAULT: u64 = 15_000;
/// The settings' key of the tip of the first loop.
pub const KEY_TIP_SEEN: &str = "whiteboard-tip-seen";

/// The whiteboard of the open workspace and what the app keeps beside it.
pub struct Glue {
    pub panel: Panel,
    /// The folder the app's files are in (`<data root>/review`).
    pub files: PathBuf,
    /// Whether the board is kept at all: a window with no folder has a board that is not.
    pub persist: bool,
    /// The board was read (or there was none): until then a loop is not dropped on it, the board would be replaced by what
    /// is read.
    pub started: bool,
    /// The lines a jump went to, lit on their page for a moment.
    pub flash: Option<Flash>,
    /// Whether the board has the keyboard: the last press was on its panel.
    pub keys: bool,
    /// The tip of the first loop was shown (it is a setting: once, never again).
    pub tip_shown: bool,
    /// The region whose words say the tip too.
    pub tip_for: Option<RegionId>,
    /// The page holds the pointer for a press on a loop (so that the carry over the bar is heard): it is let go of at the release.
    pub(crate) captured: bool,
    saving_to: Option<PathBuf>,
    exporting: Option<ExportRequest>,
    pub(crate) render_started: Vec<(ItemId, Instant)>,
    timer: Option<TimerId>,
}

impl Glue {
    /// The whiteboard of the workspace at `root`, empty, its files in `<data_root>/review`.
    #[must_use]
    pub fn new(root: &Path, data_root: &Path) -> Self {
        let workspace = root.to_string_lossy();
        let mut panel = Panel::new(&workspace);
        let _held = panel.wb.armed.set_lifetime_ms(ARM_MS_DEFAULT);
        Self {
            panel,
            files: data_root.join(crate::ABOUT.app_folder),
            persist: !root.as_os_str().is_empty(),
            started: false,
            flash: None,
            keys: false,
            tip_shown: false,
            tip_for: None,
            captured: false,
            saving_to: None,
            exporting: None,
            render_started: Vec::new(),
            timer: None,
        }
    }

    /// Whether anything on the board or the page is waiting on the clock (an armed loop, a press that may be held, a board
    /// that was deleted and can be taken back, lines that are lit).
    #[must_use]
    pub fn has_time_in_it(&self) -> bool {
        !self.panel.wb.armed.is_empty()
            || self.panel.wb.drag.is_active()
            || self.panel.switcher.toast.is_some()
            || self.flash.is_some()
    }
}

fn unix_ms() -> u64 {
    crate::agent_window::unix_now_ms()
}

/// Runs `f` on the glue and the app's state, when the app has them and the glue is not held already.
pub fn with_glue<R>(app: &RefAny, f: impl FnOnce(&mut Glue, &AppState) -> R) -> Option<R> {
    let mut handle = app.clone();
    let state = handle.downcast_ref::<AppState>()?;
    let mut glue = state.wb.try_borrow_mut().ok()?;
    Some(f(&mut glue, &state))
}

// ==== what the panel reaches the app through ====

fn with_panel(app: &mut RefAny, f: &mut dyn FnMut(&mut Panel)) -> bool {
    with_glue(app, |g, _| f(&mut g.panel)).is_some()
}

/// What the panel's callbacks reach the app through.
#[must_use]
pub fn host(app: &RefAny) -> Host {
    Host { app: app.clone(), with: with_panel, on_jump, on_changed, on_plan }
}

/// The panel, for the right of the window: the board as it is now. Nothing (an empty box) when the board is being
/// worked on in the very call that asks.
#[must_use]
pub fn panel_content(s: &AppState, data: &RefAny) -> Dom {
    let host = host(data);
    match s.wb.try_borrow_mut() {
        Ok(mut glue) => whiteboard_ui::panel_dom(&mut glue.panel, &host),
        Err(_) => Dom::create_div(),
    }
}

// ==== the start ====

/// The words of a list of boards that could not be used, in the person's language.
fn boards_words(e: &BoardsError) -> String {
    let mut args: Vec<(&str, Arg)> = Vec::new();
    if let Some(why) = e.why() {
        args.push(("why", Arg::from(why.to_string())));
    }
    match e {
        BoardsError::NotAnIndex(_) => {}
        BoardsError::TooNew { found } => args.push(("found", Arg::from(*found as usize))),
        BoardsError::TooLarge { bytes } => args.push(("bytes", Arg::from(*bytes))),
    }
    t_args(e.key(), &args)
}

/// The words of a board file that could not be used.
fn store_words(e: &StoreError) -> String {
    let mut args: Vec<(&str, Arg)> = Vec::new();
    if let Some(why) = e.why() {
        args.push(("why", Arg::from(why.to_string())));
    }
    for (k, v) in e.args() {
        args.push((k, Arg::from(usize::try_from(v).unwrap_or(usize::MAX))));
    }
    t_args(e.key(), &args)
}

/// What the settings and the repository say, told to the board: the author a document names, the form of a link to code,
/// how long a loop is armed, whether the tip was shown, the folder and its repository.
fn configure(glue: &mut Glue, s: &AppState) {
    let mut kit_ref = s.kit.clone();
    let (author, template, armed, tip) = match kit_ref.downcast_ref::<kit::Kit>() {
        Some(k) => (
            k.settings.get(whiteboard_settings::KEY_AUTHOR).map(str::to_string),
            k.settings.get(whiteboard_settings::KEY_PERMALINK).map(str::to_string),
            k.settings.get(whiteboard_settings::KEY_ARMED_SECONDS).map(str::to_string),
            k.settings.get(KEY_TIP_SEEN).map(str::to_string),
        ),
        None => (None, None, None, None),
    };
    let armed_set = armed.as_deref().is_some_and(|v| !v.trim().is_empty());
    let (mut settings, notes) = Settings::parse(&|key| match key {
        whiteboard_settings::KEY_AUTHOR => author.clone(),
        whiteboard_settings::KEY_ARMED_SECONDS => armed.clone(),
        _ => template.clone(),
    });
    for note in &notes {
        eprintln!("[AzReview] {note}");
    }
    if !armed_set {
        settings.armed_ms = ARM_MS_DEFAULT;
    }
    glue.panel.wb.set_settings(settings);
    glue.tip_shown = tip.as_deref() == Some("1");
    if glue.persist {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
        let facts = whiteboard_git::read(&s.root, home.as_deref());
        glue.panel.wb.set_folder(Some(s.root.clone()));
        glue.panel.wb.set_repo(facts);
    }
}

/// The whiteboard of a workspace that was just opened: what it is told, and its list of boards and the board that is open
/// read on a Thread.
pub fn start(data: &RefAny, info: &mut CallbackInfo) {
    let looks = with_glue(data, |g, s| {
        configure(g, s);
        if !g.persist {
            g.started = true;
            return None;
        }
        let index = g.files.join(g.panel.wb.index_name());
        let legacy = g.files.join(whiteboard_store::board_name(g.panel.wb.workspace()));
        Some((index, legacy))
    })
    .flatten();
    if let Some((index, legacy)) = looks {
        whiteboard_io::spawn_start(info, data, index, legacy, on_started as WriteBackCallbackType);
    }
}

/// The list of boards was read (or there is none): what it holds is the list now, and the board that is open is read.
extern "C" fn on_started(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(found) = whiteboard_io::take_started(&mut msg) else {
        return Update::DoNothing;
    };
    let Some((start, files)) = with_glue(&app, |g, _| {
        let start = g.panel.wb.index_loaded(found.index, found.legacy_file, &Panel::first_board_name(), unix_ms());
        if let Some(told) = &start.told {
            g.panel.hint = Some(boards_words(told));
        }
        (start, g.files.clone())
    }) else {
        return Update::DoNothing;
    };
    println!("AZREVIEW_WB boards {}", with_glue(&app, |g, _| g.panel.wb.boards().boards().count()).unwrap_or(0));
    whiteboard_io::spawn_write(&mut info, &app, files.clone(), start.effects, on_written as WriteBackCallbackType);
    read_board(&mut app, &start.load, &mut info);
    Update::RefreshDom
}

/// Small files were written (or not).
extern "C" fn on_written(mut _app: RefAny, mut msg: RefAny, _info: CallbackInfo) -> Update {
    if let Some(done) = whiteboard_io::take_written(&mut msg) {
        for (name, why) in &done.failed {
            eprintln!("[AzReview] the whiteboard's file {name} could not be written: {why}");
            println!("AZREVIEW_WB_ERROR write {name} {why}");
        }
    }
    Update::DoNothing
}

/// The file of the board that is open is read on a Thread (no file is an empty board).
fn read_board(app: &mut RefAny, name: &str, info: &mut CallbackInfo) {
    let path = with_glue(app, |g, _| g.files.join(name));
    if let Some(path) = path {
        whiteboard_io::spawn_read(info, app, path, crate::whiteboard::LIMITS, on_board_read as WriteBackCallbackType);
    }
}

/// A board was read: it is the board now (a file that cannot be used is kept aside before anything replaces it).
extern "C" fn on_board_read(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(read) = whiteboard_io::take_read(&mut msg) else {
        return Update::DoNothing;
    };
    let Some((effects, waiting, files, items)) = with_glue(&app, |g, _| {
        let now = unix_ms();
        let effects = match read.result {
            Some(result) => {
                if let Err(e) = &result {
                    g.panel.hint = Some(store_words(e));
                }
                g.panel.wb.loaded(result, read.raw, now)
            }
            None => g.panel.wb.loaded_none(),
        };
        g.collect();
        g.started = true;
        let items = g.panel.wb.board.len();
        (effects, g.panel.after_wait(), g.files.clone(), items)
    }) else {
        return Update::DoNothing;
    };
    println!("AZREVIEW_WB opened {items}");
    whiteboard_io::spawn_write(&mut info, &app, files, effects, on_written as WriteBackCallbackType);
    follow(&mut app, waiting, &mut info);
    Update::RefreshDom
}

impl Glue {
    /// What the board told since it was asked last is the panel's hint.
    fn collect(&mut self) {
        self.panel.collect_notices();
    }
}

// ==== the file ====

/// The board changed: it is kept (the zip is made and written on a Thread, one write at a time; the owner's gate says when
/// another is wanted after it).
pub fn on_changed(app: &mut RefAny, info: &mut CallbackInfo) {
    let job = with_glue(app, |g, _| {
        if !g.persist || !g.started {
            return None;
        }
        let now = unix_ms();
        let job = g.panel.wb.save_job(now)?;
        let path = g.files.join(&job.name);
        g.saving_to = Some(path.clone());
        Some((job.entries, path))
    })
    .flatten();
    if let Some((entries, path)) = job {
        whiteboard_archive::spawn_save(info, app, entries, path, on_saved as WriteBackCallbackType);
    }
}

/// The Thread has written the file (or could not).
extern "C" fn on_saved(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(result) = whiteboard_archive::take_saved(&mut msg) else {
        return Update::DoNothing;
    };
    let Some((again, waiting, index, files)) = with_glue(&app, |g, _| {
        match (&result, g.saving_to.take()) {
            (Ok(()), Some(path)) => println!("AZREVIEW_WB saved {}", path.display()),
            (Ok(()), None) => println!("AZREVIEW_WB saved"),
            (Err(why), _) => {
                println!("AZREVIEW_WB_ERROR save {why}");
                g.panel.hint = Some(t_args("azreview-save-failed", &[("why", Arg::from(why.clone()))]));
            }
        }
        let again = g.panel.wb.save_done(&result);
        let index = g.panel.wb.take_index_effect();
        (again, g.panel.after_wait(), index, g.files.clone())
    }) else {
        return Update::DoNothing;
    };
    if let Some(effect) = index {
        whiteboard_io::spawn_write(&mut info, &app, files, vec![effect], on_written as WriteBackCallbackType);
    }
    follow(&mut app, waiting, &mut info);
    if again {
        on_changed(&mut app, &mut info);
    }
    Update::RefreshDom
}

/// The board that is open is kept before the folder is left (its own Thread: what it says is not about the next workspace's
/// board).
pub fn leave(data: &RefAny, info: &mut CallbackInfo) {
    let job = with_glue(data, |g, _| {
        if !g.persist || !g.started {
            return None;
        }
        let job = g.panel.wb.save_job(unix_ms())?;
        let index = g.panel.wb.take_index_effect();
        Some((job.entries, g.files.join(&job.name), index, g.files.clone()))
    })
    .flatten();
    if let Some((entries, path, index, files)) = job {
        whiteboard_archive::spawn_save(info, data, entries, path, on_left_saved as WriteBackCallbackType);
        if let Some(effect) = index {
            whiteboard_io::spawn_write(info, data, files, vec![effect], on_written as WriteBackCallbackType);
        }
    }
}

/// The board of the workspace that was left is written; what it came to is not the new workspace's to take.
extern "C" fn on_left_saved(mut _app: RefAny, mut msg: RefAny, _info: CallbackInfo) -> Update {
    match whiteboard_archive::take_saved(&mut msg) {
        Some(Ok(())) => println!("AZREVIEW_WB left saved"),
        Some(Err(why)) => println!("AZREVIEW_WB_ERROR save {why}"),
        None => {}
    }
    Update::DoNothing
}

/// What came of a change that waited for the open board to be kept.
fn follow(app: &mut RefAny, outcome: Option<BoardOutcome>, info: &mut CallbackInfo) {
    match outcome {
        Some(BoardOutcome::Done(plan)) => on_plan(app, plan, info),
        Some(BoardOutcome::Wait) => on_changed(app, info),
        Some(BoardOutcome::Told) | None => {}
    }
}

/// The list of boards changed or another board is to be read: what the plan says is done.
pub fn on_plan(app: &mut RefAny, plan: Plan, info: &mut CallbackInfo) {
    let Some(files) = with_glue(app, |g, _| g.files.clone()) else {
        return;
    };
    if !plan.effects.is_empty() {
        whiteboard_io::spawn_write(info, app, files.clone(), plan.effects, on_written as WriteBackCallbackType);
    }
    if let Some(copy) = plan.copy {
        whiteboard_archive::spawn_save(info, app, copy.entries, files.join(&copy.name), on_copy_saved as WriteBackCallbackType);
    }
    if let Some(name) = plan.load {
        read_board(app, &name, info);
    }
    if let Some(request) = plan.export {
        ask_export_target(app, request);
    }
    if plan.import {
        ask_import_file(app);
    }
    ensure_timer(app, info);
}

/// The copy a duplicate made is written.
extern "C" fn on_copy_saved(mut _app: RefAny, mut msg: RefAny, _info: CallbackInfo) -> Update {
    match whiteboard_archive::take_saved(&mut msg) {
        Some(Ok(())) => println!("AZREVIEW_WB copy saved"),
        Some(Err(why)) => println!("AZREVIEW_WB_ERROR save {why}"),
        None => {}
    }
    Update::DoNothing
}

// ==== export and import ====

/// The person is asked where the exported file goes (a save dialog).
fn ask_export_target(app: &RefAny, request: ExportRequest) {
    let name = whiteboard_export::suggested_name(&request.meta.board_name);
    let kept = with_glue(app, |g, _| g.exporting = Some(request));
    if kept.is_none() {
        return;
    }
    let _request = FileDialog::save_file(
        AzString::from(t("azreview-whiteboard-export-go").as_str()),
        AzString::from(name.as_str()),
        app.clone(),
        on_export_target,
    );
}

/// The save dialog's answer: the board is written there on a Thread (a cancel drops the export).
extern "C" fn on_export_target(data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let path = SaveTargetResult::downcast(result)
        .into_option()
        .and_then(|r| r.target.into_option())
        .and_then(|target| target.as_path().into_option())
        .map(|p| PathBuf::from(p.inner.as_str()));
    let request = with_glue(&data, |g, _| g.exporting.take()).flatten();
    let (Some(path), Some(request)) = (path, request) else {
        return Update::DoNothing;
    };
    whiteboard_archive::spawn_export(&mut info, &data, request, path, on_exported as WriteBackCallbackType);
    Update::DoNothing
}

/// The Thread has written the file (or could not).
extern "C" fn on_exported(app: RefAny, mut msg: RefAny, _info: CallbackInfo) -> Update {
    let Some(result) = whiteboard_archive::take_exported(&mut msg) else {
        return Update::DoNothing;
    };
    with_glue(&app, |g, _| match result {
        Ok(done) => {
            println!("AZREVIEW_WB exported {} {} {} {}", done.path.display(), done.bytes, done.pictures, done.links);
            g.panel.tell(t_args(
                "azreview-whiteboard-exported",
                &[("path", Arg::from(done.path.display().to_string())), ("pictures", Arg::from(done.pictures)), ("links", Arg::from(done.links))],
            ));
        }
        Err(why) => {
            println!("AZREVIEW_WB_ERROR export {why}");
            g.panel.tell(t_args("azreview-whiteboard-export-failed", &[("why", Arg::from(why))]));
        }
    });
    Update::RefreshDom
}

/// The person is asked for a file of a board (an open dialog).
fn ask_import_file(app: &RefAny) {
    let _request = FileDialog::open_file(
        AzString::from(t("azreview-whiteboard-import").as_str()),
        OptionString::None,
        OptionFileTypeList::None,
        app.clone(),
        on_import_picked,
    );
}

/// The open dialog's answer: the file is read and its pictures decoded on a Thread (a cancel changes nothing).
extern "C" fn on_import_picked(data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing;
    };
    whiteboard_archive::spawn_import(&mut info, &data, PathBuf::from(path.inner.as_str()), crate::whiteboard::LIMITS, on_imported as WriteBackCallbackType);
    Update::DoNothing
}

/// The file has been read: it becomes a board of its own (once the open board is kept), or the person is told why not.
extern "C" fn on_imported(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(answer) = whiteboard_archive::take_imported(&mut msg) else {
        return Update::DoNothing;
    };
    let Some(outcome) = with_glue(&app, |g, _| match answer.result {
        Ok(loaded) => {
            let (left_out, lost, name) = (loaded.left_out, loaded.unreadable_pictures, loaded.meta.board_name.clone());
            let outcome = g.panel.import_loaded(loaded);
            if matches!(outcome, BoardOutcome::Done(_)) {
                let shown = g.panel.wb.active_board().1.to_string();
                println!("AZREVIEW_WB imported {} {} {left_out} {lost}", name.replace(' ', "_"), g.panel.wb.board.len());
                g.panel.tell(t_args("azreview-whiteboard-imported", &[("name", Arg::from(shown)), ("file", Arg::from(answer.file.clone()))]));
            }
            Some(outcome)
        }
        Err(e) => {
            println!("AZREVIEW_WB_ERROR import {}", e.english());
            let why = e.why().map_or_else(|| e.english(), str::to_string);
            g.panel.tell(t_args("azreview-whiteboard-import-failed", &[("why", Arg::from(why))]));
            None
        }
    }) else {
        return Update::DoNothing;
    };
    let kept = matches!(outcome, Some(BoardOutcome::Done(_)));
    follow(&mut app, outcome, &mut info);
    if kept {
        // The board that was read has no file of its own yet: it is kept now.
        on_changed(&mut app, &mut info);
    }
    Update::RefreshDom
}

// ==== the panel of the window ====

/// The right panel opens (slides out) when it is closed, and what is chosen is remembered: what a region carried to the edge of
/// the window and a region sent with its key both ask for.
pub fn open_panel(data: &mut RefAny, info: &mut CallbackInfo) {
    let opened = data.downcast_mut::<AppState>().is_some_and(|mut s| {
        if s.panels.right_open {
            return false;
        }
        s.set_panel_open(layout::Panel::Right, true);
        true
    });
    if opened {
        crate::persist_panels(data, info);
        crate::start_slide(data, info);
    }
}

// ==== the jump ====

/// A double click on a cutout: the window goes to its file and its lines, and the lines are lit for a moment.
pub fn on_jump(app: &mut RefAny, id: ItemId, info: &mut CallbackInfo) -> Update {
    let target = {
        let mut handle = app.clone();
        let Some(state) = handle.downcast_ref::<AppState>() else {
            return Update::DoNothing;
        };
        let Ok(mut glue) = state.wb.try_borrow_mut() else {
            return Update::DoNothing;
        };
        let view = WorkspaceView::new(state.source.clone(), state.root.clone());
        match jump(&mut glue.panel.wb.board, id, &view) {
            Ok(target) => {
                println!("AZREVIEW_WB jump {} {} {} {:?}", target.file, target.first_line, target.last_line, target.freshness);
                glue.panel.tell_jump(&target);
                target
            }
            Err(refused) => {
                glue.panel.hint = Some(whiteboard_ui::say(&refused));
                return Update::RefreshDom;
            }
        }
    };
    go_to(app, info, &target);
    Update::RefreshDom
}

/// The window goes to the lines of `target`: the file opens when it is not the open one, the page is brought into view, the
/// lines are lit. The place the reviewer was reviewing at is where "Continue reviewing" comes back to.
fn go_to(app: &mut RefAny, info: &mut CallbackInfo, target: &JumpTarget) {
    let page = crate::code::SourceFile::page_of_line(target.first_line as usize);
    let opened = {
        let Some(mut s) = app.downcast_mut::<AppState>() else {
            return;
        };
        let here = s.review_place();
        let open_already = s.file().is_some_and(|f| f.display == target.file);
        if !open_already && !s.open_path(&target.file) {
            false
        } else {
            // the first jump only: a jump from one place to another keeps the place the reviewing was left at
            if let Some(here) = here {
                let _ = s.progress.jumped_from(here);
            }
            s.visible_page = page;
            true
        }
    };
    if !opened {
        with_glue(app, |g, _| g.panel.hint = Some(t("azreview-whiteboard-jump-file-missing")));
        return;
    }
    with_glue(app, |g, _| {
        let now = g.panel.now_ms();
        g.flash = Some(Flash::of(target, now));
    });
    ui::scroll_to_page(info, page);
    info.trigger_all_virtual_view_rerender();
    ensure_timer(app, info);
}

// ==== time ====

/// How often the board's own clock is looked at while something waits on it, ms.
const TICK_MS: u64 = 100;

/// A timer that looks at the clock for what waits on it (an armed loop, a press that may be held, the bar that undoes a
/// deletion, the lines that are lit) while there is anything: it ends itself when nothing is left.
pub fn ensure_timer(app: &RefAny, info: &mut CallbackInfo) {
    let id = with_glue(app, |g, _| {
        if g.timer.is_some() || !g.has_time_in_it() {
            return None;
        }
        let id = TimerId::unique();
        g.timer = Some(id);
        Some(id)
    })
    .flatten();
    if let Some(id) = id {
        let timer = Timer::create(app.clone(), on_tick, info.get_system_time_fn())
            .with_interval(Duration::System(SystemTimeDiff::from_millis(TICK_MS)));
        info.add_timer(id, timer);
    }
}

/// What a tick found: what the window is asked to do about it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Ticked {
    /// A press was held: the stroke it began is dropped, the region lifts.
    pub held: bool,
    /// The panel's DOM changed (a toast went, the lines stopped being lit).
    pub panel_changed: bool,
    /// The pages' DOM changed (a loop is no longer armed, the lines stopped being lit).
    pub pages_changed: bool,
    /// Something is still waiting on the clock.
    pub more: bool,
    /// The overlay of the pages is to be drawn again (the countdown moved).
    pub redraw: bool,
}

/// One look at the clock.
pub fn tick(g: &mut Glue) -> Ticked {
    let now = g.panel.now_ms();
    tick_at(g, now)
}

/// One look at the clock, at `now` ms of the panel's.
pub fn tick_at(g: &mut Glue, now: u64) -> Ticked {
    let mut out = Ticked::default();
    if g.panel.wb.page_tick(now).is_some() {
        out.held = true;
        out.pages_changed = true;
    }
    if g.panel.wb.expire(now) {
        out.pages_changed = true;
    }
    if g.panel.expire_toast(now) {
        out.panel_changed = true;
    }
    if g.flash.as_ref().is_some_and(|f| f.is_over(now)) {
        g.flash = None;
        out.pages_changed = true;
    }
    out.redraw = !g.panel.wb.armed.is_empty();
    out.more = g.has_time_in_it();
    if !out.more {
        g.timer = None;
    }
    out
}

extern "C" fn on_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let ticked = with_glue(&data, |g, _| tick(g));
    let Some(ticked) = ticked else {
        return TimerCallbackReturn::continue_unchanged();
    };
    if ticked.held {
        // The press that was held was no stroke: the dab it began is dropped.
        if let Some(mut s) = data.downcast_mut::<AppState>() {
            s.stroke_drop();
        }
        println!("AZREVIEW_WB held");
    }
    if ticked.pages_changed {
        info.callback_info.trigger_all_virtual_view_rerender();
    }
    if ticked.redraw || ticked.held {
        info.update_all_image_callbacks();
    }
    let update = if ticked.pages_changed || ticked.panel_changed { Update::RefreshDom } else { Update::DoNothing };
    TimerCallbackReturn {
        should_update: update,
        should_terminate: if ticked.more { TerminateTimer::Continue } else { TerminateTimer::Terminate },
    }
}

// ==== the keys ====

/// Whether `window` point is on the board's panel (an open one): the board has the keyboard after a press there.
#[must_use]
pub fn point_is_on_panel(app: &RefAny, window_w: f32, window_h: f32, x: f32, y: f32) -> bool {
    with_glue(app, |_, s| {
        let solved = layout::solve(window_w, &s.panels, &s.slide, 0.0);
        let bar = crate::whiteboard_glue::bar_of(crate::whiteboard_geom::Size::new(window_w, window_h), &s.panels, &solved);
        bar.open && bar.rect.contains(crate::whiteboard_geom::Pt::new(x, y))
    })
    .unwrap_or(false)
}

/// Any press of the window's left button: the board has the keyboard when the press was on its panel, and has not when it
/// was anywhere else.
pub extern "C" fn on_window_down(data: RefAny, info: CallbackInfo) -> Update {
    let Some(at) = info.get_cursor_position().into_option() else {
        return Update::DoNothing;
    };
    let size = info.get_current_window_state().size.dimensions;
    let on = point_is_on_panel(&data, size.width, size.height, at.x, at.y);
    with_glue(&data, |g, _| g.keys = on);
    Update::DoNothing
}

/// A key the board answers when it has the keyboard (Delete or Backspace takes the selection off, Escape lets go, Mod+Z
/// steps back, Shift+Mod+Z or Mod+Y forward): `Some` when it did, and then the key is not the window's. A label that is
/// being typed in has its own text area, which has the key.
pub fn on_key(data: &mut RefAny, info: &mut CallbackInfo, code: VirtualKeyCode) -> Option<Update> {
    let mods = info.get_key_modifiers();
    let key = whiteboard_ui::key_of(code, mods.primary_down(), mods.shift)?;
    let acted = with_glue(data, |g, _| {
        if !g.keys || g.panel.editing.is_some() {
            return false;
        }
        whiteboard_ui::on_key(&mut g.panel, key)
    })?;
    if !acted {
        return None;
    }
    info.prevent_default();
    println!("AZREVIEW_WB key {key:?}");
    on_changed(data, info);
    info.trigger_all_virtual_view_rerender();
    if let Some(node) = whiteboard_ui::surface_node(info) {
        kit::redraw_image(info, node);
    }
    Some(Update::RefreshDom)
}

/// Escape for the board, without the keyboard having to be on it: a lifted region is put down, a loop that waits is let go.
/// `true` when there was anything to let go of.
pub fn escape(glue: &mut Glue) -> bool {
    glue.panel.drag_over = false;
    whiteboard_ui::on_key(&mut glue.panel, Key::Escape)
}

// ==== the settings page ====

/// The times a loop may wait for the board the settings page offers, ms.
pub static ARM_PRESETS: [u64; 4] = [8_000, 15_000, 30_000, 60_000];
/// Their labels.
static ARM_LABELS: [&str; 4] = ["azreview-armed-8", "azreview-armed-15", "azreview-armed-30", "azreview-armed-60"];
/// The lines of the help, in the order they are read.
pub static HELP_LINES: [&str; 5] = [
    "azreview-whiteboard-help-circle",
    "azreview-whiteboard-help-lift",
    "azreview-whiteboard-help-drop",
    "azreview-whiteboard-help-jump",
    "azreview-whiteboard-help-boards",
];

/// The whiteboard's section of the settings page: how long a circle waits to be taken, and the help (five lines, the way
/// from a circle to a cutout and back).
#[must_use]
pub fn settings_section(s: &AppState, app: &RefAny) -> kit::AppSection {
    let armed_ms = s.wb.try_borrow().map_or(ARM_MS_DEFAULT, |g| g.panel.wb.armed.lifetime_ms());
    let mut content = Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(kit::row(
            "azreview-whiteboard-armed-when",
            Segmented::create(labels(&ARM_LABELS))
                .with_selected_index(nearest(&ARM_PRESETS, armed_ms))
                .with_on_change(app.clone(), on_armed as SegmentedOnChangeCallbackType)
                .dom()
                .with_id(ids::SETTING_ARMED),
        ))
        .with_child(
            Dom::create_div()
                .with_id(ids::WHITEBOARD_HELP)
                .with_css("display: flex; flex-direction: column; padding-top: 6px;")
                .with_child(
                    Dom::create_span_with_text(label("azreview-whiteboard-help-title"))
                        .with_css("font-size: 13px; font-weight: 600; padding: 4px 0px;"),
                ),
        );
    for line in HELP_LINES {
        content.add_child(kit::note(line));
    }
    kit::AppSection { category: 0, title: t("azreview-category-whiteboard"), content }
}

/// View > Using the whiteboard...: the settings page, where the whiteboard's help is.
pub extern "C" fn on_menu_help(mut data: RefAny, _info: CallbackInfo) -> Update {
    let kit_ref = data.downcast_ref::<AppState>().map(|s| s.kit.clone());
    if let Some(kit_ref) = kit_ref {
        kit::open_settings(&kit_ref, None);
    }
    Update::RefreshDom
}

/// A time was chosen for how long a circle waits: it is the board's now, and kept for the next start.
extern "C" fn on_armed(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    let Some(&ms) = ARM_PRESETS.get(state.selected_index) else {
        return Update::DoNothing;
    };
    let kit_ref = {
        let Some(s) = data.downcast_ref::<AppState>() else {
            return Update::DoNothing;
        };
        if let Ok(mut g) = s.wb.try_borrow_mut() {
            let _held = g.panel.wb.armed.set_lifetime_ms(ms);
        }
        s.kit.clone()
    };
    kit::set_value(&kit_ref, &mut info, whiteboard_settings::KEY_ARMED_SECONDS, &format!("{}", ms / 1_000));
    Update::RefreshDom
}

#[cfg(test)]
#[path = "whiteboard_plug_test.rs"]
mod whiteboard_plug_test;
