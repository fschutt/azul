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
        // Item 18 (Phase 1): per-method Haskell wrapper functions
        // (`<lowerClass><MethodCamel> :: ...`). Auto-consumes Owned
        // wrapper-class args after the C call so the IORef tombstone
        // disarms the bracket finalizer. Currently scoped to
        // void-returning methods with primitive-or-wrapper-class args
        // only — non-void returns, callback args, and Vec/Option/Result
        // args / returns are out-of-scope for this phase.
        emit_haskell_method_wrappers(builder, s, ir, &delete_set);
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

/// Item 18 Phase 1 (Haskell): per-method wrapper functions named
/// `<lowerClass><MethodCamel>`. Scoped to void-returning methods
/// whose args are all primitives or wrapper-class types. Owned
/// wrapper-class args are followed by `consume<Class>` so the bracket
/// release skips `_delete` (the C side has taken ownership of the
/// bytes by value).
///
/// Skipped in Phase 1 — split into follow-up phases:
///   - non-void return types (need malloc + mkFoo path)
///   - callback args (need register<X>Callback)
///   - Vec / Option / Result args or returns
///   - String args (currently no `azStringFromString` emitter to
///     convert OCaml-side Strings to AzString round-trippably)
fn emit_haskell_method_wrappers(
    builder: &mut CodeBuilder,
    s: &super::super::ir::StructDef,
    ir: &CodegenIR,
    delete_set: &BTreeSet<&str>,
) {
    use super::super::ir::FunctionKind;
    let class_lower = super::lower_first(&haskell_data_name(&s.name));

    for func in ir.functions_for_class(&s.name) {
        // Phase 1: only Method | MethodMut. Skip DeepCopy (clone)
        // since it returns Self (non-void).
        if !matches!(func.kind, FunctionKind::Method | FunctionKind::MethodMut) {
            continue;
        }

        // Skip non-void returns (Phase 2).
        let return_void = func
            .return_type
            .as_deref()
            .map(|r| matches!(r.trim(), "" | "void" | "()" | "c_void"))
            .unwrap_or(true);
        if !return_void {
            continue;
        }

        // Skip when any arg has a callback or non-Phase-1 shape.
        let mut method_args_ok = true;
        for a in func.args.iter().skip(1) {
            if a.callback_info.is_some() {
                method_args_ok = false;
                break;
            }
            let t = a.type_name.trim();
            // Reject Vec / Option / Result / String / pointer / VecRef / Ref
            // args. (Option/Result auto-unwrap, Vec iteration, String round-trip,
            // and pointer-typedef aliases are all Phase 2+ work.)
            if t.starts_with("Vec") || t.starts_with("Option") || t.starts_with("Result")
                || t == "String" || t.contains('*')
                || t.ends_with("VecRef") || t.ends_with("Ref")
            {
                method_args_ok = false;
                break;
            }
            // Wrapper class OR primitive — accept; else reject. Wrapper
            // detection also gates on the struct's TypeCategory being a
            // proper "user-facing" one (Regular / String / CallbackDataPair),
            // not codegen scaffolding (VecRef / Boxed / Recursive /
            // DestructorOrClone / GenericTemplate).
            let is_wrapper = ir.find_struct(t)
                .map(|s| delete_set.contains(t) && matches!(
                    s.category,
                    super::super::ir::TypeCategory::Regular
                        | super::super::ir::TypeCategory::String
                        | super::super::ir::TypeCategory::CallbackDataPair
                ))
                .unwrap_or(false);
            let is_primitive = matches!(
                t,
                "bool" | "u8" | "i8" | "u16" | "i16" | "u32" | "i32"
                | "u64" | "i64" | "usize" | "isize" | "f32" | "f64"
                | "()" | "c_void"
            );
            let is_enum = ir.find_enum(t).is_some();
            if !is_wrapper && !is_primitive && !is_enum {
                method_args_ok = false;
                break;
            }
        }
        if !method_args_ok {
            continue;
        }

        // Phase 1 + 2: shimmed-with-non-wrapper args used to be a Phase
        // 1 stall; Phase 2 lights them up via `alloca + poke` per
        // primitive/enum arg. Wrapper-class args still pass `unWrapper`
        // directly. No filter rejection here — the body emit branches
        // per arg below.
        let shimmed = super::cshim::needs_shim(func);

        // First arg must be the receiver — Owned (consumed) or Ref/Mut.
        let self_consumed = func
            .args
            .first()
            .map(|a| matches!(a.ref_kind, super::super::ir::ArgRefKind::Owned))
            .unwrap_or(false);

        // Build the Haskell-side wrapper.
        let method_camel = super::lower_first(&pascal_from_snake(&func.method_name));
        let hs_fn_name = format!("{}{}", class_lower, upper_first(&method_camel));

        // The C-shim binding name — `c_<c_name>_via` for shimmed funcs,
        // `c_<c_name>` for primitive-only. `needs_shim` decides.
        let c_binding = if shimmed {
            format!("FFI.c_{}_via", func.c_name)
        } else {
            format!("FFI.c_{}", func.c_name)
        };

        // Type signature: `Class -> Arg1 -> ... -> IO ()`.
        let mut sig_parts = vec![haskell_data_name(&s.name)];
        for a in func.args.iter().skip(1) {
            sig_parts.push(haskell_arg_type(a, ir, delete_set));
        }
        sig_parts.push("IO ()".to_string());

        // Function body parameter names.
        let mut param_names: Vec<String> = vec!["self".to_string()];
        for (i, a) in func.args.iter().enumerate().skip(1) {
            let nm = if a.name.is_empty() {
                format!("a{}", i)
            } else {
                sanitize_haskell_ident(&a.name)
            };
            param_names.push(nm);
        }

        // Classify each non-self arg: wrapper / passthrough / poke.
        //   - Wrapper       → pass `(unWrapper arg)` directly.
        //   - Passthrough   → pass `arg` directly (primitives in
        //                     non-shimmed funcs, where the FFI takes
        //                     the value not a Ptr).
        //   - Poke          → shimmed primitive/enum arg; needs
        //                     `alloca + poke` because the `_via` shim
        //                     takes `Ptr T.<X>` for the by-value arg.
        enum ArgEmit {
            Wrapper(String),    // unWrapper(arg)
            Passthrough,        // arg verbatim
            Poke,               // alloca + poke, pass the alloca'd ptr
        }
        let arg_emits: Vec<ArgEmit> = func
            .args
            .iter()
            .skip(1)
            .map(|a| {
                let t = a.type_name.trim();
                let is_wrapper = ir.find_struct(t).map(|s| {
                    delete_set.contains(t) && matches!(
                        s.category,
                        super::super::ir::TypeCategory::Regular
                            | super::super::ir::TypeCategory::String
                            | super::super::ir::TypeCategory::CallbackDataPair
                    )
                }).unwrap_or(false);
                if is_wrapper {
                    ArgEmit::Wrapper(haskell_data_name(t))
                } else if shimmed {
                    // Shimmed FFI binding expects `Ptr T.<X>` for this arg.
                    ArgEmit::Poke
                } else {
                    // Non-shimmed FFI takes the value directly.
                    ArgEmit::Passthrough
                }
            })
            .collect();

        // Build the call-args list — `unWrapper self` first, then
        // per-arg expression. For Poke args, we use a placeholder ptr
        // name `__p_<argname>` which the alloca wrapper binds.
        let mut call_args: Vec<String> = Vec::new();
        call_args.push(format!("(un{} self)", haskell_data_name(&s.name)));
        for (i, kind) in arg_emits.iter().enumerate() {
            let pname = &param_names[i + 1];
            match kind {
                ArgEmit::Wrapper(wname) => {
                    call_args.push(format!("(un{} {})", wname, pname));
                }
                ArgEmit::Passthrough => {
                    call_args.push(pname.clone());
                }
                ArgEmit::Poke => {
                    call_args.push(format!("__p_{}", pname));
                }
            }
        }

        // Owned wrapper-class args to consume after the call.
        let owned_wrapper_indices: Vec<usize> = func
            .args
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(i, a)| {
                let t = a.type_name.trim();
                let is_wrapper = matches!(arg_emits[i - 1], ArgEmit::Wrapper(_));
                let _ = t;
                is_wrapper && matches!(a.ref_kind, super::super::ir::ArgRefKind::Owned)
            })
            .map(|(i, _)| i)
            .collect();

        if !func.doc.is_empty() {
            for d in &func.doc {
                builder.line(&format!("-- | {}", super::sanitize_doc(d)));
            }
        }
        builder.line(&format!("{} :: {}", hs_fn_name, sig_parts.join(" -> ")));
        builder.line(&format!(
            "{} {} = do",
            hs_fn_name,
            param_names.join(" ")
        ));
        builder.indent();

        // Emit nested `alloca` wrappers for each Poke arg. Each adds
        // one level of indent and binds `__p_<name>` for the body.
        let poke_args: Vec<(usize, &str)> = arg_emits
            .iter()
            .enumerate()
            .filter(|(_, k)| matches!(k, ArgEmit::Poke))
            .map(|(i, _)| (i + 1, param_names[i + 1].as_str()))
            .collect();
        for (_, name) in &poke_args {
            builder.line(&format!(
                "alloca $ \\__p_{} -> do",
                name
            ));
            builder.indent();
            builder.line(&format!("poke __p_{} {}", name, name));
        }

        // The C call itself.
        builder.line(&format!(
            "{} {}",
            c_binding,
            call_args.join(" ")
        ));
        // Owned-arg consume calls go AFTER the C call — at the
        // innermost-do level, which is the current `builder` cursor.
        for idx in &owned_wrapper_indices {
            let t = func.args[*idx].type_name.trim();
            let consume_fn = format!("consume{}", haskell_data_name(t));
            builder.line(&format!("{} {}", consume_fn, param_names[*idx]));
        }
        if self_consumed {
            let consume_fn = format!("consume{}", haskell_data_name(&s.name));
            builder.line(&format!("{} self", consume_fn));
        }

        // Close each `alloca`'s do-block (dedent once per Poke arg).
        for _ in &poke_args {
            builder.dedent();
        }
        builder.dedent();
        builder.blank();
    }
}

/// Map an IR `FunctionArg` to its Haskell-side type for the method-
/// wrapper signature. Wrapper classes use the bare wrapper name
/// (`Dom`, `Button`), primitives map via `Foreign.C.Types`, enums use
/// the bare data name from `Azul.Types`.
fn haskell_arg_type(
    a: &super::super::ir::FunctionArg,
    ir: &CodegenIR,
    delete_set: &BTreeSet<&str>,
) -> String {
    let t = a.type_name.trim();
    if let Some(_) = ir.find_struct(t) {
        if delete_set.contains(t) {
            return haskell_data_name(t);
        }
    }
    if ir.find_enum(t).is_some() {
        return format!("T.{}", haskell_data_name(t));
    }
    // GHC's Foreign.C.Types provides the C-side typedefs that match
    // the cdef-emitted bindings exactly. Use those for primitive args
    // so the user's Haskell type is identical to what the FFI
    // declaration expects — no manual `CBool 1` / `fromIntegral`
    // boilerplate at the call site.
    match t {
        "bool" => "CBool".to_string(),
        "u8" => "CUChar".to_string(),
        "i8" => "CChar".to_string(),
        "u16" => "CUShort".to_string(),
        "i16" => "CShort".to_string(),
        "u32" => "CUInt".to_string(),
        "i32" => "CInt".to_string(),
        "u64" => "CULong".to_string(),
        "i64" => "CLong".to_string(),
        "usize" => "CSize".to_string(),
        // GHC's Foreign.C.Types doesn't ship CSsize; Int64 is the
        // closest portable equivalent (covers ssize_t on every
        // supported 64-bit target).
        "isize" => "Int64".to_string(),
        // GHC FFI bindings (Foreign.C.Types) use CFloat / CDouble
        // for `float` / `double` C arg types — not the Haskell-native
        // `Float` / `Double`. They're newtypes around the same bits
        // but distinct at the type level.
        "f32" => "CFloat".to_string(),
        "f64" => "CDouble".to_string(),
        _ => "CSize /* TODO */".to_string(), // unreachable per the Phase 1 filter
    }
}

/// Convert snake_case (`add_child`) or already-camel (`addChild`) to
/// PascalCase (`AddChild`). Used for method-name suffixing in
/// `<class><Method>`.
fn pascal_from_snake(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut next_upper = true;
    for c in s.chars() {
        if c == '_' {
            next_upper = true;
        } else if next_upper {
            for u in c.to_uppercase() { out.push(u); }
            next_upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn upper_first(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    if let Some(c) = chars.next() {
        for u in c.to_uppercase() { out.push(u); }
    }
    out.push_str(chars.as_str());
    out
}

/// Sanitize a Rust identifier to a Haskell-valid one (lowercase, no
/// keyword collisions). Simple version — most arg names are already
/// snake_case and don't clash.
fn sanitize_haskell_ident(s: &str) -> String {
    let lowered = super::lower_first(s);
    match lowered.as_str() {
        // Reserved Haskell keywords.
        "case" | "class" | "data" | "default" | "deriving" | "do" | "else"
        | "if" | "import" | "in" | "infix" | "infixl" | "infixr" | "instance"
        | "let" | "module" | "newtype" | "of" | "then" | "type" | "where"
        | "as" | "hiding" | "qualified" => format!("{}_", lowered),
        _ => lowered,
    }
}

