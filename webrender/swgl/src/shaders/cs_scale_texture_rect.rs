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
struct CsScaleTextureRectCommon {
    attrib_locations: AttribLocations,
    u_transform: mat4,
    // Flat Varyings
    v_uv_rect: vec4,
}

#[derive(Clone, Debug, Default)]
struct AttribLocations {
    a_position: usize,
    a_scale_target_rect: usize,
    a_scale_source_rect: usize,
    a_source_rect_type: usize,
}

impl AttribLocations {
    fn bind_loc(&mut self, name: &CStr, index: i32) {
        if strcmp(name, "aPosition") {
            self.a_position = index as usize;
        } else if strcmp(name, "aScaleTargetRect") {
            self.a_scale_target_rect = index as usize;
        } else if strcmp(name, "aScaleSourceRect") {
            self.a_scale_source_rect = index as usize;
        } else if strcmp(name, "aSourceRectType") {
            self.a_source_rect_type = index as usize;
        }
    }

    fn get_loc(&self, name: &CStr) -> i32 {
        if strcmp(name, "aPosition") {
            if self.a_position != NULL_ATTRIB { self.a_position as i32 } else { -1 }
        } else if strcmp(name, "aScaleTargetRect") {
            if self.a_scale_target_rect != NULL_ATTRIB { self.a_scale_target_rect as i32 } else { -1 }
        } else if strcmp(name, "aScaleSourceRect") {
            if self.a_scale_source_rect != NULL_ATTRIB { self.a_scale_source_rect as i32 } else { -1 }
        } else if strcmp(name, "aSourceRectType") {
            if self.a_source_rect_type != NULL_ATTRIB { self.a_source_rect_type as i32 } else { -1 }
        } else {
            -1
        }
    }
}

//
// Vertex Shader
//

#[derive(Clone, Debug, Default)]
struct CsScaleTextureRectVert {
    common: CsScaleTextureRectCommon,
    // Inputs
    a_position: vec2,
    a_scale_target_rect: vec4,
    a_scale_source_rect: vec4,
    a_source_rect_type: f32,
    // Outputs
    v_uv: vec2,
    gl_position: vec4,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct InterpOutputs {
    v_uv: vec2,
}

impl CsScaleTextureRectVert {
    fn main(&mut self, _context: &ShaderContext) {
        let src_offset = self.a_scale_source_rect.xy();
        let src_size = self.a_scale_source_rect.zw() - self.a_scale_source_rect.xy();

        self.common.v_uv_rect = vec4(
            self.a_scale_source_rect.xy().min(self.a_scale_source_rect.zw()).x,
            self.a_scale_source_rect.xy().min(self.a_scale_source_rect.zw()).y,
            self.a_scale_source_rect.xy().max(self.a_scale_source_rect.zw()).x,
            self.a_scale_source_rect.xy().max(self.a_scale_source_rect.zw()).y,
        );

        self.v_uv = src_offset + (src_size * self.a_position);

        if self.a_source_rect_type as i32 == 1 {
            self.common.v_uv_rect = vec4(
                self.common.v_uv_rect.x + 0.5,
                self.common.v_uv_rect.y + 0.5,
                self.common.v_uv_rect.z - 0.5,
                self.common.v_uv_rect.w - 0.5,
            );
            // The original GLSL divides by vec2(1.0, 1.0) here, which is a no-op.
            // It is omitted in this translation.
        }

        let pos = self.a_scale_target_rect.xy().lerp(self.a_scale_target_rect.zw(), self.a_position);
        self.gl_position = self.common.u_transform * vec4(pos.x, pos.y, 0.0, 1.0);
    }
}

impl VertexShader for CsScaleTextureRectVert {
    fn init_batch(&mut self, _context: &mut ShaderContext) {}

    fn load_attribs(&mut self, attribs: &[&VertexAttrib], start: u32, instance: i32, _count: i32) {
        unsafe {
            let a_pos_attrib = &*attribs[self.common.attrib_locations.a_position];
            let a_target_rect_attrib = &*attribs[self.common.attrib_locations.a_scale_target_rect];
            let a_source_rect_attrib = &*attribs[self.common.attrib_locations.a_scale_source_rect];
            let a_source_type_attrib = &*attribs[self.common.attrib_locations.a_source_rect_type];
            
            let pos_ptr = (a_pos_attrib.data as *const u8).add(a_pos_attrib.stride * start as usize) as *const Vec2;
            self.a_position = *pos_ptr;

            let target_rect_ptr = (a_target_rect_attrib.data as *const u8).add(a_target_rect_attrib.stride * instance as usize) as *const Vec4;
            self.a_scale_target_rect = *target_rect_ptr;

            let source_rect_ptr = (a_source_rect_attrib.data as *const u8).add(a_source_rect_attrib.stride * instance as usize) as *const Vec4;
            self.a_scale_source_rect = *source_rect_ptr;

            let source_type_ptr = (a_source_type_attrib.data as *const u8).add(a_source_type_attrib.stride * instance as usize) as *const f32;
            self.a_source_rect_type = *source_type_ptr;
        }
    }

    fn run_primitive(&mut self, context: &ShaderContext, interps: *mut u8, interp_stride: usize) {
        self.main(context);

        unsafe {
            let mut dest_ptr = interps as *mut InterpOutputs;
            for _ in 0..4 {
                (*dest_ptr).v_uv = self.v_uv;
                dest_ptr = (dest_ptr as *mut u8).add(interp_stride) as *mut InterpOutputs;
            }
        }
    }
    
    fn set_uniform_1i(&mut self, _index: i32, _value: i32) {}
    fn set_uniform_4fv(&mut self, _index: i32, _value: &[f32; 4]) {}
    fn set_uniform_matrix4fv(&mut self, index: i32, value: &[f32; 16]) {
        if index == 1 { // uTransform
            self.common.u_transform = Mat4::from_cols_array(value);
        }
    }
}

//
// Fragment Shader
//

#[derive(Clone, Debug, Default)]
struct CsScaleTextureRectFrag {
    vert: CsScaleTextureRectVert,
    // Varying inputs from rasterizer
    v_uv: vec2,
    interp_step: InterpOutputs,
    interp_perspective: InterpOutputs,
}

impl CsScaleTextureRectFrag {
    #[inline(always)]
    fn main(&self, context: &ShaderContext) -> vec4 {
        let st = self.v_uv.clamp(self.vert.common.v_uv_rect.xy(), self.vert.common.v_uv_rect.zw());
        context.texture_rect(SamplerId::SColor0, st)
    }
}

impl FragmentShader for CsScaleTextureRectFrag {
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
    
    fn draw_span_rgba8(&mut self, context: &mut ShaderContext) -> i32 {
        context.commit_texture_linear_rgba8_rect(
            SamplerId::SColor0,
            self.v_uv,
            self.vert.common.v_uv_rect
        );
        1
    }
}

impl CsScaleTextureRectFrag {
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
pub struct CsScaleTextureRectProgram {
    frag: CsScaleTextureRectFrag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(CsScaleTextureRectProgram::default())
}

impl Program for CsScaleTextureRectProgram {
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
