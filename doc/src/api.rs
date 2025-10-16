use std::{collections::BTreeMap, path::Path};

use anyhow::Context;
use indexmap::IndexMap; // Use IndexMap for ordered fields where necessary
use serde_derive::{Deserialize, Serialize}; // Use BTreeMap for sorted keys (versions)

// Helper function to check if a bool is false (for skip_serializing_if)
fn is_false(b: &bool) -> bool {
    !b
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiData(
    // BTreeMap ensures versions are sorted alphabetically/numerically by key.
    pub BTreeMap<String, VersionData>,
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

    /// Get versions sorted by date (oldest first)
    /// Returns list of (version_name, version_index) where index is 1-based
    /// Oldest version gets index 1 (Az1), second oldest gets index 2 (Az2), etc.
    pub fn get_versions_by_date(&self) -> Vec<(String, usize)> {
        let mut versions: Vec<_> = self.0.iter().collect();

        // Sort by date (oldest first)
        versions.sort_by(|a, b| a.1.date.cmp(&b.1.date));

        // Return with 1-based index
        versions
            .into_iter()
            .enumerate()
            .map(|(idx, (name, _))| (name.clone(), idx + 1))
            .collect()
    }

    /// Get the prefix for a specific version (e.g., "Az1", "Az2", etc.)
    /// Based on the version's position when sorted by date
    pub fn get_version_prefix(&self, version_name: &str) -> Option<String> {
        self.get_versions_by_date()
            .into_iter()
            .find(|(name, _)| name == version_name)
            .map(|(_, idx)| format!("Az{}", idx))
    }

    /// Get the default prefix (uses "Az" without version number)
    pub fn get_default_prefix() -> &'static str {
        "Az"
    }

    // Search across all versions and modules for a class definition by name.
    // Returns Option<(version_str, module_name, class_name, &ClassData)>
    pub fn find_class_definition<'a>(
        &'a self,
        search_class_name: &str,
    ) -> Option<(&'a str, &'a str, &'a str, &'a ClassData)> {
        for (version_str, version_data) in &self.0 {
            if let Some((module_name, class_name, class_data)) =
                version_data.find_class(search_class_name)
            {
                return Some((version_str, module_name, class_name, class_data));
            }
        }
        None
    }

    /// Create ApiData from a JSON string
    pub fn from_str(json_str: &str) -> anyhow::Result<Self> {
        serde_json::from_str(json_str).map_err(|e| anyhow::anyhow!("JSON parse error: {}", e))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VersionData {
    /// Required: all API calls specific to this version are going to be prefixed with
    /// "Az[version]"
    pub apiversion: usize,
    /// Required: git revision hash, so that we know which tag this version was deployed from
    pub git: String,
    /// Required: release date
    pub date: String,
    /// Examples to view on the frontpage
    #[serde(default)]
    pub examples: Vec<Example>,
    /// Release notes as GitHub Markdown (used both on the website and on the GitHub release page)
    #[serde(default)]
    pub notes: Vec<String>,
    // Using IndexMap to preserve module order as read from JSON
    pub api: IndexMap<String, ModuleData>,
}

pub type OsId = String;
pub type ImageFilePathRelative = String;
pub type ExampleSrcFileRelative = String;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Example {
    pub name: String,
    pub alt: String,
    pub code: LangDepFilesPaths,
    pub screenshot: OsDepFilesPaths,
    pub description: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LangDepFilesPaths {
    pub c: String,
    pub cpp: String,
    pub rust: String,
    pub python: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OsDepFilesPaths {
    pub windows: String,
    pub linux: String,
    pub mac: String,
}

impl Example {
    pub fn load(
        &self,
        filerelativepath: &str,
        imagerelativepath: &str,
    ) -> anyhow::Result<LoadedExample> {
        Ok(LoadedExample {
            name: self.name.clone(),
            alt: self.alt.clone(),
            description: self.description.clone(),
            code: LangDepFiles {
                c: std::fs::read(&Path::new(filerelativepath).join(&self.code.c)).context(
                    format!("failed to load c code for example {}", self.name.clone()),
                )?,
                cpp: std::fs::read(&Path::new(filerelativepath).join(&self.code.cpp)).context(
                    format!("failed to load cpp code for example {}", self.name.clone()),
                )?,
                rust: std::fs::read(&Path::new(filerelativepath).join(&self.code.rust)).context(
                    format!("failed to load rust code for example {}", self.name.clone()),
                )?,
                python: std::fs::read(&Path::new(filerelativepath).join(&self.code.python))
                    .context(format!(
                        "failed to load python code for example {}",
                        self.name.clone()
                    ))?,
            },
            screenshot: OsDepFiles {
                windows: std::fs::read(
                    &Path::new(imagerelativepath).join(&self.screenshot.windows),
                )
                .context(format!(
                    "failed to load windows screenshot for example {}",
                    self.name.clone()
                ))?,
                linux: std::fs::read(&Path::new(imagerelativepath).join(&self.screenshot.linux))
                    .context(format!(
                        "failed to load linux screenshot for example {}",
                        self.name.clone()
                    ))?,
                mac: std::fs::read(&Path::new(imagerelativepath).join(&self.screenshot.mac))
                    .context(format!(
                        "failed to load mac screenshot for example {}",
                        self.name.clone()
                    ))?,
            },
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LoadedExample {
    /// Id of the examples
    pub name: String,
    /// Short description of the image
    pub alt: String,
    /// Markdown description of the example
    pub description: Vec<String>,
    /// Code example loaded to string
    pub code: LangDepFiles,
    /// Image file loaded to string
    pub screenshot: OsDepFiles,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OsDepFiles {
    pub windows: Vec<u8>,
    pub linux: Vec<u8>,
    pub mac: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LangDepFiles {
    pub c: Vec<u8>,
    pub cpp: Vec<u8>,
    pub rust: Vec<u8>,
    pub python: Vec<u8>,
}

impl VersionData {
    // Find a class definition within this specific version.
    // Returns Option<(module_name, class_name, &ClassData)>
    pub fn find_class<'a>(
        &'a self,
        search_class_name: &str,
    ) -> Option<(&'a str, &'a str, &'a ClassData)> {
        for (module_name, module_data) in &self.api {
            if let Some((class_name, class_data)) = module_data.find_class(search_class_name) {
                return Some((module_name.as_str(), class_name, class_data));
            }
        }
        None
    }

    // Get a specific class if module and class name are known for this version.
    pub fn get_class(&self, module_name: &str, class_name: &str) -> Option<&ClassData> {
        self.api.get(module_name)?.classes.get(class_name)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModuleData {
    pub doc: Option<String>,
    // Using IndexMap to preserve class order within a module
    pub classes: IndexMap<String, ClassData>,
}

impl ModuleData {
    // Find a class within this specific module.
    // Returns Option<(class_name, &ClassData)>
    pub fn find_class<'a>(&'a self, search_class_name: &str) -> Option<(&'a str, &'a ClassData)> {
        self.classes
            .iter()
            .find(|(name, _)| *name == search_class_name)
            .map(|(name, data)| (name.as_str(), data))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ClassData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")] // Skip if false
    pub is_boxed_object: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clone: Option<bool>, // If missing, generation logic should assume true
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_destructor: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derive: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serde: Option<String>, // Serde attributes like "transparent"
    // Renamed from "const" which is a keyword
    #[serde(rename = "const", default, skip_serializing_if = "Option::is_none")]
    pub const_value_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constants: Option<Vec<IndexMap<String, ConstantData>>>, /* Use IndexMap if field order
                                                                 * matters */
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub struct_fields: Option<Vec<IndexMap<String, FieldData>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enum_fields: Option<Vec<IndexMap<String, EnumVariantData>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback_typedef: Option<CallbackDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    // Using IndexMap to preserve function/constructor order
    pub constructors: Option<IndexMap<String, FunctionData>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub functions: Option<IndexMap<String, FunctionData>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub use_patches: Option<Vec<String>>, // For conditional patch application
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repr: Option<String>, // For things like #[repr(transparent)]
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConstantData {
    pub r#type: String, // r# to allow "type" as field name
    pub value: String,  // Keep value as string, parsing depends on type context
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct FieldData {
    pub r#type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derive: Option<Vec<String>>, // For field-level derives like #[pyo3(get, set)]
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct EnumVariantData {
    // Variants might not have an associated type (e.g., simple enums like MsgBoxIcon)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct FunctionData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    // Arguments are a list where each item is a map like {"arg_name": "type"}
    // Using IndexMap here preserves argument order.
    #[serde(default, rename = "fn_args")]
    pub fn_args: Vec<IndexMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returns: Option<ReturnTypeData>,
    #[serde(rename = "fn_body", default, skip_serializing_if = "Option::is_none")]
    pub fn_body: Option<String>, // Present in api.json for DLL generation
    #[serde(default, rename = "use_patches", skip_serializing_if = "Option::is_none")]
    pub use_patches: Option<Vec<String>>, // Which languages this patch applies to
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReturnTypeData {
    pub r#type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CallbackDefinition {
    #[serde(default, rename = "fn_args")]
    pub fn_args: Vec<CallbackArgData>,
    pub returns: Option<ReturnTypeData>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CallbackArgData {
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(rename = "ref")]
    pub ref_kind: String, // "ref", "refmut", "value"
    pub doc: Option<String>,
}
