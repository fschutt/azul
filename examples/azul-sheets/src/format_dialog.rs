//! The Format Cells dialog on screen (Ctrl+1, the launchers of HOME's Font,
//! Alignment and Number groups): a `Modal` with a `TabHeader` over the five
//! tabs of [`crate::format_cells`], each built from the standard widgets
//! (`Segmented`, `CheckBox`, `Button`); OK applies the draft's patches to the
//! selection, Cancel / Escape / the close button drop it.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType, ModalOnCloseCallbackType,
        SegmentedOnChangeCallbackType, TabOnClickCallbackType,
    },
    prelude::*,
    str::String as AzString,
    widgets::{
        Button, ButtonType, CheckBox, CheckBoxState, Modal, ModalState, Segmented, SegmentedState,
        TabHeader, TabHeaderState,
    },
};

use crate::{
    engine::{BorderPreset, HAlign, VAlign},
    format_cells::{FormatDraft, NumberCategory, TABS},
    restyle, strs, with_app, AppState,
};

/// Font colours offered: (name, `#RRGGBB`; `None` = automatic).
const FONT_COLORS: [(&str, Option<&str>); 6] = [
    ("Automatic", None),
    ("Black", Some("#000000")),
    ("Red", Some("#C00000")),
    ("Blue", Some("#2F5597")),
    ("Green", Some("#548235")),
    ("Orange", Some("#C55A11")),
];

/// Fills offered: (name, `#RRGGBB`; `None` = no fill).
const FILLS: [(&str, Option<&str>); 6] = [
    ("No fill", None),
    ("Yellow", Some("#FFF2CC")),
    ("Green", Some("#E2EFDA")),
    ("Blue", Some("#DDEBF7")),
    ("Orange", Some("#FCE4D6")),
    ("Grey", Some("#EDEDED")),
];

/// Border presets offered: (name, preset; `None` = leave the borders).
const BORDERS: [(&str, Option<BorderPreset>); 6] = [
    ("Unchanged", None),
    ("None", Some(BorderPreset::None)),
    ("Outline", Some(BorderPreset::Outer)),
    ("All", Some(BorderPreset::All)),
    ("Top", Some(BorderPreset::Top)),
    ("Bottom", Some(BorderPreset::Bottom)),
];

const H_ALIGNS: [(&str, HAlign); 4] = [
    ("General", HAlign::General),
    ("Left", HAlign::Left),
    ("Center", HAlign::Center),
    ("Right", HAlign::Right),
];

const V_ALIGNS: [(&str, VAlign); 3] = [("Top", VAlign::Top), ("Center", VAlign::Center), ("Bottom", VAlign::Bottom)];

/// The font sizes the dialog steps between.
const SIZE_RANGE: (i32, i32) = (6, 72);

/// What one control of the dialog sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Control {
    Category,
    HAlign,
    VAlign,
    FontColor,
    Fill,
    Border,
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

fn label(text: &str) -> Dom {
    Dom::create_p_with_text(text).with_css("margin: 10px 0px 4px 0px; font-size: 12px; font-weight: bold;")
}

fn note(text: &str) -> Dom {
    Dom::create_p_with_text(text).with_css("margin: 4px 0px; font-size: 12px;")
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
                .with_accessibility_name(AzString::from(text))
                .dom(),
        )
        .with_child(Dom::create_p_with_text(text).with_css("margin: 0px 0px 0px 6px; font-size: 12px;"))
}

fn button(app: &RefAny, control: Control, text: &str, primary: bool) -> Dom {
    let mut b = Button::create(AzString::from(text));
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
                .with_child(label("Category"))
                .with_child(segmented(
                    app,
                    Control::Category,
                    NumberCategory::ALL.iter().map(|c| c.label()),
                    NumberCategory::ALL.iter().position(|c| *c == d.category).unwrap_or(0),
                ));
            if d.category.has_decimals() {
                b = b.with_child(label("Decimal places")).with_child(row(vec![
                    button(app, Control::DecimalsLess, "-", false),
                    note(&format!("  {}  ", d.decimals)),
                    button(app, Control::DecimalsMore, "+", false),
                ]));
            }
            if d.category == NumberCategory::Number {
                b = b.with_child(check(app, Control::Thousands, "Use 1000 separator (,)", d.thousands));
            }
            b.with_child(label("Format code")).with_child(note(&d.style.num_fmt))
        }
        1 => body
            .with_child(label("Horizontal"))
            .with_child(segmented(app, Control::HAlign, H_ALIGNS.iter().map(|x| x.0), index_of(&H_ALIGNS, d.style.h_align)))
            .with_child(label("Vertical"))
            .with_child(segmented(app, Control::VAlign, V_ALIGNS.iter().map(|x| x.0), index_of(&V_ALIGNS, d.style.v_align)))
            .with_child(label("Text control"))
            .with_child(check(app, Control::Wrap, "Wrap text", d.style.wrap)),
        2 => body
            .with_child(label("Font style"))
            .with_child(check(app, Control::Bold, "Bold", d.style.bold))
            .with_child(check(app, Control::Italic, "Italic", d.style.italic))
            .with_child(label("Effects"))
            .with_child(check(app, Control::Underline, "Underline", d.style.underline))
            .with_child(check(app, Control::Strike, "Strikethrough", d.style.strike))
            .with_child(label("Size"))
            .with_child(row(vec![
                button(app, Control::SizeDown, "-", false),
                note(&format!("  {} pt  ", d.style.font_size)),
                button(app, Control::SizeUp, "+", false),
            ]))
            .with_child(label("Color"))
            .with_child(segmented(
                app,
                Control::FontColor,
                FONT_COLORS.iter().map(|x| x.0),
                color_index(&FONT_COLORS, d.style.font_color.as_deref()),
            )),
        3 => body
            .with_child(label("Presets"))
            .with_child(segmented(app, Control::Border, BORDERS.iter().map(|x| x.0), index_of(&BORDERS, d.border)))
            .with_child(note("The preset is drawn on the whole selection when you press OK.")),
        _ => body
            .with_child(label("Background color"))
            .with_child(segmented(app, Control::Fill, FILLS.iter().map(|x| x.0), color_index(&FILLS, d.style.fill.as_deref()))),
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
                .with_child(button(app, Control::Ok, "OK", true))
                .with_child(Dom::create_div().with_css("width: 8px;"))
                .with_child(button(app, Control::Cancel, "Cancel", false)),
        );
    Modal::create(content)
        .with_title(AzString::from("Format Cells"))
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
