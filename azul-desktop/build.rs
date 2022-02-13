fn main() {
    if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-search=framework=/Library/Frameworks");
    }
}