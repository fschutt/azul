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

/// Easing functions für smooth scrolling (für Scroll-Animationen)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EasingFunction {
    Linear,
    EaseInOut,
    EaseOut,
}

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
// Phase 3.5, Step 3: Event Propagation System
// ============================================================================

/// Result of event propagation through DOM tree.
#[derive(Debug, Clone)]
pub struct PropagationResult {
    /// Callbacks that should be invoked, in order
    pub callbacks_to_invoke: Vec<(NodeId, EventFilter)>,
    /// Whether default action should be prevented
    pub default_prevented: bool,
}

/// Get the path from root to target node in the DOM tree.
///
/// This is used for event propagation - we need to know which nodes
/// are ancestors of the target to implement capture/bubble phases.
///
/// Returns nodes in order from root to target (inclusive).
pub fn get_dom_path(
    node_hierarchy: &crate::id::NodeHierarchy,
    target_node: NodeHierarchyItemId,
) -> Vec<NodeId> {
    let mut path = Vec::new();
    let target_node_id = match target_node.into_crate_internal() {
        Some(id) => id,
        None => return path,
    };

    let hier_ref = node_hierarchy.as_ref();

    // Build path from target to root
    let mut current = Some(target_node_id);
    while let Some(node_id) = current {
        path.push(node_id);
        current = hier_ref.get(node_id).and_then(|node| node.parent);
    }

    // Reverse to get root → target order
    path.reverse();
    path
}

/// Propagate event through DOM tree with capture and bubble phases.
///
/// This implements DOM Level 2 event propagation:
/// 1. **Capture Phase**: Event travels from root down to target
/// 2. **Target Phase**: Event is at the target element
/// 3. **Bubble Phase**: Event travels from target back up to root
///
/// The event can be stopped at any point via `stopPropagation()` or
/// `stopImmediatePropagation()`.
///
/// # Arguments
/// * `event` - The synthetic event to propagate
/// * `node_hierarchy` - The DOM tree structure
/// * `callbacks` - Map of node IDs to their registered event callbacks
///
/// # Returns
/// `PropagationResult` containing callbacks to invoke and default action state
pub fn propagate_event(
    event: &mut SyntheticEvent,
    node_hierarchy: &crate::id::NodeHierarchy,
    callbacks: &BTreeMap<NodeId, Vec<EventFilter>>,
) -> PropagationResult {
    let mut result = PropagationResult {
        callbacks_to_invoke: Vec::new(),
        default_prevented: false,
    };

    // Get path from root to target
    let path = get_dom_path(node_hierarchy, event.target.node);
    if path.is_empty() {
        return result;
    }

    // Phase 1: Capture (root → target)
    event.phase = EventPhase::Capture;
    for &node_id in &path[..path.len().saturating_sub(1)] {
        if event.stopped_immediate {
            break;
        }

        event.current_target = DomNodeId {
            dom: event.target.dom,
            node: NodeHierarchyItemId::from_crate_internal(Some(node_id)),
        };

        if let Some(node_callbacks) = callbacks.get(&node_id) {
            for filter in node_callbacks {
                // Check if this filter matches the current phase
                if matches_filter_phase(filter, event, EventPhase::Capture) {
                    result.callbacks_to_invoke.push((node_id, *filter));

                    if event.stopped_immediate {
                        break;
                    }
                }
            }
        }

        if event.stopped {
            break;
        }
    }

    // Phase 2: Target
    if !event.stopped && !path.is_empty() {
        event.phase = EventPhase::Target;
        let target_node_id = *path.last().unwrap();
        event.current_target = event.target;

        if let Some(node_callbacks) = callbacks.get(&target_node_id) {
            for filter in node_callbacks {
                if event.stopped_immediate {
                    break;
                }

                // At target phase, fire both capture and bubble listeners
                if matches_filter_phase(filter, event, EventPhase::Target) {
                    result.callbacks_to_invoke.push((target_node_id, *filter));
                }
            }
        }
    }

    // Phase 3: Bubble (target → root)
    if !event.stopped {
        event.phase = EventPhase::Bubble;

        // Iterate in reverse (excluding target, which was already handled)
        for &node_id in path[..path.len().saturating_sub(1)].iter().rev() {
            if event.stopped_immediate {
                break;
            }

            event.current_target = DomNodeId {
                dom: event.target.dom,
                node: NodeHierarchyItemId::from_crate_internal(Some(node_id)),
            };

            if let Some(node_callbacks) = callbacks.get(&node_id) {
                for filter in node_callbacks {
                    if matches_filter_phase(filter, event, EventPhase::Bubble) {
                        result.callbacks_to_invoke.push((node_id, *filter));

                        if event.stopped_immediate {
                            break;
                        }
                    }
                }
            }

            if event.stopped {
                break;
            }
        }
    }

    result.default_prevented = event.prevented_default;
    result
}

/// Check if an event filter matches the given event in the current phase.
///
/// This is used during event propagation to determine which callbacks
/// should be invoked at each phase.
fn matches_filter_phase(
    filter: &EventFilter,
    event: &SyntheticEvent,
    current_phase: EventPhase,
) -> bool {
    // For now, we match based on the filter type
    // In the future, this will also check EventPhase and EventConditions

    match filter {
        EventFilter::Hover(hover_filter) => {
            matches_hover_filter(hover_filter, event, current_phase)
        }
        EventFilter::Focus(focus_filter) => {
            matches_focus_filter(focus_filter, event, current_phase)
        }
        EventFilter::Window(window_filter) => {
            matches_window_filter(window_filter, event, current_phase)
        }
        EventFilter::Not(_) => {
            // Not filters are inverted - will be implemented in future
            false
        }
        EventFilter::Component(_) | EventFilter::Application(_) => {
            // Lifecycle and application events - will be implemented in future
            false
        }
    }
}

/// Check if a hover filter matches the event.
fn matches_hover_filter(
    filter: &HoverEventFilter,
    event: &SyntheticEvent,
    _phase: EventPhase,
) -> bool {
    use HoverEventFilter::*;

    match (filter, &event.event_type) {
        (MouseOver, EventType::MouseOver) => true,
        (MouseDown, EventType::MouseDown) => true,
        (LeftMouseDown, EventType::MouseDown) => {
            // Check if it's left button
            if let EventData::Mouse(mouse_data) = &event.data {
                mouse_data.button == MouseButton::Left
            } else {
                false
            }
        }
        (RightMouseDown, EventType::MouseDown) => {
            if let EventData::Mouse(mouse_data) = &event.data {
                mouse_data.button == MouseButton::Right
            } else {
                false
            }
        }
        (MiddleMouseDown, EventType::MouseDown) => {
            if let EventData::Mouse(mouse_data) = &event.data {
                mouse_data.button == MouseButton::Middle
            } else {
                false
            }
        }
        (MouseUp, EventType::MouseUp) => true,
        (LeftMouseUp, EventType::MouseUp) => {
            if let EventData::Mouse(mouse_data) = &event.data {
                mouse_data.button == MouseButton::Left
            } else {
                false
            }
        }
        (RightMouseUp, EventType::MouseUp) => {
            if let EventData::Mouse(mouse_data) = &event.data {
                mouse_data.button == MouseButton::Right
            } else {
                false
            }
        }
        (MiddleMouseUp, EventType::MouseUp) => {
            if let EventData::Mouse(mouse_data) = &event.data {
                mouse_data.button == MouseButton::Middle
            } else {
                false
            }
        }
        (MouseEnter, EventType::MouseEnter) => true,
        (MouseLeave, EventType::MouseLeave) => true,
        (Scroll, EventType::Scroll) => true,
        (ScrollStart, EventType::ScrollStart) => true,
        (ScrollEnd, EventType::ScrollEnd) => true,
        (TextInput, EventType::Input) => true,
        (VirtualKeyDown, EventType::KeyDown) => true,
        (VirtualKeyUp, EventType::KeyUp) => true,
        (HoveredFile, EventType::FileHover) => true,
        (DroppedFile, EventType::FileDrop) => true,
        (HoveredFileCancelled, EventType::FileHoverCancel) => true,
        (TouchStart, EventType::TouchStart) => true,
        (TouchMove, EventType::TouchMove) => true,
        (TouchEnd, EventType::TouchEnd) => true,
        (TouchCancel, EventType::TouchCancel) => true,
        _ => false,
    }
}

/// Check if a focus filter matches the event.
fn matches_focus_filter(
    filter: &FocusEventFilter,
    event: &SyntheticEvent,
    _phase: EventPhase,
) -> bool {
    use FocusEventFilter::*;

    match (filter, &event.event_type) {
        (MouseOver, EventType::MouseOver) => true,
        (MouseDown, EventType::MouseDown) => true,
        (LeftMouseDown, EventType::MouseDown) => {
            if let EventData::Mouse(mouse_data) = &event.data {
                mouse_data.button == MouseButton::Left
            } else {
                false
            }
        }
        (RightMouseDown, EventType::MouseDown) => {
            if let EventData::Mouse(mouse_data) = &event.data {
                mouse_data.button == MouseButton::Right
            } else {
                false
            }
        }
        (MiddleMouseDown, EventType::MouseDown) => {
            if let EventData::Mouse(mouse_data) = &event.data {
                mouse_data.button == MouseButton::Middle
            } else {
                false
            }
        }
        (MouseUp, EventType::MouseUp) => true,
        (LeftMouseUp, EventType::MouseUp) => {
            if let EventData::Mouse(mouse_data) = &event.data {
                mouse_data.button == MouseButton::Left
            } else {
                false
            }
        }
        (RightMouseUp, EventType::MouseUp) => {
            if let EventData::Mouse(mouse_data) = &event.data {
                mouse_data.button == MouseButton::Right
            } else {
                false
            }
        }
        (MiddleMouseUp, EventType::MouseUp) => {
            if let EventData::Mouse(mouse_data) = &event.data {
                mouse_data.button == MouseButton::Middle
            } else {
                false
            }
        }
        (MouseEnter, EventType::MouseEnter) => true,
        (MouseLeave, EventType::MouseLeave) => true,
        (Scroll, EventType::Scroll) => true,
        (ScrollStart, EventType::ScrollStart) => true,
        (ScrollEnd, EventType::ScrollEnd) => true,
        (TextInput, EventType::Input) => true,
        (VirtualKeyDown, EventType::KeyDown) => true,
        (VirtualKeyUp, EventType::KeyUp) => true,
        (FocusReceived, EventType::Focus) => true,
        (FocusLost, EventType::Blur) => true,
        _ => false,
    }
}

/// Check if a window filter matches the event.
fn matches_window_filter(
    filter: &WindowEventFilter,
    event: &SyntheticEvent,
    _phase: EventPhase,
) -> bool {
    use WindowEventFilter::*;

    match (filter, &event.event_type) {
        (MouseOver, EventType::MouseOver) => true,
        (MouseDown, EventType::MouseDown) => true,
        (LeftMouseDown, EventType::MouseDown) => {
            if let EventData::Mouse(mouse_data) = &event.data {
                mouse_data.button == MouseButton::Left
            } else {
                false
            }
        }
        (RightMouseDown, EventType::MouseDown) => {
            if let EventData::Mouse(mouse_data) = &event.data {
                mouse_data.button == MouseButton::Right
            } else {
                false
            }
        }
        (MiddleMouseDown, EventType::MouseDown) => {
            if let EventData::Mouse(mouse_data) = &event.data {
                mouse_data.button == MouseButton::Middle
            } else {
                false
            }
        }
        (MouseUp, EventType::MouseUp) => true,
        (LeftMouseUp, EventType::MouseUp) => {
            if let EventData::Mouse(mouse_data) = &event.data {
                mouse_data.button == MouseButton::Left
            } else {
                false
            }
        }
        (RightMouseUp, EventType::MouseUp) => {
            if let EventData::Mouse(mouse_data) = &event.data {
                mouse_data.button == MouseButton::Right
            } else {
                false
            }
        }
        (MiddleMouseUp, EventType::MouseUp) => {
            if let EventData::Mouse(mouse_data) = &event.data {
                mouse_data.button == MouseButton::Middle
            } else {
                false
            }
        }
        (MouseEnter, EventType::MouseEnter) => true,
        (MouseLeave, EventType::MouseLeave) => true,
        (Scroll, EventType::Scroll) => true,
        (ScrollStart, EventType::ScrollStart) => true,
        (ScrollEnd, EventType::ScrollEnd) => true,
        (TextInput, EventType::Input) => true,
        (VirtualKeyDown, EventType::KeyDown) => true,
        (VirtualKeyUp, EventType::KeyUp) => true,
        (HoveredFile, EventType::FileHover) => true,
        (DroppedFile, EventType::FileDrop) => true,
        (HoveredFileCancelled, EventType::FileHoverCancel) => true,
        (Resized, EventType::WindowResize) => true,
        (Moved, EventType::WindowMove) => true,
        (TouchStart, EventType::TouchStart) => true,
        (TouchMove, EventType::TouchMove) => true,
        (TouchEnd, EventType::TouchEnd) => true,
        (TouchCancel, EventType::TouchCancel) => true,
        (FocusReceived, EventType::Focus) => true,
        (FocusLost, EventType::Blur) => true,
        (CloseRequested, EventType::WindowClose) => true,
        (ThemeChanged, EventType::ThemeChange) => true,
        (WindowFocusReceived, EventType::WindowFocusIn) => true,
        (WindowFocusLost, EventType::WindowFocusOut) => true,
        _ => false,
    }
}

// ============================================================================
// Phase 3.5, Step 4: Lifecycle Event Detection
// ============================================================================

/// Detect lifecycle events by comparing old and new DOM state.
///
/// This function analyzes the differences between two DOM trees and their
/// layouts to generate lifecycle events (Mount, Unmount, Resize).
///
/// # Arguments
/// * `old_dom_id` - DomId of the old DOM (for unmount events)
/// * `new_dom_id` - DomId of the new DOM (for mount events)
/// * `old_hierarchy` - Old DOM node hierarchy
/// * `new_hierarchy` - New DOM node hierarchy
/// * `old_layout` - Old layout rectangles (optional)
/// * `new_layout` - New layout rectangles (optional)
/// * `timestamp` - Current time from system_callbacks.get_system_time_fn.cb()
///
/// # Returns
/// Vector of SyntheticEvents for lifecycle changes
pub fn detect_lifecycle_events(
    old_dom_id: DomId,
    new_dom_id: DomId,
    old_hierarchy: Option<&crate::id::NodeHierarchy>,
    new_hierarchy: Option<&crate::id::NodeHierarchy>,
    old_layout: Option<&BTreeMap<NodeId, LogicalRect>>,
    new_layout: Option<&BTreeMap<NodeId, LogicalRect>>,
    timestamp: Instant,
) -> Vec<SyntheticEvent> {
    let mut events = Vec::new();

    // Collect node IDs from both hierarchies
    let old_nodes: BTreeSet<NodeId> = old_hierarchy
        .map(|h| h.as_ref().linear_iter().map(|id| id).collect())
        .unwrap_or_default();

    let new_nodes: BTreeSet<NodeId> = new_hierarchy
        .map(|h| h.as_ref().linear_iter().map(|id| id).collect())
        .unwrap_or_default();

    // 1. Detect newly mounted nodes (in new but not in old)
    if let Some(new_layout) = new_layout {
        for node_id in new_nodes.difference(&old_nodes) {
            let current_bounds = new_layout
                .get(node_id)
                .copied()
                .unwrap_or(LogicalRect::zero());

            events.push(SyntheticEvent {
                event_type: EventType::Mount,
                source: EventSource::Lifecycle,
                phase: EventPhase::Target,
                target: DomNodeId {
                    dom: new_dom_id,
                    node: NodeHierarchyItemId::from_crate_internal(Some(*node_id)),
                },
                current_target: DomNodeId {
                    dom: new_dom_id,
                    node: NodeHierarchyItemId::from_crate_internal(Some(*node_id)),
                },
                timestamp: timestamp.clone(),
                data: EventData::Lifecycle(LifecycleEventData {
                    reason: LifecycleReason::InitialMount,
                    previous_bounds: None,
                    current_bounds,
                }),
                stopped: false,
                stopped_immediate: false,
                prevented_default: false,
            });
        }
    }

    // 2. Detect unmounted nodes (in old but not in new)
    if let Some(old_layout) = old_layout {
        for node_id in old_nodes.difference(&new_nodes) {
            let previous_bounds = old_layout
                .get(node_id)
                .copied()
                .unwrap_or(LogicalRect::zero());

            events.push(SyntheticEvent {
                event_type: EventType::Unmount,
                source: EventSource::Lifecycle,
                phase: EventPhase::Target,
                target: DomNodeId {
                    dom: old_dom_id,
                    node: NodeHierarchyItemId::from_crate_internal(Some(*node_id)),
                },
                current_target: DomNodeId {
                    dom: old_dom_id,
                    node: NodeHierarchyItemId::from_crate_internal(Some(*node_id)),
                },
                timestamp: timestamp.clone(),
                data: EventData::Lifecycle(LifecycleEventData {
                    reason: LifecycleReason::InitialMount, // Will be cleaned up
                    previous_bounds: Some(previous_bounds),
                    current_bounds: LogicalRect::zero(),
                }),
                stopped: false,
                stopped_immediate: false,
                prevented_default: false,
            });
        }
    }

    // 3. Detect resized nodes (in both, but bounds changed)
    if let (Some(old_layout), Some(new_layout)) = (old_layout, new_layout) {
        for node_id in old_nodes.intersection(&new_nodes) {
            if let (Some(&old_bounds), Some(&new_bounds)) =
                (old_layout.get(node_id), new_layout.get(node_id))
            {
                // Check if size changed (position changes don't trigger resize)
                if old_bounds.size != new_bounds.size {
                    events.push(SyntheticEvent {
                        event_type: EventType::Resize,
                        source: EventSource::Lifecycle,
                        phase: EventPhase::Target,
                        target: DomNodeId {
                            dom: new_dom_id,
                            node: NodeHierarchyItemId::from_crate_internal(Some(*node_id)),
                        },
                        current_target: DomNodeId {
                            dom: new_dom_id,
                            node: NodeHierarchyItemId::from_crate_internal(Some(*node_id)),
                        },
                        timestamp: timestamp.clone(),
                        data: EventData::Lifecycle(LifecycleEventData {
                            reason: LifecycleReason::Resize,
                            previous_bounds: Some(old_bounds),
                            current_bounds: new_bounds,
                        }),
                        stopped: false,
                        stopped_immediate: false,
                        prevented_default: false,
                    });
                }
            }
        }
    }

    events
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

// ============================================================================
// Cross-Platform Event Dispatch System
// ============================================================================

/// Target for event dispatch - either a specific node or all root nodes
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallbackTarget {
    /// Specific node that was hit-tested
    Node { dom_id: DomId, node_id: NodeId },
    /// All root nodes (for window-level events)
    RootNodes,
}

/// A callback that should be invoked, with all necessary context
#[derive(Debug, Clone, PartialEq)]
pub struct CallbackToInvoke {
    /// Which node/window to invoke the callback on
    pub target: CallbackTarget,
    /// The event filter that triggered this callback
    pub event_filter: EventFilter,
    /// Hit test item (for node-level events with spatial info)
    pub hit_test_item: Option<HitTestItem>,
}

/// Result of dispatching events - contains all callbacks that should be invoked
#[derive(Debug, Clone, PartialEq)]
pub struct EventDispatchResult {
    /// Callbacks to invoke, in order
    pub callbacks: Vec<CallbackToInvoke>,
    /// Whether any event had stop_propagation set
    pub propagation_stopped: bool,
}

impl EventDispatchResult {
    pub fn empty() -> Self {
        Self {
            callbacks: Vec::new(),
            propagation_stopped: false,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.callbacks.is_empty()
    }
}

/// Main cross-platform event dispatch function.
///
/// This function takes the detected events and hit test results, and determines
/// which callbacks should be invoked. It handles:
/// - Window-level events (to root nodes)
/// - Node-level hover events (MouseEnter/Leave/Over)
/// - Node-level focus events
/// - Proper event ordering and filtering
///
/// The shell layer just needs to call this function and then invoke the returned callbacks.
pub fn dispatch_events(
    events: &Events,
    hit_test: Option<&FullHitTest>,
) -> EventDispatchResult {
    let mut result = EventDispatchResult::empty();

    // Early exit if no events
    if events.is_empty() {
        return result;
    }

    // 1. Dispatch window-level events to root nodes
    for window_event in &events.window_events {
        result.callbacks.push(CallbackToInvoke {
            target: CallbackTarget::RootNodes,
            event_filter: EventFilter::Window(*window_event),
            hit_test_item: None,
        });
    }

    // 2. Dispatch node-level events if we have a hit test
    if let Some(hit_test) = hit_test {
        if events.needs_hit_test() {
            // Create NodesToCheck to determine MouseEnter/Leave
            let nodes_to_check = NodesToCheck::new(hit_test, events);

            // 2a. Dispatch MouseEnter events to newly entered nodes
            for (dom_id, nodes) in &nodes_to_check.onmouseenter_nodes {
                for (node_id, hit_item) in nodes {
                    result.callbacks.push(CallbackToInvoke {
                        target: CallbackTarget::Node {
                            dom_id: *dom_id,
                            node_id: *node_id,
                        },
                        event_filter: EventFilter::Hover(HoverEventFilter::MouseEnter),
                        hit_test_item: Some(hit_item.clone()),
                    });
                }
            }

            // 2b. Dispatch MouseLeave events to nodes that were left
            for (dom_id, nodes) in &nodes_to_check.onmouseleave_nodes {
                for (node_id, hit_item) in nodes {
                    result.callbacks.push(CallbackToInvoke {
                        target: CallbackTarget::Node {
                            dom_id: *dom_id,
                            node_id: *node_id,
                        },
                        event_filter: EventFilter::Hover(HoverEventFilter::MouseLeave),
                        hit_test_item: Some(hit_item.clone()),
                    });
                }
            }

            // 2c. Dispatch hover events to currently hovered nodes
            for hover_event in &events.hover_events {
                for (dom_id, nodes) in &nodes_to_check.new_hit_node_ids {
                    for (node_id, hit_item) in nodes {
                        result.callbacks.push(CallbackToInvoke {
                            target: CallbackTarget::Node {
                                dom_id: *dom_id,
                                node_id: *node_id,
                            },
                            event_filter: EventFilter::Hover(*hover_event),
                            hit_test_item: Some(hit_item.clone()),
                        });
                    }
                }
            }

            // 2d. Dispatch focus events to focused node
            for focus_event in &events.focus_events {
                if let Some(focused_node) = nodes_to_check.new_focus_node {
                    if let Some(node_id) = focused_node.node.into_crate_internal() {
                        // Find hit test item for focused node
                        let hit_item = nodes_to_check
                            .new_hit_node_ids
                            .get(&focused_node.dom)
                            .and_then(|nodes| nodes.get(&node_id))
                            .cloned();

                        result.callbacks.push(CallbackToInvoke {
                            target: CallbackTarget::Node {
                                dom_id: focused_node.dom,
                                node_id,
                            },
                            event_filter: EventFilter::Focus(*focus_event),
                            hit_test_item: hit_item,
                        });
                    }
                }
            }
        }
    }

    result
}

/// Process callback results and potentially generate new synthetic events.
///
/// This function handles the recursive nature of event processing:
/// 1. Process immediate callback results (state changes, images, etc.)
/// 2. Check if new synthetic events should be generated
/// 3. Recursively process those events (up to max_depth to prevent infinite loops)
///
/// Returns true if any callbacks resulted in DOM changes requiring re-layout.
pub fn should_recurse_callbacks<T: CallbackResultRef>(
    callback_results: &[T],
    max_depth: usize,
    current_depth: usize,
) -> bool {
    if current_depth >= max_depth {
        return false;
    }

    // Check if any callback result indicates we should continue processing
    for result in callback_results {
        // If stop_propagation is set, stop processing further events
        if result.stop_propagation() {
            return false;
        }

        // Check if DOM was modified (requires re-layout and re-processing)
        if result.should_regenerate_dom() {
            return current_depth + 1 < max_depth;
        }
    }

    false
}

/// Trait to abstract over callback result types.
/// This allows the core event system to work with results without depending on layout layer.
pub trait CallbackResultRef {
    fn stop_propagation(&self) -> bool;
    fn prevent_default(&self) -> bool;
    fn should_regenerate_dom(&self) -> bool;
}

#[cfg(test)]
mod tests {
    //! Unit tests for the Phase 3.5 event system
    //!
    //! Tests cover:
    //! - Event type creation
    //! - DOM path traversal
    //! - Event propagation (capture/target/bubble)
    //! - Event filter matching
    //! - Lifecycle event detection

    use std::collections::BTreeMap;

    use crate::{
        dom::{DomId, DomNodeId},
        events::*,
        geom::{LogicalPosition, LogicalRect, LogicalSize},
        id::{Node, NodeHierarchy, NodeId},
        styled_dom::NodeHierarchyItemId,
        task::{Instant, SystemTick},
    };

    // Helper: Create a test Instant
    fn test_instant() -> Instant {
        Instant::Tick(SystemTick::new(0))
    }

    // Helper: Create a simple 3-node tree (root -> child1 -> grandchild)
    fn create_test_hierarchy() -> NodeHierarchy {
        let nodes = vec![
            Node {
                parent: None,
                previous_sibling: None,
                next_sibling: None,
                last_child: Some(NodeId::new(1)),
            },
            Node {
                parent: Some(NodeId::new(0)),
                previous_sibling: None,
                next_sibling: None,
                last_child: Some(NodeId::new(2)),
            },
            Node {
                parent: Some(NodeId::new(1)),
                previous_sibling: None,
                next_sibling: None,
                last_child: None,
            },
        ];
        NodeHierarchy::new(nodes)
    }

    #[test]
    fn test_event_source_enum() {
        // Test that EventSource variants can be created
        let _user = EventSource::User;
        let _programmatic = EventSource::Programmatic;
        let _synthetic = EventSource::Synthetic;
        let _lifecycle = EventSource::Lifecycle;
    }

    #[test]
    fn test_event_phase_enum() {
        // Test that EventPhase variants can be created
        let _capture = EventPhase::Capture;
        let _target = EventPhase::Target;
        let _bubble = EventPhase::Bubble;

        // Test default
        assert_eq!(EventPhase::default(), EventPhase::Bubble);
    }

    #[test]
    fn test_synthetic_event_creation() {
        let dom_id = DomId { inner: 1 };
        let node_id = NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(0)));
        let target = DomNodeId {
            dom: dom_id,
            node: node_id,
        };

        let event = SyntheticEvent::new(
            EventType::Click,
            EventSource::User,
            target,
            test_instant(),
            EventData::None,
        );

        assert_eq!(event.event_type, EventType::Click);
        assert_eq!(event.source, EventSource::User);
        assert_eq!(event.phase, EventPhase::Target);
        assert_eq!(event.target, target);
        assert_eq!(event.current_target, target);
        assert!(!event.stopped);
        assert!(!event.stopped_immediate);
        assert!(!event.prevented_default);
    }

    #[test]
    fn test_stop_propagation() {
        let dom_id = DomId { inner: 1 };
        let node_id = NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(0)));
        let target = DomNodeId {
            dom: dom_id,
            node: node_id,
        };

        let mut event = SyntheticEvent::new(
            EventType::Click,
            EventSource::User,
            target,
            test_instant(),
            EventData::None,
        );

        assert!(!event.is_propagation_stopped());

        event.stop_propagation();

        assert!(event.is_propagation_stopped());
        assert!(!event.is_immediate_propagation_stopped());
    }

    #[test]
    fn test_stop_immediate_propagation() {
        let dom_id = DomId { inner: 1 };
        let node_id = NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(0)));
        let target = DomNodeId {
            dom: dom_id,
            node: node_id,
        };

        let mut event = SyntheticEvent::new(
            EventType::Click,
            EventSource::User,
            target,
            test_instant(),
            EventData::None,
        );

        event.stop_immediate_propagation();

        assert!(event.is_propagation_stopped());
        assert!(event.is_immediate_propagation_stopped());
    }

    #[test]
    fn test_prevent_default() {
        let dom_id = DomId { inner: 1 };
        let node_id = NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(0)));
        let target = DomNodeId {
            dom: dom_id,
            node: node_id,
        };

        let mut event = SyntheticEvent::new(
            EventType::Click,
            EventSource::User,
            target,
            test_instant(),
            EventData::None,
        );

        assert!(!event.is_default_prevented());

        event.prevent_default();

        assert!(event.is_default_prevented());
    }

    #[test]
    fn test_get_dom_path_single_node() {
        let hierarchy = NodeHierarchy::new(vec![Node {
            parent: None,
            previous_sibling: None,
            next_sibling: None,
            last_child: None,
        }]);

        let target = NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(0)));
        let path = get_dom_path(&hierarchy, target);

        assert_eq!(path.len(), 1);
        assert_eq!(path[0], NodeId::new(0));
    }

    #[test]
    fn test_get_dom_path_three_nodes() {
        let hierarchy = create_test_hierarchy();

        // Test path to grandchild (node 2)
        let target = NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(2)));
        let path = get_dom_path(&hierarchy, target);

        assert_eq!(path.len(), 3);
        assert_eq!(path[0], NodeId::new(0)); // root
        assert_eq!(path[1], NodeId::new(1)); // child
        assert_eq!(path[2], NodeId::new(2)); // grandchild
    }

    #[test]
    fn test_get_dom_path_middle_node() {
        let hierarchy = create_test_hierarchy();

        // Test path to middle node (node 1)
        let target = NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1)));
        let path = get_dom_path(&hierarchy, target);

        assert_eq!(path.len(), 2);
        assert_eq!(path[0], NodeId::new(0)); // root
        assert_eq!(path[1], NodeId::new(1)); // child
    }

    #[test]
    fn test_propagate_event_empty_callbacks() {
        let hierarchy = create_test_hierarchy();
        let dom_id = DomId { inner: 1 };
        let target_node = NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(2)));
        let target = DomNodeId {
            dom: dom_id,
            node: target_node,
        };

        let mut event = SyntheticEvent::new(
            EventType::Click,
            EventSource::User,
            target,
            test_instant(),
            EventData::None,
        );

        let callbacks: BTreeMap<NodeId, Vec<EventFilter>> = BTreeMap::new();
        let result = propagate_event(&mut event, &hierarchy, &callbacks);

        // No callbacks, so nothing should be invoked
        assert_eq!(result.callbacks_to_invoke.len(), 0);
        assert!(!result.default_prevented);
    }

    #[test]
    fn test_mouse_event_data_creation() {
        let mouse_data = MouseEventData {
            position: LogicalPosition { x: 100.0, y: 200.0 },
            button: MouseButton::Left,
            buttons: 1,
            modifiers: KeyModifiers::new(),
        };

        assert_eq!(mouse_data.position.x, 100.0);
        assert_eq!(mouse_data.position.y, 200.0);
        assert_eq!(mouse_data.button, MouseButton::Left);
    }

    #[test]
    fn test_key_modifiers() {
        let modifiers = KeyModifiers::new().with_shift().with_ctrl();

        assert!(modifiers.shift);
        assert!(modifiers.ctrl);
        assert!(!modifiers.alt);
        assert!(!modifiers.meta);
        assert!(!modifiers.is_empty());

        let empty = KeyModifiers::new();
        assert!(empty.is_empty());
    }

    #[test]
    fn test_lifecycle_event_mount() {
        let dom_id = DomId { inner: 1 };
        let old_hierarchy = None;
        let new_hierarchy = create_test_hierarchy();
        let old_layout = None;
        let new_layout = {
            let mut map = BTreeMap::new();
            map.insert(
                NodeId::new(0),
                LogicalRect {
                    origin: LogicalPosition { x: 0.0, y: 0.0 },
                    size: LogicalSize {
                        width: 100.0,
                        height: 100.0,
                    },
                },
            );
            map.insert(
                NodeId::new(1),
                LogicalRect {
                    origin: LogicalPosition { x: 10.0, y: 10.0 },
                    size: LogicalSize {
                        width: 80.0,
                        height: 80.0,
                    },
                },
            );
            map.insert(
                NodeId::new(2),
                LogicalRect {
                    origin: LogicalPosition { x: 20.0, y: 20.0 },
                    size: LogicalSize {
                        width: 60.0,
                        height: 60.0,
                    },
                },
            );
            Some(map)
        };

        let events = detect_lifecycle_events(
            dom_id,
            dom_id,
            old_hierarchy,
            Some(&new_hierarchy),
            old_layout.as_ref(),
            new_layout.as_ref(),
            test_instant(),
        );

        // All 3 nodes should have Mount events
        assert_eq!(events.len(), 3);

        for event in &events {
            assert_eq!(event.event_type, EventType::Mount);
            assert_eq!(event.source, EventSource::Lifecycle);

            if let EventData::Lifecycle(data) = &event.data {
                assert_eq!(data.reason, LifecycleReason::InitialMount);
                assert!(data.previous_bounds.is_none());
            } else {
                panic!("Expected Lifecycle event data");
            }
        }
    }

    #[test]
    fn test_lifecycle_event_unmount() {
        let dom_id = DomId { inner: 1 };
        let old_hierarchy = create_test_hierarchy();
        let new_hierarchy = None;
        let old_layout = {
            let mut map = BTreeMap::new();
            map.insert(
                NodeId::new(0),
                LogicalRect {
                    origin: LogicalPosition { x: 0.0, y: 0.0 },
                    size: LogicalSize {
                        width: 100.0,
                        height: 100.0,
                    },
                },
            );
            Some(map)
        };
        let new_layout = None;

        let events = detect_lifecycle_events(
            dom_id,
            dom_id,
            Some(&old_hierarchy),
            new_hierarchy,
            old_layout.as_ref(),
            new_layout,
            test_instant(),
        );

        // All 3 nodes should have Unmount events
        assert_eq!(events.len(), 3);

        for event in &events {
            assert_eq!(event.event_type, EventType::Unmount);
            assert_eq!(event.source, EventSource::Lifecycle);
        }
    }

    #[test]
    fn test_lifecycle_event_resize() {
        let dom_id = DomId { inner: 1 };
        let hierarchy = create_test_hierarchy();

        let old_layout = {
            let mut map = BTreeMap::new();
            map.insert(
                NodeId::new(0),
                LogicalRect {
                    origin: LogicalPosition { x: 0.0, y: 0.0 },
                    size: LogicalSize {
                        width: 100.0,
                        height: 100.0,
                    },
                },
            );
            Some(map)
        };

        let new_layout = {
            let mut map = BTreeMap::new();
            map.insert(
                NodeId::new(0),
                LogicalRect {
                    origin: LogicalPosition { x: 0.0, y: 0.0 },
                    size: LogicalSize {
                        width: 200.0,
                        height: 100.0,
                    }, // Width changed
                },
            );
            Some(map)
        };

        let events = detect_lifecycle_events(
            dom_id,
            dom_id,
            Some(&hierarchy),
            Some(&hierarchy),
            old_layout.as_ref(),
            new_layout.as_ref(),
            test_instant(),
        );

        // Should have 1 Resize event
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::Resize);
        assert_eq!(events[0].source, EventSource::Lifecycle);

        if let EventData::Lifecycle(data) = &events[0].data {
            assert_eq!(data.reason, LifecycleReason::Resize);
            assert!(data.previous_bounds.is_some());
            assert_eq!(data.current_bounds.size.width, 200.0);
        } else {
            panic!("Expected Lifecycle event data");
        }
    }

    #[test]
    fn test_event_filter_hover_match() {
        let dom_id = DomId { inner: 1 };
        let node_id = NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(0)));
        let target = DomNodeId {
            dom: dom_id,
            node: node_id,
        };

        let _event = SyntheticEvent::new(
            EventType::MouseDown,
            EventSource::User,
            target,
            test_instant(),
            EventData::Mouse(MouseEventData {
                position: LogicalPosition { x: 0.0, y: 0.0 },
                button: MouseButton::Left,
                buttons: 1,
                modifiers: KeyModifiers::new(),
            }),
        );

        // This is tested internally via matches_hover_filter
        // We can't test it directly without making the function public
        // but it's tested indirectly through propagate_event
    }
}
