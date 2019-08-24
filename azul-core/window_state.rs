use std::collections::{HashSet, BTreeMap};
use crate::{
    dom::{EventFilter, NotEventFilter, HoverEventFilter, FocusEventFilter, WindowEventFilter},
    callbacks:: {CallbackInfo, Callback, CallbackType, HitTestItem, DefaultCallback, UpdateScreen},
    id_tree::NodeId,
    ui_state::UiState,
    window::{
        AcceleratorKey, FullWindowState, CallbacksOfHitTest, DetermineCallbackResult,
    },
};

/// Determine which event / which callback(s) should be called and in which order
///
/// This function also updates / mutates the current window states `focused_node`
/// as well as the `window_state.previous_state`
pub fn determine_callbacks<T>(
    window_state: &mut FullWindowState,
    hit_test_items: &[HitTestItem],
    ui_state: &UiState<T>,
) -> CallbacksOfHitTest<T> {

    use std::collections::BTreeSet;

    let mut needs_hover_redraw = false;
    let mut needs_hover_relayout = false;
    let mut nodes_with_callbacks: BTreeMap<NodeId, DetermineCallbackResult<T>> = BTreeMap::new();

    let current_window_events = get_window_events(window_state);
    let current_hover_events = get_hover_events(&current_window_events);
    let current_focus_events = get_focus_events(&current_hover_events);

    let event_was_mouse_down    = current_window_events.contains(&WindowEventFilter::MouseDown);
    let event_was_mouse_release = current_window_events.contains(&WindowEventFilter::MouseUp);
    let event_was_mouse_enter   = current_window_events.contains(&WindowEventFilter::MouseEnter);
    let event_was_mouse_leave   = current_window_events.contains(&WindowEventFilter::MouseLeave);

    // Store the current window state so we can set it in this.previous_window_state later on
    let mut previous_state = Box::new(window_state.clone());
    previous_state.previous_window_state = None;

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
        window_state.focused_node = closest_focus_node.map(|(node_id, _tab_idx)| (ui_state.dom_id.clone(), node_id));
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
        let default_window_callbacks = BTreeMap::<EventFilter, DefaultCallback<T>>::new();
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
        insert_callbacks!(&current_focused_node.1, None, focus_callbacks, focus_default_callbacks, current_focus_events, Focus);
    }

    // If the last focused node and the current focused node aren't the same,
    // submit a FocusLost for the last node and a FocusReceived for the current one.
    let mut focus_received_lost_events: BTreeMap<NodeId, FocusEventFilter> = BTreeMap::new();
    match (window_state.focused_node.as_ref(), previous_state.focused_node.as_ref()) {
        (Some((cur_dom_id, cur_node_id)), None) => {
            if *cur_dom_id == ui_state.dom_id {
                focus_received_lost_events.insert(*cur_node_id, FocusEventFilter::FocusReceived);
            }
        },
        (None, Some((prev_dom_id, prev_node_id))) => {
            if *prev_dom_id == ui_state.dom_id {
                focus_received_lost_events.insert(*prev_node_id, FocusEventFilter::FocusLost);
            }
        },
        (Some(cur), Some(prev)) => {
            if *cur != *prev {
                let (cur_dom_id, cur_node_id) = cur;
                let (prev_dom_id, prev_node_id) = prev;
                if *cur_dom_id == ui_state.dom_id {
                    focus_received_lost_events.insert(*cur_node_id, FocusEventFilter::FocusReceived);
                }
                if *prev_dom_id == ui_state.dom_id {
                    focus_received_lost_events.insert(*prev_node_id, FocusEventFilter::FocusLost);
                }
            }
        }
        (None, None) => { },
    }

    // Insert FocusReceived / FocusLost
    for (node_id, focus_event) in &focus_received_lost_events {
        let current_focus_leave_events = [focus_event.clone()];
        insert_callbacks!(node_id, None, focus_callbacks, focus_default_callbacks, current_focus_leave_events, Focus);
    }

    let current_dom_id = ui_state.dom_id.clone();

    macro_rules! mouse_enter {
        ($node_id:expr, $hit_test_item:expr, $event_filter:ident) => ({

            let node_is_focused = window_state.focused_node == Some((current_dom_id.clone(), $node_id));

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
        .filter(|(current_node_id, _)| previous_state.hovered_nodes.get(&current_dom_id).and_then(|hn| hn.get(current_node_id)).is_none())
        .map(|(x, y)| (*x, y.clone()))
        .collect();

    let onmouseenter_empty = onmouseenter_nodes.is_empty();

    // Insert Focus(MouseEnter) and Hover(MouseEnter)
    for (node_id, hit_test_item) in onmouseenter_nodes {
        mouse_enter!(node_id, hit_test_item, MouseEnter);
    }

    // Collect all On::MouseLeave nodes (for both hover and focus events)
    let onmouseleave_nodes: BTreeMap<NodeId, HitTestItem> = match previous_state.hovered_nodes.get(&current_dom_id) {
        Some(hn) => {
            hn.iter()
            .filter(|(prev_node_id, _)| new_hit_node_ids.get(prev_node_id).is_none())
            .map(|(x, y)| (*x, y.clone()))
            .collect()
        },
        None => BTreeMap::new(),
    };

    let onmouseleave_empty = onmouseleave_nodes.is_empty();

    // Insert Focus(MouseEnter) and Hover(MouseEnter)
    for (node_id, hit_test_item) in onmouseleave_nodes {
        mouse_enter!(node_id, hit_test_item, MouseLeave);
    }

    // If the mouse is down, but was up previously or vice versa, that means
    // that a :hover or :active state may be invalidated. In that case we need
    // to redraw the screen anyways. Setting relayout to true here in order to
    let event_is_click_or_release = event_was_mouse_down || event_was_mouse_release;
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
                NotEventFilter::Focus(_f) => {
                    // TODO: Same thing for focus
                }
            }
        }
    }

    window_state.hovered_nodes.insert(current_dom_id, new_hit_node_ids);
    window_state.previous_window_state = Some(previous_state);

    CallbacksOfHitTest {
        needs_redraw_anyways: needs_hover_redraw,
        needs_relayout_anyways: needs_hover_relayout,
        nodes_with_callbacks,
    }
}

pub fn get_window_events(window_state: &FullWindowState) -> HashSet<WindowEventFilter> {

    use crate::window::CursorPosition::*;

    let mut events_vec = HashSet::<WindowEventFilter>::new();

    let previous_window_state = match &window_state.previous_window_state {
        Some(s) => s,
        None => return events_vec,
    };

    // mouse move events

    match (previous_window_state.mouse_state.cursor_position, window_state.mouse_state.cursor_position) {
        (InWindow(_), OutOfWindow) |
        (InWindow(_), Uninitialized) => {
            events_vec.insert(WindowEventFilter::MouseLeave);
        },
        (OutOfWindow, InWindow(_)) |
        (Uninitialized, InWindow(_)) => {
            events_vec.insert(WindowEventFilter::MouseEnter);
        },
        (InWindow(a), InWindow(b)) => {
            if a != b {
                events_vec.insert(WindowEventFilter::MouseOver);
            }
        },
        _ => { },
    }

    // click events

    if window_state.mouse_state.mouse_down() && !previous_window_state.mouse_state.mouse_down() {
        events_vec.insert(WindowEventFilter::MouseDown);
    }

    if window_state.mouse_state.left_down && !previous_window_state.mouse_state.left_down {
        events_vec.insert(WindowEventFilter::LeftMouseDown);
    }

    if window_state.mouse_state.right_down && !previous_window_state.mouse_state.right_down {
        events_vec.insert(WindowEventFilter::RightMouseDown);
    }

    if window_state.mouse_state.middle_down && !previous_window_state.mouse_state.middle_down {
        events_vec.insert(WindowEventFilter::MiddleMouseDown);
    }

    if previous_window_state.mouse_state.mouse_down() && !window_state.mouse_state.mouse_down() {
        events_vec.insert(WindowEventFilter::MouseUp);
    }

    if previous_window_state.mouse_state.left_down && !window_state.mouse_state.left_down {
        events_vec.insert(WindowEventFilter::LeftMouseUp);
    }

    if previous_window_state.mouse_state.right_down && !window_state.mouse_state.right_down {
        events_vec.insert(WindowEventFilter::RightMouseUp);
    }

    if previous_window_state.mouse_state.middle_down && !window_state.mouse_state.middle_down {
        events_vec.insert(WindowEventFilter::MiddleMouseUp);
    }

    // scroll events

    let is_scroll_previous =
        previous_window_state.mouse_state.scroll_x.is_some() ||
        previous_window_state.mouse_state.scroll_y.is_some();

    let is_scroll_now =
        window_state.mouse_state.scroll_x.is_some() ||
        window_state.mouse_state.scroll_y.is_some();

    if !is_scroll_previous && is_scroll_now {
        events_vec.insert(WindowEventFilter::ScrollStart);
    }

    if is_scroll_now {
        events_vec.insert(WindowEventFilter::Scroll);
    }

    if is_scroll_previous && !is_scroll_now {
        events_vec.insert(WindowEventFilter::ScrollEnd);
    }

    // keyboard events

    if previous_window_state.keyboard_state.current_virtual_keycode.is_none() && window_state.keyboard_state.current_virtual_keycode.is_some() {
        events_vec.insert(WindowEventFilter::VirtualKeyDown);
    }

    if window_state.keyboard_state.current_char.is_some() {
        events_vec.insert(WindowEventFilter::TextInput);
    }

    if previous_window_state.keyboard_state.current_virtual_keycode.is_some() && window_state.keyboard_state.current_virtual_keycode.is_none() {
        events_vec.insert(WindowEventFilter::VirtualKeyUp);
    }

    // misc events

    if previous_window_state.hovered_file.is_none() && window_state.hovered_file.is_some() {
        events_vec.insert(WindowEventFilter::HoveredFile);
    }

    if previous_window_state.hovered_file.is_some() && window_state.hovered_file.is_none() {
        if window_state.dropped_file.is_some() {
            events_vec.insert(WindowEventFilter::DroppedFile);
        } else {
            events_vec.insert(WindowEventFilter::HoveredFileCancelled);
        }
    }

    events_vec
}

pub fn get_hover_events(input: &HashSet<WindowEventFilter>) -> HashSet<HoverEventFilter> {
    input.iter().filter_map(|window_event| window_event.to_hover_event_filter()).collect()
}

pub fn get_focus_events(input: &HashSet<HoverEventFilter>) -> HashSet<FocusEventFilter> {
    input.iter().filter_map(|hover_event| hover_event.to_focus_event_filter()).collect()
}

/// Utility function that, given the current keyboard state and a list of
/// keyboard accelerators + callbacks, checks what callback can be invoked
/// and the first matching callback. This leads to very readable
/// (but still type checked) code like this:
///
/// ```no_run,ignore
/// use azul::prelude::{AcceleratorKey::*, VirtualKeyCode::*};
///
/// fn my_callback<T>(info: CallbackInfo<T>) -> UpdateScreen {
///     keymap(info, &[
///         [vec![Ctrl, S], save_document],
///         [vec![Ctrl, N], create_new_document],
///         [vec![Ctrl, O], open_new_file],
///         [vec![Ctrl, Shift, N], create_new_window],
///     ])
/// }
/// ```
pub fn keymap<T>(
    info: CallbackInfo<T>,
    events: &[(Vec<AcceleratorKey>, CallbackType<T>)]
) -> UpdateScreen {

    let keyboard_state = info.get_keyboard_state().clone();

    events
        .iter()
        .filter(|(keymap_character, _)| {
            keymap_character
                .iter()
                .all(|keymap_char| keymap_char.matches(&keyboard_state))
        })
        .next()
        .and_then(|(_, callback)| (callback)(info))
}