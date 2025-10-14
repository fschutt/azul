use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Context, Result};
use serde_derive::{Deserialize, Serialize};

use crate::api::{ApiData, ClassData, ModuleData, VersionData};

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

/// Patch for a single class - all fields optional
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ClassPatch {
    /// Update external import path
    pub external: Option<String>,
    /// Update documentation
    pub doc: Option<String>,
    /// Update derive attributes
    pub derive: Option<Vec<String>>,
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
