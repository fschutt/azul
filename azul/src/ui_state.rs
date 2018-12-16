use std::{
    fmt,
    collections::BTreeMap,
};
use azul_css::CssProperty;
use {
    FastHashMap,
    window::{WindowInfo, WindowId},
    traits::Layout,
    dom::{Callback, Dom, On, TabIndex},
    app_state::AppState,
    id_tree::NodeId,
    dom::TagId,
    default_callbacks::DefaultCallbackId,
};

pub struct UiState<T: Layout> {
    pub dom: Dom<T>,
    pub tag_ids_to_callbacks: BTreeMap<TagId, BTreeMap<On, Callback<T>>>,
    pub tag_ids_to_default_callbacks: BTreeMap<TagId, BTreeMap<On, DefaultCallbackId>>,
    pub node_ids_to_tag_ids: BTreeMap<NodeId, TagId>,
    pub tag_ids_to_node_ids: BTreeMap<TagId, NodeId>,
    pub tab_index_tags: BTreeMap<TagId, (NodeId, TabIndex)>,
    pub draggable_tags: BTreeMap<TagId, NodeId>,
    /// The style properties that should be overridden for this frame, cloned from the `Css`
    pub dynamic_style_overrides: BTreeMap<NodeId, FastHashMap<String, CssProperty>>,
}

impl<T: Layout> fmt::Debug for UiState<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
            "UiState {{ \
                \tdom: {:?}, \
                \ttag_ids_to_callbacks: {:?}, \
                \ttag_ids_to_default_callbacks: {:?}, \
                \ttab_index_tags: {:?}, \
                \tdraggable_tags: {:?}, \
                \tnode_ids_to_tag_ids: {:?} \
                \ttag_ids_to_node_ids: {:?} \
            }}",
            self.dom,
            self.tag_ids_to_callbacks,
            self.tag_ids_to_default_callbacks,
            self.tab_index_tags,
            self.draggable_tags,
            self.node_ids_to_tag_ids,
            self.tag_ids_to_node_ids
        )
    }
}

impl<T: Layout> UiState<T> {
    #[allow(unused_imports, unused_variables)]
    pub(crate) fn from_app_state(app_state: &mut AppState<T>, window_id: WindowId) -> Self
    {
        use dom::{Dom, On, NodeType};
        use std::sync::atomic::Ordering;

        let window_info = WindowInfo {
            window: &mut app_state.windows[window_id.id],
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

        Self::from_dom(dom)
    }

    /// Creates the UiState from a Dom, useful for IFrame-based layout
    pub(crate) fn from_dom(dom: Dom<T>) -> Self {
        dom.into_ui_state()
    }
}

// Empty test, for some reason codecov doesn't detect any files (and therefore
// doesn't report codecov % correctly) except if they have at least one test in
// the file. This is an empty test, which should be updated later on
#[test]
fn __codecov_test_ui_state_file() {

}
