#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

extern crate azul;

use azul::prelude::*;
use std::time::Duration;

const CSS: &str = "
* {
    display: flex;
    width: 100%;
    height: 100%;
    flex-grow: 1;
    box-sizing: border-box;
}

#container {
    flex-direction: row;
}

#blue {
    background-color: #011C41;
    width: var(--drag_width);
}

#orange {
    background-color: #DA2A04;
}

#dragger {
    width: 3px;
    background-color: blue;
    position: relative;
    flex-direction: column;
    justify-content: center;
}

#dragger_handle_container, #dragger_handle {
    width: 50px;
    height: 50px;
}

#dragger_handle_container {
    position: relative;
}

#dragger_handle {
    /* justify-content: center does work if the item is not position:absolute */
    position: absolute;
    background-color: #F5A219;
    left: -25px;
    border-radius: 3px;
    box-shadow: 0px 4px 10px black;
}
";

#[derive(Default)]
struct DragMeApp {
    width: Option<f32>,
    is_dragging: bool,
}

type Event<'a> = CallbackInfo<'a, DragMeApp>;

impl Layout for DragMeApp {
    fn layout(&self, _: LayoutInfo) -> Dom<Self> {

        let mut left = Dom::new(NodeType::Div).with_id("blue");

        // Set the width of the dragger on the red element
        if let Some(w) = self.width {
            left.add_css_override("drag_width", LayoutWidth::px(w));
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
                    .with_callback(On::MouseDown, start_drag)
                    .with_callback(EventFilter::Not(NotEventFilter::Hover(HoverEventFilter::MouseDown)), click_outside_drag)
                )
            );

        Dom::new(NodeType::Div).with_id("container")
            .with_callback(On::MouseOver, update_drag)
            .with_callback(On::MouseUp, stop_drag)
            .with_child(left)
            .with_child(dragger)
            .with_child(right)
    }
}

fn click_outside_drag(_event: Event) -> UpdateScreen {
    println!("click outside drag!");
    DontRedraw
}

fn start_drag(event: Event) -> UpdateScreen {
    event.state.is_dragging = true;
    DontRedraw
}

fn stop_drag(event: Event) -> UpdateScreen {
    event.state.is_dragging = false;
    Redraw
}

fn update_drag(event: Event) -> UpdateScreen {
    let cursor_position = event.get_mouse_state().cursor_position.get_position().unwrap_or(LogicalPosition::new(0.0, 0.0));
    if event.state.is_dragging {
        event.state.width = Some(cursor_position.x as f32);
        Redraw
    } else {
        DontRedraw
    }
}

fn main() {

    let app = App::new(DragMeApp::default(), AppConfig::default()).unwrap();

    #[cfg(debug_assertions)]
    let window = WindowCreateOptions::new_hot_reload(css::hot_reload_override_native(CSS_PATH!(), Duration::from_millis(500)));

    #[cfg(not(debug_assertions))]
    let window = WindowCreateOptions::new(css::override_native(include_str!(CSS_PATH!())).unwrap());

    app.run(window);
}
