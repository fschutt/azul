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
    width: 20px;
    height: 20px;
    position: absolute;
    text-align: center;
    flex-direction: column;
    justify-content: center;
    align-items: center;
    box-sizing: border-box;
}

#btn-zoom-in {
    top: 30px;
    left: 30px;
}

#btn-zoom-out {
    top: 30px;
    left: 70px;
}

#btn-move-up {
    top: 70px;
    left: 50px;
}

#btn-move-right {
    top: 90px;
    left: 70px;
}

#btn-move-left {
    top: 90px;
    left: 30px;
}

#btn-move-down {
    top: 110px;
    left: 50px;
}
";

const SVG: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/svg/tiger.svg"));

#[derive(Debug)]
struct MyAppData {
    svg: Ref<SvgDocument>,
}

#[derive(Debug)]
struct SvgDocument {
    cache: SvgCache,
    layers: Vec<(SvgLayerId, SvgStyle)>,
    zoom: f32,
    pan_horz: f32,
    pan_vert: f32,
}

type CbInfo<'a> = CallbackInfo<'a, MyAppData>;

impl Layout for MyAppData {
    fn layout(&self, _info: LayoutInfo) -> Dom<MyAppData> {
        Dom::gl_texture(draw_svg, self.svg.clone())
        .with_callback(On::Scroll, scroll_map_contents).with_id("svg-container")
            .with_child(render_control_btn("+", "btn-zoom-in",    |info: CbInfo| { info.state.svg.borrow_mut().zoom *= 2.0; Redraw }))
            .with_child(render_control_btn("-", "btn-zoom-out",   |info: CbInfo| { info.state.svg.borrow_mut().zoom /= 2.0; Redraw }))
            .with_child(render_control_btn("^", "btn-move-up",    |info: CbInfo| { info.state.svg.borrow_mut().pan_vert += 100.0; Redraw }))
            .with_child(render_control_btn(">", "btn-move-right", |info: CbInfo| { info.state.svg.borrow_mut().pan_horz += 100.0; Redraw }))
            .with_child(render_control_btn("<", "btn-move-left",  |info: CbInfo| { info.state.svg.borrow_mut().pan_horz -= 100.0; Redraw }))
            .with_child(render_control_btn("v", "btn-move-down",  |info: CbInfo| { info.state.svg.borrow_mut().pan_vert -= 100.0; Redraw }))
    }
}

fn render_control_btn(label: &'static str, css_id: &'static str, callback: fn(CbInfo) -> UpdateScreen) -> Dom<MyAppData> {
    Button::with_label(label).dom().with_class("control-btn").with_id(css_id).with_callback(On::MouseUp, callback)
}

fn draw_svg(info: GlCallbackInfo) -> GlCallbackReturn {

    use azul::widgets::svg::SvgLayerResource::*;

    let state = info.state.downcast::<SvgDocument>()?;
    let map: &SvgDocument = &state.borrow();
    let logical_size = info.bounds.get_logical_size();

    let svg = Svg::with_layers(map.layers.iter().map(|e| Reference(*e)).collect())
        .with_pan(map.pan_horz, map.pan_vert)
        .with_zoom(map.zoom)
        .render_svg(
            &map.cache,
            info.layout_info.gl_context.clone(),
            info.bounds.hidpi_factor,
            logical_size
        );

    Some(svg)
}

fn scroll_map_contents(info: CbInfo) -> UpdateScreen {

    let scroll_y = info.get_mouse_state().scroll_y?;
    let keyboard_state = info.get_keyboard_state().clone();
    let mut svg = info.state.svg.borrow_mut();

    if keyboard_state.shift_down {
        svg.pan_horz += scroll_y;
    } else if keyboard_state.ctrl_down {
        if scroll_y.is_sign_positive() {
            svg.zoom /= 2.0;
        } else {
            svg.zoom *= 2.0;
        }
    } else {
        svg.pan_vert += scroll_y;
    }

    Redraw
}

fn main() {

    let mut svg_cache = SvgCache::empty();
    let svg_layers = svg_cache.add_svg(&SVG).unwrap();

    let app_data = MyAppData {
        svg: Ref::new(SvgDocument {
            cache: svg_cache,
            layers: svg_layers,
            zoom: 1.0,
            pan_horz: 0.0,
            pan_vert: 0.0,
        }),
    };

    let app = App::new(app_data, AppConfig::default()).unwrap();
    let css = css::override_native(include_str!(CSS_PATH!())).unwrap();
    app.run(WindowCreateOptions::new(css));
}
