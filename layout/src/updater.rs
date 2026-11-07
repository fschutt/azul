//! Version checking + update policy — Phase 4 of the observability plan.
//!
//! The posture, in order:
//!
//! 1. **The system package manager owns managed installs.** If the running binary is
//!    dpkg/rpm-owned, lives under `/usr`, or runs inside a Snap/Flatpak/`WindowsApps` sandbox,
//!    self-update is REFUSED at runtime and the effective mode clamps to
//!    [`UpdateMode::NotifyOnly`]: the dialog says "a new version is available — update via your
//!    package manager", never touches the binary.
//! 2. **Notify-only elsewhere unless the app opts into self-update.** The user's choice to update
//!    is always respected: nothing installs without the dialog's consent, and
//!    `download_automatically` only STAGES the artifact so consent applies instantly.
//! 3. Checks are ASYNC (the `CallbackInfo::check_for_updates` wrapper runs this module on an azul
//!    `Thread`) and observed through the existing `app_update_check_total` /
//!    `app_update_apply_total` metrics.
//!
//! v1 ships the `HttpManifestSource` (a small JSON manifest) and the policy
//! engine (install-kind backstops, anti-downgrade, check cooldown,
//! suspend). Signature verification (minisign chain), GitHub/OCI sources
//! and the atomic self-update swap are the documented next rungs — the
//! manifest format already carries the digest field they need.

use std::path::{Path, PathBuf};

use azul_css::AzString;

/// What kind of installation the running binary is — decides whether
/// self-update is even on the table.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub enum InstallKind {
    /// Owned by a system package manager (dpkg/rpm) or under `/usr` —
    /// updates belong to the distro.
    SystemPackageManager,
    /// Snap confinement (`$SNAP` set).
    Snap,
    /// Flatpak sandbox (`/.flatpak-info` exists).
    Flatpak,
    /// Windows Store install (`WindowsApps` in the path).
    WindowsStore,
    /// A user-writable location (`~/.local`, `AppData`, a dev checkout) —
    /// self-update is mechanically possible.
    UserWritable,
    /// The binary path is not writable by this user and not recognizably
    /// package-managed (system-wide manual install).
    ReadOnly,
}

impl InstallKind {
    /// Detects the install kind of the CURRENT executable.
    #[must_use]
    pub fn detect() -> Self {
        let exe = std::env::current_exe().unwrap_or_default();
        Self::detect_for(&exe)
    }

    /// The detection, parameterized for tests.
    #[must_use]
    pub fn detect_for(exe: &Path) -> Self {
        if std::env::var_os("SNAP").is_some() {
            return Self::Snap;
        }
        if Path::new("/.flatpak-info").exists() {
            return Self::Flatpak;
        }
        let path_str = exe.to_string_lossy();
        if path_str.contains("WindowsApps") {
            return Self::WindowsStore;
        }
        if path_str.starts_with("/usr/") || path_str.starts_with("/opt/") {
            // Under /usr the distro owns the file even when dpkg -S would
            // not answer (rpm systems, manual make-install): notify-only.
            return Self::SystemPackageManager;
        }
        #[cfg(target_os = "linux")]
        {
            // dpkg -S resolves ownership without touching the network; a
            // missing dpkg binary (non-Debian) just falls through.
            if let Ok(out) = std::process::Command::new("dpkg")
                .arg("-S")
                .arg(exe.as_os_str())
                .output()
            {
                if out.status.success() {
                    return Self::SystemPackageManager;
                }
            }
        }
        // Writable check on the binary itself: metadata + a permissions
        // heuristic beats attempting an open-for-write on our own image.
        let writable = std::fs::metadata(exe)
            .map(|m| !m.permissions().readonly())
            .unwrap_or(false);
        if writable {
            Self::UserWritable
        } else {
            Self::ReadOnly
        }
    }

    /// Whether self-update may touch the binary at all.
    #[must_use]
    pub const fn allows_self_update(&self) -> bool {
        matches!(self, Self::UserWritable)
    }

    /// The "update via …" hint the notify dialog shows for managed installs.
    #[must_use]
    pub const fn package_manager_hint(&self) -> &'static str {
        match self {
            Self::SystemPackageManager => "your system package manager (apt/dnf/…)",
            Self::Snap => "snap refresh",
            Self::Flatpak => "flatpak update",
            Self::WindowsStore => "the Microsoft Store",
            Self::UserWritable | Self::ReadOnly => "",
        }
    }
}

/// Build-time / app-chosen update behaviour; the EFFECTIVE mode is this
/// clamped by [`InstallKind`] — see [`effective_mode`]. Defined in core so
/// `AppConfig.updates` can carry it without the `updater` feature.
pub use azul_core::resources::{UpdateMode, UpdateSettings};

/// Applies the machine-wide shared config's update policy ON TOP of
/// [`effective_mode`]: `updates.autoupdate: false` (shared default or this
/// app's override in `{config_dir}/azul/config.json`) clamps `SelfUpdate`
/// to `NotifyOnly` — the machine's owner outranks the app's preference.
#[cfg(feature = "telemetry")]
#[must_use]
pub fn apply_shared_update_policy(effective: UpdateMode) -> UpdateMode {
    if effective != UpdateMode::SelfUpdate {
        return effective;
    }
    let shared = crate::telemetry::sharedconfig::SharedConfig::load();
    let app = crate::telemetry::sharedconfig::app_key().unwrap_or_default();
    match shared.updates_for(&app).autoupdate {
        Some(false) => UpdateMode::NotifyOnly,
        _ => effective,
    }
}

/// Without the shared-config machinery (telemetry feature off) there is no
/// machine-wide policy file to consult.
#[cfg(not(feature = "telemetry"))]
#[must_use]
pub const fn apply_shared_update_policy(effective: UpdateMode) -> UpdateMode {
    effective
}

/// Whether UNATTENDED update work (automatic staging; unattended apply when
/// it exists) may run right now, per the shared config's RRULE maintenance
/// window. No window configured = always allowed.
#[cfg(feature = "telemetry")]
#[must_use]
pub fn within_shared_maintenance_window(now_unix: u64) -> bool {
    let shared = crate::telemetry::sharedconfig::SharedConfig::load();
    let app = crate::telemetry::sharedconfig::app_key().unwrap_or_default();
    match shared.updates_for(&app).maintenance_window {
        Some(rule) => crate::telemetry::sharedconfig::within_maintenance_window(&rule, now_unix),
        None => true,
    }
}

/// Always allowed without the shared-config machinery.
#[cfg(not(feature = "telemetry"))]
#[must_use]
pub const fn within_shared_maintenance_window(_now_unix: u64) -> bool {
    true
}

/// Clamps the requested mode by what the installation permits: a
/// package-managed binary NEVER self-updates, whatever the app asked for.
#[must_use]
pub const fn effective_mode(requested: UpdateMode, install: &InstallKind) -> UpdateMode {
    match requested {
        UpdateMode::Disabled => UpdateMode::Disabled,
        UpdateMode::NotifyOnly => UpdateMode::NotifyOnly,
        UpdateMode::SelfUpdate => {
            if install.allows_self_update() {
                UpdateMode::SelfUpdate
            } else {
                UpdateMode::NotifyOnly
            }
        }
    }
}

/// Options for one check, mirrored into the C API.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
#[repr(C)]
pub struct UpdateOptions {
    /// Stage the artifact in the background so "install now" is instant.
    /// Staging is NOT installing — consent still gates the swap.
    pub download_automatically: bool,
}

/// One release, as the manifest describes it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[repr(C)]
pub struct ReleaseInfo {
    /// Version string, compared with [`compare_versions`].
    pub version: AzString,
    /// Where the artifact for THIS platform lives.
    pub download_url: AzString,
    /// The release's changelog (Markdown), for the `UpdateVersion` dialog.
    pub changelog_md_url: AzString,
    /// The changelog Markdown ITSELF, when the source carried it inline (a
    /// GitHub release body does). Preferred over fetching
    /// `changelog_md_url`, which saves a request and works offline-ish.
    pub changelog_md_inline: AzString,
    /// Hex digest of the artifact (verified when non-empty).
    pub digest: AzString,
    /// Minisign signature of the ARTIFACT by the release-signing key (the
    /// full `.minisig` text). Required whenever the app pins a
    /// `root_public_key`; the signing key itself comes ONLY from the
    /// root-signed statement below - a manifest cannot name its own key.
    pub signature: AzString,
    /// The signing-key statement: `azul-signing-key-v1|pubkey=<b64>|`
    /// `expires=<unix>|generation=<n>`. Delegates from the compiled-in root
    /// key to the day-to-day signing key so the latter can rotate (bump
    /// `generation`) or expire without shipping a new binary.
    pub signing_key_statement: AzString,
    /// Minisign signature of the STATEMENT string by the ROOT key
    /// (full `.minisig` text).
    pub signing_key_statement_sig: AzString,
    /// The signed release statement: `azul-release-v1|app=<id>|version=<semver>|`
    /// `platform=<os-arch>|sha256=<hex>|released=<unix>|min_generation=<n>`. It binds the
    /// artifact's hash to the app, the version and the platform it is for: the manifest's own
    /// `version`, `download_url` and rollout are unsigned, and without this a genuine old
    /// release could be served under a new version label (REVIEW Y3).
    pub release_statement: AzString,
    /// Minisign signature of the release statement by the SIGNING key the root-signed
    /// statement names (full `.minisig` text).
    pub release_statement_sig: AzString,
    /// Headers the DOWNLOAD needs, when the artifact is not on open HTTP —
    /// an OCI registry blob wants the same bearer token the manifest
    /// request used. Empty for ordinary downloads.
    pub download_headers: azul_core::window::StringPairVec,
}

/// What a check concluded.
// `Available` carries the whole ReleaseInfo — boxing is not an option in a
// repr(C,u8) ABI enum whose layout the C bindings will depend on.
#[allow(variant_size_differences)]
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum UpdateCheckResult {
    /// Current version is the newest known.
    UpToDate,
    /// A newer release exists.
    Available(ReleaseInfo),
    /// The check could not complete (offline, bad manifest, …).
    Error(AzString),
}

/// Version precedence, semver-style (`1.10.2 > 1.9.9`, `2.0.0-rc1 < 2.0.0`).
///
/// The dotted core (`1.10.2`) compares segment by segment, numerically (a non-numeric segment
/// lexically); a missing segment is below a present one (`1.2 < 1.2.1`). When the cores are equal a
/// version WITH a pre-release (`-rc1`, `-beta.2`) is below the one without; two pre-releases
/// compare identifier by identifier, numbers numerically and below words, words in ASCII order,
/// and a list that is the start of a longer one is below it. Build metadata (`+abc`) and a leading
/// `v` do not count. (REVIEW Y11: `2.0.0-rc1` used to order ABOVE `2.0.0`, so once a channel had
/// offered a release candidate the final was refused as a downgrade.) A full `VersionScheme` trait
/// rides in when a source needs another scheme.
#[must_use]
pub fn compare_versions(a: &str, b: &str) -> core::cmp::Ordering {
    use core::cmp::Ordering;
    let ((core_a, pre_a), (core_b, pre_b)) = (split_version(a), split_version(b));
    compare_dotted(core_a, core_b).then_with(|| match (pre_a, pre_b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(x), Some(y)) => compare_pre_release(x, y),
    })
}

/// `v1.2.3-rc.1+build` -> (`1.2.3`, `Some("rc.1")`): the core and the pre-release, without the
/// build metadata and the leading `v`. A `-` with nothing after it is no pre-release.
fn split_version(version: &str) -> (&str, Option<&str>) {
    let version = version.trim_start_matches('v');
    let version = version.split_once('+').map_or(version, |(head, _)| head);
    match version.split_once('-') {
        Some((core, pre)) if !pre.is_empty() => (core, Some(pre)),
        Some((core, _)) => (core, None),
        None => (version, None),
    }
}

/// The dotted core: numeric segments numerically, others lexically, a missing segment below.
fn compare_dotted(a: &str, b: &str) -> core::cmp::Ordering {
    let mut xs = a.split('.');
    let mut ys = b.split('.');
    loop {
        match (xs.next(), ys.next()) {
            (None, None) => return core::cmp::Ordering::Equal,
            (Some(_), None) => return core::cmp::Ordering::Greater,
            (None, Some(_)) => return core::cmp::Ordering::Less,
            (Some(x), Some(y)) => {
                let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(xn), Ok(yn)) => xn.cmp(&yn),
                    _ => x.cmp(y),
                };
                if ord != core::cmp::Ordering::Equal {
                    return ord;
                }
            }
        }
    }
}

/// Two pre-release lists (`alpha.1`, `rc.2`): semver 2.0.0 section 11.
fn compare_pre_release(a: &str, b: &str) -> core::cmp::Ordering {
    use core::cmp::Ordering;
    let mut xs = a.split('.');
    let mut ys = b.split('.');
    loop {
        match (xs.next(), ys.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(xn), Ok(yn)) => xn.cmp(&yn),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => x.cmp(y),
                };
                if ord != Ordering::Equal {
                    return ord;
                }
            }
        }
    }
}

/// Persistent updater state (`{data_dir}/update-state.json`): the
/// anti-downgrade high-water mark, the last-check time for the cooldown,
/// and the "remind me later" suspension.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UpdateState {
    /// Highest version this client has ever SEEN — a manifest offering less
    /// than this is a downgrade (or a rollback attack) and reports `UpToDate`.
    pub highest_seen: String,
    /// Unix seconds of the last completed check.
    pub last_check_unix: u64,
    /// "Remind me later": checks report `UpToDate` until this passes.
    pub suspended_until_unix: u64,
    /// This client's PHASED-ROLLOUT cohort (0-99), drawn once and persisted
    /// so the client stays in the same A/B cohort for every release. `None`
    /// until the first gated check draws it — see
    /// [`UpdateState::rollout_bucket`].
    pub rollout_bucket: Option<u8>,
    /// Highest signing-key GENERATION ever accepted. A statement with a
    /// lower generation is a rollback (an attacker replaying a retired,
    /// possibly leaked key) and is refused.
    pub key_generation: u64,
    /// The latest time (unix seconds) a signature chain was fully verified at: a FLOOR for the
    /// clock the chain's expiry and "dated in the future" checks use. A clock set back (an
    /// on-path NTP answer, a user's change) must not revive a signing-key statement that
    /// expired; it only goes up, and only with a chain that verified - so it never runs ahead of
    /// a statement's own expiry (X-DELTA-AZDRIVE-KEYS-33).
    pub clock_floor_unix: u64,
}

/// The file of [`UpdateState`] in a state folder.
const STATE_FILE: &str = "update-state.json";

impl UpdateState {
    /// Reads `dir/update-state.json`. A file that is not there is a first run (`Ok(default)`); a
    /// file that is there and cannot be read or parsed - or has a field of another type than it is
    /// written with - is an ERROR, not a fresh start: the marks it held (the highest version, the
    /// key generation) are what refuse a rollback to a retired, possibly leaked signing key, and
    /// starting again from zero would accept that key once more (REVIEW Y7). The message says
    /// which file to delete to start over, knowingly.
    ///
    /// # Errors
    ///
    /// Why the file could not be read or understood.
    pub fn read(dir: &Path) -> Result<Self, String> {
        let path = dir.join(STATE_FILE);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(format!("update state {}: {e}", path.display())),
        };
        let damaged = |why: &str| {
            format!(
                "update state {} is damaged ({why}): the marks that refuse an older release are \
                 in it, so it is not replaced silently - delete the file to start over, knowing \
                 that an older release is then accepted again",
                path.display()
            )
        };
        let v = serde_json::from_str::<serde_json::Value>(&text)
            .map_err(|e| damaged(&e.to_string()))?;
        if !v.is_object() {
            return Err(damaged("not a JSON object"));
        }
        // A field that is there has the type it is written with: `"key_generation": "5"` is not
        // a zero to be written back over the mark. A field that is not there is a file from
        // before the field, or a first write: its zero (X-DELTA-AZDRIVE-KEYS-33).
        let text_field = |name: &str| -> Result<String, String> {
            match v.get(name) {
                None => Ok(String::new()),
                Some(serde_json::Value::String(s)) => Ok(s.clone()),
                Some(_) => Err(damaged(&format!("`{name}` is not text"))),
            }
        };
        let number_field = |name: &str| -> Result<u64, String> {
            match v.get(name) {
                None => Ok(0),
                Some(x) => x
                    .as_u64()
                    .ok_or_else(|| damaged(&format!("`{name}` is not a whole number"))),
            }
        };
        let rollout_bucket = match v.get("rollout_bucket") {
            None | Some(serde_json::Value::Null) => None,
            Some(x) => Some(
                x.as_u64()
                    .and_then(|b| u8::try_from(b).ok())
                    .filter(|b| *b < 100)
                    .ok_or_else(|| damaged("`rollout_bucket` is not a number from 0 to 99"))?,
            ),
        };
        Ok(Self {
            highest_seen: text_field("highest_seen")?,
            last_check_unix: number_field("last_check_unix")?,
            suspended_until_unix: number_field("suspended_until_unix")?,
            rollout_bucket,
            key_generation: number_field("key_generation")?,
            clock_floor_unix: number_field("clock_floor_unix")?,
        })
    }

    /// Loads from `dir/update-state.json` (a missing file = default). For a state that is only
    /// about a check (the cooldown, the cohort): a damaged file also reads as the default HERE, but
    /// [`UpdateState::try_save`] will not write over it, so the marks in it are not lost. Whatever
    /// installs something uses [`UpdateState::read`].
    #[must_use]
    pub fn load(dir: &Path) -> Self {
        Self::read(dir).unwrap_or_default()
    }

    /// The marks only go up: raises this state's highest version, key generation and clock floor
    /// to the ones `other` holds, and takes its cohort when this has none.
    fn raise_marks_to(&mut self, other: &Self) {
        if compare_versions(&other.highest_seen, &self.highest_seen) == core::cmp::Ordering::Greater
        {
            self.highest_seen.clone_from(&other.highest_seen);
        }
        self.key_generation = self.key_generation.max(other.key_generation);
        self.clock_floor_unix = self.clock_floor_unix.max(other.clock_floor_unix);
        if self.rollout_bucket.is_none() {
            self.rollout_bucket = other.rollout_bucket;
        }
    }

    /// This client's rollout cohort (0-99): the "fake-rand" that decides
    /// WHEN a phased rollout reaches this install. Drawn once (from the
    /// clock's sub-second noise), then PERSISTED — the client stays in the
    /// same cohort for every release, which is what makes per-version A/B
    /// comparisons in Grafana meaningful. `AZ_UPDATE_BUCKET=<0-99>`
    /// overrides for drills and tests.
    pub fn rollout_bucket(&mut self) -> u8 {
        if let Some(forced) = std::env::var("AZ_UPDATE_BUCKET")
            .ok()
            .and_then(|v| v.parse::<u8>().ok())
            .filter(|b| *b < 100)
        {
            return forced;
        }
        if let Some(bucket) = self.rollout_bucket {
            return bucket;
        }
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let bucket =
            u8::try_from((u64::from(nanos) ^ u64::from(std::process::id())) % 100).unwrap_or(0);
        self.rollout_bucket = Some(bucket);
        bucket
    }

    /// Writes to `dir/update-state.json`, atomically (a temp file in the same folder, then a
    /// rename: a crash leaves the old file or the new one, never half of one), and never LOWER
    /// than what is on disk - the app and the install manager each hold the marks of a program
    /// and may both write, and the marks only go up; the read of what is on disk, the merge and
    /// the rename happen under a lock on `update-state.json.lock`. A file on disk that is damaged
    /// is not written over (see [`UpdateState::read`]).
    ///
    /// # Errors
    ///
    /// Why the state could not be recorded; whatever installs something refuses to go on then.
    pub fn try_save(&self, dir: &Path) -> Result<(), String> {
        let path = dir.join(STATE_FILE);
        std::fs::create_dir_all(dir).map_err(|e| format!("update state {}: {e}", dir.display()))?;
        // The app and the install manager both write the marks of a program: the read, the merge
        // and the rename are ONE step under an exclusive lock, or a mark one of them raised in
        // between is written over by the other (X-DELTA-AZDRIVE-KEYS-33).
        let _lock = StateLock::take(dir)?;
        let mut merged = self.clone();
        merged.raise_marks_to(&Self::read(dir)?);
        let text = serde_json::json!({
            "highest_seen": merged.highest_seen,
            "last_check_unix": merged.last_check_unix,
            "suspended_until_unix": merged.suspended_until_unix,
            "rollout_bucket": merged.rollout_bucket,
            "key_generation": merged.key_generation,
            "clock_floor_unix": merged.clock_floor_unix,
        })
        .to_string();
        let temp = dir.join(format!("{STATE_FILE}.{}.tmp", unique_suffix()));
        let written = (|| -> std::io::Result<()> {
            use std::io::Write as _;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)?;
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
            std::fs::rename(&temp, &path)
        })();
        written.map_err(|e| {
            drop(std::fs::remove_file(&temp));
            format!("update state {}: {e}", path.display())
        })
    }

    /// [`UpdateState::try_save`] for a state that is only about a check: a failure is dropped.
    pub fn save(&self, dir: &Path) {
        drop(self.try_save(dir));
    }
}

/// The exclusive lock of a state folder (`update-state.json.lock`), held for one read-merge-rename
/// and let go on drop. The OS lets go of it with the handle, also when the process dies.
struct StateLock {
    _file: std::fs::File,
}

impl StateLock {
    /// Waits for the lock of `dir` (its holders hold it for one small read and one rename).
    fn take(dir: &Path) -> Result<Self, String> {
        let path = dir.join(format!("{STATE_FILE}.lock"));
        let failed = |e: std::io::Error| format!("update state {}: {e}", path.display());
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(failed)?;
        match file.lock() {
            Ok(()) => {}
            // A platform whose files cannot be locked: the rename is atomic all the same.
            Err(e) if e.kind() == std::io::ErrorKind::Unsupported => {}
            Err(e) => return Err(failed(e)),
        }
        Ok(Self { _file: file })
    }
}

/// One stage of a PHASED rollout: `percent`% of the auto-update fleet may
/// see the release once `at_unix` passes.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct RolloutStage {
    /// Cumulative percent of the fleet (1-100).
    pub percent: u8,
    /// Unix seconds when this stage opens.
    pub at_unix: u64,
}

/// The slow-rollout plan for one release — the manifest's `latest.slow`
/// field. ON BY DEFAULT: a manifest carrying `release_date` but no `slow`
/// gets [`RolloutPlan::default_ladder`]; only an explicit `"slow": "off"`
/// (or a manifest with neither field) releases to everyone at once.
///
/// Purpose: a cooldown for inspecting Grafana per-version before the new
/// version reaches the whole fleet. Each client draws a persistent cohort
/// bucket ([`UpdateState::rollout_bucket`]); the AUTO-UPDATE path opens
/// stage by stage, and the NOTIFY path (package-managed installs) shows
/// "please update" only once the rollout hits 100%.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RolloutPlan {
    /// No staggering: everyone immediately.
    Immediate,
    /// Stages sorted by time; percent 100 completes the rollout.
    Staged(Vec<RolloutStage>),
}

impl RolloutPlan {
    /// The DEFAULT ladder when a manifest has a `release_date` but no
    /// explicit `slow` config: day 1 → 10%, day 2 → 30%, day 3 → 50%,
    /// day 4 → 100%.
    #[must_use]
    pub fn default_ladder(release_unix: u64) -> Self {
        const DAY: u64 = 86_400;
        Self::Staged(vec![
            RolloutStage {
                percent: 10,
                at_unix: release_unix + DAY,
            },
            RolloutStage {
                percent: 30,
                at_unix: release_unix + 2 * DAY,
            },
            RolloutStage {
                percent: 50,
                at_unix: release_unix + 3 * DAY,
            },
            RolloutStage {
                percent: 100,
                at_unix: release_unix + 4 * DAY,
            },
        ])
    }

    /// Percent of the fleet allowed to update at `now` (0 before the first
    /// stage opens; a plan whose LAST stage is below 100 still completes —
    /// reaching the final stage means "the rollout has run its course").
    #[must_use]
    pub fn allowed_percent(&self, now_unix: u64) -> u8 {
        match self {
            Self::Immediate => 100,
            Self::Staged(stages) => {
                let reached = stages
                    .iter()
                    .filter(|st| st.at_unix <= now_unix)
                    .map(|st| st.percent)
                    .max()
                    .unwrap_or(0);
                let last_open = stages.iter().all(|st| st.at_unix <= now_unix);
                if last_open {
                    100
                } else {
                    reached
                }
            }
        }
    }

    /// Whether the rollout has reached everyone — the gate for the
    /// notify-only "please update" hint on system-installed versions.
    #[must_use]
    pub fn is_complete(&self, now_unix: u64) -> bool {
        self.allowed_percent(now_unix) >= 100
    }
}

/// Which rollout gate applies to this client.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub enum UpdateAudience {
    /// Self-updating install: eligible once its cohort bucket falls under
    /// the currently-open percent.
    AutoUpdate,
    /// Package-managed / notify-only install: sees the notification only
    /// after the rollout completes (100%).
    NotifyOnly,
    /// A tester's install (a beta channel): every release the moment it is
    /// published. The rollout protects the fleet from a bad release; testers
    /// are the ones who find it, so the gate does not apply to them.
    Tester,
}

/// `"2026-08-18"` / `"2026-08-18T12:30:00Z"` / unix seconds (number or
/// numeric string) → unix seconds. Returns `None` for anything else.
#[must_use]
pub fn parse_manifest_datetime(v: &serde_json::Value) -> Option<u64> {
    if let Some(n) = v.as_u64() {
        return Some(n);
    }
    let text = v.as_str()?.trim();
    if let Ok(n) = text.parse::<u64>() {
        return Some(n);
    }
    // YYYY-MM-DD[THH:MM[:SS][Z]]
    let (date, time) = match text.split_once('T') {
        Some((d, t)) => (d, t.trim_end_matches('Z')),
        None => (text, ""),
    };
    let mut parts = date.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: u32 = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // Howard Hinnant's days-from-civil.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = u64::try_from(y - era * 400).ok()?;
    let mp = u64::from((month + 9) % 12);
    let doy = (153 * mp + 2) / 5 + u64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + i64::try_from(doe).ok()? - 719_468;
    let mut secs = u64::try_from(days).ok()? * 86_400;
    if !time.is_empty() {
        let mut hms = time.split(':');
        let h: u64 = hms.next()?.parse().ok()?;
        let m: u64 = hms.next().unwrap_or("0").parse().ok()?;
        let sec: u64 = hms.next().unwrap_or("0").parse().ok()?;
        secs += h * 3600 + m * 60 + sec;
    }
    Some(secs)
}

/// Extracts the rollout plan from `latest`: explicit `"slow": "off"` →
/// immediate; explicit `"slow": {"10": <datetime>, …}` → those stages;
/// absent `slow` → the default ladder from `release_date`, or immediate
/// when there is no `release_date` to ladder from.
#[must_use]
pub fn parse_rollout(latest: &serde_json::Value) -> RolloutPlan {
    let release_date = latest.get("release_date").and_then(parse_manifest_datetime);
    match latest.get("slow") {
        Some(serde_json::Value::String(s)) if s.eq_ignore_ascii_case("off") => {
            RolloutPlan::Immediate
        }
        Some(serde_json::Value::Object(map)) => {
            let mut stages: Vec<RolloutStage> = map
                .iter()
                .filter_map(|(percent, when)| {
                    let percent: u8 = percent
                        .trim()
                        .parse()
                        .ok()
                        .filter(|p| (1..=100).contains(p))?;
                    let at_unix = parse_manifest_datetime(when)?;
                    Some(RolloutStage { percent, at_unix })
                })
                .collect();
            stages.sort_by_key(|st| (st.at_unix, st.percent));
            if stages.is_empty() {
                // A malformed slow-map must not silently ship to everyone.
                release_date.map_or(RolloutPlan::Immediate, RolloutPlan::default_ladder)
            } else {
                RolloutPlan::Staged(stages)
            }
        }
        _ => release_date.map_or(RolloutPlan::Immediate, RolloutPlan::default_ladder),
    }
}

/// Parses the v1 manifest:
///
/// ```json
/// { "latest": { "version": "1.5.0",
///               "download_url": "https://…/app-1.5.0-x86_64.tar.gz",
///               "changelog_md": "https://…/CHANGELOG.md",
///               "digest": "sha256:…" } }
/// ```
///
/// Per-platform manifests are the app's concern (serve a different URL per
/// target, or template `{target}` into the URL before calling).
///
/// # Errors
///
/// Returns a description when the JSON does not parse or `latest.version`
/// is missing.
pub fn parse_manifest(json: &str) -> Result<ReleaseInfo, String> {
    parse_manifest_v1(json).map(|(release, _)| release)
}

/// [`parse_manifest`] plus the release's [`RolloutPlan`] (from
/// `latest.slow` / `latest.release_date`; see [`parse_rollout`]).
///
/// # Errors
///
/// Returns a description when the JSON does not parse or `latest.version`
/// is missing.
pub fn parse_manifest_v1(json: &str) -> Result<(ReleaseInfo, RolloutPlan), String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("manifest parse: {e}"))?;
    let latest = v
        .get("latest")
        .ok_or_else(|| "manifest: missing `latest`".to_owned())?;
    let get = |k: &str| {
        latest
            .get(k)
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_owned()
    };
    let version = get("version");
    if version.is_empty() {
        return Err("manifest: missing `latest.version`".to_owned());
    }
    let release = ReleaseInfo {
        version: version.into(),
        download_url: get("download_url").into(),
        changelog_md_url: get("changelog_md").into(),
        changelog_md_inline: get("changelog_md_inline").into(),
        digest: get("digest").into(),
        signature: get("signature").into(),
        signing_key_statement: get("signing_key_statement").into(),
        signing_key_statement_sig: get("signing_key_statement_sig").into(),
        release_statement: get("release_statement").into(),
        release_statement_sig: get("release_statement_sig").into(),
        download_headers: Vec::new().into(),
    };
    Ok((release, parse_rollout(latest)))
}

// ===========================================================================
// UPDATE SOURCES — one URL in the app config, several kinds of thing behind it
// ===========================================================================

/// What answered at the update URL.
///
/// The app configures ONE URL. Rather than making every deployment stand up
/// a manifest server, the updater looks at what it actually got back and
/// works with it — from a plain text file on static hosting all the way to
/// a full manifest. Lenient about SHAPE, never about VERIFICATION: whatever
/// the source, the digest and signature chain are checked identically.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
#[repr(C)]
pub enum UpdateSourceKind {
    /// Azul manifest v1: `{"latest": {…}}`. The full-featured form —
    /// staged rollout, changelog URL, signature chain.
    #[default]
    ManifestV1,
    /// A flat JSON object: `{"version": …, "download_url": …}`. Same fields,
    /// no `latest` wrapper — what people write by hand on the first day.
    FlatJson,
    /// A GitHub release (the API's release object, or an array of them).
    GitHubRelease,
    /// A container registry (`oci://registry/repo:tag`) — the manifest's
    /// layer digest is the artifact pin.
    OciRegistry,
    /// A bare version string, e.g. a `VERSION` file on static hosting.
    /// There is no artifact URL, so this can only NOTIFY — which is still
    /// worth having: it costs one file and no infrastructure.
    PlainVersion,
}

impl UpdateSourceKind {
    /// Lowercase name, for logs and the `app_update_check_total` label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ManifestV1 => "manifest_v1",
            Self::FlatJson => "flat_json",
            Self::GitHubRelease => "github_release",
            Self::OciRegistry => "oci_registry",
            Self::PlainVersion => "plain_version",
        }
    }
}

/// A release plus the shape it was read from.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedRelease {
    /// The release itself.
    pub release: ReleaseInfo,
    /// Its rollout plan — each release has its OWN, which is why a newer
    /// release cannot hold an older, fully-rolled-out one hostage.
    pub rollout: RolloutPlan,
    /// Which source shape produced it.
    pub kind: UpdateSourceKind,
    /// Files still to fetch before this release can be verified. Only the
    /// candidate actually offered is hydrated, so listing ten releases does
    /// not cost ten signature downloads.
    pub sidecars: Vec<Sidecar>,
}

/// A file the source referenced but did not inline (GitHub keeps signatures
/// and checksums in sibling assets). Fetched after the document is parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sidecar {
    /// Which `ReleaseInfo` field the fetched text fills.
    pub field: SidecarField,
    /// Where to get it.
    pub url: String,
}

/// Which field a [`Sidecar`] fills.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SidecarField {
    /// `ReleaseInfo::signature`.
    Signature,
    /// `ReleaseInfo::signing_key_statement`.
    Statement,
    /// `ReleaseInfo::signing_key_statement_sig`.
    StatementSig,
    /// `ReleaseInfo::release_statement`.
    ReleaseStatement,
    /// `ReleaseInfo::release_statement_sig`.
    ReleaseStatementSig,
    /// `ReleaseInfo::digest`, from a `.sha256` / `SHA256SUMS`-style file.
    Digest,
}

/// An update URL, rewritten to something fetchable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedUrl {
    /// What to GET.
    pub fetch_url: String,
    /// Explicit asset name pattern from `?asset=…`, if the URL carried one.
    /// The placeholders `{version}` / `{target}` in it are substituted.
    pub asset_pattern: Option<String>,
    /// Set when the URL was `oci://…`: a container-registry reference,
    /// which needs the registry's token dance rather than one plain GET.
    pub oci: Option<OciRef>,
}

/// A container-registry reference: `oci://registry/repository[:tag]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OciRef {
    /// Registry host, e.g. `ghcr.io`.
    pub registry: String,
    /// Repository path, e.g. `owner/app`.
    pub repository: String,
    /// Tag or digest; `latest` when the URL gave none.
    pub reference: String,
}

/// Rewrites the configured URL into something fetchable.
///
/// Accepted, all meaning "the latest GitHub release of that repo":
/// `github://owner/repo`, `https://github.com/owner/repo`,
/// `https://github.com/owner/repo/releases[/latest]`, and the API URL
/// itself. A `?asset=` query pins which asset is the artifact; without it
/// the asset is chosen by matching this build's OS and architecture.
///
/// Anything else is returned unchanged — a manifest URL is just a URL.
#[must_use]
pub fn normalize_update_url(url: &str) -> NormalizedUrl {
    let url = url.trim();
    let (base, query) = url.split_once('?').map_or((url, ""), |(b, q)| (b, q));
    let asset_pattern = query
        .split('&')
        .find_map(|kv| kv.strip_prefix("asset="))
        .map(percent_decode);

    let repo = base
        .strip_prefix("github://")
        .or_else(|| base.strip_prefix("https://github.com/"))
        .or_else(|| base.strip_prefix("http://github.com/"))
        .or_else(|| base.strip_prefix("www.github.com/"))
        .map(|rest| {
            // owner/repo, ignoring any /releases/latest tail.
            let mut parts = rest.trim_matches('/').split('/');
            let owner = parts.next().unwrap_or_default();
            let repo = parts.next().unwrap_or_default();
            (owner.to_owned(), repo.to_owned())
        })
        .filter(|(o, r)| !o.is_empty() && !r.is_empty());

    if let Some(rest) = base.strip_prefix("oci://") {
        // registry/repo/path[:tag] — the FIRST segment is the registry, the
        // tag is after the last ':' that follows the last '/'.
        let rest = rest.trim_matches('/');
        let (registry, repository) = rest.split_once('/').unwrap_or((rest, ""));
        let (repository, reference) = match repository.rsplit_once(':') {
            Some((r, t)) if !t.contains('/') => (r, t),
            _ => (repository, "latest"),
        };
        return NormalizedUrl {
            fetch_url: format!("https://{registry}/v2/{repository}/manifests/{reference}"),
            asset_pattern,
            oci: Some(OciRef {
                registry: registry.to_owned(),
                repository: repository.to_owned(),
                reference: reference.to_owned(),
            }),
        };
    }

    match repo {
        Some((owner, repo)) => NormalizedUrl {
            fetch_url: format!("https://api.github.com/repos/{owner}/{repo}/releases/latest"),
            asset_pattern,
            oci: None,
        },
        None => NormalizedUrl {
            fetch_url: base.to_owned(),
            asset_pattern,
            oci: None,
        },
    }
}

/// The `realm` / `service` / `scope` a registry's `WWW-Authenticate: Bearer`
/// challenge asks the client to use when fetching a token.
#[must_use]
pub fn parse_www_authenticate(header: &str) -> Option<(String, String, String)> {
    let rest = header
        .trim()
        .strip_prefix("Bearer ")
        .or_else(|| header.trim().strip_prefix("bearer "))?;
    let mut realm = None;
    let mut service = String::new();
    let mut scope = String::new();
    for part in rest.split(',') {
        let (key, value) = part.trim().split_once('=')?;
        let value = value.trim().trim_matches('"').to_owned();
        match key.trim() {
            "realm" => realm = Some(value),
            "service" => service = value,
            "scope" => scope = value,
            _ => {}
        }
    }
    Some((realm?, service, scope))
}

/// Picks THIS platform's manifest digest out of an OCI image index.
///
/// Returns `None` when the document is a plain manifest (no `manifests`
/// array) or lists nothing for this OS/architecture — a caller that gets
/// `None` should read the document it already has.
#[must_use]
pub fn select_index_manifest(index: &serde_json::Value) -> Option<String> {
    let entries = index.get("manifests")?.as_array()?;
    // OCI platform names differ from Rust's: darwin/amd64, not macos/x86_64.
    let want_os = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    let want_arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "x86" => "386",
        other => other,
    };
    entries
        .iter()
        .find(|m| {
            let platform = m.get("platform");
            let os = platform
                .and_then(|p| p.get("os"))
                .and_then(serde_json::Value::as_str);
            let arch = platform
                .and_then(|p| p.get("architecture"))
                .and_then(serde_json::Value::as_str);
            os == Some(want_os) && arch == Some(want_arch)
        })
        .and_then(|m| m.get("digest"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

/// One OCI image/artifact manifest → a `ReleaseInfo`.
///
/// The version comes from the standard
/// `org.opencontainers.image.version` annotation, falling back to the tag
/// when the reference names one (`:2.0.0`). The artifact is a LAYER blob —
/// selected by media type with `?asset=`, else the first layer — and its
/// `digest` is the pin, so an OCI release is digest-verified by
/// construction.
///
/// # Errors
///
/// Returns a description when the manifest carries no usable version.
pub fn parse_oci_manifest(
    manifest: &serde_json::Value,
    oci: &OciRef,
    layer_selector: Option<&str>,
) -> Result<ResolvedRelease, String> {
    let annotation = |k: &str| {
        manifest
            .get("annotations")
            .and_then(|a| a.get(k))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_owned()
    };

    let version = {
        let annotated = annotation("org.opencontainers.image.version");
        if annotated.is_empty() {
            if oci.reference == "latest" || oci.reference.starts_with("sha256:") {
                return Err(format!(
                    "the OCI manifest for {}:{} has no org.opencontainers.image.version \
                     annotation, and the reference does not name a version either",
                    oci.repository, oci.reference
                ));
            }
            strip_version_prefix(&oci.reference)
        } else {
            strip_version_prefix(&annotated)
        }
    };

    let mut rollout_src = serde_json::Map::new();
    let created = annotation("org.opencontainers.image.created");
    if !created.is_empty() {
        rollout_src.insert(
            "release_date".to_owned(),
            serde_json::Value::String(created),
        );
    }
    let rollout = parse_rollout(&serde_json::Value::Object(rollout_src));

    let empty = Vec::new();
    let layers = manifest
        .get("layers")
        .and_then(serde_json::Value::as_array)
        .unwrap_or(&empty);
    let layer = layer_selector.map_or_else(
        || layers.first(),
        |sel| {
            layers.iter().find(|l| {
                l.get("mediaType")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|m| m.contains(sel))
                    || l.get("annotations")
                        .and_then(|a| a.get("org.opencontainers.image.title"))
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|t| glob_match(sel, t))
            })
        },
    );

    let mut release = ReleaseInfo {
        version: version.into(),
        changelog_md_inline: annotation("org.opencontainers.image.description").into(),
        ..ReleaseInfo::default()
    };
    if let Some(layer) = layer {
        let digest = layer
            .get("digest")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if !digest.is_empty() {
            release.download_url = format!(
                "https://{}/v2/{}/blobs/{digest}",
                oci.registry, oci.repository
            )
            .into();
            // A registry digest IS the artifact's sha256 — the pin comes
            // free, no separate checksum file to publish or trust.
            release.digest = digest.into();
        }
    }

    Ok(ResolvedRelease {
        release,
        rollout,
        kind: UpdateSourceKind::OciRegistry,
        sidecars: Vec::new(),
    })
}

/// Minimal percent-decoding for the `?asset=` value.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b as char);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Version strings are compared numerically, so a `v` prefix has to go.
#[must_use]
pub fn strip_version_prefix(tag: &str) -> String {
    let t = tag.trim();
    t.strip_prefix('v')
        .or_else(|| t.strip_prefix('V'))
        .filter(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
        .unwrap_or(t)
        .to_owned()
}

/// Reads whatever the update URL returned, as a LIST of candidate releases.
///
/// The shape is SNIFFED, not configured: `latest` (and/or `releases`) means
/// an azul manifest, `tag_name` + `assets` means a GitHub release, an array
/// means several of them, a flat object with a `version` means a
/// hand-written manifest, and a body that is just a version number means
/// notify-only. A body that is none of those is an error naming what it
/// looked like — silently treating an HTML error page as "no update" would
/// hide a broken deployment forever.
///
/// CHANNELS: a manifest may carry `channels: {"stable": …, "beta": …}`, and
/// the client reads the one its binary was built to follow
/// (`AppConfig.updates.channel`, empty = `stable`). Falls back to
/// `latest`/`releases` when the manifest has no such map. For GitHub, a
/// non-stable channel is what includes PRERELEASES.
///
/// Several candidates come back where the source offers them, because the
/// gate must be able to pick the newest release this client is ELIGIBLE
/// for — see [`select_update_candidate`].
///
/// # Errors
///
/// Returns a description when the body matches no known shape.
pub fn parse_release_document(
    body: &str,
    asset_pattern: Option<&str>,
    channel: &str,
) -> Result<Vec<ResolvedRelease>, String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Err("the update URL returned an empty body".to_owned());
    }
    let stable = channel.trim().is_empty() || channel.eq_ignore_ascii_case("stable");

    let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
        // Not JSON. A bare version number is a legitimate, minimal source;
        // anything else is a mistake worth naming.
        let first = trimmed.lines().next().unwrap_or_default().trim();
        let version = strip_version_prefix(first);
        if !version.is_empty()
            && version.starts_with(|c: char| c.is_ascii_digit())
            && version.len() <= 64
            && version
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+' | '_'))
        {
            return Ok(vec![ResolvedRelease {
                release: ReleaseInfo {
                    version: version.into(),
                    ..ReleaseInfo::default()
                },
                rollout: RolloutPlan::Immediate,
                kind: UpdateSourceKind::PlainVersion,
                sidecars: Vec::new(),
            }]);
        }
        return Err(format!(
            "the update URL returned neither JSON nor a version number (it starts with {:?})",
            // Characters, not bytes: a body that has a multi-byte character at the 40th byte
            // panics on a slice, and with `panic = "abort"` that ends the app on every check.
            trimmed.chars().take(40).collect::<String>()
        ));
    };

    // A GitHub /releases list. On the stable channel prereleases and drafts
    // are skipped; on any other channel prereleases ARE the point.
    if let Some(array) = value.as_array() {
        let mut out = Vec::new();
        for entry in array {
            if entry.get("draft") == Some(&serde_json::Value::Bool(true)) {
                continue;
            }
            if stable && entry.get("prerelease") == Some(&serde_json::Value::Bool(true)) {
                continue;
            }
            if let Ok(r) = parse_github_release(entry, asset_pattern) {
                out.push(r);
            }
        }
        if out.is_empty() {
            return Err(
                "the releases list has no entry for this channel (all drafts or prereleases?)"
                    .to_owned(),
            );
        }
        return Ok(out);
    }

    // A manifest: an explicit channel map wins over `latest`/`releases`.
    let channel_key = if stable { "stable" } else { channel.trim() };
    if let Some(entry) = value.get("channels").and_then(|c| c.get(channel_key)) {
        let mut out = Vec::new();
        match entry {
            serde_json::Value::Array(list) => {
                for item in list {
                    out.push(manifest_entry_to_release(item));
                }
            }
            item => out.push(manifest_entry_to_release(item)),
        }
        if !out.is_empty() {
            return Ok(out);
        }
    }

    if value.get("latest").is_some() || value.get("releases").is_some() {
        let mut out = Vec::new();
        if let Some(latest) = value.get("latest") {
            out.push(manifest_entry_to_release(latest));
        }
        if let Some(list) = value.get("releases").and_then(serde_json::Value::as_array) {
            for item in list {
                out.push(manifest_entry_to_release(item));
            }
        }
        if out.iter().all(|r| r.release.version.as_str().is_empty()) {
            return Err("manifest: missing `latest.version`".to_owned());
        }
        out.retain(|r| !r.release.version.as_str().is_empty());
        return Ok(out);
    }

    if value.get("tag_name").is_some() || value.get("assets").is_some() {
        return Ok(vec![parse_github_release(&value, asset_pattern)?]);
    }

    // A flat, hand-written object.
    if value
        .get("version")
        .and_then(serde_json::Value::as_str)
        .is_some()
    {
        let mut resolved = manifest_entry_to_release(&value);
        resolved.kind = UpdateSourceKind::FlatJson;
        return Ok(vec![resolved]);
    }

    Err(
        "the update URL returned JSON with no `latest`, `channels`, `tag_name` or `version`"
            .to_owned(),
    )
}

/// One manifest entry (the object under `latest`, inside `releases`, or
/// under a channel) → a candidate.
fn manifest_entry_to_release(entry: &serde_json::Value) -> ResolvedRelease {
    let get = |k: &str| {
        entry
            .get(k)
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_owned()
    };
    let download = if get("download_url").is_empty() {
        get("url")
    } else {
        get("download_url")
    };
    ResolvedRelease {
        release: ReleaseInfo {
            version: strip_version_prefix(&get("version")).into(),
            download_url: download.into(),
            changelog_md_url: get("changelog_md").into(),
            changelog_md_inline: get("changelog_md_inline").into(),
            digest: get("digest").into(),
            signature: get("signature").into(),
            signing_key_statement: get("signing_key_statement").into(),
            signing_key_statement_sig: get("signing_key_statement_sig").into(),
            release_statement: get("release_statement").into(),
            release_statement_sig: get("release_statement_sig").into(),
            download_headers: Vec::new().into(),
        },
        rollout: parse_rollout(entry),
        kind: UpdateSourceKind::ManifestV1,
        sidecars: Vec::new(),
    }
}

/// One GitHub release object → a `ReleaseInfo` plus the sibling assets that
/// still have to be fetched.
fn parse_github_release(
    release: &serde_json::Value,
    asset_pattern: Option<&str>,
) -> Result<ResolvedRelease, String> {
    let tag = release
        .get("tag_name")
        .and_then(serde_json::Value::as_str)
        .ok_or("GitHub release has no tag_name")?;
    let version = strip_version_prefix(tag);
    let empty = Vec::new();
    let assets = release
        .get("assets")
        .and_then(serde_json::Value::as_array)
        .unwrap_or(&empty);

    // `published_at` gives the rollout ladder its start date for free.
    let mut rollout_src = serde_json::Map::new();
    if let Some(published) = release.get("published_at") {
        rollout_src.insert("release_date".to_owned(), published.clone());
    }
    let rollout = parse_rollout(&serde_json::Value::Object(rollout_src));

    let mut info = ReleaseInfo {
        version: version.clone().into(),
        // The release body IS the changelog, already Markdown — no URL to
        // fetch. `changelog_md_url` stays empty; the dialog falls back to
        // `AppConfig.changelog_md` when there is no inline text.
        ..ReleaseInfo::default()
    };
    if let Some(body) = release.get("body").and_then(serde_json::Value::as_str) {
        info.changelog_md_inline = body.into();
    }

    // The placeholders are assembled rather than written literally: a bare
    // "{version}" in source reads as a formatting argument to clippy.
    let version_ph = concat!('{', "version", '}');
    let target_ph = concat!('{', "target", '}');
    let pattern = asset_pattern.map(|p| {
        p.replace(version_ph, &version)
            .replace(target_ph, std::env::consts::ARCH)
    });
    let Some(artifact) = select_github_asset(assets, pattern.as_deref()) else {
        // Nothing here matches this platform. Report the version anyway:
        // "there is a newer release" is true and useful, and refusing to
        // guess which binary is right is the honest half.
        return Ok(ResolvedRelease {
            release: info,
            rollout,
            kind: UpdateSourceKind::GitHubRelease,
            sidecars: Vec::new(),
        });
    };

    let artifact_name = asset_str(artifact, "name");
    info.download_url = asset_str(artifact, "browser_download_url").into();

    // GitHub populates `digest` for newer uploads and leaves it null on
    // older ones — fall back to a sibling checksum file.
    let mut sidecars = Vec::new();
    match artifact.get("digest").and_then(serde_json::Value::as_str) {
        Some(d) if !d.is_empty() => info.digest = d.into(),
        _ => {
            if let Some(sum) = find_asset(assets, |n| {
                n == format!("{artifact_name}.sha256")
                    || n.eq_ignore_ascii_case("sha256sums")
                    || n.eq_ignore_ascii_case("sha256.sum")
            }) {
                sidecars.push(Sidecar {
                    field: SidecarField::Digest,
                    url: asset_str(sum, "browser_download_url"),
                });
            }
        }
    }

    // The signature chain rides as sibling assets.
    if let Some(sig) = find_asset(assets, |n| {
        n == format!("{artifact_name}.minisig") || n == format!("{artifact_name}.sig")
    }) {
        sidecars.push(Sidecar {
            field: SidecarField::Signature,
            url: asset_str(sig, "browser_download_url"),
        });
    }
    if let Some(st) = find_asset(assets, |n| {
        n == "signing-key-statement.txt" || n == "signing-key-statement"
    }) {
        sidecars.push(Sidecar {
            field: SidecarField::Statement,
            url: asset_str(st, "browser_download_url"),
        });
    }
    if let Some(sts) = find_asset(assets, |n| {
        n == "signing-key-statement.txt.minisig" || n == "signing-key-statement.minisig"
    }) {
        sidecars.push(Sidecar {
            field: SidecarField::StatementSig,
            url: asset_str(sts, "browser_download_url"),
        });
    }
    // The release statement names the platform and the hash of ONE artifact, so a release with an
    // asset per platform carries one statement per asset (`<asset>.release-statement.txt`); the
    // shared names are for a release that has a single artifact.
    if let Some(rs) = find_asset(assets, |n| {
        n == format!("{artifact_name}.release-statement.txt")
    })
    .or_else(|| {
        find_asset(assets, |n| {
            n == "release-statement.txt" || n == "release-statement"
        })
    }) {
        sidecars.push(Sidecar {
            field: SidecarField::ReleaseStatement,
            url: asset_str(rs, "browser_download_url"),
        });
    }
    if let Some(rss) = find_asset(assets, |n| {
        n == format!("{artifact_name}.release-statement.txt.minisig")
    })
    .or_else(|| {
        find_asset(assets, |n| {
            n == "release-statement.txt.minisig" || n == "release-statement.minisig"
        })
    }) {
        sidecars.push(Sidecar {
            field: SidecarField::ReleaseStatementSig,
            url: asset_str(rss, "browser_download_url"),
        });
    }

    Ok(ResolvedRelease {
        release: info,
        rollout,
        kind: UpdateSourceKind::GitHubRelease,
        sidecars,
    })
}

fn asset_str(asset: &serde_json::Value, key: &str) -> String {
    asset
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_owned()
}

fn find_asset(
    assets: &[serde_json::Value],
    pred: impl Fn(&str) -> bool,
) -> Option<&serde_json::Value> {
    assets.iter().find(|a| pred(&asset_str(a, "name")))
}

/// Names that are never the artifact: signatures, checksums, metadata.
fn is_sidecar_name(name: &str) -> bool {
    const SIDECAR_SUFFIXES: &[&str] = &[
        ".minisig", ".sig", ".asc", ".pem", ".sha256", ".sha512", ".sum", ".txt", ".json",
    ];
    let lower = name.to_ascii_lowercase();
    SIDECAR_SUFFIXES
        .iter()
        .any(|suffix| lower.ends_with(suffix))
        || lower.contains("checksum")
        || lower.contains("sha256sums")
}

/// Picks the artifact for THIS build.
///
/// An explicit `?asset=` pattern wins (exact name, or a `*` glob). Otherwise
/// assets are scored by whether their name carries this platform's OS and
/// architecture; a name matching neither is never chosen, because shipping
/// the user a random binary is worse than telling them to download it
/// themselves.
#[must_use]
pub fn select_github_asset<'a>(
    assets: &'a [serde_json::Value],
    pattern: Option<&str>,
) -> Option<&'a serde_json::Value> {
    if let Some(pat) = pattern {
        return assets
            .iter()
            .find(|a| glob_match(pat, &asset_str(a, "name")));
    }
    let os_tokens: &[&str] = match std::env::consts::OS {
        "linux" => &["linux"],
        "macos" => &["macos", "darwin", "apple", "osx"],
        "windows" => &["windows", "win"],
        "android" => &["android"],
        "ios" => &["ios"],
        other => {
            return assets.iter().find(|a| {
                let n = asset_str(a, "name").to_ascii_lowercase();
                !is_sidecar_name(&n) && n.contains(other)
            })
        }
    };
    let arch_tokens: &[&str] = match std::env::consts::ARCH {
        "x86_64" => &["x86_64", "amd64", "x64"],
        "aarch64" => &["aarch64", "arm64"],
        "x86" => &["i686", "i386", "x86"],
        "arm" => &["armv7", "armhf", "arm"],
        _ => &[],
    };

    let mut best: Option<(u8, &serde_json::Value)> = None;
    for asset in assets {
        let name = asset_str(asset, "name");
        if name.is_empty() || is_sidecar_name(&name) {
            continue;
        }
        let lower = name.to_ascii_lowercase();
        let mut score = 0u8;
        if os_tokens.iter().any(|t| lower.contains(t)) {
            score += 2;
        }
        if arch_tokens.iter().any(|t| lower.contains(t)) {
            score += 2;
        }
        if score == 0 {
            continue;
        }
        if best.is_none_or(|(b, _)| score > b) {
            best = Some((score, asset));
        }
    }
    best.map(|(_, a)| a)
}

/// `*` glob, enough for asset names.
fn glob_match(pattern: &str, name: &str) -> bool {
    if !pattern.contains('*') {
        return pattern == name;
    }
    let mut rest = name;
    let mut parts = pattern.split('*');
    if let Some(first) = parts.next() {
        if !rest.starts_with(first) {
            return false;
        }
        rest = &rest[first.len()..];
    }
    let parts: Vec<&str> = parts.collect();
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i + 1 == parts.len() && !pattern.ends_with('*') {
            return rest.ends_with(part);
        }
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    true
}

/// Media types a registry may answer a manifest request with.
const OCI_MANIFEST_ACCEPT: &str = "application/vnd.oci.image.manifest.v1+json, \
                                   application/vnd.docker.distribution.manifest.v2+json, \
                                   application/vnd.oci.image.index.v1+json";

/// Resolves a release from a container registry, doing the registry's
/// anonymous token dance when it asks for one.
///
/// Registries answer an unauthenticated manifest request with `401` and a
/// `WWW-Authenticate: Bearer realm=…` challenge; the client fetches a
/// (usually anonymous) pull token from that realm and retries. The same
/// token is handed to the DOWNLOAD through `ReleaseInfo::download_headers`,
/// because a blob request needs it too.
#[cfg(feature = "http")]
fn resolve_oci(
    manifest_url: &str,
    oci: &OciRef,
    layer_selector: Option<&str>,
) -> Result<ResolvedRelease, String> {
    let base = crate::http::HttpRequestConfig::new().with_header("Accept", OCI_MANIFEST_ACCEPT);
    let mut response = crate::http::http_get_with_config(manifest_url, &base)
        .map_err(|e| format!("registry manifest fetch: {e:?}"))?;

    let mut token = String::new();
    if response.status_code == 401 {
        let challenge = response
            .headers
            .as_ref()
            .iter()
            .find(|h| h.name.as_str().eq_ignore_ascii_case("www-authenticate"))
            .map(|h| h.value.as_str().to_owned())
            .ok_or("registry returned 401 with no WWW-Authenticate challenge")?;
        let (realm, service, scope) = parse_www_authenticate(&challenge)
            .ok_or_else(|| format!("cannot read the registry challenge: {challenge}"))?;
        let scope = if scope.is_empty() {
            format!("repository:{}:pull", oci.repository)
        } else {
            scope
        };
        let token_url = format!("{realm}?service={service}&scope={scope}");
        let token_response = crate::http::http_get_with_config(&token_url, &base)
            .map_err(|e| format!("registry token fetch: {e:?}"))?;
        let token_json: serde_json::Value = serde_json::from_slice(token_response.body.as_ref())
            .map_err(|e| format!("registry token is not JSON: {e}"))?;
        token.clear();
        token.push_str(
            token_json
                .get("token")
                .or_else(|| token_json.get("access_token"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default(),
        );
        if token.is_empty() {
            return Err("the registry issued no pull token".to_owned());
        }
        let authed = base
            .clone()
            .with_header("Authorization", format!("Bearer {token}"));
        response = crate::http::http_get_with_config(manifest_url, &authed)
            .map_err(|e| format!("registry manifest fetch (authenticated): {e:?}"))?;
    }

    if !(200..300).contains(&response.status_code) {
        return Err(format!(
            "registry manifest HTTP {} for {}",
            response.status_code, manifest_url
        ));
    }
    let mut manifest: serde_json::Value = serde_json::from_slice(response.body.as_ref())
        .map_err(|e| format!("registry manifest is not JSON: {e}"))?;

    // A multi-arch INDEX lists per-platform manifests instead of layers;
    // follow the entry for this build before looking for an artifact.
    if let Some(digest) = select_index_manifest(&manifest) {
        let child_url = format!(
            "https://{}/v2/{}/manifests/{digest}",
            oci.registry, oci.repository
        );
        // `base` is not needed after this point, so it moves.
        let child_config = if token.is_empty() {
            base
        } else {
            base.with_header("Authorization", format!("Bearer {token}"))
        };
        let child = crate::http::http_get_with_config(&child_url, &child_config)
            .map_err(|e| format!("registry child-manifest fetch: {e:?}"))?;
        if (200..300).contains(&child.status_code) {
            manifest = serde_json::from_slice(child.body.as_ref())
                .map_err(|e| format!("registry child manifest is not JSON: {e}"))?;
        }
    }

    let mut resolved = parse_oci_manifest(&manifest, oci, layer_selector)?;
    if !token.is_empty() {
        resolved.release.download_headers = vec![azul_core::window::AzStringPair {
            key: "Authorization".into(),
            value: format!("Bearer {token}").into(),
        }]
        .into();
    }
    Ok(resolved)
}

/// Fetches the update URL and reads every candidate release the source
/// offers, WITHOUT fetching their sidecars — hydrate the one you offer with
/// [`hydrate_sidecars`].
///
/// # Errors
///
/// Returns a description on transport failure or an unreadable body.
#[cfg(feature = "http")]
pub fn resolve_release_candidates(
    url: &str,
    channel: &str,
) -> Result<Vec<ResolvedRelease>, String> {
    let normalized = normalize_update_url(url);
    if let Some(oci) = &normalized.oci {
        return resolve_oci(
            &normalized.fetch_url,
            oci,
            normalized.asset_pattern.as_deref(),
        )
        .map(|r| vec![r]);
    }
    let config =
        crate::http::HttpRequestConfig::new().with_header("Accept", "application/vnd.github+json");
    let response = crate::http::http_get_with_config(&normalized.fetch_url, &config)
        .map_err(|e| format!("update source fetch: {e:?}"))?;
    if !(200..300).contains(&response.status_code) {
        return Err(format!(
            "update source HTTP {} from {}",
            response.status_code, normalized.fetch_url
        ));
    }
    let body = String::from_utf8_lossy(response.body.as_ref()).into_owned();
    parse_release_document(&body, normalized.asset_pattern.as_deref(), channel)
}

/// Fetches the sidecar files a candidate referenced (signatures, a checksum
/// file) and folds them into its `ReleaseInfo`.
///
/// A sidecar that cannot be fetched is NOT fatal — it leaves the field
/// empty, and an app that pins a root key then refuses the release as
/// unsigned, which is the correct outcome.
#[cfg(feature = "http")]
pub fn hydrate_sidecars(resolved: &mut ResolvedRelease) {
    let config =
        crate::http::HttpRequestConfig::new().with_header("Accept", "application/vnd.github+json");
    for sidecar in core::mem::take(&mut resolved.sidecars) {
        let Ok(r) = crate::http::http_get_with_config(&sidecar.url, &config) else {
            continue;
        };
        if !(200..300).contains(&r.status_code) {
            continue;
        }
        let text = String::from_utf8_lossy(r.body.as_ref()).into_owned();
        match sidecar.field {
            SidecarField::Signature => resolved.release.signature = text.trim().into(),
            SidecarField::Statement => {
                // The statement is signed as exact bytes; the file may end
                // with the newline an editor added.
                resolved.release.signing_key_statement = text.trim_end_matches(['\n', '\r']).into();
            }
            SidecarField::StatementSig => {
                resolved.release.signing_key_statement_sig = text.trim().into();
            }
            SidecarField::ReleaseStatement => {
                // Signed as exact bytes, like the key statement: no editor newline.
                resolved.release.release_statement = text.trim_end_matches(['\n', '\r']).into();
            }
            SidecarField::ReleaseStatementSig => {
                resolved.release.release_statement_sig = text.trim().into();
            }
            SidecarField::Digest => {
                if let Some(hex) =
                    parse_checksum_file(&text, resolved.release.download_url.as_str())
                {
                    resolved.release.digest = format!("sha256:{hex}").into();
                }
            }
        }
    }
}

/// The newest release this client may actually install right now.
///
/// Every candidate carries its OWN rollout plan, and this walks them from
/// newest down, returning the first that is BOTH newer than what is
/// installed and open to this client's cohort.
///
/// That walk is the point. With a single-release manifest, publishing 2.1.0
/// while 2.0.0 was mid-rollout RESET the gate: a client whose bucket had
/// already opened for 2.0.0 saw only 2.1.0, was gated out of it, and sat on
/// the old version until the new ladder caught up. Now it takes 2.0.0 —
/// the newest thing it is allowed to have — and moves to 2.1.0 when that
/// opens.
///
/// Returns `None` when nothing is newer, or when everything newer is still
/// gated (the caller reports that as `staggered`, not as an error).
#[must_use]
pub fn select_update_candidate<'a>(
    candidates: &'a [ResolvedRelease],
    current_version: &str,
    bucket: u8,
    audience: UpdateAudience,
    now_unix: u64,
) -> Option<&'a ResolvedRelease> {
    use core::cmp::Ordering;
    let mut best: Option<&ResolvedRelease> = None;
    for candidate in candidates {
        let version = candidate.release.version.as_str();
        if version.is_empty() || compare_versions(version, current_version) != Ordering::Greater {
            continue;
        }
        let eligible = match audience {
            UpdateAudience::AutoUpdate => bucket < candidate.rollout.allowed_percent(now_unix),
            UpdateAudience::NotifyOnly => candidate.rollout.is_complete(now_unix),
            UpdateAudience::Tester => true,
        };
        if !eligible {
            continue;
        }
        if best.is_none_or(|b| {
            compare_versions(version, b.release.version.as_str()) == Ordering::Greater
        }) {
            best = Some(candidate);
        }
    }
    best
}

/// Fetches the update URL and returns the newest candidate, sidecars
/// hydrated — the simple path for callers that do not run the rollout gate
/// themselves.
///
/// # Errors
///
/// Returns a description on transport failure or an unreadable body.
#[cfg(feature = "http")]
pub fn resolve_release(url: &str, channel: &str) -> Result<ResolvedRelease, String> {
    let candidates = resolve_release_candidates(url, channel)?;
    let mut newest = candidates
        .into_iter()
        .reduce(|a, b| {
            if compare_versions(b.release.version.as_str(), a.release.version.as_str())
                == core::cmp::Ordering::Greater
            {
                b
            } else {
                a
            }
        })
        .ok_or("the update source offered no releases")?;
    hydrate_sidecars(&mut newest);
    Ok(newest)
}

/// Pulls this artifact's hash out of a `.sha256` file (bare hex) or a
/// `SHA256SUMS`-style listing (`<hex>  <name>` per line).
#[must_use]
pub fn parse_checksum_file(text: &str, download_url: &str) -> Option<String> {
    let artifact = download_url.rsplit('/').next().unwrap_or_default();
    let is_hex = |s: &str| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit());
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if is_hex(line) {
            return Some(line.to_ascii_lowercase());
        }
        let mut parts = line.split_whitespace();
        let (Some(hex), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        let name = name.trim_start_matches('*');
        if is_hex(hex) && (name == artifact || artifact.is_empty()) {
            return Some(hex.to_ascii_lowercase());
        }
    }
    None
}

/// BLOCKING check against a manifest URL — run it on an azul `Thread`
/// (`CallbackInfo::check_for_updates` does exactly that), never on the UI
/// thread. Applies the anti-downgrade high-water mark, the suspension
/// window AND the release's slow-rollout gate (the client's persistent
/// cohort bucket vs the currently-open stage; notify-only audiences wait
/// for 100%), updates `state`'s bookkeeping, and observes itself through
/// `app_update_check_total{result}` — a gated client records `staggered`.
#[cfg(feature = "http")]
pub fn check_for_updates_blocking(
    manifest_url: &str,
    current_version: &str,
    channel: &str,
    state: &mut UpdateState,
    audience: UpdateAudience,
) -> UpdateCheckResult {
    use core::cmp::Ordering;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    state.last_check_unix = now;

    // ANY supported source shape: azul manifest (optionally per-channel),
    // flat JSON, GitHub releases, an OCI registry, or a bare version file.
    let candidates = match resolve_release_candidates(manifest_url, channel) {
        Ok(c) if !c.is_empty() => c,
        Ok(_) => {
            record_check("error");
            return UpdateCheckResult::Error("the update source offered no releases".into());
        }
        Err(e) => {
            record_check("error");
            return UpdateCheckResult::Error(e.into());
        }
    };

    // ANTI-DOWNGRADE, applied to the SOURCE rather than to one release: the
    // attack it stops is a manifest that regresses wholesale to an old,
    // vulnerable version. Individual candidates below the high-water mark
    // are fine — that is exactly how a client takes the newest release it
    // is eligible for while a newer one is still rolling out. The mark is the
    // highest version of a release that was VERIFIED.
    let highest_offered = candidates
        .iter()
        .map(|c| c.release.version.as_str())
        .max_by(|a, b| compare_versions(a, b))
        .unwrap_or_default()
        .to_owned();
    if !state.highest_seen.is_empty()
        && compare_versions(&highest_offered, &state.highest_seen) == Ordering::Less
    {
        record_check("downgrade_refused");
        return UpdateCheckResult::UpToDate;
    }
    // The mark does NOT move here. This manifest is unauthenticated - it can say "999.0.0" - and
    // a mark set from it would refuse every genuine release afterwards (REVIEW Y3, X-AR5b-26).
    // It moves in `verify_release_signature`, from a release statement the signing key signed.

    if state.suspended_until_unix > now {
        record_check("suspended");
        return UpdateCheckResult::UpToDate;
    }

    if compare_versions(&highest_offered, current_version) != Ordering::Greater {
        record_check("up_to_date");
        return UpdateCheckResult::UpToDate;
    }

    // SLOW-ROLLOUT gate, walked over EVERY candidate: take the newest
    // release this client's cohort is open for. Auto-updaters compare
    // their persistent bucket against each release's own stage; notify-only
    // installs wait for 100%. When something newer exists but none of it is
    // open yet, the client reports UpToDate — from the app's point of view
    // those releases do not exist for it YET.
    let bucket = state.rollout_bucket();
    let Some(chosen) = select_update_candidate(&candidates, current_version, bucket, audience, now)
    else {
        record_check("staggered");
        return UpdateCheckResult::UpToDate;
    };
    let mut chosen = chosen.clone();
    // Only the release actually offered pays for its signature files.
    hydrate_sidecars(&mut chosen);
    record_check("available");
    UpdateCheckResult::Available(chosen.release)
}

// const only without the telemetry feature; the metrics call is not const.
#[allow(clippy::missing_const_for_fn)]
fn record_check(result: &str) {
    #[cfg(feature = "telemetry")]
    crate::telemetry::record_update_check(result);
    #[cfg(not(feature = "telemetry"))]
    let _ = result;
}

/// Reads `path` once, block by block, through `each`: the bytes a check looks at are the
/// bytes it counts, and no artifact is held in memory whole. The number of bytes read.
fn read_blocks(path: &Path, mut each: impl FnMut(&[u8])) -> Result<u64, String> {
    use std::io::Read as _;
    let mut file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut block = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = file
            .read(&mut block)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if n == 0 {
            return Ok(total);
        }
        each(&block[..n]);
        total += n as u64;
    }
}

/// The SHA-256 of the file at `path`.
fn sha256_of_file(path: &Path) -> Result<[u8; 32], String> {
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    read_blocks(path, |block| hasher.update(block))?;
    let mut out = [0u8; 32];
    out.copy_from_slice(&hasher.finalize());
    Ok(out)
}

/// `hash` as 64 lowercase hex digits.
fn sha256_hex(hash: &[u8; 32]) -> String {
    use core::fmt::Write as _;
    hash.iter().fold(String::with_capacity(64), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// 64 hex digits (any case) as the 32 bytes they spell.
fn parse_sha256_hex(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 || !hex.is_ascii() {
        return None;
    }
    let mut out = [0u8; 32];
    for (slot, pair) in out.iter_mut().zip(hex.as_bytes().chunks(2)) {
        let high = char::from(pair[0]).to_digit(16)?;
        let low = char::from(pair[1]).to_digit(16)?;
        *slot = u8::try_from(high * 16 + low).ok()?;
    }
    Some(out)
}

/// A manifest's `digest` as 32 bytes: `sha256:<hex>` or bare hex. An EMPTY one is an error here,
/// never a pass (see [`verify_digest`]).
fn parse_digest(digest: &str) -> Result<[u8; 32], String> {
    let digest = digest.trim();
    if digest.is_empty() {
        return Err("no digest is pinned (the manifest's `digest` is empty)".to_owned());
    }
    parse_sha256_hex(digest.strip_prefix("sha256:").unwrap_or(digest))
        .ok_or_else(|| format!("unsupported digest format: {digest:?} (expected sha256 hex)"))
}

/// The digest a release pins: `None` when its `digest` is empty (it pins nothing), an error
/// when what it holds is no sha256.
fn pinned_digest(release: &ReleaseInfo) -> Result<Option<[u8; 32]>, String> {
    let digest = release.digest.as_str().trim();
    if digest.is_empty() {
        return Ok(None);
    }
    parse_digest(digest).map(Some)
}

/// The "digest mismatch" error, naming both hashes.
fn digest_mismatch(expected: &[u8; 32], actual: &[u8; 32]) -> String {
    format!(
        "digest mismatch: manifest pinned sha256:{}, downloaded file is sha256:{}",
        sha256_hex(expected),
        sha256_hex(actual)
    )
}

/// Verifies a staged artifact against the manifest's `digest` field.
///
/// Accepted forms: `sha256:<hex>` or bare hex (interpreted as SHA-256). An EMPTY digest is an
/// ERROR: it pins nothing, and a check that passes when there is nothing to check is how an
/// unsigned update got installed (REVIEW Y1, X-AR5b-25). A caller whose chain is signed asks
/// only when the manifest pins a digest; a digest-only policy refuses a release that pins none.
/// A digest that does not match is a hard error and the caller must DISCARD the file: a
/// mismatch is corruption or tampering, never something to install.
///
/// # Errors
///
/// Returns a description on IO failure, an empty or unsupported digest, or a mismatch (the
/// message names both hashes).
pub fn verify_digest(path: &Path, digest: &str) -> Result<(), String> {
    let expected = parse_digest(digest)?;
    let actual = sha256_of_file(path).map_err(|e| format!("digest read: {e}"))?;
    if actual == expected {
        Ok(())
    } else {
        Err(digest_mismatch(&expected, &actual))
    }
}

// ===========================================================================
// THE UPDATE POLICY - what an update has to prove before it is staged or installed
// ===========================================================================

/// What a build says when it carries no update root key and no `allow_unsigned`: updates are
/// not available. The text the UIs show.
pub const NO_ROOT_KEY: &str = "updates are not available in this build: it carries no update \
                               root key (AppConfig.updates.root_public_key)";

/// A minisign ROOT public key: the one an update's signature chain has to end in.
///
/// There is no `Default` and no empty value: an empty or unreadable text is an error, never "no
/// check". The state "unarmed" used to be an empty string that every layer between the config
/// and the verifier could produce by accident (REVIEW Y1, X-AR5b-25); it cannot be written
/// as a `RootKey`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootKey(minisign_verify::PublicKey);

impl RootKey {
    /// Parses a base64 minisign public key (the second line of a `.pub` file).
    ///
    /// # Errors
    ///
    /// An empty text, or one that is no minisign public key.
    pub fn from_base64(text: &str) -> Result<Self, String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("the update root key is empty".to_owned());
        }
        minisign_verify::PublicKey::from_base64(text)
            .map(Self)
            .map_err(|e| format!("update root key: {e}"))
    }
}

/// The entries of a root-key text: base64 keys separated by commas, semicolons or white space.
fn key_entries(text: &str) -> impl Iterator<Item = &str> {
    text.split([',', ';', ' ', '\t', '\r', '\n'])
        .filter(|entry| !entry.trim().is_empty())
}

/// The root keys an app trusts: the current one and, while a rotation is under way, the next
/// (a new root can ship in a release before the old one is retired - no flag day). At least
/// one; the statement of a signing key has to be signed by any of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootKeys(Vec<RootKey>);

impl RootKeys {
    /// Parses one or more base64 keys, separated by commas, semicolons or white space.
    ///
    /// # Errors
    ///
    /// An empty list, or any entry that is no minisign public key: a typo in the second key
    /// does not leave the first one standing alone.
    pub fn from_text(text: &str) -> Result<Self, String> {
        let keys = key_entries(text)
            .map(RootKey::from_base64)
            .collect::<Result<Vec<_>, _>>()?;
        if keys.is_empty() {
            return Err("the update root key is empty".to_owned());
        }
        Ok(Self(keys))
    }

    /// How many keys.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    /// Never empty; present for the usual pair with [`RootKeys::len`].
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether any of the keys signed `message` as `signature`; the last key's reason if none.
    fn verify(&self, message: &[u8], signature: &minisign_verify::Signature) -> Result<(), String> {
        let mut last = String::new();
        for key in &self.0 {
            match key.0.verify(message, signature, false) {
                Ok(()) => return Ok(()),
                Err(e) => last = e.to_string(),
            }
        }
        Err(last)
    }
}

/// What an update has to prove.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdatePolicy {
    /// The whole chain: a root-signed signing-key statement and a signature of the artifact by
    /// the key it names. What a shipped app uses.
    Signed(RootKeys),
    /// The manifest's sha256 pin only: it protects against corruption and a swapped file, not
    /// against whoever can write the manifest. An app that wants it says so
    /// (`AppConfig.updates.allow_unsigned`); a UI then says "unsigned".
    DigestOnly,
}

impl UpdatePolicy {
    /// The policy a configuration spells: the root key(s) as text and the app's explicit
    /// `allow_unsigned`. A key wins (`allow_unsigned` never lowers a chain that is configured);
    /// a key that does not parse is an error even when `allow_unsigned` is set - a typo in the
    /// key must not turn into "no check"; no key and no `allow_unsigned` is [`NO_ROOT_KEY`].
    ///
    /// # Errors
    ///
    /// [`NO_ROOT_KEY`], or why the key text is no key.
    pub fn from_text(root_public_key: &str, allow_unsigned: bool) -> Result<Self, String> {
        if key_entries(root_public_key).next().is_none() {
            return if allow_unsigned {
                Ok(Self::DigestOnly)
            } else {
                Err(NO_ROOT_KEY.to_owned())
            };
        }
        RootKeys::from_text(root_public_key).map(Self::Signed)
    }

    /// Whether this policy checks a signature chain.
    #[must_use]
    pub const fn is_signed(&self) -> bool {
        matches!(self, Self::Signed(_))
    }
}

/// A staged update that PASSED its checks, and the hash of the exact bytes that passed.
///
/// [`download_and_verify`] and [`verify_staged_program_update_with`] are the only makers, so
/// [`apply_update`] cannot be handed a file nobody looked at. The hash is taken in the same
/// pass that verified the signature, and [`apply_update`] hashes the COPY it installs: a file
/// swapped between the check and the install is caught in the bytes about to become the
/// program (REVIEW Y6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedArtifact {
    path: PathBuf,
    sha256: [u8; 32],
    version: String,
    signed: bool,
    download: Option<DownloadOutcome>,
}

impl VerifiedArtifact {
    /// The staged file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The version the release said.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The SHA-256 of the bytes that passed, as 64 hex digits.
    #[must_use]
    pub fn sha256_hex(&self) -> String {
        sha256_hex(&self.sha256)
    }

    /// Whether a signature chain was checked (`false`: the digest pin only).
    #[must_use]
    pub const fn is_signed(&self) -> bool {
        self.signed
    }

    /// What the download did, when this artifact was downloaded by the call that verified it.
    #[must_use]
    pub const fn download(&self) -> Option<&DownloadOutcome> {
        self.download.as_ref()
    }
}

/// The seconds since 1970 - or why the clock cannot say: a clock before the epoch is no
/// reason to call a signing-key statement unexpired.
fn unix_seconds() -> Result<u64, String> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| "the system clock is before 1970: no signature can be checked".to_owned())
}

/// A parsed signing-key statement (see [`ReleaseInfo::signing_key_statement`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigningKeyStatement {
    /// Base64 minisign public key of the day-to-day signing key.
    pub pubkey_b64: String,
    /// Unix seconds after which the statement is dead.
    pub expires_unix: u64,
    /// Monotonic rotation counter; clients persist the highest accepted
    /// value and refuse anything lower.
    pub generation: u64,
}

/// Parses the canonical statement string
/// `azul-signing-key-v1|pubkey=<b64>|expires=<unix>|generation=<n>`.
/// STRICT: unknown prefixes, missing fields or non-numeric values are
/// errors — a statement is a security boundary, not a config file.
///
/// # Errors
///
/// Returns a description of the first malformed field.
pub fn parse_signing_key_statement(statement: &str) -> Result<SigningKeyStatement, String> {
    let mut parts = statement.split('|');
    if parts.next() != Some("azul-signing-key-v1") {
        return Err(format!(
            "signing-key statement: unknown format (expected `azul-signing-key-v1|…`, got \
             {statement:?})"
        ));
    }
    let mut pubkey_b64 = None;
    let mut expires_unix = None;
    let mut generation = None;
    for part in parts {
        if let Some(v) = part.strip_prefix("pubkey=") {
            pubkey_b64 = Some(v.to_owned());
        } else if let Some(v) = part.strip_prefix("expires=") {
            expires_unix = Some(
                v.parse::<u64>()
                    .map_err(|e| format!("statement expires: {e}"))?,
            );
        } else if let Some(v) = part.strip_prefix("generation=") {
            generation = Some(
                v.parse::<u64>()
                    .map_err(|e| format!("statement generation: {e}"))?,
            );
        } else {
            return Err(format!("signing-key statement: unknown field {part:?}"));
        }
    }
    Ok(SigningKeyStatement {
        pubkey_b64: pubkey_b64.ok_or("signing-key statement: missing pubkey")?,
        expires_unix: expires_unix.ok_or("signing-key statement: missing expires")?,
        generation: generation.ok_or("signing-key statement: missing generation")?,
    })
}

/// A parsed release statement (see [`ReleaseInfo::release_statement`]): what the SIGNING key says
/// about one artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseStatement {
    /// The app the artifact is for (`azwriter`).
    pub app: String,
    /// Its version, as the manifest must also say it.
    pub version: String,
    /// The platform it is built for, `<os>-<arch>` ([`platform_label`]).
    pub platform: String,
    /// The SHA-256 of the artifact's bytes.
    pub sha256: [u8; 32],
    /// When it was released, seconds since 1970.
    pub released_unix: u64,
    /// The lowest signing-key generation that may sign releases from now on: a client that accepts
    /// this release refuses every statement of a lower generation afterwards, so a retired key
    /// stays dead for it even if it never saw the statement that retired it.
    pub min_generation: u64,
}

/// The platform this build runs on, as a release statement names one: `<os>-<arch>` in Rust's
/// words (`macos-aarch64`, `linux-x86_64`, `windows-x86_64`).
#[must_use]
pub fn platform_label() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// Parses the canonical release statement
/// `azul-release-v1|app=<id>|version=<semver>|platform=<os-arch>|sha256=<hex>|released=<unix>|min_generation=<n>`.
/// STRICT, like the signing-key statement: an unknown format or field, a missing or repeated
/// field, a value that does not parse is an error - a statement is a security boundary, not a
/// config file.
///
/// # Errors
///
/// Returns a description of the first malformed field.
pub fn parse_release_statement(statement: &str) -> Result<ReleaseStatement, String> {
    let mut parts = statement.split('|');
    if parts.next() != Some("azul-release-v1") {
        return Err(format!(
            "release statement: unknown format (expected `azul-release-v1|...`, got \
             {statement:?})"
        ));
    }
    let mut app = None;
    let mut version = None;
    let mut platform = None;
    let mut sha256 = None;
    let mut released = None;
    let mut min_generation = None;
    for part in parts {
        let Some((key, value)) = part.split_once('=') else {
            return Err(format!("release statement: field {part:?} has no value"));
        };
        let repeated = || format!("release statement: `{key}` twice");
        match key {
            "app" => {
                if app.replace(value.to_owned()).is_some() {
                    return Err(repeated());
                }
            }
            "version" => {
                if version.replace(value.to_owned()).is_some() {
                    return Err(repeated());
                }
            }
            "platform" => {
                if platform.replace(value.to_owned()).is_some() {
                    return Err(repeated());
                }
            }
            "sha256" => {
                let hash = parse_sha256_hex(value)
                    .ok_or_else(|| format!("release statement sha256: {value:?} is no sha256"))?;
                if sha256.replace(hash).is_some() {
                    return Err(repeated());
                }
            }
            "released" => {
                let at = value
                    .parse::<u64>()
                    .map_err(|e| format!("release statement released: {e}"))?;
                if released.replace(at).is_some() {
                    return Err(repeated());
                }
            }
            "min_generation" => {
                let n = value
                    .parse::<u64>()
                    .map_err(|e| format!("release statement min_generation: {e}"))?;
                if min_generation.replace(n).is_some() {
                    return Err(repeated());
                }
            }
            _ => return Err(format!("release statement: unknown field {key:?}")),
        }
    }
    let present = |field: Option<String>, name: &str| {
        field
            .filter(|v| !v.is_empty())
            .ok_or_else(|| format!("release statement: missing {name}"))
    };
    Ok(ReleaseStatement {
        app: present(app, "app")?,
        version: present(version, "version")?,
        platform: present(platform, "platform")?,
        sha256: sha256.ok_or("release statement: missing sha256")?,
        released_unix: released.ok_or("release statement: missing released")?,
        min_generation: min_generation.ok_or("release statement: missing min_generation")?,
    })
}

/// The manifest names every link of the signature chain (it may still be wrong, but nothing is
/// missing): the artifact's signature, the signing-key statement and the root's signature of it,
/// and the release statement with the signing key's signature of it.
fn chain_is_complete(release: &ReleaseInfo) -> bool {
    !release.signature.as_str().trim().is_empty()
        && !release.signing_key_statement.as_str().trim().is_empty()
        && !release.signing_key_statement_sig.as_str().trim().is_empty()
        && !release.release_statement.as_str().trim().is_empty()
        && !release.release_statement_sig.as_str().trim().is_empty()
}

/// The error of a release that does not carry the chain a signed policy needs.
const UNSIGNED_RELEASE: &str = "this app pins an update root key but the manifest offers an \
                                UNSIGNED release (missing signature / signing_key_statement / \
                                signing_key_statement_sig / release_statement / \
                                release_statement_sig)";

/// What a client knows about the release it is looking for, apart from the keys: which app it is
/// and which platform it runs on. The manifest says its own app, version and platform, but those
/// are unsigned words; the release statement is signed, and it has to agree with THIS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expect {
    /// The app (`azwriter`): the key its manifest, registry folder and statement go by.
    pub app: String,
    /// The platform, `<os>-<arch>` ([`platform_label`]).
    pub platform: String,
}

impl Expect {
    /// `app` on the platform this build runs on.
    #[must_use]
    pub fn here(app: &str) -> Self {
        Self {
            app: app.to_owned(),
            platform: platform_label(),
        }
    }
}

/// The error of a statement for another artifact than the one that arrived.
fn statement_mismatch(what: &str, said: &str, expected: &str) -> String {
    format!(
        "release statement does not match this release: {what} is {said:?} but this is \
         {expected:?}"
    )
}

/// How far into the future a release statement may be dated (clocks differ).
const STATEMENT_CLOCK_SKEW_SECS: u64 = 86_400;

/// Checks a parsed release statement against what arrived: the app and platform the client
/// expects, the manifest's version label, the hash of the bytes, the key statement that vouches
/// for the signing key (REVIEW Y3). Pure.
fn check_release_statement(
    statement: &ReleaseStatement,
    release: &ReleaseInfo,
    expect: &Expect,
    sha256: &[u8; 32],
    key: &SigningKeyStatement,
    now_unix: u64,
) -> Result<(), String> {
    if statement.app != expect.app {
        return Err(statement_mismatch("the app", &statement.app, &expect.app));
    }
    if statement.platform != expect.platform {
        return Err(statement_mismatch(
            "the platform",
            &statement.platform,
            &expect.platform,
        ));
    }
    let label = strip_version_prefix(release.version.as_str());
    if statement.version != label {
        return Err(statement_mismatch(
            "the version",
            &statement.version,
            &label,
        ));
    }
    if statement.sha256 != *sha256 {
        return Err(statement_mismatch(
            "the artifact's sha256",
            &sha256_hex(&statement.sha256),
            &sha256_hex(sha256),
        ));
    }
    if statement.released_unix > now_unix.saturating_add(STATEMENT_CLOCK_SKEW_SECS) {
        return Err(format!(
            "release statement is dated in the future (unix {}, now {now_unix})",
            statement.released_unix
        ));
    }
    if statement.min_generation > key.generation {
        return Err(format!(
            "this release requires a signing-key generation of at least {} but the statement \
             that names the signing key is generation {}",
            statement.min_generation, key.generation
        ));
    }
    Ok(())
}

/// Verifies a staged artifact's SIGNATURE CHAIN against the app's compiled-in minisign root
/// public key(s):
///
/// 1. a ROOT key must verify `signing_key_statement_sig` over the statement string - only the
///    root can appoint a signing key;
/// 2. the statement must not be expired and its `generation` must be at least
///    `state.key_generation` (rollback refusal);
/// 3. the SIGNING key from the statement must verify `signature` over the artifact bytes;
/// 4. the SAME signing key must verify `release_statement_sig` over the release statement, and
///    the statement must name this app, this platform, the version the manifest says, the hash
///    of the bytes that were just read, a date that is not in the future and a minimum
///    generation the key statement meets (REVIEW Y3: a manifest's version, URL and rollout are
///    unsigned words; the bytes of a genuine old release must not pass under a new label).
///
/// Only a fully verified chain advances the marks: the highest signing-key generation accepted
/// (also to the release's `min_generation`), the highest VERSION seen and the clock floor - an
/// unverified manifest, which can say anything, moves none of them. A chain that verifies for a
/// version BELOW the highest one seen is refused, however well it is signed: that is a rollback
/// to a release whose flaws are known. The clock the expiry and "dated in the future" checks go
/// by never runs behind the floor, so a clock set back does not revive a statement that expired.
///
/// A manifest that omits any part of the chain is a HARD error: "this release is unsigned" is
/// never a downgrade path, and there is no root key that is empty ([`RootKeys`]). The file is
/// read ONCE, block by block, through the signature check and the hash together, so the
/// returned SHA-256 is the hash of the very bytes that were verified.
///
/// # Errors
///
/// Returns a description naming the failing link; the caller must DISCARD the artifact.
pub fn verify_release_signature(
    path: &Path,
    release: &ReleaseInfo,
    roots: &RootKeys,
    expect: &Expect,
    state: &mut UpdateState,
    now_unix: u64,
) -> Result<[u8; 32], String> {
    if !chain_is_complete(release) {
        return Err(UNSIGNED_RELEASE.to_owned());
    }
    // The time of the last chain that verified is a floor for the clock (a clock set back must
    // not make an expired statement valid again).
    let now_unix = now_unix.max(state.clock_floor_unix);

    let statement_str = release.signing_key_statement.as_str();
    let statement_sig =
        minisign_verify::Signature::decode(release.signing_key_statement_sig.as_str())
            .map_err(|e| format!("signing-key statement signature: {e}"))?;
    roots
        .verify(statement_str.as_bytes(), &statement_sig)
        .map_err(|e| format!("signing-key statement not signed by the root key: {e}"))?;

    let statement = parse_signing_key_statement(statement_str)?;
    if now_unix >= statement.expires_unix {
        return Err(format!(
            "signing-key statement expired at unix {} (now {now_unix})",
            statement.expires_unix
        ));
    }
    if statement.generation < state.key_generation {
        return Err(format!(
            "signing-key generation ROLLBACK: statement is generation {} but this client already \
             accepted generation {}",
            statement.generation, state.key_generation
        ));
    }

    let signing = minisign_verify::PublicKey::from_base64(&statement.pubkey_b64)
        .map_err(|e| format!("signing public key (from statement): {e}"))?;
    let artifact_sig = minisign_verify::Signature::decode(release.signature.as_str())
        .map_err(|e| format!("artifact signature: {e}"))?;
    let mut verifier = signing
        .verify_stream(&artifact_sig)
        .map_err(|e| format!("artifact signature invalid: {e}"))?;
    let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
    read_blocks(path, |block| {
        verifier.update(block);
        sha2::Digest::update(&mut hasher, block);
    })
    .map_err(|e| format!("signature read: {e}"))?;
    verifier
        .finalize()
        .map_err(|e| format!("artifact signature invalid: {e}"))?;
    let mut sha256 = [0u8; 32];
    sha256.copy_from_slice(&sha2::Digest::finalize(hasher));

    // The release statement: what the signing key says about THIS artifact.
    let release_sig = minisign_verify::Signature::decode(release.release_statement_sig.as_str())
        .map_err(|e| format!("release statement signature: {e}"))?;
    signing
        .verify(
            release.release_statement.as_str().as_bytes(),
            &release_sig,
            false,
        )
        .map_err(|e| format!("release statement not signed by the signing key: {e}"))?;
    let released = parse_release_statement(release.release_statement.as_str())?;
    check_release_statement(&released, release, expect, &sha256, &statement, now_unix)?;
    // What is compared with the mark is the version the signing key vouches for, not a label of
    // the manifest: a manifest that lists a decoy "newer" release (one this client's cohort is
    // not offered) beside a genuine older one gets past the check on the unsigned labels.
    if compare_versions(&released.version, &state.highest_seen) == core::cmp::Ordering::Less {
        return Err(format!(
            "release version ROLLBACK: the signed release is {} but this client already \
             verified {}",
            released.version, state.highest_seen
        ));
    }

    // Only a fully verified chain advances the marks.
    state.key_generation = state
        .key_generation
        .max(statement.generation)
        .max(released.min_generation);
    if compare_versions(&released.version, &state.highest_seen) == core::cmp::Ordering::Greater {
        state.highest_seen.clone_from(&released.version);
    }
    state.clock_floor_unix = state.clock_floor_unix.max(now_unix);
    Ok(sha256)
}

/// What `policy` makes of the file at `path` as `release`'s artifact: the hash of the bytes that
/// passed, and whether a signature chain was checked. The file is not touched.
fn verify_artifact(
    path: &Path,
    release: &ReleaseInfo,
    policy: &UpdatePolicy,
    expect: &Expect,
    state: &mut UpdateState,
    now_unix: u64,
) -> Result<([u8; 32], bool), String> {
    match policy {
        UpdatePolicy::Signed(roots) => {
            let sha256 = verify_release_signature(path, release, roots, expect, state, now_unix)?;
            // The chain vouches for the bytes; a digest the manifest pins besides must agree.
            if let Some(pin) = pinned_digest(release)? {
                if pin != sha256 {
                    return Err(digest_mismatch(&pin, &sha256));
                }
            }
            Ok((sha256, true))
        }
        UpdatePolicy::DigestOnly => {
            let pin = pinned_digest(release)?.ok_or_else(|| UNPINNED_RELEASE.to_owned())?;
            let sha256 = sha256_of_file(path)?;
            if sha256 == pin {
                Ok((sha256, false))
            } else {
                Err(digest_mismatch(&pin, &sha256))
            }
        }
    }
}

/// The error of a release that pins no digest under a policy that has nothing else to check.
const UNPINNED_RELEASE: &str = "this release pins no digest, and this app accepts no update \
                                that is not signed";

/// Whether `release` can meet `policy` at all - answered before a byte is downloaded.
fn can_meet(release: &ReleaseInfo, policy: &UpdatePolicy) -> Result<(), String> {
    match policy {
        UpdatePolicy::Signed(_) if !chain_is_complete(release) => Err(UNSIGNED_RELEASE.to_owned()),
        UpdatePolicy::Signed(_) => Ok(()),
        UpdatePolicy::DigestOnly => pinned_digest(release)?
            .map(|_| ())
            .ok_or_else(|| UNPINNED_RELEASE.to_owned()),
    }
}

/// What [`download_update`] did — enough to log an honest story
/// ("resumed at byte N", "already staged").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadOutcome {
    /// The fully staged artifact.
    pub path: PathBuf,
    /// Bytes that were ALREADY on disk from an interrupted attempt
    /// (0 = fresh download).
    pub resumed_from_bytes: u64,
    /// Bytes transferred THIS call.
    pub bytes_written: u64,
    /// The final artifact already existed — nothing was transferred.
    pub used_cached: bool,
    /// The server honored the `Range` request with `206 Partial Content`;
    /// false on a resume means it ignored ranges and the file restarted.
    pub server_supports_resume: bool,
}

/// The digest check of a download: when the release pins a digest the file must match it. A
/// release that pins none is not an error HERE - what that means is the policy's to say
/// ([`verify_artifact`]: a signed chain vouches for the bytes, a digest-only policy refuses).
fn check_pin(path: &Path, release: &ReleaseInfo) -> Result<(), String> {
    let Some(pin) = pinned_digest(release)? else {
        return Ok(());
    };
    let actual = sha256_of_file(path).map_err(|e| format!("digest read: {e}"))?;
    if actual == pin {
        Ok(())
    } else {
        Err(digest_mismatch(&pin, &actual))
    }
}

/// The most bytes one update artifact may have. A server that answers `206` for ever (or lies
/// about the size) used to fill the disk, one 1 MiB window after another (REVIEW Y8).
const MAX_UPDATE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// The file name of the artifact in the staging folder: the last segment of the download URL, or
/// `update-<version>.bin` when that is empty or only dots. Every character outside
/// `[A-Za-z0-9._+-]` becomes `-` - an OCI blob URL ends in `sha256:<hex>` and `:` cannot appear
/// in a Windows file name, and a version such as `..\..\x` must not climb out of the folder
/// (REVIEW Y8: the version-based name was the one unsanitised name).
fn staged_artifact_name(download_url: &str, version: &str) -> String {
    let safe = |text: &str| -> String {
        text.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+') {
                    c
                } else {
                    '-'
                }
            })
            .collect()
    };
    let from_url = safe(download_url.rsplit('/').next().unwrap_or_default());
    if from_url.is_empty() || from_url.chars().all(|c| c == '.') {
        format!("update-{}.bin", safe(version))
    } else {
        from_url
    }
}

/// `Content-Range: bytes <start>-<end>/<total>` (`total` may be `*`): the range and the total
/// size when the server knows it.
fn parse_content_range(value: &str) -> Option<(u64, u64, Option<u64>)> {
    let rest = value.trim().strip_prefix("bytes")?.trim_start();
    let (range, total) = rest.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    let start = start.trim().parse::<u64>().ok()?;
    let end = end.trim().parse::<u64>().ok()?;
    if end < start {
        return None;
    }
    let total = match total.trim() {
        "*" => None,
        digits => Some(digits.parse::<u64>().ok()?),
    };
    Some((start, end, total))
}

/// Downloads a release artifact into `staging_dir`, CACHED and RESUMABLE:
///
/// * a fully staged artifact from an earlier call is reused verbatim (`used_cached`) — "download
///   automatically" then makes "install now" instant and offline-safe;
/// * an INTERRUPTED download leaves `<name>.partial`; the next call sends `Range: bytes=<len>-` and
///   appends on `206 Partial Content`. A server without range support answers `200` and the file
///   restarts from zero (correct, just not incremental) — `server_supports_resume` reports which
///   happened.
///
/// A `206` is believed only as far as it can be checked: its `Content-Range` has to start where
/// the request did and describe the bytes that came, the size it names may not change from one
/// window to the next, and nothing larger than 2 GiB is taken (the partial file of a refused
/// download is deleted, so the next attempt starts clean).
///
/// Verification beyond the transport (digest/minisign) is the documented
/// next rung.
///
/// # Errors
///
/// Returns a description on transport or filesystem failure.
#[cfg(feature = "http")]
pub fn download_update(
    release: &ReleaseInfo,
    staging_dir: &Path,
) -> Result<DownloadOutcome, String> {
    download_update_capped(release, staging_dir, MAX_UPDATE_BYTES)
}

/// [`download_update`] with the limit as a parameter (the tests use a small one).
#[cfg(feature = "http")]
fn download_update_capped(
    release: &ReleaseInfo,
    staging_dir: &Path,
    max_bytes: u64,
) -> Result<DownloadOutcome, String> {
    if release.download_url.as_str().trim().is_empty() {
        // Sources legitimately answer "there is a newer version" without
        // naming an artifact for THIS platform (a VERSION file, a release
        // with no matching asset). Say that, rather than failing on an
        // empty URL somewhere inside the HTTP layer.
        return Err(format!(
            "version {} is available but the update source names no download for this platform",
            release.version.as_str()
        ));
    }
    std::fs::create_dir_all(staging_dir).map_err(|e| e.to_string())?;
    let file_name = staged_artifact_name(release.download_url.as_str(), release.version.as_str());
    let final_path = staging_dir.join(&file_name);
    if final_path.exists() {
        // A cached artifact is only reusable if it still matches the pin —
        // a stale or corrupted staging file must re-download, not install.
        if let Err(e) = check_pin(&final_path, release) {
            drop(std::fs::remove_file(&final_path));
            return Err(format!(
                "cached artifact failed verification ({e}); removed — retry the download"
            ));
        }
        return Ok(DownloadOutcome {
            path: final_path,
            resumed_from_bytes: 0,
            bytes_written: 0,
            used_cached: true,
            server_supports_resume: false,
        });
    }
    let partial_path = staging_dir.join(format!("{file_name}.partial"));
    let resumed_from = std::fs::metadata(&partial_path).map_or(0, |m| m.len());
    // A refused download leaves no partial file behind.
    let refuse = |why: String| -> String {
        drop(std::fs::remove_file(&partial_path));
        why
    };
    let too_large = || {
        format!(
            "download: the file is larger than the {} MiB an update may be",
            max_bytes / (1024 * 1024)
        )
    };
    if resumed_from > max_bytes {
        return Err(refuse(too_large()));
    }

    // CHUNKED-RANGE loop: each 1 MiB window lands on disk as it completes,
    // so killing the process mid-download keeps every finished chunk and
    // the next call resumes from the `.partial` length. (The HTTP layer
    // buffers whole responses in memory — a single full-file request would
    // leave NOTHING on disk when interrupted.) A server that ignores
    // `Range` answers `200` with the whole file on the first request and
    // the loop degrades to one buffered write.
    const CHUNK: u64 = 1024 * 1024;
    use std::io::Write as _;
    let mut offset = resumed_from;
    let mut written_this_call = 0u64;
    let mut honored = false;
    let mut known_total: Option<u64> = None;
    loop {
        let mut config = crate::http::HttpRequestConfig::new()
            .with_header("Range", format!("bytes={offset}-{}", offset + CHUNK - 1));
        // A registry blob needs the bearer token the manifest request got.
        for pair in release.download_headers.as_ref() {
            config = config.with_header(pair.key.as_str(), pair.value.as_str());
        }
        let response = crate::http::http_get_with_config(release.download_url.as_str(), &config)
            .map_err(|e| format!("download: {e:?}"))?;
        match response.status_code {
            206 => {
                let bytes = response.body.as_ref();
                let length = bytes.len() as u64;
                let range = response
                    .headers
                    .as_ref()
                    .iter()
                    .find(|h| h.name.as_str().eq_ignore_ascii_case("content-range"))
                    .and_then(|h| parse_content_range(h.value.as_str()));
                let Some((start, end, total)) = range else {
                    return Err(refuse(
                        "download: the server answered 206 without a usable Content-Range"
                            .to_owned(),
                    ));
                };
                // The bytes have to be the window that was asked for, where it was asked for.
                if start != offset || end - start + 1 != length || length > CHUNK {
                    return Err(refuse(format!(
                        "download: the server sent bytes {start}-{end} ({length} bytes) for a                          request at {offset}"
                    )));
                }
                if let Some(total) = total {
                    if known_total.is_some_and(|known| known != total) {
                        return Err(refuse(
                            "download: the size of the file changed during the download".to_owned(),
                        ));
                    }
                    if total > max_bytes {
                        return Err(refuse(too_large()));
                    }
                    known_total = Some(total);
                }
                honored = true;
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&partial_path)
                    .map_err(|e| e.to_string())?;
                file.write_all(bytes).map_err(|e| e.to_string())?;
                offset += length;
                written_this_call += length;
                if offset > max_bytes {
                    return Err(refuse(too_large()));
                }
                // The end of the file: the size the server named is reached, or the window came
                // back short.
                if known_total.is_some_and(|total| offset >= total) || length < CHUNK {
                    break;
                }
            }
            // 200 = the server ignored the Range header: it sent the WHOLE
            // file; a previous partial is superseded.
            200 => {
                let bytes = response.body.as_ref();
                if bytes.len() as u64 > max_bytes {
                    return Err(refuse(too_large()));
                }
                std::fs::write(&partial_path, bytes).map_err(|e| e.to_string())?;
                written_this_call = bytes.len() as u64;
                honored = false;
                break;
            }
            // 416 Range Not Satisfiable: offset is already at (or past) the
            // end — the partial IS the complete file.
            416 => break,
            code => return Err(format!("download HTTP {code}")),
        }
    }
    // Verify BEFORE the artifact gains its final (trusted-looking) name; a
    // mismatch discards the partial so the next attempt starts clean.
    if let Err(e) = check_pin(&partial_path, release) {
        drop(std::fs::remove_file(&partial_path));
        return Err(e);
    }
    std::fs::rename(&partial_path, &final_path).map_err(|e| e.to_string())?;
    Ok(DownloadOutcome {
        path: final_path,
        resumed_from_bytes: if honored { resumed_from } else { 0 },
        bytes_written: written_this_call,
        used_cached: false,
        server_supports_resume: honored,
    })
}

/// [`download_update`] plus the FULL verification story - the one entry point the update worker
/// and the install dialog use, so no staged artifact reaches [`apply_update`] without passing
/// what `policy` demands on THIS call: under [`UpdatePolicy::Signed`] the signature chain
/// ([`verify_release_signature`]), under [`UpdatePolicy::DigestOnly`] the digest pin (a release
/// that pins none is refused). A release that cannot meet the policy at all is refused before a
/// byte is downloaded. A cached staging file is re-verified every time (disk contents are not
/// trusted across runs); any failure deletes the artifact so the next attempt starts clean.
/// `state`'s `key_generation` high-water mark advances (and is saved by the caller) on success.
///
/// # Errors
///
/// Returns a description on transport, digest or signature-chain failure.
#[cfg(feature = "http")]
pub fn download_and_verify(
    release: &ReleaseInfo,
    expect: &Expect,
    staging_dir: &Path,
    policy: &UpdatePolicy,
    state: &mut UpdateState,
) -> Result<VerifiedArtifact, String> {
    let now = unix_seconds()?;
    can_meet(release, policy)?;
    let outcome = download_update(release, staging_dir)?;
    match verify_artifact(&outcome.path, release, policy, expect, state, now) {
        Ok((sha256, signed)) => Ok(VerifiedArtifact {
            path: outcome.path.clone(),
            sha256,
            version: release.version.as_str().to_owned(),
            signed,
            download: Some(outcome),
        }),
        Err(e) => {
            drop(std::fs::remove_file(&outcome.path));
            Err(format!(
                "staged artifact failed {} ({e}); removed",
                if policy.is_signed() {
                    "signature verification"
                } else {
                    "verification"
                }
            ))
        }
    }
}

/// A name part no other call of this process used and no file planted before it ran can have.
fn unique_suffix() -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    // collapse-ok: only makes a name unique; the counter and the process id do the rest.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    format!(
        "{}-{nanos}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

/// Copies `from` to the NEW file `to` and hashes what it writes: the copy has to be the bytes
/// that were verified (`expected`). `to` is created exclusively - never through a link that is
/// already there.
fn copy_verified(from: &Path, to: &Path, expected: &[u8; 32]) -> Result<(), String> {
    use std::io::{Read as _, Write as _};

    use sha2::Digest as _;
    let mut source =
        std::fs::File::open(from).map_err(|e| format!("stage copy: {}: {e}", from.display()))?;
    let mut sink = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)
        .map_err(|e| format!("stage copy: {}: {e}", to.display()))?;
    let mut hasher = sha2::Sha256::new();
    let mut block = vec![0u8; 64 * 1024];
    loop {
        let n = source
            .read(&mut block)
            .map_err(|e| format!("stage copy: {}: {e}", from.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&block[..n]);
        sink.write_all(&block[..n])
            .map_err(|e| format!("stage copy: {}: {e}", to.display()))?;
    }
    sink.sync_all()
        .map_err(|e| format!("stage copy: {}: {e}", to.display()))?;
    let mut actual = [0u8; 32];
    actual.copy_from_slice(&hasher.finalize());
    if actual == *expected {
        Ok(())
    } else {
        Err(format!(
            "the staged file changed after it was verified (it was sha256:{}, it is sha256:{}): \
             not installed",
            sha256_hex(expected),
            sha256_hex(&actual)
        ))
    }
}

/// The kind of archive or compressed stream `head` (the first bytes of a file) starts like, if it
/// does. This updater replaces the program with the artifact AS IT IS and unpacks nothing, so an
/// archive would overwrite the executable with a `.tar.gz` (REVIEW Y10).
fn archive_kind(head: &[u8]) -> Option<&'static str> {
    const MAGIC: [(&[u8], &str); 8] = [
        (&[0x1f, 0x8b], "gzip"),
        (b"PK\x03\x04", "zip"),
        (b"PK\x05\x06", "zip"),
        (&[0xfd, b'7', b'z', b'X', b'Z', 0x00], "xz"),
        (b"BZh", "bzip2"),
        (&[0x28, 0xb5, 0x2f, 0xfd], "zstd"),
        (&[b'7', b'z', 0xbc, 0xaf, 0x27, 0x1c], "7z"),
        (b"!<arch>\n", "ar"),
    ];
    for (magic, kind) in MAGIC {
        if head.starts_with(magic) {
            return Some(kind);
        }
    }
    // A tar archive says "ustar" at byte 257.
    (head.len() >= 262 && &head[257..262] == b"ustar").then_some("tar")
}

/// The swap on a platform where the running program cannot be replaced but CAN be renamed: the old
/// file moves to `aside`, the new one takes its place. When the second move fails the old file is
/// moved BACK - the program must not be left without its executable (REVIEW Y10: it was). `rename`
/// is a parameter so the failure can be made to happen in a test.
#[cfg(any(windows, test))]
fn swap_moving_the_old_aside(
    incoming: &Path,
    target: &Path,
    aside: &Path,
    rename: impl Fn(&Path, &Path) -> std::io::Result<()>,
) -> Result<(), String> {
    drop(std::fs::remove_file(aside));
    if let Err(e) = rename(target, aside) {
        return Err(format!("move-aside: {e}"));
    }
    if let Err(e) = rename(incoming, target) {
        return Err(match rename(aside, target) {
            Ok(()) => format!("swap: {e} (the previous version was put back)"),
            Err(back) => format!(
                "swap: {e}; and the previous version could not be put back ({back}): it is at {}",
                aside.display()
            ),
        });
    }
    Ok(())
}

/// Applies a VERIFIED update over `target`: a same-directory temp copy and an ATOMIC rename (the
/// classic swap; on Windows a running image cannot be replaced in place - the rename dance moves
/// the OLD file aside first, and back if the new one cannot take its place). The COPY is hashed
/// as it is written and has to be the bytes that passed the checks ([`VerifiedArtifact`]), so a
/// file swapped between the check and the install is caught in the bytes about to become the
/// program (REVIEW Y6); the copy's name is new and created exclusively, so a link planted at a
/// fixed name leads nowhere. An archive is refused: this unpacks nothing, so it would overwrite
/// the program with the archive. Callers verify [`InstallKind::allows_self_update`] first - this
/// function only does the mechanics.
///
/// # Errors
///
/// Returns a description on filesystem failure, when the staged file is no longer the file that
/// was verified, or when it is an archive.
pub fn apply_update(artifact: &VerifiedArtifact, target: &Path) -> Result<(), String> {
    let dir = target
        .parent()
        .ok_or_else(|| "target has no parent directory".to_owned())?;
    let incoming = dir.join(format!(".update-incoming-{}", unique_suffix()));
    if let Err(e) = copy_verified(artifact.path(), &incoming, &artifact.sha256) {
        drop(std::fs::remove_file(&incoming));
        return Err(e);
    }
    // The bytes that are about to become the program, not the staged file (which may change).
    if let Some(kind) = std::fs::File::open(&incoming).ok().and_then(|mut f| {
        use std::io::Read as _;
        let mut head = [0u8; 512];
        let n = f.read(&mut head).ok()?;
        archive_kind(&head[..n])
    }) {
        drop(std::fs::remove_file(&incoming));
        return Err(format!(
            "the update is a {kind} archive, and this updater does not unpack: it would replace \
             the program with the archive. Publish the bare executable"
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        drop(std::fs::set_permissions(&incoming, perms));
    }
    #[cfg(windows)]
    {
        // A running image is locked against replacement but CAN be renamed:
        // move it aside, then move the new file in (and the old one back if that fails).
        let swapped = swap_moving_the_old_aside(
            &incoming,
            target,
            &dir.join(".update-previous"),
            |from, to| std::fs::rename(from, to),
        );
        if swapped.is_err() {
            drop(std::fs::remove_file(&incoming));
        }
        return swapped;
    }
    #[cfg(not(windows))]
    {
        if let Err(e) = std::fs::rename(&incoming, target) {
            drop(std::fs::remove_file(&incoming));
            return Err(format!("swap: {e}"));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// A manager updating other programs
// ---------------------------------------------------------------------------
//
// An install manager (AzInstallManager) checks, stages and installs programs
// other than itself. Each program keeps its own state folder (high-water
// mark, accepted key generation), while the rollout cohort belongs to the
// machine: the manager draws it once and passes it into every check, so one
// computer sits in the same cohort for every program it runs.

/// [`check_for_updates_blocking`] for another program: its state is loaded
/// from and saved to `state_dir`, and `rollout_bucket` (the machine's cohort,
/// 0-99) replaces whatever cohort that folder held. Blocking: run it on a
/// `Thread`.
#[cfg(feature = "http")]
#[must_use]
pub fn check_program_for_updates(
    manifest_url: &str,
    current_version: &str,
    channel: &str,
    state_dir: &Path,
    rollout_bucket: u8,
    audience: UpdateAudience,
) -> UpdateCheckResult {
    let mut state = UpdateState::load(state_dir);
    state.rollout_bucket = Some(rollout_bucket.min(99));
    let result =
        check_for_updates_blocking(manifest_url, current_version, channel, &mut state, audience);
    state.save(state_dir);
    result
}

/// [`download_and_verify`] for another program, as the C API calls it: the root key is TEXT
/// because that is what crosses the C boundary, and it is parsed HERE - an empty or unreadable
/// one is an error, never "no check" ([`UpdatePolicy::from_text`]); `allow_unsigned` is the
/// program's explicit choice of the digest pin alone. `app` is the program's key (`azwriter`):
/// the release statement has to be for it. The artifact lands in `staging_dir`, the accepted key
/// generation and the highest verified version in `state_dir`. Returns the proof of what was
/// staged. Blocking: run it on a `Thread`.
///
/// # Errors
///
/// Returns a description on a missing or unreadable root key, transport, digest or
/// signature-chain failure; nothing refused stays staged.
#[cfg(feature = "http")]
pub fn stage_program_update(
    release: &ReleaseInfo,
    app: &str,
    staging_dir: &Path,
    root_public_key: &str,
    allow_unsigned: bool,
    state_dir: &Path,
) -> Result<VerifiedArtifact, String> {
    let policy = UpdatePolicy::from_text(root_public_key, allow_unsigned)?;
    stage_program_update_with(release, &Expect::here(app), staging_dir, &policy, state_dir)
}

/// [`stage_program_update`] under a policy that is already a policy.
///
/// # Errors
///
/// As [`download_and_verify`].
#[cfg(feature = "http")]
pub fn stage_program_update_with(
    release: &ReleaseInfo,
    expect: &Expect,
    staging_dir: &Path,
    policy: &UpdatePolicy,
    state_dir: &Path,
) -> Result<VerifiedArtifact, String> {
    // The marks of this program: unreadable ones stop the staging (see `UpdateState::read`), and
    // marks that cannot be recorded take the staged file with them - an update that cannot move
    // the rollback marks is not installed.
    let mut state = UpdateState::read(state_dir)?;
    let staged = download_and_verify(release, expect, staging_dir, policy, &mut state);
    match (staged, state.try_save(state_dir)) {
        (Ok(artifact), Ok(())) => Ok(artifact),
        (Ok(artifact), Err(e)) => {
            drop(std::fs::remove_file(artifact.path()));
            Err(format!("{e}; the update was not staged"))
        }
        (Err(e), _) => Err(e),
    }
}

/// Checks a staged file once more right before it is installed: what the policy demands (the
/// signature chain, or the digest pin). A file on disk is not trusted across runs - it may have
/// changed since it was staged. The root key is TEXT as for [`stage_program_update`].
///
/// # Errors
///
/// Returns a description of the failing check; the caller must not install the file.
pub fn verify_staged_program_update(
    path: &Path,
    release: &ReleaseInfo,
    app: &str,
    root_public_key: &str,
    allow_unsigned: bool,
    state_dir: &Path,
) -> Result<VerifiedArtifact, String> {
    let policy = UpdatePolicy::from_text(root_public_key, allow_unsigned)?;
    verify_staged_program_update_with(path, release, &Expect::here(app), &policy, state_dir)
}

/// [`verify_staged_program_update`] under a policy that is already a policy. The file is read
/// once; the hash in the proof is the hash of the bytes that were checked.
///
/// # Errors
///
/// Returns a description of the failing check; the caller must not install the file.
pub fn verify_staged_program_update_with(
    path: &Path,
    release: &ReleaseInfo,
    expect: &Expect,
    policy: &UpdatePolicy,
    state_dir: &Path,
) -> Result<VerifiedArtifact, String> {
    can_meet(release, policy)?;
    let now = unix_seconds()?;
    let mut state = UpdateState::read(state_dir)?;
    let (sha256, signed) = verify_artifact(path, release, policy, expect, &mut state, now)?;
    state.try_save(state_dir)?;
    Ok(VerifiedArtifact {
        path: path.to_path_buf(),
        sha256,
        version: release.version.as_str().to_owned(),
        signed,
        download: None,
    })
}

/// A file that passed its checks, as the C API hands it over: where it is, the hash of the bytes
/// that passed (64 hex digits) and the version the release said. The caller installs from the
/// path and compares what it copies with the hash ([`VerifiedArtifact`]'s counterpart across the
/// C boundary).
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct StagedUpdate {
    /// The verified file.
    pub path: AzString,
    /// The SHA-256 of the bytes that passed.
    pub sha256: AzString,
    /// The version the release said.
    pub version: AzString,
}

impl From<&VerifiedArtifact> for StagedUpdate {
    fn from(artifact: &VerifiedArtifact) -> Self {
        Self {
            path: artifact.path.to_string_lossy().as_ref().into(),
            sha256: artifact.sha256_hex().into(),
            version: artifact.version.clone().into(),
        }
    }
}

/// What staging or checking a program's update concluded: the C API's
/// `Result<VerifiedArtifact, String>` for [`stage_program_update`] and
/// [`verify_staged_program_update`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum UpdateStageResult {
    /// The verified file, ready to install, with the hash of the bytes that passed.
    Staged(StagedUpdate),
    /// Why the file must not be installed.
    Error(AzString),
}

impl From<Result<VerifiedArtifact, String>> for UpdateStageResult {
    fn from(result: Result<VerifiedArtifact, String>) -> Self {
        match result {
            Ok(artifact) => Self::Staged(StagedUpdate::from(&artifact)),
            Err(e) => Self::Error(e.into()),
        }
    }
}

/// Whether unattended update work may run at `now_unix` under the maintenance
/// window `rule` (the shared config's RRULE subset, in UTC; empty: any time).
/// A manager staging other programs passes the window its own config names;
/// [`within_shared_maintenance_window`] reads the running program's.
#[must_use]
pub fn within_maintenance_window(rule: &str, now_unix: u64) -> bool {
    if rule.trim().is_empty() {
        return true;
    }
    #[cfg(feature = "telemetry")]
    {
        crate::telemetry::sharedconfig::within_maintenance_window(rule, now_unix)
    }
    // The RRULE reader lives with the shared config; without it no window
    // can be read, and an unreadable window fails open as it does there.
    #[cfg(not(feature = "telemetry"))]
    {
        let _ = now_unix;
        true
    }
}

/// Draws a rollout cohort (0-99) the way [`UpdateState::rollout_bucket`]
/// does, `AZ_UPDATE_BUCKET` included. The caller keeps it: a cohort is
/// drawn once per machine and never again.
#[must_use]
pub fn draw_rollout_bucket() -> u8 {
    UpdateState::default().rollout_bucket()
}

// ---------------------------------------------------------------------------
// Async wrapper: `CallbackInfo::check_for_updates` — the C-API surface.
// ---------------------------------------------------------------------------

use azul_core::{
    callbacks::Update,
    refany::{OptionRefAny, RefAny},
    task::{ThreadId, ThreadReceiver},
};

use crate::thread::{
    Thread, ThreadCallbackType, ThreadReceiveMsg, ThreadSender, ThreadWriteBackMsg,
    WriteBackCallbackType,
};

/// Everything one completed update check knows, delivered to the app's
/// [`UpdateCheckCallback`] on the main thread.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct UpdateCheckInfo {
    /// What the check concluded.
    pub result: UpdateCheckResult,
    /// The requested mode CLAMPED by the detected install kind — when this
    /// says [`UpdateMode::NotifyOnly`] the app must not offer an "install"
    /// button, only "a new version is available" (+ the package-manager hint).
    pub effective_mode: UpdateMode,
    /// What kind of installation the running binary is.
    pub install: InstallKind,
    /// Path of the staged artifact when `download_automatically` staged one.
    /// Staging is NOT installing — hand this to [`apply_update`] only after
    /// the user consents.
    pub staged_path: azul_css::OptionString,
}

/// Callback type for [`spawn_update_check`] results: `(data, info, check)`,
/// invoked on the MAIN thread with full [`CallbackInfo`] access.
pub type UpdateCheckCallbackType =
    extern "C" fn(RefAny, crate::callbacks::CallbackInfo, UpdateCheckInfo) -> Update;

/// Wrapper carrying the app's check-result callback across the worker thread.
#[repr(C)]
pub struct UpdateCheckCallback {
    pub cb: UpdateCheckCallbackType,
    /// For FFI: stores the foreign callable (e.g., `PyFunction`)
    /// Native Rust code sets this to None
    pub ctx: OptionRefAny,
}

impl UpdateCheckCallback {
    /// Create a new `UpdateCheckCallback`
    pub fn new(cb: UpdateCheckCallbackType) -> Self {
        Self {
            cb,
            ctx: OptionRefAny::None,
        }
    }
}

impl core::fmt::Debug for UpdateCheckCallback {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "UpdateCheckCallback {{ cb: {:p} }}",
            self.cb as *const ()
        )
    }
}

impl Clone for UpdateCheckCallback {
    fn clone(&self) -> Self {
        Self {
            cb: self.cb,
            ctx: self.ctx.clone(),
        }
    }
}

impl From<UpdateCheckCallbackType> for UpdateCheckCallback {
    fn from(cb: UpdateCheckCallbackType) -> Self {
        Self {
            cb,
            ctx: OptionRefAny::None,
        }
    }
}

impl PartialEq for UpdateCheckCallback {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.cb as *const (), other.cb as *const ())
    }
}

impl Eq for UpdateCheckCallback {}

impl PartialOrd for UpdateCheckCallback {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for UpdateCheckCallback {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (self.cb as *const () as usize).cmp(&(other.cb as *const () as usize))
    }
}

impl core::hash::Hash for UpdateCheckCallback {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        (self.cb as *const () as usize).hash(state);
    }
}

azul_core::impl_managed_callback! {
    wrapper:        UpdateCheckCallback,
    info_ty:        crate::callbacks::CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: UPDATE_CHECK_CALLBACK_INVOKER,
    invoker_ty:     AzUpdateCheckCallbackInvoker,
    thunk_fn:       az_update_check_callback_thunk,
    setter_fn:      AzApp_setUpdateCheckCallbackInvoker,
    from_handle_fn: AzUpdateCheckCallback_createFromHostHandle,
    from_handle_byref_fn: AzUpdateCheckCallback_createFromHostHandleByref,
    extra_args:     [check: UpdateCheckInfo],
}

/// The worker's input, snapshotted at spawn time.
struct CheckTask {
    options: UpdateOptions,
    env: crate::appenv::AppEnv,
    callback: UpdateCheckCallback,
}

/// The worker's output, riding the `WriteBack` message.
struct CheckOutcome {
    callback: UpdateCheckCallback,
    info: UpdateCheckInfo,
}

/// The folder the platform keeps a user's program data in, from the environment as it is given:
/// `APPDATA` (Windows), `~/Library/Application Support` (macOS), `$XDG_DATA_HOME` or
/// `~/.local/share` (elsewhere). `None` when the variable is not set - or is not an absolute
/// path: an empty `HOME` would put the folder under whatever the working directory is.
fn data_home_from(
    home: Option<std::ffi::OsString>,
    xdg_data_home: Option<std::ffi::OsString>,
    appdata: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    let absolute = |v: std::ffi::OsString| Some(PathBuf::from(v)).filter(|p| p.is_absolute());
    if cfg!(target_os = "windows") {
        appdata.and_then(absolute)
    } else if cfg!(target_os = "macos") {
        home.and_then(absolute)
            .map(|h| h.join("Library").join("Application Support"))
    } else {
        xdg_data_home.and_then(absolute).or_else(|| {
            home.and_then(absolute)
                .map(|h| h.join(".local").join("share"))
        })
    }
}

/// [`data_home_from`] of this process's environment.
fn data_home() -> Option<PathBuf> {
    data_home_from(
        std::env::var_os("HOME"),
        std::env::var_os("XDG_DATA_HOME"),
        std::env::var_os("APPDATA"),
    )
}

/// Platform data directory for the updater's state
/// (`update-state.json` + the staging area), keyed by app name. Falls back to the shared
/// temporary folder when the platform's folder is not set: fine for what is only a convenience
/// (the problem reports), NOT for the marks that refuse a rollback or the staged update - see
/// [`private_state_dir`].
#[must_use]
pub fn default_state_dir(app_name: &str) -> PathBuf {
    data_home()
        .unwrap_or_else(std::env::temp_dir)
        .join(app_name)
}

/// The updater's state folder in the user's OWN data folder, or the reason there is none. The
/// shared temporary folder is never a fallback: another user of the computer can make
/// `/tmp/<app>` first, and the marks and the staged update in it would be theirs to choose
/// (X-DELTA-AZDRIVE-KEYS-33). What checks for an update or installs one goes through this.
///
/// # Errors
///
/// The platform's data folder is not set (`HOME`, `XDG_DATA_HOME` or `APPDATA`), or `app_name` is
/// not a single folder name.
pub fn private_state_dir(app_name: &str) -> Result<PathBuf, String> {
    private_state_dir_in(data_home(), app_name)
}

/// [`private_state_dir`] under `data_home` (`None` = the platform's folder is not set).
fn private_state_dir_in(data_home: Option<PathBuf>, app_name: &str) -> Result<PathBuf, String> {
    let single_name = !app_name.is_empty()
        && app_name != "."
        && app_name != ".."
        && !app_name.contains(['/', '\\', ':', '\0']);
    if !single_name {
        return Err(format!(
            "the app name {app_name:?} is not a folder name: the update state has no place"
        ));
    }
    data_home.map(|home| home.join(app_name)).ok_or_else(|| {
        "the user's data folder is not set (HOME, XDG_DATA_HOME or APPDATA): the update state is \
         not kept in a folder that other users of the computer can write"
            .to_owned()
    })
}

/// Runs one full check (+ optional staging) as an azul [`Thread`], reading
/// the manifest URL / current version / mode from the published
/// [`crate::appenv::AppEnv`]. The `data` `RefAny` is handed back VERBATIM to
/// `callback` on the main thread. `CallbackInfo::check_for_updates` is the
/// thin wrapper that also registers the thread with the event loop.
#[must_use]
pub fn spawn_update_check(
    data: RefAny,
    callback: UpdateCheckCallback,
    options: UpdateOptions,
) -> (ThreadId, Thread) {
    let task = CheckTask {
        options,
        env: crate::appenv::app_env(),
        callback,
    };
    let worker: ThreadCallbackType = update_check_worker;
    let thread = Thread::create(RefAny::new(task), data, worker);
    (ThreadId::unique(), thread)
}

/// `download_automatically`: STAGES the release a check found so a later "install now" is
/// instant. It installs nothing; a notify-only install never stages; an app with no way to prove
/// an update ([`crate::appenv::AppEnv::update_policy`]) stages nothing; and the staging is
/// UNATTENDED work, so it defers to the machine's maintenance window (a gated stage just happens
/// on a later check). The staged file's path when there is one.
fn stage_automatically(
    env: &crate::appenv::AppEnv,
    result: &UpdateCheckResult,
    options: UpdateOptions,
    effective: UpdateMode,
    now_unix: u64,
    state_dir: &Path,
) -> azul_css::OptionString {
    let (UpdateCheckResult::Available(release), true, UpdateMode::SelfUpdate) =
        (result, options.download_automatically, effective)
    else {
        return azul_css::OptionString::None;
    };
    if !within_shared_maintenance_window(now_unix) {
        return azul_css::OptionString::None;
    }
    let Ok(policy) = env.update_policy() else {
        return azul_css::OptionString::None;
    };
    let Ok(mut state) = UpdateState::read(state_dir) else {
        return azul_css::OptionString::None;
    };
    let expect = Expect::here(&env.app_name);
    let staged = download_and_verify(
        release,
        &expect,
        &state_dir.join("staging"),
        &policy,
        &mut state,
    );
    match (staged, state.try_save(state_dir)) {
        (Ok(artifact), Ok(())) => {
            azul_css::OptionString::Some(artifact.path().to_string_lossy().as_ref().into())
        }
        (Ok(artifact), Err(_)) => {
            drop(std::fs::remove_file(artifact.path()));
            azul_css::OptionString::None
        }
        (Err(_), _) => azul_css::OptionString::None,
    }
}

/// Background half: check → (optional) stage → one `WriteBack`.
extern "C" fn update_check_worker(
    mut init: RefAny,
    mut sender: ThreadSender,
    _recv: ThreadReceiver,
) {
    let Some(task) = init.downcast_ref::<CheckTask>() else {
        return;
    };

    let install = InstallKind::detect();
    let effective = apply_shared_update_policy(effective_mode(task.env.update_mode, &install));
    // The marks that refuse a rollback and the staged update live in the user's own folder; the
    // shared temporary folder is no place for them.
    let state_dir = private_state_dir(&task.env.app_name);
    let policy = task.env.update_policy();

    let result = match (task.env.update_manifest.as_deref(), effective, &policy) {
        (_, UpdateMode::Disabled, _) => {
            UpdateCheckResult::Error("updates are disabled (UpdateMode::Disabled)".into())
        }
        (None, _, _) => UpdateCheckResult::Error(
            "no update manifest configured (AppConfig.updates.manifest_url)".into(),
        ),
        // An app that replaces itself and has no way to prove an update says so before it asks
        // anyone for one; a notify-only app needs no proof for a notification.
        (Some(_), UpdateMode::SelfUpdate, Err(why)) => UpdateCheckResult::Error(why.clone().into()),
        (Some(url), _, _) => match &state_dir {
            Err(why) => UpdateCheckResult::Error(why.clone().into()),
            Ok(dir) => {
                let audience = if effective == UpdateMode::SelfUpdate {
                    UpdateAudience::AutoUpdate
                } else {
                    UpdateAudience::NotifyOnly
                };
                let mut state = UpdateState::load(dir);
                let r = check_for_updates_blocking(
                    url,
                    &task.env.current_version,
                    &task.env.update_channel,
                    &mut state,
                    audience,
                );
                state.save(dir);
                r
            }
        },
    };

    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let staged_path = match &state_dir {
        Ok(dir) => stage_automatically(&task.env, &result, task.options, effective, now_unix, dir),
        Err(_) => azul_css::OptionString::None,
    };

    let outcome = CheckOutcome {
        callback: task.callback.clone(),
        info: UpdateCheckInfo {
            result,
            effective_mode: effective,
            install,
            staged_path,
        },
    };
    drop(task);

    let writeback: WriteBackCallbackType = update_check_writeback;
    let _ = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
        writeback,
        RefAny::new(outcome),
    )));
}

/// Main-thread half: unwrap the outcome, invoke the app's callback.
extern "C" fn update_check_writeback(
    user_data: RefAny,
    mut outcome: RefAny,
    info: crate::callbacks::CallbackInfo,
) -> Update {
    let Some(o) = outcome.downcast_ref::<CheckOutcome>() else {
        return Update::DoNothing;
    };
    let callback = o.callback.clone();
    let check = o.info.clone();
    drop(o);
    callback.invoke(user_data, info, check)
}

#[cfg(test)]
mod tests {
    use core::cmp::Ordering;

    use super::*;

    #[test]
    fn version_compare_is_numeric_not_lexical() {
        assert_eq!(compare_versions("1.10.0", "1.9.9"), Ordering::Greater);
        assert_eq!(compare_versions("v1.2.3", "1.2.3"), Ordering::Equal);
        assert_eq!(compare_versions("1.2", "1.2.1"), Ordering::Less);
        assert_eq!(compare_versions("2.0.0", "10.0.0"), Ordering::Less);
    }

    /// AZ-Y11: `0-rc1` used to fail to parse and fall back to a lexical compare, so
    /// `2.0.0-rc1 > 2.0.0`. Fails on the old code at the first assertion.
    #[test]
    fn y11_a_pre_release_orders_below_its_final_and_semver_among_themselves() {
        assert_eq!(compare_versions("2.0.0-rc1", "2.0.0"), Ordering::Less);
        assert_eq!(compare_versions("2.0.0", "2.0.0-rc1"), Ordering::Greater);
        // The chain semver 2.0.0 gives in section 11, each below the next.
        let chain = [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
        ];
        for pair in chain.windows(2) {
            assert_eq!(
                compare_versions(pair[0], pair[1]),
                Ordering::Less,
                "{} < {}",
                pair[0],
                pair[1]
            );
            assert_eq!(
                compare_versions(pair[1], pair[0]),
                Ordering::Greater,
                "{} > {}",
                pair[1],
                pair[0]
            );
        }
        // The core decides before the pre-release does.
        assert_eq!(compare_versions("2.0.1-rc1", "2.0.0"), Ordering::Greater);
        assert_eq!(compare_versions("1.9.9", "2.0.0-rc1"), Ordering::Less);
        // Build metadata, the leading v and an empty pre-release do not count.
        assert_eq!(compare_versions("v2.0.0+abc", "2.0.0+def"), Ordering::Equal);
        assert_eq!(
            compare_versions("2.0.0-rc1+x", "2.0.0-rc1"),
            Ordering::Equal
        );
        assert_eq!(compare_versions("2.0.0-", "2.0.0"), Ordering::Equal);
    }

    #[test]
    fn manifest_parses_and_rejects_missing_version() {
        let ok = parse_manifest(
            r#"{"latest":{"version":"1.5.0","download_url":"https://x/y.tar.gz",
                "changelog_md":"https://x/CHANGELOG.md","digest":"sha256:ab"}}"#,
        )
        .expect("valid manifest");
        assert_eq!(ok.version.as_str(), "1.5.0");
        assert!(parse_manifest(r#"{"latest":{}}"#).is_err());
        assert!(parse_manifest("not json").is_err());
    }

    #[test]
    fn effective_mode_clamps_managed_installs_to_notify_only() {
        for managed in [
            InstallKind::SystemPackageManager,
            InstallKind::Snap,
            InstallKind::Flatpak,
            InstallKind::WindowsStore,
            InstallKind::ReadOnly,
        ] {
            assert_eq!(
                effective_mode(UpdateMode::SelfUpdate, &managed),
                UpdateMode::NotifyOnly,
                "{managed:?} must not self-update"
            );
        }
        assert_eq!(
            effective_mode(UpdateMode::SelfUpdate, &InstallKind::UserWritable),
            UpdateMode::SelfUpdate
        );
        assert_eq!(
            effective_mode(UpdateMode::Disabled, &InstallKind::UserWritable),
            UpdateMode::Disabled
        );
    }

    #[test]
    fn install_kind_detects_usr_and_windowsapps_paths() {
        assert_eq!(
            InstallKind::detect_for(Path::new("/usr/bin/azwriter")),
            InstallKind::SystemPackageManager
        );
        assert_eq!(
            InstallKind::detect_for(Path::new(
                "C:\\Program Files\\WindowsApps\\azwriter\\azwriter.exe"
            )),
            InstallKind::WindowsStore
        );
    }

    #[test]
    fn update_state_round_trips() {
        let dir = std::env::temp_dir().join(format!("azul-updater-test-{}", std::process::id()));
        let state = UpdateState {
            highest_seen: "1.5.0".to_owned(),
            last_check_unix: 1_700_000_000,
            suspended_until_unix: 1_700_100_000,
            rollout_bucket: Some(42),
            key_generation: 3,
            clock_floor_unix: 1_700_000_500,
        };
        state.save(&dir);
        assert_eq!(UpdateState::load(&dir), state);
        drop(std::fs::remove_dir_all(&dir));
    }

    // ---- update sources ---------------------------------------------------

    #[test]
    fn github_urls_in_every_spelling_normalize_to_the_api() {
        let expect = "https://api.github.com/repos/fschutt/azul/releases/latest";
        for spelling in [
            "github://fschutt/azul",
            "https://github.com/fschutt/azul",
            "https://github.com/fschutt/azul/",
            "https://github.com/fschutt/azul/releases",
            "https://github.com/fschutt/azul/releases/latest",
        ] {
            assert_eq!(
                normalize_update_url(spelling).fetch_url,
                expect,
                "{spelling} must resolve to the releases API"
            );
        }
        // An asset pattern rides through, percent-decoded.
        let n = normalize_update_url("github://fschutt/azul?asset=azul-%2A-linux.bin");
        assert_eq!(n.asset_pattern.as_deref(), Some("azul-*-linux.bin"));
        // A manifest URL is left ALONE — the abstraction must not rewrite
        // what it does not recognise.
        assert_eq!(
            normalize_update_url("https://ex.invalid/updates.json").fetch_url,
            "https://ex.invalid/updates.json"
        );
    }

    /// LAW: the shape is sniffed, and every supported shape yields a version.
    #[test]
    fn every_source_shape_is_recognised() {
        // 1. azul manifest v1
        let c = parse_release_document(
            r#"{"latest":{"version":"2.0.0","download_url":"http://x/a.bin"}}"#,
            None,
            "",
        )
        .unwrap();
        let r = &c[0];
        assert_eq!(r.kind, UpdateSourceKind::ManifestV1);
        assert_eq!(r.release.version.as_str(), "2.0.0");

        // 2. flat, hand-written JSON — `url` accepted as well as `download_url`
        let c = parse_release_document(r#"{"version":"v3.1","url":"http://x/b.bin"}"#, None, "")
            .unwrap();
        let r = &c[0];
        assert_eq!(r.kind, UpdateSourceKind::FlatJson);
        assert_eq!(r.release.version.as_str(), "3.1", "the v prefix must go");
        assert_eq!(r.release.download_url.as_str(), "http://x/b.bin");

        // 3. a bare version file on static hosting: notify-only
        let c = parse_release_document("v4.2.0\n", None, "").unwrap();
        let r = &c[0];
        assert_eq!(r.kind, UpdateSourceKind::PlainVersion);
        assert_eq!(r.release.version.as_str(), "4.2.0");
        assert!(
            r.release.download_url.as_str().is_empty(),
            "a version file cannot say where the artifact is"
        );

        // 4. anything else is an ERROR, not a silent "up to date" — an HTML error page must not
        //    look like "no update available".
        let err = parse_release_document("<html><body>404</body></html>", None, "").unwrap_err();
        assert!(err.contains("neither JSON nor a version"), "{err}");
        assert!(parse_release_document("", None, "").is_err());
        assert!(parse_release_document(r#"{"whatever":1}"#, None, "").is_err());
    }

    /// X-DELTA-AZDRIVE-KEYS-34: the error quoted the first 40 BYTES of the body, and a body with a
    /// multi-byte character across byte 40 made `&trimmed[..40]` panic - and a panic aborts the
    /// app, on every check. Red on the old code: the call panics instead of returning the error.
    #[test]
    fn a_body_with_a_multibyte_character_at_the_cut_is_an_error_not_a_panic() {
        // 39 ASCII bytes, then a three-byte character: byte 40 is inside it.
        let body = format!("{}\u{20ac}tail", "A".repeat(39));
        let err = parse_release_document(&body, None, "").unwrap_err();
        assert!(err.contains("neither JSON nor a version"), "{err}");
        assert!(
            err.contains(&"A".repeat(39)),
            "the answer names how it starts: {err}"
        );
        // The same for a body that is only multi-byte characters, and for the replacement
        // character a lossy decode of invalid bytes leaves.
        for body in [
            "\u{20ac}".repeat(30),
            format!("{}\u{fffd}", "B".repeat(39)),
            format!("{}\u{1f980}", "x".repeat(38)),
        ] {
            let err = parse_release_document(&body, None, "stable").unwrap_err();
            assert!(err.contains("neither JSON nor a version"), "{err}");
        }
        // A body shorter than the cut is quoted whole.
        let err = parse_release_document("<html>", None, "").unwrap_err();
        assert!(err.contains("<html>"), "{err}");
    }

    fn gh_asset(name: &str, digest: Option<&str>) -> serde_json::Value {
        serde_json::json!({
            "name": name,
            "browser_download_url": format!("https://dl.invalid/{name}"),
            "digest": digest,
        })
    }

    #[test]
    fn a_github_release_becomes_a_release_info() {
        let target = format!(
            "app-2.0.0-{}-{}.bin",
            std::env::consts::ARCH,
            std::env::consts::OS
        );
        let release = serde_json::json!({
            "tag_name": "v2.0.0",
            "published_at": "2027-01-01T00:00:00Z",
            "prerelease": false,
            "body": "# 2.0.0\n\n- a change",
            "assets": [
                gh_asset("app-2.0.0-somethingelse.bin", None),
                gh_asset(&target, Some("sha256:abc")),
                gh_asset(&format!("{target}.minisig"), None),
                gh_asset("signing-key-statement.txt", None),
                gh_asset("signing-key-statement.txt.minisig", None),
                gh_asset("release-statement.txt", None),
                gh_asset("release-statement.txt.minisig", None),
            ],
        });
        let r = parse_github_release(&release, None).unwrap();
        let sidecars = &r.sidecars;
        assert_eq!(r.kind, UpdateSourceKind::GitHubRelease);
        assert_eq!(r.release.version.as_str(), "2.0.0");
        assert_eq!(
            r.release.download_url.as_str(),
            format!("https://dl.invalid/{target}"),
            "the asset for THIS platform must win"
        );
        assert_eq!(r.release.digest.as_str(), "sha256:abc");
        assert!(
            r.release.changelog_md_inline.as_str().contains("a change"),
            "the release body IS the changelog"
        );
        // published_at seeds the rollout ladder without any extra fields.
        assert!(
            matches!(r.rollout, RolloutPlan::Staged(_)),
            "published_at must give the default ladder"
        );
        // The signature chain rides as sibling assets.
        let fields: Vec<SidecarField> = sidecars.iter().map(|s| s.field).collect();
        assert!(fields.contains(&SidecarField::Signature));
        assert!(fields.contains(&SidecarField::Statement));
        assert!(fields.contains(&SidecarField::StatementSig));
        // ... and so does the signed release statement that binds the artifact to its version.
        assert!(fields.contains(&SidecarField::ReleaseStatement));
        assert!(fields.contains(&SidecarField::ReleaseStatementSig));
    }

    /// AZ-Y3: a release statement names one artifact's platform and hash, so a GitHub release with an
    /// asset per platform has one statement per asset; the asset of THIS platform's own statement
    /// is the one read, and the shared name is only for a release with a single artifact.
    #[test]
    fn y3_a_github_release_reads_the_statement_of_its_own_asset() {
        let target = format!(
            "app-2.0.0-{}-{}.bin",
            std::env::consts::ARCH,
            std::env::consts::OS
        );
        let release = serde_json::json!({
            "tag_name": "v2.0.0",
            "assets": [
                gh_asset("app-2.0.0-somethingelse.bin", None),
                gh_asset("app-2.0.0-somethingelse.bin.release-statement.txt", None),
                gh_asset("app-2.0.0-somethingelse.bin.release-statement.txt.minisig", None),
                gh_asset(&target, None),
                gh_asset(&format!("{target}.release-statement.txt"), None),
                gh_asset(&format!("{target}.release-statement.txt.minisig"), None),
                gh_asset("release-statement.txt", None),
                gh_asset("release-statement.txt.minisig", None),
            ],
        });
        let r = parse_github_release(&release, None).unwrap();
        let url_of = |field: SidecarField| {
            r.sidecars
                .iter()
                .find(|s| s.field == field)
                .map(|s| s.url.clone())
                .expect("the sidecar is there")
        };
        assert_eq!(
            url_of(SidecarField::ReleaseStatement),
            format!("https://dl.invalid/{target}.release-statement.txt"),
            "the statement of the asset this platform downloads, not another platform's and not the shared one"
        );
        assert_eq!(
            url_of(SidecarField::ReleaseStatementSig),
            format!("https://dl.invalid/{target}.release-statement.txt.minisig")
        );
    }

    /// LAW: with no asset for this platform we still report the VERSION but
    /// refuse to nominate a download — handing the user an arbitrary binary
    /// is worse than telling them to fetch it themselves.
    #[test]
    fn an_unmatched_platform_notifies_but_never_guesses() {
        let release = serde_json::json!({
            "tag_name": "9.9.9",
            "assets": [gh_asset("app-for-some-other-os.bin", None)],
        });
        let r = parse_github_release(&release, None).unwrap();
        assert_eq!(r.release.version.as_str(), "9.9.9");
        assert!(r.release.download_url.as_str().is_empty());
    }

    #[test]
    fn signatures_and_checksums_are_never_mistaken_for_the_artifact() {
        let os = std::env::consts::OS;
        let assets = vec![
            gh_asset(&format!("app-{os}.bin.minisig"), None),
            gh_asset(&format!("app-{os}.bin.sha256"), None),
            gh_asset("SHA256SUMS", None),
            gh_asset(&format!("app-{os}.bin"), None),
        ];
        let picked = select_github_asset(&assets, None).expect("an artifact must be picked");
        assert_eq!(
            picked.get("name").unwrap().as_str().unwrap(),
            format!("app-{os}.bin")
        );
        // An explicit pattern wins outright, globs included.
        let picked = select_github_asset(&assets, Some("*.sha256")).unwrap();
        assert!(picked
            .get("name")
            .unwrap()
            .as_str()
            .unwrap()
            .ends_with(".sha256"));
    }

    /// GitHub leaves `digest` null on older uploads, so a checksum file has
    /// to be readable in both common layouts.
    #[test]
    fn checksum_files_are_read_in_both_layouts() {
        assert_eq!(
            parse_checksum_file(&"a".repeat(64), "http://x/app.bin").as_deref(),
            Some("a".repeat(64).as_str()),
            "a bare .sha256 file"
        );
        let sums = format!(
            "{}  other.bin\n{}  app.bin\n",
            "b".repeat(64),
            "c".repeat(64)
        );
        assert_eq!(
            parse_checksum_file(&sums, "http://x/app.bin"),
            Some("c".repeat(64)),
            "SHA256SUMS must be matched by FILENAME, not by position"
        );
        assert_eq!(parse_checksum_file("nonsense", "http://x/app.bin"), None);
    }

    #[test]
    fn a_releases_list_skips_drafts_and_prereleases() {
        let list = serde_json::json!([
            {"tag_name": "3.0.0-rc1", "prerelease": true, "assets": []},
            {"tag_name": "2.9.0", "draft": true, "assets": []},
            {"tag_name": "2.8.0", "prerelease": false, "assets": []},
        ]);
        let c = parse_release_document(&list.to_string(), None, "").unwrap();
        let r = &c[0];
        assert_eq!(
            r.release.version.as_str(),
            "2.8.0",
            "a prerelease must never be offered as the latest stable"
        );
    }

    #[test]
    fn oci_urls_split_registry_repo_and_tag() {
        let n = normalize_update_url("oci://ghcr.io/fschutt/azul-app:2.0.0");
        let oci = n.oci.expect("an oci:// URL must be recognised");
        assert_eq!(oci.registry, "ghcr.io");
        assert_eq!(oci.repository, "fschutt/azul-app");
        assert_eq!(oci.reference, "2.0.0");
        assert_eq!(
            n.fetch_url,
            "https://ghcr.io/v2/fschutt/azul-app/manifests/2.0.0"
        );
        // No tag means `latest`; a port in the registry must not be read as one.
        let n = normalize_update_url("oci://registry.example.com:5000/team/app");
        let oci = n.oci.unwrap();
        assert_eq!(oci.registry, "registry.example.com:5000");
        assert_eq!(oci.repository, "team/app");
        assert_eq!(oci.reference, "latest");
    }

    #[test]
    fn registry_challenges_are_read() {
        let (realm, service, scope) = parse_www_authenticate(
            r#"Bearer realm="https://ghcr.io/token",service="ghcr.io",scope="repository:o/r:pull""#,
        )
        .expect("a standard challenge must parse");
        assert_eq!(realm, "https://ghcr.io/token");
        assert_eq!(service, "ghcr.io");
        assert_eq!(scope, "repository:o/r:pull");
        assert!(parse_www_authenticate("Basic realm=\"x\"").is_none());
    }

    /// LAW: an OCI release is digest-pinned BY CONSTRUCTION — the layer
    /// digest the registry reports is exactly the artifact's sha256, so
    /// there is no separate checksum to publish or forget.
    #[test]
    fn an_oci_manifest_becomes_a_digest_pinned_release() {
        let oci = OciRef {
            registry: "ghcr.io".to_owned(),
            repository: "o/app".to_owned(),
            reference: "latest".to_owned(),
        };
        let manifest = serde_json::json!({
            "schemaVersion": 2,
            "annotations": {
                "org.opencontainers.image.version": "v3.4.5",
                "org.opencontainers.image.created": "2027-01-01T00:00:00Z",
            },
            "layers": [
                {"mediaType": "application/vnd.azul.app.layer.v1+tar", "digest": "sha256:dead", "size": 10},
            ],
        });
        let r = parse_oci_manifest(&manifest, &oci, None).unwrap();
        assert_eq!(r.kind, UpdateSourceKind::OciRegistry);
        assert_eq!(r.release.version.as_str(), "3.4.5", "the v prefix must go");
        assert_eq!(
            r.release.download_url.as_str(),
            "https://ghcr.io/v2/o/app/blobs/sha256:dead"
        );
        assert_eq!(r.release.digest.as_str(), "sha256:dead");
        assert!(
            matches!(r.rollout, RolloutPlan::Staged(_)),
            "created -> ladder"
        );

        // `latest` with no version annotation cannot name a version, and
        // saying "you are up to date" there would be a lie.
        let bare = serde_json::json!({"layers": []});
        assert!(parse_oci_manifest(&bare, &oci, None).is_err());
        // …but an explicit tag IS the version.
        let tagged = OciRef {
            reference: "2.1.0".to_owned(),
            ..oci
        };
        assert_eq!(
            parse_oci_manifest(&bare, &tagged, None)
                .unwrap()
                .release
                .version
                .as_str(),
            "2.1.0"
        );
    }

    /// LAW: a multi-arch index is followed to THIS platform, and a staged
    /// artifact's filename is legal on every OS (an OCI blob URL ends in
    /// `sha256:…`, and `:` is not a legal Windows filename character).
    #[test]
    fn an_index_resolves_to_this_platform_and_blob_names_stay_portable() {
        let want_os = match std::env::consts::OS {
            "macos" => "darwin",
            other => other,
        };
        let want_arch = match std::env::consts::ARCH {
            "x86_64" => "amd64",
            "aarch64" => "arm64",
            other => other,
        };
        let index = serde_json::json!({
            "manifests": [
                {"digest": "sha256:wrong", "platform": {"os": "plan9", "architecture": "vax"}},
                {"digest": "sha256:right", "platform": {"os": want_os, "architecture": want_arch}},
            ]
        });
        assert_eq!(
            select_index_manifest(&index).as_deref(),
            Some("sha256:right")
        );
        // A plain manifest is not an index.
        assert!(select_index_manifest(&serde_json::json!({"layers": []})).is_none());
        // An index with nothing for us must not pick something at random.
        let foreign = serde_json::json!({
            "manifests": [{"digest": "sha256:x", "platform": {"os": "plan9", "architecture": "vax"}}]
        });
        assert!(select_index_manifest(&foreign).is_none());
    }

    /// A source may legitimately know a version without knowing a download
    /// (a VERSION file, an unmatched platform). That must read as such.
    #[test]
    fn downloading_without_an_artifact_url_says_so() {
        let release = ReleaseInfo {
            version: "5.0.0".into(),
            ..ReleaseInfo::default()
        };
        let err = download_update(&release, std::path::Path::new("/tmp"))
            .expect_err("an empty download URL must be refused");
        assert!(err.contains("names no download"), "{err}");
        assert!(
            err.contains("5.0.0"),
            "the message must name the version: {err}"
        );
    }

    // ---- channels + the eligibility walk ----------------------------------

    fn candidate(version: &str, rollout: RolloutPlan) -> ResolvedRelease {
        ResolvedRelease {
            release: ReleaseInfo {
                version: version.into(),
                ..ReleaseInfo::default()
            },
            rollout,
            kind: UpdateSourceKind::ManifestV1,
            sidecars: Vec::new(),
        }
    }

    /// LAW: a client takes the NEWEST release it is eligible for — never
    /// nothing.
    ///
    /// The bug this pins: with one release per manifest, publishing 2.1.0
    /// while 2.0.0 was mid-rollout reset the gate. A client already inside
    /// 2.0.0's cohort saw only 2.1.0, was gated out, and sat on 1.0.0 until
    /// the new ladder caught up — held back BY an update.
    #[test]
    fn a_newer_gated_release_never_holds_back_an_older_eligible_one() {
        // A ladder is COMPLETE once its last stage opens, so "still rolling
        // out" needs a stage in the future.
        let now = REL + 2 * DAY;
        // 2.0.0: both stages past => fully open.
        let open = RolloutPlan::Staged(vec![
            RolloutStage {
                percent: 10,
                at_unix: REL,
            },
            RolloutStage {
                percent: 100,
                at_unix: REL + DAY,
            },
        ]);
        // 2.1.0: just published, 10% open, 100% still days away.
        let fresh = RolloutPlan::Staged(vec![
            RolloutStage {
                percent: 10,
                at_unix: now,
            },
            RolloutStage {
                percent: 100,
                at_unix: now + 4 * DAY,
            },
        ]);
        let candidates = vec![candidate("2.0.0", open), candidate("2.1.0", fresh.clone())];

        // Bucket 50: outside 2.1.0's 10% cohort, inside 2.0.0's.
        let chosen =
            select_update_candidate(&candidates, "1.0.0", 50, UpdateAudience::AutoUpdate, now)
                .expect("an eligible release exists and must be offered");
        assert_eq!(
            chosen.release.version.as_str(),
            "2.0.0",
            "the client must move to the newest release its cohort is open for"
        );

        // Bucket 5 IS inside 2.1.0's cohort and must get the newer one.
        let chosen =
            select_update_candidate(&candidates, "1.0.0", 5, UpdateAudience::AutoUpdate, now)
                .unwrap();
        assert_eq!(chosen.release.version.as_str(), "2.1.0");

        // Nothing open at all => None, which the caller reports as
        // `staggered` rather than as an error.
        let all_gated = vec![candidate("2.1.0", fresh)];
        assert!(
            select_update_candidate(&all_gated, "1.0.0", 50, UpdateAudience::AutoUpdate, now)
                .is_none()
        );

        // Already current => nothing to offer.
        assert!(
            select_update_candidate(&candidates, "2.1.0", 5, UpdateAudience::AutoUpdate, now)
                .is_none()
        );
    }

    /// Notify-only installs wait for 100%, per release.
    #[test]
    fn notify_only_waits_for_the_rollout_to_finish() {
        let half = RolloutPlan::Staged(vec![
            RolloutStage {
                percent: 50,
                at_unix: REL,
            },
            RolloutStage {
                percent: 100,
                at_unix: REL + 4 * DAY,
            },
        ]);
        let done = RolloutPlan::Staged(vec![RolloutStage {
            percent: 100,
            at_unix: REL,
        }]);
        let candidates = vec![candidate("2.0.0", done), candidate("2.1.0", half)];
        let chosen = select_update_candidate(
            &candidates,
            "1.0.0",
            0,
            UpdateAudience::NotifyOnly,
            REL + DAY,
        )
        .unwrap();
        assert_eq!(
            chosen.release.version.as_str(),
            "2.0.0",
            "a half-rolled-out release is not announced to notify-only installs"
        );
    }

    /// LAW: the binary decides which channel it reads.
    #[test]
    fn the_channel_selects_which_releases_are_visible() {
        let manifest = r#"{
            "channels": {
                "stable": {"version": "2.0.0", "download_url": "http://x/s.bin"},
                "beta":   [
                    {"version": "2.1.0-rc1", "download_url": "http://x/b1.bin"},
                    {"version": "2.2.0-rc2", "download_url": "http://x/b2.bin"}
                ]
            },
            "latest": {"version": "2.0.0", "download_url": "http://x/s.bin"}
        }"#;
        // Empty channel means stable.
        for name in ["", "stable"] {
            let c = parse_release_document(manifest, None, name).unwrap();
            assert_eq!(c.len(), 1, "stable has one release");
            assert_eq!(c[0].release.version.as_str(), "2.0.0");
        }
        let beta = parse_release_document(manifest, None, "beta").unwrap();
        assert_eq!(beta.len(), 2, "a channel may list several releases");
        assert_eq!(beta[1].release.version.as_str(), "2.2.0-rc2");
        // An unknown channel falls back to latest rather than failing —
        // a typo must not silently stop updates forever.
        let unknown = parse_release_document(manifest, None, "typo").unwrap();
        assert_eq!(unknown[0].release.version.as_str(), "2.0.0");
    }

    /// A manifest may list a HISTORY, which is what makes the walk possible.
    #[test]
    fn a_manifest_may_carry_several_releases() {
        let manifest = r#"{
            "latest": {"version": "2.1.0", "download_url": "http://x/b.bin"},
            "releases": [{"version": "2.0.0", "download_url": "http://x/a.bin"}]
        }"#;
        let c = parse_release_document(manifest, None, "").unwrap();
        assert_eq!(c.len(), 2);
        let versions: Vec<&str> = c.iter().map(|r| r.release.version.as_str()).collect();
        assert!(versions.contains(&"2.0.0") && versions.contains(&"2.1.0"));
    }

    /// On the stable channel a GitHub prerelease is invisible; on any other
    /// channel it is the whole point.
    #[test]
    fn prereleases_belong_to_non_stable_channels() {
        let list = serde_json::json!([
            {"tag_name": "3.0.0-rc1", "prerelease": true, "assets": []},
            {"tag_name": "2.8.0", "prerelease": false, "assets": []},
        ])
        .to_string();
        let stable = parse_release_document(&list, None, "stable").unwrap();
        assert_eq!(stable.len(), 1);
        assert_eq!(stable[0].release.version.as_str(), "2.8.0");

        let beta = parse_release_document(&list, None, "beta").unwrap();
        assert_eq!(beta.len(), 2, "a beta channel sees prereleases too");
    }

    // ---- signature chain --------------------------------------------------

    /// Everything a chain test needs: a ROOT keypair, a SIGNING keypair, a
    /// statement delegating root→signing, and a signed artifact on disk.
    struct ChainFixture {
        root_pub_b64: String,
        /// The signing key pair (a test signs more than the fixture's own statements with it).
        signing: minisign::KeyPair,
        release: ReleaseInfo,
        artifact: std::path::PathBuf,
        _dir: std::path::PathBuf,
    }

    /// The app every fixture release is for.
    const FIXTURE_APP: &str = "fixture-app";
    /// When the fixture's releases were made (before every `now` a test uses).
    const FIXTURE_RELEASED: u64 = 1_700_000_000;
    /// The bytes of the fixture's artifact.
    const FIXTURE_BYTES: &[u8] = b"the update artifact bytes";

    /// What a client of the fixture's app on this platform expects.
    fn expect() -> Expect {
        Expect::here(FIXTURE_APP)
    }

    /// A release statement's text.
    fn release_statement_text(
        app: &str,
        version: &str,
        platform: &str,
        sha256: &[u8; 32],
        released: u64,
        min_generation: u64,
    ) -> String {
        format!(
            "azul-release-v1|app={app}|version={version}|platform={platform}|sha256={}|released={released}|min_generation={min_generation}",
            sha256_hex(sha256)
        )
    }

    /// `text` signed by the fixture's SIGNING key.
    fn signed_by_the_signing_key(f: &ChainFixture, text: &str) -> String {
        minisign::sign(
            Some(&f.signing.pk),
            &f.signing.sk,
            std::io::Cursor::new(text.as_bytes()),
            Some("azul release statement"),
            None,
        )
        .unwrap()
        .to_string()
    }

    /// The fixture's release with `text` as its release statement, signed by the signing key.
    fn with_release_statement(f: &ChainFixture, text: &str) -> ReleaseInfo {
        ReleaseInfo {
            release_statement: text.to_owned().into(),
            release_statement_sig: signed_by_the_signing_key(f, text).into(),
            ..f.release.clone()
        }
    }

    /// Signs with the REAL `minisign` crate so `minisign-verify` is checked
    /// against an independent implementation of the format, not itself.
    fn chain_fixture(expires: u64, generation: u64, tag: &str) -> ChainFixture {
        let root = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let signing = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let statement = format!(
            "azul-signing-key-v1|pubkey={}|expires={expires}|generation={generation}",
            signing.pk.to_base64()
        );
        let statement_sig = minisign::sign(
            Some(&root.pk),
            &root.sk,
            std::io::Cursor::new(statement.as_bytes()),
            Some("azul signing-key statement"),
            None,
        )
        .unwrap()
        .to_string();

        let dir = std::env::temp_dir().join(format!("azul-sigchain-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let artifact = dir.join("update-2.0.0.bin");
        std::fs::write(&artifact, FIXTURE_BYTES).unwrap();
        let artifact_sig = minisign::sign(
            Some(&signing.pk),
            &signing.sk,
            std::io::Cursor::new(FIXTURE_BYTES),
            Some("azul release 2.0.0"),
            None,
        )
        .unwrap()
        .to_string();
        let release_statement = release_statement_text(
            FIXTURE_APP,
            "2.0.0",
            &platform_label(),
            &sha256_of(FIXTURE_BYTES),
            FIXTURE_RELEASED,
            generation,
        );
        let release_statement_sig = minisign::sign(
            Some(&signing.pk),
            &signing.sk,
            std::io::Cursor::new(release_statement.as_bytes()),
            Some("azul release statement"),
            None,
        )
        .unwrap()
        .to_string();

        ChainFixture {
            root_pub_b64: root.pk.to_base64(),
            signing,
            release: ReleaseInfo {
                version: "2.0.0".into(),
                download_url: "http://x/update-2.0.0.bin".into(),
                changelog_md_url: "".into(),
                changelog_md_inline: "".into(),
                digest: "".into(),
                signature: artifact_sig.into(),
                signing_key_statement: statement.into(),
                signing_key_statement_sig: statement_sig.into(),
                release_statement: release_statement.into(),
                release_statement_sig: release_statement_sig.into(),
                download_headers: Vec::new().into(),
            },
            artifact,
            _dir: dir,
        }
    }

    const CHAIN_NOW: u64 = 1_800_000_000;

    fn roots_of(f: &ChainFixture) -> RootKeys {
        RootKeys::from_text(&f.root_pub_b64).expect("the fixture's root key parses")
    }

    fn sha256_of(bytes: &[u8]) -> [u8; 32] {
        use sha2::Digest as _;
        let mut out = [0u8; 32];
        out.copy_from_slice(&sha2::Sha256::digest(bytes));
        out
    }

    #[test]
    fn signature_chain_verifies_and_advances_the_generation_high_water() {
        let f = chain_fixture(CHAIN_NOW + DAY, 3, "ok");
        let mut state = UpdateState::default();
        let sha256 = verify_release_signature(
            &f.artifact,
            &f.release,
            &roots_of(&f),
            &expect(),
            &mut state,
            CHAIN_NOW,
        )
        .expect("a well-formed chain must verify");
        assert_eq!(
            state.key_generation, 3,
            "success must advance the high-water mark"
        );
        assert_eq!(
            sha256,
            sha256_of(b"the update artifact bytes"),
            "the proof is the hash of the very bytes that passed"
        );
        drop(std::fs::remove_dir_all(&f._dir));
    }

    #[test]
    fn signature_chain_refuses_tampering_expiry_rollback_and_unsigned() {
        let f = chain_fixture(CHAIN_NOW + DAY, 3, "sab");
        let mut state = UpdateState::default();

        // (a) tampered artifact
        std::fs::write(&f.artifact, b"EVIL bytes").unwrap();
        let err = verify_release_signature(
            &f.artifact,
            &f.release,
            &roots_of(&f),
            &expect(),
            &mut state,
            CHAIN_NOW,
        )
        .expect_err("tampered artifact MUST fail");
        assert!(
            err.contains("artifact signature"),
            "wrong link blamed: {err}"
        );
        assert_eq!(
            state.key_generation, 0,
            "a failed chain must not advance the mark"
        );
        std::fs::write(&f.artifact, b"the update artifact bytes").unwrap();

        // (b) expired statement
        let err = verify_release_signature(
            &f.artifact,
            &f.release,
            &roots_of(&f),
            &expect(),
            &mut state,
            CHAIN_NOW + 2 * DAY,
        )
        .expect_err("expired statement MUST fail");
        assert!(err.contains("expired"), "wrong link blamed: {err}");

        // (c) generation rollback
        let mut rolled = UpdateState {
            key_generation: 9,
            ..UpdateState::default()
        };
        let err = verify_release_signature(
            &f.artifact,
            &f.release,
            &roots_of(&f),
            &expect(),
            &mut rolled,
            CHAIN_NOW,
        )
        .expect_err("generation rollback MUST fail");
        assert!(err.contains("ROLLBACK"), "wrong link blamed: {err}");
        assert_eq!(rolled.key_generation, 9);

        // (d) statement not signed by the ROOT key: swap in a foreign root.
        let foreign = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let err = verify_release_signature(
            &f.artifact,
            &f.release,
            &RootKeys::from_text(&foreign.pk.to_base64()).expect("a foreign key parses"),
            &expect(),
            &mut state,
            CHAIN_NOW,
        )
        .expect_err("statement by a non-root key MUST fail");
        assert!(err.contains("statement"), "wrong link blamed: {err}");

        // (e) armed root + unsigned manifest = hard error, never a fallback.
        let unsigned = ReleaseInfo {
            signature: "".into(),
            signing_key_statement: "".into(),
            signing_key_statement_sig: "".into(),
            ..f.release.clone()
        };
        let err = verify_release_signature(
            &f.artifact,
            &unsigned,
            &roots_of(&f),
            &expect(),
            &mut state,
            CHAIN_NOW,
        )
        .expect_err("unsigned release with an armed root MUST fail");
        assert!(err.contains("UNSIGNED"), "wrong link blamed: {err}");
        drop(std::fs::remove_dir_all(&f._dir));
    }

    #[test]
    fn signing_key_statement_parse_is_strict() {
        let ok = parse_signing_key_statement(
            "azul-signing-key-v1|pubkey=RWQxyz|expires=1800000000|generation=4",
        )
        .unwrap();
        assert_eq!(
            ok,
            SigningKeyStatement {
                pubkey_b64: "RWQxyz".to_owned(),
                expires_unix: 1_800_000_000,
                generation: 4,
            }
        );
        // Every deviation is an error, not a default.
        assert!(
            parse_signing_key_statement("azul-signing-key-v2|pubkey=a|expires=1|generation=1")
                .is_err()
        );
        assert!(parse_signing_key_statement("azul-signing-key-v1|pubkey=a|expires=1").is_err());
        assert!(parse_signing_key_statement(
            "azul-signing-key-v1|pubkey=a|expires=soon|generation=1"
        )
        .is_err());
        assert!(parse_signing_key_statement(
            "azul-signing-key-v1|pubkey=a|expires=1|generation=1|extra=x"
        )
        .is_err());
    }

    // ---- REVIEW Y1: a default install authenticates no update ---------------------------

    /// An UNSIGNED release whose digest is exactly what the server sends: what answers at an
    /// update address nobody checked (a mis-issued certificate, an http MITM, a bucket).
    fn unsigned_release_on_a_server(f: &ChainFixture) -> ReleaseInfo {
        let bytes = b"the update artifact bytes".to_vec();
        ReleaseInfo {
            signature: "".into(),
            signing_key_statement: "".into(),
            signing_key_statement_sig: "".into(),
            digest: format!("sha256:{}", sha256_hex(&sha256_of(&bytes))).into(),
            download_url: serve_bytes(bytes, "app-2.0.0.bin").into(),
            ..f.release.clone()
        }
    }

    fn files_in(dir: &Path) -> usize {
        std::fs::read_dir(dir).map_or(0, Iterator::count)
    }

    #[test]
    fn y1_a_default_install_authenticates_no_update_through_the_managers_staging() {
        let f = chain_fixture(CHAIN_NOW * 2, 1, "y1-manager");
        let release = unsigned_release_on_a_server(&f);
        let staging = f._dir.join("staging");
        let staged = stage_program_update(
            &release,
            FIXTURE_APP,
            &staging,
            "",
            false,
            &f._dir.join("state"),
        );
        assert!(
            staged.is_err(),
            "an unsigned release and no root key must be refused, it was staged: {staged:?}"
        );
        assert_eq!(files_in(&staging), 0, "nothing refused stays staged");
        drop(std::fs::remove_dir_all(&f._dir));
    }

    /// The worker's and the dialog's call: the policy comes from the app's own environment, and an
    /// app that carries no key has none.
    #[test]
    fn y1_a_default_install_authenticates_no_update_through_the_apps_worker() {
        let f = chain_fixture(CHAIN_NOW * 2, 1, "y1-worker");
        let release = unsigned_release_on_a_server(&f);
        let staging = f._dir.join("staging");
        let env = crate::appenv::AppEnv::default();
        let policy = env.update_policy();
        assert_eq!(
            policy,
            Err(NO_ROOT_KEY.to_owned()),
            "an app with no root key and no allow_unsigned has no way to prove an update"
        );
        // And the unsigned release is refused even under a policy that does have a key.
        let signed = UpdatePolicy::Signed(roots_of(&f));
        let mut state = UpdateState::default();
        let staged = download_and_verify(&release, &expect(), &staging, &signed, &mut state);
        assert!(
            staged.is_err(),
            "an unsigned release under a signed policy must be refused, it was staged: {staged:?}"
        );
        assert_eq!(files_in(&staging), 0, "nothing refused stays staged");
        drop(std::fs::remove_dir_all(&f._dir));
    }

    #[test]
    fn y1_an_app_that_carries_no_key_stages_nothing_on_its_own() {
        let f = chain_fixture(CHAIN_NOW * 2, 1, "y1-auto");
        let release = unsigned_release_on_a_server(&f);
        let state_dir = f._dir.join("state");
        let options = UpdateOptions {
            download_automatically: true,
        };
        let found = UpdateCheckResult::Available(release);
        let staged = stage_automatically(
            &crate::appenv::AppEnv::default(),
            &found,
            options,
            UpdateMode::SelfUpdate,
            CHAIN_NOW,
            &state_dir,
        );
        assert_eq!(staged, azul_css::OptionString::None);
        assert!(
            !state_dir.join("staging").exists(),
            "not a byte was fetched"
        );
        drop(std::fs::remove_dir_all(&f._dir));
    }

    #[test]
    fn x_ar5b_25_an_empty_root_key_is_not_a_key() {
        for empty in ["", "  ", "\n", " , ; "] {
            assert!(RootKey::from_base64(empty).is_err(), "{empty:?}");
            assert!(RootKeys::from_text(empty).is_err(), "{empty:?}");
            assert_eq!(
                UpdatePolicy::from_text(empty, false),
                Err(NO_ROOT_KEY.to_owned()),
                "{empty:?}: an app with no key and no allow_unsigned cannot update"
            );
        }
        // The app SPELLS OUT that it wants the digest pin alone.
        assert_eq!(
            UpdatePolicy::from_text("", true),
            Ok(UpdatePolicy::DigestOnly)
        );
        // A key that is no key never falls back to "no check", allow_unsigned or not.
        for allow in [false, true] {
            assert!(UpdatePolicy::from_text("not a key", allow).is_err());
            assert!(UpdatePolicy::from_text("RWQ", allow).is_err());
        }
        // A key beside allow_unsigned stays a signed policy: the flag never lowers a chain.
        let f = chain_fixture(CHAIN_NOW + DAY, 1, "x25");
        let policy = UpdatePolicy::from_text(&f.root_pub_b64, true).expect("a key parses");
        assert!(policy.is_signed());
        drop(std::fs::remove_dir_all(&f._dir));
    }

    #[test]
    fn x_ar5b_25_the_current_and_the_next_root_key_both_sign() {
        let f = chain_fixture(CHAIN_NOW + DAY, 2, "x25-next");
        let other = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let both = RootKeys::from_text(&format!("{}, {}", other.pk.to_base64(), f.root_pub_b64))
            .expect("two keys");
        assert_eq!(both.len(), 2);
        let mut state = UpdateState::default();
        verify_release_signature(
            &f.artifact,
            &f.release,
            &both,
            &expect(),
            &mut state,
            CHAIN_NOW,
        )
        .expect("the second key signed the statement");
        // A typo in one of them is an error, not a list of one.
        assert!(RootKeys::from_text(&format!("{} oops", f.root_pub_b64)).is_err());
        // Neither of two unrelated keys signed it.
        let strangers = RootKeys::from_text(&format!(
            "{} {}",
            other.pk.to_base64(),
            minisign::KeyPair::generate_unencrypted_keypair()
                .unwrap()
                .pk
                .to_base64()
        ))
        .unwrap();
        assert!(verify_release_signature(
            &f.artifact,
            &f.release,
            &strangers,
            &expect(),
            &mut UpdateState::default(),
            CHAIN_NOW
        )
        .is_err());
        drop(std::fs::remove_dir_all(&f._dir));
    }

    #[test]
    fn x_ar5b_25_a_digest_only_policy_wants_a_pin_and_a_signed_one_wants_the_chain() {
        let f = chain_fixture(CHAIN_NOW * 2, 1, "x25-digest");
        let mut state = UpdateState::default();

        // Digest only: the pin is what is checked.
        let release = unsigned_release_on_a_server(&f);
        let verified = download_and_verify(
            &release,
            &expect(),
            &f._dir.join("digest-ok"),
            &UpdatePolicy::DigestOnly,
            &mut state,
        )
        .expect("a pinned digest that matches is enough for a digest-only policy");
        assert!(!verified.is_signed());
        assert_eq!(
            verified.sha256_hex(),
            sha256_hex(&sha256_of(b"the update artifact bytes"))
        );

        // No pin: nothing to check, so nothing is installed - and nothing was downloaded.
        let no_pin = ReleaseInfo {
            digest: "".into(),
            ..release.clone()
        };
        let staging = f._dir.join("digest-none");
        let refused = download_and_verify(
            &no_pin,
            &expect(),
            &staging,
            &UpdatePolicy::DigestOnly,
            &mut state,
        );
        assert!(
            refused
                .as_ref()
                .is_err_and(|e| e.contains("pins no digest")),
            "{refused:?}"
        );
        assert_eq!(files_in(&staging), 0);

        // A wrong pin: refused and removed.
        let wrong = ReleaseInfo {
            digest: format!("sha256:{}", "0".repeat(64)).into(),
            ..release
        };
        let staging = f._dir.join("digest-wrong");
        let refused = download_and_verify(
            &wrong,
            &expect(),
            &staging,
            &UpdatePolicy::DigestOnly,
            &mut state,
        );
        assert!(
            refused.as_ref().is_err_and(|e| e.contains("mismatch")),
            "{refused:?}"
        );
        assert_eq!(files_in(&staging), 0, "nothing refused stays staged");
        drop(std::fs::remove_dir_all(&f._dir));
    }

    #[test]
    fn x_ar5b_25_a_signed_policy_stages_a_signed_release_and_remembers_the_proof() {
        let f = chain_fixture(CHAIN_NOW * 2, 5, "x25-signed");
        let mut release = f.release.clone();
        release.download_url =
            serve_bytes(b"the update artifact bytes".to_vec(), "a-2.0.0.bin").into();
        let mut state = UpdateState::default();
        let verified = download_and_verify(
            &release,
            &expect(),
            &f._dir.join("staging"),
            &UpdatePolicy::Signed(roots_of(&f)),
            &mut state,
        )
        .expect("a well-formed chain stages");
        assert!(verified.is_signed());
        assert_eq!(verified.version(), "2.0.0");
        assert_eq!(state.key_generation, 5);
        assert_eq!(
            state.highest_seen, "2.0.0",
            "the version mark moves from a verified release statement"
        );
        assert_eq!(
            std::fs::read(verified.path()).unwrap(),
            b"the update artifact bytes"
        );
        assert!(verified.download().is_some_and(|d| !d.used_cached));
        drop(std::fs::remove_dir_all(&f._dir));
    }

    // ---- REVIEW Y6: verify and use are one read ---------------------------------------

    #[test]
    fn y6_a_file_swapped_after_it_was_verified_is_not_installed() {
        let f = chain_fixture(CHAIN_NOW * 2, 1, "y6");
        let mut release = f.release.clone();
        release.download_url =
            serve_bytes(b"the update artifact bytes".to_vec(), "a-2.0.0.bin").into();
        let mut state = UpdateState::default();
        let verified = download_and_verify(
            &release,
            &expect(),
            &f._dir.join("staging"),
            &UpdatePolicy::Signed(roots_of(&f)),
            &mut state,
        )
        .expect("verified");
        let target = f._dir.join("installed-app");
        std::fs::write(&target, b"the old program").unwrap();

        // A same-user process swaps the file between the check and the install.
        std::fs::write(verified.path(), b"EVIL bytes").unwrap();
        let refused = apply_update(&verified, &target);
        assert!(
            refused
                .as_ref()
                .is_err_and(|e| e.contains("changed after it was verified")),
            "{refused:?}"
        );
        assert_eq!(
            std::fs::read(&target).unwrap(),
            b"the old program",
            "the installed program is untouched"
        );
        let leftovers: Vec<String> = std::fs::read_dir(&f._dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(".update-incoming"))
            .collect();
        assert!(leftovers.is_empty(), "no half-copy is left: {leftovers:?}");

        // The file the check saw installs, and installs as the very bytes that passed.
        std::fs::write(verified.path(), b"the update artifact bytes").unwrap();
        apply_update(&verified, &target).expect("the verified bytes install");
        assert_eq!(
            std::fs::read(&target).unwrap(),
            b"the update artifact bytes"
        );
        drop(std::fs::remove_dir_all(&f._dir));
    }

    // ---- REVIEW Y8: the downloader's limits and its one unsanitised name ----------------------

    /// How a test server treats a `Range` request.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum RangeMode {
        /// A real server: `206` with the right `Content-Range`, `416` past the end.
        Honest,
        /// Every request gets the whole body as `200`: no range support.
        IgnoresRange,
        /// Every request gets a full 1 MiB window as `206` whatever the offset, with a right
        /// `Content-Range` and an unknown total (`/*`): a file with no end.
        Endless,
        /// Honest windows, but the total it names is 1 PiB.
        HugeTotal,
        /// `206`, but the `Content-Range` starts at 0 whatever was asked.
        WrongPlace,
        /// `206` with no `Content-Range` at all.
        NoContentRange,
        /// Honest, but the total it names grows by one with every request.
        ChangingTotal,
    }

    fn http_answer(status: u16, headers: &str, body: &[u8]) -> Vec<u8> {
        let reason = match status {
            200 => "OK",
            206 => "Partial Content",
            _ => "Range Not Satisfiable",
        };
        let mut out = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\n{headers}\r\n",
            body.len()
        )
        .into_bytes();
        out.extend_from_slice(body);
        out
    }

    /// A localhost server for `body` that reads the `Range` header; the URL of `name` on it and
    /// the number of requests it answered.
    fn serve_ranges(
        body: Vec<u8>,
        mode: RangeMode,
        name: &str,
    ) -> (String, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
        use std::io::{BufRead, BufReader, Write};
        use std::sync::atomic::{AtomicUsize, Ordering};
        const WINDOW: usize = 1024 * 1024;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!("http://{}/{name}", listener.local_addr().expect("addr"));
        let served = std::sync::Arc::new(AtomicUsize::new(0));
        let counter = served.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                'requests: loop {
                    let mut range: Option<(usize, usize)> = None;
                    let mut line = String::new();
                    loop {
                        line.clear();
                        match reader.read_line(&mut line) {
                            Ok(0) | Err(_) => break 'requests,
                            Ok(_) if line == "\r\n" => break,
                            Ok(_) => {
                                let lower = line.to_ascii_lowercase();
                                if let Some(value) = lower.strip_prefix("range: bytes=") {
                                    let (a, b) = value.trim().split_once('-').unwrap_or(("0", "0"));
                                    range = Some((a.parse().unwrap_or(0), b.parse().unwrap_or(0)));
                                }
                            }
                        }
                    }
                    let nth = counter.fetch_add(1, Ordering::SeqCst);
                    let total = body.len();
                    let window = |from: usize, to: usize| body[from..=to.min(total - 1)].to_vec();
                    let answer = match (mode, range) {
                        (RangeMode::IgnoresRange, _) | (_, None) => http_answer(200, "", &body),
                        (_, Some((from, _))) if from >= total && mode != RangeMode::Endless => {
                            http_answer(416, &format!("Content-Range: bytes */{total}\r\n"), &[])
                        }
                        (RangeMode::Honest, Some((from, to))) => {
                            let part = window(from, to);
                            let head = format!(
                                "Content-Range: bytes {from}-{}/{total}\r\n",
                                from + part.len() - 1
                            );
                            http_answer(206, &head, &part)
                        }
                        (RangeMode::ChangingTotal, Some((from, to))) => {
                            let part = window(from, to);
                            let head = format!(
                                "Content-Range: bytes {from}-{}/{}\r\n",
                                from + part.len() - 1,
                                total + nth
                            );
                            http_answer(206, &head, &part)
                        }
                        (RangeMode::HugeTotal, Some((from, to))) => {
                            let part = window(from, to);
                            let head = format!(
                                "Content-Range: bytes {from}-{}/{}\r\n",
                                from + part.len() - 1,
                                1u64 << 50
                            );
                            http_answer(206, &head, &part)
                        }
                        (RangeMode::Endless, Some((from, _))) => {
                            let head =
                                format!("Content-Range: bytes {from}-{}/*\r\n", from + WINDOW - 1);
                            http_answer(206, &head, &vec![7u8; WINDOW])
                        }
                        (RangeMode::WrongPlace, Some((from, to))) => {
                            let part = window(from, to);
                            let head =
                                format!("Content-Range: bytes 0-{}/{total}\r\n", part.len() - 1);
                            http_answer(206, &head, &part)
                        }
                        (RangeMode::NoContentRange, Some((from, to))) => {
                            http_answer(206, "", &window(from, to))
                        }
                    };
                    if stream.write_all(&answer).is_err() {
                        break;
                    }
                }
            }
        });
        (url, served)
    }

    fn range_body(len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 7) as u8)
            .collect()
    }

    /// A release of `body` at `url`, pinned by its digest.
    fn pinned_release_of(url: &str, body: &[u8]) -> ReleaseInfo {
        ReleaseInfo {
            version: "2.0.0".into(),
            download_url: url.into(),
            digest: format!("sha256:{}", sha256_hex(&sha256_of(body))).into(),
            ..ReleaseInfo::default()
        }
    }

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("azul-{tag}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn requests(served: &std::sync::Arc<std::sync::atomic::AtomicUsize>) -> usize {
        served.load(std::sync::atomic::Ordering::SeqCst)
    }

    #[test]
    fn y8_an_honest_range_server_is_downloaded_in_windows_and_ends_at_the_size_it_names() {
        let body = range_body(2 * 1024 * 1024 + 1000);
        let (url, served) = serve_ranges(body.clone(), RangeMode::Honest, "a-2.0.0.bin");
        let dir = scratch_dir("y8-honest");
        let outcome = download_update(&pinned_release_of(&url, &body), &dir).expect("downloads");
        assert_eq!(std::fs::read(&outcome.path).unwrap(), body);
        assert!(outcome.server_supports_resume);
        assert_eq!(outcome.bytes_written, body.len() as u64);
        assert_eq!(
            requests(&served),
            3,
            "three windows, and no fourth request to learn that the file ended"
        );
        assert!(!dir.join("a-2.0.0.bin.partial").exists());
        drop(std::fs::remove_dir_all(&dir));
    }

    #[test]
    fn y8_a_download_continues_from_the_partial_file() {
        let body = range_body(2 * 1024 * 1024 + 500);
        let (url, served) = serve_ranges(body.clone(), RangeMode::Honest, "a-2.0.0.bin");
        let dir = scratch_dir("y8-resume");
        std::fs::write(dir.join("a-2.0.0.bin.partial"), &body[..1024 * 1024]).unwrap();
        let outcome = download_update(&pinned_release_of(&url, &body), &dir).expect("resumes");
        assert_eq!(outcome.resumed_from_bytes, 1024 * 1024);
        assert_eq!(outcome.bytes_written, body.len() as u64 - 1024 * 1024);
        assert_eq!(std::fs::read(&outcome.path).unwrap(), body);
        assert_eq!(requests(&served), 2);
        drop(std::fs::remove_dir_all(&dir));
    }

    #[test]
    fn y8_a_server_without_range_support_restarts_the_file() {
        let body = range_body(300_000);
        let (url, served) = serve_ranges(body.clone(), RangeMode::IgnoresRange, "a-2.0.0.bin");
        let dir = scratch_dir("y8-norange");
        std::fs::write(
            dir.join("a-2.0.0.bin.partial"),
            b"garbage from an older attempt",
        )
        .unwrap();
        let outcome = download_update(&pinned_release_of(&url, &body), &dir).expect("downloads");
        assert!(!outcome.server_supports_resume);
        assert_eq!(std::fs::read(&outcome.path).unwrap(), body);
        assert_eq!(requests(&served), 1);
        drop(std::fs::remove_dir_all(&dir));
    }

    /// The register's case: a server that answers `206` for every window for ever. Fails on the old
    /// loop, which asked for the next window until the disk was full.
    #[test]
    fn y8_an_endless_server_is_stopped_by_the_cap_and_leaves_no_partial_file() {
        let (url, served) = serve_ranges(Vec::new(), RangeMode::Endless, "endless-2.0.0.bin");
        let dir = scratch_dir("y8-endless");
        let release = ReleaseInfo {
            version: "2.0.0".into(),
            download_url: url.into(),
            ..ReleaseInfo::default()
        };
        let err = download_update_capped(&release, &dir, 3 * 1024 * 1024)
            .expect_err("a file with no end must be refused");
        assert!(err.contains("larger than"), "{err}");
        assert!(
            requests(&served) <= 5,
            "it stopped after the cap, not at the end of the disk: {} requests",
            requests(&served)
        );
        assert_eq!(files_in(&dir), 0, "no partial file stays");
        drop(std::fs::remove_dir_all(&dir));
    }

    #[test]
    fn y8_a_total_above_the_cap_is_refused_at_the_first_window() {
        let body = range_body(1024 * 1024 + 10);
        let (url, served) = serve_ranges(body.clone(), RangeMode::HugeTotal, "huge-2.0.0.bin");
        let dir = scratch_dir("y8-huge");
        let err = download_update(&pinned_release_of(&url, &body), &dir)
            .expect_err("a petabyte update is refused");
        assert!(err.contains("larger than"), "{err}");
        assert_eq!(requests(&served), 1);
        assert_eq!(files_in(&dir), 0);
        drop(std::fs::remove_dir_all(&dir));
    }

    #[test]
    fn y8_a_206_that_starts_elsewhere_or_names_no_range_is_refused() {
        let body = range_body(2 * 1024 * 1024);
        for (mode, tag) in [
            (RangeMode::WrongPlace, "wrong-place"),
            (RangeMode::NoContentRange, "no-range"),
        ] {
            let (url, _) = serve_ranges(body.clone(), mode, "a-2.0.0.bin");
            let dir = scratch_dir(&format!("y8-{tag}"));
            // A partial file, so that the second window is asked for at a real offset.
            std::fs::write(dir.join("a-2.0.0.bin.partial"), &body[..1024 * 1024]).unwrap();
            let err = download_update(&pinned_release_of(&url, &body), &dir)
                .expect_err("bytes that cannot be placed are refused");
            assert!(
                err.contains("Content-Range") || err.contains("bytes"),
                "{tag}: {err}"
            );
            assert!(
                !dir.join("a-2.0.0.bin.partial").exists(),
                "{tag}: the partial is deleted"
            );
            assert!(
                !dir.join("a-2.0.0.bin").exists(),
                "{tag}: nothing is staged"
            );
            drop(std::fs::remove_dir_all(&dir));
        }
    }

    #[test]
    fn y8_a_file_whose_size_changes_during_the_download_is_refused() {
        let body = range_body(2 * 1024 * 1024 + 7);
        let (url, _) = serve_ranges(body.clone(), RangeMode::ChangingTotal, "a-2.0.0.bin");
        let dir = scratch_dir("y8-changing");
        let err = download_update(&pinned_release_of(&url, &body), &dir)
            .expect_err("a size that moves is refused");
        assert!(err.contains("changed"), "{err}");
        assert_eq!(files_in(&dir), 0);
        drop(std::fs::remove_dir_all(&dir));
    }

    #[test]
    fn y8_a_cached_artifact_is_reused_only_while_it_matches_its_pin() {
        let body = range_body(10_000);
        let (url, served) = serve_ranges(body.clone(), RangeMode::Honest, "a-2.0.0.bin");
        let dir = scratch_dir("y8-cache");
        std::fs::write(dir.join("a-2.0.0.bin"), b"a file somebody changed").unwrap();
        let err = download_update(&pinned_release_of(&url, &body), &dir)
            .expect_err("a stale cached file is not reused");
        assert!(err.contains("cached artifact failed verification"), "{err}");
        assert!(!dir.join("a-2.0.0.bin").exists(), "and it is removed");

        std::fs::write(dir.join("a-2.0.0.bin"), &body).unwrap();
        let outcome = download_update(&pinned_release_of(&url, &body), &dir).expect("cached");
        assert!(outcome.used_cached);
        assert_eq!(requests(&served), 0, "nothing was transferred");
        drop(std::fs::remove_dir_all(&dir));
    }

    /// The one unsanitised name: with no file name in the URL the version made one, and a version
    /// such as `..\..\x` climbed out of the staging folder on Windows. Fails on the old code: the
    /// download cannot even open its partial file under `update-../../escape.bin.partial`.
    #[test]
    fn y8_the_name_made_from_the_version_cannot_climb_out_of_the_staging_folder() {
        let body = range_body(5_000);
        let (url, _) = serve_ranges(body.clone(), RangeMode::Honest, "");
        let dir = scratch_dir("y8-name");
        let mut release = pinned_release_of(&url, &body);
        release.version = "../../escape".into();
        let outcome = download_update(&release, &dir).expect("a hostile version is just a label");
        assert_eq!(outcome.path.parent(), Some(dir.as_path()));
        assert_eq!(
            outcome.path.file_name().and_then(|n| n.to_str()),
            Some("update-..-..-escape.bin")
        );
        assert_eq!(files_in(&dir), 1);

        assert_eq!(
            staged_artifact_name("https://x/app-2.0.0.bin", "9"),
            "app-2.0.0.bin"
        );
        assert_eq!(
            staged_artifact_name("https://x/", "..\\..\\x"),
            "update-..-..-x.bin"
        );
        assert_eq!(
            staged_artifact_name("https://x/..", "1.0.0"),
            "update-1.0.0.bin"
        );
        assert_eq!(
            staged_artifact_name("https://r/v2/blobs/sha256:abc", "1"),
            "sha256-abc"
        );
        for version in ["../../etc/passwd", "a/b", "C:\\x", ""] {
            let name = staged_artifact_name("https://x/", version);
            assert!(!name.contains(['/', '\\', ':']), "{name}");
            assert!(name.starts_with("update-"), "{name}");
        }
        drop(std::fs::remove_dir_all(&dir));
    }

    #[test]
    fn y8_a_content_range_is_read_strictly() {
        assert_eq!(
            parse_content_range("bytes 0-1048575/3145728"),
            Some((0, 1_048_575, Some(3_145_728)))
        );
        assert_eq!(parse_content_range("bytes 10-19/*"), Some((10, 19, None)));
        assert_eq!(parse_content_range(" bytes  5-5/9 "), Some((5, 5, Some(9))));
        for bad in [
            "",
            "bytes */100",
            "bytes 5-4/100",
            "bytes a-b/100",
            "items 0-9/10",
            "bytes 0-9",
            "bytes 0-9/x",
        ] {
            assert_eq!(parse_content_range(bad), None, "{bad:?}");
        }
    }

    // ---- REVIEW Y10: the generic self-update --------------------------------------------------

    /// The Windows branch cannot run here, so the swap it is made of takes its `rename`: the second
    /// move fails, and the program must still be there (it was left without its executable).
    #[test]
    fn y10_the_old_program_is_put_back_when_the_new_one_cannot_take_its_place() {
        let dir = scratch_dir("y10-swap");
        let (target, incoming, aside) = (
            dir.join("app.exe"),
            dir.join(".update-incoming"),
            dir.join(".update-previous"),
        );
        std::fs::write(&target, b"the old program").unwrap();
        std::fs::write(&incoming, b"the new program").unwrap();

        let err = swap_moving_the_old_aside(&incoming, &target, &aside, |from, to| {
            if from == incoming.as_path() {
                Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "locked",
                ))
            } else {
                std::fs::rename(from, to)
            }
        })
        .expect_err("the second move fails");
        assert!(err.contains("put back"), "{err}");
        assert_eq!(std::fs::read(&target).unwrap(), b"the old program");
        assert!(!aside.exists(), "nothing is left aside");

        // And when nothing fails: the new program is in place and the old one is aside.
        swap_moving_the_old_aside(&incoming, &target, &aside, |from, to| {
            std::fs::rename(from, to)
        })
        .expect("swaps");
        assert_eq!(std::fs::read(&target).unwrap(), b"the new program");
        assert_eq!(std::fs::read(&aside).unwrap(), b"the old program");
        drop(std::fs::remove_dir_all(&dir));
    }

    #[test]
    fn y10_archive_kind_names_the_formats_this_updater_does_not_unpack() {
        assert_eq!(archive_kind(&[0x1f, 0x8b, 0x08, 0x00]), Some("gzip"));
        assert_eq!(archive_kind(b"PK\x03\x04rest"), Some("zip"));
        assert_eq!(archive_kind(b"BZh91AY"), Some("bzip2"));
        assert_eq!(
            archive_kind(&[0xfd, b'7', b'z', b'X', b'Z', 0x00, 1]),
            Some("xz")
        );
        assert_eq!(archive_kind(&[0x28, 0xb5, 0x2f, 0xfd, 0]), Some("zstd"));
        let mut tar = vec![0u8; 512];
        tar[257..262].copy_from_slice(b"ustar");
        assert_eq!(archive_kind(&tar), Some("tar"));
        // Programs, scripts and nothing at all are not archives.
        assert_eq!(archive_kind(b"\x7fELF\x02\x01"), None);
        assert_eq!(archive_kind(&[0xcf, 0xfa, 0xed, 0xfe]), None);
        assert_eq!(archive_kind(b"MZ\x90\x00"), None);
        assert_eq!(archive_kind(b"#!/bin/sh\n"), None);
        assert_eq!(archive_kind(b""), None);
    }

    #[test]
    fn y10_an_archive_is_not_installed_over_the_program() {
        let dir = scratch_dir("y10-archive");
        let target = dir.join("installed-app");
        std::fs::write(&target, b"the old program").unwrap();
        let staged = |name: &str, bytes: &[u8]| {
            let path = dir.join(name);
            std::fs::write(&path, bytes).unwrap();
            VerifiedArtifact {
                path,
                sha256: sha256_of(bytes),
                version: "2.0.0".into(),
                signed: true,
                download: None,
            }
        };

        let mut gzip = vec![0x1f, 0x8b, 0x08, 0x00];
        gzip.extend_from_slice(&[1; 600]);
        let err = apply_update(&staged("app.tar.gz", &gzip), &target)
            .expect_err("a .tar.gz would replace the program with the archive");
        assert!(err.contains("gzip archive"), "{err}");
        assert_eq!(std::fs::read(&target).unwrap(), b"the old program");
        let leftovers: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(".update-incoming"))
            .collect();
        assert!(leftovers.is_empty(), "no half-copy is left: {leftovers:?}");

        // A program installs.
        apply_update(&staged("app.bin", b"\x7fELF the new program"), &target).expect("installs");
        assert_eq!(std::fs::read(&target).unwrap(), b"\x7fELF the new program");
        drop(std::fs::remove_dir_all(&dir));
    }

    // ---- REVIEW Y7: the anti-rollback state ---------------------------------------------------

    /// A parse error used to reset the marks to "" and 0, and the next save wrote them over the
    /// file: a retired, unexpired signing key was accepted again.
    #[test]
    fn y7_a_damaged_state_file_is_an_error_and_is_not_written_over() {
        let dir = scratch_dir("y7-damaged");
        let file = dir.join("update-state.json");
        std::fs::write(&file, b"{\"highest_seen\": \"4.0.0\", \"key_gener").unwrap();

        let err = UpdateState::read(&dir).expect_err("half a file is not a fresh start");
        assert!(
            err.contains("damaged") && err.contains("delete the file"),
            "{err}"
        );
        assert_eq!(UpdateState::load(&dir), UpdateState::default());

        let fresh = UpdateState {
            highest_seen: "1.0.0".into(),
            ..UpdateState::default()
        };
        assert!(
            fresh.try_save(&dir).is_err(),
            "the marks in the file are not replaced"
        );
        fresh.save(&dir);
        assert_eq!(
            std::fs::read(&file).unwrap(),
            b"{\"highest_seen\": \"4.0.0\", \"key_gener",
            "the damaged file is as it was"
        );

        // A file that is not an object is damaged too; a missing one is a first run.
        std::fs::write(&file, b"[1, 2]").unwrap();
        assert!(UpdateState::read(&dir).is_err());
        std::fs::remove_file(&file).unwrap();
        assert_eq!(UpdateState::read(&dir), Ok(UpdateState::default()));
        drop(std::fs::remove_dir_all(&dir));
    }

    /// The app and the install manager both write the marks of a program, with no lock between
    /// them: a writer that loaded before the other one saved must not lower what the other raised.
    #[test]
    fn y7_a_stale_writer_does_not_lower_the_marks_another_one_raised() {
        let dir = scratch_dir("y7-merge");
        let stale = UpdateState::load(&dir);
        UpdateState {
            highest_seen: "3.0.0".into(),
            key_generation: 5,
            rollout_bucket: Some(42),
            ..UpdateState::default()
        }
        .try_save(&dir)
        .unwrap();

        let mut late = stale;
        late.highest_seen = "1.0.0".into();
        late.key_generation = 1;
        late.last_check_unix = 1_800_000_123;
        late.try_save(&dir).unwrap();

        let on_disk = UpdateState::read(&dir).unwrap();
        assert_eq!(on_disk.highest_seen, "3.0.0");
        assert_eq!(on_disk.key_generation, 5);
        assert_eq!(on_disk.rollout_bucket, Some(42), "the cohort stays");
        assert_eq!(
            on_disk.last_check_unix, 1_800_000_123,
            "what is not a mark is the writer's"
        );

        // The marks do go up.
        let mut up = on_disk;
        up.highest_seen = "3.1.0".into();
        up.key_generation = 6;
        up.try_save(&dir).unwrap();
        let on_disk = UpdateState::read(&dir).unwrap();
        assert_eq!(
            (on_disk.highest_seen.as_str(), on_disk.key_generation),
            ("3.1.0", 6)
        );
        drop(std::fs::remove_dir_all(&dir));
    }

    #[test]
    fn y7_a_save_is_one_rename_and_leaves_no_temp_file() {
        let dir = scratch_dir("y7-atomic");
        UpdateState::default().try_save(&dir).unwrap();
        UpdateState::default().try_save(&dir).unwrap();
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        // The state and the file the writers lock; no temp file is left.
        assert_eq!(
            names,
            vec![
                "update-state.json".to_owned(),
                "update-state.json.lock".to_owned()
            ]
        );
        drop(std::fs::remove_dir_all(&dir));
    }

    /// Marks that cannot be recorded mean an update that is not staged: the staging used to drop
    /// the write error and hand the file over anyway.
    #[cfg(unix)]
    #[test]
    fn y7_a_state_that_cannot_be_recorded_takes_the_staged_file_with_it() {
        use std::os::unix::fs::PermissionsExt;
        let f = chain_fixture(CHAIN_NOW * 2, 1, "y7-unrecordable");
        let mut release = f.release.clone();
        release.download_url =
            serve_bytes(std::fs::read(&f.artifact).unwrap(), "a-2.0.0.bin").into();
        let state_dir = f._dir.join("state");
        std::fs::create_dir_all(&state_dir).unwrap();
        std::fs::set_permissions(&state_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        if std::fs::write(state_dir.join("probe"), b"x").is_ok() {
            // Running as a user the permission bits do not bind (root): nothing to show.
            std::fs::set_permissions(&state_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
            drop(std::fs::remove_dir_all(&f._dir));
            return;
        }
        let staging = f._dir.join("staging");
        let err = stage_program_update_with(
            &release,
            &expect(),
            &staging,
            &UpdatePolicy::Signed(roots_of(&f)),
            &state_dir,
        )
        .expect_err("the marks cannot move, so nothing is staged");
        assert!(
            err.contains("update state") && err.contains("not staged"),
            "{err}"
        );
        assert_eq!(files_in(&staging), 0, "the verified file went with it");
        std::fs::set_permissions(&state_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        drop(std::fs::remove_dir_all(&f._dir));
    }

    /// Unreadable marks stop the staging before a byte is downloaded.
    #[test]
    fn y7_unreadable_marks_stop_the_staging() {
        let f = chain_fixture(CHAIN_NOW * 2, 1, "y7-unreadable");
        let state_dir = f._dir.join("state");
        std::fs::create_dir_all(&state_dir).unwrap();
        std::fs::write(state_dir.join("update-state.json"), b"not json").unwrap();
        let mut release = f.release.clone();
        let (url, served) = serve_ranges(
            std::fs::read(&f.artifact).unwrap(),
            RangeMode::Honest,
            "a.bin",
        );
        release.download_url = url.into();
        let err = stage_program_update_with(
            &release,
            &expect(),
            &f._dir.join("staging"),
            &UpdatePolicy::Signed(roots_of(&f)),
            &state_dir,
        )
        .expect_err("damaged marks stop the staging");
        assert!(err.contains("damaged"), "{err}");
        assert_eq!(requests(&served), 0, "before anything was downloaded");
        drop(std::fs::remove_dir_all(&f._dir));
    }

    // ---- REVIEW Y3: the release statement binds the artifact to its app, version, platform ---

    fn good_statement(extra: &str) -> String {
        format!(
            "azul-release-v1|app=azwriter|version=2.0.0|platform=macos-aarch64|sha256={}|released=1700000000|min_generation=3{extra}",
            "ab".repeat(32)
        )
    }

    #[test]
    fn y3_the_release_statement_parse_is_strict() {
        let ok = parse_release_statement(&good_statement("")).unwrap();
        assert_eq!(ok.app, "azwriter");
        assert_eq!(ok.version, "2.0.0");
        assert_eq!(ok.platform, "macos-aarch64");
        assert_eq!(sha256_hex(&ok.sha256), "ab".repeat(32));
        assert_eq!((ok.released_unix, ok.min_generation), (1_700_000_000, 3));
        // Every deviation is an error, not a default.
        let hash = "ab".repeat(32);
        let bad = [
            good_statement("").replace("azul-release-v1", "azul-release-v2"),
            good_statement("|extra=x"),
            good_statement("|app=again"),
            good_statement("|nonsense"),
            good_statement("").replace("|app=azwriter", ""),
            good_statement("").replace("|min_generation=3", ""),
            good_statement("").replace("released=1700000000", "released=soon"),
            good_statement("").replace(&hash, &"ab".repeat(31)),
            good_statement("").replace("app=azwriter", "app="),
            good_statement("").replace(&hash, &"zz".repeat(32)),
            String::new(),
        ];
        for statement in bad {
            assert!(
                parse_release_statement(&statement).is_err(),
                "{statement:?} must not parse"
            );
        }
    }

    #[test]
    fn y3_the_manifest_and_the_github_assets_carry_the_release_statement() {
        let (release, _) = parse_manifest_v1(
            r#"{"latest": {"version": "2.0.0", "release_statement": "azul-release-v1|x", "release_statement_sig": "sig"}}"#,
        )
        .unwrap();
        assert_eq!(release.release_statement.as_str(), "azul-release-v1|x");
        assert_eq!(release.release_statement_sig.as_str(), "sig");
    }

    #[test]
    fn y3_a_genuine_old_release_does_not_pass_under_a_new_version_label() {
        let f = chain_fixture(CHAIN_NOW + DAY, 3, "y3-label");
        // The manifest of an attacker: the bytes, the signatures and both statements of the
        // genuine 2.0.0, under the version "99.0.0".
        let mut relabelled = f.release.clone();
        relabelled.version = "99.0.0".into();
        let mut state = UpdateState::default();
        let err = verify_release_signature(
            &f.artifact,
            &relabelled,
            &roots_of(&f),
            &expect(),
            &mut state,
            CHAIN_NOW,
        )
        .expect_err("a relabelled release must be refused");
        assert!(err.contains("the version"), "{err}");
        assert_eq!(
            (state.highest_seen.as_str(), state.key_generation),
            ("", 0),
            "a refused release moves no mark"
        );
        drop(std::fs::remove_dir_all(&f._dir));
    }

    #[test]
    fn y3_a_release_for_another_app_or_another_platform_is_refused() {
        let f = chain_fixture(CHAIN_NOW + DAY, 3, "y3-who");
        let mut state = UpdateState::default();
        let other_app = Expect {
            app: "azmail".into(),
            platform: platform_label(),
        };
        let err = verify_release_signature(
            &f.artifact,
            &f.release,
            &roots_of(&f),
            &other_app,
            &mut state,
            CHAIN_NOW,
        )
        .expect_err("another app's release");
        assert!(err.contains("the app"), "{err}");
        let other_platform = Expect {
            app: FIXTURE_APP.into(),
            platform: "plan9-mips".into(),
        };
        let err = verify_release_signature(
            &f.artifact,
            &f.release,
            &roots_of(&f),
            &other_platform,
            &mut state,
            CHAIN_NOW,
        )
        .expect_err("another platform's release");
        assert!(err.contains("the platform"), "{err}");
        drop(std::fs::remove_dir_all(&f._dir));
    }

    #[test]
    fn y3_a_statement_of_other_bytes_is_refused_even_when_the_artifact_signature_is_good() {
        let f = chain_fixture(CHAIN_NOW + DAY, 3, "y3-bytes");
        // Another artifact, genuinely signed by the signing key, offered with the statement of
        // the first one.
        let other = b"another genuine artifact";
        std::fs::write(&f.artifact, other).unwrap();
        let signature = minisign::sign(
            Some(&f.signing.pk),
            &f.signing.sk,
            std::io::Cursor::new(&other[..]),
            Some("azul release 2.0.0"),
            None,
        )
        .unwrap()
        .to_string();
        let release = ReleaseInfo {
            signature: signature.into(),
            ..f.release.clone()
        };
        let err = verify_release_signature(
            &f.artifact,
            &release,
            &roots_of(&f),
            &expect(),
            &mut UpdateState::default(),
            CHAIN_NOW,
        )
        .expect_err("the statement names other bytes");
        assert!(err.contains("sha256"), "{err}");
        drop(std::fs::remove_dir_all(&f._dir));
    }

    #[test]
    fn y3_a_statement_from_the_future_or_above_its_key_generation_is_refused() {
        let f = chain_fixture(CHAIN_NOW + DAY, 3, "y3-dates");
        let sha = sha256_of(FIXTURE_BYTES);
        let platform = platform_label();
        let future = release_statement_text(
            FIXTURE_APP,
            "2.0.0",
            &platform,
            &sha,
            CHAIN_NOW + 2 * DAY,
            3,
        );
        let err = verify_release_signature(
            &f.artifact,
            &with_release_statement(&f, &future),
            &roots_of(&f),
            &expect(),
            &mut UpdateState::default(),
            CHAIN_NOW,
        )
        .expect_err("dated in the future");
        assert!(err.contains("future"), "{err}");
        // A floor above the generation of the statement that names the signing key.
        let high =
            release_statement_text(FIXTURE_APP, "2.0.0", &platform, &sha, FIXTURE_RELEASED, 4);
        let err = verify_release_signature(
            &f.artifact,
            &with_release_statement(&f, &high),
            &roots_of(&f),
            &expect(),
            &mut UpdateState::default(),
            CHAIN_NOW,
        )
        .expect_err("a floor the signing key does not meet");
        assert!(err.contains("requires a signing-key generation"), "{err}");
        // A statement the signing key did not sign.
        let mut forged = with_release_statement(&f, &good_statement(""));
        forged.release_statement_sig = f.release.signing_key_statement_sig.clone();
        let err = verify_release_signature(
            &f.artifact,
            &forged,
            &roots_of(&f),
            &expect(),
            &mut UpdateState::default(),
            CHAIN_NOW,
        )
        .expect_err("not signed by the signing key");
        assert!(err.contains("release statement"), "{err}");
        drop(std::fs::remove_dir_all(&f._dir));
    }

    #[test]
    fn y3_the_marks_move_only_when_the_whole_chain_verified() {
        let f = chain_fixture(CHAIN_NOW + DAY, 3, "y3-marks");
        let mut state = UpdateState::default();
        let stranger = Expect {
            app: "azmail".into(),
            platform: platform_label(),
        };
        drop(
            verify_release_signature(
                &f.artifact,
                &f.release,
                &roots_of(&f),
                &stranger,
                &mut state,
                CHAIN_NOW,
            )
            .unwrap_err(),
        );
        assert_eq!((state.highest_seen.as_str(), state.key_generation), ("", 0));
        verify_release_signature(
            &f.artifact,
            &f.release,
            &roots_of(&f),
            &expect(),
            &mut state,
            CHAIN_NOW,
        )
        .expect("the chain verifies");
        assert_eq!(
            (state.highest_seen.as_str(), state.key_generation),
            ("2.0.0", 3)
        );
        // A verified release below the mark does not lower it - it is refused (see
        // `a_signed_release_below_the_mark_is_a_rollback_and_is_refused`).
        state.highest_seen = "5.0.0".into();
        let err = verify_release_signature(
            &f.artifact,
            &f.release,
            &roots_of(&f),
            &expect(),
            &mut state,
            CHAIN_NOW,
        )
        .expect_err("a release below the mark is a rollback");
        assert!(err.contains("version ROLLBACK"), "{err}");
        assert_eq!(state.highest_seen, "5.0.0");
        drop(std::fs::remove_dir_all(&f._dir));
    }

    /// X-DELTA-AZDRIVE-KEYS-33 (a): a release the signing key vouches for that is OLDER than the
    /// highest one this client verified is refused, whatever the manifest lists. Red on the old
    /// code, which only raised the mark. The same version again is a re-install, not a rollback.
    #[test]
    fn a_signed_release_below_the_mark_is_a_rollback_and_is_refused() {
        let f = chain_fixture(CHAIN_NOW + DAY, 3, "rollback-below");
        let mut state = UpdateState {
            highest_seen: "3.0.0".into(),
            ..UpdateState::default()
        };
        let err = verify_release_signature(
            &f.artifact,
            &f.release,
            &roots_of(&f),
            &expect(),
            &mut state,
            CHAIN_NOW,
        )
        .expect_err("the genuine 2.0.0 is below the 3.0.0 this client verified");
        assert!(
            err.contains("version ROLLBACK") && err.contains("2.0.0") && err.contains("3.0.0"),
            "{err}"
        );
        assert_eq!(
            (
                state.highest_seen.as_str(),
                state.key_generation,
                state.clock_floor_unix
            ),
            ("3.0.0", 0, 0),
            "a refused chain moves no mark"
        );

        // The version it already verified is no rollback: a retry of an install that failed.
        state.highest_seen = "2.0.0".into();
        verify_release_signature(
            &f.artifact,
            &f.release,
            &roots_of(&f),
            &expect(),
            &mut state,
            CHAIN_NOW,
        )
        .expect("the same version again verifies");
        assert_eq!(state.highest_seen, "2.0.0");

        // A candidate of a LATER version is above it too.
        state.highest_seen = "2.0.1-rc1".into();
        assert!(verify_release_signature(
            &f.artifact,
            &f.release,
            &roots_of(&f),
            &expect(),
            &mut state,
            CHAIN_NOW,
        )
        .is_err());
        drop(std::fs::remove_dir_all(&f._dir));
    }

    /// X-DELTA-AZDRIVE-KEYS-33 (a), the decoy: the manifest lists a "newer" release this client's
    /// cohort is not offered (so the check on the unsigned labels passes: the highest label is
    /// above the mark) beside a genuine older one that is open to everyone. The check offers the
    /// older one; the signature step is what refuses it.
    #[test]
    fn a_decoy_newer_label_does_not_let_a_genuine_older_release_through() {
        let f = chain_fixture(CHAIN_NOW + DAY, 3, "decoy");
        let now = unix_now();
        let manifest = format!(
            r#"{{"latest": {{"version": "99.0.0", "download_url": "http://127.0.0.1:9/decoy.bin",
                "release_date": {now}, "slow": {{"10": {}, "100": {}}}}},
               "releases": [{{"version": "2.0.0", "download_url": "http://127.0.0.1:9/app.bin",
                "slow": "off"}}]}}"#,
            now + 3 * DAY,
            now + 5 * DAY
        );
        let url = serve_bytes(manifest.into_bytes(), "updates.json");
        let mut state = UpdateState {
            highest_seen: "3.0.0".into(),
            rollout_bucket: Some(50),
            ..UpdateState::default()
        };
        let result = check_for_updates_blocking(
            &url,
            "1.0.0",
            "stable",
            &mut state,
            UpdateAudience::AutoUpdate,
        );
        let UpdateCheckResult::Available(offered) = result else {
            panic!("the unsigned labels pass the manifest-level check: {result:?}");
        };
        assert_eq!(
            offered.version.as_str(),
            "2.0.0",
            "the decoy is not offered to this cohort, the genuine older release is"
        );
        let err = verify_release_signature(
            &f.artifact,
            &ReleaseInfo {
                version: offered.version.clone(),
                ..f.release.clone()
            },
            &roots_of(&f),
            &expect(),
            &mut state,
            CHAIN_NOW,
        )
        .expect_err("the signed 2.0.0 is below the mark");
        assert!(err.contains("version ROLLBACK"), "{err}");
        assert_eq!(state.highest_seen, "3.0.0");
        drop(std::fs::remove_dir_all(&f._dir));
    }

    /// X-DELTA-AZDRIVE-KEYS-33 (c): a clock set back does not make an expired signing-key
    /// statement valid again. The floor is the time of the last chain that verified; the check
    /// goes by the later of the clock and the floor. Red on the old code (no floor).
    #[test]
    fn a_clock_set_back_does_not_revive_an_expired_statement() {
        // The statement is good for a day after CHAIN_NOW.
        let f = chain_fixture(CHAIN_NOW + DAY, 3, "clock-floor");
        let mut state = UpdateState::default();
        verify_release_signature(
            &f.artifact,
            &f.release,
            &roots_of(&f),
            &expect(),
            &mut state,
            CHAIN_NOW + DAY - 10,
        )
        .expect("verifies just before it expires");
        assert_eq!(
            state.clock_floor_unix,
            CHAIN_NOW + DAY - 10,
            "a verified chain raises the floor to its time"
        );

        // Two days later the statement is expired.
        let mut later = state.clone();
        assert!(verify_release_signature(
            &f.artifact,
            &f.release,
            &roots_of(&f),
            &expect(),
            &mut later,
            CHAIN_NOW + 2 * DAY,
        )
        .unwrap_err()
        .contains("expired"));
        assert_eq!(
            later.clock_floor_unix,
            CHAIN_NOW + DAY - 10,
            "a chain that did not verify does not raise the floor"
        );

        // A clock set back by the attacker: the floor (the time the last chain verified at) is
        // later than "now"; the statement is still judged at the later time.
        let mut floored = UpdateState {
            clock_floor_unix: CHAIN_NOW + 2 * DAY,
            ..UpdateState::default()
        };
        let err = verify_release_signature(
            &f.artifact,
            &f.release,
            &roots_of(&f),
            &expect(),
            &mut floored,
            CHAIN_NOW,
        )
        .expect_err("the clock says CHAIN_NOW, the floor says two days later");
        assert!(err.contains("expired"), "{err}");
        assert_eq!(floored.clock_floor_unix, CHAIN_NOW + 2 * DAY);

        // The floor is a mark: it is saved, read back and never lowered by a stale writer.
        let dir = scratch_dir("clock-floor");
        state.try_save(&dir).unwrap();
        assert_eq!(
            UpdateState::read(&dir).unwrap().clock_floor_unix,
            CHAIN_NOW + DAY - 10
        );
        UpdateState::default().try_save(&dir).unwrap();
        assert_eq!(
            UpdateState::read(&dir).unwrap().clock_floor_unix,
            CHAIN_NOW + DAY - 10,
            "a writer that never saw the floor does not lower it"
        );
        drop(std::fs::remove_dir_all(&dir));
        drop(std::fs::remove_dir_all(&f._dir));
    }

    /// X-DELTA-AZDRIVE-KEYS-33 (d): a field that is there with another type is a damaged file, not
    /// a zero that the next save writes back over the mark. Red on the old code, which read
    /// `"key_generation": "5"` as 0.
    #[test]
    fn a_mistyped_field_of_the_state_file_is_a_damaged_file() {
        let dir = scratch_dir("typed-fields");
        let file = dir.join("update-state.json");
        for (bad, what) in [
            (r#"{"key_generation": "5"}"#, "key_generation"),
            (r#"{"key_generation": -1}"#, "key_generation"),
            (r#"{"key_generation": 5.5}"#, "key_generation"),
            (r#"{"key_generation": null}"#, "key_generation"),
            (r#"{"highest_seen": 5}"#, "highest_seen"),
            (r#"{"highest_seen": ["2.0.0"]}"#, "highest_seen"),
            (r#"{"last_check_unix": "yesterday"}"#, "last_check_unix"),
            (r#"{"suspended_until_unix": true}"#, "suspended_until_unix"),
            (r#"{"clock_floor_unix": "0"}"#, "clock_floor_unix"),
            (r#"{"rollout_bucket": "7"}"#, "rollout_bucket"),
            (r#"{"rollout_bucket": 100}"#, "rollout_bucket"),
        ] {
            std::fs::write(&file, bad).unwrap();
            let err = UpdateState::read(&dir).expect_err(bad);
            assert!(
                err.contains("damaged") && err.contains(what),
                "{bad}: {err}"
            );
            // And the marks in it are not written over.
            UpdateState {
                key_generation: 1,
                ..UpdateState::default()
            }
            .save(&dir);
            assert_eq!(std::fs::read_to_string(&file).unwrap(), bad);
        }

        // What is absent is a file from before the field; the cohort may be null (none drawn).
        std::fs::write(
            &file,
            r#"{"highest_seen": "1.2.3", "key_generation": 4, "rollout_bucket": null}"#,
        )
        .unwrap();
        let read = UpdateState::read(&dir).unwrap();
        assert_eq!(read.highest_seen, "1.2.3");
        assert_eq!(read.key_generation, 4);
        assert_eq!(read.rollout_bucket, None);
        assert_eq!(read.clock_floor_unix, 0);
        drop(std::fs::remove_dir_all(&dir));
    }

    /// X-DELTA-AZDRIVE-KEYS-33 (d): a save is read-merge-rename under a lock on the state folder,
    /// so a mark that another process raises between the read and the rename is not written over.
    /// While the lock is held elsewhere the save waits; red on the old code (no lock: it returned).
    #[test]
    fn a_save_waits_for_the_lock_another_writer_holds() {
        let dir = scratch_dir("state-lock");
        std::fs::create_dir_all(&dir).unwrap();
        let held = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.join("update-state.json.lock"))
            .unwrap();
        held.lock().unwrap();

        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let saver = {
            let dir = dir.clone();
            std::thread::spawn(move || {
                let result = UpdateState {
                    key_generation: 7,
                    ..UpdateState::default()
                }
                .try_save(&dir);
                drop(done_tx.send(result));
            })
        };
        assert!(
            done_rx
                .recv_timeout(std::time::Duration::from_millis(300))
                .is_err(),
            "the save went on while another writer held the lock"
        );
        held.unlock().unwrap();
        done_rx
            .recv_timeout(std::time::Duration::from_secs(30))
            .expect("the save goes on once the lock is let go")
            .expect("and it succeeds");
        saver.join().unwrap();
        assert_eq!(UpdateState::read(&dir).unwrap().key_generation, 7);
        drop(std::fs::remove_dir_all(&dir));
    }

    /// X-DELTA-AZDRIVE-KEYS-33 (e): the update flow keeps its state in the user's own data folder
    /// and has none when that is not set - never the shared temporary folder. Red on the old
    /// code, which fell back to `temp_dir()/<app>` for the marks and the staged update.
    #[test]
    fn the_update_state_has_no_shared_temp_folder_to_fall_back_to() {
        use std::ffi::OsString;
        let abs = |p: &str| {
            OsString::from(if cfg!(windows) {
                format!("C:{p}")
            } else {
                p.to_owned()
            })
        };

        // Nothing set: no folder at all.
        assert_eq!(data_home_from(None, None, None), None);
        let err = private_state_dir_in(None, "azwriter").unwrap_err();
        assert!(err.contains("not set"), "{err}");

        // Empty or relative values are not a folder (an empty HOME is the working directory).
        for bad in ["", "relative/path", "."] {
            assert_eq!(
                data_home_from(
                    Some(OsString::from(bad)),
                    Some(OsString::from(bad)),
                    Some(OsString::from(bad))
                ),
                None,
                "{bad:?}"
            );
        }

        // A set one is the platform's data folder.
        let home = data_home_from(
            Some(abs("/home/u")),
            Some(abs("/xdg")),
            Some(abs("/appdata")),
        )
        .expect("an absolute variable is a folder");
        if cfg!(target_os = "windows") {
            assert!(home.ends_with("appdata"));
        } else if cfg!(target_os = "macos") {
            assert!(home.ends_with("Library/Application Support"), "{home:?}");
        } else {
            assert!(home.ends_with("xdg"), "{home:?}");
            // XDG_DATA_HOME unset: under HOME.
            let fallback = data_home_from(Some(abs("/home/u")), None, None).unwrap();
            assert!(fallback.ends_with(".local/share"), "{fallback:?}");
            // XDG_DATA_HOME relative: ignored, as the spec says.
            let ignored =
                data_home_from(Some(abs("/home/u")), Some(OsString::from("rel")), None).unwrap();
            assert!(ignored.ends_with(".local/share"), "{ignored:?}");
        }
        assert_eq!(
            private_state_dir_in(Some(home.clone()), "azwriter"),
            Ok(home.join("azwriter"))
        );

        // The app name is one folder name.
        for bad in ["", ".", "..", "a/b", "a\\b", "c:x", "x\0y"] {
            assert!(
                private_state_dir_in(Some(home.clone()), bad).is_err(),
                "{bad:?}"
            );
        }
    }

    /// AZ-Y11 through the mark: a client that verified `2.0.0-rc1` takes the final `2.0.0` as a
    /// newer release and its mark moves to it (it used to stay at the rc, and the final was
    /// refused as `downgrade_refused`).
    #[test]
    fn y11_the_final_release_moves_the_mark_past_a_release_candidate() {
        let f = chain_fixture(CHAIN_NOW + DAY, 1, "y11-mark");
        let mut state = UpdateState {
            highest_seen: "2.0.0-rc1".into(),
            ..UpdateState::default()
        };
        verify_release_signature(
            &f.artifact,
            &f.release,
            &roots_of(&f),
            &expect(),
            &mut state,
            CHAIN_NOW,
        )
        .expect("the genuine 2.0.0 verifies");
        assert_eq!(state.highest_seen, "2.0.0");
        drop(std::fs::remove_dir_all(&f._dir));
    }

    /// The manifest is unauthenticated: it can say "999.0.0". The mark that refuses every genuine
    /// release afterwards must not move on its word (the freeze of REVIEW Y3).
    #[test]
    fn y3_a_manifest_that_claims_999_moves_no_highest_seen() {
        let manifest = r#"{"latest": {"version": "999.0.0", "download_url": "http://127.0.0.1:9/app.bin", "slow": "off"}}"#;
        let url = serve_bytes(manifest.as_bytes().to_vec(), "updates.json");
        let mut state = UpdateState::default();
        let result = check_for_updates_blocking(
            &url,
            "1.0.0",
            "stable",
            &mut state,
            UpdateAudience::AutoUpdate,
        );
        assert!(
            matches!(&result, UpdateCheckResult::Available(r) if r.version.as_str() == "999.0.0"),
            "{result:?}"
        );
        assert_eq!(
            state.highest_seen, "",
            "an unverified manifest moves no mark"
        );
    }

    // ---- slow rollout -----------------------------------------------------

    const DAY: u64 = 86_400;
    const REL: u64 = 1_800_000_000;

    fn manifest_with(extra: &str) -> String {
        format!(
            r#"{{"latest": {{"version": "2.0.0", "download_url": "http://x/a.bin",
                 "changelog_md": "", "digest": ""{extra}}}}}"#
        )
    }

    #[test]
    fn explicit_slow_map_parses_sorted_and_gates_by_time() {
        let json = manifest_with(&format!(
            r#", "release_date": {REL},
                "slow": {{"50": {}, "10": {}}}"#,
            REL + 2 * DAY,
            REL + DAY
        ));
        let (_, plan) = parse_manifest_v1(&json).expect("manifest parses");
        // Before day 1: nobody. After day 1: 10%. After day 2 (the LAST
        // stage): the rollout has run its course = 100%.
        assert_eq!(plan.allowed_percent(REL), 0);
        assert_eq!(plan.allowed_percent(REL + DAY + 1), 10);
        assert_eq!(plan.allowed_percent(REL + 2 * DAY + 1), 100);
    }

    #[test]
    fn slow_off_means_immediate_and_absent_release_date_means_immediate() {
        let (_, plan) =
            parse_manifest_v1(&manifest_with(r#", "slow": "off""#)).expect("manifest parses");
        assert_eq!(plan, RolloutPlan::Immediate);
        // No slow AND no release_date: nothing to ladder from.
        let (_, plan) = parse_manifest_v1(&manifest_with("")).expect("manifest parses");
        assert_eq!(plan, RolloutPlan::Immediate);
    }

    #[test]
    fn default_ladder_is_on_by_default_when_release_date_exists() {
        // THE default: 1d/10, 2d/30, 3d/50, 4d/100 — no "slow" key needed.
        let (_, plan) = parse_manifest_v1(&manifest_with(&format!(r#", "release_date": {REL}"#)))
            .expect("manifest parses");
        assert_eq!(
            plan.allowed_percent(REL + DAY / 2),
            0,
            "release day: nobody"
        );
        assert_eq!(plan.allowed_percent(REL + DAY), 10);
        assert_eq!(plan.allowed_percent(REL + 2 * DAY), 30);
        assert_eq!(plan.allowed_percent(REL + 3 * DAY), 50);
        assert_eq!(plan.allowed_percent(REL + 4 * DAY), 100);
        assert!(!plan.is_complete(REL + 3 * DAY));
        assert!(plan.is_complete(REL + 4 * DAY));
    }

    #[test]
    fn auto_audience_gates_by_bucket_and_notify_waits_for_full_rollout() {
        let plan = RolloutPlan::default_ladder(REL);
        let day1 = REL + DAY; // 10% open
                              // Cohort bucket 5 is inside the first 10%; bucket 42 is not.
        assert!(5 < plan.allowed_percent(day1));
        assert!(42 >= plan.allowed_percent(day1));
        // bucket 42 opens at day 3 (50%)...
        assert!(42 < plan.allowed_percent(REL + 3 * DAY));
        // ...but the NOTIFY audience (system-installed) sees NOTHING until
        // 100%: the "please update" hint must not race the fleet cooldown.
        assert!(!plan.is_complete(REL + 3 * DAY));
        assert!(plan.is_complete(REL + 4 * DAY));
    }

    #[test]
    fn rollout_bucket_draws_once_persists_and_env_overrides() {
        let dir = std::env::temp_dir().join(format!("azul-bucket-test-{}", std::process::id()));
        let mut state = UpdateState::default();
        let drawn = state.rollout_bucket();
        assert!(drawn < 100);
        assert_eq!(state.rollout_bucket(), drawn, "second draw = same cohort");
        state.save(&dir);
        let mut reloaded = UpdateState::load(&dir);
        assert_eq!(reloaded.rollout_bucket(), drawn, "cohort survives restart");
        drop(std::fs::remove_dir_all(&dir));
    }

    #[test]
    fn manifest_datetime_accepts_unix_and_iso_forms() {
        use serde_json::json;
        assert_eq!(
            parse_manifest_datetime(&json!(1_800_000_000_u64)),
            Some(1_800_000_000)
        );
        assert_eq!(
            parse_manifest_datetime(&json!("1800000000")),
            Some(1_800_000_000)
        );
        // 2026-08-18 00:00:00 UTC = 1787011200 (python datetime oracle).
        assert_eq!(
            parse_manifest_datetime(&json!("2026-08-18")),
            Some(1_787_011_200)
        );
        assert_eq!(
            parse_manifest_datetime(&json!("2026-08-18T01:30:00Z")),
            Some(1_787_011_200 + 5400)
        );
        assert_eq!(parse_manifest_datetime(&json!("not a date")), None);
        assert_eq!(
            parse_manifest_datetime(&json!("2026-13-01")),
            None,
            "month 13"
        );
    }

    // ---- digest verification -------------------------------------------

    #[test]
    fn digest_verifies_matches_and_rejects_mismatch_and_garbage() {
        let dir = std::env::temp_dir().join(format!("azul-digest-test-{}", std::process::id()));
        drop(std::fs::create_dir_all(&dir));
        let file = dir.join("artifact.bin");
        std::fs::write(&file, b"hello update artifact").expect("write");
        // sha256 of the exact bytes above (python hashlib oracle).
        let good = "f58c1a1c453ed138b0ffa8050949df98ab92da901f592bc92afa50f8391d8f75";

        // An empty digest pins NOTHING: it is an error, never a pass (REVIEW Y1, X-AR5b-25).
        assert!(
            verify_digest(&file, "").is_err(),
            "empty digest = no pin = not verified"
        );
        assert!(
            verify_digest(&file, "  \n").is_err(),
            "a blank digest pins nothing either"
        );
        assert!(verify_digest(&file, good).is_ok(), "bare hex form");
        assert!(
            verify_digest(&file, &format!("sha256:{good}")).is_ok(),
            "prefixed form"
        );
        assert!(
            verify_digest(&file, &good.to_uppercase()).is_ok(),
            "hex compare is case-insensitive"
        );

        // THE LAW: a wrong pin is a hard error naming both hashes.
        let wrong = format!("sha256:{}", "0".repeat(64));
        let err = verify_digest(&file, &wrong).expect_err("mismatch must fail");
        assert!(err.contains("mismatch"), "{err}");
        // Unsupported scheme fails loudly instead of verifying nothing.
        assert!(verify_digest(&file, "md5:abcd").is_err());
        assert!(verify_digest(&file, "not-a-digest").is_err());
        drop(std::fs::remove_dir_all(&dir));
    }

    // ---- a manager updating other programs --------------------------------

    #[test]
    fn a_tester_takes_the_newest_release_however_far_its_rollout_has_come() {
        let unopened = RolloutPlan::Staged(vec![RolloutStage {
            percent: 100,
            at_unix: REL + 4 * DAY,
        }]);
        let candidates = vec![
            candidate("2.0.0", RolloutPlan::Immediate),
            candidate("2.1.0", unopened),
        ];
        let tester =
            select_update_candidate(&candidates, "1.0.0", 99, UpdateAudience::Tester, REL).unwrap();
        assert_eq!(
            tester.release.version.as_str(),
            "2.1.0",
            "a tester sees a release the moment it is published"
        );
        let stable =
            select_update_candidate(&candidates, "1.0.0", 99, UpdateAudience::AutoUpdate, REL)
                .unwrap();
        assert_eq!(
            stable.release.version.as_str(),
            "2.0.0",
            "the same cohort on the gated path still waits for 2.1.0"
        );
    }

    /// A localhost HTTP/1.1 server answering every request with `body`; the
    /// URL of `name` on it.
    fn serve_bytes(body: Vec<u8>, name: &str) -> String {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!("http://{}/{name}", listener.local_addr().expect("addr"));
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                'requests: loop {
                    // One request head, up to its empty line.
                    let mut line = String::new();
                    loop {
                        line.clear();
                        match reader.read_line(&mut line) {
                            Ok(0) | Err(_) => break 'requests,
                            Ok(_) if line == "\r\n" => break,
                            Ok(_) => {}
                        }
                    }
                    let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
                    if stream.write_all(head.as_bytes()).is_err()
                        || stream.write_all(&body).is_err()
                    {
                        break;
                    }
                }
            }
        });
        url
    }

    fn unix_now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    }

    #[test]
    fn a_manager_checks_another_program_in_its_own_state_folder_under_the_machines_cohort() {
        let now = unix_now();
        // Released a day ago, half the fleet open: cohort 10 is in, cohort 90 is not yet.
        let manifest = format!(
            r#"{{"latest": {{"version": "2.0.0", "download_url": "http://127.0.0.1:9/app.bin",
                "release_date": {}, "slow": {{"50": {}, "100": {}}}}}}}"#,
            now - DAY,
            now - DAY,
            now + 3 * DAY
        );
        let url = serve_bytes(manifest.into_bytes(), "updates.json");
        let dir = std::env::temp_dir().join(format!("azul-program-check-{}", std::process::id()));
        let inside = check_program_for_updates(
            &url,
            "1.0.0",
            "stable",
            &dir.join("in"),
            10,
            UpdateAudience::AutoUpdate,
        );
        assert!(
            matches!(&inside, UpdateCheckResult::Available(r) if r.version.as_str() == "2.0.0"),
            "{inside:?}"
        );
        let outside = check_program_for_updates(
            &url,
            "1.0.0",
            "stable",
            &dir.join("out"),
            90,
            UpdateAudience::AutoUpdate,
        );
        assert_eq!(
            outside,
            UpdateCheckResult::UpToDate,
            "the rollout has not reached cohort 90"
        );
        let tester = check_program_for_updates(
            &url,
            "1.0.0",
            "beta",
            &dir.join("beta"),
            90,
            UpdateAudience::Tester,
        );
        assert!(
            matches!(tester, UpdateCheckResult::Available(_)),
            "{tester:?}"
        );
        // The check's bookkeeping is the program's own, the machine's cohort with it.
        let state = UpdateState::load(&dir.join("out"));
        assert_eq!(state.rollout_bucket, Some(90));
        // A check reads an unsigned manifest: it moves no mark (REVIEW Y3). The mark moves in
        // `verify_release_signature` only; this assertion used to expect "2.0.0" and was red.
        assert_eq!(state.highest_seen, "");
        drop(std::fs::remove_dir_all(&dir));
    }

    #[test]
    fn a_manager_stages_another_programs_update_verified_in_that_programs_folders() {
        let f = chain_fixture(CHAIN_NOW * 2, 2, "stage");
        let mut release = f.release.clone();
        release.download_url =
            serve_bytes(b"the update artifact bytes".to_vec(), "app-2.0.0.bin").into();
        let staged = stage_program_update(
            &release,
            FIXTURE_APP,
            &f._dir.join("staging"),
            &f.root_pub_b64,
            false,
            &f._dir.join("state"),
        )
        .expect("a signed artifact is staged");
        assert_eq!(staged.path(), f._dir.join("staging").join("app-2.0.0.bin"));
        assert_eq!(
            std::fs::read(staged.path()).unwrap(),
            b"the update artifact bytes"
        );
        assert_eq!(staged.sha256_hex(), sha256_hex(&sha256_of(FIXTURE_BYTES)));
        assert_eq!(UpdateState::load(&f._dir.join("state")).key_generation, 2);

        // The statement's signature is no signature of the artifact.
        let mut forged = release.clone();
        forged.signature = f.release.signing_key_statement_sig.clone();
        let err = stage_program_update(
            &forged,
            FIXTURE_APP,
            &f._dir.join("staging-forged"),
            &f.root_pub_b64,
            false,
            &f._dir.join("state"),
        )
        .expect_err("a forged signature is refused");
        assert!(err.contains("signature"), "{err}");
        let left = std::fs::read_dir(f._dir.join("staging-forged")).map_or(0, Iterator::count);
        assert_eq!(left, 0, "nothing refused stays staged");
        drop(std::fs::remove_dir_all(&f._dir));
    }

    #[test]
    fn a_manager_verifies_a_staged_program_by_its_digest_and_its_signature_chain() {
        let f = chain_fixture(CHAIN_NOW * 2, 4, "program");
        let state_dir = f._dir.join("state");
        verify_staged_program_update(
            &f.artifact,
            &f.release,
            FIXTURE_APP,
            &f.root_pub_b64,
            false,
            &state_dir,
        )
        .expect("the chain verifies");
        assert_eq!(
            UpdateState::load(&state_dir).key_generation,
            4,
            "the accepted generation is kept in the program's folder"
        );

        let mut wrong_pin = f.release.clone();
        wrong_pin.digest = format!("sha256:{}", "0".repeat(64)).into();
        let err = verify_staged_program_update(
            &f.artifact,
            &wrong_pin,
            FIXTURE_APP,
            &f.root_pub_b64,
            false,
            &state_dir,
        )
        .expect_err("a wrong pin is refused besides a good chain");
        assert!(err.contains("mismatch"), "{err}");

        std::fs::write(&f.artifact, b"tampered bytes").unwrap();
        assert!(
            verify_staged_program_update(
                &f.artifact,
                &f.release,
                FIXTURE_APP,
                &f.root_pub_b64,
                false,
                &state_dir
            )
            .is_err(),
            "a file changed after staging is refused"
        );
        drop(std::fs::remove_dir_all(&f._dir));
    }

    #[test]
    fn an_empty_maintenance_window_is_any_time_and_a_rule_confines_unattended_work() {
        assert!(within_maintenance_window("", 0));
        assert!(within_maintenance_window("  ", 12_345));
        #[cfg(feature = "telemetry")]
        {
            // 2026-01-01T00:00:00Z; the window opens daily at 02:00 UTC for two hours.
            let midnight = 1_767_225_600;
            let rule = "FREQ=DAILY;BYHOUR=2;DURATION=PT2H";
            assert!(within_maintenance_window(rule, midnight + 2 * 3600 + 1800));
            assert!(!within_maintenance_window(rule, midnight + 5 * 3600));
        }
    }

    #[test]
    fn the_machines_cohort_is_a_bucket_from_0_to_99() {
        assert!(draw_rollout_bucket() < 100);
    }
}
