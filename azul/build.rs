fn main() {
    // dynamically link azul.dll
    println!("cargo:rustc-flags=-l dylib=azul");
    println!("cargo:rustc-link-search={}", env!("AZUL_INSTALL_DIR"));
}