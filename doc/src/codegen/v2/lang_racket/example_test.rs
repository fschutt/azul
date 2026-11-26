//! The Racket example against the module the generator writes for it.
//!
//! `examples/racket/hello-world.rkt` called `(make-window-create-options)`, a name the generator
//! gives a class whose constructor is `new` (or `default`): `WindowCreateOptions` has `create(layout)`
//! and `create_default`, so the module defines `window-create-options-create` and
//! `window-create-options-create-default`, and the Racket leg of the language board stopped at
//! `hello-world.rkt:31:15: make-window-create-options: unbound identifier` (website run
//! 45738701245, on all three systems). The example was the side that was wrong: every other
//! language's example builds the window with `create(layout)`.
//!
//! These tests read the example and the module and fail when the example calls a name that neither
//! of them defines.

use std::{collections::BTreeSet, sync::OnceLock};

use super::super::config::CodegenConfig;

fn generated() -> &'static str {
    static OUT: OnceLock<String> = OnceLock::new();
    OUT.get_or_init(|| {
        super::generate(crate::codegen::v2::bug_classes::ir(), &CodegenConfig::c_header()).expect("racket codegen")
    })
}

fn example() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/racket/hello-world.rkt");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
}

/// The identifier at the start of `s`.
fn ident(s: &str) -> String {
    s.chars().take_while(|c| !c.is_whitespace() && !"()[]{}\",;'`".contains(*c)).collect()
}

/// Every name the module defines: its procedures and values, its raw `define-azul` bindings, and
/// what each `define-cstruct` binds (`make-X`, `X?`, `X-pointer`, `X-field`, `set-X-field!`).
fn defined_names(src: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut lines = src.lines();
    while let Some(line) = lines.next() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("(define-cstruct _") {
            let name = ident(rest);
            for derived in [format!("make-{name}"), format!("{name}?"), format!("{name}-pointer"), format!("_{name}")] {
                out.insert(derived);
            }
            // `([field _type] ...)`, one field to a line, ended by a line of `))`.
            if t.trim_end().ends_with("))") {
                continue;
            }
            while let Some(next) = lines.next() {
                let f = next.trim();
                if let Some(field) = f.strip_prefix('[') {
                    let field = ident(field);
                    out.insert(format!("{name}-{field}"));
                    out.insert(format!("set-{name}-{field}!"));
                }
                if f == "))" {
                    break;
                }
            }
        } else if let Some(rest) = t.strip_prefix("(define (") {
            out.insert(ident(rest));
        } else if let Some(rest) = t.strip_prefix("(define-azul ") {
            out.insert(ident(rest));
        } else if let Some(rest) = t.strip_prefix("(define ") {
            if !rest.starts_with('(') {
                out.insert(ident(rest));
            }
        }
    }
    out
}

/// Every name in call position, `(name ...)`, outside strings and comments.
fn called_names(src: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in src.lines() {
        let chars: Vec<char> = line.chars().collect();
        let mut in_string = false;
        for (i, c) in chars.iter().enumerate() {
            match c {
                '"' => in_string = !in_string,
                ';' if !in_string => break,
                '(' if !in_string => {
                    let rest: String = chars[i + 1..].iter().collect();
                    let name = ident(&rest);
                    if !name.is_empty() && !name.starts_with(|c: char| c.is_ascii_digit() || c == '#') {
                        out.insert(name);
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// What `racket/base` gives the example, as far as it uses it.
const RACKET_BASE: [&str; 7] = ["define", "require", "set-box!", "unbox", "box", "add1", "number->string"];

#[test]
fn the_names_are_read_from_the_module() {
    let names = defined_names(generated());
    for expected in ["window-create-options-create", "AzDom_createBody", "AzLogicalSize-width", "set-AzLogicalSize-width!"] {
        assert!(names.contains(expected), "`{}` is not among the {} names read from the module", expected, names.len());
    }
}

#[test]
fn the_example_calls_only_names_the_module_or_the_example_defines() {
    let src = example();
    let module = defined_names(generated());
    let own = defined_names(&src);
    let unknown: Vec<String> = called_names(&src)
        .into_iter()
        .filter(|n| !RACKET_BASE.contains(&n.as_str()) && !own.contains(n) && !module.contains(n))
        .collect();
    assert!(
        unknown.is_empty(),
        "examples/racket/hello-world.rkt calls names that neither racket/base, the example nor the generated azul.rkt define: {:?}",
        unknown
    );
}
