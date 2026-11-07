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
    anchors::{self, NoRootKey, UpdatePolicy},
    azlin_config::{AzlinConfig, Channel, UpdatesSection},
    update::{
        audience_for, background_round, copy_verified, manifest_url_for, read_json,
        read_small_text, sha256_of_file, write_json, AppPaths, ApplyRequest, Audience, Check,
        CheckRequest, InstallKind, Registration, Release, Round, RoundOutcome, Staged,
        UpdateEngine, Verified, APPS_DIR, RELEASE_FILE, REQUEST_FILE, STATE_FILE_MAX,
    },
};

/// One megabyte.
pub const MB: u64 = 1024 * 1024;

/// The file in a version's folder that holds the SHA-256 of its program as it was installed
/// (the hash of the bytes that passed the engine's checks): Revert checks the version it brings
/// back against it, since that version has not been through the engine since.
const PROGRAM_HASH_FILE: &str = "program.sha256";

/// The folder the version being replaced waits in while its successor is moved into place.
const REPLACED_DIR: &str = "replaced";

/// The folder the version Revert leaves waits in while the previous one is moved into place.
const REVERTED_DIR: &str = "reverted";

/// The rollback marks the engine keeps for an app: `<registry>/state/<channel>/update-state.json`
/// (azul-layout's `UpdateState`). They outlive the program: see [`uninstall`].
const UPDATE_STATE_FILE: &str = "update-state.json";

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
    app("azpasswordmanager", "AzPasswordManager", "key", "azinstallmgr-app-azpasswordmanager", 11, "Utility;Security;", 3),
    app("azmaps", "AzMaps", "map", "azinstallmgr-app-azmaps", 16, "Utility;Maps;", 3),
    app("azclock", "AzClock", "schedule", "azinstallmgr-app-azclock", 7, "Utility;Clock;", 3),
    app("azcalculator", "AzCalculator", "calculate", "azinstallmgr-app-azcalculator", 7, "Utility;Calculator;", 3),
    app("azcode", "AzCode", "code", "azinstallmgr-app-azcode", 18, "Development;TextEditor;", 3),
    app("azterm", "AzTerm", "terminal", "azinstallmgr-app-azterm", 12, "System;TerminalEmulator;", 3),
    app("azmonitor", "AzMonitor", "monitor_heart", "azinstallmgr-app-azmonitor", 10, "System;Monitor;", 3),
    app("azdashboard", "AzDashboard", "dashboard", "azinstallmgr-app-azdashboard", 11, "Office;Finance;", 3),
    app("azerp", "AzERP", "inventory_2", "azinstallmgr-app-azerp", 13, "Office;Finance;", 3),
];

/// The entry of `key` (the runtime too); a renamed app's former key is the app under its new
/// name (an "Update now" of the old program, its registration folder).
#[must_use]
pub fn entry(key: &str) -> Option<&'static AppEntry> {
    if key == RUNTIME_KEY {
        return Some(&RUNTIME);
    }
    let key = azul_appkit::azlin_config::current_app_key(key);
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
pub fn steps(package_manager: Option<PackageManager>, through_manager: bool) -> Vec<Step> {
    let mut steps = vec![Step::Welcome];
    if package_manager.is_some() {
        steps.push(Step::PackageManager);
    }
    if package_manager.is_some() && through_manager {
        // The package manager decides where the apps go and how they are updated.
        steps.extend([Step::Components, Step::Ready, Step::Installing, Step::Finish]);
    } else {
        steps.extend([
            Step::Destination,
            Step::Components,
            Step::Channel,
            Step::Ready,
            Step::Installing,
            Step::Finish,
        ]);
    }
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

    /// Whether installing through it asks for administrator rights (`pkexec`). Such a manager is
    /// never run by Setup: the packages `azlin-<name>` of the system's repositories are names
    /// nobody has proved to be Azlin's (a squatted one would install as root), so Setup shows the
    /// command and the user runs it (X-DELTA-AZDRIVE-KEYS-36, AZ-Y14).
    #[must_use]
    pub const fn runs_as_root(self) -> bool {
        matches!(
            self,
            PackageManager::Apt | PackageManager::Dnf | PackageManager::Pacman
        )
    }

    /// The commands (program and arguments) that install `apps` through it, run one after the
    /// other.
    #[must_use]
    pub fn install_runs(self, apps: &[&AppEntry]) -> Vec<Vec<String>> {
        let apps: Vec<&&AppEntry> = apps.iter().filter(|a| !a.is_runtime()).collect();
        if apps.is_empty() {
            return Vec::new();
        }
        let packages: Vec<String> = apps.iter().map(|a| a.package()).collect();
        let run = |head: &[&str], tail: Vec<String>| -> Vec<String> {
            head.iter().map(|w| (*w).to_string()).chain(tail).collect()
        };
        match self {
            PackageManager::Homebrew => vec![run(&["brew", "install", "--cask"], packages)],
            PackageManager::Apt => vec![run(
                &["pkexec", "sh", "-c"],
                vec![format!("apt update && apt install -y {}", packages.join(" "))],
            )],
            PackageManager::Dnf => vec![run(&["pkexec", "dnf", "install", "-y"], packages)],
            PackageManager::Pacman => vec![run(&["pkexec", "pacman", "-S", "--noconfirm"], packages)],
            PackageManager::Flatpak => vec![run(
                &["flatpak", "install", "--user", "-y", "azlin"],
                apps.iter().map(|a| a.flatpak_id()).collect(),
            )],
            PackageManager::Winget => apps
                .iter()
                .map(|a| {
                    run(
                        &["winget", "install", "--id"],
                        vec![
                            a.winget_id(),
                            String::from("-e"),
                            String::from("--accept-package-agreements"),
                            String::from("--accept-source-agreements"),
                        ],
                    )
                })
                .collect(),
        }
    }

    /// The file that shows it is installed, on `os`'s search path.
    #[must_use]
    pub fn program_file(self, os: Os) -> String {
        if os == Os::Windows {
            format!("{}.exe", self.program())
        } else {
            self.program().to_string()
        }
    }

    /// The commands that install `apps` through it, as the page shows them (the manager runs
    /// the same, [`PackageManager::install_runs`]); joined by `&&`.
    #[must_use]
    pub fn install_command(self, apps: &[&AppEntry]) -> String {
        self.install_runs(apps)
            .iter()
            .map(|run| shell_words(run))
            .collect::<Vec<_>>()
            .join(" && ")
    }
}

/// A command as a shell line: a word with a space in double quotes.
#[must_use]
pub fn shell_words(run: &[String]) -> String {
    run.iter()
        .map(|word| {
            if word.contains(' ') {
                format!("\"{word}\"")
            } else {
                word.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Runs one of [`PackageManager::install_runs`]' commands and waits for it (on a worker
/// thread): its output's lines, or why it failed (its exit and its last line). The package
/// manager asks for its own rights (pkexec's dialog).
pub fn run_command(run: &[String]) -> Result<Vec<String>, String> {
    let Some((program, args)) = run.split_first() else {
        return Ok(Vec::new());
    };
    let out = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    let mut lines: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .chain(String::from_utf8_lossy(&out.stderr).lines())
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();
    if lines.len() > 200 {
        lines.drain(..lines.len() - 200);
    }
    if out.status.success() {
        Ok(lines)
    } else {
        let last = lines.last().cloned().unwrap_or_default();
        Err(match out.status.code() {
            Some(code) => format!("{program} ended with {code}: {last}"),
            None => format!("{program} was stopped: {last}"),
        })
    }
}

/// The first package manager the search path `path_var` (`PATH`) has, in the order the
/// platform's users expect: Homebrew on macOS; apt, dnf, pacman, then Flatpak on Linux;
/// winget on Windows.
#[must_use]
pub fn detect_package_manager(path_var: &str, os: Os) -> Option<PackageManager> {
    let order: &[PackageManager] = match os {
        Os::MacOs => &[PackageManager::Homebrew],
        Os::Linux => &[
            PackageManager::Apt,
            PackageManager::Dnf,
            PackageManager::Pacman,
            PackageManager::Flatpak,
        ],
        Os::Windows => &[PackageManager::Winget],
    };
    let separator = if os == Os::Windows { ';' } else { ':' };
    order.iter().copied().find(|pm| {
        path_var
            .split(separator)
            .filter(|dir| !dir.is_empty())
            .any(|dir| Path::new(dir).join(pm.program_file(os)).is_file())
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

/// Puts the VERIFIED file `verified` in place as the app's current version: the new version is
/// laid out in `next/` first (a failure there leaves the installed one untouched; the copy is
/// hashed and has to be the bytes that passed the engine's checks, so a file swapped between the
/// check and now is not installed, and the hash is kept beside it for Revert), the current one
/// becomes the previous one, and the registration says so. A staged update of this version is used
/// up.
///
/// The order protects both versions that are kept: the installed one moves aside to `replaced/`,
/// the new one takes its name, and only then does the old previous version go and `replaced/`
/// become `prev/`. A move that fails ("in use" on Windows) leaves the previous version where it was
/// (it used to be deleted first, and with it the Revert target), and a crash in the middle is
/// finished or undone by [`settle`].
///
/// # Errors
///
/// What failed, in words; the installed version is still there.
pub fn place(
    places: &Places,
    entry: &AppEntry,
    verified: &Verified,
    release: &Release,
    channel: Channel,
    now_unix: u64,
) -> Result<Registration, String> {
    let paths = places.app(entry.key);
    std::fs::create_dir_all(&paths.install).map_err(|e| not_writable(&paths.install, &e))?;
    settle(&paths);
    let next = paths.install.join("next");
    let _ = std::fs::remove_dir_all(&next);
    let program = entry.file_name(places.os);
    let laid_out = (|| -> Result<(), String> {
        std::fs::create_dir_all(&next).map_err(|e| not_writable(&paths.install, &e))?;
        copy_verified(verified, &next.join(&program))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(next.join(&program), std::fs::Permissions::from_mode(0o755))
                .map_err(|e| not_writable(&paths.install, &e))?;
        }
        std::fs::write(next.join(PROGRAM_HASH_FILE), verified.sha256())
            .map_err(|e| not_writable(&paths.install, &e))?;
        write_json(&next.join(RELEASE_FILE), release).map_err(|e| not_writable(&paths.install, &e))
    })();
    if let Err(why) = laid_out {
        let _ = std::fs::remove_dir_all(&next);
        return Err(why);
    }
    let current = paths.current();
    if let Err(why) = activate(&paths, entry.name, |from, to| std::fs::rename(from, to)) {
        let _ = std::fs::remove_dir_all(&next);
        return Err(why);
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
    forget_staged(&paths, &release.version);
    Ok(registration)
}

/// The folders of an app's install folder that the manager makes, and no other: the installed
/// version, the one before it, the one being laid out, and the two that wait while a move is made.
fn made_folders(paths: &AppPaths) -> [PathBuf; 5] {
    [
        paths.current(),
        paths.prev(),
        paths.install.join("next"),
        paths.install.join(REPLACED_DIR),
        paths.install.join(REVERTED_DIR),
    ]
}

/// Moves the version laid out in `next/` into `current/`: the installed one waits in `replaced/`
/// meanwhile, the one kept for Revert (`prev/`) is not touched until the new one is in place, and
/// then [`settle`] makes the replaced one the previous one. A move that fails puts the installed
/// version back. `rename` is a parameter so a refused move ("in use" on Windows) can be made to
/// happen in a test.
fn activate(
    paths: &AppPaths,
    name: &str,
    rename: impl Fn(&Path, &Path) -> std::io::Result<()>,
) -> Result<(), String> {
    let (next, current) = (paths.install.join("next"), paths.current());
    let replaced = paths.install.join(REPLACED_DIR);
    if current.exists() {
        rename(&current, &replaced).map_err(|e| {
            format!(
                "{} is in use ({e}); close {name} and try again",
                current.display()
            )
        })?;
    }
    if let Err(e) = rename(&next, &current) {
        // Put the installed version back.
        if replaced.exists() {
            let _ = rename(&replaced, &current);
        }
        return Err(format!("{}: {e}", current.display()));
    }
    settle(paths);
    Ok(())
}

/// Finishes or undoes what a crash (or a refused move) left in the middle of [`place`]: a
/// `replaced/` folder with no `current/` is the installed version, moved aside and not replaced
/// (it is put back); next to a `current/` it is the version that was replaced (the previous
/// version before it goes, and it becomes `prev/`). Cheap when there is nothing to do.
pub fn settle(paths: &AppPaths) {
    let replaced = paths.install.join(REPLACED_DIR);
    if replaced.exists() && !replaced.is_dir() {
        let _ = std::fs::remove_file(&replaced);
    }
    if replaced.is_dir() {
        if paths.current().exists() {
            let _ = std::fs::remove_dir_all(paths.prev());
            // The replaced version becomes the one Revert goes back to. When it cannot (on Windows a
            // file of the old `prev/` held open leaves that folder half gone, and the move over it
            // fails), it STAYS where it is and the next settle tries again: deleting it threw away
            // the Revert target without a word (X-DELTA-AZDRIVE-KEYS-35).
            if let Err(e) = std::fs::rename(&replaced, paths.prev()) {
                eprintln!(
                    "[installmgr] the previous version stays in {}: {e}",
                    replaced.display()
                );
            }
        } else {
            let _ = std::fs::rename(&replaced, paths.current());
        }
    }
    // The same for a Revert that stopped between its two moves ([`revert`]): the version being
    // left is the installed one while `current/` is missing, a leftover once it is there.
    let left = paths.install.join(REVERTED_DIR);
    if left.is_dir() {
        if paths.current().exists() {
            let _ = std::fs::remove_dir_all(&left);
        } else {
            let _ = std::fs::rename(&left, paths.current());
        }
    }
}

/// Whether the program in `dir` is the one whose hash was recorded when it was installed
/// ([`PROGRAM_HASH_FILE`]).
fn program_matches_its_record(dir: &Path, program: &str) -> Result<(), String> {
    // A hash is 64 digits: the file is read as far as a hash goes and no further (any process of the
    // user can write it, X-DELTA-AZDRIVE-KEYS-35).
    let recorded = read_small_text(&dir.join(PROGRAM_HASH_FILE), 256)
        .ok_or_else(|| "it has no recorded hash (it was not placed by this manager)".to_string())?;
    let actual = sha256_of_file(&dir.join(program))?;
    if actual.eq_ignore_ascii_case(recorded.trim()) {
        Ok(())
    } else {
        Err(format!(
            "its program is not the one that was installed (sha256:{} then, sha256:{actual} now)",
            recorded.trim()
        ))
    }
}

/// Throws away the staged update of `version`: its file (the one the code names, never the one
/// `staged.json` does: REVIEW Y4) and the record, when the record is of that version.
fn forget_staged(paths: &AppPaths, version: &str) {
    if let Some(file) = paths.staged_file(version) {
        let _ = std::fs::remove_file(file);
    }
    if read_json::<Staged>(&paths.staged()).is_some_and(|s| s.release.version == version) {
        let _ = std::fs::remove_file(paths.staged());
    }
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
    settle(&paths);
    let previous = previous_release(&paths)
        .ok_or_else(|| format!("{} has no previous version to go back to", entry.name))?;
    // The version brought back has not been through the engine since it was installed: it has to
    // be the program whose hash was recorded then (REVIEW Y7: it was re-activated unchecked).
    program_matches_its_record(&paths.prev(), &entry.file_name(places.os)).map_err(|why| {
        format!(
            "the previous version of {} is not restored: {why}",
            entry.name
        )
    })?;
    let current: Option<Release> = read_json(&paths.current().join(RELEASE_FILE));
    let from = current.map(|r| r.version).unwrap_or_default();
    let dropped = paths.install.join(REVERTED_DIR);
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
    forget_staged(&paths, &from);
    Ok((from, registration))
}

/// The rollback marks the engine keeps for the app (`state/<channel>/update-state.json`): the
/// files and their bytes. A folder with no state is no marks; a state folder or a file that cannot
/// be read is an error and not "no marks": the uninstall that does not know them would take
/// them with the registry.
/// The bytes of `path`, at most `max` of them: a file past that is an error (`InvalidData`) and is
/// not read into memory whole. The marks are no file to drop when they are too big - the uninstall
/// stops before it removes anything (X-DELTA-AZDRIVE-KEYS-35).
fn read_bounded(path: &Path, max: u64) -> std::io::Result<Vec<u8>> {
    use std::io::Read as _;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(max.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("it is bigger than {max} bytes"),
        ));
    }
    Ok(bytes)
}

fn rollback_marks(paths: &AppPaths) -> Result<Vec<(PathBuf, Vec<u8>)>, String> {
    let state = paths.registry.join("state");
    let channels = match std::fs::read_dir(&state) {
        Ok(channels) => channels,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", state.display())),
    };
    let mut marks = Vec::new();
    for channel in channels {
        let channel = channel.map_err(|e| format!("{}: {e}", state.display()))?;
        let file = channel.path().join(UPDATE_STATE_FILE);
        match read_bounded(&file, STATE_FILE_MAX) {
            Ok(bytes) => marks.push((file, bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{}: {e}", file.display())),
        }
    }
    Ok(marks)
}

/// Removes the app: its shortcuts, its install folder and its registry folder (its data in the
/// data folder stays: that is the user's). Its rollback marks stay: they hold the highest key
/// generation the engine accepted, and a reinstall that began again at generation 0 would accept a
/// retired, unexpired signing key once more (REVIEW Y7). They are no user data and no program,
/// just two numbers and a version per channel.
///
/// The marks are read BEFORE anything is removed: in the default layout the install folder
/// (`<azlin>/apps/<key>`) and the registry folder are ONE folder, and the first removal takes the
/// marks with it (X-DELTA-AZDRIVE-KEYS-11); a folder that holds them and cannot be read stops the
/// uninstall before it removes a file.
///
/// # Errors
///
/// What could not be removed, or the marks that could not be read.
pub fn uninstall(places: &Places, entry: &AppEntry) -> Result<(), String> {
    let paths = places.app(entry.key);
    let marks = rollback_marks(&paths)?;
    let program = paths.current().join(entry.file_name(places.os));
    for shortcut in shortcuts(places, entry, &program) {
        shortcut.remove();
    }
    // The folders the manager made go, and the app's folder with them when nothing else is in it.
    // The folder can be one the user already had (`~/Documents/aznotes` chosen as the install
    // folder's place): what is not the manager's stays (X-DELTA-AZDRIVE-KEYS-35).
    for folder in made_folders(&paths) {
        if folder.exists() {
            std::fs::remove_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        }
    }
    match std::fs::remove_dir(&paths.install) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        // something of the user's is in it: the folder stays with it
        Err(_) if paths.install.is_dir() => {}
        Err(e) => return Err(format!("{}: {e}", paths.install.display())),
    }
    if paths.registry.exists() {
        std::fs::remove_dir_all(&paths.registry)
            .map_err(|e| format!("{}: {e}", paths.registry.display()))?;
    }
    for (file, bytes) in marks {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(&file, bytes).map_err(|e| format!("{}: {e}", file.display()))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Shortcuts
// ---------------------------------------------------------------------------

/// The marker of the line the manager adds to a shell's rc file.
pub const RC_MARKER: &str = "# Azlin apps (AzInstallManager)";

/// `text` as ONE word for a POSIX shell: in single quotes, where nothing is special, with a single
/// quote written as `'\''`.
#[must_use]
pub fn sh_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// `text` as one quoted argument of the `Exec` key of a `.desktop` file (Desktop Entry
/// Specification, "The Exec key"): in double quotes, with `"`, `` ` ``, `$` and `\` escaped by a
/// backslash; that backslash is itself doubled by the file's string escaping (so a literal
/// backslash is written with four), and a literal `%` is `%%` (a field code otherwise).
#[must_use]
pub fn desktop_exec_quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' | '`' | '$' => {
                out.push_str("\\\\");
                out.push(c);
            }
            '\\' => out.push_str("\\\\\\\\"),
            '%' => out.push_str("%%"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `text` as the inside of a PowerShell single-quoted string: every character PowerShell takes
/// for a single quote is doubled - the ASCII `'` and also U+2018, U+2019, U+201A and U+201B,
/// which its parser treats the same way (doubling only the ASCII one let a path end the string).
#[must_use]
pub fn ps_quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        out.push(c);
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}') {
            out.push(c);
        }
    }
    out
}

/// Whether `text` holds a control character (a new line, a NUL): no quoting carries one into a
/// line of an rc file or a key of a `.desktop` file.
fn has_control_character(text: &str) -> bool {
    text.chars().any(char::is_control)
}

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
    if entry.is_runtime() || has_control_character(&program.to_string_lossy()) {
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
            // The folder goes LAST in the PATH and in single quotes: first, a folder that an
            // environment variable moved (AZLIN_CONFIG=/tmp/x/config.json) would shadow the
            // system's commands, and unquoted, a `"` or a `$(...)` in it would run (REVIEW Y9).
            let bin_dir = places.bin_dir().to_string_lossy().into_owned();
            let line = format!("export PATH=\"$PATH\":{}", sh_quote(&bin_dir));
            let mut out = vec![Shortcut::Link {
                link: places.bin_dir().join(entry.key),
                target: program.to_path_buf(),
            }];
            // A folder with a new line in its name cannot be one line of an rc file.
            if !has_control_character(&bin_dir) {
                out.push(Shortcut::RcLine {
                    path: places.home.join(".zshrc"),
                    line: line.clone(),
                });
                let bash = places.home.join(".bash_profile");
                if bash.exists() {
                    out.push(Shortcut::RcLine { path: bash, line });
                }
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
        "[Desktop Entry]\nType=Application\nName={}\nExec={} %F\nTerminal=false\n\
         Categories={}\nX-Azlin-Key={}\n",
        entry.name,
        desktop_exec_quote(&program.to_string_lossy()),
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
                // The user's own file: its bytes are read as bytes (a Latin-1 comment, a pasted
                // escape is no reason to take it for empty), a file that is there and cannot be
                // read stops this with the file untouched, and the two lines are APPENDED - never a
                // write of the file anew, which is how an rc file became two lines
                // (X-DELTA-AZDRIVE-KEYS-12).
                let existing = match std::fs::read(path) {
                    Ok(bytes) => bytes,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
                    Err(e) => return Err(format!("{}: {e}", path.display())),
                };
                if existing
                    .split(|b| *b == b'\n')
                    .any(|l| l.trim_ascii() == line.as_bytes())
                {
                    return Ok(());
                }
                let mut add = Vec::new();
                if !existing.is_empty() && !existing.ends_with(b"\n") {
                    add.push(b'\n');
                }
                add.extend_from_slice(format!("{RC_MARKER}\n{line}\n").as_bytes());
                std::fs::OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(path)
                    .and_then(|mut file| std::io::Write::write_all(&mut file, &add))
                    .map_err(|e| format!("{}: {e}", path.display()))
            }
            Shortcut::StartMenu { path, target } => {
                mkdir(path)?;
                let quote = |p: &Path| ps_quote(&p.display().to_string());
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

    /// What an update has to prove here: this build's root key ([`anchors::update_policy`]).
    ///
    /// # Errors
    ///
    /// [`NoRootKey`]: this build cannot prove an update, so it installs and stages none.
    pub fn policy(&self) -> Result<UpdatePolicy, NoRootKey> {
        anchors::update_policy(&self.updates)
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
    // A build that cannot prove an update installs none: asked before any network (REVIEW Y1).
    let policy = fetching.policy().map_err(|e| e.to_string())?;
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
    let verified = engine.stage(entry.key, &release, &paths.staging(), &policy, &state)?;
    let placed = place(
        &fetching.places,
        entry,
        &verified,
        &release,
        channel,
        now_unix,
    );
    let _ = std::fs::remove_file(verified.path());
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
    let policy = fetching.policy().map_err(|e| e.to_string())?;
    let paths = fetching.places.app(entry.key);
    let state = paths.state(channel);
    // The staged file of this release is where the code keeps it; what staged.json says is not
    // read. It is checked again against the release in hand (a file on disk is not trusted
    // across runs); a file that fails is thrown away.
    let kept = paths.staged_file(&release.version).filter(|f| f.exists());
    let verified = match kept {
        Some(file) => match engine.verify(entry.key, &file, release, &policy, &state) {
            Ok(verified) => verified,
            Err(why) => {
                forget_staged(&paths, &release.version);
                return Err(why);
            }
        },
        None => engine.stage(entry.key, release, &paths.staging(), &policy, &state)?,
    };
    let placed = place(
        &fetching.places,
        entry,
        &verified,
        release,
        channel,
        now_unix,
    );
    let _ = std::fs::remove_file(verified.path());
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
            settle(&paths);
            // A copy installed under the app's former name is the app until it is updated.
            let registration: Registration = read_json(&paths.registration()).or_else(|| {
                let former = azul_appkit::azlin_config::former_app_key(entry.key)?;
                read_json(&places.app(former).registration())
            })?;
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
/// app's choices and the machine's cohort.
///
/// # Errors
///
/// In words: this build cannot prove an update, or no update source is configured.
pub fn round_for(fetching: &Fetching, app: &Installed) -> Result<Round, String> {
    let proof = fetching.policy().map_err(|e| e.to_string())?;
    let choices = fetching.updates.policy_for(app.entry.key);
    let manifest_url = fetching.manifest(app.entry, choices.channel)?;
    let kind = if app.is_managed() {
        InstallKind::UserWritable
    } else {
        InstallKind::ReadOnly
    };
    Ok(Round {
        paths: fetching.places.app(app.entry.key),
        current_version: app.registration.version.clone(),
        manifest_url,
        channel: choices.channel,
        cohort: fetching.cohort,
        audience: audience_for(kind, choices.channel),
        policy: proof,
        autoupdate: choices.autoupdate && app.is_managed(),
        skip_version: choices.skip_version,
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
        Ok(round) => status_of(background_round(engine, &round, now_unix)),
        Err(why) => Status::Failed(why),
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
    let mut round = round_for(fetching, app)?;
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

/// How old an "Update now" may be before it is void. The app writes it and quits; the manager
/// installs within seconds. An older one is a leftover - or something another process wrote.
pub const REQUEST_MAX_AGE_SECS: u64 = 24 * 3600;

/// Whether `request` is too old to carry out.
#[must_use]
pub fn request_is_stale(request: &ApplyRequest, now_unix: u64) -> bool {
    now_unix.saturating_sub(request.asked_unix) > REQUEST_MAX_AGE_SECS
}

/// Carries out an app's "Update now": installs the newest release the update source offers it
/// NOW, checked again under the build's policy. The request is a trigger and nothing more -
/// `request.json` and `staged.json` are files any process of the user can write, so neither
/// chooses the version or the file (REVIEW Y4, X-AR5b-26). A release staged for it is used if
/// it is where the code keeps it and passes the checks again; the engine's check only offers a
/// release newer than the installed one.
///
/// # Errors
///
/// In words: the app was not installed by the manager, the check failed or found nothing newer,
/// or the release could not be installed.
pub fn carry_out(
    engine: &dyn UpdateEngine,
    fetching: &Fetching,
    entry: &AppEntry,
    now_unix: u64,
) -> Result<Registration, String> {
    let app = installed(engine, &fetching.places, &fetching.updates)
        .into_iter()
        .find(|a| a.entry.key == entry.key && a.is_managed())
        .ok_or_else(|| format!("{} was not installed by AzInstallManager", entry.name))?;
    let release = release_now(engine, fetching, &app, now_unix)?;
    update_to(engine, fetching, entry, &release, app.channel, now_unix)
}

/// What an app's "Update now" came to: the install, and where the app starts again from.
pub struct RequestRun {
    pub result: Result<Registration, String>,
    /// The program to start again: the app asked for it (`relaunch`) and quit to be updated - it
    /// comes back whether the update worked or not (offline, refused by the signature check, already
    /// up to date), as the new program when there is one and else as the installed one
    /// (X-DELTA-AZDRIVE-KEYS-35: the request was spent before the attempt, and an attempt that failed
    /// left the app closed with nothing to say so).
    pub relaunch: Option<PathBuf>,
}

/// Carries out the waiting "Update now" `request` of `entry` ([`carry_out`]) and spends it AFTER the
/// attempt: a request is a trigger for one try, and what the try came to is reported with where the
/// app starts again from ([`RequestRun`]).
pub fn run_request(
    engine: &dyn UpdateEngine,
    fetching: &Fetching,
    entry: &AppEntry,
    request: &ApplyRequest,
    now_unix: u64,
) -> RequestRun {
    let paths = fetching.places.app(entry.key);
    let result = carry_out(engine, fetching, entry, now_unix);
    spend_request(&paths);
    let installed = match &result {
        Ok(registration) => Some(registration.binary.clone()),
        Err(_) => read_json::<Registration>(&paths.registration()).map(|r| r.binary),
    };
    RequestRun {
        relaunch: installed.filter(|_| request.relaunch).map(PathBuf::from),
        result,
    }
}

/// Removes an app's waiting "Update now" (a failure to is said: the request would be tried again).
pub fn spend_request(paths: &AppPaths) {
    if let Err(e) = std::fs::remove_file(paths.request()) {
        if e.kind() != std::io::ErrorKind::NotFound {
            eprintln!("[installmgr] {}: {e}", paths.request().display());
        }
    }
}

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
            Some(key) if entry(&key).is_some() => apply = entry(&key).map(|e| e.key.to_string()),
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

/// What this build proves an update with, for the words the screens show: nothing claims a
/// check that does not happen (REVIEW Y1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Proof {
    /// Azlin's update key: every release is checked against its signature.
    Signed,
    /// A test build that says it wants no signature: the checksum of a download is all.
    Unsigned,
    /// The build carries no key: updates are not available, nothing is fetched or installed.
    Unavailable,
}

impl Proof {
    /// The proof of the build for the config `updates` ([`anchors::update_policy`]).
    #[must_use]
    pub fn of(updates: &UpdatesSection) -> Proof {
        match anchors::update_policy(updates) {
            Ok(UpdatePolicy::Signed(_)) => Proof::Signed,
            Ok(UpdatePolicy::DigestOnly) => Proof::Unsigned,
            Err(_) => Proof::Unavailable,
        }
    }

    /// The settings screen's sentence about the signature check (a message key).
    #[must_use]
    pub const fn settings_key(self) -> &'static str {
        match self {
            Proof::Signed => "azinstallmgr-settings-signed",
            Proof::Unsigned => "azinstallmgr-settings-digest-only",
            Proof::Unavailable => "azinstallmgr-settings-unavailable",
        }
    }

    /// The welcome page's sentence about how apps are checked (a message key): "checked against
    /// its signature" only where that is true.
    #[must_use]
    pub const fn welcome_key(self) -> &'static str {
        match self {
            Proof::Signed => "azinstallmgr-welcome-signed",
            Proof::Unsigned => "azinstallmgr-welcome-unsigned",
            Proof::Unavailable => "azinstallmgr-welcome-unavailable",
        }
    }
}

/// Why nothing can be fetched with `updates` (a message key), if so: the build cannot prove an
/// update, or no update source is set. The build comes first - typing a source is no use then.
#[must_use]
pub fn fetch_blocker(updates: &UpdatesSection) -> Option<&'static str> {
    if Proof::of(updates) == Proof::Unavailable {
        Some("azinstallmgr-ready-no-key")
    } else if !has_source(updates) {
        Some("azinstallmgr-ready-no-source")
    } else {
        None
    }
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

    /// The words a screen shows follow what the build proves: "checked against its signature"
    /// only with Azlin's key, never for a build that has none.
    #[test]
    fn y1_what_the_screens_claim_follows_what_the_build_proves() {
        let none = UpdatesSection::default();
        assert_eq!(Proof::of(&none), Proof::Unavailable);
        assert_eq!(
            Proof::Unavailable.welcome_key(),
            "azinstallmgr-welcome-unavailable"
        );
        assert_eq!(
            Proof::Unavailable.settings_key(),
            "azinstallmgr-settings-unavailable"
        );
        assert_eq!(
            fetch_blocker(&none),
            Some("azinstallmgr-ready-no-key"),
            "the build first"
        );
        let (test, _) = AzlinConfig::parse(r#"{"updates": {"allow_unsigned": true}}"#);
        assert_eq!(Proof::of(&test.updates), Proof::Unsigned);
        assert_eq!(
            fetch_blocker(&test.updates),
            Some("azinstallmgr-ready-no-source")
        );
        let (typed, _) = AzlinConfig::parse(
            r#"{"updates": {"allow_unsigned": true, "manifest_url": "https://example.org/{app}.json"}}"#,
        );
        assert_eq!(fetch_blocker(&typed.updates), None);
        assert_ne!(Proof::Signed.welcome_key(), Proof::Unsigned.welcome_key());
        assert_eq!(Proof::Signed.settings_key(), "azinstallmgr-settings-signed");
    }

    /// An engine with one release per app key and a record of what it was asked.
    #[derive(Default)]
    struct FakeEngine {
        /// key -> (version, gated: only a tester gets it)
        releases: Vec<(&'static str, &'static str, bool)>,
        asked: Mutex<Vec<(String, Audience)>>,
        refuse_verify: bool,
        /// A file whose existence is written down at every check.
        watching: Option<PathBuf>,
        seen: Mutex<Vec<bool>>,
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
            if let Some(file) = &self.watching {
                self.seen.lock().unwrap().push(file.exists());
            }
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
        fn stage(
            &self,
            _: &str,
            r: &Release,
            dir: &Path,
            _: &UpdatePolicy,
            _: &Path,
        ) -> Result<Verified, String> {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            let file = dir.join(format!("{}.bin", r.version));
            std::fs::write(&file, format!("program {}", r.version)).map_err(|e| e.to_string())?;
            Verified::of(file)
        }
        fn verify(
            &self,
            _: &str,
            file: &Path,
            _: &Release,
            _: &UpdatePolicy,
            _: &Path,
        ) -> Result<Verified, String> {
            if self.refuse_verify {
                Err("artifact signature invalid".to_string())
            } else {
                Verified::of(file.to_path_buf())
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

    /// A manager with an update source and - a TEST build of the anchors (dev-dependency feature
    /// `test-hooks`) - a config that spells out `allow_unsigned`: the fake engine proves nothing.
    fn fetching(dir: &Path) -> Fetching {
        let (config, _) = AzlinConfig::parse(
            r#"{"updates": {"manifest_url": "http://example.org/{app}/{channel}.json",
                "allow_unsigned": true}}"#,
        );
        Fetching {
            places: places(dir, Os::Linux),
            updates: config.updates,
            cohort: 50,
        }
    }

    /// The same manager as a shipped build is: a source, and no root key anywhere.
    fn fetching_without_a_key(dir: &Path) -> Fetching {
        let mut f = fetching(dir);
        f.updates = UpdatesSection::default();
        f.updates.manifest_url = Some("http://example.org/{app}/{channel}.json".to_string());
        f
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

    /// A file with `text` in it that an engine has verified.
    fn file_with(dir: &Path, text: &str) -> Verified {
        let path = dir.join(format!("{text}.download"));
        std::fs::write(&path, text).unwrap();
        Verified::of(path).unwrap()
    }

    #[test]
    fn every_production_app_is_offered_once_with_its_runtime_and_a_group() {
        let names: Vec<&str> = APPS.iter().map(|a| a.name).collect();
        for name in [
            "AzDrive", "AzMail", "AzCalendar", "AzContacts", "AzTasks", "AzCode", "AzTerm",
            "AzWriter", "AzNotes", "AzSheets", "AzShow", "AzMeet", "AzMaps", "AzClock",
            "AzCalculator", "AzPhoto", "AzPaint", "AzReview", "AzMusic", "AzPlayer", "AzNews",
            "AzPasswordManager", "AzERP", "AzReader", "AzPdf", "AzMonitor", "AzVideoCut", "AzDashboard",
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

    /// X-DELTA-AZDRIVE-KEYS-36 (AZ-Y14): the managers that ask for root are the ones Setup never runs.
    #[test]
    fn x_delta_azdrive_keys_36_the_managers_that_need_root_are_exactly_the_pkexec_ones() {
        let all = [
            PackageManager::Homebrew,
            PackageManager::Apt,
            PackageManager::Dnf,
            PackageManager::Pacman,
            PackageManager::Flatpak,
            PackageManager::Winget,
        ];
        for pm in all {
            let escalates = pm
                .install_runs(&[writer()])
                .iter()
                .any(|run| run.first().map(String::as_str) == Some("pkexec"));
            assert_eq!(pm.runs_as_root(), escalates, "{}", pm.name());
        }
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
        // The config on disk spells what `fetching` holds, so reading it again keeps the policy.
        std::fs::create_dir_all(&f.places.azlin_dir).unwrap();
        std::fs::write(
            &f.places.config,
            r#"{"updates": {"manifest_url": "http://example.org/{app}/{channel}.json",
                "allow_unsigned": true}}"#,
        )
        .unwrap();
        let paths = f.places.app("azwriter");
        place(&f.places, writer(), &file_with(dir.path(), "v1"), &release("1.0.0"), Channel::Stable, 1).unwrap();
        AzlinConfig::edit(&f.places.config, |c| c.updates.set_skip_version("azwriter", Some("1.5.0"))).unwrap();
        let mut f = f;
        f.updates = AzlinConfig::load(&f.places.config).0.updates;
        std::fs::create_dir_all(paths.staging()).unwrap();
        let staged_file = paths.staged_file("2.0.0").unwrap();
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

    /// REVIEW Y1: a manager built without a root key authenticates no update. It installs and
    /// stages nothing, asks nobody for a release, and every row says why.
    #[test]
    fn y1_a_build_without_a_root_key_installs_and_updates_nothing() {
        let dir = TempDir::new("installmgr-no-key");
        let engine = FakeEngine {
            releases: vec![("azwriter", "2.0.0", false)],
            ..FakeEngine::default()
        };
        let f = fetching_without_a_key(dir.path());
        assert!(f.policy().is_err());
        let why = install_new(&engine, &f, writer(), Channel::Stable, 1).unwrap_err();
        assert!(why.starts_with(anchors::NOT_AVAILABLE), "{why}");
        assert!(
            !f.places.app("azwriter").current().exists(),
            "nothing is installed"
        );
        assert!(
            engine.asked.lock().unwrap().is_empty(),
            "no release was asked for"
        );

        let uses_none = update_to(&engine, &f, writer(), &release("2.0.0"), Channel::Stable, 2);
        assert!(uses_none.unwrap_err().starts_with(anchors::NOT_AVAILABLE));

        let ok = fetching(dir.path());
        place(
            &ok.places,
            writer(),
            &file_with(dir.path(), "v1"),
            &release("1.0.0"),
            Channel::Stable,
            3,
        )
        .unwrap();
        let apps = installed(&engine, &f.places, &f.updates);
        let round = round_for(&f, &apps[0]).unwrap_err();
        assert!(round.starts_with(anchors::NOT_AVAILABLE), "{round}");
        let Status::Failed(why) = check_installed(&engine, &f, &apps[0], 4) else {
            panic!("a row of a build with no key is not checked");
        };
        assert!(why.starts_with(anchors::NOT_AVAILABLE), "{why}");
        assert!(
            engine.asked.lock().unwrap().is_empty(),
            "still no release asked for"
        );
    }

    /// REVIEW Y6 / X-AR5b-26: placing takes the engine's proof, not a path, and installs the bytes
    /// that passed - a file swapped after the check is not installed and leaves the installed
    /// version as it was.
    #[test]
    fn y6_a_file_swapped_after_the_engine_checked_it_is_not_placed() {
        let dir = TempDir::new("installmgr-swap");
        let p = places(dir.path(), Os::Linux);
        let paths = p.app("azwriter");
        place(
            &p,
            writer(),
            &file_with(dir.path(), "v1"),
            &release("1.0.0"),
            Channel::Stable,
            1,
        )
        .unwrap();

        let checked = file_with(dir.path(), "v2");
        std::fs::write(checked.path(), "EVIL").unwrap();
        let refused = place(
            &p,
            writer(),
            &checked,
            &release("2.0.0"),
            Channel::Stable,
            2,
        )
        .unwrap_err();
        assert!(
            refused.contains("changed after it was verified"),
            "{refused}"
        );
        let reg: Registration = read_json(&paths.registration()).unwrap();
        assert_eq!(
            reg.version, "1.0.0",
            "the installed version is still registered"
        );
        assert_eq!(std::fs::read_to_string(&reg.binary).unwrap(), "v1");
        assert!(
            !paths.install.join("next").exists(),
            "no half-laid version stays"
        );
    }

    /// REVIEW Y4: staged.json names a file, and the manager used to install it and delete it. Any
    /// process of the user can write that record; the file a staged update is kept in is the
    /// one the code names.
    #[test]
    fn y4_a_staged_json_that_names_another_file_neither_installs_nor_deletes_it() {
        let dir = TempDir::new("installmgr-y4-staged");
        let f = fetching(dir.path());
        let paths = f.places.app("azwriter");
        place(
            &f.places,
            writer(),
            &file_with(dir.path(), "v1"),
            &release("1.0.0"),
            Channel::Stable,
            1,
        )
        .unwrap();
        let precious = dir.path().join("precious.txt");
        std::fs::write(&precious, "keep me").unwrap();
        write_json(
            &paths.staged(),
            &Staged {
                release: release("2.0.0"),
                file: precious.to_string_lossy().into_owned(),
                channel: "stable".into(),
                staged_unix: 5,
            },
        )
        .unwrap();
        let reg = update_to(
            &FakeEngine::default(),
            &f,
            writer(),
            &release("2.0.0"),
            Channel::Stable,
            6,
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&reg.binary).unwrap(),
            "program 2.0.0",
            "what the engine staged is installed, not the file the record names"
        );
        assert_eq!(
            std::fs::read_to_string(&precious).unwrap(),
            "keep me",
            "and it is not deleted"
        );
        assert!(!paths.staged().exists(), "the used-up record goes");
        // Reverting away from a version does not delete what a record names either.
        write_json(
            &paths.staged(),
            &Staged {
                release: release("2.0.0"),
                file: precious.to_string_lossy().into_owned(),
                ..Staged::default()
            },
        )
        .unwrap();
        revert(&f.places, writer(), 7).unwrap();
        assert!(precious.exists());
    }

    /// REVIEW Y4 / X-AR5b-26: an app's "Update now" is a trigger. The manager installs what a fresh
    /// check of the update source finds, whatever version request.json names and whatever
    /// staged.json says is staged.
    #[test]
    fn y4_update_now_installs_what_a_fresh_check_finds_and_names_no_version_or_file() {
        let dir = TempDir::new("installmgr-y4-request");
        let engine = FakeEngine {
            releases: vec![("azwriter", "2.0.0", false)],
            ..FakeEngine::default()
        };
        let f = fetching(dir.path());
        let paths = f.places.app("azwriter");
        place(
            &f.places,
            writer(),
            &file_with(dir.path(), "v1"),
            &release("1.0.0"),
            Channel::Stable,
            1,
        )
        .unwrap();
        let precious = dir.path().join("precious.txt");
        std::fs::write(&precious, "keep me").unwrap();
        azul_appkit::update::request_apply(
            &paths,
            &ApplyRequest {
                version: "9.9.9".into(),
                pid: 0,
                relaunch: false,
                asked_unix: 100,
            },
        )
        .unwrap();
        write_json(
            &paths.staged(),
            &Staged {
                release: release("9.9.9"),
                file: precious.to_string_lossy().into_owned(),
                ..Staged::default()
            },
        )
        .unwrap();
        let reg = carry_out(&engine, &f, writer(), 101).unwrap();
        assert_eq!(
            reg.version, "2.0.0",
            "the version the check found, not the one the request says"
        );
        assert_eq!(
            std::fs::read_to_string(&reg.binary).unwrap(),
            "program 2.0.0"
        );
        assert_eq!(std::fs::read_to_string(&precious).unwrap(), "keep me");

        // An app the manager did not install is not updated on its say-so.
        let other = fetching(dir.path());
        let none = carry_out(&engine, &other, entry("azclock").unwrap(), 102).unwrap_err();
        assert!(
            none.contains("was not installed by AzInstallManager"),
            "{none}"
        );
    }

    #[test]
    fn y4_an_update_now_that_is_a_day_old_is_void() {
        let asked = |asked_unix| ApplyRequest {
            version: "2.0.0".into(),
            pid: 0,
            relaunch: true,
            asked_unix,
        };
        assert!(
            !request_is_stale(&asked(1_000), 1_000 + 3_000),
            "the manager installs within seconds"
        );
        assert!(!request_is_stale(
            &asked(1_000),
            1_000 + REQUEST_MAX_AGE_SECS
        ));
        assert!(request_is_stale(
            &asked(1_000),
            1_000 + REQUEST_MAX_AGE_SECS + 1
        ));
        assert!(
            request_is_stale(&asked(0), 1_800_000_000),
            "a request without a time is no request"
        );
        assert!(
            !request_is_stale(&asked(2_000), 1_000),
            "a time in the future is not old"
        );
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

    // ---- REVIEW Y7: what an update, a revert and an uninstall keep --------------------------

    /// The files under `dir`, relative to it, sorted.
    fn files_under(dir: &Path) -> Vec<String> {
        fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, base, out);
                } else {
                    out.push(
                        path.strip_prefix(base)
                            .unwrap()
                            .to_string_lossy()
                            .into_owned(),
                    );
                }
            }
        }
        let mut out = Vec::new();
        walk(dir, dir, &mut out);
        out.sort();
        out
    }

    /// The text of the program in a version's folder.
    fn program_of(dir: &Path) -> String {
        std::fs::read_to_string(dir.join("AzWriter")).unwrap_or_default()
    }

    /// Uninstalling removed the registry folder with the rollback marks in it, so a reinstall
    /// began at key generation 0 and accepted a retired, unexpired signing key again.
    #[test]
    fn y7_uninstall_keeps_the_rollback_marks_and_removes_the_rest() {
        let dir = TempDir::new("installmgr-marks");
        let engine = FakeEngine {
            releases: vec![("azwriter", "2.0.0", false)],
            ..FakeEngine::default()
        };
        let f = fetching(dir.path());
        install_new(&engine, &f, writer(), Channel::Stable, 1).unwrap();
        let paths = f.places.app("azwriter");
        let marks = paths.state(Channel::Stable).join("update-state.json");
        std::fs::create_dir_all(marks.parent().unwrap()).unwrap();
        let text = r#"{"highest_seen":"2.0.0","key_generation":4}"#;
        std::fs::write(&marks, text).unwrap();
        std::fs::write(paths.state(Channel::Stable).join("cache.txt"), "x").unwrap();

        // The default layout: the programs and the registry are ONE folder.
        assert_eq!(paths.install, paths.registry);
        uninstall(&f.places, writer()).unwrap();
        assert!(!paths.current().exists(), "the program is gone");
        assert_eq!(
            files_under(&paths.registry),
            vec!["state/stable/update-state.json".to_string()],
            "the marks and nothing else of the registry"
        );
        assert_eq!(std::fs::read_to_string(&marks).unwrap(), text);
        assert!(
            installed(&FakeEngine::default(), &f.places, &f.updates).is_empty(),
            "a folder of marks is not an installed app"
        );
    }

    /// The version Revert brings back has not been through the engine since it was installed: it
    /// has to be the program whose hash was recorded then.
    #[test]
    fn y7_revert_brings_back_only_the_program_that_was_installed() {
        let dir = TempDir::new("installmgr-revert-hash");
        let p = places(dir.path(), Os::Linux);
        let paths = p.app("azwriter");
        place(
            &p,
            writer(),
            &file_with(dir.path(), "v1"),
            &release("1.0.0"),
            Channel::Stable,
            1,
        )
        .unwrap();
        place(
            &p,
            writer(),
            &file_with(dir.path(), "v2"),
            &release("2.0.0"),
            Channel::Stable,
            2,
        )
        .unwrap();
        let hash = std::fs::read_to_string(paths.prev().join(PROGRAM_HASH_FILE)).unwrap();
        assert_eq!(
            hash.len(),
            64,
            "the hash of the installed program is kept: {hash}"
        );

        // The kept version changed on disk.
        std::fs::write(paths.prev().join("AzWriter"), "EVIL").unwrap();
        let err = revert(&p, writer(), 3).unwrap_err();
        assert!(
            err.contains("not restored") && err.contains("not the one that was installed"),
            "{err}"
        );
        assert_eq!(
            program_of(&paths.current()),
            "v2",
            "the installed version stays"
        );
        assert_eq!(program_of(&paths.prev()), "EVIL", "and nothing is moved");

        // A version nobody recorded a hash for.
        std::fs::write(paths.prev().join("AzWriter"), "v1").unwrap();
        std::fs::remove_file(paths.prev().join(PROGRAM_HASH_FILE)).unwrap();
        let err = revert(&p, writer(), 4).unwrap_err();
        assert!(err.contains("no recorded hash"), "{err}");

        // The program that was installed comes back.
        std::fs::write(paths.prev().join(PROGRAM_HASH_FILE), hash).unwrap();
        let (from, reg) = revert(&p, writer(), 5).unwrap();
        assert_eq!((from.as_str(), reg.version.as_str()), ("2.0.0", "1.0.0"));
        assert_eq!(program_of(&paths.current()), "v1");
    }

    /// An update used to delete `prev/` BEFORE it moved the installed version: a refused move
    /// ("in use" on Windows) cost the Revert target, and a crash between the two renames left no
    /// `current/` at all. The moves are injected here so that the refusal happens on any host.
    #[test]
    fn y7_a_refused_move_leaves_the_installed_version_and_the_revert_target() {
        let dir = TempDir::new("installmgr-refused");
        let p = places(dir.path(), Os::Linux);
        let paths = p.app("azwriter");
        place(
            &p,
            writer(),
            &file_with(dir.path(), "v1"),
            &release("1.0.0"),
            Channel::Stable,
            1,
        )
        .unwrap();
        place(
            &p,
            writer(),
            &file_with(dir.path(), "v2"),
            &release("2.0.0"),
            Channel::Stable,
            2,
        )
        .unwrap();
        let next = paths.install.join("next");
        std::fs::create_dir_all(&next).unwrap();
        std::fs::write(next.join("AzWriter"), "v3").unwrap();

        // The installed version is in use: its move aside is refused.
        let err = activate(&paths, "AzWriter", |from, to| {
            if from == paths.current().as_path() {
                Err(std::io::Error::other("in use"))
            } else {
                std::fs::rename(from, to)
            }
        })
        .unwrap_err();
        assert!(err.contains("is in use"), "{err}");
        assert_eq!(program_of(&paths.current()), "v2");
        assert_eq!(
            program_of(&paths.prev()),
            "v1",
            "the Revert target survives a refused update"
        );

        // The new version cannot take its place: the installed one is put back.
        let err = activate(&paths, "AzWriter", |from, to| {
            if from == next.as_path() {
                Err(std::io::Error::other("denied"))
            } else {
                std::fs::rename(from, to)
            }
        })
        .unwrap_err();
        assert!(err.contains("denied"), "{err}");
        assert_eq!(program_of(&paths.current()), "v2");
        assert_eq!(program_of(&paths.prev()), "v1");
        assert!(!paths.install.join(REPLACED_DIR).exists());

        // Nothing refused: v3 is in, v2 is the previous version, v1 is gone.
        activate(&paths, "AzWriter", |from, to| std::fs::rename(from, to)).unwrap();
        assert_eq!(program_of(&paths.current()), "v3");
        assert_eq!(program_of(&paths.prev()), "v2");
        assert!(!paths.install.join(REPLACED_DIR).exists());
    }

    #[test]
    fn y7_a_crash_between_the_two_moves_of_an_update_leaves_a_program_again() {
        let dir = TempDir::new("installmgr-crash-a");
        let f = fetching(dir.path());
        let paths = f.places.app("azwriter");
        place(
            &f.places,
            writer(),
            &file_with(dir.path(), "v1"),
            &release("1.0.0"),
            Channel::Stable,
            1,
        )
        .unwrap();
        place(
            &f.places,
            writer(),
            &file_with(dir.path(), "v2"),
            &release("2.0.0"),
            Channel::Stable,
            2,
        )
        .unwrap();
        // The crash: the installed version had moved aside and the new one was not in yet.
        std::fs::rename(paths.current(), paths.install.join(REPLACED_DIR)).unwrap();
        std::fs::create_dir_all(paths.install.join("next")).unwrap();
        assert!(!paths.current().exists());

        // Listing the installed apps heals it.
        let apps = installed(&FakeEngine::default(), &f.places, &f.updates);
        assert_eq!(apps.len(), 1);
        assert_eq!(program_of(&paths.current()), "v2");
        assert!(!paths.install.join(REPLACED_DIR).exists());
        assert_eq!(
            program_of(&paths.prev()),
            "v1",
            "the Revert target is untouched"
        );
    }

    #[test]
    fn y7_a_crash_after_the_new_version_is_in_is_finished() {
        let dir = TempDir::new("installmgr-crash-b");
        let p = places(dir.path(), Os::Linux);
        let paths = p.app("azwriter");
        place(
            &p,
            writer(),
            &file_with(dir.path(), "v1"),
            &release("1.0.0"),
            Channel::Stable,
            1,
        )
        .unwrap();
        place(
            &p,
            writer(),
            &file_with(dir.path(), "v2"),
            &release("2.0.0"),
            Channel::Stable,
            2,
        )
        .unwrap();
        // The crash: v2 moved aside and v3 moved in, the old previous version not yet gone.
        std::fs::rename(paths.current(), paths.install.join(REPLACED_DIR)).unwrap();
        std::fs::create_dir_all(paths.current()).unwrap();
        std::fs::write(paths.current().join("AzWriter"), "v3").unwrap();

        settle(&paths);
        assert_eq!(program_of(&paths.current()), "v3");
        assert_eq!(program_of(&paths.prev()), "v2");
        assert!(!paths.install.join(REPLACED_DIR).exists());
        // A stray FILE of that name is no version and is not taken for one.
        std::fs::write(paths.install.join(REPLACED_DIR), "junk").unwrap();
        settle(&paths);
        assert!(!paths.install.join(REPLACED_DIR).exists());
        assert_eq!(program_of(&paths.prev()), "v2");
    }

    // ---- REVIEW Y9: what a launcher line carries --------------------------------------------

    #[test]
    fn y9_sh_quote_is_one_word() {
        assert_eq!(sh_quote("/a b/c"), "'/a b/c'");
        assert_eq!(sh_quote("it's"), "'it'\\''s'");
        assert_eq!(sh_quote("$(x)\"`"), "'$(x)\"`'");
    }

    /// The PATH line was `export PATH="<folder>:$PATH"`: a `"` ended the string and a `$(...)` in
    /// the folder's name ran when the shell started, and the folder was FIRST in the PATH. The
    /// line is run by `sh` here.
    #[cfg(unix)]
    #[test]
    fn y9_the_path_line_runs_nothing_from_the_folder_name_and_puts_it_last() {
        let dir = TempDir::new("installmgr-pathline");
        let marker = dir.path().join("ran");
        let mut p = places(dir.path(), Os::MacOs);
        p.azlin_dir = dir
            .path()
            .join(format!("a b\"c$(touch {})'d", marker.display()));
        let made = shortcuts(&p, writer(), Path::new("/x/AzWriter"));
        let line = made
            .iter()
            .find_map(|s| match s {
                Shortcut::RcLine { line, .. } => Some(line.clone()),
                _ => None,
            })
            .expect("an rc line");
        assert!(line.starts_with("export PATH=\"$PATH\":'"), "{line}");
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("PATH=/usr/bin:/bin; {line}; printf '%s' \"$PATH\""))
            .output()
            .expect("sh runs");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            format!("/usr/bin:/bin:{}", p.bin_dir().display()),
            "the folder is one word, appended last"
        );
        assert!(
            !marker.exists(),
            "the command in the folder's name did not run"
        );
    }

    #[test]
    fn y9_a_name_with_a_new_line_gets_no_line_in_an_rc_file_and_no_desktop_file() {
        let dir = TempDir::new("installmgr-newline");
        let mut p = places(dir.path(), Os::MacOs);
        p.azlin_dir = dir.path().join("a\nb");
        let made = shortcuts(&p, writer(), Path::new("/x/AzWriter"));
        assert!(
            made.iter().all(|s| !matches!(s, Shortcut::RcLine { .. })),
            "{made:?}"
        );
        assert!(made.iter().any(|s| matches!(s, Shortcut::Link { .. })));
        let linux = places(dir.path(), Os::Linux);
        assert!(shortcuts(&linux, writer(), Path::new("/x/Az\nWriter")).is_empty());
    }

    /// The `Exec` value as the Desktop Entry Specification reads it back: the file's own string
    /// escapes first, then one double-quoted argument.
    fn exec_argument(file_text: &str) -> String {
        let mut unescaped = String::new();
        let mut chars = file_text.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                unescaped.push(c);
                continue;
            }
            match chars.next() {
                Some('\\') => unescaped.push('\\'),
                Some(other) => {
                    unescaped.push('\\');
                    unescaped.push(other);
                }
                None => unescaped.push('\\'),
            }
        }
        let inner = unescaped
            .strip_prefix('"')
            .and_then(|t| t.strip_suffix('"'))
            .expect("one double-quoted argument");
        let mut out = String::new();
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => out.push(chars.next().expect("a character after the backslash")),
                '%' => {
                    assert_eq!(chars.next(), Some('%'), "a lone % is a field code");
                    out.push('%');
                }
                '"' | '`' | '$' => panic!("{c} unescaped inside the quotes"),
                c => out.push(c),
            }
        }
        out
    }

    #[test]
    fn y9_the_desktop_exec_reads_back_as_the_path() {
        for path in [
            "/home/ann/.azlin/apps/azwriter/current/AzWriter",
            "/home/a b/Az\"W$x`y\\z%f",
            "/tmp/$(rm -rf ~)/x",
            "/p/100%/q",
            "/p/\\\\/q",
            "/p/'single'/q",
        ] {
            assert_eq!(exec_argument(&desktop_exec_quote(path)), path, "{path}");
        }
        // The specification's own rule: a literal backslash takes four.
        assert_eq!(desktop_exec_quote("a\\b"), "\"a\\\\\\\\b\"");
        assert_eq!(desktop_exec_quote("a$b"), "\"a\\\\$b\"");
        let text = desktop_file(writer(), Path::new("/home/a b/AzWriter"));
        assert!(
            text.contains("\nExec=\"/home/a b/AzWriter\" %F\n"),
            "{text}"
        );
    }

    #[test]
    fn y9_powershell_quotes_double_every_quote_it_knows() {
        assert_eq!(ps_quote("C:\\a b\\x"), "C:\\a b\\x");
        assert_eq!(ps_quote("it's"), "it''s");
        assert_eq!(
            ps_quote("a\u{2018}b\u{2019}c\u{201A}d\u{201B}e"),
            "a\u{2018}\u{2018}b\u{2019}\u{2019}c\u{201A}\u{201A}d\u{201B}\u{201B}e"
        );
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

    /// AzKeys became AzPasswordManager: its former key, its registration folder and an "Update
    /// now" of the old program all find the app under its new name.
    #[test]
    fn an_app_installed_under_its_former_name_is_the_app_under_its_new_one() {
        assert_eq!(entry("azkeys").map(|e| e.key), Some("azpasswordmanager"));
        let (apply, _) = split_switches(["--apply", "azkeys"]).unwrap();
        assert_eq!(apply.as_deref(), Some("azpasswordmanager"));
        let dir = TempDir::new("installmgr-renamed");
        let f = fetching(dir.path());
        write_json(
            &f.places.app("azkeys").registration(),
            &Registration {
                name: "AzKeys".into(),
                key: "azkeys".into(),
                version: "1.0.0".into(),
                binary: "/opt/azlin/AzKeys".into(),
                ..Registration::default()
            },
        )
        .unwrap();
        let apps = installed(&FakeEngine::default(), &f.places, &f.updates);
        assert_eq!(apps.iter().map(|a| a.entry.key).collect::<Vec<_>>(), vec!["azpasswordmanager"]);
        assert_eq!(apps[0].registration.version, "1.0.0");
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

    /// X-DELTA-AZDRIVE-KEYS-11: the marks stayed only for a CUSTOM install folder. In the default
    /// layout the install folder is the registry folder, `remove_dir_all(install)` took the marks
    /// with it and `registry.exists()` was false afterwards, so a reinstall began at generation 0
    /// and accepted a retired, unexpired signing key again. The marks are read before the first
    /// removal; the other layout keeps them too.
    #[test]
    fn x_delta_azdrive_keys_11_the_marks_survive_an_uninstall_in_the_default_layout_and_in_a_custom_one(
    ) {
        for custom in [false, true] {
            let dir = TempDir::new("installmgr-marks-layouts");
            let engine = FakeEngine {
                releases: vec![("azwriter", "2.0.0", false)],
                ..FakeEngine::default()
            };
            let mut f = fetching(dir.path());
            if custom {
                f.places.install_dir = dir.path().join("programs");
            }
            install_new(&engine, &f, writer(), Channel::Stable, 1).unwrap();
            let paths = f.places.app("azwriter");
            assert_eq!(paths.install == paths.registry, !custom, "custom: {custom}");
            let marks = paths.state(Channel::Stable).join("update-state.json");
            std::fs::create_dir_all(marks.parent().unwrap()).unwrap();
            let text = r#"{"highest_seen":"2.0.0","key_generation":4}"#;
            std::fs::write(&marks, text).unwrap();

            uninstall(&f.places, writer()).unwrap();
            assert_eq!(
                std::fs::read_to_string(&marks).unwrap(),
                text,
                "custom: {custom}: the marks are what the engine wrote"
            );
            assert!(
                !paths.current().exists() && !paths.prev().exists(),
                "custom: {custom}"
            );
            assert!(!paths.registration().exists(), "custom: {custom}");
            if custom {
                assert!(
                    !paths.install.exists(),
                    "the custom install folder is removed"
                );
            }
            assert_eq!(
                files_under(&paths.registry),
                vec!["state/stable/update-state.json".to_string()],
                "custom: {custom}"
            );
        }
    }

    /// X-DELTA-AZDRIVE-KEYS-35 (cebf4282d): "Update now" was spent before it was tried, and needed the
    /// network; the app had quit, and a try that failed left it closed with no request to try again.
    /// The request is spent AFTER the try, and the app that asked to start again starts again - as
    /// the new program when the update worked, as the installed one when it did not.
    #[test]
    fn x_delta_azdrive_keys_35_an_update_now_is_spent_after_the_try_and_the_app_comes_back_either_way(
    ) {
        let dir = TempDir::new("installmgr-update-now");
        let f = fetching(dir.path());
        let paths = f.places.app("azwriter");
        place(
            &f.places,
            writer(),
            &file_with(dir.path(), "v2"),
            &release("2.0.0"),
            Channel::Stable,
            1,
        )
        .unwrap();
        let asked = ApplyRequest {
            version: String::from("3.0.0"),
            pid: 0,
            relaunch: true,
            asked_unix: 5,
        };

        // Offline: the update source answers nothing. The try fails, the request is still there
        // while it runs and gone after it, and the installed program is started again.
        write_json(&paths.request(), &asked).unwrap();
        let offline = FakeEngine {
            watching: Some(paths.request()),
            ..FakeEngine::default()
        };
        let run = run_request(&offline, &f, writer(), &asked, 10);
        assert!(run.result.is_err(), "{:?}", run.result);
        assert_eq!(
            run.relaunch.as_deref(),
            Some(paths.current().join("AzWriter").as_path()),
            "the app that quit for the update comes back as it was installed"
        );
        assert!(!paths.request().exists(), "spent after the try");
        assert_eq!(
            *offline.seen.lock().unwrap(),
            vec![true],
            "and not before it"
        );

        // An app that did not ask to start again is not started.
        write_json(
            &paths.request(),
            &ApplyRequest {
                relaunch: false,
                ..asked.clone()
            },
        )
        .unwrap();
        let quiet = ApplyRequest {
            relaunch: false,
            ..asked.clone()
        };
        assert_eq!(
            run_request(&offline, &f, writer(), &quiet, 11).relaunch,
            None
        );
        assert!(!paths.request().exists());

        // The update works: the new program starts.
        write_json(&paths.request(), &asked).unwrap();
        let online = FakeEngine {
            releases: vec![("azwriter", "3.0.0", false)],
            ..FakeEngine::default()
        };
        let run = run_request(&online, &f, writer(), &asked, 12);
        let registration = run.result.expect("the update went through");
        assert_eq!(registration.version, "3.0.0");
        assert_eq!(
            run.relaunch.as_deref(),
            Some(Path::new(&registration.binary))
        );
        assert!(!paths.request().exists());
    }

    /// X-DELTA-AZDRIVE-KEYS-35 (settle): the replaced version that could not become `prev/` was
    /// deleted - the Revert target lost without a word. It stays, and the next settle tries again.
    #[cfg(unix)]
    #[test]
    fn x_delta_azdrive_keys_35_a_replaced_version_that_cannot_become_the_previous_one_stays() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new("installmgr-settle-keeps");
        let p = places(dir.path(), Os::Linux);
        let paths = p.app("azwriter");
        std::fs::create_dir_all(paths.current()).unwrap();
        std::fs::write(paths.current().join("AzWriter"), "v2").unwrap();
        let replaced = paths.install.join(REPLACED_DIR);
        std::fs::create_dir_all(&replaced).unwrap();
        std::fs::write(replaced.join("AzWriter"), "v1").unwrap();
        // An old `prev/` that cannot be removed whole (a file in it held open, as on Windows): a
        // folder whose own folder takes no write leaves it half there, and the move over it fails.
        let locked = paths.prev().join("old");
        std::fs::create_dir_all(&locked).unwrap();
        std::fs::write(locked.join("AzWriter"), "v0").unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500)).unwrap();

        settle(&paths);
        let kept = std::fs::read_to_string(replaced.join("AzWriter"));
        // let the sandbox go before anything is asserted
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            kept.as_deref().ok(),
            Some("v1"),
            "the replaced version stays: {kept:?}"
        );

        // Once the old `prev/` can go, the next settle makes the replaced version the previous one.
        settle(&paths);
        assert!(!replaced.exists());
        assert_eq!(
            std::fs::read_to_string(paths.prev().join("AzWriter")).unwrap(),
            "v1"
        );
    }

    /// X-DELTA-AZDRIVE-KEYS-35 (uninstall): `<install_dir>/<key>` was removed whole, and the folder
    /// can be one the user had (the install folder chosen as `~/Documents`, with their `aznotes`
    /// in it). The manager removes the folders it made; its own folder goes when nothing else is
    /// in it.
    #[test]
    fn x_delta_azdrive_keys_35_an_uninstall_leaves_a_users_own_files_in_the_install_folder() {
        let dir = TempDir::new("installmgr-uninstall-own");
        let mut p = places(dir.path(), Os::Linux);
        p.install_dir = dir.path().join("Documents");
        let paths = p.app("azwriter");
        std::fs::create_dir_all(&paths.install).unwrap();
        std::fs::write(paths.install.join("my-notes.txt"), "mine").unwrap();
        place(
            &p,
            writer(),
            &file_with(dir.path(), "v1"),
            &release("1.0.0"),
            Channel::Stable,
            1,
        )
        .unwrap();
        place(
            &p,
            writer(),
            &file_with(dir.path(), "v2"),
            &release("2.0.0"),
            Channel::Stable,
            2,
        )
        .unwrap();
        assert!(paths.prev().exists());

        uninstall(&p, writer()).unwrap();
        assert!(
            !paths.current().exists() && !paths.prev().exists(),
            "what the manager made goes"
        );
        assert_eq!(
            std::fs::read_to_string(paths.install.join("my-notes.txt")).unwrap(),
            "mine",
            "the user's own file stays, and so does its folder"
        );

        // With nothing of the user's in it the folder goes too.
        let again = places(dir.path(), Os::Linux);
        let own = again.app("azclock");
        place(
            &again,
            entry("azclock").unwrap(),
            &file_with(dir.path(), "c1"),
            &release("1.0.0"),
            Channel::Stable,
            1,
        )
        .unwrap();
        uninstall(&again, entry("azclock").unwrap()).unwrap();
        assert!(!own.install.join("current").exists());
    }

    /// X-DELTA-AZDRIVE-KEYS-11: marks that cannot be read are not "no marks": the uninstall stops
    /// before it removes a file (the old code skipped what it could not read and removed the
    /// registry).
    #[cfg(unix)]
    #[test]
    fn x_delta_azdrive_keys_11_marks_that_cannot_be_read_stop_the_uninstall_before_it_removes_anything(
    ) {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new("installmgr-marks-unreadable");
        let engine = FakeEngine {
            releases: vec![("azwriter", "2.0.0", false)],
            ..FakeEngine::default()
        };
        let f = fetching(dir.path());
        install_new(&engine, &f, writer(), Channel::Stable, 1).unwrap();
        let paths = f.places.app("azwriter");
        let marks = paths.state(Channel::Stable).join("update-state.json");
        std::fs::create_dir_all(marks.parent().unwrap()).unwrap();
        std::fs::write(&marks, r#"{"key_generation":4}"#).unwrap();
        std::fs::set_permissions(&marks, std::fs::Permissions::from_mode(0o000)).unwrap();
        let readable = std::fs::read(&marks).is_ok();
        let outcome = uninstall(&f.places, writer());
        std::fs::set_permissions(&marks, std::fs::Permissions::from_mode(0o600)).unwrap();
        if readable {
            return; // a user the permission bits do not bind (root): nothing to show
        }
        assert!(outcome.is_err(), "{outcome:?}");
        assert!(paths.current().exists(), "nothing was removed");
        assert!(paths.registration().exists());
    }

    /// X-DELTA-AZDRIVE-KEYS-35: the marks were read whole into memory; a sparse file of any size in
    /// their place ran the uninstall out of it. Marks bigger than a state file may be are an error
    /// that stops the uninstall before it removes anything - they are not dropped.
    #[test]
    fn x_delta_azdrive_keys_35_marks_over_the_limit_stop_the_uninstall_before_it_removes_anything()
    {
        let dir = TempDir::new("installmgr-marks-huge");
        let engine = FakeEngine {
            releases: vec![("azwriter", "2.0.0", false)],
            ..FakeEngine::default()
        };
        let f = fetching(dir.path());
        install_new(&engine, &f, writer(), Channel::Stable, 1).unwrap();
        let paths = f.places.app("azwriter");
        let marks = paths.state(Channel::Stable).join("update-state.json");
        std::fs::create_dir_all(marks.parent().unwrap()).unwrap();
        std::fs::File::create(&marks)
            .unwrap()
            .set_len(STATE_FILE_MAX + 1)
            .unwrap();
        let outcome = uninstall(&f.places, writer());
        let why = outcome.expect_err("marks that big are not read");
        assert!(why.contains("bigger than"), "{why}");
        assert!(
            paths.current().exists() && paths.registration().exists(),
            "nothing was removed"
        );
        // marks of the limit's size read as they are
        std::fs::write(&marks, vec![b' '; 64]).unwrap();
        assert_eq!(read_bounded(&marks, 64).unwrap().len(), 64);
        assert!(read_bounded(&marks, 63).is_err());
    }

    /// X-DELTA-AZDRIVE-KEYS-12: a shell rc file with one byte that is not UTF-8 (a Latin-1
    /// comment, a pasted escape) was read as EMPTY and written back as the marker and the PATH
    /// line: the user's `.zshrc` replaced by two lines. The bytes stay as they were and the two
    /// lines are appended, once.
    #[test]
    fn x_delta_azdrive_keys_12_an_rc_file_that_is_not_utf8_keeps_every_byte_and_gets_the_line_once()
    {
        let dir = TempDir::new("installmgr-rc-latin1");
        let p = places(dir.path(), Os::MacOs);
        let rc = dir.path().join(".zshrc");
        let original: &[u8] = b"# caf\xe9 au lait\nexport EDITOR=vim\nalias ll='ls -l'";
        std::fs::write(&rc, original).unwrap();
        let line = Shortcut::RcLine {
            path: rc.clone(),
            line: format!(
                "export PATH=\"$PATH\":{}",
                sh_quote(&p.bin_dir().to_string_lossy())
            ),
        };
        line.make().unwrap();
        let after = std::fs::read(&rc).unwrap();
        assert!(
            after.starts_with(original),
            "every byte of the user's file stays: {after:?}"
        );
        let added = &after[original.len()..];
        let added = std::str::from_utf8(added).unwrap();
        assert!(
            added.starts_with('\n'),
            "the last line had no end: {added:?}"
        );
        assert_eq!(added.matches(RC_MARKER).count(), 1);
        assert!(added.contains("export PATH="));
        // Again: nothing more.
        line.make().unwrap();
        assert_eq!(std::fs::read(&rc).unwrap(), after);
        // A file that is not there is made.
        let fresh = dir.path().join(".bash_profile");
        Shortcut::RcLine {
            path: fresh.clone(),
            line: String::from("export A=1"),
        }
        .make()
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&fresh).unwrap(),
            format!("{RC_MARKER}\nexport A=1\n")
        );
    }

    /// X-DELTA-AZDRIVE-KEYS-12: a file that is there and cannot be read is not an empty one: the
    /// shortcut fails and the file is as it was (write-only, the old code wrote over it).
    #[cfg(unix)]
    #[test]
    fn x_delta_azdrive_keys_12_an_rc_file_that_cannot_be_read_is_left_alone() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new("installmgr-rc-unreadable");
        let rc = dir.path().join(".zshrc");
        std::fs::write(&rc, "export KEEP=1\n").unwrap();
        std::fs::set_permissions(&rc, std::fs::Permissions::from_mode(0o200)).unwrap();
        let readable = std::fs::read(&rc).is_ok();
        let outcome = Shortcut::RcLine {
            path: rc.clone(),
            line: String::from("export A=1"),
        }
        .make();
        std::fs::set_permissions(&rc, std::fs::Permissions::from_mode(0o600)).unwrap();
        if readable {
            return; // root
        }
        assert!(outcome.is_err(), "{outcome:?}");
        assert_eq!(std::fs::read_to_string(&rc).unwrap(), "export KEEP=1\n");
    }
}
