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
/// clamped by [`InstallKind`] — see [`effective_mode`].
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub enum UpdateMode {
    /// Check, notify, download+swap after consent.
    SelfUpdate,
    /// Check and notify only ("new version available"); installing is the
    /// user's/packager's job.
    NotifyOnly,
    /// Never check.
    Disabled,
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
        }
    }

    /// Writes to `dir/update-state.json`.
    pub fn save(&self, dir: &Path) {
        let value = serde_json::json!({
            "highest_seen": self.highest_seen,
            "last_check_unix": self.last_check_unix,
            "suspended_until_unix": self.suspended_until_unix,
        });
        drop(std::fs::create_dir_all(dir));
        drop(std::fs::write(dir.join("update-state.json"), value.to_string()));
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
    Ok(ReleaseInfo {
        version: version.into(),
        download_url: get("download_url").into(),
        changelog_md_url: get("changelog_md").into(),
        digest: get("digest").into(),
    })
}

/// BLOCKING check against a manifest URL — run it on an azul `Thread`
/// (`CallbackInfo::check_for_updates` does exactly that), never on the UI
/// thread. Applies the anti-downgrade high-water mark and the suspension
/// window from `state`, updates `state`'s bookkeeping, and observes itself
/// through `app_update_check_total{result}`.
#[cfg(feature = "http")]
pub fn check_for_updates_blocking(
    manifest_url: &str,
    current_version: &str,
    state: &mut UpdateState,
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
    let release = match parse_manifest(&body) {
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

    if compare_versions(&offered, current_version) == Ordering::Greater {
        record_check("available");
        UpdateCheckResult::Available(release)
    } else {
        record_check("up_to_date");
        UpdateCheckResult::UpToDate
    }
}

fn record_check(result: &str) {
    #[cfg(feature = "telemetry")]
    crate::telemetry::record_update_check(result);
    #[cfg(not(feature = "telemetry"))]
    let _ = result;
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
        };
        state.save(&dir);
        assert_eq!(UpdateState::load(&dir), state);
        drop(std::fs::remove_dir_all(&dir));
    }
}
