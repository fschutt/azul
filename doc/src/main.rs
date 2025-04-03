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

    fs::create_dir_all(&output_dir)?;
    fs::create_dir_all(&releases_dir)?;

    // Get all available versions
    let versions = api_data.get_sorted_versions();
    println!("Found versions: {:?}", versions);

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
        fs::write(output_dir.join("azul.h"), &c_api_code)?;
        println!("  - Generated C API header");

        // Generate C++ API header
        let cpp_api_code = codegen::cpp_api::generate_cpp_api(&api_data, version);
        fs::write(output_dir.join("azul.hpp"), &cpp_api_code)?;
        println!("  - Generated C++ API header");

        // Create Git repository for Rust bindings
        let lib_rs = codegen::rust_api::generate_rust_api(&api_data, version);
        deploy::create_git_repository(version, &output_dir, &lib_rs)?;

        // Export API.json
        let api_json = serde_json::to_string_pretty(api_data.get_version(version).unwrap())?;
        fs::write(output_dir.join("api.json"), api_json)?;
        println!("  - Exported API.json");

        // Build binaries for each platform
        println!("  Building binaries...");

        if config.build_windows {
            println!("  - Building Windows binaries");
            // Create DLL for Windows
            fs::write(
                output_dir.join("azul.dll"),
                format!("Windows DLL v{}", version),
            )?;
        }

        if config.build_python {
            println!("  - Building Python .pyd binaries");

            // Generate Python API
            let python_api_code = codegen::python_api::generate_python_api(&api_data, version);
            fs::write(output_dir.join("azul.py"), python_api_code)?;

            // Create Python extension for Windows  // TODO: compile python API
            fs::write(
                output_dir.join("azul.pyd"),
                format!("Windows Python Extension v{}", version),
            )?;
        }

        if config.build_linux {
            println!("  - Building Linux binaries");
            // Create shared library for Linux
            fs::write(
                output_dir.join("libazul.so"),
                format!("Linux Shared Library v{}", version),
            )?;
        }

        if config.build_macos {
            println!("  - Building macOS binaries");
            // Create dynamic library for macOS
            fs::write(
                output_dir.join("libazul.dylib"),
                format!("macOS Dynamic Library v{}", version),
            )?;
        }

        // Generate license files
        deploy::generate_license_files(version, &files_dir)?;

        // Create examples
        deploy::create_examples(version, &files_dir, &c_api_code, &cpp_api_code)?; // TODO: compile examples

        // Generate version-specific HTML
        let release_html = deploy::generate_release_html(version, "<p>Release notes go here</p>");
        fs::write(releases_dir.join(&format!("{version}.html")), release_html)?;
    }

    // Generate releases index
    let releases_index = deploy::generate_releases_index(&versions);
    fs::write(output_dir.join("releases.html"), &releases_index)?;

    // Copy static assets
    deploy::copy_static_assets(&output_dir)?;

    // Open the result in browser if not in CI
    if env::var("GITHUB_CI").is_err() {
        if let Ok(_) = open::that(
            output_dir
                .join("releases.html")
                .to_string_lossy()
                .to_string(),
        ) {
            println!("Opened releases page in browser");
        } else {
            println!("Failed to open browser, but files were created successfully");
        }
    }

    println!("Build and deployment preparation completed!");
    Ok(())
}
