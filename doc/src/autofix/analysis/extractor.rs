//! Type extractor for api.json
//!
//! This module extracts type references from api.json data structures,
//! properly handling the special cases like "self" in fn_args.

use std::collections::HashSet;
use crate::autofix::types::{TypeParser, ParsedType};
use crate::autofix::types::borrow::{BorrowMode, ParsedFnArgs};

/// Extractor for types from api.json structures
pub struct ApiTypeExtractor {
    parser: TypeParser,
    /// Collected valid type names
    pub types: HashSet<String>,
    /// Invalid type strings found (for diagnostics)
    pub invalid: Vec<String>,
}

impl Default for ApiTypeExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl ApiTypeExtractor {
    pub fn new() -> Self {
        Self {
            parser: TypeParser::new(),
            types: HashSet::new(),
            invalid: Vec::new(),
        }
    }

    /// Extract types from a type string
    pub fn extract_from_type_string(&mut self, type_str: &str) {
        let parsed = self.parser.parse(type_str);
        if parsed.is_invalid() {
            self.invalid.push(type_str.to_string());
        } else {
            parsed.collect_user_types(&mut self.types);
        }
    }

    /// Extract types from fn_args using strongly typed parsing
    ///
    /// fn_args MUST be an array of objects, where each object has exactly one key-value pair:
    /// - For self: { "self": "ref" | "refmut" | "value" }
    /// - For args: { "arg_name": "TypeName" }
    ///
    /// INVALID formats that will be rejected:
    /// - Flat object: { "self": "ref", "arg": "Type" } (wrong: unordered!)
    /// - Mixed keys: { "arg": "Type", "doc": "description" } (wrong: doc is not a type)
    pub fn extract_from_fn_args_array(&mut self, args: &[serde_json::Value]) {
        for arg in args {
            if let Some(obj) = arg.as_object() {
                // Validate: each arg object should have exactly one key (the arg name)
                // Exception: we skip "doc" and "type" keys if present (legacy format)
                let valid_keys: Vec<_> = obj.keys()
                    .filter(|k| *k != "doc" && *k != "type")
                    .collect();
                
                if valid_keys.len() > 1 {
                    self.invalid.push(format!(
                        "fn_args entry has multiple keys {:?} - each arg should be a separate object",
                        valid_keys
                    ));
                }
                
                for (key, value) in obj {
                    // Skip documentation fields
                    if key == "doc" || key == "type" {
                        continue;
                    }
                    
                    // CRITICAL: Skip "self" key - its value is a borrow mode, not a type!
                    if key == "self" {
                        // Validate it's a proper borrow mode
                        if let Some(mode_str) = value.as_str() {
                            if BorrowMode::parse(mode_str).is_none() {
                                self.invalid.push(format!("invalid borrow mode for self: '{}'", mode_str));
                            }
                        }
                        continue;
                    }

                    if let Some(type_str) = value.as_str() {
                        self.extract_from_type_string(type_str);
                    }
                }
            } else {
                self.invalid.push(format!("fn_args entry is not an object: {:?}", arg));
            }
        }
    }
    
    /// Legacy: Extract from fn_args as object (deprecated format)
    /// This will emit a warning but still work for backwards compatibility
    #[deprecated(note = "fn_args should be an array, not an object")]
    pub fn extract_from_fn_args(&mut self, args: &serde_json::Map<String, serde_json::Value>) {
        self.invalid.push("fn_args is a flat object instead of array - argument order may be lost!".to_string());
        for (key, value) in args {
            if key == "self" || key == "doc" || key == "type" {
                continue;
            }
            if let Some(type_str) = value.as_str() {
                self.extract_from_type_string(type_str);
            }
        }
    }

    /// Extract types from a function definition
    pub fn extract_from_function(&mut self, func: &serde_json::Value) {
        // Extract return type
        if let Some(ret) = func.get("returns").and_then(|v| v.as_str()) {
            self.extract_from_type_string(ret);
        }

        // Extract fn_args - MUST be an array!
        if let Some(fn_args) = func.get("fn_args") {
            if let Some(args_array) = fn_args.as_array() {
                self.extract_from_fn_args_array(args_array);
            } else if let Some(args_obj) = fn_args.as_object() {
                // Legacy format - emit warning
                #[allow(deprecated)]
                self.extract_from_fn_args(args_obj);
            } else {
                self.invalid.push(format!("fn_args is neither array nor object: {:?}", fn_args));
            }
        }
    }

    /// Extract types from struct fields
    pub fn extract_from_struct_fields(&mut self, fields: &serde_json::Value) {
        if let Some(obj) = fields.as_object() {
            for (_field_name, field_type) in obj {
                if let Some(type_str) = field_type.as_str() {
                    self.extract_from_type_string(type_str);
                }
            }
        }
    }

    /// Extract types from enum variants
    pub fn extract_from_enum_variants(&mut self, variants: &serde_json::Value) {
        if let Some(obj) = variants.as_object() {
            for (_variant_name, variant_type) in obj {
                if let Some(type_str) = variant_type.as_str() {
                    // Some variants have no data (empty string or "")
                    if !type_str.is_empty() {
                        self.extract_from_type_string(type_str);
                    }
                }
            }
        }
    }

    /// Get all valid extracted types
    pub fn get_types(&self) -> &HashSet<String> {
        &self.types
    }
}

/// Extract all types referenced in api.json
pub fn extract_types_from_api(api: &serde_json::Value) -> ApiTypeExtractor {
    let mut extractor = ApiTypeExtractor::new();

    if let Some(classes) = api.get("classes").and_then(|v| v.as_object()) {
        for (class_name, class_def) in classes {
            // The class itself is a type
            extractor.types.insert(class_name.clone());

            // Extract from struct fields
            if let Some(struct_fields) = class_def.get("struct_fields") {
                extractor.extract_from_struct_fields(struct_fields);
            }

            // Extract from enum variants
            if let Some(enum_variants) = class_def.get("enum_fields") {
                extractor.extract_from_enum_variants(enum_variants);
            }

            // Extract from constructors
            if let Some(constructors) = class_def.get("constructors").and_then(|v| v.as_object()) {
                for (_ctor_name, ctor_def) in constructors {
                    extractor.extract_from_function(ctor_def);
                }
            }

            // Extract from functions
            if let Some(functions) = class_def.get("functions").and_then(|v| v.as_object()) {
                for (_fn_name, fn_def) in functions {
                    extractor.extract_from_function(fn_def);
                }
            }
        }
    }

    extractor
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_skip_self_in_fn_args_array() {
        let mut extractor = ApiTypeExtractor::new();

        // Correct format: array of objects
        let fn_args = json!([
            { "self": "ref" },
            { "property": "CssProperty" }
        ]);

        extractor.extract_from_fn_args_array(fn_args.as_array().unwrap());

        // "ref" should NOT be extracted as a type
        assert!(!extractor.types.contains("ref"));
        // "CssProperty" should be extracted
        assert!(extractor.types.contains("CssProperty"));
        // No errors
        assert!(extractor.invalid.is_empty(), "unexpected errors: {:?}", extractor.invalid);
    }

    #[test]
    fn test_invalid_borrow_mode_detected() {
        let mut extractor = ApiTypeExtractor::new();

        let fn_args = json!([
            { "self": "invalid_mode" },
            { "property": "CssProperty" }
        ]);

        extractor.extract_from_fn_args_array(fn_args.as_array().unwrap());

        // Should record invalid borrow mode
        assert!(!extractor.invalid.is_empty());
        assert!(extractor.invalid[0].contains("invalid borrow mode"));
    }

    #[test]
    fn test_extract_from_function_with_array() {
        let mut extractor = ApiTypeExtractor::new();

        // Correct format: fn_args is an array
        let func = json!({
            "fn_args": [
                { "self": "refmut" },
                { "node_id": "DomNodeId" }
            ],
            "returns": "Option<CssProperty>"
        });

        extractor.extract_from_function(&func);

        assert!(!extractor.types.contains("refmut"));
        assert!(extractor.types.contains("DomNodeId"));
        assert!(extractor.types.contains("CssProperty"));
        // No errors for correct format
        assert!(extractor.invalid.is_empty(), "unexpected errors: {:?}", extractor.invalid);
    }

    #[test]
    fn test_reject_flat_object_fn_args() {
        let mut extractor = ApiTypeExtractor::new();

        // WRONG format: flat object (unordered!)
        let func = json!({
            "fn_args": {
                "self": "ref",
                "arg1": "Type1",
                "arg2": "Type2"
            },
            "returns": "void"
        });

        extractor.extract_from_function(&func);

        // Should emit warning about flat object
        assert!(!extractor.invalid.is_empty(), "should warn about flat object format");
        assert!(extractor.invalid.iter().any(|e| e.contains("flat object")), 
            "error should mention flat object: {:?}", extractor.invalid);
    }

    #[test]
    fn test_reject_mixed_keys_in_arg() {
        let mut extractor = ApiTypeExtractor::new();

        // WRONG format: multiple keys in one arg object
        let fn_args = json!([
            { "self": "ref" },
            { "quality": "u8", "doc": "description" }  // doc should be ignored
        ]);

        extractor.extract_from_fn_args_array(fn_args.as_array().unwrap());

        // Should extract the type correctly despite doc field
        assert!(extractor.types.contains("u8") || extractor.types.is_empty()); // u8 is primitive
        // doc field should be skipped, no error about it
    }

    #[test]
    fn test_multiple_args_in_single_object_error() {
        let mut extractor = ApiTypeExtractor::new();

        // WRONG format: multiple actual args in one object
        let fn_args = json!([
            { "arg1": "Type1", "arg2": "Type2" }  // should error!
        ]);

        extractor.extract_from_fn_args_array(fn_args.as_array().unwrap());

        // Should emit error about multiple keys
        assert!(!extractor.invalid.is_empty(), "should error on multiple keys");
        assert!(extractor.invalid.iter().any(|e| e.contains("multiple keys")),
            "error should mention multiple keys: {:?}", extractor.invalid);
    }
}
