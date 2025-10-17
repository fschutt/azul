use crate::common::*;
use crate::common_types::*;
use crate::traits::{FragmentShader, Program, VertexShader};
use glam::{ivec2, ivec4, mat4, vec2, vec4, Mat4, Vec2, Vec4};
use std::ffi::CStr;
use std::mem;
use super::strcmp;

// Corresponds to brush_linear_gradient_common
#[derive(Clone, Debug, Default)]
struct BrushLinearGradientCommon {
    attrib_locations: AttribLocations,
    u_transform: mat4,
    // Flat varyings (set in VS, used in FS)
    v_gradient_address: ivec2,
    v_gradient_repeat: vec2,
    v_repeated_size: vec2,
    v_start_offset: vec2,
    v_scale_dir: vec2,
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

// Corresponds to brush_linear_gradient_vert
#[derive(Clone, Debug, Default)]
struct BrushLinearGradientVert {
    common: BrushLinearGradientCommon,
    // Inputs
    a_position: vec2,
    a_data: ivec4,
    // Outputs
    v_pos: vec2,
    gl_position: vec4,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct InterpOutputs {
    v_pos: vec2,
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
struct Gradient {
    pub start_end_point: vec4,
    pub extend_mode: i32,
    pub stretch_size: vec2,
}

impl BrushLinearGradientVert {
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
            let segment_address = ph.specific_prim_address + 2 + (instance.segment_index * 2);
            let segment_info = context.fetch_from_gpu_cache_2(segment_address);
            let mut rect = RectWithEndpoint { p0: segment_info[0].xy(), p1: segment_info[0].zw() };
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

        let local_pos = adjusted_segment_rect.p0.lerp(adjusted_segment_rect.p1, self.a_position);
        
        let (gl_pos, vi) = context.write_vertex(local_pos, ph.local_clip_rect, ph.z, &transform, &task, &self.common.u_transform);
        self.gl_position = gl_pos;
        
        context.write_clip(vi.world_pos, &clip_area, &task);
        
        self.brush_vs(vi, ph.specific_prim_address, ph.local_rect, segment_rect, ph.user_data, brush_flags, segment_data, context);
    }

    fn fetch_gradient(&self, address: i32, context: &ShaderContext) -> Gradient {
        let data = context.fetch_from_gpu_cache_2(address);
        Gradient {
            start_end_point: data[0],
            extend_mode: data[1].x.to_bits() as i32,
            stretch_size: data[1].yz(),
        }
    }

    fn write_gradient_vertex(&mut self, vi: VertexInfo, local_rect: RectWithEndpoint, segment_rect: RectWithEndpoint, prim_user_data: ivec4, brush_flags: i32, texel_rect: vec4, extend_mode: i32, stretch_size: vec2) {
        if (brush_flags & 2) != 0 {
            self.v_pos = (vi.local_pos - segment_rect.p0) / (segment_rect.p1 - segment_rect.p0);
            self.v_pos = self.v_pos * (texel_rect.zw() - texel_rect.xy()) + texel_rect.xy();
            self.v_pos = self.v_pos * (local_rect.p1 - local_rect.p0);
        } else {
            self.v_pos = vi.local_pos - local_rect.p0;
        }

        self.common.v_repeated_size = stretch_size;
        self.v_pos /= self.common.v_repeated_size;

        self.common.v_gradient_address.x = prim_user_data.x;
        self.common.v_gradient_repeat.x = if extend_mode == 1 { 1.0 } else { 0.0 };
    }

    fn brush_vs(
        &mut self,
        vi: VertexInfo,
        prim_address: i32,
        local_rect: RectWithEndpoint,
        segment_rect: RectWithEndpoint,
        prim_user_data: ivec4,
        brush_flags: i32,
        texel_rect: vec4,
        context: &ShaderContext
    ) {
        let gradient = self.fetch_gradient(prim_address, context);
        self.write_gradient_vertex(vi, local_rect, segment_rect, prim_user_data, brush_flags, texel_rect, gradient.extend_mode, gradient.stretch_size);
        
        let start_point = gradient.start_end_point.xy();
        let end_point = gradient.start_end_point.zw();
        let dir = end_point - start_point;
        
        self.common.v_scale_dir = dir / dir.dot(dir);
        self.common.v_start_offset.x = start_point.dot(self.common.v_scale_dir);
        self.common.v_scale_dir *= self.common.v_repeated_size;
    }
}

impl VertexShader for BrushLinearGradientVert {
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
                (*dest_ptr).v_pos = self.v_pos;
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

// Corresponds to brush_linear_gradient_frag
#[derive(Clone, Debug, Default)]
struct BrushLinearGradientFrag {
    vert: BrushLinearGradientVert,
    // Varying inputs
    v_pos: vec2,
    interp_step: InterpOutputs,
    interp_perspective: InterpOutputs,
}

impl BrushLinearGradientFrag {
    fn clamp_gradient_entry(&self, offset: f32) -> f32 {
        (1.0 + (offset * 128.0)).clamp(0.0, 1.0 + 128.0)
    }
    
    fn sample_gradient(&self, offset: f32, context: &ShaderContext) -> vec4 {
        let mut offset = offset;
        offset -= offset.floor() * self.vert.common.v_gradient_repeat.x;
        let x = self.clamp_gradient_entry(offset);
        let entry_index = x.floor();
        let entry_fract = x - entry_index;
        
        let texels = context.fetch_from_gpu_buffer_2f(self.vert.common.v_gradient_address.x + (2 * entry_index as i32));

        texels[0] + (texels[1] * entry_fract) // dither is no-op
    }
    
    fn get_gradient_offset(&self, pos: vec2) -> f32 {
        pos.dot(self.vert.common.v_scale_dir) - self.vert.common.v_start_offset.x
    }
    
    fn compute_repeated_pos(&self) -> vec2 {
        self.v_pos.fract()
    }

    fn main(&self, context: &ShaderContext) -> vec4 {
        let pos = self.compute_repeated_pos();
        let offset = self.get_gradient_offset(pos);
        self.sample_gradient(offset, context)
    }
}

impl FragmentShader for BrushLinearGradientFrag {
    fn read_interp_inputs(&mut self, init: *const u8, step: *const u8) {
        unsafe {
            let init = &*(init as *const InterpOutputs);
            let step = &*(step as *const InterpOutputs);
            self.v_pos = init.v_pos;
            self.interp_step.v_pos = step.v_pos * 4.0;
        }
    }

    fn read_perspective_inputs(&mut self, init: *const u8, step: *const u8, w: f32) {
        unsafe {
            let init = &*(init as *const InterpOutputs);
            let step = &*(step as *const InterpOutputs);
            let inv_w = 1.0 / w;
            self.interp_perspective.v_pos = init.v_pos;
            self.v_pos = self.interp_perspective.v_pos * inv_w;
            self.interp_step.v_pos = step.v_pos * 4.0;
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
        let address = context.swgl_validate_gradient(SamplerId::SGpuBufferF, self.vert.common.v_gradient_address.x, (128.0 + 2.0) as i32);
        if address < 0 {
             return 0;
        }
        context.swgl_commit_linear_gradient_rgba8(
            SamplerId::SGpuBufferF,
            address,
            128.0,
            true, // is_premultiplied
            self.vert.common.v_gradient_repeat.x != 0.0,
            self.v_pos,
            self.vert.common.v_scale_dir,
            self.vert.common.v_start_offset.x
        );
        1
    }
}

impl BrushLinearGradientFrag {
    fn step_interp_inputs(&mut self, steps: i32) {
        let chunks = steps as f32 * 0.25;
        self.v_pos += self.interp_step.v_pos * chunks;
    }

    fn step_perspective_inputs(&mut self, steps: i32, next_w: &[f32; 4]) {
        let chunks = steps as f32 * 0.25;
        let inv_w = 1.0 / next_w[0];
        self.interp_perspective.v_pos += self.interp_step.v_pos * chunks;
        self.v_pos = self.interp_perspective.v_pos * inv_w;
    }
}


// Corresponds to brush_linear_gradient_program
#[derive(Clone, Debug, Default)]
pub struct BrushLinearGradientProgram {
    frag: BrushLinearGradientFrag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(BrushLinearGradientProgram::default())
}

impl Program for BrushLinearGradientProgram {
    fn get_vertex_shader(&mut self) -> &mut dyn VertexShader {
        &mut self.frag.vert
    }

    fn get_fragment_shader(&mut self) -> &mut dyn FragmentShader {
        &mut self.frag
    }
    
    fn get_uniform(&self, name: &CStr) -> i32 {
        if strcmp(name, "sClipMask") { return 7; }
        if strcmp(name, "sGpuBufferF") { return 8; }
        if strcmp(name, "sGpuBufferI") { return 9; }
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