//! OpenGL context wrappers, texture cache management, shader compilation,
//! vertex buffer abstractions, and FFI-safe GL type aliases for the C/Python API.

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
    mem::ManuallyDrop,
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
    resources::{
        Brush, Epoch, ExternalImageId, ImageDescriptor, ImageDescriptorFlags, RawImage,
        RawImageData, RawImageFormat,
    },
    svg::{TessellatedGPUSvgNode, TessellatedSvgNode},
    window::RendererType,
    OrderedMap,
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

/// Passing *const `c_void` is not easily possible when generating APIs,
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

/// FFI-safe wrapper for `&str`.
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

impl fmt::Debug for Refstr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_str().fmt(f)
    }
}

impl Refstr {
    #[must_use] pub const fn as_str(&self) -> &str {
        // AUDIT: `from_raw_parts`/`from_utf8_unchecked` are UB on a null ptr
        // (even with len==0). FFI callers can hand us a null/empty Refstr, so
        // return an empty `&str` instead of forming a slice over null.
        if self.ptr.is_null() || self.len == 0 {
            return "";
        }
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

/// FFI-safe wrapper for `&[&str]`.
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

impl fmt::Debug for RefstrVecRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_slice().fmt(f)
    }
}

impl RefstrVecRef {
    #[must_use] pub const fn as_slice(&self) -> &[Refstr] {
        // AUDIT: `from_raw_parts` is UB on a null ptr; guard FFI null/empty.
        if self.ptr.is_null() || self.len == 0 {
            return &[];
        }
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

/// FFI-safe wrapper for `&mut [GLint64]`.
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

impl fmt::Debug for GLint64VecRefMut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    #[must_use] pub const fn as_slice(&self) -> &[GLint64] {
        // AUDIT: `from_raw_parts` is UB on a null ptr; guard FFI null/empty.
        if self.ptr.is_null() || self.len == 0 {
            return &[];
        }
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
    const fn as_mut_slice(&mut self) -> &mut [GLint64] {
        // AUDIT: `from_raw_parts` is UB on a null ptr; guard FFI null/empty.
        if self.ptr.is_null() || self.len == 0 {
            return &mut [];
        }
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

/// FFI-safe wrapper for `&mut [GLfloat]`.
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

impl fmt::Debug for GLfloatVecRefMut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    #[must_use] pub const fn as_slice(&self) -> &[GLfloat] {
        // AUDIT: `from_raw_parts` is UB on a null ptr; guard FFI null/empty.
        if self.ptr.is_null() || self.len == 0 {
            return &[];
        }
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
    const fn as_mut_slice(&mut self) -> &mut [GLfloat] {
        // AUDIT: `from_raw_parts` is UB on a null ptr; guard FFI null/empty.
        if self.ptr.is_null() || self.len == 0 {
            return &mut [];
        }
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

/// FFI-safe wrapper for `&mut [GLint]`.
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

impl fmt::Debug for GLintVecRefMut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    #[must_use] pub const fn as_slice(&self) -> &[GLint] {
        // AUDIT: `from_raw_parts` is UB on a null ptr; guard FFI null/empty.
        if self.ptr.is_null() || self.len == 0 {
            return &[];
        }
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
    const fn as_mut_slice(&mut self) -> &mut [GLint] {
        // AUDIT: `from_raw_parts` is UB on a null ptr; guard FFI null/empty.
        if self.ptr.is_null() || self.len == 0 {
            return &mut [];
        }
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

/// FFI-safe wrapper for `&[GLuint]`.
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

impl fmt::Debug for GLuintVecRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    #[must_use] pub const fn as_slice(&self) -> &[GLuint] {
        // AUDIT: `from_raw_parts` is UB on a null ptr; guard FFI null/empty.
        if self.ptr.is_null() || self.len == 0 {
            return &[];
        }
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
}

/// FFI-safe wrapper for `&[GLenum]`.
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

impl fmt::Debug for GLenumVecRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    #[must_use] pub const fn as_slice(&self) -> &[GLenum] {
        // AUDIT: `from_raw_parts` is UB on a null ptr; guard FFI null/empty.
        if self.ptr.is_null() || self.len == 0 {
            return &[];
        }
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
}

/// FFI-safe wrapper for `&[u8]`.
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
    #[must_use] pub const fn as_slice(&self) -> &[u8] {
        // AUDIT: `from_raw_parts` is UB on a null ptr; guard FFI null/empty.
        if self.ptr.is_null() || self.len == 0 {
            return &[];
        }
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl fmt::Debug for U8VecRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

impl Hash for U8VecRef {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.as_slice().hash(state);
    }
}

/// FFI-safe wrapper for `&[f32]`.
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

impl fmt::Debug for F32VecRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    #[must_use] pub const fn as_slice(&self) -> &[f32] {
        // AUDIT: `from_raw_parts` is UB on a null ptr; guard FFI null/empty.
        if self.ptr.is_null() || self.len == 0 {
            return &[];
        }
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
}

/// FFI-safe wrapper for `&[i32]`.
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

impl fmt::Debug for I32VecRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    #[must_use] pub const fn as_slice(&self) -> &[i32] {
        // AUDIT: `from_raw_parts` is UB on a null ptr; guard FFI null/empty.
        if self.ptr.is_null() || self.len == 0 {
            return &[];
        }
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
}

/// FFI-safe wrapper for `&mut [GLboolean]` (i.e. `&mut [u8]`).
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

impl fmt::Debug for GLbooleanVecRefMut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    #[must_use] pub const fn as_slice(&self) -> &[GLboolean] {
        // AUDIT: `from_raw_parts` is UB on a null ptr; guard FFI null/empty.
        if self.ptr.is_null() || self.len == 0 {
            return &[];
        }
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
    const fn as_mut_slice(&mut self) -> &mut [GLboolean] {
        // AUDIT: `from_raw_parts` is UB on a null ptr; guard FFI null/empty.
        if self.ptr.is_null() || self.len == 0 {
            return &mut [];
        }
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

/// FFI-safe wrapper for `&mut [u8]`.
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

impl fmt::Debug for U8VecRefMut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    #[must_use] pub const fn as_slice(&self) -> &[u8] {
        // AUDIT: `from_raw_parts` is UB on a null ptr; guard FFI null/empty.
        if self.ptr.is_null() || self.len == 0 {
            return &[];
        }
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
    const fn as_mut_slice(&mut self) -> &mut [u8] {
        // AUDIT: `from_raw_parts` is UB on a null ptr; guard FFI null/empty.
        if self.ptr.is_null() || self.len == 0 {
            return &mut [];
        }
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
    fn from(a: GlContextGlType) -> Self {
        match a {
            GlContextGlType::Gl => Self::Gl,
            GlContextGlType::GlEs => Self::Gles,
        }
    }
}

// (U8Vec, u32)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
// `_0`/`_1`… are C-ABI tuple-payload field names exposed in api.json; cannot rename.
#[allow(clippy::pub_underscore_fields)]
pub struct GetProgramBinaryReturn {
    pub _0: U8Vec,
    pub _1: u32,
}

// (i32, u32, AzString)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
// `_0`/`_1`… are C-ABI tuple-payload field names exposed in api.json; cannot rename.
#[allow(clippy::pub_underscore_fields)]
pub struct GetActiveAttribReturn {
    pub _0: i32,
    pub _1: u32,
    pub _2: AzString,
}

// (i32, u32, AzString)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
// `_0`/`_1`… are C-ABI tuple-payload field names exposed in api.json; cannot rename.
#[allow(clippy::pub_underscore_fields)]
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

impl fmt::Debug for GLsyncPtr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:0x}", self.ptr as usize)
    }
}

impl GLsyncPtr {
    #[must_use] pub const fn new(p: GLsync) -> Self {
        Self {
            ptr: p,
            run_destructor: true,
        }
    }
    #[must_use] pub fn get(self) -> GLsync {
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
pub type GlTextureStorage = OrderedMap<Epoch, OrderedMap<ExternalImageId, Texture>>;

/// Non-cleaned up textures. When a `GlTexture` is registered, it has to stay active as long
/// as `WebRender` needs it for drawing. To transparently do this, we store the epoch that the
/// texture was originally created with, and check, **after we have drawn the frame**,
/// if there are any textures that need cleanup.
///
/// Because the Texture2d is wrapped in an Rc, the destructor (which cleans up the OpenGL
/// texture) does not run until we remove the textures
///
/// Note: Because textures could be used after the current draw call (ex. for scrolling),
/// the `ACTIVE_GL_TEXTURES` are indexed by their epoch. Use `renderer.flush_pipeline_info()`
/// to see which textures are still active and which ones can be safely removed.
///
/// See: <https://github.com/servo/webrender/issues/2940>
///
/// WARNING: Not thread-safe (however, the Texture itself is thread-unsafe, so it's unlikely to ever
/// be misused)
static mut ACTIVE_GL_TEXTURES: Option<OrderedMap<DocumentId, GlTextureStorage>> = None;

/// Sound accessor for the process-global GL texture table.
///
/// AUDIT: GL access is single-threaded by design (see the WARNING above — the
/// `Texture` itself is thread-unsafe), so no lock is used. The soundness fix
/// here is to never form an *implicit* reference to the `static mut` (the
/// edition-2024 `static_mut_refs` hard error + `&mut`-aliasing UB that
/// `ACTIVE_GL_TEXTURES.as_mut()` / `.as_ref()` triggered). Deriving the
/// reference from `&raw mut` gives it correct provenance without ever naming
/// the static as an auto-ref place. Callers must not hold two of these at once
/// (they don't — every use is a single non-reentrant scope).
#[inline]
#[allow(clippy::deref_addrof)] // the `&raw mut` deref is deliberate: it avoids naming the static as an auto-ref place (edition-2024 `static_mut_refs`)
fn active_gl_textures() -> &'static mut Option<OrderedMap<DocumentId, GlTextureStorage>> {
    // SAFETY: `&raw mut` avoids an intermediate `&mut ACTIVE_GL_TEXTURES`; the
    // static is valid for the whole program. Single-threaded access (GL thread).
    unsafe { &mut *(&raw mut ACTIVE_GL_TEXTURES) }
}

/// Inserts a new texture into the OpenGL texture cache, returns a new image ID
/// for the inserted texture
///
/// This function exists so azul doesn't have to use `lazy_static` as a dependency
///
/// # Panics
///
/// Panics if the global active-GL-texture table has not been initialized.
#[must_use]
pub fn insert_into_active_gl_textures(
    document_id: DocumentId,
    epoch: Epoch,
    texture: Texture,
) -> ExternalImageId {
    let external_image_id = ExternalImageId::new();

    let active = active_gl_textures();
    if active.is_none() {
        *active = Some(OrderedMap::new());
    }
    let active_textures = active.as_mut().unwrap();
    let active_epochs = active_textures.entry(document_id).or_default();
    let active_textures_for_epoch = active_epochs.entry(epoch).or_default();
    active_textures_for_epoch.insert(external_image_id, texture);

    external_image_id
}

/// Destroys all textures from the given `document_id`
/// where the texture is **older** than the given `epoch`.
pub fn gl_textures_remove_epochs_from_pipeline(document_id: &DocumentId, epoch: Epoch) {
    // TODO: Handle overflow of Epochs correctly (low priority)
    let Some(active_textures) = active_gl_textures().as_mut() else {
        return;
    };

    let Some(active_epochs) = active_textures.get_mut(document_id) else {
        return;
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

// document_id, epoch, external_image_id
#[must_use] pub fn remove_single_texture_from_active_gl_textures(
    document_id: &DocumentId,
    epoch: &Epoch,
    external_image_id: &ExternalImageId,
) -> Option<()> {
    let active_textures = active_gl_textures().as_mut()?;
    let epochs = active_textures.get_mut(document_id)?;
    let images_in_epoch = epochs.get_mut(epoch)?;
    images_in_epoch.remove(external_image_id);
    Some(())
}

/// Removes a `DocumentId` from the active epochs
pub fn gl_textures_remove_active_pipeline(document_id: &DocumentId) {
    let Some(active_textures) = active_gl_textures().as_mut() else {
        return;
    };
    active_textures.remove(document_id);
}

/// Destroys all textures, usually done before destroying the OpenGL context
#[allow(clippy::cast_precision_loss)] // OpenGL/graphics binding: GL-bounded numeric casts to GL* types
pub fn gl_textures_clear_opengl_cache() {
    *active_gl_textures() = None;
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
#[allow(clippy::cast_precision_loss)] // OpenGL/graphics binding: GL-bounded numeric casts
#[must_use] pub fn get_opengl_texture(image_key: &ExternalImageId) -> Option<(GLuint, (f32, f32))> {
    let active_textures = active_gl_textures().as_ref()?;
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

/// For .`get_gl_precision_format()`, but ABI-safe - returning an array or a tuple is not ABI-safe
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd)]
#[repr(C)]
// `_0`/`_1`… are C-ABI tuple-payload field names exposed in api.json; cannot rename.
#[allow(clippy::pub_underscore_fields)]
pub struct GlShaderPrecisionFormatReturn {
    pub _0: GLint,
    pub _1: GLint,
    pub _2: GLint,
}

#[repr(C)]
pub struct GlContextPtr {
    /// `ManuallyDrop` so the owned `Box` is freed ONLY when `run_destructor` is
    /// still set (see `Drop`). The codegen FFI wrappers (`AzTexture` etc.) embed
    /// this by value AND have their own `Drop` that `drop_in_place`s the real
    /// type first; Rust's drop glue would then drop this field a SECOND time on
    /// the same bytes. Gating the `Box` free on `run_destructor` (which the first
    /// drop clears in the shared memory) makes that second drop a safe no-op.
    /// Layout is unchanged: `ManuallyDrop<Box<T>>` is a single pointer, identical
    /// to the old `Box<T>` and to the FFI `*mut c_void`.
    pub ptr: ManuallyDrop<Box<Rc<GlContextPtrInner>>>,
    /// Whether to force a hardware or software renderer
    pub renderer_type: RendererType,
    pub run_destructor: bool,
}

impl Clone for GlContextPtr {
    fn clone(&self) -> Self {
        Self {
            ptr: ManuallyDrop::new((*self.ptr).clone()),
            renderer_type: self.renderer_type,
            run_destructor: true,
        }
    }
}

impl Drop for GlContextPtr {
    fn drop(&mut self) {
        // Only free the owned Box if this instance still owns it. The FFI wrapper
        // double-drop (see the struct doc) hits these same bytes a second time
        // with `run_destructor` already cleared by the first drop -> no-op, no
        // double-free.
        if self.run_destructor {
            self.run_destructor = false;
            unsafe { ManuallyDrop::drop(&mut self.ptr); }
        }
    }
}

impl GlContextPtr {
    #[must_use] pub fn get_svg_shader(&self) -> GLuint {
        self.ptr.svg_shader
    }
    /// Whether this hardware GL context proved usable at construction (the SVG
    /// shaders compiled+linked at some GLSL version). `false` means context
    /// creation succeeded but the driver can't run our shaders -- the caller
    /// should fall back to CPU rendering. Always `false` for a Software context
    /// (which never compiles these shaders); only meaningful on the GPU path.
    #[must_use] pub fn is_gl_usable(&self) -> bool {
        self.ptr.svg_shader != 0
    }
    /// The GLSL `#version` the driver accepted at construction (e.g. "150" or
    /// "300 es"), discovered by the probe. Empty string if the context is
    /// unusable / software. Exposed in the API so apps can report/branch on it.
    #[must_use] pub fn get_usable_glsl_version(&self) -> AzString {
        self.ptr.glsl_version.clone()
    }
    /// Soft-brush shader program for the GPU painting API (0 if unusable).
    #[must_use] pub fn get_brush_shader(&self) -> GLuint {
        self.ptr.brush_shader
    }
    #[must_use] pub fn get_fxaa_shader(&self) -> GLuint {
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
    /// Soft-brush shader program for the GPU painting API (0 if unusable).
    pub brush_shader: GLuint,
    /// The GLSL `#version` directive that compiled (e.g. "150" or "300 es"),
    /// discovered by the probe in `new()`. Empty if the context is unusable.
    pub glsl_version: AzString,
}

impl fmt::Debug for GlContextPtrInner {
    // `ptr` wraps the external GL context (not Debug); show the rest.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GlContextPtrInner")
            .field("svg_shader", &self.svg_shader)
            .field("svg_multicolor_shader", &self.svg_multicolor_shader)
            .field("fxaa_shader", &self.fxaa_shader)
            .field("brush_shader", &self.brush_shader)
            .field("glsl_version", &self.glsl_version)
            .finish_non_exhaustive()
    }
}

impl Drop for GlContextPtrInner {
    fn drop(&mut self) {
        self.ptr.delete_program(self.svg_shader);
        self.ptr.delete_program(self.svg_multicolor_shader);
        self.ptr.delete_program(self.fxaa_shader);
        if self.brush_shader != 0 {
            self.ptr.delete_program(self.brush_shader);
        }
    }
}

impl_option!(
    GlContextPtr,
    OptionGlContextPtr,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord]
);

impl fmt::Debug for GlContextPtr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    #define varying in
#endif

#if __VERSION__ == 100
    #define oFragColor gl_FragColor
#else
    out vec4 oFragColor;
#endif

varying vec4 fColor;

void main() {
    oFragColor = fColor;
}";

// Soft-brush shaders for the GPU painting API. A unit quad [-1,1]^2 (aUv) is
// positioned in NDC (aPos); the fragment computes the same radial falloff as
// the CPU `brush_dab_coverage` (1 - smoothstep(hardness, 1, dist)) so GPU and
// CPU strokes match. Version-agnostic via `__VERSION__` like the SVG shaders.
static BRUSH_VERTEX_SHADER: &[u8] = b"#version 150

#if __VERSION__ != 100
    #define varying out
    #define attribute in
#endif

attribute vec2 aPos;
attribute vec2 aUv;
varying vec2 vUv;

void main() {
    vUv = aUv;
    gl_Position = vec4(aPos, 0.0, 1.0);
}";

static BRUSH_FRAGMENT_SHADER: &[u8] = b"#version 150

precision highp float;

#if __VERSION__ != 100
    #define varying in
#endif

#if __VERSION__ == 100
    #define oFragColor gl_FragColor
#else
    out vec4 oFragColor;
#endif

uniform vec4 uColor;     // rgb + alpha (alpha already folds in flow * color.a)
uniform float uHardness; // 0 = soft .. 1 = hard edge

varying vec2 vUv;

void main() {
    float d = length(vUv);
    if (d > 1.0) { discard; }
    float edge0 = clamp(uHardness, 0.0, 1.0);
    float x = clamp((d - edge0) / max(1.0 - edge0, 1.0e-4), 0.0, 1.0);
    float cov = 1.0 - (x * x * (3.0 - 2.0 * x));
    oFragColor = vec4(uColor.rgb, uColor.a * cov);
}";

/// Checks if a shader compiled successfully. Logs an error under `std`.
/// (Retained for diagnostics; the version probe in `GlContextPtr::new` now does
/// its own status checks.)
#[allow(dead_code)]
#[allow(clippy::used_underscore_binding)] // intentional `_`-prefix (FFI/api.json pub field, or cfg-gated binding); access is deliberate
#[allow(clippy::cast_possible_wrap)] // OpenGL/graphics binding: GL-bounded numeric casts to GL* types
fn check_shader_compile(gl_context: &GenericGlContext, shader: GLuint, _label: &str) {
    let mut status = [0_i32];
    unsafe { gl_context.get_shader_iv(shader, gl::COMPILE_STATUS, &mut status) };
    if status[0] != gl::TRUE as i32 {
        #[cfg(feature = "std")]
        {
            let log = gl_context.get_shader_info_log(shader);
            eprintln!("azul: {_label} shader compile error: {log}");
        }
    }
}

/// Checks if a program linked successfully. Logs an error under `std`.
#[allow(dead_code)]
#[allow(clippy::used_underscore_binding)] // intentional `_`-prefix (FFI/api.json pub field, or cfg-gated binding); access is deliberate
#[allow(clippy::cast_possible_wrap)] // OpenGL/graphics binding: GL-bounded numeric casts to GL* types
fn check_program_link(gl_context: &GenericGlContext, program: GLuint, _label: &str) {
    let mut status = [0_i32];
    unsafe { gl_context.get_program_iv(program, gl::LINK_STATUS, &mut status) };
    if status[0] != gl::TRUE as i32 {
        #[cfg(feature = "std")]
        {
            let log = gl_context.get_program_info_log(program);
            eprintln!("azul: {_label} program link error: {log}");
        }
    }
}

/// Swap the leading `#version ...` line of a bundled shader for `version_line`
/// (which must include the trailing newline). The shader bodies branch on
/// `__VERSION__`, so only the directive needs to change between GL and GLES.
#[cfg(feature = "std")]
fn shader_with_glsl_version(src: &[u8], version_line: &[u8]) -> Vec<u8> {
    let body_start = src.iter().position(|&b| b == b'\n').map_or(0, |i| i + 1);
    let mut out = Vec::with_capacity(version_line.len() + src.len() - body_start);
    out.extend_from_slice(version_line);
    out.extend_from_slice(&src[body_start..]);
    out
}

/// Try to compile+link a vertex+fragment program at a specific GLSL `#version`.
/// Returns the linked program id, or `None` (after cleanup) on ANY compile or
/// link failure. This is how we PROVE a GL context is actually usable and which
/// `#version` its driver accepts -- creating a context can succeed yet leave it
/// unable to compile our shaders (broken driver, or a GLES context that rejects
/// the desktop `#version 150`).
#[cfg(feature = "std")]
// OpenGL binding: gl::* enum constants passed to the gl API as GLint/GLenum.
#[allow(clippy::cast_possible_wrap)]
fn try_compile_program(
    gl_context: &GenericGlContext,
    vert_src: &[u8],
    frag_src: &[u8],
    version_line: &[u8],
    attribs: &[(u32, &str)],
) -> Option<GLuint> {
    let vs = gl_context.create_shader(gl::VERTEX_SHADER);
    gl_context.shader_source(vs, &[shader_with_glsl_version(vert_src, version_line).as_slice()]);
    gl_context.compile_shader(vs);
    let fs = gl_context.create_shader(gl::FRAGMENT_SHADER);
    gl_context.shader_source(fs, &[shader_with_glsl_version(frag_src, version_line).as_slice()]);
    gl_context.compile_shader(fs);

    let mut s = [0_i32];
    unsafe { gl_context.get_shader_iv(vs, gl::COMPILE_STATUS, &mut s) };
    let vs_ok = s[0] == gl::TRUE as i32;
    unsafe { gl_context.get_shader_iv(fs, gl::COMPILE_STATUS, &mut s) };
    let fs_ok = s[0] == gl::TRUE as i32;
    if !vs_ok || !fs_ok {
        gl_context.delete_shader(vs);
        gl_context.delete_shader(fs);
        return None;
    }

    let prog = gl_context.create_program();
    gl_context.attach_shader(prog, vs);
    gl_context.attach_shader(prog, fs);
    for (loc, name) in attribs {
        gl_context.bind_attrib_location(prog, *loc, name);
    }
    gl_context.link_program(prog);
    gl_context.delete_shader(vs);
    gl_context.delete_shader(fs);

    let mut l = [0_i32];
    unsafe { gl_context.get_program_iv(prog, gl::LINK_STATUS, &mut l) };
    if l[0] == gl::TRUE as i32 {
        Some(prog)
    } else {
        gl_context.delete_program(prog);
        None
    }
}

/// GLSL `#version` directives to try, in preference order, per context type.
/// The first that compiles+links the SVG shaders is used for every program.
const fn glsl_version_candidates(gl_type: GlType) -> &'static [&'static [u8]] {
    match gl_type {
        GlType::Gl => &[b"#version 150\n", b"#version 330\n", b"#version 140\n"],
        GlType::Gles => &[b"#version 300 es\n", b"#version 100\n"],
    }
}

impl GlContextPtr {
    #[must_use] pub fn new(renderer_type: RendererType, gl_context: Rc<GenericGlContext>) -> Self {
        // Only attempt the SVG/FXAA GL shaders for a real GPU. In Software/CPU
        // mode nothing composites through them.
        //
        // PROVE the context is usable rather than trusting context creation: try
        // compiling the SVG program at each candidate `#version` for this context
        // type (desktop GL 1.50/3.30/1.40, or GLES 3.00/1.00) and use the first
        // that compiles+links for ALL programs. A GLES GPU (mobile) rejects the
        // desktop `#version 150`, and a broken driver rejects everything -- in the
        // latter case all program IDs stay 0 and `is_gl_usable()` returns false so
        // the window can fall back to CPU rendering.
        #[cfg(feature = "std")]
        let (svg_program_id, svg_multicolor_program_id, fxaa_program_id, brush_program_id, glsl_version) =
            if matches!(renderer_type, RendererType::Hardware) {
                use crate::gl_fxaa::{FXAA_FRAGMENT_SHADER, FXAA_VERTEX_SHADER};
                let gl_type: GlType = gl_context.get_type().into();
                // Probe via the SVG program; the first version that links wins.
                let mut svg = 0;
                let mut chosen: Option<&'static [u8]> = None;
                for ver in glsl_version_candidates(gl_type) {
                    if let Some(p) = try_compile_program(
                        &gl_context, SVG_VERTEX_SHADER, SVG_FRAGMENT_SHADER, ver, &[(0, "vAttrXY")],
                    ) {
                        svg = p;
                        chosen = Some(ver);
                        break;
                    }
                }
                chosen.map_or_else(|| {
                    eprintln!(
                        "azul: GL context UNUSABLE -- no GLSL version ({gl_type:?}) compiled the SVG \
                         shaders; the window should fall back to CPU rendering (is_gl_usable()=false)"
                    );
                    (0, 0, 0, 0, AzString::from_const_str(""))
                }, |ver| {
                    // "150" / "300 es": the directive minus "#version " and newline.
                    let ver_str: AzString = core::str::from_utf8(ver)
                        .unwrap_or("")
                        .trim()
                        .trim_start_matches("#version ")
                        .into();
                    eprintln!(
                        "azul: GL usable -- shaders compiled at GLSL {} ({:?})",
                        ver_str.as_str(),
                        gl_type
                    );
                    let mc = try_compile_program(
                        &gl_context, SVG_MULTICOLOR_VERTEX_SHADER, SVG_MULTICOLOR_FRAGMENT_SHADER,
                        ver, &[(0, "vAttrXY"), (1, "vColor")],
                    ).unwrap_or(0);
                    let fxaa = try_compile_program(
                        &gl_context, FXAA_VERTEX_SHADER, FXAA_FRAGMENT_SHADER, ver, &[(0, "vAttrXY")],
                    ).unwrap_or(0);
                    let brush = try_compile_program(
                        &gl_context, BRUSH_VERTEX_SHADER, BRUSH_FRAGMENT_SHADER, ver,
                        &[(0, "aPos"), (1, "aUv")],
                    ).unwrap_or(0);
                    (svg, mc, fxaa, brush, ver_str)
                })
            } else {
                (0, 0, 0, 0, AzString::from_const_str(""))
            };
        // no_std build keeps the original behavior (no probe / no shaders).
        #[cfg(not(feature = "std"))]
        let (svg_program_id, svg_multicolor_program_id, fxaa_program_id, brush_program_id, glsl_version) =
            (0u32, 0u32, 0u32, 0u32, AzString::from_const_str(""));

        
        Self {
            ptr: ManuallyDrop::new(Box::new(Rc::new(GlContextPtrInner {
                svg_shader: svg_program_id,
                svg_multicolor_shader: svg_multicolor_program_id,
                fxaa_shader: fxaa_program_id,
                brush_shader: brush_program_id,
                glsl_version,
                ptr: gl_context,
            }))),
            renderer_type,
            run_destructor: true,
        }
    }

    #[must_use] pub fn get(&self) -> &Rc<GenericGlContext> {
        &self.ptr.ptr
    }
    fn as_usize(&self) -> usize {
        (Rc::as_ptr(&self.ptr.ptr) as *const c_void) as usize
    }
}

// This impl is the OpenGL API wrapper: every method mirrors a C/gleam GL call and
// takes the C-ABI argument types (GlVoidPtrConst, *VecRef, …) BY VALUE to match that
// ABI/FFI calling convention. Switching them to references would break the contract,
// so needless_pass_by_value is allowed for the whole GL-binding impl.
#[allow(clippy::needless_pass_by_value)]
impl GlContextPtr {
    #[must_use] pub fn get_type(&self) -> GlType {
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
            .buffer_data_untyped(target, size, data.ptr, usage);
    }
    pub fn buffer_sub_data_untyped(
        &self,
        target: GLenum,
        offset: isize,
        size: GLsizeiptr,
        data: GlVoidPtrConst,
    ) {
        self.get()
            .buffer_sub_data_untyped(target, offset, size, data.ptr);
    }
    #[must_use] pub fn map_buffer(&self, target: GLenum, access: GLbitfield) -> GlVoidPtrMut {
        GlVoidPtrMut {
            ptr: self.get().map_buffer(target, access),
        }
    }
    #[must_use] pub fn map_buffer_range(
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
    #[must_use] pub fn unmap_buffer(&self, target: GLenum) -> GLboolean {
        self.get().unmap_buffer(target)
    }
    pub fn tex_buffer(&self, target: GLenum, internal_format: GLenum, buffer: GLuint) {
        self.get().tex_buffer(target, internal_format, buffer);
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
            .map(AsRef::as_ref)
            .collect::<Vec<_>>();
        self.get().shader_source(shader, &shaders_as_bytes);
    }
    pub fn read_buffer(&self, mode: GLenum) {
        self.get().read_buffer(mode);
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
        );
    }
    #[must_use] pub fn read_pixels(
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
                .read_pixels_into_pbo(x, y, width, height, format, pixel_type);
        }
    }
    pub fn sample_coverage(&self, value: GLclampf, invert: bool) {
        self.get().sample_coverage(value, invert);
    }
    pub fn polygon_offset(&self, factor: GLfloat, units: GLfloat) {
        self.get().polygon_offset(factor, units);
    }
    pub fn pixel_store_i(&self, name: GLenum, param: GLint) {
        self.get().pixel_store_i(name, param);
    }
    #[must_use] pub fn gen_buffers(&self, n: GLsizei) -> GLuintVec {
        self.get().gen_buffers(n).into()
    }
    #[must_use] pub fn gen_renderbuffers(&self, n: GLsizei) -> GLuintVec {
        self.get().gen_renderbuffers(n).into()
    }
    #[must_use] pub fn gen_framebuffers(&self, n: GLsizei) -> GLuintVec {
        self.get().gen_framebuffers(n).into()
    }
    #[must_use] pub fn gen_textures(&self, n: GLsizei) -> GLuintVec {
        self.get().gen_textures(n).into()
    }
    #[must_use] pub fn gen_vertex_arrays(&self, n: GLsizei) -> GLuintVec {
        self.get().gen_vertex_arrays(n).into()
    }
    #[must_use] pub fn gen_queries(&self, n: GLsizei) -> GLuintVec {
        self.get().gen_queries(n).into()
    }
    pub fn begin_query(&self, target: GLenum, id: GLuint) {
        self.get().begin_query(target, id);
    }
    pub fn end_query(&self, target: GLenum) {
        self.get().end_query(target);
    }
    pub fn query_counter(&self, id: GLuint, target: GLenum) {
        self.get().query_counter(id, target);
    }
    #[must_use] pub fn get_query_object_iv(&self, id: GLuint, pname: GLenum) -> i32 {
        self.get().get_query_object_iv(id, pname)
    }
    #[must_use] pub fn get_query_object_uiv(&self, id: GLuint, pname: GLenum) -> u32 {
        self.get().get_query_object_uiv(id, pname)
    }
    #[must_use] pub fn get_query_object_i64v(&self, id: GLuint, pname: GLenum) -> i64 {
        self.get().get_query_object_i64v(id, pname)
    }
    #[must_use] pub fn get_query_object_ui64v(&self, id: GLuint, pname: GLenum) -> u64 {
        self.get().get_query_object_ui64v(id, pname)
    }
    pub fn delete_queries(&self, queries: GLuintVecRef) {
        self.get().delete_queries(queries.as_slice());
    }
    pub fn delete_vertex_arrays(&self, vertex_arrays: GLuintVecRef) {
        self.get().delete_vertex_arrays(vertex_arrays.as_slice());
    }
    pub fn delete_buffers(&self, buffers: GLuintVecRef) {
        self.get().delete_buffers(buffers.as_slice());
    }
    pub fn delete_renderbuffers(&self, renderbuffers: GLuintVecRef) {
        self.get().delete_renderbuffers(renderbuffers.as_slice());
    }
    pub fn delete_framebuffers(&self, framebuffers: GLuintVecRef) {
        self.get().delete_framebuffers(framebuffers.as_slice());
    }
    pub fn delete_textures(&self, textures: GLuintVecRef) {
        self.get().delete_textures(textures.as_slice());
    }
    pub fn framebuffer_renderbuffer(
        &self,
        target: GLenum,
        attachment: GLenum,
        renderbuffertarget: GLenum,
        renderbuffer: GLuint,
    ) {
        self.get()
            .framebuffer_renderbuffer(target, attachment, renderbuffertarget, renderbuffer);
    }
    pub fn renderbuffer_storage(
        &self,
        target: GLenum,
        internalformat: GLenum,
        width: GLsizei,
        height: GLsizei,
    ) {
        self.get()
            .renderbuffer_storage(target, internalformat, width, height);
    }
    pub fn depth_func(&self, func: GLenum) {
        self.get().depth_func(func);
    }
    pub fn active_texture(&self, texture: GLenum) {
        self.get().active_texture(texture);
    }
    pub fn attach_shader(&self, program: GLuint, shader: GLuint) {
        self.get().attach_shader(program, shader);
    }
    pub fn bind_attrib_location(&self, program: GLuint, index: GLuint, name: &str) {
        self.get()
            .bind_attrib_location(program, index, name);
    }
    pub fn get_uniform_iv(&self, program: GLuint, location: GLint, mut result: GLintVecRefMut) {
        unsafe {
            self.get()
                .get_uniform_iv(program, location, result.as_mut_slice());
        }
    }
    pub fn get_uniform_fv(&self, program: GLuint, location: GLint, mut result: GLfloatVecRefMut) {
        unsafe {
            self.get()
                .get_uniform_fv(program, location, result.as_mut_slice());
        }
    }
    #[must_use] pub fn get_uniform_block_index(&self, program: GLuint, name: &str) -> GLuint {
        self.get().get_uniform_block_index(program, name)
    }
    #[must_use] pub fn get_uniform_indices(&self, program: GLuint, names: RefstrVecRef) -> GLuintVec {
        let names_vec = names
            .as_slice()
            .iter()
            .map(Refstr::as_str)
            .collect::<Vec<_>>();
        self.get().get_uniform_indices(program, &names_vec).into()
    }
    pub fn bind_buffer_base(&self, target: GLenum, index: GLuint, buffer: GLuint) {
        self.get().bind_buffer_base(target, index, buffer);
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
            .bind_buffer_range(target, index, buffer, offset, size);
    }
    pub fn uniform_block_binding(
        &self,
        program: GLuint,
        uniform_block_index: GLuint,
        uniform_block_binding: GLuint,
    ) {
        self.get()
            .uniform_block_binding(program, uniform_block_index, uniform_block_binding);
    }
    pub fn bind_buffer(&self, target: GLenum, buffer: GLuint) {
        self.get().bind_buffer(target, buffer);
    }
    pub fn bind_vertex_array(&self, vao: GLuint) {
        self.get().bind_vertex_array(vao);
    }
    pub fn bind_renderbuffer(&self, target: GLenum, renderbuffer: GLuint) {
        self.get().bind_renderbuffer(target, renderbuffer);
    }
    pub fn bind_framebuffer(&self, target: GLenum, framebuffer: GLuint) {
        self.get().bind_framebuffer(target, framebuffer);
    }
    pub fn bind_texture(&self, target: GLenum, texture: GLuint) {
        self.get().bind_texture(target, texture);
    }
    pub fn draw_buffers(&self, bufs: GLenumVecRef) {
        self.get().draw_buffers(bufs.as_slice());
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
        let opt_data: Option<&[u8]> = opt_data.map(U8VecRef::as_slice);
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
        );
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
        );
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
        );
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
        let opt_data: Option<&[u8]> = opt_data.map(U8VecRef::as_slice);
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
        );
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
            .copy_tex_image_2d(target, level, internal_format, x, y, width, height, border);
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
            .copy_tex_sub_image_2d(target, level, xoffset, yoffset, x, y, width, height);
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
        );
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
        );
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
        );
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
        );
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
        );
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
            .tex_storage_2d(target, levels, internal_format, width, height);
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
            .tex_storage_3d(target, levels, internal_format, width, height, depth);
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
            .get_tex_image_into_buffer(target, level, format, ty, output.as_mut_slice());
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
            );
        }
    }
    pub fn invalidate_framebuffer(&self, target: GLenum, attachments: GLenumVecRef) {
        self.get()
            .invalidate_framebuffer(target, attachments.as_slice());
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
        );
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
                .get_integer_iv(name, index, result.as_mut_slice());
        }
    }
    pub fn get_integer_64iv(&self, name: GLenum, index: GLuint, mut result: GLint64VecRefMut) {
        unsafe {
            self.get()
                .get_integer_64iv(name, index, result.as_mut_slice());
        }
    }
    pub fn get_boolean_v(&self, name: GLenum, mut result: GLbooleanVecRefMut) {
        unsafe { self.get().get_boolean_v(name, result.as_mut_slice()) }
    }
    pub fn get_float_v(&self, name: GLenum, mut result: GLfloatVecRefMut) {
        unsafe { self.get().get_float_v(name, result.as_mut_slice()) }
    }
    #[must_use] pub fn get_framebuffer_attachment_parameter_iv(
        &self,
        target: GLenum,
        attachment: GLenum,
        pname: GLenum,
    ) -> GLint {
        self.get()
            .get_framebuffer_attachment_parameter_iv(target, attachment, pname)
    }
    #[must_use] pub fn get_renderbuffer_parameter_iv(&self, target: GLenum, pname: GLenum) -> GLint {
        self.get().get_renderbuffer_parameter_iv(target, pname)
    }
    #[must_use] pub fn get_tex_parameter_iv(&self, target: GLenum, name: GLenum) -> GLint {
        self.get().get_tex_parameter_iv(target, name)
    }
    #[must_use] pub fn get_tex_parameter_fv(&self, target: GLenum, name: GLenum) -> GLfloat {
        self.get().get_tex_parameter_fv(target, name)
    }
    pub fn tex_parameter_i(&self, target: GLenum, pname: GLenum, param: GLint) {
        self.get().tex_parameter_i(target, pname, param);
    }
    pub fn tex_parameter_f(&self, target: GLenum, pname: GLenum, param: GLfloat) {
        self.get().tex_parameter_f(target, pname, param);
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
            .framebuffer_texture_2d(target, attachment, textarget, texture, level);
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
            .framebuffer_texture_layer(target, attachment, texture, level, layer);
    }
    #[allow(clippy::similar_names)] // domain-standard coordinate/control-point names
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
        );
    }
    pub fn vertex_attrib_4f(&self, index: GLuint, x: GLfloat, y: GLfloat, z: GLfloat, w: GLfloat) {
        self.get().vertex_attrib_4f(index, x, y, z, w);
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
            .vertex_attrib_pointer_f32(index, size, normalized, stride, offset);
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
            .vertex_attrib_pointer(index, size, type_, normalized, stride, offset);
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
            .vertex_attrib_i_pointer(index, size, type_, stride, offset);
    }
    pub fn vertex_attrib_divisor(&self, index: GLuint, divisor: GLuint) {
        self.get().vertex_attrib_divisor(index, divisor);
    }
    pub fn viewport(&self, x: GLint, y: GLint, width: GLsizei, height: GLsizei) {
        self.get().viewport(x, y, width, height);
    }
    pub fn scissor(&self, x: GLint, y: GLint, width: GLsizei, height: GLsizei) {
        self.get().scissor(x, y, width, height);
    }
    pub fn line_width(&self, width: GLfloat) {
        self.get().line_width(width);
    }
    pub fn use_program(&self, program: GLuint) {
        self.get().use_program(program);
    }
    pub fn validate_program(&self, program: GLuint) {
        self.get().validate_program(program);
    }
    pub fn draw_arrays(&self, mode: GLenum, first: GLint, count: GLsizei) {
        self.get().draw_arrays(mode, first, count);
    }
    pub fn draw_arrays_instanced(
        &self,
        mode: GLenum,
        first: GLint,
        count: GLsizei,
        primcount: GLsizei,
    ) {
        self.get()
            .draw_arrays_instanced(mode, first, count, primcount);
    }
    pub fn draw_elements(
        &self,
        mode: GLenum,
        count: GLsizei,
        element_type: GLenum,
        indices_offset: GLuint,
    ) {
        self.get()
            .draw_elements(mode, count, element_type, indices_offset);
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
            .draw_elements_instanced(mode, count, element_type, indices_offset, primcount);
    }
    pub fn blend_color(&self, r: f32, g: f32, b: f32, a: f32) {
        self.get().blend_color(r, g, b, a);
    }
    pub fn blend_func(&self, sfactor: GLenum, dfactor: GLenum) {
        self.get().blend_func(sfactor, dfactor);
    }
    pub fn blend_func_separate(
        &self,
        src_rgb: GLenum,
        dest_rgb: GLenum,
        src_alpha: GLenum,
        dest_alpha: GLenum,
    ) {
        self.get()
            .blend_func_separate(src_rgb, dest_rgb, src_alpha, dest_alpha);
    }
    pub fn blend_equation(&self, mode: GLenum) {
        self.get().blend_equation(mode);
    }
    pub fn blend_equation_separate(&self, mode_rgb: GLenum, mode_alpha: GLenum) {
        self.get().blend_equation_separate(mode_rgb, mode_alpha);
    }
    // mirrors glColorMask(GLboolean, GLboolean, GLboolean, GLboolean) — the four
    // RGBA write-mask flags are the GL API, not a refactorable bool soup.
    #[allow(clippy::fn_params_excessive_bools)]
    pub fn color_mask(&self, r: bool, g: bool, b: bool, a: bool) {
        self.get().color_mask(r, g, b, a);
    }
    pub fn cull_face(&self, mode: GLenum) {
        self.get().cull_face(mode);
    }
    pub fn front_face(&self, mode: GLenum) {
        self.get().front_face(mode);
    }
    pub fn enable(&self, cap: GLenum) {
        self.get().enable(cap);
    }
    pub fn disable(&self, cap: GLenum) {
        self.get().disable(cap);
    }
    pub fn hint(&self, param_name: GLenum, param_val: GLenum) {
        self.get().hint(param_name, param_val);
    }
    #[must_use] pub fn is_enabled(&self, cap: GLenum) -> GLboolean {
        self.get().is_enabled(cap)
    }
    #[must_use] pub fn is_shader(&self, shader: GLuint) -> GLboolean {
        self.get().is_shader(shader)
    }
    #[must_use] pub fn is_texture(&self, texture: GLenum) -> GLboolean {
        self.get().is_texture(texture)
    }
    #[must_use] pub fn is_framebuffer(&self, framebuffer: GLenum) -> GLboolean {
        self.get().is_framebuffer(framebuffer)
    }
    #[must_use] pub fn is_renderbuffer(&self, renderbuffer: GLenum) -> GLboolean {
        self.get().is_renderbuffer(renderbuffer)
    }
    #[must_use] pub fn check_frame_buffer_status(&self, target: GLenum) -> GLenum {
        self.get().check_frame_buffer_status(target)
    }
    pub fn enable_vertex_attrib_array(&self, index: GLuint) {
        self.get().enable_vertex_attrib_array(index);
    }
    pub fn disable_vertex_attrib_array(&self, index: GLuint) {
        self.get().disable_vertex_attrib_array(index);
    }
    pub fn uniform_1f(&self, location: GLint, v0: GLfloat) {
        self.get().uniform_1f(location, v0);
    }
    pub fn uniform_1fv(&self, location: GLint, values: F32VecRef) {
        self.get().uniform_1fv(location, values.as_slice());
    }
    pub fn uniform_1i(&self, location: GLint, v0: GLint) {
        self.get().uniform_1i(location, v0);
    }
    pub fn uniform_1iv(&self, location: GLint, values: I32VecRef) {
        self.get().uniform_1iv(location, values.as_slice());
    }
    pub fn uniform_1ui(&self, location: GLint, v0: GLuint) {
        self.get().uniform_1ui(location, v0);
    }
    pub fn uniform_2f(&self, location: GLint, v0: GLfloat, v1: GLfloat) {
        self.get().uniform_2f(location, v0, v1);
    }
    pub fn uniform_2fv(&self, location: GLint, values: F32VecRef) {
        self.get().uniform_2fv(location, values.as_slice());
    }
    pub fn uniform_2i(&self, location: GLint, v0: GLint, v1: GLint) {
        self.get().uniform_2i(location, v0, v1);
    }
    pub fn uniform_2iv(&self, location: GLint, values: I32VecRef) {
        self.get().uniform_2iv(location, values.as_slice());
    }
    pub fn uniform_2ui(&self, location: GLint, v0: GLuint, v1: GLuint) {
        self.get().uniform_2ui(location, v0, v1);
    }
    pub fn uniform_3f(&self, location: GLint, v0: GLfloat, v1: GLfloat, v2: GLfloat) {
        self.get().uniform_3f(location, v0, v1, v2);
    }
    pub fn uniform_3fv(&self, location: GLint, values: F32VecRef) {
        self.get().uniform_3fv(location, values.as_slice());
    }
    pub fn uniform_3i(&self, location: GLint, v0: GLint, v1: GLint, v2: GLint) {
        self.get().uniform_3i(location, v0, v1, v2);
    }
    pub fn uniform_3iv(&self, location: GLint, values: I32VecRef) {
        self.get().uniform_3iv(location, values.as_slice());
    }
    pub fn uniform_3ui(&self, location: GLint, v0: GLuint, v1: GLuint, v2: GLuint) {
        self.get().uniform_3ui(location, v0, v1, v2);
    }
    pub fn uniform_4f(&self, location: GLint, x: GLfloat, y: GLfloat, z: GLfloat, w: GLfloat) {
        self.get().uniform_4f(location, x, y, z, w);
    }
    pub fn uniform_4i(&self, location: GLint, x: GLint, y: GLint, z: GLint, w: GLint) {
        self.get().uniform_4i(location, x, y, z, w);
    }
    pub fn uniform_4iv(&self, location: GLint, values: I32VecRef) {
        self.get().uniform_4iv(location, values.as_slice());
    }
    pub fn uniform_4ui(&self, location: GLint, x: GLuint, y: GLuint, z: GLuint, w: GLuint) {
        self.get().uniform_4ui(location, x, y, z, w);
    }
    pub fn uniform_4fv(&self, location: GLint, values: F32VecRef) {
        self.get().uniform_4fv(location, values.as_slice());
    }
    pub fn uniform_matrix_2fv(&self, location: GLint, transpose: bool, value: F32VecRef) {
        self.get()
            .uniform_matrix_2fv(location, transpose, value.as_slice());
    }
    pub fn uniform_matrix_3fv(&self, location: GLint, transpose: bool, value: F32VecRef) {
        self.get()
            .uniform_matrix_3fv(location, transpose, value.as_slice());
    }
    pub fn uniform_matrix_4fv(&self, location: GLint, transpose: bool, value: F32VecRef) {
        self.get()
            .uniform_matrix_4fv(location, transpose, value.as_slice());
    }
    pub fn depth_mask(&self, flag: bool) {
        self.get().depth_mask(flag);
    }
    pub fn depth_range(&self, near: f64, far: f64) {
        self.get().depth_range(near, far);
    }
    #[must_use] pub fn get_active_attrib(&self, program: GLuint, index: GLuint) -> GetActiveAttribReturn {
        let r = self.get().get_active_attrib(program, index);
        GetActiveAttribReturn {
            _0: r.0,
            _1: r.1,
            _2: r.2.into(),
        }
    }
    #[must_use] pub fn get_active_uniform(&self, program: GLuint, index: GLuint) -> GetActiveUniformReturn {
        let r = self.get().get_active_uniform(program, index);
        GetActiveUniformReturn {
            _0: r.0,
            _1: r.1,
            _2: r.2.into(),
        }
    }
    #[must_use] pub fn get_active_uniforms_iv(
        &self,
        program: GLuint,
        indices: GLuintVec,
        pname: GLenum,
    ) -> GLintVec {
        self.get()
            .get_active_uniforms_iv(program, indices.into_library_owned_vec(), pname)
            .into()
    }
    #[must_use] pub fn get_active_uniform_block_i(
        &self,
        program: GLuint,
        index: GLuint,
        pname: GLenum,
    ) -> GLint {
        self.get().get_active_uniform_block_i(program, index, pname)
    }
    #[must_use] pub fn get_active_uniform_block_iv(
        &self,
        program: GLuint,
        index: GLuint,
        pname: GLenum,
    ) -> GLintVec {
        self.get()
            .get_active_uniform_block_iv(program, index, pname)
            .into()
    }
    #[must_use] pub fn get_active_uniform_block_name(&self, program: GLuint, index: GLuint) -> AzString {
        self.get()
            .get_active_uniform_block_name(program, index)
            .into()
    }
    #[must_use] pub fn get_attrib_location(&self, program: GLuint, name: &str) -> c_int {
        self.get().get_attrib_location(program, name)
    }
    #[must_use] pub fn get_frag_data_location(&self, program: GLuint, name: &str) -> c_int {
        self.get().get_frag_data_location(program, name)
    }
    #[must_use] pub fn get_uniform_location(&self, program: GLuint, name: &str) -> c_int {
        self.get().get_uniform_location(program, name)
    }
    #[must_use] pub fn get_program_info_log(&self, program: GLuint) -> AzString {
        self.get().get_program_info_log(program).into()
    }
    pub fn get_program_iv(&self, program: GLuint, pname: GLenum, mut result: GLintVecRefMut) {
        unsafe {
            self.get()
                .get_program_iv(program, pname, result.as_mut_slice());
        }
    }
    #[must_use] pub fn get_program_binary(&self, program: GLuint) -> GetProgramBinaryReturn {
        let r = self.get().get_program_binary(program);
        GetProgramBinaryReturn {
            _0: r.0.into(),
            _1: r.1,
        }
    }
    pub fn program_binary(&self, program: GLuint, format: GLenum, binary: U8VecRef) {
        self.get()
            .program_binary(program, format, binary.as_slice());
    }
    pub fn program_parameter_i(&self, program: GLuint, pname: GLenum, value: GLint) {
        self.get().program_parameter_i(program, pname, value);
    }
    pub fn get_vertex_attrib_iv(&self, index: GLuint, pname: GLenum, mut result: GLintVecRefMut) {
        unsafe {
            self.get()
                .get_vertex_attrib_iv(index, pname, result.as_mut_slice());
        }
    }
    pub fn get_vertex_attrib_fv(&self, index: GLuint, pname: GLenum, mut result: GLfloatVecRefMut) {
        unsafe {
            self.get()
                .get_vertex_attrib_fv(index, pname, result.as_mut_slice());
        }
    }
    #[must_use] pub fn get_vertex_attrib_pointer_v(&self, index: GLuint, pname: GLenum) -> GLsizeiptr {
        self.get().get_vertex_attrib_pointer_v(index, pname)
    }
    #[must_use] pub fn get_buffer_parameter_iv(&self, target: GLuint, pname: GLenum) -> GLint {
        self.get().get_buffer_parameter_iv(target, pname)
    }
    #[must_use] pub fn get_shader_info_log(&self, shader: GLuint) -> AzString {
        self.get().get_shader_info_log(shader).into()
    }
    #[must_use] pub fn get_string(&self, which: GLenum) -> AzString {
        self.get().get_string(which).into()
    }
    #[must_use] pub fn get_string_i(&self, which: GLenum, index: GLuint) -> AzString {
        self.get().get_string_i(which, index).into()
    }
    pub fn get_shader_iv(&self, shader: GLuint, pname: GLenum, mut result: GLintVecRefMut) {
        unsafe {
            self.get()
                .get_shader_iv(shader, pname, result.as_mut_slice());
        }
    }
    #[must_use] pub fn get_shader_precision_format(
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
        self.get().compile_shader(shader);
    }
    #[must_use] pub fn create_program(&self) -> GLuint {
        self.get().create_program()
    }
    pub fn delete_program(&self, program: GLuint) {
        self.get().delete_program(program);
    }
    #[must_use] pub fn create_shader(&self, shader_type: GLenum) -> GLuint {
        self.get().create_shader(shader_type)
    }
    pub fn delete_shader(&self, shader: GLuint) {
        self.get().delete_shader(shader);
    }
    pub fn detach_shader(&self, program: GLuint, shader: GLuint) {
        self.get().detach_shader(program, shader);
    }
    pub fn link_program(&self, program: GLuint) {
        self.get().link_program(program);
    }
    pub fn clear_color(&self, r: f32, g: f32, b: f32, a: f32) {
        self.get().clear_color(r, g, b, a);
    }
    pub fn clear(&self, buffer_mask: GLbitfield) {
        self.get().clear(buffer_mask);
    }
    pub fn clear_depth(&self, depth: f64) {
        self.get().clear_depth(depth);
    }
    pub fn clear_stencil(&self, s: GLint) {
        self.get().clear_stencil(s);
    }
    pub fn flush(&self) {
        self.get().flush();
    }
    pub fn finish(&self) {
        self.get().finish();
    }
    #[must_use] pub fn get_error(&self) -> GLenum {
        self.get().get_error()
    }
    pub fn stencil_mask(&self, mask: GLuint) {
        self.get().stencil_mask(mask);
    }
    pub fn stencil_mask_separate(&self, face: GLenum, mask: GLuint) {
        self.get().stencil_mask_separate(face, mask);
    }
    pub fn stencil_func(&self, func: GLenum, ref_: GLint, mask: GLuint) {
        self.get().stencil_func(func, ref_, mask);
    }
    pub fn stencil_func_separate(&self, face: GLenum, func: GLenum, ref_: GLint, mask: GLuint) {
        self.get().stencil_func_separate(face, func, ref_, mask);
    }
    pub fn stencil_op(&self, sfail: GLenum, dpfail: GLenum, dppass: GLenum) {
        self.get().stencil_op(sfail, dpfail, dppass);
    }
    pub fn stencil_op_separate(&self, face: GLenum, sfail: GLenum, dpfail: GLenum, dppass: GLenum) {
        self.get().stencil_op_separate(face, sfail, dpfail, dppass);
    }
    pub fn egl_image_target_texture2d_oes(&self, target: GLenum, image: GlVoidPtrConst) {
        self.get()
            .egl_image_target_texture2d_oes(target, image.ptr as *const c_void);
    }
    pub fn generate_mipmap(&self, target: GLenum) {
        self.get().generate_mipmap(target);
    }
    pub fn insert_event_marker_ext(&self, message: &str) {
        self.get().insert_event_marker_ext(message);
    }
    pub fn push_group_marker_ext(&self, message: &str) {
        self.get().push_group_marker_ext(message);
    }
    pub fn pop_group_marker_ext(&self) {
        self.get().pop_group_marker_ext();
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
            .debug_message_insert_khr(source, type_, id, severity, message);
    }
    pub fn push_debug_group_khr(&self, source: GLenum, id: GLuint, message: &str) {
        self.get()
            .push_debug_group_khr(source, id, message);
    }
    pub fn pop_debug_group_khr(&self) {
        self.get().pop_debug_group_khr();
    }
    #[must_use] pub fn fence_sync(&self, condition: GLenum, flags: GLbitfield) -> GLsyncPtr {
        GLsyncPtr::new(self.get().fence_sync(condition, flags))
    }
    #[must_use] pub fn client_wait_sync(&self, sync: GLsyncPtr, flags: GLbitfield, timeout: GLuint64) -> u32 {
        self.get().client_wait_sync(sync.get(), flags, timeout)
    }
    pub fn wait_sync(&self, sync: GLsyncPtr, flags: GLbitfield, timeout: GLuint64) {
        self.get().wait_sync(sync.get(), flags, timeout);
    }
    pub fn delete_sync(&self, sync: GLsyncPtr) {
        self.get().delete_sync(sync.get());
    }
    pub fn texture_range_apple(&self, target: GLenum, data: U8VecRef) {
        self.get().texture_range_apple(target, data.as_slice());
    }
    #[must_use] pub fn gen_fences_apple(&self, n: GLsizei) -> GLuintVec {
        self.get().gen_fences_apple(n).into()
    }
    pub fn delete_fences_apple(&self, fences: GLuintVecRef) {
        self.get().delete_fences_apple(fences.as_slice());
    }
    pub fn set_fence_apple(&self, fence: GLuint) {
        self.get().set_fence_apple(fence);
    }
    pub fn finish_fence_apple(&self, fence: GLuint) {
        self.get().finish_fence_apple(fence);
    }
    pub fn test_fence_apple(&self, fence: GLuint) {
        self.get().test_fence_apple(fence);
    }
    #[must_use] pub fn test_object_apple(&self, object: GLenum, name: GLuint) -> GLboolean {
        self.get().test_object_apple(object, name)
    }
    pub fn finish_object_apple(&self, object: GLenum, name: GLuint) {
        self.get().finish_object_apple(object, name);
    }
    #[must_use] pub fn get_frag_data_index(&self, program: GLuint, name: &str) -> GLint {
        self.get().get_frag_data_index(program, name)
    }
    pub fn blend_barrier_khr(&self) {
        self.get().blend_barrier_khr();
    }
    pub fn bind_frag_data_location_indexed(
        &self,
        program: GLuint,
        color_number: GLuint,
        index: GLuint,
        name: &str,
    ) {
        self.get()
            .bind_frag_data_location_indexed(program, color_number, index, name);
    }
    #[must_use] pub fn get_debug_messages(&self) -> DebugMessageVec {
        let dmv: Vec<DebugMessage> = self
            .get()
            .get_debug_messages()
            .into_iter()
            .map(|d| DebugMessage {
                message: d.message.into(),
                source: d.source,
                ty: d.ty,
                id: d.id,
                severity: d.severity,
            })
            .collect();
        dmv.into()
    }
    pub fn provoking_vertex_angle(&self, mode: GLenum) {
        self.get().provoking_vertex_angle(mode);
    }
    #[must_use] pub fn gen_vertex_arrays_apple(&self, n: GLsizei) -> GLuintVec {
        self.get().gen_vertex_arrays_apple(n).into()
    }
    pub fn bind_vertex_array_apple(&self, vao: GLuint) {
        self.get().bind_vertex_array_apple(vao);
    }
    pub fn delete_vertex_arrays_apple(&self, vertex_arrays: GLuintVecRef) {
        self.get()
            .delete_vertex_arrays_apple(vertex_arrays.as_slice());
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
        );
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
        );
    }
    pub fn egl_image_target_renderbuffer_storage_oes(&self, target: u32, image: GlVoidPtrConst) {
        self.get().egl_image_target_renderbuffer_storage_oes(
            target,
            image.ptr as *const c_void,
        );
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
        );
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
        );
    }
    pub fn buffer_storage(
        &self,
        target: GLenum,
        size: GLsizeiptr,
        data: GlVoidPtrConst,
        flags: GLbitfield,
    ) {
        self.get().buffer_storage(target, size, data.ptr, flags);
    }
    pub fn flush_mapped_buffer_range(&self, target: GLenum, offset: GLintptr, length: GLsizeiptr) {
        self.get().flush_mapped_buffer_range(target, offset, length);
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

/// Saved OpenGL state for save/restore around framebuffer operations.
/// Used by `Texture::clear()` and `GlShader::draw()` to avoid corrupting
/// the caller's GL state.
// the `current_` prefix is intentional: each field holds the saved CURRENT GL
// binding captured at save() to be restored in restore().
#[allow(clippy::struct_field_names)]
struct GlStateSave {
    current_multisample: [u8; 1],
    current_index_buffer: [i32; 1],
    current_vertex_buffer: [i32; 1],
    current_vertex_array_object: [i32; 1],
    current_program: [i32; 1],
    current_framebuffers: [i32; 1],
    current_renderbuffers: [i32; 1],
    current_texture_2d: [i32; 1],
}

impl GlStateSave {
    fn save(gl_context: &GlContextPtr) -> Self {
        let mut s = Self {
            current_multisample: [0],
            current_index_buffer: [0],
            current_vertex_buffer: [0],
            current_vertex_array_object: [0],
            current_program: [0],
            current_framebuffers: [0],
            current_renderbuffers: [0],
            current_texture_2d: [0],
        };

        gl_context.get_boolean_v(gl::MULTISAMPLE, (&mut s.current_multisample[..]).into());
        gl_context.get_integer_v(gl::ARRAY_BUFFER_BINDING, (&mut s.current_vertex_buffer[..]).into());
        gl_context.get_integer_v(gl::ELEMENT_ARRAY_BUFFER_BINDING, (&mut s.current_index_buffer[..]).into());
        gl_context.get_integer_v(gl::CURRENT_PROGRAM, (&mut s.current_program[..]).into());
        gl_context.get_integer_v(gl::VERTEX_ARRAY_BINDING, (&mut s.current_vertex_array_object[..]).into());
        gl_context.get_integer_v(gl::RENDERBUFFER, (&mut s.current_renderbuffers[..]).into());
        gl_context.get_integer_v(gl::FRAMEBUFFER, (&mut s.current_framebuffers[..]).into());
        gl_context.get_integer_v(gl::TEXTURE_2D, (&mut s.current_texture_2d[..]).into());

        s
    }

    // OpenGL binding: state values passed to the gl API as GLuint/GLsizei.
    #[allow(clippy::cast_sign_loss)]
    fn restore(&self, gl_context: &GlContextPtr) {
        if u32::from(self.current_multisample[0]) == gl::TRUE {
            gl_context.enable(gl::MULTISAMPLE);
        }
        gl_context.bind_framebuffer(gl::FRAMEBUFFER, self.current_framebuffers[0] as u32);
        gl_context.bind_texture(gl::TEXTURE_2D, self.current_texture_2d[0] as u32);
        gl_context.bind_buffer(gl::RENDERBUFFER, self.current_renderbuffers[0] as u32);
        gl_context.bind_vertex_array(self.current_vertex_array_object[0] as u32);
        gl_context.bind_buffer(gl::ELEMENT_ARRAY_BUFFER, self.current_index_buffer[0] as u32);
        gl_context.bind_buffer(gl::ARRAY_BUFFER, self.current_vertex_buffer[0] as u32);
        gl_context.use_program(self.current_program[0] as u32);
    }
}

/// AUDIT: RAII guard that deletes a transient framebuffer + renderbuffer on
/// scope exit. Used by `Texture::clear` and `GlShader::draw` so that a panic
/// mid-path (e.g. an `.unwrap()` on an empty gen-list, or any GL step that
/// panics) can't leak the FBO/RBO. Ids of `0` are skipped (GL treats delete-0
/// as a no-op anyway, but this keeps intent explicit).
struct FboRboGuard<'a> {
    gl_context: &'a GlContextPtr,
    framebuffer_id: GLuint,
    renderbuffer_id: GLuint,
}

impl Drop for FboRboGuard<'_> {
    fn drop(&mut self) {
        if self.framebuffer_id != 0 {
            self.gl_context
                .delete_framebuffers((&[self.framebuffer_id])[..].into());
        }
        if self.renderbuffer_id != 0 {
            self.gl_context
                .delete_renderbuffers((&[self.renderbuffer_id])[..].into());
        }
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
    #[allow(clippy::cast_sign_loss)] // OpenGL/graphics binding: GL-bounded numeric casts to GL* types
    fn clone(&self) -> Self {
        unsafe {
            (*self.refcount).fetch_add(1, AtomicOrdering::SeqCst);
        }
        Self {
            gl_context: self.gl_context.clone(),
            texture_id: self.texture_id,
            refcount: self.refcount,
            size: self.size,
            format: self.format,
            background_color: self.background_color,
            flags: self.flags,
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
    #[must_use] pub fn create(
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

    // OpenGL binding: gl::* enum constants and texture dimensions are passed as
    // GLint/GLsizei (i32); the values are GL-bounded and the `as i32` casts are the
    // idiomatic form for the gl API.
    #[allow(clippy::cast_possible_wrap)]
    #[allow(clippy::cast_sign_loss)] // OpenGL/graphics binding: GL-bounded numeric casts
    #[must_use] pub fn allocate_rgba8(
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
            // Format is BGRA8 for WebRender integration, despite the GL upload using RGBA
            RawImageFormat::BGRA8,
        )
    }

    /// # Panics
    ///
    /// Panics if no framebuffer/depthbuffer was allocated (the GL object lists are empty).
    // OpenGL binding: gl::* enum constants and texture dimensions passed as
    // GLint/GLsizei (i32); values are GL-bounded, `as i32` is the idiomatic form.
    #[allow(clippy::cast_possible_wrap)]
    pub fn clear(&mut self) {
        let saved = GlStateSave::save(&self.gl_context);

        let framebuffers = self.gl_context.gen_framebuffers(1);
        let framebuffer_id = *framebuffers.get(0).unwrap();
        // AUDIT: register the FBO for cleanup BEFORE the next fallible step so a
        // panic in `gen_renderbuffers().get(0).unwrap()` can't leak it.
        let mut fbo_rbo_guard = FboRboGuard {
            gl_context: &self.gl_context,
            framebuffer_id,
            renderbuffer_id: 0,
        };
        self.gl_context
            .bind_framebuffer(gl::FRAMEBUFFER, framebuffer_id);

        let depthbuffers = self.gl_context.gen_renderbuffers(1);
        let depthbuffer_id = *depthbuffers.get(0).unwrap();
        fbo_rbo_guard.renderbuffer_id = depthbuffer_id;

        self.gl_context
            .bind_texture(gl::TEXTURE_2D, self.texture_id);
        self.gl_context.tex_image_2d(
            gl::TEXTURE_2D,
            0,
            gl::RGBA as i32, // NOT RGBA8 - will generate INVALID_ENUM!
            self.size.width as i32,
            self.size.height as i32,
            0,
            gl::RGBA, // gl::BGRA?
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
            .bind_renderbuffer(gl::RENDERBUFFER, depthbuffer_id);
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
            depthbuffer_id,
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

        // AUDIT: FBO/RBO deletion is handled by `fbo_rbo_guard`'s Drop (which
        // also runs on an unwinding panic), so we restore state and let the
        // guard reclaim the GL objects at scope exit.
        saved.restore(&self.gl_context);
        drop(fbo_rbo_guard);
    }

    #[must_use] pub fn get_descriptor(&self) -> ImageDescriptor {
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
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
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
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
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
}

impl Texture {
    /// GPU painting: stamp one soft-brush dab centered at (`cx`, `cy`) in texture
    /// pixel coordinates (origin top-left, matching [`RawImage::paint_dot`]).
    /// No-op if the GL context is unusable -- the caller should then use the CPU
    /// `RawImage` path (`GlContextPtr::is_gl_usable`).
    pub fn paint_dot(&mut self, cx: f32, cy: f32, brush: Brush) {
        self.paint_stroke(cx, cy, cx, cy, brush);
    }

    /// GPU painting: stamp dabs along (`x0`,`y0`)->(`x1`,`y1`) into this texture
    /// via an FBO + the soft-brush shader, alpha-over blended. Same spacing +
    /// falloff as the CPU `RawImage::paint_stroke`. No-op if GL is unusable.
    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_precision_loss, clippy::cast_sign_loss)] // OpenGL/graphics binding: GL-bounded numeric casts to GL* types
    #[allow(clippy::many_single_char_names)] // domain-standard colour/coordinate component names
    pub fn paint_stroke(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, brush: Brush) {
        let gl = self.gl_context.clone();
        let prog = gl.get_brush_shader();
        let (tw, th) = (self.size.width as f32, self.size.height as f32);
        // `!(radius > 0.0)` intentionally also rejects NaN (`radius <= 0.0` would not).
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        if prog == 0 || self.texture_id == 0 || !(brush.radius > 0.0) || tw <= 0.0 || th <= 0.0 {
            return;
        }

        let fbo = gl.gen_framebuffers(1).get(0).copied().unwrap_or(0);
        let vbo = gl.gen_buffers(1).get(0).copied().unwrap_or(0);
        if fbo == 0 || vbo == 0 {
            if fbo != 0 {
                gl.delete_framebuffers((&[fbo][..]).into());
            }
            if vbo != 0 {
                gl.delete_buffers((&[vbo][..]).into());
            }
            return;
        }

        gl.bind_framebuffer(gl::FRAMEBUFFER, fbo);
        gl.framebuffer_texture_2d(
            gl::FRAMEBUFFER,
            gl::COLOR_ATTACHMENT0,
            gl::TEXTURE_2D,
            self.texture_id,
            0,
        );
        gl.viewport(0, 0, self.size.width as i32, self.size.height as i32);
        gl.enable(gl::BLEND);
        gl.blend_func(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
        gl.use_program(prog);

        let a = (f32::from(brush.color.a) / 255.0) * brush.flow.clamp(0.0, 1.0);
        gl.uniform_4f(
            gl.get_uniform_location(prog, "uColor"),
            f32::from(brush.color.r) / 255.0,
            f32::from(brush.color.g) / 255.0,
            f32::from(brush.color.b) / 255.0,
            a,
        );
        gl.uniform_1f(gl.get_uniform_location(prog, "uHardness"), brush.hardness);

        gl.bind_buffer(gl::ARRAY_BUFFER, vbo);
        gl.enable_vertex_attrib_array(0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(0, 2, false, 16, 0);
        gl.vertex_attrib_pointer_f32(1, 2, false, 16, 8);

        let dx = x1 - x0;
        let dy = y1 - y0;
        let len = dx.hypot(dy);
        let step = (brush.radius * brush.spacing.max(0.01)).max(0.5);
        let n = ((len / step).floor() as i32).max(0);
        let r = brush.radius;
        for i in 0..=n {
            let t = if n == 0 { 1.0 } else { i as f32 / n as f32 };
            let px = x0 + dx * t;
            let py = y0 + dy * t;
            // dab bbox -> NDC; y is flipped so (0,0) is top-left like the CPU path.
            let nx = |x: f32| (x / tw) * 2.0 - 1.0;
            let ny = |y: f32| 1.0 - (y / th) * 2.0;
            let (l, rr, tp, bt) = (nx(px - r), nx(px + r), ny(py - r), ny(py + r));
            // TRIANGLE_STRIP: TL, BL, TR, BR -- interleaved (pos.x, pos.y, uv.x, uv.y).
            let verts: [f32; 16] = [
                l, tp, -1.0, -1.0, l, bt, -1.0, 1.0, rr, tp, 1.0, -1.0, rr, bt, 1.0, 1.0,
            ];
            gl.buffer_data_untyped(
                gl::ARRAY_BUFFER,
                (verts.len() * size_of::<f32>()) as isize,
                GlVoidPtrConst {
                    ptr: verts.as_ptr() as *const GLvoid,
                    run_destructor: false,
                },
                gl::STREAM_DRAW,
            );
            gl.draw_arrays(gl::TRIANGLE_STRIP, 0, 4);
        }

        gl.disable_vertex_attrib_array(0);
        gl.disable_vertex_attrib_array(1);
        gl.bind_buffer(gl::ARRAY_BUFFER, 0);
        gl.disable(gl::BLEND);
        gl.bind_framebuffer(gl::FRAMEBUFFER, 0);
        gl.delete_buffers((&[vbo][..]).into());
        gl.delete_framebuffers((&[fbo][..]).into());
    }

    /// Read this texture's pixels back into an RGBA8 `RawImage` (top-left origin)
    /// -- for exporting the painted canvas to disk. Binds an FBO + glReadPixels.
    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)] // OpenGL/graphics binding: GL-bounded numeric casts
    #[must_use] pub fn copy_to_raw_image(&self) -> RawImage {
        let gl = self.gl_context.clone();
        let (w, h) = (self.size.width as i32, self.size.height as i32);
        if self.texture_id == 0 || w <= 0 || h <= 0 {
            return RawImage::null_image();
        }
        let fbo = gl.gen_framebuffers(1).get(0).copied().unwrap_or(0);
        if fbo == 0 {
            return RawImage::null_image();
        }
        gl.bind_framebuffer(gl::FRAMEBUFFER, fbo);
        gl.framebuffer_texture_2d(
            gl::FRAMEBUFFER,
            gl::COLOR_ATTACHMENT0,
            gl::TEXTURE_2D,
            self.texture_id,
            0,
        );
        let pixels = gl.read_pixels(0, 0, w, h, gl::RGBA, gl::UNSIGNED_BYTE);
        gl.bind_framebuffer(gl::FRAMEBUFFER, 0);
        gl.delete_framebuffers((&[fbo][..]).into());

        // glReadPixels uses a bottom-left origin; flip rows to top-left for saving.
        let mut bytes = pixels.into_library_owned_vec();
        let row = (w as usize) * 4;
        let hh = h as usize;
        if row > 0 && bytes.len() >= row * hh {
            for y in 0..hh / 2 {
                let yi = y * row;
                let yo = (hh - 1 - y) * row;
                for k in 0..row {
                    bytes.swap(yi + k, yo + k);
                }
            }
        }
        RawImage {
            pixels: RawImageData::U8(bytes.into()),
            width: w as usize,
            height: h as usize,
            premultiplied_alpha: true,
            data_format: RawImageFormat::RGBA8,
            tag: Vec::new().into(),
        }
    }
}

impl_traits_for_gl_object!(Texture, texture_id);

impl Drop for Texture {
    fn drop(&mut self) {
        // AUDIT: mirror `GlContextPtr::drop`. Without this guard a C-ABI
        // double-drop (drop_in_place run twice on the same byte-copied struct)
        // does a second `fetch_sub` on the already-freed refcount box (UAF) and
        // a second `delete_textures`. The first drop clears `run_destructor`, so
        // the second is a no-op.
        if !self.run_destructor {
            return;
        }
        self.run_destructor = false;
        let copies = unsafe { (*self.refcount).fetch_sub(1, AtomicOrdering::SeqCst) };
        if copies == 1 {
            drop(unsafe { Box::from_raw(self.refcount.cast_mut()) });
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
    // OpenGL binding: vertex-attribute layout (locations, item counts, strides,
    // offsets) passed to the gl API as GLuint/GLint/GLsizei; values are GL-bounded.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_wrap)] // OpenGL/graphics binding: GL-bounded numeric casts to GL* types
    pub fn bind(&self, gl_context: &Rc<GenericGlContext>, program_id: GLuint) {
        const VERTICES_ARE_NORMALIZED: bool = false;

        let mut offset = 0;

        let stride_between_vertices: usize =
            self.fields.iter().map(VertexAttribute::get_stride).sum();

        for vertex_attribute in &self.fields {
            let attribute_location = vertex_attribute.layout_location.as_option().map_or_else(
                || gl_context.get_attrib_location(program_id, vertex_attribute.va_name.as_str()),
                |ll| *ll as i32,
            );

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
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // OpenGL/graphics binding: GL-bounded numeric casts to GL* types
    #[allow(clippy::cast_possible_wrap)] // OpenGL/graphics binding: GL-bounded numeric casts
    pub fn unbind(&self, gl_context: &Rc<GenericGlContext>, program_id: GLuint) {
        for vertex_attribute in &self.fields {
            let attribute_location = vertex_attribute.layout_location.as_option().map_or_else(
                || gl_context.get_attrib_location(program_id, vertex_attribute.va_name.as_str()),
                |ll| *ll as i32,
            );
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
    #[must_use] pub const fn get_stride(&self) -> usize {
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
    #[must_use] pub const fn get_gl_id(&self) -> GLuint {
        use self::VertexAttributeType::{Float, Double, UnsignedByte, UnsignedShort, UnsignedInt};
        match self {
            Float => gl::FLOAT,
            Double => gl::DOUBLE,
            UnsignedByte => gl::UNSIGNED_BYTE,
            UnsignedShort => gl::UNSIGNED_SHORT,
            UnsignedInt => gl::UNSIGNED_INT,
        }
    }

    #[must_use] pub const fn get_mem_size(&self) -> usize {
        use core::mem;

        use self::VertexAttributeType::{Float, Double, UnsignedByte, UnsignedShort, UnsignedInt};
        match self {
            Float => size_of::<f32>(),
            Double => size_of::<f64>(),
            UnsignedByte => size_of::<u8>(),
            UnsignedShort => size_of::<u16>(),
            UnsignedInt => size_of::<u32>(),
        }
    }
}

pub trait VertexLayoutDescription {
    fn get_description() -> VertexLayout;
}

#[derive(Debug, PartialEq, Eq, PartialOrd)]
#[repr(C)]
pub struct VertexArrayObject {
    pub vertex_layout: VertexLayout,
    pub vao_id: GLuint,
    pub gl_context: GlContextPtr,
    pub refcount: *const AtomicUsize,
    pub run_destructor: bool,
}

impl VertexArrayObject {
    #[must_use] pub fn new(vertex_layout: VertexLayout, vao_id: GLuint, gl_context: GlContextPtr) -> Self {
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
        // AUDIT: mirror `GlContextPtr::drop` — guard against a C-ABI double-drop
        // freeing the refcount box twice (use-after-free) + double delete.
        if !self.run_destructor {
            return;
        }
        self.run_destructor = false;
        let copies = unsafe { (*self.refcount).fetch_sub(1, AtomicOrdering::SeqCst) };
        if copies == 1 {
            drop(unsafe { Box::from_raw(self.refcount.cast_mut()) });
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

impl fmt::Display for VertexBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
            vertex_buffer_id: self.vertex_buffer_id,
            vertex_buffer_len: self.vertex_buffer_len,
            index_buffer_id: self.index_buffer_id,
            index_buffer_len: self.index_buffer_len,
            refcount: self.refcount,
            index_buffer_format: self.index_buffer_format,
            run_destructor: true,
        }
    }
}

impl Drop for VertexBuffer {
    fn drop(&mut self) {
        // AUDIT: mirror `GlContextPtr::drop` — guard against a C-ABI double-drop
        // freeing the refcount box twice (use-after-free) + double delete.
        if !self.run_destructor {
            return;
        }
        self.run_destructor = false;
        let copies = unsafe { (*self.refcount).fetch_sub(1, AtomicOrdering::SeqCst) };
        if copies == 1 {
            self.vao.vertex_layout = VertexLayout {
                fields: VertexAttributeVec::from_const_slice(&[]),
            };
            drop(unsafe { Box::from_raw(self.refcount.cast_mut()) });
            self.vao
                .gl_context
                .delete_buffers((&[self.vertex_buffer_id, self.index_buffer_id])[..].into());
        }
    }
}

impl VertexBuffer {
    /// # Panics
    ///
    /// Panics if the GL driver failed to create the vertex-array/buffer objects
    /// (the returned id lists are empty).
    // OpenGL binding: buffer sizes / vertex counts passed to the gl API as
    // GLsizeiptr/GLint; values are GL-bounded.
    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)] // OpenGL/graphics binding: GL-bounded numeric casts to GL* types
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

        gl_context.get_integer_v(gl::VERTEX_ARRAY, (&mut current_vertex_array[..]).into());

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
            size_of_val(vertices) as isize,
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
            size_of_val(indices) as isize,
            GlVoidPtrConst {
                ptr: indices.as_ptr() as *const core::ffi::c_void,
                run_destructor: true,
            },
            gl::STATIC_DRAW,
        );

        let vertex_description = T::get_description();
        vertex_description.bind(&gl_context.ptr.ptr, shader_program_id);

        // Reset the OpenGL state
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

    #[must_use] pub fn new_raw(
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
    #[allow(clippy::cast_sign_loss)] // OpenGL/graphics binding: GL-bounded numeric casts
    #[must_use] pub fn get(gl_context: &GlContextPtr) -> Self {
        let mut major = [0];
        gl_context.get_integer_v(gl::MAJOR_VERSION, (&mut major[..]).into());
        let mut minor = [0];
        gl_context.get_integer_v(gl::MINOR_VERSION, (&mut minor[..]).into());

        let major = major[0] as usize;
        let minor = minor[0] as usize;

        match gl_context.get_type() {
            GlType::Gl => Self::Gl { major, minor },
            GlType::Gles => Self::GlEs { major, minor },
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
    #[must_use] pub const fn get_gl_id(&self) -> GLuint {
        use self::IndexBufferFormat::{Points, Lines, LineStrip, Triangles, TriangleStrip, TriangleFan};
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
        use self::UniformType::{Float, FloatVec2, FloatVec3, FloatVec4, Int, IntVec2, IntVec3, IntVec4, UnsignedInt, UnsignedIntVec2, UnsignedIntVec3, UnsignedIntVec4, Matrix2, Matrix3, Matrix4};
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
                gl_context.uniform_matrix_2fv(location, transpose, &matrix[..]);
            }
            Matrix3 { transpose, matrix } => {
                gl_context.uniform_matrix_3fv(location, transpose, &matrix[..]);
            }
            Matrix4 { transpose, matrix } => {
                gl_context.uniform_matrix_4fv(location, transpose, &matrix[..]);
            }
        }
    }
}

#[repr(C)]
pub struct GlShader {
    pub program_id: GLuint,
    pub gl_context: GlContextPtr,
    /// AUDIT: guards against a double-drop deleting the same GL program twice
    /// (`drop_in_place` run twice on a byte-copied struct). Set `true` on
    /// construction; the first drop clears it so a second drop is a no-op.
    pub run_destructor: bool,
}

impl ::core::fmt::Display for GlShader {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        write!(f, "GlShader {{ program_id: {} }}", self.program_id)
    }
}

impl_traits_for_gl_object!(GlShader, program_id);

impl Drop for GlShader {
    fn drop(&mut self) {
        // AUDIT: mirror `GlContextPtr::drop` — a C-ABI double-drop would call
        // `delete_program` on the same id twice. The first drop clears the flag.
        if !self.run_destructor {
            return;
        }
        self.run_destructor = false;
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
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
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
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        write!(f, "E{}: {}", self.error_id, self.info_log)
    }
}

#[derive(Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum GlShaderCompileError {
    Vertex(VertexShaderCompileError),
    Fragment(FragmentShaderCompileError),
}

impl ::core::fmt::Display for GlShaderCompileError {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        use self::GlShaderCompileError::{Vertex, Fragment};
        match self {
            Vertex(vert_err) => write!(f, "Failed to compile vertex shader: {vert_err}"),
            Fragment(frag_err) => write!(f, "Failed to compile fragment shader: {frag_err}"),
        }
    }
}

impl ::core::fmt::Debug for GlShaderCompileError {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        write!(f, "{self}")
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
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
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
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        use self::GlShaderCreateError::{Compile, Link, NoShaderCompiler};
        match self {
            Compile(compile_err) => write!(f, "Shader compile error: {compile_err}"),
            Link(link_err) => write!(f, "Shader linking error: {link_err}"),
            NoShaderCompiler => {
                write!(f, "OpenGL implementation doesn't include a shader compiler")
            }
        }
    }
}

impl ::core::fmt::Debug for GlShaderCreateError {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        write!(f, "{self}")
    }
}

impl GlShader {
    /// Compiles and creates a new OpenGL shader, created from a vertex and a fragment shader
    /// string.
    ///
    /// If the shader fails to compile, the shader object gets automatically deleted, no cleanup
    /// necessary.
    #[allow(clippy::cast_possible_truncation)] // OpenGL/graphics binding: GL-bounded numeric casts to GL* types
    /// # Errors
    ///
    /// Returns an error if the OpenGL implementation has no shader compiler, or if the vertex/fragment shader fails to compile or link.
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

        if let Some(error_id) = get_gl_shader_error(gl_context, vertex_shader_object) {
            let info_log = gl_context.get_shader_info_log(vertex_shader_object);
            gl_context.delete_shader(vertex_shader_object);
            return Err(GlShaderCreateError::Compile(GlShaderCompileError::Vertex(
                VertexShaderCompileError {
                    error_id,
                    info_log,
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

        if let Some(error_id) = get_gl_shader_error(gl_context, fragment_shader_object) {
            let info_log = gl_context.get_shader_info_log(fragment_shader_object);
            gl_context.delete_shader(vertex_shader_object);
            gl_context.delete_shader(fragment_shader_object);
            return Err(GlShaderCreateError::Compile(
                GlShaderCompileError::Fragment(FragmentShaderCompileError {
                    error_id,
                    info_log,
                }),
            ));
        }

        // Link program

        let program_id = gl_context.create_program();
        gl_context.attach_shader(program_id, vertex_shader_object);
        gl_context.attach_shader(program_id, fragment_shader_object);
        gl_context.link_program(program_id);

        if let Some(error_id) = get_gl_program_error(gl_context, program_id) {
            let info_log = gl_context.get_program_info_log(program_id);
            gl_context.delete_shader(vertex_shader_object);
            gl_context.delete_shader(fragment_shader_object);
            gl_context.delete_program(program_id);
            return Err(GlShaderCreateError::Link(GlShaderLinkError {
                error_id,
                info_log,
            }));
        }

        gl_context.delete_shader(vertex_shader_object);
        gl_context.delete_shader(fragment_shader_object);

        Ok(Self {
            program_id,
            gl_context: gl_context.clone(),
            run_destructor: true,
        })
    }

    /// Draws vertex buffers, index buffers + uniforms to the texture
    ///
    /// # Panics
    ///
    /// Panics if no framebuffer/depthbuffer was allocated (the GL object lists are empty).
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // OpenGL/graphics binding: GL-bounded numeric casts to GL* types
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose parser/builder/dispatch (one branch per input variant)
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

        let saved = GlStateSave::save(gl_context);

        // save draw()-specific state not covered by GlStateSave
        let mut current_blend_enabled = [0_u8];
        let mut current_primitive_restart_enabled = [0_u8];
        gl_context.get_boolean_v(gl::BLEND, (&mut current_blend_enabled[..]).into());
        gl_context.get_boolean_v(
            gl::PRIMITIVE_RESTART,
            (&mut current_primitive_restart_enabled[..]).into(),
        );

        // 1. Create the framebuffer
        let framebuffers = gl_context.gen_framebuffers(1);
        let framebuffer_id = *framebuffers.get(0).unwrap();
        // AUDIT: register the FBO for cleanup BEFORE the next fallible step so a
        // panic anywhere below (incl. `gen_renderbuffers().get(0).unwrap()`)
        // can't leak the FBO/RBO. Guard's Drop runs on unwind too.
        let mut fbo_rbo_guard = FboRboGuard {
            gl_context,
            framebuffer_id,
            renderbuffer_id: 0,
        };
        gl_context.bind_framebuffer(gl::FRAMEBUFFER, framebuffer_id);

        let depthbuffers = gl_context.gen_renderbuffers(1);
        let depthbuffer_id = *depthbuffers.get(0).unwrap();
        fbo_rbo_guard.renderbuffer_id = depthbuffer_id;

        gl_context.bind_texture(gl::TEXTURE_2D, texture.texture_id);
        gl_context.tex_image_2d(
            gl::TEXTURE_2D,
            0,
            gl::RGBA as i32, // NOT RGBA8 - will generate INVALID_ENUM!
            texture_size.width as i32,
            texture_size.height as i32,
            0,
            gl::RGBA, // gl::BGRA?
            gl::UNSIGNED_BYTE,
            None.into(),
        );
        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);

        gl_context.bind_renderbuffer(gl::RENDERBUFFER, depthbuffer_id);
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
            depthbuffer_id,
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
                gl::FRAMEBUFFER_INCOMPLETE_LAYER_TARGETS => {
                    println!("GL_FRAMEBUFFER_INCOMPLETE_LAYER_TARGETS");
                }
                o => {
                    println!("glFramebufferStatus returned unknown return code: {o}");
                }
            }
        }

        gl_context.viewport(0, 0, texture_size.width as i32, texture_size.height as i32);
        gl_context.enable(gl::BLEND);
        // Use GL_PRIMITIVE_RESTART (OpenGL 3.1+) instead of
        // GL_PRIMITIVE_RESTART_FIXED_INDEX (4.3+) for macOS compatibility.
        gl_context.enable(gl::PRIMITIVE_RESTART);
        unsafe {
            let gl = gl_context.get();
            if !gl.glPrimitiveRestartIndex.is_null() {
                let func: extern "system" fn(u32) =
                    core::mem::transmute(gl.glPrimitiveRestartIndex);
                func(GL_RESTART_INDEX); // u32::MAX
            }
        }
        gl_context.disable(gl::MULTISAMPLE);
        gl_context.blend_func(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA); // TODO: enable / disable
        gl_context.use_program(shader_program_id);

        // Avoid multiple calls to get_uniform_location by caching the uniform locations
        let mut uniform_locations: BTreeMap<AzString, i32> = BTreeMap::new();
        let mut max_uniform_len = 0;
        for (_, uniforms) in buffers {
            for uniform in *uniforms {
                if !uniform_locations.contains_key(&uniform.uniform_name) {
                    uniform_locations.insert(
                        uniform.uniform_name.clone(),
                        gl_context.get_uniform_location(
                            shader_program_id,
                            uniform.uniform_name.as_str(),
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

        // Reset draw()-specific state
        if u32::from(current_blend_enabled[0]) == gl::FALSE {
            gl_context.disable(gl::BLEND);
        }
        if u32::from(current_primitive_restart_enabled[0]) == gl::FALSE {
            gl_context.disable(gl::PRIMITIVE_RESTART);
        }

        // AUDIT: FBO/RBO deletion is handled by `fbo_rbo_guard`'s Drop (which
        // also runs on an unwinding panic) — reclaim them explicitly here so
        // the deletion order matches the original (delete before texture
        // metadata writes / after state restore).
        // Reset common GL state
        saved.restore(gl_context);
        drop(fbo_rbo_guard);

        texture.format = RawImageFormat::RGBA8;
        texture.flags = TextureFlags {
            is_opaque: false,
            is_video_texture: false,
        };
    }
}

#[allow(clippy::cast_possible_wrap)] // OpenGL/graphics binding: GL-bounded numeric casts to GL* types
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

#[allow(clippy::cast_possible_wrap)] // OpenGL/graphics binding: GL-bounded numeric casts to GL* types
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

#[cfg(test)]
mod audit_tests {
    use super::*;

    // AUDIT: FFI slice/str accessors must return an empty slice (not form a
    // slice over a null/dangling ptr, which is UB) when handed a null or
    // zero-length descriptor from C.
    #[test]
    fn refstr_null_and_empty_is_empty_str() {
        let null = Refstr {
            ptr: core::ptr::null(),
            len: 0,
        };
        assert_eq!(null.as_str(), "");

        // null ptr but nonzero len (garbage from FFI) must also not deref.
        let null_lenful = Refstr {
            ptr: core::ptr::null(),
            len: 5,
        };
        assert_eq!(null_lenful.as_str(), "");

        let s = "hello";
        let good: Refstr = s.into();
        assert_eq!(good.as_str(), "hello");
    }

    #[test]
    fn refstr_vec_ref_null_is_empty_slice() {
        let null = RefstrVecRef {
            ptr: core::ptr::null(),
            len: 3,
        };
        assert!(null.as_slice().is_empty());
    }

    #[test]
    fn u8_vec_ref_null_is_empty_slice() {
        let null = U8VecRef {
            ptr: core::ptr::null(),
            len: 8,
        };
        assert!(null.as_slice().is_empty());

        let data = [1u8, 2, 3];
        let good: U8VecRef = (&data[..]).into();
        assert_eq!(good.as_slice(), &[1, 2, 3]);
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // exact f32 compares are the point: these assert bit-exact round-trips / lossy-cast values
mod autotest_generated {
    use super::*;

    // ---------------------------------------------------------------------
    // Test scaffolding: a "null" GL context.
    //
    // Every field of `GenericGlContext` is a `*mut c_void` function pointer
    // (780 of them, no other field types), and every gl-context-loader method
    // null-checks its pointer and returns a default (0 / empty Vec / "") rather
    // than calling through. So an all-zero context is a SAFE no-op GL driver:
    // it lets us drive azul's whole wrapper surface off-GPU and assert how the
    // wrappers behave when the driver hands back degenerate values -- which is
    // exactly the "broken driver" path `is_gl_usable()` exists for.
    // ---------------------------------------------------------------------
    fn null_gl() -> Rc<GenericGlContext> {
        // SAFETY: `GenericGlContext` is `repr(C)` and every field is a raw
        // pointer, for which the all-zero bit pattern (null) is valid.
        Rc::new(unsafe { core::mem::zeroed::<GenericGlContext>() })
    }

    fn null_ctx() -> GlContextPtr {
        GlContextPtr::new(RendererType::Software, null_gl())
    }

    fn test_texture(texture_id: GLuint, width: u32, height: u32) -> Texture {
        Texture::create(
            texture_id,
            TextureFlags {
                is_opaque: false,
                is_video_texture: false,
            },
            PhysicalSizeU32 { width, height },
            ColorU {
                r: 1,
                g: 2,
                b: 3,
                a: 4,
            },
            null_ctx(),
            RawImageFormat::RGBA8,
        )
    }

    const ALL_ATTRIB_TYPES: [VertexAttributeType; 5] = [
        VertexAttributeType::Float,
        VertexAttributeType::Double,
        VertexAttributeType::UnsignedByte,
        VertexAttributeType::UnsignedShort,
        VertexAttributeType::UnsignedInt,
    ];

    const ALL_INDEX_FORMATS: [IndexBufferFormat; 6] = [
        IndexBufferFormat::Points,
        IndexBufferFormat::Lines,
        IndexBufferFormat::LineStrip,
        IndexBufferFormat::Triangles,
        IndexBufferFormat::TriangleStrip,
        IndexBufferFormat::TriangleFan,
    ];

    // =====================================================================
    // Refstr / *VecRef / *VecRefMut -- FFI slice+str accessors
    // (parsers: malformed / huge / boundary / unicode)
    // =====================================================================

    #[test]
    fn refstr_roundtrips_unicode_multibyte_and_combining_marks() {
        // Non-ASCII, emoji, and combining marks must survive byte-exactly:
        // `as_str` rebuilds the str via `from_utf8_unchecked` over the raw bytes.
        for s in [
            "\u{1F600}",
            "日本語のテキスト",
            "e\u{0301}\u{0328}combining",
            "\u{0}nul\u{0}embedded\u{0}",
            "\u{FFFD}\u{200B}zero-width",
        ] {
            let r: Refstr = s.into();
            assert_eq!(r.as_str(), s);
            assert_eq!(r.as_str().len(), s.len());
        }
    }

    #[test]
    fn refstr_len_zero_over_nonempty_buffer_is_empty_not_dangling() {
        // len == 0 must short-circuit even when ptr is a perfectly valid buffer.
        let backing = "not empty";
        let r = Refstr {
            ptr: backing.as_ptr(),
            len: 0,
        };
        assert_eq!(r.as_str(), "");
    }

    #[test]
    fn refstr_huge_input_does_not_hang() {
        // 1 MB single-token string: as_str is O(1) (no scan), so this must be instant.
        let huge = "x".repeat(1_000_000);
        let r: Refstr = huge.as_str().into();
        assert_eq!(r.as_str().len(), 1_000_000);
        assert_eq!(r.as_str(), huge.as_str());
    }

    #[test]
    fn refstr_debug_on_null_does_not_panic() {
        let null = Refstr {
            ptr: core::ptr::null(),
            len: usize::MAX,
        };
        // Debug goes through as_str(); a null guard failure here would be UB, not a panic.
        assert_eq!(alloc::format!("{null:?}"), "\"\"");
    }

    #[test]
    fn refstr_clone_preserves_ptr_and_len() {
        let s = "clone me";
        let r: Refstr = s.into();
        let c = r.clone();
        assert_eq!(c.as_str(), s);
        assert!(core::ptr::eq(c.ptr, r.ptr));
        assert_eq!(c.len, r.len);
    }

    #[test]
    fn every_vec_ref_with_null_ptr_and_nonzero_len_is_empty() {
        // The FFI boundary can hand us a null ptr with a garbage (nonzero) len.
        // Forming a slice over null is UB, so every accessor must return empty.
        const GARBAGE_LEN: usize = usize::MAX;

        assert!(RefstrVecRef {
            ptr: core::ptr::null(),
            len: GARBAGE_LEN
        }
        .as_slice()
        .is_empty());
        assert!(GLuintVecRef {
            ptr: core::ptr::null(),
            len: GARBAGE_LEN
        }
        .as_slice()
        .is_empty());
        assert!(GLenumVecRef {
            ptr: core::ptr::null(),
            len: GARBAGE_LEN
        }
        .as_slice()
        .is_empty());
        assert!(U8VecRef {
            ptr: core::ptr::null(),
            len: GARBAGE_LEN
        }
        .as_slice()
        .is_empty());
        assert!(F32VecRef {
            ptr: core::ptr::null(),
            len: GARBAGE_LEN
        }
        .as_slice()
        .is_empty());
        assert!(I32VecRef {
            ptr: core::ptr::null(),
            len: GARBAGE_LEN
        }
        .as_slice()
        .is_empty());

        // ...and the same for the mutable variants, through BOTH accessors.
        let mut m64 = GLint64VecRefMut {
            ptr: core::ptr::null_mut(),
            len: GARBAGE_LEN,
        };
        assert!(m64.as_slice().is_empty());
        assert!(m64.as_mut_slice().is_empty());

        let mut mf = GLfloatVecRefMut {
            ptr: core::ptr::null_mut(),
            len: GARBAGE_LEN,
        };
        assert!(mf.as_slice().is_empty());
        assert!(mf.as_mut_slice().is_empty());

        let mut mi = GLintVecRefMut {
            ptr: core::ptr::null_mut(),
            len: GARBAGE_LEN,
        };
        assert!(mi.as_slice().is_empty());
        assert!(mi.as_mut_slice().is_empty());

        let mut mb = GLbooleanVecRefMut {
            ptr: core::ptr::null_mut(),
            len: GARBAGE_LEN,
        };
        assert!(mb.as_slice().is_empty());
        assert!(mb.as_mut_slice().is_empty());

        let mut mu8 = U8VecRefMut {
            ptr: core::ptr::null_mut(),
            len: GARBAGE_LEN,
        };
        assert!(mu8.as_slice().is_empty());
        assert!(mu8.as_mut_slice().is_empty());
    }

    #[test]
    fn vec_refs_roundtrip_from_slice() {
        let u: [GLuint; 3] = [0, 1, u32::MAX];
        assert_eq!(GLuintVecRef::from(&u[..]).as_slice(), &u[..]);

        let e: [GLenum; 2] = [gl::TRIANGLES, u32::MAX];
        assert_eq!(GLenumVecRef::from(&e[..]).as_slice(), &e[..]);

        let b: [u8; 4] = [0, 127, 128, 255];
        assert_eq!(U8VecRef::from(&b[..]).as_slice(), &b[..]);

        let i: [i32; 3] = [i32::MIN, 0, i32::MAX];
        assert_eq!(I32VecRef::from(&i[..]).as_slice(), &i[..]);

        // Empty (but non-null) slices must also come back empty.
        let empty: [u8; 0] = [];
        assert!(U8VecRef::from(&empty[..]).as_slice().is_empty());
    }

    #[test]
    fn f32_vec_ref_preserves_nan_inf_and_subnormals_bit_exactly() {
        // These are pointer casts, not conversions: NaN payloads and -0.0 must
        // survive bit-for-bit (a NaN-normalizing copy would be a real bug).
        let vals: [f32; 6] = [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            -0.0,
            f32::MIN_POSITIVE / 2.0, // subnormal
            f32::MAX,
        ];
        let r = F32VecRef::from(&vals[..]);
        let got = r.as_slice();
        assert_eq!(got.len(), vals.len());
        for (g, v) in got.iter().zip(vals.iter()) {
            assert_eq!(g.to_bits(), v.to_bits());
        }
        assert!(got[0].is_nan());
        assert!(got[3].is_sign_negative());
    }

    #[test]
    fn glint64_vec_ref_mut_handles_boundary_values() {
        let mut vals: [GLint64; 4] = [i64::MIN, -1, 0, i64::MAX];
        let mut r = GLint64VecRefMut::from(&mut vals[..]);
        assert_eq!(r.as_slice(), &[i64::MIN, -1, 0, i64::MAX]);

        // Writes through as_mut_slice must land in the caller's buffer.
        r.as_mut_slice()[0] = i64::MAX;
        r.as_mut_slice()[3] = i64::MIN;
        assert_eq!(vals, [i64::MAX, -1, 0, i64::MIN]);
    }

    #[test]
    fn mut_vec_refs_write_through_to_the_caller_buffer() {
        let mut floats: [GLfloat; 2] = [0.0, 0.0];
        GLfloatVecRefMut::from(&mut floats[..]).as_mut_slice()[1] = f32::NAN;
        assert!(floats[1].is_nan());

        let mut ints: [GLint; 2] = [0, 0];
        GLintVecRefMut::from(&mut ints[..]).as_mut_slice()[0] = i32::MIN;
        assert_eq!(ints[0], i32::MIN);

        let mut bools: [GLboolean; 2] = [0, 0];
        GLbooleanVecRefMut::from(&mut bools[..]).as_mut_slice()[1] = 255;
        assert_eq!(bools[1], 255);

        let mut bytes: [u8; 3] = [1, 2, 3];
        U8VecRefMut::from(&mut bytes[..]).as_mut_slice()[2] = 0;
        assert_eq!(bytes, [1, 2, 0]);
    }

    #[test]
    fn u8_vec_ref_ord_eq_hash_agree_with_the_underlying_slice() {
        use core::hash::{BuildHasher, Hasher};

        use alloc::collections::BTreeSet;

        let a = [1u8, 2, 3];
        let b = [1u8, 2, 4];
        let ra = U8VecRef::from(&a[..]);
        let rb = U8VecRef::from(&b[..]);

        assert_eq!(ra, U8VecRef::from(&a[..]));
        assert!(ra < rb);
        assert_eq!(ra.cmp(&rb), a[..].cmp(&b[..]));

        // A null ref must compare equal to an empty one (both are the empty slice).
        let null = U8VecRef {
            ptr: core::ptr::null(),
            len: 99,
        };
        let empty: [u8; 0] = [];
        assert_eq!(null, U8VecRef::from(&empty[..]));

        // Hash must be consistent with Eq (equal values -> equal hashes).
        fn hash_of(v: &U8VecRef) -> u64 {
            let mut h = core::hash::BuildHasherDefault::<TestHasher>::default().build_hasher();
            v.hash(&mut h);
            h.finish()
        }
        #[derive(Default)]
        struct TestHasher(u64);
        impl Hasher for TestHasher {
            fn finish(&self) -> u64 {
                self.0
            }
            fn write(&mut self, bytes: &[u8]) {
                for b in bytes {
                    self.0 = self.0.wrapping_mul(31).wrapping_add(u64::from(*b));
                }
            }
        }
        assert_eq!(hash_of(&ra), hash_of(&U8VecRef::from(&a[..])));
        assert_eq!(hash_of(&null), hash_of(&U8VecRef::from(&empty[..])));

        // Ord must be a total order good enough for a BTreeSet.
        let mut set = BTreeSet::new();
        set.insert(ra.clone());
        set.insert(U8VecRef::from(&a[..]));
        set.insert(rb);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn refstr_vec_ref_roundtrips_and_maps_back_to_strs() {
        let strs = ["", "a", "\u{1F600}"];
        let refstrs: Vec<Refstr> = strs.iter().map(|s| Refstr::from(*s)).collect();
        let vec_ref = RefstrVecRef::from(&refstrs[..]);
        let got: Vec<&str> = vec_ref.as_slice().iter().map(Refstr::as_str).collect();
        assert_eq!(got, strs);
    }

    // =====================================================================
    // GLsyncPtr (constructor + round-trip)
    // =====================================================================

    #[test]
    fn glsync_ptr_roundtrips_null_and_nonnull() {
        let null = GLsyncPtr::new(core::ptr::null());
        assert!(null.clone().get().is_null());
        assert_eq!(alloc::format!("{null:?}"), "0x0");

        // A non-null (never dereferenced) sentinel must round-trip identically.
        let sentinel = usize::MAX as *const c_void;
        let p = GLsyncPtr::new(sentinel);
        assert_eq!(p.clone().get() as usize, usize::MAX);
        assert!(p.run_destructor);
    }

    // =====================================================================
    // shader_with_glsl_version (parser: the `#version` line swap)
    // =====================================================================

    #[cfg(feature = "std")]
    #[test]
    fn shader_with_glsl_version_replaces_only_the_first_line() {
        let src = b"#version 150\nbody line 1\nbody line 2";
        let out = shader_with_glsl_version(src, b"#version 300 es\n");
        assert_eq!(out, b"#version 300 es\nbody line 1\nbody line 2".to_vec());
    }

    #[cfg(feature = "std")]
    #[test]
    fn shader_with_glsl_version_empty_src_yields_just_the_version_line() {
        // No newline in src -> body_start = 0 (the map_or default), so the whole
        // (empty) src is kept and only the version line is prepended.
        assert_eq!(
            shader_with_glsl_version(b"", b"#version 150\n"),
            b"#version 150\n".to_vec()
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn shader_with_glsl_version_src_without_newline_is_kept_whole() {
        // A src with NO newline has no first line to strip: body_start stays 0,
        // so the version line is prepended and nothing is lost.
        let out = shader_with_glsl_version(b"void main(){}", b"#version 150\n");
        assert_eq!(out, b"#version 150\nvoid main(){}".to_vec());
    }

    #[cfg(feature = "std")]
    #[test]
    fn shader_with_glsl_version_leading_newline_drops_only_that_newline() {
        let out = shader_with_glsl_version(b"\nrest", b"V\n");
        assert_eq!(out, b"V\nrest".to_vec());
    }

    #[cfg(feature = "std")]
    #[test]
    fn shader_with_glsl_version_empty_version_line_still_strips_first_line() {
        let out = shader_with_glsl_version(b"#version 150\nbody", b"");
        assert_eq!(out, b"body".to_vec());

        // Both empty -> empty, no panic.
        assert!(shader_with_glsl_version(b"", b"").is_empty());
    }

    #[cfg(feature = "std")]
    #[test]
    fn shader_with_glsl_version_is_byte_exact_on_invalid_utf8_and_nul_bytes() {
        // Shader sources are bytes, not str: invalid UTF-8 / NULs must pass
        // through untouched (this is fed straight to glShaderSource).
        let src = b"#version 150\n\xFF\xFE\x00\x80body";
        let out = shader_with_glsl_version(src, b"#version 100\n");
        assert_eq!(out, b"#version 100\n\xFF\xFE\x00\x80body".to_vec());

        // A src that is ONLY invalid bytes with no newline is kept whole.
        let out2 = shader_with_glsl_version(&[0xFF, 0xFE, 0x00], b"V\n");
        assert_eq!(out2, vec![b'V', b'\n', 0xFF, 0xFE, 0x00]);
    }

    #[cfg(feature = "std")]
    #[test]
    fn shader_with_glsl_version_handles_a_1mb_source_without_hanging() {
        let mut src = b"#version 150\n".to_vec();
        src.resize(src.len() + 1_000_000, b'x');
        let out = shader_with_glsl_version(&src, b"#version 300 es\n");
        assert_eq!(out.len(), b"#version 300 es\n".len() + 1_000_000);
        assert!(out.starts_with(b"#version 300 es\n"));
        assert!(out.ends_with(b"xxxx"));
    }

    // =====================================================================
    // glsl_version_candidates (invariants the version probe relies on)
    // =====================================================================

    #[test]
    fn glsl_version_candidates_are_wellformed_and_distinct() {
        for gl_type in [GlType::Gl, GlType::Gles] {
            let candidates = glsl_version_candidates(gl_type);
            assert!(
                !candidates.is_empty(),
                "{gl_type:?} must have at least one candidate or the probe can never succeed"
            );
            for c in candidates {
                // GlContextPtr::new parses these back out with
                // `trim().trim_start_matches("#version ")`, which requires
                // valid UTF-8, the exact prefix, and a trailing newline.
                let s = core::str::from_utf8(c).expect("candidate must be valid UTF-8");
                assert!(s.starts_with("#version "), "{s:?}");
                assert!(s.ends_with('\n'), "{s:?}");
                let parsed = s.trim().trim_start_matches("#version ");
                assert!(!parsed.is_empty(), "{s:?} must parse to a nonempty version");
            }
            // No duplicates: a repeated candidate would just re-probe a known failure.
            for (i, a) in candidates.iter().enumerate() {
                for b in candidates.iter().skip(i + 1) {
                    assert_ne!(a, b, "duplicate candidate in {gl_type:?}");
                }
            }
        }

        // The GL and GLES lists must be disjoint (a GLES driver rejects `#version 150`).
        for g in glsl_version_candidates(GlType::Gl) {
            assert!(!glsl_version_candidates(GlType::Gles).contains(g));
        }

        // Documented examples from the API docs: "150" (GL) and "300 es" (GLES).
        let first_gl = core::str::from_utf8(glsl_version_candidates(GlType::Gl)[0]).unwrap();
        assert_eq!(first_gl.trim().trim_start_matches("#version "), "150");
        let first_es = core::str::from_utf8(glsl_version_candidates(GlType::Gles)[0]).unwrap();
        assert_eq!(first_es.trim().trim_start_matches("#version "), "300 es");
    }

    #[test]
    fn gl_type_from_context_gl_type_is_total() {
        assert_eq!(GlType::from(GlContextGlType::Gl), GlType::Gl);
        assert_eq!(GlType::from(GlContextGlType::GlEs), GlType::Gles);
    }

    // =====================================================================
    // GlContextPtr -- construction + the documented "unusable driver" fallback
    // =====================================================================

    #[test]
    fn gl_context_ptr_software_is_never_usable_and_has_no_shaders() {
        // Documented: "Always `false` for a Software context (which never
        // compiles these shaders)".
        let ctx = GlContextPtr::new(RendererType::Software, null_gl());
        assert!(!ctx.is_gl_usable());
        assert_eq!(ctx.get_svg_shader(), 0);
        assert_eq!(ctx.get_brush_shader(), 0);
        assert_eq!(ctx.get_fxaa_shader(), 0);
        assert_eq!(ctx.get_usable_glsl_version().as_str(), "");
        assert_eq!(ctx.renderer_type, RendererType::Software);
    }

    #[test]
    fn gl_context_ptr_hardware_with_broken_driver_falls_back_instead_of_panicking() {
        // THE contract `is_gl_usable()` exists for: context creation "succeeds"
        // but the driver compiles nothing. The probe must try every candidate
        // version, fail them all, and leave every program id at 0 -- so the
        // caller can fall back to CPU rendering -- rather than panicking or
        // handing out a garbage program id.
        let ctx = GlContextPtr::new(RendererType::Hardware, null_gl());
        assert!(
            !ctx.is_gl_usable(),
            "a driver that compiles nothing must report is_gl_usable() == false"
        );
        assert_eq!(ctx.get_svg_shader(), 0);
        assert_eq!(ctx.get_brush_shader(), 0);
        assert_eq!(ctx.get_fxaa_shader(), 0);
        assert_eq!(
            ctx.get_usable_glsl_version().as_str(),
            "",
            "glsl_version must be empty when the context is unusable"
        );
    }

    #[test]
    fn gl_context_ptr_get_type_defaults_to_desktop_gl_on_an_empty_version_string() {
        // get_type() sniffs the GL_VERSION string for "OpenGL ES"; a driver that
        // returns nothing must deterministically fall out as desktop Gl.
        assert_eq!(null_ctx().get_type(), GlType::Gl);
    }

    #[test]
    fn gl_context_ptr_clone_is_eq_but_distinct_contexts_are_not() {
        // Eq/Ord are identity-based (`as_usize` = the inner Rc address), so a
        // clone must compare equal and two independently created contexts must not.
        let a = null_ctx();
        let clone = a.clone();
        assert_eq!(a, clone);
        assert_eq!(a.cmp(&clone), core::cmp::Ordering::Equal);
        assert_eq!(a.partial_cmp(&clone), Some(core::cmp::Ordering::Equal));

        let b = null_ctx();
        assert_ne!(a, b);
        // Ord must be antisymmetric and consistent with PartialOrd.
        assert_eq!(a.cmp(&b), a.partial_cmp(&b).unwrap());
        assert_eq!(a.cmp(&b).reverse(), b.cmp(&a));

        // The clone must point at the same underlying GL context.
        assert!(Rc::ptr_eq(a.get(), clone.get()));
        assert!(!Rc::ptr_eq(a.get(), b.get()));
    }

    #[test]
    fn gl_context_ptr_gen_family_returns_empty_for_every_n_including_negatives() {
        // A driver that generates nothing returns an EMPTY list. Callers index
        // into this ([0] / .get(0).unwrap()), so the wrappers must at least not
        // panic on the way out -- and must not choke on a negative/extreme n.
        let ctx = null_ctx();
        for n in [0, 1, -1, i32::MIN, i32::MAX] {
            assert!(ctx.gen_buffers(n).as_slice().is_empty(), "gen_buffers({n})");
            assert!(ctx.gen_textures(n).as_slice().is_empty(), "gen_textures({n})");
            assert!(
                ctx.gen_framebuffers(n).as_slice().is_empty(),
                "gen_framebuffers({n})"
            );
            assert!(
                ctx.gen_renderbuffers(n).as_slice().is_empty(),
                "gen_renderbuffers({n})"
            );
            assert!(
                ctx.gen_vertex_arrays(n).as_slice().is_empty(),
                "gen_vertex_arrays({n})"
            );
            assert!(ctx.gen_queries(n).as_slice().is_empty(), "gen_queries({n})");
        }
    }

    #[test]
    fn gl_context_ptr_integer_extremes_do_not_panic() {
        // Offsets/sizes/limits at the boundary of their integer types. These are
        // passed straight through to GL, so the wrapper must not do arithmetic
        // that overflows on the way.
        let ctx = null_ctx();
        let data = [0u8; 4];
        let void_ptr = || GlVoidPtrConst {
            ptr: data.as_ptr().cast(),
            run_destructor: false,
        };

        for offset in [0isize, -1, isize::MIN, isize::MAX] {
            for size in [0isize, -1, isize::MIN, isize::MAX] {
                ctx.buffer_sub_data_untyped(gl::ARRAY_BUFFER, offset, size, void_ptr());
                let _ = ctx.map_buffer_range(gl::ARRAY_BUFFER, offset, size, 0);
            }
        }
        ctx.buffer_data_untyped(gl::ARRAY_BUFFER, isize::MIN, void_ptr(), gl::STATIC_DRAW);

        // usize offset at the extreme (reinterpreted as a byte offset pointer).
        for offset in [0usize, 1, usize::MAX] {
            ctx.tex_sub_image_2d_pbo(
                gl::TEXTURE_2D,
                0,
                0,
                0,
                1,
                1,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                offset,
            );
        }

        ctx.pixel_store_i(gl::PACK_ALIGNMENT, i32::MIN);
        ctx.pixel_store_i(gl::PACK_ALIGNMENT, i32::MAX);
        let _ = ctx.unmap_buffer(gl::ARRAY_BUFFER);
    }

    #[test]
    fn gl_context_ptr_float_extremes_do_not_panic() {
        // NaN / inf / subnormal floats must pass through the f32 wrappers untouched.
        let ctx = null_ctx();
        for v in [
            0.0,
            1.0,
            -1.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MIN,
            f32::MAX,
        ] {
            ctx.sample_coverage(v, false);
            ctx.sample_coverage(v, true);
            ctx.polygon_offset(v, v);
        }
    }

    #[test]
    fn gl_context_ptr_shader_source_handles_empty_unicode_and_embedded_nuls() {
        // shader_source NUL-terminates each string itself; an already-embedded
        // NUL or multibyte UTF-8 must not panic on the way through.
        let ctx = null_ctx();
        let strings: StringVec = vec![
            AzString::from(String::new()),
            AzString::from("\u{1F600} emoji".to_string()),
            AzString::from("has\u{0}nul".to_string()),
            AzString::from("x".repeat(100_000)),
        ]
        .into();
        ctx.shader_source(0, strings);

        // An empty list of sources must also be fine.
        ctx.shader_source(u32::MAX, Vec::<AzString>::new().into());
    }

    #[test]
    fn gl_context_ptr_read_pixels_sizes_the_buffer_from_the_dimensions() {
        // NOTE: only SAFE (small, positive) dimensions are exercised here.
        // Negative dimensions are a live hazard in this path -- see the report:
        // gl-context-loader's read_pixels does
        //   `vec![0; width as usize * height as usize * bit_depth]`
        // BEFORE its null-pointer check, so a negative width sign-extends to
        // ~usize::MAX and the multiply overflows (debug) / requests an
        // exabyte-scale allocation (release). Deliberately not provoked.
        let ctx = null_ctx();
        let px = ctx.read_pixels(0, 0, 2, 3, gl::RGBA, gl::UNSIGNED_BYTE);
        assert_eq!(px.len(), 2 * 3 * 4, "RGBA/UNSIGNED_BYTE = 4 bytes per pixel");

        // A zero-size read is the degenerate-but-safe case.
        assert_eq!(
            ctx.read_pixels(0, 0, 0, 0, gl::RGBA, gl::UNSIGNED_BYTE).len(),
            0
        );
    }

    #[test]
    fn gl_context_ptr_read_pixels_into_buffer_accepts_null_and_undersized_targets() {
        // The destination is caller-owned here (no allocation from the dims), so
        // a null / empty / undersized target must simply no-op.
        let ctx = null_ctx();
        ctx.read_pixels_into_buffer(
            0,
            0,
            4,
            4,
            gl::RGBA,
            gl::UNSIGNED_BYTE,
            U8VecRefMut {
                ptr: core::ptr::null_mut(),
                len: 64,
            },
        );

        let mut small = [0u8; 1];
        ctx.read_pixels_into_buffer(
            0,
            0,
            4,
            4,
            gl::RGBA,
            gl::UNSIGNED_BYTE,
            (&mut small[..]).into(),
        );
        assert_eq!(small, [0u8; 1]);
    }

    #[test]
    fn gl_context_ptr_uniform_getters_accept_null_result_buffers() {
        let ctx = null_ctx();
        ctx.get_uniform_iv(
            0,
            -1,
            GLintVecRefMut {
                ptr: core::ptr::null_mut(),
                len: 16,
            },
        );
        ctx.get_uniform_fv(
            0,
            i32::MIN,
            GLfloatVecRefMut {
                ptr: core::ptr::null_mut(),
                len: 16,
            },
        );

        // ...and real buffers, at extreme locations.
        let mut ints = [0i32; 2];
        ctx.get_uniform_iv(u32::MAX, i32::MAX, (&mut ints[..]).into());
        let mut floats = [0f32; 2];
        ctx.get_uniform_fv(u32::MAX, i32::MAX, (&mut floats[..]).into());
    }

    #[test]
    fn gl_context_ptr_get_uniform_indices_handles_empty_and_null_name_lists() {
        let ctx = null_ctx();
        let empty: &[Refstr] = &[];
        assert!(ctx
            .get_uniform_indices(0, empty.into())
            .as_slice()
            .is_empty());
        assert!(ctx
            .get_uniform_indices(
                0,
                RefstrVecRef {
                    ptr: core::ptr::null(),
                    len: 4,
                },
            )
            .as_slice()
            .is_empty());
    }

    #[test]
    fn gl_context_ptr_delete_family_accepts_empty_and_null_id_lists() {
        // Deleting nothing must be a no-op, not an out-of-bounds read.
        let ctx = null_ctx();
        let empty: &[GLuint] = &[];
        ctx.delete_buffers(empty.into());
        ctx.delete_textures(empty.into());
        ctx.delete_framebuffers(empty.into());
        ctx.delete_renderbuffers(empty.into());
        ctx.delete_vertex_arrays(empty.into());
        ctx.delete_queries(empty.into());

        let null = || GLuintVecRef {
            ptr: core::ptr::null(),
            len: 7,
        };
        ctx.delete_buffers(null());
        ctx.delete_textures(null());
        ctx.delete_framebuffers(null());
        ctx.delete_renderbuffers(null());
        ctx.delete_vertex_arrays(null());
        ctx.delete_queries(null());

        // A real (bogus) id list, incl. 0 and u32::MAX, must also be fine.
        let ids: &[GLuint] = &[0, 1, u32::MAX];
        ctx.delete_buffers(ids.into());
        ctx.delete_textures(ids.into());
    }

    #[test]
    fn gl_context_ptr_draw_buffers_accepts_an_empty_list() {
        let ctx = null_ctx();
        let empty: &[GLenum] = &[];
        ctx.draw_buffers(empty.into());
        ctx.draw_buffers(
            GLenumVecRef {
                ptr: core::ptr::null(),
                len: 3,
            },
        );
    }

    // =====================================================================
    // VertexAttributeType / VertexAttribute / VertexLayout (numeric + invariants)
    // =====================================================================

    #[test]
    fn vertex_attribute_type_mem_size_matches_the_rust_type_it_names() {
        assert_eq!(
            VertexAttributeType::Float.get_mem_size(),
            core::mem::size_of::<f32>()
        );
        assert_eq!(
            VertexAttributeType::Double.get_mem_size(),
            core::mem::size_of::<f64>()
        );
        assert_eq!(
            VertexAttributeType::UnsignedByte.get_mem_size(),
            core::mem::size_of::<u8>()
        );
        assert_eq!(
            VertexAttributeType::UnsignedShort.get_mem_size(),
            core::mem::size_of::<u16>()
        );
        assert_eq!(
            VertexAttributeType::UnsignedInt.get_mem_size(),
            core::mem::size_of::<u32>()
        );

        // Every size must be nonzero -- a 0 would make get_stride() collapse to 0
        // and silently produce a degenerate vertex layout.
        for t in ALL_ATTRIB_TYPES {
            assert!(t.get_mem_size() > 0, "{t:?}");
        }
    }

    #[test]
    fn vertex_attribute_type_gl_ids_are_distinct_and_nonzero() {
        for (i, a) in ALL_ATTRIB_TYPES.iter().enumerate() {
            assert_ne!(a.get_gl_id(), 0, "{a:?} maps to the GL 'no type' id 0");
            for b in ALL_ATTRIB_TYPES.iter().skip(i + 1) {
                assert_ne!(
                    a.get_gl_id(),
                    b.get_gl_id(),
                    "{a:?} and {b:?} share a GL id -- one of them would upload as the wrong type"
                );
            }
        }
        assert_eq!(VertexAttributeType::Float.get_gl_id(), gl::FLOAT);
        assert_eq!(
            VertexAttributeType::UnsignedByte.get_gl_id(),
            gl::UNSIGNED_BYTE
        );
    }

    #[test]
    fn vertex_attribute_get_stride_at_zero_and_at_the_overflow_boundary() {
        let attr = |ty, item_count| VertexAttribute {
            va_name: AzString::from_const_str("vAttrXY"),
            layout_location: OptionUsize::None,
            attribute_type: ty,
            item_count,
        };

        // Zero items -> zero stride.
        for t in ALL_ATTRIB_TYPES {
            assert_eq!(attr(t, 0).get_stride(), 0, "{t:?}");
        }

        // Exact multiplication for representative counts.
        assert_eq!(attr(VertexAttributeType::Float, 2).get_stride(), 8);
        assert_eq!(attr(VertexAttributeType::Double, 4).get_stride(), 32);
        assert_eq!(attr(VertexAttributeType::UnsignedByte, 3).get_stride(), 3);

        // The LARGEST item_count that does not overflow `mem_size * item_count`.
        // (NOTE: get_stride() is a plain `*`, so an item_count above this bound
        // overflow-panics in debug / wraps in release -- see the report.)
        for t in ALL_ATTRIB_TYPES {
            let max_items = usize::MAX / t.get_mem_size();
            assert_eq!(
                attr(t, max_items).get_stride(),
                max_items * t.get_mem_size(),
                "{t:?} at the overflow boundary"
            );
        }
    }

    #[test]
    fn vertex_layout_stride_is_the_sum_of_its_fields() {
        let attr = |name: &str, ty, item_count| VertexAttribute {
            va_name: AzString::from(name.to_string()),
            layout_location: OptionUsize::None,
            attribute_type: ty,
            item_count,
        };

        // Empty layout: zero fields, zero total stride.
        let empty = VertexLayout {
            fields: VertexAttributeVec::from_const_slice(&[]),
        };
        assert_eq!(
            empty.fields.iter().map(VertexAttribute::get_stride).sum::<usize>(),
            0
        );

        // vec2<f32> + vec4<u8> = 8 + 4 = 12 bytes per vertex.
        let layout = VertexLayout {
            fields: vec![
                attr("vAttrXY", VertexAttributeType::Float, 2),
                attr("vColor", VertexAttributeType::UnsignedByte, 4),
            ]
            .into(),
        };
        let total: usize = layout.fields.iter().map(VertexAttribute::get_stride).sum();
        assert_eq!(total, 12);

        // Equality/Hash are structural, so an identical layout compares equal.
        assert_eq!(layout, layout.clone());
    }

    #[test]
    fn index_buffer_format_gl_ids_are_distinct() {
        // A collision here would silently draw the wrong primitive.
        for (i, a) in ALL_INDEX_FORMATS.iter().enumerate() {
            for b in ALL_INDEX_FORMATS.iter().skip(i + 1) {
                assert_ne!(a.get_gl_id(), b.get_gl_id(), "{a:?} vs {b:?}");
            }
        }
        assert_eq!(IndexBufferFormat::Points.get_gl_id(), gl::POINTS);
        assert_eq!(
            IndexBufferFormat::TriangleStrip.get_gl_id(),
            gl::TRIANGLE_STRIP
        );
    }

    // =====================================================================
    // Uniform / UniformType (NaN semantics feed GlShader::draw's dedupe)
    // =====================================================================

    #[test]
    fn uniform_type_nan_never_equals_itself_so_draw_always_reuploads_it() {
        // GlShader::draw skips re-setting a uniform when
        // `current_uniforms[i] != Some(uniform.uniform_type)`. With a NaN payload
        // that comparison is ALWAYS unequal, so a NaN uniform is re-uploaded on
        // every buffer -- correct (never stale), just not deduped. Pin it.
        let nan = UniformType::Float(f32::NAN);
        assert_ne!(nan, UniformType::Float(f32::NAN));
        assert_ne!(Some(nan), Some(nan));

        let nan_vec = UniformType::FloatVec4([f32::NAN, 0.0, 0.0, 0.0]);
        assert_ne!(nan_vec, nan_vec);

        // The flip side: +0.0 and -0.0 compare EQUAL, so a -0.0 -> +0.0 change is
        // deduped away and never re-uploaded. Harmless for a uniform, but pinned
        // so a change in semantics is visible.
        assert_eq!(UniformType::Float(0.0), UniformType::Float(-0.0));

        // Integer uniforms have no such wrinkle: they dedupe exactly.
        assert_eq!(UniformType::Int(i32::MIN), UniformType::Int(i32::MIN));
        assert_ne!(UniformType::Int(0), UniformType::UnsignedInt(0));
    }

    #[test]
    fn uniform_type_matrix_transpose_flag_is_part_of_its_identity() {
        let m = [0.0f32; 4];
        assert_ne!(
            UniformType::Matrix2 {
                transpose: false,
                matrix: m
            },
            UniformType::Matrix2 {
                transpose: true,
                matrix: m
            },
        );
        // Same payload, different arity -> different uniforms.
        assert_ne!(
            UniformType::FloatVec2([1.0, 2.0]),
            UniformType::IntVec2([1, 2])
        );
    }

    #[test]
    fn uniform_create_preserves_empty_unicode_and_huge_names() {
        let huge = "n".repeat(100_000);
        for name in ["", "u_color", "\u{1F600}", "e\u{0301}", huge.as_str()] {
            let u = Uniform::create(name.to_string(), UniformType::Int(0));
            assert_eq!(u.uniform_name.as_str(), name);
        }
    }

    // =====================================================================
    // GlShader -- the no-shader-compiler error path
    // =====================================================================

    #[test]
    fn gl_shader_new_reports_no_shader_compiler_instead_of_panicking() {
        // A driver that reports no shader compiler (GL_SHADER_COMPILER == FALSE)
        // must produce a clean Err -- for ANY source, including empty, garbage,
        // unicode and very large ones.
        let ctx = null_ctx();
        let huge = "x".repeat(200_000);
        for (vert, frag) in [
            ("", ""),
            ("   \t\n  ", "\n\n"),
            ("not glsl at all ;;;{{{", "\u{1F600}"),
            ("void main(){}", "void main(){}"),
            (huge.as_str(), huge.as_str()),
        ] {
            let err = GlShader::new(&ctx, vert, frag).unwrap_err();
            assert_eq!(err, GlShaderCreateError::NoShaderCompiler);
        }
    }

    #[test]
    fn shader_error_types_format_without_panicking_on_unicode_and_extremes() {
        let vert = VertexShaderCompileError {
            error_id: i32::MIN,
            info_log: AzString::from("\u{1F600} log".to_string()),
        };
        let frag = FragmentShaderCompileError {
            error_id: i32::MAX,
            info_log: AzString::from(String::new()),
        };
        let link = GlShaderLinkError {
            error_id: -1,
            info_log: AzString::from("multi\nline\0log".to_string()),
        };

        assert!(alloc::format!("{vert}").contains("-2147483648"));
        assert!(alloc::format!("{vert}").contains("\u{1F600}"));
        assert!(alloc::format!("{frag}").contains("2147483647"));

        let compile = GlShaderCompileError::Vertex(vert);
        assert!(alloc::format!("{compile}").contains("Failed to compile vertex shader"));
        // Debug delegates to Display -- must agree, and must not recurse.
        assert_eq!(alloc::format!("{compile:?}"), alloc::format!("{compile}"));

        let frag_err = GlShaderCompileError::Fragment(frag);
        assert!(alloc::format!("{frag_err}").contains("Failed to compile fragment shader"));

        let create = GlShaderCreateError::Link(link);
        assert!(alloc::format!("{create}").contains("Shader linking error"));
        assert_eq!(alloc::format!("{create:?}"), alloc::format!("{create}"));
        assert!(alloc::format!("{}", GlShaderCreateError::NoShaderCompiler)
            .contains("doesn't include a shader compiler"));
    }

    // =====================================================================
    // Texture -- getters, refcounting, and the degenerate-driver paths
    // =====================================================================

    #[test]
    fn texture_descriptor_mirrors_the_texture_including_extreme_sizes() {
        let tex = test_texture(42, 640, 480);
        let d = tex.get_descriptor();
        assert_eq!(d.width, 640);
        assert_eq!(d.height, 480);
        assert_eq!(d.format, RawImageFormat::RGBA8);
        assert_eq!(d.offset, 0);
        assert!(!d.flags.is_opaque);
        assert!(!d.flags.allow_mipmaps, "textures map 1:1, never mipmapped");

        // Zero-size and u32::MAX-size must not panic or wrap in the descriptor.
        let zero = test_texture(1, 0, 0);
        assert_eq!(zero.get_descriptor().width, 0);
        assert_eq!(zero.get_descriptor().height, 0);

        let huge = test_texture(1, u32::MAX, u32::MAX);
        assert_eq!(huge.get_descriptor().width, u32::MAX as usize);
        assert_eq!(huge.get_descriptor().height, u32::MAX as usize);

        // is_opaque must be carried through from the flags, not hardcoded.
        let opaque = Texture::create(
            7,
            TextureFlags {
                is_opaque: true,
                is_video_texture: true,
            },
            PhysicalSizeU32 {
                width: 2,
                height: 2,
            },
            ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            },
            null_ctx(),
            RawImageFormat::BGRA8,
        );
        assert!(opaque.get_descriptor().flags.is_opaque);
        assert_eq!(opaque.get_descriptor().format, RawImageFormat::BGRA8);
    }

    #[test]
    fn texture_clone_and_drop_share_one_refcount_without_double_freeing() {
        // Texture is refcounted through a raw Box<AtomicUsize>; a miscount here
        // is a double-free (or a leaked GL texture). Exercise fan-out + drop.
        let tex = test_texture(9, 4, 4);
        let clones: Vec<Texture> = (0..16).map(|_| tex.clone()).collect();
        for c in &clones {
            assert_eq!(c.texture_id, 9);
            assert_eq!(c.size, tex.size);
            assert_eq!(c.format, tex.format);
            assert!(c.run_destructor);
        }
        // Equality/Hash are by texture id.
        assert_eq!(clones[0], tex);
        assert_eq!(clones[0], clones[1]);
        assert_ne!(tex, test_texture(10, 4, 4));

        drop(clones); // 16 drops...
        drop(tex); // ...then the last one actually frees.
    }

    #[test]
    fn texture_display_and_debug_render_id_and_size() {
        let tex = test_texture(3, 16, 32);
        assert_eq!(alloc::format!("{tex}"), "Texture { id: 3, 16x32 }");
        // Debug delegates to Display.
        assert_eq!(alloc::format!("{tex:?}"), alloc::format!("{tex}"));
    }

    #[test]
    fn texture_paint_stroke_is_a_noop_when_gl_is_unusable() {
        // Documented: "No-op if the GL context is unusable". With no brush shader
        // (program id 0) this must bail out before touching GL -- including for
        // NaN / infinite / negative radii and coordinates, which must not produce
        // a huge dab loop. (`!(radius > 0.0)` is written that way precisely so it
        // also rejects NaN.)
        let mut tex = test_texture(5, 8, 8);
        assert_eq!(tex.gl_context.get_brush_shader(), 0);

        for radius in [1.0, 0.0, -1.0, f32::NAN, f32::INFINITY, f32::MAX] {
            let mut brush = Brush::new(
                ColorU {
                    r: 255,
                    g: 0,
                    b: 0,
                    a: 255,
                },
                radius,
            );
            brush.hardness = f32::NAN;
            brush.flow = f32::INFINITY;
            brush.spacing = 0.0; // would be a divide-by-zero step without the .max(0.01)
            tex.paint_stroke(f32::NAN, f32::NEG_INFINITY, f32::MAX, -0.0, brush);
            tex.paint_dot(f32::NAN, f32::NAN, brush);
        }

        // A zero-sized texture is the other early-out (tw/th <= 0).
        let mut zero = test_texture(6, 0, 0);
        zero.paint_dot(
            0.0,
            0.0,
            Brush::new(
                ColorU {
                    r: 1,
                    g: 1,
                    b: 1,
                    a: 1,
                },
                4.0,
            ),
        );
    }

    #[test]
    fn texture_copy_to_raw_image_returns_a_null_image_on_every_degenerate_input() {
        // The `w <= 0 || h <= 0` guard is load-bearing: size is a u32 but is cast
        // to i32 for glReadPixels, so a width >= 2^31 goes NEGATIVE. Without the
        // guard that reaches gl-context-loader's
        //   `vec![0; width as usize * height as usize * bit_depth]`
        // which sign-extends the negative width to ~usize::MAX -> overflow panic
        // (debug) / exabyte allocation (release). Prove the guard holds.
        let is_null_image = |img: &RawImage| img.width == 0 && img.height == 0;

        // texture id 0 -> null image
        assert!(is_null_image(&test_texture(0, 16, 16).copy_to_raw_image()));
        // zero size -> null image
        assert!(is_null_image(&test_texture(1, 0, 0).copy_to_raw_image()));
        assert!(is_null_image(&test_texture(1, 16, 0).copy_to_raw_image()));
        assert!(is_null_image(&test_texture(1, 0, 16).copy_to_raw_image()));

        // u32::MAX / 2^31 both cast to a NEGATIVE i32 and MUST be caught by the guard.
        assert!(is_null_image(
            &test_texture(1, u32::MAX, u32::MAX).copy_to_raw_image()
        ));
        assert!(is_null_image(
            &test_texture(1, 1 << 31, 1 << 31).copy_to_raw_image()
        ));

        // Valid id + valid size, but the driver hands back no framebuffer
        // (`fbo == 0`) -> still a clean null image, no unwrap panic.
        assert!(is_null_image(&test_texture(1, 4, 4).copy_to_raw_image()));
    }

    #[test]
    #[should_panic(expected = "called `Option::unwrap()` on a `None` value")]
    fn texture_clear_panics_when_the_driver_allocates_no_framebuffer() {
        // DOCUMENTED panic: "Panics if no framebuffer/depthbuffer was allocated
        // (the GL object lists are empty)." A driver that generates nothing makes
        // gen_framebuffers(1) return an EMPTY list, so `.get(0).unwrap()` panics.
        // Pinned as documented behavior -- note that the sibling paths
        // (paint_stroke / copy_to_raw_image) degrade gracefully instead.
        test_texture(1, 4, 4).clear();
    }

    // =====================================================================
    // VertexArrayObject / VertexBuffer
    // =====================================================================

    #[test]
    fn vertex_array_object_clone_and_drop_share_one_refcount() {
        let layout = VertexLayout {
            fields: VertexAttributeVec::from_const_slice(&[]),
        };
        let vao = VertexArrayObject::new(layout, 77, null_ctx());
        assert_eq!(vao.vao_id, 77);
        assert!(vao.run_destructor);

        let clones: Vec<VertexArrayObject> = (0..8).map(|_| vao.clone()).collect();
        assert!(clones.iter().all(|c| c.vao_id == 77));
        assert_eq!(clones[0], vao);
        drop(clones);
        drop(vao);
    }

    #[test]
    fn vertex_buffer_new_raw_keeps_its_fields_and_refcounts_its_clones() {
        let vao = VertexArrayObject::new(
            VertexLayout {
                fields: VertexAttributeVec::from_const_slice(&[]),
            },
            1,
            null_ctx(),
        );
        let vb = VertexBuffer::new_raw(11, 300, vao, 22, 40, IndexBufferFormat::TriangleStrip);

        assert_eq!(vb.vertex_buffer_id, 11);
        assert_eq!(vb.vertex_buffer_len, 300);
        assert_eq!(vb.index_buffer_id, 22);
        assert_eq!(vb.index_buffer_len, 40);
        assert_eq!(vb.index_buffer_format, IndexBufferFormat::TriangleStrip);
        assert_eq!(
            alloc::format!("{vb}"),
            "VertexBuffer { buffer: 11 (length: 300) }})"
        );

        // Eq/Hash are by vertex_buffer_id.
        let clones: Vec<VertexBuffer> = (0..8).map(|_| vb.clone()).collect();
        assert_eq!(clones[0], vb);
        drop(clones);
        drop(vb);

        // A zero-length buffer is degenerate but legal.
        let empty_vao = VertexArrayObject::new(
            VertexLayout {
                fields: VertexAttributeVec::from_const_slice(&[]),
            },
            0,
            null_ctx(),
        );
        let empty = VertexBuffer::new_raw(0, 0, empty_vao, 0, 0, IndexBufferFormat::Points);
        assert_eq!(empty.vertex_buffer_len, 0);
    }

    #[allow(dead_code)] // the field only exists to give the vertex a realistic size/layout
    struct TestVertex {
        _xy: [f32; 2],
    }

    impl VertexLayoutDescription for TestVertex {
        fn get_description() -> VertexLayout {
            VertexLayout {
                fields: vec![VertexAttribute {
                    va_name: AzString::from_const_str("vAttrXY"),
                    layout_location: OptionUsize::None,
                    attribute_type: VertexAttributeType::Float,
                    item_count: 2,
                }]
                .into(),
            }
        }
    }

    #[test]
    #[should_panic(expected = "called `Option::unwrap()` on a `None` value")]
    fn vertex_buffer_new_panics_when_the_driver_allocates_no_vao() {
        // DOCUMENTED panic: "Panics if the GL driver failed to create the
        // vertex-array/buffer objects (the returned id lists are empty)."
        let verts = [TestVertex { _xy: [0.0, 0.0] }];
        let _ = VertexBuffer::new(
            null_ctx(),
            0,
            &verts[..],
            &[0u32],
            IndexBufferFormat::Triangles,
        );
    }

    // =====================================================================
    // The process-global GL texture table.
    //
    // ACTIVE_GL_TEXTURES is a `static mut` with no lock (GL is single-threaded
    // by design), and cargo test runs #[test]s on parallel threads. So ALL of
    // its coverage lives in this ONE test -- splitting it up would race the
    // table against itself. No other test in this crate touches it.
    // =====================================================================

    #[test]
    fn gl_texture_cache_insert_lookup_and_eviction_lifecycle() {
        let doc = DocumentId {
            namespace_id: crate::resources::IdNamespace(7),
            id: 1,
        };
        let other_doc = DocumentId {
            namespace_id: crate::resources::IdNamespace(7),
            id: 2,
        };

        // --- Empty / uninitialized table: every accessor must degrade, not panic.
        gl_textures_clear_opengl_cache();
        let stale = ExternalImageId { inner: u64::MAX };
        assert!(get_opengl_texture(&stale).is_none());
        assert!(
            remove_single_texture_from_active_gl_textures(&doc, &Epoch::from(0), &stale).is_none()
        );
        gl_textures_remove_epochs_from_pipeline(&doc, Epoch::from(0));
        gl_textures_remove_active_pipeline(&doc);
        gl_textures_clear_opengl_cache(); // idempotent

        // --- Insert into an UNINITIALIZED table.
        // NOTE: the doc comment on insert_into_active_gl_textures claims it
        // "Panics if the global active-GL-texture table has not been
        // initialized" -- it does not; it initializes the table itself. The doc
        // is stale (see the report). Pin the real behavior.
        let id5 = insert_into_active_gl_textures(doc, Epoch::from(5), test_texture(11, 4, 8));
        let id7 = insert_into_active_gl_textures(doc, Epoch::from(7), test_texture(22, 16, 32));
        assert_ne!(id5, id7, "each insert must mint a unique ExternalImageId");

        // --- Lookup: id -> (texture id, (w, h) as f32).
        assert_eq!(get_opengl_texture(&id5), Some((11, (4.0, 8.0))));
        assert_eq!(get_opengl_texture(&id7), Some((22, (16.0, 32.0))));
        assert!(get_opengl_texture(&stale).is_none());

        // A u32::MAX-sized texture goes through a lossy u32 -> f32 cast:
        // u32::MAX (2^32 - 1) has no exact f32, so it rounds UP to 2^32.
        let id_huge =
            insert_into_active_gl_textures(doc, Epoch::from(5), test_texture(33, u32::MAX, 1));
        assert_eq!(get_opengl_texture(&id_huge), Some((33, (4_294_967_296.0, 1.0))));

        // --- Epoch eviction is STRICTLY older-than: epoch 5 goes, epoch 7 stays.
        gl_textures_remove_epochs_from_pipeline(&doc, Epoch::from(7));
        assert!(get_opengl_texture(&id5).is_none(), "epoch 5 < 7 must be evicted");
        assert!(get_opengl_texture(&id_huge).is_none(), "epoch 5 < 7 must be evicted");
        assert_eq!(
            get_opengl_texture(&id7),
            Some((22, (16.0, 32.0))),
            "epoch 7 is NOT < 7, so it must survive"
        );

        // Evicting for an unknown document must be a no-op, not a panic.
        gl_textures_remove_epochs_from_pipeline(&other_doc, Epoch::from(u32::MAX));
        assert_eq!(get_opengl_texture(&id7), Some((22, (16.0, 32.0))));

        // --- remove_single: Some(()) when the (doc, epoch) path exists...
        assert_eq!(
            remove_single_texture_from_active_gl_textures(&doc, &Epoch::from(7), &id7),
            Some(())
        );
        assert!(get_opengl_texture(&id7).is_none());
        // ...including a second removal of an already-gone image (the path is
        // still there, so it reports Some(()) rather than None)...
        assert_eq!(
            remove_single_texture_from_active_gl_textures(&doc, &Epoch::from(7), &id7),
            Some(())
        );
        // ...but None when the document or the epoch is unknown.
        assert!(
            remove_single_texture_from_active_gl_textures(&other_doc, &Epoch::from(7), &id7)
                .is_none()
        );
        assert!(remove_single_texture_from_active_gl_textures(
            &doc,
            &Epoch::from(u32::MAX),
            &id7
        )
        .is_none());

        // --- remove_active_pipeline drops the whole document.
        let id_a = insert_into_active_gl_textures(doc, Epoch::from(1), test_texture(44, 2, 2));
        let id_b = insert_into_active_gl_textures(other_doc, Epoch::from(1), test_texture(55, 2, 2));
        assert!(get_opengl_texture(&id_a).is_some());
        gl_textures_remove_active_pipeline(&doc);
        assert!(get_opengl_texture(&id_a).is_none(), "doc was removed");
        assert!(
            get_opengl_texture(&id_b).is_some(),
            "other_doc must be untouched"
        );

        // --- clear drops everything.
        gl_textures_clear_opengl_cache();
        assert!(get_opengl_texture(&id_b).is_none());

        // Leave the table clean for anything that runs after us.
        gl_textures_clear_opengl_cache();
    }
}
