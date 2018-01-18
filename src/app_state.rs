use traits::LayoutScreen;
use input::InputEvent;

/// Faster implementation of a HashMap
pub type FastHashMap<T, U> = ::std::collections::HashMap<T, U, ::std::hash::BuildHasherDefault<::twox_hash::XxHash>>;

/// Wrapper for your application data. In order to be layout-able,
/// you need to satisfy the `LayoutScreen` trait (how the application
/// should be laid out)
pub struct AppState<T: LayoutScreen> {
	pub function_callbacks: FastHashMap<String, FnCallback<T>>,
	pub data: T,
}

/// Callback
pub enum FnCallback<T>
where T: LayoutScreen,
{
	/// One-off function (for ex. exporting a file)
	///
	/// This is best for actions that can run in the background
	/// and you don't need to get updates. It uses a background
	/// thread and therefore the data needs to be sendable.
	FnOnceNonBlocking(fn(&mut AppState<T>) -> ()),
	/// Same as the `FnOnceNonBlocking`, but it blocks the current
	/// thread and does not require the type to be `Send`.
	FnOnceBlocking(fn(&mut AppState<T>) -> ()),
}

impl<T> AppState<T> where T: LayoutScreen {

	pub fn new(initial_data: T) -> Self {
		Self {
			function_callbacks: FastHashMap::default(),
			data: initial_data,
		}
	}

	/// Convenience method to insert a new function callback
	pub fn register_function_callback(&mut self, id: String, callback: FnCallback<T>) {
		self.function_callbacks.insert(id, callback);
	}

	/// Convenience method to delete a new function callback
	pub fn delete_function_callback(&mut self, id: &str) {
		self.function_callbacks.remove(id);
	}

	#[doc(hidden)]
	pub(crate) fn update(&mut self, input: &[InputEvent])
		where T: LayoutScreen
	{

	}
}