use std::{env, fs, path::{Path, PathBuf}, process::Command};

fn main() {
    let target = env::var("TARGET").unwrap_or_default();

    check_generated_files();
    compress_debugger_assets();

    if env::var("CARGO_FEATURE_CABI_EXTERNAL").is_ok() {
        configure_dynamic_linking(&target);
    }

    #[cfg(feature = "web-transpiler-static")]
    if env::var("CARGO_FEATURE_WEB_TRANSPILER_STATIC").is_ok() {
        build_in_process_remill(&target);
    }

    #[cfg(target_os = "macos")]
    if env::var("CARGO_FEATURE_PYO3").is_ok() {
        println!("cargo:rustc-cdylib-link-arg=-undefined");
        println!("cargo:rustc-cdylib-link-arg=dynamic_lookup");
    }

    if target.contains("ios") {
        configure_ios();
    }
    if target.contains("android") {
        configure_android();
    }
}

// ── Android setup ─────────────────────────────────────────────────────

fn configure_android() {
    // Link the two system libraries every Android cdylib needs.
    println!("cargo:rustc-link-lib=android");
    println!("cargo:rustc-link-lib=log");

    if env::var("ANDROID_NDK_HOME").is_err() && env::var("ANDROID_HOME").is_err() {
        println!(
            "cargo:warning=ANDROID_NDK_HOME / ANDROID_HOME not set. Install with: \
             brew install --cask android-commandlinetools && \
             sdkmanager 'ndk;27.0.12077973'"
        );
    }
}

// ── M8.9 in-process remill + LLVM + LLD ────────────────────────────────

/// Compile dll/src/web/cpp/azul_remill.cpp and emit the link line
/// pulling in remill + LLVM + LLD static libs. Active only with the
/// `web-transpiler-static` feature — the `cc` crate is gated on it.
#[cfg(feature = "web-transpiler-static")]
fn build_in_process_remill(target: &str) {
    let is_apple = target.contains("apple") || target.contains("darwin");
    let is_linux = target.contains("linux") && !target.contains("apple");
    if !is_apple && !is_linux {
        // Windows is M8.10 work — needs MSVC-built remill + LLVM, no
        // ready cxx-common bundle.
        println!("cargo:warning=web-transpiler-static is macOS + Linux only; skipping for {}", target);
        return;
    }

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let workspace_root = Path::new(&manifest_dir).parent().unwrap();

    let remill_install = workspace_root.join("third_party/remill-install/install");
    let remill_build = workspace_root.join("third_party/remill-install/build/remill");
    // vcpkg cxx-common bundle path differs per host OS / arch. The
    // bundle on disk is the one matching the build host — scripts/
    // build_remill.sh picks it up at bootstrap time.
    let vcpkg_base = if is_apple {
        workspace_root.join(
            "third_party/cxx-common/vcpkg_macos-13_llvm-17-liftingbits-llvm_xcode-15.0_arm64\
             /installed/arm64-osx-rel",
        )
    } else {
        // Linux x86_64: vcpkg_ubuntu-22.04_llvm-17-liftingbits-llvm_x64-linux.
        // Linux aarch64: vcpkg_ubuntu-22.04_llvm-17-liftingbits-llvm_arm64-linux.
        // (Bundles are produced by trail-of-bits CI per their cxx-common repo.)
        let arch = if target.starts_with("aarch64") {
            "arm64-linux"
        } else {
            "x64-linux"
        };
        let bundle = format!(
            "third_party/cxx-common/vcpkg_ubuntu-22.04_llvm-17-liftingbits-llvm_{arch}\
             /installed/{arch}-rel"
        );
        workspace_root.join(bundle)
    };

    for p in [&remill_install, &remill_build, &vcpkg_base] {
        if !p.exists() {
            panic!(
                "web-transpiler-static requires {} — run `bash scripts/build_remill.sh` \
                 from the workspace root to bootstrap",
                p.display()
            );
        }
    }

    let semantics_dir = remill_install.join("share/remill/17/semantics");
    let build_sem_dir = remill_build.join("lib/Arch");
    let remill_inc = remill_install.join("include");
    let vcpkg_inc = vcpkg_base.join("include");
    let vcpkg_lib = vcpkg_base.join("lib");

    println!("cargo:rerun-if-changed=src/web/cpp/azul_remill.cpp");
    println!("cargo:rerun-if-changed=src/web/cpp/azul_remill.h");

    let mut cc_build = cc::Build::new();
    cc_build
        .cpp(true)
        .file("src/web/cpp/azul_remill.cpp")
        .include("src/web/cpp")
        .include(&remill_inc)
        .include(&vcpkg_inc)
        .flag("-std=c++17")
        .flag("-fPIC");
    if is_apple {
        // macOS SDK path — needed for libc++ headers (cassert, etc.).
        // cc-rs sets --target= but doesn't auto-include the libc++
        // headers from the CommandLineTools SDK.
        let sdk_path = "/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk";
        let libcxx_dir = format!("{}/usr/include/c++/v1", sdk_path);
        cc_build
            .flag(&format!("-isysroot{}", sdk_path))
            .flag(&format!("-isystem{}", libcxx_dir));
    } else {
        // Linux: cc-rs auto-detects libstdc++ headers from the
        // installed gcc/clang; no explicit -isystem needed in the
        // common case. The vcpkg LLVM bundle uses libstdc++ on Linux.
        cc_build.flag("-fno-rtti");
    }
    cc_build
        .define("GFLAGS_IS_A_DLL", "0")
        .define("NDEBUG", None)
        .define(
            "REMILL_INSTALL_SEMANTICS_DIR",
            format!("\"{}\"", semantics_dir.display()).as_str(),
        )
        .define(
            "REMILL_BUILD_SEMANTICS_DIR_AARCH64",
            format!("\"{}/AArch64/Runtime\"", build_sem_dir.display()).as_str(),
        )
        .define(
            "REMILL_BUILD_SEMANTICS_DIR_AARCH32",
            format!("\"{}/AArch32/Runtime\"", build_sem_dir.display()).as_str(),
        )
        .define(
            "REMILL_BUILD_SEMANTICS_DIR_X86",
            format!("\"{}/X86/Runtime\"", build_sem_dir.display()).as_str(),
        )
        .define(
            "REMILL_BUILD_SEMANTICS_DIR_SPARC32",
            format!("\"{}/SPARC32/Runtime\"", build_sem_dir.display()).as_str(),
        )
        .define(
            "REMILL_BUILD_SEMANTICS_DIR_SPARC64",
            format!("\"{}/SPARC64/Runtime\"", build_sem_dir.display()).as_str(),
        )
        .define(
            "REMILL_BUILD_SEMANTICS_DIR_PPC64_32ADDR",
            format!("\"{}/PPC/Runtime\"", build_sem_dir.display()).as_str(),
        );
    cc_build.compile("azul_remill_wrapper");

    // Force-load the wrapper archive so the C-ABI entry points
    // (az_remill_lift, az_remill_compile_to_wasm32_obj,
    // az_remill_wasm_link, az_remill_free, az_remill_free_buf)
    // survive dead-strip. cc::Build emits
    // `cargo:rustc-link-lib=static=azul_remill_wrapper` which causes
    // normal symbol resolution; but until `native_remill.rs`'s
    // extern decls are CALLED from somewhere reachable, the linker
    // treats them as unused and strips. force_load pulls every .o
    // from the archive even without a call site.
    //
    // ld64 (macOS) syntax: `-Wl,-force_load,<archive>`.
    // GNU ld / lld (Linux) syntax: `-Wl,--whole-archive <archive>
    //   -Wl,--no-whole-archive`.
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR");
    let wrapper_path = format!("{}/libazul_remill_wrapper.a", out_dir);
    if is_apple {
        println!("cargo:rustc-link-arg=-Wl,-force_load,{}", wrapper_path);
    } else {
        println!("cargo:rustc-link-arg=-Wl,--whole-archive");
        println!("cargo:rustc-link-arg={}", wrapper_path);
        println!("cargo:rustc-link-arg=-Wl,--no-whole-archive");
    }

    // Emit link args for every static lib remill + LLVM + LLD need.
    // The set is derived from third_party/remill-install/build/remill/
    // build.ninja's LINK_LIBRARIES for the remill-lift-17 target,
    // augmented with the LLVM targets LLD's wasm driver expects via
    // InitializeAllTargets (PowerPC, NVPTX, Sparc, WebAssembly, ARM,
    // X86, AArch64 — every backend compiled into this vcpkg LLVM
    // build), plus libLLVMOption + libLLVMLTO + lld static libs.
    //
    // Order doesn't strictly matter on macOS ld64 (it does multiple
    // passes), but we keep groups together for readability.
    let lib_paths = build_remill_link_libs(&remill_build, &vcpkg_lib);
    for lib in &lib_paths {
        // -Wl,-force_load isn't needed — the wrapper directly
        // references the symbols (initialize_llvm_targets +
        // remill::Arch::Get etc.), pulling the rest in via normal
        // static-archive resolution.
        println!("cargo:rustc-link-arg={}", lib.display());
    }

    if is_apple {
        // macOS deployment target — match the cxx-common build.
        println!("cargo:rustc-link-arg=-mmacosx-version-min=12.0");
    } else {
        // Linux: need to link libc++ / libstdc++ explicitly because
        // cc::Build's `.cpp(true)` adds `-lc++` on macOS but Linux
        // depends on the system C++ runtime. cxx-common's LLVM
        // build uses libstdc++ on Linux, so link that.
        println!("cargo:rustc-link-lib=stdc++");
        println!("cargo:rustc-link-lib=pthread");
        println!("cargo:rustc-link-lib=dl");
        println!("cargo:rustc-link-lib=m");
    }
}

/// Enumerate every static library azul_remill needs to link against.
/// Returns absolute paths so cargo doesn't have to search.
#[cfg(feature = "web-transpiler-static")]
fn build_remill_link_libs(remill_build: &Path, vcpkg_lib: &Path) -> Vec<PathBuf> {
    let mut libs = Vec::new();

    // remill's own static libs (order matters — derived from
    // build.ninja's LINK_LIBRARIES for remill-lift-17).
    for rel in &[
        "lib/BC/libremill_bc.a",
        "lib/OS/libremill_os.a",
        "lib/Arch/libremill_arch.a",
        "lib/Arch/AArch64/libremill_arch_aarch64.a",
        "lib/Arch/Sleigh/libremill_arch_sleigh.a",
        "lib/Arch/SPARC32/libremill_arch_sparc32.a",
        "lib/Arch/SPARC64/libremill_arch_sparc64.a",
        "lib/Arch/X86/libremill_arch_x86.a",
        "lib/Version/libremill_version.a",
        "_deps/sleigh-build/libsla.a",
        "_deps/sleigh-build/libdecomp.a",
        "_deps/sleigh-build/support/libslaSupport.a",
    ] {
        let p = remill_build.join(rel);
        if p.exists() {
            libs.push(p);
        }
    }

    // LLVM target backends (CodeGen, AsmParser, AsmPrinter, Desc,
    // Disassembler, Info, Utils, TargetMCA — not all variants exist
    // per target).
    let llvm_targets = ["AArch64", "ARM", "NVPTX", "PowerPC", "Sparc", "WebAssembly", "X86"];
    let llvm_kinds = [
        "CodeGen",
        "AsmParser",
        "AsmPrinter",
        "Desc",
        "Disassembler",
        "Info",
        "Utils",
        "TargetMCA",
    ];
    for t in &llvm_targets {
        for k in &llvm_kinds {
            let p = vcpkg_lib.join(format!("libLLVM{}{}.a", t, k));
            if p.exists() {
                libs.push(p);
            }
        }
    }

    // LLVM core libs (mid-level + analysis + IR + support).
    for name in &[
        "libLLVMPasses.a",
        "libLLVMCoroutines.a",
        "libLLVMIRPrinter.a",
        "libLLVMipo.a",
        "libLLVMVectorize.a",
        "libLLVMFrontendOpenMP.a",
        "libLLVMLinker.a",
        "libLLVMInterpreter.a",
        "libLLVMMCJIT.a",
        "libLLVMExecutionEngine.a",
        "libLLVMOrcTargetProcess.a",
        "libLLVMOrcShared.a",
        "libLLVMRuntimeDyld.a",
        "libLLVMInstrumentation.a",
        "libLLVMCFGuard.a",
        "libLLVMGlobalISel.a",
        "libLLVMMCDisassembler.a",
        "libLLVMAsmPrinter.a",
        "libLLVMSelectionDAG.a",
        "libLLVMCodeGen.a",
        // CodeGenTypes carries LLT (low-level type) which CodeGen
        // references heavily. Often missing if downstream projects
        // enumerate libs by hand — CMake adds it transitively.
        "libLLVMCodeGenTypes.a",
        "libLLVMBitWriter.a",
        "libLLVMObjCARCOpts.a",
        "libLLVMScalarOpts.a",
        "libLLVMAggressiveInstCombine.a",
        "libLLVMInstCombine.a",
        "libLLVMTarget.a",
        "libLLVMTransformUtils.a",
        "libLLVMAnalysis.a",
        "libLLVMProfileData.a",
        "libLLVMSymbolize.a",
        "libLLVMDebugInfoDWARF.a",
        "libLLVMDebugInfoPDB.a",
        "libLLVMObject.a",
        // ObjCopy + ObjectYAML used by lld's IR loading path
        "libLLVMObjCopy.a",
        "libLLVMIRReader.a",
        "libLLVMAsmParser.a",
        "libLLVMBitReader.a",
        "libLLVMCore.a",
        "libLLVMRemarks.a",
        "libLLVMBitstreamReader.a",
        "libLLVMTextAPI.a",
        "libLLVMDebugInfoMSF.a",
        "libLLVMDebugInfoBTF.a",
        "libLLVMMCParser.a",
        "libLLVMMC.a",
        "libLLVMBinaryFormat.a",
        "libLLVMTargetParser.a",
        "libLLVMDebugInfoCodeView.a",
        "libLLVMSupport.a",
        "libLLVMDemangle.a",
        "libLLVMOption.a",
        "libLLVMLTO.a",
    ] {
        let p = vcpkg_lib.join(name);
        if p.exists() {
            libs.push(p);
        }
    }

    // LLD static libs.
    for name in &["liblldWasm.a", "liblldCommon.a"] {
        let p = vcpkg_lib.join(name);
        if p.exists() {
            libs.push(p);
        }
    }

    // Compression + math deps remill + LLVM rely on.
    for name in &["libz3.a", "libz.a", "libzstd.a", "libxed.a", "libglog.a", "libgflags.a"] {
        let p = vcpkg_lib.join(name);
        if p.exists() {
            libs.push(p);
        }
    }

    libs
}

// ── Generated file checks ─────────────────────────────────────────────

fn check_generated_files() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let codegen_dir = Path::new(&manifest_dir).join("../target/codegen");

    let checks: &[(&str, &str)] = &[
        ("CARGO_FEATURE_CABI_INTERNAL",    "dll_api_internal.rs"),
        ("CARGO_FEATURE_CABI_EXTERNAL",    "dll_api_external.rs"),
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

    // reexports.rs is needed when rust_api feature is enabled
    if env::var("CARGO_FEATURE_RUST_API").is_ok() {
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
/// 1. `AZ_DLL_PATH` (comma-separated, absolute or workspace-relative)
/// 2. `target/release` then `target/debug` (only when AZ_DLL_PATH unset)
///
/// If only a static library is found, links statically against it.
/// Copies the found dylib into the output directory so the binary can
/// find it at runtime without setting `DYLD_LIBRARY_PATH` / `LD_LIBRARY_PATH`.
fn configure_dynamic_linking(target: &str) {
    println!("cargo:rerun-if-env-changed=AZ_DLL_PATH");

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = Path::new(&manifest_dir).parent().unwrap();

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
    let env_path = env::var("AZ_DLL_PATH").unwrap_or_default();
    let mut dirs: Vec<(PathBuf, bool)> = Vec::new();

    // 1. AZ_DLL_PATH (user override, comma-separated) — local
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

        // Inform the user which library we're linking against
        let dir_str = dir.display().to_string();
        let kind = if *is_system {
            "system"
        } else if dir_str.contains("/debug") || dir_str.ends_with("/debug") {
            "local (debug)"
        } else {
            "local"
        };
        println!("cargo:warning=Linking against {} [{}]: {}", lib_filename(target), kind, dir_str);

        if dir_str.contains("/debug") && !dir_str.contains("/release") {
            println!("cargo:warning=Note: linking against debug build of libazul — \
                consider building with: cargo build --release -p azul-dll --features build-dll");
        }

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
            println!("cargo:rustc-link-lib=dylib=azul");

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
                sname, dir.display(),
            );
            println!("cargo:rustc-link-search=native={}", dir.display());
            println!("cargo:rustc-link-lib=static=azul");
            return;
        }
    }

    // Nothing found
    let searched: Vec<_> = dirs.iter().map(|(p, _)| p.display().to_string()).collect();
    println!("cargo:warning=Could not find {} or {}", lib_filename(target), sname);
    println!("cargo:warning=Set AZ_DLL_PATH to the directory containing the library");
    println!("cargo:warning=Searched: {}", searched.join(", "));
}

// ── iOS setup ─────────────────────────────────────────────────────────

fn configure_ios() {
    if env::var("AZ_IOS_SETUP").unwrap_or_default() == "disable" {
        return;
    }

    // xcode-select is required (provides the iOS SDK linker).
    check_tool("xcode-select", &["-p"], "Run 'xcode-select --install'");
    // ios-deploy is only needed for *device* deploy. Simulator deploy uses
    // `xcrun simctl install/launch` which is part of the Xcode CLT. Warn,
    // do not panic — many devs only target the simulator.
    warn_if_tool_missing(
        "ios-deploy",
        &["--version"],
        "Run 'brew install ios-deploy' to deploy to a physical iPhone. \
         Simulator deploys via 'xcrun simctl' do not need it.",
    );

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

fn warn_if_tool_missing(name: &str, args: &[&str], install_hint: &str) {
    match Command::new(name).args(args).status() {
        Ok(s) if s.success() => {}
        _ => println!("cargo:warning='{}' not found — {}", name, install_hint),
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

// ── Debugger asset compression ───────────────────────────────────────

/// Brotli-compress debugger UI assets (CSS, JS, HTML) at build time.
/// The compressed files are written to OUT_DIR and included via include_bytes!
/// in debug_server.rs, then served with Content-Encoding: br.
fn compress_debugger_assets() {
    let out_dir = env::var("OUT_DIR").unwrap_or_default();
    if out_dir.is_empty() { return; }

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let debugger_dir = Path::new(&manifest_dir)
        .join("src/desktop/shell2/common/debugger");

    let assets = &[
        ("debugger.css", "debugger.css.br"),
        ("debugger.js", "debugger.js.br"),
        ("debugger.html", "debugger.html.br"),
    ];

    for &(src_name, br_name) in assets {
        let src_path = debugger_dir.join(src_name);
        if !src_path.exists() { continue; }

        println!("cargo:rerun-if-changed={}", src_path.display());
        brotli_compress_file(&src_path, &Path::new(&out_dir).join(br_name));
    }

}

fn brotli_compress_file(src: &Path, dst: &Path) {
    let raw = fs::read(src).unwrap();
    let mut compressed = Vec::new();
    let params = brotli::enc::BrotliEncoderParams {
        quality: 11,
        ..Default::default()
    };
    brotli::BrotliCompress(&mut &raw[..], &mut compressed, &params).unwrap();
    fs::write(dst, &compressed).unwrap();
}
