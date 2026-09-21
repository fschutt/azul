//! Client-Side Decorations (CSD) - Custom Window Titlebar
//!
//! This module provides automatic titlebar generation for frameless windows.
//! When `WindowFlags::has_decorations` is enabled, a custom titlebar with
//! window controls (close, minimize, maximize) is automatically injected
//! into the user's DOM.
//!
//! All button/drag logic lives in [`azul_layout::widgets::titlebar::Titlebar`].
//! This module is just the integration layer that:
//!   1. Decides *whether* to inject a titlebar (`should_inject_csd`)
//!   2. Creates a `Titlebar` from the live `SystemStyle`
//!   3. Styles it with the CSD stylesheet (`SystemStyle::create_csd_stylesheet`)
//!
//! The software menu bar is injected separately (for all decoration modes) in
//! `shell2::common::layout::regenerate_layout`, so by the time CSD runs it is
//! already part of the user DOM.

use azul_core::{dom::Dom, styled_dom::StyledDom, window::WindowDecorations};
use azul_css::system::SystemStyle;
use azul_layout::widgets::titlebar::Titlebar;

// ── Titlebar creation (delegates to Titlebar) ────────────────────────

/// Create a CSD titlebar `StyledDom` with window controls using `SystemStyle`.
///
/// Builds a [`Titlebar`] in full-CSD mode (`dom_with_buttons`),
/// then styles it with the CSD stylesheet from `SystemStyle`.
pub(crate) fn create_titlebar_styled_dom(title: &str, system_style: &SystemStyle) -> StyledDom {
    let tm = &system_style.metrics.titlebar;

    let titlebar = Titlebar::from_system_style_csd(title.into(), system_style);

    let mut dom = titlebar.dom_with_buttons(&tm.buttons, tm.button_side);
    let css = system_style.create_csd_stylesheet();
    StyledDom::create(&mut dom, css)
}

// NOTE: the software menu bar is no longer injected here. It is injected once,
// for all decoration modes, in `shell2::common::layout::regenerate_layout`
// (Linux-only, gated on the absence of a native global menu) via the shared
// `azul_layout::widgets::menubar` widget — see `inject_software_menubar`. By the
// time CSD runs, the menu bar already lives inside the user DOM (below where the
// titlebar is added), so there is nothing to do here.

// ── Public helpers ───────────────────────────────────────────────────────

/// Check if CSD should be injected for a window.
///
/// CSD is injected when:
/// 1. `has_decorations` flag is true, AND
/// 2. `decorations` is set to `None` (frameless window)
#[inline]
pub(crate) fn should_inject_csd(has_decorations: bool, decorations: WindowDecorations) -> bool {
    // MWA-C-csd: never inject a DESKTOP titlebar on mobile — iOS/Android
    // share regenerate_layout, and decorations==None there would otherwise
    // grow close/min/max buttons and a drag bar that make no sense on a
    // phone (windows are fullscreen surfaces; the OS owns system chrome).
    if cfg!(any(target_os = "ios", target_os = "android")) {
        return false;
    }
    has_decorations && decorations == WindowDecorations::None
}

/// Does this platform grow its own title-only bar for
/// [`WindowDecorations::NoTitleAutoInject`]?
///
/// `NoTitleAutoInject` means "native controls visible, native title hidden,
/// app draws its own title". That needs a frame showing window controls
/// WITHOUT a title — which only macOS provides (traffic lights over a
/// title-less bar). Windows (`WS_CAPTION`) and Linux (KWin/Mutter server-side
/// decorations, X11 WM decorations) always draw the full caption including the
/// title text, so a software bar there is a second, fake titlebar under the
/// real one. Mobile has no window to title at all.
#[inline]
pub(crate) const fn auto_injects_software_titlebar() -> bool {
    !cfg!(any(
        target_os = "windows",
        target_os = "linux",
        target_os = "android",
        target_os = "ios"
    ))
}

/// Can this platform's own frame show window CONTROLS without a title?
///
/// That is exactly what [`WindowDecorations::NoTitle`] asks for. macOS can
/// (traffic lights over a title-less bar) and Windows keeps its caption
/// buttons; X11's Motif hints are all-or-nothing per element - `NoTitle`
/// drops the whole title bar, and with it minimise, maximise and close - and
/// Wayland's xdg-decoration has one bit for the entire frame. On Linux the
/// window therefore comes up with no way to close it, which is why the
/// controls are drawn in software there. Mobile has no window controls to
/// begin with, so nothing is owed.
#[inline]
pub(crate) const fn frame_shows_controls_without_title() -> bool {
    !cfg!(target_os = "linux")
}

/// What the shell prepends above the user's DOM for a given set of window
/// decoration flags.
///
/// This is the DOM-shape consequence of the flags, and it is why the type
/// exists at all: `regenerate_layout` has to be able to ask "would this window
/// build a DIFFERENT tree now?" *before* it decides to skip the rebuild.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CsdInjection {
    /// The user's DOM is the window's DOM.
    None,
    /// Full CSD: a titlebar carrying close/minimise/maximise controls.
    Titlebar,
    /// A title-only software bar (no controls) — `NoTitleAutoInject`.
    SoftwareTitleOnly,
    /// The window CONTROLS alone, overlaid on the user's DOM — `NoTitle` on a
    /// platform whose frame cannot show controls without a title.
    ControlsOnly,
}

/// The injection the given flags call for. Single source of truth: the
/// injection site in `regenerate_layout` and the "did it change" precheck
/// guarding the pre-cascade skip both read THIS, so they cannot drift.
#[inline]
pub(crate) fn csd_injection_for(
    has_decorations: bool,
    decorations: WindowDecorations,
) -> CsdInjection {
    if should_inject_csd(has_decorations, decorations) {
        CsdInjection::Titlebar
    } else if decorations == WindowDecorations::NoTitleAutoInject
        && auto_injects_software_titlebar()
    {
        CsdInjection::SoftwareTitleOnly
    } else if decorations == WindowDecorations::NoTitle && !frame_shows_controls_without_title() {
        CsdInjection::ControlsOnly
    } else {
        CsdInjection::None
    }
}

/// Would a rebuild under `new` produce a differently-shaped tree than the one
/// built under `old`?
///
/// A decoration flip is invisible to the app's DOM — the layout callback
/// returns the same nodes either way — so nothing downstream of the callback
/// can notice it. Only the flags can.
#[inline]
pub(crate) fn csd_injection_changed(
    old_has_decorations: bool,
    old_decorations: WindowDecorations,
    new_has_decorations: bool,
    new_decorations: WindowDecorations,
) -> bool {
    csd_injection_for(old_has_decorations, old_decorations)
        != csd_injection_for(new_has_decorations, new_decorations)
}

/// Inject CSD titlebar and/or menu into user's DOM.
///
/// Creates a container `StyledDom` and appends:
/// 1. Titlebar with close/min/max buttons (via [`Titlebar::dom_with_buttons`])
/// 2. User's content DOM (which already includes the software menu bar, if any — that is injected
///    earlier in `regenerate_layout`, before this runs)
/// The window controls alone, to be overlaid on the user's DOM.
///
/// No title node, so it claims no width for one; the CSD stylesheet pins it
/// to the frame's corner over the app's own chrome.
pub(crate) fn create_controls_only_styled_dom(system_style: &SystemStyle) -> StyledDom {
    let tm = &system_style.metrics.titlebar;
    let titlebar = Titlebar::from_system_style_csd(String::new().into(), system_style);
    let mut dom = titlebar.dom_controls_only(&tm.buttons, tm.button_side);
    let css = system_style.create_csd_stylesheet();
    StyledDom::create(&mut dom, css)
}

/// Overlay the window controls on a `NoTitle` window whose frame cannot draw
/// them. The user's DOM keeps the whole window: the controls sit ON it, not
/// above it, because `NoTitle` promised the app the full client area.
pub(crate) fn overlay_window_controls(
    user_dom: StyledDom,
    system_style: &SystemStyle,
) -> StyledDom {
    let mut container_dom = Dom::create_html();
    let mut container_styled = StyledDom::create(&mut container_dom, azul_css::css::Css::empty());
    container_styled.append_child(user_dom);
    container_styled.append_child(create_controls_only_styled_dom(system_style));
    container_styled
}

pub(crate) fn wrap_user_dom_with_decorations(
    user_dom: StyledDom,
    window_title: &str,
    should_inject_titlebar: bool,
    system_style: &SystemStyle,
) -> StyledDom {
    // Nothing to add if no titlebar is wanted (the menu bar, if present, is
    // already inside `user_dom`).
    if !should_inject_titlebar {
        return user_dom;
    }

    // Use an Html root so we don't get double <body> nesting.
    let mut container_dom = Dom::create_html();
    let mut container_styled = StyledDom::create(&mut container_dom, azul_css::css::Css::empty());

    let titlebar_styled = create_titlebar_styled_dom(window_title, system_style);
    container_styled.append_child(titlebar_styled);

    // Append user's content (which carries the menu bar below the titlebar).
    container_styled.append_child(user_dom);

    container_styled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_inject_csd() {
        assert!(should_inject_csd(true, WindowDecorations::None));
        assert!(!should_inject_csd(false, WindowDecorations::None));
        assert!(!should_inject_csd(true, WindowDecorations::Normal));
        assert!(!should_inject_csd(true, WindowDecorations::NoTitle));
        assert!(!should_inject_csd(true, WindowDecorations::NoControls));
    }

    #[test]
    fn the_injection_follows_the_flags() {
        // Mobile owns the chrome: nothing is ever prepended there.
        let mobile = cfg!(any(target_os = "ios", target_os = "android"));

        assert_eq!(
            csd_injection_for(true, WindowDecorations::Normal),
            CsdInjection::None,
            "a server-decorated window adds nothing"
        );
        assert_eq!(
            csd_injection_for(true, WindowDecorations::None),
            if mobile {
                CsdInjection::None
            } else {
                CsdInjection::Titlebar
            },
            "frameless + has_decorations is the full CSD titlebar"
        );
        assert_eq!(
            csd_injection_for(false, WindowDecorations::None),
            CsdInjection::None,
            "frameless WITHOUT has_decorations is a bare surface by request"
        );
        assert_eq!(
            csd_injection_for(true, WindowDecorations::NoTitleAutoInject),
            if auto_injects_software_titlebar() {
                CsdInjection::SoftwareTitleOnly
            } else {
                CsdInjection::None
            },
        );
    }

    /// The predicate the pre-cascade skip consults. A decoration flip that
    /// reshapes the tree MUST be visible here — it is the only evidence the
    /// shell has, because the app's DOM is identical across the flip.
    #[test]
    fn a_reshaping_decoration_flip_is_visible() {
        use WindowDecorations::{NoTitle, NoTitleAutoInject, Normal, None as NoDeco};

        // Nothing moved.
        assert!(!csd_injection_changed(true, Normal, true, Normal));
        assert!(!csd_injection_changed(true, NoDeco, true, NoDeco));
        // A title-bar-less mode swapped for another title-bar-less mode.
        assert!(!csd_injection_changed(
            true,
            Normal,
            true,
            WindowDecorations::NoControls
        ));

        // THE BUG: the compositor refused server-side decorations, the shell
        // flipped the window to frameless+CSD, and the tree now grows a
        // titlebar it did not have.
        let flip_to_csd = csd_injection_changed(true, Normal, true, NoDeco);
        #[cfg(any(target_os = "ios", target_os = "android"))]
        assert!(!flip_to_csd, "mobile never grows a desktop titlebar");
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        assert!(
            flip_to_csd,
            "Normal -> None+has_decorations grows the CSD titlebar"
        );

        // …and the way back, which drops it again.
        assert_eq!(
            csd_injection_changed(true, NoDeco, true, Normal),
            flip_to_csd
        );

        // NoTitle -> NoTitleAutoInject reshapes wherever the two modes ask
        // for different chrome: macOS grows the software title-only bar, and
        // Linux DROPS the controls overlay `NoTitle` is owed there (its frame
        // cannot show controls without a title, `NoTitleAutoInject`'s can).
        let auto_inject_flip = csd_injection_changed(true, NoTitle, true, NoTitleAutoInject);
        #[cfg(target_os = "linux")]
        assert!(
            auto_inject_flip,
            "NoTitle carries a software controls overlay on Linux; NoTitleAutoInject does not"
        );
        #[cfg(any(target_os = "windows", target_os = "android", target_os = "ios"))]
        assert!(
            !auto_inject_flip,
            "the native caption already draws the title here — no software bar, no reshape"
        );
        #[cfg(not(any(
            target_os = "windows",
            target_os = "linux",
            target_os = "android",
            target_os = "ios"
        )))]
        assert!(
            auto_inject_flip,
            "macOS auto-injects the title-only bar, so the tree changes"
        );
    }
}

#[cfg(test)]
mod controls_only_tests {
    //! `WindowDecorations::NoTitle` promises "no title text, controls still
    //! visible". On Linux the frame cannot give half of itself: X11's Motif
    //! hints drop the whole title bar and with it minimise, maximise and
    //! close, and Wayland's xdg-decoration has one bit for the entire frame.
    //! Such a window came up with NO WAY TO CLOSE IT.

    use azul_core::window::WindowDecorations;

    use super::{csd_injection_for, frame_shows_controls_without_title, CsdInjection};

    #[test]
    fn a_no_title_window_gets_its_controls_drawn_where_the_frame_cannot() {
        let expected = if frame_shows_controls_without_title() {
            // macOS draws traffic lights over a title-less bar; Windows keeps
            // its caption buttons. Nothing is owed.
            CsdInjection::None
        } else {
            CsdInjection::ControlsOnly
        };
        assert_eq!(csd_injection_for(true, WindowDecorations::NoTitle), expected);
        // The promise is about the FRAME, so it holds whether or not the app
        // asked for decorations to be drawn by us.
        assert_eq!(
            csd_injection_for(false, WindowDecorations::NoTitle),
            expected
        );
    }

    #[test]
    fn linux_is_the_platform_that_cannot() {
        assert_eq!(
            frame_shows_controls_without_title(),
            !cfg!(target_os = "linux")
        );
    }

    #[test]
    fn no_other_decoration_mode_grows_a_controls_overlay() {
        for d in [
            WindowDecorations::Normal,
            WindowDecorations::NoControls,
            WindowDecorations::None,
            WindowDecorations::NoTitleAutoInject,
        ] {
            assert_ne!(
                csd_injection_for(true, d),
                CsdInjection::ControlsOnly,
                "{d:?} asked for no such thing"
            );
        }
    }
}
