use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use anyhow::Context;
use indexmap::IndexMap; // Use IndexMap for ordered fields where necessary
use serde_derive::{Deserialize, Serialize}; // Use BTreeMap for sorted keys (versions)

// Re-export BorrowMode for use in API structures
pub use crate::autofix::types::borrow::BorrowMode;
pub use crate::autofix::types::ref_kind::RefKind;

// Helper function to check if a bool is false (for skip_serializing_if)
fn is_false(b: &bool) -> bool {
    !b
}

// Helper function to check if a string is empty (for skip_serializing_if)
fn is_empty_string(s: &str) -> bool {
    s.is_empty()
}

// Helper function to check if an Option<String> is None or empty
fn is_none_or_empty(opt: &Option<String>) -> bool {
    match opt {
        None => true,
        Some(s) => s.is_empty(),
    }
}

// Helper function to check if an Option<Vec<String>> is None or empty
fn is_none_or_empty_vec(opt: &Option<Vec<String>>) -> bool {
    match opt {
        None => true,
        Some(v) => v.is_empty(),
    }
}

// Helper function to check if RefKind is Value (default)
fn is_ref_kind_value(kind: &RefKind) -> bool {
    kind.is_default()
}

/// Deserializes a doc field that can be either a String or Vec<String>.
/// This enables backwards compatibility with old api.json files that use String.
fn deserialize_doc<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, SeqAccess, Visitor};

    struct DocVisitor;

    impl<'de> Visitor<'de> for DocVisitor {
        type Value = Option<Vec<String>>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("null, a string, or an array of strings")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if v.is_empty() {
                Ok(None)
            } else {
                Ok(Some(vec![v.to_string()]))
            }
        }

        fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if v.is_empty() {
                Ok(None)
            } else {
                Ok(Some(vec![v]))
            }
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut vec = Vec::new();
            while let Some(item) = seq.next_element::<String>()? {
                vec.push(item);
            }
            if vec.is_empty() {
                Ok(None)
            } else {
                Ok(Some(vec))
            }
        }
    }

    deserializer.deserialize_any(DocVisitor)
}

/// Deserializes a title field that can be either a String or Vec<String>.
/// Returns Vec<String> (never None) - single string becomes a one-element vector.
fn deserialize_string_or_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, SeqAccess, Visitor};

    struct StringOrVecVisitor;

    impl<'de> Visitor<'de> for StringOrVecVisitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string or an array of strings")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(vec![v.to_string()])
        }

        fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(vec![v])
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut vec = Vec::new();
            while let Some(item) = seq.next_element::<String>()? {
                vec.push(item);
            }
            Ok(vec)
        }
    }

    deserializer.deserialize_any(StringOrVecVisitor)
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
        // Always return "Az" without version number
        if self.0.contains_key(version_name) {
            Some("Az".to_string())
        } else {
            None
        }
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
    /// Installation instructions per language/OS
    #[serde(default)]
    pub installation: Installation,
    /// Package metadata for generating .deb/.rpm packages via NFPM
    #[serde(default)]
    pub package: Option<PackageConfig>,
    /// Examples to view on the frontpage
    #[serde(default)]
    pub examples: Vec<Example>,
    /// Release notes as GitHub Markdown (used both on the website and on the GitHub release page)
    #[serde(default)]
    pub notes: Vec<String>,
    // Using IndexMap to preserve module order as read from JSON
    pub api: IndexMap<String, ModuleData>,
}

/// Configuration for generating Linux packages (.deb, .rpm) via NFPM
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PackageConfig {
    /// Package name (e.g., "azul")
    #[serde(default)]
    pub name: String,
    /// Package description
    #[serde(default)]
    pub description: String,
    /// Project homepage URL
    #[serde(default)]
    pub homepage: String,
    /// Repository URL
    #[serde(default)]
    pub repository: String,
    /// License identifier (e.g., "MIT")
    #[serde(default)]
    pub license: String,
    /// Debian section (e.g., "libs")
    #[serde(default)]
    pub section: String,
    /// Debian priority (e.g., "optional")
    #[serde(default)]
    pub priority: String,
    /// Package maintainer in "Name <email>" format
    #[serde(default)]
    pub maintainer: String,
    /// Vendor name
    #[serde(default)]
    pub vendor: String,
    /// Linux/Debian-specific package configuration
    #[serde(default)]
    pub linux: LinuxPackageConfig,
    /// RPM-specific configuration
    #[serde(default)]
    pub rpm: RpmPackageConfig,
}

/// Linux/Debian-specific package dependencies and contents
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct LinuxPackageConfig {
    /// Required dependencies (package must have these installed)
    #[serde(default)]
    pub depends: Vec<String>,
    /// Recommended packages (should be installed for full functionality)
    #[serde(default)]
    pub recommends: Vec<String>,
    /// Suggested packages (optional, nice to have)
    #[serde(default)]
    pub suggests: Vec<String>,
    /// Files to include in the package
    #[serde(default)]
    pub contents: Vec<PackageContent>,
}

/// RPM-specific package configuration
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct RpmPackageConfig {
    /// Package group (e.g., "Development/Libraries")
    #[serde(default)]
    pub group: String,
    /// Required dependencies
    #[serde(default)]
    pub depends: Vec<String>,
    /// Recommended packages
    #[serde(default)]
    pub recommends: Vec<String>,
    /// Suggested packages
    #[serde(default)]
    pub suggests: Vec<String>,
}

/// A file or directory to include in the package
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PackageContent {
    /// Source path (relative to build directory)
    pub src: String,
    /// Destination path in the installed package
    pub dst: String,
    /// Type of content: "file", "dir", "config", "symlink"
    #[serde(rename = "type", default = "default_content_type")]
    pub content_type: String,
}

fn default_content_type() -> String {
    "file".to_string()
}

pub type OsId = String;
pub type ImageFilePathRelative = String;
pub type ExampleSrcFileRelative = String;

/// Operating system identifiers for platform-specific content
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    Windows,
    Linux,
    #[serde(alias = "mac")]
    Macos,
}

impl Os {
    pub fn all() -> &'static [Os] {
        &[Os::Windows, Os::Linux, Os::Macos]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Os::Windows => "windows",
            Os::Linux => "linux",
            Os::Macos => "macos",
        }
    }
}

/// Supported programming languages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Rust,
    C,
    Cpp03,
    Cpp11,
    Cpp14,
    Cpp17,
    Cpp20,
    Cpp23,
    Python,
}

impl Language {
    pub fn all() -> &'static [Language] {
        &[
            Language::Rust,
            Language::C,
            Language::Cpp03,
            Language::Cpp11,
            Language::Cpp14,
            Language::Cpp17,
            Language::Cpp20,
            Language::Cpp23,
            Language::Python,
        ]
    }

    pub fn cpp_versions() -> &'static [Language] {
        &[
            Language::Cpp03,
            Language::Cpp11,
            Language::Cpp14,
            Language::Cpp17,
            Language::Cpp20,
            Language::Cpp23,
        ]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::C => "c",
            Language::Cpp03 => "cpp03",
            Language::Cpp11 => "cpp11",
            Language::Cpp14 => "cpp14",
            Language::Cpp17 => "cpp17",
            Language::Cpp20 => "cpp20",
            Language::Cpp23 => "cpp23",
            Language::Python => "python",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Language::Rust => "Rust",
            Language::C => "C",
            Language::Cpp03 => "C++03",
            Language::Cpp11 => "C++11",
            Language::Cpp14 => "C++14",
            Language::Cpp17 => "C++17",
            Language::Cpp20 => "C++20",
            Language::Cpp23 => "C++23",
            Language::Python => "Python",
        }
    }

    pub fn is_cpp(&self) -> bool {
        matches!(
            self,
            Language::Cpp03
                | Language::Cpp11
                | Language::Cpp14
                | Language::Cpp17
                | Language::Cpp20
                | Language::Cpp23
        )
    }

    pub fn cpp_std_flag(&self) -> Option<&'static str> {
        match self {
            Language::Cpp03 => Some("-std=c++03"),
            Language::Cpp11 => Some("-std=c++11"),
            Language::Cpp14 => Some("-std=c++14"),
            Language::Cpp17 => Some("-std=c++17"),
            Language::Cpp20 => Some("-std=c++20"),
            Language::Cpp23 => Some("-std=c++23"),
            _ => None,
        }
    }
}

/// Dialect configuration for language groups (e.g., C++ with multiple standards)
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct DialectConfig {
    /// Display name for the dialect group (e.g., "C++")
    #[serde(rename = "displayName")]
    pub display_name: String,
    /// Default variant to use (e.g., "cpp23")
    pub default: String,
    /// Available variants with their display names and alt texts
    pub variants: BTreeMap<String, DialectVariant>,
}

/// A specific dialect variant (e.g., C++23)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DialectVariant {
    /// Display name (e.g., "C++23")
    #[serde(rename = "displayName")]
    pub display_name: String,
    /// Alt text for accessibility (e.g., "Example for C++ 2023 standard")
    #[serde(rename = "altText", default)]
    pub alt_text: String,
}

/// Language installation configuration
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LanguageInstallConfig {
    /// Display name for the language
    #[serde(rename = "displayName")]
    pub display_name: String,
    /// If this is a dialect of another language group (e.g., "cpp" for cpp23)
    #[serde(rename = "dialectOf", default, skip_serializing_if = "Option::is_none")]
    pub dialect_of: Option<String>,
    /// Installation methods (for languages like Python with pip/uv)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub methods: Option<BTreeMap<String, MethodConfig>>,
    /// Platform-specific installation (for C/C++)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platforms: Option<BTreeMap<String, InstallationSteps>>,
}

/// Configuration for an installation method
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MethodConfig {
    /// Display name for the method (e.g., "pip")
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
    /// Description of the method
    pub description: String,
    /// Installation steps
    pub steps: Vec<InstallationStep>,
}

/// Installation instructions with new structure
/// Supports dialects (language groups like C++) and multiple methods per language
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Installation {
    /// Order of language tabs to display (e.g., ["rust", "python", "cpp", "c"])
    /// Languages not in this list will be appended at the end
    #[serde(default, rename = "tabOrder")]
    pub tab_order: Vec<String>,
    /// Dialect configurations (e.g., cpp -> { displayName: "C++", default: "cpp23", variants:
    /// {...} })
    #[serde(default)]
    pub dialects: BTreeMap<String, DialectConfig>,
    /// Language-specific installation instructions
    #[serde(default)]
    pub languages: BTreeMap<String, LanguageInstallConfig>,
}

impl Installation {
    /// Get the list of top-level languages (excluding dialect variants shown separately)
    pub fn get_top_level_languages(&self) -> Vec<&str> {
        let mut langs: Vec<&str> = Vec::new();
        let dialect_keys: BTreeSet<&str> = self.dialects.keys().map(|s| s.as_str()).collect();

        for (key, config) in &self.languages {
            // Skip if this is a dialect variant (has dialectOf set)
            if config.dialect_of.is_some() {
                continue;
            }
            langs.push(key.as_str());
        }

        // Add dialect groups
        for key in &dialect_keys {
            langs.push(key);
        }

        langs
    }

    /// Get installation steps for a specific language and OS
    pub fn get_steps(&self, lang: &str, os: &str) -> Option<&InstallationSteps> {
        let lang_config = self.languages.get(lang)?;

        // If the language has platform-specific installation
        if let Some(platforms) = &lang_config.platforms {
            return platforms.get(os);
        }

        // If the language has methods, return the first method's steps wrapped
        // (This is a simplified view - for methods, use get_method_steps)
        None
    }

    /// Get the methods available for a language
    pub fn get_methods(&self, lang: &str) -> Vec<&str> {
        self.languages
            .get(lang)
            .and_then(|c| c.methods.as_ref())
            .map(|m| m.keys().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }

    /// Get installation steps for a specific method
    pub fn get_method_steps(&self, lang: &str, method: &str) -> Option<InstallationSteps> {
        let lang_config = self.languages.get(lang)?;
        let methods = lang_config.methods.as_ref()?;
        let method_config = methods.get(method)?;

        Some(InstallationSteps {
            description: method_config.description.clone(),
            steps: method_config.steps.clone(),
        })
    }

    /// Get dialect configuration for a language group
    pub fn get_dialect(&self, group: &str) -> Option<&DialectConfig> {
        self.dialects.get(group)
    }

    /// Check if a language is a dialect of a group
    pub fn is_dialect(&self, lang: &str) -> bool {
        self.languages
            .get(lang)
            .and_then(|c| c.dialect_of.as_ref())
            .is_some()
    }

    /// Get the dialect group for a language (if any)
    pub fn get_dialect_group(&self, lang: &str) -> Option<&str> {
        self.languages
            .get(lang)
            .and_then(|c| c.dialect_of.as_ref())
            .map(|s| s.as_str())
    }
}

/// OS-specific installation instructions
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct OsSpecificInstallation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windows: Option<InstallationSteps>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linux: Option<InstallationSteps>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macos: Option<InstallationSteps>,
}

impl OsSpecificInstallation {
    pub fn get_for_os(&self, os: Os) -> Option<&InstallationSteps> {
        match os {
            Os::Windows => self.windows.as_ref(),
            Os::Linux => self.linux.as_ref(),
            Os::Macos => self.macos.as_ref(),
        }
    }
}

/// Installation steps with description
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InstallationSteps {
    pub description: String,
    pub steps: Vec<InstallationStep>,
}

/// A single installation step
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum InstallationStep {
    /// A code block to show (not executable)
    Code { language: String, content: String },
    /// A shell command to run
    Command { content: String },
    /// Descriptive text
    Text { content: String },
}

impl InstallationStep {
    /// Apply variable interpolation to the step content
    pub fn interpolate(&self, hostname: &str, version: &str) -> Self {
        let do_interpolate = |s: &str| {
            s.replace("$HOSTNAME", hostname)
                .replace("$VERSION", version)
        };

        match self {
            InstallationStep::Code { language, content } => InstallationStep::Code {
                language: language.clone(),
                content: do_interpolate(content),
            },
            InstallationStep::Command { content } => InstallationStep::Command {
                content: do_interpolate(content),
            },
            InstallationStep::Text { content } => InstallationStep::Text {
                content: do_interpolate(content),
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Example {
    pub name: String,
    /// Title for display - can be single line or multiple lines for wrapping
    #[serde(default, deserialize_with = "deserialize_string_or_vec")]
    pub title: Vec<String>,
    pub alt: String,
    /// Whether to show this example on the index page
    #[serde(default = "default_true")]
    pub show_on_index: bool,
    pub code: ExampleCodePaths,
    pub screenshot: OsDepFilesPaths,
    pub description: Vec<String>,
}

fn default_true() -> bool {
    true
}

/// Paths to example source files for each language
/// Supports both old format (single cpp) and new format (per-C++ standard)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExampleCodePaths {
    pub c: String,
    pub rust: String,
    pub python: String,
    /// Legacy: single C++ path (deprecated, use cpp11/cpp14/etc.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpp: Option<String>,
    /// C++03 example path
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpp03: Option<String>,
    /// C++11 example path
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpp11: Option<String>,
    /// C++14 example path
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpp14: Option<String>,
    /// C++17 example path
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpp17: Option<String>,
    /// C++20 example path
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpp20: Option<String>,
    /// C++23 example path
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpp23: Option<String>,
}

impl ExampleCodePaths {
    /// Get the path for a specific language
    pub fn get_path(&self, lang: Language) -> Option<&str> {
        match lang {
            Language::C => Some(&self.c),
            Language::Rust => Some(&self.rust),
            Language::Python => Some(&self.python),
            Language::Cpp03 => self.cpp03.as_deref().or(self.cpp.as_deref()),
            Language::Cpp11 => self.cpp11.as_deref().or(self.cpp.as_deref()),
            Language::Cpp14 => self.cpp14.as_deref().or(self.cpp.as_deref()),
            Language::Cpp17 => self.cpp17.as_deref().or(self.cpp.as_deref()),
            Language::Cpp20 => self.cpp20.as_deref().or(self.cpp.as_deref()),
            Language::Cpp23 => self.cpp23.as_deref().or(self.cpp.as_deref()),
        }
    }

    /// Get the best available C++ path (prefer cpp23, fall back to legacy cpp)
    pub fn get_default_cpp_path(&self) -> Option<&str> {
        self.cpp23
            .as_deref()
            .or(self.cpp20.as_deref())
            .or(self.cpp17.as_deref())
            .or(self.cpp14.as_deref())
            .or(self.cpp11.as_deref())
            .or(self.cpp03.as_deref())
            .or(self.cpp.as_deref())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OsDepFilesPaths {
    pub windows: String,
    pub linux: String,
    #[serde(alias = "macos")]
    pub mac: String,
}

impl Example {
    /// Load example files from disk
    /// The `filerelativepath` is the base path for example source files
    /// The `imagerelativepath` is the base path for screenshots
    pub fn load(
        &self,
        filerelativepath: &str,
        imagerelativepath: &str,
    ) -> anyhow::Result<LoadedExample> {
        let base_path = Path::new(filerelativepath);
        let img_path = Path::new(imagerelativepath);

        // Load required language files
        let c = std::fs::read(base_path.join(&self.code.c))
            .context(format!("failed to load C code for example {}", self.name))?;
        let rust = std::fs::read(base_path.join(&self.code.rust)).context(format!(
            "failed to load Rust code for example {}",
            self.name
        ))?;
        let python = std::fs::read(base_path.join(&self.code.python)).context(format!(
            "failed to load Python code for example {}",
            self.name
        ))?;

        // Load C++ code - try versioned paths first, then fall back to legacy
        let cpp = self
            .code
            .get_default_cpp_path()
            .and_then(|p| std::fs::read(base_path.join(p)).ok())
            .unwrap_or_default();

        // Load C++ versions (optional - may not all exist)
        let load_cpp_version = |path: &Option<String>| -> Vec<u8> {
            path.as_ref()
                .and_then(|p| std::fs::read(base_path.join(p)).ok())
                .unwrap_or_default()
        };

        let cpp_versions = CppVersionedCode {
            cpp03: load_cpp_version(&self.code.cpp03),
            cpp11: load_cpp_version(&self.code.cpp11),
            cpp14: load_cpp_version(&self.code.cpp14),
            cpp17: load_cpp_version(&self.code.cpp17),
            cpp20: load_cpp_version(&self.code.cpp20),
            cpp23: load_cpp_version(&self.code.cpp23),
        };

        // Load screenshots with fallback to calculator.png if missing
        let fallback_screenshot = "calculator.png";
        let load_screenshot = |path: &str| -> Vec<u8> {
            std::fs::read(img_path.join(path))
                .or_else(|_| {
                    eprintln!(
                        "  [WARN] Screenshot '{}' not found, using fallback '{}'",
                        path, fallback_screenshot
                    );
                    std::fs::read(img_path.join(fallback_screenshot))
                })
                .unwrap_or_default()
        };

        Ok(LoadedExample {
            name: self.name.clone(),
            title: if self.title.is_empty() {
                vec![self.name.replace('-', " ")]
            } else {
                self.title.clone()
            },
            alt: self.alt.clone(),
            show_on_index: self.show_on_index,
            description: self.description.clone(),
            code: LangDepFiles {
                c,
                cpp,
                cpp_versions,
                rust,
                python,
            },
            screenshot: OsDepFiles {
                windows: load_screenshot(&self.screenshot.windows),
                linux: load_screenshot(&self.screenshot.linux),
                mac: load_screenshot(&self.screenshot.mac),
            },
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LoadedExample {
    /// Id of the example
    pub name: String,
    /// Title for display - can be multiple lines for wrapping
    pub title: Vec<String>,
    /// Short description of the image
    pub alt: String,
    /// Whether to show on index page
    pub show_on_index: bool,
    /// Markdown description of the example
    pub description: Vec<String>,
    /// Code examples loaded to bytes
    pub code: LangDepFiles,
    /// Screenshot images loaded to bytes
    pub screenshot: OsDepFiles,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OsDepFiles {
    pub windows: Vec<u8>,
    pub linux: Vec<u8>,
    pub mac: Vec<u8>,
}

/// Code for each C++ standard version
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CppVersionedCode {
    pub cpp03: Vec<u8>,
    pub cpp11: Vec<u8>,
    pub cpp14: Vec<u8>,
    pub cpp17: Vec<u8>,
    pub cpp20: Vec<u8>,
    pub cpp23: Vec<u8>,
}

impl CppVersionedCode {
    /// Get code for a specific C++ version
    pub fn get(&self, lang: Language) -> Option<&[u8]> {
        let code = match lang {
            Language::Cpp03 => &self.cpp03,
            Language::Cpp11 => &self.cpp11,
            Language::Cpp14 => &self.cpp14,
            Language::Cpp17 => &self.cpp17,
            Language::Cpp20 => &self.cpp20,
            Language::Cpp23 => &self.cpp23,
            _ => return None,
        };
        if code.is_empty() {
            None
        } else {
            Some(code)
        }
    }

    /// Get the best available C++ code (prefer newest standard)
    pub fn get_best(&self) -> &[u8] {
        if !self.cpp23.is_empty() {
            &self.cpp23
        } else if !self.cpp20.is_empty() {
            &self.cpp20
        } else if !self.cpp17.is_empty() {
            &self.cpp17
        } else if !self.cpp14.is_empty() {
            &self.cpp14
        } else if !self.cpp11.is_empty() {
            &self.cpp11
        } else if !self.cpp03.is_empty() {
            &self.cpp03
        } else {
            &[]
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LangDepFiles {
    pub c: Vec<u8>,
    /// Legacy/default C++ code (for backwards compatibility)
    pub cpp: Vec<u8>,
    /// C++ code per standard version
    #[serde(default)]
    pub cpp_versions: CppVersionedCode,
    pub rust: Vec<u8>,
    pub python: Vec<u8>,
}

impl LangDepFiles {
    /// Get code for a specific language
    pub fn get(&self, lang: Language) -> Option<&[u8]> {
        match lang {
            Language::C => Some(&self.c),
            Language::Rust => Some(&self.rust),
            Language::Python => Some(&self.python),
            Language::Cpp03
            | Language::Cpp11
            | Language::Cpp14
            | Language::Cpp17
            | Language::Cpp20
            | Language::Cpp23 => {
                // Try versioned code first, fall back to legacy cpp
                self.cpp_versions.get(lang).or(if self.cpp.is_empty() {
                    None
                } else {
                    Some(&self.cpp[..])
                })
            }
        }
    }

    /// Get the best available C++ code
    pub fn get_cpp(&self) -> &[u8] {
        let versioned = self.cpp_versions.get_best();
        if !versioned.is_empty() {
            versioned
        } else {
            &self.cpp
        }
    }
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
    #[serde(
        default,
        skip_serializing_if = "is_none_or_empty_vec",
        deserialize_with = "deserialize_doc"
    )]
    pub doc: Option<Vec<String>>,
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
    #[serde(
        default,
        skip_serializing_if = "is_none_or_empty_vec",
        deserialize_with = "deserialize_doc"
    )]
    pub doc: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")] // Skip if false
    pub is_boxed_object: bool,
    /// Traits with manual `impl Trait for Type` blocks (e.g., ["Clone", "Drop"])
    /// These require DLL functions like `AzTypeName_deepCopy` and `AzTypeName_delete`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_impls: Option<Vec<String>>,
    // DEPRECATED: Use custom_impls: ["Clone"] instead. Kept for backwards compatibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clone: Option<bool>,
    // DEPRECATED: Use custom_impls: ["Drop"] instead. Kept for backwards compatibility.
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generic_params: Option<Vec<String>>, // e.g., ["T", "U"] for generic types
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_alias: Option<TypeAliasInfo>, // Information about type aliases
    /// For VecRef/VecRefMut types: the element type (e.g., "u8" for U8VecRef)
    /// This auto-generates `as_slice()` / `as_mut_slice()` methods and `From<&[T]>` impl
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vec_ref_element_type: Option<String>,
    /// Whether this is a mutable VecRef (VecRefMut) - affects generated method signatures
    #[serde(default, skip_serializing_if = "is_false")]
    pub vec_ref_is_mut: bool,
    /// For Vec types: the element type (e.g., "StringPair" for StringPairVec)
    /// This is used to generate proper trait implementations in memtest
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vec_element_type: Option<String>,
}

impl ClassData {
    /// Check if this type has a custom `impl Clone` (not #[derive(Clone)])
    /// Returns true if custom_impls contains "Clone" OR legacy clone field is Some(false)
    pub fn has_custom_clone(&self) -> bool {
        if let Some(ref impls) = self.custom_impls {
            if impls.iter().any(|s| s == "Clone") {
                return true;
            }
        }
        // Legacy: clone: false means "don't derive Clone" which implies custom impl needed
        // clone: true or None means "derive Clone automatically"
        false
    }

    /// Check if this type has a custom `impl Drop` (not automatic)
    /// Returns true if custom_impls contains "Drop" OR legacy custom_destructor is true
    pub fn has_custom_drop(&self) -> bool {
        if let Some(ref impls) = self.custom_impls {
            if impls.iter().any(|s| s == "Drop") {
                return true;
            }
        }
        // Legacy: custom_destructor: true means custom Drop impl
        self.custom_destructor.unwrap_or(false)
    }

    /// Check if Clone can be derived automatically (not custom impl)
    /// Returns true if no custom Clone impl exists
    pub fn can_derive_clone(&self) -> bool {
        !self.has_custom_clone() && self.clone.unwrap_or(true)
    }

    /// Check if a specific trait has a custom implementation
    pub fn has_custom_impl(&self, trait_name: &str) -> bool {
        if let Some(ref impls) = self.custom_impls {
            return impls.iter().any(|s| s == trait_name);
        }
        // Handle legacy fields
        match trait_name {
            "Drop" => self.custom_destructor.unwrap_or(false),
            "Clone" => false, // Legacy clone field doesn't indicate custom impl
            _ => false,
        }
    }

    /// Check if this type has a trait (either derived or custom impl)
    pub fn has_trait(&self, trait_name: &str) -> bool {
        // Check derive list
        if let Some(ref derives) = self.derive {
            if derives.iter().any(|s| s == trait_name) {
                return true;
            }
        }
        // Check custom_impls list
        self.has_custom_impl(trait_name)
    }

    /// Check if type has PartialEq (either derived or custom)
    pub fn has_partial_eq(&self) -> bool {
        self.has_trait("PartialEq")
    }

    /// Check if type has Eq (either derived or custom)
    pub fn has_eq(&self) -> bool {
        self.has_trait("Eq")
    }

    /// Check if type has PartialOrd (either derived or custom)
    pub fn has_partial_ord(&self) -> bool {
        self.has_trait("PartialOrd")
    }

    /// Check if type has Ord (either derived or custom)
    pub fn has_ord(&self) -> bool {
        self.has_trait("Ord")
    }

    /// Check if type has Hash (either derived or custom)
    pub fn has_hash(&self) -> bool {
        self.has_trait("Hash")
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConstantData {
    pub r#type: String, // r# to allow "type" as field name
    pub value: String,  // Keep value as string, parsing depends on type context
}

/// Information about a type alias, including generic instantiation
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct TypeAliasInfo {
    /// The target type (e.g., "c_void" for pointer aliases, "CssPropertyValue" for generics)
    pub target: String,
    /// Reference kind for pointer types: "constptr" (*const T), "mutptr" (*mut T), or default
    /// "value" (T)
    #[serde(default, skip_serializing_if = "is_ref_kind_value")]
    pub ref_kind: RefKind,
    /// Generic arguments for instantiation (e.g., ["LayoutZIndex"])
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub generic_args: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct FieldData {
    pub r#type: String,
    /// Reference kind for pointer types: "constptr" (*const T), "mutptr" (*mut T), or default
    /// "value" (T)
    #[serde(default, skip_serializing_if = "is_ref_kind_value")]
    pub ref_kind: RefKind,
    /// Array size for fixed-size arrays. If Some(N), the type is [T; N] where T is in `type`
    /// field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arraysize: Option<usize>,
    #[serde(
        default,
        skip_serializing_if = "is_none_or_empty_vec",
        deserialize_with = "deserialize_doc"
    )]
    pub doc: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derive: Option<Vec<String>>, // For field-level derives like #[pyo3(get, set)]
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct EnumVariantData {
    // Variants might not have an associated type (e.g., simple enums like MsgBoxIcon)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    /// Reference kind for pointer types in enum variant payloads
    #[serde(default, skip_serializing_if = "is_ref_kind_value")]
    pub ref_kind: RefKind,
    #[serde(
        default,
        skip_serializing_if = "is_none_or_empty_vec",
        deserialize_with = "deserialize_doc"
    )]
    pub doc: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct FunctionData {
    #[serde(
        default,
        skip_serializing_if = "is_none_or_empty_vec",
        deserialize_with = "deserialize_doc"
    )]
    pub doc: Option<Vec<String>>,
    // Arguments are a list where each item is a map like {"arg_name": "type"}
    // Using IndexMap here preserves argument order.
    #[serde(default, rename = "fn_args")]
    pub fn_args: Vec<IndexMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returns: Option<ReturnTypeData>,
    #[serde(rename = "fn_body", default, skip_serializing_if = "Option::is_none")]
    pub fn_body: Option<String>, // Present in api.json for DLL generation
    #[serde(
        default,
        rename = "use_patches",
        skip_serializing_if = "Option::is_none"
    )]
    pub use_patches: Option<Vec<String>>, // Which languages this patch applies to
    /// Whether this function should be `const fn` in Rust
    #[serde(default, skip_serializing_if = "is_false")]
    pub const_fn: bool,
    /// Generic type parameters for this function (e.g., ["T", "U"])
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generic_params: Option<Vec<String>>,
    /// Generic type bounds/constraints (e.g., ["T: Clone", "U: Default"])
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generic_bounds: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReturnTypeData {
    pub r#type: String,
    #[serde(
        default,
        skip_serializing_if = "is_none_or_empty_vec",
        deserialize_with = "deserialize_doc"
    )]
    pub doc: Option<Vec<String>>,
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
    /// Reference kind for callback argument - supports full range including pointers
    /// Uses same RefKind as struct fields for consistency
    #[serde(default, skip_serializing_if = "is_ref_kind_value")]
    pub ref_kind: RefKind,
    #[serde(
        default,
        skip_serializing_if = "is_none_or_empty_vec",
        deserialize_with = "deserialize_doc"
    )]
    pub doc: Option<Vec<String>>,
}

// --- HELPER FUNCTIONS BELOW ---
//
// Helper functions to traverse complex API structures and extract type references
//
// The API structures are deeply nested with Vec<IndexMap<>>, Option<>, etc.
// These helpers make it easy to extract all type references for recursive discovery.

/// Extract all type references from a ClassData
pub fn extract_types_from_class_data(class_data: &ClassData) -> BTreeSet<String> {
    let mut types = BTreeSet::new();

    // Extract from struct fields
    if let Some(struct_fields) = &class_data.struct_fields {
        for field_map in struct_fields {
            for (_field_name, field_data) in field_map {
                types.extend(extract_types_from_field_data(field_data));
            }
        }
    }

    // Extract from enum variants
    if let Some(enum_fields) = &class_data.enum_fields {
        for variant_map in enum_fields {
            for (_variant_name, variant_data) in variant_map {
                types.extend(extract_types_from_enum_variant(variant_data));
            }
        }
    }

    // Extract from functions
    if let Some(functions) = &class_data.functions {
        for (_fn_name, fn_data) in functions {
            types.extend(extract_types_from_function_data(fn_data));
        }
    }

    // Extract from callback_typedef
    if let Some(callback_def) = &class_data.callback_typedef {
        types.extend(extract_types_from_callback_definition(callback_def));
    }

    // Extract from type_alias generic arguments
    if let Some(type_alias) = &class_data.type_alias {
        // Add the target type (e.g., CssPropertyValue)
        if let Some(base_type) = extract_base_type_if_not_opaque(&type_alias.target) {
            types.insert(base_type);
        }
        // Add all generic arguments (e.g., LayoutZIndex from CssPropertyValue<LayoutZIndex>)
        for generic_arg in &type_alias.generic_args {
            if let Some(base_type) = extract_base_type_if_not_opaque(generic_arg) {
                types.insert(base_type);
            }
        }
    }

    types
}

/// Extract type from FieldData
/// Skips types behind pointers (they don't need to be in the API)
pub fn extract_types_from_field_data(field_data: &FieldData) -> BTreeSet<String> {
    let mut types = BTreeSet::new();

    // Skip types behind pointers - they're opaque and don't need to be exposed
    if let Some(base_type) = extract_base_type_if_not_opaque(&field_data.r#type) {
        types.insert(base_type);
    }

    types
}

/// Extract types from FieldData INCLUDING pointer types
/// Used for unused type analysis where we need ALL references
pub fn extract_types_from_field_data_all(field_data: &FieldData) -> BTreeSet<String> {
    let mut types = BTreeSet::new();

    // Include types behind pointers for reachability analysis
    if let Some(base_type) = extract_base_type_including_pointers(&field_data.r#type) {
        types.insert(base_type);
    }

    types
}

/// Extract types from EnumVariantData
/// Skips types behind pointers
pub fn extract_types_from_enum_variant(variant_data: &EnumVariantData) -> BTreeSet<String> {
    let mut types = BTreeSet::new();

    if let Some(variant_type) = &variant_data.r#type {
        if let Some(base_type) = extract_base_type_if_not_opaque(variant_type) {
            types.insert(base_type);
        }
    }

    types
}

/// Extract types from EnumVariantData INCLUDING pointer types
/// Used for unused type analysis
pub fn extract_types_from_enum_variant_all(variant_data: &EnumVariantData) -> BTreeSet<String> {
    let mut types = BTreeSet::new();

    if let Some(variant_type) = &variant_data.r#type {
        if let Some(base_type) = extract_base_type_including_pointers(variant_type) {
            types.insert(base_type);
        }
    }

    types
}

/// Extract types from FunctionData
/// Skips types behind pointers
pub fn extract_types_from_function_data(fn_data: &FunctionData) -> BTreeSet<String> {
    let mut types = BTreeSet::new();

    // Extract return type
    if let Some(return_data) = &fn_data.returns {
        types.extend(extract_types_from_return_data(return_data));
    }

    // Extract parameter types
    // fn_args is Vec<IndexMap<String, String>> where key=name, value=type
    // CRITICAL: When key is "self", value is a borrow mode ("ref", "refmut", "value"),
    // NOT a type! We must skip these.
    for arg_map in &fn_data.fn_args {
        for (param_name, param_type) in arg_map {
            // Skip "self" - its value is a borrow mode, not a type
            if param_name == "self" {
                continue;
            }
            if let Some(base_type) = extract_base_type_if_not_opaque(param_type) {
                types.insert(base_type);
            }
        }
    }

    types
}

/// Extract types from CallbackDefinition
/// Skips types behind pointers
pub fn extract_types_from_callback_definition(
    callback_def: &CallbackDefinition,
) -> BTreeSet<String> {
    let mut types = BTreeSet::new();

    // Extract return type
    if let Some(return_data) = &callback_def.returns {
        types.extend(extract_types_from_return_data(return_data));
    }

    // Extract parameter types
    for arg_data in &callback_def.fn_args {
        if let Some(base_type) = extract_base_type_if_not_opaque(&arg_data.r#type) {
            types.insert(base_type);
        }
    }

    types
}

/// Extract type from ReturnTypeData
/// Skips types behind pointers
pub fn extract_types_from_return_data(return_data: &ReturnTypeData) -> BTreeSet<String> {
    let mut types = BTreeSet::new();

    if let Some(base_type) = extract_base_type_if_not_opaque(&return_data.r#type) {
        types.insert(base_type);
    }

    types
}

/// Extract base type from a type string (removes Vec, Option, Box, etc.)
///
/// Examples:
/// - "Vec<Foo>" -> "Foo"
/// - "Option<Bar>" -> "Bar"
/// - "*const Baz" -> "Baz"
/// - "&mut Qux" -> "Qux"
pub fn extract_base_type(type_str: &str) -> String {
    let trimmed = type_str.trim();

    // Handle generic types like Vec<T>, Option<T>, Box<T>, etc.
    if let Some(start) = trimmed.find('<') {
        if let Some(end) = trimmed.rfind('>') {
            let inner = &trimmed[start + 1..end];

            // If inner contains a comma (e.g., BTreeMap<K, V>), take only the first type
            // This avoids creating invalid types like "String, String"
            let first_type = if let Some(comma_pos) = find_top_level_comma(inner) {
                inner[..comma_pos].trim()
            } else {
                inner.trim()
            };

            // Recursively extract from inner type
            return extract_base_type(first_type);
        }
    }

    // Handle pointer types
    if let Some(rest) = trimmed.strip_prefix("*const ") {
        return extract_base_type(rest);
    }
    if let Some(rest) = trimmed.strip_prefix("*mut ") {
        return extract_base_type(rest);
    }

    // Handle reference types
    if let Some(rest) = trimmed.strip_prefix("&mut ") {
        return extract_base_type(rest);
    }
    if let Some(rest) = trimmed.strip_prefix('&') {
        return extract_base_type(rest);
    }

    // Handle tuple types - extract first element
    if trimmed.starts_with('(') && trimmed.ends_with(')') {
        let inner = &trimmed[1..trimmed.len() - 1];
        if let Some(comma_pos) = find_top_level_comma(inner) {
            return extract_base_type(&inner[..comma_pos]);
        }
        return extract_base_type(inner);
    }

    trimmed.to_string()
}

/// Find the position of the first comma that is not inside angle brackets
fn find_top_level_comma(s: &str) -> Option<usize> {
    let mut depth: i32 = 0;
    for (i, c) in s.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Check if a type is behind a pointer or smart pointer wrapper
/// These types don't need to be exposed in the API because they're opaque
pub fn is_behind_pointer(type_str: &str) -> bool {
    let trimmed = type_str.trim();

    // Raw pointers
    if trimmed.starts_with("*const ") || trimmed.starts_with("*mut ") {
        return true;
    }

    // References (usually opaque in FFI)
    if trimmed.starts_with("&") {
        return true;
    }

    // Smart pointers that make types opaque
    let opaque_wrappers = [
        "Box<", "Arc<", "Rc<", "Weak<", "Mutex<", "RwLock<", "RefCell<", "Cell<",
    ];

    for wrapper in &opaque_wrappers {
        if trimmed.starts_with(wrapper) {
            return true;
        }
    }

    false
}

/// Primitive types that should never be added to the API as classes
/// These are built-in language types that don't need Az prefix
const PRIMITIVE_TYPES: &[&str] = &[
    "bool", "f32", "f64", "fn", "i128", "i16", "i32", "i64", "i8", "isize", "slice", "u128", "u16",
    "u32", "u64", "u8", "()", "usize", "c_void", "str", "char", "c_char", "c_schar", "c_uchar",
];

/// Single-letter types are usually generic type parameters
fn is_generic_type_param(type_name: &str) -> bool {
    type_name.len() == 1
        && type_name
            .chars()
            .next()
            .map(|c| c.is_ascii_uppercase())
            .unwrap_or(false)
}

/// Extract base type INCLUDING types behind pointers
/// Used for unused type analysis where we need to track all references
pub fn extract_base_type_including_pointers(type_str: &str) -> Option<String> {
    let base_type = extract_base_type(type_str);

    // Don't return primitive types - they're built-in and shouldn't be in API
    if PRIMITIVE_TYPES.contains(&base_type.as_str()) {
        return None;
    }

    // Don't return single-letter generic type parameters (T, U, V, etc.)
    if is_generic_type_param(&base_type) {
        return None;
    }

    Some(base_type)
}

/// Extract base type from a type string (removes Vec, Option, Box, etc.)
/// BUT: If the type is behind a pointer/smart pointer, return None
/// Also returns None for primitive types (they shouldn't be added to API)
pub fn extract_base_type_if_not_opaque(type_str: &str) -> Option<String> {
    if is_behind_pointer(type_str) {
        return None; // Don't follow types behind pointers
    }

    let base_type = extract_base_type(type_str);

    // Don't return primitive types - they're built-in and shouldn't be in API
    if PRIMITIVE_TYPES.contains(&base_type.as_str()) {
        return None;
    }

    // Don't return single-letter generic type parameters (T, U, V, etc.)
    if is_generic_type_param(&base_type) {
        return None;
    }

    Some(base_type)
}

/// Collect all type references from the entire API
pub fn collect_all_referenced_types_from_api(api_data: &crate::api::ApiData) -> BTreeSet<String> {
    let mut types = BTreeSet::new();

    // Include callback_typedefs - they can be referenced and need patches
    // (e.g. FooDestructorType is referenced from FooDestructor enum)
    for (_version_name, version_data) in &api_data.0 {
        for (_module_name, module_data) in &version_data.api {
            for (_class_name, class_data) in &module_data.classes {
                types.extend(extract_types_from_class_data(class_data));
            }
        }
    }

    types
}

/// Collect all type references from the entire API, along with reference chains
/// Returns (types, reference_chains) where reference_chains maps type_name ->
/// first_chain_description
pub fn collect_all_referenced_types_from_api_with_chains(
    api_data: &crate::api::ApiData,
) -> (BTreeSet<String>, BTreeMap<String, String>) {
    let mut types = BTreeSet::new();
    let mut chains: BTreeMap<String, String> = BTreeMap::new();

    for (_version_name, version_data) in &api_data.0 {
        for (_module_name, module_data) in &version_data.api {
            for (class_name, class_data) in &module_data.classes {
                // Track types from functions with their chain
                if let Some(functions) = &class_data.functions {
                    for (fn_name, fn_data) in functions {
                        // Parameters - fn_args is Vec<IndexMap<String, String>>
                        // Each item is a map like {"arg_name": "type"}
                        for param_map in &fn_data.fn_args {
                            for (param_name, param_type_str) in param_map {
                                if let Some(param_type) =
                                    extract_base_type_if_not_opaque(param_type_str)
                                {
                                    if !types.contains(&param_type) {
                                        types.insert(param_type.clone());
                                        chains.insert(
                                            param_type,
                                            format!(
                                                "{}::{}() param '{}'",
                                                class_name, fn_name, param_name
                                            ),
                                        );
                                    }
                                }
                            }
                        }
                        // Return type
                        if let Some(ret_type) = &fn_data.returns {
                            if let Some(base_type) =
                                extract_base_type_if_not_opaque(&ret_type.r#type)
                            {
                                if !types.contains(&base_type) {
                                    types.insert(base_type.clone());
                                    chains.insert(
                                        base_type,
                                        format!("{}::{}() returns", class_name, fn_name),
                                    );
                                }
                            }
                        }
                    }
                }

                // Track types from constructors
                if let Some(constructors) = &class_data.constructors {
                    for (ctor_name, ctor_data) in constructors {
                        // Same structure as functions
                        for param_map in &ctor_data.fn_args {
                            for (param_name, param_type_str) in param_map {
                                if let Some(param_type) =
                                    extract_base_type_if_not_opaque(param_type_str)
                                {
                                    if !types.contains(&param_type) {
                                        types.insert(param_type.clone());
                                        chains.insert(
                                            param_type,
                                            format!(
                                                "{}::{}() param '{}'",
                                                class_name, ctor_name, param_name
                                            ),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }

                // Track types from struct fields - struct_fields is Vec<IndexMap<String,
                // FieldData>>
                if let Some(fields_vec) = &class_data.struct_fields {
                    for field_map in fields_vec {
                        for (field_name, field_data) in field_map {
                            if let Some(field_type) =
                                extract_base_type_if_not_opaque(&field_data.r#type)
                            {
                                if !types.contains(&field_type) {
                                    types.insert(field_type.clone());
                                    chains.insert(
                                        field_type,
                                        format!("{}.{}", class_name, field_name),
                                    );
                                }
                            }
                        }
                    }
                }

                // Track types from enum variants - enum_fields is Vec<IndexMap<String,
                // EnumVariantData>>
                if let Some(variants_vec) = &class_data.enum_fields {
                    for variant_map in variants_vec {
                        for (variant_name, variant_data) in variant_map {
                            if let Some(variant_type) = &variant_data.r#type {
                                if let Some(base_type) =
                                    extract_base_type_if_not_opaque(variant_type)
                                {
                                    if !types.contains(&base_type) {
                                        types.insert(base_type.clone());
                                        chains.insert(
                                            base_type,
                                            format!("{}::{}", class_name, variant_name),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }

                // Track callback typedef types - fn_args is Vec<CallbackArgData>
                if let Some(callback_def) = &class_data.callback_typedef {
                    for (idx, param_data) in callback_def.fn_args.iter().enumerate() {
                        if let Some(param_type) =
                            extract_base_type_if_not_opaque(&param_data.r#type)
                        {
                            if !types.contains(&param_type) {
                                types.insert(param_type.clone());
                                chains.insert(
                                    param_type,
                                    format!("callback {} arg#{}", class_name, idx),
                                );
                            }
                        }
                    }
                    if let Some(ret) = &callback_def.returns {
                        if let Some(base_type) = extract_base_type_if_not_opaque(&ret.r#type) {
                            if !types.contains(&base_type) {
                                types.insert(base_type.clone());
                                chains
                                    .insert(base_type, format!("callback {} returns", class_name));
                            }
                        }
                    }
                }
            }
        }
    }

    (types, chains)
}

/// Find types in api.json that are never used by any function or other type
///
/// This performs a reachability analysis starting from:
/// 1. All functions (constructors, functions) - their parameters and return types
/// 2. All callback_typedef types (they are entry points for callbacks)
///
/// Then recursively follows struct fields and enum variants to find all reachable types.
/// Returns the set of type names that are defined but never reachable.
pub fn find_unused_types(api_data: &crate::api::ApiData) -> Vec<UnusedTypeInfo> {
    let mut reachable_types: BTreeSet<String> = BTreeSet::new();
    let mut all_defined_types: BTreeMap<String, (String, String)> = BTreeMap::new(); // type_name -> (module, version)

    // Collect all defined types and build a lookup for their definitions
    // Important: Keep only the "most complete" definition (one with struct_fields or enum_fields)
    let mut type_definitions: BTreeMap<String, &ClassData> = BTreeMap::new();

    for (version_name, version_data) in &api_data.0 {
        for (module_name, module_data) in &version_data.api {
            for (class_name, class_data) in &module_data.classes {
                // Only track as "defined type" if it has a real definition
                let has_body = class_data.struct_fields.is_some()
                    || class_data.enum_fields.is_some()
                    || class_data.callback_typedef.is_some()
                    || class_data.type_alias.is_some()
                    || class_data.functions.is_some()
                    || class_data.constructors.is_some();

                // Track all occurrences for "unused" detection
                all_defined_types.insert(
                    class_name.clone(),
                    (module_name.clone(), version_name.clone()),
                );

                // For type_definitions, prefer the version with struct/enum fields
                let existing = type_definitions.get(class_name);
                let should_replace = match existing {
                    None => true,
                    Some(existing_data) => {
                        // Replace if existing has no body but new one has body
                        let existing_has_body = existing_data.struct_fields.is_some()
                            || existing_data.enum_fields.is_some()
                            || existing_data.callback_typedef.is_some()
                            || existing_data.type_alias.is_some();
                        !existing_has_body && has_body
                    }
                };

                if should_replace {
                    type_definitions.insert(class_name.clone(), class_data);
                }
            }
        }
    }

    // Phase 1: Collect all "entry point" types from functions and constructors
    let mut types_to_process: Vec<String> = Vec::new();

    for (_version_name, version_data) in &api_data.0 {
        for (_module_name, module_data) in &version_data.api {
            for (class_name, class_data) in &module_data.classes {
                // Add the class itself if it has functions or constructors
                // (it's part of the public API)
                let has_functions = class_data
                    .functions
                    .as_ref()
                    .map(|f| !f.is_empty())
                    .unwrap_or(false);
                let has_constructors = class_data
                    .constructors
                    .as_ref()
                    .map(|c| !c.is_empty())
                    .unwrap_or(false);
                let is_callback_typedef = class_data.callback_typedef.is_some();

                if has_functions || has_constructors || is_callback_typedef {
                    if !reachable_types.contains(class_name) {
                        reachable_types.insert(class_name.clone());
                        types_to_process.push(class_name.clone());
                    }
                }

                // Extract types from functions
                if let Some(functions) = &class_data.functions {
                    for (_fn_name, fn_data) in functions {
                        for type_name in extract_types_from_function_data(fn_data) {
                            if !reachable_types.contains(&type_name)
                                && all_defined_types.contains_key(&type_name)
                            {
                                reachable_types.insert(type_name.clone());
                                types_to_process.push(type_name);
                            }
                        }
                    }
                }

                // Extract types from constructors
                if let Some(constructors) = &class_data.constructors {
                    for (_ctor_name, ctor_data) in constructors {
                        for type_name in extract_types_from_function_data(ctor_data) {
                            if !reachable_types.contains(&type_name)
                                && all_defined_types.contains_key(&type_name)
                            {
                                reachable_types.insert(type_name.clone());
                                types_to_process.push(type_name);
                            }
                        }
                    }
                }

                // Extract types from callback_typedef
                if let Some(callback_def) = &class_data.callback_typedef {
                    for type_name in extract_types_from_callback_definition(callback_def) {
                        if !reachable_types.contains(&type_name)
                            && all_defined_types.contains_key(&type_name)
                        {
                            reachable_types.insert(type_name.clone());
                            types_to_process.push(type_name);
                        }
                    }
                }
            }
        }
    }

    // Phase 2: Recursively follow struct fields and enum variants
    // Use _all variants to include types behind pointers (important for Vec element types)
    let mut iteration = 0;
    let max_iterations = 100; // Safety limit

    while !types_to_process.is_empty() && iteration < max_iterations {
        iteration += 1;
        let current_batch: Vec<String> = types_to_process.drain(..).collect();

        for type_name in current_batch {
            if let Some(class_data) = type_definitions.get(&type_name) {
                // Extract types from struct fields (including pointer types)
                if let Some(struct_fields) = &class_data.struct_fields {
                    for field_map in struct_fields {
                        for (_field_name, field_data) in field_map {
                            for referenced_type in extract_types_from_field_data_all(field_data) {
                                if !reachable_types.contains(&referenced_type)
                                    && all_defined_types.contains_key(&referenced_type)
                                {
                                    reachable_types.insert(referenced_type.clone());
                                    types_to_process.push(referenced_type);
                                }
                            }
                        }
                    }
                }

                // Extract types from enum variants (including pointer types)
                if let Some(enum_fields) = &class_data.enum_fields {
                    for variant_map in enum_fields {
                        for (_variant_name, variant_data) in variant_map {
                            for referenced_type in extract_types_from_enum_variant_all(variant_data)
                            {
                                if !reachable_types.contains(&referenced_type)
                                    && all_defined_types.contains_key(&referenced_type)
                                {
                                    reachable_types.insert(referenced_type.clone());
                                    types_to_process.push(referenced_type);
                                }
                            }
                        }
                    }
                }

                // Extract types from type_alias (including pointer types)
                if let Some(type_alias) = &class_data.type_alias {
                    if let Some(base_type) =
                        extract_base_type_including_pointers(&type_alias.target)
                    {
                        if !reachable_types.contains(&base_type)
                            && all_defined_types.contains_key(&base_type)
                        {
                            reachable_types.insert(base_type.clone());
                            types_to_process.push(base_type);
                        }
                    }
                    for generic_arg in &type_alias.generic_args {
                        if let Some(base_type) = extract_base_type_including_pointers(generic_arg) {
                            if !reachable_types.contains(&base_type)
                                && all_defined_types.contains_key(&base_type)
                            {
                                reachable_types.insert(base_type.clone());
                                types_to_process.push(base_type);
                            }
                        }
                    }
                }
            }
        }
    }

    // Phase 3: Find all types that are defined but not reachable
    let mut unused: Vec<UnusedTypeInfo> = Vec::new();

    for (type_name, (module_name, version_name)) in &all_defined_types {
        if !reachable_types.contains(type_name) {
            unused.push(UnusedTypeInfo {
                type_name: type_name.clone(),
                module_name: module_name.clone(),
                version_name: version_name.clone(),
            });
        }
    }

    // Sort by module, then by type name for consistent output
    unused.sort_by(|a, b| match a.module_name.cmp(&b.module_name) {
        std::cmp::Ordering::Equal => a.type_name.cmp(&b.type_name),
        other => other,
    });

    unused
}

/// Find ALL unused types recursively by simulating removal
///
/// This works by iteratively:
/// 1. Finding unused types in the current API
/// 2. Removing them from consideration completely (as if they don't exist)
/// 3. Running the analysis again to find newly-unused types
/// 4. Repeating until no new unused types are found
///
/// This catches types that become unused only after other unused types are removed.
pub fn find_all_unused_types_recursive(api_data: &crate::api::ApiData) -> Vec<UnusedTypeInfo> {
    let mut all_unused: Vec<UnusedTypeInfo> = Vec::new();
    let mut removed_types: BTreeSet<String> = BTreeSet::new();
    let mut iteration = 0;
    let max_iterations = 50; // Safety limit

    loop {
        iteration += 1;
        if iteration > max_iterations {
            eprintln!(
                "[WARN] Max iterations ({}) reached in find_all_unused_types_recursive",
                max_iterations
            );
            break;
        }

        // Find unused types, treating already-removed types as non-existent
        let unused = find_unused_types_simulating_removal(api_data, &removed_types);

        if unused.is_empty() {
            // No more unused types found
            break;
        }

        // Mark these types as "removed" for the next iteration
        for ut in &unused {
            removed_types.insert(ut.type_name.clone());
        }

        all_unused.extend(unused);
    }

    // Sort by module, then by type name for consistent output
    all_unused.sort_by(|a, b| match a.module_name.cmp(&b.module_name) {
        std::cmp::Ordering::Equal => a.type_name.cmp(&b.type_name),
        other => other,
    });

    // Deduplicate (same type might be in multiple modules with same name)
    all_unused.dedup_by(|a, b| a.type_name == b.type_name && a.module_name == b.module_name);

    all_unused
}

/// Find unused types, simulating that some types have already been removed
///
/// This is a helper for find_all_unused_types_recursive.
/// Types in `removed` are treated as if they were already removed from the API:
/// - They are not included in all_defined_types
/// - They are not considered as entry points
/// - References TO them are ignored (as if they don't exist)
fn find_unused_types_simulating_removal(
    api_data: &crate::api::ApiData,
    removed: &BTreeSet<String>,
) -> Vec<UnusedTypeInfo> {
    let mut reachable_types: BTreeSet<String> = BTreeSet::new();
    let mut all_defined_types: BTreeMap<String, (String, String)> = BTreeMap::new();
    let mut type_definitions: BTreeMap<String, &ClassData> = BTreeMap::new();

    // First pass: collect all type definitions, EXCLUDING removed types
    for (version_name, version_data) in &api_data.0 {
        for (module_name, module_data) in &version_data.api {
            for (class_name, class_data) in &module_data.classes {
                // Skip types that have been "removed"
                if removed.contains(class_name) {
                    continue;
                }

                let has_body = class_data.struct_fields.is_some()
                    || class_data.enum_fields.is_some()
                    || class_data.callback_typedef.is_some()
                    || class_data.type_alias.is_some()
                    || class_data.functions.is_some()
                    || class_data.constructors.is_some();

                all_defined_types.insert(
                    class_name.clone(),
                    (module_name.clone(), version_name.clone()),
                );

                let existing = type_definitions.get(class_name);
                let should_replace = match existing {
                    None => true,
                    Some(existing_data) => {
                        let existing_has_body = existing_data.struct_fields.is_some()
                            || existing_data.enum_fields.is_some()
                            || existing_data.callback_typedef.is_some()
                            || existing_data.type_alias.is_some();
                        !existing_has_body && has_body
                    }
                };

                if should_replace {
                    type_definitions.insert(class_name.clone(), class_data);
                }
            }
        }
    }

    // Helper closure to check if a type is valid (exists and not removed)
    let is_valid_type = |type_name: &String| -> bool {
        !removed.contains(type_name) && all_defined_types.contains_key(type_name)
    };

    // Phase 1: Collect entry points (functions, constructors, callbacks)
    let mut types_to_process: Vec<String> = Vec::new();

    for (_version_name, version_data) in &api_data.0 {
        for (_module_name, module_data) in &version_data.api {
            for (class_name, class_data) in &module_data.classes {
                // Skip removed types
                if removed.contains(class_name) {
                    continue;
                }

                let has_functions = class_data
                    .functions
                    .as_ref()
                    .map(|f| !f.is_empty())
                    .unwrap_or(false);
                let has_constructors = class_data
                    .constructors
                    .as_ref()
                    .map(|c| !c.is_empty())
                    .unwrap_or(false);
                let is_callback_typedef = class_data.callback_typedef.is_some();

                // If this type has functions/constructors/callbacks, it's an entry point
                if has_functions || has_constructors || is_callback_typedef {
                    if !reachable_types.contains(class_name) {
                        reachable_types.insert(class_name.clone());
                        types_to_process.push(class_name.clone());
                    }
                }

                // Also mark types referenced by functions/constructors as reachable
                if let Some(functions) = &class_data.functions {
                    for (_fn_name, fn_data) in functions {
                        for type_name in extract_types_from_function_data(fn_data) {
                            if !reachable_types.contains(&type_name) && is_valid_type(&type_name) {
                                reachable_types.insert(type_name.clone());
                                types_to_process.push(type_name);
                            }
                        }
                    }
                }

                if let Some(constructors) = &class_data.constructors {
                    for (_ctor_name, ctor_data) in constructors {
                        for type_name in extract_types_from_function_data(ctor_data) {
                            if !reachable_types.contains(&type_name) && is_valid_type(&type_name) {
                                reachable_types.insert(type_name.clone());
                                types_to_process.push(type_name);
                            }
                        }
                    }
                }

                if let Some(callback_def) = &class_data.callback_typedef {
                    for type_name in extract_types_from_callback_definition(callback_def) {
                        if !reachable_types.contains(&type_name) && is_valid_type(&type_name) {
                            reachable_types.insert(type_name.clone());
                            types_to_process.push(type_name);
                        }
                    }
                }
            }
        }
    }

    // Phase 2: Recursively follow struct fields and enum variants
    // Use _all variants to include types behind pointers (important for Vec element types)
    let mut iteration = 0;
    let max_iterations = 100;

    while !types_to_process.is_empty() && iteration < max_iterations {
        iteration += 1;
        let current_batch: Vec<String> = types_to_process.drain(..).collect();

        for type_name in current_batch {
            if let Some(class_data) = type_definitions.get(&type_name) {
                // Process struct fields (including pointer types)
                if let Some(struct_fields) = &class_data.struct_fields {
                    for field_map in struct_fields {
                        for (_field_name, field_data) in field_map {
                            for referenced_type in extract_types_from_field_data_all(field_data) {
                                if !reachable_types.contains(&referenced_type)
                                    && is_valid_type(&referenced_type)
                                {
                                    reachable_types.insert(referenced_type.clone());
                                    types_to_process.push(referenced_type);
                                }
                            }
                        }
                    }
                }

                // Process enum variants (including pointer types)
                if let Some(enum_fields) = &class_data.enum_fields {
                    for variant_map in enum_fields {
                        for (_variant_name, variant_data) in variant_map {
                            for referenced_type in extract_types_from_enum_variant_all(variant_data)
                            {
                                if !reachable_types.contains(&referenced_type)
                                    && is_valid_type(&referenced_type)
                                {
                                    reachable_types.insert(referenced_type.clone());
                                    types_to_process.push(referenced_type);
                                }
                            }
                        }
                    }
                }

                // Process type aliases (including pointer types)
                if let Some(type_alias) = &class_data.type_alias {
                    if let Some(base_type) =
                        extract_base_type_including_pointers(&type_alias.target)
                    {
                        if !reachable_types.contains(&base_type) && is_valid_type(&base_type) {
                            reachable_types.insert(base_type.clone());
                            types_to_process.push(base_type);
                        }
                    }
                    for generic_arg in &type_alias.generic_args {
                        if let Some(base_type) = extract_base_type_including_pointers(generic_arg) {
                            if !reachable_types.contains(&base_type) && is_valid_type(&base_type) {
                                reachable_types.insert(base_type.clone());
                                types_to_process.push(base_type);
                            }
                        }
                    }
                }
            }
        }
    }

    // Phase 3: Find unreachable types (types that are defined but not reachable)
    let mut unused: Vec<UnusedTypeInfo> = Vec::new();

    for (type_name, (module_name, version_name)) in &all_defined_types {
        if !reachable_types.contains(type_name) {
            unused.push(UnusedTypeInfo {
                type_name: type_name.clone(),
                module_name: module_name.clone(),
                version_name: version_name.clone(),
            });
        }
    }

    unused.sort_by(|a, b| match a.module_name.cmp(&b.module_name) {
        std::cmp::Ordering::Equal => a.type_name.cmp(&b.type_name),
        other => other,
    });

    unused
}

/// Generate removal patches for unused types
///
/// Creates patch files that will remove unused types from the API when applied.
/// One patch file is created per module, containing all removal operations for that module.
pub fn generate_removal_patches(unused_types: &[UnusedTypeInfo]) -> Vec<crate::patch::ApiPatch> {
    use std::collections::BTreeMap;

    use crate::patch::{ApiPatch, ClassPatch, ModulePatch, VersionPatch};

    // Group unused types by version and module
    let mut grouped: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();

    for unused in unused_types {
        grouped
            .entry(unused.version_name.clone())
            .or_default()
            .entry(unused.module_name.clone())
            .or_default()
            .push(unused.type_name.clone());
    }

    // Create one patch per module
    let mut patches = Vec::new();

    for (version_name, modules) in grouped {
        for (module_name, type_names) in modules {
            let mut module_patch = ModulePatch::default();

            for type_name in type_names {
                module_patch.classes.insert(
                    type_name,
                    ClassPatch {
                        remove: Some(true),
                        ..Default::default()
                    },
                );
            }

            let mut version_patch = VersionPatch::default();
            version_patch
                .modules
                .insert(module_name.clone(), module_patch);

            let mut api_patch = ApiPatch::default();
            api_patch
                .versions
                .insert(version_name.clone(), version_patch);

            patches.push(api_patch);
        }
    }

    patches
}

/// Remove empty modules from the API data
///
/// Returns the number of modules removed
pub fn remove_empty_modules(api_data: &mut ApiData) -> usize {
    let mut total_removed = 0;

    for (_version_name, version_data) in &mut api_data.0 {
        let empty_modules: Vec<String> = version_data
            .api
            .iter()
            .filter(|(_, module_data)| module_data.classes.is_empty())
            .map(|(module_name, _)| module_name.clone())
            .collect();

        for module_name in &empty_modules {
            version_data.api.shift_remove(module_name);
            println!("  [REMOVE] Removed empty module: {}", module_name);
            total_removed += 1;
        }
    }

    total_removed
}

/// Information about an unused type in the API
#[derive(Debug, Clone)]
pub struct UnusedTypeInfo {
    pub type_name: String,
    pub module_name: String,
    pub version_name: String,
}

/// Extract array size from a type string like "[T; N]" and normalize it.
/// Returns (base_type, array_size) where array_size is Some(N) if it's an array.
///
/// Examples:
/// - "[f32; 20]" -> ("f32", Some(20))
/// - "[FloatValue; 4]" -> ("FloatValue", Some(4))
/// - "String" -> ("String", None)
fn extract_array_info(type_str: &str) -> (String, Option<usize>) {
    let trimmed = type_str.trim();

    // Check if it's an array type: [T; N]
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = &trimmed[1..trimmed.len() - 1];
        if let Some(semicolon_pos) = inner.rfind(';') {
            let base_type = inner[..semicolon_pos].trim().to_string();
            let size_str = inner[semicolon_pos + 1..].trim();
            if let Ok(size) = size_str.parse::<usize>() {
                return (base_type, Some(size));
            }
        }
    }

    (trimmed.to_string(), None)
}

/// Normalize array types in api.json by extracting `[T; N]` into separate fields.
///
/// Before: `{ "type": "[f32; 20]" }`
/// After:  `{ "type": "f32", "arraysize": 20 }`
///
/// This should be run after syn parsing to simplify downstream code generation.
pub fn normalize_array_types(api_data: &mut ApiData) -> usize {
    let mut count = 0;

    for (_version_name, version_data) in &mut api_data.0 {
        for (_module_name, module_data) in &mut version_data.api {
            for (_class_name, class_data) in &mut module_data.classes {
                // Process struct fields
                if let Some(struct_fields) = &mut class_data.struct_fields {
                    for field_map in struct_fields {
                        for (_field_name, field_data) in field_map {
                            if field_data.arraysize.is_none() {
                                let (base_type, array_size) =
                                    extract_array_info(&field_data.r#type);
                                if let Some(size) = array_size {
                                    field_data.r#type = base_type;
                                    field_data.arraysize = Some(size);
                                    count += 1;
                                }
                            }
                        }
                    }
                }

                // Process enum variant types (keep type as-is for now since no arraysize field)
                // Arrays in enum variants are rare and can be handled manually if needed
            }
        }
    }

    count
}

/// Normalize type_alias entries in api.json by extracting pointer types into ref_kind.
///
/// Before: `{ "target": "*mut c_void", "generic_args": [] }`
/// After:  `{ "target": "c_void", "ref_kind": "mutptr" }`
///
/// Before: `{ "target": "PhysicalPosition<i32>" }`
/// After:  `{ "target": "PhysicalPosition", "generic_args": ["i32"] }`
///
/// This ensures:
/// 1. Pointer prefixes (*const, *mut) are extracted to ref_kind field
/// 2. Embedded generics (e.g., Foo<Bar>) are extracted to generic_args
/// 3. Empty generic_args are removed (via serde skip_serializing_if)
pub fn normalize_type_aliases(api_data: &mut ApiData) -> usize {
    use crate::autofix::types::ref_kind::RefKind;

    let mut count = 0;

    for (_version_name, version_data) in &mut api_data.0 {
        for (_module_name, module_data) in &mut version_data.api {
            for (_class_name, class_data) in &mut module_data.classes {
                if let Some(type_alias) = &mut class_data.type_alias {
                    let target = &type_alias.target;
                    let mut modified = false;

                    // Extract pointer prefix from target
                    let (new_target, new_ref_kind) = if target.starts_with("*const ") {
                        modified = true;
                        (
                            target.strip_prefix("*const ").unwrap().trim().to_string(),
                            Some(RefKind::ConstPtr),
                        )
                    } else if target.starts_with("*mut ") {
                        modified = true;
                        (
                            target.strip_prefix("*mut ").unwrap().trim().to_string(),
                            Some(RefKind::MutPtr),
                        )
                    } else if target.starts_with("* const ") {
                        modified = true;
                        (
                            target.strip_prefix("* const ").unwrap().trim().to_string(),
                            Some(RefKind::ConstPtr),
                        )
                    } else if target.starts_with("* mut ") {
                        modified = true;
                        (
                            target.strip_prefix("* mut ").unwrap().trim().to_string(),
                            Some(RefKind::MutPtr),
                        )
                    } else {
                        (target.clone(), None)
                    };

                    // Extract embedded generics if generic_args is empty
                    let (final_target, extracted_generic_args) =
                        if type_alias.generic_args.is_empty() {
                            if let Some(open_idx) = new_target.find('<') {
                                if let Some(close_idx) = new_target.rfind('>') {
                                    let base = new_target[..open_idx].trim().to_string();
                                    let args_str = &new_target[open_idx + 1..close_idx];
                                    let args: Vec<String> = args_str
                                        .split(',')
                                        .map(|s| s.trim().to_string())
                                        .filter(|s| !s.is_empty())
                                        .collect();
                                    if !args.is_empty() {
                                        modified = true;
                                        (base, args)
                                    } else {
                                        (new_target, vec![])
                                    }
                                } else {
                                    (new_target, vec![])
                                }
                            } else {
                                (new_target, vec![])
                            }
                        } else {
                            (new_target, vec![])
                        };

                    if modified {
                        type_alias.target = final_target;
                        if let Some(rk) = new_ref_kind {
                            if type_alias.ref_kind == RefKind::Value {
                                type_alias.ref_kind = rk;
                            }
                        }
                        if !extracted_generic_args.is_empty() {
                            type_alias.generic_args = extracted_generic_args;
                        }
                        count += 1;
                    }
                }
            }
        }
    }

    count
}

/// Normalize enum variant types in api.json by extracting pointer prefixes into ref_kind.
///
/// Autofix generates enum variant types with raw Rust pointer syntax like `"*mut T"` or `"&T"`.
/// This function extracts those into the `ref_kind` field on `EnumVariantData`.
///
/// Before: `{ "type": "*mut T" }` → After: `{ "type": "T", "ref_kind": "mutptr" }`
/// Before: `{ "type": "&T" }`     → After: `{ "type": "T", "ref_kind": "constptr" }`
/// Before: `{ "type": "*const T" }` → After: `{ "type": "T", "ref_kind": "constptr" }`
pub fn normalize_enum_variant_types(api_data: &mut ApiData) -> usize {
    use crate::autofix::types::ref_kind::RefKind;

    let mut count = 0;

    for (_version_name, version_data) in &mut api_data.0 {
        for (_module_name, module_data) in &mut version_data.api {
            for (_class_name, class_data) in &mut module_data.classes {
                if let Some(enum_fields) = &mut class_data.enum_fields {
                    for variant_map in enum_fields.iter_mut() {
                        for (_variant_name, variant_data) in variant_map.iter_mut() {
                            if let Some(ref mut type_str) = variant_data.r#type {
                                let trimmed = type_str.trim();
                                let (new_type, new_ref_kind) = if trimmed.starts_with("*mut ") {
                                    (
                                        trimmed.strip_prefix("*mut ").unwrap().trim().to_string(),
                                        Some(RefKind::MutPtr),
                                    )
                                } else if trimmed.starts_with("*const ") {
                                    (
                                        trimmed.strip_prefix("*const ").unwrap().trim().to_string(),
                                        Some(RefKind::ConstPtr),
                                    )
                                } else if trimmed.starts_with("&mut ") {
                                    (
                                        trimmed.strip_prefix("&mut ").unwrap().trim().to_string(),
                                        Some(RefKind::RefMut),
                                    )
                                } else if trimmed.starts_with("&") && !trimmed.starts_with("&mut") {
                                    (
                                        trimmed.strip_prefix("&").unwrap().trim().to_string(),
                                        Some(RefKind::ConstPtr),
                                    )
                                } else {
                                    continue;
                                };

                                *type_str = new_type;
                                if let Some(rk) = new_ref_kind {
                                    if variant_data.ref_kind == RefKind::Value {
                                        variant_data.ref_kind = rk;
                                    }
                                }
                                count += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_base_type_simple() {
        assert_eq!(extract_base_type("Foo"), "Foo");
        assert_eq!(extract_base_type("  Bar  "), "Bar");
    }

    #[test]
    fn test_extract_base_type_generic() {
        assert_eq!(extract_base_type("Vec<Foo>"), "Foo");
        assert_eq!(extract_base_type("Option<Bar>"), "Bar");
        assert_eq!(extract_base_type("Box<Baz>"), "Baz");
    }

    #[test]
    fn test_extract_base_type_nested() {
        assert_eq!(extract_base_type("Vec<Option<Foo>>"), "Foo");
        assert_eq!(extract_base_type("Option<Box<Bar>>"), "Bar");
    }

    #[test]
    fn test_extract_base_type_pointers() {
        assert_eq!(extract_base_type("*const Foo"), "Foo");
        assert_eq!(extract_base_type("*mut Bar"), "Bar");
        assert_eq!(extract_base_type("&Baz"), "Baz");
        assert_eq!(extract_base_type("&mut Qux"), "Qux");
    }

    #[test]
    fn test_extract_base_type_complex() {
        assert_eq!(extract_base_type("*const Vec<Foo>"), "Foo");
        assert_eq!(extract_base_type("&Option<Bar>"), "Bar");
    }
}
