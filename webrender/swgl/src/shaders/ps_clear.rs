use crate::common::*;
use crate::common_types::*;
use crate::traits::{FragmentShader, Program, VertexShader};
use glam::{mat4, vec2, vec4, Mat4, Vec2, Vec4};
use std::ffi::CStr;
use std::mem;
use super::strcmp;

//
// Common Structs
//

#[derive(Clone, Debug, Default)]
struct PsClearCommon {
    attrib_locations: AttribLocations,
    u_transform: mat4,
}

#[derive(Clone, Debug, Default)]
struct AttribLocations {
    a_position: usize,
    a_rect: usize,
    a_color: usize,
}

impl AttribLocations {
    fn bind_loc(&mut self, name: &CStr, index: i32) {
        if strcmp(name, "aPosition") {
            self.a_position = index as usize;
        } else if strcmp(name, "aRect") {
            self.a_rect = index as usize;
        } else if strcmp(name, "aColor") {
            self.a_color = index as usize;
        }
    }

    fn get_loc(&self, name: &CStr) -> i32 {
        if strcmp(name, "aPosition") {
            if self.a_position != NULL_ATTRIB {
                self.a_position as i32
            } else {
                -1
            }
        } else if strcmp(name, "aRect") {
            if self.a_rect != NULL_ATTRIB {
                self.a_rect as i32
            } else {
                -1
            }
        } else if strcmp(name, "aColor") {
            if self.a_color != NULL_ATTRIB {
                self.a_color as i32
            } else {
                -1
            }
        } else {
            -1
        }
    }
}

//
// Vertex Shader
//

#[derive(Clone, Debug, Default)]
struct PsClearVert {
    common: PsClearCommon,
    // Inputs
    a_position: vec2,
    a_rect: vec4,
    a_color: vec4,
    // Outputs
    v_color: vec4,
    gl_position: vec4,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct InterpOutputs {
    v_color: vec4,
}

impl PsClearVert {
    #[inline(always)]
    fn main(&mut self, _context: &ShaderContext) {
        let pos = self.a_rect.xy().lerp(self.a_rect.zw(), self.a_position);

        let mut gl_pos = self.common.u_transform * vec4(pos.x, pos.y, 0.0, 1.0);
        gl_pos.z = gl_pos.w; // Force to far plane

        self.gl_position = gl_pos;
        self.v_color = self.a_color;
    }
}

impl VertexShader for PsClearVert {
    fn init_batch(&mut self) {}

    fn load_attribs(&mut self, attribs: &[&VertexAttrib], start: u32, instance: i32, _count: i32) {
        unsafe {
            let a_pos_attrib = &*attribs[self.common.attrib_locations.a_position];
            let a_rect_attrib = &*attribs[self.common.attrib_locations.a_rect];
            let a_color_attrib = &*attribs[self.common.attrib_locations.a_color];

            let pos_ptr =
                (a_pos_attrib.data as *const u8).add(a_pos_attrib.stride * start as usize) as *const Vec2;
            self.a_position = *pos_ptr;

            let rect_ptr = (a_rect_attrib.data as *const u8)
                .add(a_rect_attrib.stride * instance as usize)
                as *const Vec4;
            self.a_rect = *rect_ptr;

            let color_ptr = (a_color_attrib.data as *const u8)
                .add(a_color_attrib.stride * instance as usize)
                as *const Vec4;
            self.a_color = *color_ptr;
        }
    }

    fn run_primitive(&mut self, context: &ShaderContext, interps: *mut u8, interp_stride: usize) {
        self.main(context);

        unsafe {
            let mut dest_ptr = interps as *mut InterpOutputs;
            // In a scalar implementation, we just write the same value four times.
            // A real SIMD implementation would use get_nth.
            for _ in 0..4 {
                (*dest_ptr).v_color = self.v_color;
                dest_ptr = (dest_ptr as *mut u8).add(interp_stride) as *mut InterpOutputs;
            }
        }
    }

    fn set_uniform_1i(&mut self, _index: i32, _value: i32) {}
    fn set_uniform_4fv(&mut self, _index: i32, _value: &[f32; 4]) {}
    fn set_uniform_matrix4fv(&mut self, index: i32, value: &[f32; 16]) {
        if index == 1 {
            self.common.u_transform = Mat4::from_cols_array(value);
        }
    }
}

//
// Fragment Shader
//

#[derive(Clone, Debug, Default)]
struct PsClearFrag {
    vert: PsClearVert,
    // Varying inputs from rasterizer
    v_color: vec4,
    interp_step: InterpOutputs,
    interp_perspective: InterpOutputs,
}

impl PsClearFrag {
    #[inline(always)]
    fn main(&self, _context: &ShaderContext) -> vec4 {
        self.v_color
    }
}

impl FragmentShader for PsClearFrag {
    fn read_interp_inputs(&mut self, init: *const u8, step: *const u8) {
        unsafe {
            let init = &*(init as *const InterpOutputs);
            let step = &*(step as *const InterpOutputs);
            self.v_color = init.v_color;
            self.interp_step.v_color = step.v_color * 4.0;
        }
    }
    
    fn read_perspective_inputs(&mut self, init: *const u8, step: *const u8, w: f32) {
        unsafe {
            let init = &*(init as *const InterpOutputs);
            let step = &*(step as *const InterpOutputs);
            let inv_w = 1.0 / w;
            self.interp_perspective.v_color = init.v_color;
            self.v_color = self.interp_perspective.v_color * inv_w;
            self.interp_step.v_color = step.v_color * 4.0;
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

impl PsClearFrag {
    fn step_interp_inputs(&mut self, steps: i32) {
        let chunks = steps as f32 * 0.25;
        self.v_color += self.interp_step.v_color * chunks;
    }

    fn step_perspective_inputs(&mut self, steps: i32, next_w: &[f32; 4]) {
        let chunks = steps as f32 * 0.25;
        let inv_w = 1.0 / next_w[0];
        self.interp_perspective.v_color += self.interp_step.v_color * chunks;
        self.v_color = self.interp_perspective.v_color * inv_w;
    }
}

//
// Program
//

#[derive(Clone, Debug, Default)]
pub struct PsClearProgram {
    frag: PsClearFrag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(PsClearProgram::default())
}

impl Program for PsClearProgram {
    fn get_vertex_shader(&mut self) -> &mut dyn VertexShader {
        &mut self.frag.vert
    }

    fn get_fragment_shader(&mut self) -> &mut dyn FragmentShader {
        &mut self.frag
    }

    fn get_uniform(&self, name: &CStr) -> i32 {
        if strcmp(name, "uTransform") {
            return 1;
        }
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
