
use crate::common::*;
use crate::common_types::*;
use crate::traits::{FragmentShader, Program, VertexShader};
use glam::{ivec2, ivec4, mat4, vec2, vec3, vec4, Mat4, Vec2, Vec3, Vec4};
use std::ffi::CStr;
use std::mem;
use super::strcmp;

//
// Local Struct Definitions (specific to ps_quad shaders)
// These would be moved to common_types.rs if shared by more shaders.
//

#[derive(Clone, Copy, Debug, Default)]
struct QuadInstance {
    pub prim_address_i: i32,
    pub prim_address_f: i32,
    pub quad_flags: i32,
    pub edge_flags: i32,
    pub part_index: i32,
    pub segment_index: i32,
    pub picture_task_address: i32,
}

#[derive(Clone, Copy, Debug, Default)]
struct QuadHeader {
    pub transform_id: i32,
    pub z_id: i32,
    pub pattern_input: ivec2,
}

#[derive(Clone, Copy, Debug, Default)]
struct QuadPrimitive {
    pub bounds: RectWithEndpoint,
    pub clip: RectWithEndpoint,
    pub uv_rect: RectWithEndpoint,
    pub pattern_scale_offset: vec4,
    pub color: vec4,
}

#[derive(Clone, Copy, Debug, Default)]
struct QuadSegment {
    pub rect: RectWithEndpoint,
    pub uv_rect: RectWithEndpoint,
}

#[derive(Clone, Copy, Debug, Default)]
struct VertexInfo {
    local_pos: vec2,
}

#[derive(Clone, Copy, Debug, Default)]
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
struct Clip {
    rect: RectWithEndpoint,
    radii: vec4,
    mode: f32,
    space: i32,
}


//
// Common Struct
//

#[derive(Clone, Debug, Default)]
struct PsQuadMaskFastPathCommon {
    attrib_locations: AttribLocations,
    u_transform: mat4,
    // Flat varyings
    v_transform_bounds: vec4,
    v_color: vec4,
    v_flags: ivec4,
    v_clip_params: vec3,
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
struct PsQuadMaskFastPathVert {
    common: PsQuadMaskFastPathCommon,
    // Inputs
    a_position: vec2,
    a_data: ivec4,
    a_clip_data: ivec4,
    // Outputs
    gl_position: vec4,
    v_clip_local_pos: vec4,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct InterpOutputs {
    v_clip_local_pos: vec4,
}

impl PsQuadMaskFastPathVert {
    fn main(&mut self, context: &ShaderContext) {
        let prim = self.quad_primive_info(context);
        
        self.common.v_flags.z = if (prim.quad_flags & 16) != 0 { 1 } else { 0 };

        self.antialiasing_vertex(prim);
        self.pattern_vertex(context, prim);
    }

    fn antialiasing_vertex(&self, _prim: PrimitiveInfo) {
        // empty in this shader
    }

    fn pattern_vertex(&mut self, context: &ShaderContext, prim_info: PrimitiveInfo) {
        let clip = self.fetch_clip(context, self.a_clip_data.y);
        let clip_transform = context.fetch_transform(self.a_clip_data.x);
        
        self.v_clip_local_pos = clip_transform.m * vec4::new(prim_info.local_pos.x, prim_info.local_pos.y, 0.0, 1.0);
        self.common.v_clip_mode.x = clip.mode;
        
        let half_size = 0.5 * (clip.rect.p1 - clip.rect.p0);
        let radius = clip.radii.x;
        
        self.v_clip_local_pos.xy() -= (half_size + clip.rect.p0) * self.v_clip_local_pos.w;
        self.common.v_clip_params = vec3::new(half_size.x - radius, half_size.y - radius, radius);
    }
    
    fn quad_primive_info(&mut self, context: &ShaderContext) -> PrimitiveInfo {
        let qi = self.decode_instance();
        let qh = self.fetch_header(context, qi.prim_address_i);
        let transform = context.fetch_transform(qh.transform_id);
        let task = context.fetch_picture_task(qi.picture_task_address);
        let prim = self.fetch_primitive(context, qi.prim_address_f);
        let z = qh.z_id as f32;

        let seg = if qi.segment_index == 255 {
            QuadSegment { rect: prim.bounds, uv_rect: prim.uv_rect }
        } else {
            self.fetch_segment(context, qi.prim_address_f, qi.segment_index)
        };

        let mut local_coverage_rect = seg.rect;
        local_coverage_rect.p0 = local_coverage_rect.p0.max(prim.clip.p0);
        local_coverage_rect.p1 = local_coverage_rect.p1.min(prim.clip.p1);
        local_coverage_rect.p1 = local_coverage_rect.p1.max(local_coverage_rect.p0);

        match qi.part_index {
            1 => { local_coverage_rect.p1.x = local_coverage_rect.p0.x + 2.0; },
            2 => {
                local_coverage_rect.p0.x += 2.0;
                local_coverage_rect.p1.x -= 2.0;
                local_coverage_rect.p1.y = local_coverage_rect.p0.y + 2.0;
            },
            3 => { local_coverage_rect.p0.x = local_coverage_rect.p1.x - 2.0; },
            4 => {
                local_coverage_rect.p0.x += 2.0;
                local_coverage_rect.p1.x -= 2.0;
                local_coverage_rect.p0.y = local_coverage_rect.p1.y - 2.0;
            },
            0 => {
                local_coverage_rect.p0.x += self.edge_aa_offset(1, qi.edge_flags);
                local_coverage_rect.p1.x -= self.edge_aa_offset(4, qi.edge_flags);
                local_coverage_rect.p0.y += self.edge_aa_offset(2, qi.edge_flags);
                local_coverage_rect.p1.y -= self.edge_aa_offset(8, qi.edge_flags);
            },
            _ => {},
        }
        
        let local_pos = local_coverage_rect.p0.lerp(local_coverage_rect.p1, self.a_position);
        
        let mut device_pixel_scale = task.device_pixel_scale;
        if (qi.quad_flags & 4) != 0 {
            device_pixel_scale = 1.0;
        }
        
        let vi = self.write_vertex(context, local_pos, z, &transform, &task, device_pixel_scale, qi.quad_flags);
        
        self.common.v_color = prim.color;
        
        let pattern_tx = prim.pattern_scale_offset;
        let seg_rect = self.scale_offset_map_rect(pattern_tx, seg.rect);

        PrimitiveInfo {
            local_pos: self.scale_offset_map_point(pattern_tx, vi.local_pos),
            local_prim_rect: self.scale_offset_map_rect(pattern_tx, prim.bounds),
            local_clip_rect: self.scale_offset_map_rect(pattern_tx, prim.clip),
            segment: QuadSegment { rect: seg_rect, ..seg },
            edge_flags: qi.edge_flags,
            quad_flags: qi.quad_flags,
            pattern_input: qh.pattern_input,
        }
    }

    fn fetch_clip(&self, context: &ShaderContext, index: i32) -> Clip {
        let space = self.a_clip_data.z;
        let texels = context.fetch_from_gpu_buffer_3f(index);
        Clip {
            rect: RectWithEndpoint { p0: texels[0].xy(), p1: texels[0].zw() },
            radii: texels[1],
            mode: texels[2].x,
            space,
        }
    }

    // Helper functions specific to quad shaders
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
    
    fn fetch_header(&self, context: &ShaderContext, address: i32) -> QuadHeader {
        let header = context.fetch_from_gpu_buffer_1i(address);
        QuadHeader {
            transform_id: header.x,
            z_id: header.y,
            pattern_input: header.zw(),
        }
    }
    
    fn fetch_primitive(&self, context: &ShaderContext, index: i32) -> QuadPrimitive {
        let texels = context.fetch_from_gpu_buffer_5f(index);
        QuadPrimitive {
            bounds: RectWithEndpoint { p0: texels[0].xy(), p1: texels[0].zw() },
            clip: RectWithEndpoint { p0: texels[1].xy(), p1: texels[1].zw() },
            uv_rect: RectWithEndpoint { p0: texels[2].xy(), p1: texels[2].zw() },
            pattern_scale_offset: texels[3],
            color: texels[4],
        }
    }
    
    fn fetch_segment(&self, context: &ShaderContext, base: i32, index: i32) -> QuadSegment {
        let texels = context.fetch_from_gpu_buffer_2f(base + 5 + (index * 2));
        QuadSegment {
            rect: RectWithEndpoint { p0: texels[0].xy(), p1: texels[0].zw() },
            uv_rect: RectWithEndpoint { p0: texels[1].xy(), p1: texels[1].zw() },
        }
    }

    fn edge_aa_offset(&self, edge: i32, flags: i32) -> f32 {
        if (flags & edge) != 0 { 2.0 } else { 0.0 }
    }

    fn scale_offset_map_point(&self, scale_offset: vec4, p: vec2) -> vec2 {
        p * scale_offset.xy() + scale_offset.zw()
    }
    
    fn scale_offset_map_rect(&self, scale_offset: vec4, r: RectWithEndpoint) -> RectWithEndpoint {
        RectWithEndpoint {
            p0: self.scale_offset_map_point(scale_offset, r.p0),
            p1: self.scale_offset_map_point(scale_offset, r.p1),
        }
    }

    fn write_vertex(&mut self, context: &ShaderContext, local_pos: vec2, z: f32, transform: &Transform, task: &PictureTask, device_pixel_scale: f32, quad_flags: i32) -> VertexInfo {
        let mut vi = VertexInfo::default();
        let world_pos = transform.m * vec4::new(local_pos.x, local_pos.y, 0.0, 1.0);
        let mut device_pos = world_pos.xy() * device_pixel_scale;
        
        if (quad_flags & 2) != 0 {
            let device_clip_rect = RectWithEndpoint {
                p0: task.content_origin,
                p1: task.content_origin + (task.task_rect.p1 - task.task_rect.p0),
            };
            device_pos = device_pos.clamp(device_clip_rect.p0, device_clip_rect.p1);
            vi.local_pos = (transform.inv_m * vec4::new(device_pos.x / device_pixel_scale, device_pos.y / device_pixel_scale, 0.0, 1.0)).xy();
        } else {
            vi.local_pos = local_pos;
        }

        let final_offset = -task.content_origin + task.task_rect.p0;
        self.gl_position = self.common.u_transform * vec4::new(
            device_pos.x + (final_offset.x * world_pos.w),
            device_pos.y + (final_offset.y * world_pos.w),
            z * world_pos.w,
            world_pos.w,
        );

        vi
    }
}

impl VertexShader for PsQuadMaskFastPathVert {
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

    fn run_primitive(&mut self, context: &ShaderContext, interps: *mut u8, interp_stride: usize) {
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
        if index == 5 {
            self.common.u_transform = Mat4::from_cols_array(value);
        }
    }
}

//
// Fragment Shader
//

#[derive(Clone, Debug, Default)]
struct PsQuadMaskFastPathFrag {
    vert: PsQuadMaskFastPathVert,
    // Varyings
    v_clip_local_pos: vec4,
    interp_step: InterpOutputs,
    interp_perspective: InterpOutputs,
}

impl PsQuadMaskFastPathFrag {
    fn main(&self) -> vec4 {
        let base_color = self.vert.common.v_color * self.antialiasing_fragment();
        let mut output_color = self.pattern_fragment(base_color);
        if self.vert.common.v_flags.z != 0 {
            output_color = output_color.xxxx();
        }
        output_color
    }

    fn antialiasing_fragment(&self) -> f32 { 1.0 }

    fn pattern_fragment(&self, _base_color: vec4) -> vec4 {
        let clip_local_pos = self.v_clip_local_pos.xy() / self.v_clip_local_pos.w;
        let aa_range = 1.0; // In a real rasterizer, this would be computed via fwidth
        
        let dist = self.sd_rounded_box(clip_local_pos, self.vert.common.v_clip_params.xy(), self.vert.common.v_clip_params.z);
        
        let alpha = self.distance_aa(aa_range, dist);
        
        let final_alpha = alpha.lerp(1.0 - alpha, self.vert.common.v_clip_mode.x);
        vec4::splat(final_alpha)
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

impl FragmentShader for PsQuadMaskFastPathFrag {
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
}

impl PsQuadMaskFastPathFrag {
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
pub struct PsQuadMaskFastPathProgram {
    frag: PsQuadMaskFastPathFrag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(PsQuadMaskFastPathProgram::default())
}

impl Program for PsQuadMaskFastPathProgram {
    fn get_vertex_shader(&mut self) -> &mut dyn VertexShader { &mut self.frag.vert }
    fn get_fragment_shader(&mut self) -> &mut dyn FragmentShader { &mut self.frag }

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

    fn interpolants_size(&self) -> usize { mem::size_of::<InterpOutputs>() }
}
