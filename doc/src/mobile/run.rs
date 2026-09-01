//! `mobile build` / `mobile run` — get a crate onto a device and drive it.
//!
//! The build step delegates to `scripts/build-android.sh` and
//! `scripts/build-ios.sh` rather than reimplementing them: those two scripts
//! are the tested path (aapt2 → zip layout → zipalign → apksigner; xcrun →
//! Info.plist → codesign), and the failure modes they already encode — the
//! `lib/<abi>/` zip prefix, asking cargo for the cdylib path instead of
//! guessing it — are not worth re-learning in Rust.
//!
//! What this adds on top is everything *around* the build: pick a device, boot
//! it if needed, install, launch, watch, screenshot, and turn an e2e scenario
//! into device input.

use std::{
    path::{Path, PathBuf},
    thread::sleep,
    time::Duration,
};

use super::{
    device::{boot_emulator, boot_simulator, Device, Driver, Platform},
    e2e::{replay_scenario, HostReplayReport},
    toolchain::{Cmd, Toolchain},
    Opts,
};

/// What we know about the crate being deployed.
pub struct Target {
    /// Cargo package name, e.g. `AzWriter`.
    pub crate_name: String,
    /// App/bundle name used for the artifact, e.g. `AzWriter`.
    pub app_name: String,
    /// `com.azul.azwriter`.
    pub bundle_id: String,
    pub manifest_dir: PathBuf,
}

impl Target {
    /// Accepts a path to a `Cargo.toml`, a directory containing one, or a bare
    /// crate name resolved under `examples/`.
    pub fn resolve(project_root: &Path, spec: &str, opts: &Opts) -> anyhow::Result<Self> {
        // main() chdirs into doc/ before dispatching, so a relative path the
        // user typed at the workspace root does not resolve as-is. Every
        // candidate is therefore tried both verbatim and against the root.
        let manifest = [
            PathBuf::from(spec),
            project_root.join(spec),
            PathBuf::from(spec).join("Cargo.toml"),
            project_root.join(spec).join("Cargo.toml"),
            project_root.join("examples").join(spec).join("Cargo.toml"),
        ]
        .into_iter()
        .find(|p| p.is_file())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "cannot find a crate for '{spec}': not a Cargo.toml, not a directory \
                 with one, and no examples/{spec}/Cargo.toml"
            )
        })?;

        let text = std::fs::read_to_string(&manifest)?;
        // Deliberately a line scan, not a TOML parse: azul-doc has no toml
        // dependency and `name = "..."` under [package] is the only field we
        // need. Stops at the first table after [package] so a [lib] name or a
        // dependency's rename cannot be mistaken for the package name.
        let mut crate_name = None;
        let mut in_package = false;
        for line in text.lines() {
            let t = line.trim();
            if t.starts_with('[') {
                in_package = t == "[package]";
                continue;
            }
            if in_package {
                if let Some(rest) = t.strip_prefix("name") {
                    if let Some(v) = rest.split('=').nth(1) {
                        crate_name = Some(v.trim().trim_matches('"').to_string());
                        break;
                    }
                }
            }
        }
        let crate_name = crate_name
            .ok_or_else(|| anyhow::anyhow!("no [package] name in {}", manifest.display()))?;

        let app_name = crate_name.clone();
        let bundle_id = opts
            .package
            .clone()
            .unwrap_or_else(|| format!("com.azul.{}", crate_name.to_lowercase().replace('-', "_")));

        Ok(Self {
            crate_name,
            app_name,
            bundle_id,
            manifest_dir: manifest.parent().unwrap_or(Path::new(".")).to_path_buf(),
        })
    }
}

/// Android component to start. The manifest template declares this subclass of
/// `NativeActivity` (it constructs the gesture/a11y bridges in `onCreate`), so
/// launching `android.app.NativeActivity` instead fails with
/// "Activity class does not exist".
pub const ANDROID_ACTIVITY: &str = "com.azul.app.AzulActivity";

fn android_triple(abi: &str) -> &'static str {
    match abi {
        "x86_64" => "x86_64-linux-android",
        "armeabi-v7a" => "armv7-linux-androideabi",
        "x86" => "i686-linux-android",
        _ => "aarch64-linux-android",
    }
}

fn android_abi_of(triple: &str) -> &'static str {
    match triple {
        "x86_64-linux-android" => "x86_64",
        "armv7-linux-androideabi" => "armeabi-v7a",
        "i686-linux-android" => "x86",
        _ => "arm64-v8a",
    }
}

/// Build the deployable artifact and return its path.
pub fn build(
    project_root: &Path,
    tc: &Toolchain,
    platform: Platform,
    target: &Target,
    opts: &Opts,
) -> anyhow::Result<PathBuf> {
    match platform {
        Platform::Android => {
            let abi = opts.abi.as_deref().unwrap_or(tc.host_abi);
            let triple = android_triple(abi);
            let mut env = tc.android_env();
            env.push(("AZ_ANDROID_NO_DEPLOY".into(), "1".into()));
            println!("\n\x1b[1m==> building {} for {triple}\x1b[0m", target.crate_name);
            Cmd::new("bash")
                .arg(
                    project_root
                        .join("scripts")
                        .join("build-android.sh")
                        .display()
                        .to_string(),
                )
                .arg(triple)
                .arg(&target.app_name)
                .arg(&target.bundle_id)
                .arg(&target.crate_name)
                .envs(env)
                .cwd(project_root)
                .run()?;
            let apk = project_root
                .join("target")
                .join("android-bundle")
                .join(format!("{}-{}", target.app_name, android_abi_of(triple)))
                .join("aligned.apk");
            if !apk.is_file() {
                anyhow::bail!("build reported success but {} is missing", apk.display());
            }
            Ok(apk)
        }
        Platform::Ios => {
            let triple = opts
                .abi
                .as_deref()
                .unwrap_or(if cfg!(target_arch = "aarch64") {
                    "aarch64-apple-ios-sim"
                } else {
                    "x86_64-apple-ios"
                });
            println!("\n\x1b[1m==> building {} for {triple}\x1b[0m", target.crate_name);
            Cmd::new("bash")
                .arg(
                    project_root
                        .join("scripts")
                        .join("build-ios.sh")
                        .display()
                        .to_string(),
                )
                .arg(triple)
                .arg(&target.crate_name)
                .envs(vec![
                    ("AZ_IOS_DRYRUN".into(), "1".into()),
                    ("APP_NAME".into(), target.app_name.clone()),
                    ("BUNDLE_ID".into(), target.bundle_id.clone()),
                ])
                .cwd(project_root)
                .run()?;
            let app = project_root
                .join("target")
                .join("ios-bundle")
                .join(format!("{}-{triple}.app", target.app_name));
            if !app.is_dir() {
                anyhow::bail!("build reported success but {} is missing", app.display());
            }
            Ok(app)
        }
    }
}

/// Acquire a device: reuse one that is already up, else boot the emulator or
/// simulator.
pub fn acquire_device(
    tc: &Toolchain,
    platform: Platform,
    opts: &Opts,
) -> anyhow::Result<Device> {
    let timeout = Duration::from_secs(opts.boot_timeout);
    match platform {
        Platform::Android => {
            let avd = opts
                .avd
                .clone()
                .unwrap_or_else(|| super::toolchain::AVD_NAME.to_string());
            let serial = boot_emulator(tc, &avd, !opts.windowed, timeout)?;
            Ok(Device::android(tc, serial))
        }
        Platform::Ios => {
            let (udid, driver) =
                boot_simulator(tc, opts.device.as_deref(), !opts.windowed, timeout)?;
            Ok(Device::ios(udid, driver))
        }
    }
}

pub struct RunReport {
    pub launched: bool,
    pub screenshot: Option<PathBuf>,
    pub log_path: PathBuf,
    pub e2e: Option<HostReplayReport>,
    /// Lines the engine logged that look like errors.
    pub errors: Vec<String>,
}

/// Install, launch, drive, and collect.
pub fn deploy_and_run(
    project_root: &Path,
    device: &Device,
    target: &Target,
    artifact: &Path,
    opts: &Opts,
) -> anyhow::Result<RunReport> {
    let out_dir = project_root
        .join("target")
        .join("mobile-run")
        .join(device.platform.name());
    std::fs::create_dir_all(&out_dir)?;

    println!("\n\x1b[1m==> installing {}\x1b[0m", artifact.display());
    // A stale install of a different signature blocks the new one; force-stop
    // first so we are not reading the previous process's log either.
    let _ = device.stop(&target.bundle_id);
    let _ = device.clear_log();
    device.install(artifact)?;

    // The on-device scenario path, for the in-process runner. Pushing it is
    // cheap and harmless even when the build has no e2e feature compiled in.
    let mut extras: Vec<(String, String)> = Vec::new();
    if let Some(scenario) = &opts.e2e {
        let remote = device.push(scenario, "az_e2e.json")?;
        println!("  scenario at {remote}");
        extras.push(("AZ_E2E".to_string(), remote));
    }

    println!("\n\x1b[1m==> launching {}\x1b[0m", target.bundle_id);
    device.launch(&target.bundle_id, ANDROID_ACTIVITY, &extras)?;

    // Give the app a moment to mount its first frame before we look at it.
    sleep(Duration::from_millis(opts.settle_ms));

    let e2e = match &opts.e2e {
        Some(scenario) => Some(replay_scenario(device, scenario, &out_dir, opts)?),
        None => None,
    };

    let shot = out_dir.join(format!("{}.png", target.app_name));
    let screenshot = match device.screenshot(&shot) {
        Ok(()) => {
            println!("  screenshot: {}", shot.display());
            Some(shot)
        }
        Err(e) => {
            println!("  screenshot failed: {e}");
            None
        }
    };

    let log = device.read_log(&target.bundle_id).unwrap_or_default();
    let log_path = out_dir.join(format!("{}.log", target.app_name));
    std::fs::write(&log_path, &log)?;

    // A native crash never reaches the engine's own logging, so we scan the
    // raw platform log for the shapes that mean the process died.
    let errors: Vec<String> = log
        .lines()
        .filter(|l| {
            l.contains("FATAL EXCEPTION")
                || l.contains("*** *** ***")
                || l.contains("signal 11")
                || l.contains("Fatal signal")
                || (l.contains("[Error]") && l.contains("azul"))
        })
        .map(|l| l.trim().to_string())
        .collect();

    let launched = log.contains("RustStdoutStderr") || log.contains("regenerate_layout");

    Ok(RunReport {
        launched,
        screenshot,
        log_path,
        e2e,
        errors,
    })
}
