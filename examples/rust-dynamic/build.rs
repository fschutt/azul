// build.rs - Tell Cargo where to find libazul_dll.dylib
fn main() {
    // Path to the compiled DLL
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let project_root = std::path::Path::new(&manifest_dir)
        .parent().unwrap()
        .parent().unwrap();
    let lib_path = project_root.join("target").join("release");
    
    println!("cargo:rustc-link-search=native={}", lib_path.display());
    
    // On macOS, also set the rpath so the executable can find the dylib at runtime
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_path.display());
}
