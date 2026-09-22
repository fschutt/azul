//! Bug-class tests for the language bindings.
//!
//! Every test here states ONE invariant over the whole API - every type,
//! every function, every shipped binding - so it catches a CLASS of defect,
//! not one instance of it. They were written from the 2026-09-19 bindings
//! audit (`scripts/audits/BINDINGS_HACK_AUDIT_2026_09_19.md`, finding ids in
//! each test's doc), and a failing test lists every offender, not the first.
//!
//! Three layers:
//!
//! * **IR invariants** - the IR built from `api.json` in memory: how types
//!   are classified, which functions exist, what callbacks can carry.
//! * **Emitter lint** - the emitter source itself: behaviour must never be
//!   keyed on the literal name of one API type or function.
//! * **Generated output** - what `azul-doc codegen all` wrote to
//!   `target/codegen`, per shipped binding. CI generates it before running
//!   these tests; locally a missing or stale output is refused (it would be
//!   a verdict about a different program), never silently skipped.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::OnceLock,
};

use super::ir::*;
use crate::api::{ApiData, ClassData, RefKind, VersionData};

// ============================================================================
// Shared fixtures
// ============================================================================

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn api() -> &'static ApiData {
    static API: OnceLock<ApiData> = OnceLock::new();
    API.get_or_init(|| {
        let src = std::fs::read_to_string(repo_root().join("api.json")).expect("api.json");
        ApiData::from_str(&src).expect("api.json parses")
    })
}

fn version() -> &'static VersionData {
    let a = api();
    a.get_version(a.get_latest_version_str().expect("a version"))
        .expect("latest version")
}

fn ir() -> &'static CodegenIR {
    static IR: OnceLock<CodegenIR> = OnceLock::new();
    IR.get_or_init(|| super::build_ir_from_api(api()).expect("IR builds"))
}

/// Every api.json class, by name.
fn classes() -> BTreeMap<&'static str, &'static ClassData> {
    version()
        .api
        .values()
        .flat_map(|m| m.classes.iter())
        .map(|(n, c)| (n.as_str(), c))
        .collect()
}

/// `(field name, type, ref kind)` of an api.json struct, in order.
fn fields(c: &ClassData) -> Vec<(&str, &str, RefKind)> {
    c.struct_fields
        .iter()
        .flatten()
        .flat_map(|m| m.iter())
        .map(|(k, v)| (k.as_str(), v.r#type.as_str(), v.ref_kind))
        .collect()
}

fn is_ptr(k: RefKind) -> bool {
    matches!(k, RefKind::ConstPtr | RefKind::MutPtr)
}

/// Panics with every offender, one per line.
fn assert_none(class: &str, offenders: impl IntoIterator<Item = String>) {
    let offenders: Vec<String> = offenders.into_iter().collect();
    if !offenders.is_empty() {
        panic!(
            "{class}: {} offender(s)\n  {}",
            offenders.len(),
            offenders.join("\n  ")
        );
    }
}

fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_lowercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

/// The trait functions' C suffixes (`delete`, `clone`, ...): names every
/// binding recognizes a function by.
fn trait_suffixes() -> BTreeSet<&'static str> {
    [
        FunctionKind::Delete,
        FunctionKind::DeepCopy,
        FunctionKind::PartialEq,
        FunctionKind::PartialCmp,
        FunctionKind::Cmp,
        FunctionKind::Hash,
        FunctionKind::Default,
        FunctionKind::DebugToString,
    ]
    .iter()
    .map(|k| k.c_suffix().trim_start_matches('_'))
    .collect()
}

/// Callback typedefs that have a `{fn, OptionRefAny}` wrapper struct, keyed by
/// typedef, with the wrapper's name. Computed from api.json, not the IR.
fn wrapper_of_typedef() -> BTreeMap<String, String> {
    let cls = classes();
    let typedefs: BTreeSet<&str> = cls
        .iter()
        .filter(|(_, c)| c.callback_typedef.is_some())
        .map(|(n, _)| *n)
        .collect();
    let mut out = BTreeMap::new();
    for (name, c) in &cls {
        let f = fields(c);
        let cbs: Vec<_> = f
            .iter()
            .filter(|(_, t, k)| *k == RefKind::Value && typedefs.contains(t))
            .collect();
        let ctx: Vec<_> = f
            .iter()
            .filter(|(_, t, k)| *k == RefKind::Value && *t == "OptionRefAny")
            .collect();
        if let ([cb], [_]) = (cbs.as_slice(), ctx.as_slice()) {
            out.insert(cb.1.to_string(), name.to_string());
        }
    }
    out
}

// ============================================================================
// IR invariants
// ============================================================================

/// S1/S2: `TypeCategory::Vec` is exactly the set of structs with the C Vec
/// layout (`ptr` pointer, `len`, `cap`, `destructor`) - no name list, and no
/// non-Vec struct overloading the category as a "uses the C API directly"
/// marker.
#[test]
fn vec_category_is_exactly_the_vec_layout() {
    let layout: BTreeSet<&str> = classes()
        .iter()
        .filter(|(_, c)| {
            let f = fields(c);
            let has = |n: &str| f.iter().any(|(k, _, _)| *k == n);
            f.len() == 4
                && f.iter().any(|(k, _, r)| *k == "ptr" && is_ptr(*r))
                && has("len")
                && has("cap")
                && has("destructor")
        })
        .map(|(n, _)| *n)
        .collect();
    let category: BTreeSet<&str> = ir()
        .structs
        .iter()
        .filter(|s| s.category == TypeCategory::Vec)
        .map(|s| s.name.as_str())
        .collect();
    assert_none(
        "TypeCategory::Vec != the ptr/len/cap/destructor layout",
        layout
            .difference(&category)
            .map(|n| format!("{n}: has the Vec layout but is not TypeCategory::Vec"))
            .chain(
                category
                    .difference(&layout)
                    .map(|n| format!("{n}: TypeCategory::Vec without the Vec layout")),
            ),
    );
}

/// S3: `TypeCategory::VecRef` is exactly the set of borrowed slices: a
/// `*VecRef` / `*VecRefMut` struct of a `ptr` pointer and a `len`.
#[test]
fn vecref_category_is_exactly_the_borrowed_slice_layout() {
    let layout: BTreeSet<&str> = classes()
        .iter()
        .filter(|(n, c)| {
            let f = fields(c);
            (n.ends_with("VecRef") || n.ends_with("VecRefMut"))
                && f.len() == 2
                && f.iter().any(|(k, _, r)| *k == "ptr" && is_ptr(*r))
                && f.iter().any(|(k, _, _)| *k == "len")
        })
        .map(|(n, _)| *n)
        .collect();
    let category: BTreeSet<&str> = ir()
        .structs
        .iter()
        .filter(|s| s.category == TypeCategory::VecRef)
        .map(|s| s.name.as_str())
        .collect();
    assert_none(
        "TypeCategory::VecRef != the *VecRef ptr/len layout",
        layout
            .difference(&category)
            .map(|n| format!("{n}: a borrowed slice that is not TypeCategory::VecRef"))
            .chain(
                category
                    .difference(&layout)
                    .map(|n| format!("{n}: TypeCategory::VecRef without the slice layout")),
            ),
    );
}

/// S4/S14: every variant of every (non-generic) enum has a constructor, and
/// no constructor takes a name the trait functions own (`_delete`, `_clone`,
/// `_hash`, ...) or `default`, the name the bindings give the `Default` impl.
#[test]
fn every_enum_variant_has_a_constructor_with_a_free_name() {
    let reserved = trait_suffixes();
    let ctors: BTreeSet<(&str, &str)> = ir()
        .functions
        .iter()
        .filter(|f| f.kind == FunctionKind::EnumVariantConstructor)
        .map(|f| (f.class_name.as_str(), f.method_name.as_str()))
        .collect();
    // A variant is constructible through its C symbol, whether the IR
    // generated it or api.json declares it by hand (`CssProperty.caret_color`
    // is `AzCssProperty_caretColor`, the name the generated one would take).
    let symbols: BTreeSet<&str> = ir().functions.iter().map(|f| f.c_name.as_str()).collect();
    let mut offenders = Vec::new();
    for e in ir().enums.iter().filter(|e| e.generic_params.is_empty()) {
        for v in &e.variants {
            let base = lower_first(&v.name);
            let found = symbols.contains(format!("Az{}_{base}", e.name).as_str())
                || symbols.contains(format!("Az{}_{base}Variant", e.name).as_str());
            if !found {
                offenders.push(format!("{}::{}: no variant constructor", e.name, v.name));
            }
        }
    }
    for (class, method) in &ctors {
        if reserved.contains(method) || *method == "default" {
            offenders.push(format!(
                "Az{class}_{method}: a variant constructor named like a trait function"
            ));
        }
    }
    assert_none("enum variant constructors", offenders);
}

/// S15: a generic alias (`CaretColorValue = CssPropertyValue<CaretColor>`)
/// without its own `derive` has exactly the traits its target has that every
/// type argument has too - `#[derive]` on a generic adds a `T: Trait` bound.
#[test]
fn generic_aliases_carry_their_targets_derives() {
    let cls = classes();
    let std_traits = |t: &str| -> Option<BTreeSet<&'static str>> {
        let ints = [
            "Debug", "Clone", "Copy", "PartialEq", "Eq", "PartialOrd", "Ord", "Hash", "Default",
        ];
        match t {
            "u8" | "u16" | "u32" | "u64" | "usize" | "i8" | "i16" | "i32" | "i64" | "isize"
            | "bool" | "char" => Some(ints.into_iter().collect()),
            "f32" | "f64" => Some(
                ["Debug", "Clone", "Copy", "PartialEq", "PartialOrd", "Default"]
                    .into_iter()
                    .collect(),
            ),
            _ => None,
        }
    };
    let own = |c: &ClassData| -> BTreeSet<String> {
        c.derive
            .iter()
            .flatten()
            .chain(c.custom_impls.iter().flatten())
            .cloned()
            .collect()
    };
    let mut offenders = Vec::new();
    for a in ir().type_aliases.iter().filter(|a| !a.generic_args.is_empty()) {
        let Some(c) = cls.get(a.name.as_str()) else { continue };
        if c.derive.is_some() || c.custom_impls.is_some() {
            continue;
        }
        let Some(target) = cls.get(a.target.trim()) else { continue };
        let args_have = |tr: &str| {
            a.generic_args.iter().all(|arg| match std_traits(arg.trim()) {
                Some(s) => s.contains(tr),
                None => cls.get(arg.trim()).is_some_and(|ac| own(ac).contains(tr)),
            })
        };
        let expected: BTreeSet<String> = own(target)
            .into_iter()
            .filter(|t| t != "Drop" && args_have(t))
            .collect();
        let t = &a.traits;
        let have: BTreeSet<String> = [
            ("Debug", t.is_debug),
            ("Clone", t.is_clone),
            ("Copy", t.is_copy),
            ("PartialEq", t.is_partial_eq),
            ("Eq", t.is_eq),
            ("PartialOrd", t.is_partial_ord),
            ("Ord", t.is_ord),
            ("Hash", t.is_hash),
            ("Default", t.is_default),
        ]
        .into_iter()
        .filter(|(_, on)| *on)
        .map(|(n, _)| n.to_string())
        .collect();
        let checked = [
            "Debug", "Clone", "Copy", "PartialEq", "Eq", "PartialOrd", "Ord", "Hash", "Default",
        ];
        let expected: BTreeSet<String> = expected
            .into_iter()
            .filter(|t| checked.contains(&t.as_str()))
            .collect();
        if expected != have {
            let missing: Vec<_> = expected.difference(&have).cloned().collect();
            let extra: Vec<_> = have.difference(&expected).cloned().collect();
            offenders.push(format!(
                "{} = {}<{}>: missing {:?}, extra {:?}",
                a.name,
                a.target,
                a.generic_args.join(", "),
                missing,
                extra
            ));
        }
    }
    assert_none("generic alias derives", offenders);
}

/// S12: a callback wrapper is recognized by its shape - exactly one callback
/// typedef field and exactly one `OptionRefAny` field - whatever it is named.
#[test]
fn callback_wrappers_are_recognised_by_structure() {
    let oracle = wrapper_of_typedef();
    let wrappers: BTreeSet<&str> = oracle.values().map(String::as_str).collect();
    let mut offenders = Vec::new();
    for s in &ir().structs {
        let is = wrappers.contains(s.name.as_str());
        match (&s.callback_wrapper_info, is) {
            (None, true) => offenders.push(format!(
                "{}: has the {{callback, OptionRefAny}} shape but no callback_wrapper_info",
                s.name
            )),
            (Some(_), false) => offenders.push(format!(
                "{}: callback_wrapper_info without the {{callback, OptionRefAny}} shape",
                s.name
            )),
            _ => {}
        }
    }
    assert_none("callback wrapper recognition", offenders);
}

/// The callback typedefs an application hands to libazul: the type of an API
/// function argument, or the function of a callback wrapper. The rest are
/// libazul's own destructors/cloners (`*VecDestructorType`, ...).
fn user_facing_typedefs() -> BTreeSet<String> {
    let typedefs: BTreeSet<&str> = ir()
        .callback_typedefs
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    let mut out = BTreeSet::new();
    for f in &ir().functions {
        if f.kind.is_trait_function() || f.kind == FunctionKind::EnumVariantConstructor {
            continue;
        }
        for a in &f.args {
            let t = a.type_name.trim();
            if typedefs.contains(t) {
                out.insert(t.to_string());
            }
        }
    }
    // A wrapper's own typedef is user-facing by definition.
    out.extend(wrapper_of_typedef().into_keys());
    out
}

/// S7 (API side): every callback an application can register has a wrapper
/// struct with an `OptionRefAny` context, so a managed binding (whose
/// closures are host objects, not C function pointers) can carry one.
#[test]
fn every_user_facing_callback_typedef_has_a_ctx_carrying_wrapper() {
    let wrapped = wrapper_of_typedef();
    // A typedef whose own arguments carry the caller's context (the data
    // `RefAny`, an `OptionRefAny`, or a raw data pointer handed back
    // verbatim) needs no wrapper: the host handle travels in that argument.
    let carries_context = |t: &str| {
        ir().callback_typedefs.iter().find(|c| c.name == t).is_some_and(|c| {
            c.args.iter().any(|a| {
                let ty = a.type_name.trim();
                ty == "RefAny" || ty == "OptionRefAny" || ty.contains("c_void")
            })
        })
    };
    assert_none(
        "user-facing callback typedefs that cannot carry a host context",
        user_facing_typedefs()
            .into_iter()
            .filter(|t| !wrapped.contains_key(t) && !carries_context(t))
            .map(|t| format!("{t}: no wrapper struct and no context argument, so no managed binding can pass a closure")),
    );
}

/// Every `impl_managed_callback!` site in the engine, by wrapper name: the
/// callback kinds libazul has a host-invoker thunk (and
/// `Az<Kind>_createFromHostHandle`, `AzApp_set<Kind>Invoker`) for.
fn engine_thunk_kinds() -> BTreeSet<String> {
    let mut engine = BTreeSet::new();
    for dir in ["core/src", "layout/src", "dll/src"] {
        for entry in walkdir::WalkDir::new(repo_root().join(dir))
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "rs"))
        {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // The macro definition and its own test fakes.
            if name == "host_invoker.rs" || name == "host_invoker_test.rs" {
                continue;
            }
            let Ok(src) = std::fs::read_to_string(path) else { continue };
            let mut rest = src.as_str();
            while let Some(i) = rest.find("impl_managed_callback!") {
                rest = &rest[i + "impl_managed_callback!".len()..];
                // A macro site opens with `{` and names its wrapper first;
                // prose that merely mentions the macro does not.
                let body = rest.trim_start();
                let Some(body) = body.strip_prefix('{') else { continue };
                let Some(after) = body.trim_start().strip_prefix("wrapper:") else { continue };
                let ident: String = after
                    .trim_start()
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                if !ident.is_empty() {
                    engine.insert(ident);
                }
            }
        }
    }
    engine
}

/// S5: libazul has a host-invoker thunk for exactly the callback wrappers
/// (by structure). A wrapper without one cannot carry a managed closure: its
/// kind silently returns the default in every managed binding. The kinds the
/// codegen emits registration for are the same set, derived from the IR.
#[test]
fn every_callback_wrapper_is_host_invokable() {
    let wrappers: BTreeSet<String> = wrapper_of_typedef().into_values().collect();
    let engine = engine_thunk_kinds();
    let codegen: BTreeSet<String> = super::managed_host_invoker::host_invoker_kinds(ir())
        .map(|cb| super::managed_host_invoker::wrapper_name(cb).to_string())
        .collect();
    assert_none(
        "callback wrappers vs host-invoker thunks",
        wrappers
            .difference(&engine)
            .map(|w| format!("{w}: no impl_managed_callback! site, managed bindings cannot register a closure for it"))
            .chain(
                engine
                    .difference(&wrappers)
                    .map(|w| format!("{w}: engine thunk for a type that is not a callback wrapper")),
            )
            .chain(
                wrappers
                    .symmetric_difference(&codegen)
                    .map(|w| format!("{w}: codegen's host-invoker kinds disagree with the wrapper structs")),
            ),
    );
}

/// S7 (IR side): an argument is a callback argument exactly when its type is
/// a callback wrapper (by structure) or the typedef one wraps - in both
/// directions, so a name that merely looks like a callback is not one - and
/// its `callback_info` names that wrapper and typedef.
#[test]
fn callback_arguments_are_recognised_by_type() {
    let wrapped = wrapper_of_typedef();
    let typedef_of: BTreeMap<&str, &str> =
        wrapped.iter().map(|(t, w)| (w.as_str(), t.as_str())).collect();
    let mut offenders = Vec::new();
    for f in &ir().functions {
        if f.kind.is_trait_function() || f.kind == FunctionKind::EnumVariantConstructor {
            continue;
        }
        for a in &f.args {
            let t = a.type_name.trim();
            if f.is_receiver_arg(a) || f.class_name == t {
                continue; // a method ON the wrapper, not a callback argument
            }
            let expect = match (wrapped.get(t), typedef_of.get(t)) {
                (Some(w), _) => Some((t, w.as_str())),
                (_, Some(td)) => Some((*td, t)),
                _ => None,
            };
            match (expect, &a.callback_info) {
                (Some(_), None) => offenders.push(format!(
                    "{}: argument `{}: {t}` has no callback_info",
                    f.c_name, a.name
                )),
                (None, Some(ci)) => offenders.push(format!(
                    "{}: argument `{}: {t}` is not a callback, but has callback_info ({} / {})",
                    f.c_name, a.name, ci.callback_typedef_name, ci.callback_wrapper_name
                )),
                (Some((td, w)), Some(ci))
                    if ci.callback_typedef_name != td || ci.callback_wrapper_name != w =>
                {
                    offenders.push(format!(
                        "{}: argument `{}: {t}` has callback_info {} / {}, the structure says {td} / {w}",
                        f.c_name, a.name, ci.callback_typedef_name, ci.callback_wrapper_name
                    ))
                }
                _ => {}
            }
        }
    }
    assert_none("callback arguments", offenders);
}

/// S5 (engine side): engine code calls a callback wrapper only through its
/// macro-generated `invoke`, which hands the callee the wrapper's context (the
/// info argument and the invocation slot). `(w.cb)(..)` on a wrapper skips
/// that: a managed-language callback returns its default without ever reaching
/// the host - VirtualView callbacks did, for every managed binding.
///
/// A direct `(<receiver>.cb)(..)` is fine when every api.json struct field and
/// function argument named like the receiver's last segment holds a type that
/// is NOT a callback wrapper (a clock function, a destructor). A receiver
/// api.json does not name needs a `direct-cb-call: <why>` comment on that line
/// or one of the three above.
#[test]
fn engine_calls_callback_wrappers_through_invoke() {
    // A wrapper, or a typedef one holds (the IR presents such an argument as
    // its wrapper, see `no_api_function_drops_a_callback_context`).
    let wrappers: BTreeSet<String> = wrapper_of_typedef()
        .into_iter()
        .flat_map(|(typedef, wrapper)| [typedef, wrapper])
        .collect();
    // name -> whether some api.json field or argument of that name is a wrapper
    let mut field_is_wrapper: BTreeMap<&str, bool> = BTreeMap::new();
    for c in classes().values() {
        for (name, ty, _) in fields(c) {
            *field_is_wrapper.entry(name).or_insert(false) |= wrappers.contains(ty);
        }
        for f in c.constructors.iter().chain(c.functions.iter()).flat_map(|m| m.values()) {
            for (name, ty) in f.fn_args.iter().flat_map(|m| m.iter()) {
                if name == "self" {
                    continue;
                }
                let ty = ty.trim_start_matches('&').trim_start_matches("mut ").trim();
                *field_is_wrapper.entry(name.as_str()).or_insert(false) |= wrappers.contains(ty);
            }
        }
    }
    let mut offenders = Vec::new();
    for dir in ["core/src", "layout/src", "dll/src"] {
        for entry in walkdir::WalkDir::new(repo_root().join(dir))
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "rs"))
        {
            let path = entry.path();
            let file = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if file.ends_with("_test.rs") || path.components().any(|c| c.as_os_str() == "tests") {
                continue;
            }
            let Ok(src) = std::fs::read_to_string(path) else { continue };
            // Unit tests call callbacks directly on purpose.
            let src = match src.find("\n#[cfg(test)]") {
                Some(i) => &src[..i],
                None => src.as_str(),
            };
            let lines: Vec<&str> = src.lines().collect();
            let mut at = 0;
            while let Some(i) = src[at..].find(".cb)(") {
                let pos = at + i;
                at = pos + 1;
                let line_no = src[..pos].matches('\n').count();
                if lines[line_no].trim_start().starts_with("//") {
                    continue;
                }
                let marked = (line_no.saturating_sub(3)..=line_no)
                    .any(|l| lines.get(l).is_some_and(|t| t.contains("direct-cb-call:")));
                if marked {
                    continue;
                }
                // The receiver's last segment, across line breaks: `x.field`
                // or a bare local `x`.
                let before = src[..pos].trim_end();
                let ident: String = before
                    .chars()
                    .rev()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                let is_field = before[..before.len() - ident.len()].trim_end().ends_with('.');
                let verdict = if ident.is_empty() { None } else { field_is_wrapper.get(ident.as_str()).copied() };
                let _ = is_field;
                let rel = path.strip_prefix(repo_root()).unwrap_or(path).display().to_string();
                match verdict {
                    Some(false) => {}
                    Some(true) => offenders.push(format!(
                        "{rel}:{}: `.{ident}.cb` is a callback wrapper - call `.{ident}.invoke(..)`",
                        line_no + 1
                    )),
                    None => offenders.push(format!(
                        "{rel}:{}: `{ident}.cb` called directly - use `invoke` for a callback wrapper, \
                         or say why not in a `direct-cb-call:` comment",
                        line_no + 1
                    )),
                }
            }
        }
    }
    assert_none("direct callback-wrapper calls in engine code", offenders);
}

/// The API functions of `ir()` whose callback arguments the shadow C API
/// covers: every constructor/method except those of the wrapper itself
/// (`Callback::create(cb)` BUILDS the wrapper), with each such argument's
/// wrapper.
fn callback_taking_functions() -> Vec<(&'static FunctionDef, Vec<String>)> {
    let wrapped = wrapper_of_typedef();
    let wrappers: BTreeSet<&str> = wrapped.values().map(String::as_str).collect();
    let mut out = Vec::new();
    for f in &ir().functions {
        if !matches!(
            f.kind,
            FunctionKind::Constructor
                | FunctionKind::StaticMethod
                | FunctionKind::Method
                | FunctionKind::MethodMut
        ) {
            continue;
        }
        let cbs: Vec<String> = f
            .args
            .iter()
            .filter(|a| !f.is_receiver_arg(a))
            .filter_map(|a| {
                let t = a.type_name.trim();
                if wrappers.contains(t) {
                    Some(t.to_string())
                } else {
                    wrapped.get(t).cloned()
                }
            })
            .filter(|w| *w != f.class_name)
            .collect();
        if !cbs.is_empty() {
            out.push((f, cbs));
        }
    }
    out
}

/// S17 (generated side): every API function that takes a callback - typed as
/// its wrapper, or as the raw typedef api.json declares for a generic
/// `C: Into<Wrapper>` source argument - is exported with the shadow C API
/// beside it: `<fn>WithCtx` (function pointer + context) and `<fn>Struct` (the
/// whole wrapper), so every binding can hand libazul a closure's context.
#[test]
fn every_callback_argument_has_the_ctx_shadow_api() {
    let exports = exported_functions();
    let mut offenders = Vec::new();
    for (f, wrappers) in callback_taking_functions() {
        let missing: Vec<String> = ["WithCtx", "Struct"]
            .iter()
            .map(|s| format!("{}{s}", f.c_name))
            .filter(|n| !exports.contains(n))
            .collect();
        if !missing.is_empty() {
            offenders.push(format!(
                "{} (takes {}): azul.h lacks {}",
                f.c_name,
                wrappers.join(", "),
                missing.join(", ")
            ));
        }
    }
    assert_none("callback-taking functions without the ctx shadow API", offenders);
}

/// S17 (IR side): the IR presents every callback argument of an API function
/// as its WRAPPER, whatever api.json declares - a raw typedef there comes
/// from a generic `C: Into<Wrapper>` source argument, whose body accepts the
/// wrapper too - so every emitter's raw / `WithCtx` / `Struct` path applies
/// and no binding is left with only the context-free function pointer. The
/// wrapper's own constructor (`Callback::create(cb)`, which builds a
/// context-free wrapper from a C function) is the one exception.
#[test]
fn no_api_function_drops_a_callback_context() {
    let wrapped = wrapper_of_typedef();
    let mut offenders = Vec::new();
    for f in &ir().functions {
        if f.kind.is_trait_function() || f.kind == FunctionKind::EnumVariantConstructor {
            continue;
        }
        for a in &f.args {
            let t = a.type_name.trim();
            if let Some(w) = wrapped.get(t) {
                if &f.class_name != w {
                    offenders.push(format!(
                        "{}: `{}: {t}` - take `{w}` so the host context survives",
                        f.c_name, a.name
                    ));
                }
            }
        }
    }
    assert_none("raw callback typedef arguments", offenders);
}

/// S6: the size a managed binding copies back for a callback's return value
/// is the C size of that type, for every host-invoker kind.
#[test]
fn callback_return_sizes_are_the_c_layout() {
    let mut offenders = Vec::new();
    for cb in super::managed_host_invoker::host_invoker_kinds(ir()) {
        let Some(rt) = cb.return_type.as_deref().map(str::trim).filter(|r| *r != "void") else {
            continue;
        };
        let want = super::lang_fortran::layout::type_layout(rt, ir()).map(|l| l.size);
        let got = super::managed_host_invoker::return_c_size(cb, ir());
        if want != got {
            offenders.push(format!("{}: returns {rt}, C size {want:?}, return_c_size {got:?}", cb.name));
        }
    }
    assert_none("callback return sizes", offenders);
}

// ============================================================================
// Emitter lint
// ============================================================================

/// The emitter source files of the shipped bindings, plus the shared code
/// every emitter inherits. Driven by `docgen::SHIPPED_LANGUAGES`: a shipped
/// language without an entry here fails the lint.
fn emitter_sources() -> BTreeMap<String, Vec<PathBuf>> {
    let v2 = repo_root().join("doc/src/codegen/v2");
    let rs_files = |p: PathBuf| -> Vec<PathBuf> {
        if p.is_file() {
            return vec![p];
        }
        walkdir::WalkDir::new(p)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "rs"))
            .map(|e| e.path().to_path_buf())
            .collect()
    };
    let mut out = BTreeMap::new();
    out.insert(
        "shared".to_string(),
        [
            "ir.rs",
            "ir_builder.rs",
            "managed_host_invoker.rs",
            "managed_lang_helpers.rs",
            "transmute_helpers.rs",
            "module_plan.rs",
            "config.rs",
            "generator.rs",
            "mod.rs",
        ]
        .iter()
        .flat_map(|f| rs_files(v2.join(f)))
        .collect(),
    );
    for lang in crate::docgen::SHIPPED_LANGUAGES {
        let paths: Vec<PathBuf> = match *lang {
            "c" => vec![v2.join("lang_c.rs")],
            "python" => vec![v2.join("lang_python.rs")],
            "rust" => vec![v2.join("lang_rust.rs"), v2.join("lang_reexports.rs"), v2.join("rust")],
            // Scala has no emitter of its own: it uses the Java binding.
            "scala" => continue,
            other => vec![v2.join(format!("lang_{other}"))],
        };
        let files: Vec<PathBuf> = paths.into_iter().flat_map(rs_files).collect();
        assert!(!files.is_empty(), "shipped language `{lang}` has no emitter sources");
        out.insert(lang.to_string(), files);
    }
    out
}

/// One ordinary string literal: its line, its text, and the source text on
/// the same line just before and just after it.
struct Literal {
    line: usize,
    text: String,
    before: String,
    after: String,
}

/// Every ordinary string literal in Rust source, skipping comments, raw
/// strings (the runtime templates an emitter writes out) and char literals.
fn string_literals(src: &str) -> Vec<Literal> {
    let b = src.as_bytes();
    let (mut i, mut line) = (0usize, 1usize);
    let mut out = Vec::new();
    while i < b.len() {
        match b[i] {
            b'\n' => {
                line += 1;
                i += 1;
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    if b[i] == b'\n' {
                        line += 1;
                    }
                    i += 1;
                }
                i += 2;
            }
            b'r' if (b.get(i + 1) == Some(&b'"') || b.get(i + 1) == Some(&b'#'))
                && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_')) =>
            {
                let mut j = i + 1;
                let mut hashes = 0;
                while b.get(j) == Some(&b'#') {
                    hashes += 1;
                    j += 1;
                }
                if b.get(j) != Some(&b'"') {
                    i += 1;
                    continue;
                }
                j += 1;
                let close: Vec<u8> = std::iter::once(b'"')
                    .chain(std::iter::repeat_n(b'#', hashes))
                    .collect();
                while j < b.len() && !b[j..].starts_with(&close) {
                    if b[j] == b'\n' {
                        line += 1;
                    }
                    j += 1;
                }
                i = j + close.len();
            }
            b'\'' => {
                // A char literal ('x', '\n', '\'') or a lifetime ('a).
                if b.get(i + 1) == Some(&b'\\') {
                    i += 2;
                    while i < b.len() && b[i] != b'\'' {
                        i += 1;
                    }
                    i += 1;
                } else if b.get(i + 2) == Some(&b'\'') {
                    i += 3;
                } else {
                    i += 1;
                }
            }
            b'"' => {
                let start_line = line;
                let mut j = i + 1;
                let mut s = String::new();
                while j < b.len() && b[j] != b'"' {
                    if b[j] == b'\\' {
                        j += 1;
                    }
                    if b[j] == b'\n' {
                        line += 1;
                    }
                    s.push(b[j] as char);
                    j += 1;
                }
                let line_start = src[..i].rfind('\n').map_or(0, |p| p + 1);
                let line_end = src[j.min(src.len())..].find('\n').map_or(src.len(), |p| j + p);
                out.push(Literal {
                    line: start_line,
                    text: s,
                    before: src[line_start..i].to_string(),
                    after: src[(j + 1).min(line_end)..line_end].to_string(),
                });
                i = j + 1;
            }
            _ => i += 1,
        }
    }
    out
}

/// Names that identify ONE API item: every class name, every `Az<Class>_*`
/// C symbol, and every method name fewer than 5 classes share (a name like
/// `create` or `default` is a convention every class follows, not one item).
fn specific_api_names() -> (BTreeSet<String>, BTreeSet<String>) {
    let class_names: BTreeSet<String> = classes().keys().map(|s| s.to_string()).collect();
    let mut method_classes: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for f in &ir().functions {
        if f.kind.is_trait_function() || f.kind == FunctionKind::EnumVariantConstructor {
            continue;
        }
        method_classes
            .entry(f.method_name.as_str())
            .or_default()
            .insert(f.class_name.as_str());
    }
    // Words every language uses for its own types and keywords.
    const COMMON: &[&str] = &[
        "bool", "char", "str", "string", "int", "float", "double", "void", "auto", "long",
        "short", "byte", "u8", "u16", "u32", "u64", "i8", "i16", "i32", "i64", "f32", "f64",
        "usize", "isize", "none", "some", "true", "false", "null", "nil", "self", "type",
        "object", "union", "struct", "enum", "class", "match", "new", "get", "set", "len",
        "ptr", "cap", "data", "value", "name", "size", "index", "item", "list", "map",
    ];
    let methods = method_classes
        .into_iter()
        .filter(|(m, c)| c.len() < 5 && m.len() > 2 && !COMMON.contains(m))
        .map(|(m, _)| m.to_string())
        .collect();
    (class_names, methods)
}

/// The emitters must never decide behaviour by the literal name of one API
/// type, function or C symbol (`== "Button"`, `["U8Vec", "StringVec"]`,
/// `"AzApp_run"`, `"with_child"`): that is how a binding ends up working for
/// the hello-world and nothing else, and how it silently breaks when api.json
/// renames or adds a type. Derive the decision from the IR (category, shape,
/// traits) instead.
///
/// The few places that must name an API item (the IR itself defining what
/// `String` and `RefAny` are) carry `allow-api-name: <reason>` on the line or
/// the line above, so every exception is visible and reviewed.
#[test]
fn emitters_never_key_behaviour_on_api_names() {
    let (class_names, methods) = specific_api_names();
    let symbol = |s: &str| {
        s.strip_prefix("Az").is_some_and(|rest| {
            rest.split_once('_').is_some_and(|(c, f)| {
                class_names.contains(c) && f.chars().next().is_some_and(|ch| ch.is_ascii_lowercase())
            })
        })
    };
    let mut offenders = Vec::new();
    for (lang, files) in emitter_sources() {
        for path in files {
            let Ok(src) = std::fs::read_to_string(&path) else { continue };
            let lines: Vec<&str> = src.lines().collect();
            // A marker covers the next line of CODE (comment lines in between
            // are part of the reason, which is usually more than one line),
            // and everything up to the matching close if that line opens a
            // block or a list: some exceptions are a whole table rather than
            // one decision - a prelude is a curated set of names by
            // definition, a C-type-to-native-type map names C types by nature
            // - and marking every line of one would bury the reason.
            // A comment never nests anything - counting the brackets in the
            // reason's own prose would end the block it is introducing.
            let depth_of = |line: &str| {
                if line.trim_start().starts_with("//") {
                    return 0;
                }
                line.chars().filter(|c| matches!(c, '[' | '(' | '{')).count() as i32
                    - line.chars().filter(|c| matches!(c, ']' | ')' | '}')).count() as i32
            };
            let mut block_allowed: BTreeSet<usize> = BTreeSet::new();
            for (i, line) in lines.iter().enumerate() {
                if !line.contains("allow-api-name") {
                    continue;
                }
                // Walk to the first line that is not a comment.
                let mut j = i;
                let mut depth = depth_of(line);
                while depth == 0 && j + 1 < lines.len() {
                    let next = lines[j + 1].trim_start();
                    if !(next.starts_with("//") || next.is_empty()) {
                        break;
                    }
                    j += 1;
                }
                if j + 1 < lines.len() {
                    block_allowed.insert(j + 2);
                    depth += depth_of(lines[j + 1]);
                }
                // Then to the close of whatever that line opened.
                let mut k = j + 1;
                while depth > 0 && k + 1 < lines.len() {
                    k += 1;
                    block_allowed.insert(k + 1);
                    depth += depth_of(lines[k]);
                }
            }
            let allowed = |line: usize| {
                let here = lines.get(line.wrapping_sub(1)).copied().unwrap_or("");
                let above = lines.get(line.wrapping_sub(2)).copied().unwrap_or("");
                here.contains("allow-api-name")
                    || above.contains("allow-api-name")
                    || block_allowed.contains(&line)
            };
            // Test modules may name concrete items: they check real api.json data.
            let test_start = src.find("#[cfg(test)]").map_or(usize::MAX, |p| src[..p].lines().count());
            for Literal { line, text: lit, before, after } in string_literals(&src) {
                if line > test_start || allowed(line) {
                    continue;
                }
                // A one-word method name (`dom`, `log`) is also an ordinary
                // word; it only keys behaviour when compared against.
                let compared = {
                    let b = before.trim_end();
                    let a = after.trim_start();
                    b.ends_with("==")
                        || b.ends_with("!=")
                        || b.ends_with("&")
                        || b.ends_with('|')
                        || before.contains("matches!(")
                        || a.starts_with("=>")
                        || a.starts_with("==")
                        || a.starts_with('|')
                };
                let why = if class_names.contains(&lit) {
                    "an API type name"
                } else if symbol(&lit) {
                    "an API C symbol"
                } else if methods.contains(&lit) && (lit.contains('_') || compared) {
                    "an API method name"
                } else {
                    continue;
                };
                let rel = path.strip_prefix(repo_root()).unwrap_or(&path).display().to_string();
                offenders.push(format!("[{lang}] {rel}:{line}: \"{lit}\" is {why}"));
            }
        }
    }
    assert_none("emitter behaviour keyed on API names", offenders);
}

// ============================================================================
// Generated output
// ============================================================================

fn codegen_dir() -> PathBuf {
    repo_root().join("target").join("codegen")
}

/// The newest modification time of any emitter source: a generated file older
/// than this was produced by a program that no longer exists.
fn newest_emitter_mtime() -> Option<std::time::SystemTime> {
    walkdir::WalkDir::new(repo_root().join("doc/src/codegen"))
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "rs"))
        // These tests do not produce any output; editing them must not make
        // every generated file look stale.
        .filter(|e| e.path().file_name().is_some_and(|n| n != "bug_classes.rs"))
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .chain(std::fs::metadata(repo_root().join("api.json")).ok()?.modified().ok())
        .max()
}

/// A generated file's text. A missing file, or (outside CI, where the
/// checkout's mtimes are meaningless) one older than the emitter sources, is a
/// test failure with the command that fixes it - never a silent pass.
fn generated(rel: &str) -> String {
    let path = codegen_dir().join(rel);
    let modified = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .unwrap_or_else(|_| {
            panic!(
                "{} is missing: run `cargo run -r -p azul-doc -- codegen all` first",
                path.display()
            )
        });
    if std::env::var_os("CI").is_none() && newest_emitter_mtime().is_some_and(|t| t > modified) {
        panic!(
            "{} is older than the codegen sources or api.json: run `cargo run -r -p azul-doc -- \
             codegen all` first (a verdict on a stale output is about a different program)",
            path.display()
        );
    }
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Every file under `dir` (relative to target/codegen) with one of `exts`.
fn generated_tree(dir: &str, exts: &[&str]) -> Vec<(String, String)> {
    let root = codegen_dir().join(dir);
    let mut out: Vec<(String, String)> = walkdir::WalkDir::new(&root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| exts.contains(&x))
        })
        .map(|e| {
            let rel = e.path().strip_prefix(codegen_dir()).unwrap().display().to_string();
            (rel.clone(), generated(&rel))
        })
        .collect();
    out.sort();
    assert!(!out.is_empty(), "no generated {exts:?} files under target/codegen/{dir}");
    out
}

/// The files that make up each shipped binding, from `docgen::SHIPPED_LANGUAGES`.
/// A shipped language this function does not know fails every output test.
fn shipped_outputs() -> BTreeMap<&'static str, Vec<(String, String)>> {
    let one = |rel: &str| vec![(rel.to_string(), generated(rel))];
    let mut out = BTreeMap::new();
    for lang in crate::docgen::SHIPPED_LANGUAGES {
        let files = match *lang {
            "c" => one("azul.h"),
            "cpp" => ["azul03.hpp", "azul11.hpp", "azul14.hpp", "azul17.hpp", "azul20.hpp", "azul23.hpp"]
                .iter()
                .flat_map(|f| one(f))
                .collect(),
            "rust" => one("dll_api_external.rs")
                .into_iter()
                .chain(one("reexports.rs"))
                .collect(),
            "python" => one("python_api.rs"),
            "csharp" => one("Azul.cs"),
            "java" | "scala" => generated_tree("java", &["java"]),
            "kotlin" => one("kotlin/Azul.kt"),
            "lua" => one("azul.lua"),
            "ruby" => one("azul.rb"),
            "node" => one("node/azul.js"),
            "ocaml" => generated_tree("ocaml", &["ml"]),
            "zig" => one("azul.zig"),
            "go" => generated_tree("go", &["go"]),
            "pascal" => one("azul.pas"),
            "fortran" => generated_tree("fortran", &["f90"]),
            "haskell" => generated_tree("haskell", &["hs", "c"]),
            "d" => one("azul.d"),
            "crystal" => one("azul.cr"),
            "swift" => one("azul.swift"),
            other => panic!("shipped language `{other}` has no entry in bug_classes::shipped_outputs"),
        };
        out.insert(*lang, files);
    }
    out
}

/// Every function libazul exports, from the generated C header.
fn exported_functions() -> BTreeSet<String> {
    let h = generated("azul.h");
    let mut out = BTreeSet::new();
    for line in h.lines().filter(|l| l.contains("DLLIMPORT")) {
        if let Some(open) = line.find('(') {
            let head = line[..open].trim_end();
            let name: String = head
                .chars()
                .rev()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            if name.starts_with("Az") {
                out.insert(name);
            }
        }
    }
    assert!(out.len() > 1000, "found only {} exports in azul.h", out.len());
    out
}

/// Everything libazul exports: the public C API (`azul.h`), every `extern "C"`
/// function the generated DLL body defines (the `Byref` / `Struct` /
/// `WithCtx` twins included), and the engine's own exports - the
/// host-invoker entry points each `impl_managed_callback!` site names
/// (`AzApp_set<X>Invoker`, `Az<X>_createFromHostHandle`) and every other
/// `extern "C" fn Az*` in core/ and layout/.
fn all_exports() -> BTreeSet<String> {
    let mut out = exported_functions();
    // azul.h's own static inline helpers and macros (`AzString_fromConstStr`,
    // the `Az<Union>_is<Variant>` tag tests): available to anything that
    // includes the header.
    for line in generated("azul.h").lines() {
        let t = line.trim_start();
        let rest = t
            .strip_prefix("#define ")
            .or_else(|| t.starts_with("static inline").then(|| t.split_once('(').map_or(t, |(h, _)| h)));
        if let Some(rest) = rest {
            let name: String = rest
                .rsplit(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .find(|w| w.starts_with("Az"))
                .unwrap_or("")
                .to_string();
            let name = if t.starts_with("#define ") {
                rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect()
            } else {
                name
            };
            if name.starts_with("Az") {
                out.insert(name);
            }
        }
    }
    let ident = |s: &str| -> String {
        s.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect()
    };
    for line in generated("dll_api_internal.rs").lines() {
        if let Some(p) = line.find("extern \"C\" fn Az") {
            out.insert(ident(&line[p + "extern \"C\" fn ".len()..]));
        }
    }
    for dir in ["core/src", "layout/src"] {
        for e in walkdir::WalkDir::new(repo_root().join(dir))
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "rs"))
            .filter(|e| !e.path().to_string_lossy().ends_with("_test.rs"))
        {
            let Ok(src) = std::fs::read_to_string(e.path()) else { continue };
            for line in src.lines() {
                let t = line.trim_start();
                for key in ["setter_fn:", "from_handle_fn:", "from_handle_byref_fn:"] {
                    if let Some(rest) = t.strip_prefix(key) {
                        out.insert(ident(rest.trim_start()));
                    }
                }
                if let Some(p) = t.find("extern \"C\" fn Az") {
                    out.insert(ident(&t[p + "extern \"C\" fn ".len()..]));
                }
            }
        }
    }
    out.retain(|s| s.starts_with("Az"));
    out
}

/// Every `Az<Type>_<function>` the text USES as a libazul symbol: quoted
/// (`"AzX_y"`, `'AzX_y'`, Ruby's `:AzX_y`) or called (`AzX_y(`) - a binding's
/// own identifiers that merely look like one (Haskell's `AzXType_inner`,
/// Fortran's `AzXType_default => null()`) are neither. Names the text defines
/// itself (a C shim's `static void AzX_trampoline(...) {`) are left out.
fn referenced_symbols(text: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut used = BTreeSet::new();
    let mut defined = BTreeSet::new();
    for line in text.lines() {
        let b = line.as_bytes();
        let mut i = 0;
        // `function AzImageRef_getRawimage_2(...); external name 'AzImageRef_getRawimage';`
        // BINDS a local name to an export. Pascal folds case, so two api.json
        // methods differing only in case need one renamed; the renamed side is
        // a declaration, not a reference to a symbol that does not exist. Only
        // a line that also quotes the real export counts as such a binding, so
        // a plain `extern` declaration still has to name something exported.
        let renames_an_export = line.contains("external")
            && line.matches(['"', '\'']).count() >= 2
            && line.contains("Az");
        while i + 2 < b.len() {
            let boundary = i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_');
            if boundary && b[i] == b'A' && b[i + 1] == b'z' && b[i + 2].is_ascii_uppercase() {
                let mut j = i;
                while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                    j += 1;
                }
                let tok = &line[i..j];
                let is_fn = tok
                    .split_once('_')
                    .is_some_and(|(_, f)| f.chars().next().is_some_and(|c| c.is_ascii_lowercase()));
                if is_fn {
                    let before = if i > 0 { b[i - 1] } else { b' ' };
                    let quoted = matches!(before, b'"' | b'\'' | b':')
                        && !(before == b':' && i > 1 && b[i - 2] == b':');
                    let rest = line[j..].trim_start();
                    let called = rest.starts_with('(');
                    // `<type> AzX_y(...) {` / `... )` : a C definition, the
                    // text before the name ending in a type (or `*`).
                    let head = line[..i].trim_end();
                    let last_word = head
                        .rsplit(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                        .next()
                        .unwrap_or("");
                    let definition = called
                        && (head.ends_with('*')
                            || (!last_word.is_empty()
                                && head.ends_with(last_word)
                                && !matches!(last_word, "return" | "else" | "case" | "sizeof" | "call" | "do")))
                        && {
                            let t = line.trim_end();
                            t.ends_with('{') || t.ends_with(')') || t.ends_with('}')
                        };
                    if definition || (renames_an_export && !quoted) {
                        defined.insert(tok.to_string());
                    } else if quoted || called {
                        used.insert(tok.to_string());
                    }
                }
                i = j;
            } else {
                i += 1;
            }
        }
    }
    (used, defined)
}

/// A failure that names thousands of items is only actionable in full, but a
/// panic message that long is unreadable. `AZ_BUG_CLASS_DUMP=<dir>` writes
/// every offender to `<dir>/<what>-<lang>.txt` while the message stays a
/// summary.
fn dump_full(lang: &str, what: &str, items: &[String]) {
    let Some(dir) = std::env::var_os("AZ_BUG_CLASS_DUMP") else {
        return;
    };
    let dir = PathBuf::from(dir);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let slug: String = what
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let _ = std::fs::write(dir.join(format!("{slug}-{lang}.txt")), items.join("\n"));
}

/// What each API function is: its kind, and the category of the type it sits
/// on, keyed by C name.
fn function_kinds() -> &'static BTreeMap<String, (FunctionKind, String)> {
    static KINDS: OnceLock<BTreeMap<String, (FunctionKind, String)>> = OnceLock::new();
    KINDS.get_or_init(|| {
        ir().functions
            .iter()
            .map(|f| {
                let category = ir()
                    .find_struct(&f.class_name)
                    .map(|s| s.category)
                    .or_else(|| ir().find_enum(&f.class_name).map(|e| e.category))
                    .map_or_else(|| "?".to_string(), |c| format!("{c:?}"));
                (f.c_name.clone(), (f.kind, category))
            })
            .collect()
    })
}

/// Summarizes missing C symbols by WHAT they are - `Delete on Vec x412`,
/// `EnumVariantConstructor on Enum x88` - so a coverage failure states the
/// shape of the gap (one emitter branch that never runs) instead of a wall of
/// names, then gives examples from the largest group.
fn summarize_symbols(lang: &str, what: &str, missing: &[String], total: usize) -> String {
    dump_full(lang, what, missing);
    let kinds = function_kinds();
    let mut groups: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for m in missing {
        let key = kinds
            .get(m)
            .map_or_else(|| "not an IR function".to_string(), |(k, c)| format!("{k:?} on {c}"));
        groups.entry(key).or_default().push(m.as_str());
    }
    let mut by_size: Vec<(&String, &Vec<&str>)> = groups.iter().collect();
    by_size.sort_by_key(|(k, v)| (std::cmp::Reverse(v.len()), (*k).clone()));
    let shape: Vec<String> = by_size.iter().map(|(k, v)| format!("{k} x{}", v.len())).collect();
    let examples: Vec<&str> = by_size
        .first()
        .map(|(_, v)| v.iter().take(3).copied().collect())
        .unwrap_or_default();
    format!(
        "[{lang}] {} of {total} {what}: {}; e.g. {}",
        missing.len(),
        shape.join(", "),
        examples.join(", ")
    )
}

/// Summarizes a per-language list of missing items for a failure message.
fn summarize(lang: &str, what: &str, missing: &[String], total: usize) -> String {
    dump_full(lang, what, missing);
    let shown: Vec<&str> = missing.iter().take(8).map(String::as_str).collect();
    format!(
        "[{lang}] {} of {total} {what}: {}{}",
        missing.len(),
        shown.join(", "),
        if missing.len() > shown.len() { ", ..." } else { "" }
    )
}

/// A binding may only call what libazul exports. A reference to anything
/// else (`AzDom_deepCopy` where the export is `AzDom_clone`, a `Byref` twin
/// the DLL never emitted) is a link error in a compiled binding and a runtime
/// crash in a dynamic one.
#[test]
fn bindings_only_reference_symbols_libazul_exports() {
    let exports = all_exports();
    let mut offenders = Vec::new();
    for (lang, files) in shipped_outputs() {
        // A name the binding defines itself (in any of its files - a C shim
        // defines what its Haskell/OCaml side imports) is not a libazul symbol.
        let mut used = BTreeSet::new();
        let mut defined = BTreeSet::new();
        for (_, text) in &files {
            let (u, d) = referenced_symbols(text);
            used.extend(u);
            defined.extend(d);
        }
        let total: BTreeSet<String> = used.difference(&defined).cloned().collect();
        let missing: BTreeSet<String> = total.iter().filter(|s| !exports.contains(*s)).cloned().collect();
        if !missing.is_empty() {
            let m: Vec<String> = missing.into_iter().collect();
            offenders.push(summarize(lang, "referenced symbols are not exported", &m, total.len()));
        }
    }
    assert_none("bindings referencing symbols libazul does not export", offenders);
}

/// The API every binding must reach: every IR function (methods, trait
/// functions, variant constructors), except those of libazul's own
/// destructor/cloner types (`*VecDestructor`, `InstantPtrCloneCallback`,
/// ...), which no binding wraps - they are memory-management plumbing - and
/// the two optional forms below.
fn api_functions() -> BTreeSet<String> {
    // A unit enum is a plain C enum value: a binding may expose it as its own
    // enum, with the language's equality, ordering and hashing of that value,
    // and never call `Az<Enum>_<variant>()` or its derives. And a type with a
    // total order may surface ordering through `_cmp` alone.
    let unit_enum = |class: &str| ir().find_enum(class).is_some_and(|e| !e.is_union);
    let has_cmp: BTreeSet<&str> = ir()
        .functions
        .iter()
        .filter(|f| f.kind == FunctionKind::Cmp)
        .map(|f| f.class_name.as_str())
        .collect();
    let optional = |f: &FunctionDef| {
        unit_enum(&f.class_name)
            || (f.kind == FunctionKind::PartialCmp && has_cmp.contains(f.class_name.as_str()))
    };
    let internal = |class: &str| {
        ir().find_struct(class)
            .is_some_and(|s| s.category == TypeCategory::DestructorOrClone)
            || ir()
                .find_enum(class)
                .is_some_and(|e| e.category == TypeCategory::DestructorOrClone)
    };
    ir().functions
        .iter()
        .filter(|f| !internal(&f.class_name) && !optional(f))
        .map(|f| f.c_name.clone())
        .collect()
}

/// A C symbol in the spelling-independent form every binding's name for it
/// shares: lowercase, no underscores, without a leading `c` (Haskell's
/// `c_AzX`) or a trailing ABI-twin suffix (`Byref`, `Struct`, `WithCtx`).
/// `AzHttpClient_partialEq`, Ruby's `az_http_client_partial_eq` and Crystal's
/// `azHttpClient_partialEq` are all `azhttpclientpartialeq`.
fn canonical_symbol(token: &str) -> Option<String> {
    let mut t: String = token.chars().filter(|c| *c != '_').collect::<String>().to_ascii_lowercase();
    if t.starts_with("caz") {
        t.remove(0);
    }
    if !t.starts_with("az") {
        return None;
    }
    for suffix in ["structbyref", "withctxbyref", "byref", "struct", "withctx"] {
        if let Some(stem) = t.strip_suffix(suffix) {
            t = stem.to_string();
            break;
        }
    }
    Some(t)
}

/// For every canonical symbol, on how many lines of `files` it appears (a
/// line that names it twice - a declaration binding an alias to the C name -
/// counts once).
fn symbol_line_counts(files: &[(String, String)]) -> BTreeMap<String, usize> {
    // A comment naming a symbol is not a call. Counting one would mean a
    // binding scores green for the very functions it documents as skipped -
    // rewarding the note instead of the wrapper - so a line that is nothing
    // but a comment, in any of the shipped languages' syntaxes, is not
    // evidence of reach.
    const COMMENT: &[&str] = &[
        "//", "/*", "*/", "*", "#", "--", "(*", "*)", "{-", "-}", "!", ";", "%", "{ ", "}",
    ];
    let mut out: BTreeMap<String, usize> = BTreeMap::new();
    for (_, text) in files {
        for line in text.lines() {
            if !line.contains("az") && !line.contains("Az") {
                continue;
            }
            let t = line.trim_start();
            if COMMENT.iter().any(|c| t.starts_with(c)) {
                continue;
            }
            let on_line: BTreeSet<String> = line
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .filter(|w| w.len() > 3)
                .filter_map(canonical_symbol)
                .collect();
            for c in on_line {
                *out.entry(c).or_default() += 1;
            }
        }
    }
    out
}

/// Every API function is declared by every binding with its own FFI layer
/// (C, C++ and Swift use azul.h itself; Python calls Rust). A missing
/// declaration is API the binding can never reach.
#[test]
fn every_api_function_is_declared_by_every_binding() {
    let api = api_functions();
    let mut offenders = Vec::new();
    for (lang, files) in shipped_outputs() {
        if matches!(lang, "c" | "cpp" | "swift" | "python") {
            continue;
        }
        let counts = symbol_line_counts(&files);
        let missing: Vec<String> = api
            .iter()
            .filter(|f| canonical_symbol(f).is_none_or(|c| !counts.contains_key(&c)))
            .cloned()
            .collect();
        if !missing.is_empty() {
            offenders.push(summarize_symbols(lang, "API functions are never declared", &missing, api.len()));
        }
    }
    assert_none("API functions missing from a binding's FFI layer", offenders);
}

/// The derives of the borrowed-slice types (`F32VecRef`, `GLuintVecRefMut`,
/// ...). A `VecRef` is `{ptr, len}` pointing at memory the CALLER owns: a
/// binding builds one at the call site from a native array and lets it die
/// there. Its `_delete` is a no-op `drop_in_place` and its `_clone` copies the
/// borrow, so surfacing either as idiomatic API hands user code a way to free
/// or duplicate someone else's buffer. No borrowed-slice type has an api.json
/// function of its own - every symbol on one is a derive - so they only have
/// to be DECLARED, which
/// `every_api_function_is_declared_by_every_binding` enforces.
///
/// `is_api_function` rather than `!is_trait_function`: the latter leaves out
/// `_clone` and `_createDefault`, which are exactly as unsafe to expose here.
fn borrowed_slice_plumbing() -> &'static BTreeSet<String> {
    static PLUMBING: OnceLock<BTreeSet<String>> = OnceLock::new();
    PLUMBING.get_or_init(|| {
        ir().functions
            .iter()
            .filter(|f| !f.kind.is_api_function())
            .filter(|f| {
                ir().find_struct(&f.class_name)
                    .is_some_and(|s| s.category == TypeCategory::VecRef)
            })
            .map(|f| f.c_name.clone())
            .collect()
    })
}

/// The declared capabilities (`_partialEq`, `_cmp`, `_hash`, `_clone`,
/// `_createDefault`, `_toDbgString`) of the API's string type.
///
/// A binding may map `String` to the language's own string, and then that
/// type's equality, ordering, hashing, copy, debug and default ARE those
/// capabilities - answered natively, without an FFI call. Surfacing the C
/// ones beside them is not extra API, it is a second, worse answer to the
/// same question: each allocates and copies the bytes to compare them, and
/// `_hash` returns Rust's `DefaultHasher` value, which is NOT the language's
/// own hash of the same string. Two hashes for one string is a silent-bug
/// generator. `api_functions()` already makes this exact call for unit enums.
///
/// The string's CONSTRUCTORS are not covered: `fromUtf16Be`, `fromCStr` and
/// friends are api.json functions with no native equivalent, and a binding
/// that cannot reach them is genuinely missing API.
fn native_string_capabilities() -> &'static BTreeSet<String> {
    static CAPS: OnceLock<BTreeSet<String>> = OnceLock::new();
    CAPS.get_or_init(|| {
        ir().functions
            .iter()
            .filter(|f| f.kind.is_declared_capability())
            .filter(|f| {
                ir().find_struct(&f.class_name)
                    .is_some_and(|s| s.category == TypeCategory::String)
            })
            .map(|f| f.c_name.clone())
            .collect()
    })
}

/// A variant constructor whose payload is an opaque pointer.
///
/// `OptionX11Visual::Some` carries an `X11Visual`, whose IR target is
/// `*const c_void` - an X server handle the caller got from Xlib. A binding whose idiomatic layer
/// refuses raw pointers on purpose (Crystal says so in its header, Python in
/// `python_unbridgeable`) cannot give it a safe form, and inventing one would
/// hand user code a pointer it cannot validate. The raw FFI layer still
/// declares it, so the capability is there for whoever holds a real Visual.
fn opaque_pointer_variant(c_name: &str) -> bool {
    ir().functions
        .iter()
        .filter(|f| f.kind == FunctionKind::EnumVariantConstructor)
        .filter(|f| f.c_name == c_name)
        .any(|f| {
            f.args.iter().any(|a| {
                ir().find_type_alias(a.type_name.trim()).is_some_and(|alias| {
                    let t = alias.target.trim_start();
                    t.starts_with("*const") || t.starts_with("*mut")
                })
            })
        })
}

/// Symbols no Fortran identifier can spell.
///
/// A Fortran identifier is at most 63 characters. The shortest lossless
/// rendering of `Az<Class>_<method>` drops the separator, so a C symbol of 65
/// characters or more has no legal Fortran name at all and the binding must
/// bind it under a hashed alias. The capability IS reachable that way - the
/// alias is a normal callable - but a probe that counts lines naming the C
/// symbol cannot see it, and papering over that with a comment mentioning the
/// symbol would make the test pass for a reason that is not reachability.
/// One symbol qualifies today:
/// `AzCssStylePerspectiveOriginParseErrorOwned_wrongNumberOfComponents`.
fn unspellable_in_fortran(c_name: &str) -> bool {
    const FORTRAN_IDENT_MAX: usize = 63;
    c_name.len().saturating_sub(1) > FORTRAN_IDENT_MAX
}

/// Go builds every tagged-union variant itself instead of calling the
/// exported constructor: `types.go` writes the discriminant and the payload
/// into a Go struct that mirrors the C layout, and a `const _ = uint(16 -
/// unsafe.Sizeof(...))` assertion next to it fails the build if that layout
/// ever stops matching. That is the whole design of the Go binding - it uses
/// purego and mirrors C types natively rather than paying an FFI call - so
/// `Az<Enum>_<variant>` is unreachable there ON PURPOSE, while every variant
/// remains constructible. The constructor is still DECLARED nowhere and
/// needs none: nothing links against it.
fn go_builds_variants_natively(kind: FunctionKind) -> bool {
    kind == FunctionKind::EnumVariantConstructor
}

/// The derives of an alias that shares its monomorphized type with another
/// alias, minus the one name that carries the impls. (`is_api_function`
/// again: `_clone` and `_createDefault` are impls on the same type too.)
///
/// api.json gives two names to one instantiation in four places
/// (`LayoutGridAutoColumnsValue` and `LayoutGridAutoRowsValue` are both
/// `CssPropertyValue<GridAutoTracks>`). Every other binding emits two distinct
/// C types, but a Rust `pub type` is transparent - the two names ARE one type -
/// so only one set of trait impls can exist; a second is E0119, conflicting
/// implementations. The twin's C symbols are therefore never called from Rust,
/// while the behaviour is fully reachable through the name that carries them.
fn rust_alias_twins() -> &'static BTreeMap<String, Vec<String>> {
    static TWINS: OnceLock<BTreeMap<String, Vec<String>>> = OnceLock::new();
    TWINS.get_or_init(|| {
        let mut by_instantiation: BTreeMap<String, Vec<&str>> = BTreeMap::new();
        for a in &ir().type_aliases {
            if a.generic_args.is_empty() {
                continue;
            }
            by_instantiation
                .entry(format!("{}<{}>", a.target, a.generic_args.join(",")))
                .or_default()
                .push(a.name.as_str());
        }
        // Which alias of a group carries the impls is the emitter's choice, so
        // map each symbol to the same method on ALL of its twins and let the
        // caller ask whether any of them is reached.
        let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for names in by_instantiation.values().filter(|n| n.len() > 1) {
            for f in ir().functions.iter().filter(|f| !f.kind.is_api_function()) {
                if !names.contains(&f.class_name.as_str()) {
                    continue;
                }
                let suffix = f.c_name.trim_start_matches(&format!("Az{}", f.class_name));
                let siblings = names.iter().map(|n| format!("Az{n}{suffix}")).collect();
                out.insert(f.c_name.clone(), siblings);
            }
        }
        out
    })
}

/// Every API function is reachable from each binding's idiomatic API, not
/// just declared: it appears on at least one line besides its declaration
/// (C++ and Swift declare nothing, so on one line). This is the silent-skip
/// class - a method, derive (`_hash`, `_partialCmp`, ...) or variant
/// constructor the binding declares and then never wraps.
///
/// KNOWN BLIND SPOT, read the numbers with it in mind: this counts LINES THAT
/// NAME THE SYMBOL, not calls. A binding whose FFI layer renames what it
/// imports - OCaml bound `AzApp_create` as `ffi_az_app_create`, which
/// `canonical_symbol` cannot map back - scores zero for its whole idiomatic
/// layer even where every function is wrapped, which is why OCaml once looked
/// like the worst binding of the 19. The probe cannot be made exact without a
/// per-language call graph; a binding that spells the C symbol it calls is
/// also the one whose link errors are traceable, so the proxy is the rule.
#[test]
fn every_api_function_is_reachable_from_the_idiomatic_api() {
    let api = api_functions();
    let mut offenders = Vec::new();
    for (lang, files) in shipped_outputs() {
        let needed = match lang {
            "c" | "python" => continue, // C is the FFI; Python has its own probe.
            "cpp" | "swift" => 1,
            _ => 2,
        };
        let counts = symbol_line_counts(&files);
        // Rust's drop glue drops a value's fields: only the types that own
        // memory directly carry a `Drop` calling their `_delete`, so an
        // uncalled `_delete` is not a leak there (it is in C++ or Zig).
        let delete_fns: BTreeSet<&str> = ir()
            .functions
            .iter()
            .filter(|f| f.kind == FunctionKind::Delete)
            .map(|f| f.c_name.as_str())
            .collect();
        let missing: Vec<String> = api
            .iter()
            .filter(|f| !(lang == "rust" && delete_fns.contains(f.as_str())))
            .filter(|f| {
                // An alias twin counts as reached when the name that carries
                // the shared impls is reached.
                lang != "rust"
                    || rust_alias_twins().get(f.as_str()).is_none_or(|twins| {
                        !twins.iter().any(|t| {
                            canonical_symbol(t).and_then(|c| counts.get(&c)).copied().unwrap_or(0)
                                >= needed
                        })
                    })
            })
            .filter(|f| {
                lang != "go"
                    || !function_kinds()
                        .get(f.as_str())
                        .is_some_and(|(k, _)| go_builds_variants_natively(*k))
            })
            .filter(|f| !(lang == "fortran" && unspellable_in_fortran(f)))
            .filter(|f| !native_string_capabilities().contains(f.as_str()))
            .filter(|f| !opaque_pointer_variant(f))
            .filter(|f| !borrowed_slice_plumbing().contains(f.as_str()))
            .filter(|f| {
                canonical_symbol(f)
                    .and_then(|c| counts.get(&c).copied())
                    .unwrap_or(0)
                    < needed
            })
            .cloned()
            .collect();
        if !missing.is_empty() {
            offenders.push(summarize_symbols(
                lang,
                "API functions are never called by the idiomatic API",
                &missing,
                api.len(),
            ));
        }
    }
    assert_none("API functions unreachable from a binding's idiomatic API", offenders);
}

/// Classes Python deliberately does not give a `#[pyclass]`, because the
/// value already IS a Python object.
///
/// * a borrowed slice (`VecRef`/`VecRefMut`) is `{ptr, len}` over memory the
///   caller owns - a pyclass over one is a dangling pointer with a `__del__`;
/// * the rest map onto a builtin: `String` is `str`, `U8Vec` is `bytes`,
///   `StringVec`/`GLintVec`/`GLuintVec` are lists, `RefAny` is the Python
///   object itself, and their methods are the builtin's own;
/// * `InstantPtr` carries its own clone/destructor function pointers around
///   an opaque `*const c_void`, and `StringMenuItem` is the recursive knot of
///   the menu tree - neither has a Python shape at all.
fn python_has_no_class(class: &str) -> bool {
    if ir()
        .find_struct(class)
        .is_some_and(|s| s.category == TypeCategory::VecRef)
    {
        return true;
    }
    // allow-api-name: the classes whose Python form is a builtin. There is no
    // IR property for "this one is a `str`"; the emitter keeps the same list.
    matches!(
        class,
        "String"
            | "U8Vec"
            | "StringVec"
            | "GLintVec"
            | "GLuintVec"
            | "RefAny"
            | "InstantPtr"
            | "StringMenuItem"
    )
}

/// Why Python cannot expose this function, or `None` if it must.
///
/// Each reason is a property of the SIGNATURE, not a list of names:
///
/// * a raw pointer argument or return - a Python object has no address the
///   callee may keep, and every `*Vec.copy_from_ptr` (124 of them) has
///   `create` / `from_item` / `with_capacity` beside it;
/// * a borrowed slice the callee writes through (`VecRefMut`) - a Python list
///   is not a contiguous typed buffer, so an honest bridge takes a length and
///   returns a list, which is a different method;
/// * a bare C function pointer with no wrapper to store a Python callable in;
/// * building or unwrapping a callback wrapper: the Python API takes the
///   callable itself everywhere, so a wrapper object has nowhere to go.
fn python_unbridgeable(f: &FunctionDef) -> Option<&'static str> {
    let is_borrowed_slice = |t: &str| {
        ir().find_struct(t.trim())
            .is_some_and(|s| s.category == TypeCategory::VecRef)
    };
    // An `OptionU8VecRef` is a borrow behind a tag: Python would have to keep
    // the buffer alive for a value that may not be there.
    let wraps_borrowed_slice = |t: &str| {
        ir().find_enum(t.trim()).is_some_and(|e| {
            e.variants.iter().any(|v| match &v.kind {
                EnumVariantKind::Tuple(types) => types.iter().any(|(ty, _)| is_borrowed_slice(ty)),
                _ => false,
            })
        })
    };
    let raw_pointer = |t: &str| t.contains("*const") || t.contains("*mut");
    let fn_pointer = |t: &str| ir().callback_typedefs.iter().any(|c| c.name == t.trim());

    for a in &f.args {
        if matches!(a.ref_kind, ArgRefKind::Ptr | ArgRefKind::PtrMut) || raw_pointer(&a.type_name) {
            return Some("takes a raw pointer");
        }
        if wraps_borrowed_slice(&a.type_name) {
            return Some("takes an optional borrowed slice");
        }
        if is_borrowed_slice(&a.type_name) {
            // A slice of primitives, or of a repr(transparent) class, is
            // bridged by lending a Python buffer for the call. One whose
            // element the IR cannot even name (`RefstrVecRef` - borrowed
            // strings inside a borrowed slice) is two levels of lent memory.
            if a.type_name.contains("Mut") {
                return Some("writes through a borrowed slice");
            }
            if ir().vecref_element(a.type_name.trim()).is_none() {
                return Some("takes a borrowed slice of borrowed values");
            }
        }
        if fn_pointer(&a.type_name) {
            return Some("takes a bare C function pointer");
        }
    }
    if let Some(ret) = &f.return_type {
        if raw_pointer(ret) {
            return Some("returns a raw pointer");
        }
        if is_borrowed_slice(ret) || wraps_borrowed_slice(ret) {
            return Some("returns a borrowed slice");
        }
    }
    // `Callback.create` / `.to_core`: a wrapper object Python has no use for.
    if ir()
        .find_struct(&f.class_name)
        .is_some_and(|s| s.callback_wrapper_info.is_some())
    {
        return Some("builds or unwraps a callback wrapper");
    }
    None
}

/// Python calls the Rust API directly, not the C symbols: every API class
/// with a Python class must expose every one of its API methods.
#[test]
fn python_exposes_every_method_of_every_class() {
    let py = generated("python_api.rs");
    let mut methods: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in py.lines() {
        if let Some(rest) = line.strip_prefix("impl Az") {
            current = rest.split(|c: char| !c.is_ascii_alphanumeric()).next().map(str::to_string);
        } else if line == "}" {
            current = None;
        } else if let (Some(c), Some(rest)) = (&current, line.trim_start().strip_prefix("fn ")) {
            let name: String = rest.chars().take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_').collect();
            methods.entry(c.clone()).or_default().insert(name);
        }
    }
    let mut missing_classes = Vec::new();
    let mut missing = Vec::new();
    let mut total = 0;
    for f in &ir().functions {
        if f.kind.is_trait_function() || f.kind == FunctionKind::EnumVariantConstructor {
            continue;
        }
        if python_has_no_class(&f.class_name) || python_unbridgeable(f).is_some() {
            continue;
        }
        total += 1;
        match methods.get(&f.class_name) {
            None => missing_classes.push(f.class_name.clone()),
            Some(m) if !m.contains(&f.method_name) => {
                missing.push(format!("{}.{}", f.class_name, f.method_name))
            }
            _ => {}
        }
    }
    missing_classes.sort();
    missing_classes.dedup();
    let mut offenders = Vec::new();
    if !missing_classes.is_empty() {
        let classes_total = ir().functions.iter().map(|f| f.class_name.as_str()).collect::<BTreeSet<_>>().len();
        offenders.push(summarize("python", "API classes have no Python class", &missing_classes, classes_total));
    }
    if !missing.is_empty() {
        offenders.push(summarize("python", "API methods are not exposed", &missing, total));
    }
    assert_none("Python binding coverage", offenders);
}

/// Every api.json constant reaches every binding (S18), spelled as the
/// language requires (Haskell identifiers must start lowercase).
#[test]
fn every_constant_reaches_every_binding() {
    let mut offenders = Vec::new();
    for (lang, files) in shipped_outputs() {
        let text: String = files.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join("\n");
        let missing: Vec<String> = ir()
            .constants
            .iter()
            .filter_map(|c| {
                let (class, name) = c.name.split_once('_')?;
                let spelled = match lang {
                    // An OCaml value name must start lowercase: `accum_alpha_bits`.
                    "ocaml" => name.to_ascii_lowercase(),
                    "haskell" => {
                        let camel: String = name
                            .split('_')
                            .map(|w| {
                                let w = w.to_ascii_lowercase();
                                let mut ch = w.chars();
                                ch.next().map(|f| f.to_ascii_uppercase().to_string() + ch.as_str()).unwrap_or_default()
                            })
                            .collect();
                        lower_first(&format!("{class}{camel}"))
                    }
                    _ => name.to_string(),
                };
                (!text.contains(&spelled)).then(|| c.name.clone())
            })
            .collect();
        if !missing.is_empty() {
            offenders.push(summarize(lang, "constants are missing", &missing, ir().constants.len()));
        }
    }
    assert_none("api.json constants missing from bindings", offenders);
}

/// No generated binding carries a placeholder instead of API: a
/// `SKIPPED` marker is an item the emitter silently gave up on, an
/// "ABI-completeness stub" a callback trampoline that returns the default
/// instead of calling the application.
#[test]
fn generated_code_has_no_skipped_placeholders() {
    let mut offenders = Vec::new();
    for (lang, files) in shipped_outputs() {
        let hits: Vec<String> = files
            .iter()
            .flat_map(|(path, text)| {
                text.lines()
                    .enumerate()
                    .filter(|(_, l)| l.contains("SKIPPED") || l.contains("ABI-completeness stub"))
                    .map(move |(n, l)| format!("{path}:{}: {}", n + 1, l.trim()))
            })
            .collect();
        if !hits.is_empty() {
            offenders.push(summarize(lang, "SKIPPED placeholders", &hits, hits.len()));
        }
    }
    assert_none("SKIPPED placeholders in generated bindings", offenders);
}

/// Splits `text` at every line that opens a type definition matching `open`
/// (the returned name), so each chunk holds one class body.
fn class_chunks<'a>(text: &'a str, open: &dyn Fn(&str) -> Option<String>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        if let Some(name) = open(line) {
            out.push((name, String::new()));
        }
        if let Some((_, body)) = out.last_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }
    out
}

/// A class that defines value equality (its `equals` calls the C
/// `_partialEq`) also defines a hash that agrees with it: equal values must
/// hash equal, so the hash must be derived from the value - the C `_hash`, the
/// debug string, or a constant. Hashing the handle's address
/// (`ptr.hashCode()`, an identity hash) breaks every hash map and set.
#[test]
fn equality_is_always_paired_with_a_consistent_hash() {
    type Opener = fn(&str) -> Option<String>;
    let java: Opener = |l| {
        let t = l.trim_start();
        (t.starts_with("public ") && t.contains(" class ")).then(|| t.to_string())
    };
    let kotlin: Opener = |l| {
        let t = l.trim_start();
        let t2 = t.trim_start_matches("public ").trim_start_matches("open ").trim_start_matches("data ");
        (t2.starts_with("class ")).then(|| t.to_string())
    };
    let csharp: Opener = |l| {
        let t = l.trim_start();
        (t.starts_with("public ") && (t.contains(" class ") || t.contains(" struct "))).then(|| t.to_string())
    };
    let ruby: Opener = |l| l.trim_start().starts_with("class ").then(|| l.trim().to_string());
    // (language, class opener, equality marker, hash marker)
    let rules: [(&str, Opener, &str, &str); 4] = [
        ("java", java, "boolean equals(", "int hashCode()"),
        ("kotlin", kotlin, "override fun equals(", "override fun hashCode()"),
        ("csharp", csharp, "override bool Equals(", "override int GetHashCode()"),
        ("ruby", ruby, "def ==(", "def hash"),
    ];
    // The hash body: from its marker to the first blank line (every generated
    // member is separated by one).
    let body_of = |text: &str, marker: &str| -> Option<String> {
        let p = text.find(marker)?;
        let rest = &text[p..];
        Some(rest.split("\n\n").next().unwrap_or(rest).to_string())
    };
    let lower = |x: &str| x.to_ascii_lowercase().replace('_', "");
    let value_based = |h: &str| {
        let l = lower(h);
        l.contains("hash(") && !l.contains("address.hash") && !l.contains("ptr.hashcode")
            || l.contains("todbgstring(")
            || h.lines().any(|l| {
                let t = l.trim().trim_end_matches(';').trim_end_matches('}').trim();
                // A constant: its own line, or an expression body
                // (`fun hashCode(): Int = 0`, `GetHashCode() => 0;`).
                matches!(t, "return 0" | "0" | "= 0" | "=> 0" | "return 1")
                    || t.ends_with(" = 0")
                    || t.ends_with("=> 0")
            })
    };
    let outputs = shipped_outputs();
    let mut offenders = Vec::new();
    for (lang, open, eq, hash) in rules {
        let Some(files) = outputs.get(lang) else { continue };
        let mut bad = Vec::new();
        let mut total = 0;
        for (path, text) in files {
            for (class, body) in class_chunks(text, &open) {
                let Some(eq_body) = body_of(&body, eq) else { continue };
                if !lower(&eq_body).contains("partialeq(") {
                    continue; // not value equality (not routed through the C `_partialEq`)
                }
                total += 1;
                match body_of(&body, hash) {
                    None => bad.push(format!("{path}: `{class}` has value equality but no hash")),
                    Some(h) if !value_based(&h) => bad.push(format!(
                        "{path}: `{class}` hashes the handle, not the value: {}",
                        h.lines().nth(1).unwrap_or("").trim()
                    )),
                    _ => {}
                }
            }
        }
        if !bad.is_empty() {
            offenders.push(summarize(lang, "classes break the equals/hash contract", &bad, total));
        }
    }
    assert_none("equality without a consistent hash", offenders);
}

/// The binding never frees a value twice on its own: C++ must not expose a raw
/// `delete_()` next to the destructor that frees the value too, and an owning
/// Fortran wrapper that is FINALIZED (`final ::`) needs a defined
/// `assignment(=)` - intrinsic assignment would copy the handle, and both
/// copies would be finalized. The Fortran binding finalizes nothing: a value
/// is freed by an explicit `%delete()`, which clears `owned` (so a second
/// `delete` of the same variable is a no-op).
#[test]
fn copies_of_owning_values_are_never_freed_twice() {
    let outputs = shipped_outputs();
    let mut offenders = Vec::new();
    if let Some(files) = outputs.get("cpp") {
        for (path, text) in files {
            let n = text.matches("void delete_()").count();
            if n > 0 {
                offenders.push(format!("[cpp] {path}: {n} public `delete_()` methods (the destructor frees the value too)"));
            }
        }
    }
    if let Some(files) = outputs.get("fortran") {
        // A finalized owning wrapper without a defined `assignment(=)`: `b = a`
        // copies the handle and the flag, and both are finalized.
        let mut bad = Vec::new();
        let mut total = 0;
        for (path, text) in files {
            let lower = text.to_ascii_lowercase();
            for chunk in lower.split("\n  type ::").skip(1) {
                let body = chunk.split("end type").next().unwrap_or("");
                if body.contains(":: owned") && body.contains("final ::") {
                    total += 1;
                    if !body.contains("assignment(=)") {
                        bad.push(format!("{path}: {}", body.lines().next().unwrap_or("").trim()));
                    }
                }
            }
            // `delete` must clear the flag, or deleting a variable twice frees twice.
            for chunk in lower.split("\n  subroutine ").skip(1) {
                let body = chunk.split("end subroutine").next().unwrap_or("");
                let name = body.split('(').next().unwrap_or("").trim();
                if name.ends_with("_delete") && body.contains("%owned") && !body.contains("%owned = .false.") {
                    bad.push(format!("{path}: {name} leaves `owned` set"));
                }
            }
        }
        if !bad.is_empty() {
            offenders.push(summarize("fortran", "owning wrappers freed twice", &bad, total));
        }
    }
    assert_none("double frees through copies", offenders);
}

/// Every generated package manifest carries the API version (ir.rs: "MUST use
/// `api_version`"), never a hard-coded one.
#[test]
fn package_versions_are_the_api_version() {
    let v = &ir().api_version;
    assert!(!v.is_empty(), "IR has no api_version");
    let mut offenders = Vec::new();
    // (file, relative to target/codegen or - `examples/...` - the repo, field
    // start, field end). The Kotlin build script's own `version` is the
    // hello-world APP's; the binding it pins is `azul.version`.
    let checks: [(&str, &str, &str); 4] = [
        ("Azul.csproj", "<Version>", "</Version>"),
        ("kotlin/build.gradle.kts", "(findProperty(\"azul.version\") as String?) ?: \"", "\""),
        ("node/package.json", "\"version\": \"", "\""),
        ("examples/kotlin/pom.xml", "<azul.version>", "</azul.version>"),
    ];
    for (file, open, close) in checks {
        let text = match file.strip_prefix("examples/") {
            Some(_) => std::fs::read_to_string(repo_root().join(file))
                .unwrap_or_else(|e| panic!("{file}: {e}")),
            None => generated(file),
        };
        match text.find(open).map(|p| &text[p + open.len()..]).and_then(|r| r.find(close).map(|q| &r[..q])) {
            Some(found) if found == v => {}
            Some(found) => offenders.push(format!("{file}: version `{found}`, api.json says `{v}`")),
            None => offenders.push(format!("{file}: no `{open}` version field")),
        }
    }
    assert_none("package versions", offenders);
}

/// No two inherent methods of the same Rust type share a name in the
/// generated Rust API. Aliases of one monomorph (`LayoutGridAutoColumnsValue`
/// and `LayoutGridAutoRowsValue`, both `CssPropertyValue<GridAutoTracks>`) are
/// ONE Rust type, so their inherent `impl` blocks meet.
#[test]
fn generated_rust_has_no_colliding_inherent_methods() {
    let mut offenders = Vec::new();
    for file in ["dll_api_internal.rs", "dll_api_external.rs"] {
        let text = generated(file);
        let mut alias: BTreeMap<String, String> = BTreeMap::new();
        for line in text.lines() {
            if let Some(rest) = line.trim_start().strip_prefix("pub type ") {
                if let Some((name, rhs)) = rest.split_once(" = ") {
                    alias.insert(name.trim().to_string(), rhs.trim_end_matches(';').trim().to_string());
                }
            }
        }
        let canon = |mut t: String| {
            for _ in 0..8 {
                match alias.get(&t) {
                    Some(r) if r != &t => t = r.clone(),
                    _ => break,
                }
            }
            t
        };
        let mut methods: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
        let mut current: Option<String> = None;
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("impl ") {
                if !rest.contains(" for ") {
                    current = rest.strip_suffix(" {").map(|t| t.trim().to_string());
                }
            } else if line == "}" {
                current = None;
            } else if let (Some(ty), Some(rest)) = (&current, line.trim_start().strip_prefix("pub fn ")) {
                // `r#type` is a raw identifier, not `r`.
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '#')
                    .collect();
                methods.entry(canon(ty.clone())).or_default().entry(name).or_default().push(ty.clone());
            }
        }
        for (ty, by_name) in methods {
            for (name, owners) in by_name {
                if owners.len() > 1 {
                    offenders.push(format!("{file}: `{ty}::{name}` defined {} times (via {})", owners.len(), owners.join(", ")));
                }
            }
        }
    }
    assert_none("colliding inherent methods in generated Rust", offenders);
}

/// Every public Vec-typed field of a wrapped struct is reachable from Python.
///
/// `generate_field_accessors` gates non-primitive fields on
/// `!is_direct_ffi_type`, which excludes BOTH the `String` and the `Vec`
/// category. Strings have their own branch above it; Vec fields had none, so
/// each one fell through to no accessor at all and the field was simply
/// absent from the Python class - silently, because nothing in the generator
/// or the compiler notices a field it declined to emit.
/// (`ComponentLibrary.components`, `Dom.children`, ~115 more.)
///
/// A Vec Python reaches through a builtin (`bytes` for `U8Vec`, a list) is
/// excluded on purpose: those have no wrapper struct, so there is no `.inner`
/// to read and no class to hold the accessor.
#[test]
fn every_wrapped_vec_field_has_a_python_accessor() {
    let py = generated("python_api.rs");
    let ir = ir();

    // The classes the generator actually emitted, so a type skipped for an
    // unrelated reason cannot fail this test.
    let emitted: BTreeSet<String> = py
        .lines()
        .filter_map(|l| l.trim().strip_prefix("#[pyclass(name = \""))
        .filter_map(|l| l.split('"').next())
        .map(str::to_string)
        .collect();

    let mut missing = Vec::new();
    for s in &ir.structs {
        if !emitted.contains(&s.name) {
            continue;
        }
        // The accessors of ONE class: everything between its `impl` line and
        // the closing brace at column 0.
        let Some(start) = py.find(&format!("\nimpl Az{} {{\n", s.name)) else {
            continue;
        };
        let body = &py[start..];
        let end = body.find("\n}\n").unwrap_or(body.len());
        let body = &body[..end];

        for f in &s.fields {
            if !f.is_public || f.ref_kind != crate::codegen::v2::ir::FieldRefKind::Owned {
                continue;
            }
            let t = f.type_name.as_str();
            let is_wrapped_vec = ir
                .find_struct(t)
                .map(|d| d.category == TypeCategory::Vec)
                .unwrap_or(false)
                && emitted.contains(t);
            if is_wrapped_vec && !body.contains(&format!("#[getter({})]", f.name)) {
                missing.push(format!("{}.{}: {}", s.name, f.name, t));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "{} Vec-typed field(s) are unreachable from Python - `generate_field_accessors` emitted \
         no accessor:\n  {}",
        missing.len(),
        missing.join("\n  ")
    );
}
