use std::{
    collections::BTreeMap,
    env,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::Result;

use crate::{api::ApiData, dllgen::license::License};

/// Verifies that all example files referenced in api.json exist on the filesystem.
///
/// Returns Ok(()) if all examples exist, or an error listing all missing files.
/// Use `strict` mode to fail the build, or non-strict to just print warnings.
pub fn verify_examples(api_data: &ApiData, examples_dir: &Path, strict: bool) -> Result<()> {
    let mut missing_files: Vec<String> = Vec::new();

    // Get all versions and their examples
    for version in api_data.get_sorted_versions() {
        if let Some(version_data) = api_data.get_version(&version) {
            for example in &version_data.examples {
                // Check required languages (c, rust, python)
                let c_path = examples_dir.join(&example.code.c);
                if !c_path.exists() {
                    missing_files.push(format!(
                        "[{}] {}: C example missing: {}",
                        version, example.name, example.code.c
                    ));
                }

                let rust_path = examples_dir.join(&example.code.rust);
                if !rust_path.exists() {
                    missing_files.push(format!(
                        "[{}] {}: Rust example missing: {}",
                        version, example.name, example.code.rust
                    ));
                }

                let python_path = examples_dir.join(&example.code.python);
                if !python_path.exists() {
                    missing_files.push(format!(
                        "[{}] {}: Python example missing: {}",
                        version, example.name, example.code.python
                    ));
                }

                // Check C++ dialects (optional fields)
                if let Some(ref cpp03) = example.code.cpp03 {
                    let cpp_path = examples_dir.join(cpp03);
                    if !cpp_path.exists() {
                        missing_files.push(format!(
                            "[{}] {}: C++03 example missing: {}",
                            version, example.name, cpp03
                        ));
                    }
                }
                if let Some(ref cpp11) = example.code.cpp11 {
                    let cpp_path = examples_dir.join(cpp11);
                    if !cpp_path.exists() {
                        missing_files.push(format!(
                            "[{}] {}: C++11 example missing: {}",
                            version, example.name, cpp11
                        ));
                    }
                }
                if let Some(ref cpp14) = example.code.cpp14 {
                    let cpp_path = examples_dir.join(cpp14);
                    if !cpp_path.exists() {
                        missing_files.push(format!(
                            "[{}] {}: C++14 example missing: {}",
                            version, example.name, cpp14
                        ));
                    }
                }
                if let Some(ref cpp17) = example.code.cpp17 {
                    let cpp_path = examples_dir.join(cpp17);
                    if !cpp_path.exists() {
                        missing_files.push(format!(
                            "[{}] {}: C++17 example missing: {}",
                            version, example.name, cpp17
                        ));
                    }
                }
                if let Some(ref cpp20) = example.code.cpp20 {
                    let cpp_path = examples_dir.join(cpp20);
                    if !cpp_path.exists() {
                        missing_files.push(format!(
                            "[{}] {}: C++20 example missing: {}",
                            version, example.name, cpp20
                        ));
                    }
                }
                if let Some(ref cpp23) = example.code.cpp23 {
                    let cpp_path = examples_dir.join(cpp23);
                    if !cpp_path.exists() {
                        missing_files.push(format!(
                            "[{}] {}: C++23 example missing: {}",
                            version, example.name, cpp23
                        ));
                    }
                }

                // Check legacy cpp field if no per-version paths exist
                if example.code.cpp03.is_none()
                    && example.code.cpp11.is_none()
                    && example.code.cpp14.is_none()
                    && example.code.cpp17.is_none()
                    && example.code.cpp20.is_none()
                    && example.code.cpp23.is_none()
                {
                    if let Some(ref cpp) = example.code.cpp {
                        let cpp_path = examples_dir.join(cpp);
                        if !cpp_path.exists() {
                            missing_files.push(format!(
                                "[{}] {}: C++ example missing: {}",
                                version, example.name, cpp
                            ));
                        }
                    }
                }
            }
        }
    }

    if missing_files.is_empty() {
        println!("  [OK] All example files verified");
        Ok(())
    } else {
        let error_msg = format!(
            "Missing {} example file(s):\n  {}",
            missing_files.len(),
            missing_files.join("\n  ")
        );

        if strict {
            anyhow::bail!("{}", error_msg);
        } else {
            eprintln!("  [WARN] {}", error_msg);
            Ok(())
        }
    }
}

/// Deploy mode controls how missing assets are handled
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeployMode {
    /// Local development mode - missing binary assets create placeholders
    Local,
    /// CI mode - missing binary assets cause deployment to fail
    Strict,
}

/// Binary assets that must be built by CI (cannot be generated on-the-fly)
#[derive(Debug, Clone)]
pub struct BinaryAsset {
    pub filename: &'static str,
    pub description: &'static str,
    pub platform: Platform,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Platform {
    Windows,
    Linux,
    MacOS,
    All,
}

impl BinaryAsset {
    pub const WINDOWS_ASSETS: &'static [BinaryAsset] = &[
        BinaryAsset {
            filename: "azul.dll",
            description: "Windows 64-bit dynamic library",
            platform: Platform::Windows,
        },
        BinaryAsset {
            filename: "azul.lib",
            description: "Windows 64-bit static library",
            platform: Platform::Windows,
        },
        BinaryAsset {
            filename: "azul.pyd",
            description: "Python Extension (Windows)",
            platform: Platform::Windows,
        },
        BinaryAsset {
            filename: "LICENSE-WINDOWS.txt",
            description: "Windows License",
            platform: Platform::Windows,
        },
    ];

    pub const LINUX_ASSETS: &'static [BinaryAsset] = &[
        BinaryAsset {
            filename: "libazul.so",
            description: "Linux 64-bit .so",
            platform: Platform::Linux,
        },
        BinaryAsset {
            filename: "libazul.linux.a",
            description: "Linux 64-bit .a",
            platform: Platform::Linux,
        },
        BinaryAsset {
            filename: "azul.cpython.so",
            description: "Python Extension (Linux)",
            platform: Platform::Linux,
        },
        BinaryAsset {
            filename: "LICENSE-LINUX.txt",
            description: "Linux License",
            platform: Platform::Linux,
        },
    ];

    pub const MACOS_ASSETS: &'static [BinaryAsset] = &[
        BinaryAsset {
            filename: "libazul.dylib",
            description: "MacOS 64-bit SO",
            platform: Platform::MacOS,
        },
        BinaryAsset {
            filename: "libazul.macos.a",
            description: "MacOS 64-bit .a",
            platform: Platform::MacOS,
        },
        BinaryAsset {
            filename: "azul.so",
            description: "Python Extension (macOS)",
            platform: Platform::MacOS,
        },
        BinaryAsset {
            filename: "LICENSE-MACOS.txt",
            description: "MacOS License",
            platform: Platform::MacOS,
        },
    ];

    pub fn all() -> Vec<&'static BinaryAsset> {
        Self::WINDOWS_ASSETS
            .iter()
            .chain(Self::LINUX_ASSETS.iter())
            .chain(Self::MACOS_ASSETS.iter())
            .collect()
    }
}

/// Information about an asset file (present or missing)
#[derive(Debug, Clone)]
pub struct AssetInfo {
    pub filename: String,
    pub description: String,
    pub size: Option<u64>,
    pub is_present: bool,
}

impl AssetInfo {
    /// Format file size in human-readable form (KB, MB, GB)
    pub fn humanize_size(&self) -> String {
        match self.size {
            None => "N/A".to_string(),
            Some(bytes) => {
                if bytes < 1024 {
                    format!("{}B", bytes)
                } else if bytes < 1024 * 1024 {
                    format!("{:.1}KB", bytes as f64 / 1024.0)
                } else if bytes < 1024 * 1024 * 1024 {
                    format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
                } else {
                    format!("{:.1}GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
                }
            }
        }
    }

    /// Check if an asset exists and get its size
    pub fn from_path(path: &Path, description: &str) -> Self {
        let is_present = path.exists();
        let size = if is_present {
            fs::metadata(path).ok().map(|m| m.len())
        } else {
            None
        };

        AssetInfo {
            filename: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            description: description.to_string(),
            size,
            is_present,
        }
    }
}

/// Collected asset information for a release version
#[derive(Debug, Clone)]
pub struct ReleaseAssets {
    pub windows: Vec<AssetInfo>,
    pub linux: Vec<AssetInfo>,
    pub macos: Vec<AssetInfo>,
    pub c_header: AssetInfo,
    pub cpp_headers: Vec<AssetInfo>,
    pub api_json: AssetInfo,
    pub examples_zip: AssetInfo,
}

impl ReleaseAssets {
    /// Collect asset information from the release directory
    pub fn collect(version_dir: &Path) -> Self {
        let mut windows = Vec::new();
        for asset in BinaryAsset::WINDOWS_ASSETS {
            windows.push(AssetInfo::from_path(
                &version_dir.join(asset.filename),
                asset.description,
            ));
        }

        let mut linux = Vec::new();
        for asset in BinaryAsset::LINUX_ASSETS {
            linux.push(AssetInfo::from_path(
                &version_dir.join(asset.filename),
                asset.description,
            ));
        }

        let mut macos = Vec::new();
        for asset in BinaryAsset::MACOS_ASSETS {
            macos.push(AssetInfo::from_path(
                &version_dir.join(asset.filename),
                asset.description,
            ));
        }

        let cpp_headers = vec![
            AssetInfo::from_path(&version_dir.join("azul_cpp03.hpp"), "C++03 Header"),
            AssetInfo::from_path(&version_dir.join("azul_cpp11.hpp"), "C++11 Header"),
            AssetInfo::from_path(&version_dir.join("azul_cpp14.hpp"), "C++14 Header"),
            AssetInfo::from_path(&version_dir.join("azul_cpp17.hpp"), "C++17 Header"),
            AssetInfo::from_path(&version_dir.join("azul_cpp20.hpp"), "C++20 Header"),
            AssetInfo::from_path(&version_dir.join("azul_cpp23.hpp"), "C++23 Header"),
        ];

        ReleaseAssets {
            windows,
            linux,
            macos,
            c_header: AssetInfo::from_path(&version_dir.join("azul.h"), "C Header"),
            cpp_headers,
            api_json: AssetInfo::from_path(&version_dir.join("api.json"), "API Description"),
            examples_zip: AssetInfo::from_path(&version_dir.join("examples.zip"), "Examples"),
        }
    }

    /// Check if all binary assets are present (for strict mode)
    pub fn validate_binary_assets(&self) -> Result<(), Vec<String>> {
        let mut missing = Vec::new();

        for asset in &self.windows {
            if !asset.is_present {
                missing.push(asset.filename.clone());
            }
        }
        for asset in &self.linux {
            if !asset.is_present {
                missing.push(asset.filename.clone());
            }
        }
        for asset in &self.macos {
            if !asset.is_present {
                missing.push(asset.filename.clone());
            }
        }

        if missing.is_empty() {
            Ok(())
        } else {
            Err(missing)
        }
    }
}

pub struct Config {
    pub build_windows: bool,
    pub build_linux: bool,
    pub build_macos: bool,
    pub build_python: bool,
    pub open: bool,
    pub apply_patch: bool,
    pub print_imports: bool,
    pub deploy_mode: DeployMode,
}

impl Config {
    pub fn print(&self) -> String {
        let mut v = Vec::new();
        let mut build = Vec::new();
        if self.build_windows {
            build.push("windows");
        }
        if self.build_linux {
            build.push("linux");
        }
        if self.build_macos {
            build.push("mac");
        }
        v.push(format!("build={}", build.join(",")));
        if self.build_python {
            v.push("python=true".to_string());
        }
        if self.open {
            v.push("open=true".to_string());
        }
        if self.apply_patch {
            v.push("apply-patch=true".to_string());
        }
        if self.print_imports {
            v.push("print-imports=true".to_string());
        }
        v.push(format!("mode={:?}", self.deploy_mode));
        v.join(" ")
    }

    pub fn from_args() -> Self {
        let args: Vec<String> = env::args().collect();
        let mut config = Self {
            build_windows: false,
            build_linux: false,
            build_macos: false,
            build_python: false,
            open: false,
            apply_patch: false,
            print_imports: false,
            deploy_mode: DeployMode::Local,
        };

        for arg in &args[1..] {
            if let Some(value) = arg.strip_prefix("--build=") {
                config.parse_build_arg(value);
                continue;
            }

            if arg == "--open" {
                config.open = true;
                continue;
            }

            if arg == "--apply-patch" {
                config.apply_patch = true;
                continue;
            }

            if arg == "--print-imports" {
                config.print_imports = true;
                continue;
            }

            if arg == "--strict" || arg == "--ci" {
                config.deploy_mode = DeployMode::Strict;
                continue;
            }
        }

        config
    }

    fn parse_build_arg(&mut self, value: &str) {
        if value == "all" {
            self.build_windows = true;
            self.build_linux = true;
            self.build_macos = true;
            self.build_python = true;
            return;
        }

        if value == "none" {
            return;
        }

        for target in value.split(',') {
            match target {
                "windows" => self.build_windows = true,
                "linux" => self.build_linux = true,
                "macos" => self.build_macos = true,
                "python" => self.build_python = true,
                _ => {}
            }
        }
    }
}

pub fn generate_license_files(version: &str, output_dir: &Path) -> Result<()> {
    println!("  Generating license files...");

    let dll_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../dll");

    assert!(Path::new(dll_path).join("Xargo.toml").exists());

    let targets = &[
        ("LICENSE-WINDOWS.txt", "x86_64-pc-windows-msvc"),
        ("LICENSE-MACOS.txt", "aarch64-apple-darwin"),
        ("LICENSE-LINUX.txt", "x86_64-unknown-linux-gnu"),
    ];

    for (f, target) in targets.iter() {
        // Use cargo-license to get dependency information
        let cargo_meta_cmd = cargo_metadata::MetadataCommand::new()
            .current_dir(dll_path)
            .env("CARGO_BUILD_TARGET", target)
            .clone();

        let opt = cargo_license::GetDependenciesOpt {
            avoid_dev_deps: true,
            avoid_build_deps: true,
            avoid_proc_macros: true,
            direct_deps_only: false,
            root_only: false,
        };

        let l = cargo_license::get_dependencies_from_cargo_lock(&cargo_meta_cmd, &opt)
            .unwrap_or_default()
            .into_iter()
            .map(|s| License {
                name: s.name.to_string(),
                version: s.version.to_string(),
                license_type: s.license.unwrap_or_default(),
                authors: s
                    .authors
                    .unwrap_or_default()
                    .split("|")
                    .map(|s| s.to_string())
                    .collect(),
            })
            .collect::<Vec<_>>();

        let default_license_text = vec![
            "[program] is based in part on the AZUL GUI toolkit (https://azul.rs),",
            "licensed under the MIT License (C) 2018 Felix Schütt.",
            "",
            "The AZUL GUI toolkit itself uses the following libraries:",
            "",
            "",
        ]
        .join("\r\n");

        let license_posttext = vec![
            "",
            "To generate the full text of the license for the license, please visit",
            "https://spdx.org/licenses/ and replace the license author in the source",
            "text in any given license with the name of the author listed above.",
        ]
        .join("\r\n");

        let mut s = String::new();
        s.push_str(&default_license_text);
        s.push_str(&crate::dllgen::license::format_license_authors(&l));
        s.push_str(&license_posttext);
        std::fs::write(&output_dir.join(f), &s)?;
    }

    println!("  - Generated license files");
    Ok(())
}

/// C++ headers for all supported standards
pub struct CppHeaders {
    pub cpp03: String,
    pub cpp11: String,
    pub cpp14: String,
    pub cpp17: String,
    pub cpp20: String,
    pub cpp23: String,
}

/// Creates examples.zip by reading example paths from api.json and loading files from disk.
/// This ensures the zip always matches the examples defined in api.json.
pub fn create_examples(
    version: &str,
    output_dir: &Path,
    azul_h: &str,
    cpp_headers: &CppHeaders,
    api_data: &ApiData,
    examples_dir: &Path,
) -> Result<()> {
    println!("  Creating example packages...");

    let source_zip_path = output_dir.join("examples.zip");
    let source_zip_file = File::create(&source_zip_path)?;

    let mut source_zip = zip::ZipWriter::new(source_zip_file);
    let options = zip::write::SimpleFileOptions::default();

    // Get examples from api.json for this version
    let version_data = api_data
        .get_version(version)
        .ok_or_else(|| anyhow::anyhow!("Version {} not found in api.json", version))?;

    // Track which files we've already added (to avoid duplicates)
    let mut added_files: std::collections::HashSet<String> = std::collections::HashSet::new();

    for example in &version_data.examples {
        // Add C example
        let c_path = &example.code.c;
        if !added_files.contains(c_path) {
            let full_path = examples_dir.join(c_path);
            if full_path.exists() {
                let content = fs::read(&full_path)?;
                source_zip.start_file(c_path, options)?;
                source_zip.write_all(&content)?;
                added_files.insert(c_path.clone());
            }
        }

        // Add Rust example
        let rust_path = &example.code.rust;
        if !added_files.contains(rust_path) {
            let full_path = examples_dir.join(rust_path);
            if full_path.exists() {
                let content = fs::read(&full_path)?;
                source_zip.start_file(rust_path, options)?;
                source_zip.write_all(&content)?;
                added_files.insert(rust_path.clone());
            }
        }

        // Add Python example
        let python_path = &example.code.python;
        if !added_files.contains(python_path) {
            let full_path = examples_dir.join(python_path);
            if full_path.exists() {
                let content = fs::read(&full_path)?;
                source_zip.start_file(python_path, options)?;
                source_zip.write_all(&content)?;
                added_files.insert(python_path.clone());
            }
        }

        // Add C++ examples for each standard
        let cpp_paths = [
            &example.code.cpp03,
            &example.code.cpp11,
            &example.code.cpp14,
            &example.code.cpp17,
            &example.code.cpp20,
            &example.code.cpp23,
            &example.code.cpp, // legacy fallback
        ];

        for cpp_opt in cpp_paths.iter() {
            if let Some(cpp_path) = cpp_opt {
                if !added_files.contains(cpp_path) {
                    let full_path = examples_dir.join(cpp_path);
                    if full_path.exists() {
                        let content = fs::read(&full_path)?;
                        source_zip.start_file(cpp_path, options)?;
                        source_zip.write_all(&content)?;
                        added_files.insert(cpp_path.clone());
                    }
                }
            }
        }
    }

    // C header
    source_zip.start_file("include/azul.h", options)?;
    source_zip.write_all(azul_h.as_bytes())?;

    // C++ headers for all standards
    source_zip.start_file("include/cpp/azul_cpp03.hpp", options)?;
    source_zip.write_all(cpp_headers.cpp03.as_bytes())?;
    source_zip.start_file("include/cpp/azul_cpp11.hpp", options)?;
    source_zip.write_all(cpp_headers.cpp11.as_bytes())?;
    source_zip.start_file("include/cpp/azul_cpp14.hpp", options)?;
    source_zip.write_all(cpp_headers.cpp14.as_bytes())?;
    source_zip.start_file("include/cpp/azul_cpp17.hpp", options)?;
    source_zip.write_all(cpp_headers.cpp17.as_bytes())?;
    source_zip.start_file("include/cpp/azul_cpp20.hpp", options)?;
    source_zip.write_all(cpp_headers.cpp20.as_bytes())?;
    source_zip.start_file("include/cpp/azul_cpp23.hpp", options)?;
    source_zip.write_all(cpp_headers.cpp23.as_bytes())?;

    // Add README
    source_zip.start_file("README.md", options)?;
    source_zip.write_all(
        format!(
            "# Azul GUI Framework v{}\n\nCross-platform GUI framework for Rust, C, C++ and Python",
            version
        )
        .as_bytes(),
    )?;

    // Finalize source zip
    source_zip.finish()?;

    println!(
        "  - Created example packages ({} files from api.json)",
        added_files.len()
    );

    Ok(())
}

pub fn create_git_repository(version: &str, output_dir: &Path, lib_rs: &str) -> Result<()> {
    println!("  Creating Git repository for version {}...", version);

    // Create repository directory
    let repo_dir = output_dir.join(format!("{}.git", version));
    fs::create_dir_all(&repo_dir)?;

    // Create basic repo structure
    fs::create_dir_all(repo_dir.join("objects/info"))?;
    fs::create_dir_all(repo_dir.join("objects/pack"))?;
    fs::create_dir_all(repo_dir.join("refs/heads"))?;
    fs::create_dir_all(repo_dir.join("refs/tags"))?;

    // Create HEAD file
    fs::write(repo_dir.join("HEAD"), "ref: refs/heads/master\n")?;

    // Create config file
    fs::write(
        repo_dir.join("config"),
        r#"[core]
    repositoryformatversion = 0
    filemode = false
    bare = true
    "#,
    )?;

    // Create description file
    fs::write(
        repo_dir.join("description"),
        format!("Azul GUI Framework v{}", version),
    )?;

    // For demonstration, create the src directory structure with lib.rs
    let src_dir = repo_dir.join("src");
    fs::create_dir_all(&src_dir)?;
    fs::write(src_dir.join("lib.rs"), lib_rs)?;

    // Create Cargo.toml
    fs::write(
        repo_dir.join("Cargo.toml"),
        format!(
            r#"[package]
        name = "azul"
        version = "{}"
        authors = ["Felix Schütt <felix.schuett@maps4print.com>"]
        license = "MIT"
        description = '''
            Azul GUI is a free, functional, reactive GUI framework
            for rapid development of desktop applications written in Rust and C,
            using the Mozilla WebRender rendering engine.
        '''
        homepage = "https://azul.rs/"
        keywords = ["gui", "GUI", "user-interface", "svg", "graphics" ]
        categories = ["gui"]
        repository = "https://github.com/fschutt/azul"
        readme = "README.md"
        exclude = ["assets/*", "doc/*", "examples/*"]
        autoexamples = false
        edition = "2021"
        build = "build.rs"
        links = "azul"

        [dependencies]
        serde = {{ version = "1", optional = true, default-features = false }}
        serde_derive = {{ version = "1", optional = true, default-features = false }}

        [features]
        default = ["link-static"]
        serde-support = ["serde_derive", "serde"]
        docs_rs = ["link-static"]
        link-dynamic = []
        link-static = []

        [package.metadata.docs.rs]
        features = ["docs_rs"]
    "#,
            version
        )
        .lines()
        .map(|s| s.trim())
        .collect::<Vec<_>>()
        .join("\r\n"),
    )?;

    // Create build.rs
    fs::write(
        repo_dir.join("build.rs"),
        r#"fn main() {
    // dynamically link azul.dll
    #[cfg(all(feature = "link-dynamic", not(feature = "link-static")))]
    {
        println!("cargo:rustc-link-search={}", env!("AZUL_LINK_PATH")); /* path to folder with azul.dll / libazul.so */
    }
}
"#,
    )?;

    println!("  - Created Git repository structure");
    Ok(())
}

/// Generate a single asset list item HTML
fn generate_asset_li(version: &str, asset: &AssetInfo) -> String {
    if asset.is_present {
        format!(
            "<li><a href='https://azul.rs/release/{version}/{filename}'>{description} ({filename} \
             - {size})</a></li>",
            version = version,
            filename = asset.filename,
            description = asset.description,
            size = asset.humanize_size()
        )
    } else {
        format!(
            "<li><span style='color: #999;'>{description} ({filename} - not available)</span></li>",
            filename = asset.filename,
            description = asset.description
        )
    }
}

pub fn generate_release_html(version: &str, api_data: &ApiData, assets: &ReleaseAssets) -> String {
    let versiondata = api_data.get_version(version).unwrap();
    let common_head_tags = crate::docgen::get_common_head_tags(false);
    let sidebar = crate::docgen::get_sidebar();
    let prism_script = crate::docgen::get_prism_script();
    let releasenotes =
        comrak::markdown_to_html(&versiondata.notes.join("\r\n"), &comrak::Options::default());
    let git = &versiondata.git;

    // Generate Windows asset list
    let windows_assets: String = assets
        .windows
        .iter()
        .map(|a| generate_asset_li(version, a))
        .collect::<Vec<_>>()
        .join("\n                ");

    // Generate Linux asset list
    let linux_assets: String = assets
        .linux
        .iter()
        .map(|a| generate_asset_li(version, a))
        .collect::<Vec<_>>()
        .join("\n                ");

    // Generate MacOS asset list
    let macos_assets: String = assets
        .macos
        .iter()
        .map(|a| generate_asset_li(version, a))
        .collect::<Vec<_>>()
        .join("\n                ");

    // Generate C header link
    let c_header_link = generate_asset_li(version, &assets.c_header);

    // Generate C++ header links
    let cpp_header_links: String = assets
        .cpp_headers
        .iter()
        .map(|a| {
            if a.is_present {
                format!(
                    "<li><a href='https://azul.rs/release/{version}/{filename}'>{description} \
                     ({filename})</a></li>",
                    version = version,
                    filename = a.filename,
                    description = a.description
                )
            } else {
                format!(
                    "<li><span style='color: #999;'>{description} ({filename} - not \
                     available)</span></li>",
                    filename = a.filename,
                    description = a.description
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n                ");

    // Generate API/examples links
    let api_json_link = generate_asset_li(version, &assets.api_json);
    let examples_zip_link = generate_asset_li(version, &assets.examples_zip);

    format!(
        "<!DOCTYPE html>
    <html lang='en'>
    <head>
        <title>Azul GUI v{version} (git {git}) \
         - Release Notes</title>
        {common_head_tags}
    </head>

    <body>
      <div class='center'>\
         
        <aside>
          <header>
            <a href='https://azul.rs/'>
              <img \
         src='https://azul.rs/logo.svg'>
            </a>
          </header>
          {sidebar}
        \
         </aside>

        <main>
          <h1>Azul GUI v{version}</h1>
          <a href='https://github.com/fschutt/azul/commit/{git}'>(git \
         {git})</a>
          <style>
            main h1 {{ margin-bottom: none; }}
            ul {{ \
         margin-left: 20px; margin-top: 20px; list-style-type: none; }} 
            nav ul {{ margin: \
         0px; }} 
            #releasenotes {{ margin-top: 20px; max-width: 700px; }}
            #releasenotes \
         ul {{ list-style-type: initial; }} 
            #releasenotes ul li {{ margin-bottom: 2px; \
         }} 
            #releasenotes p {{ margin-bottom: 10px; margin-top: 10px; }}
            </style>\
         
          <div>
              
              <div id='releasenotes'>
              {releasenotes}\
         
              </div>

              <br/>

              <strong>Links:</strong>
         <ul>
                <li><a href='https://azul.rs/api/{version}.html'>Documentation for this release</a></li>
                <li><a href='https://azul.rs/guide'>Guide</a></li>
         <br/>
                <li><a href='https://github.com/fschutt/azul/releases/tag/{version}'>GitHub release</a></li>
                <li><a href='https://crates.io/crates/azul/{version}'>Crates.io</a></li>
                <li><a href='https://docs.rs/azul/{version}'>Docs.rs</a></li>
         </ul>

              <br/>

              <strong>Files:</strong>
              <br/>
         <ul>
                {windows_assets}
              </ul>
              <ul>
                {linux_assets}
              </ul>
              <ul>
                {macos_assets}
              </ul>

         <br/>

              <strong>C Header:</strong>
              <br/>
              <ul>
         {c_header_link}
              </ul>
              
              <br/>
              <strong>C++ Headers:</strong>
              <ul>
                {cpp_header_links}\
         
              </ul>

              <br/>
              <strong>API Description &amp; Examples:</strong>\
         
              <ul>
                {api_json_link}
                {examples_zip_link}
              \
         </ul>

              <br/>
              <strong>Use Azul as Rust dependency:</strong>
              \
         <br/>

              <div style='padding:20px;background:rgb(236, 236, 236);margin-top: 20px;'>\
         
                  <p style='color:grey;font-family:monospace;'># Cargo.toml</p>
                  \
         <p style='color:black;font-family:monospace;'>[dependencies.azul]</p>
                  <p \
         style='color:black;font-family:monospace;'>git = \"https://azul.rs/{version}.git\"</p>
                  <br/>
                  <p style='color:grey;font-family:monospace;'># Dynamic linking:</p>
                  <p style='color:grey;font-family:monospace;'># export \
         AZUL_LINK_PATH=/path/to/azul.dll</p>
                  <p style='color:grey;font-family:monospace;'># features = ['link-dynamic']</p>
              </div>
          </div>
        </main>
      </div>
      {prism_script}
    </body>
    </html>"
    )
}

pub fn generate_releases_index(versions: &[String]) -> String {
    let mut version_items = String::new();
    for version in versions {
        version_items.push_str(&format!(
            "<li><a href=\"https://azul.rs/release/{}\">{}</a></li>\n",
            version, version
        ));
    }

    let header_tags = crate::docgen::get_common_head_tags(false);
    let sidebar = crate::docgen::get_sidebar();
    let prism_script = crate::docgen::get_prism_script();

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <title>Choose release version</title>

  {header_tags}
</head>

<body>
  <div class="center">
  <aside>
    <header>
      <h1 style="display:none;">Azul GUI Framework</h1>
      <a href="https://azul.rs/">
        <img src="https://azul.rs/logo.svg">
      </a>
    </header>
    {sidebar}
  </aside>
  <main>
    <h1>Choose release version</h1>
    <div>
      <ul>{}</ul>
    </div>
  </main>
  </div>
  {prism_script}
</body>
</html>"#,
        version_items
    )
}

pub fn copy_static_assets(output_dir: &Path) -> Result<()> {
    println!("Copying static assets...");

    // Get the templates directory (relative to the doc crate root)
    let templates_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let fonts_source_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fonts");

    // Create assets directories
    let fonts_dir = output_dir.join("fonts");
    let images_dir = output_dir.join("images");
    fs::create_dir_all(&fonts_dir)?;
    fs::create_dir_all(&images_dir)?;

    // Copy CSS file at runtime (so edits take effect without recompiling)
    fs::copy(templates_dir.join("main.css"), output_dir.join("main.css"))?;

    // Copy JavaScript file at runtime
    fs::copy(
        templates_dir.join("prism_code_highlighter.js"),
        output_dir.join("prism_code_highlighter.js"),
    )?;

    // Copy logo SVG at runtime
    fs::copy(templates_dir.join("logo.svg"), output_dir.join("logo.svg"))?;

    // Copy favicon.ico for local development
    let favicon_source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("examples/assets/images/favicon.ico");
    if favicon_source.exists() {
        fs::copy(&favicon_source, output_dir.join("favicon.ico"))?;
    }

    // Copy fleur-de-lis SVG (for navigation) at runtime
    fs::copy(
        templates_dir.join("fleur-de-lis.svg"),
        images_dir.join("fleur-de-lis.svg"),
    )?;

    // Copy font files at runtime
    fs::copy(
        fonts_source_dir.join("InstrumentSerif-Regular.ttf"),
        fonts_dir.join("InstrumentSerif-Regular.ttf"),
    )?;
    fs::copy(
        fonts_source_dir.join("InstrumentSerif-Italic.ttf"),
        fonts_dir.join("InstrumentSerif-Italic.ttf"),
    )?;
    fs::copy(
        fonts_source_dir.join("SourceSerifPro-Regular.ttf"),
        fonts_dir.join("SourceSerifPro-Regular.ttf"),
    )?;

    // Create favicon
    fs::write(output_dir.join("favicon.ico"), "Favicon placeholder")?;

    println!("Static assets copied successfully");
    Ok(())
}

/// Generate NFPM configuration YAML from api.json package metadata
///
/// This function reads the package configuration from api.json and generates
/// an nfpm.yaml file that can be used to build .deb, .rpm, and .apk packages.
///
/// # Arguments
/// * `version` - The version string to use (e.g., "0.0.5")
/// * `api_data` - The parsed api.json data
/// * `output_dir` - Directory where nfpm.yaml should be written
///
/// # Returns
/// The path to the generated nfpm.yaml file, or an error if package config is missing
pub fn generate_nfpm_yaml(
    version: &str,
    api_data: &ApiData,
    output_dir: &Path,
) -> Result<PathBuf> {
    let version_data = api_data
        .get_version(version)
        .ok_or_else(|| anyhow::anyhow!("Version {} not found in api.json", version))?;

    let package = version_data
        .package
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No package configuration found for version {}", version))?;

    let yaml = generate_nfpm_yaml_content(version, package);

    // Write the file
    fs::create_dir_all(output_dir)?;
    let output_path = output_dir.join("nfpm.yaml");
    fs::write(&output_path, &yaml)?;

    println!("Generated NFPM config: {}", output_path.display());
    Ok(output_path)
}

/// Generate NFPM YAML content from package config
fn generate_nfpm_yaml_content(version: &str, package: &crate::api::PackageConfig) -> String {
    let mut yaml = String::new();

    // Basic package info
    yaml.push_str(&format!("name: {}\n", package.name));
    yaml.push_str("arch: amd64\n");
    yaml.push_str("platform: linux\n");
    yaml.push_str(&format!("version: {}\n", version));
    if !package.maintainer.is_empty() {
        yaml.push_str(&format!("maintainer: {}\n", package.maintainer));
    }
    yaml.push_str(&format!("description: |\n  {}\n", package.description));
    yaml.push_str(&format!("homepage: {}\n", package.homepage));
    yaml.push_str(&format!("license: \"{}\"\n", package.license));
    if !package.vendor.is_empty() {
        yaml.push_str(&format!("vendor: {}\n", package.vendor));
    }
    if !package.section.is_empty() {
        yaml.push_str(&format!("section: {}\n", package.section));
    }
    if !package.priority.is_empty() {
        yaml.push_str(&format!("priority: {}\n", package.priority));
    }

    // Dependencies (for .deb packages)
    if !package.linux.depends.is_empty() {
        yaml.push_str("\ndepends:\n");
        for dep in &package.linux.depends {
            yaml.push_str(&format!("  - {}\n", dep));
        }
    }

    if !package.linux.recommends.is_empty() {
        yaml.push_str("\nrecommends:\n");
        for rec in &package.linux.recommends {
            yaml.push_str(&format!("  - {}\n", rec));
        }
    }

    if !package.linux.suggests.is_empty() {
        yaml.push_str("\nsuggests:\n");
        for sug in &package.linux.suggests {
            yaml.push_str(&format!("  - {}\n", sug));
        }
    }

    // RPM-specific configuration
    if !package.rpm.group.is_empty() || !package.rpm.depends.is_empty() {
        yaml.push_str("\nrpm:\n");
        if !package.rpm.group.is_empty() {
            yaml.push_str(&format!("  group: {}\n", package.rpm.group));
        }
        if !package.rpm.depends.is_empty() {
            yaml.push_str("  depends:\n");
            for dep in &package.rpm.depends {
                yaml.push_str(&format!("    - {}\n", dep));
            }
        }
        if !package.rpm.recommends.is_empty() {
            yaml.push_str("  recommends:\n");
            for rec in &package.rpm.recommends {
                yaml.push_str(&format!("    - {}\n", rec));
            }
        }
        if !package.rpm.suggests.is_empty() {
            yaml.push_str("  suggests:\n");
            for sug in &package.rpm.suggests {
                yaml.push_str(&format!("    - {}\n", sug));
            }
        }
    }

    // Contents (files to include) - now under linux
    if !package.linux.contents.is_empty() {
        yaml.push_str("\ncontents:\n");
        for content in &package.linux.contents {
            yaml.push_str(&format!("  - src: {}\n", content.src));
            yaml.push_str(&format!("    dst: {}\n", content.dst));
            if !content.content_type.is_empty() && content.content_type != "file" {
                yaml.push_str(&format!("    type: {}\n", content.content_type));
            }
        }
    }

    yaml
}
