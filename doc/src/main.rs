// Add other modules later: fs_utils, process, codegen, etc.

mod api;
use anyhow::{Context, Result};
use api::ApiData;

// Assume api.json is in the parent directory relative to the crate root
// Adjust this path if your build_runner crate is elsewhere.
const API_JSON_STR: &str = include_str!("../../api.json");

fn main() -> Result<()> {
    println!("Starting Azul Build Runner...");

    // --- Load and Parse API Data ---
    println!("Parsing embedded api.json...");
    let api_data: ApiData = serde_json::from_str(API_JSON_STR)
        .context("Failed to parse api.json")?;
    println!("API data parsed successfully.");

    // --- Get Sorted Versions ---
    let sorted_versions = api_data.get_sorted_versions();
    if sorted_versions.is_empty() {
        anyhow::bail!("No versions found in api.json");
    }
    println!("Found API versions (sorted): {:?}", sorted_versions);

    let latest_version_str = api_data.get_latest_version_str().unwrap_or("unknown");
    println!("Latest version detected: {}", latest_version_str);


    // --- Process Each Version ---
    // This loop structure is where you'll put the code generation, build, test,
    // and artifact archiving logic for each version.
    for version_str in &sorted_versions {
        println!("\n=== Processing Version: {} ===", version_str);

        // Get the specific data for this version
        let version_data = api_data.get_version(version_str)
            .ok_or_else(|| anyhow::anyhow!("Internal error: Version {} not found after sorting keys", version_str))?;

        // **Placeholder for future steps:**
        // 1. Generate code based on `version_data` -> fs_utils::write_file(...)
        //    codegen::generate_all_apis(&root_dir, version_data)?;

        // 2. Build DLL for this version -> process::build_dll(...)
        //    process::build_dll(&root_dir, Some("target-triple"))?;

        // 3. Run tests for this version -> process::run_size_tests(...)
        //    process::run_size_tests(&root_dir)?;

        // 4. Archive artifacts -> artifacts::archive_version_artifacts(...)
        //    artifacts::archive_version_artifacts(&root_dir, version_str, "target-triple", &version_artifact_dir)?;

        println!("Finished processing steps for v{} (placeholders).", version_str);
    }

    // --- Post-Loop Tasks ---
    println!("\n=== Post-Version Processing ===");

    // **Placeholder for future steps:**
    // 5. Generate Documentation (likely using latest version data)
    //    let latest_data = api_data.get_version(latest_version_str).unwrap();
    //    codegen::docs::generate_docs(&root_dir, &api_data, &html_dir, &artifacts_dir)?;

    // 6. Generate License file (using latest dependencies)
    //    codegen::license::generate_license(&root_dir)?;

    // 7. Run Reftests (using latest code)
    //    reftest::run_reftests(&root_dir, &html_dir.join("output"))?;


    println!("\n✅ Azul Build Runner finished basic parsing and version iteration.");
    Ok(())
}