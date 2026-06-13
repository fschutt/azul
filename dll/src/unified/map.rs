//! Unified `map` widget helper. See [`crate::unified`].
//!
//! `map_widget_dom` is the only desktop-only entry point (it wires the
//! background tile-fetch worker behind the `map-tiles` feature). The
//! `MapWidget` type itself lives in `azul_layout` and resolves on every target,
//! so only this wrapper needs a target-stable home.

#[cfg(all(feature = "cabi_internal", not(target_arch = "wasm32")))]
pub use crate::desktop::extra::map::*;

/// wasm fallback: no tile-fetch worker, so render the placeholder DOM directly
/// (identical to the desktop `map-tiles`-off path).
#[cfg(target_arch = "wasm32")]
pub fn map_widget_dom(widget: azul_layout::widgets::map::MapWidget) -> azul_core::dom::Dom {
    widget.dom()
}
