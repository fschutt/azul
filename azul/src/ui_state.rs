use std::{
    fmt,
    collections::BTreeMap,
};
use glium::glutin::WindowId as GliumWindowId;
use azul_css::CssProperty;
use {
    FastHashMap,
    app::RuntimeError,
    traits::Layout,
    dom::{
        Dom, TagId, TabIndex, DomString,
        HoverEventFilter, FocusEventFilter, NotEventFilter,
        WindowEventFilter
    },
    app::AppState,
    id_tree::NodeId,
    style::HoverGroup,
    callbacks::{Callback, LayoutInfo, DefaultCallbackId},
};

pub struct UiState<T: Layout> {
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

impl<T: Layout> fmt::Debug for UiState<T> {
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

impl<T: Layout> UiState<T> {

    #[allow(unused_imports, unused_variables)]
    pub(crate) fn from_app_state(app_state: &mut AppState<T>, window_id: &GliumWindowId)
    -> Result<Self, RuntimeError<T>>
    {
        use dom::{Dom, On, NodeType};
        use std::sync::atomic::Ordering;
        use app::RuntimeError::*;

        let mut fake_window = app_state.windows.get_mut(window_id).ok_or(WindowIndexError)?;
        let window_info = LayoutInfo {
            window: &mut fake_window,
            resources: &app_state.resources,
        };

        // Only shortly lock the data to get the dom out
        let dom: Dom<T> = {
            let dom_lock = app_state.data.lock().unwrap();
            #[cfg(test)]{
                Dom::<T>::new(NodeType::Div)
            }

            #[cfg(not(test))]{
                dom_lock.layout(window_info)
            }
        };

        Ok(dom.into_ui_state())
    }

    pub(crate) fn create_tags_for_hover_nodes(&mut self, hover_nodes: &BTreeMap<NodeId, HoverGroup>) {
        use dom::new_tag_id;
        for (hover_node_id, hover_group) in hover_nodes {
            let hover_tag = match self.node_ids_to_tag_ids.get(hover_node_id) {
                Some(tag_id) => *tag_id,
                None => new_tag_id(),
            };

            self.node_ids_to_tag_ids.insert(*hover_node_id, hover_tag);
            self.tag_ids_to_node_ids.insert(hover_tag, *hover_node_id);
            self.tag_ids_to_hover_active_states.insert(hover_tag, (*hover_node_id, *hover_group));
        }
    }
}
