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
        // Phase H.8: route Show / Eq through the C-ABI `_toDbgString` /
        // `_partialEq` helpers when api.json's `derive` / `custom_impls`
        // lists indicate the underlying type supports them.
        emit_show_instance_if_supported(builder, s, ir);
        emit_eq_instance_if_supported(builder, s, ir);
    }
    Ok(())
}

/// Emit `instance Show <X> where show = ...` routed through
/// `c_Az<X>_toDbgString_via` when TypeTraits.is_debug is set. The
/// helper allocates an AzString out-buffer, calls the C-side debug
/// formatter, decodes it back to a Haskell String.
fn emit_show_instance_if_supported(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
) {
    if !s.traits.is_debug {
        return;
    }
    // Confirm the helper actually exists at the FFI level (some types
    // with derive(Debug) at the Rust level still skip the C-ABI export
    // for category-related reasons).
    let helper = format!("Az{}_toDbgString_via", s.name);
    if !ir.functions.iter().any(|f| f.c_name == helper) {
        return;
    }
    let wname = haskell_data_name(&s.name);
    builder.line(&format!("instance Show {} where", wname));
    builder.indent();
    builder.line(&format!("show ({} p _) = System.IO.Unsafe.unsafePerformIO $", wname));
    builder.indent();
    builder.line("Foreign.Marshal.Alloc.alloca $ \\__buf -> do");
    builder.indent();
    builder.line(&format!("FFI.c_{} p __buf", helper));
    builder.line("__s <- Foreign.Storable.peek __buf");
    builder.line("T.azStringToString __s");
    builder.dedent();
    builder.dedent();
    builder.dedent();
    builder.blank();
}

/// Emit `instance Eq <X> where (==) = ...` routed through
/// `c_Az<X>_partialEq` when TypeTraits.is_partial_eq is set.
fn emit_eq_instance_if_supported(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
) {
    if !s.traits.is_partial_eq {
        return;
    }
    let helper = format!("Az{}_partialEq", s.name);
    if !ir.functions.iter().any(|f| f.c_name == helper) {
        return;
    }
    let wname = haskell_data_name(&s.name);
    builder.line(&format!("instance Eq {} where", wname));
    builder.indent();
    builder.line(&format!(
        "({} a _) == ({} b _) = System.IO.Unsafe.unsafePerformIO $ do",
        wname, wname
    ));
    builder.indent();
    builder.line(&format!("__cb <- FFI.c_{} a b", helper));
    builder.line("pure (__cb /= 0)");
    builder.dedent();
    builder.dedent();
    builder.blank();
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
    // (Azul.Types as T) so this wrapper isn't a duplicate
    // declaration of `Azul.Types.<Foo>`. RefAny is a phantom type
    // (`newtype RefAny a = ...`) so it needs an explicit argument.
    let qualified = if wname == "RefAny" {
        "T.RefAny ()".to_string()
    } else {
        format!("T.{}", wname)
    };
    // Two-field `data` rather than a newtype: pairs the raw pointer
    // with a per-instance consumed-flag IORef so the bracket /
    // dispose helpers can short-circuit when the C ABI has already
    // taken ownership of the bytes by value (DeepCopy / consuming-
    // self / owned-by-value wrapper arg). Without this tombstone
    // the bracket's release action would re-fire `c_AzFoo_delete`
    // on Rust-owned bytes — exactly the double-free landed in
    // 62094b885 for the JVM/CLR family. Pattern matches Ruby's
    // `ObjectSpace.undefine_finalizer` and Lua's `ffi.gc(c, nil)`.
    //
    // Field selectors stay backward-compatible: `un<Wname>` still
    // returns the raw `Ptr`, callers using only the pointer don't
    // need to know about the IORef. New `<wname>Consumed` selector
    // exposes the flag for callers that need it explicitly.
    builder.line(&format!(
        "data {} = {} {{ un{} :: !(Ptr ({})), {}Consumed :: !(IORef Bool) }}",
        wname,
        wname,
        wname,
        qualified,
        lower_first(&wname)
    ));
    builder.blank();
}

/// Lower-case the first letter (Haskell field-selector convention:
/// `Foo { fooConsumed = ... }`).
fn lower_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn emit_bracket_constructor(builder: &mut CodeBuilder, s: &StructDef, _ir: &CodegenIR) {
    let wname = haskell_data_name(&s.name);
    let with_name = format!("with{}", wname);
    let mk_name = format!("mk{}", wname);
    let dispose_name = format!("dispose{}", wname);
    let consume_name = format!("consume{}", wname);
    let raw_delete = format!("FFI.c_Az{}_delete", s.name);
    let consumed_field = format!("{}Consumed", lower_first(&wname));
    let qualified_inner = if wname == "RefAny" {
        "T.RefAny ()".to_string()
    } else {
        format!("T.{}", wname)
    };

    // `mkFoo`: pair a raw pointer with a fresh IORef False
    // (un-consumed). Used by every method that returns a fresh
    // wrapper instance — the new wrapper owns its own
    // tombstone.
    builder.line(&format!(
        "-- | Wrap a raw '{}' pointer in a managed '{}' with a fresh consumed-flag.",
        qualified_inner, wname
    ));
    builder.line(&format!(
        "{} :: Ptr ({}) -> IO {}",
        mk_name, qualified_inner, wname
    ));
    builder.line(&format!(
        "{} raw = do consumed <- newIORef False; pure ({} raw consumed)",
        mk_name, wname
    ));
    builder.blank();

    // `withFoo`: bracket-style RAII. The release action checks the
    // tombstone before firing `_delete` — if a consuming-self
    // method previously called `consumeFoo`, the release is a
    // no-op (Rust has already dropped the bytes).
    builder.line(&format!(
        "-- | RAII smart constructor for '{}'. Takes ownership of a raw",
        wname
    ));
    builder.line("-- pointer, runs the continuation, and releases the resource on the way");
    builder.line("-- out (even on exception). Skipped automatically when the wrapper has");
    builder.line(&format!("-- been consumed by a by-value C call (see '{}').", consume_name));
    builder.line(&format!(
        "{} :: Ptr ({}) -> ({} -> IO a) -> IO a",
        with_name, qualified_inner, wname
    ));
    builder.line(&format!(
        "{} raw action = do",
        with_name
    ));
    builder.indent();
    builder.line(&format!("h <- {} raw", mk_name));
    builder.line(&format!(
        "bracket (pure h) (\\h' -> do c <- readIORef ({} h'); Control.Monad.unless c ({} (un{} h'))) action",
        consumed_field, raw_delete, wname
    ));
    builder.dedent();
    builder.blank();
}

fn emit_dispose(builder: &mut CodeBuilder, s: &StructDef) {
    let wname = haskell_data_name(&s.name);
    let dispose_name = format!("dispose{}", wname);
    let consume_name = format!("consume{}", wname);
    let consumed_field = format!("{}Consumed", lower_first(&wname));
    let raw_delete = format!("FFI.c_Az{}_delete", s.name);

    builder.line(&format!(
        "-- | Explicit early disposal of '{}'. 'with{}' calls this for you on scope exit.",
        wname, wname
    ));
    builder.line(&format!("-- Short-circuits when the wrapper has been consumed."));
    builder.line(&format!("{} :: {} -> IO ()", dispose_name, wname));
    builder.line(&format!("{} h = do", dispose_name));
    builder.indent();
    builder.line(&format!("c <- readIORef ({} h)", consumed_field));
    builder.line(&format!(
        "Control.Monad.unless c $ do {} (un{} h); writeIORef ({} h) True",
        raw_delete, wname, consumed_field
    ));
    builder.dedent();
    builder.blank();

    // `consumeFoo`: explicit tombstone-set, called by codegen-emitted
    // bridges that pass `self` / args by value to the C ABI (the
    // bytes are now Rust-owned). Pattern matches Ruby's
    // `Azul._consume`, Lua's `azul._consume`, JVM `__consume()`.
    builder.line(&format!(
        "-- | Mark a '{}' as consumed (called by codegen-emitted bridges that",
        wname
    ));
    builder.line("-- transfer ownership of the underlying bytes to the C ABI by value).");
    builder.line(&format!("{} :: {} -> IO ()", consume_name, wname));
    builder.line(&format!(
        "{} h = writeIORef ({} h) True",
        consume_name, consumed_field
    ));
    builder.blank();
}

