
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

macro_rules! impl_traits_for_gl_object {
    ($struct_name:ident, $gl_id_field:ident) => {

        impl ::std::fmt::Debug for $struct_name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                write!(f, "{}", self)
            }
        }

        impl Hash for $struct_name {
            fn hash<H: Hasher>(&self, state: &mut H) {
                self.$gl_id_field.hash(state);
            }
        }

        impl PartialEq for $struct_name {
            /// Note: Comparison uses only the OpenGL ID, it doesn't compare the
            /// actual contents of the texture.
            fn eq(&self, other: &$struct_name) -> bool {
                self.$gl_id_field == other.$gl_id_field
            }
        }

        impl Eq for $struct_name { }

        impl PartialOrd for $struct_name {
            fn partial_cmp(&self, other: &Self) -> Option<::std::cmp::Ordering> {
                Some((self.$gl_id_field).cmp(&(other.$gl_id_field)))
            }
        }

        impl Ord for $struct_name {
            fn cmp(&self, other: &Self) -> ::std::cmp::Ordering {
                (self.$gl_id_field).cmp(&(other.$gl_id_field))
            }
        }
    };
    ($struct_name:ident<$lt:lifetime>, $gl_id_field:ident) => {
        impl<$lt> ::std::fmt::Debug for $struct_name<$lt> {
            fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                write!(f, "{}", self)
            }
        }

        impl<$lt> Hash for $struct_name<$lt> {
            fn hash<H: Hasher>(&self, state: &mut H) {
                self.$gl_id_field.hash(state);
            }
        }

        impl<$lt>PartialEq for $struct_name<$lt> {
            /// Note: Comparison uses only the OpenGL ID, it doesn't compare the
            /// actual contents of the texture.
            fn eq(&self, other: &$struct_name) -> bool {
                self.$gl_id_field == other.$gl_id_field
            }
        }

        impl<$lt> Eq for $struct_name<$lt> { }

        impl<$lt> PartialOrd for $struct_name<$lt> {
            fn partial_cmp(&self, other: &Self) -> Option<::std::cmp::Ordering> {
                Some((self.$gl_id_field).cmp(&(other.$gl_id_field)))
            }
        }

        impl<$lt> Ord for $struct_name<$lt> {
            fn cmp(&self, other: &Self) -> ::std::cmp::Ordering {
                (self.$gl_id_field).cmp(&(other.$gl_id_field))
            }
        }
    };
}

impl_traits_for_gl_object!(Texture, texture_id);

impl Drop for Texture {
    fn drop(&mut self) {
        self.gl_context.delete_textures(&[self.texture_id]);
    }
}

/// RGBA-backed framebuffer
pub struct FrameBuffer<'a> {
    pub framebuffer_id: GLuint,
    pub texture: &'a mut Texture,
}

impl<'a> ::std::fmt::Display for FrameBuffer<'a> {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "FrameBuffer {{ id: {}, texture: {} }}", self.framebuffer_id, self.texture)
    }
}

impl_traits_for_gl_object!(FrameBuffer<'a>, framebuffer_id);

pub struct VertexBuffer {
    pub vertex_buffer_id: GLuint,
    pub vertex_buffer_len: usize,
    pub vertex_layout: VertexLayout,
    pub gl_context: Rc<Gl>,
}

impl ::std::fmt::Display for VertexBuffer {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f,
            "VertexBuffer {{ buffer: {} (length: {}), layout: {:#?} }})",
            self.vertex_buffer_id, self.vertex_buffer_len, self.vertex_layout
        )
    }
}

impl_traits_for_gl_object!(VertexBuffer, vertex_buffer_id);

impl Drop for VertexBuffer {
    fn drop(&mut self) {
        self.gl_context.delete_buffers(&[self.vertex_buffer_id]);
    }
}

/// Describes the vertex layout and offsets
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VertexLayout {
    pub fields: Vec<VertexAttribute>,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VertexAttribute {
    pub name: String,
    pub offset: usize,
    pub layout_location: Option<usize>,
    pub size: usize,
    pub stride: usize,
    pub attribute_type: VertexAttributeType,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum VertexAttributeType {
    Float,
    UnsignedInt,
    Int,
}

pub trait VertexLayoutDescription {
    fn get_description() -> VertexLayout;
}

impl VertexBuffer {

    pub fn new<T: VertexLayoutDescription>(gl_context: Rc<Gl>, vertices: &[T]) -> Self {

        use std::mem;

        let vertex_layout = T::get_description();

        let vertex_buffer_id = gl::gen_buffers(1);
        let vertex_buffer_id = index_buffer_id[0];

        gl::buffer_data(gl::GL_ARRAY_BUFFER, mem::size_of::<T>() * vertices.len(), vertices, gl::STATIC_DRAW);

        Self {
            vertex_buffer_id,
            vertex_buffer_len: vertices.len(),
            vertex_layout,
            gl_context,
        }
    }

    pub fn empty<T: VertexLayoutDescription>(gl_context: Rc<Gl>) -> Self {
        Self::new(&[])
    }
}

impl Drop for VertexBuffer {
    fn drop(&mut self) {
        self.gl_context.delete_buffers(&[self.index_buffer_id]);
    }
}

impl IndexBuffer {
    pub fn new(gl_context: Rc<Gl>, indices: &[u32]) -> Self {
        use std::mem;

        let index_buffer_id = gl::gen_buffers(1);
        let index_buffer_id = index_buffer_id[0];

        gl::buffer_data(gl::ELEMENT_ARRAY_BUFFER, mem::size_of::<u32>() * indices.len(), indices, gl::STATIC_DRAW);

        Self {
            index_buffer_id,
            index_buffer_len: indices.len(),
            gl_context,
        }
    }
}

pub struct IndexBuffer {
    pub index_buffer_id: GLuint,
    pub index_buffer_len: usize,
    pub gl_context: Rc<Gl>,
}

impl ::std::fmt::Display for IndexBuffer {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "IndexBuffer {{ id: {}, length: {} }}", self.index_buffer_id, self.index_buffer_len)
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Uniform {
    pub name: String,
    pub uniform_type: UniformType,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub enum UniformType {
    Float(f32),
    Double(f64),
    UnsignedInt(usize),
    SignedInt(usize),
}


impl<'a> FrameBuffer<'a> {

    fn new(texture: &'a mut Texture) -> Self {
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
    pub program_id: GLuint,
    pub gl_context: Rc<Gl>,
}

impl ::std::fmt::Display for GlShader {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "GlShader({})", self.program_id)
    }
}

impl_traits_for_gl_object!(GlShader, program_id);

impl Drop for GlShader {
    fn drop(&mut self) {
        self.gl_context.delete_program(self.program_id);
    }
}

#[derive(Clone)]
pub struct VertexShaderCompileError {
    pub error_id: i32,
    pub info_log: String
}

impl_traits_for_gl_object!(VertexShaderCompileError, error_id);

impl ::std::fmt::Display for VertexShaderCompileError {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "E{}: {}", self.error_id, self.info_log)
    }
}

#[derive(Clone)]
pub struct FragmentShaderCompileError {
    pub error_id: i32,
    pub info_log: String
}

impl_traits_for_gl_object!(FragmentShaderCompileError, error_id);

impl ::std::fmt::Display for FragmentShaderCompileError {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "E{}: {}", self.error_id, self.info_log)
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum GlShaderCompileError {
    Vertex(VertexShaderCompileError),
    Fragment(FragmentShaderCompileError),
}

impl_display!(GlShaderCompileError, {
    Vertex(vert_err) => format!("Failed to compile vertex shader: {}", vert_err),
    Fragment(frag_err) => format!("Failed to compile fragment shader: {}", frag_err),
});

#[derive(Clone)]
pub struct GlShaderLinkError {
    pub error_id: i32,
    pub info_log: String
}

impl_traits_for_gl_object!(GlShaderLinkError, error_id);

impl ::std::fmt::Display for GlShaderLinkError {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "E{}: {}", self.error_id, self.info_log)
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum GlShaderCreateError {
    Compile(GlShaderCompileError),
    Link(GlShaderLinkError),
}

impl_display!(GlShaderCreateError, {
    Compile(compile_err) => format!("Shader compile error: {}", compile_err),
    Link(link_err) => format!("Shader linking error: {}", link_err),
});

impl GlShader {

    /// Compiles and creates a new OpenGL shader, created from a vertex and a fragment shader string.
    ///
    /// If the shader fails to compile, the shader object gets automatically deleted, no cleanup necessary.
    pub fn new(gl_context: Rc<Gl>, vertex_shader_source: &str, fragment_shader_source: &str) -> Result<Self, GlShaderCreateError> {

        fn str_to_bytes(input: &str) -> Vec<u8> {
            let mut v: Vec<u8> = input.into();
            v.push(0);
            v
        }

        let vertex_shader_source = str_to_bytes(vertex_shader_source);
        let fragment_shader_source = str_to_bytes(fragment_shader_source);

        // Compile vertex shader

        let vertex_shader_object = gl_context.create_shader(gl::VERTEX_SHADER);
        gl_context.shader_source(vertex_shader_object, &[&vertex_shader_source]);
        gl_context.compile_shader(vertex_shader_object);

        #[cfg(debug_assertions)] {
            if let Some(error_id) = get_gl_shader_error(&*gl_context, vertex_shader_object) {
                let info_log = gl_context.get_shader_info_log(vertex_shader_object);
                gl_context.delete_shader(vertex_shader_object);
                return Err(GlShaderCreateError::Compile(GlShaderCompileError::Vertex(VertexShaderCompileError { error_id, info_log })));
            }
        }

        // Compile fragment shader

        let fragment_shader_object = gl_context.create_shader(gl::FRAGMENT_SHADER);
        gl_context.shader_source(fragment_shader_object, &[&fragment_shader_source]);
        gl_context.compile_shader(fragment_shader_object);

        #[cfg(debug_assertions)] {
            if let Some(error_id) = get_gl_shader_error(&*gl_context, fragment_shader_object) {
                let info_log = gl_context.get_shader_info_log(fragment_shader_object);
                gl_context.delete_shader(vertex_shader_object);
                gl_context.delete_shader(fragment_shader_object);
                return Err(GlShaderCreateError::Compile(GlShaderCompileError::Fragment(FragmentShaderCompileError { error_id, info_log })));
            }
        }

        // Link program

        let program_id = gl_context.create_program();
        gl_context.attach_shader(program_id, vertex_shader_object);
        gl_context.attach_shader(program_id, fragment_shader_object);
        gl_context.link_program(program_id);

        #[cfg(debug_assertions)] {
            if let Some(error_id) = get_gl_program_error(&*gl_context, program_id) {
                let info_log = gl_context.get_program_info_log(program_id);
                gl_context.delete_shader(vertex_shader_object);
                gl_context.delete_shader(fragment_shader_object);
                gl_context.delete_program(program_id);
                return Err(GlShaderCreateError::Link(GlShaderLinkError { error_id, info_log }));
            }
        }

        gl_context.delete_shader(vertex_shader_object);
        gl_context.delete_shader(fragment_shader_object);

        Ok(GlShader { program_id, gl_context })
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
