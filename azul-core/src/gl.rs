#![allow(unused_variables)]
use std::{
    fmt,
    rc::Rc,
    hash::{Hasher, Hash},
    ffi::c_void,
    marker::PhantomData,
    os::raw::c_int,
};
use gleam::gl::{self, Gl, GlType, DebugMessage};
use crate::{
    FastHashMap,
    window::LogicalSize,
    app_resources::{Epoch, ExternalImageId},
    callbacks::PipelineId,
};
use azul_css::{AzString, StringVec, U8Vec, ColorU, ColorF};

/// Typedef for an OpenGL handle
pub type GLuint = u32;
pub type GLint = i32;
pub type GLint64 = i64;
pub type GLuint64 = u64;
pub type GLenum = u32;
pub type GLintptr = isize;
pub type GLboolean = u8;
pub type GLsizeiptr = isize;
pub type GLvoid = c_void;
pub type GLbitfield = u32;
pub type GLsizei = i32;
pub type GLclampf = f32;
pub type GLfloat = f32;
pub type GLeglImageOES = *const c_void;

// &str
#[repr(C)]
pub struct Refstr {
    pub ptr: *const u8,
    pub len: usize,
}

impl Refstr {
    fn as_str(&self) -> &str { unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.ptr, self.len)) } }
}

impl From<&str> for Refstr {
    fn from(s: &str) -> Self {
        Self { ptr: s.as_ptr(), len: s.len() }
    }
}

// &[&str]
#[repr(C)]
pub struct RefstrVecRef {
    pub ptr: *const Refstr,
    pub len: usize,
}

impl RefstrVecRef {
    fn as_slice(&self) -> &[Refstr] { unsafe { std::slice::from_raw_parts(self.ptr, self.len) } }
}

impl From<&[Refstr]> for RefstrVecRef {
    fn from(s: &[Refstr]) -> Self {
        Self { ptr: s.as_ptr(), len: s.len() }
    }
}

// &mut [GLint64]
#[repr(C)]
pub struct GLint64VecRefMut {
    pub ptr: *mut i64,
    pub len: usize,
}

impl From<&mut [GLint64]> for GLint64VecRefMut {
    fn from(s: &mut [GLint64]) -> Self {
        Self { ptr: s.as_mut_ptr(), len: s.len() }
    }
}

impl GLint64VecRefMut {
    fn as_mut_slice(&mut self) -> &mut [GLint64] { unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) } }
}

// &mut [GLfloat]
#[repr(C)]
pub struct GLfloatVecRefMut {
    pub ptr: *mut f32,
    pub len: usize,
}

impl From<&mut [GLfloat]> for GLfloatVecRefMut {
    fn from(s: &mut [GLfloat]) -> Self {
        Self { ptr: s.as_mut_ptr(), len: s.len() }
    }
}

impl GLfloatVecRefMut {
    fn as_mut_slice(&mut self) -> &mut [GLfloat] { unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) } }
}

// &mut [GLint]
#[repr(C)]
pub struct GLintVecRefMut {
    pub ptr: *mut i32,
    pub len: usize,
}

impl From<&mut [GLint]> for GLintVecRefMut {
    fn from(s: &mut [GLint]) -> Self {
        Self { ptr: s.as_mut_ptr(), len: s.len() }
    }
}

impl GLintVecRefMut {
    fn as_mut_slice(&mut self) -> &mut [GLint] { unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) } }
}

// &[GLuint]
#[repr(C)]
pub struct GLuintVecRef {
    pub ptr: *const u32,
    pub len: usize,
}

impl From<&[GLuint]> for GLuintVecRef {
    fn from(s: &[GLuint]) -> Self {
        Self { ptr: s.as_ptr(), len: s.len() }
    }
}

impl GLuintVecRef {
    fn as_slice(&self) -> &[GLuint] { unsafe { std::slice::from_raw_parts(self.ptr, self.len) } }
}

// &[GLenum]
#[repr(C)]
pub struct GLenumVecRef {
    pub ptr: *const u32,
    pub len: usize,
}

impl From<&[GLenum]> for GLenumVecRef {
    fn from(s: &[GLenum]) -> Self {
        Self { ptr: s.as_ptr(), len: s.len() }
    }
}

impl GLenumVecRef {
    fn as_slice(&self) -> &[GLenum] { unsafe { std::slice::from_raw_parts(self.ptr, self.len) } }
}


// &[u8]
#[repr(C)]
pub struct U8VecRef {
    pub ptr: *const u8,
    pub len: usize,
}

impl From<&[u8]> for U8VecRef {
    fn from(s: &[u8]) -> Self {
        Self { ptr: s.as_ptr(), len: s.len() }
    }
}

impl U8VecRef {
    fn as_slice(&self) -> &[u8] { unsafe { std::slice::from_raw_parts(self.ptr, self.len) } }
}

impl fmt::Debug for U8VecRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.as_slice().fmt(f)
    }
}

impl PartialOrd for U8VecRef {
    fn partial_cmp(&self, rhs: &Self) -> Option<std::cmp::Ordering> {
        self.as_slice().partial_cmp(rhs.as_slice())
    }
}

impl Ord for U8VecRef {
    fn cmp(&self, rhs: &Self) -> std::cmp::Ordering {
        self.as_slice().cmp(rhs.as_slice())
    }
}

impl PartialEq for U8VecRef {
    fn eq(&self, rhs: &Self) -> bool {
        self.as_slice().eq(rhs.as_slice())
    }
}

impl Eq for U8VecRef { }

impl std::hash::Hash for U8VecRef {
    fn hash<H>(&self, state: &mut H) where H: std::hash::Hasher {
        self.as_slice().hash(state)
    }
}

// &[f32]
#[repr(C)]
pub struct F32VecRef {
    pub ptr: *const f32,
    pub len: usize,
}

impl From<&[f32]> for F32VecRef {
    fn from(s: &[f32]) -> Self {
        Self { ptr: s.as_ptr(), len: s.len() }
    }
}

impl F32VecRef {
    fn as_slice(&self) -> &[f32] { unsafe { std::slice::from_raw_parts(self.ptr, self.len) } }
}

// &[i32]
#[repr(C)]
pub struct I32VecRef {
    pub ptr: *const i32,
    pub len: usize,
}

impl From<&[i32]> for I32VecRef {
    fn from(s: &[i32]) -> Self {
        Self { ptr: s.as_ptr(), len: s.len() }
    }
}

impl I32VecRef {
    fn as_slice(&self) -> &[i32] { unsafe { std::slice::from_raw_parts(self.ptr, self.len) } }
}

// &mut [u8]
#[repr(C)]
pub struct GLbooleanVecRefMut {
    pub ptr: *mut u8,
    pub len: usize,
}

impl From<&mut [GLboolean]> for GLbooleanVecRefMut {
    fn from(s: &mut [GLboolean]) -> Self {
        Self { ptr: s.as_mut_ptr(), len: s.len() }
    }
}

impl GLbooleanVecRefMut {
    fn as_mut_slice(&mut self) -> &mut [GLboolean] { unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) } }
}

// &mut [u8]
#[repr(C)]
pub struct U8VecRefMut {
    pub ptr: *mut u8,
    pub len: usize,
}

impl From<&mut [u8]> for U8VecRefMut {
    fn from(s: &mut [u8]) -> Self {
        Self { ptr: s.as_mut_ptr(), len: s.len() }
    }
}

impl U8VecRefMut {
    fn as_mut_slice(&mut self) -> &mut [u8] { unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) } }
}

impl_option!(U8VecRef, OptionU8VecRef, copy = false, clone = false, [Debug, PartialEq, Eq, PartialOrd, Ord, Hash]);

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct AzDebugMessage {
    pub message: AzString,
    pub source: GLenum,
    pub ty: GLenum,
    pub id: GLenum,
    pub severity: GLenum,
}

impl_vec!(AzDebugMessage, AzDebugMessageVec);
impl_vec!(GLint, GLintVec);
impl_vec!(GLuint, GLuintVec);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum AzGlType {
    Gl,
    Gles,
}

#[cfg(feature = "opengl")]
impl From<GlType> for AzGlType {
    fn from(a: GlType) -> AzGlType {
        match a {
            GlType::Gl => AzGlType::Gl,
            GlType::Gles => AzGlType::Gles,
        }
    }
}

// (U8Vec, u32)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct GetProgramBinaryReturn {
    pub _0: U8Vec,
    pub _1: u32,
}

// (i32, u32, AzString)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct GetActiveAttribReturn {
    pub _0: i32,
    pub _1: u32,
    pub _2: AzString,
}

// (i32, u32, AzString)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct GetActiveUniformReturn {
    pub _0: i32,
    pub _1: u32,
    pub _2: AzString,
}

#[repr(C)]
pub struct GLsyncPtr {
    pub ptr: *const c_void, /* *const __GLsync */
}

impl GLsyncPtr {
    pub fn new(p: gleam::gl::GLsync) -> Self { Self { ptr: p as *const c_void } }
    pub fn get(self) -> gleam::gl::GLsync { self.ptr as gleam::gl::GLsync }
}

/// Each pipeline (window) has its own OpenGL textures. GL Textures can technically
/// be shared across pipelines, however this turns out to be very difficult in practice.
pub(crate) type GlTextureStorage = FastHashMap<Epoch, FastHashMap<ExternalImageId, Texture>>;

/// Non-cleaned up textures. When a GlTexture is registered, it has to stay active as long
/// as WebRender needs it for drawing. To transparently do this, we store the epoch that the
/// texture was originally created with, and check, **after we have drawn the frame**,
/// if there are any textures that need cleanup.
///
/// Because the Texture2d is wrapped in an Rc, the destructor (which cleans up the OpenGL
/// texture) does not run until we remove the textures
///
/// Note: Because textures could be used after the current draw call (ex. for scrolling),
/// the ACTIVE_GL_TEXTURES are indexed by their epoch. Use `renderer.flush_pipeline_info()`
/// to see which textures are still active and which ones can be safely removed.
///
/// See: https://github.com/servo/webrender/issues/2940
///
/// WARNING: Not thread-safe (however, the Texture itself is thread-unsafe, so it's unlikely to ever be misused)
static mut ACTIVE_GL_TEXTURES: Option<FastHashMap<PipelineId, GlTextureStorage>> = None;

/// Inserts a new texture into the OpenGL texture cache, returns a new image ID
/// for the inserted texture
///
/// This function exists so azul doesn't have to use `lazy_static` as a dependency
pub fn insert_into_active_gl_textures(pipeline_id: PipelineId, epoch: Epoch, texture: Texture) -> ExternalImageId {

    let external_image_id = ExternalImageId::new();

    unsafe {
        if ACTIVE_GL_TEXTURES.is_none() {
            ACTIVE_GL_TEXTURES = Some(FastHashMap::new());
        }
        let active_textures = ACTIVE_GL_TEXTURES.as_mut().unwrap();
        let active_epochs = active_textures.entry(pipeline_id).or_insert_with(|| FastHashMap::new());
        let active_textures_for_epoch = active_epochs.entry(epoch).or_insert_with(|| FastHashMap::new());
        active_textures_for_epoch.insert(external_image_id, texture);
    }

    external_image_id
}

// Search all epoch hash maps for the given key
// There does not seem to be a way to get the epoch for the key,
// so we simply have to search all active epochs
//
// NOTE: Invalid textures can be generated on minimize / maximize
// Luckily, webrender simply ignores an invalid texture, so we don't
// need to check whether a window is maximized or minimized - if
// we encounter an invalid ID, webrender simply won't draw anything,
// but at least it won't crash. Usually invalid textures are also 0x0
// pixels large - so it's not like we had anything to draw anyway.
pub fn get_opengl_texture(image_key: &ExternalImageId) -> Option<(GLuint, (f32, f32))> {
    let active_textures = unsafe { ACTIVE_GL_TEXTURES.as_ref()? };
    active_textures.values()
    .flat_map(|active_pipeline| active_pipeline.values())
    .find_map(|active_epoch| active_epoch.get(image_key))
    .map(|tex| (tex.texture_id, (tex.size.width as f32, tex.size.height as f32)))
}

pub fn gl_textures_remove_active_pipeline(pipeline_id: &PipelineId) {
    unsafe {
        let active_textures = match ACTIVE_GL_TEXTURES.as_mut() {
            Some(s) => s,
            None => return,
        };
        active_textures.remove(pipeline_id);
    }
}

/// Destroys all textures from the pipeline `pipeline_id` where the texture is
/// **older** than the given `epoch`.
pub fn gl_textures_remove_epochs_from_pipeline(pipeline_id: &PipelineId, epoch: Epoch) {
    // TODO: Handle overflow of Epochs correctly (low priority)
    unsafe {
        let active_textures = match ACTIVE_GL_TEXTURES.as_mut() {
            Some(s) => s,
            None => return,
        };
        let active_epochs = match active_textures.get_mut(pipeline_id) {
            Some(s) => s,
            None => return,
        };
        active_epochs.retain(|gl_texture_epoch, _| *gl_texture_epoch > epoch);
    }
}

/// Destroys all textures, usually done before destroying the OpenGL context
pub fn gl_textures_clear_opengl_cache() {
    unsafe { ACTIVE_GL_TEXTURES = None; }
}


/// Virtual OpenGL "driver", that simply stores all the OpenGL
/// calls and can replay them at a later stage.
///
/// This makes it easier to debug OpenGL calls, to
/// sandbox / analyze / optimize and replay them (so that they don't interfere)
/// with other rendering tasks
#[repr(C)]
pub struct VirtualGlDriver {
    // TODO: create a "virtual" driver that only stores and replays OpenGL calls
    // - the VirtualGlDriver doesn't actually do anything, except store the OpenGL calls
    // and the replay them at a later date.
}

impl VirtualGlDriver {
    pub fn new() -> Self {
        Self { }
    }
}

impl Gl for VirtualGlDriver {
    fn get_type(&self) -> GlType {
        unimplemented()
    }

    fn buffer_data_untyped(&self, target: GLenum, size: GLsizeiptr, data: *const GLvoid, usage: GLenum) {
        unimplemented()
    }

    fn buffer_sub_data_untyped(&self, target: GLenum, offset: isize, size: GLsizeiptr, data: *const GLvoid) {
        unimplemented()
    }

    fn map_buffer(&self, target: GLenum, access: GLbitfield) -> *mut c_void {
        unimplemented()
    }

    fn map_buffer_range(&self, target: GLenum, offset: GLintptr, length: GLsizeiptr, access: GLbitfield) -> *mut c_void {
        unimplemented()
    }

    fn unmap_buffer(&self, target: GLenum) -> GLboolean {
        unimplemented()
    }

    fn tex_buffer(&self, target: GLenum, internal_format: GLenum, buffer: GLuint) {
        unimplemented()
    }

    fn shader_source(&self, shader: GLuint, strings: &[&[u8]]) {
        unimplemented()
    }

    fn read_buffer(&self, mode: GLenum) {
        unimplemented()
    }

    fn read_pixels_into_buffer(&self, x: GLint, y: GLint, width: GLsizei, height: GLsizei, format: GLenum, pixel_type: GLenum, dst_buffer: &mut [u8]) {
        unimplemented()
    }

    fn read_pixels(&self, x: GLint, y: GLint, width: GLsizei, height: GLsizei, format: GLenum, pixel_type: GLenum) -> Vec<u8> {
        unimplemented()
    }

    unsafe fn read_pixels_into_pbo(&self, x: GLint, y: GLint, width: GLsizei, height: GLsizei, format: GLenum, pixel_type: GLenum) {
        unimplemented()
    }

    fn sample_coverage(&self, value: GLclampf, invert: bool) {
        unimplemented()
    }

    fn polygon_offset(&self, factor: GLfloat, units: GLfloat) {
        unimplemented()
    }

    fn pixel_store_i(&self, name: GLenum, param: GLint) {
        unimplemented()
    }

    fn gen_buffers(&self, n: GLsizei) -> Vec<GLuint> {
        unimplemented()
    }

    fn gen_renderbuffers(&self, n: GLsizei) -> Vec<GLuint> {
        unimplemented()
    }

    fn gen_framebuffers(&self, n: GLsizei) -> Vec<GLuint> {
        unimplemented()
    }

    fn gen_textures(&self, n: GLsizei) -> Vec<GLuint> {
        unimplemented()
    }

    fn gen_vertex_arrays(&self, n: GLsizei) -> Vec<GLuint> {
        unimplemented()
    }

    fn gen_queries(&self, n: GLsizei) -> Vec<GLuint> {
        unimplemented()
    }

    fn begin_query(&self, target: GLenum, id: GLuint) {
        unimplemented()
    }

    fn end_query(&self, target: GLenum) {
        unimplemented()
    }

    fn query_counter(&self, id: GLuint, target: GLenum) {
        unimplemented()
    }

    fn get_query_object_iv(&self, id: GLuint, pname: GLenum) -> i32 {
        unimplemented()
    }

    fn get_query_object_uiv(&self, id: GLuint, pname: GLenum) -> u32 {
        unimplemented()
    }

    fn get_query_object_i64v(&self, id: GLuint, pname: GLenum) -> i64 {
        unimplemented()
    }

    fn get_query_object_ui64v(&self, id: GLuint, pname: GLenum) -> u64 {
        unimplemented()
    }

    fn delete_queries(&self, queries: &[GLuint]) {
        unimplemented()
    }

    fn delete_vertex_arrays(&self, vertex_arrays: &[GLuint]) {
        unimplemented()
    }

    fn delete_buffers(&self, buffers: &[GLuint]) {
        unimplemented()
    }

    fn delete_renderbuffers(&self, renderbuffers: &[GLuint]) {
        unimplemented()
    }

    fn delete_framebuffers(&self, framebuffers: &[GLuint]) {
        unimplemented()
    }

    fn delete_textures(&self, textures: &[GLuint]) {
        unimplemented()
    }

    fn framebuffer_renderbuffer(&self, target: GLenum, attachment: GLenum, renderbuffertarget: GLenum, renderbuffer: GLuint) {
        unimplemented()
    }

    fn renderbuffer_storage(&self, target: GLenum, internalformat: GLenum, width: GLsizei, height: GLsizei) {
        unimplemented()
    }

    fn depth_func(&self, func: GLenum) {
        unimplemented()
    }

    fn active_texture(&self, texture: GLenum) {
        unimplemented()
    }

    fn attach_shader(&self, program: GLuint, shader: GLuint) {
        unimplemented()
    }

    fn bind_attrib_location(&self, program: GLuint, index: GLuint, name: &str) {
        unimplemented()
    }

    unsafe fn get_uniform_iv(&self, program: GLuint, location: GLint, result: &mut [GLint]) {
        unimplemented()
    }

    unsafe fn get_uniform_fv(&self, program: GLuint, location: GLint, result: &mut [GLfloat]) {
        unimplemented()
    }

    fn get_uniform_block_index(&self, program: GLuint, name: &str) -> GLuint {
        unimplemented()
    }

    fn get_uniform_indices(&self,  program: GLuint, names: &[&str]) -> Vec<GLuint> {
        unimplemented()
    }

    fn bind_buffer_base(&self, target: GLenum, index: GLuint, buffer: GLuint) {
        unimplemented()
    }

    fn bind_buffer_range(&self, target: GLenum, index: GLuint, buffer: GLuint, offset: GLintptr, size: GLsizeiptr) {
        unimplemented()
    }

    fn uniform_block_binding(&self, program: GLuint, uniform_block_index: GLuint, uniform_block_binding: GLuint) {
        unimplemented()
    }

    fn bind_buffer(&self, target: GLenum, buffer: GLuint) {
        unimplemented()
    }

    fn bind_vertex_array(&self, vao: GLuint) {
        unimplemented()
    }

    fn bind_renderbuffer(&self, target: GLenum, renderbuffer: GLuint) {
        unimplemented()
    }

    fn bind_framebuffer(&self, target: GLenum, framebuffer: GLuint) {
        unimplemented()
    }

    fn bind_texture(&self, target: GLenum, texture: GLuint) {
        unimplemented()
    }

    fn draw_buffers(&self, bufs: &[GLenum]) {
        unimplemented()
    }

    fn tex_image_2d(&self, target: GLenum, level: GLint, internal_format: GLint, width: GLsizei, height: GLsizei, border: GLint, format: GLenum, ty: GLenum, opt_data: Option<&[u8]>) {
        unimplemented()
    }

    fn compressed_tex_image_2d(&self, target: GLenum, level: GLint, internal_format: GLenum, width: GLsizei, height: GLsizei, border: GLint, data: &[u8]) {
        unimplemented()
    }

    fn compressed_tex_sub_image_2d(&self, target: GLenum, level: GLint, xoffset: GLint, yoffset: GLint, width: GLsizei, height: GLsizei, format: GLenum, data: &[u8]) {
        unimplemented()
    }

    fn tex_image_3d(&self, target: GLenum, level: GLint, internal_format: GLint, width: GLsizei, height: GLsizei, depth: GLsizei, border: GLint, format: GLenum, ty: GLenum, opt_data: Option<&[u8]>) {
        unimplemented()
    }

    fn copy_tex_image_2d(&self, target: GLenum, level: GLint, internal_format: GLenum, x: GLint, y: GLint, width: GLsizei, height: GLsizei, border: GLint) {
        unimplemented()
    }

    fn copy_tex_sub_image_2d(&self, target: GLenum, level: GLint, xoffset: GLint, yoffset: GLint, x: GLint, y: GLint, width: GLsizei, height: GLsizei) {
        unimplemented()
    }

    fn copy_tex_sub_image_3d(&self, target: GLenum, level: GLint, xoffset: GLint, yoffset: GLint, zoffset: GLint, x: GLint, y: GLint, width: GLsizei, height: GLsizei) {
        unimplemented()
    }

    fn tex_sub_image_2d(&self, target: GLenum, level: GLint, xoffset: GLint, yoffset: GLint, width: GLsizei, height: GLsizei, format: GLenum, ty: GLenum, data: &[u8]) {
        unimplemented()
    }

    fn tex_sub_image_2d_pbo(&self, target: GLenum, level: GLint, xoffset: GLint, yoffset: GLint, width: GLsizei, height: GLsizei, format: GLenum, ty: GLenum, offset: usize) {
        unimplemented()
    }

    fn tex_sub_image_3d(&self, target: GLenum, level: GLint, xoffset: GLint, yoffset: GLint, zoffset: GLint, width: GLsizei, height: GLsizei, depth: GLsizei, format: GLenum, ty: GLenum, data: &[u8]) {
        unimplemented()
    }

    fn tex_sub_image_3d_pbo(&self, target: GLenum, level: GLint, xoffset: GLint, yoffset: GLint, zoffset: GLint, width: GLsizei, height: GLsizei, depth: GLsizei, format: GLenum, ty: GLenum, offset: usize) {
        unimplemented()
    }

    fn tex_storage_2d(&self, target: GLenum, levels: GLint, internal_format: GLenum, width: GLsizei, height: GLsizei) {
        unimplemented()
    }

    fn tex_storage_3d(&self, target: GLenum, levels: GLint, internal_format: GLenum, width: GLsizei, height: GLsizei, depth: GLsizei) {
        unimplemented()
    }

    fn get_tex_image_into_buffer(&self, target: GLenum, level: GLint, format: GLenum, ty: GLenum, output: &mut [u8]) {
        unimplemented()
    }

    unsafe fn copy_image_sub_data(&self, src_name: GLuint, src_target: GLenum, src_level: GLint, src_x: GLint, src_y: GLint, src_z: GLint, dst_name: GLuint, dst_target: GLenum, dst_level: GLint, dst_x: GLint, dst_y: GLint, dst_z: GLint, src_width: GLsizei, src_height: GLsizei, src_depth: GLsizei) {
        unimplemented()
    }

    fn invalidate_framebuffer(&self, target: GLenum, attachments: &[GLenum]) {
        unimplemented()
    }

    fn invalidate_sub_framebuffer(&self, target: GLenum, attachments: &[GLenum], xoffset: GLint, yoffset: GLint, width: GLsizei, height: GLsizei) {
        unimplemented()
    }

    unsafe fn get_integer_v(&self, name: GLenum, result: &mut [GLint]) {
        unimplemented()
    }

    unsafe fn get_integer_64v(&self, name: GLenum, result: &mut [GLint64]) {
        unimplemented()
    }

    unsafe fn get_integer_iv(&self, name: GLenum, index: GLuint, result: &mut [GLint]) {
        unimplemented()
    }

    unsafe fn get_integer_64iv(&self, name: GLenum, index: GLuint, result: &mut [GLint64]) {
        unimplemented()
    }

    unsafe fn get_boolean_v(&self, name: GLenum, result: &mut [GLboolean]) {
        unimplemented()
    }

    unsafe fn get_float_v(&self, name: GLenum, result: &mut [GLfloat]) {
        unimplemented()
    }

    fn get_framebuffer_attachment_parameter_iv(&self, target: GLenum, attachment: GLenum, pname: GLenum) -> GLint {
        unimplemented()
    }

    fn get_renderbuffer_parameter_iv(&self, target: GLenum, pname: GLenum) -> GLint {
        unimplemented()
    }

    fn get_tex_parameter_iv(&self, target: GLenum, name: GLenum) -> GLint {
        unimplemented()
    }

    fn get_tex_parameter_fv(&self, target: GLenum, name: GLenum) -> GLfloat {
        unimplemented()
    }

    fn tex_parameter_i(&self, target: GLenum, pname: GLenum, param: GLint) {
        unimplemented()
    }

    fn tex_parameter_f(&self, target: GLenum, pname: GLenum, param: GLfloat) {
        unimplemented()
    }

    fn framebuffer_texture_2d(&self, target: GLenum, attachment: GLenum, textarget: GLenum, texture: GLuint, level: GLint) {
        unimplemented()
    }

    fn framebuffer_texture_layer(&self, target: GLenum, attachment: GLenum, texture: GLuint, level: GLint, layer: GLint) {
        unimplemented()
    }

    fn blit_framebuffer(&self, src_x0: GLint, src_y0: GLint, src_x1: GLint, src_y1: GLint, dst_x0: GLint, dst_y0: GLint, dst_x1: GLint, dst_y1: GLint, mask: GLbitfield, filter: GLenum) {
        unimplemented()
    }

    fn vertex_attrib_4f(&self, index: GLuint, x: GLfloat, y: GLfloat, z: GLfloat, w: GLfloat) {
        unimplemented()
    }

    fn vertex_attrib_pointer_f32(&self, index: GLuint, size: GLint, normalized: bool, stride: GLsizei, offset: GLuint) {
        unimplemented()
    }

    fn vertex_attrib_pointer(&self, index: GLuint, size: GLint, type_: GLenum, normalized: bool, stride: GLsizei, offset: GLuint) {
        unimplemented()
    }

    fn vertex_attrib_i_pointer(&self, index: GLuint, size: GLint, type_: GLenum, stride: GLsizei, offset: GLuint) {
        unimplemented()
    }

    fn vertex_attrib_divisor(&self, index: GLuint, divisor: GLuint) {
        unimplemented()
    }

    fn viewport(&self, x: GLint, y: GLint, width: GLsizei, height: GLsizei) {
        unimplemented()
    }

    fn scissor(&self, x: GLint, y: GLint, width: GLsizei, height: GLsizei) {
        unimplemented()
    }

    fn line_width(&self, width: GLfloat) {
        unimplemented()
    }

    fn use_program(&self, program: GLuint) {
        unimplemented()
    }

    fn validate_program(&self, program: GLuint) {
        unimplemented()
    }

    fn draw_arrays(&self, mode: GLenum, first: GLint, count: GLsizei) {
        unimplemented()
    }

    fn draw_arrays_instanced(&self, mode: GLenum, first: GLint, count: GLsizei, primcount: GLsizei) {
        unimplemented()
    }

    fn draw_elements(&self, mode: GLenum, count: GLsizei, element_type: GLenum, indices_offset: GLuint) {
        unimplemented()
    }

    fn draw_elements_instanced(&self, mode: GLenum, count: GLsizei, element_type: GLenum, indices_offset: GLuint, primcount: GLsizei) {
        unimplemented()
    }

    fn blend_color(&self, r: f32, g: f32, b: f32, a: f32) {
        unimplemented()
    }

    fn blend_func(&self, sfactor: GLenum, dfactor: GLenum) {
        unimplemented()
    }

    fn blend_func_separate(&self, src_rgb: GLenum, dest_rgb: GLenum, src_alpha: GLenum, dest_alpha: GLenum) {
        unimplemented()
    }

    fn blend_equation(&self, mode: GLenum) {
        unimplemented()
    }

    fn blend_equation_separate(&self, mode_rgb: GLenum, mode_alpha: GLenum) {
        unimplemented()
    }

    fn color_mask(&self, r: bool, g: bool, b: bool, a: bool) {
        unimplemented()
    }

    fn cull_face(&self, mode: GLenum) {
        unimplemented()
    }

    fn front_face(&self, mode: GLenum) {
        unimplemented()
    }

    fn enable(&self, cap: GLenum) {
        unimplemented()
    }

    fn disable(&self, cap: GLenum) {
        unimplemented()
    }

    fn hint(&self, param_name: GLenum, param_val: GLenum) {
        unimplemented()
    }

    fn is_enabled(&self, cap: GLenum) -> GLboolean {
        unimplemented()
    }

    fn is_shader(&self, shader: GLuint) -> GLboolean {
        unimplemented()
    }

    fn is_texture(&self, texture: GLenum) -> GLboolean {
        unimplemented()
    }

    fn is_framebuffer(&self, framebuffer: GLenum) -> GLboolean {
        unimplemented()
    }

    fn is_renderbuffer(&self, renderbuffer: GLenum) -> GLboolean {
        unimplemented()
    }

    fn check_frame_buffer_status(&self, target: GLenum) -> GLenum {
        unimplemented()
    }

    fn enable_vertex_attrib_array(&self, index: GLuint) {
        unimplemented()
    }

    fn disable_vertex_attrib_array(&self, index: GLuint) {
        unimplemented()
    }

    fn uniform_1f(&self, location: GLint, v0: GLfloat) {
        unimplemented()
    }

    fn uniform_1fv(&self, location: GLint, values: &[f32]) {
        unimplemented()
    }

    fn uniform_1i(&self, location: GLint, v0: GLint) {
        unimplemented()
    }

    fn uniform_1iv(&self, location: GLint, values: &[i32]) {
        unimplemented()
    }

    fn uniform_1ui(&self, location: GLint, v0: GLuint) {
        unimplemented()
    }

    fn uniform_2f(&self, location: GLint, v0: GLfloat, v1: GLfloat) {
        unimplemented()
    }

    fn uniform_2fv(&self, location: GLint, values: &[f32]) {
        unimplemented()
    }

    fn uniform_2i(&self, location: GLint, v0: GLint, v1: GLint) {
        unimplemented()
    }

    fn uniform_2iv(&self, location: GLint, values: &[i32]) {
        unimplemented()
    }

    fn uniform_2ui(&self, location: GLint, v0: GLuint, v1: GLuint) {
        unimplemented()
    }

    fn uniform_3f(&self, location: GLint, v0: GLfloat, v1: GLfloat, v2: GLfloat) {
        unimplemented()
    }

    fn uniform_3fv(&self, location: GLint, values: &[f32]) {
        unimplemented()
    }

    fn uniform_3i(&self, location: GLint, v0: GLint, v1: GLint, v2: GLint) {
        unimplemented()
    }

    fn uniform_3iv(&self, location: GLint, values: &[i32]) {
        unimplemented()
    }

    fn uniform_3ui(&self, location: GLint, v0: GLuint, v1: GLuint, v2: GLuint) {
        unimplemented()
    }

    fn uniform_4f(&self, location: GLint, x: GLfloat, y: GLfloat, z: GLfloat, w: GLfloat) {
        unimplemented()
    }

    fn uniform_4i(&self, location: GLint, x: GLint, y: GLint, z: GLint, w: GLint) {
        unimplemented()
    }

    fn uniform_4iv(&self, location: GLint, values: &[i32]) {
        unimplemented()
    }

    fn uniform_4ui(&self, location: GLint, x: GLuint, y: GLuint, z: GLuint, w: GLuint) {
        unimplemented()
    }

    fn uniform_4fv(&self, location: GLint, values: &[f32]) {
        unimplemented()
    }

    fn uniform_matrix_2fv(&self, location: GLint, transpose: bool, value: &[f32]) {
        unimplemented()
    }

    fn uniform_matrix_3fv(&self, location: GLint, transpose: bool, value: &[f32]) {
        unimplemented()
    }

    fn uniform_matrix_4fv(&self, location: GLint, transpose: bool, value: &[f32]) {
        unimplemented()
    }

    fn depth_mask(&self, flag: bool) {
        unimplemented()
    }

    fn depth_range(&self, near: f64, far: f64) {
        unimplemented()
    }

    fn get_active_attrib(&self, program: GLuint, index: GLuint) -> (i32, u32, String) {
        unimplemented()
    }

    fn get_active_uniform(&self, program: GLuint, index: GLuint) -> (i32, u32, String) {
        unimplemented()
    }

    fn get_active_uniforms_iv(&self, program: GLuint, indices: Vec<GLuint>, pname: GLenum) -> Vec<GLint> {
        unimplemented()
    }

    fn get_active_uniform_block_i(&self, program: GLuint, index: GLuint, pname: GLenum) -> GLint {
        unimplemented()
    }

    fn get_active_uniform_block_iv(&self, program: GLuint, index: GLuint, pname: GLenum) -> Vec<GLint> {
        unimplemented()
    }

    fn get_active_uniform_block_name(&self, program: GLuint, index: GLuint) -> String {
        unimplemented()
    }

    fn get_attrib_location(&self, program: GLuint, name: &str) -> c_int {
        unimplemented()
    }

    fn get_frag_data_location(&self, program: GLuint, name: &str) -> c_int {
        unimplemented()
    }

    fn get_uniform_location(&self, program: GLuint, name: &str) -> c_int {
        unimplemented()
    }

    fn get_program_info_log(&self, program: GLuint) -> String {
        unimplemented()
    }

    unsafe fn get_program_iv(&self, program: GLuint, pname: GLenum, result: &mut [GLint]) {
        unimplemented()
    }

    fn get_program_binary(&self, program: GLuint) -> (Vec<u8>, GLenum) {
        unimplemented()
    }

    fn program_binary(&self, program: GLuint, format: GLenum, binary: &[u8]) {
        unimplemented()
    }

    fn program_parameter_i(&self, program: GLuint, pname: GLenum, value: GLint) {
        unimplemented()
    }

    unsafe fn get_vertex_attrib_iv(&self, index: GLuint, pname: GLenum, result: &mut [GLint]) {
        unimplemented()
    }

    unsafe fn get_vertex_attrib_fv(&self, index: GLuint, pname: GLenum, result: &mut [GLfloat]) {
        unimplemented()
    }

    fn get_vertex_attrib_pointer_v(&self, index: GLuint, pname: GLenum) -> GLsizeiptr {
        unimplemented()
    }

    fn get_buffer_parameter_iv(&self, target: GLuint, pname: GLenum) -> GLint {
        unimplemented()
    }

    fn get_shader_info_log(&self, shader: GLuint) -> String {
        unimplemented()
    }

    fn get_string(&self, which: GLenum) -> String {
        unimplemented()
    }

    fn get_string_i(&self, which: GLenum, index: GLuint) -> String {
        unimplemented()
    }

    unsafe fn get_shader_iv(&self, shader: GLuint, pname: GLenum, result: &mut [GLint]) {
        unimplemented()
    }

    fn get_shader_precision_format(&self, shader_type: GLuint, precision_type: GLuint) -> (GLint, GLint, GLint) {
        unimplemented()
    }

    fn compile_shader(&self, shader: GLuint) {
        unimplemented()
    }

    fn create_program(&self) -> GLuint {
        unimplemented()
    }

    fn delete_program(&self, program: GLuint) {
        unimplemented()
    }

    fn create_shader(&self, shader_type: GLenum) -> GLuint {
        unimplemented()
    }

    fn delete_shader(&self, shader: GLuint) {
        unimplemented()
    }

    fn detach_shader(&self, program: GLuint, shader: GLuint) {
        unimplemented()
    }

    fn link_program(&self, program: GLuint) {
        unimplemented()
    }

    fn clear_color(&self, r: f32, g: f32, b: f32, a: f32) {
        unimplemented()
    }

    fn clear(&self, buffer_mask: GLbitfield) {
        unimplemented()
    }

    fn clear_depth(&self, depth: f64) {
        unimplemented()
    }

    fn clear_stencil(&self, s: GLint) {
        unimplemented()
    }

    fn flush(&self) {
        unimplemented()
    }

    fn finish(&self) {
        unimplemented()
    }

    fn get_error(&self) -> GLenum {
        unimplemented()
    }

    fn stencil_mask(&self, mask: GLuint) {
        unimplemented()
    }

    fn stencil_mask_separate(&self, face: GLenum, mask: GLuint) {
        unimplemented()
    }

    fn stencil_func(&self, func: GLenum, ref_: GLint, mask: GLuint) {
        unimplemented()
    }

    fn stencil_func_separate(&self, face: GLenum, func: GLenum, ref_: GLint, mask: GLuint) {
        unimplemented()
    }

    fn stencil_op(&self, sfail: GLenum, dpfail: GLenum, dppass: GLenum) {
        unimplemented()
    }

    fn stencil_op_separate(&self, face: GLenum, sfail: GLenum, dpfail: GLenum, dppass: GLenum) {
        unimplemented()
    }

    fn egl_image_target_texture2d_oes(&self, target: GLenum, image: GLeglImageOES) {
        unimplemented()
    }

    fn generate_mipmap(&self, target: GLenum) {
        unimplemented()
    }

    fn insert_event_marker_ext(&self, message: &str) {
        unimplemented()
    }

    fn push_group_marker_ext(&self, message: &str) {
        unimplemented()
    }

    fn pop_group_marker_ext(&self) {
        unimplemented()
    }

    fn debug_message_insert_khr(&self, source: GLenum, type_: GLenum, id: GLuint, severity: GLenum, message: &str) {
        unimplemented()
    }

    fn push_debug_group_khr(&self, source: GLenum, id: GLuint, message: &str) {
        unimplemented()
    }

    fn pop_debug_group_khr(&self) {
        unimplemented()
    }

    fn fence_sync(&self, condition: GLenum, flags: GLbitfield) -> gleam::gl::GLsync {
        unimplemented()
    }

    fn client_wait_sync(&self, sync: gleam::gl::GLsync, flags: GLbitfield, timeout: GLuint64) {
        unimplemented()
    }

    fn wait_sync(&self, sync: gleam::gl::GLsync, flags: GLbitfield, timeout: GLuint64) {
        unimplemented()
    }

    fn delete_sync(&self, sync: gleam::gl::GLsync) {
        unimplemented()
    }

    fn texture_range_apple(&self, target: GLenum, data: &[u8]) {
        unimplemented()
    }

    fn gen_fences_apple(&self, n: GLsizei) -> Vec<GLuint> {
        unimplemented()
    }

    fn delete_fences_apple(&self, fences: &[GLuint]) {
        unimplemented()
    }

    fn set_fence_apple(&self, fence: GLuint) {
        unimplemented()
    }

    fn finish_fence_apple(&self, fence: GLuint) {
        unimplemented()
    }

    fn test_fence_apple(&self, fence: GLuint) {
        unimplemented()
    }

    fn test_object_apple(&self, object: GLenum, name: GLuint) -> GLboolean {
        unimplemented()
    }

    fn finish_object_apple(&self, object: GLenum, name: GLuint) {
        unimplemented()
    }

    fn get_frag_data_index( &self, program: GLuint, name: &str) -> GLint {
        unimplemented()
    }

    fn blend_barrier_khr(&self) {
        unimplemented()
    }

    fn bind_frag_data_location_indexed( &self, program: GLuint, color_number: GLuint, index: GLuint, name: &str) {
        unimplemented()
    }

    fn get_debug_messages(&self) -> Vec<DebugMessage> {
        unimplemented()
    }

    fn provoking_vertex_angle(&self, mode: GLenum) {
        unimplemented()
    }

    // -- gleam: 0.8

    fn gen_vertex_arrays_apple(&self, n: GLsizei) -> Vec<GLuint> {
        unimplemented()
    }

    fn bind_vertex_array_apple(&self, vao: GLuint) {
        unimplemented()
    }

    fn delete_vertex_arrays_apple(&self, vertex_arrays: &[GLuint]) {
        unimplemented()
    }

    fn copy_texture_chromium(&self, source_id: GLuint, source_level: GLint, dest_target: GLenum, dest_id: GLuint, dest_level: GLint, internal_format: GLint, dest_type: GLenum, unpack_flip_y: GLboolean, unpack_premultiply_alpha: GLboolean, unpack_unmultiply_alpha: GLboolean) {
        unimplemented()
    }

    fn copy_sub_texture_chromium(&self, source_id: GLuint, source_level: GLint, dest_target: GLenum, dest_id: GLuint, dest_level: GLint, x_offset: GLint, y_offset: GLint, x: GLint, y: GLint, width: GLsizei, height: GLsizei, unpack_flip_y: GLboolean, unpack_premultiply_alpha: GLboolean, unpack_unmultiply_alpha: GLboolean) {
        unimplemented()
    }

    // -- gleam: 0.11

    fn egl_image_target_renderbuffer_storage_oes(&self, target: u32, image: *const c_void) {
        unimplemented()
    }

    fn copy_texture_3d_angle( &self, source_id: GLuint, source_level: GLint, dest_target: GLenum, dest_id: GLuint, dest_level: GLint, internal_format: GLint, dest_type: GLenum, unpack_flip_y: GLboolean, unpack_premultiply_alpha: GLboolean, unpack_unmultiply_alpha: GLboolean) {
        unimplemented()
    }

    fn copy_sub_texture_3d_angle(&self, source_id: GLuint, source_level: GLint, dest_target: GLenum, dest_id: GLuint, dest_level: GLint, x_offset: GLint, y_offset: GLint, z_offset: GLint, x: GLint, y: GLint, z: GLint, width: GLsizei, height: GLsizei, depth: GLsizei, unpack_flip_y: GLboolean, unpack_premultiply_alpha: GLboolean, unpack_unmultiply_alpha: GLboolean) {
        unimplemented()
    }
}

fn unimplemented() -> ! {
    panic!("You cannot call OpenGL functions on the VirtualGlDriver");
}

#[cfg(feature = "opengl")]
#[repr(C)]
pub struct GlContextPtr { /* *const Rc<dyn Gl> */ pub ptr: *const c_void }

#[cfg(feature = "opengl")]
impl GlContextPtr {
    pub fn new(context: Rc<dyn Gl>) -> Self { Self { ptr: Box::into_raw(Box::new(context)) as *const c_void } }
    pub fn get<'a>(&'a self) -> &'a Rc<dyn Gl> { unsafe { &*(self.ptr as *const Rc<dyn Gl>) } }
}

#[cfg(feature = "opengl")]
impl GlContextPtr {
    pub fn get_type(&self) -> AzGlType { self.get().get_type().into() }
    pub fn buffer_data_untyped(&self, target: GLenum, size: GLsizeiptr, data: *const GLvoid, usage: GLenum) { self.get().buffer_data_untyped(target, size, data, usage, ) }
    pub fn buffer_sub_data_untyped(&self, target: GLenum, offset: isize, size: GLsizeiptr, data: *const GLvoid) { self.get().buffer_sub_data_untyped(target, offset, size, data) }
    pub fn map_buffer(&self, target: GLenum, access: GLbitfield) -> *mut c_void { self.get().map_buffer(target, access) }
    pub fn map_buffer_range(&self, target: GLenum, offset: GLintptr, length: GLsizeiptr, access: GLbitfield) -> *mut c_void { self.get().map_buffer_range(target, offset, length, access) }
    pub fn unmap_buffer(&self, target: GLenum) -> GLboolean { self.get().unmap_buffer(target) }
    pub fn tex_buffer(&self, target: GLenum, internal_format: GLenum, buffer: GLuint) { self.get().tex_buffer(target, internal_format, buffer) }
    pub fn shader_source(&self, shader: GLuint, strings: StringVec) {
        fn str_to_bytes(input: &str) -> Vec<u8> { let mut v: Vec<u8> = input.into(); v.push(0); v }
        let shaders_as_bytes = strings.into_iter().map(|s| str_to_bytes(s.as_str())).collect::<Vec<_>>();
        let shaders_as_bytes = shaders_as_bytes.iter().map(|s| s.as_ref()).collect::<Vec<_>>();
        self.get().shader_source(shader, &shaders_as_bytes)
    }
    pub fn read_buffer(&self, mode: GLenum) { self.get().read_buffer(mode) }
    pub fn read_pixels_into_buffer(&self, x: GLint, y: GLint, width: GLsizei, height: GLsizei, format: GLenum, pixel_type: GLenum, mut dst_buffer: U8VecRefMut) { self.get().read_pixels_into_buffer(x, y, width, height, format, pixel_type, dst_buffer.as_mut_slice()) }
    pub fn read_pixels(&self, x: GLint, y: GLint, width: GLsizei, height: GLsizei, format: GLenum, pixel_type: GLenum) -> U8Vec { self.get().read_pixels(x, y, width, height, format, pixel_type).into() }
    pub fn read_pixels_into_pbo(&self, x: GLint, y: GLint, width: GLsizei, height: GLsizei, format: GLenum, pixel_type: GLenum) { unsafe {  self.get().read_pixels_into_pbo(x, y, width, height, format, pixel_type)} }
    pub fn sample_coverage(&self, value: GLclampf, invert: bool) { self.get().sample_coverage(value, invert) }
    pub fn polygon_offset(&self, factor: GLfloat, units: GLfloat) { self.get().polygon_offset(factor, units) }
    pub fn pixel_store_i(&self, name: GLenum, param: GLint) { self.get().pixel_store_i(name, param) }
    pub fn gen_buffers(&self, n: GLsizei) -> GLuintVec { self.get().gen_buffers(n).into() }
    pub fn gen_renderbuffers(&self, n: GLsizei) -> GLuintVec { self.get().gen_renderbuffers(n).into() }
    pub fn gen_framebuffers(&self, n: GLsizei) -> GLuintVec { self.get().gen_framebuffers(n).into() }
    pub fn gen_textures(&self, n: GLsizei) -> GLuintVec { self.get().gen_textures(n).into() }
    pub fn gen_vertex_arrays(&self, n: GLsizei) -> GLuintVec { self.get().gen_vertex_arrays(n).into() }
    pub fn gen_queries(&self, n: GLsizei) -> GLuintVec { self.get().gen_queries(n).into() }
    pub fn begin_query(&self, target: GLenum, id: GLuint) { self.get().begin_query(target, id) }
    pub fn end_query(&self, target: GLenum) { self.get().end_query(target) }
    pub fn query_counter(&self, id: GLuint, target: GLenum) { self.get().query_counter(id, target) }
    pub fn get_query_object_iv(&self, id: GLuint, pname: GLenum) -> i32 { self.get().get_query_object_iv(id, pname) }
    pub fn get_query_object_uiv(&self, id: GLuint, pname: GLenum) -> u32 { self.get().get_query_object_uiv(id, pname) }
    pub fn get_query_object_i64v(&self, id: GLuint, pname: GLenum) -> i64 { self.get().get_query_object_i64v(id, pname) }
    pub fn get_query_object_ui64v(&self, id: GLuint, pname: GLenum) -> u64 { self.get().get_query_object_ui64v(id, pname) }
    pub fn delete_queries(&self, queries: GLuintVecRef) { self.get().delete_queries(queries.as_slice()) }
    pub fn delete_vertex_arrays(&self, vertex_arrays: GLuintVecRef) { self.get().delete_vertex_arrays(vertex_arrays.as_slice()) }
    pub fn delete_buffers(&self, buffers: GLuintVecRef) { self.get().delete_buffers(buffers.as_slice()) }
    pub fn delete_renderbuffers(&self, renderbuffers: GLuintVecRef) { self.get().delete_renderbuffers(renderbuffers.as_slice()) }
    pub fn delete_framebuffers(&self, framebuffers: GLuintVecRef) { self.get().delete_framebuffers(framebuffers.as_slice()) }
    pub fn delete_textures(&self, textures: GLuintVecRef) { self.get().delete_textures(textures.as_slice()) }
    pub fn framebuffer_renderbuffer(&self, target: GLenum, attachment: GLenum, renderbuffertarget: GLenum, renderbuffer: GLuint) { self.get().framebuffer_renderbuffer(target, attachment, renderbuffertarget, renderbuffer) }
    pub fn renderbuffer_storage(&self, target: GLenum, internalformat: GLenum, width: GLsizei, height: GLsizei) { self.get().renderbuffer_storage(target, internalformat, width, height) }
    pub fn depth_func(&self, func: GLenum) { self.get().depth_func(func) }
    pub fn active_texture(&self, texture: GLenum) { self.get().active_texture(texture) }
    pub fn attach_shader(&self, program: GLuint, shader: GLuint) { self.get().attach_shader(program, shader) }
    pub fn bind_attrib_location(&self, program: GLuint, index: GLuint, name: Refstr) { self.get().bind_attrib_location(program, index, name.as_str()) }
    pub fn get_uniform_iv(&self, program: GLuint, location: GLint, mut result: GLintVecRefMut) { unsafe {  self.get().get_uniform_iv(program, location, result.as_mut_slice())} }
    pub fn get_uniform_fv(&self, program: GLuint, location: GLint, mut result: GLfloatVecRefMut) { unsafe {  self.get().get_uniform_fv(program, location, result.as_mut_slice())} }
    pub fn get_uniform_block_index(&self, program: GLuint, name: Refstr) -> GLuint { self.get().get_uniform_block_index(program, name.as_str()) }
    pub fn get_uniform_indices(&self,  program: GLuint, names: RefstrVecRef) -> GLuintVec { let names_vec = names.as_slice().iter().map(|n| n.as_str()).collect::<Vec<_>>(); self.get().get_uniform_indices( program, &names_vec).into() }
    pub fn bind_buffer_base(&self, target: GLenum, index: GLuint, buffer: GLuint) { self.get().bind_buffer_base(target, index, buffer) }
    pub fn bind_buffer_range(&self, target: GLenum, index: GLuint, buffer: GLuint, offset: GLintptr, size: GLsizeiptr) { self.get().bind_buffer_range(target, index, buffer, offset, size) }
    pub fn uniform_block_binding(&self, program: GLuint, uniform_block_index: GLuint, uniform_block_binding: GLuint) { self.get().uniform_block_binding(program, uniform_block_index, uniform_block_binding) }
    pub fn bind_buffer(&self, target: GLenum, buffer: GLuint) { self.get().bind_buffer(target, buffer) }
    pub fn bind_vertex_array(&self, vao: GLuint) { self.get().bind_vertex_array(vao) }
    pub fn bind_renderbuffer(&self, target: GLenum, renderbuffer: GLuint) { self.get().bind_renderbuffer(target, renderbuffer) }
    pub fn bind_framebuffer(&self, target: GLenum, framebuffer: GLuint) { self.get().bind_framebuffer(target, framebuffer) }
    pub fn bind_texture(&self, target: GLenum, texture: GLuint) { self.get().bind_texture(target, texture) }
    pub fn draw_buffers(&self, bufs: GLenumVecRef) { self.get().draw_buffers(bufs.as_slice()) }
    pub fn tex_image_2d(&self, target: GLenum, level: GLint, internal_format: GLint, width: GLsizei, height: GLsizei, border: GLint, format: GLenum, ty: GLenum, opt_data: OptionU8VecRef) { let opt_data = opt_data.as_option(); let opt_data: Option<&[u8]> = opt_data.map(|o| o.as_slice()); self.get().tex_image_2d(target, level, internal_format, width, height, border, format, ty, opt_data) }
    pub fn compressed_tex_image_2d(&self, target: GLenum, level: GLint, internal_format: GLenum, width: GLsizei, height: GLsizei, border: GLint, data: U8VecRef) { self.get().compressed_tex_image_2d(target, level, internal_format, width, height, border, data.as_slice()) }
    pub fn compressed_tex_sub_image_2d(&self, target: GLenum, level: GLint, xoffset: GLint, yoffset: GLint, width: GLsizei, height: GLsizei, format: GLenum, data: U8VecRef) { self.get().compressed_tex_sub_image_2d(target, level, xoffset, yoffset, width, height, format, data.as_slice()) }
    pub fn tex_image_3d(&self, target: GLenum, level: GLint, internal_format: GLint, width: GLsizei, height: GLsizei, depth: GLsizei, border: GLint, format: GLenum, ty: GLenum, opt_data: OptionU8VecRef) { let opt_data = opt_data.as_option(); let opt_data: Option<&[u8]> = opt_data.map(|o| o.as_slice()); self.get().tex_image_3d(target, level, internal_format, width, height, depth, border, format, ty, opt_data) }
    pub fn copy_tex_image_2d(&self, target: GLenum, level: GLint, internal_format: GLenum, x: GLint, y: GLint, width: GLsizei, height: GLsizei, border: GLint) { self.get().copy_tex_image_2d(target, level, internal_format, x, y, width, height, border) }
    pub fn copy_tex_sub_image_2d(&self, target: GLenum, level: GLint, xoffset: GLint, yoffset: GLint, x: GLint, y: GLint, width: GLsizei, height: GLsizei) { self.get().copy_tex_sub_image_2d(target, level, xoffset, yoffset, x, y, width, height) }
    pub fn copy_tex_sub_image_3d(&self, target: GLenum, level: GLint, xoffset: GLint, yoffset: GLint, zoffset: GLint, x: GLint, y: GLint, width: GLsizei, height: GLsizei) { self.get().copy_tex_sub_image_3d(target, level, xoffset, yoffset, zoffset, x, y, width, height) }
    pub fn tex_sub_image_2d(&self, target: GLenum, level: GLint, xoffset: GLint, yoffset: GLint, width: GLsizei, height: GLsizei, format: GLenum, ty: GLenum, data: U8VecRef) { self.get().tex_sub_image_2d(target, level, xoffset, yoffset, width, height, format, ty, data.as_slice()) }
    pub fn tex_sub_image_2d_pbo(&self, target: GLenum, level: GLint, xoffset: GLint, yoffset: GLint, width: GLsizei, height: GLsizei, format: GLenum, ty: GLenum, offset: usize) { self.get().tex_sub_image_2d_pbo(target, level, xoffset, yoffset, width, height, format, ty, offset) }
    pub fn tex_sub_image_3d(&self, target: GLenum, level: GLint, xoffset: GLint, yoffset: GLint, zoffset: GLint, width: GLsizei, height: GLsizei, depth: GLsizei, format: GLenum, ty: GLenum, data: U8VecRef) { self.get().tex_sub_image_3d(target, level, xoffset, yoffset, zoffset, width, height, depth, format, ty, data.as_slice()) }
    pub fn tex_sub_image_3d_pbo(&self, target: GLenum, level: GLint, xoffset: GLint, yoffset: GLint, zoffset: GLint, width: GLsizei, height: GLsizei, depth: GLsizei, format: GLenum, ty: GLenum, offset: usize) { self.get().tex_sub_image_3d_pbo(target, level, xoffset, yoffset, zoffset, width, height, depth, format, ty, offset) }
    pub fn tex_storage_2d(&self, target: GLenum, levels: GLint, internal_format: GLenum, width: GLsizei, height: GLsizei) { self.get().tex_storage_2d(target, levels, internal_format, width, height) }
    pub fn tex_storage_3d(&self, target: GLenum, levels: GLint, internal_format: GLenum, width: GLsizei, height: GLsizei, depth: GLsizei) { self.get().tex_storage_3d(target, levels, internal_format, width, height, depth) }
    pub fn get_tex_image_into_buffer(&self, target: GLenum, level: GLint, format: GLenum, ty: GLenum, mut output: U8VecRefMut) { self.get().get_tex_image_into_buffer(target, level, format, ty, output.as_mut_slice()) }
    pub fn copy_image_sub_data(&self, src_name: GLuint, src_target: GLenum, src_level: GLint, src_x: GLint, src_y: GLint, src_z: GLint, dst_name: GLuint, dst_target: GLenum, dst_level: GLint, dst_x: GLint, dst_y: GLint, dst_z: GLint, src_width: GLsizei, src_height: GLsizei, src_depth: GLsizei) { unsafe {  self.get().copy_image_sub_data(src_name, src_target, src_level, src_x, src_y, src_z, dst_name, dst_target, dst_level, dst_x, dst_y, dst_z, src_width, src_height, src_depth)} }
    pub fn invalidate_framebuffer(&self, target: GLenum, attachments: GLenumVecRef) { self.get().invalidate_framebuffer(target, attachments.as_slice()) }
    pub fn invalidate_sub_framebuffer(&self, target: GLenum, attachments: GLenumVecRef, xoffset: GLint, yoffset: GLint, width: GLsizei, height: GLsizei) { self.get().invalidate_sub_framebuffer(target, attachments.as_slice(), xoffset, yoffset, width, height) }
    pub fn get_integer_v(&self, name: GLenum, mut result: GLintVecRefMut) { unsafe {  self.get().get_integer_v(name, result.as_mut_slice())} }
    pub fn get_integer_64v(&self, name: GLenum, mut result: GLint64VecRefMut) { unsafe {  self.get().get_integer_64v(name, result.as_mut_slice())} }
    pub fn get_integer_iv(&self, name: GLenum, index: GLuint, mut result: GLintVecRefMut) { unsafe {  self.get().get_integer_iv(name, index, result.as_mut_slice())} }
    pub fn get_integer_64iv(&self, name: GLenum, index: GLuint, mut result: GLint64VecRefMut) { unsafe {  self.get().get_integer_64iv(name, index, result.as_mut_slice())} }
    pub fn get_boolean_v(&self, name: GLenum, mut result: GLbooleanVecRefMut) { unsafe {  self.get().get_boolean_v(name, result.as_mut_slice())} }
    pub fn get_float_v(&self, name: GLenum, mut result: GLfloatVecRefMut) { unsafe {  self.get().get_float_v(name, result.as_mut_slice())} }
    pub fn get_framebuffer_attachment_parameter_iv(&self, target: GLenum, attachment: GLenum, pname: GLenum) -> GLint { self.get().get_framebuffer_attachment_parameter_iv(target, attachment, pname) }
    pub fn get_renderbuffer_parameter_iv(&self, target: GLenum, pname: GLenum) -> GLint { self.get().get_renderbuffer_parameter_iv(target, pname) }
    pub fn get_tex_parameter_iv(&self, target: GLenum, name: GLenum) -> GLint { self.get().get_tex_parameter_iv(target, name) }
    pub fn get_tex_parameter_fv(&self, target: GLenum, name: GLenum) -> GLfloat { self.get().get_tex_parameter_fv(target, name) }
    pub fn tex_parameter_i(&self, target: GLenum, pname: GLenum, param: GLint) { self.get().tex_parameter_i(target, pname, param) }
    pub fn tex_parameter_f(&self, target: GLenum, pname: GLenum, param: GLfloat) { self.get().tex_parameter_f(target, pname, param) }
    pub fn framebuffer_texture_2d(&self, target: GLenum, attachment: GLenum, textarget: GLenum, texture: GLuint, level: GLint) { self.get().framebuffer_texture_2d(target, attachment, textarget, texture, level) }
    pub fn framebuffer_texture_layer(&self, target: GLenum, attachment: GLenum, texture: GLuint, level: GLint, layer: GLint) { self.get().framebuffer_texture_layer(target, attachment, texture, level, layer) }
    pub fn blit_framebuffer(&self, src_x0: GLint, src_y0: GLint, src_x1: GLint, src_y1: GLint, dst_x0: GLint, dst_y0: GLint, dst_x1: GLint, dst_y1: GLint, mask: GLbitfield, filter: GLenum) { self.get().blit_framebuffer(src_x0, src_y0, src_x1, src_y1, dst_x0, dst_y0, dst_x1, dst_y1, mask, filter) }
    pub fn vertex_attrib_4f(&self, index: GLuint, x: GLfloat, y: GLfloat, z: GLfloat, w: GLfloat) { self.get().vertex_attrib_4f(index, x, y, z, w) }
    pub fn vertex_attrib_pointer_f32(&self, index: GLuint, size: GLint, normalized: bool, stride: GLsizei, offset: GLuint) { self.get().vertex_attrib_pointer_f32(index, size, normalized, stride, offset) }
    pub fn vertex_attrib_pointer(&self, index: GLuint, size: GLint, type_: GLenum, normalized: bool, stride: GLsizei, offset: GLuint) { self.get().vertex_attrib_pointer(index, size, type_, normalized, stride, offset) }
    pub fn vertex_attrib_i_pointer(&self, index: GLuint, size: GLint, type_: GLenum, stride: GLsizei, offset: GLuint) { self.get().vertex_attrib_i_pointer(index, size, type_, stride, offset) }
    pub fn vertex_attrib_divisor(&self, index: GLuint, divisor: GLuint) { self.get().vertex_attrib_divisor(index, divisor) }
    pub fn viewport(&self, x: GLint, y: GLint, width: GLsizei, height: GLsizei) { self.get().viewport(x, y, width, height) }
    pub fn scissor(&self, x: GLint, y: GLint, width: GLsizei, height: GLsizei) { self.get().scissor(x, y, width, height) }
    pub fn line_width(&self, width: GLfloat) { self.get().line_width(width) }
    pub fn use_program(&self, program: GLuint) { self.get().use_program(program) }
    pub fn validate_program(&self, program: GLuint) { self.get().validate_program(program) }
    pub fn draw_arrays(&self, mode: GLenum, first: GLint, count: GLsizei) { self.get().draw_arrays(mode, first, count) }
    pub fn draw_arrays_instanced(&self, mode: GLenum, first: GLint, count: GLsizei, primcount: GLsizei) { self.get().draw_arrays_instanced(mode, first, count, primcount) }
    pub fn draw_elements(&self, mode: GLenum, count: GLsizei, element_type: GLenum, indices_offset: GLuint) { self.get().draw_elements(mode, count, element_type, indices_offset) }
    pub fn draw_elements_instanced(&self, mode: GLenum, count: GLsizei, element_type: GLenum, indices_offset: GLuint, primcount: GLsizei) { self.get().draw_elements_instanced(mode, count, element_type, indices_offset, primcount) }
    pub fn blend_color(&self, r: f32, g: f32, b: f32, a: f32) { self.get().blend_color(r, g, b, a) }
    pub fn blend_func(&self, sfactor: GLenum, dfactor: GLenum) { self.get().blend_func(sfactor, dfactor) }
    pub fn blend_func_separate(&self, src_rgb: GLenum, dest_rgb: GLenum, src_alpha: GLenum, dest_alpha: GLenum) { self.get().blend_func_separate(src_rgb, dest_rgb, src_alpha, dest_alpha) }
    pub fn blend_equation(&self, mode: GLenum) { self.get().blend_equation(mode) }
    pub fn blend_equation_separate(&self, mode_rgb: GLenum, mode_alpha: GLenum) { self.get().blend_equation_separate(mode_rgb, mode_alpha) }
    pub fn color_mask(&self, r: bool, g: bool, b: bool, a: bool) { self.get().color_mask(r, g, b, a) }
    pub fn cull_face(&self, mode: GLenum) { self.get().cull_face(mode) }
    pub fn front_face(&self, mode: GLenum) { self.get().front_face(mode) }
    pub fn enable(&self, cap: GLenum) { self.get().enable(cap) }
    pub fn disable(&self, cap: GLenum) { self.get().disable(cap) }
    pub fn hint(&self, param_name: GLenum, param_val: GLenum) { self.get().hint(param_name, param_val) }
    pub fn is_enabled(&self, cap: GLenum) -> GLboolean { self.get().is_enabled(cap) }
    pub fn is_shader(&self, shader: GLuint) -> GLboolean { self.get().is_shader(shader) }
    pub fn is_texture(&self, texture: GLenum) -> GLboolean { self.get().is_texture(texture) }
    pub fn is_framebuffer(&self, framebuffer: GLenum) -> GLboolean { self.get().is_framebuffer(framebuffer) }
    pub fn is_renderbuffer(&self, renderbuffer: GLenum) -> GLboolean { self.get().is_renderbuffer(renderbuffer) }
    pub fn check_frame_buffer_status(&self, target: GLenum) -> GLenum { self.get().check_frame_buffer_status(target) }
    pub fn enable_vertex_attrib_array(&self, index: GLuint) { self.get().enable_vertex_attrib_array(index) }
    pub fn disable_vertex_attrib_array(&self, index: GLuint) { self.get().disable_vertex_attrib_array(index) }
    pub fn uniform_1f(&self, location: GLint, v0: GLfloat) { self.get().uniform_1f(location, v0) }
    pub fn uniform_1fv(&self, location: GLint, values: F32VecRef) { self.get().uniform_1fv(location, values.as_slice()) }
    pub fn uniform_1i(&self, location: GLint, v0: GLint) { self.get().uniform_1i(location, v0) }
    pub fn uniform_1iv(&self, location: GLint, values: I32VecRef) { self.get().uniform_1iv(location, values.as_slice()) }
    pub fn uniform_1ui(&self, location: GLint, v0: GLuint) { self.get().uniform_1ui(location, v0) }
    pub fn uniform_2f(&self, location: GLint, v0: GLfloat, v1: GLfloat) { self.get().uniform_2f(location, v0, v1) }
    pub fn uniform_2fv(&self, location: GLint, values: F32VecRef) { self.get().uniform_2fv(location, values.as_slice()) }
    pub fn uniform_2i(&self, location: GLint, v0: GLint, v1: GLint) { self.get().uniform_2i(location, v0, v1) }
    pub fn uniform_2iv(&self, location: GLint, values: I32VecRef) { self.get().uniform_2iv(location, values.as_slice()) }
    pub fn uniform_2ui(&self, location: GLint, v0: GLuint, v1: GLuint) { self.get().uniform_2ui(location, v0, v1) }
    pub fn uniform_3f(&self, location: GLint, v0: GLfloat, v1: GLfloat, v2: GLfloat) { self.get().uniform_3f(location, v0, v1, v2) }
    pub fn uniform_3fv(&self, location: GLint, values: F32VecRef) { self.get().uniform_3fv(location, values.as_slice()) }
    pub fn uniform_3i(&self, location: GLint, v0: GLint, v1: GLint, v2: GLint) { self.get().uniform_3i(location, v0, v1, v2) }
    pub fn uniform_3iv(&self, location: GLint, values: I32VecRef) { self.get().uniform_3iv(location, values.as_slice()) }
    pub fn uniform_3ui(&self, location: GLint, v0: GLuint, v1: GLuint, v2: GLuint) { self.get().uniform_3ui(location, v0, v1, v2) }
    pub fn uniform_4f(&self, location: GLint, x: GLfloat, y: GLfloat, z: GLfloat, w: GLfloat) { self.get().uniform_4f(location, x, y, z, w) }
    pub fn uniform_4i(&self, location: GLint, x: GLint, y: GLint, z: GLint, w: GLint) { self.get().uniform_4i(location, x, y, z, w) }
    pub fn uniform_4iv(&self, location: GLint, values: I32VecRef) { self.get().uniform_4iv(location, values.as_slice()) }
    pub fn uniform_4ui(&self, location: GLint, x: GLuint, y: GLuint, z: GLuint, w: GLuint) { self.get().uniform_4ui(location, x, y, z, w) }
    pub fn uniform_4fv(&self, location: GLint, values: F32VecRef) { self.get().uniform_4fv(location, values.as_slice()) }
    pub fn uniform_matrix_2fv(&self, location: GLint, transpose: bool, value: F32VecRef) { self.get().uniform_matrix_2fv(location, transpose, value.as_slice()) }
    pub fn uniform_matrix_3fv(&self, location: GLint, transpose: bool, value: F32VecRef) { self.get().uniform_matrix_3fv(location, transpose, value.as_slice()) }
    pub fn uniform_matrix_4fv(&self, location: GLint, transpose: bool, value: F32VecRef) { self.get().uniform_matrix_4fv(location, transpose, value.as_slice()) }
    pub fn depth_mask(&self, flag: bool) { self.get().depth_mask(flag) }
    pub fn depth_range(&self, near: f64, far: f64) { self.get().depth_range(near, far) }
    pub fn get_active_attrib(&self, program: GLuint, index: GLuint) -> GetActiveAttribReturn { let r = self.get().get_active_attrib(program, index); GetActiveAttribReturn { _0: r.0, _1: r.1, _2: r.2.into() } }
    pub fn get_active_uniform(&self, program: GLuint, index: GLuint) -> GetActiveUniformReturn { let r = self.get().get_active_uniform(program, index); GetActiveUniformReturn { _0: r.0, _1: r.1, _2: r.2.into() } }
    pub fn get_active_uniforms_iv(&self, program: GLuint, indices: GLuintVec, pname: GLenum) -> GLintVec { self.get().get_active_uniforms_iv(program, indices.into(), pname).into() }
    pub fn get_active_uniform_block_i(&self, program: GLuint, index: GLuint, pname: GLenum) -> GLint { self.get().get_active_uniform_block_i(program, index, pname) }
    pub fn get_active_uniform_block_iv(&self, program: GLuint, index: GLuint, pname: GLenum) -> GLintVec { self.get().get_active_uniform_block_iv(program, index, pname).into() }
    pub fn get_active_uniform_block_name(&self, program: GLuint, index: GLuint) -> AzString { self.get().get_active_uniform_block_name(program, index).into() }
    pub fn get_attrib_location(&self, program: GLuint, name: Refstr) -> c_int { self.get().get_attrib_location(program, name.as_str()) }
    pub fn get_frag_data_location(&self, program: GLuint, name: Refstr) -> c_int { self.get().get_frag_data_location(program, name.as_str()) }
    pub fn get_uniform_location(&self, program: GLuint, name: Refstr) -> c_int { self.get().get_uniform_location(program, name.as_str()) }
    pub fn get_program_info_log(&self, program: GLuint) -> AzString { self.get().get_program_info_log(program).into() }
    pub fn get_program_iv(&self, program: GLuint, pname: GLenum, mut result: GLintVecRefMut) { unsafe {  self.get().get_program_iv(program, pname, result.as_mut_slice())} }
    pub fn get_program_binary(&self, program: GLuint) -> GetProgramBinaryReturn { let r = self.get().get_program_binary(program); GetProgramBinaryReturn { _0: r.0.into(), _1: r.1 } }
    pub fn program_binary(&self, program: GLuint, format: GLenum, binary: U8VecRef) { self.get().program_binary(program, format, binary.as_slice()) }
    pub fn program_parameter_i(&self, program: GLuint, pname: GLenum, value: GLint) { self.get().program_parameter_i(program, pname, value) }
    pub fn get_vertex_attrib_iv(&self, index: GLuint, pname: GLenum, mut result: GLintVecRefMut) { unsafe {  self.get().get_vertex_attrib_iv(index, pname, result.as_mut_slice())} }
    pub fn get_vertex_attrib_fv(&self, index: GLuint, pname: GLenum, mut result: GLfloatVecRefMut) { unsafe {  self.get().get_vertex_attrib_fv(index, pname, result.as_mut_slice())} }
    pub fn get_vertex_attrib_pointer_v(&self, index: GLuint, pname: GLenum) -> GLsizeiptr { self.get().get_vertex_attrib_pointer_v(index, pname) }
    pub fn get_buffer_parameter_iv(&self, target: GLuint, pname: GLenum) -> GLint { self.get().get_buffer_parameter_iv(target, pname) }
    pub fn get_shader_info_log(&self, shader: GLuint) -> AzString { self.get().get_shader_info_log(shader).into() }
    pub fn get_string(&self, which: GLenum) -> AzString { self.get().get_string(which).into() }
    pub fn get_string_i(&self, which: GLenum, index: GLuint) -> AzString { self.get().get_string_i(which, index).into() }
    pub fn get_shader_iv(&self, shader: GLuint, pname: GLenum, mut result: GLintVecRefMut) { unsafe {  self.get().get_shader_iv(shader, pname, result.as_mut_slice())} }
    pub fn get_shader_precision_format(&self, shader_type: GLuint, precision_type: GLuint) -> [GLint;3] { let r = self.get().get_shader_precision_format(shader_type, precision_type); [r.0, r.1, r.2] }
    pub fn compile_shader(&self, shader: GLuint) { self.get().compile_shader(shader) }
    pub fn create_program(&self) -> GLuint { self.get().create_program() }
    pub fn delete_program(&self, program: GLuint) { self.get().delete_program(program) }
    pub fn create_shader(&self, shader_type: GLenum) -> GLuint { self.get().create_shader(shader_type) }
    pub fn delete_shader(&self, shader: GLuint) { self.get().delete_shader(shader) }
    pub fn detach_shader(&self, program: GLuint, shader: GLuint) { self.get().detach_shader(program, shader) }
    pub fn link_program(&self, program: GLuint) { self.get().link_program(program) }
    pub fn clear_color(&self, r: f32, g: f32, b: f32, a: f32) { self.get().clear_color(r, g, b, a) }
    pub fn clear(&self, buffer_mask: GLbitfield) { self.get().clear(buffer_mask) }
    pub fn clear_depth(&self, depth: f64) { self.get().clear_depth(depth) }
    pub fn clear_stencil(&self, s: GLint) { self.get().clear_stencil(s) }
    pub fn flush(&self) { self.get().flush() }
    pub fn finish(&self) { self.get().finish() }
    pub fn get_error(&self) -> GLenum { self.get().get_error() }
    pub fn stencil_mask(&self, mask: GLuint) { self.get().stencil_mask(mask) }
    pub fn stencil_mask_separate(&self, face: GLenum, mask: GLuint) { self.get().stencil_mask_separate(face, mask) }
    pub fn stencil_func(&self, func: GLenum, ref_: GLint, mask: GLuint) { self.get().stencil_func(func, ref_, mask) }
    pub fn stencil_func_separate(&self, face: GLenum, func: GLenum, ref_: GLint, mask: GLuint) { self.get().stencil_func_separate(face, func, ref_, mask) }
    pub fn stencil_op(&self, sfail: GLenum, dpfail: GLenum, dppass: GLenum) { self.get().stencil_op(sfail, dpfail, dppass) }
    pub fn stencil_op_separate(&self, face: GLenum, sfail: GLenum, dpfail: GLenum, dppass: GLenum) { self.get().stencil_op_separate(face, sfail, dpfail, dppass) }
    pub fn egl_image_target_texture2d_oes(&self, target: GLenum, image: GLeglImageOES) { self.get().egl_image_target_texture2d_oes(target, image) }
    pub fn generate_mipmap(&self, target: GLenum) { self.get().generate_mipmap(target) }
    pub fn insert_event_marker_ext(&self, message: Refstr) { self.get().insert_event_marker_ext(message.as_str()) }
    pub fn push_group_marker_ext(&self, message: Refstr) { self.get().push_group_marker_ext(message.as_str()) }
    pub fn pop_group_marker_ext(&self) { self.get().pop_group_marker_ext() }
    pub fn debug_message_insert_khr(&self, source: GLenum, type_: GLenum, id: GLuint, severity: GLenum, message: Refstr) { self.get().debug_message_insert_khr(source, type_, id, severity, message.as_str()) }
    pub fn push_debug_group_khr(&self, source: GLenum, id: GLuint, message: Refstr) { self.get().push_debug_group_khr(source, id, message.as_str()) }
    pub fn pop_debug_group_khr(&self) { self.get().pop_debug_group_khr() }
    pub fn fence_sync(&self, condition: GLenum, flags: GLbitfield) -> GLsyncPtr { GLsyncPtr::new(self.get().fence_sync(condition, flags)) }
    pub fn client_wait_sync(&self, sync: GLsyncPtr, flags: GLbitfield, timeout: GLuint64) { self.get().client_wait_sync(sync.get(), flags, timeout) }
    pub fn wait_sync(&self, sync: GLsyncPtr, flags: GLbitfield, timeout: GLuint64) { self.get().wait_sync(sync.get(), flags, timeout) }
    pub fn delete_sync(&self, sync: GLsyncPtr) { self.get().delete_sync(sync.get()) }
    pub fn texture_range_apple(&self, target: GLenum, data: U8VecRef) { self.get().texture_range_apple(target, data.as_slice()) }
    pub fn gen_fences_apple(&self, n: GLsizei) -> GLuintVec { self.get().gen_fences_apple(n).into() }
    pub fn delete_fences_apple(&self, fences: GLuintVecRef) { self.get().delete_fences_apple(fences.as_slice()) }
    pub fn set_fence_apple(&self, fence: GLuint) { self.get().set_fence_apple(fence) }
    pub fn finish_fence_apple(&self, fence: GLuint) { self.get().finish_fence_apple(fence) }
    pub fn test_fence_apple(&self, fence: GLuint) { self.get().test_fence_apple(fence) }
    pub fn test_object_apple(&self, object: GLenum, name: GLuint) -> GLboolean { self.get().test_object_apple(object, name) }
    pub fn finish_object_apple(&self, object: GLenum, name: GLuint) { self.get().finish_object_apple(object, name) }
    pub fn get_frag_data_index( &self, program: GLuint, name: Refstr) -> GLint { self.get().get_frag_data_index(program, name.as_str()) }
    pub fn blend_barrier_khr(&self) { self.get().blend_barrier_khr() }
    pub fn bind_frag_data_location_indexed( &self, program: GLuint, color_number: GLuint, index: GLuint, name: Refstr) { self.get().bind_frag_data_location_indexed(program, color_number, index, name.as_str()) }
    pub fn get_debug_messages(&self) -> AzDebugMessageVec { let dmv: Vec<AzDebugMessage> = self.get().get_debug_messages().into_iter().map(|d| AzDebugMessage { message: d.message.into(), source: d.source, ty: d.ty, id: d.ty, severity: d.severity }).collect(); dmv.into() }
    pub fn provoking_vertex_angle(&self, mode: GLenum) { self.get().provoking_vertex_angle(mode) }
    pub fn gen_vertex_arrays_apple(&self, n: GLsizei) -> GLuintVec { self.get().gen_vertex_arrays_apple(n).into() }
    pub fn bind_vertex_array_apple(&self, vao: GLuint) { self.get().bind_vertex_array_apple(vao) }
    pub fn delete_vertex_arrays_apple(&self, vertex_arrays: GLuintVecRef) { self.get().delete_vertex_arrays_apple(vertex_arrays.as_slice()) }
    pub fn copy_texture_chromium(&self, source_id: GLuint, source_level: GLint, dest_target: GLenum, dest_id: GLuint, dest_level: GLint, internal_format: GLint, dest_type: GLenum, unpack_flip_y: GLboolean, unpack_premultiply_alpha: GLboolean, unpack_unmultiply_alpha: GLboolean) { self.get().copy_texture_chromium(source_id, source_level, dest_target, dest_id, dest_level, internal_format, dest_type, unpack_flip_y, unpack_premultiply_alpha, unpack_unmultiply_alpha) }
    pub fn copy_sub_texture_chromium(&self, source_id: GLuint, source_level: GLint, dest_target: GLenum, dest_id: GLuint, dest_level: GLint, x_offset: GLint, y_offset: GLint, x: GLint, y: GLint, width: GLsizei, height: GLsizei, unpack_flip_y: GLboolean, unpack_premultiply_alpha: GLboolean, unpack_unmultiply_alpha: GLboolean) { self.get().copy_sub_texture_chromium(source_id, source_level, dest_target, dest_id, dest_level, x_offset, y_offset, x, y, width, height, unpack_flip_y, unpack_premultiply_alpha, unpack_unmultiply_alpha) }
    pub fn egl_image_target_renderbuffer_storage_oes(&self, target: u32, image: *const c_void) { self.get().egl_image_target_renderbuffer_storage_oes(target, image) }
    pub fn copy_texture_3d_angle( &self, source_id: GLuint, source_level: GLint, dest_target: GLenum, dest_id: GLuint, dest_level: GLint, internal_format: GLint, dest_type: GLenum, unpack_flip_y: GLboolean, unpack_premultiply_alpha: GLboolean, unpack_unmultiply_alpha: GLboolean) { self.get().copy_texture_3d_angle(source_id, source_level, dest_target, dest_id, dest_level, internal_format, dest_type, unpack_flip_y, unpack_premultiply_alpha, unpack_unmultiply_alpha) }
    pub fn copy_sub_texture_3d_angle(&self, source_id: GLuint, source_level: GLint, dest_target: GLenum, dest_id: GLuint, dest_level: GLint, x_offset: GLint, y_offset: GLint, z_offset: GLint, x: GLint, y: GLint, z: GLint, width: GLsizei, height: GLsizei, depth: GLsizei, unpack_flip_y: GLboolean, unpack_premultiply_alpha: GLboolean, unpack_unmultiply_alpha: GLboolean) { self.get().copy_sub_texture_3d_angle(source_id, source_level, dest_target, dest_id, dest_level, x_offset, y_offset, z_offset, x, y, z, width, height, depth, unpack_flip_y, unpack_premultiply_alpha, unpack_unmultiply_alpha) }
}

#[cfg(feature = "opengl")]
impl Clone for GlContextPtr {
    fn clone(&self) -> Self {
        Self::new(self.get().clone())
    }
}

#[cfg(feature = "opengl")]
impl Drop for GlContextPtr {
    fn drop(&mut self) {
        let _ = unsafe { Box::from_raw(self.ptr as *mut Rc<dyn Gl>) };
    }
}

/// OpenGL texture, use `ReadOnlyWindow::create_texture` to create a texture
#[repr(C)]
pub struct Texture {
    /// Raw OpenGL texture ID
    pub texture_id: GLuint,
    /// Hints and flags for optimization purposes
    pub flags: TextureFlags,
    /// Size of this texture (in pixels)
    pub size: LogicalSize,
    /// A reference-counted pointer to the OpenGL context (so that the texture can be deleted in the destructor)
    pub gl_context: GlContextPtr,
}

impl_option!(Texture, OptionTexture, copy = false, clone = false, [Debug, PartialEq, Eq, PartialOrd, Ord, Hash]);

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub struct TextureFlags {
    /// Whether this texture contains an alpha component
    pub is_opaque: bool,
    /// Optimization: use the compositor instead of OpenGL for energy optimization
    pub is_video_texture: bool,
}

impl ::std::fmt::Display for Texture {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "Texture {{ id: {}, {}x{} }}", self.texture_id, self.size.width, self.size.height)
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
        self.gl_context.delete_textures((&[self.texture_id])[..].into());
    }
}

/// Describes the vertex layout and offsets
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VertexLayout {
    pub fields: Vec<VertexAttribute>,
}

impl VertexLayout {

    /// Submits the vertex buffer description to OpenGL
    pub fn bind(&self, shader: &GlShader) {

        const VERTICES_ARE_NORMALIZED: bool = false;

        let gl_context = &shader.gl_context;

        let mut offset = 0;

        let stride_between_vertices: usize = self.fields.iter().map(VertexAttribute::get_stride).sum();

        for vertex_attribute in self.fields.iter() {

            let attribute_location = vertex_attribute.layout_location
                .as_option()
                .map(|ll| *ll as i32)
                .unwrap_or_else(|| gl_context.get_attrib_location(shader.program_id, vertex_attribute.name.as_str().into()));

            gl_context.vertex_attrib_pointer(
                attribute_location as u32,
                vertex_attribute.item_count as i32,
                vertex_attribute.attribute_type.get_gl_id(),
                VERTICES_ARE_NORMALIZED,
                stride_between_vertices as i32,
                offset as u32,
            );
            gl_context.enable_vertex_attrib_array(attribute_location as u32);
            offset += vertex_attribute.get_stride();
        }
    }

    /// Unsets the vertex buffer description
    pub fn unbind(&self, shader: &GlShader) {
        let gl_context = &shader.gl_context;
        for vertex_attribute in self.fields.iter() {
            let attribute_location = vertex_attribute.layout_location
                .as_option()
                .map(|ll| *ll as i32)
                .unwrap_or_else(|| gl_context.get_attrib_location(shader.program_id, vertex_attribute.name.as_str().into()));
            gl_context.disable_vertex_attrib_array(attribute_location as u32);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct VertexAttribute {
    /// Attribute name of the vertex attribute in the vertex shader, i.e. `"vAttrXY"`
    pub name: AzString,
    /// If the vertex shader has a specific location, (like `layout(location = 2) vAttrXY`),
    /// use this instead of the name to look up the uniform location.
    pub layout_location: OptionUsize,
    /// Type of items of this attribute (i.e. for a `FloatVec2`, would be `VertexAttributeType::Float`)
    pub attribute_type: VertexAttributeType,
    /// Number of items of this attribute (i.e. for a `FloatVec2`, would be `2` (= 2 consecutive f32 values))
    pub item_count: usize,
}

impl_option!(usize, OptionUsize, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

impl VertexAttribute {
    pub fn get_stride(&self) -> usize {
        self.attribute_type.get_mem_size() * self.item_count
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
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

    pub fn get_mem_size(&self) -> usize {
        use std::mem;
        use self::VertexAttributeType::*;
        match self {
            Float => mem::size_of::<f32>(),
            Double => mem::size_of::<f64>(),
            UnsignedByte => mem::size_of::<u8>(),
            UnsignedShort => mem::size_of::<u16>(),
            UnsignedInt => mem::size_of::<u32>(),
        }
    }
}

pub trait VertexLayoutDescription {
    fn get_description() -> VertexLayout;
}

pub struct VertexArrayObject {
    pub vertex_layout: VertexLayout,
    pub vao_id: GLuint,
    pub gl_context: GlContextPtr,
}

impl Drop for VertexArrayObject {
    fn drop(&mut self) {
        self.gl_context.delete_vertex_arrays((&[self.vao_id])[..].into());
    }
}

pub struct VertexBuffer<T: VertexLayoutDescription> {
    pub vertex_buffer_id: GLuint,
    pub vertex_buffer_len: usize,
    pub gl_context: GlContextPtr,
    pub vao: VertexArrayObject,
    pub vertex_buffer_type: PhantomData<T>,

    // Since vertex buffer + index buffer have to be created together (because of the VAO), s
    pub index_buffer_id: GLuint,
    pub index_buffer_len: usize,
    pub index_buffer_format: IndexBufferFormat,
}

impl<T: VertexLayoutDescription> ::std::fmt::Display for VertexBuffer<T> {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f,
            "VertexBuffer {{ buffer: {} (length: {}) }})",
            self.vertex_buffer_id, self.vertex_buffer_len
        )
    }
}

impl_traits_for_gl_object!(VertexBuffer<T: VertexLayoutDescription>, vertex_buffer_id);

impl<T: VertexLayoutDescription> Drop for VertexBuffer<T> {
    fn drop(&mut self) {
        self.gl_context.delete_buffers((&[self.vertex_buffer_id, self.index_buffer_id])[..].into());
    }
}

impl<T: VertexLayoutDescription> VertexBuffer<T> {
    pub fn new(shader: &GlShader, vertices: &[T], indices: &[u32], index_buffer_format: IndexBufferFormat) -> Self {

        use std::mem;

        let gl_context = shader.gl_context.clone();

        // Save the OpenGL state
        let mut current_vertex_array = [0_i32];
        let mut current_vertex_buffer = [0_i32];
        let mut current_index_buffer = [0_i32];

        gl_context.get_integer_v(gl::VERTEX_ARRAY, (&mut current_vertex_array[..]).into());
        gl_context.get_integer_v(gl::ARRAY_BUFFER, (&mut current_vertex_buffer[..]).into());
        gl_context.get_integer_v(gl::ELEMENT_ARRAY_BUFFER, (&mut current_index_buffer[..]).into());

        let vertex_array_object = gl_context.gen_vertex_arrays(1);
        let vertex_array_object = vertex_array_object.get(0).unwrap();

        let vertex_buffer_id = gl_context.gen_buffers(1);
        let vertex_buffer_id = vertex_buffer_id.get(0).unwrap();

        let index_buffer_id = gl_context.gen_buffers(1);
        let index_buffer_id = index_buffer_id.get(0).unwrap();

        gl_context.bind_vertex_array(*vertex_array_object);

        // Upload vertex data to GPU
        gl_context.bind_buffer(gl::ARRAY_BUFFER, *vertex_buffer_id);
        gl_context.buffer_data_untyped(
            gl::ARRAY_BUFFER,
            (mem::size_of::<T>() * vertices.len()) as isize,
            vertices.as_ptr() as *const c_void,
            gl::STATIC_DRAW
        );

        // Generate the index buffer + upload data
        gl_context.bind_buffer(gl::ELEMENT_ARRAY_BUFFER, *index_buffer_id);
        gl_context.buffer_data_untyped(
            gl::ELEMENT_ARRAY_BUFFER,
            (mem::size_of::<u32>() * indices.len()) as isize,
            indices.as_ptr() as *const c_void,
            gl::STATIC_DRAW
        );

        let vertex_description = T::get_description();
        vertex_description.bind(shader);

        // Reset the OpenGL state
        gl_context.bind_buffer(gl::ARRAY_BUFFER, current_vertex_buffer[0] as u32);
        gl_context.bind_buffer(gl::ELEMENT_ARRAY_BUFFER, current_index_buffer[0] as u32);
        gl_context.bind_vertex_array(current_vertex_array[0] as u32);

        Self {
            vertex_buffer_id: *vertex_buffer_id,
            vertex_buffer_len: vertices.len(),
            gl_context: gl_context.clone(),
            vao: VertexArrayObject {
                vertex_layout: vertex_description,
                vao_id: *vertex_array_object,
                gl_context,
            },
            vertex_buffer_type: PhantomData,
            index_buffer_id: *index_buffer_id,
            index_buffer_len: indices.len(),
            index_buffer_format,
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
    pub fn get(gl_context: &dyn Gl) -> Self {
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

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct Uniform {
    pub name: AzString,
    pub uniform_type: UniformType,
}

impl Uniform {
    pub fn new<S: Into<AzString>>(name: S, uniform_type: UniformType) -> Self {
        Self { name: name.into(), uniform_type }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C, u8)]
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
    pub fn set(self, gl_context: &Rc<dyn Gl>, location: GLint) {
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

#[repr(C)]
pub struct GlShader {
    pub program_id: GLuint,
    pub gl_context: GlContextPtr,
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
    pub fn new(gl_context: GlContextPtr, vertex_shader: &str, fragment_shader: &str) -> Result<Self, GlShaderCreateError> {

        // Check whether the OpenGL implementation supports a shader compiler...
        let mut shader_compiler_supported = [gl::FALSE];
        gl_context.get_boolean_v(gl::SHADER_COMPILER, (&mut shader_compiler_supported[..]).into());
        if shader_compiler_supported[0] == gl::FALSE {
            // Implementation only supports binary shaders
            return Err(GlShaderCreateError::NoShaderCompiler);
        }

        // Compile vertex shader

        let vertex_shader_object = gl_context.create_shader(gl::VERTEX_SHADER);
        gl_context.shader_source(vertex_shader_object, vec![AzString::from(vertex_shader.to_string())].into());
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
        gl_context.shader_source(fragment_shader_object, vec![AzString::from(fragment_shader.to_string())].into());
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

    /// Draws vertex buffers, index buffers + uniforms to the currently bound framebuffer
    ///
    /// **NOTE: `FrameBuffer::bind()` and `VertexBuffer::bind()` have to be called first!**
    pub fn draw<T: VertexLayoutDescription>(
        &mut self,
        buffers: &[(Rc<VertexBuffer<T>>, Vec<Uniform>)],
        clear_color: Option<ColorU>,
        texture_size: LogicalSize,
    ) -> Texture {

        use std::ops::Deref;
        use std::collections::HashMap;

        const INDEX_TYPE: GLuint = gl::UNSIGNED_INT;

        let gl_context = &self.gl_context;

        // save the OpenGL state
        let mut current_multisample = [0_u8];
        let mut current_index_buffer = [0_i32];
        let mut current_vertex_buffer = [0_i32];
        let mut current_vertex_array_object = [0_i32];
        let mut current_program = [0_i32];
        let mut current_framebuffers = [0_i32];
        let mut current_renderbuffers = [0_i32];
        let mut current_texture_2d = [0_i32];

        gl_context.get_boolean_v(gl::MULTISAMPLE, (&mut current_multisample[..]).into());
        gl_context.get_integer_v(gl::ARRAY_BUFFER_BINDING, (&mut current_vertex_buffer[..]).into());
        gl_context.get_integer_v(gl::ELEMENT_ARRAY_BUFFER_BINDING, (&mut current_index_buffer[..]).into());
        gl_context.get_integer_v(gl::CURRENT_PROGRAM, (&mut current_program[..]).into());
        gl_context.get_integer_v(gl::VERTEX_ARRAY_BINDING, (&mut current_vertex_array_object[..]).into());
        gl_context.get_integer_v(gl::RENDERBUFFER, (&mut current_renderbuffers[..]).into());
        gl_context.get_integer_v(gl::FRAMEBUFFER, (&mut current_framebuffers[..]).into());
        gl_context.get_integer_v(gl::TEXTURE_2D, (&mut current_texture_2d[..]).into());

        // 1. Create the texture + framebuffer

        let textures = gl_context.gen_textures(1);
        let texture_id = textures.get(0).unwrap();
        let framebuffers = gl_context.gen_framebuffers(1);
        let framebuffer_id = framebuffers.get(0).unwrap();
        gl_context.bind_framebuffer(gl::FRAMEBUFFER, *framebuffer_id);

        let depthbuffers = gl_context.gen_renderbuffers(1);
        let depthbuffer_id = depthbuffers.get(0).unwrap();

        gl_context.bind_texture(gl::TEXTURE_2D, *texture_id);
        gl_context.tex_image_2d(gl::TEXTURE_2D, 0, gl::RGBA as i32, texture_size.width as i32, texture_size.height as i32, 0, gl::RGBA, gl::UNSIGNED_BYTE, None.into());
        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);

        gl_context.bind_renderbuffer(gl::RENDERBUFFER, *depthbuffer_id);
        gl_context.renderbuffer_storage(gl::RENDERBUFFER, gl::DEPTH_COMPONENT, texture_size.width as i32, texture_size.height as i32);
        gl_context.framebuffer_renderbuffer(gl::FRAMEBUFFER, gl::DEPTH_ATTACHMENT, gl::RENDERBUFFER, *depthbuffer_id);

        gl_context.framebuffer_texture_2d(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, *texture_id, 0);
        gl_context.draw_buffers([gl::COLOR_ATTACHMENT0][..].into());
        gl_context.viewport(0, 0, texture_size.width as i32, texture_size.height as i32);

        debug_assert!(gl_context.check_frame_buffer_status(gl::FRAMEBUFFER) == gl::FRAMEBUFFER_COMPLETE);

        gl_context.use_program(self.program_id);
        gl_context.disable(gl::MULTISAMPLE);

        // Avoid multiple calls to get_uniform_location by caching the uniform locations
        let mut uniform_locations: HashMap<AzString, i32> = HashMap::new();
        let mut max_uniform_len = 0;
        for (_, uniforms) in buffers {
            for uniform in uniforms.iter() {
                if !uniform_locations.contains_key(&uniform.name) {
                    uniform_locations.insert(uniform.name.clone(), gl_context.get_uniform_location(self.program_id, uniform.name.as_str().into()));
                }
            }
            max_uniform_len = max_uniform_len.max(uniforms.len());
        }
        let mut current_uniforms = vec![None;max_uniform_len];

        // Since the description of the vertex buffers is always the same, only the first layer needs to bind its VAO


        if let Some(clear_color) = clear_color {
            let clear_color: ColorF = clear_color.into();
            gl_context.clear_color(clear_color.r, clear_color.g, clear_color.b, clear_color.a);
        }

        gl_context.clear_depth(0.0);
        gl_context.clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);

        // Draw the actual layers
        for (vi, uniforms) in buffers {

            let vertex_buffer = vi.deref();

            gl_context.bind_vertex_array(vertex_buffer.vao.vao_id);
            // NOTE: Technically not required, but some drivers...
            gl_context.bind_buffer(gl::ELEMENT_ARRAY_BUFFER, vertex_buffer.index_buffer_id);

            // Only set the uniform if the value has changed
            for (uniform_index, uniform) in uniforms.iter().enumerate() {
                if current_uniforms[uniform_index] != Some(uniform.uniform_type) {
                    let uniform_location = uniform_locations[&uniform.name];
                    uniform.uniform_type.set(gl_context.get(), uniform_location);
                    current_uniforms[uniform_index] = Some(uniform.uniform_type);
                }
            }

            gl_context.draw_elements(vertex_buffer.index_buffer_format.get_gl_id(), vertex_buffer.index_buffer_len as i32, INDEX_TYPE, 0);
        }

        // Reset the OpenGL state to what it was before
        if current_multisample[0] == gl::TRUE { gl_context.enable(gl::MULTISAMPLE); }
        gl_context.bind_vertex_array(current_vertex_array_object[0] as u32);
        gl_context.bind_framebuffer(gl::FRAMEBUFFER, current_framebuffers[0] as u32);
        gl_context.bind_texture(gl::TEXTURE_2D, current_texture_2d[0] as u32);
        gl_context.bind_texture(gl::RENDERBUFFER, current_renderbuffers[0] as u32);
        gl_context.bind_buffer(gl::ELEMENT_ARRAY_BUFFER, current_index_buffer[0] as u32);
        gl_context.bind_buffer(gl::ARRAY_BUFFER, current_vertex_buffer[0] as u32);
        gl_context.use_program(current_program[0] as u32);

        gl_context.delete_framebuffers((&[*framebuffer_id])[..].into());
        gl_context.delete_renderbuffers((&[*depthbuffer_id])[..].into());

        Texture {
            texture_id: *texture_id,
            size: texture_size,
            flags: TextureFlags {
                is_opaque: true,
                is_video_texture: false,
            },
            gl_context: self.gl_context.clone(),
        }
    }
}

#[cfg(debug_assertions)]
fn get_gl_shader_error(context: &dyn Gl, shader_object: GLuint) -> Option<i32> {
    let mut err = [0];
    unsafe { context.get_shader_iv(shader_object, gl::COMPILE_STATUS, &mut err) };
    let err_code = err[0];
    if err_code == gl::TRUE as i32 { None } else { Some(err_code) }
}

#[cfg(debug_assertions)]
fn get_gl_program_error(context: &dyn Gl, shader_object: GLuint) -> Option<i32> {
    let mut err = [0];
    unsafe { context.get_program_iv(shader_object, gl::LINK_STATUS, &mut err) };
    let err_code = err[0];
    if err_code == gl::TRUE as i32 { None } else { Some(err_code) }
}
