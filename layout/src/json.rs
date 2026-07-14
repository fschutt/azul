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

/// Restores `state`'s contents in-place from `json`, using its registered
/// deserialize fn, and **preserves the live serialize/deserialize/update hooks**
/// across the swap (`replace_contents` copies them from the freshly-deserialized
/// value, which a non-`AZ_REFLECT`-upcast deserialize would leave unset). Returns
/// `Err` with a reason if `state` has no deserialize fn, the JSON can't be
/// deserialized, or the swap fails (active borrows).
///
/// Shared by [`RefAnyUndoManager`] and the AZ_DEBUG server's `set_app_state` /
/// `restore_snapshot` so both round-trip identically.
#[cfg(feature = "json")]
pub fn restore_refany_from_json(state: &mut RefAny, json: Json) -> Result<(), String> {
    let deser_fn = state.get_deserialize_fn();
    if deser_fn == 0 {
        return Err("state has no deserialize fn (AZ_REFLECT_JSON not registered)".to_string());
    }
    let restored = deserialize_refany_from_json(json, deser_fn)?;
    let ser_fn = state.get_serialize_fn();
    let upd_fn = state.get_update_fn();
    let ok = state.replace_contents(restored);
    state.set_serialize_fn(ser_fn);
    state.set_deserialize_fn(deser_fn);
    state.set_update_fn(upd_fn);
    if ok {
        Ok(())
    } else {
        Err("replace_contents failed (active borrows exist)".to_string())
    }
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
        restore_refany_from_json(state, Json::from_serde_value(value.clone())).is_ok()
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

// ============================================================================
// Adversarial tests (autotest)
// ============================================================================

/// Adversarial coverage for this module. The whole file is already behind
/// `#[cfg(feature = "json")]` (see `lib.rs`), so no per-test feature gate is
/// needed here.
///
/// Deliberately NOT tested: passing a non-zero *bogus* `usize` as a
/// `deserialize_fn`. `deserialize_refany_from_json` transmutes it to a function
/// pointer and calls it, so any value other than `0` or a real
/// `extern "C" fn(Json) -> ResultRefAnyString` is UB by contract, not a case the
/// function can defend against.
#[cfg(test)]
mod autotest_generated {
    use super::*;

    // ------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------

    /// The canonical AZ_REFLECT_JSON pair for an `i64` state.
    extern "C" fn ser_i64(mut r: RefAny) -> Json {
        match r.downcast_ref::<i64>() {
            Some(v) => Json::integer(*v),
            None => Json::null(),
        }
    }

    /// Rejects anything that is not an exactly-representable `i64`.
    extern "C" fn deser_i64(j: Json) -> ResultRefAnyString {
        match j.as_i64().into_option() {
            Some(v) => ResultRefAnyString::Ok(RefAny::new(v)),
            None => ResultRefAnyString::Err(AzString::from("not an i64".to_string())),
        }
    }

    /// A serializer that always yields JSON `null` — i.e. "no JSON form".
    extern "C" fn ser_null(_r: RefAny) -> Json {
        Json::null()
    }

    /// A deserializer that always fails.
    extern "C" fn deser_always_err(_j: Json) -> ResultRefAnyString {
        ResultRefAnyString::Err(AzString::from("always fails".to_string()))
    }

    /// An object-shaped state whose JSON key is hostile to JSON Pointer syntax
    /// (contains both `/` and `~`), so the diff path must be escaped/unescaped.
    #[derive(Debug, Clone, PartialEq)]
    struct Doc {
        text: String,
        cursor: i64,
    }

    extern "C" fn ser_doc(mut r: RefAny) -> Json {
        let (text, cursor) = match r.downcast_ref::<Doc>() {
            Some(d) => (d.text.clone(), d.cursor),
            None => return Json::null(),
        };
        Json::object(JsonKeyValueVec::from_vec(vec![
            JsonKeyValue::create(AzString::from("a/b~c".to_string()), Json::string(text)),
            JsonKeyValue::create(AzString::from("cursor".to_string()), Json::integer(cursor)),
        ]))
    }

    extern "C" fn deser_doc(j: Json) -> ResultRefAnyString {
        let text = match j.get_key("a/b~c").and_then(|t| t.as_string().into_option()) {
            Some(s) => s.as_str().to_string(),
            None => return ResultRefAnyString::Err(AzString::from("missing text".to_string())),
        };
        let cursor = match j.get_key("cursor").and_then(|c| c.as_i64().into_option()) {
            Some(c) => c,
            None => return ResultRefAnyString::Err(AzString::from("missing cursor".to_string())),
        };
        ResultRefAnyString::Ok(RefAny::new(Doc { text, cursor }))
    }

    fn state_i64(v: i64) -> RefAny {
        let mut s = RefAny::new(v);
        s.set_serialize_fn(ser_i64 as usize);
        s.set_deserialize_fn(deser_i64 as usize);
        s
    }

    fn read_i64(s: &mut RefAny) -> i64 {
        let g = s.downcast_ref::<i64>().expect("state holds an i64");
        *g
    }

    fn write_i64(s: &mut RefAny, v: i64) {
        let mut g = s.downcast_mut::<i64>().expect("state holds an i64");
        *g = v;
    }

    // ------------------------------------------------------------------
    // json_parse — parser: malformed / huge / boundary / unicode
    // ------------------------------------------------------------------

    #[test]
    fn json_parse_empty_and_whitespace_only_is_err() {
        for src in ["", " ", "   ", "\t\n\r ", "\u{feff}"] {
            let r = json_parse(src);
            assert!(r.is_err(), "{src:?} must be rejected, got {r:?}");
        }
    }

    #[test]
    fn json_parse_garbage_is_err_and_never_panics() {
        for src in [
            "{ invalid }",
            "]]]",
            "}{",
            "nul",
            "tru",
            "'single quoted'",
            "{\"a\":1,}",
            "[1,2,]",
            "{a:1}",
            "\u{0}\u{1}\u{2}",
            "\\",
            "\"unterminated",
            "01",
            "--1",
            "[,]",
            "{\"a\"}",
            "{\"a\":}",
        ] {
            let r = json_parse(src);
            assert!(r.is_err(), "{src:?} must be rejected, got {r:?}");
        }
    }

    #[test]
    fn json_parse_leading_trailing_junk_is_deterministic() {
        // Surrounding whitespace is fine ...
        let ok = json_parse("  \n\t{\"a\":1}  \n").expect("whitespace-padded JSON parses");
        assert!(ok.is_object());
        assert_eq!(ok.len(), 1);

        // ... trailing non-whitespace is not.
        for src in ["{\"a\":1};garbage", "[1,2,3]extra", "null null", "1 2", "\"a\"\"b\""] {
            assert!(json_parse(src).is_err(), "{src:?} must be rejected");
        }
    }

    #[test]
    fn json_parse_deeply_nested_is_rejected_without_stack_overflow() {
        // serde_json's default recursion limit (128) must turn this into an
        // `Err` rather than blowing the stack.
        let deep_arrays = format!("{}{}", "[".repeat(10_000), "]".repeat(10_000));
        assert!(json_parse(&deep_arrays).is_err(), "10k-deep arrays must be rejected");

        let deep_objects = format!(
            "{}1{}",
            "{\"a\":".repeat(10_000),
            "}".repeat(10_000)
        );
        assert!(json_parse(&deep_objects).is_err(), "10k-deep objects must be rejected");

        // Unbalanced garbage of the same shape: still an error, still no crash.
        assert!(json_parse(&"[".repeat(1_000_000)).is_err());

        // Positive control: a nesting depth inside the limit still parses.
        let shallow = format!("{}1{}", "[".repeat(64), "]".repeat(64));
        assert!(json_parse(&shallow).expect("64-deep parses").is_array());
    }

    #[test]
    fn json_parse_extremely_long_input_does_not_panic_or_hang() {
        // 1M-char string payload.
        let long = "a".repeat(1_000_000);
        let j = json_parse(&format!("\"{long}\"")).expect("1M-char string parses");
        assert_eq!(
            j.as_string().into_option().expect("string").as_str().len(),
            1_000_000
        );

        // 50k-element array (each `len()` call re-parses the payload).
        let arr_src = format!("[{}]", vec!["0"; 50_000].join(","));
        let arr = json_parse(&arr_src).expect("50k-element array parses");
        assert!(arr.is_array());
        assert_eq!(arr.len(), 50_000);
        assert_eq!(arr.get_index(49_999).expect("last").as_i64().into_option(), Some(0));
        assert!(arr.get_index(50_000).is_none(), "out-of-range index must be None");
        assert!(arr.get_index(usize::MAX).is_none(), "usize::MAX index must be None");
    }

    #[test]
    fn json_parse_boundary_numbers() {
        assert_eq!(json_parse("0").expect("0").as_i64().into_option(), Some(0));

        // "-0": the sign of zero is not preserved through the f64 store.
        let neg_zero = json_parse("-0").expect("-0");
        assert_eq!(neg_zero.as_number().into_option(), Some(0.0));
        assert_eq!(json_stringify(&neg_zero).as_str(), "0");

        // i64::MIN is exactly -2^63 in f64, so it round-trips.
        let min = json_parse(&i64::MIN.to_string()).expect("i64::MIN");
        assert_eq!(min.as_i64().into_option(), Some(i64::MIN));

        // i64::MAX is NOT exactly representable — f64 rounds it up to 2^63.
        // `as_i64` must refuse it (None) rather than wrap around to i64::MIN.
        let max = json_parse(&i64::MAX.to_string()).expect("i64::MAX");
        assert_eq!(max.as_number().into_option(), Some(i64::MAX as f64));
        assert_eq!(max.as_i64().into_option(), None);

        // u64::MAX is far out of i64 range: None, never a wrapped negative.
        let umax = json_parse(&u64::MAX.to_string()).expect("u64::MAX");
        assert_eq!(umax.as_i64().into_option(), None);
        assert!(umax.as_number().into_option().expect("number") > 0.0);

        // Smallest subnormal and largest finite double stay finite and typed.
        let tiny = json_parse("5e-324").expect("subnormal");
        let t = tiny.as_number().into_option().expect("number");
        assert!(t > 0.0 && t.is_finite(), "subnormal decoded to {t}");
        let big = json_parse("1.7976931348623157e308").expect("f64::MAX");
        assert!(big.as_number().into_option().expect("number").is_finite());

        // Non-integral numbers are numbers, but never integers.
        let frac = json_parse("0.5").expect("0.5");
        assert_eq!(frac.as_number().into_option(), Some(0.5));
        assert_eq!(frac.as_i64().into_option(), None);
    }

    #[test]
    fn json_parse_rejects_non_json_number_literals() {
        for src in [
            "NaN", "nan", "Infinity", "-Infinity", "inf", "-inf", "+1", ".5", "5.", "1e",
            "1e+", "0x10", "1_000", "1,0",
        ] {
            assert!(json_parse(src).is_err(), "{src:?} is not valid JSON");
        }
    }

    #[test]
    fn json_parse_huge_magnitude_numbers_do_not_panic() {
        let nines = "9".repeat(400);
        for src in ["1e309", "-1e309", "1e-400", nines.as_str()] {
            match json_parse(src) {
                Err(_) => {} // rejecting is fine
                Ok(j) => {
                    // Whatever it decodes to, it is never a NaN and always
                    // stringifies without panicking.
                    if let Some(n) = j.as_number().into_option() {
                        assert!(!n.is_nan(), "{src:?} decoded to NaN");
                    }
                    assert!(!json_stringify(&j).as_str().is_empty());
                }
            }
        }
    }

    #[test]
    fn json_parse_unicode_payloads() {
        // Astral-plane emoji.
        let emoji = json_parse("\"\u{1F600}\"").expect("emoji");
        assert_eq!(
            emoji.as_string().into_option().expect("string").as_str(),
            "\u{1F600}"
        );

        // Combining marks in a key, bidi override + ZWJ in a value.
        let obj = json_parse("{\"e\u{301}\":\"\u{202E}abc\u{200D}\u{1F468}\"}")
            .expect("combining / bidi payload");
        assert!(obj.is_object());
        assert_eq!(obj.len(), 1);
        assert!(obj.get_key("e\u{301}").is_some());
        assert!(obj.get_key("e").is_none(), "key lookup must not normalize");

        // Escaped NUL survives the round trip.
        let nul = json_parse("\"\\u0000\"").expect("escaped NUL");
        assert_eq!(nul.as_string().into_option().expect("string").as_str(), "\0");
        assert_eq!(json_parse(json_stringify(&nul).as_str()).expect("re-parse"), nul);

        // Broken escapes / lone surrogates are rejected, not silently replaced.
        for src in ["\"\\ud800\"", "\"\\udfff\"", "\"\\x\"", "\"\\u00\""] {
            assert!(json_parse(src).is_err(), "{src:?} must be rejected");
        }
    }

    #[test]
    fn json_parse_error_is_reported_with_a_position() {
        let e = json_parse("{\n  \"a\": ,\n}").unwrap_err();
        assert!(e.line >= 1, "line must be reported");
        assert!(!e.message.as_str().is_empty());
        // Display impl must not panic on either shape.
        assert!(!format!("{e}").is_empty());
    }

    // ------------------------------------------------------------------
    // json_stringify / round-trip: encode == decode
    // ------------------------------------------------------------------

    #[test]
    fn json_roundtrip_parse_stringify_parse_is_stable() {
        for src in [
            "null",
            "true",
            "false",
            "0",
            "-1",
            "1.5",
            "\"\"",
            "\"\\n\\t\\\"\\\\\"",
            "[]",
            "{}",
            "[1,2,3]",
            "{\"a\":1}",
            "{\"a\":[1,{\"b\":null}],\"c\":\"\u{1F600}\"}",
            "[[[[[1]]]]]",
        ] {
            let a = json_parse(src).unwrap_or_else(|e| panic!("{src:?} must parse: {e}"));
            let encoded = json_stringify(&a);
            let b = json_parse(encoded.as_str())
                .unwrap_or_else(|e| panic!("re-parse of {:?} failed: {e}", encoded.as_str()));
            assert_eq!(a, b, "value changed across encode -> decode for {src:?}");
            assert_eq!(
                encoded.as_str(),
                json_stringify(&b).as_str(),
                "stringify is not idempotent for {src:?}"
            );
        }
    }

    #[test]
    fn json_roundtrip_constructed_values() {
        for v in [
            Json::null(),
            Json::bool(true),
            Json::bool(false),
            Json::integer(0),
            Json::integer(-1),
            Json::integer(i64::MIN),
            // i64::MAX is stored as 2^63 and emitted as such — it still decodes
            // back to the identical (lossy) f64, so the round trip is stable.
            Json::integer(i64::MAX),
            Json::number(1.5),
            Json::number(-0.0),
            Json::number(1e300),
            Json::number(f64::MAX),
            Json::number(f64::MIN_POSITIVE),
            Json::string(""),
            Json::string("quote\" backslash\\ newline\n tab\t"),
            Json::string("\u{1F600}e\u{301}\u{0}"),
        ] {
            let encoded = json_stringify(&v);
            let back = json_parse(encoded.as_str()).unwrap_or_else(|e| {
                panic!("{:?} did not re-parse: {e}", encoded.as_str())
            });
            assert_eq!(v, back, "{:?} did not survive the round trip", encoded.as_str());
        }
    }

    #[test]
    fn json_stringify_non_finite_numbers_does_not_panic() {
        // JSON has no representation for NaN / ±Infinity. `to_json_string` falls
        // back to Rust's float `Display` ("NaN" / "inf"), which is *not* valid
        // JSON, so such a value does not round-trip. Pinned as observed
        // behaviour: the hard requirement is only that nothing panics, and that
        // the serde bridge maps them to `null` instead of emitting garbage.
        for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let j = Json::number(v);
            let encoded = json_stringify(&j);
            assert!(!encoded.as_str().is_empty());
            let _ = json_parse(encoded.as_str()); // allowed to fail, must not panic
            assert!(j.to_serde_value().is_null(), "serde bridge must null out {v}");
        }
    }

    #[test]
    fn json_with_corrupt_ffi_payload_does_not_panic() {
        // `Json`'s fields are `pub` (C ABI), so a C caller can hand us an
        // Array/Object whose serialized payload is not JSON at all. Every
        // accessor must degrade gracefully rather than panic.
        for value_type in [JsonType::Array, JsonType::Object] {
            let bogus = Json {
                value_type,
                internal: JsonInternal {
                    string_value: AzString::from("not-json".to_string()),
                    number_value: f64::NAN,
                    bool_value: true,
                },
            };
            assert_eq!(bogus.len(), 0);
            assert!(bogus.is_empty());
            assert!(bogus.get_index(0).is_none());
            assert!(bogus.get_key("a").is_none());
            assert!(bogus.keys().is_empty());
            assert!(bogus.to_serde_value().is_null());
            assert!(!json_stringify(&bogus).as_str().is_empty());
            assert!(!bogus.to_string_pretty().as_str().is_empty());
            // Wrong-type accessors stay None instead of reading the wrong union arm.
            assert_eq!(bogus.as_bool().into_option(), None);
            assert_eq!(bogus.as_number().into_option(), None);
            assert_eq!(bogus.as_i64().into_option(), None);
            assert_eq!(bogus.as_string().into_option(), None);
        }
    }

    // ------------------------------------------------------------------
    // ResultRefAnyString — predicates / getters: invariants
    // ------------------------------------------------------------------

    #[test]
    fn result_refany_string_predicates_are_mutually_exclusive() {
        let ok = ResultRefAnyString::Ok(RefAny::new(1i64));
        assert!(ok.is_ok());
        assert!(!ok.is_err());

        // Edge payload: an *empty* error message is still the Err variant.
        let err = ResultRefAnyString::Err(AzString::from(String::new()));
        assert!(err.is_err());
        assert!(!err.is_ok());

        // ok()/err() partition exactly one value into exactly one Option.
        assert!(ok.clone().ok().is_some());
        assert!(ok.clone().err().is_none());
        assert!(err.clone().ok().is_none());
        assert_eq!(err.clone().err().expect("err").as_str(), "");
    }

    #[test]
    fn result_refany_string_err_payload_edges() {
        // Huge multi-byte error message: no truncation, no panic.
        let big = "\u{1F600}".repeat(100_000);
        let e = ResultRefAnyString::Err(AzString::from(big.clone()));
        assert!(e.is_err());
        assert_eq!(e.err().expect("err").as_str().len(), big.len());
    }

    #[test]
    fn result_refany_string_from_result_parity() {
        let ok: Result<RefAny, String> = Ok(RefAny::new(7i64));
        let ok: ResultRefAnyString = ok.into();
        assert!(ok.is_ok() && !ok.is_err());
        let mut inner = ok.ok().expect("ok");
        assert_eq!(read_i64(&mut inner), 7);

        let err: Result<RefAny, String> = Err("boom".to_string());
        let err: ResultRefAnyString = err.into();
        assert!(err.is_err() && !err.is_ok());
        assert_eq!(err.err().expect("err").as_str(), "boom");
    }

    #[test]
    fn result_refany_string_clone_keeps_the_payload_alive() {
        let r = ResultRefAnyString::Ok(RefAny::new(7i64));
        for _ in 0..64 {
            let c = r.clone();
            assert!(c.is_ok());
        }
        // 64 clone/drop cycles later the shared data is intact (no double-free)
        // and no clone leaked a strong reference.
        let mut inner = r.ok().expect("ok");
        assert_eq!(read_i64(&mut inner), 7);
        assert_eq!(inner.get_ref_count(), 1);
    }

    // ------------------------------------------------------------------
    // serialize_refany_to_json / refany_serialize_to_json
    // ------------------------------------------------------------------

    #[test]
    fn serialize_refany_without_hook_is_none() {
        let plain = RefAny::new(5i64);
        assert!(serialize_refany_to_json(&plain).is_none());
        assert!(matches!(refany_serialize_to_json(&plain), OptionJson::None));
    }

    #[test]
    fn serialize_refany_with_hook_agrees_with_the_c_wrapper() {
        let s = state_i64(42);
        let j = serialize_refany_to_json(&s).expect("hook is set");
        assert_eq!(j.as_i64().into_option(), Some(42));
        match refany_serialize_to_json(&s) {
            OptionJson::Some(j2) => assert_eq!(j2, j),
            OptionJson::None => panic!("hook is set, must be Some"),
        }
        // The temporary clone handed to the hook must be released again.
        assert_eq!(s.get_ref_count(), 1);
    }

    #[test]
    fn serialize_refany_null_result_is_treated_as_unsupported() {
        let mut s = RefAny::new(5i64);
        s.set_serialize_fn(ser_null as usize);
        assert!(serialize_refany_to_json(&s).is_none());
        assert!(matches!(refany_serialize_to_json(&s), OptionJson::None));
    }

    // ------------------------------------------------------------------
    // deserialize_refany_from_json / json_deserialize_to_refany — numeric
    // ------------------------------------------------------------------

    #[test]
    fn deserialize_refany_with_zero_fn_pointer_is_err() {
        let e = deserialize_refany_from_json(Json::integer(1), 0).unwrap_err();
        assert_eq!(e, "Type does not support JSON deserialization");

        let r = json_deserialize_to_refany(Json::null(), 0);
        assert!(r.is_err());
        assert_eq!(
            r.err().expect("err").as_str(),
            "Type does not support JSON deserialization"
        );
    }

    #[test]
    fn deserialize_refany_numeric_boundaries() {
        let fnptr = deser_i64 as usize;

        let mut zero = deserialize_refany_from_json(Json::integer(0), fnptr).expect("0");
        assert_eq!(read_i64(&mut zero), 0);

        let mut neg = deserialize_refany_from_json(Json::integer(-1), fnptr).expect("-1");
        assert_eq!(read_i64(&mut neg), -1);

        // i64::MIN is exactly -2^63 in f64 and survives.
        let mut min = deserialize_refany_from_json(Json::integer(i64::MIN), fnptr)
            .expect("i64::MIN");
        assert_eq!(read_i64(&mut min), i64::MIN);

        // i64::MAX rounds up to 2^63 in the f64 store, which is out of range —
        // it must be reported as an error, never wrapped around to i64::MIN.
        assert!(deserialize_refany_from_json(Json::integer(i64::MAX), fnptr).is_err());

        // Saturation / NaN / non-integral / wrong-type inputs all fail cleanly.
        for j in [
            Json::number(f64::NAN),
            Json::number(f64::INFINITY),
            Json::number(f64::NEG_INFINITY),
            Json::number(1e300),
            Json::number(-1e300),
            Json::number(0.5),
            Json::null(),
            Json::bool(true),
            Json::string("7"),
        ] {
            let described = json_stringify(&j).as_str().to_string();
            let r = json_deserialize_to_refany(j, fnptr);
            assert!(r.is_err(), "{described} must not deserialize into an i64");
            assert_eq!(r.err().expect("err").as_str(), "not an i64");
        }
    }

    #[test]
    fn deserialize_refany_propagates_the_hook_error_message() {
        let r = json_deserialize_to_refany(Json::integer(1), deser_always_err as usize);
        assert!(r.is_err() && !r.is_ok());
        assert_eq!(r.err().expect("err").as_str(), "always fails");
    }

    // ------------------------------------------------------------------
    // restore_refany_from_json
    // ------------------------------------------------------------------

    #[test]
    fn restore_refany_without_deserialize_fn_is_err_and_leaves_state_intact() {
        let mut plain = RefAny::new(1i64);
        let e = restore_refany_from_json(&mut plain, Json::integer(2)).unwrap_err();
        assert!(e.contains("no deserialize fn"), "unexpected message: {e}");
        assert_eq!(read_i64(&mut plain), 1);
    }

    #[test]
    fn restore_refany_preserves_the_hooks_across_the_swap() {
        let mut s = state_i64(1);
        let (ser, deser) = (s.get_serialize_fn(), s.get_deserialize_fn());
        restore_refany_from_json(&mut s, Json::integer(2)).expect("restore");

        assert_eq!(read_i64(&mut s), 2);
        assert_eq!(s.get_serialize_fn(), ser, "serialize hook was lost");
        assert_eq!(s.get_deserialize_fn(), deser, "deserialize hook was lost");
        assert!(s.can_serialize() && s.can_deserialize());

        // ... and the restored value is itself serializable again.
        assert_eq!(
            serialize_refany_to_json(&s).expect("still serializable").as_i64().into_option(),
            Some(2)
        );
    }

    #[test]
    fn restore_refany_rejects_json_the_hook_refuses() {
        let mut s = state_i64(3);
        let e = restore_refany_from_json(&mut s, Json::string("nope")).unwrap_err();
        assert_eq!(e, "not an i64");
        assert_eq!(read_i64(&mut s), 3, "state must be untouched on failure");
    }

    #[test]
    fn restore_refany_fails_while_a_borrow_is_live() {
        let mut s = state_i64(4);
        let mut sibling = s.clone(); // clones share the RefCountInner
        let guard = sibling.downcast_ref::<i64>().expect("shared borrow");

        let e = restore_refany_from_json(&mut s, Json::integer(9)).unwrap_err();
        assert!(e.contains("replace_contents failed"), "unexpected message: {e}");
        assert_eq!(*guard, 4, "the live borrow must still see the old value");
        drop(guard);

        // Once the borrow is released the very same call succeeds, and every
        // clone observes the swap.
        restore_refany_from_json(&mut s, Json::integer(9)).expect("restore after drop");
        assert_eq!(read_i64(&mut s), 9);
        assert_eq!(read_i64(&mut sibling), 9);
    }

    // ------------------------------------------------------------------
    // RefAnyUndoManager — constructor + predicates + invariants
    // ------------------------------------------------------------------

    #[test]
    fn undo_manager_new_holds_its_invariants() {
        for capacity in [0usize, 1, 2, usize::MAX] {
            let m = RefAnyUndoManager::new(capacity);
            assert_eq!(m.capacity, capacity);
            assert!(m.head.is_none());
            assert!(m.undo_diffs.is_empty());
            assert!(m.redo_diffs.is_empty());
            assert!(!m.can_undo());
            assert!(!m.can_redo());
        }
        let d = RefAnyUndoManager::default();
        assert!(!d.can_undo() && !d.can_redo());
    }

    #[test]
    fn undo_manager_ops_on_empty_history_are_noops() {
        let mut m = RefAnyUndoManager::new(0);
        let mut s = state_i64(5);
        assert!(!m.undo(&mut s));
        assert!(!m.redo(&mut s));
        m.clear(); // clearing an empty history must not panic
        assert!(!m.can_undo() && !m.can_redo());
        assert_eq!(read_i64(&mut s), 5, "state must be untouched");
    }

    #[test]
    fn undo_manager_requires_a_json_representation() {
        let mut m = RefAnyUndoManager::new(0);

        let plain = RefAny::new(1i64); // no serialize hook at all
        assert!(!m.commit(&plain));
        assert!(!m.can_undo());
        assert!(m.head.is_none());

        let mut null_ser = RefAny::new(1i64); // hook exists but yields `null`
        null_ser.set_serialize_fn(ser_null as usize);
        assert!(!m.commit(&null_ser));
        assert!(m.head.is_none(), "a null body must not seed the history");
    }

    #[test]
    fn undo_manager_unchanged_commit_keeps_the_redo_branch() {
        let mut m = RefAnyUndoManager::new(0);
        let mut s = state_i64(1);
        assert!(m.commit(&s)); // seeds the base
        write_i64(&mut s, 2);
        assert!(m.commit(&s));
        assert!(m.undo(&mut s));
        assert_eq!(read_i64(&mut s), 1);
        assert!(m.can_redo());

        // Re-committing an *unchanged* state records nothing and must not
        // orphan the redo branch.
        assert!(!m.commit(&s));
        assert!(m.can_redo());
        assert!(m.redo(&mut s));
        assert_eq!(read_i64(&mut s), 2);
        assert!(!m.can_redo());
    }

    #[test]
    fn undo_manager_capacity_caps_the_retained_history() {
        let mut m = RefAnyUndoManager::new(2);
        let mut s = state_i64(0);
        assert!(m.commit(&s));
        for v in 1..=5i64 {
            write_i64(&mut s, v);
            assert!(m.commit(&s));
        }
        assert_eq!(m.undo_diffs.len(), 2, "capacity must evict the oldest diffs");

        // Only the two most recent steps are reachable: 5 -> 4 -> 3.
        assert!(m.undo(&mut s));
        assert_eq!(read_i64(&mut s), 4);
        assert!(m.undo(&mut s));
        assert_eq!(read_i64(&mut s), 3);
        assert!(!m.can_undo());
        assert!(!m.undo(&mut s), "an exhausted history returns false, not a panic");
        assert_eq!(read_i64(&mut s), 3);
    }

    #[test]
    fn undo_manager_capacity_one_keeps_exactly_one_step() {
        let mut m = RefAnyUndoManager::new(1);
        let mut s = state_i64(0);
        assert!(m.commit(&s));
        for v in 1..=3i64 {
            write_i64(&mut s, v);
            assert!(m.commit(&s));
        }
        assert_eq!(m.undo_diffs.len(), 1);
        assert!(m.undo(&mut s));
        assert_eq!(read_i64(&mut s), 2);
        assert!(!m.can_undo());
    }

    #[test]
    fn undo_manager_deep_history_round_trips() {
        let mut m = RefAnyUndoManager::new(0); // unlimited
        let mut s = state_i64(0);
        assert!(m.commit(&s));
        for v in 1..=100i64 {
            write_i64(&mut s, v);
            assert!(m.commit(&s));
        }
        assert_eq!(m.undo_diffs.len(), 100);

        for expected in (0..100i64).rev() {
            assert!(m.undo(&mut s));
            assert_eq!(read_i64(&mut s), expected);
        }
        assert!(!m.can_undo());
        assert!(m.can_redo());

        for expected in 1..=100i64 {
            assert!(m.redo(&mut s));
            assert_eq!(read_i64(&mut s), expected);
        }
        assert!(!m.can_redo());

        m.clear();
        assert!(!m.can_undo() && !m.can_redo());
        assert!(m.head.is_none());
        assert_eq!(read_i64(&mut s), 100, "clear() must not touch the state");
    }

    #[test]
    fn undo_manager_reports_false_when_the_restore_hook_fails() {
        // Serialization works (so commits land) but deserialization always
        // fails, so `undo` cannot write the state back: it must report `false`
        // instead of panicking or silently claiming success.
        let mut s = RefAny::new(1i64);
        s.set_serialize_fn(ser_i64 as usize);
        s.set_deserialize_fn(deser_always_err as usize);

        let mut m = RefAnyUndoManager::new(0);
        assert!(m.commit(&s));
        write_i64(&mut s, 2);
        assert!(m.commit(&s));

        assert!(!m.undo(&mut s), "restore failed -> undo must report false");
        assert_eq!(read_i64(&mut s), 2, "the state is left as it was");
        assert!(!m.can_undo());
    }

    #[test]
    fn undo_manager_round_trips_an_object_state_with_pointer_hostile_keys() {
        // The JSON key contains both '/' and '~', so the diff path only works if
        // esc()/unesc() are exact inverses (RFC 6901: ~1 then ~0, in that order).
        let mut s = RefAny::new(Doc { text: "hello".to_string(), cursor: 0 });
        s.set_serialize_fn(ser_doc as usize);
        s.set_deserialize_fn(deser_doc as usize);

        let mut m = RefAnyUndoManager::new(0);
        assert!(m.commit(&s));
        {
            let mut g = s.downcast_mut::<Doc>().expect("doc");
            g.text = "hello world".to_string();
            g.cursor = 11;
        }
        assert!(m.commit(&s));

        assert!(m.undo(&mut s));
        {
            let g = s.downcast_ref::<Doc>().expect("doc");
            assert_eq!(g.text, "hello");
            assert_eq!(g.cursor, 0);
        }
        assert!(m.redo(&mut s));
        let g = s.downcast_ref::<Doc>().expect("doc");
        assert_eq!(g.text, "hello world");
        assert_eq!(g.cursor, 11);
    }

    // ------------------------------------------------------------------
    // jsondiff::diff / apply — reversibility invariants
    // ------------------------------------------------------------------

    #[test]
    fn diff_of_identical_values_is_empty_and_apply_of_nothing_is_identity() {
        use serde_json::json;
        for v in [
            json!(null),
            json!(0),
            json!("x"),
            json!([1, 2]),
            json!({"a": {"b": [1]}}),
        ] {
            assert!(super::jsondiff::diff(&v, &v).is_empty());
            assert_eq!(super::jsondiff::apply(&v, &[], true), v);
            assert_eq!(super::jsondiff::apply(&v, &[], false), v);
        }
    }

    #[test]
    fn diff_apply_is_reversible_for_every_change_shape() {
        use serde_json::json;
        let cases = [
            (json!(1), json!(2)),                                      // scalar at the root
            (json!(null), json!({"a": 1})),                            // type change at the root
            (json!({"a": 1}), json!({"a": 1, "b": 2})),                // key added
            (json!({"a": 1, "b": 2}), json!({"a": 1})),                // key removed
            (json!({"a": {"b": {"c": 1}}}), json!({"a": {"b": {"c": 2}}})), // nested leaf
            (json!({"a": [1, 2]}), json!({"a": [2, 1]})),              // arrays are leaves
            (json!({"a": 1}), json!({"a": "1"})),                      // leaf type change
            (json!({"a": 1}), json!([1])),                             // object -> array
            (json!({}), json!({})),                                    // no-op
            (json!({"": 1}), json!({"": 2})),                          // empty key
        ];
        for (a, b) in &cases {
            let d = super::jsondiff::diff(a, b);
            assert_eq!(&super::jsondiff::apply(a, &d, true), b, "forward {a} -> {b}");
            assert_eq!(&super::jsondiff::apply(b, &d, false), a, "backward {b} -> {a}");
        }
    }

    #[test]
    fn diff_apply_handles_pointer_escapes_and_unicode_keys() {
        use serde_json::json;
        // Keys that collide with RFC-6901 pointer syntax, plus an empty key and
        // a multi-byte key (the path is sliced at a byte offset).
        let a = json!({
            "a/b": 1,
            "~": 2,
            "~0": 3,
            "~1": 4,
            "a~1b": 5,
            "": 6,
            "\u{1F600}": 7,
        });
        let mut b = a.clone();
        for (_k, v) in b.as_object_mut().expect("object").iter_mut() {
            *v = json!(0);
        }

        let d = super::jsondiff::diff(&a, &b);
        assert_eq!(d.len(), 7, "each key must yield exactly one change");
        assert_eq!(super::jsondiff::apply(&a, &d, true), b, "escaping is not round-tripping");
        assert_eq!(super::jsondiff::apply(&b, &d, false), a);
    }

    #[test]
    fn apply_ignores_changes_whose_path_cannot_be_resolved() {
        use serde_json::json;
        let base = json!({"a": 1});
        let changes = vec![
            // parent does not exist
            super::jsondiff::Change {
                path: "/x/y".to_string(),
                old: None,
                new: Some(json!(1)),
            },
            // not a JSON Pointer (no leading '/')
            super::jsondiff::Change {
                path: "a/b".to_string(),
                old: None,
                new: Some(json!(2)),
            },
            // root "removal" is a documented no-op
            super::jsondiff::Change { path: String::new(), old: None, new: None },
        ];
        assert_eq!(super::jsondiff::apply(&base, &changes, true), base);
        assert_eq!(super::jsondiff::apply(&base, &changes, false), base);
    }

    #[test]
    fn diff_of_a_large_object_is_reversible() {
        let mut a = serde_json::Map::new();
        let mut b = serde_json::Map::new();
        for i in 0..2_000i64 {
            a.insert(format!("k{i}"), serde_json::json!(i));
            b.insert(format!("k{i}"), serde_json::json!(i + 1));
        }
        let a = serde_json::Value::Object(a);
        let b = serde_json::Value::Object(b);

        let d = super::jsondiff::diff(&a, &b);
        assert_eq!(d.len(), 2_000);
        assert_eq!(super::jsondiff::apply(&a, &d, true), b);
        assert_eq!(super::jsondiff::apply(&b, &d, false), a);
    }

    #[test]
    fn diff_of_deeply_nested_objects_does_not_overflow() {
        fn nest(depth: usize, leaf: i64) -> serde_json::Value {
            let mut v = serde_json::json!(leaf);
            for _ in 0..depth {
                v = serde_json::json!({ "a": v });
            }
            v
        }
        let a = nest(200, 1);
        let b = nest(200, 2);

        let d = super::jsondiff::diff(&a, &b);
        assert_eq!(d.len(), 1, "only the leaf changed");
        assert_eq!(d[0].path, "/a".repeat(200));
        assert_eq!(super::jsondiff::apply(&a, &d, true), b);
        assert_eq!(super::jsondiff::apply(&b, &d, false), a);
    }
}
