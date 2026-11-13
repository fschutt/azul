//! The theme marker class a themed widget carries on its root, and the
//! reverse lookup a live-restyling callback asks.
//!
//! The style builders that used to live here (`bg`, `themed_bg`, `border`,
//! `focus_shadow_ring`, ...) were twins of `themes::decl`'s and moved there,
//! one name per meaning (`DEDUP_WIDGETS_API` F4): `bg` -> `decl::fill`,
//! `themed_bg` / `hover_bg` / `active_bg` -> `decl::themed_fill` /
//! `hover_fill` / `active_fill`, `fill` -> `decl::fill_box`, `border` ->
//! `decl::themed_border`, `hover_border` -> `decl::hover_border_color`,
//! `drop_shadow` -> `decl::themed_shadow`, `inset_shadow` ->
//! `decl::themed_inset_shadow`, `focus_shadow_ring` ->
//! `decl::focus_halo_inset_stacked`, `focus_halo` ->
//! `decl::focus_halo_stacked`; every other builder kept its name.

use azul_core::dom::IdOrClass;
use azul_css::AzString;

use super::UiTheme;

// ---------------------------------------------------------------------------
// The theme marker
// ---------------------------------------------------------------------------

/// The class on the root of a widget the flat theme rendered.
pub const FLAT_CLASS: &str = "__azul-theme-flat";
/// The class on the root of a widget the flora theme rendered.
pub const FLORA_CLASS: &str = "__azul-theme-flora";

/// The marker class for `theme`, as the root's `IdOrClass`.
#[must_use]
pub const fn marker(theme: UiTheme) -> IdOrClass {
    IdOrClass::Class(AzString::from_const_str(match theme {
        UiTheme::Flat => FLAT_CLASS,
        UiTheme::Flora => FLORA_CLASS,
    }))
}

/// Which theme a node was rendered in, read back from its classes - what a
/// callback that live-restyles a widget asks, so the colours it writes are
/// the theme's the widget was BUILT in. No marker: the default theme.
#[must_use]
pub fn theme_of_classes(classes: &[AzString]) -> UiTheme {
    if classes.iter().any(|c| c.as_str() == FLORA_CLASS) {
        UiTheme::Flora
    } else {
        UiTheme::Flat
    }
}
