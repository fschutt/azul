
use crate::common::*;
use crate::common_types::*;
use crate::traits::{FragmentShader, Program, VertexShader};
use glam::{ivec4, mat4, vec2, vec4, Mat4, Vec2, Vec4};
use std::ffi::CStr;
use std::mem;
use super::strcmp;

// Corresponds to brush_opacity_ANTIALIASING_common
#[derive(Clone, Debug, Default)]
struct BrushOpacityAntialiasingCommon {
    attrib_locations: AttribLocations,
    u_transform: mat4,
    // Varyings (flat)
    v_uv_sample_bounds: vec4,
    v_opacity_perspective_vec: vec2,
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

// Corresponds to brush_opacity_ANTIALIASING_vert
#[derive(Clone, Debug, Default)]
struct BrushOpacityAntialiasingVert {
    common: BrushOpacityAntialiasingCommon,
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

impl BrushOpacityAntialiasingVert {
    fn main(&mut self, context: &ShaderContext) {
        let instance = context.decode_instance_attributes(self.a_data);
        let ph = context.fetch_prim_header(instance.prim_header_address);
        let transform = context.fetch_transform(ph.transform_id);
        let task = context.fetch_picture_task(ph.picture_task_address);
        let clip_area = context.fetch_clip_area(instance.clip_address);

        self.brush_shader_main_vs(context, instance, ph, transform, task, clip_area);
    }

    fn brush_shader_main_vs(
        &mut self,
        context: &ShaderContext,
        instance: Instance,
        mut ph: PrimitiveHeader,
        transform: Transform,
        task: PictureTask,
        clip_area: ClipArea,
    ) {
        let edge_flags = (instance.flags >> 12) & 15;
        let brush_flags = instance.flags & 4095;

        let (segment_rect, _segment_data) = if instance.segment_index == 65535 {
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
            // This would call into the rasterizer's state machine. For now, we replicate the side effect.
            // adjusted_segment_rect = context.clip_and_init_antialiasing(segment_rect, ph.local_rect, ph.local_clip_rect, edge_flags, ph.z, &transform, &task);
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

        // context.write_clip(vi.world_pos, &clip_area, &task);

        self.brush_vs(context, vi, ph.local_rect, ph.user_data, brush_flags);
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
        let texture_size = context.texture_size(SamplerId::SColor0, 0);

        let mut f = (vi.local_pos - local_rect.p0) / context.rect_size(local_rect);
        f = context.get_image_quad_uv(prim_user_data.x, f);

        let uv = uv0.lerp(uv1, f);

        let perspective_interpolate = if (brush_flags & 1) != 0 { 1.0 } else { 0.0 };

        self.v_uv = (uv / texture_size) * vi.world_pos.w.lerp(1.0, perspective_interpolate);
        self.common.v_opacity_perspective_vec.y = perspective_interpolate;

        self.common.v_uv_sample_bounds =
            vec4(uv0.x + 0.5, uv0.y + 0.5, uv1.x - 0.5, uv1.y - 0.5)
                / texture_size.extend(texture_size).xyxy();

        self.common.v_opacity_perspective_vec.x =
            ((prim_user_data.y as f32) / 65536.0).clamp(0.0, 1.0);
    }
}

impl VertexShader for BrushOpacityAntialiasingVert {
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
        context: &ShaderContext,
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

// Corresponds to brush_opacity_ANTIALIASING_frag
#[derive(Clone, Debug, Default)]
struct BrushOpacityAntialiasingFrag {
    vert: BrushOpacityAntialiasingVert,
    v_uv: vec2,
    interp_step: InterpOutputs,
    interp_perspective: InterpOutputs,
}

impl BrushOpacityAntialiasingFrag {
    fn antialias_brush(&self) -> f32 {
        1.0 // Placeholder for swgl_antiAliasCoverage()
    }

    fn main(&self, context: &ShaderContext) -> vec4 {
        let gl_frag_coord_w = 1.0; // Placeholder

        let perspective_divisor = (1.0 - self.vert.common.v_opacity_perspective_vec.y)
            * gl_frag_coord_w
            + self.vert.common.v_opacity_perspective_vec.y;

        let uv = self.v_uv * perspective_divisor;
        let uv_clamped = uv.clamp(
            self.vert.common.v_uv_sample_bounds.xy(),
            self.vert.common.v_uv_sample_bounds.zw(),
        );

        let color = context.texture(SamplerId::SColor0, uv_clamped);
        let mut alpha = self.vert.common.v_opacity_perspective_vec.x;

        alpha *= self.antialias_brush();

        color * alpha
    }
}

impl FragmentShader for BrushOpacityAntialiasingFrag {
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
        let gl_frag_coord_w = 1.0; // Placeholder
        let perspective_divisor = (1.0 - self.vert.common.v_opacity_perspective_vec.y) * gl_frag_coord_w + self.vert.common.v_opacity_perspective_vec.y;
        let uv = self.v_uv * perspective_divisor;
        context.commit_texture_linear_color_rgba8(
            SamplerId::SColor0, 
            uv, 
            self.vert.common.v_uv_sample_bounds, 
            self.vert.common.v_opacity_perspective_vec.x
        );
        1
    }
}

impl BrushOpacityAntialiasingFrag {
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

// Corresponds to brush_opacity_ANTIALIASING_program
#[derive(Clone, Debug, Default)]
pub struct BrushOpacityAntialiasingProgram {
    frag: BrushOpacityAntialiasingFrag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(BrushOpacityAntialiasingProgram::default())
}

impl Program for BrushOpacityAntialiasingProgram {
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
