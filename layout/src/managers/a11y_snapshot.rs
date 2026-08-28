//! Platform-neutral accessibility snapshot.
//!
//! `accesskit` covers Windows (UIA), macOS (`NSAccessibility`) and Unix
//! (AT-SPI). It ships NO UIKit backend and NO Android backend, so the iOS and
//! Android shells cannot reuse [`crate::managers::a11y::A11yManager::update_tree`]
//! — they have to hand UIKit a `UIAccessibilityElement` list and hand Android's
//! `AccessibilityNodeProvider` a virtual-view tree, both built from Azul's own
//! types.
//!
//! [`A11ySnapshot`] is that shared intermediate: a flat, index-addressed list of
//! everything a screen reader can see, with the label / value / role / bounds /
//! supported actions each platform needs, plus the parent-child links UIKit and
//! Android both require.
//!
//! # Why FLAT and index-addressed
//!
//! Android's `AccessibilityNodeProvider` addresses nodes by a plain `int`
//! virtual-view id, and UIKit's container protocol addresses them by
//! `NSInteger` index. A snapshot index is the natural id for both, and it round
//! trips back to `(DomId, NodeId)` through [`A11ySnapshot::element`] with no
//! bit-packing scheme to overflow. (`a11y::decode_a11y_node_id` packs both into
//! a `u64`, which is fine for accesskit's `NodeId` and does NOT fit an Android
//! `int`.)
//!
//! Indices are only valid for the snapshot that produced them. Rebuild the
//! snapshot on every layout and tell the platform the tree changed — which is
//! exactly what both platforms expect after a layout change anyway.
//!
//! # Node membership
//!
//! Exactly [`crate::managers::a11y::is_exposed_to_accessibility`], the same
//! predicate the accesskit tree uses. If these disagreed, a node would be
//! actionable on Linux and invisible on Android for no reason a user could
//! understand.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use std::collections::BTreeMap;

use azul_core::{
    dom::{
        AccessibilityAction, AccessibilityRole, AccessibilityState, AttributeType, DomId,
        DomNodeId, NodeData, NodeId, NodeType,
    },
    geom::{LogicalPosition, LogicalRect, LogicalSize},
};

use crate::{
    managers::{a11y::is_exposed_to_accessibility, scroll_state::ScrollManager},
    window::DomLayoutResult,
};

/// One node as assistive technology sees it.
///
/// `clippy::struct_excessive_bools` is allowed rather than satisfied. The four
/// flags are INDEPENDENT platform a11y states — a node can be focusable and
/// editable and disabled at the same time — and each maps 1:1 onto a flag the
/// bridges hand to `UIAccessibilityTraits` / `AccessibilityNodeInfo`. Folding
/// them into an enum would make illegal what the platforms consider normal, and
/// a bitflags newtype would only move the same four bits behind accessors this
/// struct exists to expose.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct A11yElement {
    /// The DOM node this element stands for.
    pub dom_id: DomId,
    /// The DOM node this element stands for.
    pub node_id: NodeId,
    /// Index of the nearest exposed ancestor in the same snapshot.
    /// `None` means "child of the window root".
    pub parent: Option<usize>,
    /// Indices of the exposed descendants that attach directly to this element.
    pub children: Vec<usize>,
    /// What a screen reader announces. Empty when the node has no name.
    pub label: String,
    /// The editable / current value, for inputs and contenteditable nodes.
    pub value: Option<String>,
    /// Element purpose.
    pub role: AccessibilityRole,
    /// Absolute bounds in LOGICAL units, padding/border inset, clipped to the
    /// window. Logical because the two consumers disagree about pixels: `UIKit`
    /// works in points (== logical), Android in physical pixels. Each bridge
    /// scales; neither has to un-scale.
    pub bounds: LogicalRect,
    /// Actions this element accepts. Drives the `UIKit` traits / Android action
    /// list, and is what [`A11ySnapshot::supports`] checks before a platform
    /// action is forwarded to the engine.
    pub actions: Vec<AccessibilityAction>,
    /// Can take keyboard focus.
    pub focusable: bool,
    /// Currently has keyboard focus.
    pub focused: bool,
    /// Text can be edited in place.
    pub editable: bool,
    /// `Some(true)` / `Some(false)` for checkable elements, `None` otherwise.
    pub checked: Option<bool>,
    /// Element is disabled and must not be activated.
    pub disabled: bool,
}

impl A11yElement {
    /// Does this element accept `action`?
    #[must_use]
    pub fn supports(&self, action: &AccessibilityAction) -> bool {
        self.actions.contains(action)
    }
}

/// A whole window's worth of [`A11yElement`]s, index-addressed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct A11ySnapshot {
    /// Window title, i.e. the root container's label.
    pub title: String,
    /// Every exposed node, in document order.
    pub elements: Vec<A11yElement>,
    /// Indices of elements with no exposed ancestor (children of the root).
    pub roots: Vec<usize>,
    /// Window size in logical units, so a bridge can convert without also
    /// needing the window state.
    pub window_size: LogicalSize,
}

impl A11ySnapshot {
    /// Build a snapshot from the current layout.
    ///
    /// Mirrors the three passes of `A11yManager::update_tree` (create, link,
    /// attach) so the two surfaces expose the same nodes with the same parents.
    #[must_use]
    // `too_many_lines` and `cognitive_complexity` are allowed, not satisfied.
    // This is a single linear projection: walk every DOM node once and emit one
    // flat element per node. The length is the number of a11y attributes a node
    // has, not nested control flow, and the obvious split — a helper per
    // attribute group — would take eight parameters each and read worse than the
    // straight line it replaced.
    //
    // Worth revisiting when this file gains test coverage: it currently has
    // none, which is the real reason not to refactor it blind today. Tracked on
    // the deferred-items task.
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    pub fn build(
        layout_results: &BTreeMap<DomId, DomLayoutResult>,
        scroll_manager: &ScrollManager,
        gpu_state: &crate::managers::gpu_state::GpuStateManager,
        focused_node: Option<DomNodeId>,
        title: &str,
        window_size: LogicalSize,
    ) -> Self {
        let mut elements: Vec<A11yElement> = Vec::new();
        let mut roots: Vec<usize> = Vec::new();
        // (dom.inner, node index) -> snapshot index
        let mut index_of: BTreeMap<(usize, usize), usize> = BTreeMap::new();

        let focused = focused_node.and_then(|f| f.node.into_crate_internal().map(|n| (f.dom, n)));

        // ── Pass 1: create an element per exposed node ─────────────────
        for (dom_id, layout_result) in layout_results {
            let styled_dom = &layout_result.styled_dom;
            let node_data_slice = styled_dom.node_data.as_ref();
            let node_hierarchy = styled_dom.node_hierarchy.as_ref();

            for (dom_idx, node_data) in node_data_slice.iter().enumerate() {
                if !is_exposed_to_accessibility(node_data) {
                    continue;
                }
                let node_id = NodeId::new(dom_idx);

                let bounds = element_bounds(
                    layout_result,
                    *dom_id,
                    node_id,
                    window_size,
                    scroll_manager,
                    gpu_state,
                );

                // Child text is the element's name when the node has no
                // interactive children — same rule the accesskit builder uses,
                // so a group is announced by its text instead of swallowing its
                // children.
                let (child_text, has_non_text_children) =
                    collect_child_text(node_data_slice, node_hierarchy, dom_idx);

                let editable = node_data.is_contenteditable()
                    || matches!(node_data.node_type, NodeType::TextArea | NodeType::Input);

                let mut label = String::new();
                let mut value: Option<String> = None;

                if let Some(info) = node_data.get_accessibility_info() {
                    if let Some(name) = info.accessibility_name.as_option() {
                        label = name.as_str().to_string();
                    }
                    if let Some(v) = info.accessibility_value.as_option() {
                        value = Some(v.as_str().to_string());
                    }
                }
                if let Some(l) = node_data.get_accessible_label() {
                    label = l.to_string();
                }
                if let Some(v) = node_data.get_accessible_value() {
                    value = Some(v.to_string());
                }
                if let NodeType::Text(text) = &node_data.node_type {
                    label = text.as_str().to_string();
                }
                if !child_text.is_empty() {
                    if editable {
                        value = Some(child_text);
                    } else if !has_non_text_children && label.is_empty() {
                        label = child_text;
                    }
                }

                // A DECLARED role wins; `Unknown` is the default that means
                // "not specified" and must not erase the element's own kind.
                // Same rule the accesskit tree applies — see
                // `crate::managers::a11y::accessibility_role_is_specified`.
                let role = match node_data.get_accessibility_info() {
                    Some(info)
                        if crate::managers::a11y::accessibility_role_is_specified(&info.role) =>
                    {
                        info.role
                    }
                    _ => node_type_to_role(&node_data.node_type),
                };

                let mut checked = None;
                let mut disabled = false;
                if let Some(info) = node_data.get_accessibility_info() {
                    for state in info.states.as_ref() {
                        match state {
                            AccessibilityState::CheckedTrue => checked = Some(true),
                            AccessibilityState::CheckedFalse => checked = Some(false),
                            AccessibilityState::Unavailable => disabled = true,
                            _ => {}
                        }
                    }
                }
                for attr in node_data.attributes().as_ref() {
                    match attr {
                        AttributeType::CheckedTrue => checked = Some(true),
                        AttributeType::CheckedFalse => checked = Some(false),
                        AttributeType::Disabled => disabled = true,
                        _ => {}
                    }
                }

                let actions = supported_actions(node_data, scroll_manager, *dom_id, node_id);

                index_of.insert((dom_id.inner, dom_idx), elements.len());
                elements.push(A11yElement {
                    dom_id: *dom_id,
                    node_id,
                    parent: None,
                    children: Vec::new(),
                    label,
                    value,
                    role,
                    bounds,
                    actions,
                    focusable: node_data.is_focusable(),
                    focused: focused == Some((*dom_id, node_id)),
                    editable,
                    checked,
                    disabled,
                });
            }
        }

        // ── Pass 2: link each element to its nearest exposed ancestor ──
        //
        // The DOM parent is often NOT exposed (a wrapper div stripped by the
        // predicate), so walk up until an exposed ancestor is found — otherwise
        // whole subtrees would detach and a screen reader would never reach
        // them. Bounded, because a corrupt hierarchy must not hang the UI
        // thread that is asking for the tree.
        for (dom_id, layout_result) in layout_results {
            let styled_dom = &layout_result.styled_dom;
            let node_hierarchy = styled_dom.node_hierarchy.as_ref();

            for dom_idx in 0..styled_dom.node_data.as_ref().len() {
                let Some(&self_idx) = index_of.get(&(dom_id.inner, dom_idx)) else {
                    continue;
                };

                let mut current = node_hierarchy[dom_idx].parent_id();
                let mut parent_idx = None;
                let mut guard = 0usize;
                while let Some(parent_node_id) = current {
                    guard += 1;
                    if guard > 10_000 {
                        break;
                    }
                    let p = parent_node_id.index();
                    if let Some(&idx) = index_of.get(&(dom_id.inner, p)) {
                        parent_idx = Some(idx);
                        break;
                    }
                    if p >= node_hierarchy.len() {
                        break;
                    }
                    current = node_hierarchy[p].parent_id();
                }

                match parent_idx {
                    Some(p) => {
                        elements[self_idx].parent = Some(p);
                        elements[p].children.push(self_idx);
                    }
                    None => roots.push(self_idx),
                }
            }
        }

        Self {
            title: title.to_string(),
            elements,
            roots,
            window_size,
        }
    }

    /// Element at `index`, or `None` when the index is stale (the snapshot was
    /// rebuilt under the platform's feet). Returning `None` rather than
    /// panicking matters: `index` comes straight from `UIKit` / Android, i.e.
    /// from another process's idea of what the tree looks like.
    #[must_use]
    pub fn element(&self, index: usize) -> Option<&A11yElement> {
        self.elements.get(index)
    }

    /// Snapshot index for a DOM node, if it is exposed.
    #[must_use]
    pub fn index_of(&self, dom_id: DomId, node_id: NodeId) -> Option<usize> {
        self.elements
            .iter()
            .position(|e| e.dom_id == dom_id && e.node_id == node_id)
    }

    /// Index of the currently focused element, if any is exposed.
    #[must_use]
    pub fn focused(&self) -> Option<usize> {
        self.elements.iter().position(|e| e.focused)
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.elements.len()
    }
}

/// Absolute, padding/border-inset, viewport-clipped bounds in logical units.
///
/// Same geometry `A11yManager::build_node` computes for accesskit, minus the
/// `HiDPI` multiply — see [`A11yElement::bounds`] for why the scale stays with
/// the platform. A node with no layout (display:none, never laid out) gets a
/// zero rect, which every platform reads as "nothing to highlight".
fn element_bounds(
    layout_result: &DomLayoutResult,
    dom_id: DomId,
    node_id: NodeId,
    window_size: LogicalSize,
    scroll_manager: &ScrollManager,
    gpu_state: &crate::managers::gpu_state::GpuStateManager,
) -> LogicalRect {
    let zero = LogicalRect {
        origin: LogicalPosition { x: 0.0, y: 0.0 },
        size: LogicalSize {
            width: 0.0,
            height: 0.0,
        },
    };

    let Some(layout_idx) = layout_result
        .layout_tree
        .dom_to_layout
        .get(&node_id)
        .and_then(|indices| indices.first())
        .copied()
    else {
        return zero;
    };
    let Some(hot) = layout_result.layout_tree.get(layout_idx) else {
        return zero;
    };
    let (Some(pos), Some(size)) = (
        layout_result
            .calculated_positions
            .get(layout_idx.index())
            .copied(),
        hot.used_size,
    ) else {
        return zero;
    };

    let bp = hot.box_props.unpack();
    let pad_left = bp.padding.left + bp.border.left;
    let pad_top = bp.padding.top + bp.border.top;
    let pad_right = bp.padding.right + bp.border.right;
    let pad_bottom = bp.padding.bottom + bp.border.bottom;

    // Static padded rect in local space, THEN mapped to on-screen space
    // (ancestor scroll offsets + reference-frame transforms — what the
    // renderer actually painted), THEN clamped to the viewport. A screen
    // reader must be told where the element IS, not where it was laid out
    // before the user scrolled or an animation moved it.
    let local = LogicalRect {
        origin: LogicalPosition {
            x: pos.x + pad_left,
            y: pos.y + pad_top,
        },
        size: LogicalSize {
            width: (size.width - pad_left - pad_right).max(0.0),
            height: (size.height - pad_top - pad_bottom).max(0.0),
        },
    };
    let on_screen = crate::headless::node_rect_to_screen(
        layout_result,
        dom_id,
        layout_idx.index(),
        local,
        &|d, n| scroll_manager.get_current_offset(d, n),
        &|d, n| {
            gpu_state
                .caches
                .get(&d)
                .and_then(|c| c.css_current_transform_values.get(&n))
                .copied()
        },
    );

    let clamp = |v: f32, max: f32| v.max(0.0).min(max);
    let x0 = clamp(on_screen.origin.x, window_size.width);
    let y0 = clamp(on_screen.origin.y, window_size.height);
    let x1 = clamp(on_screen.origin.x + on_screen.size.width, window_size.width);
    let y1 = clamp(
        on_screen.origin.y + on_screen.size.height,
        window_size.height,
    );

    if x1 <= x0 || y1 <= y0 {
        return zero;
    }
    LogicalRect {
        origin: LogicalPosition { x: x0, y: y0 },
        size: LogicalSize {
            width: x1 - x0,
            height: y1 - y0,
        },
    }
}

/// Concatenate direct text children, and report whether any non-text child
/// exists (in which case the text must NOT become a group label — the screen
/// reader has to be able to navigate into the interactive children).
fn collect_child_text(
    node_data: &[NodeData],
    node_hierarchy: &[azul_core::styled_dom::NodeHierarchyItem],
    dom_idx: usize,
) -> (String, bool) {
    let mut text = String::new();
    let mut has_non_text = false;

    let mut child = node_hierarchy[dom_idx].first_child_id(NodeId::new(dom_idx));
    let mut guard = 0usize;
    while let Some(child_id) = child {
        guard += 1;
        if guard > 10_000 {
            break;
        }
        if let Some(child_data) = node_data.get(child_id.index()) {
            if let NodeType::Text(t) = &child_data.node_type {
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str(t.as_str());
            } else {
                has_non_text = true;
            }
        }
        if child_id.index() >= node_hierarchy.len() {
            break;
        }
        child = node_hierarchy[child_id.index()].next_sibling_id();
    }

    (text, has_non_text)
}

/// Which actions this node accepts.
///
/// Mirrors what the accesskit builder advertises: `ScrollIntoView` on
/// everything, `Focus` on anything focusable or editable, `Default` on anything
/// with activation behaviour, the scroll family on an actually-scrollable
/// container, the text family on an editable one, plus whatever the app
/// declared in `AccessibilityInfo::supported_actions`.
///
/// This list is not decoration: [`A11yElement::supports`] is what the iOS and
/// Android bridges check before forwarding a platform action, so an element
/// cannot be sent an action the engine would silently drop.
fn supported_actions(
    node_data: &NodeData,
    scroll_manager: &ScrollManager,
    dom_id: DomId,
    node_id: NodeId,
) -> Vec<AccessibilityAction> {
    let mut actions = Vec::new();

    actions.push(AccessibilityAction::ScrollIntoView);

    if node_data.is_focusable() || node_data.is_contenteditable() {
        actions.push(AccessibilityAction::Focus);
        actions.push(AccessibilityAction::Blur);
    }
    if node_data.has_activation_behavior() {
        actions.push(AccessibilityAction::Default);
    }
    if node_data.is_contenteditable()
        || matches!(node_data.node_type, NodeType::TextArea | NodeType::Input)
    {
        actions.push(AccessibilityAction::SetValue(azul_css::AzString::from("")));
        actions.push(AccessibilityAction::ReplaceSelectedText(
            azul_css::AzString::from(""),
        ));
    }

    if let Some((_offset, max_x, max_y)) = scroll_manager.a11y_scroll_info(dom_id, node_id) {
        if max_y > 0.0 {
            actions.push(AccessibilityAction::ScrollUp);
            actions.push(AccessibilityAction::ScrollDown);
        }
        if max_x > 0.0 {
            actions.push(AccessibilityAction::ScrollLeft);
            actions.push(AccessibilityAction::ScrollRight);
        }
        actions.push(AccessibilityAction::SetScrollOffset(LogicalPosition {
            x: 0.0,
            y: 0.0,
        }));
    }

    if let Some(info) = node_data.get_accessibility_info() {
        for declared in info.supported_actions.as_ref() {
            if !actions.contains(declared) {
                actions.push(declared.clone());
            }
        }
    }

    actions
}

/// Fallback role for a node with no explicit `AccessibilityInfo`.
///
/// Deliberately small: only the roles the mobile platforms announce
/// differently. Everything else is `Grouping`, which both `UIKit` and Android
/// read as "a plain container" — the honest answer for a bare `<div>`, and
/// better than claiming a role the node does not have.
const fn node_type_to_role(node_type: &NodeType) -> AccessibilityRole {
    match node_type {
        NodeType::Button => AccessibilityRole::PushButton,
        NodeType::A => AccessibilityRole::Link,
        NodeType::Text(_)
        | NodeType::P
        | NodeType::Span
        | NodeType::H1
        | NodeType::H2
        | NodeType::H3
        | NodeType::H4
        | NodeType::H5
        | NodeType::H6 => AccessibilityRole::StaticText,
        NodeType::Input | NodeType::TextArea => AccessibilityRole::Text,
        NodeType::Image(_) => AccessibilityRole::Graphic,
        NodeType::Ul | NodeType::Ol => AccessibilityRole::List,
        NodeType::Li => AccessibilityRole::ListItem,
        NodeType::Table => AccessibilityRole::Table,
        NodeType::Td | NodeType::Th => AccessibilityRole::Cell,
        _ => AccessibilityRole::Grouping,
    }
}
