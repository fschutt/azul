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

use css::Css;
use app_state::AppState;
use traits::LayoutScreen;
use input::{Hotkeys, hit_test_ui};
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
		let mut ui_description_cache = vec![UiDescription::default(); self.windows.len()];
		let mut ui_state = app_state_to_ui_state(&self.app_state.lock().unwrap(), None);
		let mut hotkeys = Hotkeys::none();

		'render_loop: loop {
			let should_break = update(&mut self.app_state.lock().unwrap(), &mut ui_state, &mut hotkeys);

			if should_break == ShouldBreakUpdateLoop::Break {
				break 'render_loop;
			}

			for (window_id, window) in self.windows.iter_mut() {

				let mut app_state_lock = self.app_state.lock().unwrap();
				let css = app_state_lock.data.get_css(*window_id);
				if css.dirty {
					// Re-styles (NOT re-layouts!) the UI. Possibly very performance-heavy.
					ui_description_cache[window_id.id] = ui_state_to_ui_description::<T>(&ui_state, css);
				}

				println!("ui_description: {:#?}", ui_description_cache[window_id.id]);

				// Re-layouts the UI.
				render(window, window_id, &ui_description_cache[window_id.id]);
			}

			::std::thread::sleep(::std::time::Duration::from_millis(16));
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
		     hotkeys: &mut Hotkeys)
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

	*hotkeys = new_hotkeys;
	*ui_state = new_ui_state;
	ShouldBreakUpdateLoop::Break
}

fn render(window: &mut Window, _window_id: &WindowId, ui_description: &UiDescription) {
	use webrender::api::*;

	// todo: convert the UIDescription into the webrender display list

	let mut builder = DisplayListBuilder::new(window.internal.pipeline_id, window.internal.layout_size);
	let mut resources = ResourceUpdates::new();

	// Create a 200x200 stacking context with an animated transform property.
	let bounds = LayoutRect::new(
	    LayoutPoint::new(0.0, 0.0),
	    LayoutSize::new(200.0, 200.0),
	);

	let complex_clip = ComplexClipRegion {
	    rect: bounds,
	    radii: BorderRadius::uniform(50.0),
	    mode: ClipMode::Clip,
	};

	let info = LayoutPrimitiveInfo {
	    local_clip: LocalClip::RoundedRect(bounds, complex_clip),
	    .. LayoutPrimitiveInfo::new(bounds)
	};

	let opacity = 34.0;
	let opacity_key = PropertyBindingKey::new(43); // arbitrary magic number
	let property_key = PropertyBindingKey::new(42); // arbitrary magic number

	let filters = vec![
	    FilterOp::Opacity(PropertyBinding::Binding(opacity_key), opacity),
	];

	builder.push_stacking_context(
	    &info,
	    ScrollPolicy::Scrollable,
	    Some(PropertyBinding::Binding(property_key)),
	    TransformStyle::Flat,
	    None,
	    MixBlendMode::Normal,
	    filters,
	);

	// Fill it with a white rect
	builder.push_rect(&info, ColorF::new(0.0, 1.0, 0.0, 1.0));
	builder.pop_stacking_context();

	// create new frame
	window.internal.api.set_display_list(
	    window.internal.document_id,
	    window.internal.epoch,
	    None,
	    window.internal.layout_size,
	    builder.finalize(),
	    true,
	    resources,
	);

	window.internal.api.generate_frame(window.internal.document_id, None);
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

fn app_state_to_hotkeys<T>(app_state: &AppState<T>)
-> Hotkeys where T: LayoutScreen
{
	Hotkeys::none()
}

fn ui_state_to_ui_description<T>(ui_state: &UiState, style: &mut Css)
-> UiDescription
	where T: LayoutScreen
{
	T::style_dom(&ui_state.document_root, style)
}
