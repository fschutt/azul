//! Unified `map` widget helper. See [`crate::unified`].
//!
//! `map_widget_dom` is the only desktop-only entry point (it wires the
//! background tile-fetch worker behind the `map-tiles` feature). The
//! `MapWidget` type itself lives in `azul_layout` and resolves on every target,
//! so only this wrapper needs a target-stable home.

#[cfg(az_desktop)]
pub use crate::desktop::extra::map::*;

/// wasm fallback: no tile-fetch worker, so render the placeholder DOM directly
/// (identical to the desktop `map-tiles`-off path).
#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
pub fn map_widget_dom(widget: azul_layout::widgets::map::MapWidget) -> azul_core::dom::Dom {
    widget.dom()
}
