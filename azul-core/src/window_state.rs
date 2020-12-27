use std::collections::{HashSet, BTreeMap};
use crate::{
    dom::{EventFilter, CallbackData, NotEventFilter, HoverEventFilter, FocusEventFilter, WindowEventFilter},
    callbacks:: {CallbackInfo, CallbackType, HitTestItem, UpdateScreen},
    id_tree::NodeId,
    styled_dom::DomId,
    ui_solver::LayoutResult,
    window::{AcceleratorKey, FullWindowState, CallbacksOfHitTest, CallbackToCall},
};

/// Determine which event / which callback(s) should be called and in which order
///
/// This function also updates / mutates the current window states `focused_node`
/// as well as the `window_state.previous_state`
pub fn determine_callbacks<'a>(
    window_state: &mut FullWindowState,
    hit_test_items: &[HitTestItem],
    layout_result: &'a mut LayoutResult,
) -> CallbacksOfHitTest<'a> {

    use crate::callbacks::DomNodeId;
    use crate::styled_dom::{AzNodeId, ChangedCssProperty};

    let mut nodes_with_callbacks: Vec<CallbackToCall> = Vec::new();

    let current_window_events = get_window_events(window_state);
    let current_hover_events = get_hover_events(&current_window_events);
    let current_focus_events = get_focus_events(&current_hover_events);

    let event_was_mouse_down    = current_window_events.contains(&WindowEventFilter::MouseDown);
    let event_was_mouse_release = current_window_events.contains(&WindowEventFilter::MouseUp);
    let event_was_mouse_leave   = current_window_events.contains(&WindowEventFilter::MouseLeave);

    // Store the current window state so we can set it in this.previous_window_state later on
    window_state.previous_window_state = None;
    let previous_state = Box::new(window_state.clone());

    // TODO: If the current mouse is down, but the event wasn't a click, that means it was a drag

    // Figure out what the hovered NodeIds are
    let new_hit_node_ids: BTreeMap<NodeId, HitTestItem> = {

        let tag_ids_to_node_ids = layout_result.styled_dom.tag_ids_to_node_ids.iter()
        .filter_map(|t| Some((t.tag_id.into_crate_internal(), t.node_id.into_crate_internal()?)))
        .collect::<BTreeMap<_, _>>();

        let mut new_hits: BTreeMap<NodeId, HitTestItem> = hit_test_items.iter().filter_map(|hit_test_item| {
            tag_ids_to_node_ids
            .get(&hit_test_item.tag)
            .map(|node_id| (*node_id, hit_test_item.clone()))
        }).collect();

        if event_was_mouse_leave {
            new_hits = BTreeMap::new();
        }

        new_hits
    };
    let old_hit_node_ids: BTreeMap<NodeId, HitTestItem> = previous_state.hovered_nodes.get(&layout_result.dom_id).cloned().unwrap_or_default();

    // Figure out what the current focused NodeId is
    let old_focus_node = previous_state.focused_node.clone();
    let new_focus_node = if event_was_mouse_down || event_was_mouse_release {

        // Find the first (closest to cursor in hierarchy) item that has a tabindex
        let closest_focus_node = hit_test_items.iter().rev()
        .find_map(|item| {
            layout_result.styled_dom.tag_ids_to_node_ids.iter().find_map(|t| {
                let tab_idx = t.tab_index.into_option()?;
                if t.tag_id.into_crate_internal() == item.tag {
                    Some((t.node_id.into_crate_internal()?, tab_idx))
                } else {
                    None
                }
            })
        });

        // Even if the focused node is None, we still have to update window_state.focused_node!
        closest_focus_node.map(|(node_id, _tab_idx)| DomNodeId { dom: layout_result.dom_id, node: AzNodeId::from_crate_internal(Some(node_id)) })
    } else {
        old_focus_node.clone()
    };

    // Collect all On::MouseEnter nodes (for both hover and focus events)
    let onmouseenter_nodes: BTreeMap<NodeId, HitTestItem> = new_hit_node_ids.iter()
        .filter(|(current_node_id, _)| old_hit_node_ids.get(current_node_id).is_none())
        .map(|(x, y)| (*x, y.clone()))
        .collect();

    // Collect all On::MouseLeave nodes (for both hover and focus events)
    let onmouseleave_nodes = old_hit_node_ids
        .iter()
        .filter(|(prev_node_id, _)| new_hit_node_ids.get(prev_node_id).is_none())
        .map(|(x, y)| (*x, y.clone()))
        .collect::<BTreeMap<NodeId, HitTestItem>>();

    // iterate through all callbacks of all nodes
    for (node_id, node_data) in layout_result.styled_dom.node_data.as_ref().iter().enumerate() {
        let node_id = NodeId::new(node_id);
        for callback in node_data.get_callbacks().iter() {
            // see if the callback matches
            match callback.event {
                EventFilter::Window(wev) => {
                    if current_window_events.contains(&wev) {
                        nodes_with_callbacks.push(CallbackToCall {
                            callback,
                            hit_test_item: new_hit_node_ids.get(&node_id).cloned(),
                            node_id,
                        })
                    }
                },
                EventFilter::Hover(HoverEventFilter::MouseEnter) => {
                    if let Some(hit_test_item) = onmouseenter_nodes.get(&node_id) {
                        nodes_with_callbacks.push(CallbackToCall {
                            callback,
                            hit_test_item: Some(*hit_test_item),
                            node_id,
                        });
                    }
                },
                EventFilter::Hover(HoverEventFilter::MouseLeave) => {
                    if let Some(hit_test_item) = onmouseleave_nodes.get(&node_id) {
                        nodes_with_callbacks.push(CallbackToCall {
                            callback,
                            hit_test_item: Some(*hit_test_item),
                            node_id,
                        });
                    }
                },
                EventFilter::Hover(hev) => {
                    if let Some(hit_test_item) = new_hit_node_ids.get(&node_id) {
                        if current_hover_events.contains(&hev) {
                            nodes_with_callbacks.push(CallbackToCall {
                                callback,
                                hit_test_item: Some(*hit_test_item),
                                node_id,
                            });
                        }
                    }
                },
                EventFilter::Focus(FocusEventFilter::FocusReceived) => {
                    if new_focus_node == Some(DomNodeId { dom: layout_result.dom_id, node: AzNodeId::from_crate_internal(Some(node_id)) }) && old_focus_node != new_focus_node {
                        nodes_with_callbacks.push(CallbackToCall {
                            callback,
                            hit_test_item: None,
                            node_id,
                        });
                    }
                },
                EventFilter::Focus(FocusEventFilter::FocusLost) => {
                    if old_focus_node == Some(DomNodeId { dom: layout_result.dom_id, node: AzNodeId::from_crate_internal(Some(node_id)) }) && old_focus_node != new_focus_node {
                        nodes_with_callbacks.push(CallbackToCall {
                            callback,
                            hit_test_item: None,
                            node_id,
                        });
                    }
                },
                EventFilter::Focus(fev) => {
                    if new_focus_node == Some(DomNodeId { dom: layout_result.dom_id, node: AzNodeId::from_crate_internal(Some(node_id)) }) && current_focus_events.contains(&fev) {
                        nodes_with_callbacks.push(CallbackToCall {
                            callback,
                            hit_test_item: None,
                            node_id,
                        });
                    }
                },
                EventFilter::Not(NotEventFilter::Focus(fev)) => {
                    if Some(DomNodeId { dom: layout_result.dom_id, node: AzNodeId::from_crate_internal(Some(node_id)) }) != new_focus_node && current_focus_events.contains(&fev) {
                        nodes_with_callbacks.push(CallbackToCall {
                            callback,
                            hit_test_item: None,
                            node_id,
                        });
                    }
                },
                EventFilter::Not(NotEventFilter::Hover(hev)) => {
                    if new_hit_node_ids.get(&node_id).is_none() && current_hover_events.contains(&hev) {
                        nodes_with_callbacks.push(CallbackToCall {
                            callback,
                            hit_test_item: None,
                            node_id,
                        });
                    }
                },
            }
        }
    }

    // immediately restyle the DOM to reflect the new :hover, :active and :focus nodes
    // and determine if the DOM needs a redraw or a relayout
    let mut style_changes = BTreeMap::new();
    let mut layout_changes = BTreeMap::new();
    let is_mouse_down = window_state.mouse_state.mouse_down();

    for onmouseenter_node_id in onmouseenter_nodes.keys() {
        // style :hover nodes

        let hover_node = &mut layout_result.styled_dom.styled_nodes.as_container_mut()[*onmouseenter_node_id];
        if hover_node.needs_hover_restyle() {
            let style_props_changed = hover_node.restyle_hover();
            let mut style_style_props = style_props_changed.iter().filter(|prop| !prop.previous_prop.get_type().can_trigger_relayout()).cloned().collect::<Vec<ChangedCssProperty>>();
            let mut style_layout_props = style_props_changed.iter().filter(|prop| prop.previous_prop.get_type().can_trigger_relayout()).cloned().collect::<Vec<ChangedCssProperty>>();

            if !style_style_props.is_empty() {
                style_changes.entry(*onmouseenter_node_id).or_insert_with(|| Vec::new()).append(&mut style_style_props);
            }
            if !style_layout_props.is_empty() {
                layout_changes.entry(*onmouseenter_node_id).or_insert_with(|| Vec::new()).append(&mut style_layout_props);
            }
        }

        if is_mouse_down {
            // style :active nodes
            if hover_node.needs_active_restyle() {
                let style_props_changed = hover_node.restyle_active();
                let mut style_style_props = style_props_changed.iter().filter(|prop| !prop.previous_prop.get_type().can_trigger_relayout()).cloned().collect::<Vec<ChangedCssProperty>>();
                let mut style_layout_props = style_props_changed.iter().filter(|prop| prop.previous_prop.get_type().can_trigger_relayout()).cloned().collect::<Vec<ChangedCssProperty>>();

                if !style_style_props.is_empty() {
                    style_changes.entry(*onmouseenter_node_id).or_insert_with(|| Vec::new()).append(&mut style_style_props);
                }
                if !style_layout_props.is_empty() {
                    layout_changes.entry(*onmouseenter_node_id).or_insert_with(|| Vec::new()).append(&mut style_layout_props);
                }
            }
        }
    }

    for onmouseleave_node_id in onmouseleave_nodes.keys() {
        // style :hover nodes

        let hover_node = &mut layout_result.styled_dom.styled_nodes.as_container_mut()[*onmouseleave_node_id];
        if hover_node.needs_hover_restyle() {
            let style_props_changed = hover_node.restyle_hover();
            let mut style_style_props = style_props_changed.iter().filter(|prop| !prop.previous_prop.get_type().can_trigger_relayout()).cloned().collect::<Vec<ChangedCssProperty>>();
            let mut style_layout_props = style_props_changed.iter().filter(|prop| prop.previous_prop.get_type().can_trigger_relayout()).cloned().collect::<Vec<ChangedCssProperty>>();

            if !style_style_props.is_empty() {
                style_changes.entry(*onmouseleave_node_id).or_insert_with(|| Vec::new()).append(&mut style_style_props);
            }
            if !style_layout_props.is_empty() {
                layout_changes.entry(*onmouseleave_node_id).or_insert_with(|| Vec::new()).append(&mut style_layout_props);
            }
        }

        if is_mouse_down {
            // style :active nodes
            if hover_node.needs_active_restyle() {
                let style_props_changed = hover_node.restyle_active();
                let mut style_style_props = style_props_changed.iter().filter(|prop| !prop.previous_prop.get_type().can_trigger_relayout()).cloned().collect::<Vec<ChangedCssProperty>>();
                let mut style_layout_props = style_props_changed.iter().filter(|prop| prop.previous_prop.get_type().can_trigger_relayout()).cloned().collect::<Vec<ChangedCssProperty>>();

                if !style_style_props.is_empty() {
                    style_changes.entry(*onmouseleave_node_id).or_insert_with(|| Vec::new()).append(&mut style_style_props);
                }
                if !style_layout_props.is_empty() {
                    layout_changes.entry(*onmouseleave_node_id).or_insert_with(|| Vec::new()).append(&mut style_layout_props);
                }
            }
        }
    }

    if old_focus_node != new_focus_node {

        if let Some(DomNodeId { dom, node }) = old_focus_node {
            if dom == layout_result.dom_id {
                let node = node.into_crate_internal().unwrap();
                let old_focus_node = &mut layout_result.styled_dom.styled_nodes.as_container_mut()[node];
                if old_focus_node.needs_focus_restyle() {
                    let style_props_changed = old_focus_node.restyle_focus();
                    let mut style_style_props = style_props_changed.iter().filter(|prop| !prop.previous_prop.get_type().can_trigger_relayout()).cloned().collect::<Vec<ChangedCssProperty>>();
                    let mut style_layout_props = style_props_changed.iter().filter(|prop| prop.previous_prop.get_type().can_trigger_relayout()).cloned().collect::<Vec<ChangedCssProperty>>();

                    if !style_style_props.is_empty() {
                        style_changes.entry(node).or_insert_with(|| Vec::new()).append(&mut style_style_props);
                    }
                    if !style_layout_props.is_empty() {
                        layout_changes.entry(node).or_insert_with(|| Vec::new()).append(&mut style_layout_props);
                    }
                }
            }
        }

        if let Some(DomNodeId { dom, node }) = new_focus_node {
            if dom == layout_result.dom_id {
                let node = node.into_crate_internal().unwrap();
                let new_focus_node = &mut layout_result.styled_dom.styled_nodes.as_container_mut()[node];
                if new_focus_node.needs_focus_restyle() {
                    let style_props_changed = new_focus_node.restyle_focus();
                    let mut style_style_props = style_props_changed.iter().filter(|prop| !prop.previous_prop.get_type().can_trigger_relayout()).cloned().collect::<Vec<ChangedCssProperty>>();
                    let mut style_layout_props = style_props_changed.iter().filter(|prop| prop.previous_prop.get_type().can_trigger_relayout()).cloned().collect::<Vec<ChangedCssProperty>>();

                    if !style_style_props.is_empty() {
                        style_changes.entry(node).or_insert_with(|| Vec::new()).append(&mut style_style_props);
                    }
                    if !style_layout_props.is_empty() {
                        layout_changes.entry(node).or_insert_with(|| Vec::new()).append(&mut style_layout_props);
                    }
                }
            }
        }
    }

    window_state.focused_node = new_focus_node;
    window_state.hovered_nodes.insert(layout_result.dom_id, new_hit_node_ids);
    window_state.previous_window_state = Some(previous_state);

    CallbacksOfHitTest {
        style_changes,
        layout_changes,
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