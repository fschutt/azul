---
slug: gl-loading
title: GL Function Loading
language: en
canonical_slug: gl-loading
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: Loading GL function pointers across platforms
prerequisites: []
tracked_files:
  - core/src/gl.rs
  - core/src/glconst.rs
  - dll/src/desktop/shell2/common/gl_loader.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
---

> **WIP** — `GenericGlContext` may move to a slimmer subset; the rest of the
> loader interface is stable.

OpenGL is loaded at runtime, never linked. Each platform shell opens the system
GL library, hands a symbol-resolution closure to `load_gl_context`, and gets
back a `GenericGlContext` with ~800 function pointers populated. WebRender,
the SVG renderer, and the FXAA pass all run against that single struct.

The shared loader lives at
`dll/src/desktop/shell2/common/gl_loader.rs::load_gl_context`:

```rust,ignore
pub fn load_gl_context(
    get_func: impl Fn(&str) -> *mut gl_context_loader::c_void,
) -> GenericGlContext {
    GenericGlContext {
        glAccum: get_func("glAccum"),
        glActiveTexture: get_func("glActiveTexture"),
        // ...~800 more entries
        glEndTilingQCOM: get_func("glEndTilingQCOM"),
    }
}
```

The struct definition is in the `gl_context_loader` crate. Every field is a raw
function pointer (`*mut c_void`) cast at the call site. Missing symbols stay
null; calling them is undefined behaviour, so the platform layer must verify
the GL version before calling extension-only functions.

## Per-platform resolvers

Each platform owns one file that constructs a `GlFunctions` wrapper around the
`Rc<GenericGlContext>` and a handle to whatever needs to stay alive (DLL,
framework, or `Library`).

### Linux — dll/src/desktop/shell2/linux/common/gl.rs

```rust,ignore
pub fn initialize(egl: &Rc<Egl>) -> Result<Self, String> {
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
    // ...
}
```

`eglGetProcAddress` is the primary path — it works for all GL/GLES extensions
and modern entry points. `libGL.so.1` is the fallback for legacy 1.x core
symbols that some EGL implementations don't expose through `eglGetProcAddress`.
Used by both the X11 and Wayland backends.

### macOS — dll/src/desktop/shell2/macos/gl.rs

```rust,ignore
pub fn initialize() -> Result<Self, String> {
    const RTLD_NOW: i32 = 2;
    const RTLD_GLOBAL: i32 = 8;
    let framework_path = CString::new(
        "/System/Library/Frameworks/OpenGL.framework/OpenGL"
    ).unwrap();
    let handle = unsafe { dlopen(framework_path.as_ptr(), RTLD_NOW | RTLD_GLOBAL) };
    if handle.is_null() {
        return Err("Could not dlopen OpenGL.framework/OpenGL".to_string());
    }

    let context = load_gl_context(|s| {
        let c_string = CString::new(s).unwrap();
        (unsafe { dlsym(handle, c_string.as_ptr()) }) as *mut _
    });
    // ...
}
```

There is no `eglGetProcAddress` on macOS; every symbol comes from a single
`dlopen` of the framework binary. The hardcoded path bypasses dyld's search
logic — the framework lives at a fixed location on every supported macOS
version. `dlclose` runs in `Drop`.

The framework is deprecated by Apple but still present on every macOS release.
Migration to Metal would replace this entire file (and the WebRender backend),
not just the loader.

### Windows — dll/src/desktop/shell2/windows/gl.rs

```rust,ignore
pub fn load(&mut self) {
    let opengl32_dll = self._opengl32_dll_handle;
    self.functions = Rc::new(load_gl_context(|s| {
        use winapi::um::{
            libloaderapi::GetProcAddress,
            wingdi::wglGetProcAddress,
        };
        let mut func_name = super::encode_ascii(s);
        let addr1 = unsafe {
            wglGetProcAddress(func_name.as_mut_ptr() as *const i8)
        };
        (if addr1 != ptr::null_mut() {
            addr1
        } else if let Some(opengl32_dll) = opengl32_dll {
            unsafe {
                GetProcAddress(opengl32_dll, func_name.as_mut_ptr() as *const i8)
            }
        } else {
            addr1
        }) as *mut gl_context_loader::c_void
    }));
}
```rust

`wglGetProcAddress` only resolves entry points beyond OpenGL 1.1 — the 1.1
core (`glClear`, `glViewport`, etc.) must come from `opengl32.dll` directly.
That's why the DLL handle is loaded eagerly in `initialize()` and the loader
falls back to `GetProcAddress` on it. Any function that returns null after
both paths is unavailable on this driver.

`load()` is split from `initialize()` because `wglGetProcAddress` requires a
**current** WGL context. The platform shell creates a dummy context, calls
`load()`, then destroys the dummy context.

## Initialization order

1. The platform shell creates the native GL surface (GLX FBConfig, NSOpenGLContext, or
   WGL pixel format + dummy context).
2. The shell makes that context current.
3. The shell calls `GlFunctions::initialize(...)` (or `.load()` on Windows),
   which calls `load_gl_context` to resolve every entry point.
4. The shell hands the resulting `Rc<GenericGlContext>` to
   `core/src/gl.rs::GlContextPtr::new`, which compiles the SVG and FXAA shader
   programs and stores the program IDs in `GlContextPtrInner`.
5. WebRender is initialized against the same `Rc<GenericGlContext>`.

If any of steps 1 through 3 are out of order (particularly making the context
current *after* loading on Windows), the resulting `GenericGlContext` is full
of nulls. Every subsequent call returns a black frame or crashes in WebRender.

## Why one giant struct

`GenericGlContext` sits in the `gl_context_loader` crate (third-party fork)
and exposes every GL entry point as a public field. Three reasons it's shaped
this way:

- **No global state.** Static GL bindings (the typical C/C++ pattern) require
  a single context per process. Azul wants per-window contexts and headless
  test contexts to coexist; one struct per context is the only design that
  scales.
- **Ad-hoc context substitution.** Tests and `cpurender` swap a real
  `GenericGlContext` for a stub by constructing the struct directly with
  `mem::zeroed()` (Windows takes this path during `initialize()` before the
  real context is current).
- **Lifetimes are obvious.** `Rc<GenericGlContext>` is the GL context. When
  the last refcount drops, the struct is freed and so is every cached pointer
  — no orphaned function pointers that outlive their library.

`GlContextPtr` wraps the `Rc` and adds the compiled SVG and FXAA program IDs.
The wrapper is `repr(C)` and exposed across the FFI boundary. The compiled
shaders live alongside the function pointers because they share the same
lifetime.

- `core/src/gl.rs::GlContextPtr`

## glconst.rs

`core/src/glconst.rs` declares 1624 OpenGL enum constants — every named
`GL_*` value the implementation may need. The file is purely declarative:

```rust,ignore
pub const ACTIVE_ATTRIBUTES: types::GLenum = 0x8B89;
pub const ACTIVE_ATTRIBUTE_MAX_LENGTH: types::GLenum = 0x8B8A;
// ...
```

`pub use crate::glconst::*` in `core/src/gl.rs` re-exports them so call sites
use `gl::TEXTURE_2D` and similar without further qualification. The constants
are GL-spec-defined. If you need a value that isn't here, look it up in the
OpenGL registry and add a single `pub const` line.

## What's not in GenericGlContext

A few functions the renderer needs are *not* in `GenericGlContext`:

- **WGL extensions.** `wglCreateContextAttribsARB`, `wglSwapIntervalEXT`, and
  `wglChoosePixelFormatARB`. Loaded into a separate `ExtraWglFunctions` struct
  in `dll/src/desktop/shell2/windows/gl.rs::ExtraWglFunctions` via a dummy
  context.
- **GLX/EGL setup.** `eglGetDisplay`, `eglCreateContext`, and friends. Loaded
  directly through the EGL `Library` (X11 and Wayland) before any GL function
  pointer exists.
- **Platform context-creation primitives.** `wglMakeCurrent`,
  `[NSOpenGLContext makeCurrentContext]`, `eglMakeCurrent`. Called by the
  platform shell, never by `core` or `layout`.

Anything else missing is a bug in `gl_context_loader`. File a PR there rather
than working around it in the loader closure.

## Failure modes

- **All draws produce a black frame.** The loader closure ran without a
  current GL context. On Windows, `load()` was called before `wglMakeCurrent`.
- **Some draws work, others crash on entry.** The driver doesn't expose that
  entry point. Check for null in the consuming code, or guard with
  `GlApiVersion`.
- **Linux backend fails entirely.** `libGL.so.1` isn't installed (the `mesa`
  or `nvidia-driver` package is missing). EGL alone cannot resolve every
  legacy symbol.
- **macOS backend fails to dlopen.** The OpenGL framework was removed. Rare;
  no shipping macOS version has done this.
- **Windows backend works in dev but fails in installer.** `opengl32.dll`
  isn't on the load path of the installed binary, or the installer renamed
  the DLL.

The Linux fallback to `libGL.so.1` is the only path that swallows a missing
symbol silently. The others either return null (per-symbol failure) or refuse
to construct the loader at all (whole-context failure). If a single function
pointer is null and you call it, the program segfaults. There is no runtime
guard. The OpenGL ES tile-control entries (`glStartTilingQCOM`,
`glEndTilingQCOM`) are present in the struct for completeness but only
populate on Adreno-class mobile GPUs. Desktop drivers leave them null.

## Coming Up Next

- [Rendering Pipeline](rendering-pipeline.md) — From `StyledDom` to pixels
- [WebRender Bridge](webrender-bridge.md) — How azul talks to WebRender
- [Image Pipeline](image-pipeline.md) — Decoding, caching, and uploading raster images
