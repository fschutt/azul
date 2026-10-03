//! AzPhoto: the photo editor of the azul apps (Photoshop, Affinity Photo,
//! GIMP), on the S2 `CanvasShell`.
//!
//! The pixels live in AzPhoto's own raster core ([`raster`]) behind the
//! [`raster::RasterEngine`] trait: layers of 256 x 256 RGBA8 tiles with
//! dirty-tile tracking, blend modes and opacity composited on worker threads,
//! a brush engine with pen pressure, selection masks, non-destructive
//! adjustment layers, filters, transforms, crop and undo by tile snapshots.
//! Graphite can replace or extend it behind the same trait later.
//!
//! The canvas is ONE image node: a `RenderImageCallback` draws the viewport
//! (the visible part of the document at the current zoom) at the node's
//! physical size; a brush stroke redraws only the view rect it touched and
//! hands exactly that rect to the renderer
//! (`CallbackInfo::change_node_image_rect`), which uploads only those pixels.
//!
//! Documents are files - `photo/<uuid>/doc.json` and the layer tiles as PNG
//! in `photo/<uuid>/layers/` - written through `azul_storage::Drive` on a
//! `LocalDrive` in the user's data folder (an `S3Drive` later), from an azul
//! `Thread`. Exports are PNG or JPEG (with a quality).
//!
//! Environment: `AZPHOTO_DATA` (the data folder), `AZPHOTO_EXPORT_DIR`
//! (export there without a dialog: scripts).
//!
//! On stdout, for scripts (`scripts/azphoto_e2e.py`): `AZPHOTO_DOC <w>x<h>
//! <layers> <name>`, `AZPHOTO_LAYERS <n> <active>`, `AZPHOTO_HISTORY <n>
//! <current> <label>`, `AZPHOTO_VIEW <w>x<h>`, `AZPHOTO_UPDATE <x> <y> <w>
//! <h>` (a partial canvas update), `AZPHOTO_OPACITY <layer> <percent>`,
//! `AZPHOTO_SAVED <uuid> <tiles>`, `AZPHOTO_EXPORTED <bytes> <path>`,
//! `AZPHOTO_STATUS <text>`.

pub mod args;
pub mod canvas;
pub mod codec;
pub mod commands;
pub mod jobs;
pub mod raster;
pub mod state;
pub mod storage;
pub mod ui;
pub mod view;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use azul::{
    css::DarkLightMode,
    file::FilePath,
    image::ImageRef,
    option::OptionDarkLightMode,
    prelude::*,
    str::String as AzString,
    window::WindowDecorations,
};
use azul_storage::{Drive, LocalDrive};

use crate::{
    args::{Args, Screen},
    jobs::{Done, ExportFormat, Job, Outcome},
    raster::{Adjustment, BlendMode, Document, LayerId, Op, RasterEngine, TileEngine},
    state::PhotoState,
    storage::DocEntry,
};

/// The sample photo (examples/assets): a 1920 x 1080 JPEG.
const SAMPLE_JPEG: &[u8] = include_bytes!("../../assets/images/cat_image.jpg");

/// Which page the window shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppScreen {
    /// Open / New / the sample / recent documents.
    Start,
    Editor,
}

/// A sheet over the editor (an in-window dialog).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sheet {
    NewImage,
    Export,
    ImageSize,
    CanvasSize,
    GaussianBlur,
    Sharpen,
    Rotate,
    Feather,
    About,
    Settings,
}

/// The values the sheets edit.
#[derive(Clone, Debug, PartialEq)]
pub struct Form {
    pub new_width: f32,
    pub new_height: f32,
    pub new_transparent: bool,
    pub size_width: f32,
    pub size_height: f32,
    pub canvas_width: f32,
    pub canvas_height: f32,
    pub blur_sigma: f32,
    pub sharpen_amount: f32,
    pub sharpen_radius: f32,
    pub rotate_degrees: f32,
    pub feather: f32,
}

impl Default for Form {
    fn default() -> Self {
        Self {
            new_width: 1920.0,
            new_height: 1080.0,
            new_transparent: false,
            size_width: 0.0,
            size_height: 0.0,
            canvas_width: 0.0,
            canvas_height: 0.0,
            blur_sigma: 4.0,
            sharpen_amount: 0.8,
            sharpen_radius: 1.5,
            rotate_degrees: 15.0,
            feather: 8.0,
        }
    }
}

/// The app: the editor state plus azul's objects and the window's pages.
pub struct PhotoApp {
    pub s: PhotoState,
    pub screen: AppScreen,
    pub sheet: Option<Sheet>,
    pub drive: Arc<dyn Drive>,
    pub data_root: PathBuf,
    /// Export straight into this folder (no dialog).
    pub export_dir: Option<PathBuf>,
    pub recent: Vec<DocEntry>,
    /// Jobs running.
    pub busy: usize,
    /// The canvas node's current image (what the render callback answers).
    pub canvas_image: Option<ImageRef>,
    /// The canvas's physical pixels per logical pixel.
    pub hidpi: f32,
    /// A layer row pressed in the Layers panel (drag to reorder).
    pub layer_drag: Option<LayerId>,
    pub export_format: ExportFormat,
    pub jpeg_quality: u8,
    pub form: Form,
    /// The app theme ("flat" / "flora") and the mode, as last seen.
    pub theme: String,
    pub dark: bool,
    /// The marching-ants timer runs.
    pub ants_timer: bool,
    /// The last pinch scale (pinches report it cumulatively).
    pub last_pinch: Option<f32>,
}

/// Print one line for scripts.
pub fn say(line: &str) {
    println!("{line}");
}

/// A fresh document folder name: random, so no launch reuses another's
/// (`Uuid::v4` is a process-local marker sequence, the same in every run).
#[must_use]
pub fn new_uuid() -> String {
    azul::uuid::Uuid::from_seed(azul_storage::ids::random_seed()).as_str().to_string()
}

/// The sample: the photo, a warm light leak (Screen, 60 %) and a Curves
/// adjustment.
pub fn sample_document() -> Result<Document, String> {
    let (w, h, rgba) = codec::decode(SAMPLE_JPEG)?;
    let mut e = TileEngine::new(Document::from_rgba(w, h, rgba, "Photo"));
    let err = |e: raster::EngineError| e.to_string();
    e.apply(Op::NewLayer {
        name: "Light leak".into(),
    })
    .map_err(err)?;
    let leak = e.active_layer().ok_or("no layer")?;
    e.apply(Op::Gradient {
        from: (0.0, 0.0),
        to: (w as f32 * 0.6, h as f32 * 0.6),
        start: [255, 170, 60, 255],
        end: [255, 170, 60, 0],
    })
    .map_err(err)?;
    e.apply(Op::SetBlend(leak, BlendMode::Screen)).map_err(err)?;
    e.apply(Op::SetOpacity(leak, 0.6)).map_err(err)?;
    e.apply(Op::NewAdjustment(Adjustment::Curves {
        points: vec![(0, 0), (64, 52), (192, 206), (255, 255)],
    }))
    .map_err(err)?;
    Ok(e.document().clone())
}

impl PhotoApp {
    /// Show `doc` in the editor.
    pub fn open_document(&mut self, doc: Document, name: &str, uuid: &str, label: &str) {
        let _ = self.s.replace_document(doc, name, uuid, label);
        self.canvas_image = None;
        self.screen = AppScreen::Editor;
        self.sheet = None;
        self.announce();
    }

    /// The document lines for scripts.
    pub fn announce(&self) {
        let (w, h) = self.s.engine.size();
        let doc = self.s.engine.document();
        say(&format!(
            "AZPHOTO_DOC {w}x{h} {} {}",
            raster::layer::all_ids(&doc.layers).len(),
            self.s.name
        ));
        self.announce_layers();
        self.announce_history();
    }

    pub fn announce_layers(&self) {
        let doc = self.s.engine.document();
        let active = self
            .s
            .engine
            .active_layer()
            .and_then(|id| doc.layer(id))
            .map_or_else(String::new, |l| l.name.clone());
        say(&format!(
            "AZPHOTO_LAYERS {} {active}",
            raster::layer::all_ids(&doc.layers).len()
        ));
    }

    pub fn announce_history(&self) {
        let (labels, current) = self.s.engine.history();
        say(&format!(
            "AZPHOTO_HISTORY {} {current} {}",
            labels.len(),
            labels.get(current).map_or("", String::as_str)
        ));
    }

    /// Set the status line (and print it for scripts).
    pub fn status(&mut self, text: impl Into<String>) {
        self.s.status = text.into();
        say(&format!("AZPHOTO_STATUS {}", self.s.status));
    }
}

/// A thread's answer.
extern "C" fn on_job_done(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let handle = app.clone();
    let Some(outcome) = msg
        .downcast_mut::<Done>()
        .and_then(|mut done| done.outcome.take())
    else {
        return Update::DoNothing;
    };
    let Some(mut guard) = app.downcast_mut::<PhotoApp>() else {
        return Update::DoNothing;
    };
    let a = &mut *guard;
    a.busy = a.busy.saturating_sub(1);
    match outcome {
        Outcome::Opened {
            name,
            as_layer,
            result,
        } => match result {
            Ok((w, h, rgba)) if as_layer => {
                let (dw, dh) = a.s.engine.size();
                let x = (dw as i32 - w as i32) / 2;
                let y = (dh as i32 - h as i32) / 2;
                let e = a.s.apply(Op::AddRasterLayer {
                    name,
                    width: w,
                    height: h,
                    rgba,
                    x,
                    y,
                });
                a.announce_layers();
                return canvas::push_effects(a, &mut info, e);
            }
            Ok((w, h, rgba)) => {
                let doc = Document::from_rgba(w, h, rgba, "Background");
                a.open_document(doc, &name, &new_uuid(), "Open");
            }
            Err(e) => a.status(e),
        },
        Outcome::Saved { uuid, result } => match result {
            Ok(saved) => {
                a.s.modified = false;
                a.status(format!(
                    "Saved {} ({} tiles, {}).",
                    a.s.name,
                    saved.tiles,
                    azul::file::DiskSpace::format_bytes(saved.bytes)
                ));
                say(&format!("AZPHOTO_SAVED {uuid} {}", saved.tiles));
                a.busy += 1;
                jobs::spawn(&mut info, &handle, Job::List {
                    drive: a.drive.clone(),
                });
            }
            Err(e) => a.status(format!("Saving failed: {e}")),
        },
        Outcome::Loaded { uuid, result } => match result {
            Ok((name, doc)) => a.open_document(doc, &name, &uuid, "Open"),
            Err(e) => a.status(format!("The document could not be opened: {e}")),
        },
        Outcome::Listed(result) => match result {
            Ok(list) => a.recent = list,
            Err(e) => a.status(format!("The documents could not be listed: {e}")),
        },
        Outcome::Exported { key, result } => match result {
            Ok(bytes) => {
                a.status(format!("Exported {key} ({}).", azul::file::DiskSpace::format_bytes(bytes)));
                say(&format!("AZPHOTO_EXPORTED {bytes} {key}"));
            }
            Err(e) => a.status(format!("Export failed: {e}")),
        },
    }
    Update::RefreshDom
}

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var(var)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn user_data_dir() -> Option<PathBuf> {
    FilePath::get_data_dir()
        .into_option()
        .map(|dir| PathBuf::from(dir.inner.as_str()))
}

/// Read and decode an image file now (startup, before the window).
fn open_now(path: &Path) -> Result<(String, Document), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let (w, h, rgba) = codec::decode(&bytes)?;
    let name = path
        .file_stem()
        .map_or_else(|| "Untitled".to_string(), |s| s.to_string_lossy().into_owned());
    Ok((name, Document::from_rgba(w, h, rgba, "Background")))
}

pub fn start() {
    let args = match Args::parse(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(msg) if msg.contains("USAGE") => {
            println!("{msg}");
            return;
        }
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(2);
        }
    };
    let data_root = args
        .data
        .clone()
        .or_else(|| env_path("AZPHOTO_DATA"))
        .unwrap_or_else(|| user_data_dir().unwrap_or_else(|| PathBuf::from(".")).join("Azul"));
    let drive: Arc<dyn Drive> = Arc::new(LocalDrive::new(&data_root));
    let export_dir = args.export_dir.clone().or_else(|| env_path("AZPHOTO_EXPORT_DIR"));
    let recent = storage::list(drive.as_ref()).unwrap_or_default();

    let mut status = String::new();
    let opened: Option<(String, Document)> = if args.sample {
        match sample_document() {
            Ok(doc) => Some(("Sample".to_string(), doc)),
            Err(e) => {
                status = format!("The sample could not be made: {e}");
                None
            }
        }
    } else if let Some(path) = &args.open {
        match open_now(path) {
            Ok(opened) => Some(opened),
            Err(e) => {
                status = e;
                None
            }
        }
    } else {
        None
    };
    let has_doc = opened.is_some();
    let (name, doc) = opened.unwrap_or_else(|| {
        (
            "Untitled".to_string(),
            Document::with_background(1920, 1080, [255, 255, 255, 255]),
        )
    });
    let mut s = PhotoState::new(doc, &name, &new_uuid());
    s.status = status;
    let screen = match args.screen {
        Screen::Start => AppScreen::Start,
        Screen::Auto if !has_doc => AppScreen::Start,
        _ => AppScreen::Editor,
    };
    let sheet = match args.screen {
        Screen::Export => Some(Sheet::Export),
        Screen::NewImage => Some(Sheet::NewImage),
        Screen::Settings => Some(Sheet::Settings),
        Screen::About => Some(Sheet::About),
        _ => None,
    };
    let dark = args.dark.unwrap_or(false);
    let app = PhotoApp {
        s,
        screen,
        sheet,
        drive,
        data_root: data_root.clone(),
        export_dir,
        recent,
        busy: 0,
        canvas_image: None,
        hidpi: 1.0,
        layer_drag: None,
        export_format: ExportFormat::Png,
        jpeg_quality: 85,
        form: Form::default(),
        theme: args.theme.clone().unwrap_or_else(|| "flat".to_string()),
        dark,
        ants_timer: false,
        last_pinch: None,
    };
    eprintln!("[azphoto] data folder {}", data_root.display());
    app.announce();

    let mut config = AppConfig::create();
    if let Some(theme) = &args.theme {
        config.set_theme(theme.as_str());
    }
    if let Some(dark) = args.dark {
        config.set_mode(OptionDarkLightMode::Some(if dark {
            DarkLightMode::Dark
        } else {
            DarkLightMode::Light
        }));
    }
    let (w, h) = args.size.unwrap_or((1440.0, 900.0));
    let app = App::create(RefAny::new(app), config);
    let mut window = WindowCreateOptions::create(ui::layout);
    window.window_state.size.dimensions = LogicalSize::create(w, h);
    window.window_state.title = AzString::from("AzPhoto");
    window.window_state.flags.decorations = WindowDecorations::NoTitle;
    app.run(window);
}

#[cfg(test)]
mod ids_tests;
