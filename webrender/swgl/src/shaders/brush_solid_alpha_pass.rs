use crate::common::*;
use crate::common_types::*;
use crate::traits::{FragmentShader, Program, VertexShader};
use glam::{ivec4, mat4, vec2, vec4, Mat4, Vec2, Vec4};
use std::ffi::CStr;
use std::mem;
use super::strcmp;

// Corresponds to brush_solid_ALPHA_PASS_common
#[derive(Clone, Debug, Default)]
struct BrushSolidAlphaPassCommon {
    attrib_locations: AttribLocations,
    u_transform: mat4,
    // Flat varying
    v_color: vec4,
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

// Corresponds to brush_solid_ALPHA_PASS_vert
#[derive(Clone, Debug, Default)]
struct BrushSolidAlphaPassVert {
    common: BrushSolidAlphaPassCommon,
    // Inputs
    a_position: vec2,
    a_data: ivec4,
    // Outputs
    gl_position: vec4,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct InterpOutputs;

impl BrushSolidAlphaPassVert {
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

        let (segment_rect, _segment_data) = if instance.segment_index == 65535 {
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

        self.brush_vs(context, vi, ph.specific_prim_address, ph.user_data);
    }

    fn brush_vs(
        &mut self,
        context: &ShaderContext,
        _vi: VertexInfo,
        prim_address: i32,
        prim_user_data: ivec4,
    ) {
        let prim = context.fetch_from_gpu_cache_1(prim_address); // SolidBrush is just a vec4
        let opacity = (prim_user_data.x as f32) / 65535.0;
        self.common.v_color = prim * opacity;
    }
}

impl VertexShader for BrushSolidAlphaPassVert {
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
        _interps: *mut u8,
        _interp_stride: usize,
    ) {
        self.main(context);
    }

    fn set_uniform_1i(&mut self, _index: i32, _value: i32) {}
    fn set_uniform_4fv(&mut self, _index: i32, _value: &[f32; 4]) {}
    fn set_uniform_matrix4fv(&mut self, index: i32, value: &[f32; 16]) {
        if index == 6 {
            self.common.u_transform = Mat4::from_cols_array(value);
        }
    }
}

// Corresponds to brush_solid_ALPHA_PASS_frag
#[derive(Clone, Debug, Default)]
struct BrushSolidAlphaPassFrag {
    vert: BrushSolidAlphaPassVert,
}

impl BrushSolidAlphaPassFrag {
    fn antialias_brush(&self, context: &ShaderContext) -> f32 {
        context.swgl_anti_alias_coverage()
    }

    fn do_clip(&self, context: &ShaderContext) -> f32 {
        context.swgl_clip_mask()
    }

    fn brush_fs(&self, context: &ShaderContext) -> vec4 {
        let mut color = self.vert.common.v_color;
        color *= self.antialias_brush(context);
        color
    }

    fn main(&self, context: &ShaderContext) -> vec4 {
        let mut frag_color = self.brush_fs(context);
        let clip_alpha = self.do_clip(context);
        frag_color *= clip_alpha;
        frag_color
    }
}

impl FragmentShader for BrushSolidAlphaPassFrag {
    fn read_interp_inputs(&mut self, _init: *const u8, _step: *const u8) {}

    fn run(&mut self, context: &mut ShaderContext) {
        let color = self.main(context);
        context.write_output(color);
    }

    fn skip(&mut self, _steps: i32) {}

    fn draw_span_rgba8(&mut self, context: &mut ShaderContext) -> i32 {
        context.commit_solid_rgba8(self.vert.common.v_color);
        1
    }

    fn draw_span_r8(&mut self, context: &mut ShaderContext) -> i32 {
        context.commit_solid_r8(self.vert.common.v_color.x);
        1
    }
}

// Corresponds to brush_solid_ALPHA_PASS_program
#[derive(Clone, Debug, Default)]
pub struct BrushSolidAlphaPassProgram {
    frag: BrushSolidAlphaPassFrag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(BrushSolidAlphaPassProgram::default())
}

impl Program for BrushSolidAlphaPassProgram {
    fn get_vertex_shader(&mut self) -> &mut dyn VertexShader {
        &mut self.frag.vert
    }

    fn get_fragment_shader(&mut self) -> &mut dyn FragmentShader {
        &mut self.frag
    }

    fn get_uniform(&self, name: &CStr) -> i32 {
        if strcmp(name, "sClipMask") {
            return 7;
        }
        if strcmp(name, "sGpuCache") {
            return 2;
        }
        if strcmp(name, "sPrimitiveHeadersF") {
            return 4;
        }
        if strcmp(name, "sPrimitiveHeadersI") {
            return 5;
        }
        if strcmp(name, "sRenderTasks") {
            return 1;
        }
        if strcmp(name, "sTransformPalette") {
            return 3;
        }
        if strcmp(name, "uTransform") {
            return 6;
        }
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