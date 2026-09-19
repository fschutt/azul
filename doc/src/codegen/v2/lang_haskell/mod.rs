//! Haskell binding generator.
//!
//! Produces a library of `.hs` modules, per-module C shim files and a
//! Cabal manifest. The surface user code sees is unchanged — `import Azul`
//! — but underneath, every layer is split so GHC compiles the binding as
//! ~200 small modules in parallel (`-j`) instead of three multi-megabyte
//! ones (which peaked at 3.5 GB RSS on one core, 2026-09).
//!
//! The split follows [`super::module_plan::ModulePlan`]:
//!
//! 1. `src/Azul/Types/<Unit>.hs` — one module per plan chunk
//!    (`Azul.Types.Css`, `Azul.Types.Dom`, `Azul.Types.Css2`, ...): the
//!    Haskell data declarations that mirror the C structs, enums and
//!    tagged unions, with `Storable` instances over the cbits layout
//!    oracle. A chunk imports the chunks it references; the plan
//!    guarantees the import graph is acyclic. `Azul.Types.Common` holds
//!    the UTF-8 codec every chunk may need. See `types.rs`.
//! 2. `src/Azul/Types.hs` — a facade re-exporting every chunk, so
//!    `import qualified Azul.Types as T` keeps working.
//! 3. `src/Azul/Internal/FFI/<Module>.hs` — raw `foreign import ccall`
//!    declarations for the classes of one api.json module (plus the
//!    callback wrappers and inbound-trampoline imports of that module's
//!    callback typedefs); `Azul.Internal.FFI.Host` carries the host-invoker
//!    protocol. Every import is `safe` (a host-handle `RefAny` destructor
//!    re-enters Haskell). `src/Azul/Internal/FFI.hs` is the facade.
//! 4. The idiomatic layer, in four tiers that only ever import downwards:
//!    `Azul.Internal.Runtime` (handle table, string marshalling),
//!    `Azul.Internal.Handles.<Module>` (the managed wrapper types, one file
//!    per api.json module, plus the `Azul.Internal.Handles` facade),
//!    `Azul.Internal.Callbacks` (the host-handle `RefAny`, one closure type
//!    and invoker per callback kind) and `Azul.<Module>` (constructors and
//!    methods of that module's classes, receiver last). See `wrappers.rs`.
//! 5. `src/Azul.hs` — the facade user code imports: re-exports the whole
//!    idiomatic layer and the `Azul.Types` entities it does not wrap.
//! 6. `cbits/azul_<module>.c` — the `_via` shims, inbound trampolines and
//!    layout-oracle functions for one api.json module; `cbits/azul_host.c`
//!    the host-invoker prototypes. See `cshim.rs`.
//! 7. `azul.cabal` — Cabal manifest listing every module and C source.
//!
//! ## Output protocol
//!
//! `generate(ir, config)` returns a single `String` with multiple
//! files separated by [`FILE_MARKER`] / [`END_MARKER`] header lines:
//!
//! ```text
//! -- ==FILE: src/Azul.hs ==
//! <Azul.hs contents>
//! -- ==FILE: src/Azul/Types/Css.hs ==
//! ...
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
use super::module_plan::ModulePlan;

pub mod cshim;

/// File-marker header that introduces each per-file section in the
/// concatenated output. The orchestrator splits on lines that start
/// with this prefix.
pub const FILE_MARKER: &str = "-- ==FILE: ";

/// Trailing marker that closes the file-marker header line.
pub const END_MARKER: &str = " ==";

/// Library name used in the Cabal manifest and in the Hackage display.
pub const LIB_NAME: &str = "azul";

/// The api.json module a class or callback typedef the plan does not know
/// (a function-only class) is filed under.
const FALLBACK_MODULE: &str = "misc";

/// One generated Haskell source: its Cabal module name and contents.
struct HsFile {
    module: String,
    src: String,
}

impl HsFile {
    fn path(&self) -> String {
        format!("src/{}.hs", self.module.replace('.', "/"))
    }
}

/// `css_2` -> `Css2`, `dom` -> `Dom`: the Haskell module segment for a
/// plan chunk or api.json module.
pub fn module_segment(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper = true;
    for c in name.chars() {
        if c == '_' || c == '-' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    if out.is_empty() {
        "Misc".to_string()
    } else {
        out
    }
}

/// Everything the per-unit emitters need to know about the split.
pub struct Split {
    pub plan: ModulePlan,
}

impl Split {
    pub fn new(ir: &CodegenIR) -> Self {
        Split {
            plan: ModulePlan::build(ir),
        }
    }

    /// The api.json module a class / type / callback typedef belongs to.
    pub fn module_of(&self, type_or_class: &str) -> String {
        self.plan
            .api_module_of(type_or_class)
            .unwrap_or(FALLBACK_MODULE)
            .to_string()
    }

    /// Every api.json module the per-class surface is split over: the
    /// modules that own a type, plus the fallback for function-only
    /// classes.
    pub fn api_modules(&self, ir: &CodegenIR) -> Vec<String> {
        let mut mods: std::collections::BTreeSet<String> =
            self.plan.api_modules().into_iter().collect();
        for f in &ir.functions {
            mods.insert(self.module_of(&f.class_name));
        }
        for cb in &ir.callback_typedefs {
            mods.insert(self.module_of(&cb.name));
        }
        mods.into_iter().collect()
    }

    /// Haskell module name of a types chunk.
    pub fn types_module(&self, chunk_idx: usize) -> String {
        format!("Azul.Types.{}", module_segment(&self.plan.chunks[chunk_idx].name))
    }
}

/// Public entry point. Generates the full multi-file Haskell binding
/// concatenated into a single `String` with file markers between
/// chunks.
pub fn generate(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let split = Split::new(ir);
    let mut files: Vec<HsFile> = Vec::new();

    // 1. Types: one module per plan chunk + the codec + the facade.
    files.push(HsFile {
        module: "Azul.Types.Common".to_string(),
        src: generate_types_common(config),
    });
    for (idx, chunk) in split.plan.chunks.iter().enumerate() {
        files.push(HsFile {
            module: split.types_module(idx),
            src: generate_types_chunk(ir, config, &split, idx)?,
        });
        let _ = chunk;
    }
    let type_modules: Vec<String> = files.iter().map(|f| f.module.clone()).collect();
    files.push(HsFile {
        module: "Azul.Types".to_string(),
        src: generate_reexport_facade(
            "Azul.Types",
            "Every Haskell datatype of the binding: the union of the per-module chunks.",
            &type_modules,
            &[],
        ),
    });

    // 2. FFI: one module per api.json module + host + facade.
    let api_modules = split.api_modules(ir);
    let mut ffi_modules = Vec::new();
    for m in &api_modules {
        let module = format!("Azul.Internal.FFI.{}", module_segment(m));
        files.push(HsFile {
            module: module.clone(),
            src: generate_ffi_module(ir, config, &split, m)?,
        });
        ffi_modules.push(module);
    }
    files.push(HsFile {
        module: "Azul.Internal.FFI.Host".to_string(),
        src: generate_ffi_host(ir, config)?,
    });
    ffi_modules.push("Azul.Internal.FFI.Host".to_string());
    files.push(HsFile {
        module: "Azul.Internal.FFI".to_string(),
        src: generate_reexport_facade(
            "Azul.Internal.FFI",
            "Raw @foreign import ccall@ declarations against the libazul C ABI. This module is internal: use \"Azul\" for the curated surface.",
            &ffi_modules,
            &["{-# LANGUAGE ForeignFunctionInterface #-}"],
        ),
    });

    // 3. The idiomatic layer + the `Azul` facade.
    let ctx = wrappers::Ctx::new(ir, config, &split);
    files.push(HsFile {
        module: "Azul.Internal.Runtime".to_string(),
        src: wrappers::generate_runtime_module(&ctx),
    });
    let mut handle_modules = Vec::new();
    for m in &api_modules {
        let module = format!("Azul.Internal.Handles.{}", module_segment(m));
        files.push(HsFile {
            module: module.clone(),
            src: wrappers::generate_handles_module(&ctx, m),
        });
        handle_modules.push(module);
    }
    files.push(HsFile {
        module: "Azul.Internal.Handles".to_string(),
        src: generate_reexport_facade(
            "Azul.Internal.Handles",
            "Every managed wrapper type of the binding.",
            &handle_modules,
            &[],
        ),
    });
    files.push(HsFile {
        module: "Azul.Internal.Callbacks".to_string(),
        src: wrappers::generate_callbacks_module(&ctx),
    });
    let mut api_hs_modules = Vec::new();
    for m in &api_modules {
        let module = format!("Azul.{}", module_segment(m));
        files.push(HsFile {
            module: module.clone(),
            src: wrappers::generate_api_module(&ctx, m),
        });
        api_hs_modules.push(module);
    }
    let idiomatic_bodies: Vec<&str> = files
        .iter()
        .filter(|f| {
            f.module == "Azul.Internal.Runtime"
                || f.module == "Azul.Internal.Callbacks"
                || f.module.starts_with("Azul.Internal.Handles.")
                || api_hs_modules.contains(&f.module)
        })
        .map(|f| f.src.as_str())
        .collect();
    let mut facade_modules = vec![
        "Azul.Internal.Runtime".to_string(),
        "Azul.Internal.Handles".to_string(),
        "Azul.Internal.Callbacks".to_string(),
    ];
    facade_modules.extend(api_hs_modules.iter().cloned());
    files.push(HsFile {
        module: "Azul".to_string(),
        src: wrappers::generate_facade(&ctx, &facade_modules, &idiomatic_bodies),
    });

    // Each internal module imports the modules that declare the names it
    // uses, never a whole-API facade: through `Azul.Types`,
    // `Azul.Internal.FFI` and `Azul.Internal.Handles` every module depended
    // on (and loaded the interfaces of) every module below it, so the ~200
    // units compiled one layer at a time instead of in parallel.
    let facades: Vec<(&str, Vec<String>)> = vec![
        ("Azul.Types", type_modules.clone()),
        ("Azul.Internal.FFI", ffi_modules.clone()),
        ("Azul.Internal.Handles", handle_modules.clone()),
    ];
    narrow_facade_imports(&mut files, &facades);

    // Codegen-time guard: a Haskell module may declare each name at most
    // once. Emitters that key a declaration on something other than the
    // loop variable (a Vec's *element* type inside a per-*Vec* loop, for
    // instance) can silently produce two identical declarations, which
    // GHC only rejects much later with GHC-29916. Fail here instead.
    for f in &files {
        check_no_duplicate_declarations(&f.path(), &f.src)?;
    }

    // 4. C shims, one per api.json module, plus the host-invoker file.
    let mut c_sources: Vec<(String, String)> = Vec::new();
    for m in &api_modules {
        c_sources.push((
            format!("cbits/azul_{}.c", m),
            cshim::generate_c_shims_for_module(ir, config, &split, m),
        ));
    }
    c_sources.push((
        "cbits/azul_host.c".to_string(),
        cshim::generate_host_shims(ir, config),
    ));

    let exposed: Vec<String> = files.iter().map(|f| f.module.clone()).collect();
    let c_paths: Vec<String> = c_sources.iter().map(|(p, _)| p.clone()).collect();
    let cabal_src = cabal::generate_cabal(&ir.api_version, &exposed, &c_paths);

    let total: usize = files.iter().map(|f| f.src.len()).sum::<usize>()
        + c_sources.iter().map(|(_, s)| s.len()).sum::<usize>()
        + cabal_src.len();
    let mut out = String::with_capacity(total + 64 * files.len());
    for f in &files {
        push_section(&mut out, &f.path(), &f.src);
    }
    for (p, s) in &c_sources {
        push_section(&mut out, p, s);
    }
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

fn generated_header(builder: &mut CodeBuilder, what: &str) {
    builder.line(&format!("-- | {}", what));
    builder.line("--");
    builder.line("-- Generated by azul-doc codegen v2 (lang_haskell). DO NOT EDIT MANUALLY.");
}

/// A module that only re-exports other modules.
fn generate_reexport_facade(
    name: &str,
    what: &str,
    modules: &[String],
    pragmas: &[&str],
) -> String {
    let mut b = CodeBuilder::new("    ");
    generated_header(&mut b, what);
    for p in pragmas {
        b.line(p);
    }
    b.line("{-# OPTIONS_GHC -Wno-unused-imports #-}");
    b.blank();
    b.line(&format!("module {}", name));
    b.indent();
    for (i, m) in modules.iter().enumerate() {
        let prefix = if i == 0 { "( " } else { ", " };
        b.line(&format!("{}module {}", prefix, m));
    }
    b.line(") where");
    b.dedent();
    b.blank();
    for m in modules {
        b.line(&format!("import {}", m));
    }
    b.finish()
}

/// `Azul.Types.Common`: the UTF-8 codec for the AzString boundary.
/// `Foreign.C.String` would use the locale encoding, which is not
/// guaranteed to be UTF-8; libazul strings always are.
fn generate_types_common(config: &CodegenConfig) -> String {
    let mut builder = CodeBuilder::new(&config.indent);
    generated_header(&mut builder, "UTF-8 codec shared by every \"Azul.Types\" chunk.");
    builder.blank();
    builder.line("module Azul.Types.Common where");
    builder.blank();
    builder.line("import Data.Bits ((.&.), (.|.), shiftL, shiftR)");
    builder.line("import Data.Char (chr, ord)");
    builder.line("import Data.Word (Word8)");
    builder.blank();
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
    builder.line("| n < 0x10000 = [fromIntegral (0xE0 .|. (n `shiftR` 12)), cont (n `shiftR` 6), cont n]");
    builder.line("| otherwise = [fromIntegral (0xF0 .|. (n `shiftR` 18)), cont (n `shiftR` 12), cont (n `shiftR` 6), cont n]");
    builder.indent();
    builder.line("where n = ord c");
    builder.dedent();
    builder.dedent();
    builder.line("cont n = fromIntegral (0x80 .|. (n .&. 0x3F))");
    builder.dedent();
    builder.dedent();
    builder.blank();
    builder.line("-- | Decode UTF-8 bytes into a Haskell String (malformed sequences become U+FFFD).");
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
    builder.line("multi k acc (b : bs) | b .&. 0xC0 == 0x80 = multi (k - 1) ((acc `shiftL` 6) .|. fromIntegral (b .&. 0x3F)) bs");
    builder.line("multi _ _ bs = '\\xFFFD' : decodeUtf8 bs");
    builder.dedent();
    builder.dedent();
    builder.dedent();
    builder.blank();
    builder.finish()
}

/// One `Azul.Types.<Unit>` chunk: the declarations of the plan chunk's
/// types, importing the chunks they reference.
fn generate_types_chunk(
    ir: &CodegenIR,
    config: &CodegenConfig,
    split: &Split,
    idx: usize,
) -> Result<String> {
    let chunk = &split.plan.chunks[idx];
    let mut builder = CodeBuilder::new(&config.indent);
    generated_header(
        &mut builder,
        &format!(
            "Haskell datatypes mirroring the C ABI structs and enums of the api.json module @{}@ (unit {} of the split), with 'Storable' instances whose sizes, alignments and member offsets are the C compiler's (the cbits layout oracle, see cshim.rs).",
            chunk.api_module, chunk.ordinal
        ),
    );
    builder.line("{-# LANGUAGE ForeignFunctionInterface #-}");
    builder.line("{-# LANGUAGE GeneralizedNewtypeDeriving #-}");
    builder.line("{-# LANGUAGE DeriveFunctor #-}");
    builder.line("{-# OPTIONS_GHC -Wno-unused-imports -Wno-unused-matches #-}");
    builder.blank();
    builder.line(&format!("module {} where", split.types_module(idx)));
    builder.blank();
    builder.line("import Azul.Types.Common");
    for dep in &chunk.deps {
        builder.line(&format!("import {}", split.types_module(*dep)));
    }
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

    let members: std::collections::BTreeSet<&str> = chunk.types.iter().map(|s| s.as_str()).collect();
    types::emit_type_decls_for(&mut builder, ir, config, &|t| members.contains(t))?;
    Ok(builder.finish())
}

/// `Azul.Internal.FFI.<Module>`: the C imports of one api.json module.
fn generate_ffi_module(
    ir: &CodegenIR,
    config: &CodegenConfig,
    split: &Split,
    api_module: &str,
) -> Result<String> {
    let mut builder = CodeBuilder::new(&config.indent);
    generated_header(
        &mut builder,
        &format!(
            "Raw @foreign import ccall@ declarations for the classes and callback typedefs of the api.json module @{}@. Internal: use \"Azul\" for the curated surface.",
            api_module
        ),
    );
    builder.line("{-# LANGUAGE ForeignFunctionInterface #-}");
    builder.line("{-# LANGUAGE CApiFFI #-}");
    builder.line("{-# OPTIONS_GHC -Wno-unused-imports #-}");
    builder.blank();
    builder.line(&format!(
        "module Azul.Internal.FFI.{} where",
        module_segment(api_module)
    ));
    builder.blank();
    builder.line("import Azul.Types");
    builder.line("import Foreign.C.Types");
    builder.line("import Foreign.Ptr (Ptr, FunPtr)");
    builder.line("import Foreign.Marshal.Alloc (alloca)");
    builder.line("import Foreign.Storable (Storable(..), poke)");
    builder.line("import Data.Word (Word8, Word16, Word32, Word64)");
    builder.line("import Data.Int (Int8, Int16, Int32, Int64)");
    builder.blank();

    let belongs = |name: &str| split.module_of(name) == api_module;
    functions::emit_foreign_imports_for(&mut builder, ir, config, &belongs)?;

    // Per callback typedef, a `register<X>Callback` helper that hides the
    // inbound-trampoline triplet (mk_<X>_inner + c_Az<X>_set_inner +
    // p_Az<X>_trampoline) behind a single function. Lives here so the
    // type signatures match the mk_<X>_inner shape exactly.
    functions::emit_callback_register_helpers_for(&mut builder, ir, config, &belongs)?;

    Ok(builder.finish())
}

/// `Azul.Internal.FFI.Host`: the host-invoker protocol imports.
fn generate_ffi_host(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let mut builder = CodeBuilder::new(&config.indent);
    generated_header(
        &mut builder,
        "Host-invoker protocol imports (see core/src/host_invoker.rs): host-handle RefAny + per-kind invokers.",
    );
    builder.line("{-# LANGUAGE ForeignFunctionInterface #-}");
    builder.line("{-# OPTIONS_GHC -Wno-unused-imports #-}");
    builder.blank();
    builder.line("module Azul.Internal.FFI.Host where");
    builder.blank();
    builder.line("import Azul.Types");
    builder.line("import Foreign.C.Types");
    builder.line("import Foreign.Ptr (Ptr, FunPtr)");
    builder.line("import Data.Word (Word8, Word16, Word32, Word64)");
    builder.line("import Data.Int (Int8, Int16, Int32, Int64)");
    builder.blank();
    functions::emit_host_invoker_imports(&mut builder, ir, config);
    Ok(builder.finish())
}

// ============================================================================
// Codegen-time duplicate-declaration guard
// ============================================================================

/// Every declaration name a Haskell module binds at module scope, paired
/// with the 1-based line it was declared on.
///
/// Recognised shapes (all of which GHC rejects if repeated in one module):
/// - `data <Name>` / `newtype <Name>` / `type <Name>` at column 0
/// - a type signature `<name> :: <ty>` — at column 0 for a plain binding,
///   or indented directly under a `foreign import ...` header, which is
///   how this backend emits its FFI bindings.
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

/// Every name a module brings into scope for its importers: its
/// declarations (see [`module_scope_declarations`]) plus data constructors
/// (`data X = C ..`, `  | C ..`) and record fields (`  { f :: ..`,
/// `  , f :: ..`).
fn module_scope_names(src: &str) -> Vec<String> {
    let mut out: Vec<String> = module_scope_declarations(src)
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    let first_ident = |s: &str| -> Option<String> {
        let s = s.trim_start();
        let len = s.find(|c: char| !is_haskell_ident_char(c)).unwrap_or(s.len());
        (len > 0).then(|| s[..len].to_string())
    };
    for line in src.lines() {
        if line.starts_with("data ") || line.starts_with("newtype ") {
            if let Some(rhs) = line.split_once('=').map(|(_, r)| r) {
                out.extend(first_ident(rhs));
            }
            continue;
        }
        if !line.starts_with(' ') {
            continue;
        }
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("= ").or_else(|| t.strip_prefix("| ")) {
            out.extend(first_ident(rest).filter(|n| n.starts_with(|c: char| c.is_ascii_uppercase())));
        } else if let Some(rest) = t.strip_prefix("{ ").or_else(|| t.strip_prefix(", ")) {
            if let Some(name) = first_ident(rest) {
                if rest[name.len()..].trim_start().starts_with("::") {
                    out.push(name);
                }
            }
        }
    }
    out
}

/// Rewrite every import of a facade in `facades` - `import <F>` or
/// `import qualified <F> as <Q>` - into imports of the facade's member
/// modules that declare a name the file uses (qualified: `<Q>.<name>`;
/// unqualified: any identifier token). The facades themselves stay, for
/// user code.
fn narrow_facade_imports(files: &mut [HsFile], facades: &[(&str, Vec<String>)]) {
    use std::collections::{BTreeMap, BTreeSet};
    let by_module: BTreeMap<String, Vec<String>> = files
        .iter()
        .map(|f| (f.module.clone(), module_scope_names(&f.src)))
        .collect();
    let facade_names: BTreeSet<&str> = facades.iter().map(|(f, _)| *f).collect();
    for file in files.iter_mut() {
        if facade_names.contains(file.module.as_str()) {
            continue;
        }
        let tokens: BTreeSet<&str> = file
            .src
            .split(|c: char| !(is_haskell_ident_char(c) || c == '.'))
            .filter(|t| !t.is_empty())
            .collect();
        // A facade a module re-exports (`module Azul.Internal.Handles` in
        // `Azul`'s export list) must stay imported whole.
        let reexported = |facade: &str| {
            file.src.lines().any(|l| {
                let l = l.trim_start().trim_start_matches(['(', ',']).trim();
                l == format!("module {facade}")
            })
        };
        let mut out = String::with_capacity(file.src.len());
        for line in file.src.lines() {
            let rewritten = facades.iter().find_map(|(facade, members)| {
                if reexported(facade) {
                    return None;
                }
                let t = line.trim();
                let qualifier = if t == format!("import {facade}") {
                    None
                } else if let Some(q) = t
                    .strip_prefix(&format!("import qualified {facade} as "))
                    .filter(|q| !q.contains(' '))
                {
                    Some(q)
                } else {
                    return None;
                };
                let used = |m: &String| {
                    by_module.get(m).is_some_and(|names| {
                        names.iter().any(|n| match qualifier {
                            Some(q) => tokens.contains(format!("{q}.{n}").as_str()),
                            None => tokens.contains(n.as_str()),
                        })
                    })
                };
                let lines: Vec<String> = members
                    .iter()
                    .filter(|m| **m != file.module && used(m))
                    .map(|m| match qualifier {
                        Some(q) => format!("import qualified {m} as {q}"),
                        None => format!("import {m}"),
                    })
                    .collect();
                Some(lines)
            });
            match rewritten {
                Some(lines) => {
                    for l in lines {
                        out.push_str(&l);
                        out.push('\n');
                    }
                }
                None => {
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
        file.src = out;
    }
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
            "Haskell codegen emitted {} duplicate module-scope declaration(s) in {} \
             (GHC rejects these with GHC-29916 \"Multiple declarations of ...\"):\n{}",
            dupes.len(),
            path,
            dupes.join("\n")
        );
    }
    Ok(())
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

#[cfg(test)]
mod split_tests {
    use super::super::config::CodegenConfig;
    use super::super::module_plan::test_fixture_ir;
    use super::*;
    use std::collections::BTreeMap;

    fn generated() -> BTreeMap<String, String> {
        split_files(&test_fixture_ir())
    }

    fn split_files(ir: &CodegenIR) -> BTreeMap<String, String> {
        let out = generate(ir, &CodegenConfig::c_header()).expect("haskell codegen");
        let mut files = BTreeMap::new();
        let mut cur: Option<String> = None;
        for line in out.lines() {
            if let Some(rest) = line.strip_prefix(FILE_MARKER) {
                cur = Some(rest.trim_end_matches(END_MARKER).trim().to_string());
                files.insert(cur.clone().unwrap(), String::new());
            } else if let Some(p) = &cur {
                let f = files.get_mut(p).unwrap();
                f.push_str(line);
                f.push('\n');
            }
        }
        files
    }

    /// Every type lands in the chunk module of its api.json module, the
    /// container next to its element, and a chunk imports the chunks it
    /// references.
    #[test]
    fn types_split_per_module_with_imports() {
        let files = generated();
        let css = &files["src/Azul/Types/Css.hs"];
        let dom = &files["src/Azul/Types/Dom.hs"];
        assert!(css.contains("data Color0 = Color0"), "{}", css);
        assert!(dom.contains("data Dom = Dom"), "{}", dom);
        assert!(dom.contains("data DomVec = DomVec"), "DomVec follows Dom:\n{}", dom);
        assert!(!css.contains("data Dom ="));
        assert!(dom.contains("import Azul.Types.Css"), "dom embeds Color0:\n{}", dom);
        assert!(!css.contains("import Azul.Types.Dom"));
        assert!(files["src/Azul/Types/Widgets.hs"].contains("import Azul.Types.Dom"));
    }

    /// The facades re-export every unit, and the cabal manifest lists
    /// every module and C source the generator wrote.
    #[test]
    fn facades_and_manifest_cover_every_unit() {
        let files = generated();
        let hs: Vec<&String> = files.keys().filter(|k| k.ends_with(".hs")).collect();
        let cabal = &files["azul.cabal"];
        for path in &hs {
            let module = path.trim_start_matches("src/").trim_end_matches(".hs").replace('/', ".");
            assert!(cabal.contains(&format!("\n                        {}\n", module)), "cabal lacks {}", module);
        }
        for path in files.keys().filter(|k| k.ends_with(".c")) {
            assert!(cabal.contains(path.as_str()), "cabal lacks {}", path);
        }
        let types = &files["src/Azul/Types.hs"];
        for m in ["Azul.Types.Common", "Azul.Types.Css", "Azul.Types.Dom", "Azul.Types.Widgets"] {
            assert!(types.contains(&format!("module {}", m)), "Azul.Types lacks {}", m);
        }
        let azul = &files["src/Azul.hs"];
        for m in ["Azul.Internal.Runtime", "Azul.Internal.Handles", "Azul.Internal.Callbacks", "Azul.Dom", "Azul.Widgets"] {
            assert!(azul.contains(&format!("module {}", m)), "Azul lacks {}:\n{}", m, azul);
        }
        assert!(azul.contains("T.Update(..)"), "unwrapped types are re-exported:\n{}", azul);
    }

    /// The fixture plus a host-invoker callback kind: `RefAny`,
    /// `CallbackInfo`, `ButtonOnClickCallback` and `Button.with_on_click`.
    fn callback_fixture() -> CodegenIR {
        use super::super::ir::{
            ArgRefKind, CallbackArgInfo, CallbackTypedefDef, FunctionArg, FunctionDef,
            FunctionKind, StructDef, TypeCategory,
        };
        let mut ir = test_fixture_ir();
        let arg = |name: &str, ty: &str, rk: ArgRefKind| FunctionArg {
            name: name.into(),
            type_name: ty.into(),
            ref_kind: rk,
            doc: None,
            callback_info: None,
        };
        let func = |class: &str, method: &str, kind, args, ret: Option<&str>| FunctionDef {
            c_name: format!("Az{}_{}", class, method),
            class_name: class.into(),
            method_name: method.into(),
            kind,
            args,
            return_type: ret.map(|s| s.to_string()),
            fn_body: None,
            doc: vec![],
            is_const: false,
            is_unsafe: false,
        };
        let mut st = |name: &str, category| {
            let mut s: StructDef = ir.structs.iter().find(|s| s.name == "Button").unwrap().clone();
            s.name = name.into();
            s.module = "callbacks".into();
            s.fields.truncate(0);
            s.category = category;
            ir.type_to_module.insert(name.into(), "callbacks".into());
            ir.structs.push(s);
        };
        st("RefAny", TypeCategory::RefAny);
        st("CallbackInfo", TypeCategory::Regular);
        st("ButtonOnClickCallback", TypeCategory::Regular);
        ir.callback_typedefs.push(CallbackTypedefDef {
            name: "ButtonOnClickCallbackType".into(),
            args: vec![
                arg("data", "RefAny", ArgRefKind::Owned),
                arg("info", "CallbackInfo", ArgRefKind::Owned),
            ],
            return_type: Some("Update".into()),
            doc: vec![],
            module: "callbacks".into(),
            external_path: None,
            wrapper: None,
            dependencies: vec![],
            sort_order: 0,
        });
        ir.type_to_module.insert("ButtonOnClickCallbackType".into(), "callbacks".into());
        for class in ["RefAny", "CallbackInfo", "ButtonOnClickCallback"] {
            ir.functions.push(func(class, "delete", FunctionKind::Delete, vec![arg("x", class, ArgRefKind::RefMut)], None));
        }
        ir.functions.push(func("RefAny", "clone", FunctionKind::DeepCopy, vec![arg("instance", "RefAny", ArgRefKind::Ref)], Some("RefAny")));
        let mut cb = arg("callback", "ButtonOnClickCallback", ArgRefKind::Owned);
        cb.callback_info = Some(CallbackArgInfo {
            callback_typedef_name: "ButtonOnClickCallbackType".into(),
            callback_wrapper_name: "ButtonOnClickCallback".into(),
            trampoline_name: String::new(),
        });
        ir.functions.push(func(
            "Button",
            "with_on_click",
            FunctionKind::Method,
            vec![arg("button", "Button", ArgRefKind::Owned), arg("data", "RefAny", ArgRefKind::Owned), cb],
            Some("Button"),
        ));
        ir
    }

    /// The callback contract: the invoker runs the user's function under the
    /// guard (an exception must never unwind into libazul) with the RefAny
    /// as the current model; handlers may be functions of the typed model;
    /// `with_on_click` gets the model-bound `buttonOnClick`; by-value
    /// wrappers are moved (use-after-move raises) and a by-value `RefAny`
    /// that is not a callback's data accepts any Haskell value.
    #[test]
    fn callbacks_are_guarded_typed_and_model_bound() {
        let files = split_files(&callback_fixture());
        let cbs = &files["src/Azul/Internal/Callbacks.hs"];
        assert!(
            cbs.contains("azulInvokeButtonOnClickCallback handle p0 p1 out = azulGuard \"ButtonOnClickCallback\" azulStderr $ do"),
            "{}",
            cbs
        );
        assert!(cbs.contains("azulWithCurrentData p0 $ do"), "{}", cbs);
        assert!(!cbs.contains(" try ("), "the old unguarded try: {}", cbs);
        for head in [
            "instance {-# OVERLAPPING #-} ButtonOnClickCallbackHandler (RefAny -> CallbackInfo -> IO T.Update)",
            "instance {-# OVERLAPPABLE #-} Typeable a => ButtonOnClickCallbackHandler (a -> CallbackInfo -> IO T.Update)",
            "instance Typeable a => ButtonOnClickCallbackHandler (a -> CallbackInfo -> (a, T.Update))",
            "instance Typeable a => ButtonOnClickCallbackHandler (a -> CallbackInfo -> IO (a, T.Update))",
            "class ToRefAny d where",
        ] {
            assert!(cbs.contains(head), "missing `{}`:\n{}", head, cbs);
        }
        let widgets = &files["src/Azul/Widgets.hs"];
        assert!(
            widgets.contains("buttonWithOnClick :: (ButtonOnClickCallbackHandler h2) => RefAny -> h2 -> Button -> IO Button"),
            "{}",
            widgets
        );
        assert!(
            widgets.contains("buttonOnClick :: (ButtonOnClickCallbackHandler h2) => h2 -> Button -> IO Button"),
            "{}",
            widgets
        );
        assert!(widgets.contains("azulCurrentRefAnyClone \"buttonOnClick\""), "{}", widgets);
        assert!(widgets.contains("moveButton self $"), "{}", widgets);
        let dom = &files["src/Azul/Dom.hs"];
        assert!(dom.contains("moveDom a1 $"), "{}", dom);
        let runtime = &files["src/Azul/Internal/Runtime.hs"];
        assert!(runtime.contains("azulGuard :: String -> (String -> IO ()) -> IO () -> IO ()"));
        assert!(runtime.contains("Moved -> throwIO (AzulUseAfterMove cls)"));
    }

    /// The per-class surface is grouped by api.json module: the FFI import,
    /// the wrapper type and the method of a widget live in the widgets
    /// units, and a method returning another module's class goes through
    /// the handles facade rather than that module.
    #[test]
    fn per_class_surface_is_grouped_by_module() {
        let files = generated();
        assert!(files["src/Azul/Internal/FFI/Widgets.hs"].contains("\"AzButton_dom"));
        assert!(!files["src/Azul/Internal/FFI/Dom.hs"].contains("AzButton_"));
        assert!(files["src/Azul/Internal/Handles/Widgets.hs"].contains("data Button = Button"));
        assert!(files["src/Azul/Internal/Handles/Dom.hs"].contains("data Dom = Dom"));
        let widgets = &files["src/Azul/Widgets.hs"];
        assert!(widgets.contains("buttonDom :: Button -> IO Dom"), "{}", widgets);
        assert!(widgets.contains("import Azul.Internal.Handles"));
        assert!(files["src/Azul/Dom.hs"].contains("domCreateBody :: IO Dom"));
        assert!(files["cbits/azul_dom.c"].contains("az_hs_sizeof_Dom"));
        assert!(!files["cbits/azul_widgets.c"].contains("az_hs_sizeof_Dom"));
    }
}
