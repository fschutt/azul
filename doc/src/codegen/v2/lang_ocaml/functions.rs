//! `foreign "<C symbol>" (...)` emission for the OCaml generator.
//!
//! Every IR `FunctionDef` that survives the inclusion filter becomes a
//! `let <ocaml_name> = foreign "<C symbol>" (<sig>)` value-binding at
//! the top level of `azul.ml`. The Ctypes-Foreign DSL builds the
//! libffi cif at runtime, so there is no compile step on the
//! consumer's machine.
//!
//! Naming:
//! - The OCaml-side identifier IS the C symbol, with only its first letter lowered so OCaml
//!   accepts it as a value: `AzApp_create` becomes `azApp_create`. The idiomatic surface stays
//!   textually distinct because it lives inside nested modules (`Azul.App.create`), and the
//!   mixed-case spelling keeps these values clear of the all-lowercase Ctypes `typ` values.
//! - The `foreign "..."` link name uses the **exact** C symbol from `func.c_name`. The dynamic
//!   linker is case-sensitive even though OCaml itself is.
//!
//! Argument and return-type coercion:
//! - Owned types: pass-by-value. Maps to the corresponding Ctypes view (`uint32_t`, `az_app`,
//!   etc.).
//! - References / mutable references / pointers: collapse to `(ptr T)` (typed pointer when `T` is
//!   known) or `(ptr void)` (opaque).

use anyhow::Result;

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{ArgRefKind, CodegenIR, FunctionDef, TypeCategory},
    },
    inner_pointer_form, map_type_to_ocaml, sanitize_doc, sanitize_identifier, to_snake_case,
};

// ============================================================================
// Top-level entry
// ============================================================================

pub fn emit_foreign_bindings_for(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
    belongs: &dyn Fn(&str) -> bool,
) -> Result<()> {
    builder
        .line("(* -------------------------------------------------------------------------- *)");
    builder
        .line("(* Raw FFI value bindings (`foreign \"<symbol>\" (...)`).                       *)");
    builder
        .line("(* -------------------------------------------------------------------------- *)");
    builder.blank();

    for func in ir.functions.iter().filter(|func| belongs(&func.class_name)) {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        emit_one(builder, func, ir);
    }
    Ok(())
}

fn should_emit_function(func: &FunctionDef, ir: &CodegenIR, config: &CodegenConfig) -> bool {
    // A trait entry point an api.json `derive` declares is not what the
    // `DestructorOrClone` exclusion below is for. That category is excluded
    // because those types' ordinary methods traffic in callback function
    // pointers this binding cannot marshal; `Az{T}_toDbgString(ptr) ->
    // AzString` traffics in neither, and is the same shape as the ~2800
    // `_toDbgString` declarations this binding already emits. Excluding it
    // wholesale is why every `*VecDestructor` declared `Debug` and named it
    // nowhere. (`*VecDestructor` is a tagged union, hence `find_enum`.)
    // RECURSIVE types are here for the same reason. `XmlNodeChild` and friends
    // are excluded below because their ORDINARY methods traffic in a shape
    // this binding cannot express by value - but `Az{T}_partialEq(a, b) ->
    // bool` and `Az{T}_toDbgString(ptr) -> AzString` take a pointer and return
    // a scalar, so the exclusion never applied to them. That is why the same
    // four types - `Xml`, `XmlNodeChild`, `XmlNodeChildVec`,
    // `ResultXmlXmlError` - showed up as the residue in fourteen bindings at
    // once: one cause, not fourteen.
    if func.kind.is_declared_capability()
        && (ir.find_enum(&func.class_name).is_some_and(|e| {
            matches!(
                e.category,
                TypeCategory::DestructorOrClone | TypeCategory::Recursive
            )
        }) || ir
            .find_struct(&func.class_name)
            .is_some_and(|s| s.category == TypeCategory::Recursive))
    {
        return config.should_include_type(&func.class_name);
    }

    if !config.should_include_type(&func.class_name) {
        return false;
    }
    if let Some(s) = ir.find_struct(&func.class_name) {
        if matches!(
            s.category,
            // A borrowed slice (`VecRef`) is NOT excluded: the C struct is
            // emitted, so its trait functions belong in the FFI layer too.
            TypeCategory::Recursive
                | TypeCategory::DestructorOrClone
                | TypeCategory::GenericTemplate
        ) {
            return false;
        }
        if !s.generic_params.is_empty() {
            return false;
        }
    }
    if let Some(e) = ir.find_enum(&func.class_name) {
        if matches!(
            e.category,
            TypeCategory::Recursive
                | TypeCategory::DestructorOrClone
                | TypeCategory::GenericTemplate
        ) {
            return false;
        }
        if !e.generic_params.is_empty() {
            return false;
        }
    }
    true
}

fn emit_one(builder: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    if !func.doc.is_empty() {
        for d in &func.doc {
            builder.line(&format!("(* {} *)", sanitize_doc(d)));
        }
    }

    let ocaml_name = ocaml_binding_name(&func.c_name);

    let mut atoms: Vec<String> = Vec::new();
    if func.args.is_empty() {
        // `void` arg list: signature is `void @-> returning T` per Ctypes.
        atoms.push("void".to_string());
    } else {
        for a in &func.args {
            let view = match a.ref_kind {
                ArgRefKind::Owned => map_type_to_ocaml(&a.type_name, ir),
                ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
                    inner_pointer_form(a.type_name.trim(), ir)
                }
            };
            // Sanitize the (otherwise unused) argument name for the
            // accompanying comment so reader can still see what's
            // what.
            let _ = sanitize_identifier(&to_snake_case(&a.name));
            atoms.push(view);
        }
    }

    let returns_void = func
        .return_type
        .as_ref()
        .map(|r| {
            let t = r.trim();
            matches!(t, "" | "void" | "()" | "c_void")
        })
        .unwrap_or(true);

    let return_view = if returns_void {
        "void".to_string()
    } else {
        let r = func.return_type.as_deref().unwrap_or("void");
        map_type_to_ocaml(r, ir)
    };

    let signature = format!("{} @-> returning {}", atoms.join(" @-> "), return_view);

    // Functions with a callback-wrapper arg bind the `<c_name>Struct`
    // C symbol (whole wrapper struct by value — matches the ctypes
    // struct view built above). The raw `<c_name>` takes a bare fn ptr
    // at the C ABI; binding it with a struct view crashed on click.
    // The OCaml-side value name stays derived from the original c_name.
    builder.line(&format!(
        "let {} = foreign \"{}\" ({})",
        ocaml_name,
        super::super::managed_host_invoker::managed_c_symbol(func),
        signature
    ));
}

// ============================================================================
// Helpers
// ============================================================================

/// Convert a C symbol like `AzApp_create` to the OCaml binding
/// identifier `azApp_create`: the symbol itself, with only its first
/// letter lowered to satisfy OCaml's "a value starts lowercase" rule.
///
/// Keeping the rest of the spelling is what makes the binding
/// greppable: the name in `azul.h`, the name in the `foreign "..."`
/// link string, the name in an OCaml backtrace and the name a caller
/// writes are one string, so a symbol can be followed from the header
/// to the call site without transliterating it in your head.
///
/// It is also what keeps the FFI values clear of the Ctypes `typ`
/// values. Those are `lower_snake_case` (`az_shape_circle`), and a
/// snake-cased symbol collides with them: `AzShape_circle` (the
/// factory) and `AzShapeCircle` (the payload struct) both snake to
/// `az_shape_circle`, so the later emit shadows the typ and the next
/// `foreign` signature naming it gets a function where a `typ` should
/// be. Every C symbol is `Az<Class>_<method>`, so index 1 of the
/// lowered form is always uppercase and can never equal an
/// all-lowercase typ name.
pub fn ocaml_binding_name(c_name: &str) -> String {
    let mut chars = c_name.chars();
    let lowered = match chars.next() {
        Some(first) => first.to_ascii_lowercase().to_string() + chars.as_str(),
        None => String::new(),
    };
    sanitize_identifier(&lowered)
}
