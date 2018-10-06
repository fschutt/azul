use std::{
    fmt,
    collections::BTreeMap,
};
use {
    window::{WindowInfo, WindowId},
    traits::Layout,
    dom::{Callback, Dom, On},
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
}

impl<T: Layout> fmt::Debug for UiState<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
            "UiState {{ \
                \tdom: {:?}, \
                \ttag_ids_to_callbacks: {:?}, \
                \ttag_ids_to_default_callbacks: {:?}, \
                \tnode_ids_to_tag_ids: {:?} \
                \ttag_ids_to_node_ids: {:?} \
            }}",
        self.dom,
        self.tag_ids_to_callbacks,
        self.tag_ids_to_default_callbacks,
        self.node_ids_to_tag_ids,
        self.tag_ids_to_node_ids)
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
        use dom::NodeType;

        // DOM tree should have a single root element, necessary for
        // layout constraints having a single root

        // TODO: problematic, since the UiDescription has an Rc into the the DOM
        // and the .add_child empties / drains the original DOM arena !!!
        let dom = {
            let mut parent_dom = Dom::with_capacity(NodeType::Div, dom.len());
            parent_dom.add_child(dom);
            parent_dom
        };

        let mut tag_ids_to_callbacks = BTreeMap::new();
        let mut tag_ids_to_default_callbacks = BTreeMap::new();
        let mut node_ids_to_tag_ids = BTreeMap::new();
        let mut tag_ids_to_node_ids = BTreeMap::new();

        dom.collect_callbacks(
            &mut tag_ids_to_callbacks,
            &mut tag_ids_to_default_callbacks,
            &mut node_ids_to_tag_ids,
            &mut tag_ids_to_node_ids);

        Self {
            dom,
            tag_ids_to_callbacks,
            tag_ids_to_default_callbacks,
            node_ids_to_tag_ids,
            tag_ids_to_node_ids,
        }
    }
}

// Empty test, for some reason codecov doesn't detect any files (and therefore
// doesn't report codecov % correctly) except if they have at least one test in
// the file. This is an empty test, which should be updated later on
#[test]
fn __codecov_test_ui_state_file() {

}