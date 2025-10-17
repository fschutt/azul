use glam::{Vec2, Vec4, Mat4, IVec2, IVec4, BVec4};

// GLSL Type Aliases using glam
pub type vec2 = Vec2;
pub type vec4 = Vec4;
pub type mat4 = Mat4;
pub type ivec2 = IVec2;
pub type ivec4 = IVec4;
pub type bvec4 = BVec4;

// Sentinel for unbound attribute locations
pub const NULL_ATTRIB: usize = std::usize::MAX;

// Common structs translated from C++
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct RectWithEndpoint {
    pub p0: vec2,
    pub p1: vec2,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct PictureTask {
    pub task_rect: RectWithEndpoint,
    pub device_pixel_scale: f32,
    pub content_origin: vec2,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct ClipArea {
    pub task_rect: RectWithEndpoint,
    pub device_pixel_scale: f32,
    pub screen_origin: vec2,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Transform {
    pub m: mat4,
    pub inv_m: mat4,
    pub is_axis_aligned: bool,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Instance {
    pub prim_header_address: i32,
    pub clip_address: i32,
    pub segment_index: i32,
    pub flags: i32,
    pub resource_address: i32,
    pub brush_kind: i32,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct PrimitiveHeader {
    pub local_rect: RectWithEndpoint,
    pub local_clip_rect: RectWithEndpoint,
    pub z: f32,
    pub specific_prim_address: i32,
    pub transform_id: i32,
    pub picture_task_address: i32,
    pub user_data: ivec4,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct VertexInfo {
    pub local_pos: vec2,
    pub world_pos: vec4,
}

// Represents a handle to a vertex attribute buffer.
// The rasterizer will use this to get the actual data pointers.
#[derive(Clone, Copy, Debug)]
pub struct VertexAttrib;

// Sampler Traits (to be implemented by the rendering backend)

/// A standard 2D texture sampler that uses normalized coordinates [0.0, 1.0].
pub trait Sampler2D {
    fn texel_fetch(&self, pos: ivec2, lod: i32) -> vec4;
    fn texture(&self, uv: vec2) -> vec4;
}

/// A 2D integer texture sampler.
pub trait ISampler2D {
    fn texel_fetch(&self, pos: ivec2, lod: i32) -> ivec4;
}

/// A 2D texture sampler that uses non-normalized, integer coordinates.
pub trait Sampler2DRect {
    fn texture(&self, uv: vec2) -> vec4;
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct ImageBrushData {
    pub color: vec4,
    pub background_color: vec4,
    pub stretch_size: vec2,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct ImageSource {
    pub uv_rect: RectWithEndpoint,
    pub user_data: vec4,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct ImageSourceExtra {
    pub st_tl: vec4,
    pub st_tr: vec4,
    pub st_bl: vec4,
    pub st_br: vec4,
}
