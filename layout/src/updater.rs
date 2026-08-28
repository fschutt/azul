//! Version checking + update policy — Phase 4 of the observability plan.
//!
//! The posture, in order:
//!
//! 1. **The system package manager owns managed installs.** If the running
//!    binary is dpkg/rpm-owned, lives under `/usr`, or runs inside a
//!    Snap/Flatpak/`WindowsApps` sandbox, self-update is REFUSED at runtime
//!    and the effective mode clamps to [`UpdateMode::NotifyOnly`]: the
//!    dialog says "a new version is available — update via your package
//!    manager", never touches the binary.
//! 2. **Notify-only elsewhere unless the app opts into self-update.** The
//!    user's choice to update is always respected: nothing installs without
//!    the dialog's consent, and `download_automatically` only STAGES the
//!    artifact so consent applies instantly.
//! 3. Checks are ASYNC (the `CallbackInfo::check_for_updates` wrapper runs
//!    this module on an azul `Thread`) and observed through the existing
//!    `app_update_check_total` / `app_update_apply_total` metrics.
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

/// Dotted-numeric version compare (`1.10.2 > 1.9.9`); non-numeric segments
/// compare lexically as a tiebreaker. Enough for the manifest v1; a full
/// `VersionScheme` trait rides in when a source needs another scheme.
#[must_use]
pub fn compare_versions(a: &str, b: &str) -> core::cmp::Ordering {
    let mut xs = a.trim_start_matches('v').split('.');
    let mut ys = b.trim_start_matches('v').split('.');
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
}

impl UpdateState {
    /// Loads from `dir/update-state.json` (missing file = default).
    #[must_use]
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("update-state.json");
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            return Self::default();
        };
        Self {
            highest_seen: v
                .get("highest_seen")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_owned(),
            last_check_unix: v
                .get("last_check_unix")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
            suspended_until_unix: v
                .get("suspended_until_unix")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
            rollout_bucket: v
                .get("rollout_bucket")
                .and_then(serde_json::Value::as_u64)
                .and_then(|b| u8::try_from(b).ok())
                .filter(|b| *b < 100),
            key_generation: v
                .get("key_generation")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
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

    /// Writes to `dir/update-state.json`.
    pub fn save(&self, dir: &Path) {
        let value = serde_json::json!({
            "highest_seen": self.highest_seen,
            "last_check_unix": self.last_check_unix,
            "suspended_until_unix": self.suspended_until_unix,
            "rollout_bucket": self.rollout_bucket,
            "key_generation": self.key_generation,
        });
        drop(std::fs::create_dir_all(dir));
        drop(std::fs::write(
            dir.join("update-state.json"),
            value.to_string(),
        ));
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
pub enum UpdateAudience {
    /// Self-updating install: eligible once its cohort bucket falls under
    /// the currently-open percent.
    AutoUpdate,
    /// Package-managed / notify-only install: sees the notification only
    /// after the rollout completes (100%).
    NotifyOnly,
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
                    "the OCI manifest for {}:{} has no \
                     org.opencontainers.image.version annotation, and the reference does not \
                     name a version either",
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
            &trimmed[..trimmed.len().min(40)]
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
    // is eligible for while a newer one is still rolling out.
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
    if compare_versions(&highest_offered, &state.highest_seen) == Ordering::Greater {
        state.highest_seen.clone_from(&highest_offered);
    }

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

/// Verifies a staged artifact against the manifest's `digest` field.
///
/// Accepted forms: `sha256:<hex>` or bare hex (interpreted as SHA-256).
/// An EMPTY digest verifies trivially — the manifest simply did not pin
/// one (the minisign signature chain is the next rung and rides the same
/// field). A non-empty digest that does not match is a hard error and the
/// caller must DISCARD the file: a mismatch is corruption or tampering,
/// never something to install.
///
/// # Errors
///
/// Returns a description on IO failure, an unsupported digest scheme, or
/// a mismatch (the message names both hashes).
pub fn verify_digest(path: &Path, digest: &str) -> Result<(), String> {
    let digest = digest.trim();
    if digest.is_empty() {
        return Ok(());
    }
    let expected = digest
        .strip_prefix("sha256:")
        .unwrap_or(digest)
        .to_ascii_lowercase();
    if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!(
            "unsupported digest format: {digest:?} (expected sha256 hex)"
        ));
    }
    use sha2::Digest as _;
    let bytes = std::fs::read(path).map_err(|e| format!("digest read: {e}"))?;
    let actual = sha2::Sha256::digest(&bytes);
    use core::fmt::Write as _;
    let actual_hex = actual.iter().fold(String::with_capacity(64), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    });
    if actual_hex == expected {
        Ok(())
    } else {
        Err(format!(
            "digest mismatch: manifest pinned sha256:{expected}, downloaded file is sha256:{actual_hex}"
        ))
    }
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
            "signing-key statement: unknown format (expected `azul-signing-key-v1|…`, got {statement:?})"
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

/// Verifies a staged artifact's SIGNATURE CHAIN against the app's compiled-in
/// minisign root public key:
///
/// 1. the ROOT key must verify `signing_key_statement_sig` over the
///    statement string — only the root can appoint a signing key;
/// 2. the statement must not be expired and its `generation` must be at
///    least `state.key_generation` (rollback refusal; the high-water mark
///    advances on success);
/// 3. the SIGNING key from the statement must verify `signature` over the
///    artifact bytes.
///
/// An EMPTY `root_public_key` means the app does not pin one — the chain is
/// unarmed and verifies trivially (the digest pin still applies). With a
/// root key pinned, a manifest that omits any part of the chain is a HARD
/// error: "this release is unsigned" must never be a downgrade path.
///
/// # Errors
///
/// Returns a description naming the failing link; the caller must DISCARD
/// the artifact.
pub fn verify_release_signature(
    path: &Path,
    release: &ReleaseInfo,
    root_public_key: &str,
    state: &mut UpdateState,
    now_unix: u64,
) -> Result<(), String> {
    let root_public_key = root_public_key.trim();
    if root_public_key.is_empty() {
        return Ok(());
    }
    if release.signature.as_str().trim().is_empty()
        || release.signing_key_statement.as_str().trim().is_empty()
        || release.signing_key_statement_sig.as_str().trim().is_empty()
    {
        return Err(
            "this app pins an update root key but the manifest offers an UNSIGNED release \
             (missing signature / signing_key_statement / signing_key_statement_sig)"
                .to_owned(),
        );
    }

    let root = minisign_verify::PublicKey::from_base64(root_public_key)
        .map_err(|e| format!("root public key: {e}"))?;
    let statement_str = release.signing_key_statement.as_str();
    let statement_sig =
        minisign_verify::Signature::decode(release.signing_key_statement_sig.as_str())
            .map_err(|e| format!("signing-key statement signature: {e}"))?;
    root.verify(statement_str.as_bytes(), &statement_sig, false)
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
            "signing-key generation ROLLBACK: statement is generation {} but this client \
             already accepted generation {}",
            statement.generation, state.key_generation
        ));
    }

    let signing = minisign_verify::PublicKey::from_base64(&statement.pubkey_b64)
        .map_err(|e| format!("signing public key (from statement): {e}"))?;
    let artifact_sig = minisign_verify::Signature::decode(release.signature.as_str())
        .map_err(|e| format!("artifact signature: {e}"))?;
    let bytes = std::fs::read(path).map_err(|e| format!("signature read: {e}"))?;
    signing
        .verify(&bytes, &artifact_sig, false)
        .map_err(|e| format!("artifact signature invalid: {e}"))?;

    // Only a fully verified chain advances the rotation high-water mark.
    state.key_generation = state.key_generation.max(statement.generation);
    Ok(())
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

/// Downloads a release artifact into `staging_dir`, CACHED and RESUMABLE:
///
/// * a fully staged artifact from an earlier call is reused verbatim
///   (`used_cached`) — "download automatically" then makes "install now"
///   instant and offline-safe;
/// * an INTERRUPTED download leaves `<name>.partial`; the next call sends
///   `Range: bytes=<len>-` and appends on `206 Partial Content`. A server
///   without range support answers `200` and the file restarts from zero
///   (correct, just not incremental) — `server_supports_resume` reports
///   which happened.
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
    // An OCI blob URL ends in `sha256:<hex>`, and `:` cannot appear in a
    // Windows filename — sanitise anything that is not safe everywhere.
    let file_name = release
        .download_url
        .as_str()
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .map_or_else(
            || format!("update-{}.bin", release.version.as_str()),
            |n| {
                n.chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+') {
                            c
                        } else {
                            '-'
                        }
                    })
                    .collect()
            },
        );
    let final_path = staging_dir.join(&file_name);
    if final_path.exists() {
        // A cached artifact is only reusable if it still matches the pin —
        // a stale or corrupted staging file must re-download, not install.
        if let Err(e) = verify_digest(&final_path, release.digest.as_str()) {
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
                honored = true;
                let bytes = response.body.as_ref();
                if !bytes.is_empty() {
                    let mut file = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&partial_path)
                        .map_err(|e| e.to_string())?;
                    file.write_all(bytes).map_err(|e| e.to_string())?;
                    offset += bytes.len() as u64;
                    written_this_call += bytes.len() as u64;
                }
                // A short (or empty) chunk is the end of the file.
                if (bytes.len() as u64) < CHUNK {
                    break;
                }
            }
            // 200 = the server ignored the Range header: it sent the WHOLE
            // file; a previous partial is superseded.
            200 => {
                let bytes = response.body.as_ref();
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
    if let Err(e) = verify_digest(&partial_path, release.digest.as_str()) {
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

/// [`download_update`] plus the FULL verification story — the one entry
/// point both the update worker and the install dialog use, so no staged
/// artifact can reach `apply_update` without passing the digest pin AND the
/// signature chain ([`verify_release_signature`]) on THIS call. A cached
/// staging file is re-verified every time (disk contents are not trusted
/// across runs); any failure deletes the artifact so the next attempt
/// starts clean. `state`'s `key_generation` high-water mark advances (and
/// is saved by the caller) on success.
///
/// # Errors
///
/// Returns a description on transport, digest or signature-chain failure.
#[cfg(feature = "http")]
pub fn download_and_verify(
    release: &ReleaseInfo,
    staging_dir: &Path,
    root_public_key: &str,
    state: &mut UpdateState,
) -> Result<DownloadOutcome, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let outcome = download_update(release, staging_dir)?;
    if let Err(e) = verify_release_signature(&outcome.path, release, root_public_key, state, now) {
        drop(std::fs::remove_file(&outcome.path));
        return Err(format!(
            "staged artifact failed signature verification ({e}); removed"
        ));
    }
    Ok(outcome)
}

/// Applies a staged update over `target`: same-directory temp copy +
/// ATOMIC rename (the classic swap; on Windows a running image cannot be
/// replaced in place — the rename dance moves the OLD file aside first).
/// Callers verify [`InstallKind::allows_self_update`] first — this
/// function only does the mechanics.
///
/// # Errors
///
/// Returns a description on filesystem failure.
pub fn apply_update(staged: &Path, target: &Path) -> Result<(), String> {
    let dir = target
        .parent()
        .ok_or_else(|| "target has no parent directory".to_owned())?;
    let incoming = dir.join(".update-incoming");
    std::fs::copy(staged, &incoming).map_err(|e| format!("stage copy: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        drop(std::fs::set_permissions(&incoming, perms));
    }
    #[cfg(windows)]
    {
        // A running image is locked against replacement but CAN be renamed:
        // move it aside, then move the new file in.
        let old = dir.join(".update-previous");
        drop(std::fs::remove_file(&old));
        std::fs::rename(target, &old).map_err(|e| format!("move-aside: {e}"))?;
    }
    std::fs::rename(&incoming, target).map_err(|e| format!("swap: {e}"))?;
    Ok(())
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

/// Platform data directory for the updater's state
/// (`update-state.json` + the staging area), keyed by app name.
#[must_use]
pub fn default_state_dir(app_name: &str) -> PathBuf {
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("APPDATA").map_or_else(std::env::temp_dir, PathBuf::from);
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME").map_or_else(std::env::temp_dir, |h| {
        PathBuf::from(h).join("Library").join("Application Support")
    });
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME").map_or_else(
        || {
            std::env::var_os("HOME").map_or_else(std::env::temp_dir, |h| {
                PathBuf::from(h).join(".local").join("share")
            })
        },
        PathBuf::from,
    );
    base.join(app_name)
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
    let state_dir = default_state_dir(&task.env.app_name);

    let result = match (task.env.update_manifest.as_deref(), effective) {
        (_, UpdateMode::Disabled) => {
            UpdateCheckResult::Error("updates are disabled (UpdateMode::Disabled)".into())
        }
        (None, _) => UpdateCheckResult::Error(
            "no update manifest configured (AppConfig.updates.manifest_url)".into(),
        ),
        (Some(url), _) => {
            let audience = if effective == UpdateMode::SelfUpdate {
                UpdateAudience::AutoUpdate
            } else {
                UpdateAudience::NotifyOnly
            };
            let mut state = UpdateState::load(&state_dir);
            let r = check_for_updates_blocking(
                url,
                &task.env.current_version,
                &task.env.update_channel,
                &mut state,
                audience,
            );
            state.save(&state_dir);
            r
        }
    };

    // `download_automatically` STAGES so a later "install now" is instant;
    // it installs nothing, and a notify-only install never even stages.
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let staged_path = match (&result, task.options.download_automatically, effective) {
        // Automatic staging is UNATTENDED work: it defers to the machine's
        // maintenance window (a gated stage just happens on a later check).
        (UpdateCheckResult::Available(release), true, UpdateMode::SelfUpdate)
            if within_shared_maintenance_window(now_unix) =>
        {
            let mut state = UpdateState::load(&state_dir);
            let staged = download_and_verify(
                release,
                &state_dir.join("staging"),
                task.env.update_root_public_key.as_deref().unwrap_or(""),
                &mut state,
            );
            state.save(&state_dir);
            match staged {
                Ok(outcome) => {
                    azul_css::OptionString::Some(outcome.path.to_string_lossy().as_ref().into())
                }
                Err(_) => azul_css::OptionString::None,
            }
        }
        _ => azul_css::OptionString::None,
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
    (callback.cb)(user_data, info, check)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cmp::Ordering;

    #[test]
    fn version_compare_is_numeric_not_lexical() {
        assert_eq!(compare_versions("1.10.0", "1.9.9"), Ordering::Greater);
        assert_eq!(compare_versions("v1.2.3", "1.2.3"), Ordering::Equal);
        assert_eq!(compare_versions("1.2", "1.2.1"), Ordering::Less);
        assert_eq!(compare_versions("2.0.0", "10.0.0"), Ordering::Less);
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

        // 4. anything else is an ERROR, not a silent "up to date" — an HTML
        //    error page must not look like "no update available".
        let err = parse_release_document("<html><body>404</body></html>", None, "").unwrap_err();
        assert!(err.contains("neither JSON nor a version"), "{err}");
        assert!(parse_release_document("", None, "").is_err());
        assert!(parse_release_document(r#"{"whatever":1}"#, None, "").is_err());
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
        release: ReleaseInfo,
        artifact: std::path::PathBuf,
        _dir: std::path::PathBuf,
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
        std::fs::write(&artifact, b"the update artifact bytes").unwrap();
        let artifact_sig = minisign::sign(
            Some(&signing.pk),
            &signing.sk,
            std::io::Cursor::new(&b"the update artifact bytes"[..]),
            Some("azul release 2.0.0"),
            None,
        )
        .unwrap()
        .to_string();

        ChainFixture {
            root_pub_b64: root.pk.to_base64(),
            release: ReleaseInfo {
                version: "2.0.0".into(),
                download_url: "http://x/update-2.0.0.bin".into(),
                changelog_md_url: "".into(),
                changelog_md_inline: "".into(),
                digest: "".into(),
                signature: artifact_sig.into(),
                signing_key_statement: statement.into(),
                signing_key_statement_sig: statement_sig.into(),
                download_headers: Vec::new().into(),
            },
            artifact,
            _dir: dir,
        }
    }

    const CHAIN_NOW: u64 = 1_800_000_000;

    #[test]
    fn signature_chain_verifies_and_advances_the_generation_high_water() {
        let f = chain_fixture(CHAIN_NOW + DAY, 3, "ok");
        let mut state = UpdateState::default();
        verify_release_signature(
            &f.artifact,
            &f.release,
            &f.root_pub_b64,
            &mut state,
            CHAIN_NOW,
        )
        .expect("a well-formed chain must verify");
        assert_eq!(
            state.key_generation, 3,
            "success must advance the high-water mark"
        );

        // An UNARMED root key verifies trivially even for garbage fields.
        let mut fresh = UpdateState::default();
        verify_release_signature(&f.artifact, &f.release, "", &mut fresh, CHAIN_NOW)
            .expect("empty root key = chain not in use");
        assert_eq!(
            fresh.key_generation, 0,
            "unarmed chain must not touch state"
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
            &f.root_pub_b64,
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
            &f.root_pub_b64,
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
            &f.root_pub_b64,
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
            &foreign.pk.to_base64(),
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
            &f.root_pub_b64,
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

        assert!(verify_digest(&file, "").is_ok(), "empty digest = no pin");
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
}
