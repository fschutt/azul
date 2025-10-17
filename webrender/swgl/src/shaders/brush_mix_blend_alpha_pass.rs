use crate::common::*;
use crate::common_types::*;
use crate::traits::{FragmentShader, Program, VertexShader};
use glam::{ivec2, ivec4, mat3, mat4, vec2, vec3, vec4, Mat3, Mat4, Vec2, Vec3, Vec4};
use std::ffi::CStr;
use std::mem;
use super::strcmp;

//
// C++ Constant Definitions for Blend Modes
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
const MIX_BLEND_MODE_PLUS_LIGHTER: i32 = 16;

//
// Common Struct
//
#[derive(Clone, Debug, Default)]
struct BrushMixBlendAlphaPassCommon {
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
struct BrushMixBlendAlphaPassVert {
    common: BrushMixBlendAlphaPassCommon,
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

impl BrushMixBlendAlphaPassVert {
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
            let segment_address = ph.specific_prim_address + 3 + (instance.segment_index * 2);
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
        
        let (backdrop_uv, backdrop_uv_sample_bounds) = context.get_uv(
            prim_user_data.y,
            f,
            context.texture_size(SamplerId::SColor0, 0),
            1.0, // backdrop is not perspective corrected
        );
        self.v_backdrop_uv = backdrop_uv;
        self.common.v_backdrop_uv_sample_bounds = backdrop_uv_sample_bounds;

        let (src_uv, src_uv_sample_bounds) = context.get_uv(
            prim_user_data.z,
            f,
            context.texture_size(SamplerId::SColor1, 0),
            perspective_f,
        );
        self.v_src_uv = src_uv;
        self.common.v_src_uv_sample_bounds = src_uv_sample_bounds;
    }
}

impl VertexShader for BrushMixBlendAlphaPassVert {
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
                (*dest_ptr).v_src_uv = self.v_src_uv;
                (*dest_ptr).v_backdrop_uv = self.v_backdrop_uv;
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
struct BrushMixBlendAlphaPassFrag {
    vert: BrushMixBlendAlphaPassVert,
    // Varying inputs
    v_src_uv: vec2,
    v_backdrop_uv: vec2,
    interp_step: InterpOutputs,
    interp_perspective: InterpOutputs,
}

impl BrushMixBlendAlphaPassFrag {
    fn antialias_brush(&self) -> f32 { 1.0 }
    fn do_clip(&self, _context: &ShaderContext) -> f32 { 1.0 }

    fn main(&self, context: &ShaderContext) -> vec4 {
        let mut frag_color = self.brush_fs(context);
        let clip_alpha = self.do_clip(context);
        frag_color *= clip_alpha;
        frag_color
    }

    fn brush_fs(&self, context: &ShaderContext) -> vec4 {
        let gl_frag_coord_w = 1.0; // Placeholder

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
            MIX_BLEND_MODE_MULTIPLY => multiply(cb.rgb(), cs.rgb()),
            MIX_BLEND_MODE_OVERLAY => hard_light(cs.rgb(), cb.rgb()),
            MIX_BLEND_MODE_DARKEN => cs.rgb().min(cb.rgb()),
            MIX_BLEND_MODE_LIGHTEN => cs.rgb().max(cb.rgb()),
            MIX_BLEND_MODE_COLOR_DODGE => vec3(
                color_dodge(cb.x, cs.x),
                color_dodge(cb.y, cs.y),
                color_dodge(cb.z, cs.z),
            ),
            MIX_BLEND_MODE_COLOR_BURN => vec3(
                color_burn(cb.x, cs.x),
                color_burn(cb.y, cs.y),
                color_burn(cb.z, cs.z),
            ),
            MIX_BLEND_MODE_HARD_LIGHT => hard_light(cb.rgb(), cs.rgb()),
            MIX_BLEND_MODE_SOFT_LIGHT => vec3(
                soft_light(cb.x, cs.x),
                soft_light(cb.y, cs.y),
                soft_light(cb.z, cs.z),
            ),
            MIX_BLEND_MODE_DIFFERENCE => difference(cb.rgb(), cs.rgb()),
            MIX_BLEND_MODE_HUE => hue(cb.rgb(), cs.rgb()),
            MIX_BLEND_MODE_SATURATION => saturation(cb.rgb(), cs.rgb()),
            MIX_BLEND_MODE_COLOR => color(cb.rgb(), cs.rgb()),
            MIX_BLEND_MODE_LUMINOSITY => luminosity(cb.rgb(), cs.rgb()),
            _ => vec3(1.0, 1.0, 0.0), // Should not happen for handled modes
        };

        result_rgb = ((1.0 - cb.w) * cs.rgb()) + (cb.w * result_rgb);
        let result_w = cs.w;
        result_rgb *= result_w;

        vec4(result_rgb.x, result_rgb.y, result_rgb.z, result_w)
    }
}

impl FragmentShader for BrushMixBlendAlphaPassFrag {
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
}

impl BrushMixBlendAlphaPassFrag {
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

// Blend functions
fn multiply(cb: vec3, cs: vec3) -> vec3 { cb * cs }
fn screen(cb: vec3, cs: vec3) -> vec3 { cb + cs - (cb * cs) }
fn hard_light(cb: vec3, cs: vec3) -> vec3 {
    let m = multiply(cb, 2.0 * cs);
    let s = screen(cb, 2.0 * cs - 1.0);
    let edge = vec3::splat(0.5);
    edge.lerp(s, cs.cmpge(edge))
}
fn color_dodge(cb: f32, cs: f32) -> f32 {
    if cb == 0.0 { 0.0 } else if cs == 1.0 { 1.0 } else { (cb / (1.0 - cs)).min(1.0) }
}
fn color_burn(cb: f32, cs: f32) -> f32 {
    if cb == 1.0 { 1.0 } else if cs == 0.0 { 0.0 } else { 1.0 - (1.0 - (1.0 - cb) / cs).min(1.0) }
}
fn soft_light(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 {
        cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb)
    } else {
        let d = if cb <= 0.25 { ((16.0 * cb - 12.0) * cb + 4.0) * cb } else { cb.sqrt() };
        cb + (2.0 * cs - 1.0) * (d - cb)
    }
}
fn difference(cb: vec3, cs: vec3) -> vec3 { (cb - cs).abs() }
fn lum(c: vec3) -> f32 { c.dot(vec3::new(0.3, 0.59, 0.11)) }
fn clip_color(c: vec3) -> vec3 {
    let l = lum(c);
    let n = c.x.min(c.y.min(c.z));
    let x = c.x.max(c.y.max(c.z));
    let mut c_out = c;
    if n < 0.0 { c_out = l + (((c - l) * l) / (l - n)); }
    if x > 1.0 { c_out = l + (((c_out - l) * (1.0 - l)) / (x - l)); }
    c_out
}
fn set_lum(c: vec3, l: f32) -> vec3 { let d = l - lum(c); clip_color(c + d) }
fn sat(c: vec3) -> f32 { c.x.max(c.y.max(c.z)) - c.x.min(c.y.min(c.z)) }
fn set_sat(mut c: vec3, s: f32) -> vec3 {
    let mut components = [(c.x, 0), (c.y, 1), (c.z, 2)];
    components.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    if components[2].0 > components[0].0 {
        components[1].0 = (components[1].0 - components[0].0) * s / (components[2].0 - components[0].0);
        components[2].0 = s;
    } else {
        components[1].0 = 0.0;
        components[2].0 = 0.0;
    }
    components[0].0 = 0.0;
    components.sort_by_key(|k| k.1);
    vec3(components[0].0, components[1].0, components[2].0)
}
fn hue(cb: vec3, cs: vec3) -> vec3 { set_lum(set_sat(cs, sat(cb)), lum(cb)) }
fn saturation(cb: vec3, cs: vec3) -> vec3 { set_lum(set_sat(cb, sat(cs)), lum(cb)) }
fn color(cb: vec3, cs: vec3) -> vec3 { set_lum(cs, lum(cb)) }
fn luminosity(cb: vec3, cs: vec3) -> vec3 { set_lum(cb, lum(cs)) }

//
// Program
//
#[derive(Clone, Debug, Default)]
pub struct BrushMixBlendAlphaPassProgram {
    frag: BrushMixBlendAlphaPassFrag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(BrushMixBlendAlphaPassProgram::default())
}

impl Program for BrushMixBlendAlphaPassProgram {
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
