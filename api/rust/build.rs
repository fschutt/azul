fn main() {
    // dynamically link azul.dll
    println!("cargo:rustc-link-search={}", env!("AZUL_INSTALL_DIR"));
}