use crate::common::*;
use crate::common_types::*;
use crate::traits::{FragmentShader, Program, VertexShader};
use glam::{ivec2, ivec4, mat3, mat4, vec2, vec3, vec4, Mat3, Mat4, Vec2, Vec3, Vec4};
use std::ffi::CStr;
use std::mem;
use super::strcmp;

//
// C++ Constant Definitions
//
const RGB_FROM_YUV_REC601: Mat3 = Mat3::from_cols_array(&[1.0, 1.0, 1.0, 0.0, -0.17207, 0.886, 0.701, -0.35707, 0.0]);
const RGB_FROM_YUV_REC709: Mat3 = Mat3::from_cols_array(&[1.0, 1.0, 1.0, 0.0, -0.09366, 0.9278, 0.7874, -0.23406, 0.0]);
const RGB_FROM_YUV_REC2020: Mat3 = Mat3::from_cols_array(&[1.0, 1.0, 1.0, 0.0, -0.08228, 0.9407, 0.7373, -0.28568, 0.0]);
const RGB_FROM_YUV_GBR_IDENTITY: Mat3 = Mat3::from_cols_array(&[0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0]);

// Helper structs from C++
#[derive(Clone, Copy, Debug, Default)]
struct YuvPrimitive {
    channel_bit_depth: i32,
    color_space: i32,
    yuv_format: i32,
}

#[derive(Clone, Copy, Debug, Default)]
struct YuvColorSamplingInfo {
    rgb_from_yuv: mat3,
    packed_zero_one_vals: vec4,
}

#[derive(Clone, Copy, Debug, Default)]
struct YuvColorMatrixInfo {
    ycbcr_bias: vec3,
    rgb_from_debiased_ycbrc: mat3,
}

//
// Common Struct
//

#[derive(Clone, Debug, Default)]
struct BrushYuvImageAlphaPassTexture2DYuvCommon {
    attrib_locations: AttribLocations,
    u_transform: mat4,
    // Flat varyings
    v_uv_bounds_y: vec4,
    v_uv_bounds_u: vec4,
    v_uv_bounds_v: vec4,
    v_ycbcr_bias: vec3,
    v_rgb_from_debiased_ycbcr: mat3,
    v_format: ivec2,
    v_rescale_factor: i32,
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
struct BrushYuvImageAlphaPassTexture2DYuvVert {
    common: BrushYuvImageAlphaPassTexture2DYuvCommon,
    // Inputs
    a_position: vec2,
    a_data: ivec4,
    // Outputs
    v_uv_y: vec2,
    v_uv_u: vec2,
    v_uv_v: vec2,
    gl_position: vec4,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct InterpOutputs {
    v_uv_y: vec2,
    v_uv_u: vec2,
    v_uv_v: vec2,
}

impl BrushYuvImageAlphaPassTexture2DYuvVert {
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
            let segment_address = ph.specific_prim_address + 1 + (instance.segment_index * 2);
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
            adjusted_segment_rect = context.clip_and_init_antialiasing(segment_rect, ph.local_rect, ph.local_clip_rect, edge_flags, ph.z, &transform, &task);
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
        
        self.brush_vs(context, vi, ph.specific_prim_address, ph.local_rect, ph.user_data, brush_flags, segment_data);
    }

    fn brush_vs(
        &mut self,
        context: &ShaderContext,
        vi: VertexInfo,
        prim_address: i32,
        local_rect: RectWithEndpoint,
        prim_user_data: ivec4,
        _brush_flags: i32,
        _segment_data: vec4,
    ) {
        let f = (vi.local_pos - local_rect.p0) / context.rect_size(local_rect);
        let prim = self.fetch_yuv_primitive(prim_address, context);
        
        self.common.v_rescale_factor = 0;
        if prim.channel_bit_depth > 8 && prim.yuv_format != 1 {
            self.common.v_rescale_factor = 16 - prim.channel_bit_depth;
        }

        let mat_info = self.get_rgb_from_ycbcr_info(prim);
        self.common.v_ycbcr_bias = mat_info.ycbcr_bias;
        self.common.v_rgb_from_debiased_ycbcr = mat_info.rgb_from_debiased_ycbrc;
        self.common.v_format.x = prim.yuv_format;

        if prim.yuv_format == 3 || prim.yuv_format == 99 {
            let (res_y_uv, _) = context.fetch_image_source(prim_user_data.x);
            let (res_u_uv, _) = context.fetch_image_source(prim_user_data.y);
            let (res_v_uv, _) = context.fetch_image_source(prim_user_data.z);

            self.write_uv_rect(res_y_uv.p0, res_y_uv.p1, f, context.texture_size(SamplerId::SColor0, 0), &mut self.v_uv_y, &mut self.common.v_uv_bounds_y);
            self.write_uv_rect(res_u_uv.p0, res_u_uv.p1, f, context.texture_size(SamplerId::SColor1, 0), &mut self.v_uv_u, &mut self.common.v_uv_bounds_u);
            self.write_uv_rect(res_v_uv.p0, res_v_uv.p1, f, context.texture_size(SamplerId::SColor2, 0), &mut self.v_uv_v, &mut self.common.v_uv_bounds_v);
        } else if prim.yuv_format == 0 || prim.yuv_format == 1 {
            let (res_y_uv, _) = context.fetch_image_source(prim_user_data.x);
            let (res_u_uv, _) = context.fetch_image_source(prim_user_data.y);
            self.write_uv_rect(res_y_uv.p0, res_y_uv.p1, f, context.texture_size(SamplerId::SColor0, 0), &mut self.v_uv_y, &mut self.common.v_uv_bounds_y);
            self.write_uv_rect(res_u_uv.p0, res_u_uv.p1, f, context.texture_size(SamplerId::SColor1, 0), &mut self.v_uv_u, &mut self.common.v_uv_bounds_u);
        } else if prim.yuv_format == 4 {
            let (res_y_uv, _) = context.fetch_image_source(prim_user_data.x);
            self.write_uv_rect(res_y_uv.p0, res_y_uv.p1, f, context.texture_size(SamplerId::SColor0, 0), &mut self.v_uv_y, &mut self.common.v_uv_bounds_y);
        }
    }

    fn write_uv_rect(&self, uv0: vec2, uv1: vec2, f: vec2, texture_size: vec2, uv: &mut vec2, uv_bounds: &mut vec4) {
        *uv = uv0.lerp(uv1, f);
        *uv_bounds = vec4(uv0.x + 0.5, uv0.y + 0.5, uv1.x - 0.5, uv1.y - 0.5);
        *uv /= texture_size;
        *uv_bounds /= texture_size.extend(texture_size).xyxy();
    }
    
    fn fetch_yuv_primitive(&self, address: i32, context: &ShaderContext) -> YuvPrimitive {
        let data = context.fetch_from_gpu_cache_1(address);
        YuvPrimitive {
            channel_bit_depth: data.x as i32,
            color_space: data.y as i32,
            yuv_format: data.z as i32,
        }
    }
    
    fn get_rgb_from_ycbcr_info(&self, prim: YuvPrimitive) -> YuvColorMatrixInfo {
        let info = self.get_yuv_color_info(prim);
        let zero = info.packed_zero_one_vals.xy();
        let one = info.packed_zero_one_vals.zw();
        let scale = vec2::ONE / (one - zero);
        
        let yuv_from_debiased_ycbcr = mat3(
            vec3::new(scale.x, 0.0, 0.0),
            vec3::new(0.0, scale.y, 0.0),
            vec3::new(0.0, 0.0, scale.y),
        );
        
        YuvColorMatrixInfo {
            ycbcr_bias: vec3::new(zero.x, zero.y, zero.y),
            rgb_from_debiased_ycbrc: info.rgb_from_yuv * yuv_from_debiased_ycbcr,
        }
    }

    fn get_yuv_color_info(&self, prim: YuvPrimitive) -> YuvColorSamplingInfo {
        let mut channel_max = 255.0;
        if prim.channel_bit_depth > 8 {
            if prim.yuv_format == 1 {
                channel_max = ((1 << prim.channel_bit_depth) - 1) as f32;
            } else {
                channel_max = 65535.0;
            }
        }
        
        match prim.color_space {
            0 => YuvColorSamplingInfo { rgb_from_yuv: RGB_FROM_YUV_REC601, packed_zero_one_vals: self.yuv_channel_zero_one_narrow_range(prim.channel_bit_depth, channel_max) },
            1 => YuvColorSamplingInfo { rgb_from_yuv: RGB_FROM_YUV_REC601, packed_zero_one_vals: self.yuv_channel_zero_one_full_range(prim.channel_bit_depth, channel_max) },
            2 => YuvColorSamplingInfo { rgb_from_yuv: RGB_FROM_YUV_REC709, packed_zero_one_vals: self.yuv_channel_zero_one_narrow_range(prim.channel_bit_depth, channel_max) },
            3 => YuvColorSamplingInfo { rgb_from_yuv: RGB_FROM_YUV_REC709, packed_zero_one_vals: self.yuv_channel_zero_one_full_range(prim.channel_bit_depth, channel_max) },
            4 => YuvColorSamplingInfo { rgb_from_yuv: RGB_FROM_YUV_REC2020, packed_zero_one_vals: self.yuv_channel_zero_one_narrow_range(prim.channel_bit_depth, channel_max) },
            5 => YuvColorSamplingInfo { rgb_from_yuv: RGB_FROM_YUV_REC2020, packed_zero_one_vals: self.yuv_channel_zero_one_full_range(prim.channel_bit_depth, channel_max) },
            _ => YuvColorSamplingInfo { rgb_from_yuv: RGB_FROM_YUV_GBR_IDENTITY, packed_zero_one_vals: self.yuv_channel_zero_one_identity(prim.channel_bit_depth, channel_max) },
        }
    }
    
    fn yuv_channel_zero_one_narrow_range(&self, bit_depth: i32, channel_max: f32) -> vec4 {
        let zero_one_ints = ivec4::new(16, 128, 235, 240) << (bit_depth - 8);
        zero_one_ints.as_vec4() / channel_max
    }
    
    fn yuv_channel_zero_one_full_range(&self, bit_depth: i32, channel_max: f32) -> vec4 {
        let narrow = self.yuv_channel_zero_one_narrow_range(bit_depth, channel_max);
        let identity = self.yuv_channel_zero_one_identity(bit_depth, channel_max);
        vec4::new(0.0, narrow.y, identity.z, identity.w)
    }

    fn yuv_channel_zero_one_identity(&self, bit_depth: i32, channel_max: f32) -> vec4 {
        let all_ones_normalized = ((1 << bit_depth) - 1) as f32 / channel_max;
        vec4::new(0.0, 0.0, all_ones_normalized, all_ones_normalized)
    }
}

impl VertexShader for BrushYuvImageAlphaPassTexture2DYuvVert {
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
                (*dest_ptr).v_uv_y = self.v_uv_y;
                (*dest_ptr).v_uv_u = self.v_uv_u;
                (*dest_ptr).v_uv_v = self.v_uv_v;
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
struct BrushYuvImageAlphaPassTexture2DYuvFrag {
    vert: BrushYuvImageAlphaPassTexture2DYuvVert,
    // Varying inputs from rasterizer
    v_uv_y: vec2,
    v_uv_u: vec2,
    v_uv_v: vec2,
    interp_step: InterpOutputs,
    interp_perspective: InterpOutputs,
}

impl BrushYuvImageAlphaPassTexture2DYuvFrag {
    fn sample_yuv(&self, context: &ShaderContext, in_uv_y: vec2, in_uv_u: vec2, in_uv_v: vec2) -> vec4 {
        let mut ycbcr_sample = vec3::ZERO;
        
        match self.vert.common.v_format.x {
            3 => { // YUV444, planar
                let uv_y = in_uv_y.clamp(self.vert.common.v_uv_bounds_y.xy(), self.vert.common.v_uv_bounds_y.zw());
                let uv_u = in_uv_u.clamp(self.vert.common.v_uv_bounds_u.xy(), self.vert.common.v_uv_bounds_u.zw());
                let uv_v = in_uv_v.clamp(self.vert.common.v_uv_bounds_v.xy(), self.vert.common.v_uv_bounds_v.zw());
                ycbcr_sample.x = context.texture(SamplerId::SColor0, uv_y).x;
                ycbcr_sample.y = context.texture(SamplerId::SColor1, uv_u).x;
                ycbcr_sample.z = context.texture(SamplerId::SColor2, uv_v).x;
            },
            0 | 1 | 2 => { // NV12, etc. (Y plane and UV plane)
                let uv_y = in_uv_y.clamp(self.vert.common.v_uv_bounds_y.xy(), self.vert.common.v_uv_bounds_y.zw());
                let uv_uv = in_uv_u.clamp(self.vert.common.v_uv_bounds_u.xy(), self.vert.common.v_uv_bounds_u.zw());
                ycbcr_sample.x = context.texture(SamplerId::SColor0, uv_y).x;
                ycbcr_sample.yz() = context.texture(SamplerId::SColor1, uv_uv).rg();
            },
            4 => { // GBR_IDENTITY special case
                let uv_y = in_uv_y.clamp(self.vert.common.v_uv_bounds_y.xy(), self.vert.common.v_uv_bounds_y.zw());
                ycbcr_sample = context.texture(SamplerId::SColor0, uv_y).gbr();
            },
            _ => { // Should not happen
                ycbcr_sample = vec3::ZERO;
            }
        }
        
        let rgb = self.vert.common.v_rgb_from_debiased_ycbcr * (ycbcr_sample - self.vert.common.v_ycbcr_bias);
        vec4::new(rgb.x, rgb.y, rgb.z, 1.0).clamp(vec4::ZERO, vec4::ONE)
    }

    fn main(&self, context: &ShaderContext) -> vec4 {
        let mut color = self.sample_yuv(context, self.v_uv_y, self.v_uv_u, self.v_uv_v);
        color *= self.antialias_brush();
        
        let clip_alpha = self.do_clip(context);
        color * clip_alpha
    }

    fn antialias_brush(&self) -> f32 { 1.0 }
    fn do_clip(&self, _context: &ShaderContext) -> f32 { 1.0 }
}

impl FragmentShader for BrushYuvImageAlphaPassTexture2DYuvFrag {
    fn read_interp_inputs(&mut self, init: *const u8, step: *const u8) {
        unsafe {
            let init = &*(init as *const InterpOutputs);
            let step = &*(step as *const InterpOutputs);
            self.v_uv_y = init.v_uv_y;
            self.v_uv_u = init.v_uv_u;
            self.v_uv_v = init.v_uv_v;
            self.interp_step.v_uv_y = step.v_uv_y * 4.0;
            self.interp_step.v_uv_u = step.v_uv_u * 4.0;
            self.interp_step.v_uv_v = step.v_uv_v * 4.0;
        }
    }
    
    fn read_perspective_inputs(&mut self, init: *const u8, step: *const u8, w: f32) {
        unsafe {
            let init = &*(init as *const InterpOutputs);
            let step = &*(step as *const InterpOutputs);
            let inv_w = 1.0 / w;
            
            self.interp_perspective.v_uv_y = init.v_uv_y;
            self.interp_perspective.v_uv_u = init.v_uv_u;
            self.interp_perspective.v_uv_v = init.v_uv_v;
            
            self.v_uv_y = self.interp_perspective.v_uv_y * inv_w;
            self.v_uv_u = self.interp_perspective.v_uv_u * inv_w;
            self.v_uv_v = self.interp_perspective.v_uv_v * inv_w;
            
            self.interp_step.v_uv_y = step.v_uv_y * 4.0;
            self.interp_step.v_uv_u = step.v_uv_u * 4.0;
            self.interp_step.v_uv_v = step.v_uv_v * 4.0;
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
        if self.vert.common.v_format.x == 3 {
            context.swgl_commit_texture_linear_yuv_3plane(
                SamplerId::SColor0, self.v_uv_y, self.vert.common.v_uv_bounds_y,
                SamplerId::SColor1, self.v_uv_u, self.vert.common.v_uv_bounds_u,
                SamplerId::SColor2, self.v_uv_v, self.vert.common.v_uv_bounds_v,
                self.vert.common.v_ycbcr_bias,
                self.vert.common.v_rgb_from_debiased_ycbcr,
                self.vert.common.v_rescale_factor
            );
        } else if self.vert.common.v_format.x == 0 || self.vert.common.v_format.x == 1 {
             context.swgl_commit_texture_linear_yuv_2plane(
                SamplerId::SColor0, self.v_uv_y, self.vert.common.v_uv_bounds_y,
                SamplerId::SColor1, self.v_uv_u, self.vert.common.v_uv_bounds_u,
                self.vert.common.v_ycbcr_bias,
                self.vert.common.v_rgb_from_debiased_ycbcr,
                self.vert.common.v_rescale_factor
            );
        } else if self.vert.common.v_format.x == 4 {
            context.swgl_commit_texture_linear_yuv_1plane(
                SamplerId::SColor0, self.v_uv_y, self.vert.common.v_uv_bounds_y,
                self.vert.common.v_ycbcr_bias,
                self.vert.common.v_rgb_from_debiased_ycbcr,
                self.vert.common.v_rescale_factor
            );
        }
        1
    }
}

impl BrushYuvImageAlphaPassTexture2DYuvFrag {
    fn step_interp_inputs(&mut self, steps: i32) {
        let chunks = steps as f32 * 0.25;
        self.v_uv_y += self.interp_step.v_uv_y * chunks;
        self.v_uv_u += self.interp_step.v_uv_u * chunks;
        self.v_uv_v += self.interp_step.v_uv_v * chunks;
    }

    fn step_perspective_inputs(&mut self, steps: i32, next_w: &[f32; 4]) {
        let chunks = steps as f32 * 0.25;
        let inv_w = 1.0 / next_w[0];

        self.interp_perspective.v_uv_y += self.interp_step.v_uv_y * chunks;
        self.interp_perspective.v_uv_u += self.interp_step.v_uv_u * chunks;
        self.interp_perspective.v_uv_v += self.interp_step.v_uv_v * chunks;
        
        self.v_uv_y = self.interp_perspective.v_uv_y * inv_w;
        self.v_uv_u = self.interp_perspective.v_uv_u * inv_w;
        self.v_uv_v = self.interp_perspective.v_uv_v * inv_w;
    }
}

//
// Program
//

#[derive(Clone, Debug, Default)]
pub struct BrushYuvImageAlphaPassTexture2DYuvProgram {
    frag: BrushYuvImageAlphaPassTexture2DYuvFrag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(BrushYuvImageAlphaPassTexture2DYuvProgram::default())
}

impl Program for BrushYuvImageAlphaPassTexture2DYuvProgram {
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
        if strcmp(name, "sColor2") { return 10; }
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