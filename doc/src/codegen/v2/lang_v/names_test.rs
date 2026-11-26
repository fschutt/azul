//! The names of the V module: enum variants and union fields in snake_case, and the example that
//! spells them.
//!
//! V 0.5.2 refuses an uppercase letter in a field name - "field name `LineStart` cannot contain
//! uppercase letters, use snake_case instead" - and `azul.v` had one such error for each variant of
//! its 355 enums and each field of its unions (the V leg of the language board, website run
//! 45738701245, on all three systems: the compile stopped there). The example used the enum values
//! as they were (`azul.AzUpdate.DoNothing`) and called `azStr(...)`, which it never defines (`az_str`
//! is its helper).

use std::{collections::BTreeMap, sync::OnceLock};

use super::{snake_field, snake_fields};
use super::super::config::CodegenConfig;

fn generated() -> &'static str {
    static OUT: OnceLock<String> = OnceLock::new();
    OUT.get_or_init(|| {
        super::generate(crate::codegen::v2::bug_classes::ir(), &CodegenConfig::c_header()).expect("v codegen")
    })
}

fn example() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/v/hello-world.v");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
}

/// The members of every `pub enum` and every `pub union` block: block name -> member names.
fn blocks(src: &str) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    let mut current: Option<(String, Vec<String>)> = None;
    for line in src.lines() {
        if let Some(rest) = line.strip_prefix("pub enum ").or_else(|| line.strip_prefix("pub union ")) {
            let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            current = Some((name, Vec::new()));
        } else if line.starts_with('}') {
            if let Some((name, members)) = current.take() {
                out.insert(name, members);
            }
        } else if let Some((_, members)) = current.as_mut() {
            let t = line.trim();
            if !t.is_empty() && !t.starts_with("//") {
                members.push(t.split_whitespace().next().unwrap_or("").to_string());
            }
        }
    }
    out
}

#[test]
fn snake_field_spells_a_variant_in_lowercase_words() {
    assert_eq!(snake_field("DoNothing"), "do_nothing");
    assert_eq!(snake_field("RefreshDomAllWindows"), "refresh_dom_all_windows");
    assert_eq!(snake_field("Primary"), "primary");
    assert_eq!(snake_field("KeyA"), "key_a");
    assert_eq!(snake_field("F12"), "f12");
    assert_eq!(snake_field("Num0"), "num0");
    assert_eq!(snake_field("UTF8Mode"), "utf8_mode");
    assert_eq!(snake_field("HTTPRequest"), "http_request");
    // A word V reserves keeps its trailing underscore.
    assert_eq!(snake_field("Type"), "type_");
    assert_eq!(snake_field("None"), "none_");
}

#[test]
fn two_names_that_fold_to_one_word_stay_two_fields() {
    assert_eq!(snake_fields(["KeyA", "Key_A", "Other"]), ["key_a", "key_a_", "other"]);
}

#[test]
fn no_enum_variant_or_union_field_of_the_module_has_an_uppercase_letter() {
    let blocks = blocks(generated());
    assert!(blocks.len() > 300, "only {} enum and union blocks found: the scan does not read the module", blocks.len());
    let offenders: Vec<String> = blocks
        .iter()
        .flat_map(|(name, members)| {
            members.iter().filter(|m| m.chars().any(|c| c.is_ascii_uppercase())).map(move |m| format!("{name}.{m}"))
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "V refuses uppercase letters in a field name; {} members of azul.v have some, among them {:?}",
        offenders.len(),
        &offenders[..offenders.len().min(10)]
    );
}

#[test]
fn the_example_spells_the_enum_values_it_uses_as_the_module_does() {
    let module = blocks(generated());
    let src = example();
    let mut used = 0;
    for token in src.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.')) {
        // `azul.AzUpdate.refresh_dom`
        let Some(rest) = token.strip_prefix("azul.") else { continue };
        let Some((ty, value)) = rest.split_once('.') else { continue };
        let members = module.get(ty).unwrap_or_else(|| panic!("the example uses `{token}`: the module has no enum or union `{ty}`"));
        assert!(members.iter().any(|m| m == value), "the example uses `{token}`: `{ty}` has {members:?}");
        used += 1;
    }
    assert!(used >= 3, "the example names only {used} enum values: the scan does not read it");
}

/// Names V gives every program that the example calls.
const V_BUILTINS: [&str; 5] = ["isnil", "usize", "u64", "voidptr", "sizeof"];

/// Every unqualified `name(` of the source that is not a definition, a method or a type: what the
/// source calls as a free function.
fn free_calls(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in src.lines() {
        let line = line.split("//").next().unwrap_or("");
        let b = line.as_bytes();
        for (i, c) in line.char_indices() {
            if c != '(' {
                continue;
            }
            let start = line[..i]
                .char_indices()
                .rev()
                .find(|(_, ch)| !(ch.is_ascii_alphanumeric() || *ch == '_'))
                .map_or(0, |(p, ch)| p + ch.len_utf8());
            let name = &line[start..i];
            if name.is_empty() || name.starts_with(|ch: char| ch.is_ascii_uppercase() || ch.is_ascii_digit()) {
                continue;
            }
            // `.method(`, `C.AzDom_createBody(`, `azul.x(`: qualified; `fn name(`: a definition.
            if start > 0 && b[start - 1] == b'.' {
                continue;
            }
            if line[..start].trim_end().ends_with("fn") || matches!(name, "if" | "for" | "match" | "unsafe" | "return") {
                continue;
            }
            out.push(name.to_string());
        }
    }
    out
}

#[test]
fn the_example_calls_only_functions_it_defines() {
    let src = example();
    let defined: Vec<String> = src
        .lines()
        .filter_map(|l| l.trim_start().strip_prefix("fn "))
        .map(|rest| rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect())
        .collect();
    let unknown: Vec<String> = free_calls(&src)
        .into_iter()
        .filter(|n| !defined.contains(n) && !V_BUILTINS.contains(&n.as_str()))
        .collect();
    assert!(
        unknown.is_empty(),
        "examples/v/hello-world.v calls functions that neither V nor the example defines: {:?} (it defines {:?})",
        unknown,
        defined
    );
}
