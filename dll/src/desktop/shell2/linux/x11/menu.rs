//! X11 menu handling - delegates to unified menu system
//!
//! The unified menu system in `crate::desktop::menu` handles all platforms.
//! This module re-exports shared types for any platform-specific extensions.

pub(crate) use super::super::menu_common::{MenuLayoutData, menu_layout_callback};
