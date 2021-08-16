#![windows_subsystem = "windows"]

use azul::prelude::*;
use azul::widgets::Button;
use azul::str::String as AzString;

extern crate serde;
#[macro_use(Deserialize)]
extern crate serde_derive;
extern crate serde_json;

#[derive(Debug)]
struct OpenGlAppState {
    // vertices, uploaded on startup
    fill_vertices_to_upload: Option<TessellatedSvgNode>,
    stroke_vertices_to_upload: Option<TessellatedSvgNode>,

    // vertex (+ index) buffer ID of the uploaded tesselated node
    texture: Option<Texture>,
    fill_vertex_buffer_id: Option<TessellatedGPUSvgNode>,
    stroke_vertex_buffer_id: Option<TessellatedGPUSvgNode>,
}

static DATA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/data/testdata.json"
));

#[derive(Debug, Clone, Deserialize)]
struct Dataset {
    coordinates: Vec<Vec<Vec<[f32;2]>>>,
}

extern "C"
fn layout(data: &mut RefAny, _:  &mut LayoutCallbackInfo) -> StyledDom {
    Dom::body()
    .with_inline_style("background: #ffffff; padding: 10px;".into())
    .with_child(
        Dom::image(ImageRef::callback(data.clone(), render_my_texture))
        .with_inline_style("
            flex-grow: 1;
            border-radius: 50px;
            box-sizing: border-box;
            box-shadow: 0px 0px 10px black;
        ".into())
        .with_child(
            Button::new("Button composited over OpenGL content!".into())
            .dom()
            .with_inline_style("
                margin-top: 50px;
                margin-left: 50px;
            ".into())
        )
    ).style(Css::empty())
}

extern "C"
fn render_my_texture(data: &mut RefAny, info: &mut RenderImageCallbackInfo) -> ImageRef {

    // size = the calculated size that the div has AFTER LAYOUTING
    // this way you can render the OpenGL texture with the correct size
    // even if you don't know upfront what the size of the texture in the UI is going to be
    let size = info.get_bounds().get_physical_size();
    let invalid = ImageRef::invalid(
        size.width as usize,
        size.height as usize,
        RawImageFormat::R8
    );

    match render_my_texture_inner(data, info, size) {
        Some(s) => s,
        None => invalid
    }
}

fn render_my_texture_inner(
    data: &mut RefAny,
    info: &mut RenderImageCallbackInfo,
    texture_size: PhysicalSizeU32
) -> Option<ImageRef> {

    let mut data = data.downcast_mut::<OpenGlAppState>()?;
    let mut data = &mut *data;

    let gl_context = info.get_gl_context().into_option()?;
    let fill_vertex_buffer = data.fill_vertex_buffer_id.as_ref()?;
    let stroke_vertex_buffer = data.stroke_vertex_buffer_id.as_ref()?;
    let mut texture = data.texture.as_mut()?;

    texture.clear();

    texture.draw_tesselated_svg_gpu_node(
        fill_vertex_buffer,
        texture_size,
        ColorU::from_str("#ff0000".into()),
        StyleTransformVec::from_const_slice(&[]),
    );

    texture.draw_tesselated_svg_gpu_node(
        stroke_vertex_buffer,
        texture_size,
        ColorU::from_str("#158DE3".into()),
        StyleTransformVec::from_const_slice(&[]),
    );

    Some(ImageRef::gl_texture(texture.clone()))
}

// uploads the vertex buffer to the GPU on creation
extern "C" fn startup_window(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let _ = startup_window_inner(data, info);
    Update::DoNothing
}

// Function called when the OpenGL context has been initialized:
// allocate all textures and upload vertex buffer to GPU
fn startup_window_inner(data: &mut RefAny, info: &mut CallbackInfo) -> Option<()> {

    let mut data = data.downcast_mut::<OpenGlAppState>()?;
    let fill_vertex_buffer = data.fill_vertices_to_upload.take()?;
    let stroke_vertex_buffer = data.stroke_vertices_to_upload.take()?;
    let gl_context = info.get_gl_context().into_option()?;

    data.fill_vertex_buffer_id = Some(TessellatedGPUSvgNode::new(
        &fill_vertex_buffer,
        gl_context.clone()
    ));

    data.stroke_vertex_buffer_id = Some(TessellatedGPUSvgNode::new(
        &stroke_vertex_buffer,
        gl_context.clone()
    ));

    let mut col = ColorU::from_str("#abc0cf".into());

    data.texture = Some(Texture::allocate_rgba8(
        gl_context.clone(),
        PhysicalSizeU32 { width: 800, height: 600 },
        col,
    ));

    Some(())
}

fn parse_multipolygons(data: &str) -> Vec<SvgMultiPolygon> {
    // parse the geojson
    let parsed: Vec<Dataset> = match serde_json::from_str(data) {
        Ok(s) => s,
        Err(e) => {
            MsgBox::error(format!("{}", e).into());
            return Vec::new();
        },
    };

    // parse the multipolygons
    parsed.iter().map(|p| {
        SvgMultiPolygon {
            rings: p.coordinates[0].iter().map(|r| {
                let mut last: Option<SvgPoint> = None;
                SvgPath {
                    items: r.iter().filter_map(|i| {
                        let last_point = last.clone();

                        let mut current = SvgPoint { x: i[0], y: i[1] };
                        current.x -= 13.804493;
                        current.y -= 51.05264;
                        current.x *= 50000.0;
                        current.y *= 50000.0;
                        current.x += 500.0;
                        current.y += 500.0;
                        current.x *= 2.0;
                        current.y *= 2.0;

                        last = Some(current);
                        let last_point = last_point?;

                        Some(SvgPathElement::Line(SvgLine { start: last_point, end: current }))
                    }).collect::<Vec<_>>().into(),
                }
            }).collect::<Vec<_>>().into(),
        }
    }).collect()
}

fn main() {

    let multipolygons = parse_multipolygons(DATA);

    println!("parsed {} multipolygons!", multipolygons.len());

    // tesselate fill
    let tessellated_fill: TessellatedSvgNodeVec = multipolygons.iter().map(|mp| {
        mp.tessellate_fill(SvgFillStyle::default())
    }).collect::<Vec<_>>().into();
    let tessellated_fill_join = TessellatedSvgNode::from_nodes(tessellated_fill.as_ref_vec());

    // tesselate stroke
    let tessellated_stroke: TessellatedSvgNodeVec = multipolygons.iter().map(|mp| {
        mp.tessellate_stroke(SvgStrokeStyle::default())
    }).collect::<Vec<_>>().into();
    let tessellated_stroke_join = TessellatedSvgNode::from_nodes(tessellated_stroke.as_ref_vec());

    // initalize data
    let data = RefAny::new(OpenGlAppState {
        fill_vertices_to_upload: Some(tessellated_fill_join),
        stroke_vertices_to_upload: Some(tessellated_stroke_join),

        texture: None,
        fill_vertex_buffer_id: None,
        stroke_vertex_buffer_id: None,
    });

    let app = App::new(data, AppConfig::new(LayoutSolver::Default));

    let mut window = WindowCreateOptions::new(layout);
    window.create_callback = Some(Callback { cb: startup_window }).into();
    app.run(window);
}