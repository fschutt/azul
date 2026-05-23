use std::{env, ffi::OsStr, fs, path::Path, process::Command};

use anyhow::{Context, Result};

use crate::dllgen::deploy::Config;

pub fn build_all_configs(version: &str, output_dir: &Path, cfg: &Config) -> Result<()> {
    let mut all_configs = Vec::new();

    if cfg.build_windows {
        all_configs.extend_from_slice(&[
            (
                "windows",
                vec!["--no-default-features", "--features", "build-dll"],
                "azul.dll",
                "azul.dll",
            ),
            (
                "windows",
                vec!["--no-default-features", "--features", "build-dll"],
                "azul.dll.lib",
                "azul.dll.lib",
            ),
            (
                "windows",
                vec!["--no-default-features", "--features", "build-dll"],
                "azul.lib",
                "azul.lib",
            ),
        ]);

        if cfg.build_python {
            all_configs.push((
                "windows",
                vec!["--no-default-features", "--features", "python-extension"],
                "libazul.dll",
                "azul.pyd",
            ));
        }
    }

    if cfg.build_linux {
        all_configs.extend_from_slice(&[
            (
                "linux",
                vec!["--no-default-features", "--features", "build-dll"],
                "libazul.so",
                "libazul.so",
            ),
            (
                "linux",
                vec!["--no-default-features", "--features", "build-dll"],
                "libazul.a",
                "libazul.linux.a",
            ),
        ]);

        if cfg.build_python {
            all_configs.push((
                "linux",
                vec!["--no-default-features", "--features", "python-extension"],
                "libazul.so",
                "azul.cpython.so",
            ));
        }
    }

    if cfg.build_macos {
        all_configs.extend_from_slice(&[
            (
                "macos",
                vec!["--no-default-features", "--features", "build-dll"],
                "libazul.dylib",
                "libazul.dylib",
            ),
            (
                "macos",
                vec!["--no-default-features", "--features", "build-dll"],
                "libazul.a",
                "libazul.macos.a",
            ),
        ]);

        if cfg.build_python {
            all_configs.push((
                "macos",
                vec!["--no-default-features", "--features", "python-extension"],
                "libazul.dylib",
                "azul.so",
            ));
        }
    }

    // EXCEPTIONAL, opt-in (`deploy --with-remill`): also build the ~130 MB
    // libazul with the remill x86->WASM transpiler statically linked (web
    // backend). This is a ~30 min C++ build and needs third_party/remill-install
    // populated first (`bash scripts/build_remill.sh`), so it is OFF by default
    // and kept out of the normal ~25 MB deploy. Emitted as `*.remill.*`.
    if cfg.build_remill {
        if cfg.build_windows {
            all_configs.push(("windows", vec!["--no-default-features", "--features", "build-dll,web-transpiler-static"], "azul.dll", "azul.remill.dll"));
        }
        if cfg.build_linux {
            all_configs.push(("linux", vec!["--no-default-features", "--features", "build-dll,web-transpiler-static"], "libazul.so", "libazul.remill.so"));
        }
        if cfg.build_macos {
            all_configs.push(("macos", vec!["--no-default-features", "--features", "build-dll,web-transpiler-static"], "libazul.dylib", "libazul.remill.dylib"));
        }
    }

    for (platform, env_vars, target_path, output_path) in all_configs.iter() {
        let file = build_dll(version, platform, env_vars, &target_path)?;
        std::fs::write(output_dir.join(output_path), file)?;
    }

    Ok(())
}

pub fn build_dll(
    version: &str,
    platform: &str,
    env: &[&str],
    target_path: &str,
) -> Result<Vec<u8>> {
    println!("Building azul.dll version {version} for platform {platform} - flags: {env:?}");

    // Create temporary directory for building

    let build_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../dll");
    let build_dir = Path::new(build_dir);

    assert!(Path::new(build_dir).join("Xargo.toml").exists());

    // Set platform-specific settings
    let target = match platform {
        "windows" => "x86_64-pc-windows-gnu",
        "linux" => "x86_64-unknown-linux-musl",
        "macos" => "x86_64-apple-darwin",
        _ => return Err(anyhow::anyhow!("Unsupported platform: {}", platform)),
    };

    let _ = Command::new("rustup")
        .arg("target")
        .arg("add")
        .arg(target)
        .output()
        .unwrap();

    // Build the binary
    let status = Command::new("cargo")
        .current_dir(build_dir)
        .args(&["build", "--release", "--target", target])
        .args(env.iter().map(|v| OsStr::new(v)))
        .status()
        .context("Failed to run cargo build")?;

    if !status.success() {
        return Err(anyhow::anyhow!("Build failed with status: {}", status));
    }

    // Copy the built binary to the output directory
    let source_path = build_dir
        .parent()
        .unwrap()
        .join("target")
        .join(target)
        .join("release")
        .join(target_path);

    println!("reading {}", source_path.display());
    let bytes = fs::read(&source_path)?;

    println!(
        "Successfully built {} binaries for version {}",
        platform, version
    );

    Ok(bytes)
}
