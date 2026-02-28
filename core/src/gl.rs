#![allow(unused_variables)]
use alloc::{
    boxed::Box,
    rc::Rc,
    string::{String, ToString},
    vec::Vec,
};
use core::{
    ffi, fmt,
    hash::{Hash, Hasher},
    sync::atomic::{AtomicUsize, Ordering as AtomicOrdering},
};

use azul_css::{
    props::{
        basic::{ColorF, ColorU},
        style::StyleTransformVec,
    },
    AzString, OptionI32, OptionU32, OptionUsize, StringVec, U8Vec,
};
pub use gl_context_loader::{
    ctypes::*, gl, GLeglImageOES, GLsync, GLvoid, GenericGlContext, GlType as GlContextGlType,
};

pub use crate::glconst::*;
use crate::{
    geom::PhysicalSizeU32,
    hit_test::DocumentId,
    resources::{Epoch, ExternalImageId, ImageDescriptor, ImageDescriptorFlags, RawImageFormat},
    svg::{TessellatedGPUSvgNode, TessellatedSvgNode},
    window::RendererType,
    FastHashMap,
};

pub type GLuint = u32;
pub type GLint = i32;
pub type GLint64 = i64;
pub type GLuint64 = u64;
pub type GLenum = u32;
pub type GLintptr = isize;
pub type GLboolean = u8;
pub type GLsizeiptr = isize;
pub type GLbitfield = u32;
pub type GLsizei = i32;
pub type GLclampf = f32;
pub type GLfloat = f32;

pub const GL_RESTART_INDEX: u32 = core::u32::MAX;

/// Passing *const c_void is not easily possible when generating APIs,
/// so this wrapper struct is for easier API generation
#[repr(C)]
#[derive(Debug)]
pub struct GlVoidPtrConst {
    pub ptr: *const GLvoid,
    pub run_destructor: bool,
}

impl Clone for GlVoidPtrConst {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
            run_destructor: true,
        }
    }
}

impl Drop for GlVoidPtrConst {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

/// Struct returned from the C API
///
/// Because of Python, every object has to be clone-able,
/// so yes there may exist more than one mutable reference
#[repr(C)]
#[derive(Debug)]
pub struct GlVoidPtrMut {
    pub ptr: *mut GLvoid,
}

impl Clone for GlVoidPtrMut {
    fn clone(&self) -> Self {
        Self { ptr: self.ptr }
    }
}

// &str
#[repr(C)]
pub struct Refstr {
    pub ptr: *const u8,
    pub len: usize,
}

impl Clone for Refstr {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
            len: self.len,
        }
    }
}

impl core::fmt::Debug for Refstr {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.as_str().fmt(f)
    }
}

impl Refstr {
    pub fn as_str(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(self.ptr, self.len)) }
    }
}

impl From<&str> for Refstr {
    fn from(s: &str) -> Self {
        Self {
            ptr: s.as_ptr(),
            len: s.len(),
        }
    }
}

// &[&str]
#[repr(C)]
pub struct RefstrVecRef {
    pub ptr: *const Refstr,
    pub len: usize,
}

impl Clone for RefstrVecRef {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
            len: self.len,
        }
    }
}

impl core::fmt::Debug for RefstrVecRef {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.as_slice().fmt(f)
    }
}

impl RefstrVecRef {
    pub fn as_slice(&self) -> &[Refstr] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl From<&[Refstr]> for RefstrVecRef {
    fn from(s: &[Refstr]) -> Self {
        Self {
            ptr: s.as_ptr(),
            len: s.len(),
        }
    }
}

// &mut [GLint64]
#[repr(C)]
pub struct GLint64VecRefMut {
    pub ptr: *mut i64,
    pub len: usize,
}

impl Clone for GLint64VecRefMut {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
            len: self.len,
        }
    }
}

impl core::fmt::Debug for GLint64VecRefMut {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.as_slice().fmt(f)
    }
}

impl From<&mut [GLint64]> for GLint64VecRefMut {
    fn from(s: &mut [GLint64]) -> Self {
        Self {
            ptr: s.as_mut_ptr(),
            len: s.len(),
        }
    }
}

impl GLint64VecRefMut {
    pub fn as_slice(&self) -> &[GLint64] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
    fn as_mut_slice(&mut self) -> &mut [GLint64] {
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

// &mut [GLfloat]
#[repr(C)]
pub struct GLfloatVecRefMut {
    pub ptr: *mut f32,
    pub len: usize,
}

impl Clone for GLfloatVecRefMut {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
            len: self.len,
        }
    }
}

impl core::fmt::Debug for GLfloatVecRefMut {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.as_slice().fmt(f)
    }
}

impl From<&mut [GLfloat]> for GLfloatVecRefMut {
    fn from(s: &mut [GLfloat]) -> Self {
        Self {
            ptr: s.as_mut_ptr(),
            len: s.len(),
        }
    }
}

impl GLfloatVecRefMut {
    pub fn as_slice(&self) -> &[GLfloat] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
    fn as_mut_slice(&mut self) -> &mut [GLfloat] {
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

// &mut [GLint]
#[repr(C)]
pub struct GLintVecRefMut {
    pub ptr: *mut i32,
    pub len: usize,
}

impl Clone for GLintVecRefMut {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
            len: self.len,
        }
    }
}

impl core::fmt::Debug for GLintVecRefMut {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.as_slice().fmt(f)
    }
}

impl From<&mut [GLint]> for GLintVecRefMut {
    fn from(s: &mut [GLint]) -> Self {
        Self {
            ptr: s.as_mut_ptr(),
            len: s.len(),
        }
    }
}

impl GLintVecRefMut {
    pub fn as_slice(&self) -> &[GLint] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
    fn as_mut_slice(&mut self) -> &mut [GLint] {
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

// &[GLuint]
#[repr(C)]
pub struct GLuintVecRef {
    pub ptr: *const u32,
    pub len: usize,
}

impl Clone for GLuintVecRef {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
            len: self.len,
        }
    }
}

impl core::fmt::Debug for GLuintVecRef {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.as_slice().fmt(f)
    }
}

impl From<&[GLuint]> for GLuintVecRef {
    fn from(s: &[GLuint]) -> Self {
        Self {
            ptr: s.as_ptr(),
            len: s.len(),
        }
    }
}

impl GLuintVecRef {
    pub fn as_slice(&self) -> &[GLuint] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
}

// &[GLenum]
#[repr(C)]
pub struct GLenumVecRef {
    pub ptr: *const u32,
    pub len: usize,
}

impl Clone for GLenumVecRef {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
            len: self.len,
        }
    }
}

impl core::fmt::Debug for GLenumVecRef {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.as_slice().fmt(f)
    }
}

impl From<&[GLenum]> for GLenumVecRef {
    fn from(s: &[GLenum]) -> Self {
        Self {
            ptr: s.as_ptr(),
            len: s.len(),
        }
    }
}

impl GLenumVecRef {
    pub fn as_slice(&self) -> &[GLenum] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
}

// &[u8]
#[repr(C)]
pub struct U8VecRef {
    pub ptr: *const u8,
    pub len: usize,
}

impl Clone for U8VecRef {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
            len: self.len,
        }
    }
}

impl From<&[u8]> for U8VecRef {
    fn from(s: &[u8]) -> Self {
        Self {
            ptr: s.as_ptr(),
            len: s.len(),
        }
    }
}

impl U8VecRef {
    pub fn as_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl fmt::Debug for U8VecRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.as_slice().fmt(f)
    }
}

impl PartialOrd for U8VecRef {
    fn partial_cmp(&self, rhs: &Self) -> Option<core::cmp::Ordering> {
        self.as_slice().partial_cmp(rhs.as_slice())
    }
}

impl Ord for U8VecRef {
    fn cmp(&self, rhs: &Self) -> core::cmp::Ordering {
        self.as_slice().cmp(rhs.as_slice())
    }
}

impl PartialEq for U8VecRef {
    fn eq(&self, rhs: &Self) -> bool {
        self.as_slice().eq(rhs.as_slice())
    }
}

impl Eq for U8VecRef {}

impl core::hash::Hash for U8VecRef {
    fn hash<H>(&self, state: &mut H)
    where
        H: core::hash::Hasher,
    {
        self.as_slice().hash(state)
    }
}

// &[f32]
#[repr(C)]
pub struct F32VecRef {
    pub ptr: *const f32,
    pub len: usize,
}

impl Clone for F32VecRef {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
            len: self.len,
        }
    }
}

impl core::fmt::Debug for F32VecRef {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.as_slice().fmt(f)
    }
}

impl From<&[f32]> for F32VecRef {
    fn from(s: &[f32]) -> Self {
        Self {
            ptr: s.as_ptr(),
            len: s.len(),
        }
    }
}

impl F32VecRef {
    pub fn as_slice(&self) -> &[f32] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
}

// &[i32]
#[repr(C)]
pub struct I32VecRef {
    pub ptr: *const i32,
    pub len: usize,
}

impl Clone for I32VecRef {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
            len: self.len,
        }
    }
}

impl core::fmt::Debug for I32VecRef {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.as_slice().fmt(f)
    }
}

impl From<&[i32]> for I32VecRef {
    fn from(s: &[i32]) -> Self {
        Self {
            ptr: s.as_ptr(),
            len: s.len(),
        }
    }
}

impl I32VecRef {
    pub fn as_slice(&self) -> &[i32] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
}

// &mut [u8]
#[repr(C)]
pub struct GLbooleanVecRefMut {
    pub ptr: *mut u8,
    pub len: usize,
}

impl Clone for GLbooleanVecRefMut {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
            len: self.len,
        }
    }
}

impl core::fmt::Debug for GLbooleanVecRefMut {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.as_slice().fmt(f)
    }
}

impl From<&mut [GLboolean]> for GLbooleanVecRefMut {
    fn from(s: &mut [GLboolean]) -> Self {
        Self {
            ptr: s.as_mut_ptr(),
            len: s.len(),
        }
    }
}

impl GLbooleanVecRefMut {
    pub fn as_slice(&self) -> &[GLboolean] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
    fn as_mut_slice(&mut self) -> &mut [GLboolean] {
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

// &mut [u8]
#[repr(C)]
pub struct U8VecRefMut {
    pub ptr: *mut u8,
    pub len: usize,
}

impl Clone for U8VecRefMut {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
            len: self.len,
        }
    }
}

impl core::fmt::Debug for U8VecRefMut {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.as_slice().fmt(f)
    }
}

impl From<&mut [u8]> for U8VecRefMut {
    fn from(s: &mut [u8]) -> Self {
        Self {
            ptr: s.as_mut_ptr(),
            len: s.len(),
        }
    }
}

impl U8VecRefMut {
    pub fn as_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
    fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

impl_option!(
    U8VecRef,
    OptionU8VecRef,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct DebugMessage {
    pub message: AzString,
    pub source: GLenum,
    pub ty: GLenum,
    pub id: GLenum,
    pub severity: GLenum,
}

impl_option!(
    DebugMessage,
    OptionDebugMessage,
    copy = false,
    [Debug, Clone, PartialEq, Eq, Ord, PartialOrd, Hash]
);

impl_vec!(DebugMessage, DebugMessageVec, DebugMessageVecDestructor, DebugMessageVecDestructorType, DebugMessageVecSlice, OptionDebugMessage);
impl_vec_debug!(DebugMessage, DebugMessageVec);
impl_vec_partialord!(DebugMessage, DebugMessageVec);
impl_vec_ord!(DebugMessage, DebugMessageVec);
impl_vec_clone!(DebugMessage, DebugMessageVec, DebugMessageVecDestructor);
impl_vec_partialeq!(DebugMessage, DebugMessageVec);
impl_vec_eq!(DebugMessage, DebugMessageVec);
impl_vec_hash!(DebugMessage, DebugMessageVec);

impl_vec!(GLint, GLintVec, GLintVecDestructor, GLintVecDestructorType, GLintVecSlice, OptionI32);
impl_vec_debug!(GLint, GLintVec);
impl_vec_partialord!(GLint, GLintVec);
impl_vec_ord!(GLint, GLintVec);
impl_vec_clone!(GLint, GLintVec, GLintVecDestructor);
impl_vec_partialeq!(GLint, GLintVec);
impl_vec_eq!(GLint, GLintVec);
impl_vec_hash!(GLint, GLintVec);

impl_vec!(GLuint, GLuintVec, GLuintVecDestructor, GLuintVecDestructorType, GLuintVecSlice, OptionU32);
impl_vec_debug!(GLuint, GLuintVec);
impl_vec_partialord!(GLuint, GLuintVec);
impl_vec_ord!(GLuint, GLuintVec);
impl_vec_clone!(GLuint, GLuintVec, GLuintVecDestructor);
impl_vec_partialeq!(GLuint, GLuintVec);
impl_vec_eq!(GLuint, GLuintVec);
impl_vec_hash!(GLuint, GLuintVec);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum GlType {
    Gl,
    Gles,
}

impl From<GlContextGlType> for GlType {
    fn from(a: GlContextGlType) -> GlType {
        match a {
            GlContextGlType::Gl => GlType::Gl,
            GlContextGlType::GlEs => GlType::Gles,
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
    pub run_destructor: bool,
}

impl Clone for GLsyncPtr {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
            run_destructor: true,
        }
    }
}

impl core::fmt::Debug for GLsyncPtr {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "0x{:0x}", self.ptr as usize)
    }
}

impl GLsyncPtr {
    pub fn new(p: GLsync) -> Self {
        Self {
            ptr: p as *const c_void,
            run_destructor: true,
        }
    }
    pub fn get(self) -> GLsync {
        self.ptr as GLsync
    }
}

impl Drop for GLsyncPtr {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

/// Each pipeline (window) has its own OpenGL textures. GL Textures can technically
/// be shared across pipelines, however this turns out to be very difficult in practice.
pub type GlTextureStorage = FastHashMap<Epoch, FastHashMap<ExternalImageId, Texture>>;

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
/// WARNING: Not thread-safe (however, the Texture itself is thread-unsafe, so it's unlikely to ever
/// be misused)
static mut ACTIVE_GL_TEXTURES: Option<FastHashMap<DocumentId, GlTextureStorage>> = None;

/// Inserts a new texture into the OpenGL texture cache, returns a new image ID
/// for the inserted texture
///
/// This function exists so azul doesn't have to use `lazy_static` as a dependency
pub fn insert_into_active_gl_textures(
    document_id: DocumentId,
    epoch: Epoch,
    texture: Texture,
) -> ExternalImageId {
    let external_image_id = ExternalImageId::new();

    unsafe {
        if ACTIVE_GL_TEXTURES.is_none() {
            ACTIVE_GL_TEXTURES = Some(FastHashMap::new());
        }
        let active_textures = ACTIVE_GL_TEXTURES.as_mut().unwrap();
        let active_epochs = active_textures
            .entry(document_id)
            .or_insert_with(|| FastHashMap::new());
        let active_textures_for_epoch = active_epochs
            .entry(epoch)
            .or_insert_with(|| FastHashMap::new());
        active_textures_for_epoch.insert(external_image_id, texture);
    }

    external_image_id
}

/// Destroys all textures from the given `document_id`
/// where the texture is **older** than the given `epoch`.
pub fn gl_textures_remove_epochs_from_pipeline(document_id: &DocumentId, epoch: Epoch) {
    // TODO: Handle overflow of Epochs correctly (low priority)
    unsafe {
        let active_textures = match ACTIVE_GL_TEXTURES.as_mut() {
            Some(s) => s,
            None => return,
        };

        let active_epochs = match active_textures.get_mut(document_id) {
            Some(s) => s,
            None => return,
        };

        // NOTE: original code used retain() but that
        // doesn't work on no_std
        let mut epochs_to_remove = Vec::new();

        for (gl_texture_epoch, _) in active_epochs.iter() {
            if *gl_texture_epoch < epoch {
                epochs_to_remove.push(*gl_texture_epoch);
            }
        }

        for epoch in epochs_to_remove {
            active_epochs.remove(&epoch);
        }
    }
}

// document_id, epoch, external_image_id
pub fn remove_single_texture_from_active_gl_textures(
    document_id: &DocumentId,
    epoch: &Epoch,
    external_image_id: &ExternalImageId,
) -> Option<()> {
    let mut active_textures = unsafe { ACTIVE_GL_TEXTURES.as_mut()? };
    let mut epochs = active_textures.get_mut(document_id)?;
    let mut images_in_epoch = epochs.get_mut(epoch)?;
    images_in_epoch.remove(external_image_id);
    Some(())
}

/// Removes a DocumentId from the active epochs
pub fn gl_textures_remove_active_pipeline(document_id: &DocumentId) {
    unsafe {
        let active_textures = match ACTIVE_GL_TEXTURES.as_mut() {
            Some(s) => s,
            None => return,
        };
        active_textures.remove(document_id);
    }
}

/// Destroys all textures, usually done before destroying the OpenGL context
pub fn gl_textures_clear_opengl_cache() {
    unsafe {
        ACTIVE_GL_TEXTURES = None;
    }
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
    active_textures
        .values()
        .flat_map(|active_document| active_document.values())
        .find_map(|active_epoch| active_epoch.get(image_key))
        .map(|tex| {
            (
                tex.texture_id,
                (tex.size.width as f32, tex.size.height as f32),
            )
        })
}

/// For .get_gl_precision_format(), but ABI-safe - returning an array or a tuple is not ABI-safe
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct GlShaderPrecisionFormatReturn {
    pub _0: GLint,
    pub _1: GLint,
    pub _2: GLint,
}

#[repr(C)]
pub struct GlContextPtr {
    pub ptr: Box<Rc<GlContextPtrInner>>,
    /// Whether to force a hardware or software renderer
    pub renderer_type: RendererType,
    pub run_destructor: bool,
}

impl Clone for GlContextPtr {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr.clone(),
            renderer_type: self.renderer_type.clone(),
            run_destructor: true,
        }
    }
}

impl Drop for GlContextPtr {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

impl GlContextPtr {
    pub fn get_svg_shader(&self) -> GLuint {
        self.ptr.svg_shader
    }
    pub fn get_fxaa_shader(&self) -> GLuint {
        self.ptr.fxaa_shader
    }
}

#[repr(C)]
pub struct GlContextPtrInner {
    pub ptr: Rc<GenericGlContext>,
    /// SVG shader program (library-internal use)
    pub svg_shader: GLuint,
    /// SVG multicolor shader program (library-internal use)
    pub svg_multicolor_shader: GLuint,
    /// FXAA shader program (library-internal use)
    pub fxaa_shader: GLuint,
}

impl Drop for GlContextPtrInner {
    fn drop(&mut self) {
        self.ptr.delete_program(self.svg_shader);
        self.ptr.delete_program(self.svg_multicolor_shader);
        self.ptr.delete_program(self.fxaa_shader);
    }
}

impl_option!(
    GlContextPtr,
    OptionGlContextPtr,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord]
);

impl core::fmt::Debug for GlContextPtr {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "0x{:0x}", self.as_usize())
    }
}

static SVG_VERTEX_SHADER: &[u8] = b"#version 150

#if __VERSION__ != 100
    #define varying out
    #define attribute in
#endif

uniform vec2 vBboxSize;
uniform mat4 vTransformMatrix;

attribute vec2 vAttrXY;

void main() {
    vec4 vTransposed = vec4(vAttrXY, 1.0, 1.0) * vTransformMatrix;
    vec2 vTransposedInScreen = vTransposed.xy / vBboxSize;
    vec2 vCalcFinal = (vTransposedInScreen * vec2(2.0)) - vec2(1.0);
    gl_Position = vec4(vCalcFinal, 1.0, 1.0);
}";

static SVG_FRAGMENT_SHADER: &[u8] = b"#version 150

precision highp float;

uniform vec4 fDrawColor;

#if __VERSION__ == 100
    #define oFragColor gl_FragColor
#else
    out vec4 oFragColor;
#endif

void main() {
    oFragColor = fDrawColor;
}";

static SVG_MULTICOLOR_VERTEX_SHADER: &[u8] = b"#version 150

#if __VERSION__ != 100
    #define varying out
    #define attribute in
#endif

uniform vec2 vBboxSize;
uniform mat4 vTransformMatrix;

attribute vec3 vAttrXY;
attribute vec4 vColor;
varying vec4 fColor;

void main() {
    vec4 vTransposed = vec4(vAttrXY.xy, 1.0, 1.0) * vTransformMatrix;
    vec2 vTransposedInScreen = vTransposed.xy / vBboxSize;
    vec2 vCalcFinal = (vTransposedInScreen * vec2(2.0)) - vec2(1.0);
    gl_Position = vec4(vCalcFinal, vAttrXY.z, 1.0);
    fColor = vColor;
}";

static SVG_MULTICOLOR_FRAGMENT_SHADER: &[u8] = b"#version 150

precision highp float;

#if __VERSION__ != 100
    #define varying out
    #define attribute in
#endif

#if __VERSION__ == 100
    #define oFragColor gl_FragColor
#else
    out vec4 oFragColor;
#endif

attribute vec4 fColor;

void main() {
    oFragColor = fColor;
}";

impl GlContextPtr {
    pub fn new(renderer_type: RendererType, gl_context: Rc<GenericGlContext>) -> Self {
        // Compile basic shader
        let vertex_shader_object = gl_context.create_shader(gl::VERTEX_SHADER);
        gl_context.shader_source(vertex_shader_object, &[SVG_VERTEX_SHADER]);
        gl_context.compile_shader(vertex_shader_object);

        let fragment_shader_object = gl_context.create_shader(gl::FRAGMENT_SHADER);
        gl_context.shader_source(fragment_shader_object, &[SVG_FRAGMENT_SHADER]);
        gl_context.compile_shader(fragment_shader_object);

        let svg_program_id = gl_context.create_program();

        gl_context.attach_shader(svg_program_id, vertex_shader_object);
        gl_context.attach_shader(svg_program_id, fragment_shader_object);
        gl_context.bind_attrib_location(svg_program_id, 0, "vAttrXY".into());
        gl_context.link_program(svg_program_id);

        gl_context.delete_shader(vertex_shader_object);
        gl_context.delete_shader(fragment_shader_object);

        // Compile multi-color SVG shader
        let vertex_shader_object = gl_context.create_shader(gl::VERTEX_SHADER);
        gl_context.shader_source(vertex_shader_object, &[SVG_MULTICOLOR_VERTEX_SHADER]);
        gl_context.compile_shader(vertex_shader_object);

        let fragment_shader_object = gl_context.create_shader(gl::FRAGMENT_SHADER);
        gl_context.shader_source(fragment_shader_object, &[SVG_MULTICOLOR_FRAGMENT_SHADER]);
        gl_context.compile_shader(fragment_shader_object);

        let svg_multicolor_program_id = gl_context.create_program();

        gl_context.attach_shader(svg_multicolor_program_id, vertex_shader_object);
        gl_context.attach_shader(svg_multicolor_program_id, fragment_shader_object);
        gl_context.bind_attrib_location(svg_multicolor_program_id, 0, "vAttrXY".into());
        gl_context.bind_attrib_location(svg_multicolor_program_id, 1, "vColor".into());
        gl_context.link_program(svg_multicolor_program_id);

        gl_context.delete_shader(vertex_shader_object);
        gl_context.delete_shader(fragment_shader_object);

        // Compile FXAA shader
        use crate::gl_fxaa::{FXAA_FRAGMENT_SHADER, FXAA_VERTEX_SHADER};

        let vertex_shader_object = gl_context.create_shader(gl::VERTEX_SHADER);
        gl_context.shader_source(vertex_shader_object, &[FXAA_VERTEX_SHADER]);
        gl_context.compile_shader(vertex_shader_object);

        let fragment_shader_object = gl_context.create_shader(gl::FRAGMENT_SHADER);
        gl_context.shader_source(fragment_shader_object, &[FXAA_FRAGMENT_SHADER]);
        gl_context.compile_shader(fragment_shader_object);

        let fxaa_program_id = gl_context.create_program();

        gl_context.attach_shader(fxaa_program_id, vertex_shader_object);
        gl_context.attach_shader(fxaa_program_id, fragment_shader_object);
        gl_context.bind_attrib_location(fxaa_program_id, 0, "vAttrXY".into());
        gl_context.link_program(fxaa_program_id);

        gl_context.delete_shader(vertex_shader_object);
        gl_context.delete_shader(fragment_shader_object);

        Self {
            ptr: Box::new(Rc::new(GlContextPtrInner {
                svg_shader: svg_program_id,
                svg_multicolor_shader: svg_multicolor_program_id,
                fxaa_shader: fxaa_program_id,
                ptr: gl_context,
            })),
            renderer_type,
            run_destructor: true,
        }
    }

    pub fn get<'a>(&'a self) -> &'a Rc<GenericGlContext> {
        &self.ptr.ptr
    }
    fn as_usize(&self) -> usize {
        (Rc::as_ptr(&self.ptr.ptr) as *const c_void) as usize
    }
}

impl GlContextPtr {
    pub fn get_type(&self) -> GlType {
        self.get().get_type().into()
    }
    pub fn buffer_data_untyped(
        &self,
        target: GLenum,
        size: GLsizeiptr,
        data: GlVoidPtrConst,
        usage: GLenum,
    ) {
        self.get()
            .buffer_data_untyped(target, size, data.ptr, usage)
    }
    pub fn buffer_sub_data_untyped(
        &self,
        target: GLenum,
        offset: isize,
        size: GLsizeiptr,
        data: GlVoidPtrConst,
    ) {
        self.get()
            .buffer_sub_data_untyped(target, offset, size, data.ptr)
    }
    pub fn map_buffer(&self, target: GLenum, access: GLbitfield) -> GlVoidPtrMut {
        GlVoidPtrMut {
            ptr: self.get().map_buffer(target, access),
        }
    }
    pub fn map_buffer_range(
        &self,
        target: GLenum,
        offset: GLintptr,
        length: GLsizeiptr,
        access: GLbitfield,
    ) -> GlVoidPtrMut {
        GlVoidPtrMut {
            ptr: self.get().map_buffer_range(target, offset, length, access),
        }
    }
    pub fn unmap_buffer(&self, target: GLenum) -> GLboolean {
        self.get().unmap_buffer(target)
    }
    pub fn tex_buffer(&self, target: GLenum, internal_format: GLenum, buffer: GLuint) {
        self.get().tex_buffer(target, internal_format, buffer)
    }
    pub fn shader_source(&self, shader: GLuint, strings: StringVec) {
        fn str_to_bytes(input: &str) -> Vec<u8> {
            let mut v: Vec<u8> = input.into();
            v.push(0);
            v
        }
        let shaders_as_bytes = strings
            .iter()
            .map(|s| str_to_bytes(s.as_str()))
            .collect::<Vec<_>>();
        let shaders_as_bytes = shaders_as_bytes
            .iter()
            .map(|s| s.as_ref())
            .collect::<Vec<_>>();
        self.get().shader_source(shader, &shaders_as_bytes)
    }
    pub fn read_buffer(&self, mode: GLenum) {
        self.get().read_buffer(mode)
    }
    pub fn read_pixels_into_buffer(
        &self,
        x: GLint,
        y: GLint,
        width: GLsizei,
        height: GLsizei,
        format: GLenum,
        pixel_type: GLenum,
        mut dst_buffer: U8VecRefMut,
    ) {
        self.get().read_pixels_into_buffer(
            x,
            y,
            width,
            height,
            format,
            pixel_type,
            dst_buffer.as_mut_slice(),
        )
    }
    pub fn read_pixels(
        &self,
        x: GLint,
        y: GLint,
        width: GLsizei,
        height: GLsizei,
        format: GLenum,
        pixel_type: GLenum,
    ) -> U8Vec {
        self.get()
            .read_pixels(x, y, width, height, format, pixel_type)
            .into()
    }
    pub fn read_pixels_into_pbo(
        &self,
        x: GLint,
        y: GLint,
        width: GLsizei,
        height: GLsizei,
        format: GLenum,
        pixel_type: GLenum,
    ) {
        unsafe {
            self.get()
                .read_pixels_into_pbo(x, y, width, height, format, pixel_type)
        }
    }
    pub fn sample_coverage(&self, value: GLclampf, invert: bool) {
        self.get().sample_coverage(value, invert)
    }
    pub fn polygon_offset(&self, factor: GLfloat, units: GLfloat) {
        self.get().polygon_offset(factor, units)
    }
    pub fn pixel_store_i(&self, name: GLenum, param: GLint) {
        self.get().pixel_store_i(name, param)
    }
    pub fn gen_buffers(&self, n: GLsizei) -> GLuintVec {
        self.get().gen_buffers(n).into()
    }
    pub fn gen_renderbuffers(&self, n: GLsizei) -> GLuintVec {
        self.get().gen_renderbuffers(n).into()
    }
    pub fn gen_framebuffers(&self, n: GLsizei) -> GLuintVec {
        self.get().gen_framebuffers(n).into()
    }
    pub fn gen_textures(&self, n: GLsizei) -> GLuintVec {
        self.get().gen_textures(n).into()
    }
    pub fn gen_vertex_arrays(&self, n: GLsizei) -> GLuintVec {
        self.get().gen_vertex_arrays(n).into()
    }
    pub fn gen_queries(&self, n: GLsizei) -> GLuintVec {
        self.get().gen_queries(n).into()
    }
    pub fn begin_query(&self, target: GLenum, id: GLuint) {
        self.get().begin_query(target, id)
    }
    pub fn end_query(&self, target: GLenum) {
        self.get().end_query(target)
    }
    pub fn query_counter(&self, id: GLuint, target: GLenum) {
        self.get().query_counter(id, target)
    }
    pub fn get_query_object_iv(&self, id: GLuint, pname: GLenum) -> i32 {
        self.get().get_query_object_iv(id, pname)
    }
    pub fn get_query_object_uiv(&self, id: GLuint, pname: GLenum) -> u32 {
        self.get().get_query_object_uiv(id, pname)
    }
    pub fn get_query_object_i64v(&self, id: GLuint, pname: GLenum) -> i64 {
        self.get().get_query_object_i64v(id, pname)
    }
    pub fn get_query_object_ui64v(&self, id: GLuint, pname: GLenum) -> u64 {
        self.get().get_query_object_ui64v(id, pname)
    }
    pub fn delete_queries(&self, queries: GLuintVecRef) {
        self.get().delete_queries(queries.as_slice())
    }
    pub fn delete_vertex_arrays(&self, vertex_arrays: GLuintVecRef) {
        self.get().delete_vertex_arrays(vertex_arrays.as_slice())
    }
    pub fn delete_buffers(&self, buffers: GLuintVecRef) {
        self.get().delete_buffers(buffers.as_slice())
    }
    pub fn delete_renderbuffers(&self, renderbuffers: GLuintVecRef) {
        self.get().delete_renderbuffers(renderbuffers.as_slice())
    }
    pub fn delete_framebuffers(&self, framebuffers: GLuintVecRef) {
        self.get().delete_framebuffers(framebuffers.as_slice())
    }
    pub fn delete_textures(&self, textures: GLuintVecRef) {
        self.get().delete_textures(textures.as_slice())
    }
    pub fn framebuffer_renderbuffer(
        &self,
        target: GLenum,
        attachment: GLenum,
        renderbuffertarget: GLenum,
        renderbuffer: GLuint,
    ) {
        self.get()
            .framebuffer_renderbuffer(target, attachment, renderbuffertarget, renderbuffer)
    }
    pub fn renderbuffer_storage(
        &self,
        target: GLenum,
        internalformat: GLenum,
        width: GLsizei,
        height: GLsizei,
    ) {
        self.get()
            .renderbuffer_storage(target, internalformat, width, height)
    }
    pub fn depth_func(&self, func: GLenum) {
        self.get().depth_func(func)
    }
    pub fn active_texture(&self, texture: GLenum) {
        self.get().active_texture(texture)
    }
    pub fn attach_shader(&self, program: GLuint, shader: GLuint) {
        self.get().attach_shader(program, shader)
    }
    pub fn bind_attrib_location(&self, program: GLuint, index: GLuint, name: &str) {
        self.get()
            .bind_attrib_location(program, index, name)
    }
    pub fn get_uniform_iv(&self, program: GLuint, location: GLint, mut result: GLintVecRefMut) {
        unsafe {
            self.get()
                .get_uniform_iv(program, location, result.as_mut_slice())
        }
    }
    pub fn get_uniform_fv(&self, program: GLuint, location: GLint, mut result: GLfloatVecRefMut) {
        unsafe {
            self.get()
                .get_uniform_fv(program, location, result.as_mut_slice())
        }
    }
    pub fn get_uniform_block_index(&self, program: GLuint, name: &str) -> GLuint {
        self.get().get_uniform_block_index(program, name)
    }
    pub fn get_uniform_indices(&self, program: GLuint, names: RefstrVecRef) -> GLuintVec {
        let names_vec = names
            .as_slice()
            .iter()
            .map(|n| n.as_str())
            .collect::<Vec<_>>();
        self.get().get_uniform_indices(program, &names_vec).into()
    }
    pub fn bind_buffer_base(&self, target: GLenum, index: GLuint, buffer: GLuint) {
        self.get().bind_buffer_base(target, index, buffer)
    }
    pub fn bind_buffer_range(
        &self,
        target: GLenum,
        index: GLuint,
        buffer: GLuint,
        offset: GLintptr,
        size: GLsizeiptr,
    ) {
        self.get()
            .bind_buffer_range(target, index, buffer, offset, size)
    }
    pub fn uniform_block_binding(
        &self,
        program: GLuint,
        uniform_block_index: GLuint,
        uniform_block_binding: GLuint,
    ) {
        self.get()
            .uniform_block_binding(program, uniform_block_index, uniform_block_binding)
    }
    pub fn bind_buffer(&self, target: GLenum, buffer: GLuint) {
        self.get().bind_buffer(target, buffer)
    }
    pub fn bind_vertex_array(&self, vao: GLuint) {
        self.get().bind_vertex_array(vao)
    }
    pub fn bind_renderbuffer(&self, target: GLenum, renderbuffer: GLuint) {
        self.get().bind_renderbuffer(target, renderbuffer)
    }
    pub fn bind_framebuffer(&self, target: GLenum, framebuffer: GLuint) {
        self.get().bind_framebuffer(target, framebuffer)
    }
    pub fn bind_texture(&self, target: GLenum, texture: GLuint) {
        self.get().bind_texture(target, texture)
    }
    pub fn draw_buffers(&self, bufs: GLenumVecRef) {
        self.get().draw_buffers(bufs.as_slice())
    }
    pub fn tex_image_2d(
        &self,
        target: GLenum,
        level: GLint,
        internal_format: GLint,
        width: GLsizei,
        height: GLsizei,
        border: GLint,
        format: GLenum,
        ty: GLenum,
        opt_data: OptionU8VecRef,
    ) {
        let opt_data = opt_data.as_option();
        let opt_data: Option<&[u8]> = opt_data.map(|o| o.as_slice());
        self.get().tex_image_2d(
            target,
            level,
            internal_format,
            width,
            height,
            border,
            format,
            ty,
            opt_data,
        )
    }
    pub fn compressed_tex_image_2d(
        &self,
        target: GLenum,
        level: GLint,
        internal_format: GLenum,
        width: GLsizei,
        height: GLsizei,
        border: GLint,
        data: U8VecRef,
    ) {
        self.get().compressed_tex_image_2d(
            target,
            level,
            internal_format,
            width,
            height,
            border,
            data.as_slice(),
        )
    }
    pub fn compressed_tex_sub_image_2d(
        &self,
        target: GLenum,
        level: GLint,
        xoffset: GLint,
        yoffset: GLint,
        width: GLsizei,
        height: GLsizei,
        format: GLenum,
        data: U8VecRef,
    ) {
        self.get().compressed_tex_sub_image_2d(
            target,
            level,
            xoffset,
            yoffset,
            width,
            height,
            format,
            data.as_slice(),
        )
    }
    pub fn tex_image_3d(
        &self,
        target: GLenum,
        level: GLint,
        internal_format: GLint,
        width: GLsizei,
        height: GLsizei,
        depth: GLsizei,
        border: GLint,
        format: GLenum,
        ty: GLenum,
        opt_data: OptionU8VecRef,
    ) {
        let opt_data = opt_data.as_option();
        let opt_data: Option<&[u8]> = opt_data.map(|o| o.as_slice());
        self.get().tex_image_3d(
            target,
            level,
            internal_format,
            width,
            height,
            depth,
            border,
            format,
            ty,
            opt_data,
        )
    }
    pub fn copy_tex_image_2d(
        &self,
        target: GLenum,
        level: GLint,
        internal_format: GLenum,
        x: GLint,
        y: GLint,
        width: GLsizei,
        height: GLsizei,
        border: GLint,
    ) {
        self.get()
            .copy_tex_image_2d(target, level, internal_format, x, y, width, height, border)
    }
    pub fn copy_tex_sub_image_2d(
        &self,
        target: GLenum,
        level: GLint,
        xoffset: GLint,
        yoffset: GLint,
        x: GLint,
        y: GLint,
        width: GLsizei,
        height: GLsizei,
    ) {
        self.get()
            .copy_tex_sub_image_2d(target, level, xoffset, yoffset, x, y, width, height)
    }
    pub fn copy_tex_sub_image_3d(
        &self,
        target: GLenum,
        level: GLint,
        xoffset: GLint,
        yoffset: GLint,
        zoffset: GLint,
        x: GLint,
        y: GLint,
        width: GLsizei,
        height: GLsizei,
    ) {
        self.get().copy_tex_sub_image_3d(
            target, level, xoffset, yoffset, zoffset, x, y, width, height,
        )
    }
    pub fn tex_sub_image_2d(
        &self,
        target: GLenum,
        level: GLint,
        xoffset: GLint,
        yoffset: GLint,
        width: GLsizei,
        height: GLsizei,
        format: GLenum,
        ty: GLenum,
        data: U8VecRef,
    ) {
        self.get().tex_sub_image_2d(
            target,
            level,
            xoffset,
            yoffset,
            width,
            height,
            format,
            ty,
            data.as_slice(),
        )
    }
    pub fn tex_sub_image_2d_pbo(
        &self,
        target: GLenum,
        level: GLint,
        xoffset: GLint,
        yoffset: GLint,
        width: GLsizei,
        height: GLsizei,
        format: GLenum,
        ty: GLenum,
        offset: usize,
    ) {
        self.get().tex_sub_image_2d_pbo(
            target, level, xoffset, yoffset, width, height, format, ty, offset,
        )
    }
    pub fn tex_sub_image_3d(
        &self,
        target: GLenum,
        level: GLint,
        xoffset: GLint,
        yoffset: GLint,
        zoffset: GLint,
        width: GLsizei,
        height: GLsizei,
        depth: GLsizei,
        format: GLenum,
        ty: GLenum,
        data: U8VecRef,
    ) {
        self.get().tex_sub_image_3d(
            target,
            level,
            xoffset,
            yoffset,
            zoffset,
            width,
            height,
            depth,
            format,
            ty,
            data.as_slice(),
        )
    }
    pub fn tex_sub_image_3d_pbo(
        &self,
        target: GLenum,
        level: GLint,
        xoffset: GLint,
        yoffset: GLint,
        zoffset: GLint,
        width: GLsizei,
        height: GLsizei,
        depth: GLsizei,
        format: GLenum,
        ty: GLenum,
        offset: usize,
    ) {
        self.get().tex_sub_image_3d_pbo(
            target, level, xoffset, yoffset, zoffset, width, height, depth, format, ty, offset,
        )
    }
    pub fn tex_storage_2d(
        &self,
        target: GLenum,
        levels: GLint,
        internal_format: GLenum,
        width: GLsizei,
        height: GLsizei,
    ) {
        self.get()
            .tex_storage_2d(target, levels, internal_format, width, height)
    }
    pub fn tex_storage_3d(
        &self,
        target: GLenum,
        levels: GLint,
        internal_format: GLenum,
        width: GLsizei,
        height: GLsizei,
        depth: GLsizei,
    ) {
        self.get()
            .tex_storage_3d(target, levels, internal_format, width, height, depth)
    }
    pub fn get_tex_image_into_buffer(
        &self,
        target: GLenum,
        level: GLint,
        format: GLenum,
        ty: GLenum,
        mut output: U8VecRefMut,
    ) {
        self.get()
            .get_tex_image_into_buffer(target, level, format, ty, output.as_mut_slice())
    }
    pub fn copy_image_sub_data(
        &self,
        src_name: GLuint,
        src_target: GLenum,
        src_level: GLint,
        src_x: GLint,
        src_y: GLint,
        src_z: GLint,
        dst_name: GLuint,
        dst_target: GLenum,
        dst_level: GLint,
        dst_x: GLint,
        dst_y: GLint,
        dst_z: GLint,
        src_width: GLsizei,
        src_height: GLsizei,
        src_depth: GLsizei,
    ) {
        unsafe {
            self.get().copy_image_sub_data(
                src_name, src_target, src_level, src_x, src_y, src_z, dst_name, dst_target,
                dst_level, dst_x, dst_y, dst_z, src_width, src_height, src_depth,
            )
        }
    }
    pub fn invalidate_framebuffer(&self, target: GLenum, attachments: GLenumVecRef) {
        self.get()
            .invalidate_framebuffer(target, attachments.as_slice())
    }
    pub fn invalidate_sub_framebuffer(
        &self,
        target: GLenum,
        attachments: GLenumVecRef,
        xoffset: GLint,
        yoffset: GLint,
        width: GLsizei,
        height: GLsizei,
    ) {
        self.get().invalidate_sub_framebuffer(
            target,
            attachments.as_slice(),
            xoffset,
            yoffset,
            width,
            height,
        )
    }
    pub fn get_integer_v(&self, name: GLenum, mut result: GLintVecRefMut) {
        unsafe { self.get().get_integer_v(name, result.as_mut_slice()) }
    }
    pub fn get_integer_64v(&self, name: GLenum, mut result: GLint64VecRefMut) {
        unsafe { self.get().get_integer_64v(name, result.as_mut_slice()) }
    }
    pub fn get_integer_iv(&self, name: GLenum, index: GLuint, mut result: GLintVecRefMut) {
        unsafe {
            self.get()
                .get_integer_iv(name, index, result.as_mut_slice())
        }
    }
    pub fn get_integer_64iv(&self, name: GLenum, index: GLuint, mut result: GLint64VecRefMut) {
        unsafe {
            self.get()
                .get_integer_64iv(name, index, result.as_mut_slice())
        }
    }
    pub fn get_boolean_v(&self, name: GLenum, mut result: GLbooleanVecRefMut) {
        unsafe { self.get().get_boolean_v(name, result.as_mut_slice()) }
    }
    pub fn get_float_v(&self, name: GLenum, mut result: GLfloatVecRefMut) {
        unsafe { self.get().get_float_v(name, result.as_mut_slice()) }
    }
    pub fn get_framebuffer_attachment_parameter_iv(
        &self,
        target: GLenum,
        attachment: GLenum,
        pname: GLenum,
    ) -> GLint {
        self.get()
            .get_framebuffer_attachment_parameter_iv(target, attachment, pname)
    }
    pub fn get_renderbuffer_parameter_iv(&self, target: GLenum, pname: GLenum) -> GLint {
        self.get().get_renderbuffer_parameter_iv(target, pname)
    }
    pub fn get_tex_parameter_iv(&self, target: GLenum, name: GLenum) -> GLint {
        self.get().get_tex_parameter_iv(target, name)
    }
    pub fn get_tex_parameter_fv(&self, target: GLenum, name: GLenum) -> GLfloat {
        self.get().get_tex_parameter_fv(target, name)
    }
    pub fn tex_parameter_i(&self, target: GLenum, pname: GLenum, param: GLint) {
        self.get().tex_parameter_i(target, pname, param)
    }
    pub fn tex_parameter_f(&self, target: GLenum, pname: GLenum, param: GLfloat) {
        self.get().tex_parameter_f(target, pname, param)
    }
    pub fn framebuffer_texture_2d(
        &self,
        target: GLenum,
        attachment: GLenum,
        textarget: GLenum,
        texture: GLuint,
        level: GLint,
    ) {
        self.get()
            .framebuffer_texture_2d(target, attachment, textarget, texture, level)
    }
    pub fn framebuffer_texture_layer(
        &self,
        target: GLenum,
        attachment: GLenum,
        texture: GLuint,
        level: GLint,
        layer: GLint,
    ) {
        self.get()
            .framebuffer_texture_layer(target, attachment, texture, level, layer)
    }
    pub fn blit_framebuffer(
        &self,
        src_x0: GLint,
        src_y0: GLint,
        src_x1: GLint,
        src_y1: GLint,
        dst_x0: GLint,
        dst_y0: GLint,
        dst_x1: GLint,
        dst_y1: GLint,
        mask: GLbitfield,
        filter: GLenum,
    ) {
        self.get().blit_framebuffer(
            src_x0, src_y0, src_x1, src_y1, dst_x0, dst_y0, dst_x1, dst_y1, mask, filter,
        )
    }
    pub fn vertex_attrib_4f(&self, index: GLuint, x: GLfloat, y: GLfloat, z: GLfloat, w: GLfloat) {
        self.get().vertex_attrib_4f(index, x, y, z, w)
    }
    pub fn vertex_attrib_pointer_f32(
        &self,
        index: GLuint,
        size: GLint,
        normalized: bool,
        stride: GLsizei,
        offset: GLuint,
    ) {
        self.get()
            .vertex_attrib_pointer_f32(index, size, normalized, stride, offset)
    }
    pub fn vertex_attrib_pointer(
        &self,
        index: GLuint,
        size: GLint,
        type_: GLenum,
        normalized: bool,
        stride: GLsizei,
        offset: GLuint,
    ) {
        self.get()
            .vertex_attrib_pointer(index, size, type_, normalized, stride, offset)
    }
    pub fn vertex_attrib_i_pointer(
        &self,
        index: GLuint,
        size: GLint,
        type_: GLenum,
        stride: GLsizei,
        offset: GLuint,
    ) {
        self.get()
            .vertex_attrib_i_pointer(index, size, type_, stride, offset)
    }
    pub fn vertex_attrib_divisor(&self, index: GLuint, divisor: GLuint) {
        self.get().vertex_attrib_divisor(index, divisor)
    }
    pub fn viewport(&self, x: GLint, y: GLint, width: GLsizei, height: GLsizei) {
        self.get().viewport(x, y, width, height)
    }
    pub fn scissor(&self, x: GLint, y: GLint, width: GLsizei, height: GLsizei) {
        self.get().scissor(x, y, width, height)
    }
    pub fn line_width(&self, width: GLfloat) {
        self.get().line_width(width)
    }
    pub fn use_program(&self, program: GLuint) {
        self.get().use_program(program)
    }
    pub fn validate_program(&self, program: GLuint) {
        self.get().validate_program(program)
    }
    pub fn draw_arrays(&self, mode: GLenum, first: GLint, count: GLsizei) {
        self.get().draw_arrays(mode, first, count)
    }
    pub fn draw_arrays_instanced(
        &self,
        mode: GLenum,
        first: GLint,
        count: GLsizei,
        primcount: GLsizei,
    ) {
        self.get()
            .draw_arrays_instanced(mode, first, count, primcount)
    }
    pub fn draw_elements(
        &self,
        mode: GLenum,
        count: GLsizei,
        element_type: GLenum,
        indices_offset: GLuint,
    ) {
        self.get()
            .draw_elements(mode, count, element_type, indices_offset)
    }
    pub fn draw_elements_instanced(
        &self,
        mode: GLenum,
        count: GLsizei,
        element_type: GLenum,
        indices_offset: GLuint,
        primcount: GLsizei,
    ) {
        self.get()
            .draw_elements_instanced(mode, count, element_type, indices_offset, primcount)
    }
    pub fn blend_color(&self, r: f32, g: f32, b: f32, a: f32) {
        self.get().blend_color(r, g, b, a)
    }
    pub fn blend_func(&self, sfactor: GLenum, dfactor: GLenum) {
        self.get().blend_func(sfactor, dfactor)
    }
    pub fn blend_func_separate(
        &self,
        src_rgb: GLenum,
        dest_rgb: GLenum,
        src_alpha: GLenum,
        dest_alpha: GLenum,
    ) {
        self.get()
            .blend_func_separate(src_rgb, dest_rgb, src_alpha, dest_alpha)
    }
    pub fn blend_equation(&self, mode: GLenum) {
        self.get().blend_equation(mode)
    }
    pub fn blend_equation_separate(&self, mode_rgb: GLenum, mode_alpha: GLenum) {
        self.get().blend_equation_separate(mode_rgb, mode_alpha)
    }
    pub fn color_mask(&self, r: bool, g: bool, b: bool, a: bool) {
        self.get().color_mask(r, g, b, a)
    }
    pub fn cull_face(&self, mode: GLenum) {
        self.get().cull_face(mode)
    }
    pub fn front_face(&self, mode: GLenum) {
        self.get().front_face(mode)
    }
    pub fn enable(&self, cap: GLenum) {
        self.get().enable(cap)
    }
    pub fn disable(&self, cap: GLenum) {
        self.get().disable(cap)
    }
    pub fn hint(&self, param_name: GLenum, param_val: GLenum) {
        self.get().hint(param_name, param_val)
    }
    pub fn is_enabled(&self, cap: GLenum) -> GLboolean {
        self.get().is_enabled(cap)
    }
    pub fn is_shader(&self, shader: GLuint) -> GLboolean {
        self.get().is_shader(shader)
    }
    pub fn is_texture(&self, texture: GLenum) -> GLboolean {
        self.get().is_texture(texture)
    }
    pub fn is_framebuffer(&self, framebuffer: GLenum) -> GLboolean {
        self.get().is_framebuffer(framebuffer)
    }
    pub fn is_renderbuffer(&self, renderbuffer: GLenum) -> GLboolean {
        self.get().is_renderbuffer(renderbuffer)
    }
    pub fn check_frame_buffer_status(&self, target: GLenum) -> GLenum {
        self.get().check_frame_buffer_status(target)
    }
    pub fn enable_vertex_attrib_array(&self, index: GLuint) {
        self.get().enable_vertex_attrib_array(index)
    }
    pub fn disable_vertex_attrib_array(&self, index: GLuint) {
        self.get().disable_vertex_attrib_array(index)
    }
    pub fn uniform_1f(&self, location: GLint, v0: GLfloat) {
        self.get().uniform_1f(location, v0)
    }
    pub fn uniform_1fv(&self, location: GLint, values: F32VecRef) {
        self.get().uniform_1fv(location, values.as_slice())
    }
    pub fn uniform_1i(&self, location: GLint, v0: GLint) {
        self.get().uniform_1i(location, v0)
    }
    pub fn uniform_1iv(&self, location: GLint, values: I32VecRef) {
        self.get().uniform_1iv(location, values.as_slice())
    }
    pub fn uniform_1ui(&self, location: GLint, v0: GLuint) {
        self.get().uniform_1ui(location, v0)
    }
    pub fn uniform_2f(&self, location: GLint, v0: GLfloat, v1: GLfloat) {
        self.get().uniform_2f(location, v0, v1)
    }
    pub fn uniform_2fv(&self, location: GLint, values: F32VecRef) {
        self.get().uniform_2fv(location, values.as_slice())
    }
    pub fn uniform_2i(&self, location: GLint, v0: GLint, v1: GLint) {
        self.get().uniform_2i(location, v0, v1)
    }
    pub fn uniform_2iv(&self, location: GLint, values: I32VecRef) {
        self.get().uniform_2iv(location, values.as_slice())
    }
    pub fn uniform_2ui(&self, location: GLint, v0: GLuint, v1: GLuint) {
        self.get().uniform_2ui(location, v0, v1)
    }
    pub fn uniform_3f(&self, location: GLint, v0: GLfloat, v1: GLfloat, v2: GLfloat) {
        self.get().uniform_3f(location, v0, v1, v2)
    }
    pub fn uniform_3fv(&self, location: GLint, values: F32VecRef) {
        self.get().uniform_3fv(location, values.as_slice())
    }
    pub fn uniform_3i(&self, location: GLint, v0: GLint, v1: GLint, v2: GLint) {
        self.get().uniform_3i(location, v0, v1, v2)
    }
    pub fn uniform_3iv(&self, location: GLint, values: I32VecRef) {
        self.get().uniform_3iv(location, values.as_slice())
    }
    pub fn uniform_3ui(&self, location: GLint, v0: GLuint, v1: GLuint, v2: GLuint) {
        self.get().uniform_3ui(location, v0, v1, v2)
    }
    pub fn uniform_4f(&self, location: GLint, x: GLfloat, y: GLfloat, z: GLfloat, w: GLfloat) {
        self.get().uniform_4f(location, x, y, z, w)
    }
    pub fn uniform_4i(&self, location: GLint, x: GLint, y: GLint, z: GLint, w: GLint) {
        self.get().uniform_4i(location, x, y, z, w)
    }
    pub fn uniform_4iv(&self, location: GLint, values: I32VecRef) {
        self.get().uniform_4iv(location, values.as_slice())
    }
    pub fn uniform_4ui(&self, location: GLint, x: GLuint, y: GLuint, z: GLuint, w: GLuint) {
        self.get().uniform_4ui(location, x, y, z, w)
    }
    pub fn uniform_4fv(&self, location: GLint, values: F32VecRef) {
        self.get().uniform_4fv(location, values.as_slice())
    }
    pub fn uniform_matrix_2fv(&self, location: GLint, transpose: bool, value: F32VecRef) {
        self.get()
            .uniform_matrix_2fv(location, transpose, value.as_slice())
    }
    pub fn uniform_matrix_3fv(&self, location: GLint, transpose: bool, value: F32VecRef) {
        self.get()
            .uniform_matrix_3fv(location, transpose, value.as_slice())
    }
    pub fn uniform_matrix_4fv(&self, location: GLint, transpose: bool, value: F32VecRef) {
        self.get()
            .uniform_matrix_4fv(location, transpose, value.as_slice())
    }
    pub fn depth_mask(&self, flag: bool) {
        self.get().depth_mask(flag)
    }
    pub fn depth_range(&self, near: f64, far: f64) {
        self.get().depth_range(near, far)
    }
    pub fn get_active_attrib(&self, program: GLuint, index: GLuint) -> GetActiveAttribReturn {
        let r = self.get().get_active_attrib(program, index);
        GetActiveAttribReturn {
            _0: r.0,
            _1: r.1,
            _2: r.2.into(),
        }
    }
    pub fn get_active_uniform(&self, program: GLuint, index: GLuint) -> GetActiveUniformReturn {
        let r = self.get().get_active_uniform(program, index);
        GetActiveUniformReturn {
            _0: r.0,
            _1: r.1,
            _2: r.2.into(),
        }
    }
    pub fn get_active_uniforms_iv(
        &self,
        program: GLuint,
        indices: GLuintVec,
        pname: GLenum,
    ) -> GLintVec {
        self.get()
            .get_active_uniforms_iv(program, indices.into_library_owned_vec(), pname)
            .into()
    }
    pub fn get_active_uniform_block_i(
        &self,
        program: GLuint,
        index: GLuint,
        pname: GLenum,
    ) -> GLint {
        self.get().get_active_uniform_block_i(program, index, pname)
    }
    pub fn get_active_uniform_block_iv(
        &self,
        program: GLuint,
        index: GLuint,
        pname: GLenum,
    ) -> GLintVec {
        self.get()
            .get_active_uniform_block_iv(program, index, pname)
            .into()
    }
    pub fn get_active_uniform_block_name(&self, program: GLuint, index: GLuint) -> AzString {
        self.get()
            .get_active_uniform_block_name(program, index)
            .into()
    }
    pub fn get_attrib_location(&self, program: GLuint, name: &str) -> c_int {
        self.get().get_attrib_location(program, name)
    }
    pub fn get_frag_data_location(&self, program: GLuint, name: &str) -> c_int {
        self.get().get_frag_data_location(program, name)
    }
    pub fn get_uniform_location(&self, program: GLuint, name: &str) -> c_int {
        self.get().get_uniform_location(program, name)
    }
    pub fn get_program_info_log(&self, program: GLuint) -> AzString {
        self.get().get_program_info_log(program).into()
    }
    pub fn get_program_iv(&self, program: GLuint, pname: GLenum, mut result: GLintVecRefMut) {
        unsafe {
            self.get()
                .get_program_iv(program, pname, result.as_mut_slice())
        }
    }
    pub fn get_program_binary(&self, program: GLuint) -> GetProgramBinaryReturn {
        let r = self.get().get_program_binary(program);
        GetProgramBinaryReturn {
            _0: r.0.into(),
            _1: r.1,
        }
    }
    pub fn program_binary(&self, program: GLuint, format: GLenum, binary: U8VecRef) {
        self.get()
            .program_binary(program, format, binary.as_slice())
    }
    pub fn program_parameter_i(&self, program: GLuint, pname: GLenum, value: GLint) {
        self.get().program_parameter_i(program, pname, value)
    }
    pub fn get_vertex_attrib_iv(&self, index: GLuint, pname: GLenum, mut result: GLintVecRefMut) {
        unsafe {
            self.get()
                .get_vertex_attrib_iv(index, pname, result.as_mut_slice())
        }
    }
    pub fn get_vertex_attrib_fv(&self, index: GLuint, pname: GLenum, mut result: GLfloatVecRefMut) {
        unsafe {
            self.get()
                .get_vertex_attrib_fv(index, pname, result.as_mut_slice())
        }
    }
    pub fn get_vertex_attrib_pointer_v(&self, index: GLuint, pname: GLenum) -> GLsizeiptr {
        self.get().get_vertex_attrib_pointer_v(index, pname)
    }
    pub fn get_buffer_parameter_iv(&self, target: GLuint, pname: GLenum) -> GLint {
        self.get().get_buffer_parameter_iv(target, pname)
    }
    pub fn get_shader_info_log(&self, shader: GLuint) -> AzString {
        self.get().get_shader_info_log(shader).into()
    }
    pub fn get_string(&self, which: GLenum) -> AzString {
        self.get().get_string(which).into()
    }
    pub fn get_string_i(&self, which: GLenum, index: GLuint) -> AzString {
        self.get().get_string_i(which, index).into()
    }
    pub fn get_shader_iv(&self, shader: GLuint, pname: GLenum, mut result: GLintVecRefMut) {
        unsafe {
            self.get()
                .get_shader_iv(shader, pname, result.as_mut_slice())
        }
    }
    pub fn get_shader_precision_format(
        &self,
        shader_type: GLuint,
        precision_type: GLuint,
    ) -> GlShaderPrecisionFormatReturn {
        let r = self
            .get()
            .get_shader_precision_format(shader_type, precision_type);
        GlShaderPrecisionFormatReturn {
            _0: r.0,
            _1: r.1,
            _2: r.2,
        }
    }
    pub fn compile_shader(&self, shader: GLuint) {
        self.get().compile_shader(shader)
    }
    pub fn create_program(&self) -> GLuint {
        self.get().create_program()
    }
    pub fn delete_program(&self, program: GLuint) {
        self.get().delete_program(program)
    }
    pub fn create_shader(&self, shader_type: GLenum) -> GLuint {
        self.get().create_shader(shader_type)
    }
    pub fn delete_shader(&self, shader: GLuint) {
        self.get().delete_shader(shader)
    }
    pub fn detach_shader(&self, program: GLuint, shader: GLuint) {
        self.get().detach_shader(program, shader)
    }
    pub fn link_program(&self, program: GLuint) {
        self.get().link_program(program)
    }
    pub fn clear_color(&self, r: f32, g: f32, b: f32, a: f32) {
        self.get().clear_color(r, g, b, a)
    }
    pub fn clear(&self, buffer_mask: GLbitfield) {
        self.get().clear(buffer_mask)
    }
    pub fn clear_depth(&self, depth: f64) {
        self.get().clear_depth(depth)
    }
    pub fn clear_stencil(&self, s: GLint) {
        self.get().clear_stencil(s)
    }
    pub fn flush(&self) {
        self.get().flush()
    }
    pub fn finish(&self) {
        self.get().finish()
    }
    pub fn get_error(&self) -> GLenum {
        self.get().get_error()
    }
    pub fn stencil_mask(&self, mask: GLuint) {
        self.get().stencil_mask(mask)
    }
    pub fn stencil_mask_separate(&self, face: GLenum, mask: GLuint) {
        self.get().stencil_mask_separate(face, mask)
    }
    pub fn stencil_func(&self, func: GLenum, ref_: GLint, mask: GLuint) {
        self.get().stencil_func(func, ref_, mask)
    }
    pub fn stencil_func_separate(&self, face: GLenum, func: GLenum, ref_: GLint, mask: GLuint) {
        self.get().stencil_func_separate(face, func, ref_, mask)
    }
    pub fn stencil_op(&self, sfail: GLenum, dpfail: GLenum, dppass: GLenum) {
        self.get().stencil_op(sfail, dpfail, dppass)
    }
    pub fn stencil_op_separate(&self, face: GLenum, sfail: GLenum, dpfail: GLenum, dppass: GLenum) {
        self.get().stencil_op_separate(face, sfail, dpfail, dppass)
    }
    pub fn egl_image_target_texture2d_oes(&self, target: GLenum, image: GlVoidPtrConst) {
        self.get()
            .egl_image_target_texture2d_oes(target, image.ptr as *const gl_context_loader::c_void)
    }
    pub fn generate_mipmap(&self, target: GLenum) {
        self.get().generate_mipmap(target)
    }
    pub fn insert_event_marker_ext(&self, message: &str) {
        self.get().insert_event_marker_ext(message)
    }
    pub fn push_group_marker_ext(&self, message: &str) {
        self.get().push_group_marker_ext(message)
    }
    pub fn pop_group_marker_ext(&self) {
        self.get().pop_group_marker_ext()
    }
    pub fn debug_message_insert_khr(
        &self,
        source: GLenum,
        type_: GLenum,
        id: GLuint,
        severity: GLenum,
        message: &str,
    ) {
        self.get()
            .debug_message_insert_khr(source, type_, id, severity, message)
    }
    pub fn push_debug_group_khr(&self, source: GLenum, id: GLuint, message: &str) {
        self.get()
            .push_debug_group_khr(source, id, message)
    }
    pub fn pop_debug_group_khr(&self) {
        self.get().pop_debug_group_khr()
    }
    pub fn fence_sync(&self, condition: GLenum, flags: GLbitfield) -> GLsyncPtr {
        GLsyncPtr::new(self.get().fence_sync(condition, flags))
    }
    pub fn client_wait_sync(&self, sync: GLsyncPtr, flags: GLbitfield, timeout: GLuint64) -> u32 {
        self.get().client_wait_sync(sync.get(), flags, timeout)
    }
    pub fn wait_sync(&self, sync: GLsyncPtr, flags: GLbitfield, timeout: GLuint64) {
        self.get().wait_sync(sync.get(), flags, timeout)
    }
    pub fn delete_sync(&self, sync: GLsyncPtr) {
        self.get().delete_sync(sync.get())
    }
    pub fn texture_range_apple(&self, target: GLenum, data: U8VecRef) {
        self.get().texture_range_apple(target, data.as_slice())
    }
    pub fn gen_fences_apple(&self, n: GLsizei) -> GLuintVec {
        self.get().gen_fences_apple(n).into()
    }
    pub fn delete_fences_apple(&self, fences: GLuintVecRef) {
        self.get().delete_fences_apple(fences.as_slice())
    }
    pub fn set_fence_apple(&self, fence: GLuint) {
        self.get().set_fence_apple(fence)
    }
    pub fn finish_fence_apple(&self, fence: GLuint) {
        self.get().finish_fence_apple(fence)
    }
    pub fn test_fence_apple(&self, fence: GLuint) {
        self.get().test_fence_apple(fence)
    }
    pub fn test_object_apple(&self, object: GLenum, name: GLuint) -> GLboolean {
        self.get().test_object_apple(object, name)
    }
    pub fn finish_object_apple(&self, object: GLenum, name: GLuint) {
        self.get().finish_object_apple(object, name)
    }
    pub fn get_frag_data_index(&self, program: GLuint, name: &str) -> GLint {
        self.get().get_frag_data_index(program, name)
    }
    pub fn blend_barrier_khr(&self) {
        self.get().blend_barrier_khr()
    }
    pub fn bind_frag_data_location_indexed(
        &self,
        program: GLuint,
        color_number: GLuint,
        index: GLuint,
        name: &str,
    ) {
        self.get()
            .bind_frag_data_location_indexed(program, color_number, index, name)
    }
    pub fn get_debug_messages(&self) -> DebugMessageVec {
        let dmv: Vec<DebugMessage> = self
            .get()
            .get_debug_messages()
            .into_iter()
            .map(|d| DebugMessage {
                message: d.message.into(),
                source: d.source,
                ty: d.ty,
                id: d.ty,
                severity: d.severity,
            })
            .collect();
        dmv.into()
    }
    pub fn provoking_vertex_angle(&self, mode: GLenum) {
        self.get().provoking_vertex_angle(mode)
    }
    pub fn gen_vertex_arrays_apple(&self, n: GLsizei) -> GLuintVec {
        self.get().gen_vertex_arrays_apple(n).into()
    }
    pub fn bind_vertex_array_apple(&self, vao: GLuint) {
        self.get().bind_vertex_array_apple(vao)
    }
    pub fn delete_vertex_arrays_apple(&self, vertex_arrays: GLuintVecRef) {
        self.get()
            .delete_vertex_arrays_apple(vertex_arrays.as_slice())
    }
    pub fn copy_texture_chromium(
        &self,
        source_id: GLuint,
        source_level: GLint,
        dest_target: GLenum,
        dest_id: GLuint,
        dest_level: GLint,
        internal_format: GLint,
        dest_type: GLenum,
        unpack_flip_y: GLboolean,
        unpack_premultiply_alpha: GLboolean,
        unpack_unmultiply_alpha: GLboolean,
    ) {
        self.get().copy_texture_chromium(
            source_id,
            source_level,
            dest_target,
            dest_id,
            dest_level,
            internal_format,
            dest_type,
            unpack_flip_y,
            unpack_premultiply_alpha,
            unpack_unmultiply_alpha,
        )
    }
    pub fn copy_sub_texture_chromium(
        &self,
        source_id: GLuint,
        source_level: GLint,
        dest_target: GLenum,
        dest_id: GLuint,
        dest_level: GLint,
        x_offset: GLint,
        y_offset: GLint,
        x: GLint,
        y: GLint,
        width: GLsizei,
        height: GLsizei,
        unpack_flip_y: GLboolean,
        unpack_premultiply_alpha: GLboolean,
        unpack_unmultiply_alpha: GLboolean,
    ) {
        self.get().copy_sub_texture_chromium(
            source_id,
            source_level,
            dest_target,
            dest_id,
            dest_level,
            x_offset,
            y_offset,
            x,
            y,
            width,
            height,
            unpack_flip_y,
            unpack_premultiply_alpha,
            unpack_unmultiply_alpha,
        )
    }
    pub fn egl_image_target_renderbuffer_storage_oes(&self, target: u32, image: GlVoidPtrConst) {
        self.get().egl_image_target_renderbuffer_storage_oes(
            target,
            image.ptr as *const gl_context_loader::c_void,
        )
    }
    pub fn copy_texture_3d_angle(
        &self,
        source_id: GLuint,
        source_level: GLint,
        dest_target: GLenum,
        dest_id: GLuint,
        dest_level: GLint,
        internal_format: GLint,
        dest_type: GLenum,
        unpack_flip_y: GLboolean,
        unpack_premultiply_alpha: GLboolean,
        unpack_unmultiply_alpha: GLboolean,
    ) {
        self.get().copy_texture_3d_angle(
            source_id,
            source_level,
            dest_target,
            dest_id,
            dest_level,
            internal_format,
            dest_type,
            unpack_flip_y,
            unpack_premultiply_alpha,
            unpack_unmultiply_alpha,
        )
    }
    pub fn copy_sub_texture_3d_angle(
        &self,
        source_id: GLuint,
        source_level: GLint,
        dest_target: GLenum,
        dest_id: GLuint,
        dest_level: GLint,
        x_offset: GLint,
        y_offset: GLint,
        z_offset: GLint,
        x: GLint,
        y: GLint,
        z: GLint,
        width: GLsizei,
        height: GLsizei,
        depth: GLsizei,
        unpack_flip_y: GLboolean,
        unpack_premultiply_alpha: GLboolean,
        unpack_unmultiply_alpha: GLboolean,
    ) {
        self.get().copy_sub_texture_3d_angle(
            source_id,
            source_level,
            dest_target,
            dest_id,
            dest_level,
            x_offset,
            y_offset,
            z_offset,
            x,
            y,
            z,
            width,
            height,
            depth,
            unpack_flip_y,
            unpack_premultiply_alpha,
            unpack_unmultiply_alpha,
        )
    }
    pub fn buffer_storage(
        &self,
        target: GLenum,
        size: GLsizeiptr,
        data: GlVoidPtrConst,
        flags: GLbitfield,
    ) {
        self.get().buffer_storage(target, size, data.ptr, flags)
    }
    pub fn flush_mapped_buffer_range(&self, target: GLenum, offset: GLintptr, length: GLsizeiptr) {
        self.get().flush_mapped_buffer_range(target, offset, length)
    }
}

impl PartialEq for GlContextPtr {
    fn eq(&self, rhs: &Self) -> bool {
        self.as_usize().eq(&rhs.as_usize())
    }
}

impl Eq for GlContextPtr {}

impl PartialOrd for GlContextPtr {
    fn partial_cmp(&self, rhs: &Self) -> Option<core::cmp::Ordering> {
        self.as_usize().partial_cmp(&rhs.as_usize())
    }
}

impl Ord for GlContextPtr {
    fn cmp(&self, rhs: &Self) -> core::cmp::Ordering {
        self.as_usize().cmp(&rhs.as_usize())
    }
}

/// OpenGL texture, use `ReadOnlyWindow::create_texture` to create a texture
#[repr(C)]
pub struct Texture {
    /// A reference-counted pointer to the OpenGL context (so that the texture can be deleted in
    /// the destructor)
    pub gl_context: GlContextPtr,
    /// Raw OpenGL texture ID
    pub texture_id: GLuint,
    /// Reference count, shared across
    pub refcount: *const AtomicUsize,
    /// Size of this texture (in pixels)
    pub size: PhysicalSizeU32,
    /// Format of the texture (rgba8, brga8, etc.)
    pub format: RawImageFormat,
    /// Background color of this texture
    pub background_color: ColorU,
    /// Hints and flags for optimization purposes
    pub flags: TextureFlags,
    pub run_destructor: bool,
}

impl Clone for Texture {
    fn clone(&self) -> Self {
        unsafe {
            (*self.refcount).fetch_add(1, AtomicOrdering::SeqCst);
        }
        Self {
            gl_context: self.gl_context.clone(),
            texture_id: self.texture_id.clone(),
            refcount: self.refcount,
            size: self.size.clone(),
            format: self.format.clone(),
            background_color: self.background_color.clone(),
            flags: self.flags.clone(),
            run_destructor: true,
        }
    }
}

impl_option!(
    Texture,
    OptionTexture,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl Texture {
    pub fn create(
        texture_id: GLuint,
        flags: TextureFlags,
        size: PhysicalSizeU32,
        background_color: ColorU,
        gl_context: GlContextPtr,
        format: RawImageFormat,
    ) -> Self {
        Self {
            texture_id,
            flags,
            size,
            background_color,
            gl_context,
            format,
            refcount: Box::into_raw(Box::new(AtomicUsize::new(1))),
            run_destructor: true,
        }
    }

    pub fn allocate_rgba8(
        gl_context: GlContextPtr,
        size: PhysicalSizeU32,
        background: ColorU,
    ) -> Self {
        let textures = gl_context.gen_textures(1);
        let texture_id = textures.as_ref()[0];

        let mut current_texture_2d = [0_i32];
        gl_context.get_integer_v(gl::TEXTURE_2D, (&mut current_texture_2d[..]).into());

        gl_context.bind_texture(gl::TEXTURE_2D, texture_id);
        gl_context.tex_image_2d(
            gl::TEXTURE_2D,
            0,
            gl::RGBA as i32,
            size.width as i32,
            size.height as i32,
            0,
            gl::RGBA,
            gl::UNSIGNED_BYTE,
            None.into(),
        );
        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
        gl_context.bind_texture(gl::TEXTURE_2D, current_texture_2d[0] as u32);

        Self::create(
            texture_id,
            TextureFlags {
                is_opaque: false,
                is_video_texture: false,
            },
            size,
            background,
            gl_context,
            RawImageFormat::BGRA8,
        )
    }

    pub fn clear(&mut self) {
        // save the OpenGL state
        let mut current_multisample = [0_u8];
        let mut current_index_buffer = [0_i32];
        let mut current_vertex_buffer = [0_i32];
        let mut current_vertex_array_object = [0_i32];
        let mut current_program = [0_i32];
        let mut current_framebuffers = [0_i32];
        let mut current_renderbuffers = [0_i32];
        let mut current_texture_2d = [0_i32];

        self.gl_context
            .get_boolean_v(gl::MULTISAMPLE, (&mut current_multisample[..]).into());
        self.gl_context.get_integer_v(
            gl::ARRAY_BUFFER_BINDING,
            (&mut current_vertex_buffer[..]).into(),
        );
        self.gl_context.get_integer_v(
            gl::ELEMENT_ARRAY_BUFFER_BINDING,
            (&mut current_index_buffer[..]).into(),
        );
        self.gl_context
            .get_integer_v(gl::CURRENT_PROGRAM, (&mut current_program[..]).into());
        self.gl_context.get_integer_v(
            gl::VERTEX_ARRAY_BINDING,
            (&mut current_vertex_array_object[..]).into(),
        );
        self.gl_context
            .get_integer_v(gl::RENDERBUFFER, (&mut current_renderbuffers[..]).into());
        self.gl_context
            .get_integer_v(gl::FRAMEBUFFER, (&mut current_framebuffers[..]).into());
        self.gl_context
            .get_integer_v(gl::TEXTURE_2D, (&mut current_texture_2d[..]).into());

        let framebuffers = self.gl_context.gen_framebuffers(1);
        let framebuffer_id = framebuffers.get(0).unwrap();
        self.gl_context
            .bind_framebuffer(gl::FRAMEBUFFER, *framebuffer_id);

        let depthbuffers = self.gl_context.gen_renderbuffers(1);
        let depthbuffer_id = depthbuffers.get(0).unwrap();

        self.gl_context
            .bind_texture(gl::TEXTURE_2D, self.texture_id);
        self.gl_context.tex_image_2d(
            gl::TEXTURE_2D,
            0,
            gl::RGBA as i32, // NOT RGBA8 - will generate INVALID_ENUM!
            self.size.width as i32,
            self.size.height as i32,
            0,
            gl::RGBA, // gl::BRGA?
            gl::UNSIGNED_BYTE,
            None.into(),
        );
        self.gl_context
            .tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
        self.gl_context
            .tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
        self.gl_context.tex_parameter_i(
            gl::TEXTURE_2D,
            gl::TEXTURE_WRAP_S,
            gl::CLAMP_TO_EDGE as i32,
        );
        self.gl_context.tex_parameter_i(
            gl::TEXTURE_2D,
            gl::TEXTURE_WRAP_T,
            gl::CLAMP_TO_EDGE as i32,
        );

        self.gl_context
            .bind_renderbuffer(gl::RENDERBUFFER, *depthbuffer_id);
        self.gl_context.renderbuffer_storage(
            gl::RENDERBUFFER,
            gl::DEPTH_COMPONENT,
            self.size.width as i32,
            self.size.height as i32,
        );
        self.gl_context.framebuffer_renderbuffer(
            gl::FRAMEBUFFER,
            gl::DEPTH_ATTACHMENT,
            gl::RENDERBUFFER,
            *depthbuffer_id,
        );

        self.gl_context.framebuffer_texture_2d(
            gl::FRAMEBUFFER,
            gl::COLOR_ATTACHMENT0,
            gl::TEXTURE_2D,
            self.texture_id,
            0,
        );
        self.gl_context
            .draw_buffers([gl::COLOR_ATTACHMENT0][..].into());

        let clear_color: ColorF = self.background_color.into();
        self.gl_context
            .clear_color(clear_color.r, clear_color.g, clear_color.b, clear_color.a);
        self.gl_context.clear_depth(0.0);
        self.gl_context
            .clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);

        // Reset the OpenGL state
        if u32::from(current_multisample[0]) == gl::TRUE {
            self.gl_context.enable(gl::MULTISAMPLE);
        }
        self.gl_context
            .bind_framebuffer(gl::FRAMEBUFFER, current_framebuffers[0] as u32);
        self.gl_context
            .bind_texture(gl::TEXTURE_2D, current_texture_2d[0] as u32);
        self.gl_context
            .bind_buffer(gl::RENDERBUFFER, current_renderbuffers[0] as u32);
        self.gl_context
            .bind_vertex_array(current_vertex_array_object[0] as u32);
        self.gl_context
            .bind_buffer(gl::ELEMENT_ARRAY_BUFFER, current_index_buffer[0] as u32);
        self.gl_context
            .bind_buffer(gl::ARRAY_BUFFER, current_vertex_buffer[0] as u32);
        self.gl_context.use_program(current_program[0] as u32);

        self.gl_context
            .delete_framebuffers((&[*framebuffer_id])[..].into());
        self.gl_context
            .delete_renderbuffers((&[*depthbuffer_id])[..].into());

        self.gl_context
            .bind_texture(gl::TEXTURE_2D, current_texture_2d[0] as u32);
    }

    pub fn get_descriptor(&self) -> ImageDescriptor {
        ImageDescriptor {
            format: self.format,
            width: self.size.width as usize,
            height: self.size.height as usize,
            stride: None.into(),
            offset: 0,
            flags: ImageDescriptorFlags {
                is_opaque: self.flags.is_opaque,
                // The texture gets mapped 1:1 onto the display, so there is no need for mipmaps
                allow_mipmaps: false,
            },
        }
    }

    /// Draws a `TessellatedGPUSvgNode` with the given color to the texture
    pub fn draw_tesselated_svg_gpu_node(
        &mut self,
        node: &TessellatedGPUSvgNode,
        size: PhysicalSizeU32,
        color: ColorU,
        transforms: StyleTransformVec,
    ) -> bool {
        node.draw(self, size, color, transforms)
    }

    /// Draws a `TessellatedColoredGPUSvgNode` to the texture
    pub fn draw_tesselated_colored_svg_gpu_node(
        &mut self,
        node: &crate::svg::TessellatedColoredGPUSvgNode,
        size: PhysicalSizeU32,
        transforms: StyleTransformVec,
    ) -> bool {
        node.draw(self, size, transforms)
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub struct TextureFlags {
    /// Whether this texture contains an alpha component
    pub is_opaque: bool,
    /// Optimization: use the compositor instead of OpenGL for energy optimization
    pub is_video_texture: bool,
}

impl ::core::fmt::Display for Texture {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(
            f,
            "Texture {{ id: {}, {}x{} }}",
            self.texture_id, self.size.width, self.size.height
        )
    }
}

macro_rules! impl_traits_for_gl_object {
    ($struct_name:ident, $gl_id_field:ident) => {
        impl ::core::fmt::Debug for $struct_name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
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

        impl Eq for $struct_name {}

        impl PartialOrd for $struct_name {
            fn partial_cmp(&self, other: &Self) -> Option<::core::cmp::Ordering> {
                Some((self.$gl_id_field).cmp(&(other.$gl_id_field)))
            }
        }

        impl Ord for $struct_name {
            fn cmp(&self, other: &Self) -> ::core::cmp::Ordering {
                (self.$gl_id_field).cmp(&(other.$gl_id_field))
            }
        }
    };
    ($struct_name:ident < $lt:lifetime > , $gl_id_field:ident) => {
        impl<$lt> ::core::fmt::Debug for $struct_name<$lt> {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                write!(f, "{}", self)
            }
        }

        impl<$lt> Hash for $struct_name<$lt> {
            fn hash<H: Hasher>(&self, state: &mut H) {
                self.$gl_id_field.hash(state);
            }
        }

        impl<$lt> PartialEq for $struct_name<$lt> {
            fn eq(&self, other: &$struct_name) -> bool {
                self.$gl_id_field == other.$gl_id_field
            }
        }

        impl<$lt> Eq for $struct_name<$lt> {}

        impl<$lt> PartialOrd for $struct_name<$lt> {
            fn partial_cmp(&self, other: &Self) -> Option<::core::cmp::Ordering> {
                Some((self.$gl_id_field).cmp(&(other.$gl_id_field)))
            }
        }

        impl<$lt> Ord for $struct_name<$lt> {
            fn cmp(&self, other: &Self) -> ::core::cmp::Ordering {
                (self.$gl_id_field).cmp(&(other.$gl_id_field))
            }
        }
    };
    ($struct_name:ident < $t:ident : $constraint:ident > , $gl_id_field:ident) => {
        impl<$t: $constraint> ::core::fmt::Debug for $struct_name<$t> {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                write!(f, "{}", self)
            }
        }

        impl<$t: $constraint> Hash for $struct_name<$t> {
            fn hash<H: Hasher>(&self, state: &mut H) {
                self.$gl_id_field.hash(state);
            }
        }

        impl<$t: $constraint> PartialEq for $struct_name<$t> {
            fn eq(&self, other: &$struct_name<$t>) -> bool {
                self.$gl_id_field == other.$gl_id_field
            }
        }

        impl<$t: $constraint> Eq for $struct_name<$t> {}

        impl<$t: $constraint> PartialOrd for $struct_name<$t> {
            fn partial_cmp(&self, other: &Self) -> Option<::core::cmp::Ordering> {
                Some((self.$gl_id_field).cmp(&(other.$gl_id_field)))
            }
        }

        impl<$t: $constraint> Ord for $struct_name<$t> {
            fn cmp(&self, other: &Self) -> ::core::cmp::Ordering {
                (self.$gl_id_field).cmp(&(other.$gl_id_field))
            }
        }
    };
}

impl_traits_for_gl_object!(Texture, texture_id);

impl Drop for Texture {
    fn drop(&mut self) {
        self.run_destructor = false;
        let copies = unsafe { (*self.refcount).fetch_sub(1, AtomicOrdering::SeqCst) };
        if copies == 1 {
            let _ = unsafe { Box::from_raw(self.refcount as *mut AtomicUsize) };
            self.gl_context
                .delete_textures((&[self.texture_id])[..].into());
        }
    }
}

/// Describes the vertex layout and offsets
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct VertexLayout {
    pub fields: VertexAttributeVec,
}

impl_vec!(VertexAttribute, VertexAttributeVec, VertexAttributeVecDestructor, VertexAttributeVecDestructorType, VertexAttributeVecSlice, OptionVertexAttribute);
impl_vec_debug!(VertexAttribute, VertexAttributeVec);
impl_vec_partialord!(VertexAttribute, VertexAttributeVec);
impl_vec_ord!(VertexAttribute, VertexAttributeVec);
impl_vec_clone!(
    VertexAttribute,
    VertexAttributeVec,
    VertexAttributeVecDestructor
);
impl_vec_partialeq!(VertexAttribute, VertexAttributeVec);
impl_vec_eq!(VertexAttribute, VertexAttributeVec);
impl_vec_hash!(VertexAttribute, VertexAttributeVec);

impl VertexLayout {
    /// Submits the vertex buffer description to OpenGL
    pub fn bind(&self, gl_context: &Rc<GenericGlContext>, program_id: GLuint) {
        const VERTICES_ARE_NORMALIZED: bool = false;

        let mut offset = 0;

        let stride_between_vertices: usize =
            self.fields.iter().map(VertexAttribute::get_stride).sum();

        for vertex_attribute in self.fields.iter() {
            let attribute_location = vertex_attribute
                .layout_location
                .as_option()
                .map(|ll| *ll as i32)
                .unwrap_or_else(|| {
                    gl_context
                        .get_attrib_location(program_id, vertex_attribute.va_name.as_str().into())
                });

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
    pub fn unbind(&self, gl_context: &Rc<GenericGlContext>, program_id: GLuint) {
        for vertex_attribute in self.fields.iter() {
            let attribute_location = vertex_attribute
                .layout_location
                .as_option()
                .map(|ll| *ll as i32)
                .unwrap_or_else(|| {
                    gl_context
                        .get_attrib_location(program_id, vertex_attribute.va_name.as_str().into())
                });
            gl_context.disable_vertex_attrib_array(attribute_location as u32);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct VertexAttribute {
    /// Attribute name of the vertex attribute in the vertex shader, i.e. `"vAttrXY"`
    pub va_name: AzString,
    /// If the vertex shader has a specific location, (like `layout(location = 2) vAttrXY`),
    /// use this instead of the name to look up the uniform location.
    pub layout_location: OptionUsize,
    /// Type of items of this attribute (i.e. for a `FloatVec2`, would be
    /// `VertexAttributeType::Float`)
    pub attribute_type: VertexAttributeType,
    /// Number of items of this attribute (i.e. for a `FloatVec2`, would be `2` (= 2 consecutive
    /// f32 values))
    pub item_count: usize,
}

impl_option!(
    VertexAttribute,
    OptionVertexAttribute,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

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
    /// Returns the OpenGL id for the vertex attribute type, ex. `gl::UNSIGNED_BYTE` for
    /// `VertexAttributeType::UnsignedByte`.
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
        use core::mem;

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

#[derive(Debug, PartialEq, PartialOrd)]
#[repr(C)]
pub struct VertexArrayObject {
    pub vertex_layout: VertexLayout,
    pub vao_id: GLuint,
    pub gl_context: GlContextPtr,
    pub refcount: *const AtomicUsize,
    pub run_destructor: bool,
}

impl VertexArrayObject {
    pub fn new(vertex_layout: VertexLayout, vao_id: GLuint, gl_context: GlContextPtr) -> Self {
        Self {
            vertex_layout,
            vao_id,
            gl_context,
            refcount: Box::into_raw(Box::new(AtomicUsize::new(1))),
            run_destructor: true,
        }
    }
}

impl Clone for VertexArrayObject {
    fn clone(&self) -> Self {
        unsafe { (*self.refcount).fetch_add(1, AtomicOrdering::SeqCst) };
        Self {
            vertex_layout: self.vertex_layout.clone(),
            vao_id: self.vao_id,
            gl_context: self.gl_context.clone(),
            refcount: self.refcount,
            run_destructor: true,
        }
    }
}

impl Drop for VertexArrayObject {
    fn drop(&mut self) {
        self.run_destructor = false;
        let copies = unsafe { (*self.refcount).fetch_sub(1, AtomicOrdering::SeqCst) };
        if copies == 1 {
            let _ = unsafe { Box::from_raw(self.refcount as *mut AtomicUsize) };
            self.gl_context
                .delete_vertex_arrays((&[self.vao_id])[..].into());
        }
    }
}

#[repr(C)]
pub struct VertexBuffer {
    pub vao: VertexArrayObject,
    pub vertex_buffer_id: GLuint,
    pub vertex_buffer_len: usize,
    pub index_buffer_id: GLuint,
    pub index_buffer_len: usize,
    pub refcount: *const AtomicUsize,
    pub index_buffer_format: IndexBufferFormat,
    pub run_destructor: bool,
}

impl core::fmt::Display for VertexBuffer {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "VertexBuffer {{ buffer: {} (length: {}) }})",
            self.vertex_buffer_id, self.vertex_buffer_len
        )
    }
}

impl_traits_for_gl_object!(VertexBuffer, vertex_buffer_id);

impl Clone for VertexBuffer {
    fn clone(&self) -> Self {
        unsafe { (*self.refcount).fetch_add(1, AtomicOrdering::SeqCst) };
        Self {
            vao: self.vao.clone(),
            vertex_buffer_id: self.vertex_buffer_id.clone(),
            vertex_buffer_len: self.vertex_buffer_len.clone(),
            index_buffer_id: self.index_buffer_id.clone(),
            index_buffer_len: self.index_buffer_len.clone(),
            refcount: self.refcount,
            index_buffer_format: self.index_buffer_format.clone(),
            run_destructor: true,
        }
    }
}

impl Drop for VertexBuffer {
    fn drop(&mut self) {
        self.run_destructor = false;
        let copies = unsafe { (*self.refcount).fetch_sub(1, AtomicOrdering::SeqCst) };
        if copies == 1 {
            self.vao.vertex_layout = VertexLayout {
                fields: VertexAttributeVec::from_const_slice(&[]),
            };
            let _ = unsafe { Box::from_raw(self.refcount as *mut AtomicUsize) };
            self.vao
                .gl_context
                .delete_buffers((&[self.vertex_buffer_id, self.index_buffer_id])[..].into());
        }
    }
}

impl VertexBuffer {
    pub fn new<T: VertexLayoutDescription>(
        gl_context: GlContextPtr,
        shader_program_id: GLuint,
        vertices: &[T],
        indices: &[u32],
        index_buffer_format: IndexBufferFormat,
    ) -> Self {
        use core::mem;

        // Save the OpenGL state
        let mut current_vertex_array = [0_i32];
        // let mut current_vertex_buffer = [0_i32];
        // let mut current_index_buffer = [0_i32];

        gl_context.get_integer_v(gl::VERTEX_ARRAY, (&mut current_vertex_array[..]).into());
        // gl_context.get_integer_v(gl::ARRAY_BUFFER, (&mut current_vertex_buffer[..]).into());
        // gl_context.get_integer_v(gl::ELEMENT_ARRAY_BUFFER, (&mut
        // current_index_buffer[..]).into());

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
            GlVoidPtrConst {
                ptr: vertices.as_ptr() as *const core::ffi::c_void,
                run_destructor: true,
            },
            gl::STATIC_DRAW,
        );

        // Generate the index buffer + upload data
        gl_context.bind_buffer(gl::ELEMENT_ARRAY_BUFFER, *index_buffer_id);
        gl_context.buffer_data_untyped(
            gl::ELEMENT_ARRAY_BUFFER,
            (mem::size_of::<u32>() * indices.len()) as isize,
            GlVoidPtrConst {
                ptr: indices.as_ptr() as *const core::ffi::c_void,
                run_destructor: true,
            },
            gl::STATIC_DRAW,
        );

        let vertex_description = T::get_description();
        vertex_description.bind(&gl_context.ptr.ptr, shader_program_id);

        // Reset the OpenGL state
        // gl_context.bind_buffer(gl::ARRAY_BUFFER, current_vertex_buffer[0] as u32);
        // gl_context.bind_buffer(gl::ELEMENT_ARRAY_BUFFER, current_index_buffer[0] as u32);
        gl_context.bind_vertex_array(current_vertex_array[0] as u32);

        Self::new_raw(
            *vertex_buffer_id,
            vertices.len(),
            VertexArrayObject::new(vertex_description, *vertex_array_object, gl_context),
            *index_buffer_id,
            indices.len(),
            index_buffer_format,
        )
    }

    pub fn new_raw(
        vertex_buffer_id: GLuint,
        vertex_buffer_len: usize,
        vao: VertexArrayObject,
        index_buffer_id: GLuint,
        index_buffer_len: usize,
        index_buffer_format: IndexBufferFormat,
    ) -> Self {
        Self {
            vertex_buffer_id,
            vertex_buffer_len,
            vao,
            index_buffer_id,
            index_buffer_len,
            index_buffer_format,
            refcount: Box::into_raw(Box::new(AtomicUsize::new(1))),
            run_destructor: true,
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
    pub fn get(gl_context: &GlContextPtr) -> Self {
        let mut major = [0];
        gl_context.get_integer_v(gl::MAJOR_VERSION, (&mut major[..]).into());
        let mut minor = [0];
        gl_context.get_integer_v(gl::MINOR_VERSION, (&mut minor[..]).into());

        GlApiVersion::Gl {
            major: major[0] as usize,
            minor: minor[0] as usize,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
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
    pub uniform_name: AzString,
    pub uniform_type: UniformType,
}

impl Uniform {
    pub fn create<S: Into<AzString>>(name: S, uniform_type: UniformType) -> Self {
        Self {
            uniform_name: name.into(),
            uniform_type,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C, u8)]
pub enum UniformType {
    Float(f32),
    FloatVec2([f32; 2]),
    FloatVec3([f32; 3]),
    FloatVec4([f32; 4]),
    Int(i32),
    IntVec2([i32; 2]),
    IntVec3([i32; 3]),
    IntVec4([i32; 4]),
    UnsignedInt(u32),
    UnsignedIntVec2([u32; 2]),
    UnsignedIntVec3([u32; 3]),
    UnsignedIntVec4([u32; 4]),
    Matrix2 {
        transpose: bool,
        matrix: [f32; 2 * 2],
    },
    Matrix3 {
        transpose: bool,
        matrix: [f32; 3 * 3],
    },
    Matrix4 {
        transpose: bool,
        matrix: [f32; 4 * 4],
    },
}

impl UniformType {
    /// Set a specific uniform
    pub fn set(self, gl_context: &Rc<GenericGlContext>, location: GLint) {
        use self::UniformType::*;
        match self {
            Float(r) => gl_context.uniform_1f(location, r),
            FloatVec2([r, g]) => gl_context.uniform_2f(location, r, g),
            FloatVec3([r, g, b]) => gl_context.uniform_3f(location, r, g, b),
            FloatVec4([r, g, b, a]) => gl_context.uniform_4f(location, r, g, b, a),
            Int(r) => gl_context.uniform_1i(location, r),
            IntVec2([r, g]) => gl_context.uniform_2i(location, r, g),
            IntVec3([r, g, b]) => gl_context.uniform_3i(location, r, g, b),
            IntVec4([r, g, b, a]) => gl_context.uniform_4i(location, r, g, b, a),
            UnsignedInt(r) => gl_context.uniform_1ui(location, r),
            UnsignedIntVec2([r, g]) => gl_context.uniform_2ui(location, r, g),
            UnsignedIntVec3([r, g, b]) => gl_context.uniform_3ui(location, r, g, b),
            UnsignedIntVec4([r, g, b, a]) => gl_context.uniform_4ui(location, r, g, b, a),
            Matrix2 { transpose, matrix } => {
                gl_context.uniform_matrix_2fv(location, transpose, &matrix[..])
            }
            Matrix3 { transpose, matrix } => {
                gl_context.uniform_matrix_3fv(location, transpose, &matrix[..])
            }
            Matrix4 { transpose, matrix } => {
                gl_context.uniform_matrix_4fv(location, transpose, &matrix[..])
            }
        }
    }
}

#[repr(C)]
pub struct GlShader {
    pub program_id: GLuint,
    pub gl_context: GlContextPtr,
}

impl ::core::fmt::Display for GlShader {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "GlShader {{ program_id: {} }}", self.program_id)
    }
}

impl_traits_for_gl_object!(GlShader, program_id);

impl Drop for GlShader {
    fn drop(&mut self) {
        self.gl_context.delete_program(self.program_id);
    }
}

#[repr(C)]
#[derive(Clone)]
pub struct VertexShaderCompileError {
    pub error_id: i32,
    pub info_log: AzString,
}

impl_traits_for_gl_object!(VertexShaderCompileError, error_id);

impl ::core::fmt::Display for VertexShaderCompileError {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "E{}: {}", self.error_id, self.info_log)
    }
}

#[repr(C)]
#[derive(Clone)]
pub struct FragmentShaderCompileError {
    pub error_id: i32,
    pub info_log: AzString,
}

impl_traits_for_gl_object!(FragmentShaderCompileError, error_id);

impl ::core::fmt::Display for FragmentShaderCompileError {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "E{}: {}", self.error_id, self.info_log)
    }
}

#[derive(Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum GlShaderCompileError {
    Vertex(VertexShaderCompileError),
    Fragment(FragmentShaderCompileError),
}

impl ::core::fmt::Display for GlShaderCompileError {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        use self::GlShaderCompileError::*;
        match self {
            Vertex(vert_err) => write!(f, "Failed to compile vertex shader: {}", vert_err),
            Fragment(frag_err) => write!(f, "Failed to compile fragment shader: {}", frag_err),
        }
    }
}

impl ::core::fmt::Debug for GlShaderCompileError {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "{}", self)
    }
}

#[repr(C)]
#[derive(Clone)]
pub struct GlShaderLinkError {
    pub error_id: i32,
    pub info_log: AzString,
}

impl_traits_for_gl_object!(GlShaderLinkError, error_id);

impl ::core::fmt::Display for GlShaderLinkError {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "E{}: {}", self.error_id, self.info_log)
    }
}

#[derive(Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum GlShaderCreateError {
    Compile(GlShaderCompileError),
    Link(GlShaderLinkError),
    NoShaderCompiler,
}

impl ::core::fmt::Display for GlShaderCreateError {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        use self::GlShaderCreateError::*;
        match self {
            Compile(compile_err) => write!(f, "Shader compile error: {}", compile_err),
            Link(link_err) => write!(f, "Shader linking error: {}", link_err),
            NoShaderCompiler => {
                write!(f, "OpenGL implementation doesn't include a shader compiler")
            }
        }
    }
}

impl ::core::fmt::Debug for GlShaderCreateError {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "{}", self)
    }
}

impl GlShader {
    /// Compiles and creates a new OpenGL shader, created from a vertex and a fragment shader
    /// string.
    ///
    /// If the shader fails to compile, the shader object gets automatically deleted, no cleanup
    /// necessary.
    pub fn new(
        gl_context: &GlContextPtr,
        vertex_shader: &str,
        fragment_shader: &str,
    ) -> Result<Self, GlShaderCreateError> {
        // Check whether the OpenGL implementation supports a shader compiler...
        let mut shader_compiler_supported = [gl::FALSE as u8];
        gl_context.get_boolean_v(
            gl::SHADER_COMPILER,
            (&mut shader_compiler_supported[..]).into(),
        );
        if u32::from(shader_compiler_supported[0]) == gl::FALSE {
            // Implementation only supports binary shaders
            return Err(GlShaderCreateError::NoShaderCompiler);
        }

        // Compile vertex shader

        let vertex_shader_object = gl_context.create_shader(gl::VERTEX_SHADER);
        gl_context.shader_source(
            vertex_shader_object,
            vec![AzString::from(vertex_shader.to_string())].into(),
        );
        gl_context.compile_shader(vertex_shader_object);

        if let Some(error_id) = get_gl_shader_error(&gl_context, vertex_shader_object) {
            let info_log = gl_context.get_shader_info_log(vertex_shader_object);
            gl_context.delete_shader(vertex_shader_object);
            return Err(GlShaderCreateError::Compile(GlShaderCompileError::Vertex(
                VertexShaderCompileError {
                    error_id,
                    info_log: info_log.into(),
                },
            )));
        }

        // Compile fragment shader

        let fragment_shader_object = gl_context.create_shader(gl::FRAGMENT_SHADER);
        gl_context.shader_source(
            fragment_shader_object,
            vec![AzString::from(fragment_shader.to_string())].into(),
        );
        gl_context.compile_shader(fragment_shader_object);

        if let Some(error_id) = get_gl_shader_error(&gl_context, fragment_shader_object) {
            let info_log = gl_context.get_shader_info_log(fragment_shader_object);
            gl_context.delete_shader(vertex_shader_object);
            gl_context.delete_shader(fragment_shader_object);
            return Err(GlShaderCreateError::Compile(
                GlShaderCompileError::Fragment(FragmentShaderCompileError {
                    error_id,
                    info_log: info_log.into(),
                }),
            ));
        }

        // Link program

        let program_id = gl_context.create_program();
        gl_context.attach_shader(program_id, vertex_shader_object);
        gl_context.attach_shader(program_id, fragment_shader_object);
        gl_context.link_program(program_id);

        if let Some(error_id) = get_gl_program_error(&gl_context, program_id) {
            let info_log = gl_context.get_program_info_log(program_id);
            gl_context.delete_shader(vertex_shader_object);
            gl_context.delete_shader(fragment_shader_object);
            gl_context.delete_program(program_id);
            return Err(GlShaderCreateError::Link(GlShaderLinkError {
                error_id,
                info_log: info_log.into(),
            }));
        }

        gl_context.delete_shader(vertex_shader_object);
        gl_context.delete_shader(fragment_shader_object);

        Ok(GlShader {
            program_id,
            gl_context: gl_context.clone(),
        })
    }

    /// Draws vertex buffers, index buffers + uniforms to the texture
    pub fn draw(
        // shader to use for drawing
        shader_program_id: GLuint,
        // note: texture is &mut so the texture is reusable -
        texture: &mut Texture,
        // buffers + uniforms to draw
        buffers: &[(&VertexBuffer, &[Uniform])],
    ) {
        use alloc::collections::btree_map::BTreeMap;

        const INDEX_TYPE: GLuint = gl::UNSIGNED_INT;

        let texture_size = texture.size;

        let gl_context = &texture.gl_context;

        // save the OpenGL state
        let mut current_multisample = [0_u8];
        let mut current_index_buffer = [0_i32];
        let mut current_vertex_buffer = [0_i32];
        let mut current_vertex_array_object = [0_i32];
        let mut current_program = [0_i32];
        let mut current_framebuffers = [0_i32];
        let mut current_renderbuffers = [0_i32];
        let mut current_texture_2d = [0_i32];
        let mut current_blend_enabled = [0_u8];
        let mut current_primitive_restart_fixed_index_enabled = [0_u8];

        gl_context.get_boolean_v(gl::MULTISAMPLE, (&mut current_multisample[..]).into());
        gl_context.get_integer_v(
            gl::ARRAY_BUFFER_BINDING,
            (&mut current_vertex_buffer[..]).into(),
        );
        gl_context.get_integer_v(
            gl::ELEMENT_ARRAY_BUFFER_BINDING,
            (&mut current_index_buffer[..]).into(),
        );
        gl_context.get_integer_v(gl::CURRENT_PROGRAM, (&mut current_program[..]).into());
        gl_context.get_integer_v(
            gl::VERTEX_ARRAY_BINDING,
            (&mut current_vertex_array_object[..]).into(),
        );
        gl_context.get_integer_v(gl::RENDERBUFFER, (&mut current_renderbuffers[..]).into());
        gl_context.get_integer_v(gl::FRAMEBUFFER, (&mut current_framebuffers[..]).into());
        gl_context.get_integer_v(gl::TEXTURE_2D, (&mut current_texture_2d[..]).into());
        gl_context.get_boolean_v(gl::BLEND, (&mut current_blend_enabled[..]).into());
        gl_context.get_boolean_v(
            gl::PRIMITIVE_RESTART_FIXED_INDEX,
            (&mut current_primitive_restart_fixed_index_enabled[..]).into(),
        );

        // 1. Create the framebuffer
        let framebuffers = gl_context.gen_framebuffers(1);
        let framebuffer_id = framebuffers.get(0).unwrap();
        gl_context.bind_framebuffer(gl::FRAMEBUFFER, *framebuffer_id);

        let depthbuffers = gl_context.gen_renderbuffers(1);
        let depthbuffer_id = depthbuffers.get(0).unwrap();

        gl_context.bind_texture(gl::TEXTURE_2D, texture.texture_id);
        gl_context.tex_image_2d(
            gl::TEXTURE_2D,
            0,
            gl::RGBA as i32, // NOT RGBA8 - will generate INVALID_ENUM!
            texture_size.width as i32,
            texture_size.height as i32,
            0,
            gl::RGBA, // gl::BRGA?
            gl::UNSIGNED_BYTE,
            None.into(),
        );
        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);

        gl_context.bind_renderbuffer(gl::RENDERBUFFER, *depthbuffer_id);
        gl_context.renderbuffer_storage(
            gl::RENDERBUFFER,
            gl::DEPTH_COMPONENT,
            texture_size.width as i32,
            texture_size.height as i32,
        );
        gl_context.framebuffer_renderbuffer(
            gl::FRAMEBUFFER,
            gl::DEPTH_ATTACHMENT,
            gl::RENDERBUFFER,
            *depthbuffer_id,
        );

        gl_context.framebuffer_texture_2d(
            gl::FRAMEBUFFER,
            gl::COLOR_ATTACHMENT0,
            gl::TEXTURE_2D,
            texture.texture_id,
            0,
        );
        gl_context.draw_buffers([gl::COLOR_ATTACHMENT0][..].into());

        #[cfg(feature = "std")]
        {
            let fb_check = gl_context.check_frame_buffer_status(gl::FRAMEBUFFER);
            match fb_check {
                gl::FRAMEBUFFER_COMPLETE => {}
                gl::FRAMEBUFFER_UNDEFINED => {
                    println!("GL_FRAMEBUFFER_UNDEFINED");
                }
                gl::FRAMEBUFFER_INCOMPLETE_ATTACHMENT => {
                    println!("GL_FRAMEBUFFER_INCOMPLETE_ATTACHMENT");
                }
                gl::FRAMEBUFFER_INCOMPLETE_MISSING_ATTACHMENT => {
                    println!("GL_FRAMEBUFFER_INCOMPLETE_MISSING_ATTACHMENT");
                }
                gl::FRAMEBUFFER_INCOMPLETE_DRAW_BUFFER => {
                    println!("GL_FRAMEBUFFER_INCOMPLETE_DRAW_BUFFER");
                }
                gl::FRAMEBUFFER_INCOMPLETE_READ_BUFFER => {
                    println!("GL_FRAMEBUFFER_INCOMPLETE_READ_BUFFER");
                }
                gl::FRAMEBUFFER_UNSUPPORTED => {
                    println!("GL_FRAMEBUFFER_UNSUPPORTED");
                }
                gl::FRAMEBUFFER_INCOMPLETE_MULTISAMPLE => {
                    println!("GL_FRAMEBUFFER_INCOMPLETE_MULTISAMPLE");
                }
                gl::FRAMEBUFFER_INCOMPLETE_MULTISAMPLE => {
                    println!("GL_FRAMEBUFFER_INCOMPLETE_MULTISAMPLE");
                }
                gl::FRAMEBUFFER_INCOMPLETE_LAYER_TARGETS => {
                    println!("GL_FRAMEBUFFER_INCOMPLETE_LAYER_TARGETS");
                }
                o => {
                    println!("glFramebufferStatus returned unknown return code: {}", o);
                }
            }
        }

        gl_context.viewport(0, 0, texture_size.width as i32, texture_size.height as i32);
        gl_context.enable(gl::BLEND);
        gl_context.enable(gl::PRIMITIVE_RESTART_FIXED_INDEX);
        gl_context.disable(gl::MULTISAMPLE);
        gl_context.blend_func(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA); // TODO: enable / disable
        gl_context.use_program(shader_program_id);

        // Avoid multiple calls to get_uniform_location by caching the uniform locations
        let mut uniform_locations: BTreeMap<AzString, i32> = BTreeMap::new();
        let mut max_uniform_len = 0;
        for (_, uniforms) in buffers {
            for uniform in uniforms.iter() {
                if !uniform_locations.contains_key(&uniform.uniform_name) {
                    uniform_locations.insert(
                        uniform.uniform_name.clone(),
                        gl_context.get_uniform_location(
                            shader_program_id,
                            uniform.uniform_name.as_str().into(),
                        ),
                    );
                }
            }
            max_uniform_len = max_uniform_len.max(uniforms.len());
        }
        let mut current_uniforms = vec![None; max_uniform_len];

        // Since the description of the vertex buffers is always the same,
        // only the first layer needs to bind its VAO

        // Draw the actual layers
        for (vertex_index_buffer, uniforms) in buffers {
            gl_context.bind_vertex_array(vertex_index_buffer.vao.vao_id);
            gl_context.bind_buffer(gl::ARRAY_BUFFER, vertex_index_buffer.vertex_buffer_id);
            gl_context.bind_buffer(
                gl::ELEMENT_ARRAY_BUFFER,
                vertex_index_buffer.index_buffer_id,
            );

            // Only set the uniform if the value has changed
            for (uniform_index, uniform) in uniforms.iter().enumerate() {
                if current_uniforms[uniform_index] != Some(uniform.uniform_type) {
                    let uniform_location = uniform_locations[&uniform.uniform_name];
                    uniform.uniform_type.set(gl_context.get(), uniform_location);
                    current_uniforms[uniform_index] = Some(uniform.uniform_type);
                }
            }

            gl_context.draw_elements(
                vertex_index_buffer.index_buffer_format.get_gl_id(),
                vertex_index_buffer.index_buffer_len as i32,
                INDEX_TYPE,
                0,
            );
        }

        // Reset the OpenGL state
        if u32::from(current_multisample[0]) == gl::TRUE {
            gl_context.enable(gl::MULTISAMPLE);
        }
        if u32::from(current_blend_enabled[0]) == gl::FALSE {
            gl_context.disable(gl::BLEND);
        }
        if u32::from(current_primitive_restart_fixed_index_enabled[0]) == gl::FALSE {
            gl_context.disable(gl::PRIMITIVE_RESTART_FIXED_INDEX);
        }
        gl_context.bind_framebuffer(gl::FRAMEBUFFER, current_framebuffers[0] as u32);
        gl_context.bind_texture(gl::TEXTURE_2D, current_texture_2d[0] as u32);
        gl_context.bind_buffer(gl::RENDERBUFFER, current_renderbuffers[0] as u32);
        gl_context.bind_vertex_array(current_vertex_array_object[0] as u32);
        gl_context.bind_buffer(gl::ELEMENT_ARRAY_BUFFER, current_index_buffer[0] as u32);
        gl_context.bind_buffer(gl::ARRAY_BUFFER, current_vertex_buffer[0] as u32);
        gl_context.use_program(current_program[0] as u32);

        gl_context.delete_framebuffers((&[*framebuffer_id])[..].into());
        gl_context.delete_renderbuffers((&[*depthbuffer_id])[..].into());

        texture.format = RawImageFormat::RGBA8;
        texture.flags = TextureFlags {
            is_opaque: false,
            is_video_texture: false,
        };
    }
}

fn get_gl_shader_error(context: &GlContextPtr, shader_object: GLuint) -> Option<i32> {
    let mut err = [0];
    context.get_shader_iv(shader_object, gl::COMPILE_STATUS, (&mut err[..]).into());
    let err_code = err[0];
    if err_code == gl::TRUE as i32 {
        None
    } else {
        Some(err_code)
    }
}

fn get_gl_program_error(context: &GlContextPtr, shader_object: GLuint) -> Option<i32> {
    let mut err = [0];
    context.get_program_iv(shader_object, gl::LINK_STATUS, (&mut err[..]).into());
    let err_code = err[0];
    if err_code == gl::TRUE as i32 {
        None
    } else {
        Some(err_code)
    }
}
