use crate::common::*;
use crate::common_types::*;
use crate::traits::{FragmentShader, Program, VertexShader};
use glam::{ivec2, vec2, vec3, vec4, Mat3, Mat4, Vec2, Vec3, Vec4};
use std::ffi::CStr;
use std::mem;
use super::strcmp;

//
// C++ Constant Definitions
//

const MIX_BLEND_MODE_MULTIPLY: i32 = 1;
const MIX_BLEND_MODE_SCREEN: i32 = 2;
const MIX_BLEND_MODE_OVERLAY: i32 = 3;
const MIX_BLEND_MODE_DARKEN: i32 = 4;
const MIX_BLEND_MODE_LIGHTEN: i32 = 5;
const MIX_BLEND_MODE_COLOR_DODGE: i32 = 6;
const MIX_BLEND_MODE_COLOR_BURN: i32 = 7;
const MIX_BLEND_MODE_HARD_LIGHT: i32 = 8;
const MIX_BLEND_MODE_SOFT_LIGHT: i32 = 9;
const MIX_BLEND_MODE_DIFFERENCE: i32 = 10;
const MIX_BLEND_MODE_EXCLUSION: i32 = 11;
const MIX_BLEND_MODE_HUE: i32 = 12;
const MIX_BLEND_MODE_SATURATION: i32 = 13;
const MIX_BLEND_MODE_COLOR: i32 = 14;
const MIX_BLEND_MODE_LUMINOSITY: i32 = 15;
// const MIX_BLEND_MODE_PLUS_LIGHTER: i32 = 16; // Not used in the translated switch

//
// Blend Functions
//

fn multiply(cb: Vec3, cs: Vec3) -> Vec3 {
    cb * cs
}

fn screen(cb: Vec3, cs: Vec3) -> Vec3 {
    cb + cs - (cb * cs)
}

fn hard_light(cb: Vec3, cs: Vec3) -> Vec3 {
    let m = multiply(cb, 2.0 * cs);
    let s = screen(cb, (2.0 * cs) - 1.0);
    let edge = vec3(0.5, 0.5, 0.5);
    if cs.x > edge.x { s.x } else { m.x }.extend(if cs.y > edge.y { s.y } else { m.y }, if cs.z > edge.z { s.z } else { m.z })
}

fn color_dodge(cb: f32, cs: f32) -> f32 {
    if cb == 0.0 {
        0.0
    } else if cs == 1.0 {
        1.0
    } else {
        (1.0f32).min(cb / (1.0 - cs))
    }
}

fn color_burn(cb: f32, cs: f32) -> f32 {
    if cb == 1.0 {
        1.0
    } else if cs == 0.0 {
        0.0
    } else {
        1.0 - (1.0f32).min((1.0 - cb) / cs)
    }
}

fn soft_light(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 {
        cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb)
    } else {
        let d = if cb <= 0.25 {
            ((16.0 * cb - 12.0) * cb + 4.0) * cb
        } else {
            cb.sqrt()
        };
        cb + (2.0 * cs - 1.0) * (d - cb)
    }
}

fn difference(cb: Vec3, cs: Vec3) -> Vec3 {
    (cb - cs).abs()
}

fn lum(c: Vec3) -> f32 {
    c.dot(vec3(0.3, 0.59, 0.11))
}

fn clip_color(c: Vec3) -> Vec3 {
    let mut c = c;
    let l = lum(c);
    let n = c.x.min(c.y.min(c.z));
    let x = c.x.max(c.y.max(c.z));
    if n < 0.0 {
        c = l + (c - l) * l / (l - n);
    }
    if x > 1.0 {
        c = l + (c - l) * (1.0 - l) / (x - l);
    }
    c
}

fn set_lum(c: Vec3, l: f32) -> Vec3 {
    let d = l - lum(c);
    clip_color(c + d)
}

fn set_sat_inner(c_min: &mut f32, c_mid: &mut f32, c_max: &mut f32, s: f32) {
    if *c_max > *c_min {
        *c_mid = ((*c_mid - *c_min) * s) / (*c_max - *c_min);
        *c_max = s;
    } else {
        *c_mid = 0.0;
        *c_max = 0.0;
    }
    *c_min = 0.0;
}

fn set_sat(mut c: Vec3, s: f32) -> Vec3 {
    if c.x <= c.y {
        if c.y <= c.z {
            set_sat_inner(&mut c.x, &mut c.y, &mut c.z, s);
        } else if c.x <= c.z {
            set_sat_inner(&mut c.x, &mut c.z, &mut c.y, s);
        } else {
            set_sat_inner(&mut c.z, &mut c.x, &mut c.y, s);
        }
    } else {
        if c.x <= c.z {
            set_sat_inner(&mut c.y, &mut c.x, &mut c.z, s);
        } else if c.y <= c.z {
            set_sat_inner(&mut c.y, &mut c.z, &mut c.x, s);
        } else {
            set_sat_inner(&mut c.z, &mut c.y, &mut c.x, s);
        }
    }
    c
}

fn sat(c: Vec3) -> f32 {
    c.x.max(c.y.max(c.z)) - c.x.min(c.y.min(c.z))
}

fn hue(cb: Vec3, cs: Vec3) -> Vec3 {
    set_lum(set_sat(cs, sat(cb)), lum(cb))
}

fn saturation(cb: Vec3, cs: Vec3) -> Vec3 {
    set_lum(set_sat(cb, sat(cs)), lum(cb))
}

fn color(cb: Vec3, cs: Vec3) -> Vec3 {
    set_lum(cs, lum(cb))
}

fn luminosity(cb: Vec3, cs: Vec3) -> Vec3 {
    set_lum(cb, lum(cs))
}

//
// Common Struct
//

#[derive(Clone, Debug, Default)]
struct BrushMixBlendCommon {
    attrib_locations: AttribLocations,
    u_transform: mat4,
    // Flat varyings
    v_src_uv_sample_bounds: vec4,
    v_backdrop_uv_sample_bounds: vec4,
    v_perspective: vec2,
    v_op: ivec2,
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
struct BrushMixBlendVert {
    common: BrushMixBlendCommon,
    // Inputs
    a_position: vec2,
    a_data: ivec4,
    // Outputs
    v_src_uv: vec2,
    v_backdrop_uv: vec2,
    gl_position: vec4,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct InterpOutputs {
    v_src_uv: vec2,
    v_backdrop_uv: vec2,
}

impl BrushMixBlendVert {
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
        ph: PrimitiveHeader,
        transform: Transform,
        task: PictureTask,
        clip_area: ClipArea,
    ) {
        let brush_flags = instance.flags & 4095;
        let segment_rect = ph.local_rect;
        let adjusted_segment_rect = segment_rect;

        let local_pos =
            adjusted_segment_rect.p0.lerp(adjusted_segment_rect.p1, self.a_position);
        
        let (gl_pos, vi) = context.write_vertex(local_pos, ph.local_clip_rect, ph.z, &transform, &task, &self.common.u_transform);
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
        let f = (vi.local_pos - local_rect.p0) / context.rect_size(local_rect);
        let perspective_interpolate = if (brush_flags & 1) != 0 { 1.0 } else { 0.0 };
        let perspective_f = vi.world_pos.w.lerp(1.0, perspective_interpolate);
        self.common.v_perspective.x = perspective_interpolate;

        self.common.v_op.x = prim_user_data.x;
        
        // Backdrop (sColor0)
        let (backdrop_uv, backdrop_bounds) = context.get_uv(prim_user_data.y, f, SamplerId::SColor0, 1.0);
        self.v_backdrop_uv = backdrop_uv;
        self.common.v_backdrop_uv_sample_bounds = backdrop_bounds;

        // Source (sColor1)
        let (src_uv, src_bounds) = context.get_uv(prim_user_data.z, f, SamplerId::SColor1, perspective_f);
        self.v_src_uv = src_uv;
        self.common.v_src_uv_sample_bounds = src_bounds;
    }
}

impl VertexShader for BrushMixBlendVert {
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

    fn run_primitive(&mut self, context: &ShaderContext, interps: *mut u8, interp_stride: usize) {
        self.main(context);

        unsafe {
            let mut dest_ptr = interps as *mut InterpOutputs;
            for _ in 0..4 {
                (*dest_ptr).v_src_uv = self.v_src_uv;
                (*dest_ptr).v_backdrop_uv = self.v_backdrop_uv;
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
struct BrushMixBlendFrag {
    vert: BrushMixBlendVert,
    // Varying inputs
    v_src_uv: vec2,
    v_backdrop_uv: vec2,
    interp_step: InterpOutputs,
    interp_perspective: InterpOutputs,
}

impl BrushMixBlendFrag {
    fn main(&self, context: &ShaderContext) -> vec4 {
        let gl_frag_coord_w = 1.0; // Placeholder for rasterizer-provided gl_FragCoord.w

        let perspective_divisor = (1.0 - self.vert.common.v_perspective.x) * gl_frag_coord_w + self.vert.common.v_perspective.x;
        let src_uv = (self.v_src_uv * perspective_divisor)
            .clamp(self.vert.common.v_src_uv_sample_bounds.xy(), self.vert.common.v_src_uv_sample_bounds.zw());
        let backdrop_uv = self.v_backdrop_uv
            .clamp(self.vert.common.v_backdrop_uv_sample_bounds.xy(), self.vert.common.v_backdrop_uv_sample_bounds.zw());

        let mut cb = context.texture(SamplerId::SColor0, backdrop_uv);
        let mut cs = context.texture(SamplerId::SColor1, src_uv);

        if cb.w != 0.0 {
            cb.truncate() /= cb.w;
        }
        if cs.w != 0.0 {
            cs.truncate() /= cs.w;
        }

        let mut result_rgb = match self.vert.common.v_op.x & 255 {
            MIX_BLEND_MODE_MULTIPLY => multiply(cb.truncate(), cs.truncate()),
            MIX_BLEND_MODE_OVERLAY => hard_light(cs.truncate(), cb.truncate()),
            MIX_BLEND_MODE_DARKEN => cs.truncate().min(cb.truncate()),
            MIX_BLEND_MODE_LIGHTEN => cs.truncate().max(cb.truncate()),
            MIX_BLEND_MODE_COLOR_DODGE => vec3(color_dodge(cb.x, cs.x), color_dodge(cb.y, cs.y), color_dodge(cb.z, cs.z)),
            MIX_BLEND_MODE_COLOR_BURN => vec3(color_burn(cb.x, cs.x), color_burn(cb.y, cs.y), color_burn(cb.z, cs.z)),
            MIX_BLEND_MODE_HARD_LIGHT => hard_light(cb.truncate(), cs.truncate()),
            MIX_BLEND_MODE_SOFT_LIGHT => vec3(soft_light(cb.x, cs.x), soft_light(cb.y, cs.y), soft_light(cb.z, cs.z)),
            MIX_BLEND_MODE_DIFFERENCE => difference(cb.truncate(), cs.truncate()),
            MIX_BLEND_MODE_HUE => hue(cb.truncate(), cs.truncate()),
            MIX_BLEND_MODE_SATURATION => saturation(cb.truncate(), cs.truncate()),
            MIX_BLEND_MODE_COLOR => color(cb.truncate(), cs.truncate()),
            MIX_BLEND_MODE_LUMINOSITY => luminosity(cb.truncate(), cs.truncate()),
            // The C++ switch is missing cases for Screen, Exclusion, PlusLighter
            _ => vec3(1.0, 1.0, 0.0), // Default case from C++
        };
        
        result_rgb = (1.0 - cb.w) * cs.truncate() + cb.w * result_rgb;
        let mut result = result_rgb.extend(cs.w);
        result.truncate() *= result.w;

        result
    }
}

impl FragmentShader for BrushMixBlendFrag {
    fn read_interp_inputs(&mut self, init: *const u8, step: *const u8) {
        unsafe {
            let init = &*(init as *const InterpOutputs);
            let step = &*(step as *const InterpOutputs);
            self.v_src_uv = init.v_src_uv;
            self.v_backdrop_uv = init.v_backdrop_uv;
            self.interp_step.v_src_uv = step.v_src_uv * 4.0;
            self.interp_step.v_backdrop_uv = step.v_backdrop_uv * 4.0;
        }
    }

    fn read_perspective_inputs(&mut self, init: *const u8, step: *const u8, w: f32) {
        unsafe {
            let init = &*(init as *const InterpOutputs);
            let step = &*(step as *const InterpOutputs);
            let inv_w = 1.0 / w;
            self.interp_perspective.v_src_uv = init.v_src_uv;
            self.v_src_uv = self.interp_perspective.v_src_uv * inv_w;
            self.interp_step.v_src_uv = step.v_src_uv * 4.0;
            
            self.interp_perspective.v_backdrop_uv = init.v_backdrop_uv;
            self.v_backdrop_uv = self.interp_perspective.v_backdrop_uv * inv_w;
            self.interp_step.v_backdrop_uv = step.v_backdrop_uv * 4.0;
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

impl BrushMixBlendFrag {
    fn step_interp_inputs(&mut self, steps: i32) {
        let chunks = steps as f32 * 0.25;
        self.v_src_uv += self.interp_step.v_src_uv * chunks;
        self.v_backdrop_uv += self.interp_step.v_backdrop_uv * chunks;
    }

    fn step_perspective_inputs(&mut self, steps: i32, next_w: &[f32; 4]) {
        let chunks = steps as f32 * 0.25;
        let inv_w = 1.0 / next_w[0];
        self.interp_perspective.v_src_uv += self.interp_step.v_src_uv * chunks;
        self.v_src_uv = self.interp_perspective.v_src_uv * inv_w;
        
        self.interp_perspective.v_backdrop_uv += self.interp_step.v_backdrop_uv * chunks;
        self.v_backdrop_uv = self.interp_perspective.v_backdrop_uv * inv_w;
    }
}

//
// Program
//

#[derive(Clone, Debug, Default)]
pub struct BrushMixBlendProgram {
    frag: BrushMixBlendFrag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(BrushMixBlendProgram::default())
}

impl Program for BrushMixBlendProgram {
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
