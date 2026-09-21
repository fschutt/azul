//! The callback boundary: the generated half of "a failing callback degrades
//! instead of taking the process down".
//!
//! Zig has no exceptions and no unwinding, so there is no language-level
//! "catch whatever the user failed with" here the way there is in Pascal, the
//! JVM/CLR bindings, Ruby, Node, D, Lua or Python. A `@panic` aborts; it
//! cannot be resumed. What Zig DOES have at this boundary is the binding's
//! own check: [`super::ZIG_RUNTIME`]'s `ReflectModel(T).downcast` asks
//! `AzRefAny_isType` whether the `RefAny` the engine is handing over really
//! holds the model the user's `fn` declares, and a mismatch — a callback
//! registered against one model and fired with another — is by far the
//! likeliest real failure. That check used to answer with `std.debug.panic`,
//! i.e. it took the process down from inside Rust's call, with nothing in the
//! log to say why.
//!
//! This module emits the two things the trampoline needs to answer instead:
//!
//! * `_logThrough(CT, arg, msg)` — report through the engine's log sink if
//!   this callback kind carries one, so the failure reaches the application's
//!   log (and Grafana) rather than only stderr;
//! * `_hasFallback(Ret)` / `_fallback(Ret)` — the value to answer with.
//!
//! Both are generated because both need api.json facts the hand-written
//! prelude cannot have: which type carries a log sink, and which return types
//! the engine exports a `Default` for. `_boundaryFailure`, which calls them,
//! is generic and lives in the prelude.
//!
//! # Why the fallback is not simply a zeroed value
//!
//! Every other binding on this rollout returns "the kind's fallback" by
//! leaving alone an out-pointer the engine pre-filled with it. Zig's
//! trampoline IS the C function pointer: it has to materialize the value.
//! For a `void` kind that is free, and for a unit-enum kind (`Update`) any
//! variant is a valid value. For an aggregate it is only safe when libazul
//! exports a `Default` — a zeroed `AzImageRef` / `AzRefAny` / `AzInstantPtr`
//! is not an empty value but a handle Rust will then DROP, which is a worse
//! failure than the abort it would be replacing. Those kinds are logged and
//! then still abort; `_hasFallback` is how the prelude tells them apart.

use std::fmt::Write as _;

use super::{
    super::ir::{ArgRefKind, CodegenIR, FunctionDef, FunctionKind, TypeCategory},
    ffi_type_name,
};

/// Generate `_logThrough`, `_hasFallback` and `_fallback` (see the module
/// docs). Emitted between the `C` namespace and the runtime prelude; Zig's
/// top-level declarations are order-independent, so the prelude may call
/// them from above.
pub fn generate_boundary_helpers(ir: &CodegenIR) -> String {
    let mut out = String::new();
    out.push_str(
        "// ============================================================================\n",
    );
    out.push_str("// Callback boundary: a failing callback degrades, it does not abort.\n");
    out.push_str(
        "// ============================================================================\n\n",
    );
    emit_log_through(&mut out, ir);
    emit_fallbacks(&mut out, ir);
    out
}

// ============================================================================
// The log sink
// ============================================================================

/// The engine's log sink: an instance method of the shape
/// `(self, <unit enum> level, owned <string> message) -> ()`.
///
/// Matched by SHAPE, never by the method's name. A binding that keyed on it
/// being spelled a particular way would lose the capability the day api.json
/// renames it — and the failure would be silent, a callback that stops
/// reporting rather than a build error. `lang_java`'s `failure_logger`
/// matches the same shape for the same reason; across the whole API exactly
/// one function has it.
fn log_sink(ir: &CodegenIR) -> Option<&FunctionDef> {
    ir.functions.iter().find(|f| {
        matches!(f.kind, FunctionKind::Method | FunctionKind::MethodMut)
            // Reports, never answers: a logging call returns nothing.
            && f.return_type.is_none()
            // (self, level, message)
            && f.args.len() == 3
            && ir
                .find_enum(f.args[1].type_name.trim())
                .is_some_and(|e| !e.is_union && !e.variants.is_empty())
            // The engine's UTF-8 string type, by IR category.
            && matches!(f.args[2].ref_kind, ArgRefKind::Owned)
            && ir
                .find_struct(f.args[2].type_name.trim())
                .is_some_and(|s| matches!(s.category, TypeCategory::String))
    })
}

/// The level a boundary failure is reported at: the sink's level enum spelled
/// as its `Error` variant, or its first variant when it has none.
///
/// The severity of a failed downcast is not something any IR shape can tell
/// us, so this is the one place the name is the key — the same choice, in the
/// same words, `lang_java`'s `failure_logger` makes, so the two cannot drift
/// apart.
fn error_level(ir: &CodegenIR, level_enum: &str) -> Option<String> {
    let e = ir.find_enum(level_enum)?;
    let variant = e
        .variants
        .iter()
        .find(|v| v.name == "Error")
        .or_else(|| e.variants.first())?;
    Some(format!("C.{}_{}", ffi_type_name(level_enum), variant.name))
}

fn emit_log_through(out: &mut String, ir: &CodegenIR) {
    out.push_str("/// Report `msg` through the engine's log sink if `arg` carries one, and\n");
    out.push_str("/// answer whether it did.\n");
    out.push_str("///\n");
    out.push_str("/// One branch per API type with a sink, found by SHAPE (an instance method\n");
    out.push_str("/// taking a level enum and an owned string and returning nothing), never by\n");
    out.push_str("/// the method's name. `CT` is comptime, so only the matching branch is\n");
    out.push_str("/// analysed and the rest costs nothing.\n");
    out.push_str("fn _logThrough(comptime CT: type, arg: CT, msg: []const u8) bool {\n");

    // A `None` here is not a failure: it means api.json declares no sink at
    // all, and every report then goes to stderr (what `_boundaryFailure`
    // does when this answers false).
    if let Some(f) = log_sink(ir) {
        let level = error_level(ir, f.args[1].type_name.trim());
        if let Some(level) = level {
            let host = ffi_type_name(&f.class_name);
            let by_value = matches!(f.args[0].ref_kind, ArgRefKind::Owned);
            let _ = writeln!(out, "    if (CT == C.{}) {{", host);
            if by_value {
                let _ = writeln!(
                    out,
                    "        C.{}(arg, {}, _asAzString(msg));",
                    f.c_name, level
                );
            } else {
                out.push_str(
                    "        // The typedef hands the sink over BY VALUE; the method takes it\n",
                );
                out.push_str("        // by pointer, so the trampoline addresses its own copy.\n");
                out.push_str("        var sink = arg;\n");
                let _ = writeln!(
                    out,
                    "        C.{}(&sink, {}, _asAzString(msg));",
                    f.c_name, level
                );
            }
            out.push_str("        return true;\n");
            out.push_str("    }\n");
            // No typedef currently passes the sink by pointer, but the C ABI
            // allows it and the branch is free when nothing matches it.
            if !by_value {
                let _ = writeln!(out, "    if (CT == [*c]C.{}) {{", host);
                let _ = writeln!(
                    out,
                    "        C.{}(arg, {}, _asAzString(msg));",
                    f.c_name, level
                );
                out.push_str("        return true;\n");
                out.push_str("    }\n");
            }
        }
    }

    out.push_str("    return false;\n");
    out.push_str("}\n\n");
}

// ============================================================================
// The fallback answer
// ============================================================================

/// The Zig expression for `ret`'s fallback value, or `None` when the binding
/// cannot safely invent one.
///
/// Preference order, and why:
///
/// 1. the engine's own `Default` — a real, Rust-constructed value, safe both
///    to hand back and to free;
/// 2. a unit enum's first variant — a unit enum is an integer on the wire and
///    EVERY variant is a valid value of it, so this is always sound; for the
///    kind it actually matters for it is also the inert answer
///    (`Update.DoNothing`, which `lang_c`'s enum emission puts at 0);
/// 3. nothing. An owning handle with no `Default` (`ImageRef`, `RefAny`,
///    `InstantPtr`, `LogicalRectVec`, ...) has no zero value: a zeroed one is
///    a handle Rust would drop.
fn fallback_expr(ir: &CodegenIR, ret: &str) -> Option<String> {
    if let Some(f) = ir
        .functions_for_class(ret)
        .find(|f| f.kind == FunctionKind::Default)
    {
        return Some(format!("C.{}()", f.c_name));
    }
    let e = ir.find_enum(ret)?;
    if e.is_union {
        return None;
    }
    let v = e.variants.first()?;
    Some(format!("C.{}_{}", ffi_type_name(ret), v.name))
}

fn emit_fallbacks(out: &mut String, ir: &CodegenIR) {
    // Only the return types a trampoline can actually be instantiated for:
    // `_boundaryFailure` is generic over `Ret`, but `Ret` always comes from a
    // callback typedef. Deduplicated on the IR name; several unit enums share
    // the `c_uint` spelling in the `C` layer, so the first of those branches
    // answers for all of them - sound, because the first variant of every
    // generated C enum is 0.
    let mut seen: Vec<&str> = Vec::new();
    let mut arms: Vec<(String, Option<String>)> = Vec::new();
    for cb in &ir.callback_typedefs {
        let Some(ret) = cb.return_type.as_deref().map(str::trim) else {
            continue; // `void`, handled unconditionally below.
        };
        if ret.is_empty() || seen.contains(&ret) {
            continue;
        }
        seen.push(ret);
        arms.push((format!("C.{}", ffi_type_name(ret)), fallback_expr(ir, ret)));
    }

    out.push_str("/// Can a trampoline returning `Ret` answer without calling the application?\n");
    out.push_str("///\n");
    out.push_str("/// `void` always can. An aggregate only when libazul exports a `Default`\n");
    out.push_str("/// for it: a zeroed owning handle is not an empty value but a handle Rust\n");
    out.push_str("/// would then drop, which is worse than the abort it would replace.\n");
    out.push_str("fn _hasFallback(comptime Ret: type) bool {\n");
    out.push_str("    if (Ret == void) return true;\n");
    for (ty, fallback) in &arms {
        if fallback.is_some() {
            let _ = writeln!(out, "    if (Ret == {}) return true;", ty);
        }
    }
    out.push_str("    return false;\n");
    out.push_str("}\n\n");

    out.push_str("/// The value a trampoline answers with when the call could not be made.\n");
    out.push_str("/// Only ever instantiated behind `if (comptime _hasFallback(Ret))`.\n");
    out.push_str("fn _fallback(comptime Ret: type) Ret {\n");
    out.push_str("    if (Ret == void) return {};\n");
    for (ty, fallback) in &arms {
        if let Some(expr) = fallback {
            let _ = writeln!(out, "    if (Ret == {}) return {};", ty, expr);
        }
    }
    out.push_str("    unreachable;\n");
    out.push_str("}\n\n");
}

#[cfg(test)]
mod tests {
    use super::{super::c_decls::tests::fixture_ir, *};
    use crate::codegen::v2::ir::{FunctionArg, FunctionDef, StructDef, TypeCategory};

    fn arg(name: &str, ty: &str, rk: ArgRefKind) -> FunctionArg {
        FunctionArg {
            name: name.into(),
            type_name: ty.into(),
            ref_kind: rk,
            doc: None,
            callback_info: None,
        }
    }

    /// The fixture plus a log-sink-shaped method on an `Info` type, the level
    /// enum it takes, and the engine's string type.
    fn ir() -> CodegenIR {
        let mut ir = fixture_ir();
        ir.structs.push(StructDef {
            name: "String".into(),
            doc: vec![],
            fields: vec![],
            external_path: None,
            module: "dom".into(),
            derives: vec![],
            has_explicit_derive: false,
            custom_impls: vec![],
            is_boxed: false,
            repr: Some("C".into()),
            is_send_safe: true,
            generic_params: vec![],
            traits: Default::default(),
            category: TypeCategory::String,
            dependencies: vec![],
            sort_order: 0,
            needs_forward_decl: false,
            callback_wrapper_info: None,
        });
        ir.functions.push(FunctionDef {
            c_name: "AzInfo_log".into(),
            class_name: "Info".into(),
            method_name: "log".into(),
            kind: FunctionKind::Method,
            args: vec![
                arg("info", "Info", ArgRefKind::RefMut),
                arg("level", "Update", ArgRefKind::Owned),
                arg("message", "String", ArgRefKind::Owned),
            ],
            return_type: None,
            fn_body: None,
            doc: vec![],
            is_const: false,
            is_unsafe: false,
        });
        ir
    }

    /// The sink is found by shape, and reported at the level enum's first
    /// variant when it has no `Error` (the fixture's enum is `DoNothing` /
    /// `RefreshDom`).
    #[test]
    fn the_log_sink_is_found_by_shape() {
        let z = generate_boundary_helpers(&ir());
        assert!(z.contains("    if (CT == C.AzInfo) {\n"), "{z}");
        assert!(
            z.contains("C.AzInfo_log(&sink, C.AzUpdate_DoNothing, _asAzString(msg));"),
            "{z}"
        );
    }

    /// A callback returning `Update` can answer; the fixture exports no
    /// `Default` for it, so the answer is its first variant.
    #[test]
    fn a_unit_enum_return_always_has_a_fallback() {
        let z = generate_boundary_helpers(&ir());
        assert!(
            z.contains("    if (Ret == C.AzUpdate) return true;\n"),
            "{z}"
        );
        assert!(
            z.contains("    if (Ret == C.AzUpdate) return C.AzUpdate_DoNothing;\n"),
            "{z}"
        );
    }

    /// An API with no log-shaped method anywhere still generates a valid
    /// `_logThrough`; every report then goes to stderr.
    #[test]
    fn no_sink_still_generates_a_valid_helper() {
        // The bare fixture has no method of the sink's shape.
        let z = generate_boundary_helpers(&fixture_ir());
        assert!(z.contains("fn _logThrough(comptime CT: type, arg: CT, msg: []const u8) bool {\n    return false;\n}"), "{z}");
    }
}
