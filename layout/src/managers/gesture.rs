//! Gesture and Drag Manager
//!
//! This module manages multi-frame gestures and drag operations that span multiple
//! events and can't be handled by single-frame event callbacks.
//!
//! ## Architecture
//!
//! The manager collects raw input samples (mouse positions with timestamps) and
//! analyzes them to detect gestures. It's designed to be:
//!
//! - **Testable**: Uses injected timestamps (via ExternalSystemCallbacks) for easy mocking
//! - **Stateless queries**: Event loop queries "was this a drag?" without mutating state
//! - **Timer-driven**: Long presses use system timers to invoke callbacks periodically
//! - **Auto-clearing**: Old samples are automatically cleared after timeout
//!
//! ## Usage Flow
//!
//! 1. **Input Recording**: On mouse down, start recording samples
//! 2. **Timer Setup**: Start a system timer that periodically updates the manager
//! 3. **Query Detection**: Event loop queries manager for detected gestures
//! 4. **State Activation**: If gesture detected, promote to active drag/gesture state
//! 5. **Cleanup**: On mouse up or timeout, clear samples
//!
//! ## Supported Gestures
//!
//! - **Drag**: Mouse moved while button pressed beyond threshold
//! - **Double-click**: Two clicks within time/distance threshold
//! - **Long press**: Button held down for extended time
//! - **Swipe**: Fast directional movement (future: touch gestures)
//! - **Pinch/Rotate**: Multi-touch gestures (future, groundwork laid)

use alloc::{collections::btree_map::BTreeMap, vec::Vec};
#[cfg(feature = "std")]
use std::sync::atomic::{AtomicU64, Ordering};

use azul_core::{
    dom::{DomId, NodeId},
    geom::LogicalPosition,
    task::{Duration as CoreDuration, Instant as CoreInstant},
};

#[cfg(feature = "std")]
static NEXT_EVENT_ID: AtomicU64 = AtomicU64::new(1);

/// Allocate a new unique event ID
#[cfg(feature = "std")]
pub fn allocate_event_id() -> u64 {
    NEXT_EVENT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Allocate a new unique event ID (no_std fallback - returns 0)
#[cfg(not(feature = "std"))]
pub fn allocate_event_id() -> u64 {
    0 // In no_std environments, event IDs are not available
}

/// Helper function to convert CoreDuration to milliseconds
///
/// CoreDuration is an enum with System (std::time::Duration) and Tick variants.
/// We need to handle both cases for proper time calculations.
fn duration_to_millis(duration: CoreDuration) -> u64 {
    match duration {
        #[cfg(feature = "std")]
        CoreDuration::System(system_diff) => {
            let std_duration: std::time::Duration = system_diff.into();
            std_duration.as_millis() as u64
        }
        #[cfg(not(feature = "std"))]
        CoreDuration::System(system_diff) => {
            // Manual calculation: secs * 1000 + nanos / 1_000_000
            system_diff.secs * 1000 + (system_diff.nanos / 1_000_000) as u64
        }
        CoreDuration::Tick(tick_diff) => {
            // Assume tick = 1ms for simplicity (platform-specific)
            tick_diff.tick_diff
        }
    }
}

/// Maximum number of input samples to keep in memory
///
/// This prevents unbounded memory growth during long drags.
/// Older samples beyond this limit are automatically discarded.
pub const MAX_SAMPLES_PER_SESSION: usize = 1000;

/// Default timeout for clearing old gesture samples (milliseconds)
///
/// Samples older than this are automatically removed to prevent
/// memory leaks and stale gesture detection.
pub const DEFAULT_SAMPLE_TIMEOUT_MS: u64 = 2000;

/// Configuration for gesture detection thresholds
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GestureDetectionConfig {
    /// Minimum distance (pixels) to consider movement a drag, not a click
    pub drag_distance_threshold: f32,

    /// Maximum time between clicks for double-click detection (milliseconds)
    pub double_click_time_threshold_ms: u64,

    /// Maximum distance between clicks for double-click detection (pixels)
    pub double_click_distance_threshold: f32,

    /// Minimum time to hold button for long-press detection (milliseconds)
    pub long_press_time_threshold_ms: u64,

    /// Maximum distance to move while holding for long-press (pixels)
    pub long_press_distance_threshold: f32,

    /// Minimum samples needed to detect a gesture
    pub min_samples_for_gesture: usize,

    /// Minimum velocity for swipe detection (pixels per second)
    pub swipe_velocity_threshold: f32,

    /// Minimum scale change for pinch detection (e.g., 0.1 = 10% change)
    pub pinch_scale_threshold: f32,

    /// Minimum rotation angle for rotation detection (radians)
    pub rotation_angle_threshold: f32,

    /// How often to clear old samples (milliseconds)
    pub sample_cleanup_interval_ms: u64,
}

impl Default for GestureDetectionConfig {
    fn default() -> Self {
        Self {
            drag_distance_threshold: 5.0,
            double_click_time_threshold_ms: 500,
            double_click_distance_threshold: 5.0,
            long_press_time_threshold_ms: 500,
            long_press_distance_threshold: 10.0,
            min_samples_for_gesture: 2,
            swipe_velocity_threshold: 500.0, // 500 px/s
            pinch_scale_threshold: 0.1,      // 10% scale change
            rotation_angle_threshold: 0.1,   // ~5.7 degrees in radians
            sample_cleanup_interval_ms: DEFAULT_SAMPLE_TIMEOUT_MS,
        }
    }
}

/// Single input sample with position and timestamp
#[derive(Debug, Clone, PartialEq)]
pub struct InputSample {
    /// Position in logical coordinates
    pub position: LogicalPosition,

    /// Timestamp when this sample was recorded (from ExternalSystemCallbacks)
    pub timestamp: CoreInstant,

    /// Mouse button state (bitfield: 0x01 = left, 0x02 = right, 0x04 = middle)
    pub button_state: u8,

    /// Unique, monotonic event ID for ordering (atomic counter)
    pub event_id: u64,

    /// Pen/stylus pressure (0.0 to 1.0, 0.5 = default for mouse)
    pub pressure: f32,

    /// Pen/stylus tilt angles in degrees (x_tilt, y_tilt)
    /// Range: typically -90.0 to 90.0, (0.0, 0.0) = perpendicular
    pub tilt: (f32, f32),

    /// Touch contact radius in logical pixels (width, height)
    /// For mouse input, this is (0.0, 0.0)
    pub touch_radius: (f32, f32),
}

/// A sequence of input samples forming one button press session
#[derive(Debug, Clone, PartialEq)]
pub struct InputSession {
    /// All recorded samples for this session
    pub samples: Vec<InputSample>,

    /// Whether this session has ended (button released)
    pub ended: bool,

    /// Session ID for tracking (incremental counter)
    pub session_id: u64,
}

impl InputSession {
    /// Create a new input session
    fn new(session_id: u64, first_sample: InputSample) -> Self {
        Self {
            samples: vec![first_sample],
            ended: false,
            session_id,
        }
    }

    /// Get the first sample in this session
    pub fn first_sample(&self) -> Option<&InputSample> {
        self.samples.first()
    }

    /// Get the last sample in this session
    pub fn last_sample(&self) -> Option<&InputSample> {
        self.samples.last()
    }

    /// Get the duration of this session (first to last sample)
    pub fn duration_ms(&self) -> Option<u64> {
        let first = self.first_sample()?;
        let last = self.last_sample()?;
        let duration = last.timestamp.duration_since(&first.timestamp);
        Some(duration_to_millis(duration))
    }

    /// Get the total distance traveled in this session
    pub fn total_distance(&self) -> f32 {
        if self.samples.len() < 2 {
            return 0.0;
        }

        let mut total = 0.0;
        for i in 1..self.samples.len() {
            let prev = &self.samples[i - 1];
            let curr = &self.samples[i];
            let dx = curr.position.x - prev.position.x;
            let dy = curr.position.y - prev.position.y;
            total += (dx * dx + dy * dy).sqrt();
        }
        total
    }

    /// Get the straight-line distance from first to last sample
    pub fn direct_distance(&self) -> Option<f32> {
        let first = self.first_sample()?;
        let last = self.last_sample()?;
        let dx = last.position.x - first.position.x;
        let dy = last.position.y - first.position.y;
        Some((dx * dx + dy * dy).sqrt())
    }
}

/// Result of drag detection analysis
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectedDrag {
    /// Position where drag started
    pub start_position: LogicalPosition,

    /// Current/end position of drag
    pub current_position: LogicalPosition,

    /// Direct distance dragged (straight line, pixels)
    pub direct_distance: f32,

    /// Total distance dragged (following path, pixels)
    pub total_distance: f32,

    /// Duration of the drag (milliseconds)
    pub duration_ms: u64,

    /// Number of position samples recorded
    pub sample_count: usize,

    /// Session ID this drag belongs to
    pub session_id: u64,
}

/// Result of long-press detection
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectedLongPress {
    /// Position where long press is happening
    pub position: LogicalPosition,

    /// How long the button has been held (milliseconds)
    pub duration_ms: u64,

    /// Whether the callback has already been invoked for this long press
    pub callback_invoked: bool,

    /// Session ID this long press belongs to
    pub session_id: u64,
}

/// Primary direction of a gesture
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Result of pinch gesture detection
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectedPinch {
    /// Scale factor (< 1.0 for pinch in, > 1.0 for pinch out)
    pub scale: f32,

    /// Center point of the pinch gesture
    pub center: LogicalPosition,

    /// Initial distance between touch points
    pub initial_distance: f32,

    /// Current distance between touch points
    pub current_distance: f32,

    /// Duration of pinch (milliseconds)
    pub duration_ms: u64,
}

/// Result of rotation gesture detection
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectedRotation {
    /// Rotation angle in radians (positive = clockwise)
    pub angle_radians: f32,

    /// Center point of rotation
    pub center: LogicalPosition,

    /// Duration of rotation (milliseconds)
    pub duration_ms: u64,
}

/// State of an active node drag (after detection)
#[derive(Debug, Clone, PartialEq)]
pub struct NodeDragState {
    /// DOM ID of the node being dragged
    pub dom_id: DomId,

    /// Node ID being dragged
    pub node_id: NodeId,

    /// Position where drag started
    pub start_position: LogicalPosition,

    /// Current drag position
    pub current_position: LogicalPosition,

    /// Optional: DOM node currently under cursor (drop target)
    pub current_drop_target: Option<(DomId, NodeId)>,

    /// Hit-test result at drag start (to track what's under cursor during drag)
    pub start_hit_test: Option<azul_core::hit_test::HitTest>,

    /// Drag data (MIME types and content)
    pub drag_data: DragData,

    /// Session ID this drag was promoted from
    pub session_id: u64,
}

/// State of window being dragged (titlebar drag)
#[derive(Debug, Clone, PartialEq)]
pub struct WindowDragState {
    /// Position where window drag started
    pub start_position: LogicalPosition,

    /// Current drag position
    pub current_position: LogicalPosition,

    /// Initial window position before drag
    pub initial_window_position: azul_core::window::WindowPosition,

    /// Hit-test result at drag start (e.g., to verify we're still on titlebar)
    pub start_hit_test: Option<azul_core::hit_test::HitTest>,

    /// Session ID this drag was promoted from
    pub session_id: u64,
}

/// State of file(s) being dragged from OS over the window
#[derive(Debug, Clone, PartialEq)]
pub struct FileDropState {
    /// Files being dragged (as string paths)
    pub files: Vec<azul_css::AzString>,

    /// Current position of drag cursor
    pub position: LogicalPosition,

    /// DOM node under cursor (potential drop target)
    pub drop_target: Option<(DomId, NodeId)>,

    /// Allowed drop effect
    pub drop_effect: DropEffect,
}

/// State of pen/stylus input
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PenState {
    /// Current pen position
    pub position: LogicalPosition,

    /// Current pressure (0.0 to 1.0)
    pub pressure: f32,

    /// Current tilt angles (x_tilt, y_tilt) in degrees
    pub tilt: (f32, f32),

    /// Whether pen is in contact with surface
    pub in_contact: bool,

    /// Whether pen is inverted (eraser mode)
    pub is_eraser: bool,

    /// Whether barrel button is pressed
    pub barrel_button_pressed: bool,

    /// Unique identifier for this pen device
    pub device_id: u64,
}

impl Default for PenState {
    fn default() -> Self {
        Self {
            position: LogicalPosition::zero(),
            pressure: 0.0,
            tilt: (0.0, 0.0),
            in_contact: false,
            is_eraser: false,
            barrel_button_pressed: false,
            device_id: 0,
        }
    }
}

/// Drop effect (what happens when dropped)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropEffect {
    /// No effect
    None,

    /// Copy the data
    Copy,

    /// Move the data
    Move,

    /// Create link
    Link,
}

/// Drag data (like HTML5 DataTransfer)
#[derive(Debug, Clone, PartialEq)]
pub struct DragData {
    /// MIME type -> data mapping
    /// e.g., "text/plain" -> "Hello World"
    pub data: BTreeMap<azul_css::AzString, Vec<u8>>,

    /// Allowed drag operations
    pub effect_allowed: DragEffect,
}

/// Drag/drop effect (like HTML5 dropEffect)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragEffect {
    /// No drop allowed
    None,

    /// Copy operation
    Copy,

    /// Move operation
    Move,

    /// Link/shortcut operation
    Link,
}

impl Default for DragData {
    fn default() -> Self {
        Self::new()
    }
}

impl DragData {
    /// Create new empty drag data
    pub fn new() -> Self {
        Self {
            data: BTreeMap::new(),
            effect_allowed: DragEffect::Copy,
        }
    }

    /// Set data for a MIME type
    pub fn set_data(&mut self, mime_type: impl Into<azul_css::AzString>, data: Vec<u8>) {
        self.data.insert(mime_type.into(), data);
    }

    /// Get data for a MIME type
    pub fn get_data(&self, mime_type: &str) -> Option<&[u8]> {
        self.data
            .get(&azul_css::AzString::from(mime_type))
            .map(|v| v.as_slice())
    }

    /// Set plain text data
    pub fn set_text(&mut self, text: impl Into<azul_css::AzString>) {
        let text_str = text.into();
        self.set_data("text/plain", text_str.as_str().as_bytes().to_vec());
    }

    /// Get plain text data
    pub fn get_text(&self) -> Option<azul_css::AzString> {
        self.get_data("text/plain")
            .map(|bytes| azul_css::AzString::from(core::str::from_utf8(bytes).unwrap_or("")))
    }
}

/// Manager for multi-frame gestures and drag operations
///
/// This collects raw input samples and analyzes them to detect gestures.
/// Designed for testability and clear separation of input collection vs. detection.
#[derive(Debug, Clone, PartialEq)]
pub struct GestureAndDragManager {
    /// Configuration for gesture detection
    pub config: GestureDetectionConfig,

    /// All recorded input sessions (multiple button press sequences)
    pub input_sessions: Vec<InputSession>,

    /// Counter for generating unique session IDs
    next_session_id: u64,

    /// Current active node drag state (after promotion from detected drag)
    pub node_drag: Option<NodeDragState>,

    /// Current window drag state (titlebar drag)
    pub window_drag: Option<WindowDragState>,

    /// Current file drag/drop state (from OS)
    pub file_drop: Option<FileDropState>,

    /// Current pen/stylus state
    pub pen_state: Option<PenState>,

    /// Session IDs where long press callback has been invoked
    long_press_callbacks_invoked: Vec<u64>,
}

impl Default for GestureAndDragManager {
    fn default() -> Self {
        Self::new()
    }
}

impl GestureAndDragManager {
    /// Create a new gesture and drag manager
    pub fn new() -> Self {
        Self {
            config: GestureDetectionConfig::default(),
            input_sessions: Vec::new(),
            next_session_id: 1,
            node_drag: None,
            window_drag: None,
            file_drop: None,
            pen_state: None,
            long_press_callbacks_invoked: Vec::new(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: GestureDetectionConfig) -> Self {
        Self {
            config,
            ..Self::new()
        }
    }

    // Input Recording Methods (called from event loop / system timer)

    /// Start a new input session (mouse button pressed down)
    ///
    /// This begins recording samples for gesture detection.
    /// Call this when receiving mouse button down event.
    ///
    /// Returns the session ID for this new session.
    pub fn start_input_session(
        &mut self,
        position: LogicalPosition,
        timestamp: CoreInstant,
        button_state: u8,
    ) -> u64 {
        self.start_input_session_with_pen(
            position,
            timestamp,
            button_state,
            allocate_event_id(),
            0.5,        // default pressure for mouse
            (0.0, 0.0), // no tilt for mouse
            (0.0, 0.0), // no touch radius for mouse
        )
    }

    /// Start a new input session with pen/touch data
    pub fn start_input_session_with_pen(
        &mut self,
        position: LogicalPosition,
        timestamp: CoreInstant,
        button_state: u8,
        event_id: u64,
        pressure: f32,
        tilt: (f32, f32),
        touch_radius: (f32, f32),
    ) -> u64 {
        let session_id = self.next_session_id;
        self.next_session_id += 1;

        let sample = InputSample {
            position,
            timestamp,
            button_state,
            event_id,
            pressure,
            tilt,
            touch_radius,
        };

        let session = InputSession::new(session_id, sample);
        self.input_sessions.push(session);

        session_id
    }

    /// Record an input sample to the current session
    ///
    /// Call this on every mouse move event while button is pressed,
    /// and also periodically from a system timer to track long presses.
    ///
    /// Returns true if sample was recorded, false if no active session.
    pub fn record_input_sample(
        &mut self,
        position: LogicalPosition,
        timestamp: CoreInstant,
        button_state: u8,
    ) -> bool {
        self.record_input_sample_with_pen(
            position,
            timestamp,
            button_state,
            allocate_event_id(),
            0.5,        // default pressure for mouse
            (0.0, 0.0), // no tilt for mouse
            (0.0, 0.0), // no touch radius for mouse
        )
    }

    /// Record an input sample with pen/touch data
    pub fn record_input_sample_with_pen(
        &mut self,
        position: LogicalPosition,
        timestamp: CoreInstant,
        button_state: u8,
        event_id: u64,
        pressure: f32,
        tilt: (f32, f32),
        touch_radius: (f32, f32),
    ) -> bool {
        if let Some(session) = self.input_sessions.last_mut() {
            if !session.ended {
                // Enforce max samples limit
                if session.samples.len() >= MAX_SAMPLES_PER_SESSION {
                    // Remove oldest samples, keeping the most recent ones
                    let remove_count = session.samples.len() - MAX_SAMPLES_PER_SESSION + 100;
                    session.samples.drain(0..remove_count);
                }

                session.samples.push(InputSample {
                    position,
                    timestamp,
                    button_state,
                    event_id,
                    pressure,
                    tilt,
                    touch_radius,
                });
                return true;
            }
        }
        false
    }

    /// End the current input session (mouse button released)
    ///
    /// Call this when receiving mouse button up event.
    /// The session is kept for analysis but marked as ended.
    pub fn end_current_session(&mut self) {
        if let Some(session) = self.input_sessions.last_mut() {
            session.ended = true;
        }
    }

    /// Clear old input sessions that have timed out
    ///
    /// Call this periodically (e.g., every frame) to prevent memory leaks.
    /// Sessions older than `config.sample_cleanup_interval_ms` are removed.
    pub fn clear_old_sessions(&mut self, current_time: CoreInstant) {
        self.input_sessions.retain(|session| {
            if let Some(last_sample) = session.last_sample() {
                let duration = current_time.duration_since(&last_sample.timestamp);
                let age_ms = duration_to_millis(duration);
                age_ms < self.config.sample_cleanup_interval_ms
            } else {
                false
            }
        });

        // Also clear long press callback tracking for removed sessions
        let valid_session_ids: Vec<u64> =
            self.input_sessions.iter().map(|s| s.session_id).collect();
        self.long_press_callbacks_invoked
            .retain(|id| valid_session_ids.contains(id));
    }

    /// Clear all input sessions
    ///
    /// Call this when you want to reset all gesture detection state.
    pub fn clear_all_sessions(&mut self) {
        self.input_sessions.clear();
        self.long_press_callbacks_invoked.clear();
    }

    /// Update pen/stylus state
    ///
    /// Call this when receiving pen events from the platform.
    pub fn update_pen_state(
        &mut self,
        position: LogicalPosition,
        pressure: f32,
        tilt: (f32, f32),
        in_contact: bool,
        is_eraser: bool,
        barrel_button_pressed: bool,
        device_id: u64,
    ) {
        self.pen_state = Some(PenState {
            position,
            pressure,
            tilt,
            in_contact,
            is_eraser,
            barrel_button_pressed,
            device_id,
        });
    }

    /// Clear pen state (when pen leaves proximity)
    pub fn clear_pen_state(&mut self) {
        self.pen_state = None;
    }

    /// Get current pen state (read-only)
    pub fn get_pen_state(&self) -> Option<&PenState> {
        self.pen_state.as_ref()
    }

    // Gesture Detection Methods (query state without mutation)

    /// Detect if current input represents a drag gesture
    ///
    /// Returns Some(DetectedDrag) if a drag is detected based on distance threshold.
    pub fn detect_drag(&self) -> Option<DetectedDrag> {
        let session = self.get_current_session()?;

        if session.samples.len() < self.config.min_samples_for_gesture {
            return None;
        }

        let direct_distance = session.direct_distance()?;

        if direct_distance >= self.config.drag_distance_threshold {
            let first = session.first_sample()?;
            let last = session.last_sample()?;

            Some(DetectedDrag {
                start_position: first.position,
                current_position: last.position,
                direct_distance,
                total_distance: session.total_distance(),
                duration_ms: session.duration_ms()?,
                sample_count: session.samples.len(),
                session_id: session.session_id,
            })
        } else {
            None
        }
    }

    /// Detect if current input represents a long press
    ///
    /// Returns Some(DetectedLongPress) if button has been held long enough
    /// without moving much.
    pub fn detect_long_press(&self) -> Option<DetectedLongPress> {
        let session = self.get_current_session()?;

        if session.ended {
            return None; // Can't be long press if button already released
        }

        let duration_ms = session.duration_ms()?;

        if duration_ms < self.config.long_press_time_threshold_ms {
            return None;
        }

        let distance = session.direct_distance()?;

        if distance <= self.config.long_press_distance_threshold {
            let first = session.first_sample()?;
            let callback_invoked = self
                .long_press_callbacks_invoked
                .contains(&session.session_id);

            Some(DetectedLongPress {
                position: first.position,
                duration_ms,
                callback_invoked,
                session_id: session.session_id,
            })
        } else {
            None
        }
    }

    /// Mark long press callback as invoked for a session
    ///
    /// Call this after invoking the long press callback to prevent repeated invocations.
    pub fn mark_long_press_callback_invoked(&mut self, session_id: u64) {
        if !self.long_press_callbacks_invoked.contains(&session_id) {
            self.long_press_callbacks_invoked.push(session_id);
        }
    }

    /// Detect if last two sessions form a double-click
    ///
    /// Returns true if timing and distance match double-click criteria.
    pub fn detect_double_click(&self) -> bool {
        if self.input_sessions.len() < 2 {
            return false;
        }

        let prev_session = &self.input_sessions[self.input_sessions.len() - 2];
        let last_session = &self.input_sessions[self.input_sessions.len() - 1];

        // Both sessions must have ended (button released)
        if !prev_session.ended || !last_session.ended {
            return false;
        }

        let prev_first = match prev_session.first_sample() {
            Some(s) => s,
            None => return false,
        };

        let last_first = match last_session.first_sample() {
            Some(s) => s,
            None => return false,
        };

        let duration = last_first.timestamp.duration_since(&prev_first.timestamp);
        let time_delta_ms = duration_to_millis(duration);

        if time_delta_ms > self.config.double_click_time_threshold_ms {
            return false;
        }

        let dx = last_first.position.x - prev_first.position.x;
        let dy = last_first.position.y - prev_first.position.y;
        let distance = (dx * dx + dy * dy).sqrt();

        distance < self.config.double_click_distance_threshold
    }

    /// Get the primary direction of current drag
    pub fn get_drag_direction(&self) -> Option<GestureDirection> {
        let session = self.get_current_session()?;
        let first = session.first_sample()?;
        let last = session.last_sample()?;

        let dx = last.position.x - first.position.x;
        let dy = last.position.y - first.position.y;

        if dx.abs() > dy.abs() {
            if dx > 0.0 {
                Some(GestureDirection::Right)
            } else {
                Some(GestureDirection::Left)
            }
        } else {
            if dy > 0.0 {
                Some(GestureDirection::Down)
            } else {
                Some(GestureDirection::Up)
            }
        }
    }

    /// Get average velocity of current gesture (pixels per second)
    pub fn get_gesture_velocity(&self) -> Option<f32> {
        let session = self.get_current_session()?;

        if session.samples.len() < 2 {
            return None;
        }

        let total_distance = session.total_distance();
        let duration_ms = session.duration_ms()?;

        if duration_ms == 0 {
            return None;
        }

        let duration_secs = duration_ms as f32 / 1000.0;
        Some(total_distance / duration_secs)
    }

    /// Check if current gesture is a swipe (fast directional movement)
    pub fn is_swipe(&self) -> bool {
        if let Some(velocity) = self.get_gesture_velocity() {
            velocity >= self.config.swipe_velocity_threshold
        } else {
            false
        }
    }

    /// Detect swipe with specific direction
    ///
    /// Returns Some(direction) if gesture is a fast swipe in a clear direction
    pub fn detect_swipe_direction(&self) -> Option<GestureDirection> {
        // Must be a fast swipe first
        if !self.is_swipe() {
            return None;
        }

        // Get direction
        self.get_drag_direction()
    }

    /// Detect pinch gesture (two-touch zoom in/out)
    ///
    /// Returns Some if two touch points are active and distance is changing
    /// significantly. Scale < 1.0 = pinch in, scale > 1.0 = pinch out.
    pub fn detect_pinch(&self) -> Option<DetectedPinch> {
        // Need at least two active sessions for pinch
        if self.input_sessions.len() < 2 {
            return None;
        }

        // Get last two sessions (most recent touches)
        let session1 = &self.input_sessions[self.input_sessions.len() - 2];
        let session2 = &self.input_sessions[self.input_sessions.len() - 1];

        // Both must have samples
        let first1 = session1.first_sample()?;
        let first2 = session2.first_sample()?;
        let last1 = session1.last_sample()?;
        let last2 = session2.last_sample()?;

        // Calculate initial distance between touches
        let dx_initial = first2.position.x - first1.position.x;
        let dy_initial = first2.position.y - first1.position.y;
        let initial_distance = (dx_initial * dx_initial + dy_initial * dy_initial).sqrt();

        // Calculate current distance
        let dx_current = last2.position.x - last1.position.x;
        let dy_current = last2.position.y - last1.position.y;
        let current_distance = (dx_current * dx_current + dy_current * dy_current).sqrt();

        // Avoid division by zero
        if initial_distance < 1.0 {
            return None;
        }

        // Calculate scale factor
        let scale = current_distance / initial_distance;

        // Check if scale change is significant (threshold from config)
        let scale_threshold = 1.0 + self.config.pinch_scale_threshold;
        if scale > 1.0 / scale_threshold && scale < scale_threshold {
            return None; // Change too small
        }

        // Calculate center point
        let center = LogicalPosition {
            x: (last1.position.x + last2.position.x) / 2.0,
            y: (last1.position.y + last2.position.y) / 2.0,
        };

        // Calculate duration
        let duration = last1.timestamp.duration_since(&first1.timestamp);
        let duration_ms = duration_to_millis(duration);

        Some(DetectedPinch {
            scale,
            center,
            initial_distance,
            current_distance,
            duration_ms,
        })
    }

    /// Detect rotation gesture (two-touch rotate)
    ///
    /// Returns Some if two touch points are rotating around center.
    /// Positive angle = clockwise, negative = counterclockwise.
    pub fn detect_rotation(&self) -> Option<DetectedRotation> {
        // Need at least two active sessions
        if self.input_sessions.len() < 2 {
            return None;
        }

        // Get last two sessions
        let session1 = &self.input_sessions[self.input_sessions.len() - 2];
        let session2 = &self.input_sessions[self.input_sessions.len() - 1];

        // Both must have samples
        let first1 = session1.first_sample()?;
        let first2 = session2.first_sample()?;
        let last1 = session1.last_sample()?;
        let last2 = session2.last_sample()?;

        // Calculate center (average of both touches)
        let center = LogicalPosition {
            x: (last1.position.x + last2.position.x) / 2.0,
            y: (last1.position.y + last2.position.y) / 2.0,
        };

        // Calculate initial angle between touches
        let dx_initial = first2.position.x - first1.position.x;
        let dy_initial = first2.position.y - first1.position.y;
        let initial_angle = dy_initial.atan2(dx_initial);

        // Calculate current angle
        let dx_current = last2.position.x - last1.position.x;
        let dy_current = last2.position.y - last1.position.y;
        let current_angle = dy_current.atan2(dx_current);

        // Calculate angle difference (normalized to -π to π)
        let mut angle_diff = current_angle - initial_angle;

        // Normalize angle to -π to π range
        const PI: f32 = core::f32::consts::PI;
        while angle_diff > PI {
            angle_diff -= 2.0 * PI;
        }
        while angle_diff < -PI {
            angle_diff += 2.0 * PI;
        }

        // Check if rotation is significant (threshold from config)
        if angle_diff.abs() < self.config.rotation_angle_threshold {
            return None;
        }

        // Calculate duration
        let duration = last1.timestamp.duration_since(&first1.timestamp);
        let duration_ms = duration_to_millis(duration);

        Some(DetectedRotation {
            angle_radians: angle_diff,
            center,
            duration_ms,
        })
    }

    /// Get the current active input session (if any)
    pub fn get_current_session(&self) -> Option<&InputSession> {
        self.input_sessions.last()
    }

    /// Get current mouse position from latest sample
    pub fn get_current_mouse_position(&self) -> Option<LogicalPosition> {
        self.get_current_session()
            .and_then(|s| s.last_sample())
            .map(|sample| sample.position)
    }

    // Active State Management (promote detected gestures to active state)

    /// Promote detected drag to active node drag
    ///
    /// Call this after detect_drag() returns Some and you've determined
    /// which DOM node is being dragged.
    pub fn activate_node_drag(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        drag_data: DragData,
        start_hit_test: Option<azul_core::hit_test::HitTest>,
    ) {
        if let Some(detected) = self.detect_drag() {
            self.node_drag = Some(NodeDragState {
                dom_id,
                node_id,
                start_position: detected.start_position,
                current_position: detected.current_position,
                current_drop_target: None,
                start_hit_test,
                drag_data,
                session_id: detected.session_id,
            });
        }
    }

    /// Promote detected drag to window drag (titlebar)
    pub fn activate_window_drag(
        &mut self,
        initial_window_position: azul_core::window::WindowPosition,
        start_hit_test: Option<azul_core::hit_test::HitTest>,
    ) {
        if let Some(detected) = self.detect_drag() {
            self.window_drag = Some(WindowDragState {
                start_position: detected.start_position,
                current_position: detected.current_position,
                initial_window_position,
                start_hit_test,
                session_id: detected.session_id,
            });
        }
    }

    /// Update positions for active drags (call on mouse move)
    pub fn update_active_drag_positions(&mut self, position: LogicalPosition) {
        if let Some(ref mut node_drag) = self.node_drag {
            node_drag.current_position = position;
        }

        if let Some(ref mut window_drag) = self.window_drag {
            window_drag.current_position = position;
        }

        if let Some(ref mut file_drop) = self.file_drop {
            file_drop.position = position;
        }
    }

    /// Update drop target for node drag
    pub fn update_node_drag_target(&mut self, target: Option<(DomId, NodeId)>) {
        if let Some(ref mut node_drag) = self.node_drag {
            node_drag.current_drop_target = target;
        }
    }

    /// Update hit-test for active node drag
    pub fn update_node_drag_hit_test(&mut self, hit_test: Option<azul_core::hit_test::HitTest>) {
        if let Some(ref mut node_drag) = self.node_drag {
            node_drag.start_hit_test = hit_test;
        }
    }

    /// Update drop target for file drop
    pub fn update_file_drop_target(&mut self, target: Option<(DomId, NodeId)>) {
        if let Some(ref mut file_drop) = self.file_drop {
            file_drop.drop_target = target;
        }
    }

    /// Update hit-test for active window drag
    pub fn update_window_drag_hit_test(&mut self, hit_test: Option<azul_core::hit_test::HitTest>) {
        if let Some(ref mut window_drag) = self.window_drag {
            window_drag.start_hit_test = hit_test;
        }
    }

    /// End node drag (returns final state for drop event generation)
    pub fn end_node_drag(&mut self) -> Option<NodeDragState> {
        self.node_drag.take()
    }

    /// End window drag
    pub fn end_window_drag(&mut self) -> Option<WindowDragState> {
        self.window_drag.take()
    }

    /// Start file drop from OS
    pub fn start_file_drop(&mut self, files: Vec<azul_css::AzString>, position: LogicalPosition) {
        self.file_drop = Some(FileDropState {
            files,
            position,
            drop_target: None,
            drop_effect: DropEffect::Copy,
        });
    }

    /// End file drop (returns final state for file handling)
    pub fn end_file_drop(&mut self) -> Option<FileDropState> {
        self.file_drop.take()
    }

    /// Cancel file drop (drag left window)
    pub fn cancel_file_drop(&mut self) {
        self.file_drop = None;
    }

    // Query Methods (read-only access for callbacks)

    /// Check if any drag operation is in progress
    pub fn is_dragging(&self) -> bool {
        self.node_drag.is_some() || self.window_drag.is_some()
    }

    /// Get current node drag state (if any)
    pub fn get_node_drag(&self) -> Option<&NodeDragState> {
        self.node_drag.as_ref()
    }

    /// Get current window drag state (if any)
    pub fn get_window_drag(&self) -> Option<&WindowDragState> {
        self.window_drag.as_ref()
    }

    /// Get current file drop state (if any)
    pub fn get_file_drop(&self) -> Option<&FileDropState> {
        self.file_drop.as_ref()
    }

    /// Check if a specific node is being dragged
    pub fn is_node_dragging(&self, dom_id: DomId, node_id: NodeId) -> bool {
        self.node_drag
            .as_ref()
            .map(|d| d.dom_id == dom_id && d.node_id == node_id)
            .unwrap_or(false)
    }

    /// Get number of active input sessions
    pub fn session_count(&self) -> usize {
        self.input_sessions.len()
    }

    /// Get current session ID (if any)
    pub fn current_session_id(&self) -> Option<u64> {
        self.get_current_session().map(|s| s.session_id)
    }

    // Window Drag Helper Methods

    /// Calculate window position delta from current drag state
    ///
    /// Returns (delta_x, delta_y) to apply to window position.
    /// Returns None if no window drag is active or drag hasn't moved.
    pub fn get_window_drag_delta(&self) -> Option<(i32, i32)> {
        let drag = self.window_drag.as_ref()?;

        let delta_x = drag.current_position.x - drag.start_position.x;
        let delta_y = drag.current_position.y - drag.start_position.y;

        // Apply to initial window position
        match drag.initial_window_position {
            azul_core::window::WindowPosition::Initialized(initial_pos) => {
                Some((delta_x as i32, delta_y as i32))
            }
            _ => None,
        }
    }

    /// Get the new window position based on current drag
    ///
    /// Returns the absolute window position to set.
    pub fn get_window_position_from_drag(&self) -> Option<azul_core::window::WindowPosition> {
        use azul_core::geom::PhysicalPositionI32;

        let drag = self.window_drag.as_ref()?;

        let delta_x = drag.current_position.x - drag.start_position.x;
        let delta_y = drag.current_position.y - drag.start_position.y;

        match drag.initial_window_position {
            azul_core::window::WindowPosition::Initialized(initial_pos) => Some(
                azul_core::window::WindowPosition::Initialized(PhysicalPositionI32::new(
                    initial_pos.x + delta_x as i32,
                    initial_pos.y + delta_y as i32,
                )),
            ),
            _ => None,
        }
    }

    /// Check if window drag is active
    pub fn is_window_dragging(&self) -> bool {
        self.window_drag.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_instant(millis: u64) -> CoreInstant {
        // For testing, we can use the milliseconds as ticks
        CoreInstant::Tick(azul_core::task::SystemTick {
            tick_counter: millis,
        })
    }

    #[test]
    fn test_drag_detection() {
        let mut manager = GestureAndDragManager::new();

        // Start session at (0, 0)
        manager.start_input_session(LogicalPosition::new(0.0, 0.0), make_instant(0), 0x01);

        // Not a drag yet (distance too small)
        assert!(manager.detect_drag().is_none());

        // Move to (10, 10) - should be detected as drag
        manager.record_input_sample(LogicalPosition::new(10.0, 10.0), make_instant(100), 0x01);

        let drag = manager.detect_drag().unwrap();
        assert_eq!(drag.start_position, LogicalPosition::new(0.0, 0.0));
        assert_eq!(drag.current_position, LogicalPosition::new(10.0, 10.0));
        assert!(drag.direct_distance > 14.0); // sqrt(10^2 + 10^2) ≈ 14.14
        assert_eq!(drag.duration_ms, 100);
    }

    #[test]
    fn test_double_click_detection() {
        let mut manager = GestureAndDragManager::new();

        // First click at (5, 5)
        manager.start_input_session(LogicalPosition::new(5.0, 5.0), make_instant(0), 0x01);
        manager.end_current_session();

        // Second click at (6, 6) after 200ms - should be double click
        manager.start_input_session(LogicalPosition::new(6.0, 6.0), make_instant(200), 0x01);
        manager.end_current_session(); // Need to end the second session too

        assert!(manager.detect_double_click());
    }

    #[test]
    fn test_long_press_detection() {
        let mut manager = GestureAndDragManager::new();

        // Start press at (10, 10)
        manager.start_input_session(LogicalPosition::new(10.0, 10.0), make_instant(0), 0x01);

        // Record another sample after 600ms (exceeds threshold)
        manager.record_input_sample(LogicalPosition::new(10.0, 10.0), make_instant(600), 0x01);

        let long_press = manager.detect_long_press().unwrap();
        assert_eq!(long_press.position, LogicalPosition::new(10.0, 10.0));
        assert_eq!(long_press.duration_ms, 600);
        assert!(!long_press.callback_invoked);

        // Mark callback as invoked
        manager.mark_long_press_callback_invoked(long_press.session_id);

        let long_press2 = manager.detect_long_press().unwrap();
        assert!(long_press2.callback_invoked);
    }

    #[test]
    fn test_session_cleanup() {
        let mut manager = GestureAndDragManager::new();

        // Create an old session
        manager.start_input_session(LogicalPosition::new(0.0, 0.0), make_instant(0), 0x01);
        manager.end_current_session();

        assert_eq!(manager.session_count(), 1);

        // Clear sessions that are older than 2000ms
        manager.clear_old_sessions(make_instant(3000));

        assert_eq!(manager.session_count(), 0);
    }

    #[test]
    fn test_activate_node_drag_with_hit_test() {
        use alloc::collections::BTreeMap;

        use azul_core::hit_test::{HitTest, HitTestItem};

        let mut manager = GestureAndDragManager::new();

        // Start drag session
        manager.start_input_session(LogicalPosition::new(0.0, 0.0), make_instant(0), 0x01);
        manager.record_input_sample(LogicalPosition::new(15.0, 15.0), make_instant(100), 0x01);

        // Verify drag is detected
        assert!(manager.detect_drag().is_some());

        // Create mock hit test
        let mut hit_test = HitTest {
            regular_hit_test_nodes: BTreeMap::new(),
            scroll_hit_test_nodes: BTreeMap::new(),
            scrollbar_hit_test_nodes: BTreeMap::new(),
        };

        hit_test.regular_hit_test_nodes.insert(
            NodeId::new(1),
            HitTestItem {
                point_in_viewport: LogicalPosition::new(0.0, 0.0),
                point_relative_to_item: LogicalPosition::new(0.0, 0.0),
                is_focusable: false,
                is_iframe_hit: None,
            },
        );

        // Activate node drag with hit test
        let drag_data = DragData {
            data: BTreeMap::new(),
            effect_allowed: DragEffect::Copy,
        };

        manager.activate_node_drag(
            DomId { inner: 0 },
            NodeId::new(1),
            drag_data,
            Some(hit_test.clone()),
        );

        // Verify node drag was activated with hit test
        let node_drag = manager.get_node_drag().unwrap();
        assert_eq!(node_drag.dom_id, DomId { inner: 0 });
        assert_eq!(node_drag.node_id, NodeId::new(1));
        assert!(node_drag.start_hit_test.is_some());

        let saved_hit_test = node_drag.start_hit_test.as_ref().unwrap();
        assert_eq!(saved_hit_test.regular_hit_test_nodes.len(), 1);
    }

    #[test]
    fn test_activate_window_drag_with_hit_test() {
        use alloc::collections::BTreeMap;

        use azul_core::{
            hit_test::{HitTest, HitTestItem},
            window::WindowPosition,
        };

        let mut manager = GestureAndDragManager::new();

        // Start drag session
        manager.start_input_session(LogicalPosition::new(10.0, 10.0), make_instant(0), 0x01);
        manager.record_input_sample(LogicalPosition::new(25.0, 25.0), make_instant(100), 0x01);

        // Create mock hit test (simulating titlebar hit)
        let mut hit_test = HitTest {
            regular_hit_test_nodes: BTreeMap::new(),
            scroll_hit_test_nodes: BTreeMap::new(),
            scrollbar_hit_test_nodes: BTreeMap::new(),
        };

        hit_test.regular_hit_test_nodes.insert(
            NodeId::new(99), // Titlebar node ID
            HitTestItem {
                point_in_viewport: LogicalPosition::new(10.0, 10.0),
                point_relative_to_item: LogicalPosition::new(10.0, 10.0),
                is_focusable: false,
                is_iframe_hit: None,
            },
        );

        // Activate window drag with hit test
        manager.activate_window_drag(
            WindowPosition::Initialized(azul_core::geom::PhysicalPositionI32::new(100, 100)),
            Some(hit_test.clone()),
        );

        // Verify window drag was activated with hit test
        let window_drag = manager.get_window_drag().unwrap();
        assert!(window_drag.start_hit_test.is_some());

        let saved_hit_test = window_drag.start_hit_test.as_ref().unwrap();
        assert_eq!(saved_hit_test.regular_hit_test_nodes.len(), 1);
        assert!(saved_hit_test
            .regular_hit_test_nodes
            .contains_key(&NodeId::new(99)));
    }

    #[test]
    fn test_update_hit_test_methods() {
        use alloc::collections::BTreeMap;

        use azul_core::hit_test::HitTest;

        let mut manager = GestureAndDragManager::new();

        // Start drag and activate node drag
        manager.start_input_session(LogicalPosition::new(0.0, 0.0), make_instant(0), 0x01);
        manager.record_input_sample(LogicalPosition::new(20.0, 20.0), make_instant(100), 0x01);

        let drag_data = DragData {
            data: BTreeMap::new(),
            effect_allowed: DragEffect::Copy,
        };

        manager.activate_node_drag(
            DomId { inner: 0 },
            NodeId::new(1),
            drag_data,
            None, // Start with no hit test
        );

        assert!(manager.get_node_drag().unwrap().start_hit_test.is_none());

        // Update hit test later
        let hit_test = HitTest {
            regular_hit_test_nodes: BTreeMap::new(),
            scroll_hit_test_nodes: BTreeMap::new(),
            scrollbar_hit_test_nodes: BTreeMap::new(),
        };

        manager.update_node_drag_hit_test(Some(hit_test));

        // Verify hit test was updated
        assert!(manager.get_node_drag().unwrap().start_hit_test.is_some());
    }
}
