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
struct CsBorderSolidCommon {
    attrib_locations: AttribLocations,
    u_transform: mat4,
    // Flat varyings (set in VS, used in FS)
    v_color0: vec4,
    v_color1: vec4,
    v_color_line: vec4,
    v_mix_colors: ivec2,
    v_clip_center_sign: vec4,
    v_clip_radii: vec4,
    v_horizontal_clip_center_sign: vec4,
    v_horizontal_clip_radii: vec2,
    v_vertical_clip_center_sign: vec4,
    v_vertical_clip_radii: vec2,
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
struct CsBorderSolidVert {
    common: CsBorderSolidCommon,
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

impl CsBorderSolidVert {
    fn main(&mut self) {
        let segment = self.a_flags & 0xFF;
        let do_aa = ((self.a_flags >> 24) & 0xF0) != 0;
        let outer_scale = self.get_outer_corner_scale(segment);
        let size = self.a_rect.zw() - self.a_rect.xy();
        let outer = outer_scale * size;
        let clip_sign = vec2::ONE - (2.0 * outer_scale);

        let mix_colors = match segment {
            0..=3 => if do_aa { 1 } else { 2 },
            _ => 0,
        };
        self.common.v_mix_colors.x = mix_colors;

        self.v_pos = size * self.a_position;

        self.common.v_color0 = self.a_color0;
        self.common.v_color1 = self.a_color1;

        self.common.v_clip_center_sign = vec4(outer.x + (clip_sign.x * self.a_radii.x), outer.y + (clip_sign.y * self.a_radii.y), clip_sign.x, clip_sign.y);
        self.common.v_clip_radii = vec4(self.a_radii.x, self.a_radii.y, (self.a_radii - self.a_widths).max(vec2::ZERO).x, (self.a_radii - self.a_widths).max(vec2::ZERO).y);
        self.common.v_color_line = vec4(outer.x, outer.y, self.a_widths.y * -clip_sign.y, self.a_widths.x * clip_sign.x);
        
        let horizontal_clip_sign = vec2::new(-clip_sign.x, clip_sign.y);
        self.common.v_horizontal_clip_center_sign = vec4((self.a_clip_params1.xy() + horizontal_clip_sign * self.a_clip_params1.zw()).x, (self.a_clip_params1.xy() + horizontal_clip_sign * self.a_clip_params1.zw()).y, horizontal_clip_sign.x, horizontal_clip_sign.y);
        self.common.v_horizontal_clip_radii = self.a_clip_params1.zw();

        let vertical_clip_sign = vec2::new(clip_sign.x, -clip_sign.y);
        self.common.v_vertical_clip_center_sign = vec4((self.a_clip_params2.xy() + vertical_clip_sign * self.a_clip_params2.zw()).x, (self.a_clip_params2.xy() + vertical_clip_sign * self.a_clip_params2.zw()).y, vertical_clip_sign.x, vertical_clip_sign.y);
        self.common.v_vertical_clip_radii = self.a_clip_params2.zw();

        if clip_mode == 3 {
            let mut radius = self.a_clip_params1.z;
            if radius > 0.5 {
                radius += 2.0;
            }
            self.v_pos = self.a_clip_params1.xy() + (radius * (2.0 * self.a_position - 1.0));
            self.v_pos = self.v_pos.clamp(vec2::ZERO, size);
        } else if clip_mode == 1 {
            let center = (self.a_clip_params1.xy() + self.a_clip_params2.xy()) * 0.5;
            let dash_length = (self.a_clip_params1.xy() - self.a_clip_params2.xy()).length();
            let width = self.a_widths.x.max(self.a_widths.y);
            let r = vec2::splat(dash_length.max(width)) + 2.0;
            self.v_pos = self.v_pos.clamp(center - r, center + r);
        }

        self.gl_position = self.common.u_transform * vec4(self.a_task_origin.x + self.a_rect.x + self.v_pos.x, self.a_task_origin.y + self.a_rect.y + self.v_pos.y, 0.0, 1.0);
    }

    fn get_outer_corner_scale(&self, segment: i32) -> vec2 {
        match segment {
            0 => vec2(0.0, 0.0),
            1 => vec2(1.0, 0.0),
            2 => vec2(1.0, 1.0),
            3 => vec2(0.0, 1.0),
            _ => vec2::ZERO,
        }
    }
}

impl VertexShader for CsBorderSolidVert {
    fn init_batch(&mut self, _context: &mut ShaderContext) {}

    fn load_attribs(&mut self, attribs: &[&VertexAttrib], start: u32, instance: i32, _count: i32) {
        unsafe {
            let a_pos_attrib = &*attribs[self.common.attrib_locations.a_position];
            let a_task_origin_attrib = &*attribs[self.common.attrib_locations.a_task_origin];
            let a_rect_attrib = &*attribs[self.common.attrib_locations.a_rect];
            let a_color0_attrib = &*attribs[self.common.attrib_locations.a_color0];
            let a_color1_attrib = &*attribs[self.common.attrib_locations.a_color1];
            let a_flags_attrib = &*attribs[self.common.attrib_locations.a_flags];
            let a_widths_attrib = &*attribs[self.common.attrib_locations.a_widths];
            let a_radii_attrib = &*attribs[self.common.attrib_locations.a_radii];
            let a_clip_params1_attrib = &*attribs[self.common.attrib_locations.a_clip_params1];
            let a_clip_params2_attrib = &*attribs[self.common.attrib_locations.a_clip_params2];

            self.a_position = *(a_pos_attrib.data as *const u8).add(a_pos_attrib.stride * start as usize) as *const Vec2;
            self.a_task_origin = *(a_task_origin_attrib.data as *const u8).add(a_task_origin_attrib.stride * instance as usize) as *const Vec2;
            self.a_rect = *(a_rect_attrib.data as *const u8).add(a_rect_attrib.stride * instance as usize) as *const Vec4;
            self.a_color0 = *(a_color0_attrib.data as *const u8).add(a_color0_attrib.stride * instance as usize) as *const Vec4;
            self.a_color1 = *(a_color1_attrib.data as *const u8).add(a_color1_attrib.stride * instance as usize) as *const Vec4;
            self.a_flags = *(a_flags_attrib.data as *const u8).add(a_flags_attrib.stride * instance as usize) as *const i32;
            self.a_widths = *(a_widths_attrib.data as *const u8).add(a_widths_attrib.stride * instance as usize) as *const Vec2;
            self.a_radii = *(a_radii_attrib.data as *const u8).add(a_radii_attrib.stride * instance as usize) as *const Vec2;
            self.a_clip_params1 = *(a_clip_params1_attrib.data as *const u8).add(a_clip_params1_attrib.stride * instance as usize) as *const Vec4;
            self.a_clip_params2 = *(a_clip_params2_attrib.data as *const u8).add(a_clip_params2_attrib.stride * instance as usize) as *const Vec4;
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
        if index == 1 {
            self.common.u_transform = Mat4::from_cols_array(value);
        }
    }
}

//
// Fragment Shader
//

#[derive(Clone, Debug, Default)]
struct CsBorderSolidFrag {
    vert: CsBorderSolidVert,
    v_pos: vec2,
    interp_step: InterpOutputs,
    interp_perspective: InterpOutputs,
}

impl CsBorderSolidFrag {
    fn main(&self) -> vec4 {
        let aa_range = self.compute_aa_range(self.v_pos);
        let do_aa = self.vert.common.v_mix_colors.x != 2;
        let mut mix_factor = 0.0;
        
        if self.vert.common.v_mix_colors.x != 0 {
            let d_line = self.distance_to_line(self.vert.common.v_color_line.xy(), self.vert.common.v_color_line.zw(), self.v_pos);
            if do_aa {
                mix_factor = self.distance_aa(aa_range, -d_line);
            } else {
                mix_factor = if d_line + 0.0001 >= 0.0 { 1.0 } else { 0.0 };
            }
        }

        let mut clip_relative_pos = self.v_pos - self.vert.common.v_clip_center_sign.xy();
        let in_clip_region = (self.vert.common.v_clip_center_sign.zw() * clip_relative_pos).cmplt(vec2::ZERO).all();

        let mut d = -1.0;
        if in_clip_region {
            let d_radii_a = self.distance_to_ellipse(clip_relative_pos, self.vert.common.v_clip_radii.xy());
            let d_radii_b = self.distance_to_ellipse(clip_relative_pos, self.vert.common.v_clip_radii.zw());
            d = d_radii_a.max(-d_radii_b);
        }
        
        clip_relative_pos = self.v_pos - self.vert.common.v_horizontal_clip_center_sign.xy();
        let in_horizontal_clip_region = (self.vert.common.v_horizontal_clip_center_sign.zw() * clip_relative_pos).cmplt(vec2::ZERO).all();
        if in_horizontal_clip_region {
            let d_radii = self.distance_to_ellipse(clip_relative_pos, self.vert.common.v_horizontal_clip_radii);
            d = d_radii.max(d);
        }
        
        clip_relative_pos = self.v_pos - self.vert.common.v_vertical_clip_center_sign.xy();
        let in_vertical_clip_region = (self.vert.common.v_vertical_clip_center_sign.zw() * clip_relative_pos).cmplt(vec2::ZERO).all();
        if in_vertical_clip_region {
            let d_radii = self.distance_to_ellipse(clip_relative_pos, self.vert.common.v_vertical_clip_radii);
            d = d_radii.max(d);
        }

        let alpha = if do_aa { self.distance_aa(aa_range, d) } else { 1.0 };
        let color = self.vert.common.v_color0.lerp(self.vert.common.v_color1, mix_factor);
        color * alpha
    }
    
    fn compute_aa_range(&self, position: vec2) -> f32 {
        let fwidth = (position.dpdx() + position.dpdy()).abs();
        1.0 / fwidth.x
    }
    
    fn distance_to_line(&self, p0: vec2, perp_dir: vec2, p: vec2) -> f32 {
        let dir_to_p0 = p0 - p;
        perp_dir.normalize().dot(dir_to_p0)
    }
    
    fn distance_aa(&self, aa_range: f32, signed_distance: f32) -> f32 {
        let dist = signed_distance * aa_range;
        (0.5 - dist).clamp(0.0, 1.0)
    }

    fn inverse_radii_squared(&self, radii: vec2) -> vec2 {
        vec2::ONE / (radii * radii).max(vec2::splat(0.000001))
    }

    fn distance_to_ellipse_approx(&self, p: vec2, inv_radii_sq: vec2, scale: f32) -> f32 {
        let p_r = p * inv_radii_sq;
        let g = p.dot(p_r) - scale;
        let d_g = (1.0 + scale) * p_r;
        g * d_g.length_recip()
    }
    
    fn distance_to_ellipse(&self, p: vec2, radii: vec2) -> f32 {
        let scale = if radii.cmpgt(vec2::ZERO).all() { 1.0 } else { 0.0 };
        self.distance_to_ellipse_approx(p, self.inverse_radii_squared(radii), scale)
    }
}

impl FragmentShader for CsBorderSolidFrag {
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
        let color = self.main();
        context.write_output(color);
        self.step_interp_inputs(4);
    }
    
    fn skip(&mut self, steps: i32) {
        self.step_interp_inputs(steps);
    }

    fn run_perspective(&mut self, context: &mut ShaderContext, next_w: &[f32; 4]) {
        let color = self.main();
        context.write_output(color);
        self.step_perspective_inputs(4, next_w);
    }

    fn skip_perspective(&mut self, steps: i32, next_w: &[f32; 4]) {
        self.step_perspective_inputs(steps, next_w);
    }
}

impl CsBorderSolidFrag {
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
pub struct CsBorderSolidProgram {
    frag: CsBorderSolidFrag,
}

pub fn loader() -> Box<dyn Program> {
    Box::new(CsBorderSolidProgram::default())
}

impl Program for CsBorderSolidProgram {
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
