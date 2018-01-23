extern crate azul;

use azul::traits::LayoutScreen;
use azul::ui_state::UiState;
use azul::dom::NodeRef;
use azul::window::{WindowId, WindowCreateOptions};
use azul::app_state::AppState;
use azul::css::Css;
use azul::dom::{NodeType, DomNode};

const TEST_CSS: &str = include_str!("test_content.css");

pub struct MyAppData {
	// Your app data goes here
	pub my_data: u32,
	/// Note: it is deliberate that the trait basically forces you to store
	/// the css yourself. This way you can change the CSS style from any function
	/// (push and pop rules and styles dynamically, for example).
	pub css: Css,
}

impl LayoutScreen for MyAppData {

	fn update_dom(&self, _old_ui_state: Option<&UiState>) -> NodeRef {
		DomNode::new(NodeType::Div).id("main").with_text("Hello World").into()
	}

	fn get_css(&mut self, window_id: WindowId) -> &mut Css {
		// Note: you can match on the window ID if you have different CSS styles
		// for different windows.
		&mut self.css
	}
}

fn my_button_click_handler(app_state: &mut AppState<MyAppData>) {
	println!("my button was clicked! data is now: {:?}", app_state.data.my_data);
	app_state.data.my_data += 1;
}

fn main() {
	let css = Css::new_from_string(TEST_CSS).unwrap();

	let my_app_data = MyAppData {
		my_data: 0,
		css: css,
	};

	let mut app = azul::App::new(my_app_data);
	app.create_window(WindowCreateOptions::default()).unwrap();
	// app.register_event_handler("div#myitem:onclick", my_button_click_handler);
	app.start_render_loop();
}
