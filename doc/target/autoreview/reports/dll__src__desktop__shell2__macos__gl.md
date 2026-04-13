# Review: dll/src/desktop/shell2/macos/gl.rs

## Summary
- Lines: 873
- Public functions: 2 (`initialize`, `get_context`)
- Public structs/enums: 1 (`GlFunctions`)
- Findings: 0 high, 1 medium, 0 low

## Findings

### [MEDIUM] Duplicated GL function loading across platforms

- **Location**: `macos/gl.rs`, `windows/gl.rs`, `linux/common/gl.rs`
- **Details**: All three files contain ~750 lines of repetitive `get_func("glFoo", handle)` calls to populate the same `GenericGlContext` struct. The only difference is the function-resolution mechanism: `dlsym` (macOS), `wglGetProcAddress`/`GetProcAddress` (Windows), `eglGetProcAddress` (Linux).
- **Recommendation**: This is somewhat inherent to platform-specific loading, but a macro or code-generation approach (e.g. generating the field list from a single source) could eliminate the duplication. Low priority since the list is stable and auto-generated-looking.

## System Documentation
- System identified: yes — OpenGL rendering / GPU backend (windowing system)
- Existing doc: `doc/guide/lifecycle.md` and `doc/guide/architecture.md` touch on rendering, but no dedicated rendering-pipeline or OpenGL-backend guide exists.
- Doc needed: A `doc/guide/rendering.md` covering the rendering pipeline (OpenGL setup, function loading, context creation, frame lifecycle) across platforms would be valuable. Multiple files (`macos/gl.rs`, `windows/gl.rs`, `linux/common/gl.rs`, `wr_translate2.rs`) belong to this undocumented system.
