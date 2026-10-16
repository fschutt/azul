//! The materials drawn by the GPU: the material shader (`shader.glsl`) run
//! over the rect's device pixels into a texture of its own, which the frame
//! then shows like any GL texture image.
//!
//! The CPU draws the same surface (`shade`); the GPU is what makes the
//! cursor's sheen cheap - a new sheen is one draw call, nothing is read
//! back, nothing waits. Everything the shader samples besides the frame -
//! the material's colourless tile, its mottling, the wallpaper - is uploaded
//! once per GL context and kept as plain texture ids; a shape's distance
//! field is uploaded with the frame that needs it. The frames themselves are
//! not kept here: the window remembers each one with what it was drawn from,
//! and drops it with its own GL images.
//!
//! A context whose driver does not compile the shader (or a GLSL version
//! without what it needs) answers `None` once and for good, and the caller
//! draws on the CPU.

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use azul_core::{
    geom::PhysicalSize,
    gl::{gl, GenericGlContext, GlContextPtr, Texture},
    resources::ImageRef,
};
use azul_css::props::basic::ColorU;

use super::{
    colourless_tile, material_index, prepare, BevelFollow, MaterialParams, MaterialSource,
    SurfaceScene,
};
use crate::texture::Material;

const FRAGMENT: &str = include_str!("shader.glsl");

/// A full-screen triangle from the vertex id alone: no buffers.
const VERTEX: &str = "#version 150
void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
";

/// What one GL context keeps for the materials.
struct ContextState {
    /// The context this state belongs to. Weak: it does not keep a closed
    /// window's context alive, and while it exists no other context can be
    /// made at the same address, so a state is never taken for another
    /// context's.
    context: Weak<GenericGlContext>,
    /// The linked program, or 0 when it does not compile here.
    program: u32,
    /// An empty vertex array (core profiles draw nothing without one).
    vao: u32,
    /// Each material's colourless tile and its mottling (texture ids; 0 =
    /// not uploaded yet).
    tiles: [u32; 4],
    macros: [u32; 4],
    /// A one-pixel texture bound where a sampler has nothing to read.
    blank: u32,
    /// The wallpaper uploaded last: its id and texture.
    wallpaper: Option<(u64, u32)>,
}

thread_local! {
    static STATES: RefCell<Vec<ContextState>> = const { RefCell::new(Vec::new()) };
}

/// `material` drawn by the GPU of `gl` for a `width` x `height` device-px
/// rect, as a GL texture image; `None` when this context cannot (the caller
/// draws on the CPU).
pub(crate) fn render_material_gl(
    gl_ptr: &GlContextPtr,
    material: Material,
    width: u32,
    height: u32,
    dpr: f32,
    params: &MaterialParams,
    scene: &SurfaceScene,
) -> Option<ImageRef> {
    if !gl_ptr.is_gl_usable() || width == 0 || height == 0 || width > 16_384 || height > 16_384 {
        return None;
    }
    let version = gl_ptr.get_usable_glsl_version();
    let version = version.as_str().trim();
    // `#version 100` (GLES 2) has no `gl_VertexID`, `textureLod` in a
    // fragment shader or `out` variables: the CPU draws there.
    if version.is_empty() || version == "100" {
        return None;
    }
    let gl: Rc<GenericGlContext> = gl_ptr.get().clone();
    STATES.with(|states| {
        let mut states = states.borrow_mut();
        let index = state_index(&mut states, &gl, version);
        let state = &mut states[index];
        if state.program == 0 {
            return None;
        }
        state.draw(gl_ptr, &gl, material, [width, height], dpr, params, scene)
    })
}

/// Whether the material shader compiles and links on `gl` (tests ask, to
/// know the GPU path is the one being compared).
#[must_use]
pub fn material_shader_compiles(gl_ptr: &GlContextPtr) -> bool {
    let version = gl_ptr.get_usable_glsl_version();
    let version = version.as_str().trim();
    if !gl_ptr.is_gl_usable() || version.is_empty() || version == "100" {
        return false;
    }
    let gl = gl_ptr.get().clone();
    STATES.with(|states| {
        let mut states = states.borrow_mut();
        let index = state_index(&mut states, &gl, version);
        states[index].program != 0
    })
}

/// The index of `gl`'s state, made now if it has none; states of contexts
/// that are gone are dropped first (their GL objects went with them).
fn state_index(states: &mut Vec<ContextState>, gl: &Rc<GenericGlContext>, version: &str) -> usize {
    states.retain(|s| s.context.strong_count() > 0);
    if let Some(i) = states
        .iter()
        .position(|s| s.context.upgrade().is_some_and(|c| Rc::ptr_eq(&c, gl)))
    {
        return i;
    }
    states.push(ContextState::new(gl, version));
    states.len() - 1
}

/// `src` with its first line swapped for `#version <version>`.
fn with_version(src: &str, version: &str) -> Vec<u8> {
    let body = src.split_once('\n').map_or("", |(_, rest)| rest);
    format!("#version {version}\n{body}").into_bytes()
}

fn compile(gl: &GenericGlContext, kind: u32, src: &[u8]) -> Result<u32, String> {
    let shader = gl.create_shader(kind);
    gl.shader_source(shader, &[src]);
    gl.compile_shader(shader);
    let mut status = [0_i32];
    // SAFETY: one int is what COMPILE_STATUS writes.
    unsafe { gl.get_shader_iv(shader, gl::COMPILE_STATUS, &mut status) };
    if status[0] == gl::TRUE as i32 {
        Ok(shader)
    } else {
        let log = gl.get_shader_info_log(shader);
        gl.delete_shader(shader);
        Err(log)
    }
}

fn link(gl: &GenericGlContext, version: &str) -> Result<u32, String> {
    let vs = compile(gl, gl::VERTEX_SHADER, &with_version(VERTEX, version))?;
    let fs = match compile(gl, gl::FRAGMENT_SHADER, &with_version(FRAGMENT, version)) {
        Ok(fs) => fs,
        Err(e) => {
            gl.delete_shader(vs);
            return Err(e);
        }
    };
    let program = gl.create_program();
    gl.attach_shader(program, vs);
    gl.attach_shader(program, fs);
    gl.link_program(program);
    gl.delete_shader(vs);
    gl.delete_shader(fs);
    let mut status = [0_i32];
    // SAFETY: one int is what LINK_STATUS writes.
    unsafe { gl.get_program_iv(program, gl::LINK_STATUS, &mut status) };
    if status[0] == gl::TRUE as i32 {
        Ok(program)
    } else {
        let log = gl.get_program_info_log(program);
        gl.delete_program(program);
        Err(log)
    }
}

/// A texture of `rgba` bytes (`width` x `height`, rows from the top).
/// The shader's number for `material` (`surf` in shader.glsl: 0 leather,
/// 1 aluminium, 2 denim, else vellum) - not the tile tables' index, which
/// orders denim before aluminium.
const fn shader_material(material: Material) -> i32 {
    match material {
        Material::Leather => 0,
        Material::Aluminium => 1,
        Material::BlueJeans => 2,
        Material::Vellum => 3,
    }
}

fn upload(gl: &GenericGlContext, width: usize, height: usize, rgba: &[u8], repeat: bool, mips: bool) -> u32 {
    let id = gl.gen_textures(1).first().copied().unwrap_or(0);
    if id == 0 {
        return 0;
    }
    gl.bind_texture(gl::TEXTURE_2D, id);
    gl.tex_image_2d(
        gl::TEXTURE_2D,
        0,
        gl::RGBA as i32,
        width as i32,
        height as i32,
        0,
        gl::RGBA,
        gl::UNSIGNED_BYTE,
        Some(rgba),
    );
    let wrap = if repeat { gl::REPEAT } else { gl::CLAMP_TO_EDGE };
    gl.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, wrap as i32);
    gl.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, wrap as i32);
    gl.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
    if mips {
        gl.generate_mipmap(gl::TEXTURE_2D);
        gl.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR_MIPMAP_LINEAR as i32);
    } else {
        gl.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
    }
    id
}

/// The texture units the material shader samples: its tile, its mottling, a
/// shape's distance field and the wallpaper.
const SAMPLER_UNITS: usize = 4;

/// The context's state a material draw changes, saved before it and put back
/// after: the framebuffer, program, vertex array, viewport and active texture
/// unit, the texture bound to each unit the shader samples, and whether
/// blending, the scissor and the depth test are on. The draw runs in the
/// middle of the GL frame path - after an app's canvas callbacks, before
/// WebRender - and each of them finds the state it left.
struct SavedGlState {
    /// The framebuffer drawn into (the window's, or a renderer's target).
    framebuffer: i32,
    /// The program in use.
    program: i32,
    /// The vertex array bound.
    vertex_array: i32,
    /// The viewport: x, y, width, height.
    viewport: [i32; 4],
    /// The active texture unit (`GL_TEXTURE0 + n`).
    active_texture: i32,
    /// The 2D texture bound to each unit the shader samples.
    textures: [i32; SAMPLER_UNITS],
    /// Whether blending is on (the draw turns it off: it writes its pixels).
    blend: bool,
    /// Whether the scissor test is on (off for the draw: the whole frame).
    scissor: bool,
    /// Whether the depth test is on (off for the draw).
    depth: bool,
}

impl SavedGlState {
    fn save(gl: &GenericGlContext) -> Self {
        let one = |name: u32| {
            let mut value = [0_i32];
            // SAFETY: each query writes one int.
            unsafe { gl.get_integer_v(name, &mut value) };
            value[0]
        };
        let mut viewport = [0_i32; 4];
        // SAFETY: the viewport is four ints.
        unsafe { gl.get_integer_v(gl::VIEWPORT, &mut viewport) };
        let active_texture = one(gl::ACTIVE_TEXTURE);
        let mut textures = [0_i32; SAMPLER_UNITS];
        for (unit, bound) in textures.iter_mut().enumerate() {
            gl.active_texture(gl::TEXTURE0 + unit as u32);
            *bound = one(gl::TEXTURE_BINDING_2D);
        }
        gl.active_texture(active_texture as u32);
        Self {
            framebuffer: one(gl::FRAMEBUFFER_BINDING),
            program: one(gl::CURRENT_PROGRAM),
            vertex_array: one(gl::VERTEX_ARRAY_BINDING),
            viewport,
            active_texture,
            textures,
            blend: gl.is_enabled(gl::BLEND) != 0,
            scissor: gl.is_enabled(gl::SCISSOR_TEST) != 0,
            depth: gl.is_enabled(gl::DEPTH_TEST) != 0,
        }
    }

    fn restore(&self, gl: &GenericGlContext) {
        for (cap, on) in [
            (gl::BLEND, self.blend),
            (gl::SCISSOR_TEST, self.scissor),
            (gl::DEPTH_TEST, self.depth),
        ] {
            if on {
                gl.enable(cap);
            } else {
                gl.disable(cap);
            }
        }
        for (unit, &bound) in self.textures.iter().enumerate() {
            gl.active_texture(gl::TEXTURE0 + unit as u32);
            gl.bind_texture(gl::TEXTURE_2D, bound as u32);
        }
        gl.active_texture(self.active_texture as u32);
        gl.bind_framebuffer(gl::FRAMEBUFFER, self.framebuffer as u32);
        gl.use_program(self.program as u32);
        gl.bind_vertex_array(self.vertex_array as u32);
        gl.viewport(self.viewport[0], self.viewport[1], self.viewport[2], self.viewport[3]);
    }
}

impl ContextState {
    fn new(gl: &Rc<GenericGlContext>, version: &str) -> Self {
        let program = match link(gl, version) {
            Ok(p) => p,
            Err(log) => {
                eprintln!(
                    "azul: the material shader does not compile on this GPU (GLSL {version}); \
                     materials are drawn on the CPU: {log}"
                );
                0
            }
        };
        let vao = if program != 0 {
            gl.gen_vertex_arrays(1).first().copied().unwrap_or(0)
        } else {
            0
        };
        let blank = if program != 0 {
            upload(gl, 1, 1, &[128, 128, 128, 255], true, false)
        } else {
            0
        };
        Self {
            context: Rc::downgrade(gl),
            program,
            vao,
            tiles: [0; 4],
            macros: [0; 4],
            blank,
            wallpaper: None,
        }
    }

    /// The material's tile and mottling textures, uploaded on first use.
    fn tile_textures(&mut self, gl: &GenericGlContext, material: Material) -> (u32, u32, f32) {
        let i = material_index(material);
        let Some(tile) = colourless_tile(material) else {
            return (self.blank, self.blank, 0.0);
        };
        if self.tiles[i] == 0 {
            let bytes: Vec<u8> = tile.rgba.iter().flatten().copied().collect();
            self.tiles[i] = upload(gl, tile.side, tile.side, &bytes, true, false);
        }
        if self.macros[i] == 0 && tile.macro_side > 0 {
            let bytes: Vec<u8> = tile.macro_q.iter().flat_map(|&q| [q, q, q, 255]).collect();
            self.macros[i] = upload(gl, tile.macro_side, tile.macro_side, &bytes, true, false);
        }
        let period = if self.macros[i] != 0 { tile.macro_period } else { 0.0 };
        (self.tiles[i], if self.macros[i] != 0 { self.macros[i] } else { self.blank }, period)
    }

    /// The wallpaper `w` as a mipmapped texture (the last one is kept).
    fn wallpaper_texture(&mut self, gl: &GenericGlContext, w: &crate::texture::environment::Wallpaper) -> u32 {
        if let Some((id, tex)) = self.wallpaper {
            if id == w.id {
                return tex;
            }
            gl.delete_textures(&[tex]);
        }
        let tex = upload(gl, w.base_size[0], w.base_size[1], &w.base_rgba, false, true);
        self.wallpaper = Some((w.id, tex));
        tex
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn draw(
        &mut self,
        gl_ptr: &GlContextPtr,
        gl: &GenericGlContext,
        material: Material,
        [width, height]: [u32; 2],
        dpr: f32,
        params: &MaterialParams,
        scene: &SurfaceScene,
    ) -> Option<ImageRef> {
        // Before anything is uploaded or bound: what the frame path had set
        // up is put back after the draw.
        let saved = SavedGlState::save(gl);
        let u = prepare(material, [width, height], dpr, params, scene);
        let (tile_tex, macro_tex, macro_period) = self.tile_textures(gl, material);
        let source = match params.source {
            MaterialSource::Tile if colourless_tile(material).is_some() => 0,
            MaterialSource::Smooth => 2,
            _ => 1,
        };
        let shape = if params.bevel.follow == BevelFollow::Shape {
            scene.shape.as_deref()
        } else {
            None
        };
        let shape_tex = shape.map(|f| {
            let bytes: Vec<u8> = f.data.iter().flat_map(|&q| [q, q, q, 255]).collect();
            upload(gl, f.width, f.height, &bytes, false, false)
        });
        let wallpaper = scene.environment.wallpaper_at(scene.desktop_origin).cloned();
        let env_tex = wallpaper.as_ref().map(|w| self.wallpaper_texture(gl, w));

        let texture = Texture::allocate_rgba8(
            gl_ptr.clone(),
            PhysicalSize { width, height },
            ColorU::TRANSPARENT,
        );
        if texture.texture_id == 0 {
            if let Some(tex) = shape_tex {
                gl.delete_textures(&[tex]);
            }
            saved.restore(gl);
            return None;
        }

        let fbo = gl.gen_framebuffers(1).first().copied().unwrap_or(0);
        gl.bind_framebuffer(gl::FRAMEBUFFER, fbo);
        gl.framebuffer_texture_2d(
            gl::FRAMEBUFFER,
            gl::COLOR_ATTACHMENT0,
            gl::TEXTURE_2D,
            texture.texture_id,
            0,
        );
        let complete = fbo != 0 && gl.check_frame_buffer_status(gl::FRAMEBUFFER) == gl::FRAMEBUFFER_COMPLETE;
        if complete {
            gl.viewport(0, 0, width as i32, height as i32);
            gl.disable(gl::BLEND);
            gl.disable(gl::SCISSOR_TEST);
            gl.disable(gl::DEPTH_TEST);
            gl.use_program(self.program);
            gl.bind_vertex_array(self.vao);
            let p = self.program;
            let loc = |name: &str| gl.get_uniform_location(p, name);
            let f1 = |name: &str, v: f32| gl.uniform_1f(loc(name), v);
            let f2 = |name: &str, v: [f32; 2]| gl.uniform_2f(loc(name), v[0], v[1]);
            let f3 = |name: &str, v: [f32; 3]| gl.uniform_3f(loc(name), v[0], v[1], v[2]);
            let f4 = |name: &str, v: [f32; 4]| gl.uniform_4f(loc(name), v[0], v[1], v[2], v[3]);
            let i1 = |name: &str, v: i32| gl.uniform_1i(loc(name), v);
            f2("uSize", [width as f32, height as f32]);
            f1("uDpr", u.dpr);
            f2("uOriginPx", [u.origin_px[0] as f32, u.origin_px[1] as f32]);
            f4("uRadii", params.radii);
            i1("uShape", i32::from(shape.is_some()));
            f1("uShapeRange", shape.map_or(1.0, super::ShapeField::range));
            f4("uBevel", u.bevel);
            i1("uMat", shader_material(material));
            i1("uSource", source);
            f1("uTileNormal", if material == Material::Leather { 1.0 } else { 0.0 });
            f1("uMacroPeriod", macro_period);
            f3("uTint", params.tint);
            f2("uRough", params.roughness);
            f1("uRelief", u.relief);
            f1("uVariation", params.variation);
            f1("uMetal", u.metal);
            f1("uAniso", if u.aniso { 1.0 } else { 0.0 });
            f1("uScale", params.scale.max(0.01));
            f3("uL", u.sun);
            f3("uLightColor", u.light_color);
            f1("uAmbient", scene.ambient);
            f4("uEnvRect", u.env_rect);
            f3("uEnvFlat", scene.environment.flat_colour());
            f1("uEnvMode", if env_tex.is_some() { 1.0 } else { 0.0 });
            f1("uGrain", params.grain);
            f2("uCushion", u.cushion);
            f2("uPillow", u.pillow);
            f1("uPillowFoot", u.pillow_foot);
            f4("uGlow", u.glow);
            i1("uLightMode", i32::from(u.point.is_some()));
            f3("uLightPos", u.point.unwrap_or([0.0, 0.0, 1.0]));
            f2("uDesktopOrigin", scene.desktop_origin);
            let samplers = [
                ("uTile", tile_tex),
                ("uMacro", macro_tex),
                ("uShapeTex", shape_tex.unwrap_or(self.blank)),
                ("uEnv", env_tex.unwrap_or(self.blank)),
            ];
            for (unit, &(name, tex)) in samplers.iter().enumerate() {
                gl.active_texture(gl::TEXTURE0 + unit as u32);
                gl.bind_texture(gl::TEXTURE_2D, tex);
                i1(name, unit as i32);
            }
            gl.draw_arrays(gl::TRIANGLES, 0, 3);
            super::count_render();
        }

        saved.restore(gl);
        if fbo != 0 {
            gl.delete_framebuffers(&[fbo]);
        }
        if let Some(tex) = shape_tex {
            gl.delete_textures(&[tex]);
        }
        complete.then(|| ImageRef::new_gltexture(texture))
    }
}
