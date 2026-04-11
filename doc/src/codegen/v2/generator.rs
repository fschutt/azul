//! Main code generator
//!
//! This module provides the unified code generator that takes an IR
//! and a configuration to produce output code.

use anyhow::Result;
use std::fs;
use std::path::Path;

use super::config::*;
use super::ir::*;
use super::lang_c::CGenerator;
use super::lang_cpp; // Use new dialect-based module
use super::lang_python::PythonGenerator;
use super::lang_rust::RustGenerator;

// ============================================================================
// Code Generator Trait
// ============================================================================

/// Trait for language-specific code generators
pub trait LanguageGenerator {
    /// Generate all code for the given IR and config
    fn generate(&self, ir: &CodegenIR, config: &CodegenConfig) -> Result<String>;

    /// Generate type definitions (structs, enums)
    fn generate_types(&self, ir: &CodegenIR, config: &CodegenConfig) -> Result<String>;

    /// Generate function declarations/definitions
    fn generate_functions(&self, ir: &CodegenIR, config: &CodegenConfig) -> Result<String>;

    /// Generate trait implementations
    fn generate_trait_impls(&self, ir: &CodegenIR, config: &CodegenConfig) -> Result<String>;
}

// ============================================================================
// Main Generator
// ============================================================================

/// Main code generator that dispatches to language-specific generators
pub struct CodeGenerator;

impl CodeGenerator {
    /// Generate code from IR using the specified configuration
    pub fn generate(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
        match config.target_lang {
            TargetLang::Rust => RustGenerator.generate(ir, config),
            TargetLang::CHeader => CGenerator.generate(ir, config),
            TargetLang::CppHeader { standard } => {
                // Use new dialect-based C++ generators
                lang_cpp::generate_cpp_header(ir, config, standard)
            }
            TargetLang::Python => {
                // Python needs its own config, use default PythonConfig
                let python_config = PythonConfig::python_extension();
                PythonGenerator.generate_python(ir, &python_config)
            }
        }
    }

    /// Generate and write to file
    pub fn generate_to_file(
        ir: &CodegenIR,
        config: &CodegenConfig,
        output_path: &Path,
    ) -> Result<()> {
        let code = Self::generate(ir, config)?;

        // Create parent directory if needed
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(output_path, &code)?;

        println!(
            "[OK] Generated {} ({} bytes)",
            output_path.display(),
            code.len()
        );

        Ok(())
    }
}

// ============================================================================
// Predefined Generation Targets
// ============================================================================

/// Collection of all standard code generation targets
pub struct GenerationTargets;

impl GenerationTargets {
    /// Generate all standard output files
    pub fn generate_all(ir: &CodegenIR, project_root: &Path) -> Result<()> {
        let codegen_dir = project_root.join("target").join("codegen");

        // 1. DLL internal API (types + C-ABI function bodies, #[no_mangle] gated via cfg_attr)
        //    Used by both build-dll (with cabi_export) and link-static (without cabi_export)
        println!("[1/13] Generating DLL internal API...");
        CodeGenerator::generate_to_file(
            ir,
            &CodegenConfig::dll_internal(),
            &codegen_dir.join("dll_api_internal.rs"),
        )?;

        // 2. DLL external API (types + extern "C" declarations for dynamic linking)
        println!("[2/13] Generating DLL external API...");
        CodeGenerator::generate_to_file(
            ir,
            &CodegenConfig::dll_dynamic(),
            &codegen_dir.join("dll_api_external.rs"),
        )?;

        // 3. Re-exports (public API without Az prefix)
        println!("[3/13] Generating re-exports...");
        Self::generate_reexports_file(ir, &codegen_dir.join("reexports.rs"))?;

        // 4. C header
        println!("[4/13] Generating C header...");
        CodeGenerator::generate_to_file(
            ir,
            &CodegenConfig::c_header(),
            &codegen_dir.join("azul.h"),
        )?;

        // 5-10. C++ headers for all versions
        println!("[5-10/13] Generating C++ headers for all versions...");
        for cpp_std in CppStandard::all() {
            let filename = cpp_std.header_filename();
            println!("  Generating {}...", filename);
            CodeGenerator::generate_to_file(
                ir,
                &CodegenConfig::cpp_header(*cpp_std),
                &codegen_dir.join(&filename),
            )?;
        }

        // 11. Public Rust API (legacy, may be removed)
        println!("[11/13] Generating public Rust API...");
        CodeGenerator::generate_to_file(
            ir,
            &CodegenConfig::rust_public_api(),
            &codegen_dir.join("azul.rs"),
        )?;

        // 12. Python extension (separate from C-API!) - goes to target/codegen/ for include!() in dll
        println!("[12/13] Generating Python extension...");
        Self::generate_python(ir, &codegen_dir.join("python_api.rs"))?;

        // 13. Memtest (memory layout tests) - goes to target/codegen/ for include!() in dll
        println!("[13/13] Generating memtest...");
        CodeGenerator::generate_to_file(
            ir,
            &CodegenConfig::memtest(),
            &codegen_dir.join("memtest.rs"),
        )?;

        Ok(())
    }

    /// Generate re-exports file
    fn generate_reexports_file(ir: &CodegenIR, output_path: &Path) -> Result<()> {
        let code = super::lang_reexports::generate_reexports(ir)?;

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(output_path, &code)?;

        println!(
            "[OK] Generated {} ({} bytes)",
            output_path.display(),
            code.len()
        );

        Ok(())
    }

    /// Generate Python extension module (separate from C-API structs!)
    fn generate_python(ir: &CodegenIR, output_path: &Path) -> Result<()> {
        let python_config = PythonConfig::python_extension();
        let code = PythonGenerator.generate_python(ir, &python_config)?;

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(output_path, &code)?;

        println!(
            "[OK] Generated {} ({} bytes)",
            output_path.display(),
            code.len()
        );

        Ok(())
    }
}

// ============================================================================
// Helper: Output formatting
// ============================================================================

/// Helper struct for building formatted output
pub struct CodeBuilder {
    code: String,
    indent_level: usize,
    indent_str: String,
}

impl CodeBuilder {
    pub fn new(indent_str: &str) -> Self {
        Self {
            code: String::new(),
            indent_level: 0,
            indent_str: indent_str.to_string(),
        }
    }

    /// Add a line with current indentation
    pub fn line(&mut self, text: &str) {
        for _ in 0..self.indent_level {
            self.code.push_str(&self.indent_str);
        }
        self.code.push_str(text);
        self.code.push('\n');
    }

    /// Add an empty line
    pub fn blank(&mut self) {
        self.code.push('\n');
    }

    /// Add raw text without indentation
    pub fn raw(&mut self, text: &str) {
        self.code.push_str(text);
    }

    /// Increase indentation
    pub fn indent(&mut self) {
        self.indent_level += 1;
    }

    /// Decrease indentation
    pub fn dedent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    /// Get the built code
    pub fn finish(self) -> String {
        self.code
    }

    /// Add a block with braces
    pub fn block<F>(&mut self, header: &str, f: F)
    where
        F: FnOnce(&mut Self),
    {
        self.line(&format!("{} {{", header));
        self.indent();
        f(self);
        self.dedent();
        self.line("}");
    }
}
