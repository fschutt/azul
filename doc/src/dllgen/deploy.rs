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
            filename: "azul.dll.lib",
            description: "Windows MSVC import library",
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

    /// Cross-architecture assets (experimental, optional — only shown if present)
    pub const CROSS_ARCH_ASSETS: &'static [BinaryAsset] = &[
        // Linux cross-arch
        BinaryAsset {
            filename: "libazul.linux-i686.so",
            description: "Linux 32-bit x86 .so",
            platform: Platform::Linux,
        },
        BinaryAsset {
            filename: "libazul.linux-i686.a",
            description: "Linux 32-bit x86 .a",
            platform: Platform::Linux,
        },
        BinaryAsset {
            filename: "libazul.linux-aarch64.so",
            description: "Linux ARM64 .so",
            platform: Platform::Linux,
        },
        BinaryAsset {
            filename: "libazul.linux-aarch64.a",
            description: "Linux ARM64 .a",
            platform: Platform::Linux,
        },
        BinaryAsset {
            filename: "libazul.linux-armv7.so",
            description: "Linux ARMv7 .so (Raspberry Pi)",
            platform: Platform::Linux,
        },
        BinaryAsset {
            filename: "libazul.linux-armv7.a",
            description: "Linux ARMv7 .a (Raspberry Pi)",
            platform: Platform::Linux,
        },
        // Exotic Linux arches (experimental, big-endian + RISC-V) — match the
        // azul-linux-{ppc64,s390x,riscv64} artifacts the cross_build_binaries
        // job produces and the deploy lays out as libazul.linux-<arch>.{so,a}.
        BinaryAsset {
            filename: "libazul.linux-ppc64.so",
            description: "Linux PowerPC64 .so (big-endian)",
            platform: Platform::Linux,
        },
        BinaryAsset {
            filename: "libazul.linux-ppc64.a",
            description: "Linux PowerPC64 .a (big-endian)",
            platform: Platform::Linux,
        },
        BinaryAsset {
            filename: "libazul.linux-s390x.so",
            description: "Linux s390x .so (IBM Z, big-endian)",
            platform: Platform::Linux,
        },
        BinaryAsset {
            filename: "libazul.linux-s390x.a",
            description: "Linux s390x .a (IBM Z, big-endian)",
            platform: Platform::Linux,
        },
        BinaryAsset {
            filename: "libazul.linux-riscv64.so",
            description: "Linux RISC-V 64 .so",
            platform: Platform::Linux,
        },
        BinaryAsset {
            filename: "libazul.linux-riscv64.a",
            description: "Linux RISC-V 64 .a",
            platform: Platform::Linux,
        },
        // Windows cross-arch
        BinaryAsset {
            filename: "azul.i686.dll",
            description: "Windows 32-bit x86 DLL (Win7+/XP)",
            platform: Platform::Windows,
        },
        BinaryAsset {
            filename: "azul.i686.lib",
            description: "Windows 32-bit x86 import library",
            platform: Platform::Windows,
        },
        // macOS cross-arch
        BinaryAsset {
            filename: "libazul.x86_64.dylib",
            description: "macOS Intel x86_64 .dylib",
            platform: Platform::MacOS,
        },
        BinaryAsset {
            filename: "libazul.macos-x86_64.a",
            description: "macOS Intel x86_64 .a",
            platform: Platform::MacOS,
        },
        // rust9x (Win98/XP experimental)
        BinaryAsset {
            filename: "azul.rust9x.dll",
            description: "Windows 32-bit DLL (Win98/XP via rust9x)",
            platform: Platform::Windows,
        },
        BinaryAsset {
            filename: "azul.rust9x.lib",
            description: "Windows 32-bit import lib (Win98/XP via rust9x)",
            platform: Platform::Windows,
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
    /// Cross-architecture binaries (optional, only shown if files exist)
    pub cross_arch: Vec<AssetInfo>,
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
            AssetInfo::from_path(&version_dir.join("azul03.hpp"), "C++03 Header"),
            AssetInfo::from_path(&version_dir.join("azul11.hpp"), "C++11 Header"),
            AssetInfo::from_path(&version_dir.join("azul14.hpp"), "C++14 Header"),
            AssetInfo::from_path(&version_dir.join("azul17.hpp"), "C++17 Header"),
            AssetInfo::from_path(&version_dir.join("azul20.hpp"), "C++20 Header"),
            AssetInfo::from_path(&version_dir.join("azul23.hpp"), "C++23 Header"),
        ];

        // Cross-arch assets: only include those that actually exist on disk
        let cross_arch: Vec<AssetInfo> = BinaryAsset::CROSS_ARCH_ASSETS
            .iter()
            .map(|asset| AssetInfo::from_path(&version_dir.join(asset.filename), asset.description))
            .filter(|a| a.is_present)
            .collect();

        ReleaseAssets {
            windows,
            linux,
            macos,
            cross_arch,
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
    /// EXCEPTIONAL, opt-in (`--with-remill`): also build the ~130 MB libazul
    /// with the remill x86->WASM transpiler statically linked (web backend).
    /// Off by default — it's a ~30 min C++ build that needs
    /// `bash scripts/build_remill.sh` to populate third_party/remill-install.
    pub build_remill: bool,
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
        if self.build_remill {
            v.push("with-remill=true".to_string());
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
            build_remill: false,
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

            if arg == "--with-remill" {
                config.build_remill = true;
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

/// Where a release-dir file is sourced from.
#[derive(Debug, Clone, Copy, PartialEq)]
enum BindingSource {
    /// Generated by `azul-doc codegen all` under `target/codegen/`.
    Codegen,
    /// A hand-written per-language example under `examples/`. Used for the
    /// `hello-world.*` driver files (and a couple of build scaffolds) that the
    /// generator does not emit but the install steps still `curl`.
    Examples,
}

/// One file the per-language install steps `curl` out of `release/{version}/`,
/// together with where the deploy copies it FROM.
///
/// `dst` is the path **relative to `release/{version}/`** and must match the
/// curl target byte-for-byte (e.g. Haskell's nested `Azul/Types.hs`). `src` is
/// relative to either `target/codegen/` or `examples/` depending on `source`.
struct BindingFile {
    dst: &'static str,
    src: &'static str,
    source: BindingSource,
}

/// Every non-whitelist (`go`, `haskell`, `ada`, `pascal`, `zig`, `cobol`,
/// `fortran`, `perl`, `php`, `lisp`, `smalltalk`, `vb6`, `freebasic`,
/// `algol68`, `powershell`) binding + scaffolding file referenced by the
/// install instructions in api.json, mapped to its on-disk source.
///
/// The WHITELIST languages (c, cpp, rust, python, csharp, java, kotlin, lua,
/// ruby, node, ocaml) are intentionally absent: they download the native libs /
/// C·C++ headers (already laid down by the deploy) or generate their binding
/// locally, so nothing extra is copied for them.
///
/// `dst` values are flat because no two non-whitelist languages collide on a
/// filename — the only nested paths (`Azul/Types.hs`, `Azul/Internal/FFI.hs`)
/// are what the Haskell `curl -o src/Azul/...` steps genuinely request under
/// `release/{version}/`.
const BINDING_FILES: &[BindingFile] = &[
    // --- ada ---
    BindingFile { dst: "azul.ads", src: "azul.ads", source: BindingSource::Codegen },
    BindingFile { dst: "azul.adb", src: "azul.adb", source: BindingSource::Codegen },
    BindingFile { dst: "hello_world.gpr", src: "ada/hello_world.gpr", source: BindingSource::Examples },
    // --- algol68 ---
    BindingFile { dst: "azul.a68", src: "azul.a68", source: BindingSource::Codegen },
    BindingFile { dst: "hello-world.a68", src: "algol68/hello-world.a68", source: BindingSource::Examples },
    // --- cobol ---
    BindingFile { dst: "azul.cpy", src: "azul.cpy", source: BindingSource::Codegen },
    BindingFile { dst: "hello-world.cob", src: "cobol/hello-world.cob", source: BindingSource::Examples },
    // --- fortran (codegen emits Makefile.fortran; curl asks for `Makefile`) ---
    BindingFile { dst: "azul.f90", src: "azul.f90", source: BindingSource::Codegen },
    BindingFile { dst: "Makefile", src: "Makefile.fortran", source: BindingSource::Codegen },
    // --- freebasic ---
    BindingFile { dst: "azul.bi", src: "azul.bi", source: BindingSource::Codegen },
    BindingFile { dst: "hello-world.bas", src: "freebasic/hello-world.bas", source: BindingSource::Examples },
    // --- go ---
    BindingFile { dst: "azul.go", src: "go/azul.go", source: BindingSource::Codegen },
    BindingFile { dst: "types.go", src: "go/types.go", source: BindingSource::Codegen },
    BindingFile { dst: "functions.go", src: "go/functions.go", source: BindingSource::Codegen },
    BindingFile { dst: "wrappers.go", src: "go/wrappers.go", source: BindingSource::Codegen },
    BindingFile { dst: "go.mod", src: "go/go.mod", source: BindingSource::Codegen },
    // --- haskell (nested paths match the `curl -o src/Azul/...` steps) ---
    BindingFile { dst: "azul.cabal", src: "haskell/azul.cabal", source: BindingSource::Codegen },
    BindingFile { dst: "Azul.hs", src: "haskell/src/Azul.hs", source: BindingSource::Codegen },
    BindingFile { dst: "Azul/Types.hs", src: "haskell/src/Azul/Types.hs", source: BindingSource::Codegen },
    BindingFile { dst: "Azul/Internal/FFI.hs", src: "haskell/src/Azul/Internal/FFI.hs", source: BindingSource::Codegen },
    // --- lisp ---
    BindingFile { dst: "azul.asd", src: "azul.asd", source: BindingSource::Codegen },
    BindingFile { dst: "azul.lisp", src: "azul.lisp", source: BindingSource::Codegen },
    // --- pascal ---
    BindingFile { dst: "azul.pas", src: "azul.pas", source: BindingSource::Codegen },
    BindingFile { dst: "hello-world.pas", src: "pascal/hello-world.pas", source: BindingSource::Examples },
    // --- perl ---
    BindingFile { dst: "Azul.pm", src: "Azul.pm", source: BindingSource::Codegen },
    BindingFile { dst: "cpanfile", src: "cpanfile", source: BindingSource::Codegen },
    BindingFile { dst: "hello-world.pl", src: "perl/hello-world.pl", source: BindingSource::Examples },
    // --- php ---
    BindingFile { dst: "Azul.php", src: "Azul.php", source: BindingSource::Codegen },
    BindingFile { dst: "composer.json", src: "composer.json", source: BindingSource::Codegen },
    // --- powershell ---
    BindingFile { dst: "Azul.psd1", src: "Azul.psd1", source: BindingSource::Codegen },
    BindingFile { dst: "Azul.psm1", src: "Azul.psm1", source: BindingSource::Codegen },
    // --- smalltalk ---
    BindingFile { dst: "Azul.st", src: "Azul.st", source: BindingSource::Codegen },
    BindingFile { dst: "BaselineOfAzul.st", src: "BaselineOfAzul.st", source: BindingSource::Codegen },
    // --- vb6 (Azul.bas is the generated binding module; HelloWorld.* are the example app) ---
    BindingFile { dst: "Azul.bas", src: "vb6/Azul.bas", source: BindingSource::Codegen },
    BindingFile { dst: "HelloWorld.bas", src: "vb6/HelloWorld.bas", source: BindingSource::Examples },
    BindingFile { dst: "HelloWorld.vbp", src: "vb6/HelloWorld.vbp", source: BindingSource::Examples },
    // --- zig ---
    BindingFile { dst: "azul.zig", src: "azul.zig", source: BindingSource::Codegen },
    BindingFile { dst: "build.zig", src: "build.zig", source: BindingSource::Codegen },
];

/// Copy every non-whitelist per-language binding + scaffolding file that the
/// install instructions `curl` out of `release/{version}/` into `version_dir`,
/// using the exact names the steps expect.
///
/// Sources are the `azul-doc codegen all` outputs under `codegen_dir`
/// (`target/codegen/`) and, for the `hello-world.*` driver files the generator
/// doesn't emit, the per-language `examples/` tree. The deploy that already
/// lays down `azul.h` / `azul*.hpp` for the whitelist C/C++ tabs calls this so
/// the non-whitelist tabs stop 404-ing.
///
/// Missing sources are warnings, not errors: this is additive and must never
/// break a deploy (e.g. when `codegen all` hasn't been run, the binding files
/// simply stay absent rather than aborting the build).
pub fn copy_language_bindings(
    version: &str,
    version_dir: &Path,
    codegen_dir: &Path,
    examples_dir: &Path,
) -> Result<()> {
    println!("  Copying per-language bindings into release/{}/...", version);

    let mut copied = 0usize;
    let mut missing: Vec<String> = Vec::new();

    for bf in BINDING_FILES {
        let src = match bf.source {
            BindingSource::Codegen => codegen_dir.join(bf.src),
            BindingSource::Examples => examples_dir.join(bf.src),
        };

        if !src.exists() {
            missing.push(format!("{} (from {})", bf.dst, src.display()));
            continue;
        }

        let dst = version_dir.join(bf.dst);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&src, &dst)?;
        copied += 1;
    }

    println!(
        "  - Copied {} per-language binding file(s) into release/{}/",
        copied, version
    );

    if !missing.is_empty() {
        eprintln!(
            "  [WARN] {} per-language binding source(s) not found (run `cargo run -p azul-doc -- \
             codegen all` first?):\n    {}",
            missing.len(),
            missing.join("\n    ")
        );
    }

    Ok(())
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
    let mut added_files: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

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
    source_zip.start_file("include/cpp/azul03.hpp", options)?;
    source_zip.write_all(cpp_headers.cpp03.as_bytes())?;
    source_zip.start_file("include/cpp/azul11.hpp", options)?;
    source_zip.write_all(cpp_headers.cpp11.as_bytes())?;
    source_zip.start_file("include/cpp/azul14.hpp", options)?;
    source_zip.write_all(cpp_headers.cpp14.as_bytes())?;
    source_zip.start_file("include/cpp/azul17.hpp", options)?;
    source_zip.write_all(cpp_headers.cpp17.as_bytes())?;
    source_zip.start_file("include/cpp/azul20.hpp", options)?;
    source_zip.write_all(cpp_headers.cpp20.as_bytes())?;
    source_zip.start_file("include/cpp/azul23.hpp", options)?;
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
        println!("cargo:rustc-link-search={}", env!("AZ_LINK_PATH")); /* path to folder with azul.dll / libazul.so */
    }
}
"#,
    )?;

    println!("  - Created Git repository structure");
    Ok(())
}

/// Render an unconditional `<li>` link into the per-release artifact dir
/// (`https://azul.rs/release/{version}/{filename}`).
///
/// Unlike [`generate_asset_li`], this does NOT probe the filesystem: it always
/// emits a live link. The release page is built by the website-skeleton job
/// (with placeholder binaries) and is NOT regenerated after CI merges the real
/// artifacts — only file sizes are patched in place. So artifacts that aren't
/// placeholdered (Linux .deb/.rpm packages, the PDF guide, exotic-arch DLLs)
/// would never appear if we filtered on presence at generation time. For this
/// comprehensive "every artifact we ship" download index we therefore link
/// them unconditionally; a not-yet-built artifact 404s rather than vanishing.
fn release_link_li(version: &str, filename: &str, description: &str) -> String {
    format!(
        "<li><a href='https://azul.rs/release/{version}/{filename}'>{description} \
         ({filename})</a></li>",
        version = version,
        filename = filename,
        description = description
    )
}

/// Render an unconditional external `<li>` link (full URL, no release-dir prefix).
fn external_link_li(url: &str, label: &str) -> String {
    format!("<li><a href='{url}'>{label}</a></li>", url = url, label = label)
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
    let search_script = crate::docgen::get_search_init(crate::docgen::PageKind::Other);
    let releasenotes =
        comrak::markdown_to_html(&versiondata.notes.join("\r\n"), &comrak::Options::default());
    let git = &versiondata.git;
    let date = &versiondata.date;

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

    // Generate cross-architecture asset list (only present files)
    let cross_arch_assets: String = assets
        .cross_arch
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

    let _ = cross_arch_assets;

    // Exotic / cross-arch native libraries. Rendered unconditionally from the
    // static CROSS_ARCH_ASSETS list (NOT the present-only `assets.cross_arch`)
    // so every architecture we ship is linked on this comprehensive index even
    // though the skeleton build doesn't placeholder these files. Spans x86 →
    // riscv: i686/aarch64/armv7, ppc64/s390x/riscv64 (big-endian + RISC-V),
    // Windows i686, macOS Intel, and rust9x (Win98/XP).
    let cross_arch_assets: String = BinaryAsset::CROSS_ARCH_ASSETS
        .iter()
        .map(|a| release_link_li(version, a.filename, a.description))
        .collect::<Vec<_>>()
        .join("\n                ");
    let cross_arch_section = format!(
        "\n              <br/>\n              \
         <strong>Additional &amp; exotic architectures (experimental, x86 → \
         RISC-V):</strong>\n              <ul>\n                {}\n              </ul>",
        cross_arch_assets
    );

    // Generate API/examples links
    let api_json_link = generate_asset_li(version, &assets.api_json);
    let examples_zip_link = generate_asset_li(version, &assets.examples_zip);

    // ---- Linux packages (.deb / .rpm) ----------------------------------
    // nfpm conventional filenames: deb = `name_version_arch.deb`,
    // rpm = `name-version.arch.rpm`. The amd64 packages are built by
    // build_linux_packages; per-arch (arm64/ppc64/s390x/riscv64) are
    // experimental. nfpm maps the deb arch straight through (amd64/arm64/…)
    // and maps the rpm arch (amd64→x86_64, arm64→aarch64; ppc64/s390x/riscv64
    // pass through). The deploy copies them flat into release/{version}/.
    let package_links: String = [
        ("azul_{V}_amd64.deb", "Debian/Ubuntu package (x86-64)"),
        ("azul-{V}.x86_64.rpm", "RPM package (Fedora/RHEL/openSUSE, x86-64)"),
        ("azul_{V}_arm64.deb", "Debian/Ubuntu package (ARM64, experimental)"),
        ("azul-{V}.aarch64.rpm", "RPM package (ARM64, experimental)"),
        ("azul_{V}_ppc64.deb", "Debian/Ubuntu package (PowerPC64, experimental)"),
        ("azul-{V}.ppc64.rpm", "RPM package (PowerPC64, experimental)"),
        ("azul_{V}_s390x.deb", "Debian/Ubuntu package (s390x, experimental)"),
        ("azul-{V}.s390x.rpm", "RPM package (s390x, experimental)"),
        ("azul_{V}_riscv64.deb", "Debian/Ubuntu package (RISC-V 64, experimental)"),
        ("azul-{V}.riscv64.rpm", "RPM package (RISC-V 64, experimental)"),
    ]
    .iter()
    .map(|(fname, desc)| release_link_li(version, &fname.replace("{V}", version), desc))
    .collect::<Vec<_>>()
    .join("\n                ");

    // ---- PDF guide -------------------------------------------------------
    // The docs_pdf job uploads azul-documentation.pdf; the deploy lays it out
    // at release/{version}/azul-documentation.pdf.
    let pdf_link = release_link_li(
        version,
        "azul-documentation.pdf",
        "Full guide + API reference (PDF)",
    );

    // ---- Code coverage report -------------------------------------------
    // The `coverage` CI job runs scripts/coverage.sh (grcov + llvm-tools) over
    // the azul-css/azul-core/azul-layout test suites and uploads `coverage/`
    // (an HTML report whose entry point is index.html) as the `coverage-report`
    // artifact. The deploy lays it out at release/{version}/coverage/.
    let coverage_link = release_link_li(
        version,
        "coverage/index.html",
        "Code coverage report (grcov HTML, generated by CI)",
    );

    // ---- License files ---------------------------------------------------
    // generate_license_files writes the bundled third-party license text per
    // platform into release/{version}/. The project itself is MPL-2.0.
    const LICENSE_FILES: &[(&str, &str)] = &[
        ("LICENSE-LINUX.txt", "Bundled third-party licenses (Linux build)"),
        ("LICENSE-MACOS.txt", "Bundled third-party licenses (macOS build)"),
        ("LICENSE-WINDOWS.txt", "Bundled third-party licenses (Windows build)"),
    ];
    let license_links: String = LICENSE_FILES
        .iter()
        .map(|(fname, desc)| release_link_li(version, fname, desc))
        .collect::<Vec<_>>()
        .join("\n                ");

    // ---- Language bindings ----------------------------------------------
    // Per-language install instructions for the bindings with a solid working
    // hello-world (mirrors the frontpage install-tab whitelist). The frontpage
    // install panel is keyed by `?lang=` / the in-page selector, so we deep-link
    // to it; the examples zip below carries the full source for each.
    const BINDING_LANGS: &[(&str, &str)] = &[
        ("rust", "Rust"),
        ("python", "Python"),
        ("c", "C"),
        ("cpp", "C++"),
        ("csharp", "C# / .NET"),
        ("java", "Java"),
        ("kotlin", "Kotlin"),
        ("lua", "Lua"),
        ("ruby", "Ruby"),
        ("node", "Node.js"),
        ("ocaml", "OCaml"),
    ];
    let binding_links: String = BINDING_LANGS
        .iter()
        .map(|(lang, label)| {
            external_link_li(
                &format!("https://azul.rs/#install-{lang}"),
                &format!("{label} — install &amp; hello-world"),
            )
        })
        .collect::<Vec<_>>()
        .join("\n                ");

    // ---- Demos — download & run -----------------------------------------
    // Self-contained release binaries of the demo "goal apps" (Rust apps built
    // statically against azul). The build_demos CI job stages them as
    // <crate>-<os>[.exe] and the deploy lays them into release/{version}/demos/.
    // Each demo has up to three OS variants (linux/macos/windows). We link them
    // unconditionally (like the exotic-arch/package links) since the skeleton
    // build doesn't placeholder these — a not-yet-built one 404s rather than
    // vanishing. (crate, friendly name, one-line description.)
    const DEMO_APPS: &[(&str, &str, &str)] = &[
        ("azul-paint", "AzulPaint", "a small raster paint / drawing app"),
        ("azul-maps", "AzulMaps", "a slippy-map tile viewer"),
        ("azul-vault", "AzulVault", "an encrypted SQLite-backed password vault"),
        (
            "azul-spirit-level",
            "AzSpiritLevel",
            "a motion-sensor spirit level (accelerometer)",
        ),
        ("azul-gamepad", "AzGamepad", "a live gamepad / controller input tester"),
        ("azul-camera-app", "AzCamera", "a webcam capture & preview widget demo"),
        (
            "azul-screenshare-app",
            "AzScreenShare",
            "a screen-capture / screenshare widget demo",
        ),
        ("azul-video-app", "AzVideo", "a video playback widget demo"),
        (
            "azul-meet",
            "azul-meet",
            "a tiny video-call demo (UDP + audio sink + microphone)",
        ),
    ];
    // OS suffix → label + filename extension, matching the build_demos staging
    // names (azul-maps-linux, azul-maps-macos, azul-maps-windows.exe).
    const DEMO_OSES: &[(&str, &str, &str)] = &[
        ("linux", "Linux", ""),
        ("macos", "macOS", ""),
        ("windows", "Windows", ".exe"),
    ];
    let demo_links: String = DEMO_APPS
        .iter()
        .map(|(crate_name, friendly, desc)| {
            let os_links: String = DEMO_OSES
                .iter()
                .map(|(os_suffix, os_label, ext)| {
                    release_link_li(
                        version,
                        &format!("demos/{crate_name}-{os_suffix}{ext}"),
                        &format!("{friendly} ({os_label}) &mdash; {desc}"),
                    )
                })
                .collect::<Vec<_>>()
                .join("\n                ");
            os_links
        })
        .collect::<Vec<_>>()
        .join("\n                ");

    format!(
        "<!DOCTYPE html>
    <html lang='en'>
    <head>
        <title>Azul GUI v{version} (git {git}) - Release Notes</title>
        {common_head_tags}
    </head>

    <body>
      <div class='center'>
        <aside>
          <header>
            <a href='https://azul.rs/'>
              <img src='https://azul.rs/logo.svg'>
            </a>
          </header>
          {sidebar}
        </aside>

        <main>
          <h1>Azul v{version}</h1>
          <a href='https://github.com/fschutt/azul/commit/{git}' style='font-size:18px;'>(git {git})</a>
          <span style='font-size:14px;color:#666;'>released {date}</span>
          <style>
            main h1 {{ margin-bottom: none; }}
            main h2 {{ margin-top: 36px; margin-bottom: 4px; font-size: 24px; }}
            main h2:target {{ scroll-margin-top: 12px; }}
            ul {{ margin-left: 20px; margin-top: 20px; list-style-type: none; }}
            nav ul {{ margin: 0px; }}
            #releasenotes {{ margin-top: 20px; max-width: 700px; }}
            #releasenotes ul {{ list-style-type: initial; }} 
            #releasenotes ul li {{ margin-bottom: 2px; }} 
            #releasenotes p {{ margin-bottom: 10px; margin-top: 10px; }}
          </style>
          <div style='font-size:18px;'>
              
              <div id='releasenotes'>
              {releasenotes}
              </div>

              <br/>

              <p style='color:grey;font-size:15px;max-width:700px;'>Every artifact shipped with this release &mdash;
              native libraries from x86 to RISC-V, Linux packages, language bindings, docs, agentic files and source &mdash;
              is linked below. Jump to:
              <a href='#native-libraries'>native libs</a> &middot;
              <a href='#linux-packages'>packages</a> &middot;
              <a href='#demos'>demos</a> &middot;
              <a href='#language-bindings'>bindings</a> &middot;
              <a href='#docs-guide'>docs</a> &middot;
              <a href='#agentic'>agentic</a> &middot;
              <a href='#coverage'>coverage</a> &middot;
              <a href='#license'>license</a> &middot;
              <a href='#source'>source</a>.</p>

              <br/>

              <h2 id='native-libraries'>Native libraries</h2>
              <p style='color:grey;font-size:15px;'>Prebuilt dynamic (<code>.so</code>/<code>.dll</code>/<code>.dylib</code>)
              and static (<code>.a</code>/<code>.lib</code>) libraries + Python extension modules.</p>
              <ul>
                {windows_assets}
              </ul>
              <ul>
                {linux_assets}
              </ul>
              <ul>
                {macos_assets}
              </ul>
              {cross_arch_section}

              <br/>
              <strong>C / C++ headers:</strong>
              <ul>
                {c_header_link}
                {cpp_header_links}
              </ul>

              <br/>
              <h2 id='linux-packages'>Linux packages</h2>
              <p style='color:grey;font-size:15px;'>System packages (<code>.deb</code> / <code>.rpm</code>) per architecture.
              amd64 is solid; arm64/ppc64/s390x/riscv64 are experimental and ship only when CI builds them.</p>
              <ul>
                {package_links}
              </ul>

              <br/>
              <h2 id='demos'>Demos &mdash; download &amp; run</h2>
              <p style='color:grey;font-size:15px;max-width:700px;'>Self-contained demo apps built statically against azul &mdash;
              download one for your OS and run it directly, no install or separate library needed.
              These are best-effort builds; some demos need platform features (camera/video, motion sensors, audio)
              and may not ship for every OS.</p>
              <ul>
                {demo_links}
              </ul>

              <br/>
              <h2 id='language-bindings'>Language bindings</h2>
              <p style='color:grey;font-size:15px;'>Install instructions + a working hello-world for every supported
              language. Full source for each ships in the examples archive below.</p>
              <ul>
                {binding_links}
              </ul>

              <br/>
              <strong>Use Azul as a Rust dependency:</strong>
              <br/>
              <div style='padding:20px;background:rgb(236, 236, 236);margin-top: 20px;font-size:14px;'>
                  <p style='color:grey;font-family:monospace;'># Cargo.toml</p>
                  <p style='color:black;font-family:monospace;'>[dependencies.azul]</p>
                  <p style='color:black;font-family:monospace;'>git = \"https://azul.rs/{version}.git\"</p>
                  <br/>
                  <p style='color:grey;font-family:monospace;'># Dynamic linking:</p>
                  <p style='color:grey;font-family:monospace;'># export AZ_LINK_PATH=/path/to/azul.dll</p>
                  <p style='color:grey;font-family:monospace;'># features = ['link-dynamic']</p>
              </div>

              <br/>
              <h2 id='docs-guide'>Docs &amp; guide</h2>
              <ul>
                <li><a href='https://azul.rs/api/{version}.html'>API documentation for this release</a></li>
                <li><a href='https://azul.rs/guide'>Online guide</a></li>
                {pdf_link}
                {api_json_link}
                {examples_zip_link}
              </ul>

              <br/>
              <h2 id='agentic'>Agentic</h2>
              <p style='color:grey;font-size:15px;'>Machine-readable artifacts that make a coding agent ready to build azul apps.</p>
              <ul>
                <li><a href='https://azul.rs/skill.md'>AI agent skill (skill.md)</a> &mdash; install once to prime a coding agent</li>
                <li><a href='https://azul.rs/llms.txt'>llms.txt</a> &mdash; compact API + guide index for LLMs</li>
                <li><a href='https://azul.rs/llms-full.txt'>llms-full.txt</a> &mdash; full machine-readable index</li>
              </ul>

              <br/>
              <strong>Deploy a web app (pre-lifted WASM base image &mdash; experimental preview):</strong>
              <br/>
              <a href='https://azul.rs/guide/deploying-web'>Guide: deploying azul web apps</a>
              <div style='padding:20px;background:rgb(236, 236, 236);margin-top: 20px;font-size:14px;'>
                  <p style='color:grey;'>A <code>ghcr.io/fschutt/azul-web-base</code> base image with a pre-lifted
                  azul-library WASM cache &mdash; so your app only lifts its own callbacks, not the whole
                  library (seconds instead of minutes) &mdash; is <strong>in preparation</strong> and will be
                  published here once the web backend is stable. For now, see the guide above and build
                  <code>docker/web-base/Dockerfile</code> from the repo yourself.</p>
              </div>

              <br/>
              <h2 id='coverage'>Code coverage</h2>
              <p style='color:grey;font-size:15px;max-width:700px;'>HTML line/branch coverage report for the
              <code>azul-css</code>, <code>azul-core</code> and <code>azul-layout</code> test suites, produced by
              the <code>coverage</code> CI job (grcov + <code>llvm-tools</code>). Regenerated on every CI run; if the
              report has not been built for this release yet the link below 404s &mdash; the same report is always
              available as the <code>coverage-report</code> artifact on the
              <a href='https://github.com/fschutt/azul/actions/workflows/rust.yml'>latest CI run</a>.</p>
              <ul>
                {coverage_link}
              </ul>

              <br/>
              <h2 id='license'>License</h2>
              <p style='color:grey;font-size:15px;max-width:700px;'>Azul itself is licensed under the
              <strong>Mozilla Public License 2.0 (MPL-2.0)</strong>. The redistributable binaries above statically
              link a number of third-party crates; their combined license texts are bundled per platform below so a
              shipped binary carries the attributions it needs.</p>
              <ul>
                <li><a href='https://github.com/fschutt/azul/blob/{git}/LICENSE'>Azul project license (MPL-2.0)</a></li>
                {license_links}
              </ul>

              <br/>
              <h2 id='source'>Source</h2>
              <ul>
                <li><a href='https://github.com/fschutt/azul'>Git repository</a> &mdash; <code>git clone https://github.com/fschutt/azul</code></li>
                <li><a href='https://azul.rs/{version}.git'>Bare git repo for this release</a> &mdash; pin via <code>git = \"https://azul.rs/{version}.git\"</code></li>
                <li><a href='https://github.com/fschutt/azul/releases/tag/{version}'>GitHub release page</a></li>
                <li><a href='https://crates.io/crates/azul/{version}'>Crates.io</a></li>
                <li><a href='https://docs.rs/azul/{version}'>Docs.rs</a></li>
              </ul>
          </div>
        </main>
      </div>
      {prism_script}
      {search_script}
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
    let search_script = crate::docgen::get_search_init(crate::docgen::PageKind::Other);

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
    <h1>Releases</h1>
    <div>
      <ul>{version_items}</ul>
    </div>
  </main>
  </div>
  {prism_script}
  {search_script}
</body>
</html>"#,
        version_items = version_items,
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

    // Search panel assets. Embedded via include_str! so the binary stays
    // self-contained; we still write them out as separate static files so
    // the browser caches them independently of any HTML page.
    const AZUL_SEARCH_JS: &str = include_str!("../../templates/azul-search.js");
    const AZUL_SEARCH_CSS: &str = include_str!("../../templates/azul-search.css");
    fs::write(output_dir.join("azul-search.js"), AZUL_SEARCH_JS)?;
    fs::write(output_dir.join("azul-search.css"), AZUL_SEARCH_CSS)?;

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

/// Build the pagefind search index over the generated guide pages.
///
/// Drives the floating search panel on guide pages (the API search uses a
/// codegen-emitted index instead). Tries the `pagefind` binary first, then
/// `npx pagefind` so contributors who don't `cargo install pagefind` can
/// still rely on a Node toolchain.
///
/// Failures here are non-fatal: the JS adapter falls back to the API
/// search index if `/pagefind/pagefind.js` 404s, so guide pages stay
/// searchable either way.
pub fn run_pagefind(deploy_dir: &Path) -> Result<()> {
    use std::process::Command;

    let guide_dir = deploy_dir.join("guide");
    if !guide_dir.is_dir() {
        println!("  [INFO] No guide/ directory in deploy, skipping pagefind");
        return Ok(());
    }

    let site_arg = deploy_dir
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("deploy path is not utf-8"))?
        .to_string();

    // Pagefind args, shared between the direct binary and the npx fallback.
    // `--include-characters` keeps `_` `.` `-` as part of words so technical
    // identifiers (`fn_args`, `azul.rs`, `kebab-case`) match correctly.
    let pf_args: Vec<String> = vec![
        "--site".into(),
        site_arg,
        "--output-subdir".into(),
        "pagefind".into(),
        "--glob".into(),
        "guide/**/*.html".into(),
        "--include-characters".into(),
        "_.-".into(),
        "--quiet".into(),
    ];

    // Try the direct binary first; fall back to `npx` so contributors with
    // a Node toolchain don't need to install the binary separately.
    let mut attempts: Vec<(&str, Vec<String>)> = Vec::new();
    attempts.push(("pagefind", pf_args.clone()));
    let mut npx_args: Vec<String> = vec!["--yes".into(), "pagefind@latest".into()];
    npx_args.extend(pf_args.iter().cloned());
    attempts.push(("npx", npx_args));

    for (cmd, args) in &attempts {
        match Command::new(cmd).args(args).status() {
            Ok(status) if status.success() => {
                println!("  [OK] Generated pagefind index via `{}`", cmd);
                return Ok(());
            }
            Ok(status) => {
                eprintln!("  [WARN] `{}` exited {}, trying next launcher", cmd, status);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Try the next launcher silently — common case when the
                // pagefind binary isn't on PATH but Node is available.
            }
            Err(e) => {
                eprintln!("  [WARN] `{}` invocation failed: {}", cmd, e);
            }
        }
    }

    eprintln!(
        "  [INFO] pagefind not available; guide pages will fall back to the API search index. \
         Install via `cargo install pagefind` or rely on `npx pagefind`."
    );
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
pub fn generate_nfpm_yaml(version: &str, api_data: &ApiData, output_dir: &Path) -> Result<PathBuf> {
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

    // Global dependencies (shared by deb/rpm, but rpm uses different syntax in overrides)
    // These top-level fields apply to all packagers
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

    // RPM-specific configuration (only group is a valid field here)
    if !package.rpm.group.is_empty() {
        yaml.push_str("\nrpm:\n");
        yaml.push_str(&format!("  group: {}\n", package.rpm.group));
    }

    // Use overrides for packager-specific dependency versions
    let has_rpm_overrides = !package.rpm.depends.is_empty();
    if has_rpm_overrides {
        yaml.push_str("\noverrides:\n");
        yaml.push_str("  rpm:\n");
        if !package.rpm.depends.is_empty() {
            yaml.push_str("    depends:\n");
            for dep in &package.rpm.depends {
                yaml.push_str(&format!("      - {}\n", dep));
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
