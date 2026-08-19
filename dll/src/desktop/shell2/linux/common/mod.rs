//! Common code shared between Linux windowing backends (X11, Wayland).
//!
//! Key exports: [`gl::GlFunctions`] and [`compose::ComposeSequencer`], both
//! consumed by the X11 and Wayland backends.

pub mod compose;
pub mod gl;
