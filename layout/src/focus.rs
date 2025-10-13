//! Focus target resolution for layout
//!
//! This module handles resolving FocusTarget to actual DOM nodes

use alloc::collections::BTreeMap;

use azul_core::{
    callbacks::{FocusTarget, FocusTargetPath},
    dom::{DomId, DomNodeId, NodeId},
    styled_dom::NodeHierarchyItemId,
};

use crate::window::DomLayoutResult;

/// Warning type for focus resolution errors
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateFocusWarning {
    FocusInvalidDomId(DomId),
    FocusInvalidNodeId(NodeHierarchyItemId),
    CouldNotFindFocusNode(String),
}

/// Resolve a FocusTarget to an actual DomNodeId
pub fn resolve_focus_target(
    focus_target: &FocusTarget,
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    current_focus: Option<DomNodeId>,
) -> Result<Option<DomNodeId>, UpdateFocusWarning> {
    use azul_core::callbacks::FocusTarget::*;

    if layout_results.is_empty() {
        return Ok(None);
    }

    macro_rules! search_for_focusable_node_id {
        (
            $layout_results:expr, $start_dom_id:expr, $start_node_id:expr, $get_next_node_fn:ident
        ) => {{
            let mut start_dom_id = $start_dom_id;
            let mut start_node_id = $start_node_id;

            let min_dom_id = DomId::ROOT_ID;
            let max_dom_id = DomId {
                inner: $layout_results.len() - 1,
            };

            // iterate through all DOMs
            loop {
                let layout_result = $layout_results
                    .get(&start_dom_id)
                    .ok_or(UpdateFocusWarning::FocusInvalidDomId(start_dom_id.clone()))?;

                let node_id_valid = layout_result
                    .styled_dom
                    .node_data
                    .as_container()
                    .get(start_node_id)
                    .is_some();

                if !node_id_valid {
                    return Err(UpdateFocusWarning::FocusInvalidNodeId(
                        NodeHierarchyItemId::from_crate_internal(Some(start_node_id.clone())),
                    ));
                }

                if layout_result.styled_dom.node_data.is_empty() {
                    return Err(UpdateFocusWarning::FocusInvalidDomId(start_dom_id.clone()));
                }

                let max_node_id = NodeId::new(layout_result.styled_dom.node_data.len() - 1);
                let min_node_id = NodeId::ZERO;

                // iterate through nodes in DOM
                loop {
                    let current_node_id = NodeId::new(start_node_id.index().$get_next_node_fn(1))
                        .max(min_node_id)
                        .min(max_node_id);

                    if layout_result.styled_dom.node_data.as_container()[current_node_id]
                        .is_focusable()
                    {
                        return Ok(Some(DomNodeId {
                            dom: start_dom_id,
                            node: NodeHierarchyItemId::from_crate_internal(Some(current_node_id)),
                        }));
                    }

                    if current_node_id == min_node_id && current_node_id < start_node_id {
                        // going in decreasing (previous) direction
                        if start_dom_id == min_dom_id {
                            // root node / root dom encountered
                            return Ok(None);
                        } else {
                            start_dom_id.inner -= 1;
                            let next_layout = $layout_results.get(&start_dom_id).ok_or(
                                UpdateFocusWarning::FocusInvalidDomId(start_dom_id.clone()),
                            )?;
                            start_node_id = NodeId::new(next_layout.styled_dom.node_data.len() - 1);
                            break; // continue outer loop
                        }
                    } else if current_node_id == max_node_id && current_node_id > start_node_id {
                        // going in increasing (next) direction
                        if start_dom_id == max_dom_id {
                            // last dom / last node encountered
                            return Ok(None);
                        } else {
                            start_dom_id.inner += 1;
                            start_node_id = NodeId::ZERO;
                            break; // continue outer loop
                        }
                    } else {
                        start_node_id = current_node_id;
                    }
                }
            }
        }};
    }

    match focus_target {
        Path(FocusTargetPath { dom, css_path }) => {
            let layout_result = layout_results
                .get(dom)
                .ok_or(UpdateFocusWarning::FocusInvalidDomId(dom.clone()))?;

            // TODO: Implement proper CSS path matching
            // For now, return an error since we can't match the path yet
            Err(UpdateFocusWarning::CouldNotFindFocusNode(format!(
                "{:?}",
                css_path
            )))
        }
        Id(dom_node_id) => {
            let layout_result = layout_results.get(&dom_node_id.dom).ok_or(
                UpdateFocusWarning::FocusInvalidDomId(dom_node_id.dom.clone()),
            )?;
            let node_is_valid = dom_node_id
                .node
                .into_crate_internal()
                .map(|o| {
                    layout_result
                        .styled_dom
                        .node_data
                        .as_container()
                        .get(o)
                        .is_some()
                })
                .unwrap_or(false);

            if !node_is_valid {
                Err(UpdateFocusWarning::FocusInvalidNodeId(
                    dom_node_id.node.clone(),
                ))
            } else {
                Ok(Some(dom_node_id.clone()))
            }
        }
        Previous => {
            let last_layout_dom_id = DomId {
                inner: layout_results.len() - 1,
            };

            let (current_focus_dom, current_focus_node_id) = match current_focus {
                Some(s) => match s.node.into_crate_internal() {
                    Some(n) => (s.dom, n),
                    None => {
                        if let Some(layout_result) = layout_results.get(&s.dom) {
                            (
                                s.dom,
                                NodeId::new(layout_result.styled_dom.node_data.len() - 1),
                            )
                        } else {
                            (
                                last_layout_dom_id,
                                NodeId::new(
                                    layout_results
                                        .get(&last_layout_dom_id)
                                        .ok_or(UpdateFocusWarning::FocusInvalidDomId(
                                            last_layout_dom_id,
                                        ))?
                                        .styled_dom
                                        .node_data
                                        .len()
                                        - 1,
                                ),
                            )
                        }
                    }
                },
                None => (
                    last_layout_dom_id,
                    NodeId::new(
                        layout_results
                            .get(&last_layout_dom_id)
                            .ok_or(UpdateFocusWarning::FocusInvalidDomId(last_layout_dom_id))?
                            .styled_dom
                            .node_data
                            .len()
                            - 1,
                    ),
                ),
            };

            search_for_focusable_node_id!(
                layout_results,
                current_focus_dom,
                current_focus_node_id,
                saturating_sub
            );
        }
        Next => {
            let (current_focus_dom, current_focus_node_id) = match current_focus {
                Some(s) => match s.node.into_crate_internal() {
                    Some(n) => (s.dom, n),
                    None => {
                        if layout_results.get(&s.dom).is_some() {
                            (s.dom, NodeId::ZERO)
                        } else {
                            (DomId::ROOT_ID, NodeId::ZERO)
                        }
                    }
                },
                None => (DomId::ROOT_ID, NodeId::ZERO),
            };

            search_for_focusable_node_id!(
                layout_results,
                current_focus_dom,
                current_focus_node_id,
                saturating_add
            );
        }
        First => {
            let (current_focus_dom, current_focus_node_id) = (DomId::ROOT_ID, NodeId::ZERO);
            search_for_focusable_node_id!(
                layout_results,
                current_focus_dom,
                current_focus_node_id,
                saturating_add
            );
        }
        Last => {
            let last_layout_dom_id = DomId {
                inner: layout_results.len() - 1,
            };
            let (current_focus_dom, current_focus_node_id) = (
                last_layout_dom_id,
                NodeId::new(
                    layout_results
                        .get(&last_layout_dom_id)
                        .ok_or(UpdateFocusWarning::FocusInvalidDomId(last_layout_dom_id))?
                        .styled_dom
                        .node_data
                        .len()
                        - 1,
                ),
            );
            search_for_focusable_node_id!(
                layout_results,
                current_focus_dom,
                current_focus_node_id,
                saturating_add
            );
        }
        NoFocus => Ok(None),
    }
}
