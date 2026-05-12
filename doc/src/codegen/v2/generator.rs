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
        println!("[1/35] Generating DLL internal API...");
        CodeGenerator::generate_to_file(
            ir,
            &CodegenConfig::dll_internal(),
            &codegen_dir.join("dll_api_internal.rs"),
        )?;

        // 2. DLL external API (types + extern "C" declarations for dynamic linking)
        println!("[2/35] Generating DLL external API...");
        CodeGenerator::generate_to_file(
            ir,
            &CodegenConfig::dll_dynamic(),
            &codegen_dir.join("dll_api_external.rs"),
        )?;

        // 3. Re-exports (public API without Az prefix)
        println!("[3/35] Generating re-exports...");
        Self::generate_reexports_file(ir, &codegen_dir.join("reexports.rs"))?;

        // 4. C header
        println!("[4/35] Generating C header...");
        CodeGenerator::generate_to_file(
            ir,
            &CodegenConfig::c_header(),
            &codegen_dir.join("azul.h"),
        )?;

        // 5-10. C++ headers for all versions
        println!("[5-10/35] Generating C++ headers for all versions...");
        for cpp_std in CppStandard::all() {
            let filename = cpp_std.header_filename();
            println!("  Generating {}...", filename);
            CodeGenerator::generate_to_file(
                ir,
                &CodegenConfig::cpp_header(*cpp_std),
                &codegen_dir.join(&filename),
            )?;
        }

        // C++20+: emit a single shared `azul.cppm` module partition for
        // modules-aware toolchains. Re-exports the wrapper class names and
        // reflection helpers; users `import azul;` instead of #include.
        let cppm_path = codegen_dir.join("azul.cppm");
        let cppm_code = super::lang_cpp::generate_module_partition(
            ir,
            &CodegenConfig::cpp_header(CppStandard::Cpp20),
            CppStandard::Cpp20,
        );
        fs::write(&cppm_path, &cppm_code)?;
        println!(
            "[OK] Generated {} ({} bytes)",
            cppm_path.display(),
            cppm_code.len()
        );

        // 11. Public Rust API (legacy, may be removed)
        println!("[11/35] Generating public Rust API...");
        CodeGenerator::generate_to_file(
            ir,
            &CodegenConfig::rust_public_api(),
            &codegen_dir.join("azul.rs"),
        )?;

        // 12. Python extension (separate from C-API!) - goes to target/codegen/ for include!() in dll
        println!("[12/35] Generating Python extension...");
        Self::generate_python(ir, &codegen_dir.join("python_api.rs"))?;

        // 12b. PHP extension (Zend engine via ext-php-rs) - goes to target/codegen/ for include!() in dll
        println!("[12b/35] Generating PHP extension...");
        let php_api_code = super::lang_php_ext::generate(ir)?;
        let php_api_path = codegen_dir.join("php_api.rs");
        fs::write(&php_api_path, &php_api_code)?;
        println!(
            "[OK] Generated {} ({} bytes)",
            php_api_path.display(),
            php_api_code.len()
        );

        // 13. Memtest (memory layout tests) - goes to target/codegen/ for include!() in dll
        println!("[13/35] Generating memtest...");
        CodeGenerator::generate_to_file(
            ir,
            &CodegenConfig::memtest(),
            &codegen_dir.join("memtest.rs"),
        )?;

        // 14. C# (P/Invoke) bindings
        println!("[14/35] Generating C# bindings...");
        Self::write_string(
            super::lang_csharp::generate(ir, &CodegenConfig::c_header())?,
            &codegen_dir.join("Azul.cs"),
        )?;
        Self::write_string(
            super::lang_csharp::csproj::generate_csproj(),
            &codegen_dir.join("Azul.csproj"),
        )?;

        // 15. Ruby (ffi gem) bindings
        println!("[15/35] Generating Ruby bindings...");
        Self::write_string(
            super::lang_ruby::generate(ir, &CodegenConfig::c_header())?,
            &codegen_dir.join("azul.rb"),
        )?;
        Self::write_string(
            super::lang_ruby::gemspec::generate_gemspec(),
            &codegen_dir.join("azul.gemspec"),
        )?;

        // 16. Lua (LuaJIT FFI) bindings
        println!("[16/35] Generating Lua bindings...");
        Self::write_string(
            super::lang_lua::generate(ir, &CodegenConfig::c_header())?,
            &codegen_dir.join("azul.lua"),
        )?;
        Self::write_string(
            super::lang_lua::rockspec::generate_rockspec(),
            &codegen_dir.join("azul-1-1.rockspec"),
        )?;

        // 17. Pascal (FPC/Lazarus) bindings
        println!("[17/35] Generating Pascal bindings...");
        Self::write_string(
            super::lang_pascal::generate(ir, &CodegenConfig::c_header())?,
            &codegen_dir.join("azul.pas"),
        )?;
        Self::write_string(
            super::lang_pascal::lpi::generate_lpi("azul"),
            &codegen_dir.join("azul.lpi"),
        )?;

        // 18. Ada (GNAT) bindings — generator emits spec+body in one String
        println!("[18/35] Generating Ada bindings...");
        let ada_combined = super::lang_ada::generate(ir, &CodegenConfig::c_header())?;
        let (ada_spec, ada_body) = ada_combined
            .split_once(super::lang_ada::SPLIT_MARKER)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Ada generator output did not contain SPLIT_MARKER ({:?})",
                    super::lang_ada::SPLIT_MARKER
                )
            })?;
        Self::write_string(
            ada_spec.trim_end().to_string(),
            &codegen_dir.join("azul.ads"),
        )?;
        Self::write_string(
            ada_body.trim_start().to_string(),
            &codegen_dir.join("azul.adb"),
        )?;
        Self::write_string(
            super::lang_ada::gpr::generate_gpr(),
            &codegen_dir.join("azul.gpr"),
        )?;

        // 19. FreeBASIC bindings
        println!("[19/35] Generating FreeBASIC bindings...");
        Self::write_string(
            super::lang_freebasic::generate(ir, &CodegenConfig::c_header())?,
            &codegen_dir.join("azul.bi"),
        )?;

        // 20. Zig bindings — consumes the C header via @cImport, generator only
        //     emits idiomatic wrappers + a build.zig manifest.
        println!("[20/35] Generating Zig bindings...");
        Self::write_string(
            super::lang_zig::generate(ir, &CodegenConfig::c_header())?,
            &codegen_dir.join("azul.zig"),
        )?;
        Self::write_string(
            super::lang_zig::build_zig::generate_build_zig(),
            &codegen_dir.join("build.zig"),
        )?;

        // 21. PowerShell module — embeds the C# generator's output via Add-Type.
        println!("[21/35] Generating PowerShell bindings...");
        Self::write_string(
            super::lang_powershell::generate(ir, &CodegenConfig::c_header())?,
            &codegen_dir.join("Azul.psm1"),
        )?;
        Self::write_string(
            super::lang_powershell::manifest::generate_psd1(),
            &codegen_dir.join("Azul.psd1"),
        )?;

        // 22. PHP bindings (FFI extension, PHP 7.4+).
        println!("[22/35] Generating PHP bindings...");
        Self::write_string(
            super::lang_php::generate(ir, &CodegenConfig::c_header())?,
            &codegen_dir.join("Azul.php"),
        )?;
        Self::write_string(
            super::lang_php::composer::generate_composer_json(),
            &codegen_dir.join("composer.json"),
        )?;

        // 23. Perl bindings (FFI::Platypus).
        println!("[23/35] Generating Perl bindings...");
        Self::write_string(
            super::lang_perl::generate(ir, &CodegenConfig::c_header())?,
            &codegen_dir.join("Azul.pm"),
        )?;
        Self::write_string(
            super::lang_perl::cpanfile::generate_cpanfile(),
            &codegen_dir.join("cpanfile"),
        )?;

        // 24. OCaml bindings — generator emits .mli + .ml in one String,
        //     plus dune + dune-project manifests.
        println!("[24/35] Generating OCaml bindings...");
        let ocaml_combined = super::lang_ocaml::generate(ir, &CodegenConfig::c_header())?;
        let (ocaml_mli, ocaml_ml) = ocaml_combined
            .split_once(super::lang_ocaml::SPLIT_MARKER)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "OCaml generator output did not contain SPLIT_MARKER ({:?})",
                    super::lang_ocaml::SPLIT_MARKER
                )
            })?;
        Self::write_string(
            ocaml_mli.trim_end().to_string(),
            &codegen_dir.join("azul.mli"),
        )?;
        Self::write_string(
            ocaml_ml.trim_start().to_string(),
            &codegen_dir.join("azul.ml"),
        )?;
        Self::write_string(
            super::lang_ocaml::dune::generate_dune_project(),
            &codegen_dir.join("dune-project"),
        )?;
        Self::write_string(
            super::lang_ocaml::dune::generate_dune(),
            &codegen_dir.join("dune"),
        )?;

        // 25. Haskell bindings — multi-file: src/Azul.hs + src/Azul/Internal/FFI.hs
        //     + src/Azul/Types.hs + azul.cabal.
        println!("[25/35] Generating Haskell bindings...");
        let haskell_combined = super::lang_haskell::generate(ir, &CodegenConfig::c_header())?;
        Self::write_multifile(
            &haskell_combined,
            super::lang_haskell::FILE_MARKER,
            super::lang_haskell::END_MARKER,
            &codegen_dir.join("haskell"),
        )?;

        // 26. Java (JNA) bindings — multi-file: src/main/java/com/azul/*.java +
        //     pom.xml emitted separately.
        println!("[26/35] Generating Java bindings...");
        let java_combined = super::lang_java::generate(ir, &CodegenConfig::c_header())?;
        Self::write_multifile(
            &java_combined,
            super::lang_java::FILE_MARKER,
            super::lang_java::END_MARKER,
            &codegen_dir.join("java"),
        )?;
        Self::write_string(
            super::lang_java::pom::generate_pom_xml(),
            &codegen_dir.join("java/pom.xml"),
        )?;

        // 27. Kotlin (JNA) bindings — single Azul.kt + Gradle Kotlin DSL.
        println!("[27/35] Generating Kotlin bindings...");
        Self::write_string(
            super::lang_kotlin::generate(ir, &CodegenConfig::c_header())?,
            &codegen_dir.join("kotlin/Azul.kt"),
        )?;
        Self::write_string(
            super::lang_kotlin::gradle::generate_build_gradle_kts(),
            &codegen_dir.join("kotlin/build.gradle.kts"),
        )?;
        Self::write_string(
            super::lang_kotlin::gradle::generate_settings_gradle_kts(),
            &codegen_dir.join("kotlin/settings.gradle.kts"),
        )?;

        // 28. Fortran (F2003 iso_c_binding) bindings.
        println!("[28/35] Generating Fortran bindings...");
        Self::write_string(
            super::lang_fortran::generate(ir, &CodegenConfig::c_header())?,
            &codegen_dir.join("azul.f90"),
        )?;
        Self::write_string(
            super::lang_fortran::makefile::generate_makefile(),
            &codegen_dir.join("Makefile.fortran"),
        )?;

        // 29. Go (cgo) bindings — multi-file split.
        println!("[29/35] Generating Go bindings...");
        let go_combined = super::lang_go::generate(ir, &CodegenConfig::c_header())?;
        Self::write_multifile(
            &go_combined,
            super::lang_go::FILE_MARKER,
            super::lang_go::END_MARKER,
            &codegen_dir.join("go"),
        )?;
        Self::write_string(
            super::lang_go::gomod::generate_go_mod(),
            &codegen_dir.join("go/go.mod"),
        )?;

        // 30. Common Lisp (CFFI) bindings.
        println!("[30/35] Generating Common Lisp bindings...");
        Self::write_string(
            super::lang_lisp::generate(ir, &CodegenConfig::c_header())?,
            &codegen_dir.join("azul.lisp"),
        )?;
        Self::write_string(
            super::lang_lisp::asd::generate_asd(),
            &codegen_dir.join("azul.asd"),
        )?;

        // 31. Smalltalk (Pharo / UnifiedFFI) bindings.
        println!("[31/35] Generating Smalltalk bindings...");
        Self::write_string(
            super::lang_smalltalk::generate(ir, &CodegenConfig::c_header())?,
            &codegen_dir.join("Azul.st"),
        )?;
        Self::write_string(
            super::lang_smalltalk::baseline::generate_baseline(),
            &codegen_dir.join("BaselineOfAzul.st"),
        )?;

        // 32. Algol 68 (a68g) bindings — universal-framework showcase.
        println!("[32/35] Generating Algol 68 bindings...");
        Self::write_string(
            super::lang_algol68::generate(ir, &CodegenConfig::c_header())?,
            &codegen_dir.join("azul.a68"),
        )?;

        // 33. COBOL (GnuCOBOL) bindings — universal-framework showcase.
        println!("[33/35] Generating COBOL bindings...");
        Self::write_string(
            super::lang_cobol::generate(ir, &CodegenConfig::c_header())?,
            &codegen_dir.join("azul.cpy"),
        )?;

        // 34. Visual Basic 6 (32-bit, Windows) bindings — multi-file: Azul.bas
        //     plus one .cls per disposable type plus Azul.vbp project file.
        println!("[34/35] Generating Visual Basic 6 bindings...");
        let vb6_combined = super::lang_vb6::generate(ir, &CodegenConfig::c_header())?;
        Self::write_multifile(
            &vb6_combined,
            super::lang_vb6::FILE_MARKER,
            super::lang_vb6::END_MARKER,
            &codegen_dir.join("vb6"),
        )?;

        // 35. Node.js / Bun / Deno bindings — multi-file (single output file
        //     with runtime detection for the FFI loader, plus package.json).
        println!("[35/35] Generating Node bindings...");
        let node_combined = super::lang_node::generate(ir, &CodegenConfig::c_header())?;
        Self::write_multifile(
            &node_combined,
            super::lang_node::FILE_MARKER,
            super::lang_node::END_MARKER,
            &codegen_dir.join("node"),
        )?;
        Self::write_string(
            super::lang_node::package_json::generate_package_json(),
            &codegen_dir.join("node/package.json"),
        )?;

        Ok(())
    }

    /// Write a String to a path, creating parent dirs as needed and logging.
    fn write_string(content: String, output_path: &Path) -> Result<()> {
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(output_path, &content)?;
        println!(
            "[OK] Generated {} ({} bytes)",
            output_path.display(),
            content.len()
        );
        Ok(())
    }

    /// Split a multi-file generator output into its constituent files and
    /// write each under `base_dir`. Each section is introduced by a header
    /// line of the form `<file_marker><relative_path><end_marker>` (e.g.
    /// `// ==FILE: src/foo.go ==`). Content before the first marker is
    /// silently ignored — generators must put the first marker at the very
    /// top of their output. Empty sections are skipped.
    fn write_multifile(
        combined: &str,
        file_marker: &str,
        end_marker: &str,
        base_dir: &Path,
    ) -> Result<()> {
        let mut current_path: Option<String> = None;
        let mut buffer = String::new();
        let mut wrote_any = false;
        for line in combined.lines() {
            if let Some(rest) = line.trim_start().strip_prefix(file_marker) {
                if let Some(path) = current_path.take() {
                    Self::write_string(
                        std::mem::take(&mut buffer),
                        &base_dir.join(path),
                    )?;
                    wrote_any = true;
                }
                let path = match rest.rsplit_once(end_marker) {
                    Some((p, _)) => p.trim().to_string(),
                    None => rest.trim().to_string(),
                };
                current_path = Some(path);
            } else if current_path.is_some() {
                buffer.push_str(line);
                buffer.push('\n');
            }
        }
        if let Some(path) = current_path {
            Self::write_string(buffer, &base_dir.join(path))?;
            wrote_any = true;
        }
        if !wrote_any {
            return Err(anyhow::anyhow!(
                "multifile output had no sections (expected lines starting with {:?})",
                file_marker
            ));
        }
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
