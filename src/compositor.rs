//! The compositor takes all the textures for a frame and draws them on top of each other.
//! This makes it possible to use OpenGL images in the background and compose SVG elements
//! into the UI.

use std::sync::{Arc, Mutex};

use glium::{
    Program, VertexBuffer, Display,
    index::{NoIndices, PrimitiveType::TriangleStrip},
    texture::texture2d::Texture2d,
    backend::Facade,
};

#[derive(Default, Debug)]
pub struct Compositor {
    textures: Vec<Texture2d>,
}

// I'd wrap this in a `Arc<Mutex<>>`, but this is only available on nightly
// So, for now, this is completely thread-unsafe
//
// However, this should be fine, as we initialize the program only from the main thread
// and never de-initialize it
static mut SHADER_FULL_SCREEN: Option<CombineTwoTexturesProgram> = None;

pub const INDICES_NO_INDICES_TRIANGLE_STRIP: NoIndices = NoIndices(TriangleStrip);

const SIMPLE_FRAGMENT_SHADER: &'static str = "\
    #version 130

    in vec4 v_color;

    void main() {
        gl_FragColor = v_color;
    }
";

/// Simple fragment shader that combines two textures `tex1` and `tex2`,
/// drawing `tex2` over `tex1`
///
/// ## Inputs
///
/// - `vec2 v_tex_coords`: The texture coordinates of the vertices
///   (see `VERTEX_SHADER_FULL_SCREEN`)
/// - `uniform sampler2d tex1`: The lower texture to be drawn (RGBA)
/// - `uniform sampler2d tex2`: The texture to draw on top (RGBA)
///
/// ## Outputs
///
/// - `vec4 gl_FragColor`: The color on the screen / to a different texture
const TWO_TEXTURES_FRAGMENT_SHADER: &'static str = "\
    #version 130

    in vec2 v_tex_coords;
    uniform sampler2D tex1;
    uniform sampler2D tex2;

    void main() {
        vec4 tex1_color = texture(tex1, v_tex_coords);
        vec4 tex2_color = texture(tex2, v_tex_coords);
        gl_FragColor = mix(tex1_color, tex2_color, tex2_color.a);
    }
";

/// This is a vertex shader that should be called with a glDrawArrays(3) and no data
/// What it does is to generate a triangle that stretches over the whole screen:
///
/// ```no_run,ignore
/// +
/// |  -
/// |     -
/// |        -
/// +-----------+
/// |           |   -
/// |  screen   |      -
/// |           |         -
/// +-----------+-----------+
/// ```
///
/// It also sets up the texture coordinates. So if you pair it with the
/// `TWO_TEXTURES_FRAGMENT_SHADER`, you can draw two textures on top of each other
const VERTEX_SHADER_FULL_SCREEN: &str = "
        #version 140
        out vec2 v_tex_coords;
        void main() {
            float x = -1.0 + float((gl_VertexID & 1) << 2);
            float y = -1.0 + float((gl_VertexID & 2) << 1);
            v_tex_coords = vec2((x+1.0)*0.5, (y+1.0)*0.5);
            gl_Position = vec4(x, y, 0, 1);
        }
";

#[derive(Debug, Copy, Clone)]
pub struct SimpleGpuVertex {
    pub coordinate: [f32; 2],
    pub tex_coords: [f32; 2],
}

implement_vertex!(SimpleGpuVertex, coordinate, tex_coords);

pub const VERTEXBUFFER_FOR_FULL_SCREEN_QUAD: [SimpleGpuVertex;3] = [
    // top left
    SimpleGpuVertex {
        coordinate: [-1.0, 1.0],
        tex_coords: [-1.0, 1.0],
    },
    // bottom left
    SimpleGpuVertex {
        coordinate: [-1.0, -1.0],
        tex_coords: [-1.0, -1.0],
    },
    // top right
    SimpleGpuVertex {
        coordinate: [1.0, 1.0],
        tex_coords: [1.0, 1.0],
    },
];

impl Compositor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_texture(&mut self, texture: Texture2d) {
        self.textures.push(texture);
    }

    /// Combine all texture together. Returns None if there are no textures in `self.textures`.
    pub fn combine_all_textures<T: Facade>(&self, display: &T, sample_behaviour: SampleBehaviour)
        -> Option<Texture2d>
    {
        // lazily initialize shader
        if unsafe { SHADER_FULL_SCREEN.is_none() } {
            unsafe { SHADER_FULL_SCREEN = Some(CombineTwoTexturesProgram::new(display)) };
        }

        let shader = unsafe { SHADER_FULL_SCREEN.as_ref().unwrap() };
        let mut iter = self.textures.iter().skip(1);

        let mut initial_tex: Texture2d = match self.textures.get(0) {
            Some(tex) => {
                // TODO: this could be optimized
                let (w, h) = (tex.width(), tex.height());
                Texture2d::empty(display, w, h).unwrap()
            },
            None => return None,
        };

        while let Some(tex2) = iter.next() {
            let combined = shader.draw(display, &initial_tex, tex2, sample_behaviour);
            initial_tex = combined;
        }

        Some(initial_tex)
    }
}

#[derive(Debug)]
pub struct CombineTwoTexturesProgram {
    program: Program,
    vertex_buffer: VertexBuffer<SimpleGpuVertex>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SampleBehaviour {
    GlNearest,
    GlLinear,
}

impl CombineTwoTexturesProgram {

    /// Uses `VERTEX_SHADER_FULL_SCREEN`, `TWO_TEXTURES_FRAGMENT_SHADER`
    /// and `VERTEXBUFFER_FOR_FULL_SCREEN_QUAD`.
    pub fn new<T: Facade>(display: &T) -> Self {
        let program = Program::from_source(display, VERTEX_SHADER_FULL_SCREEN, TWO_TEXTURES_FRAGMENT_SHADER, None).unwrap();
        let vertex_buf = VertexBuffer::new(display, &VERTEXBUFFER_FOR_FULL_SCREEN_QUAD).unwrap();
        Self {
            program: program,
            vertex_buffer: vertex_buf,
        }
    }

    /// Draw tex2 over tex1, returns a new texture with the combined result
    ///
    /// NOTE: `sample_behaviour`: specify if using `GL_NEAREST` or `GL_LINEAR`
    /// for blending
    pub fn draw<T: Facade>(
        &self,
        display: &T,
        tex1: &Texture2d,
        tex2: &Texture2d,
        sample_behaviour: SampleBehaviour)
    -> Texture2d
    {
        use self::SampleBehaviour::*;
        use glium::{
            Surface,
            uniforms::{MagnifySamplerFilter, MinifySamplerFilter},
            texture::{CompressedSrgbFormat, CompressedMipmapsOption},
        };

        let max_width = tex1.width().max(tex2.width());
        let max_height = tex1.height().max(tex2.height());

        let (tex1, tex2) = match sample_behaviour {
            GlNearest => {
                (tex1.sampled()
                    .magnify_filter(MagnifySamplerFilter::Nearest)
                    .minify_filter(MinifySamplerFilter::LinearMipmapNearest),
                 tex2.sampled()
                 .magnify_filter(MagnifySamplerFilter::Nearest)
                 .minify_filter(MinifySamplerFilter::LinearMipmapNearest))
            },
            GlLinear => {
                (tex1.sampled(),
                 tex2.sampled())
            }
        };

        let uniforms = uniform! {
            tex1: tex1,
            tex2: tex2,
        };

        let target = Texture2d::empty(display, max_width, max_height).unwrap();

        target.as_surface().draw(
            &self.vertex_buffer,
            &INDICES_NO_INDICES_TRIANGLE_STRIP,
            &self.program,
            &uniforms,
            &Default::default()).unwrap();

        target
    }
}

