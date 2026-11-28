//! The two captures of the pointer an event pass applies to the events it determined - in ONE place, called by
//! every shell and by the e2e runner.
//!
//! A button that went down on a node must come up on it, wherever the pointer is when it does. Two mechanisms
//! make that so, and they are one idea at two strengths:
//!
//! * EXPLICIT capture (`CallbackInfo::capture_pointer`, W3C `setPointerCapture`): while a node holds the pointer,
//!   the capturing seat's moves and its release go to THAT node, whatever the hit test found, and the release ends
//!   the capture ([`apply_pointer_capture`]).
//! * IMPLICIT capture (the press target): the node a button was pressed on gets that button's release even when the
//!   pointer released somewhere else ([`HoverManager::apply_press_target_capture`]).
//!
//! The order is the point. The explicit capture goes FIRST: it moves the release to the node that holds the pointer,
//! and the implicit capture then asks "is the pressed node already on the release's path?" of the release as it
//! stands - which, for the usual widget (it captures from its own press handler: the pressed node IS the capturing
//! node), it is. The other way round the implicit capture saw the release where the pointer was (off the node),
//! appended a second release for the pressed node, and the explicit capture then retargeted BOTH to the capturing
//! node: every widget that captured the pointer was told of the release of a drag that ended off its box TWICE
//! (found by the e2e runner's pointer-capture scenario, which accepted it; the shells did the same).
//!
//! Before this module the shells and the runner each had a copy of the steps, the runner's without the explicit
//! capture at all (it stored the capture and never applied it), and a copy of the "is the pressed node on the
//! release's path" walk. One body now.

use alloc::vec::Vec;

use azul_core::{dom::DomNodeId, events::SyntheticEvent, styled_dom::NodeHierarchyItem};

use crate::{
    managers::hover::{apply_pointer_capture, HoverManager, PointerCapture},
    window::LayoutWindow,
};

/// Applies the explicit capture `capture` (ending it when its seat released), then the press-target capture, to the
/// `events` a pass determined. `in_release_path(press, release)` answers whether `press` is `release` or one of its
/// DOM ancestors (the node is then already on the release's propagation path).
pub fn apply_input_captures(
    hover: &mut HoverManager,
    capture: &mut Option<PointerCapture>,
    events: &mut Vec<SyntheticEvent>,
    in_release_path: &dyn Fn(DomNodeId, DomNodeId) -> bool,
) {
    if let Some(held) = *capture {
        if apply_pointer_capture(events, held) {
            *capture = None;
        }
    }
    hover.apply_press_target_capture(events, in_release_path);
}

impl LayoutWindow {
    /// [`apply_input_captures`] for this window: the pass calls it once, right after the determination of its events and
    /// before anything is dispatched. The walk up the DOM is this window's own (`layout_results`).
    pub fn apply_input_captures(&mut self, events: &mut Vec<SyntheticEvent>) {
        let Self {
            layout_results,
            hover_manager,
            pointer_capture,
            ..
        } = self;
        let in_release_path = |press: DomNodeId, release: DomNodeId| -> bool {
            if press.dom != release.dom {
                return false;
            }
            let (Some(press_node), Some(mut current)) = (
                press.node.into_crate_internal(),
                release.node.into_crate_internal(),
            ) else {
                return false;
            };
            let Some(layout_result) = layout_results.get(&release.dom) else {
                return false;
            };
            let hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
            loop {
                if current == press_node {
                    return true;
                }
                match hierarchy.get(current).and_then(NodeHierarchyItem::parent_id) {
                    Some(parent) => current = parent,
                    None => return false,
                }
            }
        };
        apply_input_captures(hover_manager, pointer_capture, events, &in_release_path);
    }
}

#[cfg(test)]
#[path = "input_capture_test.rs"]
mod input_capture_test;
