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

    match &args[1..] {
        ["print"] | ["print", ..] => {
            let api_json = load_api_json(&api_path)?;
            return print::handle_print_command(&api_json, &args[2..]);
        }
        ["normalize"] => {
            println!("🔄 Normalizing api.json...\n");
            let api_data = load_api_json(&api_path)?;
            let api_json = serde_json::to_string_pretty(&api_data)?;
            fs::write(&api_path, api_json)?;
            println!("💾 Saved normalized api.json\n");
            return Ok(());
        }
        ["autofix"] => {
            let output_dir = project_root.join("target").join("autofix");
            let api_data = load_api_json(&api_path)?;
            autofix::autofix_api_recursive(&api_data, &project_root, &output_dir)?;
            return Ok(());
        }
        ["autofix", "explain"] => {
            let patches_dir = project_root.join("target").join("autofix").join("patches");

            if !patches_dir.exists() {
                eprintln!("❌ No patches found. Run 'azul-doc autofix' first.");
                std::process::exit(1);
            }

            patch::explain_patches(&patches_dir)?;
            return Ok(());
        }
        ["patch", "safe", patch_dir] => {
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
        ["patch", patch_file] => {
            println!("🔧 Applying patches to api.json...\n");

            // Load API data (need mutable copy for patching)
            let api_json_str = fs::read_to_string(&api_path)
                .with_context(|| format!("Failed to read api.json from {}", api_path.display()))?;
            let mut api_data =
                api::ApiData::from_str(&api_json_str).context("Failed to parse API definition")?;

            // Get project root (parent of doc/) for resolving paths
            let patch_path = PathBuf::from(patch_file);
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

                if stats.successful > 0 || stats.total_changes > 0 {
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

                if stats.has_errors() {
                    std::process::exit(1);
                }
            } else {
                // Apply single patch file
                let patch = patch::ApiPatch::from_file(&patch_path).with_context(|| {
                    format!("Failed to load patch file: {}", patch_path.display())
                })?;

                match patch.apply(&mut api_data) {
                    Ok((count, errors)) => {
                        if errors.is_empty() {
                            println!("✅ Applied {} changes\n", count);
                        } else {
                            println!("⚠️  Applied {} changes with {} errors\n", count, errors.len());
                        }

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

                        if !errors.is_empty() {
                            println!("\n❌ Patch errors:");
                            for error in &errors {
                                println!("  • {}", error);
                            }
                            std::process::exit(1);
                        }
                    }
                    Err(e) => {
                        eprintln!("❌ Error applying patch: {}", e);
                        return Err(e);
                    }
                }
            }

            return Ok(());
        }
        ["memtest", "run"] => {
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
        ["memtest"] => {
            let api_data = load_api_json(&api_path)?;
            println!("🧪 Generating memory layout test crate...\n");
            codegen::memtest::generate_memtest_crate(&api_data, &project_root)
                .map_err(|e| anyhow::anyhow!(e))?;
            return Ok(());
        }
        ["reftest", "headless", test_name] => {
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
        ["reftest"] => {
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

            if args.len() > 2 && args[1] == "open" {
                if open::that(report_path).is_ok() {
                    println!("Opened report in default browser.");
                } else {
                    eprintln!("Could not open browser. Please open the report manually.");
                }
            }

            return Ok(());
        }
        ["codegen"] | ["codegen", "rust"] => {
            let api_data = load_api_json(&api_path)?;
            let version = api_data.0.keys().next().expect("No version in api.json");

            println!("🔧 Generating Rust library code...\n");

            // Generate dll/lib.rs
            let dll_code = codegen::rust_dll::generate_rust_dll(&api_data, version);
            let dll_path = project_root.join("dll").join("lib.rs");
            fs::write(&dll_path, dll_code)?;
            println!("✅ Generated: {}", dll_path.display());

            // Generate azul-rs/azul.rs
            let rust_api_code = codegen::rust_api::generate_rust_api(&api_data, version);
            let rust_api_path = project_root.join("target").join("codegen").join("azul.rs");
            fs::create_dir_all(rust_api_path.parent().unwrap())?;
            fs::write(&rust_api_path, rust_api_code)?;
            println!("✅ Generated: {}", rust_api_path.display());

            println!("\n✨ Rust code generation complete!");
            return Ok(());
        }
        ["codegen", "c"] => {
            let api_data = load_api_json(&api_path)?;
            let version = api_data.0.keys().next().expect("No version in api.json");

            println!("🔧 Generating C header file...\n");

            let c_api_code = codegen::c_api::generate_c_api(&api_data, version);
            let c_api_path = project_root.join("target").join("codegen").join("azul.h");
            fs::create_dir_all(c_api_path.parent().unwrap())?;
            fs::write(&c_api_path, c_api_code)?;
            println!("✅ Generated: {}", c_api_path.display());

            println!("\n✨ C header generation complete!");
            return Ok(());
        }
        ["codegen", "cpp"] => {
            let api_data = load_api_json(&api_path)?;
            let version = api_data.0.keys().next().expect("No version in api.json");

            println!("🔧 Generating C++ header file...\n");

            let cpp_api_code = codegen::cpp_api::generate_cpp_api(&api_data, version);
            let cpp_api_path = project_root.join("target").join("codegen").join("azul.hpp");
            fs::create_dir_all(cpp_api_path.parent().unwrap())?;
            fs::write(&cpp_api_path, cpp_api_code)?;
            println!("✅ Generated: {}", cpp_api_path.display());

            println!("\n✨ C++ header generation complete!");
            return Ok(());
        }
        ["codegen", "python"] => {
            let api_data = load_api_json(&api_path)?;
            let version = api_data.0.keys().next().expect("No version in api.json");

            println!("🔧 Generating Python bindings...\n");

            let python_api_code = codegen::python_api::generate_python_api(&api_data, version);
            let python_api_path = project_root.join("dll").join("python.rs");
            fs::write(&python_api_path, python_api_code)?;
            println!("✅ Generated: {}", python_api_path.display());

            println!("\n✨ Python bindings generation complete!");
            return Ok(());
        }
        ["codegen", "all"] => {
            let api_data = load_api_json(&api_path)?;
            let version = api_data.0.keys().next().expect("No version in api.json");

            println!("🔧 Generating all language bindings...\n");

            // Generate dll/lib.rs
            let dll_code = codegen::rust_dll::generate_rust_dll(&api_data, version);
            let dll_path = project_root.join("dll").join("lib.rs");
            fs::write(&dll_path, dll_code)?;
            println!("✅ Generated: {}", dll_path.display());

            // Generate azul-rs/azul.rs
            let rust_api_code = codegen::rust_api::generate_rust_api(&api_data, version);
            let rust_api_path = project_root.join("target").join("codegen").join("azul.rs");
            fs::create_dir_all(rust_api_path.parent().unwrap())?;
            fs::write(&rust_api_path, rust_api_code)?;
            println!("✅ Generated: {}", rust_api_path.display());

            // Generate C header
            let c_api_code = codegen::c_api::generate_c_api(&api_data, version);
            let c_api_path = project_root.join("target").join("codegen").join("azul.h");
            fs::write(&c_api_path, c_api_code)?;
            println!("✅ Generated: {}", c_api_path.display());

            // Generate C++ header
            let cpp_api_code = codegen::cpp_api::generate_cpp_api(&api_data, version);
            let cpp_api_path = project_root.join("target").join("codegen").join("azul.hpp");
            fs::write(&cpp_api_path, cpp_api_code)?;
            println!("✅ Generated: {}", cpp_api_path.display());

            // Generate Python bindings
            let python_api_code = codegen::python_api::generate_python_api(&api_data, version);
            let python_api_path = project_root.join("dll").join("python.rs");
            fs::write(&python_api_path, python_api_code)?;
            println!("✅ Generated: {}", python_api_path.display());

            println!("\n✨ All language bindings generated successfully!");
            return Ok(());
        }
        ["deploy"] => {
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

    // DEBUG: Check if type_alias field was deserialized
    if let Some(version_data) = api_data.0.values().next() {
        if let Some(css_module) = version_data.api.get("css") {
            if let Some(lziv) = css_module.classes.get("LayoutZIndexValue") {
                eprintln!("DEBUG load_api_json: LayoutZIndexValue found");
                eprintln!("  type_alias: {:?}", lziv.type_alias);
            } else {
                eprintln!("DEBUG load_api_json: LayoutZIndexValue NOT found in css.classes");
            }

            let type_alias_count = css_module
                .classes
                .values()
                .filter(|c| c.type_alias.is_some())
                .count();
            eprintln!(
                "DEBUG load_api_json: {} classes have type_alias",
                type_alias_count
            );
        }
    }

    Ok(api_data)
}

fn print_cli_help() -> anyhow::Result<()> {
    println!("Azul Documentation and Deployment Tool");
    println!("Usage:");
    println!("  azul-doc print [options]        - Print API information");
    println!("  azul-doc normalize              - Normalize api.json");
    println!("  azul-doc autofix                - Apply automatic fixes to API definitions");
    println!("  azul-doc autofix explain        - Explain what generated patches will do");
    println!("  azul-doc patch safe <dir>       - Apply and delete safe (path-only) patches");
    println!("  azul-doc patch <patch_file>     - Apply patches to api.json");
    println!("  azul-doc memtest [run]          - Generate and optionally run memory layout tests");
    println!("  azul-doc reftest [open]         - Run reftests and optionally open report");
    println!("  azul-doc codegen [rust|c|cpp|python|all] - Generate language bindings");
    println!("  azul-doc deploy                 - Build and deploy the Azul library");
    Ok(())
}
