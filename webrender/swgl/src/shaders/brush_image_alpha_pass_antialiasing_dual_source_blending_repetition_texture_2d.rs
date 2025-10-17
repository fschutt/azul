use crate::common::*;
use crate::common_types::*;
use crate::traits::{FragmentShader, Program, VertexShader};
use glam::{bvec2, ivec2, ivec4, mat4, vec2, vec3, vec4, BVec2, Mat4, Vec2, Vec4};
use std::ffi::CStr;
use std::mem;
use super::strcmp;

// Corresponds to brush_image_ALPHA_PASS_ANTIALIASING_DUAL_SOURCE_BLENDING_REPETITION_TEXTURE_2D_common
#[derive(Clone, Debug, Default)]
struct Common {
    attrib_locations: AttribLocations,
    u_transform: mat4,
    // Flat varyings (set in VS, used in FS)
    v_color: vec4,
    v_mask_swizzle: vec2,
    v_tile_repeat_bounds: vec2,
    v_uv_bounds: vec4,
    v_uv_sample_bounds: vec4,
    v_perspective: vec2,
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

// Corresponds to brush_image_ALPHA_PASS_ANTIALIASING_DUAL_SOURCE_BLENDING_REPETITION_TEXTURE_2D_vert
#[derive(Clone, Debug, Default)]
struct Vert {
    common: Common,
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

impl Vert {
    #[inline(always)]
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

        self.brush_vs(
            context,
            vi,
            ph.specific_prim_address,
            ph.local_rect,
            segment_rect,
            ph.user_data,
            instance.resource_address,
            brush_flags,
            segment_data,
        );
    }

    fn brush_vs(
        &mut self,
        context: &ShaderContext,
        vi: VertexInfo,
        prim_address: i32,
        prim_rect: RectWithEndpoint,
        segment_rect: RectWithEndpoint,
        prim_user_data: ivec4,
        specific_resource_address: i32,
        brush_flags: i32,
        segment_data: vec4,
    ) {
        let mut image_data = context.fetch_image_data(prim_address);
        let texture_size = context.texture_size(SamplerId::SColor0, 0);
        let (res_uv_rect, _res_user_data) =
            context.fetch_image_source(specific_resource_address);

        let mut uv0 = res_uv_rect.p0;
        let mut uv1 = res_uv_rect.p1;
        let mut local_rect = prim_rect;
        let mut stretch_size = image_data.stretch_size;

        if stretch_size.x < 0.0 {
            stretch_size = context.rect_size(local_rect);
        }

        if (brush_flags & 2) != 0 {
            local_rect = segment_rect;
            stretch_size = context.rect_size(local_rect);

            if (brush_flags & 512) != 0 {
                let uv_size = res_uv_rect.p1 - res_uv_rect.p0;
                uv0 = res_uv_rect.p0 + (segment_data.xy() * uv_size);
                uv1 = res_uv_rect.p0 + (segment_data.zw() * uv_size);
            }

            if (brush_flags & 512) != 0 {
                let mut repeated_stretch_size = stretch_size;
                let mut horizontal_uv_size = uv1 - uv0;
                let mut vertical_uv_size = uv1 - uv0;

                if (brush_flags & 256) != 0 {
                    repeated_stretch_size = segment_rect.p0 - prim_rect.p0;
                    let epsilon = 0.001;
                    vertical_uv_size.x = uv0.x - res_uv_rect.p0.x;
                    if vertical_uv_size.x < epsilon || repeated_stretch_size.x < epsilon {
                        vertical_uv_size.x = res_uv_rect.p1.x - uv1.x;
                        repeated_stretch_size.x = prim_rect.p1.x - segment_rect.p1.x;
                    }
                    horizontal_uv_size.y = uv0.y - res_uv_rect.p0.y;
                    if horizontal_uv_size.y < epsilon || repeated_stretch_size.y < epsilon {
                        horizontal_uv_size.y = res_uv_rect.p1.y - uv1.y;
                        repeated_stretch_size.y = prim_rect.p1.y - segment_rect.p1.y;
                    }
                }

                if (brush_flags & 4) != 0 {
                    let uv_ratio = horizontal_uv_size.x / horizontal_uv_size.y;
                    stretch_size.x = repeated_stretch_size.y * uv_ratio;
                }
                if (brush_flags & 8) != 0 {
                    let uv_ratio = vertical_uv_size.y / vertical_uv_size.x;
                    stretch_size.y = repeated_stretch_size.x * uv_ratio;
                }
            } else {
                if (brush_flags & 4) != 0 {
                    stretch_size.x = segment_data.z - segment_data.x;
                }
                if (brush_flags & 8) != 0 {
                    stretch_size.y = segment_data.w - segment_data.y;
                }
            }

            if (brush_flags & 16) != 0 {
                let segment_rect_width = segment_rect.p1.x - segment_rect.p0.x;
                let nx = (segment_rect_width / stretch_size.x).round().max(1.0);
                stretch_size.x = segment_rect_width / nx;
            }
            if (brush_flags & 32) != 0 {
                let segment_rect_height = segment_rect.p1.y - segment_rect.p0.y;
                let ny = (segment_rect_height / stretch_size.y).round().max(1.0);
                stretch_size.y = segment_rect_height / ny;
            }
        }

        let perspective_interpolate = if (brush_flags & 1) != 0 { 1.0 } else { 0.0 };
        self.common.v_perspective.x = perspective_interpolate;

        if (brush_flags & 2048) != 0 {
            uv0 *= texture_size;
            uv1 *= texture_size;
        }

        let min_uv = uv0.min(uv1);
        let max_uv = uv0.max(uv1);
        self.common.v_uv_sample_bounds = vec4(
            min_uv.x + 0.5,
            min_uv.y + 0.5,
            max_uv.x - 0.5,
            max_uv.y - 0.5,
        ) / texture_size.extend(texture_size).xyxy();

        let mut f = (vi.local_pos - local_rect.p0) / context.rect_size(local_rect);
        let color_mode = prim_user_data.x & 0xFFFF;
        let blend_mode = prim_user_data.x >> 16;
        let raster_space = prim_user_data.y;
        if raster_space == 1 {
            f = context.get_image_quad_uv(specific_resource_address, f);
        }

        let repeat = context.rect_size(local_rect) / stretch_size;
        self.v_uv = uv0.lerp(uv1, f) - min_uv;
        self.v_uv *= repeat;

        let centered = BVec2::new((brush_flags & 64) != 0, (brush_flags & 128) != 0);
        let normalized_offset =
            vec2::ZERO.lerp(vec2::ONE - (repeat * 0.5 + 0.5).fract(), centered.into());

        self.v_uv += normalized_offset * (max_uv - min_uv);
        self.v_uv /= texture_size;

        if perspective_interpolate == 0.0 {
            self.v_uv *= vi.world_pos.w;
        }

        self.common.v_uv_bounds = vec4(min_uv.x, min_uv.y, max_uv.x, max_uv.y)
            / texture_size.extend(texture_size).xyxy();
        self.v_uv /= self.common.v_uv_bounds.zw() - self.common.v_uv_bounds.xy();
        self.common.v_tile_repeat_bounds = repeat + normalized_offset;

        let opacity = (prim_user_data.z as f32) / 65535.0;
        match blend_mode {
            0 => image_data.color.w *= opacity,
            _ => image_data.color *= opacity,
        }

        match color_mode {
            0 | 2 => {
                // swgl_blendDropShadow(image_data.color);
                self.common.v_mask_swizzle = vec2(1.0, 0.0);
                self.common.v_color = vec4::ONE;
            }
            4 => {
                self.common.v_mask_swizzle = vec2(1.0, 0.0);
                self.common.v_color = image_data.color;
            }
            3 => {
                self.common.v_mask_swizzle = vec2(1.0, 0.0);
                self.common.v_color = vec4::splat(image_data.color.w);
            }
            1 => {
                self.common.v_mask_swizzle = vec2(image_data.color.w, 0.0);
                self.common.v_color = image_data.color;
            }
            5 => {
                self.common.v_mask_swizzle = vec2(-image_data.color.w, image_data.color.w);
                self.common.v_color = image_data.color;
            }
            _ => {
                self.common.v_mask_swizzle = vec2::ZERO;
                self.common.v_color = vec4::ONE;
            }
        }
    }
}

impl VertexShader for Vert {
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

// Corresponds to brush_image_ALPHA_PASS_ANTIALIASING_DUAL_SOURCE_BLENDING_REPETITION_TEXTURE_2D_frag
#[derive(Clone, Debug, Default)]
struct Frag {
    vert: Vert,
    // Varying inputs from rasterizer
    v_uv: vec2,
    interp_step: InterpOutputs,
    interp_perspective: InterpOutputs,
}

impl Frag {
    fn compute_repeated_uvs(&self, perspective_divisor: f32) -> vec2 {
        let uv_size = self.vert.common.v_uv_bounds.zw() - self.vert.common.v_uv_bounds.xy();
        let mut local_uv = self.v_uv * perspective_divisor;
        local_uv = local_uv.max(vec2::ZERO);
        let mut repeated_uv = local_uv.fract() * uv_size + self.vert.common.v_uv_bounds.xy();

        if local_uv.x >= self.vert.common.v_tile_repeat_bounds.x {
            repeated_uv.x = self.vert.common.v_uv_bounds.z;
        }
        if local_uv.y >= self.vert.common.v_tile_repeat_bounds.y {
            repeated_uv.y = self.vert.common.v_uv_bounds.w;
        }

        repeated_uv
    }

    fn antialias_brush(&self) -> f32 {
        1.0
    }

    fn do_clip(&self, _context: &ShaderContext) -> f32 {
        1.0
    }

    fn brush_fs(&self, context: &ShaderContext) -> (vec4, vec4) {
        let gl_frag_coord_w = 1.0;
        let perspective_divisor = (1.0 - self.vert.common.v_perspective.x) * gl_frag_coord_w + self.vert.common.v_perspective.x;
        
        let repeated_uv = self.compute_repeated_uvs(perspective_divisor);
        let uv = repeated_uv.clamp(
            self.vert.common.v_uv_sample_bounds.xy(),
            self.vert.common.v_uv_sample_bounds.zw(),
        );
        
        let mut texel = context.texture(SamplerId::SColor0, uv);
        let alpha = self.antialias_brush();
        
        texel.truncate() = (texel.rgb() * self.vert.common.v_mask_swizzle.x) + (vec3::splat(texel.a) * self.vert.common.v_mask_swizzle.y);

        let alpha_mask = texel * alpha;
        
        let color = self.vert.common.v_color * alpha_mask;
        let blend = (alpha_mask * self.vert.common.v_mask_swizzle.x) + (vec4::splat(alpha_mask.a) * self.vert.common.v_mask_swizzle.y);

        (color, blend)
    }

    #[inline(always)]
    fn main(&self, context: &mut ShaderContext) {
        let (mut color, mut blend) = self.brush_fs(context);
        let clip_alpha = self.do_clip(context);
        color *= clip_alpha;
        blend *= clip_alpha;
        
        context.write_output(color);
        context.write_secondary_output(blend);
    }
}

impl FragmentShader for Frag {
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

    fn draw_span_rgba8(&mut self, _context: &mut ShaderContext) -> i32 {
        // C++ version is empty, so we return 0 to indicate not handled.
        0
    }
}

impl Frag {
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

// Corresponds to brush_image_ALPHA_PASS_ANTIALIASING_DUAL_SOURCE_BLENDING_REPETITION_TEXTURE_2D_program
#[derive(Clone, Debug, Default)]
pub struct ProgramImpl {
    frag: Frag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(ProgramImpl::default())
}

impl Program for ProgramImpl {
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
