use traits::LayoutScreen;
use input::InputEvent;

type FastHashMap<T, U> = ::std::collections::HashMap<T, U, ::std::hash::BuildHasherDefault<::twox_hash::XxHash>>;

pub struct AppState<T: LayoutScreen> {
	pub function_callbacks: FastHashMap<String, FnCallback<T>>,
	pub data: T,
}

pub enum FnCallback<T>
where T: LayoutScreen
{
	FnOnceNonBlocking(fn(&mut AppState<T>) -> ()),
	FnOnceBlocking(fn(&mut AppState<T>) -> ()),
}

impl<T: LayoutScreen> AppState<T> {

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