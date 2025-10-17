use crate::common::*;
use crate::common_types::*;
use crate::traits::{FragmentShader, Program, VertexShader};
use glam::{ivec2, ivec4, mat4, vec2, vec4, Mat4, Vec2, Vec4};
use std::ffi::CStr;
use std::mem;
use super::strcmp;

//
// Common Struct
//

#[derive(Clone, Debug, Default)]
struct PsSplitCompositeCommon {
    attrib_locations: AttribLocations,
    u_transform: mat4,
    // Flat varyings
    v_transform_bounds: vec4, // Not used in frag, but present in C++ common
    v_perspective: vec2,
    v_uv_sample_bounds: vec4,
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
struct PsSplitCompositeVert {
    common: PsSplitCompositeCommon,
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

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
struct SplitGeometry {
    local: [vec2; 4],
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
struct SplitCompositeInstance {
    prim_header_index: i32,
    polygons_address: i32,
    z: f32,
    render_task_index: i32,
}

impl PsSplitCompositeVert {
    fn fetch_composite_instance(&self) -> SplitCompositeInstance {
        SplitCompositeInstance {
            prim_header_index: self.a_data.x,
            polygons_address: self.a_data.y,
            z: f32::from_bits(self.a_data.z as u32),
            render_task_index: self.a_data.w,
        }
    }

    fn fetch_split_geometry(&self, context: &ShaderContext, address: i32) -> SplitGeometry {
        let data = context.fetch_from_gpu_cache_2(address);
        SplitGeometry {
            local: [data[0].xy(), data[0].zw(), data[1].zw(), data[1].xy()],
        }
    }

    fn bilerp(a: Vec2, b: Vec2, c: Vec2, d: Vec2, s: f32, t: f32) -> Vec2 {
        let x = a.lerp(b, t);
        let y = c.lerp(d, t);
        x.lerp(y, s)
    }

    #[inline(always)]
    fn main(&mut self, context: &mut ShaderContext) {
        let ci = self.fetch_composite_instance();
        let geometry = self.fetch_split_geometry(context, ci.polygons_address);
        let ph = context.fetch_prim_header(ci.prim_header_index);
        let dest_task = context.fetch_picture_task(ci.render_task_index);
        let transform = context.fetch_transform(ph.transform_id);
        let (res_uv_rect, _res_user_data) = context.fetch_image_source(ph.user_data.x);
        let clip_area = context.fetch_clip_area(ph.user_data.w);

        let dest_origin = dest_task.task_rect.p0 - dest_task.content_origin;

        let local_pos = PsSplitCompositeVert::bilerp(
            geometry.local[0],
            geometry.local[1],
            geometry.local[3],
            geometry.local[2],
            self.a_position.y,
            self.a_position.x,
        );

        let world_pos = transform.m * vec4(local_pos.x, local_pos.y, 0.0, 1.0);

        let final_pos = vec4(
            (dest_origin * world_pos.w).x + (world_pos.x * dest_task.device_pixel_scale),
            (dest_origin * world_pos.w).y + (world_pos.y * dest_task.device_pixel_scale),
            world_pos.w * ci.z,
            world_pos.w,
        );

        context.write_clip(world_pos, &clip_area, &dest_task);
        self.gl_position = self.common.u_transform * final_pos;

        let texture_size = context.texture_size(SamplerId::SColor0, 0);
        let uv0 = res_uv_rect.p0;
        let uv1 = res_uv_rect.p1;
        let min_uv = uv0.min(uv1);
        let max_uv = uv0.max(uv1);

        self.common.v_uv_sample_bounds = vec4(min_uv.x + 0.5, min_uv.y + 0.5, max_uv.x - 0.5, max_uv.y - 0.5)
            / texture_size.extend(texture_size).xyxy();

        let mut f = (local_pos - ph.local_rect.p0) / context.rect_size(ph.local_rect);
        f = context.get_image_quad_uv(ph.user_data.x, f);
        let uv = uv0.lerp(uv1, f);

        let perspective_interpolate = f32::from_bits(ph.user_data.y as u32);
        
        self.v_uv = (uv / texture_size) * self.gl_position.w.lerp(1.0, perspective_interpolate);
        self.common.v_perspective.x = perspective_interpolate;
    }
}

impl VertexShader for PsSplitCompositeVert {
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
struct PsSplitCompositeFrag {
    vert: PsSplitCompositeVert,
    // Varying inputs
    v_uv: vec2,
    interp_step: InterpOutputs,
    interp_perspective: InterpOutputs,
}

impl PsSplitCompositeFrag {
    fn do_clip(&self, context: &mut ShaderContext) -> f32 {
        context.do_clip()
    }

    #[inline(always)]
    fn main(&self, context: &ShaderContext) -> vec4 {
        let alpha = self.do_clip(context);
        let gl_frag_coord_w = 1.0; // Placeholder for rasterizer-provided value
        let perspective_divisor = (1.0 - self.vert.common.v_perspective.x) * gl_frag_coord_w + self.vert.common.v_perspective.x;
        let uv = self.v_uv * perspective_divisor;
        let uv = uv.clamp(self.vert.common.v_uv_sample_bounds.xy(), self.vert.common.v_uv_sample_bounds.zw());
        
        context.texture(SamplerId::SColor0, uv) * alpha
    }
}

impl FragmentShader for PsSplitCompositeFrag {
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
        let perspective_divisor = (1.0 - self.vert.common.v_perspective.x) * gl_frag_coord_w + self.vert.common.v_perspective.x;
        let uv = self.v_uv * perspective_divisor;
        context.commit_texture_rgba8(SamplerId::SColor0, uv, self.vert.common.v_uv_sample_bounds);
        1
    }
}

impl PsSplitCompositeFrag {
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
pub struct PsSplitCompositeProgram {
    frag: PsSplitCompositeFrag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(PsSplitCompositeProgram::default())
}

impl Program for PsSplitCompositeProgram {
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
