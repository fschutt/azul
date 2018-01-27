use traits::LayoutScreen;
use webrender::api::ItemTag;
use FastHashMap;
use std::collections::BTreeMap;

/// This is only accessed from the main thread, so it's safe to use
static mut ITEM_TAG_ID: u64 = 0;

/// Wrapper for your application data. In order to be layout-able,
/// you need to satisfy the `LayoutScreen` trait (how the application
/// should be laid out)
pub struct AppState<T: LayoutScreen> {
	/// `div#myitem` -> [`onclick`: (54, 0), `onhover`: (38, 0)]
	///
	/// means: if `div#myitem` is clicked, execute the action 54 with cursor 0
	pub function_callbacks: FastHashMap<String, FastHashMap<String, ItemTag>>,
	pub callbacks_internal: BTreeMap<ItemTag, fn(&mut AppState<T>) -> ()>,
	pub data: T,
}

impl<T> AppState<T> where T: LayoutScreen {

	pub fn new(initial_data: T) -> Self {
		Self {
			function_callbacks: FastHashMap::default(),
			callbacks_internal: BTreeMap::new(),
			data: initial_data,
		}
	}

	/// Convenience method to insert a new function callback
	pub fn add_event_listener<S: Into<String>>(&mut self, id: S, callback_type: S, callback: fn(&mut AppState<T>) -> ()) {
		let id = id.into();
		let callback_type = callback_type.into();
		let webrender_id = unsafe { (ITEM_TAG_ID, 0) };
		unsafe { ITEM_TAG_ID += 1; }
		let callback_map = self.function_callbacks.entry(id).or_insert_with(|| FastHashMap::default());
		callback_map.insert(callback_type, webrender_id);
		self.callbacks_internal.insert(webrender_id, callback);
	}

	/// Remove all event listener from a div
	pub fn remove_all_event_listeners(&mut self, id: &str) {
		if let Some(callback_map) = self.function_callbacks.get_mut(id) {
			for value in callback_map.values() {
				self.callbacks_internal.remove(value);
			}
		}
		self.function_callbacks.remove(id);
	}

	pub fn remove_event_listener(&mut self, id: &str, callback_type: &str) {
		if let Some(callback_map) = self.function_callbacks.get_mut(id) {
			if let Some(item_id) = callback_map.get(callback_type) {
				self.callbacks_internal.remove(item_id);
			}
			callback_map.remove(callback_type);
		}
	}

	pub(crate) fn get_associated_event(&self, tag: &ItemTag) -> Option<fn(&mut AppState<T>) -> ()> {
		let callback_ref = self.callbacks_internal.get(tag);
		if callback_ref.is_none() {
			return None;
		}
		let callback_ref = (*callback_ref.unwrap()).clone();
		Some(callback_ref)
	}
}