use std::{env, fs, path::{Path, PathBuf}, process::Command};

fn main() {
    let target = env::var("TARGET").unwrap_or_default();

    check_generated_files();

    if env::var("CARGO_FEATURE_LINK_DYNAMIC").is_ok() {
        configure_dynamic_linking(&target);
    }

    #[cfg(target_os = "macos")]
    if env::var("CARGO_FEATURE_PYO3").is_ok() {
        println!("cargo:rustc-cdylib-link-arg=-undefined");
        println!("cargo:rustc-cdylib-link-arg=dynamic_lookup");
    }

    if target.contains("ios") {
        configure_ios();
    }
}

// ── Generated file checks ─────────────────────────────────────────────

fn check_generated_files() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let codegen_dir = Path::new(&manifest_dir).join("../target/codegen");

    let checks: &[(&str, &str)] = &[
        ("CARGO_FEATURE_BUILD_DLL",        "dll_api_build.rs"),
        ("CARGO_FEATURE_LINK_STATIC",      "dll_api_static.rs"),
        ("CARGO_FEATURE_LINK_DYNAMIC",     "dll_api_dynamic.rs"),
        ("CARGO_FEATURE_PYTHON_EXTENSION", "python_api.rs"),
    ];

    for &(feature_env, filename) in checks {
        if env::var(feature_env).is_ok() {
            let path = codegen_dir.join(filename);
            if !path.exists() {
                panic!(
                    "\nMissing generated file: {}\n\
                     Run: cargo run --release -p azul-doc -- codegen all\n",
                    filename,
                );
            }
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }

    // reexports.rs is needed by any link/build mode
    let needs_reexports = env::var("CARGO_FEATURE_LINK_STATIC").is_ok()
        || env::var("CARGO_FEATURE_LINK_DYNAMIC").is_ok()
        || env::var("CARGO_FEATURE_BUILD_DLL").is_ok();

    if needs_reexports {
        let path = codegen_dir.join("reexports.rs");
        if !path.exists() {
            panic!(
                "\nMissing generated file: reexports.rs\n\
                 Run: cargo run --release -p azul-doc -- codegen all\n",
            );
        }
        println!("cargo:rerun-if-changed={}", path.display());
    }
}

// ── Dynamic linking ───────────────────────────────────────────────────

fn lib_filename(target: &str) -> &'static str {
    if target.contains("apple") || target.contains("darwin") {
        "libazul.dylib"
    } else if target.contains("windows") {
        "azul.dll"
    } else {
        "libazul.so"
    }
}

fn static_lib_filename(target: &str) -> &'static str {
    if target.contains("windows") { "azul.lib" } else { "libazul.a" }
}

/// Look for a shared library or .framework in `dir`.
fn probe_dir(dir: &Path, target: &str) -> bool {
    dir.join(lib_filename(target)).exists()
        || dir.join("azul.framework").is_dir()
}

/// Set up link search paths for `link-dynamic`.
///
/// Search order:
/// 1. `AZUL_DLL_PATH` (comma-separated, absolute or workspace-relative)
/// 2. `target/release` then `target/debug` (only when AZUL_DLL_PATH unset)
///
/// If only a static library is found, links statically against it.
/// Copies the found dylib into the output directory so the binary can
/// find it at runtime without setting `DYLD_LIBRARY_PATH` / `LD_LIBRARY_PATH`.
fn configure_dynamic_linking(target: &str) {
    println!("cargo:rerun-if-env-changed=AZUL_DLL_PATH");

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = Path::new(&manifest_dir).parent().unwrap();

    // When build-dll or link-static is also active, dynamic linking is unused.
    if env::var("CARGO_FEATURE_BUILD_DLL").is_ok()
        || env::var("CARGO_FEATURE_LINK_STATIC").is_ok()
    {
        return;
    }

    // To avoid the cdylib output linking against itself ("can't link a dylib
    // with itself"), we copy the prebuilt dylib into OUT_DIR and point the
    // search path there instead of target/release/.
    let out_dir = env::var("OUT_DIR").unwrap_or_default();

    if target.contains("ios") {
        println!("cargo:warning=link-dynamic on iOS: consider link-static for production");
    } else if target.contains("android") {
        println!("cargo:warning=link-dynamic on Android: place libazul.so in jniLibs/");
    }

    // Search paths: (directory, is_system)
    // - Local paths: ship dylib with app, use rpath to @loader_path/$ORIGIN
    // - System paths: dylib is installed globally, no rpath needed
    let env_path = env::var("AZUL_DLL_PATH").unwrap_or_default();
    let mut dirs: Vec<(PathBuf, bool)> = Vec::new();

    // 1. AZUL_DLL_PATH (user override, comma-separated) — local
    if !env_path.is_empty() {
        for entry in env_path.split(',') {
            let entry = entry.trim();
            if entry.is_empty() { continue; }
            let p = Path::new(entry);
            let resolved = if p.is_absolute() { p.to_path_buf() } else { workspace_root.join(p) };
            dirs.push((resolved, false));
        }
    }

    // 2. Workspace target dirs — local
    dirs.push((workspace_root.join("target/release"), false));
    dirs.push((workspace_root.join("target/debug"), false));

    // 3. System library paths — system (no rpath, no copy)
    if target.contains("apple") {
        dirs.push((PathBuf::from("/opt/homebrew/lib"), true));
        dirs.push((PathBuf::from("/usr/local/lib"), true));
    } else if !target.contains("windows") {
        dirs.push((PathBuf::from("/usr/local/lib"), true));
        dirs.push((PathBuf::from("/usr/lib"), true));
    }

    // Where Cargo places the final binary (target/{debug,release}/)
    let bin_dir = Path::new(&out_dir)
        .ancestors()
        .find(|p| p.file_name().map(|n| n == "debug" || n == "release").unwrap_or(false))
        .map(|p| p.to_path_buf());

    // Try shared library
    for (dir, is_system) in &dirs {
        if !probe_dir(dir, target) {
            continue;
        }

        let src = dir.join(lib_filename(target));

        if *is_system {
            // System library: link directly, no rpath, no copy.
            // At runtime the system linker finds it in the standard paths.
            println!("cargo:rustc-link-search=native={}", dir.display());
            println!("cargo:rustc-link-lib=dylib=azul");
        } else {
            // Local library: copy to OUT_DIR to avoid cdylib self-link,
            // set rpath so the binary finds the dylib next to itself.
            let link_dir = PathBuf::from(&out_dir);
            let dst = link_dir.join(lib_filename(target));
            if src != dst && src.exists() {
                let _ = fs::copy(&src, &dst);
                if target.contains("apple") {
                    let _ = Command::new("install_name_tool")
                        .args(["-id", "@rpath/libazul.dylib"])
                        .arg(&dst)
                        .status();
                }
            }
            println!("cargo:rustc-link-search=native={}", link_dir.display());
            println!("cargo:rustc-link-lib=dylib=azul");

            if target.contains("apple") {
                println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path");
                println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path");
            } else if !target.contains("windows") {
                println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
            }

            // Copy next to the final binary for runtime discovery
            if let Some(ref bd) = bin_dir {
                let rt_dst = bd.join(lib_filename(target));
                if src != rt_dst && src.exists() {
                    let _ = fs::copy(&src, &rt_dst);
                }
            }
        }
        return;
    }

    // Fallback: static library
    let sname = static_lib_filename(target);
    for (dir, _) in &dirs {
        if dir.join(sname).exists() {
            println!(
                "cargo:warning=No {} found; linking statically against {} in {}",
                lib_filename(target), sname, dir.display(),
            );
            println!("cargo:rustc-link-search=native={}", dir.display());
            println!("cargo:rustc-link-lib=static=azul");
            return;
        }
    }

    // Nothing found
    let searched: Vec<_> = dirs.iter().map(|(p, _)| p.display().to_string()).collect();
    println!("cargo:warning=Could not find {} or {}", lib_filename(target), sname);
    println!("cargo:warning=Set AZUL_DLL_PATH to the directory containing the library");
    println!("cargo:warning=Searched: {}", searched.join(", "));
}

// ── iOS setup ─────────────────────────────────────────────────────────

fn configure_ios() {
    if env::var("AZUL_IOS_SETUP").unwrap_or_default() == "disable" {
        return;
    }

    check_tool("xcode-select", &["-p"], "Run 'xcode-select --install'");
    check_tool("ios-deploy", &["--version"], "Run 'brew install ios-deploy'");

    let project_root = env::var("CARGO_MANIFEST_DIR").unwrap();

    // Create .cargo/config.toml with iOS runner (only if not already set)
    let config_path = Path::new(&project_root).join(".cargo/config.toml");
    if !fs::read_to_string(&config_path)
        .unwrap_or_default()
        .contains("ios-runner.sh")
    {
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        fs::write(&config_path, "[target.aarch64-apple-ios]\nrunner = \"scripts/ios-runner.sh\"\n").unwrap();
    }

    // Create the runner script (only if missing)
    let runner_path = Path::new(&project_root).join("scripts/ios-runner.sh");
    if !runner_path.exists() {
        fs::create_dir_all(runner_path.parent().unwrap()).unwrap();
        fs::write(&runner_path, IOS_RUNNER_SCRIPT).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&runner_path, fs::Permissions::from_mode(0o755));
        }
    }
}

fn check_tool(name: &str, args: &[&str], install_hint: &str) {
    match Command::new(name).args(args).status() {
        Ok(s) if s.success() => {}
        _ => panic!("'{}' not found. {}", name, install_hint),
    }
}

const IOS_RUNNER_SCRIPT: &str = r#"#!/bin/bash
set -e
EXECUTABLE_PATH="$1"
APP_NAME=$(basename "$EXECUTABLE_PATH")
APP_BUNDLE_PATH="$(dirname "$EXECUTABLE_PATH")/${APP_NAME}.app"
echo "Deploying ${APP_BUNDLE_PATH}..."
ios-deploy --bundle "${APP_BUNDLE_PATH}" --justlaunch
"#;
