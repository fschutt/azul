//! The hand-written Fortran of the repository against the binding the generator writes from the
//! real api.json.
//!
//! A type's constructors answer to the type's name (`app_t(model, config)`) only when the type
//! and kind of the arguments tell every one of them apart (`wrappers::generic_constructors`), so
//! adding a constructor to a class can take the `<type>_t(...)` spelling away from it. `Button`
//! lost it on 2026-09-29, when `create_reset` and `create_submit` joined `create` (all three take
//! one string), and `examples/fortran/hello_world.f90`, which built its button as
//! `button_t('Increase counter')`, stopped compiling (gfortran: "Cannot convert CHARACTER(16) to
//! TYPE(azbutton)": without a generic, `button_t(...)` is the structure constructor of the type)
//! while nothing in the repository looked at it. The language board that does is the last job of
//! a release, and it is shipped-tier: it gates the website. These tests look at every Fortran
//! source a person wrote - the examples, the programs the CSS compiler's goldens pin, every guide
//! page with a Fortran fence - so the next constructor added to any class cannot break one of them
//! silently.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use super::super::config::CodegenConfig;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// `azul_api.f90` generated from the real api.json.
fn api() -> &'static str {
    static OUT: OnceLock<String> = OnceLock::new();
    OUT.get_or_init(|| {
        let ir = crate::codegen::v2::bug_classes::ir();
        let all = super::generate(ir, &CodegenConfig::c_header()).expect("fortran codegen");
        let mut cur = false;
        let mut out = String::new();
        for line in all.lines() {
            if let Some(rest) = line.strip_prefix(super::FILE_MARKER) {
                cur = rest.trim_end_matches(super::END_MARKER).trim() == "azul_api.f90";
            } else if cur {
                out.push_str(line);
                out.push('\n');
            }
        }
        out
    })
}

/// The types of the binding that answer to their own name: `interface <type>_t`.
fn generic_constructors() -> BTreeSet<String> {
    api()
        .lines()
        .filter_map(|l| l.trim().strip_prefix("interface "))
        .filter(|n| n.ends_with("_t") && !n.contains(char::is_whitespace))
        .map(str::to_string)
        .collect()
}

/// Types the source declares itself (`type :: model_t`): their structure constructors are the
/// source's own.
fn declared_types(src: &str) -> BTreeSet<String> {
    src.lines()
        .filter_map(|l| l.trim().strip_prefix("type"))
        .filter_map(|rest| rest.split_once("::"))
        // `type(button_t) :: button` declares a variable, `type :: model_t` a type.
        .filter(|(head, _)| !head.trim_start().starts_with('('))
        .map(|(_, name)| name.trim().to_string())
        .collect()
}

/// Every `<name>_t(` the source spells, lower-cased (Fortran does not tell the cases apart).
fn type_name_calls(src: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut from = 0;
    while let Some(i) = src[from..].find("_t(") {
        let end = from + i + 2;
        from = end;
        let start = src[..end]
            .char_indices()
            .rev()
            .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_'))
            .map_or(0, |(p, c)| p + c.len_utf8());
        let name = &src[start..end];
        if name.len() > 2 && name.starts_with(|c: char| c.is_ascii_lowercase()) {
            out.insert(name.to_string());
        }
    }
    out
}

/// Every Fortran source a person wrote, lower-cased, by path from the repository root: the `.f90`
/// files, and the Markdown pages that show Fortran (they have a fence of it), prose included -
/// a page that teaches `button_t('x')` in a sentence teaches it.
fn hand_written_fortran() -> Vec<(String, String)> {
    let root = repo_root();
    let skipped = |name: &str| matches!(name, "target" | ".git" | "node_modules" | "third_party");
    let walk = walkdir::WalkDir::new(&root).into_iter().filter_entry(|e| {
        !(e.depth() > 0 && e.file_type().is_dir() && skipped(&e.file_name().to_string_lossy()))
    });
    let mut out = Vec::new();
    for entry in walk.filter_map(Result::ok).filter(|e| e.file_type().is_file()) {
        let ext = entry.path().extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        if !matches!(ext.as_str(), "f90" | "f95" | "f03" | "f08" | "md") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path()) else { continue };
        if ext == "md" && !text.contains("```fortran") {
            continue;
        }
        let rel = entry.path().strip_prefix(&root).unwrap_or(entry.path()).to_string_lossy().replace('\\', "/");
        out.push((rel, text.to_ascii_lowercase()));
    }
    out.sort();
    out
}

#[test]
fn the_scan_tells_a_type_name_call_from_a_declaration() {
    let src = "type :: model_t\n  integer :: counter\nend type model_t\n\
               type(button_t) :: button\nclass(dom_t), intent(in) :: d\n\
               button = button_t('x')\napp = app_t(model_t(counter=5), c)\n\
               select type (m)\ntype is (model_t)\n";
    assert_eq!(declared_types(src), BTreeSet::from(["model_t".to_string()]));
    assert_eq!(
        type_name_calls(src),
        BTreeSet::from(["app_t".to_string(), "button_t".to_string(), "model_t".to_string()])
    );
}

#[test]
fn the_scan_finds_the_example_the_guide_and_the_css_goldens() {
    let paths: Vec<String> = hand_written_fortran().into_iter().map(|(p, _)| p).collect();
    for want in [
        "examples/fortran/hello_world.f90",
        "doc/guide/en/hello-world/fortran.md",
        "css/tests/codegen_goldens/fortran/dom_app/main.f90",
    ] {
        assert!(paths.iter().any(|p| p == want), "{} is not among the {} Fortran sources found", want, paths.len());
    }
}

#[test]
fn every_type_name_call_in_hand_written_fortran_is_a_generic_the_binding_has() {
    let sources = hand_written_fortran();
    let own: BTreeSet<String> = sources.iter().flat_map(|(_, src)| declared_types(src)).collect();
    let generic = generic_constructors();
    let mut missing = Vec::new();
    for (path, src) in &sources {
        for name in type_name_calls(src) {
            if !own.contains(&name) && !generic.contains(&name) {
                missing.push(format!("{}: {}(...)", path, name));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "these calls name a type that has no `interface` in the generated azul_api.f90, so the compiler reads them as the \
         structure constructor of the type and rejects the arguments (a type has one only while every one of its \
         constructors takes different arguments: call the flat name, e.g. `button_create`):\n  {}",
        missing.join("\n  ")
    );
}

#[test]
fn the_guide_names_the_types_with_a_generic_and_the_types_without_one() {
    let generic = generic_constructors();
    for name in ["app_t", "window_create_options_t"] {
        assert!(generic.contains(name), "the guide says `{}` is a constructor, the binding has no such interface", name);
    }
    for name in ["button_t", "dom_t"] {
        assert!(!generic.contains(name), "the guide says `{}` has no generic, the binding has one: update the guide", name);
    }
}
