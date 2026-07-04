//! Codegen v2 - Unified code generation architecture
//!
//! This module provides a clean, configuration-driven approach to code generation.
//! Instead of having separate generators with duplicated logic, we use:
//!
//! 1. **Intermediate Representation (IR)** - A unified model of all types and functions
//! 2. **CodegenConfig** - Configuration that describes what output to generate
//! 3. **CodeGenerator** - A single generator that produces output based on config
//!
//! # Architecture
//!
//! ```text
//! api.json
//!     │
//!     ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Intermediate Representation (IR)         │
//! │                                                             │
//! │  StructsIR: Vec<StructDef>     - All types with metadata    │
//! │  FunctionsIR: Vec<FunctionDef> - ALL functions incl. traits │
//! │  TraitsIR: Vec<TraitImpl>      - Trait implementations      │
//! └─────────────────────────────────────────────────────────────┘
//!     │
//!     ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                      CodegenConfig                          │
//! │                                                             │
//! │  target_lang: Rust | CHeader | CppHeader                    │
//! │  cabi_funcs: InternalBindings | ExternalBindings | None     │
//! │  structs: Prefixed | Unprefixed | None                      │
//! │  trait_impls: UsingDerive | UsingTransmute | UsingCAPI      │
//! └─────────────────────────────────────────────────────────────┘
//!     │
//!     ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                      CodeGenerator                          │
//! │                                                             │
//! │  fn generate(ir: &IR, config: &CodegenConfig) -> String     │
//! └─────────────────────────────────────────────────────────────┘
//!     │
//!     ├──► dll_api.rs           (static DLL)
//!     ├──► dll_api_dynamic.rs   (dynamic linking)
//!     ├──► azul.h               (C header)
//!     ├──► azul.hpp             (C++ header)
//!     ├──► azul.rs              (public Rust API)
//!     └──► python_structs.rs    (Python extension structs - SEPARATE from C-API)
//! ```
//!
//! # Important Design Decision
//!
//! Python extension structs are generated **separately** from C-API structs.
//! This is because:
//! - Python uses PyO3 with `#[pyclass]` attributes
//! - Python needs different trait implementations (no C-ABI calls)
//! - Python has wrapper types for callbacks that C doesn't need
//! - Python skips certain types (recursive types, VecRef types)
//!
//! The Python generator uses its own `PythonConfig` that extends the base config
//! with Python-specific options.

pub mod config;
pub mod generator;
pub mod ir;
pub mod ir_builder;
pub mod lang_ada;
pub mod lang_algol68;
pub mod lang_c;
pub mod lang_cobol;
pub mod lang_cpp;
pub mod lang_csharp;
pub mod lang_fortran;
pub mod lang_freebasic;
pub mod lang_go;
pub mod lang_haskell;
pub mod lang_java; // declared before lang_kotlin (Kotlin re-exports Java helpers)
pub mod lang_kotlin;
pub mod lang_lisp;
pub mod lang_lua;
pub mod lang_node;
pub mod lang_ocaml;
pub mod lang_pascal;
pub mod lang_perl;
pub mod lang_php;
pub mod lang_php_ext;
pub mod lang_powershell;
pub mod lang_python;
pub mod lang_reexports;
pub mod lang_ruby;
pub mod lang_rust;
pub mod lang_smalltalk;
pub mod lang_vb6;
pub mod lang_zig;
pub mod managed_host_invoker;
pub mod managed_lang_helpers;
pub mod rust;
pub mod transmute_helpers; // New Rust generators (static/dynamic binding)

pub use config::*;
pub use generator::*;
pub use ir::*;
pub use ir_builder::*;
pub use lang_reexports::generate_reexports;
pub use rust::{RustDynamicGenerator, RustStaticGenerator};

use crate::api::ApiData;
use anyhow::Result;
use std::path::Path;

// ============================================================================
// Helper: Build IR from ApiData
// ============================================================================

fn build_ir_from_api(api_data: &ApiData) -> Result<CodegenIR> {
    let version_str = api_data
        .get_latest_version_str()
        .ok_or_else(|| anyhow::anyhow!("No versions found in api.json"))?;
    let version_data = api_data
        .get_version(&version_str)
        .ok_or_else(|| anyhow::anyhow!("Version {} not found", version_str))?;

    let ir_builder = IRBuilder::new(version_data);
    let mut ir = ir_builder.build()?;
    ir.api_version = version_str.to_string();
    Ok(ir)
}

// ============================================================================
// String-returning generators (all configs return String, caller writes to file)
// ============================================================================

/// Generate DLL internal bindings API code as String
pub fn generate_dll_internal(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::dll_internal();
    CodeGenerator::generate(&ir, &config)
}

/// Generate DLL dynamic API code as String
pub fn generate_dll_dynamic(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::dll_dynamic();
    CodeGenerator::generate(&ir, &config)
}

/// Generate DLL types only (no functions) as String
pub fn generate_dll_types_only(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::dll_types_only();
    CodeGenerator::generate(&ir, &config)
}

/// Generate C header as String
pub fn generate_c_header(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::c_header();
    CodeGenerator::generate(&ir, &config)
}

/// Generate C++ header as String
pub fn generate_cpp_header(api_data: &ApiData, standard: CppStandard) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::cpp_header(standard);
    CodeGenerator::generate(&ir, &config)
}

/// Generate public Rust API as String
pub fn generate_rust_public_api(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::rust_public_api();
    CodeGenerator::generate(&ir, &config)
}

/// Generate memtest code as String
pub fn generate_memtest(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::memtest();
    CodeGenerator::generate(&ir, &config)
}

/// Generate Python extension as String
pub fn generate_python(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let python_config = PythonConfig::python_extension();
    lang_python::PythonGenerator.generate_python(&ir, &python_config)
}

/// Generate C# (P/Invoke) bindings as String. Returns `Azul.cs` source.
pub fn generate_csharp(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::c_header();
    lang_csharp::generate(&ir, &config)
}

/// Generate Ruby (`ffi` gem) bindings as String. Returns `azul.rb` source.
pub fn generate_ruby(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::c_header();
    lang_ruby::generate(&ir, &config)
}

/// Generate Lua (LuaJIT FFI) bindings as String. Returns `azul.lua` source.
pub fn generate_lua(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::c_header();
    lang_lua::generate(&ir, &config)
}

/// Generate Pascal (FPC/Lazarus) bindings as String. Returns `azul.pas` source.
pub fn generate_pascal(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::c_header();
    lang_pascal::generate(&ir, &config)
}

/// Generate Ada (GNAT) bindings as a `(spec, body)` pair where the original
/// generator output combines `azul.ads` and `azul.adb` separated by
/// [`lang_ada::SPLIT_MARKER`].
pub fn generate_ada(api_data: &ApiData) -> Result<(String, String)> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::c_header();
    let combined = lang_ada::generate(&ir, &config)?;
    match combined.split_once(lang_ada::SPLIT_MARKER) {
        Some((spec, body)) => Ok((spec.trim_end().to_string(), body.trim_start().to_string())),
        None => Err(anyhow::anyhow!(
            "Ada generator output did not contain SPLIT_MARKER ({:?})",
            lang_ada::SPLIT_MARKER
        )),
    }
}

/// Generate FreeBASIC bindings as String. Returns `azul.bi` source.
pub fn generate_freebasic(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::c_header();
    lang_freebasic::generate(&ir, &config)
}

/// Generate Zig bindings as String. Returns `azul.zig` source.
/// (Zig consumes the C header directly via `@cImport`; the generator only
/// emits idiomatic wrappers + a `build.zig`, available separately.)
pub fn generate_zig(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::c_header();
    lang_zig::generate(&ir, &config)
}

/// Generate PowerShell module as String. Returns `Azul.psm1` source which
/// embeds the C# generator's output via `Add-Type` for FFI.
pub fn generate_powershell(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::c_header();
    lang_powershell::generate(&ir, &config)
}

/// Generate PHP bindings as String. Returns `Azul.php` source using the
/// built-in `FFI::cdef` extension (PHP 7.4+).
pub fn generate_php(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::c_header();
    lang_php::generate(&ir, &config)
}

/// Generate Perl bindings as String. Returns `Azul.pm` source using
/// `FFI::Platypus` (libffi-backed pure-Perl FFI).
pub fn generate_perl(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::c_header();
    lang_perl::generate(&ir, &config)
}

/// Generate OCaml bindings as a `(mli, ml)` pair. The generator returns the
/// interface and implementation in one String separated by
/// [`lang_ocaml::SPLIT_MARKER`]; this helper splits them.
pub fn generate_ocaml(api_data: &ApiData) -> Result<(String, String)> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::c_header();
    let combined = lang_ocaml::generate(&ir, &config)?;
    match combined.split_once(lang_ocaml::SPLIT_MARKER) {
        Some((mli, ml)) => Ok((mli.trim_end().to_string(), ml.trim_start().to_string())),
        None => Err(anyhow::anyhow!(
            "OCaml generator output did not contain SPLIT_MARKER ({:?})",
            lang_ocaml::SPLIT_MARKER
        )),
    }
}

/// Generate Haskell bindings as a multi-file String separated by
/// [`lang_haskell::FILE_MARKER`] / [`lang_haskell::END_MARKER`] headers.
pub fn generate_haskell(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::c_header();
    lang_haskell::generate(&ir, &config)
}

/// Generate Java (JNA) bindings as a multi-file String separated by
/// [`lang_java::FILE_MARKER`] / [`lang_java::END_MARKER`] headers.
pub fn generate_java(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::c_header();
    lang_java::generate(&ir, &config)
}

/// Generate Kotlin (JNA) bindings as String. Returns single `Azul.kt` source.
pub fn generate_kotlin(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::c_header();
    lang_kotlin::generate(&ir, &config)
}

/// Generate Fortran (F2003 iso_c_binding) bindings as String. Returns
/// `azul.f90` source.
pub fn generate_fortran(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::c_header();
    lang_fortran::generate(&ir, &config)
}

/// Generate Go (cgo) bindings as a multi-file String separated by
/// [`lang_go::FILE_MARKER`] / [`lang_go::END_MARKER`] headers.
pub fn generate_go(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::c_header();
    lang_go::generate(&ir, &config)
}

/// Generate Common Lisp (CFFI) bindings as String. Returns `azul.lisp` source.
pub fn generate_lisp(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::c_header();
    lang_lisp::generate(&ir, &config)
}

/// Generate Smalltalk (Pharo / UnifiedFFI) bindings as String. Returns the
/// Tonel-format `Azul.st` source.
pub fn generate_smalltalk(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::c_header();
    lang_smalltalk::generate(&ir, &config)
}

/// Generate Algol 68 (a68g) bindings as String. Returns `azul.a68` source.
pub fn generate_algol68(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::c_header();
    lang_algol68::generate(&ir, &config)
}

/// Generate COBOL (GnuCOBOL) bindings as String. Returns `azul.cpy` copybook
/// source.
pub fn generate_cobol(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::c_header();
    lang_cobol::generate(&ir, &config)
}

/// Generate Visual Basic 6 (32-bit, Windows) bindings as a multi-file String
/// separated by [`lang_vb6::FILE_MARKER`] / [`lang_vb6::END_MARKER`] headers.
pub fn generate_vb6(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::c_header();
    lang_vb6::generate(&ir, &config)
}

/// Generate Node.js / Bun / Deno bindings as a multi-file String separated by
/// [`lang_node::FILE_MARKER`] / [`lang_node::END_MARKER`] headers. Uses koffi
/// for Node and the runtime built-ins for Bun/Deno.
pub fn generate_node(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::c_header();
    lang_node::generate(&ir, &config)
}

// ============================================================================
// Legacy file-writing functions (for backwards compatibility)
// ============================================================================

/// Generate DLL API using codegen v2 (writes to file)
///
/// This generates the static DLL API (equivalent to `memtest dll` command)
/// Output is written to `target/codegen/dll_api.rs`
pub fn generate_dll_api_v2(api_data: &ApiData, project_root: &Path) -> Result<()> {
    println!("Building IR from api.json...");
    let ir = build_ir_from_api(api_data)?;

    println!(
        "IR built: {} structs, {} enums, {} functions",
        ir.structs.len(),
        ir.enums.len(),
        ir.functions.len()
    );

    // Generate using internal DLL config
    let config = CodegenConfig::dll_internal();

    let output_path = project_root
        .join("target")
        .join("codegen")
        .join("dll_api.rs");

    println!("Generating to {}...", output_path.display());
    CodeGenerator::generate_to_file(&ir, &config, &output_path)?;

    println!("\n[OK] DLL API generated successfully!");
    println!("     To use this, update dll/src/lib.rs include!() path to:");
    println!("     include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../target/codegen/dll_api.rs\"));");

    Ok(())
}

/// Generate all output files using codegen v2 (writes to files)
///
/// This generates all standard code generation targets:
/// - DLL static API
/// - DLL dynamic API  
/// - C header
/// - C++ header
/// - Public Rust API
/// - Python extension
pub fn generate_all_v2(api_data: &ApiData, project_root: &Path) -> Result<()> {
    println!("Building IR from api.json...");
    let ir = build_ir_from_api(api_data)?;

    println!(
        "IR built: {} structs, {} enums, {} functions",
        ir.structs.len(),
        ir.enums.len(),
        ir.functions.len()
    );

    // Generate all targets
    GenerationTargets::generate_all(&ir, project_root)?;

    // Generate minified + brotli-compressed api.json for web backend embedding
    generate_compressed_api_json(project_root)?;

    // Brotli-compress the Material Icons font for embedding
    compress_material_icons_font(project_root)?;

    println!("\n[OK] All outputs generated successfully!");

    Ok(())
}

/// Generate a minified, gzip-compressed api.json for embedding in the web backend.
///
/// The web backend needs the full API description at runtime for function
/// classification (framework vs callback vs server-entry-point). Storing it
/// compressed reduces binary size from ~3.7 MB to ~150 KB.
///
/// Output: target/codegen/api.json.gz
fn generate_compressed_api_json(project_root: &Path) -> Result<()> {
    let api_path = project_root.join("api.json");
    if !api_path.exists() {
        anyhow::bail!("api.json not found at {}", api_path.display());
    }

    let raw = std::fs::read_to_string(&api_path)?;
    let parsed: serde_json::Value = serde_json::from_str(&raw)?;
    let minified = serde_json::to_string(&parsed)?;

    let mut compressed = Vec::new();
    let params = brotli::enc::BrotliEncoderParams {
        quality: 11,
        ..Default::default()
    };
    brotli::BrotliCompress(&mut minified.as_bytes(), &mut compressed, &params)?;

    let output_path = project_root
        .join("target")
        .join("codegen")
        .join("api.json.br");
    std::fs::write(&output_path, &compressed)?;

    println!(
        "[OK] Generated {} ({} bytes, {:.0}x compression from {} bytes raw)",
        output_path.display(),
        compressed.len(),
        raw.len() as f64 / compressed.len() as f64,
        raw.len(),
    );

    Ok(())
}

/// Brotli-compress the Material Icons font for embedding.
///
/// The compressed font is written to target/codegen/material_icons.ttf.br
/// and included via include_bytes! in azul-layout's icon module.
/// This lets the linker DCE the raw 348KB `material_icons::FONT` constant
/// since nothing references it directly.
fn compress_material_icons_font(project_root: &Path) -> Result<()> {
    // Pull the font bytes straight from the `material-icons` crate's `FONT`
    // const (a build-time `include_bytes!` of `MaterialIcons-Regular.ttf`).
    // The previous filesystem search for `material-icons-0.3.0/assets/...` in
    // the cargo registry silently `[SKIP]`ped on CI: azul-doc does not pull
    // `material-icons` transitively, so that crate's source was never unpacked
    // and no `.br` was written — which then broke azul-dll's `include_bytes!`.
    let raw: &[u8] = material_icons::FONT;
    let mut compressed = Vec::new();
    let params = brotli::enc::BrotliEncoderParams {
        quality: 11,
        ..Default::default()
    };
    brotli::BrotliCompress(&mut &raw[..], &mut compressed, &params)?;

    let output_dir = project_root.join("target").join("codegen");
    std::fs::create_dir_all(&output_dir)?;
    let output_path = output_dir.join("material_icons.ttf.br");
    std::fs::write(&output_path, &compressed)?;

    println!(
        "[OK] Generated {} ({} bytes, {:.0}x compression from {} bytes raw)",
        output_path.display(),
        compressed.len(),
        raw.len() as f64 / compressed.len() as f64,
        raw.len(),
    );

    Ok(())
}

/// Generate Python extension using codegen v2 (writes to file)
///
/// This generates the Python extension module with PyO3 bindings.
/// Output is written to `target/codegen/python_api.rs`
pub fn generate_python_v2(api_data: &ApiData, project_root: &Path) -> Result<()> {
    println!("Generating Python extension...");

    let code = generate_python(api_data)?;

    let output_path = project_root
        .join("target")
        .join("codegen")
        .join("python_api.rs");

    // Create parent directory if needed
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&output_path, &code)?;

    println!("\n[OK] Python extension generated successfully!");
    println!(
        "     Output: {} ({} bytes)",
        output_path.display(),
        code.len()
    );
    println!("     To use this, update dll/src/lib.rs include!() path to:");
    println!("     include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../target/codegen/python_api.rs\"));");

    Ok(())
}

/// Generate memtest using codegen v2 (writes to file)
///
/// This generates test code for validating the generated API sizes/alignments.
/// Output is written to `target/codegen/memtest.rs`
/// Included via include!() in dll/src/lib.rs for `cargo test`.
pub fn generate_memtest_v2(api_data: &ApiData, project_root: &Path) -> Result<()> {
    println!("Generating memtest code...");

    let code = generate_memtest(api_data)?;

    let output_path = project_root
        .join("target")
        .join("codegen")
        .join("memtest.rs");

    // Create parent directory if needed
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&output_path, &code)?;

    println!("\n[OK] Memtest generated successfully!");
    println!(
        "     Output: {} ({} bytes)",
        output_path.display(),
        code.len()
    );
    println!("     Run tests with: cd dll && cargo test");

    Ok(())
}
