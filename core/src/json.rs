//! JSON value types for C API (data definitions only, no serde_json dependency)
//!
//! The actual parsing/serialization lives in `azul_layout::json` which adds
//! serde_json-based implementations on top of these types.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use azul_css::{
    AzString, OptionString, OptionF64, OptionBool,
    impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_partialeq, impl_vec_mut,
    impl_result, impl_result_inner,
    impl_option, impl_option_inner,
};

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
    pub string_value: AzString,
    /// For numbers
    pub number_value: f64,
    /// For booleans
    pub bool_value: bool,
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

// ============================================================================
// FFI-safe collection types
// ============================================================================

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
// Note: OptionBool and OptionF64 are already exported from azul_css
impl_option!(i64, OptionI64, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

// ============================================================================
// Non-serde methods on Json (pure data, no parsing)
// ============================================================================

impl Json {
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

    /// Get the raw internal string value (for arrays/objects this is the serialized JSON)
    pub fn raw_string(&self) -> &str {
        self.internal.string_value.as_str()
    }
}

impl fmt::Display for Json {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.value_type {
            JsonType::Null => write!(f, "null"),
            JsonType::Bool => write!(f, "{}", self.internal.bool_value),
            JsonType::Number => {
                let num = self.internal.number_value;
                if num.fract() == 0.0 && num >= i64::MIN as f64 && num <= i64::MAX as f64 {
                    write!(f, "{}", num as i64)
                } else {
                    write!(f, "{}", num)
                }
            }
            JsonType::String => write!(f, "\"{}\"", self.internal.string_value.as_str()),
            JsonType::Array | JsonType::Object => {
                write!(f, "{}", self.internal.string_value.as_str())
            }
        }
    }
}

// ============================================================================
// serde_json-dependent methods (gated behind "serde-json" feature)
// ============================================================================

#[cfg(feature = "serde-json")]
impl Json {
    /// Parse JSON from a string
    pub fn parse(s: &str) -> Result<Self, JsonParseError> {
        let value: serde_json::Value = serde_json::from_str(s).map_err(|e| {
            JsonParseError {
                message: AzString::from(alloc::format!("{}", e)),
                line: e.line() as u32,
                column: e.column() as u32,
            }
        })?;
        Ok(Self::from_serde_value(value))
    }

    /// Parse JSON from bytes (UTF-8)
    pub fn parse_bytes(bytes: &[u8]) -> Result<Self, JsonParseError> {
        let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| {
            JsonParseError {
                message: AzString::from(alloc::format!("{}", e)),
                line: e.line() as u32,
                column: e.column() as u32,
            }
        })?;
        Ok(Self::from_serde_value(value))
    }

    /// Convert from serde_json::Value
    pub fn from_serde_value(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => Self::null(),
            serde_json::Value::Bool(b) => Self::bool(b),
            serde_json::Value::Number(n) => Self::number(n.as_f64().unwrap_or(0.0)),
            serde_json::Value::String(s) => Self::string(s),
            serde_json::Value::Array(arr) => {
                let json_str = serde_json::to_string(&serde_json::Value::Array(arr)).unwrap_or_default();
                Self {
                    value_type: JsonType::Array,
                    internal: JsonInternal {
                        string_value: AzString::from(json_str),
                        number_value: 0.0,
                        bool_value: false,
                    },
                }
            }
            serde_json::Value::Object(obj) => {
                let json_str = serde_json::to_string(&serde_json::Value::Object(obj)).unwrap_or_default();
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

    /// Convert this Json to a serde_json::Value
    pub fn to_serde_value(&self) -> serde_json::Value {
        match self.value_type {
            JsonType::Null => serde_json::Value::Null,
            JsonType::Bool => serde_json::Value::Bool(self.internal.bool_value),
            JsonType::Number => {
                let num = self.internal.number_value;
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
                serde_json::from_str(self.internal.string_value.as_str())
                    .unwrap_or(serde_json::Value::Null)
            }
        }
    }

    /// Create a JSON array from a vector of JSON values
    pub fn array(values: JsonVec) -> Self {
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
    pub fn object(entries: JsonKeyValueVec) -> Self {
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

    /// Get the number of elements (for arrays) or keys (for objects)
    pub fn len(&self) -> usize {
        match self.value_type {
            JsonType::Array => {
                if let Ok(serde_json::Value::Array(arr)) = serde_json::from_str(self.internal.string_value.as_str()) {
                    arr.len()
                } else {
                    0
                }
            }
            JsonType::Object => {
                if let Ok(serde_json::Value::Object(obj)) = serde_json::from_str(self.internal.string_value.as_str()) {
                    obj.len()
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    /// Check if empty (for arrays/objects)
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get array element by index
    pub fn get_index(&self, index: usize) -> Option<Json> {
        if self.value_type != JsonType::Array { return None; }
        let value: serde_json::Value = serde_json::from_str(self.internal.string_value.as_str()).ok()?;
        if let serde_json::Value::Array(arr) = value {
            arr.get(index).map(|v| Self::from_serde_value(v.clone()))
        } else {
            None
        }
    }

    /// Get object value by key
    pub fn get_key(&self, key: &str) -> Option<Json> {
        if self.value_type != JsonType::Object { return None; }
        let value: serde_json::Value = serde_json::from_str(self.internal.string_value.as_str()).ok()?;
        if let serde_json::Value::Object(obj) = value {
            obj.get(key).map(|v| Self::from_serde_value(v.clone()))
        } else {
            None
        }
    }

    /// Get all keys of an object
    pub fn keys(&self) -> Vec<AzString> {
        if self.value_type != JsonType::Object { return Vec::new(); }
        let value: serde_json::Value = match serde_json::from_str(self.internal.string_value.as_str()) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        if let serde_json::Value::Object(obj) = value {
            obj.keys().map(|k| AzString::from(k.clone())).collect()
        } else {
            Vec::new()
        }
    }

    /// Convert array to Vec<Json>
    pub fn to_array(&self) -> Option<JsonVec> {
        if self.value_type != JsonType::Array { return None; }
        let value: serde_json::Value = serde_json::from_str(self.internal.string_value.as_str()).ok()?;
        if let serde_json::Value::Array(arr) = value {
            Some(arr.into_iter().map(Self::from_serde_value).collect())
        } else {
            None
        }
    }

    /// Convert object to Vec<JsonKeyValue>
    pub fn to_object(&self) -> Option<JsonKeyValueVec> {
        if self.value_type != JsonType::Object { return None; }
        let value: serde_json::Value = serde_json::from_str(self.internal.string_value.as_str()).ok()?;
        if let serde_json::Value::Object(obj) = value {
            Some(obj.into_iter().map(|(k, v)| JsonKeyValue {
                key: AzString::from(k),
                value: Self::from_serde_value(v),
            }).collect())
        } else {
            None
        }
    }

    /// Serialize to JSON string (returns AzString)
    pub fn to_json_string(&self) -> AzString {
        match self.value_type {
            JsonType::Null => AzString::from(alloc::string::String::from("null")),
            JsonType::Bool => AzString::from(if self.internal.bool_value { alloc::string::String::from("true") } else { alloc::string::String::from("false") }),
            JsonType::Number => {
                let num = self.internal.number_value;
                if num.fract() == 0.0 && num >= i64::MIN as f64 && num <= i64::MAX as f64 {
                    AzString::from(alloc::format!("{}", num as i64))
                } else {
                    AzString::from(alloc::format!("{}", num))
                }
            }
            JsonType::String => {
                let escaped = serde_json::to_string(self.internal.string_value.as_str()).unwrap_or_default();
                AzString::from(escaped)
            }
            JsonType::Array | JsonType::Object => {
                self.internal.string_value.clone()
            }
        }
    }

    /// Serialize to pretty-printed JSON string
    pub fn to_string_pretty(&self) -> AzString {
        match self.value_type {
            JsonType::Null | JsonType::Bool | JsonType::Number | JsonType::String => {
                self.to_json_string()
            }
            JsonType::Array | JsonType::Object => {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(self.internal.string_value.as_str()) {
                    AzString::from(serde_json::to_string_pretty(&value).unwrap_or_default())
                } else {
                    self.internal.string_value.clone()
                }
            }
        }
    }

    /// Access a nested value using a JSON Pointer (RFC 6901).
    pub fn jq(&self, path: &str) -> Json {
        match self.value_type {
            JsonType::Null | JsonType::Bool | JsonType::Number | JsonType::String => {
                if path.is_empty() { self.clone() } else { Json::null() }
            }
            JsonType::Array | JsonType::Object => {
                let value: serde_json::Value = match serde_json::from_str(self.internal.string_value.as_str()) {
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
    pub fn jq_all(&self, path: &str) -> JsonVec {
        let result = match self.value_type {
            JsonType::Null | JsonType::Bool | JsonType::Number | JsonType::String => {
                if path.is_empty() { vec![self.clone()] } else { vec![] }
            }
            JsonType::Array | JsonType::Object => {
                let value: serde_json::Value = match serde_json::from_str(self.internal.string_value.as_str()) {
                    Ok(v) => v,
                    Err(_) => return JsonVec::from_vec(vec![]),
                };
                Self::jq_all_recursive(&value, path)
            }
        };
        JsonVec::from_vec(result)
    }

    /// Recursive helper for jq_all that handles wildcards
    fn jq_all_recursive(value: &serde_json::Value, path: &str) -> Vec<Json> {
        if path.is_empty() {
            return vec![Self::from_serde_value(value.clone())];
        }
        if !path.starts_with('/') { return vec![]; }
        let rest = &path[1..];
        let (component, remaining) = match rest.find('/') {
            Some(idx) => (&rest[..idx], &rest[idx..]),
            None => (rest, ""),
        };
        if component == "*" {
            let mut results = Vec::new();
            match value {
                serde_json::Value::Array(arr) => {
                    for item in arr { results.extend(Self::jq_all_recursive(item, remaining)); }
                }
                serde_json::Value::Object(obj) => {
                    for (_key, val) in obj { results.extend(Self::jq_all_recursive(val, remaining)); }
                }
                _ => {}
            }
            results
        } else {
            match value {
                serde_json::Value::Array(arr) => {
                    if let Ok(idx) = component.parse::<usize>() {
                        if let Some(item) = arr.get(idx) {
                            return Self::jq_all_recursive(item, remaining);
                        }
                    }
                    vec![]
                }
                serde_json::Value::Object(obj) => {
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
