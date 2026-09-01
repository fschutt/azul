//! Install plans for the Android and iOS toolchains.
//!
//! Both are idempotent: every step probes the filesystem first, so re-running
//! after a partial install resumes rather than repeating. The Android plan can
//! run end-to-end unattended; the iOS plan cannot, because Xcode itself is a
//! ~13 GB App Store download tied to an Apple ID. That asymmetry is stated in
//! the plan rather than papered over.

use super::{
    plan::{Plan, Step},
    toolchain::{
        Cmd, HostOs, Toolchain, ANDROID_API, AVD_NAME, BUILD_TOOLS, IMAGE_TAG, JDK_FORMULA,
        NDK_VERSION,
    },
    Opts,
};

/// Rust targets each platform needs. `cargo check` needs only these — no SDK.
pub const ANDROID_TARGETS: &[&str] = &["aarch64-linux-android", "x86_64-linux-android"];
pub const IOS_TARGETS: &[&str] = &[
    "aarch64-apple-ios",
    "aarch64-apple-ios-sim",
    "x86_64-apple-ios",
];

/// `sdkmanager` prints one licence per package and waits for `y`. There is no
/// `--accept-licenses`, so the only unattended answer is to send enough of
/// them; 40 comfortably covers a full SDK install.
fn licence_feed() -> String {
    "y\n".repeat(40)
}

pub fn android_plan(tc: &Toolchain, opts: &Opts) -> Plan {
    let abi = opts.abi.as_deref().unwrap_or(tc.host_abi);
    let api = opts.api.as_deref().unwrap_or(ANDROID_API);
    let avd = opts.avd.as_deref().unwrap_or(AVD_NAME);
    let env = tc.android_env();

    let mut plan = Plan::new(format!(
        "Android toolchain (API {api}, ABI {abi}, AVD '{avd}')"
    ));

    // --- Homebrew-provided pieces -----------------------------------------
    let brew = tc.brew_prefix.is_some();
    if !brew {
        plan.push(Step::manual(
            "Homebrew",
            "the Android command-line tools, the JDK and the platform tools all come from \
             Homebrew here; without it, install the SDK by hand and set ANDROID_HOME",
            vec![
                "/bin/bash -c \"$(curl -fsSL \
                 https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\""
                    .to_string(),
            ],
        ));
    }

    match &tc.java_home {
        Some(jh) => plan.push(Step::satisfied(
            format!("JDK ({JDK_FORMULA})"),
            format!("JAVA_HOME={}", jh.display()),
        )),
        None if brew => plan.push(Step::run(
            format!("JDK ({JDK_FORMULA})"),
            Cmd::new("brew").arg("install").arg(JDK_FORMULA),
        )),
        None => plan.push(Step::manual(
            "JDK 17",
            "sdkmanager, avdmanager, apksigner and keytool all run on it",
            vec!["install a JDK 17 and export JAVA_HOME".to_string()],
        )),
    }

    if tc.android_home_exists && tc.android_home.join("cmdline-tools").is_dir() {
        plan.push(Step::satisfied(
            "Android command-line tools",
            format!("ANDROID_HOME={}", tc.android_home.display()),
        ));
    } else if brew {
        plan.push(Step::run(
            "Android command-line tools",
            Cmd::new("brew")
                .arg("install")
                .arg("--cask")
                .arg("android-commandlinetools"),
        ));
    } else {
        plan.push(Step::manual(
            "Android command-line tools",
            "provides sdkmanager and avdmanager",
            vec!["https://developer.android.com/studio#command-line-tools-only".to_string()],
        ));
    }

    // platform-tools ships adb, which every deploy and log read goes through.
    if tc.adb().is_file() || super::toolchain::which("adb").is_some() {
        plan.push(Step::satisfied("platform-tools (adb)", ""));
    } else if brew {
        plan.push(Step::run(
            "platform-tools (adb)",
            Cmd::new("brew")
                .arg("install")
                .arg("--cask")
                .arg("android-platform-tools"),
        ));
    } else {
        plan.push(Step::manual(
            "platform-tools (adb)",
            "needed to install, launch and read logs from the emulator",
            vec!["sdkmanager platform-tools".to_string()],
        ));
    }

    // --- Rust targets ------------------------------------------------------
    let missing_targets: Vec<&str> = ANDROID_TARGETS
        .iter()
        .copied()
        .filter(|t| !tc.has_rust_target(t))
        .collect();
    if missing_targets.is_empty() {
        plan.push(Step::satisfied(
            "Rust targets",
            ANDROID_TARGETS.join(", "),
        ));
    } else {
        plan.push(Step::run(
            "Rust targets",
            Cmd::new("rustup")
                .arg("target")
                .arg("add")
                .args(missing_targets.iter().copied()),
        ));
    }

    // --- SDK packages ------------------------------------------------------
    // Batched into one sdkmanager call: each invocation re-reads the whole
    // remote repository index, which dominates the runtime for small installs.
    let mut want: Vec<(String, bool)> = vec![
        (
            "platform-tools".to_string(),
            tc.android_home.join("platform-tools").is_dir(),
        ),
        (
            format!("platforms;android-{api}"),
            tc.android_home
                .join("platforms")
                .join(format!("android-{api}"))
                .is_dir(),
        ),
        (
            format!("build-tools;{BUILD_TOOLS}"),
            tc.build_tools().is_dir(),
        ),
        ("emulator".to_string(), tc.emulator_bin().is_file()),
        (
            tc.system_image(abi, api),
            tc.system_image_dir(abi, api).is_dir(),
        ),
    ];
    if !opts.no_ndk {
        // The NDK is the one large download (~2.5 GB) and it is needed only to
        // LINK — `cargo check` never touches it. Separated so `--no-ndk` gives
        // a usable emulator setup without it.
        want.push((
            format!("ndk;{NDK_VERSION}"),
            tc.ndk_home().is_dir(),
        ));
    }

    let missing: Vec<String> = want
        .iter()
        .filter(|(_, present)| !present)
        .map(|(pkg, _)| pkg.clone())
        .collect();

    if missing.is_empty() {
        plan.push(Step::satisfied(
            "SDK packages",
            want.iter()
                .map(|(p, _)| p.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        ));
    } else {
        plan.push(Step::run(
            format!("SDK packages ({})", missing.join(", ")),
            Cmd::new(tc.sdkmanager().display().to_string())
                .args(missing.iter().cloned())
                .envs(env.clone())
                .stdin(licence_feed()),
        ));
        plan.push(Step::run(
            "Accept SDK licences",
            Cmd::new(tc.sdkmanager().display().to_string())
                .arg("--licenses")
                .envs(env.clone())
                .stdin(licence_feed()),
        ));
    }

    // --- AVD ---------------------------------------------------------------
    if tc.avd_exists(avd) {
        plan.push(Step::satisfied(
            format!("AVD '{avd}'"),
            format!("{}", tc.avd_home().join(format!("{avd}.ini")).display()),
        ));
    } else {
        // avdmanager asks "Do you wish to create a custom hardware profile?"
        // unconditionally and has no flag to suppress it; "no" takes the
        // device profile implied by the system image.
        let mut cmd = Cmd::new(tc.avdmanager().display().to_string())
            .arg("create")
            .arg("avd")
            .arg("-n")
            .arg(avd)
            .arg("-k")
            .arg(tc.system_image(abi, api))
            .envs(env.clone())
            .stdin("no\n");
        if let Some(dev) = &opts.device {
            cmd = cmd.arg("--device").arg(dev.clone());
        }
        if opts.force {
            cmd = cmd.arg("--force");
        }
        plan.push(Step::run(format!("AVD '{avd}'"), cmd));
    }

    // --- cargo-ndk (optional) ---------------------------------------------
    // scripts/build-android.sh links via the workspace .cargo/config.toml
    // rather than cargo-ndk, so this is a convenience for out-of-tree crates.
    if super::toolchain::which("cargo-ndk").is_some() {
        plan.push(Step::satisfied("cargo-ndk", "").optional());
    } else {
        plan.push(
            Step::run(
                "cargo-ndk",
                Cmd::new("cargo").arg("install").arg("cargo-ndk"),
            )
            .optional(),
        );
    }

    plan
}

pub fn ios_plan(tc: &Toolchain, _opts: &Opts) -> Plan {
    let mut plan = Plan::new("iOS toolchain");

    if tc.host_os != HostOs::MacOs {
        plan.push(Step::skipped(
            "iOS toolchain",
            "iOS tooling is macOS-only. `cargo check --target aarch64-apple-ios` still \
             works anywhere (it does not link), so the compile gate is portable; \
             building, signing and simulating are not.",
        ));
        return plan;
    }

    // --- Rust targets (the only part that works without Xcode) -------------
    let missing_targets: Vec<&str> = IOS_TARGETS
        .iter()
        .copied()
        .filter(|t| !tc.has_rust_target(t))
        .collect();
    if missing_targets.is_empty() {
        plan.push(Step::satisfied("Rust targets", IOS_TARGETS.join(", ")));
    } else {
        plan.push(Step::run(
            "Rust targets",
            Cmd::new("rustup")
                .arg("target")
                .arg("add")
                .args(missing_targets.iter().copied()),
        ));
    }

    // --- Xcode -------------------------------------------------------------
    if tc.has_xcode() {
        plan.push(Step::satisfied(
            "Xcode",
            format!(
                "{} (iphonesimulator SDK: {})",
                tc.xcode_developer_dir
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                tc.iphonesimulator_sdk
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
            ),
        ));
    } else {
        let selected = tc
            .xcode_developer_dir
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<nothing>".to_string());
        let app_installed = std::path::Path::new("/Applications/Xcode.app").is_dir();
        if app_installed {
            // Xcode is there, the active developer directory just points
            // elsewhere. That IS automatable, but it needs sudo, so we hand
            // over the exact command instead of asking for a password.
            plan.push(Step::manual(
                "Select Xcode",
                format!(
                    "/Applications/Xcode.app exists but xcode-select points at {selected}, \
                     which has no iOS sysroot"
                ),
                vec![
                    "sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer"
                        .to_string(),
                    "sudo xcodebuild -license accept".to_string(),
                ],
            ));
        } else {
            plan.push(Step::manual(
                "Xcode",
                format!(
                    "xcode-select points at {selected}. Command Line Tools give you a \
                     MacOSX.sdk and no iphonesimulator sysroot: iOS can be `cargo check`ed \
                     but not linked, bundled or simulated. Xcode is a ~13 GB App Store \
                     download tied to an Apple ID, so it cannot be installed unattended."
                ),
                vec![
                    "open 'macappstore://apps.apple.com/app/xcode/id497799835'".to_string(),
                    "# or, for a pinned version:".to_string(),
                    "brew install xcodes && xcodes install --latest".to_string(),
                    "sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer"
                        .to_string(),
                ],
            ));
        }
    }

    // --- Simulator runtime -------------------------------------------------
    // Since Xcode 14 the iOS runtime is a separate download from Xcode: a
    // fresh Xcode can have zero iOS runtimes and `simctl list devices` is
    // empty, which reads exactly like "no simulators available".
    if !tc.has_xcode() {
        plan.push(Step::skipped(
            "iOS simulator runtime",
            "needs Xcode first",
        ));
    } else {
        let runtimes = super::toolchain::capture("xcrun", &["simctl", "list", "runtimes"])
            .unwrap_or_default();
        if runtimes.contains("iOS") {
            plan.push(Step::satisfied(
                "iOS simulator runtime",
                runtimes
                    .lines()
                    .filter(|l| l.contains("iOS"))
                    .map(str::trim)
                    .collect::<Vec<_>>()
                    .join(" | "),
            ));
        } else {
            plan.push(Step::run(
                "iOS simulator runtime",
                Cmd::new("xcodebuild")
                    .arg("-downloadPlatform")
                    .arg("iOS"),
            ));
        }
    }

    // --- Optional device + automation helpers ------------------------------
    if super::toolchain::which("ios-deploy").is_some() {
        plan.push(Step::satisfied("ios-deploy", "").optional());
    } else if tc.brew_prefix.is_some() {
        plan.push(
            Step::run(
                "ios-deploy",
                Cmd::new("brew").arg("install").arg("ios-deploy"),
            )
            .optional(),
        );
    }

    // baguette links SimulatorKit/CoreSimulator directly, which is what buys
    // real gesture dispatch instead of synthesised taps — and is what
    // `mobile run ios` drives. It needs a recent Xcode.
    if super::toolchain::which("baguette").is_some() {
        plan.push(Step::satisfied("baguette (headless simulator driver)", "").optional());
    } else if let Some(major) = tc.xcode_major.filter(|m| *m < 26) {
        plan.push(
            Step::skipped(
                "baguette (headless simulator driver)",
                format!(
                    "needs Xcode 26+ for its SimulatorKit interfaces; this host has Xcode \
                     {major}. `mobile run ios` falls back to simctl, which can install, \
                     launch and screenshot but cannot inject real gestures."
                ),
            )
            .optional(),
        );
    } else if tc.brew_prefix.is_some() {
        plan.push(
            Step::run(
                "baguette (headless simulator driver)",
                Cmd::new("brew").arg("install").arg("baguette"),
            )
            .optional(),
        );
    }

    plan
}
