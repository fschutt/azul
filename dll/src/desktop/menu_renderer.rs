//! Menu Rendering - Converts Menu structures to StyledDom
//!
//! This module provides functions to render menu structures as styled DOM trees.
//! It uses SystemStyle for native look and feel, and supports:
//! - Regular menu items with labels
//! - Separators
//! - Icons (checkboxes and images)
//! - Keyboard shortcuts
//! - Hover states
//! - Submenus (hierarchical structure)
//! - Callback attachment to clickable items

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, CoreCallbackDataVec, Update},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClassVec},
    menu::{Menu, MenuItem, MenuItemIcon, MenuItemState, OptionMenuItemIcon, StringMenuItem},
    refany::RefAny,
};
use azul_css::{css::Css, props::basic::pixel::DEFAULT_FONT_SIZE, system::SystemStyle, AzString};
use azul_layout::callbacks::CallbackInfo;

use crate::{
    desktop::{menu::MenuWindowData, shell2::common::debug_server::LogCategory},
    log_debug,
};

/// Data structure for menu item click callbacks
#[derive(Debug, Clone)]
struct MenuItemCallbackData {
    menu_item: StringMenuItem,
    menu_window_data: RefAny,
    item_index: usize,
}

/// Data structure for submenu hover callbacks
#[derive(Debug, Clone)]
struct SubmenuCallbackData {
    menu_item: StringMenuItem,
    menu_window_data: RefAny,
    item_index: usize,
}

/// Callback invoked when a menu item is clicked
///
/// This:
/// 1. Invokes the menu item's original callback (if present)
/// 2. Closes the menu window
extern "C" fn menu_item_click_callback(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let callback_data = match data.downcast_ref::<MenuItemCallbackData>() {
        Some(d) => d,
        None => {
            log_debug!(
                LogCategory::General,
                "[menu_item_click_callback] Failed to downcast MenuItemCallbackData"
            );
            return Update::DoNothing;
        }
    };

    // Invoke the menu item's callback if present
    if let Some(menu_callback) = callback_data.menu_item.callback.as_option() {
        // Convert CoreCallback to actual function pointer using safe wrapper
        let callback = azul_layout::callbacks::Callback::from_core(menu_callback.callback.clone());

        // Invoke with the menu item's data
        let callback_data_refany = menu_callback.refany.clone();
        let result = callback.invoke(callback_data_refany, info);

        log_debug!(
            LogCategory::General,
            "[menu_item_click_callback] Invoked callback for menu item '{}' (index {})",
            callback_data.menu_item.label.as_str(),
            callback_data.item_index
        );

        // Close the menu window
        let mut state = info.get_current_window_state().clone();
        state.flags.close_requested = true;
        info.modify_window_state(state);

        // A menu item's action typically mutates the SHARED app state, and this menu
        // window is closing — a plain RefreshDom would only re-layout the doomed menu
        // window and be lost. Escalate to RefreshDomAllWindows so the parent window
        // (and any siblings) re-layout to reflect the change.
        return match result {
            Update::RefreshDom | Update::RefreshDomAllWindows => Update::RefreshDomAllWindows,
            Update::DoNothing => Update::DoNothing,
        };
    }

    // No callback attached, just close the menu
    let mut state = info.get_current_window_state().clone();
    state.flags.close_requested = true;
    info.modify_window_state(state);

    Update::DoNothing
}

/// Callback invoked when hovering over a menu item with children
///
/// This spawns a submenu window positioned to the right of the menu item
extern "C" fn submenu_hover_callback(mut data: RefAny, mut info: CallbackInfo) -> Update {
    use alloc::sync::Arc;

    use azul_core::{geom::LogicalPosition, menu::MenuPopupPosition, window::WindowPosition};

    let submenu_data = match data.downcast_ref::<SubmenuCallbackData>() {
        Some(d) => d,
        None => {
            log_debug!(
                LogCategory::General,
                "[submenu_hover_callback] Failed to downcast SubmenuCallbackData"
            );
            return Update::DoNothing;
        }
    };

    // Get the menu item's rectangle (for positioning the submenu)
    let item_rect = match info.get_hit_node_rect() {
        Some(rect) => rect,
        None => {
            log_debug!(
                LogCategory::General,
                "[submenu_hover_callback] Could not get hit node rect"
            );
            return Update::DoNothing;
        }
    };

    // Clone menu_window_data before downcasting to avoid borrow conflicts
    let mut menu_window_data_clone = submenu_data.menu_window_data.clone();

    // Get parent menu window data
    let parent_menu_data = match menu_window_data_clone.downcast_ref::<MenuWindowData>() {
        Some(d) => d,
        None => {
            log_debug!(
                LogCategory::General,
                "[submenu_hover_callback] Failed to downcast parent MenuWindowData"
            );
            return Update::DoNothing;
        }
    };

    // Get system style from CallbackInfo (safe Arc clone)
    let system_style = info.get_system_style();

    // Get parent window position
    let parent_pos = match info.get_current_window_state().position {
        WindowPosition::Initialized(pos) => LogicalPosition::new(pos.x as f32, pos.y as f32),
        _ => LogicalPosition::new(0.0, 0.0),
    };

    // Create submenu using the menu item's children
    let submenu = Menu {
        items: submenu_data.menu_item.children.clone(),
        position: MenuPopupPosition::RightOfHitRect, // Position to the right
        context_mouse_btn: parent_menu_data.menu.context_mouse_btn,
    };

    // Create submenu window (parent_menu_id = this menu's ID)
    let parent_id = parent_menu_data.menu_window_id;
    let submenu_options = crate::desktop::menu::show_menu(
        submenu,
        system_style,
        parent_pos,
        Some(item_rect),
        None, // No cursor position for submenu
        parent_id,
    );

    // Create the submenu window
    // TODO: Track the returned window ID and add to parent_menu_data.child_menu_ids
    info.create_window(submenu_options);

    log_debug!(
        LogCategory::General,
        "[submenu_hover_callback] Spawned submenu for item '{}' (index {})",
        submenu_data.menu_item.label.as_str(),
        submenu_data.item_index
    );

    Update::DoNothing
}

/// Create a menu DOM with deferred CSS for use in layout callbacks.
///
/// Returns a `Dom` with component CSS pushed via `.add_component_css()`
/// (deferred cascade). Use this in `LayoutCallbackType` callbacks which
/// return `Dom` instead of `StyledDom`.
pub fn create_menu_dom_with_css(
    menu: &Menu,
    system_style: &SystemStyle,
    menu_window_data: RefAny,
) -> Dom {
    let mut dom = create_menu_dom(menu, &menu_window_data);
    let css = system_style.create_menu_stylesheet();
    dom.add_component_css(css);
    dom
}

/// Create menu DOM structure with callbacks attached (internal helper)
///
/// # Arguments
/// * `menu` - Menu structure to render
/// * `menu_window_data` - MenuWindowData RefAny for callbacks
///
/// # Returns
/// DOM tree for the menu (unstyled but with callbacks)
fn create_menu_dom(menu: &Menu, menu_window_data: &RefAny) -> Dom {
    // Container for all menu items
    let mut container =
        Dom::create_div().with_ids_and_classes(IdOrClassVec::from_vec(vec![IdOrClass::Class(
            "menu-container".into(),
        )]));

    // A menu opened FOR A NODE is at least as wide as that node - the
    // `<select>` rule, and what the drop-down widget needs to stop looking
    // like a stray context menu (2026-09-01 request). Inline, so it beats the
    // stylesheet's `min-width: 160px` floor; a longer item still widens the
    // menu past it, because this is a MINIMUM.
    let anchor_width = {
        let mut d = menu_window_data.clone();
        d.downcast_ref::<MenuWindowData>()
            .and_then(|d| d.trigger_rect)
            .map(|r| r.size.width)
            .filter(|w| *w > 0.0)
    };
    if let Some(width) = anchor_width {
        use azul_css::{
            dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
            props::{
                basic::pixel::PixelValue,
                layout::LayoutMinWidth,
                property::{CssProperty, LayoutMinWidthValue},
            },
        };
        container = container.with_css_props(CssPropertyWithConditionsVec::from_vec(vec![
            CssPropertyWithConditions::simple(CssProperty::MinWidth(LayoutMinWidthValue::Exact(
                LayoutMinWidth {
                    inner: PixelValue::px(width),
                },
            ))),
        ]));
    }

    // Render each menu item with its index for identification
    for (idx, item) in menu.items.as_slice().iter().enumerate() {
        let item_dom = create_menu_item_dom(item, idx, menu_window_data);
        container = container.with_child(item_dom);
    }

    container
}

/// Create DOM for a single menu item with callbacks
///
/// # Arguments
/// * `item` - Menu item to render
/// * `idx` - Index of this item in the menu (for identification)
/// * `menu_window_data` - MenuWindowData RefAny for callbacks
///
/// # Returns
/// DOM node for this menu item
fn create_menu_item_dom(item: &MenuItem, idx: usize, menu_window_data: &RefAny) -> Dom {
    match item {
        MenuItem::String(string_item) => {
            create_string_menu_item_dom(string_item, idx, menu_window_data)
        }
        MenuItem::Separator => create_separator_dom(),
        MenuItem::BreakLine => {
            // Break lines are only used in horizontal menus (menu bars)
            // For popup menus, we ignore them
            Dom::create_div().with_ids_and_classes(IdOrClassVec::from_vec(vec![IdOrClass::Class(
                "menu-breakline".into(),
            )]))
        }
    }
}

/// Create DOM for a string menu item (label + optional icon + optional shortcut)
///
/// Structure:
/// ```text
/// <div class="menu-item [menu-item-disabled|menu-item-greyed]" id="menu-item-{idx}">
///   <div class="menu-item-icon">[checkbox or image]</div>
///   <div class="menu-item-label">Label Text</div>
///   <div class="menu-item-shortcut">Ctrl+C</div>
///   <div class="menu-item-arrow">▶</div>  <!-- only if has children -->
/// </div>
/// ```
///
/// Callbacks:
/// - MouseDown: Invoke item's callback (if not disabled)
/// - MouseOver: Show submenu if has children
fn create_string_menu_item_dom(
    item: &StringMenuItem,
    idx: usize,
    menu_window_data: &RefAny,
) -> Dom {
    let mut classes = vec![IdOrClass::Class("menu-item".into())];

    let is_disabled = item.menu_item_state == MenuItemState::Disabled
        || item.menu_item_state == MenuItemState::Greyed;
    let has_children = !item.children.as_slice().is_empty();

    // Add state classes
    match item.menu_item_state {
        MenuItemState::Normal => {}
        MenuItemState::Greyed => {
            classes.push(IdOrClass::Class("menu-item-greyed".into()));
        }
        MenuItemState::Disabled => {
            classes.push(IdOrClass::Class("menu-item-disabled".into()));
        }
    }

    // Add submenu class if has children
    if has_children {
        classes.push(IdOrClass::Class("menu-item-has-submenu".into()));
    }

    // Add unique ID for this menu item
    classes.push(IdOrClass::Id(format!("menu-item-{}", idx).into()));

    // Create container with classes
    let mut item_dom = Dom::create_div().with_ids_and_classes(IdOrClassVec::from_vec(classes));

    // Icon section (checkbox, image, or empty space)
    let icon_dom = create_icon_dom(&item.icon);
    item_dom = item_dom.with_child(icon_dom);

    // Label text
    // Wrap the label text in a block div so it lays out as a proper flex item: a
    // bare text node as a direct flex-container child gets no used_size (renders
    // invisible). Mirrors how button/drop_down widgets wrap their text.
    let label_dom = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(vec![IdOrClass::Class(
            "menu-item-label".into(),
        )]))
        .with_child(Dom::create_p_with_text(item.label.clone()));
    item_dom = item_dom.with_child(label_dom);

    // Keyboard shortcut (if present)
    if let Some(combo) = item.accelerator.as_option() {
        let shortcut_text = format_accelerator(combo);
        let shortcut_dom = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_vec(vec![IdOrClass::Class(
                "menu-item-shortcut".into(),
            )]))
            .with_child(Dom::create_p_with_text(shortcut_text));
        item_dom = item_dom.with_child(shortcut_dom);
    }

    // Submenu arrow (if has children).
    //
    // The DESKTOP's own arrow when it has one (`system:arrow-right`, loaded
    // from the session's icon theme at startup — see
    // `linux::system_icons`), because a submenu indicator is a shape every
    // platform already ships and the hardcoded "▶" is a different weight,
    // size and baseline from all of them. `create_icon` resolves through the
    // icon provider, so a session that registered no system pack falls back
    // to the glyph below and nothing regresses.
    if has_children {
        // A fallback CHAIN, which is what icon specs are for: the desktop's own
        // arrow if the session registered one, then the Material glyph that
        // ships with the engine. An unresolved icon renders as an empty div,
        // so the chain is what keeps the indicator visible off-KDE.
        let indicator = Dom::create_icon("system:arrow-right,chevron_right");
        let arrow_dom = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_vec(vec![IdOrClass::Class(
                "menu-item-arrow".into(),
            )]))
            .with_child(indicator);
        item_dom = item_dom.with_child(arrow_dom);
    }

    // Attach callbacks if not disabled
    if !is_disabled {
        let mut callbacks = Vec::new();

        // Click callback: Invoke menu item action
        if item.callback.as_option().is_some() {
            // Create callback data containing both the original callback and menu data
            let callback_data = MenuItemCallbackData {
                menu_item: item.clone(),
                menu_window_data: menu_window_data.clone(),
                item_index: idx,
            };

            callbacks.push(CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::MouseDown),
                callback: CoreCallback {
                    cb: menu_item_click_callback as *const () as usize,
                    ctx: azul_core::refany::OptionRefAny::None,
                },
                refany: RefAny::new(callback_data),
            });
        }

        // Hover callback: Show submenu if has children
        if has_children {
            let submenu_data = SubmenuCallbackData {
                menu_item: item.clone(),
                menu_window_data: menu_window_data.clone(),
                item_index: idx,
            };

            callbacks.push(CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::MouseOver),
                callback: CoreCallback {
                    cb: submenu_hover_callback as *const () as usize,
                    ctx: azul_core::refany::OptionRefAny::None,
                },
                refany: RefAny::new(submenu_data),
            });
        }

        if !callbacks.is_empty() {
            item_dom = item_dom.with_callbacks(CoreCallbackDataVec::from_vec(callbacks));
        }
    }

    item_dom
}

/// Create icon DOM element
fn create_icon_dom(icon: &OptionMenuItemIcon) -> Dom {
    let mut icon_dom =
        Dom::create_div().with_ids_and_classes(IdOrClassVec::from_vec(vec![IdOrClass::Class(
            "menu-item-icon".into(),
        )]));

    if let Some(icon_value) = icon.as_option() {
        match icon_value {
            MenuItemIcon::Checkbox(checked) => {
                // The classes stay on the icon DIV: it is the box the menu
                // stylesheet sizes and paints. Putting them on the checkmark
                // text node instead made them inert (a text node has no box)
                // AND dropped the div itself.
                icon_dom = icon_dom.with_ids_and_classes(IdOrClassVec::from_vec(vec![
                    IdOrClass::Class("menu-item-icon".into()),
                    IdOrClass::Class("menu-item-checkbox".into()),
                    IdOrClass::Class(
                        if *checked {
                            "menu-item-checkbox-checked"
                        } else {
                            "menu-item-checkbox-unchecked"
                        }
                        .into(),
                    ),
                ]));
                // Add checkmark if checked
                if *checked {
                    // The desktop's own tick where the session has one, the
                    // engine's Material glyph otherwise (an icon spec is a
                    // fallback chain; an unresolved icon renders as an empty
                    // div, so the tail is what keeps the mark visible).
                    icon_dom = icon_dom.with_child(Dom::create_icon("system:checkmark,check"));
                }
            }
            MenuItemIcon::Image(image_ref) => {
                // Render the custom image (typically 16x16) as a real image node.
                // (The old TODO predates azul's image rendering — Dom::create_image
                // now exists, so the icon's ImageRef no longer has to be dropped.)
                icon_dom = Dom::create_image(image_ref.clone()).with_ids_and_classes(
                    IdOrClassVec::from_vec(vec![
                        IdOrClass::Class("menu-item-icon".into()),
                        IdOrClass::Class("menu-item-image-icon".into()),
                    ]),
                );
            }
        }
    }

    icon_dom
}

/// Create separator DOM element
fn create_separator_dom() -> Dom {
    Dom::create_div().with_ids_and_classes(IdOrClassVec::from_vec(vec![IdOrClass::Class(
        "menu-separator".into(),
    )]))
}

/// Format keyboard accelerator for display
///
/// Converts VirtualKeyCodeCombo to human-readable string like "Ctrl+C"
fn format_accelerator(combo: &azul_core::window::VirtualKeyCodeCombo) -> AzString {
    let key_strs: Vec<&str> = combo
        .keys
        .as_slice()
        .iter()
        .map(|k| virtual_key_to_str(k))
        .collect();

    AzString::from(key_strs.join("+"))
}

fn virtual_key_to_str(key: &azul_core::window::VirtualKeyCode) -> &'static str {
    use azul_core::window::VirtualKeyCode::*;
    match key {
        Key1 => "1",
        Key2 => "2",
        Key3 => "3",
        Key4 => "4",
        Key5 => "5",
        Key6 => "6",
        Key7 => "7",
        Key8 => "8",
        Key9 => "9",
        Key0 => "0",
        A => "A",
        B => "B",
        C => "C",
        D => "D",
        E => "E",
        F => "F",
        G => "G",
        H => "H",
        I => "I",
        J => "J",
        K => "K",
        L => "L",
        M => "M",
        N => "N",
        O => "O",
        P => "P",
        Q => "Q",
        R => "R",
        S => "S",
        T => "T",
        U => "U",
        V => "V",
        W => "W",
        X => "X",
        Y => "Y",
        Z => "Z",
        Escape => "Esc",
        Tab => "Tab",
        Space => "Space",
        Return => "Enter",
        Back => "Backspace",
        Delete => "Del",
        Insert => "Ins",
        Home => "Home",
        End => "End",
        PageUp => "Page Up",
        PageDown => "Page Down",
        Left => "Left",
        Right => "Right",
        Up => "Up",
        Down => "Down",
        F1 => "F1",
        F2 => "F2",
        F3 => "F3",
        F4 => "F4",
        F5 => "F5",
        F6 => "F6",
        F7 => "F7",
        F8 => "F8",
        F9 => "F9",
        F10 => "F10",
        F11 => "F11",
        F12 => "F12",
        F13 => "F13",
        F14 => "F14",
        F15 => "F15",
        F16 => "F16",
        F17 => "F17",
        F18 => "F18",
        F19 => "F19",
        F20 => "F20",
        F21 => "F21",
        F22 => "F22",
        F23 => "F23",
        F24 => "F24",
        LControl | RControl => "Ctrl",
        LShift | RShift => "Shift",
        LAlt | RAlt => "Alt",
        LWin | RWin => "Super",
        Numpad0 => "Num 0",
        Numpad1 => "Num 1",
        Numpad2 => "Num 2",
        Numpad3 => "Num 3",
        Numpad4 => "Num 4",
        Numpad5 => "Num 5",
        Numpad6 => "Num 6",
        Numpad7 => "Num 7",
        Numpad8 => "Num 8",
        Numpad9 => "Num 9",
        NumpadAdd => "Num +",
        NumpadSubtract => "Num -",
        NumpadMultiply => "Num *",
        NumpadDivide => "Num /",
        NumpadDecimal => "Num .",
        NumpadComma => "Num ,",
        NumpadEnter => "Num Enter",
        NumpadEquals => "Num =",
        Numlock => "Num Lock",
        Scroll => "Scroll Lock",
        Snapshot => "Print Screen",
        Pause => "Pause",
        Minus => "-",
        Plus => "+",
        Equals => "=",
        LBracket => "[",
        RBracket => "]",
        Semicolon => ";",
        Apostrophe => "'",
        Grave => "`",
        Backslash => "\\",
        Slash => "/",
        Period => ".",
        Comma => ",",
        Colon => ":",
        At => "@",
        Asterisk => "*",
        Caret => "^",
        Underline => "_",
        Yen => "¥",
        Copy => "Copy",
        Paste => "Paste",
        Cut => "Cut",
        Compose => "Compose",
        Capital => "Caps Lock",
        _ => "?",
    }
}

/// Every number a menu is built from, derived ONCE from the live
/// [`SystemStyle`] so that the size the popup is CREATED at and the CSS it is
/// RENDERED with cannot disagree. They used to be two independent sets of
/// literals (a 24px item and a 200px width in the Wayland estimate, a 28px
/// item and a 220px width in the cross-platform one, a 160px floor in the
/// stylesheet), so no menu was ever the size any of them said.
///
/// Per property, what a KDE Plasma desktop can actually tell azul:
/// * `font_family` / `font_size_px` — KNOWN. `kdeglobals [General] menuFont`,
///   read by `linux::system_style::discover_kde_style`. Reported in POINTS.
/// * `border_width`, `corner_radius` — from `SystemMetrics`, but on KDE those
///   are still the `defaults::kde_breeze_*` presets: Breeze keeps them in its
///   compiled QStyle `Metrics` table and publishes no config key for them.
/// * `pad_h` / `pad_v` — the desktop's CONTROL padding, halved vertically
///   because a menu is tighter than a button in every toolkit. Breeze's own
///   `Metrics::MenuItem_MarginWidth` is likewise not exported.
/// * `icon_size` — derived from the font. Plasma DOES publish the small-icon
///   size (`kdeglobals [Icons] Size`), but `LinuxCustomization` has no field
///   to carry it, so no discovery code could read it today.
/// * `min_width` — azul's own floor, expressed in ems so it tracks the
///   desktop's font instead of pinning every menu to one pixel count. Qt
///   sizes a menu to its contents and imposes no minimum of its own.
/// * the drop shadow is deliberately NOT derived: KWin does not shadow a
///   client's popup, Qt paints its own into a surface margin, and azul never
///   calls `xdg_surface_set_window_geometry` — so a shadow drawn outside the
///   container's border box is clipped away by the popup surface.
#[derive(Debug, Clone, PartialEq)]
pub struct MenuMetrics {
    /// The desktop's menu font, in CSS pixels (it reports POINTS).
    pub font_size_px: f32,
    /// The desktop's menu font family.
    pub font_family: String,
    /// Padding left and right of an item's contents.
    pub pad_h: f32,
    /// Padding above and below an item's contents.
    pub pad_v: f32,
    /// The content box of one item: the taller of the nominal line box and
    /// the checkmark gutter. The REAL line box is only known after shaping;
    /// this is the pre-layout estimate, which `measure_popup_content`
    /// afterwards replaces with the truth.
    pub row_height: f32,
    /// One item's border box: `row_height` plus its vertical padding.
    pub item_height: f32,
    /// A separator's border box: the rule plus its margins.
    pub separator_height: f32,
    /// Padding the frame adds above the first and below the last item.
    pub frame_pad_v: f32,
    /// The frame's own line.
    pub border_width: f32,
    pub corner_radius: f32,
    /// The checkmark / icon gutter.
    pub icon_size: f32,
    /// The width the stylesheet promises and the popup is measured at.
    pub min_width: f32,
}

impl MenuMetrics {
    /// A nominal line box, as a multiple of the font size. The real one comes
    /// from the shaped font; this only has to be close enough to create the
    /// popup surface with.
    const NOMINAL_LINE_HEIGHT: f32 = 1.2;
    /// The checkmark gutter, as a multiple of the font size — a fixed 20px box
    /// beside 8px text is a huge gutter and beside 16px text a cramped one.
    const ICON_COLUMN_EMS: f32 = 1.4;
    /// The menu's minimum width, in ems of its own font.
    const MIN_WIDTH_EMS: f32 = 12.0;

    #[must_use]
    pub fn from_system_style(style: &SystemStyle) -> Self {
        // The system reports the size in POINTS. A typographic point is 1/72
        // inch and a CSS pixel 1/96, so on Linux and Windows the point size is
        // 4/3 as many pixels (KDE's "Noto Sans,10" is 13.3px); used as pixels
        // it made every menu a quarter smaller than the desktop's. On macOS a
        // Cocoa point already IS a logical pixel.
        let px_per_pt = if matches!(style.platform, azul_css::system::Platform::MacOs) {
            1.0
        } else {
            azul_css::props::basic::pixel::PT_TO_PX
        };
        // The MENU font, not the generic UI font: KDE and GNOME both let the
        // user set it separately, and a menu laid out in the wrong face is the
        // most visible way to not look like the desktop. Falls back to the UI
        // font, which is what those desktops mean by "unset" anyway.
        let font_size_px = style
            .fonts
            .menu_font_size
            .as_option()
            .or(style.fonts.ui_font_size.as_option())
            .map(|pt| pt * px_per_pt)
            .unwrap_or(14.0);
        let font_family = style
            .fonts
            .menu_font
            .as_option()
            .or(style.fonts.ui_font.as_option())
            .map(|f| f.as_str().to_string())
            .unwrap_or_else(|| "sans-serif".to_string());

        // Item padding follows the platform's control padding instead of one
        // hardcoded number, so a Breeze menu is as tight as Breeze and an
        // Adwaita one as roomy as Adwaita. Menus are tighter than buttons in
        // every toolkit, hence the vertical halving.
        let pad_h = style
            .metrics
            .button_padding_horizontal
            .map(|p| p.to_pixels_internal(1.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE))
            .unwrap_or(8.0);
        let pad_v = style
            .metrics
            .button_padding_vertical
            .map(|p| p.to_pixels_internal(1.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE))
            .unwrap_or(8.0)
            * 0.5;
        let border_width = style
            .metrics
            .border_width
            .map(|p| p.to_pixels_internal(1.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE))
            .unwrap_or(1.0);
        let corner_radius = style
            .metrics
            .corner_radius
            .map(|p| p.to_pixels_internal(1.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE))
            .unwrap_or(4.0);

        let icon_size = (font_size_px * Self::ICON_COLUMN_EMS)
            .round()
            .clamp(14.0, 24.0);
        let line_box = (font_size_px * Self::NOMINAL_LINE_HEIGHT).round();
        // The icon div is always emitted, so it is a flex sibling of the label
        // and the row is as tall as the taller of the two.
        let row_height = line_box.max(icon_size);

        Self {
            font_size_px,
            font_family,
            pad_h,
            pad_v,
            row_height,
            item_height: row_height + 2.0 * pad_v,
            separator_height: border_width + 2.0 * pad_v,
            frame_pad_v: pad_v,
            border_width,
            corner_radius,
            icon_size,
            min_width: (font_size_px * Self::MIN_WIDTH_EMS).round(),
        }
    }

    /// The size the popup surface is created at, before the DOM is measured.
    ///
    /// The WIDTH is the minimum the stylesheet promises and nothing more: a
    /// menu is laid out at this width and `get_content_size` returns the
    /// larger of it and thecontent, so it is the MEASUREMENT — not this
    /// estimate — that decides how wide a menu ends up. An estimate wider
    /// than the content (the old flat 200px) can never be corrected downwards,
    /// which is why every menu used to be 200px wide whatever it contained.
    #[must_use]
    pub fn estimate_menu_size(&self, menu: &Menu) -> azul_core::geom::LogicalSize {
        let content: f32 = menu
            .items
            .as_slice()
            .iter()
            .map(|item| match item {
                MenuItem::Separator => self.separator_height,
                _ => self.item_height,
            })
            .sum();
        azul_core::geom::LogicalSize::new(
            self.min_width,
            2.0 * self.border_width + 2.0 * self.frame_pad_v + content,
        )
    }
}

/// Extension trait to add menu stylesheet creation to SystemStyle
///
/// Generates a `Css` containing CSS classes for the menu system:
/// `.menu-container`, `.menu-item`, `.menu-item-icon`, `.menu-item-label`,
/// `.menu-item-shortcut`, `.menu-item-arrow`, `.menu-separator`,
/// `.menu-item-disabled`, `.menu-item-greyed`, and hover states.
pub trait SystemStyleMenuExt {
    /// Create a Css subtree for menu rendering based on system colors, fonts,
    /// and metrics. Rules carry `rule_priority::SYSTEM`.
    fn create_menu_stylesheet(&self) -> Css;
}

impl SystemStyleMenuExt for SystemStyle {
    fn create_menu_stylesheet(&self) -> Css {
        use azul_css::{parser2::new_from_str, props::basic::ColorU};

        let mut css = String::new();

        // Get colors from system style
        let bg_color = self
            .colors
            .window_background
            .as_option()
            .copied()
            .unwrap_or(ColorU::new_rgb(240, 240, 240));
        let text_color = self
            .colors
            .text
            .as_option()
            .copied()
            .unwrap_or(ColorU::new_rgb(0, 0, 0));
        let hover_color = self
            .colors
            .selection_background
            .as_option()
            .copied()
            .unwrap_or(ColorU::new_rgb(0, 120, 215));
        let hover_text_color = self
            .colors
            .selection_text
            .as_option()
            .copied()
            .unwrap_or(ColorU::new_rgb(255, 255, 255));
        let disabled_color = self
            .colors
            .disabled_text
            .as_option()
            .copied()
            .unwrap_or(ColorU::new_rgb(128, 128, 128)); // Fallback for disabled
        // A separator is a LINE, and every desktop publishes the colour it
        // rules its lines in (`Colors:Window/BackgroundAlternate` on KDE, read
        // by `discover_kde_style`). `colors.background` is the CONTENT surface
        // — a text field's white — so the rule came out as a bright bar across
        // a light menu and a near-black one across a dark one.
        let separator_color = self
            .colors
            .separator
            .as_option()
            .copied()
            .unwrap_or(ColorU::new_rgb(200, 200, 200)); // Fallback for separator

        // One derivation of every menu number, shared with the size the
        // popup surface is created at (`MenuMetrics::estimate_menu_size`).
        let m = MenuMetrics::from_system_style(self);
        let font_size = m.font_size_px;
        let font_family = m.font_family.clone();
        let corner_radius = m.corner_radius;
        let pad_h = m.pad_h;
        let pad_v = m.pad_v;
        let icon_size = m.icon_size;

        // Menu container
        css.push_str(&format!(
            ".menu-container {{\nbackground: rgb({}, {}, {});\nborder: {}px solid rgb({}, {}, \
             {});\nborder-radius: {}px;\nbox-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);\npadding: \
             {}px 0;\nmin-width: {}px;\n}}\n",
            bg_color.r,
            bg_color.g,
            bg_color.b,
            m.border_width,
            separator_color.r,
            separator_color.g,
            separator_color.b,
            corner_radius,
            m.frame_pad_v,
            m.min_width
        ));

        // Menu item
        css.push_str(&format!(
            ".menu-item {{\ndisplay: flex;\nflex-direction: row;\nalign-items: center;\npadding: \
             {}px {}px;\nmin-height: {}px;\ncolor: rgb({}, {}, {});\nfont-family: \
             {};\nfont-size: {}px;\ncursor: pointer;\nuser-select: none;\n}}\n",
            pad_v,
            pad_h,
            m.row_height,
            text_color.r,
            text_color.g,
            text_color.b,
            font_family,
            font_size
        ));

        // Menu item hover state
        css.push_str(&format!(
            ".menu-item:hover {{\nbackground: rgb({}, {}, {});\ncolor: rgb({}, {}, {});\n}}\n",
            hover_color.r,
            hover_color.g,
            hover_color.b,
            hover_text_color.r,
            hover_text_color.g,
            hover_text_color.b
        ));

        // Disabled menu item
        css.push_str(&format!(
            ".menu-item-disabled, .menu-item-greyed {{\ncolor: rgb({}, {}, {});\ncursor: \
             default;\n}}\n",
            disabled_color.r, disabled_color.g, disabled_color.b
        ));

        // No hover for disabled items
        css.push_str(
            ".menu-item-disabled:hover, .menu-item-greyed:hover {\nbackground: \
             transparent;\ncolor: inherit;\n}\n",
        );

        // Menu item icon
        css.push_str(&format!(
            ".menu-item-icon {{\nwidth: {}px;\nheight: {}px;\nmargin-right: {}px;\ntext-align: \
             center;\nflex-shrink: 0;\n}}\n",
            icon_size,
            icon_size,
            pad_h / 2.0
        ));

        // THE LABELS ARE `<p>`s, AND `<p>` CARRIES `margin: 1em 0` FROM THE UA
        // STYLESHEET (core/src/ua_css.rs). Every menu item therefore stood a
        // full font-size taller at BOTH ends than its padding said — roughly
        // doubling item height, which is the "menus have extreme spacing"
        // report. The widget label P-wrap is the house convention (a bare text
        // node in a flex row gets no used_size), so the fix belongs here: the
        // menu resets the margins the wrapper brings with it.
        //
        // NEGATIVE CONTROL: drop this rule and a 2-item menu grows by ~2em.
        css.push_str(
            ".menu-item p, .menu-item-label p, .menu-item-shortcut p, .menu-item-arrow p, \
             .menu-item-icon p {\nmargin-top: 0px;\nmargin-bottom: 0px;\n}\n",
        );

        // Checkbox styling
        css.push_str(".menu-item-checkbox-checked {\nfont-weight: bold;\n}\n");

        // Menu item label
        css.push_str(".menu-item-label {\nflex-grow: 1;\nwhite-space: nowrap;\n}\n");

        // Menu item shortcut
        css.push_str(&format!(
            ".menu-item-shortcut {{\nmargin-left: {}px;\nopacity: 0.6;\nfont-size: \
             {}px;\nwhite-space: nowrap;\n}}\n",
            pad_h * 2.0,
            font_size * 0.9
        ));

        // Submenu arrow
        css.push_str(&format!(
            ".menu-item-arrow {{\nmargin-left: {}px;\nopacity: 0.6;\n}}\n",
            pad_h / 2.0
        ));

        // Separator
        css.push_str(&format!(
            ".menu-separator {{\nheight: {}px;\nbackground: rgb({}, {}, {});\nmargin: {}px \
             {}px;\n}}\n",
            m.border_width, separator_color.r, separator_color.g, separator_color.b, pad_v, pad_h
        ));

        // Parse CSS and extract first stylesheet
        let (mut parsed_css, errors) = new_from_str(&css);
        if !errors.is_empty() {
            log_debug!(
                LogCategory::General,
                "[create_menu_stylesheet] CSS parse errors: {:?}",
                errors
            );
        }
        // Tag every rule as system-level so author CSS overrides win.
        for rule in parsed_css.rules.as_mut() {
            rule.priority = azul_css::css::rule_priority::SYSTEM;
        }
        parsed_css
    }
}

#[cfg(test)]
mod menu_icon_tests {
    use azul_core::{
        dom::NodeType,
        resources::{ImageRef, RawImageFormat},
    };

    use super::*;

    /// A menu item's Image icon must render as a real image node. Previously the
    /// `MenuItemIcon::Image` arm dropped the `ImageRef` (a TODO) and only added a
    /// CSS class, so custom menu icons never appeared.
    #[test]
    fn image_icon_renders_as_an_image_node() {
        let img = ImageRef::null_image(16, 16, RawImageFormat::BGRA8, Vec::new());
        let dom = create_icon_dom(&OptionMenuItemIcon::Some(MenuItemIcon::Image(img)));
        assert!(
            matches!(dom.root.node_type, NodeType::Image(_)),
            "Image icon must render as an Image node, got {:?}",
            dom.root.node_type
        );
    }

    /// A checkbox icon is NOT an image node — guards the two icon arms from being
    /// conflated.
    #[test]
    fn checkbox_icon_is_not_an_image_node() {
        let dom = create_icon_dom(&OptionMenuItemIcon::Some(MenuItemIcon::Checkbox(true)));
        assert!(!matches!(dom.root.node_type, NodeType::Image(_)));
    }
}

/// The menu stylesheet must reflect the DETECTED desktop, and must undo the
/// UA margins its own label wrapper brings in.
#[cfg(test)]
mod menu_stylesheet_tests {
    use azul_css::{
        corety::{OptionF32, OptionString},
        props::basic::pixel::{OptionPixelValue, PixelValue},
        system::defaults,
    };

    use super::*;

    fn css_text(style: &SystemStyle) -> String {
        // The rules are parsed back out of the generated text, so assert on
        // the text: it is what `create_menu_stylesheet` actually emits.
        let mut out = String::new();
        for rule in style.create_menu_stylesheet().rules.as_ref() {
            out.push_str(&format!("{:?}", rule));
        }
        out
    }

    /// Menu labels are `<p>`s (the house P-wrap convention: a bare text node
    /// in a flex row gets no used_size), and `<p>` carries `margin: 1em 0`
    /// from the UA stylesheet — so without a reset every item stands a full
    /// font-size taller at each end than its padding says. That is the
    /// "menus have extreme spacing" report.
    ///
    /// NEGATIVE CONTROL: delete the `.menu-item p` rule and this fails.
    #[test]
    fn the_menu_resets_the_ua_margins_on_its_label_wrappers() {
        let css = css_text(&defaults::kde_breeze_dark());
        assert!(
            css.contains("MarginTop") && css.contains("MarginBottom"),
            "the menu stylesheet must zero the <p> margins its labels inherit from the UA \
             stylesheet, or every item is ~2em too tall"
        );
    }

    /// The menu is laid out in the desktop's MENU font, not in whatever the
    /// generic UI font happens to be — both KDE and GNOME let the user set
    /// them apart.
    #[test]
    fn the_menu_font_wins_over_the_ui_font() {
        let mut style = defaults::kde_breeze_dark();
        style.fonts.ui_font = OptionString::Some("UiFace".into());
        style.fonts.ui_font_size = OptionF32::Some(11.0);
        style.fonts.menu_font = OptionString::Some("MenuFace".into());
        style.fonts.menu_font_size = OptionF32::Some(9.0);

        let css = css_text(&style);
        assert!(
            css.contains("MenuFace"),
            "the detected menu font must reach the menu stylesheet"
        );
        assert!(
            !css.contains("UiFace"),
            "the UI font must not override the menu font when both are set"
        );
    }

    /// The value of every font-size declaration in the menu stylesheet, as
    /// printed ("16px").
    fn font_sizes(css: &str) -> Vec<String> {
        const KEY: &str = "StyleFontSize { inner: ";
        css.match_indices(KEY)
            .filter_map(|(i, _)| {
                let rest = &css[i + KEY.len()..];
                rest.find(' ').map(|end| rest[..end].to_string())
            })
            .collect()
    }

    /// The desktop reports its menu font in POINTS ("Noto Sans,10" on KDE);
    /// CSS pixels are 1/96 inch, points 1/72. Laid out as pixels, a Breeze
    /// menu came out at 10px instead of 13.3px - visibly smaller and tighter
    /// than every other menu on the desktop. On macOS a Cocoa point IS a
    /// logical pixel, so there the number stays.
    #[test]
    fn the_menu_font_size_is_the_desktops_point_size_in_css_pixels() {
        let mut kde = defaults::kde_breeze_light();
        kde.fonts.menu_font_size = OptionF32::Some(12.0);
        let sizes = font_sizes(&css_text(&kde));
        assert!(
            sizes.iter().any(|s| s == "16px"),
            "12pt must be laid out at 16px on Linux, got {sizes:?}"
        );
        assert!(
            !sizes.iter().any(|s| s == "12px"),
            "the point size must not be used as a pixel size, got {sizes:?}"
        );

        let mut mac = defaults::macos_modern_light();
        mac.fonts.menu_font_size = OptionF32::Some(13.0);
        let sizes = font_sizes(&css_text(&mac));
        assert!(
            sizes.iter().any(|s| s == "13px"),
            "a macOS point is a logical pixel: 13pt stays 13px, got {sizes:?}"
        );
    }

    /// With no menu font detected, the UI font is what those desktops mean by
    /// "unset" — the menu must not fall back past it to `sans-serif`.
    #[test]
    fn an_undetected_menu_font_falls_back_to_the_ui_font() {
        let mut style = defaults::kde_breeze_dark();
        style.fonts.ui_font = OptionString::Some("UiFace".into());
        style.fonts.menu_font = OptionString::None;
        style.fonts.menu_font_size = OptionF32::None;

        let css = css_text(&style);
        assert!(
            css.contains("UiFace"),
            "an unset menu font must fall back to the UI font, not to sans-serif"
        );
    }

    /// A menu's frame is the desktop's own line, at the desktop's own width.
    /// A baked `1px solid rgb(180,180,180)` is a light-grey box drawn around
    /// a Breeze DARK menu - the single most obviously foreign thing about it.
    #[test]
    fn the_menu_frame_is_the_systems_own_colour() {
        use azul_css::props::basic::{ColorU, OptionColorU};

        let mut style = defaults::kde_breeze_dark();
        style.colors.separator = OptionColorU::Some(ColorU::new_rgb(11, 22, 33));
        let css = css_text(&style);

        assert!(
            css.contains("r: 11, g: 22, b: 33"),
            "the detected separator colour must draw the menu's frame"
        );
        assert!(
            !css.contains("r: 180, g: 180, b: 180"),
            "the menu frame must not be a hardcoded grey"
        );
    }

    /// ...and at the desktop's own width.
    #[test]
    fn the_menu_frame_follows_the_detected_border_width() {
        let mut thin = defaults::kde_breeze_dark();
        thin.metrics.border_width = OptionPixelValue::Some(PixelValue::px(1.0));
        let mut thick = defaults::kde_breeze_dark();
        thick.metrics.border_width = OptionPixelValue::Some(PixelValue::px(3.0));

        assert_ne!(
            css_text(&thin),
            css_text(&thick),
            "the detected border width must reach the menu frame"
        );
    }

    /// A separator is a LINE, and every desktop publishes the colour it draws
    /// its lines in. Drawing it in the CONTENT background (`Colors:View`)
    /// paints a white bar across a light menu and a near-black one across a
    /// dark menu - the colour of a text field, not of a rule.
    #[test]
    fn the_separator_is_the_systems_separator_colour_not_its_content_background() {
        use azul_css::props::basic::{ColorU, OptionColorU};

        let mut style = defaults::kde_breeze_dark();
        style.colors.background = OptionColorU::Some(ColorU::new_rgb(44, 55, 66));
        style.colors.separator = OptionColorU::Some(ColorU::new_rgb(11, 22, 33));
        let css = css_text(&style);

        assert!(
            !css.contains("r: 44, g: 55, b: 66"),
            "the separator must not be drawn in the content background"
        );
    }

    /// Item padding follows the platform's control padding, so a Breeze menu
    /// is as tight as Breeze. A hardcoded number cannot be both.
    #[test]
    fn item_padding_follows_the_detected_control_padding() {
        let mut tight = defaults::kde_breeze_dark();
        tight.metrics.button_padding_horizontal = OptionPixelValue::Some(PixelValue::px(4.0));
        let mut roomy = defaults::kde_breeze_dark();
        roomy.metrics.button_padding_horizontal = OptionPixelValue::Some(PixelValue::px(20.0));

        assert_ne!(
            css_text(&tight),
            css_text(&roomy),
            "the detected control padding must change the menu's padding"
        );
    }
}
