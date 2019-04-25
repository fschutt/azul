#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

extern crate azul;

use azul::prelude::*;
use std::time::Duration;

macro_rules! CSS_PATH { () => (concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/slider/slider.css")) }

#[derive(Default)]
struct DragMeApp {
    width: Option<f32>,
    is_dragging: bool,
}

type Event<'a> = CallbackInfo<'a, DragMeApp>;
type State = AppState<DragMeApp>;

impl Layout for DragMeApp {
    fn layout(&self, _: LayoutInfo<DragMeApp>) -> Dom<Self> {

        let mut left = Dom::new(NodeType::Div).with_id("blue");

        // Set the width of the dragger on the red element
        if let Some(w) = self.width {
            left.add_css_override("drag_width", CssProperty::Width(LayoutWidth::px(w)));
        }

        let right = Dom::new(NodeType::Div).with_id("orange");

        // The dragger is 0px wide, but has an absolutely positioned rectangle
        // inside of it, which can be dragged
        let dragger =
            Dom::div()
            .with_id("dragger")
            .with_child(
                Dom::div()
                .with_id("dragger_handle_container")
                .with_child(
                    Dom::div()
                    .with_id("dragger_handle")
                    .with_callback(On::MouseDown, Callback(start_drag))
                    .with_callback(EventFilter::Not(NotEventFilter::Hover(HoverEventFilter::MouseDown)), Callback(click_outside_drag))
                )
            );

        Dom::new(NodeType::Div).with_id("container")
            .with_callback(On::MouseOver, Callback(update_drag))
            .with_callback(On::MouseUp, Callback(stop_drag))
            .with_child(left)
            .with_child(dragger)
            .with_child(right)
    }
}

fn click_outside_drag(_state: &mut State, _event: &mut Event) -> UpdateScreen {
    println!("click outside drag!");
    DontRedraw
}

fn start_drag(state: &mut State, _event: &mut Event) -> UpdateScreen {
    state.data.is_dragging = true;
    DontRedraw
}

fn stop_drag(state: &mut State, _event: &mut Event) -> UpdateScreen {
    state.data.is_dragging = false;
    Redraw
}

fn update_drag(state: &mut State, event: &mut Event) -> UpdateScreen {
    let cursor_pos = state.windows.get(event.window_id)?.state.mouse_state.cursor_pos.get_position().unwrap_or(LogicalPosition::new(0.0, 0.0));
    if state.data.is_dragging {
        state.data.width = Some(cursor_pos.x as f32);
        Redraw
    } else {
        DontRedraw
    }
}

fn main() {

    let mut app = App::new(DragMeApp::default(), AppConfig::default()).unwrap();

    #[cfg(debug_assertions)]
    let window = {
        let hot_reloader = css::hot_reload(CSS_PATH!(), Duration::from_millis(500));
        app.create_hot_reload_window(WindowCreateOptions::default(), hot_reloader).unwrap()
    };

    #[cfg(not(debug_assertions))]
    let window = {
        let css = css::from_str(include_str!(CSS_PATH!())).unwrap();
        app.create_window(WindowCreateOptions::default(), css).unwrap()
    };

    app.run(window).unwrap();
}
