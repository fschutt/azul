# Review: dll/src/desktop/shell2/windows/gl.rs

## Summary
- Lines: 1118
- Public functions: 3 (`GlFunctions::initialize`, `GlFunctions::load`, `ExtraWglFunctions::load`)
- Public structs/enums: 3 (`GlFunctions`, `ExtraWglFunctions`, `ExtraWglFunctionsLoadError`)
- Findings: 1 high, 1 medium, 0 low

## Findings

### [HIGH] Unsafe — `mem::zeroed()` on `GenericGlContext`
- **Location**: `gl.rs:52`
- **Details**: `GenericGlContext` is zero-initialized via `unsafe { mem::zeroed() }`. All fields are function pointers (`*mut c_void`). While calling any of these null pointers would crash, the `initialize()` / `load()` two-phase pattern means the struct is held in a state where all function pointers are null between construction and the `load()` call. Any accidental use of the `functions` field in that window would segfault.
- **Evidence**: `gl.rs:52` creates zeroed context; `mod.rs:268` calls `initialize()`, and `mod.rs:305` calls `load()` — there is a gap where the context is live but all pointers are null.
- **Recommendation**: Consider combining `initialize` and `load` into a single constructor that returns a fully-loaded `GlFunctions`, or mark `initialize` as `pub(crate)` / private with a doc warning.

### [MEDIUM] Unsafe — `mem::transmute` of WGL function pointers
- **Location**: `gl.rs:1080–1082`, `gl.rs:1095`, `gl.rs:1106`
- **Details**: `wglGetProcAddress` returns `PROC` (a `*mut c_void`-like type) which is then `mem::transmute`d to specific function pointer types. While this is the standard pattern for WGL, the function signatures must exactly match the actual WGL extension signatures. The signatures at lines 986–989 appear correct for `wglCreateContextAttribsARB`, `wglSwapIntervalEXT`, and `wglChoosePixelFormatARB`. However, `mem::transmute` is fragile — a signature mismatch would cause UB with no compile-time error.
- **Recommendation**: Consider adding `// Safety:` comments documenting that each function pointer signature matches the WGL extension specification. This is a documentation concern more than a bug.

## System Documentation
- System identified: yes — Windows OpenGL / WGL context initialization, part of the windowing system
- Existing doc: none (no windowing guide in `doc/guide/`)
- Doc needed: A windowing system guide covering the platform-specific GL context setup across Windows/X11/Wayland/macOS would be valuable. This file would be one component of that documentation.
