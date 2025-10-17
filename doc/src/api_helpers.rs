/// Helper functions to traverse complex API structures and extract type references
///
/// The API structures are deeply nested with Vec<IndexMap<>>, Option<>, etc.
/// These helpers make it easy to extract all type references for recursive discovery.
use std::collections::HashSet;

#[allow(unused_imports)]
use indexmap::IndexMap;

use crate::api::{ClassData, EnumVariantData, FieldData, FunctionData, ReturnTypeData};

/// Extract all type references from a ClassData
pub fn extract_types_from_class_data(class_data: &ClassData) -> HashSet<String> {
    let mut types = HashSet::new();

    // Extract from struct fields
    if let Some(struct_fields) = &class_data.struct_fields {
        for field_map in struct_fields {
            for (_field_name, field_data) in field_map {
                types.extend(extract_types_from_field_data(field_data));
            }
        }
    }

    // Extract from enum variants
    if let Some(enum_fields) = &class_data.enum_fields {
        for variant_map in enum_fields {
            for (_variant_name, variant_data) in variant_map {
                types.extend(extract_types_from_enum_variant(variant_data));
            }
        }
    }

    // Extract from functions
    if let Some(functions) = &class_data.functions {
        for (_fn_name, fn_data) in functions {
            types.extend(extract_types_from_function_data(fn_data));
        }
    }

    types
}

/// Extract type from FieldData
/// Skips types behind pointers (they don't need to be in the API)
pub fn extract_types_from_field_data(field_data: &FieldData) -> HashSet<String> {
    let mut types = HashSet::new();

    // Skip types behind pointers - they're opaque and don't need to be exposed
    if let Some(base_type) = extract_base_type_if_not_opaque(&field_data.r#type) {
        types.insert(base_type);
    }

    types
}

/// Extract types from EnumVariantData
/// Skips types behind pointers
pub fn extract_types_from_enum_variant(variant_data: &EnumVariantData) -> HashSet<String> {
    let mut types = HashSet::new();

    if let Some(variant_type) = &variant_data.r#type {
        if let Some(base_type) = extract_base_type_if_not_opaque(variant_type) {
            types.insert(base_type);
        }
    }

    types
}

/// Extract types from FunctionData
/// Skips types behind pointers
pub fn extract_types_from_function_data(fn_data: &FunctionData) -> HashSet<String> {
    let mut types = HashSet::new();

    // Extract return type
    if let Some(return_data) = &fn_data.returns {
        types.extend(extract_types_from_return_data(return_data));
    }

    // Extract parameter types
    // fn_args is Vec<IndexMap<String, String>> where key=name, value=type
    for arg_map in &fn_data.fn_args {
        for (_param_name, param_type) in arg_map {
            if let Some(base_type) = extract_base_type_if_not_opaque(param_type) {
                types.insert(base_type);
            }
        }
    }

    types
}

/// Extract type from ReturnTypeData
/// Skips types behind pointers
pub fn extract_types_from_return_data(return_data: &ReturnTypeData) -> HashSet<String> {
    let mut types = HashSet::new();

    if let Some(base_type) = extract_base_type_if_not_opaque(&return_data.r#type) {
        types.insert(base_type);
    }

    types
}

/// Extract base type from a type string (removes Vec, Option, Box, etc.)
///
/// Examples:
/// - "Vec<Foo>" -> "Foo"
/// - "Option<Bar>" -> "Bar"
/// - "*const Baz" -> "Baz"
/// - "&mut Qux" -> "Qux"
pub fn extract_base_type(type_str: &str) -> String {
    let trimmed = type_str.trim();

    // Handle generic types like Vec<T>, Option<T>, Box<T>, etc.
    if let Some(start) = trimmed.find('<') {
        if let Some(end) = trimmed.rfind('>') {
            let inner = &trimmed[start + 1..end];
            // Recursively extract from inner type
            return extract_base_type(inner);
        }
    }

    // Handle pointer types
    if let Some(rest) = trimmed.strip_prefix("*const ") {
        return extract_base_type(rest);
    }
    if let Some(rest) = trimmed.strip_prefix("*mut ") {
        return extract_base_type(rest);
    }

    // Handle reference types
    if let Some(rest) = trimmed.strip_prefix("&mut ") {
        return extract_base_type(rest);
    }
    if let Some(rest) = trimmed.strip_prefix('&') {
        return extract_base_type(rest);
    }

    // Handle tuple types - extract first element
    if trimmed.starts_with('(') && trimmed.ends_with(')') {
        let inner = &trimmed[1..trimmed.len() - 1];
        if let Some(comma_pos) = inner.find(',') {
            return extract_base_type(&inner[..comma_pos]);
        }
        return extract_base_type(inner);
    }

    trimmed.to_string()
}

/// Check if a type is behind a pointer or smart pointer wrapper
/// These types don't need to be exposed in the API because they're opaque
pub fn is_behind_pointer(type_str: &str) -> bool {
    let trimmed = type_str.trim();

    // Raw pointers
    if trimmed.starts_with("*const ") || trimmed.starts_with("*mut ") {
        return true;
    }

    // References (usually opaque in FFI)
    if trimmed.starts_with("&") {
        return true;
    }

    // Smart pointers that make types opaque
    let opaque_wrappers = [
        "Box<", "Arc<", "Rc<", "Weak<", "Mutex<", "RwLock<", "RefCell<", "Cell<",
    ];

    for wrapper in &opaque_wrappers {
        if trimmed.starts_with(wrapper) {
            return true;
        }
    }

    false
}

/// Extract base type from a type string (removes Vec, Option, Box, etc.)
/// BUT: If the type is behind a pointer/smart pointer, return empty string
/// to signal that this type should not be recursively followed
pub fn extract_base_type_if_not_opaque(type_str: &str) -> Option<String> {
    if is_behind_pointer(type_str) {
        return None; // Don't follow types behind pointers
    }

    Some(extract_base_type(type_str))
}

/// Collect all type references from the entire API
pub fn collect_all_referenced_types_from_api(api_data: &crate::api::ApiData) -> HashSet<String> {
    let mut types = HashSet::new();

    // Include callback_typedefs - they can be referenced and need patches
    // (e.g. FooDestructorType is referenced from FooDestructor enum)
    for (_version_name, version_data) in &api_data.0 {
        for (_module_name, module_data) in &version_data.api {
            for (_class_name, class_data) in &module_data.classes {
                types.extend(extract_types_from_class_data(class_data));
            }
        }
    }

    types
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_base_type_simple() {
        assert_eq!(extract_base_type("Foo"), "Foo");
        assert_eq!(extract_base_type("  Bar  "), "Bar");
    }

    #[test]
    fn test_extract_base_type_generic() {
        assert_eq!(extract_base_type("Vec<Foo>"), "Foo");
        assert_eq!(extract_base_type("Option<Bar>"), "Bar");
        assert_eq!(extract_base_type("Box<Baz>"), "Baz");
    }

    #[test]
    fn test_extract_base_type_nested() {
        assert_eq!(extract_base_type("Vec<Option<Foo>>"), "Foo");
        assert_eq!(extract_base_type("Option<Box<Bar>>"), "Bar");
    }

    #[test]
    fn test_extract_base_type_pointers() {
        assert_eq!(extract_base_type("*const Foo"), "Foo");
        assert_eq!(extract_base_type("*mut Bar"), "Bar");
        assert_eq!(extract_base_type("&Baz"), "Baz");
        assert_eq!(extract_base_type("&mut Qux"), "Qux");
    }

    #[test]
    fn test_extract_base_type_complex() {
        assert_eq!(extract_base_type("*const Vec<Foo>"), "Foo");
        assert_eq!(extract_base_type("&Option<Bar>"), "Bar");
    }
}
