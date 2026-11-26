//! The Lisp leg of the language board on Windows.
//!
//! The runner's Git Bash has `$HOME` as `/c/Users/runneradmin`, and the SBCL that chocolatey
//! installs there is a native Windows program: it reads `/c/Users/...` as a path on the current
//! drive, and the leg stopped at its first form, `(load "/c/Users/runneradmin/quicklisp/setup.lisp")`
//! - "Couldn't load ...: file does not exist" (website run 45738701245). With the path as
//! `cygpath -m` writes it (`C:/Users/runneradmin/quicklisp/setup.lisp`) quicklisp loads and the leg
//! goes on to the next thing it needs, which the runner does not have (cffi-libffi's `ffi.h` and
//! `pkg-config`; run 45754551758): the binding stays out of the Windows gate for that, see
//! `WINDOWS_NONGATING_LANGS`.

fn repo_file(rel: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
}

/// The text of `lang_lisp` in the language board.
fn leg() -> String {
    let script = repo_file("scripts/e2e_language_matrix.sh");
    let from = script.find("\nlang_lisp() {").expect("lang_lisp in the language board");
    script[from..from + script[from..].find("\n}\n").expect("end of lang_lisp")].to_string()
}

#[test]
fn the_leg_gives_a_native_windows_sbcl_a_windows_path_to_quicklisp() {
    let leg = leg();
    assert!(
        leg.contains("cygpath -m"),
        "sbcl.exe cannot read the MSYS path $HOME has in Git Bash; the leg must convert it:\n{}",
        leg
    );
}

#[test]
fn the_load_form_uses_the_converted_path() {
    let leg = leg();
    assert!(
        !leg.contains("(load \\\"$HOME/quicklisp/setup.lisp"),
        "the load form names $HOME's own spelling of the path:\n{}",
        leg
    );
    assert!(leg.contains("(load \\\"$QL\\\")"), "the load form does not use the converted path:\n{}", leg);
}
