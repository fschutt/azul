//! The pure part of the EGL binding the phone shells' GPU path (and its Linux host test) use: the constants, the names of
//! the error codes, the extensions a display answers, the attribute lists of the config requests, the damage rectangles a
//! swap takes, and what an error of a swap means. `std` only, so a host checks every table without an EGL library.
//!
//! The binding itself (`egl_gles`) loads `libEGL.so` at run time and calls these.

/// `EGLint`.
pub type EglInt = i32;

/// The error codes of `eglGetError`.
pub const EGL_SUCCESS: EglInt = 0x3000;
/// `EGL_NOT_INITIALIZED`.
pub const EGL_NOT_INITIALIZED: EglInt = 0x3001;
/// `EGL_BAD_ACCESS`.
pub const EGL_BAD_ACCESS: EglInt = 0x3002;
/// `EGL_BAD_ALLOC`.
pub const EGL_BAD_ALLOC: EglInt = 0x3003;
/// `EGL_BAD_ATTRIBUTE`.
pub const EGL_BAD_ATTRIBUTE: EglInt = 0x3004;
/// `EGL_BAD_CONFIG`.
pub const EGL_BAD_CONFIG: EglInt = 0x3005;
/// `EGL_BAD_CONTEXT`.
pub const EGL_BAD_CONTEXT: EglInt = 0x3006;
/// `EGL_BAD_CURRENT_SURFACE`.
pub const EGL_BAD_CURRENT_SURFACE: EglInt = 0x3007;
/// `EGL_BAD_DISPLAY`.
pub const EGL_BAD_DISPLAY: EglInt = 0x3008;
/// `EGL_BAD_MATCH`.
pub const EGL_BAD_MATCH: EglInt = 0x3009;
/// `EGL_BAD_NATIVE_PIXMAP`.
pub const EGL_BAD_NATIVE_PIXMAP: EglInt = 0x300A;
/// `EGL_BAD_NATIVE_WINDOW`.
pub const EGL_BAD_NATIVE_WINDOW: EglInt = 0x300B;
/// `EGL_BAD_PARAMETER`.
pub const EGL_BAD_PARAMETER: EglInt = 0x300C;
/// `EGL_BAD_SURFACE`.
pub const EGL_BAD_SURFACE: EglInt = 0x300D;
/// `EGL_CONTEXT_LOST`: a power management event or a driver reset took the context.
pub const EGL_CONTEXT_LOST: EglInt = 0x300E;

/// Config attributes.
pub const EGL_ALPHA_SIZE: EglInt = 0x3021;
/// `EGL_BLUE_SIZE`.
pub const EGL_BLUE_SIZE: EglInt = 0x3022;
/// `EGL_GREEN_SIZE`.
pub const EGL_GREEN_SIZE: EglInt = 0x3023;
/// `EGL_RED_SIZE`.
pub const EGL_RED_SIZE: EglInt = 0x3024;
/// `EGL_DEPTH_SIZE`.
pub const EGL_DEPTH_SIZE: EglInt = 0x3025;
/// `EGL_STENCIL_SIZE`.
pub const EGL_STENCIL_SIZE: EglInt = 0x3026;
/// `EGL_SURFACE_TYPE`.
pub const EGL_SURFACE_TYPE: EglInt = 0x3033;
/// `EGL_NONE`: ends an attribute list.
pub const EGL_NONE: EglInt = 0x3038;
/// `EGL_RENDERABLE_TYPE`.
pub const EGL_RENDERABLE_TYPE: EglInt = 0x3040;
/// `EGL_NATIVE_VISUAL_ID`: the pixel format the window must be given (`ANativeWindow_setBuffersGeometry`).
pub const EGL_NATIVE_VISUAL_ID: EglInt = 0x302E;
/// `EGL_PBUFFER_BIT` of `EGL_SURFACE_TYPE`.
pub const EGL_PBUFFER_BIT: EglInt = 0x0001;
/// `EGL_WINDOW_BIT` of `EGL_SURFACE_TYPE`.
pub const EGL_WINDOW_BIT: EglInt = 0x0004;
/// `EGL_OPENGL_ES3_BIT` of `EGL_RENDERABLE_TYPE` (EGL 1.5; `EGL_OPENGL_ES3_BIT_KHR` before).
pub const EGL_OPENGL_ES3_BIT: EglInt = 0x0040;

/// `EGL_OPENGL_ES_API`, the argument of `eglBindAPI`.
pub const EGL_OPENGL_ES_API: u32 = 0x30A0;
/// `EGL_CONTEXT_CLIENT_VERSION` (`EGL_CONTEXT_MAJOR_VERSION` in EGL 1.5): the version of the ES context.
pub const EGL_CONTEXT_CLIENT_VERSION: EglInt = 0x3098;

/// `eglQueryString` names.
pub const EGL_VENDOR: EglInt = 0x3053;
/// `EGL_VERSION`.
pub const EGL_VERSION: EglInt = 0x3054;
/// `EGL_EXTENSIONS`.
pub const EGL_EXTENSIONS: EglInt = 0x3055;

/// `eglQuerySurface` attributes.
pub const EGL_HEIGHT: EglInt = 0x3056;
/// `EGL_WIDTH`.
pub const EGL_WIDTH: EglInt = 0x3057;
/// `EGL_BUFFER_AGE_EXT` (`EGL_EXT_buffer_age`): how many frames old the back buffer is.
pub const EGL_BUFFER_AGE_EXT: EglInt = 0x313D;

/// The platform of `eglGetPlatformDisplayEXT` that needs no window system (Mesa): what the Linux host test of the
/// GLES path runs on.
pub const EGL_PLATFORM_SURFACELESS_MESA: u32 = 0x31DD;

/// The name of an `eglGetError` code, for the diagnostic line.
#[must_use]
pub const fn egl_error_name(code: EglInt) -> &'static str {
    match code {
        EGL_SUCCESS => "EGL_SUCCESS",
        EGL_NOT_INITIALIZED => "EGL_NOT_INITIALIZED",
        EGL_BAD_ACCESS => "EGL_BAD_ACCESS",
        EGL_BAD_ALLOC => "EGL_BAD_ALLOC",
        EGL_BAD_ATTRIBUTE => "EGL_BAD_ATTRIBUTE",
        EGL_BAD_CONFIG => "EGL_BAD_CONFIG",
        EGL_BAD_CONTEXT => "EGL_BAD_CONTEXT",
        EGL_BAD_CURRENT_SURFACE => "EGL_BAD_CURRENT_SURFACE",
        EGL_BAD_DISPLAY => "EGL_BAD_DISPLAY",
        EGL_BAD_MATCH => "EGL_BAD_MATCH",
        EGL_BAD_NATIVE_PIXMAP => "EGL_BAD_NATIVE_PIXMAP",
        EGL_BAD_NATIVE_WINDOW => "EGL_BAD_NATIVE_WINDOW",
        EGL_BAD_PARAMETER => "EGL_BAD_PARAMETER",
        EGL_BAD_SURFACE => "EGL_BAD_SURFACE",
        EGL_CONTEXT_LOST => "EGL_CONTEXT_LOST",
        _ => "an unknown EGL error",
    }
}

/// What a display's extension strings say it can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EglCaps {
    /// `EGL_KHR_surfaceless_context`: a context can be made current with no surface at all, which is how the renderer
    /// and its GPU memory outlive the window's surface (`TerminateWindow`, a rotation).
    pub surfaceless_context: bool,
    /// `EGL_EXT_buffer_age`: the age of the back buffer can be queried, so WebRender draws only the damage of the
    /// last `age` frames.
    pub buffer_age: bool,
    /// `EGL_KHR_swap_buffers_with_damage`: `eglSwapBuffersWithDamageKHR`.
    pub swap_with_damage_khr: bool,
    /// `EGL_EXT_swap_buffers_with_damage`: `eglSwapBuffersWithDamageEXT`.
    pub swap_with_damage_ext: bool,
}

impl EglCaps {
    /// Reads the extension strings of the display (`eglQueryString(display, EGL_EXTENSIONS)`).
    #[must_use]
    pub fn from_extensions(display_extensions: &str) -> Self {
        let has = |name: &str| display_extensions.split_whitespace().any(|e| e == name);
        Self {
            surfaceless_context: has("EGL_KHR_surfaceless_context"),
            buffer_age: has("EGL_EXT_buffer_age"),
            swap_with_damage_khr: has("EGL_KHR_swap_buffers_with_damage"),
            swap_with_damage_ext: has("EGL_EXT_swap_buffers_with_damage"),
        }
    }

    /// The name of the swap-with-damage function the display has, the KHR one first.
    #[must_use]
    pub const fn swap_with_damage_symbol(self) -> Option<&'static str> {
        if self.swap_with_damage_khr {
            Some("eglSwapBuffersWithDamageKHR")
        } else if self.swap_with_damage_ext {
            Some("eglSwapBuffersWithDamageEXT")
        } else {
            None
        }
    }
}

/// One request for a framebuffer configuration. WebRender draws into the default framebuffer without a depth buffer
/// (`DrawTarget::Default` has none; the targets with depth are textures it makes itself), so the first wish is the
/// cheapest one: colour only. The others are what a driver that offers no config without depth gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigWish {
    /// `EGL_DEPTH_SIZE` asked for.
    pub depth: EglInt,
    /// `EGL_STENCIL_SIZE` asked for.
    pub stencil: EglInt,
}

/// The wishes, in the order they are tried.
pub const CONFIG_WISHES: [ConfigWish; 3] = [
    ConfigWish { depth: 0, stencil: 0 },
    ConfigWish { depth: 16, stencil: 0 },
    ConfigWish { depth: 24, stencil: 8 },
];

impl ConfigWish {
    /// The attribute list of `eglChooseConfig`: RGBA8888, OpenGL ES 3 renderable, the surface types in
    /// `surface_type` (a window and a pbuffer: `EGL_WINDOW_BIT | EGL_PBUFFER_BIT`, so the context can be current on
    /// the pbuffer while the window has no surface; a window only; a pbuffer only for the host test).
    #[must_use]
    pub fn attribs(self, surface_type: EglInt) -> Vec<EglInt> {
        vec![
            EGL_RED_SIZE,
            8,
            EGL_GREEN_SIZE,
            8,
            EGL_BLUE_SIZE,
            8,
            EGL_ALPHA_SIZE,
            8,
            EGL_DEPTH_SIZE,
            self.depth,
            EGL_STENCIL_SIZE,
            self.stencil,
            EGL_SURFACE_TYPE,
            surface_type,
            EGL_RENDERABLE_TYPE,
            EGL_OPENGL_ES3_BIT,
            EGL_NONE,
        ]
    }
}

/// The attribute list of `eglCreateContext` for an OpenGL ES context of the given major version.
#[must_use]
pub fn context_attribs(major: EglInt) -> Vec<EglInt> {
    vec![EGL_CONTEXT_CLIENT_VERSION, major, EGL_NONE]
}

/// The damage of a frame as `eglSwapBuffersWithDamage` takes it: `x, y, width, height` rectangles with the origin at
/// the BOTTOM left, from WebRender's rectangles with the origin at the top left (`(x, y, width, height)` in device
/// pixels, as `wr_translate2::device_rects_to_present_rects` makes them). `buffer_height` is the height of the surface.
#[must_use]
pub fn damage_rects_for_swap(rects: &[(u32, u32, u32, u32)], buffer_height: u32) -> Vec<EglInt> {
    let to_int = |v: u32| EglInt::try_from(v).unwrap_or(EglInt::MAX);
    let mut out = Vec::with_capacity(rects.len() * 4);
    for &(x, y, w, h) in rects {
        let flipped_y = buffer_height.saturating_sub(y.saturating_add(h));
        out.extend_from_slice(&[to_int(x), to_int(flipped_y), to_int(w), to_int(h)]);
    }
    out
}

/// What a failed `eglSwapBuffers` means for the frame loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwapError {
    /// The context is gone (`EGL_CONTEXT_LOST`, `EGL_BAD_CONTEXT`): the renderer and every GL object in it are, too. The
    /// shell rebuilds both.
    ContextLost,
    /// The window's surface is gone (`EGL_BAD_SURFACE`, `EGL_BAD_NATIVE_WINDOW`, `EGL_BAD_CURRENT_SURFACE`): the system
    /// took the window (a `TerminateWindow` is on its way). The frame is dropped, the next surface gets the next one.
    SurfaceLost,
    /// Anything else (`EGL_BAD_ALLOC`, `EGL_NOT_INITIALIZED`, an unknown code): the frame is lost and the cause is in
    /// the log; a second one in a row is a reason to leave the GPU path.
    Failed(EglInt),
}

/// Classifies the `eglGetError` code that followed a swap that returned `EGL_FALSE`.
#[must_use]
pub const fn classify_swap_error(code: EglInt) -> SwapError {
    match code {
        EGL_CONTEXT_LOST | EGL_BAD_CONTEXT => SwapError::ContextLost,
        EGL_BAD_SURFACE | EGL_BAD_NATIVE_WINDOW | EGL_BAD_CURRENT_SURFACE => SwapError::SurfaceLost,
        other => SwapError::Failed(other),
    }
}

/// The age of the back buffer for WebRender (`renderer.render(size, buffer_age)`): 0, "undefined, draw everything",
/// when the extension is missing, the query failed, or the driver answered a negative number.
#[must_use]
pub const fn usable_buffer_age(supported: bool, answered: Option<EglInt>) -> usize {
    match (supported, answered) {
        (true, Some(age)) if age > 0 => age as usize,
        _ => 0,
    }
}

#[cfg(test)]
#[path = "egl_pure_test.rs"]
mod tests;
