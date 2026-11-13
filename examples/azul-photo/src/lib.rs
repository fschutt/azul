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
//! On azul-appkit: the switches (`--data-dir`, `--theme`, `--mode`,
//! `--screen`, `--sample`, `--shot`, a bare image path), the data root
//! (`$AZLIN_DATA` or the user's), the settings page (theme and mode
//! remembered in `photo/settings.json`), the shortcuts table, About.
//!
//! On stdout, for scripts (`scripts/azphoto_e2e.py`): `AZPHOTO_DOC <w>x<h>
//! <layers> <name>`, `AZPHOTO_LAYERS <n> <active>`, `AZPHOTO_HISTORY <n>
//! <current> <label>`, `AZPHOTO_VIEW <w>x<h>`, `AZPHOTO_UPDATE <x> <y> <w>
//! <h>` (a partial canvas update), `AZPHOTO_OPACITY <layer> <percent>`,
//! `AZPHOTO_SAVED <uuid> <tiles>`, `AZPHOTO_EXPORTED <bytes> <key>`,
//! `AZPHOTO_STATUS <text>`.

pub mod args;
pub mod canvas;
pub mod codec;
pub mod commands;
pub mod ids;
pub mod jobs;
mod l10n;
#[cfg(test)]
mod l10n_tests;
pub mod raster;
pub mod state;
pub mod storage;
pub mod ui;
pub mod view;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use azul::{image::ImageRef, prelude::*};
use azul_appkit::{
    about::AboutInfo,
    l10n::{t, t_args, Arg},
    shortcuts::Shortcut,
    ui as kit,
};
use azul_storage::{Drive, LocalDrive};

use crate::{
    args::{Args, Screen, SPEC},
    jobs::{Done, ExportFormat, Job, Outcome},
    raster::{Adjustment, BlendMode, Document, LayerId, Op, RasterEngine, TileEngine},
    state::PhotoState,
    storage::DocEntry,
};

/// What the About dialog and the settings page say about AzPhoto.
pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzPhoto",
    version: env!("CARGO_PKG_VERSION"),
    summary: "A photo editor: layers with blend modes, brushes with pen pressure, selections, \
              adjustment layers, filters, transforms and text. Part of the Azlin apps, built with azul.",
    license: "MIT",
    app_folder: "photo",
};

/// The keyboard shortcuts the settings page lists (`Mod` = Cmd / Ctrl).
pub const SHORTCUTS: [Shortcut; 22] = [
    Shortcut::new("azphoto-tools", "V", "azphoto-tool-move"),
    Shortcut::new("azphoto-tools", "M", "azphoto-shortcut-marquee"),
    Shortcut::new("azphoto-tools", "L", "azphoto-tool-lasso"),
    Shortcut::new("azphoto-tools", "W", "azphoto-tool-magic-wand"),
    Shortcut::new("azphoto-tools", "C", "azphoto-tool-crop"),
    Shortcut::new("azphoto-tools", "I", "azphoto-tool-eyedropper"),
    Shortcut::new("azphoto-tools", "B", "azphoto-shortcut-brush"),
    Shortcut::new("azphoto-tools", "E", "azphoto-tool-eraser"),
    Shortcut::new("azphoto-tools", "G", "azphoto-shortcut-bucket"),
    Shortcut::new("azphoto-tools", "S", "azphoto-tool-clone-stamp"),
    Shortcut::new("azphoto-tools", "T", "azphoto-shortcut-text"),
    Shortcut::new("azphoto-tools", "U  H  Z", "azphoto-shortcut-shape-hand-zoom"),
    Shortcut::new("azphoto-shortcuts-painting", "[  ]", "azphoto-shortcut-brush-size"),
    Shortcut::new("azphoto-shortcuts-painting", "X  D", "azphoto-shortcut-colours"),
    Shortcut::new("azphoto-shortcuts-painting", "0 .. 9", "azphoto-shortcut-opacity"),
    Shortcut::new("azphoto-menu-edit", "Mod+Z  Mod+Shift+Z", "azphoto-shortcut-undo"),
    Shortcut::new("azphoto-menu-edit", "Mod+A  Mod+D  Mod+Shift+I", "azphoto-shortcut-select"),
    Shortcut::new("azphoto-menu-file", "Mod+N  Mod+O  Mod+S", "azphoto-shortcut-file"),
    Shortcut::new("azphoto-menu-file", "Mod+Shift+E", "azphoto-shortcut-export"),
    Shortcut::new("azphoto-menu-layer", "Mod+Shift+N  Mod+J  Mod+E", "azphoto-shortcut-layer"),
    Shortcut::new("azphoto-menu-view", "Mod+0  Mod+1", "azphoto-shortcut-fit"),
    Shortcut::new("azphoto-menu-view", "Mod+=  Mod+-", "azphoto-shortcut-zoom"),
];

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
    /// azul-appkit's kit: settings (remembered), the settings page, About.
    pub kit: RefAny,
    /// "Save changes?" is showing (the close guard asks).
    pub closing: bool,
    /// The window closes once the save in progress is written.
    pub close_after_save: bool,
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
                let doc = Document::from_rgba(w, h, rgba, &t("azphoto-background"));
                a.open_document(doc, &name, &new_uuid(), &t("azphoto-history-open"));
            }
            Err(e) => a.status(e),
        },
        Outcome::Saved { uuid, result } => match result {
            Ok(saved) if a.close_after_save => {
                // "Save" in the close guard's question: written, now close.
                a.close_after_save = false;
                a.s.modified = false;
                say(&format!("AZPHOTO_SAVED {uuid} {}", saved.tiles));
                info.close_window();
            }
            Ok(saved) => {
                a.s.modified = false;
                a.status(t_args(
                    "azphoto-saved",
                    &[
                        ("name", Arg::from(&a.s.name)),
                        ("tiles", Arg::from(saved.tiles)),
                        ("size", Arg::from(azul::file::DiskSpace::format_bytes(saved.bytes).to_string())),
                    ],
                ));
                say(&format!("AZPHOTO_SAVED {uuid} {}", saved.tiles));
                a.busy += 1;
                jobs::spawn(&mut info, &handle, Job::List {
                    drive: a.drive.clone(),
                });
            }
            Err(e) => {
                a.close_after_save = false;
                a.status(t_args("azphoto-save-failed", &[("why", Arg::from(e.to_string()))]));
            }
        },
        Outcome::Loaded { uuid, result } => match result {
            Ok((name, doc)) => a.open_document(doc, &name, &uuid, &t("azphoto-history-open")),
            Err(e) => a.status(t_args("azphoto-open-failed", &[("why", Arg::from(e.to_string()))])),
        },
        Outcome::Listed(result) => match result {
            Ok(list) => a.recent = list,
            Err(e) => a.status(t_args("azphoto-list-failed", &[("why", Arg::from(e.to_string()))])),
        },
        Outcome::Exported { key, result } => match result {
            Ok(bytes) => {
                a.status(t_args(
                    "azphoto-exported",
                    &[("path", Arg::from(&key)), ("size", Arg::from(azul::file::DiskSpace::format_bytes(bytes).to_string()))],
                ));
                say(&format!("AZPHOTO_EXPORTED {bytes} {key}"));
            }
            Err(e) => a.status(t_args("azphoto-export-failed", &[("why", Arg::from(e.to_string()))])),
        },
    }
    Update::RefreshDom
}

/// Read and decode an image file now (startup, before the window).
fn open_now(path: &Path) -> Result<(String, Document), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let (w, h, rgba) = codec::decode(&bytes)?;
    let name = path
        .file_stem()
        .map_or_else(|| t("azphoto-untitled"), |s| s.to_string_lossy().into_owned());
    Ok((name, Document::from_rgba(w, h, rgba, &t("azphoto-background"))))
}

/// The window is up: the `--shot` timer, then the saved documents for the
/// start screen (read on a job, never in a callback).
extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let handle = data.clone();
    let Some(mut guard) = data.downcast_mut::<PhotoApp>() else {
        return Update::DoNothing;
    };
    let a = &mut *guard;
    kit::on_window_created(&a.kit, &mut info);
    a.busy += 1;
    jobs::spawn(&mut info, &handle, Job::List {
        drive: a.drive.clone(),
    });
    Update::DoNothing
}

/// The page the window opens on for `--screen` (`screen`), with or without
/// an image to open (`has_doc`). Like Photoshop with a new file, a launch
/// without an image opens the editor on an empty untitled canvas; the start
/// page (open, new, the saved documents) only when asked for or after the
/// last document closed.
#[must_use]
pub fn first_screen(screen: Screen, has_doc: bool) -> AppScreen {
    let _ = has_doc;
    match screen {
        Screen::Start => AppScreen::Start,
        _ => AppScreen::Editor,
    }
}

pub fn start() {
    let (app_args, args) = match Args::parse(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(msg) => {
            println!("{msg}");
            std::process::exit(if msg.contains("USAGE") { 0 } else { 2 });
        }
    };
    // The kit: the data root (--data-dir, $AZLIN_DATA, the user's), the
    // settings file (app theme and mode, remembered), the shortcuts, About.
    let kit_ref = kit::create_kit(SPEC, ABOUT, &SHORTCUTS, &[], app_args.clone());
    // The words first: everything from here is said in the language chosen (Settings,
    // `--language`; the system's otherwise) - a new image's names too.
    let mut config = kit::app_config(&kit_ref);
    crate::l10n::register(&mut config);
    let mut kit_now = kit_ref.clone();
    let language = kit_now.downcast_ref::<kit::Kit>().map(|k| k.language());
    if let Some(language) = language.filter(|l| *l != azul_appkit::args::LanguagePref::System) {
        azul_appkit::l10n::set_locale(language.tag());
    }
    let data_root = {
        let mut k = kit_ref.clone();
        let root = k.downcast_ref::<kit::Kit>().map(|k| k.data_root.clone());
        root.unwrap_or_else(|| PathBuf::from("."))
    };
    if args.screen == Screen::Settings {
        kit::open_settings(&kit_ref, None);
    }
    let drive: Arc<dyn Drive> = Arc::new(LocalDrive::new(&data_root));

    let mut status = String::new();
    let opened: Option<(String, Document)> = if args.sample {
        match sample_document() {
            Ok(doc) => Some(("Sample".to_string(), doc)),
            Err(e) => {
                status = t_args("azphoto-sample-failed", &[("why", Arg::from(e.to_string()))]);
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
            t("azphoto-untitled"),
            Document::with_background(1920, 1080, [255, 255, 255, 255]),
        )
    });
    let mut s = PhotoState::new(doc, &name, &new_uuid());
    s.status = status;
    let screen = first_screen(args.screen, has_doc);
    let sheet = match args.screen {
        Screen::Export => Some(Sheet::Export),
        Screen::NewImage => Some(Sheet::NewImage),
        Screen::About => Some(Sheet::About),
        _ => None,
    };
    let app = PhotoApp {
        s,
        screen,
        sheet,
        drive,
        data_root: data_root.clone(),
        recent: Vec::new(),
        busy: 0,
        canvas_image: None,
        hidpi: 1.0,
        layer_drag: None,
        export_format: ExportFormat::Png,
        jpeg_quality: 85,
        form: Form::default(),
        theme: String::from("flat"),
        dark: false,
        ants_timer: false,
        last_pinch: None,
        kit: kit_ref.clone(),
        closing: false,
        close_after_save: false,
    };
    eprintln!("[azphoto] data folder {}", data_root.display());
    app.announce();

    let window = kit::window_options(&kit_ref, ui::layout, (1440.0, 900.0), (900.0, 600.0), on_window_created);
    App::create(RefAny::new(app), config).run(window);
}

#[cfg(test)]
mod ids_tests;

#[cfg(test)]
mod start_tests {
    use super::*;

    #[test]
    fn without_an_image_azphoto_opens_the_editor_on_an_empty_untitled_canvas() {
        assert_eq!(first_screen(Screen::Auto, false), AppScreen::Editor);
        assert_eq!(first_screen(Screen::Settings, false), AppScreen::Editor);
        assert_eq!(first_screen(Screen::Auto, true), AppScreen::Editor);
        assert_eq!(first_screen(Screen::Start, false), AppScreen::Start, "the start page on request");
    }

    #[test]
    fn the_menu_shows_once_natively_on_macos_and_in_the_window_elsewhere() {
        assert_eq!(ui::in_window_menu(), !cfg!(target_os = "macos"));
    }
}
