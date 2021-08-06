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

extern "C" fn layout(data: &mut RefAny, _: LayoutCallbackInfo) -> StyledDom {
    let rendered_svg = match data.downcast_ref::<MyAppData>() {
        Some(s) => s.svg.clone(),
        None => return StyledDom::default(),
    };

    Dom::body()
    .with_child(Dom::image(rendered_svg))
    .style(Css::from_string(CSS.into()))
}

fn main() {

    let svg = match Svg::from_string(SVG_STRING.clone(), SvgParseOptions::default()) {
        ResultSvgSvgParseError::Ok(o) => o,
        ResultSvgSvgParseError::Err(e) => { return; },
    };

    let rendered_svg = match svg.render(SvgRenderOptions::default()) {
        OptionRawImage::Some(s) => s,
        OptionRawImage::None => { return; },
    };

    let image_ref = match ImageRef::raw_image(rendered_svg) {
        OptionImageRef::Some(s) => s,
        OptionImageRef::None => { return; },
    };

    let data = RefAny::new(MyAppData { svg: image_ref });
    let app = App::new(data, AppConfig::new(LayoutSolver::Default));
    app.run(WindowCreateOptions::new(layout));
}