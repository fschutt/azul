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
    /// Internal storage - interpretation depends on `value_type`
    /// For objects/arrays, this contains serialized data
    pub internal: JsonInternal,
}

/// Internal storage for JSON values.
///
/// This is a C-FFI-compatible tagged-union-via-struct: all fields always exist,
/// but only the field(s) corresponding to `JsonType` in the parent `Json` are
/// meaningful.  For compound types (`Array`, `Object`) the serialized JSON is
/// stored in `string_value` and re-parsed on each access — this trades repeated
/// parsing cost for a flat, FFI-safe layout with no interior pointers.
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    #[must_use] pub const fn create(key: AzString, value: Json) -> Self {
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
    /// Creates a new, heap-allocated `JsonKeyValueVec` by copying elements from a C array
    #[inline]
    #[allow(clippy::not_unsafe_ptr_arg_deref)] // SAFETY/FFI: `*const T` is the C-ABI signature; the fn null-checks then derefs under the documented caller contract (C guarantees a valid ptr/len). Marking it `unsafe fn` would force unsafe blocks into the generated dll bindings.
    #[must_use] pub fn copy_from_array(ptr: *const JsonKeyValue, len: usize) -> Self {
        if ptr.is_null() || len == 0 {
            return Self::new();
        }
        let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
        Self::from_vec(slice.to_vec())
    }
}

// FFI-safe JsonVec using impl_vec! macro
impl_vec!(Json, JsonVec, JsonVecDestructor, JsonVecDestructorType, JsonVecSlice, OptionJson);
impl_vec_clone!(Json, JsonVec, JsonVecDestructor);
impl_vec_debug!(Json, JsonVec);
impl_vec_partialeq!(Json, JsonVec);
impl_vec_mut!(Json, JsonVec);

impl JsonVec {
    /// Creates a new, heap-allocated `JsonVec` by copying elements from a C array
    #[inline]
    #[allow(clippy::not_unsafe_ptr_arg_deref)] // SAFETY/FFI: `*const T` is the C-ABI signature; the fn null-checks then derefs under the documented caller contract (C guarantees a valid ptr/len). Marking it `unsafe fn` would force unsafe blocks into the generated dll bindings.
    #[must_use] pub fn copy_from_array(ptr: *const Json, len: usize) -> Self {
        if ptr.is_null() || len == 0 {
            return Self::new();
        }
        let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
        Self::from_vec(slice.to_vec())
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
// Helpers
// ============================================================================

/// Try to losslessly convert an `f64` to `i64`.
///
/// Returns `Some` only when `n` is an integer that fits in `i64` without
/// overflow.  The upper bound uses `< 2^63` (not `<= i64::MAX as f64`)
/// because `i64::MAX` cannot be represented exactly in `f64` — the cast
/// rounds up to `2^63`, which would cause overflow on `n as i64`.
#[allow(clippy::cast_possible_truncation)] // bounded DPI/dimension/number conversion
fn f64_as_i64(n: f64) -> Option<i64> {
    if n.fract() == 0.0 && n >= -(2_f64.powi(63)) && n < 2_f64.powi(63) {
        Some(n as i64)
    } else {
        None
    }
}

// ============================================================================
// Non-serde methods on Json (pure data, no parsing)
// ============================================================================

impl Json {
    /// Create a null JSON value
    #[must_use] pub fn null() -> Self {
        Self {
            value_type: JsonType::Null,
            internal: JsonInternal::default(),
        }
    }

    /// Create a boolean JSON value
    #[must_use] pub fn bool(value: bool) -> Self {
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
    #[must_use] pub fn number(value: f64) -> Self {
        Self {
            value_type: JsonType::Number,
            internal: JsonInternal {
                string_value: AzString::from(String::new()),
                number_value: value,
                bool_value: false,
            },
        }
    }

    /// Create an integer JSON value.
    ///
    /// **Note:** the value is stored as `f64` internally, so `i64` values with
    /// magnitude greater than 2^53 will lose precision silently.
    #[allow(clippy::cast_precision_loss)] // bounded DPI/dimension/number conversion
    #[must_use] pub fn integer(value: i64) -> Self {
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
    #[must_use] pub fn is_null(&self) -> bool {
        self.value_type == JsonType::Null
    }

    /// Check if this is a boolean
    #[must_use] pub fn is_bool(&self) -> bool {
        self.value_type == JsonType::Bool
    }

    /// Check if this is a number
    #[must_use] pub fn is_number(&self) -> bool {
        self.value_type == JsonType::Number
    }

    /// Check if this is a string
    #[must_use] pub fn is_string(&self) -> bool {
        self.value_type == JsonType::String
    }

    /// Check if this is an array
    #[must_use] pub fn is_array(&self) -> bool {
        self.value_type == JsonType::Array
    }

    /// Check if this is an object
    #[must_use] pub fn is_object(&self) -> bool {
        self.value_type == JsonType::Object
    }

    /// Get as boolean (returns None if not a bool)
    #[must_use] pub fn as_bool(&self) -> OptionBool {
        if self.value_type == JsonType::Bool {
            OptionBool::Some(self.internal.bool_value)
        } else {
            OptionBool::None
        }
    }

    /// Get as number (returns None if not a number)
    #[must_use] pub fn as_number(&self) -> OptionF64 {
        if self.value_type == JsonType::Number {
            OptionF64::Some(self.internal.number_value)
        } else {
            OptionF64::None
        }
    }

    /// Get as integer (returns None if not a number or not an integer)
    #[must_use] pub fn as_i64(&self) -> OptionI64 {
        if self.value_type == JsonType::Number {
            f64_as_i64(self.internal.number_value).map_or(OptionI64::None, OptionI64::Some)
        } else {
            OptionI64::None
        }
    }

    /// Get as string (returns None if not a string)
    #[must_use] pub fn as_string(&self) -> OptionString {
        if self.value_type == JsonType::String {
            OptionString::Some(self.internal.string_value.clone())
        } else {
            OptionString::None
        }
    }

    /// Get the raw internal string value (for arrays/objects this is the serialized JSON)
    #[must_use] pub fn raw_string(&self) -> &str {
        self.internal.string_value.as_str()
    }
}

/// Note: the `Display` output is meant for human-readable / debug display.
/// String values are quoted but **not** JSON-escaped (no backslash escaping
/// of embedded quotes, newlines, etc.).  Use `to_json_string()` (requires
/// the `serde-json` feature) when valid JSON output is needed.
impl fmt::Display for Json {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.value_type {
            JsonType::Null => write!(f, "null"),
            JsonType::Bool => write!(f, "{}", self.internal.bool_value),
            JsonType::Number => {
                let num = self.internal.number_value;
                if let Some(i) = f64_as_i64(num) {
                    write!(f, "{i}")
                } else {
                    write!(f, "{num}")
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
                if let Some(i) = f64_as_i64(num) {
                    serde_json::Value::Number(serde_json::Number::from(i))
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
                if let Some(i) = f64_as_i64(num) {
                    AzString::from(alloc::format!("{}", i))
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

    /// Maximum JSON-Pointer component depth for [`jq_all`](Self::jq_all).
    ///
    /// AUDIT 2026-07-08: `jq_all_recursive` recursed once per pointer component,
    /// so an attacker-supplied pointer with tens of thousands of `/` segments
    /// (e.g. `"/a".repeat(100_000)`) overflowed the stack. The single-child
    /// descent is now iterative (unbounded, allocation-free); only the wildcard
    /// (`*`) fan-out still recurses, and that recursion is capped here. 512 is far
    /// deeper than any real document nesting while staying well inside the stack.
    const JQ_MAX_WILDCARD_DEPTH: usize = 512;

    /// Recursive helper for jq_all that handles wildcards.
    ///
    /// Non-wildcard components are walked in a loop so a long linear pointer can
    /// never overflow the stack; only `*` fan-out recurses, bounded by
    /// [`JQ_MAX_WILDCARD_DEPTH`](Self::JQ_MAX_WILDCARD_DEPTH).
    fn jq_all_recursive(value: &serde_json::Value, path: &str) -> Vec<Json> {
        Self::jq_all_recursive_depth(value, path, 0)
    }

    fn jq_all_recursive_depth(
        value: &serde_json::Value,
        path: &str,
        depth: usize,
    ) -> Vec<Json> {
        // Guard the wildcard recursion; exceeding the cap yields no match rather
        // than crashing.
        if depth > Self::JQ_MAX_WILDCARD_DEPTH {
            return vec![];
        }

        // Walk non-wildcard components iteratively.
        let mut value = value;
        let mut path = path;
        loop {
            if path.is_empty() {
                return vec![Self::from_serde_value(value.clone())];
            }
            if !path.starts_with('/') {
                return vec![];
            }
            let rest = &path[1..];
            let (component, remaining) = match rest.find('/') {
                Some(idx) => (&rest[..idx], &rest[idx..]),
                None => (rest, ""),
            };

            if component == "*" {
                let mut results = Vec::new();
                match value {
                    serde_json::Value::Array(arr) => {
                        for item in arr {
                            results.extend(Self::jq_all_recursive_depth(
                                item,
                                remaining,
                                depth + 1,
                            ));
                        }
                    }
                    serde_json::Value::Object(obj) => {
                        for (_key, val) in obj {
                            results.extend(Self::jq_all_recursive_depth(
                                val,
                                remaining,
                                depth + 1,
                            ));
                        }
                    }
                    _ => {}
                }
                return results;
            }

            // Single-child descent: advance the cursor instead of recursing.
            let next = match value {
                serde_json::Value::Array(arr) => {
                    component.parse::<usize>().ok().and_then(|idx| arr.get(idx))
                }
                serde_json::Value::Object(obj) => obj.get(component),
                _ => None,
            };
            match next {
                Some(v) => {
                    value = v;
                    path = remaining;
                }
                None => return vec![],
            }
        }
    }
}

#[cfg(test)]
mod jq_recursion_tests {
    use super::*;

    /// AUDIT 2026-07-08: a pointer with a very large number of components used to
    /// overflow the stack via per-component recursion. Linear (non-wildcard)
    /// descent is now iterative, so an over-long pointer against a shallow
    /// document returns empty promptly with zero recursion instead of a deep call
    /// chain. `serde_json`'s own 128-level parse cap keeps documents shallow, but
    /// this guarantees the jq walk itself never blows the stack on a huge pointer.
    #[test]
    #[cfg(feature = "serde-json")]
    fn huge_pointer_on_shallow_doc_returns_empty() {
        let json = Json::parse("{\"a\":{\"b\":1}}").expect("parse");
        let pointer = "/a".repeat(200_000);
        assert_eq!(json.jq_all(&pointer).as_ref().len(), 0);
    }

    /// A moderately deep linear pointer (within serde's parse limit) resolves to
    /// its single leaf via the iterative descent.
    #[test]
    #[cfg(feature = "serde-json")]
    fn deep_linear_pointer_resolves_leaf() {
        const DEPTH: usize = 100; // below serde_json's 128-level parse cap

        let mut doc = String::new();
        for _ in 0..DEPTH {
            doc.push_str("{\"a\":");
        }
        doc.push_str("42");
        for _ in 0..DEPTH {
            doc.push('}');
        }

        let json = Json::parse(&doc).expect("deep doc should parse");
        let pointer = "/a".repeat(DEPTH);
        let out = json.jq_all(&pointer);
        assert_eq!(out.as_ref().len(), 1, "the single leaf should be found");
    }

    /// Ordinary wildcard + index access still works after the iterative rewrite.
    #[test]
    #[cfg(feature = "serde-json")]
    fn wildcard_and_index_still_work() {
        let json = Json::parse("{\"items\":[{\"v\":1},{\"v\":2},{\"v\":3}]}").expect("parse");
        let all = json.jq_all("/items/*/v");
        assert_eq!(all.as_ref().len(), 3);
        let one = json.jq_all("/items/1/v");
        assert_eq!(one.as_ref().len(), 1);
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::unreadable_literal)]
mod autotest_generated {
    use super::*;

    // ------------------------------------------------------------------
    // helpers
    // ------------------------------------------------------------------

    fn az(s: &str) -> AzString {
        AzString::from(String::from(s))
    }

    /// Build a `Json` by hand, bypassing the constructors. Used to feed the
    /// accessors a *corrupt* value (e.g. `value_type: Array` whose internal
    /// string is not parseable JSON) — reachable over FFI because every field
    /// of `Json` / `JsonInternal` is `pub`.
    fn raw(value_type: JsonType, string_value: &str) -> Json {
        Json {
            value_type,
            internal: JsonInternal {
                string_value: az(string_value),
                number_value: 0.0,
                bool_value: false,
            },
        }
    }

    fn two_pow_63() -> f64 {
        2_f64.powi(63)
    }

    // ==================================================================
    // f64_as_i64  (numeric: zero / min_max / negative / overflow / nan_inf)
    // ==================================================================

    #[test]
    fn f64_as_i64_zero_and_negative_zero() {
        assert_eq!(f64_as_i64(0.0), Some(0));
        // -0.0 is an integer and in range: the sign is silently dropped.
        assert_eq!(f64_as_i64(-0.0), Some(0));
    }

    #[test]
    fn f64_as_i64_min_max_boundaries() {
        // -2^63 is exactly representable and is exactly i64::MIN.
        assert_eq!(f64_as_i64(-two_pow_63()), Some(i64::MIN));
        // +2^63 is NOT a valid i64 — must be rejected rather than wrapping.
        assert_eq!(f64_as_i64(two_pow_63()), None);
        // i64::MAX rounds *up* to 2^63 when cast to f64, so it is rejected too.
        // This is the documented reason the bound is `< 2^63` and not `<= MAX`.
        #[allow(clippy::cast_precision_loss)]
        let max_as_f64 = i64::MAX as f64;
        assert_eq!(max_as_f64, two_pow_63());
        assert_eq!(f64_as_i64(max_as_f64), None);
        // The largest f64 that is a valid i64: 2^63 - 1024.
        let just_below = two_pow_63() - 1024.0;
        assert_eq!(f64_as_i64(just_below), Some(9_223_372_036_854_774_784));
    }

    #[test]
    fn f64_as_i64_negatives_are_deterministic() {
        assert_eq!(f64_as_i64(-1.0), Some(-1));
        assert_eq!(f64_as_i64(-42.0), Some(-42));
        assert_eq!(f64_as_i64(-0.5), None);
        assert_eq!(f64_as_i64(-1.0 - f64::EPSILON), None);
        // One ULP below -2^63 is out of range.
        assert_eq!(f64_as_i64(-two_pow_63() * (1.0 + f64::EPSILON)), None);
    }

    #[test]
    fn f64_as_i64_overflow_inputs_return_none_not_a_wrapped_cast() {
        for n in [
            1e19_f64,
            1e300_f64,
            -1e300_f64,
            f64::MAX,
            f64::MIN,
            two_pow_63() * 2.0,
        ] {
            assert_eq!(f64_as_i64(n), None, "{n} must not be cast to i64");
        }
    }

    #[test]
    fn f64_as_i64_nan_and_infinity_do_not_panic() {
        // NaN.fract() is NaN, and NaN == 0.0 is false, so all three fall through
        // to `None` without ever reaching the (UB-adjacent) `as i64` cast.
        assert_eq!(f64_as_i64(f64::NAN), None);
        assert_eq!(f64_as_i64(f64::INFINITY), None);
        assert_eq!(f64_as_i64(f64::NEG_INFINITY), None);
    }

    #[test]
    fn f64_as_i64_fractional_and_subnormal_return_none() {
        assert_eq!(f64_as_i64(0.5), None);
        assert_eq!(f64_as_i64(f64::EPSILON), None);
        assert_eq!(f64_as_i64(f64::MIN_POSITIVE), None);
        // Smallest subnormal.
        assert_eq!(f64_as_i64(f64::from_bits(1)), None);
    }

    // ==================================================================
    // Json::number / Json::integer  (numeric)
    // ==================================================================

    #[test]
    fn number_stores_nan_and_infinity_without_panicking() {
        for n in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let j = Json::number(n);
            assert!(j.is_number());
            assert_eq!(j.as_i64(), OptionI64::None);
            match j.as_number() {
                OptionF64::Some(v) => assert_eq!(v.is_nan(), n.is_nan()),
                OptionF64::None => panic!("as_number() must be Some for a Number"),
            }
            // Display must not panic on non-finite floats.
            let s = alloc::format!("{j}");
            assert!(!s.is_empty());
        }
    }

    #[test]
    fn number_min_max_and_zero() {
        assert_eq!(Json::number(0.0).as_i64(), OptionI64::Some(0));
        assert_eq!(Json::number(-0.0).as_i64(), OptionI64::Some(0));
        assert_eq!(Json::number(f64::MAX).as_number(), OptionF64::Some(f64::MAX));
        assert_eq!(Json::number(f64::MIN).as_number(), OptionF64::Some(f64::MIN));
        // Huge-but-finite values are numbers, but not integers.
        assert_eq!(Json::number(f64::MAX).as_i64(), OptionI64::None);
    }

    #[test]
    fn integer_min_round_trips_but_max_does_not() {
        // i64::MIN == -2^63 is exactly representable in f64.
        assert_eq!(Json::integer(i64::MIN).as_i64(), OptionI64::Some(i64::MIN));
        // i64::MAX is NOT: `value as f64` rounds it up to 2^63, which is out of
        // i64 range, so the value cannot be read back. Documented on the fn as
        // "silent precision loss" for |value| > 2^53.
        assert_eq!(Json::integer(i64::MAX).as_i64(), OptionI64::None);
        assert_eq!(
            Json::integer(i64::MAX).as_number(),
            OptionF64::Some(two_pow_63())
        );
    }

    #[test]
    fn integer_silently_loses_precision_above_2_pow_53() {
        let boundary = 1_i64 << 53; // 9_007_199_254_740_992
        assert_eq!(Json::integer(boundary).as_i64(), OptionI64::Some(boundary));
        // 2^53 + 1 is not representable: it rounds *down* to 2^53.
        assert_eq!(
            Json::integer(boundary + 1).as_i64(),
            OptionI64::Some(boundary)
        );
        assert_eq!(Json::integer(0).as_i64(), OptionI64::Some(0));
        assert_eq!(Json::integer(-1).as_i64(), OptionI64::Some(-1));
    }

    // ==================================================================
    // constructors + predicates  (predicate: basic_true_false / edge_inputs)
    // ==================================================================

    #[test]
    fn predicates_are_mutually_exclusive_for_every_type() {
        let cases = [
            (Json::null(), JsonType::Null),
            (Json::bool(false), JsonType::Bool),
            (Json::number(f64::NAN), JsonType::Number),
            (Json::integer(0), JsonType::Number),
            (Json::string(""), JsonType::String),
            (raw(JsonType::Array, "[]"), JsonType::Array),
            (raw(JsonType::Object, "{}"), JsonType::Object),
        ];
        for (j, ty) in &cases {
            let flags = [
                j.is_null(),
                j.is_bool(),
                j.is_number(),
                j.is_string(),
                j.is_array(),
                j.is_object(),
            ];
            assert_eq!(
                flags.iter().filter(|b| **b).count(),
                1,
                "exactly one predicate must hold for {ty:?}"
            );
            assert_eq!(j.value_type, *ty);
        }
    }

    #[test]
    fn predicates_on_a_default_internal_do_not_panic() {
        // A hand-rolled Json with a default (empty) payload — the FFI worst case.
        for ty in [
            JsonType::Null,
            JsonType::Bool,
            JsonType::Number,
            JsonType::String,
            JsonType::Array,
            JsonType::Object,
        ] {
            let j = Json {
                value_type: ty,
                internal: JsonInternal::default(),
            };
            assert_eq!(j.is_null(), ty == JsonType::Null);
            assert_eq!(j.is_object(), ty == JsonType::Object);
            assert_eq!(j.raw_string(), "");
        }
    }

    #[test]
    fn bool_constructor_keeps_both_values() {
        assert_eq!(Json::bool(true).as_bool(), OptionBool::Some(true));
        assert_eq!(Json::bool(false).as_bool(), OptionBool::Some(false));
        // The unused payload fields are zeroed, which the derived PartialEq relies on.
        assert_eq!(Json::bool(true).raw_string(), "");
        assert_eq!(Json::bool(true).as_number(), OptionF64::None);
    }

    #[test]
    fn string_constructor_handles_empty_unicode_and_huge_inputs() {
        assert_eq!(Json::string("").as_string(), OptionString::Some(az("")));

        let unicode = "😀 náïve \u{0301}\u{202e}\0 中文";
        assert_eq!(
            Json::string(unicode).as_string(),
            OptionString::Some(az(unicode))
        );
        assert_eq!(Json::string(unicode).raw_string(), unicode);

        let huge = "a".repeat(1_000_000);
        let j = Json::string(huge.clone());
        assert_eq!(j.raw_string().len(), 1_000_000);
        assert_eq!(j.as_string(), OptionString::Some(AzString::from(huge)));
    }

    // ==================================================================
    // getters  (getter: basic_access / edge_access)
    // ==================================================================

    #[test]
    fn getters_return_none_on_type_mismatch() {
        let null = Json::null();
        assert_eq!(null.as_bool(), OptionBool::None);
        assert_eq!(null.as_number(), OptionF64::None);
        assert_eq!(null.as_i64(), OptionI64::None);
        assert_eq!(null.as_string(), OptionString::None);

        // A Bool whose *number* payload happens to be set must still not be
        // readable as a number (the tag, not the payload, decides).
        let mut liar = Json::bool(true);
        liar.internal.number_value = 7.0;
        liar.internal.string_value = az("7");
        assert_eq!(liar.as_number(), OptionF64::None);
        assert_eq!(liar.as_i64(), OptionI64::None);
        assert_eq!(liar.as_string(), OptionString::None);
        assert_eq!(liar.as_bool(), OptionBool::Some(true));
        // ...but raw_string() is the *unchecked* accessor and does hand it back.
        assert_eq!(liar.raw_string(), "7");
    }

    #[test]
    fn raw_string_is_empty_for_scalars_and_serialized_for_containers() {
        assert_eq!(Json::null().raw_string(), "");
        assert_eq!(Json::bool(true).raw_string(), "");
        assert_eq!(Json::number(1.5).raw_string(), "");
        assert_eq!(Json::string("hi").raw_string(), "hi");
        assert_eq!(raw(JsonType::Array, "[1,2]").raw_string(), "[1,2]");
        // Corrupt payloads are handed back verbatim, never panic.
        assert_eq!(raw(JsonType::Object, "{not json").raw_string(), "{not json");
    }

    // ==================================================================
    // Display for Json  (round_trip / serializer)
    // ==================================================================

    #[test]
    fn display_scalar_values() {
        assert_eq!(alloc::format!("{}", Json::null()), "null");
        assert_eq!(alloc::format!("{}", Json::bool(true)), "true");
        assert_eq!(alloc::format!("{}", Json::bool(false)), "false");
        // Integral floats print without a fractional part (via f64_as_i64).
        assert_eq!(alloc::format!("{}", Json::number(3.0)), "3");
        assert_eq!(alloc::format!("{}", Json::integer(-42)), "-42");
        assert_eq!(alloc::format!("{}", Json::number(1.5)), "1.5");
        assert_eq!(alloc::format!("{}", Json::string("x")), "\"x\"");
    }

    #[test]
    fn display_of_non_finite_numbers_is_not_json() {
        // Characterization: Display is documented as human-readable, NOT JSON.
        assert_eq!(alloc::format!("{}", Json::number(f64::NAN)), "NaN");
        assert_eq!(alloc::format!("{}", Json::number(f64::INFINITY)), "inf");
        assert_eq!(alloc::format!("{}", Json::number(f64::NEG_INFINITY)), "-inf");
        // -0.0 loses its sign because f64_as_i64(-0.0) == Some(0).
        assert_eq!(alloc::format!("{}", Json::number(-0.0)), "0");
    }

    #[test]
    fn display_does_not_escape_strings() {
        // Documented caveat on the Display impl: embedded quotes/newlines are
        // NOT escaped, so the output is deliberately not valid JSON.
        let j = Json::string("a\"b\nc");
        assert_eq!(alloc::format!("{j}"), "\"a\"b\nc\"");
    }

    #[test]
    fn display_of_container_emits_the_raw_payload_even_when_corrupt() {
        assert_eq!(
            alloc::format!("{}", raw(JsonType::Array, "[1, 2]")),
            "[1, 2]"
        );
        assert_eq!(
            alloc::format!("{}", raw(JsonType::Object, "<<garbage>>")),
            "<<garbage>>"
        );
        assert_eq!(alloc::format!("{}", raw(JsonType::Array, "")), "");
    }

    // ==================================================================
    // JsonParseError::fmt  (serializer)
    // ==================================================================

    #[test]
    fn parse_error_display_with_and_without_position() {
        let with_pos = JsonParseError {
            message: az("expected value"),
            line: 3,
            column: 7,
        };
        assert_eq!(alloc::format!("{with_pos}"), "3:7: expected value");

        let no_pos = JsonParseError {
            message: az("expected value"),
            line: 0,
            column: 99,
        };
        assert_eq!(alloc::format!("{no_pos}"), "expected value");
    }

    #[test]
    fn parse_error_display_edge_values_do_not_panic() {
        let empty = JsonParseError {
            message: az(""),
            line: 0,
            column: 0,
        };
        assert_eq!(alloc::format!("{empty}"), "");

        let maxed = JsonParseError {
            message: az("😀"),
            line: u32::MAX,
            column: u32::MAX,
        };
        assert_eq!(
            alloc::format!("{maxed}"),
            alloc::format!("{}:{}: 😀", u32::MAX, u32::MAX)
        );
    }

    // ==================================================================
    // JsonKeyValue::create  (other: no_panic_smoke)
    // ==================================================================

    #[test]
    fn key_value_create_preserves_key_and_value() {
        let kv = JsonKeyValue::create(az(""), Json::null());
        assert_eq!(kv.key.as_str(), "");
        assert!(kv.value.is_null());

        let big_key = "k".repeat(100_000);
        let kv = JsonKeyValue::create(
            AzString::from(big_key.clone()),
            Json::number(f64::NEG_INFINITY),
        );
        assert_eq!(kv.key.as_str().len(), big_key.len());
        assert!(kv.value.is_number());

        let kv = JsonKeyValue::create(az("😀/\u{0}"), Json::string("v"));
        assert_eq!(kv.key.as_str(), "😀/\u{0}");
        assert_eq!(kv.value.as_string(), OptionString::Some(az("v")));
    }

    // ==================================================================
    // copy_from_array  (numeric: zero / min_max / overflow)
    // ==================================================================

    #[test]
    fn json_vec_copy_from_array_null_ptr_is_empty_even_at_usize_max_len() {
        // The null check runs first, so a bogus (null, huge) pair from C must
        // yield an empty vec rather than constructing a wild slice.
        let v = JsonVec::copy_from_array(core::ptr::null(), 0);
        assert!(v.is_empty());
        let v = JsonVec::copy_from_array(core::ptr::null(), usize::MAX);
        assert!(v.is_empty());
        assert_eq!(v.len(), 0);
    }

    #[test]
    fn json_vec_copy_from_array_zero_len_with_valid_ptr_is_empty() {
        let items = [Json::null(), Json::bool(true)];
        let v = JsonVec::copy_from_array(items.as_ptr(), 0);
        assert!(v.is_empty());
    }

    #[test]
    fn json_vec_copy_from_array_deep_copies_the_elements() {
        let items = vec![
            Json::null(),
            Json::bool(true),
            Json::number(f64::NAN),
            Json::string("😀"),
        ];
        let v = JsonVec::copy_from_array(items.as_ptr(), items.len());
        assert_eq!(v.len(), 4);
        assert_eq!(v.as_slice()[3].as_string(), OptionString::Some(az("😀")));
        // The copy is independent: dropping the source must not invalidate it.
        drop(items);
        assert!(v.as_slice()[0].is_null());
        assert_eq!(v.as_slice()[1].as_bool(), OptionBool::Some(true));
        assert_eq!(v.as_slice()[3].raw_string(), "😀");
    }

    #[test]
    fn key_value_vec_copy_from_array_null_ptr_is_empty_even_at_usize_max_len() {
        let v = JsonKeyValueVec::copy_from_array(core::ptr::null(), 0);
        assert!(v.is_empty());
        let v = JsonKeyValueVec::copy_from_array(core::ptr::null(), usize::MAX);
        assert!(v.is_empty());
        assert_eq!(v.len(), 0);
    }

    #[test]
    fn key_value_vec_copy_from_array_deep_copies_the_elements() {
        let items = vec![
            JsonKeyValue::create(az("a"), Json::integer(1)),
            JsonKeyValue::create(az(""), Json::null()),
        ];
        let v = JsonKeyValueVec::copy_from_array(items.as_ptr(), items.len());
        assert_eq!(v.len(), 2);
        let zero_len = JsonKeyValueVec::copy_from_array(items.as_ptr(), 0);
        assert!(zero_len.is_empty());
        drop(items);
        assert_eq!(v.as_slice()[0].key.as_str(), "a");
        assert_eq!(v.as_slice()[0].value.as_i64(), OptionI64::Some(1));
        assert_eq!(v.as_slice()[1].key.as_str(), "");
    }

    // ==================================================================
    // Json::parse / parse_bytes — malformed input
    // ==================================================================

    #[test]
    #[cfg(feature = "serde-json")]
    fn parse_empty_and_whitespace_only_input_is_an_error() {
        for s in ["", " ", "   ", "\t\n\r", "\u{feff}"] {
            let err = Json::parse(s).expect_err("empty/blank input must not parse");
            assert!(!err.message.as_str().is_empty());
            assert!(Json::parse_bytes(s.as_bytes()).is_err());
        }
        assert!(Json::parse_bytes(b"").is_err());
    }

    #[test]
    #[cfg(feature = "serde-json")]
    fn parse_garbage_returns_err_and_never_panics() {
        for s in [
            "{", "}", "[", "]", ",", ":", "nul", "tru", "'a'", "{,}", "[,]", "{\"a\"}", "{\"a\":}",
            "[1,]", "{\"a\":1,}", "\"unterminated", "\\", "\u{0}", "01", "+1", ".5", "1.", "-",
            "0x10", "--1", "1e", "{'a':1}", "undefined",
        ] {
            assert!(Json::parse(s).is_err(), "{s:?} must be rejected");
            assert!(Json::parse_bytes(s.as_bytes()).is_err(), "{s:?} (bytes)");
        }
    }

    #[test]
    #[cfg(feature = "serde-json")]
    fn parse_leading_and_trailing_junk() {
        // Surrounding whitespace is allowed and trimmed...
        assert_eq!(Json::parse("  1  ").expect("padded"), Json::integer(1));
        assert_eq!(Json::parse("\n\t{}\r\n").expect("padded"), Json::parse("{}").expect("{}"));
        // ...but trailing non-whitespace is not.
        for s in ["1;garbage", "{} {}", "null null", "1 2", "[1] x"] {
            assert!(Json::parse(s).is_err(), "{s:?} must be rejected");
        }
    }

    #[test]
    #[cfg(feature = "serde-json")]
    fn parse_bytes_rejects_invalid_utf8_without_panicking() {
        assert!(Json::parse_bytes(&[0xFF, 0xFE, 0x00]).is_err());
        // Structurally valid JSON, but the string body is not UTF-8.
        assert!(Json::parse_bytes(&[b'"', 0xFF, b'"']).is_err());
        // Lone continuation byte inside an otherwise fine document.
        assert!(Json::parse_bytes(&[b'[', 0x80, b']']).is_err());
    }

    #[test]
    #[cfg(feature = "serde-json")]
    fn parse_deeply_nested_input_errors_instead_of_overflowing_the_stack() {
        // serde_json's 128-level recursion cap turns this into an Err.
        let deep = alloc::format!("{}{}", "[".repeat(10_000), "]".repeat(10_000));
        assert!(Json::parse(&deep).is_err());
        assert!(Json::parse_bytes(deep.as_bytes()).is_err());

        let deep_obj = alloc::format!("{}1{}", "{\"a\":".repeat(10_000), "}".repeat(10_000));
        assert!(Json::parse(&deep_obj).is_err());

        // Unbalanced (never-closing) nesting must also terminate.
        let unbalanced = "[".repeat(100_000);
        assert!(Json::parse(&unbalanced).is_err());
    }

    #[test]
    #[cfg(feature = "serde-json")]
    fn parse_extremely_long_input_terminates() {
        const N: usize = 200_000;
        let mut doc = String::with_capacity(N * 2 + 2);
        doc.push('[');
        for i in 0..N {
            if i > 0 {
                doc.push(',');
            }
            doc.push('1');
        }
        doc.push(']');
        let j = Json::parse(&doc).expect("a long flat array must parse");
        assert!(j.is_array());
        assert_eq!(j.len(), N);

        // A ~1 MB string payload.
        let payload = "a".repeat(1_000_000);
        let j = Json::parse(&alloc::format!("\"{payload}\"")).expect("long string");
        assert_eq!(j.as_string(), OptionString::Some(AzString::from(payload)));
    }

    #[test]
    #[cfg(feature = "serde-json")]
    fn parse_unicode_input() {
        let j = Json::parse("\"\u{1F600}\"").expect("emoji");
        assert_eq!(j.as_string(), OptionString::Some(az("\u{1F600}")));

        // Combining marks + RTL override + escaped NUL survive the round trip.
        let j = Json::parse("\"e\\u0301\\u202e\\u0000\"").expect("escapes");
        assert_eq!(
            j.as_string(),
            OptionString::Some(az("e\u{0301}\u{202e}\u{0}"))
        );

        // Non-ASCII keys.
        let j = Json::parse("{\"ключ\":\"значение\"}").expect("cyrillic keys");
        assert_eq!(
            j.get_key("ключ").expect("key").as_string(),
            OptionString::Some(az("значение"))
        );

        // A lone surrogate is not encodable as UTF-8 and must be rejected.
        assert!(Json::parse("\"\\ud800\"").is_err());
    }

    #[test]
    #[cfg(feature = "serde-json")]
    fn parse_boundary_numbers() {
        assert_eq!(Json::parse("0").expect("0").as_i64(), OptionI64::Some(0));
        // "-0" is accepted; the sign is not observable through as_number().
        match Json::parse("-0").expect("-0").as_number() {
            OptionF64::Some(v) => assert_eq!(v, 0.0),
            OptionF64::None => panic!("-0 must be a number"),
        }

        // i64::MIN is exactly representable in f64 and reads back exactly.
        assert_eq!(
            Json::parse("-9223372036854775808").expect("i64::MIN").as_i64(),
            OptionI64::Some(i64::MIN)
        );
        // i64::MAX is NOT: it rounds up to 2^63 on the f64 hop through
        // `Number::as_f64()`, so as_i64() reports None (silent precision loss).
        let max = Json::parse("9223372036854775807").expect("i64::MAX");
        assert_eq!(max.as_number(), OptionF64::Some(two_pow_63()));
        assert_eq!(max.as_i64(), OptionI64::None);
        // Same for u64::MAX.
        assert_eq!(
            Json::parse("18446744073709551615").expect("u64::MAX").as_i64(),
            OptionI64::None
        );

        assert!(Json::parse("1e308").expect("1e308").is_number());
        assert!(Json::parse("1e-308").expect("1e-308").is_number());

        // JSON has no NaN/Infinity literals.
        for s in ["NaN", "nan", "Infinity", "-Infinity", "inf"] {
            assert!(Json::parse(s).is_err(), "{s:?} is not valid JSON");
        }

        // Overflowing exponents must not panic; whatever serde decides, the
        // result is a deterministic Ok(number) or Err.
        if let Ok(j) = Json::parse("1e400") {
            assert!(j.is_number());
        }
        if let Ok(j) = Json::parse("1e-400") {
            assert!(j.is_number());
        }
    }

    #[test]
    #[cfg(feature = "serde-json")]
    fn parse_valid_minimal_inputs() {
        assert_eq!(Json::parse("null").expect("null"), Json::null());
        assert_eq!(Json::parse("true").expect("true"), Json::bool(true));
        assert_eq!(Json::parse("false").expect("false"), Json::bool(false));
        assert_eq!(Json::parse("1").expect("1"), Json::integer(1));
        assert_eq!(Json::parse("1.5").expect("1.5"), Json::number(1.5));
        assert_eq!(Json::parse("\"s\"").expect("str"), Json::string("s"));
        assert!(Json::parse("[]").expect("[]").is_array());
        assert!(Json::parse("{}").expect("{}").is_object());
        // parse_bytes agrees with parse.
        assert_eq!(
            Json::parse_bytes(b"{\"a\":[1,2]}").expect("bytes"),
            Json::parse("{\"a\":[1,2]}").expect("str")
        );
    }

    // ==================================================================
    // round trips  (round_trip: representative / edge / stable)
    // ==================================================================

    #[cfg(feature = "serde-json")]
    fn round_trip_corpus() -> Vec<Json> {
        vec![
            Json::null(),
            Json::bool(true),
            Json::bool(false),
            Json::integer(0),
            Json::integer(-1),
            Json::integer(i64::MIN),
            Json::number(1.5),
            Json::number(-0.25),
            Json::number(1e21),
            Json::string(""),
            Json::string("😀 \"quoted\" \\slash\\ \n\t \u{0}"),
            Json::parse("[]").expect("[]"),
            Json::parse("{}").expect("{}"),
            Json::parse("[1,[2,[3]],{\"k\":null}]").expect("nested array"),
            Json::parse("{\"a\":{\"b\":[1,2,3]},\"ünï\":\"😀\"}").expect("nested object"),
        ]
    }

    #[test]
    #[cfg(feature = "serde-json")]
    fn parse_of_to_json_string_reproduces_the_value() {
        for j in round_trip_corpus() {
            let encoded = j.to_json_string();
            let decoded = Json::parse(encoded.as_str())
                .unwrap_or_else(|e| panic!("{encoded:?} must re-parse: {e}"));
            assert_eq!(decoded, j, "round trip failed for {encoded:?}");
        }
    }

    #[test]
    #[cfg(feature = "serde-json")]
    fn serialize_parse_serialize_is_idempotent() {
        for j in round_trip_corpus() {
            let once = j.to_json_string();
            let twice = Json::parse(once.as_str()).expect("re-parse").to_json_string();
            assert_eq!(once.as_str(), twice.as_str());
        }
    }

    #[test]
    #[cfg(feature = "serde-json")]
    fn round_trip_extreme_floats() {
        for n in [f64::MAX, f64::MIN, f64::MIN_POSITIVE, -f64::MIN_POSITIVE] {
            let j = Json::number(n);
            let encoded = j.to_json_string();
            let decoded = Json::parse(encoded.as_str())
                .unwrap_or_else(|e| panic!("{encoded:?} must re-parse: {e}"));
            assert_eq!(decoded.as_number(), OptionF64::Some(n));
        }
    }

    #[test]
    #[cfg(feature = "serde-json")]
    fn non_finite_numbers_do_not_survive_to_json_string() {
        // BUG (characterized, not fixed here): `to_json_string()` promises valid
        // JSON, but for a non-finite `number_value` it falls back to Rust's float
        // Display and emits the bare tokens `NaN` / `inf` / `-inf`, which no JSON
        // parser accepts. `to_serde_value()` handles the same input correctly by
        // mapping it to `null`. Callers must therefore not assume
        // `parse(to_json_string(x))` succeeds for a hand-built non-finite number.
        for (n, token) in [
            (f64::NAN, "NaN"),
            (f64::INFINITY, "inf"),
            (f64::NEG_INFINITY, "-inf"),
        ] {
            let j = Json::number(n);
            let encoded = j.to_json_string();
            assert_eq!(encoded.as_str(), token);
            assert!(
                Json::parse(encoded.as_str()).is_err(),
                "{token} is not valid JSON"
            );
            // The serde path degrades safely instead.
            assert_eq!(j.to_serde_value(), serde_json::Value::Null);
        }
    }

    // ==================================================================
    // from_serde_value / to_serde_value
    // ==================================================================

    #[test]
    #[cfg(feature = "serde-json")]
    fn from_serde_value_maps_every_variant() {
        use serde_json::json;

        assert!(Json::from_serde_value(json!(null)).is_null());
        assert_eq!(
            Json::from_serde_value(json!(true)).as_bool(),
            OptionBool::Some(true)
        );
        assert_eq!(
            Json::from_serde_value(json!(-3)).as_i64(),
            OptionI64::Some(-3)
        );
        assert_eq!(
            Json::from_serde_value(json!("😀")).as_string(),
            OptionString::Some(az("😀"))
        );

        let arr = Json::from_serde_value(json!([1, "two", null]));
        assert!(arr.is_array());
        assert_eq!(arr.len(), 3);
        assert_eq!(arr.get_index(1).expect("idx 1"), Json::string("two"));

        let obj = Json::from_serde_value(json!({"a": 1, "b": {"c": []}}));
        assert!(obj.is_object());
        assert_eq!(obj.len(), 2);
        assert_eq!(obj.get_key("a").expect("a"), Json::integer(1));
        // The serialized payload must itself be valid JSON (invariant).
        assert!(Json::parse(obj.raw_string()).is_ok());
    }

    #[test]
    #[cfg(feature = "serde-json")]
    fn to_serde_value_round_trips_through_from_serde_value() {
        use serde_json::json;

        for v in [
            json!(null),
            json!(false),
            json!(0),
            json!(-1.5),
            json!(""),
            json!([]),
            json!({}),
            json!({"k": [1, {"n": null}], "ü": "😀"}),
        ] {
            let back = Json::from_serde_value(v.clone()).to_serde_value();
            assert_eq!(back, v);
        }
    }

    #[test]
    #[cfg(feature = "serde-json")]
    fn to_serde_value_on_a_corrupt_container_yields_null() {
        assert_eq!(
            raw(JsonType::Array, "not json").to_serde_value(),
            serde_json::Value::Null
        );
        assert_eq!(
            raw(JsonType::Object, "").to_serde_value(),
            serde_json::Value::Null
        );
    }

    // ==================================================================
    // Json::array / Json::object  (other: no_panic_smoke)
    // ==================================================================

    #[test]
    #[cfg(feature = "serde-json")]
    fn array_constructor_handles_empty_and_non_finite_members() {
        let empty = Json::array(JsonVec::new());
        assert!(empty.is_array());
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
        assert_eq!(empty.raw_string(), "[]");

        // A NaN member cannot be represented in JSON — it degrades to null
        // (via to_serde_value) rather than producing invalid output or panicking.
        let with_nan = Json::array(JsonVec::from_vec(vec![
            Json::number(f64::NAN),
            Json::number(f64::INFINITY),
            Json::integer(1),
        ]));
        assert_eq!(with_nan.raw_string(), "[null,null,1]");
        assert_eq!(with_nan.len(), 3);
        assert!(Json::parse(with_nan.raw_string()).is_ok());
    }

    #[test]
    #[cfg(feature = "serde-json")]
    fn object_constructor_dedupes_duplicate_keys_last_one_wins() {
        let empty = Json::object(JsonKeyValueVec::new());
        assert!(empty.is_object());
        assert!(empty.is_empty());
        assert_eq!(empty.raw_string(), "{}");

        let dup = Json::object(JsonKeyValueVec::from_vec(vec![
            JsonKeyValue::create(az("k"), Json::integer(1)),
            JsonKeyValue::create(az("k"), Json::integer(2)),
            JsonKeyValue::create(az(""), Json::null()),
        ]));
        assert_eq!(dup.len(), 2, "duplicate keys collapse into one entry");
        assert_eq!(dup.get_key("k").expect("k"), Json::integer(2));
        assert!(dup.get_key("").expect("empty key").is_null());
    }

    #[test]
    #[cfg(feature = "serde-json")]
    fn object_constructor_escapes_hostile_keys() {
        let obj = Json::object(JsonKeyValueVec::from_vec(vec![JsonKeyValue::create(
            az("\"}\n😀"),
            Json::string("v"),
        )]));
        // The payload must still be parseable JSON — i.e. the key was escaped.
        let reparsed = Json::parse(obj.raw_string()).expect("hostile key must be escaped");
        assert_eq!(
            reparsed.get_key("\"}\n😀").expect("key").as_string(),
            OptionString::Some(az("v"))
        );
    }

    // ==================================================================
    // len / is_empty  (getter + predicate)
    // ==================================================================

    #[test]
    #[cfg(feature = "serde-json")]
    fn len_is_zero_for_scalars_which_makes_is_empty_true() {
        // Characterization: len()/is_empty() are documented "for arrays/objects";
        // for scalars they report 0 / true, so `is_empty()` is NOT "has no value".
        for j in [
            Json::null(),
            Json::bool(true),
            Json::integer(7),
            Json::string("hello"),
        ] {
            assert_eq!(j.len(), 0);
            assert!(j.is_empty());
        }
    }

    #[test]
    #[cfg(feature = "serde-json")]
    fn len_of_containers_and_corrupt_payloads() {
        assert_eq!(Json::parse("[1,2,3]").expect("arr").len(), 3);
        assert_eq!(Json::parse("{\"a\":1}").expect("obj").len(), 1);
        assert_eq!(Json::parse("[]").expect("[]").len(), 0);
        // Corrupt payload → 0 rather than a panic.
        assert_eq!(raw(JsonType::Array, "not json").len(), 0);
        assert!(raw(JsonType::Object, "").is_empty());
        // Tag/payload mismatch (Array tag over an object payload) → 0.
        assert_eq!(raw(JsonType::Array, "{\"a\":1}").len(), 0);
    }

    // ==================================================================
    // get_index / get_key / keys / to_array / to_object
    // ==================================================================

    #[test]
    #[cfg(feature = "serde-json")]
    fn get_index_boundaries() {
        let arr = Json::parse("[10,20]").expect("arr");
        assert_eq!(arr.get_index(0).expect("0"), Json::integer(10));
        assert_eq!(arr.get_index(1).expect("1"), Json::integer(20));
        assert!(arr.get_index(2).is_none());
        assert!(arr.get_index(usize::MAX).is_none());

        assert!(Json::parse("[]").expect("[]").get_index(0).is_none());
        // Non-arrays never index, whatever the payload says.
        assert!(Json::parse("{\"0\":1}").expect("obj").get_index(0).is_none());
        assert!(Json::string("abc").get_index(0).is_none());
        assert!(raw(JsonType::Array, "not json").get_index(0).is_none());
        assert!(raw(JsonType::Array, "{\"a\":1}").get_index(0).is_none());
    }

    #[test]
    #[cfg(feature = "serde-json")]
    fn get_key_edge_inputs() {
        let obj = Json::parse("{\"\":1,\"a\":null,\"😀\":[]}").expect("obj");
        assert_eq!(obj.get_key("").expect("empty key"), Json::integer(1));
        assert!(obj.get_key("a").expect("a").is_null());
        assert!(obj.get_key("😀").expect("emoji").is_array());
        assert!(obj.get_key("missing").is_none());
        // Keys are exact, not prefix/trimmed matches.
        assert!(obj.get_key(" a").is_none());
        assert!(obj.get_key("A").is_none());
        // A huge key must not panic.
        assert!(obj.get_key(&"k".repeat(1_000_000)).is_none());
        // Non-objects always return None.
        assert!(Json::parse("[1]").expect("arr").get_key("0").is_none());
        assert!(Json::null().get_key("").is_none());
        assert!(raw(JsonType::Object, "not json").get_key("a").is_none());
    }

    #[test]
    #[cfg(feature = "serde-json")]
    fn keys_returns_every_key_and_nothing_for_non_objects() {
        let obj = Json::parse("{\"b\":1,\"a\":2,\"\":3}").expect("obj");
        let keys = obj.keys();
        assert_eq!(keys.len(), 3);
        for expected in ["a", "b", ""] {
            assert!(
                keys.iter().any(|k| k.as_str() == expected),
                "missing key {expected:?}"
            );
        }
        assert!(Json::parse("{}").expect("{}").keys().is_empty());
        assert!(Json::parse("[1,2]").expect("arr").keys().is_empty());
        assert!(Json::string("x").keys().is_empty());
        assert!(raw(JsonType::Object, "not json").keys().is_empty());
    }

    #[test]
    #[cfg(feature = "serde-json")]
    fn to_array_and_to_object_on_wrong_types_and_corrupt_payloads() {
        let arr = Json::parse("[null,1]").expect("arr").to_array().expect("to_array");
        assert_eq!(arr.len(), 2);
        assert!(arr.as_slice()[0].is_null());
        assert_eq!(arr.as_slice()[1].as_i64(), OptionI64::Some(1));

        let obj = Json::parse("{\"a\":\"v\"}").expect("obj").to_object().expect("to_object");
        assert_eq!(obj.len(), 1);
        assert_eq!(obj.as_slice()[0].key.as_str(), "a");
        assert_eq!(
            obj.as_slice()[0].value.as_string(),
            OptionString::Some(az("v"))
        );

        assert!(Json::parse("[]").expect("[]").to_array().expect("empty").is_empty());
        assert!(Json::parse("{}").expect("{}").to_object().expect("empty").is_empty());

        // Wrong type / corrupt payload / tag mismatch → None, never a panic.
        assert!(Json::null().to_array().is_none());
        assert!(Json::string("[]").to_array().is_none());
        assert!(Json::parse("[]").expect("[]").to_object().is_none());
        assert!(raw(JsonType::Array, "not json").to_array().is_none());
        assert!(raw(JsonType::Object, "").to_object().is_none());
        assert!(raw(JsonType::Array, "{\"a\":1}").to_array().is_none());
        assert!(raw(JsonType::Object, "[1]").to_object().is_none());
    }

    // ==================================================================
    // to_json_string / to_string_pretty
    // ==================================================================

    #[test]
    #[cfg(feature = "serde-json")]
    fn to_json_string_escapes_strings_unlike_display() {
        let j = Json::string("a\"b\nc\\d\u{0}");
        let encoded = j.to_json_string();
        // Valid JSON that re-parses to the identical value...
        assert_eq!(Json::parse(encoded.as_str()).expect("escaped"), j);
        // ...and it differs from the (documented as non-JSON) Display output.
        assert_ne!(encoded.as_str(), alloc::format!("{j}"));
    }

    #[test]
    #[cfg(feature = "serde-json")]
    fn to_string_pretty_matches_to_json_string_for_scalars() {
        for j in [
            Json::null(),
            Json::bool(false),
            Json::integer(-7),
            Json::number(0.5),
            Json::string("😀"),
        ] {
            assert_eq!(j.to_string_pretty().as_str(), j.to_json_string().as_str());
        }
    }

    #[test]
    #[cfg(feature = "serde-json")]
    fn to_string_pretty_of_containers_reparses_to_the_same_value() {
        let j = Json::parse("{\"a\":[1,{\"b\":null}],\"c\":{}}").expect("obj");
        let pretty = j.to_string_pretty();
        assert!(pretty.as_str().contains('\n'), "pretty output must be indented");
        assert_eq!(Json::parse(pretty.as_str()).expect("pretty reparses"), j);

        // A corrupt payload is passed through verbatim instead of panicking.
        let corrupt = raw(JsonType::Object, "not json");
        assert_eq!(corrupt.to_string_pretty().as_str(), "not json");
        assert_eq!(raw(JsonType::Array, "").to_string_pretty().as_str(), "");
    }

    // ==================================================================
    // jq / jq_all  (other: no_panic_smoke)
    // ==================================================================

    #[test]
    #[cfg(feature = "serde-json")]
    fn jq_on_scalars_only_matches_the_empty_pointer() {
        for j in [
            Json::null(),
            Json::bool(true),
            Json::integer(1),
            Json::string("s"),
        ] {
            assert_eq!(j.jq(""), j);
            assert!(j.jq("/").is_null());
            assert!(j.jq("/a").is_null());
            assert!(j.jq("nonsense").is_null());
            assert_eq!(j.jq_all("").as_ref().len(), 1);
            assert_eq!(j.jq_all("/a").as_ref().len(), 0);
        }
    }

    #[test]
    #[cfg(feature = "serde-json")]
    fn jq_navigates_and_degrades_to_null() {
        let j = Json::parse("{\"a\":{\"b\":[10,20]},\"\":1}").expect("doc");
        assert_eq!(j.jq(""), j, "the empty pointer selects the whole document");
        assert_eq!(j.jq("/a/b/1"), Json::integer(20));
        assert_eq!(j.jq("/"), Json::integer(1), "'/' selects the empty-string key");

        // Misses / malformed pointers / hostile input → null, never a panic.
        assert!(j.jq("/a/b/2").is_null(), "out-of-range index");
        assert!(j.jq("/a/b/-1").is_null(), "negative index is not a usize");
        assert!(j.jq("/a/b/99999999999999999999").is_null(), "index overflows usize");
        assert!(j.jq("a/b").is_null(), "pointer must start with '/'");
        assert!(j.jq("/missing").is_null());
        assert!(j.jq(&"/a".repeat(100_000)).is_null(), "huge pointer");
        assert!(j.jq("/a/b/1/deeper").is_null(), "descending through a scalar");
        assert!(raw(JsonType::Object, "not json").jq("/a").is_null());
    }

    #[test]
    #[cfg(feature = "serde-json")]
    fn jq_all_wildcards_and_empty_results() {
        let j = Json::parse("{\"a\":[{\"v\":1},{\"v\":2}],\"b\":{\"v\":3},\"c\":7}").expect("doc");

        assert_eq!(j.jq_all("/a/*/v").as_ref().len(), 2);
        // A wildcard over an object iterates its values; scalars contribute nothing.
        assert_eq!(j.jq_all("/*/v").as_ref().len(), 1);
        assert_eq!(j.jq_all("/*").as_ref().len(), 3);
        assert_eq!(j.jq_all("").as_ref().len(), 1);

        // No match / malformed / corrupt → empty vec.
        assert_eq!(j.jq_all("/missing/*").as_ref().len(), 0);
        assert_eq!(j.jq_all("a").as_ref().len(), 0);
        assert_eq!(j.jq_all("/c/*").as_ref().len(), 0, "wildcard over a scalar");
        assert_eq!(j.jq_all("/*/*/*/*/*").as_ref().len(), 0);
        assert_eq!(
            raw(JsonType::Array, "not json").jq_all("/*").as_ref().len(),
            0
        );

        // A pointer that is only wildcards, far longer than the document is deep.
        let many = "/*".repeat(10_000);
        assert_eq!(j.jq_all(&many).as_ref().len(), 0);
    }

    // ==================================================================
    // jq_all_recursive_depth  (numeric: zero / min_max / overflow)
    // ==================================================================

    #[test]
    #[cfg(feature = "serde-json")]
    fn jq_all_recursive_depth_honours_the_wildcard_cap() {
        let value = serde_json::json!({"a": 1});

        // depth 0 (what jq_all_recursive passes) resolves normally.
        assert_eq!(Json::jq_all_recursive_depth(&value, "/a", 0).len(), 1);
        // Exactly at the cap it still resolves...
        assert_eq!(
            Json::jq_all_recursive_depth(&value, "/a", Json::JQ_MAX_WILDCARD_DEPTH).len(),
            1
        );
        // ...one past it, the guard fires and yields no match instead of recursing.
        assert_eq!(
            Json::jq_all_recursive_depth(&value, "/a", Json::JQ_MAX_WILDCARD_DEPTH + 1).len(),
            0
        );
        // usize::MAX must hit the guard before the `depth + 1` in the wildcard arm,
        // so there is no add-overflow panic.
        assert_eq!(Json::jq_all_recursive_depth(&value, "/*", usize::MAX).len(), 0);
        assert_eq!(Json::jq_all_recursive_depth(&value, "", usize::MAX).len(), 0);
    }

    #[test]
    #[cfg(feature = "serde-json")]
    fn jq_all_wildcard_fanout_deeper_than_the_cap_returns_empty() {
        // Build the document programmatically: serde_json's own 128-level parse
        // cap means such a document can never come from `Json::parse`, so this is
        // the only way to drive the wildcard recursion to its 512 limit.
        fn nest(depth: usize) -> serde_json::Value {
            let mut v = serde_json::Value::from(1_i64);
            for _ in 0..depth {
                v = serde_json::Value::Array(vec![v]);
            }
            v
        }

        // Below the cap: the leaf is found.
        let shallow = nest(400);
        let found = Json::jq_all_recursive(&shallow, &"/*".repeat(400));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].as_i64(), OptionI64::Some(1));

        // Above the cap: the guard stops the recursion and returns no match.
        let deep = nest(600);
        assert!(Json::jq_all_recursive(&deep, &"/*".repeat(600)).is_empty());
    }
}
