//! Configuration types for code generation
//!
//! This module defines the configuration structures that control
//! how code is generated for different targets.

use std::collections::BTreeSet;

// ============================================================================
// Target Language
// ============================================================================

/// Target programming language for code generation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetLang {
    /// Rust source code
    Rust,
    /// C header file
    CHeader,
    /// C++ header file with specific standard
    CppHeader { standard: CppStandard },
    /// Python extension module (PyO3)
    Python,
}

/// C++ language standard version
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CppStandard {
    Cpp03,
    Cpp11,
    Cpp14,
    Cpp17,
    Cpp20,
    Cpp23,
}

impl CppStandard {
    /// Get all supported C++ versions
    pub fn all() -> &'static [CppStandard] {
        &[
            CppStandard::Cpp03,
            CppStandard::Cpp11,
            CppStandard::Cpp14,
            CppStandard::Cpp17,
            CppStandard::Cpp20,
            CppStandard::Cpp23,
        ]
    }

    /// Get the version number as a string (e.g., "03", "11")
    pub fn version_number(&self) -> &'static str {
        match self {
            CppStandard::Cpp03 => "03",
            CppStandard::Cpp11 => "11",
            CppStandard::Cpp14 => "14",
            CppStandard::Cpp17 => "17",
            CppStandard::Cpp20 => "20",
            CppStandard::Cpp23 => "23",
        }
    }

    /// Get the standard flag for the compiler (e.g., "-std=c++11")
    pub fn standard_flag(&self) -> &'static str {
        match self {
            CppStandard::Cpp03 => "-std=c++03",
            CppStandard::Cpp11 => "-std=c++11",
            CppStandard::Cpp14 => "-std=c++14",
            CppStandard::Cpp17 => "-std=c++17",
            CppStandard::Cpp20 => "-std=c++20",
            CppStandard::Cpp23 => "-std=c++23",
        }
    }

    /// Get the header filename for this version
    pub fn header_filename(&self) -> String {
        format!("azul{}.hpp", self.version_number())
    }

    /// Check if this version supports move semantics (C++11+)
    pub fn has_move_semantics(&self) -> bool {
        *self >= CppStandard::Cpp11
    }

    /// Check if this version supports noexcept (C++11+)
    pub fn has_noexcept(&self) -> bool {
        *self >= CppStandard::Cpp11
    }

    /// Check if this version supports std::optional (C++17+)
    pub fn has_optional(&self) -> bool {
        *self >= CppStandard::Cpp17
    }

    /// Check if this version supports std::variant (C++17+)
    pub fn has_variant(&self) -> bool {
        *self >= CppStandard::Cpp17
    }

    /// Check if this version supports std::span (C++20+)
    pub fn has_span(&self) -> bool {
        *self >= CppStandard::Cpp20
    }

    /// Check if this version supports [[nodiscard]] (C++17+)
    pub fn has_nodiscard(&self) -> bool {
        *self >= CppStandard::Cpp17
    }

    /// Check if this version supports std::string_view (C++17+)
    pub fn has_string_view(&self) -> bool {
        *self >= CppStandard::Cpp17
    }

    /// Check if this version supports std::expected (C++23)
    pub fn has_expected(&self) -> bool {
        *self >= CppStandard::Cpp23
    }

    /// Check if this version supports enum class (C++11+)
    pub fn has_enum_class(&self) -> bool {
        *self >= CppStandard::Cpp11
    }

    /// Check if this version supports std::function (C++11+)
    pub fn has_std_function(&self) -> bool {
        *self >= CppStandard::Cpp11
    }

    /// Check if this version supports constexpr (C++11+)
    pub fn has_constexpr(&self) -> bool {
        *self >= CppStandard::Cpp11
    }
}

// ============================================================================
// C-ABI Function Generation Mode
// ============================================================================

/// How to generate C-ABI functions
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CAbiFunctionMode {
    /// Generate function definitions with bodies (for DLL compilation)
    ///
    /// ```rust,ignore
    /// #[no_mangle]
    /// pub extern "C" fn AzDom_new() -> AzDom {
    ///     unsafe { transmute(Dom::create_node()) }
    /// }
    /// ```
    InternalBindings {
        /// Generate #[no_mangle] attribute
        no_mangle: bool,
    },

    /// Generate extern "C" declarations for dynamic linking
    ///
    /// ```rust,ignore
    /// #[link(name = "azul_dll")]
    /// extern "C" {
    ///     fn AzDom_new() -> AzDom;
    /// }
    /// ```
    ExternalBindings {
        /// Library name for #[link(name = "...")] attribute
        link_library: String,
    },

    /// Don't generate C-ABI functions at all
    /// Used for Python extension that calls Rust directly
    None,
}

// ============================================================================
// Struct/Enum Generation Mode
// ============================================================================

/// How to generate struct and enum definitions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructMode {
    /// Generate with prefix: `pub struct AzDom { ... }`
    Prefixed,

    /// Generate without prefix: `pub struct Dom { ... }`
    Unprefixed,

    /// Generate prefixed internally, re-export unprefixed
    ///
    /// ```rust,ignore
    /// mod inner { pub struct AzDom { ... } }
    /// pub use inner::AzDom as Dom;
    /// ```
    PrefixedReexported,

    /// Don't generate struct definitions
    /// Used for files that only need function declarations
    None,
}

// ============================================================================
// Trait Implementation Mode
// ============================================================================

/// How to implement traits (Clone, Drop, PartialEq, etc.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraitImplMode {
    /// Use #[derive(...)] macros
    ///
    /// ```rust,ignore
    /// #[derive(Clone, PartialEq, Hash)]
    /// pub struct AzDom { ... }
    /// ```
    UsingDerive,

    /// Implement by transmuting to external type
    ///
    /// ```rust,ignore
    /// impl Clone for AzDom {
    ///     fn clone(&self) -> Self {
    ///         unsafe { transmute((self as *const _ as *const ExternalType).clone()) }
    ///     }
    /// }
    /// ```
    UsingTransmute {
        /// Crate containing the external types (e.g., "azul_core")
        external_crate: String,
    },

    /// Implement by calling C-ABI functions
    ///
    /// ```rust,ignore
    /// impl Clone for AzDom {
    ///     fn clone(&self) -> Self { AzDom_deepCopy(self) }
    /// }
    /// ```
    UsingCAPI,

    /// Don't implement traits (C/C++ headers don't have traits)
    None,
}

// ============================================================================
// Main Configuration
// ============================================================================

/// Configuration for code generation output
///
/// This structure controls all aspects of code generation.
/// Different configurations produce different output files.
#[derive(Debug, Clone)]
pub struct CodegenConfig {
    /// Target language syntax
    pub target_lang: TargetLang,

    /// How to generate C-ABI functions
    pub cabi_functions: CAbiFunctionMode,

    /// How to generate struct/enum definitions
    pub struct_mode: StructMode,

    /// How to implement traits (Clone, Drop, PartialEq, etc.)
    pub trait_impl_mode: TraitImplMode,

    /// Type prefix (e.g., "Az", "Az1", "")
    pub type_prefix: String,

    /// Whether to wrap output in a module
    pub module_wrapper: Option<String>,

    /// Additional imports/includes to add at top
    pub imports: Vec<String>,

    /// Filter: only generate these types (None = all)
    pub type_filter: Option<BTreeSet<String>>,

    /// Filter: skip these types
    pub type_exclude: BTreeSet<String>,

    /// Indentation string (e.g., "    " for 4 spaces)
    pub indent: String,

    /// Whether to generate documentation comments
    pub generate_docs: bool,

    /// Whether callback typedefs should alias external types
    /// When true: `pub type AzLayoutCallbackType = azul_core::callbacks::LayoutCallbackType;`
    /// When false: Generate the function pointer signature
    pub callback_typedef_use_external: bool,

    /// Replace external crate paths (e.g., "azul_dll::" -> "crate::")
    /// Used when generating code inside a crate that refers to itself
    pub external_crate_replacement: Option<(String, String)>,

    /// Whether to generate test functions for the API
    /// When true, generates #[test] functions that verify type sizes,
    /// layout compatibility, and basic API functionality
    pub generate_tests: bool,
}

impl CodegenConfig {
    /// Transform an external path based on external_crate_replacement
    pub fn transform_external_path(&self, path: &str) -> String {
        if let Some((from, to)) = &self.external_crate_replacement {
            if path.starts_with(from.as_str()) {
                return path.replacen(from.as_str(), to.as_str(), 1);
            }
        }
        path.to_string()
    }

    /// Check if a type should be included based on filters
    pub fn should_include_type(&self, type_name: &str) -> bool {
        // Check exclusion list first
        if self.type_exclude.contains(type_name) {
            return false;
        }

        // Check inclusion filter
        match &self.type_filter {
            Some(filter) => filter.contains(type_name),
            None => true,
        }
    }

    /// Apply the type prefix to a name
    ///
    /// Does NOT prefix:
    /// - Primitive types (bool, u8, usize, etc.)
    /// - Types that already have the prefix
    /// - Types containing :: (fully qualified paths)
    /// - c_void
    pub fn apply_prefix(&self, name: &str) -> String {
        apply_prefix_to_type(name, &self.type_prefix)
    }
}

/// Apply prefix to a potentially complex type string
///
/// Handles:
/// - Simple types: `Foo` → `AzFoo`
/// - Pointers: `*const Foo` → `*const AzFoo`
/// - Mutable pointers: `*mut Foo` → `*mut AzFoo`
/// - References: `&Foo` → `&AzFoo`, `&mut Foo` → `&mut AzFoo`
/// - Arrays: `[Foo;2]` → `[AzFoo;2]`
/// - Primitives: `*const u8` → `*const u8` (not prefixed)
fn apply_prefix_to_type(name: &str, prefix: &str) -> String {
    let name = name.trim();

    // Empty prefix means no prefixing
    if prefix.is_empty() {
        return name.to_string();
    }

    // Don't prefix if already prefixed
    if name.starts_with(prefix) {
        return name.to_string();
    }

    // Handle pointer types: *const T, *mut T
    if let Some(rest) = name.strip_prefix("*const ") {
        let inner = apply_prefix_to_type(rest.trim(), prefix);
        return format!("*const {}", inner);
    }
    if let Some(rest) = name.strip_prefix("*mut ") {
        let inner = apply_prefix_to_type(rest.trim(), prefix);
        return format!("*mut {}", inner);
    }

    // Handle reference types: &T, &mut T
    if let Some(rest) = name.strip_prefix("&mut ") {
        let inner = apply_prefix_to_type(rest.trim(), prefix);
        return format!("&mut {}", inner);
    }
    if let Some(rest) = name.strip_prefix("&") {
        let inner = apply_prefix_to_type(rest.trim(), prefix);
        return format!("&{}", inner);
    }

    // Handle array types: [T;N]
    if name.starts_with('[') && name.ends_with(']') {
        let inner = &name[1..name.len() - 1];
        if let Some(semi_pos) = inner.rfind(';') {
            let element_type = inner[..semi_pos].trim();
            let size = inner[semi_pos + 1..].trim();
            let prefixed_element = apply_prefix_to_type(element_type, prefix);
            return format!("[{};{}]", prefixed_element, size);
        }
    }

    // Handle Option<T>, Vec<T>, etc.
    if let Some(angle_pos) = name.find('<') {
        if name.ends_with('>') {
            let outer = &name[..angle_pos];
            let inner = &name[angle_pos + 1..name.len() - 1];
            let prefixed_outer = if is_primitive_or_builtin(outer) {
                outer.to_string()
            } else {
                format!("{}{}", prefix, outer)
            };
            let prefixed_inner = apply_prefix_to_type(inner, prefix);
            return format!("{}<{}>", prefixed_outer, prefixed_inner);
        }
    }

    // Don't prefix primitives or fully qualified paths
    if is_primitive_or_builtin(name) {
        return name.to_string();
    }

    // Don't prefix fully qualified paths
    if name.contains("::") {
        return name.to_string();
    }

    // Simple type - apply prefix
    format!("{}{}", prefix, name)
}

/// Check if a type name is a Rust primitive or built-in that shouldn't be prefixed
fn is_primitive_or_builtin(name: &str) -> bool {
    // Single letter types are generics (T, U, V, etc.)
    if name.len() == 1
        && name
            .chars()
            .next()
            .map(|c| c.is_ascii_uppercase())
            .unwrap_or(false)
    {
        return true;
    }

    matches!(
        name,
        // Integer primitives
        "bool" | "i8" | "i16" | "i32" | "i64" | "i128" | "isize" |
        "u8" | "u16" | "u32" | "u64" | "u128" | "usize" |
        // Float primitives
        "f32" | "f64" |
        // Other primitives
        "char" | "()" |
        // C types (NOT String - that's a custom Azul type, not std::string::String)
        "c_void" | "c_int" | "c_uint" | "c_long" | "c_ulong" | "c_char" | "c_uchar"
    )
}

// ============================================================================
// Predefined Configurations
// ============================================================================

impl CodegenConfig {
    /// DLL static linking API
    ///
    /// This is used when statically linking Azul into your application.
    /// Generates types + C-API functions (without #[no_mangle]) + trait impls via transmute.
    /// The C-API functions are internal (not exported) but still used by impl blocks.
    pub fn dll_static() -> Self {
        Self {
            target_lang: TargetLang::Rust,
            cabi_functions: CAbiFunctionMode::InternalBindings { no_mangle: false },
            struct_mode: StructMode::Prefixed,
            trait_impl_mode: TraitImplMode::UsingTransmute {
                external_crate: "azul_core".into(),
            },
            type_prefix: "Az".into(),
            module_wrapper: Some("dll".into()),
            imports: vec![
                "use core::ffi::c_void;".into(),
                "use core::ffi::c_int;".into(),
                "use core::mem::transmute;".into(),
                "use azul_layout::xml::svg::SvgMultiPolygonTessellation;".into(),
            ],
            type_filter: None,
            type_exclude: BTreeSet::new(),
            indent: "    ".into(),
            generate_docs: true,
            callback_typedef_use_external: false,
            // Replace azul_dll:: with crate:: since we're inside azul-dll
            external_crate_replacement: Some(("azul_dll::".into(), "crate::".into())),
            generate_tests: false,
        }
    }

    /// DLL build API (for building libazul.dylib/so/dll)
    ///
    /// This is used when compiling the azul-dll crate itself to produce
    /// the shared library. Generates types + C-API functions with #[no_mangle].
    pub fn dll_build() -> Self {
        Self {
            target_lang: TargetLang::Rust,
            cabi_functions: CAbiFunctionMode::InternalBindings { no_mangle: true },
            struct_mode: StructMode::Prefixed,
            trait_impl_mode: TraitImplMode::UsingTransmute {
                external_crate: "azul_core".into(),
            },
            type_prefix: "Az".into(),
            module_wrapper: Some("dll".into()),
            imports: vec![
                "use core::ffi::c_void;".into(),
                "use core::ffi::c_int;".into(),
                "use core::mem::transmute;".into(),
                "use azul_layout::xml::svg::SvgMultiPolygonTessellation;".into(),
            ],
            type_filter: None,
            type_exclude: BTreeSet::new(),
            indent: "    ".into(),
            generate_docs: true,
            callback_typedef_use_external: false,
            // Replace azul_dll:: with crate:: since we're inside azul-dll
            external_crate_replacement: Some(("azul_dll::".into(), "crate::".into())),
            generate_tests: false,
        }
    }

    /// DLL types only (for Python extension embedding)
    ///
    /// This generates ONLY type definitions, no C-API functions.
    /// Used by Python extension which needs the types but not the functions
    /// (functions are provided by dll_api.rs).
    pub fn dll_types_only() -> Self {
        Self {
            target_lang: TargetLang::Rust,
            cabi_functions: CAbiFunctionMode::None, // No functions!
            struct_mode: StructMode::Prefixed,
            trait_impl_mode: TraitImplMode::UsingTransmute {
                external_crate: "azul_core".into(),
            },
            type_prefix: "Az".into(),
            module_wrapper: Some("dll".into()),
            imports: vec![
                "use core::ffi::c_void;".into(),
                "use core::mem::transmute;".into(),
            ],
            type_filter: None,
            type_exclude: BTreeSet::new(),
            indent: "    ".into(),
            generate_docs: false, // Skip docs for embedded types
            callback_typedef_use_external: false,
            external_crate_replacement: Some(("azul_dll::".into(), "crate::".into())),
            generate_tests: false,
        }
    }

    /// DLL dynamic linking API
    ///
    /// This is used when linking against a pre-compiled .dylib/.so/.dll.
    /// Functions are extern declarations, traits call C-ABI functions.
    pub fn dll_dynamic() -> Self {
        Self {
            target_lang: TargetLang::Rust,
            cabi_functions: CAbiFunctionMode::ExternalBindings {
                link_library: "azul_dll".into(),
            },
            struct_mode: StructMode::Prefixed,
            trait_impl_mode: TraitImplMode::UsingCAPI,
            type_prefix: "Az".into(),
            module_wrapper: Some("dll".into()),
            imports: vec!["use core::ffi::c_void;".into()],
            type_filter: None,
            type_exclude: BTreeSet::new(),
            indent: "    ".into(),
            generate_docs: true,
            callback_typedef_use_external: false,
            external_crate_replacement: None,
            generate_tests: false,
        }
    }

    /// C header file
    pub fn c_header() -> Self {
        Self {
            target_lang: TargetLang::CHeader,
            cabi_functions: CAbiFunctionMode::ExternalBindings {
                link_library: "azul".into(),
            },
            struct_mode: StructMode::Prefixed,
            trait_impl_mode: TraitImplMode::None,
            type_prefix: "Az".into(),
            module_wrapper: None,
            imports: vec![
                "#include <stdbool.h>".into(),
                "#include <stdint.h>".into(),
                "#include <stddef.h>".into(),
            ],
            type_filter: None,
            type_exclude: BTreeSet::new(),
            indent: "    ".into(),
            generate_docs: true,
            callback_typedef_use_external: false,
            external_crate_replacement: None,
            generate_tests: false,
        }
    }

    /// C++ header file
    pub fn cpp_header(standard: CppStandard) -> Self {
        Self {
            target_lang: TargetLang::CppHeader { standard },
            cabi_functions: CAbiFunctionMode::ExternalBindings {
                link_library: "azul".into(),
            },
            struct_mode: StructMode::Prefixed,
            trait_impl_mode: TraitImplMode::None,
            type_prefix: "Az".into(),
            module_wrapper: None,
            imports: vec![
                "#include <cstdint>".into(),
                "#include <cstddef>".into(),
                "#include <cstdbool>".into(),
            ],
            type_filter: None,
            type_exclude: BTreeSet::new(),
            indent: "    ".into(),
            generate_docs: true,
            callback_typedef_use_external: false,
            external_crate_replacement: None,
            generate_tests: false,
        }
    }

    /// Public Rust API (unprefixed, ergonomic)
    ///
    /// This generates a clean public API without Az prefixes.
    /// Note: This API re-exports from crate::dll, so it doesn't need
    /// duplicate GL type aliases (those come from dll module).
    pub fn rust_public_api() -> Self {
        // Skip GL type aliases as they're already in the dll module
        let mut type_exclude = BTreeSet::new();
        for gl_type in &[
            "GLuint",
            "GLint",
            "GLenum",
            "GLint64",
            "GLuint64",
            "GLsizei",
            "GLfloat",
            "GLboolean",
            "GLbitfield",
            "GLclampf",
            "GLsizeiptr",
            "GLintptr",
        ] {
            type_exclude.insert(gl_type.to_string());
        }

        Self {
            target_lang: TargetLang::Rust,
            cabi_functions: CAbiFunctionMode::None,
            struct_mode: StructMode::Unprefixed,
            trait_impl_mode: TraitImplMode::UsingDerive,
            type_prefix: "".into(),
            module_wrapper: None,
            imports: vec!["use core::ffi::c_void;".into()],
            type_filter: None,
            type_exclude,
            indent: "    ".into(),
            generate_docs: true,
            callback_typedef_use_external: false,
            external_crate_replacement: None,
            generate_tests: false,
        }
    }

    /// Memtest configuration (for testing the generated API)
    ///
    /// Similar to dll_static but with generate_tests: true.
    /// Generates #[test] functions for type size/layout verification.
    /// Included via include!() in dll/src/lib.rs.
    pub fn memtest() -> Self {
        Self {
            target_lang: TargetLang::Rust,
            cabi_functions: CAbiFunctionMode::InternalBindings { no_mangle: false },
            struct_mode: StructMode::Prefixed,
            trait_impl_mode: TraitImplMode::UsingTransmute {
                external_crate: "azul_core".into(),
            },
            type_prefix: "Az".into(),
            module_wrapper: Some("dll".into()),
            imports: vec![
                "use core::ffi::c_void;".into(),
                "use core::ffi::c_int;".into(),
                "use core::mem::transmute;".into(),
            ],
            type_filter: None,
            type_exclude: BTreeSet::new(),
            indent: "    ".into(),
            generate_docs: false, // Skip docs for test code
            callback_typedef_use_external: false,
            // Replace azul_dll:: with crate:: since included in azul-dll
            external_crate_replacement: Some(("azul_dll::".into(), "crate::".into())),
            generate_tests: true, // Generate #[test] functions!
        }
    }
}

// ============================================================================
// Python-specific Configuration
// ============================================================================

/// Extended configuration for Python code generation
///
/// Python requires additional configuration beyond the base CodegenConfig
/// because it has unique requirements:
/// - PyO3 attributes (#[pyclass], #[pymethods])
/// - Callback trampolines for Python→Rust calls
/// - Type exclusions (recursive types, VecRef types)
/// - Wrapper types for RefAny
///
/// **Important**: Python structs are generated SEPARATELY from C-API structs.
/// They do not share the same generated file.
#[derive(Debug, Clone)]
pub struct PythonConfig {
    /// Base code generation config
    pub base: CodegenConfig,

    /// Generate #[pyclass] attributes
    pub generate_pyclass: bool,

    /// Generate #[pymethods] impl blocks
    pub generate_pymethods: bool,

    /// Types to skip in Python (recursive types, etc.)
    pub skip_types: BTreeSet<String>,

    /// Types that need callback trampolines
    pub callback_types: BTreeSet<String>,

    /// Types that need VecRef→list conversion
    pub vecref_types: BTreeSet<String>,
}

impl Default for PythonConfig {
    fn default() -> Self {
        Self::python_extension()
    }
}

impl PythonConfig {
    /// Python extension structs
    ///
    /// Output: target/codegen/python_structs.rs (SEPARATE from C-API)
    ///
    /// These structs are used by the Python extension module.
    /// They have:
    /// - #[pyclass] attributes
    /// - Clone/Drop via transmute (no C-ABI calls)
    /// - Python-specific type filtering
    pub fn python_extension() -> Self {
        // Types that cause "infinite size" errors in PyO3
        let skip_types: BTreeSet<String> = [
            "XmlNode",
            "XmlNodeChild",
            "XmlNodeChildVec",
            "Xml",
            "ResultXmlXmlError",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        // VecRef types that need special handling
        let vecref_types: BTreeSet<String> = [
            "GLuintVecRef",
            "GLintVecRef",
            "GLenumVecRef",
            "U8VecRef",
            "U16VecRef",
            "U32VecRef",
            "I32VecRef",
            "F32VecRef",
            "Refstr",
            "RefstrVecRef",
            "TessellatedSvgNodeVecRef",
            "TessellatedColoredSvgNodeVecRef",
            "OptionU8VecRef",
            "OptionI16VecRef",
            "OptionI32VecRef",
            "OptionF32VecRef",
            "OptionFloatVecRef",
            "GLintVecRefMut",
            "GLint64VecRefMut",
            "GLbooleanVecRefMut",
            "GLfloatVecRefMut",
            "U8VecRefMut",
            "F32VecRefMut",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        Self {
            base: CodegenConfig {
                target_lang: TargetLang::Python,
                cabi_functions: CAbiFunctionMode::None, // Python calls Rust directly
                struct_mode: StructMode::Prefixed,
                trait_impl_mode: TraitImplMode::UsingTransmute {
                    external_crate: "azul_core".into(),
                },
                type_prefix: "Az".into(),
                module_wrapper: Some("python_api".into()),
                imports: vec![
                    "use pyo3::prelude::*;".into(),
                    "use core::ffi::c_void;".into(),
                    "use core::mem::transmute;".into(),
                ],
                type_filter: None,
                type_exclude: BTreeSet::new(),
                indent: "    ".into(),
                generate_docs: true,
                callback_typedef_use_external: true, // Use external callback types
                external_crate_replacement: None,
                generate_tests: false,
            },
            generate_pyclass: true,
            generate_pymethods: true,
            skip_types,
            callback_types: BTreeSet::new(), // Populated by IR builder
            vecref_types,
        }
    }
}
