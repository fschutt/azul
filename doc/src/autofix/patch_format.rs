//! JSON Patch Format for api.json modifications
//!
//! This module defines a structured patch format that is:
//! - Human-readable (good for review)
//! - Machine-parseable (for automatic application)
//! - Self-documenting (includes context about what changes)

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Convert PascalCase to snake_case
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

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
    SetExternal { old: String, new: String },
    /// Set repr attribute (e.g., "C", "C, u8", None)
    SetRepr {
        old: Option<String>,
        new: Option<String>,
    },
    /// Add derive attributes
    AddDerives { derives: Vec<String> },
    /// Remove derive attributes
    RemoveDerives { derives: Vec<String> },
    /// Add custom impl traits (manual impl blocks)
    AddCustomImpls { impls: Vec<String> },
    /// Remove custom impl traits
    RemoveCustomImpls { impls: Vec<String> },
    /// Add a struct field
    AddField {
        name: String,
        #[serde(rename = "type")]
        field_type: String,
        /// Reference kind: "constptr", "mutptr", etc. Defaults to "value" if not present.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ref_kind: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        doc: Option<String>,
    },
    /// Remove a struct field
    RemoveField { name: String },
    /// Change a field's type
    ChangeFieldType {
        name: String,
        old_type: String,
        new_type: String,
        /// Reference kind: "constptr", "mutptr", etc. Defaults to "value" if not present.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ref_kind: Option<String>,
    },
    /// Add an enum variant
    AddVariant {
        name: String,
        #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
        variant_type: Option<String>,
    },
    /// Remove an enum variant
    RemoveVariant { name: String },
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        old_ref: Option<crate::api::RefKind>,
        new_ref: crate::api::RefKind,
    },
    /// Change callback return type
    ChangeCallbackReturn {
        old_type: Option<String>,
        new_type: Option<String>,
    },
    /// Set type_alias (add the entire type alias definition)
    SetTypeAlias {
        target: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        generic_args: Vec<String>,
    },
    /// Change type alias target
    ChangeTypeAlias {
        old_target: String,
        new_target: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        new_generic_args: Vec<String>,
    },
    /// Set generic params for a generic type (e.g., ["T"] for PhysicalSize<T>)
    SetGenericParams {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        old_params: Vec<String>,
        new_params: Vec<String>,
    },
    /// Replace ALL struct fields (preserves correct field order for repr(C) structs)
    ReplaceStructFields { fields: Vec<StructFieldDef> },
    /// Replace ALL enum variants (preserves correct variant order)
    ReplaceEnumVariants { variants: Vec<EnumVariantDef> },
    /// Remove struct_fields entirely (type changed from struct to enum)
    RemoveStructFields,
    /// Remove enum_fields entirely (type changed from enum to struct)
    RemoveEnumFields,
    /// Fix function self parameter
    FixFunctionSelf {
        fn_name: String,
        /// Expected self kind from source: "ref", "refmut", "value", or null for static
        expected_self: Option<String>,
    },
    /// Fix function argument count (regenerate fn_args from source)
    FixFunctionArgs {
        fn_name: String,
        /// Expected number of arguments (excluding self)
        expected_count: usize,
    },
    /// Add missing Vec functions (generated by impl_vec! macro)
    AddVecFunctions {
        /// List of function names to add (e.g., ["create", "len", "is_empty"])
        missing_functions: Vec<String>,
        /// Element type of the Vec (e.g., "SvgPath" for SvgPathVec)
        element_type: String,
    },
    /// Add a dependency type that a Vec needs (OptionX or XVecSlice)
    /// This will generate an AddStruct patch to add the type to api.json
    AddDependencyType {
        /// The type name to add (e.g., "OptionMenuItem" or "MenuItemVecSlice")
        dependency_type: String,
        /// The kind of dependency: "option" or "slice"
        dependency_kind: String,
        /// The element type this depends on (e.g., "MenuItem")
        element_type: String,
    },
}

/// Struct field definition for complete field replacement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructFieldDef {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_kind: Option<String>,
}

/// Enum variant definition for complete variant replacement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumVariantDef {
    pub name: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub variant_type: Option<String>,
}

/// Callback argument definition for patches
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackArgDef {
    #[serde(rename = "type")]
    pub arg_type: String,
    /// Reference kind - uses RefKind directly for type safety
    #[serde(default, skip_serializing_if = "is_ref_kind_default")]
    pub ref_kind: crate::api::RefKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

fn is_ref_kind_default(rk: &crate::api::RefKind) -> bool {
    rk.is_default()
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
    /// Callback typedef definition (for callback function pointer types)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callback_typedef: Option<CallbackTypedefDef>,
    /// Type alias info (for type aliases like "type HwndHandle = *mut c_void")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_alias: Option<TypeAliasDef>,
}

/// Definition for a type alias in add patches
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeAliasDef {
    /// The base target type (e.g., "c_void")
    pub target: String,
    /// Reference kind (constptr, mutptr, value)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_kind: Option<String>,
}

/// Definition for a callback typedef (function pointer type)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackTypedefDef {
    /// Arguments to the callback function
    #[serde(default)]
    pub fn_args: Vec<CallbackArg>,
    /// Return type (None = void)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returns: Option<CallbackReturn>,
}

/// Argument definition for callback typedef (simple version for add patches)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackArg {
    /// The type of the argument
    #[serde(rename = "type")]
    pub arg_type: String,
    /// Reference kind (value, constptr, mutptr)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_kind: Option<String>,
}

/// Return type definition for callback typedef
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackReturn {
    /// The return type
    #[serde(rename = "type")]
    pub return_type: String,
    /// Reference kind (value, constptr, mutptr)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_kind: Option<String>,
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
    CallbackTypedef,
}

/// Field definition for struct types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDef {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    /// The reference kind (value, constptr, mutptr, boxed)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_kind: Option<String>,
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
        ModifyChange::SetRepr { old, new } => {
            let old_str = old.as_deref().unwrap_or("none");
            let new_str = new.as_deref().unwrap_or("none");
            format!("repr: {} → {}", old_str, new_str)
        }
        ModifyChange::AddDerives { derives } => {
            format!("+ derive: {}", derives.join(", "))
        }
        ModifyChange::RemoveDerives { derives } => {
            format!("- derive: {}", derives.join(", "))
        }
        ModifyChange::AddCustomImpls { impls } => {
            format!("+ custom_impls: {}", impls.join(", "))
        }
        ModifyChange::RemoveCustomImpls { impls } => {
            format!("- custom_impls: {}", impls.join(", "))
        }
        ModifyChange::AddField {
            name,
            field_type,
            ref_kind,
            ..
        } => {
            if let Some(rk) = ref_kind {
                format!("+ field {} : {} ({})", name, field_type, rk)
            } else {
                format!("+ field {} : {}", name, field_type)
            }
        }
        ModifyChange::RemoveField { name } => {
            format!("- field {}", name)
        }
        ModifyChange::ChangeFieldType {
            name,
            old_type,
            new_type,
            ref_kind,
        } => {
            if let Some(rk) = ref_kind {
                format!("~ field {}: {} → {} ({})", name, old_type, new_type, rk)
            } else {
                format!("~ field {}: {} → {}", name, old_type, new_type)
            }
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
        ModifyChange::ChangeVariantType {
            name,
            old_type,
            new_type,
        } => {
            format!("~ variant {}: {:?} → {:?}", name, old_type, new_type)
        }
        ModifyChange::SetCallbackTypedef { args, returns } => {
            let args_str: Vec<String> = args
                .iter()
                .map(|a| format!("{}{}", a.ref_kind.as_prefix(), a.arg_type))
                .collect();
            format!(
                "+ callback_typedef({}) -> {:?}",
                args_str.join(", "),
                returns
            )
        }
        ModifyChange::ChangeCallbackArg {
            arg_index,
            old_type,
            new_type,
            old_ref,
            new_ref,
        } => {
            let old_ref_str = old_ref.as_ref().map(|rk| rk.as_prefix()).unwrap_or("");
            let new_ref_str = new_ref.as_prefix();
            format!(
                "~ callback arg[{}]: {}{} → {}{}",
                arg_index, old_ref_str, old_type, new_ref_str, new_type
            )
        }
        ModifyChange::ChangeCallbackReturn { old_type, new_type } => {
            format!("~ callback return: {:?} → {:?}", old_type, new_type)
        }
        ModifyChange::SetTypeAlias {
            target,
            generic_args,
        } => {
            if generic_args.is_empty() {
                format!("+ type_alias = {}", target)
            } else {
                format!("+ type_alias = {}<{}>", target, generic_args.join(", "))
            }
        }
        ModifyChange::ChangeTypeAlias {
            old_target,
            new_target,
            new_generic_args,
        } => {
            if new_generic_args.is_empty() {
                format!("~ type_alias: {} → {}", old_target, new_target)
            } else {
                format!(
                    "~ type_alias: {} → {}<{}>",
                    old_target,
                    new_target,
                    new_generic_args.join(", ")
                )
            }
        }
        ModifyChange::SetGenericParams {
            old_params,
            new_params,
        } => {
            let old_display = if old_params.is_empty() {
                "none".to_string()
            } else {
                format!("<{}>", old_params.join(", "))
            };
            let new_display = if new_params.is_empty() {
                "none".to_string()
            } else {
                format!("<{}>", new_params.join(", "))
            };
            format!("~ generic_params: {} → {}", old_display, new_display)
        }
        ModifyChange::ReplaceStructFields { fields } => {
            let field_names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
            format!("= struct_fields [{}]", field_names.join(", "))
        }
        ModifyChange::ReplaceEnumVariants { variants } => {
            let variant_names: Vec<&str> = variants.iter().map(|v| v.name.as_str()).collect();
            format!("= enum_variants [{}]", variant_names.join(", "))
        }
        ModifyChange::RemoveStructFields => "- struct_fields (type changed to enum)".to_string(),
        ModifyChange::RemoveEnumFields => "- enum_fields (type changed to struct)".to_string(),
        ModifyChange::FixFunctionSelf {
            fn_name,
            expected_self,
        } => {
            let self_str = expected_self.as_deref().unwrap_or("static");
            format!("~ fn {}: set self = {}", fn_name, self_str)
        }
        ModifyChange::FixFunctionArgs {
            fn_name,
            expected_count,
        } => {
            format!("~ fn {}: fix arg count to {}", fn_name, expected_count)
        }
        ModifyChange::AddVecFunctions {
            missing_functions,
            element_type,
        } => {
            format!(
                "+ vec functions for Vec<{}>: [{}]",
                element_type,
                missing_functions.join(", ")
            )
        }
        ModifyChange::AddDependencyType {
            dependency_type,
            dependency_kind,
            element_type,
        } => {
            format!(
                "+ {} type: {} (for element {})",
                dependency_kind,
                dependency_type,
                element_type
            )
        }
    }
}

// // conversion to legacy apipatch format for application
//
use indexmap::IndexMap;

use crate::{
    api::{EnumVariantData, FieldData},
    autofix::module_map::determine_module,
    patch::{ApiPatch, ClassPatch, ModulePatch, VersionPatch},
};

/// Current API version - should match what's in api.json
pub const API_VERSION: &str = "1.0.0-alpha1";

impl AutofixPatch {
    /// Convert this patch to the legacy ApiPatch format that can be applied
    ///
    /// Uses the current API version and determines the module from the type name.
    pub fn to_api_patch(&self) -> ApiPatch {
        let mut api_patch = ApiPatch::default();
        
        // First pass: collect AddDependencyType changes and generate AddStruct patches for them
        for op in &self.operations {
            if let PatchOperation::Modify(m) = op {
                for change in &m.changes {
                    if let ModifyChange::AddDependencyType {
                        dependency_type,
                        dependency_kind,
                        element_type,
                    } = change {
                        // Generate the appropriate type (Option or Slice)
                        let class_patch = generate_dependency_type_patch(
                            dependency_type,
                            dependency_kind,
                            element_type,
                        );
                        
                        // Determine the module for the new type
                        let module_name = if dependency_kind == "option" {
                            "option".to_string()
                        } else {
                            // Slice types go in the "vec" module
                            "vec".to_string()
                        };
                        
                        insert_class_patch(
                            &mut api_patch,
                            API_VERSION,
                            &module_name,
                            dependency_type,
                            class_patch,
                        );
                    }
                }
            }
        }

        // Second pass: process all other operations
        for op in &self.operations {
            match op {
                PatchOperation::Modify(m) => {
                    let class_patch = self.modify_to_class_patch(m);
                    // Use explicit module or determine from type name
                    let module_name = m.module.clone().unwrap_or_else(|| {
                        let (module, warn) = determine_module(&m.type_name);
                        if warn {
                            eprintln!(
                                "Warning: Could not determine module for '{}', using 'misc'",
                                m.type_name
                            );
                        }
                        module
                    });
                    insert_class_patch(
                        &mut api_patch,
                        API_VERSION,
                        &module_name,
                        &m.type_name,
                        class_patch,
                    );
                }
                PatchOperation::PathFix(p) => {
                    let class_patch = ClassPatch {
                        external: Some(p.new_path.clone()),
                        ..Default::default()
                    };
                    let (module_name, warn) = determine_module(&p.type_name);
                    if warn {
                        eprintln!(
                            "Warning: Could not determine module for '{}', using 'misc'",
                            p.type_name
                        );
                    }
                    insert_class_patch(
                        &mut api_patch,
                        API_VERSION,
                        &module_name,
                        &p.type_name,
                        class_patch,
                    );
                }
                PatchOperation::Add(a) => {
                    let class_patch = ClassPatch {
                        external: Some(a.external.clone()),
                        derive: a.derives.clone(),
                        repr: a.repr_c.map(|b| {
                            if b {
                                "C".to_string()
                            } else {
                                "Rust".to_string()
                            }
                        }),
                        struct_fields: a.struct_fields.as_ref().map(|fields| {
                            vec![fields
                                .iter()
                                .map(|f| {
                                    let ref_kind = f
                                        .ref_kind
                                        .as_ref()
                                        .and_then(|s| crate::api::RefKind::parse(s))
                                        .unwrap_or_default();
                                    (
                                        f.name.clone(),
                                        FieldData {
                                            r#type: f.field_type.clone(),
                                            ref_kind,
                                            arraysize: None,
                                            doc: f.doc.clone().map(|s| vec![s]),
                                            derive: None,
                                        },
                                    )
                                })
                                .collect()]
                        }),
                        enum_fields: a.enum_variants.as_ref().map(|variants| {
                            vec![variants
                                .iter()
                                .map(|v| {
                                    (
                                        v.name.clone(),
                                        EnumVariantData {
                                            r#type: v.variant_type.clone(),
                                            doc: None,
                                            ref_kind: Default::default(),
                                        },
                                    )
                                })
                                .collect()]
                        }),
                        type_alias: a.type_alias.as_ref().map(|type_alias_def| {
                            let ref_kind = type_alias_def
                                .ref_kind
                                .as_ref()
                                .and_then(|s| crate::api::RefKind::parse(s))
                                .unwrap_or_default();
                            crate::api::TypeAliasInfo {
                                target: type_alias_def.target.clone(),
                                ref_kind,
                                generic_args: Vec::new(),
                            }
                        }),
                        callback_typedef: a.callback_typedef.as_ref().map(|cb| {
                            crate::api::CallbackDefinition {
                                fn_args: cb
                                    .fn_args
                                    .iter()
                                    .map(|arg| {
                                        let ref_kind = arg
                                            .ref_kind
                                            .as_ref()
                                            .and_then(|s| crate::api::RefKind::parse(s))
                                            .unwrap_or_default();
                                        crate::api::CallbackArgData {
                                            r#type: arg.arg_type.clone(),
                                            ref_kind,
                                            doc: None,
                                        }
                                    })
                                    .collect(),
                                returns: cb.returns.as_ref().map(|r| crate::api::ReturnTypeData {
                                    r#type: r.return_type.clone(),
                                    doc: None,
                                }),
                            }
                        }),
                        ..Default::default()
                    };
                    let module_name = a.module.clone().unwrap_or_else(|| {
                        let (module, warn) = determine_module(&a.type_name);
                        if warn {
                            eprintln!(
                                "Warning: Could not determine module for '{}', using 'misc'",
                                a.type_name
                            );
                        }
                        module
                    });
                    insert_class_patch(
                        &mut api_patch,
                        API_VERSION,
                        &module_name,
                        &a.type_name,
                        class_patch,
                    );
                }
                PatchOperation::Remove(r) => {
                    let class_patch = ClassPatch {
                        remove: Some(true),
                        ..Default::default()
                    };
                    let (module_name, warn) = determine_module(&r.type_name);
                    if warn {
                        eprintln!(
                            "Warning: Could not determine module for '{}', using 'misc'",
                            r.type_name
                        );
                    }
                    insert_class_patch(
                        &mut api_patch,
                        API_VERSION,
                        &module_name,
                        &r.type_name,
                        class_patch,
                    );
                }
                PatchOperation::MoveModule(m) => {
                    // Create a patch in the source module that moves to target
                    let class_patch = ClassPatch {
                        move_to_module: Some(m.to_module.clone()),
                        ..Default::default()
                    };
                    insert_class_patch(
                        &mut api_patch,
                        API_VERSION,
                        &m.from_module,
                        &m.type_name,
                        class_patch,
                    );
                }
            }
        }

        api_patch
    }

    fn modify_to_class_patch(&self, m: &ModifyOperation) -> ClassPatch {
        let mut patch = ClassPatch::default();

        let mut derives_to_add = Vec::new();
        let mut derives_to_remove = Vec::new();
        let mut custom_impls_to_add = Vec::new();
        let mut custom_impls_to_remove = Vec::new();
        let mut struct_fields_to_add: IndexMap<String, FieldData> = IndexMap::new();
        let mut enum_variants_to_add: IndexMap<String, EnumVariantData> = IndexMap::new();
        let mut functions_to_add: IndexMap<String, crate::api::FunctionData> = IndexMap::new();

        for change in &m.changes {
            match change {
                ModifyChange::SetExternal { new, .. } => {
                    patch.external = Some(new.clone());
                }
                ModifyChange::SetRepr { new, .. } => {
                    patch.repr = new.clone();
                }
                ModifyChange::AddDerives { derives } => {
                    derives_to_add.extend(derives.clone());
                }
                ModifyChange::RemoveDerives { derives } => {
                    derives_to_remove.extend(derives.clone());
                }
                ModifyChange::AddCustomImpls { impls } => {
                    custom_impls_to_add.extend(impls.clone());
                }
                ModifyChange::RemoveCustomImpls { impls } => {
                    custom_impls_to_remove.extend(impls.clone());
                }
                ModifyChange::AddField {
                    name,
                    field_type,
                    ref_kind,
                    doc,
                } => {
                    let rk = ref_kind
                        .as_ref()
                        .and_then(|s| crate::api::RefKind::parse(s))
                        .unwrap_or_default();
                    struct_fields_to_add.insert(
                        name.clone(),
                        FieldData {
                            r#type: field_type.clone(),
                            ref_kind: rk,
                            arraysize: None,
                            doc: doc.clone().map(|s| vec![s]),
                            derive: None,
                        },
                    );
                }
                ModifyChange::RemoveField { .. } => {
                    // Note: The legacy format doesn't support removing fields directly
                }
                ModifyChange::ChangeFieldType {
                    name,
                    new_type,
                    ref_kind,
                    ..
                } => {
                    let rk = ref_kind
                        .as_ref()
                        .and_then(|s| crate::api::RefKind::parse(s))
                        .unwrap_or_default();
                    struct_fields_to_add.insert(
                        name.clone(),
                        FieldData {
                            r#type: new_type.clone(),
                            ref_kind: rk,
                            arraysize: None,
                            doc: None,
                            derive: None,
                        },
                    );
                }
                ModifyChange::AddVariant { name, variant_type } => {
                    enum_variants_to_add.insert(
                        name.clone(),
                        EnumVariantData {
                            r#type: variant_type.clone(),
                            doc: None,
                            ref_kind: Default::default(),
                        },
                    );
                }
                ModifyChange::RemoveVariant { .. } => {
                    // Note: The legacy format doesn't support removing variants directly
                }
                ModifyChange::ChangeVariantType { name, new_type, .. } => {
                    enum_variants_to_add.insert(
                        name.clone(),
                        EnumVariantData {
                            r#type: new_type.clone(),
                            doc: None,
                            ref_kind: Default::default(),
                        },
                    );
                }
                ModifyChange::SetCallbackTypedef { args, returns } => {
                    use crate::api::{CallbackArgData, CallbackDefinition, ReturnTypeData};

                    let callback_args: Vec<CallbackArgData> = args
                        .iter()
                        .map(|arg| CallbackArgData {
                            r#type: arg.arg_type.clone(),
                            ref_kind: arg.ref_kind.clone(),
                            doc: None,
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
                ModifyChange::ChangeCallbackArg {
                    arg_index,
                    new_type,
                    new_ref,
                    ..
                } => {
                    // Update a specific callback argument
                    use crate::api::CallbackDefinition;

                    // We need to update just this argument, but for now we'll need the full
                    // callback This could be improved with more granular
                    // updates
                    if let Some(ref mut callback_def) = patch.callback_typedef {
                        if let Some(arg) = callback_def.fn_args.get_mut(*arg_index) {
                            arg.r#type = new_type.clone();
                            arg.ref_kind = new_ref.clone();
                        }
                    }
                }
                ModifyChange::ChangeCallbackReturn { .. } => {
                    // For now, the entire callback_typedef would need to be re-set
                }
                ModifyChange::SetTypeAlias {
                    target,
                    generic_args,
                } => {
                    use crate::api::TypeAliasInfo;
                    // Parse pointer prefixes from target string
                    let (base_target, ref_kind) = parse_pointer_from_type(target);
                    patch.type_alias = Some(TypeAliasInfo {
                        target: base_target,
                        ref_kind,
                        generic_args: generic_args.clone(),
                    });
                }
                ModifyChange::ChangeTypeAlias {
                    new_target,
                    new_generic_args,
                    ..
                } => {
                    use crate::api::TypeAliasInfo;
                    // Parse pointer prefixes from target string
                    let (base_target, ref_kind) = parse_pointer_from_type(new_target);
                    patch.type_alias = Some(TypeAliasInfo {
                        target: base_target,
                        ref_kind,
                        generic_args: new_generic_args.clone(),
                    });
                }
                ModifyChange::SetGenericParams { new_params, .. } => {
                    if new_params.is_empty() {
                        patch.generic_params = None;
                    } else {
                        patch.generic_params = Some(new_params.clone());
                    }
                }
                ModifyChange::ReplaceStructFields { fields } => {
                    // Complete replacement - NOT a merge, preserves field order
                    let mut ordered_fields: IndexMap<String, FieldData> = IndexMap::new();
                    for field in fields {
                        let rk = field
                            .ref_kind
                            .as_ref()
                            .and_then(|s| crate::api::RefKind::parse(s))
                            .unwrap_or_default();
                        ordered_fields.insert(
                            field.name.clone(),
                            FieldData {
                                r#type: field.field_type.clone(),
                                ref_kind: rk,
                                arraysize: None,
                                doc: None,
                                derive: None,
                            },
                        );
                    }
                    patch.struct_fields = Some(vec![ordered_fields]);
                    patch.add_struct_fields = Some(false); // REPLACE, not merge
                }
                ModifyChange::ReplaceEnumVariants { variants } => {
                    // Complete replacement - NOT a merge, preserves variant order
                    let mut ordered_variants: IndexMap<String, EnumVariantData> = IndexMap::new();
                    for variant in variants {
                        ordered_variants.insert(
                            variant.name.clone(),
                            EnumVariantData {
                                r#type: variant.variant_type.clone(),
                                doc: None,
                                ref_kind: Default::default(),
                            },
                        );
                    }
                    patch.enum_fields = Some(vec![ordered_variants]);
                    patch.add_enum_fields = Some(false); // REPLACE, not merge
                }
                ModifyChange::RemoveStructFields => {
                    // Type changed from struct to enum - clear struct_fields
                    patch.struct_fields = Some(vec![]); // Empty vec signals removal
                    patch.add_struct_fields = Some(false);
                }
                ModifyChange::RemoveEnumFields => {
                    // Type changed from enum to struct - clear enum_fields
                    patch.enum_fields = Some(vec![]); // Empty vec signals removal
                    patch.add_enum_fields = Some(false);
                }
                ModifyChange::FixFunctionSelf {
                    fn_name,
                    expected_self,
                } => {
                    // Function self parameter mismatch detected
                    // We can't easily fix this here because we need to preserve other arguments
                    // The user should run: autofix add 'TypeName.fn_name' to regenerate the
                    // function
                    eprintln!(
                        "  [WARN] {}.{}: needs self='{}' - run 'autofix add \"{}.{}\"' to fix",
                        m.type_name,
                        fn_name,
                        expected_self.as_deref().unwrap_or("static"),
                        m.type_name,
                        fn_name
                    );
                }
                ModifyChange::FixFunctionArgs {
                    fn_name,
                    expected_count,
                } => {
                    // Function argument count mismatch detected
                    // The user should run: autofix add 'TypeName.fn_name' to regenerate the
                    // function
                    eprintln!(
                        "  [WARN] {}.{}: needs {} args - run 'autofix add \"{}.{}\"' to fix",
                        m.type_name, fn_name, expected_count, m.type_name, fn_name
                    );
                }
                ModifyChange::AddVecFunctions {
                    missing_functions,
                    element_type,
                } => {
                    // Vec type is missing standard impl_vec! functions
                    // Generate the functions and add them to the patch
                    // Note: known_types is None here, so all functions will be generated
                    // The codegen stage will handle missing types appropriately
                    let lowercase_type_name = to_snake_case(&m.type_name);
                    let all_vec_functions = super::workspace::generate_vec_functions(
                        &m.type_name,
                        element_type,
                        &lowercase_type_name,
                        None, // No type checking at patch generation stage
                    );
                    // Only add the missing functions
                    for fn_name in missing_functions {
                        if let Some(fn_data) = all_vec_functions.get(fn_name) {
                            functions_to_add.insert(fn_name.clone(), fn_data.clone());
                        }
                    }
                }
                ModifyChange::AddDependencyType {
                    dependency_type,
                    dependency_kind,
                    element_type,
                } => {
                    // Need to add a dependency type (OptionX or XVecSlice) to api.json
                    // This generates an AddStruct patch to the appropriate module
                    //
                    // For now, we just log a warning. The actual type addition will be done
                    // in a separate pass that generates AddStruct patches.
                    // This is because ModifyChange operates on an existing type,
                    // but AddDependencyType needs to create a new type.
                    eprintln!(
                        "  [INFO] Vec {} needs {} type '{}' for element '{}' - will be generated",
                        m.type_name, dependency_kind, dependency_type, element_type
                    );
                    // Store the dependency for later processing
                    // The actual type generation happens in generate_dependency_type_patches()
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
        if !custom_impls_to_add.is_empty() {
            patch.custom_impls = Some(custom_impls_to_add);
            patch.add_custom_impls = Some(true); // Merge with existing custom_impls
        }
        if !custom_impls_to_remove.is_empty() {
            patch.remove_custom_impls = Some(custom_impls_to_remove);
        }
        if !struct_fields_to_add.is_empty() {
            patch.struct_fields = Some(vec![struct_fields_to_add]);
            patch.add_struct_fields = Some(true); // Merge with existing struct_fields
        }
        if !enum_variants_to_add.is_empty() {
            patch.enum_fields = Some(vec![enum_variants_to_add]);
            patch.add_enum_fields = Some(true); // Merge with existing enum_fields
        }
        if !functions_to_add.is_empty() {
            patch.functions = Some(functions_to_add);
            patch.add_functions = Some(true); // Merge with existing functions
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

/// Parse pointer prefixes from a type string (e.g., "*mut c_void" -> ("c_void", MutPtr))
fn parse_pointer_from_type(target: &str) -> (String, crate::api::RefKind) {
    let trimmed = target.trim();
    if let Some(rest) = trimmed.strip_prefix("*mut ") {
        (rest.trim().to_string(), crate::api::RefKind::MutPtr)
    } else if let Some(rest) = trimmed.strip_prefix("*const ") {
        (rest.trim().to_string(), crate::api::RefKind::ConstPtr)
    } else if let Some(rest) = trimmed.strip_prefix("&mut ") {
        (rest.trim().to_string(), crate::api::RefKind::RefMut)
    } else if let Some(rest) = trimmed.strip_prefix('&') {
        (rest.trim().to_string(), crate::api::RefKind::Ref)
    } else {
        (trimmed.to_string(), crate::api::RefKind::Value)
    }
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
                ModifyChange::SetRepr {
                    old: None,
                    new: Some("C".to_string()),
                },
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
                VariantDef {
                    name: "Auto".to_string(),
                    variant_type: None,
                },
                VariantDef {
                    name: "Fixed".to_string(),
                    variant_type: Some("PixelValue".to_string()),
                },
            ]),
            callback_typedef: None,
            type_alias: None,
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
                ModifyChange::SetRepr {
                    old: None,
                    new: Some("C".to_string()),
                },
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
        assert_eq!(
            class_patch.external,
            Some("azul_core::window::AppTerminationBehavior".to_string())
        );
        assert_eq!(class_patch.repr, Some("C".to_string()));
        assert_eq!(
            class_patch.derive,
            Some(vec!["Clone".to_string(), "Copy".to_string()])
        );
    }
}

/// Generate a ClassPatch for a dependency type (OptionX or XVecSlice)
///
/// # Arguments
/// * `dependency_type` - The type name to generate (e.g., "OptionMenuItem" or "MenuItemVecSlice")
/// * `dependency_kind` - Either "option" or "slice"
/// * `element_type` - The element type this depends on (e.g., "MenuItem")
///
/// # Returns
/// A ClassPatch that can be applied to add the type to api.json
fn generate_dependency_type_patch(
    dependency_type: &str,
    dependency_kind: &str,
    element_type: &str,
) -> ClassPatch {
    use crate::api::{FieldData, RefKind};
    
    match dependency_kind {
        "option" => {
            // Generate Option type: enum with None and Some(element_type) variants
            // External path points to where the type is defined via impl_option! macro
            // We need to determine the external path from the element type
            let external_path = format!("azul_core::option::Option{}", element_type);
            
            let mut enum_variants = IndexMap::new();
            enum_variants.insert(
                "None".to_string(),
                EnumVariantData {
                    r#type: None,
                    doc: Some(vec!["No value".to_string()]),
                    ref_kind: Default::default(),
                },
            );
            enum_variants.insert(
                "Some".to_string(),
                EnumVariantData {
                    r#type: Some(element_type.to_string()),
                    doc: Some(vec![format!("Some value of type {}", element_type)]),
                    ref_kind: Default::default(),
                },
            );
            
            ClassPatch {
                external: Some(external_path),
                repr: Some("C, u8".to_string()),
                derive: Some(vec![
                    "Debug".to_string(),
                    "Clone".to_string(),
                ]),
                enum_fields: Some(vec![enum_variants]),
                ..Default::default()
            }
        }
        "slice" => {
            // Generate Slice type: struct with ptr and len fields
            // External path points to where the type is defined via impl_vec! macro
            let vec_type_name = dependency_type.trim_end_matches("Slice");
            let external_path = format!("azul_css::{}", dependency_type);
            
            let mut struct_fields = IndexMap::new();
            struct_fields.insert(
                "ptr".to_string(),
                FieldData {
                    r#type: element_type.to_string(),
                    ref_kind: RefKind::ConstPtr,
                    arraysize: None,
                    doc: Some(vec!["Pointer to the slice data".to_string()]),
                    derive: None,
                },
            );
            struct_fields.insert(
                "len".to_string(),
                FieldData {
                    r#type: "usize".to_string(),
                    ref_kind: RefKind::Value,
                    arraysize: None,
                    doc: Some(vec!["Number of elements in the slice".to_string()]),
                    derive: None,
                },
            );
            
            ClassPatch {
                external: Some(external_path),
                repr: Some("C".to_string()),
                derive: Some(vec![
                    "Debug".to_string(),
                    "Clone".to_string(),
                    "Copy".to_string(),
                ]),
                struct_fields: Some(vec![struct_fields]),
                ..Default::default()
            }
        }
        _ => {
            eprintln!("Warning: Unknown dependency kind '{}' for type '{}'", dependency_kind, dependency_type);
            ClassPatch::default()
        }
    }
}
