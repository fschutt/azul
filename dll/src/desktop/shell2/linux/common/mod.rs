//! Common code shared between Linux windowing backends (X11, Wayland).
//!
//! Key export: [`gl::GlFunctions`], consumed by both the X11 and Wayland backends.

pub mod gl;
