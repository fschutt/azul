extern crate azul;

use azul::prelude::*;

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
                force_enable_hit_test: vec![On::MouseDown],
                .. Default::default()
            }
        }).collect::<Dom<Self>>()
        .with_callback(On::MouseDown, Callback(print_which_item_was_selected));

        Dom::new(NodeType::Div).with_id("container").with_child(child_nodes)

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
            "Dolor",
            "Sit",
            "Amet",
        ],
        selected: None,
    };

    let app = App::new(data, AppConfig::default());

    macro_rules! CSS_PATH { () => (concat!(env!("CARGO_MANIFEST_DIR"), "/examples/scroll_list.css")) }

    #[cfg(debug_assertions)]
    let css = Css::hot_reload_override_native(CSS_PATH!()).unwrap();
    #[cfg(not(debug_assertions))]
    let css = Css::new_from_str(include_str!(CSS_PATH!())).unwrap();

    let window = Window::new(WindowCreateOptions::default(), css).unwrap();
    app.run(window).unwrap();
}