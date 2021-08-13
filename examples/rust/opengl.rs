#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use azul::prelude::*;
use azul::widgets::Button;
use azul::str::String as AzString;

extern crate serde;
#[macro_use(Deserialize)]
extern crate serde_derive;
extern crate serde_json;

static DATA: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/data/testdata.json"));

#[derive(Debug, Clone, Deserialize)]
struct Dataset {
    coordinates: Vec<Vec<[f32;2]>>,
}

static CSS: AzString = AzString::from_const_str("
    body {
        background: white;
    }
    img {
        flex-grow: 1;
        border-radius: 50px;
        box-sizing: border-box;
        box-shadow: 0px 0px 10px black;
    }
    #the_button {
        flex-grow: 0;
        height: 20px;
        max-width: 300px;
        position: absolute;
        top: 50px;
        left: 50px;
    }
");

struct OpenGlAppState {
    // vertices, uploaded on startup
    fill_vertices_to_upload: Option<TessellatedSvgNode>,
    stroke_vertices_to_upload: Option<TessellatedSvgNode>,

    // vertex (+ index) buffer ID of the uploaded tesselated node
    fill_vertex_buffer_id: Option<VertexBuffer>,
    stroke_vertex_buffer_id: Option<VertexBuffer>,
}

extern "C" fn layout(data: &mut RefAny, _:  &mut LayoutCallbackInfo) -> StyledDom {
    Dom::body().with_child(
        Dom::image(ImageRef::callback(RenderImageCallback { cb: render_my_texture }, data.clone()))
        .with_child(Button::new("Button composited over OpenGL content!".into()).dom().with_id("the_button".into()))
    ).style(Css::from_string(CSS.clone()))
}

extern "C" fn render_my_texture(data: &mut RefAny, info: &mut RenderImageCallbackInfo) -> ImageRef {

    // invalid texture returned in cases of error:
    // does not allocate anything
    let size = info.get_bounds().get_physical_size();
    let invalid = ImageRef::invalid(
        size.width as usize,
        size.height as usize,
        RawImageFormat::R8
    );

    let data = match data.downcast_ref::<OpenGlAppState>() {
        Some(s) => s,
        None => return invalid,
    };

    // size = the calculated size that the div has AFTER LAYOUTING
    // this way you can render the OpenGL texture with the correct size
    // even if you don't know upfront what the size of the texture in the UI is going to be
    let gl_context = match info.get_gl_context().into_option() {
        Some(s) => s,
        None => return invalid,
    };

    // Render to an OpenGL texture, texture will be managed by azul
    let tex = match render_my_texture_inner(&*data, gl_context, size) {
        Some(s) => s,
        None => return invalid,
    };

    ImageRef::gl_texture(tex)
}

fn render_my_texture_inner(
    data: &OpenGlAppState,
    gl_context: Gl,
    texture_size: PhysicalSizeU32
) -> Option<Texture> {

    let framebuffers = gl_context.gen_framebuffers(1);
    gl_context.bind_framebuffer(Gl::FRAMEBUFFER, framebuffers.get(0).copied()?);

    gl_context.enable(Gl::TEXTURE_2D);

    // Create the texture to render to
    let textures = gl_context.gen_textures(1);

    gl_context.bind_texture(Gl::TEXTURE_2D, textures.get(0).copied()?);
    gl_context.tex_image_2d(
        Gl::TEXTURE_2D, 0,
        Gl::RGBA as i32,
        texture_size.width as i32,
        texture_size.height as i32,
        0,
        Gl::RGBA,
        Gl::UNSIGNED_BYTE,
        OptionU8VecRef::None
    );

    gl_context.tex_parameter_i(Gl::TEXTURE_2D, Gl::TEXTURE_MAG_FILTER, Gl::NEAREST as i32);
    gl_context.tex_parameter_i(Gl::TEXTURE_2D, Gl::TEXTURE_MIN_FILTER, Gl::NEAREST as i32);
    gl_context.tex_parameter_i(Gl::TEXTURE_2D, Gl::TEXTURE_WRAP_S, Gl::CLAMP_TO_EDGE as i32);
    gl_context.tex_parameter_i(Gl::TEXTURE_2D, Gl::TEXTURE_WRAP_T, Gl::CLAMP_TO_EDGE as i32);

    let depthbuffers = gl_context.gen_renderbuffers(1);
    gl_context.bind_renderbuffer(Gl::RENDERBUFFER, depthbuffers.get(0).copied()?);
    gl_context.renderbuffer_storage(Gl::RENDERBUFFER, Gl::DEPTH_COMPONENT, texture_size.width as i32, texture_size.height as i32);
    gl_context.framebuffer_renderbuffer(Gl::FRAMEBUFFER, Gl::DEPTH_ATTACHMENT, Gl::RENDERBUFFER, depthbuffers.get(0).copied()?);

    // Set "textures[0]" as the color attachement #0
    gl_context.framebuffer_texture_2d(Gl::FRAMEBUFFER, Gl::COLOR_ATTACHMENT0, Gl::TEXTURE_2D, textures.get(0).copied()?, 0);

    // Check that the framebuffer is complete
    debug_assert!(gl_context.check_frame_buffer_status(Gl::FRAMEBUFFER) == Gl::FRAMEBUFFER_COMPLETE);

    // DRAW HERE
    gl_context.viewport(0, 0, texture_size.width as i32, texture_size.height as i32);
    gl_context.clear_color(0.0, 1.0, 0.0, 1.0);
    gl_context.clear(Gl::COLOR_BUFFER_BIT);
    gl_context.clear_depth(0.0);
    gl_context.clear(Gl::DEPTH_BUFFER_BIT);

    data.fill_vertex_buffer_id.as_ref()?.draw(gl_context);
    data.stroke_vertex_buffer_id.as_ref()?.draw(gl_context);

    // cleanup: note: no delete_textures(), OpenGL texture ID is returned to azul
    gl_context.delete_framebuffers(framebuffers.as_ref().into());
    gl_context.delete_renderbuffers(depthbuffers.as_ref().into());
    gl_context.active_texture(0);
    gl_context.bind_texture(Gl::TEXTURE_2D, 0);
    gl_context.bind_framebuffer(Gl::FRAMEBUFFER, 0);
    gl_context.bind_renderbuffer(Gl::RENDERBUFFER, 0);

    Some(Texture {
        texture_id: textures.get(0).copied()?,
        flags: TextureFlags::default(),
        size: texture_size,
        // azul only allows r, rg or rgba
        format: RawImageFormat::BGRA8,
        gl_context,
    })
}

// uploads the vertex buffer to the GPU on creation
extern "C" fn startup_window(data: &mut RefAny, info: &mut CallbackInfo) -> Update {

    fn startup_window_inner(data: &mut RefAny, info: &mut CallbackInfo) -> Option<()> {
        let mut data = data.downcast_mut::<OpenGlAppState>()?;
        let fill_vertex_buffer = data.fill_vertices_to_upload.take()?;
        let stroke_vertex_buffer = data.stroke_vertices_to_upload.take()?;
        let gl_context = info.get_gl_context().into_option()?;
        let fill_vertex_buffer_id = VertexBuffer::new(fill_vertex_buffer, gl_context)?;
        let stroke_vertex_buffer_id = VertexBuffer::new(stroke_vertex_buffer, gl_context)?;
        None
    }

    let _ = startup_window_inner(data, info);
    Update::DoNothing
}

fn main() {

    // parse the geojson
    let parsed: Vec<Dataset> = match serde_json::from_str(DATA) {
        Ok(s) => s,
        Err(e) => {
            MsgBox::error(format!("{}", e).into());
            return;
        },
    };

    // parse the multipolygons
    let multipolygons: Vec<SvgMultiPolygon> = parsed.iter().map(|p| {
        SvgMultiPolygon {
            rings: p.coordinates.iter().map(|r| {
                let mut last: Option<SvgPoint> = None;
                SvgPath {
                    items: r.iter().filter_map(|i| {
                        let last_point = last.clone()?;
                        let current = SvgPoint { x: i[0], y: i[1] };
                        last = Some(current);
                        Some(SvgPathElement::Line(SvgLine { start: last_point, end: current }))
                    }).collect::<Vec<_>>().into(),
                }
            }).collect::<Vec<_>>().into(),
        }
    }).collect();

    MsgBox::info(format!("Parsed {} MultiPolygons!", multipolygons.len()).into());

    // tesselate fill + stroke
    let tessellated_fill: TessellatedSvgNodeVec = multipolygons.iter().map(|mp| {
        mp.tessellate_fill(SvgFillStyle::default())
    }).collect::<Vec<_>>().into();
    let tessellated_fill_join = TessellatedSvgNode::from_nodes(tessellated_fill.as_ref_vec());

    let tessellated_stroke: TessellatedSvgNodeVec = multipolygons.iter().map(|mp| {
        mp.tessellate_stroke(SvgStrokeStyle::default())
    }).collect::<Vec<_>>().into();
    let tessellated_stroke_join = TessellatedSvgNode::from_nodes(tessellated_stroke.as_ref_vec());

    let data = RefAny::new(OpenGlAppState {
        fill_vertices_to_upload: Some(tessellated_fill_join),
        stroke_vertices_to_upload: Some(tessellated_stroke_join),

        fill_vertex_buffer_id: None,
        stroke_vertex_buffer_id: None,
    });

    let app = App::new(data, AppConfig::new(LayoutSolver::Default));
    let mut window = WindowCreateOptions::new(layout);
    window.create_callback = Some(Callback { cb: startup_window }).into();
    app.run(window);
}