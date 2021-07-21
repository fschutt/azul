fn main() {
    // dynamically link azul.dll
    #[cfg(not(any(feature = "link_static", feature = "docs_rs")))] {
        println!("cargo:rustc-link-search={}", env!("AZUL_LINK_PATH")); /* path to folder with azul.dll / libazul.so */
    }
}