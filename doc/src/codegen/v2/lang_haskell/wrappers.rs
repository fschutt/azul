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

        // Return shape — Phase 3 covers wrapper-struct returns;
        // Phase 4 adds enum + primitive returns; Phase 5 adds
        // String + Vec returns via the existing decoder helpers
        // (`azStringToString`, `<lower>VecToList`) so users get
        // idiomatic `IO String` / `IO [Elem]` instead of raw
        // `IO T.AzString` / `IO T.<Y>Vec`.
        enum ReturnShape {
            Void,
            // Aggregate return that goes through `_via outPtr + peek`.
            // Covers wrapper structs (`Dom`), tagged-union enums
            // (`Update`), non-wrapper structs (`SvgRect`), and type
            // aliases (`GLuint`).
            Aggregate(String), // user-facing Haskell type (e.g. "T.Dom")
            // Primitive return — C ABI returns by value; FFI binding's
            // `IO <Haskell-prim>` shows up directly. No alloca needed.
            Primitive(String), // Haskell-prim name (e.g. "CSize", "Word32")
            // Phase 5: AzString return → decode to `IO String` via
            // `T.azStringToString val`. Saves the user a round-trip.
            StringDecoded,
            // Phase 5: Vec return → convert to `IO [Elem]` via the
            // existing `<lower>VecToList` helper. The helper is in
            // Azul.Types (peek-based shallow walk OR clone-via per V8).
            VecToList {
                elem_type: String,   // T.<elem> for the list element
                helper_name: String, // T.<lower><X>VecToList
            },
        }
        let return_shape: ReturnShape = match func
            .return_type
            .as_deref()
            .map(|r| r.trim())
        {
            None | Some("") | Some("void") | Some("()") | Some("c_void") => ReturnShape::Void,
            Some(rt) => {
                // Reject Option / Result / pointer / *Ref returns
                // (Phase 6+).
                if rt.starts_with("Option")
                    || rt.starts_with("Result")
                    || rt.contains('*') || rt.ends_with("Ref")
                {
                    continue;
                }
                // Phase 5 — String returns decode to Haskell `String`.
                if rt == "String" {
                    ReturnShape::StringDecoded
                } else if let Some(elem) = vec_struct_elem_type(rt, ir) {
                    // Phase 5 — Vec returns convert to `IO [Elem]`.
                    let vec_lower = super::lower_first(&haskell_data_name(rt));
                    let helper_name = format!("T.{}ToList", vec_lower);
                    let elem_ty = if elem == "u8" {
                        "Word8".to_string()
                    } else {
                        format!("T.{}", haskell_data_name(&elem))
                    };
                    ReturnShape::VecToList {
                        elem_type: elem_ty,
                        helper_name,
                    }
                } else {
                    let is_wrapper = ir.find_struct(rt).map(|s| {
                        delete_set.contains(rt) && matches!(
                            s.category,
                            super::super::ir::TypeCategory::Regular
                                | super::super::ir::TypeCategory::String
                                | super::super::ir::TypeCategory::CallbackDataPair
                        )
                    }).unwrap_or(false);
                    if is_wrapper
                        || ir.find_enum(rt).is_some()
                        || ir.find_struct(rt).is_some()
                        || ir.find_type_alias(rt).is_some()
                    {
                        ReturnShape::Aggregate(format!("T.{}", haskell_data_name(rt)))
                    } else {
                        // True primitive — direct map (no qualifier needed).
                        let prim_ty = super::types::haskell_field_type(
                            rt,
                            super::super::ir::FieldRefKind::Owned,
                            ir,
                        );
                        ReturnShape::Primitive(prim_ty)
                    }
                }
            }
        };

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

        // Type signature: `Class -> Arg1 -> ... -> IO <Return>`.
        let mut sig_parts = vec![haskell_data_name(&s.name)];
        for a in func.args.iter().skip(1) {
            sig_parts.push(haskell_arg_type(a, ir, delete_set));
        }
        let return_sig = match &return_shape {
            ReturnShape::Void => "IO ()".to_string(),
            ReturnShape::Aggregate(hs) => format!("IO {}", hs),
            ReturnShape::Primitive(hs) => format!("IO {}", hs),
            ReturnShape::StringDecoded => "IO String".to_string(),
            ReturnShape::VecToList { elem_type, .. } => format!("IO [{}]", elem_type),
        };
        sig_parts.push(return_sig);

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
                // `cshim.rs::is_c_primitive` semantics — the C shim
                // takes primitives by value (no pointer wrap) even when
                // the function is shimmed for an aggregate arg/return.
                // Mirror its predicate here so the Haskell-side arg
                // marshalling matches the FFI binding signature.
                let is_c_prim = matches!(
                    t,
                    "u8" | "u16" | "u32" | "u64"
                    | "i8" | "i16" | "i32" | "i64"
                    | "usize" | "isize" | "f32" | "f64"
                    | "bool" | "()" | "c_void" | "void"
                );
                if is_wrapper {
                    ArgEmit::Wrapper(haskell_data_name(t))
                } else if shimmed && !is_c_prim {
                    // Shimmed FFI binding expects `Ptr T.<X>` for this
                    // aggregate (enum / opaque struct) arg.
                    ArgEmit::Poke
                } else {
                    // Non-shimmed FFI takes the value directly, OR
                    // shimmed-but-primitive (still by value per cshim).
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

        // Phase 3+4+5 return-shape branching:
        //   - Aggregate / StringDecoded / VecToList: alloca outPtr +
        //     peek, with a Phase-5-specific postprocess (azStringToString
        //     or VecToList helper) before the final IO action.
        //   - Primitive: no alloca; FFI call's IO IS the final
        //     expression (or capture via `__result <-` when there
        //     are consume actions to interleave).
        let needs_return_alloca = matches!(
            return_shape,
            ReturnShape::Aggregate(_)
                | ReturnShape::StringDecoded
                | ReturnShape::VecToList { .. }
        );
        let primitive_return = matches!(return_shape, ReturnShape::Primitive(_));
        let has_consume_actions =
            !owned_wrapper_indices.is_empty() || self_consumed;
        let needs_result_capture = primitive_return && has_consume_actions;

        if needs_return_alloca {
            builder.line("alloca $ \\__outPtr -> do");
            builder.indent();
            call_args.push("__outPtr".to_string());
        }

        // The C call itself. For primitive returns with consume
        // actions, capture the result so we can `pure __result` at
        // the end.
        let call_expr = format!("{} {}", c_binding, call_args.join(" "));
        if needs_result_capture {
            builder.line(&format!("__result <- {}", call_expr));
        } else {
            builder.line(&call_expr);
        }

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

        // Phase 3+4+5 final return expression.
        match &return_shape {
            ReturnShape::Aggregate(_) => {
                // `peek __outPtr` reads the bytes the `_via` shim wrote.
                builder.line("peek __outPtr");
                builder.dedent();
            }
            ReturnShape::StringDecoded => {
                // peek to AzString value, decode via Types helper.
                builder.line("__azs <- peek __outPtr");
                builder.line("T.azStringToString __azs");
                builder.dedent();
            }
            ReturnShape::VecToList { helper_name, .. } => {
                // peek to <X>Vec value, decode via per-Vec helper.
                builder.line("__vec <- peek __outPtr");
                builder.line(&format!("{} __vec", helper_name));
                builder.dedent();
            }
            ReturnShape::Primitive(_) => {
                if needs_result_capture {
                    builder.line("pure __result");
                }
                // If primitive && no consumes, the bare C call IS the
                // final IO expression — nothing more to emit.
            }
            ReturnShape::Void => {
                // Void path — bare C call (or last consume action) is
                // the final IO expression.
            }
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
/// (`Dom`, `Button`); IR-known structs/enums/typedefs that AREN'T
/// wrappers get `T.` qualification (they live in `Azul.Types` which
/// the umbrella imports qualified-as-T); primitives route through
/// the canonical `super::types::haskell_field_type` mapping (no
/// qualifier needed — `CSize` / `Word32` / `CBool` etc. come from
/// `Foreign.C.Types` / `Data.Word` directly imported into `Azul.hs`).
fn haskell_arg_type(
    a: &super::super::ir::FunctionArg,
    ir: &CodegenIR,
    delete_set: &BTreeSet<&str>,
) -> String {
    umbrella_haskell_type(a.type_name.trim(), ir, delete_set)
}

/// Phase 5: detect Vec wrappers by their struct layout (ptr, len, cap,
/// destructor). Returns the element type when matched. Mirrors the
/// `detect_vec_elem_type` predicate in `lang_haskell/types.rs:477`.
fn vec_struct_elem_type(type_name: &str, ir: &CodegenIR) -> Option<String> {
    let s = ir.find_struct(type_name)?;
    if s.fields.len() != 4 {
        return None;
    }
    let f_ptr = &s.fields[0];
    let f_len = &s.fields[1];
    let f_cap = &s.fields[2];
    let _f_dst = &s.fields[3];
    if f_ptr.name != "ptr" || f_len.name != "len" || f_cap.name != "cap" {
        return None;
    }
    if f_len.type_name.trim() != "usize" || f_cap.type_name.trim() != "usize" {
        return None;
    }
    // Element type from the ptr field — strip pointer-syntax prefix
    // OR use the raw name when the IR carried it as `ref_kind = Ptr/PtrMut`.
    let raw = f_ptr.type_name.trim();
    let elem = raw
        .strip_prefix("*mut ")
        .or_else(|| raw.strip_prefix("*const "))
        .map(str::trim)
        .unwrap_or(raw);
    if elem.is_empty() {
        return None;
    }
    Some(elem.to_string())
}

/// Shared name-resolution for both arg and return types in the
/// umbrella module emit. See `haskell_arg_type` for the qualification
/// rules.
fn umbrella_haskell_type(
    t: &str,
    ir: &CodegenIR,
    delete_set: &BTreeSet<&str>,
) -> String {
    if ir.find_struct(t).is_some() && delete_set.contains(t) {
        return haskell_data_name(t);
    }
    if ir.find_struct(t).is_some()
        || ir.find_enum(t).is_some()
        || ir.find_type_alias(t).is_some()
    {
        return format!("T.{}", haskell_data_name(t));
    }
    super::types::haskell_field_type(t, super::super::ir::FieldRefKind::Owned, ir)
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

