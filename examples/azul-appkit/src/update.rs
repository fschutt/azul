//! How an Azlin app and AzInstallManager talk about updates.
//!
//! The manager installs an app into `<install_dir>/<key>/current/` and keeps the version before
//! it in `prev/` (one, for Revert). What the two share lives in the app's registry folder
//! `<azlin>/apps/<key>/` (`<azlin>` is the folder of the shared config, `~/.azlin`):
//!
//! - `current.json` ([`Registration`]): which version of the app is where. The manager writes it
//!   when it installs; the app writes it when it starts ([`register`]), which is how the manager
//!   learns about a copy it did not install (a package manager's, a developer's build).
//! - `staged.json` ([`Staged`]): an update fetched and verified in the background, waiting for
//!   the user's yes. Staging is not installing.
//! - `request.json` ([`ApplyRequest`]): the app's "Update now". The app never replaces its own
//!   binary: it writes the request, starts the manager and quits; the manager installs the
//!   staged version once the app's process is gone and starts the app again.
//! - `state/<channel>/`: the update engine's bookkeeping for the app (the highest version seen,
//!   the accepted signing-key generation). One per channel: leaving the beta must not make the
//!   stable release look like a downgrade. `staging/`: the downloads.
//!
//! The policy is the shared config's `updates` section ([`UpdatesSection`]); the machine's
//! rollout cohort is drawn once and kept there. The engine's calls (check a manifest, stage,
//! verify) go through [`UpdateEngine`]: azul's updater ([`AzulEngine`], feature `azul`), a fake
//! in the tests. [`background_round`] is one app's check-and-stage, the same in the app and in
//! the manager.
//!
//! Inside an app, `app` (feature `azul`) is the kit: the registration at the start, the periodic
//! check, the banner over the window and its "Update now".
//!
//! No azul types outside [`AzulEngine`] and `app`: tested without a window.

use std::path::{Path, PathBuf};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::azlin_config::{AzlinConfig, Channel, UpdatesSection};

/// The update kit inside an app: registration, the periodic check, the banner, "Update now".
#[cfg(feature = "azul")]
pub mod app;

/// The folder of the registry folders, in the shared config's folder.
pub const APPS_DIR: &str = "apps";
/// The registration of an app, in its registry folder.
pub const REGISTRATION_FILE: &str = "current.json";
/// A verified update waiting for the user's yes.
pub const STAGED_FILE: &str = "staged.json";
/// The app's "Update now", for the manager.
pub const REQUEST_FILE: &str = "request.json";
/// The release an installed version came from, beside its binary (`current/`, `prev/`).
pub const RELEASE_FILE: &str = "release.json";
/// The registry key of AzInstallManager itself: an app finds the manager's program by its
/// registration to hand it an "Update now".
pub const MANAGER_KEY: &str = "installmgr";

/// The folders of one app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppPaths {
    /// `<azlin>/apps/<key>`: the registration, the staged update, the request, the engine's state.
    pub registry: PathBuf,
    /// `<install_dir>/<key>`: `current/` and `prev/`.
    pub install: PathBuf,
}

impl AppPaths {
    /// The folders of the app `key`, under the shared config's folder `azlin_dir` and the
    /// install folder `install_dir`.
    #[must_use]
    pub fn new(azlin_dir: &Path, install_dir: &Path, key: &str) -> AppPaths {
        AppPaths {
            registry: azlin_dir.join(APPS_DIR).join(key),
            install: install_dir.join(key),
        }
    }

    /// `current.json`.
    #[must_use]
    pub fn registration(&self) -> PathBuf {
        self.registry.join(REGISTRATION_FILE)
    }

    /// `staged.json`.
    #[must_use]
    pub fn staged(&self) -> PathBuf {
        self.registry.join(STAGED_FILE)
    }

    /// `request.json`.
    #[must_use]
    pub fn request(&self) -> PathBuf {
        self.registry.join(REQUEST_FILE)
    }

    /// The engine's state for `channel`.
    #[must_use]
    pub fn state(&self, channel: Channel) -> PathBuf {
        self.registry.join("state").join(channel.name())
    }

    /// Where updates are downloaded to.
    #[must_use]
    pub fn staging(&self) -> PathBuf {
        self.registry.join("staging")
    }

    /// The installed version.
    #[must_use]
    pub fn current(&self) -> PathBuf {
        self.install.join("current")
    }

    /// The version before it, for Revert.
    #[must_use]
    pub fn prev(&self) -> PathBuf {
        self.install.join("prev")
    }
}

/// The install folder: the config's `install_dir`, else `<azlin>/apps` (the registry folders and
/// the programs side by side, writable without sudo).
#[must_use]
pub fn install_dir(azlin_dir: &Path, updates: &UpdatesSection) -> PathBuf {
    match updates.install_dir.as_deref().map(str::trim) {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => azlin_dir.join(APPS_DIR),
    }
}

/// The manifest URL of the app `key` on `channel`: the config's template with `{app}` and
/// `{channel}` filled in. `None` when no source is configured.
#[must_use]
pub fn manifest_url_for(updates: &UpdatesSection, key: &str, channel: Channel) -> Option<String> {
    let template = updates.manifest_url.as_deref().map(str::trim)?;
    if template.is_empty() {
        return None;
    }
    Some(
        template
            .replace("{app}", key)
            .replace("{channel}", channel.name()),
    )
}

/// The seconds since 1970.
#[must_use]
pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

// ---------------------------------------------------------------------------
// The shared files
// ---------------------------------------------------------------------------

/// One release, as the engine's `ReleaseInfo` describes it, in a form that can be kept in a
/// file: the staged update and the installed versions keep theirs, so a file is checked against
/// its own digest and signatures right before it is installed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Release {
    /// The version, as the manifest says it.
    pub version: String,
    /// Where the program of this platform is.
    pub download_url: String,
    /// The changelog's address, and the changelog itself when the source carried it.
    pub changelog_md_url: String,
    pub changelog_md_inline: String,
    /// The `sha256:` pin of the download (empty: none).
    pub digest: String,
    /// The signature chain: the artifact's minisign signature, the root-signed statement that
    /// names the signing key, and the statement's signature. Empty when unsigned.
    pub signature: String,
    pub signing_key_statement: String,
    pub signing_key_statement_sig: String,
}

/// `current.json`: which version of an app is where.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Registration {
    /// The app's name (`AzWriter`).
    pub name: String,
    /// The app's key (`azwriter`): the registry folder, the config's override, the manifest's
    /// `{app}`.
    pub key: String,
    /// The version of `binary`.
    pub version: String,
    /// The absolute path of the program.
    pub binary: String,
    /// The channel it was installed from (`stable`, `beta`); empty when the app registered itself
    /// and nobody chose one.
    pub channel: String,
    /// The bytes of the installed version (the manager measures them; 0 from the app).
    pub size_bytes: u64,
    /// When this version was installed or first seen, seconds since 1970.
    pub updated_unix: u64,
    /// When the app last started (the app writes it).
    pub launched_unix: u64,
    /// The manager installed `binary`: it may replace, revert and remove it. `false` for a copy
    /// it found (a package manager owns it, or a developer runs a build).
    pub managed: bool,
}

/// `staged.json`: an update fetched and verified in the background.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Staged {
    /// The release it is.
    pub release: Release,
    /// The downloaded file.
    pub file: String,
    /// The channel it came from.
    pub channel: String,
    /// When it was staged.
    pub staged_unix: u64,
}

/// `request.json`: the app's "Update now".
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ApplyRequest {
    /// The version the user said yes to (the staged one).
    pub version: String,
    /// The app's process: the manager installs once it is gone (0: no process to wait for).
    pub pid: u32,
    /// Start the app again afterwards.
    pub relaunch: bool,
    /// When the user asked.
    pub asked_unix: u64,
}

/// Reads the JSON file `path` (`None`: missing or not readable as a `T`).
#[must_use]
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Writes `value` to `path` as JSON, creating the folder: through a temporary file and a
/// rename, so a reader never sees half a file.
///
/// # Errors
///
/// The I/O error that stopped the write.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string_pretty(value).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

/// Whether `a` and `b` name the same program: the same path, or the same file once links are
/// followed (a temporary folder on macOS is `/var/...` to one and `/private/var/...` to
/// `current_exe`).
#[must_use]
pub fn same_program(a: &Path, b: &Path) -> bool {
    a == b
        || match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
}

/// What a started app writes into `current.json` ([`register`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Starting<'a> {
    /// The app's name, key and running version (its About facts).
    pub name: &'a str,
    pub key: &'a str,
    pub version: &'a str,
    /// The running program (`std::env::current_exe`).
    pub binary: &'a Path,
    /// The start's time, seconds since 1970.
    pub now_unix: u64,
}

/// Registers a started app: its name, version and program in `current.json`. A record of the
/// manager's install of ANOTHER program that still exists stays as it is (a second copy runs, a
/// developer's build: the installed one is what the manager keeps current); `false` then.
///
/// # Errors
///
/// The I/O error that stopped the write.
pub fn register(paths: &AppPaths, app: &Starting<'_>) -> std::io::Result<bool> {
    let before: Option<Registration> = read_json(&paths.registration());
    if let Some(before) = &before {
        let installed = Path::new(&before.binary);
        if before.managed && !same_program(installed, app.binary) && installed.exists() {
            return Ok(false);
        }
    }
    let before = before.unwrap_or_default();
    let same = same_program(Path::new(&before.binary), app.binary);
    // The manager's install keeps the path the manager wrote; another copy writes its own.
    let binary = if same && !before.binary.is_empty() {
        before.binary.clone()
    } else {
        app.binary.to_string_lossy().into_owned()
    };
    let same_version = same && before.version == app.version;
    let record = Registration {
        name: app.name.to_string(),
        key: app.key.to_string(),
        version: app.version.to_string(),
        binary,
        channel: before.channel.clone(),
        size_bytes: if same_version { before.size_bytes } else { 0 },
        updated_unix: if same_version && before.updated_unix > 0 {
            before.updated_unix
        } else {
            app.now_unix
        },
        launched_unix: app.now_unix,
        managed: same && before.managed,
    };
    if record != before {
        write_json(&paths.registration(), &record)?;
    }
    Ok(true)
}

/// Writes the app's "Update now" for the manager.
///
/// # Errors
///
/// The I/O error that stopped the write.
pub fn request_apply(paths: &AppPaths, request: &ApplyRequest) -> std::io::Result<()> {
    write_json(&paths.request(), request)
}

// ---------------------------------------------------------------------------
// The engine
// ---------------------------------------------------------------------------

/// What kind of installation a program is (azul's `InstallKind`): whether the manager may touch
/// it at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallKind {
    /// dpkg / rpm, `/usr`, `/opt` (Homebrew too): the package manager updates it.
    SystemPackageManager,
    Snap,
    Flatpak,
    WindowsStore,
    /// A folder this user may write to.
    UserWritable,
    /// Not writable and not a package manager's: a system-wide manual install.
    ReadOnly,
}

impl InstallKind {
    /// Whether the manager may replace the program.
    #[must_use]
    pub fn is_writable(self) -> bool {
        self == InstallKind::UserWritable
    }
}

/// The rollout gate of a check (azul's `UpdateAudience`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Audience {
    /// The manager installs it: the machine's cohort decides when a release reaches it.
    Gated,
    /// Someone else installs it (a package manager): it hears of a release once the rollout is
    /// complete.
    NotifyOnly,
    /// A beta tester: every release the moment it is published.
    Tester,
}

/// The gate for a program of kind `kind` on `channel`.
#[must_use]
pub fn audience_for(kind: InstallKind, channel: Channel) -> Audience {
    match (kind.is_writable(), channel) {
        (false, _) => Audience::NotifyOnly,
        (true, Channel::Beta) => Audience::Tester,
        (true, Channel::Stable) => Audience::Gated,
    }
}

/// What a manifest says about an app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Check {
    /// Nothing newer is open to this machine (a release still gated is not shown).
    UpToDate,
    /// A newer release this machine may have.
    Available(Release),
    /// The check could not complete (offline, a bad manifest), in words.
    Failed(String),
}

/// One manifest check's question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckRequest<'a> {
    /// The app's manifest (the config's template filled in).
    pub manifest_url: &'a str,
    /// The installed version (`0` before the first install).
    pub current_version: &'a str,
    /// The channel the manifest is read for.
    pub channel: Channel,
    /// The app's engine state for the channel ([`AppPaths::state`]).
    pub state_dir: &'a Path,
    /// The machine's rollout cohort.
    pub cohort: u8,
    /// The rollout gate the answer goes through.
    pub audience: Audience,
}

/// The update engine's calls. Blocking: the app and the manager make them on a `Thread`.
pub trait UpdateEngine: Send + Sync {
    /// Reads the manifest and answers under the cohort and the gate.
    fn check(&self, request: &CheckRequest<'_>) -> Check;
    /// Downloads `release` into `staging_dir` and verifies its digest and signature chain;
    /// the staged file. Nothing refused stays.
    ///
    /// # Errors
    ///
    /// Why the release was refused or could not be fetched.
    fn stage(
        &self,
        release: &Release,
        staging_dir: &Path,
        root_public_key: &str,
        state_dir: &Path,
    ) -> Result<PathBuf, String>;
    /// Checks a staged file once more before it is installed.
    ///
    /// # Errors
    ///
    /// Why the file must not be installed.
    fn verify(
        &self,
        file: &Path,
        release: &Release,
        root_public_key: &str,
        state_dir: &Path,
    ) -> Result<(), String>;
    /// What kind of installation `program` is.
    fn install_kind(&self, program: &Path) -> InstallKind;
    /// A new rollout cohort, 0 to 99 (drawn once per machine; the caller keeps it).
    fn draw_cohort(&self) -> u8;
    /// Whether unattended work may run at `now_unix` under the maintenance window `rule`
    /// (empty: any time).
    fn within_window(&self, rule: &str, now_unix: u64) -> bool;
}

/// The machine's rollout cohort: the config's, else drawn once now and kept in the config at
/// `config` (whoever runs first, the manager or an app, draws it; every later run reads it).
/// With why the drawn one could not be kept (it is used for this run anyway).
pub fn machine_cohort(
    engine: &dyn UpdateEngine,
    config: &Path,
    updates: &UpdatesSection,
) -> (u8, Option<String>) {
    if let Some(cohort) = updates.cohort {
        return (cohort, None);
    }
    let drawn = engine.draw_cohort().min(99);
    let kept = AzlinConfig::edit(config, |c| {
        if c.updates.cohort.is_none() {
            c.updates.cohort = Some(drawn);
        }
    });
    match kept {
        Ok(c) => (c.updates.cohort.unwrap_or(drawn), None),
        Err(e) => (drawn, Some(format!("{}: {e}", config.display()))),
    }
}

// ---------------------------------------------------------------------------
// One app's background round
// ---------------------------------------------------------------------------

/// What one background round works with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Round {
    /// The app's folders (the state, the staging, `staged.json`).
    pub paths: AppPaths,
    /// The installed version.
    pub current_version: String,
    /// The check's question: the manifest, the channel, the machine's cohort, the gate.
    pub manifest_url: String,
    pub channel: Channel,
    pub cohort: u8,
    pub audience: Audience,
    /// The root key staging verifies the chain against (empty: the digest pin only).
    pub root_public_key: String,
    /// The policy's auto-update: stage what is found.
    pub autoupdate: bool,
    /// The version the user skips.
    pub skip_version: Option<String>,
    /// The maintenance window staging waits for (empty: any time).
    pub maintenance_window: String,
}

/// What a background round found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoundOutcome {
    UpToDate,
    /// A newer release, not staged: auto-update is off, someone else installs the app, or the
    /// maintenance window is closed.
    Available(Release),
    /// A newer release, verified and waiting ([`AppPaths::staged`]).
    Staged(Staged),
    /// The newer release is the one the user skips.
    Skipped(String),
    Failed(String),
}

/// One app's round: check its manifest under the machine's cohort and, when the policy and the
/// maintenance window say so, stage what it found and keep it in `staged.json`. An update staged
/// before is not fetched again. Blocking: run it on a `Thread`.
pub fn background_round(engine: &dyn UpdateEngine, round: &Round, now_unix: u64) -> RoundOutcome {
    let state_dir = round.paths.state(round.channel);
    let release = match engine.check(&CheckRequest {
        manifest_url: &round.manifest_url,
        current_version: &round.current_version,
        channel: round.channel,
        state_dir: &state_dir,
        cohort: round.cohort,
        audience: round.audience,
    }) {
        Check::UpToDate => return RoundOutcome::UpToDate,
        Check::Failed(why) => return RoundOutcome::Failed(why),
        Check::Available(release) => release,
    };
    if round.skip_version.as_deref() == Some(release.version.as_str()) {
        return RoundOutcome::Skipped(release.version);
    }
    if let Some(staged) = read_json::<Staged>(&round.paths.staged()) {
        if staged.release.version == release.version && Path::new(&staged.file).exists() {
            return RoundOutcome::Staged(staged);
        }
    }
    let unattended = round.autoupdate
        && round.audience != Audience::NotifyOnly
        && engine.within_window(&round.maintenance_window, now_unix);
    if !unattended {
        return RoundOutcome::Available(release);
    }
    match engine.stage(
        &release,
        &round.paths.staging(),
        &round.root_public_key,
        &state_dir,
    ) {
        Ok(file) => {
            let staged = Staged {
                release,
                file: file.to_string_lossy().into_owned(),
                channel: round.channel.name().to_string(),
                staged_unix: now_unix,
            };
            match write_json(&round.paths.staged(), &staged) {
                Ok(()) => RoundOutcome::Staged(staged),
                Err(e) => RoundOutcome::Failed(format!("{}: {e}", round.paths.staged().display())),
            }
        }
        Err(why) => RoundOutcome::Failed(why),
    }
}

// ---------------------------------------------------------------------------
// azul's updater
// ---------------------------------------------------------------------------

/// The engine of the apps: azul's updater (`ReleaseInfo::check_program`, `stage`,
/// `verify_staged`, `InstallKind::detect_for`).
#[cfg(feature = "azul")]
#[derive(Clone, Copy, Debug, Default)]
pub struct AzulEngine;

#[cfg(feature = "azul")]
mod azul_engine {
    use std::path::{Path, PathBuf};

    use azul::{
        callbacks::{UpdateAudience, UpdateCheckResult, UpdateStageResult},
        misc::{InstallKind as AzInstallKind, ReleaseInfo},
        str::String as AzString,
        vec::StringPairVec,
    };

    use super::{Audience, AzulEngine, Check, CheckRequest, InstallKind, Release, UpdateEngine};

    fn az(text: &str) -> AzString {
        AzString::from(text)
    }

    fn path_text(path: &Path) -> AzString {
        AzString::from(path.to_string_lossy().as_ref())
    }

    /// azul's `ReleaseInfo` of a kept release.
    fn release_info(r: &Release) -> ReleaseInfo {
        ReleaseInfo {
            version: az(&r.version),
            download_url: az(&r.download_url),
            changelog_md_url: az(&r.changelog_md_url),
            changelog_md_inline: az(&r.changelog_md_inline),
            digest: az(&r.digest),
            signature: az(&r.signature),
            signing_key_statement: az(&r.signing_key_statement),
            signing_key_statement_sig: az(&r.signing_key_statement_sig),
            download_headers: StringPairVec::from_vec(Vec::new()),
        }
    }

    /// A kept release of azul's `ReleaseInfo` (the download headers are a registry's
    /// short-lived token: not kept).
    fn release_of(r: &ReleaseInfo) -> Release {
        Release {
            version: r.version.as_str().to_string(),
            download_url: r.download_url.as_str().to_string(),
            changelog_md_url: r.changelog_md_url.as_str().to_string(),
            changelog_md_inline: r.changelog_md_inline.as_str().to_string(),
            digest: r.digest.as_str().to_string(),
            signature: r.signature.as_str().to_string(),
            signing_key_statement: r.signing_key_statement.as_str().to_string(),
            signing_key_statement_sig: r.signing_key_statement_sig.as_str().to_string(),
        }
    }

    impl UpdateEngine for AzulEngine {
        fn check(&self, q: &CheckRequest<'_>) -> Check {
            let audience = match q.audience {
                Audience::Gated => UpdateAudience::AutoUpdate,
                Audience::NotifyOnly => UpdateAudience::NotifyOnly,
                Audience::Tester => UpdateAudience::Tester,
            };
            match ReleaseInfo::check_program(
                az(q.manifest_url),
                az(q.current_version),
                az(q.channel.name()),
                path_text(q.state_dir),
                q.cohort,
                audience,
            ) {
                UpdateCheckResult::UpToDate => Check::UpToDate,
                UpdateCheckResult::Available(release) => Check::Available(release_of(&release)),
                UpdateCheckResult::Error(why) => Check::Failed(why.as_str().to_string()),
            }
        }

        fn stage(
            &self,
            release: &Release,
            staging_dir: &Path,
            root_public_key: &str,
            state_dir: &Path,
        ) -> Result<PathBuf, String> {
            match release_info(release).stage(
                path_text(staging_dir),
                az(root_public_key),
                path_text(state_dir),
            ) {
                UpdateStageResult::Staged(file) => Ok(PathBuf::from(file.as_str())),
                UpdateStageResult::Error(why) => Err(why.as_str().to_string()),
            }
        }

        fn verify(
            &self,
            file: &Path,
            release: &Release,
            root_public_key: &str,
            state_dir: &Path,
        ) -> Result<(), String> {
            match release_info(release).verify_staged(
                path_text(file),
                az(root_public_key),
                path_text(state_dir),
            ) {
                UpdateStageResult::Staged(_) => Ok(()),
                UpdateStageResult::Error(why) => Err(why.as_str().to_string()),
            }
        }

        fn install_kind(&self, program: &Path) -> InstallKind {
            match AzInstallKind::detect_for(path_text(program)) {
                AzInstallKind::SystemPackageManager => InstallKind::SystemPackageManager,
                AzInstallKind::Snap => InstallKind::Snap,
                AzInstallKind::Flatpak => InstallKind::Flatpak,
                AzInstallKind::WindowsStore => InstallKind::WindowsStore,
                AzInstallKind::UserWritable => InstallKind::UserWritable,
                AzInstallKind::ReadOnly => InstallKind::ReadOnly,
            }
        }

        fn draw_cohort(&self) -> u8 {
            ReleaseInfo::draw_rollout_bucket()
        }

        fn within_window(&self, rule: &str, now_unix: u64) -> bool {
            rule.trim().is_empty() || ReleaseInfo::within_maintenance_window(az(rule), now_unix)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use azul_storage::testing::TempDir;

    use super::*;

    /// An engine that answers from a table and records what it was asked.
    #[derive(Default)]
    struct FakeEngine {
        answer: Mutex<Option<Check>>,
        stage_error: Option<String>,
        window_open: bool,
        staged_versions: Mutex<Vec<String>>,
    }

    impl FakeEngine {
        fn answering(check: Check) -> FakeEngine {
            FakeEngine {
                answer: Mutex::new(Some(check)),
                window_open: true,
                ..FakeEngine::default()
            }
        }
    }

    impl UpdateEngine for FakeEngine {
        fn check(&self, _: &CheckRequest<'_>) -> Check {
            self.answer.lock().unwrap().clone().unwrap_or(Check::UpToDate)
        }
        fn stage(
            &self,
            release: &Release,
            staging_dir: &Path,
            _: &str,
            _: &Path,
        ) -> Result<PathBuf, String> {
            if let Some(why) = &self.stage_error {
                return Err(why.clone());
            }
            self.staged_versions.lock().unwrap().push(release.version.clone());
            std::fs::create_dir_all(staging_dir).map_err(|e| e.to_string())?;
            let file = staging_dir.join(format!("app-{}.bin", release.version));
            std::fs::write(&file, b"bytes").map_err(|e| e.to_string())?;
            Ok(file)
        }
        fn verify(&self, _: &Path, _: &Release, _: &str, _: &Path) -> Result<(), String> {
            Ok(())
        }
        fn install_kind(&self, _: &Path) -> InstallKind {
            InstallKind::UserWritable
        }
        fn draw_cohort(&self) -> u8 {
            42
        }
        fn within_window(&self, _: &str, _: u64) -> bool {
            self.window_open
        }
    }

    fn release(version: &str) -> Release {
        Release {
            version: version.to_string(),
            download_url: format!("http://example.org/app-{version}.bin"),
            ..Release::default()
        }
    }

    fn round(dir: &Path) -> Round {
        Round {
            paths: AppPaths::new(dir, &dir.join("apps"), "azwriter"),
            current_version: "1.0.0".to_string(),
            manifest_url: "http://example.org/azwriter/stable.json".to_string(),
            channel: Channel::Stable,
            cohort: 42,
            audience: Audience::Gated,
            root_public_key: String::new(),
            autoupdate: true,
            skip_version: None,
            maintenance_window: String::new(),
        }
    }

    #[test]
    fn an_apps_folders_are_its_registry_folder_and_its_install_folder() {
        let p = AppPaths::new(Path::new("/h/.azlin"), Path::new("/h/.azlin/apps"), "azwriter");
        assert_eq!(p.registration(), Path::new("/h/.azlin/apps/azwriter/current.json"));
        assert_eq!(p.state(Channel::Beta), Path::new("/h/.azlin/apps/azwriter/state/beta"));
        assert_eq!(p.current(), Path::new("/h/.azlin/apps/azwriter/current"));
        assert_eq!(p.prev(), Path::new("/h/.azlin/apps/azwriter/prev"));
        let mut updates = UpdatesSection::default();
        assert_eq!(install_dir(Path::new("/h/.azlin"), &updates), Path::new("/h/.azlin/apps"));
        updates.install_dir = Some("/data/apps".to_string());
        assert_eq!(install_dir(Path::new("/h/.azlin"), &updates), Path::new("/data/apps"));
    }

    #[test]
    fn the_manifest_url_names_the_app_and_the_channel_and_none_is_configured_by_default() {
        let mut updates = UpdatesSection::default();
        assert_eq!(manifest_url_for(&updates, "azwriter", Channel::Stable), None);
        updates.manifest_url = Some("https://example.org/{app}/{channel}/updates.json".to_string());
        assert_eq!(
            manifest_url_for(&updates, "azwriter", Channel::Beta).as_deref(),
            Some("https://example.org/azwriter/beta/updates.json")
        );
    }

    #[test]
    fn a_package_managers_copy_hears_late_a_tester_at_once_and_the_rest_by_cohort() {
        assert_eq!(audience_for(InstallKind::Flatpak, Channel::Beta), Audience::NotifyOnly);
        assert_eq!(audience_for(InstallKind::ReadOnly, Channel::Stable), Audience::NotifyOnly);
        assert_eq!(audience_for(InstallKind::UserWritable, Channel::Beta), Audience::Tester);
        assert_eq!(audience_for(InstallKind::UserWritable, Channel::Stable), Audience::Gated);
    }

    #[test]
    fn a_started_app_registers_itself_but_never_over_the_managers_install_of_another_copy() {
        let dir = TempDir::new("update-register");
        let paths = AppPaths::new(dir.path(), &dir.path().join("apps"), "azwriter");
        let dev = dir.path().join("target/AzWriter");
        let start = |binary: &Path, version: &str, now: u64| {
            register(
                &paths,
                &Starting {
                    name: "AzWriter",
                    key: "azwriter",
                    version,
                    binary,
                    now_unix: now,
                },
            )
            .unwrap()
        };
        assert!(start(&dev, "0.1.0", 10));
        let r: Registration = read_json(&paths.registration()).unwrap();
        assert_eq!((r.version.as_str(), r.updated_unix, r.managed), ("0.1.0", 10, false));

        // The manager installs; its record names its own program.
        let installed = paths.current().join("AzWriter");
        std::fs::create_dir_all(paths.current()).unwrap();
        std::fs::write(&installed, b"program").unwrap();
        let managed = Registration {
            name: "AzWriter".into(),
            key: "azwriter".into(),
            version: "2.0.0".into(),
            binary: installed.to_string_lossy().into_owned(),
            channel: "stable".into(),
            size_bytes: 7,
            updated_unix: 20,
            launched_unix: 0,
            managed: true,
        };
        write_json(&paths.registration(), &managed).unwrap();
        assert!(!start(&dev, "0.1.0", 30), "the developer's build leaves the install's record");
        assert_eq!(read_json::<Registration>(&paths.registration()).unwrap(), managed);

        // The installed program itself only notes its start.
        assert!(start(&installed, "2.0.0", 40));
        let r: Registration = read_json(&paths.registration()).unwrap();
        assert_eq!(
            (r.managed, r.size_bytes, r.updated_unix, r.launched_unix, r.channel.as_str()),
            (true, 7, 20, 40, "stable")
        );
    }

    #[test]
    fn a_program_reached_through_a_link_is_the_same_program() {
        let dir = TempDir::new("update-same");
        let program = dir.path().join("AzWriter");
        std::fs::write(&program, b"program").unwrap();
        assert!(same_program(&program, &program));
        #[cfg(unix)]
        {
            let link = dir.path().join("link");
            std::os::unix::fs::symlink(dir.path(), &link).unwrap();
            assert!(same_program(&program, &link.join("AzWriter")));
        }
        assert!(!same_program(&program, &dir.path().join("other")));
    }

    #[test]
    fn a_round_stages_what_it_finds_once_and_keeps_it_for_the_banner() {
        let dir = TempDir::new("update-round");
        let engine = FakeEngine::answering(Check::Available(release("2.0.0")));
        let r = round(dir.path());
        let RoundOutcome::Staged(staged) = background_round(&engine, &r, 100) else {
            panic!("an available release is staged");
        };
        assert_eq!(staged.release.version, "2.0.0");
        assert_eq!(staged.channel, "stable");
        assert!(Path::new(&staged.file).exists());
        assert_eq!(read_json::<Staged>(&r.paths.staged()), Some(staged.clone()));
        // The next round finds it staged: no second download.
        assert_eq!(background_round(&engine, &r, 200), RoundOutcome::Staged(staged));
        assert_eq!(*engine.staged_versions.lock().unwrap(), vec!["2.0.0".to_string()]);
    }

    #[test]
    fn a_round_only_reports_when_auto_update_is_off_the_copy_is_not_ours_or_the_window_is_closed() {
        let dir = TempDir::new("update-round-quiet");
        let engine = FakeEngine::answering(Check::Available(release("2.0.0")));
        let mut r = round(dir.path());
        r.autoupdate = false;
        assert_eq!(background_round(&engine, &r, 1), RoundOutcome::Available(release("2.0.0")));
        r.autoupdate = true;
        r.audience = Audience::NotifyOnly;
        assert_eq!(background_round(&engine, &r, 1), RoundOutcome::Available(release("2.0.0")));
        let closed = FakeEngine {
            window_open: false,
            ..FakeEngine::answering(Check::Available(release("2.0.0")))
        };
        r.audience = Audience::Gated;
        assert_eq!(background_round(&closed, &r, 1), RoundOutcome::Available(release("2.0.0")));
        assert!(engine.staged_versions.lock().unwrap().is_empty());
        assert!(!r.paths.staged().exists());
    }

    #[test]
    fn a_round_skips_the_skipped_version_and_says_why_staging_failed() {
        let dir = TempDir::new("update-round-skip");
        let engine = FakeEngine::answering(Check::Available(release("2.0.0")));
        let mut r = round(dir.path());
        r.skip_version = Some("2.0.0".to_string());
        assert_eq!(background_round(&engine, &r, 1), RoundOutcome::Skipped("2.0.0".to_string()));
        r.skip_version = Some("1.5.0".to_string());
        let refusing = FakeEngine {
            stage_error: Some("artifact signature invalid".to_string()),
            ..FakeEngine::answering(Check::Available(release("2.0.0")))
        };
        assert_eq!(
            background_round(&refusing, &r, 1),
            RoundOutcome::Failed("artifact signature invalid".to_string())
        );
        assert!(!r.paths.staged().exists(), "nothing refused is kept for the banner");
        let up_to_date = FakeEngine::answering(Check::UpToDate);
        assert_eq!(background_round(&up_to_date, &r, 1), RoundOutcome::UpToDate);
    }

    #[test]
    fn the_cohort_is_drawn_once_and_kept_in_the_config() {
        let dir = TempDir::new("update-cohort");
        let config = dir.path().join("config.json");
        let engine = FakeEngine::default();
        let (drawn, problem) = machine_cohort(&engine, &config, &UpdatesSection::default());
        assert_eq!((drawn, problem), (42, None));
        let (c, _) = AzlinConfig::load(&config);
        assert_eq!(c.updates.cohort, Some(42));
        let mut kept = c.updates.clone();
        kept.cohort = Some(7);
        assert_eq!(machine_cohort(&engine, &config, &kept).0, 7, "a kept cohort is never drawn again");
    }

    #[test]
    fn the_shared_files_round_trip_and_a_missing_one_is_none() {
        let dir = TempDir::new("update-files");
        let path = dir.path().join("apps/azmail/request.json");
        assert_eq!(read_json::<ApplyRequest>(&path), None);
        let request = ApplyRequest {
            version: "2.0.0".into(),
            pid: 4242,
            relaunch: true,
            asked_unix: 9,
        };
        request_apply(
            &AppPaths::new(dir.path(), &dir.path().join("apps"), "azmail"),
            &request,
        )
        .unwrap();
        assert_eq!(read_json::<ApplyRequest>(&path), Some(request));
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(read_json::<ApplyRequest>(&path), None);
    }
}
