extern crate webrender;
extern crate kuchiki;
extern crate limn_layout as layout;
extern crate twox_hash;

use kuchiki::NodeRef;

#[cfg(target_os="windows")]
const NATIVE_CSS_WINDOWS: &str = include_str!("../assets/native_windows.css");
#[cfg(target_os="linux")]
const NATIVE_CSS_LINUX: &str = include_str!("../assets/native_linux.css");
#[cfg(target_os="macos")]
const NATIVE_CSS_MACOS: &str = include_str!("../assets/native_macos.css");

pub type FastHashMap<T, U> = ::std::collections::HashMap<T, U, ::std::hash::BuildHasherDefault<::twox_hash::XxHash>>;

pub enum FnCallback<T>
where T: RenderDocument
{
	FnOnceNonBlocking(fn(&mut AppState<T>) -> ()),
	FnOnceBlocking(fn(&mut AppState<T>) -> ()),
}

pub struct Css {

}

#[derive(Debug)]
pub struct CssParseError {

}

impl Css {
	pub fn new() -> Self {
		Self {

		}
	}

	pub fn parse_from_string(css_string: &str) -> Result<Self, CssParseError> {
		Ok(Self {

		})
	}

	/// Returns the native style for the OS
	#[cfg(target_os="windows")]
	pub fn native() -> Self {
		Self::parse_from_string(NATIVE_CSS_WINDOWS).unwrap()
	}

	#[cfg(target_os="linux")]
	pub fn native() -> Self {
		Self::parse_from_string(NATIVE_CSS_LINUX).unwrap()
	}

	#[cfg(target_os="macos")]
	pub fn native() -> Self {
		Self::parse_from_string(NATIVE_CSS_MACOS).unwrap()
	}
}

pub trait RenderDocument {
	fn render_document(&self, old_ui_state: Option<&UiState>) -> NodeRef;
	fn style_document(nodes: &NodeRef, css: &Css) -> UiDescription {
		UiDescription {

		}
	}
}

pub struct AppState<T: RenderDocument> {
	function_callbacks: FastHashMap<String, FnCallback<T>>,
	data: T,
}

impl<T: RenderDocument> AppState<T> {

	fn new(initial_data: T) -> Self {
		Self {
			function_callbacks: FastHashMap::default(),
			data: initial_data,
		}
	}

	fn register_function_callback(&mut self, id: String, callback: FnCallback<T>) {
		self.function_callbacks.insert(id, callback);
	}
}

pub struct UiState {
	document_root: NodeRef
}

pub struct InputEvent {

}

pub struct UiDescription {

}

pub struct Hotkeys {

}

impl Hotkeys {
	/// Empty hotkey list
	pub fn none() -> Self {
		Self {

		}
	}
}

pub fn start_render_loop<T>(app_data: T)
	where T: RenderDocument
{
	let mut app_state = AppState::new(app_data);
	let mut ui_state = app_state_to_ui_state(&app_state, None);
	let mut hotkeys = Hotkeys::none();
	let mut css = Css::new();
	let mut ui_description = ui_state_to_ui_description::<T>(&ui_state, &css);

	'render_loop: loop {
		let should_break = update(&mut app_state, &mut ui_state, &mut hotkeys, &mut ui_description, &mut css);

		if should_break == ShouldBreakUpdateLoop::Break {
			break 'render_loop;
		} else {
			render(&ui_description);
		}
	}
}

#[derive(PartialEq, Eq)]
pub enum ShouldBreakUpdateLoop {
	Break,
	Continue,
}

pub fn update<T>(app_state: &mut AppState<T>,
	             ui_state: &mut UiState,
	             hotkeys: &mut Hotkeys,
			     ui_description: &mut UiDescription,
			     css: &mut Css)
-> ShouldBreakUpdateLoop
	where T: RenderDocument
{
	let frame_events = hit_test_ui(&ui_state, &hotkeys);
	if frame_events.is_empty() {
		return ShouldBreakUpdateLoop::Continue;
	}

	// updating can be parallelized if the components don't overlap each other
	update_application_state(app_state, &frame_events);

	// The next three steps can be done in parallel
	let new_hotkeys = app_state_to_hotkeys(&app_state);
	let new_ui_state = app_state_to_ui_state(&app_state, Some(&ui_state));
	let new_ui_description = ui_state_to_ui_description::<T>(&new_ui_state, css);

	*hotkeys = new_hotkeys;
	*ui_state = new_ui_state;
	*ui_description = new_ui_description;
	ShouldBreakUpdateLoop::Break
}

pub fn render(ui_description: &UiDescription) {

}

fn hit_test_ui(ui_state: &UiState, hotkeys: &Hotkeys) -> Vec<InputEvent> {
	Vec::new()
}

fn update_application_state<T>(app_state: &mut AppState<T>, input: &[InputEvent])
	where T: RenderDocument
{

}

fn app_state_to_ui_state<T>(app_state: &AppState<T>, old_ui_state: Option<&UiState>)
-> UiState where T: RenderDocument
{
	use RenderDocument;
	UiState {
		document_root: app_state.data.render_document(old_ui_state),
	}
}

fn app_state_to_hotkeys<T>(app_state: &AppState<T>)
-> Hotkeys where T: RenderDocument
{
	Hotkeys {

	}
}

fn ui_state_to_ui_description<T>(ui_state: &UiState, style: &Css)
-> UiDescription
	where T: RenderDocument
{
	use RenderDocument;
	T::style_document(&ui_state.document_root, style)
}
