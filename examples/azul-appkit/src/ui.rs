//! The UI the Azlin apps share (feature `azul`): one [`Kit`] per app.
//!
//! The app keeps the kit's `RefAny` in its own state and asks it for:
//! - the window ([`window_options`]: `NoTitle`, `--size`, a compact minimum
//!   size) and the app config ([`app_config`]: the app theme and mode from
//!   `settings.json`, a `--theme` / `--mode` switch winning for one run);
//! - the title row ([`title_row`]: azul's `Titlebar`, drawn by the app as
//!   every azul app does);
//! - the settings page ([`settings_page`], on azul's `ShellSettingsLayout`):
//!   the app's own categories first, then Appearance (theme and mode, saved
//!   to `settings.json` and applied at once), Data (the data folder),
//!   Shortcuts (the app's table) and About. Mod+, opens it, F1 opens it at
//!   the shortcuts, Escape or Back closes it ([`handle_key`]);
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
        ShellSettingsLayoutOnCategoryCallbackType, ShellSettingsLayoutOnSearchCallbackType,
        TimerCallbackInfo, TimerCallbackReturn, WriteBackCallbackType,
    },
    css::DarkLightMode,
    dom::{DomId, VirtualKeyCode},
    file::FilePath,
    option::{OptionDarkLightMode, OptionLogicalSize},
    prelude::*,
    shells::{ShellSettingsLayout, ShellSettingsSection},
    str::String as AzString,
    task::{
        Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg,
        Timer, TimerId,
    },
    time::{Duration, SystemTimeDiff},
    vec::StringVec,
    widgets::{
        AboutDialog, Button, Modal, ModalState, Segmented, SegmentedState, StandardDialogEvent,
        Titlebar,
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
    settings::{AppSettings, SETTINGS_FILE},
    shortcuts::{display_keys, groups, Shortcut, KIT_SHORTCUTS},
};

/// The kit's categories, after the app's own.
pub const KIT_CATEGORIES: [&str; 4] = ["Appearance", "Data", "Shortcuts", "About"];

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
    /// The settings page's search text.
    pub search: String,
    /// The app's own settings categories (first on the page).
    pub app_categories: Vec<String>,
    /// The last problem saving or reading the settings ("" = none).
    pub notice: String,
    /// Cmd (macOS) or Ctrl.
    pub mac: bool,
    /// The About box (azul's standard `AboutDialog`) is open.
    pub about_open: bool,
}

impl Kit {
    /// Every category of the settings page: the app's, then the kit's.
    #[must_use]
    pub fn categories(&self) -> Vec<String> {
        self.app_categories
            .iter()
            .cloned()
            .chain(KIT_CATEGORIES.iter().map(|c| (*c).to_string()))
            .collect()
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
        search: String::new(),
        app_categories: app_categories.iter().map(|c| (*c).to_string()).collect(),
        notice,
        mac: cfg!(target_os = "macos"),
        about_open: false,
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
    jobs: Option<Vec<FileJob>>,
    tag: u64,
    on_done: WriteBackCallbackType,
}

/// Runs on the worker thread: the jobs on the data root's drive, then the
/// outcomes to the UI thread.
extern "C" fn file_thread(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((root, jobs, tag, on_done)) = init.downcast_mut::<FileThreadInit>().and_then(|mut i| {
        let jobs = i.jobs.take()?;
        Some((i.root.clone(), jobs, i.tag, i.on_done))
    }) else {
        return;
    };
    let drive = LocalDrive::new(root);
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
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(FileThreadInit {
                root: root.to_path_buf(),
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

/// Shows the settings page, at the category named `category` if given.
pub fn open_settings(kit_ref: &RefAny, category: Option<&str>) {
    let mut kit = kit_ref.clone();
    if let Some(mut k) = kit.downcast_mut::<Kit>() {
        k.settings_open = true;
        k.search.clear();
        if let Some(name) = category {
            if let Some(i) = k.categories().iter().position(|c| c == name) {
                k.category = i;
            }
        }
    };
}

/// Hides the settings page.
pub fn close_settings(kit_ref: &RefAny) {
    let mut kit = kit_ref.clone();
    if let Some(mut k) = kit.downcast_mut::<Kit>() {
        k.settings_open = false;
    };
}

/// Whether the settings page is showing.
#[must_use]
pub fn settings_open(kit_ref: &RefAny) -> bool {
    let mut kit = kit_ref.clone();
    kit.downcast_ref::<Kit>().is_some_and(|k| k.settings_open)
}

/// The kit's keys, for the app's window key handler to call first:
/// Mod+, opens the settings, F1 opens them at the shortcuts, Escape closes
/// them. `Some` = handled (the app returns it), `None` = the app's key.
pub fn handle_key(kit_ref: &RefAny, info: &mut CallbackInfo) -> Option<Update> {
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
        VirtualKeyCode::Escape if about_open(kit_ref) => {
            set_about_open(kit_ref, false);
            info.prevent_default();
            Some(Update::RefreshDom)
        }
        VirtualKeyCode::Escape if settings_open(kit_ref) => {
            close_settings(kit_ref);
            info.prevent_default();
            Some(Update::RefreshDom)
        }
        _ => None,
    }
}

// ==== The settings page ====

/// One of the app's own sections: its category (an index into the app's
/// categories), its title and its content.
pub struct AppSection {
    pub category: usize,
    pub title: String,
    pub content: Dom,
}

/// A run of text (an inline span).
#[must_use]
pub fn text<S: Into<AzString>>(content: S) -> Dom {
    Dom::create_span_with_text(content)
}

fn strs(items: &[&str]) -> StringVec {
    StringVec::from_vec(items.iter().map(|s| AzString::from(*s)).collect())
}

/// A settings row: a label column and the control.
#[must_use]
pub fn row(label: &str, control: Dom) -> Dom {
    Dom::create_div()
        .with_class("appkit-row")
        .with_css("display: flex; flex-direction: row; align-items: center; padding: 6px 0px;")
        .with_child(
            Dom::create_div()
                .with_css("width: 160px; flex-shrink: 0; font-size: 13px;")
                .with_child(text(label)),
        )
        .with_child(control)
}

/// A line of secondary text under a section's rows.
#[must_use]
pub fn note(content: &str) -> Dom {
    Dom::create_div()
        .with_css("padding: 4px 0px; font-size: 12px; opacity: 0.75;")
        .with_child(text(content))
}

fn column(children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_children(DomVec::from_vec(children))
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
    column(rows)
}

fn data_section(k: &Kit) -> Dom {
    let folder = data::local_path(&k.data_root, k.about.app_folder);
    column(vec![
        row(
            "Data folder",
            Dom::create_div()
                .with_id("appkit-data-folder")
                .with_child(text(folder.display().to_string())),
        ),
        note(
            "Your data are plain files in this folder, one folder per app. An S3 drive can \
             take the place of the folder later without changing them.",
        ),
    ])
}

fn shortcuts_section(k: &Kit) -> Dom {
    let mut children = Vec::new();
    for (group, items) in groups(&k.shortcuts) {
        children.push(
            Dom::create_div()
                .with_css("padding: 8px 0px 2px 0px; font-size: 12px; font-weight: 600;")
                .with_child(text(group)),
        );
        for s in items {
            children.push(row(
                &display_keys(s.keys, k.mac),
                Dom::create_div().with_child(text(s.action)),
            ));
        }
    }
    column(children).with_id("appkit-shortcuts")
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
        children.push(row(&label, Dom::create_div().with_child(text(value))));
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
    column(children)
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

/// The settings page: a header (Back, "Settings") over the
/// `ShellSettingsLayout` with the app's sections and the kit's. With a search
/// every section is handed in and the layout hides those whose title does
/// not match; otherwise the sections of the chosen category.
#[must_use]
pub fn settings_page(kit_ref: &RefAny, app_sections: Vec<AppSection>) -> Dom {
    let mut kit = kit_ref.clone();
    let Some(k) = kit.downcast_ref::<Kit>() else {
        return Dom::create_div();
    };
    let app_count = k.app_categories.len();
    let categories = k.categories();
    let category = k.category.min(categories.len().saturating_sub(1));
    let searching = !k.search.trim().is_empty();

    let mut sections: Vec<(usize, ShellSettingsSection)> = app_sections
        .into_iter()
        .map(|s| (s.category, ShellSettingsSection::create(s.title, s.content)))
        .collect();
    sections.push((app_count, ShellSettingsSection::create("Appearance", appearance_section(&k, kit_ref))));
    sections.push((app_count + 1, ShellSettingsSection::create("Data", data_section(&k))));
    sections.push((
        app_count + 2,
        ShellSettingsSection::create("Keyboard shortcuts", shortcuts_section(&k)),
    ));
    sections.push((
        app_count + 3,
        ShellSettingsSection::create(format!("About {}", k.about.name), about_section(&k, kit_ref)),
    ));

    let mut layout = ShellSettingsLayout::create(StringVec::from_vec(
        categories.iter().map(|c| AzString::from(c.as_str())).collect(),
    ))
    .with_active_category(category)
    .with_search(k.search.as_str())
    .with_on_category(kit_ref.clone(), on_category as ShellSettingsLayoutOnCategoryCallbackType)
    .with_on_search(kit_ref.clone(), on_search as ShellSettingsLayoutOnSearchCallbackType);
    for (c, section) in sections {
        if searching || c == category {
            layout.add_section(section);
        }
    }

    let mut header = Dom::create_div()
        .with_id("appkit-settings-header")
        .with_css("display: flex; flex-direction: row; align-items: center; padding: 6px 12px;")
        .with_child(
            Button::create("Back")
                .with_icon("arrow_back")
                .with_on_click(kit_ref.clone(), on_back as ButtonOnClickCallbackType)
                .dom()
                .with_id("appkit-settings-back"),
        )
        .with_child(
            Dom::create_div()
                .with_css("padding-left: 12px; font-size: 15px; font-weight: 600;")
                .with_child(text("Settings")),
        );
    if !k.notice.is_empty() {
        header.add_child(
            Dom::create_div()
                .with_id("appkit-settings-notice")
                .with_css("padding-left: 16px; font-size: 12px;")
                .with_child(text(k.notice.as_str())),
        );
    }
    Dom::create_div()
        .with_id("appkit-settings")
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(header)
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
                .with_child(layout.dom()),
        )
        .with_child(about_modal(&k, kit_ref))
}

// ==== The settings page's callbacks (data: the kit) ====

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

extern "C" fn on_category(mut kit: RefAny, _info: CallbackInfo, index: usize) -> Update {
    if let Some(mut k) = kit.downcast_mut::<Kit>() {
        k.category = index;
        k.search.clear();
    }
    Update::RefreshDom
}

extern "C" fn on_search(mut kit: RefAny, _info: CallbackInfo, text: AzString) -> Update {
    if let Some(mut k) = kit.downcast_mut::<Kit>() {
        k.search = text.as_str().to_string();
    }
    Update::RefreshDom
}

extern "C" fn on_back(kit: RefAny, _info: CallbackInfo) -> Update {
    close_settings(&kit);
    Update::RefreshDom
}
