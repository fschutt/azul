use std::{
    fmt,
    collections::BTreeMap,
};
use {
    window::{WindowInfo, ReadOnlyWindow, WindowId},
    traits::Layout,
    dom::{NODE_ID, CALLBACK_ID, Callback, Dom, On},
    app_state::AppState,
};

pub struct UiState<T: Layout> {
    pub dom: Dom<T>,
    pub callback_list: BTreeMap<u64, Callback<T>>,
    pub node_ids_to_callbacks_list: BTreeMap<u64, BTreeMap<On, u64>>,
}

impl<T: Layout> fmt::Debug for UiState<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
            "UiState {{ \
                \tdom: {:?}, \
                \tcallback_list: {:?}, \
                \tnode_ids_to_callbacks_list: {:?} \
            }}",
        self.dom,
        self.callback_list,
        self.node_ids_to_callbacks_list)
    }
}

impl<T: Layout> UiState<T> {
    #[allow(unused_imports, unused_variables)]
    pub(crate) fn from_app_state(app_state: &AppState<T>, window_id: WindowId, read_only_window: ReadOnlyWindow) -> Self
    {
        use dom::{Dom, On, NodeType};
        use std::sync::atomic::Ordering;

        let window_info = WindowInfo {
            window_id,
            window: read_only_window,
            texts: &app_state.resources.text_cache,
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

        NODE_ID.swap(0, Ordering::SeqCst);
        CALLBACK_ID.swap(0, Ordering::SeqCst);

        let mut callback_list = BTreeMap::<u64, Callback<T>>::new();
        let mut node_ids_to_callbacks_list = BTreeMap::<u64, BTreeMap<On, u64>>::new();
        dom.collect_callbacks(&mut callback_list, &mut node_ids_to_callbacks_list);

        UiState {
            dom: dom,
            callback_list: callback_list,
            node_ids_to_callbacks_list: node_ids_to_callbacks_list,
        }
    }
}

// Empty test, for some reason codecov doesn't detect any files (and therefore
// doesn't report codecov % correctly) except if they have at least one test in
// the file. This is an empty test, which should be updated later on
#[test]
fn __codecov_test_ui_state_file() {

}