use crate::common::*;
use crate::common_types::*;
use crate::traits::{FragmentShader, Program, VertexShader};
use glam::{ivec2, ivec4, mat4, vec2, vec3, vec4, Mat4, Vec2, Vec3, Vec4};
use std::ffi::CStr;
use std::mem;
use super::strcmp;

//
// C++ Structs
//
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
struct ConicGradient {
    pub center: vec2,
    pub scale: vec2,
    pub start_offset: f32,
    pub end_offset: f32,
    pub angle: f32,
    pub repeat: f32,
}

//
// Common Struct
//

#[derive(Clone, Debug, Default)]
struct PsQuadConicGradientCommon {
    attrib_locations: AttribLocations,
    u_transform: mat4,
    // Flat varyings
    v_color: vec4,
    v_flags: ivec4,
    v_gradient_address: ivec2,
    v_gradient_repeat: vec2,
    v_start_offset_offset_scale_angle_vec: vec3,
}

#[derive(Clone, Debug, Default)]
struct AttribLocations {
    a_position: usize,
    a_data: usize,
}

impl AttribLocations {
    fn bind_loc(&mut self, name: &CStr, index: i32) {
        if strcmp(name, "aPosition") {
            self.a_position = index as usize;
        } else if strcmp(name, "aData") {
            self.a_data = index as usize;
        }
    }

    fn get_loc(&self, name: &CStr) -> i32 {
        if strcmp(name, "aPosition") {
            if self.a_position != NULL_ATTRIB { self.a_position as i32 } else { -1 }
        } else if strcmp(name, "aData") {
            if self.a_data != NULL_ATTRIB { self.a_data as i32 } else { -1 }
        } else {
            -1
        }
    }
}

//
// Vertex Shader
//

#[derive(Clone, Debug, Default)]
struct PsQuadConicGradientVert {
    common: PsQuadConicGradientCommon,
    // Inputs
    a_position: vec2,
    a_data: ivec4,
    // Outputs
    v_dir: vec2,
    gl_position: vec4,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct InterpOutputs {
    v_dir: vec2,
}

impl PsQuadConicGradientVert {
    fn main(&mut self, context: &mut ShaderContext) {
        let prim = self.quad_primive_info(context);
        
        let quad_flags = prim.quad_flags;
        if (quad_flags & 16) != 0 {
            self.common.v_flags.z = 1;
        } else {
            self.common.v_flags.z = 0;
        }

        self.antialiasing_vertex(&prim);
        self.pattern_vertex(&prim, context);
    }
    
    // Corresponds to quad_primive_info in the C++ code
    fn quad_primive_info(&mut self, context: &mut ShaderContext) -> PrimitiveInfo {
        let qi = context.decode_instance(self.a_data);
        let qh = context.fetch_header(qi.prim_address_i);
        let transform = context.fetch_transform(qh.transform_id);
        let task = context.fetch_picture_task(qi.picture_task_address);
        let prim = context.fetch_primitive(qi.prim_address_f);
        let z = (qh.z_id as f32) / 65535.0; // Approximation of `unpackSnorm16`

        let seg = if qi.segment_index == 255 {
            QuadSegment { rect: prim.bounds, uv_rect: prim.uv_rect }
        } else {
            context.fetch_segment(qi.prim_address_f, qi.segment_index as i32)
        };

        let mut local_coverage_rect = seg.rect;
        local_coverage_rect.p0 = local_coverage_rect.p0.max(prim.clip.p0);
        local_coverage_rect.p1 = local_coverage_rect.p1.min(prim.clip.p1);
        local_coverage_rect.p1 = local_coverage_rect.p1.max(local_coverage_rect.p0);

        match qi.part_index {
            1 => { local_coverage_rect.p1.x = local_coverage_rect.p0.x + 2.0; /* swgl_antiAlias(1); */ },
            2 => {
                local_coverage_rect.p0.x += 2.0;
                local_coverage_rect.p1.x -= 2.0;
                local_coverage_rect.p1.y = local_coverage_rect.p0.y + 2.0;
                /* swgl_antiAlias(2); */
            },
            3 => { local_coverage_rect.p0.x = local_coverage_rect.p1.x - 2.0; /* swgl_antiAlias(4); */ },
            4 => {
                local_coverage_rect.p0.x += 2.0;
                local_coverage_rect.p1.x -= 2.0;
                local_coverage_rect.p0.y = local_coverage_rect.p1.y - 2.0;
                /* swgl_antiAlias(8); */
            },
            0 => {
                local_coverage_rect.p0.x += if (qi.edge_flags & 1) != 0 { 2.0 } else { 0.0 };
                local_coverage_rect.p1.x -= if (qi.edge_flags & 4) != 0 { 2.0 } else { 0.0 };
                local_coverage_rect.p0.y += if (qi.edge_flags & 2) != 0 { 2.0 } else { 0.0 };
                local_coverage_rect.p1.y -= if (qi.edge_flags & 8) != 0 { 2.0 } else { 0.0 };
            },
            _ => { /* swgl_antiAlias(qi.edge_flags); */ }
        }

        let local_pos = local_coverage_rect.p0.lerp(local_coverage_rect.p1, self.a_position);
        
        let mut device_pixel_scale = task.device_pixel_scale;
        if (qi.quad_flags & 4) != 0 {
            device_pixel_scale = 1.0;
        }

        let (gl_pos, vi) = context.write_vertex_quad(local_pos, z, &transform, &task, qi.quad_flags, &self.common.u_transform);
        self.gl_position = gl_pos;

        self.common.v_color = prim.color;
        let pattern_tx = prim.pattern_scale_offset;

        PrimitiveInfo {
            local_pos: context.scale_offset_map_point(pattern_tx, vi.local_pos),
            local_prim_rect: context.scale_offset_map_rect(pattern_tx, prim.bounds),
            local_clip_rect: context.scale_offset_map_rect(pattern_tx, prim.clip),
            segment: QuadSegment {
                rect: context.scale_offset_map_rect(pattern_tx, seg.rect),
                uv_rect: seg.uv_rect,
            },
            edge_flags: qi.edge_flags,
            quad_flags: qi.quad_flags,
            pattern_input: qh.pattern_input,
        }
    }

    fn antialiasing_vertex(&mut self, _prim: &PrimitiveInfo) {
        // empty in C++
    }

    fn pattern_vertex(&mut self, info: &PrimitiveInfo, context: &ShaderContext) {
        let gradient = context.fetch_conic_gradient(info.pattern_input.x);
        self.common.v_gradient_address.x = info.pattern_input.y;
        self.common.v_gradient_repeat.x = gradient.repeat;

        let d = gradient.end_offset - gradient.start_offset;
        let offset_scale = if d != 0.0 { 1.0 / d } else { 0.0 };
        
        self.common.v_start_offset_offset_scale_angle_vec = vec3(
            gradient.start_offset * offset_scale,
            offset_scale,
            (std::f32::consts::PI / 2.0) - gradient.angle,
        );

        self.v_dir = (info.local_pos - info.local_prim_rect.p0) * gradient.scale - gradient.center;
    }
}

impl VertexShader for PsQuadConicGradientVert {
    fn init_batch(&mut self, _context: &mut ShaderContext) {}
    
    fn load_attribs(&mut self, attribs: &[&VertexAttrib], start: u32, instance: i32, _count: i32) {
        unsafe {
            let a_pos_attrib = &*attribs[self.common.attrib_locations.a_position];
            let a_data_attrib = &*attribs[self.common.attrib_locations.a_data];
            let pos_ptr = (a_pos_attrib.data as *const u8).add(a_pos_attrib.stride * start as usize) as *const Vec2;
            self.a_position = *pos_ptr;
            let data_ptr = (a_data_attrib.data as *const u8).add(a_data_attrib.stride * instance as usize) as *const ivec4;
            self.a_data = *data_ptr;
        }
    }

    fn run_primitive(&mut self, context: &mut ShaderContext, interps: *mut u8, interp_stride: usize) {
        self.main(context);
        
        unsafe {
            let mut dest_ptr = interps as *mut InterpOutputs;
            for _ in 0..4 {
                (*dest_ptr).v_dir = self.v_dir;
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
struct PsQuadConicGradientFrag {
    vert: PsQuadConicGradientVert,
    // Varying inputs
    v_dir: vec2,
    interp_step: InterpOutputs,
    interp_perspective: InterpOutputs,
}

impl PsQuadConicGradientFrag {
    fn antialiasing_fragment(&self) -> f32 {
        1.0
    }

    // From GLSL cookbook / nvidia
    fn approx_atan2(&self, y: f32, x: f32) -> f32 {
        let a = vec2(x, y).abs();
        let slope = a.x.min(a.y) / a.x.max(a.y);
        let s2 = slope * slope;
        let mut r = ((((-0.046496473 * s2) + 0.15931422) * s2) - 0.32762277) * s2 * slope + slope;

        if a.y > a.x {
            r = (std::f32::consts::PI / 2.0) - r;
        }
        if x < 0.0 {
            r = std::f32::consts::PI - r;
        }
        r * y.signum()
    }

    fn clamp_gradient_entry(&self, offset: f32) -> f32 {
        (1.0 + (offset * 128.0)).clamp(0.0, 1.0 + 128.0)
    }

    fn sample_gradient(&self, offset: f32, context: &ShaderContext) -> vec4 {
        let mut offset = offset;
        offset -= offset.floor() * self.vert.common.v_gradient_repeat.x;
        let x = self.clamp_gradient_entry(offset);
        let entry_index = x.floor();
        let entry_fract = x - entry_index;
        
        let texels = context.fetch_from_gpu_buffer_2f(self.vert.common.v_gradient_address.x + (2 * entry_index as i32));

        texels[0].lerp(texels[0] + texels[1], entry_fract) // dither is no-op
    }

    fn pattern_fragment(&self, color: vec4, context: &ShaderContext) -> vec4 {
        let current_dir = self.v_dir;
        let current_angle = self.approx_atan2(current_dir.y, current_dir.x) + self.vert.common.v_start_offset_offset_scale_angle_vec.z;
        let offset = (current_angle / (2.0 * std::f32::consts::PI)).fract() * self.vert.common.v_start_offset_offset_scale_angle_vec.y - self.vert.common.v_start_offset_offset_scale_angle_vec.x;

        color * self.sample_gradient(offset, context)
    }

    fn main(&self, context: &ShaderContext) -> vec4 {
        let base_color = self.vert.common.v_color * self.antialiasing_fragment();
        let mut output_color = self.pattern_fragment(base_color, context);

        if self.vert.common.v_flags.z != 0 {
            output_color = vec4::splat(output_color.x);
        }
        
        output_color
    }
}

impl FragmentShader for PsQuadConicGradientFrag {
    fn read_interp_inputs(&mut self, init: *const u8, step: *const u8) {
        unsafe {
            let init = &*(init as *const InterpOutputs);
            let step = &*(step as *const InterpOutputs);
            self.v_dir = init.v_dir;
            self.interp_step.v_dir = step.v_dir * 4.0;
        }
    }

    fn read_perspective_inputs(&mut self, init: *const u8, step: *const u8, w: f32) {
        unsafe {
            let init = &*(init as *const InterpOutputs);
            let step = &*(step as *const InterpOutputs);
            let inv_w = 1.0 / w;
            self.interp_perspective.v_dir = init.v_dir;
            self.v_dir = self.interp_perspective.v_dir * inv_w;
            self.interp_step.v_dir = step.v_dir * 4.0;
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

impl PsQuadConicGradientFrag {
    fn step_interp_inputs(&mut self, steps: i32) {
        let chunks = steps as f32 * 0.25;
        self.v_dir += self.interp_step.v_dir * chunks;
    }

    fn step_perspective_inputs(&mut self, steps: i32, next_w: &[f32; 4]) {
        let chunks = steps as f32 * 0.25;
        let inv_w = 1.0 / next_w[0];
        self.interp_perspective.v_dir += self.interp_step.v_dir * chunks;
        self.v_dir = self.interp_perspective.v_dir * inv_w;
    }
}

//
// Program
//

#[derive(Clone, Debug, Default)]
pub struct PsQuadConicGradientProgram {
    frag: PsQuadConicGradientFrag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(PsQuadConicGradientProgram::default())
}

impl Program for PsQuadConicGradientProgram {
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
