mod api;
mod build;
mod codegen;
mod deploy;
mod docgen;
mod license;
mod utils;

use std::{env, fs, path::PathBuf};

use anyhow::Context;
use deploy::Config;

fn main() -> anyhow::Result<()> {
    println!("Starting Azul Build and Deploy System...");

    // Parse the API definition
    let api_data = api::ApiData::from_str(include_str!("../../api.json"))
        .context("Failed to parse API definition")?;

    // Set up configuration
    let config = Config::from_env();

    // Create output directory structure
    let output_dir = PathBuf::from("target").join("deploy");
    let releases_dir = output_dir.join("release");
    let image_path = output_dir.join("images");

    fs::create_dir_all(&output_dir)?;
    fs::create_dir_all(&releases_dir)?;
    fs::create_dir_all(&image_path)?;

    // Get all available versions
    let versions = api_data.get_sorted_versions();
    println!("Found versions: {:?}", versions);

    // Generate API + guide information for all versions
    for (path, html) in docgen::generate_docs(&api_data, &image_path, "https://azul.rs/images")? {
        let path_real = output_dir.join(path);
        if let Some(parent) = path_real.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(&path_real, &html)?;
    }

    // Process each version
    for version in &versions {
        println!("Processing version: {}", version);

        // Create version directory structure
        let version_dir = releases_dir.join(version);
        let files_dir = version_dir.clone();
        fs::create_dir_all(&version_dir)?;
        fs::create_dir_all(&files_dir)?;

        // Generate API bindings
        println!("  Generating API bindings...");

        // Generate C API header
        let c_api_code = codegen::c_api::generate_c_api(&api_data, version);
        fs::write(version_dir.join("azul.h"), &c_api_code)?;
        println!("  - Generated C API header");

        // Generate C++ API header
        let cpp_api_code = codegen::cpp_api::generate_cpp_api(&api_data, version);
        fs::write(version_dir.join("azul.hpp"), &cpp_api_code)?;
        println!("  - Generated C++ API header");

        // Create Git repository for Rust bindings
        let lib_rs = codegen::rust_api::generate_rust_api(&api_data, version);
        deploy::create_git_repository(version, &output_dir, &lib_rs)?;

        // Export API.json
        let api_json = serde_json::to_string_pretty(api_data.get_version(version).unwrap())?;
        fs::write(version_dir.join("api.json"), api_json)?;
        println!("  - Exported API.json");

        // Generate license files
        deploy::generate_license_files(version, &files_dir)?;

        // Create examples
        deploy::create_examples(version, &files_dir, &c_api_code, &cpp_api_code)?; // TODO: compile examples

        // Generate version-specific HTML
        let release_html = deploy::generate_release_html(version, &api_data);
        fs::write(releases_dir.join(&format!("{version}.html")), release_html)?;

        // Build binaries for each platform
        println!("  Building binaries...");
        crate::build::build_all_configs(version, &version_dir)?;
    }

    // Generate releases index
    let releases_index = deploy::generate_releases_index(&versions);
    fs::write(output_dir.join("releases.html"), &releases_index)?;

    // Copy static assets
    deploy::copy_static_assets(&output_dir)?;

    // Open the result in browser if not in CI
    if env::var("GITHUB_CI").is_err() {
        if let Ok(_) = open::that(output_dir.join("index.html").to_string_lossy().to_string()) {
            println!("Opened releases page in browser");
        } else {
            println!("Failed to open browser, but files were created successfully");
        }
    }

    println!("Build and deployment preparation completed!");
    Ok(())
}
