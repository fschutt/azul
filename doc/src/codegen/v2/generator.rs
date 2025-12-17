//! Main code generator
//!
//! This module provides the unified code generator that takes an IR
//! and a configuration to produce output code.

use std::path::Path;
use std::fs;
use anyhow::Result;

use super::config::*;
use super::ir::*;
use super::lang_rust::RustGenerator;
use super::lang_c::CGenerator;
use super::lang_cpp::CppGenerator;
use super::lang_python::PythonGenerator;

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
                CppGenerator::new(standard).generate(ir, config)
            },
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
        let memtest_dir = project_root.join("target").join("memtest");

        // 1. DLL static API
        println!("[1/6] Generating DLL static API...");
        CodeGenerator::generate_to_file(
            ir,
            &CodegenConfig::dll_static(),
            &memtest_dir.join("dll_api.rs"),
        )?;

        // 2. DLL dynamic API
        println!("[2/6] Generating DLL dynamic API...");
        CodeGenerator::generate_to_file(
            ir,
            &CodegenConfig::dll_dynamic(),
            &codegen_dir.join("dll_api_dynamic.rs"),
        )?;

        // 3. C header
        println!("[3/6] Generating C header...");
        CodeGenerator::generate_to_file(
            ir,
            &CodegenConfig::c_header(),
            &codegen_dir.join("azul.h"),
        )?;

        // 4. C++ header
        println!("[4/6] Generating C++ header...");
        CodeGenerator::generate_to_file(
            ir,
            &CodegenConfig::cpp_header(CppStandard::Cpp11),
            &codegen_dir.join("azul.hpp"),
        )?;

        // 5. Public Rust API
        println!("[5/6] Generating public Rust API...");
        CodeGenerator::generate_to_file(
            ir,
            &CodegenConfig::rust_public_api(),
            &codegen_dir.join("azul.rs"),
        )?;

        // 6. Python extension (separate from C-API!)
        println!("[6/6] Generating Python extension...");
        Self::generate_python(ir, &codegen_dir.join("python_api.rs"))?;

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
