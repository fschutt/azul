
/// Typedef for an OpenGL handle
pub type GLuint = u32;
pub type GLint = i32;

use std::{
    rc::Rc,
    hash::{Hasher, Hash},
    ffi::c_void,
    marker::PhantomData,
};
use gleam::gl::{self, Gl};

/// OpenGL texture, use `ReadOnlyWindow::create_texture` to create a texture
pub struct Texture {
    /// Raw OpenGL texture ID
    pub texture_id: GLuint,
    /// Width of this texture in pixels
    pub width: usize,
    /// Height of this texture in pixels
    pub height: usize,
    /// A reference-counted pointer to the OpenGL context (so that the texture can be deleted in the destructor)
    pub gl_context: Rc<Gl>,
}

/// Note: Creates a new texture (calls `gen_textures()`)
impl Texture {

    pub fn new(gl_context: Rc<Gl>, width: usize, height: usize) -> Texture {

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
    ($struct_name:ident<$t:ident: $constraint:ident>, $gl_id_field:ident) => {
        impl<$t: $constraint> ::std::fmt::Debug for $struct_name<$t> {
            fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                write!(f, "{}", self)
            }
        }

        impl<$t: $constraint> Hash for $struct_name<$t> {
            fn hash<H: Hasher>(&self, state: &mut H) {
                self.$gl_id_field.hash(state);
            }
        }

        impl<$t: $constraint>PartialEq for $struct_name<$t> {
            fn eq(&self, other: &$struct_name<$t>) -> bool {
                self.$gl_id_field == other.$gl_id_field
            }
        }

        impl<$t: $constraint> Eq for $struct_name<$t> { }

        impl<$t: $constraint> PartialOrd for $struct_name<$t> {
            fn partial_cmp(&self, other: &Self) -> Option<::std::cmp::Ordering> {
                Some((self.$gl_id_field).cmp(&(other.$gl_id_field)))
            }
        }

        impl<$t: $constraint> Ord for $struct_name<$t> {
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

/// Describes the vertex layout and offsets
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VertexLayout {
    pub fields: Vec<VertexAttribute>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VertexAttribute {
    /// Attribute name of the vertex attribute in the vertex shader, i.e. `"vAttrXY"`
    pub name: &'static str,
    /// If the vertex shader has a specific location, (like `layout(location = 2) vAttrXY`),
    /// use this instead of the name to look up the uniform location.
    pub layout_location: Option<usize>,
    /// Type of items of this attribute (i.e. for a `FloatVec2`, would be `VertexAttributeType::Float`)
    pub attribute_type: VertexAttributeType,
    /// Size of a *single* item (i.e. for a `FloatVec2`, would be `mem::size_of::<f32>()`)
    pub item_size: usize,
    /// Number of items of this attribute (i.e. for a `FloatVec2`, would be `2` (= 2 consecutive f32 values))
    pub item_count: usize,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum VertexAttributeType {
    /// Vertex attribute has type `f32`
    Float,
    /// Vertex attribute has type `f64`
    Double,
    /// Vertex attribute has type `u8`
    UnsignedByte,
    /// Vertex attribute has type `u16`
    UnsignedShort,
    /// Vertex attribute has type `u32`
    UnsignedInt,
}

impl VertexAttributeType {

    /// Returns the OpenGL id for the vertex attribute type, ex. `gl::UNSIGNED_BYTE` for `VertexAttributeType::UnsignedByte`.
    pub fn get_gl_id(&self) -> GLuint {
        use self::VertexAttributeType::*;
        match self {
            Float => gl::FLOAT,
            Double => gl::DOUBLE,
            UnsignedByte => gl::UNSIGNED_BYTE,
            UnsignedShort => gl::UNSIGNED_SHORT,
            UnsignedInt => gl::UNSIGNED_INT,
        }
    }
}

pub trait VertexLayoutDescription {
    fn get_description() -> VertexLayout;
}

pub struct VertexBuffer<T: VertexLayoutDescription> {
    pub vertex_buffer_id: GLuint,
    pub vertex_buffer_len: usize,
    pub vertex_layout: VertexLayout,
    pub gl_context: Rc<Gl>,
    pub vertex_buffer_type: PhantomData<T>,
}

impl<T: VertexLayoutDescription> ::std::fmt::Display for VertexBuffer<T> {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f,
            "VertexBuffer {{ buffer: {} (length: {}), layout: {:#?} }})",
            self.vertex_buffer_id, self.vertex_buffer_len, self.vertex_layout
        )
    }
}

impl_traits_for_gl_object!(VertexBuffer<T: VertexLayoutDescription>, vertex_buffer_id);

impl<T: VertexLayoutDescription> Drop for VertexBuffer<T> {
    fn drop(&mut self) {
        self.gl_context.delete_buffers(&[self.vertex_buffer_id]);
    }
}

impl<T: VertexLayoutDescription> VertexBuffer<T> {

    pub fn new(gl_context: Rc<Gl>, vertices: &[T]) -> Self {

        use std::mem;

        let vertex_layout = T::get_description();

        let vertex_buffer_id = gl_context.gen_buffers(1);
        let vertex_buffer_id = vertex_buffer_id[0];

        // Upload vertex data to GPU
        gl_context.bind_buffer(gl::ARRAY_BUFFER, 0);
        gl_context.buffer_data_untyped(
            gl::ARRAY_BUFFER,
            (mem::size_of::<T>() * vertices.len()) as isize,
            vertices.as_ptr() as *const c_void,
            gl::STATIC_DRAW
        );
        gl_context.bind_buffer(gl::ARRAY_BUFFER, 0);

        Self {
            vertex_buffer_id,
            vertex_buffer_len: vertices.len(),
            vertex_layout,
            gl_context,
            vertex_buffer_type: PhantomData,
        }
    }

    pub fn empty(gl_context: Rc<Gl>) -> Self {
        Self::new(gl_context, &[])
    }

    /// Submits the vertex buffer description to OpenGL
    pub fn bind(&mut self, shader: &GlShader) {

        const VERTICES_ARE_NORMALIZED: bool = false;

        let gl_context = &*self.gl_context;

        let mut offset = 0;

        for vertex_attribute in self.vertex_layout.fields.iter() {
            let attribute_location = vertex_attribute.layout_location
                .map(|ll| ll as i32)
                .unwrap_or_else(|| gl_context.get_attrib_location(shader.program_id, &vertex_attribute.name));
            let stride = vertex_attribute.item_size * vertex_attribute.item_count;
            gl_context.vertex_attrib_pointer(
                attribute_location as u32,
                vertex_attribute.item_count as i32,
                vertex_attribute.attribute_type.get_gl_id(),
                VERTICES_ARE_NORMALIZED,
                stride as i32,
                offset as u32,
            );
            gl_context.enable_vertex_attrib_array(attribute_location as u32);
            offset += stride;
        }
    }

    /// Unsets the vertex buffer description
    pub fn unbind(&mut self, shader: &GlShader) {
        let gl_context = &*self.gl_context;
        for vertex_attribute in self.vertex_layout.fields.iter() {
            let attribute_location = vertex_attribute.layout_location
                .map(|ll| ll as i32)
                .unwrap_or_else(|| gl_context.get_attrib_location(shader.program_id, &vertex_attribute.name));
            gl_context.disable_vertex_attrib_array(attribute_location as u32);
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GlApiVersion {
    Gl { major: usize, minor: usize },
    GlEs { major: usize, minor: usize },
}

impl GlApiVersion {
    /// Returns the OpenGL version of the context
    pub fn get(gl_context: &Gl) -> Self {
        let mut major = [0];
        unsafe { gl_context.get_integer_v(gl::MAJOR_VERSION, &mut major) };
        let mut minor = [0];
        unsafe { gl_context.get_integer_v(gl::MINOR_VERSION, &mut minor) };

        GlApiVersion::Gl { major: major[0] as usize, minor: minor[0] as usize }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IndexBufferFormat {
    Points,
    Lines,
    LineStrip,
    Triangles,
    TriangleStrip,
    TriangleFan,
}

impl IndexBufferFormat {
    /// Returns the `gl::TRIANGLE_STRIP` / `gl::POINTS`, etc.
    pub fn get_gl_id(&self) -> GLuint {
        use self::IndexBufferFormat::*;
        match self {
            Points => gl::POINTS,
            Lines => gl::LINES,
            LineStrip => gl::LINE_STRIP,
            Triangles => gl::TRIANGLES,
            TriangleStrip => gl::TRIANGLE_STRIP,
            TriangleFan => gl::TRIANGLE_FAN,
        }
    }
}

pub struct IndexBuffer {
    pub index_buffer_id: GLuint,
    pub index_buffer_len: usize,
    pub index_buffer_format: IndexBufferFormat,
    pub gl_context: Rc<Gl>,
}

impl IndexBuffer {

    pub fn new(gl_context: Rc<Gl>, indices: &[u32], format: IndexBufferFormat) -> Self {
        use std::mem;

        let index_buffer_id = gl_context.gen_buffers(1);
        let index_buffer_id = index_buffer_id[0];

        gl_context.bind_buffer(gl::ELEMENT_ARRAY_BUFFER, index_buffer_id);
        gl_context.buffer_data_untyped(
            gl::ELEMENT_ARRAY_BUFFER,
            (mem::size_of::<u32>() * indices.len()) as isize,
            indices.as_ptr() as *const c_void,
            gl::STATIC_DRAW
        );
        gl_context.bind_buffer(gl::ELEMENT_ARRAY_BUFFER, 0);

        Self {
            index_buffer_id,
            index_buffer_len: indices.len(),
            index_buffer_format: format,
            gl_context,
        }
    }

    /// Creates an empty `IndexBuffer` with a `gl::TRIANGLES` format.
    pub fn empty(gl_context: Rc<Gl>) -> Self {
        Self::new(gl_context, &[], IndexBufferFormat::Triangles)
    }
}

impl ::std::fmt::Display for IndexBuffer {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "IndexBuffer {{ id: {}, length: {} }}", self.index_buffer_id, self.index_buffer_len)
    }
}

impl Drop for IndexBuffer {
    fn drop(&mut self) {
        self.gl_context.delete_buffers(&[self.index_buffer_id]);
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Uniform {
    pub name: String,
    pub uniform_type: UniformType,
}

impl Uniform {

    pub fn new<S: Into<String>>(name: S, uniform_type: UniformType) -> Self {
        Self { name: name.into(), uniform_type }
    }

    /// Calls `glGetUniformLocation` and then `glUniform4f` with the uniform value (depending on the type of the uniform)
    pub fn bind(&self, shader: &GlShader) {
        let gl_context = &*shader.gl_context;
        let uniform_location = gl_context.get_uniform_location(shader.program_id, &self.name);
        self.uniform_type.set(gl_context, uniform_location);
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub enum UniformType {
    Float(f32),
    FloatVec2([f32;2]),
    FloatVec3([f32;3]),
    FloatVec4([f32;4]),
    Int(i32),
    IntVec2([i32;2]),
    IntVec3([i32;3]),
    IntVec4([i32;4]),
    UnsignedInt(u32),
    UnsignedIntVec2([u32;2]),
    UnsignedIntVec3([u32;3]),
    UnsignedIntVec4([u32;4]),
    Matrix2 { transpose: bool, matrix: [f32;2*2] },
    Matrix3 { transpose: bool, matrix: [f32;3*3] },
    Matrix4 { transpose: bool, matrix: [f32;4*4] },
}

impl UniformType {
    /// Set a specific uniform
    pub fn set(self, gl_context: &Gl, location: GLint) {
        use self::UniformType::*;
        match self {
            Float(r) => gl_context.uniform_1f(location, r),
            FloatVec2([r,g]) => gl_context.uniform_2f(location, r, g),
            FloatVec3([r,g,b]) => gl_context.uniform_3f(location, r, g, b),
            FloatVec4([r,g,b,a]) => gl_context.uniform_4f(location, r, g, b, a),
            Int(r) => gl_context.uniform_1i(location, r),
            IntVec2([r,g]) => gl_context.uniform_2i(location, r, g),
            IntVec3([r,g,b]) => gl_context.uniform_3i(location, r, g, b),
            IntVec4([r,g,b,a]) => gl_context.uniform_4i(location, r, g, b, a),
            UnsignedInt(r) => gl_context.uniform_1ui(location, r),
            UnsignedIntVec2([r,g]) => gl_context.uniform_2ui(location, r, g),
            UnsignedIntVec3([r,g,b]) => gl_context.uniform_3ui(location, r, g, b),
            UnsignedIntVec4([r,g,b,a]) => gl_context.uniform_4ui(location, r, g, b, a),
            Matrix2 { transpose, matrix } => gl_context.uniform_matrix_2fv(location, transpose, &matrix[..]),
            Matrix3 { transpose, matrix } => gl_context.uniform_matrix_2fv(location, transpose, &matrix[..]),
            Matrix4 { transpose, matrix } => gl_context.uniform_matrix_2fv(location, transpose, &matrix[..]),
        }
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

impl<'a> FrameBuffer<'a> {

    pub fn new(texture: &'a mut Texture) -> Self {
        let framebuffers = texture.gl_context.gen_framebuffers(1);

        // Set "textures[0]" as the color attachement #0
        texture.gl_context.framebuffer_texture_2d(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, texture.texture_id, 0);

        // Check that the framebuffer is complete
        debug_assert!(texture.gl_context.check_frame_buffer_status(gl::FRAMEBUFFER) == gl::FRAMEBUFFER_COMPLETE);

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

    pub fn unbind(&mut self) {
        self.texture.gl_context.bind_texture(gl::TEXTURE_2D, 0);
        self.texture.gl_context.bind_framebuffer(gl::FRAMEBUFFER, 0);
    }

    /// Calls the destructor for this framebuffer and deletes the framebuffer
    pub fn finish(self) { }
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
        write!(f, "GlShader {{ program_id: {} }}", self.program_id)
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

#[derive(Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum GlShaderCompileError {
    Vertex(VertexShaderCompileError),
    Fragment(FragmentShaderCompileError),
}

impl ::std::fmt::Display for GlShaderCompileError {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        use self::GlShaderCompileError::*;
        match self {
            Vertex(vert_err) => write!(f, "Failed to compile vertex shader: {}", vert_err),
            Fragment(frag_err) => write!(f, "Failed to compile fragment shader: {}", frag_err),
        }
    }
}

impl ::std::fmt::Debug for GlShaderCompileError {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "{}", self)
    }
}

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

#[derive(Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum GlShaderCreateError {
    Compile(GlShaderCompileError),
    Link(GlShaderLinkError),
    NoShaderCompiler,
}

impl ::std::fmt::Display for GlShaderCreateError {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        use self::GlShaderCreateError::*;
        match self {
            Compile(compile_err) => write!(f, "Shader compile error: {}", compile_err),
            Link(link_err) => write!(f, "Shader linking error: {}", link_err),
            NoShaderCompiler => write!(f, "OpenGL implementation doesn't include a shader compiler"),
        }
    }
}

impl ::std::fmt::Debug for GlShaderCreateError {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl GlShader {

    /// Compiles and creates a new OpenGL shader, created from a vertex and a fragment shader string.
    ///
    /// If the shader fails to compile, the shader object gets automatically deleted, no cleanup necessary.
    pub fn new(gl_context: Rc<Gl>, vertex_shader_source: &str, fragment_shader_source: &str)
    -> Result<Self, GlShaderCreateError>
    {
        // Check whether the OpenGL implementation supports a shader compiler...
        let mut shader_compiler_supported = [gl::FALSE];
        unsafe { gl_context.get_boolean_v(gl::SHADER_COMPILER, &mut shader_compiler_supported) };
        if shader_compiler_supported[0] == gl::FALSE {
            // Implementation only supports binary shaders
            return Err(GlShaderCreateError::NoShaderCompiler);
        }

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

    /// Binds the appropriate vertex buffer
    ///
    /// **NOTE: `FrameBuffer::bind()` and `VertexBuffer::bind()` have to be called first!**
    pub fn draw<T>(&mut self, fb: &mut FrameBuffer, vertices: &VertexBuffer<T>, indices: &IndexBuffer, uniforms: &[Uniform])
        where T: VertexLayoutDescription
    {
        const INDEX_TYPE: GLuint = gl::UNSIGNED_INT; // since indices are in u32 format

        let gl_context = &*fb.texture.gl_context;

        gl_context.bind_buffer(gl::ARRAY_BUFFER, vertices.vertex_buffer_id);
        gl_context.bind_buffer(gl::ELEMENT_ARRAY_BUFFER, vertices.vertex_buffer_id);
        gl_context.use_program(self.program_id);

        for uniform in uniforms {
            uniform.bind(&self);
        }

        gl_context.draw_elements(indices.index_buffer_format.get_gl_id(), indices.index_buffer_len as i32, INDEX_TYPE, 0);

        gl_context.use_program(0);
        gl_context.bind_buffer(gl::ELEMENT_ARRAY_BUFFER, 0);
        gl_context.bind_buffer(gl::ARRAY_BUFFER, 0);
    }
}

#[cfg(debug_assertions)]
fn get_gl_shader_error(context: &Gl, shader_object: GLuint) -> Option<i32> {
    let mut err = [0];
    unsafe { context.get_shader_iv(shader_object, gl::COMPILE_STATUS, &mut err) };
    let err_code = err[0];
    if err_code == gl::TRUE as i32 { None } else { Some(err_code) }
}

#[cfg(debug_assertions)]
fn get_gl_program_error(context: &Gl, shader_object: GLuint) -> Option<i32> {
    let mut err = [0];
    unsafe { context.get_program_iv(shader_object, gl::LINK_STATUS, &mut err) };
    let err_code = err[0];
    if err_code == gl::TRUE as i32 { None } else { Some(err_code) }
}
