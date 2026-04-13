//! Wayland menu handling via popup windows
//!
//! This module provides menu popup functionality for Wayland. It returns
//! `WindowCreateOptions` for a generic popup window; xdg_popup integration
//! is pending.
//!
//! Architecture:
//! - Menu data (Menu struct) is passed as RefAny to the layout callback
//! - Events are handled through normal Azul callback system
//! - Rendering goes through the standard menu_renderer / WebRender pipeline

pub(crate) use super::super::menu_common::{MenuLayoutData, menu_layout_callback};
