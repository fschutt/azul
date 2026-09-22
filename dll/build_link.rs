// dll/build_link.rs — how a consumer links the prebuilt libazul.
//
// SHARED SOURCE, two users:
//   * dll/build.rs `include!`s this file (the in-repo `azul-dll` crate);
//   * azul-doc `include_str!`s it into the build.rs of the pre-rendered
//     `azul` crate it ships in every release (azul-rust-<ver>.tar.gz and the
//     azul.rs/ui/cargo registry; doc/src/dllgen/deploy.rs).
// One file so the documented AZ_LINK_PATH contract, the search order and the
// static-vs-dynamic rule cannot drift between the two. Only `std` items may be
// used (`env`, `fs`, `Path`, `PathBuf`, `Command` are `use`d by both includers).
//
// THE CONTRACT (AZ_LINK_PATH is the only knob):
//   * AZ_LINK_PATH names a library FILE: that file is linked. A static archive
//     (`libazul.macos.a`, `libazul.a`, `azul.lib`) links statically, a shared
//     library (`libazul.x86_64.dylib`, `libazul.so`, `azul.dll`) dynamically.
//     Any file name works, including the release's platform-suffixed ones.
//   * AZ_LINK_PATH names DIRECTORIES (comma-separated), or is unset: those,
//     then the caller's own candidate dirs, then the system library dirs are
//     searched. A shared library found ANYWHERE wins; a static archive is
//     linked only when no shared library exists in any of them.

// ── Library file names ─────────────────────────────────────────────────

/// The canonical shared-library name: what `-lazul` and the dynamic loader
/// look for, and the name a found library is copied to.
fn lib_filename(target: &str) -> &'static str {
    if target.contains("apple") || target.contains("darwin") {
        "libazul.dylib"
    } else if target.contains("windows") {
        "azul.dll"
    } else {
        "libazul.so"
    }
}

/// The canonical static-archive name `-l static=azul` resolves to.
fn static_lib_filename(target: &str) -> &'static str {
    if target.contains("windows") {
        "azul.lib"
    } else {
        "libazul.a"
    }
}

/// The platform part of the release's file names for `target`
/// (doc/src/dllgen/deploy.rs): `libazul.x86_64.dylib` / `libazul.macos-x86_64.a`
/// for Intel macOS, `libazul.linux-<arch>.{so,a}` for non-x86_64 Linux.
/// `None` where the release uses the canonical name (arm64 macOS, x86_64 Linux,
/// Windows) or ships nothing (iOS, Android, other targets).
fn release_suffix(target: &str) -> Option<&'static str> {
    let arch = target.split('-').next().unwrap_or("");
    if target.contains("apple-darwin") {
        return (arch == "x86_64").then_some("x86_64");
    }
    if target.contains("linux") && !target.contains("android") {
        return match arch {
            "i686" | "i586" => Some("linux-i686"),
            "aarch64" => Some("linux-aarch64"),
            a if a.starts_with("armv7") => Some("linux-armv7"),
            "powerpc64" | "powerpc64le" => Some("linux-ppc64"),
            "s390x" => Some("linux-s390x"),
            a if a.starts_with("riscv64") => Some("linux-riscv64"),
            _ => None,
        };
    }
    None
}

/// Every name a SHARED libazul for `target` may have, most specific first.
fn shared_lib_names(target: &str) -> Vec<String> {
    let mut names = Vec::new();
    match release_suffix(target) {
        Some(s) if target.contains("apple") => names.push(format!("libazul.{s}.dylib")),
        Some(s) => names.push(format!("libazul.{s}.so")),
        None => {}
    }
    names.push(lib_filename(target).to_string());
    names
}

/// Every name a STATIC libazul for `target` may have, most specific first.
fn static_lib_names(target: &str) -> Vec<String> {
    let mut names = Vec::new();
    if target.contains("apple-darwin") {
        names.push(match release_suffix(target) {
            Some(s) => format!("libazul.macos-{s}.a"),
            None => "libazul.macos.a".to_string(),
        });
    } else if target.contains("linux") && !target.contains("android") {
        names.push(match release_suffix(target) {
            Some(s) => format!("libazul.{s}.a"),
            None => "libazul.linux.a".to_string(),
        });
    }
    names.push(static_lib_filename(target).to_string());
    names
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum LibKind {
    Shared,
    Static,
}

/// What a library FILE named in AZ_LINK_PATH is, from its name. Import
/// libraries (`azul.dll.lib`, `libazul.dll.a`) stand for the DLL next to them.
fn classify_lib_file(path: &Path) -> Option<LibKind> {
    let name = path.file_name()?.to_str()?.to_ascii_lowercase();
    if name.ends_with(".dll.lib") || name.ends_with(".dll.a") {
        Some(LibKind::Shared)
    } else if name.ends_with(".a") || name.ends_with(".lib") {
        Some(LibKind::Static)
    } else if name.ends_with(".dylib")
        || name.ends_with(".so")
        || name.contains(".so.")
        || name.ends_with(".dll")
    {
        Some(LibKind::Shared)
    } else {
        None
    }
}

// ── Linking ────────────────────────────────────────────────────────────

/// Set up link search paths for the prebuilt libazul (see THE CONTRACT above).
///
/// Search order when AZ_LINK_PATH names no file:
/// 1. the AZ_LINK_PATH directories (comma-separated; relative entries resolve
///    against `base_dir`)
/// 2. `local_dirs`, in order — the caller's own candidates (the workspace's
///    `target/release` + `target/debug` for the in-repo crate; the crate
///    directory and its parent for the pre-rendered `azul` crate)
/// 3. the system library directories (brew / apt / dnf installs)
///
/// A found shared library is copied into the output directory so the binary
/// finds it at runtime without setting `DYLD_LIBRARY_PATH` / `LD_LIBRARY_PATH`.
fn configure_dynamic_linking(target: &str, base_dir: &Path, local_dirs: &[PathBuf]) {
    println!("cargo:rerun-if-env-changed=AZ_LINK_PATH");

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

    // 1. AZ_LINK_PATH (comma-separated): the first entry that is a library
    //    FILE is linked as-is; directories join the search.
    let env_path = env::var("AZ_LINK_PATH").unwrap_or_default();
    let mut dirs: Vec<(PathBuf, bool)> = Vec::new();
    let mut named_file: Option<(PathBuf, LibKind)> = None;
    for entry in env_path.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        let p = Path::new(entry);
        let resolved = if p.is_absolute() {
            p.to_path_buf()
        } else {
            base_dir.join(p)
        };
        if resolved.is_file() {
            match classify_lib_file(&resolved) {
                Some(kind) if named_file.is_none() => named_file = Some((resolved, kind)),
                Some(_) => println!(
                    "cargo:warning=AZ_LINK_PATH names more than one library file; ignoring {}",
                    resolved.display()
                ),
                None => println!(
                    "cargo:warning=AZ_LINK_PATH entry {} is not a library file \
                     (.dylib/.so/.dll/.a/.lib); ignoring it",
                    resolved.display()
                ),
            }
        } else {
            dirs.push((resolved, false));
        }
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

    if let Some((file, kind)) = named_file {
        match kind {
            LibKind::Static => link_static(target, &file, &out_dir),
            LibKind::Shared => link_shared(target, &file, false, &out_dir, bin_dir.as_deref()),
        }
        return;
    }

    // 2. The caller's own candidate dirs — local
    for d in local_dirs {
        dirs.push((d.clone(), false));
    }

    // 3. System library paths — system (no rpath, no copy). Package managers
    //    install the canonical names only.
    if target.contains("apple") {
        dirs.push((PathBuf::from("/opt/homebrew/lib"), true));
        dirs.push((PathBuf::from("/usr/local/lib"), true));
    } else if !target.contains("windows") {
        dirs.push((PathBuf::from("/usr/local/lib"), true));
        dirs.push((PathBuf::from("/usr/lib"), true));
    }

    let shared_names = shared_lib_names(target);
    let static_names = static_lib_names(target);
    let find = |dir: &Path, is_system: bool, names: &[String], canonical: &str| {
        if is_system {
            let p = dir.join(canonical);
            return p.is_file().then_some(p);
        }
        names.iter().map(|n| dir.join(n)).find(|p| p.is_file())
    };

    // A shared library anywhere wins …
    for (dir, is_system) in &dirs {
        if let Some(src) = find(dir, *is_system, &shared_names, lib_filename(target)) {
            link_shared(target, &src, *is_system, &out_dir, bin_dir.as_deref());
            return;
        }
    }
    // … a static archive is linked only when there is no shared library.
    for (dir, is_system) in &dirs {
        if let Some(src) = find(dir, *is_system, &static_names, static_lib_filename(target)) {
            link_static(target, &src, &out_dir);
            return;
        }
    }

    // Nothing found
    let mut tried = shared_names;
    tried.extend(static_names);
    let searched: Vec<_> = dirs.iter().map(|(p, _)| p.display().to_string()).collect();
    println!("cargo:warning=Could not find libazul (tried {})", tried.join(", "));
    println!("cargo:warning=Set AZ_LINK_PATH to the library file or the directory containing it");
    println!("cargo:warning=Searched: {}", searched.join(", "));
}

/// Link a static archive. It is exposed to the linker from a directory of its
/// own under OUT_DIR, under the name `-l static=azul` resolves to: that makes a
/// platform-suffixed archive (`libazul.macos.a`) linkable, and keeps a shared
/// library sitting next to it from being picked instead (ld64 prefers a
/// `.dylib` over a `.a` in the same directory).
fn link_static(target: &str, src: &Path, out_dir: &str) {
    let link_dir = Path::new(out_dir).join("azul-static");
    let _ = fs::create_dir_all(&link_dir);
    let dst = link_dir.join(static_lib_filename(target));
    let _ = fs::remove_file(&dst);
    // A symlink avoids copying a multi-hundred-MB archive on every build.
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(src, &dst).is_ok();
    #[cfg(not(unix))]
    let linked = false;
    if !linked {
        if let Err(e) = fs::copy(src, &dst) {
            panic!("could not stage {} for linking: {e}", src.display());
        }
    }
    println!("cargo:rerun-if-changed={}", src.display());
    println!("cargo:warning=Linking against {} [static]", src.display());
    println!("cargo:rustc-link-search=native={}", link_dir.display());
    println!("cargo:rustc-link-lib=static=azul");
    emit_static_system_deps(target);
}

/// Link a shared library found at `src` (any file name; for MSVC, the DLL or
/// its import library).
fn link_shared(target: &str, src: &Path, is_system: bool, out_dir: &str, bin_dir: Option<&Path>) {
    // An import library stands for the DLL next to it.
    let src = if target.contains("windows")
        && src.extension().map_or(false, |e| e == "lib" || e == "a")
    {
        src.parent().unwrap_or(Path::new(".")).join(lib_filename(target))
    } else {
        src.to_path_buf()
    };
    let dir = src.parent().unwrap_or(Path::new(".")).to_path_buf();

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

    // Inform the user which library we're linking against
    let dir_str = dir.display().to_string();
    let kind = if is_system {
        "system"
    } else if dir_str.contains("/debug") || dir_str.ends_with("/debug") {
        "local (debug)"
    } else {
        "local"
    };
    println!("cargo:warning=Linking against {} [{}]", src.display(), kind);

    if dir_str.contains("/debug") && !dir_str.contains("/release") {
        println!(
            "cargo:warning=Note: linking against debug build of libazul — \
            consider building with: cargo build --release -p azul-dll --features build-dll"
        );
    }

    if is_system {
        // System library: link directly, no rpath, no copy.
        // At runtime the system linker finds it in the standard paths.
        println!("cargo:rustc-link-search=native={}", dir.display());
        println!("cargo:rustc-link-lib=dylib={dylib_link_name}");
        return;
    }

    refuse_to_overwrite_the_linked_library(&dir, out_dir, target);

    // Local library: copy to OUT_DIR under the canonical name (so a
    // platform-suffixed file like libazul.x86_64.dylib links as -lazul, and to
    // avoid cdylib self-link), set rpath so the binary finds the dylib next to
    // itself.
    let link_dir = PathBuf::from(out_dir);
    let dst = link_dir.join(lib_filename(target));
    // A static archive staged by an earlier static build must not shadow it.
    let _ = fs::remove_dir_all(link_dir.join("azul-static"));
    // MSVC links the import library, so it must sit next to the copied DLL.
    if target.contains("windows-msvc") {
        let import_lib = dir.join("azul.dll.lib");
        if import_lib.exists() {
            let _ = fs::copy(&import_lib, link_dir.join("azul.dll.lib"));
        }
    }
    if src != dst && src.exists() {
        let _ = fs::copy(&src, &dst);
        if target.contains("apple") {
            // Set the install name to THIS copy's absolute path, so the
            // binary that links against it loads exactly this file.
            //
            // It used to be `@executable_path/libazul.dylib` ("next to
            // the binary"), and that resolved to target/<profile>/deps/
            // libazul.dylib for a test binary — where cargo ALSO writes
            // the azul-dll crate's own cdylib output. Under link-dynamic
            // that cdylib is an empty 16 KB stub (nothing is exported),
            // it is written after the copy below, and dyld bound every
            // Az* import to null: `cargo test -p AzWriter` died with
            // SIGSEGV at address 0 in the first FFI call. OUT_DIR is
            // cargo's, nothing overwrites it. Dev builds only — shipped
            // apps use the pre-rendered `azul` crate (rlib, no stub) or
            // link-static.
            let _ = Command::new("install_name_tool")
                .args(["-id", &dst.display().to_string()])
                .arg(&dst)
                .status();
        }
    }
    println!("cargo:rustc-link-search=native={}", link_dir.display());
    println!("cargo:rustc-link-lib=dylib={dylib_link_name}");

    // Copy the dylib to common output directories so the binary
    // finds it at runtime regardless of where cargo places it.
    if let Some(bd) = bin_dir {
        let lib_name = lib_filename(target);
        // target/{release,debug}/
        let dst1 = bd.join(lib_name);
        if src != dst1 && src.exists() {
            let _ = fs::copy(&src, &dst1);
        }
        // target/{release,debug}/examples/ (may not exist on a first build)
        let examples_dir = bd.join("examples");
        if fs::create_dir_all(&examples_dir).is_ok() {
            let _ = fs::copy(&src, examples_dir.join(lib_name));
        }
        // target/{release,debug}/deps/
        let deps_dir = bd.join("deps");
        if deps_dir.is_dir() {
            let _ = fs::copy(&src, deps_dir.join(lib_name));
        }
    }
}

/// Refuses a link-dynamic build whose stub cdylib would overwrite the library it links.
fn refuse_to_overwrite_the_linked_library(found_in: &Path, out_dir: &str, target: &str) {
    // Only azul-dll is a cdylib; the pre-rendered `azul` crate (same file) is an rlib.
    if env::var("CARGO_PKG_NAME").map_or(true, |name| name != "azul-dll") {
        return;
    }
    // OUT_DIR is <target-dir>/<profile>/build/<crate>-<hash>/out.
    let Some(profile_dir) = Path::new(out_dir).ancestors().nth(3) else {
        return;
    };
    let same_dir = match (fs::canonicalize(found_in), fs::canonicalize(profile_dir)) {
        (Ok(a), Ok(b)) => a == b,
        _ => found_in == profile_dir,
    };
    if !same_dir {
        return;
    }
    panic!(
        "\n\nrefusing to build azul-dll for link-dynamic into {dir}:\n\
         this build would overwrite the library it links against ({lib}) with an\n\
         empty stub (azul-dll is also a cdylib, and cargo copies it into that same\n\
         directory).\n\n\
         Build the consumer into a separate target directory; it still finds the\n\
         prebuilt library in {dir}:\n\n    \
         CARGO_TARGET_DIR=target/consumer cargo build ...\n\n\
         or point AZ_LINK_PATH at a copy of the library outside the target directory.\n\
         (`cargo check` is refused too: a build script cannot tell it from a build.)\n",
        dir = found_in.display(),
        lib = lib_filename(target),
    );
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
            // netdev/n0-dns-resolver under the iroh transport: SCNetworkInterface*, SCDynamicStore*
            "SystemConfiguration",
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
        // libc-adjacent ones are needed at link time — plus the C++ runtime:
        // the archive carries C++ objects (the Vulkan memory allocator wrapper
        // behind the video path, compiled with exceptions), and their
        // `__gxx_personality_v0` reference is satisfied by libstdc++. As a
        // cargo dependency the `cc` crate emits that link for us; a prebuilt
        // `.a` has no such metadata, so every Linux demo died in rust-lld with
        // "undefined symbol: __gxx_personality_v0 ... wrapper.cpp" (PR 469 run
        // 34152799415). libazul.so itself already NEEDs libstdc++, so this adds
        // no new runtime requirement.
        for lib in ["dl", "pthread", "m", "stdc++"] {
            println!("cargo:rustc-link-lib=dylib={lib}");
        }
    }
}
