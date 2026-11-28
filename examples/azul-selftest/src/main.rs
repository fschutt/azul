//! The desktop and iOS entry: runs the self-test and exits with its code (non-zero when a capability that
//! says it is available failed). Android has no `main`: the library's load-time constructor starts it.

fn main() {
    let code = azul_selftest::start();
    std::process::exit(code);
}
