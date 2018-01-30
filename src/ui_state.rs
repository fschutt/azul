use traits::LayoutScreen;
use window::WindowId;
use std::collections::BTreeMap;
use dom::{NODE_ID, CALLBACK_ID, Callback, Dom, On};
use app_state::AppState;

pub struct UiState<T: LayoutScreen> {
    pub dom: Dom<T>,
    pub callback_list: BTreeMap<u64, Callback<T>>,
    pub node_ids_to_callbacks_list: BTreeMap<u64, BTreeMap<On, u64>>,
}

impl<T: LayoutScreen> UiState<T> {
    pub(crate) fn from_app_state(app_state: &AppState<T>, window_id: WindowId) -> Self
    {
        use dom::{Dom, On};

        let dom: Dom<T> = app_state.data.get_dom(window_id);
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