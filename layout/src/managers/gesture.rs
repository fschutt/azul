//! Gesture and drag manager for multi-frame gestures and drag operations.
//!
//! Collects input samples, detects drags, double-clicks, long presses, swipes,
//! pinch/rotate gestures, and manages drag state for nodes, windows, and file drops.
//!
//! ## Unified Drag System
//!
//! This module uses the `DragContext` from `azul_core::drag` to provide a unified
//! interface for all drag operations:
//! - Text selection drag
//! - Scrollbar thumb drag
//! - Node drag-and-drop
//! - Window drag/resize
//! - File drop from OS

use alloc::vec::Vec;
#[cfg(feature = "std")]
use std::sync::atomic::{AtomicU64, Ordering};

use azul_core::{
    dom::{DomId, NodeId},
    drag::{ActiveDragType, AutoScrollDirection, DragContext, DragData},
    geom::{LogicalPosition, PhysicalPositionI32},
    hit_test::HitTest,
    task::{Duration as CoreDuration, Instant as CoreInstant},
    window::WindowPosition,
};
use azul_css::{impl_option, impl_option_inner};


#[cfg(feature = "std")]
static NEXT_EVENT_ID: AtomicU64 = AtomicU64::new(1);

/// Allocate a new unique event ID
#[cfg(feature = "std")]
pub fn allocate_event_id() -> u64 {
    NEXT_EVENT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Allocate a new unique event ID (no_std fallback: returns 0)
#[cfg(not(feature = "std"))]
pub fn allocate_event_id() -> u64 {
    0
}

/// Helper function to convert `CoreDuration` to milliseconds
///
/// `CoreDuration` is an enum with System (`std::time::Duration`) and Tick variants.
/// We need to handle both cases for proper time calculations.
#[allow(clippy::cast_possible_truncation)] // bounded layout/render numeric cast
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
            // WARNING: assumes 1 tick = 1 ms. This is correct for platforms
            // that use a millisecond tick counter, but will silently produce
            // wrong timing on platforms with a different tick resolution.
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

/// Number of samples to drain at once when the session exceeds `MAX_SAMPLES_PER_SESSION`.
///
/// Batch draining avoids per-sample overhead on every new sample.
const DRAIN_BATCH_SIZE: usize = 100;

/// MWA-B4: button-state bitfield recorded for touch-contact samples.
///
/// A finger on the surface = primary contact, mirroring `BUTTON_STATE_LEFT` in
/// the dll's mouse path so drag heuristics treat touch like a held button.
pub const TOUCH_CONTACT_BUTTON_STATE: u8 = 0x01;

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
    /// Position in logical coordinates (window-local, Y=0 at top of window)
    pub position: LogicalPosition,
    /// Position in virtual screen coordinates (Y=0 at top of primary monitor).
    ///
    /// Computed as `window_position + position` at the time the sample is recorded.
    /// This is stable during window drags because `window_pos + cursor_local`
    /// always equals the true screen position, even when the window moves.
    ///
    /// All coordinates are in logical pixels (HiDPI-independent).
    /// On Wayland, this is an estimate (compositor does not expose global position).
    pub screen_position: LogicalPosition,
    /// Timestamp when this sample was recorded (from `ExternalSystemCallbacks`)
    pub timestamp: CoreInstant,
    /// Mouse button state (bitfield: 0x01 = left, 0x02 = right, 0x04 = middle)
    pub button_state: u8,
    /// Unique, monotonic event ID for ordering (atomic counter)
    pub event_id: u64,
    /// Pen/stylus pressure (0.0 to 1.0, 0.5 = default for mouse)
    pub pressure: f32,
    /// Pen/stylus tilt angles in degrees (`x_tilt`, `y_tilt`)
    /// Range: typically -90.0 to 90.0, (0.0, 0.0) = perpendicular
    pub tilt: (f32, f32),
    /// Touch contact radius in logical pixels (width, height)
    /// For mouse input, this is (0.0, 0.0)
    pub touch_radius: (f32, f32),
}

impl_option!(
    InputSample,
    OptionInputSample,
    copy = false,
    [Debug, Clone, PartialEq]
);

/// A sequence of input samples forming one button press session
#[derive(Debug, Clone, PartialEq)]
pub struct InputSession {
    /// All recorded samples for this session
    pub samples: Vec<InputSample>,
    /// Whether this session has ended (button released)
    pub ended: bool,
    /// Session ID for tracking (incremental counter)
    pub session_id: u64,
    /// Window position at the time this session started (mouse-down).
    /// Used by titlebar drag callbacks to compute new window position.
    pub window_position_at_start: WindowPosition,
}

impl InputSession {
    /// Create a new input session
    fn new(session_id: u64, first_sample: InputSample, window_position: WindowPosition) -> Self {
        Self {
            samples: vec![first_sample],
            ended: false,
            session_id,
            window_position_at_start: window_position,
        }
    }

    /// Get the first sample in this session
    #[must_use] pub fn first_sample(&self) -> Option<&InputSample> {
        self.samples.first()
    }

    /// Get the last sample in this session
    #[must_use] pub fn last_sample(&self) -> Option<&InputSample> {
        self.samples.last()
    }

    /// Get the duration of this session (first to last sample)
    #[must_use] pub fn duration_ms(&self) -> Option<u64> {
        let first = self.first_sample()?;
        let last = self.last_sample()?;
        let duration = last.timestamp.duration_since(&first.timestamp);
        Some(duration_to_millis(duration))
    }

    /// Get the total distance traveled in this session
    #[must_use] pub fn total_distance(&self) -> f32 {
        if self.samples.len() < 2 {
            return 0.0;
        }

        let mut total = 0.0;
        for i in 1..self.samples.len() {
            let prev = &self.samples[i - 1];
            let curr = &self.samples[i];
            let dx = curr.position.x - prev.position.x;
            let dy = curr.position.y - prev.position.y;
            total += dx.hypot(dy);
        }
        total
    }

    /// Get the straight-line distance from first to last sample
    #[must_use] pub fn direct_distance(&self) -> Option<f32> {
        let first = self.first_sample()?;
        let last = self.last_sample()?;
        let dx = last.position.x - first.position.x;
        let dy = last.position.y - first.position.y;
        Some(dx.hypot(dy))
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
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
#[repr(C)]
pub enum GestureDirection {
    Up,
    Down,
    Left,
    Right,
}

impl_option!(
    GestureDirection,
    OptionGestureDirection,
    [Debug, Clone, Copy, PartialEq, Eq]
);
impl_option!(
    DetectedPinch,
    OptionDetectedPinch,
    [Debug, Clone, Copy, PartialEq]
);
impl_option!(
    DetectedRotation,
    OptionDetectedRotation,
    [Debug, Clone, Copy, PartialEq]
);
impl_option!(
    DetectedLongPress,
    OptionDetectedLongPress,
    [Debug, Clone, Copy, PartialEq, Eq]
);

/// Result of pinch gesture detection
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
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
#[repr(C)]
pub struct DetectedRotation {
    /// Rotation angle in radians (positive = clockwise)
    pub angle_radians: f32,
    /// Center point of rotation
    pub center: LogicalPosition,
    /// Duration of rotation (milliseconds)
    pub duration_ms: u64,
}


/// State of pen/stylus input
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct PenState {
    /// Current pen position
    pub position: LogicalPosition,
    /// Current pressure (0.0 to 1.0)
    pub pressure: f32,
    /// Current tilt angles (`x_tilt`, `y_tilt`) in degrees
    pub tilt: crate::callbacks::PenTilt,
    /// Whether pen is in contact with surface
    pub in_contact: bool,
    /// Whether pen is inverted (eraser mode)
    pub is_eraser: bool,
    /// Whether barrel button is pressed
    pub barrel_button_pressed: bool,
    /// Unique identifier for this pen device
    pub device_id: u64,
    /// Tangential / cylinder pressure (0.0 to 1.0). Wacom Air Brush wheel,
    /// Surface Slim Pen 2 secondary axis. `0.0` means "not reported".
    /// Maps to W3C `PointerEvent.tangentialPressure`.
    pub tangential_pressure: f32,
    /// Barrel roll angle in radians (–π to π). Wacom Art Pen rotation,
    /// Surface Pen barrel-roll axis. `0.0` means "not reported" (devices
    /// that do report it sweep through the full range as the user rolls
    /// the pen — the resting state isn't necessarily zero, so callers
    /// should compare deltas, not absolute values).
    /// Maps to W3C `PointerEvent.twist` (in radians, not degrees).
    pub barrel_roll_rad: f32,
    /// Per-tool identity for hand-held pens that report it (Wintab GUID,
    /// Apple Pencil session id, S-Pen serial). `0` means "not reported".
    /// Distinct from `device_id` so callers can both identify the
    /// hardware (`device_id`) *and* which tip / lead / button cluster is
    /// in use (`tool_id`).
    pub tool_id: u32,
}

impl_option!(PenState, OptionPenState, [Debug, Clone, Copy, PartialEq]);

impl Default for PenState {
    fn default() -> Self {
        Self {
            position: LogicalPosition::zero(),
            pressure: 0.0,
            tilt: crate::callbacks::PenTilt {
                x_tilt: 0.0,
                y_tilt: 0.0,
            },
            in_contact: false,
            is_eraser: false,
            barrel_button_pressed: false,
            device_id: 0,
            tangential_pressure: 0.0,
            barrel_roll_rad: 0.0,
            tool_id: 0,
        }
    }
}

/// State of a Wacom-style tablet **pad** — the tablet body's own hardware
/// controls, distinct from the pen ([`PenState`] already covers eraser /
/// barrel button / barrel roll / tilt / pressure).
///
/// Populated by the platform
/// backend (`dll/src/desktop/extra/wacom_pad/`: Wintab on Windows,
/// libwacom+libinput on Linux, the driver's `NSEvent` tablet events on macOS).
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct WacomPadState {
    /// `ExpressKey` bitset — bit `n` set ⇔ hardware button `n` is held (up to
    /// 32). Read via [`WacomPadState::express_key`].
    pub express_keys: u32,
    /// Touch-ring / touch-strip absolute position, `0.0`–`1.0`. Only
    /// meaningful while [`WacomPadState::touch_ring_active`] is `true`.
    pub touch_ring: f32,
    /// Whether a finger is currently on the touch-ring / touch-strip.
    pub touch_ring_active: bool,
    /// Tablet device id (to distinguish pads on multi-tablet setups).
    pub device_id: u64,
}

impl_option!(
    WacomPadState,
    OptionWacomPadState,
    [Debug, Clone, Copy, PartialEq]
);

impl Default for WacomPadState {
    fn default() -> Self {
        Self {
            express_keys: 0,
            touch_ring: 0.0,
            touch_ring_active: false,
            device_id: 0,
        }
    }
}

impl WacomPadState {
    /// Whether `ExpressKey` `index` (0-based, < 32) is currently held.
    #[must_use] pub const fn express_key(&self, index: u32) -> bool {
        index < 32 && (self.express_keys & (1u32 << index)) != 0
    }
}

/// Manager for multi-frame gestures and drag operations
///
/// This collects raw input samples and analyzes them to detect gestures.
/// Designed for testability and clear separation of input collection
/// vs. detection.
///
/// ## Unified Drag System
///
/// The manager now uses `DragContext` to unify all drag types:
/// - `active_drag`: The unified drag context (replaces individual drag states)
///
/// For backwards compatibility, the old `node_drag`, `window_drag`, `file_drop`
/// fields are still accessible but deprecated.
#[derive(Debug, Clone, PartialEq)]
pub struct GestureAndDragManager {
    /// Configuration for gesture detection
    pub config: GestureDetectionConfig,
    /// All recorded input sessions (multiple button press sequences)
    pub input_sessions: Vec<InputSession>,
    /// **NEW**: Unified drag context for all drag types
    pub active_drag: Option<DragContext>,
    /// Current pen/stylus state
    pub pen_state: Option<PenState>,
    /// Pen state as of the previous determine-events pass (for diffing pen events).
    pub previous_pen_state: Option<PenState>,
    /// Set when pen state changed; gates one pen-event diff (cleared by the event loop).
    pub pen_event_pending: bool,
    /// Latest Wacom tablet-pad state (`ExpressKeys` + touch-ring), or `None`
    /// until a pad backend delivers one.
    pub pad_state: Option<WacomPadState>,
    /// Session IDs where long press callback has been invoked
    long_press_callbacks_invoked: Vec<u64>,
    /// Counter for generating unique session IDs
    next_session_id: u64,
    /// Native-platform gesture override slot.
    ///
    /// Platforms with first-class gesture recognizers (iOS `UIKit`,
    /// Android `GestureDetector` + `ScaleGestureDetector`, macOS
    /// `NSGestureRecognizer`) inject pre-detected gestures here via
    /// [`GestureAndDragManager::inject_native_gesture`]. The
    /// `detect_*` methods consult this slot before running their
    /// in-process heuristics, so callbacks observe consistent results
    /// regardless of the detection source.
    ///
    /// Cleared automatically at the start of every new input recording
    /// cycle so a single OS event doesn't keep firing.
    pub native_gesture: Option<NativeGestureEvent>,
    /// MWA-B4: OS touch id → session id. Desktop touch events previously
    /// only filled the window's `touch_state`, so no touch ever became an
    /// input session and `detect_pinch` / `detect_rotation` (which need two
    /// concurrent sessions) were structurally dead on Windows/X11/Wayland.
    /// The shells call [`touch_down`](Self::touch_down) /
    /// [`touch_move`](Self::touch_move) / [`touch_up`](Self::touch_up); each
    /// finger gets its own session (two fingers = two live sessions).
    touch_sessions: alloc::collections::btree_map::BTreeMap<u64, u64>,
}

/// Gesture detected by a platform-native recognizer.
///
/// Platform backends construct one of these in their gesture-recognizer
/// callbacks (iOS `UIKit`, Android `GestureDetector`, macOS
/// `NSGestureRecognizer`) and hand it to
/// [`GestureAndDragManager::inject_native_gesture`]. The in-process
/// `detect_*` methods then return the native result, sidestepping their
/// fallback heuristics. On platforms with poor native gesture support
/// (X11 / Wayland touch, headless), backends never inject and the
/// in-process detector remains authoritative.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C, u8)]
pub enum NativeGestureEvent {
    /// Single tap / double-click detected natively.
    DoubleClick,
    /// Long-press detected natively (iOS `UILongPressGestureRecognizer`,
    /// Android `GestureDetector.OnGestureListener::onLongPress`).
    LongPress(DetectedLongPress),
    /// Swipe detected natively (iOS `UISwipeGestureRecognizer`,
    /// Android `GestureDetector.OnGestureListener::onFling`).
    Swipe(GestureDirection),
    /// Pinch detected natively (iOS `UIPinchGestureRecognizer`,
    /// Android `ScaleGestureDetector`, macOS magnification gesture).
    Pinch(DetectedPinch),
    /// Rotation detected natively (iOS `UIRotationGestureRecognizer`,
    /// macOS rotation gesture).
    Rotation(DetectedRotation),
}


impl Default for GestureAndDragManager {
    fn default() -> Self {
        Self::new()
    }
}

impl GestureAndDragManager {
    /// (`input_sessions`, `long_press_callbacks_invoked`). Used by
    /// `AZ_E2E_TEST` to watch for unbounded growth.
    #[must_use] pub const fn debug_counts(&self) -> (usize, usize) {
        (self.input_sessions.len(), self.long_press_callbacks_invoked.len())
    }

    /// Create a new gesture and drag manager
    #[must_use] pub fn new() -> Self {
        Self {
            config: GestureDetectionConfig::default(),
            input_sessions: Vec::new(),
            next_session_id: 1,
            active_drag: None,
            pen_state: None,
            previous_pen_state: None,
            pen_event_pending: false,
            pad_state: None,
            long_press_callbacks_invoked: Vec::new(),
            native_gesture: None,
            touch_sessions: alloc::collections::btree_map::BTreeMap::new(),
        }
    }

    /// Inject a native gesture-recognizer result, overriding the
    /// in-process detector for the current event frame. Called by the
    /// iOS / Android / macOS platform backend from their gesture
    /// recognizer callbacks. The override is read once by the next
    /// `detect_*` call.
    pub const fn inject_native_gesture(&mut self, gesture: NativeGestureEvent) {
        self.native_gesture = Some(gesture);
    }

    /// Clear any pending native-gesture override. Called by the event
    /// loop after each frame's detections have been consumed so a
    /// stale OS gesture doesn't keep firing.
    pub const fn clear_native_gesture(&mut self) {
        self.native_gesture = None;
    }

    /// Create with custom configuration
    #[must_use] pub fn with_config(config: GestureDetectionConfig) -> Self {
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
    /// `window_position` is the current OS window position at the time of mouse-down.
    /// It is stored so that drag callbacks can compute the new window position.
    ///
    /// Returns the session ID for this new session.
    pub fn start_input_session(
        &mut self,
        position: LogicalPosition,
        timestamp: CoreInstant,
        button_state: u8,
        window_position: WindowPosition,
        screen_position: LogicalPosition,
    ) -> u64 {
        self.start_input_session_with_pen(
            position,
            timestamp,
            button_state,
            allocate_event_id(),
            0.5,        // default pressure for mouse
            (0.0, 0.0), // no tilt for mouse
            (0.0, 0.0), // no touch radius for mouse
            window_position,
            screen_position,
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
        window_position: WindowPosition,
        screen_position: LogicalPosition,
    ) -> u64 {
        // Clear old ended sessions, but keep the most recent ended session
        // for double-click detection. detect_double_click() needs two ended
        // sessions to compare timing and distance.
        let last_ended_idx = self.input_sessions.iter().rposition(|s| s.ended);
        let mut idx = 0usize;
        self.input_sessions.retain(|session| {
            let keep = !session.ended || Some(idx) == last_ended_idx;
            idx += 1;
            keep
        });

        let session_id = self.next_session_id;
        self.next_session_id += 1;

        let sample = InputSample {
            position,
            screen_position,
            timestamp,
            button_state,
            event_id,
            pressure,
            tilt,
            touch_radius,
        };

        let session = InputSession::new(session_id, sample, window_position);
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
        screen_position: LogicalPosition,
    ) -> bool {
        self.record_input_sample_with_pen(
            position,
            timestamp,
            button_state,
            allocate_event_id(),
            0.5,        // default pressure for mouse
            (0.0, 0.0), // no tilt for mouse
            (0.0, 0.0), // no touch radius for mouse
            screen_position,
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
        screen_position: LogicalPosition,
    ) -> bool {
        let Some(session) = self.input_sessions.last_mut() else {
            return false;
        };

        if session.ended {
            return false;
        }

        // Enforce max samples limit
        if session.samples.len() >= MAX_SAMPLES_PER_SESSION {
            // Remove oldest samples, keeping the most recent ones
            let remove_count = session.samples.len() - MAX_SAMPLES_PER_SESSION + DRAIN_BATCH_SIZE;
            session.samples.drain(0..remove_count);
        }

        session.samples.push(InputSample {
            position,
            screen_position,
            timestamp,
            button_state,
            event_id,
            pressure,
            tilt,
            touch_radius,
        });

        true
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

    // --- Per-touch-id input sessions (MWA-B4) ---

    /// A finger made contact: open a dedicated session for `touch_id`.
    pub fn touch_down(
        &mut self,
        touch_id: u64,
        position: LogicalPosition,
        timestamp: CoreInstant,
        window_position: WindowPosition,
        screen_position: LogicalPosition,
    ) {
        let session_id = self.start_input_session(
            position,
            timestamp,
            TOUCH_CONTACT_BUTTON_STATE,
            window_position,
            screen_position,
        );
        self.touch_sessions.insert(touch_id, session_id);
    }

    /// A finger moved: record into ITS OWN session — never `last_mut()`,
    /// two concurrent fingers must not interleave into one session (that
    /// would corrupt both the drag heuristics and pinch/rotate distances).
    /// Returns `true` if a sample was recorded.
    pub fn touch_move(
        &mut self,
        touch_id: u64,
        position: LogicalPosition,
        timestamp: CoreInstant,
        screen_position: LogicalPosition,
    ) -> bool {
        let Some(session_id) = self.touch_sessions.get(&touch_id).copied() else {
            return false;
        };
        self.record_sample_for_session(session_id, position, timestamp, screen_position)
    }

    /// A finger lifted (or the OS cancelled the touch): final sample + end
    /// the session and drop the id mapping.
    pub fn touch_up(
        &mut self,
        touch_id: u64,
        position: LogicalPosition,
        timestamp: CoreInstant,
        screen_position: LogicalPosition,
    ) {
        let Some(session_id) = self.touch_sessions.remove(&touch_id) else {
            return;
        };
        let _ = self.record_sample_for_session(session_id, position, timestamp, screen_position);
        if let Some(session) = self
            .input_sessions
            .iter_mut()
            .find(|s| s.session_id == session_id)
        {
            session.ended = true;
        }
    }

    /// The OS cancelled the whole touch sequence (e.g. the compositor took
    /// the gesture over): end every touch session and drop the id map.
    pub fn touch_cancel_all(&mut self) {
        let ids: Vec<u64> = self.touch_sessions.values().copied().collect();
        self.touch_sessions.clear();
        for session_id in ids {
            if let Some(session) = self
                .input_sessions
                .iter_mut()
                .find(|s| s.session_id == session_id)
            {
                session.ended = true;
            }
        }
    }

    /// Record a sample into the session with `session_id` (MWA-B4 helper —
    /// the by-id sibling of `record_input_sample_with_pen`, which only ever
    /// writes to the LAST session).
    fn record_sample_for_session(
        &mut self,
        session_id: u64,
        position: LogicalPosition,
        timestamp: CoreInstant,
        screen_position: LogicalPosition,
    ) -> bool {
        let Some(session) = self
            .input_sessions
            .iter_mut()
            .find(|s| s.session_id == session_id)
        else {
            return false;
        };
        if session.ended {
            return false;
        }
        if session.samples.len() >= MAX_SAMPLES_PER_SESSION {
            let remove_count =
                session.samples.len() - MAX_SAMPLES_PER_SESSION + DRAIN_BATCH_SIZE;
            session.samples.drain(0..remove_count);
        }
        session.samples.push(InputSample {
            position,
            screen_position,
            timestamp,
            button_state: TOUCH_CONTACT_BUTTON_STATE,
            event_id: allocate_event_id(),
            pressure: 0.5,
            tilt: (0.0, 0.0),
            touch_radius: (0.0, 0.0),
        });
        true
    }

    /// Clear old input sessions that have timed out
    ///
    /// Call this periodically (e.g., every frame) to prevent memory leaks.
    /// Sessions older than `config.sample_cleanup_interval_ms` are removed.
    // CoreInstant is a ref-counted FFI clock handle threaded through the event loop by value;
    // &-converting would cascade through the loop call chain and across all dll backends.
    #[allow(clippy::needless_pass_by_value)]
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
    /// Call this when receiving pen events from the platform. The
    /// extended fields (`tangential_pressure`, `barrel_roll_rad`,
    /// `tool_id`) default to `0` — pass [`update_pen_state_full`] when
    /// the platform reports them.
    pub const fn update_pen_state(
        &mut self,
        position: LogicalPosition,
        pressure: f32,
        tilt: (f32, f32),
        in_contact: bool,
        is_eraser: bool,
        barrel_button_pressed: bool,
        device_id: u64,
    ) {
        self.update_pen_state_full(
            position,
            pressure,
            tilt,
            in_contact,
            is_eraser,
            barrel_button_pressed,
            device_id,
            0.0,
            0.0,
            0,
        );
    }

    /// Update pen/stylus state including the extended axes (W3C
    /// `PointerEvent.tangentialPressure` + `twist`) and per-tool id.
    pub const fn update_pen_state_full(
        &mut self,
        position: LogicalPosition,
        pressure: f32,
        tilt: (f32, f32),
        in_contact: bool,
        is_eraser: bool,
        barrel_button_pressed: bool,
        device_id: u64,
        tangential_pressure: f32,
        barrel_roll_rad: f32,
        tool_id: u32,
    ) {
        self.previous_pen_state = self.pen_state;
        self.pen_state = Some(PenState {
            position,
            pressure,
            tilt: crate::callbacks::PenTilt {
                x_tilt: tilt.0,
                y_tilt: tilt.1,
            },
            in_contact,
            is_eraser,
            barrel_button_pressed,
            device_id,
            tangential_pressure,
            barrel_roll_rad,
            tool_id,
        });
        self.pen_event_pending = true;
    }

    /// Clear pen state (when pen leaves proximity)
    pub const fn clear_pen_state(&mut self) {
        self.previous_pen_state = self.pen_state;
        self.pen_state = None;
        self.pen_event_pending = true;
    }

    /// Get current pen state (read-only)
    #[must_use] pub const fn get_pen_state(&self) -> Option<&PenState> {
        self.pen_state.as_ref()
    }

    /// Get the previous pen state (for event diffing).
    #[must_use] pub const fn get_previous_pen_state(&self) -> Option<&PenState> {
        self.previous_pen_state.as_ref()
    }

    /// Clear the pen-event-pending flag (called by the event loop after a pass).
    pub const fn clear_pen_event_pending(&mut self) {
        self.pen_event_pending = false;
    }

    /// Set the latest Wacom tablet-pad state (called by the pad backend).
    pub const fn update_pad_state(&mut self, pad: WacomPadState) {
        self.pad_state = Some(pad);
    }

    /// The latest tablet-pad state, or `None` if no pad backend delivered one.
    #[must_use] pub const fn get_pad_state(&self) -> Option<&WacomPadState> {
        self.pad_state.as_ref()
    }

    /// Clear the tablet-pad state (pad disconnected / proximity left).
    pub const fn clear_pad_state(&mut self) {
        self.pad_state = None;
    }

    // Gesture Detection Methods (query state without mutation)

    /// Detect if current input represents a drag gesture
    ///
    /// Returns Some(DetectedDrag) if a drag is detected based on distance threshold.
    #[must_use] pub fn detect_drag(&self) -> Option<DetectedDrag> {
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
    #[must_use] pub fn detect_long_press(&self) -> Option<DetectedLongPress> {
        if let Some(NativeGestureEvent::LongPress(lp)) = self.native_gesture {
            return Some(lp);
        }
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
    /// Call this after invoking the long press callback to prevent
    /// repeated invocations.
    /// MWA-B12: mark the CURRENT session's long-press as delivered. The
    /// event pass calls this right after emitting `EventType::LongPress` —
    /// nothing ever called `mark_long_press_callback_invoked`, so `LongPress`
    /// re-fired on every subsequent pass of the same hold.
    pub fn mark_current_long_press_invoked(&mut self) {
        if let Some(id) = self.get_current_session().map(|s| s.session_id) {
            self.mark_long_press_callback_invoked(id);
        }
    }

    pub fn mark_long_press_callback_invoked(&mut self, session_id: u64) {
        if !self.long_press_callbacks_invoked.contains(&session_id) {
            self.long_press_callbacks_invoked.push(session_id);
        }
    }

    /// Detect if last two sessions form a double-click.
    ///
    /// Returns true if timing and distance match double-click criteria.
    #[must_use] pub fn detect_double_click(&self) -> bool {
        if matches!(self.native_gesture, Some(NativeGestureEvent::DoubleClick)) {
            return true;
        }
        let sessions = &self.input_sessions;
        if sessions.len() < 2 {
            return false;
        }

        let prev_session = &sessions[sessions.len() - 2];
        let last_session = &sessions[sessions.len() - 1];

        // Both sessions must have ended (button released)
        if !prev_session.ended || !last_session.ended {
            return false;
        }

        let prev_first = prev_session.first_sample();
        let last_first = last_session.first_sample();
        let (Some(prev_first), Some(last_first)) = (prev_first, last_first) else {
            return false;
        };

        let duration = last_first.timestamp.duration_since(&prev_first.timestamp);
        let time_delta_ms = duration_to_millis(duration);
        if time_delta_ms > self.config.double_click_time_threshold_ms {
            return false;
        }

        let dx = last_first.position.x - prev_first.position.x;
        let dy = last_first.position.y - prev_first.position.y;
        let distance = dx.hypot(dy);

        distance < self.config.double_click_distance_threshold
    }

    /// Detect click count (1=single, 2=double, 3=triple) by examining
    /// the recent ended sessions.  Uses only timestamps and positions
    /// from the session history, so the result is fully deterministic
    /// for any given sequence of `InputSession`s (easy to unit-test
    /// with synthetic `CoreInstant`/`CoreDuration` values).
    #[must_use] pub fn detect_click_count(&self) -> u32 {
        let sessions = &self.input_sessions;
        let n = sessions.len();
        if n == 0 {
            return 1;
        }

        // We need at least 2 ended sessions for double-click,
        // 3 ended sessions for triple-click.
        // Walk backwards from the most recent ended session and count
        // how many consecutive clicks fall within the time+distance
        // thresholds.

        // Collect the last up-to-3 ended sessions (most-recent first).
        let mut recent: Vec<&InputSession> = Vec::new();
        for s in sessions.iter().rev() {
            if !s.ended {
                continue;
            }
            recent.push(s);
            if recent.len() >= 3 {
                break;
            }
        }

        if recent.is_empty() {
            return 1;
        }

        // recent[0] = most recent ended session
        // recent[1] = previous ended session (if any)
        // recent[2] = one before that (if any)
        let mut count = 1u32;

        for i in 0..recent.len() - 1 {
            let later = recent[i];
            let earlier = recent[i + 1];

            let Some(later_start) = later.first_sample() else {
                break;
            };
            let Some(earlier_start) = earlier.first_sample() else {
                break;
            };

            let duration = later_start.timestamp.duration_since(&earlier_start.timestamp);
            let time_delta_ms = duration_to_millis(duration);
            if time_delta_ms > self.config.double_click_time_threshold_ms {
                break;
            }

            let dx = later_start.position.x - earlier_start.position.x;
            let dy = later_start.position.y - earlier_start.position.y;
            let distance = dx.hypot(dy);
            if distance >= self.config.double_click_distance_threshold {
                break;
            }

            count += 1;
        }

        // Cap at 3 (triple-click selects paragraph, beyond that cycles back)
        if count > 3 { 1 } else { count }
    }

    /// Get the primary direction of current drag.
    #[must_use] pub fn get_drag_direction(&self) -> Option<GestureDirection> {
        let session = self.get_current_session()?;
        let first = session.first_sample()?;
        let last = session.last_sample()?;

        let dx = last.position.x - first.position.x;
        let dy = last.position.y - first.position.y;

        let direction = match (dx.abs() > dy.abs(), dx > 0.0, dy > 0.0) {
            (true, true, _) => GestureDirection::Right,
            (true, false, _) => GestureDirection::Left,
            (false, _, true) => GestureDirection::Down,
            (false, _, false) => GestureDirection::Up,
        };
        Some(direction)
    }

    /// Get average velocity of current gesture (pixels per second)
    #[allow(clippy::cast_precision_loss)] // bounded layout/render numeric cast
    #[must_use] pub fn get_gesture_velocity(&self) -> Option<f32> {
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

    /// Check if current gesture is a swipe (fast directional movement).
    #[must_use] pub fn is_swipe(&self) -> bool {
        self.get_gesture_velocity()
            .is_some_and(|v| v >= self.config.swipe_velocity_threshold)
    }

    /// Detect swipe with specific direction
    ///
    /// Returns Some(dir) if gesture is a fast swipe in a clear direction
    #[must_use] pub fn detect_swipe_direction(&self) -> Option<GestureDirection> {
        if let Some(NativeGestureEvent::Swipe(d)) = self.native_gesture {
            return Some(d);
        }
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
    #[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
    #[must_use] pub fn detect_pinch(&self) -> Option<DetectedPinch> {
        if let Some(NativeGestureEvent::Pinch(p)) = self.native_gesture {
            return Some(p);
        }
        // Need at least two active sessions for pinch
        if self.input_sessions.len() < 2 {
            return None;
        }

        // Get last two sessions (most recent touches)
        let session1 = &self.input_sessions[self.input_sessions.len() - 2];
        let session2 = &self.input_sessions[self.input_sessions.len() - 1];

        // A pinch is a TWO-finger gesture: both contacts must be concurrently
        // active. A desktop mouse produces *sequential* sessions (the previous one
        // is `ended` on button-up before the next begins), so without this guard a
        // stale ended session (e.g. a prior click on a button) pairs with the
        // current drag and is misread as a pinch — the map zooms on a plain click.
        if session1.ended || session2.ended {
            return None;
        }

        // Both must have samples
        let first1 = session1.first_sample()?;
        let first2 = session2.first_sample()?;
        let last1 = session1.last_sample()?;
        let last2 = session2.last_sample()?;

        // Calculate initial distance between touches
        let dx_initial = first2.position.x - first1.position.x;
        let dy_initial = first2.position.y - first1.position.y;
        let initial_distance = dx_initial.hypot(dy_initial);

        // Calculate current distance
        let dx_current = last2.position.x - last1.position.x;
        let dy_current = last2.position.y - last1.position.y;
        let current_distance = dx_current.hypot(dy_current);

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
            x: f32::midpoint(last1.position.x, last2.position.x),
            y: f32::midpoint(last1.position.y, last2.position.y),
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
    #[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
    #[must_use] pub fn detect_rotation(&self) -> Option<DetectedRotation> {
        const PI: f32 = core::f32::consts::PI;
        if let Some(NativeGestureEvent::Rotation(r)) = self.native_gesture {
            return Some(r);
        }
        // Need at least two active sessions
        if self.input_sessions.len() < 2 {
            return None;
        }

        // Get last two sessions
        let session1 = &self.input_sessions[self.input_sessions.len() - 2];
        let session2 = &self.input_sessions[self.input_sessions.len() - 1];

        // Two-finger rotation requires both contacts concurrently active; a desktop
        // mouse yields sequential sessions, so a stale ended session must not pair
        // with the current one (see detect_pinch).
        if session1.ended || session2.ended {
            return None;
        }

        // Both must have samples
        let first1 = session1.first_sample()?;
        let first2 = session2.first_sample()?;
        let last1 = session1.last_sample()?;
        let last2 = session2.last_sample()?;

        // Calculate center (average of both touches)
        let center = LogicalPosition {
            x: f32::midpoint(last1.position.x, last2.position.x),
            y: f32::midpoint(last1.position.y, last2.position.y),
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
        #[allow(clippy::while_float)] // intentional bounded float loop (angle-wrap / pixel-step); an integer counter would be artificial
        while angle_diff > PI {
            angle_diff -= 2.0 * PI;
        }
        #[allow(clippy::while_float)] // intentional bounded float loop (angle-wrap / pixel-step); an integer counter would be artificial
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
    #[must_use] pub fn get_current_session(&self) -> Option<&InputSession> {
        self.input_sessions.last()
    }

    /// Get current mouse position from latest sample
    #[must_use] pub fn get_current_mouse_position(&self) -> Option<LogicalPosition> {
        self.get_current_session()
            .and_then(|s| s.last_sample())
            .map(|sample| sample.position)
    }

    /// Get the drag delta (current mouse position minus mouse-down position)
    /// from the current input session.
    ///
    /// Returns `None` if there is no active session or not enough samples.
    #[must_use] pub fn get_drag_delta(&self) -> Option<(f32, f32)> {
        let session = self.get_current_session()?;
        let first = session.first_sample()?;
        let last = session.last_sample()?;
        Some((
            last.position.x - first.position.x,
            last.position.y - first.position.y,
        ))
    }

    /// Get the drag delta in **screen-absolute** coordinates.
    ///
    /// Unlike `get_drag_delta()` which uses window-local coordinates (and therefore
    /// oscillates during window drags due to the window moving under the cursor),
    /// this method uses screen-absolute positions that are stable regardless of
    /// window movement.
    ///
    /// **Use this for window dragging (titlebar drag).**
    /// Use `get_drag_delta()` for in-window operations (node drag-and-drop, etc.).
    ///
    /// Returns `None` if there is no active session or not enough samples.
    #[must_use] pub fn get_drag_delta_screen(&self) -> Option<(f32, f32)> {
        let session = self.get_current_session()?;
        let first = session.first_sample()?;
        let last = session.last_sample()?;
        Some((
            last.screen_position.x - first.screen_position.x,
            last.screen_position.y - first.screen_position.y,
        ))
    }

    /// Get the **incremental** (frame-to-frame) drag delta in screen coordinates.
    ///
    /// Returns `(dx, dy)` where `dx = last_screen.x - previous_screen.x` and
    /// `dy = last_screen.y - previous_screen.y`.
    ///
    /// Unlike `get_drag_delta_screen()` which returns the *total* delta since drag
    /// start, this returns only the delta since the previous sample. This is used
    /// by `titlebar_drag` to apply position changes incrementally:
    ///
    /// ```text
    /// new_pos = current_window_pos + incremental_delta
    /// ```
    ///
    /// This approach is more robust than `initial_pos + total_delta` because it
    /// automatically handles external window position changes (DPI change, OS
    /// clamping, compositor resize) that would make `initial_pos` stale.
    ///
    /// Returns `None` if there is no active session or fewer than 2 samples.
    #[must_use] pub fn get_drag_delta_screen_incremental(&self) -> Option<(f32, f32)> {
        let session = self.get_current_session()?;
        let len = session.samples.len();
        if len < 2 {
            return None;
        }
        let prev = &session.samples[len - 2];
        let last = &session.samples[len - 1];
        Some((
            last.screen_position.x - prev.screen_position.x,
            last.screen_position.y - prev.screen_position.y,
        ))
    }

    /// Get the window position that was stored when the current input session
    /// started (i.e. on mouse-down).  Titlebar drag callbacks use this
    /// together with `get_drag_delta_screen()` to compute the new window position.
    #[must_use] pub fn get_window_position_at_session_start(&self) -> Option<WindowPosition> {
        let session = self.get_current_session()?;
        Some(session.window_position_at_start)
    }

    // ========================================================================
    // UNIFIED DRAG CONTEXT API (NEW)
    // ========================================================================

    /// Get the active drag context (if any)
    #[must_use] pub const fn get_drag_context(&self) -> Option<&DragContext> {
        self.active_drag.as_ref()
    }

    /// Get the active drag context mutably (if any)
    pub const fn get_drag_context_mut(&mut self) -> Option<&mut DragContext> {
        self.active_drag.as_mut()
    }

    // NOTE: text-selection and scrollbar-thumb drags do NOT flow through this
    // manager's `active_drag`. Text selection is driven by `MultiCursorState`
    // (managers/selection.rs) and scrollbar dragging by `ScrollbarDragState`
    // (window.rs, set in common/event.rs). The former `activate_text_selection_drag`
    // / `activate_scrollbar_drag` constructors here were dead duplicates of those
    // paths (zero callers) and were removed.

    /// Activate a node drag-and-drop
    pub fn activate_node_drag(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        drag_data: DragData,
        _start_hit_test: Option<HitTest>,
    ) {
        if let Some(detected) = self.detect_drag() {
            self.active_drag = Some(DragContext::node_drag(
                dom_id,
                node_id,
                detected.start_position,
                drag_data,
                detected.session_id,
            ));
        }
    }

    /// Activate a window move drag (titlebar)
    pub fn activate_window_drag(
        &mut self,
        initial_window_position: WindowPosition,
        _start_hit_test: Option<HitTest>,
    ) {
        if let Some(detected) = self.detect_drag() {
            self.active_drag = Some(DragContext::window_move(
                detected.start_position,
                initial_window_position,
                detected.session_id,
            ));
        }
    }

    // NOTE: OS file drops are tracked by `FileDropManager` (managers/file_drop.rs),
    // not by this manager's `active_drag`. The former `start_file_drop` constructor
    // here was a dead duplicate (zero callers) and was removed.

    /// Update positions for active drag (call on mouse move)
    pub const fn update_active_drag_positions(&mut self, position: LogicalPosition) {
        if let Some(ref mut drag) = self.active_drag {
            drag.update_position(position);
        }
    }

    /// Update drop target for node or file drag
    pub fn update_drop_target(&mut self, target: Option<azul_core::dom::DomNodeId>) {
        if let Some(ref mut drag) = self.active_drag {
            match &mut drag.drag_type {
                ActiveDragType::Node(ref mut node_drag) => {
                    node_drag.current_drop_target = target.into();
                }
                ActiveDragType::FileDrop(ref mut file_drop) => {
                    file_drop.drop_target = target.into();
                }
                _ => {}
            }
        }
    }

    /// Update auto-scroll direction for text selection drag
    pub const fn update_auto_scroll_direction(&mut self, direction: AutoScrollDirection) {
        if let Some(ref mut drag) = self.active_drag {
            if let Some(text_drag) = drag.as_text_selection_mut() {
                text_drag.auto_scroll_direction = direction;
            }
        }
    }

    /// End the current drag and return the context
    pub const fn end_drag(&mut self) -> Option<DragContext> {
        self.active_drag.take()
    }

    /// Cancel the current drag
    pub fn cancel_drag(&mut self) {
        if let Some(ref mut drag) = self.active_drag {
            drag.cancelled = true;
        }
        self.active_drag = None;
    }

    // ========================================================================
    // QUERY METHODS
    // ========================================================================

    /// Check if any drag operation is in progress
    #[must_use] pub const fn is_dragging(&self) -> bool {
        self.active_drag.is_some()
    }

    /// Check if a text selection drag is active
    #[must_use] pub fn is_text_selection_dragging(&self) -> bool {
        self.active_drag.as_ref().is_some_and(DragContext::is_text_selection)
    }

    /// Check if a scrollbar thumb drag is active
    #[must_use] pub fn is_scrollbar_dragging(&self) -> bool {
        self.active_drag.as_ref().is_some_and(DragContext::is_scrollbar_thumb)
    }

    /// Check if a node drag is active
    #[must_use] pub fn is_node_drag_active(&self) -> bool {
        self.active_drag.as_ref().is_some_and(DragContext::is_node_drag)
    }

    /// Check if a specific node is being dragged
    #[must_use] pub fn is_node_dragging(&self, dom_id: DomId, node_id: NodeId) -> bool {
        self.active_drag.as_ref().is_some_and(|d| {
            d.as_node_drag().is_some_and(|node_drag| node_drag.dom_id == dom_id && node_drag.node_id == node_id)
        })
    }

    /// Check if window drag is active
    #[must_use] pub fn is_window_dragging(&self) -> bool {
        self.active_drag.as_ref().is_some_and(DragContext::is_window_move)
    }

    /// Check if file drop is active
    #[must_use] pub fn is_file_dropping(&self) -> bool {
        self.active_drag.as_ref().is_some_and(DragContext::is_file_drop)
    }

    /// Get number of active input sessions
    #[must_use] pub const fn session_count(&self) -> usize {
        self.input_sessions.len()
    }

    /// Get current session ID (if any)
    #[must_use] pub fn current_session_id(&self) -> Option<u64> {
        self.get_current_session().map(|s| s.session_id)
    }

    // ========================================================================
    // WINDOW DRAG HELPER METHODS
    // ========================================================================

    /// Calculate window position delta from current drag state
    ///
    /// Returns (`delta_x`, `delta_y`) to apply to window position.
    /// Returns None if no window drag is active or drag hasn't moved.
    #[allow(clippy::cast_possible_truncation)] // bounded layout/render numeric cast
    #[must_use] pub fn get_window_drag_delta(&self) -> Option<(i32, i32)> {
        let drag = self.active_drag.as_ref()?.as_window_move()?;

        let delta_x = drag.current_position.x - drag.start_position.x;
        let delta_y = drag.current_position.y - drag.start_position.y;

        match drag.initial_window_position {
            WindowPosition::Initialized(_initial_pos) => Some((delta_x as i32, delta_y as i32)),
            _ => None,
        }
    }

    /// Get the new window position based on current drag
    ///
    /// Returns the absolute window position to set.
    #[allow(clippy::cast_possible_truncation)] // bounded layout/render numeric cast
    #[must_use] pub fn get_window_position_from_drag(&self) -> Option<WindowPosition> {
        let drag = self.active_drag.as_ref()?.as_window_move()?;

        let delta_x = drag.current_position.x - drag.start_position.x;
        let delta_y = drag.current_position.y - drag.start_position.y;

        match drag.initial_window_position {
            WindowPosition::Initialized(initial_pos) => {
                Some(WindowPosition::Initialized(PhysicalPositionI32::new(
                    initial_pos.x + delta_x as i32,
                    initial_pos.y + delta_y as i32,
                )))
            }
            _ => None,
        }
    }

    /// Calculate the new scroll offset for scrollbar thumb drag
    #[must_use] pub fn get_scrollbar_scroll_offset(&self) -> Option<f32> {
        self.active_drag.as_ref()?.calculate_scrollbar_scroll_offset()
    }

}

impl crate::managers::NodeIdRemap for GestureAndDragManager {
    /// Remap `NodeIds` in the active drag context after DOM reconciliation.
    ///
    /// When the DOM is regenerated during an active drag, `NodeIds` change.
    /// If a critical `NodeId` was unmounted, the drag is cancelled (an active
    /// drag whose source node no longer exists cannot be completed).
    fn remap_node_ids(&mut self, dom_id: DomId, map: &crate::managers::NodeIdMap) {
        if let Some(ref mut drag) = self.active_drag {
            if !drag.remap_node_ids(dom_id, map.as_btree_map()) {
                // Critical node removed — cancel the drag
                drag.cancelled = true;
                self.active_drag = None;
            }
        }
    }
}

#[cfg(test)]
mod touch_session_tests {
    use super::*;
    use azul_core::task::{Instant as TestInstant, SystemTick};

    fn ts(n: u64) -> CoreInstant {
        TestInstant::Tick(SystemTick::new(n))
    }

    fn pos(x: f32, y: f32) -> LogicalPosition {
        LogicalPosition { x, y }
    }

    #[test]
    fn two_fingers_open_two_concurrent_sessions() {
        let mut m = GestureAndDragManager::new();
        m.touch_down(1, pos(100.0, 100.0), ts(0), WindowPosition::Uninitialized, pos(100.0, 100.0));
        m.touch_down(2, pos(200.0, 100.0), ts(1), WindowPosition::Uninitialized, pos(200.0, 100.0));
        assert_eq!(m.input_sessions.len(), 2);
        assert!(!m.input_sessions[0].ended);
        assert!(!m.input_sessions[1].ended);
    }

    #[test]
    fn moves_land_in_the_correct_session_not_the_last_one() {
        let mut m = GestureAndDragManager::new();
        m.touch_down(1, pos(100.0, 100.0), ts(0), WindowPosition::Uninitialized, pos(100.0, 100.0));
        m.touch_down(2, pos(200.0, 100.0), ts(1), WindowPosition::Uninitialized, pos(200.0, 100.0));
        // Move finger 1 — the FIRST session must receive the sample even
        // though session 2 is the most recent (record_input_sample would
        // have corrupted session 2 here).
        assert!(m.touch_move(1, pos(90.0, 100.0), ts(2), pos(90.0, 100.0)));
        assert_eq!(m.input_sessions[0].samples.len(), 2, "finger 1 session grew");
        assert_eq!(m.input_sessions[1].samples.len(), 1, "finger 2 session untouched");
    }

    #[test]
    fn spread_gesture_is_detected_as_pinch_out() {
        let mut m = GestureAndDragManager::new();
        m.touch_down(1, pos(100.0, 100.0), ts(0), WindowPosition::Uninitialized, pos(100.0, 100.0));
        m.touch_down(2, pos(200.0, 100.0), ts(1), WindowPosition::Uninitialized, pos(200.0, 100.0));
        // Spread: initial distance 100 → current distance 200.
        m.touch_move(1, pos(50.0, 100.0), ts(2), pos(50.0, 100.0));
        m.touch_move(2, pos(250.0, 100.0), ts(3), pos(250.0, 100.0));
        let pinch = m.detect_pinch().expect("two concurrent touch sessions must yield a pinch");
        assert!(
            pinch.scale > 1.5,
            "spread must read as pinch-out (scale {}), initial {} current {}",
            pinch.scale,
            pinch.initial_distance,
            pinch.current_distance
        );
    }

    #[test]
    fn touch_up_ends_only_its_own_session() {
        let mut m = GestureAndDragManager::new();
        m.touch_down(1, pos(100.0, 100.0), ts(0), WindowPosition::Uninitialized, pos(100.0, 100.0));
        m.touch_down(2, pos(200.0, 100.0), ts(1), WindowPosition::Uninitialized, pos(200.0, 100.0));
        m.touch_up(1, pos(100.0, 100.0), ts(2), pos(100.0, 100.0));
        assert!(m.input_sessions[0].ended);
        assert!(!m.input_sessions[1].ended);
        // Further moves for the lifted finger are ignored.
        assert!(!m.touch_move(1, pos(0.0, 0.0), ts(3), pos(0.0, 0.0)));
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::unreadable_literal)]
mod autotest_generated {
    use azul_core::{
        drag::ScrollbarAxis, geom::PhysicalPositionI32, styled_dom::NodeHierarchyItemId,
        task::{SystemTick, SystemTickDiff, SystemTimeDiff},
    };

    use super::*;

    // ---------------------------------------------------------------- helpers

    /// Tick-based instant: 1 tick == 1 ms for `duration_to_millis`.
    fn ts(n: u64) -> CoreInstant {
        CoreInstant::Tick(SystemTick::new(n))
    }

    fn pos(x: f32, y: f32) -> LogicalPosition {
        LogicalPosition { x, y }
    }

    /// A sample with window-local == screen position (the common mouse case).
    fn sample(x: f32, y: f32, tick: u64) -> InputSample {
        InputSample {
            position: pos(x, y),
            screen_position: pos(x, y),
            timestamp: ts(tick),
            button_state: 0x01,
            event_id: 0,
            pressure: 0.5,
            tilt: (0.0, 0.0),
            touch_radius: (0.0, 0.0),
        }
    }

    /// A synthetic *ended* session — the only way to build a >2-click history,
    /// because `start_input_session` prunes all but the newest ended session.
    fn ended_session(session_id: u64, samples: Vec<InputSample>) -> InputSession {
        InputSession {
            samples,
            ended: true,
            session_id,
            window_position_at_start: WindowPosition::Uninitialized,
        }
    }

    /// Press at `from`, move to `to`, `hold_ms` apart — a session that
    /// `detect_drag` will accept (distance permitting).
    fn dragging_manager(
        from: LogicalPosition,
        to: LogicalPosition,
        hold_ms: u64,
    ) -> GestureAndDragManager {
        let mut m = GestureAndDragManager::new();
        m.start_input_session(from, ts(0), 0x01, WindowPosition::Uninitialized, from);
        let recorded = m.record_input_sample(to, ts(hold_ms), 0x01, to);
        assert!(recorded);
        m
    }

    // ------------------------------------------------- duration_to_millis (private)

    #[test]
    fn duration_to_millis_tick_zero_and_max_do_not_panic() {
        assert_eq!(
            duration_to_millis(CoreDuration::Tick(SystemTickDiff { tick_diff: 0 })),
            0
        );
        assert_eq!(
            duration_to_millis(CoreDuration::Tick(SystemTickDiff {
                tick_diff: u64::MAX
            })),
            u64::MAX
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn duration_to_millis_system_zero_and_sub_millisecond_floor() {
        assert_eq!(
            duration_to_millis(CoreDuration::System(SystemTimeDiff { secs: 0, nanos: 0 })),
            0
        );
        // 999_999 ns is under a millisecond => floors to 0, never rounds up.
        assert_eq!(
            duration_to_millis(CoreDuration::System(SystemTimeDiff {
                secs: 0,
                nanos: 999_999
            })),
            0
        );
        assert_eq!(
            duration_to_millis(CoreDuration::System(SystemTimeDiff {
                secs: 2,
                nanos: 500_000_000
            })),
            2500
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn duration_to_millis_system_max_truncates_instead_of_panicking() {
        // as_millis() is u128 and would be MAX*1000+999; the `as u64` cast
        // truncates rather than panicking or saturating. Lock the exact value
        // so a change to saturating semantics is caught.
        let d = CoreDuration::System(SystemTimeDiff {
            secs: u64::MAX,
            nanos: 999_999_999,
        });
        let expected = ((u64::MAX as u128) * 1000 + 999) as u64;
        assert_eq!(duration_to_millis(d), expected);
    }

    // ------------------------------------------------- WacomPadState::express_key

    #[test]
    fn express_key_out_of_range_index_is_false_not_a_shift_overflow() {
        // 1u32 << 32 would panic in debug; the `index < 32` guard must short-circuit.
        let pad = WacomPadState {
            express_keys: u32::MAX,
            touch_ring: 0.0,
            touch_ring_active: false,
            device_id: 0,
        };
        assert!(pad.express_key(31));
        assert!(!pad.express_key(32));
        assert!(!pad.express_key(33));
        assert!(!pad.express_key(u32::MAX));
    }

    #[test]
    fn express_key_default_pad_has_no_keys_held() {
        let pad = WacomPadState::default();
        for i in 0..40u32 {
            assert!(!pad.express_key(i), "bit {i} must be unset on a default pad");
        }
    }

    #[test]
    fn express_key_bitset_round_trips_every_bit() {
        for bit in 0..32u32 {
            let pad = WacomPadState {
                express_keys: 1u32 << bit,
                touch_ring: 0.0,
                touch_ring_active: false,
                device_id: 0,
            };
            for probe in 0..32u32 {
                assert_eq!(
                    pad.express_key(probe),
                    probe == bit,
                    "encode bit {bit} -> decode probe {probe}"
                );
            }
        }
    }

    // ------------------------------------------------- InputSession

    #[test]
    fn input_session_new_holds_its_construction_invariants() {
        let s = InputSession::new(
            u64::MAX,
            sample(1.0, 2.0, 7),
            WindowPosition::Initialized(PhysicalPositionI32::new(-5, 9)),
        );
        assert_eq!(s.session_id, u64::MAX);
        assert!(!s.ended);
        assert_eq!(s.samples.len(), 1);
        assert_eq!(s.first_sample(), s.last_sample());
        assert_eq!(
            s.window_position_at_start,
            WindowPosition::Initialized(PhysicalPositionI32::new(-5, 9))
        );
        assert_eq!(s.total_distance(), 0.0);
        assert_eq!(s.direct_distance(), Some(0.0));
        assert_eq!(s.duration_ms(), Some(0));
    }

    #[test]
    fn empty_session_getters_return_none_instead_of_panicking() {
        let s = InputSession {
            samples: Vec::new(),
            ended: false,
            session_id: 0,
            window_position_at_start: WindowPosition::Uninitialized,
        };
        assert!(s.first_sample().is_none());
        assert!(s.last_sample().is_none());
        assert!(s.duration_ms().is_none());
        assert!(s.direct_distance().is_none());
        assert_eq!(s.total_distance(), 0.0);
    }

    #[test]
    fn duration_ms_saturates_to_zero_when_time_runs_backwards() {
        // last sample is *earlier* than the first (reordered / skewed clock).
        let s = InputSession {
            samples: vec![sample(0.0, 0.0, 900), sample(0.0, 0.0, 100)],
            ended: false,
            session_id: 1,
            window_position_at_start: WindowPosition::Uninitialized,
        };
        assert_eq!(s.duration_ms(), Some(0));
    }

    #[cfg(feature = "std")]
    #[test]
    fn duration_ms_with_mismatched_instant_kinds_is_zero() {
        let mut first = sample(0.0, 0.0, 0);
        first.timestamp = CoreInstant::now(); // System variant
        let last = sample(0.0, 0.0, 5_000); // Tick variant
        let s = InputSession {
            samples: vec![first, last],
            ended: false,
            session_id: 1,
            window_position_at_start: WindowPosition::Uninitialized,
        };
        assert_eq!(s.duration_ms(), Some(0));
    }

    #[test]
    fn total_distance_sums_the_path_while_direct_distance_is_the_chord() {
        let s = InputSession {
            samples: vec![
                sample(0.0, 0.0, 0),
                sample(3.0, 0.0, 1),
                sample(3.0, 4.0, 2),
            ],
            ended: false,
            session_id: 1,
            window_position_at_start: WindowPosition::Uninitialized,
        };
        assert_eq!(s.total_distance(), 7.0);
        assert_eq!(s.direct_distance(), Some(5.0));
    }

    #[test]
    fn distances_with_nan_coordinates_are_nan_and_do_not_panic() {
        let s = InputSession {
            samples: vec![sample(0.0, 0.0, 0), sample(f32::NAN, f32::NAN, 1)],
            ended: false,
            session_id: 1,
            window_position_at_start: WindowPosition::Uninitialized,
        };
        assert!(s.total_distance().is_nan());
        assert!(s.direct_distance().is_some_and(f32::is_nan));
    }

    #[test]
    fn distances_at_f32_extremes_saturate_to_infinity_instead_of_panicking() {
        let s = InputSession {
            samples: vec![
                sample(-f32::MAX, -f32::MAX, 0),
                sample(f32::MAX, f32::MAX, 1),
            ],
            ended: false,
            session_id: 1,
            window_position_at_start: WindowPosition::Uninitialized,
        };
        assert!(s.total_distance().is_infinite());
        assert!(s.direct_distance().is_some_and(f32::is_infinite));
    }

    // ------------------------------------------------- construction

    #[test]
    fn new_manager_is_inert_and_every_detector_is_quiet() {
        let m = GestureAndDragManager::new();
        assert_eq!(m.session_count(), 0);
        assert_eq!(m.debug_counts(), (0, 0));
        assert!(m.current_session_id().is_none());
        assert!(m.get_current_session().is_none());
        assert!(m.get_current_mouse_position().is_none());
        assert!(m.get_pen_state().is_none());
        assert!(m.get_previous_pen_state().is_none());
        assert!(m.get_pad_state().is_none());
        assert!(m.get_drag_context().is_none());
        assert!(m.detect_drag().is_none());
        assert!(m.detect_long_press().is_none());
        assert!(!m.detect_double_click());
        assert!(m.get_drag_direction().is_none());
        assert!(m.get_gesture_velocity().is_none());
        assert!(!m.is_swipe());
        assert!(m.detect_swipe_direction().is_none());
        assert!(m.detect_pinch().is_none());
        assert!(m.detect_rotation().is_none());
        assert!(m.get_drag_delta().is_none());
        assert!(m.get_drag_delta_screen().is_none());
        assert!(m.get_drag_delta_screen_incremental().is_none());
        assert!(m.get_window_position_at_session_start().is_none());
        assert!(m.get_window_drag_delta().is_none());
        assert!(m.get_window_position_from_drag().is_none());
        assert!(m.get_scrollbar_scroll_offset().is_none());
        assert!(!m.is_dragging());
        assert!(!m.is_text_selection_dragging());
        assert!(!m.is_scrollbar_dragging());
        assert!(!m.is_node_drag_active());
        assert!(!m.is_window_dragging());
        assert!(!m.is_file_dropping());
        assert!(!m.is_node_dragging(DomId::ROOT_ID, NodeId::ZERO));
        // Documented default for "no history at all".
        assert_eq!(m.detect_click_count(), 1);
        assert_eq!(m, GestureAndDragManager::default());
    }

    #[test]
    fn with_config_keeps_extreme_thresholds_verbatim_and_still_starts_at_session_1() {
        let cfg = GestureDetectionConfig {
            drag_distance_threshold: f32::NAN,
            double_click_time_threshold_ms: u64::MAX,
            double_click_distance_threshold: f32::INFINITY,
            long_press_time_threshold_ms: 0,
            long_press_distance_threshold: -1.0,
            min_samples_for_gesture: usize::MAX,
            swipe_velocity_threshold: 0.0,
            pinch_scale_threshold: f32::MAX,
            rotation_angle_threshold: -0.0,
            sample_cleanup_interval_ms: 0,
        };
        let mut m = GestureAndDragManager::with_config(cfg);
        assert!(m.config.drag_distance_threshold.is_nan());
        assert_eq!(m.config.double_click_time_threshold_ms, u64::MAX);
        assert_eq!(m.config.min_samples_for_gesture, usize::MAX);
        assert_eq!(m.session_count(), 0);
        let id = m.start_input_session(
            pos(0.0, 0.0),
            ts(0),
            0x01,
            WindowPosition::Uninitialized,
            pos(0.0, 0.0),
        );
        assert_eq!(id, 1, "with_config must not disturb the session counter");
        // min_samples_for_gesture == usize::MAX can never be reached => no drag.
        assert!(m.detect_drag().is_none());
    }

    // ------------------------------------------------- session recording

    #[test]
    fn session_ids_are_monotonic_starting_at_one() {
        let mut m = GestureAndDragManager::new();
        for expected in 1..=5u64 {
            let id = m.start_input_session(
                pos(0.0, 0.0),
                ts(expected),
                0x01,
                WindowPosition::Uninitialized,
                pos(0.0, 0.0),
            );
            assert_eq!(id, expected);
            assert_eq!(m.current_session_id(), Some(expected));
            m.end_current_session();
        }
    }

    #[test]
    fn session_id_counter_at_the_u64_boundary_does_not_overflow() {
        let mut m = GestureAndDragManager::new();
        m.next_session_id = u64::MAX - 1;
        let id = m.start_input_session(
            pos(0.0, 0.0),
            ts(0),
            0xFF,
            WindowPosition::Uninitialized,
            pos(0.0, 0.0),
        );
        assert_eq!(id, u64::MAX - 1);
        assert_eq!(m.next_session_id, u64::MAX);
    }

    #[test]
    fn recording_without_or_after_a_session_returns_false() {
        let mut m = GestureAndDragManager::new();
        assert!(!m.record_input_sample(pos(1.0, 1.0), ts(1), 0x01, pos(1.0, 1.0)));
        m.start_input_session(
            pos(0.0, 0.0),
            ts(0),
            0x01,
            WindowPosition::Uninitialized,
            pos(0.0, 0.0),
        );
        assert!(m.record_input_sample(pos(1.0, 1.0), ts(1), 0x01, pos(1.0, 1.0)));
        m.end_current_session();
        assert!(!m.record_input_sample(pos(2.0, 2.0), ts(2), 0x01, pos(2.0, 2.0)));
        // Ending twice is idempotent, and ending nothing must not panic.
        m.end_current_session();
        m.clear_all_sessions();
        m.end_current_session();
        assert_eq!(m.session_count(), 0);
    }

    #[test]
    fn sample_count_stays_bounded_by_max_samples_per_session() {
        let mut m = GestureAndDragManager::new();
        m.start_input_session(
            pos(0.0, 0.0),
            ts(0),
            0x01,
            WindowPosition::Uninitialized,
            pos(0.0, 0.0),
        );
        for i in 1..=(MAX_SAMPLES_PER_SESSION as u64 + 200) {
            assert!(m.record_input_sample(pos(i as f32, 0.0), ts(i), 0x01, pos(i as f32, 0.0)));
            assert!(
                m.get_current_session().unwrap().samples.len() <= MAX_SAMPLES_PER_SESSION,
                "sample buffer grew past MAX_SAMPLES_PER_SESSION at i={i}"
            );
        }
        // The newest sample always survives the drain.
        let last = m.get_current_mouse_position().unwrap();
        assert_eq!(last.x, (MAX_SAMPLES_PER_SESSION + 200) as f32);
    }

    #[test]
    fn pen_samples_accept_nan_inf_and_extreme_values() {
        let mut m = GestureAndDragManager::new();
        let id = m.start_input_session_with_pen(
            pos(f32::NAN, f32::INFINITY),
            ts(0),
            0xFF,
            u64::MAX,
            f32::NAN,
            (f32::INFINITY, f32::NEG_INFINITY),
            (-f32::MAX, f32::MAX),
            WindowPosition::Uninitialized,
            pos(f32::NEG_INFINITY, f32::NAN),
        );
        assert_eq!(id, 1);
        assert!(m.record_input_sample_with_pen(
            pos(0.0, 0.0),
            ts(u64::MAX),
            0x00,
            0,
            -1.0e30,
            (f32::NAN, f32::NAN),
            (f32::NAN, f32::NAN),
            pos(0.0, 0.0),
        ));
        let session = m.get_current_session().unwrap();
        assert_eq!(session.samples.len(), 2);
        let first = session.first_sample().unwrap();
        assert!(first.pressure.is_nan());
        assert!(first.tilt.0.is_infinite());
        assert_eq!(first.button_state, 0xFF);
        assert_eq!(first.event_id, u64::MAX);
        // ts(u64::MAX) - ts(0) fits: duration_since is a saturating u64 sub.
        assert_eq!(session.duration_ms(), Some(u64::MAX));
        // NaN/inf coordinates must not make any detector panic. `hypot(NaN, inf)`
        // is `+inf` per IEEE-754, so this DOES read as a drag — but only with a
        // non-finite distance, never a plausible-looking finite one.
        assert!(m.detect_drag().is_none_or(|d| !d.direct_distance.is_finite()));
        assert!(m.get_drag_direction().is_some());
    }

    #[test]
    fn starting_a_session_prunes_all_but_the_newest_ended_session() {
        let mut m = GestureAndDragManager::new();
        for tick in [0u64, 10, 20] {
            m.start_input_session(
                pos(0.0, 0.0),
                ts(tick),
                0x01,
                WindowPosition::Uninitialized,
                pos(0.0, 0.0),
            );
            m.end_current_session();
        }
        // Bounded growth: never more than "one ended + one live" session.
        assert_eq!(m.session_count(), 2);
        assert_eq!(m.input_sessions[0].session_id, 2);
        assert_eq!(m.input_sessions[1].session_id, 3);
        // KNOWN LIMITATION: because the history is pruned to a single ended
        // session, a genuine triple-click through the public API can only ever
        // report 2. detect_click_count()'s triple-click arm is unreachable here.
        assert_eq!(m.detect_click_count(), 2);
    }

    // ------------------------------------------------- touch sessions

    #[test]
    fn touch_ids_at_zero_and_u64_max_are_tracked_independently() {
        let mut m = GestureAndDragManager::new();
        m.touch_down(
            0,
            pos(0.0, 0.0),
            ts(0),
            WindowPosition::Uninitialized,
            pos(0.0, 0.0),
        );
        m.touch_down(
            u64::MAX,
            pos(50.0, 0.0),
            ts(1),
            WindowPosition::Uninitialized,
            pos(50.0, 0.0),
        );
        assert_eq!(m.session_count(), 2);
        assert!(m.touch_move(0, pos(1.0, 1.0), ts(2), pos(1.0, 1.0)));
        assert!(m.touch_move(u64::MAX, pos(60.0, 0.0), ts(3), pos(60.0, 0.0)));
        assert_eq!(m.input_sessions[0].samples.len(), 2);
        assert_eq!(m.input_sessions[1].samples.len(), 2);
        m.touch_up(0, pos(1.0, 1.0), ts(4), pos(1.0, 1.0));
        assert!(m.input_sessions[0].ended);
        assert!(!m.input_sessions[1].ended);
    }

    #[test]
    fn touch_events_for_unknown_ids_are_ignored_without_panicking() {
        let mut m = GestureAndDragManager::new();
        assert!(!m.touch_move(42, pos(0.0, 0.0), ts(0), pos(0.0, 0.0)));
        m.touch_up(42, pos(0.0, 0.0), ts(1), pos(0.0, 0.0));
        m.touch_cancel_all(); // nothing to cancel
        assert_eq!(m.session_count(), 0);
    }

    #[test]
    fn a_repeated_touch_down_for_the_same_id_rebinds_to_the_newest_session() {
        let mut m = GestureAndDragManager::new();
        m.touch_down(
            7,
            pos(0.0, 0.0),
            ts(0),
            WindowPosition::Uninitialized,
            pos(0.0, 0.0),
        );
        m.touch_down(
            7,
            pos(9.0, 9.0),
            ts(1),
            WindowPosition::Uninitialized,
            pos(9.0, 9.0),
        );
        assert_eq!(m.touch_sessions.len(), 1, "the id map must not grow");
        assert_eq!(m.session_count(), 2);
        assert_eq!(m.touch_sessions.get(&7).copied(), Some(2));
        // touch_up ends only the session the id currently maps to; the orphaned
        // first session stays open until clear_old_sessions() reaps it.
        m.touch_up(7, pos(9.0, 9.0), ts(2), pos(9.0, 9.0));
        assert!(!m.input_sessions[0].ended);
        assert!(m.input_sessions[1].ended);
        assert!(m.touch_sessions.is_empty());
    }

    #[test]
    fn touch_cancel_all_ends_every_finger_and_empties_the_id_map() {
        let mut m = GestureAndDragManager::new();
        for id in 0..3u64 {
            m.touch_down(
                id,
                pos(id as f32 * 10.0, 0.0),
                ts(id),
                WindowPosition::Uninitialized,
                pos(id as f32 * 10.0, 0.0),
            );
        }
        m.touch_cancel_all();
        assert!(m.touch_sessions.is_empty());
        assert!(m.input_sessions.iter().all(|s| s.ended));
        assert!(!m.touch_move(1, pos(0.0, 0.0), ts(9), pos(0.0, 0.0)));
    }

    #[test]
    fn touch_moves_after_clear_all_sessions_are_dropped_not_resurrected() {
        let mut m = GestureAndDragManager::new();
        m.touch_down(
            1,
            pos(0.0, 0.0),
            ts(0),
            WindowPosition::Uninitialized,
            pos(0.0, 0.0),
        );
        m.clear_all_sessions();
        // The id->session map still holds a dangling entry, but the by-id
        // lookup finds no session, so nothing is recorded and nothing panics.
        assert!(!m.touch_move(1, pos(5.0, 5.0), ts(1), pos(5.0, 5.0)));
        assert_eq!(m.session_count(), 0);
    }

    #[test]
    fn record_sample_for_session_rejects_unknown_and_ended_sessions() {
        let mut m = GestureAndDragManager::new();
        assert!(!m.record_sample_for_session(u64::MAX, pos(0.0, 0.0), ts(0), pos(0.0, 0.0)));
        let id = m.start_input_session(
            pos(0.0, 0.0),
            ts(0),
            0x01,
            WindowPosition::Uninitialized,
            pos(0.0, 0.0),
        );
        assert!(m.record_sample_for_session(id, pos(1.0, 0.0), ts(1), pos(1.0, 0.0)));
        assert!(!m.record_sample_for_session(0, pos(1.0, 0.0), ts(1), pos(1.0, 0.0)));
        m.end_current_session();
        assert!(!m.record_sample_for_session(id, pos(2.0, 0.0), ts(2), pos(2.0, 0.0)));
        assert_eq!(m.input_sessions[0].samples.len(), 2);
    }

    #[test]
    fn record_sample_for_session_is_also_bounded_by_max_samples() {
        let mut m = GestureAndDragManager::new();
        m.touch_down(
            1,
            pos(0.0, 0.0),
            ts(0),
            WindowPosition::Uninitialized,
            pos(0.0, 0.0),
        );
        for i in 1..=(MAX_SAMPLES_PER_SESSION as u64 + 150) {
            assert!(m.touch_move(1, pos(i as f32, 0.0), ts(i), pos(i as f32, 0.0)));
        }
        assert!(m.input_sessions[0].samples.len() <= MAX_SAMPLES_PER_SESSION);
    }

    // ------------------------------------------------- cleanup

    #[test]
    fn clear_old_sessions_reaps_stale_sessions_and_their_long_press_ids() {
        let mut m = GestureAndDragManager::new();
        let old = m.start_input_session(
            pos(0.0, 0.0),
            ts(0),
            0x01,
            WindowPosition::Uninitialized,
            pos(0.0, 0.0),
        );
        m.end_current_session();
        m.mark_long_press_callback_invoked(old);
        let fresh = m.start_input_session(
            pos(0.0, 0.0),
            ts(10_000),
            0x01,
            WindowPosition::Uninitialized,
            pos(0.0, 0.0),
        );
        m.mark_long_press_callback_invoked(fresh);
        assert_eq!(m.debug_counts(), (2, 2));

        // Now is 10_050 ticks: `old` is 10s stale (> 2000ms), `fresh` is 50ms old.
        m.clear_old_sessions(ts(10_050));
        assert_eq!(m.session_count(), 1);
        assert_eq!(m.current_session_id(), Some(fresh));
        assert_eq!(
            m.debug_counts(),
            (1, 1),
            "long-press bookkeeping must not grow unboundedly"
        );
    }

    #[test]
    fn clear_old_sessions_drops_sessions_that_have_no_samples() {
        let mut m = GestureAndDragManager::new();
        m.input_sessions.push(InputSession {
            samples: Vec::new(),
            ended: false,
            session_id: 99,
            window_position_at_start: WindowPosition::Uninitialized,
        });
        m.clear_old_sessions(ts(0));
        assert_eq!(m.session_count(), 0);
    }

    #[test]
    fn clear_old_sessions_with_a_backwards_clock_keeps_everything() {
        let mut m = GestureAndDragManager::new();
        m.start_input_session(
            pos(0.0, 0.0),
            ts(5_000),
            0x01,
            WindowPosition::Uninitialized,
            pos(0.0, 0.0),
        );
        // `now` is *before* the sample: duration_since saturates to 0 => age 0.
        m.clear_old_sessions(ts(0));
        assert_eq!(m.session_count(), 1);
    }

    #[test]
    fn clear_all_sessions_resets_both_counters() {
        let mut m = GestureAndDragManager::new();
        m.start_input_session(
            pos(0.0, 0.0),
            ts(0),
            0x01,
            WindowPosition::Uninitialized,
            pos(0.0, 0.0),
        );
        m.mark_current_long_press_invoked();
        assert_eq!(m.debug_counts(), (1, 1));
        m.clear_all_sessions();
        assert_eq!(m.debug_counts(), (0, 0));
        assert!(m.get_current_session().is_none());
    }

    #[test]
    fn long_press_invocation_marks_are_deduplicated() {
        let mut m = GestureAndDragManager::new();
        for _ in 0..100 {
            m.mark_long_press_callback_invoked(u64::MAX);
            m.mark_long_press_callback_invoked(0);
        }
        assert_eq!(m.debug_counts(), (0, 2));
        // Marking without a session is a no-op, not a panic.
        m.mark_current_long_press_invoked();
        assert_eq!(m.debug_counts(), (0, 2));
    }

    // ------------------------------------------------- drag / long-press detection

    #[test]
    fn detect_drag_fires_exactly_at_the_distance_threshold() {
        // hypot(3, 4) == 5.0 == drag_distance_threshold => `>=` must fire.
        let m = dragging_manager(pos(0.0, 0.0), pos(3.0, 4.0), 20);
        let drag = m.detect_drag().expect("distance == threshold must be a drag");
        assert_eq!(drag.direct_distance, 5.0);
        assert_eq!(drag.total_distance, 5.0);
        assert_eq!(drag.sample_count, 2);
        assert_eq!(drag.duration_ms, 20);
        assert_eq!(drag.session_id, 1);
        assert_eq!(drag.start_position, pos(0.0, 0.0));
        assert_eq!(drag.current_position, pos(3.0, 4.0));

        // Just below the threshold: no drag.
        let m = dragging_manager(pos(0.0, 0.0), pos(4.9, 0.0), 20);
        assert!(m.detect_drag().is_none());
    }

    #[test]
    fn detect_drag_with_nan_movement_returns_none() {
        let m = dragging_manager(pos(0.0, 0.0), pos(f32::NAN, f32::NAN), 20);
        assert!(
            m.detect_drag().is_none(),
            "NaN distance is never >= threshold"
        );
    }

    #[test]
    fn detect_drag_needs_min_samples_for_gesture() {
        let mut m = GestureAndDragManager::new();
        m.start_input_session(
            pos(0.0, 0.0),
            ts(0),
            0x01,
            WindowPosition::Uninitialized,
            pos(500.0, 500.0),
        );
        assert!(m.detect_drag().is_none(), "one sample is not a gesture");
    }

    #[test]
    fn detect_long_press_honours_time_and_distance_thresholds() {
        // Held 500ms (== threshold) without moving => long press.
        let m = dragging_manager(pos(10.0, 10.0), pos(10.0, 10.0), 500);
        let lp = m.detect_long_press().expect("500ms hold is a long press");
        assert_eq!(lp.duration_ms, 500);
        assert_eq!(lp.position, pos(10.0, 10.0));
        assert!(!lp.callback_invoked);
        assert_eq!(lp.session_id, 1);

        // One ms short => not yet.
        let m = dragging_manager(pos(10.0, 10.0), pos(10.0, 10.0), 499);
        assert!(m.detect_long_press().is_none());

        // Long enough but moved too far (> 10px).
        let m = dragging_manager(pos(0.0, 0.0), pos(11.0, 0.0), 800);
        assert!(m.detect_long_press().is_none());
    }

    #[test]
    fn detect_long_press_stops_at_button_up_and_after_being_marked() {
        let mut m = dragging_manager(pos(10.0, 10.0), pos(10.0, 10.0), 600);
        assert!(m.detect_long_press().is_some());

        m.mark_current_long_press_invoked();
        let lp = m.detect_long_press().expect("still held");
        assert!(
            lp.callback_invoked,
            "a marked long press must report callback_invoked"
        );

        m.end_current_session();
        assert!(
            m.detect_long_press().is_none(),
            "a released button cannot be a long press"
        );
    }

    // ------------------------------------------------- click counting

    #[test]
    fn detect_double_click_checks_both_timing_and_distance() {
        let mut m = GestureAndDragManager::new();
        m.input_sessions = vec![
            ended_session(1, vec![sample(10.0, 10.0, 0)]),
            ended_session(2, vec![sample(11.0, 11.0, 100)]),
        ];
        assert!(m.detect_double_click());

        // Too slow (501ms > 500ms).
        m.input_sessions[1].samples[0].timestamp = ts(501);
        assert!(!m.detect_double_click());

        // Fast, but too far apart (>= 5px).
        m.input_sessions[1].samples[0].timestamp = ts(100);
        m.input_sessions[1].samples[0].position = pos(100.0, 10.0);
        assert!(!m.detect_double_click());

        // Fast and close, but the second click is still held down.
        m.input_sessions[1].samples[0].position = pos(11.0, 11.0);
        m.input_sessions[1].ended = false;
        assert!(!m.detect_double_click());
    }

    #[test]
    fn detect_double_click_needs_two_sessions() {
        let mut m = GestureAndDragManager::new();
        m.input_sessions = vec![ended_session(1, vec![sample(0.0, 0.0, 0)])];
        assert!(!m.detect_double_click());
    }

    #[test]
    fn detect_click_count_counts_up_to_three_and_stops_at_the_first_gap() {
        let mut m = GestureAndDragManager::new();
        // Three ended clicks, each 100ms apart at (nearly) the same point.
        m.input_sessions = vec![
            ended_session(1, vec![sample(10.0, 10.0, 0)]),
            ended_session(2, vec![sample(10.0, 11.0, 100)]),
            ended_session(3, vec![sample(11.0, 10.0, 200)]),
        ];
        assert_eq!(m.detect_click_count(), 3);

        // Break the middle gap in *time*: only the newest pair counts.
        m.input_sessions[2].samples[0].timestamp = ts(900);
        assert_eq!(m.detect_click_count(), 1);

        // A backwards clock does NOT break the chain: duration_since saturates
        // to 0, which reads as "no gap at all" => the click still counts.
        m.input_sessions[2].samples[0].timestamp = ts(200);
        m.input_sessions[0].samples[0].timestamp = ts(u64::MAX);
        assert_eq!(m.detect_click_count(), 3);

        // Break the oldest gap in *distance*.
        m.input_sessions[0].samples[0].timestamp = ts(0);
        m.input_sessions[0].samples[0].position = pos(500.0, 500.0);
        assert_eq!(m.detect_click_count(), 2);
    }

    #[test]
    fn detect_click_count_ignores_live_sessions_and_defaults_to_one() {
        let mut m = GestureAndDragManager::new();
        // Only a live (un-ended) session => nothing to count => 1.
        m.start_input_session(
            pos(0.0, 0.0),
            ts(0),
            0x01,
            WindowPosition::Uninitialized,
            pos(0.0, 0.0),
        );
        assert_eq!(m.detect_click_count(), 1);
        assert_eq!(GestureAndDragManager::new().detect_click_count(), 1);
    }

    #[test]
    fn detect_click_count_with_empty_sample_vec_does_not_panic() {
        let mut m = GestureAndDragManager::new();
        m.input_sessions = vec![
            ended_session(1, Vec::new()),
            ended_session(2, vec![sample(0.0, 0.0, 10)]),
        ];
        assert_eq!(m.detect_click_count(), 1);
        assert!(!m.detect_double_click());
    }

    // ------------------------------------------------- direction / velocity / swipe

    #[test]
    fn drag_direction_is_deterministic_for_stationary_and_nan_input() {
        // No movement at all: dx == dy == 0 => documented fallback is Up.
        let m = dragging_manager(pos(5.0, 5.0), pos(5.0, 5.0), 10);
        assert_eq!(m.get_drag_direction(), Some(GestureDirection::Up));

        // NaN deltas compare false everywhere => same deterministic fallback.
        let m = dragging_manager(pos(0.0, 0.0), pos(f32::NAN, f32::NAN), 10);
        assert_eq!(m.get_drag_direction(), Some(GestureDirection::Up));
    }

    #[test]
    fn drag_direction_picks_the_dominant_axis() {
        let cases = [
            (pos(100.0, 1.0), GestureDirection::Right),
            (pos(-100.0, 1.0), GestureDirection::Left),
            (pos(1.0, 100.0), GestureDirection::Down),
            (pos(1.0, -100.0), GestureDirection::Up),
            // Perfect diagonal: |dx| > |dy| is false => vertical wins.
            (pos(50.0, 50.0), GestureDirection::Down),
        ];
        for (to, expected) in cases {
            let m = dragging_manager(pos(0.0, 0.0), to, 10);
            assert_eq!(
                m.get_drag_direction(),
                Some(expected),
                "drag to ({}, {})",
                to.x,
                to.y
            );
        }
    }

    #[test]
    fn gesture_velocity_returns_none_instead_of_dividing_by_zero() {
        // Two samples with the SAME timestamp => duration 0 => no velocity.
        let m = dragging_manager(pos(0.0, 0.0), pos(100.0, 0.0), 0);
        assert!(m.get_gesture_velocity().is_none());
        assert!(!m.is_swipe());
        assert!(m.detect_swipe_direction().is_none());

        // A single sample is not enough either.
        let mut m = GestureAndDragManager::new();
        m.start_input_session(
            pos(0.0, 0.0),
            ts(0),
            0x01,
            WindowPosition::Uninitialized,
            pos(0.0, 0.0),
        );
        assert!(m.get_gesture_velocity().is_none());
    }

    #[test]
    fn swipe_needs_velocity_above_the_configured_threshold() {
        // 60px in 100ms == 600 px/s > 500 px/s.
        let fast = dragging_manager(pos(0.0, 0.0), pos(60.0, 0.0), 100);
        assert!(fast.get_gesture_velocity().unwrap() > 500.0);
        assert!(fast.is_swipe());
        assert_eq!(
            fast.detect_swipe_direction(),
            Some(GestureDirection::Right)
        );

        // 40px in 100ms == 400 px/s < 500 px/s.
        let slow = dragging_manager(pos(0.0, 0.0), pos(0.0, -40.0), 100);
        assert!(!slow.is_swipe());
        assert!(slow.detect_swipe_direction().is_none());
    }

    #[test]
    fn gesture_velocity_with_infinite_travel_saturates_to_infinity() {
        let m = dragging_manager(pos(-f32::MAX, 0.0), pos(f32::MAX, 0.0), 1);
        let v = m.get_gesture_velocity().expect("two samples, 1ms apart");
        assert!(v.is_infinite(), "expected saturation to +inf, got {v}");
        assert!(m.is_swipe());
    }

    // ------------------------------------------------- pinch / rotation

    #[test]
    fn pinch_and_rotation_ignore_sequential_mouse_sessions() {
        // Click, release, then press-and-drag: two sessions, but the first is
        // ended — this must NOT be read as a two-finger gesture.
        let mut m = GestureAndDragManager::new();
        m.start_input_session(
            pos(0.0, 0.0),
            ts(0),
            0x01,
            WindowPosition::Uninitialized,
            pos(0.0, 0.0),
        );
        m.end_current_session();
        m.start_input_session(
            pos(200.0, 0.0),
            ts(10),
            0x01,
            WindowPosition::Uninitialized,
            pos(200.0, 0.0),
        );
        m.record_input_sample(pos(400.0, 0.0), ts(20), 0x01, pos(400.0, 0.0));
        assert_eq!(m.session_count(), 2);
        assert!(m.detect_pinch().is_none(), "an ended session is not a finger");
        assert!(m.detect_rotation().is_none());
    }

    #[test]
    fn pinch_returns_none_when_the_fingers_start_on_top_of_each_other() {
        let mut m = GestureAndDragManager::new();
        m.touch_down(
            1,
            pos(100.0, 100.0),
            ts(0),
            WindowPosition::Uninitialized,
            pos(100.0, 100.0),
        );
        m.touch_down(
            2,
            pos(100.5, 100.0),
            ts(1),
            WindowPosition::Uninitialized,
            pos(100.5, 100.0),
        );
        // initial_distance 0.5 < 1.0 => division guard returns None.
        m.touch_move(1, pos(0.0, 100.0), ts(2), pos(0.0, 100.0));
        assert!(m.detect_pinch().is_none());
    }

    #[test]
    fn pinch_below_the_scale_threshold_is_not_reported() {
        let mut m = GestureAndDragManager::new();
        m.touch_down(
            1,
            pos(100.0, 100.0),
            ts(0),
            WindowPosition::Uninitialized,
            pos(100.0, 100.0),
        );
        m.touch_down(
            2,
            pos(200.0, 100.0),
            ts(1),
            WindowPosition::Uninitialized,
            pos(200.0, 100.0),
        );
        // 100px -> 105px is a 5% change; the threshold is 10%.
        m.touch_move(2, pos(205.0, 100.0), ts(2), pos(205.0, 100.0));
        assert!(m.detect_pinch().is_none());
    }

    #[test]
    fn pinch_in_reports_a_scale_below_one() {
        let mut m = GestureAndDragManager::new();
        m.touch_down(
            1,
            pos(0.0, 0.0),
            ts(0),
            WindowPosition::Uninitialized,
            pos(0.0, 0.0),
        );
        m.touch_down(
            2,
            pos(200.0, 0.0),
            ts(1),
            WindowPosition::Uninitialized,
            pos(200.0, 0.0),
        );
        m.touch_move(1, pos(50.0, 0.0), ts(10), pos(50.0, 0.0));
        m.touch_move(2, pos(150.0, 0.0), ts(11), pos(150.0, 0.0));
        let p = m.detect_pinch().expect("200px -> 100px is a pinch in");
        assert_eq!(p.initial_distance, 200.0);
        assert_eq!(p.current_distance, 100.0);
        assert_eq!(p.scale, 0.5);
        assert_eq!(p.center, pos(100.0, 0.0));
        assert_eq!(p.duration_ms, 10);
    }

    #[test]
    fn pinch_with_infinite_coordinates_saturates_instead_of_panicking() {
        let mut m = GestureAndDragManager::new();
        m.touch_down(
            1,
            pos(0.0, 0.0),
            ts(0),
            WindowPosition::Uninitialized,
            pos(0.0, 0.0),
        );
        m.touch_down(
            2,
            pos(10.0, 0.0),
            ts(1),
            WindowPosition::Uninitialized,
            pos(10.0, 0.0),
        );
        // The spread overflows f32: MAX - (-MAX) == +inf.
        m.touch_move(1, pos(-f32::MAX, 0.0), ts(2), pos(-f32::MAX, 0.0));
        m.touch_move(2, pos(f32::MAX, 0.0), ts(3), pos(f32::MAX, 0.0));
        let p = m.detect_pinch().expect("an overflowing spread is still a pinch");
        assert!(
            !p.scale.is_finite(),
            "expected a saturated (non-finite) scale, got {}",
            p.scale
        );
        assert!(!p.scale.is_nan());
    }

    #[test]
    fn pinch_and_rotation_with_nan_coordinates_never_panic() {
        let mut m = GestureAndDragManager::new();
        m.touch_down(
            1,
            pos(f32::NAN, f32::NAN),
            ts(0),
            WindowPosition::Uninitialized,
            pos(f32::NAN, f32::NAN),
        );
        m.touch_down(
            2,
            pos(200.0, 100.0),
            ts(1),
            WindowPosition::Uninitialized,
            pos(200.0, 100.0),
        );
        // Whatever the detectors decide, they must not produce a *finite*
        // (i.e. plausible-looking but garbage) scale or angle from NaN input.
        assert!(m.detect_pinch().is_none_or(|p| !p.scale.is_finite()));
        assert!(m
            .detect_rotation()
            .is_none_or(|r| !r.angle_radians.is_finite()));
    }

    #[test]
    fn rotation_normalisation_terminates_for_extreme_coordinates() {
        // The angle-wrap `while` loops must not spin: atan2 is bounded to
        // [-PI, PI], so angle_diff can never be infinite.
        let mut m = GestureAndDragManager::new();
        m.touch_down(
            1,
            pos(-f32::MAX, -f32::MAX),
            ts(0),
            WindowPosition::Uninitialized,
            pos(0.0, 0.0),
        );
        m.touch_down(
            2,
            pos(f32::MAX, f32::MAX),
            ts(1),
            WindowPosition::Uninitialized,
            pos(0.0, 0.0),
        );
        m.touch_move(2, pos(-f32::MAX, f32::MAX), ts(2), pos(0.0, 0.0));
        let r = m.detect_rotation();
        assert!(r.is_none_or(|r| r.angle_radians.abs() <= core::f32::consts::PI + 1.0e-4));
    }

    #[test]
    fn rotation_reports_the_signed_angle_between_the_two_fingers() {
        let mut m = GestureAndDragManager::new();
        m.touch_down(
            1,
            pos(0.0, 0.0),
            ts(0),
            WindowPosition::Uninitialized,
            pos(0.0, 0.0),
        );
        m.touch_down(
            2,
            pos(10.0, 0.0),
            ts(1),
            WindowPosition::Uninitialized,
            pos(10.0, 0.0),
        );
        // Finger 2 swings from +x (angle 0) to +y (angle PI/2) around finger 1.
        m.touch_move(2, pos(0.0, 10.0), ts(50), pos(0.0, 10.0));
        let r = m.detect_rotation().expect("a quarter turn is a rotation");
        assert!(
            (r.angle_radians - core::f32::consts::FRAC_PI_2).abs() < 1.0e-4,
            "expected ~PI/2, got {}",
            r.angle_radians
        );
        assert_eq!(r.center, pos(0.0, 5.0));
    }

    #[test]
    fn rotation_below_the_angle_threshold_is_not_reported() {
        let mut m = GestureAndDragManager::new();
        m.touch_down(
            1,
            pos(0.0, 0.0),
            ts(0),
            WindowPosition::Uninitialized,
            pos(0.0, 0.0),
        );
        m.touch_down(
            2,
            pos(1000.0, 0.0),
            ts(1),
            WindowPosition::Uninitialized,
            pos(1000.0, 0.0),
        );
        // ~0.05 rad, under the 0.1 rad threshold.
        m.touch_move(2, pos(1000.0, 50.0), ts(2), pos(1000.0, 50.0));
        assert!(m.detect_rotation().is_none());
    }

    // ------------------------------------------------- native gesture override

    #[test]
    fn injected_native_gestures_win_over_the_in_process_detector() {
        let mut m = GestureAndDragManager::new();

        m.inject_native_gesture(NativeGestureEvent::DoubleClick);
        assert!(m.detect_double_click(), "no sessions, but the OS said so");
        m.clear_native_gesture();
        assert!(!m.detect_double_click());

        let lp = DetectedLongPress {
            position: pos(3.0, 4.0),
            duration_ms: u64::MAX,
            callback_invoked: true,
            session_id: u64::MAX,
        };
        m.inject_native_gesture(NativeGestureEvent::LongPress(lp));
        assert_eq!(m.detect_long_press(), Some(lp));

        m.inject_native_gesture(NativeGestureEvent::Swipe(GestureDirection::Left));
        assert_eq!(m.detect_swipe_direction(), Some(GestureDirection::Left));
        assert!(
            !m.is_swipe(),
            "is_swipe() is velocity-only and ignores the native override"
        );

        let pinch = DetectedPinch {
            scale: f32::INFINITY,
            center: pos(0.0, 0.0),
            initial_distance: 0.0,
            current_distance: f32::NAN,
            duration_ms: 0,
        };
        m.inject_native_gesture(NativeGestureEvent::Pinch(pinch));
        let got = m.detect_pinch().expect("native pinch is passed through");
        assert!(got.scale.is_infinite());

        let rot = DetectedRotation {
            angle_radians: -core::f32::consts::PI,
            center: pos(1.0, 1.0),
            duration_ms: 7,
        };
        m.inject_native_gesture(NativeGestureEvent::Rotation(rot));
        assert_eq!(m.detect_rotation(), Some(rot));

        m.clear_native_gesture();
        assert!(m.detect_long_press().is_none());
        assert!(m.detect_pinch().is_none());
        assert!(m.detect_rotation().is_none());
        assert!(m.detect_swipe_direction().is_none());
    }

    // ------------------------------------------------- pen / pad state

    #[test]
    fn pen_state_stores_extremes_verbatim_and_tracks_the_previous_state() {
        let mut m = GestureAndDragManager::new();
        m.update_pen_state(
            pos(1.0, 2.0),
            f32::NAN,
            (f32::INFINITY, f32::NEG_INFINITY),
            true,
            true,
            true,
            u64::MAX,
        );
        assert!(m.pen_event_pending);
        assert!(m.get_previous_pen_state().is_none());
        let pen = *m.get_pen_state().expect("pen state was just set");
        assert!(pen.pressure.is_nan());
        assert!(pen.tilt.x_tilt.is_infinite());
        assert!(pen.tilt.y_tilt.is_infinite());
        assert!(pen.in_contact && pen.is_eraser && pen.barrel_button_pressed);
        assert_eq!(pen.device_id, u64::MAX);
        // The short form must zero the extended axes.
        assert_eq!(pen.tangential_pressure, 0.0);
        assert_eq!(pen.barrel_roll_rad, 0.0);
        assert_eq!(pen.tool_id, 0);

        m.clear_pen_event_pending();
        assert!(!m.pen_event_pending);

        m.update_pen_state_full(
            pos(0.0, 0.0),
            1.0,
            (0.0, 0.0),
            false,
            false,
            false,
            0,
            f32::NAN,
            -f32::MAX,
            u32::MAX,
        );
        assert!(m.pen_event_pending);
        let prev = *m.get_previous_pen_state().expect("previous pen state kept");
        assert_eq!(prev.device_id, u64::MAX);
        let now = *m.get_pen_state().unwrap();
        assert!(now.tangential_pressure.is_nan());
        assert_eq!(now.barrel_roll_rad, -f32::MAX);
        assert_eq!(now.tool_id, u32::MAX);

        m.clear_pen_state();
        assert!(m.get_pen_state().is_none());
        assert_eq!(m.get_previous_pen_state().map(|p| p.tool_id), Some(u32::MAX));
        assert!(m.pen_event_pending);

        // Clearing twice must not panic and must not resurrect a state.
        m.clear_pen_state();
        assert!(m.get_pen_state().is_none());
        assert!(m.get_previous_pen_state().is_none());
    }

    #[test]
    fn pad_state_round_trips_and_clears() {
        let mut m = GestureAndDragManager::new();
        assert!(m.get_pad_state().is_none());
        m.update_pad_state(WacomPadState {
            express_keys: 0b1010,
            touch_ring: f32::NAN,
            touch_ring_active: true,
            device_id: u64::MAX,
        });
        let pad = *m.get_pad_state().expect("pad state was just set");
        assert!(!pad.express_key(0));
        assert!(pad.express_key(1));
        assert!(!pad.express_key(2));
        assert!(pad.express_key(3));
        assert!(pad.touch_ring.is_nan());
        assert_eq!(pad.device_id, u64::MAX);
        m.clear_pad_state();
        assert!(m.get_pad_state().is_none());
        m.clear_pad_state();
        assert!(m.get_pad_state().is_none());
    }

    // ------------------------------------------------- drag deltas

    #[test]
    fn drag_deltas_use_window_local_and_screen_coordinates_independently() {
        let mut m = GestureAndDragManager::new();
        m.start_input_session(
            pos(10.0, 10.0),
            ts(0),
            0x01,
            WindowPosition::Initialized(PhysicalPositionI32::new(100, 100)),
            pos(110.0, 110.0),
        );
        // One sample: totals exist, but there is no *incremental* delta yet.
        assert_eq!(m.get_drag_delta(), Some((0.0, 0.0)));
        assert_eq!(m.get_drag_delta_screen(), Some((0.0, 0.0)));
        assert!(m.get_drag_delta_screen_incremental().is_none());

        m.record_input_sample(pos(15.0, 10.0), ts(10), 0x01, pos(120.0, 130.0));
        m.record_input_sample(pos(20.0, 10.0), ts(20), 0x01, pos(125.0, 132.0));
        assert_eq!(m.get_drag_delta(), Some((10.0, 0.0)));
        assert_eq!(m.get_drag_delta_screen(), Some((15.0, 22.0)));
        assert_eq!(m.get_drag_delta_screen_incremental(), Some((5.0, 2.0)));
        assert_eq!(
            m.get_window_position_at_session_start(),
            Some(WindowPosition::Initialized(PhysicalPositionI32::new(
                100, 100
            )))
        );
        assert_eq!(m.get_current_mouse_position(), Some(pos(20.0, 10.0)));
    }

    #[test]
    fn drag_deltas_at_f32_extremes_stay_finite_or_saturate() {
        let m = dragging_manager(pos(-f32::MAX, -f32::MAX), pos(f32::MAX, f32::MAX), 5);
        let (dx, dy) = m.get_drag_delta().expect("two samples");
        assert!(dx.is_infinite() && dy.is_infinite());
        let (sx, sy) = m.get_drag_delta_screen().expect("two samples");
        assert!(sx.is_infinite() && sy.is_infinite());
    }

    // ------------------------------------------------- unified drag context

    #[test]
    fn activating_a_node_drag_without_a_detected_drag_is_a_no_op() {
        let mut m = GestureAndDragManager::new();
        // No session at all.
        m.activate_node_drag(DomId::ROOT_ID, NodeId::new(1), DragData::new(), None);
        assert!(!m.is_dragging());

        // A session that has not moved far enough to be a drag.
        let mut m = dragging_manager(pos(0.0, 0.0), pos(1.0, 1.0), 10);
        m.activate_node_drag(DomId::ROOT_ID, NodeId::new(1), DragData::new(), None);
        assert!(!m.is_node_drag_active());
        m.activate_window_drag(WindowPosition::Uninitialized, None);
        assert!(!m.is_window_dragging());
    }

    #[test]
    fn node_drag_context_tracks_its_own_node_and_drop_target() {
        let mut m = dragging_manager(pos(0.0, 0.0), pos(100.0, 0.0), 10);
        let mut data = DragData::new();
        data.set_text("payload");
        m.activate_node_drag(DomId::ROOT_ID, NodeId::new(4), data, None);

        assert!(m.is_dragging());
        assert!(m.is_node_drag_active());
        assert!(m.is_node_dragging(DomId::ROOT_ID, NodeId::new(4)));
        assert!(!m.is_node_dragging(DomId::ROOT_ID, NodeId::new(5)));
        assert!(!m.is_node_dragging(DomId { inner: 7 }, NodeId::new(4)));
        assert!(!m.is_window_dragging());
        assert!(!m.is_file_dropping());
        assert!(!m.is_text_selection_dragging());
        assert!(!m.is_scrollbar_dragging());
        assert!(m.get_window_drag_delta().is_none());
        assert!(m.get_scrollbar_scroll_offset().is_none());

        m.update_active_drag_positions(pos(42.0, -7.0));
        assert_eq!(
            m.get_drag_context().unwrap().current_position(),
            pos(42.0, -7.0)
        );

        m.update_drop_target(Some(azul_core::dom::DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(9))),
        }));
        let nd = m
            .get_drag_context()
            .and_then(DragContext::as_node_drag)
            .expect("node drag");
        assert_eq!(
            nd.current_drop_target
                .into_option()
                .and_then(|t| t.node.into_crate_internal()),
            Some(NodeId::new(9))
        );
        assert_eq!(nd.drag_data.get_data("text/plain"), Some(&b"payload"[..]));

        // Clearing the target back to None must work too.
        m.update_drop_target(None);
        assert!(m
            .get_drag_context()
            .and_then(DragContext::as_node_drag)
            .unwrap()
            .current_drop_target
            .into_option()
            .is_none());

        // Auto-scroll only applies to text-selection drags: a no-op here.
        m.update_auto_scroll_direction(AutoScrollDirection::DownRight);
        assert!(m.is_node_drag_active());

        let ctx = m.end_drag().expect("the drag context is returned");
        assert_eq!(ctx.session_id, 1);
        assert!(!m.is_dragging());
        assert!(m.end_drag().is_none());
    }

    #[test]
    fn drop_target_and_auto_scroll_updates_without_a_drag_do_not_panic() {
        let mut m = GestureAndDragManager::new();
        m.update_drop_target(None);
        m.update_active_drag_positions(pos(f32::NAN, f32::INFINITY));
        m.update_auto_scroll_direction(AutoScrollDirection::UpLeft);
        m.cancel_drag();
        assert!(!m.is_dragging());
        assert!(m.get_drag_context_mut().is_none());
    }

    #[test]
    fn text_selection_context_accepts_the_auto_scroll_direction() {
        let mut m = GestureAndDragManager::new();
        m.active_drag = Some(DragContext::text_selection(
            DomId::ROOT_ID,
            NodeId::new(2),
            pos(0.0, 0.0),
            11,
        ));
        assert!(m.is_text_selection_dragging());
        assert!(!m.is_node_drag_active());
        m.update_auto_scroll_direction(AutoScrollDirection::DownRight);
        assert_eq!(
            m.get_drag_context()
                .and_then(DragContext::as_text_selection)
                .map(|t| t.auto_scroll_direction),
            Some(AutoScrollDirection::DownRight)
        );
        // update_drop_target must leave a text-selection drag untouched.
        m.update_drop_target(None);
        assert!(m.is_text_selection_dragging());

        m.cancel_drag();
        assert!(!m.is_dragging());
        assert!(!m.is_text_selection_dragging());
    }

    // ------------------------------------------------- window drag maths

    fn window_dragging_manager(initial: WindowPosition) -> GestureAndDragManager {
        let mut m = dragging_manager(pos(0.0, 0.0), pos(100.0, 0.0), 10);
        m.activate_window_drag(initial, None);
        assert!(m.is_window_dragging());
        m
    }

    #[test]
    fn window_drag_delta_needs_an_initialized_window_position() {
        let m = window_dragging_manager(WindowPosition::Uninitialized);
        assert!(m.get_window_drag_delta().is_none());
        assert!(m.get_window_position_from_drag().is_none());
    }

    #[test]
    fn window_drag_delta_is_measured_from_the_drag_start() {
        let mut m =
            window_dragging_manager(WindowPosition::Initialized(PhysicalPositionI32::new(10, 20)));
        m.update_active_drag_positions(pos(30.5, -20.9));
        // start_position is the drag's start (0,0) => delta truncates toward zero.
        assert_eq!(m.get_window_drag_delta(), Some((30, -20)));
        assert_eq!(
            m.get_window_position_from_drag(),
            Some(WindowPosition::Initialized(PhysicalPositionI32::new(40, 0)))
        );
    }

    #[test]
    fn window_drag_delta_saturates_the_float_to_int_cast() {
        let mut m =
            window_dragging_manager(WindowPosition::Initialized(PhysicalPositionI32::new(0, 0)));
        m.update_active_drag_positions(pos(f32::MAX, -f32::MAX));
        assert_eq!(
            m.get_window_drag_delta(),
            Some((i32::MAX, i32::MIN)),
            "float->int casts must saturate, not wrap or trap"
        );
        assert_eq!(
            m.get_window_position_from_drag(),
            Some(WindowPosition::Initialized(PhysicalPositionI32::new(
                i32::MAX,
                i32::MIN
            )))
        );
    }

    #[test]
    fn window_drag_delta_with_nan_position_is_zero_not_a_trap() {
        let mut m =
            window_dragging_manager(WindowPosition::Initialized(PhysicalPositionI32::new(3, 4)));
        m.update_active_drag_positions(pos(f32::NAN, f32::NAN));
        // `NaN as i32` is defined as 0 in Rust.
        assert_eq!(m.get_window_drag_delta(), Some((0, 0)));
        assert_eq!(
            m.get_window_position_from_drag(),
            Some(WindowPosition::Initialized(PhysicalPositionI32::new(3, 4)))
        );
    }

    #[test]
    fn window_position_from_drag_at_the_i32_extremes_does_not_overflow() {
        // i32::MAX window origin dragged fully negative: MAX + MIN == -1.
        let mut m = window_dragging_manager(WindowPosition::Initialized(
            PhysicalPositionI32::new(i32::MAX, i32::MAX),
        ));
        m.update_active_drag_positions(pos(-f32::MAX, -f32::MAX));
        assert_eq!(
            m.get_window_position_from_drag(),
            Some(WindowPosition::Initialized(PhysicalPositionI32::new(-1, -1)))
        );
    }

    // ------------------------------------------------- scrollbar drag maths

    fn scrollbar_manager(
        start_offset: f32,
        track: f32,
        content: f32,
        viewport: f32,
    ) -> GestureAndDragManager {
        let mut m = GestureAndDragManager::new();
        m.active_drag = Some(DragContext::scrollbar_thumb(
            DomId::ROOT_ID,
            NodeId::new(1),
            ScrollbarAxis::Vertical,
            pos(0.0, 0.0),
            start_offset,
            track,
            content,
            viewport,
            1,
        ));
        m
    }

    #[test]
    fn scrollbar_offset_scales_the_mouse_delta_and_clamps_to_the_range() {
        let mut m = scrollbar_manager(0.0, 100.0, 1000.0, 100.0);
        assert!(m.is_scrollbar_dragging());
        assert_eq!(m.get_scrollbar_scroll_offset(), Some(0.0));

        // thumb = 10px, scrollable track = 90px, scrollable range = 900px.
        m.update_active_drag_positions(pos(0.0, 45.0));
        let half = m.get_scrollbar_scroll_offset().expect("scrollbar drag");
        assert!((half - 450.0).abs() < 0.5, "expected ~450, got {half}");

        // Way past the end of the track: clamped to the scrollable range.
        m.update_active_drag_positions(pos(0.0, 1.0e9));
        assert_eq!(m.get_scrollbar_scroll_offset(), Some(900.0));

        // Dragged backwards past the start: clamped to 0.
        m.update_active_drag_positions(pos(0.0, -1.0e9));
        assert_eq!(m.get_scrollbar_scroll_offset(), Some(0.0));
    }

    #[test]
    fn scrollbar_offset_with_nothing_to_scroll_returns_the_start_offset() {
        // content <= viewport => scrollable range <= 0.
        let mut m = scrollbar_manager(42.0, 100.0, 50.0, 100.0);
        m.update_active_drag_positions(pos(0.0, 500.0));
        assert_eq!(m.get_scrollbar_scroll_offset(), Some(42.0));

        // A zero-length track cannot be scrolled either.
        let mut m = scrollbar_manager(7.0, 0.0, 1000.0, 100.0);
        m.update_active_drag_positions(pos(0.0, 500.0));
        assert_eq!(m.get_scrollbar_scroll_offset(), Some(7.0));
    }

    #[test]
    fn scrollbar_offset_with_a_nan_mouse_position_does_not_panic() {
        let mut m = scrollbar_manager(0.0, 100.0, 1000.0, 100.0);
        m.update_active_drag_positions(pos(f32::NAN, f32::NAN));
        let v = m.get_scrollbar_scroll_offset();
        assert!(
            v.is_some_and(f32::is_nan),
            "a NaN mouse position must propagate as NaN, not panic: {v:?}"
        );
    }

    // ------------------------------------------------- event ids

    #[cfg(feature = "std")]
    #[test]
    fn allocate_event_id_is_strictly_monotonic() {
        let a = allocate_event_id();
        let b = allocate_event_id();
        let c = allocate_event_id();
        assert!(a < b && b < c, "ids must increase: {a} {b} {c}");
    }

    #[cfg(not(feature = "std"))]
    #[test]
    fn allocate_event_id_is_zero_without_std() {
        assert_eq!(allocate_event_id(), 0);
    }

    #[cfg(feature = "std")]
    #[test]
    fn recorded_samples_get_distinct_event_ids() {
        let mut m = GestureAndDragManager::new();
        m.start_input_session(
            pos(0.0, 0.0),
            ts(0),
            0x01,
            WindowPosition::Uninitialized,
            pos(0.0, 0.0),
        );
        m.record_input_sample(pos(1.0, 0.0), ts(1), 0x01, pos(1.0, 0.0));
        let s = m.get_current_session().unwrap();
        assert_ne!(s.samples[0].event_id, s.samples[1].event_id);
        assert!(s.samples[0].event_id < s.samples[1].event_id);
    }
}
