extern crate azul;

use azul::prelude::*;
use std::time::Duration;

struct List {
    items: Vec<&'static str>,
    selected: Option<usize>,
}

impl Layout for List {
    fn layout(&self, _: WindowInfo<Self>) -> Dom<Self> {

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

fn print_which_item_was_selected(app_state: &mut AppState<List>, event: CallbackInfo<List>) -> UpdateScreen {

    let selected = event.index_path_iter().next();

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
    let data = List {
        items: vec![
            "Hello",
            "World",
            "my",
            "name",
            "is",
            "Lorem",
            "Ipsum",
            "Dolor",
            "Sit",
            "Amet",
        ],
        selected: None,
    };

    macro_rules! CSS_PATH { () => (concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/scroll_list.css")) }

    let app = App::new(data, AppConfig::default());

    #[cfg(debug_assertions)]
    let window = Window::new_hot_reload(WindowCreateOptions::default(), css::hot_reload(CSS_PATH!(), Duration::from_millis(500))).unwrap();

    #[cfg(not(debug_assertions))]
    let window = Window::new(WindowCreateOptions::default(), css::override_native(include_str!(CSS_PATH!())).unwrap()).unwrap();

    app.run(window).unwrap();
}
