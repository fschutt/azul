//! The window: the S2 `CanvasShell` - menu row, options bar, tools column,
//! the document tab over the canvas (with rulers), the panels (Color,
//! Layers, Adjustments, Properties, History, Navigator), the status bar -
//! the start screen and the sheets.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType, ColorInputOnValueChangeCallbackType,
        DropDownOnChoiceChangeCallbackType, NumberInputOnValueChangeCallbackType,
        CloseGuardOnEventCallbackType, DialogOnCloseCallbackType, SegmentedOnChangeCallbackType,
        SliderOnValueChangeCallbackType,
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

use azul_appkit::{
    l10n::{label, t_args, Arg},
    pieces::Clickable,
    ui as kit,
};

use crate::{
    canvas, ids,
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
    /// A face that stands out of the chrome (the document tab).
    pub raised: &'static str,
}

// DEDUP_OFFICE D16: the chrome no longer has colours of its own. Surfaces
// are tints over the ground the ShellThemeScope paints (flat's grey, flora's
// paper), lines and text are `system:` colours, the selection is the app
// accent's soft wash (`--az-accent-soft`, published by the scope with its
// night value) - so flat and flora, light and dark each look like themselves.
// Only the canvas workspace keeps app colours (`view::ViewColors`).

pub const LIGHT: Palette = Palette {
    dark: false,
    chrome: "rgba(0, 0, 0, 0.04)",
    panel: "transparent",
    line: "system:separator",
    text: "system:text",
    muted: "system:secondary-text",
    selected: "var(--az-accent-soft, #eae0dd)",
    ruler: "rgba(0, 0, 0, 0.06)",
    raised: "rgba(255, 255, 255, 0.7)",
};

pub const DARK: Palette = Palette {
    dark: true,
    chrome: "rgba(255, 255, 255, 0.04)",
    panel: "transparent",
    line: "system:separator",
    text: "system:text",
    muted: "system:secondary-text",
    selected: "var(--az-accent-soft, #5e332d)",
    ruler: "rgba(255, 255, 255, 0.06)",
    raised: "rgba(255, 255, 255, 0.08)",
};

// ==== Small builders ====

/// A selected row's fill (a layer, the history's current step): the app accent's soft wash;
/// under flora the theme's selection, as every list of the engine shows it (flora's soft
/// wash by day, its stone at night - not the scope's clay).
fn selected_css(p: &Palette) -> String {
    format!(
        "background: {}; @theme(flora) {{ background: system:selection-background; }}",
        p.selected
    )
}

/// Choices: keys said in the window's language, plain words (a font family) as they are.
fn strings(items: &[&str]) -> StringVec {
    items.iter().map(|s| label(s)).collect::<Vec<AzString>>().into()
}

/// A key said, plain words (a name, a size) as they are.
fn text(s: &str, css: &str) -> Dom {
    Dom::create_span_with_text(label(s)).with_css(css)
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
fn button(app: &RefAny, name: &str, command: Command) -> Dom {
    Button::create(label(name))
        .with_on_click(cmd(app, command), commands::on_command as ButtonOnClickCallbackType)
        .dom()
}

/// An icon button that runs `command` (named for assistive technology by
/// its label).
fn icon_button(app: &RefAny, icon: &str, name: &str, command: Command) -> Dom {
    Button::create(AzString::from(""))
        .with_icon(AzString::from(icon))
        .with_on_click(cmd(app, command), commands::on_command as ButtonOnClickCallbackType)
        .dom()
        .with_accessibility_assign(AccessibilityInfo::named(label(name), AccessibilityRole::PushButton))
}

/// A labelled number field.
fn number(app: &RefAny, name: &str, value: f32, f: Field, p: &Palette) -> Dom {
    row("margin-right: 10px;")
        .with_child(text(name, &format!("font-size: 12px; color: {}; margin-right: 4px;", p.muted)))
        .with_child(
            NumberInput::create(value)
                .with_accessibility_name(label(name))
                .with_on_value_change(field(app, f), commands::on_number as NumberInputOnValueChangeCallbackType)
                .dom()
                .with_css("width: 64px;"),
        )
}

/// A labelled slider.
fn slider(app: &RefAny, name: &str, value: f32, min: f32, max: f32, f: Field, p: &Palette) -> Dom {
    row("margin-right: 10px;")
        .with_child(text(name, &format!("font-size: 12px; color: {}; margin-right: 4px;", p.muted)))
        .with_child(
            Slider::create(value, min, max)
                .with_accessibility_name(label(name))
                .with_on_value_change(field(app, f), commands::on_slider as SliderOnValueChangeCallbackType)
                .dom()
                .with_css("width: 110px;"),
        )
        .with_child(text(&format!("{}", value.round()), &format!("font-size: 12px; color: {}; margin-left: 4px; width: 32px;", p.muted)))
}

/// A labelled check box.
fn check(app: &RefAny, name: &str, value: bool, f: Field, p: &Palette) -> Dom {
    row("margin-right: 10px;")
        .with_child(
            CheckBox::create(value)
                .with_accessibility_name(label(name))
                .with_on_toggle(field(app, f), commands::on_check as CheckBoxOnToggleCallbackType)
                .dom(),
        )
        .with_child(text(name, &format!("font-size: 12px; color: {}; margin-left: 4px;", p.text)))
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
        .map(|(i, a)| Item(a.message_id(), C::NewAdjustment(i), NONE))
        .collect();
    vec![
        (
            "azphoto-menu-file",
            vec![
                Item("azphoto-new", C::Sheet(Sheet::NewImage), &[K::N]),
                Item("azphoto-open", C::Open, &[K::O]),
                Item("azphoto-open-sample-menu", C::OpenSample, NONE),
                Item("azphoto-place-image-as-layer", C::Place, NONE),
                Separator,
                Item("azphoto-save", C::Save, &[K::S]),
                Item("azphoto-export", C::Sheet(Sheet::Export), &[K::LShift, K::E]),
                Separator,
                Item("azphoto-close", C::CloseDocument, NONE),
            ],
        ),
        (
            "azphoto-menu-edit",
            vec![
                Item("azphoto-undo", C::Undo, &[K::Z]),
                Item("azphoto-redo", C::Redo, &[K::LShift, K::Z]),
                Separator,
                Item("azphoto-fill-foreground", C::Fill, NONE),
                Item("azphoto-history-clear", C::Clear, NONE),
                Separator,
                Item("azphoto-settings", C::Settings, NONE),
            ],
        ),
        (
            "azphoto-menu-image",
            vec![
                Item("azphoto-image-size", C::Sheet(Sheet::ImageSize), NONE),
                Item("azphoto-canvas-size", C::Sheet(Sheet::CanvasSize), NONE),
                Separator,
                Item("azphoto-rotate-90-clockwise", C::RotateCanvas(true), NONE),
                Item("azphoto-rotate-90-counter-clockwise", C::RotateCanvas(false), NONE),
                Item("azphoto-flip-canvas-horizontal", C::FlipCanvas(true), NONE),
                Item("azphoto-flip-canvas-vertical", C::FlipCanvas(false), NONE),
                Separator,
                Item("azphoto-crop-selection", C::CropToSelection, NONE),
            ],
        ),
        (
            "azphoto-menu-layer",
            vec![
                Item("azphoto-history-new-layer", C::NewLayer, &[K::LShift, K::N]),
                Item("azphoto-history-new-group", C::NewGroup, NONE),
                Sub("azphoto-new-adjustment-layer", adjustments),
                Item("azphoto-history-duplicate-layer", C::Duplicate, &[K::J]),
                Item("azphoto-history-delete-layer", C::Delete, NONE),
                Item("azphoto-history-merge-down", C::MergeDown, &[K::E]),
                Separator,
                Sub(
                    "azphoto-history-transform",
                    vec![
                        Item("azphoto-flip-horizontal", C::FlipLayer(true), NONE),
                        Item("azphoto-flip-vertical", C::FlipLayer(false), NONE),
                        Item("azphoto-rotate", C::Sheet(Sheet::Rotate), NONE),
                        Item("azphoto-scale-50", C::ScaleLayer(0.5), NONE),
                        Item("azphoto-scale-200", C::ScaleLayer(2.0), NONE),
                    ],
                ),
                Item("azphoto-layer-move-up", C::LayerUp, NONE),
                Item("azphoto-layer-move-down", C::LayerDown, NONE),
            ],
        ),
        (
            "azphoto-menu-select",
            vec![
                Item("azphoto-all", C::SelectAll, &[K::A]),
                Item("azphoto-history-deselect", C::Deselect, &[K::D]),
                Item("azphoto-history-inverse", C::Inverse, &[K::LShift, K::I]),
                Item("azphoto-feather", C::Sheet(Sheet::Feather), NONE),
            ],
        ),
        (
            "azphoto-menu-filter",
            vec![
                Item("azphoto-gaussian-blur", C::Sheet(Sheet::GaussianBlur), NONE),
                Item("azphoto-sharpen", C::Sheet(Sheet::Sharpen), NONE),
            ],
        ),
        (
            "azphoto-menu-view",
            vec![
                Item("azphoto-zoom-in", C::ZoomIn, &[K::Equals]),
                Item("azphoto-zoom-out", C::ZoomOut, &[K::Minus]),
                Item("azphoto-fit-screen", C::Fit, &[K::Key0]),
                Item("azphoto-actual-pixels", C::ActualPixels, &[K::Key1]),
                Separator,
                Item("azphoto-light-mode", C::Mode(false), NONE),
                Item("azphoto-dark-mode", C::Mode(true), NONE),
                Item("azphoto-flat-theme", C::Theme("flat"), NONE),
                Item("azphoto-flora-theme", C::Theme("flora"), NONE),
            ],
        ),
        (
            "azphoto-menu-help",
            vec![
                Item("azphoto-keyboard-shortcuts", C::Settings, NONE),
                Item("azphoto-about-azphoto", C::Sheet(Sheet::About), NONE),
            ],
        ),
    ]
}

fn menu_items(app: &RefAny, entries: Vec<Entry>) -> Vec<MenuItem> {
    entries
        .into_iter()
        .map(|e| match e {
            Entry::Item(name, command, keys) => {
                let mut item = StringMenuItem::create(label(name))
                    .with_callback(cmd(app, command), commands::on_command);
                if !keys.is_empty() {
                    let mut combo = vec![cmd_key()];
                    combo.extend_from_slice(keys);
                    item.accelerator = OptionVirtualKeyCodeCombo::Some(VirtualKeyCodeCombo { keys: combo.into() });
                }
                MenuItem::string(item)
            }
            Entry::Sub(name, children) => MenuItem::string(
                StringMenuItem::create(label(name)).with_children(menu_items(app, children)),
            ),
            Entry::Separator => MenuItem::separator(),
        })
        .collect()
}

/// Whether the window draws its own menu row: everywhere but on macOS, whose
/// menu bar at the top of the screen holds the window's menu (`layout`
/// hands it `native_menu`). Both at once showed every menu twice.
#[must_use]
pub const fn in_window_menu() -> bool {
    !cfg!(target_os = "macos")
}

/// The whole menu bar (the native one on macOS).
pub fn native_menu(app: &RefAny) -> Menu {
    Menu::create(
        menu_table()
            .into_iter()
            .map(|(title, entries)| {
                MenuItem::string(StringMenuItem::create(label(title)).with_children(menu_items(app, entries)))
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
    let mut bar = row(&format!("padding: 2px 6px; background: {}; flex-shrink: 0;", p.chrome)).with_id(ids::MENU);
    for (i, (title, _)) in menu_table().iter().enumerate() {
        bar.add_child(
            Button::create(label(title))
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
        .with_id(ids::OPTIONS)
        .with_child(text(
            &crate::l10n::word("tool", s.tool.name()),
            &format!("font-size: 12px; font-weight: bold; color: {}; margin-right: 12px;", p.text),
        ));
    match s.tool {
        Tool::Brush | Tool::Pencil | Tool::Eraser | Tool::CloneStamp => {
            bar.add_child(number(app, "azphoto-size", s.brush_size(), Field::BrushSize, p));
            if s.tool != Tool::Pencil {
                bar.add_child(slider(app, "azphoto-hardness", o.hardness * 100.0, 0.0, 100.0, Field::Hardness, p));
            }
            bar.add_child(slider(app, "azphoto-history-opacity", o.opacity * 100.0, 1.0, 100.0, Field::Opacity, p));
            bar.add_child(slider(app, "azphoto-flow", o.flow * 100.0, 1.0, 100.0, Field::Flow, p));
            bar.add_child(check(app, "azphoto-pressure-size", o.pressure_size, Field::PressureSize, p));
            bar.add_child(check(app, "azphoto-pressure-flow", o.pressure_flow, Field::PressureFlow, p));
            if s.tool == Tool::CloneStamp {
                bar.add_child(hint(match o.clone_source {
                    Some(_) => "azphoto-source-set",
                    None => "azphoto-source-how",
                }));
            }
        }
        Tool::MarqueeRect | Tool::MarqueeEllipse | Tool::Lasso => {
            let words: Vec<String> = SelectMode::ALL.iter().map(|m| crate::l10n::word("select", m.name())).collect();
            let modes: Vec<&str> = words.iter().map(String::as_str).collect();
            let at = SelectMode::ALL.iter().position(|m| *m == o.select_mode).unwrap_or(0);
            bar.add_child(segments(app, &modes, at, Field::SelectMode));
            bar.add_child(Dom::create_div().with_css("width: 12px;"));
            bar.add_child(number(app, "azphoto-history-feather", o.feather, Field::Feather, p));
            bar.add_child(hint("azphoto-shift-adds-alt-subtracts"));
        }
        Tool::MagicWand => {
            let words: Vec<String> = SelectMode::ALL.iter().map(|m| crate::l10n::word("select", m.name())).collect();
            let modes: Vec<&str> = words.iter().map(String::as_str).collect();
            let at = SelectMode::ALL.iter().position(|m| *m == o.select_mode).unwrap_or(0);
            bar.add_child(segments(app, &modes, at, Field::SelectMode));
            bar.add_child(Dom::create_div().with_css("width: 12px;"));
            bar.add_child(number(app, "azphoto-tolerance", f32::from(o.wand_tolerance), Field::WandTolerance, p));
            bar.add_child(check(app, "azphoto-contiguous", o.wand_contiguous, Field::WandContiguous, p));
            bar.add_child(check(app, "azphoto-sample-all-layers", o.sample_merged, Field::SampleMerged, p));
        }
        Tool::Bucket => {
            bar.add_child(number(app, "azphoto-tolerance", f32::from(o.bucket_tolerance), Field::BucketTolerance, p));
            bar.add_child(check(app, "azphoto-contiguous", o.bucket_contiguous, Field::BucketContiguous, p));
        }
        Tool::Gradient => {
            bar.add_child(hint("azphoto-drag-foreground-colour-background"));
        }
        Tool::Shape => {
            bar.add_child(segments(app, &["azphoto-shape-rectangle", "azphoto-shape-ellipse"], usize::from(o.shape_ellipse), Field::ShapeKind));
            bar.add_child(hint("azphoto-filled-foreground-colour-shift"));
        }
        Tool::Crop => {
            bar.add_child(hint("azphoto-drag-frame-keep-image"));
        }
        Tool::Move => bar.add_child(hint("azphoto-drag-move-active-layer")),
        Tool::Eyedropper => bar.add_child(hint("azphoto-click-picks-foreground-colour")),
        Tool::Hand | Tool::Zoom => {
            bar.add_child(button(app, "azphoto-zoom-in", Command::ZoomIn));
            bar.add_child(button(app, "azphoto-zoom-out", Command::ZoomOut));
            bar.add_child(button(app, "azphoto-fit", Command::Fit));
            bar.add_child(button(app, "100 %", Command::ActualPixels));
        }
        Tool::Text => {
            bar.add_child(
                DropDown::create(strings(&TEXT_FAMILIES))
                    .with_selected(o.text_family)
                    .with_accessibility_name(AzString::from(label("azphoto-font")))
                    .with_on_choice_change(field(app, Field::TextFamily), commands::on_choice as DropDownOnChoiceChangeCallbackType)
                    .dom()
                    .with_id(ids::TEXT_FAMILY)
                    .with_css("margin-right: 10px;"),
            );
            bar.add_child(number(app, "azphoto-size", o.text_size, Field::TextSize, p));
            bar.add_child(check(app, "azphoto-bold", o.text_bold, Field::TextBold, p));
            bar.add_child(check(app, "azphoto-italic", o.text_italic, Field::TextItalic, p));
            match &s.text {
                Some(draft) => {
                    bar.add_child(
                        TextInput::create()
                            .with_text(AzString::from(draft.text.as_str()))
                            .with_placeholder(AzString::from(label("azphoto-type-text")))
                            .with_accessibility_name(AzString::from(label("azphoto-tool-text")))
                            .with_on_text_input(field(app, Field::Text), commands::on_text as TextInputOnTextInputCallbackType)
                            .dom()
                            .with_id(ids::TEXT_FIELD)
                            .with_css("width: 220px; margin-right: 8px;"),
                    );
                    bar.add_child(
                        Button::with_type(AzString::from(label("azphoto-commit")), ButtonType::Primary)
                            .with_on_click(cmd(app, Command::TextCommit), commands::on_command as ButtonOnClickCallbackType)
                            .dom()
                            .with_id(ids::TEXT_COMMIT)
                            .with_css("margin-right: 6px;"),
                    );
                    bar.add_child(button(app, "kit-button-cancel", Command::TextCancel).with_id(ids::TEXT_CANCEL));
                }
                None => bar.add_child(hint("azphoto-click-canvas-where-text")),
            }
        }
    }
    bar.add_child(Dom::create_div().with_css("flex-grow: 1;"));
    if !s.status.is_empty() {
        bar.add_child(text(&s.status, &format!("font-size: 12px; color: {};", p.muted)).with_id(ids::STATUS_LINE));
    }
    bar
}

/// The tools column, the colour chips under it.
fn tools_column(app: &RefAny, a: &PhotoApp, p: &Palette) -> Dom {
    let mut col = column(&format!("align-items: center; padding: 4px 0px; background: {};", p.chrome)).with_id(ids::TOOLS);
    for t in Tool::ALL {
        // The chosen tool is a toggled button; a tool that cannot act on the
        // active layer (a pixel tool on an adjustment or a locked layer) is
        // disabled and says why.
        let mut b = Button::create(AzString::from(""))
            .with_icon(AzString::from(t.icon()))
            .with_toggled(t == a.s.tool);
        if !a.s.tool_available(t) {
            b = b.with_disabled(AzString::from(label("azphoto-needs-pixel-layer-not")));
        }
        col.add_child(
            b.with_on_click(cmd(app, Command::Tool(t)), commands::on_command as ButtonOnClickCallbackType)
                .dom()
                .with_id(t.dom_id())
                .with_css("margin: 1px;")
                .with_accessibility_assign(AccessibilityInfo::named(
                    format!("{} ({})", crate::l10n::word("tool", t.name()), t.key()),
                    AccessibilityRole::PushButton,
                )),
        );
    }
    let chip = |c: [u8; 4], id: AzString| {
        Dom::create_div()
            .with_id(id)
            .with_css(format!("width: 18px; height: 18px; background: {}; border: 1px solid {};", hex(c), p.line))
    };
    col.add_child(Dom::create_div().with_css("height: 8px;"));
    col.add_child(
        Dom::create_div()
            .with_css("position: relative; width: 30px; height: 30px;")
            .with_child(chip(a.s.bg, ids::BG_CHIP).with_css("position: absolute; left: 10px; top: 10px;"))
            .with_child(chip(a.s.fg, ids::FG_CHIP).with_css("position: absolute; left: 0px; top: 0px;"))
            .clickable("azphoto-swap-colours-x", cmd(app, Command::SwapColors), commands::on_command),
    );
    col
}

// ==== Panels ====

fn section(title: &str, id: AzString, p: &Palette, body: Dom) -> Dom {
    section_with(title, id, p, None, body)
}

/// A panel section whose title row also carries `actions` (right-aligned,
/// Photoshop's place for a panel's own buttons).
fn section_with(title: &str, id: AzString, p: &Palette, actions: Option<Dom>, body: Dom) -> Dom {
    let title = text(
        title,
        &format!("font-size: 11px; font-weight: bold; color: {}; flex-grow: 1;", p.muted),
    );
    let mut head = row("align-items: center; margin-bottom: 4px;").with_child(title);
    if let Some(actions) = actions {
        head.add_child(actions);
    }
    column(&format!("border-bottom: 1px solid {}; padding: 6px 8px; flex-shrink: 0;", p.line))
        .with_id(id)
        .with_child(head)
        .with_child(body)
}

fn navigator_panel(app: &RefAny, a: &PhotoApp, p: &Palette) -> Dom {
    let thumb = view::thumbnail(a.s.engine.composite(), 220, 130, a.s.colors);
    let mut body = column("align-items: center;");
    if let Some(image) = crate::codec::view_image(thumb.width, thumb.height, &thumb.bgra) {
        body.add_child(
            Dom::create_image(image)
                .with_id(ids::NAVIGATOR_IMAGE)
                .with_css(format!("width: {}px; height: {}px; margin-bottom: 4px;", thumb.width, thumb.height)),
        );
    }
    body.add_child(
        row("")
            .with_child(icon_button(app, "zoom_out", "azphoto-navigator-zoom-out", Command::ZoomOut))
            .with_child(
                NumberInput::create((a.s.view.zoom * 100.0 * 10.0).round() / 10.0)
                    .with_accessibility_name(AzString::from(label("azphoto-zoom-percent")))
                    .with_on_value_change(field(app, Field::ZoomPercent), commands::on_number as NumberInputOnValueChangeCallbackType)
                    .dom()
                    .with_id(ids::ZOOM_INPUT)
                    .with_css("width: 70px;"),
            )
            .with_child(icon_button(app, "zoom_in", "azphoto-navigator-zoom-in", Command::ZoomIn))
            .with_child(button(app, "azphoto-fit", Command::Fit)),
    );
    section("azphoto-navigator", ids::PANEL_NAVIGATOR, p, body)
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
                .with_id(ids::numbered(ids::SWATCH_PREFIX, i))
                .with_css(format!(
                    "width: 18px; height: 18px; margin: 0px 3px 3px 0px; background: {}; border: 1px solid {};",
                    hex(*c),
                    p.line
                ))
                .clickable(&hex(*c), cmd(app, Command::Swatch(i)), commands::on_command),
        );
    }
    let body = column("")
        .with_child(
            row("")
                .with_child(
                    ColorInput::create(color(a.s.fg))
                        .with_accessibility_name(AzString::from(label("azphoto-foreground-colour")))
                        .with_on_value_change(field(app, Field::FgColor), commands::on_color as ColorInputOnValueChangeCallbackType)
                        .dom(),
                )
                .with_child(text(
                    &t_args("azphoto-foreground-hex", &[("hex", Arg::from(hex(a.s.fg)))]),
                    &format!("font-size: 12px; color: {}; margin: 0px 8px;", p.text),
                ))
                .with_child(
                    ColorInput::create(color(a.s.bg))
                        .with_accessibility_name(AzString::from(label("azphoto-background-colour")))
                        .with_on_value_change(field(app, Field::BgColor), commands::on_color as ColorInputOnValueChangeCallbackType)
                        .dom(),
                )
                .with_child(text("azphoto-background", &format!("font-size: 12px; color: {}; margin-left: 8px;", p.text))),
        )
        .with_child(
            row("margin-top: 4px;")
                .with_child(icon_button(app, "swap_horiz", "azphoto-swap-colours-x", Command::SwapColors))
                .with_child(button(app, "azphoto-default", Command::DefaultColors)),
        )
        .with_child(swatches);
    section("azphoto-color", ids::PANEL_COLOR, p, body)
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
    let blend_words: Vec<String> = BlendMode::ALL.iter().map(|m| crate::l10n::word("blend", m.name())).collect();
    let blend_names: Vec<&str> = blend_words.iter().map(String::as_str).collect();
    let mut body = column("");
    if let Some(l) = active_layer {
        body.add_child(
            row("margin-bottom: 4px;")
                .with_child(
                    DropDown::create(strings(&blend_names))
                        .with_selected(l.blend.index())
                        .with_accessibility_name(AzString::from(label("azphoto-blend-mode")))
                        .with_on_choice_change(field(app, Field::LayerBlend), commands::on_choice as DropDownOnChoiceChangeCallbackType)
                        .dom()
                        .with_id(ids::LAYER_BLEND)
                        .with_css("margin-right: 8px;"),
                )
                .with_child(text("azphoto-history-opacity", &format!("font-size: 12px; color: {}; margin-right: 4px;", p.muted)))
                .with_child(
                    NumberInput::create((l.opacity * 100.0).round())
                        .with_accessibility_name(AzString::from(label("azphoto-layer-opacity-percent")))
                        .with_on_value_change(field(app, Field::LayerOpacity), commands::on_number as NumberInputOnValueChangeCallbackType)
                        .dom()
                        .with_id(ids::LAYER_OPACITY_INPUT)
                        .with_css("width: 52px;"),
                ),
        );
        body.add_child(
            Slider::create((l.opacity * 100.0).round(), 0.0, 100.0)
                .with_accessibility_name(AzString::from(label("azphoto-layer-opacity-name")))
                .with_on_value_change(field(app, Field::LayerOpacity), commands::on_slider as SliderOnValueChangeCallbackType)
                .dom()
                .with_id(ids::LAYER_OPACITY)
                .with_css("margin-bottom: 4px;"),
        );
        // Photoshop's lock row: the locks act on the active layer, a row only
        // shows that its layer is locked.
        body.add_child(
            row("align-items: center; margin-bottom: 6px;")
                .with_child(text("azphoto-lock", &format!("font-size: 12px; color: {}; margin-right: 6px;", p.muted)))
                .with_child(
                    Button::create(AzString::from(""))
                        .with_icon(AzString::from(if l.locked { "lock" } else { "lock_open" }))
                        .with_toggled(l.locked)
                        .with_on_click(cmd(app, Command::ToggleLock(l.id)), commands::on_command as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::LAYER_LOCK)
                        .with_accessibility_assign(AccessibilityInfo::named(label("azphoto-lock-unlock"), AccessibilityRole::PushButton)),
                ),
        );
    }
    let mut list = column(&format!("border: 1px solid {}; min-height: 60px;", p.line)).with_id(ids::LAYER_LIST);
    for (depth, id) in layer::rows(&doc.layers) {
        let Some(l) = doc.layer(id) else {
            continue;
        };
        let selected = Some(id) == active;
        let mut r = row(&format!(
            "align-items: center; min-height: 34px; padding: 2px 6px 2px {}px; border-bottom: 1px solid {}; {}",
            4 + depth * 14,
            p.line,
            if selected { selected_css(p) } else { String::new() }
        ))
        .with_id(ids::numbered(ids::LAYER_ROW_PREFIX, id))
        .with_callback(EventFilter::Hover(HoverEventFilter::MouseDown), cmd(app, Command::LayerPress(id)), commands::on_command)
        .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp), cmd(app, Command::LayerRelease(id)), commands::on_command)
        // The pointer selects by pressing (and drags from there); Tab + Enter selects too.
        .clickable(l.name.as_str(), cmd(app, Command::SelectLayer(id)), commands::on_command)
        .with_child(icon_button(
            app,
            if l.visible { "visibility" } else { "visibility_off" },
            "azphoto-show-hide",
            Command::ToggleVisible(id),
        ));
        if matches!(l.content, LayerContent::Group(_)) {
            r.add_child(icon_button(
                app,
                if l.expanded { "expand_more" } else { "chevron_right" },
                "azphoto-open-close-group",
                Command::ToggleExpanded(id),
            ));
        }
        // The layer's thumbnail box, its kind's mark inside.
        r.add_child(
            Dom::create_div()
                .with_css(format!(
                    "width: 28px; height: 28px; margin: 0px 8px 0px 4px; display: flex; align-items: center; \
                     justify-content: center; background: {}; border: 1px solid {};",
                    p.chrome, p.line
                ))
                .with_child(Dom::create_icon(AzString::from(layer_icon(&l.content))).with_css("font-size: 16px;")),
        );
        r.add_child(text(&l.name, &format!("font-size: 13px; color: {}; flex-grow: 1;", p.text)));
        if l.opacity < 0.999 || l.blend != BlendMode::Normal {
            r.add_child(text(
                &format!("{} {} %", crate::l10n::word("blend", l.blend.name()), (l.opacity * 100.0).round()),
                &format!("font-size: 11px; color: {}; margin-right: 4px;", p.muted),
            ));
        }
        if l.locked {
            r.add_child(Dom::create_icon(AzString::from("lock")).with_css(format!("font-size: 14px; color: {};", p.muted)));
        }
        list.add_child(r);
    }
    body.add_child(list);
    // Photoshop's bar under the list: the layer's tools, then new and delete
    // at its right end. Moving a layer is a drag of its row (or Layer >
    // Arrange).
    body.add_child(
        row("align-items: center; margin-top: 4px;")
            .with_child(icon_button(app, "create_new_folder", "azphoto-new-group", Command::NewGroup).with_id(ids::LAYER_NEW_GROUP))
            .with_child(icon_button(app, "control_point_duplicate", "azphoto-duplicate-layer", Command::Duplicate).with_id(ids::LAYER_DUPLICATE))
            .with_child(icon_button(app, "merge_type", "azphoto-merge-down", Command::MergeDown).with_id(ids::LAYER_MERGE))
            .with_child(Dom::create_div().with_css("flex-grow: 1;"))
            .with_child(icon_button(app, "add", "azphoto-new-layer", Command::NewLayer).with_id(ids::LAYER_NEW))
            .with_child(icon_button(app, "delete", "azphoto-delete-layer", Command::Delete).with_id(ids::LAYER_DELETE)),
    );
    section("azphoto-layers", ids::PANEL_LAYERS, p, body)
}

fn adjustments_panel(app: &RefAny, p: &Palette) -> Dom {
    let mut body = row("flex-wrap: wrap;");
    for (i, a) in Adjustment::catalog().iter().enumerate() {
        body.add_child(button(app, a.message_id(), Command::NewAdjustment(i)).with_id(ids::numbered(ids::ADJUST_PREFIX, i)).with_css("margin: 0px 4px 4px 0px;"));
    }
    section("azphoto-adjustments", ids::PANEL_ADJUSTMENTS, p, body)
}

fn properties_panel(app: &RefAny, a: &PhotoApp, p: &Palette) -> Dom {
    let doc = a.s.engine.document();
    let mut body = column("");
    let small = format!("font-size: 12px; color: {};", p.text);
    match a.s.engine.active_layer().and_then(|id| doc.layer(id)) {
        None => body.add_child(text("azphoto-no-layer-selected", &small)),
        Some(l) => {
            body.add_child(text(&format!("{} - {}", l.name, crate::l10n::word("layer", l.kind_name())), &small));
            match &l.content {
                LayerContent::Raster(g) => body.add_child(text(
                    &t_args(
                        "azphoto-raster-info",
                        &[
                            ("w", Arg::from(g.width().to_string())),
                            ("h", Arg::from(g.height().to_string())),
                            ("used", Arg::from(g.non_empty_tiles().len().to_string())),
                            ("tiles", Arg::from((g.cols() * g.rows()).to_string())),
                        ],
                    ),
                    &format!("font-size: 11px; color: {};", p.muted),
                )),
                LayerContent::Group(children) => {
                    body.add_child(text(
                        &t_args("azphoto-group-layers", &[("count", Arg::from(children.len()))]),
                        &format!("font-size: 11px; color: {};", p.muted),
                    ));
                }
                LayerContent::Adjustment(adj) => {
                    let params = commands::adjustment_params(adj);
                    if params.is_empty() {
                        body.add_child(text("azphoto-no-settings", &format!("font-size: 11px; color: {};", p.muted)));
                    }
                    for (n, (name, v, min, max)) in params.into_iter().enumerate() {
                        let name = if name == "azphoto-param-point" {
                            t_args("azphoto-param-point-n", &[("n", Arg::from((n + 1).to_string()))])
                        } else {
                            name.to_string()
                        };
                        body.add_child(slider(app, &name, v, min, max, Field::Adjust(n as u8), p));
                    }
                }
            }
        }
    }
    if let Some(label) = commands::selection_label(a) {
        body.add_child(text(&label, &format!("font-size: 11px; color: {}; margin-top: 4px;", p.muted)));
    }
    section("azphoto-properties", ids::PANEL_PROPERTIES, p, body)
}

fn history_panel(app: &RefAny, a: &PhotoApp, p: &Palette) -> Dom {
    let (labels, current) = a.s.engine.history();
    let mut list = column("").with_id(ids::HISTORY_LIST);
    for (i, label) in labels.iter().enumerate() {
        let css = format!(
            "font-size: 12px; padding: 2px 6px; color: {}; {}",
            if i > current { p.muted } else { p.text },
            if i == current { selected_css(p) } else { String::new() }
        );
        list.add_child(
            text(label, &css)
                .with_id(ids::numbered(ids::HISTORY_PREFIX, i))
                .clickable(label, cmd(app, Command::HistoryJump(i)), commands::on_command),
        );
    }
    // Undo / redo sit in the title row: below the list they ended under the
    // status bar at 1400 x 900 (the panels column 13 px too tall).
    let actions = row("")
        .with_child(icon_button(app, "undo", "azphoto-undo", Command::Undo).with_id(ids::HISTORY_UNDO))
        .with_child(icon_button(app, "redo", "azphoto-redo", Command::Redo).with_id(ids::HISTORY_REDO));
    section_with("azphoto-history", ids::PANEL_HISTORY, p, Some(actions), list)
}

/// The panels column (scrolls when it is taller than the window).
fn panels(app: &RefAny, a: &PhotoApp, p: &Palette) -> Dom {
    column(&format!("background: {}; overflow-y: auto; min-height: 0px; flex-grow: 1;", p.panel))
        .with_id(ids::PANELS)
        // Photoshop's Essentials column, top to bottom: colour, the active
        // layer's properties, adjustments, the layers; then history and the
        // navigator.
        .with_child(color_panel(app, a, p))
        .with_child(properties_panel(app, a, p))
        .with_child(adjustments_panel(app, p))
        .with_child(layers_panel(app, a, p))
        .with_child(history_panel(app, a, p))
        .with_child(navigator_panel(app, a, p))
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
        .with_id(if horizontal { ids::RULER_X } else { ids::RULER_Y })
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
            row(&format!("padding: 2px 8px; background: {}; border-radius: 4px 4px 0px 0px;", p.raised))
                .with_id(ids::DOC_TAB)
                .with_child(text(
                    &format!("{}{}", a.s.name, if a.s.modified { " \u{25cf}" } else { "" }),
                    &format!("font-size: 12px; color: {}; margin-right: 6px;", p.text),
                ))
                .with_child(icon_button(app, "close", "azphoto-close-document", Command::CloseDocument)),
        )
}

fn status_bar(a: &PhotoApp) -> Dom {
    let (w, h) = a.s.engine.size();
    let mut segments = vec![
        StatusBarSegment::create(AzString::from(a.s.view.percent_label())).with_marker(ids::ZOOM),
        StatusBarSegment::create(AzString::from(format!("{w} x {h} px \u{b7} RGBA 8-bit"))),
        StatusBarSegment::create(AzString::from(canvas::cursor_label(a))).with_marker(ids::CURSOR),
    ];
    if a.busy > 0 {
        segments.push(StatusBarSegment::create(AzString::from(label("azphoto-working"))));
    }
    StatusBar::create(segments).dom().with_id(ids::STATUS)
}

// ==== Start screen ====

fn start_screen(app: &RefAny, a: &PhotoApp, p: &Palette) -> Dom {
    let mut recent = column("margin-top: 16px; min-width: 360px;").with_id(ids::RECENT);
    if a.recent.is_empty() {
        recent.add_child(
            ShellEmptyState::create(AzString::from(label("azphoto-no-saved-documents-yet")))
                .with_icon(AzString::from("photo_library"))
                .with_detail(AzString::from(label("azphoto-documents-you-save-appear")))
                .dom(),
        );
    } else {
        recent.add_child(text("azphoto-recent", &format!("font-size: 11px; font-weight: bold; color: {}; margin-bottom: 4px;", p.muted)));
        for (i, d) in a.recent.iter().enumerate().take(12) {
            recent.add_child(
                row("margin-bottom: 2px;")
                    .with_child(button(app, &d.name, Command::OpenRecent(d.uuid.clone())).with_id(ids::numbered(ids::RECENT_PREFIX, i)))
                    .with_child(text(&format!("{} x {} px", d.width, d.height), &format!("font-size: 12px; color: {}; margin-left: 8px;", p.muted))),
            );
        }
    }
    column(&format!("flex-grow: 1; align-items: center; justify-content: center; background: {};", p.panel))
        .with_id(ids::START)
        .with_child(text("AzPhoto", &format!("font-size: 28px; font-weight: bold; color: {}; margin-bottom: 12px;", p.text)))
        .with_child(
            row("")
                .with_child(button(app, "azphoto-open", Command::Open).with_id(ids::START_OPEN).with_css("margin-right: 8px;"))
                .with_child(button(app, "azphoto-new-image", Command::Sheet(Sheet::NewImage)).with_id(ids::START_NEW).with_css("margin-right: 8px;"))
                .with_child(button(app, "azphoto-open-sample", Command::OpenSample).with_id(ids::START_SAMPLE)),
        )
        .with_child(text(
            "azphoto-opens-png-jpeg-webp",
            &format!("font-size: 12px; color: {}; margin-top: 10px;", p.muted),
        ))
        .with_child(text(
            "azphoto-not-yet-raw-heic",
            &format!("font-size: 12px; color: {}; margin-top: 2px;", p.muted),
        ))
        .with_child(recent)
}

// ==== Sheets ====

/// A sheet: azul's modal `Dialog` (title, close button, Escape, focus) with
/// the body, then Cancel (Close) and the OK button when it has one.
fn sheet_frame(app: &RefAny, _p: &Palette, title: &str, body: Dom, ok: Option<(&str, Command)>) -> Dom {
    let mut buttons = row("justify-content: flex-end; margin-top: 12px;");
    buttons.add_child(button(app, if ok.is_some() { "kit-button-cancel" } else { "azphoto-close" }, Command::CloseSheet).with_id(ids::SHEET_CANCEL));
    if let Some((name, command)) = ok {
        buttons.add_child(
            Button::with_type(label(name), ButtonType::Primary)
                .with_on_click(cmd(app, command), commands::on_command as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::SHEET_OK)
                .with_css("margin-left: 8px;"),
        );
    }
    Dialog::create(column("min-width: 340px;").with_child(body).with_child(buttons))
        .with_title(label(title))
        .with_open(true)
        .with_modal(true)
        .with_close_button(true)
        .with_on_close(app.clone(), commands::on_sheet_close as DialogOnCloseCallbackType)
        .dom()
        .with_id(ids::SHEET)
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
                        .with_child(number(app, "azphoto-width", f.new_width, Field::NewWidth, p))
                        .with_child(number(app, "azphoto-height", f.new_height, Field::NewHeight, p)),
                )
                .with_child(row("margin-top: 8px;").with_child(check(app, "azphoto-transparent-background", f.new_transparent, Field::NewTransparent, p)))
                .with_child(hint(&t_args("azphoto-mib-per-layer", &[("mb", Arg::from(format!("{mb:.1}")))])));
            sheet_frame(app, p, "azphoto-new-image-title", body, Some(("azphoto-create", Command::NewImageCreate)))
        }
        Sheet::Export => {
            let jpeg = a.export_format == ExportFormat::Jpeg;
            let mut body = column("").with_child(segments(app, &["PNG", "JPEG"], usize::from(jpeg), Field::ExportFormat));
            if jpeg {
                body.add_child(row("margin-top: 8px;").with_child(number(app, "azphoto-quality", f32::from(a.jpeg_quality), Field::JpegQuality, p)));
                body.add_child(hint("azphoto-jpeg-has-no-transparency"));
            }
            let (w, h) = a.s.engine.size();
            body.add_child(hint(&t_args(
                "azphoto-export-flattened",
                &[("w", Arg::from(w.to_string())), ("h", Arg::from(h.to_string()))],
            )));
            body.add_child(hint(&t_args(
                "azphoto-export-into",
                &[(
                    "path",
                    Arg::from(crate::storage::export_key(&a.s.uuid, &commands::file_name(&a.s.name, a.export_format.extension()))),
                )],
            )));
            sheet_frame(app, p, "azphoto-export-title", body, Some(("azphoto-export-title", Command::ExportApply)))
        }
        Sheet::ImageSize => {
            let body = column("")
                .with_child(
                    row("")
                        .with_child(number(app, "azphoto-width", f.size_width, Field::SizeWidth, p))
                        .with_child(number(app, "azphoto-height", f.size_height, Field::SizeHeight, p)),
                )
                .with_child(hint("azphoto-every-layer-resampled-bilinear"));
            sheet_frame(app, p, "azphoto-image-size-title", body, Some(("azphoto-resize", Command::ImageSizeApply)))
        }
        Sheet::CanvasSize => {
            let body = column("")
                .with_child(
                    row("")
                        .with_child(number(app, "azphoto-width", f.canvas_width, Field::CanvasWidth, p))
                        .with_child(number(app, "azphoto-height", f.canvas_height, Field::CanvasHeight, p)),
                )
                .with_child(hint("azphoto-image-stays-centred-new"));
            sheet_frame(app, p, "azphoto-canvas-size-title", body, Some(("azphoto-resize", Command::CanvasSizeApply)))
        }
        Sheet::GaussianBlur => {
            let body = column("")
                .with_child(number(app, "azphoto-radius-sigma-px", f.blur_sigma, Field::BlurSigma, p))
                .with_child(hint("azphoto-blurs-active-layer-inside"));
            sheet_frame(app, p, "azphoto-gaussian-blur-title", body, Some(("azphoto-blur", Command::BlurApply)))
        }
        Sheet::Sharpen => {
            let body = column("")
                .with_child(
                    row("")
                        .with_child(number(app, "azphoto-amount", f.sharpen_amount, Field::SharpenAmount, p))
                        .with_child(number(app, "azphoto-radius", f.sharpen_radius, Field::SharpenRadius, p)),
                )
                .with_child(hint("azphoto-unsharp-mask-active-layer"));
            sheet_frame(app, p, "azphoto-filter-sharpen", body, Some(("azphoto-filter-sharpen", Command::SharpenApply)))
        }
        Sheet::Rotate => {
            let body = column("")
                .with_child(number(app, "azphoto-degrees", f.rotate_degrees, Field::RotateDegrees, p))
                .with_child(hint("azphoto-turns-active-layer-about"));
            sheet_frame(app, p, "azphoto-rotate-layer", body, Some(("azphoto-rotate-ok", Command::RotateLayerApply)))
        }
        Sheet::Feather => {
            let body = column("")
                .with_child(number(app, "azphoto-radius-px", f.feather, Field::FeatherForm, p))
                .with_child(hint("azphoto-softens-selection-s-edge"));
            sheet_frame(app, p, "azphoto-feather-selection", body, Some(("azphoto-history-feather", Command::FeatherApply)))
        }
        Sheet::About => {
            let about = AboutDialog::create(AzString::from(crate::ABOUT.name), AzString::from(crate::ABOUT.version))
                .with_icon(AzString::from("photo"))
                .with_description(AzString::from(azul_appkit::l10n::app_word(
                    crate::ABOUT.name,
                    "about-summary",
                    crate::ABOUT.summary,
                )))
                .with_copyright(AzString::from(t_args(
                    "azphoto-about-copyright",
                    &[
                        ("license", Arg::from(crate::ABOUT.license)),
                        ("folder", Arg::from(azul_appkit::data::local_path(&a.data_root, crate::ABOUT.app_folder).display().to_string())),
                    ],
                )))
                .with_credit(AzString::from("azul"), AzString::from("MIT"))
                .with_on_event(app.clone(), commands::on_about_event as StandardDialogOnEventCallbackType)
                .dom();
            Dialog::create(about)
                .with_title(AzString::from(label("azphoto-about-azphoto")))
                .with_open(true)
                .with_modal(true)
                .with_close_button(true)
                .with_on_close(app.clone(), commands::on_sheet_close as DialogOnCloseCallbackType)
                .dom()
                .with_id(ids::SHEET)
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
    let mut shell = CanvasShell::create(canvas_area(app, a, p));
    if in_window_menu() {
        shell = shell.with_menu_bar(menu_row(app, p));
    }
    shell
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
    azul_appkit::l10n::begin_layout(&info);
    let dark = matches!(info.get_mode(), DarkLightMode::Dark);
    let theme = info.get_theme().as_str().to_string();
    let app_ref = data.clone();
    let Some(mut guard) = data.downcast_mut::<PhotoApp>() else {
        return Dom::create_body();
    };
    let a = &mut *guard;
    // flora and its spins ("flora:green"): the workspace is flora's desk.
    let flora = theme == "flora" || theme.starts_with("flora:");
    if a.dark != dark || a.s.colors != view::ViewColors::for_look(dark, flora) {
        a.dark = dark;
        let _ = a.s.set_look(dark, flora);
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
    let content = column("flex-grow: 1; min-height: 0px;")
        .with_child(title_row(a))
        .with_child(area);
    // "Save changes?" before the window closes over unsaved work.
    let root = kit::close_guard(content, &a.s.name)
        .with_dirty(a.s.modified && a.screen == AppScreen::Editor)
        .with_asking(a.closing)
        .with_on_event(app_ref.clone(), commands::on_close_guard as CloseGuardOnEventCallbackType)
        .dom()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;");
    let body = Dom::create_body()
        .with_css("display: flex; flex-direction: column; margin: 0px; height: 100%;")
        .with_child(
            ShellThemeScope::create(root)
                .with_accent(ShellThemeAccent::Clay)
                .dom()
                .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;"),
        )
        .with_callback(EventFilter::Window(WindowEventFilter::VirtualKeyDown), app_ref.clone(), canvas::on_key);
    if in_window_menu() {
        body
    } else {
        body.with_menu_bar(native_menu(&app_ref))
    }
}
