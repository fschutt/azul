
use crate::common::*;
use crate::common_types::*;
use crate::traits::{FragmentShader, Program, VertexShader};
use glam::{ivec2, ivec4, mat4, vec2, vec3, vec4, Mat4, Vec2, Vec3, Vec4};
use std::ffi::CStr;
use std::mem;
use super::strcmp;

//
// C++ Struct Definitions
//

#[derive(Clone, Copy, Debug, Default)]
struct ClipMaskInstanceCommon {
    sub_rect: RectWithEndpoint,
    task_origin: vec2,
    screen_origin: vec2,
    device_pixel_scale: f32,
    clip_transform_id: i32,
    prim_transform_id: i32,
}

#[derive(Clone, Copy, Debug, Default)]
struct ClipMaskInstanceRect {
    base: ClipMaskInstanceCommon,
    local_pos: vec2,
}

#[derive(Clone, Copy, Debug, Default)]
struct ClipRect {
    rect: RectWithEndpoint,
    mode: f32,
}

#[derive(Clone, Copy, Debug, Default)]
struct ClipCorner {
    rect: RectWithEndpoint,
    outer_inner_radius: vec4,
}

#[derive(Clone, Copy, Debug, Default)]
struct ClipData {
    rect: ClipRect,
    top_left: ClipCorner,
    top_right: ClipCorner,
    bottom_left: ClipCorner,
    bottom_right: ClipCorner,
}

//
// Common Struct
//

#[derive(Clone, Debug, Default)]
struct CsClipRectangleFastPathCommon {
    attrib_locations: AttribLocations,
    u_transform: mat4,
    // Varyings (flat)
    v_transform_bounds: vec4,
    v_clip_params: vec3,
    v_clip_mode: vec2,
}

#[derive(Clone, Debug, Default)]
struct AttribLocations {
    a_position: usize,
    a_clip_device_area: usize,
    a_clip_origins: usize,
    a_device_pixel_scale: usize,
    a_transform_ids: usize,
    a_clip_local_pos: usize,
    a_clip_local_rect: usize,
    a_clip_mode: usize,
    a_clip_rect_tl: usize,
    a_clip_radii_tl: usize,
    a_clip_rect_tr: usize,
    a_clip_radii_tr: usize,
    a_clip_rect_bl: usize,
    a_clip_radii_bl: usize,
    a_clip_rect_br: usize,
    a_clip_radii_br: usize,
}

impl AttribLocations {
    fn bind_loc(&mut self, name: &CStr, index: i32) {
        let index = index as usize;
        if strcmp(name, "aPosition") { self.a_position = index; }
        else if strcmp(name, "aClipDeviceArea") { self.a_clip_device_area = index; }
        else if strcmp(name, "aClipOrigins") { self.a_clip_origins = index; }
        else if strcmp(name, "aDevicePixelScale") { self.a_device_pixel_scale = index; }
        else if strcmp(name, "aTransformIds") { self.a_transform_ids = index; }
        else if strcmp(name, "aClipLocalPos") { self.a_clip_local_pos = index; }
        else if strcmp(name, "aClipLocalRect") { self.a_clip_local_rect = index; }
        else if strcmp(name, "aClipMode") { self.a_clip_mode = index; }
        else if strcmp(name, "aClipRect_TL") { self.a_clip_rect_tl = index; }
        else if strcmp(name, "aClipRadii_TL") { self.a_clip_radii_tl = index; }
        else if strcmp(name, "aClipRect_TR") { self.a_clip_rect_tr = index; }
        else if strcmp(name, "aClipRadii_TR") { self.a_clip_radii_tr = index; }
        else if strcmp(name, "aClipRect_BL") { self.a_clip_rect_bl = index; }
        else if strcmp(name, "aClipRadii_BL") { self.a_clip_radii_bl = index; }
        else if strcmp(name, "aClipRect_BR") { self.a_clip_rect_br = index; }
        else if strcmp(name, "aClipRadii_BR") { self.a_clip_radii_br = index; }
    }

    fn get_loc(&self, name: &CStr) -> i32 {
        let loc = if strcmp(name, "aPosition") { self.a_position }
        else if strcmp(name, "aClipDeviceArea") { self.a_clip_device_area }
        else if strcmp(name, "aClipOrigins") { self.a_clip_origins }
        else if strcmp(name, "aDevicePixelScale") { self.a_device_pixel_scale }
        else if strcmp(name, "aTransformIds") { self.a_transform_ids }
        else if strcmp(name, "aClipLocalPos") { self.a_clip_local_pos }
        else if strcmp(name, "aClipLocalRect") { self.a_clip_local_rect }
        else if strcmp(name, "aClipMode") { self.a_clip_mode }
        else if strcmp(name, "aClipRect_TL") { self.a_clip_rect_tl }
        else if strcmp(name, "aClipRadii_TL") { self.a_clip_radii_tl }
        else if strcmp(name, "aClipRect_TR") { self.a_clip_rect_tr }
        else if strcmp(name, "aClipRadii_TR") { self.a_clip_radii_tr }
        else if strcmp(name, "aClipRect_BL") { self.a_clip_rect_bl }
        else if strcmp(name, "aClipRadii_BL") { self.a_clip_radii_bl }
        else if strcmp(name, "aClipRect_BR") { self.a_clip_rect_br }
        else if strcmp(name, "aClipRadii_BR") { self.a_clip_radii_br }
        else { NULL_ATTRIB };
        
        if loc != NULL_ATTRIB { loc as i32 } else { -1 }
    }
}

//
// Vertex Shader
//

#[derive(Clone, Debug, Default)]
struct CsClipRectangleFastPathVert {
    common: CsClipRectangleFastPathCommon,
    // Inputs
    a_position: vec2,
    a_clip_device_area: vec4,
    a_clip_origins: vec4,
    a_device_pixel_scale: f32,
    a_transform_ids: ivec2,
    a_clip_local_pos: vec2,
    a_clip_local_rect: vec4,
    a_clip_mode: f32,
    // Per-corner attributes (even if only TL is used, they are loaded)
    a_clip_rect_tl: vec4, a_clip_radii_tl: vec4,
    a_clip_rect_tr: vec4, a_clip_radii_tr: vec4,
    a_clip_rect_bl: vec4, a_clip_radii_bl: vec4,
    a_clip_rect_br: vec4, a_clip_radii_br: vec4,
    // Outputs
    v_local_pos: vec4,
    gl_position: vec4,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct InterpOutputs {
    v_local_pos: vec4,
}

impl CsClipRectangleFastPathVert {
    #[inline(always)]
    fn main(&mut self, context: &ShaderContext) {
        let cmi = self.fetch_clip_item();
        let clip_transform = context.fetch_transform(cmi.base.clip_transform_id);
        let prim_transform = context.fetch_transform(cmi.base.prim_transform_id);
        let clip = self.fetch_clip();
        
        let mut local_rect = clip.rect.rect;
        let diff = cmi.local_pos - local_rect.p0;
        local_rect.p0 = cmi.local_pos;
        local_rect.p1 += diff;
        
        let vi = self.write_clip_tile_vertex(
            local_rect,
            &prim_transform,
            &clip_transform,
            cmi.base.sub_rect,
            cmi.base.task_origin,
            cmi.base.screen_origin,
            cmi.base.device_pixel_scale,
        );
        
        self.common.v_clip_mode.x = clip.rect.mode;
        self.v_local_pos = vi.local_pos;
        
        let half_size = 0.5 * (local_rect.p1 - local_rect.p0);
        let radius = clip.top_left.outer_inner_radius.x;
        
        self.v_local_pos.xy() -= (half_size + cmi.local_pos) * self.v_local_pos.w;
        self.common.v_clip_params = vec3(half_size.x - radius, half_size.y - radius, radius);
    }
    
    fn fetch_clip_item_common(&self) -> ClipMaskInstanceCommon {
        ClipMaskInstanceCommon {
            sub_rect: RectWithEndpoint { p0: self.a_clip_device_area.xy(), p1: self.a_clip_device_area.zw() },
            task_origin: self.a_clip_origins.xy(),
            screen_origin: self.a_clip_origins.zw(),
            device_pixel_scale: self.a_device_pixel_scale,
            clip_transform_id: self.a_transform_ids.x,
            prim_transform_id: self.a_transform_ids.y,
        }
    }
    
    fn fetch_clip_item(&self) -> ClipMaskInstanceRect {
        ClipMaskInstanceRect {
            base: self.fetch_clip_item_common(),
            local_pos: self.a_clip_local_pos,
        }
    }
    
    fn fetch_clip(&self) -> ClipData {
        ClipData {
            rect: ClipRect { rect: RectWithEndpoint { p0: self.a_clip_local_rect.xy(), p1: self.a_clip_local_rect.zw() }, mode: self.a_clip_mode },
            top_left: ClipCorner { rect: RectWithEndpoint { p0: self.a_clip_rect_tl.xy(), p1: self.a_clip_rect_tl.zw() }, outer_inner_radius: self.a_clip_radii_tl },
            top_right: ClipCorner { rect: RectWithEndpoint { p0: self.a_clip_rect_tr.xy(), p1: self.a_clip_rect_tr.zw() }, outer_inner_radius: self.a_clip_radii_tr },
            bottom_left: ClipCorner { rect: RectWithEndpoint { p0: self.a_clip_rect_bl.xy(), p1: self.a_clip_rect_bl.zw() }, outer_inner_radius: self.a_clip_radii_bl },
            bottom_right: ClipCorner { rect: RectWithEndpoint { p0: self.a_clip_rect_br.xy(), p1: self.a_clip_rect_br.zw() }, outer_inner_radius: self.a_clip_radii_br },
        }
    }

    fn write_clip_tile_vertex(&mut self, local_clip_rect: RectWithEndpoint, prim_transform: &Transform, clip_transform: &Transform, sub_rect: RectWithEndpoint, task_origin: vec2, screen_origin: vec2, device_pixel_scale: f32) -> ClipVertexInfo {
        let device_pos = screen_origin + sub_rect.p0.lerp(sub_rect.p1, self.a_position);
        let world_pos = device_pos / device_pixel_scale;
        let mut pos = prim_transform.m * vec4(world_pos.x, world_pos.y, 0.0, 1.0);
        pos /= pos.w;

        let p = self.get_node_pos(pos.xy(), clip_transform);
        let local_pos = p * pos.w;

        let vertex_pos = vec4(
            task_origin.x + sub_rect.p0.lerp(sub_rect.p1, self.a_position).x,
            task_origin.y + sub_rect.p0.lerp(sub_rect.p1, self.a_position).y,
            0.0,
            1.0,
        );
        self.gl_position = self.common.u_transform * vertex_pos;
        
        self.common.v_transform_bounds = local_clip_rect.p0.extend(local_clip_rect.p1);
        
        ClipVertexInfo { local_pos, clipped_local_rect: local_clip_rect }
    }

    fn get_node_pos(&self, pos: vec2, transform: &Transform) -> vec4 {
        let ah = transform.m * vec4(0.0, 0.0, 0.0, 1.0);
        let a = (ah.xyz()) / ah.w;
        let n = Mat3::from_mat4(transform.inv_m).transpose() * vec3(0.0, 0.0, 1.0);
        self.untransform(pos, n, a, &transform.inv_m)
    }

    fn untransform(&self, reference: vec2, n: vec3, a: vec3, inv_transform: &mat4) -> vec4 {
        let p = vec3(reference.x, reference.y, -10000.0);
        let d = vec3(0.0, 0.0, 1.0);
        let mut t = 0.0;
        if self.ray_plane(n, a, p, d, &mut t) {
            let z = p.z + d.z * t;
            return *inv_transform * vec4(reference.x, reference.y, z, 1.0);
        }
        // This case should ideally not be hit in normal operation.
        *inv_transform * vec4(reference.x, reference.y, 0.0, 1.0)
    }
    
    fn ray_plane(&self, normal: vec3, pt: vec3, ray_origin: vec3, ray_dir: vec3, t: &mut f32) -> bool {
        let denom = normal.dot(ray_dir);
        if denom.abs() > 0.000001 {
            let d = pt - ray_origin;
            *t = d.dot(normal) / denom;
            return *t >= 0.0;
        }
        false
    }
}

impl VertexShader for CsClipRectangleFastPathVert {
    fn init_batch(&mut self, _context: &mut ShaderContext) {}
    
    fn load_attribs(&mut self, attribs: &[&VertexAttrib], start: u32, instance: i32, _count: i32) {
        unsafe {
            let locs = &self.common.attrib_locations;
            
            let a_pos_attrib = &*attribs[locs.a_position];
            let pos_ptr = (a_pos_attrib.data as *const u8).add(a_pos_attrib.stride * start as usize) as *const Vec2;
            self.a_position = *pos_ptr;
            
            macro_rules! load_flat_attrib {
                ($field:ident, $loc:expr) => {
                    let attrib = &*attribs[$loc];
                    let ptr = (attrib.data as *const u8).add(attrib.stride * instance as usize) as *const _;
                    self.$field = *ptr;
                };
            }
            
            load_flat_attrib!(a_clip_device_area, locs.a_clip_device_area);
            load_flat_attrib!(a_clip_origins, locs.a_clip_origins);
            load_flat_attrib!(a_device_pixel_scale, locs.a_device_pixel_scale);
            load_flat_attrib!(a_transform_ids, locs.a_transform_ids);
            load_flat_attrib!(a_clip_local_pos, locs.a_clip_local_pos);
            load_flat_attrib!(a_clip_local_rect, locs.a_clip_local_rect);
            load_flat_attrib!(a_clip_mode, locs.a_clip_mode);
            load_flat_attrib!(a_clip_rect_tl, locs.a_clip_rect_tl);
            load_flat_attrib!(a_clip_radii_tl, locs.a_clip_radii_tl);
            load_flat_attrib!(a_clip_rect_tr, locs.a_clip_rect_tr);
            load_flat_attrib!(a_clip_radii_tr, locs.a_clip_radii_tr);
            load_flat_attrib!(a_clip_rect_bl, locs.a_clip_rect_bl);
            load_flat_attrib!(a_clip_radii_bl, locs.a_clip_radii_bl);
            load_flat_attrib!(a_clip_rect_br, locs.a_clip_rect_br);
            load_flat_attrib!(a_clip_radii_br, locs.a_clip_radii_br);
        }
    }

    fn run_primitive(&mut self, context: &ShaderContext, interps: *mut u8, interp_stride: usize) {
        self.main(context);
        unsafe {
            let mut dest_ptr = interps as *mut InterpOutputs;
            for _ in 0..4 {
                (*dest_ptr).v_local_pos = self.v_local_pos;
                dest_ptr = (dest_ptr as *mut u8).add(interp_stride) as *mut InterpOutputs;
            }
        }
    }
    
    fn set_uniform_1i(&mut self, _index: i32, _value: i32) {}
    fn set_uniform_4fv(&mut self, _index: i32, _value: &[f32; 4]) {}
    fn set_uniform_matrix4fv(&mut self, index: i32, value: &[f32; 16]) {
        if index == 4 { // uTransform
            self.common.u_transform = Mat4::from_cols_array(value);
        }
    }
}

//
// Fragment Shader
//

#[derive(Clone, Debug, Default)]
struct CsClipRectangleFastPathFrag {
    vert: CsClipRectangleFastPathVert,
    // Varying inputs
    v_local_pos: vec4,
    interp_step: InterpOutputs,
    interp_perspective: InterpOutputs,
}

impl CsClipRectangleFastPathFrag {
    #[inline(always)]
    fn main(&self) -> vec4 {
        let local_pos = self.v_local_pos.xy() / self.v_local_pos.w;
        let aa_range = 1.0; // Placeholder for fwidth
        
        let dist = self.sd_rounded_box(local_pos, self.vert.common.v_clip_params.xy(), self.vert.common.v_clip_params.z);
        let alpha = self.distance_aa(aa_range, dist);
        
        let final_alpha = if self.vert.common.v_clip_mode.x == 0.0 { alpha } else { 1.0 - alpha };
        
        let final_final_alpha = if self.v_local_pos.w > 0.0 { final_alpha } else { 0.0 };
        
        vec4(final_final_alpha, 0.0, 0.0, 1.0)
    }

    fn sd_box(&self, pos: vec2, box_size: vec2) -> f32 {
        let d = pos.abs() - box_size;
        d.max(vec2::ZERO).length() + d.x.max(d.y).min(0.0)
    }

    fn sd_rounded_box(&self, pos: vec2, box_size: vec2, radius: f32) -> f32 {
        self.sd_box(pos, box_size) - radius
    }
    
    fn distance_aa(&self, aa_range: f32, signed_distance: f32) -> f32 {
        let dist = signed_distance * aa_range;
        (0.5 - dist).clamp(0.0, 1.0)
    }
}

impl FragmentShader for CsClipRectangleFastPathFrag {
    fn read_interp_inputs(&mut self, init: *const u8, step: *const u8) {
        unsafe {
            let init = &*(init as *const InterpOutputs);
            let step = &*(step as *const InterpOutputs);
            self.v_local_pos = init.v_local_pos;
            self.interp_step.v_local_pos = step.v_local_pos * 4.0;
        }
    }
    
    fn read_perspective_inputs(&mut self, init: *const u8, step: *const u8, w: f32) {
        unsafe {
            let init = &*(init as *const InterpOutputs);
            let step = &*(step as *const InterpOutputs);
            let inv_w = 1.0 / w;
            self.interp_perspective.v_local_pos = init.v_local_pos;
            self.v_local_pos = self.interp_perspective.v_local_pos * inv_w;
            self.interp_step.v_local_pos = step.v_local_pos * 4.0;
        }
    }

    fn run(&mut self, context: &mut ShaderContext) {
        let color = self.main();
        context.write_output(color);
        self.step_interp_inputs(4);
    }
    
    fn skip(&mut self, steps: i32) {
        self.step_interp_inputs(steps);
    }

    fn run_perspective(&mut self, context: &mut ShaderContext, next_w: &[f32; 4]) {
        let color = self.main();
        context.write_output(color);
        self.step_perspective_inputs(4, next_w);
    }

    fn skip_perspective(&mut self, steps: i32, next_w: &[f32; 4]) {
        self.step_perspective_inputs(steps, next_w);
    }
    
    fn draw_span_r8(&mut self, _context: &mut ShaderContext) -> i32 {
        // The swgl_drawSpanR8 is highly specialized and would require a direct port
        // of the analytical intersection logic, which is complex and outside the
        // scope of a simple GLSL -> Rust translation. The generic path will be used.
        0
    }
}

impl CsClipRectangleFastPathFrag {
    fn step_interp_inputs(&mut self, steps: i32) {
        let chunks = steps as f32 * 0.25;
        self.v_local_pos += self.interp_step.v_local_pos * chunks;
    }
    
    fn step_perspective_inputs(&mut self, steps: i32, next_w: &[f32; 4]) {
        let chunks = steps as f32 * 0.25;
        let inv_w = 1.0 / next_w[0];
        self.interp_perspective.v_local_pos += self.interp_step.v_local_pos * chunks;
        self.v_local_pos = self.interp_perspective.v_local_pos * inv_w;
    }
}

//
// Program
//

#[derive(Clone, Debug, Default)]
pub struct CsClipRectangleFastPathProgram {
    frag: CsClipRectangleFastPathFrag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(CsClipRectangleFastPathProgram::default())
}

impl Program for CsClipRectangleFastPathProgram {
    fn get_vertex_shader(&mut self) -> &mut dyn VertexShader {
        &mut self.frag.vert
    }

    fn get_fragment_shader(&mut self) -> &mut dyn FragmentShader {
        &mut self.frag
    }
    
    fn get_uniform(&self, name: &CStr) -> i32 {
        if strcmp(name, "sGpuCache") { return 2; }
        if strcmp(name, "sRenderTasks") { return 1; }
        if strcmp(name, "sTransformPalette") { return 3; }
        if strcmp(name, "uTransform") { return 4; }
        -1
    }
    
    fn get_attrib(&self, name: &CStr) -> i32 {
        self.frag.vert.common.attrib_locations.get_loc(name)
    }

    fn bind_attrib(&mut self, name: &CStr, index: i32) {
        self.frag.vert.common.attrib_locations.bind_loc(name, index);
    }
    
    fn interpolants_size(&self) -> usize {
        mem::size_of::<InterpOutputs>()
    }
}
