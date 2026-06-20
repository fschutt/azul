//! JSON parsing module for C API
//!
//! Re-exports the data types and serde_json implementations from `azul_core::json`.
//! Adds RefAny serialization support on top, including C API wrapper functions
//! (`refany_serialize_to_json`, `json_deserialize_to_refany`) and function pointer
//! types (`RefAnySerializeFnType`, `RefAnyDeserializeFnType`).

// Re-export all data types and methods from core
pub use azul_core::json::*;

use alloc::string::String;
use azul_css::AzString;

// ============================================================================
// Public API Functions
// ============================================================================

/// Parse a JSON string
#[cfg(feature = "json")]
#[must_use]
pub fn json_parse(s: &str) -> Result<Json, JsonParseError> {
    Json::parse(s)
}

/// Serialize JSON to string
#[cfg(feature = "json")]
pub fn json_stringify(json: &Json) -> AzString {
    json.to_json_string()
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

impl ResultRefAnyString {
    /// Returns `true` if this is the `Ok` variant.
    pub fn is_ok(&self) -> bool {
        matches!(self, ResultRefAnyString::Ok(_))
    }

    /// Returns `true` if this is the `Err` variant.
    pub fn is_err(&self) -> bool {
        matches!(self, ResultRefAnyString::Err(_))
    }

    /// Converts into `Option<RefAny>`, discarding any error.
    pub fn ok(self) -> Option<RefAny> {
        match self {
            ResultRefAnyString::Ok(r) => Some(r),
            ResultRefAnyString::Err(_) => None,
        }
    }

    /// Converts into `Option<AzString>`, discarding any success value.
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
#[must_use]
pub fn serialize_refany_to_json(refany: &RefAny) -> Option<Json> {
    let serialize_fn = refany.get_serialize_fn();
    if serialize_fn == 0 {
        return None;
    }

    // Safety: `serialize_fn` is a valid `extern "C" fn(RefAny) -> Json` pointer,
    // set via `RefAny::set_serialize_fn`. The `!= 0` check above guards against null.
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
#[must_use]
pub fn deserialize_refany_from_json(
    json: Json,
    deserialize_fn: usize
) -> Result<RefAny, String> {
    if deserialize_fn == 0 {
        return Err("Type does not support JSON deserialization".to_string());
    }

    // Safety: `deserialize_fn` is a valid `extern "C" fn(Json) -> ResultRefAnyString` pointer,
    // set via `RefAny::set_deserialize_fn`. The `== 0` check above guards against null.
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
// Generic application-state undo/redo
// ============================================================================

/// A generic application-state undo/redo stack built on RefAny JSON snapshots.
///
/// Unlike the text-edit undo in `events.rs`, this snapshots the *whole* app
/// state `RefAny` via its registered serialize fn (`RefAny::set_serialize_fn` /
/// the `AZ_REFLECT_JSON` macro) and restores it via the deserialize fn +
/// `RefAny::replace_contents`.
///
/// Call [`snapshot`](Self::snapshot) at action boundaries (or drive it from the
/// RefAny on-update hook, `RefAny::set_update_fn`), then [`undo`](Self::undo) /
/// [`redo`](Self::redo). Snapshotting requires the state to support JSON
/// serialization; if it does not, `snapshot` is a no-op returning `false`.
#[cfg(feature = "json")]
#[derive(Debug, Clone, Default)]
pub struct RefAnyUndoManager {
    /// Past states, oldest → newest; the top is the state just before the most
    /// recent change.
    undo_stack: alloc::vec::Vec<Json>,
    /// States that were undone and can be re-applied.
    redo_stack: alloc::vec::Vec<Json>,
    /// Maximum undo depth (`0` = unlimited).
    capacity: usize,
}

#[cfg(feature = "json")]
impl RefAnyUndoManager {
    /// Creates an undo manager with a maximum depth (`0` = unlimited).
    pub fn new(capacity: usize) -> Self {
        Self {
            undo_stack: alloc::vec::Vec::new(),
            redo_stack: alloc::vec::Vec::new(),
            capacity,
        }
    }

    /// Records the current `state` as an undo point. Call BEFORE mutating (or
    /// from the on-update hook, which fires before the mutable borrow). Clears
    /// the redo stack, since a new edit starts a new branch. Returns `false`
    /// (no-op) if the state has no serialize fn registered.
    pub fn snapshot(&mut self, state: &RefAny) -> bool {
        match serialize_refany_to_json(state) {
            Some(json) => {
                self.undo_stack.push(json);
                self.redo_stack.clear();
                if self.capacity != 0 && self.undo_stack.len() > self.capacity {
                    self.undo_stack.remove(0);
                }
                true
            }
            None => false,
        }
    }

    /// True if there is a snapshot to undo to.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// True if there is an undone snapshot to redo.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Restores the most recent snapshot into `state`; the current state is
    /// pushed onto the redo stack. Returns `false` if there is nothing to undo
    /// or the state can't be (de)serialized.
    pub fn undo(&mut self, state: &mut RefAny) -> bool {
        let prev = match self.undo_stack.pop() {
            Some(p) => p,
            None => return false,
        };
        if let Some(current) = serialize_refany_to_json(state) {
            self.redo_stack.push(current);
        }
        Self::restore(state, prev)
    }

    /// Re-applies the most recently undone snapshot; the current state is pushed
    /// back onto the undo stack.
    pub fn redo(&mut self, state: &mut RefAny) -> bool {
        let next = match self.redo_stack.pop() {
            Some(n) => n,
            None => return false,
        };
        if let Some(current) = serialize_refany_to_json(state) {
            self.undo_stack.push(current);
        }
        Self::restore(state, next)
    }

    /// Drops all recorded history.
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    fn restore(state: &mut RefAny, json: Json) -> bool {
        let deser_fn = state.get_deserialize_fn();
        match deserialize_refany_from_json(json, deser_fn) {
            Ok(restored) => {
                // `replace_contents` copies the (de)serialize/update fn-ptrs from
                // `restored`, which a plain deserialize ctor leaves unset — so
                // save and re-attach the live hooks across the swap, otherwise the
                // next undo/redo would have no serializer.
                let ser_fn = state.get_serialize_fn();
                let upd_fn = state.get_update_fn();
                let ok = state.replace_contents(restored);
                state.set_serialize_fn(ser_fn);
                state.set_deserialize_fn(deser_fn);
                state.set_update_fn(upd_fn);
                ok
            }
            Err(_) => false,
        }
    }
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
        assert_eq!(json_true.as_bool().into_option(), Some(true));

        let json_false = Json::parse("false").unwrap();
        assert_eq!(json_false.as_bool().into_option(), Some(false));
    }

    #[test]
    #[cfg(feature = "json")]
    fn test_parse_number() {
        let json = Json::parse("42.5").unwrap();
        assert_eq!(json.as_number().into_option(), Some(42.5));

        let json_int = Json::parse("100").unwrap();
        assert_eq!(json_int.as_i64().into_option(), Some(100));
    }

    #[test]
    #[cfg(feature = "json")]
    fn test_parse_string() {
        let json = Json::parse("\"hello world\"").unwrap();
        assert_eq!(json.as_string().into_option().unwrap().as_str(), "hello world");
    }

    #[test]
    #[cfg(feature = "json")]
    fn test_parse_array() {
        let json = Json::parse("[1, 2, 3]").unwrap();
        assert!(json.is_array());
        assert_eq!(json.len(), 3);

        let first = json.get_index(0).unwrap();
        assert_eq!(first.as_number().into_option(), Some(1.0));
    }

    #[test]
    #[cfg(feature = "json")]
    fn test_parse_object() {
        let json = Json::parse(r#"{"name": "test", "value": 42}"#).unwrap();
        assert!(json.is_object());
        assert_eq!(json.len(), 2);

        let name = json.get_key("name").unwrap();
        assert_eq!(name.as_string().into_option().unwrap().as_str(), "test");

        let value = json.get_key("value").unwrap();
        assert_eq!(value.as_number().into_option(), Some(42.0));
    }

    #[test]
    #[cfg(feature = "json")]
    fn test_nested() {
        let json = Json::parse(r#"{"items": [1, 2, {"nested": true}]}"#).unwrap();

        let items = json.get_key("items").unwrap();
        assert!(items.is_array());

        let nested_obj = items.get_index(2).unwrap();
        let nested = nested_obj.get_key("nested").unwrap();
        assert_eq!(nested.as_bool().into_option(), Some(true));
    }

    #[test]
    #[cfg(feature = "json")]
    fn test_roundtrip_serde_parity() {
        // A nested value round-trips through pretty-print + re-parse unchanged —
        // exercises the AzJson <-> serde_json bridge in both directions.
        let src = r#"{"a":1,"b":[true,null,"x"],"c":{"d":2.5}}"#;
        let json = Json::parse(src).unwrap();
        let reparsed = Json::parse(json.to_string_pretty().as_str()).unwrap();
        assert_eq!(json, reparsed);
    }

    #[test]
    #[cfg(feature = "json")]
    fn test_undo_manager_roundtrip() {
        use azul_core::refany::RefAny;

        extern "C" fn ser(mut r: RefAny) -> Json {
            match r.downcast_ref::<i64>() {
                Some(v) => Json::integer(*v),
                None => Json::null(),
            }
        }
        extern "C" fn deser(j: Json) -> ResultRefAnyString {
            match j.as_i64().into_option() {
                Some(v) => Ok(RefAny::new(v)),
                None => Err("not an i64".to_string()),
            }
            .into()
        }

        let mut state = RefAny::new(10i64);
        state.set_serialize_fn(ser as usize);
        state.set_deserialize_fn(deser as usize);

        let mut undo = RefAnyUndoManager::new(0);
        undo.snapshot(&state); // record 10
        if let Some(mut v) = state.downcast_mut::<i64>() {
            *v = 20;
        }
        undo.snapshot(&state); // record 20
        if let Some(mut v) = state.downcast_mut::<i64>() {
            *v = 30;
        }

        assert!(undo.can_undo());
        assert!(undo.undo(&mut state));
        assert_eq!(*state.downcast_ref::<i64>().unwrap(), 20);
        assert!(undo.undo(&mut state));
        assert_eq!(*state.downcast_ref::<i64>().unwrap(), 10);
        assert!(undo.redo(&mut state));
        assert_eq!(*state.downcast_ref::<i64>().unwrap(), 20);
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
