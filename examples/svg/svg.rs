#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

extern crate azul;

use azul::{
    prelude::*,
    widgets::{button::Button, svg::*},
};

const CSS: &str = "
    #svg-container {
        width: 100%;
        height: 100%;
    }

    .control-btn {
        width: 50px;
        height: 50px;
        position: absolute;
    }

    #btn-zoom-in {
        top: 100px;
        left: 50px;
    }

    #btn-zoom-out {
        top: 100px;
        left: 150px;
    }

    #btn-move-up {
        top: 50px;
        left: 50px;
    }
    #btn-move-right {
        top: 50px;
        left: 50px;
    }
    #btn-move-left {
        top: 50px;
        left: 50px;
    }
    #btn-move-down {
        top: 50px;
        left: 50px;
    }
";

const SVG: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/svg/tiger.svg"));

#[derive(Debug)]
pub struct MyAppData {
    pub map: Map,
}

#[derive(Debug)]
pub struct Map {
    pub cache: SvgCache,
    pub layers: Vec<(SvgLayerId, SvgStyle)>,
    pub zoom: f32,
    pub pan_horz: f32,
    pub pan_vert: f32,
}

impl Layout for MyAppData {
    fn layout(&self, _info: LayoutInfo<Self>) -> Dom<MyAppData> {
        let ptr = StackCheckedPointer::new(self, &self.map).unwrap();
        Dom::gl_texture(draw_svg, ptr).with_callback(On::Scroll, scroll_map_contents).with_id("svg-container")
        .with_child(Button::with_label("Zoom in").dom().with_class("control-btn").with_id("btn-zoom-in"))
        .with_child(Button::with_label("Zoom out").dom().with_class("control-btn").with_id("btn-zoom-in"))
        .with_child(Button::with_label("^").dom().with_class("control-btn").with_id("btn-move-up"))
        .with_child(Button::with_label(">").dom().with_class("control-btn").with_id("btn-move-right"))
        .with_child(Button::with_label("<").dom().with_class("control-btn").with_id("btn-move-left"))
        .with_child(Button::with_label("v").dom().with_class("control-btn").with_id("btn-move-down"))
    }
}

fn draw_svg(info: GlCallbackInfoUnchecked<MyAppData>) -> GlCallbackReturn {
    unsafe {
        info.invoke_callback(|info: GlCallbackInfo<MyAppData, Map>| {
            use azul::widgets::svg::SvgLayerResource::*;

            let map = info.state;
            let physical_size = info.bounds.get_physical_size();
            let width = physical_size.width as usize;
            let height = physical_size.height as usize;

            Svg::with_layers(map.layers.iter().map(|e| Reference(*e)).collect())
                .with_pan(map.pan_horz, map.pan_vert)
                .with_zoom(map.zoom)
                .render_svg(&map.cache, &info.layout_info.window, width, height)
        })
    }
}

fn scroll_map_contents(info: CallbackInfo<MyAppData>) -> UpdateScreen {

    let window_id = info.window_id;
    let mouse_state = info.state.windows.get(&window_id)?.get_mouse_state();
    let keyboard_state = info.state.windows.get(&window_id)?.get_keyboard_state();

    if keyboard_state.shift_down {
        info.state.data.map.pan_horz += mouse_state.scroll_y;
    } else if keyboard_state.ctrl_down {
        if mouse_state.scroll_y.is_sign_positive() {
            info.state.data.map.zoom /= 2.0;
        } else {
            info.state.data.map.zoom *= 2.0;
        }
    } else {
        info.state.data.map.pan_vert += mouse_state.scroll_y;
    }

    Redraw
}

fn main() {

    let mut svg_cache = SvgCache::empty();
    let svg_layers = svg_cache.add_svg(&SVG).unwrap();

    let app_data = MyAppData {
        map: Map {
            cache: svg_cache,
            layers: svg_layers,
            zoom: 1.0,
            pan_horz: 0.0,
            pan_vert: 0.0,
        }
    };

    let css = css::override_native(CSS).unwrap();
    let mut app = App::new(app_data, AppConfig::default()).unwrap();
    let window = app.create_window(WindowCreateOptions::default(), css).unwrap();
    app.run(window).unwrap();
}
