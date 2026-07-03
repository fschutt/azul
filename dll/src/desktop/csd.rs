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
pub(crate) fn create_titlebar_styled_dom(
    title: &str,
    system_style: &SystemStyle,
) -> StyledDom {
    let tm = &system_style.metrics.titlebar;

    let titlebar = Titlebar::from_system_style_csd(
        title.into(),
        system_style,
    );

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
pub fn should_inject_csd(has_decorations: bool, decorations: WindowDecorations) -> bool {
    // MWA-C-csd: never inject a DESKTOP titlebar on mobile — iOS/Android
    // share regenerate_layout, and decorations==None there would otherwise
    // grow close/min/max buttons and a drag bar that make no sense on a
    // phone (windows are fullscreen surfaces; the OS owns system chrome).
    if cfg!(any(target_os = "ios", target_os = "android")) {
        return false;
    }
    has_decorations && decorations == WindowDecorations::None
}

/// Inject CSD titlebar and/or menu into user's DOM.
///
/// Creates a container `StyledDom` and appends:
/// 1. Titlebar with close/min/max buttons (via [`Titlebar::dom_with_buttons`])
/// 2. User's content DOM (which already includes the software menu bar, if any —
///    that is injected earlier in `regenerate_layout`, before this runs)
pub fn wrap_user_dom_with_decorations(
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
}
