fn main() {
    // dynamically link azul.dll
    #[cfg(all(feature = "link-dynamic", not(feature = "link-static")))]
    {
        println!("cargo:rustc-link-search={}", env!("AZUL_LINK_PATH")); /* path to folder with azul.dll / libazul.so */
    }
}
