//! Application / dock / taskbar icon, set at runtime from an icon-registry spec.
//!
//! Same pipeline as the tray: a spec resolves through the icon registry, renders
//! as a DOM, and comes out as RGBA — so an app icon can be any registered icon,
//! including one a custom resolver expresses as an SVG or an emoji.
//!
//! # What "app icon" means per platform, and what it does NOT mean
//!
//! Full detail in `scripts/APP_AND_WINDOW_ICON_RESEARCH_2026_08_24.md`. The
//! parts that shape this API:
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
    #[cfg(not(target_os = "macos"))]
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
        let Some(rendered) = crate::desktop::tray::render_named_icon(spec, APP_ICON_PIXELS, provider, font_manager) else {
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
