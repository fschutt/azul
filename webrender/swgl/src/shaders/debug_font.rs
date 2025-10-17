use crate::common::*;
use crate::common_types::*;
use crate::traits::{FragmentShader, Program, VertexShader};
use glam::{mat4, vec2, vec4, Mat4, Vec2, Vec4};
use std::ffi::CStr;
use std::mem;
use super::strcmp;

//
// Common Struct
//

#[derive(Clone, Debug, Default)]
struct DebugFontCommon {
    attrib_locations: AttribLocations,
    u_transform: mat4,
}

#[derive(Clone, Debug, Default)]
struct AttribLocations {
    a_position: usize,
    a_color: usize,
    a_color_tex_coord: usize,
}

impl AttribLocations {
    fn bind_loc(&mut self, name: &CStr, index: i32) {
        if strcmp(name, "aPosition") {
            self.a_position = index as usize;
        } else if strcmp(name, "aColor") {
            self.a_color = index as usize;
        } else if strcmp(name, "aColorTexCoord") {
            self.a_color_tex_coord = index as usize;
        }
    }

    fn get_loc(&self, name: &CStr) -> i32 {
        if strcmp(name, "aPosition") {
            if self.a_position != NULL_ATTRIB { self.a_position as i32 } else { -1 }
        } else if strcmp(name, "aColor") {
            if self.a_color != NULL_ATTRIB { self.a_color as i32 } else { -1 }
        } else if strcmp(name, "aColorTexCoord") {
            if self.a_color_tex_coord != NULL_ATTRIB { self.a_color_tex_coord as i32 } else { -1 }
        } else {
            -1
        }
    }
}

//
// Vertex Shader
//

#[derive(Clone, Debug, Default)]
struct DebugFontVert {
    common: DebugFontCommon,
    // Inputs
    a_position: vec2,
    a_color: vec4,
    a_color_tex_coord: vec2,
    // Outputs
    v_color: vec4,
    v_color_tex_coord: vec2,
    gl_position: vec4,
}

#[repr(C)]
#[derive(Default, Copy, Clone, Debug)]
struct InterpOutputs {
    v_color_tex_coord: vec2,
    v_color: vec4,
}

impl DebugFontVert {
    fn main(&mut self) {
        self.v_color = self.a_color;
        self.v_color_tex_coord = self.a_color_tex_coord;

        let mut pos = vec4::new(self.a_position.x, self.a_position.y, 0.0, 1.0);
        pos.x = (pos.x + 0.5).floor();
        pos.y = (pos.y + 0.5).floor();

        self.gl_position = self.common.u_transform * pos;
    }
}

impl VertexShader for DebugFontVert {
    fn init_batch(&mut self, _context: &mut ShaderContext) {}

    fn load_attribs(&mut self, attribs: &[&VertexAttrib], start: u32, _instance: i32, _count: i32) {
        unsafe {
            let a_pos_attrib = &*attribs[self.common.attrib_locations.a_position];
            let a_color_attrib = &*attribs[self.common.attrib_locations.a_color];
            let a_color_tex_coord_attrib = &*attribs[self.common.attrib_locations.a_color_tex_coord];

            let pos_ptr = (a_pos_attrib.data as *const u8).add(a_pos_attrib.stride * start as usize) as *const Vec2;
            self.a_position = *pos_ptr;

            let color_ptr = (a_color_attrib.data as *const u8).add(a_color_attrib.stride * start as usize) as *const Vec4;
            self.a_color = *color_ptr;

            let tex_coord_ptr = (a_color_tex_coord_attrib.data as *const u8).add(a_color_tex_coord_attrib.stride * start as usize) as *const Vec2;
            self.a_color_tex_coord = *tex_coord_ptr;
        }
    }

    fn run_primitive(&mut self, _context: &ShaderContext, interps: *mut u8, interp_stride: usize) {
        self.main();

        unsafe {
            let mut dest_ptr = interps as *mut InterpOutputs;
            // In a SIMD implementation, get_nth would extract the nth lane.
            // For a scalar implementation, we just write the same values 4 times.
            for _ in 0..4 {
                (*dest_ptr).v_color_tex_coord = self.v_color_tex_coord;
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
struct DebugFontFrag {
    vert: DebugFontVert,
    // Varying inputs from rasterizer
    v_color_tex_coord: vec2,
    v_color: vec4,
    // Interpolation state
    interp_step: InterpOutputs,
    interp_perspective: InterpOutputs,
}

impl DebugFontFrag {
    fn main(&self, context: &ShaderContext) -> vec4 {
        let alpha = context.texture(SamplerId::SColor0, self.v_color_tex_coord).x;
        self.v_color * alpha
    }
}

impl FragmentShader for DebugFontFrag {
    fn read_interp_inputs(&mut self, init: *const u8, step: *const u8) {
        unsafe {
            let init = &*(init as *const InterpOutputs);
            let step = &*(step as *const InterpOutputs);
            self.v_color_tex_coord = init.v_color_tex_coord;
            self.v_color = init.v_color;
            self.interp_step.v_color_tex_coord = step.v_color_tex_coord * 4.0;
            self.interp_step.v_color = step.v_color * 4.0;
        }
    }
    
    fn read_perspective_inputs(&mut self, init: *const u8, step: *const u8, w: f32) {
        unsafe {
            let init = &*(init as *const InterpOutputs);
            let step = &*(step as *const InterpOutputs);
            let inv_w = 1.0 / w;

            self.interp_perspective.v_color_tex_coord = init.v_color_tex_coord;
            self.v_color_tex_coord = self.interp_perspective.v_color_tex_coord * inv_w;
            self.interp_step.v_color_tex_coord = step.v_color_tex_coord * 4.0;
            
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

impl DebugFontFrag {
    fn step_interp_inputs(&mut self, steps: i32) {
        let chunks = steps as f32 * 0.25;
        self.v_color_tex_coord += self.interp_step.v_color_tex_coord * chunks;
        self.v_color += self.interp_step.v_color * chunks;
    }

    fn step_perspective_inputs(&mut self, steps: i32, next_w: &[f32; 4]) {
        let chunks = steps as f32 * 0.25;
        let inv_w = 1.0 / next_w[0];

        self.interp_perspective.v_color_tex_coord += self.interp_step.v_color_tex_coord * chunks;
        self.v_color_tex_coord = self.interp_perspective.v_color_tex_coord * inv_w;
        
        self.interp_perspective.v_color += self.interp_step.v_color * chunks;
        self.v_color = self.interp_perspective.v_color * inv_w;
    }
}

//
// Program
//

#[derive(Clone, Debug, Default)]
pub struct DebugFontProgram {
    frag: DebugFontFrag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(DebugFontProgram::default())
}

impl Program for DebugFontProgram {
    fn get_vertex_shader(&mut self) -> &mut dyn VertexShader {
        &mut self.frag.vert
    }

    fn get_fragment_shader(&mut self) -> &mut dyn FragmentShader {
        &mut self.frag
    }
    
    fn get_uniform(&self, name: &CStr) -> i32 {
        if strcmp(name, "sColor0") { return 2; }
        if strcmp(name, "uTransform") { return 1; }
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
