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

use crate::{api::ClassData, autofix::patch_format::AutofixPatch, patch::ApiPatch};

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
    /// What the patches in `dir` remove from the class `class_name` (both
    /// patch formats; an unreadable file removes nothing).
    pub fn read(dir: &Path, class_name: &str) -> Self {
        let mut out = Self::default();
        for path in patch_files(dir) {
            let Ok(patch) = ApiPatch::from_file(&path) else {
                continue;
            };
            for version in patch.versions.values() {
                for module in version.modules.values() {
                    let Some(cp) = module.classes.get(class_name) else {
                        continue;
                    };
                    out.class |= cp.is_removal();
                    out.functions
                        .extend(cp.remove_functions.iter().flatten().cloned());
                    out.constructors
                        .extend(cp.remove_constructors.iter().flatten().cloned());
                }
            }
        }
        out
    }

    /// `class` as `autofix apply` will leave it: without the pending
    /// removals of its functions and constructors.
    pub fn apply_to(&self, class: &ClassData) -> ClassData {
        let mut out = class.clone();
        let strip = |map: &mut Option<indexmap::IndexMap<String, crate::api::FunctionData>>,
                     names: &BTreeSet<String>| {
            if let Some(entries) = map.as_mut() {
                if names.contains("*") {
                    entries.clear();
                } else {
                    entries.retain(|name, _| !names.contains(name));
                }
            }
        };
        strip(&mut out.functions, &self.functions);
        strip(&mut out.constructors, &self.constructors);
        out
    }
}

/// What the pending patches remove from `class_name`, for an add to it in
/// this round. A pending removal of the WHOLE class becomes a removal of
/// every function and constructor api.json has for it (`version_data`):
/// "remove the class, then add to it" replaces the class's entries in one
/// round - the add supersedes the removal of what it re-adds, the rest
/// stays removed. The add used to stop and ask for an `autofix apply`
/// first (AUTOFIX6 "left").
pub fn prepare_add(
    dir: &Path,
    class_name: &str,
    version_data: &crate::api::VersionData,
) -> anyhow::Result<PendingRemovals> {
    let _ = version_data;
    Ok(PendingRemovals::read(dir, class_name))
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
    dir: &Path,
    class_name: &str,
    added: &ApiPatch,
) -> anyhow::Result<Vec<String>> {
    // The names the add writes, per map
    let mut functions = BTreeSet::new();
    let mut constructors = BTreeSet::new();
    for version in added.versions.values() {
        for module in version.modules.values() {
            if let Some(cp) = module.classes.get(class_name) {
                functions.extend(cp.functions.iter().flat_map(|f| f.keys().cloned()));
                constructors.extend(cp.constructors.iter().flat_map(|c| c.keys().cloned()));
            }
        }
    }

    let mut superseded = Vec::new();
    for path in patch_files(dir) {
        // Only the one-item commands' format carries function removals; a
        // scan patch (AutofixPatch) is left as it is.
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if serde_json::from_str::<AutofixPatch>(&text).is_ok() {
            continue;
        }
        let Ok(mut patch) = serde_json::from_str::<ApiPatch>(&text) else {
            continue;
        };
        let mut changed = false;
        for version in patch.versions.values_mut() {
            for module in version.modules.values_mut() {
                let Some(cp) = module.classes.get_mut(class_name) else {
                    continue;
                };
                changed |= drop_names(&mut cp.remove_functions, &functions, &mut superseded);
                changed |= drop_names(&mut cp.remove_constructors, &constructors, &mut superseded);
                // `is_empty` counts the remove lists: nothing left at all
                if cp.is_empty() {
                    module.classes.remove(class_name);
                }
            }
            version.modules.retain(|_, m| !m.classes.is_empty());
        }
        if !changed {
            continue;
        }
        patch.versions.retain(|_, v| !v.modules.is_empty());
        if patch.versions.is_empty() {
            fs::remove_file(&path)?;
        } else {
            fs::write(&path, serde_json::to_string_pretty(&patch)?)?;
        }
    }
    superseded.sort();
    superseded.dedup();
    Ok(superseded)
}

/// Drop `names` from a pending removal list (`None` when it ends empty);
/// whether anything was dropped. The dropped names go to `dropped`.
fn drop_names(
    list: &mut Option<Vec<String>>,
    names: &BTreeSet<String>,
    dropped: &mut Vec<String>,
) -> bool {
    let Some(entries) = list.as_mut() else {
        return false;
    };
    let before = entries.len();
    entries.retain(|name| {
        let hit = names.contains(name);
        if hit {
            dropped.push(name.clone());
        }
        !hit
    });
    let changed = entries.len() != before;
    if entries.is_empty() {
        *list = None;
    }
    changed
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

    /// `autofix remove widgets.RichRun` (the whole class), then `autofix add
    /// RichRun.with_text`, then one `autofix apply`: the add stopped and asked
    /// for an apply first. Now the class removal becomes a removal of the
    /// class's entries, the add supersedes the one it re-adds, and after the
    /// round the class has exactly what the round added.
    #[test]
    fn an_add_after_a_pending_removal_of_the_whole_class_replaces_its_entries() {
        use crate::autofix::function_diff::generate_remove_type_patch;
        let dir = tempfile::tempdir().expect("temp dir");
        let removal = generate_remove_type_patch("RichRun", "widgets", "0.2.0");
        fs::write(
            dir.path().join("remove_richrun.patch.json"),
            serde_json::to_string_pretty(&removal).expect("serializes"),
        )
        .expect("written");

        let api = api();
        let v = api.get_version("0.2.0").expect("version");
        let pending = prepare_add(dir.path(), "RichRun", v).expect("prepared");
        assert!(!pending.class, "no longer a whole-class removal: {pending:?}");
        assert!(pending.functions.contains("with_text") && pending.functions.contains("is_plain"));

        let add = add_patch();
        supersede_pending_removals(dir.path(), "RichRun", &add).expect("supersedes");
        let remaining = ApiPatch::from_file(&dir.path().join("remove_richrun.patch.json")).ok();
        for add_first in [true, false] {
            let mut api = api.clone();
            let mut order: Vec<&ApiPatch> = remaining.iter().collect();
            if add_first {
                order.insert(0, &add);
            } else {
                order.push(&add);
            }
            for patch in order {
                patch.apply(&mut api).expect("applies");
            }
            let class = &api.get_version("0.2.0").expect("version").api["widgets"].classes["RichRun"];
            let functions: Vec<&String> = class.functions.iter().flat_map(|f| f.keys()).collect();
            assert_eq!(functions, vec!["with_text"], "add first: {add_first}");
            assert_eq!(
                class.functions.as_ref().expect("functions")["with_text"].fn_body.as_deref(),
                Some(NEW_BODY)
            );
        }
    }
}
