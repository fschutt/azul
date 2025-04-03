use indexmap::IndexMap; // Use IndexMap for ordered fields where necessary
use serde_derive::Deserialize;
use std::collections::BTreeMap; // Use BTreeMap for sorted keys (versions)

// Renaming fields to be idiomatic Rust (snake_case)
// Serde handles the mapping from potential camelCase/other cases in JSON
// if you need specific renames use #[serde(rename = "...")]

#[derive(Debug, Deserialize, Clone)]
pub struct ApiData(
    // BTreeMap ensures versions are sorted alphabetically/numerically by key.
    pub BTreeMap<String, VersionData>
);

impl ApiData {
    // Helper to get sorted version strings
    pub fn get_sorted_versions(&self) -> Vec<String> {
        self.0.keys().cloned().collect() // BTreeMap keys are already sorted
    }

    // Helper to get data for a specific version by its string name
    pub fn get_version(&self, version: &str) -> Option<&VersionData> {
        self.0.get(version)
    }

     // Helper to get the latest version string (assuming BTreeMap sorting works)
     pub fn get_latest_version_str(&self) -> Option<&str> {
         self.0.keys().last().map(|s| s.as_str())
     }

     // Search across all versions and modules for a class definition by name.
     // Returns Option<(version_str, module_name, class_name, &ClassData)>
     pub fn find_class_definition<'a>(&'a self, search_class_name: &str) -> Option<(&'a str, &'a str, &'a str, &'a ClassData)> {
        for (version_str, version_data) in &self.0 {
            if let Some((module_name, class_name, class_data)) = version_data.find_class(search_class_name) {
                return Some((version_str, module_name, class_name, class_data));
            }
        }
        None
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct VersionData {
    // Using IndexMap to preserve module order as read from JSON
    #[serde(flatten)] // Assumes modules are directly under the version key like "app": { ... }
    pub modules: IndexMap<String, ModuleData>,
    // Capture top-level doc if it exists for a version
    pub doc: Option<String>,
}

impl VersionData {
    // Find a class definition within this specific version.
    // Returns Option<(module_name, class_name, &ClassData)>
    pub fn find_class<'a>(&'a self, search_class_name: &str) -> Option<(&'a str, &'a str, &'a ClassData)> {
        for (module_name, module_data) in &self.modules {
            if let Some((class_name, class_data)) = module_data.find_class(search_class_name) {
                return Some((module_name.as_str(), class_name, class_data));
            }
        }
        None
    }

    // Get a specific class if module and class name are known for this version.
    pub fn get_class(&self, module_name: &str, class_name: &str) -> Option<&ClassData> {
        self.modules.get(module_name)?.classes.get(class_name)
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct ModuleData {
    pub doc: Option<String>,
    // Using IndexMap to preserve class order within a module
    pub classes: IndexMap<String, ClassData>,
}

impl ModuleData {
    // Find a class within this specific module.
    // Returns Option<(class_name, &ClassData)>
    pub fn find_class<'a>(&'a self, search_class_name: &str) -> Option<(&'a str, &'a ClassData)> {
        self.classes.iter()
            .find(|(name, _)| *name == search_class_name)
            .map(|(name, data)| (name.as_str(), data))
    }
}


#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")] // Handles fields like isBoxedObject -> is_boxed_object
pub struct ClassData {
    pub doc: Option<String>,
    pub external: Option<String>,
    #[serde(default)] // Assumes false if missing
    pub is_boxed_object: bool,
    pub clone: Option<bool>, // If missing, generation logic should assume true
    pub custom_destructor: Option<bool>,
    #[serde(default)]
    pub derive: Option<Vec<String>>,
    pub serde: Option<String>, // Serde attributes like "transparent"
    // Renamed from "const" which is a keyword
    #[serde(rename = "const")]
    pub const_value_type: Option<String>,
    #[serde(default)]
    pub constants: Option<Vec<IndexMap<String, ConstantData>>>, // Use IndexMap if field order matters
    #[serde(default)]
    pub struct_fields: Option<Vec<IndexMap<String, FieldData>>>,
    #[serde(default)]
    pub enum_fields: Option<Vec<IndexMap<String, EnumVariantData>>>,
    #[serde(default)]
    pub callback_typedef: Option<CallbackDefinition>,
    #[serde(default)]
    // Using IndexMap to preserve function/constructor order
    pub constructors: Option<IndexMap<String, FunctionData>>,
    #[serde(default)]
    pub functions: Option<IndexMap<String, FunctionData>>,
    #[serde(default)]
    pub use_patches: Option<Vec<String>>, // For conditional patch application
    pub repr: Option<String>, // For things like #[repr(transparent)]
}


#[derive(Debug, Deserialize, Clone)]
pub struct ConstantData {
    pub r#type: String, // r# to allow "type" as field name
    pub value: String, // Keep value as string, parsing depends on type context
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct FieldData {
    pub r#type: String,
    pub doc: Option<String>,
    #[serde(default)]
    pub derive: Option<Vec<String>>, // For field-level derives like #[pyo3(get, set)]
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct EnumVariantData {
    // Variants might not have an associated type (e.g., simple enums like MsgBoxIcon)
    pub r#type: Option<String>,
    pub doc: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")] // For fnArgs
pub struct FunctionData {
    pub doc: Option<String>,
    // Arguments are a list where each item is a map like {"arg_name": "type"}
    // Using IndexMap here preserves argument order.
    #[serde(default)]
    pub fn_args: Vec<IndexMap<String, String>>,
    pub returns: Option<ReturnTypeData>,
    pub fn_body: Option<String>, // Present in api.json for DLL generation
    #[serde(default)]
    pub use_patches: Option<Vec<String>>, // Which languages this patch applies to
}

#[derive(Debug, Deserialize, Clone)]
pub struct ReturnTypeData {
    pub r#type: String,
    pub doc: Option<String>,
}


#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct CallbackDefinition {
     #[serde(default)]
    pub fn_args: Vec<CallbackArgData>,
    pub returns: Option<ReturnTypeData>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CallbackArgData {
    pub r#type: String,
    // Renamed from "ref" which is a keyword
    #[serde(rename = "ref")]
    pub ref_kind: String, // "ref", "refmut", "value"
    pub doc: Option<String>,
}