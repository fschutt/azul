use crate::common::*;
use crate::common_types::*;
use crate::traits::{FragmentShader, Program, VertexShader};
use glam::{ivec2, ivec4, mat4, vec2, vec3, vec4, Mat4, Vec2, Vec3, Vec4};
use std::ffi::CStr;
use std::mem;
use super::strcmp;

//
// Common Struct
//

#[derive(Clone, Debug, Default)]
struct BrushBlendCommon {
    attrib_locations: AttribLocations,
    u_transform: mat4,
    // Flat varyings (set in VS, used in FS)
    v_uv_sample_bounds: vec4,
    v_perspective_amount: vec2,
    v_op_table_address_vec: ivec2,
    v_color_mat: mat4,
    v_funcs: vec4,
    v_color_offset: vec4,
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
            if self.a_position != NULL_ATTRIB {
                self.a_position as i32
            } else {
                -1
            }
        } else if strcmp(name, "aData") {
            if self.a_data != NULL_ATTRIB {
                self.a_data as i32
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
struct BrushBlendVert {
    common: BrushBlendCommon,
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

impl BrushBlendVert {
    fn main(&mut self, context: &mut ShaderContext) {
        let instance = context.decode_instance_attributes(self.a_data);
        let ph = context.fetch_prim_header(instance.prim_header_address);
        let transform = context.fetch_transform(ph.transform_id);
        let task = context.fetch_picture_task(ph.picture_task_address);
        let clip_area = context.fetch_clip_area(instance.clip_address);
        self.brush_shader_main_vs(context, instance, ph, transform, task, clip_area);
    }

    fn brush_shader_main_vs(
        &mut self,
        context: &mut ShaderContext,
        instance: Instance,
        mut ph: PrimitiveHeader,
        transform: Transform,
        task: PictureTask,
        clip_area: ClipArea,
    ) {
        let edge_flags = (instance.flags >> 12) & 15;
        let brush_flags = instance.flags & 4095;
        let (segment_rect, segment_data) = if instance.segment_index == 65535 {
            (ph.local_rect, vec4::ZERO)
        } else {
            let segment_address = ph.specific_prim_address + 3 + (instance.segment_index * 2);
            let segment_info = context.fetch_from_gpu_cache_2(segment_address);
            let mut rect = RectWithEndpoint {
                p0: segment_info[0].xy(),
                p1: segment_info[0].zw(),
            };
            rect.p0 += ph.local_rect.p0;
            rect.p1 += ph.local_rect.p0;
            (rect, segment_info[1])
        };

        let mut adjusted_segment_rect = segment_rect;
        let antialiased = !transform.is_axis_aligned || (brush_flags & 1024) != 0;

        if antialiased {
            adjusted_segment_rect = context.clip_and_init_antialiasing(
                segment_rect,
                ph.local_rect,
                ph.local_clip_rect,
                edge_flags,
                ph.z,
                &transform,
                &task,
            );
            ph.local_clip_rect.p0 = vec2::splat(-1.0e16);
            ph.local_clip_rect.p1 = vec2::splat(1.0e16);
        }

        let local_pos =
            adjusted_segment_rect.p0.lerp(adjusted_segment_rect.p1, self.a_position);

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

        self.brush_vs(
            context,
            vi,
            ph.local_rect,
            ph.user_data,
            brush_flags,
        );
    }

    fn brush_vs(
        &mut self,
        context: &ShaderContext,
        vi: VertexInfo,
        local_rect: RectWithEndpoint,
        prim_user_data: ivec4,
        brush_flags: i32,
    ) {
        let (res_uv_rect, _res_user_data) = context.fetch_image_source(prim_user_data.x);
        let uv0 = res_uv_rect.p0;
        let uv1 = res_uv_rect.p1;
        let inv_texture_size = vec2::ONE / context.texture_size(SamplerId::SColor0, 0);

        let mut f = (vi.local_pos - local_rect.p0) / context.rect_size(local_rect);
        f = context.get_image_quad_uv(prim_user_data.x, f);
        let uv = uv0.lerp(uv1, f);

        let perspective_interpolate = if (brush_flags & 1) != 0 { 1.0 } else { 0.0 };
        self.v_uv = (uv * inv_texture_size) * vi.world_pos.w.lerp(1.0, perspective_interpolate);
        self.common.v_perspective_amount.x = perspective_interpolate;

        self.common.v_uv_sample_bounds =
            vec4(uv0.x + 0.5, uv0.y + 0.5, uv1.x - 0.5, uv1.y - 0.5)
                * inv_texture_size.extend(inv_texture_size).xyxy();

        let amount = (prim_user_data.z as f32) / 65536.0;
        self.common.v_op_table_address_vec.x = prim_user_data.y & 0xFFFF;
        self.common.v_perspective_amount.y = amount;
        self.common.v_funcs.x = (((prim_user_data.y >> 28) & 15)) as f32;
        self.common.v_funcs.y = (((prim_user_data.y >> 24) & 15)) as f32;
        self.common.v_funcs.z = (((prim_user_data.y >> 20) & 15)) as f32;
        self.common.v_funcs.w = (((prim_user_data.y >> 16) & 15)) as f32;

        self.setup_filter_params(
            context,
            self.common.v_op_table_address_vec.x,
            amount,
            prim_user_data.z,
        );
    }

    fn setup_filter_params(
        &mut self,
        context: &ShaderContext,
        op: i32,
        amount: f32,
        gpu_data_address: i32,
    ) {
        let lum_r = 0.2126;
        let lum_g = 0.7152;
        let lum_b = 0.0722;
        let one_minus_lum_r = 1.0 - lum_r;
        let one_minus_lum_g = 1.0 - lum_g;
        let one_minus_lum_b = 1.0 - lum_b;
        let inv_amount = 1.0 - amount;

        match op {
            1 => { // SATURATE
                self.common.v_color_mat = mat4(
                    vec4::new(lum_r + one_minus_lum_r * inv_amount, lum_r - lum_r * inv_amount, lum_r - lum_r * inv_amount, 0.0),
                    vec4::new(lum_g - lum_g * inv_amount, lum_g + one_minus_lum_g * inv_amount, lum_g - lum_g * inv_amount, 0.0),
                    vec4::new(lum_b - lum_b * inv_amount, lum_b - lum_b * inv_amount, lum_b + one_minus_lum_b * inv_amount, 0.0),
                    vec4::new(0.0, 0.0, 0.0, 1.0),
                );
                self.common.v_color_offset = vec4::ZERO;
            }
            2 => { // HUE_ROTATE
                let c = amount.cos();
                let s = amount.sin();
                self.common.v_color_mat = mat4(
                    vec4::new(lum_r + one_minus_lum_r * c - lum_r * s, lum_r - lum_r * c + 0.143 * s, lum_r - lum_r * c - one_minus_lum_r * s, 0.0),
                    vec4::new(lum_g - lum_g * c - lum_g * s, lum_g + one_minus_lum_g * c + 0.140 * s, lum_g - lum_g * c + lum_g * s, 0.0),
                    vec4::new(lum_b - lum_b * c + one_minus_lum_b * s, lum_b - lum_b * c - 0.283 * s, lum_b + one_minus_lum_b * c + lum_b * s, 0.0),
                    vec4::new(0.0, 0.0, 0.0, 1.0),
                );
                self.common.v_color_offset = vec4::ZERO;
            }
            4 => { // LUMINANCE_TO_ALPHA
                self.common.v_color_mat = mat4(
                    vec4::new(inv_amount * lum_r + amount, inv_amount * lum_r, inv_amount * lum_r, 0.0),
                    vec4::new(inv_amount * lum_g, inv_amount * lum_g + amount, inv_amount * lum_g, 0.0),
                    vec4::new(inv_amount * lum_b, inv_amount * lum_b, inv_amount * lum_b + amount, 0.0),
                    vec4::new(0.0, 0.0, 0.0, 1.0),
                );
                self.common.v_color_offset = vec4::ZERO;
            }
            5 => { // SEPIA
                self.common.v_color_mat = mat4(
                    vec4::new(0.393 + 0.607 * inv_amount, 0.349 - 0.349 * inv_amount, 0.272 - 0.272 * inv_amount, 0.0),
                    vec4::new(0.769 - 0.769 * inv_amount, 0.686 + 0.314 * inv_amount, 0.534 - 0.534 * inv_amount, 0.0),
                    vec4::new(0.189 - 0.189 * inv_amount, 0.168 - 0.168 * inv_amount, 0.131 + 0.869 * inv_amount, 0.0),
                    vec4::new(0.0, 0.0, 0.0, 1.0),
                );
                self.common.v_color_offset = vec4::ZERO;
            }
            7 => { // COLOR_MATRIX
                let mat_data = context.fetch_from_gpu_cache_4(gpu_data_address);
                self.common.v_color_mat = Mat4::from_cols(mat_data[0], mat_data[1], mat_data[2], mat_data[3]);
                self.common.v_color_offset = context.fetch_from_gpu_cache_1(gpu_data_address + 4);
            }
            11 => { // COMPONENT_TRANSFER
                self.common.v_op_table_address_vec.y = gpu_data_address;
            }
            10 => { // FLOOD
                self.common.v_color_offset = context.fetch_from_gpu_cache_1(gpu_data_address);
            }
            _ => {}
        }
    }
}

impl VertexShader for BrushBlendVert {
    fn init_batch(&mut self, _context: &mut ShaderContext) {}

    fn load_attribs(
        &mut self,
        attribs: &[&VertexAttrib],
        start: u32,
        instance: i32,
        _count: i32,
    ) {
        unsafe {
            let a_pos_attrib = &*attribs[self.common.attrib_locations.a_position];
            let a_data_attrib = &*attribs[self.common.attrib_locations.a_data];
            let pos_ptr =
                (a_pos_attrib.data as *const u8).add(a_pos_attrib.stride * start as usize) as *const Vec2;
            self.a_position = *pos_ptr;
            let data_ptr = (a_data_attrib.data as *const u8)
                .add(a_data_attrib.stride * instance as usize)
                as *const ivec4;
            self.a_data = *data_ptr;
        }
    }

    fn run_primitive(
        &mut self,
        context: &mut ShaderContext,
        interps: *mut u8,
        interp_stride: usize,
    ) {
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
struct BrushBlendFrag {
    vert: BrushBlendVert,
    // Varying inputs
    v_uv: vec2,
    interp_step: InterpOutputs,
    interp_perspective: InterpOutputs,
}

impl BrushBlendFrag {
    fn main(&self, context: &ShaderContext) -> vec4 {
        let gl_frag_coord_w = 1.0; // Placeholder

        let perspective_divisor = (1.0 - self.vert.common.v_perspective_amount.x) * gl_frag_coord_w + self.vert.common.v_perspective_amount.x;
        let uv = self.v_uv * perspective_divisor;
        let uv_clamped = uv.clamp(
            self.vert.common.v_uv_sample_bounds.xy(),
            self.vert.common.v_uv_sample_bounds.zw(),
        );
        let cs = context.texture(SamplerId::SColor0, uv_clamped);
        
        let (mut color, mut alpha) = self.calculate_filter(
            cs,
            self.vert.common.v_op_table_address_vec.x,
            self.vert.common.v_perspective_amount.y,
            self.vert.common.v_op_table_address_vec.y,
            context,
        );
        
        vec4::new(color.x, color.y, color.z, 1.0) * alpha
    }

    fn calculate_filter(&self, cs: vec4, op: i32, amount: f32, table_address: i32, context: &ShaderContext) -> (Vec3, f32) {
        let mut alpha = cs.w;
        let mut color = if alpha != 0.0 { cs.xyz() / alpha } else { cs.xyz() };

        match op {
            0 => color = contrast(color, amount),
            3 => color = invert(color, amount),
            6 => color = brightness(color, amount),
            8 => color = srgb_to_linear(color),
            9 => color = linear_to_srgb(color),
            11 => {
                let mut colora = vec4::new(color.x, color.y, color.z, alpha);
                colora = self.component_transfer(colora, table_address, context);
                color = colora.xyz();
                alpha = colora.w;
            }
            10 => {
                color = self.vert.common.v_color_offset.xyz();
                alpha = self.vert.common.v_color_offset.w;
            }
            _ => { // Includes MATRIX(7), SATURATE(1), HUE_ROTATE(2), SEPIA(5)
                let result = (self.vert.common.v_color_mat * vec4::new(color.x, color.y, color.z, alpha)) + self.vert.common.v_color_offset;
                let clamped = result.clamp(vec4::ZERO, vec4::ONE);
                color = clamped.xyz();
                alpha = clamped.w;
            }
        }
        (color, alpha)
    }
    
    fn component_transfer(&self, mut colora: vec4, table_address: i32, context: &ShaderContext) -> vec4 {
        let mut offset = 0;
        let funcs = [
            self.vert.common.v_funcs.x as i32,
            self.vert.common.v_funcs.y as i32,
            self.vert.common.v_funcs.z as i32,
            self.vert.common.v_funcs.w as i32,
        ];
        
        for i in 0..4 {
            match funcs[i] {
                0 => {}, // identity
                1 | 2 => { // table or discrete
                    let k = (colora[i] * 255.0 + 0.5).floor() as i32;
                    let texel = context.fetch_from_gpu_cache_1(table_address + offset + (k / 4));
                    colora[i] = texel[k as usize % 4].clamp(0.0, 1.0);
                    offset += 64;
                },
                3 => { // linear
                    let texel = context.fetch_from_gpu_cache_1(table_address + offset);
                    colora[i] = (texel.x * colora[i] + texel.y).clamp(0.0, 1.0);
                    offset += 1;
                },
                4 => { // gamma
                    let texel = context.fetch_from_gpu_cache_1(table_address + offset);
                    colora[i] = (texel.x * colora[i].powf(texel.y) + texel.z).clamp(0.0, 1.0);
                    offset += 1;
                },
                _ => {}
            }
        }
        colora
    }
}

// Filter helper functions
fn contrast(cs: vec3, amount: f32) -> vec3 {
    (cs * amount - (0.5 * amount) + 0.5).clamp(vec3::ZERO, vec3::ONE)
}

fn invert(cs: vec3, amount: f32) -> vec3 {
    cs.lerp(vec3::ONE - cs, amount)
}

fn brightness(cs: vec3, amount: f32) -> vec3 {
    (cs * amount).clamp(vec3::ZERO, vec3::ONE)
}

fn srgb_to_linear(color: vec3) -> vec3 {
    let c1 = color / 12.92;
    let c2 = ((color / 1.055) + (0.055 / 1.055)).powf(2.4);
    c1.lerp(c2, color.cmpge(vec3::splat(0.04045)).as_vec3())
}

fn linear_to_srgb(color: vec3) -> vec3 {
    let c1 = color * 12.92;
    let c2 = (1.055 * color.powf(1.0 / 2.4)) - 0.055;
    c1.lerp(c2, color.cmpge(vec3::splat(0.0031308)).as_vec3())
}


impl FragmentShader for BrushBlendFrag {
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
    
    // No fast path for this complex shader
    fn draw_span_rgba8(&mut self, _context: &mut ShaderContext) -> i32 {
        0
    }
}

impl BrushBlendFrag {
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
pub struct BrushBlendProgram {
    frag: BrushBlendFrag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(BrushBlendProgram::default())
}

impl Program for BrushBlendProgram {
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