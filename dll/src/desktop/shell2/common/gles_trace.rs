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
