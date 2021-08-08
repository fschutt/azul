#![windows_subsystem = "windows"]

use azul::prelude::*;
use azul::prelude::String as AzString;

const CSS: &str = "
#svg-container {
    width: 100%;
    height: 100%;
}";

const SVG: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/assets/svg/AJ_Digital_Camera.svg"));
const SVG_STRING: AzString = AzString::from_const_str(SVG);

#[derive(Debug)]
struct MyAppData {
    svg: ImageRef,
}

extern "C" fn layout(data: &mut RefAny, _: &mut LayoutCallbackInfo) -> StyledDom {
    let rendered_svg = match data.downcast_ref::<MyAppData>() {
        Some(s) => s.svg.clone(),
        None => return StyledDom::default(),
    };

    Dom::body()
    .with_child(Dom::image(rendered_svg))
    .style(Css::from_string(CSS.into()))
}

fn main() {

    let mut start = std::time::Instant::now();

    // parse the SVG
    let svg = match Svg::from_string(SVG_STRING.clone(), SvgParseOptions::default()) {
        ResultSvgSvgParseError::Ok(o) => o,
        ResultSvgSvgParseError::Err(e) => { return; },
    };

    let end = std::time::Instant::now();
    let parse_time = end - start;
    start = end;

    // render the SVG
    let rendered_svg = match svg.render(SvgRenderOptions::default()) {
        OptionRawImage::Some(s) => s,
        OptionRawImage::None => { return; },
    };

    let end = std::time::Instant::now();
    let render_time = end - start;
    start = end;

    // ---- convert the rendered image to a webrender-compatible format
    let image_ref = match ImageRef::raw_image(rendered_svg) {
        OptionImageRef::Some(s) => s,
        OptionImageRef::None => { return; },
    };

    let end = std::time::Instant::now();
    let recode_time = end - start;
    start = end;

    MsgBox::info(format!(
        "Ok - Svg file rendered!\r\n\r\nparsing took: {:?}\r\nrendering took: {:?}\r\nencoding took: {:?}\r\n",
        parse_time, render_time, recode_time).into()
    );

    let data = RefAny::new(MyAppData { svg: image_ref });
    let app = App::new(data, AppConfig::new(LayoutSolver::Default));
    app.run(WindowCreateOptions::new(layout));
}