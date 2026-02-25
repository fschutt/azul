//! JSON parsing module for C API
//!
//! Re-exports the data types and serde_json implementations from `azul_core::json`.
//! Adds RefAny serialization support on top.

// Re-export all data types and methods from core
pub use azul_core::json::*;

use alloc::string::String;
use azul_css::{AzString, impl_option, impl_option_inner};

// ============================================================================
// Public API Functions
// ============================================================================

/// Parse a JSON string
#[cfg(feature = "json")]
pub fn json_parse(s: &str) -> Result<Json, JsonParseError> {
    Json::parse(s)
}

/// Parse JSON from bytes
#[cfg(feature = "json")]
pub fn json_parse_bytes(bytes: &[u8]) -> Result<Json, JsonParseError> {
    Json::parse_bytes(bytes)
}

/// Serialize JSON to string
#[cfg(feature = "json")]
pub fn json_stringify(json: &Json) -> AzString {
    json.to_json_string()
}

/// Serialize JSON to pretty-printed string
#[cfg(feature = "json")]
pub fn json_stringify_pretty(json: &Json) -> AzString {
    json.to_string_pretty()
}

// ============================================================================
// RefAny JSON Serialization Support
// ============================================================================

use azul_core::refany::RefAny;

/// Result type for RefAny deserialization
#[derive(Debug, Clone)]
#[repr(C, u8)]
pub enum ResultRefAnyString {
    /// Successfully deserialized RefAny
    Ok(RefAny),
    /// Error message describing the failure
    Err(AzString),
}

impl_option!(ResultRefAnyString, OptionResultRefAnyString, copy = false, [Debug, Clone]);

impl ResultRefAnyString {
    pub fn is_ok(&self) -> bool {
        matches!(self, ResultRefAnyString::Ok(_))
    }

    pub fn is_err(&self) -> bool {
        matches!(self, ResultRefAnyString::Err(_))
    }

    pub fn ok(self) -> Option<RefAny> {
        match self {
            ResultRefAnyString::Ok(r) => Some(r),
            ResultRefAnyString::Err(_) => None,
        }
    }

    pub fn err(self) -> Option<AzString> {
        match self {
            ResultRefAnyString::Ok(_) => None,
            ResultRefAnyString::Err(e) => Some(e),
        }
    }
}

/// C-compatible function type for serializing a RefAny's contents to JSON.
pub type RefAnySerializeFnType = extern "C" fn(RefAny) -> Json;

/// C-compatible function type for deserializing JSON into a new RefAny.
pub type RefAnyDeserializeFnType = extern "C" fn(Json) -> ResultRefAnyString;

/// Serialize a RefAny to JSON using its registered serialize function.
#[cfg(feature = "json")]
pub fn serialize_refany_to_json(refany: &RefAny) -> Option<Json> {
    let serialize_fn = refany.get_serialize_fn();
    if serialize_fn == 0 {
        return None;
    }

    let func: RefAnySerializeFnType = unsafe {
        core::mem::transmute(serialize_fn)
    };
    let json = func(refany.clone());

    if json.is_null() {
        None
    } else {
        Some(json)
    }
}

/// Deserialize JSON into a RefAny using the provided deserialize function.
#[cfg(feature = "json")]
pub fn deserialize_refany_from_json(
    json: Json,
    deserialize_fn: usize
) -> Result<RefAny, String> {
    if deserialize_fn == 0 {
        return Err("Type does not support JSON deserialization".to_string());
    }

    let func: RefAnyDeserializeFnType = unsafe {
        core::mem::transmute(deserialize_fn)
    };

    match func(json) {
        ResultRefAnyString::Ok(refany) => Ok(refany),
        ResultRefAnyString::Err(msg) => Err(msg.as_str().to_string()),
    }
}

impl From<Result<RefAny, String>> for ResultRefAnyString {
    fn from(result: Result<RefAny, String>) -> Self {
        match result {
            Ok(refany) => ResultRefAnyString::Ok(refany),
            Err(msg) => ResultRefAnyString::Err(AzString::from(msg)),
        }
    }
}

impl ResultRefAnyString {
    pub fn ok_result(refany: RefAny) -> Self {
        ResultRefAnyString::Ok(refany)
    }

    pub fn err_result(message: AzString) -> Self {
        ResultRefAnyString::Err(message)
    }
}

/// Serialize a RefAny to JSON, returns OptionJson::None if not supported or fails.
#[cfg(feature = "json")]
pub fn refany_serialize_to_json(refany: &RefAny) -> OptionJson {
    match serialize_refany_to_json(refany) {
        Some(json) => OptionJson::Some(json),
        None => OptionJson::None,
    }
}

/// Deserialize JSON into a RefAny using the provided deserialize function.
#[cfg(feature = "json")]
pub fn json_deserialize_to_refany(json: Json, deserialize_fn: usize) -> ResultRefAnyString {
    deserialize_refany_from_json(json, deserialize_fn).into()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "json")]
    fn test_parse_null() {
        let json = Json::parse("null").unwrap();
        assert!(json.is_null());
    }

    #[test]
    #[cfg(feature = "json")]
    fn test_parse_bool() {
        let json_true = Json::parse("true").unwrap();
        assert_eq!(json_true.as_bool(), Some(true));

        let json_false = Json::parse("false").unwrap();
        assert_eq!(json_false.as_bool(), Some(false));
    }

    #[test]
    #[cfg(feature = "json")]
    fn test_parse_number() {
        let json = Json::parse("42.5").unwrap();
        assert_eq!(json.as_number(), Some(42.5));

        let json_int = Json::parse("100").unwrap();
        assert_eq!(json_int.as_i64(), Some(100));
    }

    #[test]
    #[cfg(feature = "json")]
    fn test_parse_string() {
        let json = Json::parse("\"hello world\"").unwrap();
        assert_eq!(json.as_string(), Some("hello world"));
    }

    #[test]
    #[cfg(feature = "json")]
    fn test_parse_array() {
        let json = Json::parse("[1, 2, 3]").unwrap();
        assert!(json.is_array());
        assert_eq!(json.len(), 3);

        let first = json.get_index(0).unwrap();
        assert_eq!(first.as_number(), Some(1.0));
    }

    #[test]
    #[cfg(feature = "json")]
    fn test_parse_object() {
        let json = Json::parse(r#"{"name": "test", "value": 42}"#).unwrap();
        assert!(json.is_object());
        assert_eq!(json.len(), 2);

        let name = json.get_key("name").unwrap();
        assert_eq!(name.as_string(), Some("test"));

        let value = json.get_key("value").unwrap();
        assert_eq!(value.as_number(), Some(42.0));
    }

    #[test]
    #[cfg(feature = "json")]
    fn test_nested() {
        let json = Json::parse(r#"{"items": [1, 2, {"nested": true}]}"#).unwrap();

        let items = json.get_key("items").unwrap();
        assert!(items.is_array());

        let nested_obj = items.get_index(2).unwrap();
        let nested = nested_obj.get_key("nested").unwrap();
        assert_eq!(nested.as_bool(), Some(true));
    }

    #[test]
    #[cfg(feature = "json")]
    fn test_parse_error() {
        let result = Json::parse("{ invalid }");
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.line > 0);
    }
}
