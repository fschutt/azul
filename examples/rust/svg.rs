#![windows_subsystem = "windows"]

use std::time::Duration;

use azul::prelude::{String as AzString, *};

const CSS: &str = "
#svg-container {
    width: 100%;
    height: 100%;
}";

const SVG: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/assets/svg/AJ_Digital_Camera.svg"
));
const SVG_STRING: AzString = AzString::from_const_str(SVG);

#[derive(Debug)]
struct MyAppData {
    // Timing / performance data
    timing: TimingData,
    // SVG rendered to a CPU-backed image buffer
    svg: ImageRef,
}

#[derive(Debug, Clone)]
struct TimingData {
    time_to_parse: Duration,
    time_to_render: Duration,
    time_to_convert: Duration,
}

extern "C" fn layout(data: &mut RefAny, _: &mut LayoutCallbackInfo) -> StyledDom {
    let (rendered_svg, timing) = match data.downcast_ref::<MyAppData>() {
        Some(s) => (s.svg.clone(), s.timing.clone()),
        None => return StyledDom::default(),
    };

    Dom::body()
        .with_menu_bar(Menu::new(vec![MenuItem::String(
            StringMenuItem::new("Application").with_children(vec![MenuItem::String(
                StringMenuItem::new("Select File...").with_callback(data.clone(), open_svg_file),
            )]),
        )]))
        .with_children(vec![
            Dom::image(rendered_svg).with_inline_style("display: block;"),
            Dom::text(format!("Parsing took {:?}", timing.time_to_parse)),
            Dom::text(format!("Rendering took {:?}", timing.time_to_render)),
            Dom::text(format!(
                "Converting to ImageRef took {:?}",
                timing.time_to_convert
            )),
        ])
        .style(Css::from_string("p { font-family: sans-serif; }"))
}

// ask user for file path to new file to render
extern "C" fn open_svg_file(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let mut data = match data.downcast_mut::<MyAppData>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    // note: runs on main thread, blocks UI - TODO: offload to background!
    let new_file = FileDialog::select_file("Select SVG", None, None)
        .and_then(|file_path| match File::open(file_path) {
            OptionFile::Some(s) => Some(s),
            _ => None,
        })
        .and_then(|mut file| file.read_to_string().into_option())
        .and_then(|svg_string| load_svg(svg_string));

    match new_file {
        Some((new_image, new_timing_data)) => {
            data.svg = new_image;
            data.timing = new_timing_data;
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

fn load_svg(svg: AzString) -> Option<(ImageRef, TimingData)> {
    let mut start = std::time::Instant::now();

    let svg = match Svg::from_string(svg.clone(), SvgParseOptions::default()) {
        ResultSvgSvgParseError::Ok(o) => o,
        _ => return None,
    };

    let end = std::time::Instant::now();
    let parse_time = end - start;
    start = end;

    let rendered_svg = match svg.render(SvgRenderOptions::default()) {
        OptionRawImage::Some(s) => s,
        OptionRawImage::None => {
            return None;
        }
    };

    let end = std::time::Instant::now();
    let render_time = end - start;
    start = end;

    let image_ref = match ImageRef::raw_image(rendered_svg) {
        OptionImageRef::Some(s) => s,
        OptionImageRef::None => {
            return None;
        }
    };

    let end = std::time::Instant::now();
    let recode_time = end - start;
    start = end;

    return Some((
        image_ref,
        TimingData {
            time_to_parse: parse_time,
            time_to_render: render_time,
            time_to_convert: recode_time,
        },
    ));
}

fn main() {
    let (svg, timing) = match load_svg(SVG_STRING) {
        Some(s) => s,
        None => return,
    };

    let data = RefAny::new(MyAppData { svg, timing });
    let app = App::new(data, AppConfig::new(LayoutSolver::Default));
    app.run(WindowCreateOptions::new(layout));
}
