// src/binary_builder.rs

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result};

pub fn build_dll(version: &str, platform: &str, output_dir: &Path) -> Result<()> {
    println!("Building {} binaries for version {}", platform, version);

    // Create temporary directory for building

    let build_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../dll");
    let build_dir = Path::new(build_dir);

    assert!(Path::new(build_dir).join("Xargo.toml").exists());

    // Set platform-specific settings
    let (target, output_name) = match platform {
        "windows" => ("x86_64-pc-windows-msvc", "azul.dll"),
        "linux" => ("x86_64-unknown-linux-gnu", "libazul.so"),
        "macos" => ("x86_64-apple-darwin", "libazul.dylib"),
        _ => return Err(anyhow::anyhow!("Unsupported platform: {}", platform)),
    };

    // Build the binary
    let status = Command::new("cargo")
        .current_dir(build_dir)
        .args(&["build", "--release", "--target", target])
        .status()
        .context("Failed to run cargo build")?;

    if !status.success() {
        return Err(anyhow::anyhow!("Build failed with status: {}", status));
    }

    // Copy the built binary to the output directory
    let source_path = build_dir
        .join("target")
        .join(target)
        .join("release")
        .join(output_name);

    fs::copy(&source_path, output_dir.join(output_name))
        .context(format!("Failed to copy binary from {:?}", source_path))?;

    println!(
        "Successfully built {} binaries for version {}",
        platform, version
    );

    Ok(())
}
