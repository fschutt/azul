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

/// Generate DLL static API code as String
pub fn generate_dll_static(api_data: &ApiData) -> Result<String> {
    let ir = build_ir_from_api(api_data)?;
    let config = CodegenConfig::dll_static();
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
/// Output is written to `target/codegen/v2/dll_api.rs`
pub fn generate_dll_api_v2(api_data: &ApiData, project_root: &Path) -> Result<()> {
    println!("[V2] Building IR from api.json...");
    let ir = build_ir_from_api(api_data)?;

    println!(
        "[V2] IR built: {} structs, {} enums, {} functions",
        ir.structs.len(),
        ir.enums.len(),
        ir.functions.len()
    );

    // Generate using static DLL config
    let config = CodegenConfig::dll_static();

    let output_path = project_root
        .join("target")
        .join("codegen")
        .join("v2")
        .join("dll_api.rs");

    println!("[V2] Generating to {}...", output_path.display());
    CodeGenerator::generate_to_file(&ir, &config, &output_path)?;

    println!("\n[OK] DLL API v2 generated successfully!");
    println!("     To use this, update dll/src/lib.rs include!() path to:");
    println!("     include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../target/codegen/v2/dll_api.rs\"));");

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
    println!("[V2] Building IR from api.json...");
    let ir = build_ir_from_api(api_data)?;

    println!(
        "[V2] IR built: {} structs, {} enums, {} functions",
        ir.structs.len(),
        ir.enums.len(),
        ir.functions.len()
    );

    // Generate all targets
    GenerationTargets::generate_all(&ir, project_root)?;

    println!("\n[OK] All v2 outputs generated successfully!");

    Ok(())
}

/// Generate Python extension using codegen v2 (writes to file)
///
/// This generates the Python extension module with PyO3 bindings.
/// Output is written to `target/codegen/v2/python_api.rs`
pub fn generate_python_v2(api_data: &ApiData, project_root: &Path) -> Result<()> {
    println!("[V2] Generating Python extension...");

    let code = generate_python(api_data)?;

    let output_path = project_root
        .join("target")
        .join("codegen")
        .join("v2")
        .join("python_api.rs");

    // Create parent directory if needed
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&output_path, &code)?;

    println!("\n[OK] Python extension v2 generated successfully!");
    println!(
        "     Output: {} ({} bytes)",
        output_path.display(),
        code.len()
    );
    println!("     To use this, update dll/src/lib.rs include!() path to:");
    println!("     include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../target/codegen/v2/python_api.rs\"));");

    Ok(())
}

/// Generate memtest using codegen v2 (writes to file)
///
/// This generates test code for validating the generated API sizes/alignments.
/// Output is written to `target/codegen/v2/memtest.rs`
/// Included via include!() in dll/src/lib.rs for `cargo test`.
pub fn generate_memtest_v2(api_data: &ApiData, project_root: &Path) -> Result<()> {
    println!("[V2] Generating memtest code...");

    let code = generate_memtest(api_data)?;

    let output_path = project_root
        .join("target")
        .join("codegen")
        .join("v2")
        .join("memtest.rs");

    // Create parent directory if needed
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&output_path, &code)?;

    println!("\n[OK] Memtest v2 generated successfully!");
    println!(
        "     Output: {} ({} bytes)",
        output_path.display(),
        code.len()
    );
    println!("     Run tests with: cd dll && cargo test");

    Ok(())
}
