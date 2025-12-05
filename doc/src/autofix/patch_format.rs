//! JSON Patch Format for api.json modifications
//!
//! This module defines a structured patch format that is:
//! - Human-readable (good for review)
//! - Machine-parseable (for automatic application)
//! - Self-documenting (includes context about what changes)

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A patch file containing one or more operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutofixPatch {
    /// Human-readable description of what this patch does
    pub description: String,
    /// The operations to perform
    pub operations: Vec<PatchOperation>,
}

/// A single patch operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PatchOperation {
    /// Modify an existing type's properties
    Modify(ModifyOperation),
    /// Fix the external path of a type
    PathFix(PathFixOperation),
    /// Add a new type to api.json
    Add(AddOperation),
    /// Remove a type from api.json
    Remove(RemoveOperation),
    /// Move a type to a different module
    MoveModule(MoveModuleOperation),
}

/// Modify an existing type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModifyOperation {
    /// The type name to modify
    pub type_name: String,
    /// Module hint (for finding the type)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    /// Changes to apply
    pub changes: Vec<ModifyChange>,
}

/// A single change within a modify operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum ModifyChange {
    /// Change the external path
    SetExternal {
        old: String,
        new: String,
    },
    /// Set repr(C)
    SetReprC {
        old: bool,
        new: bool,
    },
    /// Add derive attributes
    AddDerives {
        derives: Vec<String>,
    },
    /// Remove derive attributes
    RemoveDerives {
        derives: Vec<String>,
    },
    /// Add a struct field
    AddField {
        name: String,
        #[serde(rename = "type")]
        field_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        doc: Option<String>,
    },
    /// Remove a struct field
    RemoveField {
        name: String,
    },
    /// Change a field's type
    ChangeFieldType {
        name: String,
        old_type: String,
        new_type: String,
    },
    /// Add an enum variant
    AddVariant {
        name: String,
        #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
        variant_type: Option<String>,
    },
    /// Remove an enum variant
    RemoveVariant {
        name: String,
    },
    /// Change a variant's type
    ChangeVariantType {
        name: String,
        old_type: Option<String>,
        new_type: Option<String>,
    },
    /// Set callback_typedef (add the entire callback definition)
    SetCallbackTypedef {
        args: Vec<CallbackArgDef>,
        #[serde(skip_serializing_if = "Option::is_none")]
        returns: Option<String>,
    },
    /// Change a callback argument type
    ChangeCallbackArg {
        arg_index: usize,
        old_type: String,
        new_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        old_ref: Option<String>,
        new_ref: String,
    },
    /// Change callback return type
    ChangeCallbackReturn {
        old_type: Option<String>,
        new_type: Option<String>,
    },
    /// Set type_alias (add the entire type alias definition)
    SetTypeAlias {
        target: String,
    },
    /// Change type alias target
    ChangeTypeAlias {
        old_target: String,
        new_target: String,
    },
}

/// Callback argument definition for patches
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackArgDef {
    #[serde(rename = "type")]
    pub arg_type: String,
    /// Reference kind: "ref", "refmut", or "value"
    #[serde(rename = "ref")]
    pub ref_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Fix the external path of a type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathFixOperation {
    /// The type name
    pub type_name: String,
    /// The old (incorrect) path
    pub old_path: String,
    /// The new (correct) path
    pub new_path: String,
}

/// Add a new type to api.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddOperation {
    /// The type name to add
    pub type_name: String,
    /// The full path (external)
    pub external: String,
    /// The kind of type
    pub kind: TypeKind,
    /// Module to add to
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    /// Initial derive attributes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derives: Option<Vec<String>>,
    /// Whether it's repr(C)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repr_c: Option<bool>,
    /// Struct fields (for structs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub struct_fields: Option<Vec<FieldDef>>,
    /// Enum variants (for enums)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_variants: Option<Vec<VariantDef>>,
}

/// Remove a type from api.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveOperation {
    /// The type name to remove
    pub type_name: String,
    /// The path (for verification/documentation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Reason for removal
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Move a type from one module to another
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveModuleOperation {
    /// The type name to move
    pub type_name: String,
    /// The current (wrong) module
    pub from_module: String,
    /// The target (correct) module
    pub to_module: String,
}

/// Kind of type being added
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeKind {
    Struct,
    Enum,
    TypeAlias,
    Callback,
    CallbackValue,
}

/// Field definition for struct types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDef {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

/// Variant definition for enum types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantDef {
    pub name: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub variant_type: Option<String>,
}

impl AutofixPatch {
    /// Create a new patch with a description
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            operations: Vec::new(),
        }
    }

    /// Add an operation to the patch
    pub fn add_operation(&mut self, op: PatchOperation) {
        self.operations.push(op);
    }

    /// Serialize to pretty JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Parse from JSON
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Generate a human-readable explanation of what this patch does
    pub fn explain(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!("# {}", self.description));
        lines.push(String::new());

        for (i, op) in self.operations.iter().enumerate() {
            if i > 0 {
                lines.push(String::new());
            }
            match op {
                PatchOperation::Modify(m) => {
                    lines.push(format!("MODIFY {}", m.type_name));
                    for change in &m.changes {
                        lines.push(format!("  {}", explain_change(change)));
                    }
                }
                PatchOperation::PathFix(p) => {
                    lines.push(format!("PATH FIX {}", p.type_name));
                    lines.push(format!("  {} → {}", p.old_path, p.new_path));
                }
                PatchOperation::Add(a) => {
                    lines.push(format!("ADD {} ({:?})", a.type_name, a.kind));
                    lines.push(format!("  external: {}", a.external));
                    if let Some(derives) = &a.derives {
                        lines.push(format!("  derives: {}", derives.join(", ")));
                    }
                    if let Some(fields) = &a.struct_fields {
                        for f in fields {
                            lines.push(format!("  field {}: {}", f.name, f.field_type));
                        }
                    }
                    if let Some(variants) = &a.enum_variants {
                        for v in variants {
                            if let Some(ty) = &v.variant_type {
                                lines.push(format!("  variant {}({})", v.name, ty));
                            } else {
                                lines.push(format!("  variant {}", v.name));
                            }
                        }
                    }
                }
                PatchOperation::Remove(r) => {
                    lines.push(format!("REMOVE {}", r.type_name));
                    if let Some(path) = &r.path {
                        lines.push(format!("  path: {}", path));
                    }
                    if let Some(reason) = &r.reason {
                        lines.push(format!("  reason: {}", reason));
                    }
                }
                PatchOperation::MoveModule(m) => {
                    lines.push(format!("MOVE {}", m.type_name));
                    lines.push(format!("  {} → {}", m.from_module, m.to_module));
                }
            }
        }

        lines.join("\n")
    }
}

fn explain_change(change: &ModifyChange) -> String {
    match change {
        ModifyChange::SetExternal { old, new } => {
            format!("external: {} → {}", old, new)
        }
        ModifyChange::SetReprC { old, new } => {
            format!("repr(C): {} → {}", old, new)
        }
        ModifyChange::AddDerives { derives } => {
            format!("+ derive: {}", derives.join(", "))
        }
        ModifyChange::RemoveDerives { derives } => {
            format!("- derive: {}", derives.join(", "))
        }
        ModifyChange::AddField { name, field_type, .. } => {
            format!("+ field {}: {}", name, field_type)
        }
        ModifyChange::RemoveField { name } => {
            format!("- field {}", name)
        }
        ModifyChange::ChangeFieldType { name, old_type, new_type } => {
            format!("~ field {}: {} → {}", name, old_type, new_type)
        }
        ModifyChange::AddVariant { name, variant_type } => {
            if let Some(ty) = variant_type {
                format!("+ variant {}({})", name, ty)
            } else {
                format!("+ variant {}", name)
            }
        }
        ModifyChange::RemoveVariant { name } => {
            format!("- variant {}", name)
        }
        ModifyChange::ChangeVariantType { name, old_type, new_type } => {
            format!("~ variant {}: {:?} → {:?}", name, old_type, new_type)
        }
        ModifyChange::SetCallbackTypedef { args, returns } => {
            let args_str: Vec<String> = args.iter().map(|a| {
                let ref_str = match a.ref_kind.as_str() {
                    "ref" => "&",
                    "refmut" => "&mut ",
                    _ => "",
                };
                format!("{}{}", ref_str, a.arg_type)
            }).collect();
            format!("+ callback_typedef({}) -> {:?}", args_str.join(", "), returns)
        }
        ModifyChange::ChangeCallbackArg { arg_index, old_type, new_type, old_ref, new_ref } => {
            let old_ref_str = match old_ref.as_deref() {
                Some("ref") => "&",
                Some("refmut") => "&mut ",
                _ => "",
            };
            let new_ref_str = match new_ref.as_str() {
                "ref" => "&",
                "refmut" => "&mut ",
                _ => "",
            };
            format!("~ callback arg[{}]: {}{} → {}{}", arg_index, old_ref_str, old_type, new_ref_str, new_type)
        }
        ModifyChange::ChangeCallbackReturn { old_type, new_type } => {
            format!("~ callback return: {:?} → {:?}", old_type, new_type)
        }
        ModifyChange::SetTypeAlias { target } => {
            format!("+ type_alias = {}", target)
        }
        ModifyChange::ChangeTypeAlias { old_target, new_target } => {
            format!("~ type_alias: {} → {}", old_target, new_target)
        }
    }
}

// ============================================================================
// Conversion to legacy ApiPatch format for application
// ============================================================================

use crate::patch::{ApiPatch, ClassPatch, ModulePatch, VersionPatch};
use crate::api::{EnumVariantData, FieldData};
use crate::autofix::module_map::determine_module;
use indexmap::IndexMap;

/// Current API version - should match what's in api.json
pub const API_VERSION: &str = "1.0.0-alpha1";

impl AutofixPatch {
    /// Convert this patch to the legacy ApiPatch format that can be applied
    /// 
    /// Uses the current API version and determines the module from the type name.
    pub fn to_api_patch(&self) -> ApiPatch {
        let mut api_patch = ApiPatch::default();
        
        for op in &self.operations {
            match op {
                PatchOperation::Modify(m) => {
                    let class_patch = self.modify_to_class_patch(m);
                    // Use explicit module or determine from type name
                    let module_name = m.module.clone().unwrap_or_else(|| {
                        let (module, warn) = determine_module(&m.type_name);
                        if warn {
                            eprintln!("Warning: Could not determine module for '{}', using 'misc'", m.type_name);
                        }
                        module
                    });
                    insert_class_patch(&mut api_patch, API_VERSION, &module_name, &m.type_name, class_patch);
                }
                PatchOperation::PathFix(p) => {
                    let class_patch = ClassPatch {
                        external: Some(p.new_path.clone()),
                        ..Default::default()
                    };
                    let (module_name, warn) = determine_module(&p.type_name);
                    if warn {
                        eprintln!("Warning: Could not determine module for '{}', using 'misc'", p.type_name);
                    }
                    insert_class_patch(&mut api_patch, API_VERSION, &module_name, &p.type_name, class_patch);
                }
                PatchOperation::Add(a) => {
                    let class_patch = ClassPatch {
                        external: Some(a.external.clone()),
                        derive: a.derives.clone(),
                        repr: a.repr_c.map(|b| if b { "C".to_string() } else { "Rust".to_string() }),
                        struct_fields: a.struct_fields.as_ref().map(|fields| {
                            vec![fields.iter().map(|f| {
                                (f.name.clone(), FieldData {
                                    r#type: f.field_type.clone(),
                                    doc: Some(f.doc.clone().unwrap_or_default()),
                                    derive: None,
                                })
                            }).collect()]
                        }),
                        enum_fields: a.enum_variants.as_ref().map(|variants| {
                            vec![variants.iter().map(|v| {
                                (v.name.clone(), EnumVariantData {
                                    r#type: v.variant_type.clone(),
                                    doc: None,
                                })
                            }).collect()]
                        }),
                        ..Default::default()
                    };
                    let module_name = a.module.clone().unwrap_or_else(|| {
                        let (module, warn) = determine_module(&a.type_name);
                        if warn {
                            eprintln!("Warning: Could not determine module for '{}', using 'misc'", a.type_name);
                        }
                        module
                    });
                    insert_class_patch(&mut api_patch, API_VERSION, &module_name, &a.type_name, class_patch);
                }
                PatchOperation::Remove(r) => {
                    let class_patch = ClassPatch {
                        remove: Some(true),
                        ..Default::default()
                    };
                    let (module_name, warn) = determine_module(&r.type_name);
                    if warn {
                        eprintln!("Warning: Could not determine module for '{}', using 'misc'", r.type_name);
                    }
                    insert_class_patch(&mut api_patch, API_VERSION, &module_name, &r.type_name, class_patch);
                }
                PatchOperation::MoveModule(m) => {
                    // Create a patch in the source module that moves to target
                    let class_patch = ClassPatch {
                        move_to_module: Some(m.to_module.clone()),
                        ..Default::default()
                    };
                    insert_class_patch(&mut api_patch, API_VERSION, &m.from_module, &m.type_name, class_patch);
                }
            }
        }
        
        api_patch
    }
    
    fn modify_to_class_patch(&self, m: &ModifyOperation) -> ClassPatch {
        let mut patch = ClassPatch::default();
        
        let mut derives_to_add = Vec::new();
        let mut derives_to_remove = Vec::new();
        let mut struct_fields_to_add: IndexMap<String, FieldData> = IndexMap::new();
        let mut enum_variants_to_add: IndexMap<String, EnumVariantData> = IndexMap::new();
        
        for change in &m.changes {
            match change {
                ModifyChange::SetExternal { new, .. } => {
                    patch.external = Some(new.clone());
                }
                ModifyChange::SetReprC { new, .. } => {
                    patch.repr = Some(if *new { "C".to_string() } else { "Rust".to_string() });
                }
                ModifyChange::AddDerives { derives } => {
                    derives_to_add.extend(derives.clone());
                }
                ModifyChange::RemoveDerives { derives } => {
                    derives_to_remove.extend(derives.clone());
                }
                ModifyChange::AddField { name, field_type, doc } => {
                    struct_fields_to_add.insert(name.clone(), FieldData {
                        r#type: field_type.clone(),
                        doc: Some(doc.clone().unwrap_or_default()),
                        derive: None,
                    });
                }
                ModifyChange::RemoveField { .. } => {
                    // Note: The legacy format doesn't support removing fields directly
                }
                ModifyChange::ChangeFieldType { name, new_type, .. } => {
                    struct_fields_to_add.insert(name.clone(), FieldData {
                        r#type: new_type.clone(),
                        doc: None,
                        derive: None,
                    });
                }
                ModifyChange::AddVariant { name, variant_type } => {
                    enum_variants_to_add.insert(name.clone(), EnumVariantData {
                        r#type: variant_type.clone(),
                        doc: None,
                    });
                }
                ModifyChange::RemoveVariant { .. } => {
                    // Note: The legacy format doesn't support removing variants directly
                }
                ModifyChange::ChangeVariantType { name, new_type, .. } => {
                    enum_variants_to_add.insert(name.clone(), EnumVariantData {
                        r#type: new_type.clone(),
                        doc: None,
                    });
                }
                ModifyChange::SetCallbackTypedef { args, returns } => {
                    use crate::api::{CallbackDefinition, CallbackArgData, ReturnTypeData, BorrowMode};
                    
                    let callback_args: Vec<CallbackArgData> = args.iter()
                        .map(|arg| {
                            let ref_kind = match arg.ref_kind.as_str() {
                                "ref" => BorrowMode::Ref,
                                "refmut" => BorrowMode::RefMut,
                                _ => BorrowMode::Value,
                            };
                            CallbackArgData {
                                r#type: arg.arg_type.clone(),
                                ref_kind,
                                doc: None,
                            }
                        })
                        .collect();
                    
                    let callback_returns = returns.as_ref().map(|ret_type| ReturnTypeData {
                        r#type: ret_type.clone(),
                        doc: None,
                    });
                    
                    patch.callback_typedef = Some(CallbackDefinition {
                        fn_args: callback_args,
                        returns: callback_returns,
                    });
                }
                ModifyChange::ChangeCallbackArg { arg_index, new_type, new_ref, .. } => {
                    // Update a specific callback argument
                    // First, get the existing callback_typedef if any, or create a new one
                    use crate::api::{CallbackDefinition, CallbackArgData, BorrowMode};
                    
                    let ref_kind = match new_ref.as_str() {
                        "ref" => BorrowMode::Ref,
                        "refmut" => BorrowMode::RefMut,
                        _ => BorrowMode::Value,
                    };
                    
                    // We need to update just this argument, but for now we'll need the full callback
                    // This could be improved with more granular updates
                    if let Some(ref mut callback_def) = patch.callback_typedef {
                        if let Some(arg) = callback_def.fn_args.get_mut(*arg_index) {
                            arg.r#type = new_type.clone();
                            arg.ref_kind = ref_kind;
                        }
                    }
                }
                ModifyChange::ChangeCallbackReturn { .. } => {
                    // For now, the entire callback_typedef would need to be re-set
                }
                ModifyChange::SetTypeAlias { target } => {
                    use crate::api::TypeAliasInfo;
                    patch.type_alias = Some(TypeAliasInfo {
                        target: target.clone(),
                        generic_args: vec![],
                    });
                }
                ModifyChange::ChangeTypeAlias { new_target, .. } => {
                    use crate::api::TypeAliasInfo;
                    patch.type_alias = Some(TypeAliasInfo {
                        target: new_target.clone(),
                        generic_args: vec![],
                    });
                }
            }
        }
        
        if !derives_to_add.is_empty() {
            patch.derive = Some(derives_to_add);
            patch.add_derive = Some(true); // Merge with existing derives
        }
        if !derives_to_remove.is_empty() {
            patch.remove_derive = Some(derives_to_remove);
        }
        if !struct_fields_to_add.is_empty() {
            patch.struct_fields = Some(vec![struct_fields_to_add]);
        }
        if !enum_variants_to_add.is_empty() {
            patch.enum_fields = Some(vec![enum_variants_to_add]);
        }
        
        patch
    }
    
    /// Load from a JSON file and convert to applicable format
    pub fn load_and_convert(path: &std::path::Path) -> anyhow::Result<ApiPatch> {
        let content = std::fs::read_to_string(path)?;
        let patch: AutofixPatch = serde_json::from_str(&content)?;
        Ok(patch.to_api_patch())
    }
}

fn insert_class_patch(
    api_patch: &mut ApiPatch,
    version: &str,
    module: &str,
    class_name: &str,
    class_patch: ClassPatch,
) {
    api_patch
        .versions
        .entry(version.to_string())
        .or_insert_with(VersionPatch::default)
        .modules
        .entry(module.to_string())
        .or_insert_with(ModulePatch::default)
        .classes
        .insert(class_name.to_string(), class_patch);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modify_patch_serialization() {
        let mut patch = AutofixPatch::new("Update AppTerminationBehavior");
        patch.add_operation(PatchOperation::Modify(ModifyOperation {
            type_name: "AppTerminationBehavior".to_string(),
            module: None,
            changes: vec![
                ModifyChange::SetReprC { old: false, new: true },
                ModifyChange::AddDerives {
                    derives: vec!["Clone".to_string(), "Copy".to_string(), "Debug".to_string()],
                },
            ],
        }));

        let json = patch.to_json().unwrap();
        println!("{}", json);
        
        let parsed = AutofixPatch::from_json(&json).unwrap();
        assert_eq!(parsed.operations.len(), 1);
    }

    #[test]
    fn test_path_fix_patch() {
        let mut patch = AutofixPatch::new("Fix CssPropertyVec path");
        patch.add_operation(PatchOperation::PathFix(PathFixOperation {
            type_name: "CssPropertyVec".to_string(),
            old_path: "azul_dll::widgets::text_input::CssPropertyVec".to_string(),
            new_path: "azul_css::props::property::CssPropertyVec".to_string(),
        }));

        let json = patch.to_json().unwrap();
        let parsed = AutofixPatch::from_json(&json).unwrap();
        assert_eq!(parsed.operations.len(), 1);
    }

    #[test]
    fn test_add_patch() {
        let mut patch = AutofixPatch::new("Add GridTrackSizing enum");
        patch.add_operation(PatchOperation::Add(AddOperation {
            type_name: "GridTrackSizing".to_string(),
            external: "azul_css::props::layout::grid::GridTrackSizing".to_string(),
            kind: TypeKind::Enum,
            module: Some("style".to_string()),
            derives: Some(vec!["Clone".to_string(), "Debug".to_string()]),
            repr_c: Some(true),
            struct_fields: None,
            enum_variants: Some(vec![
                VariantDef { name: "Auto".to_string(), variant_type: None },
                VariantDef { name: "Fixed".to_string(), variant_type: Some("PixelValue".to_string()) },
            ]),
        }));

        let json = patch.to_json().unwrap();
        println!("{}", json);
    }

    #[test]
    fn test_remove_patch() {
        let mut patch = AutofixPatch::new("Remove unused ImageCache");
        patch.add_operation(PatchOperation::Remove(RemoveOperation {
            type_name: "ImageCache".to_string(),
            path: Some("azul_core::resources::ImageCache".to_string()),
            reason: Some("Not reachable from public API (only via *const pointer)".to_string()),
        }));

        let json = patch.to_json().unwrap();
        println!("{}", json);
    }

    #[test]
    fn test_convert_to_api_patch() {
        let mut patch = AutofixPatch::new("Fix AppTerminationBehavior");
        patch.add_operation(PatchOperation::Modify(ModifyOperation {
            type_name: "AppTerminationBehavior".to_string(),
            module: Some("app".to_string()),
            changes: vec![
                ModifyChange::SetExternal {
                    old: "old::path::AppTerminationBehavior".to_string(),
                    new: "azul_core::window::AppTerminationBehavior".to_string(),
                },
                ModifyChange::SetReprC { old: false, new: true },
                ModifyChange::AddDerives {
                    derives: vec!["Clone".to_string(), "Copy".to_string()],
                },
            ],
        }));

        let api_patch = patch.to_api_patch();
        
        // Check structure - uses API_VERSION constant
        assert!(api_patch.versions.contains_key(super::API_VERSION));
        let version = api_patch.versions.get(super::API_VERSION).unwrap();
        assert!(version.modules.contains_key("app"));
        let module = version.modules.get("app").unwrap();
        assert!(module.classes.contains_key("AppTerminationBehavior"));
        
        let class_patch = module.classes.get("AppTerminationBehavior").unwrap();
        assert_eq!(class_patch.external, Some("azul_core::window::AppTerminationBehavior".to_string()));
        assert_eq!(class_patch.repr, Some("C".to_string()));
        assert_eq!(class_patch.derive, Some(vec!["Clone".to_string(), "Copy".to_string()]));
    }
}
