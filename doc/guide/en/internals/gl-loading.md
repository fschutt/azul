---
slug: gl-loading
title: GL Function Loading
language: en
canonical_slug: gl-loading
audience: contributor
maturity: wip
guide_order: null
topic_only: false
prerequisites: []
tracked_files:
  - core/src/gl.rs
  - core/src/gl_fxaa.rs
  - core/src/glconst.rs
  - dll/src/desktop/shell2/common/gl_loader.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T12:00:00Z
---

> **WIP** — `gl.rs` is still cleaning up its FFI surface
> (vestigial `run_destructor` flags, `static mut` texture cache).
> Public types are stable; private internals may shift.

`azul` does not link `libGL` at compile time. Each platform shell opens the
appropriate driver library at runtime and resolves every entry point of
`gl_context_loader::GenericGlContext` through one shared loader,
[`load_gl_context`](#the-shared-loader) in
`dll/src/desktop/shell2/common/gl_loader.rs`. The result is wrapped in
`Rc<GenericGlContext>` and handed to `core::gl::GlContextPtr::new`, which
compiles the three built-in shaders and stores them on `GlContextPtrInner`.

## Layered structure

| Layer | Crate / file | Responsibility |
|---|---|---|
| Constants table | `core/src/glconst.rs` | 1624 `pub const NAME: GLenum = 0xNNNN;` |
| Loader trampoline | `gl_context_loader::GenericGlContext` (external) | `*mut c_void` per GL entry point |
| Shared field-by-field assignment | `dll/src/desktop/shell2/common/gl_loader.rs` | Calls `get_func("glX")` for every field |
| Per-platform symbol resolution | `shell2/{linux/common, macos, windows}/gl.rs` | `eglGetProcAddress` / `dlsym` / `wglGetProcAddress` |
| Azul wrapper + shaders | `core/src/gl.rs` (`GlContextPtr`) | Refcounting, FFI types, built-in programs, texture cache |
| Constants re-export | `core/src/gl.rs:28` (`pub use crate::glconst::*`) | Single import surface for users |

## The shared loader

`load_gl_context` exists so each platform writes ~30 lines of resolution code
instead of ~800 field assignments:

```rust,ignore
pub fn load_gl_context(
    get_func: impl Fn(&str) -> *mut gl_context_loader::c_void,
) -> GenericGlContext {
    GenericGlContext {
        glAccum: get_func("glAccum"),
        glActiveTexture: get_func("glActiveTexture"),
        // ...one line per GL entry point
        glEndTilingQCOM: get_func("glEndTilingQCOM"),
    }
}
```

Source: `dll/src/desktop/shell2/common/gl_loader.rs:12-794`. The closure is
called once per symbol; missing symbols come back as null pointers. Callers
that try to invoke an unloaded symbol get a null-deref — `gl_context_loader`
does not gate at the wrapper level.

## Per-platform resolution

### macOS

`dll/src/desktop/shell2/macos/gl.rs:46-71`. Uses `dlopen` against the system
OpenGL framework, then `dlsym` per symbol:

```rust,ignore
let framework_path =
    CString::new("/System/Library/Frameworks/OpenGL.framework/OpenGL").unwrap();
let handle = unsafe { dlopen(framework_path.as_ptr(), RTLD_NOW | RTLD_GLOBAL) };
if handle.is_null() { return Err("Could not dlopen OpenGL.framework/OpenGL".to_string()); }

let context = load_gl_context(|s| {
    let c_string = CString::new(s).unwrap();
    unsafe { dlsym(handle, c_string.as_ptr()) } as *mut _
});
```

`dlclose` runs from `GlFunctions::Drop`. macOS deprecated OpenGL but still
ships the framework; once Metal-only systems drop OpenGL entirely, this loader
will need a translation layer (e.g. ANGLE on Metal).

### Linux (X11 + Wayland)

`dll/src/desktop/shell2/linux/common/gl.rs:32-55`. Two-stage resolution:
`eglGetProcAddress` first, falling back to `libGL.so.1` for pre-EGL symbols
that EGL refuses to vend:

```rust,ignore
let opengl_lib = Library::load("libGL.so.1").ok();

let context = load_gl_context(|s| {
    let symbol_name = CString::new(s).unwrap();
    let addr = unsafe { (egl.eglGetProcAddress)(symbol_name.as_ptr()) };
    if !addr.is_null() {
        return addr as *mut gl_context_loader::c_void;
    }
    if let Some(lib) = &opengl_lib {
        if let Ok(addr) = unsafe { lib.get_symbol::<*mut c_void>(s) } {
            return addr as *mut gl_context_loader::c_void;
        }
    }
    std::ptr::null_mut()
});
```

`Egl` itself is loaded via the dlopen plumbing in
`dll/src/desktop/shell2/linux/x11/dlopen.rs`. Both X11 and Wayland backends
share this loader because both create EGL contexts (X11 via `eglCreateWindow
Surface` on the X drawable, Wayland via the `EGL_KHR_platform_wayland`
platform).

### Windows

`dll/src/desktop/shell2/windows/gl.rs:67-83`. Two-stage resolution:
`wglGetProcAddress` for OpenGL ≥ 1.2 (extensions and modern functions),
`GetProcAddress` against `opengl32.dll` for the GL 1.0/1.1 base set that
`wglGetProcAddress` returns null for:

```rust,ignore
self.functions = Rc::new(load_gl_context(|s| {
    let mut func_name = super::encode_ascii(s);
    let addr1 = unsafe { wglGetProcAddress(func_name.as_mut_ptr() as *const i8) };
    (if addr1 != ptr::null_mut() {
        addr1
    } else if let Some(opengl32_dll) = opengl32_dll {
        unsafe { GetProcAddress(opengl32_dll, func_name.as_mut_ptr() as *const i8) }
    } else {
        addr1
    }) as *mut gl_context_loader::c_void
}));
```

The `ExtraWglFunctions` struct (`shell2/windows/gl.rs:99-235`) bootstraps WGL
extension functions (`wglCreateContextAttribsARB`, `wglChoosePixelFormatARB`,
`wglSwapIntervalEXT`) by opening a temporary dummy window and a transient
WGL context — the same chicken-and-egg dance as Khronos's reference loader.

### When to call `load`

Per platform, `load` must run **after** the GL context is current on the
current thread. `wglGetProcAddress`, `eglGetProcAddress`, and `dlsym` all
return symbols scoped to the active context. This is why the Windows shell
splits `initialize` (zero-init the function pointers, open `opengl32.dll`)
from `load` (call `load_gl_context` once a real context is current).

## Constants — `core/src/glconst.rs`

A flat 1624-entry table:

```rust,ignore
pub const ACCUM: types::GLenum = 0x0100;
pub const ACTIVE_TEXTURE: types::GLenum = 0x84E0;
// ...
pub const ZOOM_X: types::GLenum = 0x0D16;
```

Re-exported from `core/src/gl.rs:28` as `pub use crate::glconst::*`, so
downstream code uses `gl::TEXTURE_2D`, not `glconst::TEXTURE_2D`. The file is
gated by `#![allow(dead_code, non_upper_case_globals)]` (`glconst.rs:3`)
because most GL programs use a fraction of the table.

## `GlContextPtr` and the FFI surface

The Rust API surface lives on `core::gl::GlContextPtr`:

```rust,ignore
#[repr(C)]
pub struct GlContextPtr {
    pub ptr: Box<Rc<GlContextPtrInner>>,
    pub renderer_type: RendererType,
    pub run_destructor: bool,
}

#[repr(C)]
pub struct GlContextPtrInner {
    pub ptr: Rc<GenericGlContext>,
    pub svg_shader: GLuint,
    pub svg_multicolor_shader: GLuint,
    pub fxaa_shader: GLuint,
}
```

`gl.rs:858-900`. The double `Box<Rc<…>>` exists for two reasons:

1. `#[repr(C)]` requires a known size; `Rc` is `repr(Rust)`, so it's wrapped
   in a `Box` to expose a single pointer field across the FFI boundary.
2. Cloning bumps the inner `Rc` count; the outer `Box` is cheap to copy.

`Drop` on `GlContextPtrInner` (`gl.rs:902-908`) deletes all three programs.
`Drop` on `GlContextPtr` itself only flips `run_destructor = false`
(`gl.rs:876-880`); the actual cleanup is in `GlContextPtrInner::Drop` once the
last `Rc` is gone.

The `run_destructor` field appears on most `gl.rs` types
(`GlVoidPtrConst`, `GLsyncPtr`, `GlContextPtr`, `Texture`,
`VertexArrayObject`, `VertexBuffer`). It is a vestigial pattern: only `Texture`
and `VertexArrayObject` actually look at it before running cleanup
(`gl.rs:2846-2856`, `3079-3089`). The other `Drop` impls just clear the flag
without doing work — the field can be removed once the FFI codegen is updated
to stop emitting it. Treat it as historical noise when reading.

## Built-in shader compilation

`GlContextPtr::new(renderer_type, gl_context)` (`gl.rs:1027-1107`) compiles
three programs sequentially before returning. Each follows the same shape:

```rust,ignore
let vs = gl_context.create_shader(gl::VERTEX_SHADER);
gl_context.shader_source(vs, &[VERTEX_SOURCE]);
gl_context.compile_shader(vs);
check_shader_compile(&gl_context, vs, "label");

let fs = gl_context.create_shader(gl::FRAGMENT_SHADER);
gl_context.shader_source(fs, &[FRAGMENT_SOURCE]);
gl_context.compile_shader(fs);
check_shader_compile(&gl_context, fs, "label");

let program = gl_context.create_program();
gl_context.attach_shader(program, vs);
gl_context.attach_shader(program, fs);
gl_context.bind_attrib_location(program, 0, "vAttrXY".into());
gl_context.link_program(program);
check_program_link(&gl_context, program, "label");

gl_context.delete_shader(vs);
gl_context.delete_shader(fs);
```

The three resulting program IDs go into `GlContextPtrInner` and are read back
by `get_svg_shader()`, `get_fxaa_shader()`, etc. Compile / link errors print
to stderr via `eprintln!` but do not abort — a missing program degrades
silently (SVG paths render as solid black). For long-term hardening this
should be a Result.

| Program | Vertex source | Fragment source |
|---|---|---|
| `svg_shader` | `gl.rs:923-940` | `gl.rs:942-956` |
| `svg_multicolor_shader` | `gl.rs:958-978` | `gl.rs:980-…` |
| `fxaa_shader` | `gl_fxaa.rs:FXAA_VERTEX_SHADER` | `gl_fxaa.rs:FXAA_FRAGMENT_SHADER` |

## FXAA configuration

`core/src/gl_fxaa.rs` exposes `FxaaConfig`:

```rust,ignore
pub struct FxaaConfig {
    pub enabled: bool,
    pub edge_threshold: f32,      // 0.063 - 0.333, default 0.125
    pub edge_threshold_min: f32,  // 0.0312 - 0.0833, default 0.0312
}
```

Presets: `enabled()`, `high_quality()` (threshold 0.063), `balanced()` (default
threshold 0.125), `performance()` (threshold 0.25). Currently only
`FxaaConfig::enabled()` is wired up — it's read by
`layout/src/xml/svg.rs:apply_fxaa`. The FXAA pass runs after SVG triangulation
to soften aliased edges; `enabled = false` skips it entirely.

The shader algorithm is the standard NVIDIA FXAA 3.11 reference: sample the
4-neighbourhood luminance, early-exit if the contrast is below threshold,
otherwise blur along the detected edge direction
(`gl_fxaa.rs:FXAA_FRAGMENT_SHADER`). Tuning the `edge_threshold` trades
sharpness for AA coverage.

## Vertex layout abstraction

`gl.rs` includes a small VAO/VBO abstraction (`VertexLayout`, `VertexAttribute`,
`VertexAttributeType`, `VertexArrayObject`, `VertexBuffer`) used by the SVG
renderer. The interesting type is `VertexLayout::bind`
(`gl.rs:2921-2950`), which iterates the field list and calls
`vertex_attrib_pointer` + `enable_vertex_attrib_array` per attribute, computing
strides and offsets from `VertexAttribute::get_stride()`. There's no global
state; rebinding the layout is cheap and stateless from the Rust side.

`VertexAttributeType` carries five variants (`Float`, `Double`, `UnsignedByte`,
`UnsignedShort`, `UnsignedInt`) with `get_gl_id()` mapping each to the GL
enum. Adding a new attribute type requires extending both methods.

## Shader compilation as a public API

`GlShader::new(&GlContextPtr, &str, &str) -> Result<Self, GlShaderCreateError>`
(`gl.rs:3478`) is the user-facing path for custom shaders. It first checks
`gl::SHADER_COMPILER` to detect drivers that only accept binary shaders
(returns `NoShaderCompiler`), then runs the same compile/attach/link/check
sequence as the built-in shaders, returning structured errors:

```rust,ignore
pub enum GlShaderCreateError {
    Compile(GlShaderCompileError),  // Vertex(VertexShaderCompileError) | Fragment(...)
    Link(GlShaderLinkError),
    NoShaderCompiler,
}
```

`GlShader::Drop` (`gl.rs:3373-3377`) deletes the linked program. `GlShader`
holds a `GlContextPtr` clone so the program outlives the borrow that produced
it.

## Texture cache (process-global)

A process-global `static mut ACTIVE_GL_TEXTURES` lives at `gl.rs:733`. It maps
`DocumentId → Epoch → ExternalImageId → Texture` and exists so WebRender can
hold an `ExternalImageId` referencing a GL texture without taking ownership.

```rust,ignore
static mut ACTIVE_GL_TEXTURES:
    Option<OrderedMap<DocumentId, GlTextureStorage>> = None;
```

API: `insert_into_active_gl_textures` (`gl.rs:739`), `get_opengl_texture`
(`gl.rs:835`), `gl_textures_remove_epochs_from_pipeline` (`gl.rs:765`),
`gl_textures_remove_active_pipeline` (`gl.rs:808`),
`gl_textures_clear_opengl_cache` (`gl.rs:819`).

The `static mut` is **not thread-safe** and Rust 2024 will refuse the
reference style. The current rationale is that `Texture` itself is `!Send`
and lives on the renderer thread, so concurrent access does not arise in
practice — but this is an invariant the type system does not enforce. The
right long-term fix is `Mutex<Option<…>>` or a thread-local with documented
ownership.

## Putting it together

A simplified construction sequence on Linux:

```rust,ignore
// 1. shell2/linux/x11/dlopen.rs — dlopen libEGL.so.1, libwayland-client.so, etc.
let egl: Rc<Egl> = Egl::load()?;

// 2. shell2/linux/common/gl.rs — resolve every GL entry point.
let gl_funcs = GlFunctions::initialize(&egl)?;
let gl: Rc<GenericGlContext> = gl_funcs.functions.clone();

// 3. core/src/gl.rs — compile built-in shaders and wrap.
let ctx_ptr = GlContextPtr::new(RendererType::Hardware, gl);

// 4. WebRender / SVG renderer call methods on `ctx_ptr` from here on.
ctx_ptr.clear(gl::COLOR_BUFFER_BIT);
let prog = ctx_ptr.get_svg_shader();
```

macOS swaps step 1 for `dlopen("/System/Library/Frameworks/OpenGL.framework/
OpenGL")`; Windows swaps it for `LoadLibraryA("opengl32.dll")` plus the WGL
dummy-context dance in `ExtraWglFunctions::load`.

## Related pages

- [Rendering pipeline](rendering-pipeline.md) — what the GL context is used
  for once it's built.
- [WebRender bridge](webrender-bridge.md) — how textures registered through
  `ACTIVE_GL_TEXTURES` reach the WebRender display list as external images.
