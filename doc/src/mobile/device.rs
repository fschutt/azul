//! Booting and driving an emulator / simulator from the command line.
//!
//! The two platforms are deliberately given the same shape — boot, wait, list,
//! install, launch, log, screenshot, input — so `mobile run` can be written
//! once against `Device` instead of branching per platform at every step.

use std::{
    path::{Path, PathBuf},
    thread::sleep,
    time::{Duration, Instant},
};

use super::toolchain::{Cmd, Toolchain};

/// Which backend actually carries input to the device.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Driver {
    /// `adb shell input …` — always available with platform-tools.
    Adb,
    /// `baguette` — links SimulatorKit/CoreSimulator, so gestures go through
    /// UIKit's real recognisers rather than being synthesised.
    Baguette,
    /// `simctl` only: install/launch/screenshot, no input injection.
    Simctl,
}

impl Driver {
    pub fn name(self) -> &'static str {
        match self {
            Driver::Adb => "adb",
            Driver::Baguette => "baguette",
            Driver::Simctl => "simctl",
        }
    }

    /// Whether this driver can inject touch/keyboard events at all.
    pub fn can_inject(self) -> bool {
        !matches!(self, Driver::Simctl)
    }
}

pub struct Device {
    pub platform: Platform,
    /// `emulator-5554` on Android, a simulator UDID on iOS.
    pub id: String,
    pub driver: Driver,
    env: Vec<(String, String)>,
    adb: PathBuf,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Android,
    Ios,
}

impl Platform {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "android" => Some(Platform::Android),
            "ios" => Some(Platform::Ios),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Platform::Android => "android",
            Platform::Ios => "ios",
        }
    }
}

// ---------------------------------------------------------------------------
// Android
// ---------------------------------------------------------------------------

/// Devices `adb` currently reports as `device` (not `offline`/`unauthorized`).
pub fn adb_devices(tc: &Toolchain) -> Vec<String> {
    let env = tc.android_env();
    let out = super::toolchain::capture_env(&tc.adb(), &["devices"], &env).unwrap_or_default();
    out.lines()
        .skip(1)
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let id = it.next()?;
            let state = it.next()?;
            (state == "device").then(|| id.to_string())
        })
        .collect()
}

/// Boot an AVD headlessly and wait for it to finish booting.
///
/// `-no-window` still produces a framebuffer, so `screencap` and
/// `uiautomator dump` both work — headless costs nothing in observability.
/// The emulator binary is invoked by its absolute path from inside its own
/// directory because it locates its bundled libraries relative to `argv[0]`;
/// a symlink on `PATH` makes it fail to find them.
pub fn boot_emulator(
    tc: &Toolchain,
    avd: &str,
    headless: bool,
    timeout: Duration,
) -> anyhow::Result<String> {
    if let Some(existing) = adb_devices(tc).into_iter().next() {
        println!("  reusing running device {existing}");
        return Ok(existing);
    }
    let emulator = tc.emulator_bin();
    if !emulator.is_file() {
        anyhow::bail!(
            "no emulator at {} — run `azul-doc mobile install android`",
            emulator.display()
        );
    }
    if !tc.avd_exists(avd) {
        anyhow::bail!(
            "no AVD named '{avd}' — run `azul-doc mobile install android --avd {avd}`"
        );
    }

    let mut args: Vec<String> = vec![
        "-avd".into(),
        avd.into(),
        "-no-audio".into(),
        "-no-boot-anim".into(),
        // Never write back to the AVD snapshot: a test run should not be able
        // to leave the next one starting from a mutated device.
        "-no-snapshot".into(),
    ];
    if headless {
        args.push("-no-window".into());
        // SwiftShader is the reliable GLES path with no window server
        // attached; `-gpu host` needs a surface.
        args.push("-gpu".into());
        args.push("swiftshader_indirect".into());
    }

    println!(
        "  booting AVD '{avd}'{}",
        if headless { " (headless)" } else { "" }
    );
    let mut cmd = std::process::Command::new(&emulator);
    cmd.args(&args)
        .current_dir(emulator.parent().unwrap_or(Path::new(".")))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    for (k, v) in tc.android_env() {
        cmd.env(k, v);
    }
    let _child = cmd.spawn()?;

    let started = Instant::now();
    let mut serial = None;
    while started.elapsed() < timeout {
        if serial.is_none() {
            serial = adb_devices(tc).into_iter().next();
        }
        if let Some(id) = &serial {
            // `device` in `adb devices` only means adbd answered; the
            // framework can still be minutes from up. sys.boot_completed is
            // the property the platform itself waits on.
            let env = tc.android_env();
            let done = super::toolchain::capture_env(
                &tc.adb(),
                &["-s", id, "shell", "getprop", "sys.boot_completed"],
                &env,
            )
            .unwrap_or_default();
            if done.trim() == "1" {
                println!("  booted in {:.1}s ({id})", started.elapsed().as_secs_f32());
                return Ok(id.clone());
            }
        }
        sleep(Duration::from_millis(500));
    }
    anyhow::bail!("emulator did not finish booting within {timeout:?}")
}

// ---------------------------------------------------------------------------
// iOS
// ---------------------------------------------------------------------------

/// Boot an iOS simulator and return its UDID.
///
/// Prefers `baguette` when installed (it can boot headlessly, without
/// Simulator.app), and falls back to `simctl` + `open -a Simulator`.
pub fn boot_simulator(
    tc: &Toolchain,
    device: Option<&str>,
    headless: bool,
    timeout: Duration,
) -> anyhow::Result<(String, Driver)> {
    if !tc.has_xcode() {
        anyhow::bail!(
            "no full Xcode selected, so there is no iOS simulator on this host. \
             `xcode-select -p` = {}. Run `azul-doc mobile install ios` for the steps.",
            tc.xcode_developer_dir
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<nothing>".into())
        );
    }

    let have_baguette = super::toolchain::which("baguette").is_some();

    // An already-booted simulator is reused, exactly like the Android path.
    let listed = super::toolchain::capture("xcrun", &["simctl", "list", "devices", "booted"])
        .unwrap_or_default();
    if let Some(udid) = first_udid(&listed) {
        println!("  reusing booted simulator {udid}");
        return Ok((
            udid,
            if have_baguette {
                Driver::Baguette
            } else {
                Driver::Simctl
            },
        ));
    }

    let available =
        super::toolchain::capture("xcrun", &["simctl", "list", "devices", "available"])
            .unwrap_or_default();
    let udid = match device {
        Some(name) => available
            .lines()
            .find(|l| l.contains(name))
            .and_then(|l| udid_of(l))
            .ok_or_else(|| anyhow::anyhow!("no available simulator matching '{name}'"))?,
        None => available
            .lines()
            .filter(|l| l.contains("iPhone"))
            .find_map(udid_of)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "no available iPhone simulator. Since Xcode 14 the iOS runtime is a \
                     separate download: run `xcodebuild -downloadPlatform iOS`."
                )
            })?,
    };

    println!("  booting simulator {udid}");
    Cmd::new("xcrun")
        .arg("simctl")
        .arg("boot")
        .arg(&udid)
        .run()?;
    if !headless {
        let _ = Cmd::new("open").arg("-a").arg("Simulator").run();
    }
    let started = Instant::now();
    while started.elapsed() < timeout {
        let booted = super::toolchain::capture("xcrun", &["simctl", "list", "devices", "booted"])
            .unwrap_or_default();
        if booted.contains(&udid) {
            println!("  booted in {:.1}s", started.elapsed().as_secs_f32());
            return Ok((
                udid,
                if have_baguette {
                    Driver::Baguette
                } else {
                    Driver::Simctl
                },
            ));
        }
        sleep(Duration::from_millis(500));
    }
    anyhow::bail!("simulator did not boot within {timeout:?}")
}

fn first_udid(listing: &str) -> Option<String> {
    listing.lines().find_map(udid_of)
}

/// `simctl list` prints `    iPhone 15 (UDID) (Booted)`.
fn udid_of(line: &str) -> Option<String> {
    let open = line.find('(')?;
    let close = line[open + 1..].find(')')? + open + 1;
    let candidate = &line[open + 1..close];
    (candidate.len() == 36 && candidate.chars().all(|c| c.is_ascii_hexdigit() || c == '-'))
        .then(|| candidate.to_string())
}

// ---------------------------------------------------------------------------
// Uniform device operations
// ---------------------------------------------------------------------------

impl Device {
    pub fn android(tc: &Toolchain, id: String) -> Self {
        Self {
            platform: Platform::Android,
            id,
            driver: Driver::Adb,
            env: tc.android_env(),
            adb: tc.adb(),
        }
    }

    pub fn ios(id: String, driver: Driver) -> Self {
        Self {
            platform: Platform::Ios,
            id,
            driver,
            env: Vec::new(),
            adb: PathBuf::from("adb"),
        }
    }

    fn adb_cmd(&self, args: &[&str]) -> Cmd {
        Cmd::new(self.adb.display().to_string())
            .arg("-s")
            .arg(&self.id)
            .args(args.iter().copied())
            .envs(self.env.clone())
    }

    pub fn install(&self, artifact: &Path) -> anyhow::Result<()> {
        match self.platform {
            Platform::Android => self
                .adb_cmd(&["install", "-r", "-g"])
                .arg(artifact.display().to_string())
                .run(),
            Platform::Ios => Cmd::new("xcrun")
                .arg("simctl")
                .arg("install")
                .arg(&self.id)
                .arg(artifact.display().to_string())
                .run(),
        }
    }

    pub fn uninstall(&self, bundle_id: &str) -> anyhow::Result<()> {
        match self.platform {
            Platform::Android => self.adb_cmd(&["uninstall"]).arg(bundle_id).run(),
            Platform::Ios => Cmd::new("xcrun")
                .arg("simctl")
                .arg("uninstall")
                .arg(&self.id)
                .arg(bundle_id)
                .run(),
        }
    }

    /// Launch the app. `activity` is the Android component to start; on iOS
    /// only the bundle id is used.
    pub fn launch(
        &self,
        bundle_id: &str,
        activity: &str,
        extras: &[(String, String)],
    ) -> anyhow::Result<()> {
        match self.platform {
            Platform::Android => {
                let mut cmd = self
                    .adb_cmd(&["shell", "am", "start", "-W", "-n"])
                    .arg(format!("{bundle_id}/{activity}"));
                for (k, v) in extras {
                    cmd = cmd.arg("--es").arg(k.clone()).arg(v.clone());
                }
                cmd.run()
            }
            Platform::Ios => {
                let mut cmd = Cmd::new("xcrun")
                    .arg("simctl")
                    .arg("launch")
                    .arg(&self.id)
                    .arg(bundle_id);
                // simctl passes trailing args to the process as argv, and
                // SIMCTL_CHILD_* env vars reach it as environment.
                for (k, v) in extras {
                    cmd.env.push((format!("SIMCTL_CHILD_{k}"), v.clone()));
                }
                cmd.run()
            }
        }
    }

    pub fn stop(&self, bundle_id: &str) -> anyhow::Result<()> {
        match self.platform {
            Platform::Android => self.adb_cmd(&["shell", "am", "force-stop"]).arg(bundle_id).run(),
            Platform::Ios => Cmd::new("xcrun")
                .arg("simctl")
                .arg("terminate")
                .arg(&self.id)
                .arg(bundle_id)
                .run(),
        }
    }

    /// Copy a file onto the device, returning the on-device path.
    pub fn push(&self, local: &Path, remote_name: &str) -> anyhow::Result<String> {
        match self.platform {
            Platform::Android => {
                // /data/local/tmp is world-readable and survives app
                // reinstalls, which /sdcard increasingly does not without
                // scoped-storage grants.
                let remote = format!("/data/local/tmp/{remote_name}");
                self.adb_cmd(&["push"])
                    .arg(local.display().to_string())
                    .arg(&remote)
                    .output()?;
                self.adb_cmd(&["shell", "chmod", "644"]).arg(&remote).run()?;
                Ok(remote)
            }
            Platform::Ios => {
                // A simulator shares the host filesystem, so there is nothing
                // to copy — the app can open the host path directly.
                Ok(local.display().to_string())
            }
        }
    }

    pub fn clear_log(&self) -> anyhow::Result<()> {
        match self.platform {
            Platform::Android => self.adb_cmd(&["logcat", "-c"]).run(),
            Platform::Ios => Ok(()),
        }
    }

    /// Everything the app has logged since `clear_log`.
    pub fn read_log(&self, bundle_id: &str) -> anyhow::Result<String> {
        match self.platform {
            Platform::Android => {
                // `-d` dumps and exits. Unfiltered: an azul app logs through
                // several tags, and a native crash lands under DEBUG/libc,
                // which is exactly what we want to see when the launch fails.
                self.adb_cmd(&["logcat", "-d", "-v", "brief"]).output()
            }
            Platform::Ios => Cmd::new("xcrun")
                .arg("simctl")
                .arg("spawn")
                .arg(&self.id)
                .arg("log")
                .arg("show")
                .arg("--last")
                .arg("2m")
                .arg("--predicate")
                .arg(format!("subsystem CONTAINS '{bundle_id}'"))
                .output(),
        }
    }

    pub fn screenshot(&self, out: &Path) -> anyhow::Result<()> {
        if let Some(parent) = out.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match self.platform {
            Platform::Android => {
                // exec-out, not shell: `adb shell` mangles binary output by
                // translating LF, which corrupts every PNG past the first
                // newline byte.
                let png = std::process::Command::new(&self.adb)
                    .args(["-s", &self.id, "exec-out", "screencap", "-p"])
                    .envs(self.env.clone())
                    .output()?;
                if !png.status.success() {
                    anyhow::bail!("screencap failed: {}", String::from_utf8_lossy(&png.stderr));
                }
                std::fs::write(out, png.stdout)?;
                Ok(())
            }
            Platform::Ios => match self.driver {
                Driver::Baguette => Cmd::new("baguette")
                    .arg("screenshot")
                    .arg("--udid")
                    .arg(&self.id)
                    .arg("--output")
                    .arg(out.display().to_string())
                    .run(),
                _ => Cmd::new("xcrun")
                    .arg("simctl")
                    .arg("io")
                    .arg(&self.id)
                    .arg("screenshot")
                    .arg(out.display().to_string())
                    .run(),
            },
        }
    }

    /// Dump the platform accessibility tree — the only structural assertion
    /// target available from outside the process.
    pub fn describe_ui(&self) -> anyhow::Result<String> {
        match self.platform {
            Platform::Android => {
                self.adb_cmd(&["shell", "uiautomator", "dump", "/dev/tty"])
                    .output()
            }
            Platform::Ios => match self.driver {
                Driver::Baguette => Cmd::new("baguette")
                    .arg("describe-ui")
                    .arg("--udid")
                    .arg(&self.id)
                    .output(),
                _ => anyhow::bail!(
                    "describe-ui needs baguette; simctl cannot read the accessibility tree"
                ),
            },
        }
    }

    pub fn tap(&self, x: f32, y: f32, size: (f32, f32)) -> anyhow::Result<()> {
        match self.platform {
            Platform::Android => self
                .adb_cmd(&["shell", "input", "tap"])
                .arg(format!("{}", x.round() as i32))
                .arg(format!("{}", y.round() as i32))
                .run(),
            Platform::Ios => Cmd::new("baguette")
                .arg("tap")
                .arg("--udid")
                .arg(&self.id)
                .arg("--x")
                .arg(format!("{x}"))
                .arg("--y")
                .arg(format!("{y}"))
                .arg("--width")
                .arg(format!("{}", size.0))
                .arg("--height")
                .arg(format!("{}", size.1))
                .run(),
        }
    }

    pub fn swipe(
        &self,
        from: (f32, f32),
        to: (f32, f32),
        size: (f32, f32),
        ms: u32,
    ) -> anyhow::Result<()> {
        match self.platform {
            Platform::Android => self
                .adb_cmd(&["shell", "input", "swipe"])
                .arg(format!("{}", from.0.round() as i32))
                .arg(format!("{}", from.1.round() as i32))
                .arg(format!("{}", to.0.round() as i32))
                .arg(format!("{}", to.1.round() as i32))
                .arg(format!("{ms}"))
                .run(),
            Platform::Ios => Cmd::new("baguette")
                .arg("swipe")
                .arg("--udid")
                .arg(&self.id)
                .arg("--startX")
                .arg(format!("{}", from.0))
                .arg("--startY")
                .arg(format!("{}", from.1))
                .arg("--endX")
                .arg(format!("{}", to.0))
                .arg("--endY")
                .arg(format!("{}", to.1))
                .arg("--width")
                .arg(format!("{}", size.0))
                .arg("--height")
                .arg(format!("{}", size.1))
                .run(),
        }
    }

    /// A single motion sample. `adb shell input motionevent` needs API 30+;
    /// baguette has no equivalent primitive, so iOS reports it unsupported
    /// rather than approximating a drag with a tap.
    pub fn motion(&self, action: &str, x: f32, y: f32, _size: (f32, f32)) -> anyhow::Result<()> {
        match self.platform {
            Platform::Android => self
                .adb_cmd(&["shell", "input", "motionevent"])
                .arg(action)
                .arg(format!("{}", x.round() as i32))
                .arg(format!("{}", y.round() as i32))
                .run(),
            Platform::Ios => anyhow::bail!(
                "individual motion samples are not expressible through baguette; \
                 use a swipe or run the scenario in-process with `azul-doc e2e`"
            ),
        }
    }

    pub fn key(&self, code: &str) -> anyhow::Result<()> {
        match self.platform {
            Platform::Android => self.adb_cmd(&["shell", "input", "keyevent"]).arg(code).run(),
            Platform::Ios => Cmd::new("baguette")
                .arg("key")
                .arg("--udid")
                .arg(&self.id)
                .arg("--code")
                .arg(code)
                .run(),
        }
    }

    pub fn type_text(&self, text: &str) -> anyhow::Result<()> {
        match self.platform {
            Platform::Android => self.adb_cmd(&["shell", "input", "text"]).arg(text).run(),
            Platform::Ios => Cmd::new("baguette")
                .arg("type")
                .arg("--udid")
                .arg(&self.id)
                .arg("--text")
                .arg(text)
                .run(),
        }
    }

    /// Logical screen size in pixels — baguette needs it to scale coordinates.
    pub fn screen_size(&self) -> Option<(f32, f32)> {
        match self.platform {
            Platform::Android => {
                let out = self.adb_cmd(&["shell", "wm", "size"]).output().ok()?;
                let dims = out.rsplit(':').next()?.trim();
                let (w, h) = dims.split_once('x')?;
                Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
            }
            Platform::Ios => None,
        }
    }
}
