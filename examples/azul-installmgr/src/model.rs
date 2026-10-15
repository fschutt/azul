//! AzInstallManager's model without azul types: the apps it offers, the steps of the first-run
//! wizard, what installing, updating, reverting and uninstalling do on disk, the shortcuts each
//! platform gets and the package managers it points to. The update engine's calls go through
//! azul-appkit's `UpdateEngine` (a fake in the tests). Tested without a window.
//!
//! On disk an app is `<install_dir>/<key>/current/<program>` with its `release.json` beside it,
//! the version before in `prev/` (one is kept), and its registry folder
//! `<azlin>/apps/<key>/` (azul-appkit's `update` module). Nothing here asks for administrator
//! rights: a folder the user cannot write to is an error that names the package manager instead.

use std::path::{Path, PathBuf};

use azul_appkit::{
    azlin_config::{AzlinConfig, Channel, UpdatesSection},
    update::{
        audience_for, background_round, manifest_url_for, read_json, write_json, AppPaths,
        ApplyRequest, Audience, Check, CheckRequest, InstallKind, Registration, Release, Round,
        RoundOutcome, Staged, UpdateEngine, APPS_DIR, RELEASE_FILE, REQUEST_FILE,
    },
};

/// One megabyte.
pub const MB: u64 = 1024 * 1024;

// ---------------------------------------------------------------------------
// The apps
// ---------------------------------------------------------------------------

/// One thing the manager installs: an app, or the runtime the apps share.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppEntry {
    /// The key: the registry folder, the manifest's `{app}`, the config's override (`azwriter`).
    pub key: &'static str,
    /// The name, and the program's file name without `.exe` (`AzWriter`).
    pub name: &'static str,
    /// The icon the lists show (azul's icon names).
    pub icon: &'static str,
    /// What it is: a message of the manager's resources.
    pub summary_key: &'static str,
    /// About how many MB it takes installed: the wizard's estimate before anything is fetched.
    pub size_mb: u64,
    /// The XDG menu category of its `.desktop` file.
    pub category: &'static str,
    /// The wizard's group (an index into [`GROUPS`]).
    pub group: usize,
}

/// The key of the runtime the apps share (libazul): installed with every app, never alone.
pub const RUNTIME_KEY: &str = "runtime";

/// The runtime: the shared library every app links against. Required, no shortcut, no banner.
pub static RUNTIME: AppEntry = AppEntry {
    key: RUNTIME_KEY,
    name: "Azlin runtime",
    icon: "memory",
    summary_key: "azinstallmgr-app-runtime",
    size_mb: 70,
    category: "",
    group: 0,
};

/// The wizard's groups of apps, as message keys, in order.
pub const GROUPS: [&str; 4] = [
    "azinstallmgr-group-office",
    "azinstallmgr-group-communication",
    "azinstallmgr-group-media",
    "azinstallmgr-group-tools",
];

const fn app(
    key: &'static str,
    name: &'static str,
    icon: &'static str,
    summary_key: &'static str,
    size_mb: u64,
    category: &'static str,
    group: usize,
) -> AppEntry {
    AppEntry {
        key,
        name,
        icon,
        summary_key,
        size_mb,
        category,
        group,
    }
}

/// Every app the manager offers (the Azlin apps in production), in the wizard's order.
pub static APPS: [AppEntry; 28] = [
    app("azwriter", "AzWriter", "description", "azinstallmgr-app-azwriter", 15, "Office;WordProcessor;", 0),
    app("azsheets", "AzSheets", "table_chart", "azinstallmgr-app-azsheets", 22, "Office;Spreadsheet;", 0),
    app("azshow", "AzShow", "slideshow", "azinstallmgr-app-azshow", 14, "Office;Presentation;", 0),
    app("aznotes", "AzNotes", "sticky_note_2", "azinstallmgr-app-aznotes", 10, "Office;", 0),
    app("azpdf", "AzPdf", "picture_as_pdf", "azinstallmgr-app-azpdf", 14, "Office;Viewer;", 0),
    app("azreader", "AzReader", "menu_book", "azinstallmgr-app-azreader", 12, "Office;Viewer;", 0),
    app("azreview", "AzReview", "rate_review", "azinstallmgr-app-azreview", 12, "Development;", 0),
    app("azmail", "AzMail", "mail", "azinstallmgr-app-azmail", 16, "Network;Email;", 1),
    app("azcalendar", "AzCalendar", "calendar_month", "azinstallmgr-app-azcalendar", 9, "Office;Calendar;", 1),
    app("azcontacts", "AzContacts", "contacts", "azinstallmgr-app-azcontacts", 8, "Office;ContactManagement;", 1),
    app("aztasks", "AzTasks", "task_alt", "azinstallmgr-app-aztasks", 8, "Office;ProjectManagement;", 1),
    app("azmeet", "AzMeet", "video_call", "azinstallmgr-app-azmeet", 20, "Network;VideoConference;", 1),
    app("aznews", "AzNews", "rss_feed", "azinstallmgr-app-aznews", 10, "Network;Feed;", 1),
    app("azphoto", "AzPhoto", "photo", "azinstallmgr-app-azphoto", 15, "Graphics;Photography;", 2),
    app("azpaint", "AzPaint", "brush", "azinstallmgr-app-azpaint", 12, "Graphics;RasterGraphics;", 2),
    app("azmusic", "AzMusic", "library_music", "azinstallmgr-app-azmusic", 11, "AudioVideo;Audio;Player;", 2),
    app("azplayer", "AzPlayer", "smart_display", "azinstallmgr-app-azplayer", 12, "AudioVideo;Video;Player;", 2),
    app("azvideocut", "AzVideoCut", "movie_edit", "azinstallmgr-app-azvideocut", 24, "AudioVideo;Video;AudioVideoEditing;", 2),
    app("azdrive", "AzDrive", "folder", "azinstallmgr-app-azdrive", 14, "System;FileManager;", 3),
    app("azkeys", "AzKeys", "key", "azinstallmgr-app-azkeys", 11, "Utility;Security;", 3),
    app("azmaps", "AzMaps", "map", "azinstallmgr-app-azmaps", 16, "Utility;Maps;", 3),
    app("azclock", "AzClock", "schedule", "azinstallmgr-app-azclock", 7, "Utility;Clock;", 3),
    app("azcalculator", "AzCalculator", "calculate", "azinstallmgr-app-azcalculator", 7, "Utility;Calculator;", 3),
    app("azcode", "AzCode", "code", "azinstallmgr-app-azcode", 18, "Development;TextEditor;", 3),
    app("azterm", "AzTerm", "terminal", "azinstallmgr-app-azterm", 12, "System;TerminalEmulator;", 3),
    app("azmonitor", "AzMonitor", "monitor_heart", "azinstallmgr-app-azmonitor", 10, "System;Monitor;", 3),
    app("azdashboard", "AzDashboard", "dashboard", "azinstallmgr-app-azdashboard", 11, "Office;Finance;", 3),
    app("azerp", "AzERP", "inventory_2", "azinstallmgr-app-azerp", 13, "Office;Finance;", 3),
];

/// The entry of `key` (the runtime too).
#[must_use]
pub fn entry(key: &str) -> Option<&'static AppEntry> {
    if key == RUNTIME_KEY {
        return Some(&RUNTIME);
    }
    APPS.iter().find(|a| a.key == key)
}

impl AppEntry {
    /// Whether this is the runtime rather than an app.
    #[must_use]
    pub fn is_runtime(&self) -> bool {
        self.key == RUNTIME_KEY
    }

    /// The installed file's name on `os`: the program (`AzWriter`, `AzWriter.exe`), or for the
    /// runtime the shared library the apps load.
    #[must_use]
    pub fn file_name(&self, os: Os) -> String {
        match (self.is_runtime(), os) {
            (true, Os::MacOs) => "libazul.dylib".to_string(),
            (true, Os::Windows) => "azul.dll".to_string(),
            (true, Os::Linux) => "libazul.so".to_string(),
            (false, Os::Windows) => format!("{}.exe", self.name),
            (false, _) => self.name.to_string(),
        }
    }

    /// The name without "Az" (`Writer`): the package names are made of it.
    #[must_use]
    pub fn short_name(&self) -> &'static str {
        self.name.strip_prefix("Az").unwrap_or(self.name)
    }

    /// The package a Homebrew cask or an apt package is called (`azlin-writer`).
    #[must_use]
    pub fn package(&self) -> String {
        format!("azlin-{}", self.short_name().to_ascii_lowercase())
    }

    /// The Flatpak application id (`app.azlin.Writer`).
    #[must_use]
    pub fn flatpak_id(&self) -> String {
        format!("app.azlin.{}", self.short_name())
    }

    /// The winget package id (`Azlin.AzWriter`).
    #[must_use]
    pub fn winget_id(&self) -> String {
        format!("Azlin.{}", self.name)
    }
}

/// The operating system, for the file names and the shortcuts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Linux,
    MacOs,
    Windows,
}

impl Os {
    /// The system this build runs on.
    #[must_use]
    pub fn current() -> Os {
        if cfg!(target_os = "macos") {
            Os::MacOs
        } else if cfg!(target_os = "windows") {
            Os::Windows
        } else {
            Os::Linux
        }
    }
}

/// The bytes the `selected` apps take, with the runtime they need.
#[must_use]
pub fn needed_bytes(selected: &[&AppEntry]) -> u64 {
    let apps: u64 = selected
        .iter()
        .filter(|a| !a.is_runtime())
        .map(|a| a.size_mb * MB)
        .sum();
    if apps == 0 {
        0
    } else {
        apps + RUNTIME.size_mb * MB
    }
}

/// One row of the wizard's component list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComponentRow {
    /// The label: an app's name, or a message key (the runtime, a group).
    pub label: &'static str,
    /// The message key of the row's description (empty: none).
    pub description_key: &'static str,
    /// The estimate the total adds up (0 for a group).
    pub bytes: u64,
    /// 0 for the runtime and the groups, 1 for an app under its group.
    pub depth: u32,
    /// The tick cannot be cleared (the runtime).
    pub required: bool,
    /// The app the row installs; `None` for a group.
    pub entry: Option<&'static AppEntry>,
}

/// The wizard's component list: the runtime (required), then each group with its apps under it.
#[must_use]
pub fn component_rows() -> Vec<ComponentRow> {
    let mut rows = vec![ComponentRow {
        label: "azinstallmgr-component-runtime",
        description_key: RUNTIME.summary_key,
        bytes: RUNTIME.size_mb * MB,
        depth: 0,
        required: true,
        entry: Some(&RUNTIME),
    }];
    for (group, label) in GROUPS.into_iter().enumerate() {
        rows.push(ComponentRow {
            label,
            description_key: "",
            bytes: 0,
            depth: 0,
            required: false,
            entry: None,
        });
        for a in APPS.iter().filter(|a| a.group == group) {
            rows.push(ComponentRow {
                label: a.name,
                description_key: a.summary_key,
                bytes: a.size_mb * MB,
                depth: 1,
                required: false,
                entry: Some(a),
            });
        }
    }
    rows
}

// ---------------------------------------------------------------------------
// The first-run wizard
// ---------------------------------------------------------------------------

/// The wizard's steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    Welcome,
    /// "Install through your package manager": only when one is found.
    PackageManager,
    Destination,
    Components,
    Channel,
    Ready,
    Installing,
    Finish,
}

impl Step {
    /// The step's label on the side panel (a message key).
    #[must_use]
    pub const fn label_key(self) -> &'static str {
        match self {
            Step::Welcome => "azinstallmgr-step-welcome",
            Step::PackageManager => "azinstallmgr-step-package-manager",
            Step::Destination => "azinstallmgr-step-destination",
            Step::Components => "azinstallmgr-step-components",
            Step::Channel => "azinstallmgr-step-channel",
            Step::Ready => "azinstallmgr-step-ready",
            Step::Installing => "azinstallmgr-step-installing",
            Step::Finish => "azinstallmgr-step-finish",
        }
    }

    /// The banner's subtitle (a message key; empty: none).
    #[must_use]
    pub const fn subtitle_key(self) -> &'static str {
        match self {
            Step::Welcome | Step::Finish => "",
            Step::PackageManager => "azinstallmgr-subtitle-package-manager",
            Step::Destination => "azinstallmgr-subtitle-destination",
            Step::Components => "azinstallmgr-subtitle-components",
            Step::Channel => "azinstallmgr-subtitle-channel",
            Step::Ready => "azinstallmgr-subtitle-ready",
            Step::Installing => "azinstallmgr-subtitle-installing",
        }
    }

    /// The name scripts and `--screen` know the step by.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Step::Welcome => "welcome",
            Step::PackageManager => "package-manager",
            Step::Destination => "destination",
            Step::Components => "components",
            Step::Channel => "channel",
            Step::Ready => "ready",
            Step::Installing => "installing",
            Step::Finish => "finish",
        }
    }

    /// Whether the side panel frames the step (welcome, finish), Wizard97's way.
    #[must_use]
    pub const fn is_outer(self) -> bool {
        matches!(self, Step::Welcome | Step::Finish)
    }
}

/// The steps of a first run: the package-manager page only when one is found; `through_manager`
/// says the apps are installed through it.
#[must_use]
pub fn steps(package_manager: Option<PackageManager>, _through_manager: bool) -> Vec<Step> {
    let mut steps = vec![Step::Welcome];
    if package_manager.is_some() {
        steps.push(Step::PackageManager);
    }
    steps.extend([
        Step::Destination,
        Step::Components,
        Step::Channel,
        Step::Ready,
        Step::Installing,
        Step::Finish,
    ]);
    steps
}

/// The step after `step` in `steps` (the last stays).
#[must_use]
pub fn next_step(steps: &[Step], step: Step) -> Step {
    let at = steps.iter().position(|s| *s == step).unwrap_or(0);
    steps[(at + 1).min(steps.len() - 1)]
}

/// The step before `step` (the first stays; nothing goes back from the installing or the end).
#[must_use]
pub fn previous_step(steps: &[Step], step: Step) -> Step {
    if matches!(step, Step::Installing | Step::Finish) {
        return step;
    }
    let at = steps.iter().position(|s| *s == step).unwrap_or(0);
    steps[at.saturating_sub(1)]
}

// ---------------------------------------------------------------------------
// Package managers
// ---------------------------------------------------------------------------

/// A package manager the Azlin apps are offered through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackageManager {
    Homebrew,
    Apt,
    Dnf,
    Pacman,
    Flatpak,
    Winget,
}

impl PackageManager {
    /// Its name, as people call it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            PackageManager::Homebrew => "Homebrew",
            PackageManager::Apt => "apt",
            PackageManager::Dnf => "dnf",
            PackageManager::Pacman => "pacman",
            PackageManager::Flatpak => "Flatpak",
            PackageManager::Winget => "winget",
        }
    }

    /// The program that shows it is installed.
    #[must_use]
    pub fn program(self) -> &'static str {
        match self {
            PackageManager::Homebrew => "brew",
            PackageManager::Apt => "apt",
            PackageManager::Dnf => "dnf",
            PackageManager::Pacman => "pacman",
            PackageManager::Flatpak => "flatpak",
            PackageManager::Winget => "winget",
        }
    }

    /// The commands (program and arguments) that install `apps` through it, run one after the
    /// other.
    #[must_use]
    pub fn install_runs(self, _apps: &[&AppEntry]) -> Vec<Vec<String>> {
        Vec::new()
    }

    /// The command that installs `apps` through it. The manager shows it with a Copy button and
    /// never runs it: a package manager asks for its own rights.
    #[must_use]
    pub fn install_command(self, apps: &[&AppEntry]) -> String {
        let apps: Vec<&&AppEntry> = apps.iter().filter(|a| !a.is_runtime()).collect();
        match self {
            PackageManager::Homebrew => {
                let names: Vec<String> = apps.iter().map(|a| a.package()).collect();
                format!("brew install --cask {}", names.join(" "))
            }
            PackageManager::Apt => {
                let names: Vec<String> = apps.iter().map(|a| a.package()).collect();
                format!("sudo apt update && sudo apt install {}", names.join(" "))
            }
            PackageManager::Flatpak => {
                let ids: Vec<String> = apps.iter().map(|a| a.flatpak_id()).collect();
                format!("flatpak install --user azlin {}", ids.join(" "))
            }
            _ => String::new(),
        }
    }
}

/// The first package manager the search path `path_var` (`PATH`) has, in the order the
/// platform's users expect: Homebrew on macOS; apt, then Flatpak elsewhere.
#[must_use]
pub fn detect_package_manager(path_var: &str, os: Os) -> Option<PackageManager> {
    let order: &[PackageManager] = match os {
        Os::MacOs => &[PackageManager::Homebrew],
        Os::Linux => &[PackageManager::Apt, PackageManager::Flatpak],
        Os::Windows => &[],
    };
    let separator = if os == Os::Windows { ';' } else { ':' };
    order.iter().copied().find(|pm| {
        path_var
            .split(separator)
            .filter(|dir| !dir.is_empty())
            .any(|dir| Path::new(dir).join(pm.program()).is_file())
    })
}

/// Who owns an installed copy of an app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Owner {
    /// The manager installed it: Update, Revert and Uninstall are its.
    Manager,
    /// Homebrew (`/opt/homebrew`, a `Cellar` or a `Caskroom`).
    Homebrew,
    /// The system's package manager (dpkg / rpm, `/usr`, `/opt`).
    System,
    Flatpak,
    Snap,
    WindowsStore,
    /// A copy the user can write to that the manager did not install (a developer's build).
    Elsewhere,
    /// A copy nobody here can write to.
    ReadOnly,
}

impl Owner {
    /// The owner of the program at `program` that `kind` describes; `managed`: the manager's
    /// registration says it installed it.
    #[must_use]
    pub fn of(kind: InstallKind, program: &Path, managed: bool) -> Owner {
        let text = program.to_string_lossy();
        match kind {
            InstallKind::SystemPackageManager
                if text.contains("/homebrew/")
                    || text.contains("/Cellar/")
                    || text.contains("/Caskroom/") =>
            {
                Owner::Homebrew
            }
            InstallKind::SystemPackageManager => Owner::System,
            InstallKind::Snap => Owner::Snap,
            InstallKind::Flatpak => Owner::Flatpak,
            InstallKind::WindowsStore => Owner::WindowsStore,
            InstallKind::UserWritable if managed => Owner::Manager,
            InstallKind::UserWritable => Owner::Elsewhere,
            InstallKind::ReadOnly => Owner::ReadOnly,
        }
    }

    /// The command that updates `app` through its owner (empty: none to show).
    #[must_use]
    pub fn update_command(&self, app: &AppEntry) -> String {
        match self {
            Owner::Homebrew => format!("brew upgrade --cask {}", app.package()),
            Owner::System => format!(
                "sudo apt update && sudo apt install --only-upgrade {}",
                app.package()
            ),
            Owner::Flatpak => format!("flatpak update {}", app.flatpak_id()),
            Owner::Snap => format!("sudo snap refresh {}", app.package()),
            Owner::Manager | Owner::WindowsStore | Owner::Elsewhere | Owner::ReadOnly => {
                String::new()
            }
        }
    }

    /// The owner's name for "Update through ..." (empty for the manager's own copies).
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Owner::Manager | Owner::Elsewhere | Owner::ReadOnly => "",
            Owner::Homebrew => "Homebrew",
            Owner::System => "apt",
            Owner::Flatpak => "Flatpak",
            Owner::Snap => "Snap",
            Owner::WindowsStore => "Microsoft Store",
        }
    }
}

// ---------------------------------------------------------------------------
// The places
// ---------------------------------------------------------------------------

/// Where the manager reads and writes: the shared config's folder (`~/.azlin`), the install
/// folder, and the folders the platform's shortcuts go into.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Places {
    /// The folder of `config.json` and of the registry folders.
    pub azlin_dir: PathBuf,
    /// The config file.
    pub config: PathBuf,
    /// Where the apps are installed.
    pub install_dir: PathBuf,
    /// The user's home folder (the shell's rc files).
    pub home: PathBuf,
    /// Linux: `$XDG_DATA_HOME` or `~/.local/share` (`applications/` takes the `.desktop` files).
    pub data_home: PathBuf,
    /// Windows: the Start Menu's `Programs` folder of this user.
    pub start_menu: PathBuf,
    /// The system: the file names and the kind of shortcut.
    pub os: Os,
}

impl Places {
    /// The folders of the app `key`.
    #[must_use]
    pub fn app(&self, key: &str) -> AppPaths {
        AppPaths::new(&self.azlin_dir, &self.install_dir, key)
    }

    /// `~/.azlin/bin`: the links to the programs (macOS).
    #[must_use]
    pub fn bin_dir(&self) -> PathBuf {
        self.azlin_dir.join("bin")
    }
}

// ---------------------------------------------------------------------------
// Installing, updating, reverting, uninstalling
// ---------------------------------------------------------------------------

/// The bytes of every file under `dir`.
#[must_use]
pub fn folder_bytes(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => folder_bytes(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

/// "Cannot write to <dir>": the folder needs rights the manager does not ask for.
fn not_writable(dir: &Path, e: &std::io::Error) -> String {
    format!(
        "cannot write to {} ({e}); choose a folder of your own or install through your \
         package manager",
        dir.display()
    )
}

/// Puts the verified file `file` in place as the app's current version: the new version is laid
/// out in `next/` first (a failure there leaves the installed one untouched), the current one
/// becomes the previous one (the one before that is removed: one is kept), and the registration
/// says so. A staged update of this version is used up.
///
/// # Errors
///
/// What failed, in words; the installed version is still there.
pub fn place(
    places: &Places,
    entry: &AppEntry,
    file: &Path,
    release: &Release,
    channel: Channel,
    now_unix: u64,
) -> Result<Registration, String> {
    let paths = places.app(entry.key);
    std::fs::create_dir_all(&paths.install).map_err(|e| not_writable(&paths.install, &e))?;
    let next = paths.install.join("next");
    let _ = std::fs::remove_dir_all(&next);
    let program = entry.file_name(places.os);
    let laid_out = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(&next)?;
        std::fs::copy(file, next.join(&program))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(next.join(&program), std::fs::Permissions::from_mode(0o755))?;
        }
        write_json(&next.join(RELEASE_FILE), release)
    })();
    if let Err(e) = laid_out {
        let _ = std::fs::remove_dir_all(&next);
        return Err(not_writable(&paths.install, &e));
    }
    let (current, prev) = (paths.current(), paths.prev());
    if current.exists() {
        let _ = std::fs::remove_dir_all(&prev);
        std::fs::rename(&current, &prev).map_err(|e| {
            let _ = std::fs::remove_dir_all(&next);
            format!("{} is in use ({e}); close {} and try again", current.display(), entry.name)
        })?;
    }
    if let Err(e) = std::fs::rename(&next, &current) {
        // Put the installed version back.
        let _ = std::fs::rename(&prev, &current);
        let _ = std::fs::remove_dir_all(&next);
        return Err(format!("{}: {e}", current.display()));
    }
    let before: Option<Registration> = read_json(&paths.registration());
    let registration = Registration {
        name: entry.name.to_string(),
        key: entry.key.to_string(),
        version: release.version.clone(),
        binary: current.join(&program).to_string_lossy().into_owned(),
        channel: channel.name().to_string(),
        size_bytes: folder_bytes(&current),
        updated_unix: now_unix,
        launched_unix: before.map_or(0, |b| b.launched_unix),
        managed: true,
    };
    write_json(&paths.registration(), &registration)
        .map_err(|e| format!("{}: {e}", paths.registration().display()))?;
    if let Some(staged) = read_json::<Staged>(&paths.staged()) {
        if staged.release.version == release.version {
            let _ = std::fs::remove_file(&staged.file);
            let _ = std::fs::remove_file(paths.staged());
        }
    }
    Ok(registration)
}

/// The version kept for Revert (`prev/`), if there is one.
#[must_use]
pub fn previous_release(paths: &AppPaths) -> Option<Release> {
    read_json(&paths.prev().join(RELEASE_FILE))
}

/// Rolls the app back to the version before: `prev/` becomes `current/` and the newer version
/// is removed (one previous version is kept; after a revert there is none). The version rolled
/// back from is skipped from now on (`config`), or auto-update would bring it straight back; a
/// newer one is offered again. The version reverted from, and the new registration.
///
/// # Errors
///
/// What failed: no previous version, or the folder is in use.
pub fn revert(
    places: &Places,
    entry: &AppEntry,
    now_unix: u64,
) -> Result<(String, Registration), String> {
    let paths = places.app(entry.key);
    let previous = previous_release(&paths)
        .ok_or_else(|| format!("{} has no previous version to go back to", entry.name))?;
    let current: Option<Release> = read_json(&paths.current().join(RELEASE_FILE));
    let from = current.map(|r| r.version).unwrap_or_default();
    let dropped = paths.install.join("reverted");
    let _ = std::fs::remove_dir_all(&dropped);
    if paths.current().exists() {
        std::fs::rename(paths.current(), &dropped).map_err(|e| {
            format!("{} is in use ({e}); close {} and try again", paths.current().display(), entry.name)
        })?;
    }
    if let Err(e) = std::fs::rename(paths.prev(), paths.current()) {
        let _ = std::fs::rename(&dropped, paths.current());
        return Err(format!("{}: {e}", paths.prev().display()));
    }
    let _ = std::fs::remove_dir_all(&dropped);
    let before: Registration = read_json(&paths.registration()).unwrap_or_default();
    let program = entry.file_name(places.os);
    let registration = Registration {
        name: entry.name.to_string(),
        key: entry.key.to_string(),
        version: previous.version.clone(),
        binary: paths.current().join(&program).to_string_lossy().into_owned(),
        channel: before.channel,
        size_bytes: folder_bytes(&paths.current()),
        updated_unix: now_unix,
        launched_unix: before.launched_unix,
        managed: true,
    };
    write_json(&paths.registration(), &registration)
        .map_err(|e| format!("{}: {e}", paths.registration().display()))?;
    if !from.is_empty() {
        let skipped = from.clone();
        AzlinConfig::edit(&places.config, |c| {
            c.updates.set_skip_version(entry.key, Some(skipped.as_str()));
        })
        .map_err(|e| format!("{}: {e}", places.config.display()))?;
    }
    // A staged copy of the version just left would be offered again.
    if let Some(staged) = read_json::<Staged>(&paths.staged()) {
        if staged.release.version == from {
            let _ = std::fs::remove_file(&staged.file);
            let _ = std::fs::remove_file(paths.staged());
        }
    }
    Ok((from, registration))
}

/// Removes the app: its shortcuts, its install folder and its registry folder (its data in the
/// data folder stays: that is the user's).
///
/// # Errors
///
/// What could not be removed.
pub fn uninstall(places: &Places, entry: &AppEntry) -> Result<(), String> {
    let paths = places.app(entry.key);
    let program = paths.current().join(entry.file_name(places.os));
    for shortcut in shortcuts(places, entry, &program) {
        shortcut.remove();
    }
    if paths.install.exists() {
        std::fs::remove_dir_all(&paths.install)
            .map_err(|e| format!("{}: {e}", paths.install.display()))?;
    }
    if paths.registry.exists() {
        std::fs::remove_dir_all(&paths.registry)
            .map_err(|e| format!("{}: {e}", paths.registry.display()))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Shortcuts
// ---------------------------------------------------------------------------

/// The marker of the line the manager adds to a shell's rc file.
pub const RC_MARKER: &str = "# Azlin apps (AzInstallManager)";

/// One thing that makes an installed app easy to start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shortcut {
    /// A file with `text` (Linux: the `.desktop` file).
    File { path: PathBuf, text: String },
    /// A symbolic link `link` to `target` (macOS: `~/.azlin/bin/<key>`).
    Link { link: PathBuf, target: PathBuf },
    /// `line` once at the end of the shell's rc file `path`, after [`RC_MARKER`] (macOS: the
    /// PATH entry of `~/.azlin/bin`). Never removed: other apps use it too.
    RcLine { path: PathBuf, line: String },
    /// A Start Menu link (`.lnk`) at `path` to `target`, made by PowerShell's `WScript.Shell`
    /// (Windows has no other way without COM).
    StartMenu { path: PathBuf, target: PathBuf },
}

/// The shortcuts of the app `entry` whose program is `program`: on Linux a `.desktop` file in
/// `~/.local/share/applications`, on macOS a link in `~/.azlin/bin` and that folder in the PATH
/// of zsh (and bash when it has a profile), on Windows a Start Menu entry. None for the runtime.
#[must_use]
pub fn shortcuts(places: &Places, entry: &AppEntry, program: &Path) -> Vec<Shortcut> {
    if entry.is_runtime() {
        return Vec::new();
    }
    match places.os {
        Os::Linux => vec![Shortcut::File {
            path: places
                .data_home
                .join("applications")
                .join(format!("azlin-{}.desktop", entry.key)),
            text: desktop_file(entry, program),
        }],
        Os::MacOs => {
            let line = format!("export PATH=\"{}:$PATH\"", places.bin_dir().display());
            let mut out = vec![
                Shortcut::Link {
                    link: places.bin_dir().join(entry.key),
                    target: program.to_path_buf(),
                },
                Shortcut::RcLine {
                    path: places.home.join(".zshrc"),
                    line: line.clone(),
                },
            ];
            let bash = places.home.join(".bash_profile");
            if bash.exists() {
                out.push(Shortcut::RcLine { path: bash, line });
            }
            out
        }
        Os::Windows => vec![Shortcut::StartMenu {
            path: places
                .start_menu
                .join("Azlin")
                .join(format!("{}.lnk", entry.name)),
            target: program.to_path_buf(),
        }],
    }
}

/// The XDG `.desktop` file of an app.
#[must_use]
pub fn desktop_file(entry: &AppEntry, program: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName={}\nExec=\"{}\" %F\nTerminal=false\n\
         Categories={}\nX-Azlin-Key={}\n",
        entry.name,
        program.display(),
        entry.category,
        entry.key
    )
}

impl Shortcut {
    /// Makes the shortcut. What failed, in words (a shortcut is a convenience: the app is
    /// installed either way).
    ///
    /// # Errors
    ///
    /// Why the shortcut could not be made.
    pub fn make(&self) -> Result<(), String> {
        let mkdir = |path: &Path| {
            path.parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .map_err(|e| format!("{}: {e}", path.display()))
        };
        match self {
            Shortcut::File { path, text } => {
                mkdir(path)?;
                std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
            }
            Shortcut::Link { link, target } => {
                mkdir(link)?;
                let _ = std::fs::remove_file(link);
                #[cfg(unix)]
                {
                    std::os::unix::fs::symlink(target, link)
                        .map_err(|e| format!("{}: {e}", link.display()))
                }
                #[cfg(not(unix))]
                {
                    let _ = target;
                    Ok(())
                }
            }
            Shortcut::RcLine { path, line } => {
                let text = std::fs::read_to_string(path).unwrap_or_default();
                if text.lines().any(|l| l.trim() == line) {
                    return Ok(());
                }
                let mut out = text;
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str(&format!("{RC_MARKER}\n{line}\n"));
                std::fs::write(path, out).map_err(|e| format!("{}: {e}", path.display()))
            }
            Shortcut::StartMenu { path, target } => {
                mkdir(path)?;
                let quote = |p: &Path| p.display().to_string().replace('\'', "''");
                let script = format!(
                    "$s = (New-Object -ComObject WScript.Shell).CreateShortcut('{}'); \
                     $s.TargetPath = '{}'; $s.Save()",
                    quote(path),
                    quote(target)
                );
                let status = std::process::Command::new("powershell")
                    .args(["-NoProfile", "-NonInteractive", "-Command", &script])
                    .status()
                    .map_err(|e| format!("powershell: {e}"))?;
                if status.success() {
                    Ok(())
                } else {
                    Err(format!("powershell could not make {}", path.display()))
                }
            }
        }
    }

    /// Removes the shortcut (an rc line stays: other apps use it).
    pub fn remove(&self) {
        match self {
            Shortcut::File { path, .. } | Shortcut::StartMenu { path, .. } => {
                let _ = std::fs::remove_file(path);
            }
            Shortcut::Link { link, .. } => {
                let _ = std::fs::remove_file(link);
            }
            Shortcut::RcLine { .. } => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Fetching: the first install and an update
// ---------------------------------------------------------------------------

/// What every fetch works with.
#[derive(Clone, Debug, PartialEq)]
pub struct Fetching {
    /// Where the apps and their records are.
    pub places: Places,
    /// The config's `updates` section as it was read when the work started.
    pub updates: UpdatesSection,
    /// The machine's rollout cohort.
    pub cohort: u8,
}

impl Fetching {
    /// The manifest URL of `entry` on `channel`.
    ///
    /// # Errors
    ///
    /// That no update source is configured.
    pub fn manifest(&self, entry: &AppEntry, channel: Channel) -> Result<String, String> {
        manifest_url_for(&self.updates, entry.key, channel).ok_or_else(|| {
            format!(
                "no update source is configured: set updates.manifest_url in {}",
                self.places.config.display()
            )
        })
    }

    fn root_key(&self) -> String {
        self.updates.root_public_key.clone().unwrap_or_default()
    }
}

/// Installs `entry` for the first time from `channel`: its newest release (a first install is
/// not held back by a rollout: the rollout protects installed copies from a bad update), staged
/// and verified, then put in place with its shortcuts. Blocking: run it on a `Thread`. The
/// registration, and the shortcuts that could not be made.
///
/// # Errors
///
/// Why the app could not be installed.
pub fn install_new(
    engine: &dyn UpdateEngine,
    fetching: &Fetching,
    entry: &AppEntry,
    channel: Channel,
    now_unix: u64,
) -> Result<(Registration, Vec<String>), String> {
    let url = fetching.manifest(entry, channel)?;
    let paths = fetching.places.app(entry.key);
    let state = paths.state(channel);
    let ask = |audience| {
        engine.check(&CheckRequest {
            manifest_url: &url,
            current_version: "0",
            channel,
            state_dir: &state,
            cohort: fetching.cohort,
            audience,
        })
    };
    let release = match ask(audience_for(InstallKind::UserWritable, channel)) {
        Check::Available(release) => release,
        Check::UpToDate => match ask(Audience::Tester) {
            Check::Available(release) => release,
            Check::UpToDate => return Err(format!("{} has no release on the {} channel", entry.name, channel.name())),
            Check::Failed(why) => return Err(why),
        },
        Check::Failed(why) => return Err(why),
    };
    let file = engine.stage(&release, &paths.staging(), &fetching.root_key(), &state)?;
    let placed = place(&fetching.places, entry, &file, &release, channel, now_unix);
    let _ = std::fs::remove_file(&file);
    let registration = placed?;
    let problems = shortcuts(&fetching.places, entry, Path::new(&registration.binary))
        .iter()
        .filter_map(|s| s.make().err())
        .collect();
    Ok((registration, problems))
}

/// Updates the installed `entry` to `release`: the staged file when one of this version is
/// there (checked once more first: a file on disk is not trusted across runs), else a fresh
/// download. Blocking: run it on a `Thread`.
///
/// # Errors
///
/// Why the update was not installed; the installed version is still there.
pub fn update_to(
    engine: &dyn UpdateEngine,
    fetching: &Fetching,
    entry: &AppEntry,
    release: &Release,
    channel: Channel,
    now_unix: u64,
) -> Result<Registration, String> {
    let paths = fetching.places.app(entry.key);
    let state = paths.state(channel);
    let staged = read_json::<Staged>(&paths.staged())
        .filter(|s| s.release.version == release.version && Path::new(&s.file).exists());
    let file = match staged {
        Some(staged) => {
            let file = PathBuf::from(&staged.file);
            if let Err(why) = engine.verify(&file, &staged.release, &fetching.root_key(), &state) {
                let _ = std::fs::remove_file(&file);
                let _ = std::fs::remove_file(paths.staged());
                return Err(why);
            }
            file
        }
        None => engine.stage(release, &paths.staging(), &fetching.root_key(), &state)?,
    };
    let placed = place(&fetching.places, entry, &file, release, channel, now_unix);
    let _ = std::fs::remove_file(&file);
    let registration = placed?;
    // Choosing an update by hand ends a skip.
    if fetching.updates.policy_for(entry.key).skip_version.is_some() {
        let _ = AzlinConfig::edit(&fetching.places.config, |c| {
            c.updates.set_skip_version(entry.key, None);
        });
    }
    let program = Path::new(&registration.binary).to_path_buf();
    for shortcut in shortcuts(&fetching.places, entry, &program) {
        let _ = shortcut.make();
    }
    Ok(registration)
}

// ---------------------------------------------------------------------------
// The installed apps
// ---------------------------------------------------------------------------

/// Where an app's row stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// Not checked since the manager started.
    Unchecked,
    Checking,
    UpToDate,
    /// A newer release this machine may have (the rollout reached its cohort, or it is a beta).
    Available(Release),
    /// A newer release, verified and waiting.
    Staged(Release),
    /// The newer release is one the user skips.
    Skipped(String),
    /// Being updated, reverted or removed.
    Working,
    Failed(String),
}

/// One installed app, as the table shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    /// The app.
    pub entry: &'static AppEntry,
    /// Its `current.json`: the version, the program, the size, the last update.
    pub registration: Registration,
    /// Who may update it.
    pub owner: Owner,
    /// Its policy (the config's override, else the shared choice).
    pub channel: Channel,
    pub autoupdate: bool,
    /// The version Revert goes back to.
    pub previous: Option<String>,
    /// What the last check or work came to.
    pub status: Status,
}

impl Installed {
    /// The version the Available column shows (empty: nothing newer).
    #[must_use]
    pub fn available(&self) -> &str {
        match &self.status {
            Status::Available(r) | Status::Staged(r) => &r.version,
            Status::Skipped(v) => v,
            _ => "",
        }
    }

    /// Whether the manager may replace this copy.
    #[must_use]
    pub fn is_managed(&self) -> bool {
        self.owner == Owner::Manager
    }
}

/// Every app with a registration (the runtime too), in the catalog's order, with its owner
/// (`engine`'s install kind of its program) and its policy.
#[must_use]
pub fn installed(engine: &dyn UpdateEngine, places: &Places, updates: &UpdatesSection) -> Vec<Installed> {
    std::iter::once(&RUNTIME)
        .chain(APPS.iter())
        .filter_map(|entry| {
            let paths = places.app(entry.key);
            let registration: Registration = read_json(&paths.registration())?;
            let program = Path::new(&registration.binary);
            let owner = Owner::of(engine.install_kind(program), program, registration.managed);
            let policy = updates.policy_for(entry.key);
            Some(Installed {
                entry,
                previous: previous_release(&paths).map(|r| r.version),
                owner,
                channel: policy.channel,
                autoupdate: policy.autoupdate,
                status: Status::Unchecked,
                registration,
            })
        })
        .collect()
}

/// The background round of an installed app (azul-appkit's [`background_round`]), under the
/// app's policy and the machine's cohort. `None` when no update source is configured.
#[must_use]
pub fn round_for(fetching: &Fetching, app: &Installed) -> Option<Round> {
    let policy = fetching.updates.policy_for(app.entry.key);
    let manifest_url = manifest_url_for(&fetching.updates, app.entry.key, policy.channel)?;
    let kind = if app.is_managed() {
        InstallKind::UserWritable
    } else {
        InstallKind::ReadOnly
    };
    Some(Round {
        paths: fetching.places.app(app.entry.key),
        current_version: app.registration.version.clone(),
        manifest_url,
        channel: policy.channel,
        cohort: fetching.cohort,
        audience: audience_for(kind, policy.channel),
        root_public_key: fetching.root_key(),
        autoupdate: policy.autoupdate && app.is_managed(),
        skip_version: policy.skip_version,
        maintenance_window: fetching.updates.maintenance_window.clone().unwrap_or_default(),
    })
}

/// The status a background round's outcome gives the app's row.
#[must_use]
pub fn status_of(outcome: RoundOutcome) -> Status {
    match outcome {
        RoundOutcome::UpToDate => Status::UpToDate,
        RoundOutcome::Available(r) => Status::Available(r),
        RoundOutcome::Staged(s) => Status::Staged(s.release),
        RoundOutcome::Skipped(v) => Status::Skipped(v),
        RoundOutcome::Failed(why) => Status::Failed(why),
    }
}

/// Checks (and, by the policy, stages) one installed app. Blocking.
#[must_use]
pub fn check_installed(
    engine: &dyn UpdateEngine,
    fetching: &Fetching,
    app: &Installed,
    now_unix: u64,
) -> Status {
    match round_for(fetching, app) {
        Some(round) => status_of(background_round(engine, &round, now_unix)),
        None => Status::Failed(
            fetching
                .manifest(app.entry, app.channel)
                .err()
                .unwrap_or_default(),
        ),
    }
}

/// The newer release of an installed app right now, for an update the user asked for: a skip
/// does not apply and nothing is staged on the way. Blocking.
///
/// # Errors
///
/// That the app is up to date, or why the check failed.
pub fn release_now(
    engine: &dyn UpdateEngine,
    fetching: &Fetching,
    app: &Installed,
    now_unix: u64,
) -> Result<Release, String> {
    let mut round = round_for(fetching, app)
        .ok_or_else(|| fetching.manifest(app.entry, app.channel).err().unwrap_or_default())?;
    round.skip_version = None;
    round.autoupdate = false;
    match background_round(engine, &round, now_unix) {
        RoundOutcome::Available(release) => Ok(release),
        RoundOutcome::Staged(staged) => Ok(staged.release),
        RoundOutcome::UpToDate | RoundOutcome::Skipped(_) => {
            Err(format!("{} is up to date", app.entry.name))
        }
        RoundOutcome::Failed(why) => Err(why),
    }
}

// ---------------------------------------------------------------------------
// The apps' "Update now"
// ---------------------------------------------------------------------------

/// Every app's waiting "Update now" (`<azlin>/apps/<key>/request.json`), by key.
#[must_use]
pub fn pending_requests(azlin_dir: &Path) -> Vec<(String, ApplyRequest)> {
    let Ok(dirs) = std::fs::read_dir(azlin_dir.join(APPS_DIR)) else {
        return Vec::new();
    };
    let mut out: Vec<(String, ApplyRequest)> = dirs
        .flatten()
        .filter_map(|d| {
            let key = d.file_name().to_string_lossy().into_owned();
            entry(&key)?;
            let request = read_json(&d.path().join(REQUEST_FILE))?;
            Some((key, request))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Whether the process `pid` still runs (0: no process to wait for).
#[must_use]
pub fn process_alive(pid: u32) -> bool {
    if pid == 0 || pid == std::process::id() {
        return false;
    }
    #[cfg(target_os = "linux")]
    {
        Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }
    #[cfg(windows)]
    {
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
    }
}

/// Starts the installed program `binary` on its own (the app after its update).
///
/// # Errors
///
/// Why it could not be started.
pub fn launch(binary: &Path) -> Result<(), String> {
    std::process::Command::new(binary)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| format!("{}: {e}", binary.display()))
}

// ---------------------------------------------------------------------------
// Dates and stdout
// ---------------------------------------------------------------------------

/// The UTC date of `unix`: (year, month 1-12, day 1-31, weekday from Monday 0-6).
#[must_use]
pub fn civil_date(unix: u64) -> (i32, u32, u32, u32) {
    let days = i64::try_from(unix / 86_400).unwrap_or(0);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    let year = i32::try_from(yoe + era * 400 + i64::from(month <= 2)).unwrap_or(1970);
    // 1970-01-01 was a Thursday (3, counted from Monday).
    let weekday = u32::try_from((days + 3).rem_euclid(7)).unwrap_or(0);
    (year, month, day, weekday)
}

/// Takes the manager's own switch out of `argv` (without the program name): `--apply <key>`,
/// an app's "Update now" (the app's key, `azwriter`). The rest, in order, is azul-appkit's.
///
/// # Errors
///
/// The reason, for an `--apply` without a key or with one no app has.
pub fn split_switches<I, S>(argv: I) -> Result<(Option<String>, Vec<String>), String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let argv: Vec<String> = argv.into_iter().map(Into::into).collect();
    let mut apply = None;
    let mut rest = Vec::with_capacity(argv.len());
    let mut i = 0;
    while i < argv.len() {
        let key = match argv[i].split_once('=') {
            Some(("--apply", v)) => Some(v.to_string()),
            _ if argv[i] == "--apply" => {
                i += 1;
                Some(argv.get(i).cloned().ok_or("--apply needs an app's key")?)
            }
            _ => None,
        };
        match key {
            Some(key) if entry(&key).is_some() => apply = Some(key),
            Some(key) => return Err(format!("--apply: no app is called {key:?}")),
            None => rest.push(argv[i].clone()),
        }
        i += 1;
    }
    Ok((apply, rest))
}

/// The update source as the user typed it: `None` for an empty field (no source), the address
/// without the spaces around it, or the message key of why it cannot be one. A source is a web
/// address: the manifests are fetched from it, `{app}` and `{channel}` filled in.
pub fn source_from_text(text: &str) -> Result<Option<String>, &'static str> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    if text.starts_with("https://") || text.starts_with("http://") {
        Ok(Some(text.to_string()))
    } else {
        Err("azinstallmgr-source-not-web")
    }
}

/// Whether the apps can be fetched at all: an update source is configured. Nothing is
/// published at a default address, so a first run starts without one.
#[must_use]
pub fn has_source(updates: &UpdatesSection) -> bool {
    manifest_url_for(updates, RUNTIME_KEY, Channel::Stable).is_some()
}

/// `AZINSTALLMGR_STEP <index> <name>`, for scripts.
#[must_use]
pub fn step_line(steps: &[Step], step: Step) -> String {
    let index = steps.iter().position(|s| *s == step).unwrap_or(0);
    format!("AZINSTALLMGR_STEP {index} {}", step.name())
}

/// `AZINSTALLMGR_ROW <key> <installed> <available or -> <status>`, for scripts.
#[must_use]
pub fn row_line(app: &Installed) -> String {
    let status = match &app.status {
        Status::Unchecked => "unchecked",
        Status::Checking => "checking",
        Status::UpToDate => "up-to-date",
        Status::Available(_) => "available",
        Status::Staged(_) => "staged",
        Status::Skipped(_) => "skipped",
        Status::Working => "working",
        Status::Failed(_) => "failed",
    };
    let available = if app.available().is_empty() {
        "-"
    } else {
        app.available()
    };
    format!(
        "AZINSTALLMGR_ROW {} {} {available} {status}",
        app.entry.key, app.registration.version
    )
}

#[cfg(test)]
mod model_tests {
    use std::sync::Mutex;

    use azul_storage::testing::TempDir;

    use super::*;

    #[test]
    fn a_typed_update_source_is_a_web_address_and_an_empty_field_is_none() {
        assert_eq!(
            source_from_text("  https://example.org/{app}/{channel}.json \n"),
            Ok(Some("https://example.org/{app}/{channel}.json".to_string()))
        );
        assert_eq!(source_from_text("http://127.0.0.1:8000/{app}.json"), Ok(Some("http://127.0.0.1:8000/{app}.json".to_string())));
        assert_eq!(source_from_text("   "), Ok(None));
        assert_eq!(source_from_text("example.org/updates"), Err("azinstallmgr-source-not-web"));
        let mut updates = UpdatesSection::default();
        assert!(!has_source(&updates), "a first run has none");
        updates.manifest_url = Some("https://example.org/{app}/{channel}.json".to_string());
        assert!(has_source(&updates));
    }

    /// An engine with one release per app key and a record of what it was asked.
    #[derive(Default)]
    struct FakeEngine {
        /// key -> (version, gated: only a tester gets it)
        releases: Vec<(&'static str, &'static str, bool)>,
        asked: Mutex<Vec<(String, Audience)>>,
        refuse_verify: bool,
    }

    impl FakeEngine {
        fn release_of(&self, url: &str) -> Option<(&'static str, bool)> {
            self.releases
                .iter()
                .find(|(key, _, _)| url.contains(&format!("/{key}/")))
                .map(|(_, v, gated)| (*v, *gated))
        }
    }

    impl UpdateEngine for FakeEngine {
        fn check(&self, q: &CheckRequest<'_>) -> Check {
            self.asked
                .lock()
                .unwrap()
                .push((q.manifest_url.to_string(), q.audience));
            match self.release_of(q.manifest_url) {
                Some((version, gated)) if version != q.current_version => {
                    if gated && q.audience != Audience::Tester {
                        Check::UpToDate
                    } else {
                        Check::Available(Release {
                            version: version.to_string(),
                            download_url: format!("{}/{version}.bin", q.manifest_url),
                            ..Release::default()
                        })
                    }
                }
                Some(_) => Check::UpToDate,
                None => Check::Failed(format!("{}: HTTP 404", q.manifest_url)),
            }
        }
        fn stage(&self, r: &Release, dir: &Path, _: &str, _: &Path) -> Result<PathBuf, String> {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            let file = dir.join(format!("{}.bin", r.version));
            std::fs::write(&file, format!("program {}", r.version)).map_err(|e| e.to_string())?;
            Ok(file)
        }
        fn verify(&self, _: &Path, _: &Release, _: &str, _: &Path) -> Result<(), String> {
            if self.refuse_verify {
                Err("artifact signature invalid".to_string())
            } else {
                Ok(())
            }
        }
        fn install_kind(&self, program: &Path) -> InstallKind {
            if program.starts_with("/usr") || program.starts_with("/opt") {
                InstallKind::SystemPackageManager
            } else {
                InstallKind::UserWritable
            }
        }
        fn draw_cohort(&self) -> u8 {
            37
        }
        fn within_window(&self, _: &str, _: u64) -> bool {
            true
        }
    }

    fn places(dir: &Path, os: Os) -> Places {
        let azlin_dir = dir.join(".azlin");
        Places {
            config: azlin_dir.join("config.json"),
            install_dir: azlin_dir.join("apps"),
            azlin_dir,
            home: dir.to_path_buf(),
            data_home: dir.join(".local/share"),
            start_menu: dir.join("Start Menu/Programs"),
            os,
        }
    }

    fn fetching(dir: &Path) -> Fetching {
        let mut updates = UpdatesSection::default();
        updates.manifest_url = Some("http://example.org/{app}/{channel}.json".to_string());
        Fetching {
            places: places(dir, Os::Linux),
            updates,
            cohort: 50,
        }
    }

    fn writer() -> &'static AppEntry {
        entry("azwriter").unwrap()
    }

    fn release(version: &str) -> Release {
        Release {
            version: version.to_string(),
            ..Release::default()
        }
    }

    fn file_with(dir: &Path, text: &str) -> PathBuf {
        let path = dir.join(format!("{text}.download"));
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn every_production_app_is_offered_once_with_its_runtime_and_a_group() {
        let names: Vec<&str> = APPS.iter().map(|a| a.name).collect();
        for name in [
            "AzDrive", "AzMail", "AzCalendar", "AzContacts", "AzTasks", "AzCode", "AzTerm",
            "AzWriter", "AzNotes", "AzSheets", "AzShow", "AzMeet", "AzMaps", "AzClock",
            "AzCalculator", "AzPhoto", "AzPaint", "AzReview", "AzMusic", "AzPlayer", "AzNews",
            "AzKeys", "AzERP", "AzReader", "AzPdf", "AzMonitor", "AzVideoCut", "AzDashboard",
        ] {
            assert_eq!(names.iter().filter(|n| **n == name).count(), 1, "{name}");
        }
        for a in &APPS {
            assert_eq!(a.key, a.name.to_ascii_lowercase(), "the key is the program's name");
            assert!(a.group < GROUPS.len());
            assert_eq!(entry(a.key), Some(a));
        }
        let rows = component_rows();
        assert!(rows[0].required && rows[0].entry == Some(&RUNTIME));
        assert_eq!(rows.iter().filter(|r| r.entry.is_some()).count(), APPS.len() + 1);
        assert_eq!(rows.iter().filter(|r| r.entry.is_none()).count(), GROUPS.len());
    }

    #[test]
    fn the_space_needed_is_the_apps_and_the_runtime_they_share() {
        assert_eq!(needed_bytes(&[]), 0);
        assert_eq!(needed_bytes(&[writer()]), (15 + 70) * MB);
        assert_eq!(needed_bytes(&[&RUNTIME, writer(), entry("azclock").unwrap()]), (15 + 7 + 70) * MB);
    }

    #[test]
    fn the_file_and_package_names_follow_the_platform() {
        assert_eq!(writer().file_name(Os::Linux), "AzWriter");
        assert_eq!(writer().file_name(Os::Windows), "AzWriter.exe");
        assert_eq!(RUNTIME.file_name(Os::MacOs), "libazul.dylib");
        assert_eq!(writer().package(), "azlin-writer");
        assert_eq!(entry("azvideocut").unwrap().flatpak_id(), "app.azlin.VideoCut");
        assert_eq!(
            PackageManager::Homebrew.install_command(&[&RUNTIME, writer(), entry("azmail").unwrap()]),
            "brew install --cask azlin-writer azlin-mail"
        );
        assert_eq!(
            PackageManager::Apt.install_command(&[writer()]),
            "pkexec sh -c \"apt update && apt install -y azlin-writer\""
        );
        assert_eq!(
            PackageManager::Flatpak.install_command(&[writer()]),
            "flatpak install --user -y azlin app.azlin.Writer"
        );
    }

    #[test]
    fn the_system_package_manager_is_found_in_the_order_each_platform_expects() {
        let dir = TempDir::new("installmgr-path");
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let path_var = format!("/nowhere:{}", bin.display());
        assert_eq!(detect_package_manager(&path_var, Os::Linux), None);
        std::fs::write(bin.join("flatpak"), "").unwrap();
        assert_eq!(detect_package_manager(&path_var, Os::Linux), Some(PackageManager::Flatpak));
        std::fs::write(bin.join("pacman"), "").unwrap();
        assert_eq!(detect_package_manager(&path_var, Os::Linux), Some(PackageManager::Pacman));
        std::fs::write(bin.join("dnf"), "").unwrap();
        assert_eq!(detect_package_manager(&path_var, Os::Linux), Some(PackageManager::Dnf));
        std::fs::write(bin.join("apt"), "").unwrap();
        assert_eq!(detect_package_manager(&path_var, Os::Linux), Some(PackageManager::Apt));
        assert_eq!(detect_package_manager(&path_var, Os::MacOs), None, "brew is macOS's");
        let windows = format!("C:\\nowhere;{}", bin.display());
        assert_eq!(detect_package_manager(&windows, Os::Windows), None);
        std::fs::write(bin.join("winget.exe"), "").unwrap();
        assert_eq!(detect_package_manager(&windows, Os::Windows), Some(PackageManager::Winget));
    }

    /// The manager is a window over the system's package manager: it runs it itself, the
    /// system's rights asked for by pkexec where the package manager needs root.
    #[test]
    fn a_package_manager_installs_the_ticked_apps_itself() {
        let apps = [&RUNTIME, writer()];
        let runs = |pm: PackageManager| pm.install_runs(&apps);
        let words = |run: &[&str]| run.iter().map(|w| w.to_string()).collect::<Vec<_>>();
        assert_eq!(runs(PackageManager::Homebrew), vec![words(&["brew", "install", "--cask", "azlin-writer"])]);
        assert_eq!(
            runs(PackageManager::Apt),
            vec![words(&["pkexec", "sh", "-c", "apt update && apt install -y azlin-writer"])]
        );
        assert_eq!(runs(PackageManager::Dnf), vec![words(&["pkexec", "dnf", "install", "-y", "azlin-writer"])]);
        assert_eq!(
            runs(PackageManager::Pacman),
            vec![words(&["pkexec", "pacman", "-S", "--noconfirm", "azlin-writer"])]
        );
        assert_eq!(
            runs(PackageManager::Flatpak),
            vec![words(&["flatpak", "install", "--user", "-y", "azlin", "app.azlin.Writer"])]
        );
        let two = PackageManager::Winget.install_runs(&[writer(), entry("azmail").unwrap()]);
        assert_eq!(two.len(), 2, "winget installs one package per call");
        assert_eq!(&two[0][..4], &words(&["winget", "install", "--id", "Azlin.AzWriter"])[..]);
        assert_eq!(
            PackageManager::Homebrew.install_command(&apps),
            "brew install --cask azlin-writer",
            "what the page shows is what runs"
        );
        assert_eq!(
            PackageManager::Apt.install_command(&apps),
            "pkexec sh -c \"apt update && apt install -y azlin-writer\""
        );
    }

    #[test]
    fn with_a_package_manager_the_wizard_installs_through_it_and_azlins_own_way_is_the_fallback() {
        let through = steps(Some(PackageManager::Apt), true);
        assert_eq!(
            through,
            vec![Step::Welcome, Step::PackageManager, Step::Components, Step::Ready, Step::Installing, Step::Finish],
            "no install folder and no update channel: the package manager's"
        );
        let own = steps(Some(PackageManager::Apt), false);
        assert_eq!(own[1], Step::PackageManager, "the page where the choice is made");
        assert!(own.contains(&Step::Destination) && own.contains(&Step::Channel));
        let plain = steps(None, true);
        assert_eq!(plain[1], Step::Destination, "no package manager: Azlin's own way only");
        assert!(!plain.contains(&Step::PackageManager));
        assert_eq!(next_step(&through, Step::PackageManager), Step::Components);
        assert_eq!(next_step(&plain, Step::Welcome), Step::Destination);
        assert_eq!(previous_step(&plain, Step::Destination), Step::Welcome);
        assert_eq!(previous_step(&plain, Step::Installing), Step::Installing);
        assert_eq!(next_step(&plain, Step::Finish), Step::Finish);
        assert_eq!(step_line(&own, Step::Components), "AZINSTALLMGR_STEP 3 components");
    }

    #[test]
    fn an_installed_copys_owner_decides_who_updates_it() {
        let usr = Path::new("/usr/bin/AzWriter");
        assert_eq!(Owner::of(InstallKind::SystemPackageManager, usr, false), Owner::System);
        let brew = Path::new("/opt/homebrew/Caskroom/azlin-writer/2.0/AzWriter.app");
        assert_eq!(Owner::of(InstallKind::SystemPackageManager, brew, false), Owner::Homebrew);
        let home = Path::new("/home/ann/.azlin/apps/azwriter/current/AzWriter");
        assert_eq!(Owner::of(InstallKind::UserWritable, home, true), Owner::Manager);
        assert_eq!(Owner::of(InstallKind::UserWritable, home, false), Owner::Elsewhere);
        assert_eq!(Owner::Homebrew.update_command(writer()), "brew upgrade --cask azlin-writer");
        assert_eq!(Owner::Flatpak.update_command(writer()), "flatpak update app.azlin.Writer");
        assert_eq!(Owner::Manager.update_command(writer()), "");
        assert_eq!(Owner::Homebrew.name(), "Homebrew");
    }

    #[test]
    fn installing_puts_the_newest_release_in_place_with_its_desktop_file() {
        let dir = TempDir::new("installmgr-install");
        let engine = FakeEngine {
            releases: vec![("azwriter", "2.0.0", true)],
            ..FakeEngine::default()
        };
        let f = fetching(dir.path());
        let (reg, problems) = install_new(&engine, &f, writer(), Channel::Stable, 1000).unwrap();
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(reg.version, "2.0.0");
        assert!(reg.managed);
        assert_eq!(reg.channel, "stable");
        let paths = f.places.app("azwriter");
        assert_eq!(Path::new(&reg.binary), paths.current().join("AzWriter"));
        assert_eq!(std::fs::read_to_string(&reg.binary).unwrap(), "program 2.0.0");
        assert_eq!(reg.size_bytes, folder_bytes(&paths.current()));
        assert_eq!(read_json::<Registration>(&paths.registration()), Some(reg.clone()));
        let asked = engine.asked.lock().unwrap().clone();
        assert_eq!(
            asked.iter().map(|(_, a)| *a).collect::<Vec<_>>(),
            vec![Audience::Gated, Audience::Tester],
            "a first install is not held back by a rollout that has not reached this machine"
        );
        let desktop = dir.path().join(".local/share/applications/azlin-azwriter.desktop");
        let text = std::fs::read_to_string(desktop).unwrap();
        assert!(text.contains("Name=AzWriter") && text.contains(&reg.binary), "{text}");
        assert!(!paths.staging().join("2.0.0.bin").exists(), "the download is used up");
    }

    #[test]
    fn without_an_update_source_or_a_release_nothing_is_installed() {
        let dir = TempDir::new("installmgr-nosource");
        let engine = FakeEngine::default();
        let mut f = fetching(dir.path());
        let err = install_new(&engine, &f, writer(), Channel::Stable, 1).unwrap_err();
        assert!(err.contains("404"), "{err}");
        f.updates.manifest_url = None;
        let err = install_new(&engine, &f, writer(), Channel::Stable, 1).unwrap_err();
        assert!(err.contains("updates.manifest_url"), "{err}");
        assert!(!f.places.app("azwriter").current().exists());
    }

    #[test]
    fn an_update_keeps_one_previous_version_and_revert_goes_back_and_skips_the_newer_one() {
        let dir = TempDir::new("installmgr-revert");
        let p = places(dir.path(), Os::Linux);
        let paths = p.app("azwriter");
        place(&p, writer(), &file_with(dir.path(), "v1"), &release("1.0.0"), Channel::Stable, 1).unwrap();
        place(&p, writer(), &file_with(dir.path(), "v2"), &release("2.0.0"), Channel::Stable, 2).unwrap();
        let reg = place(&p, writer(), &file_with(dir.path(), "v3"), &release("3.0.0"), Channel::Beta, 3).unwrap();
        assert_eq!((reg.version.as_str(), reg.channel.as_str()), ("3.0.0", "beta"));
        assert_eq!(previous_release(&paths).map(|r| r.version).as_deref(), Some("2.0.0"));
        let kept: Vec<String> = std::fs::read_dir(&paths.install)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n == "current" || n == "prev" || n == "next")
            .collect();
        assert_eq!(kept.len(), 2, "current and one previous version: {kept:?}");

        let (from, reg) = revert(&p, writer(), 4).unwrap();
        assert_eq!(from, "3.0.0");
        assert_eq!(reg.version, "2.0.0");
        assert_eq!(std::fs::read_to_string(&reg.binary).unwrap(), "v2");
        assert_eq!(reg.channel, "beta", "the app stays on its channel");
        assert_eq!(previous_release(&paths), None, "after a revert there is nothing to go back to");
        assert!(revert(&p, writer(), 5).is_err());
        let (config, _) = AzlinConfig::load(&p.config);
        assert_eq!(
            config.updates.policy_for("azwriter").skip_version.as_deref(),
            Some("3.0.0"),
            "auto-update does not bring the reverted version straight back"
        );
    }

    #[test]
    fn an_update_uses_the_staged_file_after_checking_it_again_and_ends_a_skip() {
        let dir = TempDir::new("installmgr-update");
        let f = fetching(dir.path());
        let paths = f.places.app("azwriter");
        place(&f.places, writer(), &file_with(dir.path(), "v1"), &release("1.0.0"), Channel::Stable, 1).unwrap();
        AzlinConfig::edit(&f.places.config, |c| c.updates.set_skip_version("azwriter", Some("1.5.0"))).unwrap();
        let mut f = f;
        f.updates = AzlinConfig::load(&f.places.config).0.updates;
        std::fs::create_dir_all(paths.staging()).unwrap();
        let staged_file = paths.staging().join("2.0.0.bin");
        std::fs::write(&staged_file, "staged 2.0.0").unwrap();
        write_json(
            &paths.staged(),
            &Staged {
                release: release("2.0.0"),
                file: staged_file.to_string_lossy().into_owned(),
                channel: "stable".into(),
                staged_unix: 5,
            },
        )
        .unwrap();

        let refusing = FakeEngine {
            refuse_verify: true,
            ..FakeEngine::default()
        };
        let err = update_to(&refusing, &f, writer(), &release("2.0.0"), Channel::Stable, 6).unwrap_err();
        assert!(err.contains("signature"), "{err}");
        assert!(!staged_file.exists() && !paths.staged().exists(), "a refused file is thrown away");
        assert_eq!(read_json::<Registration>(&paths.registration()).unwrap().version, "1.0.0");

        std::fs::write(&staged_file, "staged 2.0.0").unwrap();
        write_json(
            &paths.staged(),
            &Staged {
                release: release("2.0.0"),
                file: staged_file.to_string_lossy().into_owned(),
                channel: "stable".into(),
                staged_unix: 5,
            },
        )
        .unwrap();
        let reg = update_to(&FakeEngine::default(), &f, writer(), &release("2.0.0"), Channel::Stable, 7).unwrap();
        assert_eq!(std::fs::read_to_string(&reg.binary).unwrap(), "staged 2.0.0");
        assert!(!paths.staged().exists(), "the staged update is used up");
        let (config, _) = AzlinConfig::load(&f.places.config);
        assert_eq!(config.updates.policy_for("azwriter").skip_version, None);
    }

    #[test]
    fn uninstall_removes_the_program_its_records_and_its_shortcuts() {
        let dir = TempDir::new("installmgr-uninstall");
        let engine = FakeEngine {
            releases: vec![("azwriter", "2.0.0", false)],
            ..FakeEngine::default()
        };
        let f = fetching(dir.path());
        install_new(&engine, &f, writer(), Channel::Stable, 1).unwrap();
        let desktop = dir.path().join(".local/share/applications/azlin-azwriter.desktop");
        assert!(desktop.exists());
        uninstall(&f.places, writer()).unwrap();
        let paths = f.places.app("azwriter");
        assert!(!paths.install.exists() && !paths.registry.exists() && !desktop.exists());
    }

    #[test]
    fn macos_links_the_program_into_the_azlin_bin_folder_on_the_path_once() {
        let dir = TempDir::new("installmgr-macos");
        let p = places(dir.path(), Os::MacOs);
        let program = Path::new("/x/AzWriter");
        let made = shortcuts(&p, writer(), program);
        assert_eq!(
            made[0],
            Shortcut::Link {
                link: p.azlin_dir.join("bin/azwriter"),
                target: program.to_path_buf()
            }
        );
        for s in &made {
            s.make().unwrap();
        }
        for s in &made {
            s.make().unwrap();
        }
        let rc = std::fs::read_to_string(dir.path().join(".zshrc")).unwrap();
        assert_eq!(rc.matches(RC_MARKER).count(), 1, "the PATH line is added once: {rc}");
        assert!(rc.contains(&p.bin_dir().display().to_string()));
        assert!(shortcuts(&p, &RUNTIME, program).is_empty());
        let windows = places(dir.path(), Os::Windows);
        assert!(matches!(
            &shortcuts(&windows, writer(), program)[0],
            Shortcut::StartMenu { path, .. } if path.ends_with("Azlin/AzWriter.lnk")
        ));
    }

    #[test]
    fn the_installed_apps_carry_their_owner_policy_and_previous_version() {
        let dir = TempDir::new("installmgr-installed");
        let f = fetching(dir.path());
        place(&f.places, writer(), &file_with(dir.path(), "a"), &release("1.0.0"), Channel::Stable, 1).unwrap();
        place(&f.places, writer(), &file_with(dir.path(), "b"), &release("1.1.0"), Channel::Stable, 2).unwrap();
        let mail = f.places.app("azmail");
        write_json(
            &mail.registration(),
            &Registration {
                name: "AzMail".into(),
                key: "azmail".into(),
                version: "3.0.0".into(),
                binary: "/usr/bin/AzMail".into(),
                ..Registration::default()
            },
        )
        .unwrap();
        let mut updates = f.updates.clone();
        updates.set_channel(Some("azmail"), Channel::Beta);
        let apps = installed(&FakeEngine::default(), &f.places, &updates);
        assert_eq!(apps.iter().map(|a| a.entry.key).collect::<Vec<_>>(), vec!["azwriter", "azmail"]);
        assert_eq!(apps[0].owner, Owner::Manager);
        assert_eq!(apps[0].previous.as_deref(), Some("1.0.0"));
        assert_eq!(apps[1].owner, Owner::System);
        assert_eq!(apps[1].channel, Channel::Beta);
        let round = round_for(&f, &apps[1]).unwrap();
        assert_eq!(round.audience, Audience::NotifyOnly, "a package manager's copy waits for the full rollout");
        assert!(!round.autoupdate, "the manager never stages for someone else's copy");
        let round = round_for(&f, &apps[0]).unwrap();
        assert_eq!((round.audience, round.cohort), (Audience::Gated, 50));
        assert_eq!(round.manifest_url, "http://example.org/azwriter/stable.json");
        assert_eq!(row_line(&apps[0]), "AZINSTALLMGR_ROW azwriter 1.1.0 - unchecked");
    }

    #[test]
    fn the_apps_requests_are_found_by_key_and_a_finished_process_is_not_waited_for() {
        let dir = TempDir::new("installmgr-requests");
        let p = places(dir.path(), Os::Linux);
        azul_appkit::update::request_apply(
            &p.app("azwriter"),
            &ApplyRequest {
                version: "2.0.0".into(),
                pid: 0,
                relaunch: true,
                asked_unix: 1,
            },
        )
        .unwrap();
        std::fs::create_dir_all(p.azlin_dir.join("apps/not-an-app")).unwrap();
        std::fs::write(p.azlin_dir.join("apps/not-an-app/request.json"), "{}").unwrap();
        let pending = pending_requests(&p.azlin_dir);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].0, "azwriter");
        assert!(!process_alive(0));
        assert!(!process_alive(std::process::id()), "the manager never waits for itself");
    }

    #[test]
    fn the_apply_switch_names_an_app_and_the_rest_is_the_kits() {
        let (apply, rest) = split_switches(["--apply", "azwriter", "--theme=flora"]).unwrap();
        assert_eq!(apply.as_deref(), Some("azwriter"));
        assert_eq!(rest, vec!["--theme=flora"]);
        let (apply, rest) = split_switches(["--apply=azmail"]).unwrap();
        assert_eq!((apply.as_deref(), rest.len()), (Some("azmail"), 0));
        assert!(split_switches(["--apply"]).is_err());
        assert!(split_switches(["--apply", "notepad"]).is_err());
        assert_eq!(split_switches(Vec::<String>::new()).unwrap(), (None, Vec::new()));
    }

    #[test]
    fn a_date_is_read_in_utc() {
        assert_eq!(civil_date(0), (1970, 1, 1, 3));
        // 2026-10-12T10:00:00Z, a Monday.
        assert_eq!(civil_date(1_791_799_200), (2026, 10, 12, 0));
        assert_eq!(civil_date(951_782_400), (2000, 2, 29, 1));
    }
}
