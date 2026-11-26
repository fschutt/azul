//! What an OpenGL ES context says about itself, read as a decision: which version it is, whether it is a software or an
//! emulated one, and whether it meets what WebRender needs. Pure and `std` only, so a host test checks every string a real
//! driver is known to answer without a GPU.
//!
//! WebRender's device layer (the vendored `webrender/core/src/device/gl.rs`, Mozilla's, with the workarounds of the Android
//! GPUs it ships on) asks for OpenGL ES 3.0 (`glGetStringi`, vertex arrays, instancing, `glTexStorage`, `glMapBufferRange`,
//! `glBlitFramebuffer`, `glInvalidateFramebuffer`, program binaries) and a `MAX_TEXTURE_SIZE` of 2048
//! (`RendererError::MaxTextureSize` below it). Everything else it asks for degrades: no `GL_EXT_color_buffer_float` takes the
//! pixel-buffer cache update, no `GL_EXT_blend_func_extended` draws grey-scale text instead of sub-pixel text.

/// The smallest `MAX_TEXTURE_SIZE` WebRender's texture cache works with (`create_webrender_instance`).
pub const MIN_TEXTURE_SIZE: i32 = 2048;

/// An OpenGL ES version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct GlesVersion {
    /// The major number: 2 or 3.
    pub major: u32,
    /// The minor number.
    pub minor: u32,
}

/// The version `GL_VERSION` names, or `None` when the string is not an OpenGL ES one.
///
/// ES 2.0 and later answer `OpenGL ES <major>.<minor> <vendor information>` (`OpenGL ES 3.2 V@0502.0`, `OpenGL ES 3.0 Mesa
/// 23.2.1`); ES 1.x answers `OpenGL ES-CM 1.1` or `OpenGL ES-CL 1.1`; a desktop context answers `4.6.0 NVIDIA 535`.
#[must_use]
pub fn parse_gles_version(version: &str) -> Option<GlesVersion> {
    let rest = version.trim().strip_prefix("OpenGL ES")?;
    // `-CM` / `-CL` (the ES 1 profiles) follow the name without a space.
    let rest = rest.trim_start_matches(|c: char| c == '-' || c.is_ascii_uppercase());
    let number = rest.split_whitespace().next()?;
    let (major, minor) = number.split_once('.')?;
    Some(GlesVersion {
        major: major.parse().ok()?,
        minor: minor
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .ok()?,
    })
}

/// What kind of renderer `GL_RENDERER` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RendererClass {
    /// The pixels are made by the CPU: SwiftShader, llvmpipe, softpipe, a "software" rasteriser.
    pub software: bool,
    /// The renderer is the Android emulator's translator (`Android Emulator OpenGL ES Translator (...)`): what the
    /// emulator's `-gpu swiftshader_indirect` and `-gpu host` modes answer, and what WebRender's device layer applies its
    /// emulator workarounds to (it tests the prefix).
    pub emulated: bool,
}

/// Classifies `GL_RENDERER`; case does not matter.
#[must_use]
pub fn classify_renderer(renderer: &str) -> RendererClass {
    let lower = renderer.to_lowercase();
    let has = |needle: &str| lower.contains(needle);
    RendererClass {
        software: has("swiftshader")
            || has("llvmpipe")
            || has("softpipe")
            || has("swrast")
            || has("software rasterizer")
            || has("software renderer"),
        emulated: lower.starts_with("android emulator") || has("goldfish") || has("ranchu"),
    }
}

/// What the probe found out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Probe {
    /// The OpenGL ES version of the context.
    pub version: GlesVersion,
    /// The kind of renderer behind it.
    pub class: RendererClass,
}

/// Checks the context against WebRender's minimum: an OpenGL ES context of 3.0 or later whose largest texture is
/// [`MIN_TEXTURE_SIZE`] or more. The `Err` says which of them failed, in the words of the diagnostic line.
///
/// # Errors
///
/// The reason the context cannot drive WebRender.
pub fn probe(version: &str, renderer: &str, max_texture_size: i32) -> Result<Probe, String> {
    let Some(parsed) = parse_gles_version(version) else {
        return Err(format!(
            "the context is not OpenGL ES (GL_VERSION is {version:?})"
        ));
    };
    if parsed.major < 3 {
        return Err(format!(
            "OpenGL ES {}.{} is too old: WebRender needs 3.0",
            parsed.major, parsed.minor
        ));
    }
    if max_texture_size < MIN_TEXTURE_SIZE {
        return Err(format!(
            "MAX_TEXTURE_SIZE is {max_texture_size}: WebRender needs {MIN_TEXTURE_SIZE}"
        ));
    }
    Ok(Probe {
        version: parsed,
        class: classify_renderer(renderer),
    })
}

/// The GL functions WebRender calls that the context's function table must have resolved, by the name the table keeps
/// them under. Every one is core in OpenGL ES 3.0; the table has the desktop names, which is why the aliases of the
/// loader (`glBufferStorage` for `glBufferStorageEXT`) exist for the extension ones, and why `glClearDepth` is not on
/// the list: it does not exist in OpenGL ES, and the device clears depth through `glClearBufferfv` there.
pub const REQUIRED_FUNCTIONS: &[&str] = &[
    "glGetString",
    "glGetStringi",
    "glGetIntegerv",
    "glGetError",
    "glCreateShader",
    "glShaderSource",
    "glCompileShader",
    "glCreateProgram",
    "glLinkProgram",
    "glUseProgram",
    "glGenVertexArrays",
    "glBindVertexArray",
    "glGenBuffers",
    "glBindBuffer",
    "glBufferData",
    "glBufferSubData",
    "glMapBufferRange",
    "glUnmapBuffer",
    "glFlushMappedBufferRange",
    "glVertexAttribPointer",
    "glVertexAttribIPointer",
    "glVertexAttribDivisor",
    "glDrawElementsInstanced",
    "glDrawElements",
    "glGenTextures",
    "glBindTexture",
    "glTexStorage2D",
    "glTexImage2D",
    "glTexSubImage2D",
    "glTexParameteri",
    "glGenFramebuffers",
    "glBindFramebuffer",
    "glFramebufferTexture2D",
    "glFramebufferRenderbuffer",
    "glBlitFramebuffer",
    "glInvalidateFramebuffer",
    "glClearBufferfv",
    "glClear",
    "glClearColor",
    "glReadPixels",
    "glFenceSync",
    "glClientWaitSync",
    "glProgramBinary",
    "glGetProgramBinary",
];

#[cfg(test)]
#[path = "gles_probe_test.rs"]
mod tests;
