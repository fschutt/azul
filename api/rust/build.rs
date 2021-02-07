fn main() {
    // dynamically link azul.dll
    #[cfg(not(feature = "link_static"))] {
        println!("cargo:rustc-link-search={}", env!("AZUL_INSTALL_DIR"));
    }
}