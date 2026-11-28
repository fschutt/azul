//! EXPERIMENT (the Android GPU root-cause hunt, never for master): the CONTROL of the black window. A ladder of small GL programs
//! that use, one after the other, the pieces WebRender's frame is made of, and each reads back what it drew:
//!
//! 1. `clear`: a clear of the window's framebuffer;
//! 2. `shader`: a full-screen triangle with a shader and a uniform;
//! 2b. `dummy_texture`: WebRender's 1 x 1 dummy texture (the source of every solid colour tile), sampled over the window;
//! 3. `fbo_sample`: a texture (`glTexStorage2D`, RGBA8) as the colour of a framebuffer, four scissored clears into it, then the window's
//!    framebuffer drawn with a shader that samples the texture: the picture cache tile and the composite;
//! 4. `instanced`: a VAO with a vertex buffer, an index buffer and a per-instance buffer holding a `vec4` and an `ivec4` (integer
//!    attribute, divisor 1), drawn with `glDrawElementsInstanced`, sampling the same texture: `composite` of WebRender;
//! 4b. `flat_lowp`: the quad pipeline's varyings, `flat lowp ivec4` carrying values that need more than eight bits and a `flat mediump vec4`;
//! 4c. `quad_u8norm`: the same draw with WebRender's own unit quad, normalized unsigned bytes with a stride of two;
//! 4e. `big_texture`: render targets of 4 to 16 MB (WebRender's shared render targets are 2048 x 2048 RGBA8) sampled over the whole window;
//! 4d. `texture_size`: `textureSize()` in the vertex stage, for render targets and uploaded textures of several sizes and formats;
//! 5. `data_textures`: an RGBA32F and an RGBA32I texture fetched with `texelFetch`: WebRender's GPU cache and prim headers;
//! 5d. `pbo_upload`: the same two textures filled through a pixel unpack buffer (`glMapBufferRange`, `glUnmapBuffer`, `glTexSubImage2D` from an
//!     offset with `GL_UNPACK_ROW_LENGTH`, the buffer mapped a second time unsynchronized): WebRender's `TextureUploader`;
//! 5a. `vertex_fetch`: WebRender's data path in the vertex stage (an integer instance attribute names a texel of an integer texture which names
//!     the texels of a float texture, `texelFetch` in the vertex shader);
//! 5b. `scatter`: WebRender's GPU cache update, points with a `gl_PointSize` shader into an RGBA32F texture through its framebuffer;
//! 6. `depth`: a framebuffer with a colour texture and a 24-bit depth renderbuffer, depth cleared with `glClearBufferfv`, two
//!    triangles at two depths with `LEQUAL`: the depth pass of a tile.
//!
//! The caller (`gles_gpu`) swaps the window after each phase and holds it, so that a screenshot of the phone shows what each did; with
//! no WebRender anywhere in the process the result separates "the window cannot show what GL draws" from "WebRender's frame is
//! wrong". The host test runs the same ladder on Mesa to prove that the ladder itself reads what it says.

use azul_core::glconst;
use gl_context_loader::GenericGlContext;

/// How far a read-back channel may be from the expected one (of 255): an 8 bit target differs by at most one.
const TOLERANCE: i16 = 3;

/// What one phase found.
#[derive(Debug, Clone)]
pub struct Row {
    /// The phase.
    pub name: &'static str,
    /// Every read-back matched and `glGetError` was clean.
    pub ok: bool,
    /// The read-backs, expected and found, and the error.
    pub detail: String,
}

/// Where the phases draw and show: the size of the window's framebuffer, and the framebuffer that stands for the window (0 on a phone,
/// an offscreen framebuffer in the host test).
#[derive(Debug, Clone, Copy)]
pub struct Target {
    /// Width and height in pixels.
    pub size: (u32, u32),
    /// The framebuffer object of the window (0 for the default framebuffer).
    pub window: u32,
    /// Bind the way WebRender's device does: `GL_DRAW_FRAMEBUFFER` to draw (and to attach and check) and `GL_READ_FRAMEBUFFER` to
    /// read, never `GL_FRAMEBUFFER`. Off: `GL_FRAMEBUFFER` for everything.
    pub split: bool,
    /// Unbind `GL_TEXTURE_EXTERNAL_OES` before every binding of a `GL_TEXTURE_2D`, as the device does on the Android emulator
    /// (`requires_texture_external_unbind`).
    pub external_unbind: bool,
}

/// Binds the 2D texture `texture`, after the external unbind the emulator's quirk asks for when the target says so.
fn bind_2d(gl: &GenericGlContext, target: Target, texture: u32) {
    if target.external_unbind {
        gl.bind_texture(glconst::TEXTURE_EXTERNAL_OES, 0);
    }
    gl.bind_texture(glconst::TEXTURE_2D, texture);
}

/// The target a framebuffer is bound to for drawing, attaching and checking.
const fn draw_target(target: Target) -> u32 {
    if target.split {
        glconst::DRAW_FRAMEBUFFER
    } else {
        glconst::FRAMEBUFFER
    }
}

/// Makes `fbo` the framebuffer the next `px` reads (a split target binds it as the read framebuffer; a joint one has it bound already).
fn read_from(gl: &GenericGlContext, target: Target, fbo: u32) {
    if target.split {
        gl.bind_framebuffer(glconst::READ_FRAMEBUFFER, fbo);
    }
}

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const YELLOW: [u8; 4] = [255, 255, 0, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];

fn close(expected: [u8; 4], found: [u8; 4]) -> bool {
    expected
        .iter()
        .zip(found.iter())
        .all(|(e, f)| (i16::from(*e) - i16::from(*f)).abs() <= TOLERANCE)
}

fn hex(c: [u8; 4]) -> String {
    format!("#{:02x}{:02x}{:02x}{:02x}", c[0], c[1], c[2], c[3])
}

/// The pixel at `(x, y)` (origin bottom left) of the framebuffer bound for reading.
fn px(gl: &GenericGlContext, x: u32, y: u32) -> [u8; 4] {
    let mut rgba = [0_u8; 4];
    gl.read_pixels_into_buffer(
        i32::try_from(x).unwrap_or(0),
        i32::try_from(y).unwrap_or(0),
        1,
        1,
        glconst::RGBA,
        glconst::UNSIGNED_BYTE,
        &mut rgba,
    );
    rgba
}

/// A phase's row from its checks `(what, expected, found)`, the first error of the phase and a note.
fn row(name: &'static str, checks: &[(&str, [u8; 4], [u8; 4])], error: u32, note: &str) -> Row {
    let ok = error == 0 && checks.iter().all(|(_, e, f)| close(*e, *f));
    let mut detail = checks
        .iter()
        .map(|(what, e, f)| format!("{what} expect {} got {}", hex(*e), hex(*f)))
        .collect::<Vec<_>>()
        .join("; ");
    if !note.is_empty() {
        detail.push_str("; ");
        detail.push_str(note);
    }
    detail.push_str(&format!("; glGetError 0x{error:x}"));
    Row { name, ok, detail }
}

fn failed(name: &'static str, why: String) -> Row {
    Row {
        name,
        ok: false,
        detail: why,
    }
}

/// One shader. The source as it is, and when the driver refuses it, with the NUL some emulators' GLES wants at the end of a source (the
/// device layer does the same for them).
fn compile(gl: &GenericGlContext, kind: u32, source: &str) -> Result<u32, String> {
    let mut last = String::new();
    for terminated in [false, true] {
        let mut bytes = source.as_bytes().to_vec();
        if terminated {
            bytes.push(0);
        }
        let shader = gl.create_shader(kind);
        gl.shader_source(shader, &[bytes.as_slice()]);
        gl.compile_shader(shader);
        let mut status = [0_i32];
        // SAFETY: a query that writes one integer into a slice of one.
        unsafe { gl.get_shader_iv(shader, glconst::COMPILE_STATUS, &mut status) };
        if status[0] != 0 {
            return Ok(shader);
        }
        last = super::gles_probe::clean_driver_string(&gl.get_shader_info_log(shader));
        gl.delete_shader(shader);
    }
    Err(last)
}

/// A program from two sources, its attribute locations bound as asked, or why it did not build.
fn program_of(
    gl: &GenericGlContext,
    vertex: &str,
    fragment: &str,
    attributes: &[(u32, &str)],
) -> Result<u32, String> {
    let vertex = compile(gl, glconst::VERTEX_SHADER, vertex).map_err(|e| format!("vertex shader: {e}"))?;
    let fragment = match compile(gl, glconst::FRAGMENT_SHADER, fragment) {
        Ok(fragment) => fragment,
        Err(e) => {
            gl.delete_shader(vertex);
            return Err(format!("fragment shader: {e}"));
        }
    };
    let program = gl.create_program();
    gl.attach_shader(program, vertex);
    gl.attach_shader(program, fragment);
    for (index, name) in attributes {
        gl.bind_attrib_location(program, *index, name);
    }
    gl.link_program(program);
    gl.delete_shader(vertex);
    gl.delete_shader(fragment);
    let mut status = [0_i32];
    // SAFETY: a query that writes one integer into a slice of one.
    unsafe { gl.get_program_iv(program, glconst::LINK_STATUS, &mut status) };
    if status[0] == 0 {
        let log = super::gles_probe::clean_driver_string(&gl.get_program_info_log(program));
        gl.delete_program(program);
        return Err(format!("link: {log}"));
    }
    Ok(program)
}

/// A full-screen triangle from `gl_VertexID` (no buffers); the depth comes from a uniform.
const FULL_VS: &str = "#version 300 es\nuniform float uZ;\nout vec2 vUv;\nvoid main() {\n    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));\n    vUv = p;\n    gl_Position = vec4(p * 2.0 - 1.0, uZ, 1.0);\n}\n";
const FLAT_FS: &str = "#version 300 es\nprecision mediump float;\nuniform vec4 uColour;\nin vec2 vUv;\nout vec4 oColour;\nvoid main() {\n    oColour = uColour;\n}\n";
const TEXTURE_FS: &str = "#version 300 es\nprecision mediump float;\nuniform sampler2D sTex;\nin vec2 vUv;\nout vec4 oColour;\nvoid main() {\n    oColour = texture(sTex, vUv);\n}\n";
const FETCH_FLOAT_FS: &str = "#version 300 es\nprecision highp float;\nuniform highp sampler2D sData;\nin vec2 vUv;\nout vec4 oColour;\nvoid main() {\n    oColour = texelFetch(sData, ivec2(int(vUv.x * 4.0), 0), 0);\n}\n";
const FETCH_INT_FS: &str = "#version 300 es\nprecision highp float;\nprecision highp int;\nuniform highp isampler2D sData;\nin vec2 vUv;\nout vec4 oColour;\nvoid main() {\n    ivec4 v = texelFetch(sData, ivec2(int(vUv.x * 4.0), 0), 0);\n    oColour = vec4(v) / 255.0;\n}\n";
/// The composite of WebRender in small: an instanced quad, a per-instance rectangle (`vec4`) and a per-instance integer record (`ivec4`).
const INSTANCED_VS: &str = "#version 300 es\nprecision highp float;\nprecision highp int;\nlayout(location = 0) in vec2 aPosition;\nlayout(location = 1) in vec4 aRect;\nlayout(location = 2) in ivec4 aData;\nuniform mat4 uTransform;\nout vec2 vUv;\nflat out ivec4 vData;\nvoid main() {\n    vec2 pos = aRect.xy + aPosition * aRect.zw;\n    gl_Position = uTransform * vec4(pos, 0.0, 1.0);\n    vUv = aPosition;\n    vData = aData;\n}\n";
const INSTANCED_FS: &str = "#version 300 es\nprecision highp float;\nprecision highp int;\nuniform sampler2D sColor0;\nin vec2 vUv;\nflat in ivec4 vData;\nout vec4 oColour;\nvoid main() {\n    vec4 c = texture(sColor0, vUv);\n    oColour = vec4(c.r * float(vData.x), c.g * float(vData.y) * 0.5, c.b, 1.0);\n}\n";

/// WebRender's data path in the VERTEX shader: an integer attribute per instance names a texel of an integer texture, which names the
/// texels of a float texture (the GPU cache) that hold the rectangle and the colour of the quad. Both fetches are `texelFetch` in the
/// vertex stage, with WebRender's unsigned address arithmetic (`uint % 1024u`, `uint / 1024u`).
const VERTEX_FETCH_VS: &str = "#version 300 es\nprecision highp float;\nprecision highp int;\nprecision highp sampler2D;\nprecision highp isampler2D;\nuniform sampler2D sGpuCache;\nuniform isampler2D sHeaders;\nlayout(location = 0) in vec2 aPosition;\nlayout(location = 1) in ivec4 aData;\nout vec4 vColour;\nivec2 get_uv(int address) {\n    return ivec2(int(uint(address) % 1024u), int(uint(address) / 1024u));\n}\nvoid main() {\n    ivec4 header = texelFetch(sHeaders, ivec2(aData.x, 0), 0);\n    vec4 rect = texelFetch(sGpuCache, get_uv(header.x), 0);\n    vec4 colour = texelFetch(sGpuCache, get_uv(header.x + 1), 0);\n    vec2 pos = rect.xy + aPosition * rect.zw;\n    gl_Position = vec4(pos * 2.0 - 1.0, 0.0, 1.0);\n    vColour = colour;\n}\n";
const VERTEX_FETCH_FS: &str = "#version 300 es\nprecision highp float;\nin vec4 vColour;\nout vec4 oColour;\nvoid main() {\n    oColour = vColour;\n}\n";

/// WebRender's `gpu_cache_update`: one point per texel, written into a float texture (the GPU cache of every phone's frame).
const SCATTER_VS: &str = "#version 300 es\nin vec4 aValue;\nin vec2 aPosition;\nout highp vec4 vData;\nvoid main() {\n    vData = aValue;\n    gl_Position = vec4(aPosition * 2.0 - 1.0, 0.0, 1.0);\n    gl_PointSize = 1.0;\n}\n";
const SCATTER_FS: &str = "#version 300 es\nprecision highp float;\nin highp vec4 vData;\nout vec4 oValue;\nvoid main() {\n    oValue = vData;\n}\n";
const SIZE_VS: &str = "#version 300 es\nprecision highp float;\nprecision highp int;\nprecision highp sampler2D;\nuniform sampler2D sTex;\nuniform float uZ;\nflat out mediump vec4 v_size;\nout vec2 vUv;\nvoid main() {\n    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));\n    vUv = p;\n    vec2 size = vec2(textureSize(sTex, 0));\n    v_size = vec4(size / 2048.0, 0.0, 1.0);\n    gl_Position = vec4(p * 2.0 - 1.0, uZ, 1.0);\n}\n";
const SIZE_FS: &str = "#version 300 es\nprecision highp float;\nflat in mediump vec4 v_size;\nout vec4 oColour;\nvoid main() {\n    oColour = v_size;\n}\n";
const FLAGS_VS: &str = "#version 300 es\nprecision highp float;\nprecision highp int;\nlayout(location = 0) in vec2 aPosition;\nlayout(location = 1) in vec4 aRect;\nlayout(location = 2) in ivec4 aData;\nuniform mat4 uTransform;\nflat out mediump vec4 v_color;\nflat out lowp ivec4 v_flags;\nvoid main() {\n    vec2 pos = aRect.xy + aPosition * aRect.zw;\n    gl_Position = uTransform * vec4(pos, 0.0, 1.0);\n    v_color = vec4(0.25, 0.5, 0.75, 1.0);\n    v_flags = aData;\n}\n";
const FLAGS_FS: &str = "#version 300 es\nprecision highp float;\nprecision highp int;\nflat in mediump vec4 v_color;\nflat in lowp ivec4 v_flags;\nout vec4 oColour;\nvoid main() {\n    bool first = v_flags.x == 1500 && v_flags.y == 300 && v_flags.z == 2 && v_flags.w == 1;\n    bool second = v_flags.x == 7 && v_flags.y == 3 && v_flags.z == 0 && v_flags.w == 0;\n    bool colour_ok = abs(v_color.r - 0.25) < 0.01 && abs(v_color.g - 0.5) < 0.01 && abs(v_color.b - 0.75) < 0.01;\n    oColour = (first || second) ? (colour_ok ? vec4(0.0, 1.0, 0.0, 1.0) : vec4(1.0, 1.0, 0.0, 1.0)) : vec4(1.0, 0.0, 0.0, 1.0);\n}\n";

/// The programs of the ladder, the ones that built; a phase whose program did not says why.
struct Programs {
    flat: Result<u32, String>,
    texture: Result<u32, String>,
    fetch_float: Result<u32, String>,
    fetch_int: Result<u32, String>,
    instanced: Result<u32, String>,
    scatter: Result<u32, String>,
    vertex_fetch: Result<u32, String>,
    flags: Result<u32, String>,
    texture_size: Result<u32, String>,
}

impl Programs {
    fn build(gl: &GenericGlContext) -> Self {
        Self {
            flat: program_of(gl, FULL_VS, FLAT_FS, &[]),
            texture: program_of(gl, FULL_VS, TEXTURE_FS, &[]),
            fetch_float: program_of(gl, FULL_VS, FETCH_FLOAT_FS, &[]),
            fetch_int: program_of(gl, FULL_VS, FETCH_INT_FS, &[]),
            instanced: program_of(
                gl,
                INSTANCED_VS,
                INSTANCED_FS,
                &[(0, "aPosition"), (1, "aRect"), (2, "aData")],
            ),
            scatter: program_of(gl, SCATTER_VS, SCATTER_FS, &[(0, "aPosition"), (1, "aValue")]),
            vertex_fetch: program_of(gl, VERTEX_FETCH_VS, VERTEX_FETCH_FS, &[(0, "aPosition"), (1, "aData")]),
            flags: program_of(gl, FLAGS_VS, FLAGS_FS, &[(0, "aPosition"), (1, "aRect"), (2, "aData")]),
            texture_size: program_of(gl, SIZE_VS, SIZE_FS, &[]),
        }
    }

    fn delete(&self, gl: &GenericGlContext) {
        for program in [&self.flat, &self.texture, &self.fetch_float, &self.fetch_int, &self.instanced, &self.scatter, &self.vertex_fetch, &self.flags, &self.texture_size]
            .into_iter()
            .flatten()
        {
            gl.delete_program(*program);
        }
    }

    /// The build errors of the programs that did not build, for the log.
    fn problems(&self) -> Vec<String> {
        [
            ("flat", &self.flat),
            ("texture", &self.texture),
            ("fetch_float", &self.fetch_float),
            ("fetch_int", &self.fetch_int),
            ("instanced", &self.instanced),
            ("scatter", &self.scatter),
            ("vertex_fetch", &self.vertex_fetch),
            ("flags", &self.flags),
            ("texture_size", &self.texture_size),
        ]
        .into_iter()
        .filter_map(|(name, result)| result.as_ref().err().map(|why| format!("program {name}: {why}")))
        .collect()
    }
}

fn int(v: u32) -> i32 {
    i32::try_from(v).unwrap_or(i32::MAX)
}

/// The window's framebuffer, the whole surface, no scissor and no depth test or blending.
fn window_target(gl: &GenericGlContext, target: Target) {
    gl.bind_framebuffer(draw_target(target), target.window);
    read_from(gl, target, target.window);
    gl.viewport(0, 0, int(target.size.0), int(target.size.1));
    gl.disable(glconst::SCISSOR_TEST);
    gl.disable(glconst::DEPTH_TEST);
    gl.disable(glconst::BLEND);
    gl.color_mask(true, true, true, true);
}

/// A full-screen triangle with `program` into the framebuffer bound for drawing, with the uniform `uZ` at `z`.
fn full_screen(gl: &GenericGlContext, program: u32, z: f32) {
    let vao = gl.gen_vertex_arrays(1).first().copied().unwrap_or(0);
    gl.bind_vertex_array(vao);
    gl.use_program(program);
    let z_location = gl.get_uniform_location(program, "uZ");
    gl.uniform_1f(z_location, z);
    gl.draw_arrays(glconst::TRIANGLES, 0, 3);
    gl.use_program(0);
    gl.bind_vertex_array(0);
    gl.delete_vertex_arrays(&[vao]);
}

fn set_colour(gl: &GenericGlContext, program: u32, rgba: [f32; 4]) {
    let location = gl.get_uniform_location(program, "uColour");
    gl.uniform_4f(location, rgba[0], rgba[1], rgba[2], rgba[3]);
}

/// 1. The window's framebuffer cleared.
fn phase_clear(gl: &GenericGlContext, target: Target) -> Row {
    window_target(gl, target);
    gl.clear_color(1.0, 0.0, 0.0, 1.0);
    gl.clear(glconst::COLOR_BUFFER_BIT);
    gl.finish();
    let found = px(gl, target.size.0 / 2, target.size.1 / 2);
    row("clear", &[("centre", RED, found)], gl.get_error(), "")
}

/// 2. A shader draw with a uniform into the window's framebuffer, over a clear of another colour.
fn phase_shader(gl: &GenericGlContext, target: Target, programs: &Programs) -> Row {
    let program = match &programs.flat {
        Ok(program) => *program,
        Err(why) => return failed("shader", format!("the program did not build: {why}")),
    };
    window_target(gl, target);
    gl.clear_color(0.0, 1.0, 0.0, 1.0);
    gl.clear(glconst::COLOR_BUFFER_BIT);
    gl.use_program(program);
    set_colour(gl, program, [0.0, 0.0, 1.0, 1.0]);
    full_screen(gl, program, 0.0);
    gl.finish();
    let found = px(gl, target.size.0 / 2, target.size.1 / 2);
    row("shader", &[("centre", BLUE, found)], gl.get_error(), "")
}

/// 2b. WebRender's dummy texture, the one every solid colour tile of a page is composited from: a 1 x 1 RGBA8 texture with linear
/// filters, made with `glTexStorage2D` and filled with four 0xff bytes by `glTexSubImage2D` from client memory, then sampled over the
/// whole window.
fn phase_dummy_texture(gl: &GenericGlContext, target: Target, programs: &Programs) -> Row {
    let program = match &programs.texture {
        Ok(program) => *program,
        Err(why) => return failed("dummy_texture", format!("the program did not build: {why}")),
    };
    let size = target.size;
    window_target(gl, target);
    gl.clear_color(0.2, 0.2, 0.2, 1.0);
    gl.clear(glconst::COLOR_BUFFER_BIT);
    let texture = gl.gen_textures(1).first().copied().unwrap_or(0);
    gl.active_texture(glconst::TEXTURE0);
    bind_2d(gl, target, texture);
    gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MAG_FILTER, glconst::LINEAR as i32);
    gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MIN_FILTER, glconst::LINEAR as i32);
    gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_WRAP_S, glconst::CLAMP_TO_EDGE as i32);
    gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_WRAP_T, glconst::CLAMP_TO_EDGE as i32);
    gl.bind_buffer(glconst::PIXEL_UNPACK_BUFFER, 0);
    gl.tex_storage_2d(glconst::TEXTURE_2D, 1, glconst::RGBA8, 1, 1);
    gl.pixel_store_i(glconst::UNPACK_ALIGNMENT, 1);
    gl.tex_sub_image_2d(glconst::TEXTURE_2D, 0, 0, 0, 1, 1, glconst::RGBA, glconst::UNSIGNED_BYTE, &[0xff, 0xff, 0xff, 0xff]);
    gl.use_program(program);
    let sampler = gl.get_uniform_location(program, "sTex");
    gl.uniform_1i(sampler, 0);
    full_screen(gl, program, 0.0);
    gl.finish();
    let found = px(gl, size.0 / 2, size.1 / 2);
    let error = gl.get_error();
    gl.bind_texture(glconst::TEXTURE_2D, 0);
    gl.delete_textures(&[texture]);
    row("dummy_texture", &[("centre", WHITE, found)], error, "")
}

/// A 256 x 256 RGBA8 texture (`glTexStorage2D`) as the colour of a framebuffer with four scissored clears in it: bottom left red,
/// bottom right green, top left blue, top right yellow. The texture, the framebuffer and the four read-backs from the framebuffer.
struct Tile {
    texture: u32,
    framebuffer: u32,
    status: u32,
    quadrants: [[u8; 4]; 4],
}

const TILE: i32 = 256;

fn make_tile(gl: &GenericGlContext, target: Target) -> Tile {
    let texture = gl.gen_textures(1).first().copied().unwrap_or(0);
    gl.active_texture(glconst::TEXTURE0);
    bind_2d(gl, target, texture);
    gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MIN_FILTER, glconst::NEAREST as i32);
    gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MAG_FILTER, glconst::NEAREST as i32);
    gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_WRAP_S, glconst::CLAMP_TO_EDGE as i32);
    gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_WRAP_T, glconst::CLAMP_TO_EDGE as i32);
    gl.tex_storage_2d(glconst::TEXTURE_2D, 1, glconst::RGBA8, TILE, TILE);
    let framebuffer = gl.gen_framebuffers(1).first().copied().unwrap_or(0);
    gl.bind_framebuffer(draw_target(target), framebuffer);
    gl.framebuffer_texture_2d(
        draw_target(target),
        glconst::COLOR_ATTACHMENT0,
        glconst::TEXTURE_2D,
        texture,
        0,
    );
    let status = gl.check_frame_buffer_status(draw_target(target));
    gl.viewport(0, 0, TILE, TILE);
    gl.enable(glconst::SCISSOR_TEST);
    let half = TILE / 2;
    for (x, y, colour) in [
        (0, 0, [1.0, 0.0, 0.0, 1.0]),
        (half, 0, [0.0, 1.0, 0.0, 1.0]),
        (0, half, [0.0, 0.0, 1.0, 1.0]),
        (half, half, [1.0, 1.0, 0.0, 1.0]),
    ] {
        gl.scissor(x, y, half, half);
        gl.clear_color(colour[0], colour[1], colour[2], colour[3]);
        gl.clear(glconst::COLOR_BUFFER_BIT);
    }
    gl.disable(glconst::SCISSOR_TEST);
    gl.finish();
    let q = TILE as u32 / 4;
    read_from(gl, target, framebuffer);
    let quadrants = [px(gl, q, q), px(gl, 3 * q, q), px(gl, q, 3 * q), px(gl, 3 * q, 3 * q)];
    gl.bind_framebuffer(draw_target(target), target.window);
    read_from(gl, target, target.window);
    Tile {
        texture,
        framebuffer,
        status,
        quadrants,
    }
}

fn free_tile(gl: &GenericGlContext, target: Target, tile: &Tile) {
    gl.bind_framebuffer(draw_target(target), target.window);
    read_from(gl, target, target.window);
    gl.bind_texture(glconst::TEXTURE_2D, 0);
    gl.delete_framebuffers(&[tile.framebuffer]);
    gl.delete_textures(&[tile.texture]);
}

const QUADRANTS: [[u8; 4]; 4] = [RED, GREEN, BLUE, YELLOW];

/// 3. The picture cache tile and its composite: the texture is drawn into through its framebuffer, then sampled into the window's.
fn phase_fbo_sample(gl: &GenericGlContext, target: Target, programs: &Programs) -> Row {
    let program = match &programs.texture {
        Ok(program) => *program,
        Err(why) => return failed("fbo_sample", format!("the program did not build: {why}")),
    };
    let size = target.size;
    let tile = make_tile(gl, target);
    window_target(gl, target);
    gl.clear_color(0.2, 0.2, 0.2, 1.0);
    gl.clear(glconst::COLOR_BUFFER_BIT);
    gl.active_texture(glconst::TEXTURE0);
    bind_2d(gl, target, tile.texture);
    gl.use_program(program);
    let sampler = gl.get_uniform_location(program, "sTex");
    gl.uniform_1i(sampler, 0);
    full_screen(gl, program, 0.0);
    gl.finish();
    let (w, h) = (size.0.max(8), size.1.max(8));
    let points = [(w / 4, h / 4), (3 * w / 4, h / 4), (w / 4, 3 * h / 4), (3 * w / 4, 3 * h / 4)];
    let tile_names = ["tile bottom left", "tile bottom right", "tile top left", "tile top right"];
    let window_names = ["window bottom left", "window bottom right", "window top left", "window top right"];
    let mut checks: Vec<(&str, [u8; 4], [u8; 4])> = Vec::new();
    for (i, name) in tile_names.into_iter().enumerate() {
        checks.push((name, QUADRANTS[i], tile.quadrants[i]));
    }
    for (i, name) in window_names.into_iter().enumerate() {
        checks.push((name, QUADRANTS[i], px(gl, points[i].0, points[i].1)));
    }
    let error = gl.get_error();
    let note = format!("framebuffer status 0x{:x} (complete is 0x{:x})", tile.status, glconst::FRAMEBUFFER_COMPLETE);
    let mut result = row("fbo_sample", &checks, error, &note);
    result.ok &= tile.status == glconst::FRAMEBUFFER_COMPLETE;
    free_tile(gl, target, &tile);
    result
}

/// 4. WebRender's `composite` in small: VAO, vertex buffer, index buffer, a per-instance buffer with a `vec4` and an `ivec4`. Two
/// instances each draw the whole tile into one half of the window; the first keeps red and green, the second multiplies red by its
/// integer record's zero.
fn phase_instanced(gl: &GenericGlContext, target: Target, programs: &Programs) -> Row {
    let program = match &programs.instanced {
        Ok(program) => *program,
        Err(why) => return failed("instanced", format!("the program did not build: {why}")),
    };
    let size = target.size;
    let tile = make_tile(gl, target);
    window_target(gl, target);
    gl.clear_color(0.2, 0.2, 0.2, 1.0);
    gl.clear(glconst::COLOR_BUFFER_BIT);

    let vao = gl.gen_vertex_arrays(1).first().copied().unwrap_or(0);
    let buffers = gl.gen_buffers(3);
    let (corners, indices, instances) = (
        buffers.first().copied().unwrap_or(0),
        buffers.get(1).copied().unwrap_or(0),
        buffers.get(2).copied().unwrap_or(0),
    );
    gl.bind_vertex_array(vao);
    // The four corners of the unit quad, two floats each.
    let quad: [f32; 8] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0];
    let quad_bytes: Vec<u8> = quad.iter().flat_map(|v| v.to_ne_bytes()).collect();
    gl.bind_buffer(glconst::ARRAY_BUFFER, corners);
    gl.buffer_data_untyped(
        glconst::ARRAY_BUFFER,
        isize::try_from(quad_bytes.len()).unwrap_or(0),
        quad_bytes.as_ptr().cast(),
        glconst::STATIC_DRAW,
    );
    gl.vertex_attrib_pointer(0, 2, glconst::FLOAT, false, 8, 0);
    gl.enable_vertex_attrib_array(0);
    // Two instances of 32 bytes: a rectangle (four floats) and an integer record (four ints).
    let records: [([f32; 4], [i32; 4]); 2] = [
        ([0.0, 0.0, 0.5, 1.0], [1, 2, 0, 0]),
        ([0.5, 0.0, 0.5, 1.0], [0, 2, 0, 0]),
    ];
    let mut instance_bytes: Vec<u8> = Vec::new();
    for (rect, data) in &records {
        instance_bytes.extend(rect.iter().flat_map(|v| v.to_ne_bytes()));
        instance_bytes.extend(data.iter().flat_map(|v| v.to_ne_bytes()));
    }
    gl.bind_buffer(glconst::ARRAY_BUFFER, instances);
    gl.buffer_data_untyped(
        glconst::ARRAY_BUFFER,
        isize::try_from(instance_bytes.len()).unwrap_or(0),
        instance_bytes.as_ptr().cast(),
        glconst::STATIC_DRAW,
    );
    gl.vertex_attrib_pointer(1, 4, glconst::FLOAT, false, 32, 0);
    gl.enable_vertex_attrib_array(1);
    gl.vertex_attrib_divisor(1, 1);
    gl.vertex_attrib_i_pointer(2, 4, glconst::INT, 32, 16);
    gl.enable_vertex_attrib_array(2);
    gl.vertex_attrib_divisor(2, 1);
    let index_values: [u16; 6] = [0, 1, 2, 2, 1, 3];
    let index_bytes: Vec<u8> = index_values.iter().flat_map(|v| v.to_ne_bytes()).collect();
    gl.bind_buffer(glconst::ELEMENT_ARRAY_BUFFER, indices);
    gl.buffer_data_untyped(
        glconst::ELEMENT_ARRAY_BUFFER,
        isize::try_from(index_bytes.len()).unwrap_or(0),
        index_bytes.as_ptr().cast(),
        glconst::STATIC_DRAW,
    );

    gl.active_texture(glconst::TEXTURE0);
    bind_2d(gl, target, tile.texture);
    gl.use_program(program);
    let sampler = gl.get_uniform_location(program, "sColor0");
    gl.uniform_1i(sampler, 0);
    // The unit square onto the clip space.
    let transform: [f32; 16] = [
        2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -1.0, -1.0, 0.0, 1.0,
    ];
    let transform_location = gl.get_uniform_location(program, "uTransform");
    gl.uniform_matrix_4fv(transform_location, false, &transform);
    gl.draw_elements_instanced(glconst::TRIANGLES, 6, glconst::UNSIGNED_SHORT, 0, 2);
    gl.finish();

    // The left instance covers x in [0, w/2) and shows the whole tile (u over the half, v over the height): at 1/8 and 3/8 of the width
    // the tile's left and right halves, at 1/4 and 3/4 of the height its lower and upper halves. The right instance shows it again with
    // red zeroed by its integer record: red becomes black, yellow becomes green.
    let (w, h) = (size.0.max(8), size.1.max(8));
    let at = |fx: u32, fy: u32| px(gl, w * fx / 8, h * fy / 4);
    let checks = [
        ("left instance, tile bottom left (red)", RED, at(1, 1)),
        ("left instance, tile bottom right (green)", GREEN, at(3, 1)),
        ("left instance, tile top left (blue)", BLUE, at(1, 3)),
        ("left instance, tile top right (yellow)", YELLOW, at(3, 3)),
        ("right instance, tile bottom left (red zeroed)", BLACK, at(5, 1)),
        ("right instance, tile bottom right (green)", GREEN, at(7, 1)),
        ("right instance, tile top left (blue)", BLUE, at(5, 3)),
        ("right instance, tile top right (yellow loses red)", GREEN, at(7, 3)),
    ];
    let error = gl.get_error();
    let note = format!("tile framebuffer status 0x{:x}", tile.status);
    let result = row("instanced", &checks, error, &note);

    gl.use_program(0);
    gl.bind_vertex_array(0);
    gl.bind_buffer(glconst::ARRAY_BUFFER, 0);
    gl.bind_buffer(glconst::ELEMENT_ARRAY_BUFFER, 0);
    gl.delete_vertex_arrays(&[vao]);
    gl.delete_buffers(&[corners, indices, instances]);
    free_tile(gl, target, &tile);
    result
}

/// 4b. The quad pipeline's varyings: `flat lowp ivec4 v_flags` (WebRender's `ps_quad` passes flags and the gpu buffer addresses of a pattern
/// in it) and `flat mediump vec4 v_color`, two instances with integer records that need more than eight bits (1500 and 300). Both halves
/// of the window must come out green.
fn phase_flat_lowp(gl: &GenericGlContext, target: Target, programs: &Programs) -> Row {
    flags_phase(gl, target, programs, "flat_lowp", false)
}

/// 4c. The quad of every WebRender draw: `glVertexAttribPointer(0, 2, GL_UNSIGNED_BYTE, normalized, stride 2)` (four corners of two bytes, 0 and 255),
/// the instance's integer record beside it: the shader and the records of `flat_lowp`. The window must come out as green as there.
fn phase_quad_u8norm(gl: &GenericGlContext, target: Target, programs: &Programs) -> Row {
    flags_phase(gl, target, programs, "quad_u8norm", true)
}

fn flags_phase(gl: &GenericGlContext, target: Target, programs: &Programs, name: &'static str, quad_u8: bool) -> Row {
    let program = match &programs.flags {
        Ok(program) => *program,
        Err(why) => return failed(name, format!("the program did not build: {why}")),
    };
    let size = target.size;
    window_target(gl, target);
    gl.clear_color(0.2, 0.2, 0.2, 1.0);
    gl.clear(glconst::COLOR_BUFFER_BIT);

    // The order of WebRender's own VAO set-up (the host trace): the VAO, the quad's buffer with its attribute, the instance buffer with its
    // attributes, the index buffer, and only then the data of the three buffers.
    let vao = gl.gen_vertex_arrays(1).first().copied().unwrap_or(0);
    let buffers = gl.gen_buffers(3);
    let (corners, indices, instances) = (
        buffers.first().copied().unwrap_or(0),
        buffers.get(1).copied().unwrap_or(0),
        buffers.get(2).copied().unwrap_or(0),
    );
    gl.bind_vertex_array(vao);
    gl.bind_buffer(glconst::ARRAY_BUFFER, corners);
    gl.enable_vertex_attrib_array(0);
    gl.vertex_attrib_divisor(0, 0);
    if quad_u8 {
        gl.vertex_attrib_pointer(0, 2, glconst::UNSIGNED_BYTE, true, 2, 0);
    } else {
        gl.vertex_attrib_pointer(0, 2, glconst::FLOAT, false, 8, 0);
    }
    gl.bind_buffer(glconst::ARRAY_BUFFER, instances);
    gl.enable_vertex_attrib_array(1);
    gl.vertex_attrib_divisor(1, 1);
    gl.vertex_attrib_pointer(1, 4, glconst::FLOAT, false, 32, 0);
    gl.enable_vertex_attrib_array(2);
    gl.vertex_attrib_divisor(2, 1);
    gl.vertex_attrib_i_pointer(2, 4, glconst::INT, 32, 16);
    gl.bind_buffer(glconst::ELEMENT_ARRAY_BUFFER, indices);
    let index_values: [u16; 6] = [0, 1, 2, 2, 1, 3];
    let index_bytes: Vec<u8> = index_values.iter().flat_map(|v| v.to_ne_bytes()).collect();
    gl.buffer_data_untyped(
        glconst::ELEMENT_ARRAY_BUFFER,
        isize::try_from(index_bytes.len()).unwrap_or(0),
        index_bytes.as_ptr().cast(),
        glconst::STATIC_DRAW,
    );
    let quad_bytes: Vec<u8> = if quad_u8 {
        vec![0, 0, 255, 0, 0, 255, 255, 255]
    } else {
        [0.0_f32, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0].iter().flat_map(|v| v.to_ne_bytes()).collect()
    };
    gl.bind_buffer(glconst::ARRAY_BUFFER, corners);
    gl.buffer_data_untyped(
        glconst::ARRAY_BUFFER,
        isize::try_from(quad_bytes.len()).unwrap_or(0),
        quad_bytes.as_ptr().cast(),
        glconst::STATIC_DRAW,
    );
    let records: [([f32; 4], [i32; 4]); 2] = [
        ([0.0, 0.0, 0.5, 1.0], [1500, 300, 2, 1]),
        ([0.5, 0.0, 0.5, 1.0], [7, 3, 0, 0]),
    ];
    let mut instance_bytes: Vec<u8> = Vec::new();
    for (rect, data) in &records {
        instance_bytes.extend(rect.iter().flat_map(|v| v.to_ne_bytes()));
        instance_bytes.extend(data.iter().flat_map(|v| v.to_ne_bytes()));
    }
    gl.bind_buffer(glconst::ARRAY_BUFFER, instances);
    gl.buffer_data_untyped(
        glconst::ARRAY_BUFFER,
        isize::try_from(instance_bytes.len()).unwrap_or(0),
        instance_bytes.as_ptr().cast(),
        glconst::STREAM_DRAW,
    );
    gl.use_program(program);
    let transform: [f32; 16] = [
        2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -1.0, -1.0, 0.0, 1.0,
    ];
    let transform_location = gl.get_uniform_location(program, "uTransform");
    gl.uniform_matrix_4fv(transform_location, false, &transform);
    gl.draw_elements_instanced(glconst::TRIANGLES, 6, glconst::UNSIGNED_SHORT, 0, 2);
    gl.finish();

    let (w, h) = (size.0.max(8), size.1.max(8));
    let checks = [
        ("left instance (1500, 300, 2, 1)", GREEN, px(gl, w / 4, h / 2)),
        ("right instance (7, 3, 0, 0)", GREEN, px(gl, 3 * w / 4, h / 2)),
    ];
    let error = gl.get_error();
    let result = row(name, &checks, error, "red: a flat lowp ivec4 varying lost its value; yellow: the mediump colour did; nothing: no quad");

    gl.use_program(0);
    gl.bind_vertex_array(0);
    gl.bind_buffer(glconst::ARRAY_BUFFER, 0);
    gl.bind_buffer(glconst::ELEMENT_ARRAY_BUFFER, 0);
    gl.delete_vertex_arrays(&[vao]);
    gl.delete_buffers(&[corners, indices, instances]);
    result
}

/// 4e. Big render targets: WebRender's shared render targets are 2048 x 2048 RGBA8 (16 MB, 2^24 bytes). A texture of each size is given four
/// coloured quadrants by scissored clears in its framebuffer and sampled over the window with the plain texture shader: a sampler that cannot
/// address the far end of such a texture reads zero there.
fn phase_big_texture(gl: &GenericGlContext, target: Target, programs: &Programs) -> Row {
    let program = match &programs.texture {
        Ok(program) => *program,
        Err(why) => return failed("big_texture", format!("the program did not build: {why}")),
    };
    let size = target.size;
    let cases: [(&str, i32, i32); 4] = [
        ("RGBA8 1024x1024 (4 MB)", 1024, 1024),
        ("RGBA8 2048x1024 (8 MB)", 2048, 1024),
        ("RGBA8 2048x2048 (16 MB)", 2048, 2048),
        ("RGBA8 4096x1024 (16 MB)", 4096, 1024),
    ];
    let max = {
        let mut value = [0_i32];
        // SAFETY: a query that writes one integer into a slice of one.
        unsafe { gl.get_integer_v(glconst::MAX_TEXTURE_SIZE, &mut value) };
        value[0]
    };
    let mut owned: Vec<(String, [u8; 4], [u8; 4])> = Vec::new();
    for (name, width, height) in cases {
        if width > max || height > max {
            continue;
        }
        let texture = gl.gen_textures(1).first().copied().unwrap_or(0);
        gl.active_texture(glconst::TEXTURE0);
        bind_2d(gl, target, texture);
        gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MIN_FILTER, glconst::NEAREST as i32);
        gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MAG_FILTER, glconst::NEAREST as i32);
        gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_WRAP_S, glconst::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_WRAP_T, glconst::CLAMP_TO_EDGE as i32);
        gl.tex_storage_2d(glconst::TEXTURE_2D, 1, glconst::RGBA8, width, height);
        let framebuffer = gl.gen_framebuffers(1).first().copied().unwrap_or(0);
        gl.bind_framebuffer(draw_target(target), framebuffer);
        gl.framebuffer_texture_2d(draw_target(target), glconst::COLOR_ATTACHMENT0, glconst::TEXTURE_2D, texture, 0);
        gl.viewport(0, 0, width, height);
        gl.enable(glconst::SCISSOR_TEST);
        let (half_w, half_h) = (width / 2, height / 2);
        for (x, y, colour) in [
            (0, 0, [1.0, 0.0, 0.0, 1.0]),
            (half_w, 0, [0.0, 1.0, 0.0, 1.0]),
            (0, half_h, [0.0, 0.0, 1.0, 1.0]),
            (half_w, half_h, [1.0, 1.0, 0.0, 1.0]),
        ] {
            gl.scissor(x, y, half_w, half_h);
            gl.clear_color(colour[0], colour[1], colour[2], colour[3]);
            gl.clear(glconst::COLOR_BUFFER_BIT);
        }
        gl.disable(glconst::SCISSOR_TEST);
        gl.bind_framebuffer(draw_target(target), target.window);
        window_target(gl, target);
        gl.clear_color(0.2, 0.2, 0.2, 1.0);
        gl.clear(glconst::COLOR_BUFFER_BIT);
        gl.active_texture(glconst::TEXTURE0);
        bind_2d(gl, target, texture);
        gl.use_program(program);
        let sampler = gl.get_uniform_location(program, "sTex");
        gl.uniform_1i(sampler, 0);
        full_screen(gl, program, 0.0);
        gl.finish();
        let (w, h) = (size.0.max(8), size.1.max(8));
        let points = [(w / 4, h / 4), (3 * w / 4, h / 4), (w / 4, 3 * h / 4), (3 * w / 4, 3 * h / 4)];
        let corners = ["bottom left", "bottom right", "top left", "top right"];
        for (i, corner) in corners.into_iter().enumerate() {
            owned.push((format!("{name} {corner}"), QUADRANTS[i], px(gl, points[i].0, points[i].1)));
        }
        gl.bind_texture(glconst::TEXTURE_2D, 0);
        gl.delete_framebuffers(&[framebuffer]);
        gl.delete_textures(&[texture]);
    }
    let checks: Vec<(&str, [u8; 4], [u8; 4])> = owned.iter().map(|(name, expected, found)| (name.as_str(), *expected, *found)).collect();
    let error = gl.get_error();
    row("big_texture", &checks, error, &format!("MAX_TEXTURE_SIZE {max}"))
}

/// 4d. `textureSize()` in the VERTEX stage (every textured shader of WebRender normalises its UV rectangle by it: `vs_init_sample_color0`, `brush_image`):
/// textures of several sizes and kinds are bound, the vertex shader writes `size / 2048` into a `flat mediump vec4` varying, the window shows it. A
/// size read as zero or wrong makes every UV of that texture NaN or infinite, and its draws paint nothing.
fn phase_texture_size(gl: &GenericGlContext, target: Target, programs: &Programs) -> Row {
    let program = match &programs.texture_size {
        Ok(program) => *program,
        Err(why) => return failed("texture_size", format!("the program did not build: {why}")),
    };
    let size = target.size;
    // (name, internal format, side, drawn to first, the colour 255 * size / 2048 in red and green)
    let cases: [(&str, u32, i32, bool, [u8; 4]); 5] = [
        ("2048x2048 RGBA8 render target", glconst::RGBA8, 2048, true, [255, 255, 0, 255]),
        ("1024x1024 RGBA8 render target", glconst::RGBA8, 1024, true, [127, 127, 0, 255]),
        ("2048x2048 R8 render target", glconst::R8, 2048, true, [255, 255, 0, 255]),
        ("2048x2048 RGBA8, never drawn to", glconst::RGBA8, 2048, false, [255, 255, 0, 255]),
        ("64x64 RGBA8", glconst::RGBA8, 64, false, [8, 8, 0, 255]),
    ];
    let mut checks: Vec<(&str, [u8; 4], [u8; 4])> = Vec::new();
    for (name, format, side, drawn_to, expected) in cases {
        let texture = gl.gen_textures(1).first().copied().unwrap_or(0);
        gl.active_texture(glconst::TEXTURE0);
        bind_2d(gl, target, texture);
        gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MAG_FILTER, glconst::LINEAR as i32);
        gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MIN_FILTER, glconst::LINEAR as i32);
        gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_WRAP_S, glconst::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_WRAP_T, glconst::CLAMP_TO_EDGE as i32);
        gl.tex_storage_2d(glconst::TEXTURE_2D, 1, format, side, side);
        if drawn_to {
            let framebuffer = gl.gen_framebuffers(1).first().copied().unwrap_or(0);
            gl.bind_framebuffer(draw_target(target), framebuffer);
            gl.framebuffer_texture_2d(draw_target(target), glconst::COLOR_ATTACHMENT0, glconst::TEXTURE_2D, texture, 0);
            gl.viewport(0, 0, side, side);
            gl.clear_color(1.0, 0.0, 0.0, 1.0);
            gl.clear(glconst::COLOR_BUFFER_BIT);
            gl.bind_framebuffer(draw_target(target), target.window);
            gl.delete_framebuffers(&[framebuffer]);
        }
        window_target(gl, target);
        gl.clear_color(0.2, 0.2, 0.2, 1.0);
        gl.clear(glconst::COLOR_BUFFER_BIT);
        gl.active_texture(glconst::TEXTURE0);
        bind_2d(gl, target, texture);
        gl.use_program(program);
        let sampler = gl.get_uniform_location(program, "sTex");
        gl.uniform_1i(sampler, 0);
        full_screen(gl, program, 0.0);
        gl.finish();
        checks.push((name, expected, px(gl, size.0 / 2, size.1 / 2)));
        gl.bind_texture(glconst::TEXTURE_2D, 0);
        gl.delete_textures(&[texture]);
    }
    let error = gl.get_error();
    row("texture_size", &checks, error, "")
}

/// 5. Data textures: four texels of an RGBA32F texture on the top half, four of an RGBA32I texture on the bottom half, each read with
/// `texelFetch` and shown as a band.
fn phase_data_textures(gl: &GenericGlContext, target: Target, programs: &Programs) -> Row {
    let (float_program, int_program) = match (&programs.fetch_float, &programs.fetch_int) {
        (Ok(f), Ok(i)) => (*f, *i),
        (Err(why), _) | (_, Err(why)) => {
            return failed("data_textures", format!("a program did not build: {why}"))
        }
    };
    let size = target.size;
    window_target(gl, target);
    gl.clear_color(0.2, 0.2, 0.2, 1.0);
    gl.clear(glconst::COLOR_BUFFER_BIT);

    let textures = gl.gen_textures(2);
    let (float_texture, int_texture) = (
        textures.first().copied().unwrap_or(0),
        textures.get(1).copied().unwrap_or(0),
    );
    // Four texels: red, green, blue, white.
    let floats: [f32; 16] = [
        1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
    ];
    let float_bytes: Vec<u8> = floats.iter().flat_map(|v| v.to_ne_bytes()).collect();
    gl.active_texture(glconst::TEXTURE0);
    bind_2d(gl, target, float_texture);
    gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MIN_FILTER, glconst::NEAREST as i32);
    gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MAG_FILTER, glconst::NEAREST as i32);
    gl.tex_storage_2d(glconst::TEXTURE_2D, 1, glconst::RGBA32F, 4, 1);
    gl.tex_sub_image_2d(glconst::TEXTURE_2D, 0, 0, 0, 4, 1, glconst::RGBA, glconst::FLOAT, &float_bytes);
    let ints: [i32; 16] = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];
    let int_bytes: Vec<u8> = ints.iter().flat_map(|v| v.to_ne_bytes()).collect();
    bind_2d(gl, target, int_texture);
    gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MIN_FILTER, glconst::NEAREST as i32);
    gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MAG_FILTER, glconst::NEAREST as i32);
    gl.tex_storage_2d(glconst::TEXTURE_2D, 1, glconst::RGBA32I, 4, 1);
    gl.tex_sub_image_2d(glconst::TEXTURE_2D, 0, 0, 0, 4, 1, glconst::RGBA_INTEGER, glconst::INT, &int_bytes);

    let (w, h) = (int(size.0), int(size.1));
    // The top half: floats.
    bind_2d(gl, target, float_texture);
    gl.viewport(0, h / 2, w, h - h / 2);
    gl.use_program(float_program);
    let location = gl.get_uniform_location(float_program, "sData");
    gl.uniform_1i(location, 0);
    full_screen(gl, float_program, 0.0);
    // The bottom half: integers.
    bind_2d(gl, target, int_texture);
    gl.viewport(0, 0, w, h / 2);
    gl.use_program(int_program);
    let location = gl.get_uniform_location(int_program, "sData");
    gl.uniform_1i(location, 0);
    full_screen(gl, int_program, 0.0);
    gl.finish();

    let (uw, uh) = (size.0.max(8), size.1.max(8));
    let bands = [RED, GREEN, BLUE, WHITE];
    let mut checks: Vec<(&str, [u8; 4], [u8; 4])> = Vec::new();
    let names_float = ["float band 1", "float band 2", "float band 3", "float band 4"];
    let names_int = ["integer band 1", "integer band 2", "integer band 3", "integer band 4"];
    for (i, name) in names_float.into_iter().enumerate() {
        let x = uw * (2 * u32::try_from(i).unwrap_or(0) + 1) / 8;
        checks.push((name, bands[i], px(gl, x, 3 * uh / 4)));
    }
    for (i, name) in names_int.into_iter().enumerate() {
        let x = uw * (2 * u32::try_from(i).unwrap_or(0) + 1) / 8;
        checks.push((name, bands[i], px(gl, x, uh / 4)));
    }
    let error = gl.get_error();
    let result = row("data_textures", &checks, error, "");

    gl.bind_texture(glconst::TEXTURE_2D, 0);
    gl.delete_textures(&[float_texture, int_texture]);
    gl.viewport(0, 0, w, h);
    result
}

/// 5d. The device's texture upload path (`TextureUploader` without buffer storage): a pixel unpack buffer is given its storage with
/// `glBufferData`, mapped with `glMapBufferRange(WRITE)`, filled, unmapped, and `glTexSubImage2D` reads it from offset 0 with
/// `GL_UNPACK_ROW_LENGTH` set (the float data texture); the SAME buffer is mapped a second time with `MAP_UNSYNCHRONIZED_BIT` (the
/// recycled one), filled with other data and read by a second upload (the integer data texture). The window shows the two textures
/// the way `data_textures` does: floats in the top half, integers in the bottom half.
fn phase_pbo_upload(gl: &GenericGlContext, target: Target, programs: &Programs) -> Row {
    let (float_program, int_program) = match (&programs.fetch_float, &programs.fetch_int) {
        (Ok(f), Ok(i)) => (*f, *i),
        (Err(why), _) | (_, Err(why)) => return failed("pbo_upload", format!("a program did not build: {why}")),
    };
    let size = target.size;
    window_target(gl, target);
    gl.clear_color(0.2, 0.2, 0.2, 1.0);
    gl.clear(glconst::COLOR_BUFFER_BIT);

    let textures = gl.gen_textures(2);
    let (float_texture, int_texture) = (
        textures.first().copied().unwrap_or(0),
        textures.get(1).copied().unwrap_or(0),
    );
    gl.active_texture(glconst::TEXTURE0);
    for (texture, format) in [(float_texture, glconst::RGBA32F), (int_texture, glconst::RGBA32I)] {
        bind_2d(gl, target, texture);
        gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MIN_FILTER, glconst::NEAREST as i32);
        gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MAG_FILTER, glconst::NEAREST as i32);
        gl.tex_storage_2d(glconst::TEXTURE_2D, 1, format, 4, 1);
    }
    let floats: [f32; 16] = [
        1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
    ];
    let float_bytes: Vec<u8> = floats.iter().flat_map(|v| v.to_ne_bytes()).collect();
    let ints: [i32; 16] = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];
    let int_bytes: Vec<u8> = ints.iter().flat_map(|v| v.to_ne_bytes()).collect();
    let length = isize::try_from(float_bytes.len()).unwrap_or(64);

    let pbo = gl.gen_buffers(1).first().copied().unwrap_or(0);
    gl.bind_buffer(glconst::PIXEL_UNPACK_BUFFER, pbo);
    gl.buffer_data_untyped(glconst::PIXEL_UNPACK_BUFFER, length, std::ptr::null(), glconst::STREAM_DRAW);
    let mut problems: Vec<String> = Vec::new();
    // First use: a freshly allocated buffer, mapped for writing.
    let mapped = gl.map_buffer_range(glconst::PIXEL_UNPACK_BUFFER, 0, length, glconst::MAP_WRITE_BIT);
    if mapped.is_null() {
        problems.push("glMapBufferRange returned null (first use)".to_owned());
    } else {
        // SAFETY: the driver returned a writable mapping of `length` bytes, which is `float_bytes.len()`.
        unsafe { std::ptr::copy_nonoverlapping(float_bytes.as_ptr(), mapped.cast::<u8>(), float_bytes.len()) };
        if gl.unmap_buffer(glconst::PIXEL_UNPACK_BUFFER) == 0 {
            problems.push("glUnmapBuffer said the data was lost (first use)".to_owned());
        }
        gl.pixel_store_i(glconst::UNPACK_ROW_LENGTH, 4);
        bind_2d(gl, target, float_texture);
        gl.tex_sub_image_2d_pbo(glconst::TEXTURE_2D, 0, 0, 0, 4, 1, glconst::RGBA, glconst::FLOAT, 0);
        gl.pixel_store_i(glconst::UNPACK_ROW_LENGTH, 0);
    }
    // Second use: the buffer comes back from the pool and is mapped again without waiting.
    let mapped = gl.map_buffer_range(
        glconst::PIXEL_UNPACK_BUFFER,
        0,
        length,
        glconst::MAP_WRITE_BIT | glconst::MAP_UNSYNCHRONIZED_BIT,
    );
    if mapped.is_null() {
        problems.push("glMapBufferRange returned null (second use)".to_owned());
    } else {
        // SAFETY: as above.
        unsafe { std::ptr::copy_nonoverlapping(int_bytes.as_ptr(), mapped.cast::<u8>(), int_bytes.len()) };
        if gl.unmap_buffer(glconst::PIXEL_UNPACK_BUFFER) == 0 {
            problems.push("glUnmapBuffer said the data was lost (second use)".to_owned());
        }
        gl.pixel_store_i(glconst::UNPACK_ROW_LENGTH, 4);
        bind_2d(gl, target, int_texture);
        gl.tex_sub_image_2d_pbo(glconst::TEXTURE_2D, 0, 0, 0, 4, 1, glconst::RGBA_INTEGER, glconst::INT, 0);
        gl.pixel_store_i(glconst::UNPACK_ROW_LENGTH, 0);
    }
    gl.bind_buffer(glconst::PIXEL_UNPACK_BUFFER, 0);

    let (w, h) = (int(size.0), int(size.1));
    bind_2d(gl, target, float_texture);
    gl.viewport(0, h / 2, w, h - h / 2);
    gl.use_program(float_program);
    let location = gl.get_uniform_location(float_program, "sData");
    gl.uniform_1i(location, 0);
    full_screen(gl, float_program, 0.0);
    bind_2d(gl, target, int_texture);
    gl.viewport(0, 0, w, h / 2);
    gl.use_program(int_program);
    let location = gl.get_uniform_location(int_program, "sData");
    gl.uniform_1i(location, 0);
    full_screen(gl, int_program, 0.0);
    gl.finish();

    let (uw, uh) = (size.0.max(8), size.1.max(8));
    let bands = [RED, GREEN, BLUE, WHITE];
    let mut checks: Vec<(&str, [u8; 4], [u8; 4])> = Vec::new();
    let names_float = ["float band 1", "float band 2", "float band 3", "float band 4"];
    let names_int = ["integer band 1", "integer band 2", "integer band 3", "integer band 4"];
    for (i, name) in names_float.into_iter().enumerate() {
        let x = uw * (2 * u32::try_from(i).unwrap_or(0) + 1) / 8;
        checks.push((name, bands[i], px(gl, x, 3 * uh / 4)));
    }
    for (i, name) in names_int.into_iter().enumerate() {
        let x = uw * (2 * u32::try_from(i).unwrap_or(0) + 1) / 8;
        checks.push((name, bands[i], px(gl, x, uh / 4)));
    }
    let error = gl.get_error();
    let mut result = row("pbo_upload", &checks, error, &problems.join("; "));
    result.ok &= problems.is_empty();

    gl.bind_texture(glconst::TEXTURE_2D, 0);
    gl.delete_textures(&[float_texture, int_texture]);
    gl.delete_buffers(&[pbo]);
    gl.viewport(0, 0, w, h);
    result
}

/// 5c. WebRender's data path in the vertex shader: two instances of a quad, each with an integer attribute that names a texel of an
/// RGBA32I texture, which names the texels (rectangle, colour) of an RGBA32F texture, all fetched with `texelFetch` in the VERTEX stage.
/// The left half of the window must come out red and the right half blue.
fn phase_vertex_fetch(gl: &GenericGlContext, target: Target, programs: &Programs) -> Row {
    let program = match &programs.vertex_fetch {
        Ok(program) => *program,
        Err(why) => return failed("vertex_fetch", format!("the program did not build: {why}")),
    };
    let size = target.size;
    window_target(gl, target);
    gl.clear_color(0.2, 0.2, 0.2, 1.0);
    gl.clear(glconst::COLOR_BUFFER_BIT);

    let textures = gl.gen_textures(2);
    let (cache_texture, header_texture) = (
        textures.first().copied().unwrap_or(0),
        textures.get(1).copied().unwrap_or(0),
    );
    // The float texture, 1024 x 2 texels: a rectangle and a colour for each of two quads, at the addresses 1030 and 1034 (the second row, so
    // that the address arithmetic of the shader has to divide).
    let mut cache = vec![0.0_f32; 1024 * 2 * 4];
    let put = |cache: &mut Vec<f32>, address: usize, value: [f32; 4]| {
        cache[address * 4..address * 4 + 4].copy_from_slice(&value);
    };
    put(&mut cache, 1030, [0.0, 0.0, 0.5, 1.0]); // the left half
    put(&mut cache, 1031, [1.0, 0.0, 0.0, 1.0]); // red
    put(&mut cache, 1034, [0.5, 0.0, 0.5, 1.0]); // the right half
    put(&mut cache, 1035, [0.0, 0.0, 1.0, 1.0]); // blue
    let cache_bytes: Vec<u8> = cache.iter().flat_map(|v| v.to_ne_bytes()).collect();
    gl.active_texture(glconst::TEXTURE0);
    bind_2d(gl, target, cache_texture);
    gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MIN_FILTER, glconst::NEAREST as i32);
    gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MAG_FILTER, glconst::NEAREST as i32);
    gl.tex_storage_2d(glconst::TEXTURE_2D, 1, glconst::RGBA32F, 1024, 2);
    gl.tex_sub_image_2d(glconst::TEXTURE_2D, 0, 0, 0, 1024, 2, glconst::RGBA, glconst::FLOAT, &cache_bytes);
    // The integer texture: two headers, the first names the address 1030 of the cache, the second 1034.
    let headers: [i32; 8] = [1030, 0, 0, 0, 1034, 0, 0, 0];
    let header_bytes: Vec<u8> = headers.iter().flat_map(|v| v.to_ne_bytes()).collect();
    gl.active_texture(glconst::TEXTURE0 + 1);
    bind_2d(gl, target, header_texture);
    gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MIN_FILTER, glconst::NEAREST as i32);
    gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MAG_FILTER, glconst::NEAREST as i32);
    gl.tex_storage_2d(glconst::TEXTURE_2D, 1, glconst::RGBA32I, 2, 1);
    gl.tex_sub_image_2d(glconst::TEXTURE_2D, 0, 0, 0, 2, 1, glconst::RGBA_INTEGER, glconst::INT, &header_bytes);

    let vao = gl.gen_vertex_arrays(1).first().copied().unwrap_or(0);
    let buffers = gl.gen_buffers(3);
    let (corners, indices, instances) = (
        buffers.first().copied().unwrap_or(0),
        buffers.get(1).copied().unwrap_or(0),
        buffers.get(2).copied().unwrap_or(0),
    );
    gl.bind_vertex_array(vao);
    let quad: [f32; 8] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0];
    let quad_bytes: Vec<u8> = quad.iter().flat_map(|v| v.to_ne_bytes()).collect();
    gl.bind_buffer(glconst::ARRAY_BUFFER, corners);
    gl.buffer_data_untyped(
        glconst::ARRAY_BUFFER,
        isize::try_from(quad_bytes.len()).unwrap_or(0),
        quad_bytes.as_ptr().cast(),
        glconst::STATIC_DRAW,
    );
    gl.vertex_attrib_pointer(0, 2, glconst::FLOAT, false, 8, 0);
    gl.enable_vertex_attrib_array(0);
    let records: [i32; 8] = [0, 0, 0, 0, 1, 0, 0, 0];
    let record_bytes: Vec<u8> = records.iter().flat_map(|v| v.to_ne_bytes()).collect();
    gl.bind_buffer(glconst::ARRAY_BUFFER, instances);
    gl.buffer_data_untyped(
        glconst::ARRAY_BUFFER,
        isize::try_from(record_bytes.len()).unwrap_or(0),
        record_bytes.as_ptr().cast(),
        glconst::STATIC_DRAW,
    );
    gl.vertex_attrib_i_pointer(1, 4, glconst::INT, 16, 0);
    gl.enable_vertex_attrib_array(1);
    gl.vertex_attrib_divisor(1, 1);
    let index_values: [u16; 6] = [0, 1, 2, 2, 1, 3];
    let index_bytes: Vec<u8> = index_values.iter().flat_map(|v| v.to_ne_bytes()).collect();
    gl.bind_buffer(glconst::ELEMENT_ARRAY_BUFFER, indices);
    gl.buffer_data_untyped(
        glconst::ELEMENT_ARRAY_BUFFER,
        isize::try_from(index_bytes.len()).unwrap_or(0),
        index_bytes.as_ptr().cast(),
        glconst::STATIC_DRAW,
    );
    gl.use_program(program);
    let cache_sampler = gl.get_uniform_location(program, "sGpuCache");
    gl.uniform_1i(cache_sampler, 0);
    let header_sampler = gl.get_uniform_location(program, "sHeaders");
    gl.uniform_1i(header_sampler, 1);
    gl.draw_elements_instanced(glconst::TRIANGLES, 6, glconst::UNSIGNED_SHORT, 0, 2);
    gl.finish();

    let (w, h) = (size.0.max(8), size.1.max(8));
    let checks = [
        ("left quad (red)", RED, px(gl, w / 4, h / 2)),
        ("right quad (blue)", BLUE, px(gl, 3 * w / 4, h / 2)),
    ];
    let error = gl.get_error();
    let result = row("vertex_fetch", &checks, error, "");

    gl.use_program(0);
    gl.bind_vertex_array(0);
    gl.bind_buffer(glconst::ARRAY_BUFFER, 0);
    gl.bind_buffer(glconst::ELEMENT_ARRAY_BUFFER, 0);
    gl.delete_vertex_arrays(&[vao]);
    gl.delete_buffers(&[corners, indices, instances]);
    gl.active_texture(glconst::TEXTURE0 + 1);
    gl.bind_texture(glconst::TEXTURE_2D, 0);
    gl.active_texture(glconst::TEXTURE0);
    gl.bind_texture(glconst::TEXTURE_2D, 0);
    gl.delete_textures(&[cache_texture, header_texture]);
    result
}

/// 5b. WebRender's GPU cache update: five points drawn with `gpu_cache_update`'s shaders into a 16 x 4 RGBA32F texture through its
/// framebuffer (positions as normalised unsigned shorts, values as floats, `gl_PointSize` 1, blending off), the texels read back as
/// floats, then fetched with `texelFetch` into the window as four bands.
fn phase_scatter(gl: &GenericGlContext, target: Target, programs: &Programs) -> Row {
    let (scatter, fetch) = match (&programs.scatter, &programs.fetch_float) {
        (Ok(s), Ok(f)) => (*s, *f),
        (Err(why), _) | (_, Err(why)) => return failed("scatter", format!("a program did not build: {why}")),
    };
    let size = target.size;
    let texture = gl.gen_textures(1).first().copied().unwrap_or(0);
    gl.active_texture(glconst::TEXTURE0);
    bind_2d(gl, target, texture);
    gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MIN_FILTER, glconst::NEAREST as i32);
    gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MAG_FILTER, glconst::NEAREST as i32);
    gl.tex_storage_2d(glconst::TEXTURE_2D, 1, glconst::RGBA32F, 16, 4);
    let framebuffer = gl.gen_framebuffers(1).first().copied().unwrap_or(0);
    gl.bind_framebuffer(draw_target(target), framebuffer);
    gl.framebuffer_texture_2d(
        draw_target(target),
        glconst::COLOR_ATTACHMENT0,
        glconst::TEXTURE_2D,
        texture,
        0,
    );
    let status = gl.check_frame_buffer_status(draw_target(target));
    gl.viewport(0, 0, 16, 4);
    gl.disable(glconst::SCISSOR_TEST);
    gl.disable(glconst::DEPTH_TEST);
    gl.disable(glconst::BLEND);
    gl.color_mask(true, true, true, true);
    gl.clear_color(0.0, 0.0, 0.0, 0.0);
    gl.clear(glconst::COLOR_BUFFER_BIT);

    // Five texels of row 0: u = 0, 1, 2, 3 and 5. The position of texel u is ((2u + 1) << 15) / 16, of row 0 ((1) << 15) / 4.
    let texels: [(u16, [f32; 4]); 5] = [
        (0, [1.0, 0.0, 0.0, 1.0]),
        (1, [0.0, 1.0, 0.0, 1.0]),
        (2, [0.0, 0.0, 1.0, 1.0]),
        (3, [1.0, 1.0, 1.0, 1.0]),
        (5, [0.25, 0.5, 0.75, 2.0]),
    ];
    let mut position_bytes: Vec<u8> = Vec::new();
    let mut value_bytes: Vec<u8> = Vec::new();
    for (u, value) in &texels {
        let x = ((2 * u32::from(*u) + 1) << 15) / 16;
        let y = (1_u32 << 15) / 4;
        position_bytes.extend(u16::try_from(x).unwrap_or(0).to_ne_bytes());
        position_bytes.extend(u16::try_from(y).unwrap_or(0).to_ne_bytes());
        value_bytes.extend(value.iter().flat_map(|v| v.to_ne_bytes()));
    }
    let vao = gl.gen_vertex_arrays(1).first().copied().unwrap_or(0);
    let buffers = gl.gen_buffers(2);
    let (positions, values) = (buffers.first().copied().unwrap_or(0), buffers.get(1).copied().unwrap_or(0));
    gl.bind_vertex_array(vao);
    gl.bind_buffer(glconst::ARRAY_BUFFER, positions);
    gl.buffer_data_untyped(
        glconst::ARRAY_BUFFER,
        isize::try_from(position_bytes.len()).unwrap_or(0),
        position_bytes.as_ptr().cast(),
        glconst::STATIC_DRAW,
    );
    gl.enable_vertex_attrib_array(0);
    gl.vertex_attrib_divisor(0, 0);
    gl.vertex_attrib_pointer(0, 2, glconst::UNSIGNED_SHORT, true, 4, 0);
    gl.bind_buffer(glconst::ARRAY_BUFFER, values);
    gl.buffer_data_untyped(
        glconst::ARRAY_BUFFER,
        isize::try_from(value_bytes.len()).unwrap_or(0),
        value_bytes.as_ptr().cast(),
        glconst::STATIC_DRAW,
    );
    gl.enable_vertex_attrib_array(1);
    gl.vertex_attrib_divisor(1, 0);
    gl.vertex_attrib_pointer(1, 4, glconst::FLOAT, false, 16, 0);
    gl.use_program(scatter);
    gl.draw_arrays(glconst::POINTS, 0, 5);
    gl.finish();

    // The texels as floats.
    read_from(gl, target, framebuffer);
    let mut found_all = true;
    let mut detail: Vec<String> = Vec::new();
    for (u, value) in &texels {
        let mut bytes = [0_u8; 16];
        gl.read_pixels_into_buffer(i32::from(*u), 0, 1, 1, glconst::RGBA, glconst::FLOAT, &mut bytes);
        let got: Vec<f32> = bytes.chunks_exact(4).map(|c| f32::from_ne_bytes([c[0], c[1], c[2], c[3]])).collect();
        let same = got.iter().zip(value.iter()).all(|(g, e)| (g - e).abs() < 0.002);
        found_all &= same;
        detail.push(format!("texel {u} expect {value:?} got {got:?}"));
    }
    let error_after_read = gl.get_error();

    gl.use_program(0);
    gl.bind_vertex_array(0);
    gl.bind_buffer(glconst::ARRAY_BUFFER, 0);
    gl.delete_vertex_arrays(&[vao]);
    gl.delete_buffers(&[positions, values]);

    // The texture as a source: its first four texels as bands in the window.
    window_target(gl, target);
    gl.clear_color(0.2, 0.2, 0.2, 1.0);
    gl.clear(glconst::COLOR_BUFFER_BIT);
    gl.active_texture(glconst::TEXTURE0);
    bind_2d(gl, target, texture);
    gl.use_program(fetch);
    let sampler = gl.get_uniform_location(fetch, "sData");
    gl.uniform_1i(sampler, 0);
    full_screen(gl, fetch, 0.0);
    gl.finish();
    let (w, h) = (size.0.max(8), size.1.max(8));
    let bands = [RED, GREEN, BLUE, WHITE];
    let names = ["band 1", "band 2", "band 3", "band 4"];
    let mut checks: Vec<(&str, [u8; 4], [u8; 4])> = Vec::new();
    for (i, name) in names.into_iter().enumerate() {
        let x = w * (2 * u32::try_from(i).unwrap_or(0) + 1) / 8;
        checks.push((name, bands[i], px(gl, x, h / 2)));
    }
    let error = gl.get_error() | error_after_read;
    let note = format!(
        "float framebuffer status 0x{status:x}; read back as floats: {}; {}",
        if found_all { "all as written" } else { "WRONG" },
        detail.join("; ")
    );
    let mut result = row("scatter", &checks, error, &note);
    result.ok &= found_all && status == glconst::FRAMEBUFFER_COMPLETE;

    gl.bind_framebuffer(draw_target(target), target.window);
    read_from(gl, target, target.window);
    gl.bind_texture(glconst::TEXTURE_2D, 0);
    gl.delete_framebuffers(&[framebuffer]);
    gl.delete_textures(&[texture]);
    result
}

/// 6. The depth pass of a tile: a framebuffer with a colour texture and a 24-bit depth renderbuffer, depth cleared to 1.0 with
/// `glClearBufferfv` (the way the device layer clears an ES context), a green triangle at depth 0.25 and a red one behind it at 0.75
/// with `LEQUAL`: the red one must fail the test. The result is shown in the window through the sampling shader.
fn phase_depth(gl: &GenericGlContext, target: Target, programs: &Programs) -> Row {
    let (flat, textured) = match (&programs.flat, &programs.texture) {
        (Ok(f), Ok(t)) => (*f, *t),
        (Err(why), _) | (_, Err(why)) => return failed("depth", format!("a program did not build: {why}")),
    };
    if gl.glClearBufferfv.is_null() {
        return failed("depth", "glClearBufferfv is not loaded".to_owned());
    }
    let size = target.size;
    let texture = gl.gen_textures(1).first().copied().unwrap_or(0);
    gl.active_texture(glconst::TEXTURE0);
    bind_2d(gl, target, texture);
    gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MIN_FILTER, glconst::NEAREST as i32);
    gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MAG_FILTER, glconst::NEAREST as i32);
    gl.tex_storage_2d(glconst::TEXTURE_2D, 1, glconst::RGBA8, 128, 128);
    let depth = gl.gen_renderbuffers(1).first().copied().unwrap_or(0);
    gl.bind_renderbuffer(glconst::RENDERBUFFER, depth);
    gl.renderbuffer_storage(glconst::RENDERBUFFER, glconst::DEPTH_COMPONENT24, 128, 128);
    let framebuffer = gl.gen_framebuffers(1).first().copied().unwrap_or(0);
    gl.bind_framebuffer(draw_target(target), framebuffer);
    gl.framebuffer_texture_2d(
        draw_target(target),
        glconst::COLOR_ATTACHMENT0,
        glconst::TEXTURE_2D,
        texture,
        0,
    );
    gl.framebuffer_renderbuffer(
        draw_target(target),
        glconst::DEPTH_ATTACHMENT,
        glconst::RENDERBUFFER,
        depth,
    );
    let status = gl.check_frame_buffer_status(draw_target(target));
    gl.viewport(0, 0, 128, 128);
    gl.disable(glconst::SCISSOR_TEST);
    gl.disable(glconst::BLEND);
    gl.color_mask(true, true, true, true);
    gl.depth_mask(true);
    gl.clear_color(0.0, 0.0, 0.0, 0.0);
    gl.clear(glconst::COLOR_BUFFER_BIT);
    let one = 1.0_f32;
    // SAFETY: a non-null `glClearBufferfv` of the table is the function of that name: `void (GLenum buffer, GLint drawbuffer, const GLfloat *value)`.
    let clear_buffer: extern "system" fn(u32, i32, *const f32) = unsafe { std::mem::transmute(gl.glClearBufferfv) };
    clear_buffer(glconst::DEPTH, 0, &one);
    gl.enable(glconst::DEPTH_TEST);
    gl.depth_func(glconst::LEQUAL);
    // Near (clip z = -0.5, window depth 0.25): green. Far (clip z = +0.5, depth 0.75): red, which must lose.
    gl.use_program(flat);
    set_colour(gl, flat, [0.0, 1.0, 0.0, 1.0]);
    full_screen(gl, flat, -0.5);
    gl.use_program(flat);
    set_colour(gl, flat, [1.0, 0.0, 0.0, 1.0]);
    full_screen(gl, flat, 0.5);
    gl.finish();
    read_from(gl, target, framebuffer);
    let in_tile = px(gl, 64, 64);
    gl.disable(glconst::DEPTH_TEST);

    // Shown in the window.
    window_target(gl, target);
    gl.clear_color(0.2, 0.2, 0.2, 1.0);
    gl.clear(glconst::COLOR_BUFFER_BIT);
    gl.active_texture(glconst::TEXTURE0);
    bind_2d(gl, target, texture);
    gl.use_program(textured);
    let sampler = gl.get_uniform_location(textured, "sTex");
    gl.uniform_1i(sampler, 0);
    full_screen(gl, textured, 0.0);
    gl.finish();
    let in_window = px(gl, size.0 / 2, size.1 / 2);
    let error = gl.get_error();
    let note = format!("framebuffer status 0x{status:x}");
    let mut result = row(
        "depth",
        &[("tile centre", GREEN, in_tile), ("window centre", GREEN, in_window)],
        error,
        &note,
    );
    result.ok &= status == glconst::FRAMEBUFFER_COMPLETE;

    gl.bind_framebuffer(draw_target(target), target.window);
    read_from(gl, target, target.window);
    gl.bind_texture(glconst::TEXTURE_2D, 0);
    gl.delete_framebuffers(&[framebuffer]);
    gl.delete_renderbuffers(&[depth]);
    gl.delete_textures(&[texture]);
    result
}

/// 6b. WebRender's depth ids: it spends one depth-buffer step on a depth id (`1 << (24 - 2)` ids in a 24-bit buffer, the orthographic
/// projection with `near = -(1 << 22)` and `far = (1 << 22) - 1`), so a depth buffer that cannot tell two neighbouring ids apart draws the
/// wrong item. Two triangles at the depth ids 5 (green) and 6 (red, nearer: the projection gives a larger id a smaller depth) are drawn
/// farther first, nearer second, and the other way round, with `LEQUAL`: the nearer id must win in both orders.
fn phase_depth_fine(gl: &GenericGlContext, target: Target, programs: &Programs) -> Row {
    let flat = match &programs.flat {
        Ok(program) => *program,
        Err(why) => return failed("depth_fine", format!("the program did not build: {why}")),
    };
    if gl.glClearBufferfv.is_null() {
        return failed("depth_fine", "glClearBufferfv is not loaded".to_owned());
    }
    // The clip space z of a depth id under WebRender's projection.
    let ndc = |id: f32| (1.0 - 2.0 * id) / 8_388_607.0;
    let depth = gl.gen_renderbuffers(1).first().copied().unwrap_or(0);
    gl.bind_renderbuffer(glconst::RENDERBUFFER, depth);
    gl.renderbuffer_storage(glconst::RENDERBUFFER, glconst::DEPTH_COMPONENT24, 128, 128);
    let texture = gl.gen_textures(1).first().copied().unwrap_or(0);
    gl.active_texture(glconst::TEXTURE0);
    bind_2d(gl, target, texture);
    gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MIN_FILTER, glconst::NEAREST as i32);
    gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MAG_FILTER, glconst::NEAREST as i32);
    gl.tex_storage_2d(glconst::TEXTURE_2D, 1, glconst::RGBA8, 128, 128);
    let framebuffer = gl.gen_framebuffers(1).first().copied().unwrap_or(0);
    gl.bind_framebuffer(draw_target(target), framebuffer);
    gl.framebuffer_texture_2d(
        draw_target(target),
        glconst::COLOR_ATTACHMENT0,
        glconst::TEXTURE_2D,
        texture,
        0,
    );
    gl.framebuffer_renderbuffer(
        draw_target(target),
        glconst::DEPTH_ATTACHMENT,
        glconst::RENDERBUFFER,
        depth,
    );
    let status = gl.check_frame_buffer_status(draw_target(target));
    gl.viewport(0, 0, 128, 128);
    gl.disable(glconst::SCISSOR_TEST);
    gl.disable(glconst::BLEND);
    gl.color_mask(true, true, true, true);
    gl.depth_mask(true);
    let one = 1.0_f32;
    // SAFETY: a non-null `glClearBufferfv` of the table is the function of that name: `void (GLenum buffer, GLint drawbuffer, const GLfloat *value)`.
    let clear_buffer: extern "system" fn(u32, i32, *const f32) = unsafe { std::mem::transmute(gl.glClearBufferfv) };
    let mut checks: Vec<(&str, [u8; 4], [u8; 4])> = Vec::new();
    for (name, order) in [
        ("farther id first, nearer second", [(5.0_f32, [0.0_f32, 1.0, 0.0, 1.0]), (6.0, [1.0, 0.0, 0.0, 1.0])]),
        ("nearer id first, farther second", [(6.0, [1.0, 0.0, 0.0, 1.0]), (5.0, [0.0, 1.0, 0.0, 1.0])]),
    ] {
        gl.clear_color(0.0, 0.0, 0.0, 0.0);
        gl.clear(glconst::COLOR_BUFFER_BIT);
        clear_buffer(glconst::DEPTH, 0, &one);
        gl.enable(glconst::DEPTH_TEST);
        gl.depth_func(glconst::LEQUAL);
        for (id, colour) in order {
            gl.use_program(flat);
            set_colour(gl, flat, colour);
            full_screen(gl, flat, ndc(id));
        }
        gl.finish();
        read_from(gl, target, framebuffer);
        checks.push((name, RED, px(gl, 64, 64)));
        gl.disable(glconst::DEPTH_TEST);
    }
    let error = gl.get_error();
    let note = format!("framebuffer status 0x{status:x}");
    let mut result = row("depth_fine", &checks, error, &note);
    result.ok &= status == glconst::FRAMEBUFFER_COMPLETE;
    gl.bind_framebuffer(draw_target(target), target.window);
    read_from(gl, target, target.window);
    window_target(gl, target);
    gl.bind_texture(glconst::TEXTURE_2D, 0);
    gl.delete_framebuffers(&[framebuffer]);
    gl.delete_renderbuffers(&[depth]);
    gl.delete_textures(&[texture]);
    result
}

/// 7. WebRender's depth buffer the way a frame uses it: two tile textures with their own framebuffers share ONE 24-bit depth renderbuffer.
/// The first pass clears the depth with `glClearBufferfv`, the second with a quad (`DepthFunction::Always`, depth write on, colour write off,
/// depth 1.0: `ps_clear`); each draws a near and a far triangle with `LEQUAL` (the far one must lose) and then invalidates the depth
/// attachment, as `Device::invalidate_depth_target` does. Both tiles must hold the near colour afterwards.
fn phase_shared_depth(gl: &GenericGlContext, target: Target, programs: &Programs) -> Row {
    let flat = match &programs.flat {
        Ok(program) => *program,
        Err(why) => return failed("shared_depth", format!("the program did not build: {why}")),
    };
    if gl.glClearBufferfv.is_null() {
        return failed("shared_depth", "glClearBufferfv is not loaded".to_owned());
    }
    let depth = gl.gen_renderbuffers(1).first().copied().unwrap_or(0);
    gl.bind_renderbuffer(glconst::RENDERBUFFER, depth);
    gl.renderbuffer_storage(glconst::RENDERBUFFER, glconst::DEPTH_COMPONENT24, 128, 128);
    let textures = gl.gen_textures(2);
    let framebuffers = gl.gen_framebuffers(2);
    let mut checks: Vec<(&str, [u8; 4], [u8; 4])> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    for pass in 0..2_usize {
        let texture = textures.get(pass).copied().unwrap_or(0);
        let framebuffer = framebuffers.get(pass).copied().unwrap_or(0);
        gl.active_texture(glconst::TEXTURE0);
        bind_2d(gl, target, texture);
        gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MIN_FILTER, glconst::NEAREST as i32);
        gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MAG_FILTER, glconst::NEAREST as i32);
        gl.tex_storage_2d(glconst::TEXTURE_2D, 1, glconst::RGBA8, 128, 128);
        gl.bind_framebuffer(draw_target(target), framebuffer);
        gl.framebuffer_texture_2d(
            draw_target(target),
            glconst::COLOR_ATTACHMENT0,
            glconst::TEXTURE_2D,
            texture,
            0,
        );
        gl.framebuffer_renderbuffer(
            draw_target(target),
            glconst::DEPTH_ATTACHMENT,
            glconst::RENDERBUFFER,
            depth,
        );
        notes.push(format!("pass {pass} framebuffer status 0x{:x}", gl.check_frame_buffer_status(draw_target(target))));
        gl.viewport(0, 0, 128, 128);
        gl.disable(glconst::SCISSOR_TEST);
        gl.disable(glconst::BLEND);
        gl.color_mask(true, true, true, true);
        gl.depth_mask(true);
        gl.clear_color(0.0, 0.0, 0.0, 0.0);
        gl.clear(glconst::COLOR_BUFFER_BIT);
        if pass == 0 {
            let one = 1.0_f32;
            // SAFETY: a non-null `glClearBufferfv` of the table is the function of that name: `void (GLenum buffer, GLint drawbuffer, const GLfloat *value)`.
            let clear_buffer: extern "system" fn(u32, i32, *const f32) = unsafe { std::mem::transmute(gl.glClearBufferfv) };
            clear_buffer(glconst::DEPTH, 0, &one);
        } else {
            // The depth cleared by a quad at depth 1.0 that always passes, writes only depth.
            gl.enable(glconst::DEPTH_TEST);
            gl.depth_func(glconst::ALWAYS);
            gl.color_mask(false, false, false, false);
            gl.use_program(flat);
            set_colour(gl, flat, [0.0, 0.0, 0.0, 0.0]);
            full_screen(gl, flat, 1.0);
            gl.color_mask(true, true, true, true);
        }
        gl.enable(glconst::DEPTH_TEST);
        gl.depth_func(glconst::LEQUAL);
        gl.use_program(flat);
        set_colour(gl, flat, [0.0, 1.0, 0.0, 1.0]);
        full_screen(gl, flat, -0.5);
        gl.use_program(flat);
        set_colour(gl, flat, [1.0, 0.0, 0.0, 1.0]);
        full_screen(gl, flat, 0.5);
        gl.finish();
        read_from(gl, target, framebuffer);
        let name = if pass == 0 { "first tile (depth cleared by glClearBufferfv)" } else { "second tile (depth cleared by a quad)" };
        checks.push((name, GREEN, px(gl, 64, 64)));
        gl.disable(glconst::DEPTH_TEST);
        // After the pass the depth attachment is invalidated; the next pass starts from what the driver makes of that.
        gl.invalidate_framebuffer(draw_target(target), &[glconst::DEPTH_ATTACHMENT]);
    }
    let error = gl.get_error();
    gl.bind_framebuffer(draw_target(target), target.window);
    read_from(gl, target, target.window);
    window_target(gl, target);
    gl.use_program(0);
    gl.bind_texture(glconst::TEXTURE_2D, 0);
    gl.delete_framebuffers(&framebuffers);
    gl.delete_renderbuffers(&[depth]);
    gl.delete_textures(&textures);
    row("shared_depth", &checks, error, &notes.join("; "))
}

/// The phases, by name, in the order the ladder runs them.
pub const PHASES: [&str; 16] = [
    "clear",
    "shader",
    "dummy_texture",
    "fbo_sample",
    "instanced",
    "flat_lowp",
    "quad_u8norm",
    "big_texture",
    "texture_size",
    "data_textures",
    "pbo_upload",
    "vertex_fetch",
    "scatter",
    "depth",
    "depth_fine",
    "shared_depth",
];

/// What the ladder found: the rows of the first drawing of every phase, and the programs that did not build.
#[derive(Debug, Clone)]
pub struct Ladder {
    /// One row per phase, in order.
    pub rows: Vec<Row>,
    /// The programs that did not build, with the driver's message.
    pub program_problems: Vec<String>,
}

/// Runs the ladder in the framebuffer that stands for the window. After a phase has drawn and read back, `show` is called with its
/// name TWICE, the second time after the phase drew again (a swap leaves the back buffer undefined, so the phase is drawn once more
/// behind the first swap): the caller swaps and holds.
pub fn run_ladder(gl: &GenericGlContext, target: Target, show: &mut dyn FnMut(&'static str)) -> Ladder {
    let programs = Programs::build(gl);
    let mut rows = Vec::with_capacity(PHASES.len());
    let draw = |name: &'static str| -> Row {
        match name {
            "clear" => phase_clear(gl, target),
            "shader" => phase_shader(gl, target, &programs),
            "dummy_texture" => phase_dummy_texture(gl, target, &programs),
            "fbo_sample" => phase_fbo_sample(gl, target, &programs),
            "instanced" => phase_instanced(gl, target, &programs),
            "flat_lowp" => phase_flat_lowp(gl, target, &programs),
            "quad_u8norm" => phase_quad_u8norm(gl, target, &programs),
            "big_texture" => phase_big_texture(gl, target, &programs),
            "texture_size" => phase_texture_size(gl, target, &programs),
            "data_textures" => phase_data_textures(gl, target, &programs),
            "pbo_upload" => phase_pbo_upload(gl, target, &programs),
            "vertex_fetch" => phase_vertex_fetch(gl, target, &programs),
            "scatter" => phase_scatter(gl, target, &programs),
            "depth" => phase_depth(gl, target, &programs),
            "depth_fine" => phase_depth_fine(gl, target, &programs),
            _ => phase_shared_depth(gl, target, &programs),
        }
    };
    for name in PHASES {
        rows.push(draw(name));
        show(name);
        let _ = draw(name);
        show(name);
    }
    let program_problems = programs.problems();
    programs.delete(gl);
    window_target(gl, target);
    Ladder {
        rows,
        program_problems,
    }
}

#[cfg(test)]
#[path = "gles_control_test.rs"]
mod tests;
