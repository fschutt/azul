//! `azul-doc mobile …` — install, inspect and drive the mobile toolchains.
//!
//! Everything the mobile guide tells you to type by hand, as a command that
//! probes first and is safe to re-run. Four things live here:
//!
//! * `install` / `doctor` / `env` — get the SDKs onto the machine and report
//!   what is missing, without ever installing something unannounced.
//! * `emulator` / `simulator` — boot a device headlessly, which is what makes
//!   any of this usable from CI.
//! * `build` / `run` — put a crate on that device and watch it start.
//! * `check` — the compile gate across every mobile target, which needs no SDK
//!   at all because `cargo check` does not link.

pub mod device;
pub mod e2e;
pub mod install;
pub mod plan;
pub mod run;
pub mod toolchain;

use std::path::{Path, PathBuf};

use device::Platform;
use toolchain::{HostOs, Toolchain};

/// Which transport carries an `--e2e` scenario to the app.
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum E2eDriver {
    /// Try the on-device runner; fall back to the host replay if this APK has
    /// no op dispatcher compiled in. The common case, and the only one that
    /// works without knowing how the APK was built.
    #[default]
    Auto,
    /// Require the on-device runner. Full op fidelity, or an error.
    Device,
    /// Force the host replay even if the device could have run it — this is
    /// what proves the real UIKit / GestureDetector input path.
    Host,
}

#[derive(Default)]
pub struct Opts {
    pub yes: bool,
    pub dry_run: bool,
    pub verbose: bool,
    pub force: bool,
    /// Show the emulator/simulator window instead of running headless.
    pub windowed: bool,
    pub no_ndk: bool,
    pub abi: Option<String>,
    pub api: Option<String>,
    pub avd: Option<String>,
    pub device: Option<String>,
    pub package: Option<String>,
    pub e2e: Option<PathBuf>,
    /// Which transport drives an `--e2e` scenario.
    pub driver: E2eDriver,
    pub boot_timeout: u64,
    /// How long to wait for the on-device runner to finish before concluding
    /// this APK has no op dispatcher.
    pub e2e_timeout: u64,
    /// How long to let the app draw before screenshotting it.
    pub settle_ms: u64,
    pub positional: Vec<String>,
}

impl Opts {
    fn parse(args: &[&str]) -> anyhow::Result<Self> {
        let mut o = Opts {
            boot_timeout: 300,
            e2e_timeout: 120,
            settle_ms: 4000,
            verbose: true,
            ..Default::default()
        };
        let mut i = 0;
        while i < args.len() {
            let mut take = |i: &mut usize, flag: &str| -> anyhow::Result<String> {
                let v = args
                    .get(*i + 1)
                    .ok_or_else(|| anyhow::anyhow!("{flag} needs a value"))?;
                *i += 2;
                Ok((*v).to_string())
            };
            match args[i] {
                "--yes" | "-y" => {
                    o.yes = true;
                    i += 1;
                }
                "--dry-run" | "-n" => {
                    o.dry_run = true;
                    i += 1;
                }
                "--quiet" | "-q" => {
                    o.verbose = false;
                    i += 1;
                }
                "--force" => {
                    o.force = true;
                    i += 1;
                }
                "--windowed" => {
                    o.windowed = true;
                    i += 1;
                }
                "--no-ndk" => {
                    o.no_ndk = true;
                    i += 1;
                }
                "--abi" | "--target" => o.abi = Some(take(&mut i, "--abi")?),
                "--api" => o.api = Some(take(&mut i, "--api")?),
                "--avd" => o.avd = Some(take(&mut i, "--avd")?),
                "--device" => o.device = Some(take(&mut i, "--device")?),
                "--package" | "--bundle-id" => o.package = Some(take(&mut i, "--package")?),
                "--e2e" => o.e2e = Some(PathBuf::from(take(&mut i, "--e2e")?)),
                "--driver" => {
                    o.driver = match take(&mut i, "--driver")?.as_str() {
                        "auto" => E2eDriver::Auto,
                        "device" => E2eDriver::Device,
                        "host" => E2eDriver::Host,
                        other => anyhow::bail!(
                            "--driver expects auto | device | host, got '{other}'"
                        ),
                    };
                }
                "--e2e-timeout" => {
                    o.e2e_timeout = take(&mut i, "--e2e-timeout")?
                        .parse()
                        .map_err(|_| anyhow::anyhow!("--e2e-timeout expects seconds"))?;
                }
                "--boot-timeout" => {
                    o.boot_timeout = take(&mut i, "--boot-timeout")?
                        .parse()
                        .map_err(|_| anyhow::anyhow!("--boot-timeout expects seconds"))?;
                }
                "--settle" => {
                    o.settle_ms = take(&mut i, "--settle")?
                        .parse()
                        .map_err(|_| anyhow::anyhow!("--settle expects milliseconds"))?;
                }
                other if other.starts_with('-') => {
                    anyhow::bail!("unknown flag for `mobile`: {other}")
                }
                other => {
                    o.positional.push(other.to_string());
                    i += 1;
                }
            }
        }
        Ok(o)
    }
}

pub fn handle_mobile_command(project_root: &Path, args: &[&str]) -> anyhow::Result<()> {
    let mut opts = Opts::parse(args)?;
    let verb = opts
        .positional
        .first()
        .cloned()
        .unwrap_or_default();
    let verb = verb.as_str();
    let rest: Vec<&str> = opts.positional[1.min(opts.positional.len())..]
        .iter()
        .map(String::as_str)
        .collect();

    let tc = Toolchain::probe();

    // `--e2e` is relative to where the user typed it, not to doc/ (which
    // main() chdirs into before dispatching).
    if let Some(p) = &opts.e2e {
        if !p.is_file() {
            let rooted = project_root.join(p);
            if rooted.is_file() {
                opts.e2e = Some(rooted);
            }
        }
    }

    match verb {
        "" | "help" => {
            print_help();
            Ok(())
        }
        "doctor" => {
            // The install plans ARE the report: a step that would run is a
            // step that is missing, so there is nothing to keep in sync.
            install::android_plan(&tc, &opts).print();
            install::ios_plan(&tc, &opts).print();
            print_env(&tc);
            Ok(())
        }
        "env" => {
            print_env(&tc);
            Ok(())
        }
        "install" => {
            let which = rest.first().copied().unwrap_or("all");
            let mut failed = Vec::new();
            let mut blocked = Vec::new();
            for platform in expand(which)? {
                let plan = match platform {
                    Platform::Android => install::android_plan(&tc, &opts),
                    Platform::Ios => install::ios_plan(&tc, &opts),
                };
                let outcome = plan.execute(opts.yes, opts.dry_run)?;
                failed.extend(outcome.failed);
                blocked.extend(outcome.blocked);
            }
            if !failed.is_empty() {
                anyhow::bail!("failed: {}", failed.join(", "));
            }
            if !blocked.is_empty() {
                println!(
                    "\n\x1b[33mStill needs you:\x1b[0m {}\n\
                     Everything else is installed; re-run this command after doing them.",
                    blocked.join(", ")
                );
                // Not an error: the automatable part genuinely succeeded.
            } else {
                println!("\n\x1b[32mToolchain ready.\x1b[0m");
                print_env(&tc);
            }
            Ok(())
        }
        "emulator" | "simulator" => {
            let platform = if verb == "emulator" {
                Platform::Android
            } else {
                Platform::Ios
            };
            let dev = run::acquire_device(&tc, platform, &opts)?;
            println!("\n\x1b[32mready:\x1b[0m {} ({})", dev.id, dev.driver.name());
            if platform == Platform::Android {
                println!(
                    "  adb -s {} logcat            # follow the log\n  \
                     adb -s {} exec-out screencap -p > shot.png",
                    dev.id, dev.id
                );
            }
            Ok(())
        }
        "check" => {
            // Delegates to the script so there is exactly one definition of
            // "the mobile compile gate", and CI and a laptop run the same one.
            toolchain::Cmd::new("bash")
                .arg(
                    project_root
                        .join("scripts")
                        .join("mobile-check-all.sh")
                        .display()
                        .to_string(),
                )
                .cwd(project_root)
                .run()
        }
        "build" | "run" => {
            let platform = rest
                .first()
                .and_then(|p| Platform::parse(p))
                .ok_or_else(|| {
                    anyhow::anyhow!("`mobile {verb}` needs a platform: android | ios")
                })?;
            let spec = rest.get(1).copied().ok_or_else(|| {
                anyhow::anyhow!(
                    "`mobile {verb} {}` needs a crate: a path to a Cargo.toml, a \
                     directory, or an examples/ name",
                    platform.name()
                )
            })?;
            let target = run::Target::resolve(project_root, spec, &opts)?;
            println!(
                "\x1b[1m{} {} ({})\x1b[0m",
                if verb == "build" { "building" } else { "running" },
                target.crate_name,
                target.bundle_id
            );

            let artifact = run::build(project_root, &tc, platform, &target, &opts)?;
            println!("\n\x1b[32mbuilt:\x1b[0m {}", artifact.display());
            if verb == "build" {
                return Ok(());
            }

            let dev = run::acquire_device(&tc, platform, &opts)?;
            let report = run::deploy_and_run(project_root, &dev, &target, &artifact, &opts)?;

            println!("\n\x1b[1m==> result\x1b[0m");
            println!(
                "  engine started: {}",
                if report.launched {
                    "\x1b[32myes\x1b[0m"
                } else {
                    "\x1b[31mno — nothing in the log came from the Rust side\x1b[0m"
                }
            );
            if let Some(shot) = &report.screenshot {
                println!("  screenshot:     {}", shot.display());
            }
            println!("  log:            {}", report.log_path.display());
            if let Some(v) = &report.device_verdict {
                v.print();
            }
            if let Some(e2e) = &report.e2e {
                e2e.print();
            }
            if !report.errors.is_empty() {
                println!("\n  \x1b[31merrors in the device log:\x1b[0m");
                for e in report.errors.iter().take(10) {
                    println!("    {e}");
                }
            }

            let e2e_bad = report.e2e.as_ref().map(|r| !r.complete()).unwrap_or(false)
                || report.device_verdict.as_ref().map(|v| !v.ok()).unwrap_or(false);
            if !report.launched || !report.errors.is_empty() || e2e_bad {
                anyhow::bail!("run did not come up clean — see above");
            }
            Ok(())
        }
        other => anyhow::bail!("unknown `mobile` subcommand: {other}\n\nTry `azul-doc mobile help`"),
    }
}

fn expand(which: &str) -> anyhow::Result<Vec<Platform>> {
    match which {
        "all" | "both" => Ok(vec![Platform::Android, Platform::Ios]),
        "android" => Ok(vec![Platform::Android]),
        "ios" => Ok(vec![Platform::Ios]),
        other => anyhow::bail!("unknown platform '{other}' (expected android | ios | all)"),
    }
}

/// Print the environment the Android tools need, in `eval`-able form.
fn print_env(tc: &Toolchain) {
    println!("\n\x1b[1m==> environment\x1b[0m");
    println!("  \x1b[90m# eval \"$(cargo run -q -p azul-doc -- mobile env)\"\x1b[0m");
    for (k, v) in tc.android_env() {
        if k == "PATH" {
            // Printing the whole inherited PATH back is noise; print the
            // prepend instead, which is what actually needs to happen.
            println!(
                "export PATH=\"{}:{}:$PATH\"",
                tc.build_tools().display(),
                tc.android_home.join("platform-tools").display()
            );
            continue;
        }
        println!("export {k}={}", toolchain::shell_quote(&v));
    }
    if tc.host_os == HostOs::MacOs && tc.has_xcode() {
        if let Some(dev) = &tc.xcode_developer_dir {
            println!("export DEVELOPER_DIR={}", toolchain::shell_quote(&dev.display().to_string()));
        }
    }
}

fn print_help() {
    println!("azul-doc mobile — install and drive the iOS / Android toolchains");
    println!();
    println!("  mobile doctor                  What is installed, what is missing, both platforms");
    println!("  mobile env                     Print the SDK environment (eval-able)");
    println!("  mobile check                   cargo check every mobile target (no SDK needed)");
    println!();
    println!("  mobile install [android|ios|all]");
    println!("                                 Show a plan, confirm, then install. Idempotent.");
    println!("                                 --yes unattended, --dry-run to preview only,");
    println!("                                 --no-ndk skips the ~2.5 GB linker download,");
    println!("                                 --abi/--api/--avd/--device to override defaults.");
    println!();
    println!("  mobile emulator                Boot the Android AVD headlessly and wait for it");
    println!("  mobile simulator               Boot an iOS simulator (baguette if installed)");
    println!("                                 --windowed shows the UI, --avd/--device pick one");
    println!();
    println!("  mobile build <android|ios> <crate>");
    println!("  mobile run   <android|ios> <crate> [--e2e <scenario.json>]");
    println!("                                 <crate> is a Cargo.toml, a directory, or a name");
    println!("                                 under examples/. `run` boots a device, installs,");
    println!("                                 launches, screenshots and reports the log.");
    println!("                                 --e2e runs a scenario. Two transports:");
    println!("                                   device — the engine's OWN runner, inside the");
    println!("                                            app. Full op vocabulary. Needs the");
    println!("                                            dispatcher in the APK, which --e2e");
    println!("                                            arranges for you.");
    println!("                                   host   — replays the INPUT ops through adb /");
    println!("                                            baguette, proving the real UIKit /");
    println!("                                            GestureDetector path. Names every op");
    println!("                                            it cannot express instead of passing");
    println!("                                            vacuously.");
    println!("                                 --driver auto|device|host (default auto: device");
    println!("                                 if the APK can, else host). --e2e-timeout SECS.");
    println!();
    println!("Examples:");
    println!("  azul-doc mobile install android --yes");
    println!("  azul-doc mobile run android examples/azul-writer/Cargo.toml");
    println!("  azul-doc mobile run ios AzWriter --e2e e2e/op-focus-blur.json");
}
