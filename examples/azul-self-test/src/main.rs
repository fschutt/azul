// Desktop / iOS entry point — the probe logic lives in the library so the same
// crate can also build as an Android cdylib (see src/lib.rs). Exits with the
// self-test's accumulated exit code (non-zero only on a required-probe failure;
// an unavailable device is not a failure).
fn main() {
    azul_self_test::start();
    std::process::exit(azul_self_test::exit_code());
}
