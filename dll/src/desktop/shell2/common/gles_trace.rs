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

/// EXPERIMENT (the Android GPU root-cause hunt): the GL functions the vendored device layer and the engine call through the function table,
/// by their names in the table. The engine logs where each came from (`GlesFunctions::used_sources`): a name no source had is an empty slot,
/// and the wrapper skips the call without a word.
pub const DEVICE_USED_NAMES: &[&str] = &[
    "glActiveTexture", "glAttachShader", "glBeginQuery", "glBindAttribLocation", "glBindBuffer", "glBindFramebuffer",
    "glBindRenderbuffer", "glBindTexture", "glBindVertexArray", "glBlendBarrierKHR", "glBlendEquation",
    "glBlendEquationSeparate", "glBlendFunc", "glBlendFuncSeparate", "glBlitFramebuffer", "glBufferData",
    "glBufferStorage", "glBufferSubData", "glCheckFramebufferStatus", "glClear", "glClearBufferfv", "glClearColor",
    "glClearDepth", "glClientWaitSync", "glColorMask", "glCompileShader", "glCopyImageSubData", "glCreateProgram",
    "glCreateShader", "glDebugMessageInsertKHR", "glDeleteBuffers", "glDeleteFramebuffers", "glDeleteProgram",
    "glDeleteQueries", "glDeleteRenderbuffers", "glDeleteShader", "glDeleteSync", "glDeleteTextures",
    "glDeleteVertexArrays", "glDepthFunc", "glDepthMask", "glDetachShader", "glDisable",
    "glDisableVertexAttribArray", "glDrawArrays", "glDrawElements", "glDrawElementsInstanced", "glEnable",
    "glEnableVertexAttribArray", "glEndQuery", "glEndTilingQCOM", "glFenceSync", "glFlush",
    "glFlushMappedBufferRange", "glFramebufferRenderbuffer", "glFramebufferTexture2D", "glGenBuffers",
    "glGenFramebuffers", "glGenQueries", "glGenRenderbuffers", "glGenTextures", "glGenVertexArrays",
    "glGenerateMipmap", "glGetBooleanv", "glGetDebugMessageLog", "glGetError", "glGetIntegerv", "glGetProgramBinary",
    "glGetProgramInfoLog", "glGetProgramiv", "glGetQueryObjectui64v", "glGetShaderInfoLog", "glGetShaderiv",
    "glGetString", "glGetStringi", "glGetTexImage", "glGetUniformLocation", "glInsertEventMarkerEXT",
    "glInvalidateFramebuffer", "glLinkProgram", "glMapBuffer", "glMapBufferRange", "glPixelStorei",
    "glPopDebugGroupKHR", "glPopGroupMarkerEXT", "glProgramBinary", "glProgramParameteri",
    "glPushDebugGroupKHR", "glPushGroupMarkerEXT", "glReadPixels", "glRenderbufferStorage", "glScissor",
    "glShaderSource", "glStartTilingQCOM", "glTexImage2D", "glTexParameteri", "glTexStorage2D", "glTexSubImage2D",
    "glUniform1f", "glUniform1i", "glUniform2f", "glUniform4f", "glUniformMatrix4fv", "glUnmapBuffer",
    "glUseProgram", "glVertexAttribDivisor", "glVertexAttribIPointer", "glVertexAttribPointer", "glViewport",
];

/// EXPERIMENT (the Android GPU root-cause hunt): RGBA pixels shrunk by the integer factor `factor` (a box average per channel), as
/// `(width, height, pixels)`.
#[must_use]
pub fn shrink_rgba(width: usize, height: usize, pixels: &[u8], factor: usize) -> (usize, usize, Vec<u8>) {
    let factor = factor.max(1);
    let (w, h) = (width / factor, height / factor);
    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0_u32; 4];
            for dy in 0..factor {
                for dx in 0..factor {
                    let at = ((y * factor + dy) * width + (x * factor + dx)) * 4;
                    for (c, total) in sum.iter_mut().enumerate() {
                        *total += u32::from(pixels.get(at + c).copied().unwrap_or(0));
                    }
                }
            }
            let n = u32::try_from(factor * factor).unwrap_or(1);
            for total in sum {
                out.push(u8::try_from(total / n).unwrap_or(255));
            }
        }
    }
    (w, h, out)
}

/// Standard base64 of `bytes`.
#[must_use]
pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(chunk.get(1).copied().unwrap_or(0)) << 8)
            | u32::from(chunk.get(2).copied().unwrap_or(0));
        out.push(char::from(ALPHABET[((n >> 18) & 63) as usize]));
        out.push(char::from(ALPHABET[((n >> 12) & 63) as usize]));
        out.push(if chunk.len() > 1 { char::from(ALPHABET[((n >> 6) & 63) as usize]) } else { '=' });
        out.push(if chunk.len() > 2 { char::from(ALPHABET[(n & 63) as usize]) } else { '=' });
    }
    out
}

/// The lines that carry a picture through the log: `pic <label> <width>x<height> <part>/<parts> <base64 of the RGBA bytes>`, 3000
/// characters of base64 a line. The analysis script of the probe run puts them together again.
#[must_use]
pub fn picture_lines(label: &str, width: usize, height: usize, pixels: &[u8]) -> Vec<String> {
    let encoded = base64(pixels);
    let parts = encoded.len().div_ceil(3000).max(1);
    (0..parts)
        .map(|i| {
            let from = i * 3000;
            let to = (from + 3000).min(encoded.len());
            format!("pic {label} {width}x{height} {}/{parts} {}", i + 1, &encoded[from..to])
        })
        .collect()
}

/// The pixels of the framebuffer bound for reading, `width` x `height`, bottom row first, as RGBA.
#[must_use]
pub fn read_framebuffer(gl: &GenericGlContext, width: u32, height: u32) -> Vec<u8> {
    let mut pixels = vec![0_u8; width as usize * height as usize * 4];
    gl.read_pixels_into_buffer(
        0,
        0,
        i32::try_from(width).unwrap_or(0),
        i32::try_from(height).unwrap_or(0),
        glconst::RGBA,
        glconst::UNSIGNED_BYTE,
        &mut pixels,
    );
    pixels
}

/// EXPERIMENT (the Android GPU root-cause hunt): the implementation limits WebRender's shaders live on (vertex attributes, varyings, uniforms,
/// texture units per stage, render target sizes), as one line.
#[must_use]
pub fn limits_line(gl: &GenericGlContext) -> String {
    const LIMITS: [(&str, u32); 14] = [
        ("MAX_VERTEX_ATTRIBS", glconst::MAX_VERTEX_ATTRIBS),
        ("MAX_VERTEX_UNIFORM_VECTORS", glconst::MAX_VERTEX_UNIFORM_VECTORS),
        ("MAX_FRAGMENT_UNIFORM_VECTORS", glconst::MAX_FRAGMENT_UNIFORM_VECTORS),
        ("MAX_VARYING_VECTORS", glconst::MAX_VARYING_VECTORS),
        ("MAX_VERTEX_OUTPUT_COMPONENTS", glconst::MAX_VERTEX_OUTPUT_COMPONENTS),
        ("MAX_FRAGMENT_INPUT_COMPONENTS", glconst::MAX_FRAGMENT_INPUT_COMPONENTS),
        ("MAX_VERTEX_TEXTURE_IMAGE_UNITS", glconst::MAX_VERTEX_TEXTURE_IMAGE_UNITS),
        ("MAX_TEXTURE_IMAGE_UNITS", glconst::MAX_TEXTURE_IMAGE_UNITS),
        ("MAX_COMBINED_TEXTURE_IMAGE_UNITS", glconst::MAX_COMBINED_TEXTURE_IMAGE_UNITS),
        ("MAX_RENDERBUFFER_SIZE", glconst::MAX_RENDERBUFFER_SIZE),
        ("MAX_COLOR_ATTACHMENTS", glconst::MAX_COLOR_ATTACHMENTS),
        ("MAX_DRAW_BUFFERS", glconst::MAX_DRAW_BUFFERS),
        ("MAX_ARRAY_TEXTURE_LAYERS", glconst::MAX_ARRAY_TEXTURE_LAYERS),
        ("MAX_ELEMENT_INDEX", glconst::MAX_ELEMENT_INDEX),
    ];
    let mut parts = Vec::with_capacity(LIMITS.len() + 1);
    for (name, constant) in LIMITS {
        let mut value = [0_i32];
        // SAFETY: a query that writes one integer into a slice of one.
        unsafe { gl.get_integer_v(constant, &mut value) };
        parts.push(format!("{name} {}", value[0]));
    }
    let mut point = [0_f32; 2];
    // SAFETY: `glGetFloatv` of the ALIASED_POINT_SIZE_RANGE writes two floats; the table's slot is called only when it has one.
    if !gl.glGetFloatv.is_null() {
        let get_floats: extern "system" fn(u32, *mut f32) = unsafe { std::mem::transmute(gl.glGetFloatv) };
        get_floats(glconst::ALIASED_POINT_SIZE_RANGE, point.as_mut_ptr());
    }
    parts.push(format!("ALIASED_POINT_SIZE_RANGE {point:?}"));
    parts.join(", ")
}
