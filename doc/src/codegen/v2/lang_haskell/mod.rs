//! Haskell binding generator.
//!
//! Produces a small library of `.hs` modules plus a Cabal manifest:
//!
//! 1. `src/Azul.hs` — umbrella module re-exporting the curated public
//!    surface (idiomatic newtype wrappers, `withFoo` bracket
//!    constructors, `MyDataModel -> Dom` style layout entry points).
//!    Carries the architecture-alignment doc block at the top.
//! 2. `src/Azul/Internal/FFI.hs` — raw `foreign import ccall` value
//!    declarations that link directly to the C ABI symbols. `unsafe`
//!    is used for non-callback entry points; `safe` is reserved for
//!    any function that may re-enter Haskell (callback-invoking).
//! 3. `src/Azul/Types.hs` — Haskell data declarations that mirror the
//!    C structs and enums, with manually-written `Storable` instances
//!    (offsets emitted literally from the IR). Phantom-typed
//!    `RefAny a` lives here.
//! 4. `azul.cabal` — Cabal manifest declaring the library + deps.
//!
//! ## Why Haskell as a target language matters
//!
//! Azul's architecture (see `doc/guide/architecture.md` lines 107–230)
//! is explicitly a *functional* GUI model — `UI = f(data)` in the Elm
//! tradition — but with one critical refinement: `RefAny` lets the
//! State Graph be decoupled from the Visual Tree, so prop-drilling
//! (the second-generation hierarchy constraint) doesn't apply. Haskell
//! is the most natural target language for this paradigm:
//!
//! - Layout callbacks are expressible as `MyDataModel -> Dom`
//!   (effectively pure) with the `IO` effect happening at the FFI
//!   boundary inside the callback trampoline.
//! - Update callbacks are expressible as
//!   `Event -> MyDataModel -> (MyDataModel, Update)` — Elm's
//!   `update` story brought into Haskell. Imperative side-effects
//!   (RefAny mutation) are hidden inside the trampoline.
//! - `bracket` / `finally` from `Control.Exception` give us RAII over
//!   FFI handles without forcing users into manual `delete`-call
//!   discipline.
//! - `RefAny` is a phantom-typed `newtype RefAny a` so downcasts are
//!   statically tracked.
//!
//! ## Output protocol
//!
//! `generate(ir, config)` returns a single `String` with multiple
//! files separated by [`FILE_MARKER`] / [`END_MARKER`] header lines:
//!
//! ```text
//! -- ==FILE: src/Azul.hs ==
//! <Azul.hs contents>
//! -- ==FILE: src/Azul/Internal/FFI.hs ==
//! <FFI.hs contents>
//! -- ==FILE: src/Azul/Types.hs ==
//! <Types.hs contents>
//! -- ==FILE: azul.cabal ==
//! <cabal contents>
//! ```
//!
//! The marker is itself a syntactically valid Haskell line comment
//! (it starts with `--`), so even if a downstream tool fails to
//! split the file the combined text still parses as a single Haskell
//! source. The Cabal manifest section uses `--` line comments as well
//! (Cabal accepts them). The orchestrator splits on the marker and
//! writes each chunk to its respective relative path.

pub mod cabal;
pub mod functions;
pub mod types;
pub mod wrappers;

use anyhow::Result;

use super::config::CodegenConfig;
use super::generator::CodeBuilder;
use super::ir::CodegenIR;

/// File-marker header that introduces each per-file section in the
/// concatenated output. The orchestrator splits on lines that start
/// with this prefix.
pub const FILE_MARKER: &str = "-- ==FILE: ";

/// Trailing marker that closes the file-marker header line.
pub const END_MARKER: &str = " ==";

/// Library name used in the Cabal manifest and in the Hackage display.
pub const LIB_NAME: &str = "azul";

/// Public entry point. Generates the full multi-file Haskell binding
/// concatenated into a single `String` with file markers between
/// chunks.
pub fn generate(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let umbrella = generate_umbrella(ir, config)?;
    let ffi = generate_ffi(ir, config)?;
    let types_src = generate_types_module(ir, config)?;
    let cabal_src = cabal::generate_cabal();

    let mut out = String::with_capacity(umbrella.len() + ffi.len() + types_src.len() + cabal_src.len() + 256);
    push_section(&mut out, "src/Azul.hs", &umbrella);
    push_section(&mut out, "src/Azul/Internal/FFI.hs", &ffi);
    push_section(&mut out, "src/Azul/Types.hs", &types_src);
    push_section(&mut out, "azul.cabal", &cabal_src);
    Ok(out)
}

fn push_section(out: &mut String, path: &str, content: &str) {
    out.push_str(FILE_MARKER);
    out.push_str(path);
    out.push_str(END_MARKER);
    out.push('\n');
    out.push_str(content);
    if !content.ends_with('\n') {
        out.push('\n');
    }
}

// ============================================================================
// Per-file builders
// ============================================================================

fn generate_umbrella(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let mut builder = CodeBuilder::new(&config.indent);

    builder.line("{- |");
    builder.line("Module      : Azul");
    builder.line("Description : Auto-generated Haskell bindings for the Azul GUI framework.");
    builder.line("");
    builder.line("== Architecture alignment");
    builder.line("");
    builder.line("Azul's architecture (see @doc\\/guide\\/architecture.md@ lines 107-230) is");
    builder.line("explicitly a /functional/ GUI model: the UI is a pure function of the");
    builder.line("application data, @UI = f(data)@, in the Elm tradition. The critical");
    builder.line("refinement that distinguishes Azul from React/Elm is @RefAny@: a");
    builder.line("type-erased reference that lets the State Graph be /decoupled/ from the");
    builder.line("Visual Tree, so prop-drilling the React-style hierarchy constraint never");
    builder.line("arises.");
    builder.line("");
    builder.line("Haskell is the most natural target language for this paradigm:");
    builder.line("");
    builder.line("* Layout callbacks are expressed as @layout :: MyDataModel -> Dom@:");
    builder.line("  effectively pure, with the @IO@ effect happening at the FFI boundary");
    builder.line("  inside the generated callback trampoline.");
    builder.line("");
    builder.line("* Update callbacks are expressed as");
    builder.line("  @onClick :: Event -> MyDataModel -> (MyDataModel, Update)@:");
    builder.line("  Elm's @update@ story brought directly into Haskell. Imperative");
    builder.line("  side-effects (RefAny mutation) live inside the trampoline, not in");
    builder.line("  user code.");
    builder.line("");
    builder.line("* RAII over FFI handles uses 'Control.Exception.bracket' /");
    builder.line("  'Control.Exception.finally' instead of manual @_delete@ calls.");
    builder.line("");
    builder.line("* @'RefAny' a@ is a phantom-typed newtype, so downcasts are statically");
    builder.line("  tracked.");
    builder.line("");
    builder.line("Generated by azul-doc codegen v2 (lang_haskell). DO NOT EDIT MANUALLY.");
    builder.line("-}");
    builder.line("{-# LANGUAGE ForeignFunctionInterface #-}");
    builder.line("{-# LANGUAGE GeneralizedNewtypeDeriving #-}");
    builder.line("{-# LANGUAGE ScopedTypeVariables #-}");
    builder.blank();
    builder.line("module Azul");
    builder.indent();
    builder.line("( -- * Re-exports from \"Azul.Types\"");
    builder.line("  module Azul.Types");
    builder.line("  -- * Wrapper smart-constructors (RAII via @bracket@)");
    wrappers::emit_umbrella_exports(&mut builder, ir, config)?;
    builder.line(") where");
    builder.dedent();
    builder.blank();
    builder.line("import Azul.Types");
    builder.line("import qualified Azul.Internal.FFI as FFI");
    builder.line("import Control.Exception (bracket)");
    builder.line("import Foreign.Ptr (Ptr, nullPtr)");
    builder.line("import Foreign.Marshal.Alloc (alloca)");
    builder.line("import Foreign.Storable (Storable(..))");
    builder.blank();

    wrappers::emit_wrapper_bodies(&mut builder, ir, config)?;

    Ok(builder.finish())
}

fn generate_ffi(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let mut builder = CodeBuilder::new(&config.indent);

    builder.line("-- | Raw @foreign import ccall@ declarations against the libazul C ABI.");
    builder.line("-- This module is internal: use \"Azul\" for the curated surface.");
    builder.line("--");
    builder.line("-- Generated by azul-doc codegen v2 (lang_haskell). DO NOT EDIT MANUALLY.");
    builder.line("{-# LANGUAGE ForeignFunctionInterface #-}");
    builder.line("{-# LANGUAGE CApiFFI #-}");
    builder.blank();
    builder.line("module Azul.Internal.FFI where");
    builder.blank();
    builder.line("import Azul.Types");
    builder.line("import Foreign.C.Types");
    builder.line("import Foreign.Ptr (Ptr, FunPtr)");
    builder.line("import Data.Word (Word8, Word16, Word32, Word64)");
    builder.line("import Data.Int (Int8, Int16, Int32, Int64)");
    builder.blank();

    functions::emit_foreign_imports(&mut builder, ir, config)?;

    Ok(builder.finish())
}

fn generate_types_module(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let mut builder = CodeBuilder::new(&config.indent);

    builder.line("-- | Haskell datatypes that mirror the C ABI structs and enums,");
    builder.line("-- plus their hand-written 'Storable' instances.");
    builder.line("--");
    builder.line("-- @RefAny a@ is a phantom-typed newtype around the C ABI's");
    builder.line("-- type-erased reference, so downcasts to a specific user data");
    builder.line("-- model are statically tracked at the Haskell level.");
    builder.line("--");
    builder.line("-- Generated by azul-doc codegen v2 (lang_haskell). DO NOT EDIT MANUALLY.");
    builder.line("{-# LANGUAGE ForeignFunctionInterface #-}");
    builder.line("{-# LANGUAGE GeneralizedNewtypeDeriving #-}");
    builder.line("{-# LANGUAGE DeriveFunctor #-}");
    builder.blank();
    builder.line("module Azul.Types where");
    builder.blank();
    builder.line("import Foreign.C.Types");
    builder.line("import Foreign.Ptr (Ptr, FunPtr, castPtr, nullPtr)");
    builder.line("import Foreign.Storable (Storable(..))");
    builder.line("import Data.Word (Word8, Word16, Word32, Word64)");
    builder.line("import Data.Int (Int8, Int16, Int32, Int64)");
    builder.blank();

    builder.line("-- | Phantom-typed reference to type-erased Azul user data.");
    builder.line("--");
    builder.line("-- The phantom type parameter @a@ tracks at the Haskell type level which");
    builder.line("-- user data model the underlying C-side @AzRefAny@ holds. Downcasts");
    builder.line("-- ('refAnyDowncastRef') return @Maybe a@ so type errors at the boundary");
    builder.line("-- become value-level @Nothing@ rather than runtime crashes.");
    builder.line("newtype RefAny a = RefAny { unRefAny :: Ptr () }");
    builder.blank();

    types::emit_type_decls(&mut builder, ir, config)?;

    Ok(builder.finish())
}

// ============================================================================
// Shared naming helpers (used by all submodules)
// ============================================================================

/// Convert an IR type name (`PascalCase`) to the Haskell user-facing
/// data-type name. We drop the `Az` prefix at the wrapper layer but
/// keep PascalCase shape.
pub fn haskell_data_name(name: &str) -> String {
    sanitize_type_identifier(name)
}

/// Convert an IR type name to the FFI-side raw name: prefix `Az` so it
/// matches the C symbol convention. Used for C symbol references.
pub fn haskell_ffi_type_name(name: &str) -> String {
    format!("Az{}", name)
}

/// Convert an IR struct + field name to the Haskell record-field
/// accessor. Haskell records share a global namespace, so we prefix
/// each accessor with the lowercased type name to avoid collisions:
/// `App.foo` becomes `appFoo`.
pub fn haskell_field_name(struct_name: &str, field_name: &str) -> String {
    let prefix = lower_first(struct_name);
    let suffix = upper_camel_first_word(field_name);
    let combined = format!("{}{}", prefix, suffix);
    sanitize_value_identifier(&combined)
}

/// Convert an IR enum + variant name to a Haskell constructor name.
/// We prefix with the (de-prefixed) enum name so unrelated variants
/// from different enums don't collide.
pub fn haskell_variant_name(enum_name: &str, variant_name: &str) -> String {
    let combined = format!("{}_{}", enum_name, variant_name);
    sanitize_type_identifier(&combined)
}

/// Convert a snake/camel name to a Haskell value identifier (lower
/// first letter, sanitised against reserved words).
pub fn haskell_value_name(name: &str) -> String {
    sanitize_value_identifier(&lower_first_word(name))
}

/// Idiomatic method name: `new` → `withT`/`create`. The actual choice
/// of `with` vs `create` happens in `wrappers.rs`; this helper just
/// produces the lower-camel base.
pub fn haskell_method_name(method_name: &str) -> String {
    sanitize_value_identifier(&lower_first_word(method_name))
}

/// Sanitize an identifier intended to be a value-level name (must
/// start with a lowercase letter). Reserved words are mangled with
/// a trailing prime (`'`), per the Haskell convention for "modified"
/// versions of an existing binding.
pub fn sanitize_value_identifier(name: &str) -> String {
    if name.is_empty() {
        return "_anon".to_string();
    }
    let first = name.chars().next().unwrap();
    let mut s = if first.is_ascii_uppercase() {
        let mut out = String::with_capacity(name.len());
        for c in first.to_lowercase() {
            out.push(c);
        }
        out.push_str(&name[first.len_utf8()..]);
        out
    } else if first.is_ascii_digit() {
        format!("_{}", name)
    } else {
        name.to_string()
    };
    if is_haskell_reserved(&s) {
        s.push('\'');
    }
    s
}

/// Sanitize an identifier intended to be a type-level name (must
/// start with an uppercase letter). Reserved words shouldn't appear
/// here in practice (Haskell type names are CamelCase), but we mangle
/// them with a trailing prime for safety.
pub fn sanitize_type_identifier(name: &str) -> String {
    if name.is_empty() {
        return "Anon".to_string();
    }
    let first = name.chars().next().unwrap();
    let s = if first.is_ascii_lowercase() {
        let mut out = String::with_capacity(name.len());
        for c in first.to_uppercase() {
            out.push(c);
        }
        out.push_str(&name[first.len_utf8()..]);
        out
    } else if first.is_ascii_digit() {
        format!("T{}", name)
    } else {
        name.to_string()
    };
    if is_haskell_reserved(&s) {
        format!("{}'", s)
    } else {
        s
    }
}

fn is_haskell_reserved(s: &str) -> bool {
    matches!(
        s,
        "case"
            | "class"
            | "data"
            | "default"
            | "deriving"
            | "do"
            | "else"
            | "foreign"
            | "if"
            | "import"
            | "in"
            | "infix"
            | "infixl"
            | "infixr"
            | "instance"
            | "let"
            | "module"
            | "newtype"
            | "of"
            | "then"
            | "type"
            | "where"
            | "_"
    )
}

fn lower_first(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(s.len());
    let first = s.chars().next().unwrap();
    for c in first.to_lowercase() {
        out.push(c);
    }
    out.push_str(&s[first.len_utf8()..]);
    out
}

fn lower_first_word(s: &str) -> String {
    // Treat input as either snake_case or PascalCase; produce
    // camelCase suitable for value identifiers.
    if s.contains('_') {
        let mut parts = s.split('_').filter(|p| !p.is_empty());
        let first = parts.next().unwrap_or("");
        let mut out = first.to_ascii_lowercase();
        for p in parts {
            out.push_str(&upper_camel_first_word(p));
        }
        out
    } else {
        lower_first(s)
    }
}

fn upper_camel_first_word(s: &str) -> String {
    if s.contains('_') {
        let parts = s.split('_').filter(|p| !p.is_empty());
        let mut out = String::with_capacity(s.len());
        for p in parts {
            out.push_str(&upper_first(p));
        }
        out
    } else {
        upper_first(s)
    }
}

fn upper_first(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    let first = s.chars().next().unwrap();
    let mut out = String::with_capacity(s.len());
    for c in first.to_uppercase() {
        out.push(c);
    }
    out.push_str(&s[first.len_utf8()..]);
    out
}

/// Sanitize a doc-comment line so a stray `-}` doesn't terminate the
/// surrounding Haskell block comment if it ever ends up inside one.
pub fn sanitize_doc(s: &str) -> String {
    s.replace('\n', " ")
        .replace("-}", "- }")
        .trim()
        .to_string()
}
