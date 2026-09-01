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
//! AZ_DEBUG=8765 cargo run --bin my_app
//!
//! # Send events (blocks until processed)
//! curl -X POST http://localhost:8765/ -d '{"type":"get_state"}'
//! ```

use crate::solver3::layout_tree::LayoutNodeId;
use alloc::collections::BTreeMap;
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

// Import the NativeScreenshotExt trait for native screenshots

#[cfg(feature = "std")]
use std::sync::{mpsc, Arc, Mutex, OnceLock};

const ROOT_DOM_ID: azul_core::dom::DomId = azul_core::dom::DomId { inner: 0 };

/// The DOM an op addresses: the request's `dom_id`, or the root DOM when it
/// said nothing.
///
/// Every node-addressing op runs through this instead of hardcoding
/// [`ROOT_DOM_ID`], so one envelope field reaches all of them. A VirtualView's
/// document and a `<transient-window>`'s popup content are separate DOMs whose
/// nodes DOM 0 cannot see; before this, scripting them meant guessing pixel
/// coordinates. Discover the live ids with the `list_doms` op.
#[cfg(feature = "std")]
fn target_dom(request: &DebugRequest) -> azul_core::dom::DomId {
    request
        .dom_id
        .map_or(ROOT_DOM_ID, |inner| azul_core::dom::DomId {
            inner: inner as usize,
        })
}

/// Wall-clock `wall_clock_now()` for the E2E runner's own bookkeeping
/// (per-step durations and the `wait` op's resume deadline).
///
/// Every such read in this module funnels through here so that the WASM-compat
/// CI gate ("no ungated `wall_clock_now()` in azul-css / azul-core /
/// azul-layout", `rust.yml` -> Lint & Static Checks) has exactly ONE site to
/// look at instead of a call scattered through a 12k-line dispatcher.
///
/// It is sound to read the real clock here: the whole `e2e` module is
/// `#[cfg(feature = "e2e-server")]`, a desktop-test-only feature that is not in
/// `default` and is never enabled for a wasm build — which is the target where
/// `Instant::now()` panics. This is deliberately NOT the injectable test clock
/// (`azul_core::task::Instant::now`): step durations and the `wait` deadline
/// must measure real elapsed time, or `tick_ms` would let a scenario "wait"
/// without the event loop ever making progress.
// The `not(target_family = "wasm")` states in code what the comment above
// claims: `e2e-server` is not in `default` and is never enabled for a wasm
// build. A feature gate is not a target exclusion, so without this the guard
// was an assertion in prose only.
#[cfg(all(feature = "std", not(target_family = "wasm")))]
fn wall_clock_now() -> std::time::Instant {
    std::time::Instant::now()
}

// ==================== Types ====================

/// Request from HTTP thread to timer callback
#[cfg(feature = "std")]
pub struct DebugRequest {
    pub request_id: u64,
    pub event: DebugEvent,
    pub window_id: Option<String>,
    pub wait_for_render: bool,
    /// Which DOM the op addresses. `None` = the root DOM (0), which is what
    /// every op used to hardcode. A VirtualView's document and a
    /// `<transient-window>`'s popup content are DOMs of their own, and their
    /// nodes are unreachable from DOM 0: without this, scripting them meant
    /// hand-rolling coordinates and hoping. Set once on the request envelope
    /// rather than per op, so EVERY node-addressing op honours it.
    /// Discover the live ids with the `list_doms` op.
    pub dom_id: Option<u64>,
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
fn profile_kind_memory() -> ProfileKind {
    ProfileKind::Memory
}

/// Which profile a `GetProfileReport` op wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileKind {
    Memory,
    Cpu,
}

/// A profile snapshot, in the SAME units the engine's own report prints:
/// **KiB** (which is also what `/proc` reports, despite its "kB" label).
/// Mixing that with a decimal-MB source is a 4.9% error and has produced
/// wrong conclusions in this project's own analysis, so the unit is in every
/// field name.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ProfileResponse {
    /// Resident set size — the number an assertion like "under 50 MB" means.
    pub rss_kib: u64,
    pub heap_kib: u64,
    pub anon_kib: u64,
    pub binary_kib: u64,
    pub shared_libs_kib: u64,
    pub font_files_kib: u64,
    pub framebuffer_kib: u64,
    /// Live allocations per the allocator. `None` where `mallinfo2` is
    /// unavailable (musl, glibc < 2.33, macOS) — **null, never 0**, because a
    /// zero here reads as "no memory held", the worst available wrong answer.
    pub allocator_live_kib: Option<u64>,
    /// Freed by the program but still held by the allocator. Churn, not data.
    pub allocator_free_in_arena_kib: Option<u64>,
    /// Log messages evicted by the debug queue cap. **Non-zero invalidates a
    /// memory assertion taken under `AZ_E2E`**: the logger allocates per
    /// message, and an unbounded version of this queue once cost 41 MB and
    /// was briefly misreported as a resize leak.
    pub logs_dropped: u64,
    /// Wall-clock of recorded phases, `Cpu` kind only.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub phases_us: Vec<(String, u64)>,
}

/// What the application's handler returned for a `CustomOp`.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct CustomOpResponse {
    /// The op name, echoed so an async consumer can match responses.
    pub op: String,
    /// The handler's JSON result, verbatim. `null` if it returned nothing
    /// parseable — the raw string is still in `raw`.
    pub result: Option<serde_json::Value>,
    /// The handler's result before parsing. Kept because a handler returning
    /// malformed JSON is a bug worth SEEING rather than silently flattening
    /// to null.
    pub raw: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ResponseData {
    /// A memory or CPU profile snapshot, see `DebugEvent::GetProfileReport`.
    Profile(ProfileResponse),
    /// Result of an application-handled `CustomOp`.
    CustomOp(CustomOpResponse),
    /// Screenshot data (base64 encoded PNG)
    Screenshot(ScreenshotData),
    /// Node CSS properties
    NodeCssProperties(NodeCssPropertiesResponse),
    /// Node layout
    NodeLayout(NodeLayoutResponse),
    /// In-flight layout animations
    Animations(AnimationsResponse),
    /// Result of stepping layout animations
    TickAnimations(TickAnimationsResponse),
    /// All nodes layout
    AllNodesLayout(AllNodesLayoutResponse),
    /// The current DOM (nested tree + HTML), see `DebugEvent::GetDom`
    Dom(DomResponse),
    /// DOM tree
    DomTree(DomTreeResponse),
    /// Every live DOM and its addressable id, see `DebugEvent::ListDoms`
    DomList(DomListResponse),
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
    /// VirtualView states (all tracked VirtualViews and their internal state)
    VirtualViewStates(VirtualViewStatesResponse),
    /// VirtualView layout (nodes inside a specific VirtualView DOM)
    VirtualViewLayout(VirtualViewLayoutResponse),
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
    /// `focus_node` result — which node the op actually focused
    FocusNode(FocusNodeResponse),
    /// Cursor state (cursor position and blink state)
    CursorState(CursorStateResponse),
    /// E2E test results
    E2eResults(E2eResultsResponse),
    /// Per-frame damage + frame-work counters (`get_frame_report`)
    FrameReport(FrameReportResponse),
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
    /// Component preview image (CPU-rendered)
    ComponentPreview(ComponentPreviewResponse),
    /// Node dataset (RefAny serialized to JSON)
    NodeDataset(NodeDatasetResponse),
    /// Generic JSON data (for endpoints that return arbitrary JSON)
    Json(serde_json::Value),
}

/// Response for GetComponentPreview — CPU-rendered component image.
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ComponentPreviewResponse {
    /// Base64-encoded PNG data URI ("data:image/png;base64,...")
    pub data: String,
    /// Actual content width in logical pixels
    pub width: f32,
    /// Actual content height in logical pixels
    pub height: f32,
}

/// Response for GetNodeDataset — serialized RefAny dataset of a DOM node.
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeDatasetResponse {
    /// The node ID queried
    pub node_id: u64,
    /// Metadata about the RefAny type
    pub metadata: RefAnyMetadata,
    /// The serialized dataset JSON (null if not serializable)
    pub dataset: serde_json::Value,
    /// Error message if serialization failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RefAnyError>,
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

/// Response for `FocusNode`: the node DOM focus was moved to.
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct FocusNodeResponse {
    pub node_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
}

/// Response for SetNodeClasses
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeClassesSetResponse {
    pub node_id: u64,
    pub classes: Vec<String>,
    pub id: Option<String>,
}

/// One damage rect, in logical px.
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct DamageRectJson {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Response for `get_frame_report` — the whole `FrameReport`, queryable from the
/// E2E API (and from `POST /` in the inspector).
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct FrameReportResponse {
    /// Full display-list builds since the last reset — 0 across a paint-only
    /// transition tick proves the DL was PATCHED, not rebuilt.
    pub dl_rebuilds: u32,
    /// Whether the last display-list build was the per-IFC PATCH (splice +
    /// re-emit) — true after a text edit, false after a structural change.
    pub last_dl_build_patched: bool,
    pub frame_index: u64,
    /// `"none"`, `"rects"` or `"full"`.
    pub paint_damage_kind: String,
    pub paint_damage_rects: Vec<DamageRectJson>,
    pub paint_damage_area_ratio: f32,
    pub present_damage_kind: String,
    pub present_damage_rects: Vec<DamageRectJson>,
    pub present_damage_area_ratio: f32,
    /// Damage accumulated since the last `reset_frame_counters` (what the
    /// `assert_damage*` ops look at by default).
    pub accumulated_paint_damage_kind: String,
    pub accumulated_paint_damage_rects: Vec<DamageRectJson>,
    pub accumulated_present_damage_kind: String,
    pub frames_since_reset: u32,
    /// EVENT passes (`process_window_events` depth), not layout passes.
    pub relayout_iterations: u32,
    pub dom_regenerations: u32,
    /// Times layout ACTUALLY ran (`layout_and_generate_display_list`).
    pub layout_passes: u32,
    pub hit_depth_cap: bool,
    /// Terminal `ProcessEventResult` discriminant (0 = DoNothing … 6 = RegenerateDomAllWindows).
    pub terminal_result: u8,
    /// Injectable test-clock offset in ms (advanced by `tick_ms`).
    pub test_clock_offset_ms: u64,
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
    /// Named enum types defined by this library
    pub enum_models: Vec<EnumModelInfo>,
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
    /// CSS
    pub css: String,
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
    /// Flat type string (for backwards compat / display)
    pub field_type: String,
    /// Structured type descriptor
    pub field_type_structured: StructuredFieldType,
    /// Default value
    pub default: Option<String>,
    /// Whether this field is required (must be provided by the parent)
    pub required: bool,
    /// Description
    pub description: String,
}

/// Structured type descriptor for component field types.
/// Replaces flat string matching with typed JSON.
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind")]
pub enum StructuredFieldType {
    #[serde(rename = "primitive")]
    Primitive { name: String },
    #[serde(rename = "callback")]
    Callback {
        args: Vec<CallbackArgInfo>,
        return_type: String,
    },
    #[serde(rename = "ref_any")]
    RefAny { type_hint: String },
    #[serde(rename = "option")]
    OptionType { inner: Box<StructuredFieldType> },
    #[serde(rename = "vec")]
    VecType { inner: Box<StructuredFieldType> },
    #[serde(rename = "struct_ref")]
    StructRef { name: String },
    #[serde(rename = "enum_ref")]
    EnumRef { name: String },
}

/// Info about a callback argument
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct CallbackArgInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub arg_type: String,
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

/// A named enum model in a library
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct EnumModelInfo {
    /// Enum name, e.g. "UserRole"
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// Variants
    pub variants: Vec<EnumVariantInfo>,
}

/// A single enum variant
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct EnumVariantInfo {
    /// Variant name
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// Associated fields (empty for unit variants)
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
    /// Named data model types (struct definitions)
    #[serde(default)]
    pub data_models: Vec<ExportedDataModelDef>,
    /// Named enum model types
    #[serde(default)]
    pub enum_models: Vec<ExportedEnumModelDef>,
    /// Component definitions (JSON-serializable subset)
    pub components: Vec<ExportedComponentDef>,
}

/// A named data model (struct) for export/import
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExportedDataModelDef {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub fields: Vec<ExportedDataField>,
}

/// A named enum model for export/import
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExportedEnumModelDef {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub variants: Vec<ExportedEnumVariantDef>,
}

/// A single enum variant for export/import
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExportedEnumVariantDef {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub fields: Vec<ExportedDataField>,
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
    /// CSS for the component
    #[serde(default)]
    pub css: String,
}

#[cfg(feature = "std")]
fn default_param_type() -> String {
    "String".to_string()
}

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

/// One in-flight layout animation, as `get_animations` reports it.
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct AnimationNodeJson {
    pub node_id: u64,
    pub translate_x: f32,
    pub translate_y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub opacity: f32,
    pub finished: bool,
    /// Whether the value reached `css_current_transform_values` — the map the
    /// rasteriser and hit-tester read. Proves the animation is on screen and
    /// not merely bookkept.
    pub published: bool,
}

/// Response for `GetAnimations`.
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct AnimationsResponse {
    /// How many animations are in flight.
    pub active: usize,
    /// Retained departing subtrees still on screen. Must return to 0 once the
    /// exits settle — a zombie that outlives its animation is a leak, and one
    /// that vanishes early took its state with it mid-flight.
    pub zombies: usize,
    /// Diff-triggered `animation` transitions in flight. Same lifecycle law
    /// as `zombies`: must return to 0 once every override settles.
    pub transitions: usize,
    /// Cumulative retained-tree re-solves driven by zombie width/height
    /// channels. A pure slide leaves this at 0 (the frozen path); a
    /// shrinking exit re-solves per changed frame.
    pub zombie_relayouts: u64,
    /// Keyframed tracks currently driving LIVE nodes (`-azul-animation-in`
    /// and caught-mid-exit reversals).
    pub live_tracks: usize,
    pub nodes: Vec<AnimationNodeJson>,
}

/// Response for `TickAnimations`.
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct TickAnimationsResponse {
    pub dt_micros: u32,
    pub steps: u32,
    /// Animations still in flight AFTER the steps were applied.
    pub active: usize,
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

/// One live DOM, as reported by `list_doms`.
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct DomListEntry {
    /// Pass this as the envelope's `dom_id` to address this DOM.
    pub dom_id: u64,
    /// `true` for DOM 0 — the window's own document, and the default target.
    pub is_root: bool,
    pub node_count: usize,
    /// The root node's tag, so a caller can tell the documents apart at a glance.
    pub root_tag: String,
    /// The id/classes of the root node, when it carries any.
    pub root_selector: Option<String>,
    /// Set when this DOM is a VirtualView's document: the DOM + node that hosts it.
    pub virtual_view_parent: Option<DomNodeIdJson>,
    /// Laid-out size of the DOM's root, when it has one.
    pub size: Option<LogicalSizeJson>,
}

/// Response for `list_doms`.
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct DomListResponse {
    pub dom_count: usize,
    pub doms: Vec<DomListEntry>,
}

/// A `(dom, node)` pair in JSON.
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct DomNodeIdJson {
    pub dom_id: u64,
    pub node_id: u64,
}

/// Response for GetNodeHierarchy
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeHierarchyResponse {
    pub root: i64,
    pub node_count: usize,
    pub nodes: Vec<HierarchyNodeInfo>,
}

/// Response for `GetDom` — the current DOM as a NESTED tree plus its HTML.
///
/// Distinct from its two siblings: `GetDomTree` returns only counters (no
/// nodes at all) and `GetNodeHierarchy` returns a FLAT array keyed by index.
/// `GetDom` returns the DOM the way a consumer actually wants to read it —
/// nested, one call, with the serialized HTML alongside it.
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct DomResponse {
    pub dom_id: usize,
    pub node_count: usize,
    /// The same string `GetHtmlString` returns, so a caller needs one round-trip.
    pub html: String,
    /// The root node, with all children nested under it.
    pub root: DomNodeJson,
}

/// One node of the nested `GetDom` tree.
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct DomNodeJson {
    pub index: usize,
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub classes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<DomNodeJson>,
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
    /// Which component rendered this DOM node (if any).
    /// Enables the debugger to show a Component Tree alongside the DOM tree.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<ComponentOriginJson>,
    /// Whether this node has a dataset (opaque RefAny data).
    /// True if `NodeData.dataset` is `Some(...)`. The actual data is opaque
    /// but knowing it exists helps visualize component state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_dataset: Option<bool>,
}

/// JSON representation of a component origin stamp.
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ComponentOriginJson {
    /// Qualified component name, e.g. "shadcn:card"
    pub component_id: String,
    /// Data model at render time as a typed JSON value (object, number, string, etc.)
    pub data_model: serde_json::Value,
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
    pub children: Vec<usize>, // populated from layout_tree.children(idx)
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
    pub horizontal: Option<ScrollbarGeometryJson>,
    /// Vertical scrollbar info (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vertical: Option<ScrollbarGeometryJson>,
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
pub struct ScrollbarGeometryJson {
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

/// Response for GetVirtualViewStates - lists all tracked VirtualViews and their internal state
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct VirtualViewStatesResponse {
    pub virtual_view_count: usize,
    pub virtual_views: Vec<VirtualViewStateInfo>,
}

/// State info for a single VirtualView
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct VirtualViewStateInfo {
    pub parent_dom_id: usize,
    pub parent_node_id: usize,
    pub nested_dom_id: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_size_width: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_size_height: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub virtual_scroll_size_width: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub virtual_scroll_size_height: Option<f32>,
    pub was_invoked: bool,
    pub last_bounds: LogicalRectJson,
}

/// Response for GetVirtualViewLayout - layout of nodes inside a VirtualView's DOM
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct VirtualViewLayoutResponse {
    pub dom_id: usize,
    pub node_count: usize,
    pub nodes: Vec<NodeLayoutInfo>,
    /// Scroll state for the VirtualView container (from the parent DOM)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_state: Option<VirtualViewScrollStateInfo>,
    /// All DOM IDs currently in layout_results (diagnostic)
    pub available_dom_ids: Vec<usize>,
    /// Whether layout_results contains this VirtualView's DOM
    pub layout_result_found: bool,
}

/// Scroll state info specific to a VirtualView
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct VirtualViewScrollStateInfo {
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub content_width: f32,
    pub content_height: f32,
    pub container_width: f32,
    pub container_height: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub virtual_scroll_width: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub virtual_scroll_height: Option<f32>,
    pub max_scroll_x: f32,
    pub max_scroll_y: f32,
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

/// A block of text embedded in a JSON test file, written as an ARRAY OF LINES
/// so it stays readable and diffable:
///
/// ```json
/// "html": ["<div class=\"a\">", "  <p>hi</p>", "</div>"]
/// ```
///
/// A single string is also accepted (`"html": "<div/>"`). [`TextLines::join`]
/// produces the source text.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "std", serde(untagged))]
pub enum TextLines {
    /// One JSON string per source line — the readable, preferred form.
    Lines(Vec<String>),
    /// A single string containing the whole block (embedded `\n` allowed).
    Single(String),
    /// Omitted entirely.
    #[default]
    Empty,
}

impl TextLines {
    /// Join the lines back into one source string (`\n`-separated).
    #[must_use]
    pub fn join(&self) -> String {
        match self {
            Self::Lines(lines) => lines.join("\n"),
            Self::Single(s) => s.clone(),
            Self::Empty => String::new(),
        }
    }

    /// `true` when there is no text at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Lines(lines) => lines.iter().all(|l| l.trim().is_empty()),
            Self::Single(s) => s.trim().is_empty(),
            Self::Empty => true,
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "std", derive(serde::Deserialize))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum DebugEvent {
    /// Snapshot memory or CPU, so a scenario can assert a budget:
    /// `{ "op": "get_profile_report", "kind": "memory" }`.
    ///
    /// Reads the same sources as `AZ_PROFILE=memory` rather than scraping its
    /// output, so the assertion cannot drift when the printed format changes.
    GetProfileReport {
        #[serde(default = "profile_kind_memory")]
        kind: ProfileKind,
    },
    /// An op the ENGINE does not implement, handed to the application's
    /// `AppConfig::custom_e2e_op` handler. Lets a scenario drive app-level
    /// actions ("now load the document") the engine cannot express for it.
    ///
    /// If no handler is installed, or the handler does not recognise `op`,
    /// this FAILS rather than quietly succeeding — see `CustomE2eOpResult`.
    CustomOp {
        /// The application-defined op name. NOT called `op`: that key is the
        /// enum's own serde tag, so a scenario reads
        /// `{"op": "custom_op", "name": "load_document", "args": {..}}`.
        name: String,
        /// Arguments, passed through to the handler verbatim as JSON.
        #[serde(default)]
        args: serde_json::Value,
    },
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
        /// X position (optional if `selector` / `node_id` / `text` is given)
        #[serde(default)]
        x: Option<f32>,
        /// Y position (optional if `selector` / `node_id` / `text` is given)
        #[serde(default)]
        y: Option<f32>,
        /// CSS selector to find and double-click
        #[serde(default)]
        selector: Option<String>,
        /// Direct node ID to double-click
        #[serde(default)]
        node_id: Option<u64>,
        /// Text content to find and double-click
        #[serde(default)]
        text: Option<String>,
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
        /// The PRINTABLE text this key press produces, if any — the shell
        /// ingress, not the C-API one.
        ///
        /// A native shell records the text of a keystroke (`XLookupString`,
        /// `wl_keyboard` / xkb, `WM_CHAR`, `NSTextInputClient`) into the text
        /// changeset BEFORE it runs the state-diff pass, so ONE pass carries
        /// both the KeyDown and the Input event and the pass's post-callback
        /// filter decides whether the recorded edit lands. That is what makes a
        /// KeyDown handler's `prevent_default()` able to veto an insertion.
        ///
        /// `text_input` is the OTHER ingress (`CallbackChange::CreateTextInput`
        /// — the debug server / C API / IME commit), which records, dispatches
        /// `Input` and applies inside a single change. A veto raised by a
        /// KeyDown handler cannot be observed through it, because no KeyDown is
        /// dispatched in that window at all.
        #[serde(default)]
        text: Option<String>,
    },
    KeyUp {
        key: String,
        #[serde(default)]
        modifiers: Modifiers,
    },
    TextInput {
        text: String,
    },

    // Touch Events — driven through FullWindowState.touch_state; the
    // state-diff event determination fires HoverEventFilter::TouchStart /
    // TouchMove / TouchEnd / TouchCancel.
    TouchStart {
        id: u64,
        x: f32,
        y: f32,
        #[serde(default = "default_force")]
        force: f32,
    },
    TouchMove {
        id: u64,
        x: f32,
        y: f32,
        #[serde(default = "default_force")]
        force: f32,
    },
    TouchEnd {
        id: u64,
    },
    TouchCancel,

    // Pen / Stylus Events — drive GestureAndDragManager.pen_state.
    PenDown {
        x: f32,
        y: f32,
        #[serde(default = "default_force")]
        pressure: f32,
        #[serde(default)]
        x_tilt: f32,
        #[serde(default)]
        y_tilt: f32,
    },
    PenMove {
        x: f32,
        y: f32,
        #[serde(default = "default_force")]
        pressure: f32,
        #[serde(default)]
        x_tilt: f32,
        #[serde(default)]
        y_tilt: f32,
    },
    PenUp {
        x: f32,
        y: f32,
    },

    // Native Gestures — bypass the in-process detector and feed the
    // GestureAndDragManager override slot directly. The next detect_*
    // call from a callback sees the injected gesture.
    Swipe {
        #[serde(rename = "dir")]
        direction: SwipeDir,
    },
    Pinch {
        #[serde(default)]
        scale: f32,
        #[serde(default)]
        center_x: f32,
        #[serde(default)]
        center_y: f32,
        #[serde(default)]
        initial_distance: f32,
        #[serde(default)]
        current_distance: f32,
        #[serde(default)]
        duration_ms: u64,
    },
    Rotate {
        #[serde(default)]
        angle_radians: f32,
        #[serde(default)]
        center_x: f32,
        #[serde(default)]
        center_y: f32,
        #[serde(default)]
        duration_ms: u64,
    },
    LongPress {
        x: f32,
        y: f32,
        #[serde(default)]
        duration_ms: u64,
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
    /// Give the WINDOW keyboard focus (`window_focused` / `flags.has_focus`).
    /// This does NOT move DOM focus — use `focus_node` for that.
    Focus,
    /// Take WINDOW keyboard focus away. Does NOT clear DOM focus.
    Blur,
    /// Move DOM (keyboard) focus to a node.
    ///
    /// `focus` / `blur` above are WINDOW focus and never touch the focus
    /// manager. Until this op existed there was NO way to focus a node except
    /// as a side effect of a `click` that happens to land on a focusable
    /// ancestor, or of `key_down {"key": "tab"}` — and `text_input` hard-errors
    /// without a focused node, so every keyboard-editing test depended on an
    /// unstated precondition it had no way to express. Click-to-focus is not a
    /// substitute either: it needs a coordinate, and a generated test may not
    /// know or guess one.
    ///
    /// Give exactly one of `selector` / `node_id`. Refuses (loudly) if the node
    /// does not exist or cannot hold focus.
    FocusNode {
        /// CSS selector for the node to focus
        #[serde(default)]
        selector: Option<String>,
        /// Node ID to focus (alternative to `selector`)
        #[serde(default)]
        node_id: Option<u64>,
    },

    /// Perform an accessibility action on a node — what a screen reader does.
    ///
    /// This is the ONLY way any test can reach
    /// `LayoutWindow::process_accessibility_action` and the synthetic-event
    /// dispatch behind it. Until it existed, a11y was reachable exclusively
    /// from a live AT-SPI / UIA / `NSAccessibility` connection, which no test
    /// has — which is how "a screen reader activates a button and no callback
    /// runs" shipped once and stayed invisible afterwards.
    ///
    /// It is NOT a synonym for `click`: `click` moves the mouse and lets hit
    /// testing decide, whereas this addresses a node directly the way an AT
    /// does, and it exercises the action → `EventFilter` mapping table that
    /// `click` never touches.
    ///
    /// Give exactly one of `selector` / `node_id` / `text`. Refuses (loudly) if
    /// the node does not exist, if it is not exposed to assistive technology
    /// (`is_exposed_to_accessibility`), if `action` is not a known name, or if
    /// the named action needs a payload that was not supplied.
    AccessibilityAction {
        /// CSS selector for the target node
        #[serde(default)]
        selector: Option<String>,
        /// Node ID of the target (alternative to `selector`)
        #[serde(default)]
        node_id: Option<u64>,
        /// Text content of the target (alternative to `selector`)
        #[serde(default)]
        text: Option<String>,
        /// Snake-case action name, e.g. `default`, `focus`, `increment`,
        /// `scroll_down`, `set_value`. See the handler for the full list —
        /// every `AccessibilityAction` variant is nameable.
        action: String,
        /// Payload for `set_value` / `replace_selected_text`.
        #[serde(default)]
        value: Option<String>,
        /// Payload for `set_numeric_value`.
        #[serde(default)]
        number: Option<f32>,
        /// X payload for `scroll_to_point` / `set_scroll_offset`.
        #[serde(default)]
        x: Option<f32>,
        /// Y payload for `scroll_to_point` / `set_scroll_offset`.
        #[serde(default)]
        y: Option<f32>,
        /// Start payload for `set_text_selection`.
        #[serde(default)]
        selection_start: Option<u64>,
        /// End payload for `set_text_selection`.
        #[serde(default)]
        selection_end: Option<u64>,
        /// Payload for `custom_action`.
        #[serde(default)]
        custom_id: Option<i32>,
    },

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
    /// Advance layout animations by an EXACT step, bypassing the wall clock.
    ///
    /// `{ "op": "tick_animations", "dt_micros": 16666, "steps": 4 }`
    ///
    /// A headless scenario cannot sample real time — the same test would land
    /// on a different point of the curve on a fast machine than a slow one, so
    /// any mid-flight assertion would be flaky. Stepping by a fixed `dt` makes
    /// the trajectory a pure function of how many steps ran.
    TickAnimations {
        /// Microseconds per step. Defaults to one 60 Hz frame (16_666).
        #[serde(default)]
        dt_micros: Option<u32>,
        /// Steps to take, so an animation can be run to completion in one op.
        #[serde(default)]
        steps: Option<u32>,
    },
    /// Report in-flight layout animations and the transform each contributes.
    ///
    /// `{ "op": "get_animations" }`
    GetAnimations,
    /// Get all nodes with their layout info
    GetAllNodesLayout,
    /// Get detailed DOM tree structure
    GetDomTree,
    /// List every live DOM with the id to address it by.
    ///
    /// `{ "op": "list_doms" }`
    ///
    /// The window's own document is DOM 0; a VirtualView's document and a
    /// `<transient-window>`'s popup content are DOMs of their own. Every
    /// node-addressing op defaults to DOM 0 and takes the envelope's
    /// `dom_id` to reach the others — this op is how you learn the ids
    /// instead of guessing pixel coordinates.
    ListDoms,
    /// Get the raw node hierarchy (for debugging DOM structure issues).
    /// Address a child DOM (a VirtualView / transient-window document) with
    /// the envelope's `dom_id`, like every other node-addressing op.
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

    // VirtualView inspection
    /// Get all tracked VirtualView states (scroll sizes, virtual sizes, invocation status)
    GetVirtualViewStates,
    /// Get layout of nodes inside a specific VirtualView's DOM (by nested dom_id or parent node_id)
    GetVirtualViewLayout {
        /// The nested DOM ID of the VirtualView (from get_virtual_view_states result)
        #[serde(default)]
        dom_id: Option<usize>,
        /// The parent node_id of the VirtualView node (in the root DOM)
        #[serde(default)]
        node_id: Option<usize>,
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

    /// `{ "op": "print", "text": "..." }` - write a line to the run's output.
    ///
    /// Scenario-level `printf`. Without it the only way to see what an op
    /// actually returned was to assert something false and read the value out
    /// of the failure message, which is a poor way to explore a live app.
    Print {
        text: String,
    },
    /// `{ "op": "print_response" }` - dump the PREVIOUS step's response.
    ///
    /// Pairs with the query ops (`get_focus_state`, `get_cursor_state`,
    /// `get_dom`, ...): run the query, then print what it said.
    PrintResponse,

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
    /// Get the dataset (RefAny) of a specific DOM node as JSON
    GetNodeDataset {
        /// Node ID whose dataset to retrieve
        node_id: u64,
    },

    // Focus and Cursor State
    /// Get the current focus state (which node has keyboard focus)
    GetFocusState,
    /// Get the current cursor state (position, blink state)
    GetCursorState,

    // E2E Mount / Observability
    /// Mount an inline HTML/XML + CSS document as the window's DOM, replacing
    /// the app's `layout_callback` output for the rest of the process.
    ///
    /// `html` and `css` are given as an ARRAY OF LINES (one JSON string per
    /// source line) so that the markup stays human-readable and editable inside
    /// the test file; a single string is also accepted. The lines are joined
    /// with `\n` on load, the CSS is injected as a `<style>` element and the
    /// document is parsed with azul's own XML→StyledDom parser
    /// (`azul_layout::xml::parse_xml_to_styled_dom`).
    ///
    /// ```json
    /// { "op": "mount",
    ///   "html": ["<div class=\"a\">", "  <p>hi</p>", "</div>"],
    ///   "css":  [".a { width: 100px; }"] }
    /// ```
    Mount {
        /// The markup, one array element per line (or a single string).
        html: TextLines,
        /// The stylesheet, one array element per line (or a single string).
        #[serde(default)]
        css: TextLines,
    },
    /// Remove the `mount` override: the next DOM regeneration goes back to the
    /// app's own `layout_callback`.
    Unmount,
    /// Render the current frame with the FULL-repaint path and store the PNG
    /// under `as` for later `assert_changed` / `assert_damage_covers_changes`.
    SnapshotFrame {
        #[serde(rename = "as")]
        name: String,
    },
    /// Store the current font/image/resource counters under `as`, for a later
    /// `assert_resource_counts`.
    SnapshotResources {
        #[serde(rename = "as")]
        name: String,
    },
    /// Store EVERY manager's observable state under `as`, for a later
    /// `assert_only_managers_changed` — the non-interference primitive.
    ///
    /// `snapshot_resources` answers "did this op leak a font?". This answers the
    /// question one level up: "did this op move a manager it has no business
    /// touching?". Tab-focus must not move the scroll manager; a scroll must not
    /// move the selection; replacing a subtree must not resurrect a drag. None of
    /// those leaks is visible in the DOM, in the pixels or in the resource
    /// counters — the only way to see one is to record every manager BEFORE the
    /// op and diff afterwards.
    ///
    /// ```json
    /// { "op": "snapshot_managers", "as": "before_tab" },
    /// { "op": "key_down", "key": "tab" },
    /// { "op": "assert_only_managers_changed", "vs": "before_tab",
    ///   "changed": ["focus"] }
    /// ```
    SnapshotManagers {
        #[serde(rename = "as")]
        name: String,
    },
    /// Advance the injectable engine clock by `ms` and render a frame — WITHOUT
    /// sleeping. Everything time-driven (scroll momentum, scrollbar fade, cursor
    /// blink, animations, timers) reads `Instant::now()`, which honours this
    /// offset, so an animation can be driven to completion deterministically and
    /// then asserted to CONVERGE (`assert_idle_stable` → damage drains to none).
    TickMs {
        ms: u64,
    },
    /// Register a repeating timer that rewrites one text node on every expiry.
    ///
    /// This is the E2E drive surface for `CallbackInfo::add_timer`, i.e. for
    /// `CallbackChange::AddTimer`. That change carries a Rust `TimerCallback`
    /// function pointer, which no JSON scenario can supply, so without this op
    /// the whole app-facing timer API is implemented and unreachable: the only
    /// timer any scenario could ever arm was the caret blink, and the generic
    /// add/remove arms in both hosts' `apply_user_change` were dead to the
    /// suite. The op supplies the one thing JSON cannot — the callback — and
    /// pushes it through the SAME `CallbackInfo::add_timer` a real app calls.
    ///
    /// The effect is deliberately OBSERVABLE rather than a bare registry
    /// mutation: each expiry writes `"<text> <run_count>"` into node `node_id`,
    /// so a fired timer shows up as real pixel damage that `assert_changed`
    /// can see, and a timer that was removed shows up as `FrameDamage::None`.
    /// A `remove_timer` that silently did nothing would otherwise be
    /// indistinguishable from success.
    AddTimer {
        /// User timer id. Must be at or above `USER_TIMER_ID_START` (0x0100);
        /// the block below it is reserved for the engine's own timers
        /// (caret blink, scroll momentum, tooltip delay, long press …) and
        /// colliding with one would corrupt engine state rather than test it.
        timer_id: u64,
        /// Repeat interval in milliseconds; must be non-zero. The timer has no
        /// delay, so — exactly like the caret blink — it also runs once on the
        /// first pump after registration.
        interval_ms: u64,
        /// The node whose text is rewritten. Same addressing as `set_node_text`:
        /// this is the `NodeType::Text` node itself, not its element parent.
        node_id: u64,
        /// Text written on expiry. The run count is appended, so consecutive
        /// expiries never write the byte-identical string that
        /// `CallbackChange::ChangeNodeText` correctly short-circuits.
        text: String,
    },
    /// Deregister the timer registered under `timer_id` — the E2E drive surface
    /// for `CallbackInfo::remove_timer` / `CallbackChange::RemoveTimer`. See
    /// [`DebugEvent::AddTimer`].
    RemoveTimer {
        /// The id passed to `add_timer`. Refuses reserved system ids for the
        /// same reason `add_timer` does.
        timer_id: u64,
    },
    /// Return the current `FrameReport` (damage rects + work counters) as JSON —
    /// the same data the `assert_damage*` / `assert_work_bounded` ops read.
    GetFrameReport,
    /// Write the PARTIAL screen update as a PNG: the full frame, masked to the
    /// damage region (everything outside the damaged rects is transparent), plus
    /// optionally cropped to the damage bounding box. This is how a human (or a
    /// Tier-2 judge) can LOOK at what was actually repainted and see that an
    /// incremental repaint is real rather than a full redraw in disguise.
    CaptureDamagePng {
        /// Where to write the PNG.
        path: String,
        /// `"paint"` (default) or `"present"` damage.
        #[serde(default)]
        which: Option<String>,
        /// Crop the output to the damage bounding box instead of keeping the
        /// full window size with transparent surroundings. Default: false.
        #[serde(default)]
        crop: bool,
    },
    /// Zero the sticky frame-work counters (`relayout_iterations`,
    /// `dom_regenerations`, `hit_depth_cap`) on the `LayoutWindow`'s
    /// `FrameReport`. Everything `assert_work_bounded` measures is "since the
    /// last reset".
    ResetFrameCounters,

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
    /// Swap an image node's content to a synthesized solid-color raster
    /// (travels the REAL path: `CallbackChange::ChangeNodeImage` → the
    /// content chokepoint `LayoutWindow::apply_content_change`). Target must
    /// be a `NodeType::Image` node (e.g. a mounted `<img>`), matching the
    /// validate-loudly discipline of `add_timer`.
    SetNodeImage {
        /// Node ID of the image node to swap
        node_id: u64,
        /// Pixel width of the synthesized image
        width: u32,
        /// Pixel height of the synthesized image
        height: u32,
        /// CSS color name/hex for the solid fill (e.g. "red", "#00ff00")
        color: String,
    },
    /// Register a synthesized solid-color image under a CSS id
    /// (`background-image: url("<css_id>")`) — travels
    /// `CallbackChange::AddImageToCache` → the content chokepoint, whose
    /// returned dirty tier makes the registration visible NOW.
    AddImageToCache {
        /// The CSS id to register the image under
        css_id: String,
        /// Pixel width of the synthesized image
        width: u32,
        /// Pixel height of the synthesized image
        height: u32,
        /// CSS color name/hex for the solid fill
        color: String,
    },
    /// Remove a CSS-id image registration (the inverse of `add_image_to_cache`).
    RemoveImageFromCache {
        /// The CSS id to remove
        css_id: String,
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
    /// Export code as a downloadable ZIP file (base64-encoded).
    /// Contains: generated source files, component CSS, build configuration.
    ExportCodeZip {
        /// Target language: "rust", "c", "cpp", "python"
        language: String,
        /// Optional library to export (if omitted, exports all)
        #[serde(default)]
        library: Option<String>,
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
    /// Update a component's properties (CSS, data model, etc.)
    UpdateComponent {
        /// Library name
        library: String,
        /// Component tag name
        name: String,
        /// New CSS (if provided)
        #[serde(default)]
        css: Option<String>,
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
    /// Render a component to a PNG image via CPU renderer.
    /// Uses the existing window's fonts — no expensive font rebuild.
    GetComponentPreview {
        /// Library name, e.g. "builtin", "mylib"
        library: String,
        /// Component tag name, e.g. "button", "card"
        name: String,
        /// Viewport width (logical px). None = size to content.
        #[serde(default)]
        width: Option<f32>,
        /// Viewport height (logical px). None = size to content.
        #[serde(default)]
        height: Option<f32>,
        /// DPI factor. None = 1.0.
        #[serde(default)]
        dpi: Option<f32>,
        /// Background color as "#RRGGBB" or "#RRGGBBAA". None = white.
        #[serde(default)]
        background: Option<String>,
        /// Optional CSS to apply (overrides component css).
        #[serde(default)]
        css_override: Option<String>,
        /// Component arguments as typed JSON values, e.g. {"label": "Click", "disabled": true, "count": 42}.
        /// Keys must match the component's data_model field names.
        /// Values are validated against the field's ComponentFieldType.
        /// Missing fields use their default_value from the data model.
        #[serde(default)]
        args: Option<std::collections::HashMap<String, serde_json::Value>>,
        /// Override OS for @os() CSS at-rules: "windows", "mac", "linux"
        #[serde(default)]
        override_os: Option<String>,
        /// Override theme for @theme() CSS: "light", "dark"
        #[serde(default)]
        override_theme: Option<String>,
        /// Override language/locale: e.g. "en", "de", "fr"
        #[serde(default)]
        override_lang: Option<String>,
    },
    /// Get the render output of a component as a structured tree (for the mini HTML tree widget).
    GetComponentRenderTree {
        /// Library name
        library: String,
        /// Component tag name
        name: String,
    },
    /// Get the source code of a component's render_fn or compile_fn.
    GetComponentSource {
        /// Library name
        library: String,
        /// Component tag name
        name: String,
        /// "render_fn" or "compile_fn"
        source_type: String,
        /// Target language for compile_fn (ignored for render_fn). E.g. "rust", "c", "cpp", "python".
        #[serde(default)]
        language: Option<String>,
    },
    /// Update a component's render_fn source code.
    UpdateComponentRenderFn {
        /// Library name
        library: String,
        /// Component tag name
        name: String,
        /// New source code for the render_fn
        source: String,
    },
    /// Update a component's compile_fn source code for a specific language.
    UpdateComponentCompileFn {
        /// Library name
        library: String,
        /// Component tag name
        name: String,
        /// New source code for the compile_fn
        source: String,
        /// Target language: "rust", "c", "cpp", "python"
        language: String,
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

// ==================== Accessibility Action Parsing ====================

/// Parse the `accessibility_action` op's `action` name (+ its payload fields)
/// into a real [`azul_core::dom::AccessibilityAction`].
///
/// EVERY variant of the enum is nameable — the match below is exhaustive over
/// the accepted names and the list is kept in sync with `core/src/a11y.rs` by
/// hand, so a new action that nobody wires here is a name the op refuses rather
/// than one it silently mistranslates.
///
/// An unknown name is an ERROR, not a fallback to `Default`. Substituting
/// `Default` would let a scenario report that it tested `increment` while it
/// had in fact performed a click — the same class of lie as a no-op stub.
/// Likewise a payload-carrying action with no payload is an error: performing
/// `set_value` with an empty string is a different test from the one that was
/// written.
#[cfg(feature = "std")]
#[allow(clippy::too_many_arguments)]
fn parse_accessibility_action(
    name: &str,
    value: Option<&str>,
    number: Option<f32>,
    x: Option<f32>,
    y: Option<f32>,
    selection_start: Option<u64>,
    selection_end: Option<u64>,
    custom_id: Option<i32>,
) -> Result<azul_core::dom::AccessibilityAction, String> {
    use azul_core::dom::{AccessibilityAction as A, TextSelectionStartEnd};
    use azul_core::geom::LogicalPosition;

    /// Every name this op accepts, in the error message so a typo is one read
    /// away from fixed.
    const KNOWN: &str = "default, focus, blur, collapse, expand, scroll_into_view, increment, \
                         decrement, show_context_menu, hide_tooltip, show_tooltip, scroll_up, \
                         scroll_down, scroll_left, scroll_right, \
                         set_sequential_focus_navigation_starting_point, replace_selected_text, \
                         scroll_to_point, set_scroll_offset, set_text_selection, set_value, \
                         set_numeric_value, custom_action";

    let need_value = |what: &str| -> Result<azul_css::AzString, String> {
        value
            .map(azul_css::AzString::from)
            .ok_or_else(|| format!("action '{what}' needs a \"value\" string, and none was given"))
    };
    let need_point = |what: &str| -> Result<LogicalPosition, String> {
        match (x, y) {
            (Some(x), Some(y)) => Ok(LogicalPosition { x, y }),
            _ => Err(format!(
                "action '{what}' needs both \"x\" and \"y\", and at least one was missing"
            )),
        }
    };

    Ok(match name {
        "default" => A::Default,
        "focus" => A::Focus,
        "blur" => A::Blur,
        "collapse" => A::Collapse,
        "expand" => A::Expand,
        "scroll_into_view" => A::ScrollIntoView,
        "increment" => A::Increment,
        "decrement" => A::Decrement,
        "show_context_menu" => A::ShowContextMenu,
        "hide_tooltip" => A::HideTooltip,
        "show_tooltip" => A::ShowTooltip,
        "scroll_up" => A::ScrollUp,
        "scroll_down" => A::ScrollDown,
        "scroll_left" => A::ScrollLeft,
        "scroll_right" => A::ScrollRight,
        "set_sequential_focus_navigation_starting_point" => {
            A::SetSequentialFocusNavigationStartingPoint
        }
        "replace_selected_text" => A::ReplaceSelectedText(need_value("replace_selected_text")?),
        "set_value" => A::SetValue(need_value("set_value")?),
        "scroll_to_point" => A::ScrollToPoint(need_point("scroll_to_point")?),
        "set_scroll_offset" => A::SetScrollOffset(need_point("set_scroll_offset")?),
        "set_text_selection" => match (selection_start, selection_end) {
            (Some(s), Some(e)) => A::SetTextSelection(TextSelectionStartEnd {
                selection_start: s as usize,
                selection_end: e as usize,
            }),
            _ => {
                return Err(
                    "action 'set_text_selection' needs both \"selection_start\" and \
                            \"selection_end\", and at least one was missing"
                        .to_string(),
                )
            }
        },
        "set_numeric_value" => match number {
            Some(n) => A::SetNumericValue(azul_css::props::basic::length::FloatValue::new(n)),
            None => {
                return Err(
                    "action 'set_numeric_value' needs a \"number\", and none was given".to_string(),
                )
            }
        },
        "custom_action" => match custom_id {
            Some(id) => A::CustomAction(id),
            None => {
                return Err(
                    "action 'custom_action' needs a \"custom_id\" int, and none was given"
                        .to_string(),
                )
            }
        },
        other => return Err(format!("unknown action '{other}'. Known actions: {KNOWN}")),
    })
}

// ==================== Node Resolution Helper ====================

/// Resolves a node target (selector, node_id, or text) to a NodeId.
/// Returns the first matching node or None if no match found.
#[cfg(feature = "std")]
fn resolve_node_target(
    callback_info: &azul_layout::callbacks::CallbackInfo,
    dom_id: azul_core::dom::DomId,
    selector: Option<&str>,
    node_id: Option<u64>,
    text: Option<&str>,
) -> Option<azul_core::id::NodeId> {
    use azul_core::dom::DomId;
    use azul_core::id::NodeId;

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
    dom_id: azul_core::dom::DomId,
    selector: &str,
) -> Vec<azul_core::id::NodeId> {
    use azul_core::dom::DomId;
    use azul_core::id::NodeId;
    use azul_core::style::matches_html_element;
    use azul_css::parser2::parse_css_path;
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
    for attr in node_data.attributes().as_ref().iter() {
        if let Some(id) = attr.as_id() {
            selector.push('#');
            selector.push_str(id);
            break; // Only one ID
        }
    }

    // Add all classes
    for attr in node_data.attributes().as_ref().iter() {
        if let Some(class) = attr.as_class() {
            selector.push('.');
            selector.push_str(class);
        }
    }

    // If no ID or classes, add node index to make it unique
    let has_id_or_class = node_data
        .attributes()
        .as_ref()
        .iter()
        .any(|a| a.as_id().is_some() || a.as_class().is_some());
    if !has_id_or_class {
        selector.push_str(&alloc::format!(":nth-child({})", node_id.index() + 1));
    }

    Some(selector)
}

/// Resolves a node target to center position (x, y) for clicking
#[cfg(feature = "std")]
fn resolve_node_center(
    callback_info: &azul_layout::callbacks::CallbackInfo,
    dom_id: azul_core::dom::DomId,
    selector: Option<&str>,
    node_id: Option<u64>,
    text: Option<&str>,
) -> Option<(f32, f32)> {
    use azul_core::dom::{DomId, DomNodeId};
    use azul_core::id::NodeId;

    if let Some(nid) = resolve_node_target(callback_info, dom_id, selector, node_id, text) {
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

/// Default touch / pen force when the JSON test doesn't supply one. 0.5
/// is the same "pressure unavailable" sentinel `TouchPoint` documents.
fn default_force() -> f32 {
    0.5
}

/// Swipe direction accepted by the `swipe` debug event.
#[derive(Debug, Clone, Copy, Default)]
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[serde(rename_all = "lowercase")]
pub enum SwipeDir {
    #[default]
    Up,
    Down,
    Left,
    Right,
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

/// File handle for AZ_RECORD file-based logging.
/// When `AZ_RECORD=<filepath>` is set, all log messages are written to this file.
#[cfg(feature = "std")]
static RECORD_FILE: OnceLock<Mutex<std::fs::File>> = OnceLock::new();

/// Whether E2E test runner mode is active (independent of debug server).
#[cfg(feature = "std")]
static E2E_ACTIVE: AtomicBool = AtomicBool::new(false);

#[cfg(feature = "std")]
static DEBUG_PORT: OnceLock<u16> = OnceLock::new();

/// Global debug server handle (singleton — one per application).
/// Started in `AppInternal::create()` when `AZ_DEBUG=<port>` is set.
#[cfg(feature = "std")]
#[cfg(feature = "e2e-server")]
static DEBUG_SERVER: OnceLock<Arc<DebugServerHandle>> = OnceLock::new();

/// Per-window E2E scheduler slot: the half-finished scenario run that has to
/// survive between event-loop ticks.
///
/// The scenario runner YIELDS whenever a step needs a relayout
/// (`resume_e2e_continuation`) and resumes on the next tick. That state used to
/// live in a process-global `static Mutex<Option<E2eContinuation>>`, which had
/// two consequences: two windows shared ONE slot and clobbered each other, and
/// the headless runner needed a process-wide `RUN_LOCK` to serialize whole runs
/// because of it.
///
/// It now lives with the thing that ALREADY survives ticks — the `RefAny` the
/// debug timer owns (`DebugTimerData`) in the DLL path, and a local in
/// `crate::e2e::runner::run_e2e_test` in the headless path. One session per
/// window, no ambient state, and `f(UI) -> DOM -> Screen` stays a function of
/// its arguments.
#[cfg(feature = "std")]
#[derive(Default)]
pub struct E2eSession {
    /// The suspended run, if any. `None` between runs and while a run is
    /// executing (the continuation is owned by `resume_e2e_continuation`).
    pending: Option<E2eContinuation>,
    /// `true` while `resume_e2e_continuation` is on the stack. Guards against a
    /// scenario whose step is itself `run_e2e_tests`: with a single slot per
    /// window, a nested run would silently overwrite the outer one's progress.
    running: bool,
}

#[cfg(feature = "std")]
impl core::fmt::Debug for E2eSession {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("E2eSession")
            .field("pending", &self.pending.is_some())
            .field("running", &self.running)
            .finish()
    }
}

#[cfg(feature = "std")]
impl E2eSession {
    /// An idle session.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pending: None,
            running: false,
        }
    }

    /// Whether a run is suspended in this slot.
    #[must_use]
    pub const fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// The `wait` deadline of the suspended run, if it set one.
    #[must_use]
    pub fn resume_not_before(&self) -> Option<std::time::Instant> {
        self.pending.as_ref().and_then(|c| c.resume_not_before)
    }

    /// Drop any suspended run. Used by the headless runner so a previously
    /// panicked scenario cannot leak into the next one.
    pub fn clear(&mut self) {
        self.pending = None;
        self.running = false;
    }
}

/// Named full-repaint PNG snapshots taken by the `snapshot_frame` op.
///
/// `CallbackInfo::take_screenshot` re-renders the display list from scratch with
/// a fresh glyph cache — i.e. it is an INDEPENDENT full-repaint oracle, not the
/// incremental buffer. That is exactly what the damage assertions need.
/// Per-window scratch state for the E2E ops that have to remember something
/// BETWEEN the steps of one scenario.
///
/// These were four process-global `static Mutex<…>`: the named frame snapshots
/// (`snapshot_frame`), the named resource-counter snapshots
/// (`snapshot_resources`), the composition stage trace (`assert_composition`)
/// and the last presented framebuffer (`assert_damage_sound`'s pixel-identity
/// check). Process-global was already wrong for two windows; it is FATAL for a
/// parallel run, where two scenarios that use the same snapshot name would
/// silently read each other's pixels and report a confident wrong verdict.
///
/// It hangs off [`azul_layout::window::LayoutWindow`] because an assertion only
/// ever holds `&CallbackInfo`, hence `&LayoutWindow`. The `Mutex` is therefore
/// per-instance interior mutability — one uncontended lock per window — not
/// ambient state.
#[cfg(feature = "std")]
#[derive(Debug, Default)]
pub struct E2eScratch {
    /// Named full-repaint PNG snapshots taken by the `snapshot_frame` op.
    ///
    /// `CallbackInfo::take_screenshot` re-renders the display list from scratch
    /// with a fresh glyph cache — i.e. it is an INDEPENDENT full-repaint oracle,
    /// not the incremental buffer. That is exactly what the damage assertions
    /// need.
    frame_snapshots: BTreeMap<String, Vec<u8>>,
    /// Named resource-counter snapshots taken by the `snapshot_resources` op.
    resource_snapshots: BTreeMap<String, BTreeMap<String, u64>>,
    /// Named per-manager state snapshots taken by the `snapshot_managers` op and
    /// diffed by `assert_only_managers_changed` — the NON-INTERFERENCE primitive.
    /// See [`manager_fingerprints`].
    manager_snapshots: BTreeMap<String, BTreeMap<String, ManagerFingerprint>>,
    /// The node the most recent `scroll_into_view` op asked for.
    ///
    /// `scroll_into_view` is stateless — it computes `ScrollAdjustment`s, writes
    /// them into `ScrollManager` and forgets both the adjustments AND which node
    /// it was asked about. Cross-invariant X1 ("after a `scroll_into_view` the
    /// target must be inside the container's visible rect according to
    /// `ScrollManager`'s own offset") therefore has no subject to check unless
    /// the op records one. This is that record — the E2E harness's own note of
    /// what IT asked for, not a guess about engine internals. X1 hard-fails when
    /// it is `None`, so the invariant can never pass because nothing scrolled.
    last_scroll_into_view: Option<(azul_core::dom::DomId, azul_core::dom::NodeId)>,
    /// The damage-driven framebuffer of the frame just rendered `(w, h, rgba)`.
    #[cfg(feature = "cpurender")]
    presented_frame: Option<(u32, u32, Vec<u8>)>,
    /// Per-step composition history for cross-invariant X8.
    ///
    /// Reset by `e2e_reset_composition_trace` at each step boundary and appended
    /// to as composition runs, so X8 can compare this step against the previous
    /// one (`prev`/`prev2`) instead of against ambient state. `None` means no
    /// step has begun yet, which is why X8 hard-fails rather than passing when
    /// it finds none — an invariant with no subject must not report success.
    composition_trace: Option<CompositionTrace>,
}

/// Lock this window's E2E scratch. A poisoned lock is recovered rather than
/// panicked on: a scenario that panicked mid-step must not take the whole
/// (possibly parallel) run down with it.
#[cfg(feature = "std")]
fn scratch<'a>(
    callback_info: &'a azul_layout::callbacks::CallbackInfo,
) -> std::sync::MutexGuard<'a, E2eScratch> {
    callback_info
        .get_layout_window()
        .e2e_scratch
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// This window's frame report AS AN ASSERTION MUST READ IT — with a pending
/// `reset_frame_counters` already applied.
///
/// The reset is recorded as a generation bump (the op only holds
/// `&LayoutWindow`) and is folded into the stored report by the next writer.
/// Reading the stored report directly therefore reports the counters and the
/// accumulated damage of the PREVIOUS checkpoint until a frame happens to be
/// rendered — which is how `assert_work_bounded` came to report the window
/// resize from the scenario's `setup` block as if it were work done by the step
/// under test.
#[cfg(feature = "std")]
fn frame_report_of(
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> azul_layout::window::FrameReport {
    callback_info.get_layout_window().frame_report_synced()
}

/// Ask the shell for ONE more frame WITHOUT fabricating any work.
///
/// `wait_frame` and `tick_ms` are the ops that mean "let the engine produce the
/// frame that follows from what just happened". They cannot use the step
/// loop's `needs_update` flag for that: `needs_update` is turned into
/// `Update::RefreshDom` (a full `layout()` + DOM regeneration) by the debug
/// timer, so every clock tick would charge the window a DOM regeneration that
/// the application never asked for — which makes "did this idle frame do any
/// work?" unanswerable, because the harness itself is the work.
///
/// Re-pushing the CURRENT window state is the engine's existing "repaint, no
/// event pass, no DOM regeneration" signal. `apply_user_change`'s
/// `ModifyWindowState` arm gates the state-diff pass on `anything_changed`
/// (dll/src/desktop/shell2/common/event.rs) and otherwise returns
/// `ShouldReRenderCurrentWindow`, which `process_timers_and_threads` turns
/// into a redraw with `needs_layout_regeneration = false`. It also makes
/// `CallbackInfo::has_pending_relayout_change()` true, so the step loop YIELDS
/// and the frame actually lands before the next step reads the frame report.
#[cfg(feature = "std")]
fn request_repaint(callback_info: &mut azul_layout::callbacks::CallbackInfo) {
    let state = callback_info.get_current_window_state().clone();
    callback_info.modify_window_state(state);
}

/// Build the XML document for a `mount` op out of the (line-array) html + css.
///
/// The CSS is injected as a `<style>` element inside a `<head>` — the XML parser
/// collects `<style>` text into the cascade and drops `<head>` from the DOM.
#[cfg(feature = "std")]
fn build_mount_document(html: &TextLines, css: &TextLines) -> String {
    let html_src = html.join();
    let css_src = css.join();
    let trimmed = html_src.trim_start();
    let has_body = trimmed.starts_with("<body") || trimmed.starts_with("<html");
    let body = if has_body {
        html_src.clone()
    } else {
        format!("<body>\n{html_src}\n</body>")
    };
    format!("<html>\n<head>\n<style>\n{css_src}\n</style>\n</head>\n{body}\n</html>")
}

/// Snapshot the (already `pub`) resource + font-manager counters that a leak
/// assertion needs. Reachable today from `CallbackInfo::get_layout_window()`.
#[cfg(feature = "std")]
fn collect_resource_counts(
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> BTreeMap<String, u64> {
    let lw = callback_info.get_layout_window();
    let rr = &lw.renderer_resources;
    let fm = &lw.font_manager;
    let mut out = BTreeMap::new();
    out.insert(
        "fonts".to_string(),
        rr.currently_registered_fonts.len() as u64,
    );
    out.insert("font_hash_map".to_string(), rr.font_hash_map.len() as u64);
    out.insert("font_id_map".to_string(), rr.font_id_map.len() as u64);
    out.insert(
        "font_families_map".to_string(),
        rr.font_families_map.len() as u64,
    );
    out.insert(
        "images".to_string(),
        rr.currently_registered_images.len() as u64,
    );
    out.insert("image_key_map".to_string(), rr.image_key_map.len() as u64);
    out.insert(
        "parsed_fonts".to_string(),
        fm.parsed_fonts.lock().map(|p| p.len() as u64).unwrap_or(0),
    );
    out.insert(
        "font_hash_to_families".to_string(),
        fm.font_hash_to_families.len() as u64,
    );
    out.insert(
        "font_chain_cache".to_string(),
        fm.font_chain_cache.len() as u64,
    );
    out
}

/// Saved state for resuming E2E test execution across timer ticks.
#[cfg(feature = "std")]
struct E2eContinuation {
    /// Response channel from the original request
    response_tx: mpsc::Sender<DebugResponseData>,
    /// Window ID from the original request
    window_id: Option<String>,
    /// All tests
    tests: Vec<E2eTest>,
    /// Named snapshots
    snapshots: Option<std::collections::HashMap<String, serde_json::Value>>,
    /// Current test index
    test_idx: usize,
    /// Current step index within the current test
    step_idx: usize,
    /// Accumulated results for completed tests
    completed_results: Vec<E2eTestResult>,
    /// Accumulated step results for the current test
    current_step_results: Vec<E2eStepResult>,
    /// Whether the current test has failed
    current_test_failed: bool,
    /// Start time of the current test
    test_start: std::time::Instant,
    /// Component map reference
    component_map: Arc<Mutex<azul_core::xml::ComponentMap>>,
    /// App data clone
    app_data: azul_core::refany::RefAny,
    /// App-state undo/redo history (mini-git) for the `commit_undo_snapshot` /
    /// `undo_app_state` / `redo_app_state` E2E step ops — exercises the same
    /// `RefAnyUndoManager` the app-level wiring uses, from outside via E2E JSON.
    undo_manager: azul_layout::json::RefAnyUndoManager,
    /// Do not resume before this instant. Set by the `wait` step: sleeping
    /// inline on the event-loop thread would BLOCK the queued synthetic-input
    /// states and the relayout the wait exists to wait FOR — so `wait` yields
    /// with a deadline instead, and frames keep processing meanwhile.
    resume_not_before: Option<std::time::Instant>,
    /// Whether the current test's `setup` block (window size / DPI / app state)
    /// has already been applied. The setup is applied ONCE, before step 0, and
    /// the runner then yields so the resize actually reaches the window before
    /// the first step runs. Without this the whole `setup` block was parsed and
    /// then silently ignored — every test rendered at the default window size.
    setup_applied: bool,
}

// ==================== Debug Server Handle ====================

/// Handle to the debug server for clean shutdown
#[cfg(feature = "std")]
#[cfg(feature = "e2e-server")]
pub struct DebugServerHandle {
    pub shutdown_tx: mpsc::Sender<()>,
    pub thread_handle: Mutex<Option<std::thread::JoinHandle<()>>>,
    pub port: u16,
    /// The sender side of the spmc channel.
    /// HTTP thread and `queue_e2e_tests` use this to push `DebugRequest`s.
    pub request_tx: Arc<Mutex<spmc::Sender<DebugRequest>>>,
}

#[cfg(feature = "std")]
#[cfg(feature = "e2e-server-http")]
impl std::fmt::Debug for DebugServerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DebugServerHandle")
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "std")]
#[cfg(feature = "e2e-server-http")]
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
#[cfg(feature = "e2e-server-http")]
impl Drop for DebugServerHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// ==================== Public API ====================

/// Get a clone of the global `DebugServerHandle` `Arc`.
///
/// Returns `None` when `AZ_DEBUG` was not set or the server
/// hasn't been started yet.
#[cfg(feature = "std")]
#[cfg(feature = "e2e-server-http")]
pub fn get_debug_server() -> Option<Arc<DebugServerHandle>> {
    DEBUG_SERVER.get().cloned()
}

/// Check if the debug timer should be registered.
///
/// Returns `true` when either `AZ_DEBUG=<port>` started the HTTP
/// server **or** `AZ_E2E_TEST` queued tests.
#[cfg(feature = "std")]
pub fn is_debug_enabled() -> bool {
    DEBUG_ENABLED.load(Ordering::SeqCst) || E2E_ACTIVE.load(Ordering::SeqCst)
}

/// Whether the `log_*!` macros should fire. In the full (debug-server) build
/// this tracks `is_debug_enabled()` exactly — messages flow into the queue for
/// the HTTP server / `AZ_RECORD` exactly as before. (The lean stub overrides
/// this to follow `AZ_LOG` and forward to the `log` facade instead.)
#[cfg(feature = "std")]
pub fn log_active() -> bool {
    is_debug_enabled()
}

/// Initialize file-based recording from `AZ_RECORD` environment variable.
///
/// When `AZ_RECORD=<filepath>` is set, all log messages are written to the
/// specified file in addition to the normal debug server log queue. This also
/// enables debug logging so all `log_trace!` / `log_debug!` / etc. macros fire.
///
/// Called once from `App::create()` before any other logging.
#[cfg(feature = "std")]
pub fn init_recording() {
    if let Ok(path) = std::env::var("AZ_RECORD") {
        if let Ok(file) = std::fs::File::create(&path) {
            let _ = RECORD_FILE.set(Mutex::new(file));
            DEBUG_ENABLED.store(true, Ordering::SeqCst);
            SERVER_START_TIME.get_or_init(std::time::Instant::now);
            LOG_QUEUE.get_or_init(|| Mutex::new(Vec::new()));
        }
    }
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
#[cfg(feature = "e2e-server")]
pub fn queue_e2e_tests(tests: Vec<E2eTest>) -> std::sync::mpsc::Receiver<DebugResponseData> {
    E2E_ACTIVE.store(true, Ordering::SeqCst);

    let test_count = tests.len();
    let total_steps: usize = tests.iter().map(|t| t.steps.len()).sum();
    log(
        LogLevel::Info,
        LogCategory::DebugServer,
        format!(
            "[E2E] Queuing {} test(s) with {} total step(s)",
            test_count, total_steps
        ),
        None,
    );
    for (i, test) in tests.iter().enumerate() {
        log(
            LogLevel::Debug,
            LogCategory::DebugServer,
            format!(
                "[E2E]   test[{}]: '{}' ({} steps)",
                i,
                test.name,
                test.steps.len()
            ),
            None,
        );
    }

    let (tx, rx) = mpsc::channel();
    let request_id = NEXT_REQUEST_ID.fetch_add(1, Ordering::SeqCst);

    let request = DebugRequest {
        request_id,
        event: DebugEvent::RunE2eTests {
            tests,
            snapshots: None,
        },
        window_id: None,
        wait_for_render: false,
        dom_id: None,
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
/// The `AZ_DEBUG` environment variable should be set to a port number (e.g., `AZ_DEBUG=8765`).
/// Ports below 1024 require root/administrator privileges.
/// Returns `None` if not set or not a valid port number.
#[cfg(feature = "std")]
pub fn get_debug_port() -> Option<u16> {
    std::env::var("AZ_DEBUG").ok().and_then(|s| s.parse().ok())
}

/// Initialize the process-wide debug-server statics.
///
/// Split out of `start_debug_server`, which now lives in the DLL
/// (`desktop::shell2::common::debug_server::platform`) because it serves the
/// debugger UI from assets that only the DLL's `build.rs` emits. The STATE it
/// touches stays here, next to everything else that reads it.
#[cfg(feature = "std")]
#[cfg(feature = "e2e-server-http")]
pub fn init_debug_server_statics(port: u16) {
    SERVER_START_TIME.get_or_init(std::time::Instant::now);
    LOG_QUEUE.get_or_init(|| Mutex::new(Vec::new()));
    let _ = DEBUG_PORT.set(port);
    DEBUG_ENABLED.store(true, Ordering::SeqCst);
}

/// Publish the finished [`DebugServerHandle`] as the process-wide singleton.
/// Counterpart of [`get_debug_server`]; called by the DLL once its HTTP thread
/// is up.
#[cfg(feature = "std")]
#[cfg(feature = "e2e-server")]
pub fn set_debug_server(handle: Arc<DebugServerHandle>) {
    let _ = DEBUG_SERVER.set(handle);
}

/// The port the debug server was started on (`0` if it was never started).
/// Unlike [`get_debug_port`] this reads the ACTUAL bound port, not `AZ_DEBUG`.
#[cfg(feature = "std")]
#[cfg(feature = "e2e-server")]
#[must_use]
pub fn debug_server_port() -> u16 {
    DEBUG_PORT.get().copied().unwrap_or(0)
}

/// Create a debug channel without starting the HTTP server.
///
/// Used for E2E-only mode (`AZ_E2E_TEST` without `AZ_DEBUG`).
/// Creates the `spmc` channel, stores a minimal `DebugServerHandle` in
/// `DEBUG_SERVER`, and returns the receiver for window timers.
#[cfg(feature = "std")]
#[cfg(feature = "e2e-server")]
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

    let message: String = message.into();

    // Write to AZ_RECORD file if active
    if let Some(file) = RECORD_FILE.get() {
        if let Ok(mut f) = file.lock() {
            use std::io::Write;
            let _ = writeln!(
                f,
                "[{:>12}us] [{:?}] [{:?}] {}",
                timestamp_us, level, category, message
            );
        }
    }

    let msg = LogMessage {
        timestamp_us,
        level,
        category,
        message,
        location: format!("{}:{}", location.file(), location.line()),
        window_id: window_id.map(String::from),
    };

    if let Some(queue) = LOG_QUEUE.get() {
        if let Ok(mut q) = queue.lock() {
            // BOUNDED. This queue is drained by `take_logs()` when the debug
            // server is polled — but nothing forces a poll. Under `AZ_E2E`
            // with no HTTP client attached, nothing ever drains it and it
            // grows without limit: measured at **41 MB in a 45-second
            // scripted run** (224 882 messages, each holding a `String`
            // message, a `format!`ed location and a window id).
            //
            // That is not just a leak. It silently corrupts every memory
            // measurement taken under `AZ_E2E` — it was briefly reported as a
            // 41 MiB "resize retention" in azul's own RSS analysis before
            // per-call-site attribution showed the logger was the allocator.
            // An instrument that changes what it measures is worse than no
            // instrument.
            if q.len() >= MAX_LOG_MESSAGES {
                // Drop the OLDEST quarter, not one message: `drain` is O(n),
                // so evicting singly would make every push past the cap O(n)
                // and turn a memory problem into a latency one.
                let evict = MAX_LOG_MESSAGES / 4;
                q.drain(..evict);
                LOGS_DROPPED.fetch_add(evict as u64, Ordering::Relaxed);
            }
            q.push(msg);
        }
    }
}

/// Hard cap on retained log messages.
///
/// 20 000 x ~200 B is roughly 4 MB — enough to hold the recent history a
/// debugging session actually reads, small enough that an unpolled queue
/// cannot dominate the process.
const MAX_LOG_MESSAGES: usize = 20_000;

/// Messages evicted because the queue was full.
///
/// Counted rather than dropped silently: a truncated log that does not say it
/// is truncated makes the reader believe an event did not happen.
static LOGS_DROPPED: AtomicU64 = AtomicU64::new(0);

/// How many log messages have been evicted by the cap. Non-zero means the
/// queue was not drained fast enough and OLD messages were lost.
#[cfg(feature = "std")]
#[must_use]
pub fn logs_dropped() -> u64 {
    LOGS_DROPPED.load(Ordering::Relaxed)
}

#[cfg(all(test, feature = "std"))]
mod profile_report_tests {
    use super::*;

    /// The op must deserialise from the JSON a scenario actually writes, and
    /// default to `memory` when `kind` is omitted.
    #[test]
    fn get_profile_report_parses_from_scenario_json() {
        let ev: DebugEvent = serde_json::from_str(r#"{"op":"get_profile_report","kind":"memory"}"#)
            .expect("explicit kind must parse");
        assert!(matches!(
            ev,
            DebugEvent::GetProfileReport {
                kind: ProfileKind::Memory
            }
        ));

        let ev: DebugEvent =
            serde_json::from_str(r#"{"op":"get_profile_report"}"#).expect("kind must be optional");
        assert!(
            matches!(
                ev,
                DebugEvent::GetProfileReport {
                    kind: ProfileKind::Memory
                }
            ),
            "omitting kind must mean memory, not fail and not mean cpu"
        );

        let ev: DebugEvent = serde_json::from_str(r#"{"op":"get_profile_report","kind":"cpu"}"#)
            .expect("cpu must parse");
        assert!(matches!(
            ev,
            DebugEvent::GetProfileReport {
                kind: ProfileKind::Cpu
            }
        ));
    }

    /// An absent allocator must serialise as NULL, never 0.
    ///
    /// NEGATIVE CONTROL: change `allocator_live_kib` to a bare `u64` and this
    /// fails — the field comes back `0`, which a budget assertion reads as
    /// "no memory held". That is the worst available wrong answer, and it is
    /// the same failure the printed report avoids by saying "unavailable".
    #[test]
    fn absent_allocator_serialises_as_null_not_zero() {
        let r = ProfileResponse::default();
        let json = serde_json::to_string(&r).expect("serialises");
        assert!(
            json.contains(r#""allocator_live_kib":null"#),
            "absent allocator must be null, not 0; got {json}"
        );
        assert!(
            !json.contains(r#""allocator_live_kib":0"#),
            "a 0 here reads as 'no memory held'; got {json}"
        );
    }

    /// `phases_us` is omitted when empty rather than serialised as `[]`, so a
    /// memory snapshot does not carry a CPU field that was never measured.
    #[test]
    fn empty_phases_are_omitted_from_the_json() {
        let json = serde_json::to_string(&ProfileResponse::default()).unwrap();
        assert!(
            !json.contains("phases_us"),
            "empty phases must be omitted; got {json}"
        );

        let with = ProfileResponse {
            phases_us: vec![("layout".to_string(), 1234)],
            ..Default::default()
        };
        let json = serde_json::to_string(&with).unwrap();
        assert!(
            json.contains(r#""phases_us":[["layout",1234]]"#),
            "got {json}"
        );
    }
}

#[cfg(all(test, feature = "std"))]
mod log_queue_bound_tests {
    use super::*;

    /// The queue must not grow without limit.
    ///
    /// NEGATIVE CONTROL for the unbounded version: remove the eviction in
    /// `log_internal` and this fails with a length of 60 000 against a cap of
    /// 20 000. Unbounded, it reached 224 882 messages / ~41 MB in a 45-second
    /// scripted `AZ_E2E` run — and, worse, was briefly misread as a 41 MiB
    /// "resize retention" in azul's own memory analysis.
    #[test]
    fn log_queue_is_capped_and_reports_what_it_dropped() {
        // The queue only accepts messages while debug logging is on.
        DEBUG_ENABLED.store(true, Ordering::SeqCst);
        LOG_QUEUE.get_or_init(|| Mutex::new(Vec::new()));
        SERVER_START_TIME.get_or_init(std::time::Instant::now);

        let before_dropped = logs_dropped();
        let overshoot = MAX_LOG_MESSAGES * 3;
        for i in 0..overshoot {
            log_internal(LogLevel::Debug, LogCategory::General, format!("m{i}"), None);
        }

        let len = LOG_QUEUE.get().unwrap().lock().unwrap().len();
        assert!(
            len <= MAX_LOG_MESSAGES,
            "queue must stay at or under the {MAX_LOG_MESSAGES} cap; saw {len}"
        );
        assert!(
            logs_dropped() > before_dropped,
            "eviction must be COUNTED — a silently truncated log makes a \
             reader believe an event did not happen"
        );

        // The messages kept must be the RECENT ones: a debugging session
        // reads the tail, so evicting the newest would be the wrong end.
        let q = LOG_QUEUE.get().unwrap().lock().unwrap();
        let last = q.last().expect("queue is not empty").message.clone();
        assert_eq!(
            last,
            format!("m{}", overshoot - 1),
            "the most recent message must survive eviction"
        );
        drop(q);

        LOG_QUEUE.get().unwrap().lock().unwrap().clear();
        DEBUG_ENABLED.store(false, Ordering::SeqCst);
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
/// The most recent op response, so `print_response` can show it.
///
/// A `Mutex<Option<String>>` rather than a channel: it is written once per op
/// on the app thread and read by the very next op on the same thread.
pub(crate) static LAST_RESPONSE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

pub fn send_ok(
    request: &DebugRequest,
    window_state: Option<WindowStateSnapshot>,
    data: Option<ResponseData>,
) {
    if let Some(d) = data.as_ref() {
        if let Ok(json) = serde_json::to_string(d) {
            if let Ok(mut g) = LAST_RESPONSE.lock() {
                *g = Some(json);
            }
        }
    }
    // Clear logs to prevent memory buildup
    let _ = take_logs();
    let response = DebugResponseData::Ok { window_state, data };
    // Receiver may have disconnected (HTTP thread timed out) — ignore send errors
    let _ = request.response_tx.send(response);
}

/// Send an error response to a debug request
#[cfg(feature = "std")]
/// Synthesize a solid-color RGBA raster image for the image ops
/// (`set_node_image` / `add_image_to_cache`) — JSON cannot carry an ImageRef,
/// so the op smuggles `{width, height, color}` in, same discipline as
/// `add_timer` supplying the fn pointer JSON cannot express.
fn synthesize_solid_image(
    width: u32,
    height: u32,
    color: &str,
) -> Result<azul_core::resources::ImageRef, String> {
    use azul_core::resources::{ImageRef, RawImage, RawImageData, RawImageFormat};

    let col = azul_css::props::basic::color::parse_css_color(color)
        .map_err(|e| format!("Invalid color '{color}': {e:?}"))?;
    let w = width.max(1) as usize;
    let h = height.max(1) as usize;
    let mut pixels = Vec::with_capacity(w * h * 4);
    for _ in 0..(w * h) {
        pixels.extend_from_slice(&[col.r, col.g, col.b, col.a]);
    }
    ImageRef::new_rawimage(RawImage {
        pixels: RawImageData::U8(pixels.into()),
        width: w,
        height: h,
        premultiplied_alpha: false,
        data_format: RawImageFormat::RGBA8,
        tag: b"e2e-solid-image".to_vec().into(),
    })
    .ok_or_else(|| "ImageRef::new_rawimage returned None".to_string())
}

pub fn send_err(request: &DebugRequest, message: impl Into<String>) {
    // Clear logs to prevent memory buildup
    let _ = take_logs();
    let response = DebugResponseData::Err(message.into());
    // Receiver may have disconnected (HTTP thread timed out) — ignore send errors
    let _ = request.response_tx.send(response);
}

/// Helper function for serializing DebugHttpResponse
#[cfg(feature = "std")]
#[cfg(feature = "e2e-server")]
pub fn serialize_http_response(response: &DebugHttpResponse) -> String {
    serde_json::to_string_pretty(response)
        .unwrap_or_else(|_| r#"{"status":"error","message":"Serialization failed"}"#.to_string())
}

#[cfg(feature = "std")]
#[cfg(feature = "e2e-server")]
pub fn handle_event_request(
    body: &str,
    request_tx: &Arc<Mutex<spmc::Sender<DebugRequest>>>,
) -> String {
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
        /// The DOM this op addresses; omitted = the root DOM (0).
        /// A sibling of `op`, not a field of it: one spelling for every op.
        #[serde(default)]
        dom_id: Option<u64>,
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
                dom_id: req.dom_id,
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
                        DebugResponseData::Err(message) => {
                            DebugHttpResponse::Error(DebugHttpResponseError {
                                request_id: Some(request_id),
                                message,
                            })
                        }
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
#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize, Default)]
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

/// A single E2E test containing setup + steps.
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct E2eTest {
    /// Human-readable test name (required).
    pub name: String,
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
    /// Expected-outcome marker for the run.rs verdict tally.
    /// `Some("fail")` marks a KNOWN-FAILING test: a FAIL becomes XFAIL (does
    /// not fail the gate) and a PASS becomes XPASS (a failure — "bug fixed,
    /// remove the marker"). `None` is the normal pass=ok / fail=gate-failure.
    #[serde(default)]
    pub expect: Option<String>,
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
    /// Run this scenario with the caret / selection tweens (and the focus-ring
    /// glide, and `caret_scroll_glide`) TURNED ON, i.e. with
    /// `SystemAnimations::default()` instead of `SystemAnimations::disabled()`.
    ///
    /// Off by default, because a scenario that does not drive the clock in
    /// fixed steps would screenshot geometry mid-glide. It is safe to turn on
    /// precisely because engine time is virtual here: `run_e2e_test` freezes
    /// the clock for the scenario's thread and `tick_ms` / `wait` are the only
    /// things that advance it, so a tween's progress is a pure function of the
    /// ops the scenario ran.
    #[serde(default)]
    pub animations: bool,
}

#[cfg(feature = "std")]
fn default_width() -> u32 {
    800
}
#[cfg(feature = "std")]
fn default_height() -> u32 {
    600
}
#[cfg(feature = "std")]
fn default_dpi() -> u32 {
    96
}

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
        Self {
            passed: true,
            message: message.into(),
            actual: None,
            expected: None,
        }
    }
    fn fail(message: impl Into<String>) -> Self {
        Self {
            passed: false,
            message: message.into(),
            actual: None,
            expected: None,
        }
    }
    fn fail_with(
        message: impl Into<String>,
        expected: impl Into<String>,
        actual: impl Into<String>,
    ) -> Self {
        let expected = expected.into();
        let actual = actual.into();
        Self {
            passed: false,
            message: message.into(),
            actual: Some(actual),
            expected: Some(expected),
        }
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
/// | `assert_screenshot`  | `reference`, `threshold?`, `max_diff_ratio?`, `save_actual?` |
/// | `assert_state_machines_idle` | `damage?`                            |
/// | `assert_manager_invariants`  | `managers?`, `cross?`                |
/// | `assert_only_managers_changed` | `vs`, `changed`, `min_populated?`  |
/// | `assert_composition` | `expect`, `fixpoint?`, `damage?`             |
/// | `assert_damage_sound`| `vs`, `max_overpaint_ratio?`, `forbid_full?`, `pixel_identity?` |
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
        "assert_window_state" => eval_assert_window_state(params, callback_info),
        "assert_dom" => eval_assert_dom(params, callback_info),
        "assert_app_state" => eval_assert_app_state(params, app_data),
        "assert_scroll" => eval_assert_scroll(params, callback_info),
        "assert_screenshot" => eval_assert_screenshot(params, callback_info),
        // Damage / frame-work / resource observability (see FrameReport)
        "assert_damage" => eval_assert_damage(params, callback_info),
        "assert_changed" => eval_assert_changed(params, callback_info),
        "assert_damage_covers_changes" => eval_assert_damage_covers_changes(params, callback_info),
        "assert_damage_incremental" => eval_assert_damage_incremental(params, callback_info),
        "assert_idle_stable" => eval_assert_idle_stable(params, callback_info),
        "assert_work_bounded" => eval_assert_work_bounded(params, callback_info),
        "assert_resource_counts" => eval_assert_resource_counts(params, callback_info),
        // Manager / composition / damage-soundness (E2E_PLAN §(c)/(g1)/(g2)/(g3))
        "assert_state_machines_idle" => eval_assert_state_machines_idle(params, callback_info),
        "assert_manager_invariants" => eval_assert_manager_invariants(params, callback_info),
        "assert_only_managers_changed" => eval_assert_only_managers_changed(params, callback_info),
        "assert_composition" => eval_assert_composition(params, callback_info),
        "assert_damage_sound" => eval_assert_damage_sound(params, callback_info),
        "assert_stderr" => eval_assert_stderr(params),
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
            format!(
                "[E2E] assertion FAILED: {} (expected={:?}, actual={:?})",
                result.message, result.expected, result.actual
            ),
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
    if let Some(bad) = reject_unknown_params("assert_text", params, &["selector", "expected"]) {
        return bad;
    }
    let selector = match params.get("selector").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s,
        _ => return AssertionResult::fail("assert_text: missing 'selector' parameter"),
    };
    let expected = match params.get("expected").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return AssertionResult::fail("assert_text: missing 'expected' parameter"),
    };

    let node_id = match resolve_node_target(callback_info, params_dom(params), Some(selector), None, None) {
        Some(nid) => nid,
        None => {
            return AssertionResult::fail(format!(
                "assert_text: no node matches selector '{}'",
                selector
            ))
        }
    };

    // Get text content: try callback_info first, fall back to raw NodeType::Text
    use azul_core::dom::{DomId, DomNodeId};
    let dom_id = ROOT_DOM_ID;
    let dom_node_id = DomNodeId {
        dom: dom_id,
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
    if let Some(bad) = reject_unknown_params("assert_exists", params, &["selector"]) {
        return bad;
    }
    let selector = match params.get("selector").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s,
        _ => return AssertionResult::fail("assert_exists: missing 'selector' parameter"),
    };

    let matches = resolve_all_matching_nodes(callback_info, params_dom(params), selector);
    if matches.is_empty() {
        AssertionResult::fail(format!(
            "assert_exists: no node matches selector '{}'",
            selector
        ))
    } else {
        AssertionResult::pass(format!(
            "assert_exists: '{}' matched {} node(s)",
            selector,
            matches.len()
        ))
    }
}

/// `assert_not_exists`: assert that **no** node matches `selector`.
#[cfg(feature = "std")]
fn eval_assert_not_exists(
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> AssertionResult {
    if let Some(bad) = reject_unknown_params("assert_not_exists", params, &["selector"]) {
        return bad;
    }
    let selector = match params.get("selector").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s,
        _ => return AssertionResult::fail("assert_not_exists: missing 'selector' parameter"),
    };

    let matches = resolve_all_matching_nodes(callback_info, params_dom(params), selector);
    if matches.is_empty() {
        AssertionResult::pass(format!(
            "assert_not_exists: '{}' correctly has no matches",
            selector
        ))
    } else {
        AssertionResult::fail(format!(
            "assert_not_exists: selector '{}' unexpectedly matched {} node(s)",
            selector,
            matches.len()
        ))
    }
}

/// `assert_node_count`: assert that exactly `expected` nodes match `selector`.
#[cfg(feature = "std")]
fn eval_assert_node_count(
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> AssertionResult {
    if let Some(bad) = reject_unknown_params("assert_node_count", params, &["selector", "expected"])
    {
        return bad;
    }
    let selector = match params.get("selector").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s,
        _ => return AssertionResult::fail("assert_node_count: missing 'selector' parameter"),
    };
    let expected = match params.get("expected").and_then(|v| v.as_u64()) {
        Some(n) => n as usize,
        None => {
            return AssertionResult::fail(
                "assert_node_count: missing or invalid 'expected' (number)",
            )
        }
    };

    let matches = resolve_all_matching_nodes(callback_info, params_dom(params), selector);
    let actual = matches.len();

    if actual == expected {
        AssertionResult::pass(format!(
            "assert_node_count: '{}' has {} node(s)",
            selector, actual
        ))
    } else {
        AssertionResult::fail_with(
            format!("assert_node_count: selector '{}' count mismatch", selector),
            expected.to_string(),
            actual.to_string(),
        )
    }
}

/// `assert_window_state`: assert a property of the LIVE window state.
///
/// This is the observation surface for the window-level ops (`focus`, `blur`,
/// `move`, `dpi_changed`, `resize`) — without it those ops have no assertable
/// effect and a test using them is vacuous.
///
/// `property` is one of:
/// - `focused` / `window_focused` — bool (`expected` must be a bool)
/// - `dpi`, `hidpi_factor`
/// - `width` / `logical_width`, `height` / `logical_height`
/// - `physical_width`, `physical_height`
/// - `position_x`, `position_y` (fails if the window position is uninitialized)
///
/// Numeric comparisons take an optional `tolerance` (default 0.5).
#[cfg(feature = "std")]
fn eval_assert_window_state(
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> AssertionResult {
    if let Some(bad) = reject_unknown_params(
        "assert_window_state",
        params,
        &["property", "expected", "tolerance"],
    ) {
        return bad;
    }
    let property = match params.get("property").and_then(|v| v.as_str()) {
        Some(p) if !p.is_empty() => p,
        _ => return AssertionResult::fail("assert_window_state: missing 'property' parameter"),
    };
    let expected_val = match params.get("expected") {
        Some(v) => v,
        None => return AssertionResult::fail("assert_window_state: missing 'expected' parameter"),
    };

    let window_state = callback_info.get_current_window_state();
    let size = &window_state.size;

    // --- boolean properties ---
    if property == "focused" || property == "window_focused" {
        let expected = match expected_val.as_bool() {
            Some(b) => b,
            None => {
                return AssertionResult::fail(format!(
                    "assert_window_state: '{property}' needs a boolean 'expected'"
                ))
            }
        };
        // `flags.has_focus` is the OS-level flag; `window_focused` is the field
        // the state-diff pass reads to emit WindowFocusIn / WindowFocusOut.
        let actual = if property == "focused" {
            window_state.flags.has_focus
        } else {
            window_state.window_focused
        };
        return if actual == expected {
            AssertionResult::pass(format!("assert_window_state: {property} == {actual}"))
        } else {
            AssertionResult::fail_with(
                format!("assert_window_state: '{property}' mismatch"),
                expected.to_string(),
                actual.to_string(),
            )
        };
    }

    // --- numeric properties ---
    let expected = match expected_val.as_f64() {
        Some(n) => n,
        None => {
            return AssertionResult::fail(format!(
                "assert_window_state: '{property}' needs a numeric 'expected'"
            ))
        }
    };
    let tolerance = params
        .get("tolerance")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.5);

    let physical = size.get_physical_size();
    let actual = match property {
        "dpi" => f64::from(size.dpi),
        "hidpi_factor" => f64::from(size.get_hidpi_factor().inner.get()),
        "width" | "logical_width" => f64::from(size.dimensions.width),
        "height" | "logical_height" => f64::from(size.dimensions.height),
        "physical_width" => f64::from(physical.width),
        "physical_height" => f64::from(physical.height),
        "position_x" | "position_y" => match window_state.position {
            azul_core::window::WindowPosition::Initialized(p) => {
                if property == "position_x" {
                    f64::from(p.x)
                } else {
                    f64::from(p.y)
                }
            }
            other => {
                return AssertionResult::fail_with(
                    "assert_window_state: window position is not initialized".to_string(),
                    format!("{property} == {expected}"),
                    format!("{other:?}"),
                )
            }
        },
        other => {
            return AssertionResult::fail(format!(
                "assert_window_state: unknown property '{other}'"
            ))
        }
    };

    if (actual - expected).abs() <= tolerance {
        AssertionResult::pass(format!("assert_window_state: {property} == {actual}"))
    } else {
        AssertionResult::fail_with(
            format!("assert_window_state: '{property}' mismatch"),
            expected.to_string(),
            actual.to_string(),
        )
    }
}

/// `assert_dom`: assert on the DOM returned by `get_dom`.
///
/// Evaluated through the very same `build_dom_response()` the `get_dom` op
/// answers with, so the assertion covers the op's payload, not a parallel
/// re-derivation of it. At least one of:
/// - `node_count`   — total node count (exact)
/// - `min_node_count` — total node count (lower bound)
/// - `root_type`    — node type of the root (e.g. `body`)
/// - `root_children`— number of children nested under the root
/// - `contains`     — substring that must occur in the serialized HTML
/// - `not_contains` — substring that must NOT occur (catches a stale DOM read)
#[cfg(feature = "std")]
fn eval_assert_dom(
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> AssertionResult {
    if let Some(bad) = reject_unknown_params(
        "assert_dom",
        params,
        &[
            "contains",
            "not_contains",
            "node_count",
            "min_node_count",
            "root_type",
            "root_children",
        ],
    ) {
        return bad;
    }
    let Some(dom) = build_dom_response(callback_info, params_dom(params)) else {
        return AssertionResult::fail("assert_dom: no layout result for DOM 0");
    };

    let mut checks = 0usize;

    if let Some(expected) = params.get("node_count").and_then(serde_json::Value::as_u64) {
        checks += 1;
        if dom.node_count as u64 != expected {
            return AssertionResult::fail_with(
                "assert_dom: node_count mismatch".to_string(),
                expected.to_string(),
                dom.node_count.to_string(),
            );
        }
    }
    if let Some(expected) = params
        .get("min_node_count")
        .and_then(serde_json::Value::as_u64)
    {
        checks += 1;
        if (dom.node_count as u64) < expected {
            return AssertionResult::fail_with(
                "assert_dom: too few nodes".to_string(),
                format!(">= {expected}"),
                dom.node_count.to_string(),
            );
        }
    }
    if let Some(expected) = params.get("root_type").and_then(|v| v.as_str()) {
        checks += 1;
        if dom.root.node_type != expected {
            return AssertionResult::fail_with(
                "assert_dom: root_type mismatch".to_string(),
                expected.to_string(),
                dom.root.node_type.clone(),
            );
        }
    }
    if let Some(expected) = params
        .get("root_children")
        .and_then(serde_json::Value::as_u64)
    {
        checks += 1;
        if dom.root.children.len() as u64 != expected {
            return AssertionResult::fail_with(
                "assert_dom: root_children mismatch".to_string(),
                expected.to_string(),
                dom.root.children.len().to_string(),
            );
        }
    }
    if let Some(needle) = params.get("contains").and_then(|v| v.as_str()) {
        checks += 1;
        if !dom.html.contains(needle) {
            return AssertionResult::fail_with(
                "assert_dom: HTML does not contain the expected substring".to_string(),
                needle.to_string(),
                dom.html.clone(),
            );
        }
    }
    if let Some(needle) = params.get("not_contains").and_then(|v| v.as_str()) {
        checks += 1;
        if dom.html.contains(needle) {
            return AssertionResult::fail_with(
                "assert_dom: HTML still contains a substring it must not (stale DOM read)"
                    .to_string(),
                format!("no '{needle}'"),
                dom.html.clone(),
            );
        }
    }

    if checks == 0 {
        return AssertionResult::fail(
            "assert_dom: needs at least one of 'node_count' / 'root_type' / 'root_children' / \
             'contains'",
        );
    }

    AssertionResult::pass(format!(
        "assert_dom: {} check(s) passed ({} nodes)",
        checks, dom.node_count
    ))
}

/// Resolve a pointer-event target to a window position: an explicit `(x, y)`,
/// the centre of a node id, the centre of the first node matching a CSS
/// selector, or the centre of the first node whose text contains `text`.
///
/// Shared by the `click` and `double_click` ops so both accept the same
/// targeting parameters (coordinate-only targeting is brittle against layout
/// changes).
/// The centre of a node, for the ops that synthesise a click at it.
///
/// Hit-test bounds first (that is what a real pointer would land on), then the
/// laid-out rect. The fallback is what makes a CHILD DOM clickable: a
/// VirtualView's document and a `<transient-window>`'s content are laid out,
/// but their nodes are not in the parent window's hit-test structure, so
/// `get_node_hit_test_bounds` answers `None` for every one of them and a
/// selector click into AzWriter's document could only ever fail.
#[cfg(feature = "std")]
fn node_centre_for_click(
    callback_info: &azul_layout::callbacks::CallbackInfo,
    dom_node_id: azul_core::dom::DomNodeId,
) -> Option<(f32, f32)> {
    callback_info
        .get_node_hit_test_bounds(dom_node_id)
        .or_else(|| callback_info.get_node_rect(dom_node_id))
        .map(|rect| {
            (
                rect.origin.x + rect.size.width / 2.0,
                rect.origin.y + rect.size.height / 2.0,
            )
        })
}

#[cfg(feature = "std")]
fn resolve_click_position(
    callback_info: &azul_layout::callbacks::CallbackInfo,
    dom_id: azul_core::dom::DomId,
    x: Option<&f32>,
    y: Option<&f32>,
    node_id: Option<&u64>,
    selector: Option<&String>,
    text: Option<&String>,
) -> Option<(f32, f32)> {
    use azul_core::{dom::DomNodeId, id::NodeId};

    let centre = |nid: usize| -> Option<(f32, f32)> {
        node_centre_for_click(
            callback_info,
            DomNodeId {
                dom: dom_id,
                node: Some(NodeId::new(nid)).into(),
            },
        )
    };

    if let (Some(x), Some(y)) = (x, y) {
        return Some((*x, *y));
    }
    if let Some(nid) = node_id {
        return centre(*nid as usize);
    }

    let layout_window = callback_info.get_layout_window();
    let layout_result = layout_window.layout_results.get(&dom_id)?;
    let styled_dom = &layout_result.styled_dom;
    let node_data = styled_dom.node_data.as_container();

    if let Some(sel) = selector {
        use azul_core::style::matches_html_element;
        use azul_css::parser2::parse_css_path;

        let css_path = parse_css_path(sel.as_str()).ok()?;
        let node_hierarchy = styled_dom.node_hierarchy.as_container();
        let cascade_info = styled_dom.cascade_info.as_container();
        for i in 0..node_data.len() {
            if matches_html_element(
                &css_path,
                NodeId::new(i),
                &node_hierarchy,
                &node_data,
                &cascade_info,
                None,
            ) {
                if let Some(pos) = centre(i) {
                    return Some(pos);
                }
            }
        }
        return None;
    }

    if let Some(txt) = text {
        let hierarchy = styled_dom.node_hierarchy.as_container();
        for i in 0..node_data.len() {
            let azul_core::dom::NodeType::Text(t) = node_data[NodeId::new(i)].get_node_type()
            else {
                continue;
            };
            if !t.as_str().contains(txt.as_str()) {
                continue;
            }
            // Text nodes often have no hit-test bounds of their own.
            let node_hier = &hierarchy[NodeId::new(i)];
            let parent_idx = if node_hier.parent > 0 {
                node_hier.parent - 1
            } else {
                i
            };
            if let Some(pos) = centre(parent_idx).or_else(|| centre(i)) {
                return Some(pos);
            }
        }
    }
    None
}

/// `assert_layout`: assert a layout property (`x`, `y`, `width`, `height`)
/// of the first node matching `selector`. Optional `tolerance` (default 0.5).
#[cfg(feature = "std")]
fn eval_assert_layout(
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> AssertionResult {
    if let Some(bad) = reject_unknown_params(
        "assert_layout",
        params,
        &["selector", "property", "expected", "tolerance"],
    ) {
        return bad;
    }
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
    let tolerance: f64 = params
        .get("tolerance")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5);

    let node_id = match resolve_node_target(callback_info, params_dom(params), Some(selector), None, None) {
        Some(nid) => nid,
        None => {
            return AssertionResult::fail(format!(
                "assert_layout: no node matches selector '{}'",
                selector
            ))
        }
    };

    use azul_core::dom::{DomId, DomNodeId};
    let dom_node_id = DomNodeId {
        dom: ROOT_DOM_ID,
        node: Some(node_id).into(),
    };

    let rect = match callback_info.get_node_rect(dom_node_id) {
        Some(r) => r,
        None => {
            return AssertionResult::fail(format!(
                "assert_layout: node '{}' has no layout rect",
                selector
            ))
        }
    };

    let actual = match property {
        "x" => rect.origin.x as f64,
        "y" => rect.origin.y as f64,
        "width" => rect.size.width as f64,
        "height" => rect.size.height as f64,
        other => {
            return AssertionResult::fail(format!(
                "assert_layout: unknown property '{}' (use x, y, width, height)",
                other
            ))
        }
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
    if let Some(bad) =
        reject_unknown_params("assert_css", params, &["selector", "property", "expected"])
    {
        return bad;
    }
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

    let node_id = match resolve_node_target(callback_info, params_dom(params), Some(selector), None, None) {
        Some(nid) => nid,
        None => {
            return AssertionResult::fail(format!(
                "assert_css: no node matches selector '{}'",
                selector
            ))
        }
    };

    use azul_core::dom::{DomId, DomNodeId};
    use azul_css::props::property::{get_css_key_map, CssPropertyType};

    let dom_node_id = DomNodeId {
        dom: ROOT_DOM_ID,
        node: Some(node_id).into(),
    };

    // Try to parse the property name into CssPropertyType
    let key_map = get_css_key_map();
    let prop_type = match CssPropertyType::from_str(property, &key_map) {
        Some(pt) => pt,
        None => {
            return AssertionResult::fail(format!(
                "assert_css: unknown CSS property '{}'",
                property
            ))
        }
    };

    match callback_info.get_computed_css_property(dom_node_id, prop_type) {
        Some(computed) => {
            let actual = format!("{:?}", computed);
            if actual == expected {
                AssertionResult::pass(format!(
                    "assert_css: '{}' {} = {}",
                    selector, property, actual
                ))
            } else {
                AssertionResult::fail_with(
                    format!("assert_css: '{}' {} mismatch", selector, property),
                    expected,
                    actual,
                )
            }
        }
        None => AssertionResult::fail_with(
            format!(
                "assert_css: property '{}' not set on '{}'",
                property, selector
            ),
            expected,
            "(not set)",
        ),
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
    if let Some(bad) = reject_unknown_params("assert_app_state", params, &["path", "expected"]) {
        return bad;
    }
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
            "assert_app_state: app_data is not serializable (implement AzSerialize)",
        );
    }

    // Serialize app_data → JSON
    use azul_layout::json::serialize_refany_to_json;
    let json = match serialize_refany_to_json(app_data) {
        Some(j) => j,
        None => return AssertionResult::fail("assert_app_state: serialization returned null"),
    };

    // Convert our internal JSON into serde_json::Value
    let root: serde_json::Value = json.to_serde_value();

    // Navigate the dot-path
    let actual = navigate_json_path(&root, path);
    match actual {
        Some(val) => {
            if val == expected {
                AssertionResult::pass(format!("assert_app_state: '{}' = {}", path, val))
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
fn navigate_json_path<'a>(
    root: &'a serde_json::Value,
    path: &str,
) -> Option<&'a serde_json::Value> {
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
    // At least one AXIS is required. With neither, this resolved the selector,
    // confirmed the node had *some* scroll state, and returned pass — so it
    // passed for any offset, including one produced by a completely broken
    // scroll implementation.
    if params.get("x").is_none() && params.get("y").is_none() {
        return AssertionResult::fail(
            "assert_scroll: neither 'x' nor 'y' given — at least one is required, otherwise this \
             assertion never compares a position"
                .to_string(),
        );
    }
    if let Some(bad) = reject_unknown_params(
        "assert_scroll",
        params,
        &["selector", "x", "y", "tolerance"],
    ) {
        return bad;
    }
    let selector = match params.get("selector").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s,
        _ => return AssertionResult::fail("assert_scroll: missing 'selector' parameter"),
    };
    let tolerance: f64 = params
        .get("tolerance")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);

    let node_id = match resolve_node_target(callback_info, params_dom(params), Some(selector), None, None) {
        Some(nid) => nid,
        None => {
            return AssertionResult::fail(format!(
                "assert_scroll: no node matches selector '{}'",
                selector
            ))
        }
    };

    use azul_core::dom::{DomId, DomNodeId};
    let dom_id = ROOT_DOM_ID;
    let dom_node_id = DomNodeId {
        dom: dom_id,
        node: Some(node_id).into(),
    };

    let layout_window = callback_info.get_layout_window();
    let scroll_offset = layout_window
        .scroll_manager
        .get_current_offset(dom_id, node_id);

    let offset = match scroll_offset {
        Some(o) => o,
        None => {
            return AssertionResult::fail(format!(
                "assert_scroll: node '{}' is not scrollable or has no scroll state",
                selector
            ))
        }
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
        "assert_scroll: '{}' at ({:.1}, {:.1})",
        selector, offset.x, offset.y
    ))
}

/// Baseline-recording opt-in for `assert_screenshot` (`AZ_E2E_RECORD=1`).
///
/// OFF by default, on purpose: a missing reference must be RED, never a silent
/// "record whatever azul does today and call it expected".
#[cfg(feature = "std")]
fn e2e_record_mode() -> bool {
    static RECORD: OnceLock<bool> = OnceLock::new();
    *RECORD.get_or_init(|| {
        std::env::var("AZ_E2E_RECORD")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

/// `assert_screenshot`: compare current CPU-rendered frame against a reference PNG.
///
/// Parameters:
/// - `reference` (required): path to reference PNG file
/// - `threshold` (optional, default 2): per-channel tolerance (0=exact, 2=anti-alias)
/// - `max_diff_ratio` (optional, default 0.0): max fraction of pixels allowed to differ
/// - `save_actual` (optional): path to save the actual screenshot for debugging
#[cfg(feature = "std")]
fn eval_assert_screenshot(
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> AssertionResult {
    if let Some(bad) = reject_unknown_params(
        "assert_screenshot",
        params,
        &["reference", "threshold", "max_diff_ratio", "save_actual"],
    ) {
        return bad;
    }
    #[cfg(not(feature = "cpurender"))]
    {
        return AssertionResult::fail("assert_screenshot: cpurender feature not enabled");
    }

    #[cfg(feature = "cpurender")]
    {
        let reference_path = match params.get("reference").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s,
            _ => return AssertionResult::fail("assert_screenshot: missing 'reference' path"),
        };
        let threshold = params
            .get("threshold")
            .and_then(|v| v.as_u64())
            .unwrap_or(2) as u8;
        let max_diff_ratio = params
            .get("max_diff_ratio")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let save_actual = params.get("save_actual").and_then(|v| v.as_str());

        // Take a screenshot of the current frame
        let dom_id = ROOT_DOM_ID;
        let png_bytes = match callback_info.take_screenshot(dom_id) {
            Ok(bytes) => bytes,
            Err(e) => {
                return AssertionResult::fail(format!(
                    "assert_screenshot: screenshot failed: {}",
                    e.as_str()
                ))
            }
        };

        // Decode the rendered screenshot
        let rendered = match azul_layout::cpurender::AzulPixmap::decode_png(&png_bytes) {
            Ok(p) => p,
            Err(e) => {
                return AssertionResult::fail(format!(
                    "assert_screenshot: decode rendered PNG failed: {}",
                    e
                ))
            }
        };

        // Save the actual screenshot if requested
        if let Some(actual_path) = save_actual {
            let _ = std::fs::write(actual_path, &png_bytes);
        }

        // Load and compare against reference
        let ref_bytes = match std::fs::read(reference_path) {
            Ok(b) => b,
            Err(e) => {
                // A MISSING REFERENCE IS A FAILURE.
                //
                // This used to write azul's own output as the reference and
                // return `pass` — i.e. it enshrined whatever the engine did
                // today as "expected", forever. Recording a baseline is now an
                // explicit opt-in (`AZ_E2E_RECORD=1`), and a baseline recorded
                // that way is reported as PROVISIONAL and still FAILS the run,
                // so it cannot silently gate green until a human has reviewed
                // the PNG and re-run without the env var.
                if e.kind() == std::io::ErrorKind::NotFound {
                    if e2e_record_mode() {
                        if let Some(parent) = std::path::Path::new(reference_path).parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        if let Err(we) = std::fs::write(reference_path, &png_bytes) {
                            return AssertionResult::fail(format!(
                                "assert_screenshot: reference not found and could not record \
                                 baseline: {we}"
                            ));
                        }
                        return AssertionResult::fail_with(
                            format!(
                                "assert_screenshot: PROVISIONAL baseline recorded to {} ({}x{}) \
                                 — review it and re-run WITHOUT AZ_E2E_RECORD to gate on it",
                                reference_path,
                                rendered.width(),
                                rendered.height()
                            ),
                            "an existing, reviewed reference PNG".to_string(),
                            "provisional (just recorded, unreviewed)".to_string(),
                        );
                    }
                    return AssertionResult::fail_with(
                        format!(
                            "assert_screenshot: reference {reference_path} does not exist (run \
                             with AZ_E2E_RECORD=1 to record a provisional baseline)"
                        ),
                        "an existing reference PNG".to_string(),
                        "missing".to_string(),
                    );
                }
                return AssertionResult::fail(format!(
                    "assert_screenshot: cannot read reference {}: {}",
                    reference_path, e
                ));
            }
        };

        let reference = match azul_layout::cpurender::AzulPixmap::decode_png(&ref_bytes) {
            Ok(p) => p,
            Err(e) => {
                return AssertionResult::fail(format!(
                    "assert_screenshot: decode reference PNG failed: {}",
                    e
                ))
            }
        };

        let result = azul_layout::cpurender::pixel_diff(&reference, &rendered, threshold);

        if !result.dimensions_match {
            return AssertionResult::fail_with(
                "assert_screenshot: dimension mismatch".to_string(),
                format!("{}x{}", result.ref_width, result.ref_height),
                format!("{}x{}", result.test_width, result.test_height),
            );
        }

        let ratio = result.diff_ratio();
        if ratio > max_diff_ratio {
            return AssertionResult::fail_with(
                format!(
                    "assert_screenshot: {}/{} pixels differ (max_delta={})",
                    result.diff_count, result.total_pixels, result.max_delta
                ),
                format!("diff_ratio <= {:.4}", max_diff_ratio),
                format!("diff_ratio = {:.4}", ratio),
            );
        }

        AssertionResult::pass(format!(
            "assert_screenshot: match ({}x{}, {}/{} pixels differ, threshold={})",
            rendered.width(),
            rendered.height(),
            result.diff_count,
            result.total_pixels,
            threshold
        ))
    }
}

// ==================== Damage / frame-work / resource assertions ====================
//
// All of these read `LayoutWindow::frame_report` (`azul_layout::window::FrameReport`),
// which the CPU backend fills after every `render_frame` and the event loop fills
// with the frame-work counters. Before this existed, `FrameDamage` lived only on
// `CpuBackend` (the *window*), which an assertion — which only ever sees
// `CallbackInfo -> LayoutWindow` — could not reach.

/// The window's logical size, used to express damage area as a ratio.
#[cfg(feature = "std")]
fn window_logical_area(callback_info: &azul_layout::callbacks::CallbackInfo) -> f32 {
    let size = callback_info.get_current_window_state().size.dimensions;
    (size.width * size.height).max(1.0)
}

/// Pick the paint (default) or present damage out of the frame report.
///
/// By default this is the damage ACCUMULATED SINCE THE LAST `reset_frame_counters`,
/// not the last frame's: between the step that changed something and the
/// assertion, the engine may render further idle frames whose damage is `None`,
/// and those would otherwise clobber the damage the test wants to see. Pass
/// `"frame": "last"` to look at the most recent frame only (that is what
/// `assert_idle_stable` does).
#[cfg(feature = "std")]
fn damage_of(
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> (azul_layout::window::FrameDamage, &'static str) {
    let report = frame_report_of(callback_info);
    let last_only = params.get("frame").and_then(|v| v.as_str()) == Some("last");
    match params.get("which").and_then(|v| v.as_str()) {
        Some("present") => (
            if last_only {
                report.present_damage.clone()
            } else {
                report.accumulated_present_damage.clone()
            },
            "present",
        ),
        _ => (
            if last_only {
                report.paint_damage.clone()
            } else {
                report.accumulated_paint_damage.clone()
            },
            "paint",
        ),
    }
}

#[cfg(feature = "std")]
fn damage_kind_str(d: &azul_layout::window::FrameDamage) -> &'static str {
    if d.is_none() {
        "none"
    } else if d.is_full() {
        "full"
    } else {
        "rects"
    }
}

/// Serialise a `FrameReport` for `get_frame_report`.
#[cfg(feature = "std")]
fn build_frame_report_response(
    report: &azul_layout::window::FrameReport,
    window_area: f32,
) -> FrameReportResponse {
    let to_json = |d: &azul_layout::window::FrameDamage| {
        d.rects()
            .unwrap_or(&[])
            .iter()
            .map(|r| DamageRectJson {
                x: r.origin.x,
                y: r.origin.y,
                width: r.size.width,
                height: r.size.height,
            })
            .collect::<Vec<_>>()
    };
    FrameReportResponse {
        frame_index: report.frame_index,
        dl_rebuilds: report.dl_rebuilds,
        last_dl_build_patched: report.last_dl_build_patched,
        paint_damage_kind: damage_kind_str(&report.paint_damage).to_string(),
        paint_damage_rects: to_json(&report.paint_damage),
        paint_damage_area_ratio: report.paint_damage.area(window_area) / window_area,
        present_damage_kind: damage_kind_str(&report.present_damage).to_string(),
        present_damage_rects: to_json(&report.present_damage),
        present_damage_area_ratio: report.present_damage.area(window_area) / window_area,
        accumulated_paint_damage_kind: damage_kind_str(&report.accumulated_paint_damage)
            .to_string(),
        accumulated_paint_damage_rects: to_json(&report.accumulated_paint_damage),
        accumulated_present_damage_kind: damage_kind_str(&report.accumulated_present_damage)
            .to_string(),
        frames_since_reset: report.frames_since_reset,
        relayout_iterations: report.relayout_iterations,
        dom_regenerations: report.dom_regenerations,
        layout_passes: report.layout_passes,
        hit_depth_cap: report.hit_depth_cap,
        terminal_result: report.terminal_result,
        test_clock_offset_ms: azul_core::task::test_clock_offset_ms(),
    }
}

/// Render the PARTIAL SCREEN UPDATE as a PNG: the current frame masked to the
/// damage region (pixels outside the damaged rects are transparent), optionally
/// cropped to the damage bounding box.
#[cfg(all(feature = "std", feature = "cpurender"))]
fn capture_damage_png(
    callback_info: &azul_layout::callbacks::CallbackInfo,
    which: Option<&str>,
    crop: bool,
) -> Result<Vec<u8>, String> {
    use azul_layout::cpurender::AzulPixmap;

    // Accumulated since the last `reset_frame_counters` — see `damage_of`.
    let report = frame_report_of(callback_info);
    let damage = if which == Some("present") {
        report.accumulated_present_damage.clone()
    } else {
        report.accumulated_paint_damage.clone()
    };

    let frame = render_current(callback_info)?;
    let (w, h) = (frame.width(), frame.height());
    let logical_w = callback_info
        .get_current_window_state()
        .size
        .dimensions
        .width
        .max(1.0);
    #[allow(clippy::cast_precision_loss)]
    let scale = w as f32 / logical_w;

    // Physical-pixel rects of the damage region.
    let rects: Vec<(i64, i64, i64, i64)> = match &damage {
        azul_layout::window::FrameDamage::None => Vec::new(),
        azul_layout::window::FrameDamage::Full => vec![(0, 0, i64::from(w), i64::from(h))],
        azul_layout::window::FrameDamage::Rects(rs) => rs
            .iter()
            .map(|r| {
                (
                    (r.origin.x * scale).floor() as i64,
                    (r.origin.y * scale).floor() as i64,
                    ((r.origin.x + r.size.width) * scale).ceil() as i64,
                    ((r.origin.y + r.size.height) * scale).ceil() as i64,
                )
            })
            .collect(),
    };

    // Bounding box (also the crop window).
    let (mut bx0, mut by0, mut bx1, mut by1) = (i64::MAX, i64::MAX, i64::MIN, i64::MIN);
    for (x0, y0, x1, y1) in &rects {
        bx0 = bx0.min(*x0);
        by0 = by0.min(*y0);
        bx1 = bx1.max(*x1);
        by1 = by1.max(*y1);
    }
    if rects.is_empty() {
        // No damage → a 1x1 fully transparent PNG, so the file still exists and
        // its emptiness is the signal.
        let mut empty = AzulPixmap::new(1, 1).ok_or("cannot allocate 1x1 pixmap")?;
        empty.fill(0, 0, 0, 0);
        return empty.encode_png();
    }
    let (bx0, by0, bx1, by1) = (
        bx0.clamp(0, i64::from(w)),
        by0.clamp(0, i64::from(h)),
        bx1.clamp(0, i64::from(w)),
        by1.clamp(0, i64::from(h)),
    );

    let (out_w, out_h, off_x, off_y) = if crop {
        (
            (bx1 - bx0).max(1) as u32,
            (by1 - by0).max(1) as u32,
            bx0,
            by0,
        )
    } else {
        (w, h, 0, 0)
    };

    let mut out = AzulPixmap::new(out_w, out_h).ok_or("cannot allocate output pixmap")?;
    out.fill(0, 0, 0, 0); // transparent everywhere outside the damage region
    {
        let src = frame.data().to_vec();
        let dst = out.data_mut();
        for (x0, y0, x1, y1) in &rects {
            for y in (*y0).max(0)..(*y1).min(i64::from(h)) {
                for x in (*x0).max(0)..(*x1).min(i64::from(w)) {
                    let (dx, dy) = (x - off_x, y - off_y);
                    if dx < 0 || dy < 0 || dx >= i64::from(out_w) || dy >= i64::from(out_h) {
                        continue;
                    }
                    let si = ((y as usize) * (w as usize) + (x as usize)) * 4;
                    let di = ((dy as usize) * (out_w as usize) + (dx as usize)) * 4;
                    dst[di..di + 4].copy_from_slice(&src[si..si + 4]);
                }
            }
        }
    }
    out.encode_png()
}

/// Fail an assertion that was handed a parameter it does not read.
///
/// Only `assert_damage` had this guard. Every other evaluator matched its params
/// with `if let Some(..)` and silently ignored the rest, so ONE TYPO
/// (`max_relayout` for `max_relayouts`, `min_rect` for `min_rects`) turned the
/// assertion into an unconditional pass that still counted as coverage. Against
/// a generated corpus that is a false-green factory, and it gets worse as
/// parameters are added: a test asking for a bound the evaluator does not yet
/// support would go green while asserting nothing at all.
///
/// The list passed here is also the assertion's advertised param surface — the
/// generator's schema scan reads these call sites (`gene2e.rs::reject_guard_keys`),
/// so a param that is accepted is a param the model is told about, and vice
/// versa, by construction.
///
/// `op` and `screenshot` are STEP-level keys owned by the harness, never by an
/// assertion.
/// The DOM an ASSERTION addresses, from its own `dom_id` param (assertions
/// are evaluated from their JSON params, not from a `DebugRequest`). Defaults
/// to the root DOM, exactly like the ops.
#[cfg(feature = "std")]
fn params_dom(params: &serde_json::Value) -> azul_core::dom::DomId {
    params
        .get("dom_id")
        .and_then(serde_json::Value::as_u64)
        .map_or(ROOT_DOM_ID, |inner| azul_core::dom::DomId {
            inner: inner as usize,
        })
}

#[cfg(feature = "std")]
fn reject_unknown_params(
    op: &str,
    params: &serde_json::Value,
    allowed: &[&str],
) -> Option<AssertionResult> {
    // `dom_id` is an ENVELOPE key (see `DebugRequest::dom_id`), valid on
    // every op and every assertion — not something each whitelist repeats.
    const HARNESS_KEYS: &[&str] = &["op", "screenshot", "dom_id"];
    let obj = params.as_object()?;
    for key in obj.keys() {
        if HARNESS_KEYS.contains(&key.as_str()) || allowed.contains(&key.as_str()) {
            continue;
        }
        return Some(AssertionResult::fail(format!(
            "{op}: unknown parameter '{key}' (known: {}). Ignoring it would let a typo'd bound \
             assert NOTHING while the step reports green.",
            allowed.join(", ")
        )));
    }
    None
}

/// `assert_damage`: the raw predicate over the last frame's damage.
///
/// Parameters:
/// - `which`: `"paint"` (default) or `"present"`
/// - `frame`: `"last"` to look at the last frame instead of the accumulation
/// - `kind`: `"none"` | `"rects"` | `"full"` — exact damage kind
/// - `min_rects` / `max_rects`: bounds on the rect count
/// - `max_area_ratio`: damaged area / window area upper bound
///
/// AT LEAST ONE of `kind` / `min_rects` / `max_rects` / `max_area_ratio` is
/// REQUIRED, and an unrecognised key is an error. Every constraint used to be
/// matched with `if let Some(..)`, so a step with none of them — or with ONE
/// TYPO (`max_area` instead of `max_area_ratio`) — fell straight through to
/// `pass`. Against a generated corpus that is a false-green factory: the typo'd
/// test asserts nothing and counts as coverage.
#[cfg(feature = "std")]
/// `assert_stderr` — assert on the framework DIAGNOSTICS emitted so far.
///
/// ```json
/// { "op": "assert_stderr", "not_contains": "image-churn" }
/// { "op": "assert_stderr", "contains": "image-churn", "clear": true }
/// ```
///
/// Reads `azul_core::diagnostics`, the in-process ring every engine lint writes
/// to through `diagnostics::emit`. Not literal stderr: a process cannot read
/// back its own file descriptor, and capturing one would not survive an
/// application that has routed diagnostics to its own logger. The ring records
/// regardless of where the sink sends them, so this keeps working when output
/// is muted or shipped to Loki.
///
/// `clear` empties the ring AFTER evaluating, so the next step starts from a
/// known state — otherwise a warning from step 2 satisfies an assertion in
/// step 9.
///
/// The point of `not_contains` is regression pressure: a scenario that provokes
/// the conditions of a lint and asserts the lint stays quiet is a test that the
/// underlying bug has not come back.
fn eval_assert_stderr(params: &serde_json::Value) -> AssertionResult {
    const CONSTRAINTS: &[&str] = &["contains", "not_contains", "clear"];

    if let Some(bad) = reject_unknown_params("assert_stderr", params, CONSTRAINTS) {
        return bad;
    }

    let contains = params.get("contains").and_then(|v| v.as_str());
    let not_contains = params.get("not_contains").and_then(|v| v.as_str());
    if contains.is_none() && not_contains.is_none() {
        return AssertionResult::fail(
            "assert_stderr needs `contains` or `not_contains` — with neither it \
             asserts nothing and would pass forever",
        );
    }

    let recorded = azul_core::diagnostics::recorded();
    let clear_after = params
        .get("clear")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    let mut result = AssertionResult::pass("diagnostics match");

    if let Some(needle) = contains {
        if !recorded.iter().any(|m| m.contains(needle)) {
            result = AssertionResult::fail_with(
                format!("no framework diagnostic contains {needle:?}"),
                needle.to_string(),
                format!("{} diagnostic(s): {recorded:?}", recorded.len()),
            );
        }
    }

    if result.passed {
        if let Some(needle) = not_contains {
            if let Some(hit) = recorded.iter().find(|m| m.contains(needle)) {
                result = AssertionResult::fail_with(
                    format!(
                        "a framework diagnostic contains {needle:?}, which this \
                         scenario asserts must not happen"
                    ),
                    format!("no diagnostic containing {needle:?}"),
                    hit.clone(),
                );
            }
        }
    }

    if clear_after {
        azul_core::diagnostics::clear();
    }
    result
}

fn eval_assert_damage(
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> AssertionResult {
    const CONSTRAINTS: &[&str] = &["kind", "min_rects", "max_rects", "max_area_ratio"];

    if let Some(bad) = reject_unknown_params(
        "assert_damage",
        params,
        &[
            "kind",
            "min_rects",
            "max_rects",
            "max_area_ratio",
            "which",
            "frame",
        ],
    ) {
        return bad;
    }
    if !CONSTRAINTS.iter().any(|c| params.get(*c).is_some()) {
        return AssertionResult::fail(format!(
            "assert_damage: no constraint given — at least one of {} is required, otherwise this \
             assertion passes unconditionally",
            CONSTRAINTS.join(", ")
        ));
    }

    let (damage, which) = damage_of(params, callback_info);
    let kind = damage_kind_str(&damage);
    let rects = damage.rect_count();
    let area = damage.area(window_logical_area(callback_info));
    let ratio = f64::from(area / window_logical_area(callback_info));

    if let Some(expected) = params.get("kind").and_then(|v| v.as_str()) {
        if expected != kind {
            return AssertionResult::fail_with(
                format!("assert_damage: wrong {which} damage kind"),
                expected.to_string(),
                kind.to_string(),
            );
        }
    }
    if let Some(min) = params.get("min_rects").and_then(serde_json::Value::as_u64) {
        if (rects as u64) < min {
            return AssertionResult::fail_with(
                format!("assert_damage: too few {which} damage rects"),
                format!(">= {min}"),
                rects.to_string(),
            );
        }
    }
    if let Some(max) = params.get("max_rects").and_then(serde_json::Value::as_u64) {
        if (rects as u64) > max {
            return AssertionResult::fail_with(
                format!("assert_damage: too many {which} damage rects"),
                format!("<= {max}"),
                rects.to_string(),
            );
        }
    }
    if let Some(max_ratio) = params
        .get("max_area_ratio")
        .and_then(serde_json::Value::as_f64)
    {
        if ratio > max_ratio {
            return AssertionResult::fail_with(
                format!("assert_damage: {which} damage area too large"),
                format!("<= {max_ratio:.4} of the window"),
                format!("{ratio:.4}"),
            );
        }
    }
    AssertionResult::pass(format!(
        "assert_damage: {which} damage = {kind} ({rects} rect(s), {:.2}% of the window)",
        ratio * 100.0
    ))
}

/// Decode a named `snapshot_frame` PNG.
#[cfg(all(feature = "std", feature = "cpurender"))]
fn load_snapshot(
    name: &str,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> Result<azul_layout::cpurender::AzulPixmap, String> {
    let bytes = scratch(callback_info)
        .frame_snapshots
        .get(name)
        .cloned()
        .ok_or_else(|| format!("no frame snapshot named '{name}' (use snapshot_frame first)"))?;
    azul_layout::cpurender::AzulPixmap::decode_png(&bytes)
}

/// Render the current frame through the independent full-repaint path.
#[cfg(all(feature = "std", feature = "cpurender"))]
fn render_current(
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> Result<azul_layout::cpurender::AzulPixmap, String> {
    let png = callback_info
        .take_screenshot(ROOT_DOM_ID)
        .map_err(|e| e.as_str().to_string())?;
    azul_layout::cpurender::AzulPixmap::decode_png(&png)
}

/// `assert_changed` — LIVENESS, the stale-screen detector.
///
/// After a step that must alter pixels: the damage set must be NON-EMPTY *and*
/// the pixels must actually differ from the named `snapshot_frame`.
///
/// Parameters: `vs` (snapshot name, required), `min_damage_rects` (default 1),
/// `threshold` (per-channel, default 2).
#[cfg(feature = "std")]
fn eval_assert_changed(
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> AssertionResult {
    if let Some(bad) = reject_unknown_params(
        "assert_changed",
        params,
        &["vs", "min_damage_rects", "threshold", "which", "frame"],
    ) {
        return bad;
    }
    #[cfg(not(feature = "cpurender"))]
    {
        let _ = (params, callback_info);
        return AssertionResult::fail("assert_changed: cpurender feature not enabled");
    }
    #[cfg(feature = "cpurender")]
    {
        let min_rects = params
            .get("min_damage_rects")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1);
        let threshold = params
            .get("threshold")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(2) as u8;

        let (damage, which) = damage_of(params, callback_info);
        if damage.is_none() {
            return AssertionResult::fail_with(
                "assert_changed: nothing was repainted (stale screen)".to_string(),
                format!("{which} damage != none"),
                "none".to_string(),
            );
        }
        if (damage.rect_count() as u64) < min_rects {
            return AssertionResult::fail_with(
                "assert_changed: too few damage rects".to_string(),
                format!(">= {min_rects}"),
                damage.rect_count().to_string(),
            );
        }

        let Some(vs) = params.get("vs").and_then(|v| v.as_str()) else {
            return AssertionResult::pass(format!(
                "assert_changed: {which} damage = {} ({} rect(s)); no 'vs' snapshot given, pixels \
                 not compared",
                damage_kind_str(&damage),
                damage.rect_count()
            ));
        };
        let before = match load_snapshot(vs, callback_info) {
            Ok(p) => p,
            Err(e) => return AssertionResult::fail(format!("assert_changed: {e}")),
        };
        let after = match render_current(callback_info) {
            Ok(p) => p,
            Err(e) => return AssertionResult::fail(format!("assert_changed: {e}")),
        };
        let diff = azul_layout::cpurender::pixel_diff(&before, &after, threshold);
        if !diff.dimensions_match {
            // This used to `pass()` on the reasoning that differently-sized
            // images "necessarily differ". Screenshots are PHYSICAL-pixel
            // sized, so EVERY `resize` and EVERY `dpi_changed` lands here — and
            // the pass was unconditional, which meant a resize that rendered a
            // completely blank window (the classic "content vanished after
            // resize" bug) satisfied a liveness assertion.
            //
            // Its sibling `assert_damage_covers_changes` already FAILS on
            // exactly this input ("frame size changed between snapshot and
            // now"). The asymmetry WAS the bug: two assertions over the same
            // two images disagreeing about whether the comparison is possible.
            //
            // There is no honest pixel answer across a dimension change, so
            // refuse instead of inventing one. Two ways to keep the assertion:
            // compare at equal dimensions (resize away and back, then diff
            // against the pre-resize snapshot — a resize that loses content
            // shows up at the ORIGINAL size, where the comparison is real), or
            // drop `vs` and assert the damage half alone.
            return AssertionResult::fail_with(
                format!(
                    "assert_changed: frame size changed between snapshot '{vs}' and now, so the \
                     pixel comparison is undefined — it is NOT evidence that anything was drawn. \
                     Either round-trip back to the original size and compare there, or omit 'vs' \
                     and assert the damage alone."
                ),
                format!("{}x{}", before.width(), before.height()),
                format!("{}x{}", after.width(), after.height()),
            );
        }
        if diff.diff_count == 0 {
            return AssertionResult::fail_with(
                "assert_changed: damage was reported but NO pixel changed (the engine repainted \
                 to the same value — the stale-content signature)"
                    .to_string(),
                "diff_count > 0".to_string(),
                "0".to_string(),
            );
        }
        AssertionResult::pass(format!(
            "assert_changed: {} px changed, {which} damage = {} ({} rect(s))",
            diff.diff_count,
            damage_kind_str(&damage),
            damage.rect_count()
        ))
    }
}

/// `assert_damage_covers_changes` — SOUNDNESS (coverage / under-paint).
///
/// Every pixel that differs between the named snapshot and the current
/// full-repaint render must lie inside the union of the paint-damage rects.
/// An uncovered changed pixel is a stale pixel on a real screen.
///
/// Parameters: `vs` (snapshot name, required), `threshold`, `slack_px`
/// (rect inflation, default 1 — damage is logical, pixels are physical).
#[cfg(feature = "std")]
fn eval_assert_damage_covers_changes(
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> AssertionResult {
    if let Some(bad) = reject_unknown_params(
        "assert_damage_covers_changes",
        params,
        &["vs", "threshold", "slack_px", "which", "frame"],
    ) {
        return bad;
    }
    #[cfg(not(feature = "cpurender"))]
    {
        let _ = (params, callback_info);
        return AssertionResult::fail("assert_damage_covers_changes: cpurender not enabled");
    }
    #[cfg(feature = "cpurender")]
    {
        let Some(vs) = params.get("vs").and_then(|v| v.as_str()) else {
            return AssertionResult::fail("assert_damage_covers_changes: missing 'vs'");
        };
        let threshold = params
            .get("threshold")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(2) as u8;
        let slack = params
            .get("slack_px")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(1.0) as f32;

        let (damage, _) = damage_of(params, callback_info);
        if damage.is_full() {
            return AssertionResult::pass(
                "assert_damage_covers_changes: full repaint trivially covers every changed pixel \
                 (use assert_damage_incremental to forbid that)"
                    .to_string(),
            );
        }
        let before = match load_snapshot(vs, callback_info) {
            Ok(p) => p,
            Err(e) => return AssertionResult::fail(format!("assert_damage_covers_changes: {e}")),
        };
        let after = match render_current(callback_info) {
            Ok(p) => p,
            Err(e) => return AssertionResult::fail(format!("assert_damage_covers_changes: {e}")),
        };
        if before.width() != after.width() || before.height() != after.height() {
            return AssertionResult::fail(
                "assert_damage_covers_changes: frame size changed between snapshot and now",
            );
        }

        // logical → physical scale of the screenshot
        let logical_w = callback_info
            .get_current_window_state()
            .size
            .dimensions
            .width
            .max(1.0);
        let scale = after.width() as f32 / logical_w;

        let rects: Vec<(f32, f32, f32, f32)> = damage
            .rects()
            .unwrap_or(&[])
            .iter()
            .map(|r| {
                (
                    r.origin.x * scale - slack,
                    r.origin.y * scale - slack,
                    (r.origin.x + r.size.width) * scale + slack,
                    (r.origin.y + r.size.height) * scale + slack,
                )
            })
            .collect();

        let (bd, ad) = (before.data(), after.data());
        let w = after.width() as usize;
        let h = after.height() as usize;
        let mut uncovered = 0u64;
        let mut first: Option<(usize, usize)> = None;
        let mut changed = 0u64;
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                let differs = (0..4).any(|c| bd[i + c].abs_diff(ad[i + c]) > threshold);
                if !differs {
                    continue;
                }
                changed += 1;
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let covered = rects
                    .iter()
                    .any(|(x0, y0, x1, y1)| px >= *x0 && px < *x1 && py >= *y0 && py < *y1);
                if !covered {
                    uncovered += 1;
                    if first.is_none() {
                        first = Some((x, y));
                    }
                }
            }
        }
        if uncovered > 0 {
            let (fx, fy) = first.unwrap_or((0, 0));
            return AssertionResult::fail_with(
                "assert_damage_covers_changes: the damage set does NOT cover every changed pixel \
                 (under-paint → stale pixels on screen)"
                    .to_string(),
                "0 uncovered changed pixels".to_string(),
                format!("{uncovered} uncovered (of {changed} changed), first at ({fx}, {fy})"),
            );
        }
        AssertionResult::pass(format!(
            "assert_damage_covers_changes: all {changed} changed px lie inside the {} damage rect(s)",
            rects.len()
        ))
    }
}

/// `assert_damage_incremental` — INCREMENTALITY: the repaint is a PATCH, not a
/// full redraw.
///
/// Parameters: `max_area_ratio` (default 0.5), `which`.
#[cfg(feature = "std")]
fn eval_assert_damage_incremental(
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> AssertionResult {
    if let Some(bad) = reject_unknown_params(
        "assert_damage_incremental",
        params,
        &["max_area_ratio", "which", "frame"],
    ) {
        return bad;
    }
    let max_ratio = params
        .get("max_area_ratio")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.5);
    let (damage, which) = damage_of(params, callback_info);
    if damage.is_full() {
        return AssertionResult::fail_with(
            format!("assert_damage_incremental: {which} damage is a FULL repaint, not a patch"),
            "rects".to_string(),
            "full".to_string(),
        );
    }
    if damage.is_none() {
        return AssertionResult::fail_with(
            format!("assert_damage_incremental: {which} damage is empty — nothing was repainted"),
            "rects".to_string(),
            "none".to_string(),
        );
    }
    let area = window_logical_area(callback_info);
    let ratio = f64::from(damage.area(area) / area);
    if ratio > max_ratio {
        return AssertionResult::fail_with(
            format!("assert_damage_incremental: {which} repaint is not incremental enough"),
            format!("area <= {max_ratio:.4} of the window"),
            format!("{ratio:.4}"),
        );
    }
    AssertionResult::pass(format!(
        "assert_damage_incremental: {which} repaint is a patch ({} rect(s), {:.2}% of the window)",
        damage.rect_count(),
        ratio * 100.0
    ))
}

/// `assert_idle_stable` — the infinite-redraw detector.
///
/// With no input, the last frame must have produced NO damage. Optionally
/// (`vs`) the pixels must also be identical to a snapshot taken earlier.
///
/// Drive it as: `wait_frame` × K → `assert_idle_stable`.
///
/// Parameters: `vs` (snapshot name), `threshold` (per-channel pixel tolerance),
/// `max_frames` (how many frames the window may take to settle, counted from the
/// last `reset_frame_counters`).
#[cfg(feature = "std")]
fn eval_assert_idle_stable(
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> AssertionResult {
    if let Some(bad) = reject_unknown_params(
        "assert_idle_stable",
        params,
        &["vs", "threshold", "max_frames"],
    ) {
        return bad;
    }
    let report = frame_report_of(callback_info);

    // LIVENESS PRECONDITION. This is an assertion of ABSENCE ("no damage"), and
    // an assertion of absence passes for free when the machinery that would
    // produce the thing never ran at all. That is not hypothetical: before the
    // headless runner rendered any frame, `paint_damage` was permanently `None`
    // and `e2e/op-no-damage-when-idle.json` went green while proving nothing.
    // A frame must actually have been rendered since the last counter reset.
    if report.frames_since_reset == 0 {
        return AssertionResult::fail_with(
            "assert_idle_stable: NO FRAME was rendered since the last reset — 'no damage' is \
             vacuously true when nothing rendered. Drive a frame (tick_ms / wait_frame) first."
                .to_string(),
            "frames_since_reset >= 1".to_string(),
            "0".to_string(),
        );
    }

    if !report.paint_damage.is_none() {
        return AssertionResult::fail_with(
            "assert_idle_stable: an IDLE window still reports paint damage — it will re-render \
             (and burn CPU) forever"
                .to_string(),
            "paint damage = none".to_string(),
            format!(
                "{} ({} rect(s))",
                damage_kind_str(&report.paint_damage),
                report.paint_damage.rect_count()
            ),
        );
    }
    if !report.present_damage.is_none() {
        return AssertionResult::fail_with(
            "assert_idle_stable: an IDLE window still reports PRESENT damage".to_string(),
            "present damage = none".to_string(),
            damage_kind_str(&report.present_damage).to_string(),
        );
    }

    // "settles within N ticks" was unenforceable prose across 576 corpus lines:
    // the frame count was REPORTED in the pass message and never constrained,
    // so a scenario that took 20 frames satisfied a sentence that said 5, and an
    // engine that needed 20 passed a test that said it must settle in 5.
    // `frames_since_reset` counts the frames rendered since the last
    // `reset_frame_counters`, so put that reset immediately before the
    // interaction and this bounds the settle.
    if let Some(max_frames) = params.get("max_frames").and_then(serde_json::Value::as_u64) {
        if u64::from(report.frames_since_reset) > max_frames {
            return AssertionResult::fail_with(
                "assert_idle_stable: the window took more frames to settle than the test allows"
                    .to_string(),
                format!("<= {max_frames} frame(s) since the counter reset"),
                report.frames_since_reset.to_string(),
            );
        }
    }

    #[cfg(feature = "cpurender")]
    if let Some(vs) = params.get("vs").and_then(|v| v.as_str()) {
        let threshold = params
            .get("threshold")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u8;
        let before = match load_snapshot(vs, callback_info) {
            Ok(p) => p,
            Err(e) => return AssertionResult::fail(format!("assert_idle_stable: {e}")),
        };
        let after = match render_current(callback_info) {
            Ok(p) => p,
            Err(e) => return AssertionResult::fail(format!("assert_idle_stable: {e}")),
        };
        let diff = azul_layout::cpurender::pixel_diff(&before, &after, threshold);
        if !diff.dimensions_match || diff.diff_count > 0 {
            return AssertionResult::fail_with(
                "assert_idle_stable: frame N+1 differs from frame N with no input".to_string(),
                "identical pixels".to_string(),
                format!("{} px differ", diff.diff_count),
            );
        }
    }
    // Without `cpurender` the pixel half of this assertion cannot run. Say so
    // instead of silently dropping it: `assert_changed` and
    // `assert_damage_covers_changes` both FAIL in that configuration, and a
    // differential false-green (this one passing where they fail) is exactly the
    // silent-fallback bug class this suite exists to catch.
    #[cfg(not(feature = "cpurender"))]
    if params.get("vs").and_then(|v| v.as_str()).is_some() {
        return AssertionResult::fail(
            "assert_idle_stable: 'vs' requests a pixel comparison but the cpurender feature is \
             not enabled — refusing to pass while skipping the check",
        );
    }
    #[cfg(not(feature = "cpurender"))]
    let _ = params;

    AssertionResult::pass(format!(
        "assert_idle_stable: idle window is stable (damage drained to none over {} frame(s))",
        report.frames_since_reset
    ))
}

/// `assert_work_bounded` — the invalidation-loop detector.
///
/// Measures the frame-work counters SINCE THE LAST `reset_frame_counters`.
/// Today an invalidation loop just hits `MAX_EVENT_RECURSION_DEPTH` and is
/// swallowed with a `log_warn`; `hit_depth_cap` turns that into a red test.
///
/// THREE COUNTERS, THREE DIFFERENT QUESTIONS — see `FrameReport`:
/// * `max_relayouts` bounds `relayout_iterations`, the EVENT-pass depth. `0`
///   means "no state delta was processed at all". It does NOT mean "no layout
///   ran": a `set_node_*` mutation arrives through the callback API, never
///   enters `process_window_events`, and still re-lays-out the whole root DOM.
/// * `max_dom_regens` bounds `dom_regenerations`, i.e. full DOM rebuilds.
/// * `max_layout_passes` bounds `layout_passes`, i.e. how many times layout
///   ACTUALLY ran, whichever scheduler asked for it. This is the one to use for
///   "the engine schedules no relayout" over a callback-API mutation.
///
/// Every counter takes `max_`, `min_` and `exact_`. UPPER BOUNDS ALONE CANNOT
/// FAIL ON A DEAD ENGINE: `0` satisfies every `max_*` there is, so an engine
/// that dropped the interaction passed. Use `min_*` / `exact_*` to prove the
/// work DID happen.
///
/// Parameters: `max_relayouts` / `min_relayouts` / `exact_relayouts`,
/// `max_dom_regens` / `min_dom_regens` / `exact_dom_regens`,
/// `max_layout_passes` / `min_layout_passes` / `exact_layout_passes`,
/// `allow_depth_cap` (default false).
#[cfg(feature = "std")]
fn eval_assert_work_bounded(
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> AssertionResult {
    if let Some(bad) = reject_unknown_params(
        "assert_work_bounded",
        params,
        &[
            "allow_depth_cap",
            "max_relayouts",
            "min_relayouts",
            "exact_relayouts",
            "max_dom_regens",
            "min_dom_regens",
            "exact_dom_regens",
            "max_layout_passes",
            "min_layout_passes",
            "exact_layout_passes",
        ],
    ) {
        return bad;
    }

    // At least one BOUND is required. `allow_depth_cap` is deliberately not in
    // this list: it only relaxes a check, so a step carrying nothing but that
    // still asserts nothing.
    //
    // Without this, `{"op":"assert_work_bounded"}` passed on any window that
    // rendered a frame and did not hit the depth cap, while printing a
    // convincing "N event iteration(s), M DOM regen(s) ... depth cap not hit"
    // that constrains none of those numbers. assert_damage already guards this
    // way, with the same reasoning in its own message.
    const CONSTRAINTS: &[&str] = &[
        "max_relayouts",
        "min_relayouts",
        "exact_relayouts",
        "max_dom_regens",
        "min_dom_regens",
        "exact_dom_regens",
        "max_layout_passes",
        "min_layout_passes",
        "exact_layout_passes",
    ];
    if !CONSTRAINTS.iter().any(|c| params.get(*c).is_some()) {
        return AssertionResult::fail(format!(
            "assert_work_bounded: no bound given — at least one of {} is required, otherwise this \
             assertion passes unconditionally",
            CONSTRAINTS.join(", ")
        ));
    }

    let report = frame_report_of(callback_info);
    let relayouts = report.relayout_iterations;
    let regens = report.dom_regenerations;
    let layouts = report.layout_passes;
    let depth_cap = report.hit_depth_cap;

    // LIVENESS PRECONDITION — see `assert_idle_stable`. Upper bounds alone are
    // satisfied by "no work at all", including when the engine never ran a
    // frame. Require that a frame was actually produced since the last reset.
    if report.frames_since_reset == 0 {
        return AssertionResult::fail_with(
            "assert_work_bounded: NO FRAME was rendered since the last reset — every upper bound \
             is vacuously satisfied when no work happened at all"
                .to_string(),
            "frames_since_reset >= 1".to_string(),
            "0".to_string(),
        );
    }

    if depth_cap
        && !params
            .get("allow_depth_cap")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    {
        return AssertionResult::fail_with(
            "assert_work_bounded: MAX_EVENT_RECURSION_DEPTH was hit — the event did not converge \
             (invalidation loop)"
                .to_string(),
            "hit_depth_cap = false".to_string(),
            "true".to_string(),
        );
    }

    let num = |key: &str| params.get(key).and_then(serde_json::Value::as_u64);

    // ONE checker per counter, so a counter cannot grow a `max_` without the
    // matching `min_` / `exact_`. That asymmetry was the defect: with only
    // upper bounds, `0` satisfies EVERY bound there is, so an engine that
    // dropped the interaction entirely passed every bounded test in the corpus.
    let check = |what: &str,
                 actual: u32,
                 min: Option<u64>,
                 max: Option<u64>,
                 exact: Option<u64>|
     -> Option<AssertionResult> {
        let actual = u64::from(actual);
        if let Some(want) = exact {
            if actual != want {
                return Some(AssertionResult::fail_with(
                    format!("assert_work_bounded: wrong number of {what}"),
                    format!("exactly {want}"),
                    actual.to_string(),
                ));
            }
        }
        if let Some(want) = max {
            if actual > want {
                return Some(AssertionResult::fail_with(
                    format!("assert_work_bounded: too many {what}"),
                    format!("<= {want}"),
                    actual.to_string(),
                ));
            }
        }
        if let Some(want) = min {
            if actual < want {
                return Some(AssertionResult::fail_with(
                    format!(
                        "assert_work_bounded: too FEW {what} — the step did less work than the \
                         test requires, which is exactly what an engine that dropped the \
                         interaction looks like"
                    ),
                    format!(">= {want}"),
                    actual.to_string(),
                ));
            }
        }
        None
    };

    let failure = [
        check(
            "event-processing iterations (relayout_iterations)",
            relayouts,
            num("min_relayouts"),
            num("max_relayouts"),
            num("exact_relayouts"),
        ),
        check(
            "DOM regenerations",
            regens,
            num("min_dom_regens"),
            num("max_dom_regens"),
            num("exact_dom_regens"),
        ),
        check(
            "layout passes (the counter `max_relayouts` cannot see — a callback-API mutation \
             relayouts without ever running an event pass)",
            layouts,
            num("min_layout_passes"),
            num("max_layout_passes"),
            num("exact_layout_passes"),
        ),
    ]
    .into_iter()
    .flatten()
    .next();
    if let Some(fail) = failure {
        return fail;
    }

    AssertionResult::pass(format!(
        "assert_work_bounded: {relayouts} event iteration(s), {regens} DOM regen(s), {layouts} \
         layout pass(es) over {} frame(s), depth cap not hit",
        report.frames_since_reset
    ))
}

/// `assert_resource_counts` — the leak detector.
///
/// Compares the resource counters against a named `snapshot_resources`.
/// Each counter is given a mode: `"eq"` (must return to the baseline), `"le"`
/// (must not grow), `"ge"`, or an explicit number.
///
/// ```json
/// { "op": "assert_resource_counts", "vs": "baseline",
///   "images": "eq", "fonts": "eq", "parsed_fonts": "eq" }
/// ```
/// Known counters: `fonts`, `font_hash_map`, `font_id_map`, `font_families_map`,
/// `images`, `image_key_map`, `parsed_fonts`, `font_hash_to_families`,
/// `font_chain_cache`.
#[cfg(feature = "std")]
fn eval_assert_resource_counts(
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> AssertionResult {
    let current = collect_resource_counts(callback_info);
    let baseline = match params.get("vs").and_then(|v| v.as_str()) {
        Some(name) => match scratch(callback_info).resource_snapshots.get(name).cloned() {
            Some(b) => Some(b),
            None => {
                return AssertionResult::fail(format!(
                    "assert_resource_counts: no resource snapshot named '{name}' (use \
                     snapshot_resources first)"
                ))
            }
        },
        None => None,
    };

    let Some(obj) = params.as_object() else {
        return AssertionResult::fail("assert_resource_counts: params must be an object");
    };

    let mut checked = 0usize;
    for (key, want) in obj {
        if key == "op" || key == "vs" || key == "screenshot" {
            continue;
        }
        let Some(&actual) = current.get(key.as_str()) else {
            return AssertionResult::fail(format!(
                "assert_resource_counts: unknown counter '{key}' (known: {})",
                current.keys().cloned().collect::<Vec<_>>().join(", ")
            ));
        };
        checked += 1;

        if let Some(expected) = want.as_u64() {
            if actual != expected {
                return AssertionResult::fail_with(
                    format!("assert_resource_counts: '{key}' mismatch"),
                    expected.to_string(),
                    actual.to_string(),
                );
            }
            continue;
        }
        let Some(mode) = want.as_str() else {
            return AssertionResult::fail(format!(
                "assert_resource_counts: '{key}' must be a number or one of \"eq\"/\"le\"/\"ge\""
            ));
        };
        let Some(base) = baseline.as_ref().and_then(|b| b.get(key.as_str()).copied()) else {
            return AssertionResult::fail(format!(
                "assert_resource_counts: '{key}': \"{mode}\" needs a 'vs' baseline snapshot"
            ));
        };
        let ok = match mode {
            "eq" => actual == base,
            "le" => actual <= base,
            "ge" => actual >= base,
            // STRICT modes. Without them a leak test can only assert "came back
            // to baseline", which is vacuously true when the resource was never
            // acquired in the first place — exactly how the font-leak scenario
            // managed to pass while the font tables were frozen. "gt" lets a test
            // assert the LIVENESS half ("the font really was loaded") so the
            // leak half cannot go green for the wrong reason.
            "gt" => actual > base,
            "lt" => actual < base,
            other => {
                return AssertionResult::fail(format!(
                    "assert_resource_counts: unknown mode '{other}' for '{key}' (known: eq, le, \
                     ge, gt, lt)"
                ))
            }
        };
        if !ok {
            return AssertionResult::fail_with(
                format!("assert_resource_counts: '{key}' leaked / drifted from the baseline"),
                format!("{mode} {base}"),
                actual.to_string(),
            );
        }
    }

    if checked == 0 {
        return AssertionResult::fail(
            "assert_resource_counts: no counters given (e.g. \"fonts\": \"eq\")",
        );
    }
    AssertionResult::pass(format!(
        "assert_resource_counts: {checked} counter(s) within bounds ({})",
        current
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(" ")
    ))
}

// ==================== Manager / composition / damage-soundness assertions ====

// Four assertions over manager, composition and damage soundness. They read
// manager state off
// `LayoutWindow` through `CallbackInfo::get_layout_window()` — the same seam
// every other assertion in this file uses — plus, for `assert_composition`, a
// per-step sample recorded by the step loop in `resume_e2e_continuation`.
//
// ALL TEN of the plan's cross-invariants are implemented. Four of them (X1, X4,
// X7, X8) were previously refused with a reason, because each needs something
// the engine applies and forgets. What they needed:
//
// * X1 — WHICH node was passed to `scroll_into_view`. Recorded by the op itself
//   (`E2eScratch::last_scroll_into_view`); everything else is re-derived live.
// * X4 — the second `Option<DragContext>` this pair named was deleted in
//   2026-07. The seam survives as `DragState::from_context`, a derived view that
//   every reader of the public drag API sees, so the invariant becomes "the view
//   must agree with its source".
// * X7 — an issued `ScrollAdjustment` is still not retained, and that half stays
//   unobservable. What IS retained is the marker that says a cursor scroll is
//   OWED, which catches the same bug one step earlier. The narrowing is spelled
//   out at the check.
// * X8 — frame-to-frame history, now that the composition trace keeps two
//   samples and records per-container scroll offsets.
//
// They are OPT-IN, not in `DEFAULT_CROSS`: each is a statement about an
// interaction, and each HARD-FAILS when the scenario did not perform that
// interaction. A stub that always passes manufactures false-green, which is
// strictly worse than no assertion at all — and so does an invariant that
// quietly holds because its premise was empty.

/// Does `(dom, node)` name a node that exists in the CURRENT layout?
#[cfg(feature = "std")]
fn node_is_live(
    lw: &azul_layout::window::LayoutWindow,
    dom: azul_core::dom::DomId,
    node: azul_core::dom::NodeId,
) -> bool {
    lw.layout_results
        .get(&dom)
        .is_some_and(|lr| node.index() < lr.styled_dom.node_data.as_ref().len())
}

/// `DomNodeId` form of [`node_is_live`]. A `NONE` node id names the DOM itself,
/// which is live iff that DOM is still laid out.
#[cfg(feature = "std")]
fn dom_node_is_live(lw: &azul_layout::window::LayoutWindow, id: azul_core::dom::DomNodeId) -> bool {
    id.node.into_crate_internal().map_or_else(
        || lw.layout_results.contains_key(&id.dom),
        |n| node_is_live(lw, id.dom, n),
    )
}

/// Human name of the active drag kind, for diagnostics.
#[cfg(feature = "std")]
fn drag_kind_str(d: &azul_core::drag::DragContext) -> &'static str {
    use azul_core::drag::ActiveDragType;
    match d.drag_type {
        ActiveDragType::TextSelection(_) => "text-selection",
        ActiveDragType::ScrollbarThumb(_) => "scrollbar-thumb",
        ActiveDragType::Node(_) => "node",
        ActiveDragType::WindowMove(_) => "window-move",
        ActiveDragType::WindowResize(_) => "window-resize",
        ActiveDragType::FileDrop(_) => "file-drop",
    }
}

/// The `(DomId, NodeId)` an active drag hangs off, if it has one. Window
/// move/resize and file drops are not node-anchored.
#[cfg(feature = "std")]
fn drag_source_node(
    d: &azul_core::drag::DragContext,
) -> Option<(azul_core::dom::DomId, azul_core::dom::NodeId)> {
    use azul_core::drag::ActiveDragType;
    match &d.drag_type {
        ActiveDragType::TextSelection(t) => Some((t.dom_id, t.anchor_ifc_node)),
        ActiveDragType::ScrollbarThumb(s) => Some((s.dom_id, s.scroll_container_node)),
        ActiveDragType::Node(n) => Some((n.dom_id, n.node_id)),
        ActiveDragType::WindowMove(_)
        | ActiveDragType::WindowResize(_)
        | ActiveDragType::FileDrop(_) => None,
    }
}

/// Total selected span across a multi-cursor state, used by
/// `assert_composition`'s `selection_grew` stage. Bare cursors count 0; a range
/// counts its byte span, with run crossings weighted so that "the selection
/// swallowed another run" is always larger than any within-run growth.
#[cfg(feature = "std")]
fn multi_cursor_span(mc: &azul_core::selection::MultiCursorState) -> u64 {
    use azul_core::selection::Selection;
    mc.selections
        .iter()
        .map(|s| match s.selection {
            Selection::Cursor(_) => 0u64,
            Selection::Range(r) => {
                let runs = u64::from(
                    r.end
                        .cluster_id
                        .source_run
                        .abs_diff(r.start.cluster_id.source_run),
                );
                let bytes = u64::from(
                    r.end
                        .cluster_id
                        .start_byte_in_run
                        .abs_diff(r.start.cluster_id.start_byte_in_run),
                );
                runs * 1_000_000 + bytes
            }
        })
        .sum()
}

/// The (g3) state-machine sweep, shared by `assert_state_machines_idle` and the
/// fixpoint half of `assert_composition`.
///
/// Returns ONE LINE PER LEAKED STATE MACHINE — an empty vec means everything
/// settled. Every entry is a real read of a real field; there is no "unknown =
/// ok" branch.
#[cfg(feature = "std")]
fn collect_state_machine_leaks(
    callback_info: &azul_layout::callbacks::CallbackInfo,
    check_damage: bool,
) -> Vec<String> {
    let lw = callback_info.get_layout_window();
    let mut leaks: Vec<String> = Vec::new();

    // -- drag ended, drag still active --------------------------------------
    if let Some(drag) = lw.gesture_drag_manager.active_drag.as_ref() {
        leaks.push(format!(
            "gesture_drag_manager.active_drag is still Some ({} drag, session {}, cancelled={})",
            drag_kind_str(drag),
            drag.session_id,
            drag.cancelled
        ));
    }
    let unended = lw
        .gesture_drag_manager
        .input_sessions
        .iter()
        .filter(|s| !s.ended)
        .count();
    if unended > 0 {
        leaks.push(format!(
            "gesture_drag_manager has {unended} un-ended input session(s) of {} (end_current_session \
             / clear_old_sessions never ran)",
            lw.gesture_drag_manager.input_sessions.len()
        ));
    }

    // -- animation finished, still scheduling frames -------------------------
    if lw.scroll_manager.has_active_animations() {
        leaks.push(format!(
            "scroll_manager still has {} active scroll animation(s) — the platform loop keeps \
             generating frames",
            lw.scroll_manager.animating_keys().len()
        ));
    }
    if lw.scroll_manager.has_pending_scroll_changes() {
        leaks.push(
            "scroll_manager.scroll_dirty is still set — the display list will be rebuilt again"
                .to_string(),
        );
    }
    if lw.gpu_state_manager.scrollbar_fade_active {
        leaks.push(
            "gpu_state_manager.scrollbar_fade_active is still true — this is what kept an idle \
             scrollbar'd window re-presenting forever"
                .to_string(),
        );
    }

    // -- selection cleared, listeners still armed ---------------------------
    if lw.text_edit_manager.display_list_dirty {
        leaks.push(
            "text_edit_manager.display_list_dirty stayed latched — a permanently dirty flag is a \
             permanent repaint"
                .to_string(),
        );
    }
    if lw.text_edit_manager.multi_cursor.is_none() && lw.text_edit_manager.blink.blink_timer_active
    {
        leaks.push(
            "text_edit_manager.blink.blink_timer_active is true with no multi_cursor — the caret \
             blink outlived the editor"
                .to_string(),
        );
    }

    // -- focus requests that were never resolved ----------------------------
    if lw.focus_manager.pending_focus_request.is_some() {
        leaks.push(
            "focus_manager.pending_focus_request is still Some — a focus change was queued and \
             never finalized"
                .to_string(),
        );
    }
    if lw.focus_manager.pending_contenteditable_focus.is_some() {
        // The retry count is in the MESSAGE, not in the condition.
        // `finalize_pending_focus_changes` may put the request back up to
        // `MAX_PENDING_FOCUS_RETRIES` times when the node has no entry in
        // `dom_to_layout` yet (`FocusManager::rearm_pending_contenteditable_focus`),
        // but a re-arm that is still in flight at scenario end is a caret that
        // was never seeded — which is precisely what this law is for. Naming the
        // count only keeps a bounded re-arm from being misread as "finalize
        // never ran".
        leaks.push(format!(
            "focus_manager.pending_contenteditable_focus is still Some — contenteditable focus was \
             queued and never finalized (re-armed {} time(s) for want of a text layout)",
            lw.focus_manager.pending_focus_retries
        ));
    }
    if lw.focus_manager.deferred_focus_target.is_some() {
        leaks.push(
            "focus_manager.deferred_focus_target is still Some — a focus request waited for a \
             layout that never came"
                .to_string(),
        );
    }

    // -- and the frame itself must have drained ------------------------------
    if check_damage {
        let report = lw.frame_report_synced();
        if !report.paint_damage.is_none() {
            leaks.push(format!(
                "the last frame still reports PAINT damage ({}, {} rect(s)) — FrameDamage::None was \
                 never reached",
                damage_kind_str(&report.paint_damage),
                report.paint_damage.rect_count()
            ));
        }
        if !report.present_damage.is_none() {
            leaks.push(format!(
                "the last frame still reports PRESENT damage ({})",
                damage_kind_str(&report.present_damage)
            ));
        }
    }

    leaks
}

/// `assert_state_machines_idle` — E2E_PLAN §(g3), "it ended, but the manager
/// didn't notice".
///
/// The interaction is over; assert that every state machine noticed. Checks, in
/// one sweep: no active drag, no un-ended gesture session, no active scroll
/// animation, `scroll_dirty` cleared, `scrollbar_fade_active` cleared, no
/// latched `display_list_dirty`, no orphan caret blink, no unresolved focus
/// request, and (unless `damage: false`) `FrameDamage::None` on the last frame.
///
/// Parameters: `damage` (bool, default `true`) — include the
/// `FrameDamage::None` requirement.
#[cfg(feature = "std")]
fn eval_assert_state_machines_idle(
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> AssertionResult {
    if let Some(bad) = reject_unknown_params("assert_state_machines_idle", params, &["damage"]) {
        return bad;
    }

    // LIVENESS PRECONDITION, the same one assert_idle_stable and
    // assert_work_bounded already carry: this is an assertion of ABSENCE
    // ("nothing is mid-flight"), and an assertion of absence passes for free when
    // the machinery that would produce the thing never ran. On a freshly mounted
    // window where no drag, gesture, scroll, focus or edit ever happened, every
    // state machine is trivially settled and this reported "every state machine
    // settled" — true, and worth nothing.
    //
    // The contract this pins is the useful reading: "an interaction ENDED
    // cleanly", not "nothing has happened yet". Requiring a rendered frame is
    // the weakest check that separates the two, and it matches what the sibling
    // assertions demand, so a scenario cannot pass here by doing less.
    let report = frame_report_of(callback_info);
    if report.frames_since_reset == 0 {
        return AssertionResult::fail_with(
            "assert_state_machines_idle: NO FRAME was rendered since the last reset — every state \
             machine is vacuously settled when nothing ever drove one. Perform the interaction and \
             drive a frame (tick_ms / wait_frame) first."
                .to_string(),
            "frames_since_reset >= 1".to_string(),
            "0".to_string(),
        );
    }

    let check_damage = params
        .get("damage")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);

    let leaks = collect_state_machine_leaks(callback_info, check_damage);
    if leaks.is_empty() {
        return AssertionResult::pass(format!(
            "assert_state_machines_idle: every state machine settled (drag, gesture sessions, \
             scroll animation, scroll_dirty, scrollbar fade, text-edit dirty/blink, focus \
             requests{})",
            if check_damage { ", frame damage" } else { "" }
        ));
    }
    AssertionResult::fail_with(
        format!(
            "assert_state_machines_idle: {} state machine(s) did not settle after the interaction \
             ended",
            leaks.len()
        ),
        "every state machine idle".to_string(),
        leaks.join("; "),
    )
}

/// `assert_manager_invariants` — E2E_PLAN §(g2), the cross-manager consistency
/// table.
///
/// Parameters:
/// * `managers` — which managers to sweep for dangling keys (X10). Default: all
///   supported. Known: `scroll`, `hover`, `focus`, `gesture`, `selection`,
///   `text_edit`, `virtual_view`, `undo_redo`.
/// * `cross` — which pairwise invariants to check. Default:
///   `["X2","X3","X5","X6","X9","X10"]`, the six that hold unconditionally.
///   `X1`, `X4`, `X7` and `X8` are implemented but OPT-IN, because each one is a
///   statement about an interaction that must actually have happened; requesting
///   one without performing that interaction fails loudly rather than passing on
///   an empty premise. See the module comment above.
#[cfg(feature = "std")]
#[allow(clippy::too_many_lines)]
const KNOWN_MANAGERS: &[&str] = &[
    "scroll",
    "hover",
    "focus",
    "gesture",
    "selection",
    "text_edit",
    "virtual_view",
    "undo_redo",
    "gpu_state",
    "text_input",
    "permission",
];

const UNOBSERVABLE_MANAGERS: &[(&str, &str)] = &[
    (
        "a11y_snapshot",
        "pure projection — `A11ySnapshot::build()` is called on demand from `LayoutWindow::build_a11y_snapshot(&self)` and the result is handed straight to the platform bridge; nothing is stored on the window between calls. There is no state that could latch, so there is no invariant to assert. The a11y state that CAN latch lives in `a11y_manager`, which is checked",
    ),
    (
        "scroll_into_view",
        "stateless — free functions that take options and return ScrollAdjustments, retaining \
         nothing between calls",
    ),
    (
        "scroll_registration",
        "stateless — one free function that reads the finished layout and publishes each \
         scrollable node into ScrollManager. It owns no field on LayoutWindow, so nothing can \
         latch in it; everything it writes is asserted as `scroll`",
    ),
    (
        "drag_drop",
        "no LayoutWindow field exists; the live drag lives in gesture_drag_manager, which IS \
         covered",
    ),
    (
        "changeset",
        "not a manager — a text changeset is recorded in TextInputManager, which IS covered",
    ),
    (
        "clipboard",
        "the manager exposes no fields and holds no node-keyed state",
    ),
    ("file_drop", "no observable state, same as clipboard"),
    (
        "gamepad",
        "no observable state, and no host device exists in a headless run",
    ),
    ("biometric", "host capability with no headless backend"),
    ("geolocation", "host capability with no headless backend"),
    ("keyring", "host capability with no headless backend"),
    ("sensors", "host capability with no headless backend"),
    (
        "eyedropper",
        "host capability (a native screen colour-sampler) with no headless backend — its \
         issued/last_result/pending_event never populate in a headless run, so there is no \
         latch to assert. The state it CAN hold is fingerprinted as `eyedropper`",
    ),
    (
        "a11y",
        "HAS state (A11yManager.tree) and IS a LayoutWindow field, so this one is a real gap, \
         not an impossibility: proving a tree node still maps to a live DOM node needs an \
         A11yNodeId -> NodeId walk that does not exist here yet",
    ),
];

fn eval_assert_manager_invariants(
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> AssertionResult {
    if let Some(bad) = reject_unknown_params(
        "assert_manager_invariants",
        params,
        &["managers", "cross", "min_checked"],
    ) {
        return bad;
    }
    /// Every invariant a scenario may request.
    const KNOWN_CROSS: &[&str] = &["X1", "X2", "X3", "X4", "X5", "X6", "X7", "X8", "X9", "X10"];
    /// The invariants checked when a scenario does not say which.
    ///
    /// These six hold UNCONDITIONALLY — on a blank window, on a mounted one, at
    /// any point in any scenario — so running them by default costs a scenario
    /// nothing and catches everything.
    ///
    /// X1, X4, X7 and X8 are deliberately NOT here. Each one is a statement
    /// ABOUT an interaction ("after a `scroll_into_view` …", "during a text
    /// selection drag …"), so on a window where that interaction never happened
    /// it has no subject. They are opt-in and they HARD-FAIL when their subject
    /// is missing, instead of passing on an empty premise — a scenario that asks
    /// for X4 without a live drag has not checked X4, and must be told so.
    const DEFAULT_CROSS: &[&str] = &["X2", "X3", "X5", "X6", "X9", "X10"];
    /// Invariants this crate cannot check, each with the reason. Requesting one
    /// is a hard failure, so no test can go green on an assertion nobody wrote.
    ///
    /// EMPTY as of the X1/X4/X7/X8 implementation — all ten of the plan's
    /// cross-invariants are now real. The mechanism stays because the next
    /// invariant somebody sketches must land here, not in silence.
    const UNIMPLEMENTED_CROSS: &[(&str, &str)] = &[];

    // Managers this assertion does NOT check, each with the reason. Recorded
    // rather than omitted: gpu_state was simply absent from KNOWN_MANAGERS, and
    // that silence is exactly how the scrollbar-fade latch stayed invisible to
    // every invariant in this file. A reader comparing this list against
    // layout/src/managers/ must be able to account for all 22.
    //
    // Naming one of these in a scenario is a hard failure with its reason
    // attached, so nobody can go green believing they asserted something here.

    let list = |key: &str, default: &[&str]| -> Result<Vec<String>, String> {
        match params.get(key) {
            None => Ok(default.iter().map(|s| (*s).to_string()).collect()),
            Some(serde_json::Value::Array(a)) => {
                let mut out = Vec::new();
                for v in a {
                    match v.as_str() {
                        Some(s) => out.push(s.to_string()),
                        None => return Err(format!("'{key}' must be an array of strings")),
                    }
                }
                Ok(out)
            }
            Some(_) => Err(format!("'{key}' must be an array of strings")),
        }
    };

    let managers = match list("managers", KNOWN_MANAGERS) {
        Ok(m) => m,
        Err(e) => return AssertionResult::fail(format!("assert_manager_invariants: {e}")),
    };
    let cross = match list("cross", DEFAULT_CROSS) {
        Ok(c) => c,
        Err(e) => return AssertionResult::fail(format!("assert_manager_invariants: {e}")),
    };

    for m in &managers {
        if let Some((_, why)) = UNOBSERVABLE_MANAGERS.iter().find(|(n, _)| n == m) {
            return AssertionResult::fail(format!(
                "assert_manager_invariants: manager '{m}' is NOT checked here and will not be \
                 silently passed — {why}"
            ));
        }
        if !KNOWN_MANAGERS.contains(&m.as_str()) {
            return AssertionResult::fail(format!(
                "assert_manager_invariants: manager '{m}' has no key surface this assertion can \
                 read (known: {}). Refusing to pass an unchecked manager.",
                KNOWN_MANAGERS.join(", ")
            ));
        }
    }
    for c in &cross {
        if let Some((_, why)) = UNIMPLEMENTED_CROSS.iter().find(|(n, _)| n == c) {
            return AssertionResult::fail(format!(
                "assert_manager_invariants: invariant {c} is NOT IMPLEMENTED and will not be \
                 silently passed — {why}"
            ));
        }
        if !KNOWN_CROSS.contains(&c.as_str()) {
            return AssertionResult::fail(format!(
                "assert_manager_invariants: unknown invariant '{c}' (known: {})",
                KNOWN_CROSS.join(", ")
            ));
        }
    }

    let lw = callback_info.get_layout_window();
    let mut violations: Vec<String> = Vec::new();
    let mut checked = 0usize;
    let want_x10 = cross.iter().any(|c| c == "X10");

    // ---- X10: no manager key may refer to a node that no longer exists -----
    if want_x10 {
        for m in &managers {
            match m.as_str() {
                "scroll" => {
                    for (dom, node) in lw.scroll_manager.state_keys() {
                        checked += 1;
                        if !node_is_live(lw, dom, node) {
                            violations.push(format!(
                                "X10 scroll: state key ({}, {}) points at a node that no longer \
                                 exists",
                                dom.inner,
                                node.index()
                            ));
                        }
                    }
                }
                "hover" => {
                    for point in lw.hover_manager.get_active_input_points() {
                        let Some(hit) = lw.hover_manager.get_current(&point) else {
                            continue;
                        };
                        for (dom, ht) in &hit.hovered_nodes {
                            let live_dom = lw.layout_results.contains_key(dom);
                            let ids = ht
                                .regular_hit_test_nodes
                                .keys()
                                .chain(ht.scroll_hit_test_nodes.keys())
                                .chain(ht.cursor_hit_test_nodes.keys());
                            for nid in ids {
                                checked += 1;
                                if !live_dom || !node_is_live(lw, *dom, *nid) {
                                    violations.push(format!(
                                        "X10 hover: hit-test history holds ({}, {}) which no longer \
                                         exists",
                                        dom.inner,
                                        nid.index()
                                    ));
                                }
                            }
                        }
                    }
                }
                "focus" => {
                    if let Some(f) = lw.focus_manager.focused_node {
                        checked += 1;
                        if !dom_node_is_live(lw, f) {
                            violations.push(format!(
                                "X10 focus: focused_node ({}, {:?}) points at a dead node",
                                f.dom.inner,
                                f.node.into_crate_internal().map(|n| n.index())
                            ));
                        }
                    }
                }
                // The manager the scrollbar-fade latch lived in, and the reason
                // this list is not allowed to quietly omit anything: gpu_state was
                // outside KNOWN_MANAGERS, so no invariant here could see it, and
                // the bug was caught only incidentally by a hard-coded field read
                // in assert_state_machines_idle.
                //
                // Every value in a GpuValueCache is keyed by the node it animates.
                // A key that outlives its node is a GPU resource nothing will ever
                // update or release — it is invisible in the DOM and invisible in
                // the display list, which is exactly why it needs an assertion.
                "gpu_state" => {
                    for (dom, cache) in &lw.gpu_state_manager.caches {
                        checked += 1;
                        if !lw.layout_results.contains_key(dom) {
                            violations.push(format!(
                                "X10 gpu_state: a GPU value cache is still held for DOM {} which no \
                                 longer exists",
                                dom.inner
                            ));
                            // Its per-node keys cannot be judged against a DOM
                            // that is already gone; the cache itself is the defect.
                            continue;
                        }
                        // Node-keyed animated values, all within this cache's DOM.
                        let node_keyed = cache
                            .transform_keys
                            .keys()
                            .chain(cache.current_transform_values.keys())
                            .chain(cache.h_transform_keys.keys())
                            .chain(cache.h_current_transform_values.keys())
                            .chain(cache.css_transform_keys.keys())
                            .chain(cache.css_current_transform_values.keys())
                            .chain(cache.opacity_keys.keys())
                            .chain(cache.current_opacity_values.keys());
                        for nid in node_keyed {
                            checked += 1;
                            if !node_is_live(lw, *dom, *nid) {
                                violations.push(format!(
                                    "X10 gpu_state: DOM {} holds a GPU value keyed to node {}, which \
                                     no longer exists",
                                    dom.inner,
                                    nid.index()
                                ));
                            }
                        }
                        // Scrollbar opacity carries its OWN (DomId, NodeId) — the
                        // scrolled container need not live in this cache's DOM.
                        for (kdom, knid) in cache
                            .scrollbar_v_opacity_keys
                            .keys()
                            .chain(cache.scrollbar_h_opacity_keys.keys())
                        {
                            checked += 1;
                            if !node_is_live(lw, *kdom, *knid) {
                                violations.push(format!(
                                    "X10 gpu_state: a scrollbar opacity key is held for ({}, {}), \
                                     which no longer exists",
                                    kdom.inner,
                                    knid.index()
                                ));
                            }
                        }
                    }
                }
                // A text edit staged for a node that has since been deleted will
                // either be applied to whatever now occupies that id, or dropped
                // silently. Both are wrong, and neither shows up in the DOM.
                "text_input" => {
                    for queued in lw.text_input_manager.pending_changesets.iter() {
                        checked += 1;
                        if !dom_node_is_live(lw, queued.edit.node) {
                            violations.push(format!(
                                "X10 text_input: a pending text edit is staged for ({}, {:?}), which \
                                 no longer exists",
                                queued.edit.node.dom.inner,
                                queued.edit.node.node.into_crate_internal().map(|n| n.index())
                            ));
                        }
                    }
                }
                // Capability subscriptions are refcounted, so a subscriber that
                // goes away without releasing pins the capability on forever —
                // a permission prompt the user can never get rid of.
                "permission" => {
                    for (cap, entry) in &lw.permission_manager.statuses {
                        if let Some(sub) = entry.last_subscriber {
                            checked += 1;
                            if !dom_node_is_live(lw, sub) {
                                violations.push(format!(
                                    "X10 permission: capability {cap:?} is still subscribed by ({}, \
                                     {:?}) (refcount {}), which no longer exists",
                                    sub.dom.inner,
                                    sub.node.into_crate_internal().map(|n| n.index()),
                                    entry.refcount
                                ));
                            }
                        }
                    }
                }
                "gesture" => {
                    if let Some(drag) = lw.gesture_drag_manager.active_drag.as_ref() {
                        if let Some((dom, node)) = drag_source_node(drag) {
                            checked += 1;
                            if !node_is_live(lw, dom, node) {
                                violations.push(format!(
                                    "X10 gesture: the active {} drag is anchored on ({}, {}), which \
                                     no longer exists",
                                    drag_kind_str(drag),
                                    dom.inner,
                                    node.index()
                                ));
                            }
                        }
                    }
                }
                "selection" | "text_edit" => {
                    if let Some(mc) = lw.text_edit_manager.multi_cursor.as_ref() {
                        checked += 1;
                        if !dom_node_is_live(lw, mc.node_id) {
                            violations.push(format!(
                                "X10 {m}: multi_cursor is anchored on ({}, {:?}), which no longer \
                                 exists",
                                mc.node_id.dom.inner,
                                mc.node_id.node.into_crate_internal().map(|n| n.index())
                            ));
                        }
                    }
                }
                "virtual_view" => {
                    for (dom, node) in lw.virtual_view_manager.all_view_keys() {
                        checked += 1;
                        if !node_is_live(lw, dom, node) {
                            violations.push(format!(
                                "X10 virtual_view: state key ({}, {}) points at a node that no \
                                 longer exists (virtual_view is NOT in \
                                 update_managers_with_node_moves)",
                                dom.inner,
                                node.index()
                            ));
                        }
                    }
                }
                "undo_redo" => {
                    for stack in &lw.undo_redo_manager.node_stacks {
                        checked += 1;
                        if !node_is_live(lw, ROOT_DOM_ID, stack.node_id) {
                            violations.push(format!(
                                "X10 undo_redo: node_stacks holds node {} which no longer exists \
                                 (undo_redo is NOT in update_managers_with_node_moves)",
                                stack.node_id.index()
                            ));
                        }
                    }
                }
                _ => unreachable!("manager names are validated above"),
            }
        }
    }

    // ---- X2: has_active_animations() ⟺ some state carries an animation -----
    if cross.iter().any(|c| c == "X2") {
        checked += 1;
        let flag = lw.scroll_manager.has_active_animations();
        let keys = lw.scroll_manager.animating_keys();
        if flag != !keys.is_empty() {
            violations.push(format!(
                "X2 scroll: has_active_animations() = {flag} but {} state(s) carry an animation",
                keys.len()
            ));
        }
    }

    // ---- X3: an active node/text drag agrees with the hit-test history -----
    if cross.iter().any(|c| c == "X3") {
        if let Some(drag) = lw.gesture_drag_manager.active_drag.as_ref() {
            if let Some((dom, node)) = drag_source_node(drag) {
                checked += 1;
                if !node_is_live(lw, dom, node) {
                    violations.push(format!(
                        "X3: the active {} drag says ({}, {}) but the hit-test DOM has no such node",
                        drag_kind_str(drag),
                        dom.inner,
                        node.index()
                    ));
                }
                if let Some(hover) = lw.hover_manager.current_hover_node_full() {
                    checked += 1;
                    if !dom_node_is_live(lw, hover) {
                        violations.push(format!(
                            "X3: hover_manager.current_hover_node_full() = ({}, {:?}) resolves \
                             against a DOM that no longer has it, while a {} drag is active",
                            hover.dom.inner,
                            hover.node.into_crate_internal().map(|n| n.index()),
                            drag_kind_str(drag)
                        ));
                    }
                }
            }
        }
    }

    // ---- X5: the selection anchor's node exists for the whole drag ---------
    if cross.iter().any(|c| c == "X5") {
        if let Some(mc) = lw.text_edit_manager.multi_cursor.as_ref() {
            checked += 1;
            if !dom_node_is_live(lw, mc.node_id) {
                violations.push(
                    "X5: the multi-cursor anchor node was removed but the selection was not \
                     cleared (remap_node_ids must DROP a selection whose node vanished)"
                        .to_string(),
                );
            }
        }
    }

    // ---- X6: multi_cursor ⇒ focus is set, live and the same node -----------
    if cross.iter().any(|c| c == "X6") {
        if let Some(mc) = lw.text_edit_manager.multi_cursor.as_ref() {
            checked += 1;
            match lw.focus_manager.focused_node {
                None => violations.push(
                    "X6: text_edit_manager.multi_cursor is Some but focus_manager.focused_node is \
                     None — blur must clear both"
                        .to_string(),
                ),
                Some(f) if !dom_node_is_live(lw, f) => violations.push(
                    "X6: multi_cursor is Some and focus points at a node that no longer exists"
                        .to_string(),
                ),
                Some(_) => {}
            }
        }
    }

    // ---- X9: a scrollbar fade needs a scroll node to fade ------------------
    if cross.iter().any(|c| c == "X9") {
        checked += 1;
        if lw.gpu_state_manager.scrollbar_fade_active && lw.scroll_manager.state_keys().is_empty() {
            violations.push(
                "X9: gpu_state_manager.scrollbar_fade_active is true with NO registered scroll \
                 node — the flag keeps the platform loop generating frames for a scrollbar that \
                 does not exist"
                    .to_string(),
            );
        }
    }

    // ---- X1: after a scroll_into_view, the target IS inside the container ---
    //
    // `scroll_into_view` is stateless: it computes `ScrollAdjustment`s, writes
    // them into `ScrollManager` and forgets both. So X1's subject — WHICH node
    // was asked for — has to come from the harness's own record of what it
    // requested (`E2eScratch::last_scroll_into_view`), not from engine state.
    // Everything else is re-derived from live state, which is the point: the
    // check asks `ScrollManager` where the container ended up and asks the
    // layout where the target is, and the two must agree.
    if cross.iter().any(|c| c == "X1") {
        checked += 1;
        // Bind first: the guard must not still be held while the check below
        // walks the window.
        let requested = scratch(callback_info).last_scroll_into_view;
        match requested {
            None => violations.push(
                "X1 was requested but NO scroll_into_view op ran in this scenario. The invariant \
                 is about where a scroll_into_view landed; with nothing to land, asking for it \
                 proves nothing."
                    .to_string(),
            ),
            Some((tdom, tnode)) => {
                violations.extend(check_x1_target_is_visible(lw, tdom, tnode, &mut checked));
            }
        }
    }

    // ---- X4: the two representations of one drag must agree ----------------
    //
    // The plan wrote X4 as `gesture.active_drag` vs the SECOND
    // `Option<DragContext>` that `DragDropManager` used to hold. That mirror was
    // write-only and unremapped, so it was deleted in 2026-07 — the plan's own
    // instruction ("if they routinely disagree, delete the deprecated one — that
    // is a finding") was carried out.
    //
    // The seam did NOT go away: `DragState::from_context` is still a SECOND
    // representation of the same drag, and it is the one the public
    // `get_drag_state` API and every drag callback see. A stored mirror became a
    // derived view, so the invariant becomes "the derived view must agree with
    // its source" — same pair, same failure (the API says a different node, or
    // no drag, than the engine has), and now it cannot drift silently.
    if cross.iter().any(|c| c == "X4") {
        checked += 1;
        match lw.gesture_drag_manager.active_drag.as_ref() {
            None => violations.push(
                "X4 was requested but NO drag is active. The invariant compares the live \
                 DragContext against the DragState view built from it; with no drag there are no \
                 two things to compare."
                    .to_string(),
            ),
            Some(d) => {
                violations.extend(check_x4_drag_view_agrees(d, &mut checked));
            }
        }
    }

    // ---- X7: focus was cleared, so no cursor scroll may still be owed ------
    //
    // NARROWER THAN THE PLAN'S WORDING, and the difference matters. The plan
    // says "if focus was cleared, no scroll adjustment may still be pending for
    // it". An issued `ScrollAdjustment` is not retained anywhere, so an
    // already-in-flight one CANNOT be seen from here — that is the residual
    // blind spot and it is not being papered over.
    //
    // What IS retained is the engine's record that a cursor scroll is OWED:
    // `cursor_needs_initialization` and `pending_contenteditable_focus` are what
    // make the next pass seed a caret and reveal it through
    // `scroll_selection_into_view`. If focus is
    // gone and either is still set, the engine will initialize a cursor for a
    // node nothing is focused on and scroll to it — which is precisely the bug
    // X7 names, caught one step earlier than the plan proposed.
    if cross.iter().any(|c| c == "X7") {
        checked += 1;
        if lw.focus_manager.focused_node.is_none() {
            if lw.focus_manager.cursor_needs_initialization {
                violations.push(
                    "X7: focus_manager.focused_node is None but cursor_needs_initialization is \
                     still true — the next pass will initialize a caret for a node nothing is \
                     focused on, and scroll it into view"
                        .to_string(),
                );
            }
            if lw.focus_manager.pending_contenteditable_focus.is_some() {
                violations.push(
                    "X7: focus_manager.focused_node is None but pending_contenteditable_focus is \
                     still Some — a cursor scroll is owed to a focus that no longer exists"
                        .to_string(),
                );
            }
        }
        if let Some(p) = lw.focus_manager.pending_contenteditable_focus.as_ref() {
            checked += 1;
            if !node_is_live(lw, p.dom_id, p.container_node_id)
                || !node_is_live(lw, p.dom_id, p.text_node_id)
            {
                violations.push(format!(
                    "X7: a cursor scroll is pending for ({}, container {}, text {}), which no \
                     longer exists",
                    p.dom_id.inner,
                    p.container_node_id.index(),
                    p.text_node_id.index()
                ));
            }
        }
    }

    // ---- X8: selection autoscroll goes through the SELECTION's container ---
    //
    // Frame-to-frame, which nothing retained until the composition trace grew a
    // second sample. `(prev2, prev)` is the delta of the last step that did
    // something (see `CompositionTrace::prev2`), so this compares the container
    // offsets before and after the drag step and asks: of the containers that
    // moved, is the selection focus node's own scroll container one of them?
    //
    // If it is not, the selection scrolled something else — which is the shape
    // of "the selection focus moved to a node that is not under the pointer"
    // that X8 was written to catch.
    if cross.iter().any(|c| c == "X8") {
        checked += 1;
        violations.extend(check_x8_selection_autoscroll(
            lw,
            callback_info,
            &mut checked,
        ));
    }

    // A scenario can REQUIRE that this assertion actually looked at something.
    //
    // Without it the assertion passes on an empty violation list, which is also
    // what "there was no manager state to inspect" looks like — the two are
    // indistinguishable from the outside. A scenario written to exercise a
    // manager can therefore keep passing long after it stopped producing the
    // state it meant to check, which is the failure mode this whole assertion
    // exists to prevent. Opt-in, because plenty of scenarios legitimately have
    // no scroll/focus/hover state at all and must not be forced to invent some.
    if let Some(min) = params.get("min_checked") {
        let Some(min) = min.as_u64() else {
            return AssertionResult::fail(
                "assert_manager_invariants: 'min_checked' must be a number".to_string(),
            );
        };
        if (checked as u64) < min {
            return AssertionResult::fail(format!(
                "assert_manager_invariants: inspected only {checked} key(s)/invariant(s) but the \
                 scenario requires at least {min}. The managers it means to exercise are not \
                 holding the state it assumes — this assertion proved nothing."
            ));
        }
    }

    if violations.is_empty() {
        return AssertionResult::pass(format!(
            "assert_manager_invariants: {checked} key(s)/invariant(s) hold across [{}] / [{}]",
            managers.join(","),
            cross.join(",")
        ));
    }
    AssertionResult::fail_with(
        format!(
            "assert_manager_invariants: {} invariant violation(s)",
            violations.len()
        ),
        "no dangling manager state".to_string(),
        violations.join("; "),
    )
}

/// X1's body: the node the harness asked `scroll_into_view` for must now be
/// inside its scroll container's visible rect, according to `ScrollManager`'s
/// OWN offset.
///
/// Three legitimate reasons a target can sit outside are excluded by name, so a
/// correct engine is never called wrong: an axis that cannot scroll at all
/// (`max_scroll == 0`), a target taller/wider than the container (it cannot fit,
/// so no offset makes it contained), and a still-running smooth animation (the
/// offset has not landed yet — that one is reported, because a scenario that
/// asserts before the animation finishes is asking a question it cannot get a
/// meaningful answer to).
#[cfg(feature = "std")]
#[allow(clippy::cast_precision_loss)] // bounded layout geometry, isize -> f32
fn check_x1_target_is_visible(
    lw: &azul_layout::window::LayoutWindow,
    tdom: azul_core::dom::DomId,
    tnode: azul_core::dom::NodeId,
    checked: &mut usize,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if !node_is_live(lw, tdom, tnode) {
        out.push(format!(
            "X1: the scroll_into_view target ({}, {}) no longer exists",
            tdom.inner,
            tnode.index()
        ));
        return out;
    }
    let target_id = azul_core::dom::DomNodeId {
        dom: tdom,
        node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(tnode)),
    };
    let Some(anc) = lw.find_scrollable_ancestor(target_id) else {
        out.push(format!(
            "X1: the scroll_into_view target ({}, {}) has NO scrollable ancestor, so nothing \
             could have scrolled and there is no containment to check. Point the op at a node \
             inside an overflow:scroll container.",
            tdom.inner,
            tnode.index()
        ));
        return out;
    };
    let Some(anode) = anc.node.into_crate_internal() else {
        out.push(
            "X1: find_scrollable_ancestor named a DOM with no node id — there is no container to \
             measure against"
                .to_string(),
        );
        return out;
    };
    *checked += 1;

    // The plan's second half: "a ScrollAdjustment for a container with no
    // AnimatedScrollState entry is a bug".
    let Some(info) = lw.scroll_manager.get_scroll_node_info(anc.dom, anode) else {
        out.push(format!(
            "X1: the scroll container ({}, {}) that scroll_into_view had to move has NO \
             AnimatedScrollState entry — the adjustment was computed against a container \
             ScrollManager does not know about",
            anc.dom.inner,
            anode.index()
        ));
        return out;
    };
    if lw
        .scroll_manager
        .get_scroll_state(anc.dom, anode)
        .is_some_and(|s| s.animation.is_some())
    {
        out.push(format!(
            "X1: the scroll container ({}, {}) is STILL ANIMATING, so its offset has not landed \
             and no containment statement can be made yet. Drive the smooth scroll to completion \
             (tick_ms) before asserting X1.",
            anc.dom.inner,
            anode.index()
        ));
        return out;
    }

    let (Some(trect), Some(arect)) = (
        lw.get_node_bounds(tdom, tnode),
        lw.get_node_bounds(anc.dom, anode),
    ) else {
        out.push(
            "X1: layout reports no bounds for the target or for its scroll container".to_string(),
        );
        return out;
    };

    // Layout coordinates are UNSCROLLED; the visible window into the content is
    // the container's rect displaced by the current offset. Same arithmetic
    // `managers::scroll_into_view::check_if_scrollable` uses to build the
    // visible rect it decides against.
    const TOL: f32 = 1.0;
    let vis_x = arect.origin.x as f32 + info.current_offset.x;
    let vis_y = arect.origin.y as f32 + info.current_offset.y;
    let vis_w = arect.size.width as f32;
    let vis_h = arect.size.height as f32;
    let tx = trect.origin.x as f32;
    let ty = trect.origin.y as f32;
    let tw = trect.size.width as f32;
    let th = trect.size.height as f32;

    *checked += 1;
    if info.max_scroll_y > 0.0
        && th <= vis_h + TOL
        && (ty < vis_y - TOL || ty + th > vis_y + vis_h + TOL)
    {
        out.push(format!(
            "X1: after scroll_into_view, target ({}, {}) spans y [{ty:.1}, {:.1}] but container \
             ({}, {}) is scrolled to y {:.1}, showing y [{vis_y:.1}, {:.1}] — ScrollManager and \
             the layout disagree about whether the target is visible",
            tdom.inner,
            tnode.index(),
            ty + th,
            anc.dom.inner,
            anode.index(),
            info.current_offset.y,
            vis_y + vis_h
        ));
    }
    if info.max_scroll_x > 0.0
        && tw <= vis_w + TOL
        && (tx < vis_x - TOL || tx + tw > vis_x + vis_w + TOL)
    {
        out.push(format!(
            "X1: after scroll_into_view, target ({}, {}) spans x [{tx:.1}, {:.1}] but container \
             ({}, {}) is scrolled to x {:.1}, showing x [{vis_x:.1}, {:.1}] — ScrollManager and \
             the layout disagree about whether the target is visible",
            tdom.inner,
            tnode.index(),
            tx + tw,
            anc.dom.inner,
            anode.index(),
            info.current_offset.x,
            vis_x + vis_w
        ));
    }
    out
}

/// X4's body: the `DragState` the public API hands out must describe the SAME
/// drag the engine is running.
///
/// `DragState::from_context` is the only conversion, so every reader of
/// `get_drag_state` / a drag callback sees its output. If it drops the source
/// node, reports the wrong kind, or manufactures a node drag out of a text
/// selection, the app acts on a drag the engine is not running.
#[cfg(feature = "std")]
fn check_x4_drag_view_agrees(d: &azul_core::drag::DragContext, checked: &mut usize) -> Vec<String> {
    use azul_core::drag::ActiveDragType;
    use azul_layout::managers::drag_drop::{DragState, DragType};

    let mut out: Vec<String> = Vec::new();
    let view = DragState::from_context(d);
    *checked += 1;
    match &d.drag_type {
        ActiveDragType::Node(n) => {
            let want = azul_core::dom::DomNodeId {
                dom: n.dom_id,
                node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(
                    n.node_id,
                )),
            };
            match view {
                None => out.push(format!(
                    "X4: a NODE drag on ({}, {}) is active but DragState::from_context returned \
                     None — every reader of the public drag API sees no drag at all",
                    n.dom_id.inner,
                    n.node_id.index()
                )),
                Some(v) => {
                    if v.drag_type != DragType::Node {
                        out.push(format!(
                            "X4: the engine is running a NODE drag but the DragState view calls \
                             it {:?}",
                            v.drag_type
                        ));
                    }
                    match v.source_node {
                        azul_core::dom::OptionDomNodeId::Some(got) if got == want => {}
                        azul_core::dom::OptionDomNodeId::Some(got) => out.push(format!(
                            "X4: the engine's drag is anchored on ({}, {}) but the DragState view \
                             says ({}, {:?})",
                            n.dom_id.inner,
                            n.node_id.index(),
                            got.dom.inner,
                            got.node.into_crate_internal().map(|x| x.index())
                        )),
                        azul_core::dom::OptionDomNodeId::None => out.push(format!(
                            "X4: the engine's drag is anchored on ({}, {}) but the DragState view \
                             reports no source node",
                            n.dom_id.inner,
                            n.node_id.index()
                        )),
                    }
                }
            }
        }
        ActiveDragType::FileDrop(_) => match view {
            None => out.push(
                "X4: a FILE drag is active but DragState::from_context returned None".to_string(),
            ),
            Some(v) => {
                if v.drag_type != DragType::File {
                    out.push(format!(
                        "X4: the engine is running a FILE drag but the DragState view calls it \
                         {:?}",
                        v.drag_type
                    ));
                }
            }
        },
        // The remaining kinds have no `DragState` representation at all, by
        // design: `DragState` is the node/file drag-and-drop API. A `Some` here
        // would mean the view invented a node drag out of a text selection or a
        // window move, which is the same class of disagreement as a wrong node.
        _ => {
            if view.is_some() {
                out.push(format!(
                    "X4: DragState::from_context produced a node/file DragState for a {} drag, \
                     which has no such representation",
                    drag_kind_str(d)
                ));
            }
        }
    }
    out
}

/// X8's body: during a text-selection drag, the container that autoscrolled must
/// be the selection focus node's OWN scroll container.
///
/// Reads the two most recent composition samples, which is the only
/// frame-to-frame history this crate keeps. `prev` is the state as of the start
/// of the assertion step (i.e. after the last op that did something) and `prev2`
/// is the state before that op — see [`CompositionTrace::prev2`].
#[cfg(feature = "std")]
fn check_x8_selection_autoscroll(
    lw: &azul_layout::window::LayoutWindow,
    callback_info: &azul_layout::callbacks::CallbackInfo,
    checked: &mut usize,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let samples = {
        let guard = scratch(callback_info);
        guard
            .composition_trace
            .as_ref()
            .and_then(|t| match (t.prev.as_ref(), t.prev2.as_ref()) {
                (Some(a), Some(b)) => Some((a.clone(), b.clone())),
                _ => None,
            })
    };
    let Some((prev, prev2)) = samples else {
        out.push(
            "X8 needs two steps of history and this scenario has fewer. The invariant is about \
             what changed BETWEEN two frames of a drag; run the drag over at least two steps \
             before asserting it."
                .to_string(),
        );
        return out;
    };
    if !prev.text_selection_drag {
        out.push(
            "X8 was requested but no TEXT-SELECTION drag was live at the previous step. The \
             invariant speaks only about selection autoscroll; with no selection drag it has no \
             subject."
                .to_string(),
        );
        return out;
    }

    let mut moved: Vec<(usize, usize)> = Vec::new();
    for (key, now) in &prev.scroll_offsets {
        if prev2.scroll_offsets.get(key) != Some(now) {
            moved.push(*key);
        }
    }
    for key in prev2.scroll_offsets.keys() {
        if !prev.scroll_offsets.contains_key(key) {
            moved.push(*key);
        }
    }
    moved.sort_unstable();
    moved.dedup();
    *checked += 1;
    if moved.is_empty() {
        out.push(
            "X8 was requested but NO container scrolled between the last two steps. The invariant \
             is about where a selection AUTOSCROLL went; drag past the container edge so one \
             happens."
                .to_string(),
        );
        return out;
    }

    let Some((sdom, snode)) = prev.selection_focus_node else {
        out.push(format!(
            "X8: a text-selection drag scrolled {} container(s), but there is no selection focus \
             node — the scroll cannot belong to the selection",
            moved.len()
        ));
        return out;
    };
    let focus_id = azul_core::dom::DomNodeId {
        dom: azul_core::dom::DomId { inner: sdom },
        node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(
            azul_core::id::NodeId::new(snode),
        )),
    };
    let Some(anc) = lw.find_scrollable_ancestor(focus_id) else {
        out.push(format!(
            "X8: {} container(s) scrolled during the selection drag, but the selection focus node \
             ({sdom}, {snode}) has no scrollable ancestor at all — the autoscroll moved something \
             the selection is not inside",
            moved.len()
        ));
        return out;
    };
    let Some(anode) = anc.node.into_crate_internal() else {
        out.push("X8: the selection's scroll container has no node id".to_string());
        return out;
    };
    *checked += 1;
    if !moved.contains(&(anc.dom.inner, anode.index())) {
        out.push(format!(
            "X8: during a text-selection drag the container(s) that scrolled were {moved:?}, but \
             the selection focus node ({sdom}, {snode}) lives in container ({}, {}) — the \
             autoscroll went through a container the selection is not in, which is exactly the \
             'selection focus moved instead of scrolling' failure",
            anc.dom.inner,
            anode.index()
        ));
    }
    out
}

// ==================== NON-INTERFERENCE: what did this op MOVE? ==============
//
// `assert_manager_invariants` answers "is some manager's state internally
// wrong?". That is only half of the cross-manager question. The other half — the
// one a user actually asks — is "did pressing Tab quietly move the SCROLL
// manager?".
//
// Nothing could answer it. A manager that is written by an unrelated code path
// is invisible in the DOM, invisible in the pixels, invisible in the resource
// counters and (as long as the value it was given happens to be self-consistent)
// invisible to every invariant in this file. `snapshot_resources` /
// `assert_resource_counts` already established the shape for RESOURCES: record
// before, diff after, require the delta to be what the scenario said it would
// be. These two ops are the same shape for MANAGERS.
//
// The assertion is deliberately TWO-SIDED. "A manager moved that you did not
// list" is a leak; "a manager you listed did not move" is a scenario that has
// stopped exercising what it claims to exercise. Only checking the first would
// let `changed: [every, manager, here]` pass forever while asserting nothing,
// which is the same false-green `min_checked` exists to prevent on
// `assert_manager_invariants`.

/// One manager's observable state, reduced to something a diff can compare.
#[cfg(feature = "std")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManagerFingerprint {
    /// How many discrete pieces of state this manager is holding right now.
    /// `0` means idle/empty. Used by `min_populated` so a scenario can require
    /// that the managers it talks about were actually carrying state, rather
    /// than comparing two empty snapshots and calling it non-interference.
    population: usize,
    /// The state itself, rendered so a failure can point at what moved.
    digest: String,
}

#[cfg(feature = "std")]
impl ManagerFingerprint {
    fn new(population: usize, digest: String) -> Self {
        Self { population, digest }
    }
}

/// Every manager whose state `snapshot_managers` records, in the order a report
/// lists them. These are the names a scenario writes in `changed`.
///
/// `scroll` and `focus` are the same aliases `KNOWN_MANAGERS` uses for the
/// `scroll_state` / `focus_cursor` modules — what a scenario writes, not what
/// the file is called.
///
/// This list is BROADER than `KNOWN_MANAGERS`, and deliberately so: X10 needs a
/// manager to expose node KEYS it can prove live, which most of the capability
/// managers do not, whereas non-interference only needs the manager's state to
/// be readable and to move when it is written. `keyring` can be fingerprinted
/// even though there is no such thing as a dangling keyring node.
///
/// A function rather than a `const` because `a11y` is conditional: without the
/// feature, `A11yManager` is a field-less stub whose fingerprint would be a
/// constant, and a constant fingerprint is an assertion that cannot fail.
#[cfg(feature = "std")]
fn fingerprinted_managers() -> Vec<&'static str> {
    #[allow(unused_mut)]
    let mut names: Vec<&'static str> = alloc::vec![
        "scroll",
        "hover",
        "focus",
        "gesture",
        "text_edit",
        "text_input",
        "undo_redo",
        "virtual_view",
        "gpu_state",
        "permission",
        "clipboard",
        "file_drop",
        "gamepad",
        "geolocation",
        "biometric",
        "keyring",
        "sensors",
        "eyedropper",
    ];
    #[cfg(feature = "a11y")]
    names.push("a11y");
    names
}

/// Manager modules that `snapshot_managers` does NOT record, each with the
/// reason. Recorded rather than omitted, for exactly the reason
/// `UNOBSERVABLE_MANAGERS` is: an absent manager is a silent hole, and a silent
/// hole in a NON-INTERFERENCE check is worse than one in an invariant — it makes
/// the leak it would have caught read as "nothing changed".
///
/// Naming one of these in `changed` is a hard failure with its reason attached.
/// A unit test pins this list plus [`fingerprinted_managers`] against the
/// contents of `layout/src/managers/`, so a new manager cannot join the tree
/// unclassified.
#[cfg(feature = "std")]
fn not_fingerprintable() -> Vec<(&'static str, &'static str)> {
    #[allow(unused_mut)]
    let mut reasons: Vec<(&'static str, &'static str)> = alloc::vec![
    (
        "a11y_snapshot",
        "pure projection, rebuilt from the DOM on every call and never stored — see the matching entry in UNOBSERVABLE_MANAGERS. Fingerprinting it would hash a value that is recomputed rather than remembered, so it would report a change on every frame and mean nothing",
    ),
    (
        "scroll_into_view",
        "stateless — free functions that compute ScrollAdjustments, write them into ScrollManager \
         and return; there is no state of its own to move. Its effect shows up as a `scroll` \
         change, which IS fingerprinted",
    ),
    (
        "scroll_registration",
        "stateless — `register_scroll_nodes` publishes layout's scroll containers into \
         ScrollManager and returns; it keeps nothing between calls. What it wrote is hashed as \
         `scroll`, so fingerprinting it separately would hash the same bytes twice",
    ),
    (
        "drag_drop",
        "holds no state: the second Option<DragContext> was deleted in 2026-07 and what remains is \
         the stateless DragState view built on demand from gesture_drag_manager.active_drag, which \
         IS fingerprinted as `gesture`",
    ),
    (
        "changeset",
        "not a manager — a data type. A recorded changeset lives in TextInputManager, which IS \
         fingerprinted as `text_input`",
    ),
    (
        "selection",
        "there is no SelectionManager; the live selection is TextEditManager::multi_cursor, which \
         the `text_edit` fingerprint already covers. A separate entry would double-count one \
         manager and make \"only text_edit moved\" unstateable",
    ),
    ];
    #[cfg(not(feature = "a11y"))]
    reasons.push((
        "a11y",
        "this build has no `a11y` feature, so A11yManager is a field-less stub. Its fingerprint \
         would be a constant, and a constant fingerprint is an assertion that cannot fail",
    ));
    reasons
}

/// Fingerprint every manager on this window.
///
/// WHAT IS DELIBERATELY EXCLUDED, and why — an unexplained exclusion is a blind
/// spot that reads as coverage:
///
/// * `AnimatedScrollState::last_activity` (an `Instant`): moves on every scroll,
///   i.e. it is redundant with the offset that is recorded, and rendering a
///   clock value into a digest makes the digest depend on the wall clock.
/// * `ScrollManager::scrollbar_states`: private, and recomputed from geometry
///   every frame — it is a derived cache, not state anybody can leak into.
/// * `GpuValueCache`'s animated VALUES: they change every frame of a running
///   animation, so they would report "gpu_state moved" for every tick of a fade
///   the scenario asked for. The KEYS are recorded (a key is what leaks) and
///   `scrollbar_fade_active` says whether a fade is live.
/// * `VirtualViewState`'s per-view internals and geolocation's private pending
///   queues: no accessor exists. The view KEYS and the refcount are recorded.
/// * the a11y tree's CONTENT — see [`fp_a11y`] for why, and for what that costs.
///
/// A scenario must settle its frames (`wait_frame` + `wait`) before BOTH the
/// snapshot and the assertion: the transient repaint flags (`display_list_dirty`,
/// `scroll_dirty`, `pending_wheel_event`) are real state and ARE recorded, so
/// comparing a settled window against a mid-flight one reports them as moved.
/// That is the correct answer to the question asked; it is not noise.
#[cfg(feature = "std")]
pub(crate) fn manager_fingerprints(
    lw: &azul_layout::window::LayoutWindow,
) -> BTreeMap<String, ManagerFingerprint> {
    let mut out: BTreeMap<String, ManagerFingerprint> = BTreeMap::new();
    out.insert("scroll".to_string(), fp_scroll(&lw.scroll_manager));
    out.insert("hover".to_string(), fp_hover(&lw.hover_manager));
    out.insert("focus".to_string(), fp_focus(&lw.focus_manager));
    out.insert("gesture".to_string(), fp_gesture(&lw.gesture_drag_manager));
    out.insert("text_edit".to_string(), fp_text_edit(&lw.text_edit_manager));
    out.insert(
        "text_input".to_string(),
        fp_text_input(&lw.text_input_manager),
    );
    out.insert("undo_redo".to_string(), fp_undo_redo(&lw.undo_redo_manager));
    out.insert(
        "virtual_view".to_string(),
        fp_virtual_view(&lw.virtual_view_manager),
    );
    out.insert("gpu_state".to_string(), fp_gpu_state(&lw.gpu_state_manager));
    out.insert(
        "permission".to_string(),
        fp_permission(&lw.permission_manager),
    );
    out.insert("clipboard".to_string(), fp_clipboard(&lw.clipboard_manager));
    out.insert("file_drop".to_string(), fp_file_drop(&lw.file_drop_manager));
    out.insert("gamepad".to_string(), fp_gamepad(&lw.gamepad_manager));
    out.insert(
        "geolocation".to_string(),
        fp_geolocation(&lw.geolocation_manager),
    );
    out.insert("biometric".to_string(), fp_biometric(&lw.biometric_manager));
    out.insert("keyring".to_string(), fp_keyring(&lw.keyring_manager));
    out.insert("sensors".to_string(), fp_sensors(&lw.sensor_manager));
    out.insert(
        "eyedropper".to_string(),
        fp_eyedropper(&lw.eyedropper_manager),
    );
    #[cfg(feature = "a11y")]
    out.insert("a11y".to_string(), fp_a11y(&lw.a11y_manager));
    out
}

// -- the per-manager fingerprints --------------------------------------------
//
// Each takes ONLY its own manager, never the window. That is what makes them
// unit-testable without an engine: `fingerprint_moves_when_its_manager_moves`
// builds a default manager, mutates ONE field through the manager's own public
// API, and requires the digest to change. A fingerprint that ignored the field
// somebody later leaks into is an assertion that cannot fail, and this repo has
// shipped that defect before — see the scrollbar-fade latch that sat outside
// `KNOWN_MANAGERS`.

#[cfg(feature = "std")]
fn fp_scroll(m: &azul_layout::managers::scroll_state::ScrollManager) -> ManagerFingerprint {
    let keys = m.state_keys();
    let mut parts: Vec<String> = Vec::new();
    for (dom, node) in &keys {
        let (ox, oy) = m
            .get_current_offset(*dom, *node)
            .map_or((0.0_f32, 0.0_f32), |o| (o.x, o.y));
        let animating = m
            .get_scroll_state(*dom, *node)
            .is_some_and(|s| s.animation.is_some());
        parts.push(format!(
            "({},{})=({ox:.2},{oy:.2}){}",
            dom.inner,
            node.index(),
            if animating { "+anim" } else { "" }
        ));
    }
    parts.push(format!("dirty={}", m.has_pending_scroll_changes()));
    parts.push(format!("wheel={}", m.pending_wheel_event.is_some()));
    ManagerFingerprint::new(keys.len(), parts.join(" "))
}

#[cfg(feature = "std")]
fn fp_hover(m: &azul_layout::managers::hover::HoverManager) -> ManagerFingerprint {
    let (points, entries) = m.debug_counts();
    let mut parts: Vec<String> = Vec::new();
    for point in m.get_active_input_points() {
        let Some(hit) = m.get_current(&point) else {
            continue;
        };
        let mut nodes: Vec<String> = Vec::new();
        for (dom, ht) in &hit.hovered_nodes {
            for nid in ht
                .regular_hit_test_nodes
                .keys()
                .chain(ht.scroll_hit_test_nodes.keys())
                .chain(ht.cursor_hit_test_nodes.keys())
            {
                nodes.push(format!("({},{})", dom.inner, nid.index()));
            }
        }
        nodes.sort_unstable();
        nodes.dedup();
        parts.push(format!("{point:?}=[{}]", nodes.join(",")));
    }
    ManagerFingerprint::new(
        points,
        format!("points={points} entries={entries} {}", parts.join(" ")),
    )
}

#[cfg(feature = "std")]
fn fp_focus(m: &azul_layout::managers::focus_cursor::FocusManager) -> ManagerFingerprint {
    let mut population = 0usize;
    let focused = match m.focused_node {
        None => "none".to_string(),
        Some(f) => {
            population += 1;
            format!(
                "({},{:?})",
                f.dom.inner,
                f.node.into_crate_internal().map(|n| n.index())
            )
        }
    };
    if m.pending_focus_request.is_some() {
        population += 1;
    }
    let pending_ce = match m.pending_contenteditable_focus.as_ref() {
        None => "none".to_string(),
        Some(p) => {
            population += 1;
            format!(
                "({},{},{})",
                p.dom_id.inner,
                p.container_node_id.index(),
                p.text_node_id.index()
            )
        }
    };
    if m.deferred_focus_target.is_some() {
        population += 1;
    }
    ManagerFingerprint::new(
        population,
        format!(
            "focused={focused} pending={:?} cursor_init={} pending_ce={pending_ce} deferred={:?}",
            m.pending_focus_request, m.cursor_needs_initialization, m.deferred_focus_target
        ),
    )
}

#[cfg(feature = "std")]
fn fp_gesture(m: &azul_layout::managers::gesture::GestureAndDragManager) -> ManagerFingerprint {
    let mut population = m.input_sessions.len();
    let drag = match m.active_drag.as_ref() {
        None => "none".to_string(),
        Some(d) => {
            population += 1;
            let anchor = drag_source_node(d).map_or_else(
                || "-".to_string(),
                |(dom, n)| format!("({},{})", dom.inner, n.index()),
            );
            format!(
                "{}@{anchor}#{}{}",
                drag_kind_str(d),
                d.session_id,
                if d.cancelled { "!cancelled" } else { "" }
            )
        }
    };
    let sessions: Vec<String> = m
        .input_sessions
        .iter()
        .map(|s| {
            format!(
                "#{}:{}{}",
                s.session_id,
                s.samples.len(),
                if s.ended { "" } else { "+live" }
            )
        })
        .collect();
    if m.pen_state.is_some() {
        population += 1;
    }
    if m.pad_state.is_some() {
        population += 1;
    }
    if m.native_gesture.is_some() {
        population += 1;
    }
    ManagerFingerprint::new(
        population,
        format!(
            "drag={drag} sessions=[{}] pen={} pen_pending={} pad={} native={:?}",
            sessions.join(","),
            m.pen_state.as_ref().map_or_else(
                || "none".to_string(),
                |p| format!(
                    "dev{}{}",
                    p.device_id,
                    if p.in_contact { "+contact" } else { "" }
                )
            ),
            m.pen_event_pending,
            m.pad_state.is_some(),
            m.native_gesture
        ),
    )
}

#[cfg(feature = "std")]
fn fp_text_edit(m: &azul_layout::managers::text_edit::TextEditManager) -> ManagerFingerprint {
    let mut population = 0usize;
    let cursor = match m.multi_cursor.as_ref() {
        None => "none".to_string(),
        Some(mc) => {
            population += mc.selections.len();
            format!(
                "({},{:?})x{}span{}key{}",
                mc.node_id.dom.inner,
                mc.node_id.node.into_crate_internal().map(|n| n.index()),
                mc.selections.len(),
                multi_cursor_span(mc),
                mc.contenteditable_key
            )
        }
    };
    if m.preedit_text.is_some() {
        population += 1;
    }
    ManagerFingerprint::new(
        population,
        format!(
            "cursor={cursor} preedit={:?}({},{}) blink={} dirty={}",
            m.preedit_text.as_deref().unwrap_or(""),
            m.preedit_cursor_begin,
            m.preedit_cursor_end,
            m.blink.blink_timer_active,
            m.display_list_dirty
        ),
    )
}

#[cfg(feature = "std")]
fn fp_text_input(m: &azul_layout::managers::text_input::TextInputManager) -> ManagerFingerprint {
    let mut population = 0usize;
    let entries: Vec<String> = m
        .pending_changesets
        .iter()
        .map(|q| {
            population += 1;
            format!(
                "({},{:?})+{}/{}@{:?}",
                q.edit.node.dom.inner,
                q.edit.node.node.into_crate_internal().map(|n| n.index()),
                q.edit.inserted_text.as_str().len(),
                q.edit.old_text.as_str().len(),
                q.source
            )
        })
        .collect();
    let pending = if entries.is_empty() {
        "none".to_string()
    } else {
        entries.join(",")
    };
    ManagerFingerprint::new(population, format!("pending={pending}"))
}

#[cfg(feature = "std")]
fn fp_undo_redo(m: &azul_layout::managers::undo_redo::UndoRedoManager) -> ManagerFingerprint {
    let stacks: Vec<String> = m
        .node_stacks
        .iter()
        .map(|s| {
            format!(
                "{}:u{}r{}",
                s.node_id.index(),
                s.undo_stack.len(),
                s.redo_stack.len()
            )
        })
        .collect();
    ManagerFingerprint::new(
        m.node_stacks.len() + m.content_snapshots.len(),
        format!(
            "stacks=[{}] snapshots={}",
            stacks.join(","),
            m.content_snapshots.len()
        ),
    )
}

#[cfg(feature = "std")]
fn fp_virtual_view(
    m: &azul_layout::managers::virtual_view::VirtualViewManager,
) -> ManagerFingerprint {
    let keys = m.all_view_keys();
    let rendered: Vec<String> = keys
        .iter()
        .map(|(dom, node)| {
            format!(
                "({},{}){}",
                dom.inner,
                node.index(),
                if m.was_virtual_view_invoked(*dom, *node) {
                    "+invoked"
                } else {
                    ""
                }
            )
        })
        .collect();
    ManagerFingerprint::new(
        keys.len(),
        format!("views=[{}] count={}", rendered.join(","), m.debug_counts()),
    )
}

#[cfg(feature = "std")]
fn fp_gpu_state(m: &azul_layout::managers::gpu_state::GpuStateManager) -> ManagerFingerprint {
    let mut population = 0usize;
    let mut caches: Vec<String> = Vec::new();
    for (dom, cache) in &m.caches {
        let node_keys = cache.transform_keys.len()
            + cache.h_transform_keys.len()
            + cache.css_transform_keys.len()
            + cache.opacity_keys.len();
        let bar_keys = cache.scrollbar_v_opacity_keys.len() + cache.scrollbar_h_opacity_keys.len();
        population += node_keys + bar_keys;
        caches.push(format!("dom{}:n{node_keys}b{bar_keys}", dom.inner));
    }
    ManagerFingerprint::new(
        population,
        format!(
            "caches=[{}] fade={} pending={}",
            caches.join(","),
            m.scrollbar_fade_active,
            m.pending_changes.transform_key_changes.len()
                + m.pending_changes.opacity_key_changes.len()
                + m.pending_changes.scrollbar_opacity_changes.len()
        ),
    )
}

#[cfg(feature = "std")]
fn fp_permission(m: &azul_layout::managers::permission::PermissionManager) -> ManagerFingerprint {
    let entries: Vec<String> = m
        .statuses
        .iter()
        .map(|(cap, e)| {
            format!(
                "{cap:?}={:?}x{}@{}",
                e.state,
                e.refcount,
                e.last_subscriber.map_or_else(
                    || "-".to_string(),
                    |s| format!(
                        "({},{:?})",
                        s.dom.inner,
                        s.node.into_crate_internal().map(|n| n.index())
                    )
                )
            )
        })
        .collect();
    ManagerFingerprint::new(m.statuses.len(), entries.join(" "))
}

#[cfg(feature = "std")]
fn fp_clipboard(m: &azul_layout::managers::clipboard::ClipboardManager) -> ManagerFingerprint {
    let paste = m
        .get_paste_content()
        .map_or(0, |c| c.plain_text.as_str().len());
    let copy = m
        .get_copy_content()
        .map_or(0, |c| c.plain_text.as_str().len());
    ManagerFingerprint::new(
        usize::from(m.has_paste_content()) + usize::from(m.has_copy_content()),
        format!(
            "paste={}({paste}) copy={}({copy})",
            m.has_paste_content(),
            m.has_copy_content()
        ),
    )
}

#[cfg(feature = "std")]
fn fp_file_drop(m: &azul_layout::managers::file_drop::FileDropManager) -> ManagerFingerprint {
    let hovered = m.get_hovered_files();
    let dropped = m.get_dropped_files();
    ManagerFingerprint::new(
        hovered.len() + dropped.len(),
        format!(
            "hovered={} dropped={} cancelled={}",
            hovered.len(),
            dropped.len(),
            m.hover_was_cancelled()
        ),
    )
}

#[cfg(feature = "std")]
fn fp_gamepad(m: &azul_layout::managers::gamepad::GamepadManager) -> ManagerFingerprint {
    let pads = m.gamepads();
    ManagerFingerprint::new(
        pads.len(),
        format!(
            "pads={} primary={} listeners={}",
            pads.len(),
            m.primary().is_some(),
            m.has_listeners()
        ),
    )
}

#[cfg(feature = "std")]
fn fp_geolocation(
    m: &azul_layout::managers::geolocation::GeolocationManager,
) -> ManagerFingerprint {
    let mut population = m.refcount() as usize;
    if m.latest_fix().is_some() {
        population += 1;
    }
    if m.last_error.is_some() {
        population += 1;
    }
    ManagerFingerprint::new(
        population,
        format!(
            "refcount={} subscribed={} fix={} config={} error={}",
            m.refcount(),
            m.has_active_subscription(),
            m.latest_fix().is_some(),
            m.active_config.is_some(),
            m.last_error
                .as_ref()
                .map_or_else(|| "-".to_string(), |e| e.code.to_string())
        ),
    )
}

#[cfg(feature = "std")]
fn fp_biometric(m: &azul_layout::managers::biometric::BiometricManager) -> ManagerFingerprint {
    ManagerFingerprint::new(
        usize::from(m.last_result.is_some()) + m.in_flight as usize,
        format!(
            "result={:?} availability={:?} in_flight={} pending={}",
            m.last_result, m.availability, m.in_flight, m.pending_event
        ),
    )
}

#[cfg(feature = "std")]
fn fp_keyring(m: &azul_layout::managers::keyring::KeyringManager) -> ManagerFingerprint {
    ManagerFingerprint::new(
        usize::from(m.last_result.is_some()) + m.in_flight as usize,
        format!(
            "result={:?} in_flight={} pending={}",
            m.last_result, m.in_flight, m.pending_event
        ),
    )
}

#[cfg(feature = "std")]
fn fp_sensors(m: &azul_layout::managers::sensors::SensorManager) -> ManagerFingerprint {
    ManagerFingerprint::new(
        usize::from(m.accelerometer.is_some())
            + usize::from(m.gyroscope.is_some())
            + usize::from(m.magnetometer.is_some()),
        format!(
            "accel={:?} gyro={:?} mag={:?} pending={} listeners={}",
            m.accelerometer, m.gyroscope, m.magnetometer, m.pending_event, m.has_listeners
        ),
    )
}

fn fp_eyedropper(m: &azul_layout::managers::eyedropper::EyedropperManager) -> ManagerFingerprint {
    ManagerFingerprint::new(
        m.issued().len() + usize::from(m.last_result().is_some()),
        format!(
            "issued={:?} last_result={:?} pending_async={}",
            m.issued(),
            m.last_result(),
            m.has_pending_async(),
        ),
    )
}

/// PRESENCE-GRANULARITY ONLY, and that is a real limit worth stating: `tree` and
/// `last_tree_update` are recorded as booleans, not as content.
/// `update_a11y_tree` runs after EVERY successful layout and overwrites
/// `last_tree_update`, so a content-sensitive digest would report "a11y moved"
/// for every relayout in every scenario and be useless; a presence digest is
/// stable but blind to a tree that changed shape. Proving THAT would need an
/// `A11yNodeId -> NodeId` walk this crate does not have — the same gap
/// `UNOBSERVABLE_MANAGERS` records for X10.
#[cfg(all(feature = "std", feature = "a11y"))]
fn fp_a11y(m: &azul_layout::managers::a11y::A11yManager) -> ManagerFingerprint {
    ManagerFingerprint::new(
        usize::from(m.tree.is_some()) + usize::from(m.last_tree_update.is_some()),
        format!(
            "root={:?} tree={} update={} initialized={}",
            m.root_id,
            m.tree.is_some(),
            m.last_tree_update.is_some(),
            m.tree_initialized
        ),
    )
}

/// The pure diff behind `assert_only_managers_changed`.
///
/// Returns `(moved, expected_but_static)`: the managers whose fingerprint
/// differs between `before` and `after`, and the managers the scenario said
/// would move that did not. Split out from the op so it can be unit-tested
/// without an engine — the two failure directions have to be PROVEN to fire,
/// not assumed to.
#[cfg(feature = "std")]
fn diff_manager_fingerprints(
    before: &BTreeMap<String, ManagerFingerprint>,
    after: &BTreeMap<String, ManagerFingerprint>,
    expected: &[String],
) -> (Vec<String>, Vec<String>) {
    let mut moved: Vec<String> = Vec::new();
    for (name, now) in after {
        match before.get(name) {
            // A manager present in one snapshot and absent from the other is a
            // MOVE, not something to skip: it means the two snapshots were taken
            // by builds with different manager sets, and silently ignoring that
            // is how a whole manager disappears from a non-interference check.
            None => moved.push(name.clone()),
            Some(was) if was != now => moved.push(name.clone()),
            Some(_) => {}
        }
    }
    for name in before.keys() {
        if !after.contains_key(name) {
            moved.push(name.clone());
        }
    }
    moved.sort_unstable();
    moved.dedup();
    let expected_but_static: Vec<String> = expected
        .iter()
        .filter(|e| !moved.contains(*e))
        .cloned()
        .collect();
    (moved, expected_but_static)
}

/// `assert_only_managers_changed` — the non-interference assertion.
///
/// Diffs every manager against a named `snapshot_managers` and requires the set
/// that moved to be EXACTLY the set the scenario named in `changed`.
///
/// Parameters:
/// * `vs` — the `snapshot_managers` name to diff against (required).
/// * `changed` — the exact set of managers expected to have moved (required;
///   `[]` means "this op must not have touched a single manager").
/// * `min_populated` — optionally require that at least N managers are holding
///   non-empty state right now. Without it, two empty snapshots compare equal
///   and the assertion reports "nothing interfered" for a window where nothing
///   ever happened. Same guard, same reason, as `min_checked` on
///   `assert_manager_invariants`.
///
/// ```json
/// { "op": "snapshot_managers", "as": "before" },
/// { "op": "key_down", "key": "tab" },
/// { "op": "wait_frame" },
/// { "op": "assert_only_managers_changed",
///   "vs": "before", "changed": ["focus"], "min_populated": 1 }
/// ```
#[cfg(feature = "std")]
fn eval_assert_only_managers_changed(
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> AssertionResult {
    if let Some(bad) = reject_unknown_params(
        "assert_only_managers_changed",
        params,
        &["vs", "changed", "min_populated"],
    ) {
        return bad;
    }

    let Some(vs) = params.get("vs").and_then(|v| v.as_str()) else {
        return AssertionResult::fail(
            "assert_only_managers_changed: missing 'vs' (the snapshot_managers name to diff \
             against)",
        );
    };

    let Some(serde_json::Value::Array(want)) = params.get("changed") else {
        return AssertionResult::fail(
            "assert_only_managers_changed: missing 'changed' (the EXACT array of managers this op \
             is allowed to move; write [] for \"it must move nothing\"). It is not optional — a \
             default would have to be either \"anything goes\" or a guess, and both pass leaks.",
        );
    };
    let known = fingerprinted_managers();
    let unknown_reasons = not_fingerprintable();
    let mut expected: Vec<String> = Vec::new();
    for v in want {
        let Some(name) = v.as_str() else {
            return AssertionResult::fail(
                "assert_only_managers_changed: 'changed' must be an array of strings",
            );
        };
        if let Some((_, why)) = unknown_reasons.iter().find(|(n, _)| *n == name) {
            return AssertionResult::fail(format!(
                "assert_only_managers_changed: manager '{name}' is NOT recorded by \
                 snapshot_managers and will not be silently passed — {why}"
            ));
        }
        if !known.contains(&name) {
            return AssertionResult::fail(format!(
                "assert_only_managers_changed: unknown manager '{name}' (known: {}). Refusing to \
                 accept an expectation nothing can check.",
                known.join(", ")
            ));
        }
        expected.push(name.to_string());
    }
    expected.sort_unstable();
    expected.dedup();

    let Some(before) = scratch(callback_info).manager_snapshots.get(vs).cloned() else {
        return AssertionResult::fail(format!(
            "assert_only_managers_changed: no manager snapshot named '{vs}' (use \
             snapshot_managers first)"
        ));
    };
    let after = manager_fingerprints(callback_info.get_layout_window());

    // The name list and the map builder must agree. If a manager is declared but
    // never built, this assertion happily ACCEPTS its name in `changed` and then
    // never compares it — a name that reads as full coverage and checks nothing.
    // Checked here rather than in a test because only the running builder knows
    // what it actually produced.
    let missing: Vec<&str> = known
        .iter()
        .copied()
        .filter(|n| !after.contains_key(*n))
        .collect();
    if !missing.is_empty() {
        return AssertionResult::fail(format!(
            "assert_only_managers_changed: {missing:?} are named by fingerprinted_managers() but \
             manager_fingerprints did not produce them. Every name this assertion accepts must be \
             a name it actually compares; refusing to report non-interference over a set it did \
             not measure."
        ));
    }

    if let Some(min) = params.get("min_populated") {
        let Some(min) = min.as_u64() else {
            return AssertionResult::fail(
                "assert_only_managers_changed: 'min_populated' must be a number",
            );
        };
        let populated = after.values().filter(|f| f.population > 0).count();
        if (populated as u64) < min {
            return AssertionResult::fail_with(
                format!(
                    "assert_only_managers_changed: only {populated} manager(s) hold any state, but \
                     the scenario requires at least {min}. Two empty snapshots always compare \
                     equal — this assertion proved nothing about interference."
                ),
                format!("at least {min} populated manager(s)"),
                populated.to_string(),
            );
        }
    }

    let (moved, expected_but_static) = diff_manager_fingerprints(&before, &after, &expected);

    let mut violations: Vec<String> = Vec::new();
    for name in &moved {
        if expected.contains(name) {
            continue;
        }
        let was = before.get(name).map_or("<absent>", |f| f.digest.as_str());
        let now = after.get(name).map_or("<absent>", |f| f.digest.as_str());
        violations.push(format!(
            "'{name}' moved but was not listed in 'changed': [{was}] -> [{now}]"
        ));
    }
    for name in &expected_but_static {
        violations.push(format!(
            "'{name}' was listed in 'changed' but did not move — the scenario is no longer \
             exercising it, so this assertion is no longer proving what it claims"
        ));
    }

    if violations.is_empty() {
        return AssertionResult::pass(format!(
            "assert_only_managers_changed: exactly [{}] moved vs '{vs}'; the other {} manager(s) \
             are untouched",
            expected.join(","),
            after.len() - moved.len()
        ));
    }
    AssertionResult::fail_with(
        format!(
            "assert_only_managers_changed: {} manager(s) disagree with 'changed'",
            violations.len()
        ),
        format!("exactly [{}] moved", expected.join(",")),
        violations.join("; "),
    )
}

// ---- assert_composition: the per-step stage trace --------------------------

/// One sample of the manager state, taken by the step loop before every step.
#[cfg(feature = "std")]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CompositionSample {
    drag_active: bool,
    scroll_animating: bool,
    focus_set: bool,
    editing_active: bool,
    hover_active: bool,
    damage_patch: bool,
    damage_full: bool,
    selection_span: u64,
    /// Sum of |scroll offset| over every scroll state, in 1/100 logical px, so
    /// "the container started scrolling" is a plain integer comparison.
    scroll_offset_sum: i64,
    /// Per-scroll-node offsets, keyed `(dom.inner, node.index())`, in 1/100
    /// logical px. The sum above answers "did SOMETHING scroll"; X8 has to know
    /// WHICH container scrolled, because its whole point is that a selection
    /// autoscroll must move the selection's OWN container and not some other one.
    scroll_offsets: BTreeMap<(usize, usize), (i64, i64)>,
    /// The node the live selection's cursor sits in, if any. X8 compares it
    /// against the container that actually moved.
    selection_focus_node: Option<(usize, usize)>,
    /// Is the live drag a TEXT-SELECTION drag? X8 only speaks about those.
    text_selection_drag: bool,
}

/// The stage trace `assert_composition` reads: for each stage name, the index of
/// the FIRST step at which it was observed.
#[cfg(feature = "std")]
#[derive(Debug, Clone, Default)]
struct CompositionTrace {
    entered: BTreeMap<String, usize>,
    prev: Option<CompositionSample>,
    /// The sample BEFORE `prev`.
    ///
    /// Samples are taken before every step, so when an assertion step is
    /// running, `prev` is the state as of just before that assertion — i.e.
    /// AFTER the last op that did something — and `prev2` is the state before
    /// that op. `(prev2, prev)` is therefore the frame-to-frame delta of the
    /// last real op, which is the only thing X8 ("selection focus and the
    /// scrolled container must stay mutually consistent frame-to-frame") can be
    /// asked about. Comparing live state against `prev` would compare an
    /// assertion step against itself and always see zero.
    prev2: Option<CompositionSample>,
    samples: usize,
}

/// Every stage name `assert_composition` understands. An `expect` entry outside
/// this list is a hard failure, so a typo can never silently pass.
#[cfg(feature = "std")]
const COMPOSITION_STAGES: &[&str] = &[
    "drag_active",
    "selection_grew",
    "scroll_started",
    "scroll_animating",
    "damage_patch",
    "damage_full",
    "focus_set",
    "editing_active",
    "hover_active",
];

/// Reset the composition trace. Called at the start of every E2E test and by the
/// `reset_frame_counters` op (the plan's "checkpoint").
#[cfg(feature = "std")]
pub(crate) fn e2e_reset_composition_trace(callback_info: &azul_layout::callbacks::CallbackInfo) {
    scratch(callback_info).composition_trace = Some(CompositionTrace::default());
}

/// Sample the manager state and fold it into the stage trace. Called by the step
/// loop in `resume_e2e_continuation` before each step, so the sample observes the
/// PREVIOUS step's effects after the shell serviced them.
#[cfg(feature = "std")]
fn e2e_record_composition_sample(
    step_index: usize,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) {
    let lw = callback_info.get_layout_window();
    let synced_report = lw.frame_report_synced();
    let paint = &synced_report.paint_damage;
    let sample = CompositionSample {
        drag_active: lw.gesture_drag_manager.active_drag.is_some(),
        scroll_animating: lw.scroll_manager.has_active_animations(),
        focus_set: lw.focus_manager.focused_node.is_some(),
        editing_active: lw.text_edit_manager.multi_cursor.is_some(),
        hover_active: lw.hover_manager.current_hover_node_full().is_some(),
        damage_patch: !paint.is_none() && !paint.is_full(),
        damage_full: paint.is_full(),
        selection_span: lw
            .text_edit_manager
            .multi_cursor
            .as_ref()
            .map_or(0, multi_cursor_span),
        scroll_offset_sum: lw
            .scroll_manager
            .state_keys()
            .into_iter()
            .filter_map(|(d, n)| lw.scroll_manager.get_current_offset(d, n))
            .map(|o| (f64::from(o.x.abs()) * 100.0) as i64 + (f64::from(o.y.abs()) * 100.0) as i64)
            .sum(),
        scroll_offsets: lw
            .scroll_manager
            .state_keys()
            .into_iter()
            .filter_map(|(d, n)| {
                let o = lw.scroll_manager.get_current_offset(d, n)?;
                Some((
                    (d.inner, n.index()),
                    (
                        (f64::from(o.x) * 100.0) as i64,
                        (f64::from(o.y) * 100.0) as i64,
                    ),
                ))
            })
            .collect(),
        selection_focus_node: lw.text_edit_manager.multi_cursor.as_ref().and_then(|mc| {
            mc.node_id
                .node
                .into_crate_internal()
                .map(|n| (mc.node_id.dom.inner, n.index()))
        }),
        text_selection_drag: matches!(
            lw.gesture_drag_manager
                .active_drag
                .as_ref()
                .map(|d| &d.drag_type),
            Some(azul_core::drag::ActiveDragType::TextSelection(_))
        ),
    };

    let mut guard = scratch(callback_info);
    let trace = guard
        .composition_trace
        .get_or_insert_with(CompositionTrace::default);

    let mut entered: Vec<&'static str> = Vec::new();
    if sample.drag_active {
        entered.push("drag_active");
    }
    if sample.scroll_animating {
        entered.push("scroll_animating");
    }
    if sample.focus_set {
        entered.push("focus_set");
    }
    if sample.editing_active {
        entered.push("editing_active");
    }
    if sample.hover_active {
        entered.push("hover_active");
    }
    if sample.damage_patch {
        entered.push("damage_patch");
    }
    if sample.damage_full {
        entered.push("damage_full");
    }
    match trace.prev.as_ref() {
        Some(prev) => {
            if sample.selection_span > prev.selection_span {
                entered.push("selection_grew");
            }
            if sample.scroll_offset_sum != prev.scroll_offset_sum {
                entered.push("scroll_started");
            }
        }
        None => {
            if sample.scroll_offset_sum != 0 {
                entered.push("scroll_started");
            }
        }
    }
    for name in entered {
        trace.entered.entry(name.to_string()).or_insert(step_index);
    }
    trace.prev2 = trace.prev.take();
    trace.prev = Some(sample);
    trace.samples += 1;
}

/// `assert_composition` — E2E_PLAN §(g1), "the managers fire together, in order,
/// and reach a fixpoint".
///
/// Asserts that each named stage was ENTERED over the steps since the last
/// checkpoint (test start, or `reset_frame_counters`), that they were entered in
/// the LISTED ORDER, and — unless `fixpoint: false` — that the whole thing then
/// settled (`assert_state_machines_idle`'s sweep).
///
/// Parameters: `expect` (array of stage names, required), `fixpoint` (bool,
/// default `true`), `damage` (bool, default `false` — whether the fixpoint check
/// also demands `FrameDamage::None`).
#[cfg(feature = "std")]
fn eval_assert_composition(
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> AssertionResult {
    if let Some(bad) = reject_unknown_params(
        "assert_composition",
        params,
        &["expect", "fixpoint", "damage"],
    ) {
        return bad;
    }
    let Some(serde_json::Value::Array(want)) = params.get("expect") else {
        return AssertionResult::fail(format!(
            "assert_composition: missing 'expect' (an array of stage names; known: {})",
            COMPOSITION_STAGES.join(", ")
        ));
    };
    let mut stages: Vec<String> = Vec::new();
    for v in want {
        let Some(s) = v.as_str() else {
            return AssertionResult::fail("assert_composition: 'expect' must be strings");
        };
        if !COMPOSITION_STAGES.contains(&s) {
            return AssertionResult::fail(format!(
                "assert_composition: unknown stage '{s}' (known: {})",
                COMPOSITION_STAGES.join(", ")
            ));
        }
        stages.push(s.to_string());
    }
    if stages.is_empty() {
        return AssertionResult::fail("assert_composition: 'expect' is empty — it asserts nothing");
    }

    let guard = scratch(callback_info);
    let Some(trace) = guard.composition_trace.as_ref() else {
        return AssertionResult::fail(
            "assert_composition: no composition trace was recorded (the step loop never sampled — \
             this op only works inside an E2E scenario run)",
        );
    };
    if trace.samples == 0 {
        return AssertionResult::fail(
            "assert_composition: the composition trace is empty (0 samples since the last \
             checkpoint)",
        );
    }

    let seen: Vec<String> = {
        let mut v: Vec<(usize, String)> =
            trace.entered.iter().map(|(k, i)| (*i, k.clone())).collect();
        v.sort_unstable();
        v.into_iter().map(|(i, k)| format!("{k}@{i}")).collect()
    };

    let missing: Vec<&str> = stages
        .iter()
        .filter(|s| !trace.entered.contains_key(s.as_str()))
        .map(String::as_str)
        .collect();
    if !missing.is_empty() {
        return AssertionResult::fail_with(
            format!(
                "assert_composition: {} of the expected stage(s) were never entered",
                missing.len()
            ),
            stages.join(" -> "),
            format!(
                "missing [{}]; observed [{}] over {} sample(s)",
                missing.join(", "),
                seen.join(", "),
                trace.samples
            ),
        );
    }

    let mut last = 0usize;
    for s in &stages {
        let at = trace.entered[s.as_str()];
        if at < last {
            return AssertionResult::fail_with(
                "assert_composition: the stages were entered OUT OF ORDER".to_string(),
                stages.join(" -> "),
                format!("observed [{}]", seen.join(", ")),
            );
        }
        last = at;
    }
    drop(guard);

    if params
        .get("fixpoint")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true)
    {
        let check_damage = params
            .get("damage")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let leaks = collect_state_machine_leaks(callback_info, check_damage);
        if !leaks.is_empty() {
            return AssertionResult::fail_with(
                format!(
                    "assert_composition: the stages all fired but the timeline never reached a \
                     fixpoint ({} state machine(s) still running)",
                    leaks.len()
                ),
                "every state machine idle after the last stage".to_string(),
                leaks.join("; "),
            );
        }
    }

    AssertionResult::pass(format!(
        "assert_composition: [{}] entered in order (observed [{}])",
        stages.join(" -> "),
        seen.join(", ")
    ))
}

// ---- assert_damage_sound ---------------------------------------------------

/// The damage-driven framebuffer of the last rendered frame, published by the
/// headless runner (`crate::e2e::runner`). `(width, height, RGBA)`.
///
/// This is the INCREMENTAL side of the plan's pixel-identity check; the full
/// repaint side is `render_current()` (`CallbackInfo::take_screenshot`, which
/// re-renders from scratch with a fresh glyph cache). A host that does not
/// publish it — the DLL, whose frames live on the GPU — makes
/// `"pixel_identity": true` FAIL rather than silently skip.
/// Publish the damage-driven framebuffer of the frame just rendered onto the
/// window that rendered it.
#[cfg(all(feature = "std", feature = "cpurender"))]
pub fn e2e_set_presented_frame(
    layout_window: &azul_layout::window::LayoutWindow,
    pixmap: &azul_layout::cpurender::AzulPixmap,
) {
    layout_window
        .e2e_scratch
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .presented_frame = Some((pixmap.width(), pixmap.height(), pixmap.data().to_vec()));
}

/// `assert_damage_sound` — E2E_PLAN §(c), damage soundness in BOTH directions.
///
/// The global, stronger form of `assert_damage_covers_changes`. It differs in
/// four ways:
///
/// 1. `assert_damage_covers_changes` PASSES TRIVIALLY when the damage is `Full`
///    ("a full repaint trivially covers every changed pixel"). Here a full
///    repaint is still measured for TIGHTNESS, so over-paint cannot hide behind
///    it, and `forbid_full` can reject it outright.
/// 2. It checks PRESENT ⊇ PAINT, the invariant `FrameReport` documents and
///    nothing asserted.
/// 3. It adds the plan's TIGHTNESS bound: `area(damage) <=
///    max_overpaint_ratio * area(bbox of the changed pixels)`.
/// 4. With `pixel_identity: true` it additionally compares the damage-driven
///    framebuffer against an independent full repaint — two different code paths
///    for the same function.
///
/// # Across a RESIZE
///
/// This used to bail out with "frame size changed between the snapshot and now
/// — the comparison is meaningless" whenever `vs` and the current frame had
/// different dimensions, which left the one repaint path that reuses pixels
/// across a buffer realloc (the grow fast path in `cpu_backend`) with NO
/// soundness net at all.
///
/// A dimension change does not make the comparison meaningless, it makes the
/// CHANGED SET bigger, and in a way that is exactly definable:
///
/// * inside the intersection of the two frames, a pixel changed iff it differs
///   from the snapshot — the ordinary test;
/// * every pixel of the new frame OUTSIDE that intersection is newly exposed,
///   so it changed BY DEFINITION (there is no previous value for it) and the
///   damage set must cover it.
///
/// Both halves are checked, so a grow that reuses the old pixels but forgets to
/// repaint a region the reflow moved is caught by the first half, and one that
/// forgets the newly-exposed L is caught by the second. `pixel_identity` needs
/// no adjustment at all: it compares the damage-driven framebuffer against a
/// FRESH full repaint at the CURRENT size, never against the snapshot.
///
/// Parameters: `vs` (snapshot name, required), `max_overpaint_ratio` (default
/// 4.0), `forbid_full` (default false), `threshold` (default 2), `slack_px`
/// (default 1), `pixel_identity` (default false).
#[cfg(feature = "std")]
#[allow(clippy::too_many_lines)]
fn eval_assert_damage_sound(
    params: &serde_json::Value,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> AssertionResult {
    if let Some(bad) = reject_unknown_params(
        "assert_damage_sound",
        params,
        &[
            "vs",
            "threshold",
            "slack_px",
            "max_overpaint_ratio",
            "forbid_full",
            "pixel_identity",
        ],
    ) {
        return bad;
    }
    #[cfg(not(feature = "cpurender"))]
    {
        let _ = (params, callback_info);
        return AssertionResult::fail("assert_damage_sound: cpurender feature not enabled");
    }
    #[cfg(feature = "cpurender")]
    {
        let Some(vs) = params.get("vs").and_then(|v| v.as_str()) else {
            return AssertionResult::fail(
                "assert_damage_sound: missing 'vs' (the snapshot_frame to diff against)",
            );
        };
        let threshold = params
            .get("threshold")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(2) as u8;
        let slack = params
            .get("slack_px")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(1.0) as f32;
        let max_overpaint = params
            .get("max_overpaint_ratio")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(4.0);
        let forbid_full = params
            .get("forbid_full")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        let report = frame_report_of(callback_info);
        let paint = report.accumulated_paint_damage.clone();
        let present = report.accumulated_present_damage.clone();

        if forbid_full && paint.is_full() {
            return AssertionResult::fail_with(
                "assert_damage_sound: the repaint was FULL, but this case is declared incremental"
                    .to_string(),
                "rects".to_string(),
                "full".to_string(),
            );
        }

        let before = match load_snapshot(vs, callback_info) {
            Ok(p) => p,
            Err(e) => return AssertionResult::fail(format!("assert_damage_sound: {e}")),
        };
        let after = match render_current(callback_info) {
            Ok(p) => p,
            Err(e) => return AssertionResult::fail(format!("assert_damage_sound: {e}")),
        };
        // The overlap of the snapshot and the current frame. Outside it every
        // pixel of the current frame is newly exposed and therefore CHANGED —
        // see the "Across a RESIZE" section on this function.
        let overlap_w = before.width().min(after.width()) as usize;
        let overlap_h = before.height().min(after.height()) as usize;
        let resized = before.width() != after.width() || before.height() != after.height();

        let logical_w = callback_info
            .get_current_window_state()
            .size
            .dimensions
            .width
            .max(1.0);
        let scale = after.width() as f32 / logical_w;
        let to_px = |d: &azul_layout::window::FrameDamage| -> Vec<(f32, f32, f32, f32)> {
            d.rects()
                .unwrap_or(&[])
                .iter()
                .map(|r| {
                    (
                        r.origin.x * scale - slack,
                        r.origin.y * scale - slack,
                        (r.origin.x + r.size.width) * scale + slack,
                        (r.origin.y + r.size.height) * scale + slack,
                    )
                })
                .collect()
        };
        let paint_rects = to_px(&paint);
        let present_rects = to_px(&present);

        // ---- (1) COVERAGE: every changed pixel lies inside the paint damage --
        let (bd, ad) = (before.data(), after.data());
        let w = after.width() as usize;
        let h = after.height() as usize;
        let before_stride = before.width() as usize;
        let mut changed = 0u64;
        let mut uncovered = 0u64;
        let mut first: Option<(usize, usize)> = None;
        let (mut bx0, mut by0, mut bx1, mut by1) = (usize::MAX, usize::MAX, 0usize, 0usize);
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                // Inside the overlap the snapshot has a previous value for this
                // pixel, so "changed" is the ordinary per-channel diff. Outside
                // it there IS no previous value — the pixel is newly exposed by
                // the resize and counts as changed unconditionally.
                if x < overlap_w && y < overlap_h {
                    let bi = (y * before_stride + x) * 4;
                    if !(0..4).any(|c| bd[bi + c].abs_diff(ad[i + c]) > threshold) {
                        continue;
                    }
                }
                changed += 1;
                bx0 = bx0.min(x);
                by0 = by0.min(y);
                bx1 = bx1.max(x + 1);
                by1 = by1.max(y + 1);
                if paint.is_full() {
                    continue;
                }
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                if !paint_rects
                    .iter()
                    .any(|(x0, y0, x1, y1)| px >= *x0 && px < *x1 && py >= *y0 && py < *y1)
                {
                    uncovered += 1;
                    if first.is_none() {
                        first = Some((x, y));
                    }
                }
            }
        }
        if uncovered > 0 {
            let (fx, fy) = first.unwrap_or((0, 0));
            return AssertionResult::fail_with(
                "assert_damage_sound: UNDER-PAINT — the damage set does not cover every changed \
                 pixel, so those pixels are stale on a real screen"
                    .to_string(),
                "0 uncovered changed pixels".to_string(),
                format!("{uncovered} uncovered of {changed} changed, first at ({fx}, {fy})"),
            );
        }
        if changed > 0 && paint.is_none() {
            return AssertionResult::fail_with(
                format!("assert_damage_sound: {changed} pixels changed but NO damage was reported"),
                "damage != none".to_string(),
                "none".to_string(),
            );
        }

        // ---- (2) PRESENT ⊇ PAINT --------------------------------------------
        if !present.is_full() && !paint.is_full() {
            for (x0, y0, x1, y1) in &paint_rects {
                let (cx, cy) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
                let covered = present_rects
                    .iter()
                    .any(|(a, b, c, d)| cx >= *a && cx < *c && cy >= *b && cy < *d);
                if !covered {
                    return AssertionResult::fail_with(
                        "assert_damage_sound: a PAINT rect is not inside the PRESENT damage — a \
                         region was rasterised but never pushed to the screen"
                            .to_string(),
                        "present ⊇ paint".to_string(),
                        format!(
                            "paint rect ({x0:.1},{y0:.1})-({x1:.1},{y1:.1}) outside the {} present \
                             rect(s)",
                            present_rects.len()
                        ),
                    );
                }
            }
        }

        // ---- (3) TIGHTNESS ---------------------------------------------------
        let mut tightness = String::from("n/a (nothing changed)");
        if changed > 0 {
            #[allow(clippy::cast_precision_loss)]
            let bbox_area = ((bx1 - bx0) * (by1 - by0)) as f64;
            // `FrameDamage::area` already answers `window_area` for `Full`.
            let logical_area = window_logical_area(callback_info);
            let damage_area =
                f64::from(paint.area(logical_area).max(0.0)) * f64::from(scale * scale);
            let ratio = damage_area / bbox_area.max(1.0);
            tightness = format!("{ratio:.2}x the changed bbox");
            if ratio > max_overpaint {
                return AssertionResult::fail_with(
                    format!(
                        "assert_damage_sound: OVER-PAINT — the {} damage is far larger than the \
                         region that actually changed",
                        damage_kind_str(&paint)
                    ),
                    format!("area <= {max_overpaint:.2} x the changed bbox"),
                    format!(
                        "{ratio:.2}x (damage {damage_area:.0} px^2, changed bbox {bbox_area:.0} \
                         px^2 around ({bx0},{by0})-({bx1},{by1}))"
                    ),
                );
            }
        }

        // ---- (4) PIXEL IDENTITY (opt-in) ------------------------------------
        let mut identity = String::new();
        if params
            .get("pixel_identity")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
            let presented = scratch(callback_info).presented_frame.clone();
            let Some((pw, ph, data)) = presented else {
                return AssertionResult::fail(
                    "assert_damage_sound: 'pixel_identity' was requested but this host does not \
                     publish the damage-driven framebuffer (only the headless runner does). \
                     Refusing to skip the check silently.",
                );
            };
            if pw != after.width() || ph != after.height() {
                return AssertionResult::fail_with(
                    "assert_damage_sound: the damage-driven framebuffer has a different size than \
                     the full repaint"
                        .to_string(),
                    format!("{}x{}", after.width(), after.height()),
                    format!("{pw}x{ph}"),
                );
            }
            let ad2 = after.data();
            let mut diff = 0u64;
            let mut firstd: Option<usize> = None;
            for (i, (a, b)) in data.iter().zip(ad2.iter()).enumerate() {
                if a.abs_diff(*b) > threshold {
                    diff += 1;
                    if firstd.is_none() {
                        firstd = Some(i / 4);
                    }
                }
            }
            if diff > 0 {
                let p = firstd.unwrap_or(0);
                // AZ_E2E_DUMP=1: print the divergent pixels' actual channel
                // values (incremental vs fresh) with 2px of context — the
                // datum every soundness debugging session starts by needing.
                if std::env::var_os("AZ_E2E_DUMP").is_some() {
                    let (fx, fy) = (p % w, p / w);
                    for y in fy.saturating_sub(1)..=(fy + 1).min(h - 1) {
                        for x in fx.saturating_sub(2)..=(fx + 2).min(w - 1) {
                            let i = (y * w + x) * 4;
                            eprintln!(
                                "[dsound] px({x},{y}): incr={:?} fresh={:?}",
                                &data[i..i + 4],
                                &ad2[i..i + 4]
                            );
                        }
                    }
                }
                return AssertionResult::fail_with(
                    "assert_damage_sound: the incrementally-repainted buffer does NOT match an \
                     independent full repaint — the incremental path produced different pixels"
                        .to_string(),
                    "pixel-identical".to_string(),
                    format!(
                        "{diff} channel(s) differ, first at pixel ({}, {})",
                        p % w,
                        p / w
                    ),
                );
            }
            identity = String::from(", pixel-identical to a full repaint");
        }

        let across = if resized {
            format!(
                " [across a resize {}x{} -> {}x{}: every px outside the overlap counted as changed]",
                before.width(),
                before.height(),
                after.width(),
                after.height()
            )
        } else {
            String::new()
        };
        AssertionResult::pass(format!(
            "assert_damage_sound: {changed} changed px all covered by the {} paint damage ({} \
             rect(s)), present ⊇ paint, tightness {tightness}{identity}{across}",
            damage_kind_str(&paint),
            paint.rect_count()
        ))
    }
}

// ==================== E2E Continuation ====================

/// Resume an E2E test run that was paused for relayout.
///
/// Re-enters the step loop from where the previous tick left off.
/// Sends the final merged results through `cont.response_tx` when done,
/// or saves a new continuation for the next tick if another yield is needed.
#[cfg(feature = "std")]
fn resume_e2e_continuation(
    cont: E2eContinuation,
    callback_info: &mut azul_layout::callbacks::CallbackInfo,
    session: &mut E2eSession,
) -> bool {
    // `running` is what makes a nested `run_e2e_tests` step fail loudly instead
    // of silently stealing this window's single continuation slot. Set around
    // the whole body so every early `return` clears it.
    session.running = true;
    let needs_update = resume_e2e_continuation_inner(cont, callback_info, session);
    session.running = false;
    needs_update
}

#[cfg(feature = "std")]
fn resume_e2e_continuation_inner(
    mut cont: E2eContinuation,
    callback_info: &mut azul_layout::callbacks::CallbackInfo,
    session: &mut E2eSession,
) -> bool {
    let mut needs_update = false;
    let mut app_data = cont.app_data.clone();

    // Continue from cont.test_idx / cont.step_idx
    let total_tests = cont.tests.len();
    while cont.test_idx < total_tests {
        let test = &cont.tests[cont.test_idx];
        let continue_on_failure = test.config.continue_on_failure;

        // Start new test if step_idx == 0
        if cont.step_idx == 0 && !cont.setup_applied {
            cont.current_step_results.clear();
            cont.current_test_failed = false;
            cont.test_start = wall_clock_now();
            // Name the scenario for as long as it runs. Every diagnostic the
            // ENGINE emits meanwhile — image-churn, text-without-block, any
            // future lint — is tagged with it, so a Grafana query on
            // `test="..."` returns that scenario's whole story rather than a
            // slice of one flat stream. Like the composition trace above, this
            // is process-global and must be set per test.
            azul_core::diagnostics::set_scope(Some(test.name.clone()));
            // The composition trace is process-global (an assertion only ever
            // holds `&CallbackInfo`), so it must be zeroed per test or stages
            // from the previous scenario would leak into this one.
            e2e_reset_composition_trace(callback_info);
        }

        // Apply the test's `setup` block (window size / DPI / app state) BEFORE
        // step 0. This used to be dead data: `E2eSetup` was deserialized and
        // never read, so `window_width` / `window_height` never reached the
        // window and every scenario rendered at the default size.
        //
        // The resize goes through the normal `modify_window_state` channel (a
        // CallbackChange the shell applies + relayouts), so we must YIELD right
        // after pushing it — otherwise step 0 would run against the pre-resize
        // layout, which is exactly the trap `has_pending_relayout_change()`
        // exists for.
        if cont.step_idx == 0 && !cont.setup_applied {
            cont.setup_applied = true;
            if let Some(setup) = test.setup.clone() {
                let mut new_state = callback_info.get_current_window_state().clone();
                new_state.size.dimensions = azul_core::geom::LogicalSize::new(
                    setup.window_width as f32,
                    setup.window_height as f32,
                );
                new_state.size.dpi = setup.dpi;
                callback_info.modify_window_state(new_state);

                if let Some(state) = setup.app_state.clone() {
                    let mut cmd = serde_json::Map::new();
                    cmd.insert(
                        "op".into(),
                        serde_json::Value::String("set_app_state".into()),
                    );
                    cmd.insert("state".into(), state);
                    if let Ok(ev) =
                        serde_json::from_value::<DebugEvent>(serde_json::Value::Object(cmd))
                    {
                        let (tx, _rx) = mpsc::channel();
                        let req = DebugRequest {
                            request_id: NEXT_REQUEST_ID.fetch_add(1, Ordering::SeqCst),
                            event: ev,
                            window_id: cont.window_id.clone(),
                            wait_for_render: false,
                            dom_id: None,
                            response_tx: tx,
                        };
                        let _ = process_debug_event(
                            &req,
                            callback_info,
                            &mut app_data,
                            &cont.component_map,
                            session,
                        );
                    }
                }

                log(
                    LogLevel::Debug,
                    LogCategory::DebugServer,
                    format!(
                        "[E2E] setup: window {}x{} @ {} dpi (yield for relayout)",
                        setup.window_width, setup.window_height, setup.dpi
                    ),
                    None,
                );

                // Yield so the resize lands (and layout re-runs) before step 0.
                cont.app_data = app_data;
                session.pending = Some(cont);
                return true;
            }
        }

        while cont.step_idx < test.steps.len() {
            let step = &test.steps[cont.step_idx];
            let step_index = cont.step_idx;
            let step_start = wall_clock_now();
            let op = step.op.as_str();

            // Sample the manager state for `assert_composition`'s stage trace.
            // Taken BEFORE the step runs, so it observes the previous step's
            // effects after the shell serviced them (a state-changing op yields,
            // and the resume lands here).
            e2e_record_composition_sample(step_index, callback_info);

            // `wait` yields with a resume deadline instead of sleeping on the
            // event-loop thread (an inline sleep would block the very relayout
            // the wait is for; see resume_not_before).
            if op == "wait" {
                let ms = step.params.get("ms").and_then(|v| v.as_u64()).unwrap_or(0);
                // Advance the injectable clock rather than sleeping for real.
                //
                // `Instant::now()` is `StdInstant::now() + test_offset`, so to
                // everything time-driven in the engine — timers, cursor blink,
                // scrollbar fade, animations — a 250 ms real sleep and a 250 ms
                // offset bump are INDISTINGUISHABLE. The difference is that one
                // costs 250 ms of wall clock at every `wait` in every scenario and
                // reproduces only on a machine as fast as the one that recorded it,
                // and the other costs nothing and is exact on any runner at any
                // load. With a corpus this size the sleeping version cannot fit in
                // CI at all: this one scenario alone slept ~900 ms of its 1.2 s.
                let total = azul_core::task::advance_test_clock_ms(ms);
                cont.current_step_results.push(E2eStepResult {
                    step_index,
                    op: op.to_string(),
                    status: "pass".into(),
                    duration_ms: step_start.elapsed().as_millis() as u64,
                    logs: vec![format!("waited {ms} ms (virtual clock, now +{total} ms)")],
                    screenshot: None,
                    error: None,
                    response: None,
                });
                cont.step_idx = step_index + 1;
                cont.app_data = app_data;
                // Still YIELD, with no deadline. What a `wait` owes its scenario is
                // one turn of the shell's loop — `service()` applies the pending
                // redraw, pumps timers and regenerates a dirty DOM — and the yield
                // itself is what delivers that. Making the guarantee structural
                // rather than a side effect of how long we happened to sleep is the
                // whole point: an unoptimized build no longer gets less progress
                // out of a wait than an optimized one.
                cont.resume_not_before = None;
                session.pending = Some(cont);
                return needs_update;
            }

            // `assert_response` asserts on the RESPONSE PAYLOAD of the previous
            // step. It lives here, not in `evaluate_assertion`, because it is the
            // only assertion that needs the step history (`cont`) rather than the
            // engine state.
            //
            // WHY IT EXISTS: a QUERY op (get_dom, get_state, …) has no side effect
            // — its whole product is the response. Every other assertion re-reads
            // the engine, so it would pass even if the op returned nothing at all.
            // That is exactly the zombie failure mode: the `_ => Unhandled`
            // catch-all answers `ok` with `data: None`. This is the assertion that
            // catches it: `{"op":"get_dom"}, {"op":"assert_response","type":"dom"}`
            // goes RED against a zombie and green against a real handler.
            //
            // Params: `type` (the ResponseData tag, e.g. "dom") and/or `contains`
            // (substring of the serialized response JSON).
            if op == "assert_response" {
                let prev = cont
                    .current_step_results
                    .iter()
                    .rev()
                    .find(|r| r.op != "assert_response");
                let response = prev.and_then(|r| r.response.as_ref());

                let result = match response {
                    None => AssertionResult::fail_with(
                        "assert_response: the previous step returned NO response data (an op \
                         that answers `ok` with nothing is doing nothing — see the zombie-op \
                         catch-all in process_debug_event)"
                            .to_string(),
                        "a response payload".to_string(),
                        "null".to_string(),
                    ),
                    Some(resp) => {
                        let text = resp.to_string();
                        let want_type = step.params.get("type").and_then(|v| v.as_str());
                        let want_sub = step.params.get("contains").and_then(|v| v.as_str());
                        // Same convention as assert_dom: a substring that
                        // must NOT occur. This is how a counter asserts
                        // "nonzero" when its exact value is wall-clock-paced
                        // (the redraw pump ticks animations between ops).
                        let want_not = step.params.get("not_contains").and_then(|v| v.as_str());
                        let actual_type = resp
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("<none>");

                        if want_type.is_none() && want_sub.is_none() && want_not.is_none() {
                            AssertionResult::fail(
                                "assert_response: needs 'type', 'contains' and/or 'not_contains'",
                            )
                        } else if want_type.is_some_and(|t| t != actual_type) {
                            AssertionResult::fail_with(
                                "assert_response: wrong response type".to_string(),
                                want_type.unwrap_or_default().to_string(),
                                actual_type.to_string(),
                            )
                        } else if want_not.is_some_and(|s| text.contains(s)) {
                            AssertionResult::fail_with(
                                "assert_response: response CONTAINS a forbidden substring"
                                    .to_string(),
                                format!("absence of {}", want_not.unwrap_or_default()),
                                text,
                            )
                        } else if want_sub.is_some_and(|s| !text.contains(s)) {
                            AssertionResult::fail_with(
                                "assert_response: response does not contain the expected \
                                 substring"
                                    .to_string(),
                                want_sub.unwrap_or_default().to_string(),
                                text,
                            )
                        } else {
                            AssertionResult::pass(format!(
                                "assert_response: type='{actual_type}' ({} bytes)",
                                text.len()
                            ))
                        }
                    }
                };

                if result.passed {
                    cont.current_step_results.push(E2eStepResult {
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
                    cont.current_test_failed = true;
                    let error_msg = if let (Some(ref exp), Some(ref act)) =
                        (&result.expected, &result.actual)
                    {
                        format!("{}: expected {}, got {}", result.message, exp, act)
                    } else {
                        result.message.clone()
                    };
                    cont.current_step_results.push(E2eStepResult {
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

                cont.step_idx = step_index + 1;
                if cont.current_test_failed && !continue_on_failure {
                    break;
                }
                continue;
            }

            // Assertion steps
            if op.starts_with("assert_") {
                let result = evaluate_assertion(op, &step.params, callback_info, &app_data);
                if result.passed {
                    cont.current_step_results.push(E2eStepResult {
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
                    cont.current_test_failed = true;
                    let error_msg = if let (Some(ref exp), Some(ref act)) =
                        (&result.expected, &result.actual)
                    {
                        format!("{}: expected {}, got {}", result.message, exp, act)
                    } else {
                        result.message.clone()
                    };

                    // Report the failure through telemetry, tagged with the
                    // scenario and the step that produced it. At Error severity
                    // it stands out in Grafana the way a crash does, and the
                    // `test` attribute is what lets a dashboard show ONE
                    // scenario's story instead of a flat stream. Silently does
                    // nothing when telemetry is not configured, so a local
                    // `cargo test` pays nothing for it.
                    #[cfg(feature = "telemetry")]
                    crate::telemetry::report_e2e_failure(
                        &azul_core::diagnostics::current_scope()
                            .unwrap_or_else(|| "<unnamed>".to_string()),
                        op,
                        &error_msg,
                    );
                    cont.current_step_results.push(E2eStepResult {
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
            } else if op == "commit_undo_snapshot"
                || op == "undo_app_state"
                || op == "redo_app_state"
            {
                // App-state undo/redo history (mini-git) on the session's app_data,
                // via the shared RefAnyUndoManager. Exposes the undo system to E2E
                // JSON so it can be tested end-to-end from the outside.
                let ok = match op {
                    "commit_undo_snapshot" => cont.undo_manager.commit(&app_data),
                    "undo_app_state" => cont.undo_manager.undo(&mut app_data),
                    "redo_app_state" => cont.undo_manager.redo(&mut app_data),
                    _ => false,
                };
                cont.current_step_results.push(E2eStepResult {
                    step_index,
                    op: op.to_string(),
                    status: if ok { "pass".into() } else { "fail".into() },
                    duration_ms: step_start.elapsed().as_millis() as u64,
                    logs: vec![format!("{} -> {}", op, ok)],
                    screenshot: None,
                    error: if ok {
                        None
                    } else {
                        Some(format!(
                            "'{}' did nothing (no JSON serializer, or empty history)",
                            op
                        ))
                    },
                    response: None,
                });
            } else {
                // Regular debug command
                let mut cmd = serde_json::Map::new();
                cmd.insert("op".to_string(), serde_json::Value::String(op.to_string()));
                if let serde_json::Value::Object(map) = &step.params {
                    for (k, v) in map {
                        if k != "op" && k != "screenshot" {
                            cmd.insert(k.clone(), v.clone());
                        }
                    }
                }
                // `dom_id` is an ENVELOPE field, not an op field (see
                // `DebugRequest::dom_id`), so a scenario step spells it inline
                // and it is lifted out here — otherwise `from_value::<DebugEvent>`
                // would drop it silently and the step would address DOM 0.
                let step_dom_id = cmd.get("dom_id").and_then(serde_json::Value::as_u64);
                cmd.remove("dom_id");
                let cmd_json = serde_json::Value::Object(cmd);
                match serde_json::from_value::<DebugEvent>(cmd_json) {
                    Ok(debug_event) => {
                        let (step_tx, step_rx) = mpsc::channel();
                        let step_request = DebugRequest {
                            request_id: NEXT_REQUEST_ID.fetch_add(1, Ordering::SeqCst),
                            event: debug_event,
                            window_id: cont.window_id.clone(),
                            wait_for_render: false,
                            dom_id: step_dom_id,
                            response_tx: step_tx,
                        };
                        let step_needs_update = process_debug_event(
                            &step_request,
                            callback_info,
                            &mut app_data,
                            &cont.component_map,
                            session,
                        );
                        if step_needs_update {
                            needs_update = true;
                        }
                        // Yield whenever the op left a change the SHELL has to
                        // service (a window-state change, a scroll, a queued
                        // input sequence), whether or not it also asked for a
                        // DOM refresh. It used to yield only when both were
                        // true, so an op that deliberately pushes a state change
                        // *without* `needs_update` — `set_window_state`,
                        // `tick_ms`, `wait_frame` — ran every following step
                        // against the state from before it, which is exactly the
                        // trap `has_pending_relayout_change` exists to avoid.
                        if callback_info.has_pending_relayout_change() {
                            // Yield: save progress and return
                            cont.current_step_results.push(E2eStepResult {
                                step_index,
                                op: op.to_string(),
                                status: "pass".into(),
                                duration_ms: step_start.elapsed().as_millis() as u64,
                                logs: vec![format!("Executed: {} (yield for relayout)", op)],
                                screenshot: None,
                                error: None,
                                response: None,
                            });
                            cont.step_idx = step_index + 1;
                            cont.app_data = app_data;
                            session.pending = Some(cont);
                            // NOT an unconditional `true`: a repaint-only yield
                            // (`tick_ms` / `wait_frame`) must not be upgraded
                            // into a DOM regeneration on the way out.
                            return needs_update;
                        }
                        // Record result
                        match step_rx.try_recv() {
                            Ok(DebugResponseData::Ok { data, .. }) => {
                                cont.current_step_results.push(E2eStepResult {
                                    step_index,
                                    op: op.to_string(),
                                    status: "pass".into(),
                                    duration_ms: step_start.elapsed().as_millis() as u64,
                                    logs: vec![format!("Executed: {}", op)],
                                    screenshot: None,
                                    error: None,
                                    response: data
                                        .as_ref()
                                        .and_then(|d| serde_json::to_value(d).ok()),
                                });
                            }
                            Ok(DebugResponseData::Err(msg)) => {
                                cont.current_test_failed = true;
                                cont.current_step_results.push(E2eStepResult {
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
                                cont.current_step_results.push(E2eStepResult {
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
                        cont.current_test_failed = true;
                        cont.current_step_results.push(E2eStepResult {
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

            cont.step_idx += 1;

            // Apply delay between steps if configured (for visual inspection)
            if test.config.delay_between_steps_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(
                    test.config.delay_between_steps_ms,
                ));
            }

            if cont.current_test_failed && !continue_on_failure {
                break;
            }
        }

        // Finalize current test
        let steps_passed = cont
            .current_step_results
            .iter()
            .filter(|s| s.status == "pass")
            .count();
        let steps_failed = cont
            .current_step_results
            .iter()
            .filter(|s| s.status == "fail")
            .count();
        cont.completed_results.push(E2eTestResult {
            name: test.name.clone(),
            status: if cont.current_test_failed {
                "fail"
            } else {
                "pass"
            }
            .into(),
            duration_ms: cont.test_start.elapsed().as_millis() as u64,
            step_count: test.steps.len(),
            steps_passed,
            steps_failed,
            steps: std::mem::take(&mut cont.current_step_results),
            final_screenshot: None,
        });

        // Close the scenario out: one verdict record carrying `test`,
        // `kind=e2e_result` and `passed`, so a dashboard can count outcomes
        // without parsing message bodies. Then drop the scope, or the NEXT
        // scenario's engine diagnostics would be filed under this one's name —
        // which is worse than no tag, because it looks right.
        #[cfg(feature = "telemetry")]
        crate::telemetry::report_e2e_result(&test.name, steps_failed == 0, test.steps.len());
        azul_core::diagnostics::set_scope(None);

        cont.test_idx += 1;
        cont.step_idx = 0;
        // The next test gets its own `setup` block applied.
        cont.setup_applied = false;
    }

    // All tests done — send results
    let _ = cont.response_tx.send(DebugResponseData::Ok {
        window_state: None,
        data: Some(ResponseData::E2eResults(E2eResultsResponse {
            results: cont.completed_results,
        })),
    });

    needs_update
}

/// Headless driver seam: resume any pending E2E continuation exactly ONCE.
///
/// This is the same per-tick work `debug_timer_callback` does (take the stored
/// continuation, honor its `wait` deadline, resume it), minus the spmc request
/// drain. It lets the headless runner (`crate::e2e::runner`) pump the REAL
/// scenario runner (`resume_e2e_continuation`) without a platform timer or any
/// networking — the private `E2eContinuation` never leaves this module.
///
/// Returns `(needs_update, still_pending, resume_not_before)`:
/// * `needs_update` — a step mutated state and the caller must relayout;
/// * `still_pending` — the continuation yielded again (more steps remain);
/// * `resume_not_before` — a `wait` deadline the caller should honor before the
///   next pump.
#[cfg(feature = "std")]
pub fn e2e_pump_continuation(
    callback_info: &mut azul_layout::callbacks::CallbackInfo,
    session: &mut E2eSession,
) -> (bool, bool, Option<std::time::Instant>) {
    let mut pending = session.pending.take();
    // `wait` steps yield with a deadline — if it hasn't passed, put the
    // continuation back untouched and report it as still pending.
    if let Some(cont) =
        pending.take_if(|c| c.resume_not_before.is_some_and(|t| wall_clock_now() < t))
    {
        let rnb = cont.resume_not_before;
        session.pending = Some(cont);
        return (false, true, rnb);
    }
    let Some(mut cont) = pending else {
        return (false, false, None);
    };
    cont.resume_not_before = None;
    let needs_update = resume_e2e_continuation(cont, callback_info, session);
    (
        needs_update,
        session.is_pending(),
        session.resume_not_before(),
    )
}

// ==================== Timer Callback ====================

/// Timer callback that processes debug requests.
/// Called every ~16ms when debug mode is enabled.
#[cfg(feature = "std")]
#[cfg(feature = "e2e-server")]
pub extern "C" fn debug_timer_callback(
    mut timer_data: azul_core::refany::RefAny,
    mut timer_info: azul_layout::timer::TimerCallbackInfo,
) -> azul_core::callbacks::TimerCallbackReturn {
    use azul_core::callbacks::{TimerCallbackReturn, Update};
    use azul_core::task::TerminateTimer;

    // Downcast the RefAny to DebugTimerData to get app_data + channel + this
    // window's E2E session. The session is MOVED OUT for the duration of the
    // tick and put back at the end: the dispatcher needs `&mut` access to it
    // while `timer_data`'s exclusive borrow must not stay live across the
    // callbacks below.
    let (mut app_data, component_map, request_rx, my_window_id, mut session) = {
        let mut dtd = match timer_data.downcast_mut::<DebugTimerData>() {
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
        (
            dtd.app_data.clone(),
            dtd.component_map.clone(),
            dtd.request_rx.clone(),
            dtd.window_id.clone(),
            core::mem::take(&mut dtd.session),
        )
    };

    // Check for E2E continuation from a previous tick (resume after relayout)
    let mut needs_update = false;
    let mut pending_continuation = session.pending.take();
    // `wait` steps yield with a deadline — if it hasn't passed, put the
    // continuation back and let this tick process queued input/relayout.
    if let Some(cont) =
        pending_continuation.take_if(|c| c.resume_not_before.is_some_and(|t| wall_clock_now() < t))
    {
        session.pending = Some(cont);
    }
    if let Some(mut continuation) = pending_continuation {
        continuation.resume_not_before = None;
        log(
            LogLevel::Debug,
            LogCategory::DebugServer,
            format!(
                "[E2E] Resuming continuation: test {}, step {}",
                continuation.test_idx, continuation.step_idx
            ),
            None,
        );
        // `session.pending` is empty here — `resume_e2e_continuation` refills it
        // if another yield is needed.
        let result =
            resume_e2e_continuation(continuation, &mut timer_info.callback_info, &mut session);
        needs_update = needs_update || result;
    }

    // Drain all available requests from the SPMC channel
    let mut processed_count = 0;

    while let Ok(request) = request_rx.try_recv() {
        // Window-targeted routing
        if let Some(ref target_id) = request.window_id {
            if target_id != &my_window_id {
                // Not for us — but SPMC already consumed it.
                // Send error so HTTP thread doesn't hang forever.
                send_err(
                    &request,
                    format!(
                        "Request targeted window '{}' but was consumed by '{}'",
                        target_id, my_window_id
                    ),
                );
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
        let result = process_debug_event(
            &request,
            &mut timer_info.callback_info,
            &mut app_data,
            &component_map,
            &mut session,
        );
        needs_update = needs_update || result;
        processed_count += 1;
    }

    // Hand the session back to the timer's `RefAny` so the next tick resumes
    // exactly where this one left off.
    if let Some(mut dtd) = timer_data.downcast_mut::<DebugTimerData>() {
        dtd.session = session;
    }

    if processed_count > 0 {
        log(
            LogLevel::Debug,
            LogCategory::DebugServer,
            format!(
                "[timer] Processed {} request(s), needs_update={}",
                processed_count, needs_update
            ),
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
                        x: bounds.0.origin.x,
                        y: bounds.0.origin.y,
                        width: bounds.0.size.width,
                        height: bounds.0.size.height,
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
                ..
            } => {
                scroll_depth += 1;
                Some(ClipOperation {
                    index: idx,
                    op: "PushScrollFrame".to_string(),
                    clip_depth,
                    scroll_depth,
                    stacking_depth,
                    bounds: Some(LogicalRectJson {
                        x: clip_bounds.0.origin.x,
                        y: clip_bounds.0.origin.y,
                        width: clip_bounds.0.size.width,
                        height: clip_bounds.0.size.height,
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
                        x: bounds.0.origin.x,
                        y: bounds.0.origin.y,
                        width: bounds.0.size.width,
                        height: bounds.0.size.height,
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
    let source_line: Option<u32> = None;
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
                    file_name = Some(
                        CStr::from_ptr(info.dli_fname)
                            .to_string_lossy()
                            .into_owned(),
                    );
                }
                if !info.dli_sname.is_null() {
                    let raw = CStr::from_ptr(info.dli_sname)
                        .to_string_lossy()
                        .into_owned();
                    // Strip leading underscore (macOS C name-mangling convention)
                    let clean = raw.strip_prefix('_').unwrap_or(&raw).to_string();
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
                if sym[pos + 3..].chars().all(|c| c.is_ascii_hexdigit()) {
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
    use azul_core::xml::{ComponentMap, ComponentSource};

    let mut libraries = Vec::new();

    for lib in map_ref.libraries.iter() {
        let mut components = Vec::new();

        for def in lib.components.iter() {
            let tag = def.id.name.as_str();

            // --- data model (component-specific attributes) ---
            let mut data_model: Vec<ComponentDataFieldInfo> = def
                .data_model
                .fields
                .as_ref()
                .iter()
                .filter(|f| {
                    !matches!(
                        f.field_type,
                        azul_core::xml::ComponentFieldType::Callback(..)
                    )
                })
                .map(|f| ComponentDataFieldInfo {
                    name: f.name.as_str().to_string(),
                    field_type: field_type_to_string(&f.field_type),
                    field_type_structured: field_type_to_structured(&f.field_type),
                    default: default_value_to_opt_string(&f.default_value),
                    required: f.required,
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
                            field_type_structured: StructuredFieldType::Primitive {
                                name: attr_type.to_string(),
                            },
                            default: None,
                            required: false,
                            description: String::new(),
                        });
                    }
                }
            }

            // --- universal HTML attributes (separate) ---
            let universal_attributes: Vec<ComponentAttributeInfo> =
                if def.source == ComponentSource::Builtin {
                    get_universal_attributes()
                        .into_iter()
                        .map(|(name, atype)| ComponentAttributeInfo {
                            name: name.to_string(),
                            attr_type: atype.to_string(),
                            default: None,
                            description: String::new(),
                        })
                        .collect()
                } else {
                    Vec::new()
                };

            // --- callback slots (extracted from data_model.fields with Callback type) ---
            let callback_slots: Vec<ComponentCallbackSlotInfo> = def
                .data_model
                .fields
                .as_ref()
                .iter()
                .filter_map(|f| {
                    if let azul_core::xml::ComponentFieldType::Callback(ref signature) =
                        f.field_type
                    {
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
                source: source_str.to_string(),
                data_model,
                universal_attributes,
                callback_slots,
                css: def.css.as_str().to_string(),
            });
        }

        // Sort components within the library by tag name
        components.sort_by(|a, b| a.tag.cmp(&b.tag));

        // Build data model infos from library-level data models
        let data_models: Vec<DataModelInfo> = lib
            .data_models
            .as_ref()
            .iter()
            .map(|dm| DataModelInfo {
                name: dm.name.as_str().to_string(),
                description: dm.description.as_str().to_string(),
                fields: dm
                    .fields
                    .as_ref()
                    .iter()
                    .map(|f| ComponentDataFieldInfo {
                        name: f.name.as_str().to_string(),
                        field_type: field_type_to_string(&f.field_type),
                        field_type_structured: field_type_to_structured(&f.field_type),
                        default: default_value_to_opt_string(&f.default_value),
                        required: f.required,
                        description: f.description.as_str().to_string(),
                    })
                    .collect(),
            })
            .collect();

        // Build enum model infos from library-level enum models
        let enum_models: Vec<EnumModelInfo> = lib
            .enum_models
            .as_ref()
            .iter()
            .map(|em| EnumModelInfo {
                name: em.name.as_str().to_string(),
                description: em.description.as_str().to_string(),
                variants: em
                    .variants
                    .as_ref()
                    .iter()
                    .map(|v| EnumVariantInfo {
                        name: v.name.as_str().to_string(),
                        description: v.description.as_str().to_string(),
                        fields: v
                            .fields
                            .as_ref()
                            .iter()
                            .map(|f| ComponentDataFieldInfo {
                                name: f.name.as_str().to_string(),
                                field_type: field_type_to_string(&f.field_type),
                                field_type_structured: field_type_to_structured(&f.field_type),
                                default: default_value_to_opt_string(&f.default_value),
                                required: f.required,
                                description: f.description.as_str().to_string(),
                            })
                            .collect(),
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
            enum_models,
            components,
        });
    }

    ComponentRegistryResponse { libraries }
}

/// Convert a StyledDom into a JSON-serializable render tree for the mini HTML tree widget.
/// The result is a JSON object with a "nodes" array of root-level nodes,
/// each with "tag", "text", "classes", "children" fields.
#[cfg(feature = "std")]
fn styled_dom_to_render_tree(styled_dom: &azul_core::styled_dom::StyledDom) -> serde_json::Value {
    use azul_core::id::node_id::NodeId;
    use serde_json::json;

    let node_data_vec = &styled_dom.node_data;
    let node_hierarchy = &styled_dom.node_hierarchy;

    if node_data_vec.is_empty() || node_hierarchy.is_empty() {
        return json!({ "nodes": [] });
    }

    // Build a recursive tree from the flat node arrays
    fn build_tree(
        node_id: NodeId,
        node_data_vec: &azul_core::dom::NodeDataVec,
        node_hierarchy: &azul_core::styled_dom::NodeHierarchyItemVec,
    ) -> serde_json::Value {
        use serde_json::json;

        let idx = node_id.index();
        let nd = match node_data_vec.as_ref().get(idx) {
            Some(nd) => nd,
            None => return json!({}),
        };

        let tag = match &nd.node_type {
            azul_core::dom::NodeType::Div => "div".to_string(),
            azul_core::dom::NodeType::Body => "body".to_string(),
            azul_core::dom::NodeType::Br => "br".to_string(),
            azul_core::dom::NodeType::Text(s) => {
                return json!({
                    "tag": "__text__",
                    "text": s.as_str(),
                    "children": [],
                    "classes": []
                });
            }
            azul_core::dom::NodeType::Image(_) => "img".to_string(),
            azul_core::dom::NodeType::VirtualView => "virtualized-view".to_string(),
            azul_core::dom::NodeType::P => "p".to_string(),
            azul_core::dom::NodeType::Label => "label".to_string(),
            azul_core::dom::NodeType::Span => "span".to_string(),
            azul_core::dom::NodeType::Button => "button".to_string(),
            _ => {
                // Use Debug format to get tag name for other types
                let tag_str = format!("{:?}", nd.node_type);
                tag_str.to_lowercase()
            }
        };

        let classes: Vec<String> = nd
            .attributes()
            .as_ref()
            .iter()
            .filter_map(|attr| attr.as_class().map(|s| s.to_string()))
            .collect();

        // Collect children
        let mut children = Vec::new();
        let hierarchy = node_hierarchy.as_ref();
        if let Some(h) = hierarchy.get(idx) {
            if let Some(first_child) = h.first_child_id(node_id) {
                let mut child_id = first_child;
                loop {
                    children.push(build_tree(child_id, node_data_vec, node_hierarchy));
                    if let Some(h_child) = hierarchy.get(child_id.index()) {
                        if let Some(next) = h_child.next_sibling_id() {
                            child_id = next;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
        }

        json!({
            "tag": tag,
            "children": children,
            "classes": classes
        })
    }

    // Build from root (node 0)
    let root = build_tree(NodeId::ZERO, node_data_vec, node_hierarchy);
    json!({ "nodes": [root] })
}

/// Clone the component's `data_model` and override `default_value` fields from the
/// provided JSON args map. Returns a `ComponentDataModel` ready to pass to `render_fn`.
///
/// - Known fields have their defaults overridden by the parsed JSON value.
/// - Missing fields keep their existing default.
/// - Missing required fields (no default and not in JSON) are an error.
/// - Unknown keys are silently ignored.
#[cfg(feature = "std")]
fn override_data_model_defaults(
    data_model: &azul_core::xml::ComponentDataModel,
    json_args: Option<&std::collections::HashMap<String, serde_json::Value>>,
) -> Result<azul_core::xml::ComponentDataModel, String> {
    use azul_core::xml::{
        ComponentDataField, ComponentDataFieldVec, ComponentDataModel, ComponentDefaultValue,
        ComponentFieldType, OptionComponentDefaultValue,
    };

    let empty_map = std::collections::HashMap::new();
    let map = json_args.unwrap_or(&empty_map);

    let mut fields: Vec<ComponentDataField> = data_model.fields.as_ref().to_vec();

    for field in fields.iter_mut() {
        let name = field.name.as_str();
        if let Some(json_val) = map.get(name) {
            let parsed = parse_json_to_default_value(&field.field_type, json_val)
                .map_err(|e| format!("field '{}': {}", name, e))?;
            field.default_value = OptionComponentDefaultValue::Some(parsed);
        } else if field.required {
            if let OptionComponentDefaultValue::None = &field.default_value {
                return Err(format!("required field '{}' is missing", name));
            }
        }
    }

    Ok(ComponentDataModel {
        name: data_model.name.clone(),
        description: data_model.description.clone(),
        fields: ComponentDataFieldVec::from_vec(fields),
    })
}

/// Convert a single `serde_json::Value` to a `ComponentDefaultValue` given the expected type.
#[cfg(feature = "std")]
fn parse_json_to_default_value(
    ft: &azul_core::xml::ComponentFieldType,
    val: &serde_json::Value,
) -> Result<azul_core::xml::ComponentDefaultValue, String> {
    use azul_core::xml::{ComponentDefaultValue, ComponentFieldType};
    use azul_css::corety::AzString;

    match ft {
        ComponentFieldType::String => match val {
            serde_json::Value::String(s) => {
                Ok(ComponentDefaultValue::String(AzString::from(s.as_str())))
            }
            other => Ok(ComponentDefaultValue::String(AzString::from(
                other.to_string().as_str(),
            ))),
        },
        ComponentFieldType::Bool => match val {
            serde_json::Value::Bool(b) => Ok(ComponentDefaultValue::Bool(*b)),
            serde_json::Value::String(s) => s
                .parse::<bool>()
                .map(ComponentDefaultValue::Bool)
                .map_err(|_| format!("expected bool, got \"{}\"", s)),
            other => Err(format!("expected bool, got {}", other)),
        },
        ComponentFieldType::I32 => match val {
            serde_json::Value::Number(n) => n
                .as_i64()
                .and_then(|v| i32::try_from(v).ok())
                .map(ComponentDefaultValue::I32)
                .ok_or_else(|| format!("expected i32, got {}", n)),
            serde_json::Value::String(s) => s
                .parse::<i32>()
                .map(ComponentDefaultValue::I32)
                .map_err(|_| format!("expected i32, got \"{}\"", s)),
            other => Err(format!("expected i32, got {}", other)),
        },
        ComponentFieldType::I64 => match val {
            serde_json::Value::Number(n) => n
                .as_i64()
                .map(ComponentDefaultValue::I64)
                .ok_or_else(|| format!("expected i64, got {}", n)),
            serde_json::Value::String(s) => s
                .parse::<i64>()
                .map(ComponentDefaultValue::I64)
                .map_err(|_| format!("expected i64, got \"{}\"", s)),
            other => Err(format!("expected i64, got {}", other)),
        },
        ComponentFieldType::U32 => match val {
            serde_json::Value::Number(n) => n
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .map(ComponentDefaultValue::U32)
                .ok_or_else(|| format!("expected u32, got {}", n)),
            serde_json::Value::String(s) => s
                .parse::<u32>()
                .map(ComponentDefaultValue::U32)
                .map_err(|_| format!("expected u32, got \"{}\"", s)),
            other => Err(format!("expected u32, got {}", other)),
        },
        ComponentFieldType::U64 => match val {
            serde_json::Value::Number(n) => n
                .as_u64()
                .map(ComponentDefaultValue::U64)
                .ok_or_else(|| format!("expected u64, got {}", n)),
            serde_json::Value::String(s) => s
                .parse::<u64>()
                .map(ComponentDefaultValue::U64)
                .map_err(|_| format!("expected u64, got \"{}\"", s)),
            other => Err(format!("expected u64, got {}", other)),
        },
        ComponentFieldType::Usize => match val {
            serde_json::Value::Number(n) => n
                .as_u64()
                .map(|v| v as usize)
                .map(ComponentDefaultValue::Usize)
                .ok_or_else(|| format!("expected usize, got {}", n)),
            serde_json::Value::String(s) => s
                .parse::<usize>()
                .map(ComponentDefaultValue::Usize)
                .map_err(|_| format!("expected usize, got \"{}\"", s)),
            other => Err(format!("expected usize, got {}", other)),
        },
        ComponentFieldType::F32 => match val {
            serde_json::Value::Number(n) => n
                .as_f64()
                .map(|v| v as f32)
                .map(ComponentDefaultValue::F32)
                .ok_or_else(|| format!("expected f32, got {}", n)),
            serde_json::Value::String(s) => s
                .parse::<f32>()
                .map(ComponentDefaultValue::F32)
                .map_err(|_| format!("expected f32, got \"{}\"", s)),
            other => Err(format!("expected f32, got {}", other)),
        },
        ComponentFieldType::F64 => match val {
            serde_json::Value::Number(n) => n
                .as_f64()
                .map(ComponentDefaultValue::F64)
                .ok_or_else(|| format!("expected f64, got {}", n)),
            serde_json::Value::String(s) => s
                .parse::<f64>()
                .map(ComponentDefaultValue::F64)
                .map_err(|_| format!("expected f64, got \"{}\"", s)),
            other => Err(format!("expected f64, got {}", other)),
        },
        ComponentFieldType::ColorU => match val {
            serde_json::Value::String(s) => {
                let hex = s.strip_prefix('#').unwrap_or(s.as_str());
                if hex.len() >= 6 {
                    let r = u8::from_str_radix(&hex[0..2], 16)
                        .map_err(|_| format!("invalid color: {}", s))?;
                    let g = u8::from_str_radix(&hex[2..4], 16)
                        .map_err(|_| format!("invalid color: {}", s))?;
                    let b = u8::from_str_radix(&hex[4..6], 16)
                        .map_err(|_| format!("invalid color: {}", s))?;
                    let a = if hex.len() >= 8 {
                        u8::from_str_radix(&hex[6..8], 16).unwrap_or(255)
                    } else {
                        255
                    };
                    Ok(ComponentDefaultValue::ColorU(
                        azul_css::props::basic::color::ColorU { r, g, b, a },
                    ))
                } else {
                    Err(format!("expected #RRGGBB[AA], got \"{}\"", s))
                }
            }
            other => Err(format!("expected color string (#RRGGBB), got {}", other)),
        },
        ComponentFieldType::OptionType(inner) => match val {
            serde_json::Value::Null => Ok(ComponentDefaultValue::None),
            _ => parse_json_to_default_value(inner.as_ref(), val),
        },
        // For complex types we fall back to storing as string
        _ => match val {
            serde_json::Value::String(s) => {
                Ok(ComponentDefaultValue::String(AzString::from(s.as_str())))
            }
            other => Ok(ComponentDefaultValue::String(AzString::from(
                other.to_string().as_str(),
            ))),
        },
    }
}

/// Generate a compilable app from the LIVE page (the current window's DOM+CSS).
///
/// Serializes the live `StyledDom` back to HTML (`get_html_string`), reparses it,
/// and runs the per-language HTML→app code generator. Returns `(filename, source)`
/// for the app's main entry file. This is the "Export → Code" of the live UI.
#[cfg(feature = "std")]
fn build_live_page_code(
    language: &str,
    callback_info: &azul_layout::callbacks::CallbackInfo,
) -> Result<(String, String), String> {
    use azul_core::xml::{str_to_c_code, str_to_cpp_code, str_to_python_code, str_to_rust_code};

    let layout_window = callback_info.get_layout_window();
    let styled_dom = layout_window
        .layout_results
        .get(&ROOT_DOM_ID)
        .map(|lr| &lr.styled_dom)
        .ok_or_else(|| "no layout result for DOM 0".to_string())?;
    // test_mode=false wraps the DOM tree in a full <html><head>..</head>..</html>
    // document; the per-language code generators below require an <html> root
    // (get_html_node) — the bare tree (test_mode=true) fails with NoHtmlNode.
    let html = styled_dom.get_html_string("", "", false);
    let nodes = azul_layout::xml::parse_xml_string(&html)
        .map_err(|e| format!("parse live HTML: {:?}", e))?;
    let cmap = azul_core::xml::ComponentMap::with_builtin();

    let (fname, src) = match language {
        "rust" => ("src/main.rs", str_to_rust_code(nodes.as_ref(), "", &cmap)),
        "c" => ("main.c", str_to_c_code(nodes.as_ref(), &cmap)),
        "cpp" | "c++" => ("main.cpp", str_to_cpp_code(nodes.as_ref(), &cmap)),
        "python" | "py" => ("main.py", str_to_python_code(nodes.as_ref(), &cmap)),
        other => return Err(format!("unsupported language: {}", other)),
    };
    let src = src.map_err(|e| format!("codegen: {}", e))?;
    Ok((fname.to_string(), src))
}

/// Build exported code for all exportable component libraries.
///
/// Uses `compile_fn` on each exportable component to generate source code
/// in the target language, then packages the result as a set of files.
/// For the "builtin" library this is a no-op (builtin components are not exported).
#[cfg(feature = "std")]
fn build_exported_code(
    language: &str,
    map_ref: &azul_core::xml::ComponentMap,
) -> Result<ExportedCodeResponse, String> {
    use azul_core::xml::{CompileTarget, ComponentDef, ComponentMap, ResultStringCompileError};

    let target = match language {
        "rust" => CompileTarget::Rust,
        "c" => CompileTarget::C,
        "cpp" | "c++" => CompileTarget::Cpp,
        "python" => CompileTarget::Python,
        other => {
            return Err(format!(
                "Unsupported language: '{}'. Use: rust, c, cpp, python",
                other
            ))
        }
    };

    let mut files = std::collections::HashMap::new();
    let mut warnings = Vec::new();

    // Collect all exportable component definitions with their data models
    let exportable = map_ref.get_exportable_libraries();

    // Gather component info for scaffold generation
    let mut component_infos: Vec<ScaffoldComponentInfo> = Vec::new();

    for lib in &exportable {
        for def in lib.components.iter() {
            let compiled_code = match (def.compile_fn)(def, &target, &def.data_model, 0) {
                ResultStringCompileError::Ok(code) => Some(code.as_str().to_string()),
                ResultStringCompileError::Err(e) => {
                    warnings.push(format!(
                        "Failed to compile component '{}': {:?}",
                        def.id.qualified_name(),
                        e
                    ));
                    None
                }
            };

            component_infos.push(ScaffoldComponentInfo {
                name: def.id.name.as_str().to_string(),
                display_name: def.display_name.as_str().to_string(),
                data_model_name: def.data_model.name.as_str().to_string(),
                compiled_code,
                data_fields: def
                    .data_model
                    .fields
                    .as_ref()
                    .iter()
                    .filter(|f| {
                        !matches!(
                            f.field_type,
                            azul_core::xml::ComponentFieldType::Callback(..)
                                | azul_core::xml::ComponentFieldType::StyledDom
                        )
                    })
                    .map(|f| {
                        (
                            f.name.as_str().to_string(),
                            field_type_to_string(&f.field_type),
                            default_value_to_opt_string(&f.default_value),
                        )
                    })
                    .collect(),
                callback_slots: def
                    .data_model
                    .fields
                    .as_ref()
                    .iter()
                    .filter_map(|f| {
                        if let azul_core::xml::ComponentFieldType::Callback(ref signature) =
                            f.field_type
                        {
                            Some((
                                f.name.as_str().to_string(),
                                format!("Callback({})", signature.return_type.as_str()),
                            ))
                        } else {
                            None
                        }
                    })
                    .collect(),
                slot_fields: def
                    .data_model
                    .fields
                    .as_ref()
                    .iter()
                    .filter(|f| {
                        matches!(f.field_type, azul_core::xml::ComponentFieldType::StyledDom)
                    })
                    .map(|f| {
                        (
                            f.name.as_str().to_string(),
                            f.description.as_str().to_string(),
                        )
                    })
                    .collect(),
            });
        }
    }

    let scaffold_files = generate_scaffold(&target, &component_infos);
    for (filename, content) in scaffold_files {
        files.insert(filename, content);
    }

    if component_infos.is_empty() {
        warnings.push(
            "No user-defined component libraries to export. Generated minimal scaffold."
                .to_string(),
        );
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
    /// Name of the data model struct (e.g. "CardData")
    data_model_name: String,
    compiled_code: Option<String>,
    /// Data fields: (name, type_string, default_value)
    data_fields: Vec<(String, String, Option<String>)>,
    /// Callback slots: (name, callback_type_string)
    callback_slots: Vec<(String, String)>,
    /// Slot fields (StyledDom children): (name, description)
    slot_fields: Vec<(String, String)>,
}

/// Convert a `ComponentFieldType` to a JSON-friendly string for the debug protocol (legacy flat format).
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
        ComponentFieldType::Callback(signature) => {
            format!("Callback({})", signature.return_type.as_str())
        }
        ComponentFieldType::RefAny(type_hint) => format!("RefAny({})", type_hint.as_str()),
        ComponentFieldType::OptionType(inner) => {
            format!("Option<{}>", field_type_to_string(inner.as_ref()))
        }
        ComponentFieldType::VecType(inner) => {
            format!("Vec<{}>", field_type_to_string(inner.as_ref()))
        }
        ComponentFieldType::StructRef(name) => format!("struct:{}", name.as_str()),
        ComponentFieldType::EnumRef(name) => format!("enum:{}", name.as_str()),
    }
}

/// Convert a `ComponentFieldType` to a structured JSON descriptor.
#[cfg(feature = "std")]
fn field_type_to_structured(ft: &azul_core::xml::ComponentFieldType) -> StructuredFieldType {
    use azul_core::xml::ComponentFieldType;
    match ft {
        ComponentFieldType::String => StructuredFieldType::Primitive {
            name: "String".to_string(),
        },
        ComponentFieldType::Bool => StructuredFieldType::Primitive {
            name: "bool".to_string(),
        },
        ComponentFieldType::I32 => StructuredFieldType::Primitive {
            name: "i32".to_string(),
        },
        ComponentFieldType::I64 => StructuredFieldType::Primitive {
            name: "i64".to_string(),
        },
        ComponentFieldType::U32 => StructuredFieldType::Primitive {
            name: "u32".to_string(),
        },
        ComponentFieldType::U64 => StructuredFieldType::Primitive {
            name: "u64".to_string(),
        },
        ComponentFieldType::Usize => StructuredFieldType::Primitive {
            name: "usize".to_string(),
        },
        ComponentFieldType::F32 => StructuredFieldType::Primitive {
            name: "f32".to_string(),
        },
        ComponentFieldType::F64 => StructuredFieldType::Primitive {
            name: "f64".to_string(),
        },
        ComponentFieldType::ColorU => StructuredFieldType::Primitive {
            name: "ColorU".to_string(),
        },
        ComponentFieldType::CssProperty => StructuredFieldType::Primitive {
            name: "CssProperty".to_string(),
        },
        ComponentFieldType::ImageRef => StructuredFieldType::Primitive {
            name: "ImageRef".to_string(),
        },
        ComponentFieldType::FontRef => StructuredFieldType::Primitive {
            name: "FontRef".to_string(),
        },
        ComponentFieldType::StyledDom => StructuredFieldType::Primitive {
            name: "StyledDom".to_string(),
        },
        ComponentFieldType::Callback(signature) => {
            let args: Vec<CallbackArgInfo> = signature
                .args
                .as_ref()
                .iter()
                .map(|a| CallbackArgInfo {
                    name: a.name.as_str().to_string(),
                    arg_type: field_type_to_string(&a.arg_type),
                })
                .collect();
            StructuredFieldType::Callback {
                args,
                return_type: signature.return_type.as_str().to_string(),
            }
        }
        ComponentFieldType::RefAny(type_hint) => StructuredFieldType::RefAny {
            type_hint: type_hint.as_str().to_string(),
        },
        ComponentFieldType::OptionType(inner) => StructuredFieldType::OptionType {
            inner: Box::new(field_type_to_structured(inner.as_ref())),
        },
        ComponentFieldType::VecType(inner) => StructuredFieldType::VecType {
            inner: Box::new(field_type_to_structured(inner.as_ref())),
        },
        ComponentFieldType::StructRef(name) => StructuredFieldType::StructRef {
            name: name.as_str().to_string(),
        },
        ComponentFieldType::EnumRef(name) => StructuredFieldType::EnumRef {
            name: name.as_str().to_string(),
        },
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
            ComponentDefaultValue::ColorU(c) => {
                format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a)
            }
            ComponentDefaultValue::ComponentInstance(ci) => {
                format!("instance:{}", ci.component.as_str())
            }
            ComponentDefaultValue::CallbackFnPointer(s) => format!("fn:{}", s.as_str()),
            ComponentDefaultValue::Json(s) => s.as_str().to_string(),
        }),
    }
}

/// Parse a JSON-friendly type string into a `ComponentFieldType`.
/// Returns an error for unrecognized type strings instead of silently falling back.
#[cfg(feature = "std")]
fn parse_field_type_from_string(s: &str) -> Result<azul_core::xml::ComponentFieldType, String> {
    use azul_core::xml::{
        ComponentCallbackArgVec, ComponentCallbackSignature, ComponentFieldType,
        ComponentFieldTypeBox,
    };
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
                Ok(ComponentFieldType::Callback(ComponentCallbackSignature {
                    return_type: AzString::from("Update"),
                    args: ComponentCallbackArgVec::from_const_slice(&[]),
                }))
            } else if other.starts_with("RefAny") {
                let hint = other
                    .strip_prefix("RefAny(")
                    .and_then(|s| s.strip_suffix(")"))
                    .ok_or_else(|| {
                        format!(
                            "Invalid RefAny syntax '{}', expected 'RefAny(TypeHint)'",
                            other
                        )
                    })?;
                Ok(ComponentFieldType::RefAny(AzString::from(hint)))
            } else if other.starts_with("Option<") {
                let inner = other
                    .strip_prefix("Option<")
                    .and_then(|s| s.strip_suffix(">"))
                    .ok_or_else(|| {
                        format!(
                            "Invalid Option type syntax '{}', expected 'Option<InnerType>'",
                            other
                        )
                    })?;
                Ok(ComponentFieldType::OptionType(ComponentFieldTypeBox::new(
                    parse_field_type_from_string(inner)?,
                )))
            } else if other.starts_with("Vec<") {
                let inner = other
                    .strip_prefix("Vec<")
                    .and_then(|s| s.strip_suffix(">"))
                    .ok_or_else(|| {
                        format!(
                            "Invalid Vec type syntax '{}', expected 'Vec<InnerType>'",
                            other
                        )
                    })?;
                Ok(ComponentFieldType::VecType(ComponentFieldTypeBox::new(
                    parse_field_type_from_string(inner)?,
                )))
            } else if let Some(name) = other.strip_prefix("struct:") {
                if name.is_empty() {
                    return Err("Empty struct reference name in 'struct:'".to_string());
                }
                Ok(ComponentFieldType::StructRef(AzString::from(name)))
            } else if let Some(name) = other.strip_prefix("enum:") {
                if name.is_empty() {
                    return Err("Empty enum reference name in 'enum:'".to_string());
                }
                Ok(ComponentFieldType::EnumRef(AzString::from(name)))
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
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
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
fn validate_exported_field(
    field: &ExportedDataField,
) -> Result<azul_core::xml::ComponentDataField, String> {
    use azul_core::xml::{ComponentDataField, ComponentDefaultValue, OptionComponentDefaultValue};
    use azul_css::corety::AzString;

    validate_field_name(&field.name)?;

    let field_type = parse_field_type_from_string(&field.field_type)
        .map_err(|e| format!("Field '{}': {}", field.name, e))?;

    let default_value = match &field.default {
        Some(d) => parse_default_value(d, &field_type)
            .map(OptionComponentDefaultValue::Some)
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
        ComponentFieldType::I32 => value
            .parse::<i32>()
            .map(ComponentDefaultValue::I32)
            .map_err(|e| format!("expected i32: {}", e)),
        ComponentFieldType::I64 => value
            .parse::<i64>()
            .map(ComponentDefaultValue::I64)
            .map_err(|e| format!("expected i64: {}", e)),
        ComponentFieldType::U32 => value
            .parse::<u32>()
            .map(ComponentDefaultValue::U32)
            .map_err(|e| format!("expected u32: {}", e)),
        ComponentFieldType::U64 => value
            .parse::<u64>()
            .map(ComponentDefaultValue::U64)
            .map_err(|e| format!("expected u64: {}", e)),
        ComponentFieldType::Usize => value
            .parse::<usize>()
            .map(ComponentDefaultValue::Usize)
            .map_err(|e| format!("expected usize: {}", e)),
        ComponentFieldType::F32 => value
            .parse::<f32>()
            .map(ComponentDefaultValue::F32)
            .map_err(|e| format!("expected f32: {}", e)),
        ComponentFieldType::F64 => value
            .parse::<f64>()
            .map(ComponentDefaultValue::F64)
            .map_err(|e| format!("expected f64: {}", e)),
        ComponentFieldType::ColorU => {
            // Accept #rrggbb or #rrggbbaa hex strings
            let hex = value.strip_prefix('#').unwrap_or(value);
            if hex.len() == 6 || hex.len() == 8 {
                let r = u8::from_str_radix(&hex[0..2], 16)
                    .map_err(|e| format!("invalid color: {}", e))?;
                let g = u8::from_str_radix(&hex[2..4], 16)
                    .map_err(|e| format!("invalid color: {}", e))?;
                let b = u8::from_str_radix(&hex[4..6], 16)
                    .map_err(|e| format!("invalid color: {}", e))?;
                let a = if hex.len() == 8 {
                    u8::from_str_radix(&hex[6..8], 16)
                        .map_err(|e| format!("invalid color: {}", e))?
                } else {
                    255
                };
                Ok(ComponentDefaultValue::ColorU(
                    azul_css::props::basic::color::ColorU { r, g, b, a },
                ))
            } else {
                Err(format!(
                    "expected #rrggbb or #rrggbbaa hex color, got '{}'",
                    value
                ))
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
fn validate_exported_fields(
    fields: &[ExportedDataField],
) -> Result<Vec<azul_core::xml::ComponentDataField>, String> {
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
    s.split(['_', '-'])
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
        "ColorU" | "color" => "AzColorU",
        "StyledDom" | "dom" => "AzStyledDom",
        _ => "AzString",
    }
}

/// Map component type strings to C++ types
#[cfg(feature = "std")]
fn map_type_to_cpp(type_str: &str) -> &str {
    match type_str {
        "String" | "string" => "std::string",
        "bool" | "Bool" | "boolean" => "bool",
        "i32" | "int" | "Int" => "int32_t",
        "i64" => "int64_t",
        "f32" | "float" | "Float" => "float",
        "f64" | "double" | "Double" => "double",
        "u32" | "uint" => "uint32_t",
        "u64" => "uint64_t",
        "usize" => "size_t",
        "ColorU" | "color" => "ColorU",
        "StyledDom" | "dom" => "StyledDom",
        _ => "std::string",
    }
}

/// Generate C++ default initializer expression
#[cfg(feature = "std")]
fn cpp_default_init(type_str: &str) -> String {
    match type_str {
        "String" | "string" => String::new(),
        "bool" | "Bool" | "boolean" => " = false".to_string(),
        "f32" | "float" | "Float" | "f64" | "double" | "Double" => " = 0.0".to_string(),
        _ => " = 0".to_string(),
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

    // --- Per-component data structs ---
    let mut component_structs = String::new();
    let mut component_render_fns = String::new();
    let mut callback_stubs = String::new();

    for comp in components {
        let struct_name = &comp.data_model_name;
        let pascal_name = to_pascal_case(&comp.name);

        // Generate the struct
        component_structs.push_str(&format!(
            "/// Data model for the {} component\n",
            comp.display_name
        ));
        component_structs.push_str(&format!("pub struct {} {{\n", struct_name));
        for (field_name, field_type, _default) in &comp.data_fields {
            let rust_type = map_type_to_rust(field_type);
            component_structs.push_str(&format!("    pub {}: {},\n", field_name, rust_type));
        }
        for (slot_name, _desc) in &comp.slot_fields {
            component_structs.push_str(&format!("    pub {}: StyledDom,\n", slot_name));
        }
        for (cb_name, _cb_type) in &comp.callback_slots {
            component_structs.push_str(&format!("    pub {}: Option<Callback>,\n", cb_name));
        }
        component_structs.push_str("}\n\n");

        // Generate Default impl
        component_structs.push_str(&format!("impl Default for {} {{\n", struct_name));
        component_structs.push_str("    fn default() -> Self {\n");
        component_structs.push_str("        Self {\n");
        for (field_name, field_type, default_val) in &comp.data_fields {
            component_structs.push_str(&format!(
                "            {}: {},\n",
                field_name,
                rust_default_for_type(field_type, default_val.as_deref())
            ));
        }
        for (slot_name, _desc) in &comp.slot_fields {
            component_structs.push_str(&format!(
                "            {}: StyledDom::default(),\n",
                slot_name
            ));
        }
        for (cb_name, _cb_type) in &comp.callback_slots {
            component_structs.push_str(&format!("            {}: None,\n", cb_name));
        }
        component_structs.push_str("        }\n    }\n}\n\n");

        // Generate render function
        component_render_fns.push_str(&format!("/// Render the {} component\n", comp.display_name));
        component_render_fns.push_str(&format!(
            "fn render_{}(data: &{}) -> Dom {{\n",
            comp.name, struct_name
        ));
        if let Some(ref code) = comp.compiled_code {
            component_render_fns.push_str(&format!("    {}\n", code));
        } else {
            component_render_fns.push_str(&format!(
                "    Dom::create_div() // TODO: implement {} rendering\n",
                comp.display_name
            ));
        }
        component_render_fns.push_str("}\n\n");

        // Generate callback stubs
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
        layout_body.push_str("    Dom::create_body()\n");
        layout_body.push_str("        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(\"Hello from Azul!\"))\n");
        layout_body.push_str("        .with_css(\"\")\n");
    } else {
        layout_body.push_str("    Dom::create_body()\n");
        for comp in components {
            layout_body.push_str(&format!(
                "        .with_child(render_{}(&{}::default()))\n",
                comp.name, comp.data_model_name
            ));
        }
        layout_body.push_str("        .with_css(\"\")\n");
    }

    let main_rs = format!(
        r#"//! Auto-generated by Azul debugger
//! Customize this file to build your application.

extern crate azul;
use azul::prelude::*;

// =============================================================================
// Component Data Models
// =============================================================================

{component_structs}
// =============================================================================
// Component Render Functions
// =============================================================================

{render_fns}
// =============================================================================
// Callbacks
// =============================================================================
{callbacks}
/// Layout callback — returns the DOM tree for a window
extern "C" fn layout(data: &mut RefAny, _info: &mut LayoutCallbackInfo) -> Dom {{
{layout_body}}}

fn main() {{
    let app = App::create(RefAny::new(()), AppConfig::create());
    let window = WindowCreateOptions::create(layout);
    app.run(window);
}}
"#,
        component_structs = component_structs,
        render_fns = component_render_fns,
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

    // --- Per-component typedefs ---
    let mut component_typedefs = String::new();
    let mut render_fns = String::new();
    let mut callback_stubs = String::new();

    for comp in components {
        let struct_name = to_pascal_case(&comp.name);

        component_typedefs.push_str(&format!("/* Data model for {} */\n", comp.display_name));
        component_typedefs.push_str("typedef struct {\n");
        for (field_name, field_type, _default) in &comp.data_fields {
            let c_type = map_type_to_c(field_type);
            component_typedefs.push_str(&format!("    {} {};\n", c_type, field_name));
        }
        for (slot_name, _desc) in &comp.slot_fields {
            component_typedefs.push_str(&format!("    AzStyledDom {};\n", slot_name));
        }
        if comp.data_fields.is_empty() && comp.slot_fields.is_empty() {
            component_typedefs.push_str("    int _placeholder;\n");
        }
        component_typedefs.push_str(&format!("}} {}Data;\n\n", struct_name));

        // Render function
        render_fns.push_str(&format!("/* Render {} */\n", comp.display_name));
        render_fns.push_str(&format!(
            "AzDom render_{}(const {}Data* data) {{\n",
            comp.name, struct_name
        ));
        if let Some(ref code) = comp.compiled_code {
            render_fns.push_str(&format!("    return {};\n", code));
        } else {
            render_fns.push_str(&format!(
                "    return AzDom_createDiv(); /* TODO: implement {} */\n",
                comp.display_name
            ));
        }
        render_fns.push_str("}\n\n");

        for (slot_name, _cb_type) in &comp.callback_slots {
            callback_stubs.push_str(&format!(
                "AzUpdate {slot_name}(AzRefAny* data, AzCallbackInfo* info) {{\n    /* TODO: implement {slot_name} */\n    return AzUpdate_DoNothing;\n}}\n\n",
                slot_name = slot_name
            ));
        }
    }

    // Layout function
    let mut layout_body = String::new();
    layout_body.push_str("    AzDom body = AzDom_createBody();\n");
    if components.is_empty() {
        layout_body.push_str("    AzDom_addChild(&body, AzDom_createTextDoNotUseWithoutBlockLevelWrapper(AZ_STR(\"Hello from Azul!\")));\n");
    } else {
        for comp in components {
            let struct_name = to_pascal_case(&comp.name);
            layout_body.push_str(&format!(
                "    {}Data {}_data = {{ 0 }};\n",
                struct_name, comp.name
            ));
            layout_body.push_str(&format!(
                "    AzDom_addChild(&body, render_{}(&{}_data));\n",
                comp.name, comp.name
            ));
        }
    }
    layout_body.push_str("    return body;\n");

    let main_c = format!(
        r#"/* Auto-generated by Azul debugger */
#include "azul.h"
#include <string.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

{typedefs}
{render_fns}
{callbacks}
AzDom layout(AzRefAny* data, AzLayoutCallbackInfo* info) {{
{layout_body}}}

int main() {{
    AzString data_type = AZ_STR("Data");
    AzRefAny data = AzRefAny_newC((AzGlVoidPtrConst){{ .ptr = NULL }}, 0, 1, 0, data_type, NULL, 0, 0);
    AzApp app = AzApp_create(data, AzAppConfig_create());
    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}}
"#,
        typedefs = component_typedefs,
        render_fns = render_fns,
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

    let mut component_structs = String::new();
    let mut render_fns = String::new();

    for comp in components {
        let struct_name = to_pascal_case(&comp.name);

        component_structs.push_str(&format!("// Data model for {}\n", comp.display_name));
        component_structs.push_str(&format!("struct {}Data {{\n", struct_name));
        for (field_name, field_type, default_val) in &comp.data_fields {
            let cpp_type = map_type_to_cpp(field_type);
            let default_str = match default_val.as_deref() {
                Some(v) => format!(" = {}", v),
                None => cpp_default_init(field_type),
            };
            component_structs.push_str(&format!(
                "    {} {}{};\n",
                cpp_type, field_name, default_str
            ));
        }
        for (slot_name, _desc) in &comp.slot_fields {
            component_structs.push_str(&format!("    StyledDom {};\n", slot_name));
        }
        if comp.data_fields.is_empty() && comp.slot_fields.is_empty() {
            component_structs.push_str("    int _placeholder = 0;\n");
        }
        component_structs.push_str("};\n\n");

        render_fns.push_str(&format!("// Render {}\n", comp.display_name));
        render_fns.push_str(&format!(
            "Dom render_{}(const {}Data& data) {{\n",
            comp.name, struct_name
        ));
        if let Some(ref code) = comp.compiled_code {
            render_fns.push_str(&format!("    return {};\n", code));
        } else {
            render_fns.push_str(&format!(
                "    return Dom::create_div(); // TODO: {}\n",
                comp.display_name
            ));
        }
        render_fns.push_str("}\n\n");
    }

    let mut layout_body = String::new();
    layout_body.push_str("    auto body = Dom::create_body();\n");
    if components.is_empty() {
        layout_body.push_str("    body.add_child(Dom::create_text_do_not_use_without_block_level_wrapper(String(\"Hello from Azul!\")));\n");
    } else {
        for comp in components {
            let struct_name = to_pascal_case(&comp.name);
            layout_body.push_str(&format!(
                "    body.add_child(render_{}({}Data{{}}));\n",
                comp.name, struct_name
            ));
        }
    }
    layout_body.push_str("    return body.with_css(\"\");\n");

    let main_cpp = format!(
        r#"// Auto-generated by Azul debugger
#include "azul20.hpp"
using namespace azul;

{structs}
{render_fns}
Dom layout(RefAny& data, LayoutCallbackInfo& info) {{
{layout_body}}}

int main() {{
    RefAny data = RefAny::create(0);
    WindowCreateOptions window = WindowCreateOptions::create(layout);
    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));
    return 0;
}}
"#,
        structs = component_structs,
        render_fns = render_fns,
        layout_body = layout_body,
    );
    files.push(("main.cpp".to_string(), main_cpp));
    files
}

/// Generate a complete Python project scaffold
#[cfg(feature = "std")]
fn generate_python_scaffold(components: &[ScaffoldComponentInfo]) -> Vec<(String, String)> {
    let mut files = Vec::new();

    // --- Per-component data classes ---
    let mut component_classes = String::new();
    let mut render_fns = String::new();

    for comp in components {
        let class_name = format!("{}Data", to_pascal_case(&comp.name));

        // Data class with typed fields
        component_classes.push_str(&format!("class {}:\n", class_name));
        component_classes.push_str("    def __init__(self):\n");

        let mut has_fields = false;

        for (field_name, field_type, default_val) in &comp.data_fields {
            has_fields = true;
            let default_str = python_default_value(field_type, default_val.as_deref());
            component_classes.push_str(&format!("        self.{} = {}\n", field_name, default_str));
        }

        for (slot_name, _slot_type) in &comp.slot_fields {
            has_fields = true;
            component_classes.push_str(&format!(
                "        self.{} = None  # StyledDom slot\n",
                slot_name
            ));
        }

        for (cb_name, _cb_type) in &comp.callback_slots {
            has_fields = true;
            component_classes.push_str(&format!("        self.{} = None  # callback\n", cb_name));
        }

        if !has_fields {
            component_classes.push_str("        pass\n");
        }

        component_classes.push_str("\n\n");

        // Render function
        render_fns.push_str(&format!("def render_{}(data):\n", comp.name));
        render_fns.push_str(&format!(
            "    \"\"\"Render the {} component.\"\"\"\n",
            comp.display_name
        ));

        if let Some(ref code) = comp.compiled_code {
            for line in code.lines() {
                render_fns.push_str(&format!("    {}\n", line));
            }
        } else {
            render_fns.push_str("    dom = Dom.create_div()\n");
            render_fns.push_str("    # TODO: build DOM from component data\n");
            render_fns.push_str("    return dom\n");
        }

        render_fns.push_str("\n\n");
    }

    // --- Layout function ---
    let mut layout_body = String::new();
    layout_body.push_str("    body = Dom.create_body()\n");
    if components.is_empty() {
        layout_body.push_str("    body = body.with_child(Dom.create_text_do_not_use_without_block_level_wrapper(\"Hello from Azul!\"))\n");
    } else {
        for comp in components {
            let class_name = format!("{}Data", to_pascal_case(&comp.name));
            layout_body.push_str(&format!(
                "    body = body.with_child(render_{}({}()))\n",
                comp.name, class_name
            ));
        }
    }
    layout_body.push_str("    return body.with_css(\"\")\n");

    let main_py = format!(
        r#"# Auto-generated by Azul debugger
from azul import *

{classes}{render_fns}def layout(data, info):
{layout_body}

app = App.create(None, AppConfig.create())
app.run(WindowCreateOptions.create(layout))
"#,
        classes = component_classes,
        render_fns = render_fns,
        layout_body = layout_body,
    );
    files.push(("main.py".to_string(), main_py));
    files
}

/// Return a Python default value expression for a given field type
#[cfg(feature = "std")]
fn python_default_value(field_type: &str, default_val: Option<&str>) -> String {
    match default_val {
        Some(v) => match field_type {
            "String" | "string" => format!("\"{}\"", v),
            "bool" | "Bool" | "boolean" => {
                if v == "true" {
                    "True".to_string()
                } else {
                    "False".to_string()
                }
            }
            _ => v.to_string(),
        },
        None => match field_type {
            "String" | "string" => "\"\"".to_string(),
            "bool" | "Bool" | "boolean" => "False".to_string(),
            "f32" | "f64" | "float" | "double" | "Float" | "Double" => "0.0".to_string(),
            "ColorU" => "ColorU(0, 0, 0, 255)".to_string(),
            "StyledDom" => "Dom.create_div()".to_string(),
            _ => "0".to_string(),
        },
    }
}

/// Convert an `azul_core::json::Json` value to `serde_json::Value`.
///
/// For primitive types (null, bool, number, string) we convert directly.
/// For arrays/objects the internal representation is already a JSON string,
/// so we parse it with serde_json.
#[cfg(feature = "std")]
fn json_to_serde_value(json: &azul_core::json::Json) -> serde_json::Value {
    use azul_core::json::JsonType;
    match json.value_type {
        JsonType::Null => serde_json::Value::Null,
        JsonType::Bool => match json.as_bool() {
            azul_css::OptionBool::Some(b) => serde_json::Value::Bool(b),
            azul_css::OptionBool::None => serde_json::Value::Null,
        },
        JsonType::Number => match json.as_number() {
            azul_css::OptionF64::Some(n) => {
                if n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
                    serde_json::Value::Number(serde_json::Number::from(n as i64))
                } else {
                    serde_json::Number::from_f64(n)
                        .map(serde_json::Value::Number)
                        .unwrap_or(serde_json::Value::Null)
                }
            }
            azul_css::OptionF64::None => serde_json::Value::Null,
        },
        JsonType::String => serde_json::Value::String(json.raw_string().to_string()),
        JsonType::Array | JsonType::Object => {
            // Internal storage is a JSON string — just parse it
            serde_json::from_str(json.raw_string()).unwrap_or(serde_json::Value::Null)
        }
    }
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
        "img" | "image" => vec![
            ("src", "String"),
            ("alt", "String"),
            ("width", "String"),
            ("height", "String"),
        ],
        "input" => vec![
            ("type", "String"),
            ("name", "String"),
            ("value", "String"),
            ("placeholder", "String"),
            ("required", "bool"),
            ("disabled", "bool"),
            ("readonly", "bool"),
            ("checked", "bool"),
            ("min", "String"),
            ("max", "String"),
            ("step", "String"),
            ("pattern", "String"),
            ("maxlength", "i32"),
            ("minlength", "i32"),
            ("autocomplete", "String"),
        ],
        "button" => vec![
            ("type", "String"),
            ("name", "String"),
            ("value", "String"),
            ("disabled", "bool"),
        ],
        "form" => vec![("action", "String"), ("method", "String")],
        "label" => vec![("for", "String")],
        "select" => vec![
            ("name", "String"),
            ("required", "bool"),
            ("disabled", "bool"),
        ],
        "option" => vec![
            ("value", "String"),
            ("selected", "bool"),
            ("disabled", "bool"),
        ],
        "textarea" => vec![
            ("name", "String"),
            ("placeholder", "String"),
            ("rows", "i32"),
            ("cols", "i32"),
            ("required", "bool"),
            ("disabled", "bool"),
            ("readonly", "bool"),
            ("maxlength", "i32"),
        ],
        "td" | "th" => vec![("colspan", "i32"), ("rowspan", "i32"), ("scope", "String")],
        "meta" => vec![
            ("charset", "String"),
            ("name", "String"),
            ("content", "String"),
        ],
        "link" => vec![("href", "String"), ("rel", "String"), ("type", "String")],
        "script" => vec![
            ("src", "String"),
            ("type", "String"),
            ("defer", "bool"),
            ("async", "bool"),
        ],
        "source" => vec![("src", "String"), ("type", "String")],
        "video" | "audio" => vec![
            ("src", "String"),
            ("controls", "bool"),
            ("autoplay", "bool"),
            ("loop", "bool"),
        ],
        "canvas" => vec![("width", "String"), ("height", "String")],
        "virtual-view" => vec![("src", "String"), ("width", "String"), ("height", "String")],
        "icon" => vec![("name", "String")],
        "meter" => vec![
            ("value", "String"),
            ("min", "String"),
            ("max", "String"),
            ("low", "String"),
            ("high", "String"),
        ],
        "progress" => vec![("value", "String"), ("max", "String")],
        _ => vec![],
    }
}

/// Build the nested DOM tree returned by `DebugEvent::GetDom`.
///
/// `assert_dom` evaluates against the SAME builder, so an E2E test asserts the
/// exact structure the op hands out — not a parallel re-implementation of it.
#[cfg(feature = "std")]
fn build_dom_response(
    callback_info: &azul_layout::callbacks::CallbackInfo,
    dom_id: azul_core::dom::DomId,
) -> Option<DomResponse> {
    struct Flat {
        node_type: String,
        id: Option<String>,
        classes: Vec<String>,
        text: Option<String>,
        children: Vec<usize>,
    }

    fn assemble(index: usize, flat: &[Flat]) -> DomNodeJson {
        let f = &flat[index];
        DomNodeJson {
            index,
            node_type: f.node_type.clone(),
            id: f.id.clone(),
            classes: f.classes.clone(),
            text: f.text.clone(),
            children: f.children.iter().map(|c| assemble(*c, flat)).collect(),
        }
    }

    let dom_id = ROOT_DOM_ID;
    let layout_result = callback_info
        .get_layout_window()
        .layout_results
        .get(&dom_id)?;
    let styled_dom = &layout_result.styled_dom;
    let hierarchy = styled_dom.node_hierarchy.as_container();
    let node_data = styled_dom.node_data.as_container();
    let root = styled_dom.root.into_crate_internal()?;

    let mut flat = Vec::with_capacity(hierarchy.len());
    for i in 0..hierarchy.len() {
        let node_id = azul_core::id::NodeId::new(i);
        let data = &node_data[node_id];

        let mut id_attr = None;
        let mut classes = Vec::new();
        for attr in data.attributes().as_ref().iter() {
            if let Some(id) = attr.as_id() {
                id_attr = Some(id.to_string());
            } else if let Some(class) = attr.as_class() {
                classes.push(class.to_string());
            }
        }

        let text = match data.get_node_type() {
            azul_core::dom::NodeType::Text(t) => Some(t.as_str().to_string()),
            _ => None,
        };

        flat.push(Flat {
            node_type: data.get_node_type().get_path().to_string(),
            id: id_attr,
            classes,
            text,
            children: node_id.az_children(&hierarchy).map(|c| c.index()).collect(),
        });
    }

    Some(DomResponse {
        dom_id: dom_id.inner,
        node_count: flat.len(),
        html: styled_dom.get_html_string("", "", true),
        root: assemble(root.index(), &flat),
    })
}

/// The `RefAny` payload of the timer that [`DebugEvent::AddTimer`] installs.
///
/// A `Timer`'s data is an opaque `RefAny`, so this is how the op smuggles the
/// per-registration parameters (which node, what text) into a callback whose
/// signature is a bare `extern "C" fn`.
#[cfg(feature = "std")]
#[derive(Debug, Clone)]
struct E2eTickTimerData {
    node: azul_core::dom::DomNodeId,
    text: String,
}

/// The callback [`DebugEvent::AddTimer`] installs: rewrite the target node's
/// text to `"<text> <run_count>"`.
///
/// The run count is what makes each expiry OBSERVABLE. `CallbackChange::
/// ChangeNodeText` short-circuits a write of the byte-identical string (and is
/// right to — re-shaping identical text is the maximum work for a no-op), so a
/// timer that wrote a constant would produce damage exactly once and every
/// later expiry would be indistinguishable from a timer that never fired,
/// which is precisely the failure mode the removal half of the test has to
/// detect.
///
/// It writes through `CallbackInfo`, so the mutation travels the normal
/// `CallbackChange` path out of `run_single_timer` and back through
/// `apply_user_change` — the same route an app's own timer callback takes.
#[cfg(feature = "std")]
extern "C" fn e2e_tick_timer_callback(
    mut data: azul_core::refany::RefAny,
    mut info: azul_layout::timer::TimerCallbackInfo,
) -> azul_core::callbacks::TimerCallbackReturn {
    use azul_core::{
        callbacks::{TimerCallbackReturn, Update},
        task::TerminateTimer,
    };

    let Some(payload) = data.downcast_ref::<E2eTickTimerData>() else {
        // Only reachable if this module built the timer with a different
        // payload type. Terminate rather than spin a timer that can never do
        // anything — a silently-running no-op timer is the exact thing the
        // scenario's final beat is trying to rule out.
        return TimerCallbackReturn {
            should_update: Update::DoNothing,
            should_terminate: TerminateTimer::Terminate,
        };
    };
    let node = payload.node;
    let text = azul_css::AzString::from(format!("{} {}", payload.text, info.call_count));
    drop(payload);

    info.callback_info.change_node_text(node, text);

    // `DoNothing`, not `RefreshDom`: the queued `ChangeNodeText` already
    // relayouts and rebuilds the display list, whereas `RefreshDom` would
    // re-run the app's layout callback and throw the mutation away — the same
    // trap documented on the `set_node_text` op.
    TimerCallbackReturn {
        should_update: Update::DoNothing,
        should_terminate: TerminateTimer::Continue,
    }
}

/// Process a single debug event
///
/// # An INPUT op must never set `needs_update`
///
/// `needs_update` is the step loop's "the application asked for a DOM rebuild"
/// flag: the debug timer turns it into `Update::RefreshDom`, which the event
/// loop answers with an unconditional `regenerate_layout()` — a full DOM
/// rebuild and a full repaint, taken REGARDLESS of what the engine's own
/// event-determination / invalidation pipeline decided.
///
/// For an op whose whole job is to deliver input that is a false-evidence
/// factory, in both directions:
///
/// * It MANUFACTURES the damage the test then measures. Delete the engine's
///   `:hover` invalidation path entirely and a `mouse_move` still repaints the
///   window, still resolves `:hover` during the rebuild and still reports
///   non-empty damage — so `assert_changed` goes green against an engine that
///   does not invalidate anything.
/// * It makes `dom_regenerations >= 1` unconditionally, so no test can ever
///   assert that an inert event costs zero DOM regenerations.
///
/// This is the same defect `tick_ms` had (fixed in b44cb702b): the harness was
/// the work it was measuring. `Scroll` / `KeyDown` / `KeyUp` / `TextInput` /
/// `Focus` / `Blur` / `Move` were already written this way and carry an
/// explicit "do NOT set needs_update" note each; every pointer, touch, pen,
/// gesture, resize and DPI arm now follows the same model.
///
/// The engine still decides — and still regenerates where it should:
/// `modify_window_state` pushes `CallbackChange::ModifyWindowState`, whose
/// handler runs the state-diff pass when something actually changed and sets
/// `resize_pending` for a size/DPI delta (which the runner turns into
/// `ShouldRegenerateDomCurrentWindow` on its own, exactly as
/// `request_regeneration` does in the real shell).
///
/// `mount` / `unmount` / `remount` / `set_app_state` / `update_component` DO
/// still set it: those are the ops that legitimately mean "rebuild the DOM".
#[cfg(feature = "std")]
pub fn process_debug_event(
    request: &DebugRequest,
    callback_info: &mut azul_layout::callbacks::CallbackInfo,
    app_data: &mut azul_core::refany::RefAny,
    component_map: &Arc<Mutex<azul_core::xml::ComponentMap>>,
    // This window's E2E scheduler slot. Only `RunE2eTests` touches it; every
    // other op ignores it. Passing it explicitly is what replaced the
    // process-global continuation.
    session: &mut E2eSession,
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
            // NO `needs_update` — see the note on `process_debug_event`. The
            // size delta is what forces the rebuild: `ModifyWindowState` sets
            // `resize_pending` / `request_regeneration` for it, so the
            // regeneration is the ENGINE's decision, not the harness's.

            send_ok(request, None, None);
        }

        // ─── Window focus / move / DPI ───────────────────────────────
        //
        // These three drive the SAME engine path a real platform event
        // drives: the platform shells (WM_SETFOCUS/WM_KILLFOCUS,
        // X11 FocusIn/FocusOut, ConfigureNotify, WM_DPICHANGED,
        // wl_output scale) all do exactly one thing — they mutate
        // `current_window_state` and let the state-diff pass
        // (`process_window_events` → `event_determination`) derive the
        // synthetic `WindowFocusIn` / `WindowFocusOut` / `WindowMove` /
        // `WindowResize` events from current-vs-previous.
        //
        // From inside a callback the only door onto that path is
        // `CallbackChange::ModifyWindowState`, which every shell applies
        // through the single cross-platform `apply_user_change()`
        // (dll/src/desktop/shell2/common/event.rs). That handler saves
        // `previous_window_state`, applies the fields and runs the diff
        // pass — i.e. the identical sequence WM_SETFOCUS performs.
        DebugEvent::Focus => {
            log(
                LogLevel::Info,
                LogCategory::Window,
                "Window focus gained (debug)",
                None,
            );
            let mut new_state = callback_info.get_current_window_state().clone();
            new_state.window_focused = true;
            new_state.flags.has_focus = true;
            callback_info.modify_window_state(new_state);
            // NOTE: deliberately NO `needs_update`. ModifyWindowState already
            // runs the state-diff pass + repaint. `needs_update` would mean
            // Update::RefreshDom — a full DOM rebuild, which a real focus
            // event does not do.
            send_ok(request, None, None);
        }

        DebugEvent::Blur => {
            log(
                LogLevel::Info,
                LogCategory::Window,
                "Window focus lost (debug)",
                None,
            );
            let mut new_state = callback_info.get_current_window_state().clone();
            new_state.window_focused = false;
            new_state.flags.has_focus = false;
            callback_info.modify_window_state(new_state);
            send_ok(request, None, None);
        }

        DebugEvent::FocusNode { selector, node_id } => {
            use azul_core::callbacks::FocusTarget;
            use azul_core::dom::DomNodeId;
            use azul_core::styled_dom::NodeHierarchyItemId;

            let target = resolve_node_target(
                callback_info,
                target_dom(request),
                selector.as_deref(),
                *node_id,
                None,
            );
            let described = selector.clone().unwrap_or_else(|| {
                node_id.map_or_else(|| "<nothing>".to_string(), |n| format!("node {n}"))
            });

            // Resolve + validate BEFORE pushing the change. The runner's
            // `SetFocusTarget` arm answers an unresolvable target with
            // `DoNothing` — silently — so a test that focused a node that does
            // not exist, or one that can never hold focus, would run its whole
            // keyboard timeline against no focus at all and blame the engine.
            let focusable = target.and_then(|nid| {
                callback_info
                    .get_layout_window()
                    .layout_results
                    .get(&target_dom(request))
                    .and_then(|lr| {
                        lr.styled_dom
                            .node_data
                            .as_container()
                            .get(nid)
                            .map(azul_core::dom::NodeData::is_focusable)
                    })
            });

            match (target, focusable) {
                (Some(nid), Some(true)) => {
                    callback_info.set_focus(FocusTarget::Id(DomNodeId {
                        dom: target_dom(request),
                        node: NodeHierarchyItemId::from_crate_internal(Some(nid)),
                    }));
                    send_ok(
                        request,
                        None,
                        Some(ResponseData::FocusNode(FocusNodeResponse {
                            node_id: nid.index() as u64,
                            selector: selector.clone(),
                        })),
                    );
                }
                (Some(nid), _) => send_err(
                    request,
                    format!(
                        "focus_node: '{described}' resolves to node {} but that node cannot hold \
                         focus (not a/button/input/select/textarea, not contenteditable, no \
                         tabindex and no focus callback). Focusing it would leave the focus \
                         manager empty and every following keyboard step would test nothing.",
                        nid.index()
                    ),
                ),
                (None, _) => send_err(
                    request,
                    format!("focus_node: no node matches '{described}'"),
                ),
            }
        }

        // ─── Accessibility ───────────────────────────────────────────
        //
        // The only door in the whole op set onto
        // `LayoutWindow::process_accessibility_action`. Everything else here
        // drives the engine through mouse/keyboard/window state; assistive
        // technology does not, it addresses a node and names an action, and the
        // action → `EventFilter` mapping in between is code no other op reaches.
        //
        // Applied via `CallbackChange::PerformAccessibilityAction`, i.e. the
        // same path the platform `process_accessibility_actions()` pumps use —
        // so a green test here is evidence about what a screen reader gets, not
        // about a test-only shortcut.
        DebugEvent::AccessibilityAction {
            selector,
            node_id,
            text,
            action,
            value,
            number,
            x,
            y,
            selection_start,
            selection_end,
            custom_id,
        } => {
            let described = selector
                .clone()
                .or_else(|| text.as_ref().map(|t| format!("text {t:?}")))
                .unwrap_or_else(|| {
                    node_id.map_or_else(|| "<nothing>".to_string(), |n| format!("node {n}"))
                });

            let parsed = parse_accessibility_action(
                action,
                value.as_deref(),
                *number,
                *x,
                *y,
                *selection_start,
                *selection_end,
                *custom_id,
            );

            let target = resolve_node_target(
                callback_info,
                target_dom(request),
                selector.as_deref(),
                *node_id,
                text.as_deref(),
            );

            // Is the node one assistive technology can actually see? Same
            // predicate the accesskit tree builder uses, so this cannot drift
            // from what a screen reader is offered.
            let exposed = target.and_then(|nid| {
                callback_info
                    .get_layout_window()
                    .layout_results
                    .get(&target_dom(request))
                    .and_then(|lr| {
                        lr.styled_dom
                            .node_data
                            .as_container()
                            .get(nid)
                            .map(azul_layout::managers::a11y::is_exposed_to_accessibility)
                    })
            });

            match (target, exposed, parsed) {
                (_, _, Err(why)) => {
                    send_err(request, format!("accessibility_action: {why}"));
                }
                (None, _, _) => send_err(
                    request,
                    format!("accessibility_action: no node matches '{described}'"),
                ),
                (Some(nid), None, _) => send_err(
                    request,
                    format!(
                        "accessibility_action: '{described}' resolves to node {} but that node is \
                         not in the root DOM's layout results, so there is nothing to act on",
                        nid.index()
                    ),
                ),
                (Some(nid), Some(false), _) => send_err(
                    request,
                    format!(
                        "accessibility_action: '{described}' resolves to node {} but that node is \
                         NOT exposed to assistive technology (metadata / pseudo-element with no \
                         a11y info, not focusable, not contenteditable). No screen reader can \
                         reach it, so an action on it proves nothing about accessibility.",
                        nid.index()
                    ),
                ),
                (Some(nid), Some(true), Ok(parsed_action)) => {
                    log(
                        LogLevel::Info,
                        LogCategory::EventLoop,
                        format!(
                            "Accessibility action '{action}' on node {} ({described})",
                            nid.index()
                        ),
                        None,
                    );
                    callback_info.perform_accessibility_action(target_dom(request), nid, parsed_action);
                    // NO `needs_update` — see the note on `process_debug_event`.
                    // The change itself decides what re-render is owed, exactly
                    // like a real adapter's action does.
                    send_ok(request, None, None);
                }
            }
        }

        DebugEvent::Move { x, y } => {
            log(
                LogLevel::Info,
                LogCategory::Window,
                format!("Moving window to ({}, {})", x, y),
                None,
            );
            let mut new_state = callback_info.get_current_window_state().clone();
            new_state.position = azul_core::window::WindowPosition::Initialized(
                azul_core::geom::PhysicalPositionI32 { x: *x, y: *y },
            );
            callback_info.modify_window_state(new_state);
            // Same as focus: the diff pass turns the position delta into a
            // WindowMove event; `sync_window_state()` pushes the new position
            // to the OS window on the platforms that have one.
            send_ok(request, None, None);
        }

        DebugEvent::DpiChanged { dpi } => {
            // A real DPI change (WM_DPICHANGED / NSWindow backingScaleFactor /
            // wl_surface.preferred_buffer_scale) keeps the LOGICAL window size
            // and re-derives the physical/backing size from the new scale, then
            // forces a full relayout + full repaint (every glyph and border was
            // rasterised at the old scale).
            let new_dpi = (*dpi).max(1);
            let old_dpi = callback_info.get_current_window_state().size.dpi;
            log(
                LogLevel::Info,
                LogCategory::Window,
                format!("DPI change {} -> {}", old_dpi, new_dpi),
                None,
            );
            let mut new_state = callback_info.get_current_window_state().clone();
            new_state.size.dpi = new_dpi;
            callback_info.modify_window_state(new_state);
            // The relayout+repaint a real DPI change causes is driven by the DPI
            // delta itself: `ModifyWindowState` sets `resize_pending` /
            // `request_regeneration`. NO `needs_update` — see the note
            // on `process_debug_event`.

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
            // NO `needs_update` — see the note on `process_debug_event`.

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
            // NO `needs_update` — see the note on `process_debug_event`.

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
            // NO `needs_update` — see the note on `process_debug_event`.

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
                let dom_id = target_dom(request);
                let dom_node_id = DomNodeId {
                    dom: dom_id,
                    node: Some(NodeId::new(*nid as usize)).into(),
                };
                node_centre_for_click(callback_info, dom_node_id)
            } else if let Some(sel) = selector {
                // Click by CSS selector using matches_html_element
                use azul_core::style::matches_html_element;
                use azul_css::parser2::parse_css_path;

                let dom_id = target_dom(request);
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
                                    dom: dom_id,
                                    node: Some(NodeId::new(i)).into(),
                                };
                                // Hit-test bounds where they exist, laid-out
                                // rect otherwise — see `node_centre_for_click`.
                                if let Some(c) = node_centre_for_click(callback_info, dom_node_id) {
                                    found = Some(c);
                                    break;
                                }
                            }
                        }
                    }
                }
                found
            } else if let Some(txt) = text {
                // Click by text content
                let dom_id = target_dom(request);
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
                                    dom: dom_id,
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
                                    dom: dom_id,
                                    node: Some(NodeId::new(parent_idx)).into(),
                                };
                                // Use get_node_hit_test_bounds for reliable positions from display list
                                if let Some(c) =
                                    node_centre_for_click(callback_info, parent_dom_node_id)
                                {
                                    found = Some(c);
                                    break;
                                } else if let Some(c) =
                                    node_centre_for_click(callback_info, dom_node_id)
                                {
                                    found = Some(c);
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
                        .queue_window_state_sequence(vec![move_state, down_state, up_state].into());
                    // NO `needs_update` — see the note on `process_debug_event`.
                    // `QueueWindowStateSequence` runs one state-diff pass per
                    // queued state on its own.

                    let response = ClickNodeResponse {
                        success: true,
                        message: format!("Clicked at ({:.1}, {:.1})", cx, cy),
                    };
                    send_ok(request, None, Some(ResponseData::ClickNode(response)));
                }
                None => {
                    // send_err, NOT send_ok-with-success-false. The step loop maps
                    // any Ok(DebugResponseData::Ok{..}) to status "pass" without
                    // reading the payload, so a `success: false` body was invisible
                    // and `{"op":"click","selector":".typo"}` PASSED while queueing
                    // no window-state change at all.
                    //
                    // Every sibling node-addressed op already does this —
                    // FocusNode, ScrollNodeBy/To, ScrollIntoView, GetNodeLayout,
                    // Insert/Delete/SetNodeText/SetNodeClasses/SetNodeCssOverride,
                    // TextInput — and FocusNode states the reason: a test that
                    // acted on a node that does not exist "would run its whole
                    // keyboard timeline against no focus at all and blame the
                    // engine".
                    send_err(
                        request,
                        "click: could not resolve the click target (no matching node or position)"
                            .to_string(),
                    );
                }
            }
        }

        DebugEvent::DoubleClick {
            x,
            y,
            selector,
            node_id,
            text,
            button,
        } => {
            // Same target resolution as `click`: explicit position, node id,
            // CSS selector or text content. Coordinate-only double-clicks are
            // brittle against layout changes, and the ribbon's collapse
            // gesture is exactly the case that needs selector targeting.
            let Some((x, y)) = resolve_click_position(
                callback_info,
                target_dom(request),
                x.as_ref(),
                y.as_ref(),
                node_id.as_ref(),
                selector.as_ref(),
                text.as_ref(),
            ) else {
                send_err(request, "double_click: could not resolve a target position");
                return true;
            };
            let (x, y) = (&x, &y);

            log(
                LogLevel::Debug,
                LogCategory::EventLoop,
                format!("Debug double click at ({}, {}) button {:?}", x, y, button),
                None,
            );

            // A real double click is TWO complete press/release cycles at the
            // same position: the gesture manager's `detect_double_click`
            // requires two *ended* input sessions within its time+distance
            // thresholds (layout/src/managers/gesture.rs). A single
            // press/release only ever produces one session, so the previous
            // "just do a click for now" implementation could never fire a
            // `HoverEventFilter::DoubleClick` handler.
            let mut new_state = callback_info.get_current_window_state().clone();
            new_state.mouse_state.cursor_position =
                azul_core::window::CursorPosition::InWindow(LogicalPosition { x: *x, y: *y });

            let set_button_down = |state: &mut azul_layout::window_state::FullWindowState,
                                   down: bool| match button {
                MouseButton::Left => state.mouse_state.left_down = down,
                MouseButton::Right => state.mouse_state.right_down = down,
                MouseButton::Middle => state.mouse_state.middle_down = down,
            };

            for _ in 0..2 {
                set_button_down(&mut new_state, true);
                callback_info.modify_window_state(new_state.clone());
                set_button_down(&mut new_state, false);
                callback_info.modify_window_state(new_state.clone());
            }

            // Two press/release cycles alone rely on the gesture manager's
            // time+distance heuristic seeing two *ended* input sessions, which
            // in turn depends on how many frames the injected window states
            // are processed in. Platforms report a double click as a native
            // gesture, and `GestureAndDragManager::detect_double_click`
            // honours that directly, so inject it too: the op then delivers a
            // `DoubleClick` deterministically instead of hoping the heuristic
            // fires.
            callback_info.inject_native_gesture(
                azul_layout::managers::gesture::NativeGestureEvent::DoubleClick,
            );
            // NO `needs_update` — see the note on `process_debug_event`.

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
                    // The cursor is in WINDOW space, so the node's rect has to
                    // be too. `calculated_positions` is DOM-LOCAL: for a
                    // VirtualView's document (or any child DOM) its origin is
                    // the child's own, so comparing the two spaces matched the
                    // wrong node — or none — and a wheel over AzWriter's page
                    // scrolled nothing at all. `get_node_rect` is the same
                    // window-space rect `get_node_layout` reports.
                    let rect = callback_info.get_node_rect(azul_core::dom::DomNodeId {
                        dom: *dom_id,
                        node: Some(node_id).into(),
                    });
                    if let Some(r) = rect {
                        if cursor_pos.x >= r.origin.x
                            && cursor_pos.x <= r.origin.x + r.size.width
                            && cursor_pos.y >= r.origin.y
                            && cursor_pos.y <= r.origin.y + r.size.height
                        {
                            scroll_node = Some((*dom_id, node_id));
                            break;
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
            // NOTE: Do NOT set needs_update = true here!
            // callback_info.scroll_to() already pushes CallbackChange::ScrollTo
            // which will be processed by the event system as a lightweight repaint
            // (ShouldReRenderCurrentWindow → build_image_only_transaction).
            // Setting needs_update would cause Update::RefreshDom → full DOM rebuild
            // (~1s for 500 rows), during which the scrollbar opacity fades to 0.

            send_ok(request, None, None);
        }

        DebugEvent::Mount { html, css } => {
            if html.is_empty() {
                send_err(request, "mount: 'html' is empty".to_string());
            } else {
                let xml = build_mount_document(html, css);
                // Validate up-front so a broken test fails at the mount step
                // rather than silently rendering the app's own DOM.
                match azul_layout::xml::parse_xml_to_styled_dom(&xml) {
                    Ok(_) => {
                        // Route the document through the SAME change pipeline
                        // every other op uses; the shell applies it to
                        // `LayoutWindow::e2e_mount` and `regenerate_layout`
                        // reads it back. No process-global sink.
                        callback_info.push_change(
                            azul_layout::callbacks::CallbackChange::RemountDom {
                                xml: Some(xml.into()),
                            },
                        );
                        needs_update = true; // → Update::RefreshDom → regenerate_layout
                        send_ok(request, None, None);
                    }
                    Err(e) => {
                        send_err(request, format!("mount: XML parse error: {e:?}"));
                    }
                }
            }
        }

        DebugEvent::Unmount => {
            callback_info
                .push_change(azul_layout::callbacks::CallbackChange::RemountDom { xml: None });
            needs_update = true;
            send_ok(request, None, None);
        }

        DebugEvent::SnapshotFrame { name } => {
            #[cfg(feature = "cpurender")]
            {
                match callback_info.take_screenshot(target_dom(request)) {
                    Ok(png) => {
                        scratch(callback_info)
                            .frame_snapshots
                            .insert(name.clone(), png);
                        send_ok(request, None, None);
                    }
                    Err(e) => send_err(
                        request,
                        format!("snapshot_frame: screenshot failed: {}", e.as_str()),
                    ),
                }
            }
            #[cfg(not(feature = "cpurender"))]
            {
                let _ = name;
                send_err(request, "snapshot_frame: cpurender not enabled".to_string());
            }
        }

        DebugEvent::SnapshotResources { name } => {
            let counts = collect_resource_counts(callback_info);
            scratch(callback_info)
                .resource_snapshots
                .insert(name.clone(), counts);
            send_ok(request, None, None);
        }

        DebugEvent::SnapshotManagers { name } => {
            let prints = manager_fingerprints(callback_info.get_layout_window());
            scratch(callback_info)
                .manager_snapshots
                .insert(name.clone(), prints);
            send_ok(request, None, None);
        }

        DebugEvent::TickMs { ms } => {
            let total = azul_core::task::advance_test_clock_ms(*ms);
            log(
                LogLevel::Debug,
                LogCategory::EventLoop,
                format!("[E2E] tick_ms: +{ms} ms (clock offset now {total} ms)"),
                None,
            );
            // Engine time is ONE clock. A real shell ticks the animation
            // manager every rendered frame, so advancing the test clock by
            // N ms must advance animations by the same N ms — otherwise a
            // scenario that remounts a DOM (which retains exit zombies, by
            // design, on EVERY unmount) and then `tick_ms 2000`s to settle
            // its timers is left with zombies that never finish, and every
            // idle assertion after it fails on perpetual zombie damage.
            // Fixed 16.6 ms substeps because the spring integrator is
            // semi-implicit Euler — one 2000 ms step is outside its stable
            // region, and a real shell never hands it more than a frame.
            callback_info.push_change(azul_layout::callbacks::CallbackChange::TickAnimations {
                dt_micros: 16_666,
                steps: u32::try_from((*ms).div_ceil(16).max(1)).unwrap_or(u32::MAX),
            });
            // Force a frame so that time-driven state (fade / momentum / blink /
            // animation) actually advances and re-renders; an idle engine then
            // reports FrameDamage::None, which is what `assert_idle_stable` asserts.
            //
            // A REPAINT, not `needs_update`: advancing a clock is not a reason
            // to rebuild the DOM, and charging the window a DOM regeneration per
            // tick made every "an idle frame does no work" assertion
            // unanswerable. See `request_repaint`.
            request_repaint(callback_info);
            send_ok(request, None, None);
        }

        DebugEvent::AddTimer {
            timer_id,
            interval_ms,
            node_id,
            text,
        } => {
            use azul_core::{
                dom::{DomNodeId, NodeType},
                id::NodeId,
                refany::RefAny,
                styled_dom::NodeHierarchyItemId,
                task::{Duration, SystemTimeDiff, TimerId, USER_TIMER_ID_START},
            };
            use azul_layout::timer::{Timer, TimerCallback};

            let target = NodeId::new(*node_id as usize);
            let is_text_node = callback_info
                .get_layout_window()
                .layout_results
                .get(&target_dom(request))
                .and_then(|lr| {
                    lr.styled_dom
                        .node_data
                        .as_container()
                        .get(target)
                        .map(|n| matches!(n.get_node_type(), NodeType::Text(_)))
                });

            // Every rejection below is a REAL reason, reported by name. A timer
            // that is silently not registered, or registered onto a node whose
            // text can never change, produces a scenario that asserts "nothing
            // happened" and passes for the wrong reason.
            if (*timer_id as usize) < USER_TIMER_ID_START {
                send_err(
                    request,
                    format!(
                        "add_timer: timer_id {timer_id} is inside the RESERVED system-timer block \
                         (< {USER_TIMER_ID_START}); registering there would overwrite an engine \
                         timer (caret blink / scroll momentum / tooltip / long press) instead of \
                         testing one. Use an id >= {USER_TIMER_ID_START}."
                    ),
                );
            } else if *interval_ms == 0 {
                send_err(
                    request,
                    "add_timer: interval_ms must be > 0 — a zero interval fires on every single \
                     pump, which is a busy loop, not a timer"
                        .to_string(),
                );
            } else if is_text_node != Some(true) {
                send_err(
                    request,
                    format!(
                        "add_timer: node {node_id} is not a NodeType::Text node (found: {}), so \
                         the timer's ChangeNodeText would be dropped and every expiry would be \
                         invisible. Address the TEXT node, exactly like set_node_text.",
                        match is_text_node {
                            None => "no such node",
                            Some(_) => "a non-text node",
                        }
                    ),
                );
            } else {
                let timer = Timer {
                    refany: RefAny::new(E2eTickTimerData {
                        node: DomNodeId {
                            dom: target_dom(request),
                            node: NodeHierarchyItemId::from_crate_internal(Some(target)),
                        },
                        text: text.clone(),
                    }),
                    node_id: None.into(),
                    created: azul_core::task::Instant::now(),
                    run_count: 0,
                    last_run: azul_core::task::OptionInstant::None,
                    delay: azul_core::task::OptionDuration::None,
                    interval: azul_core::task::OptionDuration::Some(Duration::System(
                        SystemTimeDiff::from_millis(*interval_ms),
                    )),
                    timeout: azul_core::task::OptionDuration::None,
                    callback: TimerCallback::create(e2e_tick_timer_callback),
                };
                // Through `CallbackInfo`, NOT straight into `LayoutWindow`:
                // pushing `CallbackChange::AddTimer` is the whole point, because
                // that arm is what this op exists to exercise.
                callback_info.add_timer(
                    TimerId {
                        id: *timer_id as usize,
                    },
                    timer,
                );
                send_ok(request, None, None);
            }
        }

        DebugEvent::RemoveTimer { timer_id } => {
            use azul_core::task::{TimerId, USER_TIMER_ID_START};

            if (*timer_id as usize) < USER_TIMER_ID_START {
                send_err(
                    request,
                    format!(
                        "remove_timer: timer_id {timer_id} is inside the RESERVED system-timer \
                         block (< {USER_TIMER_ID_START}); tearing down an engine timer from a \
                         scenario would corrupt engine state, not test it."
                    ),
                );
            } else {
                callback_info.remove_timer(TimerId {
                    id: *timer_id as usize,
                });
                send_ok(request, None, None);
            }
        }

        DebugEvent::GetFrameReport => {
            let report = frame_report_of(callback_info);
            send_ok(
                request,
                None,
                Some(ResponseData::FrameReport(build_frame_report_response(
                    &report,
                    window_logical_area(callback_info),
                ))),
            );
        }

        DebugEvent::CaptureDamagePng { path, which, crop } => {
            #[cfg(feature = "cpurender")]
            {
                match capture_damage_png(callback_info, which.as_deref(), *crop) {
                    // Create the parent directory first. Scenarios write to
                    // `target/e2e/<name>.png`, which does not exist in a fresh
                    // checkout — so every `capture_damage_png` step failed on CI
                    // while passing locally, where a previous run had already
                    // left the directory behind. A scenario must not depend on
                    // leftover build artefacts to pass.
                    Ok(png) => {
                        let parent_ok = std::path::Path::new(path.as_str())
                            .parent()
                            .filter(|p| !p.as_os_str().is_empty())
                            .map_or(Ok(()), std::fs::create_dir_all);
                        match parent_ok.and_then(|()| std::fs::write(path, &png)) {
                            Ok(()) => send_ok(request, None, None),
                            Err(e) => send_err(
                                request,
                                format!("capture_damage_png: cannot write {path}: {e}"),
                            ),
                        }
                    }
                    Err(e) => send_err(request, format!("capture_damage_png: {e}")),
                }
            }
            #[cfg(not(feature = "cpurender"))]
            {
                let _ = (path, which, crop);
                send_err(
                    request,
                    "capture_damage_png: cpurender not enabled".to_string(),
                );
            }
        }

        DebugEvent::ResetFrameCounters => {
            // Zeroes the work counters AND the accumulated damage at the next
            // frame-report write (an assertion only holds `&LayoutWindow`, so the
            // reset goes through a per-window generation counter — see
            // `LayoutWindow::request_frame_report_reset`). Readers go through
            // `frame_report_of`, which applies the pending reset, so the
            // checkpoint is observable immediately rather than at the next
            // frame.
            callback_info
                .get_layout_window()
                .request_frame_report_reset();
            // Same checkpoint semantics for `assert_composition`'s stage trace.
            e2e_reset_composition_trace(callback_info);
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
            let dom_id = target_dom(request);
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

        DebugEvent::CustomOp { name, args } => {
            // The fn pointer is copied out BEFORE `app_data` is touched: the
            // handler lives on the (immutably borrowed) LayoutWindow and the
            // ctx it receives is mutable app state, so holding both borrows
            // at once would not compile.
            let handler = callback_info.get_layout_window().custom_e2e_op.cb;
            let ctx = match callback_info.get_layout_window().custom_e2e_op.ctx.as_ref() {
                Some(c) => c.clone(),
                // No explicit ctx: hand the app its OWN data. That is what a
                // "now load the document" hook needs, and RefAny is
                // refcounted so the clone is a bump.
                None => app_data.clone(),
            };
            let args_json = serde_json::to_string(args).unwrap_or_else(|_| "null".to_string());
            let out = (handler)(
                ctx,
                azul_css::AzString::from(name.clone()),
                azul_css::AzString::from(args_json),
            );
            let raw = out.json.as_str().to_string();
            if out.handled {
                let response = CustomOpResponse {
                    op: name.clone(),
                    result: serde_json::from_str(&raw).ok(),
                    raw,
                };
                send_ok(request, None, Some(ResponseData::CustomOp(response)));
            } else {
                // NOT an OK with an empty body. An unrecognised op name is the
                // most likely thing to be wrong in a scenario — a typo, or a
                // handler that was never installed — and reporting it as
                // success makes it invisible, since non-assert ops print
                // nothing of their own.
                send_err(
                    request,
                    format!(
                        "custom op '{name}' was not handled: the application's \
                         AppConfig::custom_e2e_op returned handled=false (no handler \
                         installed, or the name is not one it recognises)"
                    ),
                );
            }
        }

        DebugEvent::GetProfileReport { kind } => {
            let mut r = ProfileResponse {
                logs_dropped: logs_dropped(),
                ..Default::default()
            };
            if let Some(c) = azul_layout::probe::rss_census() {
                r.rss_kib = c.total_kib;
                r.heap_kib = c.heap_kib;
                r.anon_kib = c.anon_kib;
                r.binary_kib = c.binary_kib;
                r.shared_libs_kib = c.shared_libs_kib;
                r.font_files_kib = c.font_files_kib;
                r.framebuffer_kib = c.framebuffer_kib;
            }
            if let Some(a) = azul_layout::probe::allocator_stats() {
                r.allocator_live_kib = Some(a.live_bytes / 1024);
                r.allocator_free_in_arena_kib = Some(a.free_in_arena_bytes / 1024);
            }
            if *kind == ProfileKind::Cpu {
                // Phase timings are DRAINED by the probe, so reading them
                // consumes them. Only the Cpu kind pays that cost — a memory
                // snapshot must not silently eat another consumer's timings.
                // Only `Span` events carry a duration; `Rss` events are
                // labelled checkpoints, not phases, and folding them in would
                // report a byte count as microseconds.
                //
                // NOTE: `Probe::drain()` is a no-op without the `probe`
                // cargo feature, so `phases_us` comes back EMPTY on a stock
                // build. Empty is honest here — the field is omitted from the
                // JSON entirely rather than serialised as zeros.
                r.phases_us = azul_layout::probe::Probe::drain()
                    .into_iter()
                    .filter_map(|e| match e.kind {
                        azul_layout::probe::EventKind::Span { dur_ns } => {
                            Some((e.name.to_string(), dur_ns / 1000))
                        }
                        azul_layout::probe::EventKind::Rss { .. } => None,
                    })
                    .collect();
            }
            // Also LOG the snapshot, not just answer the HTTP request.
            //
            // Non-assert ops reply through the debug channel and log nothing,
            // so with no HTTP client attached this op produced no observable
            // output at all — it ran correctly and looked like it had not run.
            // A profile you cannot see is a profile you cannot act on, and the
            // whole point of the op is that a scenario run leaves a record of
            // what memory did.
            log(
                LogLevel::Info,
                LogCategory::DebugServer,
                match serde_json::to_string(&r) {
                    Ok(j) => format!("[PROFILE] {j}"),
                    Err(e) => format!("[PROFILE] serialise failed: {e}"),
                },
                None,
            );
            send_ok(request, None, Some(ResponseData::Profile(r)));
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
            // "Drive one frame and let it land before the next step" — which is
            // how every assertion of ABSENCE in the corpus is documented to be
            // driven ("`wait_frame` × K → `assert_idle_stable`"). This used to
            // be a pure no-op that sent `ok` and did nothing at all, so those
            // scenarios asserted against whatever frame the mount had left
            // behind. Same repaint-only path as `tick_ms`: no DOM regeneration,
            // no event pass, and a yield so the frame is rendered before the
            // step after it reads the frame report.
            request_repaint(callback_info);
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
            let dom_id = target_dom(request);
            match callback_info.take_screenshot_base64(dom_id) {
                Ok(data_uri) => {
                    let data = ScreenshotData {
                        data: data_uri.as_str().to_string(),
                    };
                    // A base64 blob in a JSON response is not something a human
                    // can look at. With AZ_E2E_SHOT_DIR set, every screenshot is
                    // also written to disk, numbered in capture order — which is
                    // what makes a mid-animation sequence inspectable at all.
                    // Unset (the default, and always in CI) this does nothing.
                    #[cfg(feature = "std")]
                    if let Some(dir) = std::env::var_os("AZ_E2E_SHOT_DIR") {
                        let dir = std::path::PathBuf::from(dir);
                        let _ = std::fs::create_dir_all(&dir);
                        static SHOT_N: core::sync::atomic::AtomicUsize =
                            core::sync::atomic::AtomicUsize::new(0);
                        let n = SHOT_N.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                        let raw = data.data.rsplit(",").next().unwrap_or("");
                        if let Ok(bytes) = base64_decode_for_shot(raw) {
                            let _ = std::fs::write(dir.join(format!("shot-{n:03}.png")), bytes);
                        }
                    }
                    send_ok(request, None, Some(ResponseData::Screenshot(data)));
                }
                Err(e) => {
                    send_err(request, e.as_str().to_string());
                }
            }
        }

        DebugEvent::Print { ref text } => {
            // Straight to stderr as well as the log: a scenario's own
            // narration should be visible whatever the log level is.
            std::eprintln!("[e2e] {text}");
            log(LogLevel::Info, LogCategory::General, text, None);
            send_ok(request, None, None);
        }

        DebugEvent::PrintResponse => {
            // The runner records each step's response; the executor prints the
            // most recent one (see `LAST_RESPONSE`).
            let last = LAST_RESPONSE
                .lock()
                .ok()
                .and_then(|g| g.clone())
                .unwrap_or_else(|| "<no previous response>".to_string());
            std::eprintln!("[e2e] response: {last}");
            send_ok(request, None, None);
        }

        DebugEvent::TakeNativeScreenshot => {
            log(
                LogLevel::Info,
                LogCategory::Rendering,
                "Taking native screenshot via debug API",
                None,
            );
            // Use the NativeScreenshotExt trait method explicitly (not the stubbed inherent method)
            match crate::e2e::hooks::take_native_screenshot_base64(callback_info) {
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

        DebugEvent::GetDom => {
            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                "Getting DOM",
                None,
            );
            match build_dom_response(callback_info, target_dom(request)) {
                Some(dom) => send_ok(request, None, Some(ResponseData::Dom(dom))),
                None => send_err(request, "No layout result for DOM 0"),
            }
        }

        DebugEvent::GetHtmlString => {
            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                "Getting HTML string",
                None,
            );
            let dom_id = target_dom(request);
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

            let resolved_node_id = resolve_node_target(
                callback_info,
                target_dom(request),
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
                dom: target_dom(request),
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

        DebugEvent::TickAnimations { dt_micros, steps } => {
            let dt_micros = dt_micros.unwrap_or(16_666);
            let steps = steps.unwrap_or(1).max(1);
            // Mutation goes through the sanctioned channel: CallbackInfo hands
            // out `&LayoutWindow` only, and `apply_system_change` is where the
            // mutable window lives.
            callback_info.push_change(azul_layout::callbacks::CallbackChange::TickAnimations {
                dt_micros,
                steps,
            });
            let active = callback_info.get_layout_window().animations.len();
            send_ok(
                request,
                None,
                Some(ResponseData::TickAnimations(TickAnimationsResponse {
                    dt_micros,
                    steps,
                    active,
                })),
            );
            // NOT needs_update: that would escalate every tick to a FULL DOM
            // regeneration (layout callback re-run!), drowning the cheap
            // paths this op exists to exercise. The queued TickAnimations
            // change already returns the exact result the tick needs —
            // DL-only for GPU-value animations, incremental relayout for
            // layout-scoped transitions, nothing for an idle tick.
            return false;
        }
        DebugEvent::GetAnimations => {
            let lw = callback_info.get_layout_window();
            let cache = lw.gpu_state_manager.caches.get(&target_dom(request));
            let mut nodes = Vec::new();
            for (key, node_id) in &lw.anim_key_to_node {
                let Some(anim) = lw.animations.get(*key) else {
                    continue;
                };
                let t = anim.current_transform();
                nodes.push(AnimationNodeJson {
                    node_id: node_id.index() as u64,
                    translate_x: t.translate_x,
                    translate_y: t.translate_y,
                    scale_x: t.scale_x,
                    scale_y: t.scale_y,
                    opacity: anim.current_opacity(),
                    finished: anim.is_finished(),
                    published: cache
                        .map(|c| c.anim_current_transform_values.contains_key(node_id))
                        .unwrap_or(false),
                });
            }
            let active = lw.animations.len();
            let zombies = lw.zombies.len();
            let transitions = lw.css_transitions.len();
            let zombie_relayouts = lw.zombie_relayouts;
            let live_tracks = lw.live_tracks.len();
            send_ok(
                request,
                None,
                Some(ResponseData::Animations(AnimationsResponse {
                    active,
                    zombies,
                    transitions,
                    zombie_relayouts,
                    live_tracks,
                    nodes,
                })),
            );
            return needs_update;
        }
        DebugEvent::GetNodeLayout {
            node_id,
            selector,
            text,
        } => {
            use azul_core::dom::{DomId, DomNodeId, NodeId};

            let resolved_node_id = resolve_node_target(
                callback_info,
                target_dom(request),
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
                dom: target_dom(request),
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

            let dom_id = target_dom(request);
            let layout_window = callback_info.get_layout_window();

            let mut nodes = Vec::new();
            if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
                let node_count = layout_result.styled_dom.node_data.len();
                for i in 0..node_count {
                    let dom_node_id = DomNodeId {
                        dom: dom_id,
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
                dom_id: dom_id.inner as u32,
                node_count: nodes.len(),
                nodes,
            };
            send_ok(request, None, Some(ResponseData::AllNodesLayout(response)));
        }

        DebugEvent::ListDoms => {
            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                "Listing live DOMs",
                None,
            );
            use azul_core::dom::{DomId, NodeType};
            use azul_core::id::NodeId;

            // VirtualView documents announce their host, so a caller can tell
            // "the AzWriter document" from "some other nested DOM" without
            // opening each one.
            let vv_parent: alloc::collections::BTreeMap<usize, DomNodeIdJson> = callback_info
                .get_layout_window()
                .virtual_view_manager
                .get_all_virtual_view_infos()
                .iter()
                .map(|i| {
                    (
                        i.nested_dom_id,
                        DomNodeIdJson {
                            dom_id: i.parent_dom_id as u64,
                            node_id: i.parent_node_id as u64,
                        },
                    )
                })
                .collect();

            let layout_window = callback_info.get_layout_window();
            let mut doms: Vec<DomListEntry> = Vec::new();
            let mut ids: Vec<DomId> = layout_window.layout_results.keys().copied().collect();
            ids.sort_by_key(|d| d.inner);

            for id in ids {
                let Some(lr) = layout_window.layout_results.get(&id) else {
                    continue;
                };
                let styled_dom = &lr.styled_dom;
                let node_data = styled_dom.node_data.as_container();
                let root = styled_dom
                    .root
                    .into_crate_internal()
                    .unwrap_or(NodeId::ZERO);
                let root_tag = node_data
                    .get(root)
                    .map(|d| alloc::format!("{:?}", d.get_node_type().get_path()).to_lowercase())
                    .unwrap_or_else(|| "?".to_string());
                let root_selector = build_selector_for_node(callback_info, id, root);
                let size = callback_info
                    .get_node_rect(azul_core::dom::DomNodeId {
                        dom: id,
                        node: Some(root).into(),
                    })
                    .map(|r| LogicalSizeJson {
                        width: r.size.width,
                        height: r.size.height,
                    });
                doms.push(DomListEntry {
                    dom_id: id.inner as u64,
                    is_root: id == ROOT_DOM_ID,
                    node_count: node_data.len(),
                    root_tag,
                    root_selector,
                    virtual_view_parent: vv_parent.get(&id.inner).copied(),
                    size,
                });
            }

            let response = DomListResponse {
                dom_count: doms.len(),
                doms,
            };
            send_ok(request, None, Some(ResponseData::DomList(response)));
        }

        DebugEvent::GetDomTree => {
            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                "Getting DOM tree",
                None,
            );
            use azul_core::dom::DomId;

            let dom_id = target_dom(request);
            let layout_window = callback_info.get_layout_window();

            if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
                let styled_dom = &layout_result.styled_dom;
                let window_state = callback_info.get_current_window_state();

                let node_count = styled_dom.node_data.len();
                let dpi = window_state.size.dpi;
                let hidpi = window_state.size.get_hidpi_factor().inner.get();
                let logical_size = &window_state.size.dimensions;

                let response = DomTreeResponse {
                    dom_id: dom_id.inner as u32,
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

            let dom_id = target_dom(request);
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

                    // Extract ID and classes from attributes
                    let mut id_attr = None;
                    let mut classes = Vec::new();
                    for attr in data.attributes().as_ref().iter() {
                        if let Some(id) = attr.as_id() {
                            id_attr = Some(id.to_string());
                        } else if let Some(class) = attr.as_class() {
                            classes.push(class.to_string());
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
                    let events: Vec<NodeEventInfo> = data
                        .callbacks
                        .as_ref()
                        .iter()
                        .map(|cb| NodeEventInfo {
                            event: format!("{:?}", cb.event),
                            callback_ptr: format!("0x{:x}", cb.callback.cb),
                        })
                        .collect();

                    // Get layout rect
                    let dom_node_id = azul_core::dom::DomNodeId {
                        dom: dom_id,
                        node: Some(NodeId::new(i)).into(),
                    };
                    let rect = callback_info
                        .get_node_rect(dom_node_id)
                        .map(|r| LogicalRectJson {
                            x: r.origin.x,
                            y: r.origin.y,
                            width: r.size.width,
                            height: r.size.height,
                        });

                    // Tab index
                    let tab_index = match data.get_tab_index() {
                        Some(ti) => match ti {
                            azul_core::dom::TabIndex::Auto => Some(0),
                            azul_core::dom::TabIndex::OverrideInParent(v) => Some(v as i32),
                            azul_core::dom::TabIndex::NoKeyboardFocus => Some(-1),
                        },
                        None => None,
                    };

                    // Extract component origin (if this node was rendered by a component)
                    let component = data.get_component_origin().map(|origin| {
                        // Convert azul_core::json::Json → serde_json::Value via the raw string
                        let dm_value = json_to_serde_value(&origin.data_model_json);
                        ComponentOriginJson {
                            component_id: origin.component_id.as_str().to_string(),
                            data_model: dm_value,
                        }
                    });

                    // Check if dataset is present
                    let has_dataset = data.get_dataset().map(|_| true);

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
                        contenteditable: data.is_contenteditable(),
                        component,
                        has_dataset,
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

            let dom_id = target_dom(request);
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

                    let cold = layout_tree.cold(LayoutNodeId::new(idx));
                    nodes.push(LayoutNodeInfo {
                        layout_idx: idx,
                        dom_idx,
                        node_type: node_type.to_string(),
                        is_anonymous: node.dom_node_id.is_none(),
                        anonymous_type: cold
                            .and_then(|c| c.anonymous_type.as_ref().map(|t| format!("{:?}", t))),
                        formatting_context: format!("{:?}", node.formatting_context),
                        parent: node.parent.map(|p| p as i64).unwrap_or(-1),
                        children: layout_tree.children(idx).to_vec(),
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

            let dom_id = target_dom(request);
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
                                x: Some(bounds.0.origin.x),
                                y: Some(bounds.0.origin.y),
                                width: Some(bounds.0.size.width),
                                height: Some(bounds.0.size.height),
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
                                x: Some(clip_rect.0.origin.x),
                                y: Some(clip_rect.0.origin.y),
                                width: Some(clip_rect.0.size.width),
                                height: Some(clip_rect.0.size.height),
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
                                x: Some(bounds.0.origin.x),
                                y: Some(bounds.0.origin.y),
                                width: Some(bounds.0.size.width),
                                height: Some(bounds.0.size.height),
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
                                azul_css::css::CssPropertyValue::Exact(c) => Some(c.inner.to_pixels_internal(0.0, 16.0, 16.0)),
                                _ => None,
                            });
                            let extract_right_width = widths.right.as_ref().and_then(|c| match c {
                                azul_css::css::CssPropertyValue::Exact(c) => Some(c.inner.to_pixels_internal(0.0, 16.0, 16.0)),
                                _ => None,
                            });
                            let extract_bottom_width = widths.bottom.as_ref().and_then(|c| match c {
                                azul_css::css::CssPropertyValue::Exact(c) => Some(c.inner.to_pixels_internal(0.0, 16.0, 16.0)),
                                _ => None,
                            });
                            let extract_left_width = widths.left.as_ref().and_then(|c| match c {
                                azul_css::css::CssPropertyValue::Exact(c) => Some(c.inner.to_pixels_internal(0.0, 16.0, 16.0)),
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
                                x: Some(bounds.0.origin.x),
                                y: Some(bounds.0.origin.y),
                                width: Some(bounds.0.size.width),
                                height: Some(bounds.0.size.height),
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
                                x: Some(bounds.0.origin.x),
                                y: Some(bounds.0.origin.y),
                                width: Some(bounds.0.size.width),
                                height: Some(bounds.0.size.height),
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
                                x: Some(bounds.0.origin.x),
                                y: Some(bounds.0.origin.y),
                                width: Some(bounds.0.size.width),
                                height: Some(bounds.0.size.height),
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
                                "track:({:.1},{:.1},{:.1}x{:.1}) thumb:({:.1},{:.1},{:.1}x{:.1}) track_color:#{:02x}{:02x}{:02x}{:02x} thumb_color:#{:02x}{:02x}{:02x}{:02x} opacity_key:{} transform_key:{}",
                                info.track_bounds.0.origin.x, info.track_bounds.0.origin.y,
                                info.track_bounds.0.size.width, info.track_bounds.0.size.height,
                                info.thumb_bounds.0.origin.x, info.thumb_bounds.0.origin.y,
                                info.thumb_bounds.0.size.width, info.thumb_bounds.0.size.height,
                                info.track_color.r, info.track_color.g, info.track_color.b, info.track_color.a,
                                info.thumb_color.r, info.thumb_color.g, info.thumb_color.b, info.thumb_color.a,
                                info.opacity_key.as_ref().map(|k| format!("{}", k.id)).unwrap_or_else(|| "none".to_string()),
                                info.thumb_transform_key.as_ref().map(|k| format!("{}", k.id)).unwrap_or_else(|| "none".to_string()),
                            );
                            DisplayListItemInfo {
                                index: idx,
                                item_type: format!("scrollbar_{}", orient_str),
                                x: Some(info.bounds.0.origin.x),
                                y: Some(info.bounds.0.origin.y),
                                width: Some(info.bounds.0.size.width),
                                height: Some(info.bounds.0.size.height),
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
                                x: Some(bounds.0.origin.x),
                                y: Some(bounds.0.origin.y),
                                width: Some(bounds.0.size.width),
                                height: Some(bounds.0.size.height),
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
                                x: Some(bounds.0.origin.x),
                                y: Some(bounds.0.origin.y),
                                width: Some(bounds.0.size.width),
                                height: Some(bounds.0.size.height),
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
                                x: Some(bounds.0.origin.x),
                                y: Some(bounds.0.origin.y),
                                width: Some(bounds.0.size.width),
                                height: Some(bounds.0.size.height),
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
                                x: Some(bounds.0.origin.x),
                                y: Some(bounds.0.origin.y),
                                width: Some(bounds.0.size.width),
                                height: Some(bounds.0.size.height),
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

            let dom_id = target_dom(request);
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

            let dom_id = target_dom(request);
            let layout_window = callback_info.get_layout_window();

            // Get scrollable nodes from layout tree
            let mut scrollable_nodes = Vec::new();

            if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
                // Check each node in the layout tree to see if it has scrollbar_info (warm tier)
                for (node_idx, node) in layout_result.layout_tree.nodes.iter().enumerate() {
                    let scrollbar_info = layout_result
                        .layout_tree
                        .warm(LayoutNodeId::new(node_idx))
                        .and_then(|w| w.scrollbar_info.as_ref());
                    if let Some(scrollbar_info) = scrollbar_info {
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
                target_dom(request),
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

            let dom_id = target_dom(request);
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
                target_dom(request),
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

            let dom_id = target_dom(request);
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
            use azul_core::events::{
                ScrollIntoViewBehavior, ScrollIntoViewOptions, ScrollLogicalPosition,
            };
            use azul_core::id::NodeId;
            use azul_core::styled_dom::NodeHierarchyItemId;
            use azul_core::task::Instant;

            let resolved_node_id = resolve_node_target(
                callback_info,
                target_dom(request),
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

            let dom_id = target_dom(request);
            let node = NodeId::new(nid as usize);
            let dom_node_id = DomNodeId {
                dom: dom_id,
                node: NodeHierarchyItemId::from_crate_internal(Some(node)),
            };

            // Remember WHAT we asked to be scrolled into view. `scroll_into_view`
            // itself keeps no record (it applies its adjustments and drops them),
            // so cross-invariant X1 would otherwise have no subject and could only
            // ever pass vacuously. See `E2eScratch::last_scroll_into_view`.
            scratch(callback_info).last_scroll_into_view = Some((dom_id, node));

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

            let dom_id = target_dom(request);
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
                                dom: dom_id,
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

            let dom_id = target_dom(request);
            let dom_node_id = DomNodeId {
                dom: dom_id,
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
                // NO `needs_update` — see the note on `process_debug_event`.

                let response = ClickNodeResponse {
                    success: true,
                    message: format!("Clicked node {} at ({}, {})", node_id, center_x, center_y),
                };
                send_ok(request, None, Some(ResponseData::ClickNode(response)));
            } else {
                // send_err for the same reason as `click` above: a `success: false`
                // payload is never inspected, so this reported "pass" for a node
                // that does not exist or has no rect.
                send_err(
                    request,
                    format!("click_node: node {node_id} not found or has no rect"),
                );
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
                target_dom(request),
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

            let dom_id = target_dom(request);
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

            fn build_scrollbar_geometry(
                state: &azul_layout::managers::scroll_state::ScrollbarState,
            ) -> ScrollbarGeometryJson {
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

                ScrollbarGeometryJson {
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

        DebugEvent::GetVirtualViewStates => {
            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                "Getting virtualized view states",
                None,
            );

            let layout_window = callback_info.get_layout_window();
            let infos = layout_window
                .virtual_view_manager
                .get_all_virtual_view_infos();

            let virtual_views: Vec<VirtualViewStateInfo> = infos
                .iter()
                .map(|info| VirtualViewStateInfo {
                    parent_dom_id: info.parent_dom_id,
                    parent_node_id: info.parent_node_id,
                    nested_dom_id: info.nested_dom_id,
                    scroll_size_width: info.scroll_size_width,
                    scroll_size_height: info.scroll_size_height,
                    virtual_scroll_size_width: info.virtual_scroll_size_width,
                    virtual_scroll_size_height: info.virtual_scroll_size_height,
                    was_invoked: info.was_invoked,
                    last_bounds: LogicalRectJson {
                        x: info.last_bounds_x,
                        y: info.last_bounds_y,
                        width: info.last_bounds_width,
                        height: info.last_bounds_height,
                    },
                })
                .collect();

            let response = VirtualViewStatesResponse {
                virtual_view_count: virtual_views.len(),
                virtual_views,
            };
            send_ok(
                request,
                None,
                Some(ResponseData::VirtualViewStates(response)),
            );
        }

        DebugEvent::GetVirtualViewLayout { dom_id, node_id } => {
            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                "Getting virtualized view layout",
                None,
            );
            use azul_core::dom::{DomId, DomNodeId, NodeId};

            let layout_window = callback_info.get_layout_window();

            // Resolve the nested dom_id: either provided directly, or look up via parent node_id
            let nested_dom_id = if let Some(did) = dom_id {
                Some(DomId { inner: *did })
            } else if let Some(nid) = node_id {
                let parent_dom = target_dom(request);
                layout_window
                    .virtual_view_manager
                    .get_nested_dom_id(parent_dom, NodeId::new(*nid))
            } else {
                None
            };

            if let Some(nested_dom_id) = nested_dom_id {
                // Get scroll state for the VirtualView container from parent DOM
                let scroll_state = if let Some(nid) = node_id {
                    let parent_dom = target_dom(request);
                    let parent_node = NodeId::new(*nid);

                    // Get scroll offset from scroll manager
                    let scroll_states = layout_window
                        .scroll_manager
                        .get_scroll_states_for_dom(parent_dom);
                    scroll_states.get(&parent_node).map(|sp| {
                        let infos = layout_window
                            .virtual_view_manager
                            .get_all_virtual_view_infos();
                        let virtual_view_info = infos.iter().find(|i| i.parent_node_id == *nid);

                        VirtualViewScrollStateInfo {
                            scroll_x: sp.children_rect.origin.x,
                            scroll_y: sp.children_rect.origin.y,
                            content_width: sp.children_rect.size.width,
                            content_height: sp.children_rect.size.height,
                            container_width: sp.parent_rect.size.width,
                            container_height: sp.parent_rect.size.height,
                            virtual_scroll_width: virtual_view_info
                                .and_then(|i| i.virtual_scroll_size_width),
                            virtual_scroll_height: virtual_view_info
                                .and_then(|i| i.virtual_scroll_size_height),
                            max_scroll_x: (sp.children_rect.size.width - sp.parent_rect.size.width)
                                .max(0.0),
                            max_scroll_y: (sp.children_rect.size.height
                                - sp.parent_rect.size.height)
                                .max(0.0),
                        }
                    })
                } else {
                    None
                };

                // Diagnostics: list all dom_ids in layout_results
                let available_dom_ids: Vec<usize> = layout_window
                    .layout_results
                    .keys()
                    .map(|d| d.inner)
                    .collect();
                let layout_result_found = layout_window.layout_results.contains_key(&nested_dom_id);

                // Get nodes from the virtualized view's layout result
                let mut nodes = Vec::new();
                if let Some(layout_result) = layout_window.layout_results.get(&nested_dom_id) {
                    let node_count = layout_result.styled_dom.node_data.len();
                    for i in 0..node_count {
                        let dom_node_id = DomNodeId {
                            dom: nested_dom_id,
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

                let response = VirtualViewLayoutResponse {
                    dom_id: nested_dom_id.inner,
                    node_count: nodes.len(),
                    nodes,
                    scroll_state,
                    available_dom_ids,
                    layout_result_found,
                };
                send_ok(
                    request,
                    None,
                    Some(ResponseData::VirtualViewLayout(response)),
                );
            } else {
                send_err(
                    request,
                    "No virtualized view found: specify dom_id or node_id. Use get_virtual_view_states to list all virtualized views.",
                );
            }
        }

        DebugEvent::GetSelectionState => {
            let layout_window = callback_info.get_layout_window();
            let mut selections = Vec::new();
            if let Some(ref mc) = layout_window.text_edit_manager.multi_cursor {
                let dom_id = mc.node_id.dom;
                let node_id = mc
                    .node_id
                    .node
                    .into_crate_internal()
                    .map(|n| n.index() as u64);
                let selector = mc
                    .node_id
                    .node
                    .into_crate_internal()
                    .and_then(|nid| build_selector_for_node(callback_info, dom_id, nid));
                let mut ranges = Vec::new();
                for s in &mc.selections {
                    use azul_core::selection::Selection;
                    let range_info = match &s.selection {
                        Selection::Cursor(cursor) => SelectionRangeInfo {
                            selection_type: "cursor".to_string(),
                            cursor_position: Some(cursor.cluster_id.start_byte_in_run as usize),
                            start: None,
                            end: None,
                            direction: None,
                        },
                        Selection::Range(range) => {
                            let sp = range.start.cluster_id.start_byte_in_run as usize;
                            let ep = range.end.cluster_id.start_byte_in_run as usize;
                            SelectionRangeInfo {
                                selection_type: "range".to_string(),
                                cursor_position: None,
                                start: Some(sp),
                                end: Some(ep),
                                direction: Some(
                                    if sp <= ep { "forward" } else { "backward" }.to_string(),
                                ),
                            }
                        }
                    };
                    ranges.push(range_info);
                }
                selections.push(DomSelectionInfo {
                    dom_id: dom_id.inner as u32,
                    node_id,
                    selector,
                    ranges,
                    rectangles: Vec::new(),
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
            let layout_window = callback_info.get_layout_window();
            let mut selections = Vec::new();
            if let Some(ref mc) = layout_window.text_edit_manager.multi_cursor {
                let dom_id = mc.node_id.dom;
                let node_id = mc
                    .node_id
                    .node
                    .into_crate_internal()
                    .map(|n| n.index() as u64);
                let selector = mc
                    .node_id
                    .node
                    .into_crate_internal()
                    .and_then(|nid| build_selector_for_node(callback_info, dom_id, nid));
                let mut sel_dumps = Vec::new();
                for s in &mc.selections {
                    use azul_core::selection::Selection;
                    sel_dumps.push(SelectionDump {
                        selection_type: match &s.selection {
                            Selection::Cursor(_) => "cursor".to_string(),
                            Selection::Range(_) => "range".to_string(),
                        },
                        debug: alloc::format!("{:?}", s.selection),
                    });
                }
                selections.push(SelectionDumpEntry {
                    dom_id: dom_id.inner as u32,
                    node_id,
                    selector,
                    selections: sel_dumps,
                });
            }
            let response = SelectionManagerDump {
                selections,
                click_state: ClickStateDump {
                    last_node: None,
                    last_position: LogicalPositionJson { x: 0.0, y: 0.0 },
                    last_time_ms: 0,
                    click_count: 0,
                },
            };
            send_ok(
                request,
                None,
                Some(ResponseData::SelectionManagerDump(response)),
            );
        }

        DebugEvent::GetDragState => {
            // Get current drag state from unified drag system
            let layout_window = callback_info.get_layout_window();
            let gesture_manager = &layout_window.gesture_drag_manager;

            let (is_dragging, drag_type, description) = if let Some(drag_ctx) =
                gesture_manager.get_drag_context()
            {
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
                        ActiveDragType::TextSelection(sel) => (
                            "text_selection",
                            None,
                            None,
                            None,
                            Some(sel.anchor_ifc_node.index() as u64),
                            Some(sel.dom_id.inner as u32),
                        ),
                        ActiveDragType::ScrollbarThumb(sb) => {
                            let axis = match sb.axis {
                                azul_core::drag::ScrollbarAxis::Horizontal => "horizontal",
                                azul_core::drag::ScrollbarAxis::Vertical => "vertical",
                            };
                            (
                                "scrollbar_thumb",
                                Some(axis.to_string()),
                                None,
                                None,
                                Some(sb.scroll_container_node.index() as u64),
                                None,
                            ) // ScrollbarThumbDrag doesn't have dom_id
                        }
                        ActiveDragType::Node(nd) => (
                            "node",
                            None,
                            None,
                            None,
                            Some(nd.node_id.index() as u64),
                            Some(nd.dom_id.inner as u32),
                        ),
                        ActiveDragType::WindowMove(_) => {
                            ("window_move", None, None, None, None, None)
                        }
                        ActiveDragType::WindowResize(wr) => {
                            let edge = alloc::format!("{:?}", wr.edge);
                            ("window_resize", None, Some(edge), None, None, None)
                        }
                        ActiveDragType::FileDrop(fd) => {
                            let file_list: Vec<String> = fd
                                .files
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
                    drag_data: None,   // TODO: convert DragData to BTreeMap
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
                        let value = json.to_serde_value();
                        let response = AppStateResponse {
                            metadata,
                            state: value,
                            error: None,
                        };
                        send_ok(request, None, Some(ResponseData::AppState(response)));
                    }
                    None => {
                        let response = AppStateResponse {
                            metadata,
                            state: serde_json::Value::Null,
                            error: Some(RefAnyError::SerdeError(
                                "Serialization returned null".to_string(),
                            )),
                        };
                        send_ok(request, None, Some(ResponseData::AppState(response)));
                    }
                }
            }
        }

        DebugEvent::SetAppState { state } => {
            use azul_layout::json::Json;

            // Get deserialize_fn from RefAny
            let deserialize_fn = app_data.get_deserialize_fn();

            if deserialize_fn == 0 {
                // send_err, not send_ok-with-success-false: the step loop maps any
                // Ok response to "pass" without reading the payload, so all three
                // of this op's failure paths reported success. A scenario that
                // seeds state with set_app_state and then asserts behaviour would
                // run its whole timeline against the ORIGINAL state and blame the
                // engine.
                send_err(
                    request,
                    "set_app_state: the app's RefAny has no deserialize fn, so state cannot be \
                     restored into it"
                        .to_string(),
                );
            } else {
                // Convert serde_json::Value to our Json type
                let json_string = state.to_string();
                match Json::parse(&json_string) {
                    Ok(json) => {
                        // Shared restore (preserves the live serialize/deserialize/update
                        // hooks across replace_contents) — same path as RefAnyUndoManager.
                        match azul_layout::json::restore_refany_from_json(app_data, json) {
                            Ok(()) => {
                                needs_update = true;
                                let response = AppStateSetResponse {
                                    success: true,
                                    error: None,
                                };
                                send_ok(request, None, Some(ResponseData::AppStateSet(response)));
                            }
                            Err(e) => {
                                send_err(
                                    request,
                                    format!("set_app_state: could not restore the app state: {e}"),
                                );
                            }
                        }
                    }
                    Err(e) => {
                        send_err(
                            request,
                            alloc::format!(
                                "set_app_state: the supplied state is not valid JSON: {e:?}"
                            ),
                        );
                    }
                }
            }
        }

        DebugEvent::GetNodeDataset { node_id } => {
            use azul_core::dom::{DomId, NodeId};
            use azul_layout::json::serialize_refany_to_json;

            let dom_id = target_dom(request);
            let layout_window = callback_info.get_layout_window();

            if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
                let node_data = layout_result.styled_dom.node_data.as_container();
                let nid = *node_id as usize;

                if nid < node_data.len() {
                    let data = &node_data[NodeId::new(nid)];
                    match data.get_dataset() {
                        Some(refany) => {
                            let metadata = RefAnyMetadata {
                                type_id: refany.get_type_id(),
                                type_name: refany.get_type_name().as_str().to_string(),
                                can_serialize: refany.can_serialize(),
                                can_deserialize: refany.can_deserialize(),
                                ref_count: refany.get_ref_count(),
                            };

                            if !refany.can_serialize() {
                                let response = NodeDatasetResponse {
                                    node_id: *node_id,
                                    metadata,
                                    dataset: serde_json::Value::Null,
                                    error: Some(RefAnyError::NotSerializable),
                                };
                                send_ok(request, None, Some(ResponseData::NodeDataset(response)));
                            } else {
                                match serialize_refany_to_json(refany) {
                                    Some(json) => {
                                        let value = json.to_serde_value();
                                        let response = NodeDatasetResponse {
                                            node_id: *node_id,
                                            metadata,
                                            dataset: value,
                                            error: None,
                                        };
                                        send_ok(
                                            request,
                                            None,
                                            Some(ResponseData::NodeDataset(response)),
                                        );
                                    }
                                    None => {
                                        let response = NodeDatasetResponse {
                                            node_id: *node_id,
                                            metadata,
                                            dataset: serde_json::Value::Null,
                                            error: Some(RefAnyError::SerdeError(
                                                "Serialization returned null".to_string(),
                                            )),
                                        };
                                        send_ok(
                                            request,
                                            None,
                                            Some(ResponseData::NodeDataset(response)),
                                        );
                                    }
                                }
                            }
                        }
                        None => {
                            send_err(request, alloc::format!("Node {} has no dataset", node_id));
                        }
                    }
                } else {
                    send_err(
                        request,
                        alloc::format!("Node {} out of range (max {})", node_id, node_data.len()),
                    );
                }
            } else {
                send_err(request, "No layout result for DOM 0");
            }
        }

        DebugEvent::KeyDown {
            key,
            modifiers,
            text,
        } => {
            use azul_core::window::{VirtualKeyCode, VirtualKeyCodeVec};

            log(
                LogLevel::Debug,
                LogCategory::EventLoop,
                format!(
                    "Debug key down: {} (shift={}, ctrl={}, alt={})",
                    key, modifiers.shift, modifiers.ctrl, modifiers.alt
                ),
                None,
            );

            // SHELL INGRESS. Every native backend records the keystroke's
            // printable text into the changeset and then runs ONE state-diff
            // pass, so the KeyDown and the Input event share a pass and the
            // pass's post-callback filter decides whether the edit lands:
            //
            //   x11/events.rs      layout_window.record_text_input(text);
            //                      … apply_key_state_change(…);
            //                      self.process_window_events(0)
            //
            // Recorded here, BEFORE `modify_window_state` — the changes drain
            // in push order, so `SetTextChangeset` (record only, `DoNothing`,
            // and the SAME arm in both hosts) lands before `ModifyWindowState`
            // runs the pass.
            //
            // KNOWN DELTA: `set_changeset` onto an empty queue labels the entry
            // `TextInputSource::Programmatic`, so the `Input` event this raises
            // carries `EventSource::Programmatic` where a real keystroke
            // carries `User`. Nothing dispatches on that label — it is a label
            // — and closing it needs a `record`-with-source `CallbackChange`
            // that does not exist yet.
            if let Some(text) = text.as_deref().filter(|t| !t.is_empty()) {
                let layout_window = callback_info.get_layout_window();
                let recorded = layout_window
                    .focus_manager
                    .get_focused_node()
                    .copied()
                    .and_then(|focused| {
                        let node_id = focused.node.into_crate_internal()?;
                        let old_inline =
                            layout_window.get_text_before_textinput(focused.dom, node_id);
                        let old_text = layout_window.extract_text_from_inline_content(&old_inline);
                        Some(azul_layout::managers::text_input::PendingTextEdit {
                            node: focused,
                            inserted_text: text.into(),
                            old_text: old_text.into(),
                        })
                    });
                if let Some(changeset) = recorded {
                    callback_info.set_text_changeset(changeset);
                }
            }

            let mut new_state = callback_info.get_current_window_state().clone();

            // Collect current keys into a Vec
            let mut pressed_keys: alloc::vec::Vec<VirtualKeyCode> = new_state
                .keyboard_state
                .pressed_virtual_keycodes
                .iter()
                .copied()
                .collect();

            // Parse the key string to VirtualKeyCode
            if let Some(keycode) = parse_virtual_keycode(key) {
                // Add the key to pressed keys if not already present
                if !pressed_keys.contains(&keycode) {
                    pressed_keys.push(keycode);
                }
                new_state.keyboard_state.current_virtual_keycode = Some(keycode).into();
            }

            // Set modifier keys based on modifiers struct
            if modifiers.shift && !pressed_keys.contains(&VirtualKeyCode::LShift) {
                pressed_keys.push(VirtualKeyCode::LShift);
            }
            if modifiers.ctrl && !pressed_keys.contains(&VirtualKeyCode::LControl) {
                pressed_keys.push(VirtualKeyCode::LControl);
            }
            if modifiers.alt && !pressed_keys.contains(&VirtualKeyCode::LAlt) {
                pressed_keys.push(VirtualKeyCode::LAlt);
            }
            if modifiers.meta && !pressed_keys.contains(&VirtualKeyCode::LWin) {
                pressed_keys.push(VirtualKeyCode::LWin);
            }

            new_state.keyboard_state.pressed_virtual_keycodes =
                VirtualKeyCodeVec::from_vec(pressed_keys);
            callback_info.modify_window_state(new_state);
            // NOTE: Do NOT set needs_update = true here!
            // modify_window_state() pushes a CallbackChange::ModifyWindowState which
            // triggers process_window_events() internally. Setting needs_update would
            // cause Update::RefreshDom → full DOM rebuild, overwriting text edits.

            // Backspace / Delete are deliberately NOT special-cased here — they
            // ride the exact route a real keystroke takes. The
            // `current_virtual_keycode` set above makes the state-diff pass emit
            // VirtualKeyDown(Back/Delete), whose default action is
            // `SystemChange::ApplySelectionOp { Delete }` →
            // `LayoutWindow::apply_selection_op` → `delete_selection`.
            //
            // This used to shortcut them to `callback_info.delete_backward()` /
            // `delete_forward()` (the C-API arm), on the false premise that native
            // macOS routes them through `doCommandBySelector:` — that selector is
            // a NO-OP; the real macOS `keyDown:` falls through to
            // `handle_key_down` → the SAME ApplySelectionOp path. The shortcut
            // BYPASSED `apply_selection_op`, so keyboard delete could be entirely
            // dead (as it was for TextInput/TextArea, whose IFC lives on a value
            // child the focused host block does not carry) while every e2e
            // backspace test still passed. Driving the real path keeps the
            // headless e2e route and the native keyDown route one code path.

            send_ok(request, None, None);
        }

        DebugEvent::KeyUp { key, modifiers } => {
            use azul_core::window::{VirtualKeyCode, VirtualKeyCodeVec};

            log(
                LogLevel::Debug,
                LogCategory::EventLoop,
                format!(
                    "Debug key up: {} (shift={}, ctrl={}, alt={})",
                    key, modifiers.shift, modifiers.ctrl, modifiers.alt
                ),
                None,
            );

            let mut new_state = callback_info.get_current_window_state().clone();

            // Collect current keys into a Vec
            let mut pressed_keys: alloc::vec::Vec<VirtualKeyCode> = new_state
                .keyboard_state
                .pressed_virtual_keycodes
                .iter()
                .copied()
                .collect();

            // Parse the key string to VirtualKeyCode and remove it
            if let Some(keycode) = parse_virtual_keycode(key) {
                pressed_keys.retain(|k| *k != keycode);
                new_state.keyboard_state.current_virtual_keycode = None.into();
            }

            // Remove modifier keys if modifiers struct says they should be released
            if !modifiers.shift {
                pressed_keys
                    .retain(|k| *k != VirtualKeyCode::LShift && *k != VirtualKeyCode::RShift);
            }
            if !modifiers.ctrl {
                pressed_keys
                    .retain(|k| *k != VirtualKeyCode::LControl && *k != VirtualKeyCode::RControl);
            }
            if !modifiers.alt {
                pressed_keys.retain(|k| *k != VirtualKeyCode::LAlt && *k != VirtualKeyCode::RAlt);
            }
            if !modifiers.meta {
                pressed_keys.retain(|k| *k != VirtualKeyCode::LWin && *k != VirtualKeyCode::RWin);
            }

            new_state.keyboard_state.pressed_virtual_keycodes =
                VirtualKeyCodeVec::from_vec(pressed_keys);
            callback_info.modify_window_state(new_state);
            // NOTE: Do NOT set needs_update = true here!
            // Same as KeyDown - modify_window_state handles event processing internally.

            send_ok(request, None, None);
        }

        DebugEvent::TextInput { text } => {
            log(
                LogLevel::Debug,
                LogCategory::EventLoop,
                format!("Received text input via debug server: '{}'", text),
                None,
            );

            // Get the focused node - text input only works on focused contenteditable
            let layout_window = callback_info.get_layout_window();
            let focused_node = layout_window.focus_manager.get_focused_node();

            if focused_node.is_some() {
                // Use the new create_text_input API which:
                // 1. Records the changeset in TextInputManager
                // 2. Triggers text input callbacks via recursive event processing
                // 3. Applies the changeset if not rejected via preventDefault
                // 4. Marks dirty nodes for re-render
                callback_info.create_text_input(text.clone().into());
                // NOTE: Do NOT set needs_update = true here!
                // create_text_input pushes a CallbackChange::CreateTextInput which
                // handles display list regeneration internally. Setting needs_update
                // would cause Update::RefreshDom → full DOM rebuild from C data model,
                // overwriting the internal text edit.
                send_ok(request, None, None);
            } else {
                send_err(
                    request,
                    "No focused node - text input requires focus on contenteditable",
                );
            }
        }

        // ─── Touch Events ────────────────────────────────────────────
        // Mutate FullWindowState.touch_state; the framework's
        // event-determination fires HoverEventFilter::TouchStart /
        // TouchMove / TouchEnd / TouchCancel via the normal state-diff
        // pipeline.
        DebugEvent::TouchStart { id, x, y, force } => {
            let mut state = callback_info.get_current_window_state().clone();
            let mut points = state
                .touch_state
                .touch_points
                .clone()
                .into_library_owned_vec();
            points.retain(|p| p.id != *id);
            points.push(azul_core::window::TouchPoint {
                id: *id,
                position: LogicalPosition { x: *x, y: *y },
                force: *force,
            });
            state.touch_state.touch_points = points.into();
            state.touch_state.num_touches = state.touch_state.touch_points.as_ref().len();
            callback_info.modify_window_state(state);
            // NO `needs_update` — see the note on `process_debug_event`.
            send_ok(request, None, None);
        }
        DebugEvent::TouchMove { id, x, y, force } => {
            let mut state = callback_info.get_current_window_state().clone();
            let mut points = state
                .touch_state
                .touch_points
                .clone()
                .into_library_owned_vec();
            if let Some(p) = points.iter_mut().find(|p| p.id == *id) {
                p.position = LogicalPosition { x: *x, y: *y };
                p.force = *force;
            } else {
                points.push(azul_core::window::TouchPoint {
                    id: *id,
                    position: LogicalPosition { x: *x, y: *y },
                    force: *force,
                });
            }
            state.touch_state.touch_points = points.into();
            state.touch_state.num_touches = state.touch_state.touch_points.as_ref().len();
            callback_info.modify_window_state(state);
            // NO `needs_update` — see the note on `process_debug_event`.
            send_ok(request, None, None);
        }
        DebugEvent::TouchEnd { id } => {
            let mut state = callback_info.get_current_window_state().clone();
            let mut points = state
                .touch_state
                .touch_points
                .clone()
                .into_library_owned_vec();
            points.retain(|p| p.id != *id);
            state.touch_state.touch_points = points.into();
            state.touch_state.num_touches = state.touch_state.touch_points.as_ref().len();
            callback_info.modify_window_state(state);
            // NO `needs_update` — see the note on `process_debug_event`.
            send_ok(request, None, None);
        }
        DebugEvent::TouchCancel => {
            // REFUSED BY NAME, not silently downgraded. `TouchState` carries no
            // cancel channel: a cancelled point and a lifted point are the
            // SAME state delta, so `determine_all_events` cannot tell them
            // apart and nothing in the engine has ever produced
            // `EventType::TouchCancel` on any platform. Clearing the points
            // here would fire `TouchEnd` for each of them and let a test named
            // after cancellation go green on end semantics — the false-green
            // this whole pass exists to remove. Use `touch_end` to lift a
            // point; re-enable this op when the platform layer grows a real
            // cancel signal to diff against.
            send_err(
                request,
                "touch_cancel: FullWindowState.touch_state has no cancel channel — a cancelled \
                 touch and a lifted touch are the same state delta, so no TouchCancel event can \
                 be determined (nothing in the engine produces EventType::TouchCancel on any \
                 platform). Use touch_end to lift the point."
                    .to_string(),
            );
        }

        // ─── Pen / Stylus Events ─────────────────────────────────────
        // Pen state is on GestureAndDragManager, so we go through
        // a NativeGesture-style injection — the pen accessors on
        // CallbackInfo (`get_pen_state` / `get_pen_pressure` /
        // `get_pen_tilt`) read it after the change applies.
        //
        // For Pen{Down,Move,Up} we also feed the mouse pipeline so
        // hover-only handlers still fire — pen and mouse are
        // intentionally unified in HoverEventFilter (PenDown is a
        // distinct variant, but `MouseDown` / `MouseUp` also fire).
        DebugEvent::PenDown {
            x,
            y,
            pressure,
            x_tilt,
            y_tilt,
        } => {
            let mut state = callback_info.get_current_window_state().clone();
            state.mouse_state.cursor_position =
                azul_core::window::CursorPosition::InWindow(LogicalPosition { x: *x, y: *y });
            state.mouse_state.left_down = true;
            callback_info.modify_window_state(state);
            // Pen-specific state goes through the gesture manager; the
            // PenState struct travels with the LongPress payload below
            // for the moment — full pen-injection is a follow-up tick.
            let _ = (pressure, x_tilt, y_tilt);
            // NO `needs_update` — see the note on `process_debug_event`.
            send_ok(request, None, None);
        }
        DebugEvent::PenMove {
            x,
            y,
            pressure,
            x_tilt,
            y_tilt,
        } => {
            let mut state = callback_info.get_current_window_state().clone();
            state.mouse_state.cursor_position =
                azul_core::window::CursorPosition::InWindow(LogicalPosition { x: *x, y: *y });
            callback_info.modify_window_state(state);
            let _ = (pressure, x_tilt, y_tilt);
            // NO `needs_update` — see the note on `process_debug_event`.
            send_ok(request, None, None);
        }
        DebugEvent::PenUp { x, y } => {
            let mut state = callback_info.get_current_window_state().clone();
            state.mouse_state.cursor_position =
                azul_core::window::CursorPosition::InWindow(LogicalPosition { x: *x, y: *y });
            state.mouse_state.left_down = false;
            callback_info.modify_window_state(state);
            // NO `needs_update` — see the note on `process_debug_event`.
            send_ok(request, None, None);
        }

        // ─── Native Gestures ─────────────────────────────────────────
        // Inject straight into the GestureAndDragManager override slot
        // via CallbackInfo::inject_native_gesture. The next user
        // callback that calls `get_swipe_direction` / `get_pinch` /
        // `get_rotation` / `get_long_press` / `was_double_clicked`
        // sees the injected gesture.
        DebugEvent::Swipe { direction } => {
            use azul_layout::managers::gesture::{GestureDirection, NativeGestureEvent};
            let dir = match direction {
                SwipeDir::Up => GestureDirection::Up,
                SwipeDir::Down => GestureDirection::Down,
                SwipeDir::Left => GestureDirection::Left,
                SwipeDir::Right => GestureDirection::Right,
            };
            callback_info.inject_native_gesture(NativeGestureEvent::Swipe(dir));
            // NO `needs_update` — see the note on `process_debug_event`.
            send_ok(request, None, None);
        }
        DebugEvent::Pinch {
            scale,
            center_x,
            center_y,
            initial_distance,
            current_distance,
            duration_ms,
        } => {
            use azul_layout::managers::gesture::{DetectedPinch, NativeGestureEvent};
            callback_info.inject_native_gesture(NativeGestureEvent::Pinch(DetectedPinch {
                scale: *scale,
                center: LogicalPosition {
                    x: *center_x,
                    y: *center_y,
                },
                initial_distance: *initial_distance,
                current_distance: *current_distance,
                duration_ms: *duration_ms,
            }));
            // NO `needs_update` — see the note on `process_debug_event`.
            send_ok(request, None, None);
        }
        DebugEvent::Rotate {
            angle_radians,
            center_x,
            center_y,
            duration_ms,
        } => {
            use azul_layout::managers::gesture::{DetectedRotation, NativeGestureEvent};
            callback_info.inject_native_gesture(NativeGestureEvent::Rotation(DetectedRotation {
                angle_radians: *angle_radians,
                center: LogicalPosition {
                    x: *center_x,
                    y: *center_y,
                },
                duration_ms: *duration_ms,
            }));
            // NO `needs_update` — see the note on `process_debug_event`.
            send_ok(request, None, None);
        }
        DebugEvent::LongPress { x, y, duration_ms } => {
            use azul_layout::managers::gesture::{DetectedLongPress, NativeGestureEvent};
            callback_info.inject_native_gesture(NativeGestureEvent::LongPress(DetectedLongPress {
                position: LogicalPosition { x: *x, y: *y },
                duration_ms: *duration_ms,
                callback_invoked: false,
                session_id: 0,
            }));
            // NO `needs_update` — see the note on `process_debug_event`.
            send_ok(request, None, None);
        }

        DebugEvent::GetFocusState => {
            let layout_window = callback_info.get_layout_window();
            let focus_manager = &layout_window.focus_manager;

            let response = if let Some(focused_node) = focus_manager.get_focused_node() {
                let dom_id = focused_node.dom;
                let internal_node_id = focused_node.node.into_crate_internal();

                let focused_info = internal_node_id.map(|node_id| {
                    // Get node info
                    let selector = build_selector_for_node(callback_info, dom_id, node_id);

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
                        .and_then(|nd| match nd.get_node_type() {
                            azul_core::dom::NodeType::Text(s) => Some(s.as_str().to_string()),
                            _ => None,
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
            let tem = &layout_window.text_edit_manager;

            let response = if let (Some(cursor), Some(mc)) =
                (tem.get_primary_cursor(), tem.multi_cursor.as_ref())
            {
                let position = cursor.cluster_id.start_byte_in_run as usize;
                let affinity = match cursor.affinity {
                    azul_core::selection::CursorAffinity::Leading => "leading".to_string(),
                    azul_core::selection::CursorAffinity::Trailing => "trailing".to_string(),
                };

                CursorStateResponse {
                    has_cursor: true,
                    cursor: Some(CursorInfo {
                        dom_id: mc.node_id.dom.inner as u32,
                        node_id: mc
                            .node_id
                            .node
                            .into_crate_internal()
                            .map(|n| n.index() as u64)
                            .unwrap_or(0),
                        position,
                        affinity,
                        is_visible: tem.blink.is_visible,
                        blink_timer_active: tem.blink.blink_timer_active,
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

        DebugEvent::RunE2eTests {
            ref tests,
            ref snapshots,
        } => {
            // One scheduler slot per window. A second run started while one is
            // suspended (or while a scenario STEP is itself `run_e2e_tests`)
            // would overwrite the first run's progress and leave its requester
            // blocked on a reply that never comes. Refuse instead of corrupting.
            if session.is_pending() || session.running {
                send_err(
                    request,
                    "run_e2e_tests: an E2E run is already in progress on this window",
                );
                return needs_update;
            }

            log(
                LogLevel::Info,
                LogCategory::DebugServer,
                format!("[E2E] RunE2eTests: executing {} test(s)", tests.len()),
                None,
            );

            // Delegate to resume_e2e_continuation with initial state.
            // This handles both straight-through execution and yield/resume
            // when a step (like resize) needs a relayout between steps.
            let cont = E2eContinuation {
                resume_not_before: None,
                setup_applied: false,
                response_tx: request.response_tx.clone(),
                window_id: request.window_id.clone(),
                tests: tests.clone(),
                snapshots: snapshots.clone(),
                test_idx: 0,
                step_idx: 0,
                completed_results: Vec::new(),
                current_step_results: Vec::new(),
                current_test_failed: false,
                test_start: wall_clock_now(),
                component_map: component_map.clone(),
                app_data: app_data.clone(),
                undo_manager: azul_layout::json::RefAnyUndoManager::new(0),
            };
            let result = resume_e2e_continuation(cont, callback_info, session);
            if result {
                needs_update = true;
            }
            // Don't send_ok — resume_e2e_continuation sends the response directly
        }

        // === DOM Mutation ===
        DebugEvent::InsertNode {
            parent_id,
            node_type,
            position,
            classes,
            id,
        } => {
            use azul_core::dom::DomId;
            use azul_core::id::NodeId;

            let dom_id = target_dom(request);
            let parent_node_id = NodeId::new(*parent_id as usize);

            // Validate parent exists
            let layout_window = callback_info.get_layout_window();
            let node_count = layout_window
                .layout_results
                .get(&dom_id)
                .map(|lr| lr.styled_dom.node_data.as_ref().len())
                .unwrap_or(0);

            // The StyledDom hierarchy is a FLAT DFS array: a node's first child
            // is DERIVED as `id + 1`, so a node's children are a contiguous run.
            // A node appended at the END of the array is therefore only a valid
            // child of a parent whose subtree already ENDS the array — i.e. of a
            // node on the tree's RIGHTMOST SPINE. Inserting anywhere else needs a
            // full DFS re-index plus a node-id remap of every manager
            // (`LayoutWindow::remap_node_ids`); until that exists, say so instead
            // of quietly appending the node to <html> (which is what this op used
            // to do — it validated `parent_id` and then ignored it, so the node
            // landed outside <body>, inherited nothing and painted nothing).
            let on_rightmost_spine = layout_window
                .layout_results
                .get(&dom_id)
                .map(|lr| {
                    let h = lr.styled_dom.node_hierarchy.as_container();
                    let mut cur = parent_node_id;
                    loop {
                        let Some(parent) = h[cur].parent_id() else {
                            return true; // reached the root
                        };
                        if h[parent].last_child_id() != Some(cur) {
                            return false;
                        }
                        cur = parent;
                    }
                })
                .unwrap_or(false);

            if parent_node_id.index() >= node_count {
                send_err(
                    request,
                    format!(
                        "Parent node {} not found (total nodes: {})",
                        parent_id, node_count
                    ),
                );
            } else if !on_rightmost_spine {
                send_err(
                    request,
                    format!(
                    "insert_node: node {parent_id} is not on the DOM's rightmost spine. The flat \
                     DFS hierarchy derives a node's first child as `id + 1`, so a node appended at \
                     the end of the array can only become the last child of a subtree that already \
                     ends the array. Inserting elsewhere requires a full re-index + \
                     remap_node_ids, which is not implemented — refusing rather than corrupting \
                     the tree."
                ),
                );
            } else {
                let new_node_id = node_count as u64; // New node will be appended at end
                let classes_az: Vec<azul_css::AzString> = classes
                    .iter()
                    .map(|c| azul_css::AzString::from(c.as_str()))
                    .collect();
                let id_az = id.as_ref().map(|i| azul_css::AzString::from(i.as_str()));

                callback_info.insert_child_node(
                    dom_id,
                    parent_node_id,
                    azul_css::AzString::from(node_type.as_str()),
                    (*position).into(),
                    classes_az.into(),
                    id_az.into(),
                );
                // NOTE: deliberately NO `needs_update = true` here — the pushed
                // CallbackChange already relayouts + rebuilds the display list.
                // needs_update would force Update::RefreshDom → a full DOM rebuild
                // from the app's layout_callback, throwing the mutation away.

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

            let dom_id = target_dom(request);
            let target_node_id = NodeId::new(*node_id as usize);

            let layout_window = callback_info.get_layout_window();
            let node_count = layout_window
                .layout_results
                .get(&dom_id)
                .map(|lr| lr.styled_dom.node_data.as_ref().len())
                .unwrap_or(0);

            if target_node_id.index() >= node_count || *node_id == 0 {
                send_err(
                    request,
                    format!("Cannot delete node {} (root or out of range)", node_id),
                );
            } else {
                callback_info.delete_node(dom_id, target_node_id);
                // NOTE: deliberately NO `needs_update = true` here — the pushed
                // CallbackChange already relayouts + rebuilds the display list.
                // needs_update would force Update::RefreshDom → a full DOM rebuild
                // from the app's layout_callback, throwing the mutation away.

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
            use azul_core::id::NodeId;
            use azul_core::styled_dom::NodeHierarchyItemId;

            let dom_id = target_dom(request);
            let target_node_id = NodeId::new(*node_id as usize);

            let layout_window = callback_info.get_layout_window();
            let node_count = layout_window
                .layout_results
                .get(&dom_id)
                .map(|lr| lr.styled_dom.node_data.as_ref().len())
                .unwrap_or(0);

            if target_node_id.index() >= node_count {
                send_err(request, format!("Node {} not found", node_id));
            } else {
                let dom_node_id = DomNodeId {
                    dom: dom_id,
                    node: NodeHierarchyItemId::from_crate_internal(Some(target_node_id)),
                };
                callback_info
                    .change_node_text(dom_node_id, azul_css::AzString::from(text.as_str()));
                // NOTE: deliberately NO `needs_update = true` here — the pushed
                // CallbackChange already relayouts + rebuilds the display list.
                // needs_update would force Update::RefreshDom → a full DOM rebuild
                // from the app's layout_callback, throwing the mutation away.

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

        DebugEvent::SetNodeClasses {
            node_id,
            classes,
            id,
        } => {
            use azul_core::dom::{DomId, IdOrClass};
            use azul_core::id::NodeId;

            let dom_id = target_dom(request);
            let target_node_id = NodeId::new(*node_id as usize);

            let layout_window = callback_info.get_layout_window();
            let node_count = layout_window
                .layout_results
                .get(&dom_id)
                .map(|lr| lr.styled_dom.node_data.as_ref().len())
                .unwrap_or(0);

            if target_node_id.index() >= node_count {
                send_err(request, format!("Node {} not found", node_id));
            } else {
                // "omit to keep current" — the variant's own doc comment. The
                // handler did the OPPOSITE: `set_node_ids_and_classes` REPLACES
                // the node's entire id+class vec, so omitting `id` DELETED the
                // id. Every "set the classes the node already has" no-op test on
                // a fixture whose target carries an `id=` was therefore a real
                // mutation wearing a no-op costume — and `#box` stopped matching
                // right after it.
                let current_id = if id.is_none() {
                    callback_info
                        .get_layout_window()
                        .layout_results
                        .get(&dom_id)
                        .and_then(|lr| {
                            lr.styled_dom
                                .node_data
                                .as_container()
                                .get(target_node_id)
                                .map(azul_core::dom::NodeData::get_ids_and_classes)
                        })
                        .and_then(|ids| {
                            ids.as_ref().iter().find_map(|entry| match entry {
                                IdOrClass::Id(s) => Some(s.clone()),
                                IdOrClass::Class(_) => None,
                            })
                        })
                } else {
                    None
                };

                let mut ids_and_classes = Vec::new();
                if let Some(id_str) = id {
                    ids_and_classes.push(IdOrClass::Id(azul_css::AzString::from(id_str.as_str())));
                } else if let Some(existing) = current_id {
                    ids_and_classes.push(IdOrClass::Id(existing));
                }
                for class in classes.iter() {
                    ids_and_classes
                        .push(IdOrClass::Class(azul_css::AzString::from(class.as_str())));
                }

                callback_info.set_node_ids_and_classes(
                    dom_id,
                    target_node_id,
                    ids_and_classes.into(),
                );
                // NOTE: deliberately NO `needs_update = true` here — the pushed
                // CallbackChange already relayouts + rebuilds the display list.
                // needs_update would force Update::RefreshDom → a full DOM rebuild
                // from the app's layout_callback, throwing the mutation away.

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

        DebugEvent::SetNodeCssOverride {
            node_id,
            property,
            value,
        } => {
            use azul_core::dom::DomId;
            use azul_core::id::NodeId;
            use azul_css::props::property::{get_css_key_map, CssPropertyType};

            let dom_id = target_dom(request);
            let target_node_id = NodeId::new(*node_id as usize);

            let layout_window = callback_info.get_layout_window();
            let node_count = layout_window
                .layout_results
                .get(&dom_id)
                .map(|lr| lr.styled_dom.node_data.as_ref().len())
                .unwrap_or(0);

            if target_node_id.index() >= node_count {
                send_err(request, format!("Node {} not found", node_id));
            } else {
                use azul_css::props::property::{
                    parse_combined_css_property, CombinedCssPropertyType,
                };
                let key_map = get_css_key_map();
                // A CSS name is either a plain property ("width", "background")
                // or a COMBINED/shorthand one ("background-color", "margin",
                // "border", "font", …), which lives in a different enum and
                // expands to several plain properties. Only the first was
                // handled, so every shorthand was rejected with "Unknown CSS
                // property" — `background-color` among them.
                let parsed: Result<Vec<azul_css::props::property::CssProperty>, String> =
                    if let Some(prop_type) = CssPropertyType::from_str(property, &key_map) {
                        azul_css::props::property::parse_css_property(prop_type, value)
                            .map(|p| vec![p])
                            .map_err(|e| {
                                format!(
                                    "Failed to parse CSS value '{value}' for property \
                                     '{property}': {e:?}"
                                )
                            })
                    } else if let Some(combined) =
                        CombinedCssPropertyType::from_str(property, &key_map)
                    {
                        parse_combined_css_property(combined, value).map_err(|e| {
                            format!(
                                "Failed to parse CSS value '{value}' for shorthand property \
                                 '{property}': {e:?}"
                            )
                        })
                    } else {
                        Err(format!("Unknown CSS property: '{property}'"))
                    };

                match parsed {
                    Ok(css_props) => {
                        callback_info.change_node_css_properties(
                            dom_id,
                            target_node_id,
                            css_props.into(),
                        );
                        // NOTE: deliberately NO `needs_update = true` here.
                        // change_node_css_properties pushes a
                        // CallbackChange::ChangeNodeCssProperties, which
                        // restyles + rebuilds the display list and reports
                        // ShouldIncrementalRelayout. needs_update would instead
                        // force Update::RefreshDom → a full DOM rebuild from the
                        // app's layout_callback, throwing the mutation away.
                        send_ok(
                            request,
                            None,
                            Some(ResponseData::NodeCssOverrideSet(
                                NodeCssOverrideSetResponse {
                                    node_id: *node_id,
                                    property: property.clone(),
                                    value: value.clone(),
                                },
                            )),
                        );
                    }
                    Err(e) => send_err(request, e),
                }
            }
        }

        DebugEvent::SetNodeImage {
            node_id,
            width,
            height,
            color,
        } => {
            use azul_core::dom::NodeType;
            use azul_core::id::NodeId;

            let dom_id = target_dom(request);
            let target_node_id = NodeId::new(*node_id as usize);

            // Validate loudly (add_timer discipline): the target must exist
            // AND be an image node — a silent no-op here would let a scenario
            // "pass" without exercising anything.
            let layout_window = callback_info.get_layout_window();
            let is_image_node = layout_window.layout_results.get(&dom_id).and_then(|lr| {
                lr.styled_dom
                    .node_data
                    .as_container()
                    .get(target_node_id)
                    .map(|n| matches!(n.get_node_type(), NodeType::Image(_)))
            });

            match is_image_node {
                None => send_err(request, format!("Node {node_id} not found")),
                Some(false) => send_err(
                    request,
                    format!("Node {node_id} is not a NodeType::Image node"),
                ),
                Some(true) => match synthesize_solid_image(*width, *height, color) {
                    Ok(image) => {
                        callback_info.change_node_image(
                            dom_id,
                            target_node_id,
                            image,
                            azul_core::resources::UpdateImageType::Content,
                        );
                        // NO needs_update: ChangeNodeImage goes through the
                        // content chokepoint, whose tier drives the repaint —
                        // RefreshDom would rebuild the DOM and hide the very
                        // paint-only path this op exists to exercise.
                        send_ok(request, None, None);
                    }
                    Err(e) => send_err(request, e),
                },
            }
        }

        DebugEvent::AddImageToCache {
            css_id,
            width,
            height,
            color,
        } => match synthesize_solid_image(*width, *height, color) {
            Ok(image) => {
                callback_info.add_image_to_cache(css_id.clone().into(), image);
                send_ok(request, None, None);
            }
            Err(e) => send_err(request, e),
        },

        DebugEvent::RemoveImageFromCache { css_id } => {
            callback_info.remove_image_from_cache(css_id.clone().into());
            send_ok(request, None, None);
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
            let map_guard = component_map.lock().unwrap_or_else(|e| e.into_inner());
            let registry = build_component_registry(&map_guard);
            drop(map_guard);
            send_ok(
                request,
                None,
                Some(ResponseData::ComponentRegistry(registry)),
            );
        }

        DebugEvent::GetLibraries => {
            let map_guard = component_map.lock().unwrap_or_else(|e| e.into_inner());
            let registry = build_component_registry(&map_guard);
            drop(map_guard);
            let libraries = registry
                .libraries
                .iter()
                .map(|lib| LibrarySummary {
                    name: lib.name.clone(),
                    version: lib.version.clone(),
                    description: lib.description.clone(),
                    exportable: lib.exportable,
                    modifiable: lib.modifiable,
                    component_count: lib.components.len(),
                })
                .collect();
            send_ok(
                request,
                None,
                Some(ResponseData::Libraries(LibraryListResponse { libraries })),
            );
        }

        DebugEvent::GetLibraryComponents { library } => {
            let map_guard = component_map.lock().unwrap_or_else(|e| e.into_inner());
            let registry = build_component_registry(&map_guard);
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
                let available: Vec<_> =
                    registry.libraries.iter().map(|l| l.name.as_str()).collect();
                send_err(
                    request,
                    format!(
                        "Library '{}' not found. Available: {:?}",
                        library, available
                    ),
                );
            }
        }

        DebugEvent::ExportCode { language } => {
            // Primary: the live page compiled to a runnable app. Best-effort:
            // also fold in any exportable component-library sources.
            match build_live_page_code(language, callback_info) {
                Ok((fname, src)) => {
                    let mut files = std::collections::HashMap::new();
                    files.insert(fname, src);
                    let map_guard = component_map.lock().unwrap_or_else(|e| e.into_inner());
                    if let Ok(comp) = build_exported_code(language, &map_guard) {
                        for (k, v) in comp.files {
                            files.entry(k).or_insert(v);
                        }
                    }
                    drop(map_guard);
                    send_ok(
                        request,
                        None,
                        Some(ResponseData::ExportedCode(ExportedCodeResponse {
                            language: language.clone(),
                            files,
                            warnings: Vec::new(),
                        })),
                    );
                }
                Err(e) => {
                    send_err(request, format!("Export failed: {}", e));
                }
            }
        }

        DebugEvent::ExportCodeZip {
            language,
            library: _lib_filter,
        } => {
            // G1/G3: Package exported code into a downloadable ZIP
            let map_guard = component_map.lock().unwrap_or_else(|e| e.into_inner());
            let result = build_exported_code(language, &map_guard);

            // Also collect component CSS
            let mut css_files = Vec::new();
            for lib in map_guard.libraries.iter() {
                if lib.exportable {
                    for comp in lib.components.iter() {
                        if !comp.css.as_str().is_empty() {
                            let css_path = format!("css/{}.css", comp.id.name.as_str());
                            css_files.push((css_path, comp.css.as_str().as_bytes().to_vec()));
                        }
                    }
                }
            }
            drop(map_guard);

            match result {
                Ok(response) => {
                    // Build ZIP entries from exported files (scaffold already includes build config)
                    let mut zip_entries: Vec<(String, Vec<u8>)> = Vec::new();
                    let mut seen_paths = std::collections::HashSet::new();

                    // The live page app is the primary artifact.
                    if let Ok((fname, src)) = build_live_page_code(language, callback_info) {
                        if seen_paths.insert(fname.clone()) {
                            zip_entries.push((fname, src.into_bytes()));
                        }
                    }

                    // Add generated source files (from generate_scaffold — includes Cargo.toml etc.)
                    for (path, content) in &response.files {
                        if seen_paths.insert(path.clone()) {
                            zip_entries.push((path.clone(), content.as_bytes().to_vec()));
                        }
                    }

                    // Add component CSS files (skip duplicates)
                    for (path, data) in css_files {
                        if seen_paths.insert(path.clone()) {
                            zip_entries.push((path, data));
                        }
                    }

                    // Add warnings as README
                    if !response.warnings.is_empty() {
                        let warnings_text = response.warnings.join("\n");
                        let path = "WARNINGS.txt".to_string();
                        if seen_paths.insert(path.clone()) {
                            zip_entries.push((path, warnings_text.into_bytes()));
                        }
                    }

                    // Create ZIP
                    let config = azul_layout::zip::ZipWriteConfig::default();
                    match azul_layout::zip::zip_create_from_files(zip_entries, &config) {
                        Ok(zip_bytes) => {
                            let base64_str = azul_layout::callbacks::base64_encode(&zip_bytes);
                            let data_uri = format!("data:application/zip;base64,{}", base64_str);
                            send_ok(
                                request,
                                None,
                                Some(ResponseData::Json(serde_json::json!({
                                    "download_url": data_uri,
                                    "filename": format!("azul-export-{}.zip", language),
                                    "size_bytes": zip_bytes.len(),
                                    "file_count": response.files.len(),
                                }))),
                            );
                        }
                        Err(e) => {
                            send_err(request, format!("ZIP creation failed: {:?}", e));
                        }
                    }
                }
                Err(e) => {
                    send_err(request, format!("Export failed: {}", e));
                }
            }
        }

        DebugEvent::ImportComponentLibrary { library: lib_json } => {
            use azul_core::xml::{
                ComponentDataFieldVec, ComponentDataModel, ComponentDef, ComponentDefVec,
                ComponentId, ComponentLibrary, ComponentLibraryVec, ComponentSource,
            };
            use azul_css::corety::AzString;

            let lib_name = lib_json.name.clone();
            let component_count = lib_json.components.len();

            // Convert ExportedLibraryResponse -> ComponentLibrary with validation
            let mut defs = Vec::new();
            let mut validation_errors = Vec::new();

            for c in &lib_json.components {
                // Validate and convert all fields
                let validated_fields = match validate_exported_fields(&c.fields) {
                    Ok(fields) => fields,
                    Err(e) => {
                        validation_errors.push(format!("Component '{}': {}", c.name, e));
                        continue;
                    }
                };

                let display_name_str = if c.display_name.is_empty() {
                    &c.name
                } else {
                    &c.display_name
                };
                defs.push(ComponentDef {
                    id: ComponentId::new(&lib_name, &c.name),
                    display_name: AzString::from(display_name_str.as_str()),
                    description: AzString::from(c.description.as_str()),
                    css: AzString::from(c.css.as_str()),
                    source: ComponentSource::UserDefined,
                    data_model: ComponentDataModel {
                        name: AzString::from(format!("{}Data", display_name_str).as_str()),
                        description: AzString::from(c.description.as_str()),
                        fields: ComponentDataFieldVec::from_vec(validated_fields),
                    },
                    render_fn: azul_core::xml::user_defined_render_fn,
                    compile_fn: azul_core::xml::user_defined_compile_fn,
                    render_fn_source: None.into(),
                    compile_fn_source: None.into(),
                });
            }

            if !validation_errors.is_empty() {
                send_err(
                    request,
                    format!(
                        "Validation errors in library '{}': {}",
                        lib_name,
                        validation_errors.join("; ")
                    ),
                );
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
                let mut map_guard = component_map.lock().unwrap_or_else(|e| e.into_inner());
                let empty_libs = ComponentLibraryVec::from_const_slice(&[]);
                let mut libs = core::mem::replace(&mut map_guard.libraries, empty_libs)
                    .into_library_owned_vec();
                let was_update =
                    if let Some(existing) = libs.iter_mut().find(|l| l.name.as_str() == lib_name) {
                        *existing = new_lib;
                        true
                    } else {
                        libs.push(new_lib);
                        false
                    };
                map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                drop(map_guard);

                send_ok(
                    request,
                    None,
                    Some(ResponseData::ImportedLibrary(ImportedLibraryResponse {
                        library_name: lib_name,
                        component_count,
                        was_update,
                    })),
                );
                needs_update = true;
            }
        }

        DebugEvent::ExportComponentLibrary {
            library: lib_name_opt,
        } => {
            let map_guard = component_map.lock().unwrap_or_else(|e| e.into_inner());
            let registry = build_component_registry(&map_guard);
            drop(map_guard);

            let exportable_libs: Vec<&ComponentLibraryInfo> = registry
                .libraries
                .iter()
                .filter(|lib| lib.exportable)
                .filter(|lib| lib_name_opt.as_ref().is_none_or(|n| &lib.name == n))
                .collect();

            if exportable_libs.is_empty() {
                if let Some(ref name) = lib_name_opt {
                    send_err(request, format!(
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
                    data_models: Vec::new(),
                    enum_models: Vec::new(),
                    components: lib
                        .components
                        .iter()
                        .map(|c| {
                            // Build unified fields from data_model + callback_slots
                            let mut fields: Vec<ExportedDataField> = c
                                .data_model
                                .iter()
                                .map(|f| ExportedDataField {
                                    name: f.name.clone(),
                                    field_type: f.field_type.clone(),
                                    default: f.default.clone(),
                                    description: f.description.clone(),
                                })
                                .collect();
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
                                css: c.css.clone(),
                            }
                        })
                        .collect(),
                };
                send_ok(request, None, Some(ResponseData::ExportedLibrary(exported)));
            }
        }

        DebugEvent::CreateLibrary { name, description } => {
            use azul_core::xml::{
                ComponentDataModelVec, ComponentDefVec, ComponentLibrary, ComponentLibraryVec,
            };
            use azul_css::corety::AzString;

            let mut map_guard = component_map.lock().unwrap_or_else(|e| e.into_inner());
            // Check if library already exists
            if map_guard
                .libraries
                .iter()
                .any(|l| l.name.as_str() == name.as_str())
            {
                drop(map_guard);
                send_err(request, format!("Library '{}' already exists", name));
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
                let mut libs = core::mem::replace(&mut map_guard.libraries, empty_libs)
                    .into_library_owned_vec();
                libs.push(new_lib);
                map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                drop(map_guard);
                send_ok(request, None, None);
            }
        }

        DebugEvent::DeleteLibrary { name } => {
            use azul_core::xml::ComponentLibraryVec;

            let mut map_guard = component_map.lock().unwrap_or_else(|e| e.into_inner());
            let empty_libs = ComponentLibraryVec::from_const_slice(&[]);
            let mut libs =
                core::mem::replace(&mut map_guard.libraries, empty_libs).into_library_owned_vec();
            let original_len = libs.len();

            // Only allow deletion of modifiable libraries
            if let Some(lib) = libs.iter().find(|l| l.name.as_str() == name.as_str()) {
                if !lib.modifiable {
                    map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                    drop(map_guard);
                    send_err(
                        request,
                        format!("Library '{}' is not modifiable and cannot be deleted", name),
                    );
                } else {
                    libs.retain(|l| l.name.as_str() != name.as_str());
                    map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                    drop(map_guard);
                    send_ok(request, None, None);
                }
            } else {
                map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                drop(map_guard);
                send_err(request, format!("Library '{}' not found", name));
            }
        }

        DebugEvent::CreateComponent {
            library,
            name,
            display_name,
        } => {
            use azul_core::xml::{
                ComponentDataFieldVec, ComponentDataModel, ComponentDef, ComponentId,
                ComponentLibraryVec, ComponentSource,
            };
            use azul_css::corety::AzString;

            let mut map_guard = component_map.lock().unwrap_or_else(|e| e.into_inner());
            let empty_libs = ComponentLibraryVec::from_const_slice(&[]);
            let mut libs =
                core::mem::replace(&mut map_guard.libraries, empty_libs).into_library_owned_vec();

            if let Some(lib) = libs
                .iter_mut()
                .find(|l| l.name.as_str() == library.as_str())
            {
                if !lib.modifiable {
                    map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                    drop(map_guard);
                    send_err(request, format!("Library '{}' is not modifiable", library));
                } else {
                    let display = display_name.as_deref().unwrap_or(name.as_str());
                    let new_def = ComponentDef {
                        id: ComponentId::new(library.as_str(), name.as_str()),
                        display_name: AzString::from(display),
                        description: AzString::from_const_str(""),
                        css: AzString::from_const_str(""),
                        source: ComponentSource::UserDefined,
                        data_model: ComponentDataModel {
                            name: AzString::from(format!("{}Data", display).as_str()),
                            description: AzString::from_const_str(""),
                            fields: ComponentDataFieldVec::from_const_slice(&[]),
                        },
                        render_fn: azul_core::xml::user_defined_render_fn,
                        compile_fn: azul_core::xml::user_defined_compile_fn,
                        render_fn_source: None.into(),
                        compile_fn_source: None.into(),
                    };
                    let mut comps = core::mem::replace(&mut lib.components, Vec::new().into())
                        .into_library_owned_vec();
                    comps.push(new_def);
                    lib.components = azul_core::xml::ComponentDefVec::from_vec(comps);
                    map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                    drop(map_guard);
                    send_ok(request, None, None);
                }
            } else {
                map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                drop(map_guard);
                send_err(request, format!("Library '{}' not found", library));
            }
        }

        DebugEvent::DeleteComponent { library, name } => {
            use azul_core::xml::ComponentLibraryVec;

            let mut map_guard = component_map.lock().unwrap_or_else(|e| e.into_inner());
            let empty_libs = ComponentLibraryVec::from_const_slice(&[]);
            let mut libs =
                core::mem::replace(&mut map_guard.libraries, empty_libs).into_library_owned_vec();

            if let Some(lib) = libs
                .iter_mut()
                .find(|l| l.name.as_str() == library.as_str())
            {
                if !lib.modifiable {
                    map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                    drop(map_guard);
                    send_err(request, format!("Library '{}' is not modifiable", library));
                } else {
                    let mut comps = core::mem::replace(&mut lib.components, Vec::new().into())
                        .into_library_owned_vec();
                    comps.retain(|c| c.id.name.as_str() != name.as_str());
                    lib.components = azul_core::xml::ComponentDefVec::from_vec(comps);
                    map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                    drop(map_guard);
                    send_ok(request, None, None);
                }
            } else {
                map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                drop(map_guard);
                send_err(request, format!("Library '{}' not found", library));
            }
        }

        DebugEvent::UpdateComponent {
            library,
            name,
            css,
            description,
            display_name,
            fields,
        } => {
            use azul_core::xml::ComponentLibraryVec;
            use azul_css::corety::AzString;

            let mut map_guard = component_map.lock().unwrap_or_else(|e| e.into_inner());
            let empty_libs = ComponentLibraryVec::from_const_slice(&[]);
            let mut libs =
                core::mem::replace(&mut map_guard.libraries, empty_libs).into_library_owned_vec();

            if let Some(lib) = libs
                .iter_mut()
                .find(|l| l.name.as_str() == library.as_str())
            {
                if !lib.modifiable {
                    map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                    drop(map_guard);
                    send_err(request, format!("Library '{}' is not modifiable", library));
                } else {
                    let mut comps = core::mem::replace(&mut lib.components, Vec::new().into())
                        .into_library_owned_vec();
                    if let Some(comp) = comps
                        .iter_mut()
                        .find(|c| c.id.name.as_str() == name.as_str())
                    {
                        if let Some(new_css) = css {
                            comp.css = AzString::from(new_css.as_str());
                        }
                        if let Some(desc) = description {
                            comp.description = AzString::from(desc.as_str());
                        }
                        if let Some(dn) = display_name {
                            comp.display_name = AzString::from(dn.as_str());
                        }
                        // Replace data_model.fields with validated new fields (if provided)
                        if let Some(new_fields) = fields {
                            match validate_exported_fields(new_fields) {
                                Ok(validated) => {
                                    comp.data_model.fields = validated.into();
                                }
                                Err(e) => {
                                    lib.components =
                                        azul_core::xml::ComponentDefVec::from_vec(comps);
                                    map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                                    drop(map_guard);
                                    send_err(
                                        request,
                                        format!("Validation error in component '{}': {}", name, e),
                                    );
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
                        send_err(
                            request,
                            format!("Component '{}' not found in library '{}'", name, library),
                        );
                    }
                }
            } else {
                map_guard.libraries = ComponentLibraryVec::from_vec(libs);
                drop(map_guard);
                send_err(request, format!("Library '{}' not found", library));
            }
        }

        DebugEvent::GetComponentPreview {
            library,
            name,
            width,
            height,
            dpi,
            background,
            css_override,
            args,
            override_os,
            override_theme,
            override_lang,
        } => {
            // --- 1. Look up the component ---
            let map_guard = component_map.lock().unwrap_or_else(|e| e.into_inner());
            let comp_found = map_guard
                .libraries
                .iter()
                .find_map(|lib| {
                    if lib.name.as_str() == library.as_str() {
                        lib.components
                            .iter()
                            .find(|c| c.id.name.as_str() == name.as_str())
                    } else {
                        None
                    }
                })
                .cloned();
            drop(map_guard);

            let comp = match comp_found {
                Some(c) => c,
                None => {
                    send_err(
                        request,
                        format!("Component '{}' not found in library '{}'", name, library),
                    );
                    return needs_update;
                }
            };

            // --- 2. Override data_model defaults with provided args ---
            let render_data_model =
                match override_data_model_defaults(&comp.data_model, args.as_ref()) {
                    Ok(v) => v,
                    Err(e) => {
                        send_err(request, format!("Invalid args for '{}': {}", name, e));
                        return needs_update;
                    }
                };

            // --- 3. Render the component to a StyledDom ---
            let map_guard = component_map.lock().unwrap_or_else(|e| e.into_inner());
            let styled_dom = match (comp.render_fn)(&comp, &render_data_model, &map_guard) {
                azul_core::xml::ResultStyledDomRenderDomError::Ok(sd) => sd,
                azul_core::xml::ResultStyledDomRenderDomError::Err(e) => {
                    send_err(request, format!("render_fn failed for '{}': {:?}", name, e));
                    return needs_update;
                }
            };
            drop(map_guard);

            // --- 4. Apply CSS (component css or overridden) ---
            let css_text = css_override.as_deref().unwrap_or_else(|| comp.css.as_str());
            let mut styled_dom = styled_dom;
            if !css_text.is_empty() {
                let css =
                    azul_css::css::Css::from_string(azul_css::corety::AzString::from(css_text));
                styled_dom.restyle(css);
            }

            // --- 5. Parse background color ---
            let bg_color = background
                .as_deref()
                .and_then(|bg_str| {
                    let hex = bg_str.strip_prefix('#').unwrap_or(bg_str);
                    if hex.len() >= 6 {
                        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                        let a = if hex.len() >= 8 {
                            u8::from_str_radix(&hex[6..8], 16).unwrap_or(255)
                        } else {
                            255
                        };
                        Some(azul_css::props::basic::color::ColorU { r, g, b, a })
                    } else {
                        None
                    }
                })
                .unwrap_or(azul_css::props::basic::color::ColorU {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255,
                });

            // --- 6. Build render options ---
            let opts = azul_layout::cpurender::ComponentPreviewOptions {
                width: *width,
                height: *height,
                dpi_factor: dpi.unwrap_or(1.0),
                background_color: bg_color,
            };

            // --- 7. Get the font manager and system style from the running window ---
            let layout_window = callback_info.get_layout_window();
            let font_manager = &layout_window.font_manager;
            let system_style = callback_info.get_system_style();

            // --- 8. Render to PNG ---
            match azul_layout::cpurender::render_component_preview(
                &styled_dom,
                font_manager,
                opts,
                Some(system_style),
            ) {
                Ok(result) => {
                    // Base64-encode the PNG data
                    let base64_str = azul_layout::callbacks::base64_encode(&result.png_data);
                    let data_uri = format!("data:image/png;base64,{}", base64_str);
                    send_ok(
                        request,
                        None,
                        Some(ResponseData::ComponentPreview(ComponentPreviewResponse {
                            data: data_uri,
                            width: result.content_width,
                            height: result.content_height,
                        })),
                    );
                }
                Err(e) => {
                    send_err(request, format!("Preview render failed: {}", e));
                }
            }
        }

        DebugEvent::GetComponentRenderTree { library, name } => {
            // Look up the component
            let map_guard = component_map.lock().unwrap_or_else(|e| e.into_inner());
            let comp_found = map_guard
                .libraries
                .iter()
                .find_map(|lib| {
                    if lib.name.as_str() == library.as_str() {
                        lib.components
                            .iter()
                            .find(|c| c.id.name.as_str() == name.as_str())
                    } else {
                        None
                    }
                })
                .cloned();

            if let Some(comp) = comp_found {
                // Build default data model
                let render_data_model = match override_data_model_defaults(&comp.data_model, None) {
                    Ok(v) => v,
                    Err(e) => {
                        drop(map_guard);
                        send_err(
                            request,
                            format!("Failed to build data model for '{}': {}", name, e),
                        );
                        return needs_update;
                    }
                };

                // Render the component to StyledDom
                let styled_dom = match (comp.render_fn)(&comp, &render_data_model, &map_guard) {
                    azul_core::xml::ResultStyledDomRenderDomError::Ok(sd) => sd,
                    azul_core::xml::ResultStyledDomRenderDomError::Err(e) => {
                        drop(map_guard);
                        send_err(request, format!("render_fn failed for '{}': {:?}", name, e));
                        return needs_update;
                    }
                };
                drop(map_guard);

                // Convert StyledDom to a JSON-serializable tree
                let tree_json = styled_dom_to_render_tree(&styled_dom);
                send_ok(request, None, Some(ResponseData::Json(tree_json)));
            } else {
                drop(map_guard);
                send_err(
                    request,
                    format!("Component '{}' not found in library '{}'", name, library),
                );
            }
        }

        DebugEvent::GetComponentSource {
            library,
            name,
            source_type,
            language,
        } => {
            // E4: Return the source code of render_fn or compile_fn
            let map_guard = component_map.lock().unwrap_or_else(|e| e.into_inner());
            let comp_found = map_guard
                .libraries
                .iter()
                .find_map(|lib| {
                    if lib.name.as_str() == library.as_str() {
                        lib.components
                            .iter()
                            .find(|c| c.id.name.as_str() == name.as_str())
                    } else {
                        None
                    }
                })
                .cloned();
            drop(map_guard);

            if let Some(comp) = comp_found {
                let source_code = match source_type.as_str() {
                    "render_fn" => {
                        // For user-defined components, the source is stored; for builtins, return a description
                        comp.render_fn_source
                            .as_ref()
                            .map(|s| s.as_str().to_string())
                            .unwrap_or_else(|| {
                                format!("// Built-in render function for '{}'", name)
                            })
                    }
                    "compile_fn" => {
                        // Generate the compile_fn output for the requested language
                        let lang = language.as_deref().unwrap_or("rust");
                        let target = match lang {
                            "c" => azul_core::xml::CompileTarget::C,
                            "cpp" | "c++" => azul_core::xml::CompileTarget::Cpp,
                            "python" => azul_core::xml::CompileTarget::Python,
                            _ => azul_core::xml::CompileTarget::Rust,
                        };
                        let map_guard = component_map.lock().unwrap_or_else(|e| e.into_inner());
                        let result = (comp.compile_fn)(&comp, &target, &comp.data_model, 0);
                        drop(map_guard);
                        match result {
                            azul_core::xml::ResultStringCompileError::Ok(s) => {
                                s.as_str().to_string()
                            }
                            azul_core::xml::ResultStringCompileError::Err(e) => {
                                format!("// Compile error: {:?}", e)
                            }
                        }
                    }
                    _ => format!("// Unknown source_type: {}", source_type),
                };
                send_ok(
                    request,
                    None,
                    Some(ResponseData::Json(serde_json::json!({
                        "source": source_code
                    }))),
                );
            } else {
                send_err(
                    request,
                    format!("Component '{}' not found in library '{}'", name, library),
                );
            }
        }

        DebugEvent::UpdateComponentRenderFn {
            library,
            name,
            source,
        } => {
            // E4: Store the render_fn source code (hot-replacement not yet supported)
            let mut map_guard = component_map.lock().unwrap_or_else(|e| e.into_inner());
            let empty_libs = azul_core::xml::ComponentLibraryVec::from_const_slice(&[]);
            let mut libs =
                core::mem::replace(&mut map_guard.libraries, empty_libs).into_library_owned_vec();

            if let Some(lib) = libs
                .iter_mut()
                .find(|l| l.name.as_str() == library.as_str())
            {
                if !lib.modifiable {
                    map_guard.libraries = azul_core::xml::ComponentLibraryVec::from_vec(libs);
                    drop(map_guard);
                    send_err(request, format!("Library '{}' is not modifiable", library));
                } else {
                    let mut comps = core::mem::replace(&mut lib.components, Vec::new().into())
                        .into_library_owned_vec();
                    if let Some(comp) = comps
                        .iter_mut()
                        .find(|c| c.id.name.as_str() == name.as_str())
                    {
                        comp.render_fn_source =
                            Some(azul_css::corety::AzString::from(source.as_str())).into();
                        lib.components = comps.into();
                        map_guard.libraries = azul_core::xml::ComponentLibraryVec::from_vec(libs);
                        drop(map_guard);
                        send_ok(request, None, None);
                        needs_update = true;
                    } else {
                        lib.components = comps.into();
                        map_guard.libraries = azul_core::xml::ComponentLibraryVec::from_vec(libs);
                        drop(map_guard);
                        send_err(request, format!("Component '{}' not found", name));
                    }
                }
            } else {
                map_guard.libraries = azul_core::xml::ComponentLibraryVec::from_vec(libs);
                drop(map_guard);
                send_err(request, format!("Library '{}' not found", library));
            }
        }

        DebugEvent::UpdateComponentCompileFn {
            library,
            name,
            source,
            language,
        } => {
            // E4: Store compile_fn source for a specific language
            let mut map_guard = component_map.lock().unwrap_or_else(|e| e.into_inner());
            let empty_libs = azul_core::xml::ComponentLibraryVec::from_const_slice(&[]);
            let mut libs =
                core::mem::replace(&mut map_guard.libraries, empty_libs).into_library_owned_vec();

            if let Some(lib) = libs
                .iter_mut()
                .find(|l| l.name.as_str() == library.as_str())
            {
                if !lib.modifiable {
                    map_guard.libraries = azul_core::xml::ComponentLibraryVec::from_vec(libs);
                    drop(map_guard);
                    send_err(request, format!("Library '{}' is not modifiable", library));
                } else {
                    let mut comps = core::mem::replace(&mut lib.components, Vec::new().into())
                        .into_library_owned_vec();
                    if let Some(comp) = comps
                        .iter_mut()
                        .find(|c| c.id.name.as_str() == name.as_str())
                    {
                        comp.compile_fn_source =
                            Some(azul_css::corety::AzString::from(source.as_str())).into();
                        lib.components = comps.into();
                        map_guard.libraries = azul_core::xml::ComponentLibraryVec::from_vec(libs);
                        drop(map_guard);
                        send_ok(request, None, None);
                    } else {
                        lib.components = comps.into();
                        map_guard.libraries = azul_core::xml::ComponentLibraryVec::from_vec(libs);
                        drop(map_guard);
                        send_err(request, format!("Component '{}' not found", name));
                    }
                }
            } else {
                map_guard.libraries = azul_core::xml::ComponentLibraryVec::from_vec(libs);
                drop(map_guard);
                send_err(request, format!("Library '{}' not found", library));
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
                        std::process::Command::new("open")
                            .arg(file.as_str())
                            .spawn()
                    }
                }
                #[cfg(target_os = "linux")]
                {
                    std::process::Command::new("xdg-open")
                        .arg(file.as_str())
                        .spawn()
                }
                #[cfg(target_os = "windows")]
                {
                    std::process::Command::new("cmd")
                        .args(&["/C", "start", "", file.as_str()])
                        .spawn()
                }
                // Mobile targets have no "open in default editor" — debug
                // server is desktop-only anyway. Return a synthesized Err so
                // the match arms below still type-check.
                #[cfg(any(target_os = "android", target_os = "ios"))]
                {
                    Err::<std::process::Child, std::io::Error>(std::io::Error::new(
                        std::io::ErrorKind::Unsupported,
                        "OpenFile not supported on mobile",
                    ))
                }
            };
            match result {
                Ok(_) => send_ok(request, None, None),
                Err(e) => send_err(request, format!("Failed to open {}: {}", file, e)),
            }
        }

        // UNREACHABLE TODAY — and that is the point. Every `DebugEvent` variant
        // now has a real match arm (the last five zombies — Focus, Blur, Move,
        // DpiChanged, GetDom — were implemented). The arm is kept, with the lint
        // silenced, as the SAFETY NET for the next variant somebody adds: a new
        // variant lands here, answers `ok` without doing anything, and
        // `azul-doc gen-e2e`'s zombie scan (gene2e.rs::parse_schema, which keys
        // off exactly this catch-all + the "Unhandled:" marker) refuses to
        // generate any test using it. Delete this arm and that net goes away.
        #[allow(unreachable_patterns)]
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
#[cfg(feature = "e2e-server")]
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
        session: E2eSession::new(),
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

/// Data stored in the debug timer's `RefAny`.
///
/// Holds the application state, component map, spmc receiver, and window ID
/// so that `debug_timer_callback` can process debug requests for this window.
#[cfg(feature = "std")]
#[cfg(feature = "e2e-server")]
struct DebugTimerData {
    /// The user's application state (`GetAppState` / `SetAppState`)
    app_data: azul_core::refany::RefAny,
    /// Shared component map built from `AppConfig::component_libraries`
    component_map: Arc<Mutex<azul_core::xml::ComponentMap>>,
    /// This window's clone of the spmc receiver
    request_rx: spmc::Receiver<DebugRequest>,
    /// This window's unique ID for request routing
    window_id: String,
    /// This window's E2E scheduler slot — the suspended scenario run that has
    /// to survive between timer ticks. Per-window on purpose: it used to be a
    /// process-global, so two windows shared one slot.
    session: E2eSession,
}

// Re-export log categories for convenience
pub use LogCategory::*;

#[cfg(test)]
mod e2e_manager_accounting {
    use super::{KNOWN_MANAGERS, UNOBSERVABLE_MANAGERS};

    /// Every module in `layout/src/managers/` must be either CHECKED by
    /// `assert_manager_invariants` or explicitly recorded as unobservable, with a
    /// reason.
    ///
    /// This is the gate, not the lists themselves. `gpu_state` was simply absent
    /// from `KNOWN_MANAGERS`, so no invariant could see it and the scrollbar-fade
    /// latch stayed invisible to every assertion in this file — caught eventually,
    /// and only incidentally, by a hard-coded field read somewhere else. Absence
    /// is silent; that is the whole problem. Adding a manager module now breaks
    /// this test until somebody classifies it, which is the only way "we forgot
    /// one" becomes impossible rather than merely unlikely.
    #[test]
    fn every_manager_module_is_either_checked_or_declared_unobservable() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/managers");
        let mut modules: Vec<String> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("cannot read {dir}: {e}"))
            .filter_map(Result::ok)
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let stem = name.strip_suffix(".rs")?.to_string();
                // `mod.rs` is the module root, not a manager.
                (stem != "mod").then_some(stem)
            })
            .collect();
        modules.sort();
        assert!(
            !modules.is_empty(),
            "found no manager modules under {dir} — this test would pass vacuously",
        );

        // `scroll_state` and `focus_cursor` are the FILE names; the assertion
        // addresses them as "scroll" and "focus", which is what a scenario writes.
        fn alias(m: &str) -> &str {
            match m {
                "scroll_state" => "scroll",
                "focus_cursor" => "focus",
                other => other,
            }
        }

        let unclassified: Vec<&String> = modules
            .iter()
            .filter(|m| {
                let a = alias(m);
                !KNOWN_MANAGERS.contains(&a) && !UNOBSERVABLE_MANAGERS.iter().any(|(n, _)| *n == a)
            })
            .collect();

        assert!(
            unclassified.is_empty(),
            "these manager modules are neither checked by assert_manager_invariants nor listed in \
             UNOBSERVABLE_MANAGERS: {unclassified:?}.\nAdd a real invariant, or record why it has \
             none. Do not leave it out — a manager nothing asserts on can latch forever and no \
             test will notice.",
        );

        // The reverse direction: a name nobody can request is dead weight that
        // reads as coverage.
        for (name, _) in UNOBSERVABLE_MANAGERS {
            assert!(
                modules.iter().any(|m| alias(m) == *name),
                "UNOBSERVABLE_MANAGERS lists '{name}', which is not a module under {dir}",
            );
        }
    }

    /// The same gate for NON-INTERFERENCE. A manager `snapshot_managers` does
    /// not record is worse than one no invariant checks: the leak it would have
    /// caught reads back as "nothing changed", i.e. as a PASS.
    #[test]
    fn every_manager_module_is_either_fingerprinted_or_declared_not_fingerprintable() {
        let fingerprinted = super::fingerprinted_managers();
        let not_fingerprintable = super::not_fingerprintable();

        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/managers");
        let mut modules: Vec<String> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("cannot read {dir}: {e}"))
            .filter_map(Result::ok)
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let stem = name.strip_suffix(".rs")?.to_string();
                (stem != "mod").then_some(stem)
            })
            .collect();
        modules.sort();
        assert!(
            !modules.is_empty(),
            "found no manager modules under {dir} — this test would pass vacuously",
        );

        fn alias(m: &str) -> &str {
            match m {
                "scroll_state" => "scroll",
                "focus_cursor" => "focus",
                other => other,
            }
        }

        let unclassified: Vec<&String> = modules
            .iter()
            .filter(|m| {
                let a = alias(m);
                !fingerprinted.contains(&a) && !not_fingerprintable.iter().any(|(n, _)| *n == a)
            })
            .collect();
        assert!(
            unclassified.is_empty(),
            "these manager modules are neither fingerprinted by snapshot_managers nor listed in \
             not_fingerprintable(): {unclassified:?}.\nFingerprint it, or record why it has no \
             state. A manager missing from the fingerprint set makes every leak into it read as \
             'nothing changed'.",
        );

        for (name, _) in &not_fingerprintable {
            assert!(
                modules.iter().any(|m| alias(m) == *name),
                "not_fingerprintable() lists '{name}', which is not a module under {dir}",
            );
        }

        // No name may be in both lists: one of the two statements would be a lie.
        for name in &fingerprinted {
            assert!(
                !not_fingerprintable.iter().any(|(n, _)| n == name),
                "'{name}' is both fingerprinted and declared not-fingerprintable",
            );
        }
    }
}

/// PROOF THAT THE NON-INTERFERENCE ASSERTION CAN FAIL.
///
/// A non-interference check that cannot go red silently blesses every
/// cross-manager leak in the engine, which is strictly worse than not having one
/// — a green suite would then be evidence FOR correctness that nothing produced.
/// Two things have to be true for `assert_only_managers_changed` to be real, and
/// each gets its own test here:
///
/// 1. The DIFF must report both failure directions: a manager that moved without
///    being listed (the leak), and a listed manager that did not move (a
///    scenario that has quietly stopped exercising what it names).
/// 2. Every FINGERPRINT must actually move when its manager is written. A
///    fingerprint that ignores the field somebody later leaks into is exactly the
///    "cannot fail" defect at the level below the diff — and this repo has
///    shipped it: `gpu_state` sat outside `KNOWN_MANAGERS`, so the scrollbar-fade
///    latch was invisible to every invariant in this file.
#[cfg(all(test, feature = "std"))]
mod non_interference_can_fail {
    use alloc::collections::BTreeMap;

    use super::{diff_manager_fingerprints, ManagerFingerprint};

    fn fp(digest: &str) -> ManagerFingerprint {
        ManagerFingerprint::new(1, digest.to_string())
    }

    fn snapshot(pairs: &[(&str, &str)]) -> BTreeMap<String, ManagerFingerprint> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), fp(v)))
            .collect()
    }

    fn names(v: &[String]) -> Vec<&str> {
        v.iter().map(String::as_str).collect()
    }

    #[test]
    fn a_manager_that_moved_without_being_listed_is_reported() {
        let before = snapshot(&[("focus", "none"), ("scroll", "(0,4)=(0.00,0.00)")]);
        // Focus moved as expected AND scroll moved, which nobody asked for.
        let after = snapshot(&[("focus", "(0,7)"), ("scroll", "(0,4)=(0.00,120.00)")]);
        let (moved, stale) = diff_manager_fingerprints(&before, &after, &["focus".to_string()]);
        assert_eq!(names(&moved), vec!["focus", "scroll"]);
        assert!(stale.is_empty());
        // The op only lists `focus`, so `scroll` is the leak the caller must see.
        let leaked: Vec<&String> = moved.iter().filter(|m| m.as_str() != "focus").collect();
        assert_eq!(
            leaked.len(),
            1,
            "a manager moved outside the declared set and the diff did not surface it — this \
             assertion would bless every cross-manager leak",
        );
    }

    #[test]
    fn a_listed_manager_that_did_not_move_is_reported() {
        let before = snapshot(&[("focus", "none"), ("scroll", "(0,4)=(0.00,0.00)")]);
        let after = snapshot(&[("focus", "none"), ("scroll", "(0,4)=(0.00,120.00)")]);
        let (moved, stale) = diff_manager_fingerprints(
            &before,
            &after,
            &["focus".to_string(), "scroll".to_string()],
        );
        assert_eq!(names(&moved), vec!["scroll"]);
        assert_eq!(
            names(&stale),
            vec!["focus"],
            "a scenario named `focus` and focus never moved; without this the scenario keeps \
             passing long after it stopped exercising focus at all",
        );
    }

    #[test]
    fn an_exact_match_is_silent() {
        let before = snapshot(&[("focus", "none"), ("scroll", "(0,4)=(0.00,0.00)")]);
        let after = snapshot(&[("focus", "(0,7)"), ("scroll", "(0,4)=(0.00,0.00)")]);
        let (moved, stale) = diff_manager_fingerprints(&before, &after, &["focus".to_string()]);
        assert_eq!(names(&moved), vec!["focus"]);
        assert!(
            stale.is_empty(),
            "the expected set moved exactly; a red here would make the assertion unusable and \
             invite somebody to weaken it",
        );
    }

    #[test]
    fn a_manager_appearing_or_vanishing_counts_as_moved() {
        // Two snapshots with different manager SETS mean the builds disagree
        // about what exists. Skipping the odd one out is how a whole manager
        // drops out of a non-interference check without anybody noticing.
        let before = snapshot(&[("focus", "none")]);
        let after = snapshot(&[("focus", "none"), ("a11y", "root=1")]);
        let (moved, _) = diff_manager_fingerprints(&before, &after, &[]);
        assert_eq!(names(&moved), vec!["a11y"]);

        let (moved_back, _) = diff_manager_fingerprints(&after, &before, &[]);
        assert_eq!(names(&moved_back), vec!["a11y"]);
    }

    /// The other half of the proof: a fingerprint must MOVE when its manager is
    /// written. Each case builds a default manager, writes ONE thing through the
    /// manager's own public API, and requires the digest to differ.
    ///
    /// This is why the `fp_*` functions take their manager rather than the
    /// window: it makes them testable with no engine, no layout and no fonts.
    #[test]
    fn every_fingerprint_moves_when_its_manager_moves() {
        use azul_core::dom::{DomId, DomNodeId, NodeId};

        const ROOT: DomId = DomId { inner: 0 };
        let mut moved: Vec<&str> = Vec::new();

        macro_rules! assert_moves {
            ($name:literal, $mk:expr, $mutate:expr, $fp:path) => {{
                let mut m = $mk;
                let before = $fp(&m);
                #[allow(clippy::redundant_closure_call)]
                ($mutate)(&mut m);
                let after = $fp(&m);
                assert_ne!(
                    before, after,
                    "the `{}` fingerprint did NOT change after writing that manager. A \
                     fingerprint blind to a field is an assertion that cannot fail: every leak \
                     into `{}` would report as 'nothing changed'.",
                    $name, $name
                );
                moved.push($name);
            }};
        }

        assert_moves!(
            "scroll",
            azul_layout::managers::scroll_state::ScrollManager::new(),
            |m: &mut azul_layout::managers::scroll_state::ScrollManager| {
                m.pending_wheel_event = Some(azul_core::geom::LogicalPosition { x: 1.0, y: 2.0 });
            },
            super::fp_scroll
        );
        assert_moves!(
            "hover",
            azul_layout::managers::hover::HoverManager::new(),
            |m: &mut azul_layout::managers::hover::HoverManager| {
                m.push_hit_test(
                    azul_layout::managers::hover::InputPointId::Mouse,
                    azul_core::hit_test::FullHitTest::empty(None),
                );
            },
            super::fp_hover
        );
        assert_moves!(
            "focus",
            azul_layout::managers::focus_cursor::FocusManager::new(),
            |m: &mut azul_layout::managers::focus_cursor::FocusManager| {
                m.cursor_needs_initialization = true;
            },
            super::fp_focus
        );
        assert_moves!(
            "gesture",
            azul_layout::managers::gesture::GestureAndDragManager::new(),
            |m: &mut azul_layout::managers::gesture::GestureAndDragManager| {
                m.pen_event_pending = true;
            },
            super::fp_gesture
        );
        assert_moves!(
            "text_edit",
            azul_layout::managers::text_edit::TextEditManager::new(),
            |m: &mut azul_layout::managers::text_edit::TextEditManager| {
                m.display_list_dirty = true;
            },
            super::fp_text_edit
        );
        assert_moves!(
            "text_input",
            azul_layout::managers::text_input::TextInputManager::new(),
            |m: &mut azul_layout::managers::text_input::TextInputManager| {
                m.record_input(
                    azul_core::dom::DomNodeId::ROOT,
                    "x".to_string(),
                    String::new(),
                    azul_layout::managers::text_input::TextInputSource::Keyboard,
                );
            },
            super::fp_text_input
        );
        assert_moves!(
            "undo_redo",
            azul_layout::managers::undo_redo::UndoRedoManager::new(),
            |m: &mut azul_layout::managers::undo_redo::UndoRedoManager| {
                m.node_stacks
                    .push(azul_layout::managers::undo_redo::NodeUndoRedoStack::new(
                        NodeId::new(3),
                    ));
            },
            super::fp_undo_redo
        );
        assert_moves!(
            "virtual_view",
            azul_layout::managers::virtual_view::VirtualViewManager::new(),
            |m: &mut azul_layout::managers::virtual_view::VirtualViewManager| {
                let _ = m.get_or_create_nested_dom_id(ROOT, NodeId::new(1));
            },
            super::fp_virtual_view
        );
        assert_moves!(
            "gpu_state",
            azul_layout::managers::gpu_state::GpuStateManager::default(),
            |m: &mut azul_layout::managers::gpu_state::GpuStateManager| {
                m.scrollbar_fade_active = true;
            },
            super::fp_gpu_state
        );
        assert_moves!(
            "permission",
            azul_layout::managers::permission::PermissionManager::new(),
            |m: &mut azul_layout::managers::permission::PermissionManager| {
                m.subscribe(
                    azul_layout::managers::permission::Capability::Camera,
                    DomNodeId::ROOT,
                );
            },
            super::fp_permission
        );
        assert_moves!(
            "clipboard",
            azul_layout::managers::clipboard::ClipboardManager::new(),
            |m: &mut azul_layout::managers::clipboard::ClipboardManager| {
                m.set_copy_content(azul_layout::managers::selection::ClipboardContent {
                    plain_text: "x".to_string().into(),
                    styled_runs: azul_layout::managers::selection::StyledTextRunVec::from_vec(
                        Vec::new(),
                    ),
                });
            },
            super::fp_clipboard
        );
        assert_moves!(
            "file_drop",
            azul_layout::managers::file_drop::FileDropManager::new(),
            |m: &mut azul_layout::managers::file_drop::FileDropManager| {
                m.set_dropped_file(Some("f".to_string().into()));
            },
            super::fp_file_drop
        );
        assert_moves!(
            "gamepad",
            azul_layout::managers::gamepad::GamepadManager::new(),
            |m: &mut azul_layout::managers::gamepad::GamepadManager| {
                m.set_has_listeners(true);
            },
            super::fp_gamepad
        );
        assert_moves!(
            "geolocation",
            azul_layout::managers::geolocation::GeolocationManager::new(),
            |m: &mut azul_layout::managers::geolocation::GeolocationManager| {
                m.set_last_error(azul_layout::managers::geolocation::LocationError {
                    code: 7,
                    message: "denied".to_string(),
                });
            },
            super::fp_geolocation
        );
        assert_moves!(
            "biometric",
            azul_layout::managers::biometric::BiometricManager::new(),
            |m: &mut azul_layout::managers::biometric::BiometricManager| {
                m.pending_event = true;
            },
            super::fp_biometric
        );
        assert_moves!(
            "keyring",
            azul_layout::managers::keyring::KeyringManager::default(),
            |m: &mut azul_layout::managers::keyring::KeyringManager| {
                m.pending_event = true;
            },
            super::fp_keyring
        );
        assert_moves!(
            "sensors",
            azul_layout::managers::sensors::SensorManager::new(),
            |m: &mut azul_layout::managers::sensors::SensorManager| {
                m.pending_event = true;
            },
            super::fp_sensors
        );
        assert_moves!(
            "eyedropper",
            azul_layout::managers::eyedropper::EyedropperManager::new(),
            |m: &mut azul_layout::managers::eyedropper::EyedropperManager| {
                // begin_request() bumps the PROCESS-GLOBAL `IN_FLIGHT` counter
                // and only fold_result() releases it — a bare begin here leaked
                // one in-flight pick forever and raced
                // `results_are_routed_to_the_window_that_asked`'s
                // `assert!(!in_flight_anywhere())` in the parallel test run.
                // Complete the pick: the fingerprint still moves, via
                // `last_result` (None → Some(None)).
                let id = m.begin_request();
                let _ = m.fold_result(id, None);
            },
            super::fp_eyedropper
        );
        #[cfg(feature = "a11y")]
        assert_moves!(
            "a11y",
            azul_layout::managers::a11y::A11yManager::new(),
            |m: &mut azul_layout::managers::a11y::A11yManager| {
                m.tree_initialized = !m.tree_initialized;
            },
            super::fp_a11y
        );

        // And the set covered here must be the WHOLE fingerprint set — otherwise
        // a manager could be fingerprinted and never proven sensitive, which is
        // the same hole one level down.
        let mut covered = moved;
        covered.sort_unstable();
        let mut declared: Vec<&str> = super::fingerprinted_managers();
        declared.sort_unstable();
        assert_eq!(
            covered, declared,
            "every fingerprinted manager must be proven to MOVE when written; the two sets \
             disagree, so at least one fingerprint is untested and may be a constant",
        );
    }
}

/// Decode a base64 payload for `AZ_E2E_SHOT_DIR`. Standard alphabet, padding
/// tolerated; anything malformed is skipped rather than panicking a test run.
#[cfg(feature = "std")]
fn base64_decode_for_shot(input: &str) -> Result<Vec<u8>, ()> {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lut = [255u8; 256];
    let mut i = 0;
    while i < 64 {
        lut[T[i] as usize] = i as u8;
        i += 1;
    }
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for b in input.bytes() {
        if b == b'=' || b.is_ascii_whitespace() {
            continue;
        }
        let v = lut[b as usize];
        if v == 255 {
            return Err(());
        }
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Ok(out)
}

#[cfg(all(test, feature = "std"))]
mod assert_stderr_tests {
    use super::*;

    /// The diagnostics ring is GLOBAL, so these must not run concurrently —
    /// one clearing it mid-assert makes another flake. This bit once already:
    /// the tests passed individually and failed as a group.
    // Workspace-wide: see azul_core::diagnostics::test_lock.

    /// `assert_stderr` reads the diagnostics ring every engine lint writes to.
    ///
    /// This is what lets an e2e scenario say "provoke the conditions of the
    /// image-churn lint and assert it stays quiet" — a regression test for the
    /// underlying bug, expressed as the absence of a warning. And, because the
    /// same diagnostics flow through telemetry::install_diagnostics_bridge, an
    /// e2e run reports into Loki/Grafana like any other run.
    #[test]
    fn assert_stderr_finds_and_refuses_diagnostics() {
        let _g = azul_core::diagnostics::test_lock().lock();
        azul_core::diagnostics::clear();
        azul_core::diagnostics::record("[azul][image-churn] node 7 rebuilt".to_string());

        let hit = eval_assert_stderr(&serde_json::json!({ "contains": "image-churn" }));
        assert!(
            hit.passed,
            "should find the recorded diagnostic: {}",
            hit.message
        );

        let missing = eval_assert_stderr(&serde_json::json!({ "contains": "no-such-lint" }));
        assert!(
            !missing.passed,
            "must not claim to find a diagnostic that is absent"
        );

        let forbidden = eval_assert_stderr(&serde_json::json!({ "not_contains": "image-churn" }));
        assert!(
            !forbidden.passed,
            "not_contains must FAIL when the needle is present"
        );

        let clean = eval_assert_stderr(&serde_json::json!({ "not_contains": "no-such-lint" }));
        assert!(
            clean.passed,
            "not_contains passes when the needle is absent"
        );

        azul_core::diagnostics::clear();
    }

    /// An assertion with no needle asserts nothing, and must say so rather than
    /// passing forever.
    #[test]
    fn assert_stderr_without_a_needle_is_rejected() {
        let _g = azul_core::diagnostics::test_lock().lock();
        let empty = eval_assert_stderr(&serde_json::json!({}));
        assert!(!empty.passed);
        assert!(empty.message.contains("contains"), "{}", empty.message);
    }

    /// `clear` empties the ring afterwards, so a warning from an early step
    /// cannot satisfy a later assertion.
    #[test]
    fn assert_stderr_can_clear_the_ring() {
        let _g = azul_core::diagnostics::test_lock().lock();
        azul_core::diagnostics::clear();
        azul_core::diagnostics::record("[azul][test] marker".to_string());
        let r = eval_assert_stderr(&serde_json::json!({ "contains": "marker", "clear": true }));
        assert!(r.passed);
        let again = eval_assert_stderr(&serde_json::json!({ "contains": "marker" }));
        assert!(
            !again.passed,
            "clear:true must empty the ring after evaluating"
        );
    }
}

/// The `dom_id` envelope field: one spelling that reaches every
/// node-addressing op, so a VirtualView's document and a
/// `<transient-window>`'s popup content are scriptable without hand-rolled
/// coordinates.
///
/// NEGATIVE CONTROL for each: hardcode `ROOT_DOM_ID` back into `target_dom` /
/// `params_dom`, or drop `"dom_id"` from `HARNESS_KEYS`.
#[cfg(test)]
mod dom_id_envelope_tests {
    use super::*;

    fn request_with(dom_id: Option<u64>) -> DebugRequest {
        let (tx, _rx) = mpsc::channel();
        DebugRequest {
            request_id: 1,
            event: DebugEvent::GetState,
            window_id: None,
            wait_for_render: false,
            dom_id,
            response_tx: tx,
        }
    }

    /// An op that says nothing addresses DOM 0 — the behaviour every existing
    /// script was written against.
    #[test]
    fn an_op_without_a_dom_id_still_addresses_the_root_dom() {
        assert_eq!(target_dom(&request_with(None)), ROOT_DOM_ID);
        assert_eq!(params_dom(&serde_json::json!({})), ROOT_DOM_ID);
    }

    /// …and one that names a DOM gets that DOM.
    #[test]
    fn a_dom_id_addresses_that_dom() {
        assert_eq!(target_dom(&request_with(Some(3))).inner, 3);
        assert_eq!(params_dom(&serde_json::json!({ "dom_id": 2 })).inner, 2);
    }

    /// `dom_id` rides the ENVELOPE, beside `op` — not inside any one op's
    /// parameter set. Parsing it as an op field would mean adding it to 90+
    /// variants and letting them disagree.
    #[test]
    fn dom_id_parses_beside_the_op_without_disturbing_it() {
        #[derive(serde::Deserialize)]
        struct Envelope {
            #[serde(flatten)]
            event: DebugEvent,
            #[serde(default)]
            dom_id: Option<u64>,
        }
        let parsed: Envelope =
            serde_json::from_str(r#"{"op":"click","selector":".mw-doc","dom_id":1}"#)
                .expect("an op with a dom_id must parse");
        assert_eq!(parsed.dom_id, Some(1));
        match parsed.event {
            DebugEvent::Click { ref selector, .. } => {
                assert_eq!(selector.as_deref(), Some(".mw-doc"));
            }
            other => panic!("dom_id must not change which op was parsed, got {other:?}"),
        }
    }

    /// Assertions take their params as raw JSON and reject unknown keys, so a
    /// typo cannot assert nothing. `dom_id` is a harness key on every one of
    /// them — rejecting it would make popup content unassertable.
    #[test]
    fn assertions_accept_dom_id_as_a_harness_key() {
        let params = serde_json::json!({ "selector": ".x", "dom_id": 1 });
        assert!(
            reject_unknown_params("assert_exists", &params, &["selector"]).is_none(),
            "dom_id must be accepted on every assertion"
        );
        let typo = serde_json::json!({ "selector": ".x", "dom_di": 1 });
        assert!(
            reject_unknown_params("assert_exists", &typo, &["selector"]).is_some(),
            "a typo'd key must still fail, or the guard is worthless"
        );
    }
}
