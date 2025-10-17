use crate::common::*;
use crate::common_types::*;
use crate::traits::{FragmentShader, Program, VertexShader};
use glam::{ivec2, mat4, vec2, vec3, vec4, Mat4, Vec2, Vec3, Vec4};
use std::ffi::CStr;
use std::mem;
use super::strcmp;

//
// Common Struct
//

#[derive(Clone, Debug, Default)]
struct CsBorderSegmentCommon {
    attrib_locations: AttribLocations,
    u_transform: mat4,
    // Flat varyings
    v_color00: vec4,
    v_color01: vec4,
    v_color10: vec4,
    v_color11: vec4,
    v_color_line: vec4,
    v_segment_clip_mode: vec2,
    v_style_edge_axis: vec4,
    v_clip_center_sign: vec4,
    v_clip_radii: vec4,
    v_edge_reference: vec4,
    v_partial_widths: vec4,
    v_clip_params1: vec4,
    v_clip_params2: vec4,
}

#[derive(Clone, Debug, Default)]
struct AttribLocations {
    a_position: usize,
    a_task_origin: usize,
    a_rect: usize,
    a_color0: usize,
    a_color1: usize,
    a_flags: usize,
    a_widths: usize,
    a_radii: usize,
    a_clip_params1: usize,
    a_clip_params2: usize,
}

impl AttribLocations {
    fn bind_loc(&mut self, name: &CStr, index: i32) {
        if strcmp(name, "aPosition") { self.a_position = index as usize; }
        else if strcmp(name, "aTaskOrigin") { self.a_task_origin = index as usize; }
        else if strcmp(name, "aRect") { self.a_rect = index as usize; }
        else if strcmp(name, "aColor0") { self.a_color0 = index as usize; }
        else if strcmp(name, "aColor1") { self.a_color1 = index as usize; }
        else if strcmp(name, "aFlags") { self.a_flags = index as usize; }
        else if strcmp(name, "aWidths") { self.a_widths = index as usize; }
        else if strcmp(name, "aRadii") { self.a_radii = index as usize; }
        else if strcmp(name, "aClipParams1") { self.a_clip_params1 = index as usize; }
        else if strcmp(name, "aClipParams2") { self.a_clip_params2 = index as usize; }
    }

    fn get_loc(&self, name: &CStr) -> i32 {
        if strcmp(name, "aPosition") { if self.a_position != NULL_ATTRIB { self.a_position as i32 } else { -1 } }
        else if strcmp(name, "aTaskOrigin") { if self.a_task_origin != NULL_ATTRIB { self.a_task_origin as i32 } else { -1 } }
        else if strcmp(name, "aRect") { if self.a_rect != NULL_ATTRIB { self.a_rect as i32 } else { -1 } }
        else if strcmp(name, "aColor0") { if self.a_color0 != NULL_ATTRIB { self.a_color0 as i32 } else { -1 } }
        else if strcmp(name, "aColor1") { if self.a_color1 != NULL_ATTRIB { self.a_color1 as i32 } else { -1 } }
        else if strcmp(name, "aFlags") { if self.a_flags != NULL_ATTRIB { self.a_flags as i32 } else { -1 } }
        else if strcmp(name, "aWidths") { if self.a_widths != NULL_ATTRIB { self.a_widths as i32 } else { -1 } }
        else if strcmp(name, "aRadii") { if self.a_radii != NULL_ATTRIB { self.a_radii as i32 } else { -1 } }
        else if strcmp(name, "aClipParams1") { if self.a_clip_params1 != NULL_ATTRIB { self.a_clip_params1 as i32 } else { -1 } }
        else if strcmp(name, "aClipParams2") { if self.a_clip_params2 != NULL_ATTRIB { self.a_clip_params2 as i32 } else { -1 } }
        else { -1 }
    }
}

//
// Vertex Shader
//

#[derive(Clone, Debug, Default)]
struct CsBorderSegmentVert {
    common: CsBorderSegmentCommon,
    // Inputs
    a_position: vec2,
    a_task_origin: vec2,
    a_rect: vec4,
    a_color0: vec4,
    a_color1: vec4,
    a_flags: i32,
    a_widths: vec2,
    a_radii: vec2,
    a_clip_params1: vec4,
    a_clip_params2: vec4,
    // Outputs
    v_pos: vec2,
    gl_position: vec4,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct InterpOutputs {
    v_pos: vec2,
}

impl CsBorderSegmentVert {
    fn get_outer_corner_scale(&self, segment: i32) -> vec2 {
        match segment {
            0 => vec2(0.0, 0.0),
            1 => vec2(1.0, 0.0),
            2 => vec2(1.0, 1.0),
            3 => vec2(0.0, 1.0),
            _ => vec2::ZERO,
        }
    }

    fn mod_color(&self, color: vec4, is_black: bool, lighter: bool) -> vec4 {
        const LIGHT_BLACK: f32 = 0.7;
        const DARK_BLACK: f32 = 0.3;
        const DARK_SCALE: f32 = 0.6666667;
        const LIGHT_SCALE: f32 = 1.0;

        if is_black {
            if lighter {
                vec4::new(LIGHT_BLACK, LIGHT_BLACK, LIGHT_BLACK, color.w)
            } else {
                vec4::new(DARK_BLACK, DARK_BLACK, DARK_BLACK, color.w)
            }
        } else {
            if lighter {
                vec4::new(color.x * LIGHT_SCALE, color.y * LIGHT_SCALE, color.z * LIGHT_SCALE, color.w)
            } else {
                vec4::new(color.x * DARK_SCALE, color.y * DARK_SCALE, color.z * DARK_SCALE, color.w)
            }
        }
    }

    fn get_colors_for_side(&self, color: vec4, style: i32) -> [vec4; 2] {
        let is_black = color.xyz() == vec3::ZERO;
        match style {
            6 => [self.mod_color(color, is_black, true), self.mod_color(color, is_black, false)],
            7 => [self.mod_color(color, is_black, false), self.mod_color(color, is_black, true)],
            _ => [color, color],
        }
    }
    
    #[inline(always)]
    fn main(&mut self) {
        let segment = self.a_flags & 0xFF;
        let style0 = (self.a_flags >> 8) & 0xFF;
        let style1 = (self.a_flags >> 16) & 0xFF;
        let clip_mode = (self.a_flags >> 24) & 0xF;

        let size = self.a_rect.zw() - self.a_rect.xy();
        let outer_scale = self.get_outer_corner_scale(segment);
        let outer = outer_scale * size;
        let clip_sign = vec2::ONE - (2.0 * outer_scale);
        
        let mut edge_axis = ivec2::ZERO;
        let mut edge_reference = vec2::ZERO;

        match segment {
            0 => {
                edge_axis = ivec2::new(0, 1);
                edge_reference = outer;
            },
            1 => {
                edge_axis = ivec2::new(1, 0);
                edge_reference = vec2::new(outer.x - self.a_widths.x, outer.y);
            },
            2 => {
                edge_axis = ivec2::new(0, 1);
                edge_reference = outer - self.a_widths;
            },
            3 => {
                edge_axis = ivec2::new(1, 0);
                edge_reference = vec2::new(outer.x, outer.y - self.a_widths.y);
            },
            5 | 7 => {
                edge_axis = ivec2::new(1, 1);
            },
            _ => {} // 4, 6, default
        }

        let do_aa = ((self.a_flags >> 24) & 0xF0) != 0;
        let mix_colors = match segment {
            0..=3 => if do_aa { 1 } else { 2 },
            _ => 0,
        };

        self.common.v_mix_colors.x = mix_colors;
        
        self.common.v_segment_clip_mode = vec2::new(segment as f32, clip_mode as f32);
        self.common.v_style_edge_axis = vec4::new(style0 as f32, style1 as f32, edge_axis.x as f32, edge_axis.y as f32);
        self.common.v_partial_widths = vec4::new(self.a_widths.x / 3.0, self.a_widths.y / 3.0, self.a_widths.x / 2.0, self.a_widths.y / 2.0);

        self.v_pos = self.a_position.xy().lerp(self.a_position.yx(), self.a_axis_select) * size;
        
        let colors0 = self.get_colors_for_side(self.a_color0, style0);
        self.common.v_color00 = colors0[0];
        self.common.v_color01 = colors0[1];

        let colors1 = self.get_colors_for_side(self.a_color1, style1);
        self.common.v_color10 = colors1[0];
        self.common.v_color11 = colors1[1];
        
        self.common.v_clip_center_sign = vec4::new(outer.x + clip_sign.x * self.a_radii.x, outer.y + clip_sign.y * self.a_radii.y, clip_sign.x, clip_sign.y);
        self.common.v_clip_radii = vec4::new(self.a_radii.x, self.a_radii.y, (self.a_radii - self.a_widths).max(vec2::ZERO).x, (self.a_radii - self.a_widths).max(vec2::ZERO).y);
        self.common.v_color_line = vec4::new(outer.x, outer.y, self.a_widths.y * -clip_sign.y, self.a_widths.x * clip_sign.x);
        self.common.v_edge_reference = vec4::new(edge_reference.x, edge_reference.y, (edge_reference + self.a_widths).x, (edge_reference + self.a_widths).y);
        
        self.common.v_clip_params1 = self.a_clip_params1;
        self.common.v_clip_params2 = self.a_clip_params2;

        if clip_mode == 3 {
            let mut radius = self.a_clip_params1.z;
            if radius > 0.5 {
                radius += 2.0;
            }
            self.v_pos = self.a_clip_params1.xy() + radius * (2.0 * self.a_position - 1.0);
            self.v_pos = self.v_pos.clamp(vec2::ZERO, size);
        } else if clip_mode == 1 {
            let center = (self.a_clip_params1.xy() + self.a_clip_params2.xy()) * 0.5;
            let dash_length = (self.a_clip_params1.xy() - self.a_clip_params2.xy()).length();
            let width = self.a_widths.x.max(self.a_widths.y);
            let r = vec2::splat(dash_length.max(width)) + 2.0;
            self.v_pos = self.v_pos.clamp(center - r, center + r);
        }
        
        self.gl_position = self.common.u_transform * vec4::new(self.a_task_origin.x + self.a_rect.x + self.v_pos.x, self.a_task_origin.y + self.a_rect.y + self.v_pos.y, 0.0, 1.0);
    }
}

impl VertexShader for CsBorderSegmentVert {
    fn init_batch(&mut self, _context: &mut ShaderContext) {}

    fn load_attribs(&mut self, attribs: &[&VertexAttrib], start: u32, instance: i32, _count: i32) {
        unsafe {
            let a_position_attrib = &*attribs[self.common.attrib_locations.a_position];
            let a_task_origin_attrib = &*attribs[self.common.attrib_locations.a_task_origin];
            let a_rect_attrib = &*attribs[self.common.attrib_locations.a_rect];
            let a_color0_attrib = &*attribs[self.common.attrib_locations.a_color0];
            let a_color1_attrib = &*attribs[self.common.attrib_locations.a_color1];
            let a_flags_attrib = &*attribs[self.common.attrib_locations.a_flags];
            let a_widths_attrib = &*attribs[self.common.attrib_locations.a_widths];
            let a_radii_attrib = &*attribs[self.common.attrib_locations.a_radii];
            let a_clip_params1_attrib = &*attribs[self.common.attrib_locations.a_clip_params1];
            let a_clip_params2_attrib = &*attribs[self.common.attrib_locations.a_clip_params2];

            let pos_ptr = (a_position_attrib.data as *const u8).add(a_position_attrib.stride * start as usize) as *const Vec2;
            self.a_position = *pos_ptr;

            let instance_offset = (a_task_origin_attrib.stride * instance as usize) as isize;
            self.a_task_origin = *((a_task_origin_attrib.data as *const u8).offset(instance_offset) as *const Vec2);
            self.a_rect = *((a_rect_attrib.data as *const u8).offset(instance_offset) as *const Vec4);
            self.a_color0 = *((a_color0_attrib.data as *const u8).offset(instance_offset) as *const Vec4);
            self.a_color1 = *((a_color1_attrib.data as *const u8).offset(instance_offset) as *const Vec4);
            self.a_flags = *((a_flags_attrib.data as *const u8).offset(instance_offset) as *const i32);
            self.a_widths = *((a_widths_attrib.data as *const u8).offset(instance_offset) as *const Vec2);
            self.a_radii = *((a_radii_attrib.data as *const u8).offset(instance_offset) as *const Vec2);
            self.a_clip_params1 = *((a_clip_params1_attrib.data as *const u8).offset(instance_offset) as *const Vec4);
            self.a_clip_params2 = *((a_clip_params2_attrib.data as *const u8).offset(instance_offset) as *const Vec4);
        }
    }

    fn run_primitive(&mut self, _context: &mut ShaderContext, interps: *mut u8, interp_stride: usize) {
        self.main();

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
        if index == 1 { // uTransform
            self.common.u_transform = Mat4::from_cols_array(value);
        }
    }
}

//
// Fragment Shader
//

#[derive(Clone, Debug, Default)]
struct CsBorderSegmentFrag {
    vert: CsBorderSegmentVert,
    // Varying inputs
    v_pos: vec2,
    interp_step: InterpOutputs,
    interp_perspective: InterpOutputs,
}

impl CsBorderSegmentFrag {
    #[inline(always)]
    fn main(&self, context: &ShaderContext) -> vec4 {
        let aa_range = context.fwidth(self.v_pos).x.recip();
        let do_aa = self.vert.common.v_mix_colors.x != 2;
        let mut mix_factor = 0.0;
        
        if self.vert.common.v_mix_colors.x != 0 {
            let d_line = context.distance_to_line(self.vert.common.v_color_line.xy(), self.vert.common.v_color_line.zw(), self.v_pos);
            if do_aa {
                mix_factor = context.distance_aa(aa_range, -d_line);
            } else {
                mix_factor = if (d_line + 0.0001) >= 0.0 { 1.0 } else { 0.0 };
            }
        }

        let clip_relative_pos = self.v_pos - self.vert.common.v_clip_center_sign.xy();
        let in_clip_region = (self.vert.common.v_clip_center_sign.zw() * clip_relative_pos).cmplt(vec2::ZERO).all();
        let mut d = -1.0;
        
        match self.vert.common.v_segment_clip_mode.y as i32 {
            3 => {
                d = (self.vert.common.v_clip_params1.xy().distance(self.v_pos)) - self.vert.common.v_clip_params1.z;
            },
            2 => {
                let is_vertical = self.vert.common.v_clip_params1.x == 0.0;
                let half_dash = if is_vertical { self.vert.common.v_clip_params1.y } else { self.vert.common.v_clip_params1.x };
                let pos = if is_vertical { self.v_pos.y } else { self.v_pos.x };
                let in_dash = pos < half_dash || pos > 3.0 * half_dash;
                if !in_dash { d = 1.0; }
            },
            1 => {
                let d0 = context.distance_to_line(self.vert.common.v_clip_params1.xy(), self.vert.common.v_clip_params1.zw(), self.v_pos);
                let d1 = context.distance_to_line(self.vert.common.v_clip_params2.xy(), self.vert.common.v_clip_params2.zw(), self.v_pos);
                d = d0.max(-d1);
            },
            _ => {},
        }
        
        let color0;
        let color1;
        
        if in_clip_region {
            let d_radii_a = context.distance_to_ellipse(clip_relative_pos, self.vert.common.v_clip_radii.xy());
            let d_radii_b = context.distance_to_ellipse(clip_relative_pos, self.vert.common.v_clip_radii.zw());
            d = d.max(d_radii_a.max(-d_radii_b));
            
            color0 = self.evaluate_color_for_style_in_corner(clip_relative_pos, self.vert.common.v_style_edge_axis.x as i32, self.vert.common.v_color00, self.vert.common.v_color01, self.vert.common.v_clip_radii, mix_factor, self.vert.common.v_segment_clip_mode.x as i32, aa_range, context);
            color1 = self.evaluate_color_for_style_in_corner(clip_relative_pos, self.vert.common.v_style_edge_axis.y as i32, self.vert.common.v_color10, self.vert.common.v_color11, self.vert.common.v_clip_radii, mix_factor, self.vert.common.v_segment_clip_mode.x as i32, aa_range, context);
        } else {
            color0 = self.evaluate_color_for_style_in_edge(self.v_pos, self.vert.common.v_style_edge_axis.x as i32, self.vert.common.v_color00, self.vert.common.v_color01, aa_range, self.vert.common.v_style_edge_axis.z as i32, context);
            color1 = self.evaluate_color_for_style_in_edge(self.v_pos, self.vert.common.v_style_edge_axis.y as i32, self.vert.common.v_color10, self.vert.common.v_color11, aa_range, self.vert.common.v_style_edge_axis.w as i32, context);
        }

        let alpha = if do_aa { context.distance_aa(aa_range, d) } else { 1.0 };
        let color = color0.lerp(color1, mix_factor);

        color * alpha
    }

    fn evaluate_color_for_style_in_corner(&self, clip_relative_pos: vec2, style: i32, mut color0: vec4, color1: vec4, clip_radii: vec4, mix_factor: f32, segment: i32, aa_range: f32, context: &ShaderContext) -> vec4 {
        match style {
            2 => { // DOTTED
                let d_radii_a = context.distance_to_ellipse(clip_relative_pos, clip_radii.xy() - self.vert.common.v_partial_widths.xy());
                let d_radii_b = context.distance_to_ellipse(clip_relative_pos, clip_radii.xy() - 2.0 * self.vert.common.v_partial_widths.xy());
                let d = (-d_radii_a).min(d_radii_b);
                color0 *= context.distance_aa(aa_range, d);
            },
            6 | 7 => { // GROOVE | RIDGE
                let d = context.distance_to_ellipse(clip_relative_pos, clip_radii.xy() - self.vert.common.v_partial_widths.zw());
                let alpha = context.distance_aa(aa_range, d);
                let swizzled_factor = match segment {
                    0 => 0.0,
                    1 => mix_factor,
                    2 => 1.0,
                    3 => 1.0 - mix_factor,
                    _ => 0.0,
                };
                let c0 = color1.lerp(color0, swizzled_factor);
                let c1 = color0.lerp(color1, swizzled_factor);
                color0 = c0.lerp(c1, alpha);
            },
            _ => {},
        }
        color0
    }

    fn evaluate_color_for_style_in_edge(&self, pos_vec: vec2, style: i32, mut color0: vec4, color1: vec4, aa_range: f32, edge_axis_id: i32, context: &ShaderContext) -> vec4 {
        let edge_axis = if edge_axis_id != 0 { vec2::new(0.0, 1.0) } else { vec2::new(1.0, 0.0) };
        let pos = pos_vec.dot(edge_axis);
        match style {
            2 => { // DOTTED
                let mut d = -1.0;
                let partial_width = self.vert.common.v_partial_widths.xy().dot(edge_axis);
                if partial_width >= 1.0 {
                    let r#ref = vec2::new(
                        self.vert.common.v_edge_reference.xy().dot(edge_axis) + partial_width,
                        self.vert.common.v_edge_reference.zw().dot(edge_axis) - partial_width,
                    );
                    d = (pos - r#ref.x).min(r#ref.y - pos);
                }
                color0 *= context.distance_aa(aa_range, d);
            },
            6 | 7 => { // GROOVE | RIDGE
                let r#ref = (self.vert.common.v_edge_reference.xy() + self.vert.common.v_partial_widths.zw()).dot(edge_axis);
                let d = pos - r#ref;
                let alpha = context.distance_aa(aa_range, d);
                color0 = color0.lerp(color1, alpha);
            },
            _ => {},
        }
        color0
    }
}

impl FragmentShader for CsBorderSegmentFrag {
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
}

impl CsBorderSegmentFrag {
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

//
// Program
//

#[derive(Clone, Debug, Default)]
pub struct CsBorderSegmentProgram {
    frag: CsBorderSegmentFrag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(CsBorderSegmentProgram::default())
}

impl Program for CsBorderSegmentProgram {
    fn get_vertex_shader(&mut self) -> &mut dyn VertexShader {
        &mut self.frag.vert
    }

    fn get_fragment_shader(&mut self) -> &mut dyn FragmentShader {
        &mut self.frag
    }
    
    fn get_uniform(&self, name: &CStr) -> i32 {
        if strcmp(name, "uTransform") { return 1; }
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
