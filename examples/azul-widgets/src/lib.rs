//! AzulWidgets — a single-window showcase of every built-in Azul widget.
//!
//! The window is one scrollable vertical column of labelled sections
//! (Inputs / Selection / Display / Feedback / Navigation / Overlays /
//! Date & Time). Each section instantiates widgets with a sensible config and,
//! where the widget exposes one, a hooked callback (so the callback wiring is
//! compile-checked and the widgets are interactive).
//!
//! All 24 high-level widgets are demoed —
//!   Switch, Divider, Card, Badge, Slider, Segmented, RadioGroup, Tooltip,
//!   TextArea, Alert, Accordion, Avatar, Chip, Spinner, Popover, ComboBox,
//!   Modal, Toast, Breadcrumb, Pagination, Stepper, SplitPane, DatePicker,
//!   TimePicker
//! — alongside the common existing ones (Button, CheckBox, ProgressBar,
//! TextInput, NumberInput, ColorInput, DropDown).

use azul::prelude::*;
use azul::widgets::*;
// The high-level widgets' on_* setters take a *wrapper* callback struct
// (`{ cb, callable }`) rather than a bare fn pointer; those structs live in
// `azul::dom`. (The existing widgets — Button/CheckBox/DropDown — instead take
// a bare fn type, so they need nothing from here.)
use azul::dom::{
    AttributeNameValue, AttributeType, IdOrClass, NodeType,
    AccordionOnToggleCallback, AlertOnDismissCallback, BreadcrumbOnNavigateCallback,
    ChipOnRemoveCallback, ComboBoxOnSelectCallback, DatePickerOnChangeCallback,
    ModalOnCloseCallback, PaginationOnChangeCallback, PopoverOnToggleCallback,
    RadioGroupOnChangeCallback, SegmentedOnChangeCallback, SliderOnValueChangeCallback,
    SplitPaneOnResizeCallback, StepperOnStepChangeCallback, SwitchOnToggleCallback,
    TextAreaOnFocusLostCallback, TimePickerOnChangeCallback, ToastOnDismissCallback,
};

use azul::menu::{Menu, MenuItem, StringMenuItem};
use azul::misc::{TransientDock, TransientTearoff};
use azul::window::TransientWindowConfig;

// ───────────────────────── Model (source of truth) ─────────────────────────

#[derive(Clone)]
struct Showcase {
    switch_on: bool,
    slider_value: f32,
    checkbox_checked: bool,
    selected_radio: usize,
    selected_segment: usize,
    selected_choice: usize,
    progress: f32,
    current_page: usize,
    current_step: usize,
    /// Bumped by every hooked callback so the UI shows that callbacks fire.
    interactions: usize,
    /// The colour picked in the `ColorInput`'s popup; the swatch shows it.
    color: ColorU,
    /// The last menu-bar / context-menu item chosen (shown in the Menus section).
    menu_status: azul::str::String,
    /// Files the user has dropped onto the drop zone.
    dropped: Vec<azul::str::String>,
    /// Whether a file is being hovered over the drop zone right now.
    file_hovering: bool,
    /// VS-style document tabs (labels), reorderable + tear-off-able.
    tabs: Vec<azul::str::String>,
    /// Which tab is active.
    active_tab: usize,
    /// The tab index a reorder drag started on (set on `DragStart`, read on
    /// `Drop`). `usize::MAX` = no drag in flight.
    drag_tab: usize,
    /// The tab the dragged tab is currently over (the insertion point). Drives
    /// the drop indicator. `usize::MAX` = none.
    drag_over: usize,
}

const CHOICES: &[&str] = &["Red", "Green", "Blue"];

// ───────────────────────────── DOM helpers ─────────────────────────────────

/// Build an azul `StringVec`-compatible vector from string literals.
fn strs(items: &[&str]) -> Vec<azul::str::String> {
    items.iter().map(|s| (*s).into()).collect()
}

/// A small caption above a widget, so each entry in a section is labelled.
fn labelled(label: &str, widget: Dom) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; margin-bottom: 16px;")
        .with_child(
            // The caption is one of two flex items, so the styling has to land
            // on a real box: a bare text node has none, and the margin, weight
            // and colour would all be inert. A SPAN rather than a div — this is
            // a label, and a div says nothing about what the text is.
            Dom::create_span_with_text(label)
                .with_css("font-size: 12px; font-weight: bold; color: #667085; margin-bottom: 6px;"),
        )
        // The caption a sighted user reads IS the control's name, so give it to
        // the accessibility tree too. A slider is a track and a thumb with no
        // text of its own, and an icon-only button's label is a glyph: the
        // widget genuinely cannot know what it is called, only this call site
        // does. `with_accessibility_name` MERGES, so the role and live value
        // the widget declared survive.
        .with_child(widget.with_accessibility_name(label))
}

/// A titled card grouping several labelled widgets.
fn section(title: &str, items: Vec<Dom>) -> Dom {
    let mut col = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; background-color: #ffffff; \
             border-radius: 10px; padding: 18px; margin-bottom: 20px;",
        )
        .with_child(
            Dom::create_div_with_text(title).with_css(
                "font-size: 18px; font-weight: bold; color: #1d2939; margin-bottom: 14px;",
            ),
        );
    for it in items {
        col = col.with_child(it);
    }
    col
}

/// Two dock zones side by side, the panel starting in the left one.
fn dock_zones() -> Dom {
    let zone = |name: &str, child: Option<Dom>| {
        let mut z = Dom::create_div()
            .with_attributes(vec![AttributeType::custom(AttributeNameValue {
                attr_name: "id".into(),
                value: name.into(),
            })])
            .with_ids_and_classes(vec![IdOrClass::class("dock-zone")])
            .with_css(
                "flex: 1; min-height: 160px; border: 1px dashed #98a2b3; border-radius: 8px; \
                 padding: 6px; background-color: #f9fafb;",
            );
        if let Some(c) = child {
            z = z.with_child(c);
        }
        z
    };
    let panel = Dom::create_node(NodeType::transient_window(
        TransientWindowConfig::opened()
            .with_dock(TransientDock::inline())
            .with_tearoff(TransientTearoff::zone()),
    ))
    .with_attributes(vec![
        AttributeType::title("Tools"),
        AttributeType::custom(AttributeNameValue { attr_name: "tearoff-zone".into(), value: ".dock-zone".into() }),
    ])
    .with_css(
        "display: flex; flex-direction: column; background-color: #ffffff; border: 1px solid #d0d5dd; \
         border-radius: 6px; box-shadow: 0px 1px 3px rgba(16, 24, 40, 0.1);",
    )
    .with_child(
        Dom::create_div()
            .with_css(
                "display: flex; flex-direction: row; align-items: center; justify-content: center; \
                 height: 18px; background-color: #eaecf0; border-radius: 6px 6px 0px 0px; cursor: grab; \
                 -azul-app-region: drag;",
            )
            .with_child(Dom::create_div().with_css("width: 36px; height: 4px; border-radius: 2px; background-color: #98a2b3;")),
    )
    .with_child(
        Dom::create_div()
            .with_css("display: flex; flex-direction: column; gap: 6px; padding: 10px;")
            .with_child(Dom::create_span_with_text("Tools").with_css("font-weight: bold; color: #1d2939;"))
            .with_child(Dom::create_span_with_text("Drag the grip bar.").with_css("font-size: 12px; color: #475467;"))
            .with_child(Button::create("A tool button").dom()),
    );
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; gap: 12px;")
        .with_child(zone("dock-left", Some(panel)))
        .with_child(zone("dock-right", None))
}

// ─────────────────────────── Menus + context menu ──────────────────────────

/// A menu item whose click records `label` into `menu_status`. The label is
/// carried in a tiny per-item RefAny so one callback serves every item.
fn menu_action(data: &RefAny, label: &'static str) -> StringMenuItem {
    // Pack (Showcase, label) — the item's callback reads the label back.
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

/// The right-click context menu for the Menus box: a couple of actions, a
/// separator, and a submenu — the same `Menu` a native menu bar uses.
fn context_menu(data: &RefAny) -> Menu {
    Menu::create(vec![
        MenuItem::string(menu_action(data, "Cut")),
        MenuItem::string(menu_action(data, "Copy")),
        MenuItem::string(menu_action(data, "Paste")),
        MenuItem::separator(),
        MenuItem::string(
            StringMenuItem::create("More")
                .with_children(vec![
                    MenuItem::string(menu_action(data, "Duplicate")),
                    MenuItem::string(menu_action(data, "Delete")),
                ]),
        ),
    ])
}

/// The window menu bar: File / Edit, wired to the same status line.
fn menu_bar(data: &RefAny) -> Menu {
    Menu::create(vec![
        MenuItem::string(
            StringMenuItem::create("File").with_children(vec![
                MenuItem::string(menu_action(data, "New")),
                MenuItem::string(menu_action(data, "Open")),
                MenuItem::separator(),
                MenuItem::string(menu_action(data, "Quit")),
            ]),
        ),
        MenuItem::string(
            StringMenuItem::create("Edit").with_children(vec![
                MenuItem::string(menu_action(data, "Undo")),
                MenuItem::string(menu_action(data, "Redo")),
            ]),
        ),
    ])
}

/// The Menus section: a box that opens the context menu on right-click.
fn menus_section(data: &RefAny, status: &str) -> Dom {
    let box_ = Dom::create_div()
        .with_css(
            "display: flex; align-items: center; justify-content: center; height: 80px; \
             border: 1px dashed #98a2b3; border-radius: 8px; background-color: #f9fafb; \
             color: #475467; cursor: context-menu;",
        )
        .with_child(Dom::create_span_with_text("Right-click me for a context menu"))
        .with_context_menu(context_menu(data));
    section(
        "Menus",
        vec![
            labelled("Context menu", box_),
            labelled("Status", Dom::create_span_with_text(status).with_css("color: #1d2939;")),
        ],
    )
}

// ─────────────────────────────── File drop ─────────────────────────────────

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

/// The Files section: a drop zone that lists the files dropped onto it.
fn files_section(data: &RefAny, dropped: &[azul::str::String], hovering: bool) -> Dom {
    let bg = if hovering { "#eef4ff" } else { "#f9fafb" };
    let border = if hovering { "#2970ff" } else { "#98a2b3" };
    let mut zone = Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: column; align-items: center; justify-content: center; \
             min-height: 90px; border: 2px dashed {border}; border-radius: 8px; background-color: {bg}; \
             color: #475467; padding: 12px;",
        ))
        .with_child(Dom::create_span_with_text(if hovering {
            "Release to drop"
        } else {
            "Drag files here from your file manager"
        }));
    zone.add_callback(EventFilter::Window(WindowEventFilter::HoveredFile), data.clone(), on_file_hover);
    zone.add_callback(EventFilter::Window(WindowEventFilter::HoveredFileCancelled), data.clone(), on_file_hover);
    zone.add_callback(EventFilter::Window(WindowEventFilter::DroppedFile), data.clone(), on_file_drop);

    let mut list = Dom::create_div().with_css("display: flex; flex-direction: column; gap: 2px; margin-top: 8px;");
    if dropped.is_empty() {
        list = list.with_child(Dom::create_span_with_text("(nothing dropped yet)").with_css("color: #98a2b3; font-size: 12px;"));
    } else {
        for f in dropped {
            list = list.with_child(Dom::create_span_with_text(f.as_str()).with_css("font-size: 12px; color: #1d2939; font-family: monospace;"));
        }
    }
    section("Files", vec![labelled("Drop zone", zone), labelled("Dropped files", list)])
}

// ──────────────────────────── VS-style tabs ────────────────────────────────
//
// A document-tab strip: click to switch, drag a tab onto another to reorder
// (exercises the drag-source routing — Drag/DragEnd stick to the tab you
// grabbed, not whatever is under the cursor), and drag a tab out of the strip
// to tear it into its own window (the transient-window tear-off).

/// Pack `(Showcase, tab_index)` so one set of callbacks serves every tab.
fn tab_data(data: &RefAny, index: usize) -> RefAny {
    RefAny::new((data.clone(), index))
}

fn tab_index_of(data: &mut RefAny) -> Option<usize> {
    data.downcast_ref::<(RefAny, usize)>().map(|p| (*p).1)
}
fn tab_showcase_of(data: &mut RefAny) -> Option<RefAny> {
    data.downcast_ref::<(RefAny, usize)>().map(|p| (*p).0.clone())
}

/// Click a tab header → make it the active document.
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

/// A reorder drag began on this tab: remember which one.
extern "C" fn on_tab_drag_start(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let (Some(mut sc), Some(idx)) = (tab_showcase_of(&mut data), tab_index_of(&mut data)) else {
        return Update::DoNothing;
    };
    // Populate the drag payload so DragOver/Drop targets have something to read.
    let mime = azul::str::String::from("application/x-azul-tab");
    info.set_drag_data(mime, format!("{idx}").into_bytes());
    if let Some(mut s) = sc.downcast_mut::<Showcase>() {
        s.drag_tab = idx;
    }
    Update::DoNothing
}

/// A tab is over this tab: accept, so a `Drop` will fire here.
extern "C" fn on_tab_drag_over(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.accept_drop();
    Update::DoNothing
}

/// A tab was dropped on this one: move the dragged tab to this slot.
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
        // Keep the same document active by following its label.
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

/// The VS-style tab section: a reorderable strip over a content pane.
fn tabs_section(data: &RefAny, tabs: &[azul::str::String], active: usize) -> Dom {
    let mut strip = Dom::create_div().with_css(
        "display: flex; flex-direction: row; gap: 2px; border-bottom: 1px solid #d0d5dd; \
         background-color: #f2f4f7; border-radius: 8px 8px 0px 0px; padding: 4px 4px 0px 4px;",
    );
    for (i, label) in tabs.iter().enumerate() {
        let is_active = i == active;
        let (bg, color, weight) = if is_active {
            ("#ffffff", "#1d2939", "bold")
        } else {
            ("#e4e7ec", "#475467", "normal")
        };
        let mut tab = Dom::create_div()
            // `Draggable(true)` makes the press a NODE drag (DragStart/Drag/Drop
            // callbacks) rather than a text selection on the label.
            .with_attributes(vec![AttributeType::draggable(true)])
            .with_css(format!(
                "display: flex; align-items: center; padding: 8px 16px; cursor: grab; \
                 background-color: {bg}; color: {color}; font-weight: {weight}; \
                 border-radius: 6px 6px 0px 0px; -azul-user-select: none;",
            ))
            .with_child(Dom::create_span_with_text(label.as_str()));
        tab.add_callback(EventFilter::Hover(HoverEventFilter::MouseDown), tab_data(data, i), on_tab_click);
        tab.add_callback(EventFilter::Hover(HoverEventFilter::DragStart), tab_data(data, i), on_tab_drag_start);
        tab.add_callback(EventFilter::Hover(HoverEventFilter::DragOver), tab_data(data, i), on_tab_drag_over);
        tab.add_callback(EventFilter::Hover(HoverEventFilter::Drop), tab_data(data, i), on_tab_drop);
        strip = strip.with_child(tab);
    }

    let active_label = tabs.get(active).map(|t| t.as_str().to_string()).unwrap_or_default();
    let pane = Dom::create_div()
        .with_css(
            "min-height: 90px; padding: 16px; background-color: #ffffff; \
             border: 1px solid #d0d5dd; border-top: none; border-radius: 0px 0px 8px 8px; \
             color: #475467; font-family: monospace;",
        )
        .with_child(Dom::create_span_with_text(format!("// {active_label}")).with_css("color: #1d2939;"));

    let strip_and_pane = Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(strip)
        .with_child(pane);

    section(
        "Documents (VS-style tabs)",
        vec![
            labelled("Drag a tab onto another to reorder", strip_and_pane),
        ],
    )
}

// ──────────────────────────── Layout callback ──────────────────────────────

extern "C" fn layout(mut data: RefAny, _: LayoutCallbackInfo) -> Dom {
    let s = match data.downcast_ref::<Showcase>() {
        Some(s) => (*s).clone(),
        None => return Dom::create_body(),
    };

    // ── Inputs ──────────────────────────────────────────────────────────
    let inputs = section(
        "Inputs",
        vec![
            labelled(
                "TextInput",
                TextInput::create()
                    .with_placeholder("Type something...")
                    .dom(),
            ),
            labelled("NumberInput", NumberInput::create(42.0).dom()),
            labelled(
                "ColorInput",
                ColorInput::create(s.color)
                    .with_accessibility_name("Accent colour")
                    .with_on_value_change(data.clone(), on_color)
                    .dom(),
            ),
            labelled(
                "TextArea",
                TextArea::create()
                    .with_placeholder("Multi-line text area...")
                    .with_on_focus_lost(
                        data.clone(),
                        TextAreaOnFocusLostCallback { cb: on_textarea_focus_lost, callable: OptionRefAny::None },
                    )
                    .dom(),
            ),
            labelled(
                "Slider",
                Slider::create(s.slider_value, 0.0, 100.0)
                    .with_on_value_change(
                        data.clone(),
                        SliderOnValueChangeCallback { cb: on_slider, callable: OptionRefAny::None },
                    )
                    .dom(),
            ),
            labelled(
                "Switch",
                Switch::create(s.switch_on)
                    .with_on_toggle(
                        data.clone(),
                        SwitchOnToggleCallback { cb: on_switch, callable: OptionRefAny::None },
                    )
                    .dom(),
            ),
        ],
    );

    // ── Selection ───────────────────────────────────────────────────────
    let selection = section(
        "Selection",
        vec![
            labelled(
                "CheckBox",
                CheckBox::create(s.checkbox_checked)
                    .with_on_toggle(data.clone(), on_checkbox)
                    .dom(),
            ),
            labelled(
                "RadioGroup",
                RadioGroup::create(strs(&["Option A", "Option B", "Option C"]))
                    .with_selected_index(s.selected_radio)
                    .with_on_change(
                        data.clone(),
                        RadioGroupOnChangeCallback { cb: on_radio, callable: OptionRefAny::None },
                    )
                    .dom(),
            ),
            labelled(
                "Segmented",
                Segmented::create(strs(&["Day", "Week", "Month"]))
                    .with_selected_index(s.selected_segment)
                    .with_on_change(
                        data.clone(),
                        SegmentedOnChangeCallback { cb: on_segmented, callable: OptionRefAny::None },
                    )
                    .dom(),
            ),
            labelled(
                "DropDown",
                DropDown::create(strs(CHOICES))
                    .with_on_choice_change(data.clone(), on_dropdown)
                    .dom(),
            ),
            labelled(
                "ComboBox",
                ComboBox::new(strs(&["Apple", "Banana", "Cherry", "Date"]))
                    .with_placeholder("Pick a fruit")
                    .with_on_select(
                        data.clone(),
                        ComboBoxOnSelectCallback { cb: on_combobox, callable: OptionRefAny::None },
                    )
                    .dom(),
            ),
        ],
    );

    // ── Display ─────────────────────────────────────────────────────────
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
                            .dom()
                            .with_css("margin-right: 8px;"),
                    )
                    .with_child(
                        Button::with_type("Primary", ButtonType::Primary)
                            .dom()
                            .with_css("margin-right: 8px;"),
                    )
                    .with_child(Button::with_type("Danger", ButtonType::Danger).dom()),
            ),
            labelled(
                "Badge",
                Dom::create_div()
                    .with_css("display: flex; flex-direction: row;")
                    .with_child(
                        Badge::with_kind("New", BadgeKind::Primary)
                            .dom()
                            .with_css("margin-right: 8px;"),
                    )
                    .with_child(
                        Badge::with_kind("OK", BadgeKind::Success)
                            .dom()
                            .with_css("margin-right: 8px;"),
                    )
                    .with_child(Badge::with_kind("!", BadgeKind::Danger).dom()),
            ),
            labelled(
                "Chip (removable)",
                Chip::with_kind("Rust", ChipKind::Primary)
                    .with_removable(true)
                    .with_on_remove(
                        data.clone(),
                        ChipOnRemoveCallback { cb: on_chip_remove, callable: OptionRefAny::None },
                    )
                    .dom(),
            ),
            labelled("Avatar", Avatar::create("FS").with_size(AvatarSize::Large).dom()),
            labelled(
                "Card",
                Card::create(Dom::create_div_with_text("Card body content"))
                    .with_flex_grow(0.0)
                    .dom(),
            ),
            labelled("Divider", Divider::create().dom()),
            labelled(
                "ProgressBar",
                ProgressBar::create(s.progress).dom().with_css("width: 240px;"),
            ),
            labelled(
                "Spinner",
                Spinner::create()
                    .with_spinner_size(32)
                    .with_color(ColorU { r: 33, g: 150, b: 243, a: 255 })
                    .dom(),
            ),
        ],
    );

    // ── Feedback ────────────────────────────────────────────────────────
    // Modal is created with `open = false` so it doesn't cover the showcase.
    let feedback = section(
        "Feedback",
        vec![
            labelled(
                "Alert (dismissible)",
                Alert::with_kind("This is an informational alert.", AlertKind::Info)
                    .with_dismissible(true)
                    .with_on_dismiss(
                        data.clone(),
                        AlertOnDismissCallback { cb: on_alert_dismiss, callable: OptionRefAny::None },
                    )
                    .dom(),
            ),
            labelled(
                "Toast",
                Toast::with_kind("Saved successfully", ToastKind::Success)
                    .with_dismissible(true)
                    .with_on_dismiss(
                        data.clone(),
                        ToastOnDismissCallback { cb: on_toast_dismiss, callable: OptionRefAny::None },
                    )
                    .dom(),
            ),
            labelled(
                "Tooltip (hover the button)",
                Tooltip::new(Button::create("Hover me").dom(), "I am a tooltip!").dom(),
            ),
            labelled(
                "Modal (starts closed)",
                Modal::create(Dom::create_div_with_text("Modal body goes here."))
                    .with_title("Example dialog")
                    .with_open(false)
                    .with_close_button(true)
                    .with_on_close(
                        data.clone(),
                        ModalOnCloseCallback { cb: on_modal_close, callable: OptionRefAny::None },
                    )
                    .dom(),
            ),
        ],
    );

    // ── Docking ─────────────────────────────────────────────────────────
    // A tool panel that is CONTENT of whichever dock zone it sits in
    // (`dock="inline"`): drag its grip out of the window to float it, drop
    // the floating palette on the other zone to move it there. The app's
    // DOM never changes - the engine re-parents the subtree in the layout.
    let docking = section(
        "Docking",
        vec![labelled("Dockable panel (drag the grip out; drop it on the other zone)", dock_zones())],
    );

    // ── Menus + Files + Tabs ────────────────────────────────────────────
    let menus = menus_section(&data, s.menu_status.as_str());
    let files = files_section(&data, &s.dropped, s.file_hovering);
    let tabs = tabs_section(&data, &s.tabs, s.active_tab);

    // ── Navigation ──────────────────────────────────────────────────────
    let navigation = section(
        "Navigation",
        vec![
            labelled(
                "Breadcrumb",
                Breadcrumb::create(strs(&["Home", "Library", "Data"]))
                    .with_on_navigate(
                        data.clone(),
                        BreadcrumbOnNavigateCallback { cb: on_breadcrumb, callable: OptionRefAny::None },
                    )
                    .dom(),
            ),
            labelled(
                "Pagination",
                Pagination::create(s.current_page, 10)
                    .with_on_change(
                        data.clone(),
                        PaginationOnChangeCallback { cb: on_pagination, callable: OptionRefAny::None },
                    )
                    .dom(),
            ),
            labelled(
                "Stepper",
                Stepper::create(strs(&["Cart", "Shipping", "Payment", "Done"]))
                    .with_current_step(s.current_step)
                    .with_on_step_change(
                        data.clone(),
                        StepperOnStepChangeCallback { cb: on_stepper, callable: OptionRefAny::None },
                    )
                    .dom(),
            ),
            labelled(
                "Accordion",
                Accordion::new(vec![
                    AccordionSection {
                        title: "What is Azul?".into(),
                        content: Dom::create_div_with_text("A cross-platform Rust GUI framework."),
                        is_open: true,
                    },
                    AccordionSection {
                        title: "How do widgets work?".into(),
                        content: Dom::create_div_with_text("Each widget builds a styled Dom."),
                        is_open: false,
                    },
                ])
                .with_on_toggle(
                    data.clone(),
                    AccordionOnToggleCallback { cb: on_accordion, callable: OptionRefAny::None },
                )
                .dom(),
            ),
        ],
    );

    // ── Overlays ────────────────────────────────────────────────────────
    // Popover starts closed; SplitPane gets an explicit height to lay out in.
    let overlays = section(
        "Overlays",
        vec![
            labelled(
                "Popover (starts closed)",
                Popover::new(
                    Button::create("Open popover").dom(),
                    Dom::create_div_with_text("Popover content"),
                )
                .with_open(false)
                .with_on_toggle(
                    data.clone(),
                    PopoverOnToggleCallback { cb: on_popover, callable: OptionRefAny::None },
                )
                .dom(),
            ),
            labelled(
                "SplitPane",
                SplitPane::create(
                    SplitDirection::Horizontal,
                    Dom::create_div_with_text("Left pane"),
                    Dom::create_div_with_text("Right pane"),
                )
                .with_ratio(0.5)
                .with_on_resize(
                    data.clone(),
                    SplitPaneOnResizeCallback { cb: on_splitpane, callable: OptionRefAny::None },
                )
                .dom()
                .with_css("height: 120px;"),
            ),
        ],
    );

    // ── Date & Time ─────────────────────────────────────────────────────
    let datetime = section(
        "Date & Time",
        vec![
            labelled(
                "DatePicker",
                DatePicker::create(2026, 6, 23)
                    .with_on_change(
                        data.clone(),
                        DatePickerOnChangeCallback { cb: on_datepicker, callable: OptionRefAny::None },
                    )
                    .dom(),
            ),
            labelled(
                "TimePicker (24h)",
                TimePicker::create(14, 30)
                    .with_24h(true)
                    .with_on_change(
                        data.clone(),
                        TimePickerOnChangeCallback { cb: on_timepicker, callable: OptionRefAny::None },
                    )
                    .dom(),
            ),
        ],
    );

    // ── Header ──────────────────────────────────────────────────────────
    let heading = Dom::create_div_with_text("Azul Widget Showcase")
        .with_css("font-size: 26px; font-weight: bold; color: #101828; margin-bottom: 4px;");
    let subtitle = Dom::create_div_with_text(
        format!("Every built-in widget (callbacks fired so far: {})", s.interactions).as_str(),
    )
    .with_css("font-size: 13px; color: #667085; margin-bottom: 20px;");

    // ── Scrollable column ───────────────────────────────────────────────
    Dom::create_body()
        .with_menu_bar(menu_bar(&data))
        .with_css("font-family: sans-serif; background-color: #f2f4f7;")
        .with_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: column; overflow-y: auto; \
                     height: 100%; padding: 24px;",
                )
                .with_child(heading)
                .with_child(subtitle)
                .with_child(inputs)
                .with_child(selection)
                .with_child(display)
                .with_child(feedback)
                .with_child(menus)
                .with_child(files)
                .with_child(tabs)
                .with_child(docking)
                .with_child(navigation)
                .with_child(overlays)
                .with_child(datetime),
        )
}

// ─────────────────────────────── Callbacks ─────────────────────────────────
//
// The high-level-widget callbacks are intentionally near-no-ops: they bump the
// `interactions` counter (proving the wiring fires + the signatures compile)
// rather than reading each widget's `State`. The three existing widgets
// (Button / CheckBox / DropDown) do the natural state update.

/// Shared helper: bump the interactions counter and refresh.
fn bump(data: &mut RefAny) -> Update {
    match data.downcast_mut::<Showcase>() {
        Some(mut s) => {
            s.interactions += 1;
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

// Existing widgets — bare fn-pointer callbacks.
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

// High-level widgets — wrapper-struct callbacks (third arg is the widget State).
//
// THE CONTROLLED-WIDGET RULE: a widget reports its new state through the
// callback, the app STORES it, and the `RefreshDom` rebuilds the widget from
// the stored value. A callback that only counts and refreshes rebuilds the
// widget at the OLD value — which is what made the Switch look like it did
// not respond to clicks, and the slider leave a thumb at 40 under the
// pointer (demo test 2026-08-21). Every stateful widget below stores first.
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
/// The picker reports every change; storing it is what makes the swatch
/// (and the rest of the UI) follow the pick.
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
extern "C" fn on_textarea_focus_lost(mut data: RefAny, _: CallbackInfo, _: TextAreaState) -> Update {
    bump(&mut data)
}
extern "C" fn on_combobox(mut data: RefAny, _: CallbackInfo, _: ComboBoxState) -> Update {
    bump(&mut data)
}
extern "C" fn on_chip_remove(mut data: RefAny, _: CallbackInfo, _: ChipState) -> Update {
    bump(&mut data)
}
extern "C" fn on_alert_dismiss(mut data: RefAny, _: CallbackInfo, _: AlertState) -> Update {
    bump(&mut data)
}
extern "C" fn on_toast_dismiss(mut data: RefAny, _: CallbackInfo, _: ToastState) -> Update {
    bump(&mut data)
}
extern "C" fn on_modal_close(mut data: RefAny, _: CallbackInfo, _: ModalState) -> Update {
    bump(&mut data)
}
extern "C" fn on_accordion(mut data: RefAny, _: CallbackInfo, _: usize) -> Update {
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
extern "C" fn on_popover(mut data: RefAny, _: CallbackInfo, _: PopoverState) -> Update {
    bump(&mut data)
}
extern "C" fn on_splitpane(mut data: RefAny, _: CallbackInfo, _: SplitPaneState) -> Update {
    bump(&mut data)
}
extern "C" fn on_datepicker(mut data: RefAny, _: CallbackInfo, _: DatePickerState) -> Update {
    bump(&mut data)
}
extern "C" fn on_timepicker(mut data: RefAny, _: CallbackInfo, _: TimePickerState) -> Update {
    bump(&mut data)
}

// ───────────────────────────────── Entry ───────────────────────────────────

/// Start the app. Desktop/iOS: blocks. Android: stashes window options.
pub fn start() {
    let data = RefAny::new(Showcase {
        switch_on: true,
        slider_value: 40.0,
        checkbox_checked: true,
        selected_radio: 0,
        selected_segment: 1,
        selected_choice: 2,
        progress: 65.0,
        current_page: 1,
        current_step: 1,
        interactions: 0,
        color: ColorU { r: 255, g: 87, b: 51, a: 255 },
        menu_status: "No menu item chosen yet.".into(),
        dropped: Vec::new(),
        file_hovering: false,
        tabs: vec!["main.rs".into(), "lib.rs".into(), "Cargo.toml".into(), "README.md".into()],
        active_tab: 0,
        drag_tab: usize::MAX,
        drag_over: usize::MAX,
    });
    let config = AppConfig::create();
    let app = App::create(data, config);
    let window = WindowCreateOptions::create(layout);
    app.run(window);
}

#[cfg(target_os = "android")]
#[ctor::ctor]
fn android_ctor() {
    start();
}
