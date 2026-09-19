//! The idiomatic layer user code imports through the `Azul` facade.
//!
//! Everything here is derived from the IR; no class is special-cased. The
//! two method-name rules (`log` reports a failed callback, `with_on_*`
//! gets a model-bound sibling) are the ones every managed binding shares.
//! The layer has four tiers, split into modules that only ever import
//! downwards (see `mod.rs` for the file layout):
//!
//! 1. **`Azul.Internal.Runtime`** — the host-handle table (see
//!    `core/src/host_invoker.rs`), the ownership state of a wrapper, the
//!    binding's `AzulError`, the callback guard, the table of the model of
//!    the running callback, and the `String` <-> `AzString` marshalling.
//!
//! 2. **`Azul.Internal.Handles.<Module>` — managed wrapper types.** Every
//!    struct that owns a resource (has a `_delete`) or has methods becomes
//!    `data C = C { cRaw :: ForeignPtr T.C, cOwnership :: IORef Ownership }`:
//!    a GC-managed, pinned buffer of exactly `sizeOf (undefined :: T.C)`
//!    bytes (the cbits layout oracle) plus who releases it. `withC` lends
//!    the bytes to a C call; `moveC` hands them to a by-value parameter and
//!    marks the wrapper `Moved` (a borrowed callback argument is cloned
//!    instead), so using a moved value again raises `AzulUseAfterMove`
//!    rather than freeing the same memory twice. `disposeC` runs `_delete`
//!    on an `Owned` value. The buffer itself is freed by the GC, never by a
//!    finalizer that calls into libazul — so nothing runs on a foreign
//!    thread.
//!
//! 3. **`Azul.Internal.Callbacks`** — the host-handle `RefAny`
//!    (`refAnyCreate` stores any `Typeable` Haskell value in the table
//!    keyed by a `Word64` handle; libazul calls the registered releaser when
//!    the last clone drops), the `ToRefAny` class by-value `RefAny`
//!    parameters take, and, for every callback kind in
//!    `HOST_INVOKER_KINDS`, one closure type, one handler class (the raw
//!    closure or a function of the typed model) and one invoker registered
//!    with libazul. The invoker runs the user's function under `azulGuard`:
//!    a model of another type or an exception is logged through the kind's
//!    `CallbackInfo.log` (stderr when it has none) and libazul's pre-filled
//!    default result stays.
//!
//! 4. **`Azul.<Module>` — constructors and methods**, one Haskell
//!    function per api.json function of that module's classes:
//!    `<class><Method>`, arguments in api.json order with the receiver
//!    LAST so builder chains read as `>>=` pipelines
//!    (`domCreateBody >>= domWithChild label`). Haskell `String`s marshal
//!    to `AzString`, `Bool` to `bool`, enums and POD structs travel as
//!    `Azul.Types` values, wrapper classes as wrappers. Every
//!    `with_on_<event>(data, callback)` method also gets a model-bound
//!    sibling `<class>On<Event> callback` whose data is the model of the
//!    running callback. `windowCreateOptionsCreate` takes the layout closure
//!    the same way (spliced into the default options at the oracle's
//!    `offsetof`).

use std::collections::{BTreeMap, BTreeSet};

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    ArgRefKind, CallbackTypedefDef, CodegenIR, FunctionArg, FunctionDef, FunctionKind,
    MonomorphizedKind, StructDef, TypeCategory,
};
use super::super::managed_host_invoker;
use super::functions::{ffi_signature, host_invoker_signature};
use super::{haskell_data_name, haskell_field_name, haskell_variant_name, lower_first, sanitize_doc, Split};

// ============================================================================
// Entry points
// ============================================================================

/// `Azul.Internal.Runtime`: the handle table and string marshalling.
pub fn generate_runtime_module(ctx: &Ctx) -> String {
    let mut b = CodeBuilder::new(&ctx.config.indent);
    emit_module_header(
        &mut b,
        "Azul.Internal.Runtime",
        "The host-handle table and the String <-> AzString marshalling every other tier of the idiomatic layer uses. Internal: use \"Azul\".",
        &[],
    );
    emit_prelude(&mut b, ctx);
    b.finish()
}

/// `Azul.Internal.Handles.<Module>`: the managed wrapper types of one
/// api.json module.
pub fn generate_handles_module(ctx: &Ctx, api_module: &str) -> String {
    let mut b = CodeBuilder::new(&ctx.config.indent);
    emit_module_header(
        &mut b,
        &format!("Azul.Internal.Handles.{}", super::module_segment(api_module)),
        &format!(
            "Managed wrapper types for the classes of the api.json module @{}@. Internal: use \"Azul\".",
            api_module
        ),
        &["Azul.Internal.Runtime"],
    );
    for s in &ctx.ir.structs {
        if ctx.wrapped.contains(&s.name) && ctx.split.module_of(&s.name) == api_module {
            emit_wrapper_class(&mut b, s, ctx);
        }
    }
    b.finish()
}

/// `Azul.Internal.Callbacks`: the host-handle `RefAny` and the closure
/// type + invoker of every callback kind.
pub fn generate_callbacks_module(ctx: &Ctx) -> String {
    let mut b = CodeBuilder::new(&ctx.config.indent);
    emit_module_header(
        &mut b,
        "Azul.Internal.Callbacks",
        "The host-handle RefAny and, per callback kind, the closure type and the invoker libazul dispatches through. Internal: use \"Azul\".",
        &["Azul.Internal.Runtime", "Azul.Internal.Handles"],
    );
    emit_managed_refany(&mut b, ctx);
    let mut loggers = BTreeSet::new();
    for cb in &ctx.kinds {
        if let Some((_, f, _)) = mismatch_logger(cb, ctx) {
            if loggers.insert(f.c_name.clone()) {
                emit_logger(&mut b, f, ctx);
            }
        }
    }
    for cb in &ctx.kinds {
        emit_callback_kind(&mut b, cb, ctx);
    }
    emit_ensure_managed(&mut b, ctx);
    b.finish()
}

/// `Azul.<Module>`: constructors and methods of one api.json module's
/// classes.
pub fn generate_api_module(ctx: &Ctx, api_module: &str) -> String {
    let mut b = CodeBuilder::new(&ctx.config.indent);
    emit_module_header(
        &mut b,
        &api_module_name(api_module),
        &format!(
            "Constructors and methods of the classes of the api.json module @{}@ (receiver last, so builder chains are @>>=@ pipelines), under their class-prefixed names. Internal: each class's module (\"Azul.Button\", ...) exports them by short name.",
            api_module
        ),
        &[
            "Azul.Internal.Runtime",
            "Azul.Internal.Handles",
            "Azul.Internal.Callbacks",
        ],
    );
    for s in &ctx.ir.structs {
        if ctx.wrapped.contains(&s.name) && ctx.split.module_of(&s.name) == api_module {
            emit_layout_factory(&mut b, s, ctx);
            emit_class_functions(&mut b, s, ctx);
        }
    }
    b.finish()
}

/// The internal module holding the class functions of an api.json module.
pub fn api_module_name(api_module: &str) -> String {
    format!("Azul.Internal.Api.{}", super::module_segment(api_module))
}

/// `Azul.<Class>`: a class's constructors and methods under their short
/// names (`Azul.Button.create` for `buttonCreate`), for a qualified import:
/// `import qualified Azul.Button as Button` -> `Button.create`. Only
/// aliases: the module imports nothing from the Prelude, so a short name
/// like `div` or `lookup` clashes with nothing.
pub fn generate_class_module(ctx: &Ctx, class: &str, aliases: &[Alias]) -> String {
    let module = class_module_name(class);
    let mut b = CodeBuilder::new(&ctx.config.indent);
    b.line("{- |");
    b.line(&format!("Module      : {module}"));
    b.line(&format!(
        "Description : The constructors and methods of '{}'.",
        haskell_data_name(class)
    ));
    b.line("");
    b.line(&format!(
        "Import it qualified: @import qualified {module} as {}@.",
        haskell_data_name(class)
    ));
    b.line("");
    b.line("Generated by azul-doc codegen v2 (lang_haskell). DO NOT EDIT MANUALLY.");
    b.line("-}");
    b.line("{-# LANGUAGE NoMonomorphismRestriction #-}");
    b.line("{-# OPTIONS_GHC -Wno-missing-signatures #-}");
    b.blank();
    b.line(&format!("module {module}"));
    b.indent();
    for (i, a) in aliases.iter().enumerate() {
        b.line(&format!("{} {}", if i == 0 { "(" } else { "," }, a.short));
    }
    b.line(") where");
    b.dedent();
    b.blank();
    b.line("import Prelude ()");
    b.line(&format!(
        "import qualified {} as I",
        api_module_name(&ctx.split.module_of(class))
    ));
    for a in aliases {
        b.blank();
        for (i, d) in a.doc.iter().enumerate() {
            let prefix = if i == 0 { "-- |" } else { "--" };
            for line in super::sanitize_doc(d).lines() {
                b.line(&format!("{prefix} {line}").trim_end().to_string());
            }
        }
        b.line(&format!("{} = I.{}", a.short, a.full));
    }
    b.finish()
}

/// The per-class module name: `Azul.<Class>`.
pub fn class_module_name(class: &str) -> String {
    format!("Azul.{}", haskell_data_name(class))
}

/// `Azul`: re-exports every tier of the idiomatic layer and the
/// `Azul.Types` entities the layer does not wrap. `bodies` are the
/// sources of the re-exported modules (to keep clashing names out of
/// the `Azul.Types` re-export list).
pub fn generate_facade(ctx: &Ctx, modules: &[String], bodies: &[&str]) -> String {
    let mut b = CodeBuilder::new(&ctx.config.indent);
    b.line("{- |");
    b.line("Module      : Azul");
    b.line("Description : Auto-generated Haskell bindings for the Azul GUI framework.");
    b.line("");
    b.line("The types of the binding: one managed wrapper type per resource-owning");
    b.line("class, the enums and plain structs (re-exported from \"Azul.Types\"),");
    b.line("and the callback types - callbacks are plain functions of your own model");
    b.line("type: @Model -> LayoutCallbackInfo -> IO Dom@ for a layout,");
    b.line("@Model -> CallbackInfo -> (Model, Update)@ for a click handler.");
    b.line("");
    b.line("A class's constructors and methods live in the class's own module, by");
    b.line("short name, receiver last so builder chains are @>>=@ pipelines:");
    b.line("@import qualified Azul.Button as Button@, then @Button.create@,");
    b.line("@Button.withOnClick@. Haskell 'String's and 'Bool's convert at the");
    b.line("boundary.");
    b.line("");
    b.line("Generated by azul-doc codegen v2 (lang_haskell). DO NOT EDIT MANUALLY.");
    b.line("-}");
    b.line("{-# OPTIONS_GHC -Wno-unused-imports #-}");
    b.blank();
    b.line("module Azul");
    b.indent();
    for (i, m) in modules.iter().enumerate() {
        let prefix = if i == 0 { "( " } else { ", " };
        b.line(&format!("{}module {}", prefix, m));
    }
    let all_bodies = bodies.join("\n");
    for item in reexports(ctx, &all_bodies) {
        b.line(&format!(", {}", item));
    }
    b.line(") where");
    b.dedent();
    b.blank();
    for m in modules {
        b.line(&format!("import {}", m));
    }
    b.line("import qualified Azul.Types as T");
    b.finish()
}

/// The pragma + import block every tier shares. `internal` are the
/// binding's own modules to import unqualified, in addition to
/// `Azul.Types` as `T` and `Azul.Internal.FFI` as `FFI`.
fn emit_module_header(b: &mut CodeBuilder, name: &str, what: &str, internal: &[&str]) {
    b.line(&format!("-- | {}", what));
    b.line("--");
    b.line("-- Generated by azul-doc codegen v2 (lang_haskell). DO NOT EDIT MANUALLY.");
    b.line("{-# LANGUAGE ScopedTypeVariables #-}");
    b.line("{-# LANGUAGE FlexibleInstances #-}");
    // `instance Typeable d => ToRefAny d`: the context is not smaller than
    // the head, which Haskell2010 alone rejects.
    b.line("{-# LANGUAGE UndecidableInstances #-}");
    b.line("{-# OPTIONS_GHC -Wno-unused-imports -Wno-unused-matches -Wno-name-shadowing -Wno-unused-local-binds #-}");
    b.blank();
    b.line(&format!("module {} where", name));
    b.blank();
    b.line("import qualified Azul.Types as T");
    b.line("import qualified Azul.Internal.FFI as FFI");
    for m in internal {
        b.line(&format!("import {}", m));
    }
    b.line("import Control.Exception (Exception(..), SomeException, catch, evaluate, finally, throwIO)");
    // Qualified: `ThreadId` is also an api.json class.
    b.line("import qualified Control.Concurrent as Conc");
    b.line("import Control.Monad (unless, when)");
    b.line("import Data.Dynamic (Dynamic, Typeable, dynTypeRep, fromDynamic, toDyn)");
    b.line("import Data.Typeable (Proxy(..), typeRep)");
    b.line("import Data.IORef (IORef, atomicModifyIORef', newIORef, readIORef, writeIORef)");
    b.line("import qualified Data.Map.Strict as Map");
    b.line("import Data.Int (Int8, Int16, Int32, Int64)");
    b.line("import Data.Word (Word8, Word16, Word32, Word64)");
    b.line("import Foreign.C.Types");
    b.line("import Foreign.ForeignPtr (ForeignPtr, mallocForeignPtr, newForeignPtr_, withForeignPtr)");
    b.line("import Foreign.Marshal.Alloc (alloca, allocaBytesAligned)");
    b.line("import Foreign.Marshal.Array (withArrayLen)");
    b.line("import Foreign.Marshal.Utils (copyBytes, fromBool, toBool)");
    b.line("import Foreign.Ptr (Ptr, FunPtr, castPtr, plusPtr)");
    b.line("import Foreign.Storable (Storable(..))");
    b.line("import System.IO (hPutStrLn, stderr)");
    b.line("import System.IO.Unsafe (unsafePerformIO)");
    b.blank();
}

// ============================================================================
// Context
// ============================================================================

pub struct Ctx<'a> {
    ir: &'a CodegenIR,
    config: &'a CodegenConfig,
    split: &'a Split,
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
    /// Every class function emitted so far, by class: what the per-class
    /// module (`Azul.Button` -> `create`, `withOnClick`, ...) re-exports
    /// under its short name. Filled while the API modules are emitted.
    pub aliases: std::cell::RefCell<BTreeMap<String, Vec<Alias>>>,
}

/// One class function under its short name in the per-class module.
#[derive(Clone, Debug)]
pub struct Alias {
    /// The name the internal API module defines (`buttonWithOnClick`).
    pub full: String,
    /// The name the class module gives it (`withOnClick`).
    pub short: String,
    /// Its documentation.
    pub doc: Vec<String>,
}

impl<'a> Ctx<'a> {
    /// Record that `full`, a function of `class`, is `short` in the class's
    /// own module.
    fn record_alias(&self, class: &str, full: &str, short: &str, doc: &[String]) {
        self.aliases
            .borrow_mut()
            .entry(class.to_string())
            .or_default()
            .push(Alias {
                full: full.to_string(),
                short: super::sanitize_value_identifier(short),
                doc: doc.to_vec(),
            });
    }

    pub fn new(ir: &'a CodegenIR, config: &'a CodegenConfig, split: &'a Split) -> Self {
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
            split,
            wrapped,
            deletable,
            kinds,
            string_from_bytes,
            string_delete,
            refany_clone,
            aliases: Default::default(),
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
// Facade exports
// ============================================================================

/// The `Azul.Types` entities re-exported through `Azul`: every type this
/// layer does not wrap (enums, POD structs, aliases, callback typedefs)
/// plus the string helpers — minus anything whose name would clash with a
/// declaration of the layer (GHC rejects conflicting exports).
fn reexports(ctx: &Ctx, body_src: &str) -> Vec<String> {
    let mut local: BTreeSet<String> = super::module_scope_declarations(body_src)
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    for name in &ctx.wrapped {
        let w = haskell_data_name(name);
        local.insert(format!("{}Raw", lower_first(&w)));
        local.insert(format!("{}Ownership", lower_first(&w)));
    }
    let clashes = |names: &[String]| names.iter().any(|n| local.contains(n));
    let mut out = Vec::new();
    for s in &ctx.ir.structs {
        if !super::types::should_emit_struct(s, ctx.config) || ctx.wrapped.contains(&s.name) {
            continue;
        }
        let hs = haskell_data_name(&s.name);
        let mut names = vec![hs.clone()];
        names.extend(s.fields.iter().map(|f| haskell_field_name(&s.name, &f.name)));
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
        names.extend(e.variants.iter().map(|v| haskell_variant_name(&e.name, &v.name)));
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
                names.extend(variants.iter().map(|v| haskell_variant_name(&ta.name, &v.name)));
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
// ============================================================================
// Prelude: handle table, string marshalling
// ============================================================================

fn emit_prelude(b: &mut CodeBuilder, ctx: &Ctx) {
    b.raw(RUNTIME_PRELUDE);
    b.blank();
    b.line("-- ---------------------------------------------------------------------------");
    b.line("-- String marshalling (Haskell String <-> AzString, UTF-8).");
    b.line("-- ---------------------------------------------------------------------------");
    b.blank();
    if let (Some(from_bytes), Some(delete)) = (&ctx.string_from_bytes, &ctx.string_delete) {
        b.line("-- | Pass a Haskell String as an AzString the callee takes ownership of.");
        b.line("withAzStringArg :: String -> (Ptr T.AzString -> IO a) -> IO a");
        b.line("withAzStringArg s k = withArrayLen (T.encodeUtf8 s) $ \\n bytes -> alloca $ \\p -> do");
        b.indent();
        b.line(&format!("FFI.c_{}_via bytes 0 (fromIntegral n) p", from_bytes));
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

/// The static part of `Azul.Internal.Runtime`: the host-handle table, the
/// ownership state every wrapper carries, the binding's exception type, the
/// callback guard and the "model of the running callback" table.
///
/// - **Ownership.** A wrapper is `Owned` (the Haskell side must release it),
///   `Borrowed` (a callback argument: libazul owns it, valid until the
///   callback returns) or `Moved` (a by-value C parameter took its bytes).
///   Haskell values look immutable, so passing one wrapper to two by-value
///   parameters is an easy mistake; `azulWith` / `azulMove` turn that
///   use-after-move into an 'AzulError' instead of a double free.
/// - **The guard.** An exception that unwinds into a `foreign import
///   "wrapper"` frame ends the program (the RTS reports it and exits), so
///   every invoker runs its whole body under `azulGuard`, which reports the
///   exception and returns normally. libazul pre-fills the out slot with the
///   kind's default, so a failed callback leaves `Update_DoNothing` / an
///   empty `Dom` behind.
/// - **The current model.** Every callback call-in runs in a fresh Haskell
///   thread, so the invoker records the `RefAny` it was called with under
///   `myThreadId`; `buttonOnClick`-style setters bind that model.
const RUNTIME_PRELUDE: &str = r#"-- ---------------------------------------------------------------------------
-- Host-handle table (see core/src/host_invoker.rs for the protocol).
-- ---------------------------------------------------------------------------

{-# NOINLINE azulHandleTable #-}
azulHandleTable :: IORef (Map.Map Word64 Dynamic)
azulHandleTable = unsafePerformIO (newIORef Map.empty)

{-# NOINLINE azulNextHandle #-}
azulNextHandle :: IORef Word64
azulNextHandle = unsafePerformIO (newIORef 1)

{-# NOINLINE azulManagedInstalled #-}
azulManagedInstalled :: IORef Bool
azulManagedInstalled = unsafePerformIO (newIORef False)

azulAllocHandle :: Dynamic -> IO Word64
azulAllocHandle v = do
    h <- atomicModifyIORef' azulNextHandle (\n -> (n + 1, n))
    atomicModifyIORef' azulHandleTable (\m -> (Map.insert h v m, ()))
    pure h

azulLookupHandle :: Word64 -> IO (Maybe Dynamic)
azulLookupHandle h = Map.lookup h <$> readIORef azulHandleTable

-- | Called by libazul (through the registered releaser) when the last
-- clone of a host-handle RefAny is dropped.
azulReleaseHandle :: Word64 -> IO ()
azulReleaseHandle h =
    atomicModifyIORef' azulHandleTable (\m -> (Map.delete h m, ()))
        `catch` \(_ :: SomeException) -> pure ()

-- ---------------------------------------------------------------------------
-- Ownership and errors.
-- ---------------------------------------------------------------------------

-- | Who releases the bytes behind a wrapper: the Haskell side ('Owned'),
-- libazul ('Borrowed', a callback argument valid until the callback
-- returns), or nobody any more ('Moved', a by-value parameter took them).
data Ownership = Owned | Borrowed | Moved
    deriving (Eq, Show)

-- | The errors the binding raises. Inside a callback they are caught and
-- logged like any other exception.
data AzulError
    = AzulUseAfterMove String
    | AzulBorrowedMove String
    | AzulModelMismatch String String
    | AzulNoCallbackModel String

instance Show AzulError where
    show (AzulUseAfterMove c) = "this " ++ c ++ " was already passed by value and cannot be used again"
    show (AzulBorrowedMove c) = "this " ++ c ++ " belongs to libazul (a callback argument) and cannot be passed by value"
    show (AzulModelMismatch expected got) = "expected a model of type " ++ expected ++ ", got " ++ got
    show (AzulNoCallbackModel f) = f ++ " binds the model of the running callback and must be called inside a callback"

instance Exception AzulError

-- | Borrow the bytes of a wrapper for one C call.
azulWith :: String -> IORef Ownership -> ForeignPtr t -> (Ptr t -> IO a) -> IO a
azulWith cls st fp k = do
    s <- readIORef st
    when (s == Moved) (throwIO (AzulUseAfterMove cls))
    withForeignPtr fp k

-- | Hand the bytes of a wrapper to a by-value C parameter. An owned value
-- is moved; a borrowed one is cloned first when the class has a clone.
azulMove :: String -> Maybe (Ptr t -> Ptr t -> IO ()) -> Int -> Int -> IORef Ownership -> ForeignPtr t -> (Ptr t -> IO a) -> IO a
azulMove cls clone size align st fp k = do
    s <- readIORef st
    case s of
        Owned -> writeIORef st Moved >> withForeignPtr fp k
        Moved -> throwIO (AzulUseAfterMove cls)
        Borrowed -> case clone of
            Nothing -> throwIO (AzulBorrowedMove cls)
            Just c -> withForeignPtr fp $ \src -> allocaBytesAligned size align $ \tmp -> do
                c src tmp
                k tmp

-- ---------------------------------------------------------------------------
-- Callback guard.
-- ---------------------------------------------------------------------------

azulStderr :: String -> IO ()
azulStderr = hPutStrLn stderr

-- | Run the body of a callback so that no exception reaches libazul: report
-- it through @report@ (stderr when that fails too) and return normally.
azulGuard :: String -> (String -> IO ()) -> IO () -> IO ()
azulGuard kind report body = body `catch` \(e :: SomeException) -> do
    let msg = case fromException e of
            Just err@(AzulModelMismatch _ _) -> "azul: " ++ kind ++ " " ++ show err
            _ -> "azul: " ++ kind ++ " raised " ++ show e
    (report msg `catch` \(_ :: SomeException) -> azulStderr msg)
        `catch` \(_ :: SomeException) -> pure ()

-- ---------------------------------------------------------------------------
-- The model of the running callback, per Haskell thread.
-- ---------------------------------------------------------------------------

{-# NOINLINE azulCurrentTable #-}
azulCurrentTable :: IORef (Map.Map Conc.ThreadId (Ptr T.RefAny))
azulCurrentTable = unsafePerformIO (newIORef Map.empty)

-- | Run @act@ with @p@ as the model of the running callback.
azulWithCurrentData :: Ptr T.RefAny -> IO a -> IO a
azulWithCurrentData p act = do
    tid <- Conc.myThreadId
    prev <- atomicModifyIORef' azulCurrentTable (\m -> (Map.insert tid p m, Map.lookup tid m))
    act `finally` atomicModifyIORef' azulCurrentTable
        (\m -> (maybe (Map.delete tid m) (\q -> Map.insert tid q m) prev, ()))

-- | The RefAny the running callback was invoked with, if any.
azulCurrentData :: IO (Maybe (Ptr T.RefAny))
azulCurrentData = do
    tid <- Conc.myThreadId
    Map.lookup tid <$> readIORef azulCurrentTable
"#;

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
        "data {w} = {w} {{ {l}Raw :: !(ForeignPtr {t}), {l}Ownership :: !(IORef Ownership) }}",
        w = w,
        l = l,
        t = t
    ));
    b.blank();
    b.line(&format!("-- | A fresh, uninitialised '{}' buffer (the C side fills it).", w));
    b.line(&format!("alloc{} :: IO {}", w, w));
    b.line(&format!("alloc{} = do", w));
    b.indent();
    b.line("fp <- mallocForeignPtr");
    b.line("st <- newIORef Owned");
    b.line(&format!("pure ({} fp st)", w));
    b.dedent();
    b.blank();
    b.line(&format!(
        "-- | View memory libazul owns (a callback argument) as a '{}'; never released here.",
        w
    ));
    b.line(&format!("borrow{} :: Ptr {} -> IO {}", w, t, w));
    b.line(&format!("borrow{} p = do", w));
    b.indent();
    b.line("fp <- newForeignPtr_ p");
    b.line("st <- newIORef Borrowed");
    b.line(&format!("pure ({} fp st)", w));
    b.dedent();
    b.blank();
    b.line(&format!("-- | Lend the bytes of a '{}' to a C call that borrows them.", w));
    b.line(&format!("with{} :: {} -> (Ptr {} -> IO a) -> IO a", w, w, t));
    b.line(&format!(
        "with{} h = azulWith \"{}\" ({}Ownership h) ({}Raw h)",
        w, w, l, l
    ));
    b.blank();
    b.line(&format!(
        "-- | Hand the bytes of a '{}' to a C call that takes them by value.",
        w
    ));
    b.line(&format!("move{} :: {} -> (Ptr {} -> IO a) -> IO a", w, w, t));
    b.line(&format!(
        "move{} h = azulMove \"{}\" {} (sizeOf (undefined :: {})) (alignment (undefined :: {})) ({}Ownership h) ({}Raw h)",
        w,
        w,
        clone_binding(s, ctx)
            .map(|c| format!("(Just FFI.{})", c))
            .unwrap_or_else(|| "Nothing".to_string()),
        t,
        t,
        l,
        l
    ));
    b.blank();
    b.line(&format!("-- | Mark a '{}' as moved into libazul.", w));
    b.line(&format!("consume{} :: {} -> IO ()", w, w));
    b.line(&format!("consume{} h = writeIORef ({}Ownership h) Moved", w, l));
    b.blank();
    b.line(&format!("-- | Release a '{}' now, unless it was moved or is borrowed.", w));
    b.line(&format!("dispose{} :: {} -> IO ()", w, w));
    b.line(&format!("dispose{} h = do", w));
    b.indent();
    b.line(&format!("st <- readIORef ({}Ownership h)", l));
    b.line("when (st == Owned) $ do");
    b.indent();
    if ctx.deletable.contains(&s.name) {
        b.line(&format!(
            "withForeignPtr ({}Raw h) FFI.c_Az{}_delete",
            l, s.name
        ));
    }
    b.line(&format!("consume{} h", w));
    b.dedent();
    b.dedent();
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

/// The FFI binding of `s`'s deep copy when it has the `(src, out)` shape
/// `move<X>` needs to pass a borrowed value by value.
fn clone_binding(s: &StructDef, ctx: &Ctx) -> Option<String> {
    let func = ctx
        .ir
        .functions_for_class(&s.name)
        .find(|f| f.kind == FunctionKind::DeepCopy)?;
    if !super::functions::should_emit_function(func, ctx.ir, ctx.config) {
        return None;
    }
    let sig = ffi_signature(func, ctx.ir);
    (sig.shimmed && sig.out_type.is_some() && sig.arg_types.len() == 1).then_some(sig.binding)
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
        "a == b = unsafePerformIO $ with{} a $ \\pa -> with{} b $ \\pb -> toBool <$> FFI.c_{} pa pb",
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
    b.raw(MANAGED_REFANY);
    b.blank();
    if let Some(clone) = &ctx.refany_clone {
        b.raw(&MANAGED_REFANY_CLONE.replace("{clone}", &format!("FFI.c_{}_via", clone)));
        b.blank();
    }
}

/// `refAnyCreate` / `refAnyGet` / `refAnyModify` and the model plumbing the
/// typed callback forms share (`azulModel`, `azulTransition`).
const MANAGED_REFANY: &str = r#"-- ---------------------------------------------------------------------------
-- RefAny: type-erased application data, a host-handle into the table above.
-- ---------------------------------------------------------------------------

-- | Wrap any Haskell value as a libazul 'RefAny'. Clones share the value;
-- the table entry is released when the last clone is dropped.
refAnyCreate :: Typeable a => a -> IO RefAny
refAnyCreate v = do
    azulEnsureManaged
    h <- azulAllocHandle (toDyn v)
    r <- allocRefAny
    withRefAny r (FFI.c_AzRefAny_newHostHandle_via h)
    pure r

azulRefAnyHandle :: RefAny -> IO Word64
azulRefAnyHandle r = withRefAny r FFI.c_AzRefAny_getHostHandle

-- | The value a 'RefAny' (or any clone of it) was created from, if it is
-- one created by 'refAnyCreate' with a value of this type.
refAnyGet :: Typeable a => RefAny -> IO (Maybe a)
refAnyGet r = do
    h <- azulRefAnyHandle r
    entry <- azulLookupHandle h
    pure (entry >>= fromDynamic)

-- | Replace the value behind a 'RefAny' with @f@ applied to it; a no-op
-- when the stored value is not of this type.
refAnyModify :: Typeable a => RefAny -> (a -> a) -> IO ()
refAnyModify r f = do
    h <- azulRefAnyHandle r
    entry <- azulLookupHandle h
    case entry >>= fromDynamic of
        Nothing -> pure ()
        Just v -> do
            v' <- evaluate (f v)
            atomicModifyIORef' azulHandleTable (\m -> (Map.insert h (toDyn v') m, ()))

-- | The model behind a callback's 'RefAny'; 'AzulModelMismatch' when it
-- holds a value of another type.
azulModel :: forall a. Typeable a => RefAny -> IO a
azulModel r = do
    h <- azulRefAnyHandle r
    entry <- azulLookupHandle h
    case entry of
        Just d | Just v <- fromDynamic d -> pure v
        _ -> throwIO (AzulModelMismatch (show (typeRep (Proxy :: Proxy a)))
                (maybe "a RefAny that refAnyCreate did not create" (show . dynTypeRep) entry))

-- | Run a state transition on the model behind a 'RefAny': store the new
-- model and answer with the verdict. The new model is evaluated before it
-- is stored, so an exception in @step@ leaves the old model in place.
azulTransition :: Typeable a => RefAny -> (a -> IO (a, r)) -> IO r
azulTransition r step = do
    m <- azulModel r
    (m', verdict) <- step m >>= evaluate
    m'' <- evaluate m'
    h <- azulRefAnyHandle r
    atomicModifyIORef' azulHandleTable (\t -> (Map.insert h (toDyn m'') t, ()))
    pure verdict

-- | The closure a host handle was registered with.
azulLookupClosure :: forall f. Typeable f => String -> Word64 -> IO f
azulLookupClosure kind h = do
    entry <- azulLookupHandle h
    case entry >>= fromDynamic of
        Just f -> pure f
        Nothing -> ioError (userError ("no " ++ kind ++ " registered under host handle " ++ show h))"#;

/// The by-value `RefAny` plumbing: `withRefAnyClone`, the `ToRefAny` class
/// every unpaired by-value `RefAny` parameter takes, and the clone of the
/// running callback's model. `{clone}` is the `RefAny` deep-copy import.
const MANAGED_REFANY_CLONE: &str = r#"-- | Hand a clone of a 'RefAny' to a by-value C parameter (the callee
-- owns the clone; the caller keeps its own).
withRefAnyClone :: RefAny -> (Ptr T.RefAny -> IO a) -> IO a
withRefAnyClone r k = withRefAny r $ \src -> alloca $ \dst -> do
    {clone} src dst
    k dst

-- | What a by-value 'RefAny' parameter such as 'appCreate''s accepts: a
-- 'RefAny' (the callee gets a clone that shares the value) or any other
-- Haskell value, wrapped in a new 'RefAny'.
class ToRefAny d where
    withRefAnyArg :: d -> (Ptr T.RefAny -> IO a) -> IO a

instance {-# OVERLAPPING #-} ToRefAny RefAny where
    withRefAnyArg = withRefAnyClone

instance {-# OVERLAPPABLE #-} Typeable d => ToRefAny d where
    withRefAnyArg v k = refAnyCreate v >>= \r -> moveRefAny r k

-- | A clone of the 'RefAny' the running callback was invoked with, for the
-- setters that bind a callback to the current model (`buttonOnClick`).
azulCurrentRefAnyClone :: String -> (Ptr T.RefAny -> IO a) -> IO a
azulCurrentRefAnyClone who k = do
    cur <- azulCurrentData
    case cur of
        Nothing -> throwIO (AzulNoCallbackModel who)
        Just src -> alloca $ \dst -> do
            {clone} src dst
            k dst"#;

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

/// Does the kind's first argument carry the model (`RefAny`)? Only those
/// kinds get the model-typed handler forms and a current model.
fn model_first(cb: &CallbackTypedefDef, ctx: &Ctx) -> bool {
    ctx.wrapped.contains("RefAny")
        && cb
            .args
            .first()
            .map(|a| a.type_name.trim() == "RefAny")
            .unwrap_or(false)
}

/// Where a failed callback of this kind is reported: the first argument
/// whose class has a `log(level, message: String)` method (`CallbackInfo`),
/// as `(argument index, the log function, the Haskell error level)`.
/// Kinds without one report on stderr.
fn mismatch_logger<'b>(
    cb: &CallbackTypedefDef,
    ctx: &Ctx<'b>,
) -> Option<(usize, &'b FunctionDef, String)> {
    for (i, a) in cb.args.iter().enumerate() {
        let t = a.type_name.trim();
        if !matches!(cb_arg(a, ctx), Some(CbArg::Borrow(_))) {
            continue;
        }
        let ir: &'b CodegenIR = ctx.ir;
        let Some(f) = ir.functions.iter().find(|f| {
            f.class_name == t
                && f.method_name == "log"
                && matches!(f.kind, FunctionKind::Method | FunctionKind::MethodMut)
                && f.args.len() == 3
                && f.args[2].type_name.trim() == "String"
        }) else {
            continue;
        };
        if !super::functions::should_emit_function(f, ctx.ir, ctx.config)
            || !f.args.iter().all(|a| arg_plan(a, ctx).is_some())
            || ret_plan(f, ctx).is_none()
        {
            continue;
        }
        let level_ty = f.args[1].type_name.trim();
        let Some(e) = ctx.ir.find_enum(level_ty) else {
            continue;
        };
        if e.is_union || e.variants.is_empty() {
            continue;
        }
        let variant = e
            .variants
            .iter()
            .find(|v| v.name == "Error")
            .unwrap_or(&e.variants[0]);
        return Some((i, f, format!("T.{}", haskell_variant_name(level_ty, &variant.name))));
    }
    None
}

fn logger_name(f: &FunctionDef) -> String {
    format!("azulLog{}", haskell_data_name(&f.class_name))
}

/// `azulLog<Class> :: <level> -> String -> <Class> -> IO ()`: the class's
/// `log` method, for the invokers (the public copy lives in a module above
/// this one).
fn emit_logger(b: &mut CodeBuilder, f: &FunctionDef, ctx: &Ctx) {
    let plans: Option<Vec<ArgPlan>> = f.args.iter().map(|a| arg_plan(a, ctx)).collect();
    let (Some(plans), Some(ret)) = (plans, ret_plan(f, ctx)) else {
        return;
    };
    emit_function(b, f, &logger_name(f), true, &plans, &ret, ctx);
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

    // The invoker libazul calls: look the closure up, run it under the
    // guard (no exception may unwind into libazul) with the RefAny as the
    // current model, and write the result into the pre-filled out slot.
    let mut params: Vec<String> = vec!["handle".to_string()];
    for i in 0..cb.args.len() {
        params.push(format!("p{}", i));
    }
    let has_out = !matches!(ret, CbRet::Void);
    if has_out {
        params.push("out".to_string());
    }
    let report = match mismatch_logger(cb, ctx) {
        Some((i, f, level)) => format!(
            "(\\msg -> borrow{} p{} >>= {} {} msg)",
            haskell_data_name(cb.args[i].type_name.trim()),
            i,
            logger_name(f),
            level
        ),
        None => "azulStderr".to_string(),
    };
    b.line(&format!("azulInvoke{} :: {}", kind, sig));
    b.line(&format!(
        "azulInvoke{} {} = azulGuard \"{}\" {} $ do",
        kind,
        params.join(" "),
        kind,
        report
    ));
    b.indent();
    b.line(&format!("f <- azulLookupClosure \"{}\" handle", kind));
    let current = model_first(cb, ctx);
    if current {
        b.line("azulWithCurrentData p0 $ do");
        b.indent();
    }
    let mut call = format!("(f :: {}Fn)", wrapper_hs);
    for (i, a) in cb.args.iter().enumerate() {
        match cb_arg(a, ctx).unwrap() {
            CbArg::Borrow(w) => b.line(&format!("v{} <- borrow{} p{}", i, w, i)),
            CbArg::Peek(_) => b.line(&format!("v{} <- peek p{}", i, i)),
        }
        call.push_str(&format!(" v{}", i));
    }
    match &ret {
        CbRet::Void => b.line(&call),
        CbRet::Poke(_) => {
            b.line(&format!("r <- {}", call));
            b.line("poke out r");
        }
        CbRet::Wrapper(w) => {
            b.line(&format!("r <- {}", call));
            b.line(&format!(
                "move{} r $ \\src -> copyBytes out src (sizeOf (undefined :: T.{}))",
                w, w
            ));
        }
    }
    if current {
        b.dedent();
    }
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
/// per shape a user may hand to a setter of this kind. For
/// `ButtonOnClickCallback` (`RefAny -> CallbackInfo -> IO Update`):
///
/// - the raw closure over the `RefAny`, as is;
/// - `model -> CallbackInfo -> IO Update`: the binding downcasts the
///   `RefAny` to the model type first (every kind; the only typed form of
///   kinds that return a `Dom` or nothing);
/// - `model -> CallbackInfo -> (model, Update)` and its `IO` twin, for kinds
///   that return a plain value: a state transition. The binding reads the
///   model, applies the function and stores the new model.
///
/// When the `RefAny` holds another type, the typed forms raise
/// `AzulModelMismatch` before the user's function runs; the invoker's guard
/// reports `expected a model of type X, got Y` and libazul keeps the
/// kind's default result.
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
    let typed = model_first(cb, ctx);
    b.line(&format!("-- | Every shape 'azulRegister{}' accepts: the raw closure, or a", kind));
    b.line("-- function of the model behind the RefAny (see the instances).");
    b.line(&format!("class {} h where", class));
    b.indent();
    b.line(&format!("{} :: h -> {}Fn", method, wrapper_hs));
    b.dedent();
    b.blank();
    let overlapping = if typed { "{-# OVERLAPPING #-} " } else { "" };
    b.line(&format!("instance {}{} ({}) where", overlapping, class, closure));
    b.indent();
    b.line(&format!("{} = id", method));
    b.dedent();
    b.blank();
    if !typed {
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
    let rest_sig: String = rest.iter().map(|t| format!("{} -> ", paren(t))).collect();
    let rest_call: String = (1..cb.args.len()).map(|i| format!(" a{}", i)).collect();
    let ret = match cb_ret(cb, ctx) {
        Some(CbRet::Void) | None => "()".to_string(),
        Some(CbRet::Wrapper(w)) => w,
        Some(CbRet::Poke(t)) => t,
    };

    b.line(&format!(
        "instance {{-# OVERLAPPABLE #-}} Typeable a => {} (a -> {}IO {}) where",
        class,
        rest_sig,
        paren(&ret)
    ));
    b.indent();
    b.line(&format!(
        "{} f dat{} = azulModel dat >>= \\m -> f m{}",
        method, rest_call, rest_call
    ));
    b.dedent();
    b.blank();
    if !matches!(cb_ret(cb, ctx), Some(CbRet::Poke(_))) {
        return;
    }
    b.line(&format!(
        "instance Typeable a => {} (a -> {}(a, {})) where",
        class, rest_sig, ret
    ));
    b.indent();
    b.line(&format!(
        "{} f dat{} = azulTransition dat (\\m -> pure (f m{}))",
        method, rest_call, rest_call
    ));
    b.dedent();
    b.blank();
    b.line(&format!(
        "instance Typeable a => {} (a -> {}IO (a, {})) where",
        class, rest_sig, ret
    ));
    b.indent();
    b.line(&format!(
        "{} f dat{} = azulTransition dat (\\m -> f m{})",
        method, rest_call, rest_call
    ));
    b.dedent();
    b.blank();
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
        b.line(&format!("inv{} <- FFI.mk_{}Invoker azulInvoke{}", kind, kind, kind));
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
    ctx.record_alias(
        &s.name,
        &format!("{}Create", l),
        "create",
        &[format!("A '{w}' whose layout is the given function of your model.")],
    );
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
#[derive(Clone)]
enum ArgPlan {
    /// Passed as is (C primitive or raw pointer); `bool` converts.
    Direct { hs: String, is_bool: bool },
    /// Haskell `String` -> AzString the callee owns / borrows.
    StringOwned,
    StringRef,
    /// A by-value `RefAny`: anything `ToRefAny` accepts (a `RefAny`, whose
    /// clone is passed, or any Haskell value, wrapped in a new `RefAny`).
    RefAnyArg,
    /// A by-value `RefAny` that is the data of the callback argument right
    /// after it: a `RefAny` only, so a model value cannot silently become a
    /// second, unshared copy of the app's state.
    RefAnyClone,
    /// The data `RefAny` of a callback, bound to the model of the running
    /// callback (the smart setters such as `buttonOnClick`); no parameter.
    CurrentModel,
    /// A wrapper class: moved (consumed) or borrowed.
    Wrapper { hs: String, consume: bool },
    /// A `Azul.Types` value: `alloca` + `poke`, pointer passed.
    Value { hs: String },
    /// A host-invoker callback: a closure.
    Closure { kind: String, hs: String },
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
    if managed_host_invoker::is_callback_wrapper(ctx.ir, t) {
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
            ArgPlan::RefAnyArg
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
        // A by-value RefAny right before a callback is that callback's data.
        for i in 1..plans.len() {
            if matches!(plans[i - 1], ArgPlan::RefAnyArg)
                && matches!(plans[i], ArgPlan::Closure { .. })
            {
                plans[i - 1] = ArgPlan::RefAnyClone;
            }
        }
        seen.insert(name.clone(), func.c_name.clone());
        emit_function(b, func, &name, receiver, &plans, &ret, ctx);
        let class_prefix = lower_first(&haskell_data_name(&s.name));
        if let Some(short) = name.strip_prefix(&class_prefix) {
            ctx.record_alias(&s.name, &name, &lower_first(short), &func.doc);
        }

        // `with_on_click(data, callback)` -> `buttonOnClick callback`: the
        // same call with the data bound to the model of the running callback
        // (the shared smart-setter rule every managed binding follows).
        let Some((smart, _)) = managed_host_invoker::smart_callback_setter_info(func) else {
            continue;
        };
        if !receiver || !matches!(plans.get(1), Some(ArgPlan::RefAnyClone)) {
            continue;
        }
        let smart_name = format!("{}{}", lower_first(&haskell_data_name(&s.name)), pascal(&smart));
        if let Some(prev) = seen.get(&smart_name) {
            b.line(&format!(
                "-- SKIPPED: {} (model-bound {}) would repeat {} ({})",
                smart_name, func.c_name, smart_name, prev
            ));
            b.blank();
            continue;
        }
        let mut smart_plans = plans.clone();
        smart_plans[1] = ArgPlan::CurrentModel;
        seen.insert(smart_name.clone(), func.c_name.clone());
        b.line(&format!(
            "-- | '{}' with the data bound to the model of the running callback.",
            name
        ));
        emit_function(b, func, &smart_name, receiver, &smart_plans, &ret, ctx);
        ctx.record_alias(&s.name, &smart_name, &lower_first(&pascal(&smart)), &func.doc);
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
    // The bound model has no Haskell parameter.
    order.retain(|&i| !matches!(plans[i], ArgPlan::CurrentModel));

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
            ArgPlan::RefAnyArg => format!("d{}", i),
            ArgPlan::RefAnyClone | ArgPlan::CurrentModel => "RefAny".to_string(),
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
            ArgPlan::RefAnyArg => Some(format!("ToRefAny d{}", i)),
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
    b.line(&format!("{} :: {}{}", name, context, sig_parts.join(" -> ")));
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
            ArgPlan::RefAnyArg => {
                openers.push((format!("withRefAnyArg {} $ \\{} -> do", p, ptr), vec![]));
                call_args.push(ptr);
            }
            ArgPlan::RefAnyClone => {
                openers.push((format!("withRefAnyClone {} $ \\{} -> do", p, ptr), vec![]));
                call_args.push(ptr);
            }
            ArgPlan::CurrentModel => {
                openers.push((
                    format!("azulCurrentRefAnyClone \"{}\" $ \\{} -> do", name, ptr),
                    vec![],
                ));
                call_args.push(ptr);
            }
            ArgPlan::Wrapper { hs, consume } => {
                let bracket = if *consume { "move" } else { "with" };
                openers.push((format!("{}{} {} $ \\{} -> do", bracket, hs, p, ptr), vec![]));
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

    let call = format!("FFI.{} {}", sig.binding, call_args.join(" ")).trim_end().to_string();
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
        let is_type_name = word.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false);
        let known = ctx.ir.structs.iter().any(|s| haskell_data_name(&s.name) == *word)
            || ctx.ir.enums.iter().any(|e| haskell_data_name(&e.name) == *word)
            || ctx.ir.type_aliases.iter().any(|a| haskell_data_name(&a.name) == *word)
            || ctx.ir.callback_typedefs.iter().any(|c| haskell_data_name(&c.name) == *word);
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
