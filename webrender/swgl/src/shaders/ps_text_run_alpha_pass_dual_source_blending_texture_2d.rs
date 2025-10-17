use crate::common::*;
use crate::common_types::*;
use crate::traits::{FragmentShader, Program, VertexShader};
use glam::{ivec4, mat4, vec2, vec3, vec4, Mat4, Vec2, Vec3, Vec4};
use std::ffi::CStr;
use std::mem;
use super::strcmp;

//
// Common Struct
//

#[derive(Clone, Debug, Default)]
struct PsTextRunAlphaPassDualSourceBlendingTexture2DCommon {
    attrib_locations: AttribLocations,
    u_transform: mat4,
    // Flat varyings
    v_color: vec4,
    v_mask_swizzle: vec3,
    v_uv_bounds: vec4,
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
struct PsTextRunAlphaPassDualSourceBlendingTexture2DVert {
    common: PsTextRunAlphaPassDualSourceBlendingTexture2DCommon,
    // Inputs
    a_position: vec2,
    a_data: ivec4,
    // Outputs
    v_uv: vec2,
    gl_position: vec4,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct InterpOutputs {
    v_uv: vec2,
}

impl PsTextRunAlphaPassDualSourceBlendingTexture2DVert {
    fn main(&mut self, context: &mut ShaderContext) {
        let instance = context.decode_instance_attributes(self.a_data);
        let ph = context.fetch_prim_header(instance.prim_header_address);
        let transform = context.fetch_transform(ph.transform_id);
        let task = context.fetch_picture_task(ph.picture_task_address);
        let clip_area = context.fetch_clip_area(instance.clip_address);

        let glyph_index = instance.segment_index;
        let color_mode = instance.flags & 255;

        let text = context.fetch_text_run(ph.specific_prim_address);
        let text_offset = ph.local_rect.p1;
        let glyph = context.fetch_glyph(ph.specific_prim_address, glyph_index);
        let (res_uv_rect, res_offset, _res_scale) = context.fetch_glyph_resource(instance.resource_address);

        let glyph_origin = glyph.offset + text_offset + res_offset;
        let glyph_rect = RectWithEndpoint {
            p0: glyph_origin,
            p1: glyph_origin + (res_uv_rect.zw() - res_uv_rect.xy()),
        };

        let local_pos = glyph_rect.p0.lerp(glyph_rect.p1, self.a_position);

        let (gl_pos, vi) = context.write_vertex(
            local_pos,
            ph.local_clip_rect,
            ph.z,
            &transform,
            &task,
            &self.common.u_transform,
        );
        self.gl_position = gl_pos;
        
        context.write_clip(vi.world_pos, &clip_area, &task);
        
        match color_mode {
            0 => {
                self.common.v_mask_swizzle = vec3(0.0, 1.0, 1.0);
                self.common.v_color = text.color;
            },
            2 => {
                // swgl_blendDropShadow(text.color);
                self.common.v_mask_swizzle = vec3(1.0, 0.0, 0.0);
                self.common.v_color = vec4::ONE;
            },
            3 => {
                self.common.v_mask_swizzle = vec3(1.0, 0.0, 0.0);
                self.common.v_color = vec4::splat(text.color.w);
            },
            1 => {
                // swgl_blendSubpixelText(text.color);
                self.common.v_mask_swizzle = vec3(1.0, 0.0, 0.0);
                self.common.v_color = vec4::ONE;
            },
            _ => {
                self.common.v_mask_swizzle = vec3::ZERO;
                self.common.v_color = vec4::ONE;
            }
        }

        let texture_size = context.texture_size(SamplerId::SColor0, 0);
        let f = (vi.local_pos - glyph_rect.p0) / context.rect_size(glyph_rect);
        let st0 = res_uv_rect.xy() / texture_size;
        let st1 = res_uv_rect.zw() / texture_size;
        self.v_uv = st0.lerp(st1, f);
        self.common.v_uv_bounds = (res_uv_rect + vec4(0.5, 0.5, -0.5, -0.5)) / texture_size.extend(texture_size).xyxy();
    }
}

impl VertexShader for PsTextRunAlphaPassDualSourceBlendingTexture2DVert {
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
struct PsTextRunAlphaPassDualSourceBlendingTexture2DFrag {
    vert: PsTextRunAlphaPassDualSourceBlendingTexture2DVert,
    v_uv: vec2,
    interp_step: InterpOutputs,
    interp_perspective: InterpOutputs,
}

impl PsTextRunAlphaPassDualSourceBlendingTexture2DFrag {
    fn text_fs(&self, context: &ShaderContext) -> (vec4, vec4) {
        let tc = self.v_uv.clamp(self.vert.common.v_uv_bounds.xy(), self.vert.common.v_uv_bounds.zw());
        let mut mask = context.texture(SamplerId::SColor0, tc);

        if self.vert.common.v_mask_swizzle.z != 0.0 {
            mask = vec4::splat(mask.x);
        }
        
        let alpha_mask = mask;
        let color = self.vert.common.v_color * alpha_mask;
        let blend = (alpha_mask * self.vert.common.v_mask_swizzle.x) + (vec4::splat(alpha_mask.a) * self.vert.common.v_mask_swizzle.y);

        (color, blend)
    }

    fn do_clip(&self, _context: &ShaderContext) -> f32 {
        1.0
    }
    
    fn main(&self, context: &ShaderContext) -> (vec4, vec4) {
        let (mut color, mut blend) = self.text_fs(context);
        let clip_alpha = self.do_clip(context);
        color *= clip_alpha;
        blend *= clip_alpha;
        (color, blend)
    }
}

impl FragmentShader for PsTextRunAlphaPassDualSourceBlendingTexture2DFrag {
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
        let (color, blend) = self.main(context);
        context.write_output(color);
        context.write_secondary_output(blend);
        self.step_interp_inputs(4);
    }
    
    fn skip(&mut self, steps: i32) {
        self.step_interp_inputs(steps);
    }

    fn run_perspective(&mut self, context: &mut ShaderContext, next_w: &[f32; 4]) {
        let (color, blend) = self.main(context);
        context.write_output(color);
        context.write_secondary_output(blend);
        self.step_perspective_inputs(4, next_w);
    }

    fn skip_perspective(&mut self, steps: i32, next_w: &[f32; 4]) {
        self.step_perspective_inputs(steps, next_w);
    }
    
    fn draw_span_rgba8(&mut self, context: &mut ShaderContext) -> i32 {
        if self.vert.common.v_mask_swizzle.x != 0.0 && self.vert.common.v_mask_swizzle.x != 1.0 {
            return 0;
        }

        // swgl_isTextureR8 is a runtime check. The logic in rust would be:
        // if context.is_texture_r8(SamplerId::SColor0) { ... }
        if false {
            context.commit_texture_linear_color_r8_to_rgba8(SamplerId::SColor0, self.v_uv, self.vert.common.v_uv_bounds, self.vert.common.v_color);
        } else {
            context.commit_texture_linear_color_rgba8(SamplerId::SColor0, self.v_uv, self.vert.common.v_uv_bounds, self.vert.common.v_color);
        }
        1
    }
}

impl PsTextRunAlphaPassDualSourceBlendingTexture2DFrag {
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
pub struct PsTextRunAlphaPassDualSourceBlendingTexture2DProgram {
    frag: PsTextRunAlphaPassDualSourceBlendingTexture2DFrag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(PsTextRunAlphaPassDualSourceBlendingTexture2DProgram::default())
}

impl Program for PsTextRunAlphaPassDualSourceBlendingTexture2DProgram {
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