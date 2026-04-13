# Review: dll/src/desktop/shell2/windows/dlopen.rs

## Summary
- Lines: 908
- Public functions: 3 (`encode_ascii`, `encode_wide`, `Win32Libraries::load`)
- Public structs/enums: 16 (`RECT`, `POINT`, `TRACKMOUSEEVENT`, `MSG`, `WNDCLASSW`, `COMPOSITIONFORM`, `DynamicLibrary`, `User32Functions`, `Gdi32Functions`, `BitmapInfoHeader`, `Imm32Functions`, `Shell32Functions`, `Kernel32Functions`, `DWM_SYSTEMBACKDROP_TYPE`, `MARGINS`, `DWM_BLURBEHIND`, `DwmapiFunctions`, `Win32Libraries`)
- Findings: 2 high, 1 medium, 0 low

## Findings

### [HIGH] Unsafe — `Win32Libraries::clone()` creates use-after-free risk
- **Location**: `dlopen.rs:877-895`
- **Details**: The manual `Clone` impl sets all `_dll` fields (`user32_dll`, `gdi32_dll`, etc.) to `None`, but copies function pointers. If the original `Win32Libraries` is dropped, `DynamicLibrary::drop` calls `FreeLibrary`, unloading the DLLs. The cloned copy then holds dangling function pointers. This is used at `mod.rs:3658` (`self.win32.clone()` for tooltip windows).
- **Evidence**: `mod.rs:3658` passes `self.win32.clone()` to `TooltipWindow::new`. If the parent window's `Win32Libraries` is dropped first, the tooltip's function pointers become invalid.
- **Recommendation**: Use `Arc<Win32Libraries>` sharing instead of this fragile clone pattern. Alternatively, use reference-counted DLL handles so the DLLs stay loaded as long as any clone exists.

### [HIGH] Duplicated functionality — `DynamicLibrary` trait in `common/dlopen.rs`
- **Location**: `dlopen.rs:283-327` vs `shell2/common/dlopen.rs:18-35`
- **Details**: There are two `DynamicLibrary` abstractions: a struct in this file and a trait in `shell2/common/dlopen.rs`. The common trait has a cleaner API with proper `DlError` types. The Windows implementation uses neither the common trait nor its error type, instead using `String` errors. The X11 and Wayland dlopen modules use the common trait.
- **Evidence**: `common/dlopen.rs:18` defines `pub trait DynamicLibrary` with `load`, `get_symbol`, `unload`. This file defines `pub struct DynamicLibrary` with the same methods but different signatures.
- **Recommendation**: Refactor Windows `DynamicLibrary` to implement the common `DynamicLibrary` trait from `shell2/common/dlopen.rs`, using `DlError` for consistency.

### [MEDIUM] Missing documentation — most type aliases and constants lack doc comments
- **Location**: `dlopen.rs:15-32`
- **Details**: 13 type aliases (`HINSTANCE`, `HWND`, `HDC`, `HGLRC`, `HMENU`, `HMONITOR`, `HICON`, `HCURSOR`, `HBRUSH`, `HDROP`, `HIMC`, `WPARAM`, `LPARAM`, `LRESULT`, `BOOL`, `UINT`, `HRESULT`, `ATOM`) have no doc comments. Only `HIMC` has an inline comment. These are re-exported Win32 types used throughout the Windows shell code.
- **Recommendation**: Add brief one-line doc comments, at least for the non-obvious ones (e.g., `HIMC` — "IME input context handle" is already noted inline but not as a doc comment).


## System Documentation
- System identified: yes — Windows windowing / platform shell (Win32 API layer)
- Existing doc: none (no `doc/guide/windowing.md` or similar)
- Doc needed: A windowing system guide covering the platform shell architecture (`shell2/`), how DLLs are loaded, the event loop, and window creation flow across Windows/X11/Wayland/macOS backends.
