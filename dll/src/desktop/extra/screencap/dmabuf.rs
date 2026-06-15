//! DMA-BUF → BGRA/RGBA readback via a self-contained, surfaceless EGL/GLES2
//! context (all entry points `dlopen`'d, no link-time GL dependency — same
//! philosophy as the PipeWire loader next door).
//!
//! Why this exists: on most Wayland desktops (GNOME/KDE) and any GPU-composited
//! X11 session, the xdg-desktop-portal ScreenCast producer hands PipeWire
//! frames as **DMA-BUF** file descriptors, never CPU-mappable memory — so the
//! consumer MUST import the dmabuf into the GPU to get pixels. We import it as
//! an `EGLImage` (`EGL_LINUX_DMA_BUF_EXT`), bind it to a GL texture, blit it
//! through a trivial shader into a plain renderable RGBA8 texture, and
//! `glReadPixels` that back to a tightly-packed RGBA buffer. The blit (rather
//! than reading the imported texture directly) is what makes it work "no matter
//! what" — tiled/compressed modifiers aren't guaranteed color-renderable, but
//! the RGBA8 destination always is.
//!
//! The same imported `EGLImage`/texture is the natural hand-off point for a
//! future zero-copy GPU encode path (Vulkan Video / `gpu-video`): keep the
//! dmabuf fd on the GPU instead of reading it back. For now cpurender needs CPU
//! BGRA, so we always read back.

use std::ffi::{c_char, c_void};
use std::ptr;
use std::sync::Mutex;

macro_rules! scd {
    ($($arg:tt)*) => {{
        if std::env::var_os("AZ_SCREENCAP_DEBUG").is_some() {
            eprintln!("[screencap/egl] {}", format!($($arg)*));
        }
    }};
}

// --- DRM fourccs (match the SPA video formats we negotiate) -----------------
pub const DRM_FORMAT_ARGB8888: u32 = 0x3432_5241; // 'AR24' <- SPA BGRA
pub const DRM_FORMAT_ABGR8888: u32 = 0x3432_4241; // 'AB24' <- SPA RGBA
pub const DRM_FORMAT_XRGB8888: u32 = 0x3432_5258; // 'XR24' <- SPA BGRx
pub const DRM_FORMAT_XBGR8888: u32 = 0x3432_4258; // 'XB24' <- SPA RGBx
pub const DRM_FORMAT_MOD_INVALID: u64 = 0x00ff_ffff_ffff_ffff;

// --- EGL tokens -------------------------------------------------------------
const EGL_TRUE: u32 = 1;
const EGL_NONE: i32 = 0x3038;
const EGL_EXTENSIONS: i32 = 0x3055;
const EGL_NO_CONTEXT: *mut c_void = ptr::null_mut();
const EGL_NO_SURFACE: *mut c_void = ptr::null_mut();
const EGL_DEFAULT_DISPLAY: *mut c_void = ptr::null_mut();
const EGL_OPENGL_ES_API: u32 = 0x30A0;
const EGL_PLATFORM_SURFACELESS_MESA: u32 = 0x31DD;
const EGL_SURFACE_TYPE: i32 = 0x3033;
const EGL_PBUFFER_BIT: i32 = 0x0001;
const EGL_RENDERABLE_TYPE: i32 = 0x3040;
const EGL_OPENGL_ES2_BIT: i32 = 0x0004;
const EGL_RED_SIZE: i32 = 0x3024;
const EGL_GREEN_SIZE: i32 = 0x3023;
const EGL_BLUE_SIZE: i32 = 0x3022;
const EGL_ALPHA_SIZE: i32 = 0x3021;
const EGL_CONTEXT_CLIENT_VERSION: i32 = 0x3098;
const EGL_WIDTH: i32 = 0x3057;
const EGL_HEIGHT: i32 = 0x3056;
const EGL_LINUX_DMA_BUF_EXT: u32 = 0x3270;
const EGL_LINUX_DRM_FOURCC_EXT: i32 = 0x3271;
// plane fd / offset / pitch / modifier-lo / modifier-hi for planes 0..3
const PLANE_FD: [i32; 4] = [0x3272, 0x3275, 0x3278, 0x3440];
const PLANE_OFFSET: [i32; 4] = [0x3273, 0x3276, 0x3279, 0x3441];
const PLANE_PITCH: [i32; 4] = [0x3274, 0x3277, 0x327A, 0x3442];
const PLANE_MOD_LO: [i32; 4] = [0x3443, 0x3445, 0x3447, 0x3449];
const PLANE_MOD_HI: [i32; 4] = [0x3444, 0x3446, 0x3448, 0x344A];

// --- GL tokens --------------------------------------------------------------
const GL_TEXTURE_2D: u32 = 0x0DE1;
const GL_TEXTURE0: u32 = 0x84C0;
const GL_TEXTURE_MIN_FILTER: u32 = 0x2801;
const GL_TEXTURE_MAG_FILTER: u32 = 0x2800;
const GL_TEXTURE_WRAP_S: u32 = 0x2802;
const GL_TEXTURE_WRAP_T: u32 = 0x2803;
const GL_NEAREST: i32 = 0x2600;
const GL_CLAMP_TO_EDGE: i32 = 0x812F;
const GL_RGBA: u32 = 0x1908;
const GL_UNSIGNED_BYTE: u32 = 0x1401;
const GL_FRAMEBUFFER: u32 = 0x8D40;
const GL_COLOR_ATTACHMENT0: u32 = 0x8CE0;
const GL_FRAMEBUFFER_COMPLETE: u32 = 0x8CD5;
const GL_ARRAY_BUFFER: u32 = 0x8892;
const GL_STATIC_DRAW: u32 = 0x88E4;
const GL_VERTEX_SHADER: u32 = 0x8B31;
const GL_FRAGMENT_SHADER: u32 = 0x8B30;
const GL_COMPILE_STATUS: u32 = 0x8B81;
const GL_LINK_STATUS: u32 = 0x8B82;
const GL_FLOAT: u32 = 0x1406;
const GL_TRIANGLES: u32 = 0x0004;

// --- function-pointer ABI types --------------------------------------------
type PFN = unsafe extern "C" fn();
type EglGetProcAddress = unsafe extern "C" fn(*const c_char) -> Option<PFN>;
type EglGetPlatformDisplay = unsafe extern "C" fn(u32, *mut c_void, *const i32) -> *mut c_void;
type EglGetDisplay = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
type EglInitialize = unsafe extern "C" fn(*mut c_void, *mut i32, *mut i32) -> u32;
type EglQueryString = unsafe extern "C" fn(*mut c_void, i32) -> *const c_char;
type EglChooseConfig =
    unsafe extern "C" fn(*mut c_void, *const i32, *mut *mut c_void, i32, *mut i32) -> u32;
type EglBindApi = unsafe extern "C" fn(u32) -> u32;
type EglCreateContext =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *const i32) -> *mut c_void;
type EglCreatePbufferSurface =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *const i32) -> *mut c_void;
type EglMakeCurrent =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *mut c_void) -> u32;
type EglGetError = unsafe extern "C" fn() -> i32;
type EglCreateImageKhr =
    unsafe extern "C" fn(*mut c_void, *mut c_void, u32, *mut c_void, *const i32) -> *mut c_void;
type EglDestroyImageKhr = unsafe extern "C" fn(*mut c_void, *mut c_void) -> u32;
type EglQueryDmaBufModifiers =
    unsafe extern "C" fn(*mut c_void, i32, i32, *mut u64, *mut u32, *mut i32) -> u32;

type GlGenObj = unsafe extern "C" fn(i32, *mut u32);
type GlBind = unsafe extern "C" fn(u32, u32);
type GlTexParameteri = unsafe extern "C" fn(u32, u32, i32);
type GlTexImage2D =
    unsafe extern "C" fn(u32, i32, i32, i32, i32, i32, u32, u32, *const c_void);
type GlFramebufferTexture2D = unsafe extern "C" fn(u32, u32, u32, u32, i32);
type GlCheckFramebufferStatus = unsafe extern "C" fn(u32) -> u32;
type GlViewport = unsafe extern "C" fn(i32, i32, i32, i32);
type GlCreateShader = unsafe extern "C" fn(u32) -> u32;
type GlShaderSource = unsafe extern "C" fn(u32, i32, *const *const c_char, *const i32);
type GlCompileShader = unsafe extern "C" fn(u32);
type GlGetShaderiv = unsafe extern "C" fn(u32, u32, *mut i32);
type GlCreateProgram = unsafe extern "C" fn() -> u32;
type GlAttachShader = unsafe extern "C" fn(u32, u32);
type GlLinkProgram = unsafe extern "C" fn(u32);
type GlGetProgramiv = unsafe extern "C" fn(u32, u32, *mut i32);
type GlUseProgram = unsafe extern "C" fn(u32);
type GlBufferData = unsafe extern "C" fn(u32, isize, *const c_void, u32);
type GlGetLoc = unsafe extern "C" fn(u32, *const c_char) -> i32;
type GlEnableVAA = unsafe extern "C" fn(u32);
type GlVertexAttribPointer = unsafe extern "C" fn(u32, i32, u32, u8, i32, *const c_void);
type GlUniform1i = unsafe extern "C" fn(i32, i32);
type GlActiveTexture = unsafe extern "C" fn(u32);
type GlDrawArrays = unsafe extern "C" fn(u32, i32, i32);
type GlReadPixels = unsafe extern "C" fn(i32, i32, i32, i32, u32, u32, *mut c_void);
type GlImageTargetTexture2DOes = unsafe extern "C" fn(u32, *mut c_void);

struct EglFns {
    initialize: EglInitialize,
    query_string: EglQueryString,
    choose_config: EglChooseConfig,
    bind_api: EglBindApi,
    create_context: EglCreateContext,
    create_pbuffer_surface: EglCreatePbufferSurface,
    make_current: EglMakeCurrent,
    get_error: EglGetError,
    create_image: EglCreateImageKhr,
    destroy_image: EglDestroyImageKhr,
    query_dmabuf_modifiers: Option<EglQueryDmaBufModifiers>,
}

struct GlFns {
    gen_textures: GlGenObj,
    bind_texture: GlBind,
    tex_parameteri: GlTexParameteri,
    tex_image_2d: GlTexImage2D,
    gen_framebuffers: GlGenObj,
    bind_framebuffer: GlBind,
    framebuffer_texture_2d: GlFramebufferTexture2D,
    check_framebuffer_status: GlCheckFramebufferStatus,
    viewport: GlViewport,
    create_shader: GlCreateShader,
    shader_source: GlShaderSource,
    compile_shader: GlCompileShader,
    get_shaderiv: GlGetShaderiv,
    create_program: GlCreateProgram,
    attach_shader: GlAttachShader,
    link_program: GlLinkProgram,
    get_programiv: GlGetProgramiv,
    use_program: GlUseProgram,
    gen_buffers: GlGenObj,
    bind_buffer: GlBind,
    buffer_data: GlBufferData,
    get_attrib_location: GlGetLoc,
    get_uniform_location: GlGetLoc,
    enable_vertex_attrib_array: GlEnableVAA,
    vertex_attrib_pointer: GlVertexAttribPointer,
    uniform1i: GlUniform1i,
    active_texture: GlActiveTexture,
    draw_arrays: GlDrawArrays,
    read_pixels: GlReadPixels,
    image_target_texture_2d_oes: GlImageTargetTexture2DOes,
}

/// One plane of a DMA-BUF frame (RGB formats are single-plane, but the import
/// path is written for up to 4 so compressed/auxiliary-plane modifiers work).
pub struct Plane {
    pub fd: i32,
    pub offset: u32,
    pub stride: u32,
}

/// GL objects built lazily on the data thread (where the context is current).
struct Blit {
    prog: u32,
    vbo: u32,
    fbo: u32,
    src_tex: u32,
    dst_tex: u32,
    dst_w: u32,
    dst_h: u32,
    a_pos: i32,
    u_tex: i32,
}

pub struct EglBackend {
    _libegl: libloading::Library,
    _libgles: libloading::Library,
    egl: EglFns,
    gl: GlFns,
    dpy: *mut c_void,
    ctx: *mut c_void,
    surface: *mut c_void, // NO_SURFACE (surfaceless) or a 1x1 pbuffer
    blit: Mutex<Option<Blit>>,
}

// dpy/ctx are only touched under `blit`'s lock from the single data thread.
unsafe impl Send for EglBackend {}
unsafe impl Sync for EglBackend {}

unsafe fn proc_addr<T: Copy>(get: EglGetProcAddress, name: &str) -> Option<T> {
    let c = std::ffi::CString::new(name).ok()?;
    let p = get(c.as_ptr())?;
    debug_assert_eq!(std::mem::size_of::<T>(), std::mem::size_of::<PFN>());
    Some(*(&p as *const PFN as *const T))
}

impl EglBackend {
    /// Build a surfaceless EGL/GLES2 context for dmabuf import. Returns `None`
    /// (and the caller falls back to shared-memory buffers) if EGL or the
    /// required extensions are unavailable. Does NOT make the context current —
    /// that happens on the PipeWire data thread in `import_to_rgba`.
    pub fn init() -> Option<EglBackend> {
        unsafe {
            let libegl = crate::desktop::open_first_lib(&["libEGL.so.1"])?;
            let libgles = crate::desktop::open_first_lib(&["libGLESv2.so.2"])?;

            macro_rules! egl_core {
                ($t:ty, $n:literal) => {{
                    let s: libloading::Symbol<$t> = libegl.get($n).ok()?;
                    *s
                }};
            }
            let get_proc: EglGetProcAddress = egl_core!(EglGetProcAddress, b"eglGetProcAddress");
            let get_display: EglGetDisplay = egl_core!(EglGetDisplay, b"eglGetDisplay");
            let initialize: EglInitialize = egl_core!(EglInitialize, b"eglInitialize");
            let query_string: EglQueryString = egl_core!(EglQueryString, b"eglQueryString");
            let choose_config: EglChooseConfig = egl_core!(EglChooseConfig, b"eglChooseConfig");
            let bind_api: EglBindApi = egl_core!(EglBindApi, b"eglBindAPI");
            let create_context: EglCreateContext =
                egl_core!(EglCreateContext, b"eglCreateContext");
            let create_pbuffer_surface: EglCreatePbufferSurface =
                egl_core!(EglCreatePbufferSurface, b"eglCreatePbufferSurface");
            let make_current: EglMakeCurrent = egl_core!(EglMakeCurrent, b"eglMakeCurrent");
            let get_error: EglGetError = egl_core!(EglGetError, b"eglGetError");

            // Extension entry points (loaded through eglGetProcAddress).
            let get_platform_display: Option<EglGetPlatformDisplay> =
                proc_addr(get_proc, "eglGetPlatformDisplayEXT");
            let create_image: EglCreateImageKhr = proc_addr(get_proc, "eglCreateImageKHR")?;
            let destroy_image: EglDestroyImageKhr = proc_addr(get_proc, "eglDestroyImageKHR")?;
            let query_dmabuf_modifiers: Option<EglQueryDmaBufModifiers> =
                proc_addr(get_proc, "eglQueryDmaBufModifiersEXT");
            let image_target_texture_2d_oes: GlImageTargetTexture2DOes =
                proc_addr(get_proc, "glEGLImageTargetTexture2DOES")?;

            // Surfaceless display preferred; fall back to the default display.
            let dpy = match get_platform_display {
                Some(f) => {
                    let d = f(EGL_PLATFORM_SURFACELESS_MESA, EGL_DEFAULT_DISPLAY, ptr::null());
                    if d.is_null() {
                        get_display(EGL_DEFAULT_DISPLAY)
                    } else {
                        d
                    }
                }
                None => get_display(EGL_DEFAULT_DISPLAY),
            };
            if dpy.is_null() {
                scd!("no EGL display");
                return None;
            }
            let (mut major, mut minor) = (0i32, 0i32);
            if initialize(dpy, &mut major, &mut minor) != EGL_TRUE {
                scd!("eglInitialize failed: {:#x}", get_error());
                return None;
            }
            scd!("EGL {}.{} initialized", major, minor);

            // Need surfaceless contexts OR we make a tiny pbuffer below.
            let exts = {
                let p = query_string(dpy, EGL_EXTENSIONS);
                if p.is_null() {
                    String::new()
                } else {
                    std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
                }
            };

            if bind_api(EGL_OPENGL_ES_API) != EGL_TRUE {
                scd!("eglBindAPI(ES) failed");
                return None;
            }

            let cfg_attribs: [i32; 13] = [
                EGL_SURFACE_TYPE,
                EGL_PBUFFER_BIT,
                EGL_RENDERABLE_TYPE,
                EGL_OPENGL_ES2_BIT,
                EGL_RED_SIZE,
                8,
                EGL_GREEN_SIZE,
                8,
                EGL_BLUE_SIZE,
                8,
                EGL_ALPHA_SIZE,
                8,
                EGL_NONE,
            ];
            let mut config: *mut c_void = ptr::null_mut();
            let mut n_config = 0i32;
            if choose_config(dpy, cfg_attribs.as_ptr(), &mut config, 1, &mut n_config) != EGL_TRUE
                || n_config < 1
            {
                scd!("eglChooseConfig found no config");
                return None;
            }

            let ctx_attribs: [i32; 3] = [EGL_CONTEXT_CLIENT_VERSION, 2, EGL_NONE];
            let ctx = create_context(dpy, config, EGL_NO_CONTEXT, ctx_attribs.as_ptr());
            if ctx.is_null() {
                scd!("eglCreateContext failed: {:#x}", get_error());
                return None;
            }

            // Surfaceless if supported, else a throwaway 1x1 pbuffer to bind to.
            let surface = if exts.contains("EGL_KHR_surfaceless_context") {
                EGL_NO_SURFACE
            } else {
                let pb: [i32; 5] = [EGL_WIDTH, 1, EGL_HEIGHT, 1, EGL_NONE];
                let s = create_pbuffer_surface(dpy, config, pb.as_ptr());
                if s.is_null() {
                    scd!("no surfaceless ctx and pbuffer creation failed");
                    return None;
                }
                s
            };

            macro_rules! gl_sym {
                ($t:ty, $n:literal) => {{
                    let s: libloading::Symbol<$t> = libgles.get($n).ok()?;
                    *s
                }};
            }
            let gl = GlFns {
                gen_textures: gl_sym!(GlGenObj, b"glGenTextures"),
                bind_texture: gl_sym!(GlBind, b"glBindTexture"),
                tex_parameteri: gl_sym!(GlTexParameteri, b"glTexParameteri"),
                tex_image_2d: gl_sym!(GlTexImage2D, b"glTexImage2D"),
                gen_framebuffers: gl_sym!(GlGenObj, b"glGenFramebuffers"),
                bind_framebuffer: gl_sym!(GlBind, b"glBindFramebuffer"),
                framebuffer_texture_2d: gl_sym!(GlFramebufferTexture2D, b"glFramebufferTexture2D"),
                check_framebuffer_status: gl_sym!(
                    GlCheckFramebufferStatus,
                    b"glCheckFramebufferStatus"
                ),
                viewport: gl_sym!(GlViewport, b"glViewport"),
                create_shader: gl_sym!(GlCreateShader, b"glCreateShader"),
                shader_source: gl_sym!(GlShaderSource, b"glShaderSource"),
                compile_shader: gl_sym!(GlCompileShader, b"glCompileShader"),
                get_shaderiv: gl_sym!(GlGetShaderiv, b"glGetShaderiv"),
                create_program: gl_sym!(GlCreateProgram, b"glCreateProgram"),
                attach_shader: gl_sym!(GlAttachShader, b"glAttachShader"),
                link_program: gl_sym!(GlLinkProgram, b"glLinkProgram"),
                get_programiv: gl_sym!(GlGetProgramiv, b"glGetProgramiv"),
                use_program: gl_sym!(GlUseProgram, b"glUseProgram"),
                gen_buffers: gl_sym!(GlGenObj, b"glGenBuffers"),
                bind_buffer: gl_sym!(GlBind, b"glBindBuffer"),
                buffer_data: gl_sym!(GlBufferData, b"glBufferData"),
                get_attrib_location: gl_sym!(GlGetLoc, b"glGetAttribLocation"),
                get_uniform_location: gl_sym!(GlGetLoc, b"glGetUniformLocation"),
                enable_vertex_attrib_array: gl_sym!(GlEnableVAA, b"glEnableVertexAttribArray"),
                vertex_attrib_pointer: gl_sym!(GlVertexAttribPointer, b"glVertexAttribPointer"),
                uniform1i: gl_sym!(GlUniform1i, b"glUniform1i"),
                active_texture: gl_sym!(GlActiveTexture, b"glActiveTexture"),
                draw_arrays: gl_sym!(GlDrawArrays, b"glDrawArrays"),
                read_pixels: gl_sym!(GlReadPixels, b"glReadPixels"),
                image_target_texture_2d_oes,
            };

            scd!("EGL backend ready (surfaceless={})", surface.is_null());
            Some(EglBackend {
                _libegl: libegl,
                _libgles: libgles,
                egl: EglFns {
                    initialize,
                    query_string,
                    choose_config,
                    bind_api,
                    create_context,
                    create_pbuffer_surface,
                    make_current,
                    get_error,
                    create_image,
                    destroy_image,
                    query_dmabuf_modifiers,
                },
                gl,
                dpy,
                ctx,
                surface,
                blit: Mutex::new(None),
            })
        }
    }

    /// DRM format modifiers EGL can import for `fourcc`. Always includes
    /// `DRM_FORMAT_MOD_INVALID` (implicit/legacy) so a producer that only does
    /// implicit modifiers still matches. Safe to call without a current context.
    pub fn query_modifiers(&self, fourcc: u32) -> Vec<u64> {
        let mut out = Vec::new();
        unsafe {
            if let Some(qm) = self.egl.query_dmabuf_modifiers {
                let mut n = 0i32;
                if qm(self.dpy, fourcc as i32, 0, ptr::null_mut(), ptr::null_mut(), &mut n)
                    == EGL_TRUE
                    && n > 0
                {
                    let mut mods = vec![0u64; n as usize];
                    let mut got = 0i32;
                    if qm(
                        self.dpy,
                        fourcc as i32,
                        n,
                        mods.as_mut_ptr(),
                        ptr::null_mut(),
                        &mut got,
                    ) == EGL_TRUE
                    {
                        mods.truncate(got.max(0) as usize);
                        out = mods;
                    }
                }
            }
        }
        if !out.contains(&DRM_FORMAT_MOD_INVALID) {
            out.push(DRM_FORMAT_MOD_INVALID);
        }
        out
    }

    /// Import a DMA-BUF frame and read it back as tightly-packed RGBA8 (top-down).
    /// Runs entirely on the caller's thread (the PipeWire data thread) and makes
    /// the EGL context current there. Returns `None` on any import/GL failure so
    /// the caller can drop the frame without killing the stream.
    pub fn import_to_rgba(
        &self,
        planes: &[Plane],
        width: u32,
        height: u32,
        fourcc: u32,
        modifier: u64,
    ) -> Option<Vec<u8>> {
        if planes.is_empty() || width == 0 || height == 0 {
            return None;
        }
        let mut guard = self.blit.lock().ok()?;
        unsafe {
            if (self.egl.make_current)(self.dpy, self.surface, self.surface, self.ctx) != EGL_TRUE {
                scd!("eglMakeCurrent failed: {:#x}", (self.egl.get_error)());
                return None;
            }
            if guard.is_none() {
                *guard = Some(self.build_blit()?);
            }
            let blit = guard.as_mut().unwrap();

            // Assemble the EGL_LINUX_DMA_BUF_EXT attribute list.
            let mut a: Vec<i32> = Vec::with_capacity(7 + planes.len() * 10 + 1);
            a.extend_from_slice(&[EGL_WIDTH, width as i32, EGL_HEIGHT, height as i32]);
            a.extend_from_slice(&[EGL_LINUX_DRM_FOURCC_EXT, fourcc as i32]);
            for (i, p) in planes.iter().enumerate().take(4) {
                a.extend_from_slice(&[PLANE_FD[i], p.fd]);
                a.extend_from_slice(&[PLANE_OFFSET[i], p.offset as i32]);
                a.extend_from_slice(&[PLANE_PITCH[i], p.stride as i32]);
                if modifier != DRM_FORMAT_MOD_INVALID {
                    a.extend_from_slice(&[PLANE_MOD_LO[i], (modifier & 0xffff_ffff) as i32]);
                    a.extend_from_slice(&[PLANE_MOD_HI[i], (modifier >> 32) as i32]);
                }
            }
            a.push(EGL_NONE);

            let img = (self.egl.create_image)(
                self.dpy,
                EGL_NO_CONTEXT,
                EGL_LINUX_DMA_BUF_EXT,
                ptr::null_mut(),
                a.as_ptr(),
            );
            if img.is_null() {
                scd!("eglCreateImageKHR failed: {:#x}", (self.egl.get_error)());
                return None;
            }

            (self.gl.bind_texture)(GL_TEXTURE_2D, blit.src_tex);
            (self.gl.image_target_texture_2d_oes)(GL_TEXTURE_2D, img);
            (self.gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_NEAREST);
            (self.gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_NEAREST);
            (self.gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
            (self.gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);

            // (Re)size the renderable destination texture to match the frame.
            if blit.dst_w != width || blit.dst_h != height {
                (self.gl.bind_texture)(GL_TEXTURE_2D, blit.dst_tex);
                (self.gl.tex_image_2d)(
                    GL_TEXTURE_2D,
                    0,
                    GL_RGBA as i32,
                    width as i32,
                    height as i32,
                    0,
                    GL_RGBA,
                    GL_UNSIGNED_BYTE,
                    ptr::null(),
                );
                (self.gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_NEAREST);
                (self.gl.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_NEAREST);
                blit.dst_w = width;
                blit.dst_h = height;
            }

            (self.gl.bind_framebuffer)(GL_FRAMEBUFFER, blit.fbo);
            (self.gl.framebuffer_texture_2d)(
                GL_FRAMEBUFFER,
                GL_COLOR_ATTACHMENT0,
                GL_TEXTURE_2D,
                blit.dst_tex,
                0,
            );
            if (self.gl.check_framebuffer_status)(GL_FRAMEBUFFER) != GL_FRAMEBUFFER_COMPLETE {
                scd!("FBO incomplete");
                (self.egl.destroy_image)(self.dpy, img);
                return None;
            }

            (self.gl.viewport)(0, 0, width as i32, height as i32);
            (self.gl.use_program)(blit.prog);
            (self.gl.active_texture)(GL_TEXTURE0);
            (self.gl.bind_texture)(GL_TEXTURE_2D, blit.src_tex);
            (self.gl.uniform1i)(blit.u_tex, 0);
            (self.gl.bind_buffer)(GL_ARRAY_BUFFER, blit.vbo);
            (self.gl.enable_vertex_attrib_array)(blit.a_pos as u32);
            (self.gl.vertex_attrib_pointer)(blit.a_pos as u32, 2, GL_FLOAT, 0, 0, ptr::null());
            (self.gl.draw_arrays)(GL_TRIANGLES, 0, 3);

            let mut out = vec![0u8; (width as usize) * (height as usize) * 4];
            (self.gl.read_pixels)(
                0,
                0,
                width as i32,
                height as i32,
                GL_RGBA,
                GL_UNSIGNED_BYTE,
                out.as_mut_ptr() as *mut c_void,
            );

            (self.egl.destroy_image)(self.dpy, img);
            Some(out)
        }
    }

    /// Compile the blit program + allocate the FBO/textures/VBO. Context must be
    /// current. The fullscreen triangle maps NDC bottom-left → texcoord (0,0),
    /// so the imported top-down frame lands top-down after `glReadPixels`'
    /// bottom-up read — matching the CPU mmap path's row order.
    unsafe fn build_blit(&self) -> Option<Blit> {
        let vs = b"attribute vec2 pos;\nvarying vec2 uv;\nvoid main(){ uv=(pos+1.0)*0.5; gl_Position=vec4(pos,0.0,1.0); }\0";
        let fs = b"precision mediump float;\nvarying vec2 uv;\nuniform sampler2D tex;\nvoid main(){ gl_FragColor=texture2D(tex,uv); }\0";

        let compile = |kind: u32, src: &[u8]| -> Option<u32> {
            let sh = (self.gl.create_shader)(kind);
            let p = src.as_ptr() as *const c_char;
            (self.gl.shader_source)(sh, 1, &p, ptr::null());
            (self.gl.compile_shader)(sh);
            let mut ok = 0i32;
            (self.gl.get_shaderiv)(sh, GL_COMPILE_STATUS, &mut ok);
            if ok == 0 {
                scd!("shader compile failed (kind {:#x})", kind);
                return None;
            }
            Some(sh)
        };
        let vsh = compile(GL_VERTEX_SHADER, vs)?;
        let fsh = compile(GL_FRAGMENT_SHADER, fs)?;
        let prog = (self.gl.create_program)();
        (self.gl.attach_shader)(prog, vsh);
        (self.gl.attach_shader)(prog, fsh);
        (self.gl.link_program)(prog);
        let mut ok = 0i32;
        (self.gl.get_programiv)(prog, GL_LINK_STATUS, &mut ok);
        if ok == 0 {
            scd!("program link failed");
            return None;
        }
        let a_pos = (self.gl.get_attrib_location)(prog, b"pos\0".as_ptr() as *const c_char);
        let u_tex = (self.gl.get_uniform_location)(prog, b"tex\0".as_ptr() as *const c_char);
        if a_pos < 0 {
            scd!("attrib 'pos' not found");
            return None;
        }

        // Fullscreen triangle (covers the viewport with 3 verts).
        let verts: [f32; 6] = [-1.0, -1.0, 3.0, -1.0, -1.0, 3.0];
        let mut vbo = 0u32;
        (self.gl.gen_buffers)(1, &mut vbo);
        (self.gl.bind_buffer)(GL_ARRAY_BUFFER, vbo);
        (self.gl.buffer_data)(
            GL_ARRAY_BUFFER,
            std::mem::size_of_val(&verts) as isize,
            verts.as_ptr() as *const c_void,
            GL_STATIC_DRAW,
        );

        let mut src_tex = 0u32;
        let mut dst_tex = 0u32;
        let mut fbo = 0u32;
        (self.gl.gen_textures)(1, &mut src_tex);
        (self.gl.gen_textures)(1, &mut dst_tex);
        (self.gl.gen_framebuffers)(1, &mut fbo);

        Some(Blit {
            prog,
            vbo,
            fbo,
            src_tex,
            dst_tex,
            dst_w: 0,
            dst_h: 0,
            a_pos,
            u_tex,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke-test the surfaceless EGL backend on the host GPU: it must
    /// initialize and report importable modifiers for the RGB fourccs we
    /// negotiate. Ignored by default (needs a GPU + libEGL); run with
    /// `cargo test -p azul-dll egl_init_and_query -- --ignored --nocapture`.
    #[test]
    #[ignore = "requires a GPU / libEGL; run explicitly"]
    fn egl_init_and_query() {
        let egl = EglBackend::init().expect("EGL backend failed to initialize");
        for (name, fourcc) in [
            ("XRGB8888", DRM_FORMAT_XRGB8888),
            ("ARGB8888", DRM_FORMAT_ARGB8888),
            ("ABGR8888", DRM_FORMAT_ABGR8888),
            ("XBGR8888", DRM_FORMAT_XBGR8888),
        ] {
            let mods = egl.query_modifiers(fourcc);
            eprintln!("{name} ({fourcc:#010x}): {} modifier(s): {:#x?}", mods.len(), mods);
            assert!(!mods.is_empty(), "{name} must offer at least MOD_INVALID");
        }
    }
}
