#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

extern crate azul;

use azul::prelude::*;
use std::time::Duration;

struct List {
    items: Vec<&'static str>,
    selected: Option<usize>,
}

impl Layout for List {
    fn layout(&self, _: LayoutInfo<Self>) -> Dom<Self> {

        let child_nodes = self.items.iter().enumerate().map(|(idx, item)| {
            NodeData {
                node_type: NodeType::Label(item.to_string()),
                classes: vec!["item".into()],
                ids: if self.selected == Some(idx) { vec!["selected".into()] } else { vec![] },
                callbacks: vec![(On::MouseDown.into(), Callback(print_which_item_was_selected))],
                .. Default::default()
            }
        }).collect::<Dom<Self>>();

        Dom::new(NodeType::Div).with_id("container").with_child(child_nodes)

    }
}

fn print_which_item_was_selected(app_state: &mut AppState<List>, event: &mut CallbackInfo<List>) -> UpdateScreen {

    let selected = event.target_index_in_parent();

    let mut state = app_state.data.lock().ok()?;
    let should_redraw = if selected != state.selected {
        state.selected = selected;
        Redraw
    } else {
        DontRedraw
    };

    println!("selected item: {:?}", state.selected);
    should_redraw
}

fn main() {

    macro_rules! CSS_PATH { () => (concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/scroll_list.css")) }

    let data = List {
        items: vec![
            "Hello",
            "World",
            "my",
            "name",
            "is",
            "Lorem",
            "Ipsum",
        ],
        selected: None,
    };

    let mut app = App::new(data, AppConfig::default()).unwrap();

    #[cfg(debug_assertions)]
    let window = {
        let hot_reloader = css::hot_reload_override_native(CSS_PATH!(), Duration::from_millis(500));
        app.create_hot_reload_window(WindowCreateOptions::default(), hot_reloader).unwrap()
    };

    #[cfg(not(debug_assertions))]
    let window = {
        let css = css::override_native(include_str!(CSS_PATH!())).unwrap();
        app.create_window(WindowCreateOptions::default(), css).unwrap()
    };

    app.run(window).unwrap();
}
