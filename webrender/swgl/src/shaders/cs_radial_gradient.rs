use crate::common::*;
use crate::common_types::*;
use crate::traits::{FragmentShader, Program, VertexShader};
use glam::{ivec2, mat4, vec2, vec4, Mat4, Vec2, Vec4};
use std::ffi::CStr;
use std::mem;
use super::strcmp;

//
// Common Struct
//

#[derive(Clone, Debug, Default)]
struct CsRadialGradientCommon {
    attrib_locations: AttribLocations,
    u_transform: mat4,
    // Flat varyings (set in VS, used in FS)
    v_gradient_address: ivec2,
    v_gradient_repeat: vec2,
    v_start_radius: vec2, // using vec2 for alignment, only .x is used
}

#[derive(Clone, Debug, Default)]
struct AttribLocations {
    a_position: usize,
    a_task_rect: usize,
    a_center: usize,
    a_scale: usize,
    a_start_radius: usize,
    a_end_radius: usize,
    a_xy_ratio: usize,
    a_extend_mode: usize,
    a_gradient_stops_address: usize,
}

impl AttribLocations {
    fn bind_loc(&mut self, name: &CStr, index: i32) {
        if strcmp(name, "aPosition") { self.a_position = index as usize; }
        else if strcmp(name, "aTaskRect") { self.a_task_rect = index as usize; }
        else if strcmp(name, "aCenter") { self.a_center = index as usize; }
        else if strcmp(name, "aScale") { self.a_scale = index as usize; }
        else if strcmp(name, "aStartRadius") { self.a_start_radius = index as usize; }
        else if strcmp(name, "aEndRadius") { self.a_end_radius = index as usize; }
        else if strcmp(name, "aXYRatio") { self.a_xy_ratio = index as usize; }
        else if strcmp(name, "aExtendMode") { self.a_extend_mode = index as usize; }
        else if strcmp(name, "aGradientStopsAddress") { self.a_gradient_stops_address = index as usize; }
    }

    fn get_loc(&self, name: &CStr) -> i32 {
        if strcmp(name, "aPosition") { if self.a_position != NULL_ATTRIB { self.a_position as i32 } else { -1 } }
        else if strcmp(name, "aTaskRect") { if self.a_task_rect != NULL_ATTRIB { self.a_task_rect as i32 } else { -1 } }
        else if strcmp(name, "aCenter") { if self.a_center != NULL_ATTRIB { self.a_center as i32 } else { -1 } }
        else if strcmp(name, "aScale") { if self.a_scale != NULL_ATTRIB { self.a_scale as i32 } else { -1 } }
        else if strcmp(name, "aStartRadius") { if self.a_start_radius != NULL_ATTRIB { self.a_start_radius as i32 } else { -1 } }
        else if strcmp(name, "aEndRadius") { if self.a_end_radius != NULL_ATTRIB { self.a_end_radius as i32 } else { -1 } }
        else if strcmp(name, "aXYRatio") { if self.a_xy_ratio != NULL_ATTRIB { self.a_xy_ratio as i32 } else { -1 } }
        else if strcmp(name, "aExtendMode") { if self.a_extend_mode != NULL_ATTRIB { self.a_extend_mode as i32 } else { -1 } }
        else if strcmp(name, "aGradientStopsAddress") { if self.a_gradient_stops_address != NULL_ATTRIB { self.a_gradient_stops_address as i32 } else { -1 } }
        else { -1 }
    }
}

//
// Vertex Shader
//

#[derive(Clone, Debug, Default)]
struct CsRadialGradientVert {
    common: CsRadialGradientCommon,
    // Inputs
    a_position: vec2,
    a_task_rect: vec4,
    a_center: vec2,
    a_scale: vec2,
    a_start_radius: f32,
    a_end_radius: f32,
    a_xy_ratio: f32,
    a_extend_mode: i32,
    a_gradient_stops_address: i32,
    // Outputs
    v_pos: vec2,
    gl_position: vec4,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct InterpOutputs {
    v_pos: vec2,
}

impl CsRadialGradientVert {
    #[inline(always)]
    fn main(&mut self) {
        let rd = self.a_end_radius - self.a_start_radius;
        let radius_scale = if rd != 0.0 { 1.0 / rd } else { 0.0 };

        let pos = self.a_task_rect.xy().lerp(self.a_task_rect.zw(), self.a_position);
        self.gl_position = self.common.u_transform * vec4(pos.x, pos.y, 0.0, 1.0);

        self.common.v_start_radius.x = self.a_start_radius * radius_scale;
        self.v_pos = (((self.a_task_rect.zw() - self.a_task_rect.xy()) * self.a_position) * self.a_scale - self.a_center) * radius_scale;
        self.v_pos.y *= self.a_xy_ratio;
        
        self.common.v_gradient_repeat.x = if self.a_extend_mode == 1 { 1.0 } else { 0.0 };
        self.common.v_gradient_address.x = self.a_gradient_stops_address;
    }
}

impl VertexShader for CsRadialGradientVert {
    fn init_batch(&mut self, _context: &mut ShaderContext) {}

    fn load_attribs(&mut self, attribs: &[&VertexAttrib], start: u32, instance: i32, _count: i32) {
        unsafe {
            let a_pos_attrib = &*attribs[self.common.attrib_locations.a_position];
            let pos_ptr = (a_pos_attrib.data as *const u8).add(a_pos_attrib.stride * start as usize) as *const Vec2;
            self.a_position = *pos_ptr;

            let a_task_rect_attrib = &*attribs[self.common.attrib_locations.a_task_rect];
            let task_rect_ptr = (a_task_rect_attrib.data as *const u8).add(a_task_rect_attrib.stride * instance as usize) as *const Vec4;
            self.a_task_rect = *task_rect_ptr;

            let a_center_attrib = &*attribs[self.common.attrib_locations.a_center];
            let center_ptr = (a_center_attrib.data as *const u8).add(a_center_attrib.stride * instance as usize) as *const Vec2;
            self.a_center = *center_ptr;
            
            let a_scale_attrib = &*attribs[self.common.attrib_locations.a_scale];
            let scale_ptr = (a_scale_attrib.data as *const u8).add(a_scale_attrib.stride * instance as usize) as *const Vec2;
            self.a_scale = *scale_ptr;
            
            let a_start_radius_attrib = &*attribs[self.common.attrib_locations.a_start_radius];
            let start_radius_ptr = (a_start_radius_attrib.data as *const u8).add(a_start_radius_attrib.stride * instance as usize) as *const f32;
            self.a_start_radius = *start_radius_ptr;

            let a_end_radius_attrib = &*attribs[self.common.attrib_locations.a_end_radius];
            let end_radius_ptr = (a_end_radius_attrib.data as *const u8).add(a_end_radius_attrib.stride * instance as usize) as *const f32;
            self.a_end_radius = *end_radius_ptr;

            let a_xy_ratio_attrib = &*attribs[self.common.attrib_locations.a_xy_ratio];
            let xy_ratio_ptr = (a_xy_ratio_attrib.data as *const u8).add(a_xy_ratio_attrib.stride * instance as usize) as *const f32;
            self.a_xy_ratio = *xy_ratio_ptr;

            let a_extend_mode_attrib = &*attribs[self.common.attrib_locations.a_extend_mode];
            let extend_mode_ptr = (a_extend_mode_attrib.data as *const u8).add(a_extend_mode_attrib.stride * instance as usize) as *const i32;
            self.a_extend_mode = *extend_mode_ptr;

            let a_gradient_stops_address_attrib = &*attribs[self.common.attrib_locations.a_gradient_stops_address];
            let grad_stops_ptr = (a_gradient_stops_address_attrib.data as *const u8).add(a_gradient_stops_address_attrib.stride * instance as usize) as *const i32;
            self.a_gradient_stops_address = *grad_stops_ptr;
        }
    }

    fn run_primitive(&mut self, _context: &mut ShaderContext, interps: *mut u8, interp_stride: usize) {
        self.main();

        unsafe {
            let mut dest_ptr = interps as *mut InterpOutputs;
            for _ in 0..4 {
                (*dest_ptr).v_pos = self.v_pos;
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
struct CsRadialGradientFrag {
    vert: CsRadialGradientVert,
    // Varying inputs from rasterizer
    v_pos: vec2,
    interp_step: InterpOutputs,
    interp_perspective: InterpOutputs,
}

impl CsRadialGradientFrag {
    fn sample_gradient(&self, offset: f32, context: &ShaderContext) -> vec4 {
        let mut offset = offset;
        offset -= offset.floor() * self.vert.common.v_gradient_repeat.x;
        let x = (1.0 + (offset * 128.0)).clamp(0.0, 1.0 + 128.0);
        let entry_index = x.floor();
        let entry_fract = x - entry_index;
        
        let texels = context.fetch_from_gpu_buffer_2f(self.vert.common.v_gradient_address.x + (2 * entry_index as i32));

        texels[0] + (texels[1] * entry_fract) // dither is no-op
    }
    
    #[inline(always)]
    fn main(&self, context: &ShaderContext) -> vec4 {
        let offset = self.v_pos.length() - self.vert.common.v_start_radius.x;
        self.sample_gradient(offset, context)
    }
}

impl FragmentShader for CsRadialGradientFrag {
    fn read_interp_inputs(&mut self, init: *const u8, step: *const u8) {
        unsafe {
            let init = &*(init as *const InterpOutputs);
            let step = &*(step as *const InterpOutputs);
            self.v_pos = init.v_pos;
            self.interp_step.v_pos = step.v_pos * 4.0;
        }
    }
    
    fn read_perspective_inputs(&mut self, init: *const u8, step: *const u8, w: f32) {
        unsafe {
            let init = &*(init as *const InterpOutputs);
            let step = &*(step as *const InterpOutputs);
            let inv_w = 1.0 / w;
            self.interp_perspective.v_pos = init.v_pos;
            self.v_pos = self.interp_perspective.v_pos * inv_w;
            self.interp_step.v_pos = step.v_pos * 4.0;
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

    fn draw_span_rgba8(&mut self, context: &mut ShaderContext) -> i32 {
        let address = context.swgl_validate_gradient(SamplerId::SGpuBufferF, self.vert.common.v_gradient_address.x, (128.0 + 2.0) as i32);
        if address < 0 {
            return 0;
        }
        context.swgl_commit_radial_gradient_rgba8(
            SamplerId::SGpuBufferF,
            address,
            128.0,
            self.vert.common.v_gradient_repeat.x != 0.0,
            self.v_pos,
            self.vert.common.v_start_radius.x
        );
        1
    }
}

impl CsRadialGradientFrag {
    #[inline(always)]
    fn step_interp_inputs(&mut self, steps: i32) {
        let chunks = steps as f32 * 0.25;
        self.v_pos += self.interp_step.v_pos * chunks;
    }
    
    #[inline(always)]
    fn step_perspective_inputs(&mut self, steps: i32, next_w: &[f32; 4]) {
        let chunks = steps as f32 * 0.25;
        let inv_w = 1.0 / next_w[0];
        self.interp_perspective.v_pos += self.interp_step.v_pos * chunks;
        self.v_pos = self.interp_perspective.v_pos * inv_w;
    }
}

//
// Program
//

#[derive(Clone, Debug, Default)]
pub struct CsRadialGradientProgram {
    frag: CsRadialGradientFrag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(CsRadialGradientProgram::default())
}

impl Program for CsRadialGradientProgram {
    fn get_vertex_shader(&mut self) -> &mut dyn VertexShader {
        &mut self.frag.vert
    }

    fn get_fragment_shader(&mut self) -> &mut dyn FragmentShader {
        &mut self.frag
    }
    
    fn get_uniform(&self, name: &CStr) -> i32 {
        if strcmp(name, "sGpuBufferF") { return 3; }
        if strcmp(name, "sGpuBufferI") { return 4; }
        if strcmp(name, "sGpuCache") { return 2; }
        if strcmp(name, "sRenderTasks") { return 1; }
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