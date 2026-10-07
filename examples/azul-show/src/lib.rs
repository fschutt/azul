//! AzShow: a PowerPoint-style presentation editor on the public azul API.
//!
//! The window is the S1 `DocumentShell` (an app-drawn `Titlebar` under
//! `WindowDecorations::NoTitle`, the ribbon or the backstage, the slide
//! rail, the slide canvas over the notes, the format pane, the status bar)
//! inside a `ShellThemeScope`; it follows the app theme (flat / flora) and
//! the OS mode. The slide show runs full screen in the main window with the
//! presenter view in a second one. Decks are files (`show/<id>/deck.json`,
//! `show/<id>/media/`) written through azul-storage from a `Thread`.
//!
//! On stdout, for scripts (`scripts/azshow_e2e.py`): `AZSHOW_READY`,
//! `AZSHOW_DECK <id>` (a deck is open), `AZSHOW_SLIDE <n>` (1-based, the
//! slide on the canvas), `AZSHOW_SLIDES <count> <current>` (a new slide),
//! `AZSHOW_ORDER <slide id,...>` (the rail / sorter moved slides),
//! `AZSHOW_FRAME <element id> x y w h rotation` (a drag or nudge committed),
//! `AZSHOW_VIEW <name>`, `AZSHOW_SHOW <slide> <step>` (the show moved),
//! `AZSHOW_SHOW_ENDED` (past the last slide), `AZSHOW_SHOW_CLOSED` (the show is over), `AZSHOW_SAVED <id>`,
//! `AZSHOW_OPENED <id>`, `AZSHOW_LISTED <n>`, `AZSHOW_EXPORTED <kind> <bytes>`.

pub mod app;
pub mod args;
mod backstage;
pub mod commands;
pub mod editor;
pub mod find;
mod find_ui;
pub mod ids;
pub mod model;
pub mod render;
mod ribbon;
mod show;
pub mod storage;
pub mod text;
pub mod themes;
pub mod views;

use std::{path::PathBuf, sync::Arc};

use azul::{
    app::App,
    callbacks::{
        CallbackInfo, CloseGuardOnEventCallbackType, LayoutCallbackInfo, RefAny, TimerCallbackInfo,
        TimerCallbackReturn, Update,
    },
    css::EventFilter,
    dom::{Dom, DomId, VirtualKeyCode},
    error::ResultRawImageDecodeImageError,
    image::{ImageRef, RawImage},
    shells::{DocumentShell, ShellEmptyState, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    svg::{CssPath, CssPathSelector},
    task::{
        TerminateTimer, Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg,
        Timer, TimerId,
    },
    time::{Duration, SystemTimeDiff},
    vec::U8VecRef,
    widgets::{CloseGuard, CloseGuardEvent, CloseGuardEventKind, ThumbnailStripLayout, Titlebar},
    window::WindowEventFilter,
};
use azul_appkit::{ui as kit, AboutInfo, Shortcut};
use azul_storage::Drive;

pub use crate::args::{Args, StartScreen};
use crate::{
    app::{command, AppState, BackstagePage, Command, Screen, View, DOCUMENT_RATIO, NAVIGATION_RATIO},
    commands::on_command,
    model::LayoutKind,
    storage::{Job, Outcome},
};

fn s(text: &str) -> AzString {
    AzString::from(text)
}

/// The shell accent that goes with the deck's theme.
fn accent_of(st: &AppState) -> ShellThemeAccent {
    themes::accent(st.editor.as_ref().map_or(1, |e| themes::index_of(&e.deck.theme)))
}

/// The window's title row: the deck's title, a star while unsaved.
fn title_row(st: &AppState, suffix: &str) -> Dom {
    let title = st.editor.as_ref().map_or_else(
        || format!("AzShow{suffix}"),
        |e| format!("{}{} - AzShow{suffix}", e.deck.title, if e.dirty { " *" } else { "" }),
    );
    Titlebar::create(s(&title)).without_border_bottom().dom()
}

/// The editor window: the S1 shell with the ribbon (or the backstage), the
/// rail, the view's document, the format pane and the status bar.
fn editor_window(app: &RefAny, st: &AppState, w: f32, h: f32, monitors: &[(u32, String)]) -> Dom {
    let title = title_row(st, "");
    if let Some(kit_ref) = st.kit.as_ref().filter(|k| kit::settings_open(k)) {
        // File > Options: the kit's settings page, one for every Azlin app.
        return DocumentShell::create(
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
                .with_child(kit::settings_page(kit_ref, Vec::new())),
        )
        .office_shell()
        .with_title_row(title)
        .dom();
    }
    if st.screen == Screen::Backstage {
        return DocumentShell::create(Dom::create_div())
            .office_shell()
            .with_title_row(title)
            .with_backstage(backstage::backstage(app, st))
            .dom();
    }
    let zoom = st.zoom_percent(w, h);
    let status = views::status_bar(app, st, zoom);
    let ribbon = ribbon::ribbon(app, st, monitors);
    let Some(ed) = st.editor.as_ref() else {
        let empty = ShellEmptyState::create(s("No presentation is open"))
            .with_icon(s("slideshow"))
            .with_detail(s("Start one from a theme, or open the sample deck."))
            .with_action_label(s("New Presentation"))
            .with_on_action(
                command(app, Command::OpenBackstage(BackstagePage::New)),
                on_command as azul::callbacks::ButtonOnClickCallbackType,
            )
            .dom();
        return DocumentShell::create(empty)
            .office_shell()
            .with_title_row(title)
            .with_ribbon(ribbon)
            .with_status_bar(status)
            .dom();
    };
    let scale = st.canvas_scale(w, h);
    let (navigation, document, side) = match st.view {
        View::Normal => (
            Some(views::strip(app, st, ed, ThumbnailStripLayout::Column)),
            views::normal_document(app, st, ed, scale),
            Some(views::format_pane(app, ed)),
        ),
        View::Sorter => (None, views::strip(app, st, ed, ThumbnailStripLayout::Grid), None),
        View::Outline => (
            Some(views::outline(app, ed)),
            views::normal_document(app, st, ed, scale),
            Some(views::format_pane(app, ed)),
        ),
        View::NotesPage => (
            Some(views::strip(app, st, ed, ThumbnailStripLayout::Column)),
            views::notes_page(app, st, ed),
            None,
        ),
    };
    let mut shell = DocumentShell::create(document)
        .with_navigation_ratio(NAVIGATION_RATIO)
        .with_document_ratio(DOCUMENT_RATIO);
    if let Some(nav) = navigation {
        shell = shell.with_navigation(nav);
    }
    // The Find / Replace pane takes the side pane while it is open.
    let side = match &st.find {
        Some(f) => Some(find_ui::pane(app, f)),
        None => side,
    };
    if let Some(side) = side {
        shell = shell.with_side_pane(side);
    }
    // The chrome is the OfficeShell's.
    shell
        .office_shell()
        .with_title_row(title)
        .with_ribbon(ribbon)
        .with_status_bar(status)
        .dom()
}

/// The main window's layout.
extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    let app = data.clone();
    // Reading the mode and the theme makes a switch of either rebuild the window.
    let _mode = info.get_mode();
    let _theme = info.get_theme();
    let (w, h) = (info.get_window_width(), info.get_window_height());
    let Some(guard) = data.downcast_ref::<AppState>() else {
        return Dom::create_body();
    };
    let st = &*guard;
    let content = if st.screen == Screen::Show {
        show::show_screen(&app, st, w, h)
    } else {
        // The connected screens, for Slide Show > Monitors.
        let monitors = crate::app::PresenterMonitor::choices(&info.get_monitors());
        editor_window(&app, st, w, h, &monitors)
    };
    // "Save changes?" before the window closes with an unsaved deck: the
    // close request is held while the deck is dirty, the standard question
    // shows, the answer comes to on_close_guard.
    let (title, dirty) = st
        .editor
        .as_ref()
        .map_or((String::from("Presentation"), false), |e| (e.deck.title.clone(), e.dirty));
    let content = CloseGuard::create(content, s(&title))
        .with_dirty(dirty)
        .with_asking(st.asking_close)
        .with_on_event(app.clone(), on_close_guard as CloseGuardOnEventCallbackType)
        .dom();
    // The scope as the window's body (SMALL6's engine fix): no UA margin,
    // the full window height.
    ShellThemeScope::create(content).with_accent(accent_of(st)).body()
        .with_callback(EventFilter::Window(WindowEventFilter::VirtualKeyDown), app, on_window_key)
}

/// The close guard's answers: ask, save then close, close without saving,
/// or stay.
extern "C" fn on_close_guard(mut data: RefAny, mut info: CallbackInfo, event: CloseGuardEvent) -> Update {
    let handle = data.clone();
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let st = &mut *guard;
    match event.kind {
        CloseGuardEventKind::Ask => st.asking_close = true,
        CloseGuardEventKind::Save => {
            st.asking_close = false;
            st.close_after_save = true;
            return commands::apply(&handle, st, Command::Save, &mut info);
        }
        CloseGuardEventKind::Discard => {
            // The guard closes the window itself.
            st.asking_close = false;
            if let Some(ed) = st.editor.as_mut() {
                ed.dirty = false;
            }
        }
        CloseGuardEventKind::Cancel => st.asking_close = false,
    }
    Update::RefreshDom
}

/// The presenter window's layout.
pub extern "C" fn presenter_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let app = data.clone();
    let Some(guard) = data.downcast_ref::<AppState>() else {
        return Dom::create_body();
    };
    let st = &*guard;
    let column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(Titlebar::create(s("Presenter View - AzShow")).without_border_bottom().dom())
        .with_child(show::presenter(&app, st));
    Dom::create_body()
        .with_css("display: flex; flex-direction: column; margin: 0px; padding: 0px; height: 100%;")
        .with_child(ShellThemeScope::create(column).with_accent(accent_of(st)).dom())
        .with_callback(EventFilter::Window(WindowEventFilter::VirtualKeyDown), app, on_window_key)
}

/// The presenter window is up: it refreshes its clock and closes itself
/// when the show is over.
pub extern "C" fn on_presenter_created(data: RefAny, mut info: CallbackInfo) -> Update {
    let timer = Timer::create(data, presenter_tick, info.get_system_time_fn())
        .with_interval(Duration::System(SystemTimeDiff::from_millis(500)));
    info.add_timer(TimerId::unique(), timer);
    Update::DoNothing
}

extern "C" fn presenter_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let over = data.downcast_ref::<AppState>().map_or(true, |st| st.show.is_none());
    if over {
        info.callback_info.close_window();
        return TimerCallbackReturn {
            should_update: Update::DoNothing,
            should_terminate: TerminateTimer::Terminate,
        };
    }
    TimerCallbackReturn {
        should_update: Update::RefreshDom,
        should_terminate: TerminateTimer::Continue,
    }
}

/// The build / transition player: redraws both windows until it is done.
pub extern "C" fn on_play_tick(mut data: RefAny, _info: TimerCallbackInfo) -> TimerCallbackReturn {
    let Some(mut st) = data.downcast_mut::<AppState>() else {
        return TimerCallbackReturn {
            should_update: Update::DoNothing,
            should_terminate: TerminateTimer::Terminate,
        };
    };
    let done = match st.show.as_mut() {
        Some(rt) => match &rt.play {
            Some(p) if !p.done() => false,
            Some(_) => {
                rt.play = None;
                true
            }
            None => true,
        },
        None => true,
    };
    if done {
        st.playing_timer = false;
    }
    TimerCallbackReturn {
        should_update: Update::RefreshDomAllWindows,
        should_terminate: if done {
            TerminateTimer::Terminate
        } else {
            TerminateTimer::Continue
        },
    }
}

/// After the next layout, the focus goes into the text being edited.
pub fn focus_text_soon(info: &mut CallbackInfo, app: &RefAny) {
    let timer = Timer::create(app.clone(), focus_text_tick, info.get_system_time_fn())
        .with_delay(Duration::System(SystemTimeDiff::from_millis(80)));
    info.add_timer(TimerId::unique(), timer);
}

extern "C" fn focus_text_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let target = data
        .downcast_mut::<AppState>()
        .and_then(|mut st| st.focus_text.take());
    if let Some(id) = target {
        info.callback_info.set_focus_to_path(
            DomId { inner: 0 },
            CssPath {
                selectors: vec![CssPathSelector::Id(AzString::from(id))].into(),
            },
        );
    }
    TimerCallbackReturn {
        should_update: Update::DoNothing,
        should_terminate: TerminateTimer::Terminate,
    }
}

// ==== The storage thread ====

struct JobInit {
    job: Option<Job>,
    drive: Arc<dyn Drive>,
}

struct JobDone {
    outcome: Option<Outcome>,
}

/// Runs `job` on a storage thread over the app's one drive.
pub fn spawn_storage(info: &mut CallbackInfo, app: &RefAny, drive: Arc<dyn Drive>, job: Job) {
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(JobInit { job: Some(job), drive }),
            app.clone(),
            storage_thread,
        ),
    );
}

extern "C" fn storage_thread(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((job, drive)) = init.downcast_mut::<JobInit>().and_then(|mut i| {
        let job = i.job.take()?;
        Some((job, Arc::clone(&i.drive)))
    }) else {
        return;
    };
    let outcome = storage::run_job(&*drive, job);
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_storage_done,
        RefAny::new(JobDone { outcome: Some(outcome) }),
    )));
}

/// A picture's bytes decoded for the renderer.
fn decode_image(bytes: &[u8]) -> Option<ImageRef> {
    match RawImage::decode_image_bytes_any(U8VecRef::from(bytes)) {
        ResultRawImageDecodeImageError::Ok(image) => ImageRef::create_rawimage(image).into_option(),
        _ => None,
    }
}

extern "C" fn on_storage_done(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(outcome) = msg.downcast_mut::<JobDone>().and_then(|mut d| d.outcome.take()) else {
        return Update::DoNothing;
    };
    let Some(mut guard) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let st = &mut *guard;
    st.busy = st.busy.saturating_sub(1);
    match outcome {
        Outcome::Saved(Ok(id)) => {
            if let Some(ed) = st.editor.as_mut().filter(|e| e.deck.id == id) {
                ed.dirty = false;
            }
            st.message = format!("Saved show/{id}/deck.json");
            println!("AZSHOW_SAVED {id}");
            if st.close_after_save {
                // "Save" in the close question: written, now the window goes.
                st.close_after_save = false;
                info.close_window();
            }
        }
        Outcome::Saved(Err(e)) => {
            // A failed save keeps the window (and the work) open.
            st.close_after_save = false;
            st.message = format!("Not saved: {e}");
        }
        Outcome::Exported(Ok(key)) => {
            let shown = st.drive.local_path(&key).map_or_else(|| key.clone(), |p| p.display().to_string());
            st.message = format!("Exported to {shown}");
            println!("AZSHOW_EXPORTED {key}");
        }
        Outcome::Exported(Err(e)) => st.message = format!("Not exported: {e}"),
        Outcome::Loaded(Ok((deck, media))) => {
            st.media.clear();
            for (key, bytes) in media {
                if let Some(image) = decode_image(&bytes) {
                    st.media.insert(key, image);
                }
            }
            let id = deck.id.clone();
            commands::open_deck(st, *deck);
            st.message = String::from("Opened");
            println!("AZSHOW_OPENED {id}");
        }
        Outcome::Loaded(Err(e)) => st.message = format!("Could not open the deck: {e}"),
        Outcome::Listed(Ok(decks)) => {
            println!("AZSHOW_LISTED {}", decks.len());
            st.decks = decks;
        }
        Outcome::Listed(Err(e)) => st.message = format!("Could not list the decks: {e}"),
        Outcome::MediaStored(Ok((key, bytes))) => match decode_image(&bytes) {
            Some(image) => {
                let size = image.get_size();
                st.media.insert(key.clone(), image);
                if let Some(ed) = st.editor.as_mut() {
                    ed.insert_image(&key, size.width, size.height);
                }
            }
            None => st.message = String::from("The picture could not be read"),
        },
        Outcome::MediaStored(Err(e)) => st.message = format!("The picture was not stored: {e}"),
    }
    Update::RefreshDom
}

// ==== Keys ====

/// Whether the keyboard is on the canvas or the rail (or nowhere), so the
/// clipboard keys are the slide's, not a text field's.
fn canvas_has_focus(info: &CallbackInfo) -> bool {
    let Some(node) = info.get_focused_node().into_option() else {
        return true;
    };
    let classes = info.get_node_classes(node);
    classes.as_ref().iter().any(|c| {
        let c = c.as_str();
        c == "__azul-native-selection-adorner" || c.starts_with("__azul-native-thumbnail-strip")
    })
}

/// The editor's shortcuts.
fn editor_shortcut(
    key: VirtualKeyCode,
    primary: bool,
    shift: bool,
    slide_keys: bool,
    editing: bool,
    screen: Screen,
) -> Option<Command> {
    use VirtualKeyCode as K;
    Some(match key {
        K::Escape if editing => Command::StopEditing,
        K::F5 if shift => Command::StartShow { from_current: true },
        K::F5 => Command::StartShow { from_current: false },
        K::S if primary => Command::Save,
        K::M if primary => Command::NewSlide(LayoutKind::TitleAndContent),
        K::Z if primary && slide_keys => Command::Undo,
        K::Y if primary && slide_keys => Command::Redo,
        K::D if primary && slide_keys => Command::Duplicate,
        K::G if primary && shift && slide_keys => Command::Ungroup,
        K::G if primary && slide_keys => Command::Group,
        K::C if primary && slide_keys => Command::Copy,
        K::X if primary && slide_keys => Command::Cut,
        K::V if primary && slide_keys => Command::Paste,
        K::A if primary && slide_keys => Command::SelectAll,
        K::F if primary => Command::Find(false),
        K::H if primary => Command::Find(true),
        K::B if primary => Command::Bold,
        K::I if primary => Command::Italic,
        K::U if primary => Command::Underline,
        K::Escape if screen == Screen::Backstage => Command::CloseBackstage,
        _ => return None,
    })
}

/// A key anywhere in a window: the show's keys during the show, the
/// editor's shortcuts otherwise.
extern "C" fn on_window_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    // The kit's settings page covers the window: its keys (Escape = Cancel, F1), and no
    // editor key acts under it.
    let kit_ref = data.downcast_ref::<AppState>().and_then(|st| st.kit.clone());
    if let Some(kit_ref) = kit_ref.filter(|k| kit::settings_open(k)) {
        return kit::handle_key(&kit_ref, &mut info).unwrap_or(Update::DoNothing);
    }
    let ks = info.get_current_keyboard_state();
    let Some(key) = ks.current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    let mods = info.get_key_modifiers();
    let primary = mods.primary_down();
    let cmd = {
        let Some(mut st) = data.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        if st.screen == Screen::Show {
            match st.show.as_mut() {
                Some(rt) => show::show_key_command(key, &mut rt.typed),
                None => None,
            }
        } else {
            let editing = st.editor.as_ref().is_some_and(|e| e.editing.is_some());
            let slide_keys = !editing && canvas_has_focus(&info);
            let cmd = editor_shortcut(key, primary, mods.shift, slide_keys, editing, st.screen);
            // Ctrl+B / I / U in the text being edited are the shared
            // editor's own keys (its selection, its history); the window's
            // only while the canvas has the keys (the whole boxes' text).
            match cmd {
                Some(Command::Bold | Command::Italic | Command::Underline) if editing || !slide_keys => None,
                other => other,
            }
        }
    };
    match cmd {
        Some(cmd) => {
            info.prevent_default();
            commands::run(&mut data, cmd, &mut info)
        }
        None => Update::DoNothing,
    }
}

// ==== Start ====

/// What the About box and the settings page say.
pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzShow",
    version: env!("CARGO_PKG_VERSION"),
    summary: "Presentations: decks as show/<id>/deck.json files, the slide show full screen with a \
              presenter window. Part of the Azlin apps, built with azul.",
    license: "MIT",
    app_folder: storage::APP_FOLDER,
};

/// The keys AzShow answers, as the settings page lists them (`Mod` = Cmd
/// on macOS, Ctrl elsewhere). The window's and the canvas's handlers act;
/// this table is what they do.
pub const SHORTCUTS: [Shortcut; 16] = [
    Shortcut::new("File", "Mod+S", "Save the deck"),
    Shortcut::new("Slides", "Mod+M", "New slide"),
    Shortcut::new("Slides", "Mod+D", "Duplicate the selection"),
    Shortcut::new("Edit", "Mod+Z / Mod+Y", "Undo / redo"),
    Shortcut::new("Edit", "Mod+C / Mod+X / Mod+V", "Copy / cut / paste the selection"),
    Shortcut::new("Edit", "Mod+A", "Select every object on the slide"),
    Shortcut::new("Edit", "Mod+G / Mod+Shift+G", "Group / ungroup"),
    Shortcut::new("Edit", "Mod+F", "Find and replace"),
    Shortcut::new("Text", "Mod+B / Mod+I / Mod+U", "Bold / italic / underline"),
    Shortcut::new("Canvas", "Arrows", "Nudge the selection (Mod: finely)"),
    Shortcut::new("Canvas", "Tab / Shift+Tab", "The next / previous object"),
    Shortcut::new("Canvas", "Enter / F2", "Edit the object's text; Escape leaves it"),
    Shortcut::new("Show", "F5 / Shift+F5", "Start from the first / the current slide"),
    Shortcut::new("Show", "Space / Right / Left", "Next / previous step"),
    Shortcut::new("Show", "B / W", "A black / white screen"),
    Shortcut::new("Show", "Escape", "End the show"),
];

/// The data root the kit resolved (`--data-dir`, `AZLIN_DATA`, the user's
/// data folder).
fn kit_data_root(kit_ref: &RefAny) -> PathBuf {
    let mut k = kit_ref.clone();
    k.downcast_ref::<kit::Kit>()
        .map_or_else(|| PathBuf::from(azul_appkit::data::ROOT_DIR), |k| k.data_root.clone())
}

extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let handle = data.clone();
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let st = &mut *guard;
    // appkit's --shot.
    if let Some(kit_ref) = st.kit.clone() {
        kit::on_window_created(&kit_ref, &mut info);
    }
    let mut update = Update::DoNothing;
    if let Some(id) = st.args.open.clone() {
        commands::spawn(&mut info, &handle, st, Job::Load { id });
    }
    match st.args.screen {
        StartScreen::BackstageOpen => commands::spawn(&mut info, &handle, st, Job::List),
        StartScreen::Show if st.editor.is_some() => update = commands::start_show(st, &mut info, false),
        _ => {}
    }
    println!("AZSHOW_READY");
    update
}

/// Starts AzShow on azul-appkit: the kit reads the settings file (the app
/// theme and the mode the user picked last time), the data root and the
/// switches; the window is the kit's (`NoTitle`, `--size`, a minimum).
pub fn start(args: Args) {
    let kit_ref = kit::create_kit(args::SPEC, ABOUT, &SHORTCUTS, &[], args.kit.clone());
    let root = kit_data_root(&kit_ref);
    let mut st = AppState::new(args.clone(), root.clone());
    st.kit = Some(kit_ref.clone());
    // Slide Show > Monitors, as it was left.
    st.presenter_monitor = {
        let mut k = kit_ref.clone();
        let value = k
            .downcast_ref::<kit::Kit>()
            .and_then(|k| k.settings.get(crate::app::PresenterMonitor::SETTING).map(str::to_string));
        crate::app::PresenterMonitor::parse(value.as_deref())
    };
    if args.sample() {
        let deck = model::sample_deck(&commands::new_deck_id(), themes::theme(1, 0, 0));
        commands::open_deck(&mut st, deck);
    }
    match args.screen {
        StartScreen::Normal | StartScreen::Show => {}
        StartScreen::Sorter => st.view = View::Sorter,
        StartScreen::Outline => st.view = View::Outline,
        StartScreen::Notes => st.view = View::NotesPage,
        StartScreen::BackstageNew => {
            st.screen = Screen::Backstage;
            st.page = BackstagePage::New;
        }
        StartScreen::BackstageOpen => {
            st.screen = Screen::Backstage;
            st.page = BackstagePage::Open;
        }
        // The kit's settings page over the window; OK / Cancel show the start screen.
        StartScreen::Options => kit::open_settings(&kit_ref, None),
    }
    eprintln!("[azshow] data root {}", root.display());
    let config = kit::app_config(&kit_ref);
    let window = kit::window_options(&kit_ref, layout, (1280.0, 800.0), (800.0, 520.0), on_window_created);
    App::create(RefAny::new(st), config).run(window);
}

#[cfg(test)]
mod ids_tests;
