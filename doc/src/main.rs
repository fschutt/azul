//! Main entry point for the documentation generation and management tool.
#![allow(unused)]
use std::{env, fs, path::PathBuf};

use anyhow::Context;
use dllgen::deploy::Config;
use reftest::RunRefTestsConfig;

pub mod api;
pub mod autofix;
pub mod codegen;
pub mod dllgen;
pub mod docgen;
pub mod patch;
pub mod print;
pub mod reftest;
pub mod utils;

fn main() -> anyhow::Result<()> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let manifest_path = PathBuf::from(manifest_dir);
    let project_root = manifest_path.parent().unwrap().to_path_buf();
    let api_path = project_root.join("api.json");

    let _ = std::env::set_current_dir(manifest_dir);

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    let args = args.iter().map(|s| s.as_str()).collect::<Vec<&str>>();

    match args[..] {
        [_, "print"] => {
            let api_json = load_api_json(&api_path)?;
            return print::handle_print_command(&api_json, &args[2..]);
        }
        [_, "normalize"] => {
            println!("🔄 Normalizing api.json...\n");
            let api_data = load_api_json(&api_path)?;
            let api_json = serde_json::to_string_pretty(&api_data)?;
            fs::write(&api_path, api_json)?;
            println!("💾 Saved normalized api.json\n");
            return Ok(());
        }
        [_, "autofix"] => {
            let output_dir = project_root.join("target").join("autofix");
            let api_data = load_api_json(&api_path)?;
            autofix::autofix_api_recursive(&api_data, &project_root, &output_dir)?;
            return Ok(());
        }
        [_, "autofix", "explain"] => {
            let patches_dir = project_root.join("target").join("autofix").join("patches");

            if !patches_dir.exists() {
                eprintln!("❌ No patches found. Run 'azul-doc autofix' first.");
                std::process::exit(1);
            }

            patch::explain_patches(&patches_dir)?;
            return Ok(());
        }
        [_, "patch", "safe", patch_dir] => {
            println!("🔧 Applying safe (path-only) patches to api.json...\n");

            // Load API data (need mutable copy for patching)
            let api_json_str = fs::read_to_string(&api_path)
                .with_context(|| format!("Failed to read api.json from {}", api_path.display()))?;
            let mut api_data =
                api::ApiData::from_str(&api_json_str).context("Failed to parse API definition")?;

            // Get project root (parent of doc/) for resolving paths
            let patch_path = PathBuf::from(&args[3]);
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

            if !patch_path.is_dir() {
                anyhow::bail!("Path must be a directory: {}", patch_path.display());
            }

            // Apply only path-only patches and delete them
            let stats = patch::apply_path_only_patches(&mut api_data, &patch_path)?;

            if stats.successful > 0 {
                // Normalize class names where external path differs from API name
                match patch::normalize_class_names(&mut api_data) {
                    Ok(count) if count > 0 => {
                        println!("\n✅ Renamed {} classes to match external paths", count);
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

            return Ok(());
        }
        [_, "patch", patch_file] => {
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
                let patch = patch::ApiPatch::from_file(&patch_path).with_context(|| {
                    format!("Failed to load patch file: {}", patch_path.display())
                })?;

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
        [_, "memtest", "run"] => {
            let api_data = load_api_json(&api_path)?;
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
        [_, "memtest"] => {
            let api_data = load_api_json(&api_path)?;
            println!("🧪 Generating memory layout test crate...\n");
            codegen::memtest::generate_memtest_crate(&api_data, &project_root)
                .map_err(|e| anyhow::anyhow!(e))?;
            return Ok(());
        }
        [_, "reftest", "headless", test_name] => {
            println!("Running headless reftest for: {}", test_name);

            let output_dir = PathBuf::from("target").join("reftest_headless");
            let test_dir = PathBuf::from(manifest_dir).join("src/reftest/working");

            reftest::run_single_reftest_headless(test_name, &test_dir, &output_dir)?;

            println!("\nHeadless reftest for '{}' complete.", test_name);
            println!("   Debug information has been printed to the console.");
            println!(
                "   Generated images can be found in: {}",
                output_dir.display()
            );

            return Ok(());
        }
        [_, "reftest"] => {
            println!("Running local reftests...");

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
                "\nReftest complete. Report generated at: {}",
                report_path.display()
            );

            if args.len() > 2 && args[2] == "open" {
                if open::that(report_path).is_ok() {
                    println!("Opened report in default browser.");
                } else {
                    eprintln!("Could not open browser. Please open the report manually.");
                }
            }

            return Ok(());
        }
        [_, "deploy"] => {
            println!("Starting Azul Build and Deploy System...");
            let api_data = load_api_json(&api_path)?;
            let config = Config::from_args();
        }
        _ => {
            print_cli_help()?;
            return Ok(());
        }
    }

    Ok(())
}

fn load_api_json(api_path: &PathBuf) -> anyhow::Result<api::ApiData> {
    let api_json_str = fs::read_to_string(&api_path)
        .with_context(|| format!("Failed to read api.json from {}", api_path.display()))?;
    let api_data =
        api::ApiData::from_str(&api_json_str).context("Failed to parse API definition")?;
    Ok(api_data)
}

fn print_cli_help() -> anyhow::Result<()> {
    println!("Azul Documentation and Deployment Tool");
    println!("Usage:");
    println!("  doc print [options]        - Print API information");
    println!("  doc normalize              - Normalize api.json");
    println!("  doc autofix                - Apply automatic fixes to API definitions");
    println!("  doc autofix explain        - Explain what generated patches will do");
    println!("  doc patch safe <dir>       - Apply and delete safe (path-only) patches");
    println!("  doc patch <patch_file>     - Apply patches to api.json");
    println!("  doc memtest [run]          - Generate and optionally run memory layout tests");
    println!("  doc reftest [open]         - Run reftests and optionally open report");
    println!("  doc deploy                 - Build and deploy the Azul library");
    Ok(())
}
