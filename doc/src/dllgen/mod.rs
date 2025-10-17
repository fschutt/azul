
pub mod build;
pub mod deploy;
pub mod license;

// TODO: This function needs to be refactored - it has many undefined variables
// and seems to be an incomplete migration from the old structure
/*
pub fn generate_dll_and_folders_deploy(config: &deploy::Config) -> Result<(), String> {
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
    for (path, html) in docgen::generate_docs(&api_data, &image_path, "https://azul.rs/images")? {
        let path_real = output_dir.join(path);
        if let Some(parent) = path_real.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(&path_real, &html)?;
    }

    // Process each version
    for version in &versions {
        println!("Generating API bindings for version {}...", version);

        let version_dir = releases_dir.join(version);
        let files_dir = version_dir.clone();
        fs::create_dir_all(&version_dir)?;
        fs::create_dir_all(&files_dir)?;

        let c_api_path = version_dir.join("azul.h");
        println!(
            "  Generating C API for version {}: {}",
            version,
            c_api_path.display()
        );
        let c_api_code = codegen::c_api::generate_c_api(&api_data, version);
        fs::write(&c_api_path, &c_api_code)?;

        let cpp_api_path = version_dir.join("azul.hpp");
        println!(
            "  Generating C++ API for version {}: {}",
            version,
            cpp_api_path.display()
        );
        let cpp_api_code = codegen::cpp_api::generate_cpp_api(&api_data, version);
        fs::write(&cpp_api_path, &cpp_api_code)?;

        let python_api_path = version_dir.join("azul_python.rs");
        println!(
            "  Generating Python bindings for version {}: {}",
            version,
            python_api_path.display()
        );
        let python_api_code = codegen::python_api::generate_python_api(&api_data, version);
        fs::write(&python_api_path, &python_api_code)?;

        let rust_dll_path = version_dir.join("azul_dll.rs");
        println!(
            "  Generating rust DLL code for version {}: {}",
            version,
            rust_dll_path.display()
        );
        let rust_dll_code = codegen::rust_dll::generate_rust_dll(&api_data, version);
        fs::write(&rust_dll_path, &rust_dll_code)?;

        let rust_api_path = output_dir
            .join(format!("azul-{version}"))
            .join("src")
            .join("lib.rs");
        println!(
            "  Generating git repository for version {}: {}",
            version,
            rust_api_path.display()
        );
        let lib_rs = codegen::rust_api::generate_rust_api(&api_data, version);
        dllgen::create_git_repository(version, &output_dir, &lib_rs)?;

        println!("  Generating api.json for version {}...", version);
        let api_json = serde_json::to_string_pretty(api_data.get_version(version).unwrap())?;
        fs::write(version_dir.join("api.json"), api_json)?;

        println!("  Generating LICENSE files for version {}...", version);
        dllgen::generate_license_files(version, &files_dir)?;

        println!("  Compiling examples for version {}...", version);
        dllgen::create_examples(version, &files_dir, &c_api_code, &cpp_api_code)?; // TODO: compile examples

        println!("  Building release.html for version {}...", version);
        let release_html = deploy::generate_release_html(version, &api_data);
        fs::write(releases_dir.join(&format!("{version}.html")), release_html)?;

        println!("  Building binaries for version {}...", version);
        crate::build::build_all_configs(version, &version_dir, &config)?;
    }

    // Generate releases index
    let releases_index = deploy::generate_releases_index(&versions);
    fs::write(output_dir.join("releases.html"), &releases_index)?;

    // Copy static assets
    deploy::copy_static_assets(&output_dir)?;

    // Generate donation page
    println!("Generating donation page...");
    let funding_yaml_bytes = include_str!("../../../.github/FUNDING.yml");
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

    println!("\n✅ All API bindings generated successfully!");
    println!("   C API:      {} versions", versions.len());
    println!("   C++ API:    {} versions", versions.len());
    println!("   Python API: {} versions", versions.len());
    println!("   Rust API:   {} versions", versions.len());
    println!("   Rust DLL:   {} versions", versions.len());

    Ok(())
}
*/
