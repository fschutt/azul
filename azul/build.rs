fn main() {
    // dynamically link azul.dll
    println!(r"cargo:rustc-link-search={}", env!(AZUL_INSTALL_DIR));
}