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
struct PsCopyCommon {
    attrib_locations: AttribLocations,
    // No uniforms in this shader besides the sampler
}

#[derive(Clone, Debug, Default)]
struct AttribLocations {
    a_position: usize,
    a_src_rect: usize,
    a_dst_rect: usize,
    a_dst_texture_size: usize,
}

impl AttribLocations {
    fn bind_loc(&mut self, name: &CStr, index: i32) {
        if strcmp(name, "aPosition") { self.a_position = index as usize; }
        else if strcmp(name, "a_src_rect") { self.a_src_rect = index as usize; }
        else if strcmp(name, "a_dst_rect") { self.a_dst_rect = index as usize; }
        else if strcmp(name, "a_dst_texture_size") { self.a_dst_texture_size = index as usize; }
    }

    fn get_loc(&self, name: &CStr) -> i32 {
        if strcmp(name, "aPosition") { if self.a_position != NULL_ATTRIB { self.a_position as i32 } else { -1 } }
        else if strcmp(name, "a_src_rect") { if self.a_src_rect != NULL_ATTRIB { self.a_src_rect as i32 } else { -1 } }
        else if strcmp(name, "a_dst_rect") { if self.a_dst_rect != NULL_ATTRIB { self.a_dst_rect as i32 } else { -1 } }
        else if strcmp(name, "a_dst_texture_size") { if self.a_dst_texture_size != NULL_ATTRIB { self.a_dst_texture_size as i32 } else { -1 } }
        else { -1 }
    }
}

//
// Vertex Shader
//

#[derive(Clone, Debug, Default)]
struct PsCopyVert {
    common: PsCopyCommon,
    // Inputs
    a_position: vec2,
    a_src_rect: vec4,
    a_dst_rect: vec4,
    a_dst_texture_size: vec2,
    // Outputs
    v_uv: vec2,
    gl_position: vec4,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct InterpOutputs {
    v_uv: vec2,
}

impl PsCopyVert {
    #[inline(always)]
    fn main(&mut self, _context: &ShaderContext) {
        self.v_uv = self.a_src_rect.xy().lerp(self.a_src_rect.zw(), self.a_position);
        let pos = self.a_dst_rect.xy().lerp(self.a_dst_rect.zw(), self.a_position);
        self.gl_position = vec4(
            (pos.x / (self.a_dst_texture_size.x * 0.5)) - 1.0,
            (pos.y / (self.a_dst_texture_size.y * 0.5)) - 1.0,
            0.0,
            1.0,
        );
    }
}

impl VertexShader for PsCopyVert {
    fn init_batch(&mut self, _context: &mut ShaderContext) {}

    fn load_attribs(&mut self, attribs: &[&VertexAttrib], start: u32, _instance: i32, _count: i32) {
        unsafe {
            let a_pos_attrib = &*attribs[self.common.attrib_locations.a_position];
            let a_src_rect_attrib = &*attribs[self.common.attrib_locations.a_src_rect];
            let a_dst_rect_attrib = &*attribs[self.common.attrib_locations.a_dst_rect];
            let a_dst_texture_size_attrib = &*attribs[self.common.attrib_locations.a_dst_texture_size];
            
            let offset = start as usize;

            let pos_ptr = (a_pos_attrib.data as *const u8).add(a_pos_attrib.stride * offset) as *const Vec2;
            self.a_position = *pos_ptr;

            let src_rect_ptr = (a_src_rect_attrib.data as *const u8).add(a_src_rect_attrib.stride * offset) as *const Vec4;
            self.a_src_rect = *src_rect_ptr;

            let dst_rect_ptr = (a_dst_rect_attrib.data as *const u8).add(a_dst_rect_attrib.stride * offset) as *const Vec4;
            self.a_dst_rect = *dst_rect_ptr;
            
            let dst_tex_size_ptr = (a_dst_texture_size_attrib.data as *const u8).add(a_dst_texture_size_attrib.stride * offset) as *const Vec2;
            self.a_dst_texture_size = *dst_tex_size_ptr;
        }
    }

    fn run_primitive(&mut self, context: &ShaderContext, interps: *mut u8, _interp_stride: usize) {
        self.main(context);

        unsafe {
            let dest = interps as *mut InterpOutputs;
            (*dest).v_uv = self.v_uv;
        }
    }
    
    fn get_gl_position(&self) -> Vec4 {
        self.gl_position
    }

    fn set_uniform_1i(&mut self, _index: i32, _value: i32) {}
    fn set_uniform_4fv(&mut self, _index: i32, _value: &[f32; 4]) {}
    fn set_uniform_matrix4fv(&mut self, _index: i32, _value: &[f32; 16]) {}
}

//
// Fragment Shader
//

#[derive(Clone, Debug, Default)]
struct PsCopyFrag {
    vert: PsCopyVert,
    // Varying inputs from rasterizer
    v_uv: vec2,
    interp_step: InterpOutputs,
    interp_perspective: InterpOutputs,
}

impl PsCopyFrag {
    #[inline(always)]
    fn main(&self, context: &ShaderContext) -> vec4 {
        context.texel_fetch(SamplerId::SColor0, self.v_uv.as_ivec2(), 0)
    }
}

impl FragmentShader for PsCopyFrag {
    fn read_interp_inputs(&mut self, init: *const u8, step: *const u8) {
        unsafe {
            let init = &*(init as *const InterpOutputs);
            let step = &*(step as *const InterpOutputs);
            self.v_uv = init.v_uv;
            self.interp_step.v_uv = step.v_uv * 4.0;
        }
    }
    
    fn read_perspective_inputs(&mut self, init: *const u8, step: *const u8, w: f32) {
        unsafe {
            let init = &*(init as *const InterpOutputs);
            let step = &*(step as *const InterpOutputs);
            
            let inv_w = 1.0 / w;
            
            self.interp_perspective.v_uv = init.v_uv;
            self.v_uv = self.interp_perspective.v_uv * inv_w;
            self.interp_step.v_uv = step.v_uv * 4.0;
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

impl PsCopyFrag {
    fn step_interp_inputs(&mut self, steps: i32) {
        let chunks = steps as f32 * 0.25;
        self.v_uv += self.interp_step.v_uv * chunks;
    }

    fn step_perspective_inputs(&mut self, steps: i32, next_w: &[f32; 4]) {
        let chunks = steps as f32 * 0.25;
        let inv_w = 1.0 / next_w[0];
        
        self.interp_perspective.v_uv += self.interp_step.v_uv * chunks;
        self.v_uv = self.interp_perspective.v_uv * inv_w;
    }
}


//
// Program
//

#[derive(Clone, Debug, Default)]
pub struct PsCopyProgram {
    frag: PsCopyFrag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(PsCopyProgram::default())
}

impl Program for PsCopyProgram {
    fn get_vertex_shader(&mut self) -> &mut dyn VertexShader {
        &mut self.frag.vert
    }

    fn get_fragment_shader(&mut self) -> &mut dyn FragmentShader {
        &mut self.frag
    }
    
    fn get_uniform(&self, name: &CStr) -> i32 {
        if strcmp(name, "sColor0") { return 1; }
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
