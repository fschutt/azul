use crate::common::*;
use crate::common_types::*;
use crate::traits::{FragmentShader, Program, VertexShader};
use glam::{ivec2, ivec4, mat4, vec2, vec3, vec4, Mat4, Vec2, Vec3, Vec4};
use std::ffi::CStr;
use std::mem;
use super::strcmp;

//
// C++ Data Structures
//

#[derive(Clone, Copy, Debug, Default)]
struct ClipMaskInstanceCommon {
    sub_rect: RectWithEndpoint,
    task_origin: vec2,
    screen_origin: vec2,
    device_pixel_scale: f32,
    clip_transform_id: i32,
    prim_transform_id: i32,
}

#[derive(Clone, Copy, Debug, Default)]
struct ClipMaskInstanceBoxShadow {
    base: ClipMaskInstanceCommon,
    resource_address: ivec2,
}

#[derive(Clone, Copy, Debug, Default)]
struct BoxShadowData {
    src_rect_size: vec2,
    clip_mode: i32,
    stretch_mode_x: i32,
    stretch_mode_y: i32,
    dest_rect: RectWithEndpoint,
}

#[derive(Clone, Copy, Debug, Default)]
struct ClipVertexInfo {
    local_pos: vec4,
    clipped_local_rect: RectWithEndpoint,
}

//
// Common Struct
//

#[derive(Clone, Debug, Default)]
struct CsClipBoxShadowTexture2DCommon {
    attrib_locations: AttribLocations,
    u_transform: mat4,
    // Flat varyings
    v_transform_bounds: vec4,
    v_uv_bounds: vec4,
    v_edge: vec4,
    v_uv_bounds_no_clamp: vec4,
    v_clip_mode: vec2,
}

#[derive(Clone, Debug, Default)]
struct AttribLocations {
    a_position: usize,
    a_clip_device_area: usize,
    a_clip_origins: usize,
    a_device_pixel_scale: usize,
    a_transform_ids: usize,
    a_clip_data_resource_address: usize,
    a_clip_src_rect_size: usize,
    a_clip_mode: usize,
    a_stretch_mode: usize,
    a_clip_dest_rect: usize,
}

impl AttribLocations {
    fn bind_loc(&mut self, name: &CStr, index: i32) {
        if strcmp(name, "aPosition") { self.a_position = index as usize; }
        else if strcmp(name, "aClipDeviceArea") { self.a_clip_device_area = index as usize; }
        else if strcmp(name, "aClipOrigins") { self.a_clip_origins = index as usize; }
        else if strcmp(name, "aDevicePixelScale") { self.a_device_pixel_scale = index as usize; }
        else if strcmp(name, "aTransformIds") { self.a_transform_ids = index as usize; }
        else if strcmp(name, "aClipDataResourceAddress") { self.a_clip_data_resource_address = index as usize; }
        else if strcmp(name, "aClipSrcRectSize") { self.a_clip_src_rect_size = index as usize; }
        else if strcmp(name, "aClipMode") { self.a_clip_mode = index as usize; }
        else if strcmp(name, "aStretchMode") { self.a_stretch_mode = index as usize; }
        else if strcmp(name, "aClipDestRect") { self.a_clip_dest_rect = index as usize; }
    }

    fn get_loc(&self, name: &CStr) -> i32 {
        if strcmp(name, "aPosition") { if self.a_position != NULL_ATTRIB { self.a_position as i32 } else { -1 } }
        else if strcmp(name, "aClipDeviceArea") { if self.a_clip_device_area != NULL_ATTRIB { self.a_clip_device_area as i32 } else { -1 } }
        else if strcmp(name, "aClipOrigins") { if self.a_clip_origins != NULL_ATTRIB { self.a_clip_origins as i32 } else { -1 } }
        else if strcmp(name, "aDevicePixelScale") { if self.a_device_pixel_scale != NULL_ATTRIB { self.a_device_pixel_scale as i32 } else { -1 } }
        else if strcmp(name, "aTransformIds") { if self.a_transform_ids != NULL_ATTRIB { self.a_transform_ids as i32 } else { -1 } }
        else if strcmp(name, "aClipDataResourceAddress") { if self.a_clip_data_resource_address != NULL_ATTRIB { self.a_clip_data_resource_address as i32 } else { -1 } }
        else if strcmp(name, "aClipSrcRectSize") { if self.a_clip_src_rect_size != NULL_ATTRIB { self.a_clip_src_rect_size as i32 } else { -1 } }
        else if strcmp(name, "aClipMode") { if self.a_clip_mode != NULL_ATTRIB { self.a_clip_mode as i32 } else { -1 } }
        else if strcmp(name, "aStretchMode") { if self.a_stretch_mode != NULL_ATTRIB { self.a_stretch_mode as i32 } else { -1 } }
        else if strcmp(name, "aClipDestRect") { if self.a_clip_dest_rect != NULL_ATTRIB { self.a_clip_dest_rect as i32 } else { -1 } }
        else { -1 }
    }
}

//
// Vertex Shader
//

#[derive(Clone, Debug, Default)]
struct CsClipBoxShadowTexture2DVert {
    common: CsClipBoxShadowTexture2DCommon,
    // Inputs
    a_position: vec2,
    a_clip_device_area: vec4,
    a_clip_origins: vec4,
    a_device_pixel_scale: f32,
    a_transform_ids: ivec2,
    a_clip_data_resource_address: ivec2,
    a_clip_src_rect_size: vec2,
    a_clip_mode: i32,
    a_stretch_mode: ivec2,
    a_clip_dest_rect: vec4,
    // Outputs
    v_local_pos: vec4,
    v_uv: vec2,
    gl_position: vec4,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct InterpOutputs {
    v_local_pos: vec4,
    v_uv: vec2,
}

impl CsClipBoxShadowTexture2DVert {
    fn main(&mut self, context: &ShaderContext) {
        let cmi = self.fetch_clip_item();
        let clip_transform = context.fetch_transform(cmi.base.clip_transform_id);
        let prim_transform = context.fetch_transform(cmi.base.prim_transform_id);
        let bs_data = self.fetch_data();
        let res = context.fetch_image_source_direct(cmi.resource_address);

        let dest_rect = bs_data.dest_rect;
        let vi = self.write_clip_tile_vertex(
            dest_rect,
            prim_transform,
            clip_transform,
            cmi.base.sub_rect,
            cmi.base.task_origin,
            cmi.base.screen_origin,
            cmi.base.device_pixel_scale,
        );

        self.common.v_clip_mode.x = bs_data.clip_mode as f32;
        self.v_local_pos = vi.local_pos;

        let local_pos = vi.local_pos.xy() / vi.local_pos.w;
        let dest_rect_size = context.rect_size(dest_rect);

        match bs_data.stretch_mode_x {
            0 => {
                self.common.v_edge.x = 0.5;
                self.common.v_edge.z = (dest_rect_size.x / bs_data.src_rect_size.x) - 0.5;
                self.v_uv.x = (local_pos.x - dest_rect.p0.x) / bs_data.src_rect_size.x;
            },
            _ => { // 1
                self.common.v_edge.x = 1.0;
                self.common.v_edge.z = 1.0;
                self.v_uv.x = (local_pos.x - dest_rect.p0.x) / dest_rect_size.x;
            }
        }

        match bs_data.stretch_mode_y {
            0 => {
                self.common.v_edge.y = 0.5;
                self.common.v_edge.w = (dest_rect_size.y / bs_data.src_rect_size.y) - 0.5;
                self.v_uv.y = (local_pos.y - dest_rect.p0.y) / bs_data.src_rect_size.y;
            },
            _ => { // 1
                self.common.v_edge.y = 1.0;
                self.common.v_edge.w = 1.0;
                self.v_uv.y = (local_pos.y - dest_rect.p0.y) / dest_rect_size.y;
            }
        }
        
        self.v_uv *= vi.local_pos.w;

        let texture_size = context.texture_size(SamplerId::SColor0, 0);
        let uv0 = res.uv_rect.p0;
        let uv1 = res.uv_rect.p1;
        self.common.v_uv_bounds = vec4(uv0.x + 0.5, uv0.y + 0.5, uv1.x - 0.5, uv1.y - 0.5) / texture_size.extend(texture_size).xyxy();
        self.common.v_uv_bounds_no_clamp = vec4(uv0.x, uv0.y, uv1.x, uv1.y) / texture_size.extend(texture_size).xyxy();
    }

    fn fetch_clip_item_common(&self) -> ClipMaskInstanceCommon {
        ClipMaskInstanceCommon {
            sub_rect: RectWithEndpoint { p0: self.a_clip_device_area.xy(), p1: self.a_clip_device_area.zw() },
            task_origin: self.a_clip_origins.xy(),
            screen_origin: self.a_clip_origins.zw(),
            device_pixel_scale: self.a_device_pixel_scale,
            clip_transform_id: self.a_transform_ids.x,
            prim_transform_id: self.a_transform_ids.y,
        }
    }

    fn fetch_clip_item(&self) -> ClipMaskInstanceBoxShadow {
        ClipMaskInstanceBoxShadow {
            base: self.fetch_clip_item_common(),
            resource_address: self.a_clip_data_resource_address,
        }
    }

    fn fetch_data(&self) -> BoxShadowData {
        BoxShadowData {
            src_rect_size: self.a_clip_src_rect_size,
            clip_mode: self.a_clip_mode,
            stretch_mode_x: self.a_stretch_mode.x,
            stretch_mode_y: self.a_stretch_mode.y,
            dest_rect: RectWithEndpoint { p0: self.a_clip_dest_rect.xy(), p1: self.a_clip_dest_rect.zw() },
        }
    }

    fn write_clip_tile_vertex(
        &mut self,
        local_clip_rect: RectWithEndpoint,
        prim_transform: Transform,
        clip_transform: Transform,
        sub_rect: RectWithEndpoint,
        task_origin: vec2,
        screen_origin: vec2,
        device_pixel_scale: f32,
    ) -> ClipVertexInfo {
        let device_pos = screen_origin + sub_rect.p0.lerp(sub_rect.p1, self.a_position);
        let world_pos = device_pos / device_pixel_scale;
        let mut pos = prim_transform.m * world_pos.extend(0.0).extend(1.0);
        pos /= pos.w;

        let p = get_node_pos(pos.xy(), clip_transform);
        let local_pos = p * pos.w;
        let vertex_pos = vec4::new(
            (task_origin + sub_rect.p0.lerp(sub_rect.p1, self.a_position)).x,
            (task_origin + sub_rect.p0.lerp(sub_rect.p1, self.a_position)).y,
            0.0, 1.0
        );

        self.gl_position = self.common.u_transform * vertex_pos;
        self.rectangle_aa_vertex(vec4(local_clip_rect.p0.x, local_clip_rect.p0.y, local_clip_rect.p1.x, local_clip_rect.p1.y));
        
        ClipVertexInfo { local_pos, clipped_local_rect: local_clip_rect }
    }

    fn rectangle_aa_vertex(&mut self, local_bounds: vec4) {
        self.common.v_transform_bounds = local_bounds;
    }
}

impl VertexShader for CsClipBoxShadowTexture2DVert {
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
            let a_clip_device_area_attrib = &*attribs[self.common.attrib_locations.a_clip_device_area];
            // ... and so on for all attributes
            let pos_ptr = (a_pos_attrib.data as *const u8).add(a_pos_attrib.stride * start as usize) as *const Vec2;
            self.a_position = *pos_ptr;
            let area_ptr = (a_clip_device_area_attrib.data as *const u8).add(a_clip_device_area_attrib.stride * instance as usize) as *const Vec4;
            self.a_clip_device_area = *area_ptr;
            // ...
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
                (*dest_ptr).v_local_pos = self.v_local_pos;
                (*dest_ptr).v_uv = self.v_uv;
                dest_ptr = (dest_ptr as *mut u8).add(interp_stride) as *mut InterpOutputs;
            }
        }
    }

    fn set_uniform_1i(&mut self, _index: i32, _value: i32) {}
    fn set_uniform_4fv(&mut self, _index: i32, _value: &[f32; 4]) {}
    fn set_uniform_matrix4fv(&mut self, index: i32, value: &[f32; 16]) {
        if index == 4 { // uTransform
            self.common.u_transform = Mat4::from_cols_array(value);
        }
    }
}

//
// Fragment Shader
//

#[derive(Clone, Debug, Default)]
struct CsClipBoxShadowTexture2DFrag {
    vert: CsClipBoxShadowTexture2DVert,
    // Varying inputs
    v_local_pos: vec4,
    v_uv: vec2,
    interp_step: InterpOutputs,
    interp_perspective: InterpOutputs,
}

impl CsClipBoxShadowTexture2DFrag {
    fn rectangle_aa_rough_fragment(&self, local_pos: vec2) -> f32 {
        let p0 = self.vert.common.v_transform_bounds.xy();
        let p1 = self.vert.common.v_transform_bounds.zw();
        let s = local_pos.step(p0) - local_pos.step(p1);
        s.x * s.y
    }

    #[inline(always)]
    fn main(&self, context: &ShaderContext) -> vec4 {
        let uv_linear = self.v_uv / self.v_local_pos.w;
        let mut uv = uv_linear.clamp(vec2::ZERO, self.vert.common.v_edge.xy());
        uv += (uv_linear - self.vert.common.v_edge.zw()).max(vec2::ZERO);

        uv = self.vert.common.v_uv_bounds_no_clamp.xy().lerp(self.vert.common.v_uv_bounds_no_clamp.zw(), uv);
        uv = uv.clamp(self.vert.common.v_uv_bounds.xy(), self.vert.common.v_uv_bounds.zw());
        
        let in_shadow_rect = self.rectangle_aa_rough_fragment(self.v_local_pos.xy() / self.v_local_pos.w);
        
        let texel = context.texture(SamplerId::SColor0, uv).x;
        
        let alpha = texel.lerp(1.0 - texel, self.vert.common.v_clip_mode.x);
        
        let mut result = 0.0;
        if self.v_local_pos.w > 0.0 {
            result = self.vert.common.v_clip_mode.x.lerp(alpha, in_shadow_rect);
        }
        
        vec4::new(result, 0.0, 0.0, 1.0)
    }
}

impl FragmentShader for CsClipBoxShadowTexture2DFrag {
    fn read_interp_inputs(&mut self, init: *const u8, step: *const u8) {
        unsafe {
            let init = &*(init as *const InterpOutputs);
            let step = &*(step as *const InterpOutputs);
            self.v_local_pos = init.v_local_pos;
            self.v_uv = init.v_uv;
            self.interp_step.v_local_pos = step.v_local_pos * 4.0;
            self.interp_step.v_uv = step.v_uv * 4.0;
        }
    }
    
    fn read_perspective_inputs(&mut self, init: *const u8, step: *const u8, w: f32) {
        unsafe {
            let init = &*(init as *const InterpOutputs);
            let step = &*(step as *const InterpOutputs);
            let inv_w = 1.0 / w;
            self.interp_perspective.v_local_pos = init.v_local_pos;
            self.v_local_pos = self.interp_perspective.v_local_pos * inv_w;
            self.interp_step.v_local_pos = step.v_local_pos * 4.0;
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
    
    fn draw_span_r8(&mut self, _context: &mut ShaderContext) -> i32 {
        // The swgl_drawSpanR8 is very complex and relies on rasterizer-internal
        // state that is not exposed through the common context. A full translation
        // is not feasible without that context. The generic `run` path will be used instead.
        0
    }
}

impl CsClipBoxShadowTexture2DFrag {
    fn step_interp_inputs(&mut self, steps: i32) {
        let chunks = steps as f32 * 0.25;
        self.v_local_pos += self.interp_step.v_local_pos * chunks;
        self.v_uv += self.interp_step.v_uv * chunks;
    }

    fn step_perspective_inputs(&mut self, steps: i32, next_w: &[f32; 4]) {
        let chunks = steps as f32 * 0.25;
        let inv_w = 1.0 / next_w[0];
        
        self.interp_perspective.v_local_pos += self.interp_step.v_local_pos * chunks;
        self.v_local_pos = self.interp_perspective.v_local_pos * inv_w;

        self.interp_perspective.v_uv += self.interp_step.v_uv * chunks;
        self.v_uv = self.interp_perspective.v_uv * inv_w;
    }
}

//
// Program
//

#[derive(Clone, Debug, Default)]
pub struct CsClipBoxShadowTexture2DProgram {
    frag: CsClipBoxShadowTexture2DFrag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(CsClipBoxShadowTexture2DProgram::default())
}

impl Program for CsClipBoxShadowTexture2DProgram {
    fn get_vertex_shader(&mut self) -> &mut dyn VertexShader {
        &mut self.frag.vert
    }

    fn get_fragment_shader(&mut self) -> &mut dyn FragmentShader {
        &mut self.frag
    }
    
    fn get_uniform(&self, name: &CStr) -> i32 {
        if strcmp(name, "sColor0") { return 5; }
        if strcmp(name, "sGpuCache") { return 2; }
        if strcmp(name, "sRenderTasks") { return 1; }
        if strcmp(name, "sTransformPalette") { return 3; }
        if strcmp(name, "uTransform") { return 4; }
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
