extern crate azul;

use azul::prelude::*;

#[derive(Default)]
struct DragMeApp {
    width: Option<f32>,
}

type Event<'a> = WindowEvent<'a, DragMeApp>;
type State = AppState<DragMeApp>;

impl Layout for DragMeApp {
    fn layout(&self, _: WindowInfo<DragMeApp>) -> Dom<Self> {

        let mut left = Dom::new(NodeType::Div).with_id("blue");

        // Set the width of the dragger on the red element
        if let Some(w) = self.width {
            left.add_css_override("drag_width", ParsedCssProperty::Width(LayoutWidth::px(w)));
        }

        let right = Dom::new(NodeType::Div).with_id("orange");

        // The dragger is 0px wide, but has an absolutely positioned rectangle
        // inside of it, which can be dragged
        let dragger = Dom::new(NodeType::Div).with_id("dragger").with_child(
            Dom::new(NodeType::Div).with_id("dragger_handle")
            .with_callback(On::MouseOver, Callback(drag)));

        Dom::new(NodeType::Div).with_id("container")
            .with_child(left)
            .with_child(dragger)
            .with_child(right)
    }
}

fn drag(app: &mut State, event: Event) -> UpdateScreen {
    let mouse_state = app.windows[event.window].state.get_mouse_state();
    if mouse_state.left_down {
        let cursor_pos = mouse_state.cursor_pos.unwrap_or(LogicalPosition::new(0.0, 0.0));
        app.data.modify(|state| state.width = Some(cursor_pos.x as f32));
        UpdateScreen::Redraw
    } else {
        UpdateScreen::DontRedraw
    }
}
fn main() {
    macro_rules! CSS_PATH { () => (concat!(env!("CARGO_MANIFEST_DIR"), "/examples/dragger.css")) }

    #[cfg(debug_assertions)]
    let css = Css::hot_reload(CSS_PATH!()).unwrap();
    #[cfg(not(debug_assertions))]
    let css = Css::new_from_str(include_str!(CSS_PATH!())).unwrap();

    let app = App::new(DragMeApp::default(), AppConfig::default());
    app.run(Window::new(WindowCreateOptions::default(), css).unwrap()).unwrap();
}