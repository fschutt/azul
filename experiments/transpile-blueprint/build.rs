// Build script: opt-in to remill linkage.
//
// When the `remill` feature is OFF (default): no C++ is compiled, no
// remill library is searched for. The Rust binary builds with just the
// pure-Rust crates listed in [dependencies].
//
// When the `remill` feature is ON: `REMILL_INSTALL_DIR` must point at a
// `cmake --install` tree produced by building third_party/remill. We
// compile the small C++ shim in cpp/shim.cpp, link it against remill's
// static libs and the system LLVM, and expose it to Rust through a cxx
// bridge declared in src/ffi.rs.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=cpp/shim.cpp");
    println!("cargo:rerun-if-changed=cpp/shim.h");
    println!("cargo:rerun-if-env-changed=REMILL_INSTALL_DIR");

    #[cfg(feature = "remill")]
    build_remill_shim();
}

#[cfg(feature = "remill")]
fn build_remill_shim() {
    let install_dir = std::env::var("REMILL_INSTALL_DIR").expect(
        "feature `remill` is ON but $REMILL_INSTALL_DIR is unset. \
         Point it at the output of `cmake --install` for third_party/remill.",
    );
    let llvm_prefix = std::env::var("LLVM_PREFIX")
        .unwrap_or_else(|_| "/opt/homebrew/opt/llvm@17".to_string());

    cxx_build::bridge("src/ffi.rs")
        .file("cpp/shim.cpp")
        .include(format!("{install_dir}/include"))
        .include(format!("{llvm_prefix}/include"))
        .flag_if_supported("-std=c++17")
        .flag_if_supported("-fno-rtti")
        .compile("transpile_blueprint_shim");

    // Linker flags. remill ships a header-only INTERFACE target plus the
    // semantics + lifter static libraries. The exact set is discovered
    // at install time — we just point at the dir.
    println!("cargo:rustc-link-search=native={install_dir}/lib");
    println!("cargo:rustc-link-search=native={llvm_prefix}/lib");
    println!("cargo:rustc-link-lib=static=remill");
    println!("cargo:rustc-link-lib=dylib=LLVM");
    println!("cargo:rustc-link-lib=dylib=c++");
}
