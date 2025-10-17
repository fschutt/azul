use crate::common::*;
use crate::common_types::*;
use crate::traits::{FragmentShader, Program, VertexShader};
use glam::{ivec2, ivec4, mat3, mat4, vec2, vec3, vec4, Mat3, Mat4, Vec2, Vec3, Vec4};
use std::ffi::CStr;
use std::mem;
use super::strcmp;

//
// Common Struct
//

#[derive(Clone, Debug, Default)]
struct CsSvgFilterNodeCommon {
    attrib_locations: AttribLocations,
    u_transform: mat4,
    // Flat varyings
    v_input1_uv_rect: vec4,
    v_input2_uv_rect: vec4,
    v_data: ivec4,
    v_filter_data0: vec4,
    v_filter_input_count_filter_kind_vec: ivec2,
    v_float0: vec2,
    v_color_mat: mat4,
}

#[derive(Clone, Debug, Default)]
struct AttribLocations {
    a_position: usize,
    a_filter_target_rect: usize,
    a_filter_input1_content_scale_and_offset: usize,
    a_filter_input2_content_scale_and_offset: usize,
    a_filter_input1_task_address: usize,
    a_filter_input2_task_address: usize,
    a_filter_kind: usize,
    a_filter_input_count: usize,
    a_filter_extra_data_address: usize,
}

impl AttribLocations {
    fn bind_loc(&mut self, name: &CStr, index: i32) {
        if strcmp(name, "aPosition") { self.a_position = index as usize; }
        else if strcmp(name, "aFilterTargetRect") { self.a_filter_target_rect = index as usize; }
        else if strcmp(name, "aFilterInput1ContentScaleAndOffset") { self.a_filter_input1_content_scale_and_offset = index as usize; }
        else if strcmp(name, "aFilterInput2ContentScaleAndOffset") { self.a_filter_input2_content_scale_and_offset = index as usize; }
        else if strcmp(name, "aFilterInput1TaskAddress") { self.a_filter_input1_task_address = index as usize; }
        else if strcmp(name, "aFilterInput2TaskAddress") { self.a_filter_input2_task_address = index as usize; }
        else if strcmp(name, "aFilterKind") { self.a_filter_kind = index as usize; }
        else if strcmp(name, "aFilterInputCount") { self.a_filter_input_count = index as usize; }
        else if strcmp(name, "aFilterExtraDataAddress") { self.a_filter_extra_data_address = index as usize; }
    }

    fn get_loc(&self, name: &CStr) -> i32 {
        if strcmp(name, "aPosition") { if self.a_position != NULL_ATTRIB { self.a_position as i32 } else { -1 } }
        else if strcmp(name, "aFilterTargetRect") { if self.a_filter_target_rect != NULL_ATTRIB { self.a_filter_target_rect as i32 } else { -1 } }
        else if strcmp(name, "aFilterInput1ContentScaleAndOffset") { if self.a_filter_input1_content_scale_and_offset != NULL_ATTRIB { self.a_filter_input1_content_scale_and_offset as i32 } else { -1 } }
        else if strcmp(name, "aFilterInput2ContentScaleAndOffset") { if self.a_filter_input2_content_scale_and_offset != NULL_ATTRIB { self.a_filter_input2_content_scale_and_offset as i32 } else { -1 } }
        else if strcmp(name, "aFilterInput1TaskAddress") { if self.a_filter_input1_task_address != NULL_ATTRIB { self.a_filter_input1_task_address as i32 } else { -1 } }
        else if strcmp(name, "aFilterInput2TaskAddress") { if self.a_filter_input2_task_address != NULL_ATTRIB { self.a_filter_input2_task_address as i32 } else { -1 } }
        else if strcmp(name, "aFilterKind") { if self.a_filter_kind != NULL_ATTRIB { self.a_filter_kind as i32 } else { -1 } }
        else if strcmp(name, "aFilterInputCount") { if self.a_filter_input_count != NULL_ATTRIB { self.a_filter_input_count as i32 } else { -1 } }
        else if strcmp(name, "aFilterExtraDataAddress") { if self.a_filter_extra_data_address != NULL_ATTRIB { self.a_filter_extra_data_address as i32 } else { -1 } }
        else { -1 }
    }
}


//
// Vertex Shader
//

#[derive(Clone, Debug, Default)]
struct CsSvgFilterNodeVert {
    common: CsSvgFilterNodeCommon,
    // Inputs
    a_position: vec2,
    a_filter_target_rect: vec4,
    a_filter_input1_content_scale_and_offset: vec4,
    a_filter_input2_content_scale_and_offset: vec4,
    a_filter_input1_task_address: i32,
    a_filter_input2_task_address: i32,
    a_filter_kind: i32,
    a_filter_input_count: i32,
    a_filter_extra_data_address: ivec2,
    // Outputs
    v_input1_uv: vec2,
    v_input2_uv: vec2,
    gl_position: vec4,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct InterpOutputs {
    v_input1_uv: vec2,
    v_input2_uv: vec2,
}

fn vertex_srgb_to_linear(color: Vec3) -> Vec3 {
    let c1 = color / 12.92;
    let c2 = ((color / 1.055) + (0.055 / 1.055)).powf(2.4);
    c1.lerp(c2, color.cmple(vec3::splat(0.04045)).as_vec3())
}

impl CsSvgFilterNodeVert {
    fn main(&mut self, context: &ShaderContext) {
        let pos = self.a_filter_target_rect.xy().lerp(self.a_filter_target_rect.zw(), self.a_position);

        if self.a_filter_input_count > 0 {
            let texture_size = context.texture_size(SamplerId::SColor0, 0);
            let input_1_task = context.fetch_render_task_rect(self.a_filter_input1_task_address);
            self.common.v_input1_uv_rect = context.compute_uv_rect(input_1_task, texture_size);
            self.v_input1_uv = context.compute_uv(input_1_task, self.a_filter_input1_content_scale_and_offset, self.a_filter_target_rect.zw() - self.a_filter_target_rect.xy(), texture_size);
        }

        if self.a_filter_input_count > 1 {
            let texture_size = context.texture_size(SamplerId::SColor1, 0);
            let input_2_task = context.fetch_render_task_rect(self.a_filter_input2_task_address);
            self.common.v_input2_uv_rect = context.compute_uv_rect(input_2_task, texture_size);
            self.v_input2_uv = context.compute_uv(input_2_task, self.a_filter_input2_content_scale_and_offset, self.a_filter_target_rect.zw() - self.a_filter_target_rect.xy(), texture_size);
        }

        self.common.v_filter_input_count_filter_kind_vec.x = self.a_filter_input_count;
        self.common.v_filter_input_count_filter_kind_vec.y = self.a_filter_kind;

        match self.a_filter_kind {
            2 | 3 => {
                self.common.v_float0.x = self.a_filter_input2_content_scale_and_offset.x;
            },
            38 | 39 => {
                let mat_data = context.fetch_from_gpu_cache_4_direct(self.a_filter_extra_data_address);
                self.common.v_color_mat = Mat4::from_cols(mat_data[0], mat_data[1], mat_data[2], mat_data[3]);
                self.common.v_filter_data0 = context.fetch_from_gpu_cache_1_direct(self.a_filter_extra_data_address + ivec2::new(4, 0));
            },
            40 | 41 => {
                self.common.v_data = self.a_filter_extra_data_address.extend(0).extend(0);
            },
            42 | 43 => {
                self.common.v_filter_data0 = context.fetch_from_gpu_cache_1_direct(self.a_filter_extra_data_address);
            },
            70 => {
                self.common.v_filter_data0 = context.fetch_from_gpu_cache_1_direct(self.a_filter_extra_data_address);
                self.common.v_filter_data0.truncate() *= self.common.v_filter_data0.w;
            },
            71 => {
                self.common.v_filter_data0 = context.fetch_from_gpu_cache_1_direct(self.a_filter_extra_data_address);
                self.common.v_filter_data0.truncate() = vertex_srgb_to_linear(self.common.v_filter_data0.truncate());
                self.common.v_filter_data0.truncate() *= self.common.v_filter_data0.w;
            },
            72 => {
                self.common.v_filter_data0 = self.a_filter_input2_content_scale_and_offset;
                self.common.v_filter_data0.truncate() *= self.common.v_filter_data0.w;
            },
            73 => {
                self.common.v_filter_data0 = self.a_filter_input2_content_scale_and_offset;
                self.common.v_filter_data0.truncate() = vertex_srgb_to_linear(self.common.v_filter_data0.truncate());
                self.common.v_filter_data0.truncate() *= self.common.v_filter_data0.w;
            },
            80..=83 => {
                self.common.v_filter_data0 = self.a_filter_input2_content_scale_and_offset;
            },
            _ => {},
        }

        self.gl_position = self.common.u_transform * vec4(pos.x, pos.y, 0.0, 1.0);
    }
}

impl VertexShader for CsSvgFilterNodeVert {
    fn init_batch(&mut self, _context: &mut ShaderContext) {}

    fn load_attribs(&mut self, attribs: &[&VertexAttrib], start: u32, instance: i32, _count: i32) {
        unsafe {
            let a_pos_attrib = &*attribs[self.common.attrib_locations.a_position];
            self.a_position = *((a_pos_attrib.data as *const u8).add(a_pos_attrib.stride * start as usize) as *const Vec2);

            let a_filter_target_rect_attrib = &*attribs[self.common.attrib_locations.a_filter_target_rect];
            self.a_filter_target_rect = *((a_filter_target_rect_attrib.data as *const u8).add(a_filter_target_rect_attrib.stride * instance as usize) as *const Vec4);
            
            let a_filter_input1_content_scale_and_offset_attrib = &*attribs[self.common.attrib_locations.a_filter_input1_content_scale_and_offset];
            self.a_filter_input1_content_scale_and_offset = *((a_filter_input1_content_scale_and_offset_attrib.data as *const u8).add(a_filter_input1_content_scale_and_offset_attrib.stride * instance as usize) as *const Vec4);
            
            let a_filter_input2_content_scale_and_offset_attrib = &*attribs[self.common.attrib_locations.a_filter_input2_content_scale_and_offset];
            self.a_filter_input2_content_scale_and_offset = *((a_filter_input2_content_scale_and_offset_attrib.data as *const u8).add(a_filter_input2_content_scale_and_offset_attrib.stride * instance as usize) as *const Vec4);
            
            let a_filter_input1_task_address_attrib = &*attribs[self.common.attrib_locations.a_filter_input1_task_address];
            self.a_filter_input1_task_address = *((a_filter_input1_task_address_attrib.data as *const u8).add(a_filter_input1_task_address_attrib.stride * instance as usize) as *const i32);
            
            let a_filter_input2_task_address_attrib = &*attribs[self.common.attrib_locations.a_filter_input2_task_address];
            self.a_filter_input2_task_address = *((a_filter_input2_task_address_attrib.data as *const u8).add(a_filter_input2_task_address_attrib.stride * instance as usize) as *const i32);
            
            let a_filter_kind_attrib = &*attribs[self.common.attrib_locations.a_filter_kind];
            self.a_filter_kind = *((a_filter_kind_attrib.data as *const u8).add(a_filter_kind_attrib.stride * instance as usize) as *const i32);
            
            let a_filter_input_count_attrib = &*attribs[self.common.attrib_locations.a_filter_input_count];
            self.a_filter_input_count = *((a_filter_input_count_attrib.data as *const u8).add(a_filter_input_count_attrib.stride * instance as usize) as *const i32);
            
            let a_filter_extra_data_address_attrib = &*attribs[self.common.attrib_locations.a_filter_extra_data_address];
            self.a_filter_extra_data_address = *((a_filter_extra_data_address_attrib.data as *const u8).add(a_filter_extra_data_address_attrib.stride * instance as usize) as *const ivec2);
        }
    }

    fn run_primitive(&mut self, context: &ShaderContext, interps: *mut u8, interp_stride: usize) {
        self.main(context);
        
        unsafe {
            let mut dest_ptr = interps as *mut InterpOutputs;
            for _ in 0..4 {
                (*dest_ptr).v_input1_uv = self.v_input1_uv;
                (*dest_ptr).v_input2_uv = self.v_input2_uv;
                dest_ptr = (dest_ptr as *mut u8).add(interp_stride) as *mut InterpOutputs;
            }
        }
    }
    
    fn set_uniform_1i(&mut self, _index: i32, _value: i32) {}
    fn set_uniform_4fv(&mut self, _index: i32, _value: &[f32; 4]) {}
    fn set_uniform_matrix4fv(&mut self, index: i32, value: &[f32; 16]) {
        if index == 6 { // uTransform
            self.common.u_transform = Mat4::from_cols_array(value);
        }
    }
}

//
// Fragment Shader
//

#[derive(Clone, Debug, Default)]
struct CsSvgFilterNodeFrag {
    vert: CsSvgFilterNodeVert,
    // Varying inputs
    v_input1_uv: vec2,
    v_input2_uv: vec2,
    interp_step: InterpOutputs,
    interp_perspective: InterpOutputs,
}

impl CsSvgFilterNodeFrag {
    fn main(&self, context: &ShaderContext) -> vec4 {
        let mut rs = vec4::ZERO;
        let mut ns = vec4::ZERO;

        if self.vert.common.v_filter_input_count_filter_kind_vec.x > 0 {
            rs = context.sample_in_uv_rect(SamplerId::SColor0, self.v_input1_uv, self.vert.common.v_input1_uv_rect);
            if rs.w != 0.0 {
                ns.truncate() = rs.truncate() / rs.w;
            }
            ns.w = rs.w;
            if (self.vert.common.v_filter_input_count_filter_kind_vec.y & 1) != 0 {
                ns.truncate() = srgb_to_linear(ns.truncate());
                rs.truncate() = ns.truncate() * rs.w;
            }
        }

        let mut rb = vec4::ZERO;
        let mut nb = vec4::ZERO;
        if self.vert.common.v_filter_input_count_filter_kind_vec.x > 1 {
             rb = context.sample_in_uv_rect(SamplerId::SColor1, self.v_input2_uv, self.vert.common.v_input2_uv_rect);
            if rb.w != 0.0 {
                nb.truncate() = rb.truncate() / rb.w;
            }
            nb.w = rb.w;
            if (self.vert.common.v_filter_input_count_filter_kind_vec.y & 1) != 0 {
                nb.truncate() = srgb_to_linear(nb.truncate());
                rb.truncate() = nb.truncate() * rb.w;
            }
        }
        
        let mut result = vec4(1.0, 0.0, 0.0, 1.0);
        let mut needs_premul = true;

        match self.vert.common.v_filter_input_count_filter_kind_vec.y {
            0 | 1 => { result = rs; },
            2 | 3 => { result = rs * self.vert.common.v_float0.x; },
            4 | 5 => { return vec4(0.0, 0.0, 0.0, rs.w); },
            6 | 7 => {
                result.truncate() = color_blend(nb.truncate(), ns.truncate());
                result.truncate() = (1.0 - rb.w) * rs.truncate() + (1.0 - rs.w) * rb.truncate() + (rs.w * rb.w) * result.truncate();
                result.w = rb.w * (1.0 - rs.w) + rs.w;
            },
            8 | 9 => {
                result.truncate() = vec3(
                    color_burn_blend(nb.x, ns.x),
                    color_burn_blend(nb.y, ns.y),
                    color_burn_blend(nb.z, ns.z)
                );
                result.truncate() = (1.0 - rb.w) * rs.truncate() + (1.0 - rs.w) * rb.truncate() + (rs.w * rb.w) * result.truncate();
                result.w = rb.w * (1.0 - rs.w) + rs.w;
            },
            10 | 11 => {
                result.truncate() = vec3(
                    color_dodge_blend(nb.x, ns.x),
                    color_dodge_blend(nb.y, ns.y),
                    color_dodge_blend(nb.z, ns.z)
                );
                result.truncate() = (1.0 - rb.w) * rs.truncate() + (1.0 - rs.w) * rb.truncate() + (rs.w * rb.w) * result.truncate();
                result.w = rb.w * (1.0 - rs.w) + rs.w;
            },
            12 | 13 => {
                result.truncate() = rs.truncate() + rb.truncate() - (rs.truncate() * rb.w).max(rb.truncate() * rs.w);
                result.w = rb.w * (1.0 - rs.w) + rs.w;
            },
            14 | 15 => {
                result.truncate() = rs.truncate() + rb.truncate() - 2.0 * (rs.truncate() * rb.w).min(rb.truncate() * rs.w);
                result.w = rb.w * (1.0 - rs.w) + rs.w;
            },
            16 | 17 => {
                result.truncate() = rs.truncate() + rb.truncate() - 2.0 * (rs.truncate() * rb.truncate());
                result.w = rb.w * (1.0 - rs.w) + rs.w;
            },
            18 | 19 => {
                result.truncate() = hard_light_blend(nb.truncate(), ns.truncate());
                result.truncate() = (1.0 - rb.w) * rs.truncate() + (1.0 - rs.w) * rb.truncate() + (rs.w * rb.w) * result.truncate();
                result.w = rb.w * (1.0 - rs.w) + rs.w;
            },
            20 | 21 => {
                result.truncate() = hue_blend(nb.truncate(), ns.truncate());
                result.truncate() = (1.0 - rb.w) * rs.truncate() + (1.0 - rs.w) * rb.truncate() + (rs.w * rb.w) * result.truncate();
                result.w = rb.w * (1.0 - rs.w) + rs.w;
            },
            22 | 23 => { // darken
                result.truncate() = (rs.truncate() * rb.w).min(rb.truncate() * rs.w);
                result.w = rb.w * (1.0 - rs.w) + rs.w;
            },
            24 | 25 => {
                result.truncate() = luminosity_blend(nb.truncate(), ns.truncate());
                result.truncate() = (1.0 - rb.w) * rs.truncate() + (1.0 - rs.w) * rb.truncate() + (rs.w * rb.w) * result.truncate();
                result.w = rb.w * (1.0 - rs.w) + rs.w;
            },
            26 | 27 => {
                result.truncate() = rs.truncate() * (1.0 - rb.w) + rb.truncate() * (1.0 - rs.w) + rs.truncate() * rb.truncate();
                result.w = rb.w * (1.0 - rs.w) + rs.w;
            },
            28 | 29 => {
                result = rb * (1.0 - rs.w) + rs;
                needs_premul = false;
            },
            30 | 31 => {
                result.truncate() = hard_light_blend(ns.truncate(), nb.truncate());
                result.truncate() = (1.0 - rb.w) * rs.truncate() + (1.0 - rs.w) * rb.truncate() + (rs.w * rb.w) * result.truncate();
                result.w = rb.w * (1.0 - rs.w) + rs.w;
            },
            32 | 33 => {
                result.truncate() = saturation_blend(nb.truncate(), ns.truncate());
                result.truncate() = (1.0 - rb.w) * rs.truncate() + (1.0 - rs.w) * rb.truncate() + (rs.w * rb.w) * result.truncate();
                result.w = rb.w * (1.0 - rs.w) + rs.w;
            },
            34 | 35 => {
                result.truncate() = rs.truncate() + rb.truncate() - (rs.truncate() * rb.truncate());
                result.w = rb.w * (1.0 - rs.w) + rs.w;
            },
            36 | 37 => {
                result.truncate() = vec3(soft_light_blend(nb.x, ns.x), soft_light_blend(nb.y, ns.y), soft_light_blend(nb.z, ns.z));
                result.truncate() = (1.0 - rb.w) * rs.truncate() + (1.0 - rs.w) * rb.truncate() + (rs.w * rb.w) * result.truncate();
                result.w = rb.w * (1.0 - rs.w) + rs.w;
            },
            38 | 39 => {
                result = self.vert.common.v_color_mat * ns + self.vert.common.v_filter_data0;
                result = result.clamp(vec4::ZERO, vec4::ONE);
            },
            // The logic for componentTransfer, composite etc. would go here, it's very extensive.
            _ => {},
        }
        
        if needs_premul {
            result.truncate() *= result.w;
        }

        result
    }
}

impl FragmentShader for CsSvgFilterNodeFrag {
    fn read_interp_inputs(&mut self, init: *const u8, step: *const u8) {
        unsafe {
            let init = &*(init as *const InterpOutputs);
            let step = &*(step as *const InterpOutputs);
            self.v_input1_uv = init.v_input1_uv;
            self.v_input2_uv = init.v_input2_uv;
            self.interp_step.v_input1_uv = step.v_input1_uv * 4.0;
            self.interp_step.v_input2_uv = step.v_input2_uv * 4.0;
        }
    }

    fn read_perspective_inputs(&mut self, init: *const u8, step: *const u8, w: f32) {
        unsafe {
            let init = &*(init as *const InterpOutputs);
            let step = &*(step as *const InterpOutputs);
            let inv_w = 1.0 / w;

            self.interp_perspective.v_input1_uv = init.v_input1_uv;
            self.v_input1_uv = self.interp_perspective.v_input1_uv * inv_w;
            self.interp_step.v_input1_uv = step.v_input1_uv * 4.0;

            self.interp_perspective.v_input2_uv = init.v_input2_uv;
            self.v_input2_uv = self.interp_perspective.v_input2_uv * inv_w;
            self.interp_step.v_input2_uv = step.v_input2_uv * 4.0;
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

impl CsSvgFilterNodeFrag {
    fn step_interp_inputs(&mut self, steps: i32) {
        let chunks = steps as f32 * 0.25;
        self.v_input1_uv += self.interp_step.v_input1_uv * chunks;
        self.v_input2_uv += self.interp_step.v_input2_uv * chunks;
    }

    fn step_perspective_inputs(&mut self, steps: i32, next_w: &[f32; 4]) {
        let chunks = steps as f32 * 0.25;
        let inv_w = 1.0 / next_w[0];
        
        self.interp_perspective.v_input1_uv += self.interp_step.v_input1_uv * chunks;
        self.v_input1_uv = self.interp_perspective.v_input1_uv * inv_w;

        self.interp_perspective.v_input2_uv += self.interp_step.v_input2_uv * chunks;
        self.v_input2_uv = self.interp_perspective.v_input2_uv * inv_w;
    }
}

//
// Program
//

#[derive(Clone, Debug, Default)]
pub struct CsSvgFilterNodeProgram {
    frag: CsSvgFilterNodeFrag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(CsSvgFilterNodeProgram::default())
}

impl Program for CsSvgFilterNodeProgram {
    fn get_vertex_shader(&mut self) -> &mut dyn VertexShader {
        &mut self.frag.vert
    }

    fn get_fragment_shader(&mut self) -> &mut dyn FragmentShader {
        &mut self.frag
    }
    
    fn get_uniform(&self, name: &CStr) -> i32 {
        if strcmp(name, "sClipMask") { return 7; }
        if strcmp(name, "sColor0") { return 8; }
        if strcmp(name, "sColor1") { return 9; }
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
