//! The window: the S2 `CanvasShell` - menu row, options bar, tools column,
//! the document tab over the canvas (with rulers), the panels (Color,
//! Layers, Adjustments, Properties, History, Navigator), the status bar -
//! the start screen and the sheets.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType, ColorInputOnValueChangeCallbackType,
        DropDownOnChoiceChangeCallbackType, NumberInputOnValueChangeCallbackType,
        SegmentedOnChangeCallbackType, SliderOnValueChangeCallbackType,
    },
    css::DarkLightMode,
    dom::{AccessibilityInfo, AccessibilityRole, VirtualKeyCode, VirtualKeyCodeCombo},
    menu::{Menu, MenuItem, StringMenuItem},
    option::OptionVirtualKeyCodeCombo,
    prelude::*,
    shells::{CanvasShell, ShellEmptyState, ShellSettingsLayout, ShellSettingsSection, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    vec::StringVec,
    widgets::{ButtonType, DropDown, Segmented, Slider, StatusBar, StatusBarSegment, Titlebar},
};

use crate::{
    canvas,
    commands::{self, cmd, field, Command, Field},
    jobs::ExportFormat,
    raster::{layer, Adjustment, BlendMode, LayerContent, LayerId, RasterEngine, SelectMode},
    state::Tool,
    view, AppScreen, PhotoApp, Sheet,
};

// ==== Colours ====

/// The app's own surfaces in one mode; the widgets follow the app theme
/// (flat / flora) and the mode themselves.
pub struct Palette {
    pub dark: bool,
    pub chrome: &'static str,
    pub panel: &'static str,
    pub line: &'static str,
    pub text: &'static str,
    pub muted: &'static str,
    pub selected: &'static str,
    pub ruler: &'static str,
}

pub const LIGHT: Palette = Palette {
    dark: false,
    chrome: "#eceef2",
    panel: "#f7f8fa",
    line: "#d5d8de",
    text: "#1d2330",
    muted: "#5d6677",
    selected: "#d6e4fb",
    ruler: "#e4e6ea",
};

pub const DARK: Palette = Palette {
    dark: true,
    chrome: "#2b2d31",
    panel: "#232428",
    line: "#3a3c42",
    text: "#e6e7ea",
    muted: "#a0a4ad",
    selected: "#2f4a72",
    ruler: "#303237",
};

// ==== Small builders ====

fn strings(items: &[&str]) -> StringVec {
    items
        .iter()
        .map(|s| AzString::from(*s))
        .collect::<Vec<AzString>>()
        .into()
}

fn text(s: &str, css: &str) -> Dom {
    Dom::create_span_with_text(AzString::from(s)).with_css(css)
}

fn hex(c: [u8; 4]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn row(css: &str) -> Dom {
    Dom::create_div().with_css(format!("display: flex; flex-direction: row; align-items: center; {css}"))
}

fn column(css: &str) -> Dom {
    Dom::create_div().with_css(format!("display: flex; flex-direction: column; {css}"))
}

/// A button that runs `command`.
fn button(app: &RefAny, label: &str, command: Command) -> Dom {
    Button::create(AzString::from(label))
        .with_on_click(cmd(app, command), commands::on_command as ButtonOnClickCallbackType)
        .dom()
}

/// An icon button that runs `command` (named for assistive technology by
/// its label).
fn icon_button(app: &RefAny, icon: &str, label: &str, command: Command) -> Dom {
    Button::create(AzString::from(""))
        .with_icon(AzString::from(icon))
        .with_on_click(cmd(app, command), commands::on_command as ButtonOnClickCallbackType)
        .dom()
        .with_accessibility_info(AccessibilityInfo::named(label, AccessibilityRole::PushButton))
}

/// A labelled number field.
fn number(app: &RefAny, label: &str, value: f32, f: Field, p: &Palette) -> Dom {
    row("margin-right: 10px;")
        .with_child(text(label, &format!("font-size: 12px; color: {}; margin-right: 4px;", p.muted)))
        .with_child(
            NumberInput::create(value)
                .with_accessibility_name(AzString::from(label))
                .with_on_value_change(field(app, f), commands::on_number as NumberInputOnValueChangeCallbackType)
                .dom()
                .with_css("width: 64px;"),
        )
}

/// A labelled slider.
fn slider(app: &RefAny, label: &str, value: f32, min: f32, max: f32, f: Field, p: &Palette) -> Dom {
    row("margin-right: 10px;")
        .with_child(text(label, &format!("font-size: 12px; color: {}; margin-right: 4px;", p.muted)))
        .with_child(
            Slider::create(value, min, max)
                .with_accessibility_name(AzString::from(label))
                .with_on_value_change(field(app, f), commands::on_slider as SliderOnValueChangeCallbackType)
                .dom()
                .with_css("width: 110px;"),
        )
        .with_child(text(&format!("{}", value.round()), &format!("font-size: 12px; color: {}; margin-left: 4px; width: 32px;", p.muted)))
}

/// A labelled check box.
fn check(app: &RefAny, label: &str, value: bool, f: Field, p: &Palette) -> Dom {
    row("margin-right: 10px;")
        .with_child(
            CheckBox::create(value)
                .with_accessibility_name(AzString::from(label))
                .with_on_toggle(field(app, f), commands::on_check as CheckBoxOnToggleCallbackType)
                .dom(),
        )
        .with_child(text(label, &format!("font-size: 12px; color: {}; margin-left: 4px;", p.text)))
}

/// A segmented choice.
fn segments(app: &RefAny, labels: &[&str], selected: usize, f: Field) -> Dom {
    Segmented::create(strings(labels))
        .with_selected_index(selected)
        .with_on_change(field(app, f), commands::on_segment as SegmentedOnChangeCallbackType)
        .dom()
}

// ==== Menus ====

/// One entry of a menu.
enum Entry {
    Item(&'static str, Command, &'static [VirtualKeyCode]),
    Sub(&'static str, Vec<Entry>),
    Separator,
}

/// The Cmd key of the platform (Command on macOS, Control elsewhere).
const fn cmd_key() -> VirtualKeyCode {
    if cfg!(target_os = "macos") {
        VirtualKeyCode::LWin
    } else {
        VirtualKeyCode::LControl
    }
}

/// The menu bar: File Edit Image Layer Select Filter View Help.
fn menu_table() -> Vec<(&'static str, Vec<Entry>)> {
    use Command as C;
    use Entry::{Item, Separator, Sub};
    use VirtualKeyCode as K;
    const NONE: &[VirtualKeyCode] = &[];
    let adjustments = Adjustment::catalog()
        .iter()
        .enumerate()
        .map(|(i, a)| Item(a.name(), C::NewAdjustment(i), NONE))
        .collect();
    vec![
        (
            "File",
            vec![
                Item("New...", C::Sheet(Sheet::NewImage), &[K::N]),
                Item("Open...", C::Open, &[K::O]),
                Item("Open Sample", C::OpenSample, NONE),
                Item("Place Image as Layer...", C::Place, NONE),
                Separator,
                Item("Save", C::Save, &[K::S]),
                Item("Export...", C::Sheet(Sheet::Export), &[K::LShift, K::E]),
                Separator,
                Item("Close", C::CloseDocument, NONE),
            ],
        ),
        (
            "Edit",
            vec![
                Item("Undo", C::Undo, &[K::Z]),
                Item("Redo", C::Redo, &[K::LShift, K::Z]),
                Separator,
                Item("Fill with Foreground", C::Fill, NONE),
                Item("Clear", C::Clear, NONE),
                Separator,
                Item("Settings...", C::Sheet(Sheet::Settings), NONE),
            ],
        ),
        (
            "Image",
            vec![
                Item("Image Size...", C::Sheet(Sheet::ImageSize), NONE),
                Item("Canvas Size...", C::Sheet(Sheet::CanvasSize), NONE),
                Separator,
                Item("Rotate 90\u{b0} Clockwise", C::RotateCanvas(true), NONE),
                Item("Rotate 90\u{b0} Counter-clockwise", C::RotateCanvas(false), NONE),
                Item("Flip Canvas Horizontal", C::FlipCanvas(true), NONE),
                Item("Flip Canvas Vertical", C::FlipCanvas(false), NONE),
                Separator,
                Item("Crop to Selection", C::CropToSelection, NONE),
            ],
        ),
        (
            "Layer",
            vec![
                Item("New Layer", C::NewLayer, &[K::LShift, K::N]),
                Item("New Group", C::NewGroup, NONE),
                Sub("New Adjustment Layer", adjustments),
                Item("Duplicate Layer", C::Duplicate, &[K::J]),
                Item("Delete Layer", C::Delete, NONE),
                Item("Merge Down", C::MergeDown, &[K::E]),
                Separator,
                Sub(
                    "Transform",
                    vec![
                        Item("Flip Horizontal", C::FlipLayer(true), NONE),
                        Item("Flip Vertical", C::FlipLayer(false), NONE),
                        Item("Rotate...", C::Sheet(Sheet::Rotate), NONE),
                        Item("Scale 50 %", C::ScaleLayer(0.5), NONE),
                        Item("Scale 200 %", C::ScaleLayer(2.0), NONE),
                    ],
                ),
                Item("Move Up", C::LayerUp, NONE),
                Item("Move Down", C::LayerDown, NONE),
            ],
        ),
        (
            "Select",
            vec![
                Item("All", C::SelectAll, &[K::A]),
                Item("Deselect", C::Deselect, &[K::D]),
                Item("Inverse", C::Inverse, &[K::LShift, K::I]),
                Item("Feather...", C::Sheet(Sheet::Feather), NONE),
            ],
        ),
        (
            "Filter",
            vec![
                Item("Gaussian Blur...", C::Sheet(Sheet::GaussianBlur), NONE),
                Item("Sharpen...", C::Sheet(Sheet::Sharpen), NONE),
            ],
        ),
        (
            "View",
            vec![
                Item("Zoom In", C::ZoomIn, &[K::Equals]),
                Item("Zoom Out", C::ZoomOut, &[K::Minus]),
                Item("Fit on Screen", C::Fit, &[K::Key0]),
                Item("Actual Pixels", C::ActualPixels, &[K::Key1]),
                Separator,
                Item("Light Mode", C::Mode(false), NONE),
                Item("Dark Mode", C::Mode(true), NONE),
                Item("Flat Theme", C::Theme("flat"), NONE),
                Item("Flora Theme", C::Theme("flora"), NONE),
            ],
        ),
        ("Help", vec![Item("About AzPhoto", C::Sheet(Sheet::About), NONE)]),
    ]
}

fn menu_items(app: &RefAny, entries: Vec<Entry>) -> Vec<MenuItem> {
    entries
        .into_iter()
        .map(|e| match e {
            Entry::Item(label, command, keys) => {
                let mut item = StringMenuItem::create(AzString::from(label))
                    .with_callback(cmd(app, command), commands::on_command);
                if !keys.is_empty() {
                    let mut combo = vec![cmd_key()];
                    combo.extend_from_slice(keys);
                    item.accelerator = OptionVirtualKeyCodeCombo::Some(VirtualKeyCodeCombo { keys: combo.into() });
                }
                MenuItem::string(item)
            }
            Entry::Sub(label, children) => MenuItem::string(
                StringMenuItem::create(AzString::from(label)).with_children(menu_items(app, children)),
            ),
            Entry::Separator => MenuItem::separator(),
        })
        .collect()
}

/// The whole menu bar (the native one on macOS).
pub fn native_menu(app: &RefAny) -> Menu {
    Menu::create(
        menu_table()
            .into_iter()
            .map(|(title, entries)| {
                MenuItem::string(StringMenuItem::create(AzString::from(title)).with_children(menu_items(app, entries)))
            })
            .collect::<Vec<MenuItem>>(),
    )
}

/// The payload of an in-window menu button.
struct MenuRef {
    app: RefAny,
    index: usize,
}

/// An in-window menu button opens its menu under itself.
extern "C" fn on_menu_button(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((app, index)) = data.downcast_ref::<MenuRef>().map(|m| (m.app.clone(), m.index)) else {
        return Update::DoNothing;
    };
    if let Some((_, entries)) = menu_table().into_iter().nth(index) {
        let menu = Menu::create(menu_items(&app, entries));
        let _ = info.open_menu_for_hit_node(menu);
    }
    Update::DoNothing
}

/// The menu row (the S2 shell's title row).
fn menu_row(app: &RefAny, p: &Palette) -> Dom {
    let mut bar = row(&format!("padding: 2px 6px; background: {}; flex-shrink: 0;", p.chrome)).with_id("photo-menu");
    for (i, (title, _)) in menu_table().iter().enumerate() {
        bar.add_child(
            Button::create(AzString::from(*title))
                .with_button_type(ButtonType::Link)
                .with_on_click(
                    RefAny::new(MenuRef {
                        app: app.clone(),
                        index: i,
                    }),
                    on_menu_button as ButtonOnClickCallbackType,
                )
                .dom()
                .with_css("margin-right: 2px;"),
        );
    }
    bar
}

// ==== Options bar and tools ====

/// The options bar: the active tool's settings.
fn options_bar(app: &RefAny, a: &PhotoApp, p: &Palette) -> Dom {
    let s = &a.s;
    let o = &s.opts;
    let hint = |t: &str| text(t, &format!("font-size: 12px; color: {}; margin-right: 10px;", p.muted));
    let mut bar = row(&format!("padding: 4px 8px; background: {}; flex-shrink: 0; min-height: 30px;", p.chrome))
        .with_id("photo-options")
        .with_child(text(s.tool.name(), &format!("font-size: 12px; font-weight: bold; color: {}; margin-right: 12px;", p.text)));
    match s.tool {
        Tool::Brush | Tool::Pencil | Tool::Eraser | Tool::CloneStamp => {
            bar.add_child(number(app, "Size", s.brush_size(), Field::BrushSize, p));
            if s.tool != Tool::Pencil {
                bar.add_child(slider(app, "Hardness", o.hardness * 100.0, 0.0, 100.0, Field::Hardness, p));
            }
            bar.add_child(slider(app, "Opacity", o.opacity * 100.0, 1.0, 100.0, Field::Opacity, p));
            bar.add_child(slider(app, "Flow", o.flow * 100.0, 1.0, 100.0, Field::Flow, p));
            bar.add_child(check(app, "Pressure \u{2192} size", o.pressure_size, Field::PressureSize, p));
            bar.add_child(check(app, "Pressure \u{2192} flow", o.pressure_flow, Field::PressureFlow, p));
            if s.tool == Tool::CloneStamp {
                bar.add_child(hint(match o.clone_source {
                    Some(_) => "Source set (Alt-click to move it)",
                    None => "Alt-click sets the source",
                }));
            }
        }
        Tool::MarqueeRect | Tool::MarqueeEllipse | Tool::Lasso => {
            let modes: Vec<&str> = SelectMode::ALL.iter().map(|m| m.name()).collect();
            let at = SelectMode::ALL.iter().position(|m| *m == o.select_mode).unwrap_or(0);
            bar.add_child(segments(app, &modes, at, Field::SelectMode));
            bar.add_child(Dom::create_div().with_css("width: 12px;"));
            bar.add_child(number(app, "Feather", o.feather, Field::Feather, p));
            bar.add_child(hint("Shift adds, Alt subtracts"));
        }
        Tool::MagicWand => {
            let modes: Vec<&str> = SelectMode::ALL.iter().map(|m| m.name()).collect();
            let at = SelectMode::ALL.iter().position(|m| *m == o.select_mode).unwrap_or(0);
            bar.add_child(segments(app, &modes, at, Field::SelectMode));
            bar.add_child(Dom::create_div().with_css("width: 12px;"));
            bar.add_child(number(app, "Tolerance", f32::from(o.wand_tolerance), Field::WandTolerance, p));
            bar.add_child(check(app, "Contiguous", o.wand_contiguous, Field::WandContiguous, p));
            bar.add_child(check(app, "Sample all layers", o.sample_merged, Field::SampleMerged, p));
        }
        Tool::Bucket => {
            bar.add_child(number(app, "Tolerance", f32::from(o.bucket_tolerance), Field::BucketTolerance, p));
            bar.add_child(check(app, "Contiguous", o.bucket_contiguous, Field::BucketContiguous, p));
        }
        Tool::Gradient => {
            bar.add_child(hint("Drag: foreground colour to background colour, in the selection"));
        }
        Tool::Shape => {
            bar.add_child(segments(app, &["Rectangle", "Ellipse"], usize::from(o.shape_ellipse), Field::ShapeKind));
            bar.add_child(hint("Filled with the foreground colour; Shift for a square"));
        }
        Tool::Crop => {
            bar.add_child(hint("Drag the frame to keep; Image > Crop to Selection crops to the selection"));
        }
        Tool::Move => bar.add_child(hint("Drag to move the active layer")),
        Tool::Eyedropper => bar.add_child(hint("Click picks the foreground colour, Alt-click the background")),
        Tool::Hand | Tool::Zoom => {
            bar.add_child(button(app, "Zoom In", Command::ZoomIn));
            bar.add_child(button(app, "Zoom Out", Command::ZoomOut));
            bar.add_child(button(app, "Fit", Command::Fit));
            bar.add_child(button(app, "100 %", Command::ActualPixels));
        }
        Tool::Text => bar.add_child(hint(crate::state::TEXT_TOOL_NOTE)),
    }
    bar.add_child(Dom::create_div().with_css("flex-grow: 1;"));
    if !s.status.is_empty() {
        bar.add_child(text(&s.status, &format!("font-size: 12px; color: {};", p.muted)).with_id("photo-status-line"));
    }
    bar
}

/// The tools column, the colour chips under it.
fn tools_column(app: &RefAny, a: &PhotoApp, p: &Palette) -> Dom {
    let mut col = column(&format!("align-items: center; padding: 4px 0px; background: {};", p.chrome)).with_id("photo-tools");
    for t in Tool::ALL {
        let selected = t == a.s.tool;
        let css = format!(
            "margin: 1px; {}{}",
            if selected { format!("background: {}; border-radius: 4px; ", p.selected) } else { String::new() },
            if t.enabled() { "" } else { "opacity: 0.45;" }
        );
        col.add_child(
            Button::create(AzString::from(""))
                .with_icon(AzString::from(t.icon()))
                .with_on_click(cmd(app, Command::Tool(t)), commands::on_command as ButtonOnClickCallbackType)
                .dom()
                .with_id(t.dom_id())
                .with_css(css)
                .with_accessibility_info(AccessibilityInfo::named(
                    format!("{} ({})", t.name(), t.key()),
                    AccessibilityRole::PushButton,
                )),
        );
    }
    let chip = |c: [u8; 4], id: &str| {
        Dom::create_div()
            .with_id(id)
            .with_css(format!("width: 18px; height: 18px; background: {}; border: 1px solid {};", hex(c), p.line))
    };
    col.add_child(Dom::create_div().with_css("height: 8px;"));
    col.add_child(
        Dom::create_div()
            .with_css("position: relative; width: 30px; height: 30px;")
            .with_child(chip(a.s.bg, "photo-bg-chip").with_css("position: absolute; left: 10px; top: 10px;"))
            .with_child(chip(a.s.fg, "photo-fg-chip").with_css("position: absolute; left: 0px; top: 0px;"))
            .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp), cmd(app, Command::SwapColors), commands::on_command),
    );
    col
}
