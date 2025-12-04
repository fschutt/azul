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
    /// This uses `ParsedFnArgs` which properly separates "self" (a borrow mode)
    /// from actual type arguments.
    pub fn extract_from_fn_args(&mut self, args: &serde_json::Map<String, serde_json::Value>) {
        for (key, value) in args {
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
    }

    /// Extract types from a function definition
    pub fn extract_from_function(&mut self, func: &serde_json::Value) {
        // Extract return type
        if let Some(ret) = func.get("returns").and_then(|v| v.as_str()) {
            self.extract_from_type_string(ret);
        }

        // Extract fn_args
        if let Some(args) = func.get("fn_args").and_then(|v| v.as_object()) {
            self.extract_from_fn_args(args);
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
    fn test_skip_self_in_fn_args() {
        let mut extractor = ApiTypeExtractor::new();

        let fn_args = json!({
            "self": "ref",
            "property": "CssProperty"
        });

        extractor.extract_from_fn_args(fn_args.as_object().unwrap());

        // "ref" should NOT be extracted as a type
        assert!(!extractor.types.contains("ref"));
        // "CssProperty" should be extracted
        assert!(extractor.types.contains("CssProperty"));
    }

    #[test]
    fn test_invalid_borrow_mode_detected() {
        let mut extractor = ApiTypeExtractor::new();

        let fn_args = json!({
            "self": "invalid_mode",
            "property": "CssProperty"
        });

        extractor.extract_from_fn_args(fn_args.as_object().unwrap());

        // Should record invalid borrow mode
        assert!(!extractor.invalid.is_empty());
        assert!(extractor.invalid[0].contains("invalid borrow mode"));
    }

    #[test]
    fn test_extract_from_function() {
        let mut extractor = ApiTypeExtractor::new();

        let func = json!({
            "fn_args": {
                "self": "refmut",
                "node_id": "DomNodeId"
            },
            "returns": "Option<CssProperty>"
        });

        extractor.extract_from_function(&func);

        assert!(!extractor.types.contains("refmut"));
        assert!(extractor.types.contains("DomNodeId"));
        assert!(extractor.types.contains("CssProperty"));
    }
}
