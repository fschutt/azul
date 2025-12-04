use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Context, Result};
use indexmap::IndexMap;
use serde_derive::{Deserialize, Serialize};

use crate::api::{
    ApiData, CallbackDefinition, ClassData, ConstantData, EnumVariantData, FieldData, FunctionData,
    ModuleData, TypeAliasInfo, VersionData,
};

// Source code parsing and retrieval modules
pub mod fallback;
pub mod index;
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
    /// If true, remove this class entirely from the API
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remove: Option<bool>,
    /// Update external import path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external: Option<String>,
    /// Update documentation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    /// Update derive attributes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derive: Option<Vec<String>>,
    /// Update is_boxed_object flag
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_boxed_object: Option<bool>,
    /// Traits with manual `impl Trait for Type` blocks (e.g., ["Clone", "Drop"])
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_impls: Option<Vec<String>>,
    // DEPRECATED: Use custom_impls: ["Clone"] instead
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clone: Option<bool>,
    // DEPRECATED: Use custom_impls: ["Drop"] instead
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_destructor: Option<bool>,
    /// Update serde attribute
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serde: Option<String>,
    /// Update repr attribute
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repr: Option<String>,
    /// Update const value type
    #[serde(rename = "const", skip_serializing_if = "Option::is_none")]
    pub const_value_type: Option<String>,
    /// Patch or replace constants
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constants: Option<Vec<IndexMap<String, ConstantData>>>,
    /// Patch or replace struct fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub struct_fields: Option<Vec<IndexMap<String, FieldData>>>,
    /// Patch or replace enum fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_fields: Option<Vec<IndexMap<String, EnumVariantData>>>,
    /// Generic type parameters (e.g., ["T"] for PhysicalPosition<T>)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generic_params: Option<Vec<String>>,
    /// Type alias information (for generic type instantiations)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_alias: Option<TypeAliasInfo>,
    /// Patch or replace callback typedef
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callback_typedef: Option<CallbackDefinition>,
    /// Patch or replace constructors
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constructors: Option<IndexMap<String, FunctionData>>,
    /// Patch or replace functions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub functions: Option<IndexMap<String, FunctionData>>,
    /// Add note about patch application
    #[serde(skip)]
    pub _patched: bool,
}

impl ClassPatch {
    /// Check if this patch only contains external path changes
    /// These are the safest patches to apply automatically
    pub fn is_path_only(&self) -> bool {
        self.external.is_some()
            && self.remove.is_none()
            && self.doc.is_none()
            && self.derive.is_none()
            && self.is_boxed_object.is_none()
            && self.custom_impls.is_none()
            && self.clone.is_none()
            && self.custom_destructor.is_none()
            && self.serde.is_none()
            && self.repr.is_none()
            && self.const_value_type.is_none()
            && self.constants.is_none()
            && self.struct_fields.is_none()
            && self.enum_fields.is_none()
            && self.generic_params.is_none()
            && self.type_alias.is_none()
            && self.callback_typedef.is_none()
            && self.constructors.is_none()
            && self.functions.is_none()
    }

    /// Check if this patch is a removal patch
    pub fn is_removal(&self) -> bool {
        self.remove == Some(true)
    }

    /// Check if this patch is completely empty (no fields set)
    pub fn is_empty(&self) -> bool {
        self.remove.is_none()
            && self.external.is_none()
            && self.doc.is_none()
            && self.derive.is_none()
            && self.is_boxed_object.is_none()
            && self.custom_impls.is_none()
            && self.clone.is_none()
            && self.custom_destructor.is_none()
            && self.serde.is_none()
            && self.repr.is_none()
            && self.const_value_type.is_none()
            && self.constants.is_none()
            && self.struct_fields.is_none()
            && self.enum_fields.is_none()
            && self.generic_params.is_none()
            && self.type_alias.is_none()
            && self.callback_typedef.is_none()
            && self.constructors.is_none()
            && self.functions.is_none()
    }
}

impl ApiPatch {
    /// Load patch from file
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read patch file: {}", path.display()))?;

        serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse patch file: {}", path.display()))
    }

    /// Load all patches from a directory
    pub fn from_directory(dir_path: &Path) -> Result<Vec<(String, Self)>> {
        if !dir_path.is_dir() {
            anyhow::bail!("Not a directory: {}", dir_path.display());
        }

        let mut patches = Vec::new();

        for entry in fs::read_dir(dir_path)
            .with_context(|| format!("Failed to read directory: {}", dir_path.display()))?
        {
            let entry = entry?;
            let path = entry.path();

            // Process both .patch and .patch.json files
            let is_patch_file = path
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.ends_with(".patch") || s.ends_with(".patch.json"))
                .unwrap_or(false);

            if is_patch_file {
                let filename = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                match Self::from_file(&path) {
                    Ok(patch) => patches.push((filename, patch)),
                    Err(e) => {
                        eprintln!("[WARN]  Failed to load {}: {}", filename, e);
                    }
                }
            }
        }

        Ok(patches)
    }

    /// Apply patch to API data
    /// Returns (patches_applied, errors) - errors are collected but don't stop processing
    pub fn apply(&self, api_data: &mut ApiData) -> Result<(usize, Vec<String>)> {
        let mut patches_applied = 0;
        let mut all_errors = Vec::new();

        for (version_name, version_patch) in &self.versions {
            if let Some(version_data) = api_data.0.get_mut(version_name) {
                let result = apply_version_patch(version_data, version_patch);
                patches_applied += result.patches_applied;
                all_errors.extend(result.errors);
            } else {
                all_errors.push(format!("Version '{}' not found in API data", version_name));
            }
        }

        Ok((patches_applied, all_errors))
    }

    /// Check if this patch only contains path-only changes
    /// Returns true if ALL classes in this patch only change external paths
    pub fn is_path_only(&self) -> bool {
        for version_patch in self.versions.values() {
            for module_patch in version_patch.modules.values() {
                for class_patch in module_patch.classes.values() {
                    if !class_patch.is_path_only() {
                        return false;
                    }
                }
            }
        }
        true
    }
}

/// Statistics about patch application
#[derive(Debug, Default)]
pub struct PatchStats {
    pub total_patches: usize,
    pub successful: usize,
    pub failed: usize,
    pub total_changes: usize,
    pub failed_patches: Vec<(String, String)>, // (filename, error) - complete failures
    pub patch_errors: Vec<(String, String)>,   // (filename, error) - partial failures (module not found, etc.)
}

impl PatchStats {
    /// Returns true if there were any errors (failed patches or patch errors)
    pub fn has_errors(&self) -> bool {
        self.failed > 0 || !self.patch_errors.is_empty()
    }

    pub fn print_summary(&self) {
        println!("\n╔════════════════════════════════════════════════════════════════╗");
        println!("║                    Patch Summary                               ║");
        println!("╚════════════════════════════════════════════════════════════════╝\n");

        println!("[STATS] Statistics:");
        println!("  Total patch files: {}", self.total_patches);
        println!("  Successfully applied: {}", self.successful);
        println!("  Failed: {}", self.failed);
        println!("  Total changes made: {}", self.total_changes);

        if !self.failed_patches.is_empty() {
            println!("\nERROR: Failed patches:");
            for (filename, error) in &self.failed_patches {
                println!("  • {}: {}", filename, error);
            }
        }

        if !self.patch_errors.is_empty() {
            println!("\nERROR: Patch errors ({} total):", self.patch_errors.len());
            for (filename, error) in &self.patch_errors {
                println!("  • {}: {}", filename, error);
            }
        }

        if !self.has_errors() {
            println!("\n[OK] All patches applied successfully!");
        } else {
            println!("\nERROR: Some patches had errors");
        }
    }
}

/// Apply patches from a directory
pub fn apply_patches_from_directory(api_data: &mut ApiData, dir_path: &Path) -> Result<PatchStats> {
    let patches = ApiPatch::from_directory(dir_path)?;

    let mut stats = PatchStats {
        total_patches: patches.len(),
        ..Default::default()
    };

    if patches.is_empty() {
        println!("[INFO]  No patch files found in {}", dir_path.display());
        return Ok(stats);
    }

    println!("[FIX] Applying {} patch files...\n", patches.len());

    for (filename, patch) in patches {
        print!("  Applying {}... ", filename);

        match patch.apply(api_data) {
            Ok((count, errors)) => {
                if errors.is_empty() {
                    println!("[OK] ({} changes)", count);
                    stats.successful += 1;
                } else {
                    // Patch had some changes but also errors
                    if count > 0 {
                        println!("WARN:  ({} changes, {} errors)", count, errors.len());
                        stats.successful += 1;
                    } else {
                        println!("ERROR: ({} errors)", errors.len());
                        stats.failed += 1;
                    }
                    for error in errors {
                        stats.patch_errors.push((filename.clone(), error));
                    }
                }
                stats.total_changes += count;
            }
            Err(e) => {
                println!("ERROR:");
                stats.failed += 1;
                stats.failed_patches.push((filename, e.to_string()));
            }
        }
    }

    Ok(stats)
}

/// Explain what patches in a directory will do without applying them
pub fn explain_patches(dir_path: &Path) -> Result<()> {
    let patches = ApiPatch::from_directory(dir_path)?;

    if patches.is_empty() {
        println!("[INFO]  No patch files found in {}", dir_path.display());
        return Ok(());
    }

    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║                    PATCH EXPLANATION                              ║");
    println!("╚══════════════════════════════════════════════════════════════════╝\n");

    // Categorize patches
    let mut path_only_patches = Vec::new();
    let mut structural_patches = Vec::new();

    for (filename, patch) in patches {
        if patch.is_path_only() {
            path_only_patches.push((filename, patch));
        } else {
            structural_patches.push((filename, patch));
        }
    }

    println!("[STATS] Summary:");
    println!(
        "  Total patches: {}",
        path_only_patches.len() + structural_patches.len()
    );
    println!(
        "  Path-only patches: {} (safe to apply)",
        path_only_patches.len()
    );
    println!(
        "  Structural patches: {} (need review)",
        structural_patches.len()
    );

    // Show path-only patches
    if !path_only_patches.is_empty() {
        println!("\n[FIX] PATH-ONLY PATCHES ({})", path_only_patches.len());
        println!(
            "   These patches only update external paths and are safe to apply automatically.\n"
        );

        for (filename, patch) in path_only_patches.iter().take(10) {
            for (version_name, version_patch) in &patch.versions {
                for (module_name, module_patch) in &version_patch.modules {
                    for (class_name, class_patch) in &module_patch.classes {
                        if let Some(external) = &class_patch.external {
                            println!("  ┌─ {}", filename);
                            println!("  │  = note: need to update path to '{}'", external);
                            println!("  │");
                        }
                    }
                }
            }
        }

        if path_only_patches.len() > 10 {
            println!(
                "  ... and {} more path-only patches",
                path_only_patches.len() - 10
            );
        }
    }

    // Show structural patches
    if !structural_patches.is_empty() {
        println!("\n[BUILD]  STRUCTURAL PATCHES ({})", structural_patches.len());
        println!("   These patches add new types or modify structures.\n");

        for (filename, patch) in structural_patches.iter().take(10) {
            for (version_name, version_patch) in &patch.versions {
                for (module_name, module_patch) in &version_patch.modules {
                    for (class_name, class_patch) in &module_patch.classes {
                        println!("  ┌─ {}", filename);

                        let mut reasons = Vec::new();

                        if let Some(external) = &class_patch.external {
                            reasons.push(format!("external path: {}", external));
                        }

                        if class_patch.struct_fields.is_some() {
                            reasons.push("add struct fields".to_string());
                        }
                        if class_patch.enum_fields.is_some() {
                            reasons.push("add enum variants".to_string());
                        }
                        if class_patch.functions.is_some() {
                            reasons.push("add functions".to_string());
                        }
                        if class_patch.constructors.is_some() {
                            reasons.push("add constructors".to_string());
                        }
                        if class_patch.callback_typedef.is_some() {
                            reasons.push("add callback typedef".to_string());
                        }

                        if !reasons.is_empty() {
                            for (i, reason) in reasons.iter().enumerate() {
                                if i == 0 {
                                    println!("  │  = note: {}", reason);
                                } else {
                                    println!("  │          {}", reason);
                                }
                            }
                        }

                        println!("  │");
                    }
                }
            }
        }

        if structural_patches.len() > 10 {
            println!(
                "  ... and {} more structural patches",
                structural_patches.len() - 10
            );
        }
    }

    println!("\n[TIP] NEXT STEPS:");
    if !path_only_patches.is_empty() {
        println!(
            "  1. Apply safe patches: azul-doc patch safe {}",
            dir_path.display()
        );
    }
    if !structural_patches.is_empty() {
        println!("  2. Review structural patches in: {}", dir_path.display());
        println!(
            "  3. Apply all patches: azul-doc patch {}",
            dir_path.display()
        );
    }

    Ok(())
}

/// Apply only path-only patches from a directory and delete them
/// This is a safe operation that only updates external paths
/// Returns statistics about applied patches
pub fn apply_path_only_patches(api_data: &mut ApiData, dir_path: &Path) -> Result<PatchStats> {
    let patches = ApiPatch::from_directory(dir_path)?;

    let mut stats = PatchStats {
        total_patches: patches.len(),
        ..Default::default()
    };

    if patches.is_empty() {
        println!("[INFO]  No patch files found in {}", dir_path.display());
        return Ok(stats);
    }

    // First pass: identify path-only patches
    let mut path_only_patches = Vec::new();
    let mut other_patches = Vec::new();

    for (filename, patch) in patches {
        if patch.is_path_only() {
            path_only_patches.push((filename, patch));
        } else {
            other_patches.push(filename);
        }
    }

    if path_only_patches.is_empty() {
        println!("[INFO]  No path-only patches found in {}", dir_path.display());
        println!(
            "   All {} patches contain structural changes",
            stats.total_patches
        );
        return Ok(stats);
    }

    println!(
        "[FIX] Found {} path-only patches (out of {} total)",
        path_only_patches.len(),
        stats.total_patches
    );
    println!("   These patches only update external paths and are safe to apply\n");

    // Apply path-only patches
    for (filename, patch) in &path_only_patches {
        print!("  Applying {}... ", filename);

        match patch.apply(api_data) {
            Ok((count, errors)) => {
                if errors.is_empty() {
                    println!("[OK] ({} changes)", count);
                    stats.successful += 1;
                } else {
                    if count > 0 {
                        println!("[WARN]  ({} changes, {} errors)", count, errors.len());
                        stats.successful += 1;
                    } else {
                        println!("ERROR: ({} errors)", errors.len());
                        stats.failed += 1;
                    }
                    for error in errors {
                        stats.patch_errors.push((filename.clone(), error));
                    }
                }
                stats.total_changes += count;
            }
            Err(e) => {
                println!("ERROR:");
                stats.failed += 1;
                stats.failed_patches.push((filename.clone(), e.to_string()));
            }
        }
    }

    // Delete successfully applied patches
    if stats.successful > 0 {
        println!(
            "\n[DELETE]  Deleting {} successfully applied patch files...",
            stats.successful
        );

        for (filename, _) in &path_only_patches {
            // Skip if this patch failed to apply
            if stats.failed_patches.iter().any(|(f, _)| f == filename) {
                continue;
            }

            let patch_path = dir_path.join(filename);
            if let Err(e) = fs::remove_file(&patch_path) {
                eprintln!("   [WARN]  Warning: Failed to delete {}: {}", filename, e);
            } else {
                println!("   [OK] Deleted {}", filename);
            }
        }
    }

    // Summary
    println!("\n[STATS] Path-only patches summary:");
    println!("  Applied and deleted: {}", stats.successful);
    println!("  Failed: {}", stats.failed);
    println!("  Total changes made: {}", stats.total_changes);

    if !other_patches.is_empty() {
        println!("\n[DIR] Remaining patches with structural changes:");
        for filename in &other_patches {
            println!("  • {}", filename);
        }
        println!(
            "\n[TIP] Apply these manually with:  patch {}",
            dir_path.display()
        );
    }

    if stats.failed > 0 {
        println!("\nERROR: Failed patches:");
        for (filename, error) in &stats.failed_patches {
            println!("  • {}: {}", filename, error);
        }
    }

    Ok(stats)
}

/// Result of applying a patch, containing both success count and any errors
#[derive(Debug, Default)]
pub struct PatchResult {
    pub patches_applied: usize,
    pub errors: Vec<String>,
}

fn apply_version_patch(version_data: &mut VersionData, patch: &VersionPatch) -> PatchResult {
    let mut result = PatchResult::default();

    for (module_name, module_patch) in &patch.modules {
        if let Some(module_data) = version_data.api.get_mut(module_name) {
            match apply_module_patch(module_data, module_patch, module_name) {
                Ok(count) => result.patches_applied += count,
                Err(e) => result.errors.push(e.to_string()),
            }
        } else {
            // Module doesn't exist - create it with the classes from the patch
            println!("  [ADD] Creating new module '{}' from patch", module_name);
            let mut new_module = crate::api::ModuleData {
                doc: None,
                classes: indexmap::IndexMap::new(),
            };
            match apply_module_patch(&mut new_module, module_patch, module_name) {
                Ok(count) => {
                    version_data.api.insert(module_name.clone(), new_module);
                    result.patches_applied += count;
                }
                Err(e) => result.errors.push(e.to_string()),
            }
        }
    }

    result
}

fn apply_module_patch(
    module_data: &mut ModuleData,
    patch: &ModulePatch,
    module_name: &str,
) -> Result<usize> {
    let mut patches_applied = 0;

    for (class_name, class_patch) in &patch.classes {
        // Handle removal patches first
        if class_patch.is_removal() {
            if module_data.classes.shift_remove(class_name).is_some() {
                println!(
                    "  [REMOVE] Removed class {}.{} from API",
                    module_name, class_name
                );
                patches_applied += 1;
            } else {
                eprintln!(
                    "Warning: Cannot remove class '{}' from module '{}' - not found",
                    class_name, module_name
                );
            }
            continue;
        }
        
        if let Some(class_data) = module_data.classes.get_mut(class_name) {
            // Update existing class
            patches_applied += apply_class_patch(class_data, class_patch, module_name, class_name)?;
        } else {
            // Insert new class from patch
            if !class_patch.is_empty() {
                println!(
                    "  [ADD] Adding new class {}.{} from patch",
                    module_name, class_name
                );
                let new_class_data = class_patch_to_class_data(class_patch);
                module_data
                    .classes
                    .insert(class_name.clone(), new_class_data);
                patches_applied += 1;
            } else {
                eprintln!(
                    "Warning: Class '{}' not found in module '{}' and patch is empty",
                    class_name, module_name
                );
            }
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
            "  [NOTE] Patching {}.{}: external path",
            module_name, class_name
        );
        println!("     Old: {:?}", class_data.external);
        println!("     New: {}", new_external);
        class_data.external = Some(new_external.clone());
        patches_applied += 1;
    }

    if let Some(new_doc) = &patch.doc {
        println!(
            "  [NOTE] Patching {}.{}: documentation",
            module_name, class_name
        );
        class_data.doc = Some(new_doc.clone());
        patches_applied += 1;
    }

    if let Some(new_derive) = &patch.derive {
        println!(
            "  [NOTE] Patching {}.{}: derive attributes",
            module_name, class_name
        );
        println!("     Old: {:?}", class_data.derive);
        println!("     New: {:?}", new_derive);
        class_data.derive = Some(new_derive.clone());
        patches_applied += 1;
    }

    if let Some(new_is_boxed) = patch.is_boxed_object {
        println!(
            "  [NOTE] Patching {}.{}: is_boxed_object",
            module_name, class_name
        );
        println!("     Old: {}", class_data.is_boxed_object);
        println!("     New: {}", new_is_boxed);
        class_data.is_boxed_object = new_is_boxed;
        patches_applied += 1;
    }

    if let Some(new_clone) = patch.clone {
        println!("  [NOTE] Patching {}.{}: clone", module_name, class_name);
        println!("     Old: {:?}", class_data.clone);
        println!("     New: {}", new_clone);
        class_data.clone = Some(new_clone);
        patches_applied += 1;
    }

    if let Some(new_custom_destructor) = patch.custom_destructor {
        println!(
            "  [NOTE] Patching {}.{}: custom_destructor",
            module_name, class_name
        );
        println!("     Old: {:?}", class_data.custom_destructor);
        println!("     New: {}", new_custom_destructor);
        class_data.custom_destructor = Some(new_custom_destructor);
        patches_applied += 1;
    }

    if let Some(ref new_custom_impls) = patch.custom_impls {
        println!(
            "  [NOTE] Patching {}.{}: custom_impls",
            module_name, class_name
        );
        println!("     Old: {:?}", class_data.custom_impls);
        println!("     New: {:?}", new_custom_impls);
        class_data.custom_impls = Some(new_custom_impls.clone());
        patches_applied += 1;
    }

    if let Some(new_serde) = &patch.serde {
        println!("  [NOTE] Patching {}.{}: serde", module_name, class_name);
        println!("     Old: {:?}", class_data.serde);
        println!("     New: {}", new_serde);
        class_data.serde = Some(new_serde.clone());
        patches_applied += 1;
    }

    if let Some(new_repr) = &patch.repr {
        println!("  [NOTE] Patching {}.{}: repr", module_name, class_name);
        println!("     Old: {:?}", class_data.repr);
        println!("     New: {}", new_repr);
        class_data.repr = Some(new_repr.clone());
        patches_applied += 1;
    }

    if let Some(new_const_value_type) = &patch.const_value_type {
        println!(
            "  [NOTE] Patching {}.{}: const value type",
            module_name, class_name
        );
        println!("     Old: {:?}", class_data.const_value_type);
        println!("     New: {}", new_const_value_type);
        class_data.const_value_type = Some(new_const_value_type.clone());
        patches_applied += 1;
    }

    if let Some(new_constants) = &patch.constants {
        println!("  [NOTE] Patching {}.{}: constants", module_name, class_name);
        class_data.constants = Some(new_constants.clone());
        patches_applied += 1;
    }

    if let Some(new_struct_fields) = &patch.struct_fields {
        println!(
            "  [NOTE] Patching {}.{}: struct_fields",
            module_name, class_name
        );
        class_data.struct_fields = Some(new_struct_fields.clone());
        patches_applied += 1;
    }

    if let Some(new_enum_fields) = &patch.enum_fields {
        println!("  [NOTE] Patching {}.{}: enum_fields", module_name, class_name);
        class_data.enum_fields = Some(new_enum_fields.clone());
        patches_applied += 1;
    }

    if let Some(new_callback_typedef) = &patch.callback_typedef {
        println!(
            "  [NOTE] Patching {}.{}: callback_typedef",
            module_name, class_name
        );
        class_data.callback_typedef = Some(new_callback_typedef.clone());
        patches_applied += 1;
    }

    if let Some(new_constructors) = &patch.constructors {
        println!("  [NOTE] Patching {}.{}: constructors", module_name, class_name);
        class_data.constructors = Some(new_constructors.clone());
        patches_applied += 1;
    }

    if let Some(new_functions) = &patch.functions {
        println!("  [NOTE] Patching {}.{}: functions", module_name, class_name);
        class_data.functions = Some(new_functions.clone());
        patches_applied += 1;
    }

    if let Some(new_type_alias) = &patch.type_alias {
        println!("  [NOTE] Patching {}.{}: type_alias", module_name, class_name);
        println!("     Old: {:?}", class_data.type_alias);
        println!("     New: {:?}", new_type_alias);
        class_data.type_alias = Some(new_type_alias.clone());
        patches_applied += 1;
    }

    Ok(patches_applied)
}

/// Convert a ClassPatch to ClassData for inserting new classes
fn class_patch_to_class_data(patch: &ClassPatch) -> ClassData {
    ClassData {
        doc: patch.doc.clone(),
        external: patch.external.clone(),
        is_boxed_object: patch.is_boxed_object.unwrap_or(false),
        custom_impls: patch.custom_impls.clone(),
        clone: patch.clone,
        custom_destructor: patch.custom_destructor,
        derive: patch.derive.clone(),
        serde: patch.serde.clone(),
        const_value_type: patch.const_value_type.clone(),
        constants: patch.constants.clone(),
        struct_fields: patch.struct_fields.clone(),
        enum_fields: patch.enum_fields.clone(),
        callback_typedef: patch.callback_typedef.clone(),
        constructors: patch.constructors.clone(),
        functions: patch.functions.clone(),
        use_patches: None,
        repr: patch.repr.clone(),
        generic_params: patch.generic_params.clone(),
        type_alias: patch.type_alias.clone(),
        vec_ref_element_type: None,
        vec_ref_is_mut: false,
    }
}

/// Print all external import paths for debugging/tracking
pub fn print_import_paths(api_data: &ApiData) {
    println!("\n[PKG] API Import Paths:\n");

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_patch_preserves_existing_fields() {
        // Create a test API with a class that has documentation and external path
        let mut api_data = ApiData(BTreeMap::from([(
            "1.0.0".to_string(),
            VersionData {
                apiversion: 1,
                git: "test".to_string(),
                date: "2025-01-01".to_string(),
                examples: vec![],
                notes: vec![],
                api: IndexMap::from([(
                    "test_module".to_string(),
                    ModuleData {
                        doc: Some("Test module".to_string()),
                        classes: IndexMap::from([(
                            "TestClass".to_string(),
                            ClassData {
                                doc: Some("Original documentation".to_string()),
                                external: Some("original::path::TestClass".to_string()),
                                is_boxed_object: false,
                                clone: Some(true),
                                custom_destructor: None,
                                derive: Some(vec!["Debug".to_string()]),
                                serde: None,
                                const_value_type: None,
                                constants: None,
                                struct_fields: Some(vec![IndexMap::from([(
                                    "field1".to_string(),
                                    FieldData {
                                        r#type: "String".to_string(),
                                        doc: Some("Field documentation".to_string()),
                                        derive: None,
                                    },
                                )])]),
                                enum_fields: None,
                                callback_typedef: None,
                                constructors: None,
                                functions: None,
                                use_patches: None,
                                repr: None,
                            },
                        )]),
                    },
                )]),
            },
        )]));

        // Create a patch that only updates struct_fields, leaving doc and external unchanged
        let patch = ApiPatch {
            versions: BTreeMap::from([(
                "1.0.0".to_string(),
                VersionPatch {
                    modules: BTreeMap::from([(
                        "test_module".to_string(),
                        ModulePatch {
                            classes: BTreeMap::from([(
                                "TestClass".to_string(),
                                ClassPatch {
                                    // Only updating struct_fields, doc and external are None
                                    doc: None,
                                    external: None,
                                    struct_fields: Some(vec![IndexMap::from([
                                        (
                                            "field1".to_string(),
                                            FieldData {
                                                r#type: "UpdatedType".to_string(),
                                                doc: Some("Field documentation".to_string()),
                                                derive: None,
                                            },
                                        ),
                                        (
                                            "field2".to_string(),
                                            FieldData {
                                                r#type: "NewField".to_string(),
                                                doc: None,
                                                derive: None,
                                            },
                                        ),
                                    ])]),
                                    ..Default::default()
                                },
                            )]),
                        },
                    )]),
                },
            )]),
        };

        // Apply the patch
        let result = patch.apply(&mut api_data);
        assert!(result.is_ok(), "Patch application should succeed");

        // Verify that doc and external were preserved
        let class_data = &api_data.0["1.0.0"].api["test_module"].classes["TestClass"];

        assert_eq!(
            class_data.doc,
            Some("Original documentation".to_string()),
            "Documentation should be preserved when not in patch"
        );

        assert_eq!(
            class_data.external,
            Some("original::path::TestClass".to_string()),
            "External path should be preserved when not in patch"
        );

        assert_eq!(
            class_data.clone,
            Some(true),
            "Clone flag should be preserved when not in patch"
        );

        assert_eq!(
            class_data.derive,
            Some(vec!["Debug".to_string()]),
            "Derive attributes should be preserved when not in patch"
        );

        // Verify that struct_fields were updated
        assert!(
            class_data.struct_fields.is_some(),
            "Struct fields should be present"
        );
        let fields = class_data.struct_fields.as_ref().unwrap().first().unwrap();

        assert_eq!(
            fields.get("field1").map(|f| f.r#type.as_str()),
            Some("UpdatedType"),
            "Existing field should be updated"
        );

        assert_eq!(
            fields.get("field2").map(|f| f.r#type.as_str()),
            Some("NewField"),
            "New field should be added"
        );
    }

    #[test]
    fn test_patch_can_update_fields() {
        // Create a test API with a class
        let mut api_data = ApiData(BTreeMap::from([(
            "1.0.0".to_string(),
            VersionData {
                apiversion: 1,
                git: "test".to_string(),
                date: "2025-01-01".to_string(),
                examples: vec![],
                notes: vec![],
                api: IndexMap::from([(
                    "test_module".to_string(),
                    ModuleData {
                        doc: Some("Test module".to_string()),
                        classes: IndexMap::from([(
                            "TestClass".to_string(),
                            ClassData {
                                doc: Some("Original documentation".to_string()),
                                external: Some("original::path::TestClass".to_string()),
                                ..Default::default()
                            },
                        )]),
                    },
                )]),
            },
        )]));

        // Create a patch that updates both doc and external
        let patch = ApiPatch {
            versions: BTreeMap::from([(
                "1.0.0".to_string(),
                VersionPatch {
                    modules: BTreeMap::from([(
                        "test_module".to_string(),
                        ModulePatch {
                            classes: BTreeMap::from([(
                                "TestClass".to_string(),
                                ClassPatch {
                                    doc: Some("Updated documentation".to_string()),
                                    external: Some("new::path::TestClass".to_string()),
                                    ..Default::default()
                                },
                            )]),
                        },
                    )]),
                },
            )]),
        };

        // Apply the patch
        let result = patch.apply(&mut api_data);
        assert!(result.is_ok(), "Patch application should succeed");

        // Verify that doc and external were updated
        let class_data = &api_data.0["1.0.0"].api["test_module"].classes["TestClass"];

        assert_eq!(
            class_data.doc,
            Some("Updated documentation".to_string()),
            "Documentation should be updated when in patch"
        );

        assert_eq!(
            class_data.external,
            Some("new::path::TestClass".to_string()),
            "External path should be updated when in patch"
        );
    }
}

/// Rename classes where the external path's last segment differs from the API name
/// This updates all references throughout the API
pub fn normalize_class_names(api_data: &mut ApiData) -> Result<usize> {
    let mut renames = Vec::new();
    
    // The prefix used in generated code
    const PREFIX: &str = "Az";

    // First pass: collect all renames needed
    for (version_name, version_data) in &api_data.0 {
        for (module_name, module_data) in &version_data.api {
            for (class_name, class_data) in &module_data.classes {
                if let Some(ref external) = class_data.external {
                    // Extract last segment from external path
                    let external_name = external.rsplit("::").next().unwrap_or(external.as_str());

                    // If names don't match, schedule rename
                    if external_name != class_name {
                        // IMPORTANT: Don't rename if external_name starts with PREFIX
                        // because that would cause double-prefixing (AzAzString)
                        // The api.json should use the UNPREFIXED name (String, not AzString)
                        if external_name.starts_with(PREFIX) {
                            // Skip - the external path has prefix, api.json name should be without prefix
                            continue;
                        }
                        
                        renames.push((
                            version_name.clone(),
                            module_name.clone(),
                            class_name.clone(),
                            external_name.to_string(),
                        ));
                    }
                }
            }
        }
    }

    let rename_count = renames.len();

    if rename_count == 0 {
        return Ok(0);
    }

    println!("\n[REFRESH] Normalizing {} class names...", rename_count);

    // Second pass: apply renames
    for (version_name, module_name, old_name, new_name) in renames {
        println!(
            "  {} → {} (in {}.{})",
            old_name, new_name, module_name, version_name
        );

        // Get mutable reference to the version
        if let Some(version_data) = api_data.0.get_mut(&version_name) {
            // Rename in the class map
            if let Some(module_data) = version_data.api.get_mut(&module_name) {
                if let Some(class_data) = module_data.classes.shift_remove(&old_name) {
                    module_data.classes.insert(new_name.clone(), class_data);
                }
            }

            // Update all type references throughout the API
            update_type_references(&mut version_data.api, &old_name, &new_name);
        }
    }

    Ok(rename_count)
}

/// Update all type references in the API from old_name to new_name
fn update_type_references(api: &mut IndexMap<String, ModuleData>, old_name: &str, new_name: &str) {
    for module_data in api.values_mut() {
        for class_data in module_data.classes.values_mut() {
            // Update struct fields
            if let Some(ref mut struct_fields) = class_data.struct_fields {
                for field_map in struct_fields.iter_mut() {
                    for field_data in field_map.values_mut() {
                        if field_data.r#type == old_name {
                            field_data.r#type = new_name.to_string();
                        }
                    }
                }
            }

            // Update enum variants
            if let Some(ref mut enum_fields) = class_data.enum_fields {
                for variant_map in enum_fields.iter_mut() {
                    for variant_data in variant_map.values_mut() {
                        if let Some(ref mut variant_type) = variant_data.r#type {
                            if variant_type == old_name {
                                *variant_type = new_name.to_string();
                            }
                        }
                    }
                }
            }

            // Update callback typedef
            if let Some(ref mut callback_typedef) = class_data.callback_typedef {
                // Update fn_args
                for arg in &mut callback_typedef.fn_args {
                    if arg.r#type == old_name {
                        arg.r#type = new_name.to_string();
                    }
                }

                // Update return type
                if let Some(ref mut ret) = callback_typedef.returns {
                    if ret.r#type == old_name {
                        ret.r#type = new_name.to_string();
                    }
                }
            }

            // Update constructors
            if let Some(ref mut constructors) = class_data.constructors {
                for constructor in constructors.values_mut() {
                    // Update fn_args (Vec<IndexMap<String, String>>)
                    for arg_map in &mut constructor.fn_args {
                        // Each arg_map is {"arg_name": "type"}
                        for (_, arg_type) in arg_map.iter_mut() {
                            if arg_type == old_name {
                                *arg_type = new_name.to_string();
                            }
                        }
                    }

                    // Update return type
                    if let Some(ref mut ret) = constructor.returns {
                        if ret.r#type == old_name {
                            ret.r#type = new_name.to_string();
                        }
                    }
                }
            }

            // Update functions
            if let Some(ref mut functions) = class_data.functions {
                for function in functions.values_mut() {
                    // Update fn_args (Vec<IndexMap<String, String>>)
                    for arg_map in &mut function.fn_args {
                        // Each arg_map is {"arg_name": "type"}
                        for (_, arg_type) in arg_map.iter_mut() {
                            if arg_type == old_name {
                                *arg_type = new_name.to_string();
                            }
                        }
                    }

                    // Update return type
                    if let Some(ref mut ret) = function.returns {
                        if ret.r#type == old_name {
                            ret.r#type = new_name.to_string();
                        }
                    }
                }
            }
        }
    }
}
