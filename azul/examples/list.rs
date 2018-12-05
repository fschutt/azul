extern crate azul;

use azul::prelude::*;

struct List {
    items: Vec<&'static str>,
    selected: Option<usize>,
}

const CUSTOM_CSS: &str = ".selected { background-color: black; color: white; }";

impl Layout for List {
    fn layout(&self, _: WindowInfo<Self>) -> Dom<Self> {
        self.items.iter().enumerate().map(|(idx, item)| {
            NodeData {
                node_type: NodeType::Label(item.to_string()),
                classes: if self.selected == Some(idx) { vec!["selected".into()] } else { vec![] },
                force_enable_hit_test: vec![On::MouseDown],
                .. Default::default()
            }
        }).collect::<Dom<Self>>()
        .with_callback(On::MouseDown, Callback(print_which_item_was_selected))
    }
}

fn print_which_item_was_selected(app_state: &mut AppState<List>, event: WindowEvent<List>) -> UpdateScreen {

    let selected = event.get_first_hit_child(event.hit_dom_node, On::MouseDown).and_then(|x| Some(x.0));
    let mut should_redraw = UpdateScreen::DontRedraw;

    app_state.data.modify(|state| {
        if selected != state.selected {
            state.selected = selected;
            should_redraw = UpdateScreen::Redraw;
        }
        println!("selected item: {:?}", state.selected);
    });

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
        ],
        selected: None,
    };

    let app = App::new(data, AppConfig::default());
    let mut style = native();
    style.merge(new_from_str(CUSTOM_CSS).unwrap());
    let window = Window::new(WindowCreateOptions::default(), style).unwrap();
    app.run(window).unwrap();
}
