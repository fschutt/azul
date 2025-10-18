//! Event and callback filtering module

#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
use alloc::{
    boxed::Box,
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    vec::Vec,
};

use azul_css::{
    props::{
        basic::{LayoutPoint, LayoutRect, LayoutSize},
        property::CssProperty,
    },
    AzString, LayoutDebugMessage,
};
use rust_fontconfig::FcFontCache;

use crate::{
    callbacks::Update,
    dom::{DomId, DomNodeId, On},
    geom::{LogicalPosition, LogicalRect},
    gl::OptionGlContextPtr,
    gpu::GpuEventChanges,
    hit_test::{FullHitTest, HitTestItem, ScrollPosition},
    id::NodeId,
    resources::{ImageCache, RendererResources},
    styled_dom::{ChangedCssProperty, NodeHierarchyItemId},
    task::Instant,
    window::RawWindowHandle,
    FastBTreeSet, FastHashMap,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Events {
    pub window_events: Vec<WindowEventFilter>,
    pub hover_events: Vec<HoverEventFilter>,
    pub focus_events: Vec<FocusEventFilter>,
    pub old_hit_node_ids: BTreeMap<DomId, BTreeMap<NodeId, HitTestItem>>,
    pub old_focus_node: Option<DomNodeId>,
    pub current_window_state_mouse_is_down: bool,
    pub previous_window_state_mouse_is_down: bool,
    pub event_was_mouse_down: bool,
    pub event_was_mouse_leave: bool,
    pub event_was_mouse_release: bool,
}

impl Events {
    pub fn is_empty(&self) -> bool {
        self.window_events.is_empty()
            && self.hover_events.is_empty()
            && self.focus_events.is_empty()
    }

    /// Checks whether the event was a resize event
    pub fn contains_resize_event(&self) -> bool {
        self.window_events.contains(&WindowEventFilter::Resized)
    }

    pub fn event_was_mouse_scroll(&self) -> bool {
        // TODO: also need to look at TouchStart / TouchDrag
        self.window_events.contains(&WindowEventFilter::Scroll)
    }

    pub fn needs_hit_test(&self) -> bool {
        !(self.hover_events.is_empty() && self.focus_events.is_empty())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodesToCheck {
    pub new_hit_node_ids: BTreeMap<DomId, BTreeMap<NodeId, HitTestItem>>,
    pub old_hit_node_ids: BTreeMap<DomId, BTreeMap<NodeId, HitTestItem>>,
    pub onmouseenter_nodes: BTreeMap<DomId, BTreeMap<NodeId, HitTestItem>>,
    pub onmouseleave_nodes: BTreeMap<DomId, BTreeMap<NodeId, HitTestItem>>,
    pub old_focus_node: Option<DomNodeId>,
    pub new_focus_node: Option<DomNodeId>,
    pub current_window_state_mouse_is_down: bool,
}

impl NodesToCheck {
    // Usually we need to perform a hit-test when the DOM is re-generated,
    // this function simulates that behaviour
    pub fn simulated_mouse_move(
        hit_test: &FullHitTest,
        old_focus_node: Option<DomNodeId>,
        mouse_down: bool,
    ) -> Self {
        let new_hit_node_ids = hit_test
            .hovered_nodes
            .iter()
            .map(|(k, v)| (k.clone(), v.regular_hit_test_nodes.clone()))
            .collect::<BTreeMap<_, _>>();

        Self {
            new_hit_node_ids: new_hit_node_ids.clone(),
            old_hit_node_ids: BTreeMap::new(),
            onmouseenter_nodes: new_hit_node_ids,
            onmouseleave_nodes: BTreeMap::new(),
            old_focus_node,
            new_focus_node: old_focus_node,
            current_window_state_mouse_is_down: mouse_down,
        }
    }

    /// Determine which nodes are even relevant for callbacks or restyling
    //
    // TODO: avoid iteration / allocation!
    pub fn new(hit_test: &FullHitTest, events: &Events) -> Self {
        // TODO: If the current mouse is down, but the event wasn't a click, that means it was a
        // drag

        // Figure out what the hovered NodeIds are
        let new_hit_node_ids = if events.event_was_mouse_leave {
            BTreeMap::new()
        } else {
            hit_test
                .hovered_nodes
                .iter()
                .map(|(k, v)| (k.clone(), v.regular_hit_test_nodes.clone()))
                .collect()
        };

        // Figure out what the current focused NodeId is
        let new_focus_node = if events.event_was_mouse_release {
            hit_test.focused_node.clone().map(|o| DomNodeId {
                dom: o.0,
                node: NodeHierarchyItemId::from_crate_internal(Some(o.1)),
            })
        } else {
            events.old_focus_node.clone()
        };

        // Collect all On::MouseEnter nodes (for both hover and focus events)
        let default_map = BTreeMap::new();
        let onmouseenter_nodes = new_hit_node_ids
            .iter()
            .filter_map(|(dom_id, nhnid)| {
                let old_hit_node_ids = events.old_hit_node_ids.get(dom_id).unwrap_or(&default_map);
                let new = nhnid
                    .iter()
                    .filter(|(current_node_id, _)| old_hit_node_ids.get(current_node_id).is_none())
                    .map(|(x, y)| (*x, y.clone()))
                    .collect::<BTreeMap<_, _>>();
                if new.is_empty() {
                    None
                } else {
                    Some((*dom_id, new))
                }
            })
            .collect::<BTreeMap<_, _>>();

        // Collect all On::MouseLeave nodes (for both hover and focus events)
        let onmouseleave_nodes = events
            .old_hit_node_ids
            .iter()
            .filter_map(|(dom_id, ohnid)| {
                let old = ohnid
                    .iter()
                    .filter(|(prev_node_id, _)| {
                        new_hit_node_ids
                            .get(dom_id)
                            .and_then(|d| d.get(prev_node_id))
                            .is_none()
                    })
                    .map(|(x, y)| (*x, y.clone()))
                    .collect::<BTreeMap<_, _>>();
                if old.is_empty() {
                    None
                } else {
                    Some((*dom_id, old))
                }
            })
            .collect::<BTreeMap<_, _>>();

        NodesToCheck {
            new_hit_node_ids,
            old_hit_node_ids: events.old_hit_node_ids.clone(),
            onmouseenter_nodes,
            onmouseleave_nodes,
            old_focus_node: events.old_focus_node.clone(),
            new_focus_node,
            current_window_state_mouse_is_down: events.current_window_state_mouse_is_down,
        }
    }

    pub fn empty(mouse_down: bool, old_focus_node: Option<DomNodeId>) -> Self {
        Self {
            new_hit_node_ids: BTreeMap::new(),
            old_hit_node_ids: BTreeMap::new(),
            onmouseenter_nodes: BTreeMap::new(),
            onmouseleave_nodes: BTreeMap::new(),
            old_focus_node,
            new_focus_node: old_focus_node,
            current_window_state_mouse_is_down: mouse_down,
        }
    }

    pub fn needs_hover_active_restyle(&self) -> bool {
        !(self.onmouseenter_nodes.is_empty() && self.onmouseleave_nodes.is_empty())
    }

    pub fn needs_focus_result(&self) -> bool {
        self.old_focus_node != self.new_focus_node
    }
}

pub type RestyleNodes = BTreeMap<NodeId, Vec<ChangedCssProperty>>;
pub type RelayoutNodes = BTreeMap<NodeId, Vec<ChangedCssProperty>>;
pub type RelayoutWords = BTreeMap<NodeId, AzString>;

#[derive(Debug, Clone, PartialEq)]
pub struct FocusChange {
    pub old: Option<DomNodeId>,
    pub new: Option<DomNodeId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CallbackToCall {
    pub node_id: NodeId,
    pub hit_test_item: Option<HitTestItem>,
    pub event_filter: EventFilter,
}

pub fn get_hover_events(input: &[WindowEventFilter]) -> Vec<HoverEventFilter> {
    input
        .iter()
        .filter_map(|window_event| window_event.to_hover_event_filter())
        .collect()
}

pub fn get_focus_events(input: &[HoverEventFilter]) -> Vec<FocusEventFilter> {
    input
        .iter()
        .filter_map(|hover_event| hover_event.to_focus_event_filter())
        .collect()
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ProcessEventResult {
    DoNothing = 0,
    ShouldReRenderCurrentWindow = 1,
    ShouldUpdateDisplayListCurrentWindow = 2,
    // GPU transforms changed: do another hit-test and recurse
    // until nothing has changed anymore
    UpdateHitTesterAndProcessAgain = 3,
    // Only refresh the display (in case of pure scroll or GPU-only events)
    ShouldRegenerateDomCurrentWindow = 4,
    ShouldRegenerateDomAllWindows = 5,
}

impl ProcessEventResult {
    pub fn order(&self) -> usize {
        use self::ProcessEventResult::*;
        match self {
            DoNothing => 0,
            ShouldReRenderCurrentWindow => 1,
            ShouldUpdateDisplayListCurrentWindow => 2,
            UpdateHitTesterAndProcessAgain => 3,
            ShouldRegenerateDomCurrentWindow => 4,
            ShouldRegenerateDomAllWindows => 5,
        }
    }
}

impl PartialOrd for ProcessEventResult {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.order().partial_cmp(&other.order())
    }
}

impl Ord for ProcessEventResult {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.order().cmp(&other.order())
    }
}

impl ProcessEventResult {
    pub fn max_self(self, other: Self) -> Self {
        self.max(other)
    }
}

// ============================================================================
// Phase 3.5: New Event System Types
// ============================================================================

/// Tracks the origin of an event for proper handling.
///
/// This allows the system to distinguish between user input, programmatic
/// changes, and synthetic events generated by UI components.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub enum EventSource {
    /// Direct user input (mouse, keyboard, touch, gamepad)
    User,
    /// API call (programmatic scroll, focus change, etc.)
    Programmatic,
    /// Generated from UI interaction (scrollbar drag, synthetic events)
    Synthetic,
    /// Generated from lifecycle hooks (mount, unmount, resize)
    Lifecycle,
}

/// Event propagation phase (similar to DOM Level 2 Events).
///
/// Events can be intercepted at different phases:
/// - **Capture**: Event travels from root down to target (rarely used)
/// - **Target**: Event is at the target element
/// - **Bubble**: Event travels from target back up to root (most common)
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub enum EventPhase {
    /// Event travels from root down to target
    Capture,
    /// Event is at the target element
    Target,
    /// Event bubbles from target back up to root
    Bubble,
}

impl Default for EventPhase {
    fn default() -> Self {
        EventPhase::Bubble
    }
}

/// Mouse button identifier for mouse events.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    Other(u8),
}

/// Scroll delta mode (how scroll deltas should be interpreted).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub enum ScrollDeltaMode {
    /// Delta is in pixels
    Pixel,
    /// Delta is in lines (e.g., 3 lines of text)
    Line,
    /// Delta is in pages
    Page,
}

/// Scroll direction for conditional event filtering.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Reason why a lifecycle event was triggered.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub enum LifecycleReason {
    /// First appearance in DOM
    InitialMount,
    /// Removed and re-added to DOM
    Remount,
    /// Layout bounds changed
    Resize,
    /// Props or state changed
    Update,
}

/// Keyboard modifier keys state.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Default)]
#[repr(C)]
pub struct KeyModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

impl KeyModifiers {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_shift(mut self) -> Self {
        self.shift = true;
        self
    }

    pub fn with_ctrl(mut self) -> Self {
        self.ctrl = true;
        self
    }

    pub fn with_alt(mut self) -> Self {
        self.alt = true;
        self
    }

    pub fn with_meta(mut self) -> Self {
        self.meta = true;
        self
    }

    pub fn is_empty(&self) -> bool {
        !self.shift && !self.ctrl && !self.alt && !self.meta
    }
}

/// Type-specific event data for mouse events.
#[derive(Debug, Clone, PartialEq)]
pub struct MouseEventData {
    /// Position of the mouse cursor
    pub position: LogicalPosition,
    /// Which button was pressed/released
    pub button: MouseButton,
    /// Bitmask of currently pressed buttons
    pub buttons: u8,
    /// Modifier keys state
    pub modifiers: KeyModifiers,
}

/// Type-specific event data for keyboard events.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyboardEventData {
    /// The virtual key code
    pub key_code: u32,
    /// The character produced (if any)
    pub char_code: Option<char>,
    /// Modifier keys state
    pub modifiers: KeyModifiers,
    /// Whether this is a repeat event
    pub repeat: bool,
}

/// Type-specific event data for scroll events.
#[derive(Debug, Clone, PartialEq)]
pub struct ScrollEventData {
    /// Scroll delta (dx, dy)
    pub delta: LogicalPosition,
    /// How the delta should be interpreted
    pub delta_mode: ScrollDeltaMode,
}

/// Type-specific event data for touch events.
#[derive(Debug, Clone, PartialEq)]
pub struct TouchEventData {
    /// Touch identifier
    pub id: u64,
    /// Touch position
    pub position: LogicalPosition,
    /// Touch force/pressure (0.0 - 1.0)
    pub force: f32,
}

/// Type-specific event data for clipboard events.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipboardEventData {
    /// The clipboard content (for paste events)
    pub content: Option<String>,
}

/// Type-specific event data for lifecycle events.
#[derive(Debug, Clone, PartialEq)]
pub struct LifecycleEventData {
    /// Why this lifecycle event was triggered
    pub reason: LifecycleReason,
    /// Previous layout bounds (for resize events)
    pub previous_bounds: Option<LogicalRect>,
    /// Current layout bounds
    pub current_bounds: LogicalRect,
}

/// Type-specific event data for window events.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowEventData {
    /// Window size (for resize events)
    pub size: Option<LogicalRect>,
    /// Window position (for move events)
    pub position: Option<LogicalPosition>,
}

/// Union of all possible event data types.
#[derive(Debug, Clone, PartialEq)]
pub enum EventData {
    /// Mouse event data
    Mouse(MouseEventData),
    /// Keyboard event data
    Keyboard(KeyboardEventData),
    /// Scroll event data
    Scroll(ScrollEventData),
    /// Touch event data
    Touch(TouchEventData),
    /// Clipboard event data
    Clipboard(ClipboardEventData),
    /// Lifecycle event data
    Lifecycle(LifecycleEventData),
    /// Window event data
    Window(WindowEventData),
    /// No additional data
    None,
}

/// High-level event type classification.
///
/// This enum categorizes all possible events that can occur in the UI.
/// It extends the existing event system with new event types for
/// lifecycle, clipboard, media, and form handling.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub enum EventType {
    // ========== Mouse Events ==========
    /// Mouse cursor is over the element
    MouseOver,
    /// Mouse cursor entered the element
    MouseEnter,
    /// Mouse cursor left the element
    MouseLeave,
    /// Mouse button pressed
    MouseDown,
    /// Mouse button released
    MouseUp,
    /// Mouse click (down + up on same element)
    Click,
    /// Mouse double-click
    DoubleClick,
    /// Right-click / context menu
    ContextMenu,

    // ========== Keyboard Events ==========
    /// Key pressed down
    KeyDown,
    /// Key released
    KeyUp,
    /// Character input (respects locale/keyboard layout)
    KeyPress,

    // ========== Focus Events ==========
    /// Element received focus
    Focus,
    /// Element lost focus
    Blur,
    /// Focus entered element or its children
    FocusIn,
    /// Focus left element and its children
    FocusOut,

    // ========== Input Events ==========
    /// Input value is being changed (fires on every keystroke)
    Input,
    /// Input value has changed (fires after editing complete)
    Change,
    /// Form submitted
    Submit,
    /// Form reset
    Reset,
    /// Form validation failed
    Invalid,

    // ========== Scroll Events ==========
    /// Element is being scrolled
    Scroll,
    /// Scroll started
    ScrollStart,
    /// Scroll ended
    ScrollEnd,

    // ========== Drag Events ==========
    /// Drag operation started
    DragStart,
    /// Element is being dragged
    Drag,
    /// Drag operation ended
    DragEnd,
    /// Dragged element entered drop target
    DragEnter,
    /// Dragged element is over drop target
    DragOver,
    /// Dragged element left drop target
    DragLeave,
    /// Element was dropped
    Drop,

    // ========== Touch Events ==========
    /// Touch started
    TouchStart,
    /// Touch moved
    TouchMove,
    /// Touch ended
    TouchEnd,
    /// Touch cancelled
    TouchCancel,

    // ========== Clipboard Events (NEW!) ==========
    /// Content copied to clipboard
    Copy,
    /// Content cut to clipboard
    Cut,
    /// Content pasted from clipboard
    Paste,

    // ========== Media Events (NEW!) ==========
    /// Media playback started
    Play,
    /// Media playback paused
    Pause,
    /// Media playback ended
    Ended,
    /// Media time updated
    TimeUpdate,
    /// Media volume changed
    VolumeChange,
    /// Media error occurred
    MediaError,

    // ========== Lifecycle Events (NEW!) ==========
    /// Component was mounted to the DOM
    Mount,
    /// Component will be unmounted from the DOM
    Unmount,
    /// Component was updated
    Update,
    /// Component layout bounds changed
    Resize,

    // ========== Window Events ==========
    /// Window resized
    WindowResize,
    /// Window moved
    WindowMove,
    /// Window close requested
    WindowClose,
    /// Window received focus
    WindowFocusIn,
    /// Window lost focus
    WindowFocusOut,
    /// System theme changed
    ThemeChange,

    // ========== File Events ==========
    /// File is being hovered
    FileHover,
    /// File was dropped
    FileDrop,
    /// File hover cancelled
    FileHoverCancel,
}

/// Unified event wrapper (similar to React's SyntheticEvent).
///
/// All events in the system are wrapped in this structure, providing
/// a consistent interface and enabling event propagation control.
#[derive(Debug, Clone, PartialEq)]
pub struct SyntheticEvent {
    /// The type of event
    pub event_type: EventType,

    /// Where the event came from
    pub source: EventSource,

    /// Current propagation phase
    pub phase: EventPhase,

    /// Target node that the event was dispatched to
    pub target: DomNodeId,

    /// Current node in the propagation path
    pub current_target: DomNodeId,

    /// Timestamp when event was created
    pub timestamp: Instant,

    /// Type-specific event data
    pub data: EventData,

    /// Whether propagation has been stopped
    pub stopped: bool,

    /// Whether immediate propagation has been stopped
    pub stopped_immediate: bool,

    /// Whether default action has been prevented
    pub prevented_default: bool,
}

impl SyntheticEvent {
    /// Create a new synthetic event.
    ///
    /// # Parameters
    /// - `timestamp`: Current time from `(system_callbacks.get_system_time_fn.cb)()`
    pub fn new(
        event_type: EventType,
        source: EventSource,
        target: DomNodeId,
        timestamp: Instant,
        data: EventData,
    ) -> Self {
        Self {
            event_type,
            source,
            phase: EventPhase::Target,
            target,
            current_target: target,
            timestamp,
            data,
            stopped: false,
            stopped_immediate: false,
            prevented_default: false,
        }
    }

    /// Stop event propagation after the current phase completes.
    ///
    /// This prevents the event from reaching handlers in subsequent phases
    /// (e.g., stopping during capture prevents bubble phase).
    pub fn stop_propagation(&mut self) {
        self.stopped = true;
    }

    /// Stop event propagation immediately.
    ///
    /// This prevents any further handlers from being called, even on the
    /// current target element.
    pub fn stop_immediate_propagation(&mut self) {
        self.stopped_immediate = true;
        self.stopped = true;
    }

    /// Prevent the default action associated with this event.
    ///
    /// For example, prevents form submission on Enter key, or prevents
    /// text selection on drag.
    pub fn prevent_default(&mut self) {
        self.prevented_default = true;
    }

    /// Check if propagation was stopped.
    pub fn is_propagation_stopped(&self) -> bool {
        self.stopped
    }

    /// Check if immediate propagation was stopped.
    pub fn is_immediate_propagation_stopped(&self) -> bool {
        self.stopped_immediate
    }

    /// Check if default action was prevented.
    pub fn is_default_prevented(&self) -> bool {
        self.prevented_default
    }
}

// ============================================================================
// Phase 3.5: Event Filter System (moved from dom.rs)
// ============================================================================

/// Event filter that only fires when an element is hovered over.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub enum HoverEventFilter {
    MouseOver,
    MouseDown,
    LeftMouseDown,
    RightMouseDown,
    MiddleMouseDown,
    MouseUp,
    LeftMouseUp,
    RightMouseUp,
    MiddleMouseUp,
    MouseEnter,
    MouseLeave,
    Scroll,
    ScrollStart,
    ScrollEnd,
    TextInput,
    VirtualKeyDown,
    VirtualKeyUp,
    HoveredFile,
    DroppedFile,
    HoveredFileCancelled,
    TouchStart,
    TouchMove,
    TouchEnd,
    TouchCancel,
}

impl HoverEventFilter {
    pub fn to_focus_event_filter(&self) -> Option<FocusEventFilter> {
        match self {
            HoverEventFilter::MouseOver => Some(FocusEventFilter::MouseOver),
            HoverEventFilter::MouseDown => Some(FocusEventFilter::MouseDown),
            HoverEventFilter::LeftMouseDown => Some(FocusEventFilter::LeftMouseDown),
            HoverEventFilter::RightMouseDown => Some(FocusEventFilter::RightMouseDown),
            HoverEventFilter::MiddleMouseDown => Some(FocusEventFilter::MiddleMouseDown),
            HoverEventFilter::MouseUp => Some(FocusEventFilter::MouseUp),
            HoverEventFilter::LeftMouseUp => Some(FocusEventFilter::LeftMouseUp),
            HoverEventFilter::RightMouseUp => Some(FocusEventFilter::RightMouseUp),
            HoverEventFilter::MiddleMouseUp => Some(FocusEventFilter::MiddleMouseUp),
            HoverEventFilter::MouseEnter => Some(FocusEventFilter::MouseEnter),
            HoverEventFilter::MouseLeave => Some(FocusEventFilter::MouseLeave),
            HoverEventFilter::Scroll => Some(FocusEventFilter::Scroll),
            HoverEventFilter::ScrollStart => Some(FocusEventFilter::ScrollStart),
            HoverEventFilter::ScrollEnd => Some(FocusEventFilter::ScrollEnd),
            HoverEventFilter::TextInput => Some(FocusEventFilter::TextInput),
            HoverEventFilter::VirtualKeyDown => Some(FocusEventFilter::VirtualKeyDown),
            HoverEventFilter::VirtualKeyUp => Some(FocusEventFilter::VirtualKeyDown),
            HoverEventFilter::HoveredFile => None,
            HoverEventFilter::DroppedFile => None,
            HoverEventFilter::HoveredFileCancelled => None,
            HoverEventFilter::TouchStart => None,
            HoverEventFilter::TouchMove => None,
            HoverEventFilter::TouchEnd => None,
            HoverEventFilter::TouchCancel => None,
        }
    }
}

/// Event filter similar to `HoverEventFilter` that only fires when the element is focused.
///
/// **Important**: In order for this to fire, the item must have a `tabindex` attribute
/// (to indicate that the item is focus-able).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub enum FocusEventFilter {
    MouseOver,
    MouseDown,
    LeftMouseDown,
    RightMouseDown,
    MiddleMouseDown,
    MouseUp,
    LeftMouseUp,
    RightMouseUp,
    MiddleMouseUp,
    MouseEnter,
    MouseLeave,
    Scroll,
    ScrollStart,
    ScrollEnd,
    TextInput,
    VirtualKeyDown,
    VirtualKeyUp,
    FocusReceived,
    FocusLost,
}

/// Event filter that fires when any action fires on the entire window
/// (regardless of whether any element is hovered or focused over).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub enum WindowEventFilter {
    MouseOver,
    MouseDown,
    LeftMouseDown,
    RightMouseDown,
    MiddleMouseDown,
    MouseUp,
    LeftMouseUp,
    RightMouseUp,
    MiddleMouseUp,
    MouseEnter,
    MouseLeave,
    Scroll,
    ScrollStart,
    ScrollEnd,
    TextInput,
    VirtualKeyDown,
    VirtualKeyUp,
    HoveredFile,
    DroppedFile,
    HoveredFileCancelled,
    Resized,
    Moved,
    TouchStart,
    TouchMove,
    TouchEnd,
    TouchCancel,
    FocusReceived,
    FocusLost,
    CloseRequested,
    ThemeChanged,
    WindowFocusReceived,
    WindowFocusLost,
}

impl WindowEventFilter {
    pub fn to_hover_event_filter(&self) -> Option<HoverEventFilter> {
        match self {
            WindowEventFilter::MouseOver => Some(HoverEventFilter::MouseOver),
            WindowEventFilter::MouseDown => Some(HoverEventFilter::MouseDown),
            WindowEventFilter::LeftMouseDown => Some(HoverEventFilter::LeftMouseDown),
            WindowEventFilter::RightMouseDown => Some(HoverEventFilter::RightMouseDown),
            WindowEventFilter::MiddleMouseDown => Some(HoverEventFilter::MiddleMouseDown),
            WindowEventFilter::MouseUp => Some(HoverEventFilter::MouseUp),
            WindowEventFilter::LeftMouseUp => Some(HoverEventFilter::LeftMouseUp),
            WindowEventFilter::RightMouseUp => Some(HoverEventFilter::RightMouseUp),
            WindowEventFilter::MiddleMouseUp => Some(HoverEventFilter::MiddleMouseUp),
            WindowEventFilter::Scroll => Some(HoverEventFilter::Scroll),
            WindowEventFilter::ScrollStart => Some(HoverEventFilter::ScrollStart),
            WindowEventFilter::ScrollEnd => Some(HoverEventFilter::ScrollEnd),
            WindowEventFilter::TextInput => Some(HoverEventFilter::TextInput),
            WindowEventFilter::VirtualKeyDown => Some(HoverEventFilter::VirtualKeyDown),
            WindowEventFilter::VirtualKeyUp => Some(HoverEventFilter::VirtualKeyDown),
            WindowEventFilter::HoveredFile => Some(HoverEventFilter::HoveredFile),
            WindowEventFilter::DroppedFile => Some(HoverEventFilter::DroppedFile),
            WindowEventFilter::HoveredFileCancelled => Some(HoverEventFilter::HoveredFileCancelled),
            // MouseEnter and MouseLeave on the **window** - does not mean a mouseenter
            // and a mouseleave on the hovered element
            WindowEventFilter::MouseEnter => None,
            WindowEventFilter::MouseLeave => None,
            WindowEventFilter::Resized => None,
            WindowEventFilter::Moved => None,
            WindowEventFilter::TouchStart => Some(HoverEventFilter::TouchStart),
            WindowEventFilter::TouchMove => Some(HoverEventFilter::TouchMove),
            WindowEventFilter::TouchEnd => Some(HoverEventFilter::TouchEnd),
            WindowEventFilter::TouchCancel => Some(HoverEventFilter::TouchCancel),
            WindowEventFilter::FocusReceived => None,
            WindowEventFilter::FocusLost => None,
            WindowEventFilter::CloseRequested => None,
            WindowEventFilter::ThemeChanged => None,
            WindowEventFilter::WindowFocusReceived => None, // specific to window!
            WindowEventFilter::WindowFocusLost => None,     // specific to window!
        }
    }
}

/// The inverse of an `onclick` event filter, fires when an item is *not* hovered / focused.
/// This is useful for cleanly implementing things like popover dialogs or dropdown boxes that
/// want to close when the user clicks any where *but* the item itself.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C, u8)]
pub enum NotEventFilter {
    Hover(HoverEventFilter),
    Focus(FocusEventFilter),
}

impl NotEventFilter {
    pub fn as_event_filter(&self) -> EventFilter {
        match self {
            NotEventFilter::Hover(e) => EventFilter::Hover(*e),
            NotEventFilter::Focus(e) => EventFilter::Focus(*e),
        }
    }
}

/// Defines events related to the lifecycle of a DOM node itself.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum ComponentEventFilter {
    /// Fired after the component is first mounted into the DOM.
    AfterMount,
    /// Fired just before the component is removed from the DOM.
    BeforeUnmount,
    /// Fired when the node's layout rectangle has been resized.
    NodeResized,
    /// Fired to trigger the default action for an accessibility component.
    DefaultAction,
    /// Fired when the component becomes selected.
    Selected,
}

/// Defines application-level events not tied to a specific window or node.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum ApplicationEventFilter {
    /// Fired when a new hardware device is connected.
    DeviceConnected,
    /// Fired when a hardware device is disconnected.
    DeviceDisconnected,
    // ... TODO: more events
}

/// Sets the target for what events can reach the callbacks specifically.
///
/// This determines the condition under which an event is fired, such as whether
/// the node is hovered, focused, or if the event is window-global.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C, u8)]
pub enum EventFilter {
    /// Calls the attached callback when the mouse is actively over the
    /// given element.
    Hover(HoverEventFilter),
    /// Inverse of `Hover` - calls the attached callback if the mouse is **not**
    /// over the given element. This is particularly useful for popover menus
    /// where you want to close the menu when the user clicks anywhere else but
    /// the menu itself.
    Not(NotEventFilter),
    /// Calls the attached callback when the element is currently focused.
    Focus(FocusEventFilter),
    /// Calls the callback when anything related to the window is happening.
    /// The "hit item" will be the root item of the DOM.
    /// For example, this can be useful for tracking the mouse position
    /// (in relation to the window). In difference to `Desktop`, this only
    /// fires when the window is focused.
    ///
    /// This can also be good for capturing controller input, touch input
    /// (i.e. global gestures that aren't attached to any component, but rather
    /// the "window" itself).
    Window(WindowEventFilter),
    /// API stub: Something happened with the node itself (node resized, created or removed).
    Component(ComponentEventFilter),
    /// Something happened with the application (started, shutdown, device plugged in).
    Application(ApplicationEventFilter),
}

impl EventFilter {
    pub const fn is_focus_callback(&self) -> bool {
        match self {
            EventFilter::Focus(_) => true,
            _ => false,
        }
    }
    pub const fn is_window_callback(&self) -> bool {
        match self {
            EventFilter::Window(_) => true,
            _ => false,
        }
    }
}

/// Creates a function inside an impl <enum type> block that returns a single
/// variant if the enum is that variant.
///
/// ```rust,no_run,ignore
/// # use azul_core::events::get_single_enum_type;
/// enum A {
///     Abc(AbcType),
/// }
///
/// struct AbcType {}
///
/// impl A {
///     // fn as_abc_type(&self) -> Option<AbcType>
///     get_single_enum_type!(as_abc_type, A::Abc(AbcType));
/// }
/// ```
macro_rules! get_single_enum_type {
    ($fn_name:ident, $enum_name:ident:: $variant:ident($return_type:ty)) => {
        pub fn $fn_name(&self) -> Option<$return_type> {
            use self::$enum_name::*;
            match self {
                $variant(e) => Some(*e),
                _ => None,
            }
        }
    };
}

impl EventFilter {
    get_single_enum_type!(as_hover_event_filter, EventFilter::Hover(HoverEventFilter));
    get_single_enum_type!(as_focus_event_filter, EventFilter::Focus(FocusEventFilter));
    get_single_enum_type!(as_not_event_filter, EventFilter::Not(NotEventFilter));
    get_single_enum_type!(
        as_window_event_filter,
        EventFilter::Window(WindowEventFilter)
    );
}

/// Convert from `On` enum to `EventFilter`.
///
/// This determines which specific filter variant is used based on the event type.
/// For example, `On::TextInput` becomes a Focus event filter, while `On::VirtualKeyDown`
/// becomes a Window event filter (since it's global to the window).
impl From<On> for EventFilter {
    fn from(input: On) -> EventFilter {
        use crate::dom::On::*;
        match input {
            MouseOver => EventFilter::Hover(HoverEventFilter::MouseOver),
            MouseDown => EventFilter::Hover(HoverEventFilter::MouseDown),
            LeftMouseDown => EventFilter::Hover(HoverEventFilter::LeftMouseDown),
            MiddleMouseDown => EventFilter::Hover(HoverEventFilter::MiddleMouseDown),
            RightMouseDown => EventFilter::Hover(HoverEventFilter::RightMouseDown),
            MouseUp => EventFilter::Hover(HoverEventFilter::MouseUp),
            LeftMouseUp => EventFilter::Hover(HoverEventFilter::LeftMouseUp),
            MiddleMouseUp => EventFilter::Hover(HoverEventFilter::MiddleMouseUp),
            RightMouseUp => EventFilter::Hover(HoverEventFilter::RightMouseUp),

            MouseEnter => EventFilter::Hover(HoverEventFilter::MouseEnter),
            MouseLeave => EventFilter::Hover(HoverEventFilter::MouseLeave),
            Scroll => EventFilter::Hover(HoverEventFilter::Scroll),
            TextInput => EventFilter::Focus(FocusEventFilter::TextInput), // focus!
            VirtualKeyDown => EventFilter::Window(WindowEventFilter::VirtualKeyDown), // window!
            VirtualKeyUp => EventFilter::Window(WindowEventFilter::VirtualKeyUp), // window!
            HoveredFile => EventFilter::Hover(HoverEventFilter::HoveredFile),
            DroppedFile => EventFilter::Hover(HoverEventFilter::DroppedFile),
            HoveredFileCancelled => EventFilter::Hover(HoverEventFilter::HoveredFileCancelled),
            FocusReceived => EventFilter::Focus(FocusEventFilter::FocusReceived), // focus!
            FocusLost => EventFilter::Focus(FocusEventFilter::FocusLost),         // focus!
        }
    }
}
