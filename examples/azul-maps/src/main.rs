// Desktop / iOS entry point — the app logic lives in the library so the same
// crate can also build as an Android cdylib (see src/lib.rs).
fn main() {
    azul_maps::start();
}
