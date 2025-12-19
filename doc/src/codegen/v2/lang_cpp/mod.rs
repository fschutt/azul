//! C++ Header Generators - Dialect-based Architecture
//!
//! This module provides C++ header generation with separate generators for each
//! C++ standard version (C++03, C++11, C++14, C++17, C++20, C++23).
//!
//! # Architecture
//!
//! ```text
//! lang_cpp/
//! ├── mod.rs        - This file: trait definitions and dispatcher
//! ├── common.rs     - Shared utilities (keyword escaping, type conversion)
//! ├── cpp03.rs      - C++03 generator (Colvin-Gibbons trick for move emulation)
//! ├── cpp11.rs      - C++11 generator (move semantics, noexcept)
//! ├── cpp17.rs      - C++17 generator (optional, string_view, nodiscard)
//! └── cpp20.rs      - C++20/23 generator (span, expected)
//! ```
//!
//! Each dialect generator inherits from previous versions and adds features.

mod common;
mod cpp03;
mod cpp11;
mod cpp17;
mod cpp20;

pub use common::*;
pub use cpp03::Cpp03Generator;
pub use cpp11::Cpp11Generator;
pub use cpp17::Cpp17Generator;
pub use cpp20::{Cpp20Generator, Cpp23Generator};

use anyhow::Result;
use super::config::*;
use super::ir::*;

// ============================================================================
// Trait Definitions
// ============================================================================

/// Base trait for C++ code generation features
/// 
/// Each dialect implements this trait with version-specific behavior.
pub trait CppDialect: Sync {
    /// Get the C++ standard version
    fn standard(&self) -> CppStandard;
    
    /// Check if this version supports move semantics (C++11+)
    fn has_move_semantics(&self) -> bool {
        self.standard() >= CppStandard::Cpp11
    }
    
    /// Check if this version supports noexcept (C++11+)
    fn has_noexcept(&self) -> bool {
        self.standard() >= CppStandard::Cpp11
    }
    
    /// Check if this version supports std::optional (C++17+)
    fn has_optional(&self) -> bool {
        self.standard() >= CppStandard::Cpp17
    }
    
    /// Check if this version supports std::variant (C++17+)
    fn has_variant(&self) -> bool {
        self.standard() >= CppStandard::Cpp17
    }
    
    /// Check if this version supports std::span (C++20+)
    fn has_span(&self) -> bool {
        self.standard() >= CppStandard::Cpp20
    }
    
    /// Check if this version supports [[nodiscard]] (C++17+)
    fn has_nodiscard(&self) -> bool {
        self.standard() >= CppStandard::Cpp17
    }
    
    /// Check if this version supports std::string_view (C++17+)
    fn has_string_view(&self) -> bool {
        self.standard() >= CppStandard::Cpp17
    }
    
    /// Check if this version supports std::expected (C++23)
    fn has_expected(&self) -> bool {
        self.standard() >= CppStandard::Cpp23
    }
    
    /// Check if this version supports enum class (C++11+)
    fn has_enum_class(&self) -> bool {
        self.standard() >= CppStandard::Cpp11
    }
    
    /// Check if this version supports std::function (C++11+)
    fn has_std_function(&self) -> bool {
        self.standard() >= CppStandard::Cpp11
    }
    
    /// Get noexcept specifier (empty for C++03)
    fn noexcept_specifier(&self) -> &'static str {
        if self.has_noexcept() { " noexcept" } else { "" }
    }
    
    /// Get [[nodiscard]] attribute (empty for pre-C++17)
    fn nodiscard_attr(&self) -> &'static str {
        if self.has_nodiscard() { "[[nodiscard]] " } else { "" }
    }
    
    /// Get memset function name (std::memset for C++11+, memset for C++03)
    fn memset_fn(&self) -> &'static str {
        if self.has_move_semantics() { "std::memset" } else { "memset" }
    }
    
    /// Get strlen function name (std::strlen for C++11+, strlen for C++03)
    fn strlen_fn(&self) -> &'static str {
        if self.has_move_semantics() { "std::strlen" } else { "strlen" }
    }
    
    /// Generate the full C++ header
    fn generate(&self, ir: &CodegenIR, config: &CodegenConfig) -> Result<String>;
    
    /// Generate class declaration (in-class, no method bodies)
    fn generate_class_declaration(
        &self,
        code: &mut String,
        struct_def: &StructDef,
        ir: &CodegenIR,
        config: &CodegenConfig,
    );
    
    /// Generate method implementations (out-of-class)
    fn generate_method_implementations(
        &self,
        code: &mut String,
        struct_def: &StructDef,
        ir: &CodegenIR,
        config: &CodegenConfig,
    );
    
    /// Generate destructor code
    fn generate_destructor(&self, code: &mut String, class_name: &str, c_type_name: &str, needs_destructor: bool);
    
    /// Generate copy/move constructors and assignment operators
    fn generate_copy_move_semantics(
        &self,
        code: &mut String,
        class_name: &str,
        c_type_name: &str,
        is_copy: bool,
        needs_destructor: bool,
    );
    
    /// Generate Vec-specific methods (iterator support, toStdVector, toSpan)
    fn generate_vec_methods(&self, code: &mut String, struct_def: &StructDef, config: &CodegenConfig);
    
    /// Generate String-specific methods (c_str, length, std::string interop)
    fn generate_string_methods(&self, code: &mut String, struct_def: &StructDef, config: &CodegenConfig);
    
    /// Generate Option-specific methods (isSome, isNone, unwrap, toStdOptional)
    fn generate_option_methods(&self, code: &mut String, struct_def: &StructDef, config: &CodegenConfig);
    
    /// Generate Result-specific methods (isOk, isErr, unwrap, toStdExpected)
    fn generate_result_methods(&self, code: &mut String, struct_def: &StructDef, config: &CodegenConfig);
}

// ============================================================================
// Dispatcher
// ============================================================================

/// Get the appropriate generator for a C++ standard
pub fn get_generator(standard: CppStandard) -> Box<dyn CppDialect> {
    match standard {
        CppStandard::Cpp03 => Box::new(Cpp03Generator),
        CppStandard::Cpp11 => Box::new(Cpp11Generator),
        CppStandard::Cpp14 => Box::new(Cpp11Generator), // C++14 uses same generator as C++11
        CppStandard::Cpp17 => Box::new(Cpp17Generator),
        CppStandard::Cpp20 => Box::new(Cpp20Generator),
        CppStandard::Cpp23 => Box::new(Cpp23Generator),
    }
}

/// Generate C++ header for a specific standard
pub fn generate_cpp_header(ir: &CodegenIR, config: &CodegenConfig, standard: CppStandard) -> Result<String> {
    let generator = get_generator(standard);
    generator.generate(ir, config)
}

/// Generate all C++ headers (all standards)
pub fn generate_all_cpp_headers(ir: &CodegenIR, config: &CodegenConfig) -> Result<Vec<(String, String)>> {
    let mut results = Vec::new();
    
    for &standard in CppStandard::all() {
        let filename = standard.header_filename();
        let code = generate_cpp_header(ir, config, standard)?;
        results.push((filename, code));
    }
    
    Ok(results)
}
