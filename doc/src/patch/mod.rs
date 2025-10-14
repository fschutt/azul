use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Context, Result};
use indexmap::IndexMap;
use serde_derive::{Deserialize, Serialize};

use crate::api::{
    ApiData, CallbackDefinition, ClassData, ConstantData, EnumVariantData, FieldData, FunctionData,
    ModuleData, VersionData,
};

// Source code parsing and retrieval modules
pub mod locatesource;
pub mod parser;

/// Patch file structure - allows selective updates to api.json
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ApiPatch {
    /// Patches for specific versions
    #[serde(default)]
    pub versions: BTreeMap<String, VersionPatch>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct VersionPatch {
    /// Patches for specific modules
    #[serde(default)]
    pub modules: BTreeMap<String, ModulePatch>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ModulePatch {
    /// Patches for specific classes
    #[serde(default)]
    pub classes: BTreeMap<String, ClassPatch>,
}

/// Patch for a single class - all fields optional, allowing complete item modification
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ClassPatch {
    /// Update external import path
    pub external: Option<String>,
    /// Update documentation
    pub doc: Option<String>,
    /// Update derive attributes
    pub derive: Option<Vec<String>>,
    /// Update is_boxed_object flag
    pub is_boxed_object: Option<bool>,
    /// Update clone flag
    pub clone: Option<bool>,
    /// Update custom_destructor flag
    pub custom_destructor: Option<bool>,
    /// Update serde attribute
    pub serde: Option<String>,
    /// Update repr attribute
    pub repr: Option<String>,
    /// Update const value type
    #[serde(rename = "const")]
    pub const_value_type: Option<String>,
    /// Patch or replace constants
    pub constants: Option<Vec<IndexMap<String, ConstantData>>>,
    /// Patch or replace struct fields
    pub struct_fields: Option<Vec<IndexMap<String, FieldData>>>,
    /// Patch or replace enum fields
    pub enum_fields: Option<Vec<IndexMap<String, EnumVariantData>>>,
    /// Patch or replace callback typedef
    pub callback_typedef: Option<CallbackDefinition>,
    /// Patch or replace constructors
    pub constructors: Option<IndexMap<String, FunctionData>>,
    /// Patch or replace functions
    pub functions: Option<IndexMap<String, FunctionData>>,
    /// Add note about patch application
    #[serde(skip)]
    pub _patched: bool,
}

impl ApiPatch {
    /// Load patch from file
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read patch file: {}", path.display()))?;

        serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse patch file: {}", path.display()))
    }

    /// Apply patch to API data
    pub fn apply(&self, api_data: &mut ApiData) -> Result<usize> {
        let mut patches_applied = 0;

        for (version_name, version_patch) in &self.versions {
            if let Some(version_data) = api_data.0.get_mut(version_name) {
                patches_applied += apply_version_patch(version_data, version_patch)?;
            } else {
                eprintln!("Warning: Version '{}' not found in API data", version_name);
            }
        }

        Ok(patches_applied)
    }
}

fn apply_version_patch(version_data: &mut VersionData, patch: &VersionPatch) -> Result<usize> {
    let mut patches_applied = 0;

    for (module_name, module_patch) in &patch.modules {
        if let Some(module_data) = version_data.api.get_mut(module_name) {
            patches_applied += apply_module_patch(module_data, module_patch, module_name)?;
        } else {
            eprintln!("Warning: Module '{}' not found", module_name);
        }
    }

    Ok(patches_applied)
}

fn apply_module_patch(
    module_data: &mut ModuleData,
    patch: &ModulePatch,
    module_name: &str,
) -> Result<usize> {
    let mut patches_applied = 0;

    for (class_name, class_patch) in &patch.classes {
        if let Some(class_data) = module_data.classes.get_mut(class_name) {
            patches_applied += apply_class_patch(class_data, class_patch, module_name, class_name)?;
        } else {
            eprintln!(
                "Warning: Class '{}' not found in module '{}'",
                class_name, module_name
            );
        }
    }

    Ok(patches_applied)
}

fn apply_class_patch(
    class_data: &mut ClassData,
    patch: &ClassPatch,
    module_name: &str,
    class_name: &str,
) -> Result<usize> {
    let mut patches_applied = 0;

    if let Some(new_external) = &patch.external {
        println!(
            "  📝 Patching {}.{}: external path",
            module_name, class_name
        );
        println!("     Old: {:?}", class_data.external);
        println!("     New: {}", new_external);
        class_data.external = Some(new_external.clone());
        patches_applied += 1;
    }

    if let Some(new_doc) = &patch.doc {
        println!(
            "  📝 Patching {}.{}: documentation",
            module_name, class_name
        );
        class_data.doc = Some(new_doc.clone());
        patches_applied += 1;
    }

    if let Some(new_derive) = &patch.derive {
        println!(
            "  📝 Patching {}.{}: derive attributes",
            module_name, class_name
        );
        println!("     Old: {:?}", class_data.derive);
        println!("     New: {:?}", new_derive);
        class_data.derive = Some(new_derive.clone());
        patches_applied += 1;
    }

    if let Some(new_is_boxed) = patch.is_boxed_object {
        println!(
            "  📝 Patching {}.{}: is_boxed_object",
            module_name, class_name
        );
        println!("     Old: {}", class_data.is_boxed_object);
        println!("     New: {}", new_is_boxed);
        class_data.is_boxed_object = new_is_boxed;
        patches_applied += 1;
    }

    if let Some(new_clone) = patch.clone {
        println!("  📝 Patching {}.{}: clone", module_name, class_name);
        println!("     Old: {:?}", class_data.clone);
        println!("     New: {}", new_clone);
        class_data.clone = Some(new_clone);
        patches_applied += 1;
    }

    if let Some(new_custom_destructor) = patch.custom_destructor {
        println!(
            "  📝 Patching {}.{}: custom_destructor",
            module_name, class_name
        );
        println!("     Old: {:?}", class_data.custom_destructor);
        println!("     New: {}", new_custom_destructor);
        class_data.custom_destructor = Some(new_custom_destructor);
        patches_applied += 1;
    }

    if let Some(new_serde) = &patch.serde {
        println!("  📝 Patching {}.{}: serde", module_name, class_name);
        println!("     Old: {:?}", class_data.serde);
        println!("     New: {}", new_serde);
        class_data.serde = Some(new_serde.clone());
        patches_applied += 1;
    }

    if let Some(new_repr) = &patch.repr {
        println!("  📝 Patching {}.{}: repr", module_name, class_name);
        println!("     Old: {:?}", class_data.repr);
        println!("     New: {}", new_repr);
        class_data.repr = Some(new_repr.clone());
        patches_applied += 1;
    }

    if let Some(new_const_value_type) = &patch.const_value_type {
        println!(
            "  📝 Patching {}.{}: const value type",
            module_name, class_name
        );
        println!("     Old: {:?}", class_data.const_value_type);
        println!("     New: {}", new_const_value_type);
        class_data.const_value_type = Some(new_const_value_type.clone());
        patches_applied += 1;
    }

    if let Some(new_constants) = &patch.constants {
        println!("  📝 Patching {}.{}: constants", module_name, class_name);
        class_data.constants = Some(new_constants.clone());
        patches_applied += 1;
    }

    if let Some(new_struct_fields) = &patch.struct_fields {
        println!(
            "  📝 Patching {}.{}: struct_fields",
            module_name, class_name
        );
        class_data.struct_fields = Some(new_struct_fields.clone());
        patches_applied += 1;
    }

    if let Some(new_enum_fields) = &patch.enum_fields {
        println!("  📝 Patching {}.{}: enum_fields", module_name, class_name);
        class_data.enum_fields = Some(new_enum_fields.clone());
        patches_applied += 1;
    }

    if let Some(new_callback_typedef) = &patch.callback_typedef {
        println!(
            "  📝 Patching {}.{}: callback_typedef",
            module_name, class_name
        );
        class_data.callback_typedef = Some(new_callback_typedef.clone());
        patches_applied += 1;
    }

    if let Some(new_constructors) = &patch.constructors {
        println!("  📝 Patching {}.{}: constructors", module_name, class_name);
        class_data.constructors = Some(new_constructors.clone());
        patches_applied += 1;
    }

    if let Some(new_functions) = &patch.functions {
        println!("  📝 Patching {}.{}: functions", module_name, class_name);
        class_data.functions = Some(new_functions.clone());
        patches_applied += 1;
    }

    Ok(patches_applied)
}

/// Print all external import paths for debugging/tracking
pub fn print_import_paths(api_data: &ApiData) {
    println!("\n📦 API Import Paths:\n");

    for (version_name, version_data) in &api_data.0 {
        println!("Version: {}", version_name);

        for (module_name, module_data) in &version_data.api {
            for (class_name, class_data) in &module_data.classes {
                if let Some(external) = &class_data.external {
                    println!("  {} → {}", class_name, external);
                } else {
                    println!("  {} → (no external path)", class_name);
                }
            }
        }
        println!();
    }
}

/// Generate a patch template for a specific class
pub fn generate_patch_template(
    version: &str,
    module: &str,
    class: &str,
    new_external: &str,
) -> String {
    serde_json::to_string_pretty(&ApiPatch {
        versions: BTreeMap::from([(
            version.to_string(),
            VersionPatch {
                modules: BTreeMap::from([(
                    module.to_string(),
                    ModulePatch {
                        classes: BTreeMap::from([(
                            class.to_string(),
                            ClassPatch {
                                external: Some(new_external.to_string()),
                                ..Default::default()
                            },
                        )]),
                    },
                )]),
            },
        )]),
    })
    .unwrap_or_else(|_| "{}".to_string())
}
