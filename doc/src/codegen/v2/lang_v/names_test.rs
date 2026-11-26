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

use super::{snake_field, snake_fields, v_field_name};
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
fn a_field_name_that_starts_with_an_underscore_moves_it_to_the_end() {
    assert_eq!(v_field_name("_reserved"), "reserved_");
    assert_eq!(v_field_name("_abi_mut"), "abi_mut_");
    assert_eq!(v_field_name("__x"), "x_");
    assert_eq!(v_field_name("_0"), "f0");
    assert_eq!(v_field_name("_12"), "f12");
    assert_eq!(v_field_name("_"), "unnamed");
    assert_eq!(v_field_name("title"), "title");
    assert_eq!(v_field_name("type"), "type_");
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

/// V refuses a field name that starts with an underscore ("field name `_reserved` cannot start with
/// `_`", V 0.5.2: 27 members of azul.v - `_reserved`, `_abi_mut`, `_abi_ref` and the fields of tuple
/// structs, `_0`, `_1`, `_2`).
#[test]
fn no_field_name_of_the_module_starts_with_an_underscore() {
    let offenders: Vec<(usize, &str)> = generated()
        .lines()
        .enumerate()
        .filter(|(_, l)| l.starts_with('\t') && l.trim_start().starts_with('_'))
        .map(|(i, l)| (i + 1, l.trim()))
        .collect();
    assert!(
        offenders.is_empty(),
        "{} members of azul.v start with an underscore, which V refuses in a field name; the first: {:?}",
        offenders.len(),
        &offenders[..offenders.len().min(5)]
    );
}

/// With no C header to include, V 0.5.2 writes no C declaration for `fn C.AzString_fromUtf8(...)`
/// and the C compiler stops ("C function `C.AzString_fromUtf8` was declared in V, but the C
/// compiler did not see a matching C declaration", run 45764257231): the V declaration of a symbol
/// that a library provides carries `@[c_extern]`.
#[test]
fn every_c_function_the_module_declares_is_marked_extern() {
    let lines: Vec<&str> = generated().lines().collect();
    let mut declared = 0;
    let mut bare = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.starts_with("fn C.") {
            declared += 1;
            if i == 0 || lines[i - 1] != "@[c_extern]" {
                bare.push(*line);
            }
        }
    }
    assert!(declared > 10_000, "only {declared} `fn C.` declarations found: the scan does not read the module");
    assert!(
        bare.is_empty(),
        "{} of the {} C functions of azul.v are not marked `@[c_extern]`; the first: {:?}",
        bare.len(),
        declared,
        &bare[..bare.len().min(3)]
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

fn repo_file(rel: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
}

/// V 0.5.2 writes a C prototype for every `fn C.` declaration of azul.v (`@[c_extern]`) but leaves
/// out the types that only the functions a program does not call name, and gcc stops at the first
/// prototype: "unknown type name `azul__AzIconDesignedFor`" (run 45765384332). With
/// `-no-skip-unused` the whole module and the example compile and only the link wants libazul (run
/// 45766307680). Everything that builds a V program against the module says so.
const NO_SKIP: &str = "-no-skip-unused";

#[test]
fn the_header_of_the_module_says_to_build_with_no_skip_unused() {
    let head: Vec<&str> = generated().lines().take(40).collect();
    assert!(head.iter().any(|l| l.contains(NO_SKIP)), "the header of azul.v does not say {NO_SKIP}:\n{}", head.join("\n"));
}

#[test]
fn the_board_builds_the_v_example_with_no_skip_unused() {
    let script = repo_file("scripts/e2e_language_matrix.sh");
    let from = script.find("\nlang_v() {").expect("lang_v in the language board");
    let leg = &script[from..from + script[from..].find("\n}\n").expect("end of lang_v")];
    let build: Vec<&str> = leg.lines().filter(|l| l.trim_start().starts_with("v ") && l.contains("-o ")).collect();
    assert!(!build.is_empty(), "no `v ... -o` build command in lang_v:\n{leg}");
    assert!(build.iter().all(|l| l.contains(NO_SKIP)), "the build of lang_v does not say {NO_SKIP}: {build:?}");
}

#[test]
fn the_install_steps_run_the_v_example_with_no_skip_unused() {
    fn v(value: &serde_json::Value) -> Option<&serde_json::Value> {
        match value {
            serde_json::Value::Object(m) => {
                if m.get("displayName").and_then(|d| d.as_str()) == Some("V") && m.contains_key("install") {
                    return Some(value);
                }
                m.values().find_map(v)
            }
            serde_json::Value::Array(a) => a.iter().find_map(v),
            _ => None,
        }
    }
    let api: serde_json::Value = serde_json::from_str(&repo_file("api.json")).expect("api.json parses");
    let flows = v(&api).expect("api.json has the V install steps")["install"].as_array().expect("flows");
    assert!(flows.len() >= 3, "{} flows of V install steps", flows.len());
    for flow in flows {
        let runs: Vec<&str> = flow["steps"]
            .as_array()
            .expect("steps")
            .iter()
            .filter_map(|s| s["content"].as_str())
            .filter(|c| c.contains(" run .") || c.starts_with("v run"))
            .collect();
        assert!(!runs.is_empty(), "the V flow `{}` has no step that runs the example", flow["id"]);
        assert!(runs.iter().all(|c| c.contains(NO_SKIP)), "the V flow `{}` runs the example without {NO_SKIP}: {runs:?}", flow["id"]);
    }
}
