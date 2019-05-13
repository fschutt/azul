use std::{
    collections::{HashSet, BTreeMap},
    fmt,
    path::PathBuf,
};
use glium::glutin::{
    WindowEvent, KeyboardInput, ElementState,
    MouseScrollDelta, ModifiersState,
    dpi::LogicalPosition as WinitLogicalPosition,
};
use {
    app::FrameEventInfo,
    dom::{EventFilter, NotEventFilter, HoverEventFilter, FocusEventFilter, WindowEventFilter},
    callbacks:: {CallbackInfo, Callback, HitTestItem, DefaultCallbackId, UpdateScreen},
    id_tree::NodeId,
    ui_state::UiState,
    app::AppState,
};
pub use azul_core::window::{
    WindowState, KeyboardState, MouseState, DebugState, AcceleratorKey,
    LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize, WindowSize,
    WaylandTheme,
};
use azul_core::callbacks::FocusTarget;

pub(crate) mod winit_translate {

    use super::{LogicalPosition, LogicalSize};
    use super::{PhysicalPosition, PhysicalSize};
    use glium::glutin::dpi::{LogicalPosition as WinitLogicalPosition, LogicalSize as WinitLogicalSize};
    use glium::glutin::dpi::{PhysicalPosition as WinitPhysicalPosition, PhysicalSize as WinitPhysicalSize};

    pub(crate) fn translate_logical_position(input: LogicalPosition) -> WinitLogicalPosition {
        WinitLogicalPosition::new(input.x as f64, input.y as f64)
    }

    pub(crate) fn translate_logical_size(input: LogicalSize) -> WinitLogicalSize {
        WinitLogicalSize::new(input.width as f64, input.height as f64)

    }

    pub(crate) fn translate_physical_position(input: PhysicalPosition) -> WinitPhysicalPosition {
        WinitPhysicalPosition::new(input.x as f64, input.y as f64)
    }

    pub(crate) fn translate_physical_size(input: PhysicalSize) -> WinitPhysicalSize {
        WinitPhysicalSize::new(input.width as f64, input.height as f64)
    }

    use azul_core::window::MouseCursorType;
    use glium::glutin::MouseCursor as WinitMouseCursorType;

    pub(crate) fn translate_mouse_cursor_type(mouse_cursor_type: MouseCursorType) -> WinitMouseCursorType {
        use azul_core::window::MouseCursorType::*;
        match mouse_cursor_type {
            Default => WinitMouseCursorType::Default,
            Crosshair => WinitMouseCursorType::Crosshair,
            Hand => WinitMouseCursorType::Hand,
            Arrow => WinitMouseCursorType::Arrow,
            Move => WinitMouseCursorType::Move,
            Text => WinitMouseCursorType::Text,
            Wait => WinitMouseCursorType::Wait,
            Help => WinitMouseCursorType::Help,
            Progress => WinitMouseCursorType::Progress,
            NotAllowed => WinitMouseCursorType::NotAllowed,
            ContextMenu => WinitMouseCursorType::ContextMenu,
            Cell => WinitMouseCursorType::Cell,
            VerticalText => WinitMouseCursorType::VerticalText,
            Alias => WinitMouseCursorType::Alias,
            Copy => WinitMouseCursorType::Copy,
            NoDrop => WinitMouseCursorType::NoDrop,
            Grab => WinitMouseCursorType::Grab,
            Grabbing => WinitMouseCursorType::Grabbing,
            AllScroll => WinitMouseCursorType::AllScroll,
            ZoomIn => WinitMouseCursorType::ZoomIn,
            ZoomOut => WinitMouseCursorType::ZoomOut,
            EResize => WinitMouseCursorType::EResize,
            NResize => WinitMouseCursorType::NResize,
            NeResize => WinitMouseCursorType::NeResize,
            NwResize => WinitMouseCursorType::NwResize,
            SResize => WinitMouseCursorType::SResize,
            SeResize => WinitMouseCursorType::SeResize,
            SwResize => WinitMouseCursorType::SwResize,
            WResize => WinitMouseCursorType::WResize,
            EwResize => WinitMouseCursorType::EwResize,
            NsResize => WinitMouseCursorType::NsResize,
            NeswResize => WinitMouseCursorType::NeswResize,
            NwseResize => WinitMouseCursorType::NwseResize,
            ColResize => WinitMouseCursorType::ColResize,
            RowResize => WinitMouseCursorType::RowResize,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FullWindowState {
    /// Current title of the window
    pub title: String,
    /// Size of the window + max width / max height: 800 x 600 by default
    pub size: WindowSize,
    /// The x and y position, or None to let the WM decide where to put the window (default)
    pub position: Option<LogicalPosition>,
    /// Is the window currently maximized
    pub is_maximized: bool,
    /// Is the window currently fullscreened?
    pub is_fullscreen: bool,
    /// Does the window have decorations (close, minimize, maximize, title bar)?
    pub has_decorations: bool,
    /// Is the window currently visible?
    pub is_visible: bool,
    /// Is the window always on top?
    pub is_always_on_top: bool,
    /// Whether the window is resizable
    pub is_resizable: bool,
    /// Mostly used for debugging, shows WebRender-builtin graphs on the screen.
    /// Used for performance monitoring and displaying frame times (rendering-only).
    pub debug_state: DebugState,
    /// Current keyboard state - NOTE: mutating this field (currently) does nothing
    /// (doesn't get synchronized with OS-level window)!
    pub keyboard_state: KeyboardState,
    /// Current mouse state - NOTE: mutating this field (currently) does nothing
    /// (doesn't get synchronized with OS-level window)!
    pub mouse_state: MouseState,
    /// Sets location of IME candidate box in client area coordinates relative to the top left
    /// Supported on all platforms.
    pub ime_position: Option<LogicalPosition>,
    /// On X11, sets window urgency hint (`XUrgencyHint`). On macOs, requests user attention via
    pub request_user_attention: bool,
    /// Set the windows Wayland theme. Irrelevant on other platforms, set to `None`
    pub wayland_theme: Option<WaylandTheme>,

    // --

    /// Previous window state, used for determining mouseout, etc. events
    pub previous_window_state: Option<Box<FullWindowState>>,
    /// Whether there is a file currently hovering over the window
    pub hovered_file: Option<PathBuf>,
    /// What node is currently hovered over, default to None. Only necessary internal
    /// to the crate, for emitting `On::FocusReceived` and `On::FocusLost` events,
    /// as well as styling `:focus` elements
    pub focused_node: Option<NodeId>,
    /// Currently hovered nodes, default to an empty Vec. Important for
    /// styling `:hover` elements.
    pub hovered_nodes: BTreeMap<NodeId, HitTestItem>,
    /// Whether there is a focus field overwrite from the last callback calls.
    pub pending_focus_target: Option<FocusTarget>,
}

impl Default for FullWindowState {
    fn default() -> Self {
        use azul_core::window::DEFAULT_TITLE;
        Self {
            title: DEFAULT_TITLE.into(),
            size: WindowSize::default(),
            position: None,
            is_maximized: false,
            is_fullscreen: false,
            has_decorations: true,
            is_visible: true,
            is_always_on_top: false,
            is_resizable: true,
            debug_state: DebugState::default(),
            keyboard_state: KeyboardState::default(),
            mouse_state: MouseState::default(),
            ime_position: None,
            request_user_attention: false,
            wayland_theme: None,

            // --

            previous_window_state: None,
            hovered_file: None,
            focused_node: None,
            hovered_nodes: BTreeMap::default(),
            pending_focus_target: None,
        }
    }
}

impl FullWindowState {

    pub fn get_mouse_state(&self) -> &MouseState {
        &self.mouse_state
    }

    pub fn get_keyboard_state(&self) -> &KeyboardState {
        &self.keyboard_state
    }

    pub fn get_hovered_file(&self) -> Option<&PathBuf> {
        self.hovered_file.as_ref()
    }

    /// Returns the window state of the previous frame, useful for calculating
    /// metrics for dragging motions. Note that you can't call this function
    /// recursively - calling `get_previous_window_state()` on the returned
    /// `WindowState` will yield a `None` value.
    pub(crate) fn get_previous_window_state(&self) -> Option<&Box<FullWindowState>> {
        self.previous_window_state.as_ref()
    }
}

/// Creates a FullWindowState from a regular WindowState, fills non-available
/// fields with their default values
pub(crate) fn full_window_state_from_normal_state(window_state: WindowState) -> FullWindowState {
    FullWindowState {
        title: window_state.title,
        position: window_state.position,
        size: window_state.size,
        is_maximized: window_state.is_maximized,
        is_fullscreen: window_state.is_fullscreen,
        has_decorations: window_state.has_decorations,
        is_visible: window_state.is_visible,
        is_always_on_top: window_state.is_always_on_top,
        mouse_state: window_state.mouse_state,
        keyboard_state: window_state.keyboard_state,
        debug_state: window_state.debug_state,
        .. Default::default()
    }
}

fn update_keyboard_state_from_modifier_state(keyboard_state: &mut KeyboardState, state: ModifiersState) {
    keyboard_state.shift_down = state.shift;
    keyboard_state.ctrl_down = state.ctrl;
    keyboard_state.alt_down = state.alt;
    keyboard_state.super_down = state.logo;
}

pub(crate) struct DetermineCallbackResult<T> {
    pub(crate) hit_test_item: Option<HitTestItem>,
    pub(crate) default_callbacks: BTreeMap<EventFilter, DefaultCallbackId>,
    pub(crate) normal_callbacks: BTreeMap<EventFilter, Callback<T>>,
}

impl<T> Default for DetermineCallbackResult<T> {
    fn default() -> Self {
        DetermineCallbackResult {
            hit_test_item: None,
            default_callbacks: BTreeMap::new(),
            normal_callbacks: BTreeMap::new(),
        }
    }
}

impl<T> Clone for DetermineCallbackResult<T> {
    fn clone(&self) -> Self {
        DetermineCallbackResult {
            hit_test_item: self.hit_test_item.clone(),
            default_callbacks: self.default_callbacks.clone(),
            normal_callbacks: self.normal_callbacks.clone(),
        }
    }
}

pub(crate) struct CallbacksOfHitTest<T> {
    /// A BTreeMap where each item is already filtered by the proper hit-testing type,
    /// meaning in order to get the proper callbacks, you simply have to iterate through
    /// all node IDs
    pub nodes_with_callbacks: BTreeMap<NodeId, DetermineCallbackResult<T>>,
    /// Whether the screen should be redrawn even if no Callback returns an `UpdateScreen::Redraw`.
    /// This is necessary for `:hover` and `:active` mouseovers - otherwise the screen would
    /// only update on the next resize.
    pub needs_redraw_anyways: bool,
    /// Same as `needs_redraw_anyways`, but for reusing the layout from the previous frame.
    /// Each `:hover` and `:active` group stores whether it modifies the layout, as
    /// a performance optimization.
    pub needs_relayout_anyways: bool,
}

impl<T> fmt::Debug for DetermineCallbackResult<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}, {:?}, {:?}", self.hit_test_item, self.default_callbacks, self.normal_callbacks)
    }
}

impl<T> Default for CallbacksOfHitTest<T> {
    fn default() -> Self {
        Self {
            nodes_with_callbacks: BTreeMap::new(),
            needs_redraw_anyways: false,
            needs_relayout_anyways: false,
        }
    }
}

/// Determine which event / which callback(s) should be called and in which order
///
/// This function also updates / mutates the current window state, so that
/// the window state is updated for the next frame
pub(crate) fn determine_callbacks<T>(
    window_state: &mut FullWindowState,
    hit_test_items: &[HitTestItem],
    event: &WindowEvent,
    ui_state: &UiState<T>,
) -> CallbacksOfHitTest<T> {

    use std::collections::BTreeSet;

    // Store the current window state so we can set it in this.previous_window_state later on
    let mut previous_state = Box::new(window_state.clone());
    previous_state.previous_window_state = None;

    let mut needs_hover_redraw = false;
    let mut needs_hover_relayout = false;

    // BTreeMap<NodeId, DetermineCallbackResult<T>>
    let mut nodes_with_callbacks: BTreeMap<NodeId, DetermineCallbackResult<T>> = BTreeMap::new();

    let current_window_events = get_window_events(window_state, event);
    let current_hover_events = get_hover_events(&current_window_events);
    let current_focus_events = get_focus_events(&current_hover_events);

    let event_was_mouse_down = if let WindowEvent::MouseInput { state: ElementState::Pressed, .. } = event { true } else { false };
    let event_was_mouse_release = if let WindowEvent::MouseInput { state: ElementState::Released, .. } = event { true } else { false };
    let event_was_mouse_enter = if let WindowEvent::CursorEntered { .. } = event { true } else { false };
    let event_was_mouse_leave = if let WindowEvent::CursorLeft { .. } = event { true } else { false };

    // TODO: If the current mouse is down, but the event
    // wasn't a click, that means it was a drag

    // Figure out what the hovered NodeIds are
    let mut new_hit_node_ids: BTreeMap<NodeId, HitTestItem> = hit_test_items.iter().filter_map(|hit_test_item| {
        ui_state.tag_ids_to_node_ids
        .get(&hit_test_item.tag.0)
        .map(|node_id| (*node_id, hit_test_item.clone()))
    }).collect();

    if event_was_mouse_leave {
        new_hit_node_ids = BTreeMap::new();
    }

    // Figure out what the current focused NodeId is
    if event_was_mouse_down || event_was_mouse_release {

        // Find the first (closest to cursor in hierarchy) item that has a tabindex
        let closest_focus_node = hit_test_items.iter().rev()
        .find_map(|item| ui_state.tab_index_tags.get(&item.tag.0))
        .cloned();

        // Even if the focused node is None, we still have to update window_state.focused_node!
        window_state.focused_node = closest_focus_node.map(|(node_id, _tab_idx)| node_id);
    }

    macro_rules! insert_only_non_empty_callbacks {
        ($node_id:expr, $hit_test_item:expr, $normal_hover_callbacks:expr, $default_hover_callbacks:expr) => ({
            if !($normal_hover_callbacks.is_empty() && $default_hover_callbacks.is_empty()) {
                let mut callback_result = nodes_with_callbacks.entry(*$node_id)
                .or_insert_with(|| DetermineCallbackResult::default());

                let item: Option<HitTestItem> = $hit_test_item;
                if let Some(hit_test_item) = item {
                    callback_result.hit_test_item = Some(hit_test_item);
                }
                callback_result.normal_callbacks.extend($normal_hover_callbacks.into_iter());
                callback_result.default_callbacks.extend($default_hover_callbacks.into_iter());
            }
        })
    }

    // Inserts the events from a given NodeId and an Option<HitTestItem> into the nodes_with_callbacks
    macro_rules! insert_callbacks {(
        $node_id:expr,
        $hit_test_item:expr,
        $hover_callbacks:ident,
        $hover_default_callbacks:ident,
        $current_hover_events:ident,
        $event_filter:ident
    ) => ({
            // BTreeMap<EventFilter, Callback<T>>
            let mut normal_hover_callbacks = BTreeMap::new();

            // Insert all normal Hover events
            if let Some(ui_state_hover_event_filters) = ui_state.$hover_callbacks.get($node_id) {
                for current_hover_event in &$current_hover_events {
                    if let Some(callback) = ui_state_hover_event_filters.get(current_hover_event) {
                        normal_hover_callbacks.insert(EventFilter::$event_filter(*current_hover_event), *callback);
                    }
                }
            }

            // BTreeMap<EventFilter, DefaultCallbackId>
            let mut default_hover_callbacks = BTreeMap::new();

            // Insert all default Hover events
            if let Some(ui_state_hover_default_event_filters) = ui_state.$hover_default_callbacks.get($node_id) {
                for current_hover_event in &$current_hover_events {
                    if let Some(callback_id) = ui_state_hover_default_event_filters.get(current_hover_event) {
                        default_hover_callbacks.insert(EventFilter::$event_filter(*current_hover_event), *callback_id);
                    }
                }
            }

            insert_only_non_empty_callbacks!($node_id, $hit_test_item, normal_hover_callbacks, default_hover_callbacks);
        })
    }

    // Insert all normal window events
    for (window_node_id, window_callbacks) in &ui_state.window_callbacks {
        let normal_window_callbacks = window_callbacks.iter()
            .filter(|(current_window_event, _)| current_window_events.contains(current_window_event))
            .map(|(current_window_event, callback)| (EventFilter::Window(*current_window_event), *callback))
            .collect::<BTreeMap<_, _>>();
        let default_window_callbacks = BTreeMap::<EventFilter, DefaultCallbackId>::new();
        insert_only_non_empty_callbacks!(window_node_id, None, normal_window_callbacks, default_window_callbacks);
    }

    // Insert all default window events
    for (window_node_id, window_callbacks) in &ui_state.window_default_callbacks {
        let normal_window_callbacks = BTreeMap::<EventFilter, Callback<T>>::new();
        let default_window_callbacks = window_callbacks.iter()
            .filter(|(current_window_event, _)| current_window_events.contains(current_window_event))
            .map(|(current_window_event, callback)| (EventFilter::Window(*current_window_event), *callback))
            .collect::<BTreeMap<_, _>>();
        insert_only_non_empty_callbacks!(window_node_id, None, normal_window_callbacks, default_window_callbacks);
    }

    // Insert (normal + default) hover events
    for (hover_node_id, hit_test_item) in &new_hit_node_ids {
        insert_callbacks!(hover_node_id, Some(hit_test_item.clone()), hover_callbacks, hover_default_callbacks, current_hover_events, Hover);
    }

    // Insert (normal + default) focus events
    if let Some(current_focused_node) = &window_state.focused_node {
        insert_callbacks!(current_focused_node, None, focus_callbacks, focus_default_callbacks, current_focus_events, Focus);
    }

    // If the last focused node and the current focused node aren't the same,
    // submit a FocusLost for the last node and a FocusReceived for the current one.
    let mut focus_received_lost_events: BTreeMap<NodeId, FocusEventFilter> = BTreeMap::new();
    match (window_state.focused_node, previous_state.focused_node) {
        (Some(cur), None) => {
            focus_received_lost_events.insert(cur, FocusEventFilter::FocusReceived);
        },
        (None, Some(prev)) => {
            focus_received_lost_events.insert(prev, FocusEventFilter::FocusLost);
        },
        (Some(cur), Some(prev)) => {
            if cur != prev {
                focus_received_lost_events.insert(cur, FocusEventFilter::FocusReceived);
                focus_received_lost_events.insert(prev, FocusEventFilter::FocusLost);
            }
        }
        (None, None) => { },
    }

    // Insert FocusReceived / FocusLost
    for (node_id, focus_event) in &focus_received_lost_events {
        let current_focus_leave_events = [focus_event.clone()];
        insert_callbacks!(node_id, None, focus_callbacks, focus_default_callbacks, current_focus_leave_events, Focus);
    }

    macro_rules! mouse_enter {
        ($node_id:expr, $hit_test_item:expr, $event_filter:ident) => ({

            let node_is_focused = window_state.focused_node == Some($node_id);

            // BTreeMap<EventFilter, Callback<T>>
            let mut normal_callbacks = BTreeMap::new();

            // Insert all normal Hover(MouseEnter) events
            if let Some(ui_state_hover_event_filters) = ui_state.hover_callbacks.get(&$node_id) {
                if let Some(callback) = ui_state_hover_event_filters.get(&HoverEventFilter::$event_filter) {
                    normal_callbacks.insert(EventFilter::Hover(HoverEventFilter::$event_filter), *callback);
                }
            }

            // Insert all normal Focus(MouseEnter) events
            if node_is_focused {
                if let Some(ui_state_focus_event_filters) = ui_state.focus_callbacks.get(&$node_id) {
                    if let Some(callback) = ui_state_focus_event_filters.get(&FocusEventFilter::$event_filter) {
                        normal_callbacks.insert(EventFilter::Focus(FocusEventFilter::$event_filter), *callback);
                    }
                }
            }

            // BTreeMap<EventFilter, DefaultCallbackId>
            let mut default_callbacks = BTreeMap::new();

            // Insert all default Hover(MouseEnter) events
            if let Some(ui_state_hover_default_event_filters) = ui_state.hover_default_callbacks.get(&$node_id) {
                if let Some(callback_id) = ui_state_hover_default_event_filters.get(&HoverEventFilter::$event_filter) {
                    default_callbacks.insert(EventFilter::Hover(HoverEventFilter::$event_filter), *callback_id);
                }
            }

            // Insert all default Focus(MouseEnter) events
            if node_is_focused {
                if let Some(ui_state_focus_default_event_filters) = ui_state.focus_default_callbacks.get(&$node_id) {
                    if let Some(callback_id) = ui_state_focus_default_event_filters.get(&FocusEventFilter::$event_filter) {
                        default_callbacks.insert(EventFilter::Focus(FocusEventFilter::$event_filter), *callback_id);
                    }
                }
            }

            if !(default_callbacks.is_empty() && normal_callbacks.is_empty()) {

                let mut callback_result = nodes_with_callbacks.entry($node_id)
                .or_insert_with(|| DetermineCallbackResult::default());

                callback_result.hit_test_item = Some($hit_test_item);
                callback_result.normal_callbacks.extend(normal_callbacks.into_iter());
                callback_result.default_callbacks.extend(default_callbacks.into_iter());
            }

            if let Some((_, hover_group)) = ui_state.node_ids_to_tag_ids.get(&$node_id).and_then(|tag_for_this_node| {
                ui_state.tag_ids_to_hover_active_states.get(&tag_for_this_node)
            }) {
                // We definitely need to redraw (on any :hover) change
                needs_hover_redraw = true;
                // Only set this to true if the :hover group actually affects the layout
                if hover_group.affects_layout {
                    needs_hover_relayout = true;
                }
            }
        })
    }

    // Collect all On::MouseEnter nodes (for both hover and focus events)
    let onmouseenter_nodes: BTreeMap<NodeId, HitTestItem> = new_hit_node_ids.iter()
        .filter(|(current_node_id, _)| previous_state.hovered_nodes.get(current_node_id).is_none())
        .map(|(x, y)| (*x, y.clone()))
        .collect();

    let onmouseenter_empty = onmouseenter_nodes.is_empty();

    // Insert Focus(MouseEnter) and Hover(MouseEnter)
    for (node_id, hit_test_item) in onmouseenter_nodes {
        mouse_enter!(node_id, hit_test_item, MouseEnter);
    }

    // Collect all On::MouseLeave nodes (for both hover and focus events)
    let onmouseleave_nodes: BTreeMap<NodeId, HitTestItem> = previous_state.hovered_nodes.iter()
        .filter(|(prev_node_id, _)| new_hit_node_ids.get(prev_node_id).is_none())
        .map(|(x, y)| (*x, y.clone()))
        .collect();

    let onmouseleave_empty = onmouseleave_nodes.is_empty();

    // Insert Focus(MouseEnter) and Hover(MouseEnter)
    for (node_id, hit_test_item) in onmouseleave_nodes {
        mouse_enter!(node_id, hit_test_item, MouseLeave);
    }

    // If the mouse is down, but was up previously or vice versa, that means
    // that a :hover or :active state may be invalidated. In that case we need
    // to redraw the screen anyways. Setting relayout to true here in order to
    let event_is_click_or_release = window_state.mouse_state.mouse_down() != previous_state.mouse_state.mouse_down();
    if event_is_click_or_release || event_was_mouse_enter || event_was_mouse_leave || !onmouseenter_empty || !onmouseleave_empty {
        needs_hover_redraw = true;
        needs_hover_relayout = true;
    }

    // Insert all Not-callbacks, we need to filter out all Hover and Focus callbacks
    // and then look at what callbacks were currently

    // In order to create the Not Events we have to record which events were fired and on what nodes
    // Then we need to go through the events and fire them if the event was present, but the NodeID was not
    let mut reverse_event_hover_normal_list = BTreeMap::<HoverEventFilter, BTreeSet<NodeId>>::new();
    let mut reverse_event_focus_normal_list = BTreeMap::<FocusEventFilter, BTreeSet<NodeId>>::new();
    let mut reverse_event_hover_default_list = BTreeMap::<HoverEventFilter, BTreeSet<NodeId>>::new();
    let mut reverse_event_focus_default_list = BTreeMap::<FocusEventFilter, BTreeSet<NodeId>>::new();

    for (node_id, DetermineCallbackResult { default_callbacks, normal_callbacks, .. }) in &nodes_with_callbacks {
        for event_filter in normal_callbacks.keys() {
            match event_filter {
                EventFilter::Hover(h) => {
                    reverse_event_hover_normal_list.entry(*h).or_insert_with(|| BTreeSet::new()).insert(*node_id);
                },
                EventFilter::Focus(f) => {
                    reverse_event_focus_normal_list.entry(*f).or_insert_with(|| BTreeSet::new()).insert(*node_id);
                },
                _ => { },
            }
        }
        for event_filter in default_callbacks.keys() {
            match event_filter {
                EventFilter::Hover(h) => {
                    reverse_event_hover_default_list.entry(*h).or_insert_with(|| BTreeSet::new()).insert(*node_id);
                },
                EventFilter::Focus(f) => {
                    reverse_event_focus_default_list.entry(*f).or_insert_with(|| BTreeSet::new()).insert(*node_id);
                },
                _ => { },
            }
        }
    }

    // Insert NotEventFilter callbacks
    for (node_id, not_event_filter_callback_list) in &ui_state.not_callbacks {
        for (event_filter, event_callback) in not_event_filter_callback_list {
            // If we have the event filter, but we don't have the NodeID, then insert the callback
            match event_filter {
                NotEventFilter::Hover(h) => {
                    if let Some(on_node_ids) = reverse_event_hover_normal_list.get(&h) {
                        if !on_node_ids.contains(node_id) {
                            nodes_with_callbacks.entry(*node_id)
                            .or_insert_with(|| DetermineCallbackResult::default())
                            .normal_callbacks.insert(EventFilter::Not(*event_filter), *event_callback);
                        }
                    }
                    // TODO: Same thing for default callbacks here
                },
                NotEventFilter::Focus(f) => {
                    // TODO: Same thing for focus
                }
            }
        }
    }

    window_state.hovered_nodes = new_hit_node_ids;
    window_state.previous_window_state = Some(previous_state);

    CallbacksOfHitTest {
        needs_redraw_anyways: needs_hover_redraw,
        needs_relayout_anyways: needs_hover_relayout,
        nodes_with_callbacks,
    }
}

// Returns the frame events + if the window should close
pub(crate) fn update_window_state(window_state: &mut FullWindowState, events: &[WindowEvent]) -> (FrameEventInfo, bool) {
    let mut frame_event_info = FrameEventInfo::default();
    let mut should_window_close = false;

    for event in events {
        if window_should_close(event, &mut frame_event_info) {
            should_window_close = true;
        }
        update_mouse_cursor_position(window_state, event);
        update_scroll_state(window_state, event);
        update_keyboard_modifiers(window_state, event);
        update_keyboard_pressed_chars(window_state, event);
        update_misc_events(window_state, event);
    }

    (frame_event_info, should_window_close)
}

fn update_keyboard_modifiers(window_state: &mut FullWindowState, event: &WindowEvent) {
    let modifiers = match event {
        WindowEvent::KeyboardInput { input: KeyboardInput { modifiers, .. }, .. } |
        WindowEvent::CursorMoved { modifiers, .. } |
        WindowEvent::MouseWheel { modifiers, .. } |
        WindowEvent::MouseInput { modifiers, .. } => {
            Some(modifiers)
        },
        _ => None,
    };

    if let Some(modifiers) = modifiers {
        update_keyboard_state_from_modifier_state(&mut window_state.keyboard_state, *modifiers);
    }
}

/// After the initial events are filtered, this will update the mouse
/// cursor position, if the event is a `CursorMoved` and set it to `None`
/// if the cursor has left the window
fn update_mouse_cursor_position(window_state: &mut FullWindowState, event: &WindowEvent) {
    use azul_core::window::CursorPosition;
    match event {
        WindowEvent::CursorMoved { position, .. } => {
            let world_pos_x = position.x as f32 / window_state.size.hidpi_factor * window_state.size.winit_hidpi_factor;
            let world_pos_y = position.y as f32 / window_state.size.hidpi_factor * window_state.size.winit_hidpi_factor;
            window_state.mouse_state.cursor_pos = CursorPosition::InWindow(LogicalPosition::new(world_pos_x, world_pos_y));
        },
        WindowEvent::CursorLeft { .. } => {
            window_state.mouse_state.cursor_pos = CursorPosition::OutOfWindow;
        },
        WindowEvent::CursorEntered { .. } => {
            window_state.mouse_state.cursor_pos = CursorPosition::InWindow(LogicalPosition::new(0.0, 0.0));
        },
        _ => { }
    }
}

fn update_scroll_state(window_state: &mut FullWindowState, event: &WindowEvent) {
    match event {
        WindowEvent::MouseWheel { delta, .. } => {
            const LINE_DELTA: f32 = 38.0;

            let (scroll_x_px, scroll_y_px) = match delta {
                MouseScrollDelta::PixelDelta(WinitLogicalPosition { x, y }) => (*x as f32, *y as f32),
                MouseScrollDelta::LineDelta(x, y) => (*x * LINE_DELTA, *y * LINE_DELTA),
            };
            window_state.mouse_state.scroll_x = -scroll_x_px;
            window_state.mouse_state.scroll_y = -scroll_y_px; // TODO: "natural scrolling"?
        },
        _ => { },
    }
}

/// Updates self.keyboard_state to reflect what characters are currently held down
fn update_keyboard_pressed_chars(window_state: &mut FullWindowState, event: &WindowEvent) {
    use wr_translate::winit_translate_virtual_keycode;
    match event {
        WindowEvent::KeyboardInput {
            input: KeyboardInput { state: ElementState::Pressed, virtual_keycode, scancode, .. }, ..
        } => {
            if let Some(vk) = virtual_keycode {
                let vk = winit_translate_virtual_keycode(*vk);
                window_state.keyboard_state.current_virtual_keycodes.insert(vk);
                window_state.keyboard_state.latest_virtual_keycode = Some(vk);
            }
            window_state.keyboard_state.current_scancodes.insert(*scancode);
        },
        // The char event is sliced inbetween a keydown and a keyup event
        // so the keyup has to clear the character again
        WindowEvent::ReceivedCharacter(c) => {
            window_state.keyboard_state.current_char = Some(*c);
        },
        WindowEvent::KeyboardInput {
            input: KeyboardInput { state: ElementState::Released, virtual_keycode, scancode, .. }, ..
        } => {
            if let Some(vk) = virtual_keycode {
                let vk = winit_translate_virtual_keycode(*vk);
                window_state.keyboard_state.current_virtual_keycodes.remove(&vk);
                window_state.keyboard_state.latest_virtual_keycode = None;
            }
            window_state.keyboard_state.current_scancodes.remove(scancode);
        },
        WindowEvent::Focused(false) => {
            window_state.keyboard_state.current_char = None;
            window_state.keyboard_state.current_virtual_keycodes.clear();
            window_state.keyboard_state.latest_virtual_keycode = None;
            window_state.keyboard_state.current_scancodes.clear();
        },
        _ => { },
    }
}

fn update_misc_events(window_state: &mut FullWindowState, event: &WindowEvent) {
    match event {
        WindowEvent::HoveredFile(path) => {
            window_state.hovered_file = Some(path.clone());
        },
        WindowEvent::DroppedFile(path) => {
            window_state.hovered_file = Some(path.clone());
        },
        WindowEvent::HoveredFileCancelled => {
            window_state.hovered_file = None;
        },
        _ => { },
    }
}

fn get_window_events(window_state: &mut FullWindowState, event: &WindowEvent) -> HashSet<WindowEventFilter> {

    use glium::glutin::MouseButton::*;

    let mut events_vec = HashSet::<WindowEventFilter>::new();

    match event {
        WindowEvent::MouseInput { state: ElementState::Pressed, button, .. } => {
            events_vec.insert(WindowEventFilter::MouseDown);
            match button {
                Left => {
                    events_vec.insert(WindowEventFilter::LeftMouseDown);
                    window_state.mouse_state.left_down = true;
                },
                Right => {
                    events_vec.insert(WindowEventFilter::RightMouseDown);
                    window_state.mouse_state.right_down = true;
                },
                Middle => {
                    events_vec.insert(WindowEventFilter::MiddleMouseDown);
                    window_state.mouse_state.middle_down = true;
                },
                _ => { }
            }
        },
        WindowEvent::MouseInput { state: ElementState::Released, button, .. } => {
            events_vec.insert(WindowEventFilter::MouseUp);
            match button {
                Left => {
                    events_vec.insert(WindowEventFilter::LeftMouseUp);
                    window_state.mouse_state.left_down = false;
                },
                Right => {
                    events_vec.insert(WindowEventFilter::RightMouseUp);
                    window_state.mouse_state.right_down = false;
                },
                Middle => {
                    events_vec.insert(WindowEventFilter::MiddleMouseUp);
                    window_state.mouse_state.middle_down = false;
                },
                _ => { }
            }
        },
        WindowEvent::MouseWheel { .. } => {
            events_vec.insert(WindowEventFilter::Scroll);
        },
        WindowEvent::KeyboardInput {
            input: KeyboardInput { state: ElementState::Pressed, virtual_keycode: Some(_), .. }, ..
        } => {
            events_vec.insert(WindowEventFilter::VirtualKeyDown);
        },
        WindowEvent::ReceivedCharacter(c) => {
            if !c.is_control() {
                events_vec.insert(WindowEventFilter::TextInput);
            }
        },
        WindowEvent::KeyboardInput {
            input: KeyboardInput { state: ElementState::Released, virtual_keycode: Some(_), .. }, ..
        } => {
            events_vec.insert(WindowEventFilter::VirtualKeyUp);
        },
        WindowEvent::HoveredFile(_) => {
            events_vec.insert(WindowEventFilter::HoveredFile);
        },
        WindowEvent::DroppedFile(_) => {
            events_vec.insert(WindowEventFilter::DroppedFile);
        },
        WindowEvent::HoveredFileCancelled => {
            events_vec.insert(WindowEventFilter::HoveredFileCancelled);
        },
        WindowEvent::CursorMoved { .. } => {
            events_vec.insert(WindowEventFilter::MouseOver);
        },
        WindowEvent::CursorEntered { .. } => {
            events_vec.insert(WindowEventFilter::MouseEnter);
        },
        WindowEvent::CursorLeft { .. } => {
            events_vec.insert(WindowEventFilter::MouseLeave);
        },
        _ => { }
    }
    events_vec
}

fn get_hover_events(input: &HashSet<WindowEventFilter>) -> HashSet<HoverEventFilter> {
    input.iter().filter_map(|window_event| window_event.to_hover_event_filter()).collect()
}

fn get_focus_events(input: &HashSet<HoverEventFilter>) -> HashSet<FocusEventFilter> {
    input.iter().filter_map(|hover_event| hover_event.to_focus_event_filter()).collect()
}

/// Pre-filters any events that are not handled by the framework yet, since it would be wasteful
/// to process them. Modifies the `frame_event_info` so that the
///
/// `awakened_task` is a special field that should be set to true if the `Task`
/// system fired a `WindowEvent::Awakened`.
pub(crate) fn window_should_close(event: &WindowEvent, frame_event_info: &mut FrameEventInfo) -> bool {

    match event {
        WindowEvent::CursorMoved { position, .. } => {
            frame_event_info.should_hittest = true;
            frame_event_info.cur_cursor_pos = LogicalPosition { x: position.x as f32, y: position.y as f32 };
        },
        WindowEvent::Resized(wh) => {
            frame_event_info.new_window_size = Some(LogicalSize { width: wh.width as f32, height: wh.height as f32 });
            frame_event_info.is_resize_event = true;
            frame_event_info.should_redraw_window = true;
        },
        WindowEvent::HiDpiFactorChanged(dpi) => {
            frame_event_info.new_dpi_factor = Some(*dpi as f32);
            frame_event_info.should_redraw_window = true;
        },
        WindowEvent::CloseRequested | WindowEvent::Destroyed => {
            // TODO: Callback the windows onclose method
            // (ex. for implementing a "do you really want to close" dialog)
            return true;
        },
        WindowEvent::KeyboardInput { .. } |
        WindowEvent::ReceivedCharacter(_) |
        WindowEvent::MouseWheel { .. } |
        WindowEvent::MouseInput { .. } |
        WindowEvent::Touch(_) => {
            frame_event_info.should_hittest = true;
        },
        _ => { },
    }

    // TODO: Event::Awakened is never invoked, since that is handled
    // by force_redraw_cache anyways

    false
}

/// Utility function that, given the current keyboard state and a list of
/// keyboard accelerators + callbacks, checks what callback can be invoked
/// and the first matching callback. This leads to very readable
/// (but still type checked) code like this:
///
/// ```no_run,ignore
/// use azul::prelude::{AcceleratorKey::*, VirtualKeyCode::*};
///
/// fn my_callback<T>(app_state: &mut AppState<T>, event: &mut CallbackInfo<T>) -> UpdateScreen {
///     keymap(app_state, event, &[
///         [vec![Ctrl, S], save_document],
///         [vec![Ctrl, N], create_new_document],
///         [vec![Ctrl, O], open_new_file],
///         [vec![Ctrl, Shift, N], create_new_window],
///     ])
/// }
/// ```
pub fn keymap<T>(
    app_state: &mut AppState<T>,
    event: &mut CallbackInfo<T>,
    events: &[(Vec<AcceleratorKey>, fn(&mut AppState<T>, &mut CallbackInfo<T>) -> UpdateScreen)]
) -> UpdateScreen {

    let keyboard_state = app_state.windows[event.window_id].get_keyboard_state().clone();

    events
        .iter()
        .filter(|(keymap_character, _)| {
            keymap_character
                .iter()
                .all(|keymap_char| keymap_char.matches(&keyboard_state))
        })
        .next()
        .and_then(|(_, callback)| (callback)(app_state, event))
}