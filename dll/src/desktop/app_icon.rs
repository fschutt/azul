//! Application / dock / taskbar icon, set at runtime from an icon-registry spec.
//!
//! Same pipeline as the tray: a spec resolves through the icon registry, renders
//! as a DOM, and comes out as RGBA — so an app icon can be any registered icon,
//! including one a custom resolver expresses as an SVG or an emoji.
//!
//! # What "app icon" means per platform, and what it does NOT mean
//!
//! The parts that shape this API:
//!
//! * **The Windows EXE icon cannot be changed at runtime.** `BeginUpdateResource`
//!   documents that the target "cannot be currently executing", and rewriting
//!   resources invalidates Authenticode anyway. What IS settable is the *window*
//!   icon (title bar, Alt+Tab, and the taskbar button of a running, un-pinned
//!   window). A pinned taskbar entry shows the shortcut's icon and is not ours.
//! * **macOS windows have no icon** beyond the document proxy icon, which only
//!   exists when the window represents a file. So `set_window_icon` is a no-op
//!   there by design rather than being faked through the proxy-icon button.
//! * **macOS `applicationIconImage` is explicitly temporary** — Apple's word.
//!   It is process-local and resets on next launch; persisting it requires an
//!   `NSDockTilePlugIn`, which is banned from the App Store. This does not touch
//!   the bundle: `NSWorkspace.setIcon:forFile:` would, and it breaks the code
//!   signature ("app is damaged" on Ventura+), so it is deliberately not offered.
//! * **Wayland cannot set a window icon from pixels** unless the compositor
//!   implements `xdg-toplevel-icon-v1` — which Mutter/GNOME still does not in
//!   2026. The fallback is `set_app_id` + a matching `.desktop` file, i.e. no
//!   runtime pixels at all.
//!
//! Everything here therefore reports what it actually did rather than returning
//! `()`, so a caller can tell "set" from "silently nothing" — which is exactly
//! how the Wayland generic-icon bug survives in other toolkits.

/// What a platform managed to do with an icon request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconOutcome {
    /// The icon is now showing.
    Applied,
    /// The platform has no such slot at all (a macOS window icon, a Wayland
    /// window icon with no `xdg-toplevel-icon-v1`). Not an error.
    Unsupported,
    /// The spec resolved to nothing, or no icon registry has been published.
    NotFound,
}

/// Set the application icon — the macOS Dock tile, and on other platforms the
/// process-wide default window icon.
///
/// `spec` is an icon-registry spec, exactly as an `<icon>` node takes.
pub fn set_app_icon(
    spec: &str,
    provider: &azul_core::icon::SharedIconProvider,
    font_manager: &azul_layout::font_traits::FontManager<azul_css::props::basic::FontRef>,
) -> IconOutcome {
    #[cfg(target_os = "macos")]
    {
        macos::set_app_icon(spec, provider, font_manager)
    }
    #[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
    {
        linux::set_app_icon(spec, provider, font_manager)
    }
    #[cfg(not(any(
        target_os = "macos",
        all(target_os = "linux", not(target_arch = "wasm32"))
    )))]
    {
        let _ = (spec, provider, font_manager);
        IconOutcome::Unsupported
    }
}

/// Set the Dock/taskbar badge — macOS `NSDockTile.badgeLabel`.
///
/// `None` clears it. Windows' equivalent is `ITaskbarList3::SetOverlayIcon`
/// (an icon, not a string) and Linux's is a de-facto D-Bus interface; neither
/// is wired yet, so both report `Unsupported`.
pub fn set_badge(label: Option<&str>) -> IconOutcome {
    #[cfg(target_os = "macos")]
    {
        macos::set_badge(label)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = label;
        IconOutcome::Unsupported
    }
}

#[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
pub(crate) use linux::default_window_icons;

#[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
mod linux {
    use std::sync::{Arc, Mutex};

    use super::IconOutcome;

    /// The rendered app icon, kept for windows created AFTER `set_app_icon`.
    /// `(width, height, RGBA8)` per size. X11 window creation reads this when
    /// the window options carry no icon of their own.
    static DEFAULT_ICONS: Mutex<Option<Arc<Vec<(u32, u32, Vec<u8>)>>>> = Mutex::new(None);

    /// The process-default window icon (set via `App::set_app_icon`), for the
    /// X11 backend to apply at window creation.
    pub(crate) fn default_window_icons() -> Option<Arc<Vec<(u32, u32, Vec<u8>)>>> {
        DEFAULT_ICONS.lock().ok().and_then(|g| g.clone())
    }

    /// X11: render the spec at the sizes `_NET_WM_ICON` consumers actually
    /// use and set it on every live window + park it for future ones. The WM
    /// picks the size (titlebar wants 16-32, Alt-Tab / taskbar previews want
    /// 48-128).
    ///
    /// Wayland windows are left alone: without `xdg-toplevel-icon-v1` (which
    /// KWin < 6.1 and all Mutter lack) there is no pixel path at all — the
    /// icon comes from the `.desktop` file matched by `app_id`, and
    /// pretending otherwise is how the generic-icon bug survives in other
    /// toolkits. The outcome reflects what actually happened.
    pub(super) fn set_app_icon(
        spec: &str,
        provider: &azul_core::icon::SharedIconProvider,
        font_manager: &azul_layout::font_traits::FontManager<azul_css::props::basic::FontRef>,
    ) -> IconOutcome {
        let mut icons: Vec<(u32, u32, Vec<u8>)> = Vec::new();
        for size in [16u32, 32, 48, 128] {
            if let Some(r) =
                crate::desktop::tray::render_named_icon(spec, size, provider, font_manager)
            {
                icons.push((r.width, r.height, r.rgba));
            }
        }
        if icons.is_empty() {
            return IconOutcome::NotFound;
        }
        let icons = Arc::new(icons);
        if let Ok(mut g) = DEFAULT_ICONS.lock() {
            *g = Some(icons.clone());
        }

        // Apply to every live X11 window now; count what actually took it.
        let mut applied = 0usize;
        let mut saw_wayland = false;
        for id in crate::desktop::shell2::linux::registry::get_all_window_ids() {
            let Some(ptr) = (unsafe { crate::desktop::shell2::linux::registry::get_window(id) })
            else {
                continue;
            };
            match unsafe { &mut *ptr } {
                crate::desktop::shell2::linux::LinuxWindow::X11(w) => {
                    w.apply_app_icon(&icons);
                    applied += 1;
                }
                crate::desktop::shell2::linux::LinuxWindow::Wayland(_) => {
                    saw_wayland = true;
                }
            }
        }
        if applied > 0 || !saw_wayland {
            // Applied to live windows, or parked for the windows to come on
            // an X11 session that has none open yet.
            IconOutcome::Applied
        } else {
            IconOutcome::Unsupported
        }
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    use objc2_foundation::NSString;

    use super::IconOutcome;

    /// Rendered once at 1024 so the Dock never has to upscale. `NSImage.size` is
    /// set in POINTS by `nsimage_from_rgba`, and the pixel/point ratio is the
    /// scale factor, so one large representation covers every display.
    ///
    /// Mission Control and the About panel ask for larger sizes than the 128pt
    /// Dock tile, which is why this is not simply 256.
    const APP_ICON_PIXELS: u32 = 1024;
    const APP_ICON_POINTS: u32 = 512;

    pub(super) fn set_app_icon(
        spec: &str,
        provider: &azul_core::icon::SharedIconProvider,
        font_manager: &azul_layout::font_traits::FontManager<azul_css::props::basic::FontRef>,
    ) -> IconOutcome {
        let Some(mtm) = MainThreadMarker::new() else {
            return IconOutcome::Unsupported;
        };
        let Some(rendered) =
            crate::desktop::tray::render_named_icon(spec, APP_ICON_PIXELS, provider, font_manager)
        else {
            return IconOutcome::NotFound;
        };
        let Some(image) = crate::desktop::tray::macos::nsimage_from_rgba(
            &rendered.rgba,
            rendered.width,
            rendered.height,
            APP_ICON_POINTS,
            mtm,
        ) else {
            return IconOutcome::NotFound;
        };

        let app = NSApplication::sharedApplication(mtm);
        // NOT a template image, unlike the status item: the Dock tile is drawn
        // in full colour and is never tinted by AppKit.
        //
        // Note macOS 26 enforces a squircle for BUNDLE icons but draws a runtime
        // applicationIconImage exactly as supplied — so a caller wanting the
        // native look has to draw the rounded square itself. We do not impose one.
        unsafe { app.setApplicationIconImage(Some(&image)) };
        IconOutcome::Applied
    }

    pub(super) fn set_badge(label: Option<&str>) -> IconOutcome {
        let Some(mtm) = MainThreadMarker::new() else {
            return IconOutcome::Unsupported;
        };
        let app = NSApplication::sharedApplication(mtm);
        let tile = unsafe { app.dockTile() };
        unsafe {
            match label {
                Some(l) => tile.setBadgeLabel(Some(&NSString::from_str(l))),
                None => tile.setBadgeLabel(None),
            }
            // MANDATORY. NSDockTile does not redraw itself: "In order to
            // initiate drawing in the view, you must call -[NSDockTile
            // display]". Omitting this is the single most common "my dock icon
            // doesn't update" cause.
            tile.display();
        }
        IconOutcome::Applied
    }
}
