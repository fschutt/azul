use std::{
    fmt,
    collections::{BTreeMap, BTreeSet},
};
use azul_style::StyleProperty;
use {
    FastHashMap,
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
    pub tag_ids_to_noop_callbacks: BTreeMap<TagId, BTreeSet<On>>,
    pub node_ids_to_tag_ids: BTreeMap<NodeId, TagId>,
    pub tag_ids_to_node_ids: BTreeMap<TagId, NodeId>,
    /// The style properties that should be overridden for this frame, cloned from the `AppStyle`
    pub dynamic_style_overrides: BTreeMap<NodeId, FastHashMap<String, StyleProperty>>,
}

impl<T: Layout> fmt::Debug for UiState<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
            "UiState {{ \
                \tdom: {:?}, \
                \ttag_ids_to_callbacks: {:?}, \
                \ttag_ids_to_default_callbacks: {:?}, \
                \tag_ids_to_noop_callbacks: {:?}, \
                \tnode_ids_to_tag_ids: {:?} \
                \ttag_ids_to_node_ids: {:?} \
            }}",
        self.dom,
        self.tag_ids_to_callbacks,
        self.tag_ids_to_default_callbacks,
        self.tag_ids_to_noop_callbacks,
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

        // NOTE: Originally it was allowed to create a DOM with
        // multiple root elements using `add_sibling()` and `with_sibling()`.
        //
        // However, it was decided to remove these functions (in commit #586933),
        // as they aren't practical (you can achieve the same thing with one
        // wrapper div and multiple add_child() calls) and they create problems
        // when layouting elements since add_sibling() essentially modifies the
        // space that the parent can distribute, which in code, simply looks weird
        // and led to bugs.
        //
        // It is assumed that the DOM returned by the user has exactly one root node
        // with no further siblings and that the root node is the Node with the ID 0.

        debug_assert!(dom.arena.borrow().node_layout[NodeId::new(0)].next_sibling.is_none());

        let mut tag_ids_to_callbacks = BTreeMap::new();
        let mut tag_ids_to_default_callbacks = BTreeMap::new();
        let mut tag_ids_to_noop_callbacks = BTreeMap::new();
        let mut node_ids_to_tag_ids = BTreeMap::new();
        let mut tag_ids_to_node_ids = BTreeMap::new();
        let mut dynamic_style_overrides = BTreeMap::new();

        dom.collect_callbacks(
            &mut tag_ids_to_callbacks,
            &mut tag_ids_to_default_callbacks,
            &mut tag_ids_to_noop_callbacks,
            &mut node_ids_to_tag_ids,
            &mut tag_ids_to_node_ids,
            &mut dynamic_style_overrides);

        Self {
            dom,
            tag_ids_to_callbacks,
            tag_ids_to_default_callbacks,
            tag_ids_to_noop_callbacks,
            node_ids_to_tag_ids,
            tag_ids_to_node_ids,
            dynamic_style_overrides,
        }
    }
}

// Empty test, for some reason codecov doesn't detect any files (and therefore
// doesn't report codecov % correctly) except if they have at least one test in
// the file. This is an empty test, which should be updated later on
#[test]
fn __codecov_test_ui_state_file() {

}
