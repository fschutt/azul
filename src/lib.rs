#[macro_use]
extern crate markup5ever;
extern crate webrender;
extern crate kuchiki;
extern crate cassowary;
extern crate twox_hash;
extern crate glium;
extern crate gleam;
extern crate euclid;
extern crate simplecss;

/// Styling & CSS parsing
pub mod css;
/// The layout traits for creating a layout-able application
pub mod traits;
/// Window handling
pub mod window;
/// State handling for user interfaces
pub mod ui_state;
/// Wrapper for the application data & application state
pub mod app_state;
/// DOM / HTML node handling
pub mod dom;
/// Input handling (mostly glium)
mod input;
/// UI Description & display list handling (webrender)
mod ui_description;
/// Constraint handling
mod constraints;
/// Converts the UI description (the styled HTML nodes)
/// to an actual display list (+ layout)
mod display_list;
mod css_parser;

use css::Css;
use app_state::AppState;
use traits::LayoutScreen;
use input::hit_test_ui;
use ui_state::UiState;
use ui_description::UiDescription;

use std::sync::{Arc, Mutex};
use std::collections::BTreeMap;
use window::{Window, WindowCreateOptions, WindowCreateError, WindowId};

/// Faster implementation of a HashMap
type FastHashMap<T, U> = ::std::collections::HashMap<T, U, ::std::hash::BuildHasherDefault<::twox_hash::XxHash>>;

/// Graphical application that maintains some kind of application state
pub struct App<T: LayoutScreen> {
	/// The graphical windows, indexed by ID
	windows: BTreeMap<WindowId, Window>,
	/// The global application state
	pub app_state: Arc<Mutex<AppState<T>>>,
}

impl<T: LayoutScreen> App<T> {

	/// Create a new, empty application (note: doesn't create a window!)
	pub fn new(initial_data: T) -> Self {
		Self {
			windows: BTreeMap::new(),
			app_state: Arc::new(Mutex::new(AppState::new(initial_data))),
		}
	}

	/// Spawn a new window on the screen
	pub fn create_window(&mut self, options: WindowCreateOptions) -> Result<WindowId, WindowCreateError> {
		let window = Window::new(options)?;
		if self.windows.len() == 0 {
			self.windows.insert(WindowId::new(0), window);
			Ok(WindowId::new(0))
		} else {
			let highest_id = *self.windows.iter().next_back().unwrap().0;
			let new_id = highest_id.id.saturating_add(1);
			self.windows.insert(WindowId::new(new_id), window);
			Ok(WindowId::new(new_id))
		}
	}

	/// Start the rendering loop for the currently open windows
	pub fn start_render_loop(&mut self)
	{
		use constraints::CssConstraint;

		let mut ui_description_cache = vec![UiDescription::default(); self.windows.len()];
		// let mut display_list_cache = vec![Vec::<CssConstraint>::new(); self.windows.len()];
		let mut ui_state = app_state_to_ui_state(&self.app_state.lock().unwrap(), None);

		'render_loop: loop {

			use glium::glutin::WindowEvent;
			use glium::glutin::Event;

			// TODO: Use threads on a per-window basis.
			// Currently, events in one window will block all others
			for (window_id, window) in self.windows.iter_mut() {

				let mut should_redraw_window = false;

				{
					let mut app_state = self.app_state.lock().unwrap();
					let api = &window.internal.api;
					let document = window.internal.document_id;
					let pipeline = window.internal.pipeline_id;

					window.events_loop.poll_events(|event| {
						match event {
							Event::WindowEvent {
								window_id,
								event
							} => {
								match event {
									WindowEvent::CursorMoved {
										device_id,
										position,
										modifiers,
									} => {
										println!("cursor moved in window: {:?}", position);
										use webrender::api::WorldPoint;
										let _ = device_id;
										let _ = modifiers;
										let point = WorldPoint::new(position.0 as f32, position.1 as f32);
										let hit_test_results = hit_test_ui(api, document, Some(pipeline), point);

										for item in hit_test_results.items {
											if let Some(fptr) = app_state.get_associated_event(&item.tag) {
												(fptr)(&mut app_state)
											};
										}

										// end of mouse handling
										should_redraw_window = true;
									},
									_ => { },
								}
							},
							_ => { },
						}
					});

					let css = app_state.data.get_css(*window_id);
					if css.dirty {
						// Re-styles (NOT re-layouts!) the UI. Possibly very performance-heavy.
						ui_description_cache[window_id.id] = ui_state_to_ui_description::<T>(&ui_state, css);
					}
				}

				// Re-layouts the UI.
				if should_redraw_window {
					render(window, window_id, &ui_description_cache[window_id.id]);
				}
			}

			::std::thread::sleep(::std::time::Duration::from_millis(16));
		}
	}

	/// Forwarding function for `AppState.add_event_listener()`
	pub fn add_event_listener<S: Into<String>>(&mut self, id: S, callback_type: S, callback: fn(&mut AppState<T>) -> ()) {
		self.app_state.lock().unwrap().add_event_listener(id, callback_type, callback);
	}

	/// Forwarding function for `AppState.add_event_listener()`
	pub fn remove_event_listener(&mut self, id: &str, callback_type: &str) {
		self.app_state.lock().unwrap().remove_event_listener(id, callback_type);
	}
}

fn render(window: &mut Window, _window_id: &WindowId, ui_description: &UiDescription) {

	// todo: convert the UIDescription into the webrender display list

	use webrender::api::*;
	use display_list::DisplayList;

	let display_list = DisplayList::new_from_ui_description(ui_description);
	let builder = display_list.into_display_list_builder(
		window.internal.pipeline_id,
		window.internal.layout_size,
		&mut window.solver.solver);

	let resources = ResourceUpdates::new();

	let mut txn = Transaction::new();
	txn.set_display_list(
	    window.internal.epoch,
	    None,
	    window.internal.layout_size,
	    builder.finalize(),
	    true,
	);

	txn.update_resources(resources);
	txn.set_root_pipeline(window.internal.pipeline_id);
	txn.generate_frame();

	window.internal.api.send_transaction(window.internal.document_id, txn);
	window.renderer.as_mut().unwrap().update();
	window.renderer.as_mut().unwrap().render(window.internal.framebuffer_size).unwrap();
	window.display.swap_buffers().unwrap();
}

fn app_state_to_ui_state<T>(app_state: &AppState<T>, old_ui_state: Option<&UiState>)
-> UiState where T: LayoutScreen
{
	UiState {
		document_root: app_state.data.update_dom(old_ui_state),
	}
}

fn ui_state_to_ui_description<T>(ui_state: &UiState, style: &mut Css)
-> UiDescription
	where T: LayoutScreen
{
	T::style_dom(&ui_state.document_root, style)
}