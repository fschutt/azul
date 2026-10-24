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
mod cpp14;
mod cpp17;
mod cpp20;

use anyhow::Result;
pub use common::{generate_module_partition, *};
pub use cpp03::Cpp03Generator;
pub use cpp11::Cpp11Generator;
pub use cpp14::Cpp14Generator;
pub use cpp17::Cpp17Generator;
pub use cpp20::{Cpp20Generator, Cpp23Generator};

use super::{config::*, ir::*};

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
        if self.has_noexcept() {
            " noexcept"
        } else {
            ""
        }
    }

    /// Get [[nodiscard]] attribute (empty for pre-C++17)
    fn nodiscard_attr(&self) -> &'static str {
        if self.has_nodiscard() {
            "[[nodiscard]] "
        } else {
            ""
        }
    }

    /// Get memset function name (std::memset for C++11+, memset for C++03)
    fn memset_fn(&self) -> &'static str {
        if self.has_move_semantics() {
            "std::memset"
        } else {
            "memset"
        }
    }

    /// Get strlen function name (std::strlen for C++11+, strlen for C++03)
    fn strlen_fn(&self) -> &'static str {
        if self.has_move_semantics() {
            "std::strlen"
        } else {
            "strlen"
        }
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
    fn generate_destructor(
        &self,
        code: &mut String,
        class_name: &str,
        c_type_name: &str,
        needs_destructor: bool,
    );

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
    fn generate_vec_methods(
        &self,
        code: &mut String,
        struct_def: &StructDef,
        config: &CodegenConfig,
    );

    /// Generate String-specific methods (c_str, length, std::string interop)
    fn generate_string_methods(
        &self,
        code: &mut String,
        struct_def: &StructDef,
        config: &CodegenConfig,
    );

    /// Generate Option-specific methods (isSome, isNone, unwrap, toStdOptional).
    /// `ir` is needed to look up the sibling enum's `Some` payload type when
    /// the simple prefix-strip is wrong (e.g. `OptionU32` → `u32`, not `U32`).
    fn generate_option_methods(
        &self,
        code: &mut String,
        struct_def: &StructDef,
        ir: &CodegenIR,
        config: &CodegenConfig,
    );

    /// Generate Result-specific methods (isOk, isErr, unwrap, toStdExpected).
    /// `ir` is needed for the sibling enum's `Ok`/`Err` payload types.
    fn generate_result_methods(
        &self,
        code: &mut String,
        struct_def: &StructDef,
        ir: &CodegenIR,
        config: &CodegenConfig,
    );
}

// ============================================================================
// Dispatcher
// ============================================================================

/// Get the appropriate generator for a C++ standard
pub fn get_generator(standard: CppStandard) -> Box<dyn CppDialect> {
    match standard {
        CppStandard::Cpp03 => Box::new(Cpp03Generator),
        CppStandard::Cpp11 => Box::new(Cpp11Generator),
        CppStandard::Cpp14 => Box::new(Cpp14Generator),
        CppStandard::Cpp17 => Box::new(Cpp17Generator),
        CppStandard::Cpp20 => Box::new(Cpp20Generator),
        CppStandard::Cpp23 => Box::new(Cpp23Generator),
    }
}

/// Generate C++ header for a specific standard
pub fn generate_cpp_header(
    ir: &CodegenIR,
    config: &CodegenConfig,
    standard: CppStandard,
) -> Result<String> {
    let generator = get_generator(standard);
    generator.generate(ir, config)
}

/// Generate all C++ headers (all standards). C++20+ also yields a sibling
/// `azul.cppm` module-partition file (one shared file across cpp20/cpp23,
/// since the contents are identical re-exports).
pub fn generate_all_cpp_headers(
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<Vec<(String, String)>> {
    let mut results = Vec::new();
    let mut module_emitted = false;

    for &standard in CppStandard::all() {
        let filename = standard.header_filename();
        let code = generate_cpp_header(ir, config, standard)?;
        results.push((filename, code));

        if standard >= CppStandard::Cpp20 && !module_emitted {
            results.push((
                "azul.cppm".to_string(),
                generate_module_partition(ir, config, standard),
            ));
            module_emitted = true;
        }
    }

    Ok(results)
}

// ============================================================================
// Field accessors (the field-access wave, 2026-10-05)
// ============================================================================

#[cfg(test)]
mod field_access_tests {
    use super::super::config::CppStandard;

    /// The real header for `standard`, CR-LF folded to LF so the expected
    /// snippets below read naturally.
    fn header(standard: CppStandard) -> String {
        let api = crate::api::ApiData::from_str(include_str!("../../../../../api.json"))
            .expect("api.json parses");
        super::super::generate_cpp_header(&api, standard)
            .expect("header generates")
            .replace("\r\n", "\n")
    }

    /// The text of `class <name> { ... };`.
    fn class_body<'a>(h: &'a str, name: &str) -> &'a str {
        let start = h
            .find(&format!("\nclass {} {{\n", name))
            .unwrap_or_else(|| panic!("no class {name}"));
        let end = start + h[start..].find("\n};\n").expect("unterminated class");
        &h[start..end]
    }

    #[test]
    fn a_cpp11_getter_deep_copies_and_a_setter_frees_the_old_value_then_takes_the_new_one() {
        let h = header(CppStandard::Cpp11);

        let fws = class_body(&h, "FullWindowState");
        assert!(fws.contains("    std::string get_title() const;\n"), "{fws}");
        assert!(fws.contains("    void set_title(String value) &;\n"), "{fws}");
        assert!(fws.contains("    WindowSize get_size() const;\n"), "{fws}");
        assert!(fws.contains("    void set_size(WindowSize value) &;\n"), "{fws}");

        // String field: decoded into a std::string, the field is not consumed.
        assert!(h.contains(
            "inline std::string FullWindowState::get_title() const {\n    return \
             inner_.title.vec.len ? std::string(reinterpret_cast<const \
             char*>(inner_.title.vec.ptr), inner_.title.vec.len) : std::string();\n}\n"
        ));
        // Setter: the old AzString is freed, the new one is moved in.
        assert!(h.contains(
            "inline void FullWindowState::set_title(String value) & {\n    \
             AzString_delete(&inner_.title);\n    inner_.title = value.release();\n}\n"
        ));
        // A Copy field: a plain copy, no _delete.
        assert!(h.contains(
            "inline WindowSize FullWindowState::get_size() const {\n    return \
             WindowSize(inner_.size);\n}\n"
        ));
        assert!(h.contains(
            "inline void FullWindowState::set_size(WindowSize value) & {\n    inner_.size = \
             value.release();\n}\n"
        ));

        // A heap-owning wrapper field: deep copy through _clone, delete-then-move.
        assert!(h.contains(
            "inline FullWindowState WindowCreateOptions::get_window_state() const {\n    return \
             FullWindowState(AzFullWindowState_clone(&inner_.window_state));\n}\n"
        ));
        assert!(h.contains(
            "inline void WindowCreateOptions::set_window_state(FullWindowState value) & {\n    \
             AzFullWindowState_delete(&inner_.window_state);\n    inner_.window_state = \
             value.release();\n}\n"
        ));

        // bool.
        assert!(h.contains(
            "inline bool CheckBoxState::get_checked() const {\n    return inner_.checked;\n}\n"
        ));
        assert!(h.contains(
            "inline void CheckBoxState::set_checked(bool value) & {\n    inner_.checked = \
             value;\n}\n"
        ));
    }

    /// `TextInputState::get_text()` is an api.json method: it keeps the name,
    /// but the `text` field must still be writable.
    #[test]
    fn an_api_method_wins_the_getter_name_but_the_field_keeps_its_setter() {
        let h = header(CppStandard::Cpp11);
        let tis = class_body(&h, "TextInputState");
        assert_eq!(tis.matches(" get_text() const;\n").count(), 1, "{tis}");
        assert!(!h.contains("inline U32Vec TextInputState::get_text() const"));
        assert!(h.contains(
            "inline void TextInputState::set_text(U32Vec value) & {\n    \
             AzU32Vec_delete(&inner_.text);\n    inner_.text = value.release();\n}\n"
        ));
    }

    /// Callback, callback-wrapper and RefAny fields are wired up by the
    /// callback plumbing, never by a field setter.
    #[test]
    fn callback_and_refany_fields_get_no_accessors() {
        let h = header(CppStandard::Cpp11);
        let fws = class_body(&h, "FullWindowState");
        assert!(!fws.contains("set_layout_callback("), "{fws}");
        let wco = class_body(&h, "WindowCreateOptions");
        assert!(!wco.contains("set_create_callback("), "{wco}");
    }

    #[test]
    fn cpp17_and_later_getters_are_nodiscard_and_setters_are_lvalue_only() {
        for standard in [CppStandard::Cpp17, CppStandard::Cpp20, CppStandard::Cpp23] {
            let h = header(standard);
            let fws = class_body(&h, "FullWindowState");
            assert!(
                fws.contains("    [[nodiscard]] std::string get_title() const;\n"),
                "{standard:?}"
            );
            assert!(fws.contains("    void set_title(String value) &;\n"), "{standard:?}");
            assert!(
                h.contains(
                    "inline void WindowCreateOptions::set_window_state(FullWindowState value) & \
                     {\n    AzFullWindowState_delete(&inner_.window_state);\n    \
                     inner_.window_state = value.release();\n}\n"
                ),
                "{standard:?}"
            );
        }
    }

    /// C++03 has no `std::string` include, no ref-qualifiers and no moves:
    /// getters return a `const` copy (so `get_x().set_y(..)` does not
    /// compile instead of silently writing a temporary), strings stay
    /// `String`, and a `const char*` overload stands in for the implicit
    /// conversion C++03's `explicit String(const char*)` does not offer.
    #[test]
    fn cpp03_getters_return_const_copies_and_string_setters_take_a_c_string() {
        let h = header(CppStandard::Cpp03);
        let fws = class_body(&h, "FullWindowState");
        assert!(fws.contains("    const String get_title() const;\n"), "{fws}");
        assert!(fws.contains("    void set_title(String value);\n"), "{fws}");
        assert!(fws.contains("    void set_title(const char* value);\n"), "{fws}");
        assert!(h.contains(
            "inline const String FullWindowState::get_title() const {\n    String::Proxy \
             _p(AzString_clone(&inner_.title));\n    return _p;\n}\n"
        ));
        assert!(h.contains(
            "inline void FullWindowState::set_title(String value) {\n    \
             AzString_delete(&inner_.title);\n    inner_.title = value.release();\n}\n"
        ));
        assert!(h.contains(
            "inline void FullWindowState::set_title(const char* value) {\n    \
             set_title(String(value));\n}\n"
        ));
        assert!(h.contains(
            "inline const FullWindowState WindowCreateOptions::get_window_state() const {\n    \
             FullWindowState::Proxy _p(AzFullWindowState_clone(&inner_.window_state));\n    \
             return _p;\n}\n"
        ));
    }

    /// Getters return copies, so a nested write is read-modify-write; the
    /// header says so where every reader starts.
    #[test]
    fn the_header_documents_read_modify_write_for_nested_fields() {
        let h = header(CppStandard::Cpp20);
        assert!(h.contains("// FIELDS\n"));
        assert!(h.contains("ws.set_title(\"My App\");"));
        assert!(h.contains("opts.set_window_state(std::move(ws));"));
    }
}
