//! Every command of the menus, buttons, rows and shortcuts, and every value
//! of the options bar, panels and sheets - one dispatch each.
//!
//! A button or menu item carries a [`Cmd`] (the app handle and a
//! [`Command`]) and calls [`on_command`]; a widget with a value carries a
//! [`FieldRef`] and calls the `on_*` callback of its kind. So there is one
//! function per kind of event, not one per button.

use std::path::PathBuf;

use azul_appkit::{
    args::{ModePref, Theme},
    ui as kit,
};

use azul::{
    css::{DarkLightMode, TextRasterStyle},
    dialog::{FileDialog, FileOpenResult},
    image::RawImageData,
    option::{OptionDarkLightMode, OptionFileTypeList},
    prelude::*,
    widgets::{
        CheckBoxState, ColorInputState, DialogState, NumberInputState, OnTextInputReturn, SegmentedState,
        CloseGuardEvent, CloseGuardEventKind, SliderState, StandardDialogEvent, TextInputState, TextInputValid,
    },
};

use crate::{
    canvas,
    jobs::{self, ExportFormat, Job},
    new_uuid,
    raster::{
        layer, Adjustment, Affine, BlendMode, Document, Filter, IRect, Interp, Layer, LayerContent, LayerId, Op,
        Placement, SelectMode,
    },
    sample_document, say,
    state::{Effects, TextSpec, Tool, TEXT_FAMILIES},
    AppScreen, PhotoApp, Sheet,
};

/// The Text tool's rasteriser: azul's text raster (`CallbackInfo::text_image`
/// - shaped by the text engine, rasterised by the CPU glyph path, with the
/// fonts the window already found) as straight RGBA8 rows.
#[must_use]
pub fn azul_text(info: &CallbackInfo, spec: &TextSpec) -> Option<(u32, u32, Vec<u8>)> {
    let [r, g, b, a] = spec.color;
    let style = TextRasterStyle::create(spec.family.as_str(), spec.size, ColorU { r, g, b, a })
        .with_bold(spec.bold)
        .with_italic(spec.italic);
    let image = info.text_image(spec.text.as_str(), style).into_option()?;
    let (width, height) = (image.width as u32, image.height as u32);
    match image.pixels {
        RawImageData::U8(bytes) => Some((width, height, bytes.as_ref().to_vec())),
        _ => None,
    }
}

/// What a button, menu item, row or shortcut does.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    // File
    Open,
    Place,
    OpenSample,
    OpenRecent(String),
    Save,
    CloseDocument,
    NewImageCreate,
    ExportApply,
    // Edit
    Undo,
    Redo,
    Fill,
    Clear,
    Escape,
    HistoryJump(usize),
    // Image
    RotateCanvas(bool),
    FlipCanvas(bool),
    CropToSelection,
    ImageSizeApply,
    CanvasSizeApply,
    // Layer
    NewLayer,
    NewGroup,
    Duplicate,
    Delete,
    MergeDown,
    NewAdjustment(usize),
    FlipLayer(bool),
    RotateLayerApply,
    ScaleLayer(f32),
    SelectLayer(LayerId),
    ToggleVisible(LayerId),
    ToggleLock(LayerId),
    ToggleExpanded(LayerId),
    LayerPress(LayerId),
    LayerRelease(LayerId),
    LayerUp,
    LayerDown,
    // Select
    SelectAll,
    Deselect,
    Inverse,
    FeatherApply,
    // Filter
    BlurApply,
    SharpenApply,
    // View
    ZoomIn,
    ZoomOut,
    Fit,
    ActualPixels,
    Mode(bool),
    Theme(&'static str),
    // Tools and colours
    Tool(Tool),
    BrushSize(bool),
    /// A digit key: the paint or the layer opacity (`0` = 100 %).
    Digit(u8),
    SwapColors,
    DefaultColors,
    Swatch(usize),
    // Sheets
    Sheet(Sheet),
    CloseSheet,
    // The Text tool
    TextCommit,
    TextCancel,
    /// azul-appkit's settings page (Appearance, Data, Shortcuts, About).
    Settings,
}

/// `f` on the kit (its settings, its switches).
fn with_kit(app: &PhotoApp, f: impl FnOnce(&mut kit::Kit)) {
    let mut handle = app.kit.clone();
    if let Some(mut k) = handle.downcast_mut::<kit::Kit>() {
        f(&mut k);
    };
}

/// A button's payload: the app and its command.
pub struct Cmd {
    pub app: RefAny,
    pub command: Command,
}

/// The payload of a button or menu item that runs `command`.
#[must_use]
pub fn cmd(app: &RefAny, command: Command) -> RefAny {
    RefAny::new(Cmd {
        app: app.clone(),
        command,
    })
}

/// A sheet's dialog was closed (its close button, Escape, the backdrop).
pub extern "C" fn on_sheet_close(mut data: RefAny, _info: CallbackInfo, _state: DialogState) -> Update {
    if let Some(mut a) = data.downcast_mut::<PhotoApp>() {
        a.sheet = None;
    }
    Update::RefreshDom
}

/// The close guard: a close of the window while the document has unsaved
/// changes was held - ask; then Save (and close once written), Don't Save
/// (the guard closes the window) or Cancel.
pub extern "C" fn on_close_guard(mut data: RefAny, mut info: CallbackInfo, event: CloseGuardEvent) -> Update {
    let handle = data.clone();
    let Some(mut guard) = data.downcast_mut::<PhotoApp>() else {
        return Update::DoNothing;
    };
    let a = &mut *guard;
    a.closing = false;
    match event.kind {
        CloseGuardEventKind::Ask => {
            a.closing = true;
            say("AZPHOTO_CLOSE_ASKED");
        }
        CloseGuardEventKind::Save => {
            let _ = a.s.commit_text();
            a.close_after_save = true;
            return run(a, &handle, &mut info, Command::Save);
        }
        CloseGuardEventKind::Discard => a.s.modified = false,
        CloseGuardEventKind::Cancel => {}
    }
    Update::RefreshDom
}

/// The About dialog's OK (or Cancel) closes it.
pub extern "C" fn on_about_event(data: RefAny, info: CallbackInfo, _event: StandardDialogEvent) -> Update {
    on_sheet_close(data, info, DialogState::default())
}

/// A button, menu item or row was used.
pub extern "C" fn on_command(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((app_ref, command)) = data
        .downcast_ref::<Cmd>()
        .map(|c| (c.app.clone(), c.command.clone()))
    else {
        return Update::DoNothing;
    };
    let mut target = app_ref.clone();
    let Some(mut guard) = target.downcast_mut::<PhotoApp>() else {
        return Update::DoNothing;
    };
    run(&mut guard, &app_ref, &mut info, command)
}

/// Run one command.
#[allow(clippy::too_many_lines)]
pub fn run(app: &mut PhotoApp, app_ref: &RefAny, info: &mut CallbackInfo, command: Command) -> Update {
    let e: Effects = match command {
        Command::Open => {
            let _ = FileDialog::open_file(
                "Open an image",
                OptionString::None,
                OptionFileTypeList::None,
                app_ref.clone(),
                on_open_picked,
            );
            return Update::DoNothing;
        }
        Command::Place => {
            let _ = FileDialog::open_file(
                "Place an image as a layer",
                OptionString::None,
                OptionFileTypeList::None,
                app_ref.clone(),
                on_place_picked,
            );
            return Update::DoNothing;
        }
        Command::OpenSample => {
            match sample_document() {
                Ok(doc) => app.open_document(doc, "Sample", &new_uuid(), "Open"),
                Err(e) => app.status(e),
            }
            return Update::RefreshDom;
        }
        Command::OpenRecent(uuid) => {
            app.busy += 1;
            app.status("Opening...");
            jobs::spawn(info, app_ref, Job::Load {
                drive: app.drive.clone(),
                uuid,
            });
            return Update::RefreshDom;
        }
        Command::Save => {
            app.busy += 1;
            app.status(format!("Saving {}...", app.s.name));
            jobs::spawn(info, app_ref, Job::Save {
                drive: app.drive.clone(),
                uuid: app.s.uuid.clone(),
                name: app.s.name.clone(),
                doc: app.s.engine.document().clone(),
            });
            return Update::RefreshDom;
        }
        Command::CloseDocument => {
            app.screen = AppScreen::Start;
            app.sheet = None;
            app.busy += 1;
            jobs::spawn(info, app_ref, Job::List {
                drive: app.drive.clone(),
            });
            return Update::RefreshDom;
        }
        Command::NewImageCreate => {
            let w = app.form.new_width.round().clamp(1.0, 30000.0) as u32;
            let h = app.form.new_height.round().clamp(1.0, 30000.0) as u32;
            let doc = if app.form.new_transparent {
                let mut d = Document::new(w, h);
                let id = d.mint_id();
                d.layers.push(Layer::raster(id, "Layer 1", crate::raster::TileGrid::new(w, h)));
                d
            } else {
                Document::with_background(w, h, [255, 255, 255, 255])
            };
            app.open_document(doc, "Untitled", &new_uuid(), "New");
            return Update::RefreshDom;
        }
        Command::ExportApply => {
            app.sheet = None;
            export(app, app_ref, info);
            return Update::RefreshDom;
        }
        Command::Undo => app.s.undo(),
        Command::Redo => app.s.redo(),
        Command::Fill => app.s.apply(Op::FillSelection(app.s.fg)),
        Command::Clear => app.s.apply(Op::ClearSelection),
        Command::Escape => {
            if app.sheet.take().is_some() {
                return Update::RefreshDom;
            }
            if app.s.text.is_none() {
                return Update::DoNothing;
            }
            app.s.cancel_text()
        }
        Command::TextCommit => app.s.commit_text(),
        Command::TextCancel => app.s.cancel_text(),
        Command::HistoryJump(i) => app.s.jump(i),
        Command::RotateCanvas(cw) => {
            let e = app.s.apply(Op::RotateCanvas90 { clockwise: cw });
            e.merge(app.s.fit())
        }
        Command::FlipCanvas(h) => app.s.apply(Op::FlipCanvas { horizontal: h }),
        Command::CropToSelection => {
            match app.s.engine.document().selection.as_ref().and_then(|m| m.bounds()) {
                Some(r) => {
                    let e = app.s.apply(Op::Crop(r));
                    e.merge(app.s.fit())
                }
                None => {
                    app.status("Select the part to keep first.");
                    Effects {
                        dom: true,
                        ..Effects::default()
                    }
                }
            }
        }
        Command::ImageSizeApply => {
            app.sheet = None;
            let w = app.form.size_width.round().clamp(1.0, 30000.0) as u32;
            let h = app.form.size_height.round().clamp(1.0, 30000.0) as u32;
            let e = app.s.apply(Op::ResizeImage {
                width: w,
                height: h,
                interp: Interp::Bilinear,
            });
            e.merge(app.s.fit())
        }
        Command::CanvasSizeApply => {
            app.sheet = None;
            let (ow, oh) = app.s.engine.size();
            let w = app.form.canvas_width.round().clamp(1.0, 30000.0) as u32;
            let h = app.form.canvas_height.round().clamp(1.0, 30000.0) as u32;
            let e = app.s.apply(Op::ResizeCanvas {
                width: w,
                height: h,
                x: (w as i32 - ow as i32) / 2,
                y: (h as i32 - oh as i32) / 2,
            });
            e.merge(app.s.fit())
        }
        Command::NewLayer => {
            let n = layer::all_ids(&app.s.engine.document().layers).len() + 1;
            app.s.apply(Op::NewLayer {
                name: format!("Layer {n}"),
            })
        }
        Command::NewGroup => app.s.apply(Op::NewGroup),
        Command::Duplicate => match app.s.engine.active_layer() {
            Some(id) => app.s.apply(Op::DuplicateLayer(id)),
            None => Effects::default(),
        },
        Command::Delete => match app.s.engine.active_layer() {
            Some(id) => app.s.apply(Op::DeleteLayer(id)),
            None => Effects::default(),
        },
        Command::MergeDown => match app.s.engine.active_layer() {
            Some(id) => app.s.apply(Op::MergeDown(id)),
            None => Effects::default(),
        },
        Command::NewAdjustment(i) => match Adjustment::catalog().get(i) {
            Some(a) => app.s.apply(Op::NewAdjustment(a.clone())),
            None => Effects::default(),
        },
        Command::FlipLayer(h) => app.s.apply(Op::FlipLayer { horizontal: h }),
        Command::RotateLayerApply => {
            app.sheet = None;
            let (w, h) = app.s.engine.size();
            let m = Affine::about(
                w as f32 / 2.0,
                h as f32 / 2.0,
                &Affine::rotate(app.form.rotate_degrees.to_radians()),
            );
            app.s.apply(Op::TransformLayer {
                m,
                interp: Interp::Bilinear,
            })
        }
        Command::ScaleLayer(f) => {
            let (w, h) = app.s.engine.size();
            let m = Affine::about(w as f32 / 2.0, h as f32 / 2.0, &Affine::scale(f, f));
            app.s.apply(Op::TransformLayer {
                m,
                interp: Interp::Bilinear,
            })
        }
        Command::SelectLayer(id) => {
            app.s.engine.set_active_layer(id);
            app.announce_layers();
            return Update::RefreshDom;
        }
        Command::ToggleVisible(id) => {
            let visible = app.s.engine.document().layer(id).is_some_and(|l| l.visible);
            app.s.apply(Op::SetVisible(id, !visible))
        }
        Command::ToggleLock(id) => {
            let locked = app.s.engine.document().layer(id).is_some_and(|l| l.locked);
            app.s.apply(Op::SetLocked(id, !locked))
        }
        Command::ToggleExpanded(id) => {
            let expanded = app.s.engine.document().layer(id).is_some_and(|l| l.expanded);
            app.s.apply(Op::SetExpanded(id, !expanded))
        }
        Command::LayerPress(id) => {
            app.layer_drag = Some(id);
            app.s.engine.set_active_layer(id);
            app.announce_layers();
            return Update::RefreshDom;
        }
        Command::LayerRelease(id) => {
            let Some(source) = app.layer_drag.take() else {
                return Update::DoNothing;
            };
            if source == id {
                return Update::DoNothing;
            }
            let into_group = app
                .s
                .engine
                .document()
                .layer(id)
                .is_some_and(|l| matches!(l.content, LayerContent::Group(_)));
            let to = if into_group {
                Placement::IntoGroup(id)
            } else {
                Placement::Above(id)
            };
            app.s.apply(Op::MoveLayer { id: source, to })
        }
        Command::LayerUp => move_active_layer(app, true),
        Command::LayerDown => move_active_layer(app, false),
        Command::SelectAll => app.s.apply(Op::SelectAll),
        Command::Deselect => app.s.apply(Op::Deselect),
        Command::Inverse => app.s.apply(Op::InvertSelection),
        Command::FeatherApply => {
            app.sheet = None;
            app.s.apply(Op::Feather(app.form.feather))
        }
        Command::BlurApply => {
            app.sheet = None;
            app.s.apply(Op::Filter(Filter::GaussianBlur {
                sigma: app.form.blur_sigma.max(0.1),
            }))
        }
        Command::SharpenApply => {
            app.sheet = None;
            app.s.apply(Op::Filter(Filter::Sharpen {
                amount: app.form.sharpen_amount,
                radius: app.form.sharpen_radius.max(0.1),
            }))
        }
        Command::ZoomIn => app.s.zoom_step(1, None),
        Command::ZoomOut => app.s.zoom_step(-1, None),
        Command::Fit => app.s.fit(),
        Command::ActualPixels => app.s.actual_pixels(),
        Command::Mode(dark) => {
            // Remembered in photo/settings.json, like the settings page's choice.
            let mode = if dark { ModePref::Dark } else { ModePref::Light };
            with_kit(app, |k| {
                k.settings.mode = mode;
                k.args.mode = None;
            });
            info.set_mode(OptionDarkLightMode::Some(if dark {
                DarkLightMode::Dark
            } else {
                DarkLightMode::Light
            }));
            kit::save_settings(&app.kit, info);
            return Update::DoNothing;
        }
        Command::Theme(name) => {
            let theme = Theme::parse(name).unwrap_or_default();
            with_kit(app, |k| {
                k.settings.theme = theme;
                k.args.theme = None;
            });
            app.theme = theme.name().to_string();
            info.set_theme(theme.name());
            kit::save_settings(&app.kit, info);
            return Update::DoNothing;
        }
        Command::Settings => {
            app.sheet = None;
            kit::open_settings(&app.kit, None);
            return Update::RefreshDom;
        }
        Command::Tool(t) => app.s.set_tool(t),
        Command::BrushSize(larger) => app.s.step_brush_size(larger),
        Command::Digit(d) => {
            let e = app.s.digit_opacity(d);
            if let Some(id) = app.s.engine.active_layer() {
                if let Some(l) = app.s.engine.document().layer(id) {
                    say(&format!("AZPHOTO_OPACITY {id} {}", (l.opacity * 100.0).round()));
                }
            }
            e
        }
        Command::SwapColors => app.s.swap_colors(),
        Command::DefaultColors => app.s.default_colors(),
        Command::Swatch(i) => {
            if let Some(c) = app.s.swatches.get(i).copied() {
                app.s.fg = c;
            }
            Effects {
                dom: true,
                ..Effects::default()
            }
        }
        Command::Sheet(sheet) => {
            let (w, h) = app.s.engine.size();
            match sheet {
                Sheet::ImageSize => {
                    app.form.size_width = w as f32;
                    app.form.size_height = h as f32;
                }
                Sheet::CanvasSize => {
                    app.form.canvas_width = w as f32;
                    app.form.canvas_height = h as f32;
                }
                _ => {}
            }
            if sheet != Sheet::NewImage && sheet != Sheet::About {
                app.screen = AppScreen::Editor;
            }
            app.sheet = Some(sheet);
            return Update::RefreshDom;
        }
        Command::CloseSheet => {
            app.sheet = None;
            return Update::RefreshDom;
        }
    };
    if e.dom {
        app.announce_layers();
        app.announce_history();
        if !app.s.status.is_empty() {
            say(&format!("AZPHOTO_STATUS {}", app.s.status));
        }
    }
    if app.s.engine.document().selection.is_some() {
        canvas::ensure_ants_timer(app, app_ref, info);
    }
    canvas::push_effects(app, info, e)
}

/// Move the active layer one place up or down in its list.
fn move_active_layer(app: &mut PhotoApp, up: bool) -> Effects {
    let Some(id) = app.s.engine.active_layer() else {
        return Effects::default();
    };
    let target = layer::parent_list(&app.s.engine.document().layers, id).and_then(|(list, i)| {
        if up {
            list.get(i + 1).map(|l| Placement::Above(l.id))
        } else if i > 0 {
            Some(Placement::Below(list[i - 1].id))
        } else {
            None
        }
    });
    match target {
        Some(to) => app.s.apply(Op::MoveLayer { id, to }),
        None => Effects::default(),
    }
}

/// A file name from the document's name.
#[must_use]
pub fn file_name(name: &str, extension: &str) -> String {
    let base: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' })
        .collect();
    let base = base.trim();
    format!("{}.{extension}", if base.is_empty() { "Untitled" } else { base })
}

/// Export the flattened image into the data tree, beside the document
/// (`photo/<uuid>/exports/<name>.png|jpg`, through the Drive on a job).
fn export(app: &mut PhotoApp, app_ref: &RefAny, info: &mut CallbackInfo) {
    let file = file_name(&app.s.name, app.export_format.extension());
    let (width, height, rgba) = app.s.engine.flatten_rgba();
    app.busy += 1;
    app.status(format!("Exporting {file}..."));
    jobs::spawn(info, app_ref, Job::Export {
        drive: app.drive.clone(),
        uuid: app.s.uuid.clone(),
        file,
        format: app.export_format,
        quality: app.jpeg_quality,
        width,
        height,
        rgba,
    });
}

fn picked_path(result: RefAny) -> Option<PathBuf> {
    let picked = FileOpenResult::downcast(result).into_option()?;
    let path = picked.path.into_option()?;
    Some(PathBuf::from(path.inner.as_str()))
}

fn open_picked(mut data: RefAny, info: &mut CallbackInfo, result: RefAny, as_layer: bool) -> Update {
    let Some(path) = picked_path(result) else {
        return Update::DoNothing;
    };
    let app_ref = data.clone();
    let Some(mut guard) = data.downcast_mut::<PhotoApp>() else {
        return Update::DoNothing;
    };
    guard.busy += 1;
    guard.status(format!("Opening {}...", path.display()));
    jobs::spawn(info, &app_ref, Job::OpenFile { path, as_layer });
    Update::RefreshDom
}

extern "C" fn on_open_picked(data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    open_picked(data, &mut info, result, false)
}

extern "C" fn on_place_picked(data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    open_picked(data, &mut info, result, true)
}

// ==== Values ====

/// A value of the options bar, a panel or a sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    BrushSize,
    Hardness,
    Opacity,
    Flow,
    PressureSize,
    PressureFlow,
    SelectMode,
    Feather,
    WandTolerance,
    WandContiguous,
    SampleMerged,
    BucketTolerance,
    BucketContiguous,
    ShapeKind,
    LayerOpacity,
    LayerBlend,
    FgColor,
    BgColor,
    NewWidth,
    NewHeight,
    NewTransparent,
    SizeWidth,
    SizeHeight,
    CanvasWidth,
    CanvasHeight,
    BlurSigma,
    SharpenAmount,
    SharpenRadius,
    RotateDegrees,
    FeatherForm,
    ExportFormat,
    JpegQuality,
    /// Parameter `n` of the active adjustment layer.
    Adjust(u8),
    ZoomPercent,
    TextFamily,
    TextSize,
    TextBold,
    TextItalic,
    /// The Text tool's field (its text comes through `on_text`).
    Text,
}

/// A value widget's payload: the app and the field.
pub struct FieldRef {
    pub app: RefAny,
    pub field: Field,
}

#[must_use]
pub fn field(app: &RefAny, field: Field) -> RefAny {
    RefAny::new(FieldRef {
        app: app.clone(),
        field,
    })
}

/// A value of any widget kind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    Number(f32),
    Index(usize),
    Bool(bool),
    Color([u8; 4]),
}

fn dispatch(mut data: RefAny, info: &mut CallbackInfo, value: Value) -> Update {
    let Some((app_ref, f)) = data.downcast_ref::<FieldRef>().map(|r| (r.app.clone(), r.field)) else {
        return Update::DoNothing;
    };
    let mut target = app_ref.clone();
    let Some(mut guard) = target.downcast_mut::<PhotoApp>() else {
        return Update::DoNothing;
    };
    set_field(&mut guard, &app_ref, info, f, value)
}

pub extern "C" fn on_number(data: RefAny, mut info: CallbackInfo, state: NumberInputState) -> Update {
    dispatch(data, &mut info, Value::Number(state.number))
}

pub extern "C" fn on_slider(data: RefAny, mut info: CallbackInfo, state: SliderState) -> Update {
    dispatch(data, &mut info, Value::Number(state.value))
}

pub extern "C" fn on_choice(data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    dispatch(data, &mut info, Value::Index(index))
}

pub extern "C" fn on_check(data: RefAny, mut info: CallbackInfo, state: CheckBoxState) -> Update {
    dispatch(data, &mut info, Value::Bool(state.checked))
}

pub extern "C" fn on_color(data: RefAny, mut info: CallbackInfo, state: ColorInputState) -> Update {
    let c = state.color;
    dispatch(data, &mut info, Value::Color([c.r, c.g, c.b, c.a]))
}

pub extern "C" fn on_segment(data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    dispatch(data, &mut info, Value::Index(state.selected_index))
}

/// The Text tool's field: the text being set follows every keystroke on
/// the canvas (the field keeps its own text - no DOM rebuild).
pub extern "C" fn on_text(mut data: RefAny, mut info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let answer = |update| OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    };
    let Some(app_ref) = data.downcast_ref::<FieldRef>().map(|r| r.app.clone()) else {
        return answer(Update::DoNothing);
    };
    let mut target = app_ref.clone();
    let Some(mut guard) = target.downcast_mut::<PhotoApp>() else {
        return answer(Update::DoNothing);
    };
    let text = state.get_text().as_str().to_string();
    let mut e = guard.s.set_text(&text, &|spec: &TextSpec| azul_text(&info, spec));
    e.dom = false;
    answer(canvas::push_effects(&mut guard, &mut info, e))
}

/// The parameters of an adjustment as (label, value, min, max).
#[must_use]
pub fn adjustment_params(a: &Adjustment) -> Vec<(&'static str, f32, f32, f32)> {
    match a {
        Adjustment::BrightnessContrast {
            brightness,
            contrast,
        } => vec![
            ("Brightness", *brightness * 100.0, -100.0, 100.0),
            ("Contrast", *contrast * 100.0, -100.0, 100.0),
        ],
        Adjustment::Levels {
            in_black,
            in_white,
            gamma,
            out_black,
            out_white,
        } => vec![
            ("Input black", f32::from(*in_black), 0.0, 254.0),
            ("Input white", f32::from(*in_white), 1.0, 255.0),
            ("Gamma", *gamma, 0.1, 9.99),
            ("Output black", f32::from(*out_black), 0.0, 255.0),
            ("Output white", f32::from(*out_white), 0.0, 255.0),
        ],
        Adjustment::Curves { points } => points
            .iter()
            .skip(1)
            .take(points.len().saturating_sub(2))
            .map(|(_, y)| ("Point", f32::from(*y), 0.0, 255.0))
            .collect(),
        Adjustment::HueSaturation {
            hue,
            saturation,
            lightness,
        } => vec![
            ("Hue", *hue, -180.0, 180.0),
            ("Saturation", *saturation * 100.0, -100.0, 100.0),
            ("Lightness", *lightness * 100.0, -100.0, 100.0),
        ],
        Adjustment::Invert => Vec::new(),
        Adjustment::Threshold { level } => vec![("Level", f32::from(*level), 0.0, 255.0)],
    }
}

/// `a` with parameter `n` set to `v` (the units of [`adjustment_params`]).
#[must_use]
pub fn with_param(a: &Adjustment, n: u8, v: f32) -> Adjustment {
    let byte = |v: f32| v.round().clamp(0.0, 255.0) as u8;
    let mut a = a.clone();
    match &mut a {
        Adjustment::BrightnessContrast {
            brightness,
            contrast,
        } => match n {
            0 => *brightness = (v / 100.0).clamp(-1.0, 1.0),
            _ => *contrast = (v / 100.0).clamp(-1.0, 1.0),
        },
        Adjustment::Levels {
            in_black,
            in_white,
            gamma,
            out_black,
            out_white,
        } => match n {
            0 => *in_black = byte(v),
            1 => *in_white = byte(v),
            2 => *gamma = v.clamp(0.1, 9.99),
            3 => *out_black = byte(v),
            _ => *out_white = byte(v),
        },
        Adjustment::Curves { points } => {
            if let Some(p) = points.get_mut(n as usize + 1) {
                p.1 = byte(v);
            }
        }
        Adjustment::HueSaturation {
            hue,
            saturation,
            lightness,
        } => match n {
            0 => *hue = v.clamp(-180.0, 180.0),
            1 => *saturation = (v / 100.0).clamp(-1.0, 1.0),
            _ => *lightness = (v / 100.0).clamp(-1.0, 1.0),
        },
        Adjustment::Invert => {}
        Adjustment::Threshold { level } => *level = byte(v),
    }
    a
}

/// Set one value. A dragged slider repaints the canvas but does not rebuild
/// the DOM (the widget keeps its drag); the panels catch up on the next
/// rebuild.
#[allow(clippy::too_many_lines)]
fn set_field(app: &mut PhotoApp, app_ref: &RefAny, info: &mut CallbackInfo, f: Field, value: Value) -> Update {
    let num = match value {
        Value::Number(n) => n,
        Value::Index(i) => i as f32,
        Value::Bool(b) => f32::from(u8::from(b)),
        Value::Color(_) => 0.0,
    };
    let flag = matches!(value, Value::Bool(true));
    let index = match value {
        Value::Index(i) => i,
        _ => 0,
    };
    let o = &mut app.s.opts;
    let mut rebuild = false;
    // The text being set follows its font, size, style and colour.
    let mut restyle = false;
    let mut e = Effects::default();
    match f {
        Field::BrushSize => app.s.set_brush_size(num),
        Field::Hardness => o.hardness = (num / 100.0).clamp(0.0, 1.0),
        Field::Opacity => o.opacity = (num / 100.0).clamp(0.0, 1.0),
        Field::Flow => o.flow = (num / 100.0).clamp(0.01, 1.0),
        Field::PressureSize => o.pressure_size = flag,
        Field::PressureFlow => o.pressure_flow = flag,
        Field::SelectMode => {
            o.select_mode = SelectMode::ALL.get(index).copied().unwrap_or(SelectMode::Replace);
            rebuild = true;
        }
        Field::Feather => o.feather = num.clamp(0.0, 250.0),
        Field::WandTolerance => o.wand_tolerance = num.round().clamp(0.0, 255.0) as u8,
        Field::WandContiguous => o.wand_contiguous = flag,
        Field::SampleMerged => o.sample_merged = flag,
        Field::BucketTolerance => o.bucket_tolerance = num.round().clamp(0.0, 255.0) as u8,
        Field::BucketContiguous => o.bucket_contiguous = flag,
        Field::ShapeKind => {
            o.shape_ellipse = index == 1;
            rebuild = true;
        }
        Field::LayerOpacity => {
            if let Some(id) = app.s.engine.active_layer() {
                let pct = num.clamp(0.0, 100.0);
                e = app.s.apply(Op::SetOpacity(id, pct / 100.0));
                e.dom = false;
                say(&format!("AZPHOTO_OPACITY {id} {}", pct.round()));
            }
        }
        Field::LayerBlend => {
            if let Some(id) = app.s.engine.active_layer() {
                let mode = BlendMode::ALL.get(index).copied().unwrap_or(BlendMode::Normal);
                e = app.s.apply(Op::SetBlend(id, mode));
            }
        }
        Field::FgColor => {
            if let Value::Color(c) = value {
                app.s.fg = c;
                rebuild = true;
                restyle = true;
            }
        }
        Field::BgColor => {
            if let Value::Color(c) = value {
                app.s.bg = c;
                rebuild = true;
            }
        }
        Field::NewWidth => app.form.new_width = num,
        Field::NewHeight => app.form.new_height = num,
        Field::NewTransparent => app.form.new_transparent = flag,
        Field::SizeWidth => app.form.size_width = num,
        Field::SizeHeight => app.form.size_height = num,
        Field::CanvasWidth => app.form.canvas_width = num,
        Field::CanvasHeight => app.form.canvas_height = num,
        Field::BlurSigma => app.form.blur_sigma = num,
        Field::SharpenAmount => app.form.sharpen_amount = num,
        Field::SharpenRadius => app.form.sharpen_radius = num,
        Field::RotateDegrees => app.form.rotate_degrees = num,
        Field::FeatherForm => app.form.feather = num,
        Field::ExportFormat => {
            app.export_format = if index == 1 { ExportFormat::Jpeg } else { ExportFormat::Png };
            rebuild = true;
        }
        Field::JpegQuality => app.jpeg_quality = num.round().clamp(1.0, 100.0) as u8,
        Field::Adjust(n) => {
            let active = app.s.engine.active_layer();
            let current = active
                .and_then(|id| app.s.engine.document().layer(id))
                .and_then(|l| match &l.content {
                    LayerContent::Adjustment(a) => Some(a.clone()),
                    _ => None,
                });
            if let (Some(id), Some(a)) = (active, current) {
                e = app.s.apply(Op::SetAdjustment(id, with_param(&a, n, num)));
                e.dom = false;
            }
        }
        Field::ZoomPercent => {
            e = app.s.zoom_to(num / 100.0, None);
            e.dom = false;
        }
        Field::TextFamily => {
            o.text_family = index.min(TEXT_FAMILIES.len() - 1);
            restyle = true;
        }
        Field::TextSize => {
            o.text_size = num.clamp(4.0, 1000.0);
            restyle = true;
        }
        Field::TextBold => {
            o.text_bold = flag;
            restyle = true;
        }
        Field::TextItalic => {
            o.text_italic = flag;
            restyle = true;
        }
        Field::Text => {}
    }
    if restyle {
        e = e.merge(app.s.restyle_text(&|spec: &TextSpec| azul_text(&*info, spec)));
    }
    if rebuild {
        e.dom = true;
    }
    let _ = app_ref;
    canvas::push_effects(app, info, e)
}

/// The rect a selection covers, for the info line.
#[must_use]
pub fn selection_label(app: &PhotoApp) -> Option<String> {
    let r: IRect = app.s.engine.document().selection.as_ref()?.bounds()?;
    Some(format!("Selection {} x {} at {}, {}", r.w, r.h, r.x, r.y))
}
