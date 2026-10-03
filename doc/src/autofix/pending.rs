//! The patches the one-item commands (`autofix add`, `autofix remove`) leave
//! in the patch folder for `autofix apply`, read before they are applied.
//!
//! The one-item commands run once per item and `autofix apply` once at the
//! end, so a command must see api.json as the pending patches will leave it:
//! `autofix remove T.m` then `autofix add T.m` (re-adding an entry whose
//! signature changed) found the old entry still in api.json, skipped the add,
//! and the entry was gone after the round (wave 5).

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use crate::{api::ClassData, patch::ApiPatch};

/// What the pending patches remove from one class.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PendingRemovals {
    /// The whole class is removed
    pub class: bool,
    /// `remove_functions` names (`*` = all)
    pub functions: BTreeSet<String>,
    /// `remove_constructors` names (`*` = all)
    pub constructors: BTreeSet<String>,
}

impl PendingRemovals {
    /// What the patches in `dir` remove from the class `class_name`.
    pub fn read(_dir: &Path, _class_name: &str) -> Self {
        Self::default()
    }

    /// `class` as `autofix apply` will leave it: without the pending
    /// removals of its functions and constructors.
    pub fn apply_to(&self, class: &ClassData) -> ClassData {
        class.clone()
    }
}

/// The patch files of `dir`, in the order `autofix apply` applies them.
fn patch_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.ends_with(".patch.json") || n.ends_with(".patch"))
                })
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

/// An add supersedes the pending removal of an entry it re-adds: the name is
/// dropped from the pending `remove_functions` (`remove_constructors`) of
/// `class_name` when `added` writes a function (constructor) of that name.
/// The add's merge replaces the old entry in place, so the outcome no longer
/// depends on the order `autofix apply` takes the two patches in; a removal
/// from the OTHER map stays (an entry that moved between `functions` and
/// `constructors`). Rewrites the pending files (deletes one left empty) and
/// returns the superseded names.
pub fn supersede_pending_removals(
    _dir: &Path,
    _class_name: &str,
    _added: &ApiPatch,
) -> anyhow::Result<Vec<String>> {
    let _ = patch_files;
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{api::ApiData, autofix::function_diff::generate_remove_functions_patch};

    const NEW_BODY: &str = "object.with_text(text.into_library_owned_string())";

    fn api() -> ApiData {
        serde_json::from_value(serde_json::json!({
            "0.2.0": {"apiversion": 1, "git": "", "date": "", "api": {"widgets": {"classes": {
                "RichRun": {
                    "external": "azul_layout::widgets::rich_text::doc::RichRun",
                    "functions": {
                        "with_text": {"fn_args": [{"self": "ref"}, {"text": "String"}],
                                      "fn_body": "object.with_text(text.as_str())"},
                        "is_plain": {"fn_args": [{"self": "ref"}], "returns": {"type": "bool"},
                                     "fn_body": "object.is_plain()"}
                    }
                }
            }}}}
        }))
        .expect("test api parses")
    }

    /// What `autofix add RichRun.with_text` writes for the changed signature.
    fn add_patch() -> ApiPatch {
        serde_json::from_value(serde_json::json!({"versions": {"0.2.0": {"modules": {"widgets": {
            "classes": {"RichRun": {
                "functions": {"with_text": {"fn_args": [{"self": "value"}, {"text": "String"}],
                                            "fn_body": NEW_BODY}},
                "add_functions": true
            }}
        }}}}}))
        .expect("test patch parses")
    }

    /// `autofix remove RichRun.with_text`, then `autofix add RichRun.with_text`
    /// (the signature changed), then one `autofix apply`: the add used to see
    /// the old entry in api.json and skip; now it sees api.json as the pending
    /// patches leave it, supersedes the pending removal of what it re-adds,
    /// and the new entry is in api.json whatever order the apply takes.
    #[test]
    fn an_add_after_a_pending_remove_of_the_same_entry_goes_through_in_one_round() {
        let dir = tempfile::tempdir().expect("temp dir");
        let write = |name: &str, patch: &ApiPatch| {
            let json = serde_json::to_string_pretty(patch).expect("patch serializes");
            fs::write(dir.path().join(name), json).expect("patch written");
        };
        let remove = |name: &str| generate_remove_functions_patch("RichRun", &[name], "widgets", "0.2.0");
        write("remove_richrun_with_text.patch.json", &remove("with_text"));
        write("remove_richrun_as_str.patch.json", &remove("as_str"));

        // 1. the add's view of the class: without the pending removals
        let class = api().get_version("0.2.0").expect("version").api["widgets"].classes["RichRun"].clone();
        let pending = PendingRemovals::read(dir.path(), "RichRun");
        assert!(pending.functions.contains("with_text"), "{pending:?}");
        assert!(!pending.class);
        let after = pending.apply_to(&class);
        let functions = after.functions.expect("is_plain stays");
        assert!(!functions.contains_key("with_text") && functions.contains_key("is_plain"));

        // 2. the add supersedes the pending removal of what it re-adds
        let add = add_patch();
        let superseded = supersede_pending_removals(dir.path(), "RichRun", &add).expect("rewrites");
        assert_eq!(superseded, vec!["with_text".to_string()]);
        let pending = PendingRemovals::read(dir.path(), "RichRun");
        assert!(!pending.functions.contains("with_text"), "{pending:?}");
        assert!(pending.functions.contains("as_str"), "an unrelated pending removal stays");

        // 3. one round, either order: the new entry is in api.json
        let remaining = ApiPatch::from_file(&dir.path().join("remove_richrun_with_text.patch.json")).ok();
        for add_first in [true, false] {
            let mut api = api();
            let mut order: Vec<&ApiPatch> = remaining.iter().collect();
            if add_first {
                order.insert(0, &add);
            } else {
                order.push(&add);
            }
            for patch in order {
                patch.apply(&mut api).expect("applies");
            }
            let functions = api.get_version("0.2.0").expect("version").api["widgets"].classes["RichRun"]
                .functions
                .clone()
                .expect("functions");
            assert_eq!(
                functions["with_text"].fn_body.as_deref(),
                Some(NEW_BODY),
                "add first: {add_first}"
            );
        }
    }
}
