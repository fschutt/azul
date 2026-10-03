//! The window: the S2 `CanvasShell` - menu row, options bar, tools column,
//! the document tab over the canvas (with rulers), the panels (Color,
//! Layers, Adjustments, Properties, History, Navigator), the status bar -
//! the start screen and the sheets.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType, ColorInputOnValueChangeCallbackType,
        DropDownOnChoiceChangeCallbackType, NumberInputOnValueChangeCallbackType,
        DialogOnCloseCallbackType, SegmentedOnChangeCallbackType, SliderOnValueChangeCallbackType,
        StandardDialogOnEventCallbackType, TextInputOnTextInputCallbackType,
    },
    css::DarkLightMode,
    dom::{AccessibilityInfo, AccessibilityRole, VirtualKeyCode, VirtualKeyCodeCombo},
    menu::{Menu, MenuItem, StringMenuItem},
    option::OptionVirtualKeyCodeCombo,
    prelude::*,
    shells::{CanvasShell, ShellEmptyState, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    vec::StringVec,
    widgets::{
        AboutDialog, ButtonType, Dialog, DropDown, Segmented, Slider, StatusBar, StatusBarSegment, TextInput, Titlebar,
    },
};

use azul_appkit::ui as kit;

use crate::{
    canvas,
    commands::{self, cmd, field, Command, Field},
    jobs::ExportFormat,
    raster::{layer, Adjustment, BlendMode, LayerContent, SelectMode},
    state::{Tool, TEXT_FAMILIES},
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

/// The swatch's colour, opaque (`#rrggbb`).
fn hex(c: [u8; 4]) -> String {
    ColorU { r: c[0], g: c[1], b: c[2], a: 255 }.to_hex().to_string()
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
                Item("Settings...", C::Settings, NONE),
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
        (
            "Help",
            vec![
                Item("Keyboard Shortcuts", C::Settings, NONE),
                Item("About AzPhoto", C::Sheet(Sheet::About), NONE),
            ],
        ),
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

/// The Text tool's field (scripts type into it).
pub const TEXT_FIELD_ID: &str = "photo-text-field";

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
        Tool::Text => {
            bar.add_child(
                DropDown::create(strings(&TEXT_FAMILIES))
                    .with_selected(o.text_family)
                    .with_accessibility_name(AzString::from("Font"))
                    .with_on_choice_change(field(app, Field::TextFamily), commands::on_choice as DropDownOnChoiceChangeCallbackType)
                    .dom()
                    .with_id("photo-text-family")
                    .with_css("margin-right: 10px;"),
            );
            bar.add_child(number(app, "Size", o.text_size, Field::TextSize, p));
            bar.add_child(check(app, "Bold", o.text_bold, Field::TextBold, p));
            bar.add_child(check(app, "Italic", o.text_italic, Field::TextItalic, p));
            match &s.text {
                Some(draft) => {
                    bar.add_child(
                        TextInput::create()
                            .with_text(AzString::from(draft.text.as_str()))
                            .with_placeholder(AzString::from("Type the text"))
                            .with_accessibility_name(AzString::from("Text"))
                            .with_on_text_input(field(app, Field::Text), commands::on_text as TextInputOnTextInputCallbackType)
                            .dom()
                            .with_id(TEXT_FIELD_ID)
                            .with_css("width: 220px; margin-right: 8px;"),
                    );
                    bar.add_child(
                        Button::with_type(AzString::from("Commit"), ButtonType::Primary)
                            .with_on_click(cmd(app, Command::TextCommit), commands::on_command as ButtonOnClickCallbackType)
                            .dom()
                            .with_id("photo-text-commit")
                            .with_css("margin-right: 6px;"),
                    );
                    bar.add_child(button(app, "Cancel", Command::TextCancel).with_id("photo-text-cancel"));
                }
                None => bar.add_child(hint("Click the canvas where the text starts")),
            }
        }
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
        // The chosen tool is a toggled button; a tool that cannot act on the
        // active layer (a pixel tool on an adjustment or a locked layer) is
        // disabled and says why.
        let mut b = Button::create(AzString::from(""))
            .with_icon(AzString::from(t.icon()))
            .with_toggled(t == a.s.tool);
        if !a.s.tool_available(t) {
            b = b.with_disabled(AzString::from("Needs a pixel layer that is not locked"));
        }
        col.add_child(
            b.with_on_click(cmd(app, Command::Tool(t)), commands::on_command as ButtonOnClickCallbackType)
                .dom()
                .with_id(t.dom_id())
                .with_css("margin: 1px;")
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

// ==== Panels ====

fn section(title: &str, id: &str, p: &Palette, body: Dom) -> Dom {
    column(&format!("border-bottom: 1px solid {}; padding: 6px 8px; flex-shrink: 0;", p.line))
        .with_id(id)
        .with_child(text(
            title,
            &format!("font-size: 11px; font-weight: bold; color: {}; margin-bottom: 4px;", p.muted),
        ))
        .with_child(body)
}

fn navigator_panel(app: &RefAny, a: &PhotoApp, p: &Palette) -> Dom {
    let thumb = view::thumbnail(a.s.engine.composite(), 220, 130, a.s.colors);
    let mut body = column("align-items: center;");
    if let Some(image) = crate::codec::view_image(thumb.width, thumb.height, &thumb.bgra) {
        body.add_child(
            Dom::create_image(image)
                .with_id("photo-navigator-image")
                .with_css(format!("width: {}px; height: {}px; margin-bottom: 4px;", thumb.width, thumb.height)),
        );
    }
    body.add_child(
        row("")
            .with_child(icon_button(app, "zoom_out", "Zoom out", Command::ZoomOut))
            .with_child(
                NumberInput::create((a.s.view.zoom * 100.0 * 10.0).round() / 10.0)
                    .with_accessibility_name(AzString::from("Zoom %"))
                    .with_on_value_change(field(app, Field::ZoomPercent), commands::on_number as NumberInputOnValueChangeCallbackType)
                    .dom()
                    .with_id("photo-zoom-input")
                    .with_css("width: 70px;"),
            )
            .with_child(icon_button(app, "zoom_in", "Zoom in", Command::ZoomIn))
            .with_child(button(app, "Fit", Command::Fit)),
    );
    section("NAVIGATOR", "panel-navigator", p, body)
}

fn color_panel(app: &RefAny, a: &PhotoApp, p: &Palette) -> Dom {
    let color = |c: [u8; 4]| ColorU {
        r: c[0],
        g: c[1],
        b: c[2],
        a: c[3],
    };
    let mut swatches = row("flex-wrap: wrap; margin-top: 6px;");
    for (i, c) in a.s.swatches.iter().enumerate() {
        swatches.add_child(
            Dom::create_div()
                .with_id(format!("swatch-{i}"))
                .with_css(format!(
                    "width: 18px; height: 18px; margin: 0px 3px 3px 0px; background: {}; border: 1px solid {};",
                    hex(*c),
                    p.line
                ))
                .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp), cmd(app, Command::Swatch(i)), commands::on_command),
        );
    }
    let body = column("")
        .with_child(
            row("")
                .with_child(
                    ColorInput::create(color(a.s.fg))
                        .with_accessibility_name(AzString::from("Foreground colour"))
                        .with_on_value_change(field(app, Field::FgColor), commands::on_color as ColorInputOnValueChangeCallbackType)
                        .dom(),
                )
                .with_child(text(&format!("Foreground {}", hex(a.s.fg)), &format!("font-size: 12px; color: {}; margin: 0px 8px;", p.text)))
                .with_child(
                    ColorInput::create(color(a.s.bg))
                        .with_accessibility_name(AzString::from("Background colour"))
                        .with_on_value_change(field(app, Field::BgColor), commands::on_color as ColorInputOnValueChangeCallbackType)
                        .dom(),
                )
                .with_child(text("Background", &format!("font-size: 12px; color: {}; margin-left: 8px;", p.text))),
        )
        .with_child(
            row("margin-top: 4px;")
                .with_child(icon_button(app, "swap_horiz", "Swap colours (X)", Command::SwapColors))
                .with_child(button(app, "Default", Command::DefaultColors)),
        )
        .with_child(swatches);
    section("COLOR", "panel-color", p, body)
}

fn layer_icon(content: &LayerContent) -> &'static str {
    match content {
        LayerContent::Raster(_) => "image",
        LayerContent::Adjustment(_) => "tune",
        LayerContent::Group(_) => "folder",
    }
}

fn layers_panel(app: &RefAny, a: &PhotoApp, p: &Palette) -> Dom {
    let doc = a.s.engine.document();
    let active = a.s.engine.active_layer();
    let active_layer = active.and_then(|id| doc.layer(id));
    let blend_names: Vec<&str> = BlendMode::ALL.iter().map(|m| m.name()).collect();
    let mut body = column("");
    if let Some(l) = active_layer {
        body.add_child(
            row("margin-bottom: 4px;")
                .with_child(
                    DropDown::create(strings(&blend_names))
                        .with_selected(l.blend.index())
                        .with_accessibility_name(AzString::from("Blend mode"))
                        .with_on_choice_change(field(app, Field::LayerBlend), commands::on_choice as DropDownOnChoiceChangeCallbackType)
                        .dom()
                        .with_id("layer-blend")
                        .with_css("margin-right: 8px;"),
                )
                .with_child(text("Opacity", &format!("font-size: 12px; color: {}; margin-right: 4px;", p.muted)))
                .with_child(
                    NumberInput::create((l.opacity * 100.0).round())
                        .with_accessibility_name(AzString::from("Layer opacity %"))
                        .with_on_value_change(field(app, Field::LayerOpacity), commands::on_number as NumberInputOnValueChangeCallbackType)
                        .dom()
                        .with_id("layer-opacity-input")
                        .with_css("width: 52px;"),
                ),
        );
        body.add_child(
            Slider::create((l.opacity * 100.0).round(), 0.0, 100.0)
                .with_accessibility_name(AzString::from("Layer opacity"))
                .with_on_value_change(field(app, Field::LayerOpacity), commands::on_slider as SliderOnValueChangeCallbackType)
                .dom()
                .with_id("layer-opacity")
                .with_css("margin-bottom: 6px;"),
        );
    }
    let mut list = column(&format!("border: 1px solid {}; min-height: 60px;", p.line)).with_id("layer-list");
    for (depth, id) in layer::rows(&doc.layers) {
        let Some(l) = doc.layer(id) else {
            continue;
        };
        let selected = Some(id) == active;
        let mut r = row(&format!(
            "padding: 3px 4px 3px {}px; border-bottom: 1px solid {}; {}",
            4 + depth * 14,
            p.line,
            if selected { format!("background: {};", p.selected) } else { String::new() }
        ))
        .with_id(format!("layer-row-{id}"))
        .with_callback(EventFilter::Hover(HoverEventFilter::MouseDown), cmd(app, Command::LayerPress(id)), commands::on_command)
        .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp), cmd(app, Command::LayerRelease(id)), commands::on_command)
        .with_child(icon_button(
            app,
            if l.visible { "visibility" } else { "visibility_off" },
            "Show or hide",
            Command::ToggleVisible(id),
        ))
        .with_child(icon_button(
            app,
            if l.locked { "lock" } else { "lock_open" },
            "Lock or unlock",
            Command::ToggleLock(id),
        ));
        if matches!(l.content, LayerContent::Group(_)) {
            r.add_child(icon_button(
                app,
                if l.expanded { "expand_more" } else { "chevron_right" },
                "Open or close the group",
                Command::ToggleExpanded(id),
            ));
        }
        r.add_child(Dom::create_icon(AzString::from(layer_icon(&l.content))).with_css("font-size: 16px; margin: 0px 6px;"));
        r.add_child(text(&l.name, &format!("font-size: 13px; color: {}; flex-grow: 1;", p.text)));
        if l.opacity < 0.999 || l.blend != BlendMode::Normal {
            r.add_child(text(
                &format!("{} {} %", l.blend.name(), (l.opacity * 100.0).round()),
                &format!("font-size: 11px; color: {};", p.muted),
            ));
        }
        list.add_child(r);
    }
    body.add_child(list);
    body.add_child(
        row("margin-top: 4px; flex-wrap: wrap;")
            .with_child(icon_button(app, "add", "New layer", Command::NewLayer).with_id("layer-new"))
            .with_child(icon_button(app, "create_new_folder", "New group", Command::NewGroup).with_id("layer-new-group"))
            .with_child(icon_button(app, "control_point_duplicate", "Duplicate layer", Command::Duplicate).with_id("layer-duplicate"))
            .with_child(icon_button(app, "merge_type", "Merge down", Command::MergeDown).with_id("layer-merge"))
            .with_child(icon_button(app, "arrow_upward", "Move up", Command::LayerUp).with_id("layer-up"))
            .with_child(icon_button(app, "arrow_downward", "Move down", Command::LayerDown).with_id("layer-down"))
            .with_child(icon_button(app, "delete", "Delete layer", Command::Delete).with_id("layer-delete")),
    );
    section("LAYERS", "panel-layers", p, body)
}

fn adjustments_panel(app: &RefAny, p: &Palette) -> Dom {
    let mut body = row("flex-wrap: wrap;");
    for (i, a) in Adjustment::catalog().iter().enumerate() {
        body.add_child(button(app, a.name(), Command::NewAdjustment(i)).with_id(format!("adjust-{i}")).with_css("margin: 0px 4px 4px 0px;"));
    }
    section("ADJUSTMENTS", "panel-adjustments", p, body)
}

fn properties_panel(app: &RefAny, a: &PhotoApp, p: &Palette) -> Dom {
    let doc = a.s.engine.document();
    let mut body = column("");
    let small = format!("font-size: 12px; color: {};", p.text);
    match a.s.engine.active_layer().and_then(|id| doc.layer(id)) {
        None => body.add_child(text("No layer selected.", &small)),
        Some(l) => {
            body.add_child(text(&format!("{} - {}", l.name, l.kind_name()), &small));
            match &l.content {
                LayerContent::Raster(g) => body.add_child(text(
                    &format!("{} x {} px, {} of {} tiles hold pixels", g.width(), g.height(), g.non_empty_tiles().len(), g.cols() * g.rows()),
                    &format!("font-size: 11px; color: {};", p.muted),
                )),
                LayerContent::Group(children) => {
                    body.add_child(text(&format!("{} layers", children.len()), &format!("font-size: 11px; color: {};", p.muted)));
                }
                LayerContent::Adjustment(adj) => {
                    let params = commands::adjustment_params(adj);
                    if params.is_empty() {
                        body.add_child(text("No settings.", &format!("font-size: 11px; color: {};", p.muted)));
                    }
                    for (n, (label, v, min, max)) in params.into_iter().enumerate() {
                        let label = if label == "Point" { format!("Point {}", n + 1) } else { label.to_string() };
                        body.add_child(slider(app, &label, v, min, max, Field::Adjust(n as u8), p));
                    }
                }
            }
        }
    }
    if let Some(label) = commands::selection_label(a) {
        body.add_child(text(&label, &format!("font-size: 11px; color: {}; margin-top: 4px;", p.muted)));
    }
    section("PROPERTIES", "panel-properties", p, body)
}

fn history_panel(app: &RefAny, a: &PhotoApp, p: &Palette) -> Dom {
    let (labels, current) = a.s.engine.history();
    let mut list = column("").with_id("history-list");
    for (i, label) in labels.iter().enumerate() {
        let css = format!(
            "font-size: 12px; padding: 2px 6px; color: {}; {}",
            if i > current { p.muted } else { p.text },
            if i == current { format!("background: {};", p.selected) } else { String::new() }
        );
        list.add_child(
            text(label, &css)
                .with_id(format!("history-{i}"))
                .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp), cmd(app, Command::HistoryJump(i)), commands::on_command),
        );
    }
    let body = column("")
        .with_child(list)
        .with_child(
            row("margin-top: 4px;")
                .with_child(icon_button(app, "undo", "Undo", Command::Undo).with_id("history-undo"))
                .with_child(icon_button(app, "redo", "Redo", Command::Redo).with_id("history-redo")),
        );
    section("HISTORY", "panel-history", p, body)
}

/// The panels column (scrolls when it is taller than the window).
fn panels(app: &RefAny, a: &PhotoApp, p: &Palette) -> Dom {
    column(&format!("background: {}; overflow-y: auto; min-height: 0px; flex-grow: 1;", p.panel))
        .with_id("photo-panels")
        .with_child(navigator_panel(app, a, p))
        .with_child(color_panel(app, a, p))
        .with_child(layers_panel(app, a, p))
        .with_child(properties_panel(app, a, p))
        .with_child(adjustments_panel(app, p))
        .with_child(history_panel(app, a, p))
}

// ==== Canvas area, tab, status ====

/// A ruler step (document pixels) that leaves at least ~60 logical px
/// between labels.
fn ruler_step(zoom: f32, hidpi: f32) -> i32 {
    const STEPS: [i32; 13] = [1, 2, 5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000, 10000];
    let per_px = zoom / hidpi.max(0.1);
    STEPS
        .into_iter()
        .find(|s| *s as f32 * per_px >= 60.0)
        .unwrap_or(20000)
}

/// The top (`horizontal`) or left ruler: a label every step.
fn ruler(a: &PhotoApp, p: &Palette, horizontal: bool) -> Dom {
    let hidpi = a.hidpi.max(0.1);
    let v = &a.s.view;
    let step = ruler_step(v.zoom, hidpi);
    let (pan, len) = if horizontal {
        (v.pan_x, v.width as f32)
    } else {
        (v.pan_y, v.height as f32)
    };
    let first = ((-pan / v.zoom) / step as f32).floor() as i32 * step;
    let mut r = Dom::create_div()
        .with_id(if horizontal { "photo-ruler-x" } else { "photo-ruler-y" })
        .with_css(format!(
            "position: relative; overflow: hidden; background: {}; {}",
            p.ruler,
            if horizontal { "height: 18px; flex-grow: 1;" } else { "width: 18px; height: 100%;" }
        ));
    let mut d = first;
    let mut guard = 0;
    while guard < 200 {
        guard += 1;
        let at = (d as f32 * v.zoom + pan) / hidpi;
        if at > len / hidpi {
            break;
        }
        if at >= 0.0 {
            let css = if horizontal {
                format!("position: absolute; left: {:.1}px; top: 1px; font-size: 9px; color: {};", at + 2.0, p.muted)
            } else {
                format!("position: absolute; top: {:.1}px; left: 1px; font-size: 9px; color: {};", at + 2.0, p.muted)
            };
            let tick = if horizontal {
                format!("position: absolute; left: {at:.1}px; top: 0px; width: 1px; height: 18px; background: {};", p.line)
            } else {
                format!("position: absolute; top: {at:.1}px; left: 0px; height: 1px; width: 18px; background: {};", p.line)
            };
            r.add_child(Dom::create_div().with_css(tick));
            r.add_child(text(&d.to_string(), &css));
        }
        d += step;
    }
    r
}

/// The rulers around the canvas node.
fn canvas_area(app: &RefAny, a: &PhotoApp, p: &Palette) -> Dom {
    column("flex-grow: 1; min-height: 0px; min-width: 0px;")
        .with_child(
            row("flex-shrink: 0;")
                .with_child(Dom::create_div().with_css(format!("width: 18px; height: 18px; background: {};", p.ruler)))
                .with_child(ruler(a, p, true)),
        )
        .with_child(
            row("flex-grow: 1; min-height: 0px; align-items: stretch;")
                .with_child(ruler(a, p, false))
                .with_child(canvas::canvas_dom(app, a.s.tool)),
        )
}

/// The document's tab: its name, a dot for unsaved changes, close.
fn doc_tab(app: &RefAny, a: &PhotoApp, p: &Palette) -> Dom {
    row(&format!("padding: 2px 8px; background: {}; flex-shrink: 0;", p.chrome))
        .with_child(
            row(&format!("padding: 2px 8px; background: {}; border-radius: 4px 4px 0px 0px;", p.panel))
                .with_id("photo-doc-tab")
                .with_child(text(
                    &format!("{}{}", a.s.name, if a.s.modified { " \u{25cf}" } else { "" }),
                    &format!("font-size: 12px; color: {}; margin-right: 6px;", p.text),
                ))
                .with_child(icon_button(app, "close", "Close the document", Command::CloseDocument)),
        )
}

fn status_bar(a: &PhotoApp) -> Dom {
    let (w, h) = a.s.engine.size();
    let mut segments = vec![
        StatusBarSegment::create(AzString::from(a.s.view.percent_label())).with_marker(AzString::from("photo-zoom")),
        StatusBarSegment::create(AzString::from(format!("{w} x {h} px \u{b7} RGBA 8-bit"))),
        StatusBarSegment::create(AzString::from(canvas::cursor_label(a))).with_marker(AzString::from(canvas::CURSOR_MARKER)),
    ];
    if a.busy > 0 {
        segments.push(StatusBarSegment::create(AzString::from("Working...")));
    }
    StatusBar::create(segments).dom().with_id("photo-status")
}

// ==== Start screen ====

fn start_screen(app: &RefAny, a: &PhotoApp, p: &Palette) -> Dom {
    let mut recent = column("margin-top: 16px; min-width: 360px;").with_id("photo-recent");
    if a.recent.is_empty() {
        recent.add_child(
            ShellEmptyState::create(AzString::from("No saved documents yet"))
                .with_icon(AzString::from("photo_library"))
                .with_detail(AzString::from("Documents you save appear here."))
                .dom(),
        );
    } else {
        recent.add_child(text("RECENT", &format!("font-size: 11px; font-weight: bold; color: {}; margin-bottom: 4px;", p.muted)));
        for (i, d) in a.recent.iter().enumerate().take(12) {
            recent.add_child(
                row("margin-bottom: 2px;")
                    .with_child(button(app, &d.name, Command::OpenRecent(d.uuid.clone())).with_id(format!("recent-{i}")))
                    .with_child(text(&format!("{} x {} px", d.width, d.height), &format!("font-size: 12px; color: {}; margin-left: 8px;", p.muted))),
            );
        }
    }
    column(&format!("flex-grow: 1; align-items: center; justify-content: center; background: {};", p.panel))
        .with_id("photo-start")
        .with_child(text("AzPhoto", &format!("font-size: 28px; font-weight: bold; color: {}; margin-bottom: 12px;", p.text)))
        .with_child(
            row("")
                .with_child(button(app, "Open...", Command::Open).with_id("start-open").with_css("margin-right: 8px;"))
                .with_child(button(app, "New image...", Command::Sheet(Sheet::NewImage)).with_id("start-new").with_css("margin-right: 8px;"))
                .with_child(button(app, "Open sample", Command::OpenSample).with_id("start-sample")),
        )
        .with_child(text(
            "Opens PNG \u{b7} JPEG \u{b7} WebP \u{b7} GIF (first frame) \u{b7} BMP \u{b7} TIFF \u{b7} TGA",
            &format!("font-size: 12px; color: {}; margin-top: 10px;", p.muted),
        ))
        .with_child(text(
            "Not yet: RAW \u{b7} HEIC \u{b7} AVIF \u{b7} PSD (no decoders in azul)",
            &format!("font-size: 12px; color: {}; margin-top: 2px;", p.muted),
        ))
        .with_child(recent)
}

// ==== Sheets ====

/// A sheet: azul's modal `Dialog` (title, close button, Escape, focus) with
/// the body, then Cancel (Close) and the OK button when it has one.
fn sheet_frame(app: &RefAny, _p: &Palette, title: &str, body: Dom, ok: Option<(&str, Command)>) -> Dom {
    let mut buttons = row("justify-content: flex-end; margin-top: 12px;");
    buttons.add_child(button(app, if ok.is_some() { "Cancel" } else { "Close" }, Command::CloseSheet).with_id("sheet-cancel"));
    if let Some((label, command)) = ok {
        buttons.add_child(
            Button::with_type(AzString::from(label), ButtonType::Primary)
                .with_on_click(cmd(app, command), commands::on_command as ButtonOnClickCallbackType)
                .dom()
                .with_id("sheet-ok")
                .with_css("margin-left: 8px;"),
        );
    }
    Dialog::create(column("min-width: 340px;").with_child(body).with_child(buttons))
        .with_title(AzString::from(title))
        .with_open(true)
        .with_modal(true)
        .with_close_button(true)
        .with_on_close(app.clone(), commands::on_sheet_close as DialogOnCloseCallbackType)
        .dom()
        .with_id("photo-sheet")
}

fn sheet_dom(app: &RefAny, a: &PhotoApp, p: &Palette, sheet: Sheet) -> Dom {
    let f = &a.form;
    let hint = |t: &str| text(t, &format!("font-size: 12px; color: {}; margin-top: 6px;", p.muted));
    match sheet {
        Sheet::NewImage => {
            let mb = f64::from(f.new_width.max(1.0)) * f64::from(f.new_height.max(1.0)) * 4.0 / (1024.0 * 1024.0);
            let body = column("")
                .with_child(
                    row("")
                        .with_child(number(app, "Width", f.new_width, Field::NewWidth, p))
                        .with_child(number(app, "Height", f.new_height, Field::NewHeight, p)),
                )
                .with_child(row("margin-top: 8px;").with_child(check(app, "Transparent background", f.new_transparent, Field::NewTransparent, p)))
                .with_child(hint(&format!("About {mb:.1} MiB per layer (8-bit RGBA tiles)")));
            sheet_frame(app, p, "New image", body, Some(("Create", Command::NewImageCreate)))
        }
        Sheet::Export => {
            let jpeg = a.export_format == ExportFormat::Jpeg;
            let mut body = column("").with_child(segments(app, &["PNG", "JPEG"], usize::from(jpeg), Field::ExportFormat));
            if jpeg {
                body.add_child(row("margin-top: 8px;").with_child(number(app, "Quality", f32::from(a.jpeg_quality), Field::JpegQuality, p)));
                body.add_child(hint("JPEG has no transparency: the image is flattened on white."));
            }
            let (w, h) = a.s.engine.size();
            body.add_child(hint(&format!("{w} x {h} px, all visible layers flattened")));
            body.add_child(hint(&format!(
                "Into the data folder: {}",
                crate::storage::export_key(&a.s.uuid, &commands::file_name(&a.s.name, a.export_format.extension()))
            )));
            sheet_frame(app, p, "Export", body, Some(("Export", Command::ExportApply)))
        }
        Sheet::ImageSize => {
            let body = column("")
                .with_child(
                    row("")
                        .with_child(number(app, "Width", f.size_width, Field::SizeWidth, p))
                        .with_child(number(app, "Height", f.size_height, Field::SizeHeight, p)),
                )
                .with_child(hint("Every layer is resampled (bilinear; an average when shrinking)."));
            sheet_frame(app, p, "Image size", body, Some(("Resize", Command::ImageSizeApply)))
        }
        Sheet::CanvasSize => {
            let body = column("")
                .with_child(
                    row("")
                        .with_child(number(app, "Width", f.canvas_width, Field::CanvasWidth, p))
                        .with_child(number(app, "Height", f.canvas_height, Field::CanvasHeight, p)),
                )
                .with_child(hint("The image stays centred; new area is transparent."));
            sheet_frame(app, p, "Canvas size", body, Some(("Resize", Command::CanvasSizeApply)))
        }
        Sheet::GaussianBlur => {
            let body = column("")
                .with_child(number(app, "Radius (sigma, px)", f.blur_sigma, Field::BlurSigma, p))
                .with_child(hint("Blurs the active layer inside the selection."));
            sheet_frame(app, p, "Gaussian blur", body, Some(("Blur", Command::BlurApply)))
        }
        Sheet::Sharpen => {
            let body = column("")
                .with_child(
                    row("")
                        .with_child(number(app, "Amount", f.sharpen_amount, Field::SharpenAmount, p))
                        .with_child(number(app, "Radius", f.sharpen_radius, Field::SharpenRadius, p)),
                )
                .with_child(hint("Unsharp mask on the active layer, inside the selection."));
            sheet_frame(app, p, "Sharpen", body, Some(("Sharpen", Command::SharpenApply)))
        }
        Sheet::Rotate => {
            let body = column("")
                .with_child(number(app, "Degrees", f.rotate_degrees, Field::RotateDegrees, p))
                .with_child(hint("Turns the active layer about the image centre (bilinear)."));
            sheet_frame(app, p, "Rotate layer", body, Some(("Rotate", Command::RotateLayerApply)))
        }
        Sheet::Feather => {
            let body = column("")
                .with_child(number(app, "Radius (px)", f.feather, Field::FeatherForm, p))
                .with_child(hint("Softens the selection's edge."));
            sheet_frame(app, p, "Feather selection", body, Some(("Feather", Command::FeatherApply)))
        }
        Sheet::About => {
            let about = AboutDialog::create(AzString::from(crate::ABOUT.name), AzString::from(crate::ABOUT.version))
                .with_icon(AzString::from("photo"))
                .with_description(AzString::from(crate::ABOUT.summary))
                .with_copyright(AzString::from(format!(
                    "{} license. Documents: {}",
                    crate::ABOUT.license,
                    azul_appkit::data::local_path(&a.data_root, crate::ABOUT.app_folder).display()
                )))
                .with_credit(AzString::from("azul"), AzString::from("MIT"))
                .with_on_event(app.clone(), commands::on_about_event as StandardDialogOnEventCallbackType)
                .dom();
            Dialog::create(about)
                .with_title(AzString::from("About AzPhoto"))
                .with_open(true)
                .with_modal(true)
                .with_close_button(true)
                .with_on_close(app.clone(), commands::on_sheet_close as DialogOnCloseCallbackType)
                .dom()
                .with_id("photo-sheet")
        }
    }
}

// ==== The window ====

fn title_row(a: &PhotoApp) -> Dom {
    let title = match a.screen {
        AppScreen::Start => "AzPhoto".to_string(),
        AppScreen::Editor => format!("{}{} - AzPhoto", a.s.name, if a.s.modified { " *" } else { "" }),
    };
    Titlebar::create(AzString::from(title)).without_border_bottom().dom()
}

fn editor(app: &RefAny, a: &PhotoApp, p: &Palette) -> Dom {
    CanvasShell::create(canvas_area(app, a, p))
        .with_menu_bar(menu_row(app, p))
        .with_tool_options(options_bar(app, a, p))
        .with_tool_palette(tools_column(app, a, p))
        .with_document_tabs(doc_tab(app, a, p))
        .with_panels(panels(app, a, p))
        .with_canvas_ratio(0.76)
        .office_shell()
        .with_status_bar(status_bar(a))
        .dom()
}

/// The window's layout callback.
pub extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    let dark = matches!(info.get_mode(), DarkLightMode::Dark);
    let theme = info.get_theme().as_str().to_string();
    let app_ref = data.clone();
    let Some(mut guard) = data.downcast_mut::<PhotoApp>() else {
        return Dom::create_body();
    };
    let a = &mut *guard;
    if a.dark != dark || a.s.colors != view::ViewColors::for_mode(dark) {
        a.dark = dark;
        let _ = a.s.set_dark(dark);
        a.canvas_image = None;
    }
    if !theme.is_empty() {
        a.theme = theme;
    }
    let p = if dark { &DARK } else { &LIGHT };
    // azul-appkit's settings page (Appearance - remembered -, Data,
    // Shortcuts, About) takes the window while it is open.
    let content = if kit::settings_open(&a.kit) {
        column("flex-grow: 1; min-height: 0px;").with_child(kit::settings_page(&a.kit, Vec::new()))
    } else {
        match a.screen {
            AppScreen::Start => start_screen(&app_ref, a, p),
            AppScreen::Editor => editor(&app_ref, a, p),
        }
    };
    let mut area = column("position: relative; flex-grow: 1; min-height: 0px;").with_child(content);
    if let Some(sheet) = a.sheet.filter(|_| !kit::settings_open(&a.kit)) {
        area.add_child(sheet_dom(&app_ref, a, p, sheet));
    }
    let root = column("flex-grow: 1; min-height: 0px;")
        .with_child(title_row(a))
        .with_child(area);
    let body = Dom::create_body()
        .with_css("display: flex; flex-direction: column; margin: 0px; height: 100%;")
        .with_child(
            ShellThemeScope::create(root)
                .with_accent(ShellThemeAccent::Blue)
                .dom()
                .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;"),
        )
        .with_callback(EventFilter::Window(WindowEventFilter::VirtualKeyDown), app_ref.clone(), canvas::on_key);
    if cfg!(target_os = "macos") {
        body.with_menu_bar(native_menu(&app_ref))
    } else {
        body
    }
}
