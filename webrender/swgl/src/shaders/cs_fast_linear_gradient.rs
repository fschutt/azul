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
struct CsFastLinearGradientCommon {
    attrib_locations: AttribLocations,
    u_transform: mat4,
    // Flat varyings
    v_color0: vec4,
    v_color1: vec4,
}

#[derive(Clone, Debug, Default)]
struct AttribLocations {
    a_position: usize,
    a_task_rect: usize,
    a_color0: usize,
    a_color1: usize,
    a_axis_select: usize,
}

impl AttribLocations {
    fn bind_loc(&mut self, name: &CStr, index: i32) {
        if strcmp(name, "aPosition") {
            self.a_position = index as usize;
        } else if strcmp(name, "aTaskRect") {
            self.a_task_rect = index as usize;
        } else if strcmp(name, "aColor0") {
            self.a_color0 = index as usize;
        } else if strcmp(name, "aColor1") {
            self.a_color1 = index as usize;
        } else if strcmp(name, "aAxisSelect") {
            self.a_axis_select = index as usize;
        }
    }

    fn get_loc(&self, name: &CStr) -> i32 {
        if strcmp(name, "aPosition") {
            if self.a_position != NULL_ATTRIB { self.a_position as i32 } else { -1 }
        } else if strcmp(name, "aTaskRect") {
            if self.a_task_rect != NULL_ATTRIB { self.a_task_rect as i32 } else { -1 }
        } else if strcmp(name, "aColor0") {
            if self.a_color0 != NULL_ATTRIB { self.a_color0 as i32 } else { -1 }
        } else if strcmp(name, "aColor1") {
            if self.a_color1 != NULL_ATTRIB { self.a_color1 as i32 } else { -1 }
        } else if strcmp(name, "aAxisSelect") {
            if self.a_axis_select != NULL_ATTRIB { self.a_axis_select as i32 } else { -1 }
        } else {
            -1
        }
    }
}

//
// Vertex Shader
//

#[derive(Clone, Debug, Default)]
struct CsFastLinearGradientVert {
    common: CsFastLinearGradientCommon,
    // Inputs
    a_position: vec2,
    a_task_rect: vec4,
    a_color0: vec4,
    a_color1: vec4,
    a_axis_select: f32,
    // Outputs
    v_pos: f32,
    gl_position: vec4,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct InterpOutputs {
    v_pos: f32,
}

impl CsFastLinearGradientVert {
    #[inline(always)]
    fn main(&mut self) {
        self.v_pos = self.a_position.x.lerp(self.a_position.y, self.a_axis_select);

        self.common.v_color0 = self.a_color0;
        self.common.v_color1 = self.a_color1;

        let pos = self.a_task_rect.xy().lerp(self.a_task_rect.zw(), self.a_position);
        self.gl_position = self.common.u_transform * vec4(pos.x, pos.y, 0.0, 1.0);
    }
}

impl VertexShader for CsFastLinearGradientVert {
    fn init_batch(&mut self, _context: &mut ShaderContext) {}

    fn load_attribs(&mut self, attribs: &[&VertexAttrib], start: u32, instance: i32, _count: i32) {
        unsafe {
            let a_pos_attrib = &*attribs[self.common.attrib_locations.a_position];
            let pos_ptr = (a_pos_attrib.data as *const u8).add(a_pos_attrib.stride * start as usize) as *const Vec2;
            self.a_position = *pos_ptr;

            let a_task_rect_attrib = &*attribs[self.common.attrib_locations.a_task_rect];
            let task_rect_ptr = (a_task_rect_attrib.data as *const u8).add(a_task_rect_attrib.stride * instance as usize) as *const Vec4;
            self.a_task_rect = *task_rect_ptr;

            let a_color0_attrib = &*attribs[self.common.attrib_locations.a_color0];
            let color0_ptr = (a_color0_attrib.data as *const u8).add(a_color0_attrib.stride * instance as usize) as *const Vec4;
            self.a_color0 = *color0_ptr;
            
            let a_color1_attrib = &*attribs[self.common.attrib_locations.a_color1];
            let color1_ptr = (a_color1_attrib.data as *const u8).add(a_color1_attrib.stride * instance as usize) as *const Vec4;
            self.a_color1 = *color1_ptr;

            let a_axis_select_attrib = &*attribs[self.common.attrib_locations.a_axis_select];
            let axis_select_ptr = (a_axis_select_attrib.data as *const u8).add(a_axis_select_attrib.stride * instance as usize) as *const f32;
            self.a_axis_select = *axis_select_ptr;
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
        if index == 1 {
            self.common.u_transform = Mat4::from_cols_array(value);
        }
    }
}

//
// Fragment Shader
//

#[derive(Clone, Debug, Default)]
struct CsFastLinearGradientFrag {
    vert: CsFastLinearGradientVert,
    v_pos: f32,
    interp_step: InterpOutputs,
    interp_perspective: InterpOutputs,
}

impl CsFastLinearGradientFrag {
    #[inline(always)]
    fn main(&self, context: &mut ShaderContext) {
        let color = self.vert.common.v_color0.lerp(self.vert.common.v_color1, self.v_pos);
        context.write_output(color);
    }
}

impl FragmentShader for CsFastLinearGradientFrag {
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
        self.main(context);
        self.step_interp_inputs(4);
    }

    fn skip(&mut self, steps: i32) {
        self.step_interp_inputs(steps);
    }

    fn run_perspective(&mut self, context: &mut ShaderContext, next_w: &[f32; 4]) {
        self.main(context);
        self.step_perspective_inputs(4, next_w);
    }

    fn skip_perspective(&mut self, steps: i32, next_w: &[f32; 4]) {
        self.step_perspective_inputs(steps, next_w);
    }
}

impl CsFastLinearGradientFrag {
    fn step_interp_inputs(&mut self, steps: i32) {
        let chunks = steps as f32 * 0.25;
        self.v_pos += self.interp_step.v_pos * chunks;
    }

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
pub struct CsFastLinearGradientProgram {
    frag: CsFastLinearGradientFrag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(CsFastLinearGradientProgram::default())
}

impl Program for CsFastLinearGradientProgram {
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