#[macro_use]
extern crate log;
extern crate webrender;
extern crate kuchiki;
extern crate limn_layout as layout;
extern crate twox_hash;
extern crate glium;
extern crate gleam;
extern crate euclid;

pub mod css;
pub mod traits;
pub mod window;
mod ui_state;
mod app_state;
mod input;
mod ui_description;

use css::Css;
use app_state::AppState;
use traits::LayoutScreen;
use input::{Hotkeys, hit_test_ui};
use ui_state::UiState;
use ui_description::UiDescription;

use std::sync::{Arc, Mutex};
use std::collections::BTreeMap;
use window::{Window, CreateWindowOptions, WindowCreateError};

/// Graphical application that maintains some kind of application state
pub struct App<T: LayoutScreen> {
	/// The graphical windows, indexed by ID
	windows: BTreeMap<u32, Window>,
	/// The global application state
	pub app_state: Arc<Mutex<AppState<T>>>,
}

impl<T: LayoutScreen> App<T> {
	/// Create a new, empty application
	pub fn new(initial_data: T) -> Self {
		Self {
			windows: BTreeMap::new(),
			app_state: Arc::new(Mutex::new(AppState::new(initial_data))),
		}
	}

	pub fn create_window(&mut self, options: CreateWindowOptions) -> Result<u32, WindowCreateError> {
		let window = Window::new(options)?;
		if self.windows.len() == 0 {
			self.windows.insert(0, window);
			Ok(0)
		} else {
			let highest_id = *self.windows.iter().next_back().unwrap().0;
			let new_id = highest_id.saturating_add(1);
			self.windows.insert(new_id, window);
			Ok(new_id)
		}
	}

	pub fn start_render_loop(&mut self)
	{
		let mut ui_state = app_state_to_ui_state(&self.app_state.lock().unwrap(), None);
		let mut hotkeys = Hotkeys::none();
		let mut css = Css::new();
		let mut ui_description = ui_state_to_ui_description::<T>(&ui_state, &css);

		'render_loop: loop {
			for window in self.windows.iter_mut() {

				let should_break = update(&mut self.app_state.lock().unwrap(), &mut ui_state, &mut hotkeys, &mut ui_description, &mut css);

				if should_break == ShouldBreakUpdateLoop::Break {
					break 'render_loop;
				} else {
					render(&ui_description);
				}
			}
		}


	}
}


#[derive(PartialEq, Eq)]
enum ShouldBreakUpdateLoop {
	Break,
	Continue,
}



fn update<T>(app_state: &mut AppState<T>,
		     ui_state: &mut UiState,
		     hotkeys: &mut Hotkeys,
		     ui_description: &mut UiDescription,
		     css: &mut Css)
-> ShouldBreakUpdateLoop
	where T: LayoutScreen
{
	let frame_events = hit_test_ui(&ui_state, &hotkeys);
	if frame_events.is_empty() {
		return ShouldBreakUpdateLoop::Continue;
	}

	// updating can be parallelized if the components don't overlap each other
	app_state.update(&frame_events);

	// The next three steps can be done in parallel
	let new_hotkeys = app_state_to_hotkeys(&app_state);
	let new_ui_state = app_state_to_ui_state(&app_state, Some(&ui_state));
	let new_ui_description = ui_state_to_ui_description::<T>(&new_ui_state, css);

	*hotkeys = new_hotkeys;
	*ui_state = new_ui_state;
	*ui_description = new_ui_description;
	ShouldBreakUpdateLoop::Break
}

fn render(ui_description: &UiDescription) {

}

fn app_state_to_ui_state<T>(app_state: &AppState<T>, old_ui_state: Option<&UiState>)
-> UiState where T: LayoutScreen
{
	UiState {
		document_root: app_state.data.update_dom(old_ui_state),
	}
}

fn app_state_to_hotkeys<T>(app_state: &AppState<T>)
-> Hotkeys where T: LayoutScreen
{
	Hotkeys::none()
}

fn ui_state_to_ui_description<T>(ui_state: &UiState, style: &Css)
-> UiDescription
	where T: LayoutScreen
{
	T::style_dom(&ui_state.document_root, style)
}
