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
    /// Hex digest of the artifact (verified when non-empty; the minisign
    /// chain is the next rung and rides the same field).
    pub digest: AzString,
}

/// What a check concluded.
// `Available` carries the whole ReleaseInfo — boxing is not an option in a
// repr(C,u8) ABI enum whose layout the C bindings will depend on.
#[allow(variant_size_differences)]
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
        let bucket = u8::try_from((u64::from(nanos) ^ u64::from(std::process::id())) % 100)
            .unwrap_or(0);
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
        });
        drop(std::fs::create_dir_all(dir));
        drop(std::fs::write(dir.join("update-state.json"), value.to_string()));
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
            RolloutStage { percent: 10, at_unix: release_unix + DAY },
            RolloutStage { percent: 30, at_unix: release_unix + 2 * DAY },
            RolloutStage { percent: 50, at_unix: release_unix + 3 * DAY },
            RolloutStage { percent: 100, at_unix: release_unix + 4 * DAY },
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
                if last_open { 100 } else { reached }
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
    let release_date = latest
        .get("release_date")
        .and_then(parse_manifest_datetime);
    match latest.get("slow") {
        Some(serde_json::Value::String(s)) if s.eq_ignore_ascii_case("off") => {
            RolloutPlan::Immediate
        }
        Some(serde_json::Value::Object(map)) => {
            let mut stages: Vec<RolloutStage> = map
                .iter()
                .filter_map(|(percent, when)| {
                    let percent: u8 = percent.trim().parse().ok().filter(|p| (1..=100).contains(p))?;
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
        digest: get("digest").into(),
    };
    Ok((release, parse_rollout(latest)))
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
    state: &mut UpdateState,
    audience: UpdateAudience,
) -> UpdateCheckResult {
    use core::cmp::Ordering;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    state.last_check_unix = now;

    let response = match crate::http::http_get_with_config(manifest_url, &crate::http::HttpRequestConfig::new()) {
        Ok(r) if (200..300).contains(&r.status_code) => r,
        Ok(r) => {
            record_check("error");
            return UpdateCheckResult::Error(format!("manifest HTTP {}", r.status_code).into());
        }
        Err(e) => {
            record_check("error");
            return UpdateCheckResult::Error(format!("manifest fetch: {e:?}").into());
        }
    };
    let body = String::from_utf8_lossy(response.body.as_ref());
    let (release, rollout) = match parse_manifest_v1(&body) {
        Ok(r) => r,
        Err(e) => {
            record_check("error");
            return UpdateCheckResult::Error(e.into());
        }
    };

    // Anti-downgrade: never offer less than the highest version ever seen.
    let offered = release.version.as_str().to_owned();
    if !state.highest_seen.is_empty()
        && compare_versions(&offered, &state.highest_seen) == Ordering::Less
    {
        record_check("downgrade_refused");
        return UpdateCheckResult::UpToDate;
    }
    if compare_versions(&offered, &state.highest_seen) == Ordering::Greater {
        state.highest_seen.clone_from(&offered);
    }

    if state.suspended_until_unix > now {
        record_check("suspended");
        return UpdateCheckResult::UpToDate;
    }

    if compare_versions(&offered, current_version) != Ordering::Greater {
        record_check("up_to_date");
        return UpdateCheckResult::UpToDate;
    }

    // SLOW-ROLLOUT gate: a newer version exists, but this client may not be
    // in the open cohort yet. Auto-updaters compare their persistent bucket
    // against the currently-open stage; notify-only installs (package-
    // managed) do not even see the notification until the rollout hits
    // 100%. A gated client reports UpToDate — from the app's point of view
    // the release does not exist for it YET.
    let eligible = match audience {
        UpdateAudience::AutoUpdate => state.rollout_bucket() < rollout.allowed_percent(now),
        UpdateAudience::NotifyOnly => rollout.is_complete(now),
    };
    if !eligible {
        record_check("staggered");
        return UpdateCheckResult::UpToDate;
    }
    record_check("available");
    UpdateCheckResult::Available(release)
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
        return Err(format!("unsupported digest format: {digest:?} (expected sha256 hex)"));
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
    std::fs::create_dir_all(staging_dir).map_err(|e| e.to_string())?;
    let file_name = release
        .download_url
        .as_str()
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .map_or_else(
            || format!("update-{}.bin", release.version.as_str()),
            str::to_owned,
        );
    let final_path = staging_dir.join(&file_name);
    if final_path.exists() {
        // A cached artifact is only reusable if it still matches the pin —
        // a stale or corrupted staging file must re-download, not install.
        if let Err(e) = verify_digest(&final_path, release.digest.as_str()) {
            drop(std::fs::remove_file(&final_path));
            return Err(format!("cached artifact failed verification ({e}); removed — retry the download"));
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
        let config = crate::http::HttpRequestConfig::new().with_header(
            "Range",
            format!("bytes={offset}-{}", offset + CHUNK - 1),
        );
        let response =
            crate::http::http_get_with_config(release.download_url.as_str(), &config)
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
        write!(f, "UpdateCheckCallback {{ cb: {:p} }}", self.cb as *const ())
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
    let base = std::env::var_os("APPDATA")
        .map_or_else(std::env::temp_dir, PathBuf::from);
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
    let effective = effective_mode(task.env.update_mode, &install);
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
            let r = check_for_updates_blocking(url, &task.env.current_version, &mut state, audience);
            state.save(&state_dir);
            r
        }
    };

    // `download_automatically` STAGES so a later "install now" is instant;
    // it installs nothing, and a notify-only install never even stages.
    let staged_path = match (&result, task.options.download_automatically, effective) {
        (UpdateCheckResult::Available(release), true, UpdateMode::SelfUpdate) => {
            match download_update(release, &state_dir.join("staging")) {
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
        };
        state.save(&dir);
        assert_eq!(UpdateState::load(&dir), state);
        drop(std::fs::remove_dir_all(&dir));
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
        let (_, plan) = parse_manifest_v1(&manifest_with(r#", "slow": "off""#))
            .expect("manifest parses");
        assert_eq!(plan, RolloutPlan::Immediate);
        // No slow AND no release_date: nothing to ladder from.
        let (_, plan) = parse_manifest_v1(&manifest_with("")).expect("manifest parses");
        assert_eq!(plan, RolloutPlan::Immediate);
    }

    #[test]
    fn default_ladder_is_on_by_default_when_release_date_exists() {
        // THE default: 1d/10, 2d/30, 3d/50, 4d/100 — no "slow" key needed.
        let (_, plan) = parse_manifest_v1(&manifest_with(&format!(
            r#", "release_date": {REL}"#
        )))
        .expect("manifest parses");
        assert_eq!(plan.allowed_percent(REL + DAY / 2), 0, "release day: nobody");
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
        assert_eq!(parse_manifest_datetime(&json!(1_800_000_000_u64)), Some(1_800_000_000));
        assert_eq!(parse_manifest_datetime(&json!("1800000000")), Some(1_800_000_000));
        // 2026-08-18 00:00:00 UTC = 1787011200 (python datetime oracle).
        assert_eq!(parse_manifest_datetime(&json!("2026-08-18")), Some(1_787_011_200));
        assert_eq!(
            parse_manifest_datetime(&json!("2026-08-18T01:30:00Z")),
            Some(1_787_011_200 + 5400)
        );
        assert_eq!(parse_manifest_datetime(&json!("not a date")), None);
        assert_eq!(parse_manifest_datetime(&json!("2026-13-01")), None, "month 13");
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

