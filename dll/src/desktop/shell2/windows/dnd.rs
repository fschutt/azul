//! Windows file drag-and-drop via the OLE `IDropTarget` COM interface.
//!
//! This replaces the legacy `DragAcceptFiles` / `WM_DROPFILES` path (which only
//! delivered the final *drop*, no hover). `IDropTarget` gives us the full
//! sequence — `DragEnter`/`DragOver` (hover), `DragLeave` (cancel) and `Drop` —
//! which maps onto the cross-platform `FileDropManager`
//! (`set_hovered_file`/`set_dropped_file`) exactly like the macOS
//! `NSDraggingDestination` delegate (item 1).
//!
//! Flow:
//!   - `register_drag_drop(hwnd)` (run loop, after the window is in the global
//!     registry) calls `OleInitialize` once + `RegisterDragDrop`.
//!   - `revoke_drag_drop(hwnd)` (`WM_DESTROY`, before the HWND dies) calls
//!     `RevokeDragDrop`.
//!   - The COM methods resolve the `Win32Window` from the HWND via the registry,
//!     call the matching `handle_file_*` method, then route the result through
//!     `route_main_window_result` so a callback-driven restyle repaints.
//!
//! The `windows` crate is metadata-only, so this whole module cross-compiles
//! cleanly from a non-Windows host (`--target x86_64-pc-windows-msvc`).

use std::sync::Once;

use windows::{
    core::{implement, Ref},
    Win32::{
        Foundation::{HWND, POINTL},
        System::{
            Com::{IDataObject, FORMATETC, DVASPECT_CONTENT, TYMED_HGLOBAL},
            Ole::{
                OleInitialize, RegisterDragDrop, ReleaseStgMedium, RevokeDragDrop, CF_HDROP,
                DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_NONE, IDropTarget, IDropTarget_Impl,
            },
            SystemServices::MODIFIERKEYS_FLAGS,
        },
        UI::Shell::{DragQueryFileW, HDROP},
    },
};

use super::{dlopen, registry, Win32Window};

/// Ensure the calling thread is OLE-initialised into an STA exactly once.
///
/// `RegisterDragDrop` requires an STA — using `CoInitialize`(Ex) with MTA, or
/// not initialising at all, makes it fail with `E_OUTOFMEMORY`.
fn ensure_ole_initialized() {
    static OLE_INIT: Once = Once::new();
    OLE_INIT.call_once(|| unsafe {
        // `None` reserved arg. Ignore the HRESULT: `S_FALSE` means already
        // initialised on this thread, which is fine.
        let _ = OleInitialize(None);
    });
}

/// Register `hwnd` as an OLE drop target. Idempotent-safe to call once per
/// window at creation. The COM object owns its own lifetime via the ref that
/// `RegisterDragDrop` takes (`AddRef`); we drop our local handle afterwards.
pub fn register_drag_drop(hwnd: dlopen::HWND) {
    ensure_ole_initialized();
    let target: IDropTarget = FileDropTarget { hwnd }.into();
    unsafe {
        if let Err(e) = RegisterDragDrop(HWND(hwnd), &target) {
            crate::log_warn!(
                crate::desktop::shell2::common::debug_server::LogCategory::Window,
                "[Win32] RegisterDragDrop failed: {e:?}"
            );
        }
    }
    // `target` is dropped here; the COM ref held by RegisterDragDrop keeps the
    // object alive until `RevokeDragDrop`.
}

/// Revoke the OLE drop target for `hwnd`. Must run while the HWND is still
/// alive (call from `WM_DESTROY`).
pub fn revoke_drag_drop(hwnd: dlopen::HWND) {
    unsafe {
        let _ = RevokeDragDrop(HWND(hwnd));
    }
}

/// Extract the dropped file paths from a data object (CF_HDROP / HGLOBAL).
/// Returns an empty vec if the data object does not carry files.
fn extract_paths(data: &IDataObject) -> Vec<String> {
    let format = FORMATETC {
        cfFormat: CF_HDROP.0,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };

    let mut medium = match unsafe { data.GetData(&format) } {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };

    // For CF_HDROP the medium is an HGLOBAL whose handle is itself the HDROP.
    let hglobal = unsafe { medium.u.hGlobal };
    let hdrop = HDROP(hglobal.0);

    let mut paths = Vec::new();
    if !hdrop.0.is_null() {
        // 0xFFFF_FFFF -> return the file count.
        let count = unsafe { DragQueryFileW(hdrop, 0xFFFF_FFFF, None) };
        for i in 0..count {
            let len = unsafe { DragQueryFileW(hdrop, i, None) };
            if len == 0 {
                continue;
            }
            // +1 for the NUL terminator the API writes.
            let mut buffer = vec![0u16; (len + 1) as usize];
            let written = unsafe { DragQueryFileW(hdrop, i, Some(&mut buffer)) };
            paths.push(String::from_utf16_lossy(&buffer[..written as usize]));
        }
    }

    unsafe {
        ReleaseStgMedium(&mut medium);
    }

    paths
}

/// Resolve the `Win32Window` for `hwnd` from the global registry and run `f`,
/// routing the resulting `ProcessEventResult` through the window's normal
/// repaint path.
fn with_window<F>(hwnd: dlopen::HWND, f: F)
where
    F: FnOnce(&mut Win32Window) -> azul_core::events::ProcessEventResult,
{
    if let Some(window_ptr) = registry::get_window(hwnd) {
        // SAFETY: the registry holds the live `Box::into_raw` pointer for the
        // window; the WndProc callbacks deref it the same way.
        let window: &mut Win32Window = unsafe { &mut *window_ptr };
        let result = f(window);
        window.route_main_window_result(hwnd, result);
    }
}

/// The OLE drop target COM object. Holds the raw HWND and resolves the owning
/// `Win32Window` lazily from the registry on each callback.
#[implement(IDropTarget)]
struct FileDropTarget {
    hwnd: dlopen::HWND,
}

#[allow(non_snake_case)]
impl IDropTarget_Impl for FileDropTarget_Impl {
    fn DragEnter(
        &self,
        pdataobj: Ref<IDataObject>,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        _pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let paths = pdataobj.as_ref().map(extract_paths).unwrap_or_default();
        let accept = !paths.is_empty();
        if accept {
            with_window(self.hwnd, |w| w.handle_file_drag_entered(paths));
        }
        if !pdweffect.is_null() {
            unsafe {
                *pdweffect = if accept { DROPEFFECT_COPY } else { DROPEFFECT_NONE };
            }
        }
        Ok(())
    }

    fn DragOver(
        &self,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        _pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        // We don't have the data object here; the hover state is already set
        // from DragEnter. Just keep advertising the copy effect.
        if !pdweffect.is_null() {
            unsafe {
                *pdweffect = DROPEFFECT_COPY;
            }
        }
        Ok(())
    }

    fn DragLeave(&self) -> windows::core::Result<()> {
        with_window(self.hwnd, |w| w.handle_file_drag_exited());
        Ok(())
    }

    fn Drop(
        &self,
        pdataobj: Ref<IDataObject>,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        _pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let paths = pdataobj.as_ref().map(extract_paths).unwrap_or_default();
        let accept = !paths.is_empty();
        if accept {
            with_window(self.hwnd, |w| w.handle_file_drop(paths));
        }
        if !pdweffect.is_null() {
            unsafe {
                *pdweffect = if accept { DROPEFFECT_COPY } else { DROPEFFECT_NONE };
            }
        }
        Ok(())
    }
}
