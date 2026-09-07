// dll/build_link.rs — how a consumer links the prebuilt libazul.
//
// SHARED SOURCE, two users:
//   * dll/build.rs `include!`s this file (the in-repo `azul-dll` crate);
//   * azul-doc `include_str!`s it into the build.rs of the pre-rendered
//     `azul` crate it ships in every release (azul-rust-<ver>.tar.gz and the
//     azul.rs/ui/cargo registry; doc/src/dllgen/deploy.rs).
// One file so the documented AZ_LINK_PATH contract, the search order and the
// static fallback cannot drift between the two. Only `std` items may be used
// (`env`, `fs`, `Path`, `PathBuf`, `Command` are `use`d by both includers).

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
    if target.contains("windows") {
        "azul.lib"
    } else {
        "libazul.a"
    }
}

/// Look for a shared library or .framework in `dir`.
fn probe_dir(dir: &Path, target: &str) -> bool {
    dir.join(lib_filename(target)).exists() || dir.join("azul.framework").is_dir()
}

/// Set up link search paths for `link-dynamic`.
///
/// Search order:
/// 1. `AZ_LINK_PATH` / `AZ_DLL_PATH` (comma-separated; relative entries resolve
///    against `base_dir`)
/// 2. `local_dirs`, in order — the caller's own candidates (the workspace's
///    `target/release` + `target/debug` for the in-repo crate; the crate
///    directory and its parent for the pre-rendered `azul` crate)
/// 3. the system library directories (brew / apt / dnf installs)
///
/// If only a static library is found, links statically against it.
/// Copies the found dylib into the output directory so the binary can
/// find it at runtime without setting `DYLD_LIBRARY_PATH` / `LD_LIBRARY_PATH`.
fn configure_dynamic_linking(target: &str, base_dir: &Path, local_dirs: &[PathBuf]) {
    println!("cargo:rerun-if-env-changed=AZ_DLL_PATH");
    println!("cargo:rerun-if-env-changed=AZ_LINK_PATH");
    println!("cargo:rerun-if-env-changed=AZ_LINK_STATIC");

    // When cabi_internal is also active, dynamic linking is unused
    // (internal bindings take precedence over external declarations).
    if env::var("CARGO_FEATURE_CABI_INTERNAL").is_ok() {
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
    // AZ_LINK_PATH is the documented name (guide/README/release page);
    // AZ_DLL_PATH predates it. Accept both, documented name first.
    let env_path = env::var("AZ_LINK_PATH")
        .or_else(|_| env::var("AZ_DLL_PATH"))
        .unwrap_or_default();
    let mut dirs: Vec<(PathBuf, bool)> = Vec::new();

    // 1. AZ_LINK_PATH / AZ_DLL_PATH (user override, comma-separated) — local.
    // Entries may point at the dylib FILE itself; use its parent dir then.
    if !env_path.is_empty() {
        for entry in env_path.split(',') {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }
            let p = Path::new(entry);
            let resolved = if p.is_absolute() {
                p.to_path_buf()
            } else {
                base_dir.join(p)
            };
            let resolved = if resolved.is_file() {
                resolved
                    .parent()
                    .map(|d| d.to_path_buf())
                    .unwrap_or(resolved)
            } else {
                resolved
            };
            dirs.push((resolved, false));
        }
    }

    // 2. The caller's own candidate dirs — local
    for d in local_dirs {
        dirs.push((d.clone(), false));
    }

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
        .find(|p| {
            p.file_name()
                .map(|n| n == "debug" || n == "release")
                .unwrap_or(false)
        })
        .map(|p| p.to_path_buf());

    // Force-static override: demos link the prebuilt libazul.a into a SINGLE
    // self-contained binary. The dylib search below also probes
    // target/{release,debug} and /usr/lib, so a stray libazul.so there would be
    // linked DYNAMICALLY instead. AZ_LINK_STATIC=1 skips dylib discovery so we
    // fall straight through to the static-lib fallback.
    let force_static = env::var("AZ_LINK_STATIC")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false);

    // MSVC resolves `-l dylib=NAME` as NAME.lib. The release ships the IMPORT
    // library as azul.dll.lib (rustc's own name for it) next to azul.dll, and
    // `azul.lib` is the multi-hundred-MB STATIC archive — so asking for
    // `azul` there linked a downloaded dylib statically, or failed outright
    // for a user who only had the DLL + import lib. `azul.dll` → azul.dll.lib.
    // GNU ld searches libazul.dll.a / azul.dll itself, so `azul` stays.
    let dylib_link_name = if target.contains("windows-msvc") {
        "azul.dll"
    } else {
        "azul"
    };

    // Try shared library (unless static linking is forced)
    for (dir, is_system) in dirs.iter().filter(|_| !force_static) {
        if !probe_dir(dir, target) {
            continue;
        }

        let src = dir.join(lib_filename(target));

        // Inform the user which library we're linking against
        let dir_str = dir.display().to_string();
        let kind = if *is_system {
            "system"
        } else if dir_str.contains("/debug") || dir_str.ends_with("/debug") {
            "local (debug)"
        } else {
            "local"
        };
        println!(
            "cargo:warning=Linking against {} [{}]: {}",
            lib_filename(target),
            kind,
            dir_str
        );

        if dir_str.contains("/debug") && !dir_str.contains("/release") {
            println!(
                "cargo:warning=Note: linking against debug build of libazul — \
                consider building with: cargo build --release -p azul-dll --features build-dll"
            );
        }

        if *is_system {
            // System library: link directly, no rpath, no copy.
            // At runtime the system linker finds it in the standard paths.
            println!("cargo:rustc-link-search=native={}", dir.display());
            println!("cargo:rustc-link-lib=dylib={dylib_link_name}");
        } else {
            // Local library: copy to OUT_DIR to avoid cdylib self-link,
            // set rpath so the binary finds the dylib next to itself.
            let link_dir = PathBuf::from(&out_dir);
            let dst = link_dir.join(lib_filename(target));
            if src != dst && src.exists() {
                let _ = fs::copy(&src, &dst);
                if target.contains("apple") {
                    // Set install_name so macOS finds the dylib next to the
                    // binary at runtime, and also so ld doesn't think it's
                    // the same dylib being built.
                    let _ = Command::new("install_name_tool")
                        .args(["-id", "@executable_path/libazul.dylib"])
                        .arg(&dst)
                        .status();
                }
            }
            println!("cargo:rustc-link-search=native={}", link_dir.display());
            println!("cargo:rustc-link-lib=dylib={dylib_link_name}");

            // Copy the dylib to common output directories so the binary
            // finds it at runtime regardless of where cargo places it.
            if let Some(ref bd) = bin_dir {
                let lib_name = lib_filename(target);
                // target/{release,debug}/
                let dst1 = bd.join(lib_name);
                if src != dst1 && src.exists() {
                    let _ = fs::copy(&src, &dst1);
                }
                // target/{release,debug}/examples/
                let examples_dir = bd.join("examples");
                if examples_dir.is_dir() {
                    let _ = fs::copy(&src, examples_dir.join(lib_name));
                }
                // target/{release,debug}/deps/
                let deps_dir = bd.join("deps");
                if deps_dir.is_dir() {
                    let _ = fs::copy(&src, deps_dir.join(lib_name));
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
                "cargo:warning=Linking against {} [static fallback]: {}",
                sname,
                dir.display(),
            );
            println!("cargo:rustc-link-search=native={}", dir.display());
            println!("cargo:rustc-link-lib=static=azul");
            emit_static_system_deps(target);
            return;
        }
    }

    // Nothing found
    let searched: Vec<_> = dirs.iter().map(|(p, _)| p.display().to_string()).collect();
    println!(
        "cargo:warning=Could not find {} or {}",
        lib_filename(target),
        sname
    );
    println!("cargo:warning=Set AZ_LINK_PATH to the directory containing the library");
    println!("cargo:warning=Searched: {}", searched.join(", "));
}

/// System libraries a consumer must link ALONGSIDE the prebuilt `libazul.a`.
///
/// A static archive carries no link metadata. When azul is consumed as a Rust
/// crate, cargo propagates every `#[link(kind = "framework")]` in its
/// dependency graph to the final binary; when it is consumed as a prebuilt
/// `.a` — which is exactly what the demo binaries do, via the static fallback
/// above — none of that graph exists and the frameworks must be named here.
///
/// Skipping this is not a link error on every target, which is why it survived:
/// a macOS **cdylib** resolves undefined symbols at load time, so
/// `cargo build -p azul-dll` succeeds. An **executable** cannot, so all ten
/// demos died with `Undefined symbols for architecture arm64` —
/// `_AVCaptureDeviceTypeBuiltInWideAngleCamera`, `_AVMediaTypeVideo`,
/// `_CFArrayCreate` … — while CI reported the job green and the release served
/// binaries from 26 days earlier.
fn emit_static_system_deps(target: &str) {
    if target.contains("apple") {
        // Camera (AVCaptureDevice*/AVMediaType*), the CoreMedia/CoreVideo
        // sample-buffer path, CF types used throughout, and Security for the
        // keyring backend.
        for fw in [
            // vImage frame resampler (`vImageScale_ARGB8888`,
            // desktop/extra/resample/macos.rs)
            "Accelerate",
            "AVFoundation",
            "CoreMedia",
            "CoreVideo",
            "CoreFoundation",
            "CoreGraphics",
            "CoreText",
            "Security",
            "IOKit",
            "AppKit",
            "Foundation",
            "QuartzCore",
            "Metal",
        ] {
            println!("cargo:rustc-link-lib=framework={fw}");
        }
        println!("cargo:rustc-link-lib=dylib=objc");
    } else if target.contains("windows") {
        // A prebuilt .a carries no #[link] metadata, so EVERY system DLL any
        // code inside azul.lib touches must be named here — discovered one
        // LNK2019 wave at a time:
        //   advapi32: CredReadW/CredWriteW/... (keyring / azul-vault)
        //   shcore:   GetDpiForMonitor (per-monitor DPI, desktop::display)
        //   bcrypt:   BCryptGenRandom (getrandom — in azul's graph, not the
        //             consumer's, so the consumer's std does not pull it)
        // The rest are cheap universally-present DLLs listed preemptively so
        // the next feature (audio timers, sockets, COM automation) does not
        // repeat this cycle.
        for lib in [
            "user32", "gdi32", "shell32", "ole32", "oleaut32", "opengl32", "dwmapi", "advapi32",
            "shcore", "bcrypt", "winmm", "ws2_32", "userenv", "ntdll",
        ] {
            println!("cargo:rustc-link-lib=dylib={lib}");
        }
    } else if target.contains("linux") {
        // The X11/Wayland/EGL entry points are dlopen'd at runtime, so only the
        // libc-adjacent ones are needed at link time.
        for lib in ["dl", "pthread", "m"] {
            println!("cargo:rustc-link-lib=dylib={lib}");
        }
    }
}
