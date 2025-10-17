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
struct CompositeTextureRectCommon {
    attrib_locations: AttribLocations,
    u_transform: mat4,
    // Flat varyings (set in VS, used in FS)
    v_color: vec4,
    v_uv_bounds: vec4,
}

#[derive(Clone, Debug, Default)]
struct AttribLocations {
    a_position: usize,
    a_device_rect: usize,
    a_device_clip_rect: usize,
    a_color: usize,
    a_params: usize,
    a_flip: usize,
    a_uv_rect0: usize,
}

impl AttribLocations {
    fn bind_loc(&mut self, name: &CStr, index: i32) {
        if strcmp(name, "aPosition") { self.a_position = index as usize; }
        else if strcmp(name, "aDeviceRect") { self.a_device_rect = index as usize; }
        else if strcmp(name, "aDeviceClipRect") { self.a_device_clip_rect = index as usize; }
        else if strcmp(name, "aColor") { self.a_color = index as usize; }
        else if strcmp(name, "aParams") { self.a_params = index as usize; }
        else if strcmp(name, "aFlip") { self.a_flip = index as usize; }
        else if strcmp(name, "aUvRect0") { self.a_uv_rect0 = index as usize; }
    }

    fn get_loc(&self, name: &CStr) -> i32 {
        if strcmp(name, "aPosition") { if self.a_position != NULL_ATTRIB { self.a_position as i32 } else { -1 } }
        else if strcmp(name, "aDeviceRect") { if self.a_device_rect != NULL_ATTRIB { self.a_device_rect as i32 } else { -1 } }
        else if strcmp(name, "aDeviceClipRect") { if self.a_device_clip_rect != NULL_ATTRIB { self.a_device_clip_rect as i32 } else { -1 } }
        else if strcmp(name, "aColor") { if self.a_color != NULL_ATTRIB { self.a_color as i32 } else { -1 } }
        else if strcmp(name, "aParams") { if self.a_params != NULL_ATTRIB { self.a_params as i32 } else { -1 } }
        else if strcmp(name, "aFlip") { if self.a_flip != NULL_ATTRIB { self.a_flip as i32 } else { -1 } }
        else if strcmp(name, "aUvRect0") { if self.a_uv_rect0 != NULL_ATTRIB { self.a_uv_rect0 as i32 } else { -1 } }
        else { -1 }
    }
}

//
// Vertex Shader
//

#[derive(Clone, Debug, Default)]
struct CompositeTextureRectVert {
    common: CompositeTextureRectCommon,
    // Inputs
    a_position: vec2,
    a_device_rect: vec4,
    a_device_clip_rect: vec4,
    a_color: vec4,
    a_params: vec4,
    a_flip: vec2,
    a_uv_rect0: vec4,
    // Outputs
    v_uv: vec2,
    gl_position: vec4,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct InterpOutputs {
    v_uv: vec2,
}

impl CompositeTextureRectVert {
    fn main(&mut self) {
        let device_rect = self.a_device_rect.lerp(self.a_device_rect.zwxy(), self.a_flip.extend(self.a_flip).xyxy());
        
        let world_pos = device_rect.xy().lerp(device_rect.zw(), self.a_position);
        let clipped_world_pos = world_pos.clamp(self.a_device_clip_rect.xy(), self.a_device_clip_rect.zw());

        let mut uv = (clipped_world_pos - device_rect.xy()) / (device_rect.zw() - device_rect.xy());
        uv = self.a_uv_rect0.xy().lerp(self.a_uv_rect0.zw(), uv);

        let mut uv_bounds = vec4(
            self.a_uv_rect0.xy().min(self.a_uv_rect0.zw()).x,
            self.a_uv_rect0.xy().min(self.a_uv_rect0.zw()).y,
            self.a_uv_rect0.xy().max(self.a_uv_rect0.zw()).x,
            self.a_uv_rect0.xy().max(self.a_uv_rect0.zw()).y,
        );

        if self.a_params.y as i32 == 1 {
            uv_bounds += vec4(0.5, 0.5, -0.5, -0.5);
            // In TEXTURE_RECT, UVs are not normalized, so texture_size division is not needed here
            // It would be needed for TEXTURE_2D
        }
        
        self.v_uv = uv;
        self.common.v_uv_bounds = uv_bounds;
        self.common.v_color = self.a_color;
        self.gl_position = self.common.u_transform * vec4::new(clipped_world_pos.x, clipped_world_pos.y, 0.0, 1.0);
    }
}

impl VertexShader for CompositeTextureRectVert {
    fn init_batch(&mut self, _context: &mut ShaderContext) {}
    
    fn load_attribs(&mut self, attribs: &[&VertexAttrib], start: u32, instance: i32, _count: i32) {
        unsafe {
            let a_pos_attrib = &*attribs[self.common.attrib_locations.a_position];
            let a_device_rect_attrib = &*attribs[self.common.attrib_locations.a_device_rect];
            let a_device_clip_rect_attrib = &*attribs[self.common.attrib_locations.a_device_clip_rect];
            let a_color_attrib = &*attribs[self.common.attrib_locations.a_color];
            let a_params_attrib = &*attribs[self.common.attrib_locations.a_params];
            let a_flip_attrib = &*attribs[self.common.attrib_locations.a_flip];
            let a_uv_rect0_attrib = &*attribs[self.common.attrib_locations.a_uv_rect0];

            let pos_ptr = (a_pos_attrib.data as *const u8).add(a_pos_attrib.stride * start as usize) as *const Vec2;
            self.a_position = *pos_ptr;

            let dr_ptr = (a_device_rect_attrib.data as *const u8).add(a_device_rect_attrib.stride * instance as usize) as *const Vec4;
            self.a_device_rect = *dr_ptr;

            let dcr_ptr = (a_device_clip_rect_attrib.data as *const u8).add(a_device_clip_rect_attrib.stride * instance as usize) as *const Vec4;
            self.a_device_clip_rect = *dcr_ptr;

            let color_ptr = (a_color_attrib.data as *const u8).add(a_color_attrib.stride * instance as usize) as *const Vec4;
            self.a_color = *color_ptr;

            let params_ptr = (a_params_attrib.data as *const u8).add(a_params_attrib.stride * instance as usize) as *const Vec4;
            self.a_params = *params_ptr;
            
            let flip_ptr = (a_flip_attrib.data as *const u8).add(a_flip_attrib.stride * instance as usize) as *const Vec2;
            self.a_flip = *flip_ptr;
            
            let uv_rect0_ptr = (a_uv_rect0_attrib.data as *const u8).add(a_uv_rect0_attrib.stride * instance as usize) as *const Vec4;
            self.a_uv_rect0 = *uv_rect0_ptr;
        }
    }

    fn run_primitive(&mut self, _context: &ShaderContext, interps: *mut u8, interp_stride: usize) {
        self.main();
        
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
        if index == 1 {
            self.common.u_transform = Mat4::from_cols_array(value);
        }
    }
}

//
// Fragment Shader
//

#[derive(Clone, Debug, Default)]
struct CompositeTextureRectFrag {
    vert: CompositeTextureRectVert,
    // Varying inputs from rasterizer
    v_uv: vec2,
    interp_step: InterpOutputs,
    interp_perspective: InterpOutputs,
}

impl CompositeTextureRectFrag {
    fn main(&self, context: &ShaderContext) -> vec4 {
        let uv = self.v_uv.clamp(self.vert.common.v_uv_bounds.xy(), self.vert.common.v_uv_bounds.zw());
        let texel = context.texture_rect(SamplerId::SColor0, uv);
        self.vert.common.v_color * texel
    }
}

impl FragmentShader for CompositeTextureRectFrag {
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
        if self.vert.common.v_color != vec4::ONE {
            context.commit_texture_color_rgba8_rect(
                SamplerId::SColor0,
                self.v_uv,
                self.vert.common.v_uv_bounds,
                self.vert.common.v_color,
            );
        } else {
            context.commit_texture_rgba8_rect(
                SamplerId::SColor0,
                self.v_uv,
                self.vert.common.v_uv_bounds,
            );
        }
        1
    }
}

impl CompositeTextureRectFrag {
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
pub struct CompositeTextureRectProgram {
    frag: CompositeTextureRectFrag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(CompositeTextureRectProgram::default())
}

impl Program for CompositeTextureRectProgram {
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
