//! Windows system tray — `Shell_NotifyIconW`. **NOT IMPLEMENTED YET.**
//!
//! This stub exists so the branch cross-compiles while the backend is written;
//! it reports the tray as unavailable rather than pretending to work.
//!
//! # What this needs, in order
//!
//! 1. **A hidden top-level window — NOT `HWND_MESSAGE`.** Message-only windows
//!    are children of `HWND_MESSAGE`, therefore not top-level, therefore
//!    invisible to BROADCAST messages — and `TaskbarCreated` is a broadcast, so
//!    the icon would never come back after an Explorer restart. Use a normal
//!    `CreateWindowExW` with `WS_EX_TOOLWINDOW | WS_OVERLAPPED`, no
//!    `WS_VISIBLE`, and simply never `ShowWindow` it.
//!
//! 2. **`Shell_NotifyIconW` in `Shell32Functions`** (`windows/dlopen.rs:623`).
//!    shell32 is already dlopen'd there for drag-and-drop, on an
//!    optional/graceful-degradation path, so this fits without new machinery.
//!    `NOTIFYICONDATAW` has to be declared locally (winapi's `shellapi` feature
//!    is not enabled and enabling it is not worth it for one struct).
//!
//! 3. **`NIM_ADD` then `NIM_SETVERSION` with `NOTIFYICON_VERSION_4`** — and
//!    `NIM_SETVERSION` must be re-sent after EVERY `NIM_ADD`, including the
//!    re-add on `TaskbarCreated`. Without v4 you get no cursor coordinates and
//!    no `WM_CONTEXTMENU` for keyboard invocation.
//!
//! 4. **v4 message decoding — note the swap.** `LOWORD(lParam)` is the event,
//!    `HIWORD(lParam)` the icon id (16-bit only, so keep `uID` small), and
//!    `GET_X_LPARAM(wParam)`/`GET_Y_LPARAM(wParam)` are SCREEN coordinates.
//!    Reading these off the wrong parameter is the classic v4 bug. Map
//!    `NIN_SELECT`/`NIN_KEYSELECT` -> Activate, `WM_CONTEXTMENU` (0x007B) ->
//!    ContextMenu, `WM_MBUTTONUP` -> SecondaryActivate.
//!
//! 5. **`RegisterWindowMessage("TaskbarCreated")`** in `WM_CREATE`, re-add on
//!    receipt. It also fires when the primary display's DPI changes on Win10,
//!    so it doubles as the cue to rebuild the `HICON`.
//!
//! 6. **RGBA -> `HICON`**: `BITMAPV5HEADER` with `bV5Height = -h` (top-down),
//!    `BI_BITFIELDS`, `bV5AlphaMask = 0xff000000`, `CreateDIBSection`, copy
//!    RGBA->BGRA **without premultiplying** (`CreateIconIndirect` wants STRAIGHT
//!    alpha — the "GDI always premultiplies" rule is for `AlphaBlend`/layered
//!    windows, not icons), plus a 1bpp AND mask that is actually COMPUTED
//!    (`bit = alpha < 128`, where 1 means transparent). An all-zero mask is
//!    invisible in the normal draw path and ugly wherever `DI_MASK` is used.
//!    `CreateIconIndirect` COPIES both bitmaps, so `DeleteObject` both after,
//!    and neither may be selected into a DC at call time.
//!
//! 7. **Context menu**: reuse `WindowsMenuBar::recursive_construct_menu` with
//!    `CreatePopupMenu`, then `SetForegroundWindow(hwnd)` before
//!    `TrackPopupMenu` (or the menu will not dismiss on an outside click) and
//!    `PostMessage(hwnd, WM_NULL, 0, 0)` after it returns (or the NEXT
//!    invocation flashes open and closes). `windows/mod.rs:5755` already has
//!    the deferred-`TrackPopupMenu` half of this.
//!
//! 8. **Ownership**: we own the `HICON`; `NIM_ADD` does not take it.
//!    `DestroyIcon` only after `NIM_MODIFY`/`NIM_DELETE` has replaced it.
//!
//! Every symbol needed is already dlopen'd except `Shell_NotifyIconW`,
//! `RegisterWindowMessageW`, `CreateIconIndirect` and `DestroyIcon`
//! (`CreatePopupMenu`, `TrackPopupMenu`, `SetForegroundWindow`,
//! `CreateWindowExW`, `PostMessageW`, `CreateDIBSection` are all present).

use azul_core::tray::TrayIconData;

use super::TrayError;

/// Windows always has a notification area, but this backend does not exist
/// yet, so claiming availability would be a lie that produces a silently
/// missing icon.
pub(super) fn is_available() -> bool {
    false
}

#[derive(Debug)]
pub(super) struct PlatformTray {
    _never: core::convert::Infallible,
}

impl PlatformTray {
    pub(super) fn new(
        _data: &TrayIconData,
        _provider: &azul_core::icon::SharedIconProvider,
        _font_manager: &azul_layout::font_traits::FontManager<azul_css::props::basic::FontRef>,
    ) -> Result<Self, TrayError> {
        Err(TrayError::Unsupported)
    }

    pub(super) fn update(
        &mut self,
        _old: &TrayIconData,
        _new: &TrayIconData,
        _provider: &azul_core::icon::SharedIconProvider,
        _font_manager: &azul_layout::font_traits::FontManager<azul_css::props::basic::FontRef>,
    ) -> Result<(), TrayError> {
        match self._never {}
    }
}

impl PlatformTray {
    /// Unreachable — `new()` never returns a value on this platform yet.
    pub(super) fn pump(&mut self) -> Vec<azul_core::menu::CoreMenuCallback> {
        // No menu backend yet, so nothing can be delivered.
        Vec::new()
    }

    #[allow(dead_code)]
    fn pump_unused(&mut self) {
        match self._never {}
    }
}
