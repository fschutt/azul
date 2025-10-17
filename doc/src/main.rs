mod api;
mod api_helpers;
mod autofix;
mod autofix_messages;
mod autofix_v2;
mod build;
mod codegen;
mod deploy;
mod discover;
mod docgen;
mod license;
mod patch;
mod print_cmd;
mod reftest;
mod utils;

use std::{env, fs, path::PathBuf};

use anyhow::Context;
use deploy::Config;
use reftest::RunRefTestsConfig;

fn main() -> anyhow::Result<()> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let manifest_path = PathBuf::from(manifest_dir);
    let project_root = manifest_path.parent().unwrap().to_path_buf();
    let api_path = project_root.join("api.json");

    let _ = std::env::set_current_dir(manifest_dir);

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();

    // Load api.json once at startup (for read-only commands)
    let needs_api = args.len() > 1
        && matches!(
            args[1].as_str(),
            "print" | "autofix" | "memtest" | "reftest"
        );

    let api_data = if needs_api {
        let api_json_str = fs::read_to_string(&api_path)
            .with_context(|| format!("Failed to read api.json from {}", api_path.display()))?;
        Some(api::ApiData::from_str(&api_json_str).context("Failed to parse API definition")?)
    } else {
        None
    };

    // Check for "print" subcommand
    if args.len() > 1 && args[1] == "print" {
        // Handle print command with remaining args
        return print_cmd::handle_print_command(api_data.as_ref().unwrap(), &args[2..]);
    }

    // Check for "patch" subcommand
    if args.len() > 1 && args[1] == "patch" {
        if args.len() < 3 {
            eprintln!("❌ Usage: azul-docs patch <file_or_directory>");
            eprintln!("  Examples:");
            eprintln!("    azul-docs patch patches/fix_something.patch");
            eprintln!("    azul-docs patch patches/");
            std::process::exit(1);
        }

        println!("🔧 Applying patches to api.json...\n");

        // Load API data (need mutable copy for patching)
        let api_json_str = fs::read_to_string(&api_path)
            .with_context(|| format!("Failed to read api.json from {}", api_path.display()))?;
        let mut api_data =
            api::ApiData::from_str(&api_json_str).context("Failed to parse API definition")?;

        // Get project root (parent of doc/) for resolving paths
        let patch_path = PathBuf::from(&args[2]);
        let patch_path = if patch_path.is_absolute() {
            patch_path
        } else {
            // Try relative to project root first
            let project_relative = project_root.join(&patch_path);
            if project_relative.exists() {
                project_relative
            } else {
                // Fall back to current dir (doc/)
                patch_path
            }
        };

        // Check if it's a directory or file
        if patch_path.is_dir() {
            // Apply all patches from directory
            let stats = patch::apply_patches_from_directory(&mut api_data, &patch_path)?;

            stats.print_summary();

            if stats.successful > 0 {
                // Normalize class names where external path differs from API name
                match patch::normalize_class_names(&mut api_data) {
                    Ok(count) if count > 0 => {
                        println!("✅ Renamed {} classes to match external paths", count);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("⚠️  Warning: Failed to normalize class names: {}", e);
                    }
                }

                // Save updated api.json
                let api_json = serde_json::to_string_pretty(&api_data)?;
                fs::write(&api_path, api_json)?;
                println!("\n💾 Saved updated api.json");
            }

            if stats.failed > 0 {
                std::process::exit(1);
            }
        } else {
            // Apply single patch file
            let patch = patch::ApiPatch::from_file(&patch_path)
                .with_context(|| format!("Failed to load patch file: {}", patch_path.display()))?;

            match patch.apply(&mut api_data) {
                Ok(count) => {
                    println!("✅ Applied {} changes\n", count);

                    // Normalize class names where external path differs from API name
                    match patch::normalize_class_names(&mut api_data) {
                        Ok(count) if count > 0 => {
                            println!("✅ Renamed {} classes to match external paths\n", count);
                        }
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("⚠️  Warning: Failed to normalize class names: {}\n", e);
                        }
                    }

                    // Save updated api.json
                    let api_json = serde_json::to_string_pretty(&api_data)?;
                    fs::write(&api_path, api_json)?;
                    println!("💾 Saved updated api.json\n");
                }
                Err(e) => {
                    eprintln!("❌ Error applying patch: {}", e);
                    return Err(e);
                }
            }
        }

        return Ok(());
    }

    // Check for "normalize" subcommand
    if args.len() > 1 && args[1] == "normalize" {
        println!("🔄 Normalizing api.json...\n");

        // Load API data from file
        let api_json_str = fs::read_to_string(&api_path)
            .with_context(|| format!("Failed to read api.json from {}", api_path.display()))?;

        let api_data =
            api::ApiData::from_str(&api_json_str).context("Failed to parse API definition")?;

        println!("✅ JSON syntax is valid");
        println!("📝 Normalizing formatting...");

        // Save normalized api.json
        let api_json = serde_json::to_string_pretty(&api_data)?;
        fs::write(&api_path, api_json)?;

        println!("💾 Saved normalized api.json\n");

        return Ok(());
    }

    // Check for "autofix" subcommand
    if args.len() > 1 && args[1] == "autofix" {
        // Default to target/autofix if no output directory specified
        let output_dir = if args.len() >= 3 {
            PathBuf::from(&args[2])
        } else {
            project_root.join("target").join("autofix")
        };

        // Use pre-loaded API data
        let api_data = api_data.as_ref().unwrap();

        // Run new recursive autofix
        autofix_v2::autofix_api_recursive(&api_data, &project_root, &output_dir)?;

        return Ok(());
    }

    // Check for "reftest" subcommand
    if args.len() > 1 && args[1] == "reftest" {
        println!("üí† Running local reftests...");

        let output_dir = PathBuf::from("target").join("reftest");
        let config = RunRefTestsConfig {
            // The test files are in `doc/src/reftest/working`
            test_dir: PathBuf::from(manifest_dir).join("src/reftest/working"),
            output_dir: output_dir.clone(),
            output_filename: "index.html",
        };

        reftest::run_reftests(config)?;

        let report_path = output_dir.join("index.html");
        println!(
            "\n‚úÖ Reftest complete. Report generated at: {}",
            report_path.display()
        );

        if args.len() > 2 && args[2] == "open" {
            if open::that(report_path).is_ok() {
                println!("üåê Opened report in default browser.");
            } else {
                eprintln!("‚ùå Could not open browser. Please open the report manually.");
            }
        }

        return Ok(());
    }

    let mut run_reftest = false;
    // Check for "reftest" subcommand
    if args.len() > 1 && args[1] == "reftest" {
        println!("üí† Running local reftests...");

        let output_dir = PathBuf::from("target").join("reftest");
        let config = RunRefTestsConfig {
            // The test files are in `doc/src/reftest/working`
            test_dir: PathBuf::from(manifest_dir).join("src/reftest/working"),
            output_dir: output_dir.clone(),
            output_filename: "index.html",
        };

        run_reftest = true;
        reftest::run_reftests(config)?;

        let report_path = output_dir.join("index.html");
        println!(
            "\n‚úÖ Reftest complete. Report generated at: {}",
            report_path.display()
        );

        if args.len() > 2 && args[2] == "open" {
            if open::that(report_path).is_ok() {
                println!("üåê Opened report in default browser.");
            } else {
                eprintln!("‚ùå Could not open browser. Please open the report manually.");
            }
        }

        return Ok(());
    }

    // Check for "reftest-headless" subcommand for LLM debugging
    if args.len() > 1 && args[1] == "reftest-headless" {
        if args.len() < 3 {
            eprintln!("‚ùå Usage: azul-doc reftest-headless <test_name>");
            eprintln!("  Example: azul-doc reftest-headless grid-span-2");
            std::process::exit(1);
        }
        let test_name = &args[2];
        println!("ü§™ Running headless reftest for: {}", test_name);

        let output_dir = PathBuf::from("target").join("reftest_headless");
        let test_dir = PathBuf::from(manifest_dir).join("src/reftest/working");

        run_reftest = true;
        reftest::run_single_reftest_headless(test_name, &test_dir, &output_dir)?;

        println!("\n‚úÖ Headless reftest for '{}' complete.", test_name);
        println!("   Debug information has been printed to the console.");
        println!(
            "   Generated images can be found in: {}",
            output_dir.display()
        );

        return Ok(());
    }

    // Check for "memtest" subcommand
    if args.len() > 1 && args[1] == "memtest" {
        // Use pre-loaded API data
        let api_data = api_data.as_ref().unwrap();

        // Handle subcommands
        if args.len() > 2 {
            match args[2].as_str() {
                "reverse-copy" | "copy-back" | "backup" => {
                    println!("⚠️  reverse-copy is no longer needed - memtest is self-contained");
                    return Ok(());
                }
                "run" => {
                    println!("🧪 Generating memory layout test crate...\n");
                    codegen::memtest::generate_memtest_crate(&api_data, &project_root)
                        .map_err(|e| anyhow::anyhow!(e))?;
                    println!("\n🏃 Running memory layout tests...\n");
                    let memtest_dir = project_root.join("target").join("memtest");
                    let status = std::process::Command::new("cargo")
                        .arg("test")
                        .arg("--")
                        .arg("--nocapture")
                        .current_dir(&memtest_dir)
                        .status()?;

                    if !status.success() {
                        eprintln!("\n❌ Memory layout tests failed!");
                        std::process::exit(1);
                    } else {
                        println!("\n✅ All memory layout tests passed!");
                    }
                    return Ok(());
                }
                _ => {}
            }
        }

        // Default: just generate
        println!("🧪 Generating memory layout test crate...\n");
        codegen::memtest::generate_memtest_crate(&api_data, &project_root)
            .map_err(|e| anyhow::anyhow!(e))?;
        return Ok(());
    }

    if args.len() > 1 && args[1] == "deploy" {
        println!("Starting Azul Build and Deploy System...");

        // Load API data (need mutable copy for potential patching)
        let api_json_str = fs::read_to_string(&api_path)
            .with_context(|| format!("Failed to read api.json from {}", api_path.display()))?;
        let mut api_data =
            api::ApiData::from_str(&api_json_str).context("Failed to parse API definition")?;

        // Set up configuration
        let config = Config::from_args();

        // Apply patch if specified
        if config.apply_patch {
            let patch_path = PathBuf::from("../../patch.json");
            if patch_path.exists() {
                println!("\n🔧 Applying patch from patch.json...");
                match patch::ApiPatch::from_file(&patch_path) {
                    Ok(patch) => {
                        match patch.apply(&mut api_data) {
                            Ok(count) => {
                                println!("✅ Applied {} patches\n", count);

                                // Save updated api.json
                                let api_json = serde_json::to_string_pretty(&api_data)?;
                                fs::write("../../api.json", api_json)?;
                                println!("💾 Saved updated api.json\n");
                            }
                            Err(e) => {
                                eprintln!("❌ Error applying patches: {}", e);
                                return Err(e);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("❌ Error loading patch file: {}", e);
                        return Err(e);
                    }
                }
            } else {
                println!("⚠️  No patch.json file found, skipping patch application\n");
            }
        }

        // Print import paths if requested
        if config.print_imports {
            patch::print_import_paths(&api_data);
        }

        println!("working dir = {manifest_dir}");
        println!("CONFIG={}", config.print());

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
        for (path, html) in docgen::generate_docs(&api_data, &image_path, "https://azul.rs/images")?
        {
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
            let c_api_path = version_dir.join("azul.h");
            fs::write(&c_api_path, &c_api_code)?;
            println!("  - Generated C API header: {}", c_api_path.display());

            // Generate C++ API header
            let cpp_api_code = codegen::cpp_api::generate_cpp_api(&api_data, version);
            let cpp_api_path = version_dir.join("azul.hpp");
            fs::write(&cpp_api_path, &cpp_api_code)?;
            println!("  - Generated C++ API header: {}", cpp_api_path.display());

            // Generate Python bindings (PyO3 Rust code)
            let python_api_code = codegen::python_api::generate_python_api(&api_data, version);
            let python_api_path = version_dir.join("azul_python.rs");
            fs::write(&python_api_path, &python_api_code)?;
            println!(
                "  - Generated Python bindings: {}",
                python_api_path.display()
            );

            // Generate Rust DLL
            let rust_dll_code = codegen::rust_dll::generate_rust_dll(&api_data, version);
            let rust_dll_path = version_dir.join("azul_dll.rs");
            fs::write(&rust_dll_path, &rust_dll_code)?;
            println!("  - Generated Rust DLL code: {}", rust_dll_path.display());

            // Create Git repository for Rust bindings
            let lib_rs = codegen::rust_api::generate_rust_api(&api_data, version);
            let rust_api_path = output_dir
                .join(format!("azul-{version}"))
                .join("src")
                .join("lib.rs");
            deploy::create_git_repository(version, &output_dir, &lib_rs)?;
            println!("  - Generated Rust API: {}", rust_api_path.display());

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
            crate::build::build_all_configs(version, &version_dir, &config)?;
        }

        // Generate releases index
        let releases_index = deploy::generate_releases_index(&versions);
        fs::write(output_dir.join("releases.html"), &releases_index)?;

        // Copy static assets
        deploy::copy_static_assets(&output_dir)?;

        // Generate donation page
        println!("Generating donation page...");
        let funding_yaml_bytes = include_str!("../../.github/FUNDING.yml");
        match docgen::donate::generate_donation_page(funding_yaml_bytes) {
            Ok(donation_html) => {
                fs::write(output_dir.join("donate.html"), &donation_html)?;
                println!("  - Generated donation page");
            }
            Err(e) => {
                eprintln!("Warning: Failed to generate donation page: {}", e);
            }
        }

        // Open the result in browser if not in CI
        if config.open {
            if let Ok(_) = open::that(output_dir.join("index.html").to_string_lossy().to_string()) {
                println!("Opened releases page in browser");
            } else {
                println!("Failed to open browser, but files were created successfully");
            }
        }

        let _ = std::fs::write(output_dir.join("CNAME"), "azul.rs");

        println!("\n╔════════════════════════════════════════════════════════════════╗");
        println!("║         Build and Deployment Completed Successfully!          ║");
        println!("╚════════════════════════════════════════════════════════════════╝\n");

        println!("📂 Output Directory: {}", output_dir.display());
        println!("\n📋 Generated Files:");
        println!("   ├─ Documentation:");
        println!("   │  ├─ index.html");
        println!("   │  ├─ releases.html");
        println!("   │  ├─ donate.html");
        if run_reftest {
            println!("   │  └─ reftest.html");
        }
        println!("   │");
        println!("   └─ API Releases:");
        for version in &versions {
            let version_dir = releases_dir.join(version);
            println!("      ├─ {}/", version);
            println!("      │  ├─ azul.h          (C API header)");
            println!("      │  ├─ azul.hpp        (C++ API header)");
            println!("      │  ├─ azul_python.rs   (Python/PyO3 bindings)");
            println!("      │  ├─ azul_dll.rs     (Rust DLL code)");
            println!("      │  ├─ api.json        (API definition)");
            println!("      │  └─ azul-{}/      (Rust API crate)", version);
        }

        if config.open {
            println!("\n🌐 Opening in browser...");
        }

        println!("\n✅ All API bindings generated successfully!");
        println!("   C API:      {} versions", versions.len());
        println!("   C++ API:    {} versions", versions.len());
        println!("   Python API: {} versions", versions.len());
        println!("   Rust API:   {} versions", versions.len());
        println!("   Rust DLL:   {} versions", versions.len());
    }

    Ok(())
}
