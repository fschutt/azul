#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

extern crate azul;

use azul::{
    prelude::*,
    widgets::{button::Button, svg::*},
};

macro_rules! CSS_PATH { () => (concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/svg/svg.css")) }

const SVG: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/svg/tiger.svg"));

#[derive(Debug)]
struct MyAppData {
    cache: SvgCache,
    layers: Vec<(SvgLayerId, SvgStyle)>,
    zoom: f32,
    pan_horz: f32,
    pan_vert: f32,
}

type CbInfo<'a, 'b> = CallbackInfo<'a, 'b, MyAppData>;

impl Layout for MyAppData {
    fn layout(&self, _info: LayoutInfo<Self>) -> Dom<MyAppData> {
        let ptr = StackCheckedPointer::new(self, self).unwrap();
        Dom::gl_texture(draw_svg, ptr).with_callback(On::Scroll, scroll_map_contents).with_id("svg-container")
        .with_child(render_control_btn("+", "btn-zoom-in",      |info: CbInfo| { info.state.data.zoom *= 2.0; Redraw }))
        .with_child(render_control_btn("-", "btn-zoom-out",     |info: CbInfo| { info.state.data.zoom /= 2.0; Redraw }))
        .with_child(render_control_btn("^", "btn-move-up",      |info: CbInfo| { info.state.data.pan_vert += 100.0; Redraw }))
        .with_child(render_control_btn(">", "btn-move-right",   |info: CbInfo| { info.state.data.pan_horz += 100.0; Redraw }))
        .with_child(render_control_btn("<", "btn-move-left",    |info: CbInfo| { info.state.data.pan_horz -= 100.0; Redraw }))
        .with_child(render_control_btn("v", "btn-move-down",    |info: CbInfo| { info.state.data.pan_vert -= 100.0; Redraw }))
    }
}

fn render_control_btn(label: &'static str, css_id: &'static str, callback: fn(CbInfo) -> UpdateScreen) -> Dom<MyAppData> {
    Button::with_label(label).dom().with_class("control-btn").with_id(css_id).with_callback(On::MouseUp, callback)
}

fn draw_svg(info: GlCallbackInfoUnchecked<MyAppData>) -> GlCallbackReturn {
    unsafe {
        info.invoke_callback(|info: GlCallbackInfo<MyAppData, MyAppData>| {
            use azul::widgets::svg::SvgLayerResource::*;

            let map = info.state;
            let logical_size = info.bounds.get_logical_size();

            Some(Svg::with_layers(map.layers.iter().map(|e| Reference(*e)).collect())
                .with_pan(map.pan_horz, map.pan_vert)
                .with_zoom(map.zoom)
                .render_svg(&map.cache, &info.layout_info.window, logical_size))
        })
    }
}

fn scroll_map_contents(info: CallbackInfo<MyAppData>) -> UpdateScreen {

    let window_id = info.window_id;
    let mouse_state = info.state.windows.get(&window_id)?.get_mouse_state();
    let keyboard_state = info.state.windows.get(&window_id)?.get_keyboard_state();

    if keyboard_state.shift_down {
        info.state.data.pan_horz += mouse_state.scroll_y;
    } else if keyboard_state.ctrl_down {
        if mouse_state.scroll_y.is_sign_positive() {
            info.state.data.zoom /= 2.0;
        } else {
            info.state.data.zoom *= 2.0;
        }
    } else {
        info.state.data.pan_vert += mouse_state.scroll_y;
    }

    Redraw
}

fn main() {

    let mut svg_cache = SvgCache::empty();
    let svg_layers = svg_cache.add_svg(&SVG).unwrap();

    let app_data = MyAppData {
        cache: svg_cache,
        layers: svg_layers,
        zoom: 1.0,
        pan_horz: 0.0,
        pan_vert: 0.0,
    };

    let mut app = App::new(app_data, AppConfig::default()).unwrap();
    let css = css::override_native(include_str!(CSS_PATH!())).unwrap();
    let window = app.create_window(WindowCreateOptions::default(), css).unwrap();
    app.run(window).unwrap();
}
