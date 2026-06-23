// Desktop / iOS entry point — app logic is in the library so the same crate
// also builds as an Android cdylib (see src/lib.rs).
fn main() {
    azul_widgets::start();
}
