//! The patch files `autofix add` writes into the patch folder.
//!
//! `autofix add` runs once per item and `autofix apply` once at the end over
//! the whole folder, so the files one add writes must survive the next add of
//! the same round.

use std::path::Path;

use crate::{
    api::VersionData,
    autofix::{
        function_diff::{generate_add_type_patches, AddTypeResult},
        patch_format::PatchOperation,
        type_index::TypeIndex,
    },
};

/// One patch file: its name in the patch folder and its JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchFile {
    pub name: String,
    pub json: String,
}

/// The file name of the functions patch `autofix add <type>.<spec>` writes
/// (`*` is spelled `all`).
pub fn functions_patch_file_name(type_name: &str, method_spec: &str) -> String {
    format!(
        "add_{}_{}.patch.json",
        type_name.to_lowercase(),
        if method_spec == "*" {
            "all"
        } else {
            method_spec
        }
    )
}

/// The patch files `autofix add <type>.<spec>` writes for a type api.json
/// does not have yet: the type with its transitive dependencies, its
/// standard-trait impls (for `*`), and the requested functions.
///
/// Every name says what the file holds, so the adds of one round never
/// overwrite each other: `add_<type>.type.<added type>.patch.json` (the same
/// content whichever method of `<type>` is added),
/// `add_<type>.impls.patch.json`, and [`functions_patch_file_name`] - one per
/// method spec, as for a type api.json has. The numbered `add_<type>_<i>` /
/// `add_<type>_functions` names of before were the same for every method:
/// three adds of one new type kept the last add's files (wave 6).
pub fn new_type_patch_files(
    type_name: &str,
    method_spec: &str,
    index: &TypeIndex,
    version_data: &VersionData,
    version: &str,
) -> Result<(Vec<PatchFile>, AddTypeResult), String> {
    let (patches, result) =
        generate_add_type_patches(type_name, Some(method_spec), index, version_data, version)?;
    let lower = type_name.to_lowercase();
    let mut files: Vec<PatchFile> = Vec::new();
    for (i, patch) in patches.iter().enumerate() {
        let what = match patch.operations.first() {
            Some(PatchOperation::Add(a)) => format!("type.{}", a.type_name.to_lowercase()),
            Some(PatchOperation::Modify(_)) => "impls".to_string(),
            _ => format!("other.{i}"),
        };
        files.push(PatchFile {
            name: format!("add_{lower}.{what}.patch.json"),
            json: patch.to_json().map_err(|e| e.to_string())?,
        });
    }
    if let Some(functions) = &result.functions_patch {
        files.push(PatchFile {
            name: functions_patch_file_name(type_name, method_spec),
            json: serde_json::to_string_pretty(functions).map_err(|e| e.to_string())?,
        });
    }
    Ok((files, result))
}

/// Write `files` into the patch folder `dir`.
pub fn write_patch_files(dir: &Path, files: &[PatchFile]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for file in files {
        std::fs::write(dir.join(&file.name), &file.json)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;

    use super::*;
    use crate::{
        api::ApiData,
        autofix::type_index::{extract_method_def, MethodDef, TypeDefKind, TypeDefinition},
    };

    /// A repr(C) struct `name` at `azul_layout::<module_path>::<name>` with
    /// the methods of `source` (one `impl X { .. }`).
    pub(crate) fn type_def(name: &str, module_path: &str, source: &str) -> TypeDefinition {
        let file: syn::File = syn::parse_file(source).expect("test source parses");
        let mut methods: Vec<MethodDef> = Vec::new();
        for item in &file.items {
            if let syn::Item::Impl(block) = item {
                for impl_item in &block.items {
                    if let syn::ImplItem::Fn(f) = impl_item {
                        methods.extend(extract_method_def(f, name));
                    }
                }
            }
        }
        TypeDefinition {
            full_path: format!("azul_layout::{module_path}::{name}"),
            type_name: name.to_string(),
            file_path: std::path::PathBuf::from("/nonexistent/autofix_add_test.rs"),
            module_path: module_path.to_string(),
            crate_name: "azul_layout".to_string(),
            kind: TypeDefKind::Struct {
                fields: IndexMap::new(),
                repr: Some("C".to_string()),
                repr_attr_count: 1,
                generic_params: Vec::new(),
                derives: Vec::new(),
                custom_impls: Vec::new(),
                is_tuple_struct: false,
            },
            source_code: String::new(),
            methods,
        }
    }

    pub(crate) fn empty_api() -> ApiData {
        serde_json::from_value(serde_json::json!({
            "0.2.0": {"apiversion": 1, "git": "", "date": "", "api": {
                "dom": {"classes": {"Dom": {"external": "azul_core::dom::Dom"}}}
            }}
        }))
        .expect("test api parses")
    }

    /// 2026-10-03, the wave-6 integration: `autofix add DateRepeatPicker.dom`,
    /// `autofix add DateRepeatPicker.with_theme`, ... for a type api.json did
    /// not have yet, then one `autofix apply`: every add wrote the same file
    /// names, each overwrote the one before, and only one method per new type
    /// reached api.json (every patch "Successfully applied"). Two adds of one
    /// new type keep both methods.
    #[test]
    fn two_add_patches_for_one_new_type_keep_both_methods() {
        let mut index = TypeIndex::new();
        index.add_type_for_test(type_def(
            "Picker",
            "widgets::picker",
            r#"impl Picker {
                pub fn create() -> Self { todo!() }
                pub fn with_bold(self, bold: bool) -> Self { todo!() }
                pub fn with_italic(self, italic: bool) -> Self { todo!() }
            }"#,
        ));
        let api = empty_api();
        let v = api.get_version("0.2.0").expect("version");

        let dir = tempfile::tempdir().expect("temp dir");
        for spec in ["create", "with_bold", "with_italic"] {
            let (files, _) =
                new_type_patch_files("Picker", spec, &index, v, "0.2.0").expect("add patches");
            write_patch_files(dir.path(), &files).expect("written");
        }

        let mut api = api.clone();
        let stats =
            crate::patch::apply_patches_from_directory(&mut api, dir.path()).expect("applies");
        assert!(!stats.has_errors(), "{stats:?}");
        let class =
            api.get_version("0.2.0").expect("version").api["widgets"].classes["Picker"].clone();
        let mut functions: Vec<String> = class
            .functions
            .iter()
            .flat_map(|f| f.keys().cloned())
            .collect();
        functions.sort();
        assert_eq!(functions, vec!["with_bold", "with_italic"]);
        let constructors: Vec<String> = class
            .constructors
            .iter()
            .flat_map(|c| c.keys().cloned())
            .collect();
        assert_eq!(constructors, vec!["create"]);
        assert_eq!(
            class.external.as_deref(),
            Some("azul_layout::widgets::picker::Picker"),
            "the type patch is there too"
        );
    }
}
