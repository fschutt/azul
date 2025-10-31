//! Client-Side Decorations (CSD) - Custom Window Titlebar
//!
//! This module provides automatic titlebar generation for frameless windows.
//! When `WindowFlags::has_decorations` is enabled, a custom titlebar with
//! window controls (close, minimize, maximize) is automatically injected
//! into the user's DOM.

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClassVec},
    menu::{Menu, MenuItem}, // Import Menu and MenuItem from menu module
    refany::RefAny,
    styled_dom::StyledDom,
    window::{WindowDecorations, WindowFrame},
};
use azul_css::{parser2::CssApiWrapper, system::SystemStyle};
use azul_layout::callbacks::CallbackInfo;

use crate::desktop::menu_renderer::SystemStyleMenuExt; // Import trait for menu stylesheet

// =============================================================================
// CSD Button Callbacks
// =============================================================================

/// Callback for the minimize button - minimizes the window
extern "C" fn csd_minimize_callback(_data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let mut flags = info.get_current_window_flags();
    flags.frame = WindowFrame::Minimized;
    info.set_window_flags(flags);
    eprintln!("[CSD Callback] Minimize button clicked - minimizing window");
    Update::DoNothing
}

/// Callback for the maximize button - toggles between maximized and normal
extern "C" fn csd_maximize_callback(_data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let mut flags = info.get_current_window_flags();
    flags.frame = if flags.frame == WindowFrame::Maximized {
        WindowFrame::Normal
    } else {
        WindowFrame::Maximized
    };
    info.set_window_flags(flags);
    eprintln!("[CSD Callback] Maximize button clicked - toggling maximize state");
    Update::DoNothing
}

/// Callback for the close button - requests window close
extern "C" fn csd_close_callback(_data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let mut flags = info.get_current_window_flags();
    flags.close_requested = true;
    info.set_window_flags(flags);
    eprintln!("[CSD Callback] Close button clicked - requesting window close");
    Update::DoNothing
}

/// Callback for menu bar items - shows dropdown menu below the item
extern "C" fn csd_menubar_item_callback(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    use std::sync::Arc;

    use azul_core::geom::LogicalPosition;

    // Data contains the Menu to show
    let menu = match data.downcast_ref::<Menu>() {
        Some(m) => m.clone(),
        None => {
            eprintln!("[CSD Menu] Failed to downcast menu data");
            return Update::DoNothing;
        }
    };

    eprintln!("[CSD Menu] Menu bar item clicked, creating popup menu");

    // Get system style Arc from CallbackInfo (safe clone)
    let system_style = info.get_system_style();

    // Get parent window position
    let window_state = info.get_current_window_state();
    let parent_pos = match window_state.position {
        azul_core::window::WindowPosition::Initialized(pos) => {
            LogicalPosition::new(pos.x as f32, pos.y as f32)
        }
        _ => LogicalPosition::new(0.0, 0.0),
    };

    // Get the clicked node's rectangle (the menu bar item)
    let trigger_rect = match info.get_hit_node_rect() {
        Some(rect) => rect,
        None => {
            eprintln!("[CSD Menu] No hit node rect available");
            return Update::DoNothing;
        }
    };

    // Create menu window positioned below the menu bar item
    let menu_options = crate::desktop::menu::show_menu(
        menu,
        system_style.clone(),
        parent_pos,
        Some(trigger_rect),
        None, // No cursor position for menu bar menus
        None, // No parent menu
    );

    // Create the menu window
    info.create_window(menu_options);

    Update::DoNothing
}

/// Callback for titlebar drag - updates window position based on mouse movement
/// Marker type to indicate titlebar drag area
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TitlebarDragMarker;

/// Callback for titlebar drag start - activates window dragging
///
/// This is called on DragStart event. The callback data contains a TitlebarDragMarker
/// which signals to the event system that this drag should activate window dragging.
extern "C" fn csd_titlebar_drag_start_callback(
    _data: &mut RefAny,
    info: &mut CallbackInfo,
) -> Update {
    // Signal that window drag should be activated by returning special flag
    // The actual activation happens in the event processing loop where we have mutable access

    eprintln!("[CSD Callback] DragStart on titlebar - requesting window drag activation");

    // We use a special Update variant to signal window drag activation
    // For now, just DoNothing - the auto-activation logic will handle it
    Update::DoNothing
}

/// Callback for titlebar dragging - uses GestureAndDragManager
///
/// This callback handles the Drag event (fired continuously during drag).
/// The window position is updated based on the drag delta from the gesture manager.
///
/// Since dragging works via the gesture manager, it continues to work even
/// when the mouse leaves the window (as long as the button is held down).
extern "C" fn csd_titlebar_drag_callback(_data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    // Get gesture manager to check drag state
    let gesture_manager = info.get_gesture_drag_manager();

    // Update window position from active drag
    if let Some(new_position) = gesture_manager.get_window_position_from_drag() {
        let mut window_state = info.get_current_window_state();
        window_state.position = new_position;
        info.set_window_state(window_state);

        // No logging during drag to avoid spam
    }

    Update::DoNothing
}

/// Callback for titlebar double-click - toggles maximize/restore
extern "C" fn csd_titlebar_doubleclick_callback(
    _data: &mut RefAny,
    info: &mut CallbackInfo,
) -> Update {
    use azul_core::window::WindowFrame;

    let mut flags = info.get_current_window_flags();
    flags.frame = if flags.frame == WindowFrame::Maximized {
        WindowFrame::Normal
    } else {
        WindowFrame::Maximized
    };
    info.set_window_flags(flags);
    eprintln!("[CSD Callback] Titlebar double-click - toggling maximize state");
    Update::DoNothing
}

/// Create a CSD titlebar StyledDom with window controls using SystemStyle
///
/// This generates a styled titlebar with:
/// - Window title text (centered or left-aligned)
/// - Close button (optional)
/// - Minimize button (optional)
/// - Maximize/Restore button (optional)
///
/// # Arguments
/// * `title` - Window title text
/// * `has_minimize` - Show minimize button
/// * `has_maximize` - Show maximize button
/// * `has_close` - Show close button
/// * `system_style` - System style for native look and feel
///
/// # Returns
/// StyledDom tree for the titlebar with CSS applied
pub fn create_titlebar_styled_dom(
    title: &str,
    has_minimize: bool,
    has_maximize: bool,
    has_close: bool,
    system_style: &SystemStyle,
) -> StyledDom {
    // Create DOM structure
    let mut dom = create_titlebar_dom(title, has_minimize, has_maximize, has_close);

    // Create stylesheet from SystemStyle
    let stylesheet = system_style.create_csd_stylesheet();

    // Wrap in Css struct (Css contains Vec<Stylesheet>)
    let css = azul_css::css::Css::new(vec![stylesheet]);

    // Apply stylesheet to DOM
    dom.style(CssApiWrapper { css })
}

/// Create a CSD menu bar StyledDom with menu items and callbacks
///
/// This generates a menu bar that displays top-level menu items horizontally.
/// Each item has a click handler that opens the corresponding submenu.
///
/// # Arguments
/// * `menu` - The menu structure to render
/// * `system_style` - System style for native look
///
/// # Returns
/// Styled DOM for the menu bar
fn create_menubar_styled_dom(menu: &Menu, system_style: &SystemStyle) -> StyledDom {
    let mut menu_items = Vec::new();

    for item in menu.items.as_slice().iter() {
        if let MenuItem::String(string_item) = item {
            let item_classes =
                IdOrClassVec::from_vec(vec![IdOrClass::Class("csd-menubar-item".into())]);

            let submenu = Menu::new(string_item.children.clone());

            let dom_item = Dom::div()
                .with_ids_and_classes(item_classes)
                .with_child(Dom::text(string_item.label.as_str()))
                .with_callbacks(
                    vec![CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::MouseDown),
                        callback: CoreCallback {
                            cb: csd_menubar_item_callback as usize,
                        },
                        data: RefAny::new(submenu),
                    }]
                    .into(),
                );

            menu_items.push(dom_item);
        }
    }

    let menubar_classes = IdOrClassVec::from_vec(vec![IdOrClass::Class("csd-menubar".into())]);
    let mut dom = Dom::div()
        .with_ids_and_classes(menubar_classes)
        .with_children(DomVec::from_vec(menu_items));

    let stylesheet = system_style.create_menu_stylesheet();
    let css = azul_css::css::Css::new(vec![stylesheet]);

    dom.style(CssApiWrapper { css })
}

/// Create a CSD titlebar DOM with window controls (internal helper)
///
/// This generates the DOM structure for a titlebar with:
/// - Window title text (centered or left-aligned)
/// - Close button (optional)
/// - Minimize button (optional)
/// - Maximize/Restore button (optional)
///
/// # Arguments
/// * `title` - Window title text
/// * `has_minimize` - Show minimize button
/// * `has_maximize` - Show maximize button
/// * `has_close` - Show close button
///
/// # Returns
/// DOM tree for the titlebar (unstyled)
fn create_titlebar_dom(
    title: &str,
    has_minimize: bool,
    has_maximize: bool,
    has_close: bool,
) -> Dom {
    let mut buttons = Vec::new();

    // Minimize button
    if has_minimize {
        let classes = IdOrClassVec::from_vec(vec![
            IdOrClass::Id("csd-button-minimize".into()),
            IdOrClass::Class("csd-button".into()),
            IdOrClass::Class("csd-minimize".into()),
        ]);
        let minimize_btn = Dom::div()
            .with_ids_and_classes(classes)
            .with_child(Dom::text("−"))
            .with_callbacks(
                vec![CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::MouseDown),
                    callback: CoreCallback {
                        cb: csd_minimize_callback as usize,
                    },
                    data: RefAny::new(()),
                }]
                .into(),
            );
        buttons.push(minimize_btn);
    }

    // Maximize button
    if has_maximize {
        let classes = IdOrClassVec::from_vec(vec![
            IdOrClass::Id("csd-button-maximize".into()),
            IdOrClass::Class("csd-button".into()),
            IdOrClass::Class("csd-maximize".into()),
        ]);
        let maximize_btn = Dom::div()
            .with_ids_and_classes(classes)
            .with_child(Dom::text("□"))
            .with_callbacks(
                vec![CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::MouseDown),
                    callback: CoreCallback {
                        cb: csd_maximize_callback as usize,
                    },
                    data: RefAny::new(()),
                }]
                .into(),
            );
        buttons.push(maximize_btn);
    }

    // Close button (optional)
    if has_close {
        let classes = IdOrClassVec::from_vec(vec![
            IdOrClass::Id("csd-button-close".into()),
            IdOrClass::Class("csd-button".into()),
            IdOrClass::Class("csd-close".into()),
        ]);
        let close_btn = Dom::div()
            .with_ids_and_classes(classes)
            .with_child(Dom::text("×"))
            .with_callbacks(
                vec![CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::MouseDown),
                    callback: CoreCallback {
                        cb: csd_close_callback as usize,
                    },
                    data: RefAny::new(()),
                }]
                .into(),
            );
        buttons.push(close_btn);
    }

    // Title text (centered) - with drag callback
    let title_classes = IdOrClassVec::from_vec(vec![IdOrClass::Class("csd-title".into())]);
    let title_text = Dom::div()
        .with_ids_and_classes(title_classes)
        .with_child(Dom::text(title))
        .with_callbacks(
            vec![
                CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::DragStart),
                    callback: CoreCallback {
                        cb: csd_titlebar_drag_start_callback as usize,
                    },
                    data: RefAny::new(TitlebarDragMarker),
                },
                CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::Drag),
                    callback: CoreCallback {
                        cb: csd_titlebar_drag_callback as usize,
                    },
                    data: RefAny::new(TitlebarDragMarker),
                },
            ]
            .into(),
        );

    // Button container
    let button_classes = IdOrClassVec::from_vec(vec![IdOrClass::Class("csd-buttons".into())]);
    let button_vec = DomVec::from_vec(buttons);
    let button_container = Dom::div()
        .with_ids_and_classes(button_classes)
        .with_children(button_vec);

    // Main titlebar container
    let titlebar_classes = IdOrClassVec::from_vec(vec![IdOrClass::Class("csd-titlebar".into())]);

    Dom::div()
        .with_ids_and_classes(titlebar_classes)
        .with_child(title_text)
        .with_child(button_container)
}

/// Default CSS styling for CSD titlebar
///
/// This provides a basic, functional titlebar design that works across
/// all platforms. Users can override these styles with their own CSS.
pub fn get_default_csd_css() -> &'static str {
    r#"
    .csd-titlebar {
        display: flex;
        flex-direction: row;
        align-items: center;
        justify-content: space-between;
        height: 32px;
        min-height: 32px;
        background: linear-gradient(to bottom, #f0f0f0, #e0e0e0);
        border-bottom: 1px solid #c0c0c0;
        padding: 0 8px;
        -webkit-app-region: drag;
        user-select: none;
    }
    
    .csd-title {
        flex: 1;
        text-align: center;
        font-size: 13px;
        font-weight: 600;
        color: #333333;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }
    
    .csd-buttons {
        display: flex;
        flex-direction: row;
        gap: 4px;
        -webkit-app-region: no-drag;
    }
    
    .csd-button {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 32px;
        height: 24px;
        border-radius: 4px;
        background: transparent;
        color: #333333;
        font-size: 16px;
        font-weight: bold;
        cursor: pointer;
        transition: background-color 0.15s ease;
    }
    
    .csd-button:hover {
        background: rgba(0, 0, 0, 0.1);
    }
    
    .csd-button:active {
        background: rgba(0, 0, 0, 0.2);
    }
    
    .csd-close:hover {
        background: #e81123;
        color: white;
    }
    
    .csd-close:active {
        background: #c50f1f;
        color: white;
    }
    
    /* macOS-style titlebar (when detected) */
    @media (platform: macos) {
        .csd-titlebar {
            background: linear-gradient(to bottom, #ececec, #d6d6d6);
            border-bottom: 1px solid #b4b4b4;
        }
        
        .csd-button {
            width: 12px;
            height: 12px;
            border-radius: 50%;
            font-size: 10px;
            margin: 0 4px;
        }
        
        .csd-close {
            background: #ff5f57;
            color: transparent;
        }
        
        .csd-minimize {
            background: #ffbd2e;
            color: transparent;
        }
        
        .csd-maximize {
            background: #28ca42;
            color: transparent;
        }
        
        .csd-close:hover {
            color: #000000;
        }
        
        .csd-minimize:hover {
            color: #000000;
        }
        
        .csd-maximize:hover {
            color: #000000;
        }
    }
    
    /* Linux-style titlebar */
    @media (platform: linux) {
        .csd-titlebar {
            background: #f6f5f4;
            border-bottom: 1px solid #d3d2d1;
        }
        
        .csd-title {
            text-align: left;
            padding-left: 8px;
        }
    }
    "#
}

/// Check if CSD should be injected for a window
///
/// CSD is injected when:
/// 1. `has_decorations` flag is true, AND
/// 2. `decorations` is set to `None` (frameless window)
///
/// # Arguments
/// * `has_decorations` - CSD enable flag
/// * `decorations` - Window decoration style
///
/// # Returns
/// `true` if CSD titlebar should be injected
#[inline]
pub fn should_inject_csd(has_decorations: bool, decorations: WindowDecorations) -> bool {
    has_decorations && decorations == WindowDecorations::None
}

/// Inject CSD titlebar and/or menu into user's DOM using a container approach
///
/// This creates a container StyledDom and appends:
/// 1. Titlebar (if CSD is enabled)
/// 2. Menu bar (if menu is present on root node)
/// 3. User's content DOM
///
/// The container approach allows us to inject system UI elements without
/// modifying the user's DOM structure directly.
///
/// # Arguments
/// * `user_dom` - The user's styled DOM from their layout callback
/// * `window_title` - Current window title
/// * `should_inject_titlebar` - Whether to inject CSD titlebar
/// * `has_minimize` - Show minimize button
/// * `has_maximize` - Show maximize button
/// * `system_style` - System style for native look and feel
///
/// # Returns
/// Container styled DOM with all components
pub fn wrap_user_dom_with_decorations(
    user_dom: StyledDom,
    window_title: &str,
    should_inject_titlebar: bool,
    has_minimize: bool,
    has_maximize: bool,
    system_style: &SystemStyle,
) -> StyledDom {
    // Extract menu bar from user DOM's root node if present
    let menu_bar = user_dom
        .node_data
        .as_container()
        .get(azul_core::dom::NodeId::ZERO)
        .and_then(|root_node| root_node.get_menu_bar())
        .map(|boxed_menu| (**boxed_menu).clone());

    // If no decorations needed and no menu bar, return user's DOM unmodified
    if !should_inject_titlebar && menu_bar.is_none() {
        return user_dom;
    }

    // Create container StyledDom
    let mut container_styled = StyledDom::default();

    // Inject titlebar if needed
    if should_inject_titlebar {
        let titlebar_styled = create_titlebar_styled_dom(
            window_title,
            has_minimize,
            has_maximize,
            true, // has_close - always show close button by default
            system_style,
        );
        container_styled.append_child(titlebar_styled);
    }

    // Inject menu bar if present
    if let Some(menu) = menu_bar {
        let menubar_styled = create_menubar_styled_dom(&menu, system_style);
        container_styled.append_child(menubar_styled);
    }

    // Append user's content
    container_styled.append_child(user_dom);

    container_styled
}

/// CSD button actions that can be triggered by clicking titlebar buttons
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CsdAction {
    /// Close button was clicked
    Close,
    /// Minimize button was clicked
    Minimize,
    /// Maximize/Restore button was clicked
    Maximize,
}

/// Check if a clicked node is a CSD button and return the corresponding action
///
/// This function checks if a node (identified by its CSS ID) is one of the
/// CSD titlebar buttons and returns the appropriate action.
///
/// # Arguments
/// * `node_id_str` - The CSS ID of the clicked node (e.g., "csd-button-close")
///
/// # Returns
/// * `Some(CsdAction)` - If the node is a CSD button
/// * `None` - If the node is not a CSD button
pub fn get_csd_action_for_node(node_id_str: &str) -> Option<CsdAction> {
    match node_id_str {
        "csd-button-close" => Some(CsdAction::Close),
        "csd-button-minimize" => Some(CsdAction::Minimize),
        "csd-button-maximize" => Some(CsdAction::Maximize),
        _ => None,
    }
}

/// Handle a CSD button click by modifying window flags
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_inject_csd() {
        // Should inject when has_decorations=true and decorations=None
        assert!(should_inject_csd(true, WindowDecorations::None));

        // Should NOT inject when has_decorations=false
        assert!(!should_inject_csd(false, WindowDecorations::None));

        // Should NOT inject when decorations != None
        assert!(!should_inject_csd(true, WindowDecorations::Normal));
        assert!(!should_inject_csd(true, WindowDecorations::NoTitle));
        assert!(!should_inject_csd(true, WindowDecorations::NoControls));
    }

    #[test]
    fn test_create_titlebar_dom() {
        use azul_core::dom::NodeType;

        // Test with all buttons enabled
        let dom = create_titlebar_dom("Test Window", true, true, true);

        // Verify structure: should be a div with class "csd-titlebar"
        // The DOM has children: title, buttons container
        // We can check that it's a Div node type
        match dom.root.node_type {
            NodeType::Div => {
                // Success - it's a div as expected
                assert!(true);
            }
            _ => panic!("Expected Div node type for titlebar"),
        }

        // Test with no buttons
        let dom_no_buttons = create_titlebar_dom("Test", false, false, false);
        match dom_no_buttons.root.node_type {
            NodeType::Div => assert!(true),
            _ => panic!("Expected Div node type"),
        }
    }

    #[test]
    fn test_get_csd_action_for_node() {
        assert_eq!(
            get_csd_action_for_node("csd-button-close"),
            Some(CsdAction::Close)
        );
        assert_eq!(
            get_csd_action_for_node("csd-button-minimize"),
            Some(CsdAction::Minimize)
        );
        assert_eq!(
            get_csd_action_for_node("csd-button-maximize"),
            Some(CsdAction::Maximize)
        );
        assert_eq!(get_csd_action_for_node("some-other-id"), None);
        assert_eq!(get_csd_action_for_node(""), None);
    }

    #[test]
    fn test_default_css_not_empty() {
        let css = get_default_csd_css();
        assert!(!css.is_empty());
        assert!(css.contains(".csd-titlebar"));
    }
}
