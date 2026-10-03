//! AzPdf - a PDF viewer on the public azul API (wave 9, PDF9).
//!
//! azul parses the PDF once (`ParsedPdf`, printpdf underneath) and hands
//! each page over as SVG; azul's SVG renderer draws it into a picture on a
//! Thread (`jobs`). The window is the document shell: the thumbnails and the
//! outline in the navigation pane, the toolbar and the pages in the document
//! (a vertical VirtualView: only the pages in view exist), the search hits in
//! the side pane, the status bar under it. azul-appkit gives the switches
//! (`AzPdf [FILE.pdf]`, `--theme`, `--mode`, `--size`, `--shot`,
//! `--sample`, `--data-dir`), the settings page (Mod+,) and About.
//!
//! Data: a PDF stays where it is; the data tree holds the recent documents
//! (`pdf/recent.json`, with the page each was left at) and, with `--sample`,
//! the sample document (`pdf/samples/AzPdf sample.pdf`), written through
//! azul-storage's drive on a Thread.
//!
//! Without a window: `AzPdf --export-png OUT.png [--page N] [--width W]
//! FILE.pdf` draws one page through the viewer's own render path,
//! `--export-svg OUT.svg` writes the page's SVG (the Chrome probe,
//! `scripts/pdf_chrome_probe.py`).
//!
//! stdout, for scripts (`scripts/azpdf_e2e.py`): `AZPDF_OPENED <pages> <path>`,
//! `AZPDF_OPEN_ERROR <why>`, `AZPDF_RENDERED <page> <width>` (a page in the
//! view, 1-based), `AZPDF_PAGE <n>` when the current page changes,
//! `AZPDF_ZOOM <percent>`, `AZPDF_HITS <n>`, `AZPDF_RECENT_SAVED`.

use std::{
    ops::Range,
    path::{Path, PathBuf},
    time::Instant as StdInstant,
};

use azul::{
    dialog::{FileDialog, FileOpenResult},
    dom::{DomId, VirtualKeyCode},
    file::FileTypeList,
    image::ImageRef,
    option::OptionFileTypeList,
    prelude::*,
    str::String as AzString,
    time::SystemTimeDiff,
    vec::StringVec,
};
use azul_appkit::{
    about::AboutInfo,
    args::{AppArgs, AppSpec},
    data::{app_key, local_path},
    files::{FileJob, FileOutcome},
    find::TextMatch,
    shortcuts::Shortcut,
    ui as kit,
};

pub mod ids;
pub mod jobs;
pub mod model;
pub mod ui;

#[cfg(test)]
mod model_tests;

use jobs::{Doc, Done, Job, JobInit, Kind, Outcome};
use model::{
    plan_renders, render_width, search, ExportFormat, ExportRequest, Hit, PageCache, Recent,
    RecentDoc, Strip, Zoom,
};

/// What azul-appkit's switches know about AzPdf.
pub const SPEC: AppSpec = AppSpec {
    name: "AzPdf",
    binary: "AzPdf",
    summary: "a PDF viewer",
    screens: &["viewer"],
    files_help: "the PDF to open",
};

/// The About facts.
pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzPdf",
    version: env!("CARGO_PKG_VERSION"),
    summary: "A PDF viewer: page thumbnails, the outline, zoom and search. azul reads the PDF \
              and draws every page through its SVG renderer. The recent documents are kept in \
              your data folder.",
    license: "MIT",
    app_folder: "pdf",
};

/// The keys AzPdf answers (the kit adds Mod+, / F1 / Escape).
pub const SHORTCUTS: [Shortcut; 7] = [
    Shortcut::new("Document", "Mod+O", "Open a PDF"),
    Shortcut::new("View", "Mod+=", "Zoom in"),
    Shortcut::new("View", "Mod+-", "Zoom out"),
    Shortcut::new("View", "Mod+0", "Actual size (100 %)"),
    Shortcut::new("Pages", "Page Down / Page Up", "The next / previous page"),
    Shortcut::new("Pages", "Mod+Home / Mod+End", "The first / last page"),
    Shortcut::new(
        "Search",
        "Return in the search field",
        "Find in the document",
    ),
];

/// The recent documents' file in the app's folder.
pub const RECENT_FILE: &str = "recent.json";
/// The sample document's key in the app's folder.
pub const SAMPLE_FILE: &str = "samples/AzPdf sample.pdf";

/// How many render threads run at once, and pages per thread.
const MAX_RENDER_THREADS: usize = 2;
const RENDER_BATCH: usize = 2;
/// Rendered pages kept: the view's, and the thumbnails'.
const PAGE_CACHE: usize = 10;
const THUMB_CACHE: usize = 160;
/// The thumbnails' width, CSS px.
pub const THUMB_W: f32 = 112.0;
/// The pump: plans renders, scrolls, saves the recent list.
const PUMP_MS: u64 = 120;
/// The recent list is saved at most this often while pages turn.
const RECENT_SAVE_MS: u128 = 2_000;

/// The write-back tags of the kit's file jobs.
const TAG_RECENT_LOAD: u64 = 1;
const TAG_RECENT_SAVE: u64 = 2;
const TAG_SAMPLE_SAVE: u64 = 3;

/// The navigation pane's tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    Pages,
    Outline,
}

/// The search: the query, the pages' text (read once per document), the hits.
#[derive(Default)]
pub struct SearchState {
    pub query: String,
    pub texts: Option<Vec<String>>,
    pub running: bool,
    /// A search ran (the side pane shows its hits, none included).
    pub ran: bool,
    pub hits: Vec<Hit>,
}

pub struct AppState {
    pub kit: RefAny,
    pub data_root: PathBuf,
    pub doc: Option<Doc>,
    /// The path being opened (the parse runs).
    pub loading: Option<String>,
    /// Why the last open failed.
    pub error: Option<String>,
    /// Bumped per open: answers about an older document are dropped.
    pub generation: u64,
    /// The generation the page view last laid out (a scroll waits for it).
    pub view_generation: u64,
    pub zoom: Zoom,
    /// The page view's size in CSS px and the display's density.
    pub view: (f32, f32),
    pub dpi: f32,
    pub current_page: usize,
    /// The page the window shows as current (refresh when it changes).
    pub shown_page: usize,
    pub visible: Range<usize>,
    pub thumbs_visible: Range<usize>,
    pub pages: PageCache<ImageRef>,
    pub thumbs: PageCache<ImageRef>,
    /// Renders in flight: (kind, page, width).
    pub running: Vec<(Kind, usize, u32)>,
    pub render_threads: usize,
    pub nav: Nav,
    pub recent: Recent,
    pub recent_dirty: bool,
    pub saving_recent: bool,
    pub last_recent_save: StdInstant,
    pub search: SearchState,
    /// Scroll to this page once the view has laid out (after an open or a
    /// zoom change).
    pub pending_scroll: Option<usize>,
    pub status: String,
    pub pump: TimerId,
    /// From the command line: a PDF to open, the sample to make.
    pub open_on_start: Option<String>,
    pub sample: bool,
}

impl AppState {
    fn new(kit: RefAny, data_root: PathBuf, open_on_start: Option<String>, sample: bool) -> Self {
        AppState {
            kit,
            data_root,
            doc: None,
            loading: None,
            error: None,
            generation: 0,
            view_generation: 0,
            zoom: Zoom::FitWidth,
            view: (900.0, 700.0),
            dpi: 1.0,
            current_page: 0,
            shown_page: 0,
            visible: 0..0,
            thumbs_visible: 0..0,
            pages: PageCache::new(PAGE_CACHE),
            thumbs: PageCache::new(THUMB_CACHE),
            running: Vec::new(),
            render_threads: 0,
            nav: Nav::Pages,
            recent: Recent::default(),
            recent_dirty: false,
            saving_recent: false,
            last_recent_save: StdInstant::now(),
            search: SearchState::default(),
            pending_scroll: None,
            status: String::new(),
            pump: TimerId::unique(),
            open_on_start,
            sample,
        }
    }

    /// The scale the pages are shown at.
    #[must_use]
    pub fn scale(&self) -> f32 {
        let sizes = self.doc.as_ref().map_or(&[][..], |d| d.sizes.as_slice());
        self.zoom.scale(self.view.0, self.view.1, sizes)
    }

    /// The strip of pages at the current scale.
    #[must_use]
    pub fn strip(&self) -> Strip {
        let sizes = self.doc.as_ref().map_or(&[][..], |d| d.sizes.as_slice());
        Strip::new(sizes, self.scale())
    }

    /// The thumbnails' scale: the widest page is [`THUMB_W`] wide.
    #[must_use]
    pub fn thumb_scale(&self) -> f32 {
        let widest = self.doc.as_ref().map_or(0.0, |d| {
            d.sizes.iter().map(|p| p.css(1.0).0).fold(0.0_f32, f32::max)
        });
        if widest > 0.0 {
            THUMB_W / widest
        } else {
            1.0
        }
    }

    /// The width page `page` is rendered at in the view.
    #[must_use]
    pub fn page_render_width(&self, page: usize) -> u32 {
        let scale = self.scale();
        let css = self
            .doc
            .as_ref()
            .and_then(|d| d.sizes.get(page))
            .map_or(0.0, |p| p.css(scale).0);
        render_width(css, self.dpi)
    }

    /// The width page `page` is rendered at in the thumbnail rail.
    #[must_use]
    pub fn thumb_render_width(&self, page: usize) -> u32 {
        let scale = self.thumb_scale();
        let css = self
            .doc
            .as_ref()
            .and_then(|d| d.sizes.get(page))
            .map_or(0.0, |p| p.css(scale).0);
        render_width(css, self.dpi)
    }

    fn running_of(&self, kind: Kind) -> Vec<(usize, u32)> {
        self.running
            .iter()
            .filter(|(k, _, _)| *k == kind)
            .map(|(_, p, w)| (*p, *w))
            .collect()
    }

    /// The next pages to render: the pages in view, one either side, then
    /// the thumbnails in view - minus what is drawn or being drawn.
    fn next_batch(&self) -> Vec<(Kind, usize, u32)> {
        let Some(doc) = self.doc.as_ref() else {
            return Vec::new();
        };
        let n = doc.page_count();
        if n == 0 {
            return Vec::new();
        }
        let mut wanted: Vec<usize> = self.visible.clone().filter(|p| *p < n).collect();
        if wanted.is_empty() {
            wanted.push(self.current_page.min(n - 1));
        }
        let first = wanted[0];
        let last = wanted[wanted.len() - 1];
        if first > 0 {
            wanted.push(first - 1);
        }
        if last + 1 < n {
            wanted.push(last + 1);
        }
        let mut out = Vec::new();
        let running = self.running_of(Kind::Page);
        for page in wanted {
            let width = self.page_render_width(page);
            let planned = plan_renders(&[page], width, |p| self.pages.has(p, width), &running, 1);
            out.extend(planned.into_iter().map(|(p, w)| (Kind::Page, p, w)));
            if out.len() >= RENDER_BATCH {
                return out;
            }
        }
        if self.nav == Nav::Pages {
            let running = self.running_of(Kind::Thumb);
            for page in self.thumbs_visible.clone().filter(|p| *p < n) {
                let width = self.thumb_render_width(page);
                let planned =
                    plan_renders(&[page], width, |p| self.thumbs.has(p, width), &running, 1);
                out.extend(planned.into_iter().map(|(p, w)| (Kind::Thumb, p, w)));
                if out.len() >= RENDER_BATCH {
                    return out;
                }
            }
        }
        out
    }

    /// The render threads to start now (at most [`MAX_RENDER_THREADS`] run).
    fn plan_jobs(&mut self) -> Vec<Job> {
        let mut jobs = Vec::new();
        while self.render_threads < MAX_RENDER_THREADS {
            let Some(pdf) = self.doc.as_ref().map(|d| d.pdf.clone()) else {
                break;
            };
            let batch = self.next_batch();
            if batch.is_empty() {
                break;
            }
            self.running.extend(batch.iter().copied());
            self.render_threads += 1;
            jobs.push(Job::Render {
                generation: self.generation,
                pdf,
                pages: batch,
            });
        }
        jobs
    }

    /// The recent entry of the open document, at its current page.
    fn recent_entry(&self) -> Option<RecentDoc> {
        let doc = self.doc.as_ref()?;
        Some(RecentDoc {
            path: doc.path.clone(),
            title: doc.title.clone(),
            pages: doc.page_count(),
            last_page: self.current_page,
            opened: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
        })
    }

    /// Runs the search over the pages' texts it has.
    fn run_search(&mut self) {
        let texts = self.search.texts.as_deref().unwrap_or(&[]);
        self.search.hits = search(texts, self.search.query.trim(), TextMatch::default());
        self.search.ran = true;
        self.search.running = false;
        println!("AZPDF_HITS {}", self.search.hits.len());
        self.status = match self.search.hits.len() {
            0 => format!(
                "No matches for \u{201c}{}\u{201d}",
                self.search.query.trim()
            ),
            1 => "1 match".to_string(),
            n => format!("{n} matches"),
        };
    }
}

// ==== Start ====

pub fn run() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if let Some(request) = model::parse_export(&argv) {
        let code = match request.and_then(|r| export(&r)) {
            Ok(done) => {
                println!("{done}");
                0
            }
            Err(why) => {
                eprintln!("AzPdf: {why}");
                2
            }
        };
        std::process::exit(code);
    }
    let args = match AppArgs::from_env(&SPEC) {
        Ok(a) => a,
        Err(why) => {
            eprintln!("{why}");
            std::process::exit(2);
        }
    };
    let open_on_start = args.files.first().map(|p| p.display().to_string());
    let sample = args.sample;
    let kit_ref = kit::create_kit(SPEC, ABOUT, &SHORTCUTS, &[], args);
    let data_root = {
        let mut k = kit_ref.clone();
        k.downcast_ref::<kit::Kit>()
            .map(|k| k.data_root.clone())
            .unwrap_or_default()
    };
    let state = AppState::new(kit_ref.clone(), data_root, open_on_start, sample);
    let app = App::create(RefAny::new(state), kit::app_config(&kit_ref));
    let window = kit::window_options(
        &kit_ref,
        ui::layout,
        (1280.0, 860.0),
        (720.0, 480.0),
        on_window_created,
    );
    app.run(window);
}

/// `--export-png` / `--export-svg`: one page written without a window,
/// through the viewer's own path.
fn export(request: &ExportRequest) -> Result<String, String> {
    let doc = jobs::open(&request.file)?;
    if request.page >= doc.page_count() {
        return Err(format!(
            "{} has {} pages, there is no page {}",
            request.file,
            doc.page_count(),
            request.page + 1
        ));
    }
    let bytes = match request.format {
        ExportFormat::Svg => jobs::page_svg(&doc.pdf, request.page)
            .ok_or_else(|| "the page has no SVG".to_string())?
            .into_bytes(),
        ExportFormat::Png => {
            let raw = jobs::render_page_raw(&doc.pdf, request.page, request.width)
                .ok_or_else(|| "the page could not be drawn".to_string())?;
            match raw.encode_png().into_result() {
                Ok(png) => png.as_ref().to_vec(),
                Err(_) => return Err("the page could not be encoded as PNG".to_string()),
            }
        }
    };
    std::fs::write(&request.out, &bytes)
        .map_err(|e| format!("{} could not be written: {e}", request.out))?;
    Ok(format!(
        "AZPDF_EXPORTED {} {} {}",
        request.page + 1,
        doc.page_count(),
        request.out
    ))
}

/// The kit's handle, out of the app's state.
fn kit_of(data: &mut RefAny) -> Option<RefAny> {
    data.downcast_ref::<AppState>().map(|s| s.kit.clone())
}

/// The window exists: the kit's `--shot` timer, the recent list, the pump,
/// then the document from the command line (or the sample).
extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    if let Some(kit_ref) = kit_of(&mut data) {
        kit::on_window_created(&kit_ref, &mut info);
    }
    let handle = data.clone();
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    kit::spawn_file_jobs(
        &mut info,
        &s.data_root,
        vec![FileJob::Get {
            key: app_key(ABOUT.app_folder, RECENT_FILE),
        }],
        handle.clone(),
        TAG_RECENT_LOAD,
        on_file_done,
    );
    let pump = Timer::create(handle.clone(), on_pump, info.get_system_time_fn())
        .with_interval(Duration::System(SystemTimeDiff::from_millis(PUMP_MS)));
    info.add_timer(s.pump, pump);
    if let Some(path) = s.open_on_start.take() {
        start_open(&mut info, &handle, &mut s, path);
    } else if s.sample {
        s.status = "Making the sample document\u{2026}".to_string();
        spawn(&mut info, &handle, Job::Sample);
    }
    Update::RefreshDom
}

/// Starts `job` on an azul Thread; its answer comes to [`on_job_done`].
fn spawn(info: &mut CallbackInfo, app: &RefAny, job: Job) {
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(JobInit { job: Some(job) }),
            app.clone(),
            jobs::job_thread,
        ),
    );
}

/// Opens the PDF at `path` (read and parsed on a Thread).
pub(crate) fn start_open(info: &mut CallbackInfo, app: &RefAny, s: &mut AppState, path: String) {
    s.generation += 1;
    s.loading = Some(path.clone());
    s.error = None;
    s.running.clear();
    s.status = format!("Opening {}\u{2026}", model::file_title(&path));
    spawn(
        info,
        app,
        Job::Open {
            generation: s.generation,
            path,
        },
    );
}

/// The document `doc` is open: the view starts at the page it was left at.
fn show(s: &mut AppState, doc: Doc) {
    let last = s
        .recent
        .get(&doc.path)
        .map_or(0, |r| r.last_page)
        .min(doc.page_count().saturating_sub(1));
    println!("AZPDF_OPENED {} {}", doc.page_count(), doc.path);
    s.status = if doc.warnings > 0 {
        format!(
            "{} pages \u{2014} {} parts of the file were skipped",
            doc.page_count(),
            doc.warnings
        )
    } else {
        format!("{} pages", doc.page_count())
    };
    s.pages.clear();
    s.thumbs.clear();
    s.running.clear();
    s.search = SearchState::default();
    s.doc = Some(doc);
    s.current_page = last;
    s.shown_page = last;
    s.visible = last..last + 1;
    s.thumbs_visible = 0..0;
    s.pending_scroll = Some(last);
    if let Some(entry) = s.recent_entry() {
        s.recent.touch(entry);
        s.recent_dirty = true;
    }
}

/// Writes `pdf/recent.json` on a Thread (one write at a time).
fn save_recent(info: &mut CallbackInfo, app: &RefAny, s: &mut AppState) {
    if s.saving_recent {
        s.recent_dirty = true;
        return;
    }
    s.saving_recent = true;
    s.recent_dirty = false;
    s.last_recent_save = StdInstant::now();
    kit::spawn_file_jobs(
        info,
        &s.data_root,
        vec![FileJob::Put {
            key: app_key(ABOUT.app_folder, RECENT_FILE),
            bytes: s.recent.to_json().into_bytes(),
        }],
        app.clone(),
        TAG_RECENT_SAVE,
        on_file_done,
    );
}

// ==== The pump: renders, scrolls, saves ====

/// Every [`PUMP_MS`]: the pending scroll (once the view has laid out the
/// document), new render threads for the pages in view, the recent list.
extern "C" fn on_pump(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let handle = data.clone();
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return TimerCallbackReturn::terminate_unchanged();
    };
    let mut refresh = false;
    if s.doc.is_some() && s.view_generation == s.generation {
        if let Some(page) = s.pending_scroll.take() {
            let y = s.strip().top_of(page);
            scroll_pages_to(&mut info.callback_info, y);
            s.current_page = page;
        }
    }
    for job in s.plan_jobs() {
        spawn(&mut info.callback_info, &handle, job);
    }
    if s.current_page != s.shown_page {
        s.shown_page = s.current_page;
        println!("AZPDF_PAGE {}", s.current_page + 1);
        if let Some(path) = s.doc.as_ref().map(|d| d.path.clone()) {
            let page = s.current_page;
            s.recent.set_page(&path, page);
            s.recent_dirty = true;
        }
        refresh = true;
    }
    if s.recent_dirty
        && !s.saving_recent
        && s.last_recent_save.elapsed().as_millis() >= RECENT_SAVE_MS
    {
        save_recent(&mut info.callback_info, &handle, &mut s);
    }
    if refresh {
        TimerCallbackReturn::continue_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_unchanged()
    }
}

/// Scrolls the page view to offset `y` (CSS px).
pub(crate) fn scroll_pages_to(info: &mut CallbackInfo, y: f32) {
    let dom = DomId { inner: 0 };
    let node = info.get_node_id_by_id_attribute(dom, ids::PAGES_NAME);
    info.scroll_to(dom, node, LogicalPosition::create(0.0, y.max(0.0)));
}

/// Shows page `page` (0-based): scrolls there now.
pub(crate) fn jump_to(info: &mut CallbackInfo, s: &mut AppState, page: usize) {
    let Some(count) = s.doc.as_ref().map(Doc::page_count) else {
        return;
    };
    if count == 0 {
        return;
    }
    let page = page.min(count - 1);
    let y = s.strip().top_of(page);
    scroll_pages_to(info, y);
    s.current_page = page;
}

/// A new zoom: the view keeps the current page.
pub(crate) fn set_zoom(s: &mut AppState, zoom: Zoom) {
    s.zoom = zoom;
    s.pending_scroll = Some(s.current_page);
    let percent = (s.scale() * 100.0).round() as u32;
    println!("AZPDF_ZOOM {percent}");
}

// ==== Answers from the threads ====

/// A job's answer arrived (the write-back of [`jobs::job_thread`]).
pub extern "C" fn on_job_done(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(outcome) = msg
        .downcast_mut::<Done>()
        .and_then(|mut done| done.outcome.take())
    else {
        return Update::DoNothing;
    };
    let handle = app.clone();
    let Some(mut s) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    match outcome {
        Outcome::Opened {
            generation,
            path,
            result,
        } => {
            if generation != s.generation {
                return Update::DoNothing;
            }
            s.loading = None;
            match result {
                Ok(doc) => {
                    show(&mut s, doc);
                    save_recent(&mut info, &handle, &mut s);
                }
                Err(why) => {
                    println!("AZPDF_OPEN_ERROR {why}");
                    s.status = format!("{} could not be opened", model::file_title(&path));
                    s.error = Some(why);
                }
            }
            Update::RefreshDom
        }
        Outcome::Rendered {
            generation,
            kind,
            page,
            width,
            image,
        } => {
            if generation != s.generation {
                return Update::DoNothing;
            }
            s.running.retain(|r| *r != (kind, page, width));
            let Some(image) = image else {
                return Update::DoNothing;
            };
            match kind {
                Kind::Page => {
                    println!("AZPDF_RENDERED {} {width}", page + 1);
                    s.pages.insert(page, width, image);
                    if s.visible.contains(&page) {
                        Update::RefreshDom
                    } else {
                        Update::DoNothing
                    }
                }
                Kind::Thumb => {
                    s.thumbs.insert(page, width, image);
                    if s.thumbs_visible.contains(&page) {
                        Update::RefreshDom
                    } else {
                        Update::DoNothing
                    }
                }
            }
        }
        Outcome::RenderDone { .. } => {
            s.render_threads = s.render_threads.saturating_sub(1);
            Update::DoNothing
        }
        Outcome::Texts { generation, texts } => {
            if generation != s.generation {
                return Update::DoNothing;
            }
            s.search.texts = Some(texts);
            s.run_search();
            Update::RefreshDom
        }
        Outcome::Sample { result } => match result {
            Ok(bytes) => {
                let root = s.data_root.clone();
                kit::spawn_file_jobs(
                    &mut info,
                    &root,
                    vec![FileJob::Put {
                        key: app_key(ABOUT.app_folder, SAMPLE_FILE),
                        bytes,
                    }],
                    handle.clone(),
                    TAG_SAMPLE_SAVE,
                    on_file_done,
                );
                Update::DoNothing
            }
            Err(why) => {
                s.status = format!("The sample could not be made: {why}");
                Update::RefreshDom
            }
        },
    }
}

/// The kit's file jobs answered: the recent list read or written, the
/// sample written (then it opens).
extern "C" fn on_file_done(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(reply) = kit::take_reply(&mut msg) else {
        return Update::DoNothing;
    };
    let handle = app.clone();
    let Some(mut s) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    match reply.tag {
        TAG_RECENT_LOAD => {
            for outcome in reply.outcomes {
                if let FileOutcome::Got {
                    result: Ok(Some(bytes)),
                    ..
                } = outcome
                {
                    let mut loaded = Recent::parse(&String::from_utf8_lossy(&bytes));
                    // A document opened before the list arrived stays first.
                    if let Some(entry) = s.recent_entry() {
                        loaded.touch(entry);
                    }
                    s.recent = loaded;
                }
            }
            Update::RefreshDom
        }
        TAG_RECENT_SAVE => {
            s.saving_recent = false;
            match reply.outcomes.iter().find_map(FileOutcome::error) {
                Some(why) => s.status = format!("The recent list could not be saved: {why}"),
                None => println!("AZPDF_RECENT_SAVED"),
            }
            Update::DoNothing
        }
        TAG_SAMPLE_SAVE => {
            let key = app_key(ABOUT.app_folder, SAMPLE_FILE);
            match reply.outcomes.iter().find_map(FileOutcome::error) {
                Some(why) => {
                    s.status = format!("The sample could not be saved: {why}");
                }
                None => {
                    let path = local_path(&s.data_root, &key).display().to_string();
                    start_open(&mut info, &handle, &mut s, path);
                }
            }
            Update::RefreshDom
        }
        _ => Update::DoNothing,
    }
}

// ==== Opening ====

/// The toolbar's / the start screen's Open: the file dialog.
pub extern "C" fn on_open(data: RefAny, _info: CallbackInfo) -> Update {
    let filter = FileTypeList {
        document_types: StringVec::from_vec(vec![AzString::from("*.pdf")]),
        document_descriptor: AzString::from("PDF documents"),
    };
    let _request = FileDialog::open_file(
        "Open a PDF",
        OptionString::None,
        OptionFileTypeList::Some(filter),
        data,
        on_open_picked,
    );
    Update::DoNothing
}

/// The file dialog answered.
extern "C" fn on_open_picked(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing; // cancelled
    };
    let path = path.as_string().as_str().to_string();
    let handle = data.clone();
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    start_open(&mut info, &handle, &mut s, path);
    Update::RefreshDom
}

/// Files dropped on the window: the first PDF opens.
pub extern "C" fn on_dropped(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let dropped: Option<String> = info
        .get_dropped_files()
        .as_slice()
        .iter()
        .map(|f| f.as_str().to_string())
        .find(|p| model::is_pdf_path(p));
    let Some(path) = dropped else {
        return Update::DoNothing;
    };
    let handle = data.clone();
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    start_open(&mut info, &handle, &mut s, path);
    Update::RefreshDom
}

/// A recent document on the start screen.
pub extern "C" fn on_recent(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(index) = ui::index_of(&mut info) else {
        return Update::DoNothing;
    };
    let handle = data.clone();
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let Some(path) = s.recent.docs.get(index).map(|d| d.path.clone()) else {
        return Update::DoNothing;
    };
    if !Path::new(&path).exists() {
        s.status = format!("{} is no longer there", model::file_title(&path));
        return Update::RefreshDom;
    }
    start_open(&mut info, &handle, &mut s, path);
    Update::RefreshDom
}

// ==== Pages and zoom ====

/// A thumbnail, an outline entry or a search hit: its page.
pub extern "C" fn on_go_to_page(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(page) = ui::index_of(&mut info) else {
        return Update::DoNothing;
    };
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    jump_to(&mut info, &mut s, page);
    Update::RefreshDom
}

pub extern "C" fn on_prev(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let page = s.current_page.saturating_sub(1);
    jump_to(&mut info, &mut s, page);
    Update::RefreshDom
}

pub extern "C" fn on_next(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let page = s.current_page + 1;
    jump_to(&mut info, &mut s, page);
    Update::RefreshDom
}

pub extern "C" fn on_zoom_in(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let zoom = Zoom::zoom_in(s.scale());
    set_zoom(&mut s, zoom);
    Update::RefreshDom
}

pub extern "C" fn on_zoom_out(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let zoom = Zoom::zoom_out(s.scale());
    set_zoom(&mut s, zoom);
    Update::RefreshDom
}

/// The zoom drop-down: [`Zoom::choices`]`[index]`.
pub extern "C" fn on_zoom_choice(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
    let Some(zoom) = Zoom::choices().get(index).copied() else {
        return Update::DoNothing;
    };
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    set_zoom(&mut s, zoom);
    Update::RefreshDom
}

/// The navigation pane's Pages / Outline switch.
pub extern "C" fn on_nav_tab(
    mut data: RefAny,
    _info: CallbackInfo,
    state: azul::widgets::SegmentedState,
) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.nav = if state.selected_index == 1 {
        Nav::Outline
    } else {
        Nav::Pages
    };
    Update::RefreshDom
}

/// Return in the page field: that page.
pub extern "C" fn on_page_field_key(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: azul::widgets::TextInputState,
) -> azul::widgets::OnTextInputReturn {
    let mut update = Update::DoNothing;
    if is_return(&info) {
        let text = state.get_text().as_str().to_string();
        if let Some(mut s) = data.downcast_mut::<AppState>() {
            let count = s.doc.as_ref().map_or(0, Doc::page_count);
            if let Some(page) = model::parse_page_field(&text, count) {
                info.stop_propagation();
                jump_to(&mut info, &mut s, page);
                update = Update::RefreshDom;
            }
        }
    }
    azul::widgets::OnTextInputReturn {
        update,
        valid: azul::widgets::TextInputValid::Yes,
    }
}

/// Return in the search field: find in the document (the pages' text is
/// read on a Thread the first time).
pub extern "C" fn on_search_key(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: azul::widgets::TextInputState,
) -> azul::widgets::OnTextInputReturn {
    let mut update = Update::DoNothing;
    if is_return(&info) {
        info.stop_propagation();
        let query = state.get_text().as_str().to_string();
        let handle = data.clone();
        if let Some(mut s) = data.downcast_mut::<AppState>() {
            update = start_search(&mut info, &handle, &mut s, query);
        }
    }
    azul::widgets::OnTextInputReturn {
        update,
        valid: azul::widgets::TextInputValid::Yes,
    }
}

fn is_return(info: &CallbackInfo) -> bool {
    matches!(
        info.get_current_keyboard_state()
            .current_virtual_keycode
            .into_option(),
        Some(VirtualKeyCode::Return) | Some(VirtualKeyCode::NumpadEnter)
    )
}

/// Searches for `query`: now if the pages' text is read, else after it is.
fn start_search(info: &mut CallbackInfo, app: &RefAny, s: &mut AppState, query: String) -> Update {
    s.search.query = query;
    if s.search.query.trim().is_empty() {
        s.search = SearchState {
            texts: s.search.texts.take(),
            ..SearchState::default()
        };
        return Update::RefreshDom;
    }
    if s.search.texts.is_some() {
        s.run_search();
        return Update::RefreshDom;
    }
    let Some(pdf) = s.doc.as_ref().map(|d| d.pdf.clone()) else {
        return Update::DoNothing;
    };
    if !s.search.running {
        s.search.running = true;
        s.status = "Reading the pages\u{2019} text\u{2026}".to_string();
        spawn(
            info,
            app,
            Job::Texts {
                generation: s.generation,
                pdf,
            },
        );
    }
    Update::RefreshDom
}

/// The hits pane's close button.
pub extern "C" fn on_search_close(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.search = SearchState {
        texts: s.search.texts.take(),
        ..SearchState::default()
    };
    Update::RefreshDom
}

/// The toolbar's gear: azul-appkit's settings page.
pub extern "C" fn on_settings_open(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(kit_ref) = kit_of(&mut data) {
        kit::open_settings(&kit_ref, None);
    }
    Update::RefreshDom
}

// ==== Keys ====

/// The kit's keys first (Mod+, settings, F1 shortcuts, Escape); then Mod+O,
/// the zoom keys and the page keys.
pub extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(kit_ref) = kit_of(&mut data) else {
        return Update::DoNothing;
    };
    if let Some(update) = kit::handle_key(&kit_ref, &mut info) {
        return update;
    }
    if kit::settings_open(&kit_ref) {
        return Update::DoNothing;
    }
    let Some(key) = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option()
    else {
        return Update::DoNothing;
    };
    let primary = info.get_key_modifiers().primary_down();
    if primary && matches!(key, VirtualKeyCode::O) {
        info.prevent_default();
        return on_open(data, info);
    }
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    if s.doc.is_none() {
        return Update::DoNothing;
    }
    let page = s.current_page;
    let last = s
        .doc
        .as_ref()
        .map_or(0, |d| d.page_count().saturating_sub(1));
    match key {
        VirtualKeyCode::Equals | VirtualKeyCode::Plus | VirtualKeyCode::NumpadAdd if primary => {
            let zoom = Zoom::zoom_in(s.scale());
            set_zoom(&mut s, zoom);
        }
        VirtualKeyCode::Minus | VirtualKeyCode::NumpadSubtract if primary => {
            let zoom = Zoom::zoom_out(s.scale());
            set_zoom(&mut s, zoom);
        }
        VirtualKeyCode::Key0 if primary => set_zoom(&mut s, Zoom::Percent(100)),
        VirtualKeyCode::PageDown => jump_to(&mut info, &mut s, page + 1),
        VirtualKeyCode::PageUp => jump_to(&mut info, &mut s, page.saturating_sub(1)),
        VirtualKeyCode::Home if primary => jump_to(&mut info, &mut s, 0),
        VirtualKeyCode::End if primary => jump_to(&mut info, &mut s, last),
        _ => return Update::DoNothing,
    }
    info.prevent_default();
    Update::RefreshDom
}
