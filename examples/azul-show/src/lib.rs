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
//! `AZSHOW_SHOW_ENDED`, `AZSHOW_SHOW_END`, `AZSHOW_SAVED <id>`,
//! `AZSHOW_OPENED <id>`, `AZSHOW_LISTED <n>`, `AZSHOW_EXPORTED <kind> <bytes>`.

pub mod app;
pub mod args;
mod backstage;
pub mod commands;
pub mod editor;
pub mod model;
pub mod render;
mod ribbon;
mod show;
pub mod storage;
pub mod text;
pub mod themes;
pub mod views;

use std::path::PathBuf;

use azul::{
    app::{App, AppConfig},
    callbacks::{CallbackInfo, LayoutCallbackInfo, RefAny, TimerCallbackInfo, TimerCallbackReturn, Update},
    css::{DarkLightMode, EventFilter},
    dom::{Callback, Dom, DomId, VirtualKeyCode},
    error::ResultRawImageDecodeImageError,
    file::FilePath,
    image::{ImageRef, RawImage},
    option::OptionDarkLightMode,
    shells::{DocumentShell, ShellEmptyState, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    svg::{CssPath, CssPathSelector},
    task::{
        TerminateTimer, Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg,
        Timer, TimerId,
    },
    time::{Duration, SystemTimeDiff},
    vec::U8VecRef,
    widgets::{ThumbnailStripLayout, Titlebar},
    window::{WindowCreateOptions, WindowDecorations, WindowEventFilter},
};

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
    match st.editor.as_ref().map_or(1, |e| themes::index_of(&e.deck.theme)) {
        2 => ShellThemeAccent::Leaf,
        3 => ShellThemeAccent::Plum,
        4 => ShellThemeAccent::Clay,
        5 => ShellThemeAccent::Slate,
        _ => ShellThemeAccent::Blue,
    }
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
fn editor_window(app: &RefAny, st: &AppState, w: f32, h: f32, theme: &str) -> Dom {
    let title = title_row(st, "");
    if st.screen == Screen::Backstage {
        return DocumentShell::create(Dom::create_div())
            .with_title_row(title)
            .with_backstage(backstage::backstage(app, st, theme, st.mode_choice))
            .dom();
    }
    let zoom = st.zoom_percent(w, h);
    let status = views::status_bar(app, st, zoom);
    let ribbon = ribbon::ribbon(app, st);
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
        .with_title_row(title)
        .with_ribbon(ribbon)
        .with_status_bar(status)
        .with_navigation_ratio(NAVIGATION_RATIO)
        .with_document_ratio(DOCUMENT_RATIO);
    if let Some(nav) = navigation {
        shell = shell.with_navigation(nav);
    }
    if let Some(side) = side {
        shell = shell.with_side_pane(side);
    }
    shell.dom()
}

/// The main window's layout.
extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    let app = data.clone();
    // Reading the mode and the theme makes a switch of either rebuild the window.
    let _mode = info.get_mode();
    let theme = info.get_theme().as_str().to_string();
    let (w, h) = (info.get_window_width(), info.get_window_height());
    let Some(guard) = data.downcast_ref::<AppState>() else {
        return Dom::create_body();
    };
    let st = &*guard;
    let content = if st.screen == Screen::Show {
        show::show_screen(&app, st, w, h)
    } else {
        editor_window(&app, st, w, h, &theme)
    };
    Dom::create_body()
        .with_css("display: flex; flex-direction: column; margin: 0px; padding: 0px; height: 100%;")
        .with_child(ShellThemeScope::create(content).with_accent(accent_of(st)).dom())
        .with_callback(EventFilter::Window(WindowEventFilter::VirtualKeyDown), app, on_window_key)
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
                selectors: vec![CssPathSelector::Id(AzString::from(text::host_id(id)))].into(),
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
    root: PathBuf,
}

struct JobDone {
    outcome: Option<Outcome>,
}

/// Runs `job` on a storage thread over the drive at `root`.
pub fn spawn_storage(info: &mut CallbackInfo, app: &RefAny, root: PathBuf, job: Job) {
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(JobInit { job: Some(job), root }),
            app.clone(),
            storage_thread,
        ),
    );
}

extern "C" fn storage_thread(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((job, root)) = init.downcast_mut::<JobInit>().and_then(|mut i| {
        let job = i.job.take()?;
        Some((job, i.root.clone()))
    }) else {
        return;
    };
    let drive = storage::local_drive(root);
    let outcome = storage::run_job(&drive, job);
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

extern "C" fn on_storage_done(mut app: RefAny, mut msg: RefAny, _info: CallbackInfo) -> Update {
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
        }
        Outcome::Saved(Err(e)) => st.message = format!("Not saved: {e}"),
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
fn editor_shortcut(key: VirtualKeyCode, primary: bool, shift: bool, slide_keys: bool, screen: Screen) -> Option<Command> {
    use VirtualKeyCode as K;
    Some(match key {
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
    let ks = info.get_current_keyboard_state();
    let Some(key) = ks.current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    let mods = info.get_key_modifiers();
    let primary = mods.ctrl || mods.meta;
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
            let cmd = editor_shortcut(key, primary, mods.shift, slide_keys, st.screen);
            // Ctrl+B / I / U are the slide's while a text is edited (the
            // selection's format) or the canvas has the keys.
            match cmd {
                Some(Command::Bold | Command::Italic | Command::Underline) if !editing && !slide_keys => None,
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

struct ShotConfig {
    path: PathBuf,
}

extern "C" fn shot_tick(mut data: RefAny, info: TimerCallbackInfo) -> TimerCallbackReturn {
    let Some(path) = data.downcast_ref::<ShotConfig>().map(|c| c.path.clone()) else {
        return TimerCallbackReturn {
            should_update: Update::DoNothing,
            should_terminate: TerminateTimer::Terminate,
        };
    };
    match info.callback_info.take_screenshot(DomId { inner: 0 }).into_result() {
        Ok(png) => match std::fs::write(&path, png.as_ref()) {
            Ok(()) => {
                eprintln!("[azshow] screenshot written: {}", path.display());
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("[azshow] screenshot FAILED: {e}");
                std::process::exit(2);
            }
        },
        Err(e) => {
            eprintln!("[azshow] screenshot FAILED: {}", e.as_str());
            std::process::exit(2);
        }
    }
}

extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let handle = data.clone();
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let st = &mut *guard;
    let mut update = Update::DoNothing;
    if let Some(id) = st.args.open.clone() {
        commands::spawn(&mut info, &handle, st, Job::Load { id });
    }
    match st.args.screen {
        StartScreen::BackstageOpen => commands::spawn(&mut info, &handle, st, Job::List),
        StartScreen::Show if st.editor.is_some() => update = commands::start_show(st, &mut info, false),
        _ => {}
    }
    if let Some(path) = st.args.shot.clone() {
        let timer = Timer::create(RefAny::new(ShotConfig { path }), shot_tick, info.get_system_time_fn())
            .with_delay(Duration::System(SystemTimeDiff::from_millis(st.args.shot_delay_ms)));
        info.add_timer(TimerId::unique(), timer);
    }
    println!("AZSHOW_READY");
    update
}

/// Starts AzShow with `args`.
pub fn start(args: Args) {
    let user_data = FilePath::get_data_dir()
        .into_option()
        .map(|p| PathBuf::from(p.inner.as_str()))
        .filter(|p| !p.as_os_str().is_empty());
    let root = storage::data_root(user_data);
    let mut st = AppState::new(args.clone(), root.clone());
    if args.sample {
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
    }
    if let Some(dark) = args.mode.as_deref().map(|m| m == "dark") {
        st.mode_choice = Some(dark);
    }
    eprintln!("[azshow] data root {}", root.display());

    let mut config = AppConfig::create();
    if let Some(theme) = args.theme.as_deref() {
        config.set_theme(s(theme));
    }
    if let Some(dark) = st.mode_choice {
        config.set_mode(OptionDarkLightMode::Some(if dark {
            DarkLightMode::Dark
        } else {
            DarkLightMode::Light
        }));
    }
    let app = App::create(RefAny::new(st), config);
    let mut window = WindowCreateOptions::create(layout);
    window.window_state.title = s("AzShow");
    window.window_state.flags.decorations = WindowDecorations::NoTitle;
    let (w, h) = args.size.unwrap_or((1280.0, 800.0));
    window.window_state.size.dimensions.width = w;
    window.window_state.size.dimensions.height = h;
    window.create_callback = Some(Callback::create(on_window_created)).into();
    app.run(window);
}
