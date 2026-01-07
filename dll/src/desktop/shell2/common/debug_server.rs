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
    /// Scroll to node result
    ScrollToNode(ScrollToNodeResponse),
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
pub enum HttpResponse {
    #[serde(rename = "ok")]
    Ok(HttpResponseOk),
    #[serde(rename = "error")]
    Error(HttpResponseError),
}

/// Successful HTTP response body
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct HttpResponseOk {
    pub request_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_state: Option<WindowStateSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<ResponseData>,
}

/// Error HTTP response body
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct HttpResponseError {
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
}

#[cfg(feature = "std")]
impl serde::Serialize for WindowStateSnapshot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("WindowStateSnapshot", 9)?;
        s.serialize_field("window_id", &self.window_id)?;
        s.serialize_field("logical_width", &self.logical_width)?;
        s.serialize_field("logical_height", &self.logical_height)?;
        s.serialize_field("physical_width", &self.physical_width)?;
        s.serialize_field("physical_height", &self.physical_height)?;
        s.serialize_field("dpi", &self.dpi)?;
        s.serialize_field("hidpi_factor", &self.hidpi_factor)?;
        s.serialize_field("focused", &self.focused)?;
        s.serialize_field("dom_node_count", &self.dom_node_count)?;
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
    pub text: Option<String>,
    pub parent: i64,
    pub prev_sibling: i64,
    pub next_sibling: i64,
    pub last_child: i64,
    pub children: Vec<usize>,
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

/// Response for ScrollToNode
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct ScrollToNodeResponse {
    pub scrolled: bool,
    pub node_id: u64,
    pub x: f32,
    pub y: f32,
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

// ==================== Debug Events ====================

#[derive(Debug, Clone)]
#[cfg_attr(feature = "std", derive(serde::Deserialize))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DebugEvent {
    // Mouse Events
    MouseMove { x: f32, y: f32 },
    MouseDown { x: f32, y: f32, #[serde(default)] button: MouseButton },
    MouseUp { x: f32, y: f32, #[serde(default)] button: MouseButton },
    Click { x: f32, y: f32, #[serde(default)] button: MouseButton },
    DoubleClick { x: f32, y: f32, #[serde(default)] button: MouseButton },
    Scroll { x: f32, y: f32, delta_x: f32, delta_y: f32 },
    
    // Keyboard Events
    KeyDown { key: String, #[serde(default)] modifiers: Modifiers },
    KeyUp { key: String, #[serde(default)] modifiers: Modifiers },
    TextInput { text: String },
    
    // Window Events
    Resize { width: f32, height: f32 },
    Move { x: i32, y: i32 },
    Focus,
    Blur,
    Close,
    DpiChanged { dpi: u32 },
    
    // Queries
    GetState,
    GetDom,
    HitTest { x: f32, y: f32 },
    GetLogs { #[serde(default)] since_request_id: Option<u64> },
    
    // DOM Inspection
    /// Get the HTML representation of the DOM
    GetHtmlString,
    /// Get all computed CSS properties for a node
    GetNodeCssProperties { 
        #[serde(default)] 
        node_id: u64 
    },
    /// Get node layout information (position, size)
    GetNodeLayout { 
        #[serde(default)] 
        node_id: u64 
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
    /// Scroll a specific node to a position
    ScrollToNode { node_id: u64, x: f32, y: f32 },
    
    // Node Finding & Interaction
    /// Find a node by CSS class name (returns node_id and bounds)
    FindNodeByClass { class_name: String },
    /// Find a node by text content (returns node_id and bounds)
    FindNodeByText { text: String },
    /// Click on a specific node by its ID (clicks at center of node)
    ClickNode { node_id: u64, #[serde(default)] button: MouseButton },
    
    // Control
    Relayout,
    Redraw,
    
    // Testing
    WaitFrame,
    Wait { ms: u64 },
    
    // Screenshots
    TakeScreenshot,
    TakeNativeScreenshot,
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
static REQUEST_QUEUE: OnceLock<Mutex<VecDeque<DebugRequest>>> = OnceLock::new();

#[cfg(feature = "std")]
static LOG_QUEUE: OnceLock<Mutex<Vec<LogMessage>>> = OnceLock::new();

#[cfg(feature = "std")]
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

#[cfg(feature = "std")]
static SERVER_START_TIME: OnceLock<std::time::Instant> = OnceLock::new();

#[cfg(feature = "std")]
static DEBUG_ENABLED: AtomicBool = AtomicBool::new(false);

#[cfg(feature = "std")]
static DEBUG_PORT: OnceLock<u16> = OnceLock::new();

// ==================== Debug Server Handle ====================

/// Handle to the debug server for clean shutdown
#[cfg(feature = "std")]
pub struct DebugServerHandle {
    pub shutdown_tx: mpsc::Sender<()>,
    pub thread_handle: Mutex<Option<std::thread::JoinHandle<()>>>,
    pub port: u16,
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

/// Check if debug mode is enabled
#[cfg(feature = "std")]
pub fn is_debug_enabled() -> bool {
    DEBUG_ENABLED.load(Ordering::SeqCst)
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
/// 1. Binds to the port (exits process if port is taken)
/// 2. Starts the HTTP server thread
/// 3. Blocks until the server is ready to accept connections
/// 4. Returns a handle for clean shutdown
/// 
/// Should be called from App::create().
#[cfg(feature = "std")]
pub fn start_debug_server(port: u16) -> DebugServerHandle {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::Duration;

    // Initialize static state
    SERVER_START_TIME.get_or_init(std::time::Instant::now);
    REQUEST_QUEUE.get_or_init(|| Mutex::new(VecDeque::new()));
    LOG_QUEUE.get_or_init(|| Mutex::new(Vec::new()));
    let _ = DEBUG_PORT.set(port);
    DEBUG_ENABLED.store(true, Ordering::SeqCst);

    // Try to bind - exit if port is taken
    let listener = match TcpListener::bind(format!("127.0.0.1:{}", port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("ERROR: Debug server failed to bind to port {}: {}", port, e);
            eprintln!("       Another process may be using this port.");
            eprintln!("       Try a different port: AZUL_DEBUG=<other_port>");
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
                        handle_http_connection(&mut stream);
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

    DebugServerHandle {
        shutdown_tx,
        thread_handle: Mutex::new(Some(thread_handle)),
        port,
    }
}

/// Log a message (thread-safe, lock-free when debug is disabled)
#[cfg(feature = "std")]
#[track_caller]
pub fn log(level: LogLevel, category: LogCategory, message: impl Into<String>, window_id: Option<&str>) {
    if !is_debug_enabled() {
        return;
    }
    log_internal(level, category, message, window_id);
}

#[cfg(feature = "std")]
#[track_caller]
fn log_internal(level: LogLevel, category: LogCategory, message: impl Into<String>, window_id: Option<&str>) {
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

/// Pop a debug request from the queue (called by timer callback)
#[cfg(feature = "std")]
pub fn pop_request() -> Option<DebugRequest> {
    REQUEST_QUEUE.get()?.lock().ok()?.pop_front()
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
        eprintln!("[DEBUG] ERROR sending response {}: {:?}", request.request_id, e);
    }
}

/// Send an error response to a debug request
#[cfg(feature = "std")]
pub fn send_err(request: &DebugRequest, message: impl Into<String>) {
    // Clear logs to prevent memory buildup
    let _ = take_logs();
    let response = DebugResponseData::Err(message.into());
    if let Err(e) = request.response_tx.send(response) {
        eprintln!("[DEBUG] ERROR sending response {}: {:?}", request.request_id, e);
    }
}

/// Helper function for serializing HttpResponse
#[cfg(feature = "std")]
fn serialize_http_response(response: &HttpResponse) -> String {
    serde_json::to_string_pretty(response).unwrap_or_else(|_| r#"{"status":"error","message":"Serialization failed"}"#.to_string())
}

// ==================== HTTP Server ====================

#[cfg(feature = "std")]
fn handle_http_connection(stream: &mut std::net::TcpStream) {
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

    let response_json = match (method, path) {
        // Health check - GET /
        ("GET", "/") | ("GET", "/health") => {
            let logs = take_logs();
            let health = HealthResponse {
                port: DEBUG_PORT.get().copied().unwrap_or(0),
                pending_logs: logs.len(),
                logs: logs.iter().map(|l| LogMessageJson {
                    timestamp_us: l.timestamp_us,
                    level: format!("{:?}", l.level),
                    category: format!("{:?}", l.category),
                    message: l.message.clone(),
                }).collect(),
            };
            serialize_http_response(&HttpResponse::Ok(HttpResponseOk {
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
                handle_event_request(body)
            } else {
                serialize_http_response(&HttpResponse::Error(HttpResponseError {
                    request_id: None,
                    message: "No request body".to_string(),
                }))
            }
        }

        _ => {
            serialize_http_response(&HttpResponse::Error(HttpResponseError {
                request_id: None,
                message: "Use GET / for status or POST / with JSON body".to_string(),
            }))
        }
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
                eprintln!("[DebugServer] Error writing chunk to stream after {} of {} bytes: {:?}", 
                    bytes_written, body_bytes.len(), e);
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
        if n == 0 { break; } // EOF received, client closed connection
    }
}

#[cfg(feature = "std")]
fn handle_event_request(body: &str) -> String {
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

            // Push to queue
            if let Some(queue) = REQUEST_QUEUE.get() {
                if let Ok(mut q) = queue.lock() {
                    q.push_back(request);
                }
            }

            // Wait for response (with timeout)
            match rx.recv_timeout(Duration::from_secs(30)) {
                Ok(response_data) => {
                    let http_response = match response_data {
                        DebugResponseData::Ok { window_state, data } => {
                            HttpResponse::Ok(HttpResponseOk {
                                request_id,
                                window_state,
                                data,
                            })
                        }
                        DebugResponseData::Err(message) => {
                            HttpResponse::Error(HttpResponseError {
                                request_id: Some(request_id),
                                message,
                            })
                        }
                    };
                    serialize_http_response(&http_response)
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    serialize_http_response(&HttpResponse::Error(HttpResponseError {
                        request_id: Some(request_id),
                        message: "Timeout waiting for response (is the timer running?)".to_string(),
                    }))
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    serialize_http_response(&HttpResponse::Error(HttpResponseError {
                        request_id: Some(request_id),
                        message: "Event loop disconnected".to_string(),
                    }))
                }
            }
        }
        Err(e) => {
            serialize_http_response(&HttpResponse::Error(HttpResponseError {
                request_id: None,
                message: format!("Invalid JSON: {}", e),
            }))
        }
    }
}

// ==================== Timer Callback ====================

/// Timer callback that processes debug requests.
/// Called every ~16ms when debug mode is enabled.
#[cfg(feature = "std")]
pub extern "C" fn debug_timer_callback(
    _timer_data: azul_core::refany::RefAny,
    mut timer_info: azul_layout::timer::TimerCallbackInfo,
) -> azul_core::callbacks::TimerCallbackReturn {
    use azul_core::callbacks::{TimerCallbackReturn, Update};
    use azul_core::task::TerminateTimer;

    // Check queue length first (without popping)
    let queue_len = REQUEST_QUEUE.get()
        .and_then(|q| q.lock().ok())
        .map(|q| q.len())
        .unwrap_or(0);
    


    // Process all pending requests (no debug output unless there's work to do)
    let mut needs_update = false;
    let mut processed_count = 0;

    while let Some(request) = pop_request() {
        log(
            LogLevel::Debug,
            LogCategory::DebugServer,
            format!("Processing: {:?}", request.event),
            request.window_id.as_deref(),
        );

        let result = process_debug_event(&request, &mut timer_info.callback_info);
        needs_update = needs_update || result;
        processed_count += 1;
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
fn process_debug_event(
    request: &DebugRequest,
    callback_info: &mut azul_layout::callbacks::CallbackInfo,
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
            new_state.mouse_state.cursor_position = azul_core::window::CursorPosition::InWindow(
                LogicalPosition { x: *x, y: *y }
            );
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
            new_state.mouse_state.cursor_position = azul_core::window::CursorPosition::InWindow(
                LogicalPosition { x: *x, y: *y }
            );
            match button {
                MouseButton::Left => new_state.mouse_state.left_down = true,
                MouseButton::Right => new_state.mouse_state.right_down = true,
                MouseButton::Middle => new_state.mouse_state.middle_down = true,
            }
            callback_info.modify_window_state(new_state);
            needs_update = true;

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
            new_state.mouse_state.cursor_position = azul_core::window::CursorPosition::InWindow(
                LogicalPosition { x: *x, y: *y }
            );
            match button {
                MouseButton::Left => new_state.mouse_state.left_down = false,
                MouseButton::Right => new_state.mouse_state.right_down = false,
                MouseButton::Middle => new_state.mouse_state.middle_down = false,
            }
            callback_info.modify_window_state(new_state);
            needs_update = true;

            send_ok(request, None, None);
        }

        DebugEvent::Click { x, y, button } => {
            log(
                LogLevel::Debug,
                LogCategory::EventLoop,
                format!("Debug click at ({}, {}) button {:?}", x, y, button),
                None,
            );

            // Click = mouse down + mouse up at same position
            let mut new_state = callback_info.get_current_window_state().clone();
            new_state.mouse_state.cursor_position = azul_core::window::CursorPosition::InWindow(
                LogicalPosition { x: *x, y: *y }
            );
            // First set button down
            match button {
                MouseButton::Left => new_state.mouse_state.left_down = true,
                MouseButton::Right => new_state.mouse_state.right_down = true,
                MouseButton::Middle => new_state.mouse_state.middle_down = true,
            }
            callback_info.modify_window_state(new_state.clone());
            
            // Then set button up (this should trigger the mouseUp event)
            match button {
                MouseButton::Left => new_state.mouse_state.left_down = false,
                MouseButton::Right => new_state.mouse_state.right_down = false,
                MouseButton::Middle => new_state.mouse_state.middle_down = false,
            }
            callback_info.modify_window_state(new_state);
            needs_update = true;

            send_ok(request, None, None);
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
            new_state.mouse_state.cursor_position = azul_core::window::CursorPosition::InWindow(
                LogicalPosition { x: *x, y: *y }
            );
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

        DebugEvent::Scroll { x, y, delta_x, delta_y } => {
            log(
                LogLevel::Debug,
                LogCategory::EventLoop,
                format!("Debug scroll at ({}, {}) delta ({}, {})", x, y, delta_x, delta_y),
                None,
            );

            // Scroll events are handled differently - just move cursor and log for now
            // TODO: Implement scroll state modification when available
            let mut new_state = callback_info.get_current_window_state().clone();
            new_state.mouse_state.cursor_position = azul_core::window::CursorPosition::InWindow(
                LogicalPosition { x: *x, y: *y }
            );
            callback_info.modify_window_state(new_state);
            needs_update = true;

            send_ok(request, None, None);
        }

        DebugEvent::Relayout => {
            log(LogLevel::Info, LogCategory::Layout, "Forcing relayout", None);
            needs_update = true;
            send_ok(request, None, None);
        }

        DebugEvent::Redraw => {
            log(LogLevel::Info, LogCategory::Rendering, "Requesting redraw", None);
            needs_update = true;
            send_ok(request, None, None);
        }

        DebugEvent::Close => {
            log(LogLevel::Info, LogCategory::EventLoop, "Close via close_window()", None);
            callback_info.close_window();
            needs_update = true;
            send_ok(request, None, None);
        }

        DebugEvent::HitTest { x, y } => {
            let hit_test = callback_info.get_hit_test_frame(0);
            let response = HitTestResponse {
                x: *x,
                y: *y,
                node_id: None, // TODO: extract from hit_test
                node_tag: None,
            };
            send_ok(request, None, Some(ResponseData::HitTest(response)));
        }

        DebugEvent::GetLogs { .. } => {
            let logs = take_logs();
            send_ok(request, None, Some(ResponseData::Logs(LogsResponse { logs })));
        }

        DebugEvent::WaitFrame => {
            send_ok(request, None, None);
        }

        DebugEvent::Wait { ms } => {
            std::thread::sleep(std::time::Duration::from_millis(*ms));
            send_ok(request, None, None);
        }

        DebugEvent::TakeScreenshot => {
            log(LogLevel::Info, LogCategory::Rendering, "Taking CPU screenshot via debug API", None);
            // Use DomId(0) as default - first DOM in the window
            let dom_id = azul_core::dom::DomId { inner: 0 };
            match callback_info.take_screenshot_base64(dom_id) {
                Ok(data_uri) => {
                    let data = ScreenshotData { data: data_uri.as_str().to_string() };
                    send_ok(request, None, Some(ResponseData::Screenshot(data)));
                }
                Err(e) => {
                    send_err(request, e.as_str().to_string());
                }
            }
        }

        DebugEvent::TakeNativeScreenshot => {
            log(LogLevel::Info, LogCategory::Rendering, "Taking native screenshot via debug API", None);
            // Use the NativeScreenshotExt trait method explicitly (not the stubbed inherent method)
            match NativeScreenshotExt::take_native_screenshot_base64(callback_info) {
                Ok(data_uri) => {
                    let data = ScreenshotData { data: data_uri.as_str().to_string() };
                    send_ok(request, None, Some(ResponseData::Screenshot(data)));
                }
                Err(e) => {
                    send_err(request, e.as_str().to_string());
                }
            }
        }

        DebugEvent::GetHtmlString => {
            log(LogLevel::Debug, LogCategory::DebugServer, "Getting HTML string", None);
            let dom_id = azul_core::dom::DomId { inner: 0 };
            let layout_window = callback_info.get_layout_window();
            if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
                let html = layout_result.styled_dom.get_html_string("", "", true);
                send_ok(request, None, Some(ResponseData::HtmlString(HtmlStringResponse { html })));
            } else {
                send_err(request, "No layout result for DOM 0");
            }
        }

        DebugEvent::GetNodeCssProperties { node_id } => {
            log(LogLevel::Debug, LogCategory::DebugServer, format!("Getting CSS properties for node {}", node_id), None);
            use azul_css::props::property::CssPropertyType;
            use azul_core::dom::{DomId, DomNodeId, NodeId};
            use strum::IntoEnumIterator;
            
            let dom_node_id = DomNodeId {
                dom: DomId { inner: 0 },
                node: NodeId::from_usize(*node_id as usize).into(),
            };
            
            // Collect all CSS properties that are set on this node
            let mut props = Vec::new();
            
            // Iterate over all CSS property types
            for prop_type in CssPropertyType::iter() {
                if let Some(prop) = callback_info.get_computed_css_property(dom_node_id, prop_type) {
                    props.push(format!("{}: {:?}", prop_type.to_str(), prop));
                }
            }
            
            let response = NodeCssPropertiesResponse {
                node_id: *node_id,
                property_count: props.len(),
                properties: props,
            };
            send_ok(request, None, Some(ResponseData::NodeCssProperties(response)));
        }

        DebugEvent::GetNodeLayout { node_id } => {
            log(LogLevel::Debug, LogCategory::DebugServer, format!("Getting layout for node {}", node_id), None);
            use azul_core::dom::{DomId, DomNodeId, NodeId};
            
            let dom_node_id = DomNodeId {
                dom: DomId { inner: 0 },
                node: NodeId::from_usize(*node_id as usize).into(),
            };
            
            let size = callback_info.get_node_size(dom_node_id);
            let pos = callback_info.get_node_position(dom_node_id);
            let rect = callback_info.get_node_rect(dom_node_id);
            
            let response = NodeLayoutResponse {
                node_id: *node_id,
                size: size.map(|s| LogicalSizeJson { width: s.width, height: s.height }),
                position: pos.map(|p| LogicalPositionJson { x: p.x, y: p.y }),
                rect: rect.map(|r| LogicalRectJson { x: r.origin.x, y: r.origin.y, width: r.size.width, height: r.size.height }),
            };
            send_ok(request, None, Some(ResponseData::NodeLayout(response)));
        }

        DebugEvent::GetAllNodesLayout => {
            log(LogLevel::Debug, LogCategory::DebugServer, "Getting all nodes layout", None);
            use azul_core::dom::{DomId, DomNodeId, NodeId};
            
            let dom_id = DomId { inner: 0 };
            let layout_window = callback_info.get_layout_window();
            
            let mut nodes = Vec::new();
            if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
                let node_count = layout_result.styled_dom.node_data.len();
                for i in 0..node_count {
                    let dom_node_id = DomNodeId {
                        dom: dom_id.clone(),
                        node: NodeId::from_usize(i).into(),
                    };
                    
                    let rect = callback_info.get_node_rect(dom_node_id);
                    let tag = callback_info.get_node_tag_name(dom_node_id);
                    let id_attr = callback_info.get_node_id(dom_node_id);
                    let classes = callback_info.get_node_classes(dom_node_id);
                    
                    nodes.push(NodeLayoutInfo {
                        node_id: i,
                        tag: tag.map(|s| s.as_str().to_string()),
                        id: id_attr.map(|s| s.as_str().to_string()),
                        classes: classes.as_ref().iter().map(|s| s.as_str().to_string()).collect(),
                        rect: rect.map(|r| LogicalRectJson { 
                            x: r.origin.x, 
                            y: r.origin.y, 
                            width: r.size.width, 
                            height: r.size.height 
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
            log(LogLevel::Debug, LogCategory::DebugServer, "Getting DOM tree", None);
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
            log(LogLevel::Debug, LogCategory::DebugServer, "Getting node hierarchy", None);
            use azul_core::dom::DomId;
            use azul_core::id::NodeId;
            
            let dom_id = DomId { inner: 0 };
            let layout_window = callback_info.get_layout_window();
            
            if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
                let styled_dom = &layout_result.styled_dom;
                let hierarchy = styled_dom.node_hierarchy.as_container();
                let node_data = styled_dom.node_data.as_container();
                
                let root_decoded = styled_dom.root.into_crate_internal()
                    .map(|n| n.index() as i64)
                    .unwrap_or(-1);
                
                let mut nodes = Vec::new();
                for i in 0..hierarchy.len() {
                    let node_id = NodeId::new(i);
                    let hier = &hierarchy[node_id];
                    let data = &node_data[node_id];
                    
                    let node_type = match data.get_node_type() {
                        azul_core::dom::NodeType::Html => "Html",
                        azul_core::dom::NodeType::Head => "Head",
                        azul_core::dom::NodeType::Body => "Body",
                        azul_core::dom::NodeType::Div => "Div",
                        azul_core::dom::NodeType::Span => "Span",
                        azul_core::dom::NodeType::P => "P",
                        azul_core::dom::NodeType::Text(_) => "Text",
                        azul_core::dom::NodeType::Image(_) => "Image",
                        azul_core::dom::NodeType::IFrame(_) => "IFrame",
                        _ => "Other",
                    };
                    
                    let text_content = match data.get_node_type() {
                        azul_core::dom::NodeType::Text(t) => {
                            let s = t.as_str();
                            if s.len() > 50 { Some(format!("{}...", &s[..47])) } else { Some(s.to_string()) }
                        }
                        _ => None,
                    };
                    
                    let parent_decoded = if hier.parent == 0 { -1i64 } else { (hier.parent - 1) as i64 };
                    let prev_sib_decoded = if hier.previous_sibling == 0 { -1i64 } else { (hier.previous_sibling - 1) as i64 };
                    let next_sib_decoded = if hier.next_sibling == 0 { -1i64 } else { (hier.next_sibling - 1) as i64 };
                    let last_child_decoded = if hier.last_child == 0 { -1i64 } else { (hier.last_child - 1) as i64 };
                    let children: Vec<usize> = node_id.az_children(&hierarchy).map(|c| c.index()).collect();
                    
                    nodes.push(HierarchyNodeInfo {
                        index: i,
                        node_type: node_type.to_string(),
                        text: text_content,
                        parent: parent_decoded,
                        prev_sibling: prev_sib_decoded,
                        next_sibling: next_sib_decoded,
                        last_child: last_child_decoded,
                        children,
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
            log(LogLevel::Debug, LogCategory::DebugServer, "Getting layout tree", None);
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
            log(LogLevel::Debug, LogCategory::DebugServer, "Getting display list", None);
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
                
                let mut items = Vec::new();
                
                for (idx, item) in items_list.iter().enumerate() {
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
                            }
                        }
                        azul_layout::solver3::display_list::DisplayListItem::Border { bounds, .. } => {
                            border_count += 1;
                            DisplayListItemInfo {
                                index: idx,
                                item_type: "border".to_string(),
                                x: Some(bounds.origin.x),
                                y: Some(bounds.origin.y),
                                width: Some(bounds.size.width),
                                height: Some(bounds.size.height),
                                color: None,
                                font_size: None,
                                glyph_count: None,
                                z_index: None,
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
                            }
                        }
                    };
                    items.push(info);
                }
                
                let response = DisplayListResponse {
                    total_items: items_list.len(),
                    rect_count,
                    text_count,
                    border_count,
                    image_count,
                    other_count,
                    items,
                };
                send_ok(request, None, Some(ResponseData::DisplayList(response)));
            } else {
                send_err(request, "No layout result for DOM 0");
            }
        }

        DebugEvent::GetScrollStates => {
            log(LogLevel::Debug, LogCategory::DebugServer, "Getting scroll states", None);
            use azul_core::dom::DomId;
            
            let dom_id = DomId { inner: 0 };
            let layout_window = callback_info.get_layout_window();
            
            // Get scroll states from the scroll manager
            let scroll_states = layout_window.scroll_manager.get_scroll_states_for_dom(dom_id);
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
            log(LogLevel::Debug, LogCategory::DebugServer, "Getting scrollable nodes", None);
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

        DebugEvent::ScrollToNode { node_id, x, y } => {
            log(LogLevel::Debug, LogCategory::DebugServer, format!("Scrolling node {} to ({}, {})", node_id, x, y), None);
            use azul_core::dom::DomId;
            use azul_core::id::NodeId;
            use azul_core::styled_dom::NodeHierarchyItemId;
            use azul_core::geom::LogicalPosition;
            
            let dom_id = DomId { inner: 0 };
            let node = NodeId::from_usize(*node_id as usize);
            let hierarchy_id = NodeHierarchyItemId::from(node);
            
            callback_info.scroll_to(dom_id, hierarchy_id, LogicalPosition { x: *x, y: *y });
            needs_update = true;
            
            let response = ScrollToNodeResponse {
                scrolled: true,
                node_id: *node_id,
                x: *x,
                y: *y,
            };
            send_ok(request, None, Some(ResponseData::ScrollToNode(response)));
        }

        DebugEvent::FindNodeByClass { class_name } => {
            log(LogLevel::Debug, LogCategory::DebugServer, format!("Finding node by class: {}", class_name), None);
            use azul_core::dom::{DomId, DomNodeId};
            use azul_core::id::NodeId;
            
            let dom_id = DomId { inner: 0 };
            let layout_window = callback_info.get_layout_window();
            
            if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
                let styled_dom = &layout_result.styled_dom;
                let node_count = styled_dom.node_data.len();
                
                let mut found_node = None;
                for i in 0..node_count {
                    let dom_node_id = DomNodeId {
                        dom: dom_id.clone(),
                        node: NodeId::from_usize(i).into(),
                    };
                    
                    let classes = callback_info.get_node_classes(dom_node_id);
                    for class in classes.as_ref().iter() {
                        if class.as_str() == class_name.as_str() {
                            found_node = Some((i, dom_node_id));
                            break;
                        }
                    }
                    if found_node.is_some() {
                        break;
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
                        classes: Some(classes.as_ref().iter().map(|s| s.as_str().to_string()).collect()),
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

        DebugEvent::FindNodeByText { text } => {
            log(LogLevel::Debug, LogCategory::DebugServer, format!("Finding node by text: {}", text), None);
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
                                node: NodeId::from_usize(i).into(),
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
                        classes: Some(classes.as_ref().iter().map(|s| s.as_str().to_string()).collect()),
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
            log(LogLevel::Debug, LogCategory::DebugServer, format!("Clicking node {} with button {:?}", node_id, button), None);
            use azul_core::dom::{DomId, DomNodeId};
            use azul_core::id::NodeId;
            
            let dom_id = DomId { inner: 0 };
            let dom_node_id = DomNodeId {
                dom: dom_id.clone(),
                node: NodeId::from_usize(*node_id as usize).into(),
            };
            
            // Get the node's rect to find the center position
            if let Some(rect) = callback_info.get_node_rect(dom_node_id) {
                let center_x = rect.origin.x + rect.size.width / 2.0;
                let center_y = rect.origin.y + rect.size.height / 2.0;
                
                // Simulate click at the center of the node
                let mut new_state = callback_info.get_current_window_state().clone();
                new_state.mouse_state.cursor_position = azul_core::window::CursorPosition::InWindow(
                    LogicalPosition { x: center_x, y: center_y }
                );
                
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

/// Create a Timer for the debug server polling
#[cfg(feature = "std")]
pub fn create_debug_timer(
    get_system_time_fn: azul_core::task::GetSystemTimeCallback,
) -> azul_layout::timer::Timer {
    use azul_core::refany::RefAny;
    use azul_core::task::Duration;
    use azul_layout::timer::{Timer, TimerCallback};

    let timer_data = RefAny::new(());

    Timer::create(
        timer_data,
        TimerCallback::create(debug_timer_callback),
        get_system_time_fn,
    )
    .with_interval(Duration::System(azul_core::task::SystemTimeDiff::from_millis(16)))
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
