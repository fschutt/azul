use azul::{prelude::*, str::String as AzString, widgets::Button};

extern crate serde;
#[macro_use(Deserialize)]
extern crate serde_derive;
extern crate serde_json;

static DATA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../assets/data/testdata.json"
));

#[derive(Debug, Clone, Deserialize)]
struct Dataset {
    coordinates: Vec<Vec<Vec<[f32; 2]>>>,
}

#[derive(Debug)]
struct OpenGlAppState {
    rotation_deg: f32,
    // vertices, uploaded on startup
    fill_vertices_to_upload: Option<TessellatedSvgNode>,
    stroke_vertices_to_upload: Option<TessellatedSvgNode>,
    // vertex (+ index) buffer ID of the uploaded tesselated node
    fill_vertex_buffer_id: Option<TessellatedGPUSvgNode>,
    stroke_vertex_buffer_id: Option<TessellatedGPUSvgNode>,
}

extern "C" 
fn layout(mut data: RefAny, _: LayoutCallbackInfo) -> StyledDom {
    Dom::new_body()
        .with_inline_style("
            background: linear-gradient(blue, black);
            padding: 10px;
        ")
        .with_child(
            Dom::image(
                ImageRef::callback(data.clone(), render_my_texture)
            )
            .with_inline_style("
                flex-grow: 1;
                border-radius: 50px;
                box-sizing: border-box;
                box-shadow: 0px 0px 10px black;
            ")
            .with_child(
                Button::new("Button drawn on top of OpenGL!")
                    .dom()
                    .with_inline_style(
                        "
                margin-top: 50px;
                margin-left: 50px;
            ",
                    ),
            ),
        )
        .style(Css::empty())
}

extern "C" 
fn render_my_texture(mut data: RefAny, info: RenderImageCallbackInfo) -> ImageRef {

    // size = the calculated size that the div has AFTER LAYOUTING
    // this way you can render the OpenGL texture with the correct size
    // even if you don't know upfront what the size of the texture in the UI is going to be

    let size = info.get_bounds().get_physical_size();
    let invalid = ImageRef::null_image(
        size.width as usize,
        size.height as usize,
        RawImageFormat::R8,
        Vec::new(),
    );

    match render_my_texture_inner(data, info, size) {
        Some(s) => s,
        None => invalid,
    }
}

fn render_my_texture_inner(
    data: &mut RefAny,
    info: &mut RenderImageCallbackInfo,
    texture_size: PhysicalSizeU32,
) -> Option<ImageRef> {

    let mut data = data.downcast_mut::<OpenGlAppState>()?;
    let mut data = &mut *data;

    let gl_context = info.get_gl_context().into_option()?;
    let fill_vertex_buffer = data.fill_vertex_buffer_id.as_ref()?;
    let stroke_vertex_buffer = data.stroke_vertex_buffer_id.as_ref()?;
    let rotation_deg = data.rotation_deg;
    let mut texture = Texture::allocate_rgba8(
        gl_context.clone(),
        texture_size,
        ColorU::from_str("#ffffffef"),
    );

    texture.clear();

    texture.draw_tesselated_svg_gpu_node(
        fill_vertex_buffer as *const _,
        texture_size,
        ColorU::from_str("#cc00cc"),
        vec![
            StyleTransform::Translate(StyleTransformTranslate2D {
                x: PixelValue::const_percent(50),
                y: PixelValue::const_percent(50),
            }),
            StyleTransform::Rotate(AngleValue::deg(rotation_deg)),
        ],
    );

    texture.draw_tesselated_svg_gpu_node(
        stroke_vertex_buffer as *const _,
        texture_size,
        ColorU::from_str("#158DE3"),
        vec![StyleTransform::Rotate(AngleValue::deg(rotation_deg))],
    );

    // TODO: segfault when inserting the following line:
    // let tx = ImageRef::gl_texture(texture.clone());

    Some(ImageRef::gl_texture(texture))
}

// uploads the vertex buffer to the GPU on window creation
extern "C" 
fn startup_window(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let _ = startup_window_inner(&mut data, &mut info);
    Update::DoNothing
}

// Function called when the OpenGL context has been initialized:
// allocate all textures and upload vertex buffer to GPU
fn startup_window_inner(data: &mut RefAny, info: &mut CallbackInfo) -> Option<()> {
    {
        let mut data = data.downcast_mut::<OpenGlAppState>()?;
        let fill_vertex_buffer = data.fill_vertices_to_upload.take()?;
        let stroke_vertex_buffer = data.stroke_vertices_to_upload.take()?;
        let gl_context = info.get_gl_context().into_option()?;

        data.fill_vertex_buffer_id = Some(TessellatedGPUSvgNode::new(
            &fill_vertex_buffer as *const _,
            gl_context.clone(),
        ));

        data.stroke_vertex_buffer_id = Some(TessellatedGPUSvgNode::new(
            &stroke_vertex_buffer as *const _,
            gl_context.clone(),
        ));
    }

    info.start_timer(Timer::new(data.clone(), animate, info.get_system_time_fn()));

    Some(())
}

fn parse_multipolygons(data: &str) -> Vec<SvgMultiPolygon> {
    // parse the geojson
    let parsed: Vec<Dataset> = match serde_json::from_str(data) {
        Ok(s) => s,
        Err(e) => {
            MsgBox::error(format!("{}", e));
            return Vec::new();
        }
    };

    // parse the multipolygons
    parsed
        .iter()
        .map(|p| SvgMultiPolygon {
            rings: p.coordinates[0]
                .iter()
                .map(|r| {
                    let mut last: Option<SvgPoint> = None;
                    SvgPath {
                        items: r
                            .iter()
                            .filter_map(|i| {
                                let last_point = last.clone();

                                let mut current = SvgPoint { x: i[0], y: i[1] };
                                current.x -= 13.804483;
                                current.y -= 51.05274;
                                current.x *= 50000.0;
                                current.y *= 50000.0;
                                current.x += 700.0;
                                current.y += 700.0;
                                current.x *= 2.0;
                                current.y *= 2.0;

                                last = Some(current);
                                let last_point = last_point?;

                                Some(SvgPathElement::Line(SvgLine {
                                    start: last_point,
                                    end: current,
                                }))
                            })
                            .collect::<Vec<_>>()
                            .into(),
                    }
                })
                .collect::<Vec<_>>()
                .into(),
        })
        .collect()
}

/// Animation function rotating the map constantly
extern "C" 
fn animate(
    mut timer_data: RefAny,
    info: TimerCallbackInfo,
) -> TimerCallbackReturn {
    TimerCallbackReturn {
        should_terminate: TerminateTimer::Continue,
        should_update: match timer_data.downcast_mut::<OpenGlAppState>() {
            Some(mut s) => {
                s.rotation_deg += 1.0;
                Update::RefreshDom
            }
            None => Update::DoNothing,
        },
    }
}

fn main() {
    println!("starting!");

    let multipolygons = parse_multipolygons(DATA);

    println!("parsed {} multipolygons!", multipolygons.len());

    // tesselate fill
    let tessellated_fill: TessellatedSvgNodeVec = multipolygons
        .iter()
        .map(|mp| mp.tessellate_fill(SvgFillStyle::default()))
        .collect::<Vec<_>>()
        .into();
    
    let tessellated_fill_join = TessellatedSvgNode::from_nodes(
        tessellated_fill.as_ref_vec()
    );

    let mut stroke_style = SvgStrokeStyle::default();
    stroke_style.line_width = 4.0;

    // tesselate stroke
    let tessellated_stroke: TessellatedSvgNodeVec = multipolygons
        .iter()
        .map(|mp| mp.tessellate_stroke(stroke_style))
        .collect::<Vec<_>>()
        .into();
    
    let tessellated_stroke_join = TessellatedSvgNode::from_nodes(
        tessellated_stroke.as_ref_vec()
    );

    // initalize data
    let data = RefAny::new(OpenGlAppState {
        fill_vertices_to_upload: Some(tessellated_fill_join),
        stroke_vertices_to_upload: Some(tessellated_stroke_join),
        rotation_deg: 0.0,

        fill_vertex_buffer_id: None,
        stroke_vertex_buffer_id: None,
    });

    println!("starting app");
    let mut app = App::new(data, AppConfig::new());

    let mut window = WindowCreateOptions::new(layout);
    window.state.flags.frame = WindowFrame::Maximized;
    window.create_callback = Some(Callback { cb: startup_window }).into();
    app.run(window);
}
