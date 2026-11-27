//! What the GL context and the pixels of a frame say at the points where a window that stays black has to be told apart from one
//! that draws: reads of the context's state and of a few pixels, and one small self-test of the driver that the environment turns
//! on (`AZ_GPU_SELFTEST=1`). Nothing here is part of a frame.
//!
//! The self-test asks the driver, before WebRender is made, for the three things WebRender needs of it: that a clear of the window's
//! framebuffer reads back, that a texture can be a framebuffer's colour and be cleared and read, and that a draw with a shader
//! (a triangle from `gl_VertexID`, no buffers) lands in the window's framebuffer. Each answer is one line.

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
