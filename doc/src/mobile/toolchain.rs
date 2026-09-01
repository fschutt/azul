//! Where the mobile SDKs actually are, and what is missing from them.
//!
//! Everything else in this module asks `Toolchain` rather than shelling out to
//! `command -v` on its own, so a single probe decides what `install`, `doctor`,
//! `emulator` and `run` all believe about the host.
//!
//! The paths here are deliberately *discovered*, not hard-coded: the Homebrew
//! prefix differs between Apple silicon (`/opt/homebrew`) and Intel
//! (`/usr/local`), and an Android SDK installed by Android Studio lives
//! somewhere else again. `scripts/build-android.sh` and
//! `scripts/mobile-check-all.sh` both hard-code `/opt/homebrew/...` as a
//! *default* — this agrees with them when that path exists and keeps working
//! when it does not.

use std::{
    io::Write as _,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// Android API level the whole toolchain pins to. Matches
/// `scripts/build-android.sh` (`platforms/android-34`) and the manifest
/// template's `targetSdkVersion`.
pub const ANDROID_API: &str = "34";
/// Build-tools revision providing `aapt2`, `zipalign`, `apksigner`, `d8`.
pub const BUILD_TOOLS: &str = "34.0.0";
/// NDK revision the workspace `.cargo/config.toml` points its linkers at.
pub const NDK_VERSION: &str = "27.0.12077973";
/// System-image tag. `google_apis` (not `default`) because it ships the Play
/// services stubs a real app expects, and `google_apis_playstore` images are
/// not rootable, which blocks `adb root` for log/file access during tests.
pub const IMAGE_TAG: &str = "google_apis";
/// Default AVD name created by `mobile install android`.
pub const AVD_NAME: &str = "azul";
/// JDK the Android command-line tools run under.
pub const JDK_FORMULA: &str = "openjdk@17";

/// A resolved view of the host's mobile toolchains.
pub struct Toolchain {
    pub host_os: HostOs,
    /// `arm64-v8a` on Apple silicon, `x86_64` elsewhere. A system image whose
    /// ABI differs from the host runs under full CPU emulation and is unusably
    /// slow, so this is not a cosmetic choice.
    pub host_abi: &'static str,
    pub brew_prefix: Option<PathBuf>,
    pub java_home: Option<PathBuf>,
    pub android_home: PathBuf,
    /// True when `android_home` exists on disk (it is a *guess* otherwise).
    pub android_home_exists: bool,
    pub rust_targets: Vec<String>,
    /// `xcode-select -p`, whatever it points at.
    pub xcode_developer_dir: Option<PathBuf>,
    /// Set only when a *full* Xcode is selected — Command Line Tools alone
    /// give a `MacOSX.sdk` and no iOS sysroot.
    pub iphonesimulator_sdk: Option<PathBuf>,
    /// Major version from `xcodebuild -version`, when Xcode is present.
    pub xcode_major: Option<u32>,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum HostOs {
    MacOs,
    Linux,
    Other,
}

impl Toolchain {
    pub fn probe() -> Self {
        let host_os = if cfg!(target_os = "macos") {
            HostOs::MacOs
        } else if cfg!(target_os = "linux") {
            HostOs::Linux
        } else {
            HostOs::Other
        };
        let host_abi = if cfg!(target_arch = "aarch64") {
            "arm64-v8a"
        } else {
            "x86_64"
        };
        let brew_prefix = capture("brew", &["--prefix"])
            .map(|s| PathBuf::from(s.trim()))
            .filter(|p| p.is_dir());

        let java_home = resolve_java_home(brew_prefix.as_deref());
        let android_home = resolve_android_home(brew_prefix.as_deref());
        let android_home_exists = android_home.is_dir();

        let rust_targets = capture("rustup", &["target", "list", "--installed"])
            .map(|s| s.lines().map(|l| l.trim().to_string()).collect())
            .unwrap_or_default();

        // `xcode-select -p` answers even when it points at Command Line Tools,
        // so its success proves nothing on its own — the SDK probe below is
        // what separates "Xcode installed" from "CLT only".
        let xcode_developer_dir = if host_os == HostOs::MacOs {
            capture("xcode-select", &["-p"]).map(|s| PathBuf::from(s.trim()))
        } else {
            None
        };
        let iphonesimulator_sdk = if host_os == HostOs::MacOs {
            capture("xcrun", &["--sdk", "iphonesimulator", "--show-sdk-path"])
                .map(|s| PathBuf::from(s.trim()))
                .filter(|p| p.is_dir())
        } else {
            None
        };
        let xcode_major = capture("xcodebuild", &["-version"]).and_then(|s| {
            s.lines()
                .next()?
                .trim()
                .strip_prefix("Xcode ")?
                .split('.')
                .next()?
                .parse()
                .ok()
        });

        Self {
            host_os,
            host_abi,
            brew_prefix,
            java_home,
            android_home,
            android_home_exists,
            rust_targets,
            xcode_developer_dir,
            iphonesimulator_sdk,
            xcode_major,
        }
    }

    /// True when a full Xcode (not just Command Line Tools) is selected. This
    /// is the gate for *linking* iOS binaries, booting a simulator and running
    /// `simctl` — `cargo check` needs none of it.
    pub fn has_xcode(&self) -> bool {
        self.iphonesimulator_sdk.is_some()
            && self
                .xcode_developer_dir
                .as_ref()
                .map(|p| !p.ends_with("CommandLineTools"))
                .unwrap_or(false)
    }

    pub fn ndk_home(&self) -> PathBuf {
        std::env::var_os("ANDROID_NDK_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| self.android_home.join("ndk").join(NDK_VERSION))
    }

    pub fn build_tools(&self) -> PathBuf {
        self.android_home.join("build-tools").join(BUILD_TOOLS)
    }

    pub fn emulator_bin(&self) -> PathBuf {
        self.android_home.join("emulator").join("emulator")
    }

    /// `adb` from the SDK when present, else whatever is on `PATH`.
    pub fn adb(&self) -> PathBuf {
        let sdk = self.android_home.join("platform-tools").join("adb");
        if sdk.is_file() {
            sdk
        } else {
            PathBuf::from("adb")
        }
    }

    pub fn sdkmanager(&self) -> PathBuf {
        let sdk = self
            .android_home
            .join("cmdline-tools")
            .join("latest")
            .join("bin")
            .join("sdkmanager");
        if sdk.is_file() {
            sdk
        } else {
            PathBuf::from("sdkmanager")
        }
    }

    pub fn avdmanager(&self) -> PathBuf {
        let sdk = self
            .android_home
            .join("cmdline-tools")
            .join("latest")
            .join("bin")
            .join("avdmanager");
        if sdk.is_file() {
            sdk
        } else {
            PathBuf::from("avdmanager")
        }
    }

    pub fn system_image(&self, abi: &str, api: &str) -> String {
        format!("system-images;android-{api};{IMAGE_TAG};{abi}")
    }

    pub fn system_image_dir(&self, abi: &str, api: &str) -> PathBuf {
        self.android_home
            .join("system-images")
            .join(format!("android-{api}"))
            .join(IMAGE_TAG)
            .join(abi)
    }

    /// Where `avdmanager` writes AVD definitions. Honours `ANDROID_AVD_HOME`
    /// because the emulator does.
    pub fn avd_home(&self) -> PathBuf {
        std::env::var_os("ANDROID_AVD_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().join(".android").join("avd"))
    }

    pub fn avd_exists(&self, name: &str) -> bool {
        self.avd_home().join(format!("{name}.ini")).is_file()
    }

    pub fn has_rust_target(&self, triple: &str) -> bool {
        self.rust_targets.iter().any(|t| t == triple)
    }

    /// The environment every Android child process needs. Returned rather than
    /// exported globally so `mobile env` can print exactly what we use.
    pub fn android_env(&self) -> Vec<(String, String)> {
        let mut env = vec![
            (
                "ANDROID_HOME".to_string(),
                self.android_home.display().to_string(),
            ),
            (
                "ANDROID_SDK_ROOT".to_string(),
                self.android_home.display().to_string(),
            ),
            (
                "ANDROID_NDK_HOME".to_string(),
                self.ndk_home().display().to_string(),
            ),
        ];
        if let Some(jh) = &self.java_home {
            env.push(("JAVA_HOME".to_string(), jh.display().to_string()));
        }
        // Prepend the SDK bins so `aapt2`/`zipalign`/`keytool` resolve without
        // the caller having arranged a PATH first.
        let mut prefix: Vec<String> = vec![
            self.build_tools().display().to_string(),
            self.android_home
                .join("platform-tools")
                .display()
                .to_string(),
        ];
        if let Some(jh) = &self.java_home {
            prefix.push(jh.join("bin").display().to_string());
        }
        let existing = std::env::var("PATH").unwrap_or_default();
        prefix.push(existing);
        env.push(("PATH".to_string(), prefix.join(":")));
        env
    }
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// A JDK home must contain `bin/java` **and** `lib/` — Homebrew's
/// `opt/openjdk@17` has the first and not the second, which is why the guide's
/// old `export JAVA_HOME=/opt/homebrew/opt/openjdk@17` limps along for
/// `sdkmanager` (it only runs `$JAVA_HOME/bin/java`) and breaks anything that
/// looks for the real home. Prefer the `libexec/openjdk.jdk/Contents/Home`
/// bundle path, fall back to the shim, then ask macOS.
fn resolve_java_home(brew_prefix: Option<&Path>) -> Option<PathBuf> {
    if let Some(jh) = std::env::var_os("JAVA_HOME").map(PathBuf::from) {
        if jh.join("bin").join("java").is_file() {
            return Some(jh);
        }
    }
    if let Some(prefix) = brew_prefix {
        let opt = prefix.join("opt").join(JDK_FORMULA);
        let bundled = opt.join("libexec").join("openjdk.jdk").join("Contents").join("Home");
        if bundled.join("bin").join("java").is_file() {
            return Some(bundled);
        }
        if opt.join("bin").join("java").is_file() {
            return Some(opt);
        }
    }
    capture("/usr/libexec/java_home", &["-v", "17"])
        .map(|s| PathBuf::from(s.trim()))
        .filter(|p| p.join("bin").join("java").is_file())
}

fn resolve_android_home(brew_prefix: Option<&Path>) -> PathBuf {
    for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(v) = std::env::var_os(var) {
            let p = PathBuf::from(v);
            if p.is_dir() {
                return p;
            }
        }
    }
    let mut candidates = Vec::new();
    if let Some(prefix) = brew_prefix {
        candidates.push(prefix.join("share").join("android-commandlinetools"));
    }
    // Android Studio defaults, in case the SDK came from there.
    candidates.push(home_dir().join("Library").join("Android").join("sdk"));
    candidates.push(home_dir().join("Android").join("Sdk"));
    for c in &candidates {
        if c.is_dir() {
            return c.clone();
        }
    }
    // Nothing exists yet: return the Homebrew path we would install into, so
    // the install plan has somewhere to point.
    candidates
        .into_iter()
        .next()
        .unwrap_or_else(|| PathBuf::from("/opt/homebrew/share/android-commandlinetools"))
}

/// Run a command and return trimmed stdout, or `None` if it is missing or fails.
pub fn capture(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Same, but with an explicit environment (Android tools need `JAVA_HOME`).
pub fn capture_env(program: &Path, args: &[&str], env: &[(String, String)]) -> Option<String> {
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null()).stderr(Stdio::null());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn on_path(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
        || which(program).is_some()
}

pub fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(program))
        .find(|p| p.is_file())
}

/// One runnable command, with everything a child needs to be reproducible.
#[derive(Clone)]
pub struct Cmd {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    /// Fed to the child's stdin. `sdkmanager` asks to accept each licence and
    /// `avdmanager` asks whether you want a custom hardware profile; both are
    /// unconditional prompts with no `--yes`, so the only way to run them
    /// unattended is to answer.
    pub stdin: Option<String>,
    pub cwd: Option<PathBuf>,
}

impl Cmd {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
            stdin: None,
            cwd: None,
        }
    }

    pub fn arg(mut self, a: impl Into<String>) -> Self {
        self.args.push(a.into());
        self
    }

    pub fn args<I, S>(mut self, it: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(it.into_iter().map(Into::into));
        self
    }

    pub fn envs(mut self, env: Vec<(String, String)>) -> Self {
        self.env = env;
        self
    }

    pub fn stdin(mut self, s: impl Into<String>) -> Self {
        self.stdin = Some(s.into());
        self
    }

    pub fn cwd(mut self, p: impl Into<PathBuf>) -> Self {
        self.cwd = Some(p.into());
        self
    }

    /// A copy-pasteable rendering, used by `--dry-run` and the plan preview.
    /// `PATH` is elided because printing the inherited one is pure noise.
    pub fn display(&self) -> String {
        let mut s = String::new();
        for (k, v) in &self.env {
            if k == "PATH" {
                continue;
            }
            s.push_str(&format!("{k}={} ", shell_quote(v)));
        }
        s.push_str(&shell_quote(&self.program));
        for a in &self.args {
            s.push(' ');
            s.push_str(&shell_quote(a));
        }
        if let Some(feed) = &self.stdin {
            s = format!("printf {} | {s}", shell_quote(feed));
        }
        s
    }

    /// Run it, streaming child output straight through to our own stdio.
    pub fn run(&self) -> anyhow::Result<()> {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args);
        for (k, v) in &self.env {
            cmd.env(k, v);
        }
        if let Some(dir) = &self.cwd {
            cmd.current_dir(dir);
        }
        cmd.stdin(if self.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
        let mut child = cmd
            .spawn()
            .map_err(|e| anyhow::anyhow!("cannot start `{}`: {e}", self.program))?;
        if let Some(feed) = &self.stdin {
            if let Some(mut sink) = child.stdin.take() {
                let _ = sink.write_all(feed.as_bytes());
            }
        }
        let status = child.wait()?;
        if !status.success() {
            anyhow::bail!("`{}` exited with {status}", self.program);
        }
        Ok(())
    }

    /// Run it and capture stdout instead of streaming it.
    pub fn output(&self) -> anyhow::Result<String> {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args);
        for (k, v) in &self.env {
            cmd.env(k, v);
        }
        if let Some(dir) = &self.cwd {
            cmd.current_dir(dir);
        }
        cmd.stdin(Stdio::null());
        let out = cmd
            .output()
            .map_err(|e| anyhow::anyhow!("cannot start `{}`: {e}", self.program))?;
        if !out.status.success() {
            anyhow::bail!(
                "`{}` exited with {}: {}",
                self.program,
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

pub fn shell_quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-/=:,@+".contains(c))
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}
