//! AzInstallManager: the first-run and update manager of the Azlin apps.
//!
//! The first run (no app installed yet, or `--screen wizard`) is the install wizard
//! ([`wizard`]): it fetches each app the user picks from its update manifest, verifies the
//! signature chain and puts it under `~/.azlin/apps` with the platform's shortcuts. Later runs
//! show the installed apps. The work runs on a `Thread` ([`jobs`]); what it does on disk is the
//! [`model`]'s. The window is the utility shell with azul's `Titlebar` as its title row.
//!
//! A timer carries out the apps' "Update now" (an app writes its request, starts the manager
//! with `--apply <key>` and quits; the manager installs the staged version once the app's
//! process is gone and starts it again).
//!
//! On stdout, for scripts (`scripts/installmgr_e2e.py`): `AZINSTALLMGR_SCREEN <screen>`,
//! `AZINSTALLMGR_STEP <index> <step>`, `AZINSTALLMGR_TOTAL <bytes>`, `AZINSTALLMGR_PATH <path>`,
//! `AZINSTALLMGR_CHANNEL <channel> autoupdate=<bool>`, `AZINSTALLMGR_COPIED <command>`,
//! `AZINSTALLMGR_INSTALLED <key> <version>`, `AZINSTALLMGR_INSTALL_FAILED <key> <why>`,
//! `AZINSTALLMGR_PROGRESS <percent>`, `AZINSTALLMGR_DONE installed=<n> failed=<n>`,
//! `AZINSTALLMGR_FINISHED show_apps=<bool>`, `AZINSTALLMGR_ROW <key> <installed> <available>
//! <status>`, `AZINSTALLMGR_REQUESTED <key> <version or error>`.

mod apps;
mod jobs;
mod l10n;
#[cfg(test)]
mod l10n_tests;
pub mod model;
mod settings;
mod wizard;

use std::path::PathBuf;

use azul::{
    dom::VirtualKeyCode,
    file::FilePath,
    prelude::*,
    shells::{ShellThemeAccent, ShellThemeScope, UtilityShell},
    str::String as AzString,
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
    widgets::{
        AboutDialog, MessageBox, MessageBoxKind, Modal, ModalState, StandardDialogEvent,
        StandardDialogEventKind, WizardLayoutSize,
    },
};
use azul_appkit::{
    about::AboutInfo,
    args::{AppArgs, AppSpec, LanguagePref},
    azlin_config::{self, AzlinConfig, UpdatesSection},
    l10n::{t, t_args, Arg},
    shortcuts::Shortcut,
    ui as kit,
    update::{self, AzulEngine},
};

use crate::{
    jobs::{Job, JobKind, Progress},
    model::{Fetching, Installed, Os, Places},
};

/// The names `--screen` takes: `auto` (the wizard until an app is installed, then the apps),
/// the wizard, the installed apps, the update settings.
pub const SCREENS: [&str; 4] = ["auto", "wizard", "apps", "settings"];

/// What azul-appkit's switches know about AzInstallManager.
pub const SPEC: AppSpec = AppSpec {
    name: "AzInstallManager",
    binary: "AzInstallManager",
    summary: "installs the Azlin apps and keeps them up to date (--apply <app>: carry out an \
              app's Update now)",
    screens: &SCREENS,
    files_help: "",
};

/// The About facts.
pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzInstallManager",
    version: env!("CARGO_PKG_VERSION"),
    // The About box says it by `azinstallmgr-about-summary`.
    summary: "Installs the Azlin apps and keeps them up to date: stable or beta, staged \
              rollouts, one previous version kept for Revert.",
    license: "MIT",
    app_folder: "installmgr",
};

/// The keys the manager answers.
pub const SHORTCUTS: [Shortcut; 3] = [
    Shortcut::new("azinstallmgr-shortcuts", "F1", "azinstallmgr-shortcut-about"),
    Shortcut::new("azinstallmgr-shortcuts", "F5", "azinstallmgr-shortcut-check"),
    Shortcut::new("azinstallmgr-shortcuts", "Escape", "azinstallmgr-shortcut-escape"),
];

/// How often the apps' "Update now" requests are looked for.
const POLL_MS: u64 = 3_000;
/// How long the periodic check waits after the last one, with auto-update on.
const CHECK_EVERY_SECS: u64 = 6 * 3600;

/// What the window shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Screen {
    Wizard,
    Apps,
    Settings,
}

impl Screen {
    /// The name scripts know the screen by.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Screen::Wizard => "wizard",
            Screen::Apps => "apps",
            Screen::Settings => "settings",
        }
    }
}

/// Everything the window shows.
pub(crate) struct Manager {
    /// azul-appkit's kit: the switches, the data root, settings.json.
    pub(crate) kit: RefAny,
    /// Where the manager reads and writes.
    pub(crate) places: Places,
    /// The config's `updates` section as last read: the policy every job works with.
    pub(crate) updates: UpdatesSection,
    /// The machine's rollout cohort (kept in the config).
    pub(crate) cohort: u8,
    pub(crate) screen: Screen,
    pub(crate) wizard: wizard::Wizard,
    /// The installed apps: the table's rows.
    pub(crate) apps: Vec<Installed>,
    /// A job runs (not the request poll): the buttons wait for it.
    pub(crate) busy: bool,
    /// The poll of the apps' requests runs (it never holds the buttons).
    pub(crate) polling: bool,
    /// The last thing to tell the user, under the table (empty: nothing).
    pub(crate) notice: String,
    /// Why the update source last typed was not kept (not a web address), shown under its
    /// field on the settings screen and the wizard's Updates page; empty: nothing to say.
    pub(crate) source_problem: String,
    pub(crate) about_open: bool,
    /// The wizard's "Exit Setup?" question is open.
    pub(crate) confirm_cancel: bool,
    /// The app the "Uninstall ...?" question is about (open while `Some`).
    pub(crate) confirm_uninstall: Option<&'static model::AppEntry>,
    /// The running job checks: the rows say "Checking".
    pub(crate) checking: bool,
    /// When every app was last checked (the periodic check waits [`CHECK_EVERY_SECS`]).
    pub(crate) last_check_unix: u64,
}

impl Manager {
    /// What a job works with: the places, the policy as read now, the cohort.
    pub(crate) fn fetching(&self) -> Fetching {
        Fetching {
            places: self.places.clone(),
            updates: self.updates.clone(),
            cohort: self.cohort,
        }
    }

    /// Reads the installed apps again (their registrations, their owners) on the worker.
    pub(crate) fn load_apps(&mut self, info: &mut CallbackInfo, app: &RefAny) {
        jobs::spawn(info, app, Job::Load, self.fetching());
    }

    /// A first run on the config `config` with the policy `updates`, nothing installed: what
    /// the tests of the screens build them from.
    #[cfg(test)]
    pub(crate) fn for_test(config: PathBuf, updates: UpdatesSection) -> Manager {
        let places = places_for(config, &updates);
        Manager {
            kit: RefAny::new(0u8),
            wizard: wizard::Wizard::new(places.install_dir.to_string_lossy().into_owned(), None),
            places,
            updates,
            cohort: 1,
            screen: Screen::Wizard,
            apps: Vec::new(),
            busy: false,
            polling: false,
            notice: String::new(),
            source_problem: String::new(),
            about_open: false,
            confirm_cancel: false,
            confirm_uninstall: None,
            checking: false,
            last_check_unix: 0,
        }
    }
}

/// Whether `dom` or a box inside it has the id `id` (the tests of the screens).
#[cfg(test)]
pub(crate) fn has_id(dom: &Dom, id: &str) -> bool {
    dom.root.has_id(AzString::from(id)) || dom.children.as_slice().iter().any(|c| has_id(c, id))
}

/// The places of the config `config`: its folder is the Azlin folder; the shortcuts' folders
/// from the environment.
fn places_for(config: PathBuf, updates: &UpdatesSection) -> Places {
    let home = FilePath::get_home_dir()
        .into_option()
        .map_or_else(|| PathBuf::from("."), |p| PathBuf::from(p.inner.as_str()));
    let azlin_dir = config
        .parent()
        .map_or_else(|| home.join(azlin_config::CONFIG_DIR), PathBuf::from);
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map_or_else(|| home.join(".local").join("share"), PathBuf::from);
    let start_menu = std::env::var_os("APPDATA")
        .map_or_else(|| home.join("AppData").join("Roaming"), PathBuf::from)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs");
    Places {
        install_dir: update::install_dir(&azlin_dir, updates),
        azlin_dir,
        config,
        home,
        data_home,
        start_menu,
        os: Os::current(),
    }
}

// ==== The window ====

/// The About box and the wizard's exit question: modals the window always carries.
fn modals(m: &Manager, app: &RefAny) -> Vec<Dom> {
    let question = MessageBox::create(
        MessageBoxKind::Question,
        AzString::from(t("azinstallmgr-exit-title")),
        AzString::from(t("azinstallmgr-exit-text")),
    )
    .with_buttons(
        azul_appkit::l10n::labels(&["azinstallmgr-yes", "azinstallmgr-no"]),
        1,
    )
    .with_on_event(app.clone(), on_confirm)
    .dom();
    let uninstall = MessageBox::create(
        MessageBoxKind::Question,
        AzString::from(t_args(
            "azinstallmgr-uninstall-title",
            &[("app", Arg::from(m.confirm_uninstall.map_or("", |e| e.name)))],
        )),
        AzString::from(t("azinstallmgr-uninstall-text")),
    )
    .with_buttons(
        azul_appkit::l10n::labels(&["azinstallmgr-uninstall", "azinstallmgr-button-cancel"]),
        1,
    )
    .with_on_event(app.clone(), on_confirm_uninstall)
    .dom();
    let about = AboutDialog::create(
        AzString::from(ABOUT.name),
        AzString::from(format!("{} {}", t("azinstallmgr-version"), ABOUT.version)),
    )
    .with_icon(AzString::from("install_desktop"))
    .with_description(AzString::from(t("azinstallmgr-about-summary")))
    .with_copyright(AzString::from("Copyright 2026 the azul contributors"))
    .with_credit(AzString::from("azul"), AzString::from("MIT"))
    .with_on_event(app.clone(), on_about)
    .dom();
    vec![
        Modal::create(question)
            .with_title(AzString::from(t("azinstallmgr-wizard-title")))
            .with_open(m.confirm_cancel)
            .with_on_close(app.clone(), on_modal_close)
            .dom(),
        Modal::create(uninstall)
            .with_title(AzString::from(t("azinstallmgr-title")))
            .with_open(m.confirm_uninstall.is_some())
            .with_on_close(app.clone(), on_modal_close)
            .dom(),
        Modal::create(about)
            .with_title(AzString::from(t("azinstallmgr-about-title")))
            .with_open(m.about_open)
            .with_on_close(app.clone(), on_modal_close)
            .dom(),
    ]
}

extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    azul_appkit::l10n::begin_layout(&info);
    // Reading the mode makes a light / dark switch rebuild the window.
    let _mode = info.get_mode();
    let app = data.clone();
    let Some(guard) = data.downcast_ref::<Manager>() else {
        return Dom::create_body();
    };
    let m = &*guard;
    let (title, content) = match m.screen {
        Screen::Wizard => (t("azinstallmgr-wizard-title"), wizard::wizard(m, &app)),
        Screen::Apps => (t("azinstallmgr-title"), apps::screen(m, &app)),
        Screen::Settings => (t("azinstallmgr-settings"), settings::screen(m, &app)),
    };
    let shell = UtilityShell::create(content)
        .with_title_row(kit::title_row(&title))
        .with_label(AzString::from(title.as_str()))
        .dom();
    let mut column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(shell);
    for modal in modals(m, &app) {
        column.add_child(modal);
    }
    ShellThemeScope::create(column)
        .with_accent(ShellThemeAccent::Blue)
        .body()
        .with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            app,
            on_key,
        )
}

// ==== Callbacks ====

/// One step of a job, from the worker.
pub(crate) extern "C" fn on_progress(
    mut data: RefAny,
    mut msg: RefAny,
    _info: CallbackInfo,
) -> Update {
    let Some(progress) = msg.downcast_ref::<Progress>().map(|p| Progress::clone(&p)) else {
        return Update::DoNothing;
    };
    let Some(mut guard) = data.downcast_mut::<Manager>() else {
        return Update::DoNothing;
    };
    let m = &mut *guard;
    match progress {
        Progress::Started { key } => {
            if m.screen == Screen::Wizard {
                wizard::started(&mut m.wizard, &key);
            } else {
                apps::started(m, &key);
            }
        }
        Progress::Installed {
            key,
            result,
            problems,
        } => {
            let version = result.map(|r| r.version);
            wizard::installed(&mut m.wizard, &key, &version);
            m.wizard.progress.log.extend(problems);
        }
        Progress::Loaded { apps } => {
            for a in &apps {
                println!("{}", model::row_line(a));
            }
            m.apps = apps;
        }
        Progress::Requested { key, result } => {
            match &result {
                Ok(r) => println!("AZINSTALLMGR_REQUESTED {key} {}", r.version),
                Err(why) => println!("AZINSTALLMGR_REQUESTED {key} error: {why}"),
            }
            apps::updated(m, &key, result);
        }
        Progress::Checked { key, status } => apps::checked(m, &key, status),
        Progress::Configured { result } => settings::configured(m, result),
        Progress::Updated { key, result } => apps::updated(m, &key, result),
        Progress::Reverted { key, result } => apps::reverted(m, &key, result),
        Progress::Uninstalled { key, result } => apps::uninstalled(m, &key, result),
        Progress::RanManager { command, result } => wizard::ran_manager(&mut m.wizard, &command, &result),
        Progress::Finished { kind } => {
            match kind {
                JobKind::Requests => {
                    m.polling = false;
                    return Update::DoNothing;
                }
                JobKind::Background => return Update::DoNothing,
                JobKind::Work => {}
            }
            m.busy = false;
            if m.screen == Screen::Wizard && m.wizard.step == model::Step::Installing {
                wizard::finished(&mut m.wizard);
            }
            if m.checking {
                apps::check_finished(m);
            }
        }
    }
    Update::RefreshDom
}

/// Every few seconds: the apps' "Update now" requests; with auto-update on for any app, every
/// app is checked (and what is found staged) once the last check is old enough.
extern "C" fn on_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<Manager>() else {
        return TimerCallbackReturn::terminate_unchanged();
    };
    let m = &mut *guard;
    if !m.polling {
        m.polling = true;
        let fetching = m.fetching();
        jobs::spawn(&mut info.callback_info, &app, Job::Requests, fetching);
    }
    let due = update::unix_now().saturating_sub(m.last_check_unix) >= CHECK_EVERY_SECS;
    let wanted = m.apps.iter().any(|a| a.autoupdate && a.is_managed());
    if m.screen == Screen::Apps && due && wanted && !m.busy {
        apps::check_all(m, &mut info.callback_info, &app);
        return TimerCallbackReturn::continue_and_refresh_dom();
    }
    TimerCallbackReturn::continue_unchanged()
}

extern "C" fn on_confirm_uninstall(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: StandardDialogEvent,
) -> Update {
    let app = data.clone();
    let Some(mut m) = data.downcast_mut::<Manager>() else {
        return Update::DoNothing;
    };
    let yes = matches!((event.kind, event.index), (StandardDialogEventKind::Button, 0));
    apps::uninstall_answered(&mut *m, &mut info, &app, yes);
    Update::RefreshDom
}

extern "C" fn on_confirm(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: StandardDialogEvent,
) -> Update {
    let Some(mut m) = data.downcast_mut::<Manager>() else {
        return Update::DoNothing;
    };
    match (event.kind, event.index) {
        (StandardDialogEventKind::Button, 0) => {
            println!("AZINSTALLMGR_CANCELLED");
            info.close_window();
            Update::DoNothing
        }
        _ => {
            m.confirm_cancel = false;
            Update::RefreshDom
        }
    }
}

extern "C" fn on_about(
    mut data: RefAny,
    _info: CallbackInfo,
    _event: StandardDialogEvent,
) -> Update {
    let Some(mut m) = data.downcast_mut::<Manager>() else {
        return Update::DoNothing;
    };
    m.about_open = false;
    Update::RefreshDom
}

extern "C" fn on_modal_close(mut data: RefAny, _info: CallbackInfo, _state: ModalState) -> Update {
    let Some(mut m) = data.downcast_mut::<Manager>() else {
        return Update::DoNothing;
    };
    m.about_open = false;
    m.confirm_cancel = false;
    m.confirm_uninstall = None;
    Update::RefreshDom
}

/// F1 opens the About box; Escape closes it (or the exit question), else asks to exit the
/// wizard - an installer's Cancel.
extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option();
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<Manager>() else {
        return Update::DoNothing;
    };
    let m = &mut *guard;
    match key {
        Some(VirtualKeyCode::F1) if !m.about_open => m.about_open = true,
        Some(VirtualKeyCode::F5) if m.screen == Screen::Apps => apps::check_all(m, &mut info, &app),
        Some(VirtualKeyCode::Escape)
            if m.about_open || m.confirm_cancel || m.confirm_uninstall.is_some() =>
        {
            m.about_open = false;
            m.confirm_cancel = false;
            m.confirm_uninstall = None;
        }
        Some(VirtualKeyCode::Escape) if m.screen == Screen::Settings => {
            m.screen = Screen::Apps;
            println!("AZINSTALLMGR_SCREEN apps");
        }
        Some(VirtualKeyCode::Escape)
            if m.screen == Screen::Wizard && m.wizard.step != model::Step::Finish =>
        {
            m.confirm_cancel = true;
        }
        _ => return Update::DoNothing,
    }
    info.prevent_default();
    Update::RefreshDom
}

/// The window exists: the `--shot` timer, the request poll.
extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(m) = data.downcast_ref::<Manager>() else {
        return Update::DoNothing;
    };
    let kit_ref = m.kit.clone();
    let screen = m.screen;
    drop(m);
    kit::on_window_created(&kit_ref, &mut info);
    let timer = Timer::create(app, on_tick, info.get_system_time_fn())
        .with_interval(Duration::System(SystemTimeDiff::from_millis(POLL_MS)));
    info.add_timer(TimerId::unique(), timer);
    println!("AZINSTALLMGR_SCREEN {}", screen.name());
    Update::DoNothing
}

// ==== Entry ====

pub fn start() {
    let (apply, rest) = match model::split_switches(std::env::args().skip(1)) {
        Ok(split) => split,
        Err(why) => {
            eprintln!("{why}");
            std::process::exit(2);
        }
    };
    let args = match AppArgs::parse(&SPEC, rest) {
        Ok(a) => a,
        Err(why) => {
            println!("{why}");
            std::process::exit(if why.contains("USAGE") { 0 } else { 2 });
        }
    };
    let kit_ref = kit::create_kit(SPEC, ABOUT, &SHORTCUTS, &[], args.clone());
    let mut config = kit::app_config(&kit_ref);
    l10n::register(&mut config);
    let mut kit_now = kit_ref.clone();
    let language = kit_now.downcast_ref::<kit::Kit>().map(|k| k.language());
    if let Some(language) = language.filter(|l| *l != LanguagePref::System) {
        azul_appkit::l10n::set_locale(language.tag());
    }

    // The manager works on the shared config even where the kit reads none (a --shot run):
    // it is where the apps and their policy are.
    let home = FilePath::get_home_dir()
        .into_option()
        .map(|p| PathBuf::from(p.inner.as_str()));
    let config_path = azlin_config::config_path(
        std::env::var(azlin_config::CONFIG_VAR).ok().as_deref(),
        home.as_deref(),
    )
    .unwrap_or_else(|| PathBuf::from(azlin_config::CONFIG_DIR).join(azlin_config::CONFIG_FILE));
    let (shared, problem) = AzlinConfig::load(&config_path);
    if let Some(problem) = problem {
        eprintln!("[installmgr] {}: {problem}", config_path.display());
    }
    let mut updates = shared.updates;
    let places = places_for(config_path, &updates);
    let (cohort, problem) = update::machine_cohort(&AzulEngine, &places.config, &updates);
    if let Some(problem) = problem {
        eprintln!("[installmgr] {problem}");
    }
    updates.cohort = Some(cohort);
    // The apps find the manager by its registration (their "Update now" starts it).
    if let Ok(binary) = std::env::current_exe() {
        let registered = update::register(
            &places.app(update::MANAGER_KEY),
            &update::Starting {
                name: SPEC.name,
                key: update::MANAGER_KEY,
                version: ABOUT.version,
                binary: &binary,
                now_unix: update::unix_now(),
            },
        );
        if let Err(e) = registered {
            eprintln!("[installmgr] {e}");
        }
    }
    let apps = model::installed(&AzulEngine, &places, &updates);
    let any_app = apps.iter().any(|a| !a.entry.is_runtime());
    let screen = match args.screen.as_deref() {
        Some("wizard") => Screen::Wizard,
        Some("apps") => Screen::Apps,
        Some("settings") => Screen::Settings,
        _ if any_app || apply.is_some() => Screen::Apps,
        _ => Screen::Wizard,
    };
    let package_manager = model::detect_package_manager(
        &std::env::var("PATH").unwrap_or_default(),
        places.os,
    );
    let state = Manager {
        kit: kit_ref.clone(),
        wizard: wizard::Wizard::new(places.install_dir.to_string_lossy().into_owned(), package_manager),
        places,
        updates,
        cohort,
        screen,
        apps,
        busy: false,
        polling: false,
        notice: String::new(),
        source_problem: String::new(),
        about_open: false,
        confirm_cancel: false,
        confirm_uninstall: None,
        checking: false,
        // An app's "Update now" first: the periodic check waits its turn.
        last_check_unix: if apply.is_some() { update::unix_now() } else { 0 },
    };
    if screen == Screen::Wizard {
        println!("{}", model::step_line(&state.wizard.steps, state.wizard.step));
    }
    let (size, min) = if screen == Screen::Wizard {
        (
            (WizardLayoutSize::Classic.width(), WizardLayoutSize::Classic.height() + 34.0),
            (WizardLayoutSize::Compact.width(), WizardLayoutSize::Compact.height() + 34.0),
        )
    } else {
        ((960.0, 620.0), (720.0, 420.0))
    };
    // The install wizard's pages are laid out for the classic size: it is never maximized.
    if screen == Screen::Wizard {
        kit::keep_default_size(&kit_ref);
    }
    let mut window = kit::window_options(&kit_ref, layout, size, min, on_window_created);
    window.window_state.title = AzString::from(t("azinstallmgr-title"));
    App::create(RefAny::new(state), config).run(window);
}
