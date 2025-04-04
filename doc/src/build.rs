// src/binary_builder.rs

use std::{env, ffi::OsStr, fs, path::Path, process::Command};

use anyhow::{Context, Result};

pub fn build_all_configs(version: &str, output_dir: &Path) -> Result<()> {
    let all_configs = &[
        /*
        ("windows", vec!["--no-default-features", "--features", "desktop-cdylib"], "azul_dll.dll", "azul.dll"),
        ("windows", vec!["--no-default-features", "--features", "python-extension"], "libazul_dll.dll", "windows.pyd"),
        ("windows", vec!["--no-default-features", "--features", "desktop-staticlib"], "azul_dll.lib", "azul.lib"),
        */

        /*
        (
            "linux",
            vec!["--no-default-features", "--features", "desktop-cdylib"],
            "libazul_dll.so",
            "libazul.so",
        ),
        (
            "linux",
            vec!["--no-default-features", "--features", "python-extension"],
            "libazul_dll.so",
            "linux.pyd",
        ),
        (
            "linux",
            vec!["--no-default-features", "--features", "desktop-staticlib"],
            "libazul_dll.a",
            "libazul.linux.a",
        ),
        */
        (
            "macos",
            vec!["--no-default-features", "--features", "desktop-cdylib"],
            "libazul_dll.dylib",
            "libazul.dylib",
        ),
        (
            "macos",
            vec!["--no-default-features", "--features", "python-extension"],
            "libazul_dll.dylib",
            "macos.pyd",
        ),
        (
            "macos",
            vec!["--no-default-features", "--features", "desktop-staticlib"],
            "libazul_dll.a",
            "libazul.macos.a",
        ),
    ];

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
