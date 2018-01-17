#[macro_use]
extern crate azul;

use azul::traits::LayoutScreen;
use azul::ui_state::UiState;
use azul::NodeRef;
use azul::window::WindowCreateOptions;
use azul::app_state::AppState;
use azul::css::Css;

const TEST_CSS: &str = include_str!("test_content.css");

pub struct MyAppData {
	// your app data here
	pub my_data: u32,
}

impl LayoutScreen for MyAppData {
	fn update_dom(&self, _old_ui_state: Option<&UiState>) -> NodeRef {
		use std::cell::RefCell;
		use azul::{Attributes, NodeData, ElementData};
		// use azul::{QualName, Prefix, Namespace, LocalName};
		use std::collections::HashMap;
		use azul::QualName;

		// serialize your data to a UI however you want
		// ex: NodeRef::of_type("div").with_id("myitem")
		// to register identifier where you can later hook up event handlers
		//
		// this can also be loaded from an HTML file later on, the
		// memory model is the same
		let mut attributes = HashMap::new();
		attributes.insert(QualName::new(None, ns!(html), local_name!("id")), String::from("main"));
		NodeRef::new(NodeData::Element(ElementData {
			name: QualName::new(None, ns!(html), local_name!("div")),
			attributes: RefCell::new(Attributes {
				map: attributes,
			}),
			template_contents: None,
		}))
	}
}

fn my_button_click_handler(app_state: &mut AppState<MyAppData>) {
	println!("my button was clicked! data is now: {:?}", app_state.data.my_data);
	app_state.data.my_data += 1;
}

fn main() {
	let css = Css::new_from_string(TEST_CSS).unwrap();
	for rule in css.rules {
		println!("rule - {:?}", rule);
	}

	let my_app_data = MyAppData { my_data: 0 };
	let mut app = azul::App::new(my_app_data);
	let window_id = app.create_window(WindowCreateOptions::default()).unwrap();
	// app.register_event_handler("div#myitem:onclick", my_button_click_handler);
	app.start_render_loop();
}
