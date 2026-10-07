//! The UI the Azlin apps share (feature `azul`): one [`Kit`] per app.
//!
//! The app keeps the kit's `RefAny` in its own state and asks it for:
//! - the window ([`window_options`]: `NoTitle`, `--size`, a compact minimum
//!   size) and the app config ([`app_config`]: the app theme and mode from
//!   `settings.json`, a `--theme` / `--mode` switch winning for one run);
//! - the title row ([`title_row`]: azul's `Titlebar`, drawn by the app as
//!   every azul app does);
//! - the settings page ([`settings_page`]) in the shape of Outlook 2010's
//!   Options dialog ([`crate::options`]): the categories on the left - the
//!   app's own first, then General (theme and mode, saved to `settings.json`
//!   and applied at once), Data (the data folder), Shortcuts (the app's
//!   table) and About - and on the right the chosen category's header line
//!   over its sections, each a band with its rows; OK keeps the changes,
//!   Cancel puts back the settings the page found. Mod+, opens it, F1 opens
//!   it at the shortcuts, Escape is Cancel ([`handle_key`]);
//! - file jobs on an azul `Thread` ([`spawn_file_jobs`]), so no callback ever
//!   waits on the disk (or, later, the network);
//! - the `--shot` screenshot ([`on_window_created`]).
//!
//! - the About box: azul's standard `AboutDialog` in a `Modal`, opened from
//!   the settings page's About section (its rows are
//!   [`crate::about::about_rows`]); Escape closes it first.

use std::path::{Path, PathBuf};

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CallbackType, LayoutCallbackType, SegmentedOnChangeCallbackType,
        TimerCallbackInfo, TimerCallbackReturn, WriteBackCallbackType,
    },
    css::DarkLightMode,
    dom::{DomId, TabIndex, VirtualKeyCode},
    file::FilePath,
    option::{OptionDarkLightMode, OptionLogicalSize},
    prelude::*,
    str::String as AzString,
    task::{
        Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg,
        Timer, TimerId,
    },
    time::{Duration, SystemTimeDiff},
    widgets::{
        AboutDialog, Button, ButtonType, Modal, ModalState, Segmented, SegmentedState,
        StandardDialogEvent, Titlebar,
    },
    window::{WindowCreateOptions, WindowDecorations},
};
use azul_storage::LocalDrive;

use crate::{
    about::{about_rows, AboutInfo},
    args::{AppArgs, AppSpec, ModePref, Theme},
    data::{self, app_key},
    files::{run_jobs, FileJob, FileOutcome},
    migrate,
    options::{categories, category_id, category_index, Category, Snapshot},
    settings::{AppSettings, SETTINGS_FILE},
    shortcuts::{display_keys, groups, Shortcut, KIT_SHORTCUTS},
};

/// The kit's categories, after the app's own ([`crate::options::KIT_CATEGORIES`]).
pub use crate::options::KIT_CATEGORIES;

// ==== The look of the settings page (Outlook 2010's Options dialog) ====
//
// The colours are the theme's (`system:` colours), named here so a palette - Office 2010's
// silver and orange, or any other - can be swapped in without touching the page.

/// The dialog's own surface: around the two panes and under OK / Cancel.
const DIALOG_BG: &str = "system:window-background";
/// The category list's and the options pane's surface (Outlook's white panes).
const PANE_BG: &str = "system:control-background";
/// The frame of the two panes, and the rules between the groups of categories.
const PANE_EDGE: &str = "system:separator";
/// The selected category (Outlook 2010: the orange bar).
const SELECTED_BG: &str = "system:selection-background";
/// The selected category's text.
const SELECTED_TEXT: &str = "system:selection-text";
/// A category under the pointer.
const HOVER_BG: &str = "system:selection-background-inactive";
/// A section's band (Outlook's grey "User Interface options" bar over the rows).
const BAND_BG: &str = "system:window-background";
/// The header line's text ("General options for working with AzNotes.") and a row's note.
const QUIET_TEXT: &str = "system:secondary-text";
/// The header line's icon.
const HEADER_ICON: &str = "system:accent";
/// The width of the category list (it gives way down to 100 px in a narrow window).
const CATEGORY_WIDTH: &str = "180px";
/// The width of a row's label column (it gives way down to 72 px).
const LABEL_WIDTH: &str = "160px";

/// The write-back tag of a settings save (the kit's own jobs).
const SETTINGS_TAG: u64 = u64::MAX;

/// One app's kit: its settings, its data root and the settings page's state.
pub struct Kit {
    pub spec: AppSpec,
    pub about: AboutInfo,
    /// The app's shortcuts, then the kit's.
    pub shortcuts: Vec<Shortcut>,
    pub args: AppArgs,
    pub settings: AppSettings,
    /// The data root (the bucket's root, later).
    pub data_root: PathBuf,
    /// The settings page is showing.
    pub settings_open: bool,
    /// The settings page's category (an index into [`Kit::categories`]).
    pub category: usize,
    /// The app's own settings categories (first on the page).
    pub app_categories: Vec<String>,
    /// The last problem saving or reading the settings ("" = none).
    pub notice: String,
    /// Cmd (macOS) or Ctrl.
    pub mac: bool,
    /// The About box (azul's standard `AboutDialog`) is open.
    pub about_open: bool,
    /// The settings as the page found them when it opened: what Cancel puts back.
    snapshot: Option<Snapshot>,
    /// While the page shows: how the app re-reads its own copies of its values after Cancel
    /// ([`settings_page_with_reload`]); dropped when the page closes.
    reload: Option<Reload>,
    /// The app's key handler calls [`handle_key`]: Escape is handled there, so the page's own
    /// key handler (for the apps that do not route their keys through the kit) stays out.
    keys_routed: bool,
}

/// What an app does after Cancel put its settings back: re-read its own copies of its values
/// from `settings` (the restored ones). `app` is the data the app handed to
/// [`settings_page_with_reload`]; the kit is not borrowed while this runs.
pub type ReloadSettings = fn(app: &mut RefAny, info: &mut CallbackInfo, settings: &AppSettings);

/// An app's [`ReloadSettings`] with its data.
#[derive(Clone)]
struct Reload {
    app: RefAny,
    reload: ReloadSettings,
}

impl Kit {
    /// Every category of the settings page: the app's, then the kit's.
    #[must_use]
    pub fn categories(&self) -> Vec<String> {
        categories(&self.app_categories)
    }

    /// The key of the settings file.
    #[must_use]
    pub fn settings_key(&self) -> String {
        app_key(self.about.app_folder, SETTINGS_FILE)
    }

    /// An app key in this app's folder.
    #[must_use]
    pub fn key(&self, name: &str) -> String {
        app_key(self.about.app_folder, name)
    }

    /// The theme and mode this run shows.
    #[must_use]
    pub fn effective(&self) -> (Theme, ModePref) {
        self.settings.effective(&self.args)
    }
}

/// The user's data folder from the OS.
fn os_data_dir() -> Option<PathBuf> {
    FilePath::get_data_dir()
        .into_option()
        .map(|dir| PathBuf::from(dir.inner.as_str()))
        .filter(|p| !p.as_os_str().is_empty())
}

/// Makes the kit of an app: resolves the data root and reads the settings
/// file. Called from `start()` before the window exists (not from a
/// callback): the theme has to be known before the first frame.
#[must_use]
pub fn create_kit(
    spec: AppSpec,
    about: AboutInfo,
    app_shortcuts: &[Shortcut],
    app_categories: &[&str],
    args: AppArgs,
) -> RefAny {
    let os_dir = os_data_dir();
    let data_root = data::data_root(
        args.data_dir.as_deref(),
        std::env::var(data::DATA_VAR).ok().as_deref(),
        os_dir.clone(),
    );
    // One data root for every app: this app's folder from the data folder an
    // older build used (`azul/`, `Azul/`, `AzNotes/`) moves in, once.
    if let Some(os_dir) = os_dir.as_deref() {
        let migration = migrate::migrate_app_data(os_dir, &data_root, about.app_folder);
        if !migration.moved.is_empty() || !migration.failed.is_empty() {
            eprintln!("[{}] {}", spec.binary, migration.summary(&data_root));
        }
    }
    let drive = LocalDrive::new(&data_root);
    let key = app_key(about.app_folder, SETTINGS_FILE);
    let mut notice = String::new();
    let settings = match run_jobs(&drive, vec![FileJob::Get { key: key.clone() }]).pop() {
        Some(FileOutcome::Got {
            result: Ok(Some(bytes)),
            ..
        }) => {
            let (settings, problem) = AppSettings::parse(&String::from_utf8_lossy(&bytes));
            if let Some(problem) = problem {
                notice = format!("The settings file could not be read fully ({problem}).");
                eprintln!("[{}] {key}: {problem}", spec.binary);
            }
            settings
        }
        Some(FileOutcome::Got { result: Err(e), .. }) => {
            notice = format!("The settings file could not be read: {e}");
            AppSettings::default()
        }
        _ => AppSettings::default(),
    };
    let mut shortcuts = app_shortcuts.to_vec();
    shortcuts.extend(KIT_SHORTCUTS);
    RefAny::new(Kit {
        spec,
        about,
        shortcuts,
        args,
        settings,
        data_root,
        settings_open: false,
        category: 0,
        app_categories: app_categories.iter().map(|c| (*c).to_string()).collect(),
        notice,
        mac: cfg!(target_os = "macos"),
        about_open: false,
        snapshot: None,
        reload: None,
        keys_routed: false,
    })
}

/// The mode azul is told: `None` follows the OS.
fn mode_option(mode: ModePref) -> OptionDarkLightMode {
    match mode {
        ModePref::System => OptionDarkLightMode::None,
        ModePref::Light => OptionDarkLightMode::Some(DarkLightMode::Light),
        ModePref::Dark => OptionDarkLightMode::Some(DarkLightMode::Dark),
    }
}

/// The app config: the app theme and the mode this run uses.
#[must_use]
pub fn app_config(kit: &RefAny) -> AppConfig {
    let mut kit = kit.clone();
    let (theme, mode) = match kit.downcast_ref::<Kit>() {
        Some(k) => k.effective(),
        None => (Theme::default(), ModePref::default()),
    };
    AppConfig::create()
        .with_theme(theme.name())
        .with_mode(mode_option(mode))
}

/// The main window: the app's title, `NoTitle` (the app draws the title row),
/// `--size` or the default size, a minimum size, the create callback.
#[must_use]
pub fn window_options(
    kit: &RefAny,
    layout: LayoutCallbackType,
    default_size: (f32, f32),
    min_size: (f32, f32),
    on_create: CallbackType,
) -> WindowCreateOptions {
    let mut kit = kit.clone();
    let (name, size) = match kit.downcast_ref::<Kit>() {
        Some(k) => (k.spec.name.to_string(), k.args.size.unwrap_or(default_size)),
        None => (String::new(), default_size),
    };
    let mut window = WindowCreateOptions::create(layout);
    window.window_state.title = AzString::from(name.as_str());
    window.window_state.size.dimensions = LogicalSize::create(size.0, size.1);
    window.window_state.size.min_dimensions =
        OptionLogicalSize::Some(LogicalSize::create(min_size.0, min_size.1));
    window.window_state.flags.decorations = WindowDecorations::NoTitle;
    window.create_callback = Some(Callback::create(on_create)).into();
    window
}

/// The window's title row: azul's `Titlebar` (the window is `NoTitle`).
#[must_use]
pub fn title_row(title: &str) -> Dom {
    Titlebar::create(title).without_border_bottom().dom()
}

/// Called from the app's window-created callback: starts the `--shot` timer.
pub fn on_window_created(kit: &RefAny, info: &mut CallbackInfo) {
    let mut kit = kit.clone();
    let shot = kit
        .downcast_ref::<Kit>()
        .and_then(|k| k.args.shot.clone().map(|p| (p, k.args.shot_delay_ms)));
    if let Some((path, delay_ms)) = shot {
        let timer = Timer::create(RefAny::new(ShotConfig { path }), shot_tick, info.get_system_time_fn())
            .with_delay(Duration::System(SystemTimeDiff::from_millis(delay_ms)));
        info.add_timer(TimerId::unique(), timer);
    }
}

struct ShotConfig {
    path: PathBuf,
}

/// `--shot`: the window as a PNG, written, then the process ends (0 = written).
extern "C" fn shot_tick(mut data: RefAny, info: TimerCallbackInfo) -> TimerCallbackReturn {
    let Some(path) = data.downcast_ref::<ShotConfig>().map(|c| c.path.clone()) else {
        return TimerCallbackReturn::terminate_unchanged();
    };
    match info.callback_info.take_screenshot(DomId { inner: 0 }).into_result() {
        Ok(png) => match std::fs::write(&path, png.as_slice()) {
            Ok(()) => {
                eprintln!("[appkit] screenshot written: {}", path.display());
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("[appkit] screenshot not written to {}: {e}", path.display());
                std::process::exit(2);
            }
        },
        Err(e) => {
            eprintln!("[appkit] screenshot failed: {}", e.as_str());
            std::process::exit(2);
        }
    }
}

// ==== File jobs on a Thread ====

/// What a file thread hands back: the app's tag and every job's outcome.
pub struct FileReply {
    pub tag: u64,
    pub outcomes: Vec<FileOutcome>,
}

/// Takes the reply out of a write-back's message (`None` if it is not one).
#[must_use]
pub fn take_reply(msg: &mut RefAny) -> Option<FileReply> {
    let mut guard = msg.downcast_mut::<FileReply>()?;
    Some(FileReply {
        tag: guard.tag,
        outcomes: std::mem::take(&mut guard.outcomes),
    })
}

struct FileThreadInit {
    root: PathBuf,
    /// `root` is the data tree (its drive keeps the `.azlin/cache`
    /// manifest); `false` for a folder outside it (`spawn_outside_read`).
    data_tree: bool,
    jobs: Option<Vec<FileJob>>,
    tag: u64,
    on_done: WriteBackCallbackType,
}

/// Runs on the worker thread: the jobs on the data root's drive, then the
/// outcomes to the UI thread.
extern "C" fn file_thread(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((root, data_tree, jobs, tag, on_done)) = init.downcast_mut::<FileThreadInit>().and_then(|mut i| {
        let jobs = i.jobs.take()?;
        Some((i.root.clone(), i.data_tree, jobs, i.tag, i.on_done))
    }) else {
        return;
    };
    let drive = if data_tree {
        LocalDrive::new(root)
    } else {
        LocalDrive::without_manifest(root)
    };
    let outcomes = run_jobs(&drive, jobs);
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_done,
        RefAny::new(FileReply { tag, outcomes }),
    )));
}

/// Runs `jobs` (in order) on an azul `Thread` against the drive at `root`;
/// `on_done(reply_to, FileReply, info)` gets the outcomes on the UI thread.
pub fn spawn_file_jobs(
    info: &mut CallbackInfo,
    root: &Path,
    jobs: Vec<FileJob>,
    reply_to: RefAny,
    tag: u64,
    on_done: WriteBackCallbackType,
) {
    if jobs.is_empty() {
        return;
    }
    spawn_jobs_at(info, root, true, jobs, reply_to, tag, on_done);
}

/// Reads `path` - a file the user picked OUTSIDE the data tree (a file to
/// import) - on an azul `Thread`, through a drive at its folder that keeps
/// no manifest ([`crate::files::outside_read`]); `on_done` gets one
/// `FileOutcome::Got` whose key is the file's name. `false` (nothing
/// spawned) for a path without a UTF-8 file name.
pub fn spawn_outside_read(
    info: &mut CallbackInfo,
    path: &Path,
    reply_to: RefAny,
    tag: u64,
    on_done: WriteBackCallbackType,
) -> bool {
    let Some((folder, job)) = crate::files::outside_read(path) else {
        return false;
    };
    spawn_jobs_at(info, &folder, false, vec![job], reply_to, tag, on_done);
    true
}

/// The one file thread: `jobs` on the drive at `root` (`data_tree`: the
/// data root's drive with its manifest; else a folder outside it).
fn spawn_jobs_at(
    info: &mut CallbackInfo,
    root: &Path,
    data_tree: bool,
    jobs: Vec<FileJob>,
    reply_to: RefAny,
    tag: u64,
    on_done: WriteBackCallbackType,
) {
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(FileThreadInit {
                root: root.to_path_buf(),
                data_tree,
                jobs: Some(jobs),
                tag,
                on_done,
            }),
            reply_to,
            file_thread,
        ),
    );
}

/// Saves the kit's settings file (on a Thread).
pub fn save_settings(kit_ref: &RefAny, info: &mut CallbackInfo) {
    let mut kit = kit_ref.clone();
    let Some((root, key, json)) = kit
        .downcast_ref::<Kit>()
        .map(|k| (k.data_root.clone(), k.settings_key(), k.settings.to_json()))
    else {
        return;
    };
    spawn_file_jobs(
        info,
        &root,
        vec![FileJob::Put {
            key,
            bytes: json.into_bytes(),
        }],
        kit_ref.clone(),
        SETTINGS_TAG,
        on_settings_saved,
    );
}

extern "C" fn on_settings_saved(mut kit: RefAny, mut msg: RefAny, _info: CallbackInfo) -> Update {
    let Some(reply) = take_reply(&mut msg) else {
        return Update::DoNothing;
    };
    let Some(mut k) = kit.downcast_mut::<Kit>() else {
        return Update::DoNothing;
    };
    match reply.outcomes.iter().find_map(FileOutcome::error) {
        Some(e) => {
            k.notice = format!("The settings could not be saved: {e}");
            println!("{}_SETTINGS_ERROR {e}", k.spec.binary.to_uppercase());
        }
        None => {
            k.notice.clear();
            println!("{}_SETTINGS_SAVED {}", k.spec.binary.to_uppercase(), k.settings_key());
        }
    }
    Update::RefreshDom
}

/// Sets one of the app's own values and saves the settings file.
pub fn set_value(kit_ref: &RefAny, info: &mut CallbackInfo, key: &str, value: &str) {
    {
        let mut kit = kit_ref.clone();
        let Some(mut k) = kit.downcast_mut::<Kit>() else {
            return;
        };
        if k.settings.get(key) == Some(value) {
            return;
        }
        k.settings.set(key, value);
    }
    save_settings(kit_ref, info);
}

// ==== Opening and closing the settings ====

/// Shows the settings page, at the category named `category` if given ("Appearance" finds
/// General). The settings as they are now are what Cancel puts back; opening the page again
/// while it shows (F1 on it) keeps the first ones.
pub fn open_settings(kit_ref: &RefAny, category: Option<&str>) {
    let mut kit = kit_ref.clone();
    if let Some(mut k) = kit.downcast_mut::<Kit>() {
        if !k.settings_open || k.snapshot.is_none() {
            let snapshot = Snapshot::take(&k.settings, &k.args);
            k.snapshot = Some(snapshot);
        }
        k.settings_open = true;
        if let Some(name) = category {
            if let Some(i) = category_index(&k.categories(), name) {
                k.category = i;
            }
        }
    };
}

/// Hides the settings page, keeping every change (the page's OK; an app's own Back).
pub fn close_settings(kit_ref: &RefAny) {
    let mut kit = kit_ref.clone();
    if let Some(mut k) = kit.downcast_mut::<Kit>() {
        k.settings_open = false;
        k.snapshot = None;
        k.reload = None;
    };
}

/// The page's Cancel (and Escape): puts back the settings the page found when it opened - the
/// theme and the mode shown again at once, `settings.json` written again - and hides the page.
/// The app's [`ReloadSettings`] (if it gave one) then re-reads its own copies of its values.
/// `Some` = the settings changed back.
pub fn cancel_settings(kit_ref: &RefAny, info: &mut CallbackInfo) -> Option<AppSettings> {
    let mut kit = kit_ref.clone();
    let (restored, settings, reload) = {
        let mut guard = kit.downcast_mut::<Kit>()?;
        let k: &mut Kit = &mut guard;
        let open = k.settings_open;
        k.settings_open = false;
        k.about_open = false;
        let reload = k.reload.take();
        let snapshot = k.snapshot.take()?;
        if open {
            println!("{}_SETTINGS_CLOSED cancel", k.spec.binary.to_uppercase());
        }
        let restored = snapshot.restore(&mut k.settings, &mut k.args);
        (restored, k.settings.clone(), reload)
    };
    if let Some((theme, mode)) = restored.look {
        info.set_theme(theme.name());
        info.set_mode(mode_option(mode));
    }
    if restored.save {
        save_settings(kit_ref, info);
    }
    if !restored.save && restored.look.is_none() {
        return None;
    }
    if let Some(Reload { mut app, reload }) = reload {
        reload(&mut app, info, &settings);
    }
    Some(settings)
}

/// Whether the settings page is showing.
#[must_use]
pub fn settings_open(kit_ref: &RefAny) -> bool {
    let mut kit = kit_ref.clone();
    kit.downcast_ref::<Kit>().is_some_and(|k| k.settings_open)
}

/// The kit's keys, for the app's window key handler to call first:
/// Mod+, opens the settings, F1 opens them at the shortcuts, Escape closes
/// the About box or cancels the settings. `Some` = handled (the app returns
/// it), `None` = the app's key.
pub fn handle_key(kit_ref: &RefAny, info: &mut CallbackInfo) -> Option<Update> {
    {
        let mut kit = kit_ref.clone();
        if let Some(mut k) = kit.downcast_mut::<Kit>() {
            k.keys_routed = true;
        };
    }
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option()?;
    let modifiers = info.get_key_modifiers();
    let command = modifiers.primary_down();
    match key {
        VirtualKeyCode::Comma if command => {
            open_settings(kit_ref, None);
            info.prevent_default();
            Some(Update::RefreshDom)
        }
        VirtualKeyCode::F1 => {
            open_settings(kit_ref, Some("Shortcuts"));
            info.prevent_default();
            Some(Update::RefreshDom)
        }
        VirtualKeyCode::Escape if about_open(kit_ref) || settings_open(kit_ref) => {
            escape(kit_ref, info);
            info.prevent_default();
            Some(Update::RefreshDom)
        }
        _ => None,
    }
}

/// Escape on the page: the About box closes first, then Cancel.
fn escape(kit_ref: &RefAny, info: &mut CallbackInfo) {
    if about_open(kit_ref) {
        set_about_open(kit_ref, false);
    } else {
        let _changed_back = cancel_settings(kit_ref, info);
    }
}

// ==== The settings page ====

/// One of the app's own sections: its category (an index into the app's
/// categories), its title (the band over it) and its content (its rows).
pub struct AppSection {
    pub category: usize,
    pub title: String,
    pub content: Dom,
}

/// A run of text (an inline span); the kit's pieces are in [`crate::pieces`].
pub use crate::pieces::text;
use crate::pieces::{column, strs};

/// A settings row: a label column and the control (Outlook's "Color scheme: [Silver]").
#[must_use]
pub fn row(label: &str, control: Dom) -> Dom {
    Dom::create_div()
        .with_class("appkit-row")
        .with_css(
            "display: flex; flex-direction: row; align-items: center; padding: 4px 0px; \
             min-width: 0px;",
        )
        .with_child(
            Dom::create_div()
                .with_css(format!(
                    "width: {LABEL_WIDTH}; flex-shrink: 1; min-width: 72px; padding-right: 8px; \
                     font-size: 13px;"
                ))
                .with_child(text(label)),
        )
        .with_child(control)
}

/// A line of secondary text under a section's rows.
#[must_use]
pub fn note(content: &str) -> Dom {
    Dom::create_div()
        .with_css(format!("padding: 4px 0px; font-size: 12px; color: {QUIET_TEXT};"))
        .with_child(text(content))
}

/// A section's band: its title in bold on Outlook's grey bar.
fn band(title: &str) -> Dom {
    Dom::create_div()
        .with_class("appkit-band")
        .with_css(format!(
            "padding: 4px 8px; margin-top: 12px; background: {BAND_BG}; font-size: 13px; \
             font-weight: bold;"
        ))
        .with_child(text(title))
}

/// A section: its band over its rows, the rows indented under it.
fn section(title: &str, content: Dom) -> Dom {
    column(
        "flex-shrink: 0; min-width: 0px;",
        vec![
            band(title),
            Dom::create_div()
                .with_css("padding: 6px 4px 4px 20px; min-width: 0px;")
                .with_child(content),
        ],
    )
}

/// The line over a category's sections: a big icon and what the category is for.
fn header_line(icon: &str, line: &str) -> Dom {
    Dom::create_div()
        .with_id("appkit-settings-header")
        .with_css(
            "display: flex; flex-direction: row; align-items: center; padding: 2px 0px 6px \
             0px; flex-shrink: 0;",
        )
        .with_child(Dom::create_icon(icon).with_css(format!(
            "font-size: 32px; color: {HEADER_ICON}; margin-right: 12px; flex-shrink: 0;"
        )))
        .with_child(
            Dom::create_div()
                .with_css(format!(
                    "flex-grow: 1; min-width: 0px; font-size: 15px; color: {QUIET_TEXT};"
                ))
                .with_child(text(line)),
        )
}

fn appearance_section(k: &Kit, kit_ref: &RefAny) -> Dom {
    let theme_labels: Vec<&str> = Theme::ALL.iter().map(|t| t.label()).collect();
    let mode_labels: Vec<&str> = ModePref::ALL.iter().map(|m| m.label()).collect();
    let mut rows = vec![
        row(
            "Theme",
            Segmented::create(strs(&theme_labels))
                .with_selected_index(k.settings.theme.index())
                .with_on_change(kit_ref.clone(), on_theme as SegmentedOnChangeCallbackType)
                .dom()
                .with_id("appkit-theme"),
        ),
        row(
            "Mode",
            Segmented::create(strs(&mode_labels))
                .with_selected_index(k.settings.mode.index())
                .with_on_change(kit_ref.clone(), on_mode as SegmentedOnChangeCallbackType)
                .dom()
                .with_id("appkit-mode"),
        ),
    ];
    if k.args.theme.is_some() || k.args.mode.is_some() {
        rows.push(note(
            "A --theme or --mode switch overrides these settings until the app restarts.",
        ));
    }
    column("", rows)
}

fn data_section(k: &Kit) -> Dom {
    let folder = data::local_path(&k.data_root, k.about.app_folder);
    column("", vec![
        row(
            "Data folder",
            Dom::create_div()
                .with_id("appkit-data-folder")
                .with_css(
                    "flex-grow: 1; min-width: 0px; font-size: 13px; overflow-wrap: anywhere;",
                )
                .with_child(text(folder.display().to_string())),
        ),
        note(
            "Your data are plain files in this folder, one folder per app. An S3 drive can \
             take the place of the folder later without changing them.",
        ),
    ])
}

/// The shortcuts: one section per group, a row per shortcut (the keys, what they do).
fn shortcuts_sections(k: &Kit) -> Dom {
    let sections = groups(&k.shortcuts)
        .into_iter()
        .map(|(group, items)| {
            let rows = items
                .iter()
                .map(|s| {
                    row(
                        &display_keys(s.keys, k.mac),
                        Dom::create_div()
                            .with_css("flex-grow: 1; min-width: 0px; font-size: 13px;")
                            .with_child(text(s.action)),
                    )
                })
                .collect();
            section(group, column("", rows))
        })
        .collect();
    column("min-width: 0px;", sections).with_id("appkit-shortcuts")
}

fn about_section(k: &Kit, kit_ref: &RefAny) -> Dom {
    let mut children = vec![
        Dom::create_div()
            .with_id("appkit-about-name")
            .with_css("font-size: 20px; font-weight: 600; padding-bottom: 4px;")
            .with_child(text(k.about.name)),
        note(k.about.summary),
    ];
    for (label, value) in about_rows(&k.about, &k.data_root) {
        children.push(row(
            &label,
            Dom::create_div()
                .with_css(
                    "flex-grow: 1; min-width: 0px; font-size: 13px; overflow-wrap: anywhere;",
                )
                .with_child(text(value)),
        ));
    }
    children.push(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; padding-top: 8px;")
            .with_child(
                Button::create(format!("About {}\u{2026}", k.about.name))
                    .with_icon("info")
                    .with_on_click(kit_ref.clone(), on_about_open as ButtonOnClickCallbackType)
                    .dom()
                    .with_id("appkit-about-open"),
            ),
    );
    column("", children)
}

// ==== The About box: azul's standard AboutDialog in a Modal ====

/// Whether the About box is open.
#[must_use]
pub fn about_open(kit_ref: &RefAny) -> bool {
    let mut kit = kit_ref.clone();
    kit.downcast_ref::<Kit>().is_some_and(|k| k.about_open)
}

/// Opens or closes the About box (stdout `<APP>_ABOUT open|closed`, for
/// scripts).
pub fn set_about_open(kit_ref: &RefAny, open: bool) {
    let mut kit = kit_ref.clone();
    if let Some(mut k) = kit.downcast_mut::<Kit>() {
        k.about_open = open;
        println!(
            "{}_ABOUT {}",
            k.spec.binary.to_uppercase(),
            if open { "open" } else { "closed" }
        );
    };
}

/// The About box: the app's facts in azul's `AboutDialog` (the name, the
/// version, the summary, the rows of the About section as credits), in a
/// `Modal` that is open while the kit says so. Part of the settings page.
fn about_modal(k: &Kit, kit_ref: &RefAny) -> Dom {
    let mut dialog = AboutDialog::create(k.about.name, k.about.version)
        .with_icon("info")
        .with_description(k.about.summary)
        .with_copyright(format!("{} - {}", k.about.name, k.about.license))
        .with_on_event(kit_ref.clone(), on_about_event);
    for (label, value) in about_rows(&k.about, &k.data_root) {
        dialog = dialog.with_credit(label, value);
    }
    Modal::create(dialog.dom())
        .with_title(format!("About {}", k.about.name))
        .with_open(k.about_open)
        .with_on_close(kit_ref.clone(), on_about_close)
        .dom()
        .with_id("appkit-about")
}

extern "C" fn on_about_open(kit: RefAny, _info: CallbackInfo) -> Update {
    set_about_open(&kit, true);
    Update::RefreshDom
}

/// The About box's Close (its one button).
extern "C" fn on_about_event(kit: RefAny, _info: CallbackInfo, _event: StandardDialogEvent) -> Update {
    set_about_open(&kit, false);
    Update::RefreshDom
}

extern "C" fn on_about_close(kit: RefAny, _info: CallbackInfo, _state: ModalState) -> Update {
    set_about_open(&kit, false);
    Update::RefreshDom
}

// ==== The page: the category list, the options pane, OK / Cancel ====

/// The category list: the app's categories, a rule, General / Data / Shortcuts, a rule, About
/// (Outlook's groups); the chosen one on the selection's colour.
fn category_list(kit_ref: &RefAny, categories: &[String], chosen: usize, app_count: usize) -> Dom {
    let rule = || {
        Dom::create_div().with_css(format!(
            "height: 1px; margin: 5px 8px; background: {PANE_EDGE}; flex-shrink: 0;"
        ))
    };
    let item = "padding: 6px 12px; margin: 1px 4px; font-size: 13px; border-radius: 2px; \
                cursor: pointer; flex-shrink: 0;";
    let mut items = Vec::with_capacity(categories.len() + 2);
    for (index, name) in categories.iter().enumerate() {
        if (index == app_count && app_count > 0)
            || Category::of(index, app_count) == Category::About
        {
            items.push(rule());
        }
        let css = if index == chosen {
            format!("{item} background: {SELECTED_BG}; color: {SELECTED_TEXT};")
        } else {
            format!("{item} color: system:text; :hover {{ background: {HOVER_BG}; }}")
        };
        let data = RefAny::new(CategoryRef {
            kit: kit_ref.clone(),
            index,
        });
        items.push(
            Dom::create_div()
                .with_id(category_id(name))
                .with_css(css)
                .with_tab_index(TabIndex::Auto)
                .with_child(text(name.as_str()))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    data.clone(),
                    on_category,
                )
                // Enter / Space on the focused category (the engine's keyboard activation).
                .with_callback(EventFilter::Hover(HoverEventFilter::Click), data, on_category),
        );
    }
    column(
        &format!(
            "width: {CATEGORY_WIDTH}; flex-shrink: 1; min-width: 100px; min-height: 0px; \
             padding: 4px 0px; background: {PANE_BG}; border: 1px solid {PANE_EDGE}; \
             overflow-y: auto; overflow-x: hidden;"
        ),
        items,
    )
    .with_id("appkit-settings-categories")
}

/// The settings page, in the shape of Outlook 2010's Options dialog: the categories on the
/// left (the app's, then General, Data, Shortcuts, About), on the right the chosen category's
/// header line over its sections (a band, the rows under it), OK and Cancel under both. A
/// change takes effect at once; OK keeps it, Cancel and Escape put back what the page found.
///
/// An app that keeps its own copies of its values (not only in the kit's settings) hands
/// [`settings_page_with_reload`] how to read them again after Cancel.
#[must_use]
pub fn settings_page(kit_ref: &RefAny, app_sections: Vec<AppSection>) -> Dom {
    options_page(kit_ref, app_sections, None)
}

/// [`settings_page`] for an app that keeps its own copies of its values: after Cancel put
/// the settings back, `reload(app, info, settings)` reads them again.
#[must_use]
pub fn settings_page_with_reload(
    kit_ref: &RefAny,
    app_sections: Vec<AppSection>,
    app: &RefAny,
    reload: ReloadSettings,
) -> Dom {
    options_page(
        kit_ref,
        app_sections,
        Some(Reload {
            app: app.clone(),
            reload,
        }),
    )
}

fn options_page(kit_ref: &RefAny, app_sections: Vec<AppSection>, reload: Option<Reload>) -> Dom {
    {
        // Cancel - the page's button or Escape in the app's key handler - finds the reload
        // here; it is dropped when the page closes.
        let mut kit = kit_ref.clone();
        if let Some(mut k) = kit.downcast_mut::<Kit>() {
            k.reload = reload;
        };
    }
    let mut kit = kit_ref.clone();
    let Some(k) = kit.downcast_ref::<Kit>() else {
        return Dom::create_div();
    };
    let app_count = k.app_categories.len();
    let categories = k.categories();
    let chosen = k.category.min(categories.len().saturating_sub(1));
    let kind = Category::of(chosen, app_count);
    let label = categories.get(chosen).cloned().unwrap_or_default();

    let mut pane = vec![header_line(kind.icon(), &kind.header(&label, k.about.name))];
    match kind {
        Category::App(index) => {
            for s in app_sections.into_iter().filter(|s| s.category == index) {
                pane.push(section(&s.title, s.content));
            }
        }
        Category::General => pane.push(section("Appearance", appearance_section(&k, kit_ref))),
        Category::Data => pane.push(section("Your data", data_section(&k))),
        Category::Shortcuts => pane.push(shortcuts_sections(&k)),
        Category::About => pane.push(section(
            &format!("About {}", k.about.name),
            about_section(&k, kit_ref),
        )),
    }

    let page_data = RefAny::new(PageRef {
        kit: kit_ref.clone(),
    });
    let body = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; flex-grow: 1; min-height: 0px; min-width: 0px; \
             padding: 10px 10px 0px 10px;",
        )
        .with_child(category_list(kit_ref, &categories, chosen, app_count))
        .with_child(
            // The scrolling pane holds one column that does not shrink: a long category
            // scrolls instead of squeezing its rows.
            Dom::create_div()
                .with_id("appkit-settings-pane")
                .with_css(format!(
                    "display: flex; flex-direction: column; flex-grow: 1; flex-shrink: 1; \
                     min-width: 0px; min-height: 0px; margin-left: 8px; background: {PANE_BG}; \
                     border: 1px solid {PANE_EDGE}; overflow-y: auto; overflow-x: hidden;"
                ))
                .with_child(column(
                    "flex-shrink: 0; min-width: 0px; padding: 12px 18px 18px 18px;",
                    pane,
                )),
        );

    let mut buttons = Dom::create_div().with_id("appkit-settings-buttons").with_css(
        "display: flex; flex-direction: row; align-items: center; padding: 10px; flex-shrink: 0;",
    );
    buttons.add_child(if k.notice.is_empty() {
        Dom::create_div().with_css("flex-grow: 1;")
    } else {
        Dom::create_div()
            .with_id("appkit-settings-notice")
            .with_css(format!(
                "flex-grow: 1; min-width: 0px; padding-right: 12px; font-size: 12px; color: \
                 {QUIET_TEXT};"
            ))
            .with_child(text(k.notice.as_str()))
    });
    buttons.add_child(
        Dom::create_div().with_css("min-width: 88px; margin-left: 8px;").with_child(
            Button::create("OK")
                .with_button_type(ButtonType::Primary)
                .with_on_click(page_data.clone(), on_ok as ButtonOnClickCallbackType)
                .dom()
                .with_id("appkit-settings-ok"),
        ),
    );
    buttons.add_child(
        Dom::create_div().with_css("min-width: 88px; margin-left: 8px;").with_child(
            Button::create("Cancel")
                .with_on_click(page_data.clone(), on_cancel as ButtonOnClickCallbackType)
                .dom()
                .with_id("appkit-settings-cancel"),
        ),
    );

    Dom::create_div()
        .with_id("appkit-settings")
        .with_css(format!(
            "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; min-width: \
             0px; background: {DIALOG_BG}; color: system:text;"
        ))
        // Escape for the apps that do not route their keys through `handle_key`.
        .with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            page_data,
            on_page_key,
        )
        .with_child(body)
        .with_child(buttons)
        .with_child(about_modal(&k, kit_ref))
}

// ==== The settings page's callbacks ====

/// A category of the list: the kit and its index.
struct CategoryRef {
    kit: RefAny,
    index: usize,
}

/// The page's OK / Cancel / Escape: the kit.
struct PageRef {
    kit: RefAny,
}

/// The kit of a page callback's data.
fn page_kit(data: &mut RefAny) -> Option<RefAny> {
    data.downcast_ref::<PageRef>().map(|p| p.kit.clone())
}

/// The user chose the app theme: in effect at once (every window), kept in
/// settings.json, and winning over a `--theme` switch from now on. The one
/// path the settings page and an app's own theme control take.
pub fn choose_theme(kit_ref: &RefAny, info: &mut CallbackInfo, theme: Theme) {
    let mut kit = kit_ref.clone();
    let save = match kit.downcast_mut::<Kit>() {
        Some(mut k) => {
            k.settings.theme = theme;
            k.args.theme = None; // the user's choice now wins over the switch
            true
        }
        None => false,
    };
    if save {
        info.set_theme(theme.name());
        save_settings(kit_ref, info);
    }
}

/// The user chose light, dark or the system's mode: [`choose_theme`]'s twin.
pub fn choose_mode(kit_ref: &RefAny, info: &mut CallbackInfo, mode: ModePref) {
    let mut kit = kit_ref.clone();
    let save = match kit.downcast_mut::<Kit>() {
        Some(mut k) => {
            k.settings.mode = mode;
            k.args.mode = None;
            true
        }
        None => false,
    };
    if save {
        info.set_mode(mode_option(mode));
        save_settings(kit_ref, info);
    }
}

extern "C" fn on_theme(kit: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    let theme = Theme::ALL[state.selected_index.min(Theme::ALL.len() - 1)];
    choose_theme(&kit, &mut info, theme);
    Update::RefreshDom
}

extern "C" fn on_mode(kit: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    let mode = ModePref::ALL[state.selected_index.min(ModePref::ALL.len() - 1)];
    choose_mode(&kit, &mut info, mode);
    Update::RefreshDom
}

extern "C" fn on_category(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut kit, index)) =
        data.downcast_ref::<CategoryRef>().map(|c| (c.kit.clone(), c.index))
    else {
        return Update::DoNothing;
    };
    let Some(mut k) = kit.downcast_mut::<Kit>() else {
        return Update::DoNothing;
    };
    if k.category == index {
        return Update::DoNothing;
    }
    k.category = index;
    Update::RefreshDom
}

/// OK: the changes stay (each was saved as it was made); the page closes.
extern "C" fn on_ok(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(kit_ref) = page_kit(&mut data) else {
        return Update::DoNothing;
    };
    {
        let mut kit = kit_ref.clone();
        if let Some(k) = kit.downcast_ref::<Kit>() {
            println!("{}_SETTINGS_CLOSED ok", k.spec.binary.to_uppercase());
        };
    }
    close_settings(&kit_ref);
    Update::RefreshDom
}

/// Cancel: the settings the page found come back; the page closes.
extern "C" fn on_cancel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(kit_ref) = page_kit(&mut data) else {
        return Update::DoNothing;
    };
    let _changed_back = cancel_settings(&kit_ref, &mut info);
    Update::RefreshDom
}

/// Escape on the page, for an app whose key handler does not call [`handle_key`] (that one
/// handles Escape for the others, before this runs: the app's handler sits on an ancestor).
extern "C" fn on_page_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(kit_ref) = page_kit(&mut data) else {
        return Update::DoNothing;
    };
    let routed = {
        let mut kit = kit_ref.clone();
        kit.downcast_ref::<Kit>().map_or(true, |k| k.keys_routed || !k.settings_open)
    };
    let escape_key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option()
        == Some(VirtualKeyCode::Escape);
    if routed || !escape_key {
        return Update::DoNothing;
    }
    escape(&kit_ref, &mut info);
    info.prevent_default();
    Update::RefreshDom
}
