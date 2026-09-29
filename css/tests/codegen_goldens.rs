//! Golden tests of the CSS code generators: ONE shared set of stylesheets,
//! printed by every binding language's backend and compared with
//! `tests/codegen_goldens/<lang>/<case>.<ext>` (plus the standalone project
//! of the `basic` case under `<lang>/project/`).
//!
//! Run: `cargo test -p azul-css --features codegen --test codegen_goldens`.
//! After an intended output change, rewrite the files with
//! `AZ_BLESS=1 cargo test -p azul-css --features codegen --test codegen_goldens`
//! and review the diff. A missing golden file is a failure (never a silent
//! pass), with the bless command in the message.

use std::{fs, path::PathBuf};

use azul_css::codegen::backend_for;

/// (backend id, snippet file extension). Every language azul has bindings
/// for (`doc/src/codegen/v2/lang_*`).
const LANGS: &[(&str, &str)] = &[
    ("rust", "rs"),
    ("c", "h"),
    ("cpp", "hpp"),
    ("python", "py"),
    ("csharp", "cs"),
    ("java", "java"),
    ("kotlin", "kt"),
    ("go", "go"),
    ("swift", "swift"),
    ("node", "js"),
    ("ruby", "rb"),
    ("php", "php"),
    ("lua", "lua"),
    ("zig", "zig"),
    ("nim", "nim"),
    ("d", "d"),
    ("ocaml", "ml"),
    ("haskell", "hs"),
    ("julia", "jl"),
    ("pascal", "pas"),
    ("ada", "adb"),
    ("algol68", "a68"),
    ("cobol", "cob"),
    ("crystal", "cr"),
    ("fortran", "f90"),
    ("freebasic", "bas"),
    ("lisp", "lisp"),
    ("odin", "odin"),
    ("perl", "pl"),
    ("powershell", "ps1"),
    ("racket", "rkt"),
    ("red", "red"),
    ("smalltalk", "st"),
    ("v", "v"),
    ("vb6", "bas"),
];

mod codegen_cases;

use codegen_cases::{BASIC, CASES};

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("codegen_goldens")
}

fn bless() -> bool {
    std::env::var_os("AZ_BLESS").is_some()
}

/// Compare `got` with the golden at `rel`, or write it when blessing.
fn compare(rel: &str, got: &str, failures: &mut Vec<String>) {
    let path = golden_dir().join(rel);
    if bless() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, got).unwrap();
        return;
    }
    let Ok(want) = fs::read_to_string(&path) else {
        failures.push(format!(
            "{rel}: golden file missing - review the output and create it with \
             `AZ_BLESS=1 cargo test -p azul-css --features codegen --test codegen_goldens`\n\
             ---- output ----\n{got}"
        ));
        return;
    };
    if want != got {
        let first = want
            .lines()
            .zip(got.lines())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| want.lines().count().min(got.lines().count()));
        failures.push(format!(
            "{rel}: differs from the golden file (first difference at line {}):\n  want: {:?}\n  \
             got:  {:?}\n---- full output ----\n{got}",
            first + 1,
            want.lines().nth(first).unwrap_or("<eof>"),
            got.lines().nth(first).unwrap_or("<eof>"),
        ));
    }
}

fn check_lang(lang: &str) {
    let backend =
        backend_for(lang).unwrap_or_else(|| panic!("no codegen backend for {lang:?}"));
    let ext = LANGS
        .iter()
        .find(|(l, _)| *l == lang)
        .map(|(_, e)| *e)
        .unwrap();
    let mut failures = Vec::new();
    for (case, css) in CASES {
        let (parsed, _warnings) = azul_css::parser2::new_from_str(css);
        let got = backend.emit_css(&parsed);
        compare(&format!("{lang}/{case}.{ext}"), &got, &mut failures);
    }
    let (parsed, _warnings) = azul_css::parser2::new_from_str(BASIC);
    for file in backend.emit_project(&parsed) {
        compare(&format!("{lang}/project/{}", file.path), &file.contents, &mut failures);
    }
    // the exact `Css` value (rules, selectors, priorities, conditions)
    let (parsed, _warnings) = azul_css::parser2::new_from_str(codegen_cases::CONDITIONS);
    compare(
        &format!("{lang}/stylesheet.{ext}"),
        &backend.emit_stylesheet(&parsed),
        &mut failures,
    );
    // a programmatic property list: revert / unset / keywords / text-shadow
    compare(
        &format!("{lang}/keywords.{ext}"),
        &backend.emit_module(&codegen_cases::keyword_module()),
        &mut failures,
    );
    assert!(
        failures.is_empty(),
        "{} golden mismatch(es) for {lang}:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

macro_rules! golden_tests {
    ($($name:ident => $lang:literal),* $(,)?) => {$(
        #[test]
        fn $name() {
            check_lang($lang);
        }
    )*};
}

golden_tests! {
    the_rust_output_matches_its_golden_files => "rust",
    the_c_output_matches_its_golden_files => "c",
    the_cpp_output_matches_its_golden_files => "cpp",
    the_python_output_matches_its_golden_files => "python",
    the_csharp_output_matches_its_golden_files => "csharp",
    the_java_output_matches_its_golden_files => "java",
    the_kotlin_output_matches_its_golden_files => "kotlin",
    the_go_output_matches_its_golden_files => "go",
    the_swift_output_matches_its_golden_files => "swift",
    the_node_output_matches_its_golden_files => "node",
    the_ruby_output_matches_its_golden_files => "ruby",
    the_php_output_matches_its_golden_files => "php",
    the_lua_output_matches_its_golden_files => "lua",
    the_zig_output_matches_its_golden_files => "zig",
    the_nim_output_matches_its_golden_files => "nim",
    the_d_output_matches_its_golden_files => "d",
    the_ocaml_output_matches_its_golden_files => "ocaml",
    the_haskell_output_matches_its_golden_files => "haskell",
    the_julia_output_matches_its_golden_files => "julia",
    the_pascal_output_matches_its_golden_files => "pascal",
    the_ada_output_matches_its_golden_files => "ada",
    the_algol68_output_matches_its_golden_files => "algol68",
    the_cobol_output_matches_its_golden_files => "cobol",
    the_crystal_output_matches_its_golden_files => "crystal",
    the_fortran_output_matches_its_golden_files => "fortran",
    the_freebasic_output_matches_its_golden_files => "freebasic",
    the_lisp_output_matches_its_golden_files => "lisp",
    the_odin_output_matches_its_golden_files => "odin",
    the_perl_output_matches_its_golden_files => "perl",
    the_powershell_output_matches_its_golden_files => "powershell",
    the_racket_output_matches_its_golden_files => "racket",
    the_red_output_matches_its_golden_files => "red",
    the_smalltalk_output_matches_its_golden_files => "smalltalk",
    the_v_output_matches_its_golden_files => "v",
    the_vb6_output_matches_its_golden_files => "vb6",
}
