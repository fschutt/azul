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
    AccordionOnToggleCallback, AlertOnDismissCallback, BreadcrumbOnNavigateCallback,
    ChipOnRemoveCallback, ComboBoxOnSelectCallback, DatePickerOnChangeCallback,
    ModalOnCloseCallback, PaginationOnChangeCallback, PopoverOnToggleCallback,
    RadioGroupOnChangeCallback, SegmentedOnChangeCallback, SliderOnValueChangeCallback,
    SplitPaneOnResizeCallback, StepperOnStepChangeCallback, SwitchOnToggleCallback,
    TextAreaOnFocusLostCallback, TimePickerOnChangeCallback, ToastOnDismissCallback,
};

// ───────────────────────── Model (source of truth) ─────────────────────────

#[derive(Default, Clone)]
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
            Dom::create_text(label)
                .with_css("font-size: 12px; font-weight: bold; color: #667085; margin-bottom: 6px;"),
        )
        .with_child(widget)
}

/// A titled card grouping several labelled widgets.
fn section(title: &str, items: Vec<Dom>) -> Dom {
    let mut col = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; background-color: #ffffff; \
             border-radius: 10px; padding: 18px; margin-bottom: 20px;",
        )
        .with_child(
            Dom::create_text(title).with_css(
                "font-size: 18px; font-weight: bold; color: #1d2939; margin-bottom: 14px;",
            ),
        );
    for it in items {
        col = col.with_child(it);
    }
    col
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
                ColorInput::create(ColorU { r: 255, g: 87, b: 51, a: 255 }).dom(),
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
                Card::create(Dom::create_text("Card body content"))
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
                Modal::create(Dom::create_text("Modal body goes here."))
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
                        content: Dom::create_text("A cross-platform Rust GUI framework."),
                        is_open: true,
                    },
                    AccordionSection {
                        title: "How do widgets work?".into(),
                        content: Dom::create_text("Each widget builds a styled Dom."),
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
                    Dom::create_text("Popover content"),
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
                    Dom::create_text("Left pane"),
                    Dom::create_text("Right pane"),
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
    let heading = Dom::create_text("Azul Widget Showcase")
        .with_css("font-size: 26px; font-weight: bold; color: #101828; margin-bottom: 4px;");
    let subtitle = Dom::create_text(
        format!("Every built-in widget (callbacks fired so far: {})", s.interactions).as_str(),
    )
    .with_css("font-size: 13px; color: #667085; margin-bottom: 20px;");

    // ── Scrollable column ───────────────────────────────────────────────
    Dom::create_body()
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
extern "C" fn on_switch(mut data: RefAny, _: CallbackInfo, _: SwitchState) -> Update {
    bump(&mut data)
}
extern "C" fn on_slider(mut data: RefAny, _: CallbackInfo, _: SliderState) -> Update {
    bump(&mut data)
}
extern "C" fn on_segmented(mut data: RefAny, _: CallbackInfo, _: SegmentedState) -> Update {
    bump(&mut data)
}
extern "C" fn on_radio(mut data: RefAny, _: CallbackInfo, _: RadioGroupState) -> Update {
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
extern "C" fn on_pagination(mut data: RefAny, _: CallbackInfo, _: PaginationState) -> Update {
    bump(&mut data)
}
extern "C" fn on_stepper(mut data: RefAny, _: CallbackInfo, _: StepperState) -> Update {
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
