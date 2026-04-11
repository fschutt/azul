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
pub mod lang_c;
pub mod lang_cpp;
pub mod lang_python;
pub mod lang_reexports;
pub mod lang_rust;
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
    ir_builder.build()
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
    // Try local development path first, then cargo registry
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();

    let local = project_root.join("../material-icons/assets/MaterialIcons-Regular.ttf");
    let font_path = if local.exists() {
        local
    } else {
        // Search cargo registry
        let registry_base = Path::new(&home).join(".cargo/registry/src");
        let mut found = None;
        if registry_base.exists() {
            if let Ok(entries) = std::fs::read_dir(&registry_base) {
                for entry in entries.flatten() {
                    let candidate = entry.path()
                        .join("material-icons-0.3.0/assets/MaterialIcons-Regular.ttf");
                    if candidate.exists() {
                        found = Some(candidate);
                        break;
                    }
                }
            }
        }
        match found {
            Some(p) => p,
            None => {
                println!("[SKIP] Material Icons font not found, skipping compression");
                return Ok(());
            }
        }
    };

    let raw = std::fs::read(&font_path)?;
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
