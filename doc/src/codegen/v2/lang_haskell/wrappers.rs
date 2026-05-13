//! Idiomatic Haskell wrappers: phantom-typed `newtype` handles plus
//! `bracket`-based smart constructors that take care of teardown.
//!
//! For every IR struct that has a matching `<TypeName>_delete` C
//! function we emit:
//!
//! - A `newtype <Type> = <Type> { un<Type> :: Ptr Raw<Type> }` handle.
//!   The phantom-typed nesting is the Haskell analogue of C++'s
//!   `unique_ptr` or C#'s `IDisposable`.
//! - A `with<Type> :: <ConstructorArgs> -> (<Type> -> IO a) -> IO a`
//!   smart constructor that allocates the C-side resource, hands it
//!   to the user-supplied continuation inside `Control.Exception.bracket`,
//!   and unconditionally releases it via the matching `_delete`
//!   function on the way out.
//!
//! Plain POD structs without a `_delete` get *no* wrapper — the user
//! constructs and consumes them through the raw FFI types directly,
//! since they are by-value and have no resource semantics.
//!
//! Tagged-union enums get a static factory per unit variant in the
//! umbrella module; payload variants are surfaced through the
//! algebraic data type from `Azul.Types` and the user reaches for the
//! raw FFI primitives if they need to round-trip them.

use anyhow::Result;
use std::collections::BTreeSet;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{ArgRefKind, CallbackTypedefDef, CodegenIR, FunctionKind, StructDef, TypeCategory};
use super::{haskell_data_name, sanitize_doc};

// ============================================================================
// Public entry: export list + bodies
// ============================================================================

/// Emit the `,`-separated list of names that appear in the umbrella
/// module's export header (already inside `Azul (...)`). Each line is
/// `, foo` — the umbrella header has already opened the list with `(`.
pub fn emit_umbrella_exports(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    let delete_set = collect_delete_targets(ir);
    let mut first = true;
    for s in &ir.structs {
        if !should_wrap(s, config) {
            continue;
        }
        let wname = haskell_data_name(&s.name);
        if delete_set.contains(s.name.as_str()) {
            // Haskell export lists are comma-SEPARATED, not
            // comma-prefixed. Emit the first entry without a leading
            // comma; subsequent entries use the leading comma so
            // diffs stay clean.
            let sep = if first { " " } else { ", " };
            first = false;
            builder.line(&format!("{}{}(..)", sep, wname));
            builder.line(&format!(", with{}", wname));
            builder.line(&format!(", dispose{}", wname));
        } else {
            // Plain types are not re-exported by Azul (user code imports
            // Azul.Types directly when needed). No entry here.
        }
    }
    Ok(())
}

/// Emit the actual newtype + bracket-constructor bodies inside the
/// umbrella module body.
pub fn emit_wrapper_bodies(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("-- ---------------------------------------------------------------------------");
    builder.line("-- Wrapper newtypes + 'bracket'-based smart constructors.");
    builder.line("--");
    builder.line("-- The Haskell analogue of C++'s unique_ptr / C#'s IDisposable. Each");
    builder.line("-- @with<Type>@ allocates the C-side resource, runs the supplied");
    builder.line("-- continuation, and releases the resource on the way out, even when");
    builder.line("-- the continuation throws.");
    builder.line("-- ---------------------------------------------------------------------------");
    builder.blank();

    let delete_set = collect_delete_targets(ir);
    for s in &ir.structs {
        if !should_wrap(s, config) {
            continue;
        }
        if !delete_set.contains(s.name.as_str()) {
            continue;
        }
        emit_newtype(builder, s);
        emit_bracket_constructor(builder, s, ir);
        emit_dispose(builder, s);
    }
    Ok(())
}

// emit_callback_register_helpers moved to functions.rs (same module as
// the FFI declarations they pair with). FFI.hs imports Azul.Types
// unqualified, which means the helper signatures don't need `T.`
// prefixes — they share the same scope as the mk_<X>_inner types.

// ============================================================================
// Filters
// ============================================================================

fn should_wrap(s: &StructDef, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&s.name) {
        return false;
    }
    if !s.generic_params.is_empty() {
        return false;
    }
    !matches!(
        s.category,
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::DestructorOrClone
            | TypeCategory::GenericTemplate
    )
}

fn collect_delete_targets(ir: &CodegenIR) -> BTreeSet<&str> {
    ir.functions
        .iter()
        .filter(|f| f.kind == FunctionKind::Delete)
        .map(|f| f.class_name.as_str())
        .collect()
}

// ============================================================================
// Wrapper bodies
// ============================================================================

fn emit_newtype(builder: &mut CodeBuilder, s: &StructDef) {
    let wname = haskell_data_name(&s.name);

    if !s.doc.is_empty() {
        for d in &s.doc {
            builder.line(&format!("-- | {}", sanitize_doc(d)));
        }
    }
    // Reference the underlying data type via `T.` qualifier
    // (Azul.Types as T) so this wrapper newtype isn't a duplicate
    // declaration of `Azul.Types.<Foo>`. RefAny is a phantom type
    // (`newtype RefAny a = ...`) so it needs an explicit argument.
    let qualified = if wname == "RefAny" {
        "T.RefAny ()".to_string()
    } else {
        format!("T.{}", wname)
    };
    builder.line(&format!(
        "newtype {} = {} {{ un{} :: Ptr ({}) }}",
        wname, wname, wname, qualified
    ));
    builder.blank();
}

fn emit_bracket_constructor(builder: &mut CodeBuilder, s: &StructDef, _ir: &CodegenIR) {
    let wname = haskell_data_name(&s.name);
    let with_name = format!("with{}", wname);
    let dispose_name = format!("dispose{}", wname);
    let raw_delete = format!("FFI.c_Az{}_delete", s.name);
    // The wrapper newtype wraps `Ptr T.<wname>`. The bracket-takes
    // form needs to accept that same type so the user's pointer
    // value matches the newtype constructor's expected payload.
    let qualified_inner = if wname == "RefAny" {
        "T.RefAny ()".to_string()
    } else {
        format!("T.{}", wname)
    };

    // We can't always synthesise a faithful constructor call from the
    // IR alone: most azul constructors take heterogeneous, type-rich
    // arguments and return the struct by value, while our wrapper
    // stores 'Ptr <Wrapper>' for symmetric RAII. The most honest shape
    // is therefore an 'alloca'-based bracket: the user supplies
    // ownership of an already-acquired raw pointer; we run the
    // continuation and call '_delete' on the way out unconditionally.
    //
    // This matches how Haskell's 'Foreign.Marshal.Alloc' wrappers are
    // typically used and lets the C-API layer (FFI module) own the
    // construction details.
    builder.line(&format!(
        "-- | RAII smart constructor for '{}'. Takes ownership of a raw",
        wname
    ));
    builder.line("-- pointer that the caller acquired through the FFI module, runs the");
    builder.line(&format!(
        "-- continuation, and releases the resource via '{}' on the way out,",
        dispose_name
    ));
    builder.line("-- even if the continuation throws (see 'Control.Exception.bracket').");
    builder.line(&format!(
        "{} :: Ptr ({}) -> ({} -> IO a) -> IO a",
        with_name, qualified_inner, wname
    ));
    builder.line(&format!(
        "{} raw action = bracket (pure ({} raw)) (\\h -> {} (un{} h)) action",
        with_name, wname, raw_delete, wname
    ));
    builder.blank();
}

fn emit_dispose(builder: &mut CodeBuilder, s: &StructDef) {
    let wname = haskell_data_name(&s.name);
    let dispose_name = format!("dispose{}", wname);

    builder.line(&format!(
        "-- | Explicit early disposal of '{}'. 'with{}' calls this for you on scope exit.",
        wname, wname
    ));
    builder.line(&format!("{} :: {} -> IO ()", dispose_name, wname));
    builder.line(&format!(
        "{} h = FFI.c_Az{}_delete (un{} h)",
        dispose_name, s.name, wname
    ));
    builder.blank();
}

