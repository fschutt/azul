//! Client-side decorations for X11 windows.
//!
//! This module integrates the common CSD implementation with X11-specific
//! window management. The decorations are rendered using StyledDom and
//! event handling is done through the hit-test system.

use azul_core::{
    dom::Dom,
    window::{LinuxDecorationsState, WindowDecorations},
};

use super::{defines::*, X11Window};
use crate::desktop::shell2::linux::common::decorations::{
    create_decorations_dom, load_csd_stylesheet, wrap_with_decorations,
};

/// Checks if this window should use client-side decorations.
pub fn should_use_csd(window: &X11Window) -> bool {
    // Use CSD if decorations are set to None (meaning system shouldn't provide them)
    window.current_window_state.flags.decorations == WindowDecorations::None
}

/// Wraps the user's DOM with CSD elements if needed.
///
/// This is called after the window's layout callback to prepend
/// the title bar and menu bar to the user content.
pub fn apply_decorations_if_needed(window: &X11Window, user_dom: Dom) -> Dom {
    if !should_use_csd(window) {
        // System provides decorations, return user DOM as-is
        return user_dom;
    }

    // Get window title
    let title = window.current_window_state.title.as_str();

    // Check if window has menu
    let has_menu = false; // TODO: Check from window state

    // Create decorations and wrap user content
    let decorations = create_decorations_dom(title, has_menu);

    // Combine decorations with user content
    let mut container = Dom::div();

    container = container.with_child(decorations).with_child(user_dom);

    container
}

/// Handles X11 events that interact with decorations.
///
/// Returns true if the event was consumed by decorations.
pub fn handle_decoration_event(window: &mut X11Window, event: &XEvent) -> bool {
    if !should_use_csd(window) {
        return false;
    }

    // TODO: Implement event handling through hit-test system
    // - Title bar drag for window movement
    // - Button clicks (minimize, maximize, close)
    // - Hover states for visual feedback

    false
}

/// Gets or creates the decorations state for this window.
fn get_decorations_state(window: &X11Window) -> LinuxDecorationsState {
    window
        .current_window_state
        .platform_specific_options
        .linux_options
        .x11_decorations_state
        .into_option()
        .unwrap_or_default()
}

/// Updates the decorations state for this window.
fn set_decorations_state(window: &mut X11Window, state: LinuxDecorationsState) {
    window
        .current_window_state
        .platform_specific_options
        .linux_options
        .x11_decorations_state = Some(state).into();
}
