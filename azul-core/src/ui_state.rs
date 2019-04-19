use std::{
    fmt,
    collections::BTreeMap,
};
use azul_css::CssProperty;
use {
    FastHashMap,
    dom::{
        Dom, TagId, TabIndex, DomString,
        HoverEventFilter, FocusEventFilter, NotEventFilter,
        WindowEventFilter
    },
    id_tree::NodeId,
    callbacks::{Callback, DefaultCallbackId},
};

pub struct UiState<T> {
    /// The actual DOM, rendered from the .layout() function
    pub dom: Dom<T>,
    /// The style properties that should be overridden for this frame, cloned from the `Css`
    pub dynamic_css_overrides: BTreeMap<NodeId, FastHashMap<DomString, CssProperty>>,
    /// Stores all tags for nodes that need to activate on a `:hover` or `:active` event.
    pub tag_ids_to_hover_active_states: BTreeMap<TagId, (NodeId, HoverGroup)>,

    /// Tags -> Focusable nodes
    pub tab_index_tags: BTreeMap<TagId, (NodeId, TabIndex)>,
    /// Tags -> Draggable nodes
    pub draggable_tags: BTreeMap<TagId, NodeId>,
    /// Tag IDs -> Node IDs
    pub tag_ids_to_node_ids: BTreeMap<TagId, NodeId>,
    /// Reverse of `tag_ids_to_node_ids`.
    pub node_ids_to_tag_ids: BTreeMap<NodeId, TagId>,

    // For hover, focus and not callbacks, there needs to be a tag generated
    // for hit-testing. Since window and desktop callbacks are not attached to
    // any element, they only store the NodeId (where the event came from), but have
    // no tag themselves.
    //
    // There are two maps per event, one for the regular callbacks and one for
    // the default callbacks. This is done for consistency, since otherwise the
    // event filtering logic gets much more complicated than it already is.
    pub hover_callbacks:                BTreeMap<NodeId, BTreeMap<HoverEventFilter, Callback<T>>>,
    pub hover_default_callbacks:        BTreeMap<NodeId, BTreeMap<HoverEventFilter, DefaultCallbackId>>,
    pub focus_callbacks:                BTreeMap<NodeId, BTreeMap<FocusEventFilter, Callback<T>>>,
    pub focus_default_callbacks:        BTreeMap<NodeId, BTreeMap<FocusEventFilter, DefaultCallbackId>>,
    pub not_callbacks:                  BTreeMap<NodeId, BTreeMap<NotEventFilter, Callback<T>>>,
    pub not_default_callbacks:          BTreeMap<NodeId, BTreeMap<NotEventFilter, DefaultCallbackId>>,
    pub window_callbacks:               BTreeMap<NodeId, BTreeMap<WindowEventFilter, Callback<T>>>,
    pub window_default_callbacks:       BTreeMap<NodeId, BTreeMap<WindowEventFilter, DefaultCallbackId>>,
}

impl<T> fmt::Debug for UiState<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
            "UiState {{ \

                dom: {:?}, \
                dynamic_css_overrides: {:?}, \
                tag_ids_to_hover_active_states: {:?}, \
                tab_index_tags: {:?}, \
                draggable_tags: {:?}, \
                tag_ids_to_node_ids: {:?}, \
                node_ids_to_tag_ids: {:?}, \
                hover_callbacks: {:?}, \
                hover_default_callbacks: {:?}, \
                focus_callbacks: {:?}, \
                focus_default_callbacks: {:?}, \
                not_callbacks: {:?}, \
                not_default_callbacks: {:?}, \
                window_callbacks: {:?}, \
                window_default_callbacks: {:?}, \
            }}",

            self.dom,
            self.dynamic_css_overrides,
            self.tag_ids_to_hover_active_states,
            self.tab_index_tags,
            self.draggable_tags,
            self.tag_ids_to_node_ids,
            self.node_ids_to_tag_ids,
            self.hover_callbacks,
            self.hover_default_callbacks,
            self.focus_callbacks,
            self.focus_default_callbacks,
            self.not_callbacks,
            self.not_default_callbacks,
            self.window_callbacks,
            self.window_default_callbacks,
        )
    }
}

/// In order to support :hover, the element must have a TagId, otherwise it
/// will be disregarded in the hit-testing. A hover group
#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct HoverGroup {
    /// Whether any property in the hover group will trigger a re-layout.
    /// This is important for creating
    pub affects_layout: bool,
    /// Whether this path ends with `:active` or with `:hover`
    pub active_or_hover: ActiveHover,
}

/// Sets whether an element needs to be selected for `:active` or for `:hover`
#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum ActiveHover {
    Active,
    Hover,
}