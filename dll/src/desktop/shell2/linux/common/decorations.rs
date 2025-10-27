//! Shared Client-Side Decorations (CSD) for Linux backends.
//!
//! Renders window decorations (title bar, buttons, menu bar) using a
//! system-provided stylesheet for theming.

use azul_core::{dom::Dom, window::LinuxDecorationsState};

/// Creates the decorations DOM structure (title bar, buttons, menu).
///
/// This creates a simple structure that will later be styled with CSS.
/// After the window's layout callback is invoked, this DOM is prepended
/// to create the complete window with decorations.
pub fn create_decorations_dom(window_title: &str, has_menu: bool) -> Dom {
    // For now, return a simple placeholder DOM
    // TODO: Build proper DOM structure with IDs for styling:
    // - Title bar with ID "csd-titlebar"
    // - Title text with ID "csd-title"
    // - Buttons container with ID "csd-buttons"
    // - Individual buttons with IDs "csd-minimize", "csd-maximize", "csd-close"
    // - Optional menu bar with ID "csd-menubar"

    Dom::div() // Placeholder for CSD structure
}

/// Loads the system-specific CSD stylesheet.
///
/// On Linux, this should load:
/// - GTK theme if using X11/Wayland
/// - Custom theme if specified in config
/// - Fallback default theme
pub fn load_csd_stylesheet() -> String {
    // TODO: Load from system theme
    // For now, return a minimal default stylesheet
    create_default_csd_stylesheet()
}

/// Creates a default CSD stylesheet as fallback.
fn create_default_csd_stylesheet() -> String {
    r#"
        #csd-titlebar {
            width: 100%;
            height: 30px;
            background: #2d2d2d;
            flex-direction: row;
            align-items: center;
            justify-content: space-between;
        }
        
        #csd-title {
            flex-grow: 1;
            padding-left: 10px;
            color: #dcdcdc;
            font-size: 12px;
        }
        
        #csd-buttons {
            flex-direction: row;
        }
        
        #csd-button {
            width: 45px;
            height: 30px;
            background: #323232;
            align-items: center;
            justify-content: center;
            color: #dcdcdc;
            font-size: 16px;
        }
        
        #csd-button:hover {
            background: #464646;
        }
        
        #csd-close:hover {
            background: #dc4646;
        }
        
        #csd-menubar {
            width: 100%;
            height: 25px;
            background: #252525;
            flex-direction: row;
            align-items: center;
        }
    "#
    .to_string()
}

/// Wraps user content with decorations.
///
/// This is called after the window's layout callback to prepend
/// the title bar and menu to the user's DOM.
pub fn wrap_with_decorations(user_dom: Dom, window_title: &str, has_menu: bool) -> Dom {
    let decorations = create_decorations_dom(window_title, has_menu);

    // Create container that holds both decorations and user content
    let mut container = Dom::div();

    container = container.with_child(decorations).with_child(user_dom);

    container
}

// -- Event Handlers --
// These will be registered through the hit-test system later
