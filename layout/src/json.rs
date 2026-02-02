//! JSON parsing module for C API
//!
//! Provides a C-compatible JSON type using serde_json.
//! This allows parsing JSON without requiring type information upfront.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use azul_css::{AzString, OptionString, OptionF64, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_partialeq, impl_vec_mut, impl_result, impl_result_inner, impl_option, impl_option_inner};

#[cfg(feature = "json")]
use serde_json::Value;

// ============================================================================
// JSON Value Type
// ============================================================================

/// A generic JSON value that can hold any JSON type
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct Json {
    /// The type of this JSON value
    pub value_type: JsonType,
    /// Internal storage - interpretation depends on value_type
    /// For objects/arrays, this contains serialized data
    pub internal: JsonInternal,
}

/// Internal storage for JSON values
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct JsonInternal {
    /// For strings and serialized objects/arrays
    string_value: AzString,
    /// For numbers
    number_value: f64,
    /// For booleans
    bool_value: bool,
}

impl Default for JsonInternal {
    fn default() -> Self {
        Self {
            string_value: AzString::from(String::new()),
            number_value: 0.0,
            bool_value: false,
        }
    }
}

/// Type of a JSON value
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum JsonType {
    /// JSON null
    Null,
    /// JSON boolean (true/false)
    Bool,
    /// JSON number (stored as f64)
    Number,
    /// JSON string
    String,
    /// JSON array
    Array,
    /// JSON object
    Object,
}

/// Error when parsing JSON
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct JsonParseError {
    /// Error message
    pub message: AzString,
    /// Line number (if available)
    pub line: u32,
    /// Column number (if available)
    pub column: u32,
}

impl fmt::Display for JsonParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line > 0 {
            write!(f, "{}:{}: {}", self.line, self.column, self.message.as_str())
        } else {
            write!(f, "{}", self.message.as_str())
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for JsonParseError {}

/// A key-value pair in a JSON object
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct JsonKeyValue {
    /// The key
    pub key: AzString,
    /// The value
    pub value: Json,
}

impl JsonKeyValue {
    /// Create a new key-value pair
    pub fn create(key: AzString, value: Json) -> Self {
        Self { key, value }
    }
}

/// Option type for JsonKeyValue
impl_option!(JsonKeyValue, OptionJsonKeyValue, copy = false, [Debug, Clone, PartialEq]);

/// Vec of JsonKeyValue (FFI-safe)
impl_vec!(JsonKeyValue, JsonKeyValueVec, JsonKeyValueVecDestructor, JsonKeyValueVecDestructorType, JsonKeyValueVecSlice, OptionJsonKeyValue);
impl_vec_clone!(JsonKeyValue, JsonKeyValueVec, JsonKeyValueVecDestructor);
impl_vec_debug!(JsonKeyValue, JsonKeyValueVec);

impl JsonKeyValueVec {
    /// Creates a new, heap-allocated JsonKeyValueVec by copying elements from a C array
    #[inline]
    pub fn copy_from_array(ptr: *const JsonKeyValue, len: usize) -> Self {
        if ptr.is_null() || len == 0 {
            return Self::new();
        }
        let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
        Self::from_vec(slice.iter().cloned().collect())
    }
}

// FFI-safe JsonVec using impl_vec! macro
impl_vec!(Json, JsonVec, JsonVecDestructor, JsonVecDestructorType, JsonVecSlice, OptionJson);
impl_vec_clone!(Json, JsonVec, JsonVecDestructor);
impl_vec_debug!(Json, JsonVec);
impl_vec_partialeq!(Json, JsonVec);
impl_vec_mut!(Json, JsonVec);

impl JsonVec {
    /// Creates a new, heap-allocated JsonVec by copying elements from a C array
    #[inline]
    pub fn copy_from_array(ptr: *const Json, len: usize) -> Self {
        if ptr.is_null() || len == 0 {
            return Self::new();
        }
        let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
        Self::from_vec(slice.iter().cloned().collect())
    }
}

// FFI-safe Result type for JSON parsing
impl_result!(
    Json,
    JsonParseError,
    ResultJsonJsonParseError,
    copy = false,
    [Debug, Clone, PartialEq]
);

// FFI-safe Option types for JSON
impl_option!(Json, OptionJson, copy = false, [Clone, Debug, PartialEq]);
impl_option!(JsonVec, OptionJsonVec, copy = false, [Clone, Debug]);
impl_option!(JsonKeyValueVec, OptionJsonKeyValueVec, copy = false, [Clone, Debug]);

// FFI-safe Option types for JSON value extraction
// Re-export OptionBool from azul_css to avoid duplication
pub use azul_css::OptionBool;
impl_option!(i64, OptionI64, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

// ============================================================================
// JSON Parsing
// ============================================================================

impl Json {
    /// Parse JSON from a string
    #[cfg(feature = "json")]
    pub fn parse(s: &str) -> Result<Self, JsonParseError> {
        let value: Value = serde_json::from_str(s).map_err(|e| {
            JsonParseError {
                message: AzString::from(e.to_string()),
                line: e.line() as u32,
                column: e.column() as u32,
            }
        })?;
        
        Ok(Self::from_serde_value(value))
    }
    
    /// Parse JSON from bytes (UTF-8)
    #[cfg(feature = "json")]
    pub fn parse_bytes(bytes: &[u8]) -> Result<Self, JsonParseError> {
        let value: Value = serde_json::from_slice(bytes).map_err(|e| {
            JsonParseError {
                message: AzString::from(e.to_string()),
                line: e.line() as u32,
                column: e.column() as u32,
            }
        })?;
        
        Ok(Self::from_serde_value(value))
    }
    
    /// Convert from serde_json::Value
    #[cfg(feature = "json")]
    fn from_serde_value(value: Value) -> Self {
        match value {
            Value::Null => Self::null(),
            Value::Bool(b) => Self::bool(b),
            Value::Number(n) => Self::number(n.as_f64().unwrap_or(0.0)),
            Value::String(s) => Self::string(s),
            Value::Array(arr) => {
                // Serialize array back to JSON string for storage
                let json_str = serde_json::to_string(&Value::Array(arr)).unwrap_or_default();
                Self {
                    value_type: JsonType::Array,
                    internal: JsonInternal {
                        string_value: AzString::from(json_str),
                        number_value: 0.0,
                        bool_value: false,
                    },
                }
            }
            Value::Object(obj) => {
                // Serialize object back to JSON string for storage
                let json_str = serde_json::to_string(&Value::Object(obj)).unwrap_or_default();
                Self {
                    value_type: JsonType::Object,
                    internal: JsonInternal {
                        string_value: AzString::from(json_str),
                        number_value: 0.0,
                        bool_value: false,
                    },
                }
            }
        }
    }
    
    /// Create a null JSON value
    pub fn null() -> Self {
        Self {
            value_type: JsonType::Null,
            internal: JsonInternal::default(),
        }
    }
    
    /// Create a boolean JSON value
    pub fn bool(value: bool) -> Self {
        Self {
            value_type: JsonType::Bool,
            internal: JsonInternal {
                string_value: AzString::from(String::new()),
                number_value: 0.0,
                bool_value: value,
            },
        }
    }
    
    /// Create a number JSON value (floating-point)
    pub fn number(value: f64) -> Self {
        Self {
            value_type: JsonType::Number,
            internal: JsonInternal {
                string_value: AzString::from(String::new()),
                number_value: value,
                bool_value: false,
            },
        }
    }
    
    /// Create an integer JSON value
    pub fn integer(value: i64) -> Self {
        Self {
            value_type: JsonType::Number,
            internal: JsonInternal {
                string_value: AzString::from(String::new()),
                number_value: value as f64,
                bool_value: false,
            },
        }
    }
    
    /// Create a string JSON value
    pub fn string(value: impl Into<String>) -> Self {
        Self {
            value_type: JsonType::String,
            internal: JsonInternal {
                string_value: AzString::from(value.into()),
                number_value: 0.0,
                bool_value: false,
            },
        }
    }
    
    /// Create a JSON array from a vector of JSON values
    #[cfg(feature = "json")]
    pub fn array(values: JsonVec) -> Self {
        // Convert JsonVec to serde_json::Value::Array for serialization
        let serde_array: Vec<serde_json::Value> = values
            .as_slice()
            .iter()
            .map(|j| j.to_serde_value())
            .collect();
        let json_str = serde_json::to_string(&serde_json::Value::Array(serde_array))
            .unwrap_or_else(|_| "[]".to_string());
        
        Self {
            value_type: JsonType::Array,
            internal: JsonInternal {
                string_value: AzString::from(json_str),
                number_value: 0.0,
                bool_value: false,
            },
        }
    }
    
    /// Create a JSON object from key-value pairs
    #[cfg(feature = "json")]
    pub fn object(entries: JsonKeyValueVec) -> Self {
        // Convert JsonKeyValueVec to serde_json::Value::Object for serialization
        let mut map = serde_json::Map::new();
        for kv in entries.as_slice() {
            map.insert(kv.key.as_str().to_string(), kv.value.to_serde_value());
        }
        let json_str = serde_json::to_string(&serde_json::Value::Object(map))
            .unwrap_or_else(|_| "{}".to_string());
        
        Self {
            value_type: JsonType::Object,
            internal: JsonInternal {
                string_value: AzString::from(json_str),
                number_value: 0.0,
                bool_value: false,
            },
        }
    }
    
    /// Convert this Json to a serde_json::Value (internal helper)
    #[cfg(feature = "json")]
    fn to_serde_value(&self) -> serde_json::Value {
        match self.value_type {
            JsonType::Null => serde_json::Value::Null,
            JsonType::Bool => serde_json::Value::Bool(self.internal.bool_value),
            JsonType::Number => {
                let num = self.internal.number_value;
                // Check if the number is an integer (no fractional part)
                if num.fract() == 0.0 && num >= i64::MIN as f64 && num <= i64::MAX as f64 {
                    serde_json::Value::Number(serde_json::Number::from(num as i64))
                } else {
                    serde_json::Number::from_f64(num)
                        .map(serde_json::Value::Number)
                        .unwrap_or(serde_json::Value::Null)
                }
            }
            JsonType::String => serde_json::Value::String(self.internal.string_value.as_str().to_string()),
            JsonType::Array | JsonType::Object => {
                // Parse the stored JSON string
                serde_json::from_str(self.internal.string_value.as_str())
                    .unwrap_or(serde_json::Value::Null)
            }
        }
    }
    
    /// Check if this is null
    pub fn is_null(&self) -> bool {
        self.value_type == JsonType::Null
    }
    
    /// Check if this is a boolean
    pub fn is_bool(&self) -> bool {
        self.value_type == JsonType::Bool
    }
    
    /// Check if this is a number
    pub fn is_number(&self) -> bool {
        self.value_type == JsonType::Number
    }
    
    /// Check if this is a string
    pub fn is_string(&self) -> bool {
        self.value_type == JsonType::String
    }
    
    /// Check if this is an array
    pub fn is_array(&self) -> bool {
        self.value_type == JsonType::Array
    }
    
    /// Check if this is an object
    pub fn is_object(&self) -> bool {
        self.value_type == JsonType::Object
    }
    
    /// Get as boolean (returns None if not a bool)
    pub fn as_bool(&self) -> OptionBool {
        if self.value_type == JsonType::Bool {
            OptionBool::Some(self.internal.bool_value)
        } else {
            OptionBool::None
        }
    }
    
    /// Get as number (returns None if not a number)
    pub fn as_number(&self) -> OptionF64 {
        if self.value_type == JsonType::Number {
            OptionF64::Some(self.internal.number_value)
        } else {
            OptionF64::None
        }
    }
    
    /// Get as integer (returns None if not a number or not an integer)
    pub fn as_i64(&self) -> OptionI64 {
        if self.value_type == JsonType::Number {
            let n = self.internal.number_value;
            if n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
                OptionI64::Some(n as i64)
            } else {
                OptionI64::None
            }
        } else {
            OptionI64::None
        }
    }
    
    /// Get as string (returns None if not a string)
    pub fn as_string(&self) -> OptionString {
        if self.value_type == JsonType::String {
            OptionString::Some(self.internal.string_value.clone())
        } else {
            OptionString::None
        }
    }
    
    /// Get the number of elements (for arrays) or keys (for objects)
    #[cfg(feature = "json")]
    pub fn len(&self) -> usize {
        match self.value_type {
            JsonType::Array => {
                if let Ok(Value::Array(arr)) = serde_json::from_str(self.internal.string_value.as_str()) {
                    arr.len()
                } else {
                    0
                }
            }
            JsonType::Object => {
                if let Ok(Value::Object(obj)) = serde_json::from_str(self.internal.string_value.as_str()) {
                    obj.len()
                } else {
                    0
                }
            }
            _ => 0,
        }
    }
    
    /// Check if empty (for arrays/objects)
    #[cfg(feature = "json")]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    
    /// Get array element by index
    #[cfg(feature = "json")]
    pub fn get_index(&self, index: usize) -> Option<Json> {
        if self.value_type != JsonType::Array {
            return None;
        }
        
        let value: Value = serde_json::from_str(self.internal.string_value.as_str()).ok()?;
        if let Value::Array(arr) = value {
            arr.get(index).map(|v| Self::from_serde_value(v.clone()))
        } else {
            None
        }
    }
    
    /// Get object value by key
    #[cfg(feature = "json")]
    pub fn get_key(&self, key: &str) -> Option<Json> {
        if self.value_type != JsonType::Object {
            return None;
        }
        
        let value: Value = serde_json::from_str(self.internal.string_value.as_str()).ok()?;
        if let Value::Object(obj) = value {
            obj.get(key).map(|v| Self::from_serde_value(v.clone()))
        } else {
            None
        }
    }
    
    /// Get all keys of an object
    #[cfg(feature = "json")]
    pub fn keys(&self) -> Vec<AzString> {
        if self.value_type != JsonType::Object {
            return Vec::new();
        }
        
        let value: Value = match serde_json::from_str(self.internal.string_value.as_str()) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        
        if let Value::Object(obj) = value {
            obj.keys().map(|k| AzString::from(k.clone())).collect()
        } else {
            Vec::new()
        }
    }
    
    /// Convert array to Vec<Json>
    #[cfg(feature = "json")]
    pub fn to_array(&self) -> Option<JsonVec> {
        if self.value_type != JsonType::Array {
            return None;
        }
        
        let value: Value = serde_json::from_str(self.internal.string_value.as_str()).ok()?;
        if let Value::Array(arr) = value {
            Some(arr.into_iter().map(Self::from_serde_value).collect())
        } else {
            None
        }
    }
    
    /// Convert object to Vec<JsonKeyValue>
    #[cfg(feature = "json")]
    pub fn to_object(&self) -> Option<JsonKeyValueVec> {
        if self.value_type != JsonType::Object {
            return None;
        }
        
        let value: Value = serde_json::from_str(self.internal.string_value.as_str()).ok()?;
        if let Value::Object(obj) = value {
            Some(obj.into_iter().map(|(k, v)| JsonKeyValue {
                key: AzString::from(k),
                value: Self::from_serde_value(v),
            }).collect())
        } else {
            None
        }
    }
    
    /// Serialize to JSON string
    #[cfg(feature = "json")]
    pub fn to_string(&self) -> AzString {
        match self.value_type {
            JsonType::Null => AzString::from("null".to_string()),
            JsonType::Bool => AzString::from(if self.internal.bool_value { "true" } else { "false" }.to_string()),
            JsonType::Number => {
                let num = self.internal.number_value;
                // Serialize integers without decimal point
                if num.fract() == 0.0 && num >= i64::MIN as f64 && num <= i64::MAX as f64 {
                    AzString::from((num as i64).to_string())
                } else {
                    AzString::from(num.to_string())
                }
            }
            JsonType::String => {
                // Properly escape the string
                let escaped = serde_json::to_string(self.internal.string_value.as_str()).unwrap_or_default();
                AzString::from(escaped)
            }
            JsonType::Array | JsonType::Object => {
                // Already stored as JSON string
                self.internal.string_value.clone()
            }
        }
    }
    
    /// Serialize to pretty-printed JSON string
    #[cfg(feature = "json")]
    pub fn to_string_pretty(&self) -> AzString {
        match self.value_type {
            JsonType::Null | JsonType::Bool | JsonType::Number | JsonType::String => {
                self.to_string()
            }
            JsonType::Array | JsonType::Object => {
                if let Ok(value) = serde_json::from_str::<Value>(self.internal.string_value.as_str()) {
                    AzString::from(serde_json::to_string_pretty(&value).unwrap_or_default())
                } else {
                    self.internal.string_value.clone()
                }
            }
        }
    }
    
    /// Access a nested value using a JSON Pointer (RFC 6901).
    /// 
    /// The pointer syntax uses `/` to separate path components:
    /// - `/foo` accesses key "foo" in an object
    /// - `/0` accesses index 0 in an array
    /// - `/foo/bar/0` accesses nested paths
    /// 
    /// Returns Json::null() if the path doesn't exist or points to an invalid location.
    /// 
    /// # Example
    /// ```ignore
    /// let json = Json::parse(r#"{"users": [{"name": "Alice"}, {"name": "Bob"}]}"#).unwrap();
    /// let name = json.jq("/users/0/name"); // Json::string("Alice")
    /// ```
    #[cfg(feature = "json")]
    pub fn jq(&self, path: &str) -> Json {
        // For non-container types, only empty path matches
        match self.value_type {
            JsonType::Null | JsonType::Bool | JsonType::Number | JsonType::String => {
                if path.is_empty() {
                    self.clone()
                } else {
                    Json::null()
                }
            }
            JsonType::Array | JsonType::Object => {
                let value: Value = match serde_json::from_str(self.internal.string_value.as_str()) {
                    Ok(v) => v,
                    Err(_) => return Json::null(),
                };
                match value.pointer(path) {
                    Some(v) => Self::from_serde_value(v.clone()),
                    None => Json::null(),
                }
            }
        }
    }

    /// Access nested values using a JSON Pointer with wildcard support.
    /// 
    /// Similar to `jq()` but supports `*` wildcard to iterate over all 
    /// elements in an array or all values in an object.
    /// 
    /// The wildcard `*` matches all keys/indices at that position and returns
    /// a Vec of all matched values.
    /// 
    /// # Examples
    /// ```ignore
    /// let json = Json::parse(r#"{"users": [{"name": "Alice"}, {"name": "Bob"}]}"#).unwrap();
    /// 
    /// // Get all user names
    /// let names = json.jq_all("/users/*/name"); // [Json::string("Alice"), Json::string("Bob")]
    /// 
    /// // Without wildcard, behaves like jq()
    /// let first = json.jq_all("/users/0/name"); // [Json::string("Alice")]
    /// ```
    #[cfg(feature = "json")]
    pub fn jq_all(&self, path: &str) -> JsonVec {
        // For non-container types, only empty path matches
        let result = match self.value_type {
            JsonType::Null | JsonType::Bool | JsonType::Number | JsonType::String => {
                if path.is_empty() {
                    vec![self.clone()]
                } else {
                    vec![]
                }
            }
            JsonType::Array | JsonType::Object => {
                let value: Value = match serde_json::from_str(self.internal.string_value.as_str()) {
                    Ok(v) => v,
                    Err(_) => return JsonVec::from_vec(vec![]),
                };
                Self::jq_all_recursive(&value, path)
            }
        };
        JsonVec::from_vec(result)
    }

    /// Recursive helper for jq_all that handles wildcards
    #[cfg(feature = "json")]
    fn jq_all_recursive(value: &Value, path: &str) -> Vec<Json> {
        // Empty path returns current value
        if path.is_empty() {
            return vec![Self::from_serde_value(value.clone())];
        }

        // Path must start with /
        if !path.starts_with('/') {
            return vec![];
        }

        // Find next path component
        let rest = &path[1..]; // Skip leading /
        let (component, remaining) = match rest.find('/') {
            Some(idx) => (&rest[..idx], &rest[idx..]),
            None => (rest, ""),
        };

        // Handle wildcard
        if component == "*" {
            let mut results = Vec::new();
            match value {
                Value::Array(arr) => {
                    for item in arr {
                        results.extend(Self::jq_all_recursive(item, remaining));
                    }
                }
                Value::Object(obj) => {
                    for (_key, val) in obj {
                        results.extend(Self::jq_all_recursive(val, remaining));
                    }
                }
                _ => {} // Wildcard on non-container returns empty
            }
            results
        } else {
            // Regular component - try as array index or object key
            match value {
                Value::Array(arr) => {
                    if let Ok(idx) = component.parse::<usize>() {
                        if let Some(item) = arr.get(idx) {
                            return Self::jq_all_recursive(item, remaining);
                        }
                    }
                    vec![]
                }
                Value::Object(obj) => {
                    if let Some(val) = obj.get(component) {
                        return Self::jq_all_recursive(val, remaining);
                    }
                    vec![]
                }
                _ => vec![],
            }
        }
    }
}

impl fmt::Display for Json {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[cfg(feature = "json")]
        {
            write!(f, "{}", self.to_string().as_str())
        }
        #[cfg(not(feature = "json"))]
        {
            write!(f, "<json>")
        }
    }
}

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
    json.to_string()
}

/// Serialize JSON to pretty string
#[cfg(feature = "json")]
pub fn json_stringify_pretty(json: &Json) -> AzString {
    json.to_string_pretty()
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
    /// Returns true if the result is Ok
    pub fn is_ok(&self) -> bool {
        matches!(self, ResultRefAnyString::Ok(_))
    }
    
    /// Returns true if the result is Err
    pub fn is_err(&self) -> bool {
        matches!(self, ResultRefAnyString::Err(_))
    }
    
    /// Converts to Option<RefAny>, discarding any error
    pub fn ok(self) -> Option<RefAny> {
        match self {
            ResultRefAnyString::Ok(r) => Some(r),
            ResultRefAnyString::Err(_) => None,
        }
    }
    
    /// Converts to Option<AzString>, discarding success value
    pub fn err(self) -> Option<AzString> {
        match self {
            ResultRefAnyString::Ok(_) => None,
            ResultRefAnyString::Err(e) => Some(e),
        }
    }
}

/// C-compatible function type for serializing a RefAny's contents to JSON.
///
/// The function receives a cloned RefAny (by value) and should return
/// a `Json` value representing the serialized data.
///
/// Returns `Json::null()` on serialization failure.
///
/// # Example Implementation
///
/// ```ignore
/// extern "C" fn my_serialize(refany: RefAny) -> Json {
///     if let Some(data) = refany.downcast_ref::<MyData>() {
///         // Build JSON from data
///         Json::object(...)
///     } else {
///         Json::null()
///     }
/// }
/// ```
pub type RefAnySerializeFnType = extern "C" fn(RefAny) -> Json;

/// C-compatible function type for deserializing JSON into a new RefAny.
///
/// The function receives a `Json` value and should return either:
/// - `ResultRefAnyString::Ok(RefAny)` with the deserialized data
/// - `ResultRefAnyString::Err(AzString)` with an error message describing the failure
///
/// Error messages should indicate whether the failure was due to:
/// - Serde parsing error (invalid JSON structure)
/// - Type construction error (valid JSON but cannot build RefAny)
///
/// # Note
///
/// This creates a NEW RefAny - it does not modify an existing one.
///
/// # Example Implementation
///
/// ```ignore
/// extern "C" fn my_deserialize(json: Json) -> ResultRefAnyString {
///     let name = match json.get_key("name") {
///         Some(n) => n.as_string().unwrap_or_default(),
///         None => return ResultRefAnyString::Err(AzString::from("Missing field: name")),
///     };
///     
///     let data = MyData { name: name.to_string() };
///     let refany = RefAny::new(data);
///     ResultRefAnyString::Ok(refany)
/// }
/// ```
pub type RefAnyDeserializeFnType = extern "C" fn(Json) -> ResultRefAnyString;

/// Serialize a RefAny to JSON using its registered serialize function.
///
/// Returns `None` if:
/// - Serialization is not supported (serialize_fn == 0)
/// - The serialize function returns `Json::null()`
///
/// # Example
///
/// ```ignore
/// let data = MyData::new();
/// let refany = MyData_upcast(data); // Created with AZ_REFLECT_JSON
/// 
/// if let Some(json) = serialize_refany_to_json(&refany) {
///     println!("JSON: {}", json.to_json_string());
/// }
/// ```
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
/// 
/// # Parameters
///
/// - `json`: The JSON data to deserialize
/// - `deserialize_fn`: Function pointer obtained from a RefAny of the target type
///                     (via `refany.get_deserialize_fn()`)
/// 
/// # Returns
///
/// - `Ok(RefAny)` on success
/// - `Err(String)` with error message on failure
///
/// # Example
///
/// ```ignore
/// // Get the deserialize function from an existing RefAny of the target type
/// let template_refany = MyData_upcast(MyData::default());
/// let deserialize_fn = template_refany.get_deserialize_fn();
///
/// // Deserialize new data from JSON
/// let json = Json::parse(r#"{"name": "test"}"#).unwrap();
/// match deserialize_refany_from_json(json, deserialize_fn) {
///     Ok(refany) => { /* use refany */ },
///     Err(e) => eprintln!("Failed: {}", e),
/// }
/// ```
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
    /// FFI-friendly constructor for Ok variant
    pub fn ok_result(refany: RefAny) -> Self {
        ResultRefAnyString::Ok(refany)
    }
    
    /// FFI-friendly constructor for Err variant
    pub fn err_result(message: AzString) -> Self {
        ResultRefAnyString::Err(message)
    }
}

// ============================================================================
// FFI-friendly wrappers for RefAny JSON serialization
// ============================================================================

/// Serialize a RefAny to JSON using its registered serialize function.
/// Returns OptionJson::None if serialization is not supported or fails.
#[cfg(feature = "json")]
pub fn refany_serialize_to_json(refany: &RefAny) -> OptionJson {
    match serialize_refany_to_json(refany) {
        Some(json) => OptionJson::Some(json),
        None => OptionJson::None,
    }
}

impl Json {
    /// Deserialize JSON into a RefAny using the provided deserialize function.
    ///
    /// # Parameters
    /// - `deserialize_fn`: Function pointer obtained from `RefAny::get_deserialize_fn()`
    ///
    /// # Returns
    /// - `Ok(RefAny)` on success
    /// - `Err(String)` with error message on failure
    #[cfg(feature = "json")]
    pub fn deserialize_to_refany(self, deserialize_fn: usize) -> ResultRefAnyString {
        deserialize_refany_from_json(self, deserialize_fn).into()
    }
}
