
use crate::common::*;
use crate::common_types::*;
use crate::traits::{FragmentShader, Program, VertexShader};
use glam::{ivec2, mat4, vec2, vec3, vec4, Mat4, Vec2, Vec3, Vec4};
use std::ffi::CStr;
use std::mem;
use super::strcmp;

//
// Helper Structs
//

#[derive(Clone, Copy, Debug, Default)]
struct BlurTask {
    task_rect: RectWithEndpoint,
    blur_radius: f32,
    blur_region: vec2,
}

//
// Common Struct
//

#[derive(Clone, Debug, Default)]
struct CsBlurAlphaTargetCommon {
    attrib_locations: AttribLocations,
    u_transform: mat4,
    // Flat varyings
    v_uv_rect: vec4,
    v_offset_scale: vec2,
    v_support: ivec2,
    v_gauss_coefficients: vec2,
}

#[derive(Clone, Debug, Default)]
struct AttribLocations {
    a_position: usize,
    a_data: usize,
    a_blur_render_task_address: usize,
    a_blur_source_task_address: usize,
    a_blur_direction: usize,
    a_blur_params: usize,
}

impl AttribLocations {
    fn bind_loc(&mut self, name: &CStr, index: i32) {
        let index = index as usize;
        if strcmp(name, "aPosition") { self.a_position = index; }
        else if strcmp(name, "aData") { self.a_data = index; }
        else if strcmp(name, "aBlurRenderTaskAddress") { self.a_blur_render_task_address = index; }
        else if strcmp(name, "aBlurSourceTaskAddress") { self.a_blur_source_task_address = index; }
        else if strcmp(name, "aBlurDirection") { self.a_blur_direction = index; }
        else if strcmp(name, "aBlurParams") { self.a_blur_params = index; }
    }

    fn get_loc(&self, name: &CStr) -> i32 {
        if strcmp(name, "aPosition") { if self.a_position != NULL_ATTRIB { self.a_position as i32 } else { -1 } }
        else if strcmp(name, "aData") { if self.a_data != NULL_ATTRIB { self.a_data as i32 } else { -1 } }
        else if strcmp(name, "aBlurRenderTaskAddress") { if self.a_blur_render_task_address != NULL_ATTRIB { self.a_blur_render_task_address as i32 } else { -1 } }
        else if strcmp(name, "aBlurSourceTaskAddress") { if self.a_blur_source_task_address != NULL_ATTRIB { self.a_blur_source_task_address as i32 } else { -1 } }
        else if strcmp(name, "aBlurDirection") { if self.a_blur_direction != NULL_ATTRIB { self.a_blur_direction as i32 } else { -1 } }
        else if strcmp(name, "aBlurParams") { if self.a_blur_params != NULL_ATTRIB { self.a_blur_params as i32 } else { -1 } }
        else { -1 }
    }
}


//
// Vertex Shader
//

#[derive(Clone, Debug, Default)]
struct CsBlurAlphaTargetVert {
    common: CsBlurAlphaTargetCommon,
    // Inputs
    a_position: vec2,
    a_data: ivec4, // Although present in generated code, it's unused in this shader's logic.
    a_blur_render_task_address: i32,
    a_blur_source_task_address: i32,
    a_blur_direction: i32,
    a_blur_params: vec3,
    // Outputs
    v_uv: vec2,
    gl_position: vec4,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct InterpOutputs {
    v_uv: vec2,
}

impl CsBlurAlphaTargetVert {
    fn fetch_render_task_rect(&self, index: i32, context: &ShaderContext) -> RectWithEndpoint {
        let (rect, _) = context.fetch_render_task_data(index);
        rect
    }

    fn fetch_blur_task(&self, address: i32, context: &ShaderContext) -> BlurTask {
        let task_rect = self.fetch_render_task_rect(address, context);
        BlurTask {
            task_rect,
            blur_radius: self.a_blur_params.x,
            blur_region: self.a_blur_params.yz(),
        }
    }

    fn calculate_gauss_coefficients(&mut self, sigma: f32) {
        self.common.v_gauss_coefficients = vec2(
            1.0 / (((2.0 * std::f32::consts::PI).sqrt()) * sigma),
            (-0.5 / (sigma * sigma)).exp(),
        );
        let mut gauss_coefficient = vec3(
            self.common.v_gauss_coefficients.x,
            self.common.v_gauss_coefficients.y,
            self.common.v_gauss_coefficients.y * self.common.v_gauss_coefficients.y,
        );
        let mut gauss_coefficient_total = gauss_coefficient.x;

        let mut i = 1;
        while i <= self.common.v_support.x {
            gauss_coefficient.xy = gauss_coefficient.yz();
            let mut gauss_coefficient_subtotal = gauss_coefficient.x;
            gauss_coefficient.xy = gauss_coefficient.yz();
            gauss_coefficient_subtotal += gauss_coefficient.x;
            gauss_coefficient_total += 2.0 * gauss_coefficient_subtotal;
            i += 2;
        }
        self.common.v_gauss_coefficients.x /= gauss_coefficient_total;
    }

    #[inline(always)]
    fn main(&mut self, context: &ShaderContext) {
        let blur_task = self.fetch_blur_task(self.a_blur_render_task_address, context);
        let src_rect = self.fetch_render_task_rect(self.a_blur_source_task_address, context);
        let target_rect = blur_task.task_rect;
        let texture_size = context.texture_size(SamplerId::SColor0, 0);

        self.common.v_support.x = (1.5 * blur_task.blur_radius).ceil() as i32 * 2;
        if self.common.v_support.x > 0 {
            self.calculate_gauss_coefficients(blur_task.blur_radius);
        } else {
            self.common.v_gauss_coefficients = vec2(1.0, 1.0);
        }

        self.common.v_offset_scale = match self.a_blur_direction {
            0 => vec2(1.0 / texture_size.x, 0.0),
            1 => vec2(0.0, 1.0 / texture_size.y),
            _ => vec2::ZERO,
        };

        self.common.v_uv_rect = vec4(
            src_rect.p0.x + 0.5,
            src_rect.p0.y + 0.5,
            (src_rect.p0 + blur_task.blur_region).x - 0.5,
            (src_rect.p0 + blur_task.blur_region).y - 0.5,
        ) / texture_size.extend(texture_size).xyxy();

        let pos = target_rect.p0.lerp(target_rect.p1, self.a_position);
        
        let uv0 = src_rect.p0 / texture_size;
        let uv1 = src_rect.p1 / texture_size;
        self.v_uv = uv0.lerp(uv1, self.a_position);
        
        self.gl_position = self.common.u_transform * vec4::new(pos.x, pos.y, 0.0, 1.0);
    }
}

impl VertexShader for CsBlurAlphaTargetVert {
    fn init_batch(&mut self) {}

    fn load_attribs(&mut self, attribs: &[&VertexAttrib], start: u32, instance: i32, _count: i32) {
        unsafe {
            let a_pos_attrib = &*attribs[self.common.attrib_locations.a_position];
            let a_data_attrib = &*attribs[self.common.attrib_locations.a_data];
            let a_blur_render_task_address_attrib = &*attribs[self.common.attrib_locations.a_blur_render_task_address];
            let a_blur_source_task_address_attrib = &*attribs[self.common.attrib_locations.a_blur_source_task_address];
            let a_blur_direction_attrib = &*attribs[self.common.attrib_locations.a_blur_direction];
            let a_blur_params_attrib = &*attribs[self.common.attrib_locations.a_blur_params];

            let pos_ptr = (a_pos_attrib.data as *const u8).add(a_pos_attrib.stride * start as usize) as *const Vec2;
            self.a_position = *pos_ptr;
            
            let data_ptr = (a_data_attrib.data as *const u8).add(a_data_attrib.stride * instance as usize) as *const ivec4;
            self.a_data = *data_ptr;

            let render_task_ptr = (a_blur_render_task_address_attrib.data as *const u8).add(a_blur_render_task_address_attrib.stride * instance as usize) as *const i32;
            self.a_blur_render_task_address = *render_task_ptr;

            let source_task_ptr = (a_blur_source_task_address_attrib.data as *const u8).add(a_blur_source_task_address_attrib.stride * instance as usize) as *const i32;
            self.a_blur_source_task_address = *source_task_ptr;

            let direction_ptr = (a_blur_direction_attrib.data as *const u8).add(a_blur_direction_attrib.stride * instance as usize) as *const i32;
            self.a_blur_direction = *direction_ptr;

            let params_ptr = (a_blur_params_attrib.data as *const u8).add(a_blur_params_attrib.stride * instance as usize) as *const Vec3;
            self.a_blur_params = *params_ptr;
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
        if index == 6 {
            self.common.u_transform = Mat4::from_cols_array(value);
        }
    }
}

//
// Fragment Shader
//

#[derive(Clone, Debug, Default)]
struct CsBlurAlphaTargetFrag {
    vert: CsBlurAlphaTargetVert,
    v_uv: vec2,
    interp_step: InterpOutputs,
    interp_perspective: InterpOutputs,
}

impl CsBlurAlphaTargetFrag {
    #[inline(always)]
    fn main(&self, context: &ShaderContext) -> vec4 {
        let original_color = context.texture(SamplerId::SColor0, self.v_uv).x;
        let mut gauss_coefficient = vec3(
            self.vert.common.v_gauss_coefficients.x,
            self.vert.common.v_gauss_coefficients.y,
            self.vert.common.v_gauss_coefficients.y * self.vert.common.v_gauss_coefficients.y
        );

        let mut avg_color = original_color * gauss_coefficient.x;

        let support = self.vert.common.v_support.x.min(300);
        let mut i = 1;
        while i <= support {
            gauss_coefficient.xy = gauss_coefficient.yz();
            let mut gauss_coefficient_subtotal = gauss_coefficient.x;
            gauss_coefficient.xy = gauss_coefficient.yz();
            gauss_coefficient_subtotal += gauss_coefficient.x;

            let gauss_ratio = gauss_coefficient.x / gauss_coefficient_subtotal;

            let offset = self.vert.common.v_offset_scale * (i as f32 + gauss_ratio);

            let st0 = (self.v_uv - offset).max(self.vert.common.v_uv_rect.xy());
            let st1 = (self.v_uv + offset).min(self.vert.common.v_uv_rect.zw());

            avg_color += (context.texture(SamplerId::SColor0, st0).x + context.texture(SamplerId::SColor0, st1).x) * gauss_coefficient_subtotal;

            i += 2;
        }

        vec4::splat(avg_color)
    }
}

impl FragmentShader for CsBlurAlphaTargetFrag {
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

    fn draw_span_r8(&mut self, context: &mut ShaderContext) -> i32 {
        context.swgl_commit_gaussian_blur_r8(
            SamplerId::SColor0,
            self.v_uv,
            self.vert.common.v_uv_rect,
            self.vert.common.v_offset_scale.x != 0.0,
            self.vert.common.v_support.x,
            self.vert.common.v_gauss_coefficients,
        );
        1
    }
}

impl CsBlurAlphaTargetFrag {
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
pub struct CsBlurAlphaTargetProgram {
    frag: CsBlurAlphaTargetFrag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(CsBlurAlphaTargetProgram::default())
}

impl Program for CsBlurAlphaTargetProgram {
    fn get_vertex_shader(&mut self) -> &mut dyn VertexShader {
        &mut self.frag.vert
    }

    fn get_fragment_shader(&mut self) -> &mut dyn FragmentShader {
        &mut self.frag
    }
    
    fn get_uniform(&self, name: &CStr) -> i32 {
        if strcmp(name, "sClipMask") { return 7; }
        if strcmp(name, "sColor0") { return 8; }
        if strcmp(name, "sGpuCache") { return 2; }
        if strcmp(name, "sPrimitiveHeadersF") { return 4; }
        if strcmp(name, "sPrimitiveHeadersI") { return 5; }
        if strcmp(name, "sRenderTasks") { return 1; }
        if strcmp(name, "sTransformPalette") { return 3; }
        if strcmp(name, "uTransform") { return 6; }
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