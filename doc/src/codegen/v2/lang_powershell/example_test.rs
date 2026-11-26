//! The PowerShell example against the C# the module embeds.
//!
//! `examples/powershell/hello-world.ps1` cast its layout script block to
//! `Func[IntPtr, IntPtr, object]` and handed it to `[Azul.HostInvoker]::RegisterLayoutCallback`,
//! which takes a `LayoutCallbackInvokerDelegate(ulong, IntPtr, IntPtr, IntPtr)`: the PowerShell leg
//! of the language board, once the module parsed, stopped at `Cannot find an overload for
//! "RegisterLayoutCallback" and the argument count: "1"` (run 45754551758, windows-2022). The
//! `WindowCreateOptions.Default()` and the `Azul.Native.AzButtonType` after it were as stale: the
//! binding has `CreateDefault()` and no `Azul.Native` namespace.
//!
//! The example now uses the typed delegates the binding generates for exactly this
//! (`LayoutCallbackWithData<T>`, `ButtonOnClickCallbackWithData<T>`). These tests read the example
//! and fail when a type, a static member or a method it names is not in the generated C#.

use std::{collections::BTreeSet, sync::OnceLock};

use super::super::config::CodegenConfig;

fn module() -> &'static str {
    static OUT: OnceLock<String> = OnceLock::new();
    OUT.get_or_init(|| {
        super::generate(crate::codegen::v2::bug_classes::ir(), &CodegenConfig::c_header()).expect("powershell codegen")
    })
}

fn example() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/powershell/hello-world.ps1");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
}

/// The text of a type of the `Azul` namespace (a class, a struct or an enum at four spaces of
/// indentation), from its name to the closing brace.
fn type_body<'a>(src: &'a str, name: &str) -> Option<&'a str> {
    for kind in ["class", "struct", "enum"] {
        let needle = format!(" {kind} {name}");
        let mut from = 0;
        while let Some(p) = src[from..].find(&needle) {
            let at = from + p;
            let line_start = src[..at].rfind('\n').map_or(0, |i| i + 1);
            let head = &src[line_start..at];
            let named = src[at + needle.len()..].chars().next().is_some_and(|c| c.is_whitespace() || c == ':' || c == '<');
            if named && head.starts_with("    public") && !head.starts_with("     ") {
                let end = src[at..].find("\n    }\n").map_or(src.len(), |e| at + e);
                return Some(&src[at..end]);
            }
            from = at + needle.len();
        }
    }
    None
}

/// The identifier at the start of `s`.
fn ident(s: &str) -> &str {
    let end = s.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(s.len());
    &s[..end]
}

/// Every `[Azul.Type]`, `[Azul.Outer+Inner[...]]` and `[Azul.Type]::member` of the example:
/// (type, nested type, static member).
fn type_literals(src: &str) -> Vec<(String, Option<String>, Option<String>)> {
    let mut out = Vec::new();
    let mut rest = src;
    while let Some(p) = rest.find("[Azul.") {
        let after = &rest[p + "[Azul.".len()..];
        let ty = ident(after).to_string();
        let mut tail = &after[ty.len()..];
        let mut nested = None;
        if let Some(t) = tail.strip_prefix('+') {
            let inner = ident(t).to_string();
            tail = &t[inner.len()..];
            nested = Some(inner);
        }
        // `[object]` after the name: generic arguments, up to the bracket that closes them.
        if tail.starts_with('[') {
            let mut depth = 0;
            let mut end = 0;
            for (i, c) in tail.char_indices() {
                match c {
                    '[' => depth += 1,
                    ']' => {
                        depth -= 1;
                        if depth == 0 {
                            end = i + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            tail = &tail[end..];
        }
        let member = tail.strip_prefix("]::").map(|m| ident(m).to_string()).filter(|m| !m.is_empty());
        out.push((ty, nested, member));
        rest = after;
    }
    out
}

#[test]
fn the_module_is_read() {
    let m = module();
    assert!(type_body(m, "HostInvoker").is_some_and(|b| b.contains("RegisterLayoutCallback")), "HostInvoker is not read from the module");
    assert!(type_body(m, "Update").is_some_and(|b| b.contains("RefreshDom")), "the Update enum is not read from the module");
    assert!(type_literals("x [Azul.Dom]::Create(1) [Azul.HostInvoker+Cb[object]] {")
        .contains(&("HostInvoker".to_string(), Some("Cb".to_string()), None)));
    assert!(type_literals("x [Azul.Dom]::Create(1)").contains(&("Dom".to_string(), None, Some("Create".to_string()))));
}

#[test]
fn every_type_and_static_member_the_example_names_is_in_the_module() {
    let m = module();
    let src = example();
    let literals = type_literals(&src);
    assert!(literals.len() >= 8, "only {} type literals found in the example", literals.len());
    let mut missing = BTreeSet::new();
    for (ty, nested, member) in literals {
        let Some(body) = type_body(m, &ty) else {
            missing.insert(format!("[Azul.{ty}]: no such type in the module"));
            continue;
        };
        if let Some(inner) = nested {
            // A delegate declared inside `HostInvoker`: `delegate Dom LayoutCallbackWithData<T>(...)`.
            let declared = body.contains(&format!(" {inner}<")) || body.contains(&format!(" {inner}("));
            if !declared {
                missing.insert(format!("[Azul.{ty}+{inner}]: {ty} declares no such delegate"));
            }
        }
        if let Some(member) = member {
            let declared = body.contains(&format!(" {member}(")) || body.contains(&format!(" {member}<")) || body.contains(&format!("\n        {member},"));
            if !declared {
                missing.insert(format!("[Azul.{ty}]::{member}: {ty} has no such member"));
            }
        }
    }
    assert!(missing.is_empty(), "examples/powershell/hello-world.ps1 names what the generated C# does not have: {:?}", missing);
}

#[test]
fn every_method_the_example_calls_exists_in_the_module() {
    let m = module();
    let src = example();
    let mut missing = BTreeSet::new();
    let mut calls = 0;
    for line in src.lines().filter(|l| !l.trim_start().starts_with('#')) {
        let mut rest = line;
        while let Some(p) = rest.find('.') {
            let after = &rest[p + 1..];
            let name = ident(after);
            let follows = &after[name.len()..];
            // `.Method(` and `.Method[object](`: a call.
            let is_call = follows.starts_with('(') || (follows.starts_with('[') && follows.contains("]("));
            if is_call && name.starts_with(|c: char| c.is_ascii_uppercase()) && name != "GetNewClosure" {
                calls += 1;
                if !(m.contains(&format!(" {name}(")) || m.contains(&format!(" {name}<"))) {
                    missing.insert(name.to_string());
                }
            }
            rest = after;
        }
    }
    assert!(calls >= 6, "only {calls} method calls found in the example");
    assert!(missing.is_empty(), "examples/powershell/hello-world.ps1 calls methods the generated C# does not define: {:?}", missing);
}
