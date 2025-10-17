// swgl_shaders/src/shaders/cs_clip_rectangle.rs

use crate::common::*;
use crate::common_types::*;
use crate::traits::{FragmentShader, Program, VertexShader};
use glam::{ivec2, mat4, vec2, vec3, vec4, Mat4, Vec2, Vec3, Vec4};
use std::ffi::CStr;
use std::mem;
use super::strcmp;

//
// Common Struct
//
#[derive(Clone, Debug, Default)]
struct CsClipRectangleCommon {
    attrib_locations: AttribLocations,
    u_transform: mat4,
    // Flat varyings
    v_transform_bounds: vec4,
    v_clip_center_radius_tl: vec4,
    v_clip_center_radius_tr: vec4,
    v_clip_center_radius_bl: vec4,
    v_clip_center_radius_br: vec4,
    v_clip_plane_tl: vec3,
    v_clip_plane_tr: vec3,
    v_clip_plane_bl: vec3,
    v_clip_plane_br: vec3,
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
        let u_index = index as usize;
        if strcmp(name, "aPosition") { self.a_position = u_index; }
        else if strcmp(name, "aClipDeviceArea") { self.a_clip_device_area = u_index; }
        else if strcmp(name, "aClipOrigins") { self.a_clip_origins = u_index; }
        else if strcmp(name, "aDevicePixelScale") { self.a_device_pixel_scale = u_index; }
        else if strcmp(name, "aTransformIds") { self.a_transform_ids = u_index; }
        else if strcmp(name, "aClipLocalPos") { self.a_clip_local_pos = u_index; }
        else if strcmp(name, "aClipLocalRect") { self.a_clip_local_rect = u_index; }
        else if strcmp(name, "aClipMode") { self.a_clip_mode = u_index; }
        else if strcmp(name, "aClipRect_TL") { self.a_clip_rect_tl = u_index; }
        else if strcmp(name, "aClipRadii_TL") { self.a_clip_radii_tl = u_index; }
        else if strcmp(name, "aClipRect_TR") { self.a_clip_rect_tr = u_index; }
        else if strcmp(name, "aClipRadii_TR") { self.a_clip_radii_tr = u_index; }
        else if strcmp(name, "aClipRect_BL") { self.a_clip_rect_bl = u_index; }
        else if strcmp(name, "aClipRadii_BL") { self.a_clip_radii_bl = u_index; }
        else if strcmp(name, "aClipRect_BR") { self.a_clip_rect_br = u_index; }
        else if strcmp(name, "aClipRadii_BR") { self.a_clip_radii_br = u_index; }
    }

    fn get_loc(&self, name: &CStr) -> i32 {
        if strcmp(name, "aPosition") { return if self.a_position != NULL_ATTRIB { self.a_position as i32 } else { -1 }; }
        if strcmp(name, "aClipDeviceArea") { return if self.a_clip_device_area != NULL_ATTRIB { self.a_clip_device_area as i32 } else { -1 }; }
        if strcmp(name, "aClipOrigins") { return if self.a_clip_origins != NULL_ATTRIB { self.a_clip_origins as i32 } else { -1 }; }
        if strcmp(name, "aDevicePixelScale") { return if self.a_device_pixel_scale != NULL_ATTRIB { self.a_device_pixel_scale as i32 } else { -1 }; }
        if strcmp(name, "aTransformIds") { return if self.a_transform_ids != NULL_ATTRIB { self.a_transform_ids as i32 } else { -1 }; }
        if strcmp(name, "aClipLocalPos") { return if self.a_clip_local_pos != NULL_ATTRIB { self.a_clip_local_pos as i32 } else { -1 }; }
        if strcmp(name, "aClipLocalRect") { return if self.a_clip_local_rect != NULL_ATTRIB { self.a_clip_local_rect as i32 } else { -1 }; }
        if strcmp(name, "aClipMode") { return if self.a_clip_mode != NULL_ATTRIB { self.a_clip_mode as i32 } else { -1 }; }
        if strcmp(name, "aClipRect_TL") { return if self.a_clip_rect_tl != NULL_ATTRIB { self.a_clip_rect_tl as i32 } else { -1 }; }
        if strcmp(name, "aClipRadii_TL") { return if self.a_clip_radii_tl != NULL_ATTRIB { self.a_clip_radii_tl as i32 } else { -1 }; }
        if strcmp(name, "aClipRect_TR") { return if self.a_clip_rect_tr != NULL_ATTRIB { self.a_clip_rect_tr as i32 } else { -1 }; }
        if strcmp(name, "aClipRadii_TR") { return if self.a_clip_radii_tr != NULL_ATTRIB { self.a_clip_radii_tr as i32 } else { -1 }; }
        if strcmp(name, "aClipRect_BL") { return if self.a_clip_rect_bl != NULL_ATTRIB { self.a_clip_rect_bl as i32 } else { -1 }; }
        if strcmp(name, "aClipRadii_BL") { return if self.a_clip_radii_bl != NULL_ATTRIB { self.a_clip_radii_bl as i32 } else { -1 }; }
        if strcmp(name, "aClipRect_BR") { return if self.a_clip_rect_br != NULL_ATTRIB { self.a_clip_rect_br as i32 } else { -1 }; }
        if strcmp(name, "aClipRadii_BR") { return if self.a_clip_radii_br != NULL_ATTRIB { self.a_clip_radii_br as i32 } else { -1 }; }
        -1
    }
}

//
// Helper Structs from C++
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
struct ClipRect {
    rect: RectWithEndpoint,
    mode: f32,
}

#[derive(Clone, Copy, Debug, Default)]
struct ClipCorner {
    _rect: RectWithEndpoint,
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

#[derive(Clone, Copy, Debug, Default)]
struct ClipVertexInfo {
    local_pos: vec4,
    clipped_local_rect: RectWithEndpoint,
}

//
// Vertex Shader
//

#[derive(Clone, Debug, Default)]
struct CsClipRectangleVert {
    common: CsClipRectangleCommon,
    // Inputs (all flat/per-instance except aPosition)
    a_position: vec2,
    a_clip_device_area: vec4,
    a_clip_origins: vec4,
    a_device_pixel_scale: f32,
    a_transform_ids: ivec2,
    a_clip_local_pos: vec2,
    a_clip_local_rect: vec4,
    a_clip_mode: f32,
    a_clip_rect_tl: vec4,
    a_clip_radii_tl: vec4,
    a_clip_rect_tr: vec4,
    a_clip_radii_tr: vec4,
    a_clip_rect_bl: vec4,
    a_clip_radii_bl: vec4,
    a_clip_rect_br: vec4,
    a_clip_radii_br: vec4,
    // Outputs
    v_local_pos: vec4,
    gl_position: vec4,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct InterpOutputs {
    v_local_pos: vec4,
}

impl CsClipRectangleVert {
    fn main(&mut self, context: &ShaderContext) {
        let cmi_base = self.fetch_clip_item_common();
        let clip_transform = context.fetch_transform(cmi_base.clip_transform_id);
        let prim_transform = context.fetch_transform(cmi_base.prim_transform_id);
        let clip = self.fetch_clip();

        let vi = self.write_clip_tile_vertex(
            clip.rect.rect,
            prim_transform,
            clip_transform,
            cmi_base.sub_rect,
            cmi_base.task_origin,
            cmi_base.screen_origin,
            cmi_base.device_pixel_scale,
        );

        self.common.v_clip_mode.x = clip.rect.mode;
        self.v_local_pos = vi.local_pos;

        let clip_rect = vi.clipped_local_rect;
        let r_tl = clip.top_left.outer_inner_radius.xy();
        let r_tr = clip.top_right.outer_inner_radius.xy();
        let r_br = clip.bottom_right.outer_inner_radius.xy();
        let r_bl = clip.bottom_left.outer_inner_radius.xy();

        self.common.v_clip_center_radius_tl = vec4(
            (clip_rect.p0 + r_tl).x,
            (clip_rect.p0 + r_tl).y,
            (self.inverse_radii_squared(r_tl)).x,
            (self.inverse_radii_squared(r_tl)).y,
        );
        self.common.v_clip_center_radius_tr = vec4(
            clip_rect.p1.x - r_tr.x,
            clip_rect.p0.y + r_tr.y,
            (self.inverse_radii_squared(r_tr)).x,
            (self.inverse_radii_squared(r_tr)).y,
        );
        self.common.v_clip_center_radius_br = vec4(
            (clip_rect.p1 - r_br).x,
            (clip_rect.p1 - r_br).y,
            (self.inverse_radii_squared(r_br)).x,
            (self.inverse_radii_squared(r_br)).y,
        );
        self.common.v_clip_center_radius_bl = vec4(
            clip_rect.p0.x + r_bl.x,
            clip_rect.p1.y - r_bl.y,
            (self.inverse_radii_squared(r_bl)).x,
            (self.inverse_radii_squared(r_bl)).y,
        );
        
        let n_tl = vec2(-r_tl.y, r_tl.x);
        let n_tr = vec2(r_tr.y, -r_tr.x);
        let n_br = vec2(r_br.y, r_br.x);
        let n_bl = vec2(-r_bl.y, r_bl.x);

        self.common.v_clip_plane_tl = vec3(n_tl.x, n_tl.y, n_tl.dot(vec2(clip_rect.p0.x, clip_rect.p0.y + r_tl.y)));
        self.common.v_clip_plane_tr = vec3(n_tr.x, n_tr.y, n_tr.dot(vec2(clip_rect.p1.x - r_tr.x, clip_rect.p0.y)));
        self.common.v_clip_plane_br = vec3(n_br.x, n_br.y, n_br.dot(vec2(clip_rect.p1.x, clip_rect.p1.y - r_br.y)));
        self.common.v_clip_plane_bl = vec3(n_bl.x, n_bl.y, n_bl.dot(vec2(clip_rect.p0.x + r_bl.x, clip_rect.p1.y)));
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

    fn fetch_clip(&self) -> ClipData {
        ClipData {
            rect: ClipRect { rect: RectWithEndpoint { p0: self.a_clip_local_rect.xy(), p1: self.a_clip_local_rect.zw() }, mode: self.a_clip_mode },
            top_left: ClipCorner { _rect: RectWithEndpoint { p0: self.a_clip_rect_tl.xy(), p1: self.a_clip_rect_tl.zw() }, outer_inner_radius: self.a_clip_radii_tl },
            top_right: ClipCorner { _rect: RectWithEndpoint { p0: self.a_clip_rect_tr.xy(), p1: self.a_clip_rect_tr.zw() }, outer_inner_radius: self.a_clip_radii_tr },
            bottom_left: ClipCorner { _rect: RectWithEndpoint { p0: self.a_clip_rect_bl.xy(), p1: self.a_clip_rect_bl.zw() }, outer_inner_radius: self.a_clip_radii_bl },
            bottom_right: ClipCorner { _rect: RectWithEndpoint { p0: self.a_clip_rect_br.xy(), p1: self.a_clip_rect_br.zw() }, outer_inner_radius: self.a_clip_radii_br },
        }
    }
    
    fn write_clip_tile_vertex(&mut self, local_clip_rect: RectWithEndpoint, prim_transform: Transform, clip_transform: Transform, sub_rect: RectWithEndpoint, task_origin: vec2, screen_origin: vec2, device_pixel_scale: f32) -> ClipVertexInfo {
        let device_pos = screen_origin + sub_rect.p0.lerp(sub_rect.p1, self.a_position);
        let world_pos = device_pos / device_pixel_scale;
        let mut pos = prim_transform.m * vec4(world_pos.x, world_pos.y, 0.0, 1.0);
        pos.xyz() /= pos.w;
        
        let p = self.get_node_pos(pos.xy(), clip_transform);
        let local_pos = p * pos.w;

        let vertex_pos = vec4(
            (task_origin + sub_rect.p0.lerp(sub_rect.p1, self.a_position)).x,
            (task_origin + sub_rect.p0.lerp(sub_rect.p1, self.a_position)).y,
            0.0, 1.0
        );
        self.gl_position = self.common.u_transform * vertex_pos;
        
        self.common.v_transform_bounds = vec4(local_clip_rect.p0.x, local_clip_rect.p0.y, local_clip_rect.p1.x, local_clip_rect.p1.y);
        
        ClipVertexInfo { local_pos, clipped_local_rect: local_clip_rect }
    }

    fn ray_plane(&self, normal: vec3, pt: vec3, ray_origin: Vec3, ray_dir: vec3, t: &mut f32) -> bool {
        let denom = normal.dot(ray_dir);
        if denom.abs() > 1e-6 {
            let d = pt - ray_origin;
            *t = d.dot(normal) / denom;
            return *t >= 0.0;
        }
        false
    }

    fn untransform(&self, reference: vec2, n: vec3, a: vec3, inv_transform: mat4) -> vec4 {
        let p = vec3(reference.x, reference.y, -10000.0);
        let d = vec3(0.0, 0.0, 1.0);
        let mut t = 0.0;
        self.ray_plane(n, a, p, d, &mut t);
        let z = p.z + (d.z * t);
        inv_transform * vec4(reference.x, reference.y, z, 1.0)
    }

    fn get_node_pos(&self, pos: vec2, transform: Transform) -> vec4 {
        let ah = transform.m * vec4::W;
        let a = ah.xyz() / ah.w;
        let n = Mat3::from_mat4(transform.inv_m).transpose() * vec3(0.0, 0.0, 1.0);
        self.untransform(pos, n, a, transform.inv_m)
    }

    fn inverse_radii_squared(&self, radii: vec2) -> vec2 {
        vec2::ONE / (radii * radii).max(vec2::splat(1e-6))
    }
}

impl VertexShader for CsClipRectangleVert {
    fn init_batch(&mut self) {}

    fn load_attribs(&mut self, attribs: &[&VertexAttrib], start: u32, instance: i32, _count: i32) {
        unsafe {
            self.a_position = *( (*attribs[self.common.attrib_locations.a_position]).data as *const Vec2 ).add(start as usize);
            self.a_clip_device_area = *( (*attribs[self.common.attrib_locations.a_clip_device_area]).data as *const Vec4 ).add(instance as usize);
            self.a_clip_origins = *( (*attribs[self.common.attrib_locations.a_clip_origins]).data as *const Vec4 ).add(instance as usize);
            self.a_device_pixel_scale = *( (*attribs[self.common.attrib_locations.a_device_pixel_scale]).data as *const f32 ).add(instance as usize);
            self.a_transform_ids = *( (*attribs[self.common.attrib_locations.a_transform_ids]).data as *const ivec2 ).add(instance as usize);
            self.a_clip_local_pos = *( (*attribs[self.common.attrib_locations.a_clip_local_pos]).data as *const Vec2 ).add(instance as usize);
            self.a_clip_local_rect = *( (*attribs[self.common.attrib_locations.a_clip_local_rect]).data as *const Vec4 ).add(instance as usize);
            self.a_clip_mode = *( (*attribs[self.common.attrib_locations.a_clip_mode]).data as *const f32 ).add(instance as usize);
            self.a_clip_rect_tl = *( (*attribs[self.common.attrib_locations.a_clip_rect_tl]).data as *const Vec4 ).add(instance as usize);
            self.a_clip_radii_tl = *( (*attribs[self.common.attrib_locations.a_clip_radii_tl]).data as *const Vec4 ).add(instance as usize);
            self.a_clip_rect_tr = *( (*attribs[self.common.attrib_locations.a_clip_rect_tr]).data as *const Vec4 ).add(instance as usize);
            self.a_clip_radii_tr = *( (*attribs[self.common.attrib_locations.a_clip_radii_tr]).data as *const Vec4 ).add(instance as usize);
            self.a_clip_rect_bl = *( (*attribs[self.common.attrib_locations.a_clip_rect_bl]).data as *const Vec4 ).add(instance as usize);
            self.a_clip_radii_bl = *( (*attribs[self.common.attrib_locations.a_clip_radii_bl]).data as *const Vec4 ).add(instance as usize);
            self.a_clip_rect_br = *( (*attribs[self.common.attrib_locations.a_clip_rect_br]).data as *const Vec4 ).add(instance as usize);
            self.a_clip_radii_br = *( (*attribs[self.common.attrib_locations.a_clip_radii_br]).data as *const Vec4 ).add(instance as usize);
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
        if index == 4 { self.common.u_transform = Mat4::from_cols_array(value); }
    }
}

//
// Fragment Shader
//
#[derive(Clone, Debug, Default)]
struct CsClipRectangleFrag {
    vert: CsClipRectangleVert,
    v_local_pos: vec4,
    interp_step: InterpOutputs,
    interp_perspective: InterpOutputs,
}

impl CsClipRectangleFrag {
    fn main(&self, context: &ShaderContext) -> vec4 {
        let local_pos = self.v_local_pos.xy() / self.v_local_pos.w;
        let aa_range = context.fwidth(local_pos).x.recip();

        let dist = self.distance_to_rounded_rect(
            local_pos,
            self.vert.common.v_clip_plane_tl, self.vert.common.v_clip_center_radius_tl,
            self.vert.common.v_clip_plane_tr, self.vert.common.v_clip_center_radius_tr,
            self.vert.common.v_clip_plane_br, self.vert.common.v_clip_center_radius_br,
            self.vert.common.v_clip_plane_bl, self.vert.common.v_clip_center_radius_bl,
            self.vert.common.v_transform_bounds,
        );

        let alpha = self.distance_aa(aa_range, dist);
        let final_alpha = (alpha).lerp(1.0 - alpha, self.vert.common.v_clip_mode.x);
        
        let final_final_alpha = if self.v_local_pos.w > 0.0 { final_alpha } else { 0.0 };
        vec4(final_final_alpha, 0.0, 0.0, 1.0)
    }

    fn signed_distance_rect_xy(&self, pos: Vec2, p0: Vec2, p1: Vec2) -> Vec2 {
        (p0 - pos).max(pos - p1)
    }

    fn signed_distance_rect(&self, pos: Vec2, p0: Vec2, p1: Vec2) -> f32 {
        let d = self.signed_distance_rect_xy(pos, p0, p1);
        d.x.max(d.y)
    }

    fn distance_to_ellipse_approx(&self, p: Vec2, inv_radii_sq: Vec2, scale: f32) -> f32 {
        let p_r = p * inv_radii_sq;
        let g = p.dot(p_r) - scale;
        let dg = (1.0 + scale) * p_r;
        g * (dg.dot(dg)).inv_sqrt()
    }
    
    fn distance_to_rounded_rect(&self, pos: Vec2, plane_tl: Vec3, center_radius_tl: Vec4, plane_tr: Vec3, center_radius_tr: Vec4, plane_br: Vec3, center_radius_br: Vec4, plane_bl: Vec3, center_radius_bl: Vec4, rect_bounds: Vec4) -> f32 {
        let mut corner = vec4(1e-6, 1e-6, 1.0, 1.0);
        let mut cr_tl = center_radius_tl; cr_tl.xy() -= pos;
        let mut cr_tr = center_radius_tr; cr_tr.xy() = (cr_tr.xy() - pos) * vec2(-1.0, 1.0);
        let mut cr_br = center_radius_br; cr_br.xy() = pos - cr_br.xy();
        let mut cr_bl = center_radius_bl; cr_bl.xy() = (cr_bl.xy() - pos) * vec2(1.0, -1.0);
        
        if pos.dot(plane_tl.xy()) > plane_tl.z { corner = cr_tl; }
        if pos.dot(plane_tr.xy()) > plane_tr.z { corner = cr_tr; }
        if pos.dot(plane_br.xy()) > plane_br.z { corner = cr_br; }
        if pos.dot(plane_bl.xy()) > plane_bl.z { corner = cr_bl; }

        self.distance_to_ellipse_approx(corner.xy(), corner.zw(), 1.0)
            .max(self.signed_distance_rect(pos, rect_bounds.xy(), rect_bounds.zw()))
    }
    
    fn distance_aa(&self, aa_range: f32, signed_distance: f32) -> f32 {
        let dist = signed_distance * aa_range;
        (0.5 - dist).clamp(0.0, 1.0)
    }
}

impl FragmentShader for CsClipRectangleFrag {
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
        let color = self.main(context);
        context.write_output(color);
        self.step_interp_inputs(4);
    }
    
    fn skip(&mut self, steps: i32) {
        self.step_interp_inputs(steps);
    }

    fn run_perspective(&mut self, context: &mut ShaderContext, next_w: &[f32; 4]) {
        let color = self.main(context);
        context.write_output(color);
        self.step_perspective_inputs(4, next_w);
    }
    
    fn skip_perspective(&mut self, steps: i32, next_w: &[f32; 4]) {
        self.step_perspective_inputs(steps, next_w);
    }
    
    fn draw_span_r8(&mut self, context: &mut ShaderContext) -> i32 {
        // A direct translation of this highly specialized span function would be very complex
        // and depend heavily on rasterizer internals (`swgl_...` functions).
        // Returning 0 indicates that the generic `run` path should be taken.
        0
    }
}

impl CsClipRectangleFrag {
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
pub struct CsClipRectangleProgram {
    frag: CsClipRectangleFrag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(CsClipRectangleProgram::default())
}

impl Program for CsClipRectangleProgram {
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
