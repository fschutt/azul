//! Classify Azul C API functions from api.json.
//!
//! Each function in the API is classified into one of:
//! - **Framework**: Needed in azul-mini.wasm (DOM, layout, CSS, hit-testing)
//! - **ServerEntryPoint**: Excluded from WASM (AzApp_run, event loop)
//! - **ReplaceWithDomPatcher**: GPU/display-list fns replaced with setStyle() emitters
//! - **UserCallback**: Discovered from DOM tree, lifted into separate .wasm files
//!
//! In Phase 0, this is stubbed — no api.json is parsed. The classification
//! returns empty results. When remill is integrated, this will drive which
//! functions get lifted into azul-mini.wasm vs. replaced with shims.

/// How a C API function should be treated when generating WASM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FnClass {
    /// Framework fn needed in WASM (AzDom_addChild, AzRefAny_clone, ...)
    Framework,
    /// Server entry point — NOT included in WASM (AzApp_run, etc.)
    ServerEntryPoint,
    /// Display list / GPU path — replaced with setStyle() emitters
    ReplaceWithDomPatcher,
}

/// Result of classifying all API functions.
pub struct ApiClassification {
    pub functions: Vec<(String, FnClass)>,
}

impl ApiClassification {
    pub fn total(&self) -> usize {
        self.functions.len()
    }

    pub fn framework_count(&self) -> usize {
        self.functions.iter().filter(|(_, c)| *c == FnClass::Framework).count()
    }

    pub fn excluded_count(&self) -> usize {
        self.functions.iter().filter(|(_, c)| *c != FnClass::Framework).count()
    }
}

/// Classify all API functions. Stubbed in Phase 0.
///
/// In the future, this will:
/// 1. Decompress the embedded api.json (brotli, from include_bytes!)
/// 2. Parse every function entry
/// 3. Classify based on name prefix and signature
pub fn classify_api_functions() -> ApiClassification {
    // Phase 0 stub: return empty classification.
    // When api.json parsing is implemented, this will populate the list.
    ApiClassification {
        functions: Vec::new(),
    }
}
