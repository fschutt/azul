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
// Generic application-state undo/redo ("mini-git" with reversible JSON diffs)
// ============================================================================

/// Reversible JSON diffing for the undo history. A diff is a flat list of leaf
/// changes keyed by RFC-6901 JSON Pointer: objects diff per-key (recursively),
/// while scalars / arrays / type-changes are whole-value leaf replacements.
/// Each change records both `old` and `new`, so a diff applies forwards (redo)
/// or backwards (undo).
#[cfg(feature = "json")]
mod jsondiff {
    use alloc::{format, string::String, vec::Vec};

    use serde_json::Value;

    /// One reversible change at a JSON Pointer path. `old`/`new` are `None` when
    /// the key is absent on that side (key added / removed).
    #[derive(Debug, Clone)]
    pub struct Change {
        pub path: String,
        pub old: Option<Value>,
        pub new: Option<Value>,
    }

    /// Computes a reversible diff `old → new`.
    pub fn diff(old: &Value, new: &Value) -> Vec<Change> {
        let mut out = Vec::new();
        diff_rec(old, new, String::new(), &mut out);
        out
    }

    fn diff_rec(old: &Value, new: &Value, path: String, out: &mut Vec<Change>) {
        if old == new {
            return;
        }
        if let (Value::Object(o), Value::Object(n)) = (old, new) {
            for (k, ov) in o {
                let child = format!("{}/{}", path, esc(k));
                match n.get(k) {
                    Some(nv) => diff_rec(ov, nv, child, out),
                    None => out.push(Change { path: child, old: Some(ov.clone()), new: None }),
                }
            }
            for (k, nv) in n {
                if !o.contains_key(k) {
                    out.push(Change {
                        path: format!("{}/{}", path, esc(k)),
                        old: None,
                        new: Some(nv.clone()),
                    });
                }
            }
        } else {
            out.push(Change { path, old: Some(old.clone()), new: Some(new.clone()) });
        }
    }

    /// Applies a diff to `base`. `forward = true` moves `old → new` (redo);
    /// `forward = false` moves `new → old` (undo).
    pub fn apply(base: &Value, diff: &[Change], forward: bool) -> Value {
        let mut v = base.clone();
        for ch in diff {
            let target = if forward { &ch.new } else { &ch.old };
            set_at(&mut v, &ch.path, target);
        }
        v
    }

    fn set_at(root: &mut Value, path: &str, val: &Option<Value>) {
        if path.is_empty() {
            if let Some(v) = val {
                *root = v.clone();
            }
            return;
        }
        let split = path.rfind('/').unwrap_or(0);
        let parent_ptr = &path[..split];
        let last = unesc(&path[split + 1..]);
        let parent = if parent_ptr.is_empty() {
            root
        } else {
            match root.pointer_mut(parent_ptr) {
                Some(p) => p,
                None => return,
            }
        };
        if let Some(obj) = parent.as_object_mut() {
            match val {
                Some(v) => {
                    obj.insert(last, v.clone());
                }
                None => {
                    obj.remove(&last);
                }
            }
        }
    }

    fn esc(k: &str) -> String {
        k.replace('~', "~0").replace('/', "~1")
    }
    fn unesc(s: &str) -> String {
        s.replace("~1", "/").replace("~0", "~")
    }
}

/// A generic application-state undo/redo history — a "mini-git" for the app's
/// state `RefAny`, storing reversible **JSON diffs** between commits rather than
/// full snapshots (memory-efficient for large models like a text document).
///
/// Workflow: [`commit`](Self::commit) the current state at action / auto-save
/// boundaries (e.g. from a timer callback, or driven by the RefAny `update_fn`
/// hook marking the state dirty), then [`undo`](Self::undo) / [`redo`](Self::redo)
/// walk the history. Like git, committing a new state *after* an undo discards
/// the now-orphaned redo branch. Requires the state's JSON (de)serialize fns
/// (`AZ_REFLECT_JSON`); all ops are no-ops returning `false` otherwise.
///
/// Wired into `CallbackInfo` (`commit_undo_snapshot` / `undo` / `redo`) so a
/// callback — including a timer callback — can manage history on the app model.
#[cfg(feature = "json")]
#[derive(Debug, Clone, Default)]
pub struct RefAnyUndoManager {
    /// The last committed state (serde value): the base the next diff is computed
    /// against and that undo/redo diffs are applied to. `None` until first commit.
    head: Option<serde_json::Value>,
    /// Reversible diffs, each from commit N-1 → N (top = most recent).
    undo_diffs: alloc::vec::Vec<alloc::vec::Vec<jsondiff::Change>>,
    /// Diffs of undone commits, available to redo.
    redo_diffs: alloc::vec::Vec<alloc::vec::Vec<jsondiff::Change>>,
    /// Maximum number of undo diffs retained (`0` = unlimited).
    capacity: usize,
}

#[cfg(feature = "json")]
impl RefAnyUndoManager {
    /// Creates a history with a maximum depth (`0` = unlimited).
    pub fn new(capacity: usize) -> Self {
        Self {
            head: None,
            undo_diffs: alloc::vec::Vec::new(),
            redo_diffs: alloc::vec::Vec::new(),
            capacity,
        }
    }

    /// Commits `state` as a new history point, recording the reversible diff from
    /// the previous commit. The first commit just seeds the base. A commit that
    /// changed something discards the redo branch (git-like). Returns `true` if a
    /// commit was recorded (JSON supported and, after the first, state changed).
    pub fn commit(&mut self, state: &RefAny) -> bool {
        let cur = match serialize_refany_to_json(state) {
            Some(j) => j.to_serde_value(),
            None => return false,
        };
        match self.head.take() {
            None => {
                self.head = Some(cur); // seed base, no diff yet
                true
            }
            Some(prev) => {
                let d = jsondiff::diff(&prev, &cur);
                if d.is_empty() {
                    self.head = Some(prev);
                    return false; // nothing changed
                }
                self.undo_diffs.push(d);
                self.redo_diffs.clear(); // new commit orphans the redo branch
                if self.capacity != 0 && self.undo_diffs.len() > self.capacity {
                    self.undo_diffs.remove(0);
                }
                self.head = Some(cur);
                true
            }
        }
    }

    /// True if there is a commit to undo.
    pub fn can_undo(&self) -> bool {
        !self.undo_diffs.is_empty()
    }

    /// True if there is an undone commit to redo.
    pub fn can_redo(&self) -> bool {
        !self.redo_diffs.is_empty()
    }

    /// Reverts the most recent commit, restoring the previous state into `state`.
    pub fn undo(&mut self, state: &mut RefAny) -> bool {
        let d = match self.undo_diffs.pop() {
            Some(d) => d,
            None => return false,
        };
        let head = match self.head.take() {
            Some(h) => h,
            None => return false,
        };
        let reverted = jsondiff::apply(&head, &d, false);
        let ok = Self::restore(state, &reverted);
        self.head = Some(reverted);
        self.redo_diffs.push(d);
        ok
    }

    /// Re-applies the most recently undone commit.
    pub fn redo(&mut self, state: &mut RefAny) -> bool {
        let d = match self.redo_diffs.pop() {
            Some(d) => d,
            None => return false,
        };
        let head = match self.head.take() {
            Some(h) => h,
            None => return false,
        };
        let applied = jsondiff::apply(&head, &d, true);
        let ok = Self::restore(state, &applied);
        self.head = Some(applied);
        self.undo_diffs.push(d);
        ok
    }

    /// Drops all recorded history.
    pub fn clear(&mut self) {
        self.head = None;
        self.undo_diffs.clear();
        self.redo_diffs.clear();
    }

    fn restore(state: &mut RefAny, value: &serde_json::Value) -> bool {
        let deser_fn = state.get_deserialize_fn();
        let json = Json::from_serde_value(value.clone());
        match deserialize_refany_from_json(json, deser_fn) {
            Ok(restored) => {
                // `replace_contents` copies the (de)serialize/update fn-ptrs from
                // `restored`, which a plain deserialize ctor leaves unset — re-attach
                // the live hooks so subsequent undo/redo still has a serializer.
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
        undo.commit(&state); // commit 10 (seeds base)
        if let Some(mut v) = state.downcast_mut::<i64>() {
            *v = 20;
        }
        undo.commit(&state); // commit 20
        if let Some(mut v) = state.downcast_mut::<i64>() {
            *v = 30;
        }
        undo.commit(&state); // commit 30

        assert!(undo.can_undo());
        assert!(undo.undo(&mut state));
        assert_eq!(*state.downcast_ref::<i64>().unwrap(), 20);
        assert!(undo.undo(&mut state));
        assert_eq!(*state.downcast_ref::<i64>().unwrap(), 10);
        assert!(undo.redo(&mut state));
        assert_eq!(*state.downcast_ref::<i64>().unwrap(), 20);

        // mini-git branching: a new commit after an undo discards the orphaned
        // redo branch ("do a -> undo -> do b clears a").
        assert!(undo.can_redo()); // 30 is still redoable here
        if let Some(mut v) = state.downcast_mut::<i64>() {
            *v = 99;
        }
        undo.commit(&state); // branch from 20 -> 99
        assert!(!undo.can_redo()); // the 30 branch is gone
        assert!(undo.undo(&mut state));
        assert_eq!(*state.downcast_ref::<i64>().unwrap(), 20);
    }

    #[test]
    #[cfg(feature = "json")]
    fn test_json_diff_apply_reversible() {
        // A word-editor-like model: text + cursor + nested meta. The reversible
        // diff is the heart of the mini-git history, so it must round-trip both
        // directions.
        let a = Json::parse(r#"{"text":"hello","cursor":0,"meta":{"saved":true}}"#)
            .unwrap()
            .to_serde_value();
        let b = Json::parse(
            r#"{"text":"hello world","cursor":11,"meta":{"saved":false},"tags":[1,2]}"#,
        )
        .unwrap()
        .to_serde_value();
        let d = super::jsondiff::diff(&a, &b);
        assert!(!d.is_empty());
        assert_eq!(super::jsondiff::apply(&a, &d, true), b); // forward: a -> b
        assert_eq!(super::jsondiff::apply(&b, &d, false), a); // backward: b -> a
        assert!(super::jsondiff::diff(&a, &a).is_empty()); // unchanged -> empty diff
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
