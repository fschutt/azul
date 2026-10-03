use azul::window::WindowDecorations;
use azul::dom::{
    AccordionOnToggleCallback, AlertOnDismissCallback, AttributeNameValue, AttributeType,
    BreadcrumbOnNavigateCallback, ChipOnRemoveCallback, ComboBoxOnSelectCallback,
    DatePickerOnChangeCallback, IdOrClass, NodeType, PaginationOnChangeCallback,
    RadioGroupOnChangeCallback,
    SegmentedOnChangeCallback, SliderOnValueChangeCallback, SplitPaneOnResizeCallback,
    StepperOnStepChangeCallback, SwitchOnToggleCallback, TextAreaOnFocusLostCallback,
    TimePickerOnChangeCallback,
};
use azul::{
    css::DarkLightMode,
    menu::{Menu, MenuItem, StringMenuItem},
    misc::{TransientDock, TransientTearoff},
    option::OptionDarkLightMode,
    prelude::*,
    widgets::*,
    window::TransientWindowConfig,
};

mod forms;
mod hotkeys;
mod blocks;
mod dialogs;
mod mail;
mod notifications;
mod video;

#[derive(Clone)]
struct Showcase {
    switch_on: bool,
    slider_value: f32,
    number: f32,
    text: azul::str::String,
    /// The TextArea's text, handed back on every rebuild like the
    /// TextInput's, so a rebuild never depends on the engine still holding
    /// what was typed.
    textarea_text: azul::str::String,
    checkbox_checked: bool,
    selected_radio: usize,
    selected_segment: usize,
    selected_choice: usize,
    progress: f32,
    current_page: usize,
    current_step: usize,
    interactions: usize,
    color: ColorU,
    menu_status: azul::str::String,
    dropped: Vec<azul::str::String>,
    file_hovering: bool,
    tabs: Vec<azul::str::String>,
    active_tab: usize,
    drag_tab: usize,
    drag_over: usize,
    date: DatePickerState,
    time: TimePickerState,
    combo_text: azul::str::String,
    accordion_open: Vec<bool>,
    /// The SplitPane's first-pane fraction: the pane is a controlled widget,
    /// so the ratio its `on_resize` reports is stored here and handed back
    /// on every rebuild (a fixed ratio snapped the divider back on the
    /// first rebuild after a drag).
    split_ratio: f32,
    /// How the modal dialog was last closed (its return value).
    dialog_result: azul::str::String,
    notifications: notifications::NotificationsDemo,
    /// The Video card's own state (see `video.rs`).
    video: RefAny,
    hotkey: hotkeys::HotkeyDemo,
    /// The toolbar's mode segment: 0 System, 1 Light, 2 Dark. The choice
    /// itself is the ENGINE's (`CallbackInfo::set_mode`, app-wide); this is
    /// only the controlled segment's selection.
    mode_index: usize,
    /// The "Every input type" form and the "Raw HTML inputs" form (see
    /// `forms.rs`).
    form: forms::FormDemo,
    /// The "Mail" section's values: the Outlook-style mail widgets (see
    /// `mail.rs`).
    mail: mail::MailDemo,
    /// The "Dialogs" section's values: the wizard, settings and standard
    /// dialogs (see `dialogs.rs`).
    dialogs: dialogs::DialogsDemo,
    /// The "Building blocks" section's values: the rich text editor, list
    /// selection, the close guard, the zoom range, toggled / disabled
    /// buttons, the week start (see `blocks.rs`).
    blocks: blocks::BlocksDemo,
}

const CHOICES: &[&str] = &["Red", "Green", "Blue"];

fn strs(items: &[&str]) -> Vec<azul::str::String> {
    items.iter().map(|s| (*s).into()).collect()
}

// Every colour on this page is a `system:` colour: the page, the cards, the
// text and the rules are the desktop's own palette, resolved in whichever
// theme the window is in - the same palette the widgets paint from - so the
// demo follows the platform in light AND dark with one value each.
// Surfaces: the page is `system:background`, a card
// `system:window-background`, a drop zone `system:control-background`; the
// titlebar has no fill of its own, like a transparent native titlebar; text is
// `system:text` / `system:secondary-text` / `system:tertiary-text`, rules are
// `system:separator`. The face is the platform's UI font (`system:ui`), and
// the titlebar's is its bold title face (`system:title:bold`).

/// A caption above `widget`, which also becomes the widget's accessible name.
/// For a control whose ROOT is the control and that has no name of its own.
fn labelled(label: &str, widget: Dom) -> Dom {
    captioned(label, widget.with_accessibility_name(label))
}

/// A caption above `content`, leaving its accessibility alone: for a control
/// that already has a name (its own text, or `.with_accessibility_name` on
/// its builder - which reaches the node that carries the role, where a name
/// patched onto the root may not), and for content that is not one control.
fn captioned(label: &str, content: Dom) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; margin-bottom: 16px;")
        .with_child(Dom::create_span_with_text(label).with_css(
            "font-size: 12px; font-weight: bold; color: system:secondary-text; \
             margin-bottom: 6px;",
        ))
        .with_child(content)
}

/// One spinner of the Spinner row, its indicator named under it.
fn spinner_sample(name: &str, spinner: Dom) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; align-items: center; gap: 6px;")
        .with_child(spinner)
        .with_child(
            Dom::create_span_with_text(name)
                .with_css("font-size: 11px; color: system:tertiary-text;"),
        )
}

fn section(title: &str, items: Vec<Dom>) -> Dom {
    let mut col =
        Dom::create_div()
            .with_css(
                "display: flex; flex-direction: column; background-color: \
                 system:window-background; border-radius: 10px; padding: 18px; margin-bottom: \
                 20px;",
            )
            // A section title is a heading (the UA gives h2 a top margin;
            // the card's padding is the spacing here).
            .with_child(Dom::create_h2_with_text(title).with_css(
                "font-size: 18px; font-weight: bold; color: system:text; margin-top: 0px; \
                 margin-bottom: 14px;",
            ));
    for it in items {
        col = col.with_child(it);
    }
    col
}

fn dock_zones(theme: UiTheme) -> Dom {
    let zone = |name: &str, child: Option<Dom>| {
        let mut z = Dom::create_div()
            .with_attributes(vec![AttributeType::custom(AttributeNameValue {
                attr_name: "id".into(),
                value: name.into(),
            })])
            .with_ids_and_classes(vec![IdOrClass::class("dock-zone")])
            .with_css(
                "flex: 1; min-height: 160px; border: 1px dashed system:tertiary-text; \
                 border-radius: 8px; padding: 6px; background-color: system:control-background;",
            );
        if let Some(c) = child {
            z = z.with_child(c);
        }
        z
    };
    let panel = Dom::create_node(NodeType::transient_window(
        TransientWindowConfig::opened()
            .with_dock(TransientDock::inline())
            .with_tearoff(TransientTearoff::zone())
            .with_material(azul::css::WindowBackgroundMaterial::Transparent),
    ))
    .with_attributes(vec![
        AttributeType::title("Tools"),
        AttributeType::custom(AttributeNameValue {
            attr_name: "tearoff-zone".into(),
            value: ".dock-zone".into(),
        }),
    ])
    .with_css(
        "display: flex; flex-direction: column; background-color: system:window-background; \
         border: 1px solid system:separator; border-radius: 6px; box-shadow: 0px 1px 3px \
         rgba(16, 24, 40, 0.1);",
    )
    .with_child(
        Dom::create_div()
            .with_css(
                "display: flex; flex-direction: row; align-items: center; justify-content: \
                 center; height: 18px; background-color: system:selection-background-inactive; \
                 border-radius: 6px 6px 0px 0px; cursor: grab; -azul-app-region: drag;",
            )
            .with_child(Dom::create_div().with_css(
                "width: 36px; height: 4px; border-radius: 2px; background-color: \
                 system:tertiary-text;",
            )),
    )
    .with_child(
        Dom::create_div()
            .with_css("display: flex; flex-direction: column; gap: 6px; padding: 10px;")
            .with_child(Dom::create_span_with_text("Tools").with_css(
                "font-weight: bold; color: system:text;",
            ))
            .with_child(Dom::create_span_with_text("Drag the grip bar.").with_css(
                "font-size: 12px; color: system:secondary-text;",
            ))
            .with_child(Button::create("A tool button").with_theme(theme).dom()),
    );
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; gap: 12px;")
        .with_child(zone("dock-left", Some(panel)))
        .with_child(zone("dock-right", None))
}

fn menu_action(data: &RefAny, label: &'static str) -> StringMenuItem {
    let item_data = RefAny::new((data.clone(), label));
    StringMenuItem::create(label).with_callback(item_data, on_menu_item)
}

extern "C" fn on_menu_item(mut data: RefAny, _: CallbackInfo) -> Update {
    let (mut showcase, label) = match data.downcast_ref::<(RefAny, &'static str)>() {
        Some(pair) => ((*pair).0.clone(), (*pair).1),
        None => return Update::DoNothing,
    };
    if let Some(mut s) = showcase.downcast_mut::<Showcase>() {
        s.menu_status = format!("Chose: {label}").into();
        s.interactions += 1;
        return Update::RefreshDom;
    }
    Update::DoNothing
}

fn context_menu(data: &RefAny) -> Menu {
    Menu::create(vec![
        MenuItem::string(menu_action(data, "Cut")),
        MenuItem::string(menu_action(data, "Copy")),
        MenuItem::string(menu_action(data, "Paste")),
        MenuItem::separator(),
        MenuItem::string(StringMenuItem::create("More").with_children(vec![
            MenuItem::string(menu_action(data, "Duplicate")),
            MenuItem::string(menu_action(data, "Delete")),
        ])),
    ])
}

fn menu_bar(data: &RefAny) -> Menu {
    Menu::create(vec![
        MenuItem::string(StringMenuItem::create("File").with_children(vec![
            MenuItem::string(menu_action(data, "New")),
            MenuItem::string(menu_action(data, "Open")),
            MenuItem::separator(),
            MenuItem::string(menu_action(data, "Quit")),
        ])),
        MenuItem::string(StringMenuItem::create("Edit").with_children(vec![
            MenuItem::string(menu_action(data, "Undo")),
            MenuItem::string(menu_action(data, "Redo")),
        ])),
    ])
}

fn menus_section(data: &RefAny, status: &str) -> Dom {
    let box_ = Dom::create_div()
        .with_css(
            "display: flex; align-items: center; justify-content: center; height: 80px; border: \
             1px dashed system:tertiary-text; border-radius: 8px; background-color: \
             system:control-background; color: system:secondary-text; cursor: context-menu;",
        )
        .with_child(Dom::create_span_with_text(
            "Right-click me for a context menu",
        ))
        .with_context_menu(context_menu(data));
    section(
        "Menus",
        vec![
            labelled("Context menu", box_),
            labelled(
                "Status",
                Dom::create_span_with_text(status).with_css(
                    "color: system:text;",
                ),
            ),
        ],
    )
}

/// The drop zone at rest, and while files hover over it: the field surface,
/// then the accent and the text-selection tint of the desktop.
const DROP_ZONE_IDLE_CSS: &str =
    "display: flex; flex-direction: column; align-items: center; justify-content: center; \
     min-height: 90px; border: 2px dashed system:tertiary-text; border-radius: 8px; \
     background-color: system:control-background; color: system:secondary-text; padding: 12px;";
const DROP_ZONE_HOVER_CSS: &str =
    "display: flex; flex-direction: column; align-items: center; justify-content: center; \
     min-height: 90px; border: 2px dashed system:accent; border-radius: 8px; \
     background-color: system:text-selection-background; color: system:text; padding: 12px;";

extern "C" fn on_file_hover(mut data: RefAny, info: CallbackInfo) -> Update {
    let hovering = info.is_file_drag_active();
    if let Some(mut s) = data.downcast_mut::<Showcase>() {
        if s.file_hovering != hovering {
            s.file_hovering = hovering;
            return Update::RefreshDom;
        }
    }
    Update::DoNothing
}

extern "C" fn on_file_drop(mut data: RefAny, info: CallbackInfo) -> Update {
    let files = info.get_dropped_files();
    if let Some(mut s) = data.downcast_mut::<Showcase>() {
        s.file_hovering = false;
        for f in files.as_ref() {
            s.dropped.push(f.clone());
        }
        s.interactions += 1;
        return Update::RefreshDom;
    }
    Update::DoNothing
}

fn files_section(data: &RefAny, dropped: &[azul::str::String], hovering: bool) -> Dom {
    let mut zone = Dom::create_div()
        .with_css(if hovering {
            DROP_ZONE_HOVER_CSS
        } else {
            DROP_ZONE_IDLE_CSS
        })
        .with_child(Dom::create_span_with_text(if hovering {
            "Release to drop"
        } else {
            "Drag files here from your file manager"
        }));
    zone.add_callback(
        EventFilter::Window(WindowEventFilter::HoveredFile),
        data.clone(),
        on_file_hover,
    );
    zone.add_callback(
        EventFilter::Window(WindowEventFilter::HoveredFileCancelled),
        data.clone(),
        on_file_hover,
    );
    zone.add_callback(
        EventFilter::Window(WindowEventFilter::DroppedFile),
        data.clone(),
        on_file_drop,
    );

    let mut list = Dom::create_div()
        .with_css("display: flex; flex-direction: column; gap: 2px; margin-top: 8px;");
    if dropped.is_empty() {
        list = list.with_child(Dom::create_span_with_text("(nothing dropped yet)").with_css(
            "color: system:tertiary-text; font-size: 12px;",
        ));
    } else {
        for f in dropped {
            list = list.with_child(Dom::create_span_with_text(f.as_str()).with_css(
                "font-size: 12px; color: system:text; font-family: system:monospace;",
            ));
        }
    }
    section(
        "Files",
        vec![labelled("Drop zone", zone), labelled("Dropped files", list)],
    )
}

fn tab_data(data: &RefAny, index: usize) -> RefAny {
    RefAny::new((data.clone(), index))
}

fn tab_index_of(data: &mut RefAny) -> Option<usize> {
    data.downcast_ref::<(RefAny, usize)>().map(|p| (*p).1)
}
fn tab_showcase_of(data: &mut RefAny) -> Option<RefAny> {
    data.downcast_ref::<(RefAny, usize)>()
        .map(|p| (*p).0.clone())
}

extern "C" fn on_tab_click(mut data: RefAny, _: CallbackInfo) -> Update {
    let (Some(mut sc), Some(idx)) = (tab_showcase_of(&mut data), tab_index_of(&mut data)) else {
        return Update::DoNothing;
    };
    if let Some(mut s) = sc.downcast_mut::<Showcase>() {
        if s.active_tab != idx {
            s.active_tab = idx;
            return Update::RefreshDom;
        }
    }
    Update::DoNothing
}

extern "C" fn on_tab_drag_start(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let (Some(mut sc), Some(idx)) = (tab_showcase_of(&mut data), tab_index_of(&mut data)) else {
        return Update::DoNothing;
    };
    let mime = azul::str::String::from("application/x-azul-tab");
    info.set_drag_data(mime, format!("{idx}").into_bytes());
    if let Some(mut s) = sc.downcast_mut::<Showcase>() {
        s.drag_tab = idx;
    }
    Update::DoNothing
}

extern "C" fn on_tab_drag_over(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.accept_drop();
    Update::DoNothing
}

extern "C" fn on_tab_drop(mut data: RefAny, _: CallbackInfo) -> Update {
    let (Some(mut sc), Some(target)) = (tab_showcase_of(&mut data), tab_index_of(&mut data)) else {
        return Update::DoNothing;
    };
    if let Some(mut s) = sc.downcast_mut::<Showcase>() {
        let src = s.drag_tab;
        s.drag_tab = usize::MAX;
        if src == usize::MAX || src >= s.tabs.len() || target >= s.tabs.len() || src == target {
            return Update::DoNothing;
        }
        let moving = s.tabs.remove(src);
        s.tabs.insert(target, moving);
        let active_label = s.tabs.get(target).cloned();
        if let Some(al) = active_label {
            if let Some(pos) = s.tabs.iter().position(|t| t.as_str() == al.as_str()) {
                s.active_tab = pos;
            }
        }
        s.interactions += 1;
        return Update::RefreshDom;
    }
    Update::DoNothing
}

/// The selected document tab: the pane's own surface, so the two read as one
/// sheet.
const TAB_ACTIVE_CSS: &str =
    "display: flex; align-items: center; padding: 8px 16px; cursor: grab; background-color: \
     system:window-background; color: system:text; font-weight: bold; border-radius: 6px 6px \
     0px 0px; -azul-user-select: none;";
/// A document tab in the background: recessed between the strip and the pane.
const TAB_IDLE_CSS: &str =
    "display: flex; align-items: center; padding: 8px 16px; cursor: grab; background-color: \
     system:under-page-background; color: system:secondary-text; font-weight: normal; \
     border-radius: 6px 6px 0px 0px; -azul-user-select: none;";

fn tabs_section(data: &RefAny, tabs: &[azul::str::String], active: usize) -> Dom {
    let mut strip = Dom::create_div().with_css(
        "display: flex; flex-direction: row; gap: 2px; border-bottom: 1px solid \
         system:separator; background-color: system:background; border-radius: 8px 8px 0px 0px; \
         padding: 4px 4px 0px 4px;",
    );
    for (i, label) in tabs.iter().enumerate() {
        let is_active = i == active;
        let mut tab = Dom::create_div()
            .with_attributes(vec![AttributeType::draggable(true)])
            .with_css(if is_active {
                TAB_ACTIVE_CSS
            } else {
                TAB_IDLE_CSS
            })
            .with_child(Dom::create_span_with_text(label.as_str()));
        tab.add_callback(
            EventFilter::Hover(HoverEventFilter::MouseDown),
            tab_data(data, i),
            on_tab_click,
        );
        tab.add_callback(
            EventFilter::Hover(HoverEventFilter::DragStart),
            tab_data(data, i),
            on_tab_drag_start,
        );
        tab.add_callback(
            EventFilter::Hover(HoverEventFilter::DragOver),
            tab_data(data, i),
            on_tab_drag_over,
        );
        tab.add_callback(
            EventFilter::Hover(HoverEventFilter::Drop),
            tab_data(data, i),
            on_tab_drop,
        );
        strip = strip.with_child(tab);
    }

    let active_label = tabs
        .get(active)
        .map(|t| t.as_str().to_string())
        .unwrap_or_default();
    let pane = Dom::create_div()
        .with_css(
            "min-height: 90px; padding: 16px; background-color: system:window-background; \
             border: 1px solid system:separator; border-top-style: none; border-radius: 0px 0px \
             8px 8px; color: system:secondary-text; font-family: system:monospace;",
        )
        .with_child(
            Dom::create_span_with_text(format!("// {active_label}")).with_css(
                "color: system:text;",
            ),
        );

    let strip_and_pane = Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(strip)
        .with_child(pane);

    section(
        "Documents (VS-style tabs)",
        vec![labelled(
            "Drag a tab onto another to reorder",
            strip_and_pane,
        )],
    )
}

/// A toolbar label.
const TOOLBAR_CAPTION_CSS: &str =
    "font-size: 12px; font-weight: bold; color: system:secondary-text; margin-right: 8px;";

/// The bar under the titlebar: the app's MODE (System / Light / Dark - the
/// engine's `CallbackInfo::set_mode`, for every window) and the app THEME
/// (Flat / Flora - `CallbackInfo::set_theme`; every themed widget on the
/// page is built in it).
///
/// `shown` is the light / dark the window shows, read in `layout()` with
/// `LayoutCallbackInfo::get_mode` so "System (dark)" can say which. Reading
/// it is what makes a mode switch - or a desktop flip while on System -
/// re-run this `layout()`; an app whose `layout()` never reads it is only
/// re-styled, its DOM kept.
fn toolbar(data: &RefAny, mode_index: usize, theme: UiTheme, shown: DarkLightMode) -> Dom {
    let shown = match shown {
        DarkLightMode::Dark => "dark",
        DarkLightMode::Light => "light",
    };
    let mode_note = match mode_index {
        1 | 2 => format!("pinned {shown}"),
        _ => format!("System ({shown})"),
    };
    let theme_index = match theme {
        UiTheme::Flat => 0,
        UiTheme::Flora => 1,
    };
    let group = |caption: &str, control: Dom| {
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: center;")
            .with_child(Dom::create_span_with_text(caption).with_css(TOOLBAR_CAPTION_CSS))
            .with_child(control)
    };
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; align-items: center; gap: 24px; flex-grow: 0; \
             flex-shrink: 0; padding: 8px 24px; border-bottom: 1px solid system:separator; \
             background-color: system:window-background;",
        )
        .with_child(group(
            "Mode",
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-items: center; gap: 8px;")
                .with_child(
                    Segmented::create(strs(&["System", "Light", "Dark"]))
                        .with_selected_index(mode_index)
                        .with_on_change(data.clone(), on_mode)
                        .with_theme(theme)
                        .dom()
                        .with_accessibility_name("Mode"),
                )
                .with_child(
                    Dom::create_span_with_text(mode_note.as_str())
                        .with_css("font-size: 12px; color: system:tertiary-text;"),
                ),
        ))
        .with_child(group(
            "Theme",
            Segmented::create(strs(&["Flat", "Flora"]))
                .with_selected_index(theme_index)
                .with_on_change(data.clone(), on_theme)
                .with_theme(theme)
                .dom()
                .with_accessibility_name("Theme"),
        ))
}

/// The mode segment: the APP-wide light / dark choice, applied by the engine
/// to every window when this returns - a restyle (colours only) for a window
/// whose `layout()` never read the mode. This page reads it (the
/// "System (dark)" note), and the segment is a controlled widget, so the
/// demo asks for its own rebuild as well.
extern "C" fn on_mode(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: SegmentedState,
) -> Update {
    let mode = match state.selected_index {
        1 => OptionDarkLightMode::Some(DarkLightMode::Light),
        2 => OptionDarkLightMode::Some(DarkLightMode::Dark),
        _ => OptionDarkLightMode::None,
    };
    info.set_mode(mode);
    match data.downcast_mut::<Showcase>() {
        Some(mut s) => {
            s.mode_index = state.selected_index;
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

/// The theme segment: the APP theme (`CallbackInfo::set_theme`, every
/// window rebuilt in it - a theme may change a widget's DOM, so this is a
/// rebuild, never a restyle). The page reads it back in `layout()`, so the
/// debug server's `set_theme` and this segment are one switch.
extern "C" fn on_theme(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    info.set_theme(if state.selected_index == 1 { "flora" } else { "flat" });
    bump(&mut data)
}

extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    let s = match data.downcast_ref::<Showcase>() {
        Some(s) => (*s).clone(),
        None => return Dom::create_body(),
    };
    // THE theme of this pass: the app theme (the toolbar's Flat / Flora, or
    // the debug server's `set_theme`); every widget below that has a theme
    // (`with_theme`) is built in it.
    let theme = if info.get_theme().as_str() == "flora" {
        UiTheme::Flora
    } else {
        UiTheme::Flat
    };

    let inputs = section(
        "Inputs",
        vec![
            labelled(
                "TextInput",
                TextInput::create()
                    .with_text(s.text.clone())
                    .with_placeholder("Type something...")
                    .with_on_text_input(data.clone(), on_text_input)
                    .with_theme(theme)
                    .dom(),
            ),
            labelled(
                "NumberInput",
                NumberInput::create(s.number)
                    .with_on_value_change(data.clone(), on_number)
                    .with_theme(theme)
                    .dom(),
            ),
            labelled(
                "TextArea",
                TextArea::create()
                    .with_text(s.textarea_text.clone())
                    .with_placeholder("Multi-line text area...")
                    .with_on_text_input(data.clone(), on_textarea_input)
                    .with_on_focus_lost(
                        data.clone(),
                        on_textarea_focus_lost,
                    )
                    .with_theme(theme)
                    .dom(),
            ),
            captioned(
                "ColorInput",
                ColorInput::create(s.color)
                    .with_accessibility_name("Accent colour")
                    .with_on_value_change(data.clone(), on_color)
                    .with_theme(theme)
                    .dom(),
            ),
            captioned(
                "Slider",
                Slider::create(s.slider_value, 0.0, 100.0)
                    .with_accessibility_name("Slider")
                    .with_on_value_change(
                        data.clone(),
                        on_slider,
                    )
                    .with_theme(theme)
                    .dom(),
            ),
            captioned(
                "Switch",
                Switch::create(s.switch_on)
                    .with_accessibility_name("Switch")
                    .with_on_toggle(
                        data.clone(),
                        on_switch,
                    )
                    .with_theme(theme)
                    .dom(),
            ),
        ],
    );

    let selection = section(
        "Selection",
        vec![
            captioned(
                "CheckBox",
                CheckBox::create(s.checkbox_checked)
                    .with_accessibility_name("CheckBox")
                    .with_on_toggle(data.clone(), on_checkbox)
                    .with_theme(theme)
                    .dom(),
            ),
            captioned(
                "RadioGroup",
                RadioGroup::create(strs(&["Option A", "Option B", "Option C"]))
                    .with_accessibility_name("RadioGroup")
                    .with_selected_index(s.selected_radio)
                    .with_on_change(
                        data.clone(),
                        on_radio,
                    )
                    .with_theme(theme)
                    .dom(),
            ),
            labelled(
                "Segmented",
                Segmented::create(strs(&["Day", "Week", "Month"]))
                    .with_selected_index(s.selected_segment)
                    .with_on_change(
                        data.clone(),
                        on_segmented,
                    )
                    .with_theme(theme)
                    .dom(),
            ),
            labelled(
                "DropDown",
                DropDown::create(strs(CHOICES))
                    .with_selected(s.selected_choice)
                    .with_on_choice_change(data.clone(), on_dropdown)
                    .with_theme(theme)
                    .dom(),
            ),
            // ComboBox has no widget theme (yet): the same look in both.
            labelled(
                "ComboBox",
                ComboBox::create_with_items(strs(&["Apple", "Banana", "Cherry", "Date"]))
                    .with_placeholder("Pick a fruit")
                    .with_text(s.combo_text.clone())
                    .with_on_select(
                        data.clone(),
                        on_combobox,
                    )
                    .dom(),
            ),
        ],
    );

    let display = section(
        "Display",
        vec![
            labelled(
                "Button (default / primary / danger)",
                Dom::create_div()
                    .with_css("display: flex; flex-direction: row;")
                    .with_child(
                        Button::create("Default")
                            .with_on_click(data.clone(), on_button)
                            .with_theme(theme)
                            .dom()
                            .with_css("margin-right: 8px;"),
                    )
                    .with_child(
                        Button::with_type("Primary", ButtonType::Primary)
                            .with_theme(theme)
                            .dom()
                            .with_css("margin-right: 8px;"),
                    )
                    .with_child(
                        Button::with_type("Danger", ButtonType::Danger)
                            .with_theme(theme)
                            .dom(),
                    ),
            ),
            labelled(
                "Badge",
                Dom::create_div()
                    .with_css("display: flex; flex-direction: row;")
                    .with_child(
                        Badge::with_kind("New", BadgeKind::Primary)
                            .with_theme(theme)
                            .dom()
                            .with_css("margin-right: 8px;"),
                    )
                    .with_child(
                        Badge::with_kind("OK", BadgeKind::Success)
                            .with_theme(theme)
                            .dom()
                            .with_css("margin-right: 8px;"),
                    )
                    .with_child(
                        Badge::with_kind("!", BadgeKind::Danger)
                            .with_theme(theme)
                            .dom(),
                    ),
            ),
            labelled(
                "Chip (removable)",
                Chip::with_kind("Rust", ChipKind::Primary)
                    .with_removable(true)
                    .with_on_remove(
                        data.clone(),
                        on_chip_remove,
                    )
                    .with_theme(theme)
                    .dom(),
            ),
            labelled(
                "Avatar",
                Avatar::create("FS")
                    .with_size(AvatarSize::Large)
                    .with_theme(theme)
                    .dom(),
            ),
            labelled(
                "Card",
                Card::create(Dom::create_p_with_text("Card body content").with_css("margin: 0px;"))
                    .with_flex_grow(0.0)
                    .with_theme(theme)
                    .dom(),
            ),
            // A group box: the title names the group, the content is its own.
            captioned(
                "Frame",
                Frame::create(
                    "Shipping",
                    Dom::create_p_with_text("A titled group of related controls.")
                        .with_css("margin: 0px;"),
                )
                .with_flex_grow(0.0)
                .with_theme(theme)
                .dom(),
            ),
            labelled("Divider", Divider::create().with_theme(theme).dom()),
            captioned(
                "ProgressBar",
                ProgressBar::create(s.progress)
                    .with_accessibility_name("ProgressBar")
                    .with_theme(theme)
                    .dom()
                    .with_css("width: 240px;"),
            ),
            // No colour of its own: each theme paints its native ink (flat's
            // ring in the desktop accent, flora's spokes in its ink).
            captioned(
                "Spinner (the theme's own / spokes / ring)",
                Dom::create_div()
                    .with_css("display: flex; flex-direction: row; align-items: center; gap: 24px;")
                    .with_child(spinner_sample(
                        "Theme default",
                        Spinner::create().with_theme(theme).dom(),
                    ))
                    .with_child(spinner_sample(
                        "Spokes",
                        Spinner::create()
                            .with_indicator(SpinnerStyle::Spokes)
                            .with_theme(theme)
                            .dom(),
                    ))
                    .with_child(spinner_sample(
                        "Ring",
                        Spinner::create()
                            .with_indicator(SpinnerStyle::Ring)
                            .with_theme(theme)
                            .dom(),
                    )),
            ),
        ],
    );

    let feedback = section(
        "Feedback",
        vec![
            labelled(
                "Alert (dismissible)",
                Alert::with_kind("This is an informational alert.", AlertKind::Info)
                    .with_dismissible(true)
                    .with_on_dismiss(
                        data.clone(),
                        on_alert_dismiss,
                    )
                    .with_theme(theme)
                    .dom(),
            ),
            // The wrapper only listens for the hover that shows the tip; the
            // button inside is the control, named by its own text.
            captioned(
                "Tooltip (hover the button)",
                Tooltip::create(
                    Button::create("Hover me").with_theme(theme).dom(),
                    "I am a tooltip!",
                )
                .with_theme(theme)
                .dom(),
            ),
            labelled(
                "Dialog (modal: Escape, a button or \u{00D7} closes it)",
                Dom::create_div()
                    .with_css("display: flex; flex-direction: column; align-items: flex-start;")
                    .with_child(
                        Dialog::create(dialog_body(&data, theme))
                            .with_title("Delete \u{201C}report.pdf\u{201D}?")
                            .with_invoker(
                                Button::with_type("Delete file\u{2026}", ButtonType::Danger)
                                    .with_theme(theme)
                                    .dom(),
                            )
                            .with_modal(true)
                            .with_close_button(true)
                            .with_on_close(data.clone(), on_dialog_close)
                            .with_theme(theme)
                            .dom(),
                    )
                    .with_child(Dom::create_p_with_text(s.dialog_result.as_str()).with_css(
                        "font-size: 12px; color: system:secondary-text; margin-top: 6px;",
                    )),
            ),
        ],
    );

    let docking = section(
        "Docking",
        vec![labelled(
            "Dockable panel (drag the grip out; drop it on the other zone)",
            dock_zones(theme),
        )],
    );

    let notifications = notifications::notifications_section(&data, &s.notifications, theme);
    let video_card = video::card(&s.video, theme);
    let menus = menus_section(&data, s.menu_status.as_str());
    let hotkey = hotkeys::hotkey_section(&data, &s.hotkey, &info, theme);
    let files = files_section(&data, &s.dropped, s.file_hovering);
    let tabs = tabs_section(&data, &s.tabs, s.active_tab);
    // `get_mode` DECLARES that this DOM depends on the light / dark the
    // window shows (the "System (dark)" note): a mode switch re-runs this
    // `layout()` rather than only re-styling the page.
    let bar = toolbar(&data, s.mode_index, theme, info.get_mode());

    let navigation = section(
        "Navigation",
        vec![
            labelled(
                "Breadcrumb",
                Breadcrumb::create(strs(&["Home", "Library", "Data"]))
                    .with_on_navigate(
                        data.clone(),
                        on_breadcrumb,
                    )
                    .with_theme(theme)
                    .dom(),
            ),
            labelled(
                "Pagination",
                Pagination::create(s.current_page, 10)
                    .with_on_change(
                        data.clone(),
                        on_pagination,
                    )
                    .with_theme(theme)
                    .dom(),
            ),
            labelled(
                "Stepper",
                Stepper::create(strs(&["Cart", "Shipping", "Payment", "Done"]))
                    .with_current_step(s.current_step)
                    .with_on_step_change(
                        data.clone(),
                        on_stepper,
                    )
                    .with_theme(theme)
                    .dom(),
            ),
            labelled(
                "Accordion",
                Accordion::create_with_sections(vec![
                    AccordionSection {
                        title: "What is Azul?".into(),
                        content: Dom::create_p_with_text("A cross-platform Rust GUI framework.")
                            .with_css("margin: 0px;"),
                        count: azul::option::OptionUsize::None,
                        is_open: s.accordion_open.first().copied().unwrap_or(true),
                    },
                    AccordionSection {
                        title: "How do widgets work?".into(),
                        content: Dom::create_p_with_text("Each widget builds a styled Dom.")
                            .with_css("margin: 0px;"),
                        count: azul::option::OptionUsize::None,
                        is_open: s.accordion_open.get(1).copied().unwrap_or(false),
                    },
                ])
                .with_on_toggle(
                    data.clone(),
                    on_accordion,
                )
                .with_theme(theme)
                .dom(),
            ),
        ],
    );

    let overlays = section(
        "Overlays",
        vec![
            labelled(
                "Popover (click outside, Escape or \u{00D7} closes it)",
                Dom::create_div()
                    .with_css("display: flex; flex-direction: column; align-items: flex-start;")
                    .with_child(
                        Dialog::create(Dom::create_p_with_text(
                            "A non-modal dialog below its button.",
                        ))
                        .with_title("Popover")
                        .with_invoker(Button::create("Open popover").with_theme(theme).dom())
                        .with_closed_by(DialogClosedBy::Any)
                        .with_close_button(true)
                        .with_on_close(data.clone(), on_popover_close)
                        .with_theme(theme)
                        .dom(),
                    ),
            ),
            labelled(
                "SplitPane",
                SplitPane::create(
                    SplitDirection::Horizontal,
                    Dom::create_p_with_text("Left pane").with_css("margin: 0px;"),
                    Dom::create_p_with_text("Right pane").with_css("margin: 0px;"),
                )
                .with_ratio(s.split_ratio)
                .with_on_resize(
                    data.clone(),
                    on_splitpane,
                )
                .with_theme(theme)
                .dom()
                .with_css("height: 120px;"),
            ),
        ],
    );

    let datetime = section(
        "Date & Time",
        vec![
            labelled(
                "DatePicker",
                DatePicker::create(s.date.year, s.date.month, s.date.day)
                    .with_on_change(
                        data.clone(),
                        on_datepicker,
                    )
                    .with_theme(theme)
                    .dom(),
            ),
            labelled(
                "TimePicker (24h)",
                TimePicker::create(s.time.hour, s.time.minute)
                    .with_24h(s.time.is_24h)
                    .with_pm(s.time.is_pm)
                    .with_on_change(
                        data.clone(),
                        on_timepicker,
                    )
                    .with_theme(theme)
                    .dom(),
            ),
        ],
    );

    // Last on the page: every HTML input type, as widgets in a Form and as
    // raw HTML the engine turns into the same widgets.
    let every_input = forms::every_input_section(&data, &s.form, theme);
    let raw_inputs = forms::raw_inputs_section(&data, &s.form, theme);
    // The Outlook-style mail widgets.
    let mail = mail::mail_section(&data, &s.mail, theme);
    // The wizard, settings and standard dialogs.
    let dialogs = dialogs::dialogs_section(&data, &s.dialogs, theme);
    // The building blocks the apps share (rich text, selection, close guard, ...).
    let blocks = blocks::blocks_section(&data, &s.blocks, theme);

    let heading = Dom::create_h1_with_text("Azul Widget Showcase").with_css(
        "font-size: 26px; font-weight: bold; color: system:text; margin-top: 0px; \
         margin-bottom: 4px;",
    );
    let subtitle = Dom::create_p_with_text(
        format!(
            "Every built-in widget (callbacks fired so far: {})",
            s.interactions
        )
        .as_str(),
    )
    .with_css(
        "font-size: 13px; color: system:secondary-text; margin-top: 0px; margin-bottom: 20px;",
    );

    // The bar this window draws under `WindowDecorations::NoTitle`, matched to
    // the one AppKit draws: 28px, the height of a titlebar WITHOUT a toolbar,
    // whose midline the traffic lights sit on (38 is the height with a compact
    // toolbar, and put the title 5px below the lights). No fill of its own;
    // the system separator, inside the 28px. The title is the system's bold
    // title face, centred on the WINDOW: the padding is the same on both
    // sides, the left one keeping it clear of the traffic lights (x 8 to 60),
    // and the label is taken out of the row so it cannot push the title over.
    let titlebar = Dom::create_div()
        .with_css(
            "height: 28px; box-sizing: border-box; flex-grow: 0; flex-shrink: 0; \
             display: flex; flex-direction: row; align-items: center; \
             position: relative; padding-left: 78px; padding-right: 78px; \
             border-bottom: 0.5px solid system:separator; cursor: grab; \
             user-select: none; -azul-app-region: drag;",
        )
        .with_child(
            Dom::create_span_with_text("Azul Widget Showcase").with_css(
                "font-family: system:title:bold; font-size: 13px; color: system:text; \
                 flex-grow: 1; flex-basis: 0px; min-width: 0px; text-align: center; \
                 white-space: nowrap; overflow: hidden;",
            ),
        )
        .with_child(Dom::create_span_with_text("custom titlebar").with_css(
            "position: absolute; top: 0px; right: 12px; line-height: 28px; \
             font-size: 11px; color: system:tertiary-text; -azul-app-region: no-drag;",
        ));

    Dom::create_body()
        .with_menu_bar(menu_bar(&data))
        .with_css(
            "margin: 0; font-family: system:ui; color: system:text; background-color: \
             system:background; display: flex; flex-direction: column; height: 100%;",
        )
        .with_child(titlebar)
        .with_child(bar)
        .with_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: column; overflow-y: auto; flex-grow: 1; \
                     min-height: 0; padding: 24px;",
                )
                .with_child(heading)
                .with_child(subtitle)
                .with_child(inputs)
                .with_child(selection)
                .with_child(display)
                .with_child(video_card)
                .with_child(feedback)
                .with_child(notifications)
                .with_child(menus)
                .with_child(hotkey)
                .with_child(files)
                .with_child(tabs)
                .with_child(docking)
                .with_child(navigation)
                .with_child(overlays)
                .with_child(datetime)
                .with_child(mail)
                .with_child(dialogs)
                .with_child(blocks)
                .with_child(every_input)
                .with_child(raw_inputs),
        )
}

/// THE way a section keeps what a widget reported: `put` writes it into the
/// showcase, the interaction counter goes up, the page is rebuilt. Every
/// section's callbacks go through it (no per-section copy).
pub(crate) fn keep(data: &mut RefAny, put: impl FnOnce(&mut Showcase)) -> Update {
    match data.downcast_mut::<Showcase>() {
        Some(mut s) => {
            put(&mut *s);
            s.interactions += 1;
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

fn bump(data: &mut RefAny) -> Update {
    keep(data, |_| {})
}

extern "C" fn on_button(mut data: RefAny, _: CallbackInfo) -> Update {
    bump(&mut data)
}
extern "C" fn on_checkbox(mut data: RefAny, _: CallbackInfo, state: CheckBoxState) -> Update {
    match data.downcast_mut::<Showcase>() {
        Some(mut s) => {
            s.checkbox_checked = state.checked;
            s.interactions += 1;
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}
extern "C" fn on_dropdown(mut data: RefAny, _: CallbackInfo, choice: usize) -> Update {
    match data.downcast_mut::<Showcase>() {
        Some(mut s) => {
            s.selected_choice = choice;
            s.interactions += 1;
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

extern "C" fn on_switch(mut data: RefAny, _: CallbackInfo, state: SwitchState) -> Update {
    if let Some(mut s) = data.downcast_mut::<Showcase>() {
        s.switch_on = state.checked;
    }
    bump(&mut data)
}
extern "C" fn on_slider(mut data: RefAny, _: CallbackInfo, state: SliderState) -> Update {
    if let Some(mut s) = data.downcast_mut::<Showcase>() {
        s.slider_value = state.value;
    }
    bump(&mut data)
}
extern "C" fn on_number(mut data: RefAny, _: CallbackInfo, state: NumberInputState) -> Update {
    if let Some(mut s) = data.downcast_mut::<Showcase>() {
        s.number = state.number;
        s.interactions += 1;
    }
    Update::DoNothing
}

extern "C" fn on_text_input(
    mut data: RefAny,
    _: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<Showcase>() {
        s.text = state.get_text().as_str().into();
        s.interactions += 1;
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_color(mut data: RefAny, _: CallbackInfo, state: ColorInputState) -> Update {
    match data.downcast_mut::<Showcase>() {
        Some(mut s) => {
            s.color = state.color;
            s.interactions += 1;
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}
extern "C" fn on_segmented(mut data: RefAny, _: CallbackInfo, state: SegmentedState) -> Update {
    if let Some(mut s) = data.downcast_mut::<Showcase>() {
        s.selected_segment = state.selected_index;
    }
    bump(&mut data)
}
extern "C" fn on_radio(mut data: RefAny, _: CallbackInfo, state: RadioGroupState) -> Update {
    if let Some(mut s) = data.downcast_mut::<Showcase>() {
        s.selected_radio = state.selected_index;
    }
    bump(&mut data)
}
extern "C" fn on_textarea_input(
    mut data: RefAny,
    _: CallbackInfo,
    state: TextAreaState,
) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<Showcase>() {
        s.textarea_text = state.get_text();
        s.interactions += 1;
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}
extern "C" fn on_textarea_focus_lost(
    mut data: RefAny,
    _: CallbackInfo,
    _: TextAreaState,
) -> Update {
    bump(&mut data)
}
extern "C" fn on_combobox(mut data: RefAny, _: CallbackInfo, state: ComboBoxState) -> Update {
    if let Some(mut s) = data.downcast_mut::<Showcase>() {
        s.combo_text = state.text;
    }
    bump(&mut data)
}
extern "C" fn on_chip_remove(mut data: RefAny, _: CallbackInfo, _: ChipState) -> Update {
    bump(&mut data)
}
extern "C" fn on_alert_dismiss(mut data: RefAny, _: CallbackInfo, _: AlertState) -> Update {
    bump(&mut data)
}
/// The modal dialog's content: a message and two buttons that close it
/// with a return value (HTML `dialog.close(value)`).
fn dialog_body(data: &RefAny, theme: UiTheme) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(
            Dom::create_p_with_text("It will be deleted permanently. This cannot be undone.")
                .with_css("color: system:text; margin-bottom: 16px;"),
        )
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; justify-content: flex-end;")
                .with_child(
                    Button::create("Keep")
                        .with_on_click(data.clone(), on_dialog_keep)
                        .with_theme(theme)
                        .dom()
                        .with_css("margin-right: 8px;"),
                )
                .with_child(
                    Button::with_type("Delete", ButtonType::Danger)
                        .with_on_click(data.clone(), on_dialog_delete)
                        .with_theme(theme)
                        .dom(),
                ),
        )
}
extern "C" fn on_dialog_keep(_data: RefAny, info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();
    let _ = Dialog::close_from(info, hit, "keep");
    Update::DoNothing
}
extern "C" fn on_dialog_delete(_data: RefAny, info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();
    let _ = Dialog::close_from(info, hit, "delete");
    Update::DoNothing
}
/// The dialog closed - by a button, Escape or the close button: say how.
extern "C" fn on_dialog_close(mut data: RefAny, _: CallbackInfo, state: DialogState) -> Update {
    let how = if state.return_value.as_str().is_empty() {
        "Dismissed (Escape or \u{00D7}).".to_string()
    } else {
        format!("Closed with \u{201C}{}\u{201D}.", state.return_value.as_str())
    };
    if let Some(mut s) = data.downcast_mut::<Showcase>() {
        s.dialog_result = how.into();
    }
    bump(&mut data)
}
extern "C" fn on_accordion(mut data: RefAny, _: CallbackInfo, index: usize) -> Update {
    if let Some(mut s) = data.downcast_mut::<Showcase>() {
        if let Some(flag) = s.accordion_open.get_mut(index) {
            *flag = !*flag;
        }
    }
    bump(&mut data)
}
extern "C" fn on_breadcrumb(mut data: RefAny, _: CallbackInfo, _: BreadcrumbState) -> Update {
    bump(&mut data)
}
extern "C" fn on_pagination(mut data: RefAny, _: CallbackInfo, state: PaginationState) -> Update {
    if let Some(mut s) = data.downcast_mut::<Showcase>() {
        s.current_page = state.current_page;
    }
    bump(&mut data)
}
extern "C" fn on_stepper(mut data: RefAny, _: CallbackInfo, state: StepperState) -> Update {
    if let Some(mut s) = data.downcast_mut::<Showcase>() {
        s.current_step = state.current_step;
    }
    bump(&mut data)
}
extern "C" fn on_popover_close(mut data: RefAny, _: CallbackInfo, _: DialogState) -> Update {
    bump(&mut data)
}
extern "C" fn on_splitpane(mut data: RefAny, _: CallbackInfo, state: SplitPaneState) -> Update {
    if let Some(mut s) = data.downcast_mut::<Showcase>() {
        s.split_ratio = state.ratio;
    }
    bump(&mut data)
}
extern "C" fn on_datepicker(mut data: RefAny, _: CallbackInfo, state: DatePickerState) -> Update {
    if let Some(mut s) = data.downcast_mut::<Showcase>() {
        s.date = state;
    }
    bump(&mut data)
}
extern "C" fn on_timepicker(mut data: RefAny, _: CallbackInfo, state: TimePickerState) -> Update {
    if let Some(mut s) = data.downcast_mut::<Showcase>() {
        s.time = state;
    }
    bump(&mut data)
}

pub fn start() {
    let data = RefAny::new(Showcase {
        switch_on: true,
        slider_value: 40.0,
        number: 42.0,
        text: "".into(),
        textarea_text: "".into(),
        checkbox_checked: true,
        selected_radio: 0,
        selected_segment: 1,
        selected_choice: 2,
        progress: 65.0,
        current_page: 1,
        current_step: 1,
        interactions: 0,
        color: ColorU {
            r: 255,
            g: 87,
            b: 51,
            a: 255,
        },
        menu_status: "No menu item chosen yet.".into(),
        dropped: Vec::new(),
        file_hovering: false,
        tabs: vec![
            "main.rs".into(),
            "lib.rs".into(),
            "Cargo.toml".into(),
            "README.md".into(),
        ],
        active_tab: 0,
        drag_tab: usize::MAX,
        drag_over: usize::MAX,
        date: DatePickerState {
            year: 2026,
            month: 6,
            day: 23,
        },
        time: TimePickerState {
            hour: 14,
            minute: 30,
            is_pm: false,
            is_24h: true,
        },
        combo_text: "".into(),
        accordion_open: vec![true, false],
        split_ratio: 0.5,
        dialog_result: "Not opened yet.".into(),
        notifications: notifications::NotificationsDemo::probe(),
        video: video::new_state(),
        hotkey: hotkeys::HotkeyDemo::default(),
        // Follow the desktop's light / dark (the toolbar's "System").
        mode_index: 0,
        form: forms::FormDemo::create(),
        mail: mail::MailDemo::create(),
        dialogs: dialogs::DialogsDemo::create(),
        blocks: blocks::BlocksDemo::create(),
    });
    // `None` follows the desktop - the default, spelled out: an app that
    // starts pinned passes `OptionDarkLightMode::Some(DarkLightMode::Dark)`.
    let config = AppConfig::create().with_mode(OptionDarkLightMode::None);
    let app = App::create(data, config);
    let mut window = WindowCreateOptions::create(layout);
    window.window_state.title = "Azul Widget Showcase".into();
    window.window_state.flags.decorations = WindowDecorations::NoTitle;
    app.run(window);
}

#[cfg(target_os = "android")]
#[ctor::ctor]
fn android_ctor() {
    start();
}
