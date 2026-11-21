//! The Format Cells dialog on screen (Ctrl+1, the launchers of HOME's Font,
//! Alignment and Number groups): a `Modal` with a `TabHeader` over the five
//! tabs of [`crate::format_cells`], each built from the standard widgets
//! (`Segmented`, `CheckBox`, `Button`, `ColorInput` for any colour beyond the
//! presets); OK applies the draft's patches to the selection as one undo
//! step, Cancel / Escape / the close button drop it.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType, ColorInputOnValueChangeCallbackType,
        ModalOnCloseCallbackType, SegmentedOnChangeCallbackType, TabOnClickCallbackType,
    },
    css::ColorU,
    prelude::*,
    widgets::{
        Button, ButtonType, CheckBox, CheckBoxState, ColorInput, ColorInputState, Modal, ModalState, Segmented,
        SegmentedState, TabHeader, TabHeaderState,
    },
};

use azul_appkit::l10n;

use crate::{
    engine::{BorderPreset, HAlign, VAlign},
    format_cells::{FormatDraft, NumberCategory, TABS},
    restyle, strs, with_app, AppState,
};

/// Font colours offered: (name, `#RRGGBB`; `None` = automatic). The names are keys of
/// AzSheets' resources, as every label of the dialog.
const FONT_COLORS: [(&str, Option<&str>); 6] = [
    ("azsheets-color-automatic", None),
    ("azsheets-color-black", Some("#000000")),
    ("azsheets-color-red", Some("#C00000")),
    ("azsheets-color-blue", Some("#2F5597")),
    ("azsheets-color-green", Some("#548235")),
    ("azsheets-color-orange", Some("#C55A11")),
];

/// Fills offered: (name, `#RRGGBB`; `None` = no fill).
const FILLS: [(&str, Option<&str>); 6] = [
    ("azsheets-no-fill", None),
    ("azsheets-color-yellow", Some("#FFF2CC")),
    ("azsheets-color-green", Some("#E2EFDA")),
    ("azsheets-color-blue", Some("#DDEBF7")),
    ("azsheets-color-orange", Some("#FCE4D6")),
    ("azsheets-color-grey", Some("#EDEDED")),
];

/// Border presets offered: (name, preset; `None` = leave the borders).
const BORDERS: [(&str, Option<BorderPreset>); 6] = [
    ("azsheets-border-unchanged", None),
    ("azsheets-border-none", Some(BorderPreset::None)),
    ("azsheets-border-outline", Some(BorderPreset::Outer)),
    ("azsheets-border-all", Some(BorderPreset::All)),
    ("azsheets-top", Some(BorderPreset::Top)),
    ("azsheets-bottom", Some(BorderPreset::Bottom)),
];

const H_ALIGNS: [(&str, HAlign); 4] = [
    ("azsheets-general", HAlign::General),
    ("azsheets-align-left", HAlign::Left),
    ("azsheets-align-center", HAlign::Center),
    ("azsheets-align-right", HAlign::Right),
];

const V_ALIGNS: [(&str, VAlign); 3] = [
    ("azsheets-top", VAlign::Top),
    ("azsheets-middle", VAlign::Center),
    ("azsheets-bottom", VAlign::Bottom),
];

/// The font sizes the dialog steps between.
const SIZE_RANGE: (i32, i32) = (6, 72);

/// What one control of the dialog sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Control {
    Category,
    HAlign,
    VAlign,
    FontColor,
    /// The Font tab's picker: any colour.
    FontColorPick,
    Fill,
    /// The Fill tab's picker: any colour.
    FillPick,
    Border,
    /// The Border tab's picker: the colour of the borders drawn.
    BorderColorPick,
    Wrap,
    Bold,
    Italic,
    Underline,
    Strike,
    Thousands,
    DecimalsMore,
    DecimalsLess,
    SizeUp,
    SizeDown,
    Ok,
    Cancel,
}

struct ControlRef {
    app: RefAny,
    control: Control,
}

fn control_ref(app: &RefAny, control: Control) -> RefAny {
    RefAny::new(ControlRef { app: app.clone(), control })
}

/// A caption: a key said in the window's language.
fn label(text: &str) -> Dom {
    Dom::create_p_with_text(l10n::label(text)).with_css("margin: 10px 0px 4px 0px; font-size: 12px; font-weight: bold;")
}

/// A note: a key said, plain words (a format code, a size) as they are.
fn note(text: &str) -> Dom {
    Dom::create_p_with_text(l10n::label(text)).with_css("margin: 4px 0px; font-size: 12px;")
}

fn segmented<'a>(app: &RefAny, control: Control, labels: impl IntoIterator<Item = &'a str>, selected: usize) -> Dom {
    Segmented::create(strs(labels))
        .with_selected_index(selected)
        .with_on_change(control_ref(app, control), on_segment as SegmentedOnChangeCallbackType)
        .dom()
}

fn check(app: &RefAny, control: Control, text: &str, checked: bool) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; margin: 3px 0px;")
        .with_child(
            CheckBox::create(checked)
                .with_on_toggle(control_ref(app, control), on_check as CheckBoxOnToggleCallbackType)
                .with_accessibility_name(l10n::label(text))
                .dom(),
        )
        .with_child(Dom::create_p_with_text(l10n::label(text)).with_css("margin: 0px 0px 0px 6px; font-size: 12px;"))
}

fn button(app: &RefAny, control: Control, text: &str, primary: bool) -> Dom {
    let mut b = Button::create(l10n::label(text));
    if primary {
        b = b.with_button_type(ButtonType::Primary);
    }
    b.with_on_click(control_ref(app, control), on_button as ButtonOnClickCallbackType)
        .dom()
}

fn row(children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center;")
        .with_children(children)
}

fn index_of<T: PartialEq + Copy>(items: &[(&str, T)], value: T) -> usize {
    items.iter().position(|(_, v)| *v == value).unwrap_or(0)
}

/// A picker of any colour, showing `current` (`fallback` when unset).
fn color_picker(app: &RefAny, control: Control, name: &str, current: Option<&str>, fallback: &str) -> Dom {
    let color = ColorU::parse_hex(current.unwrap_or(fallback))
        .into_option()
        .unwrap_or_else(ColorU::black);
    row(vec![
        note(&format!("{}  ", l10n::t("azsheets-more-colors"))),
        ColorInput::create(color)
            .with_accessibility_name(l10n::label(name))
            .with_on_value_change(control_ref(app, control), on_color as ColorInputOnValueChangeCallbackType)
            .dom(),
    ])
}

fn color_index(items: &[(&str, Option<&str>)], value: Option<&str>) -> usize {
    items
        .iter()
        .position(|(_, v)| v.map(str::to_ascii_uppercase) == value.map(str::to_ascii_uppercase))
        .unwrap_or(0)
}

/// The front tab's controls.
fn tab_body(d: &FormatDraft, app: &RefAny) -> Dom {
    let body = Dom::create_div().with_css("display: flex; flex-direction: column; min-height: 220px;");
    match d.tab {
        0 => {
            let mut b = body
                .with_child(label("azsheets-category"))
                .with_child(segmented(
                    app,
                    Control::Category,
                    NumberCategory::ALL.iter().map(|c| c.label()),
                    NumberCategory::ALL.iter().position(|c| *c == d.category).unwrap_or(0),
                ));
            if d.category.has_decimals() {
                b = b.with_child(label("azsheets-decimal-places")).with_child(row(vec![
                    button(app, Control::DecimalsLess, "-", false),
                    note(&format!("  {}  ", d.decimals)),
                    button(app, Control::DecimalsMore, "+", false),
                ]));
            }
            if d.category == NumberCategory::Number {
                b = b.with_child(check(app, Control::Thousands, "azsheets-thousands", d.thousands));
            }
            b.with_child(label("azsheets-format-code")).with_child(note(&d.style.num_fmt))
        }
        1 => body
            .with_child(label("azsheets-horizontal"))
            .with_child(segmented(app, Control::HAlign, H_ALIGNS.iter().map(|x| x.0), index_of(&H_ALIGNS, d.style.h_align)))
            .with_child(label("azsheets-vertical"))
            .with_child(segmented(app, Control::VAlign, V_ALIGNS.iter().map(|x| x.0), index_of(&V_ALIGNS, d.style.v_align)))
            .with_child(label("azsheets-text-control"))
            .with_child(check(app, Control::Wrap, "azsheets-wrap-text", d.style.wrap)),
        2 => body
            .with_child(label("azsheets-font-style"))
            .with_child(check(app, Control::Bold, "azsheets-bold", d.style.bold))
            .with_child(check(app, Control::Italic, "azsheets-italic", d.style.italic))
            .with_child(label("azsheets-effects"))
            .with_child(check(app, Control::Underline, "azsheets-underline", d.style.underline))
            .with_child(check(app, Control::Strike, "azsheets-strikethrough", d.style.strike))
            .with_child(label("azsheets-size"))
            .with_child(row(vec![
                button(app, Control::SizeDown, "-", false),
                note(&format!("  {} pt  ", d.style.font_size)),
                button(app, Control::SizeUp, "+", false),
            ]))
            .with_child(label("azsheets-color"))
            .with_child(segmented(
                app,
                Control::FontColor,
                FONT_COLORS.iter().map(|x| x.0),
                color_index(&FONT_COLORS, d.style.font_color.as_deref()),
            ))
            .with_child(color_picker(app, Control::FontColorPick, "azsheets-font-color", d.style.font_color.as_deref(), "#000000")),
        3 => body
            .with_child(label("azsheets-presets"))
            .with_child(segmented(app, Control::Border, BORDERS.iter().map(|x| x.0), index_of(&BORDERS, d.border)))
            .with_child(label("azsheets-color"))
            .with_child(color_picker(app, Control::BorderColorPick, "azsheets-border-color", Some(d.border_color.as_str()), "#000000"))
            .with_child(note("azsheets-preset-on-ok")),
        _ => body
            .with_child(label("azsheets-background-color"))
            .with_child(segmented(app, Control::Fill, FILLS.iter().map(|x| x.0), color_index(&FILLS, d.style.fill.as_deref())))
            .with_child(color_picker(app, Control::FillPick, "azsheets-background-color", d.style.fill.as_deref(), "#FFFFFF")),
    }
}

/// The dialog for the draft `d`.
pub(crate) fn dialog(d: &FormatDraft, app: &RefAny) -> Dom {
    let content = Dom::create_div()
        .with_css("display: flex; flex-direction: column; width: 560px; padding: 4px 8px;")
        .with_child(
            TabHeader::create(strs(TABS))
                .with_active_tab(d.tab)
                .with_on_click(app.clone(), on_tab as TabOnClickCallbackType)
                .dom(),
        )
        .with_child(tab_body(d, app))
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; justify-content: flex-end; margin-top: 12px;")
                .with_child(button(app, Control::Ok, "kit-button-ok", true))
                .with_child(Dom::create_div().with_css("width: 8px;"))
                .with_child(button(app, Control::Cancel, "kit-button-cancel", false)),
        );
    Modal::create(content)
        .with_title(l10n::label("azsheets-format-cells"))
        .with_open(true)
        .with_on_close(app.clone(), on_close as ModalOnCloseCallbackType)
        .dom()
}

/// Opens the dialog on the active cell's style, on tab `tab`.
pub(crate) fn open(s: &mut AppState, style: crate::engine::CellStyle, tab: usize) {
    let mut d = FormatDraft::open(style);
    d.tab = tab.min(TABS.len() - 1);
    s.format = Some(d);
}

/// Applies the draft to the selection - every change at once, ONE undo
/// step - and closes the dialog.
fn apply(info: &mut CallbackInfo, app: &RefAny, s: &mut AppState) {
    if let Some(d) = s.format.take() {
        let patches = d.patches();
        if !patches.is_empty() {
            restyle(info, app, s, patches);
        }
    }
}

/// Runs `f` on the open draft of the dialog a control belongs to.
fn on_draft(data: &mut RefAny, info: &mut CallbackInfo, f: impl FnOnce(Control, &mut FormatDraft)) -> Update {
    let Some((mut app, control)) = data.downcast_ref::<ControlRef>().map(|r| (r.app.clone(), r.control)) else {
        return Update::DoNothing;
    };
    with_app(&mut app, info, |_, _, s| {
        if let Some(d) = s.format.as_mut() {
            f(control, d);
        }
    })
}

extern "C" fn on_tab(mut data: RefAny, mut info: CallbackInfo, state: TabHeaderState) -> Update {
    with_app(&mut data, &mut info, |_, _, s| {
        if let Some(d) = s.format.as_mut() {
            d.tab = state.active_tab.min(TABS.len() - 1);
        }
    })
}

extern "C" fn on_segment(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    let i = state.selected_index;
    on_draft(&mut data, &mut info, |control, d| match control {
        Control::Category => {
            if let Some(c) = NumberCategory::ALL.get(i) {
                d.set_category(*c);
            }
        }
        Control::HAlign => d.style.h_align = H_ALIGNS.get(i).map_or(HAlign::General, |x| x.1),
        Control::VAlign => d.style.v_align = V_ALIGNS.get(i).map_or(VAlign::Bottom, |x| x.1),
        Control::FontColor => d.style.font_color = FONT_COLORS.get(i).and_then(|x| x.1).map(String::from),
        Control::Fill => d.style.fill = FILLS.get(i).and_then(|x| x.1).map(String::from),
        Control::Border => d.border = BORDERS.get(i).and_then(|x| x.1),
        _ => {}
    })
}

/// A picker's colour: the font colour, the fill or the border colour.
extern "C" fn on_color(mut data: RefAny, mut info: CallbackInfo, state: ColorInputState) -> Update {
    let hex = state.color.to_hex();
    let hex = hex.as_str();
    on_draft(&mut data, &mut info, |control, d| match control {
        Control::FontColorPick => d.set_font_color(Some(hex)),
        Control::FillPick => d.set_fill(Some(hex)),
        Control::BorderColorPick => d.set_border_color(hex),
        _ => {}
    })
}

extern "C" fn on_check(mut data: RefAny, mut info: CallbackInfo, state: CheckBoxState) -> Update {
    let on = state.checked;
    on_draft(&mut data, &mut info, |control, d| match control {
        Control::Wrap => d.style.wrap = on,
        Control::Bold => d.style.bold = on,
        Control::Italic => d.style.italic = on,
        Control::Underline => d.style.underline = on,
        Control::Strike => d.style.strike = on,
        Control::Thousands => d.set_thousands(on),
        _ => {}
    })
}

extern "C" fn on_button(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, control)) = data.downcast_ref::<ControlRef>().map(|r| (r.app.clone(), r.control)) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |info, app, s| match control {
        Control::Ok => apply(info, app, s),
        Control::Cancel => s.format = None,
        _ => {
            if let Some(d) = s.format.as_mut() {
                match control {
                    Control::DecimalsMore => d.step_decimals(true),
                    Control::DecimalsLess => d.step_decimals(false),
                    Control::SizeUp => d.style.font_size = (d.style.font_size + 1).clamp(SIZE_RANGE.0, SIZE_RANGE.1),
                    Control::SizeDown => d.style.font_size = (d.style.font_size - 1).clamp(SIZE_RANGE.0, SIZE_RANGE.1),
                    _ => {}
                }
            }
        }
    })
}

/// Escape or the modal's close button: Cancel.
extern "C" fn on_close(mut data: RefAny, mut info: CallbackInfo, _state: ModalState) -> Update {
    with_app(&mut data, &mut info, |_, _, s| s.format = None)
}
