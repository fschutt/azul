//! The `Azul` umbrella module: the idiomatic layer user code imports.
//!
//! Everything here is derived from the IR; nothing is keyed on a method
//! or class name. The module has four parts:
//!
//! 1. **Managed wrapper types.** Every struct that owns a resource (has a `_delete`) or has methods
//!    becomes `data C = C { cRaw :: ForeignPtr T.C, cConsumed :: IORef Bool }`: a GC-managed,
//!    pinned buffer of exactly `sizeOf (undefined :: T.C)` bytes (the cbits layout oracle) plus a
//!    tombstone. Passing the value to a by-value C parameter *moves* the bytes into libazul and
//!    sets the tombstone; `disposeC` runs `_delete` unless the tombstone is set. The buffer itself
//!    is freed by the GC, never by a finalizer that calls into libazul — so nothing runs on a
//!    foreign thread.
//!
//! 2. **Constructors and methods**, one Haskell function per api.json function: `<class><Method>`,
//!    arguments in api.json order with the receiver LAST so builder chains read as `>>=` pipelines
//!    (`domCreateBody >>= domWithChild label`). Haskell `String`s marshal to `AzString`, `Bool` to
//!    `bool`, enums and POD structs travel as `Azul.Types` values, wrapper classes as wrappers,
//!    `RefAny` by clone.
//!
//! 3. **The host-handle `RefAny`.** `refAnyCreate` stores any `Typeable` Haskell value in a
//!    process-wide table keyed by a `Word64` handle and wraps the handle in a libazul `RefAny`
//!    (`AzRefAny_newHostHandle`); `refAnyGet` / `refAnyModify` look the value up again from any
//!    clone of that `RefAny`, and libazul calls the registered releaser when the last clone drops.
//!    This is the same protocol every managed binding (Lua, Ruby, OCaml, C#, ...) uses — see
//!    `core/src/host_invoker.rs`.
//!
//! 4. **Callbacks as closures.** For every callback kind in `HOST_INVOKER_KINDS` the module
//!    registers one invoker with libazul that dispatches on the handle stored in the callback's
//!    `ctx`, so a setter such as `buttonWithOnClick` takes a plain Haskell closure and every button
//!    can have its own. `windowCreateOptionsCreate` takes the layout closure the same way (spliced
//!    into the default options at the oracle's `offsetof`).

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{
            ArgRefKind, CallbackTypedefDef, CodegenIR, FunctionArg, FunctionDef, FunctionKind,
            MonomorphizedKind, StructDef, TypeCategory,
        },
        managed_host_invoker,
    },
    functions::{ffi_signature, host_invoker_signature},
    haskell_data_name, haskell_field_name, haskell_variant_name, lower_first, sanitize_doc,
};

// ============================================================================
// Entry point
// ============================================================================

/// Emit the complete `src/Azul.hs`.
pub fn emit_umbrella_module(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    let ctx = Ctx::new(ir, config);

    // The body first: the export header needs to know which names the
    // module declares before it can decide what to re-export from
    // `Azul.Types` without a clash.
    let mut body = CodeBuilder::new(&config.indent);
    emit_prelude(&mut body, &ctx);
    for s in &ir.structs {
        if ctx.wrapped.contains(&s.name) {
            emit_wrapper_class(&mut body, s, &ctx);
        }
    }
    emit_managed_refany(&mut body, &ctx);
    for cb in &ctx.kinds {
        emit_callback_kind(&mut body, cb, &ctx);
    }
    emit_ensure_managed(&mut body, &ctx);
    for s in &ir.structs {
        if ctx.wrapped.contains(&s.name) {
            emit_layout_factory(&mut body, s, &ctx);
            emit_class_functions(&mut body, s, &ctx);
        }
    }
    let body_src = body.finish();

    emit_header(builder, &ctx, &body_src);
    builder.line(&body_src);
    Ok(())
}

// ============================================================================
// Context
// ============================================================================

struct Ctx<'a> {
    ir: &'a CodegenIR,
    config: &'a CodegenConfig,
    /// api.json names of the structs that get a managed wrapper type.
    wrapped: BTreeSet<String>,
    /// Classes with a `_delete` export.
    deletable: BTreeSet<String>,
    /// Callback typedefs with a host invoker whose wrapper struct exists.
    kinds: Vec<&'a CallbackTypedefDef>,
    /// `AzString_copyFromBytes` / `AzString_delete` C names, when present.
    string_from_bytes: Option<String>,
    string_delete: Option<String>,
    /// `AzRefAny_clone` C name, when present.
    refany_clone: Option<String>,
}

impl<'a> Ctx<'a> {
    fn new(ir: &'a CodegenIR, config: &'a CodegenConfig) -> Self {
        let deletable: BTreeSet<String> = ir
            .functions
            .iter()
            .filter(|f| f.kind == FunctionKind::Delete)
            .map(|f| f.class_name.clone())
            .collect();
        let wrapped = ir
            .structs
            .iter()
            .filter(|s| should_wrap(s, config, &deletable, ir))
            .map(|s| s.name.clone())
            .collect();
        let kinds = managed_host_invoker::host_invoker_kinds(ir)
            .filter(|cb| {
                config.should_include_type(&cb.name)
                    && ir
                        .find_struct(managed_host_invoker::wrapper_name(cb))
                        .is_some()
            })
            .collect();
        let string_class = ir
            .structs
            .iter()
            .find(|s| s.category == TypeCategory::String)
            .map(|s| s.name.clone());
        let find = |class: &Option<String>, method: &str| {
            class.as_ref().and_then(|c| {
                ir.functions
                    .iter()
                    .find(|f| &f.class_name == c && f.method_name == method)
                    .map(|f| f.c_name.clone())
            })
        };
        let string_from_bytes = find(&string_class, "copy_from_bytes");
        let string_delete = string_class.as_ref().and_then(|c| {
            ir.functions
                .iter()
                .find(|f| &f.class_name == c && f.kind == FunctionKind::Delete)
                .map(|f| f.c_name.clone())
        });
        let refany_clone = ir
            .functions
            .iter()
            .find(|f| f.class_name == "RefAny" && f.kind == FunctionKind::DeepCopy)
            .map(|f| f.c_name.clone());
        Self {
            ir,
            config,
            wrapped,
            deletable,
            kinds,
            string_from_bytes,
            string_delete,
            refany_clone,
        }
    }

    fn is_string(&self, t: &str) -> bool {
        self.ir
            .find_struct(t)
            .map(|s| s.category == TypeCategory::String)
            .unwrap_or(false)
    }

    /// The `Azul.Types` value type of an IR type, qualified as `T.`.
    fn value_type(&self, t: &str) -> Option<String> {
        let t = t.trim();
        if let Some(s) = self.ir.find_struct(t) {
            if s.generic_params.is_empty() && self.config.should_include_type(t) {
                return Some(format!("T.{}", haskell_data_name(t)));
            }
            return None;
        }
        if let Some(e) = self.ir.find_enum(t) {
            if e.generic_params.is_empty() && self.config.should_include_type(t) {
                return Some(format!("T.{}", haskell_data_name(t)));
            }
            return None;
        }
        if self.ir.find_type_alias(t).is_some() && self.config.should_include_type(t) {
            return Some(format!("T.{}", haskell_data_name(t)));
        }
        None
    }
}

/// A struct gets a managed wrapper when it owns a resource (`_delete`) or
/// has any API function; plain POD structs without either stay
/// `Azul.Types` values.
fn should_wrap(
    s: &StructDef,
    config: &CodegenConfig,
    deletable: &BTreeSet<String>,
    ir: &CodegenIR,
) -> bool {
    if !config.should_include_type(&s.name) || !s.generic_params.is_empty() {
        return false;
    }
    if matches!(
        s.category,
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::DestructorOrClone
            | TypeCategory::GenericTemplate
    ) {
        return false;
    }
    deletable.contains(&s.name)
        || ir.functions_for_class(&s.name).any(|f| {
            matches!(
                f.kind,
                FunctionKind::Constructor
                    | FunctionKind::StaticMethod
                    | FunctionKind::Method
                    | FunctionKind::MethodMut
                    | FunctionKind::DeepCopy
                    | FunctionKind::Default
            )
        })
}

// ============================================================================
// Header: exports + imports
// ============================================================================

fn emit_header(builder: &mut CodeBuilder, ctx: &Ctx, body_src: &str) {
    builder.line("{- |");
    builder.line("Module      : Azul");
    builder.line("Description : Auto-generated Haskell bindings for the Azul GUI framework.");
    builder.line("");
    builder.line("The idiomatic surface: one managed wrapper type per resource-owning");
    builder.line("class, one function per api.json method with the receiver LAST so");
    builder.line("builder chains are @>>=@ pipelines, Haskell 'String's and 'Bool's at the");
    builder.line("boundary, 'refAnyCreate' / 'refAnyGet' / 'refAnyModify' for the type-erased");
    builder.line("application data, and callbacks as plain closures. The types this module");
    builder.line("does not wrap (enums, plain structs) are re-exported from \"Azul.Types\".");
    builder.line("");
    builder.line("Generated by azul-doc codegen v2 (lang_haskell). DO NOT EDIT MANUALLY.");
    builder.line("-}");
    builder.line("{-# LANGUAGE ScopedTypeVariables #-}");
    builder.line("{-# LANGUAGE FlexibleInstances #-}");
    builder.line(
        "{-# OPTIONS_GHC -Wno-unused-imports -Wno-unused-matches -Wno-name-shadowing \
         -Wno-unused-local-binds #-}",
    );
    builder.blank();
    builder.line("module Azul");
    builder.indent();
    builder.line("( module Azul");
    for item in reexports(ctx, body_src) {
        builder.line(&format!(", {}", item));
    }
    builder.line(") where");
    builder.dedent();
    builder.blank();
    builder.line("import qualified Azul.Types as T");
    builder.line("import qualified Azul.Internal.FFI as FFI");
    builder.line("import Control.Exception (SomeException, try)");
    builder.line("import Control.Monad (unless, when)");
    builder.line("import Data.Dynamic (Dynamic, Typeable, fromDynamic, toDyn)");
    builder.line("import Data.IORef (IORef, atomicModifyIORef', newIORef, readIORef, writeIORef)");
    builder.line("import qualified Data.Map.Strict as Map");
    builder.line("import Data.Int (Int8, Int16, Int32, Int64)");
    builder.line("import Data.Word (Word8, Word16, Word32, Word64)");
    builder.line("import Foreign.C.Types");
    builder.line(
        "import Foreign.ForeignPtr (ForeignPtr, mallocForeignPtrBytes, newForeignPtr_, \
         withForeignPtr)",
    );
    builder.line("import Foreign.Marshal.Alloc (alloca)");
    builder.line("import Foreign.Marshal.Array (withArrayLen)");
    builder.line("import Foreign.Marshal.Utils (copyBytes, fromBool, toBool)");
    builder.line("import Foreign.Ptr (Ptr, FunPtr, castPtr, plusPtr)");
    builder.line("import Foreign.Storable (Storable(..))");
    builder.line("import System.IO (hPutStrLn, stderr)");
    builder.line("import System.IO.Unsafe (unsafePerformIO)");
    builder.blank();
}

/// The `Azul.Types` entities re-exported through `Azul`: every type this
/// module does not wrap (enums, POD structs, aliases, callback typedefs)
/// plus the string helpers — minus anything whose name would clash with a
/// declaration of this module (GHC rejects conflicting exports).
fn reexports(ctx: &Ctx, body_src: &str) -> Vec<String> {
    let mut local: BTreeSet<String> = super::module_scope_declarations(body_src)
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    for name in &ctx.wrapped {
        let w = haskell_data_name(name);
        local.insert(format!("{}Raw", lower_first(&w)));
        local.insert(format!("{}Consumed", lower_first(&w)));
    }
    let clashes = |names: &[String]| names.iter().any(|n| local.contains(n));
    let mut out = Vec::new();
    for s in &ctx.ir.structs {
        if !super::types::should_emit_struct(s, ctx.config) || ctx.wrapped.contains(&s.name) {
            continue;
        }
        let hs = haskell_data_name(&s.name);
        let mut names = vec![hs.clone()];
        names.extend(
            s.fields
                .iter()
                .map(|f| haskell_field_name(&s.name, &f.name)),
        );
        if !clashes(&names) {
            out.push(format!("T.{}(..)", hs));
        }
    }
    for e in &ctx.ir.enums {
        if !super::types::should_emit_enum(e, ctx.config) || e.variants.is_empty() {
            continue;
        }
        let hs = haskell_data_name(&e.name);
        let mut names = vec![hs.clone()];
        names.extend(
            e.variants
                .iter()
                .map(|v| haskell_variant_name(&e.name, &v.name)),
        );
        if !clashes(&names) {
            out.push(format!("T.{}(..)", hs));
        }
    }
    for ta in &ctx.ir.type_aliases {
        if !ctx.config.should_include_type(&ta.name) {
            continue;
        }
        let hs = haskell_data_name(&ta.name);
        match ta.monomorphized_def.as_ref().map(|m| &m.kind) {
            Some(MonomorphizedKind::SimpleEnum { variants, .. }) => {
                if variants.is_empty() {
                    continue;
                }
                let mut names = vec![hs.clone()];
                names.extend(variants.iter().map(|v| haskell_variant_name(&ta.name, v)));
                if !clashes(&names) {
                    out.push(format!("T.{}(..)", hs));
                }
            }
            Some(MonomorphizedKind::Struct { fields }) => {
                let mut names = vec![hs.clone()];
                names.extend(fields.iter().map(|f| haskell_field_name(&ta.name, &f.name)));
                if !clashes(&names) {
                    out.push(format!("T.{}(..)", hs));
                }
            }
            Some(MonomorphizedKind::TaggedUnion { variants, .. }) => {
                if variants.is_empty() {
                    continue;
                }
                let mut names = vec![hs.clone()];
                names.extend(
                    variants
                        .iter()
                        .map(|v| haskell_variant_name(&ta.name, &v.name)),
                );
                if !clashes(&names) {
                    out.push(format!("T.{}(..)", hs));
                }
            }
            None => {
                let target = super::types::map_owned_type(&ta.target, ctx.ir);
                if target != hs && !local.contains(&hs) {
                    out.push(format!("T.{}", hs));
                }
            }
        }
    }
    for cb in &ctx.ir.callback_typedefs {
        if !ctx.config.should_include_type(&cb.name) {
            continue;
        }
        let hs = haskell_data_name(&cb.name);
        if !local.contains(&hs) {
            out.push(format!("T.{}(..)", hs));
        }
    }
    for helper in ["azStringToString", "encodeUtf8", "decodeUtf8"] {
        if !local.contains(helper) {
            out.push(format!("T.{}", helper));
        }
    }
    out
}

// ============================================================================
// Prelude: handle table, string marshalling
// ============================================================================

fn emit_prelude(b: &mut CodeBuilder, ctx: &Ctx) {
    b.line("-- ---------------------------------------------------------------------------");
    b.line("-- Host-handle table (see core/src/host_invoker.rs for the protocol).");
    b.line("-- ---------------------------------------------------------------------------");
    b.blank();
    b.line("{-# NOINLINE azulHandleTable #-}");
    b.line("azulHandleTable :: IORef (Map.Map Word64 Dynamic)");
    b.line("azulHandleTable = unsafePerformIO (newIORef Map.empty)");
    b.blank();
    b.line("{-# NOINLINE azulNextHandle #-}");
    b.line("azulNextHandle :: IORef Word64");
    b.line("azulNextHandle = unsafePerformIO (newIORef 1)");
    b.blank();
    b.line("{-# NOINLINE azulManagedInstalled #-}");
    b.line("azulManagedInstalled :: IORef Bool");
    b.line("azulManagedInstalled = unsafePerformIO (newIORef False)");
    b.blank();
    b.line("azulAllocHandle :: Dynamic -> IO Word64");
    b.line("azulAllocHandle v = do");
    b.indent();
    b.line("h <- atomicModifyIORef' azulNextHandle (\\n -> (n + 1, n))");
    b.line("atomicModifyIORef' azulHandleTable (\\m -> (Map.insert h v m, ()))");
    b.line("pure h");
    b.dedent();
    b.blank();
    b.line("azulLookupHandle :: Word64 -> IO (Maybe Dynamic)");
    b.line("azulLookupHandle h = Map.lookup h <$> readIORef azulHandleTable");
    b.blank();
    b.line("-- | Called by libazul (through the registered releaser) when the last");
    b.line("-- clone of a host-handle RefAny is dropped.");
    b.line("azulReleaseHandle :: Word64 -> IO ()");
    b.line(
        "azulReleaseHandle h = atomicModifyIORef' azulHandleTable (\\m -> (Map.delete h m, ()))",
    );
    b.blank();

    b.line("-- ---------------------------------------------------------------------------");
    b.line("-- String marshalling (Haskell String <-> AzString, UTF-8).");
    b.line("-- ---------------------------------------------------------------------------");
    b.blank();
    if let (Some(from_bytes), Some(delete)) = (&ctx.string_from_bytes, &ctx.string_delete) {
        b.line("-- | Pass a Haskell String as an AzString the callee takes ownership of.");
        b.line("withAzStringArg :: String -> (Ptr T.AzString -> IO a) -> IO a");
        b.line(
            "withAzStringArg s k = withArrayLen (T.encodeUtf8 s) $ \\n bytes -> alloca $ \\p -> do",
        );
        b.indent();
        b.line(&format!(
            "FFI.c_{}_via bytes 0 (fromIntegral n) p",
            from_bytes
        ));
        b.line("k p");
        b.dedent();
        b.blank();
        b.line("-- | Pass a Haskell String as an AzString the callee only borrows.");
        b.line("withAzStringRef :: String -> (Ptr T.AzString -> IO a) -> IO a");
        b.line("withAzStringRef s k = withAzStringArg s $ \\p -> do");
        b.indent();
        b.line("r <- k p");
        b.line(&format!("FFI.c_{} p", delete));
        b.line("pure r");
        b.dedent();
        b.blank();
        b.line("-- | Decode an AzString a C function returned, then release it.");
        b.line("azulTakeString :: Ptr T.AzString -> IO String");
        b.line("azulTakeString p = do");
        b.indent();
        b.line("s <- peek p");
        b.line("str <- T.azStringToString s");
        b.line(&format!("FFI.c_{} p", delete));
        b.line("pure str");
        b.dedent();
        b.blank();
    }
}

// ============================================================================
// Wrapper classes
// ============================================================================

fn emit_wrapper_class(b: &mut CodeBuilder, s: &StructDef, ctx: &Ctx) {
    let w = haskell_data_name(&s.name);
    let l = lower_first(&w);
    let t = format!("T.{}", w);

    b.line("-- ---------------------------------------------------------------------------");
    b.line(&format!("-- {}", w));
    b.line("-- ---------------------------------------------------------------------------");
    b.blank();
    for d in &s.doc {
        b.line(&format!("-- | {}", sanitize_doc(d)));
    }
    b.line(&format!(
        "data {w} = {w} {{ {l}Raw :: !(ForeignPtr {t}), {l}Consumed :: !(IORef Bool) }}",
        w = w,
        l = l,
        t = t
    ));
    b.blank();
    b.line(&format!(
        "-- | A fresh, uninitialised '{}' buffer (the C side fills it).",
        w
    ));
    b.line(&format!("alloc{} :: IO {}", w, w));
    b.line(&format!("alloc{} = do", w));
    b.indent();
    b.line(&format!(
        "fp <- mallocForeignPtrBytes (sizeOf (undefined :: {}))",
        t
    ));
    b.line("c <- newIORef False");
    b.line(&format!("pure ({} fp c)", w));
    b.dedent();
    b.blank();
    b.line(&format!(
        "-- | View memory libazul owns (a callback argument) as a '{}'; never deleted.",
        w
    ));
    b.line(&format!("borrow{} :: Ptr {} -> IO {}", w, t, w));
    b.line(&format!("borrow{} p = do", w));
    b.indent();
    b.line("fp <- newForeignPtr_ p");
    b.line("c <- newIORef True");
    b.line(&format!("pure ({} fp c)", w));
    b.dedent();
    b.blank();
    b.line(&format!(
        "with{} :: {} -> (Ptr {} -> IO a) -> IO a",
        w, w, t
    ));
    b.line(&format!("with{} h = withForeignPtr ({}Raw h)", w, l));
    b.blank();
    b.line(&format!(
        "-- | Mark a '{}' as moved into libazul (a by-value C parameter took it).",
        w
    ));
    b.line(&format!("consume{} :: {} -> IO ()", w, w));
    b.line(&format!(
        "consume{} h = writeIORef ({}Consumed h) True",
        w, l
    ));
    b.blank();
    b.line(&format!(
        "-- | Release a '{}' now, unless it has been consumed.",
        w
    ));
    b.line(&format!("dispose{} :: {} -> IO ()", w, w));
    if ctx.deletable.contains(&s.name) {
        b.line(&format!("dispose{} h = do", w));
        b.indent();
        b.line(&format!("c <- readIORef ({}Consumed h)", l));
        b.line("unless c $ do");
        b.indent();
        b.line(&format!("with{} h FFI.c_Az{}_delete", w, s.name));
        b.line(&format!("writeIORef ({}Consumed h) True", l));
        b.dedent();
        b.dedent();
    } else {
        b.line(&format!("dispose{} = consume{}", w, w));
    }
    b.blank();
    b.line(&format!("{}FromValue :: {} -> IO {}", l, t, w));
    b.line(&format!("{}FromValue v = do", l));
    b.indent();
    b.line(&format!("h <- alloc{}", w));
    b.line(&format!("with{} h (\\p -> poke p v)", w));
    b.line("pure h");
    b.dedent();
    b.blank();
    b.line(&format!("{}ToValue :: {} -> IO {}", l, w, t));
    b.line(&format!("{}ToValue h = with{} h peek", l, w));
    b.blank();

    emit_show_instance(b, s, ctx);
    emit_eq_instance(b, s, ctx);
}

/// `instance Show <X>` through `_toDbgString` when api.json derives Debug.
fn emit_show_instance(b: &mut CodeBuilder, s: &StructDef, ctx: &Ctx) {
    if !s.traits.is_debug {
        return;
    }
    let helper = format!("Az{}_toDbgString", s.name);
    if !ctx.ir.functions.iter().any(|f| f.c_name == helper) || ctx.string_delete.is_none() {
        return;
    }
    let w = haskell_data_name(&s.name);
    b.line(&format!("instance Show {} where", w));
    b.indent();
    b.line(&format!(
        "show h = unsafePerformIO $ with{} h $ \\p -> alloca $ \\buf -> do",
        w
    ));
    b.indent();
    b.line(&format!("FFI.c_{}_via p buf", helper));
    b.line("azulTakeString buf");
    b.dedent();
    b.dedent();
    b.blank();
}

/// `instance Eq <X>` through `_partialEq` when api.json derives PartialEq.
fn emit_eq_instance(b: &mut CodeBuilder, s: &StructDef, ctx: &Ctx) {
    if !s.traits.is_partial_eq {
        return;
    }
    let helper = format!("Az{}_partialEq", s.name);
    if !ctx.ir.functions.iter().any(|f| f.c_name == helper) {
        return;
    }
    let w = haskell_data_name(&s.name);
    b.line(&format!("instance Eq {} where", w));
    b.indent();
    b.line(&format!(
        "a == b = unsafePerformIO $ with{} a $ \\pa -> with{} b $ \\pb -> toBool <$> FFI.c_{} pa \
         pb",
        w, w, helper
    ));
    b.dedent();
    b.blank();
}

// ============================================================================
// Managed RefAny
// ============================================================================

fn emit_managed_refany(b: &mut CodeBuilder, ctx: &Ctx) {
    if !ctx.wrapped.contains("RefAny") {
        return;
    }
    b.line("-- ---------------------------------------------------------------------------");
    b.line("-- RefAny: type-erased application data, a host-handle into the table above.");
    b.line("-- ---------------------------------------------------------------------------");
    b.blank();
    b.line("-- | Wrap any Haskell value as a libazul 'RefAny'. Clones share the value;");
    b.line("-- the table entry is released when the last clone is dropped.");
    b.line("refAnyCreate :: Typeable a => a -> IO RefAny");
    b.line("refAnyCreate v = do");
    b.indent();
    b.line("azulEnsureManaged");
    b.line("h <- azulAllocHandle (toDyn v)");
    b.line("r <- allocRefAny");
    b.line("withRefAny r (FFI.c_AzRefAny_newHostHandle_via h)");
    b.line("pure r");
    b.dedent();
    b.blank();
    b.line("azulRefAnyHandle :: RefAny -> IO Word64");
    b.line("azulRefAnyHandle r = withRefAny r FFI.c_AzRefAny_getHostHandle");
    b.blank();
    b.line("-- | The value a 'RefAny' (or any clone of it) was created from, if it is");
    b.line("-- one created by 'refAnyCreate' with a value of this type.");
    b.line("refAnyGet :: Typeable a => RefAny -> IO (Maybe a)");
    b.line("refAnyGet r = do");
    b.indent();
    b.line("h <- azulRefAnyHandle r");
    b.line("entry <- azulLookupHandle h");
    b.line("pure (entry >>= fromDynamic)");
    b.dedent();
    b.blank();
    b.line("-- | Replace the value behind a 'RefAny' with @f@ applied to it; a no-op");
    b.line("-- when the stored value is not of this type.");
    b.line("refAnyModify :: Typeable a => RefAny -> (a -> a) -> IO ()");
    b.line("refAnyModify r f = do");
    b.indent();
    b.line("h <- azulRefAnyHandle r");
    b.line("atomicModifyIORef' azulHandleTable $ \\m ->");
    b.indent();
    b.line("case Map.lookup h m >>= fromDynamic of");
    b.indent();
    b.line("Just v -> (Map.insert h (toDyn (f v)) m, ())");
    b.line("Nothing -> (m, ())");
    b.dedent();
    b.dedent();
    b.dedent();
    b.blank();
    b.line("-- | Apply a pure update to the model behind a 'RefAny' and answer with the");
    b.line("-- given verdict: the one-expression body of a click handler,");
    b.line("-- @refAnyUpdate dat (\\m -> m { counter = counter m + 1 }) Update_RefreshDom@.");
    b.line("-- A 'RefAny' that does not hold a value of this type is left untouched.");
    b.line("refAnyUpdate :: Typeable a => RefAny -> (a -> a) -> r -> IO r");
    b.line("refAnyUpdate r f verdict = refAnyModify r f >> pure verdict");
    b.blank();
    if let Some(clone) = &ctx.refany_clone {
        b.line("-- | Hand a clone of a 'RefAny' to a by-value C parameter (the callee");
        b.line("-- owns the clone; the caller keeps its own).");
        b.line("withRefAnyClone :: RefAny -> (Ptr T.RefAny -> IO a) -> IO a");
        b.line("withRefAnyClone r k = withRefAny r $ \\src -> alloca $ \\dst -> do");
        b.indent();
        b.line(&format!("FFI.c_{}_via src dst", clone));
        b.line("k dst");
        b.dedent();
        b.blank();
    }
}

// ============================================================================
// Callback kinds: closure type, invoker, registration
// ============================================================================

/// How one callback argument reaches the user's closure.
enum CbArg {
    /// A wrapper class: borrowed view of libazul's memory.
    Borrow(String),
    /// A `Azul.Types` value (enum, POD struct, alias) or primitive: peeked.
    Peek(String),
}

enum CbRet {
    Void,
    /// A wrapper class: bytes moved into the out-pointer, wrapper consumed.
    Wrapper(String),
    /// A `Azul.Types` value or primitive: poked into the out-pointer.
    Poke(String),
}

fn cb_arg(a: &FunctionArg, ctx: &Ctx) -> Option<CbArg> {
    let t = a.type_name.trim();
    if ctx.wrapped.contains(t) {
        return Some(CbArg::Borrow(haskell_data_name(t)));
    }
    if super::cshim::is_c_primitive(t) {
        return Some(CbArg::Peek(hs_type_q(t, ctx)));
    }
    ctx.value_type(t).map(CbArg::Peek)
}

fn cb_ret(cb: &CallbackTypedefDef, ctx: &Ctx) -> Option<CbRet> {
    let Some(r) = cb.return_type.as_deref() else {
        return Some(CbRet::Void);
    };
    let t = r.trim();
    if matches!(t, "" | "void" | "()" | "c_void") {
        return Some(CbRet::Void);
    }
    if ctx.wrapped.contains(t) {
        return Some(CbRet::Wrapper(haskell_data_name(t)));
    }
    if super::cshim::is_c_primitive(t) {
        return Some(CbRet::Poke(hs_type_q(t, ctx)));
    }
    ctx.value_type(t).map(CbRet::Poke)
}

/// The Haskell type of the closure a user passes for callback kind `cb`,
/// e.g. `RefAny -> CallbackInfo -> IO T.Update`.
fn closure_type(cb: &CallbackTypedefDef, ctx: &Ctx) -> Option<String> {
    let mut parts = Vec::new();
    for a in &cb.args {
        parts.push(match cb_arg(a, ctx)? {
            CbArg::Borrow(w) => w,
            CbArg::Peek(t) => t,
        });
    }
    let ret = match cb_ret(cb, ctx)? {
        CbRet::Void => "()".to_string(),
        CbRet::Wrapper(w) => w,
        CbRet::Poke(t) => t,
    };
    parts.push(format!("IO {}", paren(&ret)));
    Some(parts.join(" -> "))
}

fn emit_callback_kind(b: &mut CodeBuilder, cb: &CallbackTypedefDef, ctx: &Ctx) {
    let kind = managed_host_invoker::wrapper_name(cb);
    let Some(closure) = closure_type(cb, ctx) else {
        b.line(&format!(
            "-- SKIPPED: callback kind {} (an argument or the return has no Haskell shape)",
            kind
        ));
        b.blank();
        return;
    };
    let ret = cb_ret(cb, ctx).unwrap();
    let wrapper_hs = haskell_data_name(kind);
    let sig = qualify_types(&host_invoker_signature(cb, ctx.ir), ctx);

    b.line("-- ---------------------------------------------------------------------------");
    b.line(&format!("-- {} as a Haskell closure", kind));
    b.line("-- ---------------------------------------------------------------------------");
    b.blank();
    b.line(&format!("type {}Fn = {}", wrapper_hs, closure));
    b.blank();
    emit_handler_class(b, cb, kind, &wrapper_hs, &closure, ctx);
    b.line(&format!("azulInvoke{} :: {}", kind, sig));
    let mut params: Vec<String> = vec!["handle".to_string()];
    for i in 0..cb.args.len() {
        params.push(format!("p{}", i));
    }
    let has_out = !matches!(ret, CbRet::Void);
    if has_out {
        params.push("out".to_string());
    }
    b.line(&format!("azulInvoke{} {} = do", kind, params.join(" ")));
    b.indent();
    b.line("entry <- azulLookupHandle handle");
    b.line("case entry >>= fromDynamic of");
    b.indent();
    b.line("Nothing -> pure ()");
    b.line(&format!("Just (f :: {}Fn) -> do", wrapper_hs));
    b.indent();
    let mut call = "f".to_string();
    for (i, a) in cb.args.iter().enumerate() {
        match cb_arg(a, ctx).unwrap() {
            CbArg::Borrow(w) => b.line(&format!("v{} <- borrow{} p{}", i, w, i)),
            CbArg::Peek(_) => b.line(&format!("v{} <- peek p{}", i, i)),
        }
        call.push_str(&format!(" v{}", i));
    }
    b.line(&format!("r <- try ({})", call));
    b.line("case r of");
    b.indent();
    b.line(&format!(
        "Left (e :: SomeException) -> hPutStrLn stderr (\"[azul] {} callback raised: \" ++ show e)",
        kind
    ));
    match ret {
        CbRet::Void => b.line("Right () -> pure ()"),
        CbRet::Poke(_) => b.line("Right v -> poke out v"),
        CbRet::Wrapper(w) => {
            b.line("Right v -> do");
            b.indent();
            b.line(&format!(
                "with{} v $ \\src -> copyBytes out src (sizeOf (undefined :: T.{}))",
                w, w
            ));
            b.line(&format!("consume{} v", w));
            b.dedent();
        }
    }
    b.dedent();
    b.dedent();
    b.dedent();
    b.dedent();
    b.blank();
    b.line(&format!(
        "-- | Store a closure in the handle table and hand libazul the {} that",
        kind
    ));
    b.line("-- dispatches to it (consumed by the C parameter it is passed to).");
    b.line(&format!(
        "azulRegister{} :: {}Fn -> (Ptr T.{} -> IO a) -> IO a",
        kind, wrapper_hs, wrapper_hs
    ));
    b.line(&format!("azulRegister{} f k = do", kind));
    b.indent();
    b.line("azulEnsureManaged");
    b.line("h <- azulAllocHandle (toDyn f)");
    b.line("alloca $ \\p -> do");
    b.indent();
    b.line(&format!("FFI.c_Az{}_createFromHostHandle_via h p", kind));
    b.line("k p");
    b.dedent();
    b.dedent();
    b.blank();
}

/// `class <K>Handler h where <k>Handler :: h -> <K>Fn`, with one instance
/// per shape a user may hand to a setter of this kind:
///
/// - the raw closure (`RefAny -> CallbackInfo -> IO T.Update`), as is;
/// - for kinds that return a value, a pure state transition on the model behind the `RefAny`:
///   `model -> CallbackInfo -> (model, T.Update)`. The binding reads the model, applies the
///   function, stores the new model and returns the verdict — `UI = f(data)` with `update :: model
///   -> (model, verdict)`, nothing mutated in user code;
/// - for kinds that return a wrapper (a `Dom`), the model-typed view `RefAny -> model ->
///   LayoutCallbackInfo -> IO Dom` (the `RefAny` stays in scope because attaching a child's
///   callback needs it).
///
/// A `RefAny` that does not hold the model type raises inside the
/// invoker, which logs it and leaves libazul's default return in place.
fn emit_handler_class(
    b: &mut CodeBuilder,
    cb: &CallbackTypedefDef,
    kind: &str,
    wrapper_hs: &str,
    closure: &str,
    ctx: &Ctx,
) {
    let class = format!("{}Handler", wrapper_hs);
    let method = format!("{}Handler", lower_first(wrapper_hs));
    b.line(&format!(
        "-- | Every shape 'azulRegister{}' accepts: the raw closure, or a",
        kind
    ));
    b.line("-- function of the model behind the RefAny (see the instances).");
    b.line(&format!("class {} h where", class));
    b.indent();
    b.line(&format!("{} :: h -> {}Fn", method, wrapper_hs));
    b.dedent();
    b.blank();
    b.line(&format!("instance {} ({}) where", class, closure));
    b.indent();
    b.line(&format!("{} = id", method));
    b.dedent();
    b.blank();

    // The model forms need the first argument to be the RefAny.
    let first_is_refany = cb
        .args
        .first()
        .map(|a| a.type_name.trim() == "RefAny")
        .unwrap_or(false);
    if !first_is_refany || !ctx.wrapped.contains("RefAny") {
        return;
    }
    let rest: Vec<String> = cb.args[1..]
        .iter()
        .map(|a| match cb_arg(a, ctx) {
            Some(CbArg::Borrow(w)) => w,
            Some(CbArg::Peek(t)) => t,
            None => String::new(),
        })
        .collect();
    let rest_vars: Vec<String> = (1..cb.args.len()).map(|i| format!("a{}", i)).collect();
    let rest_sig = if rest.is_empty() {
        String::new()
    } else {
        format!(
            "{} -> ",
            rest.iter()
                .map(|t| paren(t))
                .collect::<Vec<_>>()
                .join(" -> ")
        )
    };
    let rest_call = if rest_vars.is_empty() {
        String::new()
    } else {
        format!(" {}", rest_vars.join(" "))
    };
    match cb_ret(cb, ctx) {
        Some(CbRet::Poke(r)) => {
            b.line(&format!(
                "instance Typeable a => {} (a -> {}(a, {})) where",
                class, rest_sig, r
            ));
            b.indent();
            b.line(&format!("{} f dat{} = do", method, rest_call));
            b.indent();
            b.line("h <- azulRefAnyHandle dat");
            b.line(
                "r <- atomicModifyIORef' azulHandleTable $ \\m -> case Map.lookup h m >>= \
                 fromDynamic of",
            );
            b.indent();
            b.line(&format!(
                "Just v -> let (v', out) = f v{} in v' `seq` (Map.insert h (toDyn v') m, Just out)",
                rest_call
            ));
            b.line("Nothing -> (m, Nothing)");
            b.dedent();
            b.line(&format!(
                "maybe (ioError (userError \"{}: the RefAny does not hold the model type this \
                 handler expects\")) pure r",
                kind
            ));
            b.dedent();
            b.dedent();
            b.blank();
        }
        Some(CbRet::Wrapper(w)) => {
            b.line(&format!(
                "instance Typeable a => {} (RefAny -> a -> {}IO {}) where",
                class, rest_sig, w
            ));
            b.indent();
            b.line(&format!("{} f dat{} = do", method, rest_call));
            b.indent();
            b.line("v <- refAnyGet dat");
            b.line(&format!(
                "maybe (ioError (userError \"{}: the RefAny does not hold the model type this \
                 handler expects\")) (\\m -> f dat m{}) v",
                kind, rest_call
            ));
            b.dedent();
            b.dedent();
            b.blank();
        }
        _ => {}
    }
}

fn emit_ensure_managed(b: &mut CodeBuilder, ctx: &Ctx) {
    b.line("-- | Register the handle releaser and every callback-kind invoker with");
    b.line("-- libazul, once per process.");
    b.line("azulEnsureManaged :: IO ()");
    b.line("azulEnsureManaged = do");
    b.indent();
    b.line("installed <- atomicModifyIORef' azulManagedInstalled (\\s -> (True, s))");
    b.line("unless installed $ do");
    b.indent();
    b.line("releaser <- FFI.mk_HostHandleReleaser azulReleaseHandle");
    b.line("FFI.c_AzApp_setHostHandleReleaser releaser");
    for cb in &ctx.kinds {
        if closure_type(cb, ctx).is_none() {
            continue;
        }
        let kind = managed_host_invoker::wrapper_name(cb);
        b.line(&format!(
            "inv{} <- FFI.mk_{}Invoker azulInvoke{}",
            kind, kind, kind
        ));
        b.line(&format!("FFI.c_AzApp_set{}Invoker inv{}", kind, kind));
    }
    b.dedent();
    b.dedent();
    b.blank();
}

// ============================================================================
// Layout factory (WindowCreateOptions.create(layout))
// ============================================================================

/// `<class>Create :: <closure> -> IO <Class>` for a class whose raw
/// `create(LayoutCallbackType)` takes a bare fn pointer: build the default
/// value and splice the host-handle callback struct into the field the IR
/// says holds it, at the oracle's `offsetof`.
fn emit_layout_factory(b: &mut CodeBuilder, s: &StructDef, ctx: &Ctx) {
    let Some(info) = managed_host_invoker::layout_callback_factory_info(s, ctx.ir) else {
        return;
    };
    let Some(cb) = ctx
        .kinds
        .iter()
        .find(|cb| managed_host_invoker::wrapper_name(cb) == info.callback_wrapper)
    else {
        return;
    };
    if closure_type(cb, ctx).is_none() {
        return;
    }
    let w = haskell_data_name(&s.name);
    let l = lower_first(&w);
    let cb_hs = haskell_data_name(&info.callback_wrapper);

    // The byte offset of the callback field: the sum of the oracle offsets
    // along the field path (`window_state` in WindowCreateOptions, then
    // `layout_callback` in FullWindowState).
    let mut offsets = Vec::new();
    let mut owner = s.name.clone();
    for (i, seg) in info.field_path.iter().enumerate() {
        let owner_l = lower_first(&haskell_data_name(&owner));
        offsets.push(format!(
            "fromIntegral T.c_az_hs_offsetof_{}_{}",
            owner_l,
            super::super::lang_c::escape_cpp_keyword_for_c(seg)
        ));
        if i + 1 < info.field_path.len() {
            owner = info.field_types[i].clone();
        }
    }

    b.line(&format!(
        "-- | A '{}' whose layout is the given closure (the raw @create@ takes",
        w
    ));
    b.line("-- a bare function pointer and cannot carry a closure).");
    b.line(&format!(
        "{}Create :: {}Handler h => h -> IO {}",
        l, cb_hs, w
    ));
    b.line(&format!("{}Create f = do", l));
    b.indent();
    b.line(&format!("h <- alloc{}", w));
    b.line(&format!("with{} h FFI.c_{}_via", w, info.default_c_name));
    b.line(&format!(
        "azulRegister{} ({}Handler f) $ \\cb -> with{} h $ \\p ->",
        info.callback_wrapper,
        lower_first(&cb_hs),
        w
    ));
    b.indent();
    b.line(&format!(
        "copyBytes (castPtr p `plusPtr` ({})) (castPtr cb) (sizeOf (undefined :: T.{}))",
        offsets.join(" + "),
        cb_hs
    ));
    b.dedent();
    b.line("pure h");
    b.dedent();
    b.blank();
}

// ============================================================================
// Per-function wrappers
// ============================================================================

/// How one argument is marshalled.
enum ArgPlan {
    /// Passed as is (C primitive or raw pointer); `bool` converts.
    Direct {
        hs: String,
        is_bool: bool,
    },
    /// Haskell `String` -> AzString the callee owns / borrows.
    StringOwned,
    StringRef,
    /// `RefAny`: a clone is handed to a by-value parameter; borrowed otherwise.
    RefAnyClone,
    /// A wrapper class: moved (consumed) or borrowed.
    Wrapper {
        hs: String,
        consume: bool,
    },
    /// A `Azul.Types` value: `alloca` + `poke`, pointer passed.
    Value {
        hs: String,
    },
    /// A host-invoker callback: a closure.
    Closure {
        kind: String,
        hs: String,
    },
}

enum RetPlan {
    Void,
    Direct { hs: String, is_bool: bool },
    HsString,
    Wrapper(String),
    Value(String),
}

fn arg_plan(a: &FunctionArg, ctx: &Ctx) -> Option<ArgPlan> {
    let t = a.type_name.trim();
    let by_value = matches!(a.ref_kind, ArgRefKind::Owned);
    if managed_host_invoker::is_callback_wrapper(t) {
        let cb = ctx
            .kinds
            .iter()
            .find(|cb| managed_host_invoker::wrapper_name(cb) == t)?;
        let hs = closure_type(cb, ctx)?;
        return Some(ArgPlan::Closure {
            kind: t.to_string(),
            hs: haskell_data_name(t),
        })
        .filter(|_| !hs.is_empty());
    }
    if ctx.ir.callback_typedefs.iter().any(|c| c.name == t) {
        return None;
    }
    if t.starts_with("*const ") || t.starts_with("*mut ") || t.starts_with('&') {
        return Some(ArgPlan::Direct {
            hs: hs_type_q(t, ctx),
            is_bool: false,
        });
    }
    if super::cshim::is_c_primitive(t) {
        if by_value {
            return Some(ArgPlan::Direct {
                hs: hs_type_q(t, ctx),
                is_bool: t == "bool",
            });
        }
        return Some(ArgPlan::Direct {
            hs: format!("Ptr {}", paren(&hs_type_q(t, ctx))),
            is_bool: false,
        });
    }
    if ctx.is_string(t) && ctx.string_from_bytes.is_some() && ctx.string_delete.is_some() {
        return Some(if by_value {
            ArgPlan::StringOwned
        } else {
            ArgPlan::StringRef
        });
    }
    if t == "RefAny" && ctx.wrapped.contains(t) {
        return Some(if by_value && ctx.refany_clone.is_some() {
            ArgPlan::RefAnyClone
        } else {
            ArgPlan::Wrapper {
                hs: "RefAny".to_string(),
                consume: false,
            }
        });
    }
    if ctx.wrapped.contains(t) {
        return Some(ArgPlan::Wrapper {
            hs: haskell_data_name(t),
            consume: by_value,
        });
    }
    if matches!(a.ref_kind, ArgRefKind::Ptr | ArgRefKind::PtrMut) {
        return Some(ArgPlan::Direct {
            hs: format!("Ptr {}", paren(&hs_type_q(t, ctx))),
            is_bool: false,
        });
    }
    ctx.value_type(t).map(|hs| ArgPlan::Value { hs })
}

fn ret_plan(func: &FunctionDef, ctx: &Ctx) -> Option<RetPlan> {
    let Some(r) = func.return_type.as_deref() else {
        return Some(RetPlan::Void);
    };
    let t = r.trim();
    if matches!(t, "" | "void" | "()" | "c_void") {
        return Some(RetPlan::Void);
    }
    if t.starts_with("*const ") || t.starts_with("*mut ") || t.starts_with('&') {
        return Some(RetPlan::Direct {
            hs: hs_type_q(t, ctx),
            is_bool: false,
        });
    }
    if super::cshim::is_c_primitive(t) {
        return Some(RetPlan::Direct {
            hs: hs_type_q(t, ctx),
            is_bool: t == "bool",
        });
    }
    if ctx.is_string(t) && ctx.string_delete.is_some() {
        return Some(RetPlan::HsString);
    }
    if ctx.wrapped.contains(t) {
        return Some(RetPlan::Wrapper(haskell_data_name(t)));
    }
    ctx.value_type(t).map(RetPlan::Value)
}

/// Haskell-side name of one api.json function: `<class><Method>`.
fn function_name(s: &StructDef, func: &FunctionDef) -> String {
    let class = lower_first(&haskell_data_name(&s.name));
    let method = match func.kind {
        FunctionKind::DeepCopy => "Clone".to_string(),
        FunctionKind::Default => "Default".to_string(),
        _ => pascal(&func.method_name),
    };
    format!("{}{}", class, method)
}

fn emit_class_functions(b: &mut CodeBuilder, s: &StructDef, ctx: &Ctx) {
    let has_factory = managed_host_invoker::layout_callback_factory_info(s, ctx.ir).is_some();
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for func in ctx.ir.functions_for_class(&s.name) {
        if !matches!(
            func.kind,
            FunctionKind::Constructor
                | FunctionKind::StaticMethod
                | FunctionKind::Method
                | FunctionKind::MethodMut
                | FunctionKind::DeepCopy
                | FunctionKind::Default
        ) {
            continue;
        }
        if !super::functions::should_emit_function(func, ctx.ir, ctx.config) {
            continue;
        }
        let name = function_name(s, func);
        // The layout factory replaces the raw `create(fn ptr)`.
        if has_factory && name == format!("{}Create", lower_first(&haskell_data_name(&s.name))) {
            continue;
        }
        if let Some(prev) = seen.get(&name) {
            b.line(&format!(
                "-- SKIPPED: {} ({}) would repeat {} ({})",
                name, func.c_name, name, prev
            ));
            b.blank();
            continue;
        }
        // The IR spells a method's receiver as its first argument (named
        // after the class, `button`, or `instance` for `_clone`); static
        // functions and constructors have none.
        let receiver = matches!(
            func.kind,
            FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
        ) && !func.args.is_empty();
        let mut plans = Vec::with_capacity(func.args.len());
        let mut ok = true;
        for a in &func.args {
            match arg_plan(a, ctx) {
                Some(p) => plans.push(p),
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            b.line(&format!(
                "-- SKIPPED: {} ({}): an argument has no Haskell shape yet",
                name, func.c_name
            ));
            b.blank();
            continue;
        }
        let Some(ret) = ret_plan(func, ctx) else {
            b.line(&format!(
                "-- SKIPPED: {} ({}): the return type has no Haskell shape yet",
                name, func.c_name
            ));
            b.blank();
            continue;
        };
        seen.insert(name.clone(), func.c_name.clone());
        emit_function(b, func, &name, receiver, &plans, &ret, ctx);
    }
}

fn emit_function(
    b: &mut CodeBuilder,
    func: &FunctionDef,
    name: &str,
    receiver: bool,
    plans: &[ArgPlan],
    ret: &RetPlan,
    ctx: &Ctx,
) {
    let sig = ffi_signature(func, ctx.ir);

    // Parameter names: `a1..` in api.json order, the receiver last as
    // `self`.
    let params: Vec<String> = (0..func.args.len())
        .map(|i| {
            if receiver && i == 0 {
                "self".to_string()
            } else {
                format!("a{}", i)
            }
        })
        .collect();
    let mut order: Vec<usize> = (0..func.args.len()).collect();
    if receiver {
        order.remove(0);
        order.push(0);
    }

    let param_ty = |i: usize| -> String {
        match &plans[i] {
            ArgPlan::Direct { hs, is_bool } => {
                if *is_bool {
                    "Bool".to_string()
                } else {
                    hs.clone()
                }
            }
            ArgPlan::StringOwned | ArgPlan::StringRef => "String".to_string(),
            ArgPlan::RefAnyClone => "RefAny".to_string(),
            ArgPlan::Wrapper { hs, .. } => hs.clone(),
            ArgPlan::Value { hs } => hs.clone(),
            ArgPlan::Closure { .. } => format!("h{}", i),
        }
    };
    let constraints: Vec<String> = plans
        .iter()
        .enumerate()
        .filter_map(|(i, p)| match p {
            ArgPlan::Closure { hs, .. } => Some(format!("{}Handler h{}", hs, i)),
            _ => None,
        })
        .collect();
    let ret_ty = match ret {
        RetPlan::Void => "()".to_string(),
        RetPlan::Direct { hs, is_bool } => {
            if *is_bool {
                "Bool".to_string()
            } else {
                hs.clone()
            }
        }
        RetPlan::HsString => "String".to_string(),
        RetPlan::Wrapper(w) => w.clone(),
        RetPlan::Value(t) => t.clone(),
    };
    let mut sig_parts: Vec<String> = order.iter().map(|&i| paren(&param_ty(i))).collect();
    sig_parts.push(format!("IO {}", paren(&ret_ty)));

    for d in &func.doc {
        b.line(&format!("-- | {}", sanitize_doc(d)));
    }
    let context = if constraints.is_empty() {
        String::new()
    } else {
        format!("({}) => ", constraints.join(", "))
    };
    b.line(&format!(
        "{} :: {}{}",
        name,
        context,
        sig_parts.join(" -> ")
    ));
    let param_list: Vec<String> = order.iter().map(|&i| params[i].clone()).collect();
    b.line(&format!("{} {} = do", name, param_list.join(" ")));
    b.indent();

    // Openers: one nested bracket per marshalled argument, plus the
    // out-pointer for aggregate returns.
    let mut openers: Vec<(String, Vec<String>)> = Vec::new();
    let mut call_args: Vec<String> = Vec::new();
    for (i, plan) in plans.iter().enumerate() {
        let p = &params[i];
        let ptr = format!("__p{}", i);
        match plan {
            ArgPlan::Direct { is_bool, .. } => {
                call_args.push(if *is_bool {
                    format!("(fromBool {})", p)
                } else {
                    p.clone()
                });
            }
            ArgPlan::StringOwned => {
                openers.push((format!("withAzStringArg {} $ \\{} -> do", p, ptr), vec![]));
                call_args.push(ptr);
            }
            ArgPlan::StringRef => {
                openers.push((format!("withAzStringRef {} $ \\{} -> do", p, ptr), vec![]));
                call_args.push(ptr);
            }
            ArgPlan::RefAnyClone => {
                openers.push((format!("withRefAnyClone {} $ \\{} -> do", p, ptr), vec![]));
                call_args.push(ptr);
            }
            ArgPlan::Wrapper { hs, .. } => {
                openers.push((format!("with{} {} $ \\{} -> do", hs, p, ptr), vec![]));
                call_args.push(ptr);
            }
            ArgPlan::Value { .. } => {
                openers.push((
                    format!("alloca $ \\{} -> do", ptr),
                    vec![format!("poke {} {}", ptr, p)],
                ));
                call_args.push(ptr);
            }
            ArgPlan::Closure { kind, hs } => {
                openers.push((
                    format!(
                        "azulRegister{} ({}Handler {}) $ \\{} -> do",
                        kind,
                        lower_first(hs),
                        p,
                        ptr
                    ),
                    vec![],
                ));
                call_args.push(ptr);
            }
        }
    }
    let mut tail: Vec<String> = Vec::new();
    let mut captures = false;
    match ret {
        RetPlan::Void => {}
        RetPlan::Direct { .. } => {
            captures = true;
        }
        RetPlan::HsString => {
            captures = true;
            openers.push(("alloca $ \\__po -> do".to_string(), vec![]));
            call_args.push("__po".to_string());
            tail.push("azulTakeString __po".to_string());
        }
        RetPlan::Value(_) => {
            captures = true;
            openers.push(("alloca $ \\__po -> do".to_string(), vec![]));
            call_args.push("__po".to_string());
            tail.push("peek __po".to_string());
        }
        RetPlan::Wrapper(w) => {
            b.line(&format!("__out <- alloc{}", w));
            openers.push((format!("with{} __out $ \\__po -> do", w), vec![]));
            call_args.push("__po".to_string());
        }
    }
    debug_assert_eq!(
        call_args.len(),
        sig.arg_types.len() + usize::from(sig.out_type.is_some()),
        "{}: wrapper call shape must match the FFI import",
        func.c_name
    );

    let call = format!("FFI.{} {}", sig.binding, call_args.join(" "))
        .trim_end()
        .to_string();
    let capture = if captures { "__r <- " } else { "" };
    if openers.is_empty() {
        b.line(&format!("{}{}", capture, call));
    } else {
        let depth = openers.len();
        for (i, (line, extra)) in openers.iter().enumerate() {
            let prefix = if i == 0 { capture } else { "" };
            b.line(&format!("{}{}", prefix, line));
            b.indent();
            for e in extra {
                b.line(e);
            }
        }
        b.line(&call);
        for t in &tail {
            b.line(t);
        }
        for _ in 0..depth {
            b.dedent();
        }
    }

    // Moves: the callee took the bytes of every by-value wrapper argument.
    for (i, plan) in plans.iter().enumerate() {
        if let ArgPlan::Wrapper { hs, consume: true } = plan {
            b.line(&format!("consume{} {}", hs, params[i]));
        }
    }
    match ret {
        RetPlan::Void => {}
        RetPlan::Direct { is_bool, .. } => {
            if *is_bool {
                b.line("pure (toBool __r)");
            } else {
                b.line("pure __r");
            }
        }
        RetPlan::HsString | RetPlan::Value(_) => b.line("pure __r"),
        RetPlan::Wrapper(_) => b.line("pure __out"),
    }
    b.dedent();
    b.blank();
}

// ============================================================================
// Helpers
// ============================================================================

/// The umbrella-side spelling of an IR type: primitives as in
/// `Azul.Types`, IR types qualified as `T.`.
fn hs_type_q(t: &str, ctx: &Ctx) -> String {
    let t = t.trim();
    for prefix in ["*const ", "*mut ", "&mut ", "&"] {
        if let Some(inner) = t.strip_prefix(prefix) {
            let inner = inner.trim();
            if inner.is_empty() || matches!(inner, "c_void" | "void" | "()") {
                return "Ptr ()".to_string();
            }
            if matches!(inner, "c_char" | "u8") {
                return "Ptr Word8".to_string();
            }
            return format!("Ptr {}", paren(&hs_type_q(inner, ctx)));
        }
    }
    if super::cshim::is_c_primitive(t) {
        return super::types::map_owned_type(t, ctx.ir);
    }
    match ctx.value_type(t) {
        Some(q) => q,
        None => "Ptr ()".to_string(),
    }
}

/// Qualify the `Azul.Types` names inside an FFI-side signature (which
/// spells them unqualified) as `T.` for the umbrella module.
fn qualify_types(sig: &str, ctx: &Ctx) -> String {
    let mut out = String::with_capacity(sig.len() + 16);
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        if word.is_empty() {
            return;
        }
        let is_type_name = word
            .chars()
            .next()
            .map(|c| c.is_ascii_uppercase())
            .unwrap_or(false);
        let known = ctx
            .ir
            .structs
            .iter()
            .any(|s| haskell_data_name(&s.name) == *word)
            || ctx
                .ir
                .enums
                .iter()
                .any(|e| haskell_data_name(&e.name) == *word)
            || ctx
                .ir
                .type_aliases
                .iter()
                .any(|a| haskell_data_name(&a.name) == *word)
            || ctx
                .ir
                .callback_typedefs
                .iter()
                .any(|c| haskell_data_name(&c.name) == *word);
        if is_type_name && known {
            out.push_str("T.");
        }
        out.push_str(word);
        word.clear();
    };
    for c in sig.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '\'' {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

fn paren(s: &str) -> String {
    let needs = s.contains(' ') && !(s.starts_with('(') && s.ends_with(')'));
    if needs {
        format!("({})", s)
    } else {
        s.to_string()
    }
}

/// `add_child` / `addChild` -> `AddChild`.
fn pascal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper = true;
    for c in s.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}
