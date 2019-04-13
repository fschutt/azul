
/// Typedef for an OpenGL handle
pub type GLuint = u32;

use std::{
    rc::Rc,
    hash::{Hasher, Hash},
};
use gleam::gl::{self, Gl};

/// OpenGL texture, use `ReadOnlyWindow::create_texture` to create a texture
///
/// **WARNING**: Don't forget to call `ReadOnlyWindow::unbind_framebuffer()`
/// when you are done with your OpenGL drawing, otherwise WebRender will render
/// to the texture, not the window, so your texture will actually never show up.
/// If you use a `Texture` and you get a blank screen, this is probably why.
pub struct Texture {
    /// Raw OpenGL texture ID
    pub texture_id: GLuint,
    /// Dimensions (width, height in pixels).
    pub width: usize,
    pub height: usize,
    /// A reference-counted pointer to the OpenGL context (so that the texture can be deleted in the destructor)
    pub gl_context: Rc<Gl>,
}

impl Texture {

    /// Note: Creates a new texture (calls `gen_textures()`)
    pub fn new(gl_context: Rc<Gl>, width: usize, height: usize) -> Self {

        let textures = gl_context.gen_textures(1);
        let texture_id = textures[0];

        gl_context.bind_texture(gl::TEXTURE_2D, texture_id);
        gl_context.tex_image_2d(
            gl::TEXTURE_2D,
            0,
            gl::RGBA as i32,
            width as i32,
            height as i32,
            0,
            gl::RGBA,
            gl::UNSIGNED_BYTE,
            None
        );

        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);

        Self {
            texture_id,
            width,
            height,
            gl_context,
        }
    }

    /// Sets the current texture as the target for `gl::COLOR_ATTACHEMENT0`, so that
    pub fn get_framebuffer<'a>(&'a mut self) -> FrameBuffer<'a> {

        let fb = FrameBuffer::new(self);

        // Set "textures[0]" as the color attachement #0
        self.gl_context.framebuffer_texture_2d(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, self.texture_id, 0);
        self.gl_context.draw_buffers(&[gl::COLOR_ATTACHMENT0]);

        // Check that the framebuffer is complete
        debug_assert!(self.gl_context.check_frame_buffer_status(gl::FRAMEBUFFER) == gl::FRAMEBUFFER_COMPLETE);

        fb
    }
}

impl ::std::fmt::Display for Texture {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "Texture {{ id: {}, {}x{} }}", self.texture_id, self.width, self.height)
    }
}

impl ::std::fmt::Debug for Texture {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl Hash for Texture {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.texture_id.hash(state);
    }
}

impl PartialEq for Texture {
    /// Note: Comparison uses only the OpenGL ID, it doesn't compare the
    /// actual contents of the texture.
    fn eq(&self, other: &Texture) -> bool {
        self.texture_id == other.texture_id
    }
}

impl Eq for Texture { }

impl PartialOrd for Texture {
    fn partial_cmp(&self, other: &Self) -> Option<::std::cmp::Ordering> {
        Some((self.texture_id).cmp(&(other.texture_id)))
    }
}

impl Ord for Texture {
    fn cmp(&self, other: &Self) -> ::std::cmp::Ordering {
        (self.texture_id).cmp(&(other.texture_id))
    }
}

impl Drop for Texture {
    fn drop(&mut self) {
        self.gl_context.delete_textures(&[self.texture_id]);
    }
}

/// RGBA-backed framebuffer
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct FrameBuffer<'a> {
    pub framebuffer_id: GLuint,
    pub texture: &'a mut Texture,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct VertexBuffer {
    pub vertex_buffer_id: GLuint,
    // pub gl_context: Rc<Gl>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct IndexBuffer {
    pub index_buffer_id: GLuint,
    // pub gl_context: Rc<Gl>,
}

impl<'a> FrameBuffer<'a> {

    fn new(texture: &'a Texture) -> Self {
        let framebuffers = texture.gl_context.gen_framebuffers(1);

        Self {
            framebuffer_id: framebuffers[0],
            texture,
        }
    }

    pub fn bind(&mut self) {
        self.texture.gl_context.bind_texture(gl::TEXTURE_2D, self.texture.texture_id);
        self.texture.gl_context.bind_framebuffer(gl::FRAMEBUFFER, self.framebuffer_id);
        self.texture.gl_context.viewport(0, 0, self.texture.width as i32, self.texture.height as i32);
    }

    pub fn draw(&mut self, shader: GlShader, vertices: VertexBuffer) {
        // TODO!
    }

    pub fn unbind(&mut self) {
        self.texture.gl_context.bind_texture(gl::TEXTURE_2D, 0);
        self.texture.gl_context.bind_framebuffer(gl::FRAMEBUFFER, 0);
    }
}

impl<'a> Drop for FrameBuffer<'a> {
    fn drop(&mut self) {
        self.texture.gl_context.delete_framebuffers(&[self.framebuffer_id]);
    }
}

pub struct GlShader {
    pub shader_program: GLuint,
    pub gl_context: Rc<Gl>,
}

impl ::std::fmt::Display for GlShader {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "GlShader({})", self.shader_program)
    }
}

impl ::std::fmt::Debug for GlShader {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl ::std::hash::Hash for GlShader {
    fn hash<H>(&self, state: &mut H) where H: ::std::hash::Hasher {
        state.write_usize(self.shader_program as usize);
    }
}

impl PartialEq for GlShader {
    fn eq(&self, rhs: &Self) -> bool {
        self.shader_program as usize == rhs.shader_program as usize
    }
}

impl PartialOrd for GlShader {
    fn partial_cmp(&self, other: &Self) -> Option<::std::cmp::Ordering> {
        Some((self.shader_program as usize).cmp(&(other.shader_program as usize)))
    }
}

impl Ord for GlShader {
    fn cmp(&self, other: &Self) -> ::std::cmp::Ordering {
        (self.shader_program as usize).cmp(&(other.shader_program as usize))
    }
}

impl Eq for GlShader { }

impl Drop for GlShader {
    fn drop(&mut self) {
        self.gl_context.delete_program(self.shader_program);
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct VertexShaderCompileError {
    pub error_id: i32,
    pub info_log: String
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct FragmentShaderCompileError {
    pub error_id: i32,
    pub info_log: String
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum GlShaderCompileError {
    Vertex(VertexShaderCompileError),
    Fragment(FragmentShaderCompileError),
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct GlShaderLinkError {
    pub error_id: i32,
    pub info_log: String
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum GlShaderCreateError {
    Compile(GlShaderCompileError),
    Link(GlShaderLinkError),
}

impl GlShader {

    /// Compiles and creates a new OpenGL shader, created from a vertex and a fragment shader string.
    ///
    /// If the shader fails to compile, the shader object gets automatically deleted, no cleanup necessary.
    pub fn new(context: Rc<Gl>, vertex_shader_source: &str, fragment_shader_source: &str) -> Result<Self, GlShaderCreateError> {

        fn str_to_bytes(input: &str) -> Vec<u8> {
            let mut v: Vec<u8> = input.into();
            v.push(0);
            v
        }

        let vertex_shader_source = str_to_bytes(vertex_shader_source);
        let fragment_shader_source = str_to_bytes(fragment_shader_source);

        // Compile vertex shader

        let vertex_shader_object = context.create_shader(gl::VERTEX_SHADER);
        context.shader_source(vertex_shader_object, &[&vertex_shader_source]);
        context.compile_shader(vertex_shader_object);

        #[cfg(debug_assertions)] {
            if let Some(error_id) = get_gl_shader_error(&*context, vertex_shader_object) {
                let info_log = context.get_shader_info_log(vertex_shader_object);
                context.delete_shader(vertex_shader_object);
                return Err(GlShaderCreateError::Compile(GlShaderCompileError::Vertex(VertexShaderCompileError { error_id, info_log })));
            }
        }

        // Compile fragment shader

        let fragment_shader_object = context.create_shader(gl::FRAGMENT_SHADER);
        context.shader_source(fragment_shader_object, &[&fragment_shader_source]);
        context.compile_shader(fragment_shader_object);

        #[cfg(debug_assertions)] {
            if let Some(error_id) = get_gl_shader_error(&*context, fragment_shader_object) {
                let info_log = context.get_shader_info_log(fragment_shader_object);
                context.delete_shader(vertex_shader_object);
                context.delete_shader(fragment_shader_object);
                return Err(GlShaderCreateError::Compile(GlShaderCompileError::Fragment(FragmentShaderCompileError { error_id, info_log })));
            }
        }

        // Link program

        let program = context.create_program();
        context.attach_shader(program, vertex_shader_object);
        context.attach_shader(program, fragment_shader_object);
        context.link_program(program);

        #[cfg(debug_assertions)] {
            if let Some(error_id) = get_gl_program_error(&*context, program) {
                let info_log = context.get_program_info_log(program);
                context.delete_shader(vertex_shader_object);
                context.delete_shader(fragment_shader_object);
                context.delete_program(program);
                return Err(GlShaderCreateError::Link(GlShaderLinkError { error_id, info_log }));
            }
        }

        context.delete_shader(vertex_shader_object);
        context.delete_shader(fragment_shader_object);

        Ok(GlShader {
            shader_program: program,
            gl_context: context,
        })
    }
}

#[cfg(debug_assertions)]
fn get_gl_shader_error(context: &Gl, shader_object: GLuint) -> Option<i32> {
    let mut err = [0];
    unsafe { context.get_shader_iv(shader_object, gl::COMPILE_STATUS, &mut err) };
    let err_code = err[0];
    if err_code == 0 { None } else { Some(err_code) }
}

#[cfg(debug_assertions)]
fn get_gl_program_error(context: &Gl, shader_object: GLuint) -> Option<i32> {
    let mut err = [0];
    unsafe { context.get_program_iv(shader_object, gl::LINK_STATUS, &mut err) };
    let err_code = err[0];
    if err_code == 0 { None } else { Some(err_code) }
}
