use crate::common::*;
use crate::common_types::*;
use crate::traits::{FragmentShader, Program, VertexShader};
use glam::{ivec2, ivec4, mat4, vec2, vec3, vec4, Mat4, Vec2, Vec3, Vec4};
use std::ffi::CStr;
use std::mem;
use super::strcmp;

//
// Local Structs from C++ (specific to this shader family)
//

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
struct QuadSegment {
    rect: RectWithEndpoint,
    uv_rect: RectWithEndpoint,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
struct QuadPrimitive {
    bounds: RectWithEndpoint,
    clip: RectWithEndpoint,
    uv_rect: RectWithEndpoint,
    pattern_scale_offset: vec4,
    color: vec4,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
struct QuadHeader {
    transform_id: i32,
    z_id: i32,
    pattern_input: ivec2,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
struct QuadInstance {
    prim_address_i: i32,
    prim_address_f: i32,
    quad_flags: i32,
    edge_flags: i32,
    part_index: i32,
    segment_index: i32,
    picture_task_address: i32,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
struct VertexInfo {
    local_pos: vec2,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
struct PrimitiveInfo {
    local_pos: vec2,
    local_prim_rect: RectWithEndpoint,
    local_clip_rect: RectWithEndpoint,
    segment: QuadSegment,
    edge_flags: i32,
    quad_flags: i32,
    pattern_input: ivec2,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
struct Clip {
    rect: RectWithEndpoint,
    radii_top: vec4,
    radii_bottom: vec4,
    mode: f32,
    space: i32,
}

//
// Common Struct
//

#[derive(Clone, Debug, Default)]
struct PsQuadMaskCommon {
    attrib_locations: AttribLocations,
    u_transform: mat4,
    // Flat varyings
    v_transform_bounds: vec4,
    v_color: vec4,
    v_flags: ivec4,
    v_clip_center_radius_tl: vec4,
    v_clip_center_radius_tr: vec4,
    v_clip_center_radius_br: vec4,
    v_clip_center_radius_bl: vec4,
    v_clip_plane_a: vec4,
    v_clip_plane_b: vec4,
    v_clip_plane_c: vec4,
    v_clip_mode: vec2,
}

#[derive(Clone, Debug, Default)]
struct AttribLocations {
    a_position: usize,
    a_data: usize,
    a_clip_data: usize,
}

impl AttribLocations {
    fn bind_loc(&mut self, name: &CStr, index: i32) {
        if strcmp(name, "aPosition") { self.a_position = index as usize; }
        else if strcmp(name, "aData") { self.a_data = index as usize; }
        else if strcmp(name, "aClipData") { self.a_clip_data = index as usize; }
    }

    fn get_loc(&self, name: &CStr) -> i32 {
        if strcmp(name, "aPosition") {
            if self.a_position != NULL_ATTRIB { self.a_position as i32 } else { -1 }
        } else if strcmp(name, "aData") {
            if self.a_data != NULL_ATTRIB { self.a_data as i32 } else { -1 }
        } else if strcmp(name, "aClipData") {
            if self.a_clip_data != NULL_ATTRIB { self.a_clip_data as i32 } else { -1 }
        } else {
            -1
        }
    }
}

//
// Vertex Shader
//

#[derive(Clone, Debug, Default)]
struct PsQuadMaskVert {
    common: PsQuadMaskCommon,
    // Inputs
    a_position: vec2,
    a_data: ivec4,
    a_clip_data: ivec4,
    // Outputs
    v_clip_local_pos: vec4,
    gl_position: vec4,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct InterpOutputs {
    v_clip_local_pos: vec4,
}

impl PsQuadMaskVert {
    fn main(&mut self, context: &mut ShaderContext) {
        let prim = self.quad_primive_info(context);
        let _ = self.antialiasing_vertex(&prim);
        self.pattern_vertex(&prim, context);
    }

    fn decode_instance(&self) -> QuadInstance {
        QuadInstance {
            prim_address_i: self.a_data.x,
            prim_address_f: self.a_data.y,
            quad_flags: (self.a_data.z >> 24) & 255,
            edge_flags: (self.a_data.z >> 16) & 255,
            part_index: (self.a_data.z >> 8) & 255,
            segment_index: self.a_data.z & 255,
            picture_task_address: self.a_data.w,
        }
    }

    fn fetch_header(&self, address: i32, context: &ShaderContext) -> QuadHeader {
        let header = context.fetch_from_gpu_buffer_1i(address);
        QuadHeader {
            transform_id: header.x,
            z_id: header.y,
            pattern_input: header.zw(),
        }
    }
    
    fn fetch_primitive(&self, index: i32, context: &ShaderContext) -> QuadPrimitive {
        let texels = context.fetch_from_gpu_buffer_5f(index);
        QuadPrimitive {
            bounds: RectWithEndpoint { p0: texels[0].xy(), p1: texels[0].zw() },
            clip: RectWithEndpoint { p0: texels[1].xy(), p1: texels[1].zw() },
            uv_rect: RectWithEndpoint { p0: texels[2].xy(), p1: texels[2].zw() },
            pattern_scale_offset: texels[3],
            color: texels[4],
        }
    }

    fn fetch_segment(&self, base: i32, index: i32, context: &ShaderContext) -> QuadSegment {
        let texels = context.fetch_from_gpu_buffer_2f(base + 5 + (index * 2));
        QuadSegment {
            rect: RectWithEndpoint { p0: texels[0].xy(), p1: texels[0].zw() },
            uv_rect: RectWithEndpoint { p0: texels[1].xy(), p1: texels[1].zw() },
        }
    }
    
    fn write_vertex(&self, local_pos: vec2, z: f32, transform: &Transform, content_origin: vec2, task_rect: RectWithEndpoint, device_pixel_scale: f32, quad_flags: i32) -> VertexInfo {
        let mut vi = VertexInfo::default();
        let world_pos = transform.m * vec4::new(local_pos.x, local_pos.y, 0.0, 1.0);
        let mut device_pos = world_pos.xy() * device_pixel_scale;
        
        if (quad_flags & 2) != 0 {
            let device_clip_rect = RectWithEndpoint { p0: content_origin, p1: content_origin + (task_rect.p1 - task_rect.p0) };
            device_pos = device_pos.clamp(device_clip_rect.p0, device_clip_rect.p1);
            vi.local_pos = (transform.inv_m * vec4::new((device_pos / device_pixel_scale).x, (device_pos / device_pixel_scale).y, 0.0, 1.0)).xy();
        } else {
            vi.local_pos = local_pos;
        }

        let final_offset = -content_origin + task_rect.p0;
        self.common.u_transform * vec4::new(
            device_pos.x + (final_offset.x * world_pos.w),
            device_pos.y + (final_offset.y * world_pos.w),
            z * world_pos.w,
            world_pos.w
        );
        vi
    }

    fn scale_offset_map_point(&self, scale_offset: vec4, p: vec2) -> vec2 {
        (p * scale_offset.xy()) + scale_offset.zw()
    }
    
    fn scale_offset_map_rect(&self, scale_offset: vec4, r: RectWithEndpoint) -> RectWithEndpoint {
        RectWithEndpoint {
            p0: self.scale_offset_map_point(scale_offset, r.p0),
            p1: self.scale_offset_map_point(scale_offset, r.p1),
        }
    }

    fn quad_primive_info(&mut self, context: &mut ShaderContext) -> PrimitiveInfo {
        let qi = self.decode_instance();
        let qh = self.fetch_header(qi.prim_address_i, context);
        let transform = context.fetch_transform(qh.transform_id);
        let task = context.fetch_picture_task(qi.picture_task_address);
        let prim = self.fetch_primitive(qi.prim_address_f, context);
        let z = qh.z_id as f32;
        
        let mut seg = if qi.segment_index == 255 {
            QuadSegment { rect: prim.bounds, uv_rect: prim.uv_rect }
        } else {
            self.fetch_segment(qi.prim_address_f, qi.segment_index, context)
        };
        
        let mut local_coverage_rect = seg.rect;
        local_coverage_rect.p0 = local_coverage_rect.p0.max(prim.clip.p0);
        local_coverage_rect.p1 = local_coverage_rect.p1.min(prim.clip.p1);
        local_coverage_rect.p1 = local_coverage_rect.p1.max(local_coverage_rect.p0);

        match qi.part_index {
            1 => { local_coverage_rect.p1.x = local_coverage_rect.p0.x + 2.0; context.swgl_anti_alias(1); },
            2 => { local_coverage_rect.p0.x += 2.0; local_coverage_rect.p1.x -= 2.0; local_coverage_rect.p1.y = local_coverage_rect.p0.y + 2.0; context.swgl_anti_alias(2); },
            3 => { local_coverage_rect.p0.x = local_coverage_rect.p1.x - 2.0; context.swgl_anti_alias(4); },
            4 => { local_coverage_rect.p0.x += 2.0; local_coverage_rect.p1.x -= 2.0; local_coverage_rect.p0.y = local_coverage_rect.p1.y - 2.0; context.swgl_anti_alias(8); },
            0 => {
                local_coverage_rect.p0.x += if (qi.edge_flags & 1) != 0 { 2.0 } else { 0.0 };
                local_coverage_rect.p1.x -= if (qi.edge_flags & 4) != 0 { 2.0 } else { 0.0 };
                local_coverage_rect.p0.y += if (qi.edge_flags & 2) != 0 { 2.0 } else { 0.0 };
                local_coverage_rect.p1.y -= if (qi.edge_flags & 8) != 0 { 2.0 } else { 0.0 };
            },
            _ => { context.swgl_anti_alias(qi.edge_flags); }
        }
        
        let local_pos = local_coverage_rect.p0.lerp(local_coverage_rect.p1, self.a_position);
        
        let mut device_pixel_scale = task.device_pixel_scale;
        if (qi.quad_flags & 4) != 0 {
            device_pixel_scale = 1.0;
        }

        let vi = self.write_vertex(local_pos, z, &transform, task.content_origin, task.task_rect, device_pixel_scale, qi.quad_flags);
        self.common.v_color = prim.color;
        
        let pattern_tx = prim.pattern_scale_offset;
        seg.rect = self.scale_offset_map_rect(pattern_tx, seg.rect);

        PrimitiveInfo {
            local_pos: self.scale_offset_map_point(pattern_tx, vi.local_pos),
            local_prim_rect: self.scale_offset_map_rect(pattern_tx, prim.bounds),
            local_clip_rect: self.scale_offset_map_rect(pattern_tx, prim.clip),
            segment: seg,
            edge_flags: qi.edge_flags,
            quad_flags: qi.quad_flags,
            pattern_input: qh.pattern_input,
        }
    }

    fn antialiasing_vertex(&self, _prim: &PrimitiveInfo) {
        // Empty in original shader
    }
    
    fn fetch_clip(&self, index: i32, context: &ShaderContext) -> Clip {
        let space = self.a_clip_data.z;
        let texels = context.fetch_from_gpu_buffer_4f(index);
        Clip {
            rect: RectWithEndpoint { p0: texels[0].xy(), p1: texels[0].zw() },
            radii_top: texels[1],
            radii_bottom: texels[2],
            mode: texels[3].x,
            space,
        }
    }
    
    fn inverse_radii_squared(&self, radii: vec2) -> vec2 {
        vec2::ONE / radii.powf(2.0).max(vec2::splat(0.000001))
    }

    fn pattern_vertex(&mut self, prim_info: &PrimitiveInfo, context: &ShaderContext) {
        let clip = self.fetch_clip(self.a_clip_data.y, context);
        let clip_transform = context.fetch_transform(self.a_clip_data.x);
        
        self.v_clip_local_pos = clip_transform.m * vec4::new(prim_info.local_pos.x, prim_info.local_pos.y, 0.0, 1.0);

        if clip.space == 0 {
            self.common.v_transform_bounds = vec4(clip.rect.p0.x, clip.rect.p0.y, clip.rect.p1.x, clip.rect.p1.y);
        } else {
            let xf_bounds = RectWithEndpoint {
                p0: clip.rect.p0.max(prim_info.local_clip_rect.p0),
                p1: clip.rect.p1.min(prim_info.local_clip_rect.p1),
            };
            self.common.v_transform_bounds = vec4(xf_bounds.p0.x, xf_bounds.p0.y, xf_bounds.p1.x, xf_bounds.p1.y);
        }
        
        self.common.v_clip_mode.x = clip.mode;
        
        let r_tl = clip.radii_top.xy();
        let r_tr = clip.radii_top.zw();
        let r_br = clip.radii_bottom.zw();
        let r_bl = clip.radii_bottom.xy();
        
        self.common.v_clip_center_radius_tl = (clip.rect.p0 + r_tl).extend(self.inverse_radii_squared(r_tl).x).extend(self.inverse_radii_squared(r_tl).y);
        self.common.v_clip_center_radius_tr = vec2(clip.rect.p1.x - r_tr.x, clip.rect.p0.y + r_tr.y).extend(self.inverse_radii_squared(r_tr).x).extend(self.inverse_radii_squared(r_tr).y);
        self.common.v_clip_center_radius_br = (clip.rect.p1 - r_br).extend(self.inverse_radii_squared(r_br).x).extend(self.inverse_radii_squared(r_br).y);
        self.common.v_clip_center_radius_bl = vec2(clip.rect.p0.x + r_bl.x, clip.rect.p1.y - r_bl.y).extend(self.inverse_radii_squared(r_bl).x).extend(self.inverse_radii_squared(r_bl).y);

        let n_tl = vec2(-r_tl.y, r_tl.x);
        let n_tr = vec2(r_tr.y, -r_tr.x);
        let n_br = vec2(r_br.y, r_br.x);
        let n_bl = vec2(-r_bl.y, r_bl.x);

        let tl = n_tl.extend(n_tl.dot(vec2(clip.rect.p0.x, clip.rect.p0.y + r_tl.y)));
        let tr = n_tr.extend(n_tr.dot(vec2(clip.rect.p1.x - r_tr.x, clip.rect.p0.y)));
        let br = n_br.extend(n_br.dot(vec2(clip.rect.p1.x, clip.rect.p1.y - r_br.y)));
        let bl = n_bl.extend(n_bl.dot(vec2(clip.rect.p0.x + r_bl.x, clip.rect.p1.y)));
        
        self.common.v_clip_plane_a = vec4(tl.x, tl.y, tl.z, tr.x);
        self.common.v_clip_plane_b = vec4(tr.y, tr.z, br.x, br.y);
        self.common.v_clip_plane_c = vec4(br.z, bl.x, bl.y, bl.z);
    }
}

impl VertexShader for PsQuadMaskVert {
    fn init_batch(&mut self, _context: &mut ShaderContext) {}

    fn load_attribs(&mut self, attribs: &[&VertexAttrib], start: u32, instance: i32, _count: i32) {
        unsafe {
            let a_pos_attrib = &*attribs[self.common.attrib_locations.a_position];
            let a_data_attrib = &*attribs[self.common.attrib_locations.a_data];
            let a_clip_data_attrib = &*attribs[self.common.attrib_locations.a_clip_data];

            let pos_ptr = (a_pos_attrib.data as *const u8).add(a_pos_attrib.stride * start as usize) as *const Vec2;
            self.a_position = *pos_ptr;

            let data_ptr = (a_data_attrib.data as *const u8).add(a_data_attrib.stride * instance as usize) as *const ivec4;
            self.a_data = *data_ptr;
            
            let clip_data_ptr = (a_clip_data_attrib.data as *const u8).add(a_clip_data_attrib.stride * instance as usize) as *const ivec4;
            self.a_clip_data = *clip_data_ptr;
        }
    }

    fn run_primitive(&mut self, context: &mut ShaderContext, interps: *mut u8, interp_stride: usize) {
        self.main(context);
        unsafe {
            let mut dest_ptr = interps as *mut InterpOutputs;
            for _ in 0..4 {
                (*dest_ptr).v_clip_local_pos = self.v_clip_local_pos;
                dest_ptr = (dest_ptr as *mut u8).add(interp_stride) as *mut InterpOutputs;
            }
        }
    }
    
    fn set_uniform_1i(&mut self, _index: i32, _value: i32) {}
    fn set_uniform_4fv(&mut self, _index: i32, _value: &[f32; 4]) {}
    fn set_uniform_matrix4fv(&mut self, index: i32, value: &[f32; 16]) {
        if index == 5 { // uTransform
            self.common.u_transform = Mat4::from_cols_array(value);
        }
    }
}

//
// Fragment Shader
//

#[derive(Clone, Debug, Default)]
struct PsQuadMaskFrag {
    vert: PsQuadMaskVert,
    // Varying inputs
    v_clip_local_pos: vec4,
    interp_step: InterpOutputs,
    interp_perspective: InterpOutputs,
}

impl PsQuadMaskFrag {
    fn main(&self, context: &ShaderContext) -> vec4 {
        let base_color = self.vert.common.v_color * self.antialiasing_fragment();
        let mut output_color = self.pattern_fragment(base_color, context);
        if self.vert.common.v_flags.z != 0 {
            output_color = vec4::splat(output_color.r);
        }
        output_color
    }

    fn antialiasing_fragment(&self) -> f32 { 1.0 }

    fn distance_to_ellipse_approx(&self, p: vec2, inv_radii_sq: vec2, scale: f32) -> f32 {
        let p_r = p * inv_radii_sq;
        let g = p.dot(p_r) - scale;
        let d_g = (1.0 + scale) * p_r;
        g * d_g.length_recip()
    }
    
    fn signed_distance_rect(&self, pos: vec2, p0: vec2, p1: vec2) -> f32 {
        let d = (p0 - pos).max(pos - p1);
        d.x.max(d.y)
    }

    fn distance_to_rounded_rect(&self, pos: vec2, plane_tl: vec3, center_radius_tl: vec4, plane_tr: vec3, center_radius_tr: vec4, plane_br: vec3, center_radius_br: vec4, plane_bl: vec3, center_radius_bl: vec4, rect_bounds: vec4) -> f32 {
        let mut corner = vec4(0.000001, 0.000001, 1.0, 1.0);
        let mut cr_tl = center_radius_tl;
        let mut cr_tr = center_radius_tr;
        let mut cr_br = center_radius_br;
        let mut cr_bl = center_radius_bl;

        cr_tl.xy() = cr_tl.xy() - pos;
        cr_tr.xy() = (cr_tr.xy() - pos) * vec2(-1.0, 1.0);
        cr_br.xy() = pos - cr_br.xy();
        cr_bl.xy() = (cr_bl.xy() - pos) * vec2(1.0, -1.0);

        if pos.dot(plane_tl.xy()) > plane_tl.z { corner = cr_tl; }
        if pos.dot(plane_tr.xy()) > plane_tr.z { corner = cr_tr; }
        if pos.dot(plane_br.xy()) > plane_br.z { corner = cr_br; }
        if pos.dot(plane_bl.xy()) > plane_bl.z { corner = cr_bl; }
        
        self.distance_to_ellipse_approx(corner.xy(), corner.zw(), 1.0).max(self.signed_distance_rect(pos, rect_bounds.xy(), rect_bounds.zw()))
    }
    
    fn distance_aa(&self, aa_range: f32, signed_distance: f32) -> f32 {
        let dist = signed_distance * aa_range;
        (0.5 - dist).clamp(0.0, 1.0)
    }

    fn pattern_fragment(&self, _base_color: vec4, context: &ShaderContext) -> vec4 {
        let clip_local_pos = self.v_clip_local_pos.xy() / self.v_clip_local_pos.w;
        let aa_range = context.fwidth(clip_local_pos).x.recip();
        
        let plane_tl = vec3(self.vert.common.v_clip_plane_a.x, self.vert.common.v_clip_plane_a.y, self.vert.common.v_clip_plane_a.z);
        let plane_tr = vec3(self.vert.common.v_clip_plane_a.w, self.vert.common.v_clip_plane_b.x, self.vert.common.v_clip_plane_b.y);
        let plane_br = vec3(self.vert.common.v_clip_plane_b.z, self.vert.common.v_clip_plane_b.w, self.vert.common.v_clip_plane_c.x);
        let plane_bl = vec3(self.vert.common.v_clip_plane_c.y, self.vert.common.v_clip_plane_c.z, self.vert.common.v_clip_plane_c.w);

        let dist = self.distance_to_rounded_rect(
            clip_local_pos,
            plane_tl, self.vert.common.v_clip_center_radius_tl,
            plane_tr, self.vert.common.v_clip_center_radius_tr,
            plane_br, self.vert.common.v_clip_center_radius_br,
            plane_bl, self.vert.common.v_clip_center_radius_bl,
            self.vert.common.v_transform_bounds,
        );

        let alpha = self.distance_aa(aa_range, dist);
        let final_alpha = (1.0 - self.vert.common.v_clip_mode.x) * alpha + self.vert.common.v_clip_mode.x * (1.0 - alpha);
        
        vec4::splat(final_alpha)
    }
}

impl FragmentShader for PsQuadMaskFrag {
    fn read_interp_inputs(&mut self, init: *const u8, step: *const u8) {
        unsafe {
            let init = &*(init as *const InterpOutputs);
            let step = &*(step as *const InterpOutputs);
            self.v_clip_local_pos = init.v_clip_local_pos;
            self.interp_step.v_clip_local_pos = step.v_clip_local_pos * 4.0;
        }
    }
    fn read_perspective_inputs(&mut self, init: *const u8, step: *const u8, w: f32) {
        unsafe {
            let init = &*(init as *const InterpOutputs);
            let step = &*(step as *const InterpOutputs);
            let inv_w = 1.0 / w;
            self.interp_perspective.v_clip_local_pos = init.v_clip_local_pos;
            self.v_clip_local_pos = self.interp_perspective.v_clip_local_pos * inv_w;
            self.interp_step.v_clip_local_pos = step.v_clip_local_pos * 4.0;
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
}

impl PsQuadMaskFrag {
    fn step_interp_inputs(&mut self, steps: i32) {
        let chunks = steps as f32 * 0.25;
        self.v_clip_local_pos += self.interp_step.v_clip_local_pos * chunks;
    }
    fn step_perspective_inputs(&mut self, steps: i32, next_w: &[f32; 4]) {
        let chunks = steps as f32 * 0.25;
        let inv_w = 1.0 / next_w[0];
        self.interp_perspective.v_clip_local_pos += self.interp_step.v_clip_local_pos * chunks;
        self.v_clip_local_pos = self.interp_perspective.v_clip_local_pos * inv_w;
    }
}


//
// Program
//

#[derive(Clone, Debug, Default)]
pub struct PsQuadMaskProgram {
    frag: PsQuadMaskFrag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(PsQuadMaskProgram::default())
}

impl Program for PsQuadMaskProgram {
    fn get_vertex_shader(&mut self) -> &mut dyn VertexShader {
        &mut self.frag.vert
    }

    fn get_fragment_shader(&mut self) -> &mut dyn FragmentShader {
        &mut self.frag
    }
    
    fn get_uniform(&self, name: &CStr) -> i32 {
        if strcmp(name, "sGpuBufferF") { return 3; }
        if strcmp(name, "sGpuBufferI") { return 4; }
        if strcmp(name, "sRenderTasks") { return 2; }
        if strcmp(name, "sTransformPalette") { return 1; }
        if strcmp(name, "uTransform") { return 5; }
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
