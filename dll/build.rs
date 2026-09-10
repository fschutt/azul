use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

/// Emit `AZUL_LIFT_BUILD_ID` for the web lift cache (see `main()`). A CI/docker
/// build can override it explicitly; otherwise it's the short git hash, plus a
/// `-dirty` marker when the tree has uncommitted changes.
fn emit_lift_build_id() {
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-env-changed=AZUL_LIFT_BUILD_ID");
    if let Ok(v) = env::var("AZUL_LIFT_BUILD_ID") {
        if !v.is_empty() {
            println!("cargo:rustc-env=AZUL_LIFT_BUILD_ID={v}");
            return;
        }
    }
    let git = |args: &[&str]| -> Option<String> {
        let out = Command::new("git").args(args).output().ok()?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!s.is_empty()).then_some(s)
    };
    let id = match git(&["rev-parse", "--short=12", "HEAD"]) {
        Some(hash) => {
            let dirty = git(&["status", "--porcelain"]).is_some_and(|s| !s.is_empty());
            if dirty {
                format!("{hash}-dirty")
            } else {
                hash
            }
        }
        None => "unknown".to_string(),
    };
    println!("cargo:rustc-env=AZUL_LIFT_BUILD_ID={id}");
}

fn main() {
    let target = env::var("TARGET").unwrap_or_default();

    // `az_db_engine`: the bundled SQLite engine (`turso`) is compiled in.
    // It is `feature = "db-sqlite"` MINUS 32-bit Linux: turso pulls
    // turso_sync_engine 0.7.2, whose sparse_io.rs passes `pos as i64` to
    // libc::lseek/fallocate — `off_t` is i32 on i686/armv7 glibc, so the crate
    // itself does not compile there (upstream main still has it, 2026-09-07).
    // Cargo.toml declares `turso`/`aegis` for the same target set, so on
    // 32-bit Linux the feature stays ON (the api.json surface is unchanged),
    // the engine is OFF, and `Db::open` returns an invalid handle exactly as
    // it does without the feature. Drop the target clause once the fork
    // (fschutt/turso) ships a sync engine with `pos as libc::off_t`.
    // `az_gpu_video`: the Vulkan Video decoder (`gpu-video`, which pulls the
    // C++ `vk-mem`) is compiled in. ONE definition for both halves — the
    // dependency's target cfg in Cargo.toml and the `mod decode_vulkan` that
    // uses it — because stating the same condition twice is how they drifted:
    // narrowing the manifest to glibc left the module compiled on musl, where
    // `use gpu_video::…` then failed to resolve (CI 2026-09-08).
    println!("cargo:rustc-check-cfg=cfg(az_gpu_video)");
    {
        let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
        let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
        let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
        // Keep in step with dll/Cargo.toml's `[target.'cfg(...)'.dependencies]`
        // for `gpu-video`: x86_64 desktop, and on Linux only glibc (no musl C++
        // toolchain in the cross-compile checks).
        let dep_present =
            arch == "x86_64" && ((os == "linux" && target_env == "gnu") || os == "windows");
        if env::var("CARGO_FEATURE_VIDEO_NATIVE").is_ok() && dep_present {
            println!("cargo:rustc-cfg=az_gpu_video");
        }
    }

    println!("cargo:rustc-check-cfg=cfg(az_db_engine)");
    {
        let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
        let width = env::var("CARGO_CFG_TARGET_POINTER_WIDTH").unwrap_or_default();
        let engine_target = !(os == "linux" && width == "32");
        if env::var("CARGO_FEATURE_DB_SQLITE").is_ok() && engine_target {
            println!("cargo:rustc-cfg=az_db_engine");
        }
    }

    // Embed a build identity for the web lift cache. A CLEAN git checkout keys the
    // framework lift cache by (ref + fn name) — arch-neutral, so an aarch64-lifted
    // WASM cache is reused by an x86 server (transpiler_remill::lift_cache_path).
    // A DIRTY tree gets a `-dirty` marker so dev builds fall back to byte-keying
    // (which catches every recompile). Re-lifts when the azul source ref changes.
    emit_lift_build_id();

    // Fail fast on mutually-exclusive *config* — NOT a platform gate. Platform
    // features (camera, etc.) dlopen at runtime and must never fail the build so
    // cross-compilation always works; but two global allocators cannot coexist.
    if env::var("CARGO_FEATURE_ALLOCATOR_MIMALLOC").is_ok()
        && env::var("CARGO_FEATURE_ALLOCATOR_JEMALLOC").is_ok()
    {
        panic!(
            "azul-dll: features `allocator_mimalloc` and `allocator_jemalloc` are mutually \
             exclusive — enable at most one global allocator (and do not use `--all-features`, \
             which turns on both)."
        );
    }

    check_generated_files();
    compress_debugger_assets();
    bundle_e2e_web_runner();

    if env::var("CARGO_FEATURE_CABI_EXTERNAL").is_ok() {
        // Relative AZ_LINK_PATH entries resolve against the workspace root; the
        // workspace's own target dirs are the local candidates (a `cargo build
        // --release -p azul-dll --features build-dll` puts libazul there).
        let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
        let workspace_root = Path::new(&manifest_dir).parent().unwrap().to_path_buf();
        configure_dynamic_linking(
            &target,
            &workspace_root,
            &[
                workspace_root.join("target/release"),
                workspace_root.join("target/debug"),
            ],
        );
    }

    // Restrict the cdylib's exported symbols to the azul C API. Without this
    // the shipped libazul carries ~120 stray turso/SQLite extension exports
    // (time_*, uuid*, dur_*, *_GenerateSeriesVTabModule — turso's proc-macros
    // slap #[no_mangle] on every registered SQL function) plus a stray
    // material-icons `icon_to_char`. Generic names like `time_parse` in the
    // host's global dynamic namespace are a real symbol-clash risk on Linux's
    // flat namespace, and bloat the export tables everywhere. Keeps every
    // Az* (the C API + all host-invoker AzApp_set*Invoker /
    // *_createFromHostHandle exports), az_* (az_purge_allocator), PyInit_*,
    // and — on Linux python builds — Py*/_Py* so libpython can still interpose
    // the weak stubs at import. Audit §2.1. Only affects the cdylib; the
    // staticlib keeps all symbols for static consumers of db-sqlite.
    if env::var("CARGO_FEATURE_BUILD_DLL").is_ok()
        || env::var("CARGO_FEATURE_PYTHON_EXTENSION").is_ok()
    {
        restrict_cdylib_exports(&target);
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

    // rustc's default LC_ID_DYLIB is the ABSOLUTE build path
    // (…/target/release/deps/libazul.dylib). Every executable that links the
    // downloaded dylib then records that build-machine path and dies at dyld
    // load on the user's machine — the documented `-Wl,-rpath,@executable_path`
    // install steps only work if the id is @rpath-relative. (CI e2e never
    // catches this because it links and runs on the same machine.)
    if target.contains("apple-darwin") {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/libazul.dylib");
    }

    // Python: the pyo3 extension leaves every `Py*` symbol UNDEFINED (the
    // interpreter resolves them at import — the standard pyo3/manylinux way).
    // The extension (azul.so) ships SEPARATELY from the python-free C
    // libazul.so; there are NO baked-in Py* stubs. The old single-.so "bake
    // Py* stubs into the cdylib" scheme was an LLVM dso_local anti-pattern that
    // SIGSEGV'd every pyclass teardown — see dll/src/lib.rs.

    if target.contains("ios") {
        configure_ios();
    }
    if target.contains("android") {
        configure_android();
    }
}

// ── Export restriction: keep only the azul C API in the cdylib ────────

/// Emit a linker export list so the cdylib exports only the azul C API
/// (Az*), az_* helpers, and the python module init. Everything else (turso SQL-function
/// exports, material-icons `icon_to_char`, wasm-bindgen shims) is localized.
/// See the call site for the rationale. Windows is intentionally skipped:
/// DLL symbol resolution is not a flat namespace (no cross-DLL clash), and a
/// .def would have to enumerate all ~13,900 Az names.
fn restrict_cdylib_exports(target: &str) {
    let out = env::var("OUT_DIR").expect("OUT_DIR");
    let is_python = env::var("CARGO_FEATURE_PYTHON_EXTENSION").is_ok()
        || env::var("CARGO_FEATURE_PYO3").is_ok();

    if target.contains("apple") || target.contains("darwin") || target.contains("ios") {
        // Mach-O: -exported_symbols_list KEEPS only matching globs; all other
        // global symbols become private extern (localized). Mach-O prefixes an
        // underscore, so C `AzApp_new` is `_AzApp_new`. The objc2 per-class
        // registration statics (__CLASS_Azul*, __IVAR_OFFSET_Azul*,
        // __DROP_FLAG_OFFSET_Azul*, __REGISTER_CLASS_Azul*) are kept by the
        // SPECIFIC globs below — not a broad `*Azul*`, which also exported
        // azul's own mangled `_ZN..AzulPixmap..` / `..AzulMenuTarget..` methods
        // and tripped the CI export gate (which allows exactly these statics).
        // NB (verified on ld-1115 / ld-prime): -exported_symbols_list IS
        // honoured (an all-miss list errors "symbol(s) not found"), and it
        // localizes the bulk — the shipped cdylib goes from ~285 k global
        // symbols to ~14 k, essentially just `Az*`. BUT ld-prime keeps a
        // dependency's `#[no_mangle]` symbols exported regardless of the
        // whitelist (rustc force-marks them no-dead-strip), and
        // -unexported_symbols_list "cannot be used together with
        // -exported_symbols_list", so ~121 turso/limbo SQLite loadable-
        // extension entry points (`*VTabModule`, `time_*`, `uuid*`, …) +
        // material-icons `icon_to_char` still leak on macOS. They are a
        // non-issue there (Mach-O two-level namespace already scopes lookups)
        // and are fully hidden on Linux, where the version-script below IS
        // authoritative over dep `#[no_mangle]`. The CI export gate allows
        // exactly this family set on macOS and demands zero strays on Linux.
        let patterns = "_Az*\n_az_*\n_PyInit_*\n*_CLASS_Azul*\n*_IVAR_OFFSET_Azul*\n*\
                        _DROP_FLAG_OFFSET_Azul*\n*_REGISTER_CLASS_Azul*\n";
        let path = format!("{}/azul_exported_symbols.txt", out);
        fs::write(&path, patterns).expect("write exported symbols list");
        println!("cargo:rustc-cdylib-link-arg=-Wl,-exported_symbols_list,{path}");
    } else if target.contains("linux") || target.contains("android") {
        // ELF: rely on rustc's OWN cdylib export control (an anonymous
        // version node that exports the reachable `#[no_mangle]`/`#[export_name]`
        // symbols and hides mangled Rust symbols). We must NOT add our own
        // export restriction here:
        //   * a second `--version-script` fails — GNU ld rejects "anonymous version tag cannot be
        //     combined with other version tags".
        //   * `--exclude-libs,ALL` localizes ALL static-archive symbols, which hides azul's C-ABI
        //     exports that live in the azul-core/azul-layout rlibs — most importantly the
        //     host-invoker surface (`AzApp_setHostHandleReleaser`, `AzRefAny_newHostHandle`, …)
        //     that every scripting binding (python/lua/ruby/…) needs. Hiding those breaks the whole
        //     host-invoker family at runtime with "undefined symbol".
        // So rustc's default is authoritative: it exports azul's `Az*`/`az_*`/
        // `PyInit_*` (wherever they are defined) plus a handful of dependency
        // `#[no_mangle]` strays (turso/limbo SQLite entry points, material-icons
        // `icon_to_char`, …). Those strays are harmless symbol-table bloat and
        // are allow-listed by the "Assert cdylib exports" CI gate, same as on
        // macOS. Mangled Rust symbols are not `#[no_mangle]`, so rustc already
        // keeps them out of `.dynsym`.
        let _ = is_python;
    }
    // Windows / other: no restriction (see doc comment).
}

// ── Python: no Py* stubs ──────────────────────────────────────────────
//
// The pyo3 extension leaves `Py*` UNDEFINED (interpreter resolves at import).
// No fallback stubs are baked in — defining CPython-API symbols in the pyo3
// cdylib self-mis-binds via LLVM dso_local and SIGSEGVs pyclass teardown
// (removed 2026-07-07; see dll/src/lib.rs). Ship the extension separately from
// the python-free C libazul.so.

// ── Android setup ─────────────────────────────────────────────────────

fn configure_android() {
    // Link the system libraries every Android cdylib needs.
    println!("cargo:rustc-link-lib=android");
    println!("cargo:rustc-link-lib=log");
    // Camera backend calls NDK media APIs; link their stub libs so a binary that
    // statically links azul (link-static demos) resolves the symbols instead of
    // failing with "undefined symbol: ACameraManager_* / AImageReader_*". These
    // are API 24, so the Android floor is API 24 (Android 7.0). AAudio (API 26)
    // is NOT linked here — extra::audio::aaudio dlopen's libaaudio.so at runtime,
    // so the app still loads on API 24/25 (audio just reports unavailable there).
    println!("cargo:rustc-link-lib=camera2ndk"); // extra::camera::android (API 24)
    println!("cargo:rustc-link-lib=mediandk"); // AImageReader (API 24)

    if env::var("ANDROID_NDK_HOME").is_err() && env::var("ANDROID_HOME").is_err() {
        println!(
            "cargo:warning=ANDROID_NDK_HOME / ANDROID_HOME not set. Install with: brew install \
             --cask android-commandlinetools && sdkmanager 'ndk;27.0.12077973'"
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
        println!(
            "cargo:warning=web-transpiler-static is macOS + Linux only; skipping for {}",
            target
        );
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
            "third_party/cxx-common/vcpkg_macos-13_llvm-17-liftingbits-llvm_xcode-15.0_arm64/\
             installed/arm64-osx-rel",
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
            "third_party/cxx-common/vcpkg_ubuntu-22.04_llvm-17-liftingbits-llvm_{arch}/installed/\
             {arch}-rel"
        );
        workspace_root.join(bundle)
    };

    for p in [&remill_install, &remill_build, &vcpkg_base] {
        if !p.exists() {
            panic!(
                "web-transpiler-static requires {} — run `bash scripts/build_remill.sh` from the \
                 workspace root to bootstrap",
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
    let llvm_targets = [
        "AArch64",
        "ARM",
        "NVPTX",
        "PowerPC",
        "Sparc",
        "WebAssembly",
        "X86",
    ];
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
    for name in &[
        "libz3.a",
        "libz.a",
        "libzstd.a",
        "libxed.a",
        "libglog.a",
        "libgflags.a",
    ] {
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
    // AZ_CODEGEN_DIR: where to find the azul-doc-generated sources. Lets a
    // consumer who depends on this crate from a read-only checkout (e.g. a
    // cargo git dependency) point at a directory they ran `azul-doc codegen
    // all` into, instead of the workspace-relative default.
    println!("cargo:rerun-if-env-changed=AZ_CODEGEN_DIR");
    let codegen_dir = match env::var("AZ_CODEGEN_DIR") {
        Ok(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => Path::new(&manifest_dir).join("../target/codegen"),
    };

    let checks: &[(&str, &str)] = &[
        ("CARGO_FEATURE_CABI_INTERNAL", "dll_api_internal.rs"),
        ("CARGO_FEATURE_CABI_EXTERNAL", "dll_api_external.rs"),
        ("CARGO_FEATURE_PYTHON_EXTENSION", "python_api.rs"),
    ];

    for &(feature_env, filename) in checks {
        if env::var(feature_env).is_ok() {
            let path = codegen_dir.join(filename);
            if !path.exists() {
                panic!(
                    "\nMissing generated file: {}\nIn a checkout of the azul repo, run:\n\x20 \
                     cargo run --release -p azul-doc codegen all\nDepending on azul-dll as a bare \
                     `--git` dependency cannot work\n(these sources are generated, not committed) \
                     — clone the repo,\nrun the codegen line above, then depend on it by \
                     `--path`,\nor set AZ_CODEGEN_DIR to a directory holding the generated \
                     files.\n",
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
                "\nMissing generated file: reexports.rs\nRun: cargo run --release -p azul-doc \
                 codegen all\n(or set AZ_CODEGEN_DIR — see the note above about git \
                 dependencies)\n",
            );
        }
        println!("cargo:rerun-if-changed={}", path.display());
    }
}

// ── Dynamic linking — shared with the pre-rendered `azul` crate, see the file.
include!("build_link.rs");

// ── iOS setup ─────────────────────────────────────────────────────────

fn configure_ios() {
    if env::var("AZ_IOS_SETUP").unwrap_or_default() == "disable" {
        return;
    }

    // xcode-select provides the iOS SDK linker — needed to LINK/bundle a real
    // iOS build, but NOT to `cargo check`. Warn instead of panicking (matching
    // ios-deploy below) so cross-compile type-checks from a non-macOS host
    // aren't blocked outright; a real iOS link without it still fails later with
    // a clear linker error.
    warn_if_tool_missing("xcode-select", &["-p"], "Run 'xcode-select --install'");
    // ios-deploy is only needed for *device* deploy. Simulator deploy uses
    // `xcrun simctl install/launch` which is part of the Xcode CLT. Warn,
    // do not panic — many devs only target the simulator.
    warn_if_tool_missing(
        "ios-deploy",
        &["--version"],
        "Run 'brew install ios-deploy' to deploy to a physical iPhone. Simulator deploys via \
         'xcrun simctl' do not need it.",
    );
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

// ── Debugger asset compression ───────────────────────────────────────

/// Brotli-compress debugger UI assets (CSS, JS, HTML) at build time.
/// The compressed files are written to OUT_DIR and included via include_bytes!
/// in debug_server.rs, then served with Content-Encoding: br.
fn compress_debugger_assets() {
    let out_dir = env::var("OUT_DIR").unwrap_or_default();
    if out_dir.is_empty() {
        return;
    }

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let debugger_dir = Path::new(&manifest_dir).join("src/desktop/shell2/common/debugger");

    let assets = &[
        ("debugger.css", "debugger.css.br"),
        ("debugger.js", "debugger.js.br"),
        ("debugger.html", "debugger.html.br"),
    ];

    for &(src_name, br_name) in assets {
        let src_path = debugger_dir.join(src_name);
        if !src_path.exists() {
            continue;
        }

        println!("cargo:rerun-if-changed={}", src_path.display());
        brotli_compress_file(&src_path, &Path::new(&out_dir).join(br_name));
    }
}

/// Bundle the web e2e harness (scripts/e2e-web/*.mjs) into the dll so any
/// app binary can act as the test executor without the user hunting for
/// script files: `AZ_BACKEND=<http url> AZ_E2E=<dir|file> ./app` extracts the
/// bundle to a temp dir and spawns node/bun/deno on it (see
/// src/e2e_web_runner.rs). Format: brotli over a simple TLV stream of
/// [u32 path_len][path bytes][u32 content_len][content bytes] entries.
fn bundle_e2e_web_runner() {
    if env::var("CARGO_FEATURE_WEB_E2E_RUNNER").is_err() {
        return;
    }
    let out_dir = env::var("OUT_DIR").unwrap_or_default();
    if out_dir.is_empty() {
        return;
    }
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let src_root = Path::new(&manifest_dir).join("../scripts/e2e-web");

    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut collect = |rel_dir: &str| {
        let dir = src_root.join(rel_dir);
        let Ok(entries) = fs::read_dir(&dir) else {
            return;
        };
        let mut names: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |x| x == "mjs"))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort(); // deterministic bundle → stable content hash
        for name in names {
            let p = dir.join(&name);
            println!("cargo:rerun-if-changed={}", p.display());
            let rel = if rel_dir.is_empty() {
                name.clone()
            } else {
                format!("{rel_dir}/{name}")
            };
            files.push((rel, fs::read(&p).unwrap()));
        }
    };
    collect("");
    collect("lib");
    if files.is_empty() {
        panic!(
            "azul-dll: feature web-e2e-runner is enabled but no .mjs files found in {}",
            src_root.display()
        );
    }
    println!("cargo:rerun-if-changed={}", src_root.display());

    let mut tlv = Vec::new();
    for (path, content) in &files {
        tlv.extend_from_slice(&(path.len() as u32).to_le_bytes());
        tlv.extend_from_slice(path.as_bytes());
        tlv.extend_from_slice(&(content.len() as u32).to_le_bytes());
        tlv.extend_from_slice(content);
    }
    let mut compressed = Vec::new();
    let params = brotli::enc::BrotliEncoderParams {
        quality: 11,
        ..Default::default()
    };
    brotli::BrotliCompress(&mut &tlv[..], &mut compressed, &params).unwrap();
    fs::write(Path::new(&out_dir).join("e2e_web_bundle.br"), &compressed).unwrap();
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
