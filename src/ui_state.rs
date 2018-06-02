use traits::Layout;
use window::WindowId;
use std::collections::BTreeMap;
use dom::{NODE_ID, CALLBACK_ID, Callback, Dom, On};
use app_state::AppState;
use std::fmt;

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
    pub(crate) fn from_app_state(app_state: &AppState<T>, window_id: WindowId) -> Self
    {
        use dom::{Dom, On};

        // Only shortly lock the data to get the dom out
         let dom: Dom<T> = {
            let dom_lock = app_state.data.lock().unwrap();
            dom_lock.layout(window_id)
        };

        unsafe { NODE_ID = 0 };
        unsafe { CALLBACK_ID = 0 };

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