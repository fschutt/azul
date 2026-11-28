//! What the GL context and the pixels of a frame say at the points where a window that stays black has to be told apart from one
//! that draws: reads of the context's state and of a few pixels. Nothing here is part of a frame.

use azul_core::glconst;
use gl_context_loader::GenericGlContext;

/// The pixel at `(x, y)` of the framebuffer bound for reading, as `#rrggbbaa`.
#[must_use]
pub fn read_pixel(gl: &GenericGlContext, x: u32, y: u32) -> String {
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
    format!("#{:02x}{:02x}{:02x}{:02x}", rgba[0], rgba[1], rgba[2], rgba[3])
}

/// Five pixels of the window's framebuffer (the centre and the four quarter points), as `#rrggbbaa` each: what the frame left in the
/// back buffer before the swap. Finishes the pending commands first, and leaves the default framebuffer bound.
#[must_use]
pub fn sample_pixels(gl: &GenericGlContext, size: (u32, u32)) -> String {
    gl.bind_buffer(glconst::PIXEL_PACK_BUFFER, 0);
    gl.bind_framebuffer(glconst::FRAMEBUFFER, 0);
    gl.finish();
    let (w, h) = (size.0.max(4), size.1.max(4));
    format!(
        "centre {} quarters {} {} {} {}",
        read_pixel(gl, w / 2, h / 2),
        read_pixel(gl, w / 4, h / 4),
        read_pixel(gl, 3 * w / 4, h / 4),
        read_pixel(gl, w / 4, 3 * h / 4),
        read_pixel(gl, 3 * w / 4, 3 * h / 4)
    )
}

/// The state of the context that decides where a frame lands: the framebuffers bound for drawing and reading, the buffer pixels are
/// read into, the viewport, the scissor test, the colour mask, and whether the bound framebuffer is complete.
#[must_use]
pub fn state_line(gl: &GenericGlContext) -> String {
    let ints = |name: u32, n: usize| {
        let mut values = vec![0_i32; n];
        // SAFETY: a query that writes `n` integers into a slice of `n`.
        unsafe { gl.get_integer_v(name, &mut values) };
        values
    };
    let first = |name: u32| ints(name, 1).first().copied().unwrap_or(0);
    let mut mask = [0; 4];
    // SAFETY: a query that writes four booleans into a slice of four.
    unsafe { gl.get_boolean_v(glconst::COLOR_WRITEMASK, &mut mask) };
    format!(
        "draw framebuffer {}, read framebuffer {}, pixel pack buffer {}, viewport {:?}, scissor test {}, colour mask {:?}, framebuffer status 0x{:x}",
        first(glconst::DRAW_FRAMEBUFFER_BINDING),
        first(glconst::READ_FRAMEBUFFER_BINDING),
        first(glconst::PIXEL_PACK_BUFFER_BINDING),
        ints(glconst::VIEWPORT, 4),
        gl.is_enabled(glconst::SCISSOR_TEST) != 0,
        mask,
        gl.check_frame_buffer_status(glconst::FRAMEBUFFER)
    )
}

/// The extensions of the context that WebRender asks about, as present (`+`) or missing (`-`), and how many the driver lists: what the
/// device layer chose its formats, its upload paths and its text rendering from.
#[must_use]
pub fn extensions_line(gl: &GenericGlContext) -> String {
    const ASKED: [&str; 12] = [
        "GL_EXT_color_buffer_float",
        "GL_EXT_color_buffer_half_float",
        "GL_EXT_texture_format_BGRA8888",
        "GL_EXT_blend_func_extended",
        "GL_EXT_buffer_storage",
        "GL_EXT_texture_storage",
        "GL_EXT_copy_image",
        "GL_OES_EGL_image_external_essl3",
        "GL_KHR_blend_equation_advanced",
        "GL_KHR_debug",
        "GL_EXT_disjoint_timer_query",
        "GL_QCOM_tiled_rendering",
    ];
    let mut count = [0_i32];
    // SAFETY: a query that writes one integer into a slice of one.
    unsafe { gl.get_integer_v(glconst::NUM_EXTENSIONS, &mut count) };
    let count = u32::try_from(count[0].max(0)).unwrap_or(0);
    let names: Vec<String> = (0..count)
        .map(|i| super::gles_probe::clean_driver_string(&gl.get_string_i(glconst::EXTENSIONS, i)))
        .collect();
    let marks: Vec<String> = ASKED
        .iter()
        .map(|name| {
            let present = names.iter().any(|n| n == name);
            format!("{}{}", if present { '+' } else { '-' }, name.trim_start_matches("GL_"))
        })
        .collect();
    format!("{} extensions; {}", names.len(), marks.join(" "))
}

/// The three checks of the driver, one line each (see the module's note). Leaves the window's framebuffer bound, holding what the last
/// check drew; the caller clears it.
#[must_use]
pub fn selftest(gl: &GenericGlContext, size: (u32, u32)) -> Vec<String> {
    let mut lines = Vec::with_capacity(3);

    // 1. the window's framebuffer: what a clear leaves reads back
    gl.bind_buffer(glconst::PIXEL_PACK_BUFFER, 0);
    gl.bind_framebuffer(glconst::FRAMEBUFFER, 0);
    gl.clear_color(1.0, 0.0, 1.0, 1.0);
    gl.clear(glconst::COLOR_BUFFER_BIT);
    gl.finish();
    lines.push(format!(
        "window framebuffer: cleared to #ff00ffff reads {} (glGetError 0x{:x})",
        read_pixel(gl, size.0 / 2, size.1 / 2),
        gl.get_error()
    ));

    // 2. a texture as the colour of a framebuffer
    lines.push(texture_framebuffer(gl));

    // 3. a draw with a shader into the window's framebuffer
    lines.push(draw_triangle(gl, size));

    // 4. what WebRender's data paths ask: a float texture and an integer texture fetched in a shader, an instanced draw with an integer
    // attribute, and the depth test of a framebuffer with a depth renderbuffer
    lines.push(fetch_float_texture(gl, size));
    lines.push(fetch_integer_texture(gl, size));
    lines.push(instanced_integer_attribute(gl, size));
    lines.push(depth_test(gl, true));
    lines.push(depth_test(gl, false));
    gl.bind_framebuffer(glconst::FRAMEBUFFER, 0);
    lines
}

fn texture_framebuffer(gl: &GenericGlContext) -> String {
    let texture = gl.gen_textures(1).first().copied().unwrap_or(0);
    gl.bind_texture(glconst::TEXTURE_2D, texture);
    gl.tex_storage_2d(glconst::TEXTURE_2D, 1, glconst::RGBA8, 64, 64);
    let framebuffer = gl.gen_framebuffers(1).first().copied().unwrap_or(0);
    gl.bind_framebuffer(glconst::FRAMEBUFFER, framebuffer);
    gl.framebuffer_texture_2d(
        glconst::FRAMEBUFFER,
        glconst::COLOR_ATTACHMENT0,
        glconst::TEXTURE_2D,
        texture,
        0,
    );
    let status = gl.check_frame_buffer_status(glconst::FRAMEBUFFER);
    gl.clear_color(0.0, 1.0, 0.0, 1.0);
    gl.clear(glconst::COLOR_BUFFER_BIT);
    let pixel = read_pixel(gl, 32, 32);
    let error = gl.get_error();
    gl.bind_framebuffer(glconst::FRAMEBUFFER, 0);
    gl.bind_texture(glconst::TEXTURE_2D, 0);
    gl.delete_framebuffers(&[framebuffer]);
    gl.delete_textures(&[texture]);
    format!("texture framebuffer: status 0x{status:x}, cleared to #00ff00ff reads {pixel} (glGetError 0x{error:x})")
}

/// A full-screen triangle made of `gl_VertexID` (no buffers), so the draw depends on nothing but the program and the framebuffer.
const TRIANGLE_VERTEX: &str = "#version 300 es\nvoid main() {\n    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));\n    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);\n}\n";
const TRIANGLE_FRAGMENT: &str = "#version 300 es\nprecision mediump float;\nout vec4 colour;\nvoid main() {\n    colour = vec4(0.5, 0.0, 1.0, 1.0);\n}\n";

/// One shader: its source as it is, and when the driver refuses that, with the NUL some emulators' GLES wants at the end of a source
/// (the device layer does the same for them).
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

fn build_program(gl: &GenericGlContext) -> Result<u32, String> {
    let vertex = compile(gl, glconst::VERTEX_SHADER, TRIANGLE_VERTEX)?;
    let fragment = match compile(gl, glconst::FRAGMENT_SHADER, TRIANGLE_FRAGMENT) {
        Ok(fragment) => fragment,
        Err(e) => {
            gl.delete_shader(vertex);
            return Err(e);
        }
    };
    let program = gl.create_program();
    gl.attach_shader(program, vertex);
    gl.attach_shader(program, fragment);
    gl.link_program(program);
    gl.delete_shader(vertex);
    gl.delete_shader(fragment);
    let mut status = [0_i32];
    // SAFETY: a query that writes one integer into a slice of one.
    unsafe { gl.get_program_iv(program, glconst::LINK_STATUS, &mut status) };
    if status[0] == 0 {
        let log = super::gles_probe::clean_driver_string(&gl.get_program_info_log(program));
        gl.delete_program(program);
        return Err(log);
    }
    Ok(program)
}

fn draw_triangle(gl: &GenericGlContext, size: (u32, u32)) -> String {
    let program = match build_program(gl) {
        Ok(program) => program,
        Err(e) => return format!("draw: the program did not build: {e}"),
    };
    gl.bind_framebuffer(glconst::FRAMEBUFFER, 0);
    gl.viewport(0, 0, i32::try_from(size.0).unwrap_or(1), i32::try_from(size.1).unwrap_or(1));
    let vao = gl.gen_vertex_arrays(1).first().copied().unwrap_or(0);
    gl.bind_vertex_array(vao);
    gl.use_program(program);
    gl.draw_arrays(glconst::TRIANGLES, 0, 3);
    gl.finish();
    let pixel = read_pixel(gl, size.0 / 2, size.1 / 2);
    let error = gl.get_error();
    gl.use_program(0);
    gl.bind_vertex_array(0);
    gl.delete_vertex_arrays(&[vao]);
    gl.delete_program(program);
    format!("draw: a full-screen triangle of #8000ffff (or #7f00ffff) reads {pixel} (glGetError 0x{error:x})")
}

/// A program from two sources, or why it did not build.
fn program_of(gl: &GenericGlContext, vertex: &str, fragment: &str) -> Result<u32, String> {
    let vertex = compile(gl, glconst::VERTEX_SHADER, vertex)?;
    let fragment = match compile(gl, glconst::FRAGMENT_SHADER, fragment) {
        Ok(fragment) => fragment,
        Err(e) => {
            gl.delete_shader(vertex);
            return Err(e);
        }
    };
    let program = gl.create_program();
    gl.attach_shader(program, vertex);
    gl.attach_shader(program, fragment);
    gl.link_program(program);
    gl.delete_shader(vertex);
    gl.delete_shader(fragment);
    let mut status = [0_i32];
    // SAFETY: a query that writes one integer into a slice of one.
    unsafe { gl.get_program_iv(program, glconst::LINK_STATUS, &mut status) };
    if status[0] == 0 {
        let log = super::gles_probe::clean_driver_string(&gl.get_program_info_log(program));
        gl.delete_program(program);
        return Err(log);
    }
    Ok(program)
}

const FULL_SCREEN_VERTEX: &str = "#version 300 es\nvoid main() {\n    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));\n    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);\n}\n";

/// Clears the window's framebuffer to transparent black, runs `draw`, reads the centre pixel and cleans up.
fn draw_and_read(gl: &GenericGlContext, size: (u32, u32), program: u32, draw: impl FnOnce()) -> String {
    gl.bind_framebuffer(glconst::FRAMEBUFFER, 0);
    gl.viewport(0, 0, i32::try_from(size.0).unwrap_or(1), i32::try_from(size.1).unwrap_or(1));
    gl.clear_color(0.0, 0.0, 0.0, 0.0);
    gl.clear(glconst::COLOR_BUFFER_BIT);
    let vao = gl.gen_vertex_arrays(1).first().copied().unwrap_or(0);
    gl.bind_vertex_array(vao);
    gl.use_program(program);
    draw();
    gl.finish();
    let pixel = read_pixel(gl, size.0 / 2, size.1 / 2);
    let error = gl.get_error();
    gl.use_program(0);
    gl.bind_vertex_array(0);
    gl.delete_vertex_arrays(&[vao]);
    gl.delete_program(program);
    format!("{pixel} (glGetError 0x{error:x})")
}

fn fetch_float_texture(gl: &GenericGlContext, size: (u32, u32)) -> String {
    let fragment = "#version 300 es\nprecision highp float;\nuniform highp sampler2D sData;\nout vec4 colour;\nvoid main() {\n    colour = texelFetch(sData, ivec2(1, 0), 0);\n}\n";
    let program = match program_of(gl, FULL_SCREEN_VERTEX, fragment) {
        Ok(program) => program,
        Err(e) => return format!("float texture fetch: the program did not build: {e}"),
    };
    let texture = gl.gen_textures(1).first().copied().unwrap_or(0);
    gl.active_texture(glconst::TEXTURE0);
    gl.bind_texture(glconst::TEXTURE_2D, texture);
    gl.tex_storage_2d(glconst::TEXTURE_2D, 1, glconst::RGBA32F, 2, 2);
    gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MIN_FILTER, glconst::NEAREST as i32);
    gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MAG_FILTER, glconst::NEAREST as i32);
    let texels: [f32; 16] = [0.0, 0.0, 0.0, 0.0, 0.25, 0.5, 0.75, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    let bytes: Vec<u8> = texels.iter().flat_map(|value| value.to_ne_bytes()).collect();
    gl.tex_sub_image_2d(glconst::TEXTURE_2D, 0, 0, 0, 2, 2, glconst::RGBA, glconst::FLOAT, &bytes);
    let location = gl.get_uniform_location(program, "sData");
    gl.use_program(program);
    gl.uniform_1i(location, 0);
    let result = draw_and_read(gl, size, program, || gl.draw_arrays(glconst::TRIANGLES, 0, 3));
    gl.bind_texture(glconst::TEXTURE_2D, 0);
    gl.delete_textures(&[texture]);
    format!("float texture fetch: an RGBA32F texel of (0.25, 0.5, 0.75, 1) reads {result}, expect about #4080bfff")
}

fn fetch_integer_texture(gl: &GenericGlContext, size: (u32, u32)) -> String {
    let fragment = "#version 300 es\nprecision highp float;\nprecision highp int;\nuniform highp isampler2D sData;\nout vec4 colour;\nvoid main() {\n    ivec4 v = texelFetch(sData, ivec2(1, 0), 0);\n    colour = vec4(v) / 255.0;\n}\n";
    let program = match program_of(gl, FULL_SCREEN_VERTEX, fragment) {
        Ok(program) => program,
        Err(e) => return format!("integer texture fetch: the program did not build: {e}"),
    };
    let texture = gl.gen_textures(1).first().copied().unwrap_or(0);
    gl.active_texture(glconst::TEXTURE0);
    gl.bind_texture(glconst::TEXTURE_2D, texture);
    gl.tex_storage_2d(glconst::TEXTURE_2D, 1, glconst::RGBA32I, 2, 2);
    gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MIN_FILTER, glconst::NEAREST as i32);
    gl.tex_parameter_i(glconst::TEXTURE_2D, glconst::TEXTURE_MAG_FILTER, glconst::NEAREST as i32);
    let texels: [i32; 16] = [0, 0, 0, 0, 64, 128, 191, 255, 0, 0, 0, 0, 0, 0, 0, 0];
    let bytes: Vec<u8> = texels.iter().flat_map(|value| value.to_ne_bytes()).collect();
    gl.tex_sub_image_2d(glconst::TEXTURE_2D, 0, 0, 0, 2, 2, glconst::RGBA_INTEGER, glconst::INT, &bytes);
    let location = gl.get_uniform_location(program, "sData");
    gl.use_program(program);
    gl.uniform_1i(location, 0);
    let result = draw_and_read(gl, size, program, || gl.draw_arrays(glconst::TRIANGLES, 0, 3));
    gl.bind_texture(glconst::TEXTURE_2D, 0);
    gl.delete_textures(&[texture]);
    format!("integer texture fetch: an RGBA32I texel of (64, 128, 191, 255) reads {result}, expect #4080bfff")
}

fn instanced_integer_attribute(gl: &GenericGlContext, size: (u32, u32)) -> String {
    let vertex = "#version 300 es\nlayout(location = 0) in ivec4 aData;\nflat out vec4 vColour;\nvoid main() {\n    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));\n    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);\n    vColour = vec4(aData) / 255.0;\n}\n";
    let fragment = "#version 300 es\nprecision mediump float;\nflat in vec4 vColour;\nout vec4 colour;\nvoid main() {\n    colour = vColour;\n}\n";
    let program = match program_of(gl, vertex, fragment) {
        Ok(program) => program,
        Err(e) => return format!("instanced integer attribute: the program did not build: {e}"),
    };
    let buffer = gl.gen_buffers(1).first().copied().unwrap_or(0);
    let values: [i32; 4] = [64, 128, 191, 255];
    let bytes: Vec<u8> = values.iter().flat_map(|value| value.to_ne_bytes()).collect();
    let result = draw_and_read(gl, size, program, || {
        gl.bind_buffer(glconst::ARRAY_BUFFER, buffer);
        gl.buffer_data_untyped(
            glconst::ARRAY_BUFFER,
            isize::try_from(bytes.len()).unwrap_or(16),
            bytes.as_ptr().cast(),
            glconst::STATIC_DRAW,
        );
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_i_pointer(0, 4, glconst::INT, 0, 0);
        gl.vertex_attrib_divisor(0, 1);
        gl.draw_arrays_instanced(glconst::TRIANGLES, 0, 3, 1);
    });
    gl.bind_buffer(glconst::ARRAY_BUFFER, 0);
    gl.delete_buffers(&[buffer]);
    format!("instanced integer attribute: an ivec4 per instance of (64, 128, 191, 255) reads {result}, expect #4080bfff")
}

/// The depth test of a framebuffer with a colour texture and a 24-bit depth renderbuffer, the clear of the depth with `glClearBufferfv`
/// (the way the vendored device clears an ES context) or with `glClear`: a triangle at depth 0.5 must pass `LEQUAL` against a depth cleared
/// to 1.0.
fn depth_test(gl: &GenericGlContext, with_clear_buffer: bool) -> String {
    let fragment = "#version 300 es\nprecision mediump float;\nout vec4 colour;\nvoid main() {\n    colour = vec4(0.5, 0.0, 1.0, 1.0);\n}\n";
    let program = match program_of(gl, FULL_SCREEN_VERTEX, fragment) {
        Ok(program) => program,
        Err(e) => return format!("depth test: the program did not build: {e}"),
    };
    let texture = gl.gen_textures(1).first().copied().unwrap_or(0);
    gl.bind_texture(glconst::TEXTURE_2D, texture);
    gl.tex_storage_2d(glconst::TEXTURE_2D, 1, glconst::RGBA8, 64, 64);
    let depth = gl.gen_renderbuffers(1).first().copied().unwrap_or(0);
    gl.bind_renderbuffer(glconst::RENDERBUFFER, depth);
    gl.renderbuffer_storage(glconst::RENDERBUFFER, glconst::DEPTH_COMPONENT24, 64, 64);
    let framebuffer = gl.gen_framebuffers(1).first().copied().unwrap_or(0);
    gl.bind_framebuffer(glconst::FRAMEBUFFER, framebuffer);
    gl.framebuffer_texture_2d(glconst::FRAMEBUFFER, glconst::COLOR_ATTACHMENT0, glconst::TEXTURE_2D, texture, 0);
    gl.framebuffer_renderbuffer(glconst::FRAMEBUFFER, glconst::DEPTH_ATTACHMENT, glconst::RENDERBUFFER, depth);
    let status = gl.check_frame_buffer_status(glconst::FRAMEBUFFER);
    gl.viewport(0, 0, 64, 64);
    gl.clear_color(0.0, 0.0, 0.0, 0.0);
    gl.depth_mask(true);
    if with_clear_buffer {
        gl.clear(glconst::COLOR_BUFFER_BIT);
        let value = 1.0_f32;
        if gl.glClearBufferfv.is_null() {
            return "depth test: glClearBufferfv is not loaded".to_owned();
        }
        // SAFETY: a non-null `glClearBufferfv` of the table is the function of that name: `void (GLenum buffer, GLint drawbuffer, const GLfloat *value)`.
        let clear: extern "system" fn(u32, i32, *const f32) = unsafe { std::mem::transmute(gl.glClearBufferfv) };
        clear(glconst::DEPTH, 0, &value);
    } else {
        gl.clear(glconst::COLOR_BUFFER_BIT | glconst::DEPTH_BUFFER_BIT);
    }
    gl.enable(glconst::DEPTH_TEST);
    gl.depth_func(glconst::LEQUAL);
    let vao = gl.gen_vertex_arrays(1).first().copied().unwrap_or(0);
    gl.bind_vertex_array(vao);
    gl.use_program(program);
    gl.draw_arrays(glconst::TRIANGLES, 0, 3);
    gl.finish();
    let pixel = read_pixel(gl, 32, 32);
    let error = gl.get_error();
    gl.disable(glconst::DEPTH_TEST);
    gl.use_program(0);
    gl.bind_vertex_array(0);
    gl.delete_vertex_arrays(&[vao]);
    gl.delete_program(program);
    gl.bind_framebuffer(glconst::FRAMEBUFFER, 0);
    gl.delete_framebuffers(&[framebuffer]);
    gl.delete_renderbuffers(&[depth]);
    gl.bind_texture(glconst::TEXTURE_2D, 0);
    gl.delete_textures(&[texture]);
    format!(
        "depth test (depth cleared with {}): framebuffer status 0x{status:x}, a triangle at depth 0.5 with LEQUAL reads {pixel} (glGetError 0x{error:x}), expect #8000ffff",
        if with_clear_buffer { "glClearBufferfv" } else { "glClear" }
    )
}
