//! Haskell binding generator.
//!
//! Produces a small library of `.hs` modules, a C shim file and a Cabal
//! manifest:
//!
//! 1. `src/Azul.hs` — the umbrella module user code imports: one managed wrapper type per
//!    resource-owning class, one function per api.json method (receiver last, so builder chains are
//!    `>>=` pipelines), the host-handle `RefAny` (`refAnyCreate` / `refAnyGet` / `refAnyModify`),
//!    callbacks as plain closures, and re-exports of the `Azul.Types` enums and plain structs. See
//!    `wrappers.rs`.
//! 2. `src/Azul/Internal/FFI.hs` — raw `foreign import ccall` declarations that link to the C ABI
//!    symbols (through the `_via` shims wherever a struct travels by value). Every import is
//!    `safe`: a host-handle `RefAny` destructor re-enters Haskell through the releaser, so any
//!    function that may drop one can call back.
//! 3. `src/Azul/Types.hs` — Haskell data declarations that mirror the C structs, enums and tagged
//!    unions, with `Storable` instances whose sizes, alignments and member offsets are imports of
//!    the cbits layout oracle (the C compiler's `sizeof` / `_Alignof` / `offsetof`).
//! 4. `cbits/azul_shims.c` — the `_via` shims, the inbound trampolines, the host-invoker prototypes
//!    and the layout oracle.
//! 5. `azul.cabal` — Cabal manifest declaring the library + deps.
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
//! -- ==FILE: cbits/azul_shims.c ==
//! <shim contents>
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

use super::{config::CodegenConfig, generator::CodeBuilder, ir::CodegenIR};

pub mod cshim;

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
    let cabal_src = cabal::generate_cabal(&ir.api_version);
    let cshim_src = cshim::generate_c_shims(ir, config);

    // Codegen-time guard: a Haskell module may declare each name at most
    // once. Emitters that key a declaration on something other than the
    // loop variable (a Vec's *element* type inside a per-*Vec* loop, for
    // instance) can silently produce two identical declarations, which
    // GHC only rejects much later with GHC-29916. Fail here instead.
    check_no_duplicate_declarations("src/Azul.hs", &umbrella)?;
    check_no_duplicate_declarations("src/Azul/Internal/FFI.hs", &ffi)?;
    check_no_duplicate_declarations("src/Azul/Types.hs", &types_src)?;

    let mut out = String::with_capacity(
        umbrella.len() + ffi.len() + types_src.len() + cabal_src.len() + cshim_src.len() + 256,
    );
    push_section(&mut out, "src/Azul.hs", &umbrella);
    push_section(&mut out, "src/Azul/Internal/FFI.hs", &ffi);
    push_section(&mut out, "src/Azul/Types.hs", &types_src);
    push_section(&mut out, "cbits/azul_shims.c", &cshim_src);
    push_section(&mut out, "azul.cabal", &cabal_src);
    Ok(out)
}

// ============================================================================
// Codegen-time duplicate-declaration guard
// ============================================================================

/// Every declaration name a Haskell module binds at module scope, paired
/// with the 1-based line it was declared on.
///
/// Recognised shapes (all of which GHC rejects if repeated in one module):
/// - `data <Name>` / `newtype <Name>` / `type <Name>` at column 0
/// - a type signature `<name> :: <ty>` — at column 0 for a plain binding, or indented directly
///   under a `foreign import ...` header, which is how this backend emits its FFI bindings.
///
/// Deliberately *not* matched, because they are not module-scope
/// declarations: record fields (`{ foo :: !T` / `, bar :: !T` — the line
/// starts with a brace or comma), inline type annotations
/// (`x <- peekByteOff p 0 :: IO CSize`, `sizeOf (undefined :: T)` — the
/// identifier is not immediately followed by `::`), and comments.
fn module_scope_declarations(src: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    // Haddock block comments (`{- | ... -}`) carry prose that can look
    // like a declaration; skip their contents. Haskell block comments
    // nest, so track depth rather than a boolean.
    let mut comment_depth: usize = 0;
    for (idx, line) in src.lines().enumerate() {
        let lineno = idx + 1;
        let trimmed = line.trim_start();
        let opens = line.matches("{-").count();
        let closes = line.matches("-}").count();
        let was_in_comment = comment_depth > 0;
        comment_depth = (comment_depth + opens).saturating_sub(closes);
        if was_in_comment || opens > 0 {
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with("--") {
            continue;
        }

        // `data X = ...` / `newtype X = ...` / `type X = ...` / `class X h where`
        if line.starts_with("data ")
            || line.starts_with("newtype ")
            || line.starts_with("type ")
            || line.starts_with("class ")
        {
            if let Some(name) = trimmed.split_whitespace().nth(1) {
                let name = name.trim_end_matches(|c: char| !is_haskell_ident_char(c));
                if !name.is_empty() {
                    out.push((name.to_string(), lineno));
                }
            }
            continue;
        }

        // `<name> ::` — a type signature, at column 0 or indented under a
        // `foreign import` header.
        let ident_len = trimmed
            .find(|c: char| !is_haskell_ident_char(c))
            .unwrap_or(trimmed.len());
        if ident_len == 0 {
            continue;
        }
        let (ident, rest) = trimmed.split_at(ident_len);
        if !rest.trim_start().starts_with("::") {
            continue;
        }
        if !ident.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
            continue;
        }
        out.push((ident.to_string(), lineno));
    }
    out
}

fn is_haskell_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '\''
}

/// Fail codegen if `src` declares any name more than once at module
/// scope. Duplicate declarations are a hard GHC error (GHC-29916
/// "Multiple declarations of ..."), so catching them here turns a
/// downstream Cabal build failure into an actionable codegen failure.
fn check_no_duplicate_declarations(path: &str, src: &str) -> Result<()> {
    let mut seen: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut dupes: Vec<String> = Vec::new();
    for (name, lineno) in module_scope_declarations(src) {
        match seen.get(&name) {
            Some(first) => dupes.push(format!(
                "  `{}` declared at line {} and again at line {}",
                name, first, lineno
            )),
            None => {
                seen.insert(name, lineno);
            }
        }
    }
    if !dupes.is_empty() {
        anyhow::bail!(
            "Haskell codegen emitted {} duplicate module-scope declaration(s) in {} (GHC rejects \
             these with GHC-29916 \"Multiple declarations of ...\"):\n{}",
            dupes.len(),
            path,
            dupes.join("\n")
        );
    }
    Ok(())
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
    wrappers::emit_umbrella_module(&mut builder, ir, config)?;
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
    builder.line("import Foreign.Marshal.Alloc (alloca)");
    builder.line("import Foreign.Storable (Storable(..), poke)");
    builder.line("import Data.Word (Word8, Word16, Word32, Word64)");
    builder.line("import Data.Int (Int8, Int16, Int32, Int64)");
    builder.blank();

    functions::emit_foreign_imports(&mut builder, ir, config)?;

    // Per callback typedef, a `register<X>Callback` helper that hides the
    // inbound-trampoline triplet (mk_<X>_inner + c_Az<X>_set_inner +
    // p_Az<X>_trampoline) behind a single function. Lives here (FFI.hs)
    // so the type signatures match the mk_<X>_inner shape exactly — both
    // modules import `Azul.Types` unqualified.
    functions::emit_callback_register_helpers(&mut builder, ir, config)?;

    Ok(builder.finish())
}

fn generate_types_module(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let mut builder = CodeBuilder::new(&config.indent);

    builder.line("-- | Haskell datatypes that mirror the C ABI structs and enums,");
    builder.line("-- with 'Storable' instances whose sizes, alignments and member offsets");
    builder.line("-- are the C compiler's (the cbits layout oracle, see cshim.rs).");
    builder.line("--");
    builder.line("-- Generated by azul-doc codegen v2 (lang_haskell). DO NOT EDIT MANUALLY.");
    builder.line("{-# LANGUAGE ForeignFunctionInterface #-}");
    builder.line("{-# LANGUAGE GeneralizedNewtypeDeriving #-}");
    builder.line("{-# LANGUAGE DeriveFunctor #-}");
    builder.line("{-# OPTIONS_GHC -Wno-unused-imports -Wno-unused-matches #-}");
    builder.blank();
    builder.line("module Azul.Types where");
    builder.blank();
    builder.line("import Foreign.C.Types");
    builder.line("import Foreign.Ptr (Ptr, FunPtr, castPtr, nullPtr)");
    builder.line("import qualified Foreign.Ptr");
    // `<vec>ToList` needs `alloca` for the per-element clone out-buffer.
    // Qualified-only so the symbol doesn't pollute the import namespace
    // of users who already had unqualified imports from
    // Foreign.Marshal.Alloc in their own modules.
    builder.line("import qualified Foreign.Marshal.Alloc");
    builder.line("import Foreign.Storable (Storable(..))");
    builder.line("import Data.Bits ((.&.), (.|.), shiftL, shiftR)");
    builder.line("import Data.Char (chr, ord)");
    builder.line("import Data.Word (Word8, Word16, Word32, Word64)");
    builder.line("import Data.Int (Int8, Int16, Int32, Int64)");
    builder.blank();

    // UTF-8 codec for the AzString boundary. `Foreign.C.String` would use
    // the locale encoding, which is not guaranteed to be UTF-8; libazul
    // strings always are.
    builder.line("-- | Encode a Haskell String as UTF-8 bytes (what every AzString holds).");
    builder.line("encodeUtf8 :: String -> [Word8]");
    builder.line("encodeUtf8 = concatMap enc");
    builder.indent();
    builder.line("where");
    builder.indent();
    builder.line("enc c");
    builder.indent();
    builder.line("| n < 0x80 = [fromIntegral n]");
    builder.line("| n < 0x800 = [fromIntegral (0xC0 .|. (n `shiftR` 6)), cont n]");
    builder.line(
        "| n < 0x10000 = [fromIntegral (0xE0 .|. (n `shiftR` 12)), cont (n `shiftR` 6), cont n]",
    );
    builder.line(
        "| otherwise = [fromIntegral (0xF0 .|. (n `shiftR` 18)), cont (n `shiftR` 12), cont (n \
         `shiftR` 6), cont n]",
    );
    builder.indent();
    builder.line("where n = ord c");
    builder.dedent();
    builder.dedent();
    builder.line("cont n = fromIntegral (0x80 .|. (n .&. 0x3F))");
    builder.dedent();
    builder.dedent();
    builder.blank();
    builder
        .line("-- | Decode UTF-8 bytes into a Haskell String (malformed sequences become U+FFFD).");
    builder.line("decodeUtf8 :: [Word8] -> String");
    builder.line("decodeUtf8 [] = []");
    builder.line("decodeUtf8 (b0 : rest)");
    builder.indent();
    builder.line("| b0 < 0x80 = chr (fromIntegral b0) : decodeUtf8 rest");
    builder.line("| b0 .&. 0xE0 == 0xC0 = multi 1 (fromIntegral (b0 .&. 0x1F)) rest");
    builder.line("| b0 .&. 0xF0 == 0xE0 = multi 2 (fromIntegral (b0 .&. 0x0F)) rest");
    builder.line("| b0 .&. 0xF8 == 0xF0 = multi 3 (fromIntegral (b0 .&. 0x07)) rest");
    builder.line("| otherwise = '\\xFFFD' : decodeUtf8 rest");
    builder.indent();
    builder.line("where");
    builder.indent();
    builder.line("multi :: Int -> Int -> [Word8] -> String");
    builder.line("multi 0 acc bs = chr acc : decodeUtf8 bs");
    builder.line(
        "multi k acc (b : bs) | b .&. 0xC0 == 0x80 = multi (k - 1) ((acc `shiftL` 6) .|. \
         fromIntegral (b .&. 0x3F)) bs",
    );
    builder.line("multi _ _ bs = '\\xFFFD' : decodeUtf8 bs");
    builder.dedent();
    builder.dedent();
    builder.dedent();
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

/// Does this Haskell type name shadow a Prelude type? Variants that
/// carry payloads of these types trip GHC's "Ambiguous occurrence"
/// check because the local `Azul.Types.<Name>` clashes with `Prelude.<Name>`.
fn shadows_prelude_type(s: &str) -> bool {
    matches!(
        s,
        "String"
            | "Maybe"
            | "Either"
            | "Bool"
            | "Int"
            | "Char"
            | "Float"
            | "Double"
            | "Word"
            | "IO"
            | "Map"
            | "Set"
            | "Show"
            | "Eq"
            | "Ord"
            | "Read"
            | "Functor"
            | "Monad"
            | "Applicative"
            | "Ordering"
            | "FilePath"
            | "Either"
            | "Handle"
            | "FileError"
            | "IOError"
            | "Maybe"
    )
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
    // Names that shadow Prelude types — `String`, `Maybe`, `Either`,
    // `Bool`, etc. — break with `Ambiguous occurrence` whenever a
    // variant constructor takes them as a payload, because GHC can't
    // decide between `Azul.Types.X` and `Prelude.X`. Prefix with `Az`
    // for those, mirroring the JVM/Java fix.
    let s = if shadows_prelude_type(&s) {
        format!("Az{}", s)
    } else {
        s
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
    s.replace('\n', " ").replace("-}", "- }").trim().to_string()
}
