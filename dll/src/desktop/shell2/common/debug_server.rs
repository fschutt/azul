//! HTTP Debug Server for Azul
//!
//! This module provides an HTTP debug server that integrates with Azul's timer system
//! for cross-platform automated testing and debugging.
//!
//! ## Architecture
//!
//! The debug server is started in `App::create()` and runs on a background thread.
//! It accepts JSON commands on "/" and forwards them to the timer callback for
//! cross-platform processing via CallbackInfo.
//!
//! ## Usage
//!
//! ```bash
//! # Start app with debug server
//! AZUL_DEBUG=8765 cargo run --bin my_app
//!
//! # Send events (blocks until processed)
//! curl -X POST http://localhost:8765/ -d '{"type":"get_state"}'
//! ```

use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

// Import the NativeScreenshotExt trait for native screenshots
use crate::desktop::native_screenshot::NativeScreenshotExt;

#[cfg(feature = "std")]
use std::sync::{mpsc, Arc, Mutex, OnceLock};

// ==================== Types ====================

/// Request from HTTP thread to timer callback
#[cfg(feature = "std")]
pub struct DebugRequest {
    pub request_id: u64,
    pub event: DebugEvent,
    pub window_id: Option<String>,
    pub wait_for_render: bool,
    pub response_tx: mpsc::Sender<DebugResponseData>,
}

/// Response data from timer callback to HTTP thread (internal)
#[cfg(feature = "std")]
#[derive(Debug, Clone)]
pub enum DebugResponseData {
    /// Successful response with optional data
    Ok {
        window_state: Option<WindowStateSnapshot>,
        data: Option<ResponseData>,
    },
    /// Error response
    Err(String),
}

/// Typed response data variants
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ResponseData {
    /// Screenshot data (base64 encoded PNG)
    Screenshot(ScreenshotData),
    /// Node CSS properties
    NodeCssProperties(NodeCssPropertiesResponse),
    /// Node layout
    NodeLayout(NodeLayoutResponse),
    /// All nodes layout
    AllNodesLayout(AllNodesLayoutResponse),
    /// DOM tree
    DomTree(DomTreeResponse),
    /// Node hierarchy
    NodeHierarchy(NodeHierarchyResponse),
    /// Layout tree
    LayoutTree(LayoutTreeResponse),
    /// Display list
    DisplayList(DisplayListResponse),
    /// Scroll states
    ScrollStates(ScrollStatesResponse),
    /// Scrollable nodes
    ScrollableNodes(ScrollableNodesResponse),
    /// Scroll node by delta result
    ScrollNodeBy(ScrollNodeByResponse),
    /// Scroll node to position result
    ScrollNodeTo(ScrollNodeToResponse),
    /// Scroll node into view result
    ScrollIntoView(ScrollIntoViewResponse),
    /// Hit test result
    HitTest(HitTestResponse),
    /// HTML string
    HtmlString(HtmlStringResponse),
    /// Log messages
    Logs(LogsResponse),
    /// Health check
    Health(HealthResponse),
    /// Find node result
    FindNode(FindNodeResponse),
    /// Click node result
    ClickNode(ClickNodeResponse),
    /// Scrollbar info result
    ScrollbarInfo(ScrollbarInfoResponse),
    /// Selection state result
    SelectionState(SelectionStateResponse),
    /// Full selection manager dump
    SelectionManagerDump(SelectionManagerDump),
    /// App state as JSON
    AppState(AppStateResponse),
    /// App state set result
    AppStateSet(AppStateSetResponse),
    /// Drag state from unified drag system
    DragState(DragStateResponse),
    /// Detailed drag context
    DragContext(DragContextResponse),
    /// Focus state (which node has keyboard focus)
    FocusState(FocusStateResponse),
    /// Cursor state (cursor position and blink state)
    CursorState(CursorStateResponse),
    /// E2E test results
    E2eResults(E2eResultsResponse),
    /// Node inserted result (returns new node_id)
    NodeInserted(NodeInsertedResponse),
    /// Node deleted result
    NodeDeleted(NodeDeletedResponse),
    /// Node text set result
    NodeTextSet(NodeTextSetResponse),
    /// Node classes set result
    NodeClassesSet(NodeClassesSetResponse),
    /// Node CSS override result
    NodeCssOverrideSet(NodeCssOverrideSetResponse),
    /// Resolved function pointer names
    FunctionPointers(FunctionPointersResponse),
    /// Component registry (available tags and their attributes)
    ComponentRegistry(ComponentRegistryResponse),
    /// Library list (lightweight summary)
    Libraries(LibraryListResponse),
    /// Components within a specific library
    LibraryComponents(LibraryComponentsResponse),
    /// Exported code (compiled project files)
    ExportedCode(ExportedCodeResponse),
    /// Component library imported successfully
    ImportedLibrary(ImportedLibraryResponse),
    /// Component library exported as JSON
    ExportedLibrary(ExportedLibraryResponse),
}

/// Wrapper for E2E test results
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct E2eResultsResponse {
    pub results: Vec<E2eTestResult>,
}

/// Response for InsertNode: returns the new node's ID
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeInsertedResponse {
    pub new_node_id: u64,
    pub parent_id: u64,
    pub node_type: String,
}

/// Response for DeleteNode
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct NodeDeletedResponse {
    pub node_id: u64,
    pub success: bool,
}

/// Response for SetNodeText
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeTextSetResponse {
    pub node_id: u64,
    pub new_text: String,
}

/// Response for SetNodeClasses
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeClassesSetResponse {
    pub node_id: u64,
    pub classes: Vec<String>,
    pub id: Option<String>,
}

/// Response for SetNodeCssOverride
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeCssOverrideSetResponse {
    pub node_id: u64,
    pub property: String,
    pub value: String,
}

/// Response for ResolveFunctionPointers
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct FunctionPointersResponse {
    pub resolved: Vec<ResolvedFunctionPointer>,
}

/// A resolved function pointer
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ResolvedFunctionPointer {
    pub address: String,
    pub symbol_name: Option<String>,
    /// The shared library / binary file that contains this symbol
    pub file_name: Option<String>,
    /// Source file path (if resolved via backtrace or heuristic)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_file: Option<String>,
    /// Source line number (if resolved via backtrace)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_line: Option<u32>,
    /// Human-readable hint about resolution quality
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Whether the resolved info is approximate (heuristic-based)
    #[serde(default)]
    pub approximate: bool,
}

/// Response for ExportCode: compiled project as a base64-encoded ZIP
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ExportedCodeResponse {
    /// Target language used
    pub language: String,
    /// Map of filename → file content (source code)
    pub files: std::collections::HashMap<String, String>,
    /// Any warnings or notes from compilation
    pub warnings: Vec<String>,
}

/// Response for GetComponentRegistry
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ComponentRegistryResponse {
    /// Libraries of components, grouped by collection name
    pub libraries: Vec<ComponentLibraryInfo>,
}

/// A library / collection of components
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ComponentLibraryInfo {
    /// Library name (e.g., "builtin", "shadcn", "myproject")
    pub name: String,
    /// Library version
    pub version: String,
    /// Human-readable description
    pub description: String,
    /// Whether this library can be exported
    pub exportable: bool,
    /// Whether this library can be modified (add/remove/edit components)
    pub modifiable: bool,
    /// Named data model types defined by this library
    pub data_models: Vec<DataModelInfo>,
    /// Components in this library
    pub components: Vec<ComponentInfo>,
}

/// Information about a registered component/tag
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ComponentInfo {
    /// The tag name (e.g., "div", "a", "button")
    pub tag: String,
    /// The qualified name (e.g., "builtin:div", "shadcn:avatar")
    pub qualified_name: String,
    /// Display name for the GUI (e.g., "Link" for "a", "Avatar")
    pub display_name: String,
    /// Description / documentation
    pub description: String,
    /// Whether this tag accepts text content
    pub accepts_text: bool,
    /// Child policy: "no_children", "any_children", "text_only"
    pub child_policy: String,
    /// Source: "builtin", "compiled", "user_defined"
    pub source: String,
    /// Component-specific data model fields (the component's own attributes,
    /// e.g., href/target/rel for <a>). These ARE the component's main data model.
    pub data_model: Vec<ComponentDataFieldInfo>,
    /// Universal HTML attributes (id, class, style, etc.)
    /// Shown in a collapsed section in the debugger.
    pub universal_attributes: Vec<ComponentAttributeInfo>,
    /// Callback slots this component exposes
    pub callback_slots: Vec<ComponentCallbackSlotInfo>,
    /// Example XML usage
    pub example_xml: String,
    /// Scoped CSS
    pub scoped_css: String,
    /// XML template body (for user-defined components)
    pub template: String,
}

/// Info about an attribute a component accepts
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ComponentAttributeInfo {
    /// Attribute name (e.g., "href", "src", "alt")
    pub name: String,
    /// Attribute type hint (e.g., "String", "bool", "i32")
    pub attr_type: String,
    /// Default value, if any
    pub default: Option<String>,
    /// Description
    pub description: String,
}

/// Info about a callback slot
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ComponentCallbackSlotInfo {
    /// Slot name (e.g., "on_click", "on_value_change")
    pub name: String,
    /// Callback type name (e.g., "ButtonOnClickCallbackType")
    pub callback_type: String,
    /// Description
    pub description: String,
}

/// Info about a data model field
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ComponentDataFieldInfo {
    /// Field name
    pub name: String,
    /// Field type
    pub field_type: String,
    /// Default value
    pub default: Option<String>,
    /// Description
    pub description: String,
}

/// Response for GetLibraries — list of registered component libraries (without component details)
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct LibraryListResponse {
    pub libraries: Vec<LibrarySummary>,
}

/// Summary info for a library (no component details)
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct LibrarySummary {
    pub name: String,
    pub version: String,
    pub description: String,
    pub exportable: bool,
    pub modifiable: bool,
    pub component_count: usize,
}

/// A named data model (struct definition) in a library
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct DataModelInfo {
    /// Type name, e.g. "UserProfile"
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// Fields in this struct
    pub fields: Vec<ComponentDataFieldInfo>,
}

/// Response for GetLibraryComponents — components within a specific library
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct LibraryComponentsResponse {
    pub library: String,
    pub components: Vec<ComponentInfo>,
}

/// Response for ImportComponentLibrary
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ImportedLibraryResponse {
    /// Name of the imported library
    pub library_name: String,
    /// Number of components imported
    pub component_count: usize,
    /// Whether this was an update (true) or new addition (false)
    pub was_update: bool,
}

/// Response for ExportComponentLibrary — JSON-serializable library definition
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExportedLibraryResponse {
    /// Library name
    pub name: String,
    /// Library version
    pub version: String,
    /// Library description
    pub description: String,
    /// Component definitions (JSON-serializable subset)
    pub components: Vec<ExportedComponentDef>,
}

/// A component definition in JSON-serializable form (for import/export).
/// Uses a unified `fields` list instead of separate parameters/data_fields/callback_slots.
/// Callbacks are fields with type "Callback(...)", struct refs use "struct:Name", etc.
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExportedComponentDef {
    /// Component name (without collection prefix)
    pub name: String,
    /// Human-readable display name
    pub display_name: String,
    /// Markdown description
    #[serde(default)]
    pub description: String,
    /// Unified data model fields (data fields, callbacks, parameters — all in one list).
    /// Callbacks have type "Callback(...)", regular fields have type "String", "bool", etc.
    #[serde(default)]
    pub fields: Vec<ExportedDataField>,
    /// Whether this component accepts text content
    #[serde(default)]
    pub accepts_text: bool,
    /// Child policy: "no_children", "any_children", "text_only"
    #[serde(default = "default_child_policy")]
    pub child_policy: String,
    /// Scoped CSS for the component
    #[serde(default)]
    pub scoped_css: String,
    /// Example XML usage
    #[serde(default)]
    pub example_xml: String,
    /// XML template body (for rendering)
    #[serde(default)]
    pub template: String,
}

#[cfg(feature = "std")]
fn default_child_policy() -> String { "any_children".to_string() }

#[cfg(feature = "std")]
fn default_param_type() -> String { "String".to_string() }

/// A data model field in JSON form (unified: data fields, callbacks, parameters).
/// For callbacks, set `type` to "Callback(...)" or "Callback".
/// For struct/enum references, use "struct:TypeName" or "enum:TypeName".
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExportedDataField {
    /// Field name (must be a valid identifier)
    pub name: String,
    /// Field type string: "String", "bool", "i32", "Callback(...)", "Option<i32>", etc.
    #[serde(rename = "type", default = "default_param_type")]
    pub field_type: String,
    /// Default value as string (parsed according to field type)
    #[serde(default)]
    pub default: Option<String>,
    /// Human-readable description
    #[serde(default)]
    pub description: String,
}

/// Metadata about a RefAny's type
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct RefAnyMetadata {
    /// The compiler-generated type ID
    pub type_id: u64,
    /// Human-readable type name (e.g., "app::MyStruct")
    pub type_name: String,
    /// Whether this RefAny supports JSON serialization
    pub can_serialize: bool,
    /// Whether this RefAny type supports JSON deserialization
    pub can_deserialize: bool,
    /// Number of active references to this data
    pub ref_count: usize,
}

/// Error information for RefAny operations
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "error_type", content = "message", rename_all = "snake_case")]
pub enum RefAnyError {
    /// Type does not support JSON serialization
    NotSerializable,
    /// Type does not support JSON deserialization
    NotDeserializable,
    /// Serde serialization/deserialization failed
    SerdeError(String),
    /// Valid JSON but cannot construct RefAny (type mismatch, missing fields, etc.)
    TypeConstructionError(String),
}

/// App state response (JSON serialized) with full metadata
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct AppStateResponse {
    /// Metadata about the RefAny type
    pub metadata: RefAnyMetadata,
    /// The serialized JSON data (null if serialization failed or not supported)
    pub state: serde_json::Value,
    /// Error message if serialization failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RefAnyError>,
}

/// App state set result
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct AppStateSetResponse {
    /// Whether the operation succeeded
    pub success: bool,
    /// Optional error details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RefAnyError>,
}

/// Screenshot response data
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScreenshotData {
    /// Base64 encoded PNG with data URI prefix
    pub data: String,
}

/// Hit test response
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct HitTestResponse {
    pub x: f32,
    pub y: f32,
    pub node_id: Option<u64>,
    pub node_tag: Option<String>,
}

/// Find node response - returns location and size of found node
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct FindNodeResponse {
    pub found: bool,
    pub node_id: Option<u64>,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub tag: Option<String>,
    pub classes: Option<Vec<String>>,
}

/// Click node response
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ClickNodeResponse {
    pub success: bool,
    pub message: String,
}

/// HTML string response
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct HtmlStringResponse {
    pub html: String,
}

/// Logs response
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct LogsResponse {
    pub logs: Vec<LogMessage>,
}

/// Health check response
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct HealthResponse {
    pub port: u16,
    pub pending_logs: usize,
    pub logs: Vec<LogMessageJson>,
}

/// JSON-friendly log message
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct LogMessageJson {
    pub timestamp_us: u64,
    pub level: String,
    pub category: String,
    pub message: String,
}

/// HTTP response wrapper for serialization
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "status")]
pub enum DebugHttpResponse {
    #[serde(rename = "ok")]
    Ok(DebugHttpResponseOk),
    #[serde(rename = "error")]
    Error(DebugHttpResponseError),
}

/// Successful HTTP response body
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct DebugHttpResponseOk {
    pub request_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_state: Option<WindowStateSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<ResponseData>,
}

/// Error HTTP response body
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct DebugHttpResponseError {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<u64>,
    pub message: String,
}

/// A log message
#[derive(Debug, Clone)]
pub struct LogMessage {
    pub timestamp_us: u64,
    pub level: LogLevel,
    pub category: LogCategory,
    pub message: String,
    pub location: String,
    pub window_id: Option<String>,
}

#[cfg(feature = "std")]
impl serde::Serialize for LogMessage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("LogMessage", 6)?;
        s.serialize_field("timestamp_us", &self.timestamp_us)?;
        s.serialize_field("level", &format!("{:?}", self.level))?;
        s.serialize_field("category", &format!("{:?}", self.category))?;
        s.serialize_field("message", &self.message)?;
        s.serialize_field("location", &self.location)?;
        s.serialize_field("window_id", &self.window_id)?;
        s.end()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogCategory {
    General,
    Window,
    EventLoop,
    Input,
    Layout,
    Text,
    DisplayList,
    Rendering,
    Resources,
    Callbacks,
    Timer,
    DebugServer,
    Platform,
}

/// Snapshot of window state for response
#[derive(Debug, Clone)]
pub struct WindowStateSnapshot {
    pub window_id: String,
    pub logical_width: f32,
    pub logical_height: f32,
    pub physical_width: u32,
    pub physical_height: u32,
    pub dpi: u32,
    pub hidpi_factor: f32,
    pub focused: bool,
    pub dom_node_count: usize,
    pub focused_node: Option<u64>,
}

#[cfg(feature = "std")]
impl serde::Serialize for WindowStateSnapshot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("WindowStateSnapshot", 10)?;
        s.serialize_field("window_id", &self.window_id)?;
        s.serialize_field("logical_width", &self.logical_width)?;
        s.serialize_field("logical_height", &self.logical_height)?;
        s.serialize_field("physical_width", &self.physical_width)?;
        s.serialize_field("physical_height", &self.physical_height)?;
        s.serialize_field("dpi", &self.dpi)?;
        s.serialize_field("hidpi_factor", &self.hidpi_factor)?;
        s.serialize_field("focused", &self.focused)?;
        s.serialize_field("dom_node_count", &self.dom_node_count)?;
        s.serialize_field("focused_node", &self.focused_node)?;
        s.end()
    }
}

// ==================== Response Data Structures ====================

/// Response for GetNodeCssProperties
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeCssPropertiesResponse {
    pub node_id: u64,
    pub property_count: usize,
    pub properties: Vec<String>,
}

/// Response for GetNodeLayout
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct NodeLayoutResponse {
    pub node_id: u64,
    pub size: Option<LogicalSizeJson>,
    pub position: Option<LogicalPositionJson>,
    pub rect: Option<LogicalRectJson>,
}

/// Response for GetAllNodesLayout
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct AllNodesLayoutResponse {
    pub dom_id: u32,
    pub node_count: usize,
    pub nodes: Vec<NodeLayoutInfo>,
}

/// Layout info for a single node
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeLayoutInfo {
    pub node_id: usize,
    pub tag: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub rect: Option<LogicalRectJson>,
}

/// Response for GetDomTree
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct DomTreeResponse {
    pub dom_id: u32,
    pub node_count: usize,
    pub dpi: u32,
    pub hidpi_factor: f32,
    pub logical_width: f32,
    pub logical_height: f32,
}

/// Response for GetNodeHierarchy
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeHierarchyResponse {
    pub root: i64,
    pub node_count: usize,
    pub nodes: Vec<HierarchyNodeInfo>,
}

/// Hierarchy info for a single node
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct HierarchyNodeInfo {
    pub index: usize,
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub classes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub parent: i64,
    pub children: Vec<usize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<NodeEventInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rect: Option<LogicalRectJson>,
    pub tab_index: Option<i32>,
    pub contenteditable: bool,
}

/// Event handler info for a single callback on a node
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeEventInfo {
    pub event: String,
    pub callback_ptr: String,
}

/// Response for GetLayoutTree
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct LayoutTreeResponse {
    pub root: usize,
    pub node_count: usize,
    pub nodes: Vec<LayoutNodeInfo>,
}

/// Layout tree info for a single node
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct LayoutNodeInfo {
    pub layout_idx: usize,
    pub dom_idx: i64,
    #[serde(rename = "type")]
    pub node_type: String,
    pub is_anonymous: bool,
    pub anonymous_type: Option<String>,
    pub formatting_context: String,
    pub parent: i64,
    pub children: Vec<usize>,
}

/// Response for GetDisplayList
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct DisplayListResponse {
    pub total_items: usize,
    pub rect_count: usize,
    pub text_count: usize,
    pub border_count: usize,
    pub image_count: usize,
    pub other_count: usize,
    pub items: Vec<DisplayListItemInfo>,
    /// Clip chain analysis - shows push/pop balance
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip_analysis: Option<ClipChainAnalysis>,
}

/// Clip chain analysis for debugging clipping issues
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ClipChainAnalysis {
    /// Final clip depth (should be 0 if balanced)
    pub final_clip_depth: i32,
    /// Final scroll depth (should be 0 if balanced)
    pub final_scroll_depth: i32,
    /// Final stacking context depth (should be 0 if balanced)
    pub final_stacking_depth: i32,
    /// Whether all push/pop pairs are balanced
    pub balanced: bool,
    /// List of clip operations in order
    pub operations: Vec<ClipOperation>,
}

/// A single clip/scroll/stacking operation
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ClipOperation {
    /// Index in display list
    pub index: usize,
    /// Operation type
    pub op: String,
    /// Clip depth after this operation
    pub clip_depth: i32,
    /// Scroll depth after this operation
    pub scroll_depth: i32,
    /// Stacking context depth after this operation
    pub stacking_depth: i32,
    /// Bounds if applicable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<LogicalRectJson>,
    /// Content size (for scroll frames)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_size: Option<LogicalSizeJson>,
    /// Scroll ID (for scroll frames)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_id: Option<u64>,
}

/// Display list item info
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct DisplayListItemInfo {
    pub index: usize,
    #[serde(rename = "type")]
    pub item_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glyph_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub z_index: Option<i32>,
    /// Current clip depth when this item is rendered
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip_depth: Option<i32>,
    /// Current scroll depth when this item is rendered
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_depth: Option<i32>,
    /// Content size (for scroll frames)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_size: Option<LogicalSizeJson>,
    /// Scroll ID (for scroll frames)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_id: Option<u64>,
    /// Debug info string (for debugging scrollbar bounds, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_info: Option<String>,
    /// Border colors per side (for border items)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_colors: Option<BorderColorsJson>,
    /// Border widths per side (for border items)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_widths: Option<BorderWidthsJson>,
}

/// Border colors for all four sides (JSON output)
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct BorderColorsJson {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bottom: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<String>,
}

/// Border widths for all four sides (JSON output)
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct BorderWidthsJson {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bottom: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<f32>,
}

/// Response for GetScrollStates
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScrollStatesResponse {
    pub scroll_node_count: usize,
    pub scroll_states: Vec<ScrollStateInfo>,
}

/// Scroll state info for a single node
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct ScrollStateInfo {
    pub node_id: usize,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub content_width: f32,
    pub content_height: f32,
    pub container_width: f32,
    pub container_height: f32,
    pub max_scroll_x: f32,
    pub max_scroll_y: f32,
}

/// Response for GetScrollableNodes
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScrollableNodesResponse {
    pub scrollable_node_count: usize,
    pub scrollable_nodes: Vec<ScrollableNodeInfo>,
}

/// Scrollable node info
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct ScrollableNodeInfo {
    pub node_id: usize,
    pub dom_node_id: Option<usize>,
    pub container_width: f32,
    pub container_height: f32,
    pub can_scroll_x: bool,
    pub can_scroll_y: bool,
}

/// Response for ScrollNodeBy
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct ScrollNodeByResponse {
    pub scrolled: bool,
    pub node_id: u64,
    pub delta_x: f32,
    pub delta_y: f32,
}

/// Response for ScrollNodeTo
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct ScrollNodeToResponse {
    pub scrolled: bool,
    pub node_id: u64,
    pub x: f32,
    pub y: f32,
}

/// Response for ScrollIntoView
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct ScrollIntoViewResponse {
    pub scrolled: bool,
    pub node_id: u64,
    pub adjustments_count: usize,
}

/// Response for GetScrollbarInfo - detailed scrollbar geometry and state
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScrollbarInfoResponse {
    /// Whether a scrollbar was found
    pub found: bool,
    /// Node ID of the scrollable element
    pub node_id: u64,
    /// DOM node ID (may differ from layout node ID)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dom_node_id: Option<u64>,
    /// Requested orientation
    pub orientation: String,
    /// Horizontal scrollbar info (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub horizontal: Option<ScrollbarGeometry>,
    /// Vertical scrollbar info (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vertical: Option<ScrollbarGeometry>,
    /// Current scroll position
    pub scroll_x: f32,
    pub scroll_y: f32,
    /// Maximum scroll values
    pub max_scroll_x: f32,
    pub max_scroll_y: f32,
    /// Container (viewport) rect
    pub container_rect: LogicalRectJson,
    /// Content rect (total scrollable area)
    pub content_rect: LogicalRectJson,
}

/// Detailed scrollbar geometry for hit-testing and automation
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct ScrollbarGeometry {
    /// Is this scrollbar visible?
    pub visible: bool,
    /// The full track rect (includes buttons at each end)
    pub track_rect: LogicalRectJson,
    /// Center of the track (for clicking)
    pub track_center: LogicalPositionJson,
    /// Base size (button width/height)
    pub button_size: f32,
    /// Top/Left button rect
    pub top_button_rect: LogicalRectJson,
    /// Bottom/Right button rect  
    pub bottom_button_rect: LogicalRectJson,
    /// Thumb rect (the draggable part)
    pub thumb_rect: LogicalRectJson,
    /// Center of the thumb (for dragging)
    pub thumb_center: LogicalPositionJson,
    /// Thumb position ratio (0.0 = top/left, 1.0 = bottom/right)
    pub thumb_position_ratio: f32,
    /// Thumb size ratio (relative to track)
    pub thumb_size_ratio: f32,
}

/// Response for GetSelectionState - text selection state
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct SelectionStateResponse {
    /// Whether any selection exists
    pub has_selection: bool,
    /// Number of DOMs with selections
    pub selection_count: usize,
    /// Selections per DOM
    pub selections: Vec<DomSelectionInfo>,
}

/// Selection info for a single DOM
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct DomSelectionInfo {
    /// DOM ID
    pub dom_id: u32,
    /// Node that contains the selection
    pub node_id: Option<u64>,
    /// CSS selector path to the node (e.g. "div#main > p.intro")
    pub selector: Option<String>,
    /// Selection ranges within this DOM
    pub ranges: Vec<SelectionRangeInfo>,
    /// Selection rectangles (visual bounds of each selected region)
    pub rectangles: Vec<LogicalRectJson>,
}

/// Information about a single selection range
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct SelectionRangeInfo {
    /// Selection type: "cursor", "range", or "block"
    pub selection_type: String,
    /// For cursor: the cursor position (character index)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor_position: Option<usize>,
    /// For range: start character index
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<usize>,
    /// For range: end character index
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<usize>,
    /// Direction: "forward", "backward", or "none"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
}

/// JSON-serializable LogicalSize
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct LogicalSizeJson {
    pub width: f32,
    pub height: f32,
}

/// JSON-serializable LogicalPosition
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct LogicalPositionJson {
    pub x: f32,
    pub y: f32,
}

/// JSON-serializable LogicalRect
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct LogicalRectJson {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Full dump of the SelectionManager for debugging
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct SelectionManagerDump {
    /// All selections indexed by DOM ID
    pub selections: Vec<SelectionDumpEntry>,
    /// Click state for multi-click detection
    pub click_state: ClickStateDump,
}

/// Single selection entry in the dump
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct SelectionDumpEntry {
    /// DOM ID
    pub dom_id: u32,
    /// Node ID 
    pub node_id: Option<u64>,
    /// CSS selector for the node
    pub selector: Option<String>,
    /// All selections on this node
    pub selections: Vec<SelectionDump>,
}

/// Dump of a single Selection
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct SelectionDump {
    /// "cursor" or "range"
    pub selection_type: String,
    /// Raw debug representation
    pub debug: String,
}

/// Dump of click state
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ClickStateDump {
    /// Last clicked node
    pub last_node: Option<String>,
    /// Last click position
    pub last_position: LogicalPositionJson,
    /// Last click time in ms
    pub last_time_ms: u64,
    /// Current click count (1=single, 2=double, 3=triple)
    pub click_count: u8,
}

/// Response for GetDragState - current drag state from unified drag system
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct DragStateResponse {
    /// Whether any drag is currently active
    pub is_dragging: bool,
    /// Type of active drag (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drag_type: Option<String>,
    /// Brief description of the drag state
    pub description: String,
}

/// Response for GetDragContext - detailed drag context information
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct DragContextResponse {
    /// Whether any drag is currently active
    pub is_dragging: bool,
    /// Type of active drag (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drag_type: Option<String>,
    /// Start position of the drag
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_position: Option<LogicalPositionJson>,
    /// Current position of the drag
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_position: Option<LogicalPositionJson>,
    /// Target node ID (for node drags)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_node_id: Option<u64>,
    /// Target DOM ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_dom_id: Option<u32>,
    /// Scrollbar axis (for scrollbar drags)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scrollbar_axis: Option<String>,
    /// Window resize edge (for window resize drags)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resize_edge: Option<String>,
    /// Files being dragged (for file drops)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<String>>,
    /// Drag data (MIME type -> data)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drag_data: Option<std::collections::BTreeMap<String, String>>,
    /// Current drag effect
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drag_effect: Option<String>,
    /// Full debug representation
    pub debug: String,
}

/// Response for GetFocusState - which node has keyboard focus
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct FocusStateResponse {
    /// Whether any node has focus
    pub has_focus: bool,
    /// Focused node information (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focused_node: Option<FocusedNodeInfo>,
}

/// Information about the focused node
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct FocusedNodeInfo {
    /// DOM ID
    pub dom_id: u32,
    /// Node ID within the DOM
    pub node_id: u64,
    /// CSS selector for the node
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    /// Whether the node is contenteditable
    pub is_contenteditable: bool,
    /// Text content of the node (if text node)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_content: Option<String>,
}

/// Response for GetCursorState - cursor position and blink state
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct CursorStateResponse {
    /// Whether a cursor is active
    pub has_cursor: bool,
    /// Cursor information (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<CursorInfo>,
}

/// Information about the text cursor
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct CursorInfo {
    /// DOM ID where cursor is located
    pub dom_id: u32,
    /// Node ID within the DOM
    pub node_id: u64,
    /// Cursor position (grapheme cluster index)
    pub position: usize,
    /// Cursor affinity ("upstream" or "downstream")
    pub affinity: String,
    /// Whether the cursor is currently visible (false during blink off phase)
    pub is_visible: bool,
    /// Whether the cursor blink timer is active
    pub blink_timer_active: bool,
}

// ==================== Debug Events ====================

#[derive(Debug, Clone)]
#[cfg_attr(feature = "std", derive(serde::Deserialize))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum DebugEvent {
    // Mouse Events
    MouseMove {
        x: f32,
        y: f32,
    },
    MouseDown {
        x: f32,
        y: f32,
        #[serde(default)]
        button: MouseButton,
    },
    MouseUp {
        x: f32,
        y: f32,
        #[serde(default)]
        button: MouseButton,
    },
    Click {
        /// X position (used if no selector/node_id provided)
        #[serde(default)]
        x: Option<f32>,
        /// Y position (used if no selector/node_id provided)
        #[serde(default)]
        y: Option<f32>,
        /// CSS selector (e.g. ".button", "#my-id", "div")
        #[serde(default)]
        selector: Option<String>,
        /// Direct node ID to click
        #[serde(default)]
        node_id: Option<u64>,
        /// Text content to find and click
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        button: MouseButton,
    },
    DoubleClick {
        x: f32,
        y: f32,
        #[serde(default)]
        button: MouseButton,
    },
    Scroll {
        x: f32,
        y: f32,
        delta_x: f32,
        delta_y: f32,
    },

    // Keyboard Events
    KeyDown {
        key: String,
        #[serde(default)]
        modifiers: Modifiers,
    },
    KeyUp {
        key: String,
        #[serde(default)]
        modifiers: Modifiers,
    },
    TextInput {
        text: String,
    },

    // Window Events
    Resize {
        width: f32,
        height: f32,
    },
    Move {
        x: i32,
        y: i32,
    },
    Focus,
    Blur,
    Close,
    DpiChanged {
        dpi: u32,
    },

    // Queries
    GetState,
    GetDom,
    HitTest {
        x: f32,
        y: f32,
    },
    GetLogs {
        #[serde(default)]
        since_request_id: Option<u64>,
    },

    // DOM Inspection
    /// Get the HTML representation of the DOM
    GetHtmlString,
    /// Get all computed CSS properties for a node (supports selector, node_id, or text)
    GetNodeCssProperties {
        #[serde(default)]
        node_id: Option<u64>,
        #[serde(default)]
        selector: Option<String>,
        #[serde(default)]
        text: Option<String>,
    },
    /// Get node layout information (position, size) - supports selector, node_id, or text
    GetNodeLayout {
        #[serde(default)]
        node_id: Option<u64>,
        #[serde(default)]
        selector: Option<String>,
        #[serde(default)]
        text: Option<String>,
    },
    /// Get all nodes with their layout info
    GetAllNodesLayout,
    /// Get detailed DOM tree structure
    GetDomTree,
    /// Get the raw node hierarchy (for debugging DOM structure issues)
    GetNodeHierarchy,
    /// Get the layout tree structure (for debugging layout tree building)
    GetLayoutTree,
    /// Get the display list items (what's actually being rendered)
    GetDisplayList,
    /// Get all scroll states (scroll positions for scrollable nodes)
    GetScrollStates,
    /// Get all scrollable nodes (nodes with overflow that can be scrolled)
    GetScrollableNodes,
    /// Scroll a specific node by a delta amount (supports selector, node_id, or text)
    ScrollNodeBy {
        #[serde(default)]
        node_id: Option<u64>,
        #[serde(default)]
        selector: Option<String>,
        #[serde(default)]
        text: Option<String>,
        delta_x: f32,
        delta_y: f32,
    },
    /// Scroll a specific node to an absolute position (supports selector, node_id, or text)
    ScrollNodeTo {
        #[serde(default)]
        node_id: Option<u64>,
        #[serde(default)]
        selector: Option<String>,
        #[serde(default)]
        text: Option<String>,
        x: f32,
        y: f32,
    },
    /// Scroll a node into view (W3C scrollIntoView API)
    /// Scrolls the element into the visible area of its scroll container
    ScrollIntoView {
        #[serde(default)]
        node_id: Option<u64>,
        #[serde(default)]
        selector: Option<String>,
        #[serde(default)]
        text: Option<String>,
        /// Vertical alignment: "start", "center", "end", "nearest" (default)
        #[serde(default)]
        block: Option<String>,
        /// Horizontal alignment: "start", "center", "end", "nearest" (default)
        #[serde(default)]
        inline: Option<String>,
        /// Animation: "auto" (default), "instant", "smooth"
        #[serde(default)]
        behavior: Option<String>,
    },

    // Node Finding
    /// Find a node by text content (returns node_id and bounds)
    FindNodeByText {
        text: String,
    },
    /// Click on a specific node by its ID (deprecated, use Click with node_id)
    ClickNode {
        node_id: u64,
        #[serde(default)]
        button: MouseButton,
    },

    /// Get detailed scrollbar information for a node (supports selector, node_id, or text)
    /// Returns geometry for both horizontal and vertical scrollbars if present
    GetScrollbarInfo {
        #[serde(default)]
        node_id: Option<u64>,
        #[serde(default)]
        selector: Option<String>,
        #[serde(default)]
        text: Option<String>,
        /// Which scrollbar to query: "horizontal", "vertical", or "both" (default)
        #[serde(default)]
        orientation: Option<String>,
    },

    // Selection
    /// Get the current text selection state (selection ranges, cursor positions)
    GetSelectionState,
    /// Dump the entire selection manager state for debugging
    DumpSelectionManager,

    // Drag State
    /// Get the current drag state from the unified drag system
    GetDragState,
    /// Get detailed drag context information (for debugging drag operations)
    GetDragContext,

    // Control
    Relayout,
    Redraw,

    // Testing
    WaitFrame,
    Wait {
        ms: u64,
    },

    // Screenshots
    TakeScreenshot,
    TakeNativeScreenshot,

    // App State (JSON Serialization)
    /// Get the global app state as JSON (requires RefAny with serialize_fn)
    GetAppState,
    /// Set the global app state from JSON (requires RefAny with deserialize_fn)
    SetAppState {
        /// The JSON value to set as the new app state
        state: serde_json::Value,
    },

    // Focus and Cursor State
    /// Get the current focus state (which node has keyboard focus)
    GetFocusState,
    /// Get the current cursor state (position, blink state)
    GetCursorState,

    // E2E Test Execution
    /// Run one or more E2E tests.
    /// This is a regular debug command — send via `POST /` with
    /// `{"op": "run_e2e_tests", "tests": [...]}` or queue
    /// programmatically via `queue_e2e_tests()`.
    RunE2eTests {
        tests: Vec<E2eTest>,
        /// Named snapshots map (alias → saved app_state JSON).
        /// Used by `restore_snapshot` steps to look up pre-saved states.
        #[serde(default)]
        snapshots: Option<std::collections::HashMap<String, serde_json::Value>>,
    },

    // DOM Mutation
    /// Insert a new child node into the DOM tree
    InsertNode {
        /// Parent node ID to insert under
        parent_id: u64,
        /// Node type / tag name (e.g. "div", "p", "span", "text:Hello World")
        node_type: String,
        /// Child index to insert at (omit to append at end)
        #[serde(default)]
        position: Option<usize>,
        /// CSS classes for the new node
        #[serde(default)]
        classes: Vec<String>,
        /// Optional ID attribute for the new node
        #[serde(default)]
        id: Option<String>,
    },
    /// Delete a node from the DOM tree (tombstones it)
    DeleteNode {
        /// Node ID to delete
        node_id: u64,
    },
    /// Set the text content of a node
    SetNodeText {
        /// Node ID to modify
        node_id: u64,
        /// New text content
        text: String,
    },
    /// Set CSS classes on a node (replaces existing classes)
    SetNodeClasses {
        /// Node ID to modify
        node_id: u64,
        /// New CSS classes
        classes: Vec<String>,
        /// Optional new ID (omit to keep current)
        #[serde(default)]
        id: Option<String>,
    },
    /// Override CSS properties on a node
    SetNodeCssOverride {
        /// Node ID to modify
        node_id: u64,
        /// CSS property name (e.g. "width", "background-color")
        property: String,
        /// CSS property value (e.g. "100px", "red")
        value: String,
    },
    /// Resolve function pointers to symbol names (via dladdr)
    ResolveFunctionPointers {
        /// List of function pointer addresses (as decimal strings)
        addresses: Vec<String>,
    },
    /// Get the component registry: which tags are available and what attributes they accept
    GetComponentRegistry,
    /// Get just the list of registered component libraries (lightweight, no component details)
    GetLibraries,
    /// Get all components within a specific library
    GetLibraryComponents {
        /// Library name, e.g. "builtin", "shadcn"
        library: String,
    },
    /// Export code: compile all exportable components into a project scaffold
    /// and return the result as base64-encoded ZIP
    ExportCode {
        /// Target language: "rust", "c", "cpp", "python"
        language: String,
    },
    /// Import a component library from JSON definition.
    /// Components are added to the runtime component map as user-defined.
    ImportComponentLibrary {
        /// The library definition in JSON form
        library: ExportedLibraryResponse,
    },
    /// Export a component library as JSON.
    /// Only user-defined (exportable) libraries can be exported.
    /// If no library name is given, exports ALL exportable libraries.
    ExportComponentLibrary {
        /// Library name to export, or omit for all exportable
        #[serde(default)]
        library: Option<String>,
    },
    /// Create a new empty user-defined component library
    CreateLibrary {
        /// Library name
        name: String,
        /// Optional description
        #[serde(default)]
        description: Option<String>,
    },
    /// Delete a user-defined component library
    DeleteLibrary {
        /// Library name to delete
        name: String,
    },
    /// Create a new empty component in a library
    CreateComponent {
        /// Library name
        library: String,
        /// Component tag name
        name: String,
        /// Human-readable display name
        #[serde(default)]
        display_name: Option<String>,
    },
    /// Delete a component from a library
    DeleteComponent {
        /// Library name
        library: String,
        /// Component tag name
        name: String,
    },
    /// Update a component's properties (template, CSS, data model, etc.)
    UpdateComponent {
        /// Library name
        library: String,
        /// Component tag name
        name: String,
        /// New template (if provided)
        #[serde(default)]
        template: Option<String>,
        /// New scoped CSS (if provided)
        #[serde(default)]
        scoped_css: Option<String>,
        /// New description (if provided)
        #[serde(default)]
        description: Option<String>,
        /// New display name (if provided)
        #[serde(default)]
        display_name: Option<String>,
        /// Replace all data model fields (if provided).
        /// Unified list: includes both data fields and callbacks.
        #[serde(default)]
        fields: Option<Vec<ExportedDataField>>,
    },
    /// Open a source file in the user's editor (best-effort)
    OpenFile {
        /// Absolute path to the file
        file: String,
        /// Line number (1-based, 0 = don't jump)
        #[serde(default)]
        line: u32,
    },
}

// ==================== Node Resolution Helper ====================

/// Resolves a node target (selector, node_id, or text) to a NodeId.
/// Returns the first matching node or None if no match found.
#[cfg(feature = "std")]
fn resolve_node_target(
    callback_info: &azul_layout::callbacks::CallbackInfo,
    selector: Option<&str>,
    node_id: Option<u64>,
    text: Option<&str>,
) -> Option<azul_core::id::NodeId> {
    use azul_core::dom::DomId;
    use azul_core::id::NodeId;

    let dom_id = DomId { inner: 0 };

    // Direct node ID
    if let Some(nid) = node_id {
        return Some(NodeId::new(nid as usize));
    }

    // CSS selector
    if let Some(sel) = selector {
        use azul_core::style::matches_html_element;
        use azul_css::parser2::parse_css_path;

        let layout_window = callback_info.get_layout_window();
        if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
            if let Ok(css_path) = parse_css_path(sel) {
                let styled_dom = &layout_result.styled_dom;
                let node_hierarchy = styled_dom.node_hierarchy.as_container();
                let node_data = styled_dom.node_data.as_container();
                let cascade_info = styled_dom.cascade_info.as_container();

                for i in 0..node_data.len() {
                    let node_id = NodeId::new(i);
                    if matches_html_element(
                        &css_path,
                        node_id,
                        &node_hierarchy,
                        &node_data,
                        &cascade_info,
                        None,
                    ) {
                        return Some(node_id);
                    }
                }
            }
        }
    }

    // Text content
    if let Some(txt) = text {
        let layout_window = callback_info.get_layout_window();
        if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
            let styled_dom = &layout_result.styled_dom;
            let node_data = styled_dom.node_data.as_container();

            for i in 0..node_data.len() {
                let data = &node_data[NodeId::new(i)];
                if let azul_core::dom::NodeType::Text(t) = data.get_node_type() {
                    if t.as_str().contains(txt) {
                        return Some(NodeId::new(i));
                    }
                }
            }
        }
    }

    None
}

/// Resolve a CSS selector to **all** matching NodeIds (not just the first).
///
/// Used by `assert_node_count` and also internally by the assertion engine
/// to verify existence / non-existence of nodes.
#[cfg(feature = "std")]
fn resolve_all_matching_nodes(
    callback_info: &azul_layout::callbacks::CallbackInfo,
    selector: &str,
) -> Vec<azul_core::id::NodeId> {
    use azul_core::dom::DomId;
    use azul_core::id::NodeId;
    use azul_core::style::matches_html_element;
    use azul_css::parser2::parse_css_path;

    let dom_id = DomId { inner: 0 };
    let layout_window = callback_info.get_layout_window();

    let layout_result = match layout_window.layout_results.get(&dom_id) {
        Some(lr) => lr,
        None => return Vec::new(),
    };

    let css_path = match parse_css_path(selector) {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };

    let styled_dom = &layout_result.styled_dom;
    let node_hierarchy = styled_dom.node_hierarchy.as_container();
    let node_data = styled_dom.node_data.as_container();
    let cascade_info = styled_dom.cascade_info.as_container();

    let mut results = Vec::new();
    for i in 0..node_data.len() {
        let node_id = NodeId::new(i);
        if matches_html_element(
            &css_path,
            node_id,
            &node_hierarchy,
            &node_data,
            &cascade_info,
            None,
        ) {
            results.push(node_id);
        }
    }
    results
}

/// Builds a CSS selector string for a node (e.g., "div#my-id.class1.class2")
/// Returns a selector that can be used to find this node again
#[cfg(feature = "std")]
fn build_selector_for_node(
    callback_info: &azul_layout::callbacks::CallbackInfo,
    dom_id: azul_core::dom::DomId,
    node_id: azul_core::id::NodeId,
) -> Option<String> {
    use alloc::string::ToString;

    let layout_window = callback_info.get_layout_window();
    let layout_result = layout_window.layout_results.get(&dom_id)?;
    let styled_dom = &layout_result.styled_dom;
    let node_data_container = styled_dom.node_data.as_container();
    
    if node_id.index() >= node_data_container.len() {
        return None;
    }
    
    let node_data = &node_data_container[node_id];
    
    // Get tag name from NodeTypeTag (lowercase HTML tag name)
    let node_type_tag = node_data.get_node_type().get_path();
    let tag_name = alloc::format!("{:?}", node_type_tag).to_lowercase();
    
    let mut selector = tag_name;
    
    // Add ID if present (first ID wins)
    let ids_and_classes = node_data.get_ids_and_classes();
    for idc in ids_and_classes.iter() {
        if let Some(id) = idc.as_id() {
            selector.push('#');
            selector.push_str(id);
            break; // Only one ID
        }
    }
    
    // Add all classes
    for idc in ids_and_classes.iter() {
        if let Some(class) = idc.as_class() {
            selector.push('.');
            selector.push_str(class);
        }
    }
    
    // If no ID or classes, add node index to make it unique
    let has_id_or_class = ids_and_classes.iter().any(|idc| idc.as_id().is_some() || idc.as_class().is_some());
    if !has_id_or_class {
        selector.push_str(&alloc::format!(":nth-child({})", node_id.index() + 1));
    }
    
    Some(selector)
}

/// Resolves a node target to center position (x, y) for clicking
#[cfg(feature = "std")]
fn resolve_node_center(
    callback_info: &azul_layout::callbacks::CallbackInfo,
    selector: Option<&str>,
    node_id: Option<u64>,
    text: Option<&str>,
) -> Option<(f32, f32)> {
    use azul_core::dom::{DomId, DomNodeId};
    use azul_core::id::NodeId;

    let dom_id = DomId { inner: 0 };

    if let Some(nid) = resolve_node_target(callback_info, selector, node_id, text) {
        let dom_node_id = DomNodeId {
            dom: dom_id,
            node: Some(nid).into(),
        };
        if let Some(rect) = callback_info.get_node_rect(dom_node_id) {
            return Some((
                rect.origin.x + rect.size.width / 2.0,
                rect.origin.y + rect.size.height / 2.0,
            ));
        }
    }

    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    #[default]
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, Default)]
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
pub struct Modifiers {
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub meta: bool,
}

// ==================== Global State ====================

#[cfg(feature = "std")]
static LOG_QUEUE: OnceLock<Mutex<Vec<LogMessage>>> = OnceLock::new();

#[cfg(feature = "std")]
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

#[cfg(feature = "std")]
static SERVER_START_TIME: OnceLock<std::time::Instant> = OnceLock::new();

#[cfg(feature = "std")]
static DEBUG_ENABLED: AtomicBool = AtomicBool::new(false);

/// Whether E2E test runner mode is active (independent of debug server).
#[cfg(feature = "std")]
static E2E_ACTIVE: AtomicBool = AtomicBool::new(false);

#[cfg(feature = "std")]
static DEBUG_PORT: OnceLock<u16> = OnceLock::new();

/// Global debug server handle (singleton — one per application).
/// Started in `AppInternal::create()` when `AZUL_DEBUG=<port>` is set.
#[cfg(feature = "std")]
static DEBUG_SERVER: OnceLock<Arc<DebugServerHandle>> = OnceLock::new();

// ==================== Debug Server Handle ====================

/// Handle to the debug server for clean shutdown
#[cfg(feature = "std")]
pub struct DebugServerHandle {
    pub shutdown_tx: mpsc::Sender<()>,
    pub thread_handle: Mutex<Option<std::thread::JoinHandle<()>>>,
    pub port: u16,
    /// The sender side of the spmc channel.
    /// HTTP thread and `queue_e2e_tests` use this to push `DebugRequest`s.
    pub request_tx: Arc<Mutex<spmc::Sender<DebugRequest>>>,
}

#[cfg(feature = "std")]
impl std::fmt::Debug for DebugServerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DebugServerHandle")
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "std")]
impl DebugServerHandle {
    /// Signal the server to shut down
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
        // Give the server thread a moment to exit
        if let Ok(mut guard) = self.thread_handle.lock() {
            if let Some(handle) = guard.take() {
                let _ = handle.join();
            }
        }
    }
}

#[cfg(feature = "std")]
impl Drop for DebugServerHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// ==================== Public API ====================

/// Get a clone of the global `DebugServerHandle` `Arc`.
///
/// Returns `None` when `AZUL_DEBUG` was not set or the server
/// hasn't been started yet.
#[cfg(feature = "std")]
pub fn get_debug_server() -> Option<Arc<DebugServerHandle>> {
    DEBUG_SERVER.get().cloned()
}

/// Check if the debug timer should be registered.
///
/// Returns `true` when either `AZUL_DEBUG=<port>` started the HTTP
/// server **or** `AZUL_RUN_E2E_TESTS` queued tests.
#[cfg(feature = "std")]
pub fn is_debug_enabled() -> bool {
    DEBUG_ENABLED.load(Ordering::SeqCst) || E2E_ACTIVE.load(Ordering::SeqCst)
}

/// Push `RunE2eTests` via the spmc channel and return the response
/// receiver.  Activates the E2E flag so `is_debug_enabled()` returns
/// `true` and the timer gets registered by the platform event loop.
///
/// Requires `DEBUG_SERVER` to be set (by `start_debug_server` or
/// `create_debug_channel`).
///
/// The caller is responsible for receiving from the returned channel —
/// typically on a background thread that prints results and calls
/// `std::process::exit`.
#[cfg(feature = "std")]
pub fn queue_e2e_tests(
    tests: Vec<E2eTest>,
) -> std::sync::mpsc::Receiver<DebugResponseData> {
    E2E_ACTIVE.store(true, Ordering::SeqCst);

    let test_count = tests.len();
    let total_steps: usize = tests.iter().map(|t| t.steps.len()).sum();
    log(
        LogLevel::Info,
        LogCategory::DebugServer,
        format!("[E2E] Queuing {} test(s) with {} total step(s)", test_count, total_steps),
        None,
    );
    for (i, test) in tests.iter().enumerate() {
        log(
            LogLevel::Debug,
            LogCategory::DebugServer,
            format!("[E2E]   test[{}]: '{}' ({} steps)", i, test.name, test.steps.len()),
            None,
        );
    }

    let (tx, rx) = mpsc::channel();
    let request_id = NEXT_REQUEST_ID.fetch_add(1, Ordering::SeqCst);

    let request = DebugRequest {
        request_id,
        event: DebugEvent::RunE2eTests { tests, snapshots: None },
        window_id: None,
        wait_for_render: false,
        response_tx: tx,
    };

    if let Some(handle) = DEBUG_SERVER.get() {
        if let Ok(mut sender) = handle.request_tx.lock() {
            let _ = sender.send(request);
        }
    }

    rx
}

/// Get debug server port from environment
///
/// The `AZUL_DEBUG` environment variable should be set to a port number (e.g., `AZUL_DEBUG=8765`).
/// Ports below 1024 require root/administrator privileges.
/// Returns `None` if not set or not a valid port number.
#[cfg(feature = "std")]
pub fn get_debug_port() -> Option<u16> {
    std::env::var("AZUL_DEBUG")
        .ok()
        .and_then(|s| s.parse().ok())
}

/// Initialize and start the debug server.
///
/// This function:
/// 1. Creates an `spmc::channel` for debug requests
/// 2. Binds to the port (exits process if port is taken)
/// 3. Starts the HTTP server thread (captures the `spmc::Sender`)
/// 4. Blocks until the server is ready to accept connections
/// 5. Stores the handle in `DEBUG_SERVER` for global access
/// 6. Returns the handle AND the `spmc::Receiver` for window timers
///
/// Called once from `run()` when `AZUL_DEBUG=<port>` is set.
/// Subsequent calls return the existing handle (without a new receiver).
#[cfg(feature = "std")]
pub fn start_debug_server(
    port: u16,
) -> (Arc<DebugServerHandle>, spmc::Receiver<DebugRequest>) {

    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::Duration;

    // Initialize static state
    SERVER_START_TIME.get_or_init(std::time::Instant::now);
    LOG_QUEUE.get_or_init(|| Mutex::new(Vec::new()));
    let _ = DEBUG_PORT.set(port);
    DEBUG_ENABLED.store(true, Ordering::SeqCst);

    // Create spmc channel for debug requests
    let (request_tx, request_rx) = spmc::channel::<DebugRequest>();
    let request_tx = Arc::new(Mutex::new(request_tx));
    let request_tx_for_thread = request_tx.clone();

    // Try to bind - exit if port is taken
    let listener = match TcpListener::bind(format!("127.0.0.1:{}", port)) {
        Ok(l) => l,
        Err(e) => {
            std::process::exit(1);
        }
    };

    // Set a short timeout for accept() so we can check for shutdown
    listener.set_nonblocking(false).ok();

    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();

    // Channel to signal when server is ready
    let (ready_tx, ready_rx) = mpsc::channel::<()>();

    // Start server thread
    let thread_handle = thread::Builder::new()
        .name("azul-debug-server".to_string())
        .spawn(move || {
            // Signal that we're ready
            let _ = ready_tx.send(());

            // Set a timeout on the listener so we can check for shutdown
            listener.set_nonblocking(true).ok();

            log_internal(
                LogLevel::Info,
                LogCategory::DebugServer,
                format!("Debug server listening on http://127.0.0.1:{}", port),
                None,
            );

            loop {
                // Check for shutdown signal (non-blocking)
                if shutdown_rx.try_recv().is_ok() {
                    log_internal(
                        LogLevel::Info,
                        LogCategory::DebugServer,
                        "Debug server shutting down",
                        None,
                    );
                    break;
                }

                // Try to accept a connection (non-blocking)
                match listener.accept() {
                    Ok((mut stream, _addr)) => {
                        // NOTE: Stream explicitly set to blocking mode
                        // The listener is non-blocking, but accepted streams may inherit this.
                        // This causes the final read loop to fail immediately with WouldBlock,
                        // closing the socket before the client has read all data.
                        stream.set_nonblocking(false).ok();
                        // Set read timeout
                        stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
                        // Increase write timeout to 30s for large screenshot transfers
                        stream.set_write_timeout(Some(Duration::from_secs(30))).ok();
                        handle_http_connection(&mut stream, &request_tx_for_thread);
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        // No connection pending, sleep a bit
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => {
                        // Other error, continue
                        thread::sleep(Duration::from_millis(10));
                    }
                }
            }
        })
        .expect("Failed to spawn debug server thread");

    // Wait for server to be ready
    let _ = ready_rx.recv_timeout(Duration::from_secs(5));

    // Verify server is actually accepting connections
    for _ in 0..10 {
        if TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", port).parse().unwrap(),
            Duration::from_millis(100),
        )
        .is_ok()
        {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }

    log_internal(
        LogLevel::Info,
        LogCategory::DebugServer,
        format!("Debug server ready on http://127.0.0.1:{}", port),
        None,
    );

    let handle = Arc::new(DebugServerHandle {
        shutdown_tx,
        thread_handle: Mutex::new(Some(thread_handle)),
        port,
        request_tx,
    });
    let _ = DEBUG_SERVER.set(handle.clone());
    (handle, request_rx)
}

/// Create a debug channel without starting the HTTP server.
///
/// Used for E2E-only mode (`AZUL_RUN_E2E_TESTS` without `AZUL_DEBUG`).
/// Creates the `spmc` channel, stores a minimal `DebugServerHandle` in
/// `DEBUG_SERVER`, and returns the receiver for window timers.
#[cfg(feature = "std")]
pub fn create_debug_channel() -> (Arc<DebugServerHandle>, spmc::Receiver<DebugRequest>) {
    SERVER_START_TIME.get_or_init(std::time::Instant::now);
    LOG_QUEUE.get_or_init(|| Mutex::new(Vec::new()));

    let (request_tx, request_rx) = spmc::channel::<DebugRequest>();
    let request_tx = Arc::new(Mutex::new(request_tx));
    let (shutdown_tx, _shutdown_rx) = mpsc::channel::<()>();

    let handle = Arc::new(DebugServerHandle {
        shutdown_tx,
        thread_handle: Mutex::new(None),
        port: 0,
        request_tx,
    });
    let _ = DEBUG_SERVER.set(handle.clone());
    (handle, request_rx)
}

/// Log a message (thread-safe, lock-free when debug is disabled)
#[cfg(feature = "std")]
#[track_caller]
pub fn log(
    level: LogLevel,
    category: LogCategory,
    message: impl Into<String>,
    window_id: Option<&str>,
) {
    if !is_debug_enabled() {
        return;
    }
    log_internal(level, category, message, window_id);
}

#[cfg(feature = "std")]
#[track_caller]
fn log_internal(
    level: LogLevel,
    category: LogCategory,
    message: impl Into<String>,
    window_id: Option<&str>,
) {
    let location = core::panic::Location::caller();
    let timestamp_us = SERVER_START_TIME
        .get()
        .map(|t| t.elapsed().as_micros() as u64)
        .unwrap_or(0);

    let msg = LogMessage {
        timestamp_us,
        level,
        category,
        message: message.into(),
        location: format!("{}:{}", location.file(), location.line()),
        window_id: window_id.map(String::from),
    };

    if let Some(queue) = LOG_QUEUE.get() {
        if let Ok(mut q) = queue.lock() {
            q.push(msg);
        }
    }
}

/// Take all log messages
#[cfg(feature = "std")]
pub fn take_logs() -> Vec<LogMessage> {
    if let Some(queue) = LOG_QUEUE.get() {
        if let Ok(mut q) = queue.lock() {
            return core::mem::take(&mut *q);
        }
    }
    Vec::new()
}

/// Send a successful response to a debug request
#[cfg(feature = "std")]
pub fn send_ok(
    request: &DebugRequest,
    window_state: Option<WindowStateSnapshot>,
    data: Option<ResponseData>,
) {
    // Clear logs to prevent memory buildup
    let _ = take_logs();
    let response = DebugResponseData::Ok { window_state, data };
    if let Err(e) = request.response_tx.send(response) {
    }
}

/// Send an error response to a debug request
#[cfg(feature = "std")]
pub fn send_err(request: &DebugRequest, message: impl Into<String>) {
    // Clear logs to prevent memory buildup
    let _ = take_logs();
    let response = DebugResponseData::Err(message.into());
    if let Err(e) = request.response_tx.send(response) {
    }
}

/// Helper function for serializing DebugHttpResponse
#[cfg(feature = "std")]
fn serialize_http_response(response: &DebugHttpResponse) -> String {
    serde_json::to_string_pretty(response)
        .unwrap_or_else(|_| r#"{"status":"error","message":"Serialization failed"}"#.to_string())
}

// ==================== HTTP Server ====================

#[cfg(feature = "std")]
fn handle_http_connection(stream: &mut std::net::TcpStream, request_tx: &Arc<Mutex<spmc::Sender<DebugRequest>>>) {
    use std::io::{Read, Write};

    let mut buffer = [0u8; 16384];
    let bytes_read = match stream.read(&mut buffer) {
        Ok(n) if n > 0 => n,
        _ => return,
    };

    let request = String::from_utf8_lossy(&buffer[..bytes_read]);

    // Parse HTTP request
    let lines: Vec<&str> = request.lines().collect();
    if lines.is_empty() {
        return;
    }

    let first_line = lines[0];
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }

    let method = parts[0];
    let path = parts[1];

    // ── Route: GET /debugger.css → serve CSS ──
    if method == "GET" && path == "/debugger.css" {
        let css = include_str!("debugger/debugger.css");
        let header = format!(
            "HTTP/1.0 200 OK\r\nContent-Type: text/css; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            css.len()
        );
        stream.set_nodelay(true).ok();
        let _ = stream.write_all(header.as_bytes());
        for chunk in css.as_bytes().chunks(8192) {
            if stream.write_all(chunk).is_err() { return; }
        }
        let _ = stream.flush();
        let _ = stream.shutdown(std::net::Shutdown::Write);
        let mut drain = [0u8; 64];
        while let Ok(n) = stream.read(&mut drain) { if n == 0 { break; } }
        return;
    }

    // ── Route: GET /debugger.js → serve JS ──
    if method == "GET" && path == "/debugger.js" {
        let js = include_str!("debugger/debugger.js");
        let header = format!(
            "HTTP/1.0 200 OK\r\nContent-Type: application/javascript; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            js.len()
        );
        stream.set_nodelay(true).ok();
        let _ = stream.write_all(header.as_bytes());
        for chunk in js.as_bytes().chunks(8192) {
            if stream.write_all(chunk).is_err() { return; }
        }
        let _ = stream.flush();
        let _ = stream.shutdown(std::net::Shutdown::Write);
        let mut drain = [0u8; 64];
        while let Ok(n) = stream.read(&mut drain) { if n == 0 { break; } }
        return;
    }

    // ── Route: GET / → serve the debugger UI ──
    if method == "GET" && (path == "/" || path == "/index.html") {
        let html = include_str!("debugger/debugger.html");
        let header = format!(
            "HTTP/1.0 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            html.len()
        );
        stream.set_nodelay(true).ok();
        let _ = stream.write_all(header.as_bytes());
        for chunk in html.as_bytes().chunks(8192) {
            if stream.write_all(chunk).is_err() { return; }
        }
        let _ = stream.flush();
        let _ = stream.shutdown(std::net::Shutdown::Write);
        // Read client close
        let mut drain = [0u8; 64];
        while let Ok(n) = stream.read(&mut drain) { if n == 0 { break; } }
        return;
    }

    let response_json = match (method, path) {
        // Health check - GET /health
        ("GET", "/health") => {
            let logs = take_logs();
            let health = HealthResponse {
                port: DEBUG_PORT.get().copied().unwrap_or(0),
                pending_logs: logs.len(),
                logs: logs
                    .iter()
                    .map(|l| LogMessageJson {
                        timestamp_us: l.timestamp_us,
                        level: format!("{:?}", l.level),
                        category: format!("{:?}", l.category),
                        message: l.message.clone(),
                    })
                    .collect(),
            };
            serialize_http_response(&DebugHttpResponse::Ok(DebugHttpResponseOk {
                request_id: 0,
                window_state: None,
                data: Some(ResponseData::Health(health)),
            }))
        }

        // Event handling - POST /
        ("POST", "/") => {
            // Parse body
            let body_start = request
                .find("\r\n\r\n")
                .map(|i| i + 4)
                .or_else(|| request.find("\n\n").map(|i| i + 2));

            if let Some(start) = body_start {
                let body = &request[start..];
                handle_event_request(body, request_tx)
            } else {
                serialize_http_response(&DebugHttpResponse::Error(DebugHttpResponseError {
                    request_id: None,
                    message: "No request body".to_string(),
                }))
            }
        }

        _ => serialize_http_response(&DebugHttpResponse::Error(DebugHttpResponseError {
            request_id: None,
            message: "GET / → debugger UI, GET /debugger.css → CSS, GET /debugger.js → JS, GET /health → status, POST / → debug commands (incl. run_e2e_tests)".to_string(),
        })),
    };

    // Calculate length for Content-Length header
    let body_bytes = response_json.as_bytes();
    let header = format!(
        "HTTP/1.0 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body_bytes.len()
    );

    // Set NoDelay to push packets immediately
    stream.set_nodelay(true).ok();

    // 1. Write Header (Small, safe to write all at once)
    if stream.write_all(header.as_bytes()).is_err() {
        return;
    }

    // 2. Write Body in Chunks (Safer for large data like screenshots)
    let mut bytes_written = 0usize;
    for chunk in body_bytes.chunks(8192) {
        match stream.write_all(chunk) {
            Ok(_) => {
                bytes_written += chunk.len();
            }
            Err(e) => {
                return;
            }
        }
    }

    // 3. Flush ensures data is in the kernel buffer
    if stream.flush().is_err() {
        return;
    }

    // Graceful Shutdown Pattern
    // 1. Shutdown WRITE side only. This sends TCP FIN to the client.
    if stream.shutdown(std::net::Shutdown::Write).is_err() {
        return;
    }

    // 2. Read until EOF. This keeps the socket alive until the client
    //    confirms receipt and closes their end. This prevents the OS
    //    from destroying the socket while data is still in flight (RST).
    let mut buffer = [0u8; 512];
    while let Ok(n) = stream.read(&mut buffer) {
        if n == 0 {
            break;
        } // EOF received, client closed connection
    }
}

#[cfg(feature = "std")]
fn handle_event_request(body: &str, request_tx: &Arc<Mutex<spmc::Sender<DebugRequest>>>) -> String {
    use std::time::Duration;

    // Parse the event request
    #[derive(serde::Deserialize)]
    struct EventRequest {
        #[serde(flatten)]
        event: DebugEvent,
        #[serde(default)]
        window_id: Option<String>,
        #[serde(default)]
        wait_for_render: bool,
        /// Override the default 30 s response timeout (seconds).
        /// E2E tests should set this to 300+.
        #[serde(default)]
        timeout_secs: Option<u64>,
    }

    let parsed: Result<EventRequest, _> = serde_json::from_str(body);

    match parsed {
        Ok(req) => {
            // Create request and channel
            let (tx, rx) = mpsc::channel();
            let request_id = NEXT_REQUEST_ID.fetch_add(1, Ordering::SeqCst);

            let request = DebugRequest {
                request_id,
                event: req.event,
                window_id: req.window_id,
                wait_for_render: req.wait_for_render,
                response_tx: tx,
            };

            // Send via spmc channel
            if let Ok(mut sender) = request_tx.lock() {
                let _ = sender.send(request);
            }

            // Wait for response (with timeout)
            let timeout = Duration::from_secs(req.timeout_secs.unwrap_or(30));
            match rx.recv_timeout(timeout) {
                Ok(response_data) => {
                    let http_response = match response_data {
                        DebugResponseData::Ok { window_state, data } => {
                            DebugHttpResponse::Ok(DebugHttpResponseOk {
                                request_id,
                                window_state,
                                data,
                            })
                        }
                        DebugResponseData::Err(message) => DebugHttpResponse::Error(DebugHttpResponseError {
                            request_id: Some(request_id),
                            message,
                        }),
                    };
                    serialize_http_response(&http_response)
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    serialize_http_response(&DebugHttpResponse::Error(DebugHttpResponseError {
                        request_id: Some(request_id),
                        message: "Timeout waiting for response (is the timer running?)".to_string(),
                    }))
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    serialize_http_response(&DebugHttpResponse::Error(DebugHttpResponseError {
                        request_id: Some(request_id),
                        message: "Event loop disconnected".to_string(),
                    }))
                }
            }
        }
        Err(e) => serialize_http_response(&DebugHttpResponse::Error(DebugHttpResponseError {
            request_id: None,
            message: format!("Invalid JSON: {}", e),
        })),
    }
}

// ==================== E2E Test Types ====================

/// Runtime configuration that governs how the E2E runner executes tests.
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
pub struct E2eConfig {
    /// failure instead of aborting the test immediately.  Default: `false`.
    #[serde(default)]
    pub continue_on_failure: bool,
    /// Milliseconds to wait between successive steps.  Useful for visual
    /// inspection when the test runs on a visible window.  Default: `0`.
    #[serde(default)]
    pub delay_between_steps_ms: u64,
}

#[cfg(feature = "std")]
impl Default for E2eConfig {
    fn default() -> Self {
        Self {
            continue_on_failure: false,
            delay_between_steps_ms: 0,
        }
    }
}

/// A single E2E test containing setup + steps.
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct E2eTest {
    /// Human-readable test name (required).
    pub name: String,
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
    /// Optional runtime configuration (continue_on_failure, delay, …).
    #[serde(default)]
    pub config: E2eConfig,
    /// Optional setup (window size, DPI, initial app state).
    #[serde(default)]
    pub setup: Option<E2eSetup>,
    /// Ordered list of steps (commands + assertions).
    pub steps: Vec<E2eStep>,
}

/// Optional setup block applied before running steps.
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct E2eSetup {
    #[serde(default = "default_width")]
    pub window_width: u32,
    #[serde(default = "default_height")]
    pub window_height: u32,
    #[serde(default = "default_dpi")]
    pub dpi: u32,
    /// If set, `set_app_state` is called before the first step.
    #[serde(default)]
    pub app_state: Option<serde_json::Value>,
}

#[cfg(feature = "std")]
fn default_width() -> u32 { 800 }
#[cfg(feature = "std")]
fn default_height() -> u32 { 600 }
#[cfg(feature = "std")]
fn default_dpi() -> u32 { 96 }

/// A single step inside an E2E test.
///
/// Steps are either regular debug commands (click, text_input, …) or
/// assertions (assert_text, assert_exists, …).  The JSON format is the
/// same as the debug API: `{"op": "click", "selector": ".btn"}`.
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct E2eStep {
    /// Operation name (same as DebugEvent discriminant, plus assert_* ops).
    pub op: String,
    /// Whether to capture a screenshot after this step.
    #[serde(default)]
    pub screenshot: bool,
    /// All other fields are forwarded as command parameters.
    #[serde(flatten)]
    pub params: serde_json::Value,
}

/// Result of running a single E2E test.
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct E2eTestResult {
    pub name: String,
    /// "pass" or "fail"
    pub status: String,
    pub duration_ms: u64,
    pub step_count: usize,
    pub steps_passed: usize,
    pub steps_failed: usize,
    pub steps: Vec<E2eStepResult>,
    /// Screenshot taken after the last step (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_screenshot: Option<String>,
}

/// Result of running a single step.
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct E2eStepResult {
    pub step_index: usize,
    pub op: String,
    /// "pass" or "fail"
    pub status: String,
    pub duration_ms: u64,
    pub logs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<serde_json::Value>,
}

// ==================== E2E Assertion Evaluation ====================

/// Result of an assertion evaluation.
#[derive(Debug, Clone)]
#[cfg(feature = "std")]
pub struct AssertionResult {
    /// Whether the assertion passed.
    pub passed: bool,
    /// Human-readable message (details for pass or failure reason).
    pub message: String,
    /// Actual value encountered (for diagnostics).
    pub actual: Option<String>,
    /// Expected value (for diagnostics).
    pub expected: Option<String>,
}

#[cfg(feature = "std")]
impl AssertionResult {
    fn pass(message: impl Into<String>) -> Self {
        Self { passed: true, message: message.into(), actual: None, expected: None }
    }
    fn fail(message: impl Into<String>) -> Self {
        Self { passed: false, message: message.into(), actual: None, expected: None }
    }
    fn fail_with(message: impl Into<String>, expected: impl Into<String>, actual: impl Into<String>) -> Self {
        let expected = expected.into();
        let actual = actual.into();
        Self { passed: false, message: message.into(), actual: Some(actual), expected: Some(expected) }
    }
}

/// Evaluate an assertion step against the **live DOM state**.
///
/// Unlike the old `evaluate_assertion()` which only validated parameters,
/// this function actually queries the DOM / layout / app state through
/// `callback_info` and `app_data` and returns a concrete pass/fail result.
///
/// # Assertion operations
///
/// | `op`                 | Required params                              |
/// |----------------------|----------------------------------------------|
/// | `assert_text`        | `selector`, `expected`                       |
/// | `assert_exists`      | `selector`                                   |
/// | `assert_not_exists`  | `selector`                                   |
/// | `assert_node_count`  | `selector`, `expected` (number)              |
/// | `assert_layout`      | `selector`, `property`, `expected`, `tolerance?` |
/// | `assert_css`         | `selector`, `property`, `expected`           |
/// | `assert_app_state`   | `path`, `expected`                           |
/// | `assert_scroll`      | `selector`, `x?`, `y?`, `tolerance?`         |
#[cfg(feature = "std")]
pub fn evaluate_assertion(
    op: &str,
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
    app_data: &azul_core::refany::RefAny,
) -> AssertionResult {
    log(
        LogLevel::Debug,
        LogCategory::DebugServer,
        format!("[E2E] evaluate_assertion: op='{}', params={}", op, params),
        None,
    );
    let result = match op {
        "assert_text" => eval_assert_text(params, callback_info),
        "assert_exists" => eval_assert_exists(params, callback_info),
        "assert_not_exists" => eval_assert_not_exists(params, callback_info),
        "assert_node_count" => eval_assert_node_count(params, callback_info),
        "assert_layout" => eval_assert_layout(params, callback_info),
        "assert_css" => eval_assert_css(params, callback_info),
        "assert_app_state" => eval_assert_app_state(params, app_data),
        "assert_scroll" => eval_assert_scroll(params, callback_info),
        other => AssertionResult::fail(format!("Unknown assertion: {}", other)),
    };
    if result.passed {
        log(
            LogLevel::Debug,
            LogCategory::DebugServer,
            format!("[E2E] assertion PASSED: {}", result.message),
            None,
        );
    } else {
        log(
            LogLevel::Info,
            LogCategory::DebugServer,
            format!("[E2E] assertion FAILED: {} (expected={:?}, actual={:?})", result.message, result.expected, result.actual),
            None,
        );
    }
    result
}

// ---- Individual assertion implementations ----

/// `assert_text`: assert that the text content of the first node matching
/// `selector` equals `expected`.
#[cfg(feature = "std")]
fn eval_assert_text(
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> AssertionResult {
    let selector = match params.get("selector").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s,
        _ => return AssertionResult::fail("assert_text: missing 'selector' parameter"),
    };
    let expected = match params.get("expected").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return AssertionResult::fail("assert_text: missing 'expected' parameter"),
    };

    let node_id = match resolve_node_target(callback_info, Some(selector), None, None) {
        Some(nid) => nid,
        None => return AssertionResult::fail(format!(
            "assert_text: no node matches selector '{}'", selector
        )),
    };

    // Get text content: try callback_info first, fall back to raw NodeType::Text
    use azul_core::dom::{DomId, DomNodeId};
    let dom_id = DomId { inner: 0 };
    let dom_node_id = DomNodeId {
        dom: dom_id.clone(),
        node: Some(node_id).into(),
    };

    // First try the inline-content path (works for text inputs, editable nodes)
    let actual_text = callback_info
        .get_node_text_content(dom_node_id)
        .or_else(|| {
            // Fallback: read raw NodeType::Text from the styled DOM
            let layout_window = callback_info.get_layout_window();
            let layout_result = layout_window.layout_results.get(&dom_id)?;
            let node_data = layout_result.styled_dom.node_data.as_container();
            if node_id.index() < node_data.len() {
                if let azul_core::dom::NodeType::Text(t) = node_data[node_id].get_node_type() {
                    return Some(t.as_str().to_string());
                }
            }
            None
        })
        .unwrap_or_default();

    if actual_text == expected {
        AssertionResult::pass(format!("assert_text: '{}' matches", selector))
    } else {
        AssertionResult::fail_with(
            format!("assert_text: selector '{}' text mismatch", selector),
            expected,
            actual_text,
        )
    }
}

/// `assert_exists`: assert that at least one node matches `selector`.
#[cfg(feature = "std")]
fn eval_assert_exists(
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> AssertionResult {
    let selector = match params.get("selector").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s,
        _ => return AssertionResult::fail("assert_exists: missing 'selector' parameter"),
    };

    let matches = resolve_all_matching_nodes(callback_info, selector);
    if matches.is_empty() {
        AssertionResult::fail(format!(
            "assert_exists: no node matches selector '{}'", selector
        ))
    } else {
        AssertionResult::pass(format!(
            "assert_exists: '{}' matched {} node(s)", selector, matches.len()
        ))
    }
}

/// `assert_not_exists`: assert that **no** node matches `selector`.
#[cfg(feature = "std")]
fn eval_assert_not_exists(
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> AssertionResult {
    let selector = match params.get("selector").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s,
        _ => return AssertionResult::fail("assert_not_exists: missing 'selector' parameter"),
    };

    let matches = resolve_all_matching_nodes(callback_info, selector);
    if matches.is_empty() {
        AssertionResult::pass(format!(
            "assert_not_exists: '{}' correctly has no matches", selector
        ))
    } else {
        AssertionResult::fail(format!(
            "assert_not_exists: selector '{}' unexpectedly matched {} node(s)",
            selector, matches.len()
        ))
    }
}

/// `assert_node_count`: assert that exactly `expected` nodes match `selector`.
#[cfg(feature = "std")]
fn eval_assert_node_count(
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> AssertionResult {
    let selector = match params.get("selector").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s,
        _ => return AssertionResult::fail("assert_node_count: missing 'selector' parameter"),
    };
    let expected = match params.get("expected").and_then(|v| v.as_u64()) {
        Some(n) => n as usize,
        None => return AssertionResult::fail("assert_node_count: missing or invalid 'expected' (number)"),
    };

    let matches = resolve_all_matching_nodes(callback_info, selector);
    let actual = matches.len();

    if actual == expected {
        AssertionResult::pass(format!(
            "assert_node_count: '{}' has {} node(s)", selector, actual
        ))
    } else {
        AssertionResult::fail_with(
            format!("assert_node_count: selector '{}' count mismatch", selector),
            expected.to_string(),
            actual.to_string(),
        )
    }
}

/// `assert_layout`: assert a layout property (`x`, `y`, `width`, `height`)
/// of the first node matching `selector`. Optional `tolerance` (default 0.5).
#[cfg(feature = "std")]
fn eval_assert_layout(
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> AssertionResult {
    let selector = match params.get("selector").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s,
        _ => return AssertionResult::fail("assert_layout: missing 'selector' parameter"),
    };
    let property = match params.get("property").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return AssertionResult::fail("assert_layout: missing 'property' parameter"),
    };
    let expected: f64 = match params.get("expected").and_then(|v| v.as_f64()) {
        Some(n) => n,
        None => return AssertionResult::fail("assert_layout: missing or non-numeric 'expected'"),
    };
    let tolerance: f64 = params.get("tolerance").and_then(|v| v.as_f64()).unwrap_or(0.5);

    let node_id = match resolve_node_target(callback_info, Some(selector), None, None) {
        Some(nid) => nid,
        None => return AssertionResult::fail(format!(
            "assert_layout: no node matches selector '{}'", selector
        )),
    };

    use azul_core::dom::{DomId, DomNodeId};
    let dom_node_id = DomNodeId {
        dom: DomId { inner: 0 },
        node: Some(node_id).into(),
    };

    let rect = match callback_info.get_node_rect(dom_node_id) {
        Some(r) => r,
        None => return AssertionResult::fail(format!(
            "assert_layout: node '{}' has no layout rect", selector
        )),
    };

    let actual = match property {
        "x" => rect.origin.x as f64,
        "y" => rect.origin.y as f64,
        "width" => rect.size.width as f64,
        "height" => rect.size.height as f64,
        other => return AssertionResult::fail(format!(
            "assert_layout: unknown property '{}' (use x, y, width, height)", other
        )),
    };

    if (actual - expected).abs() <= tolerance {
        AssertionResult::pass(format!(
            "assert_layout: '{}' {} = {:.1} (expected {:.1} ± {:.1})",
            selector, property, actual, expected, tolerance
        ))
    } else {
        AssertionResult::fail_with(
            format!("assert_layout: '{}' {} mismatch", selector, property),
            format!("{:.1} (± {:.1})", expected, tolerance),
            format!("{:.1}", actual),
        )
    }
}

/// `assert_css`: assert a computed CSS property value on the first node
/// matching `selector`.
#[cfg(feature = "std")]
fn eval_assert_css(
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> AssertionResult {
    let selector = match params.get("selector").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s,
        _ => return AssertionResult::fail("assert_css: missing 'selector' parameter"),
    };
    let property = match params.get("property").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return AssertionResult::fail("assert_css: missing 'property' parameter"),
    };
    let expected = match params.get("expected").and_then(|v| v.as_str()) {
        Some(e) => e,
        None => return AssertionResult::fail("assert_css: missing 'expected' parameter"),
    };

    let node_id = match resolve_node_target(callback_info, Some(selector), None, None) {
        Some(nid) => nid,
        None => return AssertionResult::fail(format!(
            "assert_css: no node matches selector '{}'", selector
        )),
    };

    use azul_core::dom::{DomId, DomNodeId};
    use azul_css::props::property::{CssPropertyType, get_css_key_map};

    let dom_node_id = DomNodeId {
        dom: DomId { inner: 0 },
        node: Some(node_id).into(),
    };

    // Try to parse the property name into CssPropertyType
    let key_map = get_css_key_map();
    let prop_type = match CssPropertyType::from_str(property, &key_map) {
        Some(pt) => pt,
        None => return AssertionResult::fail(format!(
            "assert_css: unknown CSS property '{}'", property
        )),
    };

    match callback_info.get_computed_css_property(dom_node_id, prop_type) {
        Some(computed) => {
            let actual = format!("{:?}", computed);
            if actual == expected {
                AssertionResult::pass(format!(
                    "assert_css: '{}' {} = {}", selector, property, actual
                ))
            } else {
                AssertionResult::fail_with(
                    format!("assert_css: '{}' {} mismatch", selector, property),
                    expected,
                    actual,
                )
            }
        }
        None => {
            AssertionResult::fail_with(
                format!("assert_css: property '{}' not set on '{}'", property, selector),
                expected,
                "(not set)",
            )
        }
    }
}

/// `assert_app_state`: assert a field in the serialized application state.
///
/// Uses dot-notation for the `path` parameter, e.g. `"counter"` or
/// `"user.name"`. The `expected` value is compared as a JSON value.
#[cfg(feature = "std")]
fn eval_assert_app_state(
    params: &serde_json::Value,
    app_data: &azul_core::refany::RefAny,
) -> AssertionResult {
    let path = match params.get("path").and_then(|v| v.as_str()) {
        Some(p) if !p.is_empty() => p,
        _ => return AssertionResult::fail("assert_app_state: missing 'path' parameter"),
    };
    let expected = match params.get("expected") {
        Some(v) => v,
        None => return AssertionResult::fail("assert_app_state: missing 'expected' parameter"),
    };

    if !app_data.can_serialize() {
        return AssertionResult::fail(
            "assert_app_state: app_data is not serializable (implement AzSerialize)"
        );
    }

    // Serialize app_data → JSON
    use azul_layout::json::serialize_refany_to_json;
    let json = match serialize_refany_to_json(app_data) {
        Some(j) => j,
        None => return AssertionResult::fail("assert_app_state: serialization returned null"),
    };

    // Parse our internal JSON into serde_json::Value
    let root: serde_json::Value = match serde_json::from_str(&json.to_string().as_str()) {
        Ok(v) => v,
        Err(e) => return AssertionResult::fail(format!(
            "assert_app_state: JSON parse error: {}", e
        )),
    };

    // Navigate the dot-path
    let actual = navigate_json_path(&root, path);
    match actual {
        Some(val) => {
            if val == expected {
                AssertionResult::pass(format!(
                    "assert_app_state: '{}' = {}", path, val
                ))
            } else {
                AssertionResult::fail_with(
                    format!("assert_app_state: '{}' mismatch", path),
                    expected.to_string(),
                    val.to_string(),
                )
            }
        }
        None => AssertionResult::fail_with(
            format!("assert_app_state: path '{}' not found in state", path),
            expected.to_string(),
            "(path not found)",
        ),
    }
}

/// Navigate a dot-separated path in a `serde_json::Value`.
///
/// E.g. `navigate_json_path(root, "user.address.city")` walks
/// `root["user"]["address"]["city"]`. Supports array indices via
/// bracket notation: `"items[0].name"`.
#[cfg(feature = "std")]
fn navigate_json_path<'a>(root: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut current = root;
    for segment in path.split('.') {
        // Handle array index: "items[0]"
        if let Some(bracket_pos) = segment.find('[') {
            let key = &segment[..bracket_pos];
            let idx_str = segment[bracket_pos + 1..].trim_end_matches(']');

            current = current.get(key)?;
            let idx: usize = idx_str.parse().ok()?;
            current = current.get(idx)?;
        } else {
            current = current.get(segment)?;
        }
    }
    Some(current)
}

/// `assert_scroll`: assert the scroll position of a scrollable node.
/// Optional `x`, `y`, `tolerance` (default 1.0).
#[cfg(feature = "std")]
fn eval_assert_scroll(
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> AssertionResult {
    let selector = match params.get("selector").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s,
        _ => return AssertionResult::fail("assert_scroll: missing 'selector' parameter"),
    };
    let tolerance: f64 = params.get("tolerance").and_then(|v| v.as_f64()).unwrap_or(1.0);

    let node_id = match resolve_node_target(callback_info, Some(selector), None, None) {
        Some(nid) => nid,
        None => return AssertionResult::fail(format!(
            "assert_scroll: no node matches selector '{}'", selector
        )),
    };

    use azul_core::dom::{DomId, DomNodeId};
    let dom_id = DomId { inner: 0 };
    let dom_node_id = DomNodeId {
        dom: dom_id.clone(),
        node: Some(node_id).into(),
    };

    let layout_window = callback_info.get_layout_window();
    let scroll_offset = layout_window
        .scroll_manager
        .get_current_offset(dom_id, node_id);

    let offset = match scroll_offset {
        Some(o) => o,
        None => return AssertionResult::fail(format!(
            "assert_scroll: node '{}' is not scrollable or has no scroll state", selector
        )),
    };

    // Check x if specified
    if let Some(expected_x) = params.get("x").and_then(|v| v.as_f64()) {
        let actual_x = offset.x as f64;
        if (actual_x - expected_x).abs() > tolerance {
            return AssertionResult::fail_with(
                format!("assert_scroll: '{}' scroll-x mismatch", selector),
                format!("{:.1} (± {:.1})", expected_x, tolerance),
                format!("{:.1}", actual_x),
            );
        }
    }

    // Check y if specified
    if let Some(expected_y) = params.get("y").and_then(|v| v.as_f64()) {
        let actual_y = offset.y as f64;
        if (actual_y - expected_y).abs() > tolerance {
            return AssertionResult::fail_with(
                format!("assert_scroll: '{}' scroll-y mismatch", selector),
                format!("{:.1} (± {:.1})", expected_y, tolerance),
                format!("{:.1}", actual_y),
            );
        }
    }

    AssertionResult::pass(format!(
        "assert_scroll: '{}' at ({:.1}, {:.1})", selector, offset.x, offset.y
    ))
}

// ==================== Timer Callback ====================

/// Timer callback that processes debug requests.
/// Called every ~16ms when debug mode is enabled.
#[cfg(feature = "std")]
pub extern "C" fn debug_timer_callback(
    mut timer_data: azul_core::refany::RefAny,
    mut timer_info: azul_layout::timer::TimerCallbackInfo,
) -> azul_core::callbacks::TimerCallbackReturn {
    use azul_core::callbacks::{TimerCallbackReturn, Update};
    use azul_core::task::TerminateTimer;

    // Downcast the RefAny to DebugTimerData to get app_data + channel
    let dtd = match timer_data.downcast_ref::<DebugTimerData>() {
        Some(d) => d,
        None => {
            log(
                LogLevel::Error,
                LogCategory::DebugServer,
                "[timer] Failed to downcast DebugTimerData",
                None,
            );
            return TimerCallbackReturn {
                should_update: Update::DoNothing,
                should_terminate: TerminateTimer::Continue,
            };
        }
    };
    let mut app_data = dtd.app_data.clone();
    let component_map = dtd.component_map.clone();
    let request_rx = dtd.request_rx.clone();
    let my_window_id = dtd.window_id.clone();
    drop(dtd);

    // Drain all available requests from the SPMC channel
    let mut needs_update = false;
    let mut processed_count = 0;

    while let Ok(request) = request_rx.try_recv() {
        // Window-targeted routing
        if let Some(ref target_id) = request.window_id {
            if target_id != &my_window_id {
                // Not for us — but SPMC already consumed it.
                // Send error so HTTP thread doesn't hang forever.
                send_err(&request, format!(
                    "Request targeted window '{}' but was consumed by '{}'",
                    target_id, my_window_id
                ));
                continue;
            }
        }

        log(
            LogLevel::Debug,
            LogCategory::DebugServer,
            format!("Processing: {:?}", request.event),
            request.window_id.as_deref(),
        );

        // Pass the app_data and component_map to process_debug_event
        let result = process_debug_event(&request, &mut timer_info.callback_info, &mut app_data, &component_map);
        needs_update = needs_update || result;
        processed_count += 1;
    }

    if processed_count > 0 {
        log(
            LogLevel::Debug,
            LogCategory::DebugServer,
            format!("[timer] Processed {} request(s), needs_update={}", processed_count, needs_update),
            None,
        );
    }

    TimerCallbackReturn {
        should_update: if needs_update {
            Update::RefreshDom
        } else {
            Update::DoNothing
        },
        should_terminate: TerminateTimer::Continue,
    }
}

/// Process a single debug event
#[cfg(feature = "std")]
fn build_clip_analysis(
    items: &[azul_layout::solver3::display_list::DisplayListItem],
) -> ClipChainAnalysis {
    use azul_layout::solver3::display_list::DisplayListItem;

    let mut clip_depth = 0i32;
    let mut scroll_depth = 0i32;
    let mut stacking_depth = 0i32;
    let mut operations = Vec::new();

    for (idx, item) in items.iter().enumerate() {
        let op_info = match item {
            DisplayListItem::PushClip { bounds, .. } => {
                clip_depth += 1;
                Some(ClipOperation {
                    index: idx,
                    op: "PushClip".to_string(),
                    clip_depth,
                    scroll_depth,
                    stacking_depth,
                    bounds: Some(LogicalRectJson {
                        x: bounds.origin.x,
                        y: bounds.origin.y,
                        width: bounds.size.width,
                        height: bounds.size.height,
                    }),
                    content_size: None,
                    scroll_id: None,
                })
            }
            DisplayListItem::PopClip => {
                let op = ClipOperation {
                    index: idx,
                    op: "PopClip".to_string(),
                    clip_depth: clip_depth - 1,
                    scroll_depth,
                    stacking_depth,
                    bounds: None,
                    content_size: None,
                    scroll_id: None,
                };
                clip_depth -= 1;
                Some(op)
            }
            DisplayListItem::PushScrollFrame {
                clip_bounds,
                content_size,
                scroll_id,
            } => {
                scroll_depth += 1;
                Some(ClipOperation {
                    index: idx,
                    op: "PushScrollFrame".to_string(),
                    clip_depth,
                    scroll_depth,
                    stacking_depth,
                    bounds: Some(LogicalRectJson {
                        x: clip_bounds.origin.x,
                        y: clip_bounds.origin.y,
                        width: clip_bounds.size.width,
                        height: clip_bounds.size.height,
                    }),
                    content_size: Some(LogicalSizeJson {
                        width: content_size.width,
                        height: content_size.height,
                    }),
                    scroll_id: Some(*scroll_id),
                })
            }
            DisplayListItem::PopScrollFrame => {
                let op = ClipOperation {
                    index: idx,
                    op: "PopScrollFrame".to_string(),
                    clip_depth,
                    scroll_depth: scroll_depth - 1,
                    stacking_depth,
                    bounds: None,
                    content_size: None,
                    scroll_id: None,
                };
                scroll_depth -= 1;
                Some(op)
            }
            DisplayListItem::PushStackingContext { bounds, .. } => {
                stacking_depth += 1;
                Some(ClipOperation {
                    index: idx,
                    op: "PushStackingContext".to_string(),
                    clip_depth,
                    scroll_depth,
                    stacking_depth,
                    bounds: Some(LogicalRectJson {
                        x: bounds.origin.x,
                        y: bounds.origin.y,
                        width: bounds.size.width,
                        height: bounds.size.height,
                    }),
                    content_size: None,
                    scroll_id: None,
                })
            }
            DisplayListItem::PopStackingContext => {
                let op = ClipOperation {
                    index: idx,
                    op: "PopStackingContext".to_string(),
                    clip_depth,
                    scroll_depth,
                    stacking_depth: stacking_depth - 1,
                    bounds: None,
                    content_size: None,
                    scroll_id: None,
                };
                stacking_depth -= 1;
                Some(op)
            }
            _ => None,
        };

        if let Some(op) = op_info {
            operations.push(op);
        }
    }

    ClipChainAnalysis {
        final_clip_depth: clip_depth,
        final_scroll_depth: scroll_depth,
        final_stacking_depth: stacking_depth,
        balanced: clip_depth == 0 && scroll_depth == 0 && stacking_depth == 0,
        operations,
    }
}

/// Parse a key string to a VirtualKeyCode
#[cfg(feature = "std")]
fn parse_virtual_keycode(key: &str) -> Option<azul_core::window::VirtualKeyCode> {
    use azul_core::window::VirtualKeyCode;
    
    match key.to_lowercase().as_str() {
        // Letters
        "a" => Some(VirtualKeyCode::A),
        "b" => Some(VirtualKeyCode::B),
        "c" => Some(VirtualKeyCode::C),
        "d" => Some(VirtualKeyCode::D),
        "e" => Some(VirtualKeyCode::E),
        "f" => Some(VirtualKeyCode::F),
        "g" => Some(VirtualKeyCode::G),
        "h" => Some(VirtualKeyCode::H),
        "i" => Some(VirtualKeyCode::I),
        "j" => Some(VirtualKeyCode::J),
        "k" => Some(VirtualKeyCode::K),
        "l" => Some(VirtualKeyCode::L),
        "m" => Some(VirtualKeyCode::M),
        "n" => Some(VirtualKeyCode::N),
        "o" => Some(VirtualKeyCode::O),
        "p" => Some(VirtualKeyCode::P),
        "q" => Some(VirtualKeyCode::Q),
        "r" => Some(VirtualKeyCode::R),
        "s" => Some(VirtualKeyCode::S),
        "t" => Some(VirtualKeyCode::T),
        "u" => Some(VirtualKeyCode::U),
        "v" => Some(VirtualKeyCode::V),
        "w" => Some(VirtualKeyCode::W),
        "x" => Some(VirtualKeyCode::X),
        "y" => Some(VirtualKeyCode::Y),
        "z" => Some(VirtualKeyCode::Z),
        
        // Numbers
        "0" | "key0" => Some(VirtualKeyCode::Key0),
        "1" | "key1" => Some(VirtualKeyCode::Key1),
        "2" | "key2" => Some(VirtualKeyCode::Key2),
        "3" | "key3" => Some(VirtualKeyCode::Key3),
        "4" | "key4" => Some(VirtualKeyCode::Key4),
        "5" | "key5" => Some(VirtualKeyCode::Key5),
        "6" | "key6" => Some(VirtualKeyCode::Key6),
        "7" | "key7" => Some(VirtualKeyCode::Key7),
        "8" | "key8" => Some(VirtualKeyCode::Key8),
        "9" | "key9" => Some(VirtualKeyCode::Key9),
        
        // Special keys
        "tab" => Some(VirtualKeyCode::Tab),
        "enter" | "return" => Some(VirtualKeyCode::Return),
        "space" | " " => Some(VirtualKeyCode::Space),
        "escape" | "esc" => Some(VirtualKeyCode::Escape),
        "backspace" | "back" => Some(VirtualKeyCode::Back),
        "delete" => Some(VirtualKeyCode::Delete),
        "insert" => Some(VirtualKeyCode::Insert),
        "home" => Some(VirtualKeyCode::Home),
        "end" => Some(VirtualKeyCode::End),
        "pageup" | "page_up" => Some(VirtualKeyCode::PageUp),
        "pagedown" | "page_down" => Some(VirtualKeyCode::PageDown),
        
        // Arrow keys
        "arrowup" | "up" => Some(VirtualKeyCode::Up),
        "arrowdown" | "down" => Some(VirtualKeyCode::Down),
        "arrowleft" | "left" => Some(VirtualKeyCode::Left),
        "arrowright" | "right" => Some(VirtualKeyCode::Right),
        
        // Function keys
        "f1" => Some(VirtualKeyCode::F1),
        "f2" => Some(VirtualKeyCode::F2),
        "f3" => Some(VirtualKeyCode::F3),
        "f4" => Some(VirtualKeyCode::F4),
        "f5" => Some(VirtualKeyCode::F5),
        "f6" => Some(VirtualKeyCode::F6),
        "f7" => Some(VirtualKeyCode::F7),
        "f8" => Some(VirtualKeyCode::F8),
        "f9" => Some(VirtualKeyCode::F9),
        "f10" => Some(VirtualKeyCode::F10),
        "f11" => Some(VirtualKeyCode::F11),
        "f12" => Some(VirtualKeyCode::F12),
        
        // Modifier keys (for explicit key presses)
        "shift" | "lshift" => Some(VirtualKeyCode::LShift),
        "rshift" => Some(VirtualKeyCode::RShift),
        "ctrl" | "control" | "lctrl" | "lcontrol" => Some(VirtualKeyCode::LControl),
        "rctrl" | "rcontrol" => Some(VirtualKeyCode::RControl),
        "alt" | "lalt" => Some(VirtualKeyCode::LAlt),
        "ralt" => Some(VirtualKeyCode::RAlt),
        "meta" | "super" | "lwin" | "lmeta" => Some(VirtualKeyCode::LWin),
        "rwin" | "rmeta" => Some(VirtualKeyCode::RWin),
        
        _ => None,
    }
}

/// Resolved symbol info from a function pointer address
#[cfg(feature = "std")]
struct ResolvedSymbolInfo {
    symbol_name: Option<String>,
    file_name: Option<String>,
    source_file: Option<String>,
    source_line: Option<u32>,
    hint: Option<String>,
    approximate: bool,
}

/// Resolve a function pointer address to a symbol name and containing
/// library/binary using `dladdr` (macOS/Linux) or Windows APIs.
///
/// This runs inside the process so ASLR is not an issue — the runtime
/// address is exactly what `dladdr` expects. No filesystem scanning,
/// no `backtrace` crate — just a single syscall that returns instantly.
#[cfg(feature = "std")]
fn resolve_function_pointer(address: usize) -> ResolvedSymbolInfo {
    if address == 0 {
        return ResolvedSymbolInfo {
            symbol_name: None,
            file_name: None,
            source_file: None,
            source_line: None,
            hint: None,
            approximate: false,
        };
    }

    let mut symbol_name: Option<String> = None;
    let mut file_name: Option<String> = None;
    let mut source_file: Option<String> = None;
    let mut source_line: Option<u32> = None;
    let mut hint: Option<String> = None;
    let mut approximate = false;

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use std::ffi::CStr;

        #[repr(C)]
        struct DlInfo {
            dli_fname: *const std::os::raw::c_char,
            dli_fbase: *mut std::os::raw::c_void,
            dli_sname: *const std::os::raw::c_char,
            dli_saddr: *mut std::os::raw::c_void,
        }

        extern "C" {
            fn dladdr(addr: *const std::os::raw::c_void, info: *mut DlInfo) -> std::os::raw::c_int;
        }

        unsafe {
            let mut info: DlInfo = std::mem::zeroed();
            let result = dladdr(address as *const std::os::raw::c_void, &mut info);
            if result != 0 {
                if !info.dli_fname.is_null() {
                    file_name = Some(CStr::from_ptr(info.dli_fname).to_string_lossy().into_owned());
                }
                if !info.dli_sname.is_null() {
                    let raw = CStr::from_ptr(info.dli_sname).to_string_lossy().into_owned();
                    // Strip leading underscore (macOS C name-mangling convention)
                    let clean = raw.strip_prefix('_')
                        .unwrap_or(&raw)
                        .to_string();
                    symbol_name = Some(clean);
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;

        extern "system" {
            fn GetModuleHandleExW(
                flags: u32,
                module_name: *const u16,
                module: *mut *mut std::os::raw::c_void,
            ) -> i32;
            fn GetModuleFileNameW(
                module: *mut std::os::raw::c_void,
                filename: *mut u16,
                size: u32,
            ) -> u32;
        }

        // Windows: SymFromAddr would resolve the symbol name but requires
        // dbghelp.dll + SymInitialize. For now just get the module name.
        unsafe {
            let mut module = std::ptr::null_mut();
            let flags = 0x04 | 0x02; // FROM_ADDRESS | UNCHANGED_REFCOUNT
            let ret = GetModuleHandleExW(flags, address as *const u16, &mut module);
            if ret != 0 && !module.is_null() {
                let mut buf = [0u16; 260];
                let len = GetModuleFileNameW(module, buf.as_mut_ptr(), 260);
                if len > 0 {
                    file_name = Some(
                        OsString::from_wide(&buf[..len as usize])
                            .to_string_lossy()
                            .into_owned(),
                    );
                }
            }
        }
    }

    // Heuristic: try to guess source file from symbol name
    // e.g. "azul_core::dom::Dom::new" → "azul-core/src/dom.rs" (approximate)
    if source_file.is_none() {
        if let Some(ref sym) = symbol_name {
            // Strip hash suffix (e.g. "::h1234abcd")
            let clean = if let Some(pos) = sym.rfind("::h") {
                if sym[pos+3..].chars().all(|c| c.is_ascii_hexdigit()) {
                    &sym[..pos]
                } else {
                    sym.as_str()
                }
            } else {
                sym.as_str()
            };
            // Split into crate::module::... → guess crate/src/module.rs
            let parts: Vec<&str> = clean.split("::").collect();
            if parts.len() >= 2 {
                let crate_name = parts[0].replace('_', "-");
                let module = parts[1];
                source_file = Some(format!("{}/src/{}.rs", crate_name, module));
                approximate = true;
                hint = Some("Guessed from symbol name (approximate)".into());
            }
        }
    }

    ResolvedSymbolInfo {
        symbol_name,
        file_name,
        source_file,
        source_line,
        hint,
        approximate,
    }
}

/// Build the component registry from the provided `ComponentMap`.
///
/// For builtin HTML elements the well-known attribute tables are merged
/// in so the debugger inspector can show them.
#[cfg(feature = "std")]
fn build_component_registry(map_ref: &azul_core::xml::ComponentMap) -> ComponentRegistryResponse {
    use azul_core::xml::{ComponentMap, ComponentSource, ChildPolicy};

    let mut libraries = Vec::new();

    for lib in map_ref.libraries.iter() {
        let mut components = Vec::new();

        for def in lib.components.iter() {
            let tag = def.id.name.as_str();

            // --- data model (component-specific attributes) ---
            let mut data_model: Vec<ComponentDataFieldInfo> = def.data_model.fields.as_ref().iter()
                .filter(|f| !matches!(f.field_type, azul_core::xml::ComponentFieldType::Callback { .. }))
                .map(|f| ComponentDataFieldInfo {
                    name: f.name.as_str().to_string(),
                    field_type: field_type_to_string(&f.field_type),
                    default: default_value_to_opt_string(&f.default_value),
                    description: f.description.as_str().to_string(),
                })
                .collect();

            // For builtins, also add tag-specific attributes from the well-known table
            // (in case they weren't already registered as data_model fields)
            if def.source == ComponentSource::Builtin {
                for (attr_name, attr_type) in get_tag_specific_attributes(tag) {
                    if !data_model.iter().any(|f| f.name == attr_name) {
                        data_model.push(ComponentDataFieldInfo {
                            name: attr_name.to_string(),
                            field_type: attr_type.to_string(),
                            default: None,
                            description: String::new(),
                        });
                    }
                }
            }

            // --- universal HTML attributes (separate) ---
            let universal_attributes: Vec<ComponentAttributeInfo> = if def.source == ComponentSource::Builtin {
                get_universal_attributes().into_iter().map(|(name, atype)| ComponentAttributeInfo {
                    name: name.to_string(),
                    attr_type: atype.to_string(),
                    default: None,
                    description: String::new(),
                }).collect()
            } else {
                Vec::new()
            };

            // --- callback slots (extracted from data_model.fields with Callback type) ---
            let callback_slots: Vec<ComponentCallbackSlotInfo> = def.data_model.fields.as_ref().iter()
                .filter_map(|f| {
                    if let azul_core::xml::ComponentFieldType::Callback { ref signature } = f.field_type {
                        Some(ComponentCallbackSlotInfo {
                            name: f.name.as_str().to_string(),
                            callback_type: format!("Callback({})", signature.return_type.as_str()),
                            description: f.description.as_str().to_string(),
                        })
                    } else {
                        None
                    }
                })
                .collect();

            let child_policy_str = match def.child_policy {
                ChildPolicy::NoChildren => "no_children",
                ChildPolicy::AnyChildren => "any_children",
                ChildPolicy::TextOnly => "text_only",
            };

            let source_str = match def.source {
                ComponentSource::Builtin => "builtin",
                ComponentSource::Compiled => "compiled",
                ComponentSource::UserDefined => "user_defined",
            };

            components.push(ComponentInfo {
                tag: tag.to_string(),
                qualified_name: def.id.qualified_name(),
                display_name: def.display_name.as_str().to_string(),
                description: def.description.as_str().to_string(),
                accepts_text: def.accepts_text,
                child_policy: child_policy_str.to_string(),
                source: source_str.to_string(),
                data_model,
                universal_attributes,
                callback_slots,
                example_xml: def.example_xml.as_str().to_string(),
                scoped_css: def.scoped_css.as_str().to_string(),
                template: def.template.as_str().to_string(),
            });
        }

        // Sort components within the library by tag name
        components.sort_by(|a, b| a.tag.cmp(&b.tag));

        // Build data model infos from library-level data models
        let data_models: Vec<DataModelInfo> = lib.data_models.as_ref().iter()
            .map(|dm| DataModelInfo {
                name: dm.name.as_str().to_string(),
                description: dm.description.as_str().to_string(),
                fields: dm.fields.as_ref().iter()
                    .map(|f| ComponentDataFieldInfo {
                        name: f.name.as_str().to_string(),
                        field_type: field_type_to_string(&f.field_type),
                        default: default_value_to_opt_string(&f.default_value),
                        description: f.description.as_str().to_string(),
                    })
                    .collect(),
            })
            .collect();

        libraries.push(ComponentLibraryInfo {
            name: lib.name.as_str().to_string(),
            version: lib.version.as_str().to_string(),
            description: lib.description.as_str().to_string(),
            exportable: lib.exportable,
            modifiable: lib.modifiable,
            data_models,
            components,
        });
    }

    ComponentRegistryResponse { libraries }
}

/// Build exported code for all exportable component libraries.
///
/// Uses `compile_fn` on each exportable component to generate source code
/// in the target language, then packages the result as a set of files.
/// For the "builtin" library this is a no-op (builtin components are not exported).
#[cfg(feature = "std")]
fn build_exported_code(language: &str, map_ref: &azul_core::xml::ComponentMap) -> Result<ExportedCodeResponse, String> {
    use azul_core::xml::{
        ComponentMap, CompileTarget, XmlComponentMap,
        FilteredComponentArguments, ComponentDef,
    };
    use azul_css::corety::OptionString;

    let target = match language {
        "rust" => CompileTarget::Rust,
        "c" => CompileTarget::C,
        "cpp" | "c++" => CompileTarget::Cpp,
        "python" => CompileTarget::Python,
        other => return Err(format!("Unsupported language: '{}'. Use: rust, c, cpp, python", other)),
    };

    let xml_map = XmlComponentMap::default();
    let mut files = std::collections::HashMap::new();
    let mut warnings = Vec::new();

    // Collect all exportable component definitions with their data models
    let exportable = map_ref.get_exportable_libraries();

    // Gather component info for scaffold generation
    let mut component_infos: Vec<ScaffoldComponentInfo> = Vec::new();

    for lib in &exportable {
        for def in lib.components.iter() {
            let args = FilteredComponentArguments::default();
            let text = OptionString::None;

            let compiled_code = match (def.compile_fn)(def, &target, &xml_map, &args, &text, 0) {
                Ok(code) => Some(code),
                Err(e) => {
                    warnings.push(format!(
                        "Failed to compile component '{}': {:?}",
                        def.id.qualified_name(), e
                    ));
                    None
                }
            };

            component_infos.push(ScaffoldComponentInfo {
                name: def.id.name.as_str().to_string(),
                display_name: def.display_name.as_str().to_string(),
                compiled_code,
                data_fields: def.data_model.fields.as_ref().iter()
                    .filter(|f| !matches!(f.field_type, azul_core::xml::ComponentFieldType::Callback { .. }))
                    .map(|f| (
                        f.name.as_str().to_string(),
                        field_type_to_string(&f.field_type),
                        default_value_to_opt_string(&f.default_value),
                    )).collect(),
                callback_slots: def.data_model.fields.as_ref().iter()
                    .filter_map(|f| {
                        if let azul_core::xml::ComponentFieldType::Callback { ref signature } = f.field_type {
                            Some((
                                f.name.as_str().to_string(),
                                format!("Callback({})", signature.return_type.as_str()),
                            ))
                        } else {
                            None
                        }
                    }).collect(),
                parameters: Vec::new(),
            });
        }
    }

    let scaffold_files = generate_scaffold(&target, &component_infos);
    for (filename, content) in scaffold_files {
        files.insert(filename, content);
    }

    if component_infos.is_empty() {
        warnings.push("No user-defined component libraries to export. Generated minimal scaffold.".to_string());
    }

    Ok(ExportedCodeResponse {
        language: language.to_string(),
        files,
        warnings,
    })
}

/// Collected info about a component for scaffold generation
#[cfg(feature = "std")]
struct ScaffoldComponentInfo {
    name: String,
    display_name: String,
    compiled_code: Option<String>,
    data_fields: Vec<(String, String, Option<String>)>,    // (name, type, default)
    callback_slots: Vec<(String, String)>,                  // (name, callback_type)
    parameters: Vec<(String, String, Option<String>)>,      // (name, type, default)
}

/// Convert a `ComponentFieldType` to a JSON-friendly string for the debug protocol.
#[cfg(feature = "std")]
fn field_type_to_string(ft: &azul_core::xml::ComponentFieldType) -> String {
    use azul_core::xml::ComponentFieldType;
    match ft {
        ComponentFieldType::String => "String".to_string(),
        ComponentFieldType::Bool => "bool".to_string(),
        ComponentFieldType::I32 => "i32".to_string(),
        ComponentFieldType::I64 => "i64".to_string(),
        ComponentFieldType::U32 => "u32".to_string(),
        ComponentFieldType::U64 => "u64".to_string(),
        ComponentFieldType::Usize => "usize".to_string(),
        ComponentFieldType::F32 => "f32".to_string(),
        ComponentFieldType::F64 => "f64".to_string(),
        ComponentFieldType::ColorU => "ColorU".to_string(),
        ComponentFieldType::CssProperty => "CssProperty".to_string(),
        ComponentFieldType::ImageRef => "ImageRef".to_string(),
        ComponentFieldType::FontRef => "FontRef".to_string(),
        ComponentFieldType::StyledDom => "StyledDom".to_string(),
        ComponentFieldType::Callback { signature } => format!("Callback({})", signature.return_type.as_str()),
        ComponentFieldType::RefAny { type_hint } => format!("RefAny({})", type_hint.as_str()),
        ComponentFieldType::OptionType { inner } => format!("Option<{}>", field_type_to_string(inner.as_ref())),
        ComponentFieldType::VecType { inner } => format!("Vec<{}>", field_type_to_string(inner.as_ref())),
        ComponentFieldType::StructRef { name } => format!("struct:{}", name.as_str()),
        ComponentFieldType::EnumRef { name } => format!("enum:{}", name.as_str()),
    }
}

/// Convert `OptionComponentDefaultValue` to `Option<String>` for JSON serialization.
#[cfg(feature = "std")]
fn default_value_to_opt_string(dv: &azul_core::xml::OptionComponentDefaultValue) -> Option<String> {
    use azul_core::xml::{ComponentDefaultValue, OptionComponentDefaultValue};
    match dv {
        OptionComponentDefaultValue::None => None,
        OptionComponentDefaultValue::Some(v) => Some(match v {
            ComponentDefaultValue::None => return None,
            ComponentDefaultValue::String(s) => s.as_str().to_string(),
            ComponentDefaultValue::Bool(b) => b.to_string(),
            ComponentDefaultValue::I32(i) => i.to_string(),
            ComponentDefaultValue::I64(i) => i.to_string(),
            ComponentDefaultValue::U32(u) => u.to_string(),
            ComponentDefaultValue::U64(u) => u.to_string(),
            ComponentDefaultValue::Usize(u) => u.to_string(),
            ComponentDefaultValue::F32(f) => f.to_string(),
            ComponentDefaultValue::F64(f) => f.to_string(),
            ComponentDefaultValue::ColorU(c) => format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a),
            ComponentDefaultValue::ComponentInstance(ci) => format!("instance:{}", ci.component.as_str()),
            ComponentDefaultValue::CallbackFnPointer(s) => format!("fn:{}", s.as_str()),
        }),
    }
}

/// Parse a JSON-friendly type string into a `ComponentFieldType`.
/// Returns an error for unrecognized type strings instead of silently falling back.
#[cfg(feature = "std")]
fn parse_field_type_from_string(s: &str) -> Result<azul_core::xml::ComponentFieldType, String> {
    use azul_core::xml::{ComponentFieldType, ComponentCallbackSignature, ComponentCallbackArgVec, ComponentFieldTypeBox};
    use azul_css::corety::AzString;
    match s {
        "String" | "string" => Ok(ComponentFieldType::String),
        "bool" | "Bool" | "boolean" => Ok(ComponentFieldType::Bool),
        "i32" | "I32" | "int" => Ok(ComponentFieldType::I32),
        "i64" | "I64" => Ok(ComponentFieldType::I64),
        "u32" | "U32" => Ok(ComponentFieldType::U32),
        "u64" | "U64" => Ok(ComponentFieldType::U64),
        "usize" | "Usize" => Ok(ComponentFieldType::Usize),
        "f32" | "F32" | "float" => Ok(ComponentFieldType::F32),
        "f64" | "F64" | "double" => Ok(ComponentFieldType::F64),
        "ColorU" | "color" | "Color" => Ok(ComponentFieldType::ColorU),
        "CssProperty" => Ok(ComponentFieldType::CssProperty),
        "ImageRef" | "image" => Ok(ComponentFieldType::ImageRef),
        "FontRef" | "font" => Ok(ComponentFieldType::FontRef),
        "StyledDom" | "dom" | "Dom" => Ok(ComponentFieldType::StyledDom),
        other => {
            if other.starts_with("Callback") {
                Ok(ComponentFieldType::Callback {
                    signature: ComponentCallbackSignature {
                        return_type: AzString::from("Update"),
                        args: ComponentCallbackArgVec::from_const_slice(&[]),
                    },
                })
            } else if other.starts_with("RefAny") {
                let hint = other.strip_prefix("RefAny(").and_then(|s| s.strip_suffix(")"))
                    .ok_or_else(|| format!("Invalid RefAny syntax '{}', expected 'RefAny(TypeHint)'", other))?;
                Ok(ComponentFieldType::RefAny { type_hint: AzString::from(hint) })
            } else if other.starts_with("Option<") {
                let inner = other.strip_prefix("Option<").and_then(|s| s.strip_suffix(">"))
                    .ok_or_else(|| format!("Invalid Option type syntax '{}', expected 'Option<InnerType>'", other))?;
                Ok(ComponentFieldType::OptionType {
                    inner: ComponentFieldTypeBox::new(parse_field_type_from_string(inner)?),
                })
            } else if other.starts_with("Vec<") {
                let inner = other.strip_prefix("Vec<").and_then(|s| s.strip_suffix(">"))
                    .ok_or_else(|| format!("Invalid Vec type syntax '{}', expected 'Vec<InnerType>'", other))?;
                Ok(ComponentFieldType::VecType {
                    inner: ComponentFieldTypeBox::new(parse_field_type_from_string(inner)?),
                })
            } else if other.starts_with("struct:") {
                let name = &other[7..];
                if name.is_empty() {
                    return Err("Empty struct reference name in 'struct:'".to_string());
                }
                Ok(ComponentFieldType::StructRef { name: AzString::from(name) })
            } else if other.starts_with("enum:") {
                let name = &other[5..];
                if name.is_empty() {
                    return Err("Empty enum reference name in 'enum:'".to_string());
                }
                Ok(ComponentFieldType::EnumRef { name: AzString::from(name) })
            } else {
                Err(format!(
                    "Unknown field type '{}'. Valid types: String, bool, i32, i64, u32, u64, usize, \
                     f32, f64, ColorU, CssProperty, ImageRef, FontRef, StyledDom, Callback(...), \
                     RefAny(...), Option<...>, Vec<...>, struct:Name, enum:Name",
                    other
                ))
            }
        }
    }
}

/// Validate a field name: must be a valid identifier (alphanumeric + underscore, not starting with digit).
#[cfg(feature = "std")]
fn validate_field_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Field name cannot be empty".to_string());
    }
    if name.starts_with(|c: char| c.is_ascii_digit()) {
        return Err(format!("Field name '{}' cannot start with a digit", name));
    }
    if !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        return Err(format!(
            "Field name '{}' contains invalid characters (only alphanumeric, underscore, hyphen allowed)",
            name
        ));
    }
    Ok(())
}

/// Validate and convert an `ExportedDataField` into a `ComponentDataField`.
/// Validates field name format, type string, and default value compatibility.
#[cfg(feature = "std")]
fn validate_exported_field(field: &ExportedDataField) -> Result<azul_core::xml::ComponentDataField, String> {
    use azul_core::xml::{ComponentDataField, ComponentDefaultValue, OptionComponentDefaultValue};
    use azul_css::corety::AzString;

    validate_field_name(&field.name)?;

    let field_type = parse_field_type_from_string(&field.field_type)
        .map_err(|e| format!("Field '{}': {}", field.name, e))?;

    let default_value = match &field.default {
        Some(d) => parse_default_value(d, &field_type)
            .map(|v| OptionComponentDefaultValue::Some(v))
            .map_err(|e| format!("Field '{}': invalid default '{}': {}", field.name, d, e))?,
        None => OptionComponentDefaultValue::None,
    };

    Ok(ComponentDataField {
        name: AzString::from(field.name.as_str()),
        field_type,
        default_value,
        required: field.default.is_none(),
        description: AzString::from(field.description.as_str()),
    })
}

/// Parse a default value string according to the declared field type.
#[cfg(feature = "std")]
fn parse_default_value(
    value: &str,
    field_type: &azul_core::xml::ComponentFieldType,
) -> Result<azul_core::xml::ComponentDefaultValue, String> {
    use azul_core::xml::{ComponentDefaultValue, ComponentFieldType};

    match field_type {
        ComponentFieldType::String => Ok(ComponentDefaultValue::String(value.into())),
        ComponentFieldType::Bool => match value {
            "true" | "1" | "yes" => Ok(ComponentDefaultValue::Bool(true)),
            "false" | "0" | "no" => Ok(ComponentDefaultValue::Bool(false)),
            _ => Err(format!("expected bool ('true'/'false'), got '{}'", value)),
        },
        ComponentFieldType::I32 => value.parse::<i32>()
            .map(ComponentDefaultValue::I32)
            .map_err(|e| format!("expected i32: {}", e)),
        ComponentFieldType::I64 => value.parse::<i64>()
            .map(ComponentDefaultValue::I64)
            .map_err(|e| format!("expected i64: {}", e)),
        ComponentFieldType::U32 => value.parse::<u32>()
            .map(ComponentDefaultValue::U32)
            .map_err(|e| format!("expected u32: {}", e)),
        ComponentFieldType::U64 => value.parse::<u64>()
            .map(ComponentDefaultValue::U64)
            .map_err(|e| format!("expected u64: {}", e)),
        ComponentFieldType::Usize => value.parse::<usize>()
            .map(ComponentDefaultValue::Usize)
            .map_err(|e| format!("expected usize: {}", e)),
        ComponentFieldType::F32 => value.parse::<f32>()
            .map(ComponentDefaultValue::F32)
            .map_err(|e| format!("expected f32: {}", e)),
        ComponentFieldType::F64 => value.parse::<f64>()
            .map(ComponentDefaultValue::F64)
            .map_err(|e| format!("expected f64: {}", e)),
        ComponentFieldType::ColorU => {
            // Accept #rrggbb or #rrggbbaa hex strings
            let hex = value.strip_prefix('#').unwrap_or(value);
            if hex.len() == 6 || hex.len() == 8 {
                let r = u8::from_str_radix(&hex[0..2], 16).map_err(|e| format!("invalid color: {}", e))?;
                let g = u8::from_str_radix(&hex[2..4], 16).map_err(|e| format!("invalid color: {}", e))?;
                let b = u8::from_str_radix(&hex[4..6], 16).map_err(|e| format!("invalid color: {}", e))?;
                let a = if hex.len() == 8 {
                    u8::from_str_radix(&hex[6..8], 16).map_err(|e| format!("invalid color: {}", e))?
                } else { 255 };
                Ok(ComponentDefaultValue::ColorU(azul_css::props::basic::color::ColorU { r, g, b, a }))
            } else {
                Err(format!("expected #rrggbb or #rrggbbaa hex color, got '{}'", value))
            }
        }
        ComponentFieldType::Callback { .. } => {
            // Callbacks can have a function pointer name as default
            Ok(ComponentDefaultValue::CallbackFnPointer(value.into()))
        }
        // For complex types (Option, Vec, StructRef, EnumRef, etc.), store as string
        _ => Ok(ComponentDefaultValue::String(value.into())),
    }
}

/// Validate all fields of an exported component definition for uniqueness and correctness.
#[cfg(feature = "std")]
fn validate_exported_fields(fields: &[ExportedDataField]) -> Result<Vec<azul_core::xml::ComponentDataField>, String> {
    let mut seen_names = std::collections::HashSet::new();
    let mut validated = Vec::with_capacity(fields.len());

    for field in fields {
        if !seen_names.insert(field.name.to_lowercase()) {
            return Err(format!("Duplicate field name '{}'", field.name));
        }
        validated.push(validate_exported_field(field)?);
    }

    Ok(validated)
}

/// Convert a snake_case or kebab-case name to PascalCase
#[cfg(feature = "std")]
fn to_pascal_case(s: &str) -> String {
    s.split(|c: char| c == '_' || c == '-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    let mut s = first.to_uppercase().to_string();
                    s.extend(chars);
                    s
                }
                None => String::new(),
            }
        })
        .collect()
}

/// Map component type strings to Rust types
#[cfg(feature = "std")]
fn map_type_to_rust(type_str: &str) -> &str {
    match type_str {
        "String" | "string" => "String",
        "bool" | "Bool" | "boolean" => "bool",
        "i32" | "int" | "Int" => "i32",
        "i64" => "i64",
        "f32" | "float" | "Float" => "f32",
        "f64" | "double" | "Double" => "f64",
        "u32" | "uint" => "u32",
        "u64" => "u64",
        "usize" => "usize",
        _ => "String", // fallback
    }
}

/// Map component type strings to C types
#[cfg(feature = "std")]
fn map_type_to_c(type_str: &str) -> &str {
    match type_str {
        "String" | "string" => "AzString",
        "bool" | "Bool" | "boolean" => "bool",
        "i32" | "int" | "Int" => "int32_t",
        "i64" => "int64_t",
        "f32" | "float" | "Float" => "float",
        "f64" | "double" | "Double" => "double",
        "u32" | "uint" => "uint32_t",
        "u64" => "uint64_t",
        "usize" => "size_t",
        _ => "AzString",
    }
}

/// Generate default value expression for a type in Rust
#[cfg(feature = "std")]
fn rust_default_for_type(type_str: &str, default_val: Option<&str>) -> String {
    if let Some(val) = default_val {
        match type_str {
            "String" | "string" => format!("\"{}\".to_string()", val),
            "bool" | "Bool" | "boolean" => val.to_string(),
            _ => val.to_string(),
        }
    } else {
        match type_str {
            "String" | "string" => "String::new()".to_string(),
            "bool" | "Bool" | "boolean" => "false".to_string(),
            "i32" | "int" | "Int" | "i64" | "u32" | "u64" | "usize" => "0".to_string(),
            "f32" | "float" | "Float" | "f64" | "double" | "Double" => "0.0".to_string(),
            _ => "String::new()".to_string(),
        }
    }
}

/// Generate a project scaffold for the given target language
#[cfg(feature = "std")]
fn generate_scaffold(
    target: &azul_core::xml::CompileTarget,
    components: &[ScaffoldComponentInfo],
) -> Vec<(String, String)> {
    use azul_core::xml::CompileTarget;

    match target {
        CompileTarget::Rust => generate_rust_scaffold(components),
        CompileTarget::C => generate_c_scaffold(components),
        CompileTarget::Cpp => generate_cpp_scaffold(components),
        CompileTarget::Python => generate_python_scaffold(components),
    }
}

/// Generate a complete Rust project scaffold
#[cfg(feature = "std")]
fn generate_rust_scaffold(components: &[ScaffoldComponentInfo]) -> Vec<(String, String)> {
    let mut files = Vec::new();

    // --- Cargo.toml ---
    let cargo_toml = r#"[package]
name = "my-azul-app"
version = "0.1.0"
edition = "2021"

[dependencies]
azul = "0.0.1"
"#;
    files.push(("Cargo.toml".to_string(), cargo_toml.to_string()));

    // --- Build DataModel struct ---
    let mut data_model_fields = String::new();
    let mut data_model_defaults = String::new();
    let mut has_data_fields = false;

    for comp in components {
        for (field_name, field_type, default_val) in &comp.data_fields {
            has_data_fields = true;
            let rust_type = map_type_to_rust(field_type);
            data_model_fields.push_str(&format!("    pub {}: {},\n", field_name, rust_type));
            data_model_defaults.push_str(&format!(
                "            {}: {},\n",
                field_name,
                rust_default_for_type(field_type, default_val.as_deref())
            ));
        }
    }

    if !has_data_fields {
        data_model_fields.push_str("    pub counter: usize,\n");
        data_model_defaults.push_str("            counter: 0,\n");
    }

    // --- Build callback stubs ---
    let mut callback_stubs = String::new();
    for comp in components {
        for (slot_name, _cb_type) in &comp.callback_slots {
            callback_stubs.push_str(&format!(
                r#"
extern "C" fn {slot_name}(data: &mut RefAny, info: &mut CallbackInfo) -> Update {{
    // TODO: implement {slot_name} callback
    Update::DoNothing
}}
"#,
                slot_name = slot_name
            ));
        }
    }

    // --- Build layout function ---
    let mut layout_body = String::new();
    if components.is_empty() {
        layout_body.push_str("    Dom::body()\n");
        layout_body.push_str("        .with_child(Dom::text(\"Hello from Azul!\"))\n");
        layout_body.push_str("        .style(Css::empty())\n");
    } else {
        layout_body.push_str("    Dom::body()\n");
        for comp in components {
            if let Some(ref code) = comp.compiled_code {
                layout_body.push_str(&format!(
                    "        .with_child({}) // {}\n",
                    code, comp.display_name
                ));
            } else {
                layout_body.push_str(&format!(
                    "        .with_child(Dom::div()) // TODO: {} component\n",
                    comp.display_name
                ));
            }
        }
        layout_body.push_str("        .style(Css::empty())\n");
    }

    // --- Assemble main.rs ---
    let main_rs = format!(
        r#"//! Auto-generated by Azul debugger
//! Customize this file to build your application.

extern crate azul;
use azul::prelude::*;

/// Application data model
struct DataModel {{
{fields}}}

impl Default for DataModel {{
    fn default() -> Self {{
        Self {{
{defaults}        }}
    }}
}}
{callbacks}
/// Layout callback — returns the DOM tree for a window
extern "C" fn layout(data: &mut RefAny, _info: &mut LayoutCallbackInfo) -> StyledDom {{
    let _data = match data.downcast_ref::<DataModel>() {{
        Some(d) => d,
        None => return StyledDom::default(),
    }};

{layout_body}}}

fn main() {{
    let app = App::new(RefAny::new(DataModel::default()), AppConfig::new(LayoutSolver::Default));
    let window = WindowCreateOptions::new(layout);
    app.run(window);
}}
"#,
        fields = data_model_fields,
        defaults = data_model_defaults,
        callbacks = callback_stubs,
        layout_body = layout_body,
    );
    files.push(("src/main.rs".to_string(), main_rs));

    files
}

/// Generate a complete C project scaffold
#[cfg(feature = "std")]
fn generate_c_scaffold(components: &[ScaffoldComponentInfo]) -> Vec<(String, String)> {
    let mut files = Vec::new();

    // --- Build DataModel struct ---
    let mut struct_fields = String::new();
    let mut has_data_fields = false;

    for comp in components {
        for (field_name, field_type, _default) in &comp.data_fields {
            has_data_fields = true;
            let c_type = map_type_to_c(field_type);
            struct_fields.push_str(&format!("    {} {};\n", c_type, field_name));
        }
    }

    if !has_data_fields {
        struct_fields.push_str("    size_t counter;\n");
    }

    // --- Build layout body ---
    let mut layout_body = String::new();
    if components.is_empty() {
        layout_body.push_str("    AzDom body = AzDom_body();\n");
        layout_body.push_str("    AzDom text = AzDom_text(AzString_fromConstStr(\"Hello from Azul!\"));\n");
        layout_body.push_str("    AzDom_addChild(&body, text);\n");
        layout_body.push_str("    return AzStyledDom_new(body, AzCss_empty());\n");
    } else {
        layout_body.push_str("    AzDom body = AzDom_body();\n");
        for comp in components {
            if let Some(ref code) = comp.compiled_code {
                layout_body.push_str(&format!("    AzDom_addChild(&body, {}); /* {} */\n", code, comp.display_name));
            } else {
                layout_body.push_str(&format!("    AzDom_addChild(&body, AzDom_div()); /* TODO: {} */\n", comp.display_name));
            }
        }
        layout_body.push_str("    return AzStyledDom_new(body, AzCss_empty());\n");
    }

    // --- Callback stubs ---
    let mut callback_stubs = String::new();
    for comp in components {
        for (slot_name, _cb_type) in &comp.callback_slots {
            callback_stubs.push_str(&format!(
                r#"
AzUpdate {slot_name}(AzRefAny* data, AzCallbackInfo* info) {{
    /* TODO: implement {slot_name} */
    return AzUpdate_DoNothing;
}}
"#,
                slot_name = slot_name
            ));
        }
    }

    let main_c = format!(
        r#"/* Auto-generated by Azul debugger */
#include "azul.h"

typedef struct {{
{fields}}} DataModel;
{callbacks}
AzStyledDom layout(AzRefAny* data, AzLayoutCallbackInfo* info) {{
{layout_body}}}

int main() {{
    DataModel model = {{ 0 }};
    AzRefAny ref_any = AzRefAny_newC(&model, sizeof(DataModel), 0, NULL, NULL, NULL);
    AzAppConfig config = AzAppConfig_new(AzLayoutSolver_Default);
    AzApp* app = AzApp_new(ref_any, config);
    AzWindowCreateOptions w = AzWindowCreateOptions_new(layout);
    AzApp_run(app, w);
    return 0;
}}
"#,
        fields = struct_fields,
        callbacks = callback_stubs,
        layout_body = layout_body,
    );
    files.push(("main.c".to_string(), main_c));
    files
}

/// Generate a complete C++ project scaffold
#[cfg(feature = "std")]
fn generate_cpp_scaffold(components: &[ScaffoldComponentInfo]) -> Vec<(String, String)> {
    let mut files = Vec::new();

    let mut struct_fields = String::new();
    let mut has_data_fields = false;

    for comp in components {
        for (field_name, field_type, default_val) in &comp.data_fields {
            has_data_fields = true;
            let rust_type = map_type_to_rust(field_type);
            let cpp_type = match rust_type {
                "String" => "std::string",
                "bool" => "bool",
                "i32" => "int32_t",
                "i64" => "int64_t",
                "f32" => "float",
                "f64" => "double",
                "u32" => "uint32_t",
                "u64" => "uint64_t",
                "usize" => "size_t",
                other => other,
            };
            let default_str = match default_val.as_deref() {
                Some(v) => format!(" = {}", v),
                None => match rust_type {
                    "String" => String::new(),
                    "bool" => " = false".to_string(),
                    _ => " = 0".to_string(),
                },
            };
            struct_fields.push_str(&format!("    {} {}{};\n", cpp_type, field_name, default_str));
        }
    }

    if !has_data_fields {
        struct_fields.push_str("    size_t counter = 0;\n");
    }

    let mut layout_body = String::new();
    if components.is_empty() {
        layout_body.push_str("    auto body = Dom::body();\n");
        layout_body.push_str("    body.add_child(Dom::text(\"Hello from Azul!\"));\n");
        layout_body.push_str("    return body.style(Css::empty());\n");
    } else {
        layout_body.push_str("    auto body = Dom::body();\n");
        for comp in components {
            if let Some(ref code) = comp.compiled_code {
                layout_body.push_str(&format!("    body.add_child({}); // {}\n", code, comp.display_name));
            } else {
                layout_body.push_str(&format!("    body.add_child(Dom::div()); // TODO: {}\n", comp.display_name));
            }
        }
        layout_body.push_str("    return body.style(Css::empty());\n");
    }

    let main_cpp = format!(
        r#"// Auto-generated by Azul debugger
#include "azul.hpp"
using namespace Azul;

struct DataModel {{
{fields}}};

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {{
{layout_body}}}

int main() {{
    DataModel model;
    auto app = App(RefAny::new(model), AppConfig(LayoutSolver::Default));
    app.run(WindowCreateOptions(layout));
    return 0;
}}
"#,
        fields = struct_fields,
        layout_body = layout_body,
    );
    files.push(("main.cpp".to_string(), main_cpp));
    files
}

/// Generate a complete Python project scaffold
#[cfg(feature = "std")]
fn generate_python_scaffold(components: &[ScaffoldComponentInfo]) -> Vec<(String, String)> {
    let mut files = Vec::new();

    let mut class_fields = String::new();
    let mut has_data_fields = false;

    for comp in components {
        for (field_name, field_type, default_val) in &comp.data_fields {
            has_data_fields = true;
            let default_str = match default_val.as_deref() {
                Some(v) => match field_type.as_str() {
                    "String" | "string" => format!("\"{}\"", v),
                    _ => v.to_string(),
                },
                None => match field_type.as_str() {
                    "String" | "string" => "\"\"".to_string(),
                    "bool" | "Bool" | "boolean" => "False".to_string(),
                    "f32" | "f64" | "float" | "double" | "Float" | "Double" => "0.0".to_string(),
                    _ => "0".to_string(),
                },
            };
            class_fields.push_str(&format!("        self.{} = {}\n", field_name, default_str));
        }
    }

    if !has_data_fields {
        class_fields.push_str("        self.counter = 0\n");
    }

    let mut layout_body = String::new();
    if components.is_empty() {
        layout_body.push_str("    body = Dom.body()\n");
        layout_body.push_str("    body.add_child(Dom.text(\"Hello from Azul!\"))\n");
        layout_body.push_str("    return body.style(Css.empty())\n");
    } else {
        layout_body.push_str("    body = Dom.body()\n");
        for comp in components {
            if let Some(ref code) = comp.compiled_code {
                layout_body.push_str(&format!("    body.add_child({})  # {}\n", code, comp.display_name));
            } else {
                layout_body.push_str(&format!("    body.add_child(Dom.div())  # TODO: {}\n", comp.display_name));
            }
        }
        layout_body.push_str("    return body.style(Css.empty())\n");
    }

    // --- Callback stubs ---
    let mut callback_stubs = String::new();
    for comp in components {
        for (slot_name, _cb_type) in &comp.callback_slots {
            callback_stubs.push_str(&format!(
                r#"
def {slot_name}(data, info):
    """TODO: implement {slot_name}"""
    return Update.DoNothing
"#,
                slot_name = slot_name
            ));
        }
    }

    let main_py = format!(
        r#"# Auto-generated by Azul debugger
from azul import *

class DataModel:
    def __init__(self):
{class_fields}
{callbacks}
def layout(data, info):
{layout_body}

model = DataModel()
app = App(RefAny(model), AppConfig(LayoutSolver.Default))
app.run(WindowCreateOptions(layout))
"#,
        class_fields = class_fields,
        callbacks = callback_stubs,
        layout_body = layout_body,
    );
    files.push(("main.py".to_string(), main_py));
    files
}

/// Returns universal HTML attributes that all elements support
fn get_universal_attributes() -> Vec<(&'static str, &'static str)> {
    vec![
        ("id", "String"),
        ("class", "String"),
        ("style", "String"),
        ("tabindex", "i32"),
        ("contenteditable", "bool"),
        ("draggable", "bool"),
        ("hidden", "bool"),
        ("lang", "String"),
        ("dir", "String"),
        ("title", "String"),
        ("aria-label", "String"),
        ("aria-labelledby", "String"),
        ("aria-describedby", "String"),
        ("role", "String"),
        ("data-*", "String"),
    ]
}

/// Returns tag-specific attributes based on the HTML element type
fn get_tag_specific_attributes(tag: &str) -> Vec<(&'static str, &'static str)> {
    match tag {
        "a" => vec![("href", "String"), ("target", "String"), ("rel", "String")],
        "img" | "image" => vec![("src", "String"), ("alt", "String"), ("width", "String"), ("height", "String")],
        "input" => vec![
            ("type", "String"), ("name", "String"), ("value", "String"),
            ("placeholder", "String"), ("required", "bool"), ("disabled", "bool"),
            ("readonly", "bool"), ("checked", "bool"), ("min", "String"),
            ("max", "String"), ("step", "String"), ("pattern", "String"),
            ("maxlength", "i32"), ("minlength", "i32"), ("autocomplete", "String"),
        ],
        "button" => vec![("type", "String"), ("name", "String"), ("value", "String"), ("disabled", "bool")],
        "form" => vec![("action", "String"), ("method", "String")],
        "label" => vec![("for", "String")],
        "select" => vec![("name", "String"), ("required", "bool"), ("disabled", "bool")],
        "option" => vec![("value", "String"), ("selected", "bool"), ("disabled", "bool")],
        "textarea" => vec![
            ("name", "String"), ("placeholder", "String"), ("rows", "i32"),
            ("cols", "i32"), ("required", "bool"), ("disabled", "bool"),
            ("readonly", "bool"), ("maxlength", "i32"),
        ],
        "td" | "th" => vec![("colspan", "i32"), ("rowspan", "i32"), ("scope", "String")],
        "meta" => vec![("charset", "String"), ("name", "String"), ("content", "String")],
        "link" => vec![("href", "String"), ("rel", "String"), ("type", "String")],
        "script" => vec![("src", "String"), ("type", "String"), ("defer", "bool"), ("async", "bool")],
        "source" => vec![("src", "String"), ("type", "String")],
        "video" | "audio" => vec![("src", "String"), ("controls", "bool"), ("autoplay", "bool"), ("loop", "bool")],
        "canvas" => vec![("width", "String"), ("height", "String")],
        "iframe" => vec![("src", "String"), ("width", "String"), ("height", "String")],
        "icon" => vec![("name", "String")],
        "meter" => vec![("value", "String"), ("min", "String"), ("max", "String"), ("low", "String"), ("high", "String")],
        "progress" => vec![("value", "String"), ("max", "String")],
        _ => vec![],
    }
}

/// Process a single debug event
#[cfg(feature = "std")]
fn process_debug_event(
    request: &DebugRequest,
    callback_info: &mut azul_layout::callbacks::CallbackInfo,
    app_data: &mut azul_core::refany::RefAny,
    component_map: &Arc<Mutex<azul_core::xml::ComponentMap>>,
) -> bool {
    use azul_core::geom::{LogicalPosition, LogicalSize};

    let mut needs_update = false;

    match &request.event {
        DebugEvent::GetState => {
            let window_state = callback_info.get_current_window_state();
            let size = &window_state.size;
            let physical = size.get_physical_size();
            let hidpi = size.get_hidpi_factor();
            let window_id_str = window_state.window_id.as_str();
            
            // Get the focused node from the focus manager
            let focused_node_raw = callback_info.get_focused_node();
            let focused_node = focused_node_raw
                .and_then(|dom_node_id| dom_node_id.node.into_crate_internal())
                .map(|node_id| node_id.index() as u64);

            let snapshot = WindowStateSnapshot {
                window_id: window_id_str.to_string(),
                logical_width: size.dimensions.width,
                logical_height: size.dimensions.height,
                physical_width: physical.width,
                physical_height: physical.height,
                dpi: size.dpi,
                hidpi_factor: hidpi.inner.get(),
                focused: window_state.flags.has_focus,
                dom_node_count: 0,
                focused_node,
            };

            send_ok(request, Some(snapshot), None);
        }

        DebugEvent::Resize { width, height } => {
            log(
                LogLevel::Info,
                LogCategory::Window,
                format!("Resizing to {}x{}", width, height),
                None,
            );

            let mut new_state = callback_info.get_current_window_state().clone();
            new_state.size.dimensions = LogicalSize::new(*width, *height);
            callback_info.modify_window_state(new_state);
            needs_update = true;

            send_ok(request, None, None);
        }

        DebugEvent::MouseMove { x, y } => {
            log(
                LogLevel::Debug,
                LogCategory::EventLoop,
                format!("Debug mouse move to ({}, {})", x, y),
                None,
            );

            let mut new_state = callback_info.get_current_window_state().clone();
            new_state.mouse_state.cursor_position =
                azul_core::window::CursorPosition::InWindow(LogicalPosition { x: *x, y: *y });
            callback_info.modify_window_state(new_state);
            needs_update = true;

            send_ok(request, None, None);
        }

        DebugEvent::MouseDown { x, y, button } => {
            log(
                LogLevel::Debug,
                LogCategory::EventLoop,
                format!("Debug mouse down at ({}, {}) button {:?}", x, y, button),
                None,
            );

            let mut new_state = callback_info.get_current_window_state().clone();
            new_state.mouse_state.cursor_position =
                azul_core::window::CursorPosition::InWindow(LogicalPosition { x: *x, y: *y });
            match button {
                MouseButton::Left => new_state.mouse_state.left_down = true,
                MouseButton::Right => new_state.mouse_state.right_down = true,
                MouseButton::Middle => new_state.mouse_state.middle_down = true,
            }
            callback_info.modify_window_state(new_state);
            needs_update = true;

            // Text selection is now handled automatically by the normal event pipeline.
            // When modify_window_state is called, it triggers apply_user_change
            // which detects mouse_state_changed and calls process_window_events.
            // This generates a TextClick internal event with the correct position from mouse_state.

            send_ok(request, None, None);
        }

        DebugEvent::MouseUp { x, y, button } => {
            log(
                LogLevel::Debug,
                LogCategory::EventLoop,
                format!("Debug mouse up at ({}, {}) button {:?}", x, y, button),
                None,
            );

            let mut new_state = callback_info.get_current_window_state().clone();
            new_state.mouse_state.cursor_position =
                azul_core::window::CursorPosition::InWindow(LogicalPosition { x: *x, y: *y });
            match button {
                MouseButton::Left => new_state.mouse_state.left_down = false,
                MouseButton::Right => new_state.mouse_state.right_down = false,
                MouseButton::Middle => new_state.mouse_state.middle_down = false,
            }
            callback_info.modify_window_state(new_state);
            needs_update = true;

            send_ok(request, None, None);
        }

        DebugEvent::Click {
            x,
            y,
            button,
            selector,
            node_id,
            text,
        } => {
            use azul_core::dom::{DomId, DomNodeId};
            use azul_core::id::NodeId;

            // Resolve the click target position
            let click_pos: Option<(f32, f32)> = if let (Some(x), Some(y)) = (x, y) {
                // Direct position provided
                Some((*x, *y))
            } else if let Some(nid) = node_id {
                // Click by node ID - use hit test bounds from display list
                let dom_id = DomId { inner: 0 };
                let dom_node_id = DomNodeId {
                    dom: dom_id,
                    node: Some(NodeId::new(*nid as usize)).into(),
                };
                if let Some(rect) = callback_info.get_node_hit_test_bounds(dom_node_id) {
                    Some((
                        rect.origin.x + rect.size.width / 2.0,
                        rect.origin.y + rect.size.height / 2.0,
                    ))
                } else {
                    None
                }
            } else if let Some(sel) = selector {
                // Click by CSS selector using matches_html_element
                use azul_core::style::matches_html_element;
                use azul_css::parser2::parse_css_path;

                let dom_id = DomId { inner: 0 };
                let layout_window = callback_info.get_layout_window();
                let mut found = None;

                if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
                    // Parse the CSS selector string into a CssPath
                    if let Ok(css_path) = parse_css_path(sel.as_str()) {
                        let styled_dom = &layout_result.styled_dom;
                        let node_hierarchy = styled_dom.node_hierarchy.as_container();
                        let node_data = styled_dom.node_data.as_container();
                        let cascade_info = styled_dom.cascade_info.as_container();
                        let node_count = node_data.len();

                        // Iterate through all nodes and find the first match
                        for i in 0..node_count {
                            let node_id = NodeId::new(i);
                            if matches_html_element(
                                &css_path,
                                node_id,
                                &node_hierarchy,
                                &node_data,
                                &cascade_info,
                                None, // No expected pseudo-selector
                            ) {
                                let dom_node_id = DomNodeId {
                                    dom: dom_id.clone(),
                                    node: Some(NodeId::new(i)).into(),
                                };
                                // Use get_node_hit_test_bounds for reliable positions from display list
                                if let Some(rect) =
                                    callback_info.get_node_hit_test_bounds(dom_node_id)
                                {
                                    found = Some((
                                        rect.origin.x + rect.size.width / 2.0,
                                        rect.origin.y + rect.size.height / 2.0,
                                    ));
                                    break;
                                }
                            }
                        }
                    }
                }
                found
            } else if let Some(txt) = text {
                // Click by text content
                let dom_id = DomId { inner: 0 };
                let layout_window = callback_info.get_layout_window();
                let mut found = None;

                if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
                    let styled_dom = &layout_result.styled_dom;
                    let node_data = styled_dom.node_data.as_container();
                    let node_count = node_data.len();

                    for i in 0..node_count {
                        let data = &node_data[NodeId::new(i)];
                        if let azul_core::dom::NodeType::Text(t) = data.get_node_type() {
                            if t.as_str().contains(txt.as_str()) {
                                // For text nodes, get the parent's rect (the container)
                                let dom_node_id = DomNodeId {
                                    dom: dom_id.clone(),
                                    node: Some(NodeId::new(i)).into(),
                                };
                                // Try parent first (text nodes might not have rects)
                                let hierarchy = styled_dom.node_hierarchy.as_container();
                                let node_hier = &hierarchy[NodeId::new(i)];
                                let parent_idx = if node_hier.parent > 0 {
                                    node_hier.parent - 1
                                } else {
                                    i
                                };
                                let parent_dom_node_id = DomNodeId {
                                    dom: dom_id.clone(),
                                    node: Some(NodeId::new(parent_idx)).into(),
                                };
                                // Use get_node_hit_test_bounds for reliable positions from display list
                                if let Some(rect) =
                                    callback_info.get_node_hit_test_bounds(parent_dom_node_id)
                                {
                                    found = Some((
                                        rect.origin.x + rect.size.width / 2.0,
                                        rect.origin.y + rect.size.height / 2.0,
                                    ));
                                    break;
                                } else if let Some(rect) =
                                    callback_info.get_node_hit_test_bounds(dom_node_id)
                                {
                                    found = Some((
                                        rect.origin.x + rect.size.width / 2.0,
                                        rect.origin.y + rect.size.height / 2.0,
                                    ));
                                    break;
                                }
                            }
                        }
                    }
                }
                found
            } else {
                None
            };

            match click_pos {
                Some((cx, cy)) => {
                    log(
                        LogLevel::Debug,
                        LogCategory::EventLoop,
                        format!("Debug click at ({}, {}) button {:?}", cx, cy, button),
                        None,
                    );

                    // Click = mouse move + mouse down + mouse up at same position
                    // We use queue_window_state_sequence to ensure each state change
                    // is processed separately, allowing the event system to detect
                    // the transitions (down→up) and trigger the appropriate callbacks.
                    let base_state = callback_info.get_current_window_state().clone();

                    // State 1: Move cursor to position
                    let mut move_state = base_state.clone();
                    move_state.mouse_state.cursor_position =
                        azul_core::window::CursorPosition::InWindow(LogicalPosition {
                            x: cx,
                            y: cy,
                        });

                    // State 2: Mouse button down
                    let mut down_state = move_state.clone();
                    match button {
                        MouseButton::Left => down_state.mouse_state.left_down = true,
                        MouseButton::Right => down_state.mouse_state.right_down = true,
                        MouseButton::Middle => down_state.mouse_state.middle_down = true,
                    }

                    // State 3: Mouse button up (this triggers MouseUp event)
                    let mut up_state = down_state.clone();
                    match button {
                        MouseButton::Left => up_state.mouse_state.left_down = false,
                        MouseButton::Right => up_state.mouse_state.right_down = false,
                        MouseButton::Middle => up_state.mouse_state.middle_down = false,
                    }

                    // Queue all states to be applied in sequence across frames
                    callback_info
                        .queue_window_state_sequence(vec![move_state, down_state, up_state]);
                    needs_update = true;

                    let response = ClickNodeResponse {
                        success: true,
                        message: format!("Clicked at ({:.1}, {:.1})", cx, cy),
                    };
                    send_ok(request, None, Some(ResponseData::ClickNode(response)));
                }
                None => {
                    let response = ClickNodeResponse {
                        success: false,
                        message: "Could not resolve click target (no matching node or position)"
                            .to_string(),
                    };
                    send_ok(request, None, Some(ResponseData::ClickNode(response)));
                }
            }
        }

        DebugEvent::DoubleClick { x, y, button } => {
            log(
                LogLevel::Debug,
                LogCategory::EventLoop,
                format!("Debug double click at ({}, {}) button {:?}", x, y, button),
                None,
            );

            // For double click, we set the position and rely on timing
            // In practice, we just do a click for now
            let mut new_state = callback_info.get_current_window_state().clone();
            new_state.mouse_state.cursor_position =
                azul_core::window::CursorPosition::InWindow(LogicalPosition { x: *x, y: *y });
            match button {
                MouseButton::Left => {
                    new_state.mouse_state.left_down = true;
                    callback_info.modify_window_state(new_state.clone());
                    new_state.mouse_state.left_down = false;
                }
                MouseButton::Right => {
                    new_state.mouse_state.right_down = true;
                    callback_info.modify_window_state(new_state.clone());
                    new_state.mouse_state.right_down = false;
                }
                MouseButton::Middle => {
                    new_state.mouse_state.middle_down = true;
                    callback_info.modify_window_state(new_state.clone());
                    new_state.mouse_state.middle_down = false;
                }
            }
            callback_info.modify_window_state(new_state);
            needs_update = true;

            send_ok(request, None, None);
        }

        DebugEvent::Scroll {
            x,
            y,
            delta_x,
            delta_y,
        } => {
            use azul_core::dom::DomId;
            use azul_core::id::NodeId;
            use azul_core::styled_dom::NodeHierarchyItemId;

            log(
                LogLevel::Debug,
                LogCategory::EventLoop,
                format!(
                    "Debug scroll at ({}, {}) delta ({}, {})",
                    x, y, delta_x, delta_y
                ),
                None,
            );

            // Update cursor position
            let mut new_state = callback_info.get_current_window_state().clone();
            new_state.mouse_state.cursor_position =
                azul_core::window::CursorPosition::InWindow(LogicalPosition { x: *x, y: *y });
            callback_info.modify_window_state(new_state);

            // Find scrollable node that contains the point (x, y)
            // We iterate through scroll manager states and check if the point is inside
            let layout_window = callback_info.get_layout_window();
            let cursor_pos = LogicalPosition { x: *x, y: *y };
            
            let mut scroll_node: Option<(DomId, NodeId)> = None;
            for (dom_id, layout_result) in &layout_window.layout_results {
                for (scroll_id, &node_id) in &layout_result.scroll_id_to_node_id {
                    // Get node bounds from layout tree
                    if let Some(layout_indices) = layout_result.layout_tree.dom_to_layout.get(&node_id) {
                        if let Some(&layout_idx) = layout_indices.first() {
                            if let Some(layout_node) = layout_result.layout_tree.get(layout_idx) {
                                let node_pos = layout_result
                                    .calculated_positions
                    .get(layout_idx)
                                    .copied()
                                    .unwrap_or_default();
                                let node_size = layout_node.used_size.unwrap_or_default();
                                
                                // Check if cursor is inside this node
                                if cursor_pos.x >= node_pos.x
                                    && cursor_pos.x <= node_pos.x + node_size.width
                                    && cursor_pos.y >= node_pos.y
                                    && cursor_pos.y <= node_pos.y + node_size.height
                                {
                                    scroll_node = Some((*dom_id, node_id));
                                    break;
                                }
                            }
                        }
                    }
                }
                if scroll_node.is_some() {
                    break;
                }
            }

            if let Some((dom_id, node_id)) = scroll_node {
                let current = callback_info
                    .get_scroll_offset_for_node(dom_id, node_id)
                    .unwrap_or(LogicalPosition { x: 0.0, y: 0.0 });
                let new_pos = LogicalPosition {
                    x: current.x + *delta_x,
                    y: current.y + *delta_y,
                };
                let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
                callback_info.scroll_to(dom_id, hierarchy_id, new_pos);
                log(
                    LogLevel::Debug,
                    LogCategory::EventLoop,
                    format!(
                        "Scrolled node {:?}/{:?} from ({:.1}, {:.1}) to ({:.1}, {:.1})",
                        dom_id, node_id, current.x, current.y, new_pos.x, new_pos.y
                    ),
                    None,
                );
            } else {
                log(
                    LogLevel::Debug,
                    LogCategory::EventLoop,
                    format!("No scrollable node found at ({}, {})", x, y),
                    None,
                );
            }
            needs_update = true;

            send_ok(request, None, None);
        }

        DebugEvent::Relayout => {
            log(
                LogLevel::Info,
                LogCategory::Layout,
                "Forcing relayout",
                None,
            );
            needs_update = true;
            send_ok(request, None, None);
        }

        DebugEvent::Redraw => {
            log(
                LogLevel::Info,
                LogCategory::Rendering,
                "Requesting redraw",
                None,
            );
            needs_update = true;
            send_ok(request, None, None);
        }

        DebugEvent::Close => {
            log(
                LogLevel::Info,
                LogCategory::EventLoop,
                "Close via close_window()",
                None,
            );
            callback_info.close_window();
            needs_update = true;
            send_ok(request, None, None);
        }

        DebugEvent::HitTest { x, y } => {
            use azul_core::dom::{DomId, DomNodeId};
            use azul_core::id::NodeId;

            let mut result_node_id: Option<u64> = None;
            let mut result_tag: Option<String> = None;

            // Iterate all nodes and find the deepest one whose bounds contain (x, y).
            // Later nodes in the tree (higher NodeId) that are nested deeper will
            // naturally be the "topmost" rendered element at that point.
            let dom_id = DomId { inner: 0 };
            let layout_window = callback_info.get_layout_window();

            if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
                let node_count = layout_result.styled_dom.node_data.as_container().len();

                for i in 0..node_count {
                    let node_id = NodeId::new(i);
                    let dom_node_id = DomNodeId {
                        dom: dom_id,
                        node: Some(node_id).into(),
                    };

                    if let Some(rect) = callback_info.get_node_hit_test_bounds(dom_node_id) {
                        let px = *x;
                        let py = *y;
                        if px >= rect.origin.x
                            && px <= rect.origin.x + rect.size.width
                            && py >= rect.origin.y
                            && py <= rect.origin.y + rect.size.height
                        {
                            result_node_id = Some(i as u64);
                            result_tag = callback_info
                                .get_node_tag_name(dom_node_id)
                                .map(|s| s.as_str().to_string());
                        }
                    }
                }
            }

            let response = HitTestResponse {
                x: *x,
                y: *y,
                node_id: result_node_id,
                node_tag: result_tag,
            };
            send_ok(request, None, Some(ResponseData::HitTest(response)));
        }

        DebugEvent::GetLogs { .. } => {
            let logs = take_logs();
            send_ok(
                request,
                None,
                Some(ResponseData::Logs(LogsResponse { logs })),
            );
        }

        DebugEvent::WaitFrame => {
            send_ok(request, None, None);
        }

        DebugEvent::Wait { ms } => {
            std::thread::sleep(std::time::Duration::from_millis(*ms));
            send_ok(request, None, None);
        }

        DebugEvent::TakeScreenshot => {
            log(
                LogLevel::Info,
                LogCategory::Rendering,
                "Taking CPU screenshot via debug API",
                None,
            );
            // Use DomId(0) as default - first DOM in the window
            let dom_id = azul_core::dom::DomId { inner: 0 };
            match callback_info.take_screenshot_base64(dom_id) {
                Ok(data_uri) => {
                    let data = ScreenshotData {
                        data: data_uri.as_str().to_string(),
                    };
                    send_ok(request, None, Some(ResponseData::Screenshot(data)));
                }
                Err(e) => {
                    send_err(request, e.as_str().to_string());
                }
            }
        }

        DebugEvent::TakeNativeScreenshot => {
            log(
                LogLevel::Info,
                LogCategory::Rendering,
                "Taking native screenshot via debug API",
                None,
            );
            // Use the NativeScreenshotExt trait method explicitly (not the stubbed inherent method)
            match NativeScreenshotExt::take_native_screenshot_base64(callback_info) {
                Ok(data_uri) => {
                    let data = ScreenshotData {
                        data: data_uri.as_str().to_string(),
                    };
                    send_ok(request, None, Some(ResponseData::Screenshot(data)));
                }
                Err(e) => {
                    send_err(request, e.as_str().to_string());
                }
            }
        }

        DebugEvent::GetHtmlString => {
            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                "Getting HTML string",
                None,
            );
            let dom_id = azul_core::dom::DomId { inner: 0 };
            let layout_window = callback_info.get_layout_window();
            if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
                let html = layout_result.styled_dom.get_html_string("", "", true);
                send_ok(
                    request,
                    None,
                    Some(ResponseData::HtmlString(HtmlStringResponse { html })),
                );
            } else {
                send_err(request, "No layout result for DOM 0");
            }
        }

        DebugEvent::GetNodeCssProperties {
            node_id,
            selector,
            text,
        } => {
            use azul_core::dom::{DomId, DomNodeId, NodeId};
            use azul_css::props::property::CssPropertyType;
            use strum::IntoEnumIterator;

            let resolved_node_id = resolve_node_target(
                callback_info,
                selector.as_deref(),
                *node_id,
                text.as_deref(),
            );

            let nid = match resolved_node_id {
                Some(n) => n.index() as u64,
                None => {
                    send_err(request, "No node found matching the specified target");
                    return needs_update;
                }
            };

            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                format!("Getting CSS properties for node {}", nid),
                None,
            );

            let dom_node_id = DomNodeId {
                dom: DomId { inner: 0 },
                node: Some(NodeId::new(nid as usize)).into(),
            };

            // Collect all CSS properties that are set on this node
            let mut props = Vec::new();

            // Iterate over all CSS property types
            for prop_type in CssPropertyType::iter() {
                if let Some(prop) = callback_info.get_computed_css_property(dom_node_id, prop_type)
                {
                    props.push(format!("{}: {}", prop.key(), prop.value()));
                }
            }

            let response = NodeCssPropertiesResponse {
                node_id: nid,
                property_count: props.len(),
                properties: props,
            };
            send_ok(
                request,
                None,
                Some(ResponseData::NodeCssProperties(response)),
            );
        }

        DebugEvent::GetNodeLayout {
            node_id,
            selector,
            text,
        } => {
            use azul_core::dom::{DomId, DomNodeId, NodeId};

            let resolved_node_id = resolve_node_target(
                callback_info,
                selector.as_deref(),
                *node_id,
                text.as_deref(),
            );

            let nid = match resolved_node_id {
                Some(n) => n.index() as u64,
                None => {
                    send_err(request, "No node found matching the specified target");
                    return needs_update;
                }
            };

            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                format!("Getting layout for node {}", nid),
                None,
            );

            let dom_node_id = DomNodeId {
                dom: DomId { inner: 0 },
                node: Some(NodeId::new(nid as usize)).into(),
            };

            let size = callback_info.get_node_size(dom_node_id);
            let pos = callback_info.get_node_position(dom_node_id);
            let rect = callback_info.get_node_rect(dom_node_id);

            let response = NodeLayoutResponse {
                node_id: nid,
                size: size.map(|s| LogicalSizeJson {
                    width: s.width,
                    height: s.height,
                }),
                position: pos.map(|p| LogicalPositionJson { x: p.x, y: p.y }),
                rect: rect.map(|r| LogicalRectJson {
                    x: r.origin.x,
                    y: r.origin.y,
                    width: r.size.width,
                    height: r.size.height,
                }),
            };
            send_ok(request, None, Some(ResponseData::NodeLayout(response)));
        }

        DebugEvent::GetAllNodesLayout => {
            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                "Getting all nodes layout",
                None,
            );
            use azul_core::dom::{DomId, DomNodeId, NodeId};

            let dom_id = DomId { inner: 0 };
            let layout_window = callback_info.get_layout_window();

            let mut nodes = Vec::new();
            if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
                let node_count = layout_result.styled_dom.node_data.len();
                for i in 0..node_count {
                    let dom_node_id = DomNodeId {
                        dom: dom_id.clone(),
                        node: Some(NodeId::new(i)).into(),
                    };

                    let rect = callback_info.get_node_rect(dom_node_id);
                    let tag = callback_info.get_node_tag_name(dom_node_id);
                    let id_attr = callback_info.get_node_id(dom_node_id);
                    let classes = callback_info.get_node_classes(dom_node_id);

                    nodes.push(NodeLayoutInfo {
                        node_id: i,
                        tag: tag.map(|s| s.as_str().to_string()),
                        id: id_attr.map(|s| s.as_str().to_string()),
                        classes: classes
                            .as_ref()
                            .iter()
                            .map(|s| s.as_str().to_string())
                            .collect(),
                        rect: rect.map(|r| LogicalRectJson {
                            x: r.origin.x,
                            y: r.origin.y,
                            width: r.size.width,
                            height: r.size.height,
                        }),
                    });
                }
            }

            let response = AllNodesLayoutResponse {
                dom_id: 0,
                node_count: nodes.len(),
                nodes,
            };
            send_ok(request, None, Some(ResponseData::AllNodesLayout(response)));
        }

        DebugEvent::GetDomTree => {
            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                "Getting DOM tree",
                None,
            );
            use azul_core::dom::DomId;

            let dom_id = DomId { inner: 0 };
            let layout_window = callback_info.get_layout_window();

            if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
                let styled_dom = &layout_result.styled_dom;
                let window_state = callback_info.get_current_window_state();

                let node_count = styled_dom.node_data.len();
                let dpi = window_state.size.dpi;
                let hidpi = window_state.size.get_hidpi_factor().inner.get();
                let logical_size = &window_state.size.dimensions;

                let response = DomTreeResponse {
                    dom_id: 0,
                    node_count,
                    dpi,
                    hidpi_factor: hidpi,
                    logical_width: logical_size.width,
                    logical_height: logical_size.height,
                };
                send_ok(request, None, Some(ResponseData::DomTree(response)));
            } else {
                send_err(request, "No layout result for DOM 0");
            }
        }

        DebugEvent::GetNodeHierarchy => {
            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                "Getting node hierarchy",
                None,
            );
            use azul_core::dom::DomId;
            use azul_core::id::NodeId;

            let dom_id = DomId { inner: 0 };
            let layout_window = callback_info.get_layout_window();

            if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
                let styled_dom = &layout_result.styled_dom;
                let hierarchy = styled_dom.node_hierarchy.as_container();
                let node_data = styled_dom.node_data.as_container();

                let root_decoded = styled_dom
                    .root
                    .into_crate_internal()
                    .map(|n| n.index() as i64)
                    .unwrap_or(-1);

                let mut nodes = Vec::new();
                for i in 0..hierarchy.len() {
                    let node_id = NodeId::new(i);
                    let hier = &hierarchy[node_id];
                    let data = &node_data[node_id];

                    let node_type = data.get_node_type().get_path().to_string();

                    // Extract tag name from node type
                    let tag = Some(node_type.clone());

                    // Extract ID and classes from ids_and_classes
                    let mut id_attr = None;
                    let mut classes = Vec::new();
                    for ioc in data.ids_and_classes.as_ref().iter() {
                        match ioc {
                            azul_core::dom::IdOrClass::Id(s) => { id_attr = Some(s.as_str().to_string()); }
                            azul_core::dom::IdOrClass::Class(s) => { classes.push(s.as_str().to_string()); }
                        }
                    }

                    let text_content = match data.get_node_type() {
                        azul_core::dom::NodeType::Text(t) => {
                            let s = t.as_str();
                            if s.len() > 200 {
                                Some(format!("{}...", &s[..197]))
                            } else {
                                Some(s.to_string())
                            }
                        }
                        _ => None,
                    };

                    let parent_decoded = if hier.parent == 0 {
                        -1i64
                    } else {
                        (hier.parent - 1) as i64
                    };
                    let children: Vec<usize> =
                        node_id.az_children(&hierarchy).map(|c| c.index()).collect();

                    // Extract event handlers
                    let events: Vec<NodeEventInfo> = data.callbacks.as_ref().iter().map(|cb| {
                        NodeEventInfo {
                            event: format!("{:?}", cb.event),
                            callback_ptr: format!("0x{:x}", cb.callback.cb),
                        }
                    }).collect();

                    // Get layout rect
                    let dom_node_id = azul_core::dom::DomNodeId {
                        dom: dom_id.clone(),
                        node: Some(NodeId::new(i)).into(),
                    };
                    let rect = callback_info.get_node_rect(dom_node_id).map(|r| LogicalRectJson {
                        x: r.origin.x,
                        y: r.origin.y,
                        width: r.size.width,
                        height: r.size.height,
                    });

                    // Tab index
                    let tab_index = match data.tab_index {
                        azul_core::dom::OptionTabIndex::Some(ti) => match ti {
                            azul_core::dom::TabIndex::Auto => Some(0),
                            azul_core::dom::TabIndex::OverrideInParent(v) => Some(v as i32),
                            azul_core::dom::TabIndex::NoKeyboardFocus => Some(-1),
                        },
                        azul_core::dom::OptionTabIndex::None => None,
                    };

                    nodes.push(HierarchyNodeInfo {
                        index: i,
                        node_type: node_type.to_string(),
                        tag,
                        id: id_attr,
                        classes,
                        text: text_content,
                        parent: parent_decoded,
                        children,
                        events,
                        rect,
                        tab_index,
                        contenteditable: data.contenteditable,
                    });
                }

                let response = NodeHierarchyResponse {
                    root: root_decoded,
                    node_count: nodes.len(),
                    nodes,
                };
                send_ok(request, None, Some(ResponseData::NodeHierarchy(response)));
            } else {
                send_err(request, "No layout result for DOM 0");
            }
        }

        DebugEvent::GetLayoutTree => {
            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                "Getting layout tree",
                None,
            );
            use azul_core::dom::DomId;

            let dom_id = DomId { inner: 0 };
            let layout_window = callback_info.get_layout_window();

            if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
                let layout_tree = &layout_result.layout_tree;

                let mut nodes = Vec::new();
                for (idx, node) in layout_tree.nodes.iter().enumerate() {
                    let (node_type, dom_idx) = if let Some(dom_id) = node.dom_node_id {
                        let node_data = &layout_result.styled_dom.node_data.as_container()[dom_id];
                        let nt = match node_data.get_node_type() {
                            azul_core::dom::NodeType::Html => "Html",
                            azul_core::dom::NodeType::Body => "Body",
                            azul_core::dom::NodeType::Div => "Div",
                            azul_core::dom::NodeType::Span => "Span",
                            azul_core::dom::NodeType::P => "P",
                            azul_core::dom::NodeType::Text(_) => "Text",
                            azul_core::dom::NodeType::Image(_) => "Image",
                            _ => "Other",
                        };
                        (nt, dom_id.index() as i64)
                    } else {
                        ("Anonymous", -1i64)
                    };

                    nodes.push(LayoutNodeInfo {
                        layout_idx: idx,
                        dom_idx,
                        node_type: node_type.to_string(),
                        is_anonymous: node.is_anonymous,
                        anonymous_type: node.anonymous_type.as_ref().map(|t| format!("{:?}", t)),
                        formatting_context: format!("{:?}", node.formatting_context),
                        parent: node.parent.map(|p| p as i64).unwrap_or(-1),
                        children: node.children.clone(),
                    });
                }

                let response = LayoutTreeResponse {
                    root: layout_tree.root,
                    node_count: nodes.len(),
                    nodes,
                };
                send_ok(request, None, Some(ResponseData::LayoutTree(response)));
            } else {
                send_err(request, "No layout result for DOM 0");
            }
        }

        DebugEvent::GetDisplayList => {
            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                "Getting display list",
                None,
            );
            use azul_core::dom::DomId;

            let dom_id = DomId { inner: 0 };
            let layout_window = callback_info.get_layout_window();

            if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
                let display_list = &layout_result.display_list;
                let items_list = &display_list.items;

                // Count item types
                let mut rect_count = 0;
                let mut text_count = 0;
                let mut border_count = 0;
                let mut image_count = 0;
                let mut other_count = 0;

                // Track clip/scroll depths for each item
                let mut clip_depth = 0i32;
                let mut scroll_depth = 0i32;

                let mut items = Vec::new();

                for (idx, item) in items_list.iter().enumerate() {
                    // Track depth changes BEFORE creating item info
                    match item {
                        azul_layout::solver3::display_list::DisplayListItem::PushClip {
                            ..
                        } => {
                            clip_depth += 1;
                        }
                        azul_layout::solver3::display_list::DisplayListItem::PopClip => {
                            clip_depth -= 1;
                        }
                        azul_layout::solver3::display_list::DisplayListItem::PushScrollFrame {
                            ..
                        } => {
                            scroll_depth += 1;
                        }
                        azul_layout::solver3::display_list::DisplayListItem::PopScrollFrame => {
                            scroll_depth -= 1;
                        }
                        _ => {}
                    }
                    let info = match item {
                        azul_layout::solver3::display_list::DisplayListItem::Rect { bounds, color, .. } => {
                            rect_count += 1;
                            DisplayListItemInfo {
                                index: idx,
                                item_type: "rect".to_string(),
                                x: Some(bounds.origin.x),
                                y: Some(bounds.origin.y),
                                width: Some(bounds.size.width),
                                height: Some(bounds.size.height),
                                color: Some(format!("#{:02x}{:02x}{:02x}{:02x}", color.r, color.g, color.b, color.a)),
                                font_size: None,
                                glyph_count: None,
                                z_index: None,
                                clip_depth: Some(clip_depth),
                                scroll_depth: Some(scroll_depth),
                                content_size: None,
                                scroll_id: None,
                                debug_info: None,
                                border_colors: None,
                                border_widths: None,
                            }
                        }
                        azul_layout::solver3::display_list::DisplayListItem::Text { glyphs, font_size_px, color, clip_rect, .. } => {
                            text_count += 1;
                            DisplayListItemInfo {
                                index: idx,
                                item_type: "text".to_string(),
                                x: Some(clip_rect.origin.x),
                                y: Some(clip_rect.origin.y),
                                width: Some(clip_rect.size.width),
                                height: Some(clip_rect.size.height),
                                color: Some(format!("#{:02x}{:02x}{:02x}{:02x}", color.r, color.g, color.b, color.a)),
                                font_size: Some(*font_size_px),
                                glyph_count: Some(glyphs.len()),
                                z_index: None,
                                clip_depth: Some(clip_depth),
                                scroll_depth: Some(scroll_depth),
                                content_size: None,
                                scroll_id: None,
                                debug_info: None,
                                border_colors: None,
                                border_widths: None,
                            }
                        }
                        azul_layout::solver3::display_list::DisplayListItem::TextLayout { bounds, font_size_px, color, .. } => {
                            text_count += 1;
                            DisplayListItemInfo {
                                index: idx,
                                item_type: "text_layout".to_string(),
                                x: Some(bounds.origin.x),
                                y: Some(bounds.origin.y),
                                width: Some(bounds.size.width),
                                height: Some(bounds.size.height),
                                color: Some(format!("#{:02x}{:02x}{:02x}{:02x}", color.r, color.g, color.b, color.a)),
                                font_size: Some(*font_size_px),
                                glyph_count: None,
                                z_index: None,
                                clip_depth: Some(clip_depth),
                                scroll_depth: Some(scroll_depth),
                                content_size: None,
                                scroll_id: None,
                                debug_info: None,
                                border_colors: None,
                                border_widths: None,
                            }
                        }
                        azul_layout::solver3::display_list::DisplayListItem::Border { bounds, colors, widths, .. } => {
                            border_count += 1;
                            // Extract border colors from each side
                            let extract_top_color = colors.top.as_ref().and_then(|c| match c {
                                azul_css::css::CssPropertyValue::Exact(c) => Some(format!("#{:02x}{:02x}{:02x}{:02x}", c.inner.r, c.inner.g, c.inner.b, c.inner.a)),
                                _ => None,
                            });
                            let extract_right_color = colors.right.as_ref().and_then(|c| match c {
                                azul_css::css::CssPropertyValue::Exact(c) => Some(format!("#{:02x}{:02x}{:02x}{:02x}", c.inner.r, c.inner.g, c.inner.b, c.inner.a)),
                                _ => None,
                            });
                            let extract_bottom_color = colors.bottom.as_ref().and_then(|c| match c {
                                azul_css::css::CssPropertyValue::Exact(c) => Some(format!("#{:02x}{:02x}{:02x}{:02x}", c.inner.r, c.inner.g, c.inner.b, c.inner.a)),
                                _ => None,
                            });
                            let extract_left_color = colors.left.as_ref().and_then(|c| match c {
                                azul_css::css::CssPropertyValue::Exact(c) => Some(format!("#{:02x}{:02x}{:02x}{:02x}", c.inner.r, c.inner.g, c.inner.b, c.inner.a)),
                                _ => None,
                            });
                            // Extract border widths from each side
                            let extract_top_width = widths.top.as_ref().and_then(|c| match c {
                                azul_css::css::CssPropertyValue::Exact(c) => Some(c.inner.to_pixels_internal(0.0, 16.0)),
                                _ => None,
                            });
                            let extract_right_width = widths.right.as_ref().and_then(|c| match c {
                                azul_css::css::CssPropertyValue::Exact(c) => Some(c.inner.to_pixels_internal(0.0, 16.0)),
                                _ => None,
                            });
                            let extract_bottom_width = widths.bottom.as_ref().and_then(|c| match c {
                                azul_css::css::CssPropertyValue::Exact(c) => Some(c.inner.to_pixels_internal(0.0, 16.0)),
                                _ => None,
                            });
                            let extract_left_width = widths.left.as_ref().and_then(|c| match c {
                                azul_css::css::CssPropertyValue::Exact(c) => Some(c.inner.to_pixels_internal(0.0, 16.0)),
                                _ => None,
                            });
                            let border_colors = BorderColorsJson {
                                top: extract_top_color.clone(),
                                right: extract_right_color,
                                bottom: extract_bottom_color,
                                left: extract_left_color,
                            };
                            let border_widths = BorderWidthsJson {
                                top: extract_top_width,
                                right: extract_right_width,
                                bottom: extract_bottom_width,
                                left: extract_left_width,
                            };
                            // Use top color as main color for backwards compatibility
                            let color_str = extract_top_color;
                            DisplayListItemInfo {
                                index: idx,
                                item_type: "border".to_string(),
                                x: Some(bounds.origin.x),
                                y: Some(bounds.origin.y),
                                width: Some(bounds.size.width),
                                height: Some(bounds.size.height),
                                color: color_str,
                                font_size: None,
                                glyph_count: None,
                                z_index: None,
                                clip_depth: Some(clip_depth),
                                scroll_depth: Some(scroll_depth),
                                content_size: None,
                                scroll_id: None,
                                debug_info: None,
                                border_colors: Some(border_colors),
                                border_widths: Some(border_widths),
                            }
                        }
                        azul_layout::solver3::display_list::DisplayListItem::Image { bounds, .. } => {
                            image_count += 1;
                            DisplayListItemInfo {
                                index: idx,
                                item_type: "image".to_string(),
                                x: Some(bounds.origin.x),
                                y: Some(bounds.origin.y),
                                width: Some(bounds.size.width),
                                height: Some(bounds.size.height),
                                color: None,
                                font_size: None,
                                glyph_count: None,
                                z_index: None,
                                clip_depth: Some(clip_depth),
                                scroll_depth: Some(scroll_depth),
                                content_size: None,
                                scroll_id: None,
                                debug_info: None,
                                border_colors: None,
                                border_widths: None,
                            }
                        }
                        azul_layout::solver3::display_list::DisplayListItem::ScrollBar { bounds, color, orientation, .. } => {
                            other_count += 1;
                            let orient_str = match orientation {
                                azul_core::dom::ScrollbarOrientation::Vertical => "vertical",
                                azul_core::dom::ScrollbarOrientation::Horizontal => "horizontal",
                            };
                            DisplayListItemInfo {
                                index: idx,
                                item_type: format!("scrollbar_{}", orient_str),
                                x: Some(bounds.origin.x),
                                y: Some(bounds.origin.y),
                                width: Some(bounds.size.width),
                                height: Some(bounds.size.height),
                                color: Some(format!("#{:02x}{:02x}{:02x}{:02x}", color.r, color.g, color.b, color.a)),
                                font_size: None,
                                glyph_count: None,
                                z_index: None,
                                clip_depth: Some(clip_depth),
                                scroll_depth: Some(scroll_depth),
                                content_size: None,
                                scroll_id: None,
                                debug_info: None,
                                border_colors: None,
                                border_widths: None,
                            }
                        }
                        azul_layout::solver3::display_list::DisplayListItem::ScrollBarStyled { info } => {
                            other_count += 1;
                            let orient_str = match info.orientation {
                                azul_core::dom::ScrollbarOrientation::Vertical => "vertical_styled",
                                azul_core::dom::ScrollbarOrientation::Horizontal => "horizontal_styled",
                            };
                            // Debug: include track and thumb bounds in the output
                            let debug_info_str = format!(
                                "track:({:.1},{:.1},{:.1}x{:.1}) thumb:({:.1},{:.1},{:.1}x{:.1}) track_color:#{:02x}{:02x}{:02x}{:02x} thumb_color:#{:02x}{:02x}{:02x}{:02x}",
                                info.track_bounds.origin.x, info.track_bounds.origin.y,
                                info.track_bounds.size.width, info.track_bounds.size.height,
                                info.thumb_bounds.origin.x, info.thumb_bounds.origin.y,
                                info.thumb_bounds.size.width, info.thumb_bounds.size.height,
                                info.track_color.r, info.track_color.g, info.track_color.b, info.track_color.a,
                                info.thumb_color.r, info.thumb_color.g, info.thumb_color.b, info.thumb_color.a,
                            );
                            DisplayListItemInfo {
                                index: idx,
                                item_type: format!("scrollbar_{}", orient_str),
                                x: Some(info.bounds.origin.x),
                                y: Some(info.bounds.origin.y),
                                width: Some(info.bounds.size.width),
                                height: Some(info.bounds.size.height),
                                color: Some(format!("#{:02x}{:02x}{:02x}{:02x}", 
                                    info.thumb_color.r, info.thumb_color.g, info.thumb_color.b, info.thumb_color.a)),
                                font_size: None,
                                glyph_count: None,
                                z_index: None,
                                clip_depth: Some(clip_depth),
                                scroll_depth: Some(scroll_depth),
                                content_size: None,
                                scroll_id: None,
                                debug_info: Some(debug_info_str),
                                border_colors: None,
                                border_widths: None,
                            }
                        }
                        azul_layout::solver3::display_list::DisplayListItem::PushStackingContext { z_index, bounds } => {
                            other_count += 1;
                            DisplayListItemInfo {
                                index: idx,
                                item_type: "push_stacking_context".to_string(),
                                x: Some(bounds.origin.x),
                                y: Some(bounds.origin.y),
                                width: Some(bounds.size.width),
                                height: Some(bounds.size.height),
                                color: None,
                                font_size: None,
                                glyph_count: None,
                                z_index: Some(*z_index),
                                clip_depth: Some(clip_depth),
                                scroll_depth: Some(scroll_depth),
                                content_size: None,
                                scroll_id: None,
                                debug_info: None,
                                border_colors: None,
                                border_widths: None,
                            }
                        }
                        azul_layout::solver3::display_list::DisplayListItem::PopStackingContext => {
                            other_count += 1;
                            DisplayListItemInfo {
                                index: idx,
                                item_type: "pop_stacking_context".to_string(),
                                x: None,
                                y: None,
                                width: None,
                                height: None,
                                color: None,
                                font_size: None,
                                glyph_count: None,
                                z_index: None,
                                clip_depth: Some(clip_depth),
                                scroll_depth: Some(scroll_depth),
                                content_size: None,
                                scroll_id: None,
                                debug_info: None,
                                border_colors: None,
                                border_widths: None,
                            }
                        }
                        azul_layout::solver3::display_list::DisplayListItem::HitTestArea { bounds, tag } => {
                            other_count += 1;
                            DisplayListItemInfo {
                                index: idx,
                                item_type: "hit_test_area".to_string(),
                                x: Some(bounds.origin.x),
                                y: Some(bounds.origin.y),
                                width: Some(bounds.size.width),
                                height: Some(bounds.size.height),
                                color: None,
                                font_size: None,
                                glyph_count: None,
                                z_index: None,
                                clip_depth: Some(clip_depth),
                                scroll_depth: Some(scroll_depth),
                                content_size: None,
                                scroll_id: None,
                                debug_info: Some(format!("tag:({},0x{:04X})", tag.0, tag.1)),
                                border_colors: None,
                                border_widths: None,
                            }
                        }
                        azul_layout::solver3::display_list::DisplayListItem::CursorRect { bounds, color } => {
                            other_count += 1;
                            DisplayListItemInfo {
                                index: idx,
                                item_type: "cursor".to_string(),
                                x: Some(bounds.origin.x),
                                y: Some(bounds.origin.y),
                                width: Some(bounds.size.width),
                                height: Some(bounds.size.height),
                                color: Some(format!("#{:02x}{:02x}{:02x}{:02x}", color.r, color.g, color.b, color.a)),
                                font_size: None,
                                glyph_count: None,
                                z_index: None,
                                clip_depth: Some(clip_depth),
                                scroll_depth: Some(scroll_depth),
                                content_size: None,
                                scroll_id: None,
                                debug_info: None,
                                border_colors: None,
                                border_widths: None,
                            }
                        }
                        azul_layout::solver3::display_list::DisplayListItem::SelectionRect { bounds, color, .. } => {
                            other_count += 1;
                            DisplayListItemInfo {
                                index: idx,
                                item_type: "selection".to_string(),
                                x: Some(bounds.origin.x),
                                y: Some(bounds.origin.y),
                                width: Some(bounds.size.width),
                                height: Some(bounds.size.height),
                                color: Some(format!("#{:02x}{:02x}{:02x}{:02x}", color.r, color.g, color.b, color.a)),
                                font_size: None,
                                glyph_count: None,
                                z_index: None,
                                clip_depth: Some(clip_depth),
                                scroll_depth: Some(scroll_depth),
                                content_size: None,
                                scroll_id: None,
                                debug_info: None,
                                border_colors: None,
                                border_widths: None,
                            }
                        }
                        _ => {
                            other_count += 1;
                            DisplayListItemInfo {
                                index: idx,
                                item_type: "unknown".to_string(),
                                x: None,
                                y: None,
                                width: None,
                                height: None,
                                color: None,
                                font_size: None,
                                glyph_count: None,
                                z_index: None,
                                clip_depth: Some(clip_depth),
                                scroll_depth: Some(scroll_depth),
                                content_size: None,
                                scroll_id: None,
                                debug_info: None,
                                border_colors: None,
                                border_widths: None,
                            }
                        }
                    };
                    items.push(info);
                }

                // Build clip chain analysis
                let clip_analysis = build_clip_analysis(items_list);

                let response = DisplayListResponse {
                    total_items: items_list.len(),
                    rect_count,
                    text_count,
                    border_count,
                    image_count,
                    other_count,
                    items,
                    clip_analysis: Some(clip_analysis),
                };
                send_ok(request, None, Some(ResponseData::DisplayList(response)));
            } else {
                send_err(request, "No layout result for DOM 0");
            }
        }

        DebugEvent::GetScrollStates => {
            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                "Getting scroll states",
                None,
            );
            use azul_core::dom::DomId;

            let dom_id = DomId { inner: 0 };
            let layout_window = callback_info.get_layout_window();

            // Get scroll states from the scroll manager
            let scroll_states = layout_window
                .scroll_manager
                .get_scroll_states_for_dom(dom_id);
            let mut states = Vec::new();

            for (node_id, scroll_position) in scroll_states.iter() {
                let scroll_x = scroll_position.children_rect.origin.x;
                let scroll_y = scroll_position.children_rect.origin.y;
                let content_width = scroll_position.children_rect.size.width;
                let content_height = scroll_position.children_rect.size.height;
                let container_width = scroll_position.parent_rect.size.width;
                let container_height = scroll_position.parent_rect.size.height;

                states.push(ScrollStateInfo {
                    node_id: node_id.index(),
                    scroll_x,
                    scroll_y,
                    content_width,
                    content_height,
                    container_width,
                    container_height,
                    max_scroll_x: (content_width - container_width).max(0.0),
                    max_scroll_y: (content_height - container_height).max(0.0),
                });
            }

            let response = ScrollStatesResponse {
                scroll_node_count: states.len(),
                scroll_states: states,
            };
            send_ok(request, None, Some(ResponseData::ScrollStates(response)));
        }

        DebugEvent::GetScrollableNodes => {
            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                "Getting scrollable nodes",
                None,
            );
            use azul_core::dom::DomId;

            let dom_id = DomId { inner: 0 };
            let layout_window = callback_info.get_layout_window();

            // Get scrollable nodes from layout tree
            let mut scrollable_nodes = Vec::new();

            if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
                // Check each node in the layout tree to see if it has scrollbar_info
                for (node_idx, node) in layout_result.layout_tree.nodes.iter().enumerate() {
                    if let Some(ref scrollbar_info) = node.scrollbar_info {
                        if scrollbar_info.needs_vertical || scrollbar_info.needs_horizontal {
                            let container = node.used_size.unwrap_or_default();
                            scrollable_nodes.push(ScrollableNodeInfo {
                                node_id: node_idx,
                                dom_node_id: node.dom_node_id.map(|n| n.index()),
                                container_width: container.width,
                                container_height: container.height,
                                can_scroll_x: scrollbar_info.needs_horizontal,
                                can_scroll_y: scrollbar_info.needs_vertical,
                            });
                        }
                    }
                }
            }

            let response = ScrollableNodesResponse {
                scrollable_node_count: scrollable_nodes.len(),
                scrollable_nodes,
            };
            send_ok(request, None, Some(ResponseData::ScrollableNodes(response)));
        }

        DebugEvent::ScrollNodeBy {
            node_id,
            selector,
            text,
            delta_x,
            delta_y,
        } => {
            use azul_core::dom::DomId;
            use azul_core::geom::LogicalPosition;
            use azul_core::id::NodeId;
            use azul_core::styled_dom::NodeHierarchyItemId;

            let resolved_node_id = resolve_node_target(
                callback_info,
                selector.as_deref(),
                *node_id,
                text.as_deref(),
            );

            let nid = match resolved_node_id {
                Some(n) => n.index() as u64,
                None => {
                    send_err(request, "No node found matching the specified target");
                    return needs_update;
                }
            };

            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                format!("Scrolling node {} by ({}, {})", nid, delta_x, delta_y),
                None,
            );

            let dom_id = DomId { inner: 0 };
            let node = NodeId::new(nid as usize);
            let hierarchy_id = NodeHierarchyItemId::from(Some(node));

            // Get current scroll position and add delta
            let current = callback_info
                .get_scroll_offset_for_node(dom_id, node)
                .unwrap_or(LogicalPosition { x: 0.0, y: 0.0 });
            let new_pos = LogicalPosition {
                x: current.x + *delta_x,
                y: current.y + *delta_y,
            };
            callback_info.scroll_to(dom_id, hierarchy_id, new_pos);
            needs_update = true;

            let response = ScrollNodeByResponse {
                scrolled: true,
                node_id: nid,
                delta_x: *delta_x,
                delta_y: *delta_y,
            };
            send_ok(request, None, Some(ResponseData::ScrollNodeBy(response)));
        }

        DebugEvent::ScrollNodeTo {
            node_id,
            selector,
            text,
            x,
            y,
        } => {
            use azul_core::dom::DomId;
            use azul_core::geom::LogicalPosition;
            use azul_core::id::NodeId;
            use azul_core::styled_dom::NodeHierarchyItemId;

            let resolved_node_id = resolve_node_target(
                callback_info,
                selector.as_deref(),
                *node_id,
                text.as_deref(),
            );

            let nid = match resolved_node_id {
                Some(n) => n.index() as u64,
                None => {
                    send_err(request, "No node found matching the specified target");
                    return needs_update;
                }
            };

            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                format!("Scrolling node {} to position ({}, {})", nid, x, y),
                None,
            );

            let dom_id = DomId { inner: 0 };
            let node = NodeId::new(nid as usize);
            let hierarchy_id = NodeHierarchyItemId::from(Some(node));

            callback_info.scroll_to(dom_id, hierarchy_id, LogicalPosition { x: *x, y: *y });
            needs_update = true;

            let response = ScrollNodeToResponse {
                scrolled: true,
                node_id: nid,
                x: *x,
                y: *y,
            };
            send_ok(request, None, Some(ResponseData::ScrollNodeTo(response)));
        }

        DebugEvent::ScrollIntoView {
            node_id,
            selector,
            text,
            block,
            inline,
            behavior,
        } => {
            use azul_core::dom::{DomId, DomNodeId};
            use azul_core::events::{ScrollIntoViewBehavior, ScrollIntoViewOptions, ScrollLogicalPosition};
            use azul_core::id::NodeId;
            use azul_core::styled_dom::NodeHierarchyItemId;
            use azul_core::task::Instant;

            let resolved_node_id = resolve_node_target(
                callback_info,
                selector.as_deref(),
                *node_id,
                text.as_deref(),
            );

            let nid = match resolved_node_id {
                Some(n) => n.index() as u64,
                None => {
                    send_err(request, "No node found matching the specified target");
                    return needs_update;
                }
            };

            // Parse alignment options
            let block_align = match block.as_deref() {
                Some("start") => ScrollLogicalPosition::Start,
                Some("center") => ScrollLogicalPosition::Center,
                Some("end") => ScrollLogicalPosition::End,
                _ => ScrollLogicalPosition::Nearest,
            };

            let inline_align = match inline.as_deref() {
                Some("start") => ScrollLogicalPosition::Start,
                Some("center") => ScrollLogicalPosition::Center,
                Some("end") => ScrollLogicalPosition::End,
                _ => ScrollLogicalPosition::Nearest,
            };

            let scroll_behavior = match behavior.as_deref() {
                Some("instant") => ScrollIntoViewBehavior::Instant,
                Some("smooth") => ScrollIntoViewBehavior::Smooth,
                _ => ScrollIntoViewBehavior::Auto,
            };

            let options = ScrollIntoViewOptions {
                block: block_align,
                inline_axis: inline_align,
                behavior: scroll_behavior,
            };

            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                format!(
                    "Scrolling node {} into view (block: {:?}, inline: {:?}, behavior: {:?})",
                    nid, block_align, inline_align, scroll_behavior
                ),
                None,
            );

            let dom_id = DomId { inner: 0 };
            let node = NodeId::new(nid as usize);
            let dom_node_id = DomNodeId {
                dom: dom_id,
                node: NodeHierarchyItemId::from_crate_internal(Some(node)),
            };

            // Call scroll_node_into_view on CallbackInfo (queues the scroll change)
            callback_info.scroll_node_into_view(dom_node_id, options);

            // The scroll will be processed after the callback returns
            needs_update = true;

            let response = ScrollIntoViewResponse {
                scrolled: true,
                node_id: nid,
                adjustments_count: 0, // Count is not known until change is processed
            };
            
            send_ok(request, None, Some(ResponseData::ScrollIntoView(response)));
        }

        DebugEvent::FindNodeByText { text } => {
            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                format!("Finding node by text: {}", text),
                None,
            );
            use azul_core::dom::{DomId, DomNodeId};
            use azul_core::id::NodeId;

            let dom_id = DomId { inner: 0 };
            let layout_window = callback_info.get_layout_window();

            if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
                let styled_dom = &layout_result.styled_dom;
                let node_data = styled_dom.node_data.as_container();
                let node_count = node_data.len();

                let mut found_node = None;
                for i in 0..node_count {
                    let data = &node_data[NodeId::new(i)];
                    if let azul_core::dom::NodeType::Text(t) = data.get_node_type() {
                        if t.as_str().contains(text.as_str()) {
                            let dom_node_id = DomNodeId {
                                dom: dom_id.clone(),
                                node: Some(NodeId::new(i)).into(),
                            };
                            found_node = Some((i, dom_node_id));
                            break;
                        }
                    }
                }

                if let Some((node_idx, dom_node_id)) = found_node {
                    let rect = callback_info.get_node_rect(dom_node_id);
                    let tag = callback_info.get_node_tag_name(dom_node_id);
                    let classes = callback_info.get_node_classes(dom_node_id);

                    let response = FindNodeResponse {
                        found: true,
                        node_id: Some(node_idx as u64),
                        x: rect.as_ref().map(|r| r.origin.x),
                        y: rect.as_ref().map(|r| r.origin.y),
                        width: rect.as_ref().map(|r| r.size.width),
                        height: rect.as_ref().map(|r| r.size.height),
                        tag: tag.map(|s| s.as_str().to_string()),
                        classes: Some(
                            classes
                                .as_ref()
                                .iter()
                                .map(|s| s.as_str().to_string())
                                .collect(),
                        ),
                    };
                    send_ok(request, None, Some(ResponseData::FindNode(response)));
                } else {
                    let response = FindNodeResponse {
                        found: false,
                        node_id: None,
                        x: None,
                        y: None,
                        width: None,
                        height: None,
                        tag: None,
                        classes: None,
                    };
                    send_ok(request, None, Some(ResponseData::FindNode(response)));
                }
            } else {
                send_err(request, "No layout result for DOM 0");
            }
        }

        DebugEvent::ClickNode { node_id, button } => {
            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                format!("Clicking node {} with button {:?}", node_id, button),
                None,
            );
            use azul_core::dom::{DomId, DomNodeId};
            use azul_core::id::NodeId;

            let dom_id = DomId { inner: 0 };
            let dom_node_id = DomNodeId {
                dom: dom_id.clone(),
                node: Some(NodeId::new(*node_id as usize)).into(),
            };

            // Get the node's rect to find the center position
            if let Some(rect) = callback_info.get_node_rect(dom_node_id) {
                let center_x = rect.origin.x + rect.size.width / 2.0;
                let center_y = rect.origin.y + rect.size.height / 2.0;

                // Simulate click at the center of the node
                let mut new_state = callback_info.get_current_window_state().clone();
                new_state.mouse_state.cursor_position =
                    azul_core::window::CursorPosition::InWindow(LogicalPosition {
                        x: center_x,
                        y: center_y,
                    });

                // Mouse down
                match button {
                    MouseButton::Left => new_state.mouse_state.left_down = true,
                    MouseButton::Right => new_state.mouse_state.right_down = true,
                    MouseButton::Middle => new_state.mouse_state.middle_down = true,
                }
                callback_info.modify_window_state(new_state.clone());

                // Mouse up
                match button {
                    MouseButton::Left => new_state.mouse_state.left_down = false,
                    MouseButton::Right => new_state.mouse_state.right_down = false,
                    MouseButton::Middle => new_state.mouse_state.middle_down = false,
                }
                callback_info.modify_window_state(new_state);
                needs_update = true;

                let response = ClickNodeResponse {
                    success: true,
                    message: format!("Clicked node {} at ({}, {})", node_id, center_x, center_y),
                };
                send_ok(request, None, Some(ResponseData::ClickNode(response)));
            } else {
                let response = ClickNodeResponse {
                    success: false,
                    message: format!("Node {} not found or has no rect", node_id),
                };
                send_ok(request, None, Some(ResponseData::ClickNode(response)));
            }
        }

        DebugEvent::GetScrollbarInfo {
            node_id,
            selector,
            text,
            orientation,
        } => {
            use azul_core::dom::{DomId, ScrollbarOrientation};
            use azul_core::geom::LogicalPosition;
            use azul_core::id::NodeId;

            log(LogLevel::Debug, LogCategory::DebugServer,
                format!("Getting scrollbar info for node_id={:?}, selector={:?}, text={:?}, orientation={:?}", 
                    node_id, selector, text, orientation), None);

            let resolved_node_id = resolve_node_target(
                callback_info,
                selector.as_deref(),
                *node_id,
                text.as_deref(),
            );

            let nid = match resolved_node_id {
                Some(n) => n.index() as u64,
                None => {
                    send_err(request, "No node found matching the specified target");
                    return needs_update;
                }
            };

            let dom_id = DomId { inner: 0 };
            let node = NodeId::new(nid as usize);
            let layout_window = callback_info.get_layout_window();

            // Get current scroll state
            let scroll_offset = callback_info
                .get_scroll_offset_for_node(dom_id, node)
                .unwrap_or(LogicalPosition { x: 0.0, y: 0.0 });

            // Get container and content rects from scroll manager
            let scroll_states = layout_window
                .scroll_manager
                .get_scroll_states_for_dom(dom_id);
            let scroll_info = scroll_states
                .iter()
                .find(|(n, _)| **n == node)
                .map(|(_, s)| s);

            // Default container/content rects if not in scroll manager
            let (container_rect, content_rect, max_scroll_x, max_scroll_y) = match scroll_info {
                Some(state) => {
                    let max_x =
                        (state.children_rect.size.width - state.parent_rect.size.width).max(0.0);
                    let max_y =
                        (state.children_rect.size.height - state.parent_rect.size.height).max(0.0);
                    (state.parent_rect, state.children_rect, max_x, max_y)
                }
                None => {
                    // Fallback: try to get from layout
                    let zero_rect = azul_core::geom::LogicalRect {
                        origin: LogicalPosition { x: 0.0, y: 0.0 },
                        size: azul_core::geom::LogicalSize {
                            width: 0.0,
                            height: 0.0,
                        },
                    };
                    (zero_rect, zero_rect, 0.0, 0.0)
                }
            };

            // Helper function to build ScrollbarGeometry from ScrollbarState
            fn build_scrollbar_geometry(
                state: &azul_layout::managers::scroll_state::ScrollbarState,
            ) -> ScrollbarGeometry {
                let track = state.track_rect;
                let button_size = state.base_size;

                // Calculate thumb rect based on orientation
                let (thumb_rect, top_button_rect, bottom_button_rect) = match state.orientation {
                    ScrollbarOrientation::Vertical => {
                        let track_height_usable = track.size.height - 2.0 * button_size;
                        let thumb_height = track_height_usable * state.thumb_size_ratio;
                        let thumb_y_start = button_size
                            + (track_height_usable - thumb_height) * state.thumb_position_ratio;

                        let top_btn = azul_core::geom::LogicalRect {
                            origin: track.origin,
                            size: azul_core::geom::LogicalSize {
                                width: track.size.width,
                                height: button_size,
                            },
                        };
                        let bottom_btn = azul_core::geom::LogicalRect {
                            origin: LogicalPosition {
                                x: track.origin.x,
                                y: track.origin.y + track.size.height - button_size,
                            },
                            size: azul_core::geom::LogicalSize {
                                width: track.size.width,
                                height: button_size,
                            },
                        };
                        let thumb = azul_core::geom::LogicalRect {
                            origin: LogicalPosition {
                                x: track.origin.x,
                                y: track.origin.y + thumb_y_start,
                            },
                            size: azul_core::geom::LogicalSize {
                                width: track.size.width,
                                height: thumb_height,
                            },
                        };
                        (thumb, top_btn, bottom_btn)
                    }
                    ScrollbarOrientation::Horizontal => {
                        let track_width_usable = track.size.width - 2.0 * button_size;
                        let thumb_width = track_width_usable * state.thumb_size_ratio;
                        let thumb_x_start = button_size
                            + (track_width_usable - thumb_width) * state.thumb_position_ratio;

                        let left_btn = azul_core::geom::LogicalRect {
                            origin: track.origin,
                            size: azul_core::geom::LogicalSize {
                                width: button_size,
                                height: track.size.height,
                            },
                        };
                        let right_btn = azul_core::geom::LogicalRect {
                            origin: LogicalPosition {
                                x: track.origin.x + track.size.width - button_size,
                                y: track.origin.y,
                            },
                            size: azul_core::geom::LogicalSize {
                                width: button_size,
                                height: track.size.height,
                            },
                        };
                        let thumb = azul_core::geom::LogicalRect {
                            origin: LogicalPosition {
                                x: track.origin.x + thumb_x_start,
                                y: track.origin.y,
                            },
                            size: azul_core::geom::LogicalSize {
                                width: thumb_width,
                                height: track.size.height,
                            },
                        };
                        (thumb, left_btn, right_btn)
                    }
                };

                ScrollbarGeometry {
                    visible: state.visible,
                    track_rect: LogicalRectJson {
                        x: track.origin.x,
                        y: track.origin.y,
                        width: track.size.width,
                        height: track.size.height,
                    },
                    track_center: LogicalPositionJson {
                        x: track.origin.x + track.size.width / 2.0,
                        y: track.origin.y + track.size.height / 2.0,
                    },
                    button_size,
                    top_button_rect: LogicalRectJson {
                        x: top_button_rect.origin.x,
                        y: top_button_rect.origin.y,
                        width: top_button_rect.size.width,
                        height: top_button_rect.size.height,
                    },
                    bottom_button_rect: LogicalRectJson {
                        x: bottom_button_rect.origin.x,
                        y: bottom_button_rect.origin.y,
                        width: bottom_button_rect.size.width,
                        height: bottom_button_rect.size.height,
                    },
                    thumb_rect: LogicalRectJson {
                        x: thumb_rect.origin.x,
                        y: thumb_rect.origin.y,
                        width: thumb_rect.size.width,
                        height: thumb_rect.size.height,
                    },
                    thumb_center: LogicalPositionJson {
                        x: thumb_rect.origin.x + thumb_rect.size.width / 2.0,
                        y: thumb_rect.origin.y + thumb_rect.size.height / 2.0,
                    },
                    thumb_position_ratio: state.thumb_position_ratio,
                    thumb_size_ratio: state.thumb_size_ratio,
                }
            }

            // Get scrollbar states
            let v_state = layout_window.scroll_manager.get_scrollbar_state(
                dom_id,
                node,
                ScrollbarOrientation::Vertical,
            );
            let h_state = layout_window.scroll_manager.get_scrollbar_state(
                dom_id,
                node,
                ScrollbarOrientation::Horizontal,
            );

            let vertical = v_state.map(build_scrollbar_geometry);
            let horizontal = h_state.map(build_scrollbar_geometry);

            let has_any = vertical.is_some() || horizontal.is_some();

            let response = ScrollbarInfoResponse {
                found: has_any,
                node_id: nid,
                dom_node_id: Some(nid),
                orientation: orientation.clone().unwrap_or_else(|| "both".to_string()),
                horizontal,
                vertical,
                scroll_x: scroll_offset.x,
                scroll_y: scroll_offset.y,
                max_scroll_x,
                max_scroll_y,
                container_rect: LogicalRectJson {
                    x: container_rect.origin.x,
                    y: container_rect.origin.y,
                    width: container_rect.size.width,
                    height: container_rect.size.height,
                },
                content_rect: LogicalRectJson {
                    x: content_rect.origin.x,
                    y: content_rect.origin.y,
                    width: content_rect.size.width,
                    height: content_rect.size.height,
                },
            };
            send_ok(request, None, Some(ResponseData::ScrollbarInfo(response)));
        }

        DebugEvent::GetSelectionState => {
            // Get the selection manager from layout window
            let layout_window = callback_info.get_layout_window();
            let selection_manager = &layout_window.selection_manager;
            let all_selections = selection_manager.get_all_selections();
            
            for (dom_id, sel_state) in all_selections.iter() {
            }

            let mut selections = Vec::new();

            for (dom_id, selection_state) in all_selections.iter() {
                // Get the node ID from selection state
                let internal_node_id = selection_state.node_id.node.into_crate_internal();
                let node_id = internal_node_id.map(|n| n.index() as u64);
                
                // Build CSS selector for this node
                let selector = internal_node_id
                    .and_then(|nid| build_selector_for_node(&callback_info, *dom_id, nid));

                // Convert selections to range info
                let mut ranges = Vec::new();
                for selection in selection_state.selections.as_slice() {
                    use azul_core::selection::Selection;
                    let range_info = match selection {
                        Selection::Cursor(cursor) => SelectionRangeInfo {
                            selection_type: "cursor".to_string(),
                            cursor_position: Some(cursor.cluster_id.start_byte_in_run as usize),
                            start: None,
                            end: None,
                            direction: None,
                        },
                        Selection::Range(range) => {
                            let start_pos = range.start.cluster_id.start_byte_in_run as usize;
                            let end_pos = range.end.cluster_id.start_byte_in_run as usize;
                            SelectionRangeInfo {
                                selection_type: "range".to_string(),
                                cursor_position: None,
                                start: Some(start_pos),
                                end: Some(end_pos),
                                direction: Some(if start_pos <= end_pos { "forward" } else { "backward" }.to_string()),
                            }
                        },
                    };
                    ranges.push(range_info);
                }

                // Note: Selection rectangles would require accessing private methods
                // For now, we just return the selection ranges without visual rectangles
                let rectangles = Vec::new();

                selections.push(DomSelectionInfo {
                    dom_id: dom_id.inner as u32,
                    node_id,
                    selector,
                    ranges,
                    rectangles,
                });
            }

            let response = SelectionStateResponse {
                has_selection: !selections.is_empty(),
                selection_count: selections.len(),
                selections,
            };
            send_ok(request, None, Some(ResponseData::SelectionState(response)));
        }

        DebugEvent::DumpSelectionManager => {
            // Dump entire selection manager state for debugging
            let layout_window = callback_info.get_layout_window();
            let selection_manager = &layout_window.selection_manager;
            let all_selections = selection_manager.get_all_selections();
            
            let mut selections = Vec::new();
            for (dom_id, selection_state) in all_selections.iter() {
                let internal_node_id = selection_state.node_id.node.into_crate_internal();
                let node_id = internal_node_id.map(|n| n.index() as u64);
                let selector = internal_node_id
                    .and_then(|nid| build_selector_for_node(&callback_info, *dom_id, nid));
                
                let mut sel_dumps = Vec::new();
                for sel in selection_state.selections.as_slice() {
                    use azul_core::selection::Selection;
                    sel_dumps.push(SelectionDump {
                        selection_type: match sel {
                            Selection::Cursor(_) => "cursor".to_string(),
                            Selection::Range(_) => "range".to_string(),
                        },
                        debug: alloc::format!("{:?}", sel),
                    });
                }
                
                selections.push(SelectionDumpEntry {
                    dom_id: dom_id.inner as u32,
                    node_id,
                    selector,
                    selections: sel_dumps,
                });
            }
            
            let click_state = &selection_manager.click_state;
            let response = SelectionManagerDump {
                selections,
                click_state: ClickStateDump {
                    last_node: click_state.last_node.map(|n| alloc::format!("{:?}", n)),
                    last_position: LogicalPositionJson {
                        x: click_state.last_position.x,
                        y: click_state.last_position.y,
                    },
                    last_time_ms: click_state.last_time_ms,
                    click_count: click_state.click_count,
                },
            };
            send_ok(request, None, Some(ResponseData::SelectionManagerDump(response)));
        }

        DebugEvent::GetDragState => {
            // Get current drag state from unified drag system
            let layout_window = callback_info.get_layout_window();
            let gesture_manager = &layout_window.gesture_drag_manager;
            
            let (is_dragging, drag_type, description) = if let Some(drag_ctx) = gesture_manager.get_drag_context() {
                use azul_core::drag::ActiveDragType;
                let type_str = match &drag_ctx.drag_type {
                    ActiveDragType::TextSelection(_) => "text_selection",
                    ActiveDragType::ScrollbarThumb(_) => "scrollbar_thumb", 
                    ActiveDragType::Node(_) => "node",
                    ActiveDragType::WindowMove(_) => "window_move",
                    ActiveDragType::WindowResize(_) => "window_resize",
                    ActiveDragType::FileDrop(_) => "file_drop",
                };
                let desc = alloc::format!("{} drag from {:?}", type_str, drag_ctx.start_position());
                (true, Some(type_str.to_string()), desc)
            } else {
                (false, None, "No active drag".to_string())
            };
            
            let response = DragStateResponse {
                is_dragging,
                drag_type,
                description,
            };
            send_ok(request, None, Some(ResponseData::DragState(response)));
        }

        DebugEvent::GetDragContext => {
            // Get detailed drag context from unified drag system
            let layout_window = callback_info.get_layout_window();
            let gesture_manager = &layout_window.gesture_drag_manager;
            
            let response = if let Some(drag_ctx) = gesture_manager.get_drag_context() {
                use azul_core::drag::ActiveDragType;
                let (type_str, scrollbar_axis, resize_edge, files, target_node_id, target_dom_id) = 
                    match &drag_ctx.drag_type {
                        ActiveDragType::TextSelection(sel) => {
                            ("text_selection", None, None, None, 
                             Some(sel.anchor_ifc_node.index() as u64),
                             Some(sel.dom_id.inner as u32))
                        }
                        ActiveDragType::ScrollbarThumb(sb) => {
                            let axis = match sb.axis {
                                azul_core::drag::ScrollbarAxis::Horizontal => "horizontal",
                                azul_core::drag::ScrollbarAxis::Vertical => "vertical",
                            };
                            ("scrollbar_thumb", Some(axis.to_string()), None, None,
                             Some(sb.scroll_container_node.index() as u64),
                             None) // ScrollbarThumbDrag doesn't have dom_id
                        }
                        ActiveDragType::Node(nd) => {
                            ("node", None, None, None,
                             Some(nd.node_id.index() as u64),
                             Some(nd.dom_id.inner as u32))
                        }
                        ActiveDragType::WindowMove(_) => {
                            ("window_move", None, None, None, None, None)
                        }
                        ActiveDragType::WindowResize(wr) => {
                            let edge = alloc::format!("{:?}", wr.edge);
                            ("window_resize", None, Some(edge), None, None, None)
                        }
                        ActiveDragType::FileDrop(fd) => {
                            let file_list: Vec<String> = fd.files
                                .as_slice()
                                .iter()
                                .map(|f| f.as_str().to_string())
                                .collect();
                            ("file_drop", None, None, Some(file_list), None, None)
                        }
                    };
                
                DragContextResponse {
                    is_dragging: true,
                    drag_type: Some(type_str.to_string()),
                    start_position: Some(LogicalPositionJson {
                        x: drag_ctx.start_position().x,
                        y: drag_ctx.start_position().y,
                    }),
                    current_position: Some(LogicalPositionJson {
                        x: drag_ctx.current_position().x,
                        y: drag_ctx.current_position().y,
                    }),
                    target_node_id,
                    target_dom_id,
                    scrollbar_axis,
                    resize_edge,
                    files,
                    drag_data: None, // TODO: convert DragData to BTreeMap
                    drag_effect: None, // TODO: convert DragEffect
                    debug: alloc::format!("{:?}", drag_ctx),
                }
            } else {
                DragContextResponse {
                    is_dragging: false,
                    drag_type: None,
                    start_position: None,
                    current_position: None,
                    target_node_id: None,
                    target_dom_id: None,
                    scrollbar_axis: None,
                    resize_edge: None,
                    files: None,
                    drag_data: None,
                    drag_effect: None,
                    debug: "No active drag".to_string(),
                }
            };
            send_ok(request, None, Some(ResponseData::DragContext(response)));
        }

        // Note: GetAppState and SetAppState require access to the app's RefAny,
        // which is now passed in via the timer_data parameter.
        DebugEvent::GetAppState => {
            use azul_layout::json::serialize_refany_to_json;

            // Build metadata
            let metadata = RefAnyMetadata {
                type_id: app_data.get_type_id(),
                type_name: app_data.get_type_name().as_str().to_string(),
                can_serialize: app_data.can_serialize(),
                can_deserialize: app_data.can_deserialize(),
                ref_count: app_data.get_ref_count(),
            };

            if !app_data.can_serialize() {
                let response = AppStateResponse {
                    metadata,
                    state: serde_json::Value::Null,
                    error: Some(RefAnyError::NotSerializable),
                };
                send_ok(request, None, Some(ResponseData::AppState(response)));
            } else {
                match serialize_refany_to_json(app_data) {
                    Some(json) => {
                        // Convert our Json type to serde_json::Value for the response
                        let json_string = json.to_string();
                        match serde_json::from_str(&json_string.as_str()) {
                            Ok(value) => {
                                let response = AppStateResponse {
                                    metadata,
                                    state: value,
                                    error: None,
                                };
                                send_ok(request, None, Some(ResponseData::AppState(response)));
                            }
                            Err(e) => {
                                let response = AppStateResponse {
                                    metadata,
                                    state: serde_json::Value::Null,
                                    error: Some(RefAnyError::SerdeError(e.to_string())),
                                };
                                send_ok(request, None, Some(ResponseData::AppState(response)));
                            }
                        }
                    }
                    None => {
                        let response = AppStateResponse {
                            metadata,
                            state: serde_json::Value::Null,
                            error: Some(RefAnyError::SerdeError("Serialization returned null".to_string())),
                        };
                        send_ok(request, None, Some(ResponseData::AppState(response)));
                    }
                }
            }
        }

        DebugEvent::SetAppState { state } => {
            use azul_layout::json::{deserialize_refany_from_json, Json};

            // Get deserialize_fn from RefAny
            let deserialize_fn = app_data.get_deserialize_fn();
            
            if deserialize_fn == 0 {
                let response = AppStateSetResponse {
                    success: false,
                    error: Some(RefAnyError::NotDeserializable),
                };
                send_ok(request, None, Some(ResponseData::AppStateSet(response)));
            } else {
                // Convert serde_json::Value to our Json type
                let json_string = state.to_string();
                match Json::parse(&json_string) {
                    Ok(json) => {
                        match deserialize_refany_from_json(json, deserialize_fn) {
                            Ok(new_app_data) => {
                                // Replace the app data contents - this is visible to all clones
                                let success = app_data.replace_contents(new_app_data);
                                needs_update = success;
                                
                                let response = AppStateSetResponse {
                                    success,
                                    error: if success { None } else { Some(RefAnyError::TypeConstructionError("Failed to replace contents - active borrows exist".to_string())) },
                                };
                                send_ok(request, None, Some(ResponseData::AppStateSet(response)));
                            }
                            Err(e) => {
                                let response = AppStateSetResponse {
                                    success: false,
                                    error: Some(RefAnyError::TypeConstructionError(e)),
                                };
                                send_ok(request, None, Some(ResponseData::AppStateSet(response)));
                            }
                        }
                    }
                    Err(e) => {
                        let response = AppStateSetResponse {
                            success: false,
                            error: Some(RefAnyError::SerdeError(alloc::format!("{:?}", e))),
                        };
                        send_ok(request, None, Some(ResponseData::AppStateSet(response)));
                    }
                }
            }
        }

        DebugEvent::KeyDown { key, modifiers } => {
            use azul_core::window::{VirtualKeyCode, VirtualKeyCodeVec};
            
            log(
                LogLevel::Debug,
                LogCategory::EventLoop,
                format!("Debug key down: {} (shift={}, ctrl={}, alt={})", key, modifiers.shift, modifiers.ctrl, modifiers.alt),
                None,
            );

            let mut new_state = callback_info.get_current_window_state().clone();
            
            // Collect current keys into a Vec
            let mut pressed_keys: alloc::vec::Vec<VirtualKeyCode> = new_state.keyboard_state.pressed_virtual_keycodes.iter().copied().collect();
            
            // Parse the key string to VirtualKeyCode
            if let Some(keycode) = parse_virtual_keycode(key) {
                // Add the key to pressed keys if not already present
                if !pressed_keys.iter().any(|k| *k == keycode) {
                    pressed_keys.push(keycode);
                }
                new_state.keyboard_state.current_virtual_keycode = Some(keycode).into();
            }
            
            // Set modifier keys based on modifiers struct
            if modifiers.shift && !pressed_keys.iter().any(|k| *k == VirtualKeyCode::LShift) {
                pressed_keys.push(VirtualKeyCode::LShift);
            }
            if modifiers.ctrl && !pressed_keys.iter().any(|k| *k == VirtualKeyCode::LControl) {
                pressed_keys.push(VirtualKeyCode::LControl);
            }
            if modifiers.alt && !pressed_keys.iter().any(|k| *k == VirtualKeyCode::LAlt) {
                pressed_keys.push(VirtualKeyCode::LAlt);
            }
            if modifiers.meta && !pressed_keys.iter().any(|k| *k == VirtualKeyCode::LWin) {
                pressed_keys.push(VirtualKeyCode::LWin);
            }
            
            new_state.keyboard_state.pressed_virtual_keycodes = VirtualKeyCodeVec::from_vec(pressed_keys);
            callback_info.modify_window_state(new_state);
            needs_update = true;

            send_ok(request, None, None);
        }

        DebugEvent::KeyUp { key, modifiers } => {
            use azul_core::window::{VirtualKeyCode, VirtualKeyCodeVec};
            
            log(
                LogLevel::Debug,
                LogCategory::EventLoop,
                format!("Debug key up: {} (shift={}, ctrl={}, alt={})", key, modifiers.shift, modifiers.ctrl, modifiers.alt),
                None,
            );

            let mut new_state = callback_info.get_current_window_state().clone();
            
            // Collect current keys into a Vec
            let mut pressed_keys: alloc::vec::Vec<VirtualKeyCode> = new_state.keyboard_state.pressed_virtual_keycodes.iter().copied().collect();
            
            // Parse the key string to VirtualKeyCode and remove it
            if let Some(keycode) = parse_virtual_keycode(key) {
                pressed_keys.retain(|k| *k != keycode);
                new_state.keyboard_state.current_virtual_keycode = None.into();
            }
            
            // Remove modifier keys if modifiers struct says they should be released
            if !modifiers.shift {
                pressed_keys.retain(|k| *k != VirtualKeyCode::LShift && *k != VirtualKeyCode::RShift);
            }
            if !modifiers.ctrl {
                pressed_keys.retain(|k| *k != VirtualKeyCode::LControl && *k != VirtualKeyCode::RControl);
            }
            if !modifiers.alt {
                pressed_keys.retain(|k| *k != VirtualKeyCode::LAlt && *k != VirtualKeyCode::RAlt);
            }
            if !modifiers.meta {
                pressed_keys.retain(|k| *k != VirtualKeyCode::LWin && *k != VirtualKeyCode::RWin);
            }
            
            new_state.keyboard_state.pressed_virtual_keycodes = VirtualKeyCodeVec::from_vec(pressed_keys);
            callback_info.modify_window_state(new_state);
            needs_update = true;

            send_ok(request, None, None);
        }

        DebugEvent::TextInput { text } => {
            log(
                LogLevel::Debug,
                LogCategory::EventLoop,
                format!("[DEBUG TextInput] Received text input via debug server: '{}'", text),
                None,
            );
            println!("[DEBUG TextInput] ============================================");
            println!("[DEBUG TextInput] Step 1: Debug server received text: '{}'", text);
            
            // Get the focused node - text input only works on focused contenteditable
            let layout_window = callback_info.get_layout_window();
            let focused_node = layout_window.focus_manager.get_focused_node();
            println!("[DEBUG TextInput] Step 2: Focused node: {:?}", focused_node);
            
            if focused_node.is_some() {
                // Use the new create_text_input API which:
                // 1. Records the changeset in TextInputManager
                // 2. Triggers text input callbacks via recursive event processing
                // 3. Applies the changeset if not rejected via preventDefault
                // 4. Marks dirty nodes for re-render
                callback_info.create_text_input(text.clone().into());
                println!("[DEBUG TextInput] Step 3: Called callback_info.create_text_input()");
                println!("[DEBUG TextInput] NOTE: Text input will be processed recursively");
                println!("[DEBUG TextInput] NOTE: User callbacks can intercept via OnTextInput");
                
                needs_update = true;
                send_ok(request, None, None);
            } else {
                println!("[DEBUG TextInput] ERROR: No focused node - text input requires focus on contenteditable");
                send_err(request, "No focused node - text input requires focus on contenteditable");
            }
            println!("[DEBUG TextInput] ============================================");
        }
        DebugEvent::GetFocusState => {
            let layout_window = callback_info.get_layout_window();
            let focus_manager = &layout_window.focus_manager;
            
            let response = if let Some(focused_node) = focus_manager.get_focused_node() {
                let dom_id = focused_node.dom;
                let internal_node_id = focused_node.node.into_crate_internal();
                
                let focused_info = internal_node_id.map(|node_id| {
                    // Get node info
                    let selector = build_selector_for_node(&callback_info, dom_id, node_id);
                    
                    // Check if contenteditable
                    let is_contenteditable = callback_info
                        .get_layout_window()
                        .layout_results
                        .get(&dom_id)
                        .and_then(|lr| lr.styled_dom.node_data.get(node_id.index()))
                        .map(|nd| nd.is_contenteditable())
                        .unwrap_or(false);
                    
                    // Get text content - extract from NodeType::Text if available
                    let text_content = callback_info
                        .get_layout_window()
                        .layout_results
                        .get(&dom_id)
                        .and_then(|lr| lr.styled_dom.node_data.get(node_id.index()))
                        .and_then(|nd| {
                            match nd.get_node_type() {
                                azul_core::dom::NodeType::Text(s) => Some(s.as_str().to_string()),
                                _ => None,
                            }
                        });
                    
                    FocusedNodeInfo {
                        dom_id: dom_id.inner as u32,
                        node_id: node_id.index() as u64,
                        selector,
                        is_contenteditable,
                        text_content,
                    }
                });
                
                FocusStateResponse {
                    has_focus: focused_info.is_some(),
                    focused_node: focused_info,
                }
            } else {
                FocusStateResponse {
                    has_focus: false,
                    focused_node: None,
                }
            };
            
            send_ok(request, None, Some(ResponseData::FocusState(response)));
        }

        DebugEvent::GetCursorState => {
            let layout_window = callback_info.get_layout_window();
            let cursor_manager = &layout_window.cursor_manager;
            
            let response = if let (Some(cursor), Some(location)) = (&cursor_manager.cursor, &cursor_manager.cursor_location) {
                let position = cursor.cluster_id.start_byte_in_run as usize;
                let affinity = match cursor.affinity {
                    azul_core::selection::CursorAffinity::Leading => "leading".to_string(),
                    azul_core::selection::CursorAffinity::Trailing => "trailing".to_string(),
                };
                
                CursorStateResponse {
                    has_cursor: true,
                    cursor: Some(CursorInfo {
                        dom_id: location.dom_id.inner as u32,
                        node_id: location.node_id.index() as u64,
                        position,
                        affinity,
                        is_visible: cursor_manager.is_visible,
                        blink_timer_active: cursor_manager.blink_timer_active,
                    }),
                }
            } else {
                CursorStateResponse {
                    has_cursor: false,
                    cursor: None,
                }
            };
            
            send_ok(request, None, Some(ResponseData::CursorState(response)));
        }

        DebugEvent::RunE2eTests { ref tests, ref snapshots } => {
            let mut all_results = Vec::new();
            let empty_snapshots = std::collections::HashMap::new();
            let snap_map = snapshots.as_ref().unwrap_or(&empty_snapshots);

            log(
                LogLevel::Info,
                LogCategory::DebugServer,
                format!("[E2E] RunE2eTests: executing {} test(s)", tests.len()),
                None,
            );

            for test in tests {
                let test_start = std::time::Instant::now();
                let mut step_results = Vec::new();
                let mut test_failed = false;
                let continue_on_failure = test.config.continue_on_failure;
                let delay_ms = test.config.delay_between_steps_ms;

                log(
                    LogLevel::Info,
                    LogCategory::DebugServer,
                    format!("[E2E] === Test '{}' ({} steps, continue_on_failure={}) ===", test.name, test.steps.len(), continue_on_failure),
                    None,
                );

                // Apply E2eSetup.app_state before any steps (if provided)
                if let Some(ref setup) = test.setup {
                    if let Some(ref initial_state) = setup.app_state {
                        log(
                            LogLevel::Debug,
                            LogCategory::DebugServer,
                            format!("[E2E] Applying setup.app_state for test '{}'", test.name),
                            None,
                        );
                        let set_state_event = DebugEvent::SetAppState {
                            state: initial_state.clone(),
                        };
                        let (setup_tx, _setup_rx) = mpsc::channel();
                        let setup_request = DebugRequest {
                            request_id: NEXT_REQUEST_ID.fetch_add(1, Ordering::SeqCst),
                            event: set_state_event,
                            window_id: request.window_id.clone(),
                            wait_for_render: false,
                            response_tx: setup_tx,
                        };
                        let setup_update = process_debug_event(
                            &setup_request,
                            callback_info,
                            app_data,
                            component_map,
                        );
                        if setup_update { needs_update = true; }
                    }
                }

                for (step_index, step) in test.steps.iter().enumerate() {

                    // Delay between steps (for visual inspection)
                    if step_index > 0 && delay_ms > 0 {
                        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                    }

                    let step_start = std::time::Instant::now();
                    let op = step.op.as_str();

                    log(
                        LogLevel::Debug,
                        LogCategory::DebugServer,
                        format!("[E2E]   step[{}]: op='{}', params={}", step_index, op, step.params),
                        None,
                    );

                    // Check if this is an assertion step
                    if op.starts_with("assert_") {
                        let result = evaluate_assertion(op, &step.params, callback_info, app_data);
                        if result.passed {
                            step_results.push(E2eStepResult {
                                step_index,
                                op: op.to_string(),
                                status: "pass".into(),
                                duration_ms: step_start.elapsed().as_millis() as u64,
                                logs: vec![result.message],
                                screenshot: None,
                                error: None,
                                response: None,
                            });
                        } else {
                            test_failed = true;
                            let error_msg = if let (Some(ref exp), Some(ref act)) = (&result.expected, &result.actual) {
                                format!("{}: expected {}, got {}", result.message, exp, act)
                            } else {
                                result.message.clone()
                            };
                            step_results.push(E2eStepResult {
                                step_index,
                                op: op.to_string(),
                                status: "fail".into(),
                                duration_ms: step_start.elapsed().as_millis() as u64,
                                logs: vec![],
                                screenshot: None,
                                error: Some(error_msg),
                                response: None,
                            });
                        }
                    } else if op == "restore_snapshot" {
                        // Look up snapshot alias from the snapshots map
                        let alias = step.params.get("alias")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if let Some(state_json) = snap_map.get(alias) {
                            // Execute set_app_state with the snapshot data
                            let set_event = DebugEvent::SetAppState {
                                state: state_json.clone(),
                            };
                            let (snap_tx, snap_rx) = mpsc::channel();
                            let snap_req = DebugRequest {
                                request_id: NEXT_REQUEST_ID.fetch_add(1, Ordering::SeqCst),
                                event: set_event,
                                window_id: request.window_id.clone(),
                                wait_for_render: false,
                                response_tx: snap_tx,
                            };
                            let snap_update = process_debug_event(
                                &snap_req,
                                callback_info,
                                app_data,
                                component_map,
                            );
                            if snap_update { needs_update = true; }
                            match snap_rx.try_recv() {
                                Ok(DebugResponseData::Ok { .. }) => {
                                    step_results.push(E2eStepResult {
                                        step_index,
                                        op: op.to_string(),
                                        status: "pass".into(),
                                        duration_ms: step_start.elapsed().as_millis() as u64,
                                        logs: vec![format!("Restored snapshot '{}'", alias)],
                                        screenshot: None,
                                        error: None,
                                        response: None,
                                    });
                                }
                                Ok(DebugResponseData::Err(msg)) => {
                                    test_failed = true;
                                    step_results.push(E2eStepResult {
                                        step_index,
                                        op: op.to_string(),
                                        status: "fail".into(),
                                        duration_ms: step_start.elapsed().as_millis() as u64,
                                        logs: vec![],
                                        screenshot: None,
                                        error: Some(format!("restore_snapshot '{}' failed: {}", alias, msg)),
                                        response: None,
                                    });
                                }
                                Err(_) => {
                                    step_results.push(E2eStepResult {
                                        step_index,
                                        op: op.to_string(),
                                        status: "pass".into(),
                                        duration_ms: step_start.elapsed().as_millis() as u64,
                                        logs: vec![format!("Restored snapshot '{}' (no response)", alias)],
                                        screenshot: None,
                                        error: None,
                                        response: None,
                                    });
                                }
                            }
                        } else {
                            test_failed = true;
                            step_results.push(E2eStepResult {
                                step_index,
                                op: op.to_string(),
                                status: "fail".into(),
                                duration_ms: step_start.elapsed().as_millis() as u64,
                                logs: vec![],
                                screenshot: None,
                                error: Some(format!("Snapshot '{}' not found in snapshots map", alias)),
                                response: None,
                            });
                        }
                    } else {
                        // Regular debug command — try to parse as DebugEvent and execute
                        // Build a JSON object with "op" + flattened params
                        let mut cmd = serde_json::Map::new();
                        cmd.insert("op".to_string(), serde_json::Value::String(op.to_string()));
                        if let serde_json::Value::Object(map) = &step.params {
                            for (k, v) in map {
                                if k != "op" && k != "screenshot" {
                                    cmd.insert(k.clone(), v.clone());
                                }
                            }
                        }

                        let cmd_json = serde_json::Value::Object(cmd);
                        match serde_json::from_value::<DebugEvent>(cmd_json.clone()) {
                            Ok(debug_event) => {
                                // Create a temporary request to execute this step
                                let (step_tx, step_rx) = mpsc::channel();
                                let step_request = DebugRequest {
                                    request_id: NEXT_REQUEST_ID.fetch_add(1, Ordering::SeqCst),
                                    event: debug_event,
                                    window_id: request.window_id.clone(),
                                    wait_for_render: false,
                                    response_tx: step_tx,
                                };

                                // Execute the step by recursively calling process_debug_event
                                let step_needs_update = process_debug_event(
                                    &step_request,
                                    callback_info,
                                    app_data,
                                    component_map,
                                );
                                if step_needs_update {
                                    needs_update = true;
                                }

                                // Collect the result
                                match step_rx.try_recv() {
                                    Ok(DebugResponseData::Ok { data, .. }) => {
                                        let response_json = data.as_ref().and_then(|d| {
                                            serde_json::to_value(d).ok()
                                        });

                                        // Check for semantic failures in the response:
                                        // Some commands return status "ok" but contain
                                        // success: false, found: false, or passed: false
                                        // inside data.value — these should fail the step.
                                        let semantic_failure = response_json.as_ref().and_then(|rj| {
                                            rj.get("value")
                                        }).and_then(|val| {
                                            if val.get("success") == Some(&serde_json::Value::Bool(false)) {
                                                Some(val.get("message")
                                                    .or_else(|| val.get("error").and_then(|e| e.get("message")))
                                                    .and_then(|m| m.as_str())
                                                    .unwrap_or("success: false")
                                                    .to_string())
                                            } else if val.get("found") == Some(&serde_json::Value::Bool(false)) {
                                                Some(val.get("message")
                                                    .and_then(|m| m.as_str())
                                                    .unwrap_or("found: false")
                                                    .to_string())
                                            } else if val.get("passed") == Some(&serde_json::Value::Bool(false)) {
                                                Some(val.get("message")
                                                    .and_then(|m| m.as_str())
                                                    .unwrap_or("passed: false")
                                                    .to_string())
                                            } else {
                                                None
                                            }
                                        });

                                        if let Some(error_msg) = semantic_failure {
                                            test_failed = true;
                                            step_results.push(E2eStepResult {
                                                step_index,
                                                op: op.to_string(),
                                                status: "fail".into(),
                                                duration_ms: step_start.elapsed().as_millis() as u64,
                                                logs: vec![],
                                                screenshot: None,
                                                error: Some(error_msg),
                                                response: response_json,
                                            });
                                        } else {
                                            step_results.push(E2eStepResult {
                                                step_index,
                                                op: op.to_string(),
                                                status: "pass".into(),
                                                duration_ms: step_start.elapsed().as_millis() as u64,
                                                logs: vec![format!("Executed: {}", op)],
                                                screenshot: None,
                                                error: None,
                                                response: response_json,
                                            });
                                        }
                                    }
                                    Ok(DebugResponseData::Err(msg)) => {
                                        test_failed = true;
                                        step_results.push(E2eStepResult {
                                            step_index,
                                            op: op.to_string(),
                                            status: "fail".into(),
                                            duration_ms: step_start.elapsed().as_millis() as u64,
                                            logs: vec![],
                                            screenshot: None,
                                            error: Some(msg),
                                            response: None,
                                        });
                                    }
                                    Err(_) => {
                                        // No response received — step executed but didn't send response
                                        step_results.push(E2eStepResult {
                                            step_index,
                                            op: op.to_string(),
                                            status: "pass".into(),
                                            duration_ms: step_start.elapsed().as_millis() as u64,
                                            logs: vec![format!("Executed (no response): {}", op)],
                                            screenshot: None,
                                            error: None,
                                            response: None,
                                        });
                                    }
                                }
                            }
                            Err(e) => {
                                test_failed = true;
                                step_results.push(E2eStepResult {
                                    step_index,
                                    op: op.to_string(),
                                    status: "fail".into(),
                                    duration_ms: step_start.elapsed().as_millis() as u64,
                                    logs: vec![],
                                    screenshot: None,
                                    error: Some(format!("Unknown op '{}': {}", op, e)),
                                    response: None,
                                });
                            }
                        }
                    }

                    // Log step result
                    if let Some(last) = step_results.last() {
                        log(
                            LogLevel::Debug,
                            LogCategory::DebugServer,
                            format!("[E2E]   step[{}] => {} ({} ms){}",
                                step_index,
                                last.status,
                                last.duration_ms,
                                last.error.as_ref().map(|e| format!(": {}", e)).unwrap_or_default(),
                            ),
                            None,
                        );
                    }

                    // Stop executing further steps if a step failed (unless continue_on_failure)
                    if test_failed && !continue_on_failure {
                        log(
                            LogLevel::Debug,
                            LogCategory::DebugServer,
                            format!("[E2E]   stopping test '{}' early (continue_on_failure=false)", test.name),
                            None,
                        );
                        break;
                    }
                }

                let steps_passed = step_results.iter().filter(|s| s.status == "pass").count();
                let steps_failed = step_results.iter().filter(|s| s.status == "fail").count();
                let duration = test_start.elapsed().as_millis();

                log(
                    LogLevel::Info,
                    LogCategory::DebugServer,
                    format!("[E2E] === Test '{}' {} ({} ms, {}/{} passed) ===",
                        test.name,
                        if test_failed { "FAILED" } else { "PASSED" },
                        duration,
                        steps_passed,
                        test.steps.len(),
                    ),
                    None,
                );

                all_results.push(E2eTestResult {
                    name: test.name.clone(),
                    status: if test_failed { "fail" } else { "pass" }.into(),
                    duration_ms: duration as u64,
                    step_count: test.steps.len(),
                    steps_passed,
                    steps_failed,
                    steps: step_results,
                    final_screenshot: None, // TODO: CpuRender screenshot
                });
            }

            send_ok(
                request,
                None,
                Some(ResponseData::E2eResults(E2eResultsResponse {
                    results: all_results,
                })),
            );
        }

        // === DOM Mutation ===

        DebugEvent::InsertNode { parent_id, node_type, position, classes, id } => {
            use azul_core::dom::DomId;
            use azul_core::id::NodeId;

            let dom_id = DomId { inner: 0 };
            let parent_node_id = NodeId::new(*parent_id as usize);

            // Validate parent exists
            let layout_window = callback_info.get_layout_window();
            let node_count = layout_window.layout_results.get(&dom_id)
                .map(|lr| lr.styled_dom.node_data.as_ref().len())
                .unwrap_or(0);

            if parent_node_id.index() >= node_count {
                send_err(request, format!("Parent node {} not found (total nodes: {})", parent_id, node_count));
            } else {
                let new_node_id = node_count as u64; // New node will be appended at end
                let classes_az: Vec<azul_css::AzString> = classes.iter()
                    .map(|c| azul_css::AzString::from(c.as_str()))
                    .collect();
                let id_az = id.as_ref().map(|i| azul_css::AzString::from(i.as_str()));

                callback_info.insert_child_node(
                    dom_id,
                    parent_node_id,
                    azul_css::AzString::from(node_type.as_str()),
                    *position,
                    classes_az,
                    id_az,
                );
                needs_update = true;

                send_ok(
                    request,
                    None,
                    Some(ResponseData::NodeInserted(NodeInsertedResponse {
                        new_node_id,
                        parent_id: *parent_id,
                        node_type: node_type.clone(),
                    })),
                );
            }
        }

        DebugEvent::DeleteNode { node_id } => {
            use azul_core::dom::DomId;
            use azul_core::id::NodeId;

            let dom_id = DomId { inner: 0 };
            let target_node_id = NodeId::new(*node_id as usize);

            let layout_window = callback_info.get_layout_window();
            let node_count = layout_window.layout_results.get(&dom_id)
                .map(|lr| lr.styled_dom.node_data.as_ref().len())
                .unwrap_or(0);

            if target_node_id.index() >= node_count || *node_id == 0 {
                send_err(request, format!("Cannot delete node {} (root or out of range)", node_id));
            } else {
                callback_info.delete_node(dom_id, target_node_id);
                needs_update = true;

                send_ok(
                    request,
                    None,
                    Some(ResponseData::NodeDeleted(NodeDeletedResponse {
                        node_id: *node_id,
                        success: true,
                    })),
                );
            }
        }

        DebugEvent::SetNodeText { node_id, text } => {
            use azul_core::dom::{DomId, DomNodeId};
            use azul_core::styled_dom::NodeHierarchyItemId;
            use azul_core::id::NodeId;

            let dom_id = DomId { inner: 0 };
            let target_node_id = NodeId::new(*node_id as usize);

            let layout_window = callback_info.get_layout_window();
            let node_count = layout_window.layout_results.get(&dom_id)
                .map(|lr| lr.styled_dom.node_data.as_ref().len())
                .unwrap_or(0);

            if target_node_id.index() >= node_count {
                send_err(request, format!("Node {} not found", node_id));
            } else {
                let dom_node_id = DomNodeId {
                    dom: dom_id,
                    node: NodeHierarchyItemId::from_crate_internal(Some(target_node_id)),
                };
                callback_info.change_node_text(
                    dom_node_id,
                    azul_css::AzString::from(text.as_str()),
                );
                needs_update = true;

                send_ok(
                    request,
                    None,
                    Some(ResponseData::NodeTextSet(NodeTextSetResponse {
                        node_id: *node_id,
                        new_text: text.clone(),
                    })),
                );
            }
        }

        DebugEvent::SetNodeClasses { node_id, classes, id } => {
            use azul_core::dom::{DomId, IdOrClass};
            use azul_core::id::NodeId;

            let dom_id = DomId { inner: 0 };
            let target_node_id = NodeId::new(*node_id as usize);

            let layout_window = callback_info.get_layout_window();
            let node_count = layout_window.layout_results.get(&dom_id)
                .map(|lr| lr.styled_dom.node_data.as_ref().len())
                .unwrap_or(0);

            if target_node_id.index() >= node_count {
                send_err(request, format!("Node {} not found", node_id));
            } else {
                let mut ids_and_classes = Vec::new();
                if let Some(id_str) = id {
                    ids_and_classes.push(IdOrClass::Id(azul_css::AzString::from(id_str.as_str())));
                }
                for class in classes.iter() {
                    ids_and_classes.push(IdOrClass::Class(azul_css::AzString::from(class.as_str())));
                }

                callback_info.set_node_ids_and_classes(
                    dom_id,
                    target_node_id,
                    ids_and_classes.into(),
                );
                needs_update = true;

                send_ok(
                    request,
                    None,
                    Some(ResponseData::NodeClassesSet(NodeClassesSetResponse {
                        node_id: *node_id,
                        classes: classes.clone(),
                        id: id.clone(),
                    })),
                );
            }
        }

        DebugEvent::SetNodeCssOverride { node_id, property, value } => {
            use azul_core::dom::DomId;
            use azul_core::id::NodeId;
            use azul_css::props::property::{CssPropertyType, get_css_key_map};

            let dom_id = DomId { inner: 0 };
            let target_node_id = NodeId::new(*node_id as usize);

            let layout_window = callback_info.get_layout_window();
            let node_count = layout_window.layout_results.get(&dom_id)
                .map(|lr| lr.styled_dom.node_data.as_ref().len())
                .unwrap_or(0);

            if target_node_id.index() >= node_count {
                send_err(request, format!("Node {} not found", node_id));
            } else {
                let key_map = get_css_key_map();
                match CssPropertyType::from_str(&property, &key_map) {
                    Some(prop_type) => {
                        match azul_css::props::property::parse_css_property(prop_type, &value) {
                            Ok(css_prop) => {
                                callback_info.change_node_css_properties(
                                    dom_id,
                                    target_node_id,
                                    vec![css_prop].into(),
                                );
                                needs_update = true;

                                send_ok(
                                    request,
                                    None,
                                    Some(ResponseData::NodeCssOverrideSet(NodeCssOverrideSetResponse {
                                        node_id: *node_id,
                                        property: property.clone(),
                                        value: value.clone(),
                                    })),
                                );
                            }
                            Err(e) => {
                                send_err(request, format!("Failed to parse CSS value '{}' for property '{}': {:?}", value, property, e));
                            }
                        }
                    }
                    None => {
                        send_err(request, format!("Unknown CSS property: '{}'", property));
                    }
                }
            }
        }

        DebugEvent::ResolveFunctionPointers { addresses } => {
            let mut resolved = Vec::new();

            for addr_str in addresses.iter() {
                // Support both decimal and hex (0x...) addresses
                let address = if addr_str.starts_with("0x") || addr_str.starts_with("0X") {
                    usize::from_str_radix(&addr_str[2..], 16).unwrap_or(0)
                } else {
                    addr_str.parse::<usize>().unwrap_or(0)
                };
                let info = resolve_function_pointer(address);
                resolved.push(ResolvedFunctionPointer {
                    address: addr_str.clone(),
                    symbol_name: info.symbol_name,
                    file_name: info.file_name,
                    source_file: info.source_file,
                    source_line: info.source_line,
                    hint: info.hint,
                    approximate: info.approximate,
                });
            }

            send_ok(
                request,
                None,
                Some(ResponseData::FunctionPointers(FunctionPointersResponse {
                    resolved,
                })),
            );
        }

        DebugEvent::GetComponentRegistry => {
            let map_guard = component_map.lock().unwrap();
            let registry = build_component_registry(&*map_guard);
            drop(map_guard);
            send_ok(
                request,
                None,
                Some(ResponseData::ComponentRegistry(registry)),
            );
        }

        DebugEvent::GetLibraries => {
            let map_guard = component_map.lock().unwrap();
            let registry = build_component_registry(&*map_guard);
            drop(map_guard);
            let libraries = registry.libraries.iter().map(|lib| LibrarySummary {
                name: lib.name.clone(),
                version: lib.version.clone(),
                description: lib.description.clone(),
                exportable: lib.exportable,
                modifiable: lib.modifiable,
                component_count: lib.components.len(),
            }).collect();
            send_ok(
                request,
                None,
                Some(ResponseData::Libraries(LibraryListResponse { libraries })),
            );
        }

        DebugEvent::GetLibraryComponents { library } => {
            let map_guard = component_map.lock().unwrap();
            let registry = build_component_registry(&*map_guard);
            drop(map_guard);
            if let Some(lib) = registry.libraries.iter().find(|l| l.name == *library) {
                send_ok(
                    request,
                    None,
                    Some(ResponseData::LibraryComponents(LibraryComponentsResponse {
                        library: library.clone(),
                        components: lib.components.clone(),
                    })),
                );
            } else {
                let available: Vec<_> = registry.libraries.iter().map(|l| l.name.as_str()).collect();
                send_err(request, &format!(
                    "Library '{}' not found. Available: {:?}", library, available
                ));
            }
        }

        DebugEvent::ExportCode { language } => {
            let map_guard = component_map.lock().unwrap();
            let result = build_exported_code(&language, &*map_guard);
            drop(map_guard);
            match result {
                Ok(response) => {
                    send_ok(
                        request,
                        None,
                        Some(ResponseData::ExportedCode(response)),
                    );
                }
                Err(e) => {
                    send_err(request, &format!("Export failed: {}", e));
                }
            }
        }

        DebugEvent::ImportComponentLibrary { library: lib_json } => {
            use azul_core::xml::{
                ComponentDef, ComponentId, ComponentDataModel,
                ComponentLibrary, ChildPolicy,
                ComponentSource, ComponentDefVec, ComponentLibraryVec,
                ComponentDataFieldVec,
            };
            use azul_css::corety::AzString;

            let lib_name = lib_json.name.clone();
            let component_count = lib_json.components.len();

            // Convert ExportedLibraryResponse -> ComponentLibrary with validation
            let mut defs = Vec::new();
            let mut validation_errors = Vec::new();

            for c in &lib_json.components {
                let child_policy = match c.child_policy.as_str() {
                    "no_children" => ChildPolicy::NoChildren,
                    "text_only" => ChildPolicy::TextOnly,
                    _ => ChildPolicy::AnyChildren,
                };

                // Validate and convert all fields
                let validated_fields = match validate_exported_fields(&c.fields) {
                    Ok(fields) => fields,
                    Err(e) => {
                        validation_errors.push(format!("Component '{}': {}", c.name, e));
                        continue;
                    }
                };

                let display_name_str = if c.display_name.is_empty() { &c.name } else { &c.display_name };
                defs.push(ComponentDef {
                    id: ComponentId::new(&lib_name, &c.name),
                    display_name: AzString::from(display_name_str.as_str()),
                    description: AzString::from(c.description.as_str()),
                    accepts_text: c.accepts_text,
                    child_policy,
                    scoped_css: AzString::from(c.scoped_css.as_str()),
                    example_xml: AzString::from(c.example_xml.as_str()),
                    source: ComponentSource::UserDefined,
                    data_model: ComponentDataModel {
                        name: AzString::from(format!("{}Data", display_name_str).as_str()),
                        description: AzString::from(c.description.as_str()),
                        fields: ComponentDataFieldVec::from_vec(validated_fields),
                    },
                    template: AzString::from(c.template.as_str()),
                    render_fn: azul_core::xml::user_defined_render_fn,
                    compile_fn: azul_core::xml::user_defined_compile_fn,
                    node_type: azul_core::dom::OptionNodeType::None,
                });
            }

            if !validation_errors.is_empty() {
                send_err(request, &format!(
                    "Validation errors in library '{}': {}",
                    lib_name,
                    validation_errors.join("; ")
                ));
            } else {
                let new_lib = ComponentLibrary {
                    name: AzString::from(lib_name.as_str()),
                    version: AzString::from(lib_json.version.as_str()),
                    description: AzString::from(lib_json.description.as_str()),
                    components: ComponentDefVec::from_vec(defs),
                    exportable: true,
                    modifiable: true,
                    data_models: azul_core::xml::ComponentDataModelVec::from_const_slice(&[]),
                    enum_models: azul_core::xml::ComponentEnumModelVec::from_const_slice(&[]),
                };

                // Insert or replace in the component map
                let mut map_guard = component_map.lock().unwrap();
                let empty_libs = ComponentLibraryVec::from_const_slice(&[]);
                let mut libs = core::mem::replace(&mut map_guard.libraries, empty_libs).into_library_owned_vec();
                let was_update = if let Some(existing) = libs.iter_mut().find(|l| l.name.as_str() == lib_name) {
                    *existing = new_lib;
                    true
                } else {
                    libs.push(new_lib);
                    false
                };
                map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                drop(map_guard);

                send_ok(request, None, Some(ResponseData::ImportedLibrary(ImportedLibraryResponse {
                    library_name: lib_name,
                    component_count,
                    was_update,
                })));
                needs_update = true;
            }
        }

        DebugEvent::ExportComponentLibrary { library: lib_name_opt } => {
            let map_guard = component_map.lock().unwrap();
            let registry = build_component_registry(&*map_guard);
            drop(map_guard);

            let exportable_libs: Vec<&ComponentLibraryInfo> = registry.libraries.iter()
                .filter(|lib| lib.exportable)
                .filter(|lib| lib_name_opt.as_ref().map_or(true, |n| &lib.name == n))
                .collect();

            if exportable_libs.is_empty() {
                if let Some(ref name) = lib_name_opt {
                    send_err(request, &format!(
                        "Library '{}' not found or is not exportable (builtin/compiled libraries cannot be exported)", name
                    ));
                } else {
                    send_err(request, "No exportable component libraries found. Only user-defined libraries can be exported.");
                }
            } else {
                // Export the first matching library (or the only one)
                let lib = exportable_libs[0];
                let exported = ExportedLibraryResponse {
                    name: lib.name.clone(),
                    version: lib.version.clone(),
                    description: lib.description.clone(),
                    components: lib.components.iter().map(|c| {
                        // Build unified fields from data_model + callback_slots
                        let mut fields: Vec<ExportedDataField> = c.data_model.iter().map(|f| ExportedDataField {
                            name: f.name.clone(),
                            field_type: f.field_type.clone(),
                            default: f.default.clone(),
                            description: f.description.clone(),
                        }).collect();
                        for s in c.callback_slots.iter() {
                            fields.push(ExportedDataField {
                                name: s.name.clone(),
                                field_type: s.callback_type.clone(),
                                default: None,
                                description: s.description.clone(),
                            });
                        }
                        ExportedComponentDef {
                            name: c.tag.clone(),
                            display_name: c.display_name.clone(),
                            description: c.description.clone(),
                            fields,
                            accepts_text: c.accepts_text,
                            child_policy: c.child_policy.clone(),
                            scoped_css: c.scoped_css.clone(),
                            example_xml: c.example_xml.clone(),
                            template: c.template.clone(),
                        }
                    }).collect(),
                };
                send_ok(request, None, Some(ResponseData::ExportedLibrary(exported)));
            }
        }

        DebugEvent::CreateLibrary { name, description } => {
            use azul_core::xml::{ComponentLibrary, ComponentDefVec, ComponentLibraryVec, ComponentDataModelVec};
            use azul_css::corety::AzString;

            let mut map_guard = component_map.lock().unwrap();
            // Check if library already exists
            if map_guard.libraries.iter().any(|l| l.name.as_str() == name.as_str()) {
                drop(map_guard);
                send_err(request, &format!("Library '{}' already exists", name));
            } else {
                let new_lib = ComponentLibrary {
                    name: AzString::from(name.as_str()),
                    version: AzString::from_const_str("0.1.0"),
                    description: AzString::from(description.as_deref().unwrap_or("")),
                    components: ComponentDefVec::from_const_slice(&[]),
                    exportable: true,
                    modifiable: true,
                    data_models: ComponentDataModelVec::from_const_slice(&[]),
                    enum_models: azul_core::xml::ComponentEnumModelVec::from_const_slice(&[]),
                };
                let empty_libs = ComponentLibraryVec::from_const_slice(&[]);
                let mut libs = core::mem::replace(&mut map_guard.libraries, empty_libs).into_library_owned_vec();
                libs.push(new_lib);
                map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                drop(map_guard);
                send_ok(request, None, None);
            }
        }

        DebugEvent::DeleteLibrary { name } => {
            use azul_core::xml::ComponentLibraryVec;

            let mut map_guard = component_map.lock().unwrap();
            let empty_libs = ComponentLibraryVec::from_const_slice(&[]);
            let mut libs = core::mem::replace(&mut map_guard.libraries, empty_libs).into_library_owned_vec();
            let original_len = libs.len();

            // Only allow deletion of modifiable libraries
            if let Some(lib) = libs.iter().find(|l| l.name.as_str() == name.as_str()) {
                if !lib.modifiable {
                    map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                    drop(map_guard);
                    send_err(request, &format!("Library '{}' is not modifiable and cannot be deleted", name));
                } else {
                    libs.retain(|l| l.name.as_str() != name.as_str());
                    map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                    drop(map_guard);
                    send_ok(request, None, None);
                }
            } else {
                map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                drop(map_guard);
                send_err(request, &format!("Library '{}' not found", name));
            }
        }

        DebugEvent::CreateComponent { library, name, display_name } => {
            use azul_core::xml::{ComponentDef, ComponentId, ComponentSource, ChildPolicy, ComponentLibraryVec, ComponentDataModel, ComponentDataFieldVec};
            use azul_css::corety::AzString;

            let mut map_guard = component_map.lock().unwrap();
            let empty_libs = ComponentLibraryVec::from_const_slice(&[]);
            let mut libs = core::mem::replace(&mut map_guard.libraries, empty_libs).into_library_owned_vec();

            if let Some(lib) = libs.iter_mut().find(|l| l.name.as_str() == library.as_str()) {
                if !lib.modifiable {
                    map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                    drop(map_guard);
                    send_err(request, &format!("Library '{}' is not modifiable", library));
                } else {
                    let display = display_name.as_deref().unwrap_or(name.as_str());
                    let new_def = ComponentDef {
                        id: ComponentId::new(library.as_str(), name.as_str()),
                        display_name: AzString::from(display),
                        description: AzString::from_const_str(""),
                        accepts_text: false,
                        child_policy: ChildPolicy::AnyChildren,
                        scoped_css: AzString::from_const_str(""),
                        example_xml: AzString::from(format!("<{} />", name).as_str()),
                        source: ComponentSource::UserDefined,
                        data_model: ComponentDataModel {
                            name: AzString::from(format!("{}Data", display).as_str()),
                            description: AzString::from_const_str(""),
                            fields: ComponentDataFieldVec::from_const_slice(&[]),
                        },
                        template: AzString::from_const_str(""),
                        render_fn: azul_core::xml::user_defined_render_fn,
                        compile_fn: azul_core::xml::user_defined_compile_fn,
                        node_type: azul_core::dom::OptionNodeType::None,
                    };
                    let mut comps = core::mem::replace(&mut lib.components, Vec::new().into()).into_library_owned_vec();
                    comps.push(new_def);
                    lib.components = azul_core::xml::ComponentDefVec::from_vec(comps);
                    map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                    drop(map_guard);
                    send_ok(request, None, None);
                }
            } else {
                map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                drop(map_guard);
                send_err(request, &format!("Library '{}' not found", library));
            }
        }

        DebugEvent::DeleteComponent { library, name } => {
            use azul_core::xml::ComponentLibraryVec;

            let mut map_guard = component_map.lock().unwrap();
            let empty_libs = ComponentLibraryVec::from_const_slice(&[]);
            let mut libs = core::mem::replace(&mut map_guard.libraries, empty_libs).into_library_owned_vec();

            if let Some(lib) = libs.iter_mut().find(|l| l.name.as_str() == library.as_str()) {
                if !lib.modifiable {
                    map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                    drop(map_guard);
                    send_err(request, &format!("Library '{}' is not modifiable", library));
                } else {
                    let mut comps = core::mem::replace(&mut lib.components, Vec::new().into()).into_library_owned_vec();
                    comps.retain(|c| c.id.name.as_str() != name.as_str());
                    lib.components = azul_core::xml::ComponentDefVec::from_vec(comps);
                    map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                    drop(map_guard);
                    send_ok(request, None, None);
                }
            } else {
                map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                drop(map_guard);
                send_err(request, &format!("Library '{}' not found", library));
            }
        }

        DebugEvent::UpdateComponent { library, name, template, scoped_css, description, display_name, fields } => {
            use azul_core::xml::ComponentLibraryVec;
            use azul_css::corety::AzString;

            let mut map_guard = component_map.lock().unwrap();
            let empty_libs = ComponentLibraryVec::from_const_slice(&[]);
            let mut libs = core::mem::replace(&mut map_guard.libraries, empty_libs).into_library_owned_vec();

            if let Some(lib) = libs.iter_mut().find(|l| l.name.as_str() == library.as_str()) {
                if !lib.modifiable {
                    map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                    drop(map_guard);
                    send_err(request, &format!("Library '{}' is not modifiable", library));
                } else {
                    let mut comps = core::mem::replace(&mut lib.components, Vec::new().into()).into_library_owned_vec();
                    if let Some(comp) = comps.iter_mut().find(|c| c.id.name.as_str() == name.as_str()) {
                        if let Some(t) = template {
                            comp.template = AzString::from(t.as_str());
                        }
                        if let Some(css) = scoped_css {
                            comp.scoped_css = AzString::from(css.as_str());
                        }
                        if let Some(desc) = description {
                            comp.description = AzString::from(desc.as_str());
                        }
                        if let Some(dn) = display_name {
                            comp.display_name = AzString::from(dn.as_str());
                        }
                        // Replace data_model.fields with validated new fields (if provided)
                        if let Some(new_fields) = fields {
                            match validate_exported_fields(&new_fields) {
                                Ok(validated) => {
                                    comp.data_model.fields = validated.into();
                                }
                                Err(e) => {
                                    lib.components = azul_core::xml::ComponentDefVec::from_vec(comps);
                                    map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                                    drop(map_guard);
                                    send_err(request, &format!(
                                        "Validation error in component '{}': {}", name, e
                                    ));
                                    return needs_update;
                                }
                            }
                        }
                        lib.components = azul_core::xml::ComponentDefVec::from_vec(comps);
                        map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                        drop(map_guard);
                        needs_update = true;
                        send_ok(request, None, None);
                    } else {
                        lib.components = azul_core::xml::ComponentDefVec::from_vec(comps);
                        map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                        drop(map_guard);
                        send_err(request, &format!("Component '{}' not found in library '{}'", name, library));
                    }
                }
            } else {
                map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                drop(map_guard);
                send_err(request, &format!("Library '{}' not found", library));
            }
        }

        DebugEvent::OpenFile { file, line } => {
            // Best-effort: open file in the user's default editor
            // Use platform-native "open" command (not "code") to respect user's preference
            let result = {
                #[cfg(target_os = "macos")]
                {
                    // macOS `open` doesn't support line numbers, so try `code --goto` first for precision
                    if *line > 0 {
                        std::process::Command::new("open")
                            .arg(file.as_str())
                            .spawn()
                    } else {
                        std::process::Command::new("open").arg(file.as_str()).spawn()
                    }
                }
                #[cfg(target_os = "linux")]
                {
                    std::process::Command::new("xdg-open").arg(file.as_str()).spawn()
                }
                #[cfg(target_os = "windows")]
                {
                    std::process::Command::new("cmd").args(&["/C", "start", "", file.as_str()]).spawn()
                }
            };
            match result {
                Ok(_) => send_ok(request, None, None),
                Err(e) => send_err(request, format!("Failed to open {}: {}", file, e)),
            }
        }

        _ => {
            log(
                LogLevel::Warn,
                LogCategory::DebugServer,
                format!("Unhandled: {:?}", request.event),
                None,
            );
            send_ok(request, None, None);
        }
    }

    needs_update
}

/// Create a Timer for the debug server polling.
///
/// # Arguments
/// * `app_data` - The application state (`GetAppState` / `SetAppState`)
/// * `get_system_time_fn` - Callback to get the current system time
/// * `request_rx` - The spmc receiver for debug requests (cloned per window)
/// * `component_map` - Shared component map (Arc-cloned per window)
/// * `window_id` - This window's unique ID string
#[cfg(feature = "std")]
pub fn create_debug_timer(
    app_data: azul_core::refany::RefAny,
    get_system_time_fn: azul_core::task::GetSystemTimeCallback,
    request_rx: spmc::Receiver<DebugRequest>,
    component_map: Arc<Mutex<azul_core::xml::ComponentMap>>,
    window_id: String,
) -> azul_layout::timer::Timer {
    use azul_core::task::Duration;
    use azul_layout::timer::{Timer, TimerCallback};

    let timer_data = azul_core::refany::RefAny::new(DebugTimerData {
        app_data,
        component_map,
        request_rx,
        window_id,
    });

    Timer::create(
        timer_data,
        TimerCallback::create(debug_timer_callback),
        get_system_time_fn,
    )
    .with_interval(Duration::System(
        azul_core::task::SystemTimeDiff::from_millis(16),
    ))
}

/// Register the debug timer on a window if `AZUL_DEBUG` or E2E mode is active.
///
/// This is the single cross-platform entry point that replaces the
/// copy-pasted registration blocks in each platform window constructor.
/// It reads `app_data` and `window_id` from the window, then creates
/// a `DebugTimerData` with the given channel receiver and component map.
#[cfg(feature = "std")]
pub fn register_debug_timer(
    window: &mut dyn crate::desktop::shell2::common::event::PlatformWindow,
    request_rx: spmc::Receiver<DebugRequest>,
    component_map: Arc<Mutex<azul_core::xml::ComponentMap>>,
) {
    if !is_debug_enabled() {
        return;
    }

    log(
        LogLevel::Debug,
        LogCategory::DebugServer,
        "[Window Init] Registering debug timer",
        None,
    );

    let timer_id: usize = 0xDEBE;
    let app_data_for_timer = window.get_app_data().borrow().clone();
    let window_id = window.get_current_window_state().window_id.as_str().to_string();
    let get_system_time_fn = azul_layout::callbacks::ExternalSystemCallbacks::rust_internal().get_system_time_fn;
    let debug_timer = create_debug_timer(app_data_for_timer, get_system_time_fn, request_rx, component_map, window_id);
    window.start_timer(timer_id, debug_timer);

    log(
        LogLevel::Debug,
        LogCategory::DebugServer,
        &format!("[Window Init] Debug timer registered with ID 0x{:X}", timer_id),
        None,
    );
}

/// Data stored in the debug timer's `RefAny`.
///
/// Holds the application state, component map, spmc receiver, and window ID
/// so that `debug_timer_callback` can process debug requests for this window.
#[cfg(feature = "std")]
struct DebugTimerData {
    /// The user's application state (`GetAppState` / `SetAppState`)
    app_data: azul_core::refany::RefAny,
    /// Shared component map built from `AppConfig::component_libraries`
    component_map: Arc<Mutex<azul_core::xml::ComponentMap>>,
    /// This window's clone of the spmc receiver
    request_rx: spmc::Receiver<DebugRequest>,
    /// This window's unique ID for request routing
    window_id: String,
}

// ==================== Logging Macros ====================

/// Log a trace message (only evaluated when debug server is active)
#[macro_export]
macro_rules! log_trace {
    ($cat:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::debug_server::is_debug_enabled() {
            $crate::desktop::shell2::common::debug_server::log(
                $crate::desktop::shell2::common::debug_server::LogLevel::Trace,
                $cat,
                format!($($arg)*),
                None,
            );
        }
    };
    ($cat:expr, $win:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::debug_server::is_debug_enabled() {
            $crate::desktop::shell2::common::debug_server::log(
                $crate::desktop::shell2::common::debug_server::LogLevel::Trace,
                $cat,
                format!($($arg)*),
                Some($win),
            );
        }
    };
}

/// Log a debug message (only evaluated when debug server is active)
#[macro_export]
macro_rules! log_debug {
    ($cat:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::debug_server::is_debug_enabled() {
            $crate::desktop::shell2::common::debug_server::log(
                $crate::desktop::shell2::common::debug_server::LogLevel::Debug,
                $cat,
                format!($($arg)*),
                None,
            );
        }
    };
    ($cat:expr, $win:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::debug_server::is_debug_enabled() {
            $crate::desktop::shell2::common::debug_server::log(
                $crate::desktop::shell2::common::debug_server::LogLevel::Debug,
                $cat,
                format!($($arg)*),
                Some($win),
            );
        }
    };
}

/// Log an info message (only evaluated when debug server is active)
#[macro_export]
macro_rules! log_info {
    ($cat:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::debug_server::is_debug_enabled() {
            $crate::desktop::shell2::common::debug_server::log(
                $crate::desktop::shell2::common::debug_server::LogLevel::Info,
                $cat,
                format!($($arg)*),
                None,
            );
        }
    };
    ($cat:expr, $win:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::debug_server::is_debug_enabled() {
            $crate::desktop::shell2::common::debug_server::log(
                $crate::desktop::shell2::common::debug_server::LogLevel::Info,
                $cat,
                format!($($arg)*),
                Some($win),
            );
        }
    };
}

/// Log a warning message (only evaluated when debug server is active)
#[macro_export]
macro_rules! log_warn {
    ($cat:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::debug_server::is_debug_enabled() {
            $crate::desktop::shell2::common::debug_server::log(
                $crate::desktop::shell2::common::debug_server::LogLevel::Warn,
                $cat,
                format!($($arg)*),
                None,
            );
        }
    };
    ($cat:expr, $win:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::debug_server::is_debug_enabled() {
            $crate::desktop::shell2::common::debug_server::log(
                $crate::desktop::shell2::common::debug_server::LogLevel::Warn,
                $cat,
                format!($($arg)*),
                Some($win),
            );
        }
    };
}

/// Log an error message (only evaluated when debug server is active)
#[macro_export]
macro_rules! log_error {
    ($cat:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::debug_server::is_debug_enabled() {
            $crate::desktop::shell2::common::debug_server::log(
                $crate::desktop::shell2::common::debug_server::LogLevel::Error,
                $cat,
                format!($($arg)*),
                None,
            );
        }
    };
    ($cat:expr, $win:expr, $($arg:tt)*) => {
        if $crate::desktop::shell2::common::debug_server::is_debug_enabled() {
            $crate::desktop::shell2::common::debug_server::log(
                $crate::desktop::shell2::common::debug_server::LogLevel::Error,
                $cat,
                format!($($arg)*),
                Some($win),
            );
        }
    };
}

// Re-export log categories for convenience
pub use LogCategory::*;
