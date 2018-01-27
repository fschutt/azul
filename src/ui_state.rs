use kuchiki::NodeRef;
use traits::LayoutScreen;
use dom::{WrCallbackList};
use std::collections::BTreeMap;
use webrender::api::ItemTag;
use dom::On;

pub struct UiState<T: LayoutScreen> {
	pub document_root: NodeRef,
	pub callback_list: WrCallbackList<T>,
	pub node_ids_to_callbacks_list: BTreeMap<ItemTag, BTreeMap<On, u64>>,
}