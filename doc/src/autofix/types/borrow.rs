//! Strongly typed borrow modes and function arguments
//!
//! This module provides type-safe representations for:
//! - Borrow modes ("ref", "refmut", "value")
//! - Self parameters
//! - Function arguments

use std::fmt;

use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};

/// Borrow mode for self parameters and callback arguments
///
/// In api.json, `"self": "ref"` means `&self`, not a type called "ref".
/// This enum ensures type-safe parsing and prevents treating borrow modes as types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BorrowMode {
    /// `&self` / `&T` - immutable reference
    Ref,
    /// `&mut self` / `&mut T` - mutable reference  
    RefMut,
    /// `self` / `T` - ownership transfer (by value)
    Value,
}

impl BorrowMode {
    /// Parse from api.json string
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "ref" => Some(BorrowMode::Ref),
            "refmut" => Some(BorrowMode::RefMut),
            "value" => Some(BorrowMode::Value),
            _ => None,
        }
    }

    /// Check if a string is a borrow mode keyword
    pub fn is_borrow_keyword(s: &str) -> bool {
        Self::parse(s).is_some()
    }

    /// Convert to JSON string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            BorrowMode::Ref => "ref",
            BorrowMode::RefMut => "refmut",
            BorrowMode::Value => "value",
        }
    }

    /// Convert to Rust syntax
    pub fn to_rust_self(&self) -> &'static str {
        match self {
            BorrowMode::Ref => "&self",
            BorrowMode::RefMut => "&mut self",
            BorrowMode::Value => "self",
        }
    }

    /// Convert to Rust reference prefix for a type
    pub fn to_rust_ref_prefix(&self) -> &'static str {
        match self {
            BorrowMode::Ref => "&",
            BorrowMode::RefMut => "&mut ",
            BorrowMode::Value => "",
        }
    }

    /// Convert to C-compatible representation
    pub fn to_c_pointer(&self, type_name: &str) -> String {
        match self {
            BorrowMode::Ref => format!("const {}*", type_name),
            BorrowMode::RefMut => format!("{}*", type_name),
            BorrowMode::Value => type_name.to_string(),
        }
    }
}

impl Default for BorrowMode {
    fn default() -> Self {
        BorrowMode::Value
    }
}

impl fmt::Display for BorrowMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for BorrowMode {
    type Err = ParseBorrowModeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or(ParseBorrowModeError(s.to_string()))
    }
}

// Serde serialization - writes as string "ref", "refmut", or "value"
impl Serialize for BorrowMode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

// Serde deserialization - fails immediately on invalid values
impl<'de> Deserialize<'de> for BorrowMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct BorrowModeVisitor;

        impl<'de> Visitor<'de> for BorrowModeVisitor {
            type Value = BorrowMode;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("one of: \"ref\", \"refmut\", \"value\"")
            }

            fn visit_str<E>(self, value: &str) -> Result<BorrowMode, E>
            where
                E: de::Error,
            {
                BorrowMode::parse(value).ok_or_else(|| {
                    de::Error::custom(format!(
                        "invalid borrow mode '{}', expected one of: ref, refmut, value",
                        value
                    ))
                })
            }
        }

        deserializer.deserialize_str(BorrowModeVisitor)
    }
}

#[derive(Debug, Clone)]
pub struct ParseBorrowModeError(pub String);

impl fmt::Display for ParseBorrowModeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid borrow mode: '{}' (expected 'ref', 'refmut', or 'value')",
            self.0
        )
    }
}

impl std::error::Error for ParseBorrowModeError {}

/// Self parameter in a function
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfParam {
    #[serde(rename = "self")]
    pub borrow_mode: BorrowMode,
}

impl SelfParam {
    pub fn new(mode: BorrowMode) -> Self {
        Self { borrow_mode: mode }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        BorrowMode::parse(s).map(|mode| Self { borrow_mode: mode })
    }
}

/// A function argument (not self)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnArg {
    /// Parameter name
    pub name: String,
    /// Type string (e.g., "CssProperty", "Option<DomNodeId>")
    pub type_str: String,
}

impl FnArg {
    pub fn new(name: impl Into<String>, type_str: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            type_str: type_str.into(),
        }
    }
}

/// Parsed function arguments from api.json
///
/// This properly separates "self" from actual arguments.
#[derive(Debug, Clone, Default)]
pub struct ParsedFnArgs {
    /// Self parameter, if present
    pub self_param: Option<SelfParam>,
    /// Regular arguments (name -> type)
    pub args: Vec<FnArg>,
}

impl ParsedFnArgs {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse from api.json fn_args structure
    ///
    /// The api.json format is: `[{"self": "ref", "property": "CssProperty"}]`
    pub fn parse_from_json(fn_args: &[indexmap::IndexMap<String, String>]) -> Self {
        let mut result = ParsedFnArgs::new();

        for arg_map in fn_args {
            for (key, value) in arg_map {
                if key == "self" {
                    // This is a borrow mode, not a type!
                    if let Some(self_param) = SelfParam::from_str(value) {
                        result.self_param = Some(self_param);
                    }
                } else {
                    // Regular argument: key is name, value is type
                    result.args.push(FnArg::new(key.clone(), value.clone()));
                }
            }
        }

        result
    }

    /// Get all type strings referenced (excludes self borrow mode)
    pub fn get_type_strings(&self) -> impl Iterator<Item = &str> {
        self.args.iter().map(|arg| arg.type_str.as_str())
    }

    /// Check if function takes self
    pub fn has_self(&self) -> bool {
        self.self_param.is_some()
    }

    /// Check if function takes mutable self
    pub fn has_mut_self(&self) -> bool {
        self.self_param
            .as_ref()
            .map_or(false, |s| s.borrow_mode == BorrowMode::RefMut)
    }
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;

    use super::*;

    #[test]
    fn test_borrow_mode_parse() {
        assert_eq!(BorrowMode::parse("ref"), Some(BorrowMode::Ref));
        assert_eq!(BorrowMode::parse("refmut"), Some(BorrowMode::RefMut));
        assert_eq!(BorrowMode::parse("value"), Some(BorrowMode::Value));
        assert_eq!(BorrowMode::parse("CssProperty"), None);
    }

    #[test]
    fn test_borrow_mode_is_keyword() {
        assert!(BorrowMode::is_borrow_keyword("ref"));
        assert!(BorrowMode::is_borrow_keyword("refmut"));
        assert!(BorrowMode::is_borrow_keyword("value"));
        assert!(!BorrowMode::is_borrow_keyword("String"));
    }

    #[test]
    fn test_borrow_mode_serde() {
        // Test serialization
        let mode = BorrowMode::RefMut;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, "\"refmut\"");

        // Test deserialization
        let parsed: BorrowMode = serde_json::from_str("\"ref\"").unwrap();
        assert_eq!(parsed, BorrowMode::Ref);

        // Test invalid value fails
        let result: Result<BorrowMode, _> = serde_json::from_str("\"invalid\"");
        assert!(result.is_err());
    }

    #[test]
    fn test_parsed_fn_args() {
        let mut map1 = IndexMap::new();
        map1.insert("self".to_string(), "ref".to_string());
        map1.insert("property".to_string(), "CssProperty".to_string());

        let fn_args = vec![map1];
        let parsed = ParsedFnArgs::parse_from_json(&fn_args);

        assert!(parsed.self_param.is_some());
        assert_eq!(parsed.self_param.unwrap().borrow_mode, BorrowMode::Ref);
        assert_eq!(parsed.args.len(), 1);
        assert_eq!(parsed.args[0].name, "property");
        assert_eq!(parsed.args[0].type_str, "CssProperty");
    }

    #[test]
    fn test_get_type_strings_excludes_self() {
        let mut map1 = IndexMap::new();
        map1.insert("self".to_string(), "refmut".to_string());
        map1.insert("node_id".to_string(), "DomNodeId".to_string());

        let fn_args = vec![map1];
        let parsed = ParsedFnArgs::parse_from_json(&fn_args);

        let types: Vec<_> = parsed.get_type_strings().collect();
        assert_eq!(types, vec!["DomNodeId"]);
        // "refmut" should NOT be in the types
        assert!(!types.contains(&"refmut"));
    }
}
